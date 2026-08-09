#if !defined(_WIN32)
#error "platform_windows.c must be compiled on Windows"
#endif

#include <assert.h>
#include <stdio.h>
#include <string.h>

#include "../src/platform/platform.h"

typedef struct
{
  AuraPlatformMutex *mutex;
  AuraPlatformCond *condition;
  volatile LONG *ready;
} ThreadState;

static void *platform_thread(void *raw)
{
  ThreadState *state = (ThreadState *)raw;
  aura_platform_mutex_lock(state->mutex);
  InterlockedExchange(state->ready, 1);
  aura_platform_cond_signal(state->condition);
  aura_platform_mutex_unlock(state->mutex);
  return NULL;
}

int main(void)
{
  AuraPlatformMutex mutex;
  AuraPlatformCond condition;
  AuraPlatformThread thread;
  volatile LONG ready = 0;
  ThreadState state = {&mutex, &condition, &ready};
  assert(aura_platform_mutex_init(&mutex, 1) == 0);
  assert(aura_platform_cond_init(&condition) == 0);
  assert(aura_platform_thread_create(&thread, platform_thread, &state) == 0);
  aura_platform_mutex_lock(&mutex);
  while (InterlockedCompareExchange(&ready, 0, 0) == 0)
    aura_platform_cond_wait(&condition, &mutex);
  aura_platform_mutex_unlock(&mutex);
  assert(aura_platform_thread_join(&thread) == 0);

  char path[MAX_PATH];
  char temp_dir[MAX_PATH];
  DWORD length = GetTempPathA((DWORD)sizeof(temp_dir), temp_dir);
  assert(length > 0 && length < sizeof(temp_dir));
  assert(GetTempFileNameA(temp_dir, "aur", 0, path) != 0);
  AuraPlatformFile file = aura_platform_file_open(path, 1);
  assert(file != AURA_PLATFORM_FILE_INVALID);
  assert(aura_platform_file_write(file, "platform", 8) == 8);
  assert(aura_platform_file_flush(file) == 0);
  assert(aura_platform_file_close(file) == 0);
  assert(DeleteFileA(path) != 0);

  AuraPlatformWake wake;
  assert(aura_platform_wake_init(&wake) == 0);
  assert(aura_platform_wake_signal(&wake) == 0);
  assert(aura_platform_wake_drain(&wake) == 0);
  aura_platform_wake_destroy(&wake);

  assert(aura_platform_random_bytes(path, 1) != 0);
  assert(aura_platform_monotonic_millis() > 0);
  AuraPlatformSocket socket_handle = aura_platform_socket_open(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
  assert(!AURA_PLATFORM_SOCKET_IS_INVALID(socket_handle));
  assert(aura_platform_socket_close(socket_handle) == 0);
  aura_platform_socket_shutdown();
  aura_platform_cond_destroy(&condition);
  aura_platform_mutex_destroy(&mutex);
  puts("platform Windows: passed");
  return 0;
}
