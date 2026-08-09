/* ---- C22j task-frame ABI (single-threaded MVP) ----
 *
 * A task frame is an opaque, heap-owned state machine object.  The poll
 * callback owns the state transition; it may retain frame_data across a
 * pending return.  A frame owns its result payload and invokes result_destroy
 * exactly once when the frame is destroyed.  The optional frame_destroy hook
 * runs before frame_data is freed and is the place for state-machine-specific
 * cleanup.  The context pointer is borrowed by the runtime and is never
 * freed.
 *
 * This ABI deliberately has no scheduler or channel dependency.  C22k adds
 * the executor that drives these callbacks.
 */

uint32_t aura_runtime_abi_version(void)
{
  return AURA_RT_ABI_VERSION;
}

const char *aura_runtime_abi_identity(void)
{
  return AURA_RT_ABI_ID;
}

int aura_runtime_check_abi(uint32_t expected_version, const char *expected_identity)
{
  const char *available = aura_runtime_abi_identity();
  if (expected_version == aura_runtime_abi_version() &&
      expected_identity != NULL && strcmp(expected_identity, available) == 0)
  {
    return 1;
  }
  fprintf(stderr,
          "aura: runtime ABI mismatch: expected version %u identity %s, available version %u identity %s\n",
          expected_version,
          expected_identity ? expected_identity : "(missing)",
          aura_runtime_abi_version(),
          available);
  return 0;
}

/* ---- R1/R2 deterministic race event model ----
 *
 * The current executor is single-threaded, so this tracker records the total
 * order that a future concurrent detector will refine into vector clocks.
 * Every event carries task, address, and source identity for stable reports.
 */
typedef enum
{
  AURA_RACE_READ = 0,
  AURA_RACE_WRITE = 1,
  AURA_RACE_TASK_SPAWN = 2,
  AURA_RACE_TASK_JOIN = 3,
  AURA_RACE_SYNC_ACQUIRE = 4,
  AURA_RACE_SYNC_RELEASE = 5,
  AURA_RACE_TASK_COMPLETE = 6,
  AURA_RACE_TASK_FAILED = 7,
  AURA_RACE_TASK_CANCELLED = 8,
  AURA_RACE_CHANNEL_SEND = 9,
  AURA_RACE_CHANNEL_RECEIVE = 10,
  AURA_RACE_CHANNEL_CLOSE = 11
} AuraRaceEventKind;

typedef struct
{
  uint64_t sequence;
  uint64_t task_id;
  uint64_t stack_id;
  uintptr_t address;
  uint32_t source_id;
  AuraRaceEventKind kind;
} AuraRaceEvent;

typedef struct
{
  uint64_t identity;
  AuraRaceEvent first;
  AuraRaceEvent second;
  const char *missing_synchronization;
} AuraRaceReport;

typedef struct
{
  AuraRaceEvent *events;
  size_t count;
  size_t capacity;
  uint64_t clock;
} AuraRaceTracker;

/* R3 compiler instrumentation is deliberately process-local and opt-in.
 * Generated development/test binaries install a tracker here; ordinary
 * binaries leave it NULL, so the instrumentation helpers are no-ops. */
static AuraRaceTracker *aura_race_active_tracker = NULL;
static uint64_t aura_race_active_task_id = 0;
static uint32_t aura_race_active_source_id = 0;
static uint64_t aura_race_active_stack_id = 0;

static int aura_race_tracker_record_internal(AuraRaceTracker *tracker,
                                             uint64_t task_id,
                                             uintptr_t address,
                                             uint32_t source_id,
                                             AuraRaceEventKind kind);

void aura_race_tracker_set_active(AuraRaceTracker *tracker)
{
  aura_race_active_tracker = tracker;
  aura_race_active_task_id = 0;
  aura_race_active_source_id = 0;
  aura_race_active_stack_id = 0;
}

void aura_race_set_source_id(uint32_t source_id)
{
  aura_race_active_source_id = source_id;
}

void aura_race_set_stack_id(uint64_t stack_id)
{
  aura_race_active_stack_id = stack_id;
}

void aura_race_record_access(uintptr_t address,
                             uint32_t source_id,
                             AuraRaceEventKind kind)
{
  if (aura_race_active_tracker == NULL ||
      (kind != AURA_RACE_READ && kind != AURA_RACE_WRITE))
  {
    return;
  }
  (void)aura_race_tracker_record_internal(aura_race_active_tracker,
                                          aura_race_active_task_id,
                                          address,
                                          source_id,
                                          kind);
}

AuraRaceTracker *aura_race_tracker_new(void)
{
  AuraRaceTracker *tracker = (AuraRaceTracker *)calloc(1, sizeof(*tracker));
  if (tracker == NULL)
  {
    return NULL;
  }
  tracker->capacity = 16;
  tracker->events = (AuraRaceEvent *)calloc(tracker->capacity, sizeof(*tracker->events));
  if (tracker->events == NULL)
  {
    free(tracker);
    return NULL;
  }
  return tracker;
}

