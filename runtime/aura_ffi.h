#ifndef AURA_FFI_H
#define AURA_FFI_H

/*
 * Stable, allocation-only C ABI for the bounded F3 slice.
 *
 * These are deliberately separate from Aura's internal String and Array
 * layouts.  A foreign function may borrow a view for the duration of a call,
 * copy a view into an Aura-owned value, or transfer a malloc-compatible buffer
 * exactly once.  No callback, arbitrary element destructor, or raw pointer
 * dereference is part of this ABI.  The callback surface below is a separate,
 * synchronous F5 contract and never borrows an environment past deregistration.
 */
#include <stddef.h>
#include <stdint.h>

#ifndef AURA_FILE_H
#define AURA_FILE_H
typedef struct AuraFile AuraFile;

typedef enum AuraFileStatus {
  AURA_FILE_OK = 0,
  AURA_FILE_PENDING = 1,
  AURA_FILE_EOF = 2,
  AURA_FILE_ERROR = -1,
  AURA_FILE_CLOSED = -2,
  AURA_FILE_UNSUPPORTED = -3,
  AURA_FILE_PERMISSION = -4
} AuraFileStatus;

typedef enum AuraFileMode {
  AURA_FILE_READ = 0,
  AURA_FILE_WRITE = 1,
  AURA_FILE_READ_WRITE = 2,
  AURA_FILE_APPEND = 3
} AuraFileMode;

/* Bounded file operations. Buffers are borrowed for one call only. A
 * successful open owns the descriptor until close/destroy; close is safe to
 * repeat. On POSIX regular files these calls perform one bounded syscall and
 * never suspend an Aura task. */
AuraFileStatus aura_file_open(const char *path, AuraFileMode mode,
                              AuraFile **out);
AuraFileStatus aura_file_read(AuraFile *file, void *buffer, uint64_t capacity,
                              uint64_t *out_read);
AuraFileStatus aura_file_write(AuraFile *file, const void *buffer,
                               uint64_t length, uint64_t *out_written);
AuraFileStatus aura_file_flush(AuraFile *file);
AuraFileStatus aura_file_close(AuraFile *file);
AuraFileStatus aura_file_destroy(AuraFile **file);
const char *aura_file_last_error(void);
#endif

/* Bounded std.net transport ABI.  Handles are opaque and own their socket
 * until close/destroy.  The current Aura FFI primitive contract cannot pass
 * these handles (only Int, Bool, String, and Unit are legal), so these
 * declarations are for native integrations and the focused primitive bridge
 * in std/net/native.  A future typed-handle binding must preserve this
 * ownership rule and reject handles across task/await/callback boundaries. */
#if defined(AURA_FFI_DECLARE_NET) && !defined(AURA_NET_H)
#define AURA_NET_H
typedef struct AuraTcpListener AuraTcpListener;
typedef struct AuraTcpStream AuraTcpStream;

typedef enum AuraTcpStatus {
  AURA_TCP_OK = 0,
  AURA_TCP_PENDING = 1,
  AURA_TCP_EOF = 2,
  AURA_TCP_TIMEOUT = 3,
  AURA_TCP_ERROR = -1,
  AURA_TCP_CLOSED = -2,
  AURA_TCP_UNSUPPORTED = -3
} AuraTcpStatus;

AuraTcpStatus aura_tcp_listener_bind(uint16_t port, uint16_t *out_port,
                                     AuraTcpListener **out_listener);
AuraTcpStatus aura_tcp_listener_accept(AuraTcpListener *listener,
                                       int timeout_ms,
                                       AuraTcpStream **out_stream);
AuraTcpStatus aura_tcp_stream_connect(uint16_t port, int timeout_ms,
                                      AuraTcpStream **out_stream);
AuraTcpStatus aura_tcp_stream_read(AuraTcpStream *stream, void *buffer,
                                   size_t capacity, size_t *out_bytes,
                                   int timeout_ms);
AuraTcpStatus aura_tcp_stream_write(AuraTcpStream *stream, const void *buffer,
                                    size_t capacity, size_t *out_bytes,
                                    int timeout_ms);
int aura_tcp_listener_close(AuraTcpListener *listener);
void aura_tcp_listener_destroy(AuraTcpListener *listener);
int aura_tcp_stream_close(AuraTcpStream *stream);
void aura_tcp_stream_destroy(AuraTcpStream *stream);
const char *aura_tcp_last_error(void);
#endif

#define AURA_FFI_ABI_VERSION 1u

typedef enum AuraFfiStatus {
  AURA_FFI_OK = 0,
  AURA_FFI_INVALID = 1,
  AURA_FFI_OOM = 2
} AuraFfiStatus;

typedef struct AuraFfiStringView {
  const char *data;
  uint64_t len;
} AuraFfiStringView;

