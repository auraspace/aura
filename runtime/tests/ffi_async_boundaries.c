#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <poll.h>
#include <stdio.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

typedef struct
{
  AuraFile file;
  AuraIoOperationHandle *operation;
  unsigned polls;
  char value;
} IoTask;

static void close_file(void *resource)
{
  AuraFile *file = (AuraFile *)resource;
  assert(file != NULL);
  if (!file->closed) assert(aura_file_close(file) == AURA_FILE_OK);
}

static AuraTaskPollState poll_io(AuraTaskFrame *frame)
{
  IoTask *task = (IoTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0)
  {
    task->operation = aura_file_async_read_handle_new(&task->file, close_file);
    assert(task->operation != NULL);
    assert(aura_io_operation_handle_kind(task->operation) ==
           AURA_IO_OPERATION_FILE_READ);
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
    assert(aura_io_operation_handle_check_boundary(
               task->operation, AURA_FFI_BOUNDARY_CALLBACK) ==
           AURA_FFI_BOUNDARY_REJECTED);
    return AURA_TASK_PENDING;
  }

  assert(aura_io_operation_handle_state(task->operation) ==
         AURA_IO_OPERATION_COMPLETE);
  /* Completion detaches the operation from its task frame; no boundary may
   * retain a completed handle. */
  assert(aura_io_operation_handle_check_boundary(
             task->operation, AURA_FFI_BOUNDARY_TASK) == AURA_FFI_INVALID);
  assert(aura_io_operation_handle_check_boundary(
             task->operation, AURA_FFI_BOUNDARY_AWAIT) ==
         AURA_FFI_INVALID);
  uint64_t read = 0;
  assert(aura_file_read(&task->file, &task->value, 1, &read) == AURA_FILE_OK);
  assert(read == 1 && task->value == 'F');
  assert(aura_file_close(&task->file) == AURA_FILE_OK);
  assert(aura_io_operation_handle_release(&task->operation) == 1);
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  int pipe_fds[2];
  AuraTaskExecutor *executor;
  AuraTaskFrame *frame;
  IoTask *task;

  assert(pipe(pipe_fds) == 0);
  executor = aura_task_executor_new();
  assert(executor != NULL);
  frame = aura_task_frame_new(sizeof(IoTask), poll_io, NULL);
  assert(frame != NULL);
  task = (IoTask *)aura_task_frame_data(frame);
  task->file.fd = pipe_fds[0];
  task->file.closed = false;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(write(pipe_fds[1], "F", 1) == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  assert(close(pipe_fds[1]) == 0);
  aura_gc_shutdown();
  puts("ffi/async boundary coverage: passed");
  return 0;
}
