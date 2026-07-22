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
AuraFfiStatus aura_ffi_handle_pin_resource(const AuraFfiHandlePin *pin,
                                           void **out_resource);
AuraFfiStatus aura_ffi_handle_unpin(AuraFfiHandlePin *pin);

/* Release invalidates the resource immediately and invokes its destructor at
 * most once (deferred until all pins are unpinned).  Invalidation is the same
 * operation for runtimes that observe an external resource death. */
AuraFfiStatus aura_ffi_handle_release(AuraFfiOpaqueHandle *handle);
AuraFfiStatus aura_ffi_handle_invalidate(AuraFfiOpaqueHandle *handle);
AuraFfiStatus aura_ffi_handle_destroy(AuraFfiOpaqueHandle **handle);

/* Only synchronous calls may carry an opaque pointer handle in this alpha
 * ABI.  Task, await, channel, and callback crossings are rejected. */
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