void aura_race_tracker_destroy(AuraRaceTracker *tracker)
{
  if (tracker != NULL)
  {
    free(tracker->events);
    free(tracker);
  }
}

void aura_race_tracker_reset(AuraRaceTracker *tracker)
{
  if (tracker != NULL)
  {
    tracker->count = 0;
    tracker->clock = 0;
  }
}

int aura_race_tracker_record(AuraRaceTracker *tracker,
                             uint64_t task_id,
                             uintptr_t address,
                             uint32_t source_id,
                             AuraRaceEventKind kind,
                             AuraRaceEvent *out)
{
  if (tracker == NULL)
  {
    return 0;
  }
  if (tracker->count == tracker->capacity)
  {
    size_t next_capacity = tracker->capacity * 2;
    AuraRaceEvent *next = (AuraRaceEvent *)realloc(
        tracker->events, next_capacity * sizeof(*tracker->events));
    if (next == NULL)
    {
      return 0;
    }
    tracker->events = next;
    tracker->capacity = next_capacity;
  }
  AuraRaceEvent event = {++tracker->clock, task_id, 0, address, source_id, kind};
  tracker->events[tracker->count++] = event;
  if (out != NULL)
  {
    *out = event;
  }
  return 1;
}

static uint64_t aura_race_hash_u64(uint64_t hash, uint64_t value)
{
  for (unsigned int shift = 0; shift < 64; shift += 8)
  {
    hash ^= (value >> shift) & UINT64_C(0xff);
    hash *= UINT64_C(1099511628211);
  }
  return hash;
}

static int aura_race_is_access(const AuraRaceEvent *event)
{
  return event != NULL &&
         (event->kind == AURA_RACE_READ || event->kind == AURA_RACE_WRITE);
}

static int aura_race_is_conflicting(const AuraRaceEvent *first,
                                    const AuraRaceEvent *second)
{
  return aura_race_is_access(first) && aura_race_is_access(second) &&
         first->address == second->address && first->task_id != second->task_id &&
         (first->kind == AURA_RACE_WRITE || second->kind == AURA_RACE_WRITE);
}

/* Alpha's bounded synchronization model: an observed join, lock hand-off, or
 * channel hand-off between the two accesses is a sufficient edge.  The
 * executor is deterministic, so this deliberately avoids wall-clock state. */
static const char *aura_race_missing_sync(const AuraRaceTracker *tracker,
                                          size_t first,
                                          size_t second)
{
  int saw_release = 0;
  int saw_send = 0;
  for (size_t i = first + 1; i < second; ++i)
  {
    const AuraRaceEvent *event = &tracker->events[i];
    if (event->kind == AURA_RACE_TASK_JOIN)
    {
      return NULL;
    }
    if (event->kind == AURA_RACE_SYNC_RELEASE && event->address != 0)
    {
      saw_release = 1;
    }
    else if (event->kind == AURA_RACE_SYNC_ACQUIRE && saw_release &&
             event->address != 0)
    {
      return NULL;
    }
    else if (event->kind == AURA_RACE_CHANNEL_SEND && event->address != 0)
    {
      saw_send = 1;
    }
    else if (event->kind == AURA_RACE_CHANNEL_RECEIVE && saw_send &&
             event->address != 0)
    {
      return NULL;
    }
  }
  return "no join, lock hand-off, or channel hand-off was observed";
}

static uint64_t aura_race_report_identity(const AuraRaceEvent *first,
                                          const AuraRaceEvent *second)
{
  /* Do not hash sequence numbers or raw addresses: both are run-local. */
  uint64_t a = first->source_id;
  uint64_t b = second->source_id;
  uint64_t sa = first->stack_id;
  uint64_t sb = second->stack_id;
  AuraRaceEventKind ka = first->kind;
  AuraRaceEventKind kb = second->kind;
  if (a > b || (a == b && (sa > sb || (sa == sb && ka > kb))))
  {
    uint64_t tmp = a; a = b; b = tmp;
    tmp = sa; sa = sb; sb = tmp;
    AuraRaceEventKind kt = ka; ka = kb; kb = kt;
  }
  uint64_t hash = UINT64_C(1469598103934665603);
  hash = aura_race_hash_u64(hash, UINT64_C(1));
  hash = aura_race_hash_u64(hash, a);
  hash = aura_race_hash_u64(hash, b);
  hash = aura_race_hash_u64(hash, sa);
  hash = aura_race_hash_u64(hash, sb);
  hash = aura_race_hash_u64(hash, (uint64_t)ka);
  return aura_race_hash_u64(hash, (uint64_t)kb);
}