typedef struct AuraFfiString {
  char *data;
  uint64_t len;
} AuraFfiString;

typedef enum AuraFfiArrayKind {
  AURA_FFI_ARRAY_BYTES = 1,
  AURA_FFI_ARRAY_INT64 = 2,
  AURA_FFI_ARRAY_BOOL = 3
} AuraFfiArrayKind;

typedef struct AuraFfiArrayView {
  const void *data;
  uint64_t len;
  uint64_t cap;
  uint64_t elem_size;
  AuraFfiArrayKind kind;
} AuraFfiArrayView;

typedef struct AuraFfiArray {
  void *data;
  uint64_t len;
  uint64_t cap;
  uint64_t elem_size;
  AuraFfiArrayKind kind;
} AuraFfiArray;

/* Borrow: no allocation and no destruction.  `data` is valid only while the
 * foreign call retains the caller's value. */
AuraFfiStatus aura_ffi_string_borrow(const char *data, uint64_t len,
                                     AuraFfiStringView *out);
AuraFfiStatus aura_ffi_array_borrow(const void *data, uint64_t len,
                                    uint64_t cap, uint64_t elem_size,
                                    AuraFfiArrayKind kind,
                                    AuraFfiArrayView *out);

/* Copy: allocates an independent value owned by the destination. */
AuraFfiStatus aura_ffi_string_copy(AuraFfiStringView view,
                                   AuraFfiString *out);
AuraFfiStatus aura_ffi_array_copy(AuraFfiArrayView view, AuraFfiArray *out);

/* Transfer: accepts only malloc-compatible storage and consumes it exactly
 * once.  On success the caller must no longer access `data`; on failure the
 * caller retains ownership and must release it. */
AuraFfiStatus aura_ffi_string_transfer(char *data, uint64_t len,
                                       AuraFfiString *out);
AuraFfiStatus aura_ffi_array_transfer(void *data, uint64_t len, uint64_t cap,
                                      uint64_t elem_size, AuraFfiArrayKind kind,
                                      AuraFfiArray *out);

/* Idempotent destruction of values created by copy/transfer. */
void aura_ffi_string_destroy(AuraFfiString *value);
void aura_ffi_array_destroy(AuraFfiArray *value);

/* Root a GC-managed slot across a synchronous foreign call.  The guard does
 * not extend lifetime across await, task, or callback boundaries. */
typedef struct AuraFfiRootGuard {
  void **slot;
  int active;
} AuraFfiRootGuard;

AuraFfiStatus aura_ffi_root_begin(AuraFfiRootGuard *guard, void **slot);
void aura_ffi_root_end(AuraFfiRootGuard *guard);

/* Mark a GC object reachable from a task frame mark callback. */
void aura_gc_mark_ptr(void *obj);

/* F4 opaque foreign-resource handles.  The resource pointer is never exposed
 * by the handle itself: foreign code must hold a live pin token and ask the
 * runtime to validate it before each operation.  A released handle remains a
 * tombstone until aura_ffi_handle_destroy, so stale aliases fail safely. */
typedef struct AuraFfiOpaqueHandle AuraFfiOpaqueHandle;
typedef void (*AuraFfiHandleDestroyFn)(void *resource);

typedef struct AuraFfiHandlePin {
  AuraFfiOpaqueHandle *handle;
  void *resource;
  uint64_t generation;
} AuraFfiHandlePin;

typedef enum AuraFfiBoundary {
  AURA_FFI_BOUNDARY_SYNC = 0,
  AURA_FFI_BOUNDARY_TASK = 1,
  AURA_FFI_BOUNDARY_AWAIT = 2,
  AURA_FFI_BOUNDARY_CHANNEL = 3,
  AURA_FFI_BOUNDARY_CALLBACK = 4
} AuraFfiBoundary;

#define AURA_FFI_BOUNDARY_REJECTED ((AuraFfiStatus)3)
#define AURA_FFI_BUSY ((AuraFfiStatus)4)

/* Typed scheduler-owned I/O operation handles.  An operation is opaque to
 * foreign callers and may be inspected synchronously while it is pending.
 * Starting it binds it to one executor frame; the frame owns the suspension
 * boundary and the operation must not be copied through await/channel/callback
 * crossings.  Completion is published by the executor's readiness poller;
 * cancellation remains idempotent and releases the resource at most once. */
typedef struct AuraTaskExecutor AuraTaskExecutor;
typedef struct AuraTaskFrame AuraTaskFrame;
typedef void (*AuraTaskFrameGcMarkFn)(AuraTaskFrame *frame);
typedef struct AuraTcpListener AuraTcpListener;
typedef struct AuraTcpStream AuraTcpStream;
typedef struct AuraIoOperationHandle AuraIoOperationHandle;
typedef void (*AuraIoOperationCleanupFn)(void *resource);

