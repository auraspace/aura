#include <assert.h>
#include <stdint.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  int value;
  uint32_t source;
} FailureState;

typedef struct
{
  size_t count;
  uint64_t task_ids[8];
  uint32_t source_ids[8];
  int values[8];
} FailureLog;

static int drops;

static void drop_error(void *data, size_t size)
{
  assert(size == sizeof(int));
  drops++;
  free(data);
}

static AuraTaskPollState poll_failure(AuraTaskFrame *frame)
{
  FailureState *state = (FailureState *)aura_task_frame_data(frame);
  int *error = (int *)malloc(sizeof(*error));
  assert(error != NULL);
  *error = state->value;
  aura_task_frame_set_error_at(frame, error, sizeof(*error), drop_error,
                               state->source);
  return AURA_TASK_FAILED;
}

static AuraTaskPollState poll_pending(AuraTaskFrame *frame)
{
  (void)frame;
  return AURA_TASK_PENDING;
}

static AuraTaskFrame *new_failure(int value, uint32_t source)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(FailureState),
                                             poll_failure, NULL);
  assert(frame != NULL);
  FailureState *state = (FailureState *)aura_task_frame_data(frame);
  state->value = value;
  state->source = source;
  return frame;
}

static void record_failure(const AuraTaskFailureDiagnostic *diagnostic,
                           void *context)
{
  FailureLog *log = (FailureLog *)context;
  assert(diagnostic != NULL);
  assert(diagnostic->state == AURA_TASK_FAILED);
  assert(diagnostic->error.data != NULL);
  assert(diagnostic->error.size == sizeof(int));
  assert(log->count < 8);
  log->task_ids[log->count] = diagnostic->task_id;
  log->source_ids[log->count] = diagnostic->source_id;
  log->values[log->count] = *(int *)diagnostic->error.data;
  log->count++;
}

static void test_release_reports_unjoined_failure(void)
{
  FailureLog log = {0};
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  aura_task_executor_set_failure_hook(executor, record_failure, &log);

  AuraTaskFrame *frame = new_failure(17, 4101);
  uint64_t task_id = aura_task_frame_task_id(frame);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_FAILED);
  assert(log.count == 0);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(frame == NULL);
  assert(log.count == 1);
  assert(log.task_ids[0] == task_id);
  assert(log.source_ids[0] == 4101);
  assert(log.values[0] == 17);
  assert(drops == 1);
  aura_task_executor_shutdown(executor);
}

static void test_join_suppresses_unjoined_report(void)
{
  FailureLog log = {0};
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  aura_task_executor_set_failure_hook(executor, record_failure, &log);
  AuraTaskFrame *frame = new_failure(23, 4202);
  AuraTaskResult result;
  AuraTaskResult error;
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_FAILED);
  assert(error.data != NULL && *(int *)error.data == 23);
  assert(log.count == 0);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(log.count == 0);
  assert(drops == 2);
  aura_task_executor_shutdown(executor);
}

static void test_shutdown_reports_all_unjoined_failures(void)
{
  FailureLog log = {0};
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  aura_task_executor_set_failure_hook(executor, record_failure, &log);
  AuraTaskFrame *first = new_failure(31, 4303);
  AuraTaskFrame *second = new_failure(37, 4404);
  uint64_t first_id = aura_task_frame_task_id(first);
  uint64_t second_id = aura_task_frame_task_id(second);
  assert(aura_task_executor_submit(executor, first) == 1);
  assert(aura_task_executor_submit(executor, second) == 1);
  assert(aura_task_executor_run(executor) == 2);
  assert(log.count == 0);
  aura_task_executor_shutdown(executor);
  assert(log.count == 2);
  assert(log.task_ids[0] == second_id);
  assert(log.task_ids[1] == first_id);
  assert(log.source_ids[0] == 4404 && log.values[0] == 37);
  assert(log.source_ids[1] == 4303 && log.values[1] == 31);
  assert(drops == 4);
}

static void test_cancellation_is_not_failure(void)
{
  FailureLog log = {0};
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  aura_task_executor_set_failure_hook(executor, record_failure, &log);
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_pending, NULL);
  assert(frame != NULL);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_CANCELLED);
  assert(log.count == 0);
  aura_task_executor_shutdown(executor);
}

int main(void)
{
  test_release_reports_unjoined_failure();
  test_join_suppresses_unjoined_report();
  test_shutdown_reports_all_unjoined_failures();
  test_cancellation_is_not_failure();
  aura_gc_shutdown();
  assert(drops == 4);
  return 0;
}
