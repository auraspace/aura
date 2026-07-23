#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static AuraTaskPollState poll_once_after_wake(AuraTaskFrame *frame)
{
  if (aura_task_frame_resume_state(frame) == 0)
  {
    aura_task_frame_set_resume_state(frame, 1);
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_once_after_wake, NULL);
  int token = 0;

  assert(executor != NULL && frame != NULL);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  aura_task_frame_set_waiting(frame, &token);
  assert(aura_task_frame_is_waiting(frame));
  assert(aura_task_frame_waiting_token(frame) == &token);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  /* The adapter-owned helper clears the token before wake. */
  assert(aura_task_executor_wake_waiting(executor, frame) == 1);
  assert(!aura_task_frame_is_waiting(frame));
  assert(aura_task_frame_waiting_token(frame) == NULL);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);

  /* A completed operation cannot be delivered a second time. */
  assert(aura_task_executor_wake_waiting(executor, frame) == 0);

  aura_task_executor_shutdown(executor);
  return 0;
}
