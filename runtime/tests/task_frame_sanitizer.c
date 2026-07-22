#include <assert.h>
#include <stdint.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  uint64_t magic;
  int polls;
} PendingState;

static int malloc_drops;
static int gc_drops;

static void drop_malloc(void *data, size_t size)
{
  assert(size == sizeof(int));
  malloc_drops++;
  free(data);
}

static void drop_gc(void *data)
{
  (void)data;
  gc_drops++;
}

static void drop_pending(void *data, size_t size)
{
  assert(size == sizeof(int));
  drop_malloc(data, size);
}

static AuraTaskPollState poll_pending_then_ready(AuraTaskFrame *frame)
{
  PendingState *state = (PendingState *)aura_task_frame_data(frame);
  if (state->polls++ == 0)
  {
    int *operation = (int *)malloc(sizeof(*operation));
    assert(operation != NULL);
    *operation = 17;
    assert(aura_task_frame_set_pending_with_ownership(
               frame, operation, sizeof(*operation), drop_pending,
               AURA_TASK_TRANSFERRED) == 1);
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_COMPLETE;
}

static AuraTaskPollState poll_failure(AuraTaskFrame *frame)
{
  int *error = (int *)malloc(sizeof(*error));
  assert(error != NULL);
  *error = 91;
  aura_task_frame_set_error_at(frame, error, sizeof(*error), drop_malloc, 7001);
  return AURA_TASK_FAILED;
}

static AuraTaskFrame *new_pending_frame(void)
{
  AuraTaskFrame *frame = aura_task_frame_new(
      sizeof(PendingState), poll_pending_then_ready, NULL);
  assert(frame != NULL);
  return frame;
}

static void test_pending_frame_retains_gc_root(void)
{
  void *sentinel = NULL;
  aura_gc_add_root(&sentinel);

  AuraTaskFrame *frame = new_pending_frame();
  PendingState *state = (PendingState *)aura_task_frame_data(frame);
  state->magic = UINT64_C(0xa8c0ffee);
  void *payload = aura_gc_alloc_full(sizeof(uint64_t), drop_gc, NULL);
  assert(payload != NULL);
  *(uint64_t *)payload = UINT64_C(0xfeedface);
  aura_task_frame_set_captures(frame, payload, sizeof(uint64_t), NULL);

  assert(aura_task_frame_poll_once(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  aura_gc_collect();
  assert(*(uint64_t *)payload == UINT64_C(0xfeedface));
  assert(aura_task_frame_pending(frame).data != NULL);

  aura_task_frame_destroy(frame);
  aura_gc_collect();
  assert(gc_drops == 1);
  assert(malloc_drops == 1);
  aura_gc_remove_root(&sentinel);
}

static void test_repeated_polling(void)
{
  AuraTaskFrame *frame = new_pending_frame();
  assert(aura_task_frame_poll_once(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_poll_once(frame) == AURA_TASK_COMPLETE);
  assert(aura_task_frame_poll_once(frame) == AURA_TASK_COMPLETE);
  aura_task_frame_destroy(frame);
  assert(malloc_drops == 2);
}

static void test_cancellation(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_pending_frame();
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_CANCELLED);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(frame == NULL);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(malloc_drops == 3);
  aura_task_executor_shutdown(executor);
}

static void test_dropped_handle_shutdown(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *dropped_handle = new_pending_frame();
  assert(aura_task_executor_submit(executor, dropped_handle) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(dropped_handle) == AURA_TASK_PENDING);
  aura_task_executor_shutdown(executor);
  dropped_handle = NULL;
  assert(malloc_drops == 4);
}

static void test_failure_and_release(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_failure, NULL);
  assert(frame != NULL);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_FAILED);
  AuraTaskResult error = aura_task_frame_error(frame);
  assert(error.data != NULL && *(int *)error.data == 91);
  assert(aura_task_frame_error_source_id(frame) == 7001);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(frame == NULL);
  assert(malloc_drops == 5);
  aura_task_executor_shutdown(executor);
}

int main(void)
{
  test_pending_frame_retains_gc_root();
  test_repeated_polling();
  test_cancellation();
  test_dropped_handle_shutdown();
  test_failure_and_release();

  aura_gc_shutdown();
  assert(gc_drops == 1);
  assert(malloc_drops == 5);
  return 0;
}
