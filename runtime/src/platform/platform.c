#include "platform.h"

#include <stdlib.h>

#if defined(_WIN32)

#include <errno.h>
#include <io.h>
#include <process.h>
#include <signal.h>

typedef struct
{
  AuraPlatformThreadFn function;
  void *context;
} AuraPlatformThreadStart;

static unsigned __stdcall aura_platform_thread_start(void *raw)
{
  AuraPlatformThreadStart *start = (AuraPlatformThreadStart *)raw;
  AuraPlatformThreadFn function = start->function;
  void *context = start->context;
  free(start);
  (void)function(context);
  return 0;
}

int aura_platform_mutex_init(AuraPlatformMutex *mutex, int recursive)
{
  (void)recursive;
  if (mutex == NULL) return EINVAL;
  InitializeCriticalSection(&mutex->native);
  return 0;
}

void aura_platform_mutex_destroy(AuraPlatformMutex *mutex)
{
  if (mutex != NULL) DeleteCriticalSection(&mutex->native);
}

void aura_platform_mutex_lock(AuraPlatformMutex *mutex)
{
  EnterCriticalSection(&mutex->native);
}

void aura_platform_mutex_unlock(AuraPlatformMutex *mutex)
{
  LeaveCriticalSection(&mutex->native);
}

int aura_platform_cond_init(AuraPlatformCond *condition)
{
  if (condition == NULL) return EINVAL;
  InitializeConditionVariable(&condition->native);
  return 0;
}

void aura_platform_cond_destroy(AuraPlatformCond *condition) { (void)condition; }

void aura_platform_cond_wait(AuraPlatformCond *condition, AuraPlatformMutex *mutex)
{
  (void)SleepConditionVariableCS(&condition->native, &mutex->native, INFINITE);
}

void aura_platform_cond_signal(AuraPlatformCond *condition)
{
  WakeConditionVariable(&condition->native);
}

void aura_platform_cond_broadcast(AuraPlatformCond *condition)
{
  WakeAllConditionVariable(&condition->native);
}

int aura_platform_thread_create(AuraPlatformThread *thread,
                                AuraPlatformThreadFn function, void *context)
{
  AuraPlatformThreadStart *start;
  uintptr_t handle;
  if (thread == NULL || function == NULL) return EINVAL;
  start = (AuraPlatformThreadStart *)malloc(sizeof(*start));
  if (start == NULL) return ENOMEM;
  start->function = function;
  start->context = context;
  handle = _beginthreadex(NULL, 0, aura_platform_thread_start, start, 0, NULL);
  if (handle == 0)
  {
    free(start);
    return errno ? errno : EAGAIN;
  }
  thread->native = (HANDLE)handle;
  return 0;
}

int aura_platform_thread_join(AuraPlatformThread *thread)
{
  if (thread == NULL || thread->native == NULL) return EINVAL;
  if (WaitForSingleObject(thread->native, INFINITE) != WAIT_OBJECT_0) return EAGAIN;
  CloseHandle(thread->native);
  thread->native = NULL;
  return 0;
}

AuraPlatformFile aura_platform_file_open(const char *path, int flags)
{
  int access = 0;
  int creation = OPEN_EXISTING;
  if (flags == 0) access = _O_RDONLY;
  else if (flags == 1) { access = _O_WRONLY; creation = CREATE_ALWAYS; }
  else if (flags == 2) { access = _O_RDWR; creation = OPEN_ALWAYS; }
  else if (flags == 3) { access = _O_WRONLY; creation = OPEN_ALWAYS; }
  else { errno = EINVAL; return AURA_PLATFORM_FILE_INVALID; }
  int fd = _open(path, access | _O_BINARY, _S_IREAD | _S_IWRITE);
  if (fd < 0 && creation != OPEN_EXISTING)
  {
    fd = _open(path, access | _O_BINARY | (creation == CREATE_ALWAYS ? _O_CREAT | _O_TRUNC : _O_CREAT),
               _S_IREAD | _S_IWRITE);
  }
  if (fd >= 0 && flags == 3) (void)_lseek(fd, 0, SEEK_END);
  return (AuraPlatformFile)fd;
}

