#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <stdlib.h>
#include <time.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

typedef struct
{
  int value;
  int *destroyed;
} BlockingJob;

static void destroy_result(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

static void destroy_job(void *data)
{
  BlockingJob *job = (BlockingJob *)data;
  (*job->destroyed)++;
  free(job);
}

static void run_job(AuraTaskFrame *frame, void *environment)
{
  BlockingJob *job = (BlockingJob *)environment;
  struct timespec delay = {0, 20000000L};
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  nanosleep(&delay, NULL);
  *result = job->value;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_result);
}

int main(void)
{
  int destroyed = 0;
  AuraTaskExecutor *executor = aura_task_executor_new();
  BlockingJob *job = (BlockingJob *)malloc(sizeof(*job));
  assert(executor != NULL && job != NULL);
  job->value = 42;
  job->destroyed = &destroyed;
  AuraTaskFrame *frame = aura_task_frame_new_blocking(
      executor, run_job, job, destroy_job);
  assert(frame != NULL);
  AuraTaskOutcome outcome = aura_task_executor_join_outcome(executor, frame);
  assert(outcome.state == AURA_TASK_COMPLETE);
  assert(outcome.result.data != NULL && *(int *)outcome.result.data == 42);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(destroyed == 1);
  aura_task_executor_shutdown(executor);
  return 0;
}
