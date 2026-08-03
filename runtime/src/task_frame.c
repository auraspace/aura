typedef struct AuraTaskFrame AuraTaskFrame;
typedef struct AuraTaskExecutor AuraTaskExecutor;
typedef struct AuraTaskChannel AuraTaskChannel;
typedef struct AuraTaskSelect AuraTaskSelect;
typedef struct AuraReactor AuraReactor;

typedef int (*AuraReactorPollFn)(void *data, AuraTaskExecutor *executor,
                                 int timeout_ms);
typedef void (*AuraReactorDestroyFn)(void *data);

#define AURA_REACTOR_ABI_VERSION 1u

struct AuraReactor
{
  uint32_t abi_version;
  AuraReactorPollFn poll;
  void *data;
  AuraReactorDestroyFn data_destroy;
};

static int aura_posix_reactor_poll(void *data, AuraTaskExecutor *executor,
                                   int timeout_ms);
static AuraReactor *aura_reactor_posix_new_internal(void);

typedef void (*AuraTaskResultDestroyFn)(void *data, size_t size);
typedef void *(*AuraTaskResultCloneFn)(const void *data, size_t size,
                                       size_t *cloned_size);
typedef AuraTaskPollState (*AuraTaskPollFn)(AuraTaskFrame *frame);
typedef AuraTaskPollState (*AuraTaskCancelFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskFrameDestroyFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskFrameDataDropFn)(AuraTaskFrame *frame, void *data,
                                        size_t size);
typedef void (*AuraTaskCleanupFn)(void *data);

typedef enum
{
  AURA_TASK_OWNED = 0,
  AURA_TASK_BORROWED = 1,
  AURA_TASK_PINNED = 2,
  AURA_TASK_SHARED = 3,
  AURA_TASK_TRANSFERRED = 4
} AuraTaskOwnership;

/* C22m: callback used for the currently supported `spawn {}` unit slice.
 * Non-empty spawned bodies still require the C22l suspension/capture lowering. */
AuraTaskPollState aura_task_poll_unit(AuraTaskFrame *frame)
{
  (void)frame;
  return AURA_TASK_COMPLETE;
}

typedef struct
{
  void *data;
  size_t size;
} AuraTaskResult;

/* A join observation is a borrowed, immutable snapshot of a terminal frame.
 * The state is authoritative: result is populated only for COMPLETE and
 * error is populated only for FAILED.  Neither payload is transferred by
 * this API; both remain owned by the frame until its handle is released or
 * the executor shuts down.  This makes repeated observations safe while
 * making use-after-release an explicit caller error. */
typedef struct
{
  AuraTaskPollState state;
  AuraTaskResult result;
  AuraTaskResult error;
} AuraTaskOutcome;

/* An owned copy of a terminal outcome.  Unlike AuraTaskOutcome, this value
 * remains valid after the task frame is released.  The caller supplies clone
 * and destroy functions because the runtime cannot infer payload ownership. */
typedef struct
{
  AuraTaskPollState state;
  AuraTaskResult result;
  AuraTaskResult error;
  AuraTaskResultDestroyFn result_destroy;
  AuraTaskResultDestroyFn error_destroy;
} AuraTaskOwnedOutcome;

int aura_task_outcome_clone(const AuraTaskOutcome *source,
                            AuraTaskResultCloneFn result_clone,
                            AuraTaskResultDestroyFn result_destroy,
                            AuraTaskResultCloneFn error_clone,
                            AuraTaskResultDestroyFn error_destroy,
                            AuraTaskOwnedOutcome *out);
void aura_task_owned_outcome_destroy(AuraTaskOwnedOutcome *out);

typedef struct
{
  uint64_t task_id;
  uint32_t source_id;
  AuraTaskPollState state;
  AuraTaskResult error;
} AuraTaskFailureDiagnostic;

typedef void (*AuraTaskFailureHookFn)(
    const AuraTaskFailureDiagnostic *diagnostic, void *context);

typedef struct
{
  void *data;
  size_t size;
  AuraTaskResultDestroyFn destroy;
  AuraTaskOwnership ownership;
  int rooted;
} AuraTaskFrameStorage;

typedef struct
{
  void *data;
  AuraTaskCleanupFn cleanup;
} AuraTaskFrameCleanup;

typedef struct AuraTaskFfiPin AuraTaskFfiPin;

struct AuraTaskFrame
{
  uint32_t abi_version;
  uint64_t task_id;
  uint32_t race_source_id;
  AuraTaskPollFn poll;
  AuraTaskCancelFn cancel;
  AuraTaskFrameDestroyFn destroy;
  void *data;
  size_t data_size;
  AuraTaskResult result;
  AuraTaskResultDestroyFn result_destroy;
  int result_rooted;
  AuraTaskFrameStorage captures;
  AuraTaskFrameStorage pending;
  AuraTaskFrameCleanup cleanup;
  AuraTaskResult error;
  AuraTaskResultCloneFn error_clone;
  AuraTaskResultDestroyFn error_destroy;
  int error_rooted;
  AuraTaskResult error_payload;
  AuraTaskResultCloneFn error_payload_clone;
  AuraTaskResultDestroyFn error_payload_destroy;
  int error_payload_rooted;
  char *error_type_name;
  uint32_t error_source_id;
  uint32_t error_span_start;
  uint32_t error_span_end;
  uint32_t resume_state;
  AuraTaskPollState state;
  int cancel_requested;
  int64_t cancel_deadline_ms;
  int join_observed;
  int failure_reported;
  int queued;
  int inline_parked;
  AuraTaskExecutor *executor;
  AuraTaskFrame *queue_next;
  AuraTaskFrame *owned_next;
  AuraTaskChannel *waiting_channel;
  void *waiting_node;
  AuraTaskSelect *waiting_select;
  int fd_wait_fd;
  short fd_wait_events;
  int fd_wait_active;
  int64_t fd_wait_deadline_ms;
  int fd_wait_timed_out;
  AuraTaskFrame *wait_target;
  AuraTaskFrame *waiters_head;
  AuraTaskFrame *waiter_next;
  AuraTaskFrame *cancel_parent;
  AuraTaskFrame *cancel_children_head;
  AuraTaskFrame *cancel_sibling_next;
  AuraTaskFrameGcMarkFn gc_mark;
  AuraTaskFrameDataDropFn data_drop;
  AuraTaskFrame *gc_next;
  AuraTaskFfiPin *ffi_pins;
  AuraTaskScope *scope;
  AuraTaskFrame *scope_next;
  int scope_owned;
  int handle_owned;
  /* Scheduler-bound payloads hold independent references to the frame. */
  size_t payload_refs;
#if defined(AURA_TCP_POSIX)
  pthread_t blocking_thread;
  pthread_mutex_t blocking_lock;
  AuraTaskBlockingFn blocking_fn;
  AuraTaskBlockingEnvDestroyFn blocking_env_destroy;
  void *blocking_env;
  int blocking_started;
  int blocking_thread_created;
  int blocking_done;
#endif
};

static AuraTaskFrame *aura_gc_task_frames = NULL;

static int aura_task_executor_has_workers(AuraTaskExecutor *executor);
int aura_task_executor_start_workers(AuraTaskExecutor *executor,
                                     size_t worker_count);

static void aura_task_frame_unlink_cancel_parent(AuraTaskFrame *frame);
static void aura_task_frame_detach_cancel_children(AuraTaskFrame *frame);
static void aura_task_frame_request_cancel_children(AuraTaskFrame *frame);

struct AuraTaskFfiPin
{
  AuraFfiHandlePin pin;
  AuraTaskFfiPin *next;
};

static void aura_task_frame_unpin_foreign_handles(AuraTaskFrame *frame)
{
  AuraTaskFfiPin *node;
  AuraTaskFfiPin *next;
  if (frame == NULL)
  {
    return;
  }
  node = frame->ffi_pins;
  frame->ffi_pins = NULL;
  while (node != NULL)
  {
    next = node->next;
    (void)aura_ffi_handle_unpin(&node->pin);
    free(node);
    node = next;
  }
}

