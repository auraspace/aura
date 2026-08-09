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

/* Forward declarations shared by the TLS ABI and the optional net ABI. */
typedef struct AuraTcpStream AuraTcpStream;
typedef struct AuraFfiOpaqueHandle AuraFfiOpaqueHandle;

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

/* Stable scheduler/task/channel ABI.  A task handle is an executor-owned
 * reference; a channel value transfers exactly one retained payload and its
 * destructor. These declarations make TaskHandle<T> and Channel<T> crossings
 * explicit for foreign callers instead of requiring internal runtime symbols. */
typedef struct AuraTaskExecutor AuraTaskExecutor;
typedef struct AuraTaskFrame AuraTaskFrame;
typedef struct AuraTaskChannel AuraTaskChannel;
#ifndef AURA_TASK_POLL_STATE_DEFINED
#define AURA_TASK_POLL_STATE_DEFINED 1
typedef enum AuraTaskPollState {
  AURA_TASK_READY = 0,
  AURA_TASK_PENDING = 1,
  AURA_TASK_COMPLETE = 2,
  AURA_TASK_FAILED = 3,
  AURA_TASK_CANCELLED = 4
} AuraTaskPollState;
#endif
typedef AuraTaskPollState (*AuraTaskPollFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskFrameDestroyFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskChannelValueDestroyFn)(void *data, size_t size);
typedef struct {
  void *data;
  size_t size;
  AuraTaskChannelValueDestroyFn destroy;
} AuraTaskChannelValue;
typedef enum AuraTaskChannelStatus {
  AURA_CHANNEL_OK = 0,
  AURA_CHANNEL_PENDING = 1,
  AURA_CHANNEL_CLOSED = 2,
  AURA_CHANNEL_ERROR = 3
} AuraTaskChannelStatus;
#define AURA_TASK_CHANNEL_ABI_DEFINED 1

AuraTaskExecutor *aura_task_executor_new(void);
AuraTaskFrame *aura_task_frame_new(size_t data_size, AuraTaskPollFn poll,
                                   AuraTaskFrameDestroyFn destroy);
void *aura_task_frame_data(AuraTaskFrame *frame);
AuraTaskPollState aura_task_poll_unit(AuraTaskFrame *frame);
int aura_task_executor_submit(AuraTaskExecutor *executor, AuraTaskFrame *frame);
size_t aura_task_executor_run(AuraTaskExecutor *executor);
int aura_task_executor_release(AuraTaskExecutor *executor,
                               AuraTaskFrame **handle);
void aura_task_executor_shutdown(AuraTaskExecutor *executor);
int aura_llvm_task_join_i64(AuraTaskExecutor *executor, AuraTaskFrame *frame,
                            int64_t *out);
int aura_llvm_task_join_ptr(AuraTaskExecutor *executor, AuraTaskFrame *frame,
                            void **out);
int aura_llvm_task_join_unit(AuraTaskExecutor *executor, AuraTaskFrame *frame);
int aura_llvm_task_join_status(AuraTaskExecutor *executor, AuraTaskFrame *frame);
void aura_llvm_task_raise_failure(AuraTaskFrame *frame);
void *aura_llvm_lazy_int_new(void *environment, void *function);
int64_t aura_llvm_lazy_int_get(void *value);
int aura_llvm_lazy_is_initialized(void *value);
void aura_llvm_lazy_int_destroy(void *value);
int64_t aura_llvm_sync_load(int64_t *value);
void aura_llvm_sync_store(int64_t *value, int64_t next);
int64_t aura_llvm_sync_fetch_add(int64_t *value, int64_t amount);
int aura_llvm_sync_compare_exchange(int64_t *value, int64_t expected, int64_t desired);
int aura_llvm_sync_try_lock(int64_t *value);
void aura_llvm_sync_unlock(int64_t *value);
int aura_llvm_sync_is_locked(int64_t *value);
int aura_llvm_sync_try_read(int64_t *value);
int aura_llvm_sync_try_write(int64_t *value);
void aura_llvm_sync_unlock_read(int64_t *value);
void aura_llvm_sync_unlock_write(int64_t *value);
int64_t aura_llvm_sync_reader_count(int64_t *value);
int aura_llvm_sync_is_write_locked(int64_t *value);
int aura_llvm_task_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame);
int aura_llvm_task_release(AuraTaskExecutor *executor, AuraTaskFrame *frame);

