#include <assert.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

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
  assert(aura_task_executor_release(executor, &success) == 1);
  assert(success == NULL);
  assert(payload_drops == 1);

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
  assert(aura_task_executor_release(executor, &failed) == 1);
  assert(payload_drops == 2);

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
  assert(aura_task_executor_release(executor, &cancelled) == 1);
  assert(payload_drops == 2);

  aura_task_executor_shutdown(executor);
  return 0;
}
