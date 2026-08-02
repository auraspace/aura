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
#include "../aura_ffi.h"

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
typedef void *(*AuraTypeErasedCloneFn)(const void *data, size_t size,
                                       size_t *cloned_size);
typedef void (*AuraTypeErasedDropFn)(void *data, size_t size);
typedef void (*AuraTypeErasedMarkFn)(const void *data, size_t size);
typedef struct AuraTypeErasedOps
{
  uint32_t abi_version;
  AuraTypeErasedCloneFn clone;
  AuraTypeErasedDropFn drop;
  AuraTypeErasedMarkFn mark;
} AuraTypeErasedOps;
typedef struct AuraTypeErasedValue
{
  void *data;
  size_t size;
  const AuraTypeErasedOps *ops;
} AuraTypeErasedValue;
#define AURA_TYPE_ERASED_ABI_VERSION 1u
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
typedef void *(*AuraFfiPayloadCloneFn)(const void *payload, uint64_t payload_len,
                                       uint64_t *cloned_len);
typedef void (*AuraFfiPayloadDestroyFn)(void *payload, uint64_t payload_len);
typedef struct AuraFfiOwnedPayload
{
  void *data;
  uint64_t len;
  AuraFfiPayloadDestroyFn destroy;
} AuraFfiOwnedPayload;

#ifndef AURA_FFI_MAX_OWNED_CALLBACK_PAYLOAD
#define AURA_FFI_MAX_OWNED_CALLBACK_PAYLOAD \
  (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))
#endif
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