int aura_task_executor_retain_payload(AuraTaskExecutor *executor,
                                       AuraTaskFrame *frame);
int aura_task_executor_release_payload(AuraTaskExecutor *executor,
                                       AuraTaskFrame **payload);
AuraTaskChannel *aura_task_channel_new(size_t capacity);
int aura_task_channel_retain(AuraTaskChannel *channel);
AuraTaskChannelStatus aura_task_channel_send(AuraTaskChannel *channel,
                                             AuraTaskFrame *sender,
                                             AuraTaskChannelValue value);
AuraTaskChannelStatus aura_task_channel_receive(AuraTaskChannel *channel,
                                                AuraTaskFrame *receiver,
                                                AuraTaskChannelValue *out);
int aura_task_channel_close(AuraTaskChannel *channel);
void aura_task_channel_destroy(AuraTaskChannel *channel);
AuraTaskChannelValue aura_task_channel_value_from_task(AuraTaskExecutor *executor,
                                                        AuraTaskFrame *frame);
AuraTaskFrame *aura_task_channel_value_take_task(void *data, size_t size);
AuraTaskChannelValue aura_task_channel_value_from_channel(AuraTaskChannel *channel);
AuraTaskChannel *aura_task_channel_value_take_channel(void *data, size_t size);
void aura_task_channel_value_destroy_free(void *data, size_t size);
void aura_task_channel_value_destroy_task(void *data, size_t size);
void aura_task_channel_value_destroy_channel(void *data, size_t size);

/* Bounded std.net transport ABI.  Handles are opaque and own their socket
 * until close/destroy.  The current Aura FFI primitive contract cannot pass
 * these handles (only Int, Bool, String, and Unit are legal), so these
 * declarations are for native integrations and the focused primitive bridge
 * in std/net/native.  A future typed-handle binding must preserve this
 * ownership rule and reject handles across task/await/callback boundaries. */
#if defined(AURA_FFI_DECLARE_NET) && !defined(AURA_NET_H)
#define AURA_NET_H
typedef struct AuraTcpListener AuraTcpListener;

typedef enum AuraTcpStatus {
  AURA_TCP_OK = 0,
  AURA_TCP_PENDING = 1,
  AURA_TCP_EOF = 2,
  AURA_TCP_TIMEOUT = 3,
  AURA_TCP_PARTIAL_EOF = 4,
  AURA_TCP_ERROR = -1,
  AURA_TCP_CLOSED = -2,
  AURA_TCP_UNSUPPORTED = -3
} AuraTcpStatus;

AuraTcpStatus aura_tcp_listener_bind(uint16_t port, uint16_t *out_port,
                                     AuraTcpListener **out_listener);
AuraTcpStatus aura_tcp_listener_bind_endpoint(const char *endpoint,
                                              uint16_t *out_port,
                                              AuraTcpListener **out_listener);
AuraTcpStatus aura_tcp_listener_accept(AuraTcpListener *listener,
                                       int timeout_ms,
                                       AuraTcpStream **out_stream);
AuraTcpStatus aura_tcp_stream_connect(uint16_t port, int timeout_ms,
                                      AuraTcpStream **out_stream);
AuraTcpStatus aura_tcp_stream_connect_endpoint(const char *endpoint,
                                               int timeout_ms,
                                               AuraTcpStream **out_stream);
AuraTcpStatus aura_tcp_stream_read(AuraTcpStream *stream, void *buffer,
                                   size_t capacity, size_t *out_bytes,
                                   int timeout_ms);
AuraTcpStatus aura_tcp_stream_write(AuraTcpStream *stream, const void *buffer,
                                    size_t capacity, size_t *out_bytes,
                                    int timeout_ms);
AuraTcpStatus aura_tcp_stream_read_exactly(AuraTcpStream *stream, void *buffer,
                                           size_t length, size_t *out_bytes,
                                           int timeout_ms);
AuraTcpStatus aura_tcp_stream_write_all(AuraTcpStream *stream, const void *buffer,
                                        size_t length, size_t *out_bytes,
                                        int timeout_ms);
int aura_tcp_listener_close(AuraTcpListener *listener);
void aura_tcp_listener_destroy(AuraTcpListener *listener);
int aura_tcp_stream_close(AuraTcpStream *stream);
void aura_tcp_stream_destroy(AuraTcpStream *stream);
const char *aura_tcp_last_error(void);
#endif

