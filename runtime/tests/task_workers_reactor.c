#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <poll.h>
#include <stdatomic.h>
#include <time.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct
{
  int fd;
  atomic_int *done;
  atomic_int polls;
} ReactorTask;

static AuraTaskPollState poll_reactor_task(AuraTaskFrame *frame)
{
  ReactorTask *task = (ReactorTask *)aura_task_frame_data(frame);
  if (atomic_fetch_add(&task->polls, 1) == 0)
  {
    assert(aura_task_frame_wait_fd(frame, task->fd, POLLIN) == 1);
    return AURA_TASK_PENDING;
  }
  char byte = 0;
  assert(read(task->fd, &byte, sizeof(byte)) == 1);
  atomic_store(task->done, 1);
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  int fds[2];
  assert(pipe(fds) == 0);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  assert(aura_task_executor_start_workers(executor, 2) == 1);
  atomic_int done = 0;
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(ReactorTask), poll_reactor_task, NULL);
  assert(frame != NULL);
  ReactorTask *task = (ReactorTask *)aura_task_frame_data(frame);
  *task = (ReactorTask){fds[0], &done, 0};
  assert(aura_task_executor_submit(executor, frame) == 1);
  for (int i = 0; i < 100 && atomic_load(&task->polls) == 0; i++)
  {
    struct timespec delay = {0, 1000000L};
    nanosleep(&delay, NULL);
  }
  assert(atomic_load(&task->polls) >= 1);
  assert(write(fds[1], "R", 1) == 1);
  for (int i = 0; i < 200 && !atomic_load(&done); i++)
  {
    struct timespec delay = {0, 1000000L};
    nanosleep(&delay, NULL);
  }
  assert(atomic_load(&done) == 1);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  close(fds[0]);
  close(fds[1]);
  return 0;
}
