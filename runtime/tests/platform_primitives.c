#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <stdatomic.h>
#include <stdio.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct
{
  AuraPlatformMutex *mutex;
  AuraPlatformCond *condition;
  atomic_int *ready;
} PlatformThreadState;

static void *platform_thread(void *raw)
{
  PlatformThreadState *state = (PlatformThreadState *)raw;
  aura_platform_mutex_lock(state->mutex);
  atomic_store(state->ready, 1);
  aura_platform_cond_signal(state->condition);
  aura_platform_mutex_unlock(state->mutex);
  return NULL;
}

int main(void)
{
  AuraPlatformMutex mutex;
  AuraPlatformCond condition;
  AuraPlatformThread thread;
  atomic_int ready = 0;
  PlatformThreadState state = {&mutex, &condition, &ready};
  assert(aura_platform_mutex_init(&mutex, 1) == 0);
  assert(aura_platform_cond_init(&condition) == 0);
  assert(aura_platform_thread_create(&thread, platform_thread, &state) == 0);
  aura_platform_mutex_lock(&mutex);
  while (atomic_load(&ready) == 0) aura_platform_cond_wait(&condition, &mutex);
  aura_platform_mutex_unlock(&mutex);
  assert(aura_platform_thread_join(&thread) == 0);

  char path[64];
  snprintf(path, sizeof(path), "/tmp/aura-platform-XXXXXX");
  int temporary = mkstemp(path);
  assert(temporary >= 0);
  assert(close(temporary) == 0);
  AuraPlatformFile file = aura_platform_file_open(path, 1);
  assert(file != AURA_PLATFORM_FILE_INVALID);
  assert(aura_platform_file_write(file, "platform", 8) == 8);
  assert(aura_platform_file_flush(file) == 0);
  assert(aura_platform_file_close(file) == 0);
  assert(unlink(path) == 0);
  aura_platform_cond_destroy(&condition);
  aura_platform_mutex_destroy(&mutex);
  puts("platform primitives: passed");
  return 0;
}
