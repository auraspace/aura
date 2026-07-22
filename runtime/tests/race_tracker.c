#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

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

  aura_race_tracker_reset(tracker);
  assert(aura_race_tracker_count(tracker) == 0);
  aura_race_tracker_destroy(tracker);
  return 0;
}
