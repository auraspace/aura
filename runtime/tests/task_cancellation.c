#include <assert.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  int polls;
  AuraTaskPollState terminal;
} CancellationTask;

typedef struct
{
  int *cleanup_count;
  int *cleanup_state;
} CancellationCapture;

static int failure_reports;

static void drop_capture(void *data, size_t size)
{
  CancellationCapture *capture = (CancellationCapture *)data;
  assert(size == sizeof(*capture));
  assert(capture->cleanup_count != NULL);
  assert(capture->cleanup_state != NULL);
  (*capture->cleanup_count)++;
  /* Cleanup is an acknowledgement predecessor: terminal cancellation is
   * published only after this callback has returned. */
  assert(*capture->cleanup_state != AURA_TASK_CANCELLED);
  free(capture);
}

static AuraTaskPollState poll_task(AuraTaskFrame *frame)
{
  CancellationTask *task = (CancellationTask *)aura_task_frame_data(frame);
  task->polls++;
  return task->terminal;
}

static AuraTaskFrame *new_task(AuraTaskPollState terminal,
                               int *cleanup_count,
                               int *cleanup_state)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(CancellationTask),
                                             poll_task, NULL);
  assert(frame != NULL);
  CancellationTask *task = (CancellationTask *)aura_task_frame_data(frame);
  task->terminal = terminal;
  CancellationCapture *capture = (CancellationCapture *)malloc(sizeof(*capture));
  assert(capture != NULL);
  capture->cleanup_count = cleanup_count;
  capture->cleanup_state = cleanup_state;
  aura_task_frame_set_captures(frame, capture, sizeof(*capture), drop_capture);
  return frame;
}

static void record_failure(const AuraTaskFailureDiagnostic *diagnostic,
                           void *context)
{
  (void)diagnostic;
  (void)context;
  failure_reports++;
}

static void test_ready_request_ack_and_join(void)
{
  int cleanups = 0;
  int observed_state = AURA_TASK_READY;
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  aura_task_executor_set_failure_hook(executor, record_failure, NULL);

  AuraTaskFrame *frame = new_task(AURA_TASK_COMPLETE, &cleanups,
                                  &observed_state);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_READY);
  assert(aura_task_frame_cancel_requested(frame) == 0);

  /* Request is accepted but not acknowledged while the frame is queued. */
  assert(aura_task_executor_cancel(executor, frame) == 1);
  assert(aura_task_frame_cancel_requested(frame) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_READY);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  observed_state = aura_task_frame_state(frame);
  assert(observed_state == AURA_TASK_CANCELLED);
  assert(((CancellationTask *)aura_task_frame_data(frame))->polls == 0);
  assert(cleanups == 1);

  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_CANCELLED);
  assert(result.data == NULL && error.data == NULL);
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_CANCELLED);
  assert(aura_task_executor_cancel(executor, frame) == 0);
  assert(cleanups == 1);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(frame == NULL);
  assert(failure_reports == 0);
  aura_task_executor_shutdown(executor);
}

static AuraTaskPollState poll_pending(AuraTaskFrame *frame)
{
  CancellationTask *task = (CancellationTask *)aura_task_frame_data(frame);
  task->polls++;
  return AURA_TASK_PENDING;
}

static void test_pending_request_ack_and_unjoined_shutdown(void)
{
  int cleanups = 0;
  int observed_state = AURA_TASK_PENDING;
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  aura_task_executor_set_failure_hook(executor, record_failure, NULL);

  AuraTaskFrame *frame = aura_task_frame_new(sizeof(CancellationTask),
                                             poll_pending, NULL);
  assert(frame != NULL);
  CancellationTask *task = (CancellationTask *)aura_task_frame_data(frame);
  CancellationCapture *capture = (CancellationCapture *)malloc(sizeof(*capture));
  assert(capture != NULL);
  capture->cleanup_count = &cleanups;
  capture->cleanup_state = &observed_state;
  aura_task_frame_set_captures(frame, capture, sizeof(*capture), drop_capture);

  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(task->polls == 1);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  /* Cancellation wakes a pending frame so the scheduler can acknowledge the
   * request; the state remains non-terminal until that poll. */
  assert(aura_task_frame_state(frame) == AURA_TASK_READY);
  assert(aura_task_executor_ready_count(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  observed_state = aura_task_frame_state(frame);
  assert(observed_state == AURA_TASK_CANCELLED);
  assert(task->polls == 1);
  assert(cleanups == 1);

  /* An unjoined cancellation is the same terminal outcome: it is not sent to
   * the failure hook and shutdown releases the executor-owned frame once. */
  aura_task_executor_shutdown(executor);
  assert(cleanups == 1);
  assert(failure_reports == 0);
}

static void test_completion_wins_if_published_first(void)
{
  int cleanups = 0;
  int observed_state = AURA_TASK_READY;
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_task(AURA_TASK_COMPLETE, &cleanups,
                                  &observed_state);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  observed_state = aura_task_frame_state(frame);
  assert(observed_state == AURA_TASK_COMPLETE);
  assert(((CancellationTask *)aura_task_frame_data(frame))->polls == 1);
  assert(cleanups == 0);

  /* The completion/cancellation race is linearized by the terminal state. */
  assert(aura_task_executor_cancel(executor, frame) == 0);
  assert(aura_task_frame_cancel_requested(frame) == 0);
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  assert(aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_COMPLETE);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(cleanups == 1);
  aura_task_executor_shutdown(executor);
}

int main(void)
{
  test_ready_request_ack_and_join();
  test_pending_request_ack_and_unjoined_shutdown();
  test_completion_wins_if_published_first();
  aura_gc_shutdown();
  return 0;
}
