/* ---- GC (C3x free-all + C4z roots + C5f mark/sweep + C6a deep mark + C6e/C7b) ----
 * aura_gc_collect: if roots registered → mark from roots and Array-of-class
 * buffers (C6e), then deep-scan object bodies for nested GC pointers
 * (conservative pointer slots) + per-object mark_extras (C7b Array fields)
 * + sweep unmarked (C7b: dtor frees owned Array buffers). If no roots →
 * mark-all (safe until compiler emits roots). Shutdown still free-all remaining.
 */

typedef struct AuraGcNode
{
  void *ptr;
  size_t size;                    /* C6a: payload size for deep field scan */
  unsigned char color;            /* 0 white, 1 gray, 2 black */
  void (*dtor)(void *ptr);        /* C7b: free non-GC field buffers before free */
  void (*mark_extras)(void *ptr); /* C7b: mark Array-of-class field elems */
  int precise_trace;              /* typed callback covers all GC fields */
  struct AuraGcNode *next;
} AuraGcNode;

static AuraGcNode *aura_gc_list = NULL;

static AuraPlatformMutex aura_gc_lock;
static atomic_int aura_gc_lock_ready = 0;

static void aura_gc_lock_enter(void)
{
  int expected = 0;
  if (atomic_compare_exchange_strong(&aura_gc_lock_ready, &expected, 1))
  {
    (void)aura_platform_mutex_init(&aura_gc_lock, 1);
    atomic_store(&aura_gc_lock_ready, 2);
  }
  else
  {
    while (atomic_load(&aura_gc_lock_ready) != 2) {}
  }
  aura_platform_mutex_lock(&aura_gc_lock);
}

static void aura_gc_lock_leave(void)
{
  aura_platform_mutex_unlock(&aura_gc_lock);
}

/* Root slots are explicit compiler/runtime metadata, never inferred from heap bytes. */
#define AURA_GC_MAX_ROOTS 256
static void **aura_gc_roots[AURA_GC_MAX_ROOTS];
static int aura_gc_root_n = 0;

/* C6e: Array-of-class locals — scan .data[0..len) as GC pointer slots.
 * data_slot points at the Array.data field; len_slot at Array.len. */
typedef struct
{
  void **data_slot;
  int64_t *len_slot;
  void (*mark)(const void *data, int64_t len);
} AuraGcArrayRoot;

#define AURA_GC_MAX_ARRAY_ROOTS 256
static AuraGcArrayRoot aura_gc_array_roots[AURA_GC_MAX_ARRAY_ROOTS];
static int aura_gc_array_root_n = 0;

/* Worklist for deep mark (C6a). */
#define AURA_GC_MARK_STACK 1024
static AuraGcNode *aura_gc_mark_stack[AURA_GC_MARK_STACK];
static int aura_gc_mark_sp = 0;

typedef enum
{
  AURA_GC_IDLE = 0,
  AURA_GC_MARKING = 1,
  AURA_GC_SWEEPING = 2
} AuraGcPhase;

static AuraGcPhase aura_gc_phase = AURA_GC_IDLE;
static AuraGcNode **aura_gc_sweep_link = NULL;
static AuraGcPauseFn aura_gc_pause_before_sweep = NULL;
static AuraGcResumeFn aura_gc_resume_after_sweep = NULL;
static void *aura_gc_safepoint_context = NULL;
static int aura_gc_sweep_paused = 0;
static int aura_gc_sweep_pause_requested = 0;

static AuraPlatformMutex aura_gc_worker_lock;
static AuraPlatformCond aura_gc_worker_cond;
static AuraPlatformThread aura_gc_worker;
static atomic_int aura_gc_worker_sync_ready = 0;
static int aura_gc_worker_started = 0;
static int aura_gc_worker_requested = 0;
static int aura_gc_worker_running = 0;
static int aura_gc_worker_stop = 0;

static void aura_gc_worker_sync_init(void)
{
  int expected = 0;
  if (atomic_compare_exchange_strong(&aura_gc_worker_sync_ready, &expected, 1))
  {
    (void)aura_platform_mutex_init(&aura_gc_worker_lock, 0);
    (void)aura_platform_cond_init(&aura_gc_worker_cond);
    atomic_store(&aura_gc_worker_sync_ready, 2);
  }
  else
  {
    while (atomic_load(&aura_gc_worker_sync_ready) != 2) {}
  }
}

