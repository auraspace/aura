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
typedef void (*AuraTaskFrameGcMarkFn)(AuraTaskFrame *frame);
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

AuraTaskPollState aura_task_executor_join(AuraTaskExecutor *executor,
                                          AuraTaskFrame *frame,
                                          AuraTaskResult *out_result,
                                          AuraTaskResult *out_error);
int aura_task_executor_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame);
int aura_task_executor_release(AuraTaskExecutor *executor,
                               AuraTaskFrame **handle);

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
  const AuraTaskFrameGcSlot *gc_stack_map;
  size_t gc_stack_map_len;
  const AuraTaskFrameGcSlot *captures_gc_stack_map;
  size_t captures_gc_stack_map_len;
  const AuraTaskFrameGcSlot *result_gc_stack_map;
  size_t result_gc_stack_map_len;
  const AuraTaskFrameGcSlot *error_gc_stack_map;
  size_t error_gc_stack_map_len;
  const AuraTaskFrameGcSlot *error_payload_gc_stack_map;
  size_t error_payload_gc_stack_map_len;
  AuraTaskFrameDataDropFn data_drop;
  AuraTaskFrame *gc_next;
  AuraTaskFfiPin *ffi_pins;
  AuraTaskScope *scope;
  AuraTaskFrame *scope_next;
  int scope_owned;
  int handle_owned;
  /* Scheduler-bound payloads hold independent references to the frame. */
  size_t payload_refs;
#if AURA_PLATFORM_NETWORK
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

static void aura_gc_mark_task_frame_unlocked(AuraTaskFrame *frame)
{
  /* Every allocator-owned frame storage area needs an explicit map.  The
   * compiler supplies callbacks for nested layouts; raw offsets cover simple
   * pointer slots without treating arbitrary bytes as references. */
  if (frame == NULL)
  {
    return;
  }
  {
    const AuraTaskFrameStorage *storage = &frame->captures;
    const AuraTaskFrameGcSlot *slots = frame->captures_gc_stack_map;
    size_t slot_count = frame->captures_gc_stack_map_len;
    for (size_t i = 0; storage->data != NULL && i < slot_count; i++)
    {
      size_t offset = slots[i].offset;
      if (offset <= storage->size && sizeof(void *) <= storage->size - offset)
      {
        void *candidate = NULL;
        memcpy(&candidate, (const unsigned char *)storage->data + offset,
               sizeof(candidate));
        aura_gc_mark_ptr(candidate);
      }
    }
  }
  {
    const AuraTaskResult *outcomes[] = {&frame->result, &frame->error,
                                        &frame->error_payload};
    const AuraTaskFrameGcSlot *maps[] = {frame->result_gc_stack_map,
                                         frame->error_gc_stack_map,
                                         frame->error_payload_gc_stack_map};
    const size_t map_lens[] = {frame->result_gc_stack_map_len,
                               frame->error_gc_stack_map_len,
                               frame->error_payload_gc_stack_map_len};
    for (size_t o = 0; o < sizeof(outcomes) / sizeof(outcomes[0]); o++)
    {
      for (size_t i = 0; outcomes[o]->data != NULL && i < map_lens[o]; i++)
      {
        size_t offset = maps[o][i].offset;
        if (offset <= outcomes[o]->size &&
            sizeof(void *) <= outcomes[o]->size - offset)
        {
          void *candidate = NULL;
          memcpy(&candidate, (const unsigned char *)outcomes[o]->data + offset,
                 sizeof(candidate));
          aura_gc_mark_ptr(candidate);
        }
      }
    }
    if (frame->data != NULL && frame->gc_stack_map != NULL)
    {
      for (size_t i = 0; i < frame->gc_stack_map_len; i++)
      {
        size_t offset = frame->gc_stack_map[i].offset;
        if (offset <= frame->data_size && sizeof(void *) <= frame->data_size - offset)
        {
          void *candidate = NULL;
          memcpy(&candidate, (const unsigned char *)frame->data + offset,
                 sizeof(candidate));
          aura_gc_mark_ptr(candidate);
        }
      }
    }
    if (frame->gc_mark != NULL)
    {
      frame->gc_mark(frame);
    }
  }
}

static void aura_gc_mark_task_frames(void)
{
  for (AuraTaskFrame *frame = aura_gc_task_frames; frame != NULL;
       frame = frame->gc_next)
  {
    aura_gc_mark_task_frame_unlocked(frame);
  }
}

/* Refresh a running frame at a scheduler boundary while concurrent marking
 * is active. The poller has finished mutating the frame before this call. */
void aura_gc_mark_task_frame_safepoint(AuraTaskFrame *frame)
{
  aura_gc_lock_enter();
  aura_gc_mark_task_frame_unlocked(frame);
  aura_gc_lock_leave();
}

static void aura_gc_unlink_task_frame(AuraTaskFrame *frame)
{
  aura_gc_lock_enter();
  AuraTaskFrame **link = &aura_gc_task_frames;
  while (*link != NULL)
  {
    if (*link == frame)
    {
      *link = frame->gc_next;
      frame->gc_next = NULL;
      aura_gc_lock_leave();
      return;
    }
    link = &(*link)->gc_next;
  }
  aura_gc_lock_leave();
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
  aura_gc_lock_enter();
  frame->gc_next = aura_gc_task_frames;
  aura_gc_task_frames = frame;
  aura_gc_lock_leave();
  return frame;
}

#if AURA_PLATFORM_NETWORK
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
#if AURA_PLATFORM_NETWORK
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

/* LLVM std.udp receive bridge. The generated backend supplies the concrete
 * class type ids, while this runtime owns the readiness loop and socket data. */
#if defined(AURA_LLVM_RUNTIME)
extern int aura_task_frame_wait_fd(AuraTaskFrame *frame, int fd, short events);
extern void aura_task_frame_set_result(AuraTaskFrame *frame, void *data, size_t size,
                                       AuraTaskResultDestroyFn destroy);
extern void aura_task_frame_set_error_at(AuraTaskFrame *frame, void *data, size_t size,
                                         AuraTaskResultDestroyFn destroy, uint32_t source_id);
extern int64_t aura_io_read_fd(int fd, void *buffer, uint64_t capacity);
extern int64_t aura_io_write_fd(int fd, const void *buffer, uint64_t length);
extern void *aura_llvm_str_data(void *value);
extern void *aura_llvm_str_new(const char *source);
extern void aura_llvm_str_release(void *value);

typedef struct
{
  int fd;
  int write_mode;
  int64_t capacity;
  int64_t offset;
  char *buffer;
  int outcome_mode;
  int64_t ok_tag;
  int64_t err_tag;
  void *ok_destructor;
  void *err_destructor;
} AuraLlvmFdIoData;

static void aura_llvm_fd_io_destroy(AuraTaskFrame *frame)
{
  AuraLlvmFdIoData *data = (AuraLlvmFdIoData *)aura_task_frame_data(frame);
  if (data != NULL) free(data->buffer);
}

static void aura_llvm_fd_outcome_destroy(void *raw, size_t size);
static void aura_llvm_fd_error_destroy(void *raw, size_t size);

static void aura_llvm_fd_io_set_error(AuraTaskFrame *frame, const char *message)
{
  size_t length = strlen(message) + 1;
  char *copy = (char *)malloc(length);
  if (copy == NULL) return;
  memcpy(copy, message, length);
  aura_task_frame_set_error_at(frame, copy, length, aura_llvm_fd_error_destroy, 0);
}

static void aura_llvm_fd_error_destroy(void *raw, size_t size)
{
  (void)size;
  free(raw);
}

static void aura_llvm_fd_string_destroy(void *raw, size_t size)
{
  (void)size;
  if (raw != NULL)
  {
    aura_llvm_str_release(*(void **)raw);
    free(raw);
  }
}

static void aura_llvm_fd_i64_destroy(void *raw, size_t size)
{
  (void)size;
  free(raw);
}

extern void *aura_llvm_enum_alloc(int64_t fields, void *destructor);
extern void aura_llvm_enum_release(void *value);

static int aura_llvm_fd_set_outcome(AuraTaskFrame *frame, int64_t tag,
                                    int64_t raw, void *destructor)
{
  void **slot = (void **)malloc(sizeof(*slot));
  void *outcome;
  if (slot == NULL) return 0;
  outcome = aura_llvm_enum_alloc(1, destructor);
  if (outcome == NULL) { free(slot); return 0; }
  ((int64_t *)outcome)[1] = tag;
  ((int64_t *)outcome)[3] = raw;
  *slot = outcome;
  aura_task_frame_set_result(frame, slot, sizeof(*slot), aura_llvm_fd_outcome_destroy);
  return 1;
}

static void aura_llvm_fd_outcome_destroy(void *raw, size_t size)
{
  (void)size;
  if (raw != NULL)
  {
    aura_llvm_enum_release(*(void **)raw);
    free(raw);
  }
}

static int aura_llvm_fd_set_error_outcome(AuraTaskFrame *frame, AuraLlvmFdIoData *data)
{
  void *error = aura_llvm_str_new(data->write_mode ? "writeFd failed" : "readFd failed");
  if (error == NULL) return 0;
  if (!aura_llvm_fd_set_outcome(frame, data->err_tag, (int64_t)(uintptr_t)error,
                                data->err_destructor))
  {
    aura_llvm_str_release(error);
    return 0;
  }
  return 1;
}

