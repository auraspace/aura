#include <assert.h>
#include <stdlib.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/runtime.c"

typedef struct
{
  int mode;
} OutcomeApiTask;

static int payload_drops;

static void drop_payload(void *data, size_t size)
{
  assert(size == sizeof(int));
  payload_drops++;
  free(data);
}

static void *clone_payload(const void *data, size_t size, size_t *out_size)
{
  void *copy = malloc(size);
  assert(copy != NULL);
  memcpy(copy, data, size);
  *out_size = size;
  return copy;
}

static AuraTaskFrame *new_empty_frame(void)
{
  AuraTaskFrame *frame = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(frame != NULL);
  return frame;
}

static AuraTaskPollState poll_outcome_api(AuraTaskFrame *frame)
{
  OutcomeApiTask *task = (OutcomeApiTask *)aura_task_frame_data(frame);
  if (task->mode == 1)
  {
    int *error = (int *)malloc(sizeof(*error));
    assert(error != NULL);
    *error = 17;
    aura_task_frame_set_error_at(frame, error, sizeof(*error), drop_payload,
                                 UINT32_C(0xabad));
    return AURA_TASK_FAILED;
  }
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = 42;
  aura_task_frame_set_result(frame, result, sizeof(*result), drop_payload);
  return AURA_TASK_COMPLETE;
}

static AuraTaskFrame *new_outcome_api_task(int mode)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(OutcomeApiTask),
                                             poll_outcome_api, NULL);
  assert(frame != NULL);
  ((OutcomeApiTask *)aura_task_frame_data(frame))->mode = mode;
  return frame;
}

