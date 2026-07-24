#include <assert.h>
#include <stdint.h>

#include "../aura_ffi.h"

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

static unsigned released_resources;

typedef struct
{
  AuraFfiOpaqueHandle *handle;
  AuraFfiHandlePin pin;
  unsigned polls;
  unsigned releases_before;
} AsyncFfiTask;

static void destroy_resource(void *resource)
{
  assert(resource != NULL);
  released_resources++;
  free(resource);
}

static void test_nullable_and_boundaries(void)
{
  AuraFfiOpaqueHandle *empty = NULL;
  assert(aura_ffi_handle_new(NULL, NULL, &empty) == AURA_FFI_INVALID);
  assert(aura_ffi_handle_new_nullable(NULL, NULL, &empty) == AURA_FFI_OK);
  assert(aura_ffi_handle_is_null(empty));
  assert(aura_ffi_handle_check_boundary(empty, AURA_FFI_BOUNDARY_SYNC) ==
         AURA_FFI_OK);
  assert(aura_ffi_handle_check_boundary(empty, AURA_FFI_BOUNDARY_TASK) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(aura_ffi_handle_check_boundary(empty, AURA_FFI_BOUNDARY_AWAIT) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(aura_ffi_handle_check_boundary(empty, AURA_FFI_BOUNDARY_CHANNEL) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(aura_ffi_handle_check_boundary(empty, AURA_FFI_BOUNDARY_CALLBACK) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(aura_ffi_handle_release(empty) == AURA_FFI_OK);
  assert(aura_ffi_handle_release(empty) == AURA_FFI_INVALID);
  assert(aura_ffi_handle_destroy(&empty) == AURA_FFI_OK);
  assert(empty == NULL);
}

static void test_pin_release_and_stale_alias(void)
{
  int *value = (int *)malloc(sizeof(*value));
  assert(value != NULL);
  *value = 42;

  AuraFfiOpaqueHandle *handle = NULL;
  assert(aura_ffi_handle_new(value, destroy_resource, &handle) == AURA_FFI_OK);
  AuraFfiOpaqueHandle *stale_alias = handle;
  AuraFfiHandlePin live_pin = {0};
  AuraFfiHandlePin stale_pin = {0};
  void *resource = NULL;
  assert(aura_ffi_handle_pin(handle, &live_pin) == AURA_FFI_OK);
  assert(aura_ffi_handle_pin_resource(&live_pin, &resource) == AURA_FFI_OK);
  assert(resource == value && *(int *)resource == 42);

  /* Release invalidates all aliases immediately, but defers destruction until
   * the outstanding operation pin is returned. */
  assert(aura_ffi_handle_release(handle) == AURA_FFI_OK);
  assert(released_resources == 0);
  assert(aura_ffi_handle_is_null(stale_alias));
  assert(aura_ffi_handle_pin(stale_alias, &stale_pin) == AURA_FFI_INVALID);
  assert(aura_ffi_handle_pin_resource(&stale_pin, &resource) == AURA_FFI_INVALID);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_BUSY);
  assert(aura_ffi_handle_unpin(&stale_pin) == AURA_FFI_INVALID);
  assert(aura_ffi_handle_unpin(&live_pin) == AURA_FFI_OK);
  assert(released_resources == 1);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_OK);

  /* Re-create the pin state for the deferred-release path. */
  AuraFfiOpaqueHandle *second = NULL;
  value = (int *)malloc(sizeof(*value));
  assert(value != NULL);
  assert(aura_ffi_handle_new(value, destroy_resource, &second) == AURA_FFI_OK);
  assert(aura_ffi_handle_pin(second, &live_pin) == AURA_FFI_OK);
  assert(aura_ffi_handle_release(second) == AURA_FFI_OK);
  assert(aura_ffi_handle_unpin(&live_pin) == AURA_FFI_OK);
  assert(released_resources == 2);
  assert(aura_ffi_handle_destroy(&second) == AURA_FFI_OK);
}

static void test_pin_and_destroy_boundaries(void)
{
  AuraFfiOpaqueHandle *handle = NULL;
  int *value = (int *)malloc(sizeof(*value));
  assert(value != NULL);
  assert(aura_ffi_handle_new(value, destroy_resource, &handle) == AURA_FFI_OK);

  AuraFfiHandlePin pin = {0};
  assert(aura_ffi_handle_pin(handle, &pin) == AURA_FFI_OK);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_INVALID);
  assert(aura_ffi_handle_release(handle) == AURA_FFI_OK);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_BUSY);
  assert(aura_ffi_handle_unpin(&pin) == AURA_FFI_OK);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_OK);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_INVALID);
}

