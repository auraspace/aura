#include <assert.h>
#include <stdint.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  int mode;
  int polls;
} OutcomeTask;

typedef struct
{
  int *cleanup_count;
  uint32_t tag;
} CleanupToken;

static int cleanup_count;

static void cleanup_token(void *data, size_t size)
{
  CleanupToken *token = (CleanupToken *)data;
  assert(size == sizeof(*token));
  assert(token->cleanup_count == &cleanup_count);
  (*token->cleanup_count)++;
  free(token);
}

static void destroy_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

static AuraTaskPollState cancel_with_error(AuraTaskFrame *frame)
{
  int *error = (int *)malloc(sizeof(*error));
  assert(error != NULL);
  *error = 88;
  aura_task_frame_set_error_span(frame, error, sizeof(*error), destroy_int,
                                 UINT32_C(0xca11), 401, 409);
  return AURA_TASK_FAILED;
}

static AuraTaskPollState poll_outcome(AuraTaskFrame *frame)
{
  OutcomeTask *task = (OutcomeTask *)aura_task_frame_data(frame);
  task->polls++;

  /* A state-machine cleanup action is represented by releasing the owned
   * capture before installing the terminal result/error payload. */
  aura_task_frame_set_captures(frame, NULL, 0, NULL);
  if (task->mode == 0)
  {
    int *result = (int *)malloc(sizeof(*result));
    assert(result != NULL);
    *result = 42;
    aura_task_frame_set_result(frame, result, sizeof(*result), destroy_int);
    return AURA_TASK_COMPLETE;
  }
  if (task->mode == 1)
  {
    int *error = (int *)malloc(sizeof(*error));
    assert(error != NULL);
    *error = 77;
    aura_task_frame_set_error_at(frame, error, sizeof(*error), destroy_int,
                                 0xa701U);
    return AURA_TASK_FAILED;
  }
  assert(!"cancelled tasks must not enter their poll callback");
  return AURA_TASK_FAILED;
}

static AuraTaskFrame *new_outcome_task(int mode)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(OutcomeTask), poll_outcome,
                                             NULL);
  assert(frame != NULL);
  ((OutcomeTask *)aura_task_frame_data(frame))->mode = mode;
  CleanupToken *token = (CleanupToken *)malloc(sizeof(*token));
  assert(token != NULL);
  token->cleanup_count = &cleanup_count;
  token->tag = UINT32_C(0xa7);
  aura_task_frame_set_captures(frame, token, sizeof(*token), cleanup_token);
  return frame;
}

static void test_success_direct_poll(void)
{
  AuraTaskFrame *frame = new_outcome_task(0);
  assert(aura_task_frame_poll_once(frame) == AURA_TASK_COMPLETE);
  AuraTaskResult result = aura_task_frame_result(frame);
  assert(result.data != NULL && *(int *)result.data == 42);
  assert(aura_task_frame_error(frame).data == NULL);
  assert(cleanup_count == 1);
  aura_task_frame_destroy(frame);
}

static void test_failure_join_preserves_source_identity(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_outcome_task(1);
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_FAILED);
  assert(result.data == NULL);
  assert(error.data != NULL && *(int *)error.data == 77);
  assert(aura_task_frame_error_source_id(frame) == UINT32_C(0xa701));
  assert(cleanup_count == 2);
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_FAILED);
  assert(aura_task_frame_error_source_id(frame) == UINT32_C(0xa701));
  assert(cleanup_count == 2);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(frame == NULL);
  aura_task_executor_shutdown(executor);
}

static void test_cancellation_is_deterministic_and_cleans_before_join(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_outcome_task(2);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_CANCELLED);
  assert(result.data == NULL && error.data == NULL);
  assert(((OutcomeTask *)aura_task_frame_data(frame))->polls == 0);
  assert(cleanup_count == 3);
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_CANCELLED);
  assert(cleanup_count == 3);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
}

static void test_cancellation_exception_is_failure_after_cleanup(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_outcome_task(2);
  aura_task_frame_set_cancel_handler(frame, cancel_with_error);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_FAILED);
  assert(result.data == NULL);
  assert(error.data != NULL && *(int *)error.data == 88);
  assert(aura_task_frame_error_source_id(frame) == UINT32_C(0xca11));
  assert(aura_task_frame_error_span_start(frame) == 401);
  assert(aura_task_frame_error_span_end(frame) == 409);
  assert(cleanup_count == 4);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
}

int main(void)
{
  test_success_direct_poll();
  test_failure_join_preserves_source_identity();
  test_cancellation_is_deterministic_and_cleans_before_join();
  test_cancellation_exception_is_failure_after_cleanup();
  return 0;
}
