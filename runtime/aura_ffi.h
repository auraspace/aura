#ifndef AURA_FFI_H
#define AURA_FFI_H

/*
 * Stable, allocation-only C ABI for the bounded F3 slice.
 *
 * These are deliberately separate from Aura's internal String and Array
 * layouts.  A foreign function may borrow a view for the duration of a call,
 * copy a view into an Aura-owned value, or transfer a malloc-compatible buffer
 * exactly once.  No callback, arbitrary element destructor, or raw pointer
 * dereference is part of this ABI (those belong to F4/F5).
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

#endif