int64_t aura_platform_file_read(AuraPlatformFile file, void *buffer, size_t length)
{
  return _read((int)file, buffer, (unsigned)length);
}

int64_t aura_platform_file_write(AuraPlatformFile file, const void *buffer, size_t length)
{
  return _write((int)file, buffer, (unsigned)length);
}

int aura_platform_file_flush(AuraPlatformFile file) { return _commit((int)file); }
int aura_platform_file_close(AuraPlatformFile file) { return _close((int)file); }

static int aura_platform_wsa_started;

int aura_platform_socket_startup(void)
{
  WSADATA data;
  if (aura_platform_wsa_started) return 0;
  if (WSAStartup(MAKEWORD(2, 2), &data) != 0) return WSAGetLastError();
  aura_platform_wsa_started = 1;
  return 0;
}

void aura_platform_socket_shutdown(void)
{
  if (aura_platform_wsa_started) { WSACleanup(); aura_platform_wsa_started = 0; }
}

AuraPlatformSocket aura_platform_socket_open(int family, int type, int protocol)
{ return socket(family, type, protocol); }
int aura_platform_socket_close(AuraPlatformSocket socket_handle)
{ return closesocket(socket_handle); }
int aura_platform_socket_nonblocking(AuraPlatformSocket socket_handle)
{ u_long enabled = 1; return ioctlsocket(socket_handle, FIONBIO, &enabled); }
int aura_platform_socket_wait(AuraPlatformSocket socket_handle, short events, int timeout_ms,
                              short *revents)
{
  WSAPOLLFD descriptor = {socket_handle, events, 0};
  int result = WSAPoll(&descriptor, 1, timeout_ms);
  if (revents != NULL) *revents = descriptor.revents;
  return result;
}
int64_t aura_platform_socket_recv(AuraPlatformSocket socket_handle, void *buffer, size_t length)
{ return recv(socket_handle, buffer, (int)length, 0); }
int64_t aura_platform_socket_send(AuraPlatformSocket socket_handle, const void *buffer, size_t length,
                                  int flags)
{ return send(socket_handle, buffer, (int)length, flags); }

int aura_platform_signal_install_shutdown(void (*handler)(int))
{
  if (signal(SIGINT, handler) == SIG_ERR) return 0;
  return signal(SIGTERM, handler) == SIG_ERR ? 0 : 1;
}
int aura_platform_signal_restore_shutdown(void) { return 1; }

#else

#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>

int aura_platform_mutex_init(AuraPlatformMutex *mutex, int recursive)
{
  pthread_mutexattr_t attributes;
  if (mutex == NULL || pthread_mutexattr_init(&attributes) != 0) return EINVAL;
  if (recursive) pthread_mutexattr_settype(&attributes, PTHREAD_MUTEX_RECURSIVE);
  int result = pthread_mutex_init(&mutex->native, &attributes);
  pthread_mutexattr_destroy(&attributes);
  return result;
}
void aura_platform_mutex_destroy(AuraPlatformMutex *mutex)
{ if (mutex != NULL) pthread_mutex_destroy(&mutex->native); }
void aura_platform_mutex_lock(AuraPlatformMutex *mutex) { pthread_mutex_lock(&mutex->native); }
void aura_platform_mutex_unlock(AuraPlatformMutex *mutex) { pthread_mutex_unlock(&mutex->native); }
int aura_platform_cond_init(AuraPlatformCond *condition)
{ return condition == NULL ? EINVAL : pthread_cond_init(&condition->native, NULL); }
void aura_platform_cond_destroy(AuraPlatformCond *condition)
{ if (condition != NULL) pthread_cond_destroy(&condition->native); }
void aura_platform_cond_wait(AuraPlatformCond *condition, AuraPlatformMutex *mutex)
{ pthread_cond_wait(&condition->native, &mutex->native); }
void aura_platform_cond_signal(AuraPlatformCond *condition) { pthread_cond_signal(&condition->native); }
void aura_platform_cond_broadcast(AuraPlatformCond *condition) { pthread_cond_broadcast(&condition->native); }
int aura_platform_thread_create(AuraPlatformThread *thread,
                                AuraPlatformThreadFn function, void *context)
{ return thread == NULL || function == NULL ? EINVAL : pthread_create(&thread->native, NULL, function, context); }
int aura_platform_thread_join(AuraPlatformThread *thread)
{ return thread == NULL ? EINVAL : pthread_join(thread->native, NULL); }

