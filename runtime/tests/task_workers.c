#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <stdatomic.h>
#include <stdint.h>
#include <time.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct
{
  atomic_int *completed;
  uintptr_t *threads;
  atomic_int *thread_count;
} WorkerState;

static AuraTaskPollState worker_poll(AuraTaskFrame *frame)
{
  WorkerState *state = (WorkerState *)aura_task_frame_data(frame);
  struct timespec delay = {0, 20000000L};
  uintptr_t thread = (uintptr_t)pthread_self();
  int index = atomic_fetch_add(state->thread_count, 1);
  if (index < 16)
  {
    state->threads[index] = thread;
  }
  nanosleep(&delay, NULL);
  atomic_fetch_add(state->completed, 1);
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  atomic_int completed = 0;
  atomic_int thread_count = 0;
  uintptr_t threads[16] = {0};
  AuraTaskFrame *frames[8] = {0};
  assert(executor != NULL);
  assert(aura_task_executor_start_workers(executor, 4) == 1);

  for (size_t i = 0; i < 8; i++)
  {
    frames[i] = aura_task_frame_new(sizeof(WorkerState), worker_poll, NULL);
    assert(frames[i] != NULL);
    WorkerState *state = (WorkerState *)aura_task_frame_data(frames[i]);
    state->completed = &completed;
    state->threads = threads;
    state->thread_count = &thread_count;
    assert(aura_task_executor_submit(executor, frames[i]) == 1);
  }
  for (int i = 0; i < 200 && atomic_load(&completed) != 8; i++)
  {
    struct timespec delay = {0, 10000000L};
    nanosleep(&delay, NULL);
  }
  assert(atomic_load(&completed) == 8);
  int distinct = 0;
  for (int i = 0; i < atomic_load(&thread_count) && i < 16; i++)
  {
    int seen = 0;
    for (int j = 0; j < i; j++)
    {
      if (threads[j] == threads[i])
      {
        seen = 1;
        break;
      }
    }
    if (!seen)
    {
      distinct++;
    }
  }
  assert(distinct >= 2);
  for (size_t i = 0; i < 8; i++)
  {
    assert(aura_task_executor_release(executor, &frames[i]) == 1);
  }
  aura_task_executor_shutdown(executor);
  return 0;
}