static void test_async_boundary_pin_owns_resource(void)
{
  int *value = (int *)malloc(sizeof(*value));
  assert(value != NULL);
  *value = 77;

  AuraFfiOpaqueHandle *handle = NULL;
  AuraFfiHandlePin pin = {0};
  AuraFfiHandlePin rejected = {(AuraFfiOpaqueHandle *)1, (void *)1, 1};
  void *resource = NULL;
  assert(aura_ffi_handle_new(value, destroy_resource, &handle) == AURA_FFI_OK);

  assert(aura_ffi_handle_pin_for_boundary(
             handle, AURA_FFI_BOUNDARY_TASK, &pin) == AURA_FFI_OK);
  assert(aura_ffi_handle_pin_resource(&pin, &resource) == AURA_FFI_OK);
  assert(resource == value && *(int *)resource == 77);
  assert(aura_ffi_handle_release(handle) == AURA_FFI_OK);
  assert(released_resources == 3);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_BUSY);
  assert(aura_ffi_handle_unpin(&pin) == AURA_FFI_OK);
  assert(released_resources == 4);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_OK);

  value = (int *)malloc(sizeof(*value));
  assert(value != NULL);
  assert(aura_ffi_handle_new(value, destroy_resource, &handle) == AURA_FFI_OK);
  assert(aura_ffi_handle_pin_for_boundary(
             handle, AURA_FFI_BOUNDARY_AWAIT, &pin) == AURA_FFI_OK);
  assert(aura_ffi_handle_pin_for_boundary(
             handle, AURA_FFI_BOUNDARY_CHANNEL, &rejected) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(rejected.handle == NULL && rejected.resource == NULL &&
         rejected.generation == 0);
  assert(aura_ffi_handle_pin_for_boundary(
             handle, AURA_FFI_BOUNDARY_CALLBACK, &rejected) ==
         AURA_FFI_BOUNDARY_REJECTED);
  assert(aura_ffi_handle_unpin(&pin) == AURA_FFI_OK);
  assert(aura_ffi_handle_release(handle) == AURA_FFI_OK);
  assert(aura_ffi_handle_destroy(&handle) == AURA_FFI_OK);
  assert(released_resources == 5);
}

static AuraTaskPollState poll_async_ffi_handle(AuraTaskFrame *frame)
{
  AsyncFfiTask *task = (AsyncFfiTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    assert(aura_ffi_handle_pin_for_boundary(
               task->handle, AURA_FFI_BOUNDARY_TASK, &task->pin) ==
           AURA_FFI_OK);
    task->releases_before = released_resources;

    /* The owner may release its alias while the task still owns a pin.  The
     * resource must stay alive, but the released handle must not be reused. */
    assert(aura_ffi_handle_release(task->handle) == AURA_FFI_OK);
    assert(released_resources == task->releases_before);
    assert(aura_ffi_handle_destroy(&task->handle) == AURA_FFI_BUSY);
    return AURA_TASK_PENDING;
  }

  void *resource = NULL;
  assert(aura_ffi_handle_pin_resource(&task->pin, &resource) ==
         AURA_FFI_INVALID);
  assert(resource == NULL);
  assert(released_resources == task->releases_before);
  assert(aura_ffi_handle_unpin(&task->pin) == AURA_FFI_OK);
  assert(released_resources == task->releases_before + 1);
  assert(aura_ffi_handle_destroy(&task->handle) == AURA_FFI_OK);
  return AURA_TASK_COMPLETE;
}

static void test_task_frame_pin_owns_foreign_resource(void)
{
  int *value = (int *)malloc(sizeof(*value));
  assert(value != NULL);
  *value = 88;

  AuraFfiOpaqueHandle *handle = NULL;
  assert(aura_ffi_handle_new(value, destroy_resource, &handle) == AURA_FFI_OK);

  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame =
      aura_task_frame_new(sizeof(AsyncFfiTask), poll_async_ffi_handle, NULL);
  assert(frame != NULL);
  AsyncFfiTask *task = (AsyncFfiTask *)aura_task_frame_data(frame);
  task->handle = handle;

  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_wake(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(task->handle == NULL);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
}

int main(void)
{
  test_nullable_and_boundaries();
  test_pin_release_and_stale_alias();
  test_pin_and_destroy_boundaries();
  test_async_boundary_pin_owns_resource();
  test_task_frame_pin_owns_foreign_resource();
  assert(released_resources == 6);
  return 0;
}
