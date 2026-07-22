#include <assert.h>
#include <stdio.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static void assert_report_text(const AuraRaceReport *report)
{
  FILE *human = tmpfile();
  FILE *json = tmpfile();
  assert(human != NULL && json != NULL);
  assert(aura_race_report_write_human(report, human));
  assert(aura_race_report_write_json(report, json));
  char line[1024];
  rewind(human);
  assert(fgets(line, sizeof(line), human) != NULL);
  assert(strstr(line, "missing synchronization") != NULL);
  assert(strstr(line, "task=1") != NULL);
  assert(strstr(line, "task=2") != NULL);
  rewind(json);
  assert(fgets(line, sizeof(line), json) != NULL);
  assert(strstr(line, "\"identity\"") != NULL);
  assert(strstr(line, "\"missing_synchronization\"") != NULL);
  fclose(human);
  fclose(json);
}

int main(void)
{
  AuraRaceTracker *tracker = aura_race_tracker_new();
  assert(tracker != NULL);
  AuraRaceEvent event;
  assert(aura_race_tracker_record(tracker, 1, (uintptr_t)0x2000, 101,
                                  AURA_RACE_WRITE, &event));
  assert(aura_race_tracker_record(tracker, 2, (uintptr_t)0x2000, 202,
                                  AURA_RACE_READ, &event));
  assert(aura_race_tracker_report_count(tracker) == 1);
  AuraRaceReport report;
  assert(aura_race_tracker_report(tracker, 0, &report));
  assert(report.first.task_id == 1 || report.second.task_id == 1);
  assert(report.first.source_id == 101 || report.second.source_id == 101);
  assert(report.missing_synchronization != NULL);
  assert_report_text(&report);

  assert(aura_race_tracker_record(tracker, 1, (uintptr_t)0x2000, 101,
                                  AURA_RACE_WRITE, &event));
  assert(aura_race_tracker_record(tracker, 2, (uintptr_t)0x2000, 202,
                                  AURA_RACE_READ, &event));
  assert(aura_race_tracker_report_count(tracker) == 1);

  aura_race_tracker_reset(tracker);
  assert(aura_race_tracker_record(tracker, 1, (uintptr_t)0x2000, 101,
                                  AURA_RACE_WRITE, &event));
  assert(aura_race_tracker_record(tracker, 1, (uintptr_t)0, 303,
                                  AURA_RACE_TASK_JOIN, &event));
  assert(aura_race_tracker_record(tracker, 2, (uintptr_t)0x2000, 202,
                                  AURA_RACE_READ, &event));
  assert(aura_race_tracker_report_count(tracker) == 0);

  aura_race_tracker_reset(tracker);
  assert(aura_race_tracker_record(tracker, 1, (uintptr_t)0x2000, 101,
                                  AURA_RACE_WRITE, &event));
  assert(aura_race_tracker_record(tracker, 1, (uintptr_t)0x3000, 304,
                                  AURA_RACE_CHANNEL_SEND, &event));
  assert(aura_race_tracker_record(tracker, 2, (uintptr_t)0x3000, 305,
                                  AURA_RACE_CHANNEL_RECEIVE, &event));
  assert(aura_race_tracker_record(tracker, 2, (uintptr_t)0x2000, 202,
                                  AURA_RACE_READ, &event));
  assert(aura_race_tracker_report_count(tracker) == 0);
  aura_race_tracker_destroy(tracker);
  return 0;
}
