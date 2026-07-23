#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

typedef struct
{
  int fd;
  int polls;
} FdTask;

static void destroy_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

static AuraTaskPollState poll_fd(AuraTaskFrame *frame)
{
  FdTask *task = (FdTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    assert(aura_task_frame_wait_fd(frame, task->fd, POLLIN) == 1);
    return AURA_TASK_PENDING;
  }

  char value = 0;
  assert(read(task->fd, &value, sizeof(value)) == 1);
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = (unsigned char)value;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_int);
  return AURA_TASK_COMPLETE;
}

static AuraTaskFrame *new_fd_task(int fd)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(FdTask), poll_fd, NULL);
  assert(frame != NULL);
  ((FdTask *)aura_task_frame_data(frame))->fd = fd;
  return frame;
}

static void test_ready_fd_wakes_pending_frame(void)
{
  int pipe_fds[2];
  assert(pipe(pipe_fds) == 0);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_fd_task(pipe_fds[0]);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_poll_waiting(executor, 0) == 0);

  const char byte = 'Z';
  assert(write(pipe_fds[1], &byte, sizeof(byte)) == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_READY);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  AuraTaskResult result = aura_task_frame_result(frame);
  assert(result.data != NULL && *(int *)result.data == 'Z');
  assert(aura_task_executor_release(executor, &frame) == 1);
  close(pipe_fds[1]);
  aura_task_executor_shutdown(executor);
}

static void test_cancellation_clears_fd_registration(void)
{
  int pipe_fds[2];
  assert(pipe(pipe_fds) == 0);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_fd_task(pipe_fds[0]);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  assert(aura_task_executor_poll_waiting(executor, 0) == 0);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_CANCELLED);
  assert(aura_task_executor_release(executor, &frame) == 1);
  close(pipe_fds[0]);
  close(pipe_fds[1]);
  aura_task_executor_shutdown(executor);
}

int main(void)
{
  test_ready_fd_wakes_pending_frame();
  test_cancellation_clears_fd_registration();
  aura_gc_shutdown();
  puts("task fd wait: passed");
  return 0;
}