static AuraTaskPollState aura_llvm_fd_io_poll(AuraTaskFrame *frame)
{
  AuraLlvmFdIoData *data = (AuraLlvmFdIoData *)aura_task_frame_data(frame);
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->write_mode)
  {
    int64_t count = aura_io_write_fd(data->fd, data->buffer + data->offset,
                                     (uint64_t)(data->capacity - data->offset));
    if (count < 0)
    {
      if (count == -EAGAIN || count == -EWOULDBLOCK)
      {
        if (!aura_task_frame_wait_fd(frame, data->fd, POLLOUT)) return AURA_TASK_FAILED;
        return AURA_TASK_PENDING;
      }
      if (data->outcome_mode) return aura_llvm_fd_set_error_outcome(frame, data) ? AURA_TASK_COMPLETE : AURA_TASK_FAILED;
      aura_llvm_fd_io_set_error(frame, "writeFd failed"); return AURA_TASK_FAILED;
    }
    if (count == 0)
    {
      if (data->outcome_mode) return aura_llvm_fd_set_error_outcome(frame, data) ? AURA_TASK_COMPLETE : AURA_TASK_FAILED;
      aura_llvm_fd_io_set_error(frame, "writeFd failed"); return AURA_TASK_FAILED;
    }
    data->offset += count;
    if (data->offset < data->capacity)
    {
      if (!aura_task_frame_wait_fd(frame, data->fd, POLLOUT)) return AURA_TASK_FAILED;
      return AURA_TASK_PENDING;
    }
    int64_t *result = (int64_t *)malloc(sizeof(*result));
    if (result == NULL) return AURA_TASK_FAILED;
    *result = data->offset;
    if (data->outcome_mode)
    {
      int complete = aura_llvm_fd_set_outcome(frame, data->ok_tag, *result, data->ok_destructor);
      free(result);
      return complete ? AURA_TASK_COMPLETE : AURA_TASK_FAILED;
    }
    aura_task_frame_set_result(frame, result, sizeof(*result), aura_llvm_fd_i64_destroy);
    return AURA_TASK_COMPLETE;
  }

  int64_t count = aura_io_read_fd(data->fd, data->buffer, (uint64_t)data->capacity);
  if (count < 0)
  {
    if (count == -EAGAIN || count == -EWOULDBLOCK)
    {
      if (!aura_task_frame_wait_fd(frame, data->fd, POLLIN)) return AURA_TASK_FAILED;
      return AURA_TASK_PENDING;
    }
    if (data->outcome_mode) return aura_llvm_fd_set_error_outcome(frame, data) ? AURA_TASK_COMPLETE : AURA_TASK_FAILED;
    aura_llvm_fd_io_set_error(frame, "readFd failed"); return AURA_TASK_FAILED;
  }
  data->buffer[count] = '\0';
  void **result = (void **)malloc(sizeof(*result));
  if (result == NULL) return AURA_TASK_FAILED;
  *result = aura_llvm_str_new(data->buffer);
  if (*result == NULL) { free(result); return AURA_TASK_FAILED; }
  if (data->outcome_mode)
  {
    int complete = aura_llvm_fd_set_outcome(frame, data->ok_tag, (int64_t)(uintptr_t)*result, data->ok_destructor);
    free(result);
    return complete ? AURA_TASK_COMPLETE : AURA_TASK_FAILED;
  }
  aura_task_frame_set_result(frame, result, sizeof(*result), aura_llvm_fd_string_destroy);
  return AURA_TASK_COMPLETE;
}

void *aura_llvm_io_read_fd_task(void *executor, int64_t fd, int64_t capacity)
{
  AuraTaskFrame *frame;
  AuraLlvmFdIoData *data;
  if (executor == NULL || fd < 0 || capacity <= 0) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_fd_io_poll, aura_llvm_fd_io_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmFdIoData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->fd = (int)fd; data->capacity = capacity;
  data->buffer = (char *)malloc((size_t)capacity + 1);
  if (data->buffer == NULL || !aura_task_executor_submit((AuraTaskExecutor *)executor, frame))
  { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

void *aura_llvm_io_write_fd_task(void *executor, int64_t fd, void *content)
{
  AuraTaskFrame *frame;
  AuraLlvmFdIoData *data;
  const char *text = (const char *)aura_llvm_str_data(content);
  if (executor == NULL || fd < 0 || text == NULL) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_fd_io_poll, aura_llvm_fd_io_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmFdIoData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->fd = (int)fd; data->write_mode = 1;
  data->capacity = (int64_t)strlen(text); data->buffer = strdup(text);
  if (data->buffer == NULL || !aura_task_executor_submit((AuraTaskExecutor *)executor, frame))
  { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

void *aura_llvm_io_read_fd_result_task(void *executor, int64_t fd, int64_t capacity,
                                       int64_t ok_tag, int64_t err_tag,
                                       void *ok_destructor, void *err_destructor)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)aura_llvm_io_read_fd_task(executor, fd, capacity);
  AuraLlvmFdIoData *data;
  if (frame == NULL) return NULL;
  data = (AuraLlvmFdIoData *)aura_task_frame_data(frame);
  data->outcome_mode = 1; data->ok_tag = ok_tag; data->err_tag = err_tag;
  data->ok_destructor = ok_destructor; data->err_destructor = err_destructor;
  return frame;
}

void *aura_llvm_io_write_fd_result_task(void *executor, int64_t fd, void *content,
                                        int64_t ok_tag, int64_t err_tag,
                                        void *ok_destructor, void *err_destructor)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)aura_llvm_io_write_fd_task(executor, fd, content);
  AuraLlvmFdIoData *data;
  if (frame == NULL) return NULL;
  data = (AuraLlvmFdIoData *)aura_task_frame_data(frame);
  data->outcome_mode = 1; data->ok_tag = ok_tag; data->err_tag = err_tag;
  data->ok_destructor = ok_destructor; data->err_destructor = err_destructor;
  return frame;
}

extern void *aura_llvm_class_alloc(uint64_t field_count, uint64_t type_id);
extern void *aura_llvm_str_new(const char *value);
extern void aura_llvm_class_release(void *value);
uint32_t aura_task_frame_resume_state(const AuraTaskFrame *frame);
void aura_task_frame_set_resume_state(AuraTaskFrame *frame, uint32_t state);
void aura_task_frame_set_result(AuraTaskFrame *frame, void *data, size_t size,
                                AuraTaskResultDestroyFn destroy);

typedef struct
{
  char *host;
  int64_t port;
  int64_t capacity;
  uint64_t endpoint_type;
  uint64_t datagram_type;
} AuraLlvmUdpReceiveData;

static void aura_llvm_udp_receive_destroy(AuraTaskFrame *frame)
{
  AuraLlvmUdpReceiveData *data = (AuraLlvmUdpReceiveData *)aura_task_frame_data(frame);
  if (data != NULL) free(data->host);
}

static void aura_llvm_udp_receive_result_destroy(void *raw, size_t size)
{
  (void)size;
  if (raw != NULL)
  {
    void *value = *(void **)raw;
    aura_llvm_class_release(value);
    free(raw);
  }
}

static AuraTaskPollState aura_llvm_udp_receive_poll(AuraTaskFrame *frame)
{
  AuraLlvmUdpReceiveData *data = (AuraLlvmUdpReceiveData *)aura_task_frame_data(frame);
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (aura_task_frame_resume_state(frame) == 0)
  {
    if (data->capacity <= 0 || !aura_udp_bind(data->host, data->port)) return AURA_TASK_FAILED;
    aura_task_frame_set_resume_state(frame, 1);
  }
  if (!aura_udp_wait(data->host, data->port, 0))
  {
    if (!aura_task_frame_wait_deadline(frame, 1)) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  int64_t source_port = 0;
  const char *source_host = NULL;
  const char *payload = aura_udp_receive(data->host, data->port, data->capacity,
                                         &source_port, &source_host);
  if (payload == NULL || source_host == NULL) return AURA_TASK_FAILED;
  void *source = aura_llvm_class_alloc(2, data->endpoint_type);
  void *source_name = aura_llvm_str_new(source_host);
  void *body = aura_llvm_str_new(payload);
  free((void *)source_host);
  free((void *)payload);
  if (source == NULL || source_name == NULL || body == NULL) return AURA_TASK_FAILED;
  ((uint64_t *)source)[1] = (uint64_t)(uintptr_t)source_name;
  ((uint64_t *)source)[2] = (uint64_t)source_port;
  void *result = aura_llvm_class_alloc(2, data->datagram_type);
  if (result == NULL) return AURA_TASK_FAILED;
  ((uint64_t *)result)[1] = (uint64_t)(uintptr_t)source;
  ((uint64_t *)result)[2] = (uint64_t)(uintptr_t)body;
  void **result_slot = (void **)malloc(sizeof(*result_slot));
  if (result_slot == NULL) return AURA_TASK_FAILED;
  *result_slot = result;
  aura_task_frame_set_result(frame, result_slot, sizeof(*result_slot), aura_llvm_udp_receive_result_destroy);
  return AURA_TASK_COMPLETE;
}

AuraTaskFrame *aura_llvm_udp_receive_task(AuraTaskExecutor *executor, const char *host,
                                          int64_t port, int64_t capacity,
                                          uint64_t endpoint_type, uint64_t datagram_type)
{
  AuraTaskFrame *frame;
  AuraLlvmUdpReceiveData *data;
  if (executor == NULL || host == NULL) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_udp_receive_poll,
                              aura_llvm_udp_receive_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmUdpReceiveData *)aura_task_frame_data(frame);
  data->host = strdup(host);
  data->port = port;
  data->capacity = capacity;
  data->endpoint_type = endpoint_type;
  data->datagram_type = datagram_type;
  if (data->host == NULL || !aura_task_executor_submit(executor, frame))
  {
    aura_task_frame_destroy(frame);
    return NULL;
  }
  return frame;
}
#endif