void aura_gc_add_root(void **slot)
{
  aura_gc_lock_enter();
  if (slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    if (aura_gc_roots[i] == slot)
    {
      aura_gc_lock_leave();
      return;
    }
  }
  if (aura_gc_root_n >= AURA_GC_MAX_ROOTS)
  {
    fputs("aura: GC root table full\n", stderr);
    abort();
  }
  aura_gc_roots[aura_gc_root_n++] = slot;
  aura_gc_lock_leave();
}

void aura_gc_remove_root(void **slot)
{
  aura_gc_lock_enter();
  if (slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    if (aura_gc_roots[i] == slot)
    {
      aura_gc_roots[i] = aura_gc_roots[aura_gc_root_n - 1];
      aura_gc_root_n--;
      aura_gc_lock_leave();
      return;
    }
  }
  aura_gc_lock_leave();
}

/* C6e: register Array.data / Array.len so collect marks element GC pointers. */
void aura_gc_add_array_root_typed(void **data_slot, int64_t *len_slot,
                                  void (*mark)(const void *, int64_t));

void aura_gc_add_array_root(void **data_slot, int64_t *len_slot)
{
  aura_gc_add_array_root_typed(data_slot, len_slot, NULL);
}

void aura_gc_add_array_root_typed(void **data_slot, int64_t *len_slot,
                                  void (*mark)(const void *, int64_t))
{
  aura_gc_lock_enter();
  if (data_slot == NULL || len_slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    if (aura_gc_array_roots[i].data_slot == data_slot)
    {
      aura_gc_array_roots[i].len_slot = len_slot;
      aura_gc_array_roots[i].mark = mark;
      aura_gc_lock_leave();
      return;
    }
  }
  if (aura_gc_array_root_n >= AURA_GC_MAX_ARRAY_ROOTS)
  {
    fputs("aura: GC array root table full\n", stderr);
    abort();
  }
  aura_gc_array_roots[aura_gc_array_root_n].data_slot = data_slot;
  aura_gc_array_roots[aura_gc_array_root_n].len_slot = len_slot;
  aura_gc_array_roots[aura_gc_array_root_n].mark = mark;
  aura_gc_array_root_n++;
  aura_gc_lock_leave();
}

void aura_gc_remove_array_root(void **data_slot)
{
  aura_gc_lock_enter();
  if (data_slot == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    if (aura_gc_array_roots[i].data_slot == data_slot)
    {
      aura_gc_array_roots[i] = aura_gc_array_roots[aura_gc_array_root_n - 1];
      aura_gc_array_root_n--;
      aura_gc_lock_leave();
      return;
    }
  }
  aura_gc_lock_leave();
}

static AuraGcNode *aura_gc_find(void *ptr)
{
  for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
  {
    if (n->ptr == ptr)
    {
      return n;
    }
  }
  return NULL;
}

static void aura_gc_mark_push(AuraGcNode *n)
{
  if (n == NULL || n->color != 0)
  {
    return;
  }
  n->color = 1;
  if (aura_gc_mark_sp >= AURA_GC_MARK_STACK)
  {
    fputs("aura: GC mark stack overflow\n", stderr);
    abort();
  }
  aura_gc_mark_stack[aura_gc_mark_sp++] = n;
}

static void *aura_gc_alloc_internal(size_t size, void (*dtor)(void *),
                                    void (*mark_extras)(void *),
                                    int precise_trace)
{
  aura_gc_lock_enter();
  void *p = malloc(size);
  if (p == NULL && size > 0)
  {
    fputs("aura: GC allocation failed\n", stderr);
    abort();
  }
  if (p != NULL && size > 0)
  {
    memset(p, 0, size);
  }
  AuraGcNode *n = (AuraGcNode *)malloc(sizeof(AuraGcNode));
  if (n == NULL)
  {
    fputs("aura: GC metadata allocation failed\n", stderr);
    abort();
  }
  n->ptr = p;
  n->size = size;
  /* Objects allocated during concurrent marking start black. Their future
   * managed stores are covered by the write barrier, so they cannot be swept
   * before the current cycle ends. */
  n->color = aura_gc_phase != AURA_GC_IDLE ? 2 : 0;
  n->dtor = dtor;
  n->mark_extras = mark_extras;
  n->precise_trace = precise_trace;
  n->next = aura_gc_list;
  aura_gc_list = n;
  aura_gc_lock_leave();
  return p;
}

