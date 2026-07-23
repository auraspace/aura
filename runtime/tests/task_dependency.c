#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  int polls;
  int release;
} ChildState;

typedef struct
{
  AuraTaskFrame *child;
  int polls;
} ParentState;

static AuraTaskPollState poll_child(AuraTaskFrame *frame)
{
  ChildState *state = (ChildState *)aura_task_frame_data(frame);
  state->polls++;
  if (!state->release)
  {
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_COMPLETE;
}

static AuraTaskPollState poll_parent(AuraTaskFrame *frame)
{
  ParentState *state = (ParentState *)aura_task_frame_data(frame);
  state->polls++;
  if (aura_task_frame_resume_state(frame) == 0)
  {
    aura_task_frame_set_resume_state(frame, 1);
    assert(aura_task_frame_wait_on(frame, state->child) == 1);
    return AURA_TASK_PENDING;
  }
  assert(aura_task_frame_state(state->child) == AURA_TASK_COMPLETE);
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  AuraTaskFrame *child = aura_task_frame_new(sizeof(ChildState), poll_child, NULL);
  AuraTaskFrame *parent = aura_task_frame_new(sizeof(ParentState), poll_parent, NULL);
  ChildState *child_state;
  ParentState *parent_state;

  assert(executor != NULL && child != NULL && parent != NULL);
  child_state = (ChildState *)aura_task_frame_data(child);
  parent_state = (ParentState *)aura_task_frame_data(parent);
  parent_state->child = child;
  assert(aura_task_executor_submit(executor, child) == 1);
  assert(aura_task_executor_submit(executor, parent) == 1);

  /* The child parks first; the parent then registers a dependency and parks. */
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(child) == AURA_TASK_PENDING);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_PENDING);
  assert(aura_task_frame_waiting_token(parent) == child);

  child_state->release = 1;
  assert(aura_task_executor_wake(executor, child) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(child) == AURA_TASK_COMPLETE);
  assert(aura_task_frame_state(parent) == AURA_TASK_READY);
  assert(aura_task_frame_waiting_token(parent) == NULL);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_COMPLETE);
  assert(child_state->polls == 2);
  assert(parent_state->polls == 2);

  assert(aura_task_executor_release(executor, &parent) == 1);
  assert(aura_task_executor_release(executor, &child) == 1);
  aura_task_executor_shutdown(executor);
  return 0;
}
