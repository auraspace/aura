#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <poll.h>
#include <stdio.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

typedef struct
{
  AuraIoOperationHandle *operation;
  AuraFile file;
  char value;
  int polls;
} FileOperationTask;

typedef struct
{
  AuraIoOperationHandle *operation;
  AuraTcpStream *stream;
  int cleanup_count;
  int polls;
} TcpOperationTask;

static void close_file_resource(void *resource)
{
  AuraFile *file = (AuraFile *)resource;
  assert(file != NULL);
  assert(aura_file_close(file) == AURA_FILE_OK);
}

static int tcp_cleanup_count;

static void close_tcp_resource(void *resource)
{
  AuraTcpStream *stream = (AuraTcpStream *)resource;
  assert(stream != NULL);
  assert(aura_tcp_stream_close(stream) == 1);
  tcp_cleanup_count++;
}

static void drop_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

static AuraTaskPollState poll_file_operation(AuraTaskFrame *frame)
{
  FileOperationTask *task = (FileOperationTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    task->operation = aura_file_async_read_handle_new(
        &task->file, close_file_resource);
    assert(task->operation != NULL);
    assert(aura_io_operation_handle_kind(task->operation) ==
           AURA_IO_OPERATION_FILE_READ);
    assert(aura_io_operation_handle_start(task->operation,
                                          frame->executor, frame) == 1);
    return AURA_TASK_PENDING;
  }

  assert(aura_io_operation_handle_state(task->operation) ==
         AURA_IO_OPERATION_COMPLETE);
  /* The operation's task owns the actual output count and buffer. */
  {
    uint64_t count = 0;
    assert(aura_file_read(&task->file, &task->value, 1, &count) == AURA_FILE_OK);
    assert(count == 1 && task->value == 'F');
  }
  {
    int *result = (int *)malloc(sizeof(*result));
    assert(result != NULL);
    *result = task->value;
    aura_task_frame_set_result(frame, result, sizeof(*result), drop_int);
  }
  assert(aura_file_close(&task->file) == AURA_FILE_OK);
  return AURA_TASK_COMPLETE;
}

static AuraTaskPollState poll_tcp_operation(AuraTaskFrame *frame)
{
  TcpOperationTask *task = (TcpOperationTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    task->operation = aura_tcp_async_read_handle_new(
        task->stream, close_tcp_resource);
    assert(task->operation != NULL);
    assert(aura_io_operation_handle_start(task->operation,
                                          frame->executor, frame) == 1);
    return AURA_TASK_PENDING;
  }
  assert(aura_io_operation_handle_state(task->operation) ==
         AURA_IO_OPERATION_CANCELLED);
  return AURA_TASK_CANCELLED;
}

static void test_file_handle_completion_wakes_task(void)
{
  int pipe_fds[2];
  assert(pipe(pipe_fds) == 0);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(FileOperationTask),
                                             poll_file_operation, NULL);
  assert(frame != NULL);
  FileOperationTask *task = (FileOperationTask *)aura_task_frame_data(frame);
  task->file.fd = pipe_fds[0];
  task->file.closed = false;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  assert(write(pipe_fds[1], "F", 1) == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_io_operation_handle_state(task->operation) ==
         AURA_IO_OPERATION_COMPLETE);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(*(int *)aura_task_frame_result(frame).data == 'F');
  assert(aura_io_operation_handle_release(&task->operation) == 1);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  assert(close(pipe_fds[1]) == 0);
}

static void test_tcp_handle_cancellation_cleans_once(void)
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
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(TcpOperationTask),
                                             poll_tcp_operation, NULL);
  assert(frame != NULL);
  TcpOperationTask *task = (TcpOperationTask *)aura_task_frame_data(frame);
  task->stream = accepted;
  tcp_cleanup_count = 0;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);

  assert(aura_io_operation_handle_cancel(task->operation) == 1);
  assert(tcp_cleanup_count == 1);
  assert(aura_task_executor_poll_waiting(executor, 0) == 0);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_CANCELLED);
  assert(aura_io_operation_handle_cancel(task->operation) == 0);
  assert(aura_io_operation_handle_release(&task->operation) == 1);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  aura_tcp_stream_destroy(client);
  aura_tcp_stream_destroy(accepted);
  aura_tcp_listener_destroy(listener);
}

int main(void)
{
  test_file_handle_completion_wakes_task();
  test_tcp_handle_cancellation_cleans_once();
  aura_gc_shutdown();
  puts("task I/O operation handles: passed");
  return 0;
}