AuraFfiStatus aura_task_frame_pin_foreign_handle(AuraTaskFrame *frame,
                                                 AuraFfiOpaqueHandle *handle,
                                                 AuraFfiBoundary boundary)
{
  AuraTaskFfiPin *node;
  AuraFfiStatus status;
  if (frame == NULL)
  {
    return AURA_FFI_INVALID;
  }
  node = (AuraTaskFfiPin *)calloc(1, sizeof(*node));
  if (node == NULL)
  {
    return AURA_FFI_OOM;
  }
  status = aura_ffi_handle_pin_for_boundary(handle, boundary, &node->pin);
  if (status != AURA_FFI_OK)
  {
    free(node);
    return status;
  }
  node->next = frame->ffi_pins;
  frame->ffi_pins = node;
  return AURA_FFI_OK;
}

static void aura_gc_mark_task_frames(void)
{
  /* Frame captures and pending payloads are allocator-owned storage rather
   * than tracing-heap nodes.  Scan their pointer-sized words conservatively
   * so a compiler frame remains a complete GC root even when it has no custom
   * mark callback.  Explicit callbacks still handle typed nested layouts and
   * remain the preferred precise path. */
  for (AuraTaskFrame *frame = aura_gc_task_frames; frame != NULL;
       frame = frame->gc_next)
  {
    const AuraTaskFrameStorage *storage[] = {&frame->captures,
                                             &frame->pending};
    for (size_t s = 0; s < sizeof(storage) / sizeof(storage[0]); s++)
    {
      const unsigned char *bytes = (const unsigned char *)storage[s]->data;
      size_t words = storage[s]->size / sizeof(void *);
      if (storage[s]->data != NULL)
      {
        aura_gc_mark_ptr(storage[s]->data);
      }
      for (size_t i = 0; bytes != NULL && i < words; i++)
      {
        void *candidate = NULL;
        memcpy(&candidate, bytes + i * sizeof(void *), sizeof(candidate));
        aura_gc_mark_ptr(candidate);
      }
    }
    /* Terminal outcomes are allocator-owned payloads too.  Scan their
     * pointer-sized words so a class/array payload remains live until the
     * owning task frame is released. */
    const AuraTaskResult *outcomes[] = {&frame->result, &frame->error,
                                        &frame->error_payload};
    for (size_t o = 0; o < sizeof(outcomes) / sizeof(outcomes[0]); o++)
    {
      const unsigned char *bytes =
          (const unsigned char *)outcomes[o]->data;
      size_t words = outcomes[o]->size / sizeof(void *);
      if (outcomes[o]->data != NULL)
      {
        aura_gc_mark_ptr(outcomes[o]->data);
      }
      for (size_t i = 0; bytes != NULL && i < words; i++)
      {
        void *candidate = NULL;
        memcpy(&candidate, bytes + i * sizeof(candidate), sizeof(candidate));
        aura_gc_mark_ptr(candidate);
      }
    }
    if (frame->gc_mark != NULL)
    {
      frame->gc_mark(frame);
    }
  }
}

static void aura_gc_unlink_task_frame(AuraTaskFrame *frame)
{
  AuraTaskFrame **link = &aura_gc_task_frames;
  while (*link != NULL)
  {
    if (*link == frame)
    {
      *link = frame->gc_next;
      frame->gc_next = NULL;
      return;
    }
    link = &(*link)->gc_next;
  }
}

static void aura_task_frame_detach_wait_target(AuraTaskFrame *frame);
static void aura_task_frame_detach_waiters(AuraTaskFrame *frame);
static void aura_task_frame_wake_waiters(AuraTaskFrame *frame);

static uint64_t aura_task_next_id = 1;
int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame);
int aura_task_executor_submit(AuraTaskExecutor *executor, AuraTaskFrame *frame);
void aura_task_frame_destroy(AuraTaskFrame *frame);

AuraTaskFrame *aura_task_frame_new(size_t data_size,
                                   AuraTaskPollFn poll,
                                   AuraTaskFrameDestroyFn destroy)
{
  if (poll == NULL)
  {
    return NULL;
  }
  AuraTaskFrame *frame = (AuraTaskFrame *)calloc(1, sizeof(*frame));
  if (frame == NULL)
  {
    return NULL;
  }
  if (data_size != 0)
  {
    /* Frame locals are the suspended task's live state.  Store them in the
     * tracing heap so the collector can deep-scan GC pointers held by the
     * state while the task is pending. */
    frame->data = aura_gc_alloc(data_size);
    if (frame->data == NULL)
    {
      free(frame);
      return NULL;
    }
    aura_gc_add_root(&frame->data);
  }
  frame->abi_version = AURA_RT_ABI_VERSION;
  frame->task_id = aura_task_next_id++;
  frame->poll = poll;
  frame->destroy = destroy;
  frame->data_size = data_size;
  frame->resume_state = 0;
  frame->state = AURA_TASK_READY;
  frame->gc_next = aura_gc_task_frames;
  aura_gc_task_frames = frame;
  return frame;
}

#if defined(AURA_TCP_POSIX)
static AuraTaskPollState aura_task_blocking_poll(AuraTaskFrame *frame)
{
  int done;
  if (frame == NULL)
  {
    return AURA_TASK_FAILED;
  }
  pthread_mutex_lock(&frame->blocking_lock);
  done = frame->blocking_done;
  pthread_mutex_unlock(&frame->blocking_lock);
  return done ? AURA_TASK_COMPLETE : AURA_TASK_PENDING;
}

static void *aura_task_blocking_thread(void *context)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)context;
  frame->blocking_fn(frame, frame->blocking_env);
  pthread_mutex_lock(&frame->blocking_lock);
  frame->blocking_done = 1;
  pthread_mutex_unlock(&frame->blocking_lock);
  if (frame->executor != NULL)
  {
    (void)aura_task_executor_wake(frame->executor, frame);
  }
  return NULL;
}

static void aura_task_blocking_run_inline(AuraTaskFrame *frame)
{
  int cancelled;
  if (frame == NULL || frame->blocking_fn == NULL) return;
  pthread_mutex_lock(&frame->blocking_lock);
  cancelled = frame->cancel_requested;
  frame->blocking_started = 1;
  pthread_mutex_unlock(&frame->blocking_lock);
  if (!cancelled)
    frame->blocking_fn(frame, frame->blocking_env);
  pthread_mutex_lock(&frame->blocking_lock);
  frame->blocking_done = 1;
  pthread_mutex_unlock(&frame->blocking_lock);
}

AuraTaskFrame *aura_task_frame_new_blocking(
    AuraTaskExecutor *executor, AuraTaskBlockingFn function, void *environment,
    AuraTaskBlockingEnvDestroyFn environment_destroy)
{
  AuraTaskFrame *frame;
  if (executor == NULL || function == NULL)
  {
    return NULL;
  }
  frame = aura_task_frame_new(0, aura_task_blocking_poll, NULL);
  if (frame == NULL || pthread_mutex_init(&frame->blocking_lock, NULL) != 0)
  {
    aura_task_frame_destroy(frame);
    return NULL;
  }
  frame->blocking_fn = function;
  frame->blocking_env = environment;
  frame->blocking_env_destroy = environment_destroy;
#if defined(AURA_TCP_POSIX)
  if (!aura_task_executor_has_workers(executor))
    (void)aura_task_executor_start_workers(executor, 4);
#endif
  if (aura_task_executor_has_workers(executor))
  {
    frame->blocking_started = 0;
    frame->blocking_thread_created = 0;
  }
  else if (pthread_create(&frame->blocking_thread, NULL, aura_task_blocking_thread,
                          frame) != 0)
  {
    frame->blocking_started = 0;
    aura_task_frame_destroy(frame);
    return NULL;
  }
  else
  {
    frame->blocking_started = 1;
    frame->blocking_thread_created = 1;
  }
  if (!aura_task_executor_submit(executor, frame))
  {
    aura_task_frame_destroy(frame);
    return NULL;
  }
  return frame;
}
#endif

void aura_task_frame_set_gc_mark(AuraTaskFrame *frame,
                                 AuraTaskFrameGcMarkFn mark)
{
  if (frame != NULL)
  {
    frame->gc_mark = mark;
  }
}

void aura_task_frame_set_data_drop(AuraTaskFrame *frame,
                                   AuraTaskFrameDataDropFn drop)
{
  if (frame != NULL)
  {
    frame->data_drop = drop;
  }
}

void aura_task_frame_set_cancel_handler(AuraTaskFrame *frame,
                                        AuraTaskCancelFn cancel)
{
  if (frame != NULL)
  {
    frame->cancel = cancel;
  }
}

void *aura_task_frame_data(AuraTaskFrame *frame)
{
  return frame != NULL ? frame->data : NULL;
}