#if defined(AURA_LLVM_RUNTIME)
extern void *aura_llvm_str_data(void *value);
extern void *aura_llvm_str_new(const char *value);
extern void aura_llvm_str_release(void *value);
extern void *aura_llvm_enum_alloc(int64_t fields, void *destructor);
extern void aura_llvm_enum_release(void *value);
int aura_task_frame_wait_tcp_stream(AuraTaskFrame *frame,
                                    const AuraTcpStream *stream,
                                    short events);

static void aura_llvm_destroy_tcp_stream(void *resource)
{
  if (resource != NULL) aura_tcp_stream_destroy((AuraTcpStream *)resource);
}

static void aura_llvm_destroy_tcp_listener(void *resource)
{
  if (resource != NULL) aura_tcp_listener_destroy((AuraTcpListener *)resource);
}

void *aura_llvm_net_listen(void *endpoint)
{
  AuraTcpListener *listener = NULL;
  AuraFfiOpaqueHandle *handle = NULL;
  const char *text = (const char *)aura_llvm_str_data(endpoint);
  uint16_t port = 0;
  if (text == NULL || aura_tcp_listener_bind_endpoint(text, &port, &listener) != AURA_TCP_OK ||
      listener == NULL || aura_ffi_handle_new(listener, aura_llvm_destroy_tcp_listener, &handle) != AURA_FFI_OK)
  {
    if (listener != NULL) aura_tcp_listener_destroy(listener);
    return NULL;
  }
  return handle;
}

void *aura_llvm_net_connect(void *endpoint, int64_t timeout_ms)
{
  AuraTcpStream *stream = NULL;
  AuraFfiOpaqueHandle *handle = NULL;
  const char *text = (const char *)aura_llvm_str_data(endpoint);
  if (text == NULL || timeout_ms < 0 ||
      aura_tcp_stream_connect_endpoint(text, (int)timeout_ms, &stream) != AURA_TCP_OK ||
      stream == NULL || aura_ffi_handle_new(stream, aura_llvm_destroy_tcp_stream, &handle) != AURA_FFI_OK)
  {
    if (stream != NULL) aura_tcp_stream_destroy(stream);
    return NULL;
  }
  return handle;
}

int32_t aura_llvm_net_close_listener(void *handle)
{
  AuraFfiHandlePin pin;
  int32_t result;
  if (handle == NULL || aura_ffi_handle_pin((AuraFfiOpaqueHandle *)handle, &pin) != AURA_FFI_OK)
    return 0;
  result = aura_tcp_listener_close((AuraTcpListener *)pin.resource);
  (void)aura_ffi_handle_unpin(&pin);
  return result == 0 ? 1 : 0;
}

int32_t aura_llvm_net_close_stream(void *handle)
{
  AuraFfiHandlePin pin;
  int32_t result;
  if (handle == NULL || aura_ffi_handle_pin((AuraFfiOpaqueHandle *)handle, &pin) != AURA_FFI_OK)
    return 0;
  result = aura_tcp_stream_close((AuraTcpStream *)pin.resource);
  (void)aura_ffi_handle_unpin(&pin);
  return result == 0 ? 1 : 0;
}

typedef struct
{
  AuraFfiOpaqueHandle *handle;
  AuraFfiHandlePin pin;
  int64_t capacity;
  int64_t offset;
  char *buffer;
  int write_mode;
  int outcome_mode;
  int64_t outcome_tag;
  void *outcome_destructor;
} AuraLlvmNetIoData;

static void aura_llvm_net_io_destroy(AuraTaskFrame *frame)
{
  AuraLlvmNetIoData *data = (AuraLlvmNetIoData *)aura_task_frame_data(frame);
  if (data != NULL)
  {
    free(data->buffer);
    if (data->pin.handle != NULL) (void)aura_ffi_handle_unpin(&data->pin);
  }
}

static void aura_llvm_net_io_result_destroy(void *raw, size_t size)
{
  (void)size;
  if (raw != NULL)
  {
    aura_llvm_str_release(*(void **)raw);
    free(raw);
  }
}

static void aura_llvm_net_i64_destroy(void *raw, size_t size)
{
  (void)size;
  free(raw);
}

static void aura_llvm_net_outcome_destroy(void *raw, size_t size)
{
  (void)size;
  if (raw != NULL)
  {
    aura_llvm_enum_release(*(void **)raw);
    free(raw);
  }
}

static AuraTaskPollState aura_llvm_net_io_poll(AuraTaskFrame *frame)
{
  AuraLlvmNetIoData *data = (AuraLlvmNetIoData *)aura_task_frame_data(frame);
  size_t count = 0;
  AuraTcpStatus status;
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->pin.handle == NULL)
  {
    if (data->handle == NULL || aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK)
      return AURA_TASK_FAILED;
    if (!data->write_mode)
    {
      data->buffer = (char *)malloc((size_t)data->capacity + 1);
      if (data->buffer == NULL) return AURA_TASK_FAILED;
    }
  }
  if (data->write_mode)
    status = aura_tcp_stream_write((AuraTcpStream *)data->pin.resource, data->buffer + data->offset,
                                   (size_t)(data->capacity - data->offset), &count, 0);
  else
    status = aura_tcp_stream_read((AuraTcpStream *)data->pin.resource, data->buffer,
                                  (size_t)data->capacity, &count, 0);
  if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT)
  {
    if (!aura_task_frame_wait_tcp_stream(frame, (const AuraTcpStream *)data->pin.resource,
                                         data->write_mode ? 4 : 1)) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  if (status != AURA_TCP_OK && !(status == AURA_TCP_EOF && !data->write_mode)) return AURA_TASK_FAILED;
  if (data->write_mode)
  {
    data->offset += (int64_t)count;
    if (data->offset < data->capacity) return count == 0 ? AURA_TASK_FAILED : AURA_TASK_PENDING;
    int64_t *result = (int64_t *)malloc(sizeof(*result));
    if (result == NULL) return AURA_TASK_FAILED;
    *result = data->offset;
    if (data->outcome_mode)
    {
      void **outcome_slot = (void **)malloc(sizeof(*outcome_slot));
      void *outcome;
      if (outcome_slot == NULL) { free(result); return AURA_TASK_FAILED; }
      outcome = aura_llvm_enum_alloc(1, data->outcome_destructor);
      if (outcome == NULL) { free(outcome_slot); free(result); return AURA_TASK_FAILED; }
      ((int64_t *)outcome)[1] = data->outcome_tag;
      ((int64_t *)outcome)[3] = *result;
      *outcome_slot = outcome;
      free(result);
      aura_task_frame_set_result(frame, outcome_slot, sizeof(*outcome_slot), aura_llvm_net_outcome_destroy);
    }
    else aura_task_frame_set_result(frame, result, sizeof(*result), aura_llvm_net_i64_destroy);
  }
  else
  {
    void **result = (void **)malloc(sizeof(*result));
    if (result == NULL) return AURA_TASK_FAILED;
    data->buffer[count] = '\0';
    *result = aura_llvm_str_new(data->buffer);
    if (*result == NULL) { free(result); return AURA_TASK_FAILED; }
    if (data->outcome_mode)
    {
      void **outcome_slot = (void **)malloc(sizeof(*outcome_slot));
      void *outcome;
      if (outcome_slot == NULL) { free(*result); free(result); return AURA_TASK_FAILED; }
      outcome = aura_llvm_enum_alloc(1, data->outcome_destructor);
      if (outcome == NULL) { free(outcome_slot); free(*result); free(result); return AURA_TASK_FAILED; }
      ((int64_t *)outcome)[1] = data->outcome_tag;
      ((int64_t *)outcome)[3] = (int64_t)(uintptr_t)*result;
      *outcome_slot = outcome;
      free(result);
      aura_task_frame_set_result(frame, outcome_slot, sizeof(*outcome_slot), aura_llvm_net_outcome_destroy);
    }
    else aura_task_frame_set_result(frame, result, sizeof(*result), aura_llvm_net_io_result_destroy);
  }
  return AURA_TASK_COMPLETE;
}

