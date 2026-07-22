#define _POSIX_C_SOURCE 200809L
#include <assert.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_ffi.h"
#include "../aura_rt.c"

static void make_path(char *path, size_t size)
{
  snprintf(path, size, "/tmp/aura-file-io-XXXXXX");
  int fd = mkstemp(path);
  assert(fd >= 0);
  assert(close(fd) == 0);
}

int main(void)
{
  char path[64];
  make_path(path, sizeof(path));

  AuraFile *file = NULL;
  assert(aura_file_open(path, AURA_FILE_WRITE, &file) == AURA_FILE_OK);
  const char payload[] = "alpha\nbeta";
  uint64_t count = 0;
  assert(aura_file_write(file, payload, sizeof(payload) - 1, &count) == AURA_FILE_OK);
  assert(count == sizeof(payload) - 1);
  assert(aura_file_flush(file) == AURA_FILE_OK);
  assert(aura_file_close(file) == AURA_FILE_OK);
  assert(aura_file_close(file) == AURA_FILE_CLOSED);
  char closed_buffer[1];
  assert(aura_file_read(file, closed_buffer, 1, &count) == AURA_FILE_CLOSED);
  assert(aura_file_destroy(&file) == AURA_FILE_OK);
  assert(file == NULL);

  assert(aura_file_open(path, AURA_FILE_READ, &file) == AURA_FILE_OK);
  char buffer[32] = {0};
  assert(aura_file_read(file, buffer, sizeof(buffer), &count) == AURA_FILE_OK);
  assert(count == sizeof(payload) - 1);
  assert(memcmp(buffer, payload, count) == 0);
  assert(aura_file_read(file, buffer, sizeof(buffer), &count) == AURA_FILE_EOF);
  assert(count == 0);
  assert(aura_file_close(file) == AURA_FILE_OK);
  assert(aura_file_destroy(&file) == AURA_FILE_OK);

  assert(aura_file_open(path, AURA_FILE_APPEND, &file) == AURA_FILE_OK);
  assert(aura_file_write(file, "!", 1, &count) == AURA_FILE_OK);
  assert(count == 1);
  assert(aura_file_destroy(&file) == AURA_FILE_OK);

  assert(aura_file_open("/definitely/missing/aura-file", AURA_FILE_READ, &file) ==
         AURA_FILE_ERROR);
  assert(strstr(aura_file_last_error(), "open") != NULL);
  assert(aura_file_open(path, (AuraFileMode)99, &file) == AURA_FILE_ERROR);
  assert(aura_file_destroy(NULL) == AURA_FILE_CLOSED);

  assert(unlink(path) == 0);
  puts("file I/O bounded semantics: passed");
  return 0;
}
