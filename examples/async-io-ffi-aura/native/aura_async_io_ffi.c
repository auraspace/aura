#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <poll.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../../runtime/aura_rt.c"

typedef struct {
  AuraIoOperationHandle *operation;
  AuraFile file;
  int polls;
  char value;
} SmokeTask;

static void close_file(void *resource)
{
  AuraFile *file = (AuraFile *)resource;
  if (file != NULL) (void)aura_file_close(file);
}

static AuraTaskPollState poll_smoke(AuraTaskFrame *frame)
{
  SmokeTask *task = (SmokeTask *)aura_task_frame_data(frame);
  if (task->polls++ == 0) {
    task->operation = aura_file_async_read_handle_new(&task->file, close_file);
    if (task->operation == NULL ||
        aura_io_operation_handle_start(task->operation, frame->executor, frame) != 1 ||
        aura_io_operation_handle_check_boundary(task->operation,
                                                AURA_FFI_BOUNDARY_TASK) != AURA_FFI_OK) {
      return AURA_TASK_FAILED;
    }
    return AURA_TASK_PENDING;
  }
  if (aura_io_operation_handle_state(task->operation) != AURA_IO_OPERATION_COMPLETE) {
    return AURA_TASK_FAILED;
  }
  uint64_t count = 0;
  if (aura_file_read(&task->file, &task->value, 1, &count) != AURA_FILE_OK ||
      count != 1 || task->value != 'A') {
    return AURA_TASK_FAILED;
  }
  (void)aura_file_close(&task->file);
  (void)aura_io_operation_handle_release(&task->operation);
  return AURA_TASK_COMPLETE;
}

int aura_async_io_ffi_status(void)
{
  int pipe_fds[2];
  if (pipe(pipe_fds) != 0) return 1;
  AuraTaskExecutor *executor = aura_task_executor_new();
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(SmokeTask), poll_smoke, NULL);
  if (executor == NULL || frame == NULL) return 1;
  SmokeTask *task = (SmokeTask *)aura_task_frame_data(frame);
  task->file.fd = pipe_fds[0];
  task->file.closed = false;
  if (aura_task_executor_submit(executor, frame) != 1 ||
      aura_task_executor_run_one(executor) != 1 ||
      aura_task_frame_state(frame) != AURA_TASK_PENDING ||
      write(pipe_fds[1], "A", 1) != 1 ||
      aura_task_executor_poll_waiting(executor, 1000) != 1 ||
      aura_task_executor_run_one(executor) != 1 ||
      aura_task_frame_state(frame) != AURA_TASK_COMPLETE) {
    aura_task_executor_shutdown(executor);
    close(pipe_fds[1]);
    return 1;
  }
  (void)aura_task_executor_release(executor, &frame);
  aura_task_executor_shutdown(executor);
  close(pipe_fds[1]);
  aura_gc_shutdown();
  return 0;
}