void *aura_llvm_net_read_task(void *executor, void *handle, int64_t capacity, int64_t unused)
{
  AuraTaskFrame *frame;
  AuraLlvmNetIoData *data;
  (void)unused;
  if (executor == NULL || handle == NULL || capacity <= 0) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_net_io_poll, aura_llvm_net_io_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmNetIoData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->handle = (AuraFfiOpaqueHandle *)handle; data->capacity = capacity;
  if (!aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

void *aura_llvm_net_write_task(void *executor, void *handle, void *content, int64_t unused)
{
  AuraTaskFrame *frame;
  AuraLlvmNetIoData *data;
  const char *text = (const char *)aura_llvm_str_data(content);
  (void)unused;
  if (executor == NULL || handle == NULL || text == NULL) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_net_io_poll, aura_llvm_net_io_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmNetIoData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->handle = (AuraFfiOpaqueHandle *)handle;
  data->capacity = (int64_t)strlen(text); data->write_mode = 1; data->buffer = strdup(text);
  if (data->buffer == NULL || !aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

void *aura_llvm_net_read_result_task(void *executor, void *handle, int64_t capacity,
                                     int64_t tag, void *destructor)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)aura_llvm_net_read_task(executor, handle, capacity, 0);
  AuraLlvmNetIoData *data;
  if (frame == NULL) return NULL;
  data = (AuraLlvmNetIoData *)aura_task_frame_data(frame);
  data->outcome_mode = 1; data->outcome_tag = tag; data->outcome_destructor = destructor;
  return frame;
}

void *aura_llvm_net_write_result_task(void *executor, void *handle, void *content,
                                      int64_t tag, void *destructor)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)aura_llvm_net_write_task(executor, handle, content, 0);
  AuraLlvmNetIoData *data;
  if (frame == NULL) return NULL;
  data = (AuraLlvmNetIoData *)aura_task_frame_data(frame);
  data->outcome_mode = 1; data->outcome_tag = tag; data->outcome_destructor = destructor;
  return frame;
}

extern void *aura_llvm_array_alloc(int64_t length, int64_t kind);
extern int64_t aura_llvm_array_len(void *value);
extern int64_t aura_llvm_array_get(void *value, int64_t index);
extern void aura_llvm_array_set(void *value, int64_t index, int64_t raw);
extern void *aura_llvm_class_alloc(uint64_t field_count, uint64_t type_id);
int aura_task_frame_wait_tcp_stream(AuraTaskFrame *frame, const AuraTcpStream *stream, short events);
int aura_task_frame_wait_tcp_stream_timeout(AuraTaskFrame *frame, const AuraTcpStream *stream, short events, int timeout_ms);

typedef struct {
  AuraFfiOpaqueHandle *handle;
  AuraFfiHandlePin pin;
  int64_t length;
  int64_t offset;
  int64_t deadline_ms;
  uint8_t *buffer;
  uint64_t class_type_id;
} AuraLlvmNetExactData;

static void aura_llvm_net_exact_destroy(AuraTaskFrame *frame)
{
  AuraLlvmNetExactData *data = (AuraLlvmNetExactData *)aura_task_frame_data(frame);
  if (data != NULL) { free(data->buffer); if (data->pin.handle != NULL) (void)aura_ffi_handle_unpin(&data->pin); }
}

static void aura_llvm_net_exact_result_destroy(void *raw, size_t size)
{
  (void)size;
  if (raw != NULL) { aura_llvm_class_release(*(void **)raw); free(raw); }
}

static AuraTaskPollState aura_llvm_net_exact_poll(AuraTaskFrame *frame)
{
  AuraLlvmNetExactData *data = (AuraLlvmNetExactData *)aura_task_frame_data(frame);
  size_t count = 0;
  AuraTcpStatus status;
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->pin.handle == NULL)
  {
    if (data->handle == NULL || aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED;
    data->buffer = (uint8_t *)malloc((size_t)data->length);
    if (data->buffer == NULL) return AURA_TASK_FAILED;
  }
  status = aura_tcp_stream_read((AuraTcpStream *)data->pin.resource, data->buffer + data->offset,
                                (size_t)(data->length - data->offset), &count, 0);
  data->offset += (int64_t)count;
  if (data->offset == data->length)
  {
    void *array = aura_llvm_array_alloc(data->length, 0);
    void *result = aura_llvm_class_alloc(1, data->class_type_id);
    void **slot;
    if (array == NULL || result == NULL) return AURA_TASK_FAILED;
    for (int64_t index = 0; index < data->length; index++) aura_llvm_array_set(array, index, (int64_t)data->buffer[index]);
    ((uint64_t *)result)[1] = (uint64_t)(uintptr_t)array;
    slot = (void **)malloc(sizeof(*slot));
    if (slot == NULL) return AURA_TASK_FAILED;
    *slot = result;
    aura_task_frame_set_result(frame, slot, sizeof(*slot), aura_llvm_net_exact_result_destroy);
    return AURA_TASK_COMPLETE;
  }
  if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT)
  {
    if (data->deadline_ms > 0)
    {
      int64_t left = data->deadline_ms - aura_time_monotonic_millis();
      if (left <= 0 || !aura_task_frame_wait_tcp_stream_timeout(frame, (const AuraTcpStream *)data->pin.resource, 1, left > INT_MAX ? INT_MAX : (int)left)) return AURA_TASK_FAILED;
    }
    else if (!aura_task_frame_wait_tcp_stream(frame, (const AuraTcpStream *)data->pin.resource, 1)) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_FAILED;
}

