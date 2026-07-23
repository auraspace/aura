#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

static void make_path(char *path, size_t capacity)
{
  int fd;
  snprintf(path, capacity, "/tmp/aura-ffi-io-XXXXXX");
  fd = mkstemp(path);
  assert(fd >= 0);
  assert(close(fd) == 0);
}

static void test_file_status_and_mode_boundaries(void)
{
  char path[64];
  AuraFile *file = NULL;
  uint64_t count = 99;
  char buffer[8] = {0};

  make_path(path, sizeof(path));
  assert(aura_file_open(NULL, AURA_FILE_READ, &file) == AURA_FILE_ERROR);
  assert(file == NULL);
  assert(aura_file_open(path, (AuraFileMode)99, &file) == AURA_FILE_ERROR);
  assert(aura_file_open(path, AURA_FILE_READ, NULL) == AURA_FILE_ERROR);

  assert(aura_file_open(path, AURA_FILE_WRITE, &file) == AURA_FILE_OK);
  assert(aura_file_write(file, "ffi", 3, &count) == AURA_FILE_OK);
  assert(count == 3);
  assert(aura_file_write(file, NULL, 1, &count) == AURA_FILE_ERROR);
  assert(count == 0);
  assert(aura_file_flush(file) == AURA_FILE_OK);
  assert(aura_file_destroy(&file) == AURA_FILE_OK);
  assert(file == NULL);

  assert(aura_file_open(path, AURA_FILE_READ_WRITE, &file) == AURA_FILE_OK);
  assert(aura_file_read(file, buffer, sizeof(buffer), NULL) == AURA_FILE_ERROR);
  assert(aura_file_read(file, NULL, 1, &count) == AURA_FILE_ERROR);
  assert(aura_file_close(file) == AURA_FILE_OK);
  assert(aura_file_read(file, buffer, sizeof(buffer), &count) == AURA_FILE_CLOSED);
  assert(aura_file_destroy(&file) == AURA_FILE_OK);

  assert(unlink(path) == 0);
}

int main(void)
{
  test_file_status_and_mode_boundaries();
  puts("ffi/io boundary coverage: passed");
  return 0;
}
