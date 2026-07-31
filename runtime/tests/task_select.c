#include <assert.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static void destroy_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

static AuraTaskChannelValue int_value(int value)
{
  int *data = (int *)malloc(sizeof(*data));
  assert(data != NULL);
  *data = value;
  return (AuraTaskChannelValue){data, sizeof(*data), destroy_int};
}

typedef struct
{
  AuraTaskSelect *select;
  AuraTaskChannelValue value;
  size_t index;
  AuraTaskChannelStatus status;
} SelectState;

static AuraTaskPollState poll_select(AuraTaskFrame *frame)
{
  SelectState *state = (SelectState *)aura_task_frame_data(frame);
  state->status = aura_task_select_next(state->select, frame, &state->value, &state->index);
  if (state->status == AURA_CHANNEL_PENDING) return AURA_TASK_PENDING;
  return state->status == AURA_CHANNEL_ERROR ? AURA_TASK_FAILED : AURA_TASK_COMPLETE;
}

int main(void)
{
  AuraTaskChannel *left = aura_task_channel_new(1);
  AuraTaskChannel *right = aura_task_channel_new(1);
  AuraTaskSelect *select = aura_task_select_new();
  assert(left != NULL && right != NULL && select != NULL);
  assert(aura_task_select_add(select, left) == 1);
  assert(aura_task_select_add(select, right) == 1);

  assert(aura_task_channel_send(right, NULL, int_value(20)) == AURA_CHANNEL_OK);
  AuraTaskChannelValue out = {NULL, 0, NULL};
  size_t index = 99;
  assert(aura_task_select_next(select, NULL, &out, &index) == AURA_CHANNEL_OK);
  assert(index == 1 && *(int *)out.data == 20);
  aura_task_channel_value_destroy(&out);

  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(SelectState), poll_select, NULL);
  assert(frame != NULL);
  SelectState *state = (SelectState *)aura_task_frame_data(frame);
  *state = (SelectState){select, {NULL, 0, NULL}, 0, AURA_CHANNEL_ERROR};
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_channel_send(left, NULL, int_value(10)) == AURA_CHANNEL_OK);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(state->status == AURA_CHANNEL_OK && state->index == 0);
  assert(*(int *)state->value.data == 10);
  aura_task_channel_value_destroy(&state->value);

  AuraTaskFrame *closed_frame = aura_task_frame_new(sizeof(SelectState), poll_select, NULL);
  assert(closed_frame != NULL);
  SelectState *closed_state = (SelectState *)aura_task_frame_data(closed_frame);
  *closed_state = (SelectState){select, {NULL, 0, NULL}, 0, AURA_CHANNEL_ERROR};
  assert(aura_task_executor_submit(executor, closed_frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(closed_frame) == AURA_TASK_PENDING);
  assert(aura_task_channel_close(right) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(closed_frame) == AURA_TASK_COMPLETE);
  assert(closed_state->status == AURA_CHANNEL_CLOSED && closed_state->index == 1);

  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(aura_task_executor_release(executor, &closed_frame) == 1);
  aura_task_select_destroy(select);
  aura_task_channel_destroy(left);
  aura_task_channel_destroy(right);
  aura_task_executor_shutdown(executor);
  return 0;
}