AuraPlatformFile aura_platform_file_open(const char *path, int flags)
{
  int native_flags;
  switch (flags)
  {
    case 0: native_flags = O_RDONLY; break;
    case 1: native_flags = O_WRONLY | O_CREAT | O_TRUNC; break;
    case 2: native_flags = O_RDWR | O_CREAT; break;
    case 3: native_flags = O_WRONLY | O_CREAT | O_APPEND; break;
    default: errno = EINVAL; return AURA_PLATFORM_FILE_INVALID;
  }
  return (AuraPlatformFile)open(path, native_flags, 0666);
}
int64_t aura_platform_file_read(AuraPlatformFile file, void *buffer, size_t length)
{ return (int64_t)read((int)file, buffer, length); }
int64_t aura_platform_file_write(AuraPlatformFile file, const void *buffer, size_t length)
{ return (int64_t)write((int)file, buffer, length); }
int aura_platform_file_flush(AuraPlatformFile file) { return fsync((int)file); }
int aura_platform_file_close(AuraPlatformFile file) { return close((int)file); }

int aura_platform_socket_startup(void) { return 0; }
void aura_platform_socket_shutdown(void) {}
AuraPlatformSocket aura_platform_socket_open(int family, int type, int protocol)
{ return socket(family, type, protocol); }
int aura_platform_socket_close(AuraPlatformSocket socket_handle) { return close(socket_handle); }
int aura_platform_socket_nonblocking(AuraPlatformSocket socket_handle)
{ int flags = fcntl(socket_handle, F_GETFL, 0); return flags < 0 ? -1 : fcntl(socket_handle, F_SETFL, flags | O_NONBLOCK); }
int aura_platform_socket_wait(AuraPlatformSocket socket_handle, short events, int timeout_ms,
                              short *revents)
{
  struct pollfd descriptor = {socket_handle, events, 0};
  int result = poll(&descriptor, 1, timeout_ms);
  if (revents != NULL) *revents = descriptor.revents;
  return result;
}
int64_t aura_platform_socket_recv(AuraPlatformSocket socket_handle, void *buffer, size_t length)
{ return (int64_t)recv(socket_handle, buffer, length, 0); }
int64_t aura_platform_socket_send(AuraPlatformSocket socket_handle, const void *buffer, size_t length,
                                  int flags)
{ return (int64_t)send(socket_handle, buffer, length, flags); }

int aura_platform_signal_install_shutdown(void (*handler)(int))
{
  if (signal(SIGINT, handler) == SIG_ERR) return 0;
  return signal(SIGTERM, handler) == SIG_ERR ? 0 : 1;
}
int aura_platform_signal_restore_shutdown(void) { return 1; }

#endif

#if !defined(_WIN32)
#include <time.h>
int64_t aura_platform_monotonic_millis(void)
{
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return 0;
  return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}
#endif

#if defined(_WIN32)
int64_t aura_platform_monotonic_millis(void)
{
  static LARGE_INTEGER frequency;
  static LONG initialized;
  LARGE_INTEGER counter;
  if (!initialized)
  {
    if (!QueryPerformanceFrequency(&frequency) || frequency.QuadPart <= 0) return 0;
    InterlockedExchange(&initialized, 1);
  }
  if (!QueryPerformanceCounter(&counter)) return 0;
  return (int64_t)((counter.QuadPart * 1000) / frequency.QuadPart);
}
#endif