#define AURA_FFI_ABI_VERSION 1u

/* Binary protocol helpers. These APIs carry explicit lengths and never
 * interpret payload bytes as UTF-8 or NUL-terminated strings. */
typedef struct AuraByteBuffer AuraByteBuffer;

typedef enum AuraByteBufferStatus {
  AURA_BYTE_BUFFER_OK = 0,
  AURA_BYTE_BUFFER_EOF = 1,
  AURA_BYTE_BUFFER_PARTIAL_EOF = 2,
  AURA_BYTE_BUFFER_TIMEOUT = 3,
  AURA_BYTE_BUFFER_CLOSED = 4,
  AURA_BYTE_BUFFER_ERROR = -1,
  AURA_BYTE_BUFFER_INVALID = -2,
  AURA_BYTE_BUFFER_OOM = -3
} AuraByteBufferStatus;

uint8_t aura_byte_from_u8(uint64_t value, int *valid);
AuraByteBuffer *aura_byte_buffer_new(void);
AuraByteBuffer *aura_byte_buffer_from_bytes(const void *data, size_t length);
void aura_byte_buffer_destroy(AuraByteBuffer *buffer);
size_t aura_byte_buffer_length(const AuraByteBuffer *buffer);
AuraByteBufferStatus aura_byte_buffer_append_byte(AuraByteBuffer *buffer,
                                                  uint8_t value);
AuraByteBufferStatus aura_byte_buffer_read_byte(const AuraByteBuffer *buffer,
                                                size_t index, uint8_t *out);
AuraByteBuffer *aura_byte_buffer_slice(const AuraByteBuffer *buffer,
                                       size_t start, size_t length);
AuraByteBuffer *aura_byte_buffer_concat(const AuraByteBuffer *left,
                                         const AuraByteBuffer *right);
const uint8_t *aura_byte_buffer_data(const AuraByteBuffer *buffer);

uint16_t aura_read_int16_be(const uint8_t *data);
uint32_t aura_read_int32_be(const uint8_t *data);
void aura_write_int16_be(uint8_t *data, uint16_t value);
void aura_write_int32_be(uint8_t *data, uint32_t value);

/* TLS stream adapters. A wrapped stream retains `owner` until close. */
int aura_tls_wrap_stream(const char *endpoint, AuraTcpStream *stream,
                         AuraFfiOpaqueHandle *owner, const char *server_name,
                         int verify_peer);
AuraTcpStream *aura_tls_stream(const char *endpoint);
short aura_tls_pending_events(const char *endpoint);

/* Length-aware crypto primitives. Output buffers are caller-owned. */
int aura_crypto_random_bytes_raw(void *output, size_t length);
int aura_crypto_sha256_bytes(const void *input, size_t length,
                             uint8_t output[32]);
int aura_crypto_md5_bytes(const void *input, size_t length,
                          uint8_t output[16]);
int aura_crypto_hmac_sha256_bytes(const void *key, size_t key_length,
                                  const void *input, size_t input_length,
                                  uint8_t output[32]);
int aura_crypto_pbkdf2_sha256(const void *password, size_t password_length,
                              const void *salt, size_t salt_length,
                              uint32_t iterations, void *output,
                              size_t output_length);

int aura_tls_read_bytes(const char *endpoint, void *output, size_t capacity,
                        size_t *out_bytes, int timeout_ms);
int aura_tls_write_bytes(const char *endpoint, const void *input, size_t length,
                         size_t *out_bytes, int timeout_ms);

/* Bounded std.dns numeric address selection. */
const char *aura_dns_resolve_host(const char *host, int prefer_ipv6);
const char *aura_dns_resolve_host_list(const char *host, int prefer_ipv6);
int aura_udp_bind(const char *host, int64_t port);
int aura_udp_wait(const char *host, int64_t port, int timeout_ms);
const char *aura_udp_receive(const char *host, int64_t port, int64_t capacity,
                             int64_t *source_port, const char **source_host);
int64_t aura_udp_send(const char *host, int64_t port, const char *target_host,
                      int64_t target_port, const char *payload);
