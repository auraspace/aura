#include <assert.h>

#include "../../runtime/aura_ffi.h"

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(frame != NULL);
  assert(aura_task_executor_submit(executor, frame) == 1);

  AuraTaskChannel *channel = aura_task_channel_new(1);
  assert(channel != NULL);
  AuraTaskChannelValue value =
      aura_task_channel_value_from_task(executor, frame);
  assert(value.data != NULL);
  assert(aura_task_channel_send(channel, NULL, value) == AURA_CHANNEL_OK);

  AuraTaskChannelValue received = {0};
  assert(aura_task_channel_receive(channel, NULL, &received) == AURA_CHANNEL_OK);
  AuraTaskFrame *transferred =
      aura_task_channel_value_take_task(received.data, received.size);
  assert(transferred == frame);
  received.data = NULL;

  aura_task_channel_destroy(channel);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(frame == NULL);
  assert(aura_task_executor_run(executor) == 1);
  assert(aura_task_executor_release(executor, &transferred) == 1);
  assert(transferred == NULL);
  aura_task_executor_shutdown(executor);
  return 0;
}
