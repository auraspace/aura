/* ---- C22k deterministic single-threaded executor ----
 *
 * Submission transfers frame ownership to the executor.  The executor keeps
 * terminal frames alive so generated code can read their result until an
 * explicit release or shutdown; aura_task_executor_shutdown destroys every
 * remaining submitted frame once.
 * A poll callback returning READY is immediately queued at the FIFO tail.
 * PENDING parks the frame until aura_task_executor_wake is called.  No OS
 * threads, blocking waits, or implicit polling are used.
 */

#if defined(AURA_TCP_POSIX)
struct AuraLazyCell
{
  pthread_mutex_t lock;
  pthread_cond_t condition;
  int state; /* 0=uninitialized, 1=initializing, 2=initialized, 3=failed */
  int published;
  void *value;
  AuraLazyInitFn init;
  void *environment;
  AuraTaskBlockingEnvDestroyFn environment_destroy;
  AuraLazyValueDestroyFn value_destroy;
};

AuraLazyCell *aura_lazy_cell_new(AuraLazyInitFn init, void *environment,
                                 AuraTaskBlockingEnvDestroyFn environment_destroy)
{
  AuraLazyCell *cell;
  if (init == NULL)
  {
    return NULL;
  }
  cell = (AuraLazyCell *)calloc(1, sizeof(*cell));
  if (cell == NULL)
  {
    return NULL;
  }
  if (pthread_mutex_init(&cell->lock, NULL) != 0)
  {
    free(cell);
    return NULL;
  }
  if (pthread_cond_init(&cell->condition, NULL) != 0)
  {
    pthread_mutex_destroy(&cell->lock);
    free(cell);
    return NULL;
  }
  cell->init = init;
  cell->environment = environment;
  cell->environment_destroy = environment_destroy;
  return cell;
}

void aura_lazy_cell_publish(AuraLazyCell *cell, void *value, size_t size,
                            AuraLazyValueDestroyFn value_destroy)
{
  (void)size;
  if (cell == NULL)
  {
    return;
  }
  pthread_mutex_lock(&cell->lock);
  cell->value = value;
  cell->value_destroy = value_destroy;
  cell->published = 1;
  pthread_mutex_unlock(&cell->lock);
}

void *aura_lazy_cell_value(AuraLazyCell *cell)
{
  void *value = NULL;
  if (cell == NULL)
  {
    return NULL;
  }
  pthread_mutex_lock(&cell->lock);
  while (cell->state == 1)
  {
    pthread_cond_wait(&cell->condition, &cell->lock);
  }
  if (cell->state == 0)
  {
    cell->state = 1;
    pthread_mutex_unlock(&cell->lock);
    cell->init(cell, cell->environment);
    pthread_mutex_lock(&cell->lock);
    cell->state = cell->published ? 2 : 3;
    if (cell->environment_destroy != NULL && cell->environment != NULL)
    {
      cell->environment_destroy(cell->environment);
      cell->environment = NULL;
    }
    pthread_cond_broadcast(&cell->condition);
  }
  if (cell->state == 2)
  {
    value = cell->value;
  }
  pthread_mutex_unlock(&cell->lock);
  return value;
}

int aura_lazy_cell_is_initialized(AuraLazyCell *cell)
{
  int initialized;
  if (cell == NULL)
  {
    return 0;
  }
  pthread_mutex_lock(&cell->lock);
  initialized = cell->state == 2;
  pthread_mutex_unlock(&cell->lock);
  return initialized;
}

void aura_lazy_cell_destroy(AuraLazyCell *cell)
{
  if (cell == NULL)
  {
    return;
  }
  pthread_mutex_lock(&cell->lock);
  while (cell->state == 1)
  {
    pthread_cond_wait(&cell->condition, &cell->lock);
  }
  if (cell->value_destroy != NULL && cell->value != NULL)
  {
    cell->value_destroy(cell->value);
    cell->value = NULL;
  }
  if (cell->environment_destroy != NULL && cell->environment != NULL)
  {
    cell->environment_destroy(cell->environment);
    cell->environment = NULL;
  }
  pthread_mutex_unlock(&cell->lock);
  pthread_cond_destroy(&cell->condition);
  pthread_mutex_destroy(&cell->lock);
  free(cell);
}
#endif

struct AuraTaskExecutor
{
  AuraTaskFrame *ready_head;
  AuraTaskFrame *ready_tail;
  AuraTaskFrame *owned_head;
  size_t ready_count;
  size_t owned_count;
  size_t max_live_tasks;
  int shutdown;
  AuraRaceTracker *race_tracker;
  AuraTaskFailureHookFn failure_hook;
  void *failure_hook_context;
  int wake_pipe[2];
  AuraReactor *reactor;
#if defined(AURA_TCP_POSIX)
  pthread_mutex_t worker_lock;
  pthread_cond_t worker_cond;
  pthread_t *workers;
  size_t worker_count;
  int workers_stop;
  int workers_started;
  pthread_t reactor_thread;
  int reactor_stop;
  int reactor_started;
  int reactor_active;
  int active_workers;
  int gc_requested;
#endif
};

struct AuraTaskScope
{
  AuraTaskExecutor *executor;
  AuraTaskScope *previous;
  AuraTaskFrame *frames;
  int active;
};

static _Thread_local AuraTaskScope *aura_task_current_scope = NULL;
static _Thread_local AuraTaskExecutor *aura_task_current_executor = NULL;

static int aura_task_executor_has_workers(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  return executor != NULL && executor->workers_started;
#else
  (void)executor;
  return 0;
#endif
}
static void aura_task_scope_adopt(AuraTaskFrame *frame);
int aura_task_executor_poll_waiting(AuraTaskExecutor *executor, int timeout_ms);

void aura_gc_collect_executor(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->gc_requested = 1;
    pthread_cond_broadcast(&executor->worker_cond);
    int current_worker = aura_task_current_executor == executor;
    while (executor->active_workers > (current_worker ? 1 : 0))
    {
      pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
    }
    pthread_mutex_unlock(&executor->worker_lock);

    aura_gc_collect();

    pthread_mutex_lock(&executor->worker_lock);
    executor->gc_requested = 0;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
    return;
  }
#endif
  aura_gc_collect();
}

static void aura_task_executor_lock(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
    pthread_mutex_lock(&executor->worker_lock);
#else
  (void)executor;
#endif
}

static void aura_task_executor_unlock(AuraTaskExecutor *executor)
{
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
    pthread_mutex_unlock(&executor->worker_lock);
#else
  (void)executor;
#endif
}

#define AURA_TASK_DEFAULT_MAX_LIVE_TASKS ((size_t)4096)
#define AURA_TASK_MAX_LIVE_TASKS_LIMIT ((size_t)65536)

/* Defined with the typed I/O operation implementation below.  Keeping this
 * small bridge here lets the scheduler publish readiness without exposing the
 * operation layout to the executor code. */
static int aura_io_operation_ready(AuraTaskFrame *frame, short revents);

#if defined(AURA_TCP_POSIX)
#define AURA_TASK_MAX_WORKERS ((size_t)64)
static void aura_task_executor_finish_poll_unlocked(
    AuraTaskExecutor *executor, AuraTaskFrame *frame, AuraTaskPollState state);
#endif