uint64_t aura_task_frame_task_id(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->task_id : 0;
}

void aura_task_frame_set_race_source_id(AuraTaskFrame *frame, uint32_t source_id)
{
  if (frame != NULL)
  {
    frame->race_source_id = source_id;
  }
}

AuraTaskPollState aura_task_frame_state(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->state : AURA_TASK_FAILED;
}

int aura_task_frame_cancel_requested(const AuraTaskFrame *frame)
{
  return frame != NULL && frame->cancel_requested;
}

int aura_task_frame_set_cancel_deadline(AuraTaskFrame *frame, int timeout_ms)
{
  int64_t now;
  if (frame == NULL || timeout_ms < 0 ||
      frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED) {
    return 0;
  }
  now = aura_time_monotonic_millis();
  if (now <= 0 || now > INT64_MAX - timeout_ms) return 0;
  frame->cancel_deadline_ms = now + timeout_ms;
  return 1;
}

int aura_task_frame_is_waiting(const AuraTaskFrame *frame)
{
  return frame != NULL && (frame->waiting_channel != NULL || frame->waiting_node != NULL);
}

/* Adapter-owned wait registration. The token is borrowed by the frame and
 * must remain valid until the adapter clears it; the frame never frees it.
 * Completion should clear the token before calling aura_task_executor_wake.
 * Cancellation and frame destruction use the separate cleanup hook for owned
 * operation resources. */
void aura_task_frame_set_waiting(AuraTaskFrame *frame, void *token)
{
  if (frame == NULL || frame->state == AURA_TASK_COMPLETE ||
      frame->state == AURA_TASK_FAILED || frame->state == AURA_TASK_CANCELLED)
  {
    return;
  }
  frame->waiting_node = token;
  frame->fd_wait_active = 0;
  if (token != NULL)
  {
    frame->state = AURA_TASK_PENDING;
  }
}

void aura_task_frame_clear_waiting(AuraTaskFrame *frame)
{
  if (frame != NULL)
  {
    frame->waiting_node = NULL;
    frame->fd_wait_active = 0;
    frame->fd_wait_deadline_ms = 0;
  }
}

int64_t aura_time_monotonic_millis(void)
{
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0)
  {
    return 0;
  }
  return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}

void *aura_task_frame_waiting_token(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->waiting_node : NULL;
}

/* Register one borrowed POSIX descriptor readiness wait on the frame. The
 * descriptor and event mask live inline in the executor-owned frame, so no
 * adapter token allocation can outlive cancellation or destruction. A later
 * aura_task_executor_poll_waiting call performs the bounded poll and wakes the
 * frame through the same clear-before-queue protocol as other adapters. */
int aura_task_frame_wait_fd(AuraTaskFrame *frame, int fd, short events)
{
  if (frame == NULL || fd < 0 || events == 0 || frame->state == AURA_TASK_COMPLETE ||
      frame->state == AURA_TASK_FAILED || frame->state == AURA_TASK_CANCELLED ||
      frame->waiting_channel != NULL || frame->wait_target != NULL ||
      frame->fd_wait_active)
  {
    return 0;
  }
  frame->fd_wait_fd = fd;
  frame->fd_wait_events = events;
  frame->fd_wait_active = 1;
  frame->fd_wait_deadline_ms = 0;
  frame->fd_wait_timed_out = 0;
  frame->waiting_node = &frame->fd_wait_active;
  frame->state = AURA_TASK_PENDING;
  return 1;
}

/* A readiness timeout wakes the frame instead of cancelling it.  The polling
 * operation consumes the timeout marker and chooses its typed outcome. */
int aura_task_frame_wait_fd_timeout(AuraTaskFrame *frame, int fd, short events,
                                    int timeout_ms)
{
  int64_t now;
  if (timeout_ms < 0 || !aura_task_frame_wait_fd(frame, fd, events))
  {
    return 0;
  }
  now = aura_time_monotonic_millis();
  if (now <= 0 || now > INT64_MAX - timeout_ms)
  {
    aura_task_frame_clear_waiting(frame);
    return 0;
  }
  frame->fd_wait_deadline_ms = now + timeout_ms;
  return 1;
}

int aura_task_frame_take_fd_wait_timeout(AuraTaskFrame *frame)
{
  int timed_out = frame != NULL && frame->fd_wait_timed_out;
  if (frame != NULL)
  {
    frame->fd_wait_timed_out = 0;
  }
  return timed_out;
}

/* Park a task until its monotonic deadline without occupying a descriptor.
 * The executor's normal readiness poll also services these timer waits. */
int aura_task_frame_wait_deadline(AuraTaskFrame *frame, int timeout_ms)
{
  int64_t now;
  if (frame == NULL || timeout_ms < 0 || frame->state == AURA_TASK_COMPLETE ||
      frame->state == AURA_TASK_FAILED || frame->state == AURA_TASK_CANCELLED ||
      frame->waiting_node != NULL || frame->waiting_channel != NULL ||
      frame->wait_target != NULL)
  {
    return 0;
  }
  now = aura_time_monotonic_millis();
  if (now <= 0 || now > INT64_MAX - timeout_ms)
  {
    return 0;
  }
  frame->fd_wait_active = 0;
  frame->fd_wait_deadline_ms = now + timeout_ms;
  frame->fd_wait_timed_out = 0;
  frame->waiting_node = &frame->fd_wait_timed_out;
  frame->state = AURA_TASK_PENDING;
  return 1;
}

int aura_task_frame_wait_file(AuraTaskFrame *frame, const AuraFile *file,
                              short events)
{
  if (file == NULL || file->closed)
  {
    return 0;
  }
  return aura_task_frame_wait_fd(frame, file->fd, events);
}

/* TCP adapters keep resource ownership in the existing listener/stream
 * objects and only borrow their nonblocking descriptor for this wait. */
