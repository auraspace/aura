#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

typedef struct
{
  char path[64];
  AuraFile *file;
  AuraTcpListener *listener;
  AuraTcpStream *client;
  AuraTcpStream *accepted;
  int *cleanup_count;
} IoResources;

typedef struct
{
  int fail;
  IoResources *resources;
} IoTask;

static void cleanup_io_resources(void *data)
{
  IoResources *resources = (IoResources *)data;
  assert(resources != NULL);
  assert(resources->cleanup_count != NULL);
  assert(aura_file_destroy(&resources->file) == AURA_FILE_OK);
  aura_tcp_stream_destroy(resources->client);
  resources->client = NULL;
  aura_tcp_stream_destroy(resources->accepted);
  resources->accepted = NULL;
  aura_tcp_listener_destroy(resources->listener);
  resources->listener = NULL;
  assert(unlink(resources->path) == 0);
  (*resources->cleanup_count)++;
  free(resources);
}

static void drop_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

static AuraTaskPollState poll_io(AuraTaskFrame *frame)
{
  IoTask *task = (IoTask *)aura_task_frame_data(frame);
  IoResources *resources = task->resources;

  if (resources == NULL)
  {
    resources = (IoResources *)calloc(1, sizeof(*resources));
    assert(resources != NULL);
    snprintf(resources->path, sizeof(resources->path),
             "/tmp/aura-task-io-XXXXXX");
    int fd = mkstemp(resources->path);
    assert(fd >= 0);
    assert(close(fd) == 0);
    assert(aura_file_open(resources->path, AURA_FILE_WRITE,
                          &resources->file) == AURA_FILE_OK);

    uint16_t port = 0;
    assert(aura_tcp_listener_bind(0, &port, &resources->listener) ==
           AURA_TCP_OK);
    assert(aura_tcp_stream_connect(port, 1000, &resources->client) ==
           AURA_TCP_OK);
    assert(aura_tcp_listener_accept(resources->listener, 1000,
                                    &resources->accepted) == AURA_TCP_OK);

    task->resources = resources;
    assert(aura_task_frame_set_pending_with_ownership(
               frame, &task->resources, sizeof(task->resources), NULL,
               AURA_TASK_TRANSFERRED) == 1);
    aura_task_frame_set_cleanup(frame, resources, cleanup_io_resources);
    return AURA_TASK_PENDING;
  }

  if (task->fail)
  {
    int *error = (int *)malloc(sizeof(*error));
    assert(error != NULL);
    *error = 17;
    aura_task_frame_set_error(frame, error, sizeof(*error), drop_int);
    return AURA_TASK_FAILED;
  }

  /* Completion transfers responsibility away from the frame. */
  aura_task_frame_clear_cleanup(frame);
  aura_task_frame_set_pending(frame, NULL, 0, NULL);
  return AURA_TASK_COMPLETE;
}

static AuraTaskFrame *new_io_task(int fail)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(IoTask), poll_io, NULL);
  assert(frame != NULL);
  ((IoTask *)aura_task_frame_data(frame))->fail = fail;
  return frame;
}

static void test_cancel_closes_file_and_tcp_once(void)
{
  int cleanup_count = 0;
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *frame = new_io_task(0);
  assert(aura_task_executor_submit(executor, frame) == 1);
  /* The resource callback needs the counter; install it through task data
   * after the first poll has created the operation. */
  assert(aura_task_executor_run_one(executor) == 1);
  IoResources *resources = ((IoTask *)aura_task_frame_data(frame))->resources;
  resources->cleanup_count = &cleanup_count;
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  assert(aura_task_executor_ready_count(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_CANCELLED);
  assert(cleanup_count == 1);
  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(frame == NULL);
  assert(cleanup_count == 1);
  aura_task_executor_shutdown(executor);
}

static void test_failure_and_shutdown_cleanup_once(void)
{
  int cleanup_count = 0;
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskFrame *failed = new_io_task(1);
  assert(aura_task_executor_submit(executor, failed) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  IoResources *resources = ((IoTask *)aura_task_frame_data(failed))->resources;
  resources->cleanup_count = &cleanup_count;
  assert(aura_task_executor_wake(executor, failed) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(failed) == AURA_TASK_FAILED);
  assert(cleanup_count == 1);
  assert(aura_task_executor_release(executor, &failed) == 1);

  AuraTaskFrame *shutdown = new_io_task(0);
  assert(aura_task_executor_submit(executor, shutdown) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  resources = ((IoTask *)aura_task_frame_data(shutdown))->resources;
  resources->cleanup_count = &cleanup_count;
  assert(aura_task_frame_state(shutdown) == AURA_TASK_PENDING);
  aura_task_executor_shutdown(executor);
  assert(cleanup_count == 2);
}

int main(void)
{
  test_cancel_closes_file_and_tcp_once();
  test_failure_and_shutdown_cleanup_once();
  aura_gc_shutdown();
  puts("task I/O cleanup sanitizer: passed");
  return 0;
}
