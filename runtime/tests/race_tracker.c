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
  aura_race_tracker_destroy(tracker);
  return 0;
}