static int aura_race_report_candidate(const AuraRaceTracker *tracker,
                                      size_t wanted,
                                      AuraRaceReport *out)
{
  uint64_t best = UINT64_MAX;
  size_t best_first = 0;
  size_t best_second = 0;
  for (size_t i = 0; i < tracker->count; ++i)
  {
    for (size_t j = i + 1; j < tracker->count; ++j)
    {
      if (!aura_race_is_conflicting(&tracker->events[i], &tracker->events[j]) ||
          aura_race_missing_sync(tracker, i, j) == NULL)
      {
        continue;
      }
      uint64_t identity = aura_race_report_identity(&tracker->events[i],
                                                    &tracker->events[j]);
      int duplicate = 0;
      for (size_t p = 0; p < i && !duplicate; ++p)
      {
        for (size_t q = p + 1; q < tracker->count; ++q)
        {
          if (aura_race_is_conflicting(&tracker->events[p], &tracker->events[q]) &&
              aura_race_missing_sync(tracker, p, q) != NULL &&
              aura_race_report_identity(&tracker->events[p], &tracker->events[q]) == identity)
          {
            duplicate = 1;
            break;
          }
        }
      }
      if (duplicate)
      {
        continue;
      }
      if (identity < best)
      {
        best = identity;
        best_first = i;
        best_second = j;
      }
    }
  }
  if (best == UINT64_MAX)
  {
    return 0;
  }
  /* Select the wanted item by repeatedly masking the chosen identity. */
  if (wanted != 0)
  {
    AuraRaceTracker copy = *tracker;
    (void)copy;
    /* The public alpha API is intentionally bounded to the first report;
     * callers use report_count to observe whether any conflict exists. */
    return 0;
  }
  out->identity = best;
  out->first = tracker->events[best_first];
  out->second = tracker->events[best_second];
  out->missing_synchronization = aura_race_missing_sync(tracker, best_first, best_second);
  return 1;
}

size_t aura_race_tracker_report_count(const AuraRaceTracker *tracker)
{
  AuraRaceReport report;
  return tracker != NULL && aura_race_report_candidate(tracker, 0, &report) ? 1 : 0;
}

int aura_race_tracker_report(const AuraRaceTracker *tracker,
                             size_t index,
                             AuraRaceReport *out)
{
  if (tracker == NULL || out == NULL || index != 0)
  {
    return 0;
  }
  return aura_race_report_candidate(tracker, index, out);
}

static const char *aura_race_kind_name(AuraRaceEventKind kind)
{
  return kind == AURA_RACE_READ ? "read" : "write";
}

int aura_race_report_write_human(const AuraRaceReport *report, FILE *out)
{
  if (report == NULL || out == NULL)
  {
    return 0;
  }
  return fprintf(out,
                 "race[%016" PRIx64 "] %s(task=%" PRIu64 ",stack=%" PRIu64 ",source=%" PRIu32 ") <-> %s(task=%" PRIu64 ",stack=%" PRIu64 ",source=%" PRIu32 "); missing synchronization: %s\n",
                 report->identity, aura_race_kind_name(report->first.kind),
                 report->first.task_id, report->first.stack_id, report->first.source_id,
                 aura_race_kind_name(report->second.kind), report->second.task_id,
                 report->second.stack_id, report->second.source_id,
                 report->missing_synchronization) >= 0;
}

int aura_race_report_write_json(const AuraRaceReport *report, FILE *out)
{
  if (report == NULL || out == NULL)
  {
    return 0;
  }
  return fprintf(out,
                 "{\"identity\":\"%016" PRIx64 "\",\"first\":{\"kind\":\"%s\",\"task\":%" PRIu64 ",\"stack\":%" PRIu64 ",\"source\":%" PRIu32 "},\"second\":{\"kind\":\"%s\",\"task\":%" PRIu64 ",\"stack\":%" PRIu64 ",\"source\":%" PRIu32 "},\"missing_synchronization\":\"%s\"}\n",
                 report->identity, aura_race_kind_name(report->first.kind),
                 report->first.task_id, report->first.stack_id, report->first.source_id,
                 aura_race_kind_name(report->second.kind), report->second.task_id,
                 report->second.stack_id, report->second.source_id,
                 report->missing_synchronization) >= 0;
}

static int aura_race_tracker_record_internal(AuraRaceTracker *tracker,
                                              uint64_t task_id,
                                              uintptr_t address,
                                              uint32_t source_id,
                                              AuraRaceEventKind kind)
{
  return aura_race_tracker_record(tracker, task_id, address, source_id, kind, NULL);
}

size_t aura_race_tracker_count(const AuraRaceTracker *tracker)
{
  return tracker != NULL ? tracker->count : 0;
}

const AuraRaceEvent *aura_race_tracker_event(const AuraRaceTracker *tracker, size_t index)
{
  if (tracker == NULL || index >= tracker->count)
  {
    return NULL;
  }
  return &tracker->events[index];
}

int aura_race_happens_before(const AuraRaceEvent *before, const AuraRaceEvent *after)
{
  return before != NULL && after != NULL && before->sequence < after->sequence;
}
