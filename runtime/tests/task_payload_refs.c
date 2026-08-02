#include <assert.h>

#include "../../runtime/aura_ffi.h"
#include "../../runtime/runtime.c"

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);

  AuraTaskFrame *task = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(task != NULL);
  assert(aura_task_executor_submit(executor, task) == 1);
  AuraTaskFrame *foreign_payload = task;
  assert(aura_task_executor_retain_payload(executor, foreign_payload) == 1);
  assert(aura_task_executor_release_payload(executor, &foreign_payload) == 1);
  assert(foreign_payload == NULL);

  AuraTaskChannel *channel = aura_task_channel_new(1);
  assert(channel != NULL);
  AuraTaskChannelValue value =
      aura_task_channel_value_from_task(executor, task);
  assert(value.data != NULL);
  assert(aura_task_channel_send(channel, NULL, value) == AURA_CHANNEL_OK);

  AuraTaskChannelValue received = {0};
  assert(aura_task_channel_receive(channel, NULL, &received) == AURA_CHANNEL_OK);
  AuraTaskFrame *received_task =
      aura_task_channel_value_take_task(received.data, received.size);
  assert(received_task == task);
  received.data = NULL;

  /* Closing the lexical channel owner must not invalidate the transferred
   * channel value or its frame payload. */
  aura_task_channel_destroy(channel);
  assert(aura_task_executor_release(executor, &task) == 1);
  assert(task == NULL);

  (void)aura_task_executor_run(executor);
  assert(aura_task_executor_release(executor, &received_task) == 1);
  assert(received_task == NULL);

  aura_task_executor_shutdown(executor);

  /* A queued payload keeps its frame alive after scheduler shutdown. */
  executor = aura_task_executor_new();
  assert(executor != NULL);
  task = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(task != NULL);
  assert(aura_task_executor_submit(executor, task) == 1);
  channel = aura_task_channel_new(1);
  assert(channel != NULL);
  value = aura_task_channel_value_from_task(executor, task);
  assert(value.data != NULL);
  assert(aura_task_channel_send(channel, NULL, value) == AURA_CHANNEL_OK);
  aura_task_executor_shutdown(executor);
  aura_task_channel_destroy(channel);

  return 0;
}
