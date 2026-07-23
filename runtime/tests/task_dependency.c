#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  int polls;
  int release;
  int fail;
} ChildState;

typedef struct
{
  AuraTaskFrame *child;
  int polls;
} ParentState;

typedef struct
{
  char *message;
  int code;
} OwnedError;

static void drop_payload(void *data, size_t size)
{
  (void)size;
  free(data);
}

static void drop_owned_error(void *data, size_t size)
{
  OwnedError *error = (OwnedError *)data;
  (void)size;
  if (error != NULL)
  {
    free(error->message);
    free(error);
  }
}

static void *clone_owned_error(const void *data, size_t size,
                               size_t *cloned_size)
{
  const OwnedError *source = (const OwnedError *)data;
  OwnedError *copy;
  size_t length;
  (void)size;
  if (source == NULL || source->message == NULL || cloned_size == NULL)
  {
    return NULL;
  }
  copy = (OwnedError *)malloc(sizeof(*copy));
  if (copy == NULL)
  {
    return NULL;
  }
  length = strlen(source->message);
  copy->message = (char *)malloc(length + 1);
  if (copy->message == NULL)
  {
    free(copy);
    return NULL;
  }
  memcpy(copy->message, source->message, length + 1);
  copy->code = source->code;
  *cloned_size = sizeof(*copy);
  return copy;
}

