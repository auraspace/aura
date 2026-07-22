#include <assert.h>
#include <stdint.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static size_t frame_destroyed;
static size_t result_destroyed;
static size_t payload_destroyed;

static void destroy_frame(AuraTaskFrame *frame)
{
  (void)frame;
  frame_destroyed++;
}

static void destroy_result(void *data, size_t size)
{
  assert(size == sizeof(uint64_t));
  result_destroyed++;
  free(data);
}

static void destroy_payload(void *data, size_t size)
{
  assert(size == sizeof(uint64_t));
  payload_destroyed++;
  free(data);
}

static AuraTaskPollState complete_task(AuraTaskFrame *frame)
{
  uint64_t *result = (uint64_t *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = 22;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_result);
  return AURA_TASK_COMPLETE;
}

typedef struct
{
  AuraTaskChannel *channel;
  AuraTaskChannelValue value;
} SenderState;

static AuraTaskPollState send_until_woken(AuraTaskFrame *frame)
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
  if (status == AURA_CHANNEL_OK || status == AURA_CHANNEL_CLOSED)
  {
    state->value = (AuraTaskChannelValue){NULL, 0, NULL};
  }
  return status == AURA_CHANNEL_ERROR ? AURA_TASK_FAILED : AURA_TASK_COMPLETE;
}

typedef struct
{
  AuraTaskChannel *channel;
} ReceiverState;

static AuraTaskPollState receive_until_closed(AuraTaskFrame *frame)
{
  ReceiverState *state = (ReceiverState *)aura_task_frame_data(frame);
  AuraTaskChannelValue out = {NULL, 0, NULL};
  AuraTaskChannelStatus status = aura_task_channel_receive(state->channel, frame, &out);
  if (status == AURA_CHANNEL_OK)
  {
    aura_task_channel_value_destroy(&out);
    return AURA_TASK_COMPLETE;
  }
  return status == AURA_CHANNEL_PENDING ? AURA_TASK_PENDING : AURA_TASK_COMPLETE;
}

static AuraTaskChannelValue payload(uint64_t value)
{
  uint64_t *data = (uint64_t *)malloc(sizeof(*data));
  assert(data != NULL);
  *data = value;
  return (AuraTaskChannelValue){data, sizeof(*data), destroy_payload};
}

static AuraTaskFrame *new_sender(AuraTaskChannel *channel, uint64_t value)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(SenderState), send_until_woken, NULL);
  assert(frame != NULL);
  SenderState *state = (SenderState *)aura_task_frame_data(frame);
  state->channel = channel;
  state->value = payload(value);
  return frame;
}

int main(void)
{
  for (size_t i = 0; i < 1000; i++)
  {
    AuraTaskFrame *frame = aura_task_frame_new(32, complete_task, destroy_frame);
    assert(frame != NULL);
    assert(aura_task_frame_data(frame) != NULL);
    uint64_t *result = (uint64_t *)malloc(sizeof(*result));
    assert(result != NULL);
    *result = i;
    aura_task_frame_set_result(frame, result, sizeof(*result), destroy_result);
    aura_task_frame_destroy(frame);
  }
  assert(frame_destroyed == 1000);
  assert(result_destroyed == 1000);

  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  for (size_t i = 0; i < 1000; i++)
  {
    AuraTaskFrame *frame = aura_task_frame_new(0, complete_task, destroy_frame);
    assert(frame != NULL);
    assert(aura_task_executor_submit(executor, frame) == 1);
  }
  assert(aura_task_executor_run(executor) == 1000);
  assert(aura_task_executor_ready_count(executor) == 0);
  assert(aura_task_executor_task_count(executor) == 1000);

  for (size_t i = 0; i < 1000; i++)
  {
    AuraTaskChannel *channel = aura_task_channel_new(1);
    assert(channel != NULL);
    assert(aura_task_channel_send(channel, NULL, payload(i)) == AURA_CHANNEL_OK);
    AuraTaskFrame *sender = new_sender(channel, i + 1000);
    assert(aura_task_executor_submit(executor, sender) == 1);
    assert(aura_task_executor_run_one(executor) == 1);
    assert(aura_task_frame_state(sender) == AURA_TASK_PENDING);
    assert(aura_task_executor_cancel(executor, sender) == 1);
    assert(aura_task_executor_run_one(executor) == 1);
    assert(aura_task_frame_state(sender) == AURA_TASK_CANCELLED);
    assert(aura_task_channel_close(channel) == 1);
    aura_task_channel_destroy(channel);
  }
  assert(payload_destroyed == 2000);

  for (size_t i = 0; i < 1000; i++)
  {
    AuraTaskChannel *channel = aura_task_channel_new(1);
    assert(channel != NULL);
    AuraTaskFrame *receiver = aura_task_frame_new(sizeof(ReceiverState), receive_until_closed, NULL);
    assert(receiver != NULL);
    ((ReceiverState *)aura_task_frame_data(receiver))->channel = channel;
    assert(aura_task_executor_submit(executor, receiver) == 1);
    assert(aura_task_executor_run_one(executor) == 1);
    assert(aura_task_frame_state(receiver) == AURA_TASK_PENDING);
    assert(aura_task_channel_close(channel) == 1);
    assert(aura_task_executor_run_one(executor) == 1);
    assert(aura_task_frame_state(receiver) == AURA_TASK_COMPLETE);
    aura_task_channel_destroy(channel);
  }

  aura_task_executor_shutdown(executor);
  assert(frame_destroyed == 2000);
  assert(result_destroyed == 2000);
  return 0;
}
