/* Aura runtime — linked into every binary produced by aura build. */
#if !defined(_POSIX_C_SOURCE)
#define _POSIX_C_SOURCE 200809L
#endif
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdbool.h>
#include <errno.h>
#include <ctype.h>
#include <inttypes.h>
#include <time.h>
#include <signal.h>
#include <sys/stat.h>
#if defined(__unix__) || defined(__APPLE__)
#include <dirent.h>
#endif
#if defined(__unix__) || defined(__APPLE__)
#include <unistd.h>
#include <pthread.h>
#endif

static volatile sig_atomic_t aura_shutdown_signal = 0;

static void aura_signal_handler(int signal_number)
{
  (void)signal_number;
  aura_shutdown_signal = 1;
}

int aura_signal_install_shutdown(void)
{
#if defined(__unix__) || defined(__APPLE__)
  if (signal(SIGINT, aura_signal_handler) == SIG_ERR ||
      signal(SIGTERM, aura_signal_handler) == SIG_ERR)
  {
    return 0;
  }
  return 1;
#else
  return 0;
#endif
}

_Bool aura_signal_shutdown_requested(void)
{
  return aura_shutdown_signal != 0;
}

void aura_signal_clear_shutdown(void)
{
  aura_shutdown_signal = 0;
}

int64_t aura_error_kind_code(int64_t code)
{
  if (code == EINVAL || code == E2BIG || code == ENAMETOOLONG) return 0;
  if (code == ENOENT || code == ENOTDIR || code == ENODEV) return 1;
  if (code == EACCES || code == EPERM || code == EROFS) return 2;
  if (code == EIO || code == ENOMEM || code == EBADF) return 3;
  if (code == ECONNRESET || code == ECONNREFUSED || code == ENETDOWN ||
      code == ENETUNREACH || code == EHOSTUNREACH || code == EPIPE) return 4;
  if (code == ETIMEDOUT || code == EAGAIN || code == EWOULDBLOCK) return 5;
  if (code == ECANCELED) return 6;
  if (code == EPROTO || code == EBADMSG) return 7;
  if (code == EOVERFLOW || code == ENOBUFS || code == EMSGSIZE) return 8;
  if (code == ENOTCONN) return 9;
  if (code == ENOSYS || code == ENOTSUP) return 10;
  return 11;
}
#if defined(__unix__) || defined(__APPLE__)
#include <arpa/inet.h>
#include <netdb.h>
#include <netinet/in.h>
#include <sys/socket.h>
#ifndef INADDR_LOOPBACK
#define INADDR_LOOPBACK 0x7f000001U
#endif
#endif
#if defined(__has_include)
#if __has_include("aura_ffi.h")
#include "aura_ffi.h"
#endif
#endif

/* Scheduler helpers appear before the timer implementation. */
int64_t aura_time_monotonic_millis(void);

/* Generated artifacts embed this runtime as one copied C translation unit, so
 * the optional public FFI header is not necessarily beside that copy. Keep a
 * matching declaration fallback here; the header guard makes it harmless
 * when a fixture includes aura_ffi.h before including this file. */
#ifndef AURA_FFI_H
#define AURA_FFI_H
typedef enum AuraFfiStatus
{
  AURA_FFI_OK = 0,
  AURA_FFI_INVALID = 1,
  AURA_FFI_OOM = 2
} AuraFfiStatus;
typedef struct AuraFfiStringView { const char *data; uint64_t len; } AuraFfiStringView;
typedef struct AuraFfiString { char *data; uint64_t len; } AuraFfiString;
typedef enum AuraFfiArrayKind
{
  AURA_FFI_ARRAY_BYTES = 1,
  AURA_FFI_ARRAY_INT64 = 2,
  AURA_FFI_ARRAY_BOOL = 3
} AuraFfiArrayKind;
typedef struct AuraFfiArrayView
{
  const void *data;
  uint64_t len;
  uint64_t cap;
  uint64_t elem_size;
  AuraFfiArrayKind kind;
} AuraFfiArrayView;
typedef struct AuraFfiArray
{
  void *data;
  uint64_t len;
  uint64_t cap;
  uint64_t elem_size;
  AuraFfiArrayKind kind;
} AuraFfiArray;
typedef struct AuraFfiRootGuard { void **slot; int active; } AuraFfiRootGuard;
typedef struct AuraFfiOpaqueHandle AuraFfiOpaqueHandle;
typedef void (*AuraFfiHandleDestroyFn)(void *resource);
typedef struct AuraFfiHandlePin
{
  AuraFfiOpaqueHandle *handle;
  void *resource;
  uint64_t generation;
} AuraFfiHandlePin;
typedef enum AuraFfiBoundary
{
  AURA_FFI_BOUNDARY_SYNC = 0,
  AURA_FFI_BOUNDARY_TASK = 1,
  AURA_FFI_BOUNDARY_AWAIT = 2,
  AURA_FFI_BOUNDARY_CHANNEL = 3,
  AURA_FFI_BOUNDARY_CALLBACK = 4
} AuraFfiBoundary;
#define AURA_FFI_BOUNDARY_REJECTED ((AuraFfiStatus)3)
#define AURA_FFI_BUSY ((AuraFfiStatus)4)
typedef struct AuraTaskFrame AuraTaskFrame;
typedef struct AuraTaskScope AuraTaskScope;
#ifndef AURA_LAZY_TYPES_DEFINED
#define AURA_LAZY_TYPES_DEFINED 1
typedef struct AuraLazyCell AuraLazyCell;
typedef void (*AuraLazyInitFn)(AuraLazyCell *cell, void *environment);
typedef void (*AuraLazyValueDestroyFn)(void *value);
#endif
typedef void (*AuraTaskFrameGcMarkFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskBlockingFn)(AuraTaskFrame *frame, void *environment);
typedef void (*AuraTaskBlockingEnvDestroyFn)(void *environment);
typedef struct AuraIoOperationHandle AuraIoOperationHandle;
typedef void (*AuraIoOperationCleanupFn)(void *resource);
typedef enum AuraIoOperationKind
{
  AURA_IO_OPERATION_FILE_READ = 1,
  AURA_IO_OPERATION_FILE_WRITE = 2,
  AURA_IO_OPERATION_TCP_ACCEPT = 3,
  AURA_IO_OPERATION_TCP_CONNECT = 4,
  AURA_IO_OPERATION_TCP_READ = 5,
  AURA_IO_OPERATION_TCP_WRITE = 6
} AuraIoOperationKind;
typedef enum AuraIoOperationState
{
  AURA_IO_OPERATION_PENDING = 0,
  AURA_IO_OPERATION_COMPLETE = 1,
  AURA_IO_OPERATION_CANCELLED = 2,
  AURA_IO_OPERATION_FAILED = 3
} AuraIoOperationState;
typedef enum AuraIoOutcome
{
  AURA_IO_OUTCOME_OK = 0,
  AURA_IO_OUTCOME_EOF = 1,
  AURA_IO_OUTCOME_CANCELLED = 2,
  AURA_IO_OUTCOME_CLOSED = 3,
  AURA_IO_OUTCOME_PERMISSION = 4,
  AURA_IO_OUTCOME_TIMEOUT = 5,
  AURA_IO_OUTCOME_UNSUPPORTED = 6,
  AURA_IO_OUTCOME_ERROR = 7
} AuraIoOutcome;
typedef struct AuraIoOperationResult
{
  AuraIoOperationKind kind;
  AuraIoOperationState state;
  AuraIoOutcome outcome;
  uint64_t bytes_transferred;
  int32_t native_status;
} AuraIoOperationResult;
typedef struct AuraFfiCallbackFrame AuraFfiCallbackFrame;
typedef struct AuraFfiCallback AuraFfiCallback;
typedef int32_t (*AuraFfiCallbackFn)(void *environment, const void *payload,
                                     uint64_t payload_len);
typedef void (*AuraFfiCallbackEnvDestroyFn)(void *environment);
typedef enum AuraFfiOutcome
{
  AURA_FFI_OUTCOME_OK = 0,
  AURA_FFI_OUTCOME_CANCELLED = 1,
  AURA_FFI_OUTCOME_INVALID = 2,
  AURA_FFI_OUTCOME_NOT_FOUND = 3,
  AURA_FFI_OUTCOME_PERMISSION = 4,
  AURA_FFI_OUTCOME_UNAVAILABLE = 5,
  AURA_FFI_OUTCOME_TIMEOUT = 6,
  AURA_FFI_OUTCOME_FOREIGN_ERROR = 7
} AuraFfiOutcome;
#endif

/* Task types are used by HTTP declarations below and must remain available
 * even when aura_ffi.h was included before this translation unit. */
typedef struct AuraTaskExecutor AuraTaskExecutor;
typedef struct AuraTaskFrame AuraTaskFrame;
#ifndef AURA_LAZY_TYPES_DEFINED
#define AURA_LAZY_TYPES_DEFINED 1
typedef struct AuraLazyCell AuraLazyCell;
typedef void (*AuraLazyInitFn)(AuraLazyCell *cell, void *environment);
typedef void (*AuraLazyValueDestroyFn)(void *value);
#endif

#ifndef AURA_TASK_POLL_STATE_DEFINED
#define AURA_TASK_POLL_STATE_DEFINED 1
typedef enum AuraTaskPollState
{
  AURA_TASK_READY = 0,
  AURA_TASK_PENDING = 1,
  AURA_TASK_COMPLETE = 2,
  AURA_TASK_FAILED = 3,
  AURA_TASK_CANCELLED = 4
} AuraTaskPollState;
#endif

#ifndef AURA_FILE_H
#define AURA_FILE_H
typedef struct AuraFile AuraFile;
typedef enum AuraFileStatus
{
  AURA_FILE_OK = 0,
  AURA_FILE_PENDING = 1,
  AURA_FILE_EOF = 2,
  AURA_FILE_ERROR = -1,
  AURA_FILE_CLOSED = -2,
  AURA_FILE_UNSUPPORTED = -3,
  AURA_FILE_PERMISSION = -4
} AuraFileStatus;
typedef enum AuraFileMode
{
  AURA_FILE_READ = 0,
  AURA_FILE_WRITE = 1,
  AURA_FILE_READ_WRITE = 2,
  AURA_FILE_APPEND = 3
} AuraFileMode;
#endif

#if defined(__linux__) || defined(__APPLE__)
#define AURA_TCP_POSIX 1
#include <fcntl.h>
#include <netinet/in.h>
#include <poll.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <arpa/inet.h>
#include <unistd.h>
#else
#define AURA_TCP_POSIX 0
#endif

/* Forward decls for throw (defined below) */
void aura_throw_string(const char *s);
void aura_throw_int(int64_t v);
void aura_throw_bool(bool v);

/* ---- Console I/O ---- */

void aura_print(const char *s)
{
  if (s == NULL)
  {
    fputs("null", stdout);
  }
  else
  {
    fputs(s, stdout);
  }
  fflush(stdout);
}

void aura_println(const char *s)
{
  if (s == NULL)
  {
    puts("null");
  }
  else
  {
    puts(s);
  }
}

void aura_eprint(const char *s)
{
  if (s == NULL)
  {
    fputs("null", stderr);
  }
  else
  {
    fputs(s, stderr);
  }
  fflush(stderr);
}

void aura_eprintln(const char *s)
{
  if (s == NULL)
  {
    fputs("null\n", stderr);
  }
  else
  {
    fputs(s, stderr);
    fputc('\n', stderr);
  }
  fflush(stderr);
}

static int aura_log_min_level = 0;

int aura_log_set_min_level(int level)
{
  if (level < 0 || level > 3) return 0;
  aura_log_min_level = level;
  return 1;
}

int aura_log_get_min_level(void)
{
  return aura_log_min_level;
}

void aura_log(int level, const char *message)
{
  static const char *const names[] = {"DEBUG", "INFO", "WARN", "ERROR"};
  if (level < aura_log_min_level) return;
  const char *name = (level >= 0 && level < 4) ? names[level] : "INFO";
  fprintf(stderr, "[%s] %s\n", name, message != NULL ? message : "null");
  fflush(stderr);
}

/* ---- File I/O (std.io) ----
 * Text errors throw String messages (single-threaded; static errbuf).
 * Strings are UTF-8 byte sequences; binary with embedded NUL is not supported
 * for the String path (matches the rest of the String surface).
 */

#define AURA_IO_MAX_FILE ((int64_t)256 * 1024 * 1024)

static char aura_io_errbuf[1024];

/* ---- Bounded file handle I/O ----
 * Regular files do not provide portable readiness notifications: POSIX
 * O_NONBLOCK is ignored for them. Keep this API explicit about that fact.
 * Each operation owns no caller buffer beyond the call and performs at most
 * one read/write syscall, so an eventual async scheduler can resume around
 * this boundary without changing ownership semantics. */
struct AuraFile
{
  int fd;
  bool closed;
};

static char aura_file_errbuf[256] = "no error";

const char *aura_file_last_error(void)
{
  return aura_file_errbuf;
}

static AuraFileStatus aura_file_status_for_errno(int error)
{
  if (error == EACCES || error == EPERM || error == EROFS)
  {
    return AURA_FILE_PERMISSION;
  }
  if (error == EAGAIN || error == EWOULDBLOCK)
  {
    return AURA_FILE_PENDING;
  }
  return AURA_FILE_ERROR;
}

static AuraFileStatus aura_file_error(const char *op, int error)
{
  if (error == 0)
  {
    error = EIO;
  }
  snprintf(aura_file_errbuf, sizeof(aura_file_errbuf), "file %s failed: %s",
           op ? op : "operation", strerror(error));
  return aura_file_status_for_errno(error);
}

#if AURA_TCP_POSIX
AuraFileStatus aura_file_open(const char *path, AuraFileMode mode, AuraFile **out)
{
  if (out == NULL)
  {
    return aura_file_error("open", EINVAL);
  }
  *out = NULL;
  if (path == NULL || path[0] == '\0')
  {
    return aura_file_error("open", EINVAL);
  }
  int flags = 0;
  switch (mode)
  {
    case AURA_FILE_READ: flags = O_RDONLY; break;
    case AURA_FILE_WRITE: flags = O_WRONLY | O_CREAT | O_TRUNC; break;
    case AURA_FILE_READ_WRITE: flags = O_RDWR | O_CREAT; break;
    case AURA_FILE_APPEND: flags = O_WRONLY | O_CREAT | O_APPEND; break;
    default: return aura_file_error("open", EINVAL);
  }
  int fd;
  do
  {
    fd = open(path, flags, 0666);
  } while (fd < 0 && errno == EINTR);
  if (fd < 0)
  {
    return aura_file_error("open", errno);
  }
  AuraFile *file = (AuraFile *)calloc(1, sizeof(*file));
  if (file == NULL)
  {
    int error = errno ? errno : ENOMEM;
    close(fd);
    return aura_file_error("open", error);
  }
  file->fd = fd;
  *out = file;
  return AURA_FILE_OK;
}

AuraFileStatus aura_file_read(AuraFile *file, void *buffer, uint64_t capacity,
                              uint64_t *out_read)
{
  if (out_read != NULL) *out_read = 0;
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  if (out_read == NULL || (capacity > 0 && buffer == NULL))
    return aura_file_error("read", EINVAL);
  ssize_t result = read(file->fd, buffer, (size_t)capacity);
  if (result > 0)
  {
    *out_read = (uint64_t)result;
    return AURA_FILE_OK;
  }
  if (result == 0) return AURA_FILE_EOF;
  return aura_file_error("read", errno);
}

AuraFileStatus aura_file_write(AuraFile *file, const void *buffer,
                               uint64_t length, uint64_t *out_written)
{
  if (out_written != NULL) *out_written = 0;
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  if (out_written == NULL || (length > 0 && buffer == NULL))
    return aura_file_error("write", EINVAL);
  ssize_t result = write(file->fd, buffer, (size_t)length);
  if (result >= 0)
  {
    *out_written = (uint64_t)result;
    return result == 0 && length > 0 ? AURA_FILE_PENDING : AURA_FILE_OK;
  }
  return aura_file_error("write", errno);
}

AuraFileStatus aura_file_flush(AuraFile *file)
{
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  return fsync(file->fd) == 0 ? AURA_FILE_OK : aura_file_error("flush", errno);
}

AuraFileStatus aura_file_close(AuraFile *file)
{
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  file->closed = true;
  if (close(file->fd) != 0) return aura_file_error("close", errno);
  return AURA_FILE_OK;
}

AuraFileStatus aura_file_destroy(AuraFile **file)
{
  if (file == NULL || *file == NULL) return AURA_FILE_CLOSED;
  AuraFileStatus status = aura_file_close(*file);
  free(*file);
  *file = NULL;
  return status == AURA_FILE_CLOSED ? AURA_FILE_OK : status;
}
#else
AuraFileStatus aura_file_open(const char *path, AuraFileMode mode, AuraFile **out)
{ (void)path; (void)mode; if (out) *out = NULL; return AURA_FILE_UNSUPPORTED; }
AuraFileStatus aura_file_read(AuraFile *file, void *buffer, uint64_t capacity, uint64_t *out_read)
{ (void)file; (void)buffer; (void)capacity; if (out_read) *out_read = 0; return AURA_FILE_UNSUPPORTED; }
AuraFileStatus aura_file_write(AuraFile *file, const void *buffer, uint64_t length, uint64_t *out_written)
{ (void)file; (void)buffer; (void)length; if (out_written) *out_written = 0; return AURA_FILE_UNSUPPORTED; }
AuraFileStatus aura_file_flush(AuraFile *file) { (void)file; return AURA_FILE_UNSUPPORTED; }
AuraFileStatus aura_file_close(AuraFile *file) { (void)file; return AURA_FILE_UNSUPPORTED; }
AuraFileStatus aura_file_destroy(AuraFile **file) { if (file) *file = NULL; return AURA_FILE_UNSUPPORTED; }
#endif

/* ---- Bounded TCP I/O (std.net, POSIX alpha slice) ----
 *
 * The handles below are opaque at the API boundary.  A handle owns its file
 * descriptor until close/destroy; close transitions it to a permanently
 * closed state, so repeated close calls are harmless.  Sockets are
 * nonblocking.  Every operation that may wait accepts a timeout in
 * milliseconds (zero means do not wait, and a positive value is the maximum
 * poll interval for that operation).  Port-only wrappers remain localhost
 * compatible; endpoint-aware entry points accept host:port strings.
 */

typedef struct AuraTcpListener AuraTcpListener;
typedef struct AuraTcpStream AuraTcpStream;

typedef enum
{
  AURA_TCP_OK = 0,
  AURA_TCP_PENDING = 1,
  AURA_TCP_EOF = 2,
  AURA_TCP_TIMEOUT = 3,
  AURA_TCP_ERROR = -1,
  AURA_TCP_CLOSED = -2,
  AURA_TCP_UNSUPPORTED = -3
} AuraTcpStatus;

struct AuraTcpListener
{
  int fd;
};

struct AuraTcpStream
{
  int fd;
};

static char aura_tcp_errbuf[256] = "no error";

const char *aura_tcp_last_error(void)
{
  return aura_tcp_errbuf;
}

#if AURA_TCP_POSIX

static void aura_tcp_clear_error(void)
{
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "no error");
}

static void aura_tcp_error_errno(const char *op)
{
  int saved = errno;
  const char *detail = strerror(saved);
  if (detail == NULL)
  {
    detail = "unknown error";
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp %s failed: %s", op, detail);
}

static void aura_tcp_error_text(const char *text)
{
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp %s", text ? text : "error");
}

static int aura_tcp_set_nonblocking(int fd)
{
  int flags = fcntl(fd, F_GETFL, 0);
  if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0)
  {
    return -1;
  }
  return 0;
}

static void aura_tcp_disable_sigpipe(int fd)
{
#if defined(SO_NOSIGPIPE)
  int enabled = 1;
  (void)setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &enabled, sizeof(enabled));
#else
  (void)fd;
#endif
}

static AuraTcpStatus aura_tcp_wait(int fd, short events, int timeout_ms)
{
  if (timeout_ms < 0)
  {
    errno = EINVAL;
    aura_tcp_error_errno("timeout");
    return AURA_TCP_ERROR;
  }
  struct pollfd descriptor = {fd, events, 0};
  int result = poll(&descriptor, 1, timeout_ms);
  if (result < 0)
  {
    aura_tcp_error_errno("poll");
    return AURA_TCP_ERROR;
  }
  if (result == 0)
  {
    return AURA_TCP_TIMEOUT;
  }
  if ((descriptor.revents & (POLLERR | POLLNVAL)) != 0)
  {
    errno = descriptor.revents & POLLNVAL ? EBADF : EIO;
    aura_tcp_error_errno("poll");
    return AURA_TCP_ERROR;
  }
  if ((descriptor.revents & events) == 0)
  {
    errno = ECONNRESET;
    aura_tcp_error_errno("poll");
    return AURA_TCP_ERROR;
  }
  return AURA_TCP_OK;
}

static AuraTcpStatus aura_tcp_wait_or_pending(int fd, short events, int timeout_ms)
{
  AuraTcpStatus status = aura_tcp_wait(fd, events, timeout_ms);
  return status == AURA_TCP_TIMEOUT && timeout_ms == 0 ? AURA_TCP_PENDING : status;
}

static AuraTcpStream *aura_tcp_stream_from_fd(int fd)
{
  AuraTcpStream *stream = (AuraTcpStream *)malloc(sizeof(*stream));
  if (stream == NULL)
  {
    errno = ENOMEM;
    aura_tcp_error_errno("allocate stream");
    close(fd);
    return NULL;
  }
  stream->fd = fd;
  return stream;
}

static void aura_tcp_endpoint_error(const char *text)
{
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp endpoint: %s", text);
}

static int aura_tcp_endpoint_parts(const char *endpoint, char *host,
                                   size_t host_capacity, char *service,
                                   size_t service_capacity)
{
  if (endpoint == NULL || endpoint[0] == '\0')
  {
    return 0;
  }
  size_t length = strlen(endpoint);
  if (endpoint[0] == '[')
  {
    const char *closing = strchr(endpoint, ']');
    if (closing == NULL || closing[1] != ':')
    {
      return 0;
    }
    size_t host_length = (size_t)(closing - endpoint - 1);
    size_t service_length = strlen(closing + 2);
    if (host_length == 0 || host_length >= host_capacity ||
        service_length == 0 || service_length >= service_capacity)
    {
      return 0;
    }
    memcpy(host, endpoint + 1, host_length);
    host[host_length] = '\0';
    memcpy(service, closing + 2, service_length + 1);
    return 1;
  }
  size_t digits = 0;
  while (digits < length && endpoint[digits] >= '0' && endpoint[digits] <= '9')
  {
    digits++;
  }
  if (digits == length)
  {
    if (strlen("127.0.0.1") >= host_capacity || length >= service_capacity)
    {
      return 0;
    }
    strcpy(host, "127.0.0.1");
    strcpy(service, endpoint);
    return 1;
  }
  const char *separator = strrchr(endpoint, ':');
  if (separator == NULL || separator == endpoint || separator[1] == '\0' ||
      strchr(endpoint, ':') != separator)
  {
    return 0;
  }
  size_t host_length = (size_t)(separator - endpoint);
  size_t service_length = strlen(separator + 1);
  if (host_length >= host_capacity || service_length >= service_capacity)
  {
    return 0;
  }
  memcpy(host, endpoint, host_length);
  host[host_length] = '\0';
  memcpy(service, separator + 1, service_length + 1);
  return 1;
}

static int aura_tcp_endpoint_valid_service(const char *service, int allow_zero)
{
  if (service == NULL || service[0] == '\0')
  {
    return 0;
  }
  char *end = NULL;
  unsigned long value = strtoul(service, &end, 10);
  return end != service && *end == '\0' && value <= UINT16_MAX &&
         (allow_zero || value != 0);
}

AuraTcpStatus aura_tcp_listener_bind_endpoint(const char *endpoint,
                                              uint16_t *out_port,
                                              AuraTcpListener **out_listener)
{
  aura_tcp_clear_error();
  if (out_port == NULL || out_listener == NULL)
  {
    errno = EINVAL;
    aura_tcp_error_errno("bind");
    return AURA_TCP_ERROR;
  }
  *out_port = 0;
  *out_listener = NULL;
  char host[256];
  char service[32];
  if (!aura_tcp_endpoint_parts(endpoint, host, sizeof(host), service,
                               sizeof(service)) ||
      !aura_tcp_endpoint_valid_service(service, 1))
  {
    aura_tcp_endpoint_error("expected PORT, HOST:PORT, or [IPv6]:PORT");
    return AURA_TCP_ERROR;
  }
  struct addrinfo hints;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_UNSPEC;
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_PASSIVE;
  struct addrinfo *addresses = NULL;
  int resolved = getaddrinfo(host, service, &hints, &addresses);
  if (resolved != 0)
  {
    aura_tcp_endpoint_error(gai_strerror(resolved));
    return AURA_TCP_ERROR;
  }

  int fd = -1;
  for (struct addrinfo *candidate = addresses; candidate != NULL;
       candidate = candidate->ai_next)
  {
    fd = socket(candidate->ai_family, candidate->ai_socktype,
                candidate->ai_protocol);
    if (fd < 0)
    {
      continue;
    }
    int reuse = 1;
    if (setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse)) != 0 ||
        bind(fd, candidate->ai_addr, candidate->ai_addrlen) != 0 ||
        listen(fd, 16) != 0 || aura_tcp_set_nonblocking(fd) != 0)
    {
      close(fd);
      fd = -1;
      continue;
    }
    break;
  }
  freeaddrinfo(addresses);
  if (fd < 0)
  {
    aura_tcp_error_errno("listen");
    return AURA_TCP_ERROR;
  }
  struct sockaddr_storage bound;
  socklen_t bound_size = (socklen_t)sizeof(bound);
  if (getsockname(fd, (struct sockaddr *)&bound, &bound_size) != 0)
  {
    aura_tcp_error_errno("read bound port");
    close(fd);
    return AURA_TCP_ERROR;
  }
  AuraTcpListener *listener = (AuraTcpListener *)malloc(sizeof(*listener));
  if (listener == NULL)
  {
    errno = ENOMEM;
    aura_tcp_error_errno("allocate listener");
    close(fd);
    return AURA_TCP_ERROR;
  }
  listener->fd = fd;
  if (bound.ss_family == AF_INET)
  {
    *out_port = ntohs(((struct sockaddr_in *)&bound)->sin_port);
  }
  else if (bound.ss_family == AF_INET6)
  {
    *out_port = ntohs(((struct sockaddr_in6 *)&bound)->sin6_port);
  }
  else
  {
    aura_tcp_error_text("unsupported bound address");
    close(listener->fd);
    free(listener);
    return AURA_TCP_ERROR;
  }
  *out_listener = listener;
  return AURA_TCP_OK;
}

AuraTcpStatus aura_tcp_listener_bind(uint16_t port, uint16_t *out_port,
                                     AuraTcpListener **out_listener)
{
  char endpoint[32];
  snprintf(endpoint, sizeof(endpoint), "127.0.0.1:%u", port);
  return aura_tcp_listener_bind_endpoint(endpoint, out_port, out_listener);
}

AuraTcpStatus aura_tcp_listener_accept(AuraTcpListener *listener, int timeout_ms,
                                       AuraTcpStream **out_stream)
{
  aura_tcp_clear_error();
  if (out_stream == NULL)
  {
    errno = EINVAL;
    aura_tcp_error_errno("accept");
    return AURA_TCP_ERROR;
  }
  *out_stream = NULL;
  if (listener == NULL || listener->fd < 0)
  {
    aura_tcp_error_text("accept on closed listener");
    return AURA_TCP_CLOSED;
  }
  AuraTcpStatus waited = aura_tcp_wait_or_pending(listener->fd, POLLIN, timeout_ms);
  if (waited != AURA_TCP_OK)
  {
    return waited;
  }
  int fd = accept(listener->fd, NULL, NULL);
  if (fd < 0)
  {
    if (errno == EAGAIN || errno == EWOULDBLOCK)
    {
      return AURA_TCP_PENDING;
    }
    aura_tcp_error_errno("accept");
    return AURA_TCP_ERROR;
  }
  if (aura_tcp_set_nonblocking(fd) != 0)
  {
    aura_tcp_error_errno("nonblocking stream");
    close(fd);
    return AURA_TCP_ERROR;
  }
  aura_tcp_disable_sigpipe(fd);
  *out_stream = aura_tcp_stream_from_fd(fd);
  return *out_stream == NULL ? AURA_TCP_ERROR : AURA_TCP_OK;
}

AuraTcpStatus aura_tcp_stream_connect_endpoint(const char *endpoint,
                                               int timeout_ms,
                                               AuraTcpStream **out_stream)
{
  aura_tcp_clear_error();
  if (out_stream == NULL)
  {
    errno = EINVAL;
    aura_tcp_error_errno("connect");
    return AURA_TCP_ERROR;
  }
  *out_stream = NULL;
  char host[256];
  char service[32];
  if (timeout_ms < 0 ||
      !aura_tcp_endpoint_parts(endpoint, host, sizeof(host), service,
                               sizeof(service)) ||
      !aura_tcp_endpoint_valid_service(service, 0))
  {
    aura_tcp_endpoint_error("expected PORT, HOST:PORT, or [IPv6]:PORT");
    return AURA_TCP_ERROR;
  }
  struct addrinfo hints;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_UNSPEC;
  hints.ai_socktype = SOCK_STREAM;
  struct addrinfo *addresses = NULL;
  int resolved = getaddrinfo(host, service, &hints, &addresses);
  if (resolved != 0)
  {
    aura_tcp_endpoint_error(gai_strerror(resolved));
    return AURA_TCP_ERROR;
  }

  AuraTcpStatus result = AURA_TCP_ERROR;
  for (struct addrinfo *candidate = addresses; candidate != NULL;
       candidate = candidate->ai_next)
  {
    int fd = socket(candidate->ai_family, candidate->ai_socktype,
                    candidate->ai_protocol);
    if (fd < 0 || aura_tcp_set_nonblocking(fd) != 0)
    {
      if (fd >= 0)
      {
        close(fd);
      }
      continue;
    }
    aura_tcp_disable_sigpipe(fd);
    if (connect(fd, candidate->ai_addr, candidate->ai_addrlen) != 0)
    {
      if (errno != EINPROGRESS && errno != EALREADY)
      {
        close(fd);
        continue;
      }
      AuraTcpStatus waited = aura_tcp_wait(fd, POLLOUT, timeout_ms);
      if (waited != AURA_TCP_OK)
      {
        result = waited;
        close(fd);
        continue;
      }
      int connect_error = 0;
      socklen_t error_size = (socklen_t)sizeof(connect_error);
      if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &connect_error, &error_size) !=
              0 ||
          connect_error != 0)
      {
        if (connect_error != 0)
        {
          errno = connect_error;
        }
        close(fd);
        continue;
      }
    }
    *out_stream = aura_tcp_stream_from_fd(fd);
    result = *out_stream == NULL ? AURA_TCP_ERROR : AURA_TCP_OK;
    break;
  }
  freeaddrinfo(addresses);
  if (result == AURA_TCP_TIMEOUT)
  {
    aura_tcp_error_text("connect timed out");
  }
  return result;
}

AuraTcpStatus aura_tcp_stream_connect(uint16_t port, int timeout_ms,
                                      AuraTcpStream **out_stream)
{
  char endpoint[32];
  snprintf(endpoint, sizeof(endpoint), "127.0.0.1:%u", port);
  return aura_tcp_stream_connect_endpoint(endpoint, timeout_ms, out_stream);
}

AuraTcpStatus aura_tcp_stream_read(AuraTcpStream *stream, void *buffer, size_t capacity,
                                   size_t *out_bytes, int timeout_ms)
{
  aura_tcp_clear_error();
  if (out_bytes == NULL || (buffer == NULL && capacity != 0) || timeout_ms < 0)
  {
    errno = EINVAL;
    aura_tcp_error_errno("read");
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  if (stream == NULL || stream->fd < 0)
  {
    aura_tcp_error_text("read on closed stream");
    return AURA_TCP_CLOSED;
  }
  if (capacity == 0)
  {
    return AURA_TCP_OK;
  }
  AuraTcpStatus waited = aura_tcp_wait_or_pending(stream->fd, POLLIN, timeout_ms);
  if (waited != AURA_TCP_OK)
  {
    return waited;
  }
  ssize_t count = recv(stream->fd, buffer, capacity, 0);
  if (count > 0)
  {
    *out_bytes = (size_t)count;
    return AURA_TCP_OK;
  }
  if (count == 0)
  {
    return AURA_TCP_EOF;
  }
  if (errno == EAGAIN || errno == EWOULDBLOCK)
  {
    return AURA_TCP_PENDING;
  }
  aura_tcp_error_errno("read");
  return AURA_TCP_ERROR;
}

AuraTcpStatus aura_tcp_stream_write(AuraTcpStream *stream, const void *buffer, size_t capacity,
                                    size_t *out_bytes, int timeout_ms)
{
  aura_tcp_clear_error();
  if (out_bytes == NULL || (buffer == NULL && capacity != 0) || timeout_ms < 0)
  {
    errno = EINVAL;
    aura_tcp_error_errno("write");
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  if (stream == NULL || stream->fd < 0)
  {
    aura_tcp_error_text("write on closed stream");
    return AURA_TCP_CLOSED;
  }
  if (capacity == 0)
  {
    return AURA_TCP_OK;
  }
  AuraTcpStatus waited = aura_tcp_wait_or_pending(stream->fd, POLLOUT, timeout_ms);
  if (waited != AURA_TCP_OK)
  {
    return waited;
  }
  int flags = 0;
#if defined(MSG_NOSIGNAL)
  flags |= MSG_NOSIGNAL;
#endif
  ssize_t count = send(stream->fd, buffer, capacity, flags);
  if (count >= 0)
  {
    *out_bytes = (size_t)count;
    return AURA_TCP_OK;
  }
  if (errno == EAGAIN || errno == EWOULDBLOCK)
  {
    return AURA_TCP_PENDING;
  }
  aura_tcp_error_errno("write");
  return AURA_TCP_ERROR;
}

int aura_tcp_listener_close(AuraTcpListener *listener)
{
  if (listener == NULL || listener->fd < 0)
  {
    return 0;
  }
  int fd = listener->fd;
  listener->fd = -1;
  if (close(fd) != 0)
  {
    aura_tcp_error_errno("close listener");
  }
  return 1;
}

void aura_tcp_listener_destroy(AuraTcpListener *listener)
{
  if (listener == NULL)
  {
    return;
  }
  (void)aura_tcp_listener_close(listener);
  free(listener);
}

int aura_tcp_stream_close(AuraTcpStream *stream)
{
  if (stream == NULL || stream->fd < 0)
  {
    return 0;
  }
  int fd = stream->fd;
  stream->fd = -1;
  if (close(fd) != 0)
  {
    aura_tcp_error_errno("close stream");
  }
  return 1;
}

void aura_tcp_stream_destroy(AuraTcpStream *stream)
{
  if (stream == NULL)
  {
    return;
  }
  (void)aura_tcp_stream_close(stream);
  free(stream);
}

#else

AuraTcpStatus aura_tcp_listener_bind_endpoint(const char *endpoint,
                                              uint16_t *out_port,
                                              AuraTcpListener **out_listener)
{
  (void)endpoint;
  if (out_port != NULL)
  {
    *out_port = 0;
  }
  if (out_listener != NULL)
  {
    *out_listener = NULL;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_listener_bind(uint16_t port, uint16_t *out_port,
                                     AuraTcpListener **out_listener)
{
  (void)port;
  if (out_port != NULL)
  {
    *out_port = 0;
  }
  if (out_listener != NULL)
  {
    *out_listener = NULL;
  }
  (void)out_port;
  (void)out_listener;
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_listener_accept(AuraTcpListener *listener, int timeout_ms,
                                       AuraTcpStream **out_stream)
{
  (void)listener;
  (void)timeout_ms;
  if (out_stream != NULL)
  {
    *out_stream = NULL;
  }
  (void)out_stream;
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_connect(uint16_t port, int timeout_ms,
                                      AuraTcpStream **out_stream)
{
  (void)port;
  (void)timeout_ms;
  if (out_stream != NULL)
  {
    *out_stream = NULL;
  }
  (void)out_stream;
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_connect_endpoint(const char *endpoint,
                                               int timeout_ms,
                                               AuraTcpStream **out_stream)
{
  (void)endpoint;
  (void)timeout_ms;
  if (out_stream != NULL)
  {
    *out_stream = NULL;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_read(AuraTcpStream *stream, void *buffer, size_t capacity,
                                   size_t *out_bytes, int timeout_ms)
{
  (void)stream;
  (void)buffer;
  (void)capacity;
  (void)timeout_ms;
  if (out_bytes != NULL)
  {
    *out_bytes = 0;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_write(AuraTcpStream *stream, const void *buffer, size_t capacity,
                                    size_t *out_bytes, int timeout_ms)
{
  (void)stream;
  (void)buffer;
  (void)capacity;
  (void)timeout_ms;
  if (out_bytes != NULL)
  {
    *out_bytes = 0;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

int aura_tcp_listener_close(AuraTcpListener *listener)
{
  (void)listener;
  return 0;
}

void aura_tcp_listener_destroy(AuraTcpListener *listener)
{
  free(listener);
}

int aura_tcp_stream_close(AuraTcpStream *stream)
{
  (void)stream;
  return 0;
}

void aura_tcp_stream_destroy(AuraTcpStream *stream)
{
  free(stream);
}

#endif

/* ---- Bounded HTTP/1.1 request parser (transport-independent) ----
 *
 * This parser consumes one complete request from a byte buffer.  It does not
 * read from a socket and does not retain the input buffer: every field exposed
 * by AuraHttpRequest is heap-owned and is released by
 * aura_http_request_destroy.  A caller can use out_consumed to leave a
 * following keep-alive request in the input buffer.
 */

#define AURA_HTTP_MAX_REQUEST_LINE_BYTES ((size_t)8 * 1024)
#define AURA_HTTP_MAX_HEADERS ((size_t)64)
#define AURA_HTTP_MAX_HEADER_BYTES ((size_t)16 * 1024)
#define AURA_HTTP_MAX_BODY_BYTES ((size_t)8 * 1024 * 1024)
#define AURA_HTTP_MAX_TOTAL_BYTES ((size_t)16 * 1024 * 1024)

typedef enum
{
  AURA_HTTP_PARSE_ERROR = -1,
  AURA_HTTP_PARSE_OK = 0,
  AURA_HTTP_PARSE_INCOMPLETE = 1,
  AURA_HTTP_PARSE_BAD_REQUEST = 400,
  AURA_HTTP_PARSE_METHOD_NOT_ALLOWED = 405,
  AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE = 413
} AuraHttpParseStatus;

typedef struct
{
  char *name;
  char *value;
} AuraHttpHeader;

typedef struct AuraHttpContentLengthReader AuraHttpContentLengthReader;

typedef struct AuraHttpRequest
{
  char *method;
  char *target;
  char *version;
  AuraHttpHeader *headers;
  size_t header_count;
  unsigned char *body;
  size_t body_length;
  size_t total_length;
  /* Borrowed from an active async connection; never owned by the request. */
  AuraHttpContentLengthReader *body_reader;
} AuraHttpRequest;

static int aura_http_is_token(unsigned char c)
{
  if ((c >= (unsigned char)'A' && c <= (unsigned char)'Z') ||
      (c >= (unsigned char)'a' && c <= (unsigned char)'z') ||
      (c >= (unsigned char)'0' && c <= (unsigned char)'9'))
  {
    return 1;
  }
  switch (c)
  {
  case '!':
  case '#':
  case '$':
  case '%':
  case '&':
  case '\'':
  case '*':
  case '+':
  case '-':
  case '.':
  case '^':
  case '_':
  case '`':
  case '|':
  case '~':
    return 1;
  default:
    return 0;
  }
}

static int aura_http_ascii_equal_ci(const unsigned char *left, size_t left_len,
                                    const char *right)
{
  size_t right_len = right == NULL ? 0 : strlen(right);
  size_t i;
  if (right == NULL || left_len != right_len)
  {
    return 0;
  }
  for (i = 0; i < left_len; i++)
  {
    unsigned char a = left[i];
    unsigned char b = (unsigned char)right[i];
    if (a >= (unsigned char)'A' && a <= (unsigned char)'Z')
    {
      a = (unsigned char)(a + ((unsigned char)'a' - (unsigned char)'A'));
    }
    if (b >= (unsigned char)'A' && b <= (unsigned char)'Z')
    {
      b = (unsigned char)(b + ((unsigned char)'a' - (unsigned char)'A'));
    }
    if (a != b)
    {
      return 0;
    }
  }
  return 1;
}

static char *aura_http_copy_string(const unsigned char *data, size_t length)
{
  char *copy;
  if (length == SIZE_MAX)
  {
    return NULL;
  }
  copy = (char *)malloc(length + 1);
  if (copy == NULL)
  {
    return NULL;
  }
  if (length != 0)
  {
    memcpy(copy, data, length);
  }
  copy[length] = '\0';
  return copy;
}

static unsigned char *aura_http_copy_body(const unsigned char *data, size_t length)
{
  unsigned char *copy;
  if (length == 0)
  {
    return NULL;
  }
  copy = (unsigned char *)malloc(length);
  if (copy == NULL)
  {
    return NULL;
  }
  memcpy(copy, data, length);
  return copy;
}

void aura_http_request_destroy(AuraHttpRequest *request)
{
  size_t i;
  if (request == NULL)
  {
    return;
  }
  free(request->method);
  free(request->target);
  free(request->version);
  if (request->headers != NULL)
  {
    for (i = 0; i < request->header_count; i++)
    {
      free(request->headers[i].name);
      free(request->headers[i].value);
    }
  }
  free(request->headers);
  free(request->body);
  memset(request, 0, sizeof(*request));
}

const AuraHttpHeader *aura_http_request_find_header(const AuraHttpRequest *request,
                                                    const char *name)
{
  size_t i;
  if (request == NULL || name == NULL)
  {
    return NULL;
  }
  for (i = 0; i < request->header_count; i++)
  {
    if (aura_http_ascii_equal_ci((const unsigned char *)request->headers[i].name,
                                 strlen(request->headers[i].name), name))
    {
      return &request->headers[i];
    }
  }
  return NULL;
}

const char *aura_http_request_method(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->method;
}

const char *aura_http_request_target(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->target;
}

const char *aura_http_request_version(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->version;
}

size_t aura_http_request_header_count(const AuraHttpRequest *request)
{
  return request == NULL ? 0 : request->header_count;
}

const char *aura_http_request_header_name(const AuraHttpRequest *request,
                                          size_t index)
{
  if (request == NULL || index >= request->header_count)
  {
    return NULL;
  }
  return request->headers[index].name;
}

const char *aura_http_request_header_value(const AuraHttpRequest *request,
                                           size_t index)
{
  if (request == NULL || index >= request->header_count)
  {
    return NULL;
  }
  return request->headers[index].value;
}

const unsigned char *aura_http_request_body(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->body;
}

size_t aura_http_request_body_length(const AuraHttpRequest *request)
{
  return request == NULL ? 0 : request->body_length;
}

typedef enum
{
  AURA_HTTP_LINE_FOUND,
  AURA_HTTP_LINE_INCOMPLETE,
  AURA_HTTP_LINE_BAD,
  AURA_HTTP_LINE_TOO_LARGE
} AuraHttpLineResult;

static AuraHttpLineResult aura_http_find_line(const unsigned char *data,
                                              size_t length, size_t start,
                                              size_t limit, size_t *out_end)
{
  size_t i;
  if (start > length)
  {
    return AURA_HTTP_LINE_INCOMPLETE;
  }
  for (i = start; i < length; i++)
  {
    unsigned char c = data[i];
    if (c == (unsigned char)'\n')
    {
      if (i == start || data[i - 1] != (unsigned char)'\r')
      {
        return AURA_HTTP_LINE_BAD;
      }
      if (i + 1 - start > limit)
      {
        return AURA_HTTP_LINE_TOO_LARGE;
      }
      *out_end = i + 1;
      return AURA_HTTP_LINE_FOUND;
    }
    if (c == (unsigned char)'\r' &&
        i + 1 < length && data[i + 1] != (unsigned char)'\n')
    {
      return AURA_HTTP_LINE_BAD;
    }
    if (i + 1 - start > limit)
    {
      return AURA_HTTP_LINE_TOO_LARGE;
    }
  }
  return length - start > limit ? AURA_HTTP_LINE_TOO_LARGE : AURA_HTTP_LINE_INCOMPLETE;
}

static AuraHttpParseStatus aura_http_line_status(AuraHttpLineResult result)
{
  switch (result)
  {
  case AURA_HTTP_LINE_TOO_LARGE:
    return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
  case AURA_HTTP_LINE_BAD:
    return AURA_HTTP_PARSE_BAD_REQUEST;
  case AURA_HTTP_LINE_INCOMPLETE:
    return AURA_HTTP_PARSE_INCOMPLETE;
  case AURA_HTTP_LINE_FOUND:
    return AURA_HTTP_PARSE_OK;
  default:
    return AURA_HTTP_PARSE_ERROR;
  }
}

static int aura_http_header_name_equal(const unsigned char *name, size_t length,
                                       const char *expected)
{
  return aura_http_ascii_equal_ci(name, length, expected);
}

static int aura_http_parse_content_length(const unsigned char *value, size_t length,
                                          size_t *out_length)
{
  size_t i;
  size_t parsed = 0;
  if (length == 0)
  {
    return 0;
  }
  for (i = 0; i < length; i++)
  {
    unsigned char c = value[i];
    size_t digit;
    if (c < (unsigned char)'0' || c > (unsigned char)'9')
    {
      return 0;
    }
    digit = (size_t)(c - (unsigned char)'0');
    if (parsed > (SIZE_MAX - digit) / 10)
    {
      return 0;
    }
    parsed = parsed * 10 + digit;
  }
  *out_length = parsed;
  return 1;
}

/* Decode a bounded chunked body into the request-owned snapshot. Trailers are
 * intentionally not exposed by the first HTTP/1.1 body API, so only an empty
 * trailer section is accepted. */
static AuraHttpParseStatus aura_http_decode_chunked_body(
    const unsigned char *data, size_t input_length, size_t start,
    unsigned char **out_body, size_t *out_length, size_t *out_consumed)
{
  unsigned char *body = NULL;
  size_t body_length = 0;
  size_t capacity = 0;
  size_t cursor = start;

  for (;;)
  {
    AuraHttpLineResult line_result;
    unsigned char *next_body;
    size_t line_end = 0;
    size_t chunk_length = 0;
    size_t i;
    int saw_digit = 0;

    if (cursor > AURA_HTTP_MAX_TOTAL_BYTES)
    {
      free(body);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    line_result = aura_http_find_line(data, input_length, cursor,
                                      AURA_HTTP_MAX_REQUEST_LINE_BYTES, &line_end);
    if (line_result != AURA_HTTP_LINE_FOUND)
    {
      free(body);
      return aura_http_line_status(line_result);
    }
    if (line_end > AURA_HTTP_MAX_TOTAL_BYTES)
    {
      free(body);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    for (i = cursor; i + 2 < line_end; i++)
    {
      unsigned char c = data[i];
      size_t digit;
      if (c >= (unsigned char)'0' && c <= (unsigned char)'9')
      {
        digit = (size_t)(c - (unsigned char)'0');
      }
      else if (c >= (unsigned char)'a' && c <= (unsigned char)'f')
      {
        digit = (size_t)(c - (unsigned char)'a') + 10;
      }
      else if (c >= (unsigned char)'A' && c <= (unsigned char)'F')
      {
        digit = (size_t)(c - (unsigned char)'A') + 10;
      }
      else
      {
        free(body);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      if (chunk_length > (SIZE_MAX - digit) / 16)
      {
        free(body);
        return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
      }
      chunk_length = chunk_length * 16 + digit;
      saw_digit = 1;
    }
    if (!saw_digit)
    {
      free(body);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    cursor = line_end;
    if (chunk_length == 0)
    {
      size_t trailer_end = 0;
      line_result = aura_http_find_line(data, input_length, cursor,
                                        AURA_HTTP_MAX_HEADER_BYTES, &trailer_end);
      if (line_result != AURA_HTTP_LINE_FOUND)
      {
        free(body);
        return aura_http_line_status(line_result);
      }
      if (trailer_end != cursor + 2)
      {
        free(body);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      if (trailer_end > AURA_HTTP_MAX_TOTAL_BYTES)
      {
        free(body);
        return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
      }
      *out_body = body;
      *out_length = body_length;
      *out_consumed = trailer_end;
      return AURA_HTTP_PARSE_OK;
    }
    if (chunk_length > AURA_HTTP_MAX_BODY_BYTES - body_length)
    {
      free(body);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    if (cursor > AURA_HTTP_MAX_TOTAL_BYTES ||
        chunk_length > AURA_HTTP_MAX_TOTAL_BYTES - cursor ||
        cursor > input_length || chunk_length > input_length - cursor ||
        input_length - cursor - chunk_length < 2)
    {
      free(body);
      return AURA_HTTP_PARSE_INCOMPLETE;
    }
    if (data[cursor + chunk_length] != (unsigned char)'\r' ||
        data[cursor + chunk_length + 1] != (unsigned char)'\n')
    {
      free(body);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    if (body_length + chunk_length > capacity)
    {
      size_t next_capacity = capacity == 0 ? 256 : capacity;
      while (next_capacity < body_length + chunk_length)
      {
        if (next_capacity > AURA_HTTP_MAX_BODY_BYTES / 2)
        {
          next_capacity = AURA_HTTP_MAX_BODY_BYTES;
          break;
        }
        next_capacity *= 2;
      }
      next_body = (unsigned char *)realloc(body, next_capacity);
      if (next_body == NULL)
      {
        free(body);
        return AURA_HTTP_PARSE_ERROR;
      }
      body = next_body;
      capacity = next_capacity;
    }
    memcpy(body + body_length, data + cursor, chunk_length);
    body_length += chunk_length;
    cursor += chunk_length + 2;
  }
}

static int aura_http_header_value_valid(const unsigned char *value, size_t length)
{
  size_t i;
  for (i = 0; i < length; i++)
  {
    unsigned char c = value[i];
    if (c == 0 || c == (unsigned char)'\r' || c == (unsigned char)'\n' ||
        (c < 0x20 && c != (unsigned char)'\t') || c == 0x7f)
    {
      return 0;
    }
  }
  return 1;
}

static AuraHttpParseStatus aura_http_request_parse_impl(
    const void *input, size_t input_length, AuraHttpRequest *out_request,
    size_t *out_consumed, int headers_only, size_t *out_header_end,
    size_t *out_content_length, int *out_chunked)
{
  const unsigned char *data = (const unsigned char *)input;
  AuraHttpRequest parsed;
  AuraHttpLineResult line_result;
  size_t request_line_end = 0;
  size_t request_line_length;
  size_t first_space = SIZE_MAX;
  size_t second_space = SIZE_MAX;
  size_t i;
  size_t cursor;
  size_t header_start;
  size_t header_end = 0;
  size_t content_length = 0;
  int has_content_length = 0;
  int chunked = 0;
  int method_allowed = 0;

  if (out_request == NULL || (input == NULL && input_length != 0))
  {
    return AURA_HTTP_PARSE_ERROR;
  }
  memset(out_request, 0, sizeof(*out_request));
  if (out_consumed != NULL)
  {
    *out_consumed = 0;
  }
  if (out_header_end != NULL)
  {
    *out_header_end = 0;
  }
  if (out_content_length != NULL)
  {
    *out_content_length = 0;
  }
  if (out_chunked != NULL)
  {
    *out_chunked = 0;
  }
  memset(&parsed, 0, sizeof(parsed));

  if (input_length == 0)
  {
    return AURA_HTTP_PARSE_INCOMPLETE;
  }
  line_result = aura_http_find_line(data, input_length, 0,
                                    AURA_HTTP_MAX_REQUEST_LINE_BYTES,
                                    &request_line_end);
  if (line_result != AURA_HTTP_LINE_FOUND)
  {
    return aura_http_line_status(line_result);
  }
  request_line_length = request_line_end - 2;
  for (i = 0; i < request_line_length; i++)
  {
    if (data[i] == (unsigned char)' ')
    {
      if (first_space == SIZE_MAX)
      {
        first_space = i;
      }
      else if (second_space == SIZE_MAX)
      {
        second_space = i;
      }
    }
  }
  if (first_space == SIZE_MAX || second_space == SIZE_MAX || first_space == 0 ||
      second_space <= first_space + 1 || second_space + 1 >= request_line_length)
  {
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }
  for (i = second_space + 1; i < request_line_length; i++)
  {
    if (data[i] == (unsigned char)' ' || data[i] == (unsigned char)'\t')
    {
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
  }
  for (i = 0; i < first_space; i++)
  {
    if (!aura_http_is_token(data[i]))
    {
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
  }
  if (data[first_space + 1] != (unsigned char)'/' ||
      second_space - first_space - 1 == 0)
  {
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }
  for (i = first_space + 1; i < second_space; i++)
  {
    unsigned char c = data[i];
    if (c < 0x21 || c == 0x7f)
    {
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
  }
  if (request_line_length - second_space - 1 != strlen("HTTP/1.1") ||
      memcmp(data + second_space + 1, "HTTP/1.1", strlen("HTTP/1.1")) != 0)
  {
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }

  parsed.method = aura_http_copy_string(data, first_space);
  parsed.target = aura_http_copy_string(data + first_space + 1,
                                        second_space - first_space - 1);
  parsed.version = aura_http_copy_string(data + second_space + 1,
                                         request_line_length - second_space - 1);
  if (parsed.method == NULL || parsed.target == NULL || parsed.version == NULL)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_ERROR;
  }
  method_allowed = strcmp(parsed.method, "GET") == 0 ||
                   strcmp(parsed.method, "HEAD") == 0 ||
                   strcmp(parsed.method, "POST") == 0;

  parsed.headers = (AuraHttpHeader *)calloc(AURA_HTTP_MAX_HEADERS,
                                             sizeof(*parsed.headers));
  if (parsed.headers == NULL)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_ERROR;
  }
  header_start = request_line_end;
  cursor = header_start;
  for (;;)
  {
    size_t line_end = 0;
    size_t line_content_end;
    size_t colon = SIZE_MAX;
    size_t name_length;
    size_t value_start;
    size_t value_end;
    size_t value_length;
    line_result = aura_http_find_line(data, input_length, cursor,
                                      AURA_HTTP_MAX_HEADER_BYTES, &line_end);
    if (line_result != AURA_HTTP_LINE_FOUND)
    {
      AuraHttpParseStatus status = aura_http_line_status(line_result);
      aura_http_request_destroy(&parsed);
      return status;
    }
    if (line_end - header_start > AURA_HTTP_MAX_HEADER_BYTES)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    line_content_end = line_end - 2;
    if (line_content_end == cursor)
    {
      header_end = line_end;
      break;
    }
    if (parsed.header_count == AURA_HTTP_MAX_HEADERS)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    for (i = cursor; i < line_content_end; i++)
    {
      if (data[i] == (unsigned char)':')
      {
        colon = i;
        break;
      }
    }
    if (colon == SIZE_MAX || colon == cursor)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    name_length = colon - cursor;
    for (i = cursor; i < colon; i++)
    {
      if (!aura_http_is_token(data[i]))
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
    }
    value_start = colon + 1;
    value_end = line_content_end;
    while (value_start < value_end &&
           (data[value_start] == (unsigned char)' ' ||
            data[value_start] == (unsigned char)'\t'))
    {
      value_start++;
    }
    while (value_end > value_start &&
           (data[value_end - 1] == (unsigned char)' ' ||
            data[value_end - 1] == (unsigned char)'\t'))
    {
      value_end--;
    }
    value_length = value_end - value_start;
    if (!aura_http_header_value_valid(data + value_start, value_length))
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    if (aura_http_header_name_equal(data + cursor, name_length,
                                    "Transfer-Encoding"))
    {
      if (chunked || !aura_http_ascii_equal_ci(data + value_start, value_length,
                                                "chunked"))
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      chunked = 1;
    }
    if (aura_http_header_name_equal(data + cursor, name_length, "Content-Length"))
    {
      size_t candidate = 0;
      if (!aura_http_parse_content_length(data + value_start, value_length,
                                          &candidate))
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      if (has_content_length && candidate != content_length)
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      has_content_length = 1;
      content_length = candidate;
    }
    parsed.headers[parsed.header_count].name =
        aura_http_copy_string(data + cursor, name_length);
    parsed.headers[parsed.header_count].value =
        aura_http_copy_string(data + value_start, value_length);
    if (parsed.headers[parsed.header_count].name == NULL ||
        parsed.headers[parsed.header_count].value == NULL)
    {
      parsed.header_count++;
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_ERROR;
    }
    parsed.header_count++;
    cursor = line_end;
  }

  if (chunked && has_content_length)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }
  if (!chunked && (content_length > AURA_HTTP_MAX_BODY_BYTES ||
                   header_end > AURA_HTTP_MAX_TOTAL_BYTES - content_length))
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
  }
  if (headers_only)
  {
    if (parsed.header_count == 0)
    {
      free(parsed.headers);
      parsed.headers = NULL;
    }
    else
    {
      AuraHttpHeader *shrunk = (AuraHttpHeader *)realloc(
          parsed.headers, parsed.header_count * sizeof(*parsed.headers));
      if (shrunk != NULL)
      {
        parsed.headers = shrunk;
      }
    }
    if (!method_allowed)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_METHOD_NOT_ALLOWED;
    }
    parsed.total_length = header_end;
    *out_request = parsed;
    if (out_consumed != NULL)
    {
      *out_consumed = header_end;
    }
    if (out_header_end != NULL)
    {
      *out_header_end = header_end;
    }
    if (out_content_length != NULL)
    {
      *out_content_length = content_length;
    }
    if (out_chunked != NULL)
    {
      *out_chunked = chunked;
    }
    return AURA_HTTP_PARSE_OK;
  }
  if (chunked)
  {
    AuraHttpParseStatus chunk_status = aura_http_decode_chunked_body(
        data, input_length, header_end, &parsed.body, &parsed.body_length,
        &parsed.total_length);
    if (chunk_status != AURA_HTTP_PARSE_OK)
    {
      aura_http_request_destroy(&parsed);
      return chunk_status;
    }
  }
  else
  {
    parsed.total_length = header_end + content_length;
    if (input_length < parsed.total_length)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_INCOMPLETE;
    }
    parsed.body_length = content_length;
    parsed.body = aura_http_copy_body(data + header_end, content_length);
    if (content_length != 0 && parsed.body == NULL)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_ERROR;
    }
  }
  if (parsed.header_count == 0)
  {
    free(parsed.headers);
    parsed.headers = NULL;
  }
  else
  {
    AuraHttpHeader *shrunk = (AuraHttpHeader *)realloc(
        parsed.headers, parsed.header_count * sizeof(*parsed.headers));
    if (shrunk != NULL)
    {
      parsed.headers = shrunk;
    }
  }
  if (!method_allowed)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_METHOD_NOT_ALLOWED;
  }
  *out_request = parsed;
  if (out_consumed != NULL)
  {
    *out_consumed = parsed.total_length;
  }
  return AURA_HTTP_PARSE_OK;
}

AuraHttpParseStatus aura_http_request_parse(const void *input, size_t input_length,
                                            AuraHttpRequest *out_request,
                                            size_t *out_consumed)
{
  return aura_http_request_parse_impl(input, input_length, out_request,
                                      out_consumed, 0, NULL, NULL, NULL);
}

/* Header-first parsing is the foundation for an async request-body reader.
 * It owns the same request metadata as the full parser but deliberately does
 * not consume or copy any body bytes. */
static AuraHttpParseStatus aura_http_request_parse_headers(
    const void *input, size_t input_length, AuraHttpRequest *out_request,
    size_t *out_header_end, size_t *out_content_length, int *out_chunked)
{
  return aura_http_request_parse_impl(input, input_length, out_request, NULL,
                                      1, out_header_end, out_content_length,
                                      out_chunked);
}

/* One Content-Length-framed body reader borrows the connection's unread
 * buffer. It never reads more than `remaining`, so bytes for a pipelined next
 * request remain untouched in the socket/buffer. The caller owns the output
 * buffer and waits for POLLIN when this returns AURA_TCP_PENDING. */
struct AuraHttpContentLengthReader
{
  AuraTcpStream *stream;
  unsigned char *buffer;
  size_t *used;
  size_t remaining;
  int timeout_ms;
  int read_active;
  int chunked;
  int chunk_state;
  unsigned char line[64];
  size_t line_length;
};

static int aura_http_content_length_reader_init(
    AuraHttpContentLengthReader *reader, AuraTcpStream *stream,
    unsigned char *buffer, size_t *used, size_t content_length, int timeout_ms)
{
  if (reader == NULL || stream == NULL || used == NULL ||
      (*used != 0 && buffer == NULL) || timeout_ms < 0)
  {
    return 0;
  }
  reader->stream = stream;
  reader->buffer = buffer;
  reader->used = used;
  reader->remaining = content_length;
  reader->timeout_ms = timeout_ms;
  reader->chunked = 0;
  reader->chunk_state = 0;
  reader->line_length = 0;
  return 1;
}

static int aura_http_chunked_reader_init(
    AuraHttpContentLengthReader *reader, AuraTcpStream *stream,
    unsigned char *buffer, size_t *used, int timeout_ms)
{
  if (reader == NULL || stream == NULL || used == NULL ||
      (*used != 0 && buffer == NULL) || timeout_ms < 0)
  {
    return 0;
  }
  memset(reader, 0, sizeof(*reader));
  reader->stream = stream;
  reader->buffer = buffer;
  reader->used = used;
  reader->timeout_ms = timeout_ms;
  reader->chunked = 1;
  return 1;
}

static AuraTcpStatus aura_http_body_reader_byte(
    AuraHttpContentLengthReader *reader, unsigned char *out)
{
  size_t count = 0;
  AuraTcpStatus status;
  if (reader == NULL || out == NULL || reader->stream == NULL ||
      reader->used == NULL || (*reader->used != 0 && reader->buffer == NULL))
  {
    return AURA_TCP_ERROR;
  }
  if (*reader->used != 0)
  {
    *out = reader->buffer[0];
    memmove(reader->buffer, reader->buffer + 1, *reader->used - 1);
    *reader->used -= 1;
    return AURA_TCP_OK;
  }
  status = aura_tcp_stream_read(reader->stream, out, 1, &count, 0);
  if (status == AURA_TCP_OK && count != 1)
  {
    return AURA_TCP_ERROR;
  }
  return status;
}

static int aura_http_hex_value(unsigned char value)
{
  if (value >= (unsigned char)'0' && value <= (unsigned char)'9')
  {
    return (int)(value - (unsigned char)'0');
  }
  if (value >= (unsigned char)'a' && value <= (unsigned char)'f')
  {
    return (int)(value - (unsigned char)'a') + 10;
  }
  if (value >= (unsigned char)'A' && value <= (unsigned char)'F')
  {
    return (int)(value - (unsigned char)'A') + 10;
  }
  return -1;
}

static AuraTcpStatus aura_http_chunked_reader_read(
    AuraHttpContentLengthReader *reader, unsigned char *out, size_t capacity,
    size_t *out_bytes)
{
  size_t count = 0;
  if (out_bytes == NULL || out == NULL || capacity == 0 || reader == NULL ||
      !reader->chunked)
  {
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  for (;;)
  {
    unsigned char byte = 0;
    AuraTcpStatus status;
    if (reader->chunk_state == 4)
    {
      return AURA_TCP_EOF;
    }
    if (reader->chunk_state == 0)
    {
      status = aura_http_body_reader_byte(reader, &byte);
      if (status != AURA_TCP_OK)
      {
        return status;
      }
      if (reader->line_length >= sizeof(reader->line))
      {
        return AURA_TCP_ERROR;
      }
      reader->line[reader->line_length++] = byte;
      if (reader->line_length >= 2 &&
          reader->line[reader->line_length - 2] == (unsigned char)'\r' &&
          reader->line[reader->line_length - 1] == (unsigned char)'\n')
      {
        size_t i;
        size_t digits = reader->line_length - 2;
        size_t size = 0;
        if (digits == 0 || digits > 16)
        {
          return AURA_TCP_ERROR;
        }
        for (i = 0; i < digits; i++)
        {
          int value = aura_http_hex_value(reader->line[i]);
          if (value < 0 || size > (SIZE_MAX - (size_t)value) / 16)
          {
            return AURA_TCP_ERROR;
          }
          size = size * 16 + (size_t)value;
        }
        reader->line_length = 0;
        reader->remaining = size;
        reader->chunk_state = size == 0 ? 3 : 1;
      }
      continue;
    }
    if (reader->chunk_state == 1)
    {
      size_t want = reader->remaining < capacity ? reader->remaining : capacity;
      if (*reader->used != 0)
      {
        count = *reader->used < want ? *reader->used : want;
        memcpy(out, reader->buffer, count);
        memmove(reader->buffer, reader->buffer + count, *reader->used - count);
        *reader->used -= count;
        status = AURA_TCP_OK;
      }
      else
      {
        status = aura_tcp_stream_read(reader->stream, out, want, &count, 0);
      }
      if (status != AURA_TCP_OK)
      {
        return status;
      }
      if (count == 0 || count > reader->remaining)
      {
        return AURA_TCP_ERROR;
      }
      reader->remaining -= count;
      *out_bytes = count;
      if (reader->remaining == 0)
      {
        reader->chunk_state = 2;
      }
      return AURA_TCP_OK;
    }
    if (reader->chunk_state == 2)
    {
      status = aura_http_body_reader_byte(reader, &byte);
      if (status != AURA_TCP_OK || byte != (unsigned char)'\r')
      {
        return AURA_TCP_ERROR;
      }
      status = aura_http_body_reader_byte(reader, &byte);
      if (status != AURA_TCP_OK || byte != (unsigned char)'\n')
      {
        return AURA_TCP_ERROR;
      }
      reader->chunk_state = 0;
      continue;
    }
    /* The alpha streaming reader consumes only an empty trailer section. */
    status = aura_http_body_reader_byte(reader, &byte);
    if (status != AURA_TCP_OK || byte != (unsigned char)'\r')
    {
      return AURA_TCP_ERROR;
    }
    status = aura_http_body_reader_byte(reader, &byte);
    if (status != AURA_TCP_OK || byte != (unsigned char)'\n')
    {
      return AURA_TCP_ERROR;
    }
    reader->chunk_state = 4;
    return AURA_TCP_EOF;
  }
}

static AuraTcpStatus aura_http_content_length_reader_read(
    AuraHttpContentLengthReader *reader, unsigned char *out, size_t capacity,
    size_t *out_bytes)
{
  size_t available;
  size_t count;
  AuraTcpStatus status;

  if (out_bytes == NULL || (out == NULL && capacity != 0) || reader == NULL ||
      reader->stream == NULL || reader->used == NULL ||
      (*reader->used != 0 && reader->buffer == NULL) || capacity == 0)
  {
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  if (reader->remaining == 0)
  {
    return AURA_TCP_EOF;
  }
  available = *reader->used;
  if (available != 0)
  {
    count = available;
    if (count > reader->remaining)
    {
      count = reader->remaining;
    }
    if (count > capacity)
    {
      count = capacity;
    }
    memcpy(out, reader->buffer, count);
    memmove(reader->buffer, reader->buffer + count, available - count);
    *reader->used = available - count;
    reader->remaining -= count;
    *out_bytes = count;
    return AURA_TCP_OK;
  }
  count = capacity < reader->remaining ? capacity : reader->remaining;
  status = aura_tcp_stream_read(reader->stream, out, count, &count, 0);
  if (status == AURA_TCP_OK)
  {
    if (count == 0 || count > reader->remaining)
    {
      return AURA_TCP_ERROR;
    }
    reader->remaining -= count;
    *out_bytes = count;
  }
  return status;
}

/* The reader is connection-owned and exists only while its handler runs. */
int aura_http_request_read_body(
    const AuraHttpRequest *request, unsigned char *out, size_t capacity,
    size_t *out_bytes)
{
  if (request == NULL || request->body_reader == NULL)
  {
    return AURA_TCP_ERROR;
  }
  if (request->body_reader->chunked)
  {
    return aura_http_chunked_reader_read(request->body_reader, out, capacity,
                                         out_bytes);
  }
  return aura_http_content_length_reader_read(request->body_reader, out,
                                              capacity, out_bytes);
}

int aura_http_request_body_read_begin(const AuraHttpRequest *request)
{
  AuraHttpContentLengthReader *reader;
  if (request == NULL || request->body_reader == NULL)
  {
    return 0;
  }
  reader = request->body_reader;
  if (reader->read_active)
  {
    return 0;
  }
  reader->read_active = 1;
  return 1;
}

void aura_http_request_body_read_end(const AuraHttpRequest *request)
{
  if (request != NULL && request->body_reader != NULL)
  {
    request->body_reader->read_active = 0;
  }
}

static int aura_http_body_reader_complete(
    const AuraHttpContentLengthReader *reader)
{
  if (reader == NULL)
  {
    return 0;
  }
  return reader->chunked ? reader->chunk_state == 4 : reader->remaining == 0;
}

/* ---- Bounded HTTP/1.1 response builder (transport-independent) ----
 *
 * The builder owns copies of caller-provided headers and body bytes.  It is
 * deliberately a final-response API: informational (1xx) responses,
 * transfer encoding, and caller-supplied Content-Length/Connection headers
 * are outside the alpha contract.  Content-Length and Connection are always
 * emitted by the serializer in a fixed order.
 */

#define AURA_HTTP_MAX_RESPONSE_HEADERS ((size_t)64)
#define AURA_HTTP_MAX_RESPONSE_HEADER_BYTES ((size_t)16 * 1024)
#define AURA_HTTP_MAX_RESPONSE_BODY_BYTES ((size_t)8 * 1024 * 1024)
#define AURA_HTTP_MAX_RESPONSE_BYTES ((size_t)16 * 1024 * 1024)

typedef enum
{
  AURA_HTTP_RESPONSE_OK = 0,
  AURA_HTTP_RESPONSE_INVALID = -1,
  AURA_HTTP_RESPONSE_TOO_LARGE = -2,
  AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL = -3,
  AURA_HTTP_RESPONSE_ALLOCATION = -4
} AuraHttpResponseStatus;

typedef enum
{
  AURA_HTTP_RESPONSE_CLOSE = 0,
  AURA_HTTP_RESPONSE_KEEP_ALIVE = 1
} AuraHttpResponseConnection;

typedef struct AuraHttpResponse
{
  int status_code;
  AuraHttpHeader *headers;
  size_t header_count;
  unsigned char *body;
  size_t body_length;
  AuraHttpResponseConnection connection;
  int streaming_committed;
} AuraHttpResponse;

static int aura_http_response_status_valid(int status_code)
{
  return status_code >= 200 && status_code <= 599;
}

static int aura_http_response_forbids_body(int status_code)
{
  return status_code == 204 || status_code == 304;
}

static const char *aura_http_response_reason(int status_code)
{
  switch (status_code)
  {
  case 200: return "OK";
  case 201: return "Created";
  case 202: return "Accepted";
  case 203: return "Non-Authoritative Information";
  case 204: return "No Content";
  case 205: return "Reset Content";
  case 206: return "Partial Content";
  case 300: return "Multiple Choices";
  case 301: return "Moved Permanently";
  case 302: return "Found";
  case 304: return "Not Modified";
  case 400: return "Bad Request";
  case 401: return "Unauthorized";
  case 403: return "Forbidden";
  case 404: return "Not Found";
  case 405: return "Method Not Allowed";
  case 408: return "Request Timeout";
  case 409: return "Conflict";
  case 413: return "Payload Too Large";
  case 415: return "Unsupported Media Type";
  case 429: return "Too Many Requests";
  case 500: return "Internal Server Error";
  case 501: return "Not Implemented";
  case 502: return "Bad Gateway";
  case 503: return "Service Unavailable";
  case 504: return "Gateway Timeout";
  default: return "Unknown";
  }
}

static int aura_http_response_size_add(size_t *total, size_t value)
{
  if (value > SIZE_MAX - *total)
  {
    return 0;
  }
  *total += value;
  return 1;
}

static int aura_http_response_header_bytes(const AuraHttpResponse *response,
                                           size_t *out_bytes)
{
  size_t i;
  size_t total = 0;
  if (response == NULL || out_bytes == NULL)
  {
    return 0;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (response->headers[i].name == NULL || response->headers[i].value == NULL)
    {
      return 0;
    }
    size_t name_length = strlen(response->headers[i].name);
    size_t value_length = strlen(response->headers[i].value);
    if (!aura_http_response_size_add(&total, name_length) ||
        !aura_http_response_size_add(&total, 2) ||
        !aura_http_response_size_add(&total, value_length) ||
        !aura_http_response_size_add(&total, 2))
    {
      return 0;
    }
  }
  *out_bytes = total;
  return 1;
}

static AuraHttpResponseStatus aura_http_response_validate(
    const AuraHttpResponse *response)
{
  size_t i;
  size_t header_bytes;
  if (response == NULL || !aura_http_response_status_valid(response->status_code) ||
      response->header_count > AURA_HTTP_MAX_RESPONSE_HEADERS - 2 ||
      (response->body == NULL && response->body_length != 0) ||
      response->body_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES ||
      (aura_http_response_forbids_body(response->status_code) &&
       response->body_length != 0) ||
      (response->connection != AURA_HTTP_RESPONSE_CLOSE &&
       response->connection != AURA_HTTP_RESPONSE_KEEP_ALIVE) ||
      (response->headers == NULL && response->header_count != 0))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  if (!aura_http_response_header_bytes(response, &header_bytes) ||
      header_bytes > AURA_HTTP_MAX_RESPONSE_HEADER_BYTES)
  {
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  for (i = 0; i < response->header_count; i++)
  {
    size_t j;
    const char *name = response->headers[i].name;
    const char *value = response->headers[i].value;
    if (name == NULL || value == NULL || name[0] == '\0' ||
        !aura_http_is_token((unsigned char)name[0]) ||
        !aura_http_header_value_valid((const unsigned char *)value, strlen(value)) ||
        aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                    "Content-Length") ||
        aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                    "Connection"))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
    for (j = 0; name[j] != '\0'; j++)
    {
      if (!aura_http_is_token((unsigned char)name[j]))
      {
        return AURA_HTTP_RESPONSE_INVALID;
      }
    }
    for (j = i + 1; j < response->header_count; j++)
    {
      if (response->headers[j].name != NULL &&
          aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                      response->headers[j].name))
      {
        return AURA_HTTP_RESPONSE_INVALID;
      }
    }
  }
  return AURA_HTTP_RESPONSE_OK;
}

void aura_http_response_init(AuraHttpResponse *response)
{
  if (response == NULL)
  {
    return;
  }
  memset(response, 0, sizeof(*response));
  response->status_code = 200;
  response->connection = AURA_HTTP_RESPONSE_CLOSE;
}

void aura_http_response_destroy(AuraHttpResponse *response)
{
  size_t i;
  if (response == NULL)
  {
    return;
  }
  for (i = 0; i < response->header_count; i++)
  {
    free(response->headers[i].name);
    free(response->headers[i].value);
  }
  free(response->headers);
  free(response->body);
  memset(response, 0, sizeof(*response));
}

int aura_http_response_status(const AuraHttpResponse *response)
{
  return response == NULL ? 0 : response->status_code;
}

size_t aura_http_response_header_count(const AuraHttpResponse *response)
{
  return response == NULL ? 0 : response->header_count;
}

const char *aura_http_response_header_name(const AuraHttpResponse *response,
                                           size_t index)
{
  if (response == NULL || index >= response->header_count)
  {
    return NULL;
  }
  return response->headers[index].name;
}

const char *aura_http_response_header_value(const AuraHttpResponse *response,
                                            size_t index)
{
  if (response == NULL || index >= response->header_count)
  {
    return NULL;
  }
  return response->headers[index].value;
}

const unsigned char *aura_http_response_body(const AuraHttpResponse *response)
{
  return response == NULL ? NULL : response->body;
}

size_t aura_http_response_body_length(const AuraHttpResponse *response)
{
  return response == NULL ? 0 : response->body_length;
}

int aura_http_response_keep_alive(const AuraHttpResponse *response)
{
  return response != NULL && response->connection == AURA_HTTP_RESPONSE_KEEP_ALIVE;
}

int aura_http_response_stream_started(const AuraHttpResponse *response)
{
  return response != NULL && response->streaming_committed;
}

AuraHttpResponseStatus aura_http_response_set_status(AuraHttpResponse *response,
                                                      int status_code)
{
  if (response == NULL || !aura_http_response_status_valid(status_code) ||
      response->streaming_committed ||
      (aura_http_response_forbids_body(status_code) && response->body_length != 0))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  response->status_code = status_code;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_set_connection(
    AuraHttpResponse *response, AuraHttpResponseConnection connection)
{
  if (response == NULL ||
      response->streaming_committed ||
      (connection != AURA_HTTP_RESPONSE_CLOSE &&
       connection != AURA_HTTP_RESPONSE_KEEP_ALIVE))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  response->connection = connection;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_add_header(AuraHttpResponse *response,
                                                      const char *name,
                                                      const char *value)
{
  size_t i;
  size_t header_bytes;
  AuraHttpHeader *grown;
  char *name_copy;
  char *value_copy;
  if (response == NULL || name == NULL || value == NULL || name[0] == '\0' ||
      response->streaming_committed ||
      response->header_count >= AURA_HTTP_MAX_RESPONSE_HEADERS - 2 ||
      !aura_http_is_token((unsigned char)name[0]))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  for (i = 0; name[i] != '\0'; i++)
  {
    if (!aura_http_is_token((unsigned char)name[i]))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
  }
  if (!aura_http_header_value_valid((const unsigned char *)value, strlen(value)) ||
      aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                  "Content-Length") ||
      aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                  "Connection"))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                    response->headers[i].name))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
  }
  if (!aura_http_response_header_bytes(response, &header_bytes) ||
      !aura_http_response_size_add(&header_bytes, strlen(name)) ||
      !aura_http_response_size_add(&header_bytes, 2) ||
      !aura_http_response_size_add(&header_bytes, strlen(value)) ||
      !aura_http_response_size_add(&header_bytes, 2) ||
      header_bytes > AURA_HTTP_MAX_RESPONSE_HEADER_BYTES)
  {
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  name_copy = aura_http_copy_string((const unsigned char *)name, strlen(name));
  value_copy = aura_http_copy_string((const unsigned char *)value, strlen(value));
  if (name_copy == NULL || value_copy == NULL)
  {
    free(name_copy);
    free(value_copy);
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  grown = (AuraHttpHeader *)realloc(response->headers,
                                    (response->header_count + 1) * sizeof(*grown));
  if (grown == NULL)
  {
    free(name_copy);
    free(value_copy);
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  response->headers = grown;
  response->headers[response->header_count].name = name_copy;
  response->headers[response->header_count].value = value_copy;
  response->header_count++;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_set_body(AuraHttpResponse *response,
                                                    const void *body,
                                                    size_t body_length)
{
  unsigned char *copy = NULL;
  if (response == NULL || (body == NULL && body_length != 0) ||
      response->streaming_committed ||
      body_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES ||
      (aura_http_response_forbids_body(response->status_code) && body_length != 0))
  {
    return body_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES
               ? AURA_HTTP_RESPONSE_TOO_LARGE
               : AURA_HTTP_RESPONSE_INVALID;
  }
  if (body_length != 0)
  {
    copy = (unsigned char *)malloc(body_length);
    if (copy == NULL)
    {
      return AURA_HTTP_RESPONSE_ALLOCATION;
    }
    memcpy(copy, body, body_length);
  }
  free(response->body);
  response->body = copy;
  response->body_length = body_length;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_set_error(AuraHttpResponse *response,
                                                     int status_code,
                                                     const char *error_code)
{
  char *body;
  int needed;
  AuraHttpResponseStatus result;
  if (response == NULL || error_code == NULL || error_code[0] == '\0' ||
      (status_code != 400 && status_code != 404 && status_code != 405 &&
       status_code != 408 && status_code != 413 && status_code != 500))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  for (size_t i = 0; error_code[i] != '\0'; i++)
  {
    if (!aura_http_is_token((unsigned char)error_code[i]))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
  }
  needed = snprintf(NULL, 0, "{\"error\":\"%s\"}", error_code);
  if (needed < 0)
  {
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  body = (char *)malloc((size_t)needed + 1);
  if (body == NULL)
  {
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  snprintf(body, (size_t)needed + 1, "{\"error\":\"%s\"}", error_code);
  result = aura_http_response_set_status(response, status_code);
  if (result == AURA_HTTP_RESPONSE_OK)
  {
    result = aura_http_response_set_connection(response, AURA_HTTP_RESPONSE_CLOSE);
  }
  if (result == AURA_HTTP_RESPONSE_OK)
  {
    result = aura_http_response_set_body(response, body, (size_t)needed);
  }
  free(body);
  if (result == AURA_HTTP_RESPONSE_OK)
  {
    result = aura_http_response_add_header(response, "Content-Type",
                                           "application/json");
  }
  return result;
}

static int aura_http_response_append(char *output, size_t capacity, size_t *cursor,
                                     const void *data, size_t length);

/* Streaming responses commit HTTP/1.1 headers before any body bytes. Their
 * chunk framing is owned by the connection task, never by application code. */
static int aura_http_response_has_transfer_encoding(const AuraHttpResponse *response)
{
  size_t i;
  for (i = 0; i < response->header_count; i++)
  {
    if (aura_http_header_name_equal(
            (const unsigned char *)response->headers[i].name,
            strlen(response->headers[i].name), "Transfer-Encoding"))
    {
      return 1;
    }
  }
  return 0;
}

int aura_http_response_stream_begin(
    AuraHttpResponse *response, void *output, size_t capacity, size_t *out_length)
{
  const char *reason;
  const char *connection;
  char status_line[64];
  size_t cursor = 0;
  size_t i;
  int status_size;

  if (response == NULL || out_length == NULL ||
      (output == NULL && capacity != 0) || response->streaming_committed ||
      response->body_length != 0 || aura_http_response_forbids_body(response->status_code) ||
      aura_http_response_has_transfer_encoding(response))
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  if (aura_http_response_validate(response) != AURA_HTTP_RESPONSE_OK)
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_INVALID;
  }
  reason = aura_http_response_reason(response->status_code);
  connection = response->connection == AURA_HTTP_RESPONSE_KEEP_ALIVE
                   ? "keep-alive" : "close";
  status_size = snprintf(status_line, sizeof(status_line), "HTTP/1.1 %d %s\r\n",
                         response->status_code, reason);
  if (status_size < 0 || (size_t)status_size >= sizeof(status_line) ||
      !aura_http_response_append((char *)output, capacity, &cursor, status_line,
                                 (size_t)status_size))
  {
    *out_length = cursor;
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (!aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].name,
                                   strlen(response->headers[i].name)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, ": ", 2) ||
        !aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].value,
                                   strlen(response->headers[i].value)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, "\r\n", 2))
    {
      *out_length = cursor;
      return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
    }
  }
  if (!aura_http_response_append((char *)output, capacity, &cursor,
                                 "Transfer-Encoding: chunked\r\n", 28) ||
      !aura_http_response_append((char *)output, capacity, &cursor,
                                 "Connection: ", 12) ||
      !aura_http_response_append((char *)output, capacity, &cursor, connection,
                                 strlen(connection)) ||
      !aura_http_response_append((char *)output, capacity, &cursor, "\r\n\r\n", 4))
  {
    *out_length = cursor;
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  *out_length = cursor;
  if (cursor > AURA_HTTP_MAX_RESPONSE_BYTES || output == NULL || capacity < cursor)
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  response->streaming_committed = 1;
  return AURA_HTTP_RESPONSE_OK;
}

int aura_http_response_stream_chunk(
    const void *chunk, size_t chunk_length, void *output, size_t capacity,
    size_t *out_length)
{
  char size_line[32];
  int size_length;
  size_t cursor = 0;
  if (out_length == NULL || (chunk == NULL && chunk_length != 0) ||
      (output == NULL && capacity != 0) || chunk_length == 0 ||
      chunk_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES)
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  size_length = snprintf(size_line, sizeof(size_line), "%zx\r\n", chunk_length);
  if (size_length < 0 || (size_t)size_length >= sizeof(size_line) ||
      !aura_http_response_append((char *)output, capacity, &cursor, size_line,
                                 (size_t)size_length) ||
      !aura_http_response_append((char *)output, capacity, &cursor, chunk,
                                 chunk_length) ||
      !aura_http_response_append((char *)output, capacity, &cursor, "\r\n", 2))
  {
    *out_length = cursor;
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  *out_length = cursor;
  return output == NULL || capacity < cursor ? AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL
                                               : AURA_HTTP_RESPONSE_OK;
}

int aura_http_response_stream_finish(
    const AuraHttpResponse *response, void *output, size_t capacity,
    size_t *out_length)
{
  static const char terminal[] = "0\r\n\r\n";
  if (out_length == NULL || response == NULL || !response->streaming_committed ||
      (output == NULL && capacity != 0))
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  *out_length = sizeof(terminal) - 1;
  if (output == NULL || capacity < sizeof(terminal) - 1)
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  memcpy(output, terminal, sizeof(terminal) - 1);
  return AURA_HTTP_RESPONSE_OK;
}

static int aura_http_response_append(char *output, size_t capacity, size_t *cursor,
                                     const void *data, size_t length)
{
  if (output != NULL)
  {
    if (*cursor > capacity || length > capacity - *cursor)
    {
      return 0;
    }
    if (length != 0)
    {
      memcpy(output + *cursor, data, length);
    }
  }
  *cursor += length;
  return 1;
}

AuraHttpResponseStatus aura_http_response_serialize(const AuraHttpResponse *response,
                                                     void *output, size_t capacity,
                                                     size_t *out_length)
{
  const char *reason;
  const char *connection;
  size_t required = 0;
  size_t i;
  size_t cursor = 0;
  char status_line[64];
  char content_length[64];
  int status_size;
  int length_size;
  if (out_length == NULL || (output == NULL && capacity != 0))
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  {
    AuraHttpResponseStatus validation = aura_http_response_validate(response);
    if (validation != AURA_HTTP_RESPONSE_OK)
    {
      *out_length = 0;
      return validation;
    }
  }
  reason = aura_http_response_reason(response->status_code);
  connection = response->connection == AURA_HTTP_RESPONSE_KEEP_ALIVE
                   ? "keep-alive" : "close";
  status_size = snprintf(status_line, sizeof(status_line), "HTTP/1.1 %d %s\r\n",
                         response->status_code, reason);
  length_size = snprintf(content_length, sizeof(content_length),
                         "Content-Length: %zu\r\n", response->body_length);
  if (status_size < 0 || length_size < 0 ||
      (size_t)status_size >= sizeof(status_line) ||
      (size_t)length_size >= sizeof(content_length))
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_INVALID;
  }
  if (!aura_http_response_size_add(&required, (size_t)status_size))
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (!aura_http_response_size_add(&required, strlen(response->headers[i].name)) ||
        !aura_http_response_size_add(&required, 2) ||
        !aura_http_response_size_add(&required, strlen(response->headers[i].value)) ||
        !aura_http_response_size_add(&required, 2))
    {
      *out_length = 0;
      return AURA_HTTP_RESPONSE_TOO_LARGE;
    }
  }
  if (!aura_http_response_size_add(&required, (size_t)length_size) ||
      !aura_http_response_size_add(&required, strlen("Connection: ")) ||
      !aura_http_response_size_add(&required, strlen(connection)) ||
      !aura_http_response_size_add(&required, 2) ||
      !aura_http_response_size_add(&required, 2) ||
      !aura_http_response_size_add(&required, response->body_length) ||
      required > AURA_HTTP_MAX_RESPONSE_BYTES)
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  *out_length = required;
  if (output == NULL || capacity < required)
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  if (!aura_http_response_append((char *)output, capacity, &cursor,
                                 status_line, (size_t)status_size))
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (!aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].name,
                                   strlen(response->headers[i].name)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, ": ", 2) ||
        !aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].value,
                                   strlen(response->headers[i].value)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, "\r\n", 2))
    {
      return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
    }
  }
  if (!aura_http_response_append((char *)output, capacity, &cursor,
                                 content_length, (size_t)length_size) ||
      !aura_http_response_append((char *)output, capacity, &cursor,
                                 "Connection: ", strlen("Connection: ")) ||
      !aura_http_response_append((char *)output, capacity, &cursor, connection,
                                 strlen(connection)) ||
      !aura_http_response_append((char *)output, capacity, &cursor, "\r\n\r\n", 4) ||
      !aura_http_response_append((char *)output, capacity, &cursor,
                                 response->body, response->body_length))
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  return AURA_HTTP_RESPONSE_OK;
}

/* ---- Bounded HTTP connection lifecycle (H4, synchronous alpha slice) ----
 *
 * This layer deliberately stops at a transport-backed, bounded request /
 * response loop.  The callback is an application-neutral response hook; it
 * is not a router and does not suspend.  H5 owns async integration, and H6
 * owns handler/path dispatch.
 */

typedef struct AuraHttpServer AuraHttpServer;
typedef struct AuraHttpConnection AuraHttpConnection;

typedef enum
{
  AURA_HTTP_CONNECTION_OK = 0,
  AURA_HTTP_CONNECTION_CLOSED = 1,
  AURA_HTTP_CONNECTION_TIMEOUT = 2,
  AURA_HTTP_CONNECTION_DISCONNECTED = 3,
  AURA_HTTP_CONNECTION_SHUTDOWN = 4,
  AURA_HTTP_CONNECTION_LIMIT = 5,
  AURA_HTTP_CONNECTION_ERROR = -1,
  AURA_HTTP_CONNECTION_UNSUPPORTED = -2
} AuraHttpConnectionStatus;

typedef enum
{
  AURA_HTTP_HANDLER_KEEP_ALIVE = 0,
  AURA_HTTP_HANDLER_CLOSE = 1,
  AURA_HTTP_HANDLER_ERROR = -1
} AuraHttpHandlerResult;

typedef AuraTaskPollState (*AuraHttpTaskHandler)(AuraTaskFrame *frame,
                                                  const AuraHttpRequest *request,
                                                  AuraHttpResponse *response,
                                                  void *user_data);

typedef struct
{
  size_t max_requests;
  int read_timeout_ms;
  int write_timeout_ms;
  int idle_timeout_ms;
} AuraHttpConnectionConfig;

typedef AuraHttpHandlerResult (*AuraHttpHandler)(const AuraHttpRequest *request,
                                                  AuraHttpResponse *response,
                                                  void *user_data);

typedef struct
{
  const char *method;
  const char *path;
  AuraHttpHandler handler;
  void *user_data;
} AuraHttpRoute;

/* Bounded synchronous route dispatch. The request and response remain owned
 * by the connection loop; handlers may only borrow them during this call and
 * may not suspend or transfer them across an async boundary. */
AuraHttpHandlerResult aura_http_dispatch_routes(const AuraHttpRequest *request,
                                                AuraHttpResponse *response,
                                                const AuraHttpRoute *routes,
                                                size_t route_count)
{
  int path_seen = 0;
  size_t i;
  if (request == NULL || response == NULL || routes == NULL ||
      request->method == NULL || request->target == NULL)
  {
    return AURA_HTTP_HANDLER_ERROR;
  }
  for (i = 0; i < route_count; i++)
  {
    if (routes[i].path == NULL || routes[i].method == NULL ||
        routes[i].handler == NULL || strcmp(routes[i].path, request->target) != 0)
    {
      continue;
    }
    path_seen = 1;
    if (strcmp(routes[i].method, request->method) != 0)
    {
      continue;
    }
    AuraHttpHandlerResult result =
        routes[i].handler(request, response, routes[i].user_data);
    if (result == AURA_HTTP_HANDLER_ERROR)
    {
      (void)aura_http_response_set_error(response, 500, "handler_failure");
      return AURA_HTTP_HANDLER_CLOSE;
    }
    return result;
  }
  if (aura_http_response_set_error(response, path_seen ? 405 : 404,
                                   path_seen ? "method_not_allowed" :
                                               "not_found") != AURA_HTTP_RESPONSE_OK)
  {
    return AURA_HTTP_HANDLER_ERROR;
  }
  return AURA_HTTP_HANDLER_CLOSE;
}

struct AuraHttpConnection
{
  AuraTcpStream *stream;
  AuraHttpServer *server;
  AuraHttpConnectionConfig config;
  size_t requests_served;
  int closed;
  unsigned char *async_buffer;
  size_t async_used;
  size_t async_capacity;
  AuraHttpResponse async_response;
  int async_response_active;
  char *async_output;
  size_t async_output_length;
  size_t async_output_offset;
  AuraHttpHandler async_handler;
  AuraHttpTaskHandler async_task_handler;
  void *async_user_data;
  int async_active;
  int async_phase;
  int async_close_after_write;
  AuraHttpRequest async_request;
  int async_request_active;
  AuraHttpContentLengthReader async_body_reader;
  int async_body_reader_active;
  int async_handler_started;
  AuraFfiHandlePin async_handle_pin;
  int async_handle_pin_active;
  AuraTaskFrame *async_handle_frame;
};

struct AuraHttpServer
{
  AuraTcpListener *listener;
  AuraHttpConnectionConfig config;
  size_t max_connections;
  size_t active_connections;
  int stopping;
};

int aura_http_connection_stream_write(AuraHttpConnection *connection,
                                      const void *data, size_t length,
                                      size_t *out_written)
{
  if (connection == NULL || connection->closed || connection->stream == NULL)
  {
    if (out_written != NULL)
    {
      *out_written = 0;
    }
    return AURA_TCP_CLOSED;
  }
  return aura_tcp_stream_write(connection->stream, data, length, out_written, 0);
}

static AuraHttpConnectionConfig aura_http_connection_default_config(void)
{
  AuraHttpConnectionConfig config;
  config.max_requests = 100;
  config.read_timeout_ms = 30000;
  config.write_timeout_ms = 30000;
  config.idle_timeout_ms = 30000;
  return config;
}

void aura_http_connection_config_init(AuraHttpConnectionConfig *config)
{
  if (config != NULL)
  {
    *config = aura_http_connection_default_config();
  }
}

static int aura_http_timeout_valid(int timeout_ms)
{
  return timeout_ms >= 0 && timeout_ms <= 30000;
}

static int aura_http_connection_config_valid(const AuraHttpConnectionConfig *config)
{
  return config != NULL && config->max_requests > 0 && config->max_requests <= 1024 &&
         aura_http_timeout_valid(config->read_timeout_ms) &&
         aura_http_timeout_valid(config->write_timeout_ms) &&
         aura_http_timeout_valid(config->idle_timeout_ms) &&
         config->idle_timeout_ms > 0;
}

static int aura_http_min_timeout(int first, int second)
{
  return first < second ? first : second;
}

static int aura_http_connection_header_has(const AuraHttpRequest *request,
                                           const char *token)
{
  const AuraHttpHeader *header = aura_http_request_find_header(request, "Connection");
  const char *value;
  size_t token_length;
  size_t i = 0;
  if (header == NULL || token == NULL)
  {
    return 0;
  }
  value = header->value;
  token_length = strlen(token);
  while (value[i] != '\0')
  {
    size_t start;
    size_t end;
    while (value[i] == ' ' || value[i] == '\t' || value[i] == ',')
    {
      i++;
    }
    start = i;
    while (value[i] != '\0' && value[i] != ',' && value[i] != ' ' &&
           value[i] != '\t')
    {
      i++;
    }
    end = i;
    if (end - start == token_length &&
        aura_http_ascii_equal_ci((const unsigned char *)value + start,
                                  end - start, token))
    {
      return 1;
    }
  }
  return 0;
}

static int aura_http_connection_write_all(AuraHttpConnection *connection,
                                          const unsigned char *data, size_t length)
{
  size_t sent = 0;
  while (sent < length)
  {
    size_t written = 0;
    AuraTcpStatus status = aura_tcp_stream_write(
        connection->stream, data + sent, length - sent, &written,
        connection->config.write_timeout_ms);
    if (status != AURA_TCP_OK || written == 0)
    {
      return status == AURA_TCP_TIMEOUT ? AURA_HTTP_CONNECTION_TIMEOUT
                                        : AURA_HTTP_CONNECTION_DISCONNECTED;
    }
    sent += written;
  }
  return AURA_HTTP_CONNECTION_OK;
}

static AuraHttpConnectionStatus aura_http_connection_send_error(
    AuraHttpConnection *connection, int status_code, const char *error_code)
{
  AuraHttpResponse response;
  AuraHttpResponseStatus response_status;
  size_t required = 0;
  char *serialized;
  AuraHttpConnectionStatus result;

  aura_http_response_init(&response);
  response_status = aura_http_response_set_error(&response, status_code, error_code);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    aura_http_response_destroy(&response);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  response_status = aura_http_response_serialize(&response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    aura_http_response_destroy(&response);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  serialized = (char *)malloc(required);
  if (serialized == NULL || aura_http_response_serialize(&response, serialized, required,
                                                          &required) != AURA_HTTP_RESPONSE_OK)
  {
    free(serialized);
    aura_http_response_destroy(&response);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  result = (AuraHttpConnectionStatus)aura_http_connection_write_all(
      connection, (const unsigned char *)serialized, required);
  free(serialized);
  aura_http_response_destroy(&response);
  return result;
}

static AuraHttpConnectionStatus aura_http_connection_send_response(
    AuraHttpConnection *connection, AuraHttpResponse *response)
{
  AuraHttpResponseStatus response_status;
  size_t required = 0;
  char *serialized;
  AuraHttpConnectionStatus result;

  response_status = aura_http_response_serialize(response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  serialized = (char *)malloc(required);
  if (serialized == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  response_status = aura_http_response_serialize(response, serialized, required, &required);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    free(serialized);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  result = (AuraHttpConnectionStatus)aura_http_connection_write_all(
      connection, (const unsigned char *)serialized, required);
  free(serialized);
  return result;
}

static void aura_http_connection_release_server(AuraHttpConnection *connection)
{
  if (connection->server != NULL && connection->server->active_connections > 0)
  {
    connection->server->active_connections--;
  }
  connection->server = NULL;
}

int aura_http_connection_close(AuraHttpConnection *connection)
{
  int changed;
  if (connection == NULL)
  {
    return 0;
  }
  changed = !connection->closed;
  if (!connection->closed)
  {
    connection->closed = 1;
    (void)aura_tcp_stream_close(connection->stream);
    aura_http_connection_release_server(connection);
  }
  return changed;
}

void aura_http_connection_destroy(AuraHttpConnection *connection)
{
  if (connection == NULL)
  {
    return;
  }
  (void)aura_http_connection_close(connection);
  aura_tcp_stream_destroy(connection->stream);
  free(connection);
}

void aura_http_connection_destroy_resource(void *resource)
{
  aura_http_connection_destroy((AuraHttpConnection *)resource);
}

AuraHttpConnectionStatus aura_http_connection_run(AuraHttpConnection *connection,
                                                   AuraHttpHandler handler,
                                                   void *user_data)
{
  unsigned char *buffer;
  size_t used = 0;
  AuraHttpConnectionStatus result = AURA_HTTP_CONNECTION_OK;

  if (connection == NULL || handler == NULL || connection->stream == NULL ||
      connection->closed)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  buffer = (unsigned char *)malloc(AURA_HTTP_MAX_TOTAL_BYTES);
  if (buffer == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  for (;;)
  {
    AuraHttpRequest request;
    size_t consumed = 0;
    AuraHttpParseStatus parse_status;
    while (1)
    {
      parse_status = aura_http_request_parse(buffer, used, &request, &consumed);
      if (parse_status != AURA_HTTP_PARSE_INCOMPLETE)
      {
        break;
      }
      if (used == AURA_HTTP_MAX_TOTAL_BYTES)
      {
        parse_status = AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
        break;
      }
      {
        size_t read = 0;
        int timeout = aura_http_min_timeout(connection->config.read_timeout_ms,
                                            connection->config.idle_timeout_ms);
        AuraTcpStatus read_status = aura_tcp_stream_read(connection->stream,
                                                          buffer + used,
                                                          AURA_HTTP_MAX_TOTAL_BYTES - used,
                                                          &read, timeout);
        if (read_status == AURA_TCP_TIMEOUT)
        {
          result = AURA_HTTP_CONNECTION_TIMEOUT;
          goto done;
        }
        if (read_status == AURA_TCP_EOF)
        {
          result = AURA_HTTP_CONNECTION_DISCONNECTED;
          goto done;
        }
        if (read_status != AURA_TCP_OK || read == 0)
        {
          result = AURA_HTTP_CONNECTION_DISCONNECTED;
          goto done;
        }
        used += read;
      }
    }

    if (parse_status == AURA_HTTP_PARSE_OK)
    {
      AuraHttpResponse response;
      AuraHttpHandlerResult handler_result;
      int request_close = aura_http_connection_header_has(&request, "close");
      int response_close;
      if (consumed == 0 || consumed > used)
      {
        aura_http_request_destroy(&request);
        result = AURA_HTTP_CONNECTION_ERROR;
        goto done;
      }
      memmove(buffer, buffer + consumed, used - consumed);
      used -= consumed;
      aura_http_response_init(&response);
      if (!request_close && aura_http_response_set_connection(
              &response, AURA_HTTP_RESPONSE_KEEP_ALIVE) != AURA_HTTP_RESPONSE_OK)
      {
        aura_http_request_destroy(&request);
        aura_http_response_destroy(&response);
        result = AURA_HTTP_CONNECTION_ERROR;
        goto done;
      }
      handler_result = handler(&request, &response, user_data);
      if (handler_result == AURA_HTTP_HANDLER_ERROR)
      {
        aura_http_response_destroy(&response);
        aura_http_response_init(&response);
        if (aura_http_response_set_error(&response, 500, "handler_failure") !=
            AURA_HTTP_RESPONSE_OK)
        {
          aura_http_request_destroy(&request);
          aura_http_response_destroy(&response);
          result = AURA_HTTP_CONNECTION_ERROR;
          goto done;
        }
      }
      if (handler_result == AURA_HTTP_HANDLER_CLOSE || request_close)
      {
        (void)aura_http_response_set_connection(&response, AURA_HTTP_RESPONSE_CLOSE);
      }
      if (connection->requests_served + 1 >= connection->config.max_requests)
      {
        (void)aura_http_response_set_connection(&response, AURA_HTTP_RESPONSE_CLOSE);
      }
      response_close = response.connection == AURA_HTTP_RESPONSE_CLOSE;
      result = aura_http_connection_send_response(connection, &response);
      aura_http_response_destroy(&response);
      aura_http_request_destroy(&request);
      if (result != AURA_HTTP_CONNECTION_OK)
      {
        goto done;
      }
      connection->requests_served++;
      if (response_close)
      {
        result = AURA_HTTP_CONNECTION_OK;
        goto done;
      }
      continue;
    }

    if (parse_status == AURA_HTTP_PARSE_INCOMPLETE)
    {
      result = AURA_HTTP_CONNECTION_ERROR;
      goto done;
    }
    {
      int status_code;
      const char *error_code;
      switch (parse_status)
      {
      case AURA_HTTP_PARSE_METHOD_NOT_ALLOWED:
        status_code = 405;
        error_code = "method_not_allowed";
        break;
      case AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE:
        status_code = 413;
        error_code = "payload_too_large";
        break;
      case AURA_HTTP_PARSE_BAD_REQUEST:
        status_code = 400;
        error_code = "bad_request";
        break;
      default:
        status_code = 500;
        error_code = "request_parse_failure";
        break;
      }
      (void)aura_http_connection_send_error(connection, status_code, error_code);
      result = AURA_HTTP_CONNECTION_OK;
      goto done;
    }
  }

done:
  free(buffer);
  (void)aura_http_connection_close(connection);
  return result;
}

AuraHttpConnectionStatus aura_http_server_create(
    AuraTcpListener *listener, size_t max_connections,
    const AuraHttpConnectionConfig *config, AuraHttpServer **out_server)
{
  AuraHttpServer *server;
  AuraHttpConnectionConfig defaults;
  if (out_server == NULL || listener == NULL || max_connections == 0 ||
      max_connections > 1024)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  defaults = aura_http_connection_default_config();
  if (config == NULL)
  {
    config = &defaults;
  }
  if (!aura_http_connection_config_valid(config))
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  server = (AuraHttpServer *)calloc(1, sizeof(*server));
  if (server == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  server->listener = listener;
  server->config = *config;
  server->max_connections = max_connections;
  *out_server = server;
  return AURA_HTTP_CONNECTION_OK;
}

AuraHttpConnectionStatus aura_http_server_accept(AuraHttpServer *server,
                                                  int timeout_ms,
                                                  AuraHttpConnection **out_connection)
{
  AuraTcpStream *stream = NULL;
  AuraHttpConnection *connection;
  AuraTcpStatus status;
  if (out_connection == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  *out_connection = NULL;
  if (server == NULL || server->stopping)
  {
    return AURA_HTTP_CONNECTION_SHUTDOWN;
  }
  if (server->active_connections >= server->max_connections)
  {
    return AURA_HTTP_CONNECTION_LIMIT;
  }
  status = aura_tcp_listener_accept(server->listener, timeout_ms, &stream);
  if (status == AURA_TCP_TIMEOUT || status == AURA_TCP_PENDING)
  {
    return AURA_HTTP_CONNECTION_TIMEOUT;
  }
  if (status == AURA_TCP_UNSUPPORTED)
  {
    return AURA_HTTP_CONNECTION_UNSUPPORTED;
  }
  if (status != AURA_TCP_OK || stream == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection = (AuraHttpConnection *)calloc(1, sizeof(*connection));
  if (connection == NULL)
  {
    aura_tcp_stream_destroy(stream);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection->stream = stream;
  connection->server = server;
  connection->config = server->config;
  server->active_connections++;
  *out_connection = connection;
  return AURA_HTTP_CONNECTION_OK;
}

/* Wrap an accepted stream after its std.net handle transfers ownership. */
AuraHttpConnectionStatus aura_http_connection_create_from_stream(
    AuraTcpStream *stream, const AuraHttpConnectionConfig *config,
    AuraHttpConnection **out_connection)
{
  AuraHttpConnection *connection;
  AuraHttpConnectionConfig defaults;
  if (out_connection == NULL || stream == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  *out_connection = NULL;
  defaults = aura_http_connection_default_config();
  if (config == NULL)
  {
    config = &defaults;
  }
  if (!aura_http_connection_config_valid(config))
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection = (AuraHttpConnection *)calloc(1, sizeof(*connection));
  if (connection == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection->stream = stream;
  connection->config = *config;
  *out_connection = connection;
  return AURA_HTTP_CONNECTION_OK;
}

int aura_http_server_shutdown(AuraHttpServer *server)
{
  if (server == NULL || server->stopping)
  {
    return 0;
  }
  server->stopping = 1;
  (void)aura_tcp_listener_close(server->listener);
  return 1;
}

size_t aura_http_server_active_connections(const AuraHttpServer *server)
{
  return server == NULL ? 0 : server->active_connections;
}

int aura_http_server_destroy(AuraHttpServer *server)
{
  if (server == NULL || server->active_connections != 0)
  {
    return 0;
  }
  aura_tcp_listener_destroy(server->listener);
  free(server);
  return 1;
}

/* Structured std.io wrappers use heap-owned String payloads for errors.  Keep
 * this helper separate from the throwing path: the throw path borrows the
 * static buffer above, while Result errors must survive the call boundary. */
char *aura_io_owned_error(const char *op, const char *path)
{
  const char *safe_op = op ? op : "io";
  const char *safe_path = path ? path : "(null)";
  const char *err = strerror(errno);
  if (err == NULL)
  {
    err = "unknown error";
  }
  int needed = snprintf(NULL, 0, "io %s failed: %s: %s", safe_op, safe_path, err);
  if (needed < 0)
  {
    return NULL;
  }
  char *message = (char *)malloc((size_t)needed + 1);
  if (message == NULL)
  {
    return NULL;
  }
  snprintf(message, (size_t)needed + 1, "io %s failed: %s: %s", safe_op, safe_path, err);
  return message;
}

void aura_io_owned_error_free(char *message)
{
  free(message);
}

static void aura_io_throw(const char *op, const char *path)
{
  const char *p = path ? path : "(null)";
  const char *err = strerror(errno);
  if (err == NULL)
  {
    err = "unknown error";
  }
  snprintf(aura_io_errbuf, sizeof(aura_io_errbuf), "io %s failed: %s: %s", op, p, err);
  aura_throw_string(aura_io_errbuf);
}

static void aura_io_throw_msg(const char *msg)
{
  snprintf(aura_io_errbuf, sizeof(aura_io_errbuf), "%s", msg ? msg : "io error");
  aura_throw_string(aura_io_errbuf);
}

bool aura_file_exists(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    return false;
  }
  struct stat st;
  return stat(path, &st) == 0 && S_ISREG(st.st_mode);
}

_Bool aura_fs_is_directory(const char *path)
{
  struct stat info;
  return path != NULL && stat(path, &info) == 0 && S_ISDIR(info.st_mode);
}

int64_t aura_fs_file_mode(const char *path)
{
  struct stat info;
  if (path == NULL || stat(path, &info) != 0) return 0;
  if (S_ISREG(info.st_mode)) return 1;
  if (S_ISDIR(info.st_mode)) return 2;
  return 3;
}

int64_t aura_fs_permissions(const char *path)
{
  struct stat info;
  if (path == NULL || stat(path, &info) != 0) return 0;
  return (int64_t)(info.st_mode & 0777);
}

int64_t aura_fs_modified_millis(const char *path)
{
  struct stat info;
  if (path == NULL || stat(path, &info) != 0) return -1;
  if (info.st_mtime > INT64_MAX / 1000) return -1;
  return (int64_t)info.st_mtime * 1000;
}

const char *aura_fs_list_names(const char *path)
{
#if defined(__unix__) || defined(__APPLE__)
  DIR *directory;
  struct dirent *entry;
  size_t used = 0;
  const size_t limit = 65536;
  char *out;
  if (path == NULL || (directory = opendir(path)) == NULL) return NULL;
  out = (char *)malloc(limit + 1);
  if (out == NULL) { closedir(directory); return NULL; }
  while ((entry = readdir(directory)) != NULL)
  {
    size_t length = strlen(entry->d_name);
    if (length > limit - used - (used == 0 ? 0 : 1))
    {
      free(out);
      closedir(directory);
      return NULL;
    }
    if (used != 0) out[used++] = '\n';
    memcpy(out + used, entry->d_name, length);
    used += length;
  }
  closedir(directory);
  out[used] = '\0';
  return out;
#else
  (void)path;
  return NULL;
#endif
}

_Bool aura_fs_is_symlink(const char *path)
{
#if defined(__unix__) || defined(__APPLE__)
  struct stat info;
  return path != NULL && lstat(path, &info) == 0 && S_ISLNK(info.st_mode);
#else
  (void)path;
  return false;
#endif
}

int64_t aura_file_size(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    errno = EINVAL;
    aura_io_throw("file_size", path);
  }
  struct stat st;
  if (stat(path, &st) != 0)
  {
    aura_io_throw("file_size", path);
  }
  if (!S_ISREG(st.st_mode))
  {
    errno = EISDIR;
    aura_io_throw("file_size", path);
  }
  return (int64_t)st.st_size;
}

const char *aura_read_file(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    errno = EINVAL;
    aura_io_throw("read_file", path);
  }
  FILE *f = fopen(path, "rb");
  if (f == NULL)
  {
    aura_io_throw("read_file", path);
  }
  if (fseek(f, 0, SEEK_END) != 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  long end = ftell(f);
  if (end < 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  if ((int64_t)end > AURA_IO_MAX_FILE)
  {
    fclose(f);
    aura_io_throw_msg("io read_file failed: file exceeds 256 MiB limit");
  }
  if (fseek(f, 0, SEEK_SET) != 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  size_t n = (size_t)end;
  char *buf = (char *)malloc(n + 1);
  if (buf == NULL)
  {
    fclose(f);
    aura_io_throw_msg("io read_file failed: out of memory");
  }
  size_t got = fread(buf, 1, n, f);
  if (got != n)
  {
    int e = ferror(f) ? errno : EIO;
    free(buf);
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  fclose(f);
  buf[n] = '\0';
  if (memchr(buf, '\0', n) != NULL)
  {
    free(buf);
    aura_io_throw_msg("io read_file failed: file contains embedded NUL (not a String)");
  }
  return buf;
}

/* C12p: soft read — same constraints as aura_read_file, but returns NULL on
 * missing path / I/O error / oversize / OOM / embedded NUL (never throws). */
const char *aura_try_read_file(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    return NULL;
  }
  FILE *f = fopen(path, "rb");
  if (f == NULL)
  {
    return NULL;
  }
  if (fseek(f, 0, SEEK_END) != 0)
  {
    fclose(f);
    return NULL;
  }
  long end = ftell(f);
  if (end < 0)
  {
    fclose(f);
    return NULL;
  }
  if ((int64_t)end > AURA_IO_MAX_FILE)
  {
    fclose(f);
    return NULL;
  }
  if (fseek(f, 0, SEEK_SET) != 0)
  {
    fclose(f);
    return NULL;
  }
  size_t n = (size_t)end;
  char *buf = (char *)malloc(n + 1);
  if (buf == NULL)
  {
    fclose(f);
    return NULL;
  }
  size_t got = fread(buf, 1, n, f);
  if (got != n)
  {
    free(buf);
    fclose(f);
    return NULL;
  }
  fclose(f);
  buf[n] = '\0';
  if (memchr(buf, '\0', n) != NULL)
  {
    free(buf);
    return NULL;
  }
  return buf;
}

static void aura_write_file_mode(const char *path, const char *content, const char *mode, const char *op)
{
  if (path == NULL || path[0] == '\0')
  {
    errno = EINVAL;
    aura_io_throw(op, path);
  }
  FILE *f = fopen(path, mode);
  if (f == NULL)
  {
    aura_io_throw(op, path);
  }
  const char *s = content ? content : "";
  size_t n = strlen(s);
  if (n > 0)
  {
    size_t wrote = fwrite(s, 1, n, f);
    if (wrote != n)
    {
      int e = errno;
      fclose(f);
      errno = e;
      aura_io_throw(op, path);
    }
  }
  if (fflush(f) != 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw(op, path);
  }
  if (fclose(f) != 0)
  {
    aura_io_throw(op, path);
  }
}

void aura_write_file(const char *path, const char *content)
{
  aura_write_file_mode(path, content, "wb", "write_file");
}

void aura_append_file(const char *path, const char *content)
{
  aura_write_file_mode(path, content, "ab", "append_file");
}

/* Soft write: true on success; false on empty path / open / write / flush / close fail.
 * Does not throw (unlike aura_write_file). */
bool aura_try_write_file(const char *path, const char *content)
{
  if (path == NULL || path[0] == '\0')
  {
    return false;
  }
  FILE *f = fopen(path, "wb");
  if (f == NULL)
  {
    return false;
  }
  const char *s = content ? content : "";
  size_t n = strlen(s);
  if (n > 0)
  {
    size_t wrote = fwrite(s, 1, n, f);
    if (wrote != n)
    {
      fclose(f);
      return false;
    }
  }
  if (fflush(f) != 0)
  {
    fclose(f);
    return false;
  }
  if (fclose(f) != 0)
  {
    return false;
  }
  return true;
}

/* ---- Stdin (std.io.readLine / readAllStdin) ----
 * readLine: one line without trailing \n or \r\n; NULL on EOF; empty line is "".
 * Oversized line / whole-stdin throws String (MVP caps).
 */

#define AURA_IO_MAX_LINE ((int64_t)1 * 1024 * 1024)

const char *aura_read_line(void)
{
  size_t cap = 128;
  size_t n = 0;
  char *buf = (char *)malloc(cap);
  if (buf == NULL)
  {
    aura_io_throw_msg("io read_line failed: out of memory");
  }
  int c = EOF;
  for (;;)
  {
    c = fgetc(stdin);
    if (c == EOF)
    {
      break;
    }
    if (c == '\n')
    {
      break;
    }
    /* Treat \r or \r\n as end of line (strip CR). */
    if (c == '\r')
    {
      int next = fgetc(stdin);
      if (next != '\n' && next != EOF)
      {
        ungetc(next, stdin);
      }
      break;
    }
    if ((int64_t)n >= AURA_IO_MAX_LINE)
    {
      free(buf);
      aura_io_throw_msg("io read_line failed: line exceeds 1 MiB limit");
    }
    if (n + 1 >= cap)
    {
      size_t ncap = cap * 2;
      if ((int64_t)ncap > AURA_IO_MAX_LINE + 1)
      {
        ncap = (size_t)AURA_IO_MAX_LINE + 1;
      }
      char *nb = (char *)realloc(buf, ncap);
      if (nb == NULL)
      {
        free(buf);
        aura_io_throw_msg("io read_line failed: out of memory");
      }
      buf = nb;
      cap = ncap;
    }
    buf[n++] = (char)c;
  }
  if (ferror(stdin))
  {
    free(buf);
    aura_io_throw_msg("io read_line failed: stdin read error");
  }
  /* Immediate EOF with no bytes → null (String?). */
  if (c == EOF && n == 0)
  {
    free(buf);
    return NULL;
  }
  buf[n] = '\0';
  return buf;
}

const char *aura_read_all_stdin(void)
{
  size_t cap = 4096;
  size_t n = 0;
  char *buf = (char *)malloc(cap);
  if (buf == NULL)
  {
    aura_io_throw_msg("io read_all_stdin failed: out of memory");
  }
  for (;;)
  {
    if (n + 1 >= cap)
    {
      if ((int64_t)cap >= AURA_IO_MAX_FILE)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
      }
      size_t ncap = cap * 2;
      if ((int64_t)ncap > AURA_IO_MAX_FILE)
      {
        ncap = (size_t)AURA_IO_MAX_FILE;
      }
      if (ncap <= cap)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
      }
      char *nb = (char *)realloc(buf, ncap);
      if (nb == NULL)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: out of memory");
      }
      buf = nb;
      cap = ncap;
    }
    size_t want = cap - n - 1; /* leave room for NUL */
    if (want == 0)
    {
      free(buf);
      aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
    }
    size_t got = fread(buf + n, 1, want, stdin);
    n += got;
    if (got < want)
    {
      if (ferror(stdin))
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: stdin read error");
      }
      break; /* EOF */
    }
    if ((int64_t)n >= AURA_IO_MAX_FILE)
    {
      int extra = fgetc(stdin);
      if (extra != EOF)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
      }
      if (ferror(stdin))
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: stdin read error");
      }
      break;
    }
  }
  if (memchr(buf, '\0', n) != NULL)
  {
    free(buf);
    aura_io_throw_msg("io read_all_stdin failed: input contains embedded NUL (not a String)");
  }
  buf[n] = '\0';
  return buf;
}

void aura_assert(bool cond)
{
  if (!cond)
  {
    aura_throw_string("assertion failed");
  }
}

void aura_assert_eq_int(int64_t a, int64_t b)
{
  if (a != b)
  {
    aura_throw_string("assert_eq failed (Int)");
  }
}

void aura_assert_eq_string(const char *a, const char *b)
{
  if (a == NULL && b == NULL)
  {
    return;
  }
  if (a == NULL || b == NULL || strcmp(a, b) != 0)
  {
    aura_throw_string("assert_eq failed (String)");
  }
}

void aura_assert_eq_bool(bool a, bool b)
{
  if (a != b)
  {
    aura_throw_string("assert_eq failed (Bool)");
  }
}

/* ---- Unchecked exceptions (setjmp / longjmp) ---- */

#define AURA_EX_MAX 64

typedef struct AuraExCause AuraExCause;

struct AuraExCause
{
  char *type_name;
  uint32_t source_span_start;
  uint32_t source_span_end;
  AuraExCause *next;
};

typedef struct
{
  jmp_buf *buf;
  const char *type_name; /* "String" | "Int" | "Bool" | class name */
  uint32_t source_span_start;
  uint32_t source_span_end;
  int owns_obj;          /* payload.as_obj is owned by the exception frame */
  void (*destroy_obj)(void *);
  AuraExCause *cause_head;
  AuraExCause *cause_tail;
  union
  {
    const char *as_string;
    int64_t as_int;
    bool as_bool;
    void *as_obj; /* heap copy of class/struct value (C3g) */
  } payload;
} AuraExFrame;

static AuraExFrame aura_ex_stack[AURA_EX_MAX];
static int aura_ex_sp = 0;
static int aura_ex_pending = 0;
static uint32_t aura_ex_unhandled_span_start = 0;
static uint32_t aura_ex_unhandled_span_end = 0;
static AuraExCause *aura_ex_cleared_causes_head = NULL;
static AuraExCause *aura_ex_cleared_causes_tail = NULL;
static uint32_t aura_ex_cleared_span_start = 0;
static uint32_t aura_ex_cleared_span_end = 0;

static char *aura_ex_copy_string(const char *text)
{
  size_t len;
  char *copy;
  if (text == NULL)
  {
    return NULL;
  }
  len = strlen(text);
  copy = (char *)malloc(len + 1);
  if (copy != NULL)
  {
    memcpy(copy, text, len + 1);
  }
  return copy;
}

static void aura_ex_dispose_causes(AuraExFrame *f)
{
  AuraExCause *cause;
  AuraExCause *next;
  if (f == NULL)
  {
    return;
  }
  cause = f->cause_head;
  f->cause_head = NULL;
  f->cause_tail = NULL;
  while (cause != NULL)
  {
    next = cause->next;
    free(cause->type_name);
    free(cause);
    cause = next;
  }
}

static void aura_ex_dispose_cleared_causes(void)
{
  AuraExCause *cause = aura_ex_cleared_causes_head;
  aura_ex_cleared_causes_head = NULL;
  aura_ex_cleared_causes_tail = NULL;
  while (cause != NULL)
  {
    AuraExCause *next = cause->next;
    free(cause->type_name);
    free(cause);
    cause = next;
  }
}

static AuraExCause *aura_ex_query_causes(void)
{
  if (aura_ex_sp > 0 && aura_ex_stack[aura_ex_sp - 1].cause_head != NULL)
  {
    return aura_ex_stack[aura_ex_sp - 1].cause_head;
  }
  return aura_ex_cleared_causes_head;
}

/* Compiler-generated throws set this before transferring control. Runtime
 * helpers leave it at zero, preserving a stable unknown location. */
void aura_ex_set_source_span(uint32_t start, uint32_t end)
{
  aura_ex_unhandled_span_start = start;
  aura_ex_unhandled_span_end = end;
  if (aura_ex_sp > 0)
  {
    aura_ex_stack[aura_ex_sp - 1].source_span_start = start;
    aura_ex_stack[aura_ex_sp - 1].source_span_end = end;
  }
}

uint32_t aura_ex_source_span_start(void)
{
  if (aura_ex_sp > 0 && aura_ex_stack[aura_ex_sp - 1].source_span_end != 0)
  {
    return aura_ex_stack[aura_ex_sp - 1].source_span_start;
  }
  return aura_ex_cleared_span_start;
}

uint32_t aura_ex_source_span_end(void)
{
  if (aura_ex_sp > 0 && aura_ex_stack[aura_ex_sp - 1].source_span_end != 0)
  {
    return aura_ex_stack[aura_ex_sp - 1].source_span_end;
  }
  return aura_ex_cleared_span_end;
}

void aura_throw_obj_with_destructor(const char *type_name, void *obj,
                                    void (*destroy_obj)(void *));

static void aura_ex_dispose_frame(AuraExFrame *f)
{
  if (f == NULL)
  {
    return;
  }
  if (f->owns_obj && f->payload.as_obj != NULL)
  {
    if (f->destroy_obj != NULL)
    {
      f->destroy_obj(f->payload.as_obj);
    }
    else
    {
      free(f->payload.as_obj);
    }
    f->payload.as_obj = NULL;
  }
  aura_ex_dispose_causes(f);
  f->owns_obj = 0;
  f->destroy_obj = NULL;
  f->type_name = NULL;
}

/* An uncaught object still owns its payload until the process terminates.
 * Dispose it before aborting so custom destructors release nested resources
 * even when there is no catch frame to perform the final cleanup. */
static void aura_ex_abort_uncaught(const char *type_name, void *obj,
                                   void (*destroy_obj)(void *),
                                   uint32_t source_span_start,
                                   uint32_t source_span_end,
                                   AuraExCause *causes)
{
  if (source_span_end > source_span_start)
  {
    fprintf(stderr, "uncaught exception (%s) at source span [%u,%u)\n",
            type_name ? type_name : "object", source_span_start,
            source_span_end);
  }
  else
  {
    fprintf(stderr, "uncaught exception (%s)\n",
            type_name ? type_name : "object");
  }
  for (AuraExCause *cause = causes; cause != NULL; cause = cause->next)
  {
    if (cause->source_span_end > cause->source_span_start)
    {
      fprintf(stderr, "  caused by (%s) at source span [%u,%u)\n",
              cause->type_name ? cause->type_name : "exception",
              cause->source_span_start, cause->source_span_end);
    }
    else
    {
      fprintf(stderr, "  caused by (%s)\n",
              cause->type_name ? cause->type_name : "exception");
    }
  }
  if (obj != NULL)
  {
    if (destroy_obj != NULL)
    {
      destroy_obj(obj);
    }
    else
    {
      free(obj);
    }
  }
  while (causes != NULL)
  {
    AuraExCause *next = causes->next;
    free(causes->type_name);
    free(causes);
    causes = next;
  }
  abort();
}

static void aura_ex_replace_payload(AuraExFrame *f)
{
  aura_ex_dispose_frame(f);
  if (f != NULL)
  {
    f->payload.as_obj = NULL;
  }
}

void aura_try_enter(jmp_buf *buf)
{
  if (aura_ex_sp >= AURA_EX_MAX)
  {
    fputs("aura: exception stack overflow\n", stderr);
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp++];
  aura_ex_dispose_cleared_causes();
  aura_ex_cleared_span_start = 0;
  aura_ex_cleared_span_end = 0;
  f->buf = buf;
  f->type_name = NULL;
  f->source_span_start = 0;
  f->source_span_end = 0;
  f->cause_head = NULL;
  f->cause_tail = NULL;
  f->owns_obj = 0;
  f->destroy_obj = NULL;
  f->payload.as_obj = NULL;
}

void aura_try_leave(void)
{
  if (aura_ex_sp > 0)
  {
    /* Leaving a catch is the final ownership boundary when the caller did
     * not explicitly clear the payload. Generated catches still clear first. */
    aura_ex_dispose_frame(&aura_ex_stack[aura_ex_sp - 1]);
    aura_ex_sp--;
    if (aura_ex_sp == 0)
    {
      aura_ex_pending = 0;
    }
  }
}

void aura_throw_string(const char *s)
{
  if (aura_ex_sp == 0)
  {
    fprintf(stderr, "uncaught exception: %s\n", s ? s : "null");
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = "String";
  f->owns_obj = 0;
  f->payload.as_string = s;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

void aura_throw_int(int64_t v)
{
  if (aura_ex_sp == 0)
  {
    fprintf(stderr, "uncaught exception: Int(%lld)\n", (long long)v);
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = "Int";
  f->owns_obj = 0;
  f->payload.as_int = v;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

void aura_throw_bool(bool v)
{
  if (aura_ex_sp == 0)
  {
    fprintf(stderr, "uncaught exception: Bool(%s)\n", v ? "true" : "false");
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = "Bool";
  f->owns_obj = 0;
  f->payload.as_bool = v;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

/* Throw a class/struct instance with the legacy malloc ownership contract. */
void aura_throw_obj(const char *type_name, void *obj)
{
  aura_throw_obj_with_destructor(type_name, obj, free);
}

/* Throw a class/struct instance and transfer its complete ownership to the
 * exception frame.  The destructor is invoked exactly once by clear, the
 * final try_leave, or after ownership is transferred by rethrow.  This is
 * required for payloads containing owned runtime resources (for example a
 * heap-backed String field), where a shallow free(obj) is insufficient. */
void aura_throw_obj_with_destructor(const char *type_name, void *obj,
                                    void (*destroy_obj)(void *))
{
  if (aura_ex_sp == 0)
  {
    aura_ex_abort_uncaught(type_name, obj,
                           destroy_obj != NULL ? destroy_obj : free,
                           aura_ex_unhandled_span_start,
                           aura_ex_unhandled_span_end, NULL);
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = type_name;
  f->owns_obj = 1;
  f->destroy_obj = destroy_obj != NULL ? destroy_obj : free;
  f->payload.as_obj = obj;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

int aura_ex_add_cause(const char *type_name, uint32_t source_span_start,
                      uint32_t source_span_end)
{
  AuraExCause *cause;
  AuraExCause **head;
  AuraExCause **tail;
  if (type_name == NULL)
  {
    return 0;
  }
  cause = (AuraExCause *)calloc(1, sizeof(*cause));
  if (cause == NULL)
  {
    return 0;
  }
  cause->type_name = aura_ex_copy_string(type_name);
  if (cause->type_name == NULL)
  {
    free(cause);
    return 0;
  }
  cause->source_span_start = source_span_start;
  cause->source_span_end = source_span_end;
  if (aura_ex_sp > 0)
  {
    head = &aura_ex_stack[aura_ex_sp - 1].cause_head;
    tail = &aura_ex_stack[aura_ex_sp - 1].cause_tail;
  }
  else
  {
    head = &aura_ex_cleared_causes_head;
    tail = &aura_ex_cleared_causes_tail;
  }
  if (*tail == NULL)
  {
    *head = cause;
  }
  else
  {
    (*tail)->next = cause;
  }
  *tail = cause;
  return 1;
}

size_t aura_ex_cause_count(void)
{
  size_t count = 0;
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      count++;
    }
  }
  return count;
}

const char *aura_ex_cause_type(size_t index)
{
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    size_t current = 0;
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      if (current++ == index)
      {
        return cause->type_name;
      }
    }
  }
  return NULL;
}

uint32_t aura_ex_cause_span_start(size_t index)
{
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    size_t current = 0;
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      if (current++ == index)
      {
        return cause->source_span_start;
      }
    }
  }
  return 0;
}

const char *aura_ex_cause_type_copy(size_t index)
{
  return aura_ex_copy_string(aura_ex_cause_type(index));
}

uint32_t aura_ex_cause_span_end(size_t index)
{
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    size_t current = 0;
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      if (current++ == index)
      {
        return cause->source_span_end;
      }
    }
  }
  return 0;
}

int aura_ex_matches(const char *type_name)
{
  if (aura_ex_sp == 0 || !aura_ex_pending)
  {
    return 0;
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  return f->type_name && type_name && strcmp(f->type_name, type_name) == 0;
}

const char *aura_ex_type_name(void)
{
  if (aura_ex_sp == 0 || !aura_ex_pending)
  {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].type_name;
}

const char *aura_ex_as_string(void)
{
  if (aura_ex_sp == 0)
  {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_string;
}

int64_t aura_ex_as_int(void)
{
  if (aura_ex_sp == 0)
  {
    return 0;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_int;
}

bool aura_ex_as_bool(void)
{
  if (aura_ex_sp == 0)
  {
    return false;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_bool;
}

void *aura_ex_as_obj(void)
{
  if (aura_ex_sp == 0)
  {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_obj;
}

/* Transfer an object payload out of the active exception frame. */
void *aura_ex_take_obj(void)
{
  AuraExFrame *f;
  void *obj;

  if (aura_ex_sp == 0 || !aura_ex_pending)
  {
    return NULL;
  }
  f = &aura_ex_stack[aura_ex_sp - 1];
  if (!f->owns_obj)
  {
    return NULL;
  }
  obj = f->payload.as_obj;
  f->payload.as_obj = NULL;
  f->owns_obj = 0;
  f->destroy_obj = NULL;
  return obj;
}

void aura_ex_clear(void)
{
  if (aura_ex_sp > 0)
  {
    AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
    /* Keep causes queryable for the catch body after its frame is left. */
    aura_ex_dispose_cleared_causes();
    aura_ex_cleared_span_start = f->source_span_start;
    aura_ex_cleared_span_end = f->source_span_end;
    aura_ex_cleared_causes_head = f->cause_head;
    aura_ex_cleared_causes_tail = f->cause_tail;
    f->cause_head = NULL;
    f->cause_tail = NULL;
    if (f->owns_obj && f->payload.as_obj != NULL)
    {
      if (f->destroy_obj != NULL)
      {
        f->destroy_obj(f->payload.as_obj);
      }
      else
      {
        free(f->payload.as_obj);
      }
      f->payload.as_obj = NULL;
    }
    f->owns_obj = 0;
    f->destroy_obj = NULL;
    f->type_name = NULL;
  }
  aura_ex_pending = 0;
}

void aura_ex_rethrow(void)
{
  if (!aura_ex_pending || aura_ex_sp == 0)
  {
    abort();
  }
  aura_ex_dispose_cleared_causes();
  /* Pop current frame and longjmp to outer, or uncaught. */
  AuraExFrame cur = aura_ex_stack[aura_ex_sp - 1];
  aura_ex_sp--;
  if (aura_ex_sp == 0)
  {
    aura_ex_abort_uncaught(cur.type_name,
                           cur.owns_obj ? cur.payload.as_obj : NULL,
                           cur.owns_obj ? cur.destroy_obj : NULL,
                           cur.source_span_start, cur.source_span_end,
                           cur.cause_head);
  }
  AuraExFrame *outer = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(outer);
  outer->type_name = cur.type_name;
  outer->source_span_start = cur.source_span_start;
  outer->source_span_end = cur.source_span_end;
  outer->owns_obj = cur.owns_obj;
  outer->destroy_obj = cur.destroy_obj;
  outer->payload = cur.payload;
  outer->cause_head = cur.cause_head;
  outer->cause_tail = cur.cause_tail;
  cur.cause_head = NULL;
  cur.cause_tail = NULL;
  longjmp(*outer->buf, 1);
}

/* ---- GC (C3x free-all + C4z roots + C5f mark/sweep + C6a deep mark + C6e/C7b) ----
 * aura_gc_collect: if roots registered → mark from roots and Array-of-class
 * buffers (C6e), then deep-scan object bodies for nested GC pointers
 * (conservative pointer slots) + per-object mark_extras (C7b Array fields)
 * + sweep unmarked (C7b: dtor frees owned Array buffers). If no roots →
 * mark-all (safe until compiler emits roots). Shutdown still free-all remaining.
 */

typedef struct AuraGcNode
{
  void *ptr;
  size_t size;                    /* C6a: payload size for deep field scan */
  int marked;                     /* C4z: mark bit for STW collect */
  void (*dtor)(void *ptr);        /* C7b: free non-GC field buffers before free */
  void (*mark_extras)(void *ptr); /* C7b: mark Array-of-class field elems */
  struct AuraGcNode *next;
} AuraGcNode;

static AuraGcNode *aura_gc_list = NULL;

#if defined(__unix__) || defined(__APPLE__)
static pthread_once_t aura_gc_lock_once = PTHREAD_ONCE_INIT;
static pthread_mutex_t aura_gc_lock;

static void aura_gc_lock_init(void)
{
  pthread_mutexattr_t attributes;
  pthread_mutexattr_init(&attributes);
  pthread_mutexattr_settype(&attributes, PTHREAD_MUTEX_RECURSIVE);
  pthread_mutex_init(&aura_gc_lock, &attributes);
  pthread_mutexattr_destroy(&attributes);
}

static void aura_gc_lock_enter(void)
{
  pthread_once(&aura_gc_lock_once, aura_gc_lock_init);
  pthread_mutex_lock(&aura_gc_lock);
}

static void aura_gc_lock_leave(void)
{
  pthread_mutex_unlock(&aura_gc_lock);
}
#else
static void aura_gc_lock_enter(void) {}
static void aura_gc_lock_leave(void) {}
#endif

/* Conservative root slots: pointers to variables that hold GC pointers. */
#define AURA_GC_MAX_ROOTS 256
static void **aura_gc_roots[AURA_GC_MAX_ROOTS];
static int aura_gc_root_n = 0;

/* C6e: Array-of-class locals — scan .data[0..len) as GC pointer slots.
 * data_slot points at the Array.data field; len_slot at Array.len. */
typedef struct
{
  void **data_slot;
  int64_t *len_slot;
} AuraGcArrayRoot;

#define AURA_GC_MAX_ARRAY_ROOTS 256
static AuraGcArrayRoot aura_gc_array_roots[AURA_GC_MAX_ARRAY_ROOTS];
static int aura_gc_array_root_n = 0;

/* Worklist for deep mark (C6a). */
#define AURA_GC_MARK_STACK 1024
static AuraGcNode *aura_gc_mark_stack[AURA_GC_MARK_STACK];
static int aura_gc_mark_sp = 0;

void aura_gc_add_root(void **slot)
{
  aura_gc_lock_enter();
  if (slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    if (aura_gc_roots[i] == slot)
    {
      aura_gc_lock_leave();
      return;
    }
  }
  if (aura_gc_root_n >= AURA_GC_MAX_ROOTS)
  {
    fputs("aura: GC root table full\n", stderr);
    abort();
  }
  aura_gc_roots[aura_gc_root_n++] = slot;
  aura_gc_lock_leave();
}

void aura_gc_remove_root(void **slot)
{
  aura_gc_lock_enter();
  if (slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    if (aura_gc_roots[i] == slot)
    {
      aura_gc_roots[i] = aura_gc_roots[aura_gc_root_n - 1];
      aura_gc_root_n--;
      aura_gc_lock_leave();
      return;
    }
  }
  aura_gc_lock_leave();
}

/* C6e: register Array.data / Array.len so collect marks element GC pointers. */
void aura_gc_add_array_root(void **data_slot, int64_t *len_slot)
{
  aura_gc_lock_enter();
  if (data_slot == NULL || len_slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    if (aura_gc_array_roots[i].data_slot == data_slot)
    {
      aura_gc_array_roots[i].len_slot = len_slot;
      aura_gc_lock_leave();
      return;
    }
  }
  if (aura_gc_array_root_n >= AURA_GC_MAX_ARRAY_ROOTS)
  {
    fputs("aura: GC array root table full\n", stderr);
    abort();
  }
  aura_gc_array_roots[aura_gc_array_root_n].data_slot = data_slot;
  aura_gc_array_roots[aura_gc_array_root_n].len_slot = len_slot;
  aura_gc_array_root_n++;
  aura_gc_lock_leave();
}

void aura_gc_remove_array_root(void **data_slot)
{
  aura_gc_lock_enter();
  if (data_slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    if (aura_gc_array_roots[i].data_slot == data_slot)
    {
      aura_gc_array_roots[i] = aura_gc_array_roots[aura_gc_array_root_n - 1];
      aura_gc_array_root_n--;
      aura_gc_lock_leave();
      return;
    }
  }
  aura_gc_lock_leave();
}

static AuraGcNode *aura_gc_find(void *ptr)
{
  for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
  {
    if (n->ptr == ptr)
    {
      return n;
    }
  }
  return NULL;
}

static void aura_gc_mark_push(AuraGcNode *n)
{
  if (n == NULL || n->marked)
  {
    return;
  }
  n->marked = 1;
  if (aura_gc_mark_sp >= AURA_GC_MARK_STACK)
  {
    fputs("aura: GC mark stack overflow\n", stderr);
    abort();
  }
  aura_gc_mark_stack[aura_gc_mark_sp++] = n;
}

/* C6a: mark object and enqueue; scan body for nested GC pointers. */
static void aura_gc_mark_scan(AuraGcNode *n)
{
  if (n == NULL || n->ptr == NULL || n->size < sizeof(void *))
  {
    return;
  }
  /* Align scan to pointer-sized slots within the allocation. */
  uintptr_t base = (uintptr_t)n->ptr;
  size_t nslots = n->size / sizeof(void *);
  for (size_t i = 0; i < nslots; i++)
  {
    void *candidate = *(void **)(base + i * sizeof(void *));
    if (candidate == NULL)
    {
      continue;
    }
    AuraGcNode *child = aura_gc_find(candidate);
    if (child != NULL)
    {
      aura_gc_mark_push(child);
    }
  }
}

void *aura_gc_alloc_full(size_t size, void (*dtor)(void *), void (*mark_extras)(void *))
{
  aura_gc_lock_enter();
  void *p = malloc(size);
  if (p == NULL && size > 0)
  {
    fputs("aura: GC allocation failed\n", stderr);
    abort();
  }
  if (p != NULL && size > 0)
  {
    memset(p, 0, size);
  }
  AuraGcNode *n = (AuraGcNode *)malloc(sizeof(AuraGcNode));
  if (n == NULL)
  {
    fputs("aura: GC metadata allocation failed\n", stderr);
    abort();
  }
  n->ptr = p;
  n->size = size;
  n->marked = 0;
  n->dtor = dtor;
  n->mark_extras = mark_extras;
  n->next = aura_gc_list;
  aura_gc_list = n;
  aura_gc_lock_leave();
  return p;
}

void *aura_gc_alloc(size_t size)
{
  return aura_gc_alloc_full(size, NULL, NULL);
}

/* Release one runtime-owned GC allocation immediately.  Task frame locals
 * are rooted while the frame exists, but their storage is still owned by the
 * frame and must not be left on the GC list until an unrelated collection. */
static void aura_gc_release(void *ptr)
{
  if (ptr == NULL)
  {
    return;
  }
  aura_gc_lock_enter();
  AuraGcNode **link = &aura_gc_list;
  while (*link != NULL)
  {
    AuraGcNode *n = *link;
    if (n->ptr == ptr)
    {
      *link = n->next;
      if (n->dtor != NULL)
      {
        n->dtor(n->ptr);
      }
      free(n->ptr);
      free(n);
      aura_gc_lock_leave();
      return;
    }
    link = &n->next;
  }
  aura_gc_lock_leave();
}

/* C7b: mark a GC object pointer (for generated mark_extras on Array fields). */
void aura_gc_mark_ptr(void *obj)
{
  aura_gc_lock_enter();
  if (obj == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  AuraGcNode *n = aura_gc_find(obj);
  if (n != NULL)
  {
    aura_gc_mark_push(n);
  }
  aura_gc_lock_leave();
}

/* Frames are malloc-owned, so their opaque data is not visible to the
 * collector unless the frame supplies an explicit mark contract. */
static void aura_gc_mark_task_frames(void);

/* C12k/C12l/C13e: Fun capture env header (must match codegen layout).
 * Layout of every capturing env:
 *   void (*__drop)(void *);
 *   int32_t __refs;
 *   … capture slots (class GC roots, boxes, nested Fun fat pointers, …)
 * Array capture slots are non-owning header views — drop must not free buffers.
 * C12m: by-ref Int/Bool captures release their shared boxes in drop.
 * C13e: Fun slots retain nested env; drop releases nested env once via RC. */
typedef struct
{
  void (*drop)(void *);
  int32_t refs;
} aura_fun_env_hdr;

void aura_fun_env_retain(void *env)
{
  if (env == NULL)
  {
    return;
  }
  aura_fun_env_hdr *h = (aura_fun_env_hdr *)env;
  h->refs++;
}

/* Release one ownership share; on zero refs run __drop then free. */
void aura_fun_env_free(void *env)
{
  if (env == NULL)
  {
    return;
  }
  aura_fun_env_hdr *h = (aura_fun_env_hdr *)env;
  if (h->refs > 1)
  {
    h->refs--;
    return;
  }
  h->refs = 0;
  if (h->drop != NULL)
  {
    h->drop(env);
  }
  else
  {
    free(env);
  }
}

/* C20b: generic shared pointer box for future mutable class/Array/Fun
 * captures.  The box owns only the callback contract supplied by its caller;
 * it does not infer whether value is GC-managed, an Array header, or a Fun
 * environment.  This keeps the ABI additive and lets codegen select the
 * appropriate drop policy when those capture forms are enabled. */
typedef void (*aura_box_ptr_drop_fn)(void *value);

typedef struct aura_box_ptr
{
  void *value;
  int32_t refs;
  aura_box_ptr_drop_fn drop;
} aura_box_ptr;

aura_box_ptr *aura_box_ptr_new(void *value, aura_box_ptr_drop_fn drop)
{
  aura_box_ptr *b = (aura_box_ptr *)malloc(sizeof(aura_box_ptr));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box ptr)\n");
    exit(1);
  }
  b->value = value;
  b->refs = 1;
  b->drop = drop;
  return b;
}

void aura_box_ptr_retain(aura_box_ptr *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_ptr_release(aura_box_ptr *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    if (b->drop != NULL && b->value != NULL)
    {
      b->drop(b->value);
    }
    free(b);
  }
}

void *aura_box_ptr_get(const aura_box_ptr *b)
{
  return b == NULL ? NULL : b->value;
}

void *aura_box_ptr_set(aura_box_ptr *b, void *value,
                      aura_box_ptr_drop_fn drop)
{
  if (b == NULL)
  {
    return NULL;
  }
  if (b->value == value && b->drop == drop)
  {
    return b->value;
  }
  if (b->drop != NULL && b->value != NULL)
  {
    b->drop(b->value);
  }
  b->value = value;
  b->drop = drop;
  return b->value;
}

/* C12m: shared mutable boxes for `var` Int/Bool lambda captures (refcounted). */
typedef struct aura_box_i64
{
  int64_t value;
  int32_t refs;
} aura_box_i64;

typedef struct aura_box_bool
{
  bool value;
  int32_t refs;
} aura_box_bool;

aura_box_i64 *aura_box_i64_new(int64_t v)
{
  aura_box_i64 *b = (aura_box_i64 *)malloc(sizeof(aura_box_i64));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box i64)\n");
    exit(1);
  }
  b->value = v;
  b->refs = 1;
  return b;
}

void aura_box_i64_retain(aura_box_i64 *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_i64_release(aura_box_i64 *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    free(b);
  }
}

aura_box_bool *aura_box_bool_new(bool v)
{
  aura_box_bool *b = (aura_box_bool *)malloc(sizeof(aura_box_bool));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box bool)\n");
    exit(1);
  }
  b->value = v;
  b->refs = 1;
  return b;
}

void aura_box_bool_retain(aura_box_bool *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_bool_release(aura_box_bool *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    free(b);
  }
}

/* C13f: shared mutable box for `var` String lambda captures (refcounted).
 * The box always owns a heap copy of the string so release can free safely
 * (literals and temporary concat results both work). */
typedef struct aura_box_str
{
  const char *value;
  int32_t refs;
} aura_box_str;

static char *aura_box_str_dup(const char *v)
{
  if (v == NULL)
  {
    return NULL;
  }
  size_t n = strlen(v);
  char *p = (char *)malloc(n + 1);
  if (p == NULL)
  {
    fprintf(stderr, "aura: out of memory (box str copy)\n");
    exit(1);
  }
  if (n > 0)
  {
    memcpy(p, v, n);
  }
  p[n] = '\0';
  return p;
}

aura_box_str *aura_box_str_new(const char *v)
{
  aura_box_str *b = (aura_box_str *)malloc(sizeof(aura_box_str));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box str)\n");
    exit(1);
  }
  b->value = aura_box_str_dup(v);
  b->refs = 1;
  return b;
}

void aura_box_str_retain(aura_box_str *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_str_release(aura_box_str *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    free((void *)b->value);
    free(b);
  }
}

/* Replace boxed string; frees previous owned value. Safe for self-assign
 * (copy first). Used by codegen for `var` String by-ref capture writes.
 * Returns the new owned pointer (or NULL). */
const char *aura_box_str_set(aura_box_str *b, const char *v)
{
  if (b == NULL)
  {
    return NULL;
  }
  const char *copy = aura_box_str_dup(v);
  free((void *)b->value);
  b->value = copy;
  return b->value;
}

/* Snapshot boxed string for escape (return/bind/eq/concat). Caller owns the
 * buffer so later box mutations do not invalidate it. */
const char *aura_box_str_get(aura_box_str *b)
{
  if (b == NULL)
  {
    return NULL;
  }
  return aura_box_str_dup(b->value);
}

/* C14: compiler-backed Hashable implementation for String.
 * Keep the same deterministic 31-based hash used by std.collections. */
int64_t aura_hash_string(const char *s)
{
  int64_t h = 0;
  if (s == NULL)
  {
    return 0;
  }
  for (const unsigned char *p = (const unsigned char *)s; *p != '\0'; ++p)
  {
    h = h * 31 + (int64_t)*p;
  }
  return h < 0 ? -h : h;
}

/* C13c: Int.toString() — decimal (base 10), no locale.
 * Returns a freshly malloc'd NUL-terminated C string. Caller owns the buffer
 * (same ownership as other owned strings: substring/trim/split segments, concat).
 * Handles 0, negatives, and INT64_MIN. */
const char *aura_i64_to_string(int64_t v)
{
  /* "-9223372036854775808" + NUL = 21; pad for safety. */
  char buf[32];
  size_t i = 0;
  uint64_t u;
  if (v < 0)
  {
    /* Negate via unsigned to keep INT64_MIN well-defined. */
    u = (uint64_t)(-(v + 1)) + 1;
  }
  else
  {
    u = (uint64_t)v;
  }
  if (u == 0)
  {
    buf[i++] = '0';
  }
  else
  {
    char tmp[32];
    size_t n = 0;
    while (u > 0)
    {
      tmp[n++] = (char)('0' + (u % 10));
      u /= 10;
    }
    while (n > 0)
    {
      buf[i++] = tmp[--n];
    }
  }
  size_t dig_len = i;
  size_t total = dig_len + (v < 0 ? 1 : 0);
  char *out = (char *)malloc(total + 1);
  if (out == NULL)
  {
    fprintf(stderr, "aura: out of memory (i64_to_string)\n");
    exit(1);
  }
  size_t o = 0;
  if (v < 0)
  {
    out[o++] = '-';
  }
  memcpy(out + o, buf, dig_len);
  out[o + dig_len] = '\0';
  return (const char *)out;
}

static int aura_encoding_hex_value(unsigned char c)
{
  if (c >= '0' && c <= '9') return (int)(c - '0');
  if (c >= 'a' && c <= 'f') return (int)(c - 'a') + 10;
  if (c >= 'A' && c <= 'F') return (int)(c - 'A') + 10;
  return -1;
}

const char *aura_encoding_hex_encode(const char *value)
{
  static const char digits[] = "0123456789abcdef";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 1) / 2) return NULL;
  char *out = (char *)malloc(length * 2 + 1);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i++) {
    unsigned char byte = (unsigned char)value[i];
    out[i * 2] = digits[byte >> 4];
    out[i * 2 + 1] = digits[byte & 0x0f];
  }
  out[length * 2] = '\0';
  return out;
}

const char *aura_encoding_hex_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if ((length & 1u) != 0) return NULL;
  char *out = (char *)malloc(length / 2 + 1);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i += 2) {
    int hi = aura_encoding_hex_value((unsigned char)value[i]);
    int lo = aura_encoding_hex_value((unsigned char)value[i + 1]);
    if (hi < 0 || lo < 0 || (hi == 0 && lo == 0)) { free(out); return NULL; }
    out[i / 2] = (char)((hi << 4) | lo);
  }
  out[length / 2] = '\0';
  return out;
}

static int aura_encoding_base64_value(unsigned char c)
{
  if (c >= 'A' && c <= 'Z') return (int)(c - 'A');
  if (c >= 'a' && c <= 'z') return (int)(c - 'a') + 26;
  if (c >= '0' && c <= '9') return (int)(c - '0') + 52;
  if (c == '+') return 62;
  if (c == '/') return 63;
  return -1;
}

const char *aura_encoding_base64_encode(const char *value)
{
  static const char digits[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 4) / 4 * 3) return NULL;
  size_t output_length = ((length + 2) / 3) * 4;
  char *out = (char *)malloc(output_length + 1);
  if (out == NULL) return NULL;
  size_t i = 0, o = 0;
  while (i < length) {
    size_t remaining = length - i;
    unsigned int a = (unsigned char)value[i++];
    unsigned int b = remaining > 1 ? (unsigned char)value[i++] : 0;
    unsigned int c = remaining > 2 ? (unsigned char)value[i++] : 0;
    out[o++] = digits[a >> 2];
    out[o++] = digits[((a & 3u) << 4) | (b >> 4)];
    out[o++] = remaining > 1 ? digits[((b & 15u) << 2) | (c >> 6)] : '=';
    out[o++] = remaining > 2 ? digits[c & 63u] : '=';
  }
  out[o] = '\0';
  return out;
}

const char *aura_encoding_base64_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if (length == 0) {
    char *empty = (char *)malloc(1);
    if (empty != NULL) empty[0] = '\0';
    return empty;
  }
  if ((length & 3u) != 0) return NULL;
  size_t output_length = (length / 4) * 3;
  if (value[length - 1] == '=') output_length--;
  if (value[length - 2] == '=') output_length--;
  char *out = (char *)malloc(output_length + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i += 4) {
    int a = aura_encoding_base64_value((unsigned char)value[i]);
    int b = aura_encoding_base64_value((unsigned char)value[i + 1]);
    int c = value[i + 2] == '=' ? 0 : aura_encoding_base64_value((unsigned char)value[i + 2]);
    int d = value[i + 3] == '=' ? 0 : aura_encoding_base64_value((unsigned char)value[i + 3]);
    int last = i + 4 == length;
    if (a < 0 || b < 0 || c < 0 || d < 0 ||
        (!last && (value[i + 2] == '=' || value[i + 3] == '=')) ||
        (value[i + 2] == '=' && value[i + 3] != '=') ||
        (value[i + 2] == '=' && (b & 15) != 0) ||
        (value[i + 3] == '=' && value[i + 2] != '=' && (c & 3) != 0)) {
      free(out); return NULL;
    }
    unsigned int triple = ((unsigned int)a << 18) | ((unsigned int)b << 12) |
                          ((unsigned int)c << 6) | (unsigned int)d;
    if (o < output_length) out[o++] = (char)(triple >> 16);
    if (o < output_length) out[o++] = (char)(triple >> 8);
    if (o < output_length) out[o++] = (char)triple;
  }
  for (size_t i = 0; i < output_length; i++) if (out[i] == '\0') { free(out); return NULL; }
  out[output_length] = '\0';
  return out;
}

static int aura_encoding_percent_unreserved(unsigned char c)
{
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
         (c >= '0' && c <= '9') || c == '-' || c == '.' || c == '_' || c == '~';
}

const char *aura_encoding_percent_encode(const char *value)
{
  static const char digits[] = "0123456789ABCDEF";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 1) / 3) return NULL;
  char *out = (char *)malloc(length * 3 + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)value[i];
    if (aura_encoding_percent_unreserved(c)) out[o++] = (char)c;
    else { out[o++] = '%'; out[o++] = digits[c >> 4]; out[o++] = digits[c & 15]; }
  }
  out[o] = '\0';
  return out;
}

const char *aura_encoding_percent_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)value[i];
    if (c == '%') {
      if (i + 2 >= length) { free(out); return NULL; }
      int hi = aura_encoding_hex_value((unsigned char)value[++i]);
      int lo = aura_encoding_hex_value((unsigned char)value[++i]);
      if (hi < 0 || lo < 0 || (hi == 0 && lo == 0)) { free(out); return NULL; }
      out[o++] = (char)((hi << 4) | lo);
    } else out[o++] = (char)c;
  }
  out[o] = '\0';
  return out;
}

_Bool aura_encoding_is_valid_utf8(const char *value)
{
  const unsigned char *bytes = (const unsigned char *)(value == NULL ? "" : value);
  size_t length = strlen((const char *)bytes);
  size_t i = 0;
  while (i < length) {
    unsigned char lead = bytes[i++];
    uint32_t codepoint;
    size_t continuation;
    if (lead <= 0x7f) continue;
    if (lead >= 0xc2 && lead <= 0xdf) { codepoint = lead & 0x1f; continuation = 1; }
    else if (lead >= 0xe0 && lead <= 0xef) { codepoint = lead & 0x0f; continuation = 2; }
    else if (lead >= 0xf0 && lead <= 0xf4) { codepoint = lead & 0x07; continuation = 3; }
    else return false;
    if (i + continuation > length) return false;
    for (size_t j = 0; j < continuation; j++) {
      unsigned char tail = bytes[i++];
      if ((tail & 0xc0) != 0x80) return false;
      codepoint = (codepoint << 6) | (tail & 0x3f);
    }
    if ((continuation == 2 && codepoint < 0x800) ||
        (continuation == 3 && codepoint < 0x10000) ||
        codepoint > 0x10ffff || (codepoint >= 0xd800 && codepoint <= 0xdfff)) return false;
  }
  return true;
}

typedef struct AuraJsonCursor
{
  const unsigned char *data;
  size_t length;
  size_t index;
  unsigned depth;
} AuraJsonCursor;

static void aura_json_skip_ws(AuraJsonCursor *cursor)
{
  while (cursor->index < cursor->length &&
         (cursor->data[cursor->index] == ' ' || cursor->data[cursor->index] == '\n' ||
          cursor->data[cursor->index] == '\r' || cursor->data[cursor->index] == '\t'))
  {
    cursor->index++;
  }
}

static int aura_json_hex(unsigned char c)
{
  if (c >= '0' && c <= '9') return c - '0';
  if (c >= 'a' && c <= 'f') return c - 'a' + 10;
  if (c >= 'A' && c <= 'F') return c - 'A' + 10;
  return -1;
}

static int aura_json_string(AuraJsonCursor *cursor)
{
  if (cursor->index >= cursor->length || cursor->data[cursor->index++] != '"') return 0;
  while (cursor->index < cursor->length)
  {
    unsigned char c = cursor->data[cursor->index++];
    if (c == '"') return 1;
    if (c < 0x20) return 0;
    if (c != '\\') continue;
    if (cursor->index >= cursor->length) return 0;
    c = cursor->data[cursor->index++];
    if (strchr("\\\"/bfnrt", (int)c) != NULL) continue;
    if (c != 'u' || cursor->index + 4 > cursor->length) return 0;
    for (unsigned i = 0; i < 4; i++)
    {
      if (aura_json_hex(cursor->data[cursor->index++]) < 0) return 0;
    }
  }
  return 0;
}

static int aura_json_value(AuraJsonCursor *cursor);

static int aura_json_array(AuraJsonCursor *cursor)
{
  if (cursor->data[cursor->index++] != '[' || ++cursor->depth > 64) return 0;
  aura_json_skip_ws(cursor);
  if (cursor->index < cursor->length && cursor->data[cursor->index] == ']')
  {
    cursor->index++;
    cursor->depth--;
    return 1;
  }
  for (;;)
  {
    if (!aura_json_value(cursor)) return 0;
    aura_json_skip_ws(cursor);
    if (cursor->index >= cursor->length) return 0;
    if (cursor->data[cursor->index] == ']') { cursor->index++; cursor->depth--; return 1; }
    if (cursor->data[cursor->index++] != ',') return 0;
    aura_json_skip_ws(cursor);
  }
}

static int aura_json_object(AuraJsonCursor *cursor)
{
  if (cursor->data[cursor->index++] != '{' || ++cursor->depth > 64) return 0;
  aura_json_skip_ws(cursor);
  if (cursor->index < cursor->length && cursor->data[cursor->index] == '}')
  {
    cursor->index++;
    cursor->depth--;
    return 1;
  }
  for (;;)
  {
    if (!aura_json_string(cursor)) return 0;
    aura_json_skip_ws(cursor);
    if (cursor->index >= cursor->length || cursor->data[cursor->index++] != ':') return 0;
    aura_json_skip_ws(cursor);
    if (!aura_json_value(cursor)) return 0;
    aura_json_skip_ws(cursor);
    if (cursor->index >= cursor->length) return 0;
    if (cursor->data[cursor->index] == '}') { cursor->index++; cursor->depth--; return 1; }
    if (cursor->data[cursor->index++] != ',') return 0;
    aura_json_skip_ws(cursor);
  }
}

static int aura_json_number(AuraJsonCursor *cursor)
{
  size_t start = cursor->index;
  if (cursor->index < cursor->length && cursor->data[cursor->index] == '-') cursor->index++;
  if (cursor->index >= cursor->length) return 0;
  if (cursor->data[cursor->index] == '0') cursor->index++;
  else
  {
    if (cursor->data[cursor->index] < '1' || cursor->data[cursor->index] > '9') return 0;
    while (cursor->index < cursor->length && isdigit(cursor->data[cursor->index])) cursor->index++;
  }
  if (cursor->index < cursor->length && cursor->data[cursor->index] == '.')
  {
    cursor->index++;
    if (cursor->index >= cursor->length || !isdigit(cursor->data[cursor->index])) return 0;
    while (cursor->index < cursor->length && isdigit(cursor->data[cursor->index])) cursor->index++;
  }
  if (cursor->index < cursor->length && (cursor->data[cursor->index] == 'e' || cursor->data[cursor->index] == 'E'))
  {
    cursor->index++;
    if (cursor->index < cursor->length && (cursor->data[cursor->index] == '+' || cursor->data[cursor->index] == '-')) cursor->index++;
    if (cursor->index >= cursor->length || !isdigit(cursor->data[cursor->index])) return 0;
    while (cursor->index < cursor->length && isdigit(cursor->data[cursor->index])) cursor->index++;
  }
  return cursor->index > start;
}

static int aura_json_value(AuraJsonCursor *cursor)
{
  aura_json_skip_ws(cursor);
  if (cursor->index >= cursor->length) return 0;
  switch (cursor->data[cursor->index])
  {
    case '"': return aura_json_string(cursor);
    case '[': return aura_json_array(cursor);
    case '{': return aura_json_object(cursor);
    case 't':
      if (cursor->index + 4 <= cursor->length && memcmp(cursor->data + cursor->index, "true", 4) == 0) { cursor->index += 4; return 1; }
      return 0;
    case 'f':
      if (cursor->index + 5 <= cursor->length && memcmp(cursor->data + cursor->index, "false", 5) == 0) { cursor->index += 5; return 1; }
      return 0;
    case 'n':
      if (cursor->index + 4 <= cursor->length && memcmp(cursor->data + cursor->index, "null", 4) == 0) { cursor->index += 4; return 1; }
      return 0;
    default: return aura_json_number(cursor);
  }
}

_Bool aura_json_is_valid(const char *value)
{
  AuraJsonCursor cursor;
  if (value == NULL || !aura_encoding_is_valid_utf8(value)) return false;
  cursor.data = (const unsigned char *)value;
  cursor.length = strlen(value);
  cursor.index = 0;
  cursor.depth = 0;
  if (!aura_json_value(&cursor)) return false;
  aura_json_skip_ws(&cursor);
  return cursor.index == cursor.length;
}

int64_t aura_json_error_offset(const char *value)
{
  AuraJsonCursor cursor;
  if (value == NULL || !aura_encoding_is_valid_utf8(value)) return 0;
  cursor.data = (const unsigned char *)value;
  cursor.length = strlen(value);
  cursor.index = 0;
  cursor.depth = 0;
  if (!aura_json_value(&cursor)) return (int64_t)cursor.index;
  aura_json_skip_ws(&cursor);
  return cursor.index == cursor.length ? -1 : (int64_t)cursor.index;
}

const char *aura_json_escape_string(const char *value)
{
  const unsigned char *input = (const unsigned char *)(value == NULL ? "" : value);
  size_t length = strlen((const char *)input);
  if (!aura_encoding_is_valid_utf8((const char *)input) || length > (SIZE_MAX - 3) / 2) return NULL;
  char *out = (char *)malloc(length * 2 + 3);
  size_t o = 0;
  if (out == NULL) return NULL;
  out[o++] = '"';
  for (size_t i = 0; i < length; i++)
  {
    unsigned char c = input[i];
    switch (c)
    {
      case '"': out[o++] = '\\'; out[o++] = '"'; break;
      case '\\': out[o++] = '\\'; out[o++] = '\\'; break;
      case '\b': out[o++] = '\\'; out[o++] = 'b'; break;
      case '\f': out[o++] = '\\'; out[o++] = 'f'; break;
      case '\n': out[o++] = '\\'; out[o++] = 'n'; break;
      case '\r': out[o++] = '\\'; out[o++] = 'r'; break;
      case '\t': out[o++] = '\\'; out[o++] = 't'; break;
      default: out[o++] = (char)c; break;
    }
  }
  out[o++] = '"';
  out[o] = '\0';
  return out;
}

static int aura_url_target_byte_allowed(unsigned char c)
{
  return c >= 0x21 && c != 0x23 && c != 0x7f;
}

static char *aura_url_copy_range(const char *value, size_t length)
{
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  if (length != 0) memcpy(out, value, length);
  out[length] = '\0';
  return out;
}

static int aura_url_origin_parts(const char *target, size_t *path_length,
                                 size_t *query_start)
{
  size_t length = target == NULL ? 0 : strlen(target);
  if (length == 0 || target[0] != '/' || (length > 1 && target[1] == '/')) return 0;
  size_t question = SIZE_MAX;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)target[i];
    if (!aura_url_target_byte_allowed(c)) return 0;
    if (c == '?' && question == SIZE_MAX) question = i;
  }
  if (path_length != NULL) *path_length = question == SIZE_MAX ? length : question;
  if (query_start != NULL) *query_start = question;
  return 1;
}

static int aura_url_absolute_parts(const char *target, size_t *authority_start,
                                   size_t *authority_length)
{
  size_t length = target == NULL ? 0 : strlen(target);
  size_t i = 0;
  if (length == 0 || !isalpha((unsigned char)target[0])) return 0;
  i = 1;
  while (i < length && (isalnum((unsigned char)target[i]) || target[i] == '+' ||
                        target[i] == '-' || target[i] == '.'))
    i++;
  if (i + 3 > length || target[i] != ':' || target[i + 1] != '/' || target[i + 2] != '/') return 0;
  size_t start = i + 3;
  size_t end = start;
  while (end < length && target[end] != '/' && target[end] != '?' && target[end] != '#') {
    unsigned char c = (unsigned char)target[end];
    if (c <= 0x20 || c == 0x7f) return 0;
    end++;
  }
  if (end == start) return 0;
  if (authority_start != NULL) *authority_start = start;
  if (authority_length != NULL) *authority_length = end - start;
  return 1;
}

static char *aura_bytes_copy_n(const char *value, size_t length)
{
  char *out = (char *)malloc(length + 1u);
  if (out == NULL)
  {
    return NULL;
  }
  if (length != 0 && value != NULL)
  {
    memcpy(out, value, length);
  }
  out[length] = '\0';
  return out;
}

const char *aura_bytes_copy(const char *value)
{
  const char *source = value == NULL ? "" : value;
  return aura_bytes_copy_n(source, strlen(source));
}

const char *aura_bytes_concat(const char *left, const char *right)
{
  const char *a = left == NULL ? "" : left;
  const char *b = right == NULL ? "" : right;
  size_t alen = strlen(a);
  size_t blen = strlen(b);
  if (alen > SIZE_MAX - blen || alen + blen == SIZE_MAX)
  {
    return NULL;
  }
  char *out = (char *)malloc(alen + blen + 1u);
  if (out == NULL)
  {
    return NULL;
  }
  memcpy(out, a, alen);
  memcpy(out + alen, b, blen);
  out[alen + blen] = '\0';
  return out;
}

const char *aura_bytes_slice(const char *value, int64_t start, int64_t length)
{
  const char *source = value == NULL ? "" : value;
  size_t total = strlen(source);
  if (start < 0 || length < 0 || (uint64_t)start > (uint64_t)total ||
      (uint64_t)length > (uint64_t)total - (uint64_t)start)
  {
    return NULL;
  }
  return aura_bytes_copy_n(source + (size_t)start, (size_t)length);
}

_Bool aura_bytes_equals(const char *left, const char *right)
{
  const char *a = left == NULL ? "" : left;
  const char *b = right == NULL ? "" : right;
  size_t alen = strlen(a);
  size_t blen = strlen(b);
  return alen == blen && (alen == 0 || memcmp(a, b, alen) == 0);
}

static const char *aura_fs_text(const char *path)
{
  return path == NULL ? "" : path;
}

const char *aura_fs_join(const char *base, const char *child)
{
  const char *a = aura_fs_text(base);
  const char *b = aura_fs_text(child);
  size_t alen = strlen(a);
  size_t blen = strlen(b);
  while (alen > 0 && a[alen - 1] == '/')
  {
    alen--;
  }
  while (blen > 0 && *b == '/')
  {
    b++;
    blen--;
  }
  if (alen == 0)
  {
    return aura_bytes_copy_n(b, blen);
  }
  if (blen == 0)
  {
    return aura_bytes_copy_n(a, alen);
  }
  if (alen > SIZE_MAX - blen || alen + blen > SIZE_MAX - 2u)
  {
    return NULL;
  }
  char *out = (char *)malloc(alen + blen + 2u);
  if (out == NULL)
  {
    return NULL;
  }
  memcpy(out, a, alen);
  out[alen] = '/';
  memcpy(out + alen + 1u, b, blen);
  out[alen + blen + 1u] = '\0';
  return out;
}

const char *aura_fs_basename(const char *path)
{
  const char *p = aura_fs_text(path);
  size_t len = strlen(p);
  if (len == 1 && p[0] == '/')
  {
    return aura_bytes_copy_n("/", 1);
  }
  while (len > 0 && p[len - 1] == '/')
  {
    len--;
  }
  size_t start = len;
  while (start > 0 && p[start - 1] != '/')
  {
    start--;
  }
  return aura_bytes_copy_n(p + start, len - start);
}

const char *aura_fs_dirname(const char *path)
{
  const char *p = aura_fs_text(path);
  size_t len = strlen(p);
  if (len == 1 && p[0] == '/')
  {
    return aura_bytes_copy_n("/", 1);
  }
  while (len > 1 && p[len - 1] == '/')
  {
    len--;
  }
  size_t slash = len;
  while (slash > 0 && p[slash - 1] != '/')
  {
    slash--;
  }
  if (slash == 0)
  {
    return aura_bytes_copy_n(".", 1);
  }
  while (slash > 1 && p[slash - 1] == '/')
  {
    slash--;
  }
  return aura_bytes_copy_n(p, slash);
}

const char *aura_fs_extension(const char *path)
{
  const char *name = aura_fs_text(path);
  size_t len = strlen(name);
  while (len > 0 && name[len - 1] == '/')
  {
    len--;
  }
  size_t start = len;
  while (start > 0 && name[start - 1] != '/')
  {
    start--;
  }
  size_t dot = len;
  while (dot > start && name[dot - 1] != '.')
  {
    dot--;
  }
  if (dot == start || dot == len || (dot == start + 1u && len == start + 1u))
  {
    return NULL;
  }
  return aura_bytes_copy_n(name + dot - 1u, len - dot + 1u);
}

_Bool aura_fs_is_absolute(const char *path)
{
  const char *p = aura_fs_text(path);
  return p[0] == '/';
}

const char *aura_os_get_env(const char *name)
{
  const char *key = name == NULL ? "" : name;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return NULL;
  }
  const char *value = getenv(key);
  return value == NULL ? NULL : aura_bytes_copy(value);
}

_Bool aura_os_set_env(const char *name, const char *value)
{
  const char *key = name == NULL ? "" : name;
  const char *text = value == NULL ? "" : value;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return false;
  }
#if defined(__unix__) || defined(__APPLE__)
  return setenv(key, text, 1) == 0;
#else
  (void)text;
  return false;
#endif
}

_Bool aura_os_unset_env(const char *name)
{
  const char *key = name == NULL ? "" : name;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return false;
  }
#if defined(__unix__) || defined(__APPLE__)
  return unsetenv(key) == 0;
#else
  return false;
#endif
}

const char *aura_os_cwd(void)
{
#if defined(__unix__) || defined(__APPLE__)
  size_t capacity = 256;
  for (;;)
  {
    char *buffer = (char *)malloc(capacity);
    if (buffer == NULL)
    {
      return NULL;
    }
    if (getcwd(buffer, capacity) != NULL)
    {
      return buffer;
    }
    free(buffer);
    if (errno != ERANGE || capacity > SIZE_MAX / 2u)
    {
      return NULL;
    }
    capacity *= 2u;
  }
#else
  return NULL;
#endif
}

int64_t aura_os_pid(void)
{
#if defined(__unix__) || defined(__APPLE__)
  return (int64_t)getpid();
#else
  return -1;
#endif
}

const char *aura_os_platform(void)
{
#if defined(__APPLE__)
  return aura_bytes_copy("macos");
#elif defined(__linux__)
  return aura_bytes_copy("linux");
#elif defined(_WIN32)
  return aura_bytes_copy("windows");
#else
  return aura_bytes_copy("unknown");
#endif
}

/* Bounded numeric DNS lookup: return one address in presentation form. The
 * resolver result is copied into Aura-owned storage and the addrinfo chain is
 * released before returning. */
const char *aura_dns_resolve_host(const char *host, int prefer_ipv6)
{
#if defined(__unix__) || defined(__APPLE__)
  struct addrinfo hints;
  struct addrinfo *results = NULL;
  struct addrinfo *entry;
  char address[INET6_ADDRSTRLEN];
  int families[2];
  int i;

  if (host == NULL || host[0] == '\0') return NULL;
  memset(&hints, 0, sizeof(hints));
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_ADDRCONFIG;
  families[0] = prefer_ipv6 ? AF_INET6 : AF_INET;
  families[1] = prefer_ipv6 ? AF_INET : AF_INET6;
  for (i = 0; i < 2; i++)
  {
    hints.ai_family = families[i];
    if (getaddrinfo(host, NULL, &hints, &results) != 0) continue;
    for (entry = results; entry != NULL; entry = entry->ai_next)
    {
      const void *source = NULL;
      if (entry->ai_family == AF_INET)
      {
        source = &((const struct sockaddr_in *)entry->ai_addr)->sin_addr;
      }
      else if (entry->ai_family == AF_INET6)
      {
        source = &((const struct sockaddr_in6 *)entry->ai_addr)->sin6_addr;
      }
      if (source != NULL && inet_ntop(entry->ai_family, source, address,
                                      sizeof(address)) != NULL)
      {
        const char *copy = aura_bytes_copy(address);
        freeaddrinfo(results);
        return copy;
      }
    }
    freeaddrinfo(results);
    results = NULL;
  }
  return NULL;
#else
  (void)host;
  (void)prefer_ipv6;
  return NULL;
#endif
}

/* Return a bounded, preference-ordered address snapshot. Each line contains
 * one numeric address; the result is Aura-owned and capped at 64 KiB. */
const char *aura_dns_resolve_host_list(const char *host, int prefer_ipv6)
{
#if defined(__unix__) || defined(__APPLE__)
  struct addrinfo hints;
  struct addrinfo *results = NULL;
  struct addrinfo *entry;
  char address[INET6_ADDRSTRLEN];
  int families[2];
  int i;
  size_t used = 0;
  size_t capacity = 64u * 1024u;
  char *output;

  if (host == NULL || host[0] == '\0') return NULL;
  output = (char *)malloc(capacity);
  if (output == NULL) return NULL;
  output[0] = '\0';
  memset(&hints, 0, sizeof(hints));
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_ADDRCONFIG;
  families[0] = prefer_ipv6 ? AF_INET6 : AF_INET;
  families[1] = prefer_ipv6 ? AF_INET : AF_INET6;
  for (i = 0; i < 2; i++)
  {
    hints.ai_family = families[i];
    if (getaddrinfo(host, NULL, &hints, &results) != 0) continue;
    for (entry = results; entry != NULL; entry = entry->ai_next)
    {
      const void *source = NULL;
      size_t length;
      if (entry->ai_family == AF_INET)
      {
        source = &((const struct sockaddr_in *)entry->ai_addr)->sin_addr;
      }
      else if (entry->ai_family == AF_INET6)
      {
        source = &((const struct sockaddr_in6 *)entry->ai_addr)->sin6_addr;
      }
      if (source == NULL || inet_ntop(entry->ai_family, source, address,
                                      sizeof(address)) == NULL)
        continue;
      length = strlen(address);
      if (used + length + (used == 0 ? 0u : 1u) + 1u >= capacity) break;
      if (used != 0) output[used++] = '\n';
      memcpy(output + used, address, length);
      used += length;
      output[used] = '\0';
    }
    freeaddrinfo(results);
    results = NULL;
  }
  if (used == 0)
  {
    free(output);
    return NULL;
  }
  return output;
#else
  (void)host;
  (void)prefer_ipv6;
  return NULL;
#endif
}

_Bool aura_url_is_origin_form(const char *target)
{
  return aura_url_origin_parts(target, NULL, NULL) != 0;
}

const char *aura_url_path(const char *target)
{
  size_t path_length = 0;
  if (!aura_url_origin_parts(target, &path_length, NULL)) return NULL;
  return aura_url_copy_range(target, path_length);
}

const char *aura_url_normalize_path(const char *path)
{
  size_t length = path == NULL ? 0 : strlen(path);
  if (length == 0 || path[0] != '/') return NULL;
  for (size_t i = 0; i < length; i++)
  {
    unsigned char c = (unsigned char)path[i];
    if (!aura_url_target_byte_allowed(c) || c == '?' || c == '#') return NULL;
  }
  char *out = (char *)malloc(length + 2);
  if (out == NULL) return NULL;
  size_t used = 1;
  out[0] = '/';
  size_t segment_start = 1;
  for (size_t i = 1; i <= length; i++)
  {
    if (i != length && path[i] != '/') continue;
    size_t segment_length = i - segment_start;
    if (segment_length == 0 || (segment_length == 1 && path[segment_start] == '.'))
    {
      /* Repeated separators and dot segments do not add output. */
    }
    else if (segment_length == 2 && path[segment_start] == '.' && path[segment_start + 1] == '.')
    {
      if (used > 1)
      {
        used--;
        while (used > 1 && out[used - 1] != '/') used--;
      }
    }
    else
    {
      if (used > 1 && out[used - 1] != '/') out[used++] = '/';
      memcpy(out + used, path + segment_start, segment_length);
      used += segment_length;
    }
    segment_start = i + 1;
  }
  if (length > 1 && path[length - 1] == '/' && used > 1 && out[used - 1] != '/') out[used++] = '/';
  out[used] = '\0';
  return out;
}

const char *aura_url_query(const char *target)
{
  size_t query_start = SIZE_MAX;
  if (!aura_url_origin_parts(target, NULL, &query_start) || query_start == SIZE_MAX) return NULL;
  return aura_url_copy_range(target + query_start + 1, strlen(target) - query_start - 1);
}

_Bool aura_url_is_absolute(const char *target)
{
  return aura_url_absolute_parts(target, NULL, NULL) != 0;
}

const char *aura_url_authority(const char *target)
{
  size_t start = 0;
  size_t length = 0;
  if (!aura_url_absolute_parts(target, &start, &length)) return NULL;
  return aura_url_copy_range(target + start, length);
}

static int aura_url_authority_bounds(const char *target, size_t *start,
                                     size_t *length)
{
  size_t authority_start = 0;
  size_t authority_length = 0;
  if (!aura_url_absolute_parts(target, &authority_start, &authority_length)) return 0;
  size_t end = authority_start + authority_length;
  size_t userinfo = SIZE_MAX;
  for (size_t i = authority_start; i < end; i++) {
    if (target[i] == '@') userinfo = i;
  }
  if (userinfo != SIZE_MAX) authority_start = userinfo + 1;
  if (authority_start >= end) return 0;
  if (start != NULL) *start = authority_start;
  if (length != NULL) *length = end - authority_start;
  return 1;
}

const char *aura_url_authority_host(const char *target)
{
  size_t start = 0, length = 0;
  if (!aura_url_authority_bounds(target, &start, &length)) return NULL;
  size_t end = start + length;
  size_t host_end = end;
  if (target[start] == '[') {
    size_t close = start + 1;
    while (close < end && target[close] != ']') close++;
    if (close >= end || close == start + 1) return NULL;
    host_end = close;
    return aura_url_copy_range(target + start + 1, host_end - start - 1);
  }
  size_t colon = SIZE_MAX;
  for (size_t i = start; i < end; i++) {
    if (target[i] == ':') {
      if (colon != SIZE_MAX) return NULL;
      colon = i;
    }
  }
  if (colon != SIZE_MAX) host_end = colon;
  if (host_end == start) return NULL;
  return aura_url_copy_range(target + start, host_end - start);
}

const char *aura_url_authority_port(const char *target)
{
  size_t start = 0, length = 0;
  if (!aura_url_authority_bounds(target, &start, &length)) return NULL;
  size_t end = start + length;
  size_t port_start = SIZE_MAX;
  if (target[start] == '[') {
    size_t close = start + 1;
    while (close < end && target[close] != ']') close++;
    if (close >= end || close + 1 >= end || target[close + 1] != ':') return NULL;
    port_start = close + 2;
  } else {
    for (size_t i = start; i < end; i++) {
      if (target[i] == ':') {
        if (port_start != SIZE_MAX) return NULL;
        port_start = i + 1;
      }
    }
  }
  if (port_start == SIZE_MAX || port_start >= end) return NULL;
  for (size_t i = port_start; i < end; i++) {
    if (!isdigit((unsigned char)target[i])) return NULL;
  }
  return aura_url_copy_range(target + port_start, end - port_start);
}

const char *aura_url_query_value(const char *target, const char *key)
{
  if (target == NULL || key == NULL || key[0] == '\0') return NULL;
  const char *question = strchr(target, '?');
  if (question == NULL) return NULL;
  const char *cursor = question + 1;
  size_t key_length = strlen(key);
  while (*cursor != '\0' && *cursor != '#') {
    const char *amp = strchr(cursor, '&');
    const char *end = amp == NULL ? cursor + strlen(cursor) : amp;
    const char *equals = memchr(cursor, '=', (size_t)(end - cursor));
    const char *value = equals == NULL ? end : equals + 1;
    size_t candidate_length = equals == NULL ? (size_t)(end - cursor) : (size_t)(equals - cursor);
    if (candidate_length == key_length && memcmp(cursor, key, key_length) == 0)
      return aura_url_copy_range(value, (size_t)(end - value));
    if (amp == NULL) break;
    cursor = amp + 1;
  }
  return NULL;
}

static int aura_mime_token_byte(unsigned char c)
{
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
         (c >= '0' && c <= '9') || strchr("!#$%&'*+-.^_`|~", (int)c) != NULL;
}

_Bool aura_mime_is_valid_type(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  size_t i = 0;
  if (length == 0) return false;
  size_t type_start = i;
  while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
  if (i == type_start || i >= length || value[i++] != '/') return false;
  size_t subtype_start = i;
  while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
  if (i == subtype_start) return false;
  while (i < length) {
    if (value[i++] != ';') return false;
    while (i < length && (value[i] == ' ' || value[i] == '\t')) i++;
    size_t key_start = i;
    while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
    if (i == key_start || i >= length || value[i++] != '=') return false;
    if (i >= length) return false;
    if (value[i] == '"') {
      i++;
      while (i < length && value[i] != '"') {
        if ((unsigned char)value[i] < 0x20 || value[i] == '\\') return false;
        i++;
      }
      if (i >= length || value[i++] != '"') return false;
    } else {
      size_t value_start = i;
      while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
      if (i == value_start) return false;
    }
    while (i < length && (value[i] == ' ' || value[i] == '\t')) i++;
  }
  return true;
}

const char *aura_mime_sanitize_filename(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if (length == 0 || (length == 1 && value[0] == '.') ||
      (length == 2 && value[0] == '.' && value[1] == '.')) return NULL;
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  size_t start = 0, out_length = 0;
  for (size_t i = 0; i <= length; i++) {
    if (i == length || value[i] == '/' || value[i] == '\\') {
      if (i > start) {
        if (out_length != 0) out[out_length++] = '_';
        for (size_t j = start; j < i; j++) {
          unsigned char c = (unsigned char)value[j];
          if (c < 0x20 || c == 0x7f) { free(out); return NULL; }
          out[out_length++] = (char)c;
        }
      }
      start = i + 1;
    }
  }
  if (out_length == 0) { free(out); return NULL; }
  out[out_length] = '\0';
  return out;
}

const char *aura_mime_disposition_filename(const char *value)
{
  if (value == NULL) return NULL;
  const char *cursor = value;
  while (*cursor != '\0') {
    while (*cursor == ';' || isspace((unsigned char)*cursor)) cursor++;
    const char *name = cursor;
    while (*cursor != '\0' && *cursor != '=' && *cursor != ';') cursor++;
    size_t name_length = (size_t)(cursor - name);
    while (name_length > 0 && isspace((unsigned char)name[name_length - 1])) name_length--;
    if (*cursor != '=') {
      while (*cursor != '\0' && *cursor != ';') cursor++;
      continue;
    }
    cursor++;
    while (isspace((unsigned char)*cursor)) cursor++;
    const char *raw = cursor;
    size_t raw_length = 0;
    if (*cursor == '"') {
      raw = ++cursor;
      while (*cursor != '\0' && *cursor != '"') cursor++;
      raw_length = (size_t)(cursor - raw);
      if (*cursor == '"') cursor++;
    } else {
      while (*cursor != '\0' && *cursor != ';') cursor++;
      raw_length = (size_t)(cursor - raw);
      while (raw_length > 0 && isspace((unsigned char)raw[raw_length - 1])) raw_length--;
    }
    if (name_length == 8) {
      static const char filename_name[] = "filename";
      int matches = 1;
      for (size_t i = 0; i < 8; i++) {
        unsigned char c = (unsigned char)name[i];
        if ((unsigned char)tolower(c) != (unsigned char)filename_name[i]) {
          matches = 0;
          break;
        }
      }
      if (matches) {
        char *raw_copy = aura_url_copy_range(raw, raw_length);
        const char *safe = aura_mime_sanitize_filename(raw_copy);
        free(raw_copy);
        return safe;
      }
    }
  }
  return NULL;
}

/* C4z/C5f/C6a/C6e: stop-the-world deep mark + sweep when roots are registered. */
void aura_gc_collect(void)
{
  aura_gc_lock_enter();
  for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
  {
    n->marked = 0;
  }
  if (aura_gc_root_n == 0 && aura_gc_array_root_n == 0)
  {
    /* No roots: keep everything (compiler may not have registered yet). */
    for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
    {
      n->marked = 1;
    }
    aura_gc_lock_leave();
    return;
  }
  aura_gc_mark_sp = 0;
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    void **slot = aura_gc_roots[i];
    if (slot == NULL)
    {
      continue;
    }
    void *obj = *slot;
    if (obj == NULL)
    {
      continue;
    }
    AuraGcNode *n = aura_gc_find(obj);
    if (n != NULL)
    {
      aura_gc_mark_push(n);
    }
  }
  /* C6e: mark GC objects stored in Array-of-class buffers. */
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    void **data_slot = aura_gc_array_roots[i].data_slot;
    int64_t *len_slot = aura_gc_array_roots[i].len_slot;
    if (data_slot == NULL || len_slot == NULL)
    {
      continue;
    }
    void **elems = (void **)*data_slot;
    int64_t len = *len_slot;
    if (elems == NULL || len <= 0)
    {
      continue;
    }
    for (int64_t j = 0; j < len; j++)
    {
      void *obj = elems[j];
      if (obj == NULL)
      {
        continue;
      }
      AuraGcNode *n = aura_gc_find(obj);
      if (n != NULL)
      {
        aura_gc_mark_push(n);
      }
    }
  }
  aura_gc_mark_task_frames();
  /* C6a/C7b: deep mark + per-type mark_extras (Array-of-class fields). */
  while (aura_gc_mark_sp > 0)
  {
    AuraGcNode *n = aura_gc_mark_stack[--aura_gc_mark_sp];
    if (n->mark_extras != NULL && n->ptr != NULL)
    {
      n->mark_extras(n->ptr);
    }
    aura_gc_mark_scan(n);
  }
  /* C5f/C7b: sweep unmarked objects; run dtor to free owned Array buffers. */
  AuraGcNode **link = &aura_gc_list;
  while (*link != NULL)
  {
    AuraGcNode *n = *link;
    if (!n->marked)
    {
      *link = n->next;
      if (n->dtor != NULL && n->ptr != NULL)
      {
        n->dtor(n->ptr);
      }
      free(n->ptr);
      free(n);
    }
    else
    {
      link = &n->next;
    }
  }
  aura_gc_lock_leave();
}

void aura_gc_shutdown(void)
{
  aura_gc_lock_enter();
  AuraGcNode *n = aura_gc_list;
  while (n != NULL)
  {
    AuraGcNode *next = n->next;
    if (n->dtor != NULL && n->ptr != NULL)
    {
      n->dtor(n->ptr);
    }
    free(n->ptr);
    free(n);
    n = next;
  }
  aura_gc_list = NULL;
  aura_gc_root_n = 0;
  aura_gc_array_root_n = 0;
  aura_ex_dispose_cleared_causes();
  aura_gc_lock_leave();
}

/* ---- F3 bounded foreign String/Array ABI ---- */

static int aura_ffi_array_shape_ok(uint64_t len, uint64_t cap,
                                   uint64_t elem_size, AuraFfiArrayKind kind)
{
  if (len > cap || (cap != 0 && elem_size == 0))
  {
    return 0;
  }
  if (kind == AURA_FFI_ARRAY_BYTES)
  {
    return elem_size == 1;
  }
  if (kind == AURA_FFI_ARRAY_INT64)
  {
    return elem_size == sizeof(int64_t);
  }
  if (kind == AURA_FFI_ARRAY_BOOL)
  {
    return elem_size == sizeof(uint8_t);
  }
  return 0;
}

AuraFfiStatus aura_ffi_string_borrow(const char *data, uint64_t len,
                                     AuraFfiStringView *out)
{
  if (out == NULL || (data == NULL && len != 0))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_string_copy(AuraFfiStringView view, AuraFfiString *out)
{
  if (out == NULL || (view.data == NULL && view.len != 0) ||
      view.len > (uint64_t)(SIZE_MAX - 1u))
  {
    return AURA_FFI_INVALID;
  }
  char *copy = (char *)malloc((size_t)view.len + 1u);
  if (copy == NULL)
  {
    return AURA_FFI_OOM;
  }
  if (view.len != 0)
  {
    memcpy(copy, view.data, (size_t)view.len);
  }
  copy[view.len] = '\0';
  out->data = copy;
  out->len = view.len;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_string_transfer(char *data, uint64_t len,
                                       AuraFfiString *out)
{
  if (out == NULL || (data == NULL && len != 0))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  return AURA_FFI_OK;
}

void aura_ffi_string_destroy(AuraFfiString *value)
{
  if (value == NULL)
  {
    return;
  }
  free(value->data);
  value->data = NULL;
  value->len = 0;
}

AuraFfiStatus aura_ffi_array_borrow(const void *data, uint64_t len,
                                    uint64_t cap, uint64_t elem_size,
                                    AuraFfiArrayKind kind,
                                    AuraFfiArrayView *out)
{
  if (out == NULL || (data == NULL && len != 0) ||
      !aura_ffi_array_shape_ok(len, cap, elem_size, kind))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  out->cap = cap;
  out->elem_size = elem_size;
  out->kind = kind;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_array_copy(AuraFfiArrayView view, AuraFfiArray *out)
{
  if (out == NULL || (view.data == NULL && view.len != 0) ||
      !aura_ffi_array_shape_ok(view.len, view.len, view.elem_size, view.kind) ||
      (view.elem_size != 0 && view.len > (uint64_t)(SIZE_MAX / view.elem_size)))
  {
    return AURA_FFI_INVALID;
  }
  size_t bytes = (size_t)view.len * (size_t)view.elem_size;
  void *copy = bytes == 0 ? NULL : malloc(bytes);
  if (bytes != 0 && copy == NULL)
  {
    return AURA_FFI_OOM;
  }
  if (bytes != 0)
  {
    memcpy(copy, view.data, bytes);
  }
  out->data = copy;
  out->len = view.len;
  out->cap = view.len;
  out->elem_size = view.elem_size;
  out->kind = view.kind;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_array_transfer(void *data, uint64_t len, uint64_t cap,
                                      uint64_t elem_size, AuraFfiArrayKind kind,
                                      AuraFfiArray *out)
{
  if (out == NULL || (data == NULL && len != 0) ||
      !aura_ffi_array_shape_ok(len, cap, elem_size, kind))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  out->cap = cap;
  out->elem_size = elem_size;
  out->kind = kind;
  return AURA_FFI_OK;
}

void aura_ffi_array_destroy(AuraFfiArray *value)
{
  if (value == NULL)
  {
    return;
  }
  free(value->data);
  value->data = NULL;
  value->len = 0;
  value->cap = 0;
  value->elem_size = 0;
  value->kind = 0;
}

AuraFfiStatus aura_ffi_root_begin(AuraFfiRootGuard *guard, void **slot)
{
  if (guard == NULL || slot == NULL || guard->active)
  {
    return AURA_FFI_INVALID;
  }
  aura_gc_add_root(slot);
  guard->slot = slot;
  guard->active = 1;
  return AURA_FFI_OK;
}

void aura_ffi_root_end(AuraFfiRootGuard *guard)
{
  if (guard == NULL || !guard->active)
  {
    return;
  }
  aura_gc_remove_root(guard->slot);
  guard->slot = NULL;
  guard->active = 0;
}

/* ---- F4 opaque foreign-resource handle ABI ---- */

struct AuraFfiOpaqueHandle
{
  void *resource;
  AuraFfiHandleDestroyFn destroy;
  uint64_t generation;
  size_t pins;
  uint32_t owners;
  int nullable;
  int released;
  int destroyed;
};

static void aura_ffi_handle_finish(AuraFfiOpaqueHandle *handle)
{
  if (handle == NULL || !handle->released || handle->pins != 0 ||
      handle->destroyed)
  {
    return;
  }
  void *resource = handle->resource;
  handle->resource = NULL;
  handle->destroyed = 1;
  if (handle->destroy != NULL && resource != NULL)
  {
    handle->destroy(resource);
  }
}

static void aura_ffi_handle_free_if_unowned(AuraFfiOpaqueHandle *handle)
{
  if (handle != NULL && handle->released && handle->pins == 0 &&
      handle->owners == 0)
  {
    free(handle);
  }
}

static AuraFfiStatus aura_ffi_handle_new_impl(void *resource,
                                               AuraFfiHandleDestroyFn destroy,
                                               int nullable,
                                               AuraFfiOpaqueHandle **out)
{
  if (out == NULL || (!nullable && resource == NULL))
  {
    return AURA_FFI_INVALID;
  }
  *out = NULL;
  AuraFfiOpaqueHandle *handle =
      (AuraFfiOpaqueHandle *)calloc(1, sizeof(*handle));
  if (handle == NULL)
  {
    return AURA_FFI_OOM;
  }
  handle->resource = resource;
  handle->destroy = destroy;
  handle->generation = 1;
  handle->owners = 1;
  handle->nullable = nullable;
  *out = handle;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_new(void *resource,
                                  AuraFfiHandleDestroyFn destroy,
                                  AuraFfiOpaqueHandle **out)
{
  return aura_ffi_handle_new_impl(resource, destroy, 0, out);
}

AuraFfiStatus aura_ffi_handle_new_nullable(void *resource,
                                            AuraFfiHandleDestroyFn destroy,
                                            AuraFfiOpaqueHandle **out)
{
  return aura_ffi_handle_new_impl(resource, destroy, 1, out);
}

int aura_ffi_handle_is_null(const AuraFfiOpaqueHandle *handle)
{
  return handle == NULL || handle->released || handle->resource == NULL;
}

AuraFfiStatus aura_ffi_handle_pin(AuraFfiOpaqueHandle *handle,
                                  AuraFfiHandlePin *out)
{
  if (out == NULL)
  {
    return AURA_FFI_INVALID;
  }
  memset(out, 0, sizeof(*out));
  if (handle == NULL || handle->released || handle->resource == NULL ||
      handle->destroyed)
  {
    return AURA_FFI_INVALID;
  }
  handle->pins++;
  out->handle = handle;
  out->resource = handle->resource;
  out->generation = handle->generation;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_pin_for_boundary(AuraFfiOpaqueHandle *handle,
                                               AuraFfiBoundary boundary,
                                               AuraFfiHandlePin *out)
{
  if (boundary != AURA_FFI_BOUNDARY_SYNC &&
      boundary != AURA_FFI_BOUNDARY_TASK &&
      boundary != AURA_FFI_BOUNDARY_AWAIT)
  {
    if (out != NULL)
    {
      memset(out, 0, sizeof(*out));
    }
    return AURA_FFI_BOUNDARY_REJECTED;
  }
  return aura_ffi_handle_pin(handle, out);
}

AuraFfiStatus aura_ffi_handle_retain(AuraFfiOpaqueHandle *handle)
{
  if (handle == NULL || handle->released || handle->destroyed ||
      handle->owners == UINT32_MAX)
  {
    return AURA_FFI_INVALID;
  }
  handle->owners++;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_pin_resource(const AuraFfiHandlePin *pin,
                                           void **out_resource)
{
  if (out_resource == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = NULL;
  if (pin == NULL || pin->handle == NULL || pin->resource == NULL ||
      pin->handle->released || pin->handle->destroyed ||
      pin->handle->generation != pin->generation ||
      pin->handle->resource != pin->resource)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = pin->resource;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_unpin(AuraFfiHandlePin *pin)
{
  if (pin == NULL || pin->handle == NULL || pin->resource == NULL ||
      pin->generation != pin->handle->generation || pin->handle->pins == 0)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiOpaqueHandle *handle = pin->handle;
  handle->pins--;
  memset(pin, 0, sizeof(*pin));
  aura_ffi_handle_finish(handle);
  aura_ffi_handle_free_if_unowned(handle);
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_release(AuraFfiOpaqueHandle *handle)
{
  if (handle == NULL || handle->released || handle->destroyed)
  {
    return AURA_FFI_INVALID;
  }
  handle->released = 1;
  aura_ffi_handle_finish(handle);
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_invalidate(AuraFfiOpaqueHandle *handle)
{
  return aura_ffi_handle_release(handle);
}

AuraFfiStatus aura_ffi_handle_destroy(AuraFfiOpaqueHandle **handle)
{
  if (handle == NULL || *handle == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiOpaqueHandle *value = *handle;
  if (!value->released)
  {
    return AURA_FFI_INVALID;
  }
  if (value->pins != 0)
  {
    return AURA_FFI_BUSY;
  }
  if (value->owners == 0)
  {
    return AURA_FFI_INVALID;
  }
  value->owners--;
  if (value->owners == 0)
  {
    aura_ffi_handle_finish(value);
    free(value);
  }
  *handle = NULL;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_drop(AuraFfiOpaqueHandle **handle)
{
  if (handle == NULL || *handle == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiOpaqueHandle *value = *handle;
  if (value->owners == 0)
  {
    return AURA_FFI_INVALID;
  }
  value->owners--;
  if (value->owners == 0)
  {
    value->released = 1;
    aura_ffi_handle_finish(value);
    /* A task pin may outlive the lexical owner; unpin performs the final free. */
    aura_ffi_handle_free_if_unowned(value);
  }
  *handle = NULL;
  return AURA_FFI_OK;
}

/* Consume the sole owner of a handle without running its destructor.  This is
 * used only to move an accepted stream into its owning HTTP connection. */
AuraFfiStatus aura_ffi_handle_take_owned(AuraFfiOpaqueHandle **handle,
                                         void **out_resource)
{
  AuraFfiOpaqueHandle *value;
  if (handle == NULL || *handle == NULL || out_resource == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = NULL;
  value = *handle;
  if (value->released || value->destroyed || value->resource == NULL ||
      value->pins != 0 || value->owners != 1)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = value->resource;
  value->resource = NULL;
  value->destroyed = 1;
  return aura_ffi_handle_drop(handle);
}

AuraFfiStatus aura_ffi_handle_check_boundary(const AuraFfiOpaqueHandle *handle,
                                             AuraFfiBoundary boundary)
{
  if (handle == NULL || handle->released || handle->destroyed)
  {
    return AURA_FFI_INVALID;
  }
  /* Nullable handles may cross a synchronous call as an explicit null value,
   * but task/await pinning requires a live resource to retain. */
  if (handle->resource == NULL &&
      (boundary == AURA_FFI_BOUNDARY_TASK ||
       boundary == AURA_FFI_BOUNDARY_AWAIT))
  {
    return AURA_FFI_INVALID;
  }
  return boundary == AURA_FFI_BOUNDARY_SYNC ||
                 boundary == AURA_FFI_BOUNDARY_TASK ||
                 boundary == AURA_FFI_BOUNDARY_AWAIT
             ? AURA_FFI_OK
             : AURA_FFI_BOUNDARY_REJECTED;
}

/* ---- F5 bounded callback and foreign-outcome ABI ---- */

struct AuraFfiCallbackFrame
{
  uint64_t owner_task;
  size_t registrations;
  int valid;
};

struct AuraFfiCallback
{
  AuraFfiCallbackFrame *frame;
  AuraFfiCallbackFn callback;
  void *environment;
  AuraFfiCallbackEnvDestroyFn environment_destroy;
  int registered;
  int dispatching;
};

AuraFfiOutcome aura_ffi_map_error(int32_t foreign_code)
{
  switch (foreign_code)
  {
  case 0:
    return AURA_FFI_OUTCOME_OK;
  case 1:
    return AURA_FFI_OUTCOME_CANCELLED;
  case 2:
    return AURA_FFI_OUTCOME_INVALID;
  case 3:
    return AURA_FFI_OUTCOME_NOT_FOUND;
  case 4:
    return AURA_FFI_OUTCOME_PERMISSION;
  case 5:
    return AURA_FFI_OUTCOME_UNAVAILABLE;
  case 6:
    return AURA_FFI_OUTCOME_TIMEOUT;
  default:
    return AURA_FFI_OUTCOME_FOREIGN_ERROR;
  }
}

AuraFfiStatus aura_ffi_callback_frame_new(uint64_t owner_task,
                                          AuraFfiCallbackFrame **out)
{
  if (out == NULL || owner_task == 0)
  {
    return AURA_FFI_INVALID;
  }
  *out = NULL;
  AuraFfiCallbackFrame *frame =
      (AuraFfiCallbackFrame *)calloc(1, sizeof(*frame));
  if (frame == NULL)
  {
    return AURA_FFI_OOM;
  }
  frame->owner_task = owner_task;
  frame->valid = 1;
  *out = frame;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_frame_invalidate(AuraFfiCallbackFrame *frame)
{
  if (frame == NULL || !frame->valid)
  {
    return AURA_FFI_INVALID;
  }
  frame->valid = 0;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_frame_destroy(AuraFfiCallbackFrame **frame)
{
  if (frame == NULL || *frame == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiCallbackFrame *value = *frame;
  if (value->registrations != 0)
  {
    return AURA_FFI_BUSY;
  }
  free(value);
  *frame = NULL;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_register(
    AuraFfiCallbackFrame *frame, AuraFfiCallbackFn callback, void *environment,
    AuraFfiCallbackEnvDestroyFn environment_destroy, AuraFfiCallback **out)
{
  if (out == NULL || frame == NULL || !frame->valid || callback == NULL ||
      environment == NULL || environment_destroy == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *out = NULL;
  AuraFfiCallback *registration =
      (AuraFfiCallback *)calloc(1, sizeof(*registration));
  if (registration == NULL)
  {
    return AURA_FFI_OOM;
  }
  registration->frame = frame;
  registration->callback = callback;
  registration->environment = environment;
  registration->environment_destroy = environment_destroy;
  registration->registered = 1;
  frame->registrations++;
  *out = registration;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_invoke(AuraFfiCallback *registration,
                                       uint64_t current_task,
                                       AuraFfiBoundary boundary,
                                       const void *payload,
                                       uint64_t payload_len,
                                       AuraFfiOutcome *outcome)
{
  if (outcome == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *outcome = AURA_FFI_OUTCOME_FOREIGN_ERROR;
  if (registration == NULL || !registration->registered ||
      registration->frame == NULL || !registration->frame->valid ||
      registration->callback == NULL ||
      (payload == NULL && payload_len != 0))
  {
    return AURA_FFI_INVALID;
  }
  if (boundary != AURA_FFI_BOUNDARY_SYNC ||
      current_task != registration->frame->owner_task)
  {
    return AURA_FFI_BOUNDARY_REJECTED;
  }
  if (registration->dispatching)
  {
    return AURA_FFI_BUSY;
  }
  registration->dispatching = 1;
  int32_t foreign_code = registration->callback(
      registration->environment, payload, payload_len);
  registration->dispatching = 0;
  *outcome = aura_ffi_map_error(foreign_code);
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_deregister(AuraFfiCallback *registration)
{
  if (registration == NULL || !registration->registered)
  {
    return AURA_FFI_INVALID;
  }
  if (registration->dispatching)
  {
    return AURA_FFI_BUSY;
  }
  registration->registered = 0;
  if (registration->environment_destroy != NULL &&
      registration->environment != NULL)
  {
    registration->environment_destroy(registration->environment);
  }
  registration->environment = NULL;
  registration->environment_destroy = NULL;
  if (registration->frame != NULL && registration->frame->registrations != 0)
  {
    registration->frame->registrations--;
  }
  registration->frame = NULL;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_shutdown(AuraFfiCallback *registration)
{
  return aura_ffi_callback_deregister(registration);
}

AuraFfiStatus aura_ffi_callback_destroy(AuraFfiCallback **registration)
{
  if (registration == NULL || *registration == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiCallback *value = *registration;
  if (value->registered || value->dispatching)
  {
    return AURA_FFI_BUSY;
  }
  free(value);
  *registration = NULL;
  return AURA_FFI_OK;
}

/* ---- C22j task-frame ABI (single-threaded MVP) ----
 *
 * A task frame is an opaque, heap-owned state machine object.  The poll
 * callback owns the state transition; it may retain frame_data across a
 * pending return.  A frame owns its result payload and invokes result_destroy
 * exactly once when the frame is destroyed.  The optional frame_destroy hook
 * runs before frame_data is freed and is the place for state-machine-specific
 * cleanup.  The context pointer is borrowed by the runtime and is never
 * freed.
 *
 * This ABI deliberately has no scheduler or channel dependency.  C22k adds
 * the executor that drives these callbacks.
 */

#define AURA_RT_ABI_VERSION 1u
#define AURA_RT_ABI_ID "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1"

uint32_t aura_runtime_abi_version(void)
{
  return AURA_RT_ABI_VERSION;
}

const char *aura_runtime_abi_identity(void)
{
  return AURA_RT_ABI_ID;
}

int aura_runtime_check_abi(uint32_t expected_version, const char *expected_identity)
{
  const char *available = aura_runtime_abi_identity();
  if (expected_version == aura_runtime_abi_version() &&
      expected_identity != NULL && strcmp(expected_identity, available) == 0)
  {
    return 1;
  }
  fprintf(stderr,
          "aura: runtime ABI mismatch: expected version %u identity %s, available version %u identity %s\n",
          expected_version,
          expected_identity ? expected_identity : "(missing)",
          aura_runtime_abi_version(),
          available);
  return 0;
}

/* ---- R1/R2 deterministic race event model ----
 *
 * The current executor is single-threaded, so this tracker records the total
 * order that a future concurrent detector will refine into vector clocks.
 * Every event carries task, address, and source identity for stable reports.
 */
typedef enum
{
  AURA_RACE_READ = 0,
  AURA_RACE_WRITE = 1,
  AURA_RACE_TASK_SPAWN = 2,
  AURA_RACE_TASK_JOIN = 3,
  AURA_RACE_SYNC_ACQUIRE = 4,
  AURA_RACE_SYNC_RELEASE = 5,
  AURA_RACE_TASK_COMPLETE = 6,
  AURA_RACE_TASK_FAILED = 7,
  AURA_RACE_TASK_CANCELLED = 8,
  AURA_RACE_CHANNEL_SEND = 9,
  AURA_RACE_CHANNEL_RECEIVE = 10,
  AURA_RACE_CHANNEL_CLOSE = 11
} AuraRaceEventKind;

typedef struct
{
  uint64_t sequence;
  uint64_t task_id;
  uint64_t stack_id;
  uintptr_t address;
  uint32_t source_id;
  AuraRaceEventKind kind;
} AuraRaceEvent;

typedef struct
{
  uint64_t identity;
  AuraRaceEvent first;
  AuraRaceEvent second;
  const char *missing_synchronization;
} AuraRaceReport;

typedef struct
{
  AuraRaceEvent *events;
  size_t count;
  size_t capacity;
  uint64_t clock;
} AuraRaceTracker;

/* R3 compiler instrumentation is deliberately process-local and opt-in.
 * Generated development/test binaries install a tracker here; ordinary
 * binaries leave it NULL, so the instrumentation helpers are no-ops. */
static AuraRaceTracker *aura_race_active_tracker = NULL;
static uint64_t aura_race_active_task_id = 0;
static uint32_t aura_race_active_source_id = 0;
static uint64_t aura_race_active_stack_id = 0;

static int aura_race_tracker_record_internal(AuraRaceTracker *tracker,
                                             uint64_t task_id,
                                             uintptr_t address,
                                             uint32_t source_id,
                                             AuraRaceEventKind kind);

void aura_race_tracker_set_active(AuraRaceTracker *tracker)
{
  aura_race_active_tracker = tracker;
  aura_race_active_task_id = 0;
  aura_race_active_source_id = 0;
  aura_race_active_stack_id = 0;
}

void aura_race_set_source_id(uint32_t source_id)
{
  aura_race_active_source_id = source_id;
}

void aura_race_set_stack_id(uint64_t stack_id)
{
  aura_race_active_stack_id = stack_id;
}

void aura_race_record_access(uintptr_t address,
                             uint32_t source_id,
                             AuraRaceEventKind kind)
{
  if (aura_race_active_tracker == NULL ||
      (kind != AURA_RACE_READ && kind != AURA_RACE_WRITE))
  {
    return;
  }
  (void)aura_race_tracker_record_internal(aura_race_active_tracker,
                                          aura_race_active_task_id,
                                          address,
                                          source_id,
                                          kind);
}

AuraRaceTracker *aura_race_tracker_new(void)
{
  AuraRaceTracker *tracker = (AuraRaceTracker *)calloc(1, sizeof(*tracker));
  if (tracker == NULL)
  {
    return NULL;
  }
  tracker->capacity = 16;
  tracker->events = (AuraRaceEvent *)calloc(tracker->capacity, sizeof(*tracker->events));
  if (tracker->events == NULL)
  {
    free(tracker);
    return NULL;
  }
  return tracker;
}

void aura_race_tracker_destroy(AuraRaceTracker *tracker)
{
  if (tracker != NULL)
  {
    free(tracker->events);
    free(tracker);
  }
}

void aura_race_tracker_reset(AuraRaceTracker *tracker)
{
  if (tracker != NULL)
  {
    tracker->count = 0;
    tracker->clock = 0;
  }
}

int aura_race_tracker_record(AuraRaceTracker *tracker,
                             uint64_t task_id,
                             uintptr_t address,
                             uint32_t source_id,
                             AuraRaceEventKind kind,
                             AuraRaceEvent *out)
{
  if (tracker == NULL)
  {
    return 0;
  }
  if (tracker->count == tracker->capacity)
  {
    size_t next_capacity = tracker->capacity * 2;
    AuraRaceEvent *next = (AuraRaceEvent *)realloc(
        tracker->events, next_capacity * sizeof(*tracker->events));
    if (next == NULL)
    {
      return 0;
    }
    tracker->events = next;
    tracker->capacity = next_capacity;
  }
  AuraRaceEvent event = {++tracker->clock, task_id, 0, address, source_id, kind};
  tracker->events[tracker->count++] = event;
  if (out != NULL)
  {
    *out = event;
  }
  return 1;
}

static uint64_t aura_race_hash_u64(uint64_t hash, uint64_t value)
{
  for (unsigned int shift = 0; shift < 64; shift += 8)
  {
    hash ^= (value >> shift) & UINT64_C(0xff);
    hash *= UINT64_C(1099511628211);
  }
  return hash;
}

static int aura_race_is_access(const AuraRaceEvent *event)
{
  return event != NULL &&
         (event->kind == AURA_RACE_READ || event->kind == AURA_RACE_WRITE);
}

static int aura_race_is_conflicting(const AuraRaceEvent *first,
                                    const AuraRaceEvent *second)
{
  return aura_race_is_access(first) && aura_race_is_access(second) &&
         first->address == second->address && first->task_id != second->task_id &&
         (first->kind == AURA_RACE_WRITE || second->kind == AURA_RACE_WRITE);
}

/* Alpha's bounded synchronization model: an observed join, lock hand-off, or
 * channel hand-off between the two accesses is a sufficient edge.  The
 * executor is deterministic, so this deliberately avoids wall-clock state. */
static const char *aura_race_missing_sync(const AuraRaceTracker *tracker,
                                          size_t first,
                                          size_t second)
{
  int saw_release = 0;
  int saw_send = 0;
  for (size_t i = first + 1; i < second; ++i)
  {
    const AuraRaceEvent *event = &tracker->events[i];
    if (event->kind == AURA_RACE_TASK_JOIN)
    {
      return NULL;
    }
    if (event->kind == AURA_RACE_SYNC_RELEASE && event->address != 0)
    {
      saw_release = 1;
    }
    else if (event->kind == AURA_RACE_SYNC_ACQUIRE && saw_release &&
             event->address != 0)
    {
      return NULL;
    }
    else if (event->kind == AURA_RACE_CHANNEL_SEND && event->address != 0)
    {
      saw_send = 1;
    }
    else if (event->kind == AURA_RACE_CHANNEL_RECEIVE && saw_send &&
             event->address != 0)
    {
      return NULL;
    }
  }
  return "no join, lock hand-off, or channel hand-off was observed";
}

static uint64_t aura_race_report_identity(const AuraRaceEvent *first,
                                          const AuraRaceEvent *second)
{
  /* Do not hash sequence numbers or raw addresses: both are run-local. */
  uint64_t a = first->source_id;
  uint64_t b = second->source_id;
  uint64_t sa = first->stack_id;
  uint64_t sb = second->stack_id;
  AuraRaceEventKind ka = first->kind;
  AuraRaceEventKind kb = second->kind;
  if (a > b || (a == b && (sa > sb || (sa == sb && ka > kb))))
  {
    uint64_t tmp = a; a = b; b = tmp;
    tmp = sa; sa = sb; sb = tmp;
    AuraRaceEventKind kt = ka; ka = kb; kb = kt;
  }
  uint64_t hash = UINT64_C(1469598103934665603);
  hash = aura_race_hash_u64(hash, UINT64_C(1));
  hash = aura_race_hash_u64(hash, a);
  hash = aura_race_hash_u64(hash, b);
  hash = aura_race_hash_u64(hash, sa);
  hash = aura_race_hash_u64(hash, sb);
  hash = aura_race_hash_u64(hash, (uint64_t)ka);
  return aura_race_hash_u64(hash, (uint64_t)kb);
}

static int aura_race_report_candidate(const AuraRaceTracker *tracker,
                                      size_t wanted,
                                      AuraRaceReport *out)
{
  uint64_t best = UINT64_MAX;
  size_t best_first = 0;
  size_t best_second = 0;
  for (size_t i = 0; i < tracker->count; ++i)
  {
    for (size_t j = i + 1; j < tracker->count; ++j)
    {
      if (!aura_race_is_conflicting(&tracker->events[i], &tracker->events[j]) ||
          aura_race_missing_sync(tracker, i, j) == NULL)
      {
        continue;
      }
      uint64_t identity = aura_race_report_identity(&tracker->events[i],
                                                    &tracker->events[j]);
      int duplicate = 0;
      for (size_t p = 0; p < i && !duplicate; ++p)
      {
        for (size_t q = p + 1; q < tracker->count; ++q)
        {
          if (aura_race_is_conflicting(&tracker->events[p], &tracker->events[q]) &&
              aura_race_missing_sync(tracker, p, q) != NULL &&
              aura_race_report_identity(&tracker->events[p], &tracker->events[q]) == identity)
          {
            duplicate = 1;
            break;
          }
        }
      }
      if (duplicate)
      {
        continue;
      }
      if (identity < best)
      {
        best = identity;
        best_first = i;
        best_second = j;
      }
    }
  }
  if (best == UINT64_MAX)
  {
    return 0;
  }
  /* Select the wanted item by repeatedly masking the chosen identity. */
  if (wanted != 0)
  {
    AuraRaceTracker copy = *tracker;
    (void)copy;
    /* The public alpha API is intentionally bounded to the first report;
     * callers use report_count to observe whether any conflict exists. */
    return 0;
  }
  out->identity = best;
  out->first = tracker->events[best_first];
  out->second = tracker->events[best_second];
  out->missing_synchronization = aura_race_missing_sync(tracker, best_first, best_second);
  return 1;
}

size_t aura_race_tracker_report_count(const AuraRaceTracker *tracker)
{
  AuraRaceReport report;
  return tracker != NULL && aura_race_report_candidate(tracker, 0, &report) ? 1 : 0;
}

int aura_race_tracker_report(const AuraRaceTracker *tracker,
                             size_t index,
                             AuraRaceReport *out)
{
  if (tracker == NULL || out == NULL || index != 0)
  {
    return 0;
  }
  return aura_race_report_candidate(tracker, index, out);
}

static const char *aura_race_kind_name(AuraRaceEventKind kind)
{
  return kind == AURA_RACE_READ ? "read" : "write";
}

int aura_race_report_write_human(const AuraRaceReport *report, FILE *out)
{
  if (report == NULL || out == NULL)
  {
    return 0;
  }
  return fprintf(out,
                 "race[%016" PRIx64 "] %s(task=%" PRIu64 ",stack=%" PRIu64 ",source=%" PRIu32 ") <-> %s(task=%" PRIu64 ",stack=%" PRIu64 ",source=%" PRIu32 "); missing synchronization: %s\n",
                 report->identity, aura_race_kind_name(report->first.kind),
                 report->first.task_id, report->first.stack_id, report->first.source_id,
                 aura_race_kind_name(report->second.kind), report->second.task_id,
                 report->second.stack_id, report->second.source_id,
                 report->missing_synchronization) >= 0;
}

int aura_race_report_write_json(const AuraRaceReport *report, FILE *out)
{
  if (report == NULL || out == NULL)
  {
    return 0;
  }
  return fprintf(out,
                 "{\"identity\":\"%016" PRIx64 "\",\"first\":{\"kind\":\"%s\",\"task\":%" PRIu64 ",\"stack\":%" PRIu64 ",\"source\":%" PRIu32 "},\"second\":{\"kind\":\"%s\",\"task\":%" PRIu64 ",\"stack\":%" PRIu64 ",\"source\":%" PRIu32 "},\"missing_synchronization\":\"%s\"}\n",
                 report->identity, aura_race_kind_name(report->first.kind),
                 report->first.task_id, report->first.stack_id, report->first.source_id,
                 aura_race_kind_name(report->second.kind), report->second.task_id,
                 report->second.stack_id, report->second.source_id,
                 report->missing_synchronization) >= 0;
}

static int aura_race_tracker_record_internal(AuraRaceTracker *tracker,
                                              uint64_t task_id,
                                              uintptr_t address,
                                              uint32_t source_id,
                                              AuraRaceEventKind kind)
{
  return aura_race_tracker_record(tracker, task_id, address, source_id, kind, NULL);
}

size_t aura_race_tracker_count(const AuraRaceTracker *tracker)
{
  return tracker != NULL ? tracker->count : 0;
}

const AuraRaceEvent *aura_race_tracker_event(const AuraRaceTracker *tracker, size_t index)
{
  if (tracker == NULL || index >= tracker->count)
  {
    return NULL;
  }
  return &tracker->events[index];
}

int aura_race_happens_before(const AuraRaceEvent *before, const AuraRaceEvent *after)
{
  return before != NULL && after != NULL && before->sequence < after->sequence;
}

typedef struct AuraTaskFrame AuraTaskFrame;
typedef struct AuraTaskExecutor AuraTaskExecutor;
typedef struct AuraTaskChannel AuraTaskChannel;
typedef struct AuraTaskSelect AuraTaskSelect;

typedef void (*AuraTaskResultDestroyFn)(void *data, size_t size);
typedef void *(*AuraTaskResultCloneFn)(const void *data, size_t size,
                                       size_t *cloned_size);
typedef AuraTaskPollState (*AuraTaskPollFn)(AuraTaskFrame *frame);
typedef AuraTaskPollState (*AuraTaskCancelFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskFrameDestroyFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskCleanupFn)(void *data);

typedef enum
{
  AURA_TASK_OWNED = 0,
  AURA_TASK_BORROWED = 1,
  AURA_TASK_PINNED = 2,
  AURA_TASK_SHARED = 3,
  AURA_TASK_TRANSFERRED = 4
} AuraTaskOwnership;

/* C22m: callback used for the currently supported `spawn {}` unit slice.
 * Non-empty spawned bodies still require the C22l suspension/capture lowering. */
AuraTaskPollState aura_task_poll_unit(AuraTaskFrame *frame)
{
  (void)frame;
  return AURA_TASK_COMPLETE;
}

typedef struct
{
  void *data;
  size_t size;
} AuraTaskResult;

/* A join observation is a borrowed, immutable snapshot of a terminal frame.
 * The state is authoritative: result is populated only for COMPLETE and
 * error is populated only for FAILED.  Neither payload is transferred by
 * this API; both remain owned by the frame until its handle is released or
 * the executor shuts down.  This makes repeated observations safe while
 * making use-after-release an explicit caller error. */
typedef struct
{
  AuraTaskPollState state;
  AuraTaskResult result;
  AuraTaskResult error;
} AuraTaskOutcome;

/* An owned copy of a terminal outcome.  Unlike AuraTaskOutcome, this value
 * remains valid after the task frame is released.  The caller supplies clone
 * and destroy functions because the runtime cannot infer payload ownership. */
typedef struct
{
  AuraTaskPollState state;
  AuraTaskResult result;
  AuraTaskResult error;
  AuraTaskResultDestroyFn result_destroy;
  AuraTaskResultDestroyFn error_destroy;
} AuraTaskOwnedOutcome;

int aura_task_outcome_clone(const AuraTaskOutcome *source,
                            AuraTaskResultCloneFn result_clone,
                            AuraTaskResultDestroyFn result_destroy,
                            AuraTaskResultCloneFn error_clone,
                            AuraTaskResultDestroyFn error_destroy,
                            AuraTaskOwnedOutcome *out);
void aura_task_owned_outcome_destroy(AuraTaskOwnedOutcome *out);

typedef struct
{
  uint64_t task_id;
  uint32_t source_id;
  AuraTaskPollState state;
  AuraTaskResult error;
} AuraTaskFailureDiagnostic;

typedef void (*AuraTaskFailureHookFn)(
    const AuraTaskFailureDiagnostic *diagnostic, void *context);

typedef struct
{
  void *data;
  size_t size;
  AuraTaskResultDestroyFn destroy;
  AuraTaskOwnership ownership;
  int rooted;
} AuraTaskFrameStorage;

typedef struct
{
  void *data;
  AuraTaskCleanupFn cleanup;
} AuraTaskFrameCleanup;

typedef struct AuraTaskFfiPin AuraTaskFfiPin;

struct AuraTaskFrame
{
  uint32_t abi_version;
  uint64_t task_id;
  uint32_t race_source_id;
  AuraTaskPollFn poll;
  AuraTaskCancelFn cancel;
  AuraTaskFrameDestroyFn destroy;
  void *data;
  size_t data_size;
  AuraTaskResult result;
  AuraTaskResultDestroyFn result_destroy;
  int result_rooted;
  AuraTaskFrameStorage captures;
  AuraTaskFrameStorage pending;
  AuraTaskFrameCleanup cleanup;
  AuraTaskResult error;
  AuraTaskResultCloneFn error_clone;
  AuraTaskResultDestroyFn error_destroy;
  int error_rooted;
  AuraTaskResult error_payload;
  AuraTaskResultCloneFn error_payload_clone;
  AuraTaskResultDestroyFn error_payload_destroy;
  int error_payload_rooted;
  char *error_type_name;
  uint32_t error_source_id;
  uint32_t error_span_start;
  uint32_t error_span_end;
  uint32_t resume_state;
  AuraTaskPollState state;
  int cancel_requested;
  int64_t cancel_deadline_ms;
  int join_observed;
  int failure_reported;
  int queued;
  AuraTaskExecutor *executor;
  AuraTaskFrame *queue_next;
  AuraTaskFrame *owned_next;
  AuraTaskChannel *waiting_channel;
  void *waiting_node;
  AuraTaskSelect *waiting_select;
  int fd_wait_fd;
  short fd_wait_events;
  int fd_wait_active;
  int64_t fd_wait_deadline_ms;
  int fd_wait_timed_out;
  AuraTaskFrame *wait_target;
  AuraTaskFrame *waiters_head;
  AuraTaskFrame *waiter_next;
  AuraTaskFrame *cancel_parent;
  AuraTaskFrame *cancel_children_head;
  AuraTaskFrame *cancel_sibling_next;
  AuraTaskFrameGcMarkFn gc_mark;
  AuraTaskFrame *gc_next;
  AuraTaskFfiPin *ffi_pins;
  AuraTaskScope *scope;
  AuraTaskFrame *scope_next;
  int scope_owned;
  int handle_owned;
#if defined(AURA_TCP_POSIX)
  pthread_t blocking_thread;
  pthread_mutex_t blocking_lock;
  AuraTaskBlockingFn blocking_fn;
  AuraTaskBlockingEnvDestroyFn blocking_env_destroy;
  void *blocking_env;
  int blocking_started;
  int blocking_thread_created;
  int blocking_done;
#endif
};

static AuraTaskFrame *aura_gc_task_frames = NULL;

static int aura_task_executor_has_workers(AuraTaskExecutor *executor);
int aura_task_executor_start_workers(AuraTaskExecutor *executor,
                                     size_t worker_count);

static void aura_task_frame_unlink_cancel_parent(AuraTaskFrame *frame);
static void aura_task_frame_detach_cancel_children(AuraTaskFrame *frame);
static void aura_task_frame_request_cancel_children(AuraTaskFrame *frame);

struct AuraTaskFfiPin
{
  AuraFfiHandlePin pin;
  AuraTaskFfiPin *next;
};

static void aura_task_frame_unpin_foreign_handles(AuraTaskFrame *frame)
{
  AuraTaskFfiPin *node;
  AuraTaskFfiPin *next;
  if (frame == NULL)
  {
    return;
  }
  node = frame->ffi_pins;
  frame->ffi_pins = NULL;
  while (node != NULL)
  {
    next = node->next;
    (void)aura_ffi_handle_unpin(&node->pin);
    free(node);
    node = next;
  }
}

AuraFfiStatus aura_task_frame_pin_foreign_handle(AuraTaskFrame *frame,
                                                 AuraFfiOpaqueHandle *handle,
                                                 AuraFfiBoundary boundary)
{
  AuraTaskFfiPin *node;
  AuraFfiStatus status;
  if (frame == NULL)
  {
    return AURA_FFI_INVALID;
  }
  node = (AuraTaskFfiPin *)calloc(1, sizeof(*node));
  if (node == NULL)
  {
    return AURA_FFI_OOM;
  }
  status = aura_ffi_handle_pin_for_boundary(handle, boundary, &node->pin);
  if (status != AURA_FFI_OK)
  {
    free(node);
    return status;
  }
  node->next = frame->ffi_pins;
  frame->ffi_pins = node;
  return AURA_FFI_OK;
}

static void aura_gc_mark_task_frames(void)
{
  /* Frame captures and pending payloads are allocator-owned storage rather
   * than tracing-heap nodes.  Scan their pointer-sized words conservatively
   * so a compiler frame remains a complete GC root even when it has no custom
   * mark callback.  Explicit callbacks still handle typed nested layouts and
   * remain the preferred precise path. */
  for (AuraTaskFrame *frame = aura_gc_task_frames; frame != NULL;
       frame = frame->gc_next)
  {
    const AuraTaskFrameStorage *storage[] = {&frame->captures,
                                             &frame->pending};
    for (size_t s = 0; s < sizeof(storage) / sizeof(storage[0]); s++)
    {
      const unsigned char *bytes = (const unsigned char *)storage[s]->data;
      size_t words = storage[s]->size / sizeof(void *);
      if (storage[s]->data != NULL)
      {
        aura_gc_mark_ptr(storage[s]->data);
      }
      for (size_t i = 0; bytes != NULL && i < words; i++)
      {
        void *candidate = NULL;
        memcpy(&candidate, bytes + i * sizeof(void *), sizeof(candidate));
        aura_gc_mark_ptr(candidate);
      }
    }
    /* Terminal outcomes are allocator-owned payloads too.  Scan their
     * pointer-sized words so a class/array payload remains live until the
     * owning task frame is released. */
    const AuraTaskResult *outcomes[] = {&frame->result, &frame->error};
    for (size_t o = 0; o < sizeof(outcomes) / sizeof(outcomes[0]); o++)
    {
      const unsigned char *bytes =
          (const unsigned char *)outcomes[o]->data;
      size_t words = outcomes[o]->size / sizeof(void *);
      if (outcomes[o]->data != NULL)
      {
        aura_gc_mark_ptr(outcomes[o]->data);
      }
      for (size_t i = 0; bytes != NULL && i < words; i++)
      {
        void *candidate = NULL;
        memcpy(&candidate, bytes + i * sizeof(candidate), sizeof(candidate));
        aura_gc_mark_ptr(candidate);
      }
    }
    if (frame->gc_mark != NULL)
    {
      frame->gc_mark(frame);
    }
  }
}

static void aura_gc_unlink_task_frame(AuraTaskFrame *frame)
{
  AuraTaskFrame **link = &aura_gc_task_frames;
  while (*link != NULL)
  {
    if (*link == frame)
    {
      *link = frame->gc_next;
      frame->gc_next = NULL;
      return;
    }
    link = &(*link)->gc_next;
  }
}

static void aura_task_frame_detach_wait_target(AuraTaskFrame *frame);
static void aura_task_frame_detach_waiters(AuraTaskFrame *frame);
static void aura_task_frame_wake_waiters(AuraTaskFrame *frame);

static uint64_t aura_task_next_id = 1;
int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame);
int aura_task_executor_submit(AuraTaskExecutor *executor, AuraTaskFrame *frame);
void aura_task_frame_destroy(AuraTaskFrame *frame);

AuraTaskFrame *aura_task_frame_new(size_t data_size,
                                   AuraTaskPollFn poll,
                                   AuraTaskFrameDestroyFn destroy)
{
  if (poll == NULL)
  {
    return NULL;
  }
  AuraTaskFrame *frame = (AuraTaskFrame *)calloc(1, sizeof(*frame));
  if (frame == NULL)
  {
    return NULL;
  }
  if (data_size != 0)
  {
    /* Frame locals are the suspended task's live state.  Store them in the
     * tracing heap so the collector can deep-scan GC pointers held by the
     * state while the task is pending. */
    frame->data = aura_gc_alloc(data_size);
    if (frame->data == NULL)
    {
      free(frame);
      return NULL;
    }
    aura_gc_add_root(&frame->data);
  }
  frame->abi_version = AURA_RT_ABI_VERSION;
  frame->task_id = aura_task_next_id++;
  frame->poll = poll;
  frame->destroy = destroy;
  frame->data_size = data_size;
  frame->resume_state = 0;
  frame->state = AURA_TASK_READY;
  frame->gc_next = aura_gc_task_frames;
  aura_gc_task_frames = frame;
  return frame;
}

#if defined(AURA_TCP_POSIX)
static AuraTaskPollState aura_task_blocking_poll(AuraTaskFrame *frame)
{
  int done;
  if (frame == NULL)
  {
    return AURA_TASK_FAILED;
  }
  pthread_mutex_lock(&frame->blocking_lock);
  done = frame->blocking_done;
  pthread_mutex_unlock(&frame->blocking_lock);
  return done ? AURA_TASK_COMPLETE : AURA_TASK_PENDING;
}

static void *aura_task_blocking_thread(void *context)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)context;
  frame->blocking_fn(frame, frame->blocking_env);
  pthread_mutex_lock(&frame->blocking_lock);
  frame->blocking_done = 1;
  pthread_mutex_unlock(&frame->blocking_lock);
  if (frame->executor != NULL)
  {
    (void)aura_task_executor_wake(frame->executor, frame);
  }
  return NULL;
}

static void aura_task_blocking_run_inline(AuraTaskFrame *frame)
{
  int cancelled;
  if (frame == NULL || frame->blocking_fn == NULL) return;
  pthread_mutex_lock(&frame->blocking_lock);
  cancelled = frame->cancel_requested;
  frame->blocking_started = 1;
  pthread_mutex_unlock(&frame->blocking_lock);
  if (!cancelled)
    frame->blocking_fn(frame, frame->blocking_env);
  pthread_mutex_lock(&frame->blocking_lock);
  frame->blocking_done = 1;
  pthread_mutex_unlock(&frame->blocking_lock);
}

AuraTaskFrame *aura_task_frame_new_blocking(
    AuraTaskExecutor *executor, AuraTaskBlockingFn function, void *environment,
    AuraTaskBlockingEnvDestroyFn environment_destroy)
{
  AuraTaskFrame *frame;
  if (executor == NULL || function == NULL)
  {
    return NULL;
  }
  frame = aura_task_frame_new(0, aura_task_blocking_poll, NULL);
  if (frame == NULL || pthread_mutex_init(&frame->blocking_lock, NULL) != 0)
  {
    aura_task_frame_destroy(frame);
    return NULL;
  }
  frame->blocking_fn = function;
  frame->blocking_env = environment;
  frame->blocking_env_destroy = environment_destroy;
#if defined(AURA_TCP_POSIX)
  if (!aura_task_executor_has_workers(executor))
    (void)aura_task_executor_start_workers(executor, 4);
#endif
  if (aura_task_executor_has_workers(executor))
  {
    frame->blocking_started = 0;
    frame->blocking_thread_created = 0;
  }
  else if (pthread_create(&frame->blocking_thread, NULL, aura_task_blocking_thread,
                          frame) != 0)
  {
    frame->blocking_started = 0;
    aura_task_frame_destroy(frame);
    return NULL;
  }
  else
  {
    frame->blocking_started = 1;
    frame->blocking_thread_created = 1;
  }
  if (!aura_task_executor_submit(executor, frame))
  {
    aura_task_frame_destroy(frame);
    return NULL;
  }
  return frame;
}
#endif

void aura_task_frame_set_gc_mark(AuraTaskFrame *frame,
                                 AuraTaskFrameGcMarkFn mark)
{
  if (frame != NULL)
  {
    frame->gc_mark = mark;
  }
}

void aura_task_frame_set_cancel_handler(AuraTaskFrame *frame,
                                        AuraTaskCancelFn cancel)
{
  if (frame != NULL)
  {
    frame->cancel = cancel;
  }
}

void *aura_task_frame_data(AuraTaskFrame *frame)
{
  return frame != NULL ? frame->data : NULL;
}

uint64_t aura_task_frame_task_id(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->task_id : 0;
}

void aura_task_frame_set_race_source_id(AuraTaskFrame *frame, uint32_t source_id)
{
  if (frame != NULL)
  {
    frame->race_source_id = source_id;
  }
}

AuraTaskPollState aura_task_frame_state(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->state : AURA_TASK_FAILED;
}

int aura_task_frame_cancel_requested(const AuraTaskFrame *frame)
{
  return frame != NULL && frame->cancel_requested;
}

int aura_task_frame_set_cancel_deadline(AuraTaskFrame *frame, int timeout_ms)
{
  int64_t now;
  if (frame == NULL || timeout_ms < 0 ||
      frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED) {
    return 0;
  }
  now = aura_time_monotonic_millis();
  if (now <= 0 || now > INT64_MAX - timeout_ms) return 0;
  frame->cancel_deadline_ms = now + timeout_ms;
  return 1;
}

int aura_task_frame_is_waiting(const AuraTaskFrame *frame)
{
  return frame != NULL && (frame->waiting_channel != NULL || frame->waiting_node != NULL);
}

/* Adapter-owned wait registration. The token is borrowed by the frame and
 * must remain valid until the adapter clears it; the frame never frees it.
 * Completion should clear the token before calling aura_task_executor_wake.
 * Cancellation and frame destruction use the separate cleanup hook for owned
 * operation resources. */
void aura_task_frame_set_waiting(AuraTaskFrame *frame, void *token)
{
  if (frame == NULL || frame->state == AURA_TASK_COMPLETE ||
      frame->state == AURA_TASK_FAILED || frame->state == AURA_TASK_CANCELLED)
  {
    return;
  }
  frame->waiting_node = token;
  frame->fd_wait_active = 0;
  if (token != NULL)
  {
    frame->state = AURA_TASK_PENDING;
  }
}

void aura_task_frame_clear_waiting(AuraTaskFrame *frame)
{
  if (frame != NULL)
  {
    frame->waiting_node = NULL;
    frame->fd_wait_active = 0;
    frame->fd_wait_deadline_ms = 0;
  }
}

int64_t aura_time_monotonic_millis(void)
{
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0)
  {
    return 0;
  }
  return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}

void *aura_task_frame_waiting_token(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->waiting_node : NULL;
}

/* Register one borrowed POSIX descriptor readiness wait on the frame. The
 * descriptor and event mask live inline in the executor-owned frame, so no
 * adapter token allocation can outlive cancellation or destruction. A later
 * aura_task_executor_poll_waiting call performs the bounded poll and wakes the
 * frame through the same clear-before-queue protocol as other adapters. */
int aura_task_frame_wait_fd(AuraTaskFrame *frame, int fd, short events)
{
  if (frame == NULL || fd < 0 || events == 0 || frame->state == AURA_TASK_COMPLETE ||
      frame->state == AURA_TASK_FAILED || frame->state == AURA_TASK_CANCELLED ||
      frame->waiting_channel != NULL || frame->wait_target != NULL)
  {
    return 0;
  }
  frame->fd_wait_fd = fd;
  frame->fd_wait_events = events;
  frame->fd_wait_active = 1;
  frame->fd_wait_deadline_ms = 0;
  frame->fd_wait_timed_out = 0;
  frame->waiting_node = &frame->fd_wait_active;
  frame->state = AURA_TASK_PENDING;
  return 1;
}

/* A readiness timeout wakes the frame instead of cancelling it.  The polling
 * operation consumes the timeout marker and chooses its typed outcome. */
int aura_task_frame_wait_fd_timeout(AuraTaskFrame *frame, int fd, short events,
                                    int timeout_ms)
{
  int64_t now;
  if (timeout_ms < 0 || !aura_task_frame_wait_fd(frame, fd, events))
  {
    return 0;
  }
  now = aura_time_monotonic_millis();
  if (now <= 0 || now > INT64_MAX - timeout_ms)
  {
    aura_task_frame_clear_waiting(frame);
    return 0;
  }
  frame->fd_wait_deadline_ms = now + timeout_ms;
  return 1;
}

int aura_task_frame_take_fd_wait_timeout(AuraTaskFrame *frame)
{
  int timed_out = frame != NULL && frame->fd_wait_timed_out;
  if (frame != NULL)
  {
    frame->fd_wait_timed_out = 0;
  }
  return timed_out;
}

/* Park a task until its monotonic deadline without occupying a descriptor.
 * The executor's normal readiness poll also services these timer waits. */
int aura_task_frame_wait_deadline(AuraTaskFrame *frame, int timeout_ms)
{
  int64_t now;
  if (frame == NULL || timeout_ms < 0 || frame->state == AURA_TASK_COMPLETE ||
      frame->state == AURA_TASK_FAILED || frame->state == AURA_TASK_CANCELLED ||
      frame->waiting_node != NULL || frame->waiting_channel != NULL ||
      frame->wait_target != NULL)
  {
    return 0;
  }
  now = aura_time_monotonic_millis();
  if (now <= 0 || now > INT64_MAX - timeout_ms)
  {
    return 0;
  }
  frame->fd_wait_active = 0;
  frame->fd_wait_deadline_ms = now + timeout_ms;
  frame->fd_wait_timed_out = 0;
  frame->waiting_node = &frame->fd_wait_timed_out;
  frame->state = AURA_TASK_PENDING;
  return 1;
}

int aura_task_frame_wait_file(AuraTaskFrame *frame, const AuraFile *file,
                              short events)
{
  if (file == NULL || file->closed)
  {
    return 0;
  }
  return aura_task_frame_wait_fd(frame, file->fd, events);
}

/* TCP adapters keep resource ownership in the existing listener/stream
 * objects and only borrow their nonblocking descriptor for this wait. */
int aura_task_frame_wait_tcp_listener(AuraTaskFrame *frame,
                                      const AuraTcpListener *listener,
                                      short events)
{
  if (listener == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_fd(frame, listener->fd, events);
}

int aura_task_frame_wait_tcp_stream(AuraTaskFrame *frame,
                                    const AuraTcpStream *stream,
                                    short events)
{
  if (stream == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_fd(frame, stream->fd, events);
}

int aura_task_frame_wait_tcp_stream_timeout(AuraTaskFrame *frame,
                                            const AuraTcpStream *stream,
                                            short events, int timeout_ms)
{
  if (stream == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_fd_timeout(frame, stream->fd, events, timeout_ms);
}

int aura_http_connection_wait_write(AuraTaskFrame *frame,
                                    const AuraHttpConnection *connection)
{
  if (connection == NULL || connection->stream == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_tcp_stream_timeout(
      frame, connection->stream, POLLOUT, connection->config.write_timeout_ms);
}

/* The body reader is connection-owned, so only it may choose the stream and
 * deadline used to resume a generated RequestBody task. */
int aura_http_request_wait_body(AuraTaskFrame *frame,
                                const AuraHttpRequest *request)
{
  AuraHttpContentLengthReader *reader;
  if (frame == NULL || request == NULL || request->body_reader == NULL)
  {
    return 0;
  }
  reader = request->body_reader;
  return aura_task_frame_wait_tcp_stream_timeout(
      frame, reader->stream, POLLIN, reader->timeout_ms);
}

enum
{
  AURA_HTTP_ASYNC_READ = 1,
  AURA_HTTP_ASYNC_WRITE = 2,
  AURA_HTTP_ASYNC_HANDLER = 3
};

void aura_task_frame_set_cleanup(AuraTaskFrame *frame, void *data,
                                 AuraTaskCleanupFn cleanup);
void aura_task_frame_clear_cleanup(AuraTaskFrame *frame);

static void aura_http_connection_async_reset(AuraHttpConnection *connection)
{
  if (connection == NULL)
  {
    return;
  }
  free(connection->async_buffer);
  connection->async_buffer = NULL;
  connection->async_used = 0;
  connection->async_capacity = 0;
  if (connection->async_response_active)
  {
    aura_http_response_destroy(&connection->async_response);
    connection->async_response_active = 0;
  }
  if (connection->async_request_active)
  {
    aura_http_request_destroy(&connection->async_request);
    connection->async_request_active = 0;
  }
  memset(&connection->async_body_reader, 0,
         sizeof(connection->async_body_reader));
  connection->async_body_reader_active = 0;
  free(connection->async_output);
  connection->async_output = NULL;
  connection->async_output_length = 0;
  connection->async_output_offset = 0;
  connection->async_handler = NULL;
  connection->async_task_handler = NULL;
  connection->async_user_data = NULL;
  connection->async_active = 0;
  connection->async_phase = 0;
  connection->async_close_after_write = 0;
  connection->async_handler_started = 0;
}

static void aura_http_connection_async_release_handle_pin(
    AuraHttpConnection *connection)
{
  if (connection != NULL && connection->async_handle_pin_active)
  {
    AuraFfiHandlePin pin = connection->async_handle_pin;
    /* Unpin may destroy the connection after the lexical owner was dropped;
     * clear connection state before invoking that destructor. */
    memset(&connection->async_handle_pin, 0,
           sizeof(connection->async_handle_pin));
    connection->async_handle_pin_active = 0;
    connection->async_handle_frame = NULL;
    (void)aura_ffi_handle_unpin(&pin);
  }
}

static void aura_http_connection_async_cleanup(void *data)
{
  AuraHttpConnection *connection = (AuraHttpConnection *)data;
  if (connection == NULL)
  {
    return;
  }
  (void)aura_http_connection_close(connection);
  aura_http_connection_async_reset(connection);
  aura_http_connection_async_release_handle_pin(connection);
}

static AuraTaskPollState aura_http_connection_async_failure(
    AuraTaskFrame *frame, AuraHttpConnection *connection)
{
  aura_task_frame_clear_cleanup(frame);
  (void)aura_http_connection_close(connection);
  aura_http_connection_async_reset(connection);
  aura_http_connection_async_release_handle_pin(connection);
  return AURA_TASK_FAILED;
}

static int aura_http_connection_async_prepare_response(AuraHttpConnection *connection,
                                                       const AuraHttpRequest *request)
{
  AuraHttpHandlerResult handler_result = AURA_HTTP_HANDLER_CLOSE;
  int request_close = aura_http_connection_header_has(request, "close");
  int status_code = 0;
  const char *error_code = NULL;
  size_t required = 0;
  AuraHttpResponseStatus response_status;

  aura_http_response_init(&connection->async_response);
  connection->async_response_active = 1;
  if (request == NULL)
  {
    status_code = 400;
    error_code = "bad_request";
  }
  else
  {
    if (!request_close && aura_http_response_set_connection(
                              &connection->async_response,
                              AURA_HTTP_RESPONSE_KEEP_ALIVE) != AURA_HTTP_RESPONSE_OK)
    {
      return 0;
    }
    handler_result = connection->async_handler(
        request, &connection->async_response, connection->async_user_data);
    if (handler_result == AURA_HTTP_HANDLER_ERROR)
    {
      status_code = 500;
      error_code = "handler_failure";
    }
  }
  if (status_code != 0)
  {
    aura_http_response_destroy(&connection->async_response);
    aura_http_response_init(&connection->async_response);
    response_status = aura_http_response_set_error(
        &connection->async_response, status_code, error_code);
    if (response_status != AURA_HTTP_RESPONSE_OK)
    {
      return 0;
    }
  }
  connection->async_close_after_write =
      status_code != 0 || request_close || handler_result == AURA_HTTP_HANDLER_CLOSE ||
      connection->async_response.connection == AURA_HTTP_RESPONSE_CLOSE ||
      connection->requests_served + 1 >= connection->config.max_requests;
  if (connection->async_close_after_write &&
      aura_http_response_set_connection(&connection->async_response,
                                        AURA_HTTP_RESPONSE_CLOSE) !=
          AURA_HTTP_RESPONSE_OK)
  {
    return 0;
  }
  response_status = aura_http_response_serialize(
      &connection->async_response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    return 0;
  }
  connection->async_output = (char *)malloc(required);
  if (connection->async_output == NULL)
  {
    return 0;
  }
  connection->async_output_length = required;
  response_status = aura_http_response_serialize(
      &connection->async_response, connection->async_output, required,
      &connection->async_output_length);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    return 0;
  }
  connection->async_output_offset = 0;
  connection->async_phase = AURA_HTTP_ASYNC_WRITE;
  return 1;
}

/* Application and request failures write one bounded response before closing.
 * The request stays connection-owned until that response has been drained. */
static AuraTaskPollState aura_http_connection_async_prepare_error_response(
    AuraHttpConnection *connection, int status_code, const char *error_code)
{
  size_t required = 0;

  if (connection == NULL)
  {
    return AURA_TASK_FAILED;
  }
  if (connection->async_response_active)
  {
    aura_http_response_destroy(&connection->async_response);
  }
  aura_http_response_init(&connection->async_response);
  connection->async_response_active = 1;
  if (aura_http_response_set_error(&connection->async_response, status_code,
                                   error_code) != AURA_HTTP_RESPONSE_OK ||
      aura_http_response_set_connection(&connection->async_response,
                                        AURA_HTTP_RESPONSE_CLOSE) !=
          AURA_HTTP_RESPONSE_OK ||
      aura_http_response_serialize(&connection->async_response, NULL, 0,
                                   &required) !=
          AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL ||
      required == 0)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output = (char *)malloc(required);
  if (connection->async_output == NULL ||
      aura_http_response_serialize(&connection->async_response,
                                   connection->async_output, required,
                                   &connection->async_output_length) !=
          AURA_HTTP_RESPONSE_OK)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output_offset = 0;
  connection->async_close_after_write = 1;
  connection->async_phase = AURA_HTTP_ASYNC_WRITE;
  return AURA_TASK_READY;
}

static AuraTaskPollState aura_http_connection_async_prepare_handler_failure(
    AuraHttpConnection *connection)
{
  return aura_http_connection_async_prepare_error_response(
      connection, 500, "handler_failure");
}

/* Prepare and resume a task-backed handler.  Request/response objects remain
 * connection-owned while this function returns PENDING, so a generated Aura
 * handler can suspend on any runtime readiness source without borrowing a
 * stack object across the suspension. */
static AuraTaskPollState aura_http_connection_async_run_task_handler(
    AuraTaskFrame *frame, AuraHttpConnection *connection)
{
  AuraTaskPollState state;
  AuraHttpResponseStatus response_status;
  size_t required = 0;

  if (frame == NULL || connection == NULL || connection->async_task_handler == NULL ||
      !connection->async_request_active)
  {
    return AURA_TASK_FAILED;
  }
  if (!connection->async_handler_started)
  {
    aura_http_response_init(&connection->async_response);
    connection->async_response_active = 1;
    if (!aura_http_connection_header_has(&connection->async_request, "close") &&
        aura_http_response_set_connection(&connection->async_response,
                                          AURA_HTTP_RESPONSE_KEEP_ALIVE) !=
            AURA_HTTP_RESPONSE_OK)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_handler_started = 1;
  }
  state = connection->async_task_handler(
      frame, &connection->async_request, &connection->async_response,
      connection->async_user_data);
  if (state == AURA_TASK_PENDING)
  {
    connection->async_phase = AURA_HTTP_ASYNC_HANDLER;
    return state;
  }
  if (state != AURA_TASK_COMPLETE)
  {
    if (connection->async_response.streaming_committed)
    {
      return AURA_TASK_FAILED;
    }
    return aura_http_connection_async_prepare_handler_failure(connection);
  }
  connection->async_close_after_write =
      aura_http_connection_header_has(&connection->async_request, "close") ||
      (connection->async_body_reader_active &&
       !aura_http_body_reader_complete(&connection->async_body_reader)) ||
      connection->async_response.connection == AURA_HTTP_RESPONSE_CLOSE ||
      connection->requests_served + 1 >= connection->config.max_requests;
  if (connection->async_response.streaming_committed)
  {
    if (aura_http_response_stream_finish(&connection->async_response, NULL, 0,
                                         &required) != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL ||
        required == 0)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_output = (char *)malloc(required);
    if (connection->async_output == NULL ||
        aura_http_response_stream_finish(&connection->async_response,
                                         connection->async_output, required,
                                         &connection->async_output_length) !=
            AURA_HTTP_RESPONSE_OK)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_output_offset = 0;
    connection->async_phase = AURA_HTTP_ASYNC_WRITE;
    return AURA_TASK_READY;
  }
  if (connection->async_close_after_write &&
      aura_http_response_set_connection(&connection->async_response,
                                        AURA_HTTP_RESPONSE_CLOSE) !=
          AURA_HTTP_RESPONSE_OK)
  {
    return AURA_TASK_FAILED;
  }
  response_status = aura_http_response_serialize(
      &connection->async_response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output = (char *)malloc(required);
  if (connection->async_output == NULL)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output_length = required;
  response_status = aura_http_response_serialize(
      &connection->async_response, connection->async_output, required,
      &connection->async_output_length);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output_offset = 0;
  connection->async_phase = AURA_HTTP_ASYNC_WRITE;
  return AURA_TASK_READY;
}

/* H5 bridge: request/response storage stays connection-owned while the task is
 * pending. The handler remains synchronous in this runtime ABI, but reads and
 * writes are independently readiness-driven. A successful response may keep
 * the connection alive; cancellation and every terminal path use the armed
 * frame cleanup hook for exactly-once close and buffer release. */
AuraTaskPollState aura_http_connection_poll_async(AuraTaskFrame *frame,
                                                  AuraHttpConnection *connection,
                                                  AuraHttpHandler handler,
                                                  void *user_data)
{
  if (frame == NULL || connection == NULL || connection->stream == NULL ||
      connection->closed || (handler == NULL && connection->async_task_handler == NULL))
  {
    return AURA_TASK_FAILED;
  }
  if (!connection->async_active)
  {
    connection->async_buffer = (unsigned char *)malloc(4096);
    if (connection->async_buffer == NULL)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_capacity = 4096;
    if (handler != NULL)
    {
      connection->async_handler = handler;
      connection->async_user_data = user_data;
    }
    connection->async_active = 1;
    connection->async_phase = AURA_HTTP_ASYNC_READ;
    aura_task_frame_set_cleanup(frame, connection,
                                aura_http_connection_async_cleanup);
  }
  if (aura_task_frame_take_fd_wait_timeout(frame))
  {
    if (connection->async_phase != AURA_HTTP_ASYNC_READ ||
        aura_http_connection_async_prepare_error_response(
            connection, 408, "request_timeout") != AURA_TASK_READY)
    {
      return aura_http_connection_async_failure(frame, connection);
    }
  }
  for (;;)
  {
    if (connection->async_phase == AURA_HTTP_ASYNC_READ)
    {
      AuraHttpRequest request;
      size_t consumed = 0;
      size_t header_end = 0;
      size_t content_length = 0;
      int chunked = 0;
      AuraHttpParseStatus parse_status;
      for (;;)
      {
        if (connection->async_task_handler != NULL)
        {
          parse_status = aura_http_request_parse_headers(
              connection->async_buffer, connection->async_used, &request,
              &header_end, &content_length, &chunked);
          if (parse_status == AURA_HTTP_PARSE_OK && content_length == 0 &&
              !chunked)
          {
            aura_http_request_destroy(&request);
            parse_status = aura_http_request_parse(
                connection->async_buffer, connection->async_used, &request,
                &consumed);
          }
        }
        else
        {
          parse_status = aura_http_request_parse(connection->async_buffer,
                                                 connection->async_used, &request,
                                                 &consumed);
        }
        if (parse_status != AURA_HTTP_PARSE_INCOMPLETE)
        {
          break;
        }
        if (connection->async_used == AURA_HTTP_MAX_TOTAL_BYTES)
        {
          parse_status = AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
          break;
        }
        if (connection->async_used == connection->async_capacity)
        {
          size_t next = connection->async_capacity * 2;
          unsigned char *grown;
          if (next > AURA_HTTP_MAX_TOTAL_BYTES)
          {
            next = AURA_HTTP_MAX_TOTAL_BYTES;
          }
          if (next <= connection->async_capacity)
          {
            parse_status = AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
            break;
          }
          grown = (unsigned char *)realloc(connection->async_buffer, next);
          if (grown == NULL)
          {
            return aura_http_connection_async_failure(frame, connection);
          }
          connection->async_buffer = grown;
          connection->async_capacity = next;
        }
        {
          size_t received = 0;
          AuraTcpStatus status = aura_tcp_stream_read(
              connection->stream, connection->async_buffer + connection->async_used,
              connection->async_capacity - connection->async_used, &received, 0);
          if (status == AURA_TCP_PENDING)
          {
            if (!aura_task_frame_wait_tcp_stream_timeout(
                    frame, connection->stream, POLLIN,
                    aura_http_min_timeout(connection->config.read_timeout_ms,
                                          connection->config.idle_timeout_ms)))
            {
              return aura_http_connection_async_failure(frame, connection);
            }
            return AURA_TASK_PENDING;
          }
          if (status == AURA_TCP_EOF || status != AURA_TCP_OK || received == 0)
          {
            return aura_http_connection_async_failure(frame, connection);
          }
          connection->async_used += received;
        }
      }
      if (parse_status == AURA_HTTP_PARSE_OK)
      {
        if (connection->async_task_handler != NULL &&
            ((!chunked && content_length != 0) || chunked))
        {
          consumed = header_end;
        }
        if (consumed == 0 || consumed > connection->async_used)
        {
          aura_http_request_destroy(&request);
          return aura_http_connection_async_failure(frame, connection);
        }
        memmove(connection->async_buffer, connection->async_buffer + consumed,
                connection->async_used - consumed);
        connection->async_used -= consumed;
        if (connection->async_task_handler != NULL)
        {
          connection->async_request = request;
          memset(&request, 0, sizeof(request));
          connection->async_request_active = 1;
          if (!chunked && content_length != 0)
          {
            if (!aura_http_content_length_reader_init(
                    &connection->async_body_reader, connection->stream,
                    connection->async_buffer, &connection->async_used,
                    content_length, connection->config.read_timeout_ms))
            {
              return aura_http_connection_async_failure(frame, connection);
            }
            connection->async_body_reader_active = 1;
            connection->async_request.body_reader = &connection->async_body_reader;
          }
          else if (chunked)
          {
            if (!aura_http_chunked_reader_init(
                    &connection->async_body_reader, connection->stream,
                    connection->async_buffer, &connection->async_used,
                    connection->config.read_timeout_ms))
            {
              return aura_http_connection_async_failure(frame, connection);
            }
            connection->async_body_reader_active = 1;
            connection->async_request.body_reader = &connection->async_body_reader;
          }
          AuraTaskPollState handler_state =
              aura_http_connection_async_run_task_handler(frame, connection);
          if (handler_state == AURA_TASK_PENDING)
          {
            return AURA_TASK_PENDING;
          }
          if (handler_state == AURA_TASK_FAILED)
          {
            return aura_http_connection_async_failure(frame, connection);
          }
        }
        else
        {
          if (!aura_http_connection_async_prepare_response(connection, &request))
          {
            aura_http_request_destroy(&request);
            return aura_http_connection_async_failure(frame, connection);
          }
          aura_http_request_destroy(&request);
        }
      }
      else
      {
        int status_code = parse_status == AURA_HTTP_PARSE_METHOD_NOT_ALLOWED
                              ? 405
                              : parse_status == AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE
                                    ? 413
                                    : 400;
        const char *error_code = status_code == 405
                                     ? "method_not_allowed"
                                     : status_code == 413 ? "payload_too_large" : "bad_request";
        size_t required = 0;
        aura_http_response_init(&connection->async_response);
        connection->async_response_active = 1;
        if (aura_http_response_set_error(&connection->async_response, status_code,
                                         error_code) != AURA_HTTP_RESPONSE_OK ||
            aura_http_response_serialize(&connection->async_response, NULL, 0,
                                         &required) != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL ||
            required == 0)
        {
          return aura_http_connection_async_failure(frame, connection);
        }
        connection->async_output = (char *)malloc(required);
        if (connection->async_output == NULL ||
            aura_http_response_serialize(&connection->async_response,
                                         connection->async_output, required,
                                         &required) != AURA_HTTP_RESPONSE_OK)
        {
          return aura_http_connection_async_failure(frame, connection);
        }
        connection->async_output_length = required;
        connection->async_output_offset = 0;
        connection->async_close_after_write = 1;
        connection->async_phase = AURA_HTTP_ASYNC_WRITE;
      }
    }
    if (connection->async_phase == AURA_HTTP_ASYNC_HANDLER)
    {
      AuraTaskPollState handler_state =
          aura_http_connection_async_run_task_handler(frame, connection);
      if (handler_state == AURA_TASK_PENDING)
      {
        return AURA_TASK_PENDING;
      }
      if (handler_state == AURA_TASK_FAILED)
      {
        return aura_http_connection_async_failure(frame, connection);
      }
    }
    if (connection->async_phase == AURA_HTTP_ASYNC_WRITE)
    {
      while (connection->async_output_offset < connection->async_output_length)
      {
        size_t written = 0;
        AuraTcpStatus status = aura_tcp_stream_write(
            connection->stream,
            connection->async_output + connection->async_output_offset,
            connection->async_output_length - connection->async_output_offset,
            &written, 0);
        if (status == AURA_TCP_PENDING)
        {
          if (!aura_task_frame_wait_tcp_stream_timeout(
                  frame, connection->stream, POLLOUT,
                  connection->config.write_timeout_ms))
          {
            return aura_http_connection_async_failure(frame, connection);
          }
          return AURA_TASK_PENDING;
        }
        if (status != AURA_TCP_OK || written == 0)
        {
          return aura_http_connection_async_failure(frame, connection);
        }
        connection->async_output_offset += written;
      }
      connection->requests_served++;
      free(connection->async_output);
      connection->async_output = NULL;
      connection->async_output_length = 0;
      connection->async_output_offset = 0;
      if (connection->async_response_active)
      {
        aura_http_response_destroy(&connection->async_response);
        connection->async_response_active = 0;
      }
      if (connection->async_request_active)
      {
        connection->async_request.body_reader = NULL;
        aura_http_request_destroy(&connection->async_request);
        connection->async_request_active = 0;
      }
      memset(&connection->async_body_reader, 0,
             sizeof(connection->async_body_reader));
      connection->async_body_reader_active = 0;
      connection->async_handler_started = 0;
      if (connection->async_close_after_write)
      {
        aura_task_frame_clear_cleanup(frame);
        (void)aura_http_connection_close(connection);
        aura_http_connection_async_reset(connection);
        aura_http_connection_async_release_handle_pin(connection);
        return AURA_TASK_COMPLETE;
      }
      connection->async_close_after_write = 0;
      connection->async_phase = AURA_HTTP_ASYNC_READ;
    }
  }
}

/* HTTP-001 task boundary for compiler-generated handlers.  The supplied
 * handler is called on the connection task's frame, so its ordinary Aura
 * await lowering can use the same readiness/cancellation machinery as any
 * other async function. */
AuraTaskPollState aura_http_connection_poll_async_task(
    AuraTaskFrame *frame, AuraHttpConnection *connection,
    AuraHttpTaskHandler handler, void *user_data)
{
  if (frame == NULL || connection == NULL || handler == NULL ||
      (connection->async_active && connection->async_task_handler != handler))
  {
    return AURA_TASK_FAILED;
  }
  if (!connection->async_active)
  {
    connection->async_task_handler = handler;
    connection->async_user_data = user_data;
  }
  return aura_http_connection_poll_async(frame, connection, NULL, NULL);
}

AuraTaskPollState aura_http_connection_poll_async_task_handle(
    AuraTaskFrame *frame, AuraFfiOpaqueHandle *handle,
    AuraHttpTaskHandler handler, void *user_data)
{
  AuraFfiHandlePin pin;
  AuraHttpConnection *connection;
  if (frame == NULL || handle == NULL || handler == NULL)
  {
    return AURA_TASK_FAILED;
  }
  if (handle->resource != NULL && handle->pins != 0 &&
      !handle->destroyed)
  {
    connection = (AuraHttpConnection *)handle->resource;
    if (connection->async_handle_pin_active)
    {
      if (connection->async_handle_frame != frame)
      {
        return AURA_TASK_FAILED;
      }
      return aura_http_connection_poll_async_task(frame, connection, handler,
                                                   user_data);
    }
  }
  memset(&pin, 0, sizeof(pin));
  if (aura_ffi_handle_pin_for_boundary(handle, AURA_FFI_BOUNDARY_TASK, &pin) !=
      AURA_FFI_OK)
  {
    return AURA_TASK_FAILED;
  }
  connection = (AuraHttpConnection *)pin.resource;
  if (connection == NULL || connection->closed || connection->stream == NULL ||
      connection->async_handle_pin_active)
  {
    (void)aura_ffi_handle_unpin(&pin);
    return AURA_TASK_FAILED;
  }
  connection->async_handle_pin = pin;
  connection->async_handle_pin_active = 1;
  connection->async_handle_frame = frame;
  return aura_http_connection_poll_async_task(frame, connection, handler,
                                               user_data);
}

uint32_t aura_task_frame_resume_state(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->resume_state : 0;
}

void aura_task_frame_set_resume_state(AuraTaskFrame *frame, uint32_t state)
{
  if (frame != NULL)
  {
    frame->resume_state = state;
  }
}

AuraTaskFrameStorage aura_task_frame_captures(const AuraTaskFrame *frame)
{
  AuraTaskFrameStorage empty = {NULL, 0, NULL, AURA_TASK_OWNED, 0};
  return frame != NULL ? frame->captures : empty;
}

static void aura_task_frame_storage_release(AuraTaskFrameStorage *storage)
{
  void *data;
  size_t size;
  AuraTaskResultDestroyFn destroy;

  if (storage == NULL)
  {
    return;
  }
  if (storage->rooted)
  {
    aura_gc_remove_root(&storage->data);
  }

  /* Clear the slot before invoking user cleanup.  Besides making the
   * release operation idempotent, this keeps a re-entrant cleanup callback
   * from observing a live ownership record after its root was removed. */
  data = storage->data;
  size = storage->size;
  destroy = storage->destroy;
  *storage = (AuraTaskFrameStorage){NULL, 0, NULL, AURA_TASK_OWNED, 0};

  if (destroy != NULL && data != NULL)
  {
    destroy(data, size);
  }
}

static int aura_task_frame_storage_set(AuraTaskFrameStorage *storage,
                                       void *data,
                                       size_t size,
                                       AuraTaskResultDestroyFn destroy,
                                       AuraTaskOwnership ownership)
{
  if (storage == NULL || ownership == AURA_TASK_BORROWED)
  {
    return 0;
  }
  aura_task_frame_storage_release(storage);
  *storage = (AuraTaskFrameStorage){data, size, destroy, ownership, 0};
  if (data != NULL)
  {
    aura_gc_add_root(&storage->data);
    storage->rooted = 1;
  }
  return 1;
}

void aura_task_frame_set_captures(AuraTaskFrame *frame,
                                  void *data,
                                  size_t size,
                                  AuraTaskResultDestroyFn destroy)
{
  if (frame != NULL)
  {
    (void)aura_task_frame_storage_set(
        &frame->captures, data, size, destroy, AURA_TASK_OWNED);
  }
}

int aura_task_frame_set_captures_with_ownership(AuraTaskFrame *frame,
                                                void *data,
                                                size_t size,
                                                AuraTaskResultDestroyFn destroy,
                                                AuraTaskOwnership ownership)
{
  return frame != NULL ? aura_task_frame_storage_set(
                             &frame->captures, data, size, destroy, ownership)
                       : 0;
}

AuraTaskFrameStorage aura_task_frame_pending(const AuraTaskFrame *frame)
{
  AuraTaskFrameStorage empty = {NULL, 0, NULL, AURA_TASK_OWNED, 0};
  return frame != NULL ? frame->pending : empty;
}

/* A frame-scoped cleanup is the bounded bridge between a pending I/O
 * operation and the task lifecycle.  The callback owns the resource while
 * armed; clearing the slot before invoking it makes cancellation, failure,
 * and shutdown cleanup re-entrant and exactly-once. */
static void aura_task_frame_cleanup_run(AuraTaskFrame *frame)
{
  void *data;
  AuraTaskCleanupFn cleanup;

  if (frame == NULL || frame->cleanup.cleanup == NULL)
  {
    return;
  }
  data = frame->cleanup.data;
  cleanup = frame->cleanup.cleanup;
  frame->cleanup = (AuraTaskFrameCleanup){NULL, NULL};
  if (cleanup != NULL && data != NULL)
  {
    cleanup(data);
  }
}

void aura_task_frame_set_cleanup(AuraTaskFrame *frame,
                                  void *data,
                                  AuraTaskCleanupFn cleanup)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_frame_cleanup_run(frame);
  frame->cleanup = (AuraTaskFrameCleanup){data, cleanup};
}

void aura_task_frame_clear_cleanup(AuraTaskFrame *frame)
{
  if (frame != NULL)
  {
    frame->cleanup = (AuraTaskFrameCleanup){NULL, NULL};
  }
}

void aura_task_frame_set_pending(AuraTaskFrame *frame,
                                 void *data,
                                 size_t size,
                                 AuraTaskResultDestroyFn destroy)
{
  if (frame != NULL)
  {
    (void)aura_task_frame_storage_set(
        &frame->pending, data, size, destroy, AURA_TASK_TRANSFERRED);
    if (data != NULL)
    {
      frame->state = AURA_TASK_PENDING;
    }
  }
}

int aura_task_frame_set_pending_with_ownership(AuraTaskFrame *frame,
                                               void *data,
                                               size_t size,
                                               AuraTaskResultDestroyFn destroy,
                                               AuraTaskOwnership ownership)
{
  if (frame == NULL || !aura_task_frame_storage_set(
                           &frame->pending, data, size, destroy, ownership))
  {
    return 0;
  }
  if (data != NULL)
  {
    frame->state = AURA_TASK_PENDING;
  }
  return 1;
}

AuraTaskOwnership aura_task_frame_capture_ownership(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->captures.ownership : AURA_TASK_BORROWED;
}

AuraTaskOwnership aura_task_frame_pending_ownership(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->pending.ownership : AURA_TASK_BORROWED;
}

AuraTaskResult aura_task_frame_error(const AuraTaskFrame *frame)
{
  AuraTaskResult empty = {NULL, 0};
  return frame != NULL ? frame->error : empty;
}

AuraTaskResult aura_task_frame_error_payload(const AuraTaskFrame *frame)
{
  AuraTaskResult empty = {NULL, 0};
  return frame != NULL ? frame->error_payload : empty;
}

const char *aura_task_frame_error_type_name(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_type_name : NULL;
}

uint32_t aura_task_frame_error_source_id(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_source_id : 0;
}

uint32_t aura_task_frame_error_span_start(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_span_start : 0;
}

uint32_t aura_task_frame_error_span_end(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_span_end : 0;
}

static void aura_task_result_release(AuraTaskResult *result,
                                     AuraTaskResultCloneFn *clone,
                                     AuraTaskResultDestroyFn *destroy,
                                     int *rooted)
{
  void *data;
  size_t size;
  AuraTaskResultDestroyFn drop;

  if (result == NULL || destroy == NULL || rooted == NULL)
  {
    return;
  }
  if (*rooted)
  {
    aura_gc_remove_root(&result->data);
  }
  data = result->data;
  size = result->size;
  drop = *destroy;
  *result = (AuraTaskResult){NULL, 0};
  if (clone != NULL)
  {
    *clone = NULL;
  }
  *destroy = NULL;
  *rooted = 0;
  if (drop != NULL && data != NULL)
  {
    drop(data, size);
  }
}

static void aura_task_frame_clear_error_type_name(AuraTaskFrame *frame)
{
  if (frame == NULL)
  {
    return;
  }
  free(frame->error_type_name);
  frame->error_type_name = NULL;
}

void aura_task_frame_set_error_type_name(AuraTaskFrame *frame,
                                         const char *type_name)
{
  size_t length;
  char *copy;
  if (frame == NULL)
  {
    return;
  }
  aura_task_frame_clear_error_type_name(frame);
  if (type_name == NULL)
  {
    return;
  }
  length = strlen(type_name);
  copy = (char *)malloc(length + 1);
  if (copy == NULL)
  {
    abort();
  }
  memcpy(copy, type_name, length + 1);
  frame->error_type_name = copy;
}

void aura_task_frame_set_error_span_with_clone(
    AuraTaskFrame *frame, void *data, size_t size, AuraTaskResultCloneFn clone,
    AuraTaskResultDestroyFn destroy, uint32_t source_id, uint32_t span_start,
    uint32_t span_end)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_result_release(&frame->error_payload,
                           &frame->error_payload_clone,
                           &frame->error_payload_destroy,
                           &frame->error_payload_rooted);
  aura_task_result_release(&frame->error, &frame->error_clone,
                           &frame->error_destroy,
                           &frame->error_rooted);
  aura_task_frame_clear_error_type_name(frame);
  frame->error = (AuraTaskResult){data, size};
  frame->error_clone = clone;
  frame->error_destroy = destroy;
  frame->error_source_id = source_id;
  frame->error_span_start = span_start;
  frame->error_span_end = span_end;
  if (data != NULL)
  {
    aura_gc_add_root(&frame->error.data);
    frame->error_rooted = 1;
    frame->state = AURA_TASK_FAILED;
  }
}

void aura_task_frame_set_error_payload_with_clone(
    AuraTaskFrame *frame, void *data, size_t size,
    AuraTaskResultCloneFn clone, AuraTaskResultDestroyFn destroy)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_result_release(&frame->error_payload,
                           &frame->error_payload_clone,
                           &frame->error_payload_destroy,
                           &frame->error_payload_rooted);
  frame->error_payload = (AuraTaskResult){data, size};
  frame->error_payload_clone = clone;
  frame->error_payload_destroy = destroy;
  if (data != NULL)
  {
    aura_gc_add_root(&frame->error_payload.data);
    frame->error_payload_rooted = 1;
  }
}

void aura_task_frame_set_error_span(AuraTaskFrame *frame,
                                    void *data,
                                    size_t size,
                                    AuraTaskResultDestroyFn destroy,
                                    uint32_t source_id,
                                    uint32_t span_start,
                                    uint32_t span_end)
{
  aura_task_frame_set_error_span_with_clone(
      frame, data, size, NULL, destroy, source_id, span_start, span_end);
}

void aura_task_frame_set_error_at(AuraTaskFrame *frame,
                                  void *data,
                                  size_t size,
                                  AuraTaskResultDestroyFn destroy,
                                  uint32_t source_id)
{
  aura_task_frame_set_error_span(frame, data, size, destroy, source_id,
                                 source_id, source_id);
}

void aura_task_frame_set_error(AuraTaskFrame *frame,
                               void *data,
                               size_t size,
                               AuraTaskResultDestroyFn destroy)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_frame_set_error_at(
      frame, data, size, destroy, frame->race_source_id);
}

static void aura_task_error_copy_destroy(void *data, size_t size)
{
  (void)size;
  free(data);
}

static void *aura_task_error_shallow_clone(const void *data, size_t size,
                                           size_t *cloned_size)
{
  void *copy;
  if (cloned_size == NULL)
  {
    return NULL;
  }
  copy = malloc(size == 0 ? 1 : size);
  if (copy == NULL)
  {
    return NULL;
  }
  if (size != 0 && data != NULL)
  {
    memcpy(copy, data, size);
  }
  *cloned_size = size;
  return copy;
}

/* Propagate an error with an explicit payload clone.  The clone callback is
 * responsible for recursively copying owned fields; the destroy callback is
 * responsible for releasing that independent copy.  This keeps the runtime
 * generic while allowing generated code to preserve nested String/Array
 * ownership across an async child-to-parent boundary. */
int aura_task_frame_propagate_error_with_clone(
    AuraTaskFrame *frame, const AuraTaskFrame *source,
    AuraTaskResultCloneFn clone, AuraTaskResultDestroyFn destroy)
{
  AuraTaskResult error;
  size_t cloned_size = 0;
  void *copy;

  if (frame == NULL || source == NULL || clone == NULL ||
      source->state != AURA_TASK_FAILED || source->error.data == NULL)
  {
    return 0;
  }
  error = source->error;
  copy = clone(error.data, error.size, &cloned_size);
  if (copy == NULL)
  {
    return 0;
  }
  aura_task_frame_set_error_span_with_clone(
      frame, copy, cloned_size, clone, destroy, source->error_source_id,
      source->error_span_start, source->error_span_end);
  return 1;
}

/* Copy a terminal child error into its waiting parent before the parent
 * publishes AURA_TASK_FAILED. The child remains executor-owned and retains
 * its original payload/source ID; the parent receives an independent payload
 * so either frame may be released independently. */
int aura_task_frame_propagate_error(AuraTaskFrame *frame,
                                    const AuraTaskFrame *source)
{
  int propagated;
  if (source != NULL && source->error_clone != NULL)
  {
    propagated = aura_task_frame_propagate_error_with_clone(
        frame, source, source->error_clone, source->error_destroy);
  }
  else
  {
    propagated = aura_task_frame_propagate_error_with_clone(
        frame, source, aura_task_error_shallow_clone,
        aura_task_error_copy_destroy);
  }
  if (!propagated || source == NULL || source->error_payload.data == NULL)
  {
    return propagated;
  }
  if (source->error_payload_clone == NULL ||
      source->error_payload_destroy == NULL)
  {
    return 0;
  }
  size_t cloned_size = 0;
  void *copy = source->error_payload_clone(
      source->error_payload.data, source->error_payload.size, &cloned_size);
  if (copy == NULL)
  {
    return 0;
  }
  aura_task_frame_set_error_payload_with_clone(
      frame, copy, cloned_size, source->error_payload_clone,
      source->error_payload_destroy);
  aura_task_frame_set_error_type_name(frame, source->error_type_name);
  return 1;
}

void aura_task_frame_set_result(AuraTaskFrame *frame,
                                void *data,
                                size_t size,
                                AuraTaskResultDestroyFn destroy);

/* Publish a child's complete terminal outcome into its waiting parent. A
 * payload is copied only through the caller-supplied clone/destroy pair;
 * cancellation has no payload and is forwarded as a cancellation request. */
AuraTaskPollState aura_task_frame_propagate_outcome(
    AuraTaskFrame *frame, const AuraTaskFrame *source,
    AuraTaskResultCloneFn result_clone, AuraTaskResultDestroyFn result_destroy)
{
  size_t cloned_size = 0;
  void *copy;

  if (frame == NULL || source == NULL ||
      (source->state != AURA_TASK_COMPLETE &&
       source->state != AURA_TASK_FAILED &&
       source->state != AURA_TASK_CANCELLED))
  {
    return AURA_TASK_FAILED;
  }
  if (source->state == AURA_TASK_CANCELLED)
  {
    frame->cancel_requested = 1;
    frame->state = AURA_TASK_CANCELLED;
    return AURA_TASK_CANCELLED;
  }
  if (source->state == AURA_TASK_FAILED)
  {
    (void)aura_task_frame_propagate_error(frame, source);
    return AURA_TASK_FAILED;
  }
  if (source->result.data == NULL)
  {
    aura_task_frame_set_result(frame, NULL, 0, NULL);
    return AURA_TASK_COMPLETE;
  }
  if (result_clone == NULL)
  {
    return AURA_TASK_FAILED;
  }
  copy = result_clone(source->result.data, source->result.size, &cloned_size);
  if (copy == NULL)
  {
    return AURA_TASK_FAILED;
  }
  aura_task_frame_set_result(frame, copy, cloned_size, result_destroy);
  return AURA_TASK_COMPLETE;
}

void aura_task_frame_set_result(AuraTaskFrame *frame,
                                void *data,
                                size_t size,
                                AuraTaskResultDestroyFn destroy)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_result_release(&frame->result, NULL, &frame->result_destroy,
                           &frame->result_rooted);
  frame->result.data = data;
  frame->result.size = size;
  frame->result_destroy = destroy;
  if (data != NULL)
  {
    aura_gc_add_root(&frame->result.data);
    frame->result_rooted = 1;
  }
}

AuraTaskResult aura_task_frame_result(const AuraTaskFrame *frame)
{
  AuraTaskResult empty = {NULL, 0};
  return frame != NULL ? frame->result : empty;
}

void aura_task_frame_destroy(AuraTaskFrame *frame)
{
  if (frame == NULL)
  {
    return;
  }
#if defined(AURA_TCP_POSIX)
  if (frame->blocking_thread_created)
  {
    pthread_join(frame->blocking_thread, NULL);
    frame->blocking_started = 0;
    frame->blocking_thread_created = 0;
  }
  if (frame->blocking_env_destroy != NULL && frame->blocking_env != NULL)
  {
    frame->blocking_env_destroy(frame->blocking_env);
    frame->blocking_env = NULL;
  }
#endif
  aura_gc_unlink_task_frame(frame);
  aura_task_frame_unlink_cancel_parent(frame);
  aura_task_frame_detach_cancel_children(frame);
  aura_task_frame_detach_wait_target(frame);
  aura_task_frame_detach_waiters(frame);
  aura_task_frame_clear_waiting(frame);
  aura_task_frame_cleanup_run(frame);
  if (frame->destroy != NULL)
  {
    frame->destroy(frame);
  }
  aura_task_frame_unpin_foreign_handles(frame);
  aura_task_result_release(&frame->result, NULL, &frame->result_destroy,
                           &frame->result_rooted);
  aura_task_frame_storage_release(&frame->captures);
  aura_task_frame_storage_release(&frame->pending);
  aura_task_result_release(&frame->error, &frame->error_clone,
                           &frame->error_destroy,
                           &frame->error_rooted);
  aura_task_result_release(&frame->error_payload,
                           &frame->error_payload_clone,
                           &frame->error_payload_destroy,
                           &frame->error_payload_rooted);
  aura_task_frame_clear_error_type_name(frame);
  if (frame->data != NULL)
  {
    aura_gc_remove_root(&frame->data);
    aura_gc_release(frame->data);
    frame->data = NULL;
  }
#if defined(AURA_TCP_POSIX)
  if (frame->blocking_fn != NULL)
  {
    pthread_mutex_destroy(&frame->blocking_lock);
  }
#endif
  free(frame);
}

/* ---- C22k deterministic single-threaded executor ----
 *
 * Submission transfers frame ownership to the executor.  The executor keeps
 * terminal frames alive so generated code can read their result until an
 * explicit release or shutdown; aura_task_executor_shutdown destroys every
 * remaining submitted frame once.
 * A poll callback returning READY is immediately queued at the FIFO tail.
 * PENDING parks the frame until aura_task_executor_wake is called.  No OS
 * threads, blocking waits, or implicit polling are used.
 */

#if defined(AURA_TCP_POSIX)
struct AuraLazyCell
{
  pthread_mutex_t lock;
  pthread_cond_t condition;
  int state; /* 0=uninitialized, 1=initializing, 2=initialized, 3=failed */
  int published;
  void *value;
  AuraLazyInitFn init;
  void *environment;
  AuraTaskBlockingEnvDestroyFn environment_destroy;
  AuraLazyValueDestroyFn value_destroy;
};

AuraLazyCell *aura_lazy_cell_new(AuraLazyInitFn init, void *environment,
                                 AuraTaskBlockingEnvDestroyFn environment_destroy)
{
  AuraLazyCell *cell;
  if (init == NULL)
  {
    return NULL;
  }
  cell = (AuraLazyCell *)calloc(1, sizeof(*cell));
  if (cell == NULL)
  {
    return NULL;
  }
  if (pthread_mutex_init(&cell->lock, NULL) != 0)
  {
    free(cell);
    return NULL;
  }
  if (pthread_cond_init(&cell->condition, NULL) != 0)
  {
    pthread_mutex_destroy(&cell->lock);
    free(cell);
    return NULL;
  }
  cell->init = init;
  cell->environment = environment;
  cell->environment_destroy = environment_destroy;
  return cell;
}

void aura_lazy_cell_publish(AuraLazyCell *cell, void *value, size_t size,
                            AuraLazyValueDestroyFn value_destroy)
{
  (void)size;
  if (cell == NULL)
  {
    return;
  }
  pthread_mutex_lock(&cell->lock);
  cell->value = value;
  cell->value_destroy = value_destroy;
  cell->published = 1;
  pthread_mutex_unlock(&cell->lock);
}

void *aura_lazy_cell_value(AuraLazyCell *cell)
{
  void *value = NULL;
  if (cell == NULL)
  {
    return NULL;
  }
  pthread_mutex_lock(&cell->lock);
  while (cell->state == 1)
  {
    pthread_cond_wait(&cell->condition, &cell->lock);
  }
  if (cell->state == 0)
  {
    cell->state = 1;
    pthread_mutex_unlock(&cell->lock);
    cell->init(cell, cell->environment);
    pthread_mutex_lock(&cell->lock);
    cell->state = cell->published ? 2 : 3;
    if (cell->environment_destroy != NULL && cell->environment != NULL)
    {
      cell->environment_destroy(cell->environment);
      cell->environment = NULL;
    }
    pthread_cond_broadcast(&cell->condition);
  }
  if (cell->state == 2)
  {
    value = cell->value;
  }
  pthread_mutex_unlock(&cell->lock);
  return value;
}

int aura_lazy_cell_is_initialized(AuraLazyCell *cell)
{
  int initialized;
  if (cell == NULL)
  {
    return 0;
  }
  pthread_mutex_lock(&cell->lock);
  initialized = cell->state == 2;
  pthread_mutex_unlock(&cell->lock);
  return initialized;
}

void aura_lazy_cell_destroy(AuraLazyCell *cell)
{
  if (cell == NULL)
  {
    return;
  }
  pthread_mutex_lock(&cell->lock);
  while (cell->state == 1)
  {
    pthread_cond_wait(&cell->condition, &cell->lock);
  }
  if (cell->value_destroy != NULL && cell->value != NULL)
  {
    cell->value_destroy(cell->value);
    cell->value = NULL;
  }
  if (cell->environment_destroy != NULL && cell->environment != NULL)
  {
    cell->environment_destroy(cell->environment);
    cell->environment = NULL;
  }
  pthread_mutex_unlock(&cell->lock);
  pthread_cond_destroy(&cell->condition);
  pthread_mutex_destroy(&cell->lock);
  free(cell);
}
#endif

struct AuraTaskExecutor
{
  AuraTaskFrame *ready_head;
  AuraTaskFrame *ready_tail;
  AuraTaskFrame *owned_head;
  size_t ready_count;
  size_t owned_count;
  size_t max_live_tasks;
  int shutdown;
  AuraRaceTracker *race_tracker;
  AuraTaskFailureHookFn failure_hook;
  void *failure_hook_context;
  int wake_pipe[2];
#if defined(AURA_TCP_POSIX)
  pthread_mutex_t worker_lock;
  pthread_cond_t worker_cond;
  pthread_t *workers;
  size_t worker_count;
  int workers_stop;
  int workers_started;
  pthread_t reactor_thread;
  int reactor_stop;
  int reactor_started;
  int reactor_active;
  int active_workers;
  int gc_requested;
#endif
};

struct AuraTaskScope
{
  AuraTaskExecutor *executor;
  AuraTaskScope *previous;
  AuraTaskFrame *frames;
  int active;
};

static _Thread_local AuraTaskScope *aura_task_current_scope = NULL;
static _Thread_local AuraTaskExecutor *aura_task_current_executor = NULL;

static int aura_task_executor_has_workers(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  return executor != NULL && executor->workers_started;
#else
  (void)executor;
  return 0;
#endif
}
static void aura_task_scope_adopt(AuraTaskFrame *frame);
int aura_task_executor_poll_waiting(AuraTaskExecutor *executor, int timeout_ms);

void aura_gc_collect_executor(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->gc_requested = 1;
    pthread_cond_broadcast(&executor->worker_cond);
    int current_worker = aura_task_current_executor == executor;
    while (executor->active_workers > (current_worker ? 1 : 0))
    {
      pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
    }
    pthread_mutex_unlock(&executor->worker_lock);

    aura_gc_collect();

    pthread_mutex_lock(&executor->worker_lock);
    executor->gc_requested = 0;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
    return;
  }
#endif
  aura_gc_collect();
}

static void aura_task_executor_lock(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
    pthread_mutex_lock(&executor->worker_lock);
#else
  (void)executor;
#endif
}

static void aura_task_executor_unlock(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
    pthread_mutex_unlock(&executor->worker_lock);
#else
  (void)executor;
#endif
}

#define AURA_TASK_DEFAULT_MAX_LIVE_TASKS ((size_t)4096)
#define AURA_TASK_MAX_LIVE_TASKS_LIMIT ((size_t)65536)

/* Defined with the typed I/O operation implementation below.  Keeping this
 * small bridge here lets the scheduler publish readiness without exposing the
 * operation layout to the executor code. */
static int aura_io_operation_ready(AuraTaskFrame *frame, short revents);

#if defined(AURA_TCP_POSIX)
#define AURA_TASK_MAX_WORKERS ((size_t)64)
static void aura_task_executor_finish_poll_unlocked(
    AuraTaskExecutor *executor, AuraTaskFrame *frame, AuraTaskPollState state);
#endif

int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame);
int aura_task_executor_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame);
static void aura_task_channel_cancel_wait(AuraTaskFrame *frame);
static void aura_task_select_cancel_wait(AuraTaskFrame *frame);
static int aura_task_executor_wake_unlocked(AuraTaskExecutor *executor,
                                            AuraTaskFrame *frame);

static void aura_task_scope_adopt(AuraTaskFrame *frame)
{
  AuraTaskScope *scope = aura_task_current_scope;
  if (frame == NULL || scope == NULL || !scope->active ||
      scope->executor != frame->executor)
  {
    return;
  }
  frame->scope = scope;
  frame->scope_owned = 1;
  frame->scope_next = scope->frames;
  scope->frames = frame;
}

AuraTaskPollState aura_task_frame_poll_once(AuraTaskFrame *frame)
{
  int64_t now;
  if (frame == NULL || frame->poll == NULL)
  {
    return AURA_TASK_FAILED;
  }
  if (frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED)
  {
    return frame->state;
  }
  now = aura_time_monotonic_millis();
  if (frame->cancel_deadline_ms != 0 && now >= frame->cancel_deadline_ms)
  {
    frame->cancel_requested = 1;
  }
  if (frame->cancel_requested)
  {
    /* Cancellation publishes a terminal outcome only after operation and
     * capture ownership has been released.  The frame itself remains
     * executor-owned until join/release, so its terminal metadata is still
     * observable without keeping cancelled work alive. */
    aura_task_frame_request_cancel_children(frame);
    aura_task_frame_storage_release(&frame->pending);
    aura_task_frame_storage_release(&frame->captures);
    aura_task_frame_cleanup_run(frame);
    aura_task_frame_clear_waiting(frame);
    /* Cancellation is terminal unless its bounded cancellation handler
     * publishes an exception. The handler runs after owned cleanup, so an
     * exception raised during cancellation cannot leak the cancelled
     * operation's resources. */
    if (frame->cancel != NULL &&
        frame->cancel(frame) == AURA_TASK_FAILED && frame->error.data != NULL)
    {
      frame->state = AURA_TASK_FAILED;
    }
    else
    {
      frame->state = AURA_TASK_CANCELLED;
    }
    aura_task_frame_wake_waiters(frame);
    return frame->state;
  }
  AuraTaskPollState state = frame->poll(frame);
  if (state < AURA_TASK_READY || state > AURA_TASK_CANCELLED)
  {
    state = AURA_TASK_FAILED;
  }
  if (state == AURA_TASK_FAILED || state == AURA_TASK_CANCELLED)
  {
    aura_task_frame_cleanup_run(frame);
  }
  frame->state = state;
  if (state == AURA_TASK_COMPLETE || state == AURA_TASK_FAILED ||
      state == AURA_TASK_CANCELLED)
  {
    aura_task_frame_wake_waiters(frame);
  }
  return state;
}

AuraTaskExecutor *aura_task_executor_new(void)
{
  AuraTaskExecutor *executor =
      (AuraTaskExecutor *)calloc(1, sizeof(AuraTaskExecutor));
  if (executor != NULL)
  {
    executor->max_live_tasks = AURA_TASK_DEFAULT_MAX_LIVE_TASKS;
    executor->wake_pipe[0] = -1;
    executor->wake_pipe[1] = -1;
#if defined(AURA_TCP_POSIX)
    if (pipe(executor->wake_pipe) == 0)
    {
      (void)fcntl(executor->wake_pipe[0], F_SETFL, O_NONBLOCK);
      (void)fcntl(executor->wake_pipe[1], F_SETFL, O_NONBLOCK);
    }
#endif
  }
  return executor;
}

int aura_task_executor_set_max_live_tasks(AuraTaskExecutor *executor,
                                           size_t max_live_tasks)
{
  if (executor == NULL || executor->shutdown || max_live_tasks == 0 ||
      max_live_tasks > AURA_TASK_MAX_LIVE_TASKS_LIMIT ||
      executor->owned_count > max_live_tasks)
  {
    return 0;
  }
  executor->max_live_tasks = max_live_tasks;
  return 1;
}

void aura_task_executor_set_race_tracker(AuraTaskExecutor *executor,
                                         AuraRaceTracker *tracker)
{
  if (executor != NULL && !executor->shutdown)
  {
    executor->race_tracker = tracker;
    aura_race_tracker_set_active(tracker);
  }
}

static void aura_task_default_failure_hook(
    const AuraTaskFailureDiagnostic *diagnostic, void *context)
{
  (void)context;
  if (diagnostic == NULL)
  {
    return;
  }
  fprintf(stderr,
          "aura task failure: task=%" PRIu64 " source=%" PRIu32
          " error_size=%zu\n",
          diagnostic->task_id, diagnostic->source_id, diagnostic->error.size);
}

/* Install the destination for failures that reach terminal state without a
 * successful join.  The diagnostic and its error bytes are borrowed only for
 * the duration of the callback.  Passing NULL restores the deterministic
 * stderr logger, so an unjoined failure is never silently discarded. */
void aura_task_executor_set_failure_hook(AuraTaskExecutor *executor,
                                          AuraTaskFailureHookFn hook,
                                          void *context)
{
  if (executor == NULL || executor->shutdown)
  {
    return;
  }
  executor->failure_hook = hook != NULL ? hook : aura_task_default_failure_hook;
  executor->failure_hook_context = context;
}

static void aura_task_executor_report_unjoined_failure(AuraTaskExecutor *executor,
                                                       AuraTaskFrame *frame)
{
  AuraTaskFailureDiagnostic diagnostic;
  AuraTaskFailureHookFn hook;

  if (executor == NULL || frame == NULL || frame->state != AURA_TASK_FAILED ||
      frame->join_observed || frame->failure_reported)
  {
    return;
  }
  frame->failure_reported = 1;
  diagnostic.task_id = frame->task_id;
  diagnostic.source_id = frame->error_source_id;
  diagnostic.state = frame->state;
  diagnostic.error = frame->error;
  hook = executor->failure_hook != NULL ? executor->failure_hook
                                        : aura_task_default_failure_hook;
  hook(&diagnostic, executor->failure_hook_context);
}

static void aura_task_executor_push_owned(AuraTaskExecutor *executor,
                                           AuraTaskFrame *frame)
{
  frame->owned_next = executor->owned_head;
  executor->owned_head = frame;
  executor->owned_count++;
  frame->executor = executor;
}

int aura_task_executor_submit(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  int result;
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
  }
#endif
  if (executor == NULL || frame == NULL || executor->shutdown || frame->executor != NULL ||
      executor->owned_count >= executor->max_live_tasks)
  {
#if defined(AURA_TCP_POSIX)
    if (executor != NULL && executor->workers_started)
    {
      pthread_mutex_unlock(&executor->worker_lock);
    }
#endif
    return 0;
  }
  aura_task_executor_push_owned(executor, frame);
  frame->handle_owned = 1;
  aura_task_scope_adopt(frame);
  if (executor->race_tracker != NULL)
  {
    (void)aura_race_tracker_record(executor->race_tracker,
                                   frame->task_id,
                                   0,
                                   frame->race_source_id,
                                   AURA_RACE_TASK_SPAWN,
                                   NULL);
  }
  frame->state = AURA_TASK_READY;
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    result = aura_task_executor_wake_unlocked(executor, frame);
    if (result != 0)
    {
      pthread_cond_signal(&executor->worker_cond);
    }
    pthread_mutex_unlock(&executor->worker_lock);
    return result;
  }
#endif
  return aura_task_executor_wake(executor, frame);
}

static int aura_task_executor_wake_unlocked(AuraTaskExecutor *executor,
                                            AuraTaskFrame *frame)
{
  if (executor == NULL || frame == NULL || executor->shutdown || frame->executor != executor ||
      frame->queued || frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  frame->queue_next = NULL;
  frame->queued = 1;
  if (executor->ready_tail == NULL)
  {
    executor->ready_head = frame;
  }
  else
  {
    executor->ready_tail->queue_next = frame;
  }
  executor->ready_tail = frame;
  executor->ready_count++;
  frame->state = AURA_TASK_READY;
  return 1;
}

int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  int result;
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->wake_pipe[1] >= 0)
  {
    const unsigned char signal_byte = 1;
    /* Signal before taking the queue lock so a reactor blocked in poll can
     * observe the wake even while another worker is publishing the queue. */
    (void)write(executor->wake_pipe[1], &signal_byte, sizeof(signal_byte));
  }
  if (executor != NULL && executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    result = aura_task_executor_wake_unlocked(executor, frame);
    if (result != 0)
    {
      pthread_cond_signal(&executor->worker_cond);
    }
    pthread_mutex_unlock(&executor->worker_lock);
    return result;
  }
#endif
  return aura_task_executor_wake_unlocked(executor, frame);
}

static void aura_task_frame_detach_wait_target(AuraTaskFrame *frame)
{
  AuraTaskFrame **link;

  if (frame == NULL || frame->wait_target == NULL)
  {
    return;
  }
  link = &frame->wait_target->waiters_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->waiter_next;
  }
  if (*link == frame)
  {
    *link = frame->waiter_next;
  }
  frame->wait_target = NULL;
  frame->waiter_next = NULL;
  if (frame->waiting_node != NULL)
  {
    frame->waiting_node = NULL;
  }
}

static void aura_task_frame_detach_waiters(AuraTaskFrame *frame)
{
  AuraTaskFrame *waiter;

  if (frame == NULL)
  {
    return;
  }
  waiter = frame->waiters_head;
  frame->waiters_head = NULL;
  while (waiter != NULL)
  {
    AuraTaskFrame *next = waiter->waiter_next;
    waiter->wait_target = NULL;
    waiter->waiter_next = NULL;
    if (waiter->waiting_node == frame)
    {
      waiter->waiting_node = NULL;
    }
    waiter = next;
  }
}

static void aura_task_frame_unlink_cancel_parent(AuraTaskFrame *frame)
{
  AuraTaskFrame **link;

  if (frame == NULL || frame->cancel_parent == NULL)
  {
    return;
  }
  link = &frame->cancel_parent->cancel_children_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->cancel_sibling_next;
  }
  if (*link == frame)
  {
    *link = frame->cancel_sibling_next;
  }
  frame->cancel_parent = NULL;
  frame->cancel_sibling_next = NULL;
}

static void aura_task_frame_detach_cancel_children(AuraTaskFrame *frame)
{
  AuraTaskFrame *child;

  if (frame == NULL)
  {
    return;
  }
  child = frame->cancel_children_head;
  frame->cancel_children_head = NULL;
  while (child != NULL)
  {
    AuraTaskFrame *next = child->cancel_sibling_next;
    child->cancel_parent = NULL;
    child->cancel_sibling_next = NULL;
    child = next;
  }
}

int aura_task_frame_link_cancellation(AuraTaskFrame *parent,
                                      AuraTaskFrame *child)
{
  if (parent == NULL || child == NULL || parent == child ||
      parent->executor == NULL || parent->executor != child->executor ||
      parent->state == AURA_TASK_COMPLETE || parent->state == AURA_TASK_FAILED ||
      parent->state == AURA_TASK_CANCELLED || child->state == AURA_TASK_COMPLETE ||
      child->state == AURA_TASK_FAILED || child->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  aura_task_frame_unlink_cancel_parent(child);
  child->cancel_parent = parent;
  child->cancel_sibling_next = parent->cancel_children_head;
  parent->cancel_children_head = child;
  return 1;
}

static void aura_task_frame_request_cancel_children(AuraTaskFrame *parent)
{
  AuraTaskFrame *child;

  if (parent == NULL)
  {
    return;
  }
  child = parent->cancel_children_head;
  while (child != NULL)
  {
    AuraTaskFrame *next = child->cancel_sibling_next;
    if (child->state != AURA_TASK_COMPLETE && child->state != AURA_TASK_FAILED &&
        child->state != AURA_TASK_CANCELLED && child->executor != NULL)
    {
      child->cancel_requested = 1;
      aura_task_channel_cancel_wait(child);
      aura_task_frame_detach_wait_target(child);
      aura_task_frame_clear_waiting(child);
      if (!child->queued)
      {
        (void)aura_task_executor_wake(child->executor, child);
      }
    }
    child = next;
  }
}

static void aura_task_frame_wake_waiters(AuraTaskFrame *frame)
{
  AuraTaskFrame *waiter;

  if (frame == NULL)
  {
    return;
  }
  waiter = frame->waiters_head;
  frame->waiters_head = NULL;
  while (waiter != NULL)
  {
    AuraTaskFrame *next = waiter->waiter_next;
    waiter->wait_target = NULL;
    waiter->waiter_next = NULL;
    if (waiter->waiting_node == frame)
    {
      waiter->waiting_node = NULL;
    }
    if (waiter->executor != NULL && !waiter->executor->shutdown &&
        waiter->state != AURA_TASK_COMPLETE && waiter->state != AURA_TASK_FAILED &&
        waiter->state != AURA_TASK_CANCELLED)
    {
      (void)aura_task_executor_wake(waiter->executor, waiter);
    }
    waiter = next;
  }
}

/* Register a parent frame against one child frame. The child owns no parent
 * memory; the embedded links are detached on cancellation/destruction and
 * all waiters are queued exactly once when the child becomes terminal. */
int aura_task_frame_wait_on(AuraTaskFrame *frame, AuraTaskFrame *target)
{
  if (frame == NULL || target == NULL || frame == target ||
      frame->executor == NULL || frame->executor != target->executor ||
      frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED || target->state == AURA_TASK_COMPLETE ||
      target->state == AURA_TASK_FAILED || target->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  aura_task_frame_detach_wait_target(frame);
  frame->wait_target = target;
  frame->waiter_next = target->waiters_head;
  target->waiters_head = frame;
  frame->waiting_node = target;
  frame->state = AURA_TASK_PENDING;
  return 1;
}

/* Complete an adapter-owned wait registration and queue the frame in one
 * bounded-runtime operation. The token is cleared before queueing so a
 * completion/failure callback cannot wake the same frame twice or leave a
 * borrowed registration visible while the poller resumes. */
int aura_task_executor_wake_waiting(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  if (executor == NULL || frame == NULL || frame->executor != executor ||
      !aura_task_frame_is_waiting(frame))
  {
    return 0;
  }
  aura_task_frame_clear_waiting(frame);
  return aura_task_executor_wake(executor, frame);
}

/* Poll all executor-owned frames registered with wait_fd. A zero return means
 * no descriptor became ready before timeout; a positive return is the number
 * of frames cleared and queued. This bounded single-threaded API provides a
 * deterministic multi-descriptor readiness turn without claiming a full
 * cross-platform event-loop policy. */
static int aura_task_executor_poll_waiting_impl(AuraTaskExecutor *executor,
                                                int timeout_ms)
{
  AuraTaskFrame *frame;
  struct pollfd *descriptors;
  AuraTaskFrame **frames;
  size_t count = 0;
  size_t descriptor_count;
  size_t index = 0;
  size_t woke = 0;
  int pipe_woke = 0;
  int result;
  int64_t now;
  int poll_timeout = timeout_ms;
  int has_deadline = 0;

  if (executor == NULL || executor->shutdown || timeout_ms < 0)
  {
    return 0;
  }
#if defined(AURA_TCP_POSIX)
  /* Consume stale submissions before blocking; a wake arriving after this
   * drain remains visible to poll and interrupts the wait. */
  if (executor->wake_pipe[0] >= 0)
  {
    unsigned char buffer[64];
    while (read(executor->wake_pipe[0], buffer, sizeof(buffer)) > 0) {}
  }
#endif
  for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
  {
    if (frame->state != AURA_TASK_PENDING)
    {
      continue;
    }
    if (frame->cancel_deadline_ms != 0)
    {
      int64_t remaining = frame->cancel_deadline_ms - aura_time_monotonic_millis();
      if (remaining <= 0)
      {
        woke += (size_t)aura_task_executor_cancel(executor, frame);
        continue;
      }
      if (remaining < poll_timeout) poll_timeout = (int)remaining;
      has_deadline = 1;
    }
    if (frame->waiting_node == NULL)
    {
      continue;
    }
    if (frame->fd_wait_deadline_ms != 0)
    {
      int64_t remaining;
      now = aura_time_monotonic_millis();
      remaining = frame->fd_wait_deadline_ms - now;
      if (remaining <= 0)
      {
        frame->fd_wait_timed_out = 1;
        woke += (size_t)aura_task_executor_wake_waiting(executor, frame);
        continue;
      }
      if (remaining < poll_timeout)
      {
        poll_timeout = (int)remaining;
      }
      has_deadline = 1;
    }
    if (frame->fd_wait_active)
    {
      count++;
    }
  }
  if (woke != 0)
  {
    return (int)woke;
  }
  descriptor_count = count;
#if defined(AURA_TCP_POSIX)
  if (executor->wake_pipe[0] >= 0)
  {
    descriptor_count++;
  }
#endif
  if (descriptor_count == 0 && !has_deadline)
  {
    return 0;
  }
  if (descriptor_count == 0)
  {
    (void)poll(NULL, 0, poll_timeout);
    now = aura_time_monotonic_millis();
    for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
    {
      if (frame->state != AURA_TASK_PENDING) continue;
      if (frame->cancel_deadline_ms != 0 && now >= frame->cancel_deadline_ms)
      {
        woke += (size_t)aura_task_executor_cancel(executor, frame);
        continue;
      }
      if (frame->waiting_node == NULL || frame->fd_wait_deadline_ms == 0 ||
          now < frame->fd_wait_deadline_ms)
      {
        continue;
      }
      frame->fd_wait_timed_out = 1;
      woke += (size_t)aura_task_executor_wake_waiting(executor, frame);
    }
    return (int)woke;
  }
  descriptors = (struct pollfd *)calloc(descriptor_count, sizeof(*descriptors));
  frames = (AuraTaskFrame **)calloc(descriptor_count, sizeof(*frames));
  if (descriptors == NULL || frames == NULL)
  {
    free(descriptors);
    free(frames);
    return 0;
  }
  index = 0;
#if defined(AURA_TCP_POSIX)
  if (executor->wake_pipe[0] >= 0)
  {
    descriptors[index] = (struct pollfd){executor->wake_pipe[0], POLLIN, 0};
    frames[index] = NULL;
    index++;
  }
#endif
  for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
  {
    if (!frame->fd_wait_active || frame->waiting_node == NULL ||
        frame->state != AURA_TASK_PENDING)
    {
      continue;
    }
    descriptors[index] = (struct pollfd){
      frame->fd_wait_fd,
      frame->fd_wait_events,
      0,
    };
    frames[index] = frame;
    index++;
  }
  result = poll(descriptors, descriptor_count, poll_timeout);
  if (result > 0 || (result < 0 && errno != EINTR))
  {
    for (index = 0; index < descriptor_count; index++)
    {
      if (result < 0 || descriptors[index].revents != 0)
      {
        AuraTaskFrame *ready_frame = frames[index];
#if defined(AURA_TCP_POSIX)
        if (ready_frame == NULL)
        {
          unsigned char buffer[64];
          while (read(executor->wake_pipe[0], buffer, sizeof(buffer)) > 0) {}
          pipe_woke = 1;
          continue;
        }
#endif
        if (result >= 0)
        {
          int operation_result =
              aura_io_operation_ready(ready_frame, descriptors[index].revents);
          if (operation_result > 0)
          {
            woke++;
          }
          else if (operation_result == 0)
          {
            woke += (size_t)aura_task_executor_wake_waiting(executor,
                                                             ready_frame);
          }
          /* A typed operation can consume a short nonblocking write and stay
           * pending. Its fd registration remains active for the next poll. */
        }
        else
        {
          woke += (size_t)aura_task_executor_wake_waiting(executor,
                                                           ready_frame);
        }
      }
    }
  }
  now = aura_time_monotonic_millis();
  for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
  {
    if (frame->state == AURA_TASK_PENDING && frame->cancel_deadline_ms != 0 &&
        now >= frame->cancel_deadline_ms)
    {
      woke += (size_t)aura_task_executor_cancel(executor, frame);
      continue;
    }
    if (frame->waiting_node == NULL || frame->state != AURA_TASK_PENDING ||
        frame->fd_wait_deadline_ms == 0 || now < frame->fd_wait_deadline_ms)
    {
      continue;
    }
    frame->fd_wait_timed_out = 1;
    woke += (size_t)aura_task_executor_wake_waiting(executor, frame);
  }
  free(descriptors);
  free(frames);
  if (woke == 0 && pipe_woke && executor->ready_count != 0)
  {
    return 1;
  }
  return (int)woke;
}

int aura_task_executor_poll_waiting(AuraTaskExecutor *executor, int timeout_ms)
{
  int result;
  if (executor == NULL) return 0;
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->reactor_active++;
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  result = aura_task_executor_poll_waiting_impl(executor, timeout_ms);
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    if (executor->reactor_active != 0) executor->reactor_active--;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  return result;
}

int aura_task_executor_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  if (executor == NULL || frame == NULL || frame->executor != executor || executor->shutdown)
  {
    return 0;
  }
  if (frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  frame->cancel_requested = 1;
  if (frame->waiting_select != NULL)
    aura_task_select_cancel_wait(frame);
  else
    aura_task_channel_cancel_wait(frame);
  aura_task_frame_detach_wait_target(frame);
  aura_task_frame_clear_waiting(frame);
  if (!frame->queued)
  {
    aura_task_executor_wake(executor, frame);
  }
  return 1;
}

size_t aura_task_executor_ready_count(const AuraTaskExecutor *executor)
{
  size_t count = 0;
  if (executor != NULL)
  {
    aura_task_executor_lock((AuraTaskExecutor *)executor);
    count = executor->ready_count;
    aura_task_executor_unlock((AuraTaskExecutor *)executor);
  }
  return count;
}

size_t aura_task_executor_task_count(const AuraTaskExecutor *executor)
{
  size_t count = 0;
  if (executor != NULL)
  {
    aura_task_executor_lock((AuraTaskExecutor *)executor);
    count = executor->owned_count;
    aura_task_executor_unlock((AuraTaskExecutor *)executor);
  }
  return count;
}

static AuraTaskFrame *aura_task_executor_pop_ready_unlocked(
    AuraTaskExecutor *executor)
{
  AuraTaskFrame *frame;
  if (executor == NULL || executor->ready_head == NULL)
  {
    return NULL;
  }
  frame = executor->ready_head;
  executor->ready_head = frame->queue_next;
  if (executor->ready_head == NULL)
  {
    executor->ready_tail = NULL;
  }
  frame->queue_next = NULL;
  frame->queued = 0;
  executor->ready_count--;
  return frame;
}

static void aura_task_executor_finish_poll_unlocked(
    AuraTaskExecutor *executor, AuraTaskFrame *frame, AuraTaskPollState state)
{
  if (executor == NULL || frame == NULL)
  {
    return;
  }
  if (state == AURA_TASK_READY)
  {
    (void)aura_task_executor_wake_unlocked(executor, frame);
  }
  else if (state == AURA_TASK_PENDING || state == AURA_TASK_COMPLETE ||
           state == AURA_TASK_FAILED || state == AURA_TASK_CANCELLED)
  {
    frame->state = state;
  }
  else
  {
    frame->state = AURA_TASK_FAILED;
  }
  if (executor->race_tracker != NULL &&
      (state == AURA_TASK_COMPLETE || state == AURA_TASK_FAILED ||
       state == AURA_TASK_CANCELLED))
  {
    AuraRaceEventKind kind = AURA_RACE_TASK_COMPLETE;
    if (state == AURA_TASK_FAILED)
    {
      kind = AURA_RACE_TASK_FAILED;
    }
    else if (state == AURA_TASK_CANCELLED)
    {
      kind = AURA_RACE_TASK_CANCELLED;
    }
    (void)aura_race_tracker_record(executor->race_tracker, frame->task_id, 0,
                                   frame->race_source_id, kind, NULL);
  }
}

#if defined(AURA_TCP_POSIX)
static void *aura_task_executor_worker_main(void *context)
{
  AuraTaskExecutor *executor = (AuraTaskExecutor *)context;
  for (;;)
  {
    AuraTaskFrame *frame;
    AuraTaskPollState state;
    AuraTaskScope *previous_scope;
    pthread_mutex_lock(&executor->worker_lock);
    while ((executor->ready_head == NULL || executor->gc_requested) &&
           !executor->workers_stop)
    {
      pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
    }
    if (executor->ready_head == NULL && executor->workers_stop)
    {
      pthread_mutex_unlock(&executor->worker_lock);
      return NULL;
    }
    frame = aura_task_executor_pop_ready_unlocked(executor);
    if (frame != NULL) executor->active_workers++;
    pthread_mutex_unlock(&executor->worker_lock);
    if (frame == NULL)
    {
      continue;
    }
    previous_scope = aura_task_current_scope;
    aura_task_current_scope = frame->scope;
    AuraTaskExecutor *previous_executor = aura_task_current_executor;
    aura_task_current_executor = executor;
    aura_race_active_task_id = frame->task_id;
    aura_race_active_source_id = frame->race_source_id;
    if (frame->blocking_fn != NULL && !frame->blocking_started)
      aura_task_blocking_run_inline(frame);
    state = aura_task_frame_poll_once(frame);
    aura_race_active_task_id = 0;
    aura_race_active_source_id = 0;
    aura_task_current_scope = previous_scope;
    aura_task_current_executor = previous_executor;
    pthread_mutex_lock(&executor->worker_lock);
    aura_task_executor_finish_poll_unlocked(executor, frame, state);
    if (executor->active_workers != 0) executor->active_workers--;
    pthread_cond_broadcast(&executor->worker_cond);
    if (state == AURA_TASK_READY)
    {
      pthread_cond_signal(&executor->worker_cond);
    }
    pthread_mutex_unlock(&executor->worker_lock);
  }
}

static void *aura_task_executor_reactor_main(void *context)
{
  AuraTaskExecutor *executor = (AuraTaskExecutor *)context;
  for (;;)
  {
    pthread_mutex_lock(&executor->worker_lock);
    int stopping = executor->reactor_stop;
    pthread_mutex_unlock(&executor->worker_lock);
    if (stopping) return NULL;
    (void)aura_task_executor_poll_waiting(executor, 100);
  }
}

int aura_task_executor_start_workers(AuraTaskExecutor *executor,
                                     size_t worker_count)
{
  size_t started = 0;
  if (executor == NULL || executor->shutdown || executor->workers_started ||
      worker_count == 0 || worker_count > AURA_TASK_MAX_WORKERS)
  {
    return 0;
  }
  if (pthread_mutex_init(&executor->worker_lock, NULL) != 0)
  {
    return 0;
  }
  if (pthread_cond_init(&executor->worker_cond, NULL) != 0)
  {
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->workers = (pthread_t *)calloc(worker_count, sizeof(*executor->workers));
  if (executor->workers == NULL)
  {
    pthread_cond_destroy(&executor->worker_cond);
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->worker_count = worker_count;
  executor->workers_stop = 0;
  executor->workers_started = 1;
  for (; started < worker_count; started++)
  {
    if (pthread_create(&executor->workers[started], NULL,
                       aura_task_executor_worker_main, executor) != 0)
    {
      break;
    }
  }
  if (started != worker_count)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->workers_stop = 1;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
    for (size_t i = 0; i < started; i++)
    {
      pthread_join(executor->workers[i], NULL);
    }
    free(executor->workers);
    executor->workers = NULL;
    executor->worker_count = 0;
    executor->workers_started = 0;
    pthread_cond_destroy(&executor->worker_cond);
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->reactor_stop = 0;
  if (pthread_create(&executor->reactor_thread, NULL,
                    aura_task_executor_reactor_main, executor) != 0)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->workers_stop = 1;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
    for (size_t i = 0; i < worker_count; i++)
      pthread_join(executor->workers[i], NULL);
    free(executor->workers);
    executor->workers = NULL;
    executor->worker_count = 0;
    executor->workers_started = 0;
    pthread_cond_destroy(&executor->worker_cond);
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->reactor_started = 1;
  return 1;
}

void aura_task_executor_stop_workers(AuraTaskExecutor *executor)
{
  if (executor == NULL || !executor->workers_started)
  {
    return;
  }
  pthread_mutex_lock(&executor->worker_lock);
  executor->reactor_stop = 1;
  pthread_mutex_unlock(&executor->worker_lock);
  if (executor->reactor_started)
  {
    if (executor->wake_pipe[1] >= 0)
    {
      const unsigned char signal_byte = 1;
      (void)write(executor->wake_pipe[1], &signal_byte, sizeof(signal_byte));
    }
    pthread_join(executor->reactor_thread, NULL);
    executor->reactor_started = 0;
  }
  pthread_mutex_lock(&executor->worker_lock);
  executor->workers_stop = 1;
  pthread_cond_broadcast(&executor->worker_cond);
  pthread_mutex_unlock(&executor->worker_lock);
  for (size_t i = 0; i < executor->worker_count; i++)
  {
    pthread_join(executor->workers[i], NULL);
  }
  free(executor->workers);
  executor->workers = NULL;
  executor->worker_count = 0;
  executor->workers_started = 0;
  pthread_cond_destroy(&executor->worker_cond);
  pthread_mutex_destroy(&executor->worker_lock);
}
#endif

int aura_task_executor_run_one(AuraTaskExecutor *executor)
{
  AuraTaskFrame *frame;
  AuraTaskPollState state;
  AuraTaskScope *previous_scope;
  uint64_t previous_task_id;
  uint32_t previous_source_id;
  if (executor == NULL || executor->shutdown)
  {
    return 0;
  }
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    while (executor->gc_requested && !executor->workers_stop)
      pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
  }
#endif
  frame = aura_task_executor_pop_ready_unlocked(executor);
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started && frame != NULL) executor->active_workers++;
  if (executor->workers_started)
  {
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  if (frame == NULL)
  {
    return 0;
  }
  previous_task_id = aura_race_active_task_id;
  previous_source_id = aura_race_active_source_id;
  previous_scope = aura_task_current_scope;
  aura_task_current_scope = frame->scope;
  AuraTaskExecutor *previous_executor = aura_task_current_executor;
  aura_task_current_executor = executor;
  aura_race_active_task_id = frame->task_id;
  aura_race_active_source_id = frame->race_source_id;
  if (frame->blocking_fn != NULL && !frame->blocking_started)
    aura_task_blocking_run_inline(frame);
  state = aura_task_frame_poll_once(frame);
  aura_race_active_task_id = previous_task_id;
  aura_race_active_source_id = previous_source_id;
  aura_task_current_scope = previous_scope;
  aura_task_current_executor = previous_executor;
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
  }
#endif
  aura_task_executor_finish_poll_unlocked(executor, frame, state);
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    if (executor->active_workers != 0) executor->active_workers--;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  return 1;
}

size_t aura_task_executor_run(AuraTaskExecutor *executor)
{
  size_t polled = 0;
  while (aura_task_executor_run_one(executor) != 0)
  {
    polled++;
  }
  return polled;
}

int aura_task_executor_has_live_tasks(const AuraTaskExecutor *executor)
{
  int live = 0;
  if (executor == NULL || executor->shutdown)
  {
    return 0;
  }
  aura_task_executor_lock((AuraTaskExecutor *)executor);
  for (const AuraTaskFrame *frame = executor->owned_head; frame != NULL;
       frame = frame->owned_next)
  {
    if (frame->state != AURA_TASK_COMPLETE && frame->state != AURA_TASK_FAILED &&
        frame->state != AURA_TASK_CANCELLED)
    {
      live = 1;
      break;
    }
  }
  aura_task_executor_unlock((AuraTaskExecutor *)executor);
  return live;
}

/* Observe a frame owned by this executor. Joining an unsubmitted frame
 * submits it exactly once; joining an already-owned frame only observes it.
 * Result and error snapshots are borrowed from executor-owned frame storage.
 * A PENDING result is explicit: no wake source is available to this bounded
 * single-threaded helper, so it does not pretend to support delayed awaits. */
AuraTaskOutcome aura_task_executor_join_outcome(AuraTaskExecutor *executor,
                                                AuraTaskFrame *frame)
{
  AuraTaskOutcome outcome = {AURA_TASK_FAILED, {NULL, 0}, {NULL, 0}};
  if (executor == NULL || frame == NULL || executor->shutdown)
  {
    return outcome;
  }
  if (frame->executor == NULL && frame->state != AURA_TASK_COMPLETE &&
      frame->state != AURA_TASK_FAILED && frame->state != AURA_TASK_CANCELLED)
  {
    if (!aura_task_executor_submit(executor, frame))
    {
      return outcome;
    }
  }
  else if (frame->executor != NULL && frame->executor != executor)
  {
    return outcome;
  }

  while (frame->state != AURA_TASK_COMPLETE &&
         frame->state != AURA_TASK_FAILED &&
         frame->state != AURA_TASK_CANCELLED)
  {
    if (aura_task_executor_run_one(executor) != 0)
    {
      continue;
    }
    /* A compiler-generated I/O frame can be the only ready source left. */
    if (aura_task_executor_poll_waiting(executor, 1000) == 0)
    {
      break;
    }
  }

  outcome.state = frame->state;
  outcome.result = frame->result;
  outcome.error = frame->error;
  if (frame->state == AURA_TASK_FAILED)
  {
    frame->join_observed = 1;
  }
  if (executor->race_tracker != NULL &&
      (frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
       frame->state == AURA_TASK_CANCELLED))
  {
    (void)aura_race_tracker_record(executor->race_tracker,
                                   frame->task_id,
                                   aura_race_active_source_id,
                                   0,
                                   AURA_RACE_TASK_JOIN,
                                   NULL);
  }
  return outcome;
}

int aura_task_outcome_clone(const AuraTaskOutcome *source,
                            AuraTaskResultCloneFn result_clone,
                            AuraTaskResultDestroyFn result_destroy,
                            AuraTaskResultCloneFn error_clone,
                            AuraTaskResultDestroyFn error_destroy,
                            AuraTaskOwnedOutcome *out)
{
  size_t cloned_size = 0;

  if (source == NULL || out == NULL)
  {
    return 0;
  }
  *out = (AuraTaskOwnedOutcome){source->state, {NULL, 0}, {NULL, 0}, NULL, NULL};
  if (source->state == AURA_TASK_COMPLETE && source->result.data != NULL)
  {
    if (result_clone == NULL || result_destroy == NULL)
    {
      return 0;
    }
    out->result.data = result_clone(source->result.data, source->result.size,
                                    &cloned_size);
    if (out->result.data == NULL)
    {
      return 0;
    }
    out->result.size = cloned_size;
    out->result_destroy = result_destroy;
  }
  if (source->state == AURA_TASK_FAILED && source->error.data != NULL)
  {
    cloned_size = 0;
    if (error_clone == NULL || error_destroy == NULL)
    {
      aura_task_owned_outcome_destroy(out);
      return 0;
    }
    out->error.data = error_clone(source->error.data, source->error.size,
                                  &cloned_size);
    if (out->error.data == NULL)
    {
      aura_task_owned_outcome_destroy(out);
      return 0;
    }
    out->error.size = cloned_size;
    out->error_destroy = error_destroy;
  }
  return 1;
}

void aura_task_owned_outcome_destroy(AuraTaskOwnedOutcome *out)
{
  if (out == NULL)
  {
    return;
  }
  if (out->result.data != NULL && out->result_destroy != NULL)
  {
    out->result_destroy(out->result.data, out->result.size);
  }
  if (out->error.data != NULL && out->error_destroy != NULL)
  {
    out->error_destroy(out->error.data, out->error.size);
  }
  *out = (AuraTaskOwnedOutcome){0, {NULL, 0}, {NULL, 0}, NULL, NULL};
}

AuraTaskPollState aura_task_executor_join(AuraTaskExecutor *executor,
                                          AuraTaskFrame *frame,
                                          AuraTaskResult *out_result,
                                          AuraTaskResult *out_error)
{
  AuraTaskOutcome outcome = aura_task_executor_join_outcome(executor, frame);
  if (out_result != NULL)
  {
    *out_result = outcome.result;
  }
  if (out_error != NULL)
  {
    *out_error = outcome.error;
  }
  return outcome.state;
}

/* Remove a frame from the FIFO ready queue before releasing it. */
static int aura_task_executor_unqueue(AuraTaskExecutor *executor,
                                       AuraTaskFrame *frame)
{
  AuraTaskFrame **link;

  if (executor == NULL || frame == NULL || !frame->queued)
  {
    return frame != NULL && !frame->queued;
  }
  link = &executor->ready_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->queue_next;
  }
  if (*link == NULL)
  {
    return 0;
  }
  *link = frame->queue_next;
  if (executor->ready_tail == frame)
  {
    executor->ready_tail = NULL;
    for (AuraTaskFrame *tail = executor->ready_head; tail != NULL;
         tail = tail->queue_next)
    {
      executor->ready_tail = tail;
    }
  }
  frame->queue_next = NULL;
  frame->queued = 0;
  if (executor->ready_count != 0)
  {
    executor->ready_count--;
  }
  return 1;
}

/* Release an executor-owned frame through its task-handle slot.
 *
 * The pointer-to-pointer API is intentional: releasing also clears the
 * caller's handle, making repeated release and dropped-handle cleanup
 * idempotent without dereferencing freed storage.  A non-terminal frame is
 * cancelled and polled to acknowledge cancellation before it is unlinked.
 * The owned list is singly linked, so unlink the exact node before destroying
 * it; shutdown can then walk the remaining list without observing freed nodes.
 */
int aura_task_executor_release(AuraTaskExecutor *executor, AuraTaskFrame **handle)
{
  AuraTaskFrame *frame;
  AuraTaskFrame **link;

  if (handle == NULL || *handle == NULL)
  {
    return 1;
  }
  frame = *handle;
  if (executor == NULL || executor->shutdown || frame->executor != executor)
  {
    return 0;
  }
  if (frame->scope_owned)
  {
    /* The lexical handle gives up only its reference; the active scope keeps
     * the executor-owned frame alive until the scope drains it. */
    frame->handle_owned = 0;
    *handle = NULL;
    return 1;
  }
  if (frame->state != AURA_TASK_COMPLETE && frame->state != AURA_TASK_FAILED &&
      frame->state != AURA_TASK_CANCELLED)
  {
    if (!aura_task_executor_cancel(executor, frame))
    {
      return 0;
    }
#if defined(AURA_TCP_POSIX)
    if (executor->workers_started)
    {
      pthread_mutex_lock(&executor->worker_lock);
      while (frame->state != AURA_TASK_COMPLETE &&
             frame->state != AURA_TASK_FAILED &&
             frame->state != AURA_TASK_CANCELLED && !executor->shutdown)
      {
        pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
      }
      pthread_mutex_unlock(&executor->worker_lock);
    }
    else
#endif
    if (!aura_task_executor_unqueue(executor, frame) ||
        (aura_task_frame_poll_once(frame) != AURA_TASK_CANCELLED &&
         frame->state != AURA_TASK_FAILED))
    {
      return 0;
    }
  }
  aura_task_executor_lock(executor);
#if defined(AURA_TCP_POSIX)
  while (executor->workers_started && executor->reactor_active != 0)
  {
    pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
  }
#endif
  if (frame->queued || frame->waiting_channel != NULL ||
      frame->waiting_node != NULL)
  {
    aura_task_executor_unlock(executor);
    return 0;
  }

  link = &executor->owned_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->owned_next;
  }
  if (*link == NULL)
  {
    aura_task_executor_unlock(executor);
    return 0;
  }
  *link = frame->owned_next;
  frame->owned_next = NULL;
  frame->executor = NULL;
  if (executor->owned_count != 0)
  {
    executor->owned_count--;
  }
  aura_task_executor_unlock(executor);
  *handle = NULL;
  frame->handle_owned = 0;
  aura_task_executor_report_unjoined_failure(executor, frame);
  aura_task_frame_destroy(frame);
  return 1;
}

AuraTaskScope *aura_task_scope_begin(AuraTaskExecutor *executor)
{
  AuraTaskScope *scope;
  if (executor == NULL || executor->shutdown)
  {
    return NULL;
  }
  scope = (AuraTaskScope *)calloc(1, sizeof(*scope));
  if (scope == NULL)
  {
    return NULL;
  }
  scope->executor = executor;
  scope->previous = aura_task_current_scope;
  scope->active = 1;
  aura_task_current_scope = scope;
  return scope;
}

static int aura_task_scope_has_live_frames(const AuraTaskScope *scope)
{
  for (const AuraTaskFrame *frame = scope->frames; frame != NULL;
       frame = frame->scope_next)
  {
    if (frame->state != AURA_TASK_COMPLETE &&
        frame->state != AURA_TASK_FAILED &&
        frame->state != AURA_TASK_CANCELLED)
    {
      return 1;
    }
  }
  return 0;
}

int aura_task_scope_end(AuraTaskScope *scope)
{
  AuraTaskFrame *frame;
  int outcome = 0;
  if (scope == NULL || !scope->active)
  {
    return 0;
  }
  scope->active = 0;
  if (aura_task_current_scope == scope)
  {
    aura_task_current_scope = scope->previous;
  }
  while (aura_task_scope_has_live_frames(scope))
  {
    if (aura_task_executor_run_one(scope->executor) == 0 &&
        aura_task_executor_poll_waiting(scope->executor, 1000) == 0)
    {
      break;
    }
  }
  for (frame = scope->frames; frame != NULL; frame = frame->scope_next)
  {
    if (frame->state == AURA_TASK_FAILED) outcome = 1;
    else if (frame->state == AURA_TASK_CANCELLED && outcome == 0) outcome = 2;
  }
  frame = scope->frames;
  scope->frames = NULL;
  while (frame != NULL)
  {
    AuraTaskFrame *next = frame->scope_next;
    frame->scope_next = NULL;
    frame->scope = NULL;
    frame->scope_owned = 0;
    if (!frame->handle_owned && frame->executor == scope->executor)
    {
      AuraTaskFrame *owned = frame;
      (void)aura_task_executor_release(scope->executor, &owned);
    }
    frame = next;
  }
  free(scope);
  return outcome;
}

/* Release a frame whose terminal state was observed by a direct parent poll.
 * Normal task handles keep the stricter queued-frame rule because their result
 * may still be borrowed by generated outcome code. */
int aura_task_executor_release_terminal(AuraTaskExecutor *executor,
                                         AuraTaskFrame **handle)
{
  if (handle == NULL || *handle == NULL || executor == NULL || executor->shutdown)
  {
    return handle == NULL || *handle == NULL ? 1 : 0;
  }
  AuraTaskFrame *frame = *handle;
  if (frame->state != AURA_TASK_COMPLETE && frame->state != AURA_TASK_FAILED &&
      frame->state != AURA_TASK_CANCELLED)
  {
    return 0;
  }
  if (frame->queued && !aura_task_executor_unqueue(executor, frame))
  {
    return 0;
  }
  return aura_task_executor_release(executor, handle);
}

void aura_task_executor_shutdown(AuraTaskExecutor *executor)
{
  if (executor == NULL || executor->shutdown)
  {
    return;
  }
#if defined(AURA_TCP_POSIX)
  aura_task_executor_stop_workers(executor);
#endif
  executor->shutdown = 1;
  AuraTaskFrame *frame = executor->owned_head;
  while (frame != NULL)
  {
    AuraTaskFrame *next = frame->owned_next;
    frame->executor = NULL;
    frame->queued = 0;
    aura_task_channel_cancel_wait(frame);
    aura_task_executor_report_unjoined_failure(executor, frame);
    aura_task_frame_destroy(frame);
    frame = next;
  }
#if defined(AURA_TCP_POSIX)
  if (executor->wake_pipe[0] >= 0)
  {
    close(executor->wake_pipe[0]);
  }
  if (executor->wake_pipe[1] >= 0)
  {
    close(executor->wake_pipe[1]);
  }
#endif
  free(executor);
}

/* Small POSIX descriptor bridge used by compiler-generated std.io frames.
 * Encode errno as a negative result so generated C can retry EAGAIN without
 * exposing a process-global error slot across suspension. */
int64_t aura_io_read_fd(int fd, void *buffer, uint64_t capacity)
{
  ssize_t result;
  if (fd < 0 || (capacity != 0 && buffer == NULL) || capacity > SIZE_MAX)
  {
    return -EINVAL;
  }
  result = read(fd, buffer, (size_t)capacity);
  if (result < 0)
  {
    return -(int64_t)errno;
  }
  return (int64_t)result;
}

/* Encode errno as a negative result so generated C can retry EAGAIN after
 * suspension without exposing a process-global error slot. */
int64_t aura_io_write_fd(int fd, const void *buffer, uint64_t length)
{
  ssize_t result;
  if (fd < 0 || (length != 0 && buffer == NULL) || length > SIZE_MAX)
  {
    return -EINVAL;
  }
  result = write(fd, buffer, (size_t)length);
  if (result < 0)
  {
    return -(int64_t)errno;
  }
  return (int64_t)result;
}

/* ---- G3 asynchronous file/TCP operation handles ----
 *
 * File and TCP adapters need a lifetime-bearing token in addition to the
 * frame's borrowed readiness registration.  The token is deliberately
 * small: the task owns its buffer and performs the bounded read/write after
 * the token is completed.  Cancellation owns the resource cleanup callback;
 * completion never invokes it, because the task still has to consume the
 * operation result.  This makes cancellation and completion mutually
 * exclusive and gives adapters one place to enforce exactly-once cleanup.
 */

struct AuraIoOperationHandle
{
  AuraTaskExecutor *executor;
  AuraTaskFrame *frame;
  AuraIoOperationKind kind;
  AuraIoOperationState state;
  int fd;
  short events;
  void *resource;
  AuraIoOperationCleanupFn cleanup;
  int cleanup_done;
  void *buffer;
  uint64_t length;
  uint64_t offset;
  int typed;
  AuraIoOperationResult result;
};

static void aura_io_operation_cleanup_once(AuraIoOperationHandle *operation)
{
  if (operation == NULL || operation->cleanup_done)
  {
    return;
  }
  operation->cleanup_done = 1;
  if (operation->cleanup != NULL && operation->resource != NULL)
  {
    operation->cleanup(operation->resource);
  }
}

static void aura_io_operation_frame_cleanup(void *data)
{
  AuraIoOperationHandle *operation = (AuraIoOperationHandle *)data;
  if (operation == NULL)
  {
    return;
  }
  if (operation->state == AURA_IO_OPERATION_PENDING)
  {
    operation->state = AURA_IO_OPERATION_CANCELLED;
    operation->result.state = AURA_IO_OPERATION_CANCELLED;
    operation->result.outcome = AURA_IO_OUTCOME_CANCELLED;
  }
  aura_io_operation_cleanup_once(operation);
  operation->frame = NULL;
  operation->executor = NULL;
}

static AuraIoOperationHandle *aura_io_operation_handle_new(
    AuraIoOperationKind kind, int fd, short events, void *resource,
    AuraIoOperationCleanupFn cleanup)
{
  AuraIoOperationHandle *operation;
  if (fd < 0 || events == 0 || resource == NULL)
  {
    return NULL;
  }
  operation = (AuraIoOperationHandle *)calloc(1, sizeof(*operation));
  if (operation == NULL)
  {
    return NULL;
  }
  operation->kind = kind;
  operation->state = AURA_IO_OPERATION_PENDING;
  operation->fd = fd;
  operation->events = events;
  operation->resource = resource;
  operation->cleanup = cleanup;
  operation->result.kind = kind;
  operation->result.state = AURA_IO_OPERATION_PENDING;
  operation->result.outcome = AURA_IO_OUTCOME_OK;
  return operation;
}

static AuraIoOperationHandle *aura_io_typed_operation_new(
    AuraIoOperationKind kind, int fd, short events, void *resource,
    void *buffer, uint64_t length, AuraIoOperationCleanupFn cleanup)
{
  AuraIoOperationHandle *operation;
  if (length > 0 && buffer == NULL)
  {
    return NULL;
  }
  operation = aura_io_operation_handle_new(kind, fd, events, resource, cleanup);
  if (operation != NULL)
  {
    operation->buffer = buffer;
    operation->length = length;
    operation->typed = 1;
  }
  return operation;
}

AuraIoOperationHandle *aura_file_async_read_handle_new(
    AuraFile *file, AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_FILE_READ, file->fd,
                                     POLLIN, file, cleanup);
}

AuraIoOperationHandle *aura_file_async_write_handle_new(
    AuraFile *file, AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_FILE_WRITE, file->fd,
                                     POLLOUT, file, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_accept_handle_new(
    AuraTcpListener *listener, AuraIoOperationCleanupFn cleanup)
{
  if (listener == NULL || listener->fd < 0)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_TCP_ACCEPT,
                                      listener->fd, POLLIN, listener, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_read_handle_new(
    AuraTcpStream *stream, AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_TCP_READ, stream->fd,
                                     POLLIN, stream, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_write_handle_new(
    AuraTcpStream *stream, AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_TCP_WRITE,
                                     stream->fd, POLLOUT, stream, cleanup);
}

AuraIoOperationHandle *aura_file_async_read_operation_new(
    AuraFile *file, void *buffer, uint64_t capacity,
    AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_FILE_READ, file->fd,
                                     POLLIN, file, buffer, capacity, cleanup);
}

AuraIoOperationHandle *aura_file_async_write_operation_new(
    AuraFile *file, const void *buffer, uint64_t length,
    AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_FILE_WRITE, file->fd,
                                     POLLOUT, file, (void *)buffer, length,
                                     cleanup);
}

AuraIoOperationHandle *aura_tcp_async_read_operation_new(
    AuraTcpStream *stream, void *buffer, uint64_t capacity,
    AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0 || capacity > SIZE_MAX)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_TCP_READ, stream->fd,
                                     POLLIN, stream, buffer, capacity, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_write_operation_new(
    AuraTcpStream *stream, const void *buffer, uint64_t length,
    AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0 || length > SIZE_MAX)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_TCP_WRITE, stream->fd,
                                     POLLOUT, stream, (void *)buffer, length,
                                     cleanup);
}

int aura_io_operation_handle_start(AuraIoOperationHandle *operation,
                                   AuraTaskExecutor *executor,
                                   AuraTaskFrame *frame)
{
  if (operation == NULL || executor == NULL || frame == NULL ||
      operation->state != AURA_IO_OPERATION_PENDING ||
      frame->executor != executor || operation->frame != NULL)
  {
    return 0;
  }
  if (!aura_task_frame_wait_fd(frame, operation->fd, operation->events))
  {
    return 0;
  }
  operation->executor = executor;
  operation->frame = frame;
  /* wait_fd owns the inline descriptor registration.  Replace only its
   * borrowed token; set_waiting would intentionally disable fd polling. */
  frame->waiting_node = operation;
  aura_task_frame_set_cleanup(frame, operation, aura_io_operation_frame_cleanup);
  return 1;
}

AuraIoOperationState aura_io_operation_handle_state(
    const AuraIoOperationHandle *operation)
{
  return operation != NULL ? operation->state : AURA_IO_OPERATION_FAILED;
}

AuraIoOperationKind aura_io_operation_handle_kind(
    const AuraIoOperationHandle *operation)
{
  return operation != NULL ? operation->kind : 0;
}

int aura_io_operation_handle_result(const AuraIoOperationHandle *operation,
                                    AuraIoOperationResult *out)
{
  if (operation == NULL || out == NULL ||
      operation->state == AURA_IO_OPERATION_PENDING)
  {
    return 0;
  }
  *out = operation->result;
  return 1;
}

int aura_io_operation_handle_complete(AuraIoOperationHandle *operation,
                                      int success)
{
  int already_ready;
  if (operation == NULL || operation->state != AURA_IO_OPERATION_PENDING ||
      operation->executor == NULL || operation->frame == NULL ||
      operation->executor->shutdown)
  {
    return 0;
  }
  already_ready = operation->frame->queued;
  operation->state = success ? AURA_IO_OPERATION_COMPLETE
                             : AURA_IO_OPERATION_FAILED;
  operation->result.state = operation->state;
  if (!operation->typed)
  {
    operation->result.outcome =
        success ? AURA_IO_OUTCOME_OK : AURA_IO_OUTCOME_ERROR;
  }
  aura_task_frame_clear_waiting(operation->frame);
  aura_task_frame_clear_cleanup(operation->frame);
  if (!already_ready &&
      !aura_task_executor_wake(operation->executor, operation->frame))
  {
    operation->state = AURA_IO_OPERATION_FAILED;
    return 0;
  }
  operation->frame = NULL;
  operation->executor = NULL;
  return 1;
}

static AuraIoOutcome aura_io_file_outcome(AuraFileStatus status)
{
  switch (status)
  {
  case AURA_FILE_OK:
    return AURA_IO_OUTCOME_OK;
  case AURA_FILE_EOF:
    return AURA_IO_OUTCOME_EOF;
  case AURA_FILE_CLOSED:
    return AURA_IO_OUTCOME_CLOSED;
  case AURA_FILE_PERMISSION:
    return AURA_IO_OUTCOME_PERMISSION;
  case AURA_FILE_UNSUPPORTED:
    return AURA_IO_OUTCOME_UNSUPPORTED;
  default:
    return AURA_IO_OUTCOME_ERROR;
  }
}

static AuraIoOutcome aura_io_tcp_outcome(AuraTcpStatus status)
{
  switch (status)
  {
  case AURA_TCP_OK:
    return AURA_IO_OUTCOME_OK;
  case AURA_TCP_EOF:
    return AURA_IO_OUTCOME_EOF;
  case AURA_TCP_CLOSED:
    return AURA_IO_OUTCOME_CLOSED;
  case AURA_TCP_TIMEOUT:
    return AURA_IO_OUTCOME_TIMEOUT;
  case AURA_TCP_UNSUPPORTED:
    return AURA_IO_OUTCOME_UNSUPPORTED;
  default:
    return AURA_IO_OUTCOME_ERROR;
  }
}

static int aura_io_operation_perform(AuraIoOperationHandle *operation)
{
  uint64_t file_bytes = 0;
  size_t tcp_bytes = 0;
  uint64_t remaining = operation->length;
  void *buffer = operation->buffer;
  int32_t status;

  if (operation->kind == AURA_IO_OPERATION_FILE_WRITE ||
      operation->kind == AURA_IO_OPERATION_TCP_WRITE)
  {
    if (operation->offset > operation->length)
    {
      operation->result.outcome = AURA_IO_OUTCOME_ERROR;
      operation->result.native_status = AURA_FILE_ERROR;
      return 0;
    }
    remaining = operation->length - operation->offset;
    if (buffer != NULL)
    {
      buffer = (unsigned char *)buffer + operation->offset;
    }
  }

  switch (operation->kind)
  {
  case AURA_IO_OPERATION_FILE_READ:
    status = aura_file_read((AuraFile *)operation->resource, operation->buffer,
                            operation->length, &file_bytes);
    operation->result.outcome = aura_io_file_outcome((AuraFileStatus)status);
    operation->result.bytes_transferred = file_bytes;
    break;
  case AURA_IO_OPERATION_FILE_WRITE:
    status = aura_file_write((AuraFile *)operation->resource, buffer, remaining,
                             &file_bytes);
    operation->result.outcome = aura_io_file_outcome((AuraFileStatus)status);
    operation->offset += file_bytes;
    operation->result.bytes_transferred = operation->offset;
    break;
  case AURA_IO_OPERATION_TCP_READ:
    status = aura_tcp_stream_read((AuraTcpStream *)operation->resource,
                                  operation->buffer, (size_t)operation->length,
                                  &tcp_bytes, 0);
    operation->result.outcome = aura_io_tcp_outcome((AuraTcpStatus)status);
    operation->result.bytes_transferred = (uint64_t)tcp_bytes;
    break;
  case AURA_IO_OPERATION_TCP_WRITE:
    status = aura_tcp_stream_write((AuraTcpStream *)operation->resource, buffer,
                                   (size_t)remaining,
                                   &tcp_bytes, 0);
    operation->result.outcome = aura_io_tcp_outcome((AuraTcpStatus)status);
    operation->offset += (uint64_t)tcp_bytes;
    operation->result.bytes_transferred = operation->offset;
    break;
  default:
    status = AURA_FILE_ERROR;
    operation->result.outcome = AURA_IO_OUTCOME_ERROR;
    break;
  }
  operation->result.native_status = status;
  if (status == AURA_FILE_PENDING || status == AURA_TCP_PENDING)
  {
    return 0;
  }
  if ((operation->kind == AURA_IO_OPERATION_FILE_WRITE ||
       operation->kind == AURA_IO_OPERATION_TCP_WRITE) &&
      operation->offset < operation->length &&
      operation->result.outcome == AURA_IO_OUTCOME_OK)
  {
    operation->result.native_status =
        operation->kind == AURA_IO_OPERATION_FILE_WRITE ? AURA_FILE_PENDING
                                                        : AURA_TCP_PENDING;
    return 0;
  }
  return operation->result.outcome == AURA_IO_OUTCOME_OK ||
         operation->result.outcome == AURA_IO_OUTCOME_EOF;
}

static int aura_io_operation_ready(AuraTaskFrame *frame, short revents)
{
  AuraIoOperationHandle *operation;

  if (frame == NULL || frame->waiting_node == NULL ||
      frame->waiting_node == &frame->fd_wait_active)
  {
    return 0;
  }
  operation = (AuraIoOperationHandle *)frame->waiting_node;
  if (operation->frame != frame || operation->executor != frame->executor ||
      operation->state != AURA_IO_OPERATION_PENDING)
  {
    return 0;
  }
  if (operation->typed && (revents & POLLNVAL) == 0)
  {
    int success = aura_io_operation_perform(operation);
    if (operation->result.native_status == AURA_FILE_PENDING ||
        operation->result.native_status == AURA_TCP_PENDING)
    {
      return -1;
    }
    return aura_io_operation_handle_complete(operation, success);
  }
  /* POLLNVAL is a descriptor failure.  POLLERR/POLLHUP still wake the task so
   * its bounded read/write/accept call can publish EOF or the native error. */
  return aura_io_operation_handle_complete(operation,
                                           (revents & POLLNVAL) == 0);
}

int aura_io_operation_handle_cancel(AuraIoOperationHandle *operation)
{
  AuraTaskExecutor *executor;
  AuraTaskFrame *frame;
  if (operation == NULL || operation->state != AURA_IO_OPERATION_PENDING)
  {
    return 0;
  }
  executor = operation->executor;
  frame = operation->frame;
  operation->state = AURA_IO_OPERATION_CANCELLED;
  operation->result.state = AURA_IO_OPERATION_CANCELLED;
  operation->result.outcome = AURA_IO_OUTCOME_CANCELLED;
  if (frame != NULL)
  {
    aura_task_frame_clear_waiting(frame);
  }
  aura_io_operation_cleanup_once(operation);
  if (executor == NULL || frame == NULL)
  {
    return 1;
  }
  return aura_task_executor_cancel(executor, frame);
}

int aura_io_operation_handle_release(AuraIoOperationHandle **handle)
{
  AuraIoOperationHandle *operation;
  if (handle == NULL || *handle == NULL)
  {
    return 1;
  }
  operation = *handle;
  if (operation->state == AURA_IO_OPERATION_PENDING ||
      operation->frame != NULL)
  {
    return 0;
  }
  *handle = NULL;
  free(operation);
  return 1;
}

AuraFfiStatus aura_io_operation_handle_check_boundary(
    const AuraIoOperationHandle *operation, AuraFfiBoundary boundary)
{
  if (operation == NULL || operation->state != AURA_IO_OPERATION_PENDING)
  {
    return AURA_FFI_INVALID;
  }
  if (boundary == AURA_FFI_BOUNDARY_SYNC)
  {
    return operation->frame == NULL ? AURA_FFI_OK : AURA_FFI_BOUNDARY_REJECTED;
  }
  if (boundary == AURA_FFI_BOUNDARY_TASK)
  {
    return operation->frame != NULL ? AURA_FFI_OK
                                    : AURA_FFI_BOUNDARY_REJECTED;
  }
  return AURA_FFI_BOUNDARY_REJECTED;
}

/* ---- C22n bounded FIFO channels (single-threaded MVP) ----
 *
 * A channel owns every value accepted by aura_task_channel_send.  A queued
 * value is delivered by moving the value record to the receiver's output;
 * after that point the receiver owns it and must invoke its destroy callback.
 * Values rejected by a closed channel, or held by a waiting sender when the
 * channel closes, are destroyed exactly once by the channel.  A send that
 * returns AURA_CHANNEL_PENDING transfers ownership to the channel.
 *
 * Waiting frames are borrowed references.  The executor owns their lifetime;
 * cancellation and executor shutdown unlink waiters before destroying the
 * frame.  Wakeups are FIFO and use the frame's executor, with no OS threads.
 */

typedef void (*AuraTaskChannelValueDestroyFn)(void *data, size_t size);

typedef struct
{
  void *data;
  size_t size;
  AuraTaskChannelValueDestroyFn destroy;
} AuraTaskChannelValue;

typedef enum
{
  AURA_CHANNEL_OK = 0,
  AURA_CHANNEL_PENDING = 1,
  AURA_CHANNEL_CLOSED = 2,
  AURA_CHANNEL_ERROR = 3
} AuraTaskChannelStatus;

typedef struct AuraTaskChannelWaiter AuraTaskChannelWaiter;

struct AuraTaskChannelWaiter
{
  AuraTaskFrame *frame;
  AuraTaskChannelValue value;
  AuraTaskChannelValue *out;
  AuraTaskSelect *select;
  size_t select_index;
  AuraTaskChannelWaiter *next;
};

struct AuraTaskSelect
{
  AuraTaskChannel **channels;
  size_t count;
  size_t capacity;
  size_t next_index;
  AuraTaskFrame *frame;
  AuraTaskChannelValue value;
  size_t selected_index;
  AuraTaskChannelStatus selected_status;
  int selected;
};

AuraTaskChannelStatus aura_task_channel_receive(AuraTaskChannel *channel,
                                                 AuraTaskFrame *receiver,
                                                 AuraTaskChannelValue *out);

struct AuraTaskChannel
{
  AuraTaskChannelValue *values;
  size_t capacity;
  size_t head;
  size_t tail;
  size_t count;
  int closed;
  AuraTaskChannelWaiter *send_head;
  AuraTaskChannelWaiter *send_tail;
  AuraTaskChannelWaiter *receive_head;
  AuraTaskChannelWaiter *receive_tail;
  AuraRaceTracker *race_tracker;
};

#if defined(__unix__) || defined(__APPLE__)
static pthread_once_t aura_task_channel_lock_once = PTHREAD_ONCE_INIT;
static pthread_mutex_t aura_task_channel_lock;

static void aura_task_channel_lock_init(void)
{
  pthread_mutexattr_t attributes;
  pthread_mutexattr_init(&attributes);
  pthread_mutexattr_settype(&attributes, PTHREAD_MUTEX_RECURSIVE);
  pthread_mutex_init(&aura_task_channel_lock, &attributes);
  pthread_mutexattr_destroy(&attributes);
}

static void aura_task_channel_lock_enter(void)
{
  pthread_once(&aura_task_channel_lock_once, aura_task_channel_lock_init);
  pthread_mutex_lock(&aura_task_channel_lock);
}

static void aura_task_channel_lock_leave(void)
{
  pthread_mutex_unlock(&aura_task_channel_lock);
}
#else
static void aura_task_channel_lock_enter(void) {}
static void aura_task_channel_lock_leave(void) {}
#endif

void aura_task_channel_set_race_tracker(AuraTaskChannel *channel,
                                         AuraRaceTracker *tracker)
{
  if (channel != NULL)
  {
    channel->race_tracker = tracker;
  }
}

static void aura_task_channel_record(AuraTaskChannel *channel,
                                     AuraTaskFrame *frame,
                                     AuraRaceEventKind kind)
{
  if (channel != NULL && channel->race_tracker != NULL)
  {
    (void)aura_race_tracker_record(channel->race_tracker,
                                   frame != NULL ? frame->task_id : 0,
                                   (uintptr_t)channel,
                                   0,
                                   kind,
                                   NULL);
  }
}

static void aura_task_channel_value_destroy(AuraTaskChannelValue *value)
{
  if (value != NULL && value->destroy != NULL && value->data != NULL)
  {
    value->destroy(value->data, value->size);
  }
  if (value != NULL)
  {
    value->data = NULL;
    value->size = 0;
    value->destroy = NULL;
  }
}

/* C22o glue: generated send/receive expressions use these stable callbacks.
 * The class form also releases the temporary GC root held by the payload box. */
void aura_task_channel_value_destroy_free(void *data, size_t size)
{
  (void)size;
  free(data);
}

void aura_task_channel_value_destroy_class(void *data, size_t size)
{
  (void)size;
  if (data != NULL)
  {
    aura_gc_remove_root((void **)data);
    free(data);
  }
}

static void aura_task_channel_wake(AuraTaskFrame *frame)
{
  if (frame != NULL && frame->executor != NULL)
  {
    (void)aura_task_executor_wake(frame->executor, frame);
  }
}

static void aura_task_channel_unlink(AuraTaskChannel *channel,
                                     AuraTaskChannelWaiter *target,
                                     int receiver)
{
  AuraTaskChannelWaiter **link = receiver ? &channel->receive_head : &channel->send_head;
  AuraTaskChannelWaiter *tail = receiver ? channel->receive_tail : channel->send_tail;
  while (*link != NULL && *link != target)
  {
    link = &(*link)->next;
  }
  if (*link == NULL)
  {
    return;
  }
  *link = target->next;
  if (tail == target)
  {
    if (receiver)
    {
      channel->receive_tail = NULL;
      for (AuraTaskChannelWaiter *w = channel->receive_head; w != NULL; w = w->next)
        channel->receive_tail = w;
    }
    else
    {
      channel->send_tail = NULL;
      for (AuraTaskChannelWaiter *w = channel->send_head; w != NULL; w = w->next)
        channel->send_tail = w;
    }
  }
  target->next = NULL;
}

static void aura_task_channel_cancel_wait(AuraTaskFrame *frame)
{
  if (frame == NULL || frame->waiting_channel == NULL || frame->waiting_node == NULL)
  {
    return;
  }
  AuraTaskChannel *channel = frame->waiting_channel;
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)frame->waiting_node;
  aura_task_channel_lock_enter();
  int receiver = waiter->out != NULL;
  aura_task_channel_unlink(channel, waiter, receiver);
  if (!receiver)
  {
    aura_task_channel_value_destroy(&waiter->value);
  }
  free(waiter);
  frame->waiting_channel = NULL;
  frame->waiting_node = NULL;
  aura_task_channel_lock_leave();
}

static void aura_task_select_unlink_waiters(AuraTaskSelect *select)
{
  if (select == NULL) return;
  for (size_t i = 0; i < select->count; i++)
  {
    AuraTaskChannel *channel = select->channels[i];
    if (channel == NULL) continue;
    AuraTaskChannelWaiter **link = &channel->receive_head;
    while (*link != NULL)
    {
      AuraTaskChannelWaiter *waiter = *link;
      if (waiter->select != select)
      {
        link = &waiter->next;
        continue;
      }
      *link = waiter->next;
      if (channel->receive_tail == waiter)
      {
        channel->receive_tail = NULL;
        for (AuraTaskChannelWaiter *tail = channel->receive_head; tail != NULL; tail = tail->next)
          channel->receive_tail = tail;
      }
      free(waiter);
    }
  }
}

static void aura_task_select_cancel_wait(AuraTaskFrame *frame)
{
  if (frame == NULL || frame->waiting_select == NULL) return;
  aura_task_channel_lock_enter();
  AuraTaskSelect *select = frame->waiting_select;
  aura_task_select_unlink_waiters(select);
  select->frame = NULL;
  frame->waiting_select = NULL;
  frame->waiting_node = NULL;
  aura_task_channel_lock_leave();
}

static void aura_task_select_finish(AuraTaskSelect *select,
                                    AuraTaskChannelStatus status,
                                    size_t index,
                                    AuraTaskChannelValue value)
{
  if (select == NULL || select->selected) return;
  select->selected = 1;
  select->selected_index = index;
  select->selected_status = status;
  select->value = value;
  AuraTaskFrame *frame = select->frame;
  aura_task_select_unlink_waiters(select);
  select->frame = NULL;
  if (frame != NULL)
  {
    frame->waiting_select = NULL;
    frame->waiting_node = NULL;
    aura_task_channel_wake(frame);
  }
  (void)status;
}

AuraTaskSelect *aura_task_select_new(void)
{
  return (AuraTaskSelect *)calloc(1, sizeof(AuraTaskSelect));
}

int aura_task_select_add(AuraTaskSelect *select, AuraTaskChannel *channel)
{
  if (select == NULL || channel == NULL) return 0;
  aura_task_channel_lock_enter();
  if (select->count == select->capacity)
  {
    size_t capacity = select->capacity == 0 ? 4 : select->capacity * 2;
    AuraTaskChannel **channels = (AuraTaskChannel **)realloc(select->channels, capacity * sizeof(*channels));
    if (channels == NULL) { aura_task_channel_lock_leave(); return 0; }
    select->channels = channels;
    select->capacity = capacity;
  }
  select->channels[select->count++] = channel;
  aura_task_channel_lock_leave();
  return 1;
}

AuraTaskChannelStatus aura_task_select_next(AuraTaskSelect *select,
                                            AuraTaskFrame *frame,
                                            AuraTaskChannelValue *out,
                                            size_t *index)
{
  if (select == NULL || out == NULL || index == NULL || select->count == 0)
    return AURA_CHANNEL_ERROR;
  aura_task_channel_lock_enter();
  if (select->selected)
  {
    *out = select->value;
    select->value = (AuraTaskChannelValue){NULL, 0, NULL};
    *index = select->selected_index;
    select->selected = 0;
    AuraTaskChannelStatus status = select->selected_status;
    aura_task_channel_lock_leave();
    return status;
  }
  size_t start = select->next_index % select->count;
  for (size_t offset = 0; offset < select->count; offset++)
  {
    size_t i = (start + offset) % select->count;
    AuraTaskChannelValue value = {NULL, 0, NULL};
    AuraTaskChannelStatus status = aura_task_channel_receive(select->channels[i], NULL, &value);
    if (status == AURA_CHANNEL_OK)
    {
      select->next_index = (i + 1) % select->count;
      *out = value;
      *index = i;
      aura_task_channel_lock_leave();
      return status;
    }
    if (status == AURA_CHANNEL_CLOSED)
    {
      select->next_index = (i + 1) % select->count;
      *out = (AuraTaskChannelValue){NULL, 0, NULL};
      *index = i;
      aura_task_channel_lock_leave();
      return status;
    }
  }
  if (frame == NULL) { aura_task_channel_lock_leave(); return AURA_CHANNEL_PENDING; }
  select->frame = frame;
  frame->waiting_select = select;
  frame->waiting_node = select;
  for (size_t offset = 0; offset < select->count; offset++)
  {
    size_t i = (start + offset) % select->count;
    AuraTaskChannel *channel = select->channels[i];
    AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
    if (waiter == NULL)
    {
      aura_task_select_cancel_wait(frame);
      aura_task_channel_lock_leave();
      return AURA_CHANNEL_ERROR;
    }
    waiter->frame = frame;
    waiter->select = select;
    waiter->select_index = i;
    if (channel->receive_tail == NULL) channel->receive_head = waiter;
    else channel->receive_tail->next = waiter;
    channel->receive_tail = waiter;
  }
  aura_task_channel_lock_leave();
  return AURA_CHANNEL_PENDING;
}

size_t aura_task_select_selected_index(const AuraTaskSelect *select)
{
  return select != NULL ? select->selected_index : 0;
}

void aura_task_select_destroy(AuraTaskSelect *select)
{
  if (select == NULL) return;
  aura_task_channel_lock_enter();
  aura_task_select_unlink_waiters(select);
  free(select->channels);
  aura_task_channel_value_destroy(&select->value);
  aura_task_channel_lock_leave();
  free(select);
}

AuraTaskChannel *aura_task_channel_new(size_t capacity)
{
  if (capacity == 0)
  {
    return NULL;
  }
  AuraTaskChannel *channel = (AuraTaskChannel *)calloc(1, sizeof(*channel));
  if (channel == NULL)
  {
    return NULL;
  }
  channel->values = (AuraTaskChannelValue *)calloc(capacity, sizeof(*channel->values));
  if (channel->values == NULL)
  {
    free(channel);
    return NULL;
  }
  channel->capacity = capacity;
  return channel;
}

size_t aura_task_channel_capacity(const AuraTaskChannel *channel)
{
  return channel != NULL ? channel->capacity : 0;
}

size_t aura_task_channel_count(const AuraTaskChannel *channel)
{
  size_t count = 0;
  if (channel != NULL)
  {
    aura_task_channel_lock_enter();
    count = channel->count;
    aura_task_channel_lock_leave();
  }
  return count;
}

int aura_task_channel_is_closed(const AuraTaskChannel *channel)
{
  int closed = 0;
  if (channel != NULL)
  {
    aura_task_channel_lock_enter();
    closed = channel->closed;
    aura_task_channel_lock_leave();
  }
  return closed;
}

AuraTaskChannelStatus aura_task_channel_send(AuraTaskChannel *channel,
                                              AuraTaskFrame *sender,
                                              AuraTaskChannelValue value)
{
  if (channel == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  aura_task_channel_lock_enter();
  aura_task_channel_record(channel, sender, AURA_RACE_CHANNEL_SEND);
  if (channel->closed)
  {
    aura_task_channel_value_destroy(&value);
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_CLOSED;
  }
  if (channel->receive_head != NULL)
  {
    AuraTaskChannelWaiter *receiver = channel->receive_head;
    aura_task_channel_unlink(channel, receiver, 1);
    if (receiver->select != NULL)
    {
      AuraTaskSelect *select = receiver->select;
      size_t index = receiver->select_index;
      free(receiver);
      aura_task_select_finish(select, AURA_CHANNEL_OK, index, value);
      aura_task_channel_lock_leave();
      return AURA_CHANNEL_OK;
    }
    *receiver->out = value;
    AuraTaskFrame *receiver_frame = receiver->frame;
    receiver_frame->waiting_channel = NULL;
    receiver_frame->waiting_node = NULL;
    free(receiver);
    aura_task_channel_wake(receiver_frame);
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_OK;
  }
  if (channel->count < channel->capacity)
  {
    channel->values[channel->tail] = value;
    channel->tail = (channel->tail + 1) % channel->capacity;
    channel->count++;
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_OK;
  }
  if (sender == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_PENDING;
  }
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
  if (waiter == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_ERROR;
  }
  waiter->frame = sender;
  waiter->value = value;
  if (channel->send_tail == NULL)
    channel->send_head = waiter;
  else
    channel->send_tail->next = waiter;
  channel->send_tail = waiter;
  sender->waiting_channel = channel;
  sender->waiting_node = waiter;
  aura_task_channel_lock_leave();
  return AURA_CHANNEL_PENDING;
}

AuraTaskChannelStatus aura_task_channel_receive(AuraTaskChannel *channel,
                                                 AuraTaskFrame *receiver,
                                                 AuraTaskChannelValue *out)
{
  if (channel == NULL || out == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  aura_task_channel_lock_enter();
  aura_task_channel_record(channel, receiver, AURA_RACE_CHANNEL_RECEIVE);
  if (channel->count != 0)
  {
    *out = channel->values[channel->head];
    channel->values[channel->head] = (AuraTaskChannelValue){NULL, 0, NULL};
    channel->head = (channel->head + 1) % channel->capacity;
    channel->count--;
    if (channel->send_head != NULL)
    {
      AuraTaskChannelWaiter *sender = channel->send_head;
      aura_task_channel_unlink(channel, sender, 0);
      channel->values[channel->tail] = sender->value;
      channel->tail = (channel->tail + 1) % channel->capacity;
      channel->count++;
      AuraTaskFrame *sender_frame = sender->frame;
      sender_frame->waiting_channel = NULL;
      sender_frame->waiting_node = NULL;
      free(sender);
    aura_task_channel_wake(sender_frame);
    }
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_OK;
  }
  if (channel->closed)
  {
    *out = (AuraTaskChannelValue){NULL, 0, NULL};
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_CLOSED;
  }
  if (receiver == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_PENDING;
  }
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
  if (waiter == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_ERROR;
  }
  waiter->frame = receiver;
  waiter->out = out;
  if (channel->receive_tail == NULL)
    channel->receive_head = waiter;
  else
    channel->receive_tail->next = waiter;
  channel->receive_tail = waiter;
  receiver->waiting_channel = channel;
  receiver->waiting_node = waiter;
  aura_task_channel_lock_leave();
  return AURA_CHANNEL_PENDING;
}

int aura_task_channel_close_from(AuraTaskChannel *channel, AuraTaskFrame *closer)
{
  if (channel == NULL)
  {
    return 0;
  }
  aura_task_channel_lock_enter();
  if (channel->closed)
  {
    aura_task_channel_lock_leave();
    return 0;
  }
  aura_task_channel_record(channel, closer, AURA_RACE_CHANNEL_CLOSE);
  channel->closed = 1;
  while (channel->send_head != NULL)
  {
    AuraTaskChannelWaiter *waiter = channel->send_head;
    aura_task_channel_unlink(channel, waiter, 0);
    aura_task_channel_value_destroy(&waiter->value);
    waiter->frame->waiting_channel = NULL;
    waiter->frame->waiting_node = NULL;
    AuraTaskFrame *frame = waiter->frame;
    free(waiter);
    aura_task_channel_wake(frame);
  }
  while (channel->receive_head != NULL)
  {
    AuraTaskChannelWaiter *waiter = channel->receive_head;
    aura_task_channel_unlink(channel, waiter, 1);
    AuraTaskFrame *frame = waiter->frame;
    if (waiter->select != NULL)
    {
      AuraTaskSelect *select = waiter->select;
      size_t index = waiter->select_index;
      free(waiter);
      aura_task_select_finish(select, AURA_CHANNEL_CLOSED, index,
                              (AuraTaskChannelValue){NULL, 0, NULL});
    }
    else
    {
      frame->waiting_channel = NULL;
      frame->waiting_node = NULL;
      free(waiter);
      aura_task_channel_wake(frame);
    }
  }
  aura_task_channel_lock_leave();
  return 1;
}

int aura_task_channel_close(AuraTaskChannel *channel)
{
  return aura_task_channel_close_from(channel, NULL);
}

void aura_task_channel_destroy(AuraTaskChannel *channel)
{
  if (channel == NULL)
  {
    return;
  }
  (void)aura_task_channel_close(channel);
  aura_task_channel_lock_enter();
  while (channel->count != 0)
  {
    aura_task_channel_value_destroy(&channel->values[channel->head]);
    channel->head = (channel->head + 1) % channel->capacity;
    channel->count--;
  }
  free(channel->values);
  aura_task_channel_lock_leave();
  free(channel);
}

/* ---- Process argv (std.io.args) ----
 * Stashed from C main before aura_main so user programs keep fun main().
 * Each returned string is an owned copy because Array<String> frees its
 * elements when the array is dropped.
 */

static int aura_saved_argc = 0;
static char **aura_saved_argv = NULL;

void aura_set_args(int argc, char **argv)
{
  aura_saved_argc = argc > 0 ? argc : 0;
  aura_saved_argv = argv;
}

int64_t aura_args_count(void)
{
  return (int64_t)aura_saved_argc;
}

const char *aura_args_get(int64_t i)
{
  if (i < 0 || i >= (int64_t)aura_saved_argc || aura_saved_argv == NULL)
  {
    aura_throw_string("args index out of bounds");
  }
  const char *s = aura_saved_argv[i] != NULL ? aura_saved_argv[i] : "";
  size_t n = strlen(s);
  char *copy = (char *)malloc(n + 1);
  if (copy == NULL)
  {
    aura_throw_string("args allocation failed");
  }
  memcpy(copy, s, n + 1);
  return copy;
}

/* Bounded CLI companion used by examples/http-health-cli.  The public Aura
 * surface intentionally exposes this as one primitive smoke operation rather
 * than leaking opaque HTTP handles into the alpha stdlib. */
static AuraHttpHandlerResult aura_http_health_cli_handler(
    const AuraHttpRequest *request, AuraHttpResponse *response, void *user_data)
{
  (void)user_data;
  if (request == NULL || strcmp(request->method, "GET") != 0 ||
      strcmp(request->target, "/health") != 0 ||
      aura_http_response_set_body(response, "ok\n", 3) != AURA_HTTP_RESPONSE_OK)
  {
    return AURA_HTTP_HANDLER_ERROR;
  }
  return AURA_HTTP_HANDLER_CLOSE;
}

typedef struct
{
  AuraHttpConnection *connection;
} AuraHttpHealthCliTask;

static AuraTaskPollState aura_http_health_cli_poll(AuraTaskFrame *frame)
{
  AuraHttpHealthCliTask *task =
      (AuraHttpHealthCliTask *)aura_task_frame_data(frame);
  return aura_http_connection_poll_async(
      frame, task->connection, aura_http_health_cli_handler, NULL);
}

int64_t aura_http_health_smoke(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpListener *listener = NULL;
  AuraTcpStream *client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  uint16_t port = 0;
  char response[512] = {0};
  size_t used = 0;
  int64_t status = 1;

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  if (aura_tcp_listener_bind(0, &port, &listener) != AURA_TCP_OK ||
      aura_http_server_create(listener, 1, &config, &server) !=
          AURA_HTTP_CONNECTION_OK ||
      aura_tcp_stream_connect(port, 1000, &client) != AURA_TCP_OK ||
      aura_http_server_accept(server, 1000, &connection) !=
          AURA_HTTP_CONNECTION_OK)
  {
    goto cleanup;
  }
  executor = aura_task_executor_new();
  if (executor == NULL)
  {
    goto cleanup;
  }
  frame = aura_task_frame_new(sizeof(AuraHttpHealthCliTask),
                               aura_http_health_cli_poll, NULL);
  if (frame == NULL)
  {
    goto cleanup;
  }
  ((AuraHttpHealthCliTask *)aura_task_frame_data(frame))->connection = connection;
  if (aura_task_executor_submit(executor, frame) != 1 ||
      aura_task_executor_run_one(executor) != 1 ||
      aura_task_frame_state(frame) != AURA_TASK_PENDING)
  {
    goto cleanup;
  }
  {
    const char request[] = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
    size_t written = 0;
    if (aura_tcp_stream_write(client, request, sizeof(request) - 1,
                              &written, 1000) != AURA_TCP_OK ||
        written != sizeof(request) - 1 ||
        aura_task_executor_poll_waiting(executor, 1000) != 1 ||
        aura_task_executor_run_one(executor) != 1 ||
        aura_task_frame_state(frame) != AURA_TASK_COMPLETE)
    {
      goto cleanup;
    }
  }
  while (used + 1 < sizeof(response))
  {
    size_t received = 0;
    AuraTcpStatus read_status = aura_tcp_stream_read(
        client, response + used, sizeof(response) - used - 1, &received, 1000);
    if (read_status != AURA_TCP_OK || received == 0)
    {
      break;
    }
    used += received;
    response[used] = '\0';
    if (strstr(response, "HTTP/1.1 200 OK") != NULL &&
        strstr(response, "\r\n\r\nok\n") != NULL)
    {
      status = 0;
      break;
    }
  }

cleanup:
  if (frame != NULL)
  {
    if (aura_task_frame_state(frame) == AURA_TASK_COMPLETE ||
        aura_task_frame_state(frame) == AURA_TASK_FAILED ||
        aura_task_frame_state(frame) == AURA_TASK_CANCELLED)
    {
      (void)aura_task_executor_release(executor, &frame);
    }
  }
  if (executor != NULL)
  {
    aura_task_executor_shutdown(executor);
  }
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  if (server != NULL)
  {
    (void)aura_http_server_shutdown(server);
    (void)aura_http_server_destroy(server);
  }
  return status;
}

/* ---- Process exit (std.io.exit) ----
 * Flush stdio, then terminate with the given status (truncated to int).
 * Does not return. Prefer exit over _Exit so atexit/flush run.
 */

void aura_exit(int64_t code)
{
  fflush(stdout);
  fflush(stderr);
  exit((int)code);
}

/* Provided by generated code */
int aura_main(void);

#ifndef AURA_RUNTIME_NO_MAIN
int main(int argc, char **argv)
{
  aura_set_args(argc, argv);
  int rc = aura_main();
  aura_gc_shutdown();
  return rc;
}
#endif