int aura_task_frame_wait_tcp_listener(AuraTaskFrame *frame,
                                      const AuraTcpListener *listener,
                                      short events)
{
  if (listener == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_fd(frame, listener->fd, events);
}

int aura_task_frame_wait_tcp_stream(AuraTaskFrame *frame,
                                    const AuraTcpStream *stream,
                                    short events)
{
  if (stream == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_fd(frame, stream->fd, events);
}

int aura_task_frame_wait_tcp_stream_timeout(AuraTaskFrame *frame,
                                            const AuraTcpStream *stream,
                                            short events, int timeout_ms)
{
  if (stream == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_fd_timeout(frame, stream->fd, events, timeout_ms);
}

int aura_http_connection_wait_write(AuraTaskFrame *frame,
                                    const AuraHttpConnection *connection)
{
  if (connection == NULL || connection->stream == NULL)
  {
    return 0;
  }
  return aura_task_frame_wait_tcp_stream_timeout(
      frame, connection->stream, POLLOUT, connection->config.write_timeout_ms);
}

/* The body reader is connection-owned, so only it may choose the stream and
 * deadline used to resume a generated RequestBody task. */
int aura_http_request_wait_body(AuraTaskFrame *frame,
                                const AuraHttpRequest *request)
{
  AuraHttpContentLengthReader *reader;
  if (frame == NULL || request == NULL || request->body_reader == NULL)
  {
    return 0;
  }
  reader = request->body_reader;
  return aura_task_frame_wait_tcp_stream_timeout(
      frame, reader->stream, POLLIN, reader->timeout_ms);
}

enum
{
  AURA_HTTP_ASYNC_READ = 1,
  AURA_HTTP_ASYNC_WRITE = 2,
  AURA_HTTP_ASYNC_HANDLER = 3
};

void aura_task_frame_set_cleanup(AuraTaskFrame *frame, void *data,
                                 AuraTaskCleanupFn cleanup);
void aura_task_frame_clear_cleanup(AuraTaskFrame *frame);

static void aura_http_connection_async_reset(AuraHttpConnection *connection)
{
  if (connection == NULL)
  {
    return;
  }
  free(connection->async_buffer);
  connection->async_buffer = NULL;
  connection->async_used = 0;
  connection->async_capacity = 0;
  if (connection->async_response_active)
  {
    aura_http_response_destroy(&connection->async_response);
    connection->async_response_active = 0;
  }
  if (connection->async_request_active)
  {
    aura_http_request_destroy(&connection->async_request);
    connection->async_request_active = 0;
  }
  memset(&connection->async_body_reader, 0,
         sizeof(connection->async_body_reader));
  connection->async_body_reader_active = 0;
  free(connection->async_output);
  connection->async_output = NULL;
  connection->async_output_length = 0;
  connection->async_output_offset = 0;
  connection->async_handler = NULL;
  connection->async_task_handler = NULL;
  connection->async_user_data = NULL;
  connection->async_active = 0;
  connection->async_phase = 0;
  connection->async_close_after_write = 0;
  connection->async_handler_started = 0;
}

static void aura_http_connection_async_release_handle_pin(
    AuraHttpConnection *connection)
{
  if (connection != NULL && connection->async_handle_pin_active)
  {
    AuraFfiHandlePin pin = connection->async_handle_pin;
    /* Unpin may destroy the connection after the lexical owner was dropped;
     * clear connection state before invoking that destructor. */
    memset(&connection->async_handle_pin, 0,
           sizeof(connection->async_handle_pin));
    connection->async_handle_pin_active = 0;
    connection->async_handle_frame = NULL;
    (void)aura_ffi_handle_unpin(&pin);
  }
}

static void aura_http_connection_async_cleanup(void *data)
{
  AuraHttpConnection *connection = (AuraHttpConnection *)data;
  if (connection == NULL)
  {
    return;
  }
  (void)aura_http_connection_close(connection);
  aura_http_connection_async_reset(connection);
  aura_http_connection_async_release_handle_pin(connection);
}

static AuraTaskPollState aura_http_connection_async_failure(
    AuraTaskFrame *frame, AuraHttpConnection *connection)
{
  aura_task_frame_clear_cleanup(frame);
  (void)aura_http_connection_close(connection);
  aura_http_connection_async_reset(connection);
  aura_http_connection_async_release_handle_pin(connection);
  return AURA_TASK_FAILED;
}

static int aura_http_connection_async_prepare_response(AuraHttpConnection *connection,
                                                       const AuraHttpRequest *request)
{
  AuraHttpHandlerResult handler_result = AURA_HTTP_HANDLER_CLOSE;
  int request_close = aura_http_connection_header_has(request, "close");
  int status_code = 0;
  const char *error_code = NULL;
  size_t required = 0;
  AuraHttpResponseStatus response_status;

  aura_http_response_init(&connection->async_response);
  connection->async_response_active = 1;
  if (request == NULL)
  {
    status_code = 400;
    error_code = "bad_request";
  }
  else
  {
    if (!request_close && aura_http_response_set_connection(
                              &connection->async_response,
                              AURA_HTTP_RESPONSE_KEEP_ALIVE) != AURA_HTTP_RESPONSE_OK)
    {
      return 0;
    }
    handler_result = connection->async_handler(
        request, &connection->async_response, connection->async_user_data);
    if (handler_result == AURA_HTTP_HANDLER_ERROR)
    {
      status_code = 500;
      error_code = "handler_failure";
    }
  }
  if (status_code != 0)
  {
    aura_http_response_destroy(&connection->async_response);
    aura_http_response_init(&connection->async_response);
    response_status = aura_http_response_set_error(
        &connection->async_response, status_code, error_code);
    if (response_status != AURA_HTTP_RESPONSE_OK)
    {
      return 0;
    }
  }
  connection->async_close_after_write =
      status_code != 0 || request_close || handler_result == AURA_HTTP_HANDLER_CLOSE ||
      connection->async_response.connection == AURA_HTTP_RESPONSE_CLOSE ||
      connection->requests_served + 1 >= connection->config.max_requests;
  if (connection->async_close_after_write &&
      aura_http_response_set_connection(&connection->async_response,
                                        AURA_HTTP_RESPONSE_CLOSE) !=
          AURA_HTTP_RESPONSE_OK)
  {
    return 0;
  }
  response_status = aura_http_response_serialize(
      &connection->async_response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    return 0;
  }
  connection->async_output = (char *)malloc(required);
  if (connection->async_output == NULL)
  {
    return 0;
  }
  connection->async_output_length = required;
  response_status = aura_http_response_serialize(
      &connection->async_response, connection->async_output, required,
      &connection->async_output_length);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    return 0;
  }
  connection->async_output_offset = 0;
  connection->async_phase = AURA_HTTP_ASYNC_WRITE;
  return 1;
}

/* Application and request failures write one bounded response before closing.
 * The request stays connection-owned until that response has been drained. */
static AuraTaskPollState aura_http_connection_async_prepare_error_response(
    AuraHttpConnection *connection, int status_code, const char *error_code)
{
  size_t required = 0;

  if (connection == NULL)
  {
    return AURA_TASK_FAILED;
  }
  if (connection->async_response_active)
  {
    aura_http_response_destroy(&connection->async_response);
  }
  aura_http_response_init(&connection->async_response);
  connection->async_response_active = 1;
  if (aura_http_response_set_error(&connection->async_response, status_code,
                                   error_code) != AURA_HTTP_RESPONSE_OK ||
      aura_http_response_set_connection(&connection->async_response,
                                        AURA_HTTP_RESPONSE_CLOSE) !=
          AURA_HTTP_RESPONSE_OK ||
      aura_http_response_serialize(&connection->async_response, NULL, 0,
                                   &required) !=
          AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL ||
      required == 0)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output = (char *)malloc(required);
  if (connection->async_output == NULL ||
      aura_http_response_serialize(&connection->async_response,
                                   connection->async_output, required,
                                   &connection->async_output_length) !=
          AURA_HTTP_RESPONSE_OK)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output_offset = 0;
  connection->async_close_after_write = 1;
  connection->async_phase = AURA_HTTP_ASYNC_WRITE;
  return AURA_TASK_READY;
}

static AuraTaskPollState aura_http_connection_async_prepare_handler_failure(
    AuraHttpConnection *connection)
{
  return aura_http_connection_async_prepare_error_response(
      connection, 500, "handler_failure");
}

/* Prepare and resume a task-backed handler.  Request/response objects remain
 * connection-owned while this function returns PENDING, so a generated Aura
 * handler can suspend on any runtime readiness source without borrowing a
 * stack object across the suspension. */
static AuraTaskPollState aura_http_connection_async_run_task_handler(
    AuraTaskFrame *frame, AuraHttpConnection *connection)
{
  AuraTaskPollState state;
  AuraHttpResponseStatus response_status;
  size_t required = 0;

  if (frame == NULL || connection == NULL || connection->async_task_handler == NULL ||
      !connection->async_request_active)
  {
    return AURA_TASK_FAILED;
  }
  if (!connection->async_handler_started)
  {
    aura_http_response_init(&connection->async_response);
    connection->async_response_active = 1;
    if (!aura_http_connection_header_has(&connection->async_request, "close") &&
        aura_http_response_set_connection(&connection->async_response,
                                          AURA_HTTP_RESPONSE_KEEP_ALIVE) !=
            AURA_HTTP_RESPONSE_OK)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_handler_started = 1;
  }
  state = connection->async_task_handler(
      frame, &connection->async_request, &connection->async_response,
      connection->async_user_data);
  if (state == AURA_TASK_PENDING)
  {
    connection->async_phase = AURA_HTTP_ASYNC_HANDLER;
    return state;
  }
  if (state != AURA_TASK_COMPLETE)
  {
    if (connection->async_response.streaming_committed)
    {
      return AURA_TASK_FAILED;
    }
    return aura_http_connection_async_prepare_handler_failure(connection);
  }
  connection->async_close_after_write =
      aura_http_connection_header_has(&connection->async_request, "close") ||
      (connection->async_body_reader_active &&
       !aura_http_body_reader_complete(&connection->async_body_reader)) ||
      connection->async_response.connection == AURA_HTTP_RESPONSE_CLOSE ||
      connection->requests_served + 1 >= connection->config.max_requests;
  if (connection->async_response.streaming_committed)
  {
    if (aura_http_response_stream_finish(&connection->async_response, NULL, 0,
                                         &required) != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL ||
        required == 0)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_output = (char *)malloc(required);
    if (connection->async_output == NULL ||
        aura_http_response_stream_finish(&connection->async_response,
                                         connection->async_output, required,
                                         &connection->async_output_length) !=
            AURA_HTTP_RESPONSE_OK)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_output_offset = 0;
    connection->async_phase = AURA_HTTP_ASYNC_WRITE;
    return AURA_TASK_READY;
  }
  if (connection->async_close_after_write &&
      aura_http_response_set_connection(&connection->async_response,
                                        AURA_HTTP_RESPONSE_CLOSE) !=
          AURA_HTTP_RESPONSE_OK)
  {
    return AURA_TASK_FAILED;
  }
  response_status = aura_http_response_serialize(
      &connection->async_response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output = (char *)malloc(required);
  if (connection->async_output == NULL)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output_length = required;
  response_status = aura_http_response_serialize(
      &connection->async_response, connection->async_output, required,
      &connection->async_output_length);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    return AURA_TASK_FAILED;
  }
  connection->async_output_offset = 0;
  connection->async_phase = AURA_HTTP_ASYNC_WRITE;
  return AURA_TASK_READY;
}

/* H5 bridge: request/response storage stays connection-owned while the task is
 * pending. The handler remains synchronous in this runtime ABI, but reads and
 * writes are independently readiness-driven. A successful response may keep
 * the connection alive; cancellation and every terminal path use the armed
 * frame cleanup hook for exactly-once close and buffer release. */
AuraTaskPollState aura_http_connection_poll_async(AuraTaskFrame *frame,
                                                  AuraHttpConnection *connection,
                                                  AuraHttpHandler handler,
                                                  void *user_data)
{
  if (frame == NULL || connection == NULL || connection->stream == NULL ||
      connection->closed || (handler == NULL && connection->async_task_handler == NULL))
  {
    return AURA_TASK_FAILED;
  }
  if (!connection->async_active)
  {
    connection->async_buffer = (unsigned char *)malloc(4096);
    if (connection->async_buffer == NULL)
    {
      return AURA_TASK_FAILED;
    }
    connection->async_capacity = 4096;
    if (handler != NULL)
    {
      connection->async_handler = handler;
      connection->async_user_data = user_data;
    }
    connection->async_active = 1;
    connection->async_phase = AURA_HTTP_ASYNC_READ;
    if (frame->cleanup.data != connection ||
        frame->cleanup.cleanup != aura_http_connection_async_cleanup)
    {
      aura_task_frame_set_cleanup(frame, connection,
                                  aura_http_connection_async_cleanup);
    }
  }
  if (aura_task_frame_take_fd_wait_timeout(frame))
  {
    if (connection->async_phase != AURA_HTTP_ASYNC_READ ||
        aura_http_connection_async_prepare_error_response(
            connection, 408, "request_timeout") != AURA_TASK_READY)
    {
      return aura_http_connection_async_failure(frame, connection);
    }
  }
  for (;;)
  {
    if (connection->async_phase == AURA_HTTP_ASYNC_READ)
    {
      AuraHttpRequest request;
      size_t consumed = 0;
      size_t header_end = 0;
      size_t content_length = 0;
      int chunked = 0;
      AuraHttpParseStatus parse_status;
      for (;;)
      {
        if (connection->async_task_handler != NULL)
        {
          parse_status = aura_http_request_parse_headers(
              connection->async_buffer, connection->async_used, &request,
              &header_end, &content_length, &chunked);
          if (parse_status == AURA_HTTP_PARSE_OK && content_length == 0 &&
              !chunked)
          {
            aura_http_request_destroy(&request);
            parse_status = aura_http_request_parse(
                connection->async_buffer, connection->async_used, &request,
                &consumed);
          }
        }
        else
        {
          parse_status = aura_http_request_parse(connection->async_buffer,
                                                 connection->async_used, &request,
                                                 &consumed);
        }
        if (parse_status != AURA_HTTP_PARSE_INCOMPLETE)
        {
          break;
        }
        if (connection->async_used == AURA_HTTP_MAX_TOTAL_BYTES)
        {
          parse_status = AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
          break;
        }
        if (connection->async_used == connection->async_capacity)
        {
          size_t next = connection->async_capacity * 2;
          unsigned char *grown;
          if (next > AURA_HTTP_MAX_TOTAL_BYTES)
          {
            next = AURA_HTTP_MAX_TOTAL_BYTES;
          }
          if (next <= connection->async_capacity)
          {
            parse_status = AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
            break;
          }
          grown = (unsigned char *)realloc(connection->async_buffer, next);
          if (grown == NULL)
          {
            return aura_http_connection_async_failure(frame, connection);
          }
          connection->async_buffer = grown;
          connection->async_capacity = next;
        }
        {
          size_t received = 0;
          AuraTcpStatus status = aura_tcp_stream_read(
              connection->stream, connection->async_buffer + connection->async_used,
              connection->async_capacity - connection->async_used, &received, 0);
          if (status == AURA_TCP_PENDING)
          {
            if (!aura_task_frame_wait_tcp_stream_timeout(
                    frame, connection->stream, POLLIN,
                    aura_http_min_timeout(connection->config.read_timeout_ms,
                                          connection->config.idle_timeout_ms)))
            {
              return aura_http_connection_async_failure(frame, connection);
            }
            return AURA_TASK_PENDING;
          }
          if (status == AURA_TCP_EOF || status != AURA_TCP_OK || received == 0)
          {
            return aura_http_connection_async_failure(frame, connection);
          }
          connection->async_used += received;
        }
      }
      if (parse_status == AURA_HTTP_PARSE_OK)
      {
        if (connection->async_task_handler != NULL &&
            ((!chunked && content_length != 0) || chunked))
        {
          consumed = header_end;
        }
        if (consumed == 0 || consumed > connection->async_used)
        {
          aura_http_request_destroy(&request);
          return aura_http_connection_async_failure(frame, connection);
        }
        memmove(connection->async_buffer, connection->async_buffer + consumed,
                connection->async_used - consumed);
        connection->async_used -= consumed;
        if (connection->async_task_handler != NULL)
        {
          connection->async_request = request;
          memset(&request, 0, sizeof(request));
          connection->async_request_active = 1;
          if (!chunked && content_length != 0)
          {
            if (!aura_http_content_length_reader_init(
                    &connection->async_body_reader, connection->stream,
                    connection->async_buffer, &connection->async_used,
                    content_length, connection->config.read_timeout_ms))
            {
              return aura_http_connection_async_failure(frame, connection);
            }
            connection->async_body_reader_active = 1;
            connection->async_request.body_reader = &connection->async_body_reader;
          }
          else if (chunked)
          {
            if (!aura_http_chunked_reader_init(
                    &connection->async_body_reader, connection->stream,
                    connection->async_buffer, &connection->async_used,
                    connection->config.read_timeout_ms))
            {
              return aura_http_connection_async_failure(frame, connection);
            }
            connection->async_body_reader_active = 1;
            connection->async_request.body_reader = &connection->async_body_reader;
          }
          AuraTaskPollState handler_state =
              aura_http_connection_async_run_task_handler(frame, connection);
          if (handler_state == AURA_TASK_PENDING)
          {
            return AURA_TASK_PENDING;
          }
          if (handler_state == AURA_TASK_FAILED)
          {
            return aura_http_connection_async_failure(frame, connection);
          }
        }
        else
        {
          if (!aura_http_connection_async_prepare_response(connection, &request))
          {
            aura_http_request_destroy(&request);
            return aura_http_connection_async_failure(frame, connection);
          }
          aura_http_request_destroy(&request);
        }
      }
      else
      {
        int status_code = parse_status == AURA_HTTP_PARSE_METHOD_NOT_ALLOWED
                              ? 405
                              : parse_status == AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE
                                    ? 413
                                    : 400;
        const char *error_code = status_code == 405
                                     ? "method_not_allowed"
                                     : status_code == 413 ? "payload_too_large" : "bad_request";
        size_t required = 0;
        aura_http_response_init(&connection->async_response);
        connection->async_response_active = 1;
        if (aura_http_response_set_error(&connection->async_response, status_code,
                                         error_code) != AURA_HTTP_RESPONSE_OK ||
            aura_http_response_serialize(&connection->async_response, NULL, 0,
                                         &required) != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL ||
            required == 0)
        {
          return aura_http_connection_async_failure(frame, connection);
        }
        connection->async_output = (char *)malloc(required);
        if (connection->async_output == NULL ||
            aura_http_response_serialize(&connection->async_response,
                                         connection->async_output, required,
                                         &required) != AURA_HTTP_RESPONSE_OK)
        {
          return aura_http_connection_async_failure(frame, connection);
        }
        connection->async_output_length = required;
        connection->async_output_offset = 0;
        connection->async_close_after_write = 1;
        connection->async_phase = AURA_HTTP_ASYNC_WRITE;
      }
    }
    if (connection->async_phase == AURA_HTTP_ASYNC_HANDLER)
    {
      AuraTaskPollState handler_state =
          aura_http_connection_async_run_task_handler(frame, connection);
      if (handler_state == AURA_TASK_PENDING)
      {
        return AURA_TASK_PENDING;
      }
      if (handler_state == AURA_TASK_FAILED)
      {
        return aura_http_connection_async_failure(frame, connection);
      }
    }
    if (connection->async_phase == AURA_HTTP_ASYNC_WRITE)
    {
      while (connection->async_output_offset < connection->async_output_length)
      {
        size_t written = 0;
        AuraTcpStatus status = aura_tcp_stream_write(
            connection->stream,
            connection->async_output + connection->async_output_offset,
            connection->async_output_length - connection->async_output_offset,
            &written, 0);
        if (status == AURA_TCP_PENDING)
        {
          if (!aura_task_frame_wait_tcp_stream_timeout(
                  frame, connection->stream, POLLOUT,
                  connection->config.write_timeout_ms))
          {
            return aura_http_connection_async_failure(frame, connection);
          }
          return AURA_TASK_PENDING;
        }
        if (status != AURA_TCP_OK || written == 0)
        {
          return aura_http_connection_async_failure(frame, connection);
        }
        connection->async_output_offset += written;
      }
      connection->requests_served++;
      free(connection->async_output);
      connection->async_output = NULL;
      connection->async_output_length = 0;
      connection->async_output_offset = 0;
      if (connection->async_response_active)
      {
        aura_http_response_destroy(&connection->async_response);
        connection->async_response_active = 0;
      }
      if (connection->async_request_active)
      {
        connection->async_request.body_reader = NULL;
        aura_http_request_destroy(&connection->async_request);
        connection->async_request_active = 0;
      }
      memset(&connection->async_body_reader, 0,
             sizeof(connection->async_body_reader));
      connection->async_body_reader_active = 0;
      connection->async_handler_started = 0;
      if (connection->async_close_after_write)
      {
        int defer_handle_release = connection->async_handle_pin_active;
        if (!defer_handle_release)
        {
          aura_task_frame_clear_cleanup(frame);
        }
        (void)aura_http_connection_close(connection);
        aura_http_connection_async_reset(connection);
        if (!defer_handle_release)
        {
          aura_http_connection_async_release_handle_pin(connection);
        }
        return AURA_TASK_COMPLETE;
      }
      connection->async_close_after_write = 0;
      connection->async_phase = AURA_HTTP_ASYNC_READ;
    }
  }
}

/* HTTP-001 task boundary for compiler-generated handlers.  The supplied
 * handler is called on the connection task's frame, so its ordinary Aura
 * await lowering can use the same readiness/cancellation machinery as any
 * other async function. */
AuraTaskPollState aura_http_connection_poll_async_task(
    AuraTaskFrame *frame, AuraHttpConnection *connection,
    AuraHttpTaskHandler handler, void *user_data)
{
  if (frame == NULL || connection == NULL || handler == NULL ||
      (connection->async_active && connection->async_task_handler != handler))
  {
    return AURA_TASK_FAILED;
  }
  if (!connection->async_active)
  {
    connection->async_task_handler = handler;
    connection->async_user_data = user_data;
  }
  return aura_http_connection_poll_async(frame, connection, NULL, NULL);
}

AuraTaskPollState aura_http_connection_poll_async_task_handle(
    AuraTaskFrame *frame, AuraFfiOpaqueHandle *handle,
    AuraHttpTaskHandler handler, void *user_data)
{
  AuraFfiHandlePin pin;
  AuraHttpConnection *connection;
  if (frame == NULL || handle == NULL || handler == NULL)
  {
    return AURA_TASK_FAILED;
  }
  if (handle->resource != NULL && handle->pins != 0 &&
      !handle->destroyed)
  {
    connection = (AuraHttpConnection *)handle->resource;
    if (connection->async_handle_pin_active)
    {
      if (connection->async_handle_frame != frame)
      {
        return AURA_TASK_FAILED;
      }
      return aura_http_connection_poll_async_task(frame, connection, handler,
                                                   user_data);
    }
  }
  memset(&pin, 0, sizeof(pin));
  if (aura_ffi_handle_pin_for_boundary(handle, AURA_FFI_BOUNDARY_TASK, &pin) !=
      AURA_FFI_OK)
  {
    return AURA_TASK_FAILED;
  }
  connection = (AuraHttpConnection *)pin.resource;
  if (connection == NULL || connection->closed || connection->stream == NULL ||
      connection->async_handle_pin_active)
  {
    (void)aura_ffi_handle_unpin(&pin);
    return AURA_TASK_FAILED;
  }
  connection->async_handle_pin = pin;
  connection->async_handle_pin_active = 1;
  connection->async_handle_frame = frame;
  /* Arm cleanup before the first async initialization allocation.  The
   * connection poller normally installs this hook after allocating its
   * buffer, but a handle-backed task owns this pin immediately; an early
   * allocation/initialization failure must release it as well. */
  aura_task_frame_set_cleanup(frame, connection,
                              aura_http_connection_async_cleanup);
  return aura_http_connection_poll_async_task(frame, connection, handler,
                                               user_data);
}

uint32_t aura_task_frame_resume_state(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->resume_state : 0;
}

void aura_task_frame_set_resume_state(AuraTaskFrame *frame, uint32_t state)
{
  if (frame != NULL)
  {
    frame->resume_state = state;
  }
}

AuraTaskFrameStorage aura_task_frame_captures(const AuraTaskFrame *frame)
{
  AuraTaskFrameStorage empty = {NULL, 0, NULL, AURA_TASK_OWNED, 0};
  return frame != NULL ? frame->captures : empty;
}

static void aura_task_frame_storage_release(AuraTaskFrameStorage *storage)
{
  void *data;
  size_t size;
  AuraTaskResultDestroyFn destroy;

  if (storage == NULL)
  {
    return;
  }
  if (storage->rooted)
  {
    aura_gc_remove_root(&storage->data);
  }

  /* Clear the slot before invoking user cleanup.  Besides making the
   * release operation idempotent, this keeps a re-entrant cleanup callback
   * from observing a live ownership record after its root was removed. */
  data = storage->data;
  size = storage->size;
  destroy = storage->destroy;
  *storage = (AuraTaskFrameStorage){NULL, 0, NULL, AURA_TASK_OWNED, 0};

  if (destroy != NULL && data != NULL)
  {
    destroy(data, size);
  }
}

static int aura_task_frame_storage_set(AuraTaskFrameStorage *storage,
                                       void *data,
                                       size_t size,
                                       AuraTaskResultDestroyFn destroy,
                                       AuraTaskOwnership ownership)
{
  if (storage == NULL || ownership == AURA_TASK_BORROWED)
  {
    return 0;
  }
  aura_task_frame_storage_release(storage);
  *storage = (AuraTaskFrameStorage){data, size, destroy, ownership, 0};
  if (data != NULL)
  {
    aura_gc_add_root(&storage->data);
    storage->rooted = 1;
  }
  return 1;
}

void aura_task_frame_set_captures(AuraTaskFrame *frame,
                                  void *data,
                                  size_t size,
                                  AuraTaskResultDestroyFn destroy)
{
  if (frame != NULL)
  {
    (void)aura_task_frame_storage_set(
        &frame->captures, data, size, destroy, AURA_TASK_OWNED);
  }
}

int aura_task_frame_set_captures_with_ownership(AuraTaskFrame *frame,
                                                void *data,
                                                size_t size,
                                                AuraTaskResultDestroyFn destroy,
                                                AuraTaskOwnership ownership)
{
  return frame != NULL ? aura_task_frame_storage_set(
                             &frame->captures, data, size, destroy, ownership)
                       : 0;
}

AuraTaskFrameStorage aura_task_frame_pending(const AuraTaskFrame *frame)
{
  AuraTaskFrameStorage empty = {NULL, 0, NULL, AURA_TASK_OWNED, 0};
  return frame != NULL ? frame->pending : empty;
}

/* A frame-scoped cleanup is the bounded bridge between a pending I/O
 * operation and the task lifecycle.  The callback owns the resource while
 * armed; clearing the slot before invoking it makes cancellation, failure,
 * and shutdown cleanup re-entrant and exactly-once. */
static void aura_task_frame_cleanup_run(AuraTaskFrame *frame)
{
  void *data;
  AuraTaskCleanupFn cleanup;

  if (frame == NULL || frame->cleanup.cleanup == NULL)
  {
    return;
  }
  data = frame->cleanup.data;
  cleanup = frame->cleanup.cleanup;
  frame->cleanup = (AuraTaskFrameCleanup){NULL, NULL};
  if (cleanup != NULL && data != NULL)
  {
    cleanup(data);
  }
}

void aura_task_frame_set_cleanup(AuraTaskFrame *frame,
                                  void *data,
                                  AuraTaskCleanupFn cleanup)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_frame_cleanup_run(frame);
  frame->cleanup = (AuraTaskFrameCleanup){data, cleanup};
}

void aura_task_frame_clear_cleanup(AuraTaskFrame *frame)
{
  if (frame != NULL)
  {
    frame->cleanup = (AuraTaskFrameCleanup){NULL, NULL};
  }
}

void aura_task_frame_set_pending(AuraTaskFrame *frame,
                                 void *data,
                                 size_t size,
                                 AuraTaskResultDestroyFn destroy)
{
  if (frame != NULL)
  {
    (void)aura_task_frame_storage_set(
        &frame->pending, data, size, destroy, AURA_TASK_TRANSFERRED);
    if (data != NULL)
    {
      frame->state = AURA_TASK_PENDING;
    }
  }
}

int aura_task_frame_set_pending_with_ownership(AuraTaskFrame *frame,
                                               void *data,
                                               size_t size,
                                               AuraTaskResultDestroyFn destroy,
                                               AuraTaskOwnership ownership)
{
  if (frame == NULL || !aura_task_frame_storage_set(
                           &frame->pending, data, size, destroy, ownership))
  {
    return 0;
  }
  if (data != NULL)
  {
    frame->state = AURA_TASK_PENDING;
  }
  return 1;
}

AuraTaskOwnership aura_task_frame_capture_ownership(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->captures.ownership : AURA_TASK_BORROWED;
}

AuraTaskOwnership aura_task_frame_pending_ownership(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->pending.ownership : AURA_TASK_BORROWED;
}

AuraTaskResult aura_task_frame_error(const AuraTaskFrame *frame)
{
  AuraTaskResult empty = {NULL, 0};
  return frame != NULL ? frame->error : empty;
}

AuraTaskResult aura_task_frame_error_payload(const AuraTaskFrame *frame)
{
  AuraTaskResult empty = {NULL, 0};
  return frame != NULL ? frame->error_payload : empty;
}

const char *aura_task_frame_error_type_name(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_type_name : NULL;
}

uint32_t aura_task_frame_error_source_id(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_source_id : 0;
}

uint32_t aura_task_frame_error_span_start(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_span_start : 0;
}

uint32_t aura_task_frame_error_span_end(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->error_span_end : 0;
}

static void aura_task_result_release(AuraTaskResult *result,
                                     AuraTaskResultCloneFn *clone,
                                     AuraTaskResultDestroyFn *destroy,
                                     int *rooted)
{
  void *data;
  size_t size;
  AuraTaskResultDestroyFn drop;

  if (result == NULL || rooted == NULL)
  {
    return;
  }
  if (*rooted)
  {
    aura_gc_remove_root(&result->data);
  }
  data = result->data;
  size = result->size;
  drop = destroy != NULL ? *destroy : NULL;
  *result = (AuraTaskResult){NULL, 0};
  if (clone != NULL)
  {
    *clone = NULL;
  }
  if (destroy != NULL)
  {
    *destroy = NULL;
  }
  *rooted = 0;
  if (drop != NULL && data != NULL)
  {
    drop(data, size);
  }
}

static void aura_task_frame_clear_error_type_name(AuraTaskFrame *frame)
{
  if (frame == NULL)
  {
    return;
  }
  free(frame->error_type_name);
  frame->error_type_name = NULL;
}

void aura_task_frame_set_error_type_name(AuraTaskFrame *frame,
                                         const char *type_name)
{
  size_t length;
  char *copy;
  if (frame == NULL)
  {
    return;
  }
  aura_task_frame_clear_error_type_name(frame);
  if (type_name == NULL)
  {
    return;
  }
  length = strlen(type_name);
  copy = (char *)malloc(length + 1);
  if (copy == NULL)
  {
    abort();
  }
  memcpy(copy, type_name, length + 1);
  frame->error_type_name = copy;
}

void aura_task_frame_set_error_span_with_clone(
    AuraTaskFrame *frame, void *data, size_t size, AuraTaskResultCloneFn clone,
    AuraTaskResultDestroyFn destroy, uint32_t source_id, uint32_t span_start,
    uint32_t span_end)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_result_release(&frame->error_payload,
                           &frame->error_payload_clone,
                           &frame->error_payload_destroy,
                           &frame->error_payload_rooted);
  aura_task_result_release(&frame->error, &frame->error_clone,
                           &frame->error_destroy,
                           &frame->error_rooted);
  aura_task_frame_clear_error_type_name(frame);
  frame->error = (AuraTaskResult){data, size};
  frame->error_clone = clone;
  frame->error_destroy = destroy;
  frame->error_source_id = source_id;
  frame->error_span_start = span_start;
  frame->error_span_end = span_end;
  if (data != NULL)
  {
    aura_gc_add_root(&frame->error.data);
    frame->error_rooted = 1;
    frame->state = AURA_TASK_FAILED;
  }
}