int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame);
int aura_task_executor_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame);
static void aura_task_channel_cancel_wait(AuraTaskFrame *frame);
static void aura_task_select_cancel_wait(AuraTaskFrame *frame);
static int aura_task_executor_wake_unlocked(AuraTaskExecutor *executor,
                                            AuraTaskFrame *frame);

static void aura_task_scope_adopt(AuraTaskFrame *frame)
{
  AuraTaskScope *scope = aura_task_current_scope;
  if (frame == NULL || scope == NULL || !scope->active ||
      scope->executor != frame->executor)
  {
    return;
  }
  frame->scope = scope;
  frame->scope_owned = 1;
  frame->scope_next = scope->frames;
  scope->frames = frame;
}

AuraTaskPollState aura_task_frame_poll_once(AuraTaskFrame *frame)
{
  int64_t now;
  if (frame == NULL || frame->poll == NULL)
  {
    return AURA_TASK_FAILED;
  }
  if (frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED)
  {
    return frame->state;
  }
  now = aura_time_monotonic_millis();
  if (frame->cancel_deadline_ms != 0 && now >= frame->cancel_deadline_ms)
  {
    frame->cancel_requested = 1;
  }
  if (frame->cancel_requested)
  {
    /* Cancellation publishes a terminal outcome only after operation and
     * capture ownership has been released.  The frame itself remains
     * executor-owned until join/release, so its terminal metadata is still
     * observable without keeping cancelled work alive. */
    aura_task_frame_request_cancel_children(frame);
    aura_task_frame_storage_release(&frame->pending);
    aura_task_frame_storage_release(&frame->captures);
    aura_task_frame_cleanup_run(frame);
    aura_task_frame_clear_waiting(frame);
    /* Cancellation is terminal unless its bounded cancellation handler
     * publishes an exception. The handler runs after owned cleanup, so an
     * exception raised during cancellation cannot leak the cancelled
     * operation's resources. */
    if (frame->cancel != NULL &&
        frame->cancel(frame) == AURA_TASK_FAILED && frame->error.data != NULL)
    {
      frame->state = AURA_TASK_FAILED;
    }
    else
    {
      frame->state = AURA_TASK_CANCELLED;
    }
    aura_task_frame_wake_waiters(frame);
    return frame->state;
  }
  AuraTaskPollState state = frame->poll(frame);
  if (state < AURA_TASK_READY || state > AURA_TASK_CANCELLED)
  {
    state = AURA_TASK_FAILED;
  }
  if (state == AURA_TASK_FAILED || state == AURA_TASK_CANCELLED)
  {
    aura_task_frame_cleanup_run(frame);
  }
  frame->state = state;
  if (state == AURA_TASK_COMPLETE || state == AURA_TASK_FAILED ||
      state == AURA_TASK_CANCELLED)
  {
    aura_task_frame_wake_waiters(frame);
  }
  return state;
}

AuraTaskExecutor *aura_task_executor_new(void)
{
  AuraTaskExecutor *executor =
      (AuraTaskExecutor *)calloc(1, sizeof(AuraTaskExecutor));
  if (executor != NULL)
  {
    executor->max_live_tasks = AURA_TASK_DEFAULT_MAX_LIVE_TASKS;
    executor->wake_pipe[0] = -1;
    executor->wake_pipe[1] = -1;
    executor->reactor = aura_reactor_posix_new_internal();
    if (executor->reactor == NULL)
    {
      free(executor);
      return NULL;
    }
#if defined(AURA_TCP_POSIX)
    if (pipe(executor->wake_pipe) == 0)
    {
      (void)fcntl(executor->wake_pipe[0], F_SETFL, O_NONBLOCK);
      (void)fcntl(executor->wake_pipe[1], F_SETFL, O_NONBLOCK);
    }
#endif
  }
  return executor;
}

int aura_task_executor_set_max_live_tasks(AuraTaskExecutor *executor,
                                           size_t max_live_tasks)
{
  if (executor == NULL || executor->shutdown || max_live_tasks == 0 ||
      max_live_tasks > AURA_TASK_MAX_LIVE_TASKS_LIMIT ||
      executor->owned_count > max_live_tasks)
  {
    return 0;
  }
  executor->max_live_tasks = max_live_tasks;
  return 1;
}

void aura_task_executor_set_race_tracker(AuraTaskExecutor *executor,
                                         AuraRaceTracker *tracker)
{
  if (executor != NULL && !executor->shutdown)
  {
    executor->race_tracker = tracker;
    aura_race_tracker_set_active(tracker);
  }
}

static void aura_task_default_failure_hook(
    const AuraTaskFailureDiagnostic *diagnostic, void *context)
{
  (void)context;
  if (diagnostic == NULL)
  {
    return;
  }
  fprintf(stderr,
          "aura task failure: task=%" PRIu64 " source=%" PRIu32
          " error_size=%zu\n",
          diagnostic->task_id, diagnostic->source_id, diagnostic->error.size);
}

/* Install the destination for failures that reach terminal state without a
 * successful join.  The diagnostic and its error bytes are borrowed only for
 * the duration of the callback.  Passing NULL restores the deterministic
 * stderr logger, so an unjoined failure is never silently discarded. */
void aura_task_executor_set_failure_hook(AuraTaskExecutor *executor,
                                          AuraTaskFailureHookFn hook,
                                          void *context)
{
  if (executor == NULL || executor->shutdown)
  {
    return;
  }
  executor->failure_hook = hook != NULL ? hook : aura_task_default_failure_hook;
  executor->failure_hook_context = context;
}

static void aura_task_executor_report_unjoined_failure(AuraTaskExecutor *executor,
                                                       AuraTaskFrame *frame)
{
  AuraTaskFailureDiagnostic diagnostic;
  AuraTaskFailureHookFn hook;

  if (executor == NULL || frame == NULL || frame->state != AURA_TASK_FAILED ||
      frame->join_observed || frame->failure_reported)
  {
    return;
  }
  frame->failure_reported = 1;
  diagnostic.task_id = frame->task_id;
  diagnostic.source_id = frame->error_source_id;
  diagnostic.state = frame->state;
  diagnostic.error = frame->error;
  hook = executor->failure_hook != NULL ? executor->failure_hook
                                        : aura_task_default_failure_hook;
  hook(&diagnostic, executor->failure_hook_context);
}

static void aura_task_executor_push_owned(AuraTaskExecutor *executor,
                                           AuraTaskFrame *frame)
{
  frame->owned_next = executor->owned_head;
  executor->owned_head = frame;
  executor->owned_count++;
  frame->executor = executor;
}

int aura_task_executor_submit(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  int result;
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
  }
#endif
  if (executor == NULL || frame == NULL || executor->shutdown || frame->executor != NULL ||
      executor->owned_count >= executor->max_live_tasks)
  {
#if defined(AURA_TCP_POSIX)
    if (executor != NULL && executor->workers_started)
    {
      pthread_mutex_unlock(&executor->worker_lock);
    }
#endif
    return 0;
  }
  aura_task_executor_push_owned(executor, frame);
  frame->handle_owned = 1;
  aura_task_scope_adopt(frame);
  if (executor->race_tracker != NULL)
  {
    (void)aura_race_tracker_record(executor->race_tracker,
                                   frame->task_id,
                                   0,
                                   frame->race_source_id,
                                   AURA_RACE_TASK_SPAWN,
                                   NULL);
  }
  frame->state = AURA_TASK_READY;
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    result = aura_task_executor_wake_unlocked(executor, frame);
    if (result != 0)
    {
      pthread_cond_signal(&executor->worker_cond);
    }
    pthread_mutex_unlock(&executor->worker_lock);
    return result;
  }
