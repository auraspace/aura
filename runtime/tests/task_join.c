#include <assert.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  int polls;
  int outcome;
  int value;
} JoinTask;

static void destroy_result(void *data, size_t size)
{
  (void)size;
  free(data);
}

static AuraTaskPollState poll_join_task(AuraTaskFrame *frame)
{
  JoinTask *task = (JoinTask *)aura_task_frame_data(frame);
  task->polls++;
  if (task->polls == 1 && task->outcome == 0)
  {
    return AURA_TASK_READY;
  }
  if (task->outcome == 1)
  {
    int *error = (int *)malloc(sizeof(*error));
    assert(error != NULL);
    *error = 99;
    aura_task_frame_set_error(frame, error, sizeof(*error), destroy_result);
    return AURA_TASK_FAILED;
  }
  if (task->outcome == 2)
  {
    return AURA_TASK_CANCELLED;
  }
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = task->value;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_result);
  return AURA_TASK_COMPLETE;
}

static AuraTaskFrame *new_join_task(int outcome, int value)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(JoinTask), poll_join_task, NULL);
  assert(frame != NULL);
  JoinTask *task = (JoinTask *)aura_task_frame_data(frame);
  task->outcome = outcome;
  task->value = value;
  return frame;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);

  /* Join-before-completion: first poll yields READY, second completes. */
  AuraTaskFrame *success = new_join_task(0, 42);
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  assert(aura_task_executor_join(executor, success, &result, &error) == AURA_TASK_COMPLETE);
  assert(result.data != NULL && *(int *)result.data == 42);

  /* A terminal frame that was polled before join is observed without submit. */
  AuraTaskFrame *already_complete = new_join_task(0, 7);
  assert(aura_task_frame_poll_once(already_complete) == AURA_TASK_READY);
  assert(aura_task_frame_poll_once(already_complete) == AURA_TASK_COMPLETE);
  assert(aura_task_executor_join(executor, already_complete, &result, &error) ==
         AURA_TASK_COMPLETE);
  assert(result.data != NULL && *(int *)result.data == 7);
  aura_task_frame_destroy(already_complete);
  assert(error.data == NULL);
  /* A second observation does not submit the executor-owned frame again. */
  assert(aura_task_executor_join(executor, success, &result, &error) == AURA_TASK_COMPLETE);
  assert(result.data != NULL && *(int *)result.data == 42);

  AuraTaskFrame *failed = new_join_task(1, 0);
  assert(aura_task_executor_join(executor, failed, &result, &error) == AURA_TASK_FAILED);
  assert(result.data == NULL);
  assert(error.data != NULL && *(int *)error.data == 99);

  AuraTaskFrame *cancelled = new_join_task(2, 0);
  assert(aura_task_executor_submit(executor, cancelled) == 1);
  assert(aura_task_executor_cancel(executor, cancelled) == 1);
  assert(aura_task_executor_join(executor, cancelled, &result, &error) == AURA_TASK_CANCELLED);
  assert(result.data == NULL && error.data == NULL);

  aura_task_executor_shutdown(executor);
  return 0;
}
