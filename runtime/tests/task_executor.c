#include <assert.h>

#include "../../runtime/aura_rt.c"

typedef struct
{
  int id;
  int polls;
  int *log;
  int *log_len;
  int ready_once;
} TaskState;

static int destroyed;

static void destroy_task(AuraTaskFrame *frame)
{
  (void)frame;
  destroyed++;
}

static AuraTaskPollState poll_task(AuraTaskFrame *frame)
{
  TaskState *task = (TaskState *)aura_task_frame_data(frame);
  task->log[(*task->log_len)++] = task->id;
  task->polls++;
  if (task->ready_once && task->polls == 1)
  {
    return AURA_TASK_READY;
  }
  if (!task->ready_once && task->polls == 1)
  {
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_COMPLETE;
}

static AuraTaskFrame *new_task(int id, int *log, int *log_len, int ready_once)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(TaskState), poll_task, destroy_task);
  assert(frame != NULL);
  TaskState *task = (TaskState *)aura_task_frame_data(frame);
  task->id = id;
  task->log = log;
  task->log_len = log_len;
  task->ready_once = ready_once;
  return frame;
}

int main(void)
{
  int log[8] = {0};
  int log_len = 0;
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);

  AuraTaskFrame *yielding = new_task(1, log, &log_len, 1);
  AuraTaskFrame *pending = new_task(2, log, &log_len, 0);
  assert(aura_task_executor_submit(executor, yielding) == 1);
  assert(aura_task_executor_submit(executor, pending) == 1);
  assert(aura_task_executor_ready_count(executor) == 2);
  assert(aura_task_executor_task_count(executor) == 2);

  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(log[0] == 1 && log[1] == 2 && log_len == 2);
  assert(aura_task_frame_state(yielding) == AURA_TASK_READY);
  assert(aura_task_frame_state(pending) == AURA_TASK_PENDING);
  assert(aura_task_executor_ready_count(executor) == 1);

  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(yielding) == AURA_TASK_COMPLETE);
  assert(log[2] == 1 && log_len == 3);
  assert(aura_task_executor_wake(executor, pending) == 1);
  assert(aura_task_executor_run(executor) == 1);
  assert(aura_task_frame_state(pending) == AURA_TASK_COMPLETE);

  AuraTaskFrame *cancelled = new_task(3, log, &log_len, 0);
  assert(aura_task_executor_submit(executor, cancelled) == 1);
  assert(aura_task_executor_cancel(executor, cancelled) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(cancelled) == AURA_TASK_CANCELLED);
  assert(log_len == 4);

  aura_task_executor_shutdown(executor);
  assert(destroyed == 3);
  aura_task_executor_shutdown(NULL);
  return 0;
}