#endif
  return aura_task_executor_wake(executor, frame);
}

static int aura_task_executor_wake_unlocked(AuraTaskExecutor *executor,
                                            AuraTaskFrame *frame)
{
  if (executor == NULL || frame == NULL || executor->shutdown || frame->executor != executor ||
      frame->queued || frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  frame->queue_next = NULL;
  frame->queued = 1;
  if (executor->ready_tail == NULL)
  {
    executor->ready_head = frame;
  }
  else
  {
    executor->ready_tail->queue_next = frame;
  }
  executor->ready_tail = frame;
  executor->ready_count++;
  frame->state = AURA_TASK_READY;
  return 1;
}

int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  int result;
#if defined(AURA_TCP_POSIX)
  if (executor != NULL && executor->wake_pipe[1] >= 0)
  {
    const unsigned char signal_byte = 1;
    /* Signal before taking the queue lock so a reactor blocked in poll can
     * observe the wake even while another worker is publishing the queue. */
    (void)write(executor->wake_pipe[1], &signal_byte, sizeof(signal_byte));
  }
  if (executor != NULL && executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    result = aura_task_executor_wake_unlocked(executor, frame);
    if (result != 0)
    {
      pthread_cond_signal(&executor->worker_cond);
    }
    pthread_mutex_unlock(&executor->worker_lock);
    return result;
  }
#endif
  return aura_task_executor_wake_unlocked(executor, frame);
}

static void aura_task_frame_detach_wait_target(AuraTaskFrame *frame)
{
  AuraTaskFrame **link;

  if (frame == NULL || frame->wait_target == NULL)
  {
    return;
  }
  link = &frame->wait_target->waiters_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->waiter_next;
  }
  if (*link == frame)
  {
    *link = frame->waiter_next;
  }
  frame->wait_target = NULL;
  frame->waiter_next = NULL;
  if (frame->waiting_node != NULL)
  {
    frame->waiting_node = NULL;
  }
}

static void aura_task_frame_detach_waiters(AuraTaskFrame *frame)
{
  AuraTaskFrame *waiter;

  if (frame == NULL)
  {
    return;
  }
  waiter = frame->waiters_head;
  frame->waiters_head = NULL;
  while (waiter != NULL)
  {
    AuraTaskFrame *next = waiter->waiter_next;
    waiter->wait_target = NULL;
    waiter->waiter_next = NULL;
    if (waiter->waiting_node == frame)
    {
      waiter->waiting_node = NULL;
    }
    waiter = next;
  }
}

static void aura_task_frame_unlink_cancel_parent(AuraTaskFrame *frame)
{
  AuraTaskFrame **link;

  if (frame == NULL || frame->cancel_parent == NULL)
  {
    return;
  }
  link = &frame->cancel_parent->cancel_children_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->cancel_sibling_next;
  }
  if (*link == frame)
  {
    *link = frame->cancel_sibling_next;
  }
  frame->cancel_parent = NULL;
  frame->cancel_sibling_next = NULL;
}

static void aura_task_frame_detach_cancel_children(AuraTaskFrame *frame)
{
  AuraTaskFrame *child;

  if (frame == NULL)
  {
    return;
  }
  child = frame->cancel_children_head;
  frame->cancel_children_head = NULL;
  while (child != NULL)
  {
    AuraTaskFrame *next = child->cancel_sibling_next;
    child->cancel_parent = NULL;
    child->cancel_sibling_next = NULL;
    child = next;
  }
}