typedef enum AuraIoOperationKind {
  AURA_IO_OPERATION_FILE_READ = 1,
  AURA_IO_OPERATION_FILE_WRITE = 2,
  AURA_IO_OPERATION_TCP_ACCEPT = 3,
  AURA_IO_OPERATION_TCP_CONNECT = 4,
  AURA_IO_OPERATION_TCP_READ = 5,
  AURA_IO_OPERATION_TCP_WRITE = 6
} AuraIoOperationKind;

typedef enum AuraIoOperationState {
  AURA_IO_OPERATION_PENDING = 0,
  AURA_IO_OPERATION_COMPLETE = 1,
  AURA_IO_OPERATION_CANCELLED = 2,
  AURA_IO_OPERATION_FAILED = 3
} AuraIoOperationState;

typedef enum AuraIoOutcome {
  AURA_IO_OUTCOME_OK = 0,
  AURA_IO_OUTCOME_EOF = 1,
  AURA_IO_OUTCOME_CANCELLED = 2,
  AURA_IO_OUTCOME_CLOSED = 3,
  AURA_IO_OUTCOME_PERMISSION = 4,
  AURA_IO_OUTCOME_TIMEOUT = 5,
  AURA_IO_OUTCOME_UNSUPPORTED = 6,
  AURA_IO_OUTCOME_ERROR = 7
} AuraIoOutcome;

typedef struct AuraIoOperationResult {
  AuraIoOperationKind kind;
  AuraIoOperationState state;
  AuraIoOutcome outcome;
  uint64_t bytes_transferred;
  int32_t native_status;
} AuraIoOperationResult;

/* A suspended frame owns its opaque data, but the runtime cannot infer which
 * fields contain GC references.  The mark callback must call aura_gc_mark_ptr
 * for every GC object reachable from that frame's live state. */
void aura_task_frame_set_gc_mark(AuraTaskFrame *frame,
                                 AuraTaskFrameGcMarkFn mark);

AuraIoOperationHandle *aura_file_async_read_handle_new(
    AuraFile *file, AuraIoOperationCleanupFn cleanup);
AuraIoOperationHandle *aura_file_async_write_handle_new(
    AuraFile *file, AuraIoOperationCleanupFn cleanup);
AuraIoOperationHandle *aura_tcp_async_accept_handle_new(
    AuraTcpListener *listener, AuraIoOperationCleanupFn cleanup);
AuraIoOperationHandle *aura_tcp_async_read_handle_new(
    AuraTcpStream *stream, AuraIoOperationCleanupFn cleanup);
AuraIoOperationHandle *aura_tcp_async_write_handle_new(
    AuraTcpStream *stream, AuraIoOperationCleanupFn cleanup);
/* Typed operations borrow their buffer until the operation leaves PENDING.
 * Read/write completion performs one bounded native call and records a stable
 * result; callers do not need a second synchronous syscall after wakeup. */
AuraIoOperationHandle *aura_file_async_read_operation_new(
    AuraFile *file, void *buffer, uint64_t capacity,
    AuraIoOperationCleanupFn cleanup);
AuraIoOperationHandle *aura_file_async_write_operation_new(
    AuraFile *file, const void *buffer, uint64_t length,
    AuraIoOperationCleanupFn cleanup);
AuraIoOperationHandle *aura_tcp_async_read_operation_new(
    AuraTcpStream *stream, void *buffer, uint64_t capacity,
    AuraIoOperationCleanupFn cleanup);
AuraIoOperationHandle *aura_tcp_async_write_operation_new(
    AuraTcpStream *stream, const void *buffer, uint64_t length,
    AuraIoOperationCleanupFn cleanup);
int aura_io_operation_handle_start(AuraIoOperationHandle *operation,
                                   AuraTaskExecutor *executor,
                                   AuraTaskFrame *frame);
AuraIoOperationState aura_io_operation_handle_state(
    const AuraIoOperationHandle *operation);
AuraIoOperationKind aura_io_operation_handle_kind(
    const AuraIoOperationHandle *operation);
int aura_io_operation_handle_result(const AuraIoOperationHandle *operation,
                                    AuraIoOperationResult *out);
int aura_io_operation_handle_complete(AuraIoOperationHandle *operation,
                                      int success);
int aura_io_operation_handle_cancel(AuraIoOperationHandle *operation);
int aura_io_operation_handle_release(AuraIoOperationHandle **handle);
AuraFfiStatus aura_io_operation_handle_check_boundary(
    const AuraIoOperationHandle *operation, AuraFfiBoundary boundary);

/* Non-null and nullable construction are intentionally separate operations. */
AuraFfiStatus aura_ffi_handle_new(void *resource,
                                  AuraFfiHandleDestroyFn destroy,
                                  AuraFfiOpaqueHandle **out);