void aura_task_frame_set_error_payload_with_clone(
    AuraTaskFrame *frame, void *data, size_t size,
    AuraTaskResultCloneFn clone, AuraTaskResultDestroyFn destroy)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_result_release(&frame->error_payload,
                           &frame->error_payload_clone,
                           &frame->error_payload_destroy,
                           &frame->error_payload_rooted);
  frame->error_payload = (AuraTaskResult){data, size};
  frame->error_payload_clone = clone;
  frame->error_payload_destroy = destroy;
  if (data != NULL)
  {
    aura_gc_add_root(&frame->error_payload.data);
    frame->error_payload_rooted = 1;
  }
}

void aura_task_frame_set_error_span(AuraTaskFrame *frame,
                                    void *data,
                                    size_t size,
                                    AuraTaskResultDestroyFn destroy,
                                    uint32_t source_id,
                                    uint32_t span_start,
                                    uint32_t span_end)
{
  aura_task_frame_set_error_span_with_clone(
      frame, data, size, NULL, destroy, source_id, span_start, span_end);
}

void aura_task_frame_set_error_at(AuraTaskFrame *frame,
                                  void *data,
                                  size_t size,
                                  AuraTaskResultDestroyFn destroy,
                                  uint32_t source_id)
{
  aura_task_frame_set_error_span(frame, data, size, destroy, source_id,
                                 source_id, source_id);
}