int aura_task_frame_link_cancellation(AuraTaskFrame *parent,
                                      AuraTaskFrame *child)
{
  if (parent == NULL || child == NULL || parent == child ||
      parent->executor == NULL || parent->executor != child->executor ||
      parent->state == AURA_TASK_COMPLETE || parent->state == AURA_TASK_FAILED ||
      parent->state == AURA_TASK_CANCELLED || child->state == AURA_TASK_COMPLETE ||
      child->state == AURA_TASK_FAILED || child->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  aura_task_frame_unlink_cancel_parent(child);
  child->cancel_parent = parent;
  child->cancel_sibling_next = parent->cancel_children_head;
  parent->cancel_children_head = child;
  return 1;
}

static void aura_task_frame_request_cancel_children(AuraTaskFrame *parent)
{
  AuraTaskFrame *child;

  if (parent == NULL)
  {
    return;
  }
  child = parent->cancel_children_head;
  while (child != NULL)
  {
    AuraTaskFrame *next = child->cancel_sibling_next;
    if (child->state != AURA_TASK_COMPLETE && child->state != AURA_TASK_FAILED &&
        child->state != AURA_TASK_CANCELLED && child->executor != NULL)
    {
      child->cancel_requested = 1;
      aura_task_channel_cancel_wait(child);
      aura_task_frame_detach_wait_target(child);
      aura_task_frame_clear_waiting(child);
      if (!child->queued)
      {
        (void)aura_task_executor_wake(child->executor, child);
      }
    }
    child = next;
  }
}

static void aura_task_frame_wake_waiters(AuraTaskFrame *frame)
{
  AuraTaskFrame *waiter;

  if (frame == NULL)
  {
    return;
  }
  waiter = frame->waiters_head;
  frame->waiters_head = NULL;
  while (waiter != NULL)
  {
    AuraTaskFrame *next = waiter->waiter_next;
    waiter->wait_target = NULL;
    waiter->waiter_next = NULL;
    if (waiter->waiting_node == frame)
    {
      waiter->waiting_node = NULL;
    }
    if (waiter->executor != NULL && !waiter->executor->shutdown &&
        waiter->state != AURA_TASK_COMPLETE && waiter->state != AURA_TASK_FAILED &&
        waiter->state != AURA_TASK_CANCELLED)
    {
      (void)aura_task_executor_wake(waiter->executor, waiter);
    }
    waiter = next;
  }
}

/* Register a parent frame against one child frame. The child owns no parent
 * memory; the embedded links are detached on cancellation/destruction and
 * all waiters are queued exactly once when the child becomes terminal. */
int aura_task_frame_wait_on(AuraTaskFrame *frame, AuraTaskFrame *target)
{
  if (frame == NULL || target == NULL || frame == target ||
      frame->executor == NULL || frame->executor != target->executor ||
      frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED || target->state == AURA_TASK_COMPLETE ||
      target->state == AURA_TASK_FAILED || target->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  aura_task_frame_detach_wait_target(frame);
  frame->wait_target = target;
  frame->waiter_next = target->waiters_head;
  target->waiters_head = frame;
  frame->waiting_node = target;
  frame->state = AURA_TASK_PENDING;
  return 1;
}

/* Complete an adapter-owned wait registration and queue the frame in one
 * bounded-runtime operation. The token is cleared before queueing so a
 * completion/failure callback cannot wake the same frame twice or leave a
 * borrowed registration visible while the poller resumes. */
int aura_task_executor_wake_waiting(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  if (executor == NULL || frame == NULL || frame->executor != executor ||
      !aura_task_frame_is_waiting(frame))
  {
    return 0;
  }
  aura_task_frame_clear_waiting(frame);
  return aura_task_executor_wake(executor, frame);
}

/* Poll all executor-owned frames registered with wait_fd. A zero return means
 * no descriptor became ready before timeout; a positive return is the number
 * of frames cleared and queued. This bounded single-threaded API provides a
 * deterministic multi-descriptor readiness turn without claiming a full
 * cross-platform event-loop policy. */
static int aura_posix_reactor_poll(void *data, AuraTaskExecutor *executor,
                                   int timeout_ms)
{
  AuraTaskFrame *frame;
  struct pollfd *descriptors;
  AuraTaskFrame **frames;
  size_t count = 0;
  size_t descriptor_count;
  size_t index = 0;
  size_t woke = 0;
  int pipe_woke = 0;
  int result;
  int64_t now;
  int poll_timeout = timeout_ms;
  int has_deadline = 0;

  (void)data;
  if (executor == NULL || executor->shutdown || timeout_ms < 0)
  {
    return 0;
  }
#if defined(AURA_TCP_POSIX)
  /* Consume stale submissions before blocking; a wake arriving after this
   * drain remains visible to poll and interrupts the wait. */
  if (executor->wake_pipe[0] >= 0)
  {
    unsigned char buffer[64];
    while (read(executor->wake_pipe[0], buffer, sizeof(buffer)) > 0) {}
  }
#endif
  for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
  {
    if (frame->state != AURA_TASK_PENDING)
    {
      continue;
    }
    if (frame->cancel_deadline_ms != 0)
    {
      int64_t remaining = frame->cancel_deadline_ms - aura_time_monotonic_millis();
      if (remaining <= 0)
      {
        woke += (size_t)aura_task_executor_cancel(executor, frame);
        continue;
      }
      if (remaining < poll_timeout) poll_timeout = (int)remaining;
      has_deadline = 1;
    }
    if (frame->waiting_node == NULL)
    {
      continue;
    }
    if (frame->fd_wait_deadline_ms != 0)
    {
      int64_t remaining;
      now = aura_time_monotonic_millis();
      remaining = frame->fd_wait_deadline_ms - now;
      if (remaining <= 0)
      {
        frame->fd_wait_timed_out = 1;
        woke += (size_t)aura_task_executor_wake_waiting(executor, frame);
        continue;
      }
      if (remaining < poll_timeout)
      {
        poll_timeout = (int)remaining;
      }
      has_deadline = 1;
    }
    if (frame->fd_wait_active)
    {
      count++;
    }
  }
  if (woke != 0)
  {
    return (int)woke;
  }
  descriptor_count = count;
#if defined(AURA_TCP_POSIX)
  if (executor->wake_pipe[0] >= 0)
  {
    descriptor_count++;
  }
#endif
  if (descriptor_count == 0 && !has_deadline)
  {
    return 0;
  }
  if (descriptor_count == 0)
  {
    (void)poll(NULL, 0, poll_timeout);
    now = aura_time_monotonic_millis();
    for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
    {
      if (frame->state != AURA_TASK_PENDING) continue;
      if (frame->cancel_deadline_ms != 0 && now >= frame->cancel_deadline_ms)
      {
        woke += (size_t)aura_task_executor_cancel(executor, frame);
        continue;
      }
      if (frame->waiting_node == NULL || frame->fd_wait_deadline_ms == 0 ||
          now < frame->fd_wait_deadline_ms)
      {
        continue;
      }
      frame->fd_wait_timed_out = 1;
      woke += (size_t)aura_task_executor_wake_waiting(executor, frame);
    }
    return (int)woke;
  }
  descriptors = (struct pollfd *)calloc(descriptor_count, sizeof(*descriptors));
  frames = (AuraTaskFrame **)calloc(descriptor_count, sizeof(*frames));
  if (descriptors == NULL || frames == NULL)
  {
    free(descriptors);
    free(frames);
    return 0;
  }
  index = 0;
#if defined(AURA_TCP_POSIX)
  if (executor->wake_pipe[0] >= 0)
  {
    descriptors[index] = (struct pollfd){executor->wake_pipe[0], POLLIN, 0};
    frames[index] = NULL;
    index++;
  }
#endif
  for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
  {
    if (!frame->fd_wait_active || frame->waiting_node == NULL ||
        frame->state != AURA_TASK_PENDING)
    {
      continue;
    }
    descriptors[index] = (struct pollfd){
      frame->fd_wait_fd,
      frame->fd_wait_events,
      0,
    };
    frames[index] = frame;
    index++;
  }
  result = poll(descriptors, descriptor_count, poll_timeout);
  if (result > 0 || (result < 0 && errno != EINTR))
  {
    for (index = 0; index < descriptor_count; index++)
    {
      if (result < 0 || descriptors[index].revents != 0)
      {
        AuraTaskFrame *ready_frame = frames[index];
#if defined(AURA_TCP_POSIX)
        if (ready_frame == NULL)
        {
          unsigned char buffer[64];
          while (read(executor->wake_pipe[0], buffer, sizeof(buffer)) > 0) {}
          pipe_woke = 1;
          continue;
        }
#endif
        if (result >= 0)
        {
          int operation_result =
              aura_io_operation_ready(ready_frame, descriptors[index].revents);
          if (operation_result > 0)
          {
            woke++;
          }
          else if (operation_result == 0)
          {
            woke += (size_t)aura_task_executor_wake_waiting(executor,
                                                             ready_frame);
          }
          /* A typed operation can consume a short nonblocking write and stay
           * pending. Its fd registration remains active for the next poll. */
        }
        else
        {
          woke += (size_t)aura_task_executor_wake_waiting(executor,
                                                           ready_frame);
        }
      }
    }
  }
  now = aura_time_monotonic_millis();
  for (frame = executor->owned_head; frame != NULL; frame = frame->owned_next)
  {
    if (frame->state == AURA_TASK_PENDING && frame->cancel_deadline_ms != 0 &&
        now >= frame->cancel_deadline_ms)
    {
      woke += (size_t)aura_task_executor_cancel(executor, frame);
      continue;
    }
    if (frame->waiting_node == NULL || frame->state != AURA_TASK_PENDING ||
        frame->fd_wait_deadline_ms == 0 || now < frame->fd_wait_deadline_ms)
    {
      continue;
    }
    frame->fd_wait_timed_out = 1;
    woke += (size_t)aura_task_executor_wake_waiting(executor, frame);
  }
  free(descriptors);
  free(frames);
  if (woke == 0 && pipe_woke && executor->ready_count != 0)
  {
    return 1;
  }
  return (int)woke;
}

AuraReactor *aura_reactor_new(AuraReactorPollFn poll, void *data,
                              AuraReactorDestroyFn data_destroy)
{
  AuraReactor *reactor;
  if (poll == NULL)
  {
    return NULL;
  }
  reactor = (AuraReactor *)calloc(1, sizeof(*reactor));
  if (reactor == NULL)
  {
    return NULL;
  }
  reactor->abi_version = AURA_REACTOR_ABI_VERSION;
  reactor->poll = poll;
  reactor->data = data;
  reactor->data_destroy = data_destroy;
  return reactor;
}

static AuraReactor *aura_reactor_posix_new_internal(void)
{
  return aura_reactor_new(aura_posix_reactor_poll, NULL, NULL);
}

AuraReactor *aura_reactor_posix_new(void)
{
  return aura_reactor_posix_new_internal();
}

void aura_reactor_destroy(AuraReactor *reactor)
{
  if (reactor == NULL)
  {
    return;
  }
  if (reactor->data_destroy != NULL && reactor->data != NULL)
  {
    reactor->data_destroy(reactor->data);
    reactor->data = NULL;
  }
  free(reactor);
}

int aura_task_executor_set_reactor(AuraTaskExecutor *executor,
                                   AuraReactor *reactor)
{
  AuraReactor *replacement = reactor;
  if (executor == NULL || executor->shutdown || executor->owned_count != 0 ||
      executor->ready_count != 0 ||
      aura_task_executor_has_workers(executor))
  {
    return 0;
  }
  if (replacement == NULL)
  {
    replacement = aura_reactor_posix_new_internal();
    if (replacement == NULL)
    {
      return 0;
    }
  }
  if (replacement->abi_version != AURA_REACTOR_ABI_VERSION ||
      replacement->poll == NULL)
  {
    if (replacement != reactor)
    {
      aura_reactor_destroy(replacement);
    }
    return 0;
  }
  aura_reactor_destroy(executor->reactor);
  executor->reactor = replacement;
  return 1;
}

int aura_task_executor_poll_waiting(AuraTaskExecutor *executor, int timeout_ms)
{
  int result;
  if (executor == NULL) return 0;
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->reactor_active++;
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  result = executor->reactor != NULL && executor->reactor->poll != NULL
               ? executor->reactor->poll(executor->reactor->data, executor,
                                         timeout_ms)
               : aura_posix_reactor_poll(NULL, executor, timeout_ms);
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    if (executor->reactor_active != 0) executor->reactor_active--;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  return result;
}

int aura_task_executor_cancel(AuraTaskExecutor *executor, AuraTaskFrame *frame)
{
  if (executor == NULL || frame == NULL || frame->executor != executor || executor->shutdown)
  {
    return 0;
  }
  if (frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED)
  {
    return 0;
  }
  frame->cancel_requested = 1;
  if (frame->waiting_select != NULL)
    aura_task_select_cancel_wait(frame);
  else
    aura_task_channel_cancel_wait(frame);
  aura_task_frame_detach_wait_target(frame);
  aura_task_frame_clear_waiting(frame);
  if (!frame->queued)
  {
    aura_task_executor_wake(executor, frame);
  }
  return 1;
}

size_t aura_task_executor_ready_count(const AuraTaskExecutor *executor)
{
  size_t count = 0;
  if (executor != NULL)
  {
    aura_task_executor_lock((AuraTaskExecutor *)executor);
    count = executor->ready_count;
    aura_task_executor_unlock((AuraTaskExecutor *)executor);
  }
  return count;
}

size_t aura_task_executor_task_count(const AuraTaskExecutor *executor)
{
  size_t count = 0;
  if (executor != NULL)
  {
    aura_task_executor_lock((AuraTaskExecutor *)executor);
    count = executor->owned_count;
    aura_task_executor_unlock((AuraTaskExecutor *)executor);
  }
  return count;
}

static AuraTaskFrame *aura_task_executor_pop_ready_unlocked(
    AuraTaskExecutor *executor)
{
  AuraTaskFrame *frame;
  if (executor == NULL || executor->ready_head == NULL)
  {
    return NULL;
  }
  frame = executor->ready_head;
  executor->ready_head = frame->queue_next;
  if (executor->ready_head == NULL)
  {
    executor->ready_tail = NULL;
  }
  frame->queue_next = NULL;
  frame->queued = 0;
  executor->ready_count--;
  return frame;
}

static void aura_task_executor_finish_poll_unlocked(
    AuraTaskExecutor *executor, AuraTaskFrame *frame, AuraTaskPollState state)
{
  if (executor == NULL || frame == NULL)
  {
    return;
  }
  if (state == AURA_TASK_READY)
  {
    (void)aura_task_executor_wake_unlocked(executor, frame);
  }
  else if (state == AURA_TASK_PENDING || state == AURA_TASK_COMPLETE ||
           state == AURA_TASK_FAILED || state == AURA_TASK_CANCELLED)
  {
    frame->state = state;
  }
  else
  {
    frame->state = AURA_TASK_FAILED;
  }
  if (executor->race_tracker != NULL &&
      (state == AURA_TASK_COMPLETE || state == AURA_TASK_FAILED ||
       state == AURA_TASK_CANCELLED))
  {
    AuraRaceEventKind kind = AURA_RACE_TASK_COMPLETE;
    if (state == AURA_TASK_FAILED)
    {
      kind = AURA_RACE_TASK_FAILED;
    }
    else if (state == AURA_TASK_CANCELLED)
    {
      kind = AURA_RACE_TASK_CANCELLED;
    }
    (void)aura_race_tracker_record(executor->race_tracker, frame->task_id, 0,
                                   frame->race_source_id, kind, NULL);
  }
}

#if defined(AURA_TCP_POSIX)
static void *aura_task_executor_worker_main(void *context)
{
  AuraTaskExecutor *executor = (AuraTaskExecutor *)context;
  for (;;)
  {
    AuraTaskFrame *frame;
    AuraTaskPollState state;
    AuraTaskScope *previous_scope;
    pthread_mutex_lock(&executor->worker_lock);
    while ((executor->ready_head == NULL || executor->gc_requested) &&
           !executor->workers_stop)
    {
      pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
    }
    if (executor->ready_head == NULL && executor->workers_stop)
    {
      pthread_mutex_unlock(&executor->worker_lock);
      return NULL;
    }
    frame = aura_task_executor_pop_ready_unlocked(executor);
    if (frame != NULL) executor->active_workers++;
    pthread_mutex_unlock(&executor->worker_lock);
    if (frame == NULL)
    {
      continue;
    }
    previous_scope = aura_task_current_scope;
    aura_task_current_scope = frame->scope;
    AuraTaskExecutor *previous_executor = aura_task_current_executor;
    aura_task_current_executor = executor;
    aura_race_active_task_id = frame->task_id;
    aura_race_active_source_id = frame->race_source_id;
    if (frame->blocking_fn != NULL && !frame->blocking_started)
      aura_task_blocking_run_inline(frame);
    state = aura_task_frame_poll_once(frame);
    aura_race_active_task_id = 0;
    aura_race_active_source_id = 0;
    aura_task_current_scope = previous_scope;
    aura_task_current_executor = previous_executor;
    pthread_mutex_lock(&executor->worker_lock);
    aura_task_executor_finish_poll_unlocked(executor, frame, state);
    if (executor->active_workers != 0) executor->active_workers--;
    pthread_cond_broadcast(&executor->worker_cond);
    if (state == AURA_TASK_READY)
    {
      pthread_cond_signal(&executor->worker_cond);
    }
    pthread_mutex_unlock(&executor->worker_lock);
  }
}

static void *aura_task_executor_reactor_main(void *context)
{
  AuraTaskExecutor *executor = (AuraTaskExecutor *)context;
  for (;;)
  {
    pthread_mutex_lock(&executor->worker_lock);
    int stopping = executor->reactor_stop;
    pthread_mutex_unlock(&executor->worker_lock);
    if (stopping) return NULL;
    (void)aura_task_executor_poll_waiting(executor, 100);
  }
}

int aura_task_executor_start_workers(AuraTaskExecutor *executor,
                                     size_t worker_count)
{
  size_t started = 0;
  if (executor == NULL || executor->shutdown || executor->workers_started ||
      worker_count == 0 || worker_count > AURA_TASK_MAX_WORKERS)
  {
    return 0;
  }
  if (pthread_mutex_init(&executor->worker_lock, NULL) != 0)
  {
    return 0;
  }
  if (pthread_cond_init(&executor->worker_cond, NULL) != 0)
  {
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->workers = (pthread_t *)calloc(worker_count, sizeof(*executor->workers));
  if (executor->workers == NULL)
  {
    pthread_cond_destroy(&executor->worker_cond);
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->worker_count = worker_count;
  executor->workers_stop = 0;
  executor->workers_started = 1;
  for (; started < worker_count; started++)
  {
    if (pthread_create(&executor->workers[started], NULL,
                       aura_task_executor_worker_main, executor) != 0)
    {
      break;
    }
  }
  if (started != worker_count)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->workers_stop = 1;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
    for (size_t i = 0; i < started; i++)
    {
      pthread_join(executor->workers[i], NULL);
    }
    free(executor->workers);
    executor->workers = NULL;
    executor->worker_count = 0;
    executor->workers_started = 0;
    pthread_cond_destroy(&executor->worker_cond);
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->reactor_stop = 0;
  if (pthread_create(&executor->reactor_thread, NULL,
                    aura_task_executor_reactor_main, executor) != 0)
  {
    pthread_mutex_lock(&executor->worker_lock);
    executor->workers_stop = 1;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
    for (size_t i = 0; i < worker_count; i++)
      pthread_join(executor->workers[i], NULL);
    free(executor->workers);
    executor->workers = NULL;
    executor->worker_count = 0;
    executor->workers_started = 0;
    pthread_cond_destroy(&executor->worker_cond);
    pthread_mutex_destroy(&executor->worker_lock);
    return 0;
  }
  executor->reactor_started = 1;
  return 1;
}

void aura_task_executor_stop_workers(AuraTaskExecutor *executor)
{
  if (executor == NULL || !executor->workers_started)
  {
    return;
  }
  pthread_mutex_lock(&executor->worker_lock);
  executor->reactor_stop = 1;
  pthread_mutex_unlock(&executor->worker_lock);
  if (executor->reactor_started)
  {
    if (executor->wake_pipe[1] >= 0)
    {
      const unsigned char signal_byte = 1;
      (void)write(executor->wake_pipe[1], &signal_byte, sizeof(signal_byte));
    }
    pthread_join(executor->reactor_thread, NULL);
    executor->reactor_started = 0;
  }
  pthread_mutex_lock(&executor->worker_lock);
  executor->workers_stop = 1;
  pthread_cond_broadcast(&executor->worker_cond);
  pthread_mutex_unlock(&executor->worker_lock);
  for (size_t i = 0; i < executor->worker_count; i++)
  {
    pthread_join(executor->workers[i], NULL);
  }
  free(executor->workers);
  executor->workers = NULL;
  executor->worker_count = 0;
  executor->workers_started = 0;
  pthread_cond_destroy(&executor->worker_cond);
  pthread_mutex_destroy(&executor->worker_lock);
}
#endif

int aura_task_executor_run_one(AuraTaskExecutor *executor)
{
  AuraTaskFrame *frame;
  AuraTaskPollState state;
  AuraTaskScope *previous_scope;
  uint64_t previous_task_id;
  uint32_t previous_source_id;
  if (executor == NULL || executor->shutdown)
  {
    return 0;
  }
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
    while (executor->gc_requested && !executor->workers_stop)
      pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
  }
#endif
  frame = aura_task_executor_pop_ready_unlocked(executor);
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started && frame != NULL) executor->active_workers++;
  if (executor->workers_started)
  {
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  if (frame == NULL)
  {
    return 0;
  }
  previous_task_id = aura_race_active_task_id;
  previous_source_id = aura_race_active_source_id;
  previous_scope = aura_task_current_scope;
  aura_task_current_scope = frame->scope;
  AuraTaskExecutor *previous_executor = aura_task_current_executor;
  aura_task_current_executor = executor;
  aura_race_active_task_id = frame->task_id;
  aura_race_active_source_id = frame->race_source_id;
  if (frame->blocking_fn != NULL && !frame->blocking_started)
    aura_task_blocking_run_inline(frame);
  state = aura_task_frame_poll_once(frame);
  aura_race_active_task_id = previous_task_id;
  aura_race_active_source_id = previous_source_id;
  aura_task_current_scope = previous_scope;
  aura_task_current_executor = previous_executor;
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    pthread_mutex_lock(&executor->worker_lock);
  }
#endif
  aura_task_executor_finish_poll_unlocked(executor, frame, state);
#if defined(AURA_TCP_POSIX)
  if (executor->workers_started)
  {
    if (executor->active_workers != 0) executor->active_workers--;
    pthread_cond_broadcast(&executor->worker_cond);
    pthread_mutex_unlock(&executor->worker_lock);
  }
#endif
  return 1;
}

size_t aura_task_executor_run(AuraTaskExecutor *executor)
{
  size_t polled = 0;
  while (aura_task_executor_run_one(executor) != 0)
  {
    polled++;
  }
  return polled;
}

int aura_task_executor_has_live_tasks(const AuraTaskExecutor *executor)
{
  int live = 0;
  if (executor == NULL || executor->shutdown)
  {
    return 0;
  }
  aura_task_executor_lock((AuraTaskExecutor *)executor);
  for (const AuraTaskFrame *frame = executor->owned_head; frame != NULL;
       frame = frame->owned_next)
  {
    if (frame->state != AURA_TASK_COMPLETE && frame->state != AURA_TASK_FAILED &&
        frame->state != AURA_TASK_CANCELLED)
    {
      live = 1;
      break;
    }
  }
  aura_task_executor_unlock((AuraTaskExecutor *)executor);
  return live;
}

/* Observe a frame owned by this executor. Joining an unsubmitted frame
 * submits it exactly once; joining an already-owned frame only observes it.
 * Result and error snapshots are borrowed from executor-owned frame storage.
 * A PENDING result is explicit: no wake source is available to this bounded
 * single-threaded helper, so it does not pretend to support delayed awaits. */
AuraTaskOutcome aura_task_executor_join_outcome(AuraTaskExecutor *executor,
                                                AuraTaskFrame *frame)
{
  AuraTaskOutcome outcome = {AURA_TASK_FAILED, {NULL, 0}, {NULL, 0}};
  if (executor == NULL || frame == NULL || executor->shutdown)
  {
    return outcome;
  }
  if (frame->executor == NULL && frame->state != AURA_TASK_COMPLETE &&
      frame->state != AURA_TASK_FAILED && frame->state != AURA_TASK_CANCELLED)
  {
    if (!aura_task_executor_submit(executor, frame))
    {
      return outcome;
    }
  }
  else if (frame->executor != NULL && frame->executor != executor)
  {
    return outcome;
  }

  while (frame->state != AURA_TASK_COMPLETE &&
         frame->state != AURA_TASK_FAILED &&
         frame->state != AURA_TASK_CANCELLED)
  {
    if (aura_task_executor_run_one(executor) != 0)
    {
      continue;
    }
    /* A compiler-generated I/O frame can be the only ready source left. */
    if (aura_task_executor_poll_waiting(executor, 1000) == 0)
    {
      break;
    }
  }

  outcome.state = frame->state;
  outcome.result = frame->result;
  outcome.error = frame->error;
  if (frame->state == AURA_TASK_FAILED)
  {
    frame->join_observed = 1;
  }
  if (executor->race_tracker != NULL &&
      (frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
       frame->state == AURA_TASK_CANCELLED))
  {
    (void)aura_race_tracker_record(executor->race_tracker,
                                   frame->task_id,
                                   aura_race_active_source_id,
                                   0,
                                   AURA_RACE_TASK_JOIN,
                                   NULL);
  }
  return outcome;
}

int aura_task_outcome_clone(const AuraTaskOutcome *source,
                            AuraTaskResultCloneFn result_clone,
                            AuraTaskResultDestroyFn result_destroy,
                            AuraTaskResultCloneFn error_clone,
                            AuraTaskResultDestroyFn error_destroy,
                            AuraTaskOwnedOutcome *out)
{
  size_t cloned_size = 0;

  if (source == NULL || out == NULL)
  {
    return 0;
  }
  *out = (AuraTaskOwnedOutcome){source->state, {NULL, 0}, {NULL, 0}, NULL, NULL};
  if (source->state == AURA_TASK_COMPLETE && source->result.data != NULL)
  {
    if (result_clone == NULL || result_destroy == NULL)
    {
      return 0;
    }
    out->result.data = result_clone(source->result.data, source->result.size,
                                    &cloned_size);
    if (out->result.data == NULL)
    {
      return 0;
    }
    out->result.size = cloned_size;
    out->result_destroy = result_destroy;
  }
  if (source->state == AURA_TASK_FAILED && source->error.data != NULL)
  {
    cloned_size = 0;
    if (error_clone == NULL || error_destroy == NULL)
    {
      aura_task_owned_outcome_destroy(out);
      return 0;
    }
    out->error.data = error_clone(source->error.data, source->error.size,
                                  &cloned_size);
    if (out->error.data == NULL)
    {
      aura_task_owned_outcome_destroy(out);
      return 0;
    }
    out->error.size = cloned_size;
    out->error_destroy = error_destroy;
  }
  return 1;
}

void aura_task_owned_outcome_destroy(AuraTaskOwnedOutcome *out)
{
  if (out == NULL)
  {
    return;
  }
  if (out->result.data != NULL && out->result_destroy != NULL)
  {
    out->result_destroy(out->result.data, out->result.size);
  }
  if (out->error.data != NULL && out->error_destroy != NULL)
  {
    out->error_destroy(out->error.data, out->error.size);
  }
  *out = (AuraTaskOwnedOutcome){0, {NULL, 0}, {NULL, 0}, NULL, NULL};
}

AuraTaskPollState aura_task_executor_join(AuraTaskExecutor *executor,
                                          AuraTaskFrame *frame,
                                          AuraTaskResult *out_result,
                                          AuraTaskResult *out_error)
{
  AuraTaskOutcome outcome = aura_task_executor_join_outcome(executor, frame);
  if (out_result != NULL)
  {
    *out_result = outcome.result;
  }
  if (out_error != NULL)
  {
    *out_error = outcome.error;
  }
  return outcome.state;
}

/* Remove a frame from the FIFO ready queue before releasing it. */
static int aura_task_executor_unqueue(AuraTaskExecutor *executor,
                                       AuraTaskFrame *frame)
{
  AuraTaskFrame **link;

  if (executor == NULL || frame == NULL || !frame->queued)
  {
    return frame != NULL && !frame->queued;
  }
  link = &executor->ready_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->queue_next;
  }
  if (*link == NULL)
  {
    return 0;
  }
  *link = frame->queue_next;
  if (executor->ready_tail == frame)
  {
    executor->ready_tail = NULL;
    for (AuraTaskFrame *tail = executor->ready_head; tail != NULL;
         tail = tail->queue_next)
    {
      executor->ready_tail = tail;
    }
  }
  frame->queue_next = NULL;
  frame->queued = 0;
  if (executor->ready_count != 0)
  {
    executor->ready_count--;
  }
  return 1;
}

/* Release an executor-owned frame through its task-handle slot.
 *
 * The pointer-to-pointer API is intentional: releasing also clears the
 * caller's handle, making repeated release and dropped-handle cleanup
 * idempotent without dereferencing freed storage.  A non-terminal frame is
 * cancelled and polled to acknowledge cancellation before it is unlinked.
 * The owned list is singly linked, so unlink the exact node before destroying
 * it; shutdown can then walk the remaining list without observing freed nodes.
 */
int aura_task_executor_release(AuraTaskExecutor *executor, AuraTaskFrame **handle)
{
  AuraTaskFrame *frame;
  AuraTaskFrame **link;

  if (handle == NULL || *handle == NULL)
  {
    return 1;
  }
  frame = *handle;
  if (executor == NULL || executor->shutdown || frame->executor != executor)
  {
    return 0;
  }
  if (frame->scope_owned)
  {
    /* The lexical handle gives up only its reference; a scope or scheduler
     * payload keeps the executor-owned frame alive until its final release. */
    frame->handle_owned = 0;
    *handle = NULL;
    return 1;
  }
  if (frame->payload_refs != 0)
  {
    /* A task moved through a scheduler payload has two owners: its original
     * lexical handle and the payload reference. The first ordinary release
     * drops the lexical handle; the next one consumes the transferred ref.
     */
    if (frame->handle_owned)
    {
      frame->handle_owned = 0;
      *handle = NULL;
      return 1;
    }
    aura_task_executor_lock(executor);
    if (frame->payload_refs == 0 || frame->executor != executor)
    {
      aura_task_executor_unlock(executor);
      return 0;
    }
    frame->payload_refs--;
    int final_payload = frame->payload_refs == 0;
    aura_task_executor_unlock(executor);
    *handle = NULL;
    if (final_payload)
    {
      AuraTaskFrame *owned = frame;
      return aura_task_executor_release(executor, &owned);
    }
    return 1;
  }
  if (frame->state != AURA_TASK_COMPLETE && frame->state != AURA_TASK_FAILED &&
      frame->state != AURA_TASK_CANCELLED)
  {
    if (!aura_task_executor_cancel(executor, frame))
    {
      return 0;
    }
#if defined(AURA_TCP_POSIX)
    if (executor->workers_started)
    {
      pthread_mutex_lock(&executor->worker_lock);
      while (frame->state != AURA_TASK_COMPLETE &&
             frame->state != AURA_TASK_FAILED &&
             frame->state != AURA_TASK_CANCELLED && !executor->shutdown)
      {
        pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
      }
      pthread_mutex_unlock(&executor->worker_lock);
    }
    else
#endif
    if (!aura_task_executor_unqueue(executor, frame) ||
        (aura_task_frame_poll_once(frame) != AURA_TASK_CANCELLED &&
         frame->state != AURA_TASK_FAILED))
    {
      return 0;
    }
  }
  aura_task_executor_lock(executor);
#if defined(AURA_TCP_POSIX)
  while (executor->workers_started && executor->reactor_active != 0)
  {
    pthread_cond_wait(&executor->worker_cond, &executor->worker_lock);
  }
#endif
  if (frame->queued || frame->waiting_channel != NULL ||
      frame->waiting_node != NULL)
  {
    aura_task_executor_unlock(executor);
    return 0;
  }

  link = &executor->owned_head;
  while (*link != NULL && *link != frame)
  {
    link = &(*link)->owned_next;
  }
  if (*link == NULL)
  {
    aura_task_executor_unlock(executor);
    return 0;
  }
  *link = frame->owned_next;
  frame->owned_next = NULL;
  frame->executor = NULL;
  if (executor->owned_count != 0)
  {
    executor->owned_count--;
  }
  aura_task_executor_unlock(executor);
  *handle = NULL;
  frame->handle_owned = 0;
  aura_task_executor_report_unjoined_failure(executor, frame);
  aura_task_frame_destroy(frame);
  return 1;
}

/* Retain one scheduler-owned payload reference without copying the raw frame
 * pointer. The matching release_payload call may outlive the lexical handle. */
int aura_task_executor_retain_payload(AuraTaskExecutor *executor,
                                       AuraTaskFrame *frame)
{
  if (executor == NULL || frame == NULL || executor->shutdown ||
      frame->executor != executor)
  {
    return 0;
  }
  aura_task_executor_lock(executor);
  if (executor->shutdown || frame->executor != executor)
  {
    aura_task_executor_unlock(executor);
    return 0;
  }
  frame->payload_refs++;
  aura_task_executor_unlock(executor);
  return 1;
}

int aura_task_executor_release_payload(AuraTaskExecutor *executor,
                                        AuraTaskFrame **payload)
{
  if (payload == NULL || *payload == NULL)
  {
    return 1;
  }
  AuraTaskFrame *frame = *payload;
  /* Shutdown detaches payload-held frames before freeing the executor. The
   * payload still owns the frame and can finish destruction independently. */
  if (frame->executor == NULL)
  {
    *payload = NULL;
    if (frame->payload_refs != 0)
    {
      frame->payload_refs--;
      if (frame->payload_refs == 0)
      {
        aura_task_frame_destroy(frame);
      }
      return 1;
    }
    return 0;
  }
  if (executor == NULL || executor->shutdown || frame->executor != executor)
  {
    return 0;
  }
  aura_task_executor_lock(executor);
  if (frame->payload_refs == 0 || frame->executor != executor)
  {
    aura_task_executor_unlock(executor);
    return 0;
  }
  frame->payload_refs--;
  int final = frame->payload_refs == 0 && !frame->handle_owned &&
              !frame->scope_owned;
  aura_task_executor_unlock(executor);
  *payload = NULL;
  if (final)
  {
    AuraTaskFrame *owned = frame;
    return aura_task_executor_release(executor, &owned);
  }
  return 1;
}

AuraTaskScope *aura_task_scope_begin(AuraTaskExecutor *executor)
{
  AuraTaskScope *scope;
  if (executor == NULL || executor->shutdown)
  {
    return NULL;
  }
  scope = (AuraTaskScope *)calloc(1, sizeof(*scope));
  if (scope == NULL)
  {
    return NULL;
  }
  scope->executor = executor;
  scope->previous = aura_task_current_scope;
  scope->active = 1;
  aura_task_current_scope = scope;
  return scope;
}

static int aura_task_scope_has_live_frames(const AuraTaskScope *scope)
{
  for (const AuraTaskFrame *frame = scope->frames; frame != NULL;
       frame = frame->scope_next)
  {
    if (frame->state != AURA_TASK_COMPLETE &&
        frame->state != AURA_TASK_FAILED &&
        frame->state != AURA_TASK_CANCELLED)
    {
      return 1;
    }
  }
  return 0;
}

int aura_task_scope_end(AuraTaskScope *scope)
{
  AuraTaskFrame *frame;
  int outcome = 0;
  if (scope == NULL || !scope->active)
  {
    return 0;
  }
  scope->active = 0;
  if (aura_task_current_scope == scope)
  {
    aura_task_current_scope = scope->previous;
  }
  while (aura_task_scope_has_live_frames(scope))
  {
    if (aura_task_executor_run_one(scope->executor) == 0 &&
        aura_task_executor_poll_waiting(scope->executor, 1000) == 0)
    {
      break;
    }
  }
  for (frame = scope->frames; frame != NULL; frame = frame->scope_next)
  {
    if (frame->state == AURA_TASK_FAILED) outcome = 1;
    else if (frame->state == AURA_TASK_CANCELLED && outcome == 0) outcome = 2;
  }
  frame = scope->frames;
  scope->frames = NULL;
  while (frame != NULL)
  {
    AuraTaskFrame *next = frame->scope_next;
    frame->scope_next = NULL;
    frame->scope = NULL;
    frame->scope_owned = 0;
    if (!frame->handle_owned && frame->executor == scope->executor)
    {
      AuraTaskFrame *owned = frame;
      (void)aura_task_executor_release(scope->executor, &owned);
    }
    frame = next;
  }
  free(scope);
  return outcome;
}

/* Release a frame whose terminal state was observed by a direct parent poll.
 * Normal task handles keep the stricter queued-frame rule because their result
 * may still be borrowed by generated outcome code. */
int aura_task_executor_release_terminal(AuraTaskExecutor *executor,
                                         AuraTaskFrame **handle)
{
  if (handle == NULL || *handle == NULL || executor == NULL || executor->shutdown)
  {
    return handle == NULL || *handle == NULL ? 1 : 0;
  }
  AuraTaskFrame *frame = *handle;
  if (frame->state != AURA_TASK_COMPLETE && frame->state != AURA_TASK_FAILED &&
      frame->state != AURA_TASK_CANCELLED)
  {
    return 0;
  }
  if (frame->queued && !aura_task_executor_unqueue(executor, frame))
  {
    return 0;
  }
  return aura_task_executor_release(executor, handle);
}

void aura_task_executor_shutdown(AuraTaskExecutor *executor)
{
  if (executor == NULL || executor->shutdown)
  {
    return;
  }
#if defined(AURA_TCP_POSIX)
  aura_task_executor_stop_workers(executor);
#endif
  executor->shutdown = 1;
  AuraTaskFrame *frame = executor->owned_head;
  executor->owned_head = NULL;
  executor->owned_count = 0;
  while (frame != NULL)
  {
    AuraTaskFrame *next = frame->owned_next;
    frame->owned_next = NULL;
    frame->executor = NULL;
    frame->handle_owned = 0;
    frame->scope_owned = 0;
    frame->queued = 0;
    aura_task_channel_cancel_wait(frame);
    aura_task_executor_report_unjoined_failure(executor, frame);
    if (frame->payload_refs == 0)
    {
      aura_task_frame_destroy(frame);
    }
    frame = next;
  }
#if defined(AURA_TCP_POSIX)
  if (executor->wake_pipe[0] >= 0)
  {
    close(executor->wake_pipe[0]);
  }
  if (executor->wake_pipe[1] >= 0)
  {
    close(executor->wake_pipe[1]);
  }
#endif
  aura_reactor_destroy(executor->reactor);
  free(executor);
}