static void assert_same_outcome(AuraTaskOutcome first, AuraTaskOutcome second)
{
  assert(first.state == second.state);
  assert(first.result.data == second.result.data);
  assert(first.result.size == second.result.size);
  assert(first.error.data == second.error.data);
  assert(first.error.size == second.error.size);
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);

  AuraTaskFrame *success = new_outcome_api_task(0);
  AuraTaskOutcome success_first =
      aura_task_executor_join_outcome(executor, success);
  AuraTaskOutcome success_second =
      aura_task_executor_join_outcome(executor, success);
  assert(success_first.state == AURA_TASK_COMPLETE);
  assert(success_first.result.data != NULL &&
         *(int *)success_first.result.data == 42);
  assert(success_first.error.data == NULL);
  assert_same_outcome(success_first, success_second);
  AuraTaskOwnedOutcome owned_success = {0};
  assert(aura_task_outcome_clone(&success_first, clone_payload, drop_payload,
                                 clone_payload, drop_payload, &owned_success));
  assert(owned_success.state == AURA_TASK_COMPLETE);
  assert(owned_success.result.data != NULL &&
         *(int *)owned_success.result.data == 42);
  assert(aura_task_executor_release(executor, &success) == 1);
  assert(success == NULL);
  assert(payload_drops == 1);
  assert(*(int *)owned_success.result.data == 42);
  aura_task_owned_outcome_destroy(&owned_success);
  assert(payload_drops == 2);

  AuraTaskFrame *failed = new_outcome_api_task(1);
  AuraTaskOutcome failed_first =
      aura_task_executor_join_outcome(executor, failed);
  AuraTaskOutcome failed_second =
      aura_task_executor_join_outcome(executor, failed);
  assert(failed_first.state == AURA_TASK_FAILED);
  assert(failed_first.result.data == NULL);
  assert(failed_first.error.data != NULL &&
         *(int *)failed_first.error.data == 17);
  assert_same_outcome(failed_first, failed_second);
  assert(aura_task_frame_error_source_id(failed) == UINT32_C(0xabad));
  AuraTaskOwnedOutcome owned_failed = {0};
  assert(aura_task_outcome_clone(&failed_first, clone_payload, drop_payload,
                                 clone_payload, drop_payload, &owned_failed));
  assert(owned_failed.state == AURA_TASK_FAILED);
  assert(owned_failed.error.data != NULL &&
         *(int *)owned_failed.error.data == 17);
  assert(aura_task_executor_release(executor, &failed) == 1);
  assert(payload_drops == 3);
  assert(*(int *)owned_failed.error.data == 17);
  aura_task_owned_outcome_destroy(&owned_failed);
  assert(payload_drops == 4);

  AuraTaskFrame *cancelled = new_outcome_api_task(0);
  assert(aura_task_executor_submit(executor, cancelled) == 1);
  assert(aura_task_executor_cancel(executor, cancelled) == 1);
  AuraTaskOutcome cancelled_first =
      aura_task_executor_join_outcome(executor, cancelled);
  AuraTaskOutcome cancelled_second =
      aura_task_executor_join_outcome(executor, cancelled);
  assert(cancelled_first.state == AURA_TASK_CANCELLED);
  assert(cancelled_first.result.data == NULL);
  assert(cancelled_first.error.data == NULL);
  assert_same_outcome(cancelled_first, cancelled_second);
  AuraTaskOwnedOutcome owned_cancelled = {0};
  assert(aura_task_outcome_clone(&cancelled_first, clone_payload, drop_payload,
                                 clone_payload, drop_payload, &owned_cancelled));
  assert(owned_cancelled.state == AURA_TASK_CANCELLED);
  assert(owned_cancelled.result.data == NULL && owned_cancelled.error.data == NULL);
  aura_task_owned_outcome_destroy(&owned_cancelled);
  assert(aura_task_executor_release(executor, &cancelled) == 1);
  assert(payload_drops == 4);

  /* Full outcome propagation clones a successful payload, so repeated
   * observations remain valid even when the parent is released first. */
  AuraTaskFrame *source = new_outcome_api_task(0);
  assert(aura_task_frame_poll_once(source) == AURA_TASK_COMPLETE);
  AuraTaskFrame *parent = new_empty_frame();
  assert(aura_task_frame_propagate_outcome(parent, source, clone_payload,
                                           drop_payload) == AURA_TASK_COMPLETE);
  AuraTaskResult copied = aura_task_frame_result(parent);
  assert(copied.data != NULL && *(int *)copied.data == 42);
  assert(copied.data != aura_task_frame_result(source).data);
  *(int *)aura_task_frame_result(source).data = 99;
  assert(*(int *)copied.data == 42);
  aura_task_frame_destroy(source);
  aura_task_frame_destroy(parent);
  assert(payload_drops == 6);

  /* Cancellation is an explicit terminal outcome, not a failed payload. */
  AuraTaskFrame *cancel_source = new_outcome_api_task(0);
  AuraTaskFrame *cancel_parent = new_empty_frame();
  assert(aura_task_executor_submit(executor, cancel_source) == 1);
  assert(aura_task_executor_cancel(executor, cancel_source) == 1);
  assert(aura_task_executor_join_outcome(executor, cancel_source).state ==
         AURA_TASK_CANCELLED);
  assert(aura_task_frame_propagate_outcome(cancel_parent, cancel_source,
                                           clone_payload, drop_payload) ==
         AURA_TASK_CANCELLED);
  assert(aura_task_frame_state(cancel_parent) == AURA_TASK_CANCELLED);
  assert(aura_task_frame_result(cancel_parent).data == NULL);
  assert(aura_task_frame_error(cancel_parent).data == NULL);
  aura_task_frame_destroy(cancel_parent);
  assert(aura_task_executor_release(executor, &cancel_source) == 1);

  /* Error text and raw class-like payloads travel independently through a
   * parent frame, and both remain valid after the source is released. */
  AuraTaskFrame *payload_source = new_outcome_api_task(1);
  assert(aura_task_executor_join_outcome(executor, payload_source).state ==
         AURA_TASK_FAILED);
  int *raw_error = (int *)malloc(sizeof(*raw_error));
  assert(raw_error != NULL);
  *raw_error = 73;
  aura_task_frame_set_error_payload_with_clone(
      payload_source, raw_error, sizeof(*raw_error), clone_payload, drop_payload);
  AuraTaskFrame *payload_parent = new_empty_frame();
  assert(aura_task_frame_propagate_error(payload_parent, payload_source) == 1);
  AuraTaskResult raw_copy = aura_task_frame_error_payload(payload_parent);
  assert(raw_copy.data != NULL && *(int *)raw_copy.data == 73);
  assert(raw_copy.data != aura_task_frame_error_payload(payload_source).data);
  assert(aura_task_executor_release(executor, &payload_source) == 1);
  assert(*(int *)raw_copy.data == 73);
  aura_task_frame_destroy(payload_parent);
  assert(payload_drops == 9);

  aura_task_executor_shutdown(executor);
  return 0;
}