void *aura_gc_alloc_full(size_t size, void (*dtor)(void *), void (*mark_extras)(void *))
{
  /* The historical name remains ABI-compatible, but its callback is now the
   * complete typed trace contract; NULL means the object has no GC fields. */
  return aura_gc_alloc_internal(size, dtor, mark_extras, 1);
}

void *aura_gc_alloc_typed(size_t size, void (*dtor)(void *),
                          void (*trace)(void *))
{
  if (trace == NULL)
  {
    fputs("aura: typed GC allocation requires a trace callback\n", stderr);
    abort();
  }
  return aura_gc_alloc_internal(size, dtor, trace, 1);
}

void *aura_gc_alloc(size_t size)
{
  return aura_gc_alloc_full(size, NULL, NULL);
}

/* Release one runtime-owned GC allocation immediately.  Task frame locals
 * are rooted while the frame exists, but their storage is still owned by the
 * frame and must not be left on the GC list until an unrelated collection. */
static void aura_gc_release(void *ptr)
{
  if (ptr == NULL)
  {
    return;
  }
  aura_gc_lock_enter();
  AuraGcNode **link = &aura_gc_list;
  while (*link != NULL)
  {
    AuraGcNode *n = *link;
    if (n->ptr == ptr)
    {
      *link = n->next;
      if (n->dtor != NULL)
      {
        n->dtor(n->ptr);
      }
      free(n->ptr);
      free(n);
      aura_gc_lock_leave();
      return;
    }
    link = &n->next;
  }
  aura_gc_lock_leave();
}

/* C7b: mark a GC object pointer (for generated mark_extras on Array fields). */
void aura_gc_mark_ptr(void *obj)
{
  aura_gc_lock_enter();
  if (obj == NULL)
  {
    aura_gc_lock_leave();
    return;
  }
  AuraGcNode *n = aura_gc_find(obj);
  if (n != NULL)
  {
    aura_gc_mark_push(n);
  }
  aura_gc_lock_leave();
}

static void aura_gc_write_barrier_locked(void *owner, void *value)
{
  AuraGcNode *owner_node = aura_gc_find(owner);
  if (aura_gc_phase == AURA_GC_MARKING && owner_node != NULL &&
      owner_node->color == 2)
  {
    AuraGcNode *value_node = aura_gc_find(value);
    aura_gc_mark_push(value_node);
  }
}

void aura_gc_write_barrier(void *owner, void *value)
{
  aura_gc_lock_enter();
  aura_gc_write_barrier_locked(owner, value);
  aura_gc_lock_leave();
}

void *aura_gc_store_ptr(void **slot, void *owner, void *value)
{
  aura_gc_lock_enter();
  if (slot != NULL)
  {
    *slot = value;
    aura_gc_write_barrier_locked(owner, value);
  }
  aura_gc_lock_leave();
  return value;
}

/* Frames are malloc-owned, so their opaque data is not visible to the
 * collector unless the frame supplies an explicit mark contract. */
static void aura_gc_mark_task_frames(void);

/* Seed a tri-color cycle from all registered roots. */
static void aura_gc_begin_locked(void)
{
  for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
  {
    n->color = 0;
  }
  if (aura_gc_root_n == 0 && aura_gc_array_root_n == 0)
  {
    for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
    {
      n->color = 2;
    }
    aura_gc_phase = AURA_GC_IDLE;
    return;
  }
  aura_gc_mark_sp = 0;
  aura_gc_phase = AURA_GC_MARKING;
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    void **slot = aura_gc_roots[i];
    if (slot != NULL)
    {
      AuraGcNode *n = aura_gc_find(*slot);
      if (n != NULL) aura_gc_mark_push(n);
    }
  }
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    void **data_slot = aura_gc_array_roots[i].data_slot;
    int64_t *len_slot = aura_gc_array_roots[i].len_slot;
    if (data_slot == NULL || len_slot == NULL || *data_slot == NULL || *len_slot <= 0)
    {
      continue;
    }
    void *data = *data_slot;
    int64_t len = *len_slot;
    if (aura_gc_array_roots[i].mark != NULL)
    {
      aura_gc_array_roots[i].mark(data, len);
    }
    else
    {
      void **elems = (void **)data;
      for (int64_t j = 0; j < len; j++) aura_gc_mark_push(aura_gc_find(elems[j]));
    }
  }
  aura_gc_mark_task_frames();
}