int aura_udp_close(const char *host, int64_t port);
const char *aura_udp_last_error(void);
int aura_ws_connect(const char *endpoint);
int64_t aura_ws_send(const char *endpoint, int64_t kind, const char *payload);
const char *aura_ws_receive(const char *endpoint, int64_t *kind);
int aura_ws_close(const char *endpoint);
const char *aura_url_normalize_path(const char *path);
_Bool aura_json_is_valid(const char *value);
int64_t aura_json_error_offset(const char *value);
const char *aura_json_escape_string(const char *value);
const char *aura_json_object_get(const char *value, const char *key);
const char *aura_json_array_at(const char *value, int64_t index);
int64_t aura_json_array_count(const char *value);
const char *aura_json_object_keys(const char *value);
const char *aura_json_decode_string(const char *value);
const char *aura_json_duplicate_key(const char *value);
int aura_signal_install_shutdown(void);
_Bool aura_signal_shutdown_requested(void);
void aura_signal_clear_shutdown(void);
int64_t aura_error_kind_code(int64_t code);
_Bool aura_fs_is_directory(const char *path);
int64_t aura_fs_file_mode(const char *path);
int64_t aura_fs_permissions(const char *path);
int64_t aura_fs_modified_millis(const char *path);
const char *aura_fs_list_names(const char *path);
_Bool aura_fs_is_symlink(const char *path);

typedef enum AuraFfiStatus {
  AURA_FFI_OK = 0,
  AURA_FFI_INVALID = 1,
  AURA_FFI_OOM = 2
} AuraFfiStatus;

/* Stable type-erased ownership contract for genuinely open generic payloads.
 * Concrete compiler monomorphs continue to use their typed ABI; this shape
 * is reserved for values that must cross a runtime/plugin boundary without
 * pretending that an unknown T is an integer. */
typedef void *(*AuraTypeErasedCloneFn)(const void *data, size_t size,
                                       size_t *cloned_size);
typedef void (*AuraTypeErasedDropFn)(void *data, size_t size);
typedef void (*AuraTypeErasedMarkFn)(const void *data, size_t size);
typedef struct AuraTypeErasedOps {
  uint32_t abi_version;
  AuraTypeErasedCloneFn clone;
  AuraTypeErasedDropFn drop;
  AuraTypeErasedMarkFn mark;
} AuraTypeErasedOps;
typedef struct AuraTypeErasedValue {
  void *data;
  size_t size;
  const AuraTypeErasedOps *ops;
} AuraTypeErasedValue;

#define AURA_TYPE_ERASED_ABI_VERSION 1u
AuraFfiStatus aura_type_erased_clone(const AuraTypeErasedValue *source,
                                     AuraTypeErasedValue *out);
void aura_type_erased_drop(AuraTypeErasedValue *value);
void aura_type_erased_mark(const AuraTypeErasedValue *value);
AuraFfiStatus aura_task_frame_set_erased_result(
    AuraTaskFrame *frame, const AuraTypeErasedValue *value);
/* Clone the terminal erased result; `out` owns its payload independently of
 * the frame and remains valid after the frame is released. */
AuraFfiStatus aura_task_frame_result_erased(
    const AuraTaskFrame *frame, AuraTypeErasedValue *out);

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
/* Allocate an object whose callback precisely traces every GC field. */
void *aura_gc_alloc_typed(size_t size, void (*dtor)(void *),
                          void (*trace)(void *));

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
typedef struct AuraTaskScope AuraTaskScope;
typedef struct AuraReactor AuraReactor;
typedef int (*AuraReactorPollFn)(void *data, AuraTaskExecutor *executor,
                                 int timeout_ms);
typedef void (*AuraReactorDestroyFn)(void *data);

#define AURA_REACTOR_ABI_VERSION 1u

/* A reactor owns only readiness policy. Task frames remain executor-owned;
 * poll implementations must wake them through aura_task_executor_wake_waiting
 * and must not destroy or retain a frame after the poll call returns. */
AuraReactor *aura_reactor_new(AuraReactorPollFn poll, void *data,
                              AuraReactorDestroyFn data_destroy);
AuraReactor *aura_reactor_posix_new(void);
void aura_reactor_destroy(AuraReactor *reactor);
int aura_task_executor_set_reactor(AuraTaskExecutor *executor,
                                   AuraReactor *reactor);
void aura_gc_collect_executor(AuraTaskExecutor *executor);
typedef struct AuraTaskChannel AuraTaskChannel;
typedef struct AuraTaskSelect AuraTaskSelect;
typedef void (*AuraTaskFrameGcMarkFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskFrameDataDropFn)(AuraTaskFrame *frame, void *data,
                                        size_t size);