void aura_task_frame_set_error(AuraTaskFrame *frame,
                               void *data,
                               size_t size,
                               AuraTaskResultDestroyFn destroy)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_frame_set_error_at(
      frame, data, size, destroy, frame->race_source_id);
}

static void aura_task_error_copy_destroy(void *data, size_t size)
{
  (void)size;
  free(data);
}

static void *aura_task_error_shallow_clone(const void *data, size_t size,
                                           size_t *cloned_size)
{
  void *copy;
  if (cloned_size == NULL)
  {
    return NULL;
  }
  copy = malloc(size == 0 ? 1 : size);
  if (copy == NULL)
  {
    return NULL;
  }
  if (size != 0 && data != NULL)
  {
    memcpy(copy, data, size);
  }
  *cloned_size = size;
  return copy;
}

/* Propagate an error with an explicit payload clone.  The clone callback is
 * responsible for recursively copying owned fields; the destroy callback is
 * responsible for releasing that independent copy.  This keeps the runtime
 * generic while allowing generated code to preserve nested String/Array
 * ownership across an async child-to-parent boundary. */
int aura_task_frame_propagate_error_with_clone(
    AuraTaskFrame *frame, const AuraTaskFrame *source,
    AuraTaskResultCloneFn clone, AuraTaskResultDestroyFn destroy)
{
  AuraTaskResult error;
  size_t cloned_size = 0;
  void *copy;

  if (frame == NULL || source == NULL || clone == NULL ||
      source->state != AURA_TASK_FAILED || source->error.data == NULL)
  {
    return 0;
  }
  error = source->error;
  copy = clone(error.data, error.size, &cloned_size);
  if (copy == NULL)
  {
    return 0;
  }
  aura_task_frame_set_error_span_with_clone(
      frame, copy, cloned_size, clone, destroy, source->error_source_id,
      source->error_span_start, source->error_span_end);
  return 1;
}