static void aura_gc_process_one_locked(void)
{
  AuraGcNode *n = aura_gc_mark_stack[--aura_gc_mark_sp];
  if (n->mark_extras != NULL && n->ptr != NULL) n->mark_extras(n->ptr);
  n->color = 2;
}

static void *aura_gc_worker_main(void *unused)
{
  (void)unused;
  aura_gc_worker_sync_init();
  for (;;)
  {
    aura_platform_mutex_lock(&aura_gc_worker_lock);
    while (!aura_gc_worker_requested && !aura_gc_worker_stop)
    {
      aura_platform_cond_wait(&aura_gc_worker_cond, &aura_gc_worker_lock);
    }
    if (aura_gc_worker_stop)
    {
      aura_platform_mutex_unlock(&aura_gc_worker_lock);
      return NULL;
    }
    aura_gc_worker_requested = 0;
    aura_platform_mutex_unlock(&aura_gc_worker_lock);

    while (aura_gc_step(64) != 0) {}

    aura_platform_mutex_lock(&aura_gc_worker_lock);
    aura_gc_worker_running = 0;
    aura_platform_cond_broadcast(&aura_gc_worker_cond);
    aura_platform_mutex_unlock(&aura_gc_worker_lock);
  }
}

/* Advance marking and sweeping in bounded units so schedulers can keep pauses
 * below their configured budget. */
int aura_gc_step(size_t budget)
{
  AuraGcPauseFn pause = NULL;
  AuraGcResumeFn resume = NULL;
  void *safepoint_context = NULL;
  int request_pause = 0;
  int release_pause = 0;
  aura_gc_lock_enter();
  if (aura_gc_phase == AURA_GC_IDLE)
  {
    aura_gc_begin_locked();
    if (aura_gc_phase == AURA_GC_IDLE)
    {
      aura_gc_lock_leave();
      return 0;
    }
  }
  while (budget > 0 && aura_gc_phase == AURA_GC_MARKING && aura_gc_mark_sp > 0)
  {
    aura_gc_process_one_locked();
    budget--;
  }
  if (aura_gc_phase == AURA_GC_MARKING && aura_gc_mark_sp == 0)
  {
    if (aura_gc_pause_before_sweep != NULL && !aura_gc_sweep_pause_requested)
    {
      aura_gc_sweep_pause_requested = 1;
      pause = aura_gc_pause_before_sweep;
      safepoint_context = aura_gc_safepoint_context;
      request_pause = 1;
    }
    else if (!aura_gc_sweep_pause_requested)
    {
      aura_gc_phase = AURA_GC_SWEEPING;
      aura_gc_sweep_link = &aura_gc_list;
    }
  }
  if (request_pause)
  {
    aura_gc_lock_leave();
    (void)pause(safepoint_context);
    aura_gc_lock_enter();
    aura_gc_phase = AURA_GC_SWEEPING;
    aura_gc_sweep_link = &aura_gc_list;
    aura_gc_sweep_paused = 1;
  }
  while (budget > 0 && aura_gc_phase == AURA_GC_SWEEPING &&
         aura_gc_sweep_link != NULL && *aura_gc_sweep_link != NULL)
  {
    AuraGcNode *n = *aura_gc_sweep_link;
    if (n->color == 0)
    {
      *aura_gc_sweep_link = n->next;
      if (n->dtor != NULL && n->ptr != NULL) n->dtor(n->ptr);
      free(n->ptr);
      free(n);
    }
    else
    {
      aura_gc_sweep_link = &n->next;
    }
    budget--;
  }
  if (aura_gc_phase == AURA_GC_SWEEPING &&
      (aura_gc_sweep_link == NULL || *aura_gc_sweep_link == NULL))
  {
    aura_gc_phase = AURA_GC_IDLE;
    aura_gc_sweep_link = NULL;
    if (aura_gc_sweep_paused && aura_gc_resume_after_sweep != NULL)
    {
      resume = aura_gc_resume_after_sweep;
      safepoint_context = aura_gc_safepoint_context;
      release_pause = 1;
    }
    aura_gc_sweep_paused = 0;
    aura_gc_sweep_pause_requested = 0;
    aura_gc_pause_before_sweep = NULL;
    aura_gc_resume_after_sweep = NULL;
    aura_gc_safepoint_context = NULL;
  }
  int active = aura_gc_phase != AURA_GC_IDLE;
  aura_gc_lock_leave();
  if (release_pause)
  {
    resume(safepoint_context);
  }
  return active;
}

