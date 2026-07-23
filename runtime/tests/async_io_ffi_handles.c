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
  int polls;
  char value;
} FileTask;

static void close_file(void *resource)
{
  assert(resource != NULL);
  assert(aura_file_close((AuraFile *)resource) == AURA_FILE_OK);
}

static void drop_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

static AuraTaskPollState poll_file(AuraTaskFrame *frame)
{
  FileTask *task = (FileTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    task->operation = aura_file_async_read_operation_new(
        &task->file, &task->value, 1, close_file);
    assert(task->operation != NULL);
    assert(aura_io_operation_handle_check_boundary(
               task->operation, AURA_FFI_BOUNDARY_SYNC) == AURA_FFI_OK);
    assert(aura_io_operation_handle_start(task->operation, frame->executor,
                                          frame) == 1);
    assert(aura_io_operation_handle_check_boundary(
               task->operation, AURA_FFI_BOUNDARY_TASK) == AURA_FFI_OK);
    assert(aura_io_operation_handle_check_boundary(
               task->operation, AURA_FFI_BOUNDARY_AWAIT) ==
           AURA_FFI_BOUNDARY_REJECTED);
    assert(aura_io_operation_handle_check_boundary(
               task->operation, AURA_FFI_BOUNDARY_CHANNEL) ==
           AURA_FFI_BOUNDARY_REJECTED);
    return AURA_TASK_PENDING;
  }

  assert(aura_io_operation_handle_state(task->operation) ==
         AURA_IO_OPERATION_COMPLETE);
  AuraIoOperationResult io = {0};
  assert(aura_io_operation_handle_result(task->operation, &io) == 1);
  assert(io.kind == AURA_IO_OPERATION_FILE_READ);
  assert(io.state == AURA_IO_OPERATION_COMPLETE);
  assert(io.outcome == AURA_IO_OUTCOME_OK);
  assert(io.native_status == AURA_FILE_OK);
  assert(io.bytes_transferred == 1 && task->value == 'A');
  int *result = (int *)malloc(sizeof(*result));
  assert(result != NULL);
  *result = task->value;
  aura_task_frame_set_result(frame, result, sizeof(*result), drop_int);
  assert(aura_file_close(&task->file) == AURA_FILE_OK);
  assert(aura_io_operation_handle_release(&task->operation) == 1);
  return AURA_TASK_COMPLETE;
}

typedef struct
{
  AuraIoOperationHandle *operation;
  AuraTcpStream stream;
  int polls;
  char value;
} TcpEofTask;

static AuraTaskPollState poll_tcp_eof(AuraTaskFrame *frame)
{
  TcpEofTask *task = (TcpEofTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    task->operation =
        aura_tcp_async_read_operation_new(&task->stream, &task->value, 1, NULL);
    assert(task->operation != NULL);
    assert(aura_io_operation_handle_start(task->operation, frame->executor,
                                          frame) == 1);
    return AURA_TASK_PENDING;
  }

  AuraIoOperationResult io = {0};
  assert(aura_io_operation_handle_result(task->operation, &io) == 1);
  assert(io.kind == AURA_IO_OPERATION_TCP_READ);
  assert(io.state == AURA_IO_OPERATION_COMPLETE);
  assert(io.outcome == AURA_IO_OUTCOME_EOF);
  assert(io.native_status == AURA_TCP_EOF);
  assert(io.bytes_transferred == 0);
  assert(aura_tcp_stream_close(&task->stream) == 1);
  assert(aura_io_operation_handle_release(&task->operation) == 1);
  return AURA_TASK_COMPLETE;
}

static void test_tcp_peer_eof_is_a_typed_completion(void)
{
  int sockets[2];
  assert(socketpair(AF_UNIX, SOCK_STREAM, 0, sockets) == 0);
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame =
      aura_task_frame_new(sizeof(TcpEofTask), poll_tcp_eof, NULL);
  assert(frame != NULL);
  TcpEofTask *task = (TcpEofTask *)aura_task_frame_data(frame);
  task->stream.fd = sockets[0];
  assert(aura_tcp_set_nonblocking(task->stream.fd) == 0);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(close(sockets[1]) == 0);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
}

static void test_scheduler_completion_and_typed_boundary(void)
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
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(write(pipe_fds[1], "A", 1) == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_io_operation_handle_state(task->operation) ==
         AURA_IO_OPERATION_COMPLETE);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(*(int *)aura_task_frame_result(frame).data == 'A');
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  assert(close(pipe_fds[1]) == 0);
}

static void test_cancel_invalidates_boundary_and_cleans_once(void)
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
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_io_operation_handle_cancel(task->operation) == 1);
  assert(aura_io_operation_handle_state(task->operation) ==
         AURA_IO_OPERATION_CANCELLED);
  assert(aura_io_operation_handle_check_boundary(
             task->operation, AURA_FFI_BOUNDARY_TASK) == AURA_FFI_INVALID);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_CANCELLED);
  /* Cancellation closes the borrowed descriptor exactly once. */
  assert(task->file.closed);
  assert(aura_io_operation_handle_release(&task->operation) == 1);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  assert(close(pipe_fds[1]) == 0);
}

int main(void)
{
  test_scheduler_completion_and_typed_boundary();
  test_cancel_invalidates_boundary_and_cleans_once();
  test_tcp_peer_eof_is_a_typed_completion();
  aura_gc_shutdown();
  puts("async io ffi handles: passed");
  return 0;
}