/* Copy a terminal child error into its waiting parent before the parent
 * publishes AURA_TASK_FAILED. The child remains executor-owned and retains
 * its original payload/source ID; the parent receives an independent payload
 * so either frame may be released independently. */
int aura_task_frame_propagate_error(AuraTaskFrame *frame,
                                    const AuraTaskFrame *source)
{
  int propagated;
  if (source != NULL && source->error_clone != NULL)
  {
    propagated = aura_task_frame_propagate_error_with_clone(
        frame, source, source->error_clone, source->error_destroy);
  }
  else
  {
    propagated = aura_task_frame_propagate_error_with_clone(
        frame, source, aura_task_error_shallow_clone,
        aura_task_error_copy_destroy);
  }
  if (!propagated || source == NULL || source->error_payload.data == NULL)
  {
    return propagated;
  }
  if (source->error_payload_clone == NULL ||
      source->error_payload_destroy == NULL)
  {
    return 0;
  }
  size_t cloned_size = 0;
  void *copy = source->error_payload_clone(
      source->error_payload.data, source->error_payload.size, &cloned_size);
  if (copy == NULL)
  {
    return 0;
  }
  aura_task_frame_set_error_payload_with_clone(
      frame, copy, cloned_size, source->error_payload_clone,
      source->error_payload_destroy);
  aura_task_frame_set_error_type_name(frame, source->error_type_name);
  return 1;
}

