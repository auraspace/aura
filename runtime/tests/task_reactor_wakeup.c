#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <pthread.h>
#include <time.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct
{
  int polls;
} WakeState;

static AuraTaskPollState poll_once_after_wake(AuraTaskFrame *frame)
{
  WakeState *state = (WakeState *)aura_task_frame_data(frame);
  state->polls++;
  return state->polls == 1 ? AURA_TASK_PENDING : AURA_TASK_COMPLETE;
}

typedef struct
{
  AuraTaskExecutor *executor;
  AuraTaskFrame *frame;
} WakeArgs;

static void *wake_from_foreign_thread(void *data)
{
  WakeArgs *args = (WakeArgs *)data;
  struct timespec delay = {.tv_sec = 0, .tv_nsec = 10 * 1000 * 1000};
  nanosleep(&delay, NULL);
  assert(aura_task_executor_wake(args->executor, args->frame) == 1);
  return NULL;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  AuraTaskFrame *frame = aura_task_frame_new(
      sizeof(WakeState), poll_once_after_wake, NULL);
  assert(executor != NULL && frame != NULL);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  WakeArgs args = {executor, frame};
  pthread_t thread;
  assert(pthread_create(&thread, NULL, wake_from_foreign_thread, &args) == 0);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(pthread_join(thread, NULL) == 0);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);

  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  return 0;
}
