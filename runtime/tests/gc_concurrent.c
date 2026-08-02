#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>
#include <time.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct
{
  void *object;
  atomic_int polls;
} GcTask;

static AuraTaskPollState poll_gc_task(AuraTaskFrame *frame)
{
  GcTask *task = (GcTask *)aura_task_frame_data(frame);
  if (atomic_fetch_add(&task->polls, 1) == 0)
  {
    task->object = aura_gc_alloc(64);
    assert(aura_task_frame_wait_deadline(frame, 1) == 1);
    struct timespec delay = {0, 20000000L};
    nanosleep(&delay, NULL);
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_COMPLETE;
}

typedef struct
{
  AuraTaskExecutor *executor;
  atomic_int *stop;
} Collector;

static void *collect_concurrently(void *arg)
{
  Collector *collector = (Collector *)arg;
  for (int cycle = 0; cycle < 8 && !atomic_load(collector->stop); cycle++)
  {
    aura_gc_collect_executor(collector->executor);
    struct timespec delay = {0, 1000000L};
    nanosleep(&delay, NULL);
  }
  atomic_store(collector->stop, 1);
  return NULL;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  assert(aura_task_executor_start_workers(executor, 4) == 1);
  atomic_int stop = 0;
  Collector collector = {executor, &stop};
  pthread_t collector_thread;
  assert(pthread_create(&collector_thread, NULL, collect_concurrently, &collector) == 0);
  AuraTaskFrame *frames[8] = {NULL};
  for (size_t i = 0; i < 8; i++)
  {
    frames[i] = aura_task_frame_new(sizeof(GcTask), poll_gc_task, NULL);
    assert(frames[i] != NULL);
    assert(aura_task_executor_submit(executor, frames[i]) == 1);
  }
  for (int spin = 0; spin < 500 && aura_task_executor_has_live_tasks(executor); spin++)
  {
    struct timespec delay = {0, 1000000L};
    nanosleep(&delay, NULL);
  }
  for (size_t i = 0; i < 8; i++)
    assert(aura_task_frame_state(frames[i]) == AURA_TASK_COMPLETE);
  atomic_store(&stop, 1);
  assert(pthread_join(collector_thread, NULL) == 0);
  for (size_t i = 0; i < 8; i++)
    assert(aura_task_executor_release(executor, &frames[i]) == 1);
  aura_task_executor_shutdown(executor);
  aura_gc_collect();
  return 0;
}