typedef void (*AuraTaskBlockingFn)(AuraTaskFrame *frame, void *environment);
typedef void (*AuraTaskBlockingEnvDestroyFn)(void *environment);
AuraTaskFrame *aura_task_frame_new_blocking(
    AuraTaskExecutor *executor, AuraTaskBlockingFn function, void *environment,
    AuraTaskBlockingEnvDestroyFn environment_destroy);
AuraTaskScope *aura_task_scope_begin(AuraTaskExecutor *executor);
int aura_task_scope_end(AuraTaskScope *scope);
AuraTaskSelect *aura_task_select_new(void);
int aura_task_select_add(AuraTaskSelect *select, AuraTaskChannel *channel);
void aura_task_select_destroy(AuraTaskSelect *select);
int64_t aura_time_monotonic_millis(void);
int aura_task_frame_set_cancel_deadline(AuraTaskFrame *frame, int timeout_ms);
int aura_task_frame_link_cancellation(AuraTaskFrame *parent,
                                      AuraTaskFrame *child);
int aura_task_executor_set_max_live_tasks(AuraTaskExecutor *executor,
                                          size_t max_live_tasks);
int aura_task_executor_start_workers(AuraTaskExecutor *executor,
                                     size_t worker_count);
void aura_task_executor_stop_workers(AuraTaskExecutor *executor);
#ifndef AURA_TASK_POLL_STATE_DEFINED
#define AURA_TASK_POLL_STATE_DEFINED 1
typedef enum AuraTaskPollState {
  AURA_TASK_READY = 0,
  AURA_TASK_PENDING = 1,
  AURA_TASK_COMPLETE = 2,
  AURA_TASK_FAILED = 3,
  AURA_TASK_CANCELLED = 4
} AuraTaskPollState;
#endif
typedef struct AuraTcpListener AuraTcpListener;
typedef struct AuraTcpStream AuraTcpStream;
typedef struct AuraIoOperationHandle AuraIoOperationHandle;
typedef void (*AuraIoOperationCleanupFn)(void *resource);

/* HTTP task handlers borrow request/response objects while the connection
 * frame is polling. A handle-aware entry point pins the connection resource
 * across readiness waits and releases it only after terminal cleanup. */
typedef struct AuraHttpConnection AuraHttpConnection;
typedef struct AuraHttpRequest AuraHttpRequest;
typedef struct AuraHttpResponse AuraHttpResponse;

/* Borrowed typed HTTP views. Returned strings/bytes remain owned by the
 * request or response and are valid only while that object is alive. */
const char *aura_http_request_method(const AuraHttpRequest *request);
const char *aura_http_request_target(const AuraHttpRequest *request);
const char *aura_http_request_version(const AuraHttpRequest *request);
size_t aura_http_request_header_count(const AuraHttpRequest *request);
const char *aura_http_request_header_name(const AuraHttpRequest *request,
                                          size_t index);
const char *aura_http_request_header_value(const AuraHttpRequest *request,
                                           size_t index);
const unsigned char *aura_http_request_body(const AuraHttpRequest *request);
size_t aura_http_request_body_length(const AuraHttpRequest *request);
/* Returns an AuraTcpStatus value without requiring the optional net ABI. */
int aura_http_request_read_body(const AuraHttpRequest *request,
                                unsigned char *out, size_t capacity,
                                size_t *out_bytes);
int aura_http_request_body_read_begin(const AuraHttpRequest *request);
void aura_http_request_body_read_end(const AuraHttpRequest *request);
int aura_http_request_wait_body(AuraTaskFrame *frame,
                                const AuraHttpRequest *request);
int aura_http_response_status(const AuraHttpResponse *response);
size_t aura_http_response_header_count(const AuraHttpResponse *response);
const char *aura_http_response_header_name(const AuraHttpResponse *response,
                                           size_t index);
const char *aura_http_response_header_value(const AuraHttpResponse *response,
                                            size_t index);
const unsigned char *aura_http_response_body(const AuraHttpResponse *response);
size_t aura_http_response_body_length(const AuraHttpResponse *response);
int aura_http_response_keep_alive(const AuraHttpResponse *response);
int aura_http_response_stream_started(const AuraHttpResponse *response);
int aura_http_response_stream_begin(AuraHttpResponse *response, void *output,
                                    size_t capacity, size_t *out_length);
int aura_http_response_stream_chunk(const void *chunk, size_t chunk_length,
                                    void *output, size_t capacity,
                                    size_t *out_length);
