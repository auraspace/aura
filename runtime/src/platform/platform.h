#ifndef AURA_PLATFORM_H
#define AURA_PLATFORM_H

#include <stddef.h>
#include <stdint.h>

#if defined(_WIN32)
#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <stdatomic.h>
typedef SOCKET AuraPlatformSocket;
#define AURA_PLATFORM_SOCKET_INVALID INVALID_SOCKET
#define AURA_PLATFORM_SOCKET_IS_INVALID(value) ((value) == INVALID_SOCKET)
#define AURA_PLATFORM_NETWORK 1

#else
#include <pthread.h>
#include <poll.h>
typedef int AuraPlatformSocket;
#define AURA_PLATFORM_SOCKET_INVALID (-1)
#define AURA_PLATFORM_SOCKET_IS_INVALID(value) ((value) < 0)
#define AURA_PLATFORM_NETWORK 1
#endif

typedef struct AuraPlatformMutex
{
#if defined(_WIN32)
  CRITICAL_SECTION native;
#else
  pthread_mutex_t native;
#endif
} AuraPlatformMutex;

typedef struct AuraPlatformCond
{
#if defined(_WIN32)
  CONDITION_VARIABLE native;
#else
  pthread_cond_t native;
#endif
} AuraPlatformCond;

typedef struct AuraPlatformThread
{
#if defined(_WIN32)
  HANDLE native;
#else
  pthread_t native;
#endif
} AuraPlatformThread;

typedef void *(*AuraPlatformThreadFn)(void *context);

typedef struct AuraPlatformWake
{
  AuraPlatformSocket read;
  AuraPlatformSocket write;
} AuraPlatformWake;

#if defined(_WIN32)
typedef WSAPOLLFD AuraPlatformPollFd;
#else
typedef struct pollfd AuraPlatformPollFd;
#endif

int aura_platform_poll(AuraPlatformPollFd *descriptors, size_t count,
                       int timeout_ms);
int aura_platform_random_bytes(void *output, size_t length);

int aura_platform_wake_init(AuraPlatformWake *wake);
int aura_platform_wake_signal(const AuraPlatformWake *wake);
int aura_platform_wake_drain(const AuraPlatformWake *wake);
void aura_platform_wake_destroy(AuraPlatformWake *wake);

int aura_platform_mutex_init(AuraPlatformMutex *mutex, int recursive);
void aura_platform_mutex_destroy(AuraPlatformMutex *mutex);
void aura_platform_mutex_lock(AuraPlatformMutex *mutex);
void aura_platform_mutex_unlock(AuraPlatformMutex *mutex);
int aura_platform_cond_init(AuraPlatformCond *condition);
void aura_platform_cond_destroy(AuraPlatformCond *condition);
void aura_platform_cond_wait(AuraPlatformCond *condition, AuraPlatformMutex *mutex);
void aura_platform_cond_signal(AuraPlatformCond *condition);
void aura_platform_cond_broadcast(AuraPlatformCond *condition);
int aura_platform_thread_create(AuraPlatformThread *thread,
                                AuraPlatformThreadFn function, void *context);
int aura_platform_thread_join(AuraPlatformThread *thread);

#if defined(_WIN32)
/* Compatibility names keep legacy runtime translation units source-stable
 * while their synchronization is implemented by this platform layer. */
typedef AuraPlatformMutex pthread_mutex_t;
typedef AuraPlatformCond pthread_cond_t;
typedef AuraPlatformThread pthread_t;
typedef struct { int recursive; } pthread_mutexattr_t;
typedef atomic_int pthread_once_t;
#define PTHREAD_ONCE_INIT ATOMIC_VAR_INIT(0)
#define PTHREAD_MUTEX_RECURSIVE 1
static inline int pthread_mutexattr_init(pthread_mutexattr_t *a) { a->recursive = 0; return 0; }
static inline int pthread_mutexattr_settype(pthread_mutexattr_t *a, int type) { a->recursive = type == PTHREAD_MUTEX_RECURSIVE; return 0; }
static inline int pthread_mutexattr_destroy(pthread_mutexattr_t *a) { (void)a; return 0; }
static inline int pthread_mutex_init(pthread_mutex_t *m, const pthread_mutexattr_t *a) { return aura_platform_mutex_init(m, a != NULL && a->recursive); }
static inline int pthread_mutex_destroy(pthread_mutex_t *m) { aura_platform_mutex_destroy(m); return 0; }
static inline int pthread_mutex_lock(pthread_mutex_t *m) { aura_platform_mutex_lock(m); return 0; }
static inline int pthread_mutex_unlock(pthread_mutex_t *m) { aura_platform_mutex_unlock(m); return 0; }
static inline int pthread_cond_init(pthread_cond_t *c, const void *a) { (void)a; return aura_platform_cond_init(c); }
static inline int pthread_cond_destroy(pthread_cond_t *c) { aura_platform_cond_destroy(c); return 0; }
static inline int pthread_cond_wait(pthread_cond_t *c, pthread_mutex_t *m) { aura_platform_cond_wait(c, m); return 0; }
static inline int pthread_cond_signal(pthread_cond_t *c) { aura_platform_cond_signal(c); return 0; }
static inline int pthread_cond_broadcast(pthread_cond_t *c) { aura_platform_cond_broadcast(c); return 0; }
static inline int pthread_create(pthread_t *t, const void *a, void *(*f)(void *), void *c) { (void)a; return aura_platform_thread_create(t, f, c); }
static inline int pthread_join(pthread_t t, void **result) { (void)result; return aura_platform_thread_join(&t); }
static inline int pthread_once(pthread_once_t *once, void (*f)(void))
{
  int expected = 0;
  if (atomic_compare_exchange_strong(once, &expected, 1)) f();
  return 0;
}
#endif

#ifndef AURA_PLATFORM_FILE_TYPE_DEFINED
#define AURA_PLATFORM_FILE_TYPE_DEFINED 1
typedef intptr_t AuraPlatformFile;
#endif
#define AURA_PLATFORM_FILE_INVALID ((AuraPlatformFile)-1)

AuraPlatformFile aura_platform_file_open(const char *path, int flags);
int64_t aura_platform_file_read(AuraPlatformFile file, void *buffer, size_t length);
int64_t aura_platform_file_write(AuraPlatformFile file, const void *buffer, size_t length);
int aura_platform_file_flush(AuraPlatformFile file);
int aura_platform_file_close(AuraPlatformFile file);

int aura_platform_socket_startup(void);
void aura_platform_socket_shutdown(void);
AuraPlatformSocket aura_platform_socket_open(int family, int type, int protocol);
int aura_platform_socket_close(AuraPlatformSocket socket);
int aura_platform_socket_nonblocking(AuraPlatformSocket socket);
int aura_platform_socket_wait(AuraPlatformSocket socket, short events, int timeout_ms,
                              short *revents);
int64_t aura_platform_socket_recv(AuraPlatformSocket socket, void *buffer, size_t length);
int64_t aura_platform_socket_send(AuraPlatformSocket socket, const void *buffer, size_t length,
                                  int flags);

int aura_platform_signal_install_shutdown(void (*handler)(int));
int aura_platform_signal_restore_shutdown(void);

/* Monotonic time is the only clock accepted by scheduler deadlines. */
int64_t aura_platform_monotonic_millis(void);

#endif