void aura_gc_collect(void)
{
  while (aura_gc_step(SIZE_MAX) != 0) {}
}

int aura_gc_start_concurrent(void *context, AuraGcPauseFn pause,
                             AuraGcResumeFn resume)
{
  if (pause == NULL || resume == NULL)
  {
    return 0;
  }
  aura_gc_worker_sync_init();
  aura_platform_mutex_lock(&aura_gc_worker_lock);
  if (aura_gc_worker_running || aura_gc_worker_requested)
  {
    aura_platform_mutex_unlock(&aura_gc_worker_lock);
    return 1;
  }
  if (!aura_gc_worker_started)
  {
    aura_gc_worker_stop = 0;
    if (aura_platform_thread_create(&aura_gc_worker, aura_gc_worker_main, NULL) != 0)
    {
      aura_platform_mutex_unlock(&aura_gc_worker_lock);
      return 0;
    }
    aura_gc_worker_started = 1;
  }
  aura_platform_mutex_unlock(&aura_gc_worker_lock);

  aura_gc_lock_enter();
  if (aura_gc_phase != AURA_GC_IDLE)
  {
    aura_gc_lock_leave();
    return 1;
  }
  aura_gc_pause_before_sweep = pause;
  aura_gc_resume_after_sweep = resume;
  aura_gc_safepoint_context = context;
  aura_gc_lock_leave();
  if (aura_gc_step(0) == 0)
  {
    aura_gc_lock_enter();
    aura_gc_pause_before_sweep = NULL;
    aura_gc_resume_after_sweep = NULL;
    aura_gc_safepoint_context = NULL;
    aura_gc_lock_leave();
    return 0;
  }
  aura_platform_mutex_lock(&aura_gc_worker_lock);
  aura_gc_worker_running = 1;
  aura_gc_worker_requested = 1;
  aura_platform_cond_signal(&aura_gc_worker_cond);
  aura_platform_mutex_unlock(&aura_gc_worker_lock);
  return 1;
}

void aura_gc_wait_background(void)
{
  aura_gc_worker_sync_init();
  aura_platform_mutex_lock(&aura_gc_worker_lock);
  while (aura_gc_worker_running || aura_gc_worker_requested)
  {
    aura_platform_cond_wait(&aura_gc_worker_cond, &aura_gc_worker_lock);
  }
  aura_platform_mutex_unlock(&aura_gc_worker_lock);
}

void aura_gc_shutdown(void)
{
  aura_gc_wait_background();
  aura_platform_mutex_lock(&aura_gc_worker_lock);
  if (aura_gc_worker_started)
  {
    aura_gc_worker_stop = 1;
    aura_platform_cond_signal(&aura_gc_worker_cond);
    aura_platform_mutex_unlock(&aura_gc_worker_lock);
    aura_platform_thread_join(&aura_gc_worker);
    aura_platform_mutex_lock(&aura_gc_worker_lock);
    aura_gc_worker_started = 0;
    aura_gc_worker_stop = 0;
    aura_platform_mutex_unlock(&aura_gc_worker_lock);
  }
  else
  {
    aura_platform_mutex_unlock(&aura_gc_worker_lock);
  }
  aura_gc_lock_enter();
  AuraGcNode *n = aura_gc_list;
  while (n != NULL)
  {
    AuraGcNode *next = n->next;
    if (n->dtor != NULL && n->ptr != NULL)
    {
      n->dtor(n->ptr);
    }
    free(n->ptr);
    free(n);
    n = next;
  }
  aura_gc_list = NULL;
  aura_gc_phase = AURA_GC_IDLE;
  aura_gc_sweep_link = NULL;
  aura_gc_root_n = 0;
  aura_gc_array_root_n = 0;
  aura_ex_dispose_cleared_causes();
  aura_gc_lock_leave();
}

/* ---- F3 bounded foreign String/Array ABI ---- */