int aura_http_response_stream_finish(const AuraHttpResponse *response,
                                     void *output, size_t capacity,
                                     size_t *out_length);
int aura_http_connection_stream_write(AuraHttpConnection *connection,
                                      const void *data, size_t length,
                                      size_t *out_written);
int aura_http_connection_wait_write(AuraTaskFrame *frame,
                                    const AuraHttpConnection *connection);

typedef AuraTaskPollState (*AuraHttpTaskHandler)(AuraTaskFrame *frame,
                                                  const AuraHttpRequest *request,
                                                  AuraHttpResponse *response,
                                                  void *user_data);
AuraTaskPollState aura_http_connection_poll_async_task_handle(
    AuraTaskFrame *frame, AuraFfiOpaqueHandle *handle,
    AuraHttpTaskHandler handler, void *user_data);

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

/* Compiler bridge for the bounded generated descriptor-read frame.  A
 * nonnegative return is a byte count; a negative return is -errno. */
int64_t aura_io_read_fd(int fd, void *buffer, uint64_t capacity);
int64_t aura_io_write_fd(int fd, const void *buffer, uint64_t length);

/* A suspended frame owns its opaque data, but the runtime cannot infer which
 * fields contain GC references.  The mark callback must call aura_gc_mark_ptr
 * for every GC object reachable from that frame's live state. */
void aura_task_frame_set_gc_mark(AuraTaskFrame *frame,
                                 AuraTaskFrameGcMarkFn mark);
/* Drop typed references stored in frame data exactly once, after the
 * poll-specific destroy callback and before the frame data is released. */
void aura_task_frame_set_data_drop(AuraTaskFrame *frame,
                                   AuraTaskFrameDataDropFn drop);

/* Retain a foreign handle pin in the task frame until frame destruction.  This
 * is the ownership bridge for compiler-generated TASK/AWAIT state; callers do
 * not need to keep a pin token in an ad-hoc side allocation. */
AuraFfiStatus aura_task_frame_pin_foreign_handle(AuraTaskFrame *frame,
                                                 AuraFfiOpaqueHandle *handle,
                                                 AuraFfiBoundary boundary);

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
/* Retain one independent owner for a task/frame capture.  The retained
 * owner keeps the resource live until its matching drop call. */
AuraFfiStatus aura_ffi_handle_retain(AuraFfiOpaqueHandle *handle);
/* Destructor for compiler-created exception payload wrappers containing one
 * retained opaque handle pointer. */
void aura_destroy_foreign_handle_payload(void *payload);
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
/* Drop one lexical/frame owner without invalidating other retained owners. */
AuraFfiStatus aura_ffi_handle_drop(AuraFfiOpaqueHandle **handle);

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
typedef void *(*AuraFfiPayloadCloneFn)(const void *payload, uint64_t payload_len,
                                       uint64_t *cloned_len);
typedef void (*AuraFfiPayloadDestroyFn)(void *payload, uint64_t payload_len);

#define AURA_FFI_MAX_OWNED_CALLBACK_PAYLOAD (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))

/* An owned callback snapshot. The destroy hook is part of the value so an
 * allocator-specific payload never crosses the ABI without its destructor. */
typedef struct AuraFfiOwnedPayload {
  void *data;
  uint64_t len;
  AuraFfiPayloadDestroyFn destroy;
} AuraFfiOwnedPayload;

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
/* Clone the borrowed input before synchronous delivery and return that clone
 * to the caller only for a successful foreign outcome. The callback never
 * receives a pointer whose lifetime ends before its invocation returns. */
AuraFfiStatus aura_ffi_callback_invoke_owned(
    AuraFfiCallback *callback, uint64_t current_task, AuraFfiBoundary boundary,
    const void *payload, uint64_t payload_len, AuraFfiPayloadCloneFn clone,
    AuraFfiPayloadDestroyFn destroy, AuraFfiOwnedPayload *owned_payload,
    AuraFfiOutcome *outcome);
AuraFfiStatus aura_ffi_owned_payload_destroy(AuraFfiOwnedPayload *payload);
AuraFfiStatus aura_ffi_callback_deregister(AuraFfiCallback *callback);
AuraFfiStatus aura_ffi_callback_shutdown(AuraFfiCallback *callback);
AuraFfiStatus aura_ffi_callback_destroy(AuraFfiCallback **callback);

#endif
