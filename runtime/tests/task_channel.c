#include <assert.h>
#include <stdlib.h>

#include "../../runtime/aura_rt.c"

static int destroyed;

static void destroy_value(void *data, size_t size)
{
  assert(size == sizeof(int));
  destroyed++;
  free(data);
}

static AuraTaskChannelValue value(int n)
{
  int *data = (int *)malloc(sizeof(*data));
  assert(data != NULL);
  *data = n;
  AuraTaskChannelValue result = {data, sizeof(*data), destroy_value};
  return result;
}

typedef struct
{
  AuraTaskChannel *channel;
  AuraTaskChannelValue *out;
} ReceiverState;

static AuraTaskPollState receive_once(AuraTaskFrame *frame)
{
  ReceiverState *state = (ReceiverState *)aura_task_frame_data(frame);
  AuraTaskChannelStatus status = aura_task_channel_receive(state->channel, frame, state->out);
  return status == AURA_CHANNEL_PENDING ? AURA_TASK_PENDING : AURA_TASK_COMPLETE;
}

typedef struct
{
  AuraTaskChannel *channel;
  AuraTaskChannelValue value;
} SenderState;

static AuraTaskPollState send_once(AuraTaskFrame *frame)
{
  SenderState *state = (SenderState *)aura_task_frame_data(frame);
  if (state->value.data == NULL)
  {
    return AURA_TASK_COMPLETE;
  }
  AuraTaskChannelStatus status = aura_task_channel_send(state->channel, frame, state->value);
  if (status == AURA_CHANNEL_PENDING)
  {
    state->value = (AuraTaskChannelValue){NULL, 0, NULL};
    return AURA_TASK_PENDING;
  }
  if (status == AURA_CHANNEL_OK)
  {
    state->value = (AuraTaskChannelValue){NULL, 0, NULL};
  }
  return AURA_TASK_COMPLETE;
}

static AuraTaskFrame *new_receiver(AuraTaskChannel *channel, AuraTaskChannelValue *out)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(ReceiverState), receive_once, NULL);
  assert(frame != NULL);
  ReceiverState *state = (ReceiverState *)aura_task_frame_data(frame);
  state->channel = channel;
  state->out = out;
  return frame;
}

static AuraTaskFrame *new_sender(AuraTaskChannel *channel, AuraTaskChannelValue payload)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(SenderState), send_once, NULL);
  assert(frame != NULL);
  SenderState *state = (SenderState *)aura_task_frame_data(frame);
  state->channel = channel;
  state->value = payload;
  return frame;
}

int main(void)
{
  assert(aura_task_channel_new(0) == NULL);
  AuraTaskChannel *channel = aura_task_channel_new(2);
  assert(channel != NULL);
  assert(aura_task_channel_capacity(channel) == 2);

  assert(aura_task_channel_send(channel, NULL, value(10)) == AURA_CHANNEL_OK);
  assert(aura_task_channel_send(channel, NULL, value(20)) == AURA_CHANNEL_OK);
  assert(aura_task_channel_count(channel) == 2);
  AuraTaskChannelValue out = {NULL, 0, NULL};
  assert(aura_task_channel_receive(channel, NULL, &out) == AURA_CHANNEL_OK);
  assert(*(int *)out.data == 10);
  aura_task_channel_value_destroy(&out);
  assert(aura_task_channel_receive(channel, NULL, &out) == AURA_CHANNEL_OK);
  assert(*(int *)out.data == 20);
  aura_task_channel_value_destroy(&out);

  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *receiver = new_receiver(channel, &out);
  assert(aura_task_executor_submit(executor, receiver) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(receiver) == AURA_TASK_PENDING);
  assert(aura_task_channel_send(channel, NULL, value(30)) == AURA_CHANNEL_OK);
  assert(aura_task_executor_ready_count(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(*(int *)out.data == 30);
  aura_task_channel_value_destroy(&out);

  AuraTaskChannel *full = aura_task_channel_new(2);
  assert(full != NULL);
  assert(aura_task_channel_send(full, NULL, value(40)) == AURA_CHANNEL_OK);
  assert(aura_task_channel_send(full, NULL, value(50)) == AURA_CHANNEL_OK);
  AuraTaskFrame *sender = new_sender(full, value(60));
  assert(aura_task_executor_submit(executor, sender) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(sender) == AURA_TASK_PENDING);
  assert(aura_task_channel_receive(full, NULL, &out) == AURA_CHANNEL_OK);
  aura_task_channel_value_destroy(&out);
  assert(aura_task_executor_ready_count(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(sender) == AURA_TASK_COMPLETE);

  AuraTaskFrame *cancelled = new_sender(full, value(70));
  assert(aura_task_executor_submit(executor, cancelled) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(cancelled) == AURA_TASK_PENDING);
  assert(aura_task_executor_cancel(executor, cancelled) == 1);
  assert(destroyed == 5);

  assert(aura_task_channel_close(full) == 1);
  assert(aura_task_channel_close(full) == 0);
  assert(aura_task_channel_receive(full, NULL, &out) == AURA_CHANNEL_OK);
  assert(*(int *)out.data == 50);
  aura_task_channel_value_destroy(&out);
  assert(aura_task_channel_receive(full, NULL, &out) == AURA_CHANNEL_OK);
  assert(*(int *)out.data == 60);
  aura_task_channel_value_destroy(&out);
  assert(aura_task_channel_receive(full, NULL, &out) == AURA_CHANNEL_CLOSED);
  assert(aura_task_channel_send(full, NULL, value(80)) == AURA_CHANNEL_CLOSED);
  aura_task_channel_destroy(full);
  aura_task_channel_destroy(channel);
  aura_task_executor_shutdown(executor);
  assert(destroyed == 8);
  return 0;
}