static AuraTaskPollState poll_child(AuraTaskFrame *frame)
{
  ChildState *state = (ChildState *)aura_task_frame_data(frame);
  state->polls++;
  if (!state->release)
  {
    return AURA_TASK_PENDING;
  }
  if (state->fail)
  {
    if (state->fail == 2)
    {
      OwnedError *payload = (OwnedError *)malloc(sizeof(*payload));
      assert(payload != NULL);
      payload->message = (char *)malloc(13);
      assert(payload->message != NULL);
      memcpy(payload->message, "child-failed", 13);
      payload->code = 42;
      aura_task_frame_set_error_span_with_clone(
          frame, payload, sizeof(*payload), clone_owned_error, drop_owned_error,
          701, 130, 139);
      return AURA_TASK_FAILED;
    }
    int *payload = (int *)malloc(sizeof(*payload));
    assert(payload != NULL);
    *payload = 99;
    aura_task_frame_set_error_span(frame, payload, sizeof(*payload),
                                   drop_payload, 700, 120, 127);
    return AURA_TASK_FAILED;
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

static AuraTaskPollState poll_error_parent(AuraTaskFrame *frame)
{
  ParentState *state = (ParentState *)aura_task_frame_data(frame);
  state->polls++;
  if (aura_task_frame_resume_state(frame) == 0)
  {
    aura_task_frame_set_resume_state(frame, 1);
    assert(aura_task_frame_wait_on(frame, state->child) == 1);
    return AURA_TASK_PENDING;
  }
  assert(aura_task_frame_state(state->child) == AURA_TASK_FAILED);
  assert(aura_task_frame_propagate_error(frame, state->child) == 1);
  return AURA_TASK_FAILED;
}

static AuraTaskPollState poll_owned_error_parent(AuraTaskFrame *frame)
{
  ParentState *state = (ParentState *)aura_task_frame_data(frame);
  state->polls++;
  if (aura_task_frame_resume_state(frame) == 0)
  {
    aura_task_frame_set_resume_state(frame, 1);
    assert(aura_task_frame_wait_on(frame, state->child) == 1);
    return AURA_TASK_PENDING;
  }
  assert(aura_task_frame_state(state->child) == AURA_TASK_FAILED);
  assert(aura_task_frame_propagate_error(frame, state->child) == 1);
  return AURA_TASK_FAILED;
}

static AuraTaskPollState poll_cancel_parent(AuraTaskFrame *frame)
{
  ParentState *state = (ParentState *)aura_task_frame_data(frame);
  state->polls++;
  if (aura_task_frame_resume_state(frame) == 0)
  {
    aura_task_frame_set_resume_state(frame, 1);
    assert(aura_task_frame_wait_on(frame, state->child) == 1);
    return AURA_TASK_PENDING;
  }
  assert(aura_task_frame_state(state->child) == AURA_TASK_CANCELLED);
  return AURA_TASK_CANCELLED;
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

  /* An explicit clone callback must deep-copy nested owned payload fields so
   * child and parent can be released independently. */
  child = aura_task_frame_new(sizeof(ChildState), poll_child, NULL);
  parent = aura_task_frame_new(sizeof(ParentState), poll_owned_error_parent, NULL);
  assert(child != NULL && parent != NULL);
  child_state = (ChildState *)aura_task_frame_data(child);
  parent_state = (ParentState *)aura_task_frame_data(parent);
  parent_state->child = child;
  child_state->fail = 2;
  assert(aura_task_executor_submit(executor, child) == 1);
  assert(aura_task_executor_submit(executor, parent) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  child_state->release = 1;
  assert(aura_task_executor_wake(executor, child) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_FAILED);
  OwnedError *parent_error = (OwnedError *)aura_task_frame_error(parent).data;
  OwnedError *child_error = (OwnedError *)aura_task_frame_error(child).data;
  assert(parent_error != child_error);
  assert(strcmp(parent_error->message, "child-failed") == 0);
  assert(parent_error->code == 42);
  parent_error->message[0] = 'P';
  assert(child_error->message[0] == 'c');
  assert(aura_task_executor_release(executor, &parent) == 1);
  assert(aura_task_executor_release(executor, &child) == 1);

  /* A failed child wakes its parent and transfers an independent error
   * payload/source identity; releasing either frame remains safe. */
  child = aura_task_frame_new(sizeof(ChildState), poll_child, NULL);
  parent = aura_task_frame_new(sizeof(ParentState), poll_error_parent, NULL);
  assert(child != NULL && parent != NULL);
  child_state = (ChildState *)aura_task_frame_data(child);
  parent_state = (ParentState *)aura_task_frame_data(parent);
  parent_state->child = child;
  child_state->fail = 1;
  aura_task_frame_set_race_source_id(child, 700);
  assert(aura_task_executor_submit(executor, child) == 1);
  assert(aura_task_executor_submit(executor, parent) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  child_state->release = 1;
  assert(aura_task_executor_wake(executor, child) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(child) == AURA_TASK_FAILED);
  assert(aura_task_frame_state(parent) == AURA_TASK_READY);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_FAILED);
  assert(aura_task_frame_error_source_id(parent) == 700);
  assert(aura_task_frame_error_span_start(parent) == 120);
  assert(aura_task_frame_error_span_end(parent) == 127);
  assert(aura_task_frame_error(parent).size == sizeof(int));
  assert(*(int *)aura_task_frame_error(parent).data == 99);
  assert(aura_task_executor_release(executor, &parent) == 1);
  assert(aura_task_executor_release(executor, &child) == 1);

  /* A cancelled child is terminal dependency completion: it wakes the parent
   * once, clears both embedded dependency links, and lets the parent publish
   * the same cancellation outcome without an extra scheduler turn. */
  child = aura_task_frame_new(sizeof(ChildState), poll_child, NULL);
  parent = aura_task_frame_new(sizeof(ParentState), poll_cancel_parent, NULL);
  assert(child != NULL && parent != NULL);
  child_state = (ChildState *)aura_task_frame_data(child);
  parent_state = (ParentState *)aura_task_frame_data(parent);
  parent_state->child = child;
  assert(aura_task_executor_submit(executor, child) == 1);
  assert(aura_task_executor_submit(executor, parent) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(child) == AURA_TASK_PENDING);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_PENDING);
  assert(aura_task_frame_waiting_token(parent) == child);
  assert(aura_task_executor_cancel(executor, child) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(child) == AURA_TASK_CANCELLED);
  assert(aura_task_frame_state(parent) == AURA_TASK_READY);
  assert(aura_task_frame_waiting_token(parent) == NULL);
  assert(aura_task_executor_ready_count(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_CANCELLED);
  assert(parent_state->polls == 2);
  assert(aura_task_executor_ready_count(executor) == 0);
  assert(aura_task_executor_release(executor, &parent) == 1);
  assert(aura_task_executor_release(executor, &child) == 1);

  /* Cancellation at an await boundary detaches only the parent. The child
   * remains a valid executor-owned pending frame and is cancelled separately. */
  child = aura_task_frame_new(sizeof(ChildState), poll_child, NULL);
  parent = aura_task_frame_new(sizeof(ParentState), poll_parent, NULL);
  assert(child != NULL && parent != NULL);
  child_state = (ChildState *)aura_task_frame_data(child);
  parent_state = (ParentState *)aura_task_frame_data(parent);
  parent_state->child = child;
  assert(aura_task_executor_submit(executor, child) == 1);
  assert(aura_task_executor_submit(executor, parent) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_PENDING);
  assert(aura_task_executor_cancel(executor, parent) == 1);
  assert(aura_task_frame_waiting_token(parent) == NULL);
  assert(aura_task_frame_state(child) == AURA_TASK_PENDING);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(parent) == AURA_TASK_CANCELLED);
  assert(parent_state->polls == 1);
  assert(aura_task_executor_cancel(executor, child) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(child) == AURA_TASK_CANCELLED);
  assert(aura_task_executor_release(executor, &parent) == 1);
  assert(aura_task_executor_release(executor, &child) == 1);

  aura_task_executor_shutdown(executor);
  return 0;
}
