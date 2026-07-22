#include <assert.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static int drops;

static void drop(void *data, size_t size)
{
  (void)size;
  drops++;
  free(data);
}

static AuraTaskPollState poll_twice(AuraTaskFrame *frame)
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
  AuraTaskFrame *frame = aura_task_frame_new(8, poll_twice, NULL);
  assert(frame != NULL);
  assert(frame->data_size == 8);
  assert(aura_task_frame_resume_state(frame) == 0);
  assert(aura_task_frame_state(frame) == AURA_TASK_READY);

  int *capture = malloc(sizeof(*capture));
  int *pending = malloc(sizeof(*pending));
  int *error = malloc(sizeof(*error));
  assert(capture != NULL && pending != NULL && error != NULL);
  aura_task_frame_set_captures(frame, capture, sizeof(*capture), drop);
  aura_task_frame_set_pending(frame, pending, sizeof(*pending), drop);
  assert(aura_task_frame_capture_ownership(frame) == AURA_TASK_OWNED);
  assert(aura_task_frame_pending_ownership(frame) == AURA_TASK_TRANSFERRED);
  assert(aura_task_frame_captures(frame).data == capture);
  assert(aura_task_frame_pending(frame).data == pending);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  aura_task_frame_set_error(frame, error, sizeof(*error), drop);
  assert(aura_task_frame_error(frame).data == error);
  assert(aura_task_frame_state(frame) == AURA_TASK_FAILED);
  int *borrowed = malloc(sizeof(*borrowed));
  assert(borrowed != NULL);
  assert(!aura_task_frame_set_captures_with_ownership(
      frame, borrowed, sizeof(*borrowed), drop, AURA_TASK_BORROWED));
  free(borrowed);
  aura_task_frame_destroy(frame);
  assert(drops == 3);

  AuraTaskExecutor *executor = aura_task_executor_new();
  frame = aura_task_frame_new(0, poll_twice, NULL);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_wake(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  aura_task_executor_shutdown(executor);
  return 0;
}