AuraFfiStatus aura_ffi_handle_new_nullable(void *resource,
                                            AuraFfiHandleDestroyFn destroy,
                                            AuraFfiOpaqueHandle **out);
int aura_ffi_handle_is_null(const AuraFfiOpaqueHandle *handle);

/* Pinning grants a checked, synchronous operation window. */
AuraFfiStatus aura_ffi_handle_pin(AuraFfiOpaqueHandle *handle,
                                  AuraFfiHandlePin *out);
/* Pin a handle for a specific ownership boundary.  SYNC, TASK, and AWAIT
 * pins are valid while the caller retains the token; CHANNEL and CALLBACK
 * crossings remain rejected until those ownership contracts are defined. */
AuraFfiStatus aura_ffi_handle_pin_for_boundary(AuraFfiOpaqueHandle *handle,
                                               AuraFfiBoundary boundary,
                                               AuraFfiHandlePin *out);
AuraFfiStatus aura_ffi_handle_pin_resource(const AuraFfiHandlePin *pin,
                                           void **out_resource);
AuraFfiStatus aura_ffi_handle_unpin(AuraFfiHandlePin *pin);

/* Release invalidates the resource immediately and invokes its destructor at
 * most once (deferred until all pins are unpinned).  Invalidation is the same
 * operation for runtimes that observe an external resource death. */
AuraFfiStatus aura_ffi_handle_release(AuraFfiOpaqueHandle *handle);
AuraFfiStatus aura_ffi_handle_invalidate(AuraFfiOpaqueHandle *handle);
AuraFfiStatus aura_ffi_handle_destroy(AuraFfiOpaqueHandle **handle);

/* Direct unpinned pointer use is synchronous-only.  Use
 * aura_ffi_handle_pin_for_boundary for a checked TASK or AWAIT transfer. */
AuraFfiStatus aura_ffi_handle_check_boundary(const AuraFfiOpaqueHandle *handle,
                                             AuraFfiBoundary boundary);

/* F5 bounded callback ABI.  A registration owns `environment` and invokes its
 * destructor exactly once, at deregistration or shutdown.  A callback is
 * synchronous, single-thread-affine, and may not cross task/await/channel
 * boundaries.  The frame is retained by the registration, so destroying the
 * caller's frame while registered is rejected rather than leaving a dangling
 * callback target. */
typedef struct AuraFfiCallbackFrame AuraFfiCallbackFrame;
typedef struct AuraFfiCallback AuraFfiCallback;
typedef int32_t (*AuraFfiCallbackFn)(void *environment, const void *payload,
                                     uint64_t payload_len);
typedef void (*AuraFfiCallbackEnvDestroyFn)(void *environment);

typedef enum AuraFfiOutcome {
  AURA_FFI_OUTCOME_OK = 0,
  AURA_FFI_OUTCOME_CANCELLED = 1,
  AURA_FFI_OUTCOME_INVALID = 2,
  AURA_FFI_OUTCOME_NOT_FOUND = 3,
  AURA_FFI_OUTCOME_PERMISSION = 4,
  AURA_FFI_OUTCOME_UNAVAILABLE = 5,
  AURA_FFI_OUTCOME_TIMEOUT = 6,
  AURA_FFI_OUTCOME_FOREIGN_ERROR = 7
} AuraFfiOutcome;

/* Foreign callbacks return these bounded error codes; unknown values map to
 * AURA_FFI_OUTCOME_FOREIGN_ERROR and are never treated as success. */
AuraFfiOutcome aura_ffi_map_error(int32_t foreign_code);

AuraFfiStatus aura_ffi_callback_frame_new(uint64_t owner_task,
                                          AuraFfiCallbackFrame **out);
AuraFfiStatus aura_ffi_callback_frame_invalidate(AuraFfiCallbackFrame *frame);
AuraFfiStatus aura_ffi_callback_frame_destroy(AuraFfiCallbackFrame **frame);

AuraFfiStatus aura_ffi_callback_register(
    AuraFfiCallbackFrame *frame, AuraFfiCallbackFn callback, void *environment,
    AuraFfiCallbackEnvDestroyFn environment_destroy, AuraFfiCallback **out);
AuraFfiStatus aura_ffi_callback_invoke(AuraFfiCallback *callback,
                                       uint64_t current_task,
                                       AuraFfiBoundary boundary,
                                       const void *payload,
                                       uint64_t payload_len,
                                       AuraFfiOutcome *outcome);
AuraFfiStatus aura_ffi_callback_deregister(AuraFfiCallback *callback);
AuraFfiStatus aura_ffi_callback_shutdown(AuraFfiCallback *callback);
AuraFfiStatus aura_ffi_callback_destroy(AuraFfiCallback **callback);

#endif