void *aura_llvm_net_read_exact_task(void *executor, void *handle, int64_t length,
                                    int64_t timeout_ms, int64_t unused_type_id,
                                    int64_t class_type_id)
{
  AuraTaskFrame *frame;
  AuraLlvmNetExactData *data;
  (void)unused_type_id;
  if (executor == NULL || handle == NULL || length <= 0 || timeout_ms < 0) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_net_exact_poll, aura_llvm_net_exact_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmNetExactData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->handle = (AuraFfiOpaqueHandle *)handle; data->length = length; data->class_type_id = (uint64_t)class_type_id;
  if (timeout_ms > 0) { int64_t now = aura_time_monotonic_millis(); if (now <= 0 || now > INT64_MAX - timeout_ms) { aura_task_frame_destroy(frame); return NULL; } data->deadline_ms = now + timeout_ms; }
  if (!aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

typedef struct {
  AuraFfiOpaqueHandle *handle;
  AuraFfiHandlePin pin;
  int64_t length;
  int64_t offset;
  uint8_t *buffer;
} AuraLlvmNetWriteAllData;

static void aura_llvm_net_write_all_destroy(AuraTaskFrame *frame)
{
  AuraLlvmNetWriteAllData *data = (AuraLlvmNetWriteAllData *)aura_task_frame_data(frame);
  if (data != NULL) { free(data->buffer); if (data->pin.handle != NULL) (void)aura_ffi_handle_unpin(&data->pin); }
}

static AuraTaskPollState aura_llvm_net_write_all_poll(AuraTaskFrame *frame)
{
  AuraLlvmNetWriteAllData *data = (AuraLlvmNetWriteAllData *)aura_task_frame_data(frame);
  size_t count = 0;
  AuraTcpStatus status;
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->pin.handle == NULL)
  {
    if (data->handle == NULL || aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK) return AURA_TASK_FAILED;
  }
  status = aura_tcp_stream_write((AuraTcpStream *)data->pin.resource, data->buffer + data->offset,
                                 (size_t)(data->length - data->offset), &count, 0);
  data->offset += (int64_t)count;
  if (data->offset == data->length)
  {
    int64_t *result = (int64_t *)malloc(sizeof(*result));
    if (result == NULL) return AURA_TASK_FAILED;
    *result = data->offset;
    aura_task_frame_set_result(frame, result, sizeof(*result), aura_llvm_net_i64_destroy);
    return AURA_TASK_COMPLETE;
  }
  if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT)
  {
    if (!aura_task_frame_wait_tcp_stream(frame, (const AuraTcpStream *)data->pin.resource, 4)) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_FAILED;
}

void *aura_llvm_net_write_all_task(void *executor, void *handle, void *buffer, int64_t unused)
{
  AuraTaskFrame *frame;
  AuraLlvmNetWriteAllData *data;
  void *array;
  int64_t length;
  (void)unused;
  if (executor == NULL || handle == NULL || buffer == NULL) return NULL;
  array = (void *)(uintptr_t)((uint64_t *)buffer)[1];
  length = aura_llvm_array_len(array);
  if (array == NULL || length < 0) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_net_write_all_poll, aura_llvm_net_write_all_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmNetWriteAllData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->handle = (AuraFfiOpaqueHandle *)handle; data->length = length;
  data->buffer = (uint8_t *)malloc((size_t)length);
  if (data->buffer == NULL && length != 0) { aura_task_frame_destroy(frame); return NULL; }
  for (int64_t index = 0; index < length; index++) data->buffer[index] = (uint8_t)aura_llvm_array_get(array, index);
  if (!aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

extern short aura_tls_pending_events(const char *endpoint);
extern AuraTcpStream *aura_tls_stream(const char *endpoint);
extern int aura_tls_read_bytes(const char *endpoint, void *output, size_t capacity, size_t *out_bytes, int timeout_ms);
extern int aura_tls_write_bytes(const char *endpoint, const void *input, size_t length, size_t *out_bytes, int timeout_ms);

typedef struct {
  char *endpoint;
  int64_t length;
  int64_t offset;
  int64_t deadline_ms;
  uint8_t *buffer;
  uint64_t class_type_id;
  int write_mode;
} AuraLlvmTlsData;

static void aura_llvm_tls_destroy(AuraTaskFrame *frame)
{
  AuraLlvmTlsData *data = (AuraLlvmTlsData *)aura_task_frame_data(frame);
  if (data != NULL) { free(data->endpoint); free(data->buffer); }
}

static AuraTaskPollState aura_llvm_tls_poll(AuraTaskFrame *frame)
{
  AuraLlvmTlsData *data = (AuraLlvmTlsData *)aura_task_frame_data(frame);
  size_t count = 0;
  int status;
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->deadline_ms > 0 && aura_time_monotonic_millis() >= data->deadline_ms) return AURA_TASK_FAILED;
  if (aura_tls_stream(data->endpoint) == NULL) return AURA_TASK_FAILED;
  if (!data->write_mode && data->offset == data->length)
  {
    void *array = aura_llvm_array_alloc(data->length, 0);
    void *result = aura_llvm_class_alloc(1, data->class_type_id);
    void **slot;
    if (array == NULL || result == NULL) return AURA_TASK_FAILED;
    for (int64_t index = 0; index < data->length; index++) aura_llvm_array_set(array, index, (int64_t)data->buffer[index]);
    ((uint64_t *)result)[1] = (uint64_t)(uintptr_t)array;
    slot = (void **)malloc(sizeof(*slot));
    if (slot == NULL) return AURA_TASK_FAILED;
    *slot = result;
    aura_task_frame_set_result(frame, slot, sizeof(*slot), aura_llvm_net_exact_result_destroy);
    return AURA_TASK_COMPLETE;
  }
  if (data->write_mode && data->offset == data->length)
  {
    int64_t *result = (int64_t *)malloc(sizeof(*result));
    if (result == NULL) return AURA_TASK_FAILED;
    *result = data->offset;
    aura_task_frame_set_result(frame, result, sizeof(*result), aura_llvm_net_i64_destroy);
    return AURA_TASK_COMPLETE;
  }
  if (data->write_mode)
    status = aura_tls_write_bytes(data->endpoint, data->buffer + data->offset, (size_t)(data->length - data->offset), &count, 0);
  else
    status = aura_tls_read_bytes(data->endpoint, data->buffer + data->offset, (size_t)(data->length - data->offset), &count, 0);
  data->offset += (int64_t)count;
  /* TLS reads are capacity reads, unlike the exact TCP helper: return the
   * first successful/EOF chunk even when it is shorter than the capacity. */
  if (!data->write_mode && (status == 0 || status == 1)) data->length = data->offset;
  if ((!data->write_mode && data->offset == data->length) || (data->write_mode && data->offset == data->length)) return aura_llvm_tls_poll(frame);
  if (status == 3)
  {
    int timeout = -1;
    if (data->deadline_ms > 0)
    {
      int64_t left = data->deadline_ms - aura_time_monotonic_millis();
      if (left <= 0) return AURA_TASK_FAILED;
      timeout = left > INT_MAX ? INT_MAX : (int)left;
    }
    if (timeout < 0)
    {
      if (!aura_task_frame_wait_tcp_stream(frame, (const AuraTcpStream *)aura_tls_stream(data->endpoint), aura_tls_pending_events(data->endpoint))) return AURA_TASK_FAILED;
    }
    else if (!aura_task_frame_wait_tcp_stream_timeout(frame, (const AuraTcpStream *)aura_tls_stream(data->endpoint), aura_tls_pending_events(data->endpoint), timeout)) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  return AURA_TASK_FAILED;
}

static void *aura_llvm_tls_task_new(void *executor, const char *endpoint, int64_t length,
                                    int64_t timeout_ms, int64_t class_type_id,
                                    uint8_t *buffer, int write_mode)
{
  AuraTaskFrame *frame;
  AuraLlvmTlsData *data;
  if (executor == NULL || endpoint == NULL || length < 0 || timeout_ms < 0) { free(buffer); return NULL; }
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_tls_poll, aura_llvm_tls_destroy);
  if (frame == NULL) { free(buffer); return NULL; }
  data = (AuraLlvmTlsData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->endpoint = strdup(endpoint); data->length = length; data->class_type_id = (uint64_t)class_type_id; data->buffer = buffer; data->write_mode = write_mode;
  if (data->endpoint == NULL) { aura_task_frame_destroy(frame); return NULL; }
  if (timeout_ms > 0) { int64_t now = aura_time_monotonic_millis(); if (now <= 0 || now > INT64_MAX - timeout_ms) { aura_task_frame_destroy(frame); return NULL; } data->deadline_ms = now + timeout_ms; }
  if (!aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

void *aura_llvm_tls_read_task(void *executor, void *endpoint, int64_t capacity, int64_t timeout_ms, int64_t class_type_id)
{
  return aura_llvm_tls_task_new(executor, (const char *)aura_llvm_str_data(endpoint), capacity, timeout_ms, class_type_id, capacity > 0 ? (uint8_t *)calloc((size_t)capacity, 1) : NULL, 0);
}

void *aura_llvm_tls_write_task(void *executor, void *endpoint, void *buffer, int64_t timeout_ms)
{
  void *array = buffer == NULL ? NULL : (void *)(uintptr_t)((uint64_t *)buffer)[1];
  int64_t length = array == NULL ? 0 : aura_llvm_array_len(array);
  uint8_t *copy = length > 0 ? (uint8_t *)malloc((size_t)length) : NULL;
  if (length > 0 && copy == NULL) return NULL;
  for (int64_t index = 0; index < length; index++) copy[index] = (uint8_t)aura_llvm_array_get(array, index);
  return aura_llvm_tls_task_new(executor, (const char *)aura_llvm_str_data(endpoint), length, timeout_ms, 0, copy, 1);
}

typedef struct
{
  AuraFfiOpaqueHandle *handle;
  AuraFfiHandlePin pin;
  int64_t capacity;
  char *buffer;
  int read_active;
  int outcome_mode;
  int64_t outcome_tag;
  void *outcome_destructor;
} AuraLlvmHttpReadData;

static void aura_llvm_http_read_destroy(AuraTaskFrame *frame)
{
  AuraLlvmHttpReadData *data = (AuraLlvmHttpReadData *)aura_task_frame_data(frame);
  if (data != NULL)
  {
    if (data->read_active && data->pin.handle != NULL)
      aura_http_request_body_read_end((const AuraHttpRequest *)data->pin.resource);
    free(data->buffer);
    if (data->pin.handle != NULL) (void)aura_ffi_handle_unpin(&data->pin);
  }
}

static AuraTaskPollState aura_llvm_http_read_poll(AuraTaskFrame *frame)
{
  AuraLlvmHttpReadData *data = (AuraLlvmHttpReadData *)aura_task_frame_data(frame);
  size_t count = 0;
  int status;
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->pin.handle == NULL)
  {
    if (data->handle == NULL || data->capacity <= 0 ||
        aura_ffi_handle_pin_for_boundary(data->handle, AURA_FFI_BOUNDARY_TASK, &data->pin) != AURA_FFI_OK)
      return AURA_TASK_FAILED;
    if (!aura_http_request_body_read_begin((const AuraHttpRequest *)data->pin.resource)) return AURA_TASK_FAILED;
    data->read_active = 1;
    data->buffer = (char *)malloc((size_t)data->capacity + 1);
    if (data->buffer == NULL) return AURA_TASK_FAILED;
  }
  status = aura_http_request_read_body((const AuraHttpRequest *)data->pin.resource,
                                       (unsigned char *)data->buffer,
                                       (size_t)data->capacity, &count);
  if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT)
  {
    if (!aura_http_request_wait_body(frame, (const AuraHttpRequest *)data->pin.resource)) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  if (data->read_active) aura_http_request_body_read_end((const AuraHttpRequest *)data->pin.resource);
  data->read_active = 0;
  if (status != AURA_TCP_OK && status != AURA_TCP_EOF) return AURA_TASK_FAILED;
  {
    void **result = (void **)malloc(sizeof(*result));
    if (result == NULL) return AURA_TASK_FAILED;
    data->buffer[count] = '\0';
    *result = aura_llvm_str_new(data->buffer);
    if (*result == NULL) { free(result); return AURA_TASK_FAILED; }
    if (data->outcome_mode)
    {
      void **outcome_slot = (void **)malloc(sizeof(*outcome_slot));
      void *outcome = aura_llvm_enum_alloc(1, data->outcome_destructor);
      if (outcome_slot == NULL || outcome == NULL)
      {
        free(outcome_slot); if (outcome != NULL) aura_llvm_enum_release(outcome);
        aura_llvm_str_release(*result); free(result); return AURA_TASK_FAILED;
      }
      ((int64_t *)outcome)[1] = data->outcome_tag;
      ((int64_t *)outcome)[3] = (int64_t)(uintptr_t)*result;
      *outcome_slot = outcome; free(result);
      aura_task_frame_set_result(frame, outcome_slot, sizeof(*outcome_slot), aura_llvm_net_outcome_destroy);
    }
    else aura_task_frame_set_result(frame, result, sizeof(*result), aura_llvm_net_io_result_destroy);
  }
  return AURA_TASK_COMPLETE;
}

void *aura_llvm_http_read_chunk_task(void *executor, void *handle, int64_t capacity)
{
  AuraTaskFrame *frame;
  AuraLlvmHttpReadData *data;
  if (executor == NULL || handle == NULL || capacity <= 0) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_http_read_poll, aura_llvm_http_read_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmHttpReadData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->handle = (AuraFfiOpaqueHandle *)handle; data->capacity = capacity;
  if (!aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

void *aura_llvm_http_read_chunk_result_task(void *executor, void *handle, int64_t capacity,
                                            int64_t tag, void *destructor)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)aura_llvm_http_read_chunk_task(executor, handle, capacity);
  AuraLlvmHttpReadData *data;
  if (frame == NULL) return NULL;
  data = (AuraLlvmHttpReadData *)aura_task_frame_data(frame);
  data->outcome_mode = 1; data->outcome_tag = tag; data->outcome_destructor = destructor;
  return frame;
}

typedef struct
{
  AuraFfiOpaqueHandle *response_handle;
  AuraFfiOpaqueHandle *connection_handle;
  AuraFfiHandlePin response_pin;
  AuraFfiHandlePin connection_pin;
  char *body;
  char *output;
  size_t output_length;
  size_t output_offset;
  int outcome_mode;
  int64_t outcome_tag;
  void *outcome_destructor;
} AuraLlvmHttpWriteData;

static void aura_llvm_http_write_destroy(AuraTaskFrame *frame)
{
  AuraLlvmHttpWriteData *data = (AuraLlvmHttpWriteData *)aura_task_frame_data(frame);
  if (data != NULL)
  {
    free(data->body); free(data->output);
    if (data->connection_pin.handle != NULL) (void)aura_ffi_handle_unpin(&data->connection_pin);
    if (data->response_pin.handle != NULL) (void)aura_ffi_handle_unpin(&data->response_pin);
  }
}

static AuraTaskPollState aura_llvm_http_write_poll(AuraTaskFrame *frame)
{
  AuraLlvmHttpWriteData *data = (AuraLlvmHttpWriteData *)aura_task_frame_data(frame);
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->response_pin.handle == NULL)
  {
    size_t headers = 0, chunk = 0, written = 0;
    AuraHttpResponse *response;
    AuraHttpConnection *connection;
    if (data->response_handle == NULL || data->connection_handle == NULL || data->body == NULL) return AURA_TASK_FAILED;
    if (aura_ffi_handle_pin_for_boundary(data->response_handle, AURA_FFI_BOUNDARY_TASK, &data->response_pin) != AURA_FFI_OK ||
        aura_ffi_handle_pin_for_boundary(data->connection_handle, AURA_FFI_BOUNDARY_TASK, &data->connection_pin) != AURA_FFI_OK) return AURA_TASK_FAILED;
    response = (AuraHttpResponse *)data->response_pin.resource;
    connection = (AuraHttpConnection *)data->connection_pin.resource;
    if (!aura_http_response_stream_started(response) && (aura_http_response_stream_begin(response, NULL, 0, &headers) != -3 || headers == 0)) return AURA_TASK_FAILED;
    if (aura_http_response_stream_chunk(data->body, strlen(data->body), NULL, 0, &chunk) != -3 || chunk == 0 || headers > SIZE_MAX - chunk) return AURA_TASK_FAILED;
    data->output_length = headers + chunk; data->output = (char *)malloc(data->output_length);
    if (data->output == NULL) return AURA_TASK_FAILED;
    if (headers != 0 && (aura_http_response_stream_begin(response, data->output, headers, &written) != 0 || written != headers)) return AURA_TASK_FAILED;
    if (aura_http_response_stream_chunk(data->body, strlen(data->body), data->output + headers, chunk, &written) != 0 || written != chunk) return AURA_TASK_FAILED;
    data->output_offset = 0;
  }
  while (data->output_offset < data->output_length)
  {
    size_t written = 0;
    AuraTcpStatus status = aura_http_connection_stream_write((AuraHttpConnection *)data->connection_pin.resource,
                                                             data->output + data->output_offset,
                                                             data->output_length - data->output_offset, &written);
    if (status == AURA_TCP_PENDING)
    {
      if (!aura_http_connection_wait_write(frame, (const AuraHttpConnection *)data->connection_pin.resource)) return AURA_TASK_FAILED;
      return AURA_TASK_PENDING;
    }
    if (status != AURA_TCP_OK || written == 0) return AURA_TASK_FAILED;
    data->output_offset += written;
  }
  if (data->outcome_mode)
  {
    void **outcome_slot = (void **)malloc(sizeof(*outcome_slot));
    void *outcome = aura_llvm_enum_alloc(1, data->outcome_destructor);
    if (outcome_slot == NULL || outcome == NULL)
    {
      free(outcome_slot); if (outcome != NULL) aura_llvm_enum_release(outcome);
      return AURA_TASK_FAILED;
    }
    ((int64_t *)outcome)[1] = data->outcome_tag;
    ((int64_t *)outcome)[3] = 1;
    *outcome_slot = outcome;
    aura_task_frame_set_result(frame, outcome_slot, sizeof(*outcome_slot), aura_llvm_net_outcome_destroy);
  }
  return AURA_TASK_COMPLETE;
}

void *aura_llvm_http_write_chunk_task(void *executor, void *response_handle, void *connection_handle,
                                      void *body)
{
  AuraTaskFrame *frame;
  AuraLlvmHttpWriteData *data;
  const char *text = (const char *)aura_llvm_str_data(body);
  if (executor == NULL || response_handle == NULL || connection_handle == NULL || text == NULL || text[0] == '\0') return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_http_write_poll, aura_llvm_http_write_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmHttpWriteData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->response_handle = (AuraFfiOpaqueHandle *)response_handle;
  data->connection_handle = (AuraFfiOpaqueHandle *)connection_handle; data->body = strdup(text);
  if (data->body == NULL || !aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

void *aura_llvm_http_write_chunk_result_task(void *executor, void *response_handle,
                                             void *connection_handle, void *body,
                                             int64_t tag, void *destructor)
{
  AuraTaskFrame *frame = (AuraTaskFrame *)aura_llvm_http_write_chunk_task(
      executor, response_handle, connection_handle, body);
  AuraLlvmHttpWriteData *data;
  if (frame == NULL) return NULL;
  data = (AuraLlvmHttpWriteData *)aura_task_frame_data(frame);
  data->outcome_mode = 1; data->outcome_tag = tag; data->outcome_destructor = destructor;
  return frame;
}

typedef AuraTaskFrame *(*AuraLlvmHttpHandlerFn)(void *environment,
                                                void *request,
                                                void *response);

typedef struct
{
  AuraFfiOpaqueHandle *stream_handle;
  AuraFfiOpaqueHandle *connection_handle;
  AuraFfiOpaqueHandle *request_handle;
  AuraFfiOpaqueHandle *response_handle;
  void *handler_environment;
  AuraLlvmHttpHandlerFn handler;
  void *request;
  void *response;
  AuraTaskFrame *child;
  AuraTaskExecutor *executor;
  uint64_t request_fields;
  uint64_t request_type_id;
  uint64_t response_fields;
  uint64_t response_type_id;
} AuraLlvmHttpServeData;

extern void aura_fun_env_retain(void *environment);
extern void aura_fun_env_free(void *environment);
extern AuraTaskPollState aura_task_executor_poll_inline(AuraTaskExecutor *executor, AuraTaskFrame *frame);
extern int aura_task_frame_wait_on(AuraTaskFrame *frame, AuraTaskFrame *child);
extern int aura_task_frame_propagate_error(AuraTaskFrame *frame, const AuraTaskFrame *child);
extern int aura_task_executor_release_terminal(AuraTaskExecutor *executor, AuraTaskFrame **frame);
extern AuraTaskPollState aura_task_frame_state(const AuraTaskFrame *frame);
extern int aura_task_frame_wait_tcp_listener(AuraTaskFrame *frame,
                                             const AuraTcpListener *listener,
                                             short events);
extern void aura_llvm_class_release(void *value);

static void aura_llvm_http_serve_destroy(AuraTaskFrame *frame)
{
  AuraLlvmHttpServeData *data = (AuraLlvmHttpServeData *)aura_task_frame_data(frame);
  if (data == NULL) return;
  if (data->child != NULL && data->executor != NULL)
    (void)aura_task_executor_release(data->executor, &data->child);
  if (data->request != NULL) aura_llvm_class_release(data->request);
  if (data->response != NULL) aura_llvm_class_release(data->response);
  if (data->request_handle != NULL) (void)aura_ffi_handle_drop(&data->request_handle);
  if (data->response_handle != NULL) (void)aura_ffi_handle_drop(&data->response_handle);
  if (data->connection_handle != NULL) (void)aura_ffi_handle_drop(&data->connection_handle);
  if (data->stream_handle != NULL) (void)aura_ffi_handle_drop(&data->stream_handle);
  aura_fun_env_free(data->handler_environment);
}

static AuraTaskPollState aura_llvm_http_serve_bridge(AuraTaskFrame *frame,
                                                     const AuraHttpRequest *request,
                                                     AuraHttpResponse *response,
                                                     void *user_data)
{
  AuraLlvmHttpServeData *data = (AuraLlvmHttpServeData *)user_data;
  AuraTaskPollState state;
  if (data == NULL || request == NULL || response == NULL || data->handler == NULL)
    return AURA_TASK_FAILED;
  if (data->child == NULL)
  {
    if (aura_ffi_handle_new((void *)request, NULL, &data->request_handle) != AURA_FFI_OK ||
        aura_ffi_handle_new((void *)response, NULL, &data->response_handle) != AURA_FFI_OK)
      return AURA_TASK_FAILED;
    data->request = aura_llvm_class_alloc(data->request_fields, data->request_type_id);
    data->response = aura_llvm_class_alloc(data->response_fields, data->response_type_id);
    if (data->request == NULL || data->response == NULL) return AURA_TASK_FAILED;
    ((uint64_t *)data->request)[1] = (uint64_t)(uintptr_t)data->request_handle;
    ((uint64_t *)data->response)[1] = (uint64_t)(uintptr_t)data->response_handle;
    ((uint64_t *)data->response)[2] = (uint64_t)(uintptr_t)data->connection_handle;
    data->child = data->handler(data->handler_environment, data->request, data->response);
    if (data->child == NULL) return AURA_TASK_FAILED;
  }
  state = aura_task_frame_state(data->child);
  if (state == AURA_TASK_READY)
    state = aura_task_executor_poll_inline(data->executor, data->child);
  if (state == AURA_TASK_PENDING)
  {
    if (!aura_task_frame_wait_on(frame, data->child)) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  if (state == AURA_TASK_FAILED)
  {
    (void)aura_task_frame_propagate_error(frame, data->child);
    return AURA_TASK_FAILED;
  }
  if (state != AURA_TASK_COMPLETE) return state;
  if (data->executor != NULL)
    (void)aura_task_executor_release_terminal(data->executor, &data->child);
  return AURA_TASK_COMPLETE;
}

static AuraTaskPollState aura_llvm_http_serve_poll(AuraTaskFrame *frame)
{
  AuraLlvmHttpServeData *data = (AuraLlvmHttpServeData *)aura_task_frame_data(frame);
  AuraTcpStream *stream = NULL;
  AuraHttpConnection *connection = NULL;
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  if (data->connection_handle == NULL)
  {
    void *raw = NULL;
    if (aura_ffi_handle_take_owned(&data->stream_handle, &raw) != AURA_FFI_OK || raw == NULL ||
        aura_http_connection_create_from_stream((AuraTcpStream *)raw, NULL, &connection) != AURA_HTTP_CONNECTION_OK ||
        connection == NULL || aura_ffi_handle_new(connection, aura_http_connection_destroy_resource, &data->connection_handle) != AURA_FFI_OK)
    {
      if (raw != NULL && connection == NULL) aura_tcp_stream_destroy((AuraTcpStream *)raw);
      return AURA_TASK_FAILED;
    }
  }
  return aura_http_connection_poll_async_task_handle(frame, data->connection_handle,
                                                       aura_llvm_http_serve_bridge, data);
}

void *aura_llvm_http_serve_connection_task(void *executor, void *stream_handle,
                                           void *handler_environment, void *handler,
                                           int64_t request_fields, int64_t request_type_id,
                                           int64_t response_fields, int64_t response_type_id)
{
  AuraTaskFrame *frame;
  AuraLlvmHttpServeData *data;
  if (executor == NULL || stream_handle == NULL || handler == NULL || request_fields <= 0 || response_fields < 2)
    return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_http_serve_poll, aura_llvm_http_serve_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmHttpServeData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data));
  data->stream_handle = (AuraFfiOpaqueHandle *)stream_handle;
  data->executor = (AuraTaskExecutor *)executor;
  data->handler_environment = handler_environment;
  data->handler = (AuraLlvmHttpHandlerFn)handler;
  data->request_fields = (uint64_t)request_fields; data->request_type_id = (uint64_t)request_type_id;
  data->response_fields = (uint64_t)response_fields; data->response_type_id = (uint64_t)response_type_id;
  if (!aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

typedef struct
{
  AuraTaskExecutor *executor;
  AuraFfiOpaqueHandle *listener_handle;
  AuraFfiHandlePin listener_pin;
  void *handler_environment;
  void *handler;
  AuraTaskFrame **connections;
  size_t connection_count;
  size_t connection_capacity;
  int stopping;
  uint64_t request_fields;
  uint64_t request_type_id;
  uint64_t response_fields;
  uint64_t response_type_id;
} AuraLlvmHttpServeLoopData;

static void aura_llvm_http_serve_loop_destroy(AuraTaskFrame *frame)
{
  AuraLlvmHttpServeLoopData *data = (AuraLlvmHttpServeLoopData *)aura_task_frame_data(frame);
  if (data == NULL) return;
  if (data->executor != NULL)
  {
    for (size_t index = 0; index < data->connection_count; index++)
      (void)aura_task_executor_release(data->executor, &data->connections[index]);
  }
  free(data->connections);
  if (data->listener_pin.handle != NULL) (void)aura_ffi_handle_unpin(&data->listener_pin);
  if (data->listener_handle != NULL) (void)aura_ffi_handle_drop(&data->listener_handle);
  aura_fun_env_free(data->handler_environment);
}

static AuraTaskPollState aura_llvm_http_serve_loop_poll(AuraTaskFrame *frame)
{
  AuraLlvmHttpServeLoopData *data = (AuraLlvmHttpServeLoopData *)aura_task_frame_data(frame);
  if (data == NULL || aura_task_frame_cancel_requested(frame)) return AURA_TASK_CANCELLED;
  for (size_t index = 0; index < data->connection_count;)
  {
    AuraTaskPollState state = aura_task_frame_state(data->connections[index]);
    if (state == AURA_TASK_COMPLETE || state == AURA_TASK_FAILED || state == AURA_TASK_CANCELLED)
    {
      AuraTaskFrame *connection = data->connections[index];
      if (!aura_task_executor_release_terminal(data->executor, &connection))
      {
        index++;
        continue;
      }
      data->connection_count--;
      data->connections[index] = data->connections[data->connection_count];
      continue;
    }
    index++;
  }
  if (aura_signal_shutdown_requested() && !data->stopping)
  {
    (void)aura_tcp_listener_close((AuraTcpListener *)data->listener_pin.resource);
    data->stopping = 1;
  }
  if (data->listener_pin.handle == NULL)
  {
    if (data->listener_handle == NULL ||
        aura_ffi_handle_pin_for_boundary(data->listener_handle, AURA_FFI_BOUNDARY_TASK, &data->listener_pin) != AURA_FFI_OK)
      return AURA_TASK_FAILED;
  }
  if (data->stopping)
  {
    if (data->connection_count == 0) return AURA_TASK_COMPLETE;
    if (!aura_task_frame_wait_on(frame, data->connections[0])) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  if (data->connection_count >= 64)
  {
    if (!aura_task_frame_wait_on(frame, data->connections[0])) return AURA_TASK_FAILED;
    return AURA_TASK_PENDING;
  }
  {
    AuraTcpStream *stream = NULL;
    AuraTcpStatus status = aura_tcp_listener_accept((AuraTcpListener *)data->listener_pin.resource, 0, &stream);
    if (status == AURA_TCP_PENDING || status == AURA_TCP_TIMEOUT)
    {
      if (!aura_task_frame_wait_tcp_listener(frame, (const AuraTcpListener *)data->listener_pin.resource, 1)) return AURA_TASK_FAILED;
      return AURA_TASK_PENDING;
    }
    if (status == AURA_TCP_CLOSED) return AURA_TASK_COMPLETE;
    if (status != AURA_TCP_OK || stream == NULL) return AURA_TASK_FAILED;
    AuraFfiOpaqueHandle *stream_handle = NULL;
    if (aura_ffi_handle_new(stream, aura_llvm_destroy_tcp_stream, &stream_handle) != AURA_FFI_OK)
    {
      aura_tcp_stream_destroy(stream); return AURA_TASK_FAILED;
    }
    aura_fun_env_retain(data->handler_environment);
    AuraTaskFrame *connection = (AuraTaskFrame *)aura_llvm_http_serve_connection_task(
        data->executor, stream_handle, data->handler_environment, data->handler,
        (int64_t)data->request_fields, (int64_t)data->request_type_id,
        (int64_t)data->response_fields, (int64_t)data->response_type_id);
    if (connection == NULL)
    {
      aura_fun_env_free(data->handler_environment);
      (void)aura_ffi_handle_drop(&stream_handle);
      return AURA_TASK_FAILED;
    }
    if (data->connection_count == data->connection_capacity)
    {
      size_t next_capacity = data->connection_capacity == 0 ? 8 : data->connection_capacity * 2;
      AuraTaskFrame **next = (AuraTaskFrame **)realloc(
          data->connections, next_capacity * sizeof(*next));
      if (next == NULL)
      {
        (void)aura_task_executor_release(data->executor, &connection);
        return AURA_TASK_FAILED;
      }
      data->connections = next;
      data->connection_capacity = next_capacity;
    }
    data->connections[data->connection_count++] = connection;
  }
  return AURA_TASK_PENDING;
}

void *aura_llvm_http_serve_task(void *executor, void *listener_handle,
                                void *handler_environment, void *handler,
                                int64_t request_fields, int64_t request_type_id,
                                int64_t response_fields, int64_t response_type_id)
{
  AuraTaskFrame *frame;
  AuraLlvmHttpServeLoopData *data;
  if (executor == NULL || listener_handle == NULL || handler == NULL) return NULL;
  frame = aura_task_frame_new(sizeof(*data), aura_llvm_http_serve_loop_poll,
                              aura_llvm_http_serve_loop_destroy);
  if (frame == NULL) return NULL;
  data = (AuraLlvmHttpServeLoopData *)aura_task_frame_data(frame);
  memset(data, 0, sizeof(*data)); data->executor = (AuraTaskExecutor *)executor;
  data->listener_handle = (AuraFfiOpaqueHandle *)listener_handle;
  data->handler_environment = handler_environment; data->handler = handler;
  data->request_fields = (uint64_t)request_fields; data->request_type_id = (uint64_t)request_type_id;
  data->response_fields = (uint64_t)response_fields; data->response_type_id = (uint64_t)response_type_id;
  if (!aura_task_executor_submit((AuraTaskExecutor *)executor, frame)) { aura_task_frame_destroy(frame); return NULL; }
  return frame;
}

#endif

#if defined(AURA_LLVM_RUNTIME)
AuraLazyCell *aura_lazy_cell_new(AuraLazyInitFn init, void *environment,
                                 AuraTaskBlockingEnvDestroyFn environment_destroy);
void aura_lazy_cell_publish(AuraLazyCell *cell, void *value, size_t size,
                            AuraLazyValueDestroyFn value_destroy);
void *aura_lazy_cell_value(AuraLazyCell *cell);
int aura_lazy_cell_is_initialized(AuraLazyCell *cell);
void aura_lazy_cell_destroy(AuraLazyCell *cell);
void aura_fun_env_free(void *environment);
typedef int64_t (*AuraLlvmLazyIntFn)(void *environment);
typedef struct
{
  void *environment;
  AuraLlvmLazyIntFn function;
} AuraLlvmLazyIntEnv;

static void aura_llvm_lazy_int_init(AuraLazyCell *cell, void *value)
{
  AuraLlvmLazyIntEnv *env = (AuraLlvmLazyIntEnv *)value;
  int64_t *result;
  if (env == NULL || env->function == NULL)
    return;
  result = (int64_t *)malloc(sizeof(*result));
  if (result == NULL)
    return;
  *result = env->function(env->environment);
  aura_lazy_cell_publish(cell, result, sizeof(*result), free);
}

static void aura_llvm_lazy_int_env_destroy(void *value)
{
  AuraLlvmLazyIntEnv *env = (AuraLlvmLazyIntEnv *)value;
  if (env == NULL)
    return;
  aura_fun_env_free(env->environment);
  free(env);
}

void *aura_llvm_lazy_int_new(void *environment, void *function)
{
  AuraLlvmLazyIntEnv *env = (AuraLlvmLazyIntEnv *)calloc(1, sizeof(*env));
  AuraLazyCell *cell;
  if (env == NULL)
    return NULL;
  env->environment = environment;
  env->function = (AuraLlvmLazyIntFn)function;
  cell = aura_lazy_cell_new(
      aura_llvm_lazy_int_init, env, aura_llvm_lazy_int_env_destroy);
  if (cell == NULL)
  {
    aura_llvm_lazy_int_env_destroy(env);
    return NULL;
  }
  return cell;
}

int64_t aura_llvm_lazy_int_get(void *value)
{
  int64_t *result = (int64_t *)aura_lazy_cell_value((AuraLazyCell *)value);
  return result == NULL ? 0 : *result;
}

int aura_llvm_lazy_is_initialized(void *value)
{
  return aura_lazy_cell_is_initialized((AuraLazyCell *)value);
}

void aura_llvm_lazy_int_destroy(void *value)
{
  aura_lazy_cell_destroy((AuraLazyCell *)value);
}

int64_t aura_llvm_sync_load(int64_t *value)
{
  return value == NULL ? 0 : __atomic_load_n(value, __ATOMIC_SEQ_CST);
}

void aura_llvm_sync_store(int64_t *value, int64_t next)
{
  if (value != NULL)
    __atomic_store_n(value, next, __ATOMIC_SEQ_CST);
}

int64_t aura_llvm_sync_fetch_add(int64_t *value, int64_t amount)
{
  return value == NULL ? 0 : __atomic_fetch_add(value, amount, __ATOMIC_SEQ_CST);
}

int aura_llvm_sync_compare_exchange(int64_t *value, int64_t expected,
                                     int64_t desired)
{
  return value != NULL &&
         __atomic_compare_exchange_n(value, &expected, desired, false,
                                     __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
}

int aura_llvm_sync_try_lock(int64_t *value)
{
  return aura_llvm_sync_compare_exchange(value, 0, 1);
}

void aura_llvm_sync_unlock(int64_t *value)
{
  aura_llvm_sync_store(value, 0);
}

int aura_llvm_sync_is_locked(int64_t *value)
{
  return aura_llvm_sync_load(value) != 0;
}

int aura_llvm_sync_try_read(int64_t *value)
{
  int64_t state;
  if (value == NULL)
    return 0;
  state = aura_llvm_sync_load(value);
  while (state >= 0 && state != INT64_MAX)
  {
    if (__atomic_compare_exchange_n(value, &state, state + 1, false,
                                    __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST))
      return 1;
  }
  return 0;
}

int aura_llvm_sync_try_write(int64_t *value)
{
  return aura_llvm_sync_compare_exchange(value, 0, -1);
}

void aura_llvm_sync_unlock_read(int64_t *value)
{
  int64_t state;
  if (value == NULL)
    return;
  state = aura_llvm_sync_load(value);
  while (state > 0 && !__atomic_compare_exchange_n(
             value, &state, state - 1, false, __ATOMIC_SEQ_CST,
             __ATOMIC_SEQ_CST))
  {
  }
}

void aura_llvm_sync_unlock_write(int64_t *value)
{
  (void)aura_llvm_sync_compare_exchange(value, -1, 0);
}

int64_t aura_llvm_sync_reader_count(int64_t *value)
{
  int64_t state = aura_llvm_sync_load(value);
  return state > 0 ? state : 0;
}

int aura_llvm_sync_is_write_locked(int64_t *value)
{
  return aura_llvm_sync_load(value) == -1;
}

void aura_task_frame_set_error(AuraTaskFrame *frame, void *data, size_t size,
                               AuraTaskResultDestroyFn destroy);
char *aura_ex_message_copy(void);
static void aura_llvm_task_exception_destroy(void *data, size_t size)
{
  (void)size;
  free(data);
}

int aura_llvm_task_fail_from_exception(AuraTaskFrame *frame)
{
  char *copy = aura_ex_message_copy();
  if (copy != NULL)
    aura_task_frame_set_error(frame, copy, strlen(copy) + 1,
                              aura_llvm_task_exception_destroy);
  aura_ex_clear();
  aura_try_leave();
  return AURA_TASK_FAILED;
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

void aura_task_frame_set_gc_stack_map(AuraTaskFrame *frame,
                                      const AuraTaskFrameGcSlot *slots,
                                      size_t slot_count)
{
  if (frame != NULL)
  {
    frame->gc_stack_map = slots;
    frame->gc_stack_map_len = slots == NULL ? 0 : slot_count;
  }
}

void aura_task_frame_set_gc_storage_stack_map(
    AuraTaskFrame *frame, AuraTaskFrameGcStorage storage,
    const AuraTaskFrameGcSlot *slots, size_t slot_count)
{
  if (frame == NULL) return;
  size_t length = slots == NULL ? 0 : slot_count;
  switch (storage)
  {
    case AURA_TASK_FRAME_GC_CAPTURES:
      frame->captures_gc_stack_map = slots;
      frame->captures_gc_stack_map_len = length;
      break;
    case AURA_TASK_FRAME_GC_RESULT:
      frame->result_gc_stack_map = slots;
      frame->result_gc_stack_map_len = length;
      break;
    case AURA_TASK_FRAME_GC_ERROR:
      frame->error_gc_stack_map = slots;
      frame->error_gc_stack_map_len = length;
      break;
    case AURA_TASK_FRAME_GC_ERROR_PAYLOAD:
      frame->error_payload_gc_stack_map = slots;
      frame->error_payload_gc_stack_map_len = length;
      break;
    default:
      break;
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
  return aura_platform_monotonic_millis();
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

const char *aura_llvm_task_error_message(const AuraTaskFrame *frame)
{
  return frame != NULL && frame->error.data != NULL
             ? (const char *)frame->error.data
             : "task failed";
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

int aura_llvm_task_join_i64(AuraTaskExecutor *executor, AuraTaskFrame *frame,
                            int64_t *out)
{
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  AuraTaskPollState state;
  if (out == NULL)
  {
    return 0;
  }
  state = aura_task_executor_join(executor, frame, &result, &error);
  if (state != AURA_TASK_COMPLETE || result.data == NULL ||
      result.size != sizeof(*out))
  {
    return 0;
  }
  memcpy(out, result.data, sizeof(*out));
  return 1;
}

int aura_llvm_task_join_ptr(AuraTaskExecutor *executor, AuraTaskFrame *frame,
                            void **out)
{
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  AuraTaskPollState state;
  if (out == NULL)
  {
    return 0;
  }
  state = aura_task_executor_join(executor, frame, &result, &error);
  if (state != AURA_TASK_COMPLETE || result.data == NULL ||
      result.size != sizeof(*out))
  {
    return 0;
  }
  memcpy(out, result.data, sizeof(*out));
  return 1;
}

int aura_llvm_task_join_unit(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  return aura_task_executor_join(executor, frame, &result, &error) ==
         AURA_TASK_COMPLETE;
}

int aura_llvm_task_join_status(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  AuraTaskResult result = {NULL, 0};
  AuraTaskResult error = {NULL, 0};
  return aura_task_executor_join(executor, frame, &result, &error);
}

const char *aura_llvm_task_error_message(const AuraTaskFrame *frame);

void aura_llvm_task_raise_failure(AuraTaskFrame *frame)
{
  if (frame != NULL && frame->state == AURA_TASK_CANCELLED)
  {
    aura_throw_string("task cancelled");
  }
  aura_throw_string(aura_llvm_task_error_message(frame));
}

int aura_llvm_task_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  return aura_task_executor_cancel(executor, frame);
}

int aura_llvm_task_release(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  AuraTaskFrame *owned = frame;
  return aura_task_executor_release(executor, &owned);
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
#if AURA_PLATFORM_NETWORK
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
#if AURA_PLATFORM_NETWORK
  if (frame->blocking_fn != NULL)
  {
    pthread_mutex_destroy(&frame->blocking_lock);
  }
#endif
  free(frame);
}
