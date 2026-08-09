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
  int marked;                     /* C4z: mark bit for STW collect */
  void (*dtor)(void *ptr);        /* C7b: free non-GC field buffers before free */
  void (*mark_extras)(void *ptr); /* C7b: mark Array-of-class field elems */
  int precise_trace;              /* typed callback covers all GC fields */
  struct AuraGcNode *next;
} AuraGcNode;

static AuraGcNode *aura_gc_list = NULL;

#if defined(__unix__) || defined(__APPLE__)
static pthread_once_t aura_gc_lock_once = PTHREAD_ONCE_INIT;
static pthread_mutex_t aura_gc_lock;

static void aura_gc_lock_init(void)
{
  pthread_mutexattr_t attributes;
  pthread_mutexattr_init(&attributes);
  pthread_mutexattr_settype(&attributes, PTHREAD_MUTEX_RECURSIVE);
  pthread_mutex_init(&aura_gc_lock, &attributes);
  pthread_mutexattr_destroy(&attributes);
}

static void aura_gc_lock_enter(void)
{
  pthread_once(&aura_gc_lock_once, aura_gc_lock_init);
  pthread_mutex_lock(&aura_gc_lock);
}

static void aura_gc_lock_leave(void)
{
  pthread_mutex_unlock(&aura_gc_lock);
}
#else
static void aura_gc_lock_enter(void) {}
static void aura_gc_lock_leave(void) {}
#endif

/* Conservative root slots: pointers to variables that hold GC pointers. */
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
  if (n == NULL || n->marked)
  {
    return;
  }
  n->marked = 1;
  if (aura_gc_mark_sp >= AURA_GC_MARK_STACK)
  {
    fputs("aura: GC mark stack overflow\n", stderr);
    abort();
  }
  aura_gc_mark_stack[aura_gc_mark_sp++] = n;
}

/* C6a: mark object and enqueue; scan body for nested GC pointers. */
static void aura_gc_mark_scan(AuraGcNode *n)
{
  if (n == NULL || n->ptr == NULL || n->size < sizeof(void *))
  {
    return;
  }
  /* Align scan to pointer-sized slots within the allocation. */
  uintptr_t base = (uintptr_t)n->ptr;
  size_t nslots = n->size / sizeof(void *);
  for (size_t i = 0; i < nslots; i++)
  {
    void *candidate = *(void **)(base + i * sizeof(void *));
    if (candidate == NULL)
    {
      continue;
    }
    AuraGcNode *child = aura_gc_find(candidate);
    if (child != NULL)
    {
      aura_gc_mark_push(child);
    }
  }
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
  n->marked = 0;
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
  /* Legacy callers retain conservative body scanning semantics. */
  return aura_gc_alloc_internal(size, dtor, mark_extras, 0);
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

void aura_gc_write_barrier(void *owner, void *value)
{
  aura_gc_lock_enter();
  AuraGcNode *owner_node = aura_gc_find(owner);
  if (owner_node != NULL && owner_node->marked)
  {
    AuraGcNode *value_node = aura_gc_find(value);
    aura_gc_mark_push(value_node);
  }
  aura_gc_lock_leave();
}

/* Frames are malloc-owned, so their opaque data is not visible to the
 * collector unless the frame supplies an explicit mark contract. */
static void aura_gc_mark_task_frames(void);

/* C4z/C5f/C6a/C6e: stop-the-world deep mark + sweep when roots are registered. */
void aura_gc_collect(void)
{
  aura_gc_lock_enter();
  for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
  {
    n->marked = 0;
  }
  if (aura_gc_root_n == 0 && aura_gc_array_root_n == 0)
  {
    /* No roots: keep everything (compiler may not have registered yet). */
    for (AuraGcNode *n = aura_gc_list; n != NULL; n = n->next)
    {
      n->marked = 1;
    }
    aura_gc_lock_leave();
    return;
  }
  aura_gc_mark_sp = 0;
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    void **slot = aura_gc_roots[i];
    if (slot == NULL)
    {
      continue;
    }
    void *obj = *slot;
    if (obj == NULL)
    {
      continue;
    }
    AuraGcNode *n = aura_gc_find(obj);
    if (n != NULL)
    {
      aura_gc_mark_push(n);
    }
  }
  /* C6e: mark GC objects stored in Array-of-class buffers. */
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    void **data_slot = aura_gc_array_roots[i].data_slot;
    int64_t *len_slot = aura_gc_array_roots[i].len_slot;
    if (data_slot == NULL || len_slot == NULL)
    {
      continue;
    }
    int64_t len = *len_slot;
    void *data = *data_slot;
    if (data == NULL || len <= 0)
    {
      continue;
    }
    if (aura_gc_array_roots[i].mark != NULL)
    {
      aura_gc_array_roots[i].mark(data, len);
      continue;
    }
    void **elems = (void **)data;
    for (int64_t j = 0; j < len; j++)
    {
      void *obj = elems[j];
      if (obj == NULL)
      {
        continue;
      }
      AuraGcNode *n = aura_gc_find(obj);
      if (n != NULL)
      {
        aura_gc_mark_push(n);
      }
    }
  }
  aura_gc_mark_task_frames();
  /* C6a/C7b: deep mark + per-type mark_extras (Array-of-class fields). */
  while (aura_gc_mark_sp > 0)
  {
    AuraGcNode *n = aura_gc_mark_stack[--aura_gc_mark_sp];
    if (n->mark_extras != NULL && n->ptr != NULL)
    {
      n->mark_extras(n->ptr);
    }
    if (!n->precise_trace)
    {
      aura_gc_mark_scan(n);
    }
  }
  /* C5f/C7b: sweep unmarked objects; run dtor to free owned Array buffers. */
  AuraGcNode **link = &aura_gc_list;
  while (*link != NULL)
  {
    AuraGcNode *n = *link;
    if (!n->marked)
    {
      *link = n->next;
      if (n->dtor != NULL && n->ptr != NULL)
      {
        n->dtor(n->ptr);
      }
      free(n->ptr);
      free(n);
    }
    else
    {
      link = &n->next;
    }
  }
  aura_gc_lock_leave();
}

void aura_gc_shutdown(void)
{
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
  aura_gc_root_n = 0;
  aura_gc_array_root_n = 0;
  aura_ex_dispose_cleared_causes();
  aura_gc_lock_leave();
}

/* ---- F3 bounded foreign String/Array ABI ---- */