void aura_task_frame_set_result(AuraTaskFrame *frame,
                                void *data,
                                size_t size,
                                AuraTaskResultDestroyFn destroy);

/* Publish a child's complete terminal outcome into its waiting parent. A
 * payload is copied only through the caller-supplied clone/destroy pair;
 * cancellation has no payload and is forwarded as a cancellation request. */
AuraTaskPollState aura_task_frame_propagate_outcome(
    AuraTaskFrame *frame, const AuraTaskFrame *source,
    AuraTaskResultCloneFn result_clone, AuraTaskResultDestroyFn result_destroy)
{
  size_t cloned_size = 0;
  void *copy;

  if (frame == NULL || source == NULL ||
      (source->state != AURA_TASK_COMPLETE &&
       source->state != AURA_TASK_FAILED &&
       source->state != AURA_TASK_CANCELLED))
  {
    return AURA_TASK_FAILED;
  }
  if (source->state == AURA_TASK_CANCELLED)
  {
    frame->cancel_requested = 1;
    frame->state = AURA_TASK_CANCELLED;
    return AURA_TASK_CANCELLED;
  }
  if (source->state == AURA_TASK_FAILED)
  {
    (void)aura_task_frame_propagate_error(frame, source);
    return AURA_TASK_FAILED;
  }
  if (source->result.data == NULL)
  {
    aura_task_frame_set_result(frame, NULL, 0, NULL);
    return AURA_TASK_COMPLETE;
  }
  if (result_clone == NULL)
  {
    return AURA_TASK_FAILED;
  }
  copy = result_clone(source->result.data, source->result.size, &cloned_size);
  if (copy == NULL)
  {
    return AURA_TASK_FAILED;
  }
  aura_task_frame_set_result(frame, copy, cloned_size, result_destroy);
  return AURA_TASK_COMPLETE;
}

void aura_task_frame_set_result(AuraTaskFrame *frame,
                                void *data,
                                size_t size,
                                AuraTaskResultDestroyFn destroy)
{
  if (frame == NULL)
  {
    return;
  }
  aura_task_result_release(&frame->result, NULL, &frame->result_destroy,
                           &frame->result_rooted);
  frame->result.data = data;
  frame->result.size = size;
  frame->result_destroy = destroy;
  if (data != NULL)
  {
    aura_gc_add_root(&frame->result.data);
    frame->result_rooted = 1;
  }
}

AuraTaskResult aura_task_frame_result(const AuraTaskFrame *frame)
{
  AuraTaskResult empty = {NULL, 0};
  return frame != NULL ? frame->result : empty;
}

static void *aura_type_erased_result_clone(const void *raw, size_t size,
                                           size_t *out_size)
{
  const AuraTypeErasedValue *source = (const AuraTypeErasedValue *)raw;
  AuraTypeErasedValue *copy;
  if (raw == NULL || size != sizeof(*source) || out_size == NULL)
  {
    return NULL;
  }
  copy = (AuraTypeErasedValue *)calloc(1, sizeof(*copy));
  if (copy == NULL || aura_type_erased_clone(source, copy) != AURA_FFI_OK)
  {
    free(copy);
    return NULL;
  }
  *out_size = sizeof(*copy);
  return copy;
}

static void aura_type_erased_result_destroy(void *raw, size_t size)
{
  if (raw != NULL && size == sizeof(AuraTypeErasedValue))
  {
    AuraTypeErasedValue *value = (AuraTypeErasedValue *)raw;
    aura_type_erased_drop(value);
  }
  free(raw);
}

AuraFfiStatus aura_task_frame_set_erased_result(
    AuraTaskFrame *frame, const AuraTypeErasedValue *value)
{
  void *copy;
  size_t size = 0;
  if (frame == NULL || value == NULL)
  {
    return AURA_FFI_INVALID;
  }
  copy = aura_type_erased_result_clone(value, sizeof(*value), &size);
  if (copy == NULL)
  {
    return AURA_FFI_OOM;
  }
  aura_task_frame_set_result(frame, copy, size,
                             aura_type_erased_result_destroy);
  return AURA_FFI_OK;
}

AuraFfiStatus aura_task_frame_result_erased(const AuraTaskFrame *frame,
                                            AuraTypeErasedValue *out)
{
  AuraTaskResult result;
  if (frame == NULL || out == NULL ||
      aura_task_frame_state(frame) != AURA_TASK_COMPLETE)
  {
    return AURA_FFI_INVALID;
  }
  result = aura_task_frame_result(frame);
  if (result.data == NULL || result.size != sizeof(AuraTypeErasedValue))
  {
    return AURA_FFI_INVALID;
  }
  /* Retrieval is clone-out: the terminal frame owns its result until it is
   * released, so callers must not receive a borrowed descriptor payload. */
  return aura_type_erased_clone((const AuraTypeErasedValue *)result.data, out);
}

void aura_task_frame_destroy(AuraTaskFrame *frame)
{
  if (frame == NULL)
  {
    return;
  }
#if defined(AURA_TCP_POSIX)
  if (frame->blocking_thread_created)
  {
    pthread_join(frame->blocking_thread, NULL);
    frame->blocking_started = 0;
    frame->blocking_thread_created = 0;
  }
  if (frame->blocking_env_destroy != NULL && frame->blocking_env != NULL)
  {
    frame->blocking_env_destroy(frame->blocking_env);
    frame->blocking_env = NULL;
  }
#endif
  aura_gc_unlink_task_frame(frame);
  aura_task_frame_unlink_cancel_parent(frame);
  aura_task_frame_detach_cancel_children(frame);
  aura_task_frame_detach_wait_target(frame);
  aura_task_frame_detach_waiters(frame);
  aura_task_frame_clear_waiting(frame);
  aura_task_frame_cleanup_run(frame);
  if (frame->destroy != NULL)
  {
    frame->destroy(frame);
  }
  if (frame->data_drop != NULL)
  {
    frame->data_drop(frame, frame->data, frame->data_size);
  }
  aura_task_frame_unpin_foreign_handles(frame);
  aura_task_result_release(&frame->result, NULL, &frame->result_destroy,
                           &frame->result_rooted);
  aura_task_frame_storage_release(&frame->captures);
  aura_task_frame_storage_release(&frame->pending);
  aura_task_result_release(&frame->error, &frame->error_clone,
                           &frame->error_destroy,
                           &frame->error_rooted);
  aura_task_result_release(&frame->error_payload,
                           &frame->error_payload_clone,
                           &frame->error_payload_destroy,
                           &frame->error_payload_rooted);
  aura_task_frame_clear_error_type_name(frame);
  if (frame->data != NULL)
  {
    aura_gc_remove_root(&frame->data);
    aura_gc_release(frame->data);
    frame->data = NULL;
  }
#if defined(AURA_TCP_POSIX)
  if (frame->blocking_fn != NULL)
  {
    pthread_mutex_destroy(&frame->blocking_lock);
  }
#endif
  free(frame);
}
