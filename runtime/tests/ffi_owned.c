#include <assert.h>
#include <stdint.h>
#include <string.h>

#include "../aura_ffi.h"

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

static void test_string_borrow_copy_transfer(void)
{
  AuraFfiStringView view;
  AuraFfiString copy = {0};
  AuraFfiString moved = {0};
  const char text[] = "nested\0ignored";

  assert(aura_ffi_string_borrow(text, 6, &view) == AURA_FFI_OK);
  assert(view.data == text && view.len == 6);
  assert(aura_ffi_string_copy(view, &copy) == AURA_FFI_OK);
  assert(copy.len == 6 && memcmp(copy.data, "nested", 6) == 0);
  assert(copy.data != text);
  aura_ffi_string_destroy(&copy);
  aura_ffi_string_destroy(&copy); /* exactly-once cleanup is idempotent */

  char *owned = (char *)malloc(5);
  assert(owned != NULL);
  memcpy(owned, "copy", 5);
  assert(aura_ffi_string_transfer(owned, 4, &moved) == AURA_FFI_OK);
  assert(moved.data == owned && moved.len == 4);
  aura_ffi_string_destroy(&moved);
}

static void test_array_empty_large_and_copy(void)
{
  AuraFfiArrayView empty;
  AuraFfiArray copied = {0};
  assert(aura_ffi_array_borrow(NULL, 0, 0, sizeof(int64_t),
                               AURA_FFI_ARRAY_INT64, &empty) == AURA_FFI_OK);
  assert(aura_ffi_array_copy(empty, &copied) == AURA_FFI_OK);
  assert(copied.data == NULL && copied.len == 0);
  aura_ffi_array_destroy(&copied);

  int64_t values[1024];
  for (size_t i = 0; i < 1024; i++) values[i] = (int64_t)i * 3;
  AuraFfiArrayView view;
  assert(aura_ffi_array_borrow(values, 1024, 1024, sizeof(int64_t),
                               AURA_FFI_ARRAY_INT64, &view) == AURA_FFI_OK);
  assert(aura_ffi_array_copy(view, &copied) == AURA_FFI_OK);
  assert(copied.data != values && copied.len == 1024);
  assert(((int64_t *)copied.data)[1023] == values[1023]);
  aura_ffi_array_destroy(&copied);
}

static void test_root_guard_lifetime(void)
{
  void *slot = aura_gc_alloc(16);
  AuraFfiRootGuard guard = {0};
  assert(slot != NULL);
  assert(aura_ffi_root_begin(&guard, &slot) == AURA_FFI_OK);
  aura_gc_collect();
  assert(slot != NULL);
  aura_ffi_root_end(&guard);
  aura_ffi_root_end(&guard);
  aura_gc_collect();
  aura_gc_shutdown();
}

int main(void)
{
  test_string_borrow_copy_transfer();
  test_array_empty_large_and_copy();
  test_root_guard_lifetime();
  return 0;
}
