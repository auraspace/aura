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

typedef struct
{
  AuraTcpStream *stream;
  int polls;
} TcpTask;

typedef struct
{
  AuraFile file;
  int polls;
} FileTask;

typedef struct
{
  AuraTcpListener *listener;
  AuraTcpStream *accepted;
  int polls;
} TcpListenerTask;

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

static AuraTaskPollState poll_file(AuraTaskFrame *frame)
{
  FileTask *task = (FileTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    assert(aura_task_frame_wait_file(frame, &task->file, POLLIN) == 1);
    return AURA_TASK_PENDING;
  }

  char value = 0;
  uint64_t read_count = 0;
  assert(aura_file_read(&task->file, &value, sizeof(value), &read_count) == AURA_FILE_OK);
  assert(read_count == 1);
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = (unsigned char)value;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_int);
  return AURA_TASK_COMPLETE;
}

static void test_file_descriptor_adapter_wait(void)
{
  int pipe_fds[2];
  assert(pipe(pipe_fds) == 0);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(FileTask), poll_file, NULL);
  assert(frame != NULL);
  FileTask *task = (FileTask *)aura_task_frame_data(frame);
  task->file.fd = pipe_fds[0];
  task->file.closed = false;
  task->polls = 0;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  const char byte = 'F';
  assert(write(pipe_fds[1], &byte, sizeof(byte)) == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  AuraTaskResult result = aura_task_frame_result(frame);
  assert(result.data != NULL && *(int *)result.data == 'F');
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  close(pipe_fds[0]);
  close(pipe_fds[1]);
}

static AuraTaskPollState poll_tcp_stream(AuraTaskFrame *frame)
{
  TcpTask *task = (TcpTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    assert(aura_task_frame_wait_tcp_stream(frame, task->stream, POLLIN) == 1);
    return AURA_TASK_PENDING;
  }

  char value = 0;
  size_t read_count = 0;
  assert(aura_tcp_stream_read(task->stream, &value, sizeof(value), &read_count, 0) == AURA_TCP_OK);
  assert(read_count == 1);
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = (unsigned char)value;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_int);
  return AURA_TASK_COMPLETE;
}

static void test_tcp_stream_adapter_wait(void)
{
  AuraTcpListener *listener = NULL;
  AuraTcpStream *client = NULL;
  AuraTcpStream *accepted = NULL;
  uint16_t port = 0;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_tcp_listener_accept(listener, 1000, &accepted) == AURA_TCP_OK);

  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(TcpTask), poll_tcp_stream, NULL);
  assert(frame != NULL);
  TcpTask *task = (TcpTask *)aura_task_frame_data(frame);
  task->stream = accepted;
  task->polls = 0;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  const char byte = 'T';
  size_t written = 0;
  assert(aura_tcp_stream_write(client, &byte, sizeof(byte), &written, 1000) == AURA_TCP_OK);
  assert(written == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  AuraTaskResult result = aura_task_frame_result(frame);
  assert(result.data != NULL && *(int *)result.data == 'T');
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  aura_tcp_stream_destroy(client);
  aura_tcp_stream_destroy(accepted);
  aura_tcp_listener_destroy(listener);
}

static AuraTaskPollState poll_tcp_listener(AuraTaskFrame *frame)
{
  TcpListenerTask *task = (TcpListenerTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    assert(aura_task_frame_wait_tcp_listener(frame, task->listener, POLLIN) == 1);
    return AURA_TASK_PENDING;
  }

  assert(aura_tcp_listener_accept(task->listener, 0, &task->accepted) == AURA_TCP_OK);
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = 1;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_int);
  return AURA_TASK_COMPLETE;
}

static void test_tcp_listener_adapter_wait(void)
{
  AuraTcpListener *listener = NULL;
  AuraTcpStream *client = NULL;
  uint16_t port = 0;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(TcpListenerTask), poll_tcp_listener, NULL);
  assert(frame != NULL);
  TcpListenerTask *task = (TcpListenerTask *)aura_task_frame_data(frame);
  task->listener = listener;
  task->accepted = NULL;
  task->polls = 0;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(task->accepted != NULL);
  AuraTcpStream *accepted = task->accepted;
  AuraTaskResult result = aura_task_frame_result(frame);
  assert(result.data != NULL && *(int *)result.data == 1);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  aura_tcp_stream_destroy(client);
  aura_tcp_stream_destroy(accepted);
  aura_tcp_listener_destroy(listener);
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

static void test_multiple_ready_fds_wake_in_one_turn(void)
{
  int pipes[2][2];
  assert(pipe(pipes[0]) == 0);
  assert(pipe(pipes[1]) == 0);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *first = new_fd_task(pipes[0][0]);
  AuraTaskFrame *second = new_fd_task(pipes[1][0]);
  assert(aura_task_executor_submit(executor, first) == 1);
  assert(aura_task_executor_submit(executor, second) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  const char bytes[2] = {'A', 'B'};
  assert(write(pipes[0][1], &bytes[0], 1) == 1);
  assert(write(pipes[1][1], &bytes[1], 1) == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 2);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(first) == AURA_TASK_COMPLETE);
  assert(aura_task_frame_state(second) == AURA_TASK_COMPLETE);
  assert(aura_task_executor_release(executor, &first) == 1);
  assert(aura_task_executor_release(executor, &second) == 1);
  close(pipes[0][1]);
  close(pipes[1][1]);
  aura_task_executor_shutdown(executor);
}

int main(void)
{
  test_ready_fd_wakes_pending_frame();
  test_file_descriptor_adapter_wait();
  test_cancellation_clears_fd_registration();
  test_multiple_ready_fds_wake_in_one_turn();
  test_tcp_stream_adapter_wait();
  test_tcp_listener_adapter_wait();
  aura_gc_shutdown();
  puts("task fd wait: passed");
  return 0;
}
