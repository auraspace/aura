#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static AuraTaskPollState fail_poll(AuraTaskFrame *frame)
{
  (void)frame;
  return AURA_TASK_FAILED;
}

int main(void)
{
  AuraRaceTracker *tracker = aura_race_tracker_new();
  assert(tracker != NULL);
  AuraRaceEvent read_event;
  AuraRaceEvent write_event;
  assert(aura_race_tracker_record(
      tracker, 1, (uintptr_t)0x1000, 7, AURA_RACE_READ, &read_event));
  assert(aura_race_tracker_record(
      tracker, 2, (uintptr_t)0x1000, 8, AURA_RACE_WRITE, &write_event));
  assert(read_event.sequence == 1);
  assert(write_event.sequence == 2);
  assert(read_event.address == write_event.address);
  assert(aura_race_happens_before(&read_event, &write_event));
  assert(aura_race_tracker_count(tracker) == 2);
  assert(aura_race_tracker_event(tracker, 1)->source_id == 8);
  assert(aura_race_tracker_event(tracker, 2) == NULL);

  AuraTaskExecutor *executor = aura_task_executor_new();
  AuraTaskFrame *frame = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  aura_task_executor_set_race_tracker(executor, tracker);
  aura_race_tracker_reset(tracker);
  assert(aura_task_executor_submit(executor, frame));
  assert(aura_race_tracker_count(tracker) == 1);
  assert(aura_race_tracker_event(tracker, 0)->kind == AURA_RACE_TASK_SPAWN);
  assert(aura_task_executor_run_one(executor));
  assert(aura_race_tracker_count(tracker) == 2);
  assert(aura_race_tracker_event(tracker, 1)->kind == AURA_RACE_TASK_COMPLETE);
  assert(aura_race_happens_before(aura_race_tracker_event(tracker, 0),
                                  aura_race_tracker_event(tracker, 1)));

  frame = aura_task_frame_new(0, fail_poll, NULL);
  assert(aura_task_executor_submit(executor, frame));
  assert(aura_task_executor_run_one(executor));
  assert(aura_race_tracker_event(tracker, 3)->kind == AURA_RACE_TASK_FAILED);

  frame = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(aura_task_executor_submit(executor, frame));
  assert(aura_task_executor_cancel(executor, frame));
  assert(aura_task_executor_run_one(executor));
  assert(aura_race_tracker_event(tracker, 5)->kind == AURA_RACE_TASK_CANCELLED);
  aura_task_executor_shutdown(executor);

  aura_race_tracker_reset(tracker);
  assert(aura_race_tracker_count(tracker) == 0);

  AuraTaskExecutor *ordinary_executor = aura_task_executor_new();
  AuraTaskFrame *ordinary_frame =
      aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(ordinary_executor != NULL && ordinary_frame != NULL);
  assert(aura_task_executor_submit(ordinary_executor, ordinary_frame));
  assert(aura_task_executor_run_one(ordinary_executor));
  assert(aura_race_tracker_count(tracker) == 0);
  aura_task_executor_shutdown(ordinary_executor);

  AuraTaskExecutor *join_executor = aura_task_executor_new();
  AuraTaskFrame *joined_frame =
      aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(join_executor != NULL && joined_frame != NULL);
  aura_task_executor_set_race_tracker(join_executor, tracker);
  assert(aura_task_executor_submit(join_executor, joined_frame));
  assert(aura_task_executor_run_one(join_executor));
  AuraTaskResult join_result = {NULL, 0};
  AuraTaskResult join_error = {NULL, 0};
  assert(aura_task_executor_join(join_executor, joined_frame, &join_result,
                                 &join_error) == AURA_TASK_COMPLETE);
  assert(aura_race_tracker_count(tracker) == 3);
  assert(aura_race_tracker_event(tracker, 0)->kind == AURA_RACE_TASK_SPAWN);
  assert(aura_race_tracker_event(tracker, 1)->kind == AURA_RACE_TASK_COMPLETE);
  assert(aura_race_tracker_event(tracker, 2)->kind == AURA_RACE_TASK_JOIN);
  aura_task_executor_shutdown(join_executor);

  aura_race_tracker_reset(tracker);
  AuraTaskChannel *tracked_channel = aura_task_channel_new(1);
  assert(tracked_channel != NULL);
  aura_task_channel_set_race_tracker(tracked_channel, tracker);
  AuraTaskFrame *sender = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  AuraTaskFrame *receiver = aura_task_frame_new(0, aura_task_poll_unit, NULL);
  assert(sender != NULL && receiver != NULL);
  uint64_t sender_id = aura_task_frame_task_id(sender);
  uint64_t receiver_id = aura_task_frame_task_id(receiver);
  int payload = 42;
  AuraTaskChannelValue sent = {&payload, sizeof(payload), NULL};
  AuraTaskChannelValue received = {NULL, 0, NULL};
  assert(aura_task_channel_send(tracked_channel, sender, sent) == AURA_CHANNEL_OK);
  assert(aura_task_channel_receive(tracked_channel, receiver, &received) == AURA_CHANNEL_OK);
  assert(received.data == &payload);
  assert(aura_task_channel_close_from(tracked_channel, receiver) == 1);
  assert(aura_race_tracker_count(tracker) == 3);
  assert(aura_race_tracker_event(tracker, 0)->kind == AURA_RACE_CHANNEL_SEND);
  assert(aura_race_tracker_event(tracker, 0)->task_id == sender_id);
  assert(aura_race_tracker_event(tracker, 1)->kind == AURA_RACE_CHANNEL_RECEIVE);
  assert(aura_race_tracker_event(tracker, 1)->task_id == receiver_id);
  assert(aura_race_tracker_event(tracker, 2)->kind == AURA_RACE_CHANNEL_CLOSE);
  assert(aura_race_tracker_event(tracker, 2)->task_id == receiver_id);
  assert(aura_race_tracker_event(tracker, 0)->address == (uintptr_t)tracked_channel);
  assert(aura_race_tracker_event(tracker, 0)->address ==
         aura_race_tracker_event(tracker, 1)->address);
  assert(aura_race_tracker_event(tracker, 1)->address ==
         aura_race_tracker_event(tracker, 2)->address);
  aura_task_frame_destroy(sender);
  aura_task_frame_destroy(receiver);
  aura_task_channel_destroy(tracked_channel);

  AuraTaskChannel *untracked_channel = aura_task_channel_new(1);
  assert(untracked_channel != NULL);
  assert(aura_task_channel_send(untracked_channel, NULL, sent) == AURA_CHANNEL_OK);
  assert(aura_task_channel_receive(untracked_channel, NULL, &received) == AURA_CHANNEL_OK);
  assert(aura_task_channel_close(untracked_channel) == 1);
  assert(aura_race_tracker_count(tracker) == 3);
  aura_task_channel_destroy(untracked_channel);

  aura_race_tracker_destroy(tracker);
  return 0;
}
