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

void *aura_gc_alloc_full(size_t size, void (*dtor)(void *), void (*mark_extras)(void *))
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
  n->next = aura_gc_list;
  aura_gc_list = n;
  aura_gc_lock_leave();
  return p;
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

AuraFfiStatus aura_type_erased_clone(const AuraTypeErasedValue *source,
                                     AuraTypeErasedValue *out)
{
  size_t cloned_size = 0;
  void *copy;
  if (source == NULL || out == NULL || source->ops == NULL ||
      source->ops->abi_version != AURA_TYPE_ERASED_ABI_VERSION ||
      source->ops->clone == NULL)
  {
    return AURA_FFI_INVALID;
  }
  copy = source->ops->clone(source->data, source->size, &cloned_size);
  if (copy == NULL && cloned_size != 0)
  {
    return AURA_FFI_OOM;
  }
  out->data = copy;
  out->size = cloned_size;
  out->ops = source->ops;
  return AURA_FFI_OK;
}

void aura_type_erased_drop(AuraTypeErasedValue *value)
{
  if (value == NULL)
  {
    return;
  }
  if (value->data != NULL && value->ops != NULL &&
      value->ops->abi_version == AURA_TYPE_ERASED_ABI_VERSION &&
      value->ops->drop != NULL)
  {
    value->ops->drop(value->data, value->size);
  }
  value->data = NULL;
  value->size = 0;
  value->ops = NULL;
}

void aura_type_erased_mark(const AuraTypeErasedValue *value)
{
  if (value != NULL && value->data != NULL && value->ops != NULL &&
      value->ops->abi_version == AURA_TYPE_ERASED_ABI_VERSION &&
      value->ops->mark != NULL)
  {
    value->ops->mark(value->data, value->size);
  }
}

/* Frames are malloc-owned, so their opaque data is not visible to the
 * collector unless the frame supplies an explicit mark contract. */
static void aura_gc_mark_task_frames(void);

/* C12k/C12l/C13e: Fun capture env header (must match codegen layout).
 * Layout of every capturing env:
 *   void (*__drop)(void *);
 *   int32_t __refs;
 *   … capture slots (class GC roots, boxes, nested Fun fat pointers, …)
 * Array capture slots are owned snapshots for immutable captures; mutable
 * captures use retained shared cells. Drop releases the matching ownership
 * contract emitted by the compiler.
 * C12m: by-ref Int/Bool captures release their shared boxes in drop.
 * C13e: Fun slots retain nested env; drop releases nested env once via RC. */
typedef struct
{
  void (*drop)(void *);
  int32_t refs;
} aura_fun_env_hdr;

void aura_fun_env_retain(void *env)
{
  if (env == NULL)
  {
    return;
  }
  aura_fun_env_hdr *h = (aura_fun_env_hdr *)env;
  h->refs++;
}

/* Release one ownership share; on zero refs run __drop then free. */
void aura_fun_env_free(void *env)
{
  if (env == NULL)
  {
    return;
  }
  aura_fun_env_hdr *h = (aura_fun_env_hdr *)env;
  if (h->refs > 1)
  {
    h->refs--;
    return;
  }
  h->refs = 0;
  if (h->drop != NULL)
  {
    h->drop(env);
  }
  else
  {
    free(env);
  }
}

/* C20b: generic shared pointer box for future mutable class/Array/Fun
 * captures.  The box owns only the callback contract supplied by its caller;
 * it does not infer whether value is GC-managed, an Array header, or a Fun
 * environment.  This keeps the ABI additive and lets codegen select the
 * appropriate drop policy when those capture forms are enabled. */
typedef void (*aura_box_ptr_drop_fn)(void *value);

typedef struct aura_box_ptr
{
  void *value;
  int32_t refs;
  aura_box_ptr_drop_fn drop;
} aura_box_ptr;

aura_box_ptr *aura_box_ptr_new(void *value, aura_box_ptr_drop_fn drop)
{
  aura_box_ptr *b = (aura_box_ptr *)malloc(sizeof(aura_box_ptr));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box ptr)\n");
    exit(1);
  }
  b->value = value;
  b->refs = 1;
  b->drop = drop;
  return b;
}

void aura_box_ptr_retain(aura_box_ptr *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_ptr_release(aura_box_ptr *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    if (b->drop != NULL && b->value != NULL)
    {
      b->drop(b->value);
    }
    free(b);
  }
}

void *aura_box_ptr_get(const aura_box_ptr *b)
{
  return b == NULL ? NULL : b->value;
}

void *aura_box_ptr_set(aura_box_ptr *b, void *value,
                      aura_box_ptr_drop_fn drop)
{
  if (b == NULL)
  {
    return NULL;
  }
  if (b->value == value && b->drop == drop)
  {
    return b->value;
  }
  if (b->drop != NULL && b->value != NULL)
  {
    b->drop(b->value);
  }
  b->value = value;
  b->drop = drop;
  return b->value;
}

/* C12m: shared mutable boxes for `var` Int/Bool lambda captures (refcounted). */
typedef struct aura_box_i64
{
  int64_t value;
  int32_t refs;
} aura_box_i64;

typedef struct aura_box_bool
{
  bool value;
  int32_t refs;
} aura_box_bool;

aura_box_i64 *aura_box_i64_new(int64_t v)
{
  aura_box_i64 *b = (aura_box_i64 *)malloc(sizeof(aura_box_i64));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box i64)\n");
    exit(1);
  }
  b->value = v;
  b->refs = 1;
  return b;
}

void aura_box_i64_retain(aura_box_i64 *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_i64_release(aura_box_i64 *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    free(b);
  }
}

aura_box_bool *aura_box_bool_new(bool v)
{
  aura_box_bool *b = (aura_box_bool *)malloc(sizeof(aura_box_bool));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box bool)\n");
    exit(1);
  }
  b->value = v;
  b->refs = 1;
  return b;
}

void aura_box_bool_retain(aura_box_bool *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_bool_release(aura_box_bool *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    free(b);
  }
}

/* C13f: shared mutable box for `var` String lambda captures (refcounted).
 * The box always owns a heap copy of the string so release can free safely
 * (literals and temporary concat results both work). */
typedef struct aura_box_str
{
  const char *value;
  int32_t refs;
} aura_box_str;

static char *aura_box_str_dup(const char *v)
{
  if (v == NULL)
  {
    return NULL;
  }
  size_t n = strlen(v);
  char *p = (char *)malloc(n + 1);
  if (p == NULL)
  {
    fprintf(stderr, "aura: out of memory (box str copy)\n");
    exit(1);
  }
  if (n > 0)
  {
    memcpy(p, v, n);
  }
  p[n] = '\0';
  return p;
}

aura_box_str *aura_box_str_new(const char *v)
{
  aura_box_str *b = (aura_box_str *)malloc(sizeof(aura_box_str));
  if (b == NULL)
  {
    fprintf(stderr, "aura: out of memory (box str)\n");
    exit(1);
  }
  b->value = aura_box_str_dup(v);
  b->refs = 1;
  return b;
}

void aura_box_str_retain(aura_box_str *b)
{
  if (b != NULL)
  {
    b->refs++;
  }
}

void aura_box_str_release(aura_box_str *b)
{
  if (b == NULL)
  {
    return;
  }
  b->refs--;
  if (b->refs <= 0)
  {
    free((void *)b->value);
    free(b);
  }
}

/* Replace boxed string; frees previous owned value. Safe for self-assign
 * (copy first). Used by codegen for `var` String by-ref capture writes.
 * Returns the new owned pointer (or NULL). */
const char *aura_box_str_set(aura_box_str *b, const char *v)
{
  if (b == NULL)
  {
    return NULL;
  }
  const char *copy = aura_box_str_dup(v);
  free((void *)b->value);
  b->value = copy;
  return b->value;
}

/* Snapshot boxed string for escape (return/bind/eq/concat). Caller owns the
 * buffer so later box mutations do not invalidate it. */
const char *aura_box_str_get(aura_box_str *b)
{
  if (b == NULL)
  {
    return NULL;
  }
  return aura_box_str_dup(b->value);
}

/* C14: compiler-backed Hashable implementation for String.
 * Keep the same deterministic 31-based hash used by std.collections. */
int64_t aura_hash_string(const char *s)
{
  int64_t h = 0;
  if (s == NULL)
  {
    return 0;
  }
  for (const unsigned char *p = (const unsigned char *)s; *p != '\0'; ++p)
  {
    h = h * 31 + (int64_t)*p;
  }
  return h < 0 ? -h : h;
}

/* C13c: Int.toString() — decimal (base 10), no locale.
 * Returns a freshly malloc'd NUL-terminated C string. Caller owns the buffer
 * (same ownership as other owned strings: substring/trim/split segments, concat).
 * Handles 0, negatives, and INT64_MIN. */
const char *aura_i64_to_string(int64_t v)
{
  /* "-9223372036854775808" + NUL = 21; pad for safety. */
  char buf[32];
  size_t i = 0;
  uint64_t u;
  if (v < 0)
  {
    /* Negate via unsigned to keep INT64_MIN well-defined. */
    u = (uint64_t)(-(v + 1)) + 1;
  }
  else
  {
    u = (uint64_t)v;
  }
  if (u == 0)
  {
    buf[i++] = '0';
  }
  else
  {
    char tmp[32];
    size_t n = 0;
    while (u > 0)
    {
      tmp[n++] = (char)('0' + (u % 10));
      u /= 10;
    }
    while (n > 0)
    {
      buf[i++] = tmp[--n];
    }
  }
  size_t dig_len = i;
  size_t total = dig_len + (v < 0 ? 1 : 0);
  char *out = (char *)malloc(total + 1);
  if (out == NULL)
  {
    fprintf(stderr, "aura: out of memory (i64_to_string)\n");
    exit(1);
  }
  size_t o = 0;
  if (v < 0)
  {
    out[o++] = '-';
  }
  memcpy(out + o, buf, dig_len);
  out[o + dig_len] = '\0';
  return (const char *)out;
}

static int aura_encoding_hex_value(unsigned char c)
{
  if (c >= '0' && c <= '9') return (int)(c - '0');
  if (c >= 'a' && c <= 'f') return (int)(c - 'a') + 10;
  if (c >= 'A' && c <= 'F') return (int)(c - 'A') + 10;
  return -1;
}

const char *aura_encoding_hex_encode(const char *value)
{
  static const char digits[] = "0123456789abcdef";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 1) / 2) return NULL;
  char *out = (char *)malloc(length * 2 + 1);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i++) {
    unsigned char byte = (unsigned char)value[i];
    out[i * 2] = digits[byte >> 4];
    out[i * 2 + 1] = digits[byte & 0x0f];
  }
  out[length * 2] = '\0';
  return out;
}

const char *aura_encoding_hex_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if ((length & 1u) != 0) return NULL;
  char *out = (char *)malloc(length / 2 + 1);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i += 2) {
    int hi = aura_encoding_hex_value((unsigned char)value[i]);
    int lo = aura_encoding_hex_value((unsigned char)value[i + 1]);
    if (hi < 0 || lo < 0 || (hi == 0 && lo == 0)) { free(out); return NULL; }
    out[i / 2] = (char)((hi << 4) | lo);
  }
  out[length / 2] = '\0';
  return out;
}

static int aura_encoding_base64_value(unsigned char c)
{
  if (c >= 'A' && c <= 'Z') return (int)(c - 'A');
  if (c >= 'a' && c <= 'z') return (int)(c - 'a') + 26;
  if (c >= '0' && c <= '9') return (int)(c - '0') + 52;
  if (c == '+') return 62;
  if (c == '/') return 63;
  return -1;
}

const char *aura_encoding_base64_encode(const char *value)
{
  static const char digits[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 4) / 4 * 3) return NULL;
  size_t output_length = ((length + 2) / 3) * 4;
  char *out = (char *)malloc(output_length + 1);
  if (out == NULL) return NULL;
  size_t i = 0, o = 0;
  while (i < length) {
    size_t remaining = length - i;
    unsigned int a = (unsigned char)value[i++];
    unsigned int b = remaining > 1 ? (unsigned char)value[i++] : 0;
    unsigned int c = remaining > 2 ? (unsigned char)value[i++] : 0;
    out[o++] = digits[a >> 2];
    out[o++] = digits[((a & 3u) << 4) | (b >> 4)];
    out[o++] = remaining > 1 ? digits[((b & 15u) << 2) | (c >> 6)] : '=';
    out[o++] = remaining > 2 ? digits[c & 63u] : '=';
  }
  out[o] = '\0';
  return out;
}

const char *aura_encoding_base64_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if (length == 0) {
    char *empty = (char *)malloc(1);
    if (empty != NULL) empty[0] = '\0';
    return empty;
  }
  if ((length & 3u) != 0) return NULL;
  size_t output_length = (length / 4) * 3;
  if (value[length - 1] == '=') output_length--;
  if (value[length - 2] == '=') output_length--;
  char *out = (char *)malloc(output_length + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i += 4) {
    int a = aura_encoding_base64_value((unsigned char)value[i]);
    int b = aura_encoding_base64_value((unsigned char)value[i + 1]);
    int c = value[i + 2] == '=' ? 0 : aura_encoding_base64_value((unsigned char)value[i + 2]);
    int d = value[i + 3] == '=' ? 0 : aura_encoding_base64_value((unsigned char)value[i + 3]);
    int last = i + 4 == length;
    if (a < 0 || b < 0 || c < 0 || d < 0 ||
        (!last && (value[i + 2] == '=' || value[i + 3] == '=')) ||
        (value[i + 2] == '=' && value[i + 3] != '=') ||
        (value[i + 2] == '=' && (b & 15) != 0) ||
        (value[i + 3] == '=' && value[i + 2] != '=' && (c & 3) != 0)) {
      free(out); return NULL;
    }
    unsigned int triple = ((unsigned int)a << 18) | ((unsigned int)b << 12) |
                          ((unsigned int)c << 6) | (unsigned int)d;
    if (o < output_length) out[o++] = (char)(triple >> 16);
    if (o < output_length) out[o++] = (char)(triple >> 8);
    if (o < output_length) out[o++] = (char)triple;
  }
  for (size_t i = 0; i < output_length; i++) if (out[i] == '\0') { free(out); return NULL; }
  out[output_length] = '\0';
  return out;
}

static int aura_encoding_percent_unreserved(unsigned char c)
{
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
         (c >= '0' && c <= '9') || c == '-' || c == '.' || c == '_' || c == '~';
}

const char *aura_encoding_percent_encode(const char *value)
{
  static const char digits[] = "0123456789ABCDEF";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 1) / 3) return NULL;
  char *out = (char *)malloc(length * 3 + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)value[i];
    if (aura_encoding_percent_unreserved(c)) out[o++] = (char)c;
    else { out[o++] = '%'; out[o++] = digits[c >> 4]; out[o++] = digits[c & 15]; }
  }
  out[o] = '\0';
  return out;
}

const char *aura_encoding_percent_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)value[i];
    if (c == '%') {
      if (i + 2 >= length) { free(out); return NULL; }
      int hi = aura_encoding_hex_value((unsigned char)value[++i]);
      int lo = aura_encoding_hex_value((unsigned char)value[++i]);
      if (hi < 0 || lo < 0 || (hi == 0 && lo == 0)) { free(out); return NULL; }
      out[o++] = (char)((hi << 4) | lo);
    } else out[o++] = (char)c;
  }
  out[o] = '\0';
  return out;
}

_Bool aura_encoding_is_valid_utf8(const char *value)
{
  const unsigned char *bytes = (const unsigned char *)(value == NULL ? "" : value);
  size_t length = strlen((const char *)bytes);
  size_t i = 0;
  while (i < length) {
    unsigned char lead = bytes[i++];
    uint32_t codepoint;
    size_t continuation;
    if (lead <= 0x7f) continue;
    if (lead >= 0xc2 && lead <= 0xdf) { codepoint = lead & 0x1f; continuation = 1; }
    else if (lead >= 0xe0 && lead <= 0xef) { codepoint = lead & 0x0f; continuation = 2; }
    else if (lead >= 0xf0 && lead <= 0xf4) { codepoint = lead & 0x07; continuation = 3; }
    else return false;
    if (i + continuation > length) return false;
    for (size_t j = 0; j < continuation; j++) {
      unsigned char tail = bytes[i++];
      if ((tail & 0xc0) != 0x80) return false;
      codepoint = (codepoint << 6) | (tail & 0x3f);
    }
    if ((continuation == 2 && codepoint < 0x800) ||
        (continuation == 3 && codepoint < 0x10000) ||
        codepoint > 0x10ffff || (codepoint >= 0xd800 && codepoint <= 0xdfff)) return false;
  }
  return true;
}

typedef struct AuraJsonCursor
{
  const unsigned char *data;
  size_t length;
  size_t index;
  unsigned depth;
} AuraJsonCursor;

static void aura_json_skip_ws(AuraJsonCursor *cursor)
{
  while (cursor->index < cursor->length &&
         (cursor->data[cursor->index] == ' ' || cursor->data[cursor->index] == '\n' ||
          cursor->data[cursor->index] == '\r' || cursor->data[cursor->index] == '\t'))
  {
    cursor->index++;
  }
}

static int aura_json_hex(unsigned char c)
{
  if (c >= '0' && c <= '9') return c - '0';
  if (c >= 'a' && c <= 'f') return c - 'a' + 10;
  if (c >= 'A' && c <= 'F') return c - 'A' + 10;
  return -1;
}

static int aura_json_string(AuraJsonCursor *cursor)
{
  if (cursor->index >= cursor->length || cursor->data[cursor->index++] != '"') return 0;
  while (cursor->index < cursor->length)
  {
    unsigned char c = cursor->data[cursor->index++];
    if (c == '"') return 1;
    if (c < 0x20) return 0;
    if (c != '\\') continue;
    if (cursor->index >= cursor->length) return 0;
    c = cursor->data[cursor->index++];
    if (strchr("\\\"/bfnrt", (int)c) != NULL) continue;
    if (c != 'u' || cursor->index + 4 > cursor->length) return 0;
    for (unsigned i = 0; i < 4; i++)
    {
      if (aura_json_hex(cursor->data[cursor->index++]) < 0) return 0;
    }
  }
  return 0;
}

static int aura_json_value(AuraJsonCursor *cursor);

static int aura_json_array(AuraJsonCursor *cursor)
{
  if (cursor->data[cursor->index++] != '[' || ++cursor->depth > 64) return 0;
  aura_json_skip_ws(cursor);
  if (cursor->index < cursor->length && cursor->data[cursor->index] == ']')
  {
    cursor->index++;
    cursor->depth--;
    return 1;
  }
  for (;;)
  {
    if (!aura_json_value(cursor)) return 0;
    aura_json_skip_ws(cursor);
    if (cursor->index >= cursor->length) return 0;
    if (cursor->data[cursor->index] == ']') { cursor->index++; cursor->depth--; return 1; }
    if (cursor->data[cursor->index++] != ',') return 0;
    aura_json_skip_ws(cursor);
  }
}

static int aura_json_object(AuraJsonCursor *cursor)
{
  if (cursor->data[cursor->index++] != '{' || ++cursor->depth > 64) return 0;
  aura_json_skip_ws(cursor);
  if (cursor->index < cursor->length && cursor->data[cursor->index] == '}')
  {
    cursor->index++;
    cursor->depth--;
    return 1;
  }
  for (;;)
  {
    if (!aura_json_string(cursor)) return 0;
    aura_json_skip_ws(cursor);
    if (cursor->index >= cursor->length || cursor->data[cursor->index++] != ':') return 0;
    aura_json_skip_ws(cursor);
    if (!aura_json_value(cursor)) return 0;
    aura_json_skip_ws(cursor);
    if (cursor->index >= cursor->length) return 0;
    if (cursor->data[cursor->index] == '}') { cursor->index++; cursor->depth--; return 1; }
    if (cursor->data[cursor->index++] != ',') return 0;
    aura_json_skip_ws(cursor);
  }
}

static int aura_json_number(AuraJsonCursor *cursor)
{
  size_t start = cursor->index;
  if (cursor->index < cursor->length && cursor->data[cursor->index] == '-') cursor->index++;
  if (cursor->index >= cursor->length) return 0;
  if (cursor->data[cursor->index] == '0') cursor->index++;
  else
  {
    if (cursor->data[cursor->index] < '1' || cursor->data[cursor->index] > '9') return 0;
    while (cursor->index < cursor->length && isdigit(cursor->data[cursor->index])) cursor->index++;
  }
  if (cursor->index < cursor->length && cursor->data[cursor->index] == '.')
  {
    cursor->index++;
    if (cursor->index >= cursor->length || !isdigit(cursor->data[cursor->index])) return 0;
    while (cursor->index < cursor->length && isdigit(cursor->data[cursor->index])) cursor->index++;
  }
  if (cursor->index < cursor->length && (cursor->data[cursor->index] == 'e' || cursor->data[cursor->index] == 'E'))
  {
    cursor->index++;
    if (cursor->index < cursor->length && (cursor->data[cursor->index] == '+' || cursor->data[cursor->index] == '-')) cursor->index++;
    if (cursor->index >= cursor->length || !isdigit(cursor->data[cursor->index])) return 0;
    while (cursor->index < cursor->length && isdigit(cursor->data[cursor->index])) cursor->index++;
  }
  return cursor->index > start;
}

static int aura_json_value(AuraJsonCursor *cursor)
{
  aura_json_skip_ws(cursor);
  if (cursor->index >= cursor->length) return 0;
  switch (cursor->data[cursor->index])
  {
    case '"': return aura_json_string(cursor);
    case '[': return aura_json_array(cursor);
    case '{': return aura_json_object(cursor);
    case 't':
      if (cursor->index + 4 <= cursor->length && memcmp(cursor->data + cursor->index, "true", 4) == 0) { cursor->index += 4; return 1; }
      return 0;
    case 'f':
      if (cursor->index + 5 <= cursor->length && memcmp(cursor->data + cursor->index, "false", 5) == 0) { cursor->index += 5; return 1; }
      return 0;
    case 'n':
      if (cursor->index + 4 <= cursor->length && memcmp(cursor->data + cursor->index, "null", 4) == 0) { cursor->index += 4; return 1; }
      return 0;
    default: return aura_json_number(cursor);
  }
}

_Bool aura_json_is_valid(const char *value)
{
  AuraJsonCursor cursor;
  if (value == NULL || !aura_encoding_is_valid_utf8(value)) return false;
  cursor.data = (const unsigned char *)value;
  cursor.length = strlen(value);
  cursor.index = 0;
  cursor.depth = 0;
  if (!aura_json_value(&cursor)) return false;
  aura_json_skip_ws(&cursor);
  return cursor.index == cursor.length;
}

int64_t aura_json_error_offset(const char *value)
{
  AuraJsonCursor cursor;
  if (value == NULL || !aura_encoding_is_valid_utf8(value)) return 0;
  cursor.data = (const unsigned char *)value;
  cursor.length = strlen(value);
  cursor.index = 0;
  cursor.depth = 0;
  if (!aura_json_value(&cursor)) return (int64_t)cursor.index;
  aura_json_skip_ws(&cursor);
  return cursor.index == cursor.length ? -1 : (int64_t)cursor.index;
}

static const char *aura_json_trim_start(const char *value)
{
  const char *cursor = value == NULL ? "" : value;
  while (*cursor == ' ' || *cursor == '\t' || *cursor == '\n' || *cursor == '\r') cursor++;
  return cursor;
}

_Bool aura_json_parse_int(const char *value, int64_t *out)
{
  const char *cursor = aura_json_trim_start(value);
  char *end = NULL;
  if (*cursor == '+') return false;
  errno = 0;
  long long parsed = strtoll(cursor, &end, 10);
  if (cursor == end || errno == ERANGE || (*end != '\0' && *end != ' ' && *end != '\t' && *end != '\n' && *end != '\r')) return false;
  end = (char *)aura_json_trim_start(end);
  if (*end != '\0' || !aura_json_is_valid(value)) return false;
  if (out != NULL) *out = (int64_t)parsed;
  return true;
}

_Bool aura_json_parse_bool(const char *value, _Bool *out)
{
  const char *cursor = aura_json_trim_start(value);
  _Bool result;
  if (strcmp(cursor, "true") == 0) result = true;
  else if (strcmp(cursor, "false") == 0) result = false;
  else return false;
  if (out != NULL) *out = result;
  return true;
}

const char *aura_json_escape_string(const char *value)
{
  const unsigned char *input = (const unsigned char *)(value == NULL ? "" : value);
  size_t length = strlen((const char *)input);
  if (!aura_encoding_is_valid_utf8((const char *)input) || length > (SIZE_MAX - 3) / 2) return NULL;
  char *out = (char *)malloc(length * 2 + 3);
  size_t o = 0;
  if (out == NULL) return NULL;
  out[o++] = '"';
  for (size_t i = 0; i < length; i++)
  {
    unsigned char c = input[i];
    switch (c)
    {
      case '"': out[o++] = '\\'; out[o++] = '"'; break;
      case '\\': out[o++] = '\\'; out[o++] = '\\'; break;
      case '\b': out[o++] = '\\'; out[o++] = 'b'; break;
      case '\f': out[o++] = '\\'; out[o++] = 'f'; break;
      case '\n': out[o++] = '\\'; out[o++] = 'n'; break;
      case '\r': out[o++] = '\\'; out[o++] = 'r'; break;
      case '\t': out[o++] = '\\'; out[o++] = 't'; break;
      default: out[o++] = (char)c; break;
    }
  }
  out[o++] = '"';
  out[o] = '\0';
  return out;
}

static char *aura_json_copy_range(const unsigned char *data, size_t start, size_t end)
{
  if (data == NULL || end < start) return NULL;
  size_t length = end - start;
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  if (length != 0) memcpy(out, data + start, length);
  out[length] = '\0';
  return out;
}

static int aura_json_parse_value_span(const unsigned char *data, size_t length,
                                      size_t start, size_t *end)
{
  AuraJsonCursor cursor = { data, length, start, 0 };
  if (!aura_json_value(&cursor)) return 0;
  if (end != NULL) *end = cursor.index;
  return 1;
}

static int aura_json_parse_u16(const unsigned char *data, size_t length,
                               size_t *index, uint32_t *value)
{
  if (*index + 4 > length) return 0;
  uint32_t out = 0;
  for (unsigned i = 0; i < 4; i++)
  {
    int digit = aura_json_hex(data[(*index)++]);
    if (digit < 0) return 0;
    out = (out << 4) | (uint32_t)digit;
  }
  *value = out;
  return 1;
}

static int aura_json_append_utf8(char *out, size_t capacity, size_t *index,
                                 uint32_t codepoint)
{
  if (codepoint == 0 || codepoint > 0x10ffff ||
      (codepoint >= 0xd800 && codepoint <= 0xdfff)) return 0;
  if (codepoint <= 0x7f)
  {
    if (*index + 1 >= capacity) return 0;
    out[(*index)++] = (char)codepoint;
  }
  else if (codepoint <= 0x7ff)
  {
    if (*index + 2 >= capacity) return 0;
    out[(*index)++] = (char)(0xc0 | (codepoint >> 6));
    out[(*index)++] = (char)(0x80 | (codepoint & 0x3f));
  }
  else if (codepoint <= 0xffff)
  {
    if (*index + 3 >= capacity) return 0;
    out[(*index)++] = (char)(0xe0 | (codepoint >> 12));
    out[(*index)++] = (char)(0x80 | ((codepoint >> 6) & 0x3f));
    out[(*index)++] = (char)(0x80 | (codepoint & 0x3f));
  }
  else
  {
    if (*index + 4 >= capacity) return 0;
    out[(*index)++] = (char)(0xf0 | (codepoint >> 18));
    out[(*index)++] = (char)(0x80 | ((codepoint >> 12) & 0x3f));
    out[(*index)++] = (char)(0x80 | ((codepoint >> 6) & 0x3f));
    out[(*index)++] = (char)(0x80 | (codepoint & 0x3f));
  }
  return 1;
}

static char *aura_json_decode_range(const unsigned char *data, size_t start, size_t end)
{
  if (data == NULL || end <= start || data[start] != '"' || data[end - 1] != '"') return NULL;
  size_t capacity = end - start + 1;
  char *out = (char *)malloc(capacity);
  if (out == NULL) return NULL;
  size_t input = start + 1;
  size_t output = 0;
  while (input + 1 < end)
  {
    unsigned char c = data[input++];
    if (c != '\\')
    {
      if (c < 0x20 || output + 1 >= capacity) { free(out); return NULL; }
      out[output++] = (char)c;
      continue;
    }
    if (input >= end) { free(out); return NULL; }
    c = data[input++];
    switch (c)
    {
      case '"': out[output++] = '"'; break;
      case '\\': out[output++] = '\\'; break;
      case '/': out[output++] = '/'; break;
      case 'b': out[output++] = '\b'; break;
      case 'f': out[output++] = '\f'; break;
      case 'n': out[output++] = '\n'; break;
      case 'r': out[output++] = '\r'; break;
      case 't': out[output++] = '\t'; break;
      case 'u':
      {
        uint32_t codepoint = 0;
        if (!aura_json_parse_u16(data, end, &input, &codepoint)) { free(out); return NULL; }
        if (codepoint >= 0xd800 && codepoint <= 0xdbff)
        {
          if (input + 6 > end || data[input++] != '\\' || data[input++] != 'u') { free(out); return NULL; }
          uint32_t low = 0;
          if (!aura_json_parse_u16(data, end, &input, &low) || low < 0xdc00 || low > 0xdfff)
          {
            free(out);
            return NULL;
          }
          codepoint = 0x10000 + ((codepoint - 0xd800) << 10) + (low - 0xdc00);
        }
        if (!aura_json_append_utf8(out, capacity, &output, codepoint)) { free(out); return NULL; }
        break;
      }
      default: free(out); return NULL;
    }
    if (output >= capacity) { free(out); return NULL; }
  }
  out[output] = '\0';
  return out;
}

const char *aura_json_decode_string(const char *value)
{
  if (value == NULL || !aura_json_is_valid(value) || value[0] != '"') return NULL;
  size_t length = strlen(value);
  AuraJsonCursor cursor = { (const unsigned char *)value, length, 0, 0 };
  if (!aura_json_string(&cursor) || cursor.index != length) return NULL;
  return aura_json_decode_range((const unsigned char *)value, 0, length);
}

const char *aura_json_object_get(const char *value, const char *key)
{
  if (value == NULL || key == NULL || !aura_json_is_valid(value)) return NULL;
  size_t length = strlen(value);
  AuraJsonCursor cursor = { (const unsigned char *)value, length, 0, 0 };
  aura_json_skip_ws(&cursor);
  if (cursor.index >= length || cursor.data[cursor.index++] != '{') return NULL;
  aura_json_skip_ws(&cursor);
  if (cursor.index < length && cursor.data[cursor.index] == '}') return NULL;
  for (;;)
  {
    size_t key_start = cursor.index;
    if (!aura_json_string(&cursor)) return NULL;
    size_t key_end = cursor.index;
    char *decoded = aura_json_decode_range(cursor.data, key_start, key_end);
    aura_json_skip_ws(&cursor);
    if (cursor.index >= length || cursor.data[cursor.index++] != ':') { free(decoded); return NULL; }
    aura_json_skip_ws(&cursor);
    size_t value_start = cursor.index;
    size_t value_end = 0;
    if (!aura_json_parse_value_span(cursor.data, length, value_start, &value_end)) { free(decoded); return NULL; }
    cursor.index = value_end;
    if (decoded != NULL && strcmp(decoded, key) == 0)
    {
      free(decoded);
      return aura_json_copy_range(cursor.data, value_start, value_end);
    }
    free(decoded);
    aura_json_skip_ws(&cursor);
    if (cursor.index >= length || cursor.data[cursor.index] == '}') return NULL;
    if (cursor.data[cursor.index++] != ',') return NULL;
    aura_json_skip_ws(&cursor);
  }
}

const char *aura_json_array_at(const char *value, int64_t wanted)
{
  if (value == NULL || wanted < 0 || !aura_json_is_valid(value)) return NULL;
  size_t length = strlen(value);
  AuraJsonCursor cursor = { (const unsigned char *)value, length, 0, 0 };
  aura_json_skip_ws(&cursor);
  if (cursor.index >= length || cursor.data[cursor.index++] != '[') return NULL;
  aura_json_skip_ws(&cursor);
  int64_t index = 0;
  while (cursor.index < length && cursor.data[cursor.index] != ']')
  {
    size_t value_start = cursor.index;
    size_t value_end = 0;
    if (!aura_json_parse_value_span(cursor.data, length, value_start, &value_end)) return NULL;
    if (index == wanted) return aura_json_copy_range(cursor.data, value_start, value_end);
    index++;
    cursor.index = value_end;
    aura_json_skip_ws(&cursor);
    if (cursor.index < length && cursor.data[cursor.index] == ',') { cursor.index++; aura_json_skip_ws(&cursor); }
  }
  return NULL;
}

int64_t aura_json_array_count(const char *value)
{
  if (value == NULL || !aura_json_is_valid(value)) return 0;
  size_t length = strlen(value);
  AuraJsonCursor cursor = { (const unsigned char *)value, length, 0, 0 };
  aura_json_skip_ws(&cursor);
  if (cursor.index >= length || cursor.data[cursor.index++] != '[') return 0;
  aura_json_skip_ws(&cursor);
  int64_t count = 0;
  while (cursor.index < length && cursor.data[cursor.index] != ']')
  {
    size_t end = 0;
    if (!aura_json_parse_value_span(cursor.data, length, cursor.index, &end)) return 0;
    count++;
    cursor.index = end;
    aura_json_skip_ws(&cursor);
    if (cursor.index < length && cursor.data[cursor.index] == ',') { cursor.index++; aura_json_skip_ws(&cursor); }
  }
  return count;
}

typedef struct AuraJsonBuffer { char *data; size_t length; size_t capacity; } AuraJsonBuffer;

static int aura_json_buffer_append(AuraJsonBuffer *buffer, const unsigned char *data, size_t length)
{
  if (length > SIZE_MAX - buffer->length - 1) return 0;
  size_t needed = buffer->length + length + 1;
  if (needed > buffer->capacity)
  {
    size_t capacity = buffer->capacity == 0 ? 32 : buffer->capacity;
    while (capacity < needed) {
      if (capacity > SIZE_MAX / 2) { capacity = needed; break; }
      capacity *= 2;
    }
    char *grown = (char *)realloc(buffer->data, capacity);
    if (grown == NULL) return 0;
    buffer->data = grown;
    buffer->capacity = capacity;
  }
  if (length != 0) memcpy(buffer->data + buffer->length, data, length);
  buffer->length += length;
  buffer->data[buffer->length] = '\0';
  return 1;
}

const char *aura_json_object_keys(const char *value)
{
  if (value == NULL || !aura_json_is_valid(value)) return NULL;
  size_t length = strlen(value);
  AuraJsonCursor cursor = { (const unsigned char *)value, length, 0, 0 };
  aura_json_skip_ws(&cursor);
  if (cursor.index >= length || cursor.data[cursor.index++] != '{') return NULL;
  AuraJsonBuffer output = { NULL, 0, 0 };
  if (!aura_json_buffer_append(&output, (const unsigned char *)"[", 1)) return NULL;
  aura_json_skip_ws(&cursor);
  int first = 1;
  while (cursor.index < length && cursor.data[cursor.index] != '}')
  {
    size_t key_start = cursor.index;
    if (!aura_json_string(&cursor)) { free(output.data); return NULL; }
    size_t key_end = cursor.index;
    if (!first && !aura_json_buffer_append(&output, (const unsigned char *)",", 1)) { free(output.data); return NULL; }
    if (!aura_json_buffer_append(&output, cursor.data + key_start, key_end - key_start)) { free(output.data); return NULL; }
    first = 0;
    aura_json_skip_ws(&cursor);
    if (cursor.index >= length || cursor.data[cursor.index++] != ':') { free(output.data); return NULL; }
    aura_json_skip_ws(&cursor);
    size_t end = 0;
    if (!aura_json_parse_value_span(cursor.data, length, cursor.index, &end)) { free(output.data); return NULL; }
    cursor.index = end;
    aura_json_skip_ws(&cursor);
    if (cursor.index < length && cursor.data[cursor.index] == ',') { cursor.index++; aura_json_skip_ws(&cursor); }
  }
  if (!aura_json_buffer_append(&output, (const unsigned char *)"]", 1)) { free(output.data); return NULL; }
  return output.data;
}

const char *aura_json_duplicate_key(const char *value)
{
  if (value == NULL || !aura_json_is_valid(value)) return NULL;
  size_t length = strlen(value);
  AuraJsonCursor cursor = { (const unsigned char *)value, length, 0, 0 };
  aura_json_skip_ws(&cursor);
  if (cursor.index >= length || cursor.data[cursor.index++] != '{') return NULL;
  char **seen = NULL;
  size_t count = 0;
  aura_json_skip_ws(&cursor);
  while (cursor.index < length && cursor.data[cursor.index] != '}')
  {
    size_t key_start = cursor.index;
    if (!aura_json_string(&cursor)) break;
    char *key = aura_json_decode_range(cursor.data, key_start, cursor.index);
    if (key == NULL) break;
    for (size_t i = 0; i < count; i++)
    {
      if (strcmp(seen[i], key) == 0)
      {
        for (size_t j = 0; j < count; j++) free(seen[j]);
        free(seen);
        return key;
      }
    }
    char **grown = (char **)realloc(seen, (count + 1) * sizeof(*seen));
    if (grown == NULL) { free(key); break; }
    seen = grown;
    seen[count++] = key;
    aura_json_skip_ws(&cursor);
    if (cursor.index >= length || cursor.data[cursor.index++] != ':') break;
    aura_json_skip_ws(&cursor);
    size_t end = 0;
    if (!aura_json_parse_value_span(cursor.data, length, cursor.index, &end)) break;
    cursor.index = end;
    aura_json_skip_ws(&cursor);
    if (cursor.index < length && cursor.data[cursor.index] == ',') { cursor.index++; aura_json_skip_ws(&cursor); }
  }
  for (size_t i = 0; i < count; i++) free(seen[i]);
  free(seen);
  return NULL;
}

static int aura_url_target_byte_allowed(unsigned char c)
{
  return c >= 0x21 && c != 0x23 && c != 0x7f;
}

static char *aura_url_copy_range(const char *value, size_t length)
{
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  if (length != 0) memcpy(out, value, length);
  out[length] = '\0';
  return out;
}

static int aura_url_origin_parts(const char *target, size_t *path_length,
                                 size_t *query_start)
{
  size_t length = target == NULL ? 0 : strlen(target);
  if (length == 0 || target[0] != '/' || (length > 1 && target[1] == '/')) return 0;
  size_t question = SIZE_MAX;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)target[i];
    if (!aura_url_target_byte_allowed(c)) return 0;
    if (c == '?' && question == SIZE_MAX) question = i;
  }
  if (path_length != NULL) *path_length = question == SIZE_MAX ? length : question;
  if (query_start != NULL) *query_start = question;
  return 1;
}

static int aura_url_absolute_parts(const char *target, size_t *authority_start,
                                   size_t *authority_length)
{
  size_t length = target == NULL ? 0 : strlen(target);
  size_t i = 0;
  if (length == 0 || !isalpha((unsigned char)target[0])) return 0;
  i = 1;
  while (i < length && (isalnum((unsigned char)target[i]) || target[i] == '+' ||
                        target[i] == '-' || target[i] == '.'))
    i++;
  if (i + 3 > length || target[i] != ':' || target[i + 1] != '/' || target[i + 2] != '/') return 0;
  size_t start = i + 3;
  size_t end = start;
  while (end < length && target[end] != '/' && target[end] != '?' && target[end] != '#') {
    unsigned char c = (unsigned char)target[end];
    if (c <= 0x20 || c == 0x7f) return 0;
    end++;
  }
  if (end == start) return 0;
  if (authority_start != NULL) *authority_start = start;
  if (authority_length != NULL) *authority_length = end - start;
  return 1;
}

static char *aura_bytes_copy_n(const char *value, size_t length)
{
  char *out = (char *)malloc(length + 1u);
  if (out == NULL)
  {
    return NULL;
  }
  if (length != 0 && value != NULL)
  {
    memcpy(out, value, length);
  }
  out[length] = '\0';
  return out;
}

const char *aura_bytes_copy(const char *value)
{
  const char *source = value == NULL ? "" : value;
  return aura_bytes_copy_n(source, strlen(source));
}

const char *aura_bytes_concat(const char *left, const char *right)
{
  const char *a = left == NULL ? "" : left;
  const char *b = right == NULL ? "" : right;
  size_t alen = strlen(a);
  size_t blen = strlen(b);
  if (alen > SIZE_MAX - blen || alen + blen == SIZE_MAX)
  {
    return NULL;
  }
  char *out = (char *)malloc(alen + blen + 1u);
  if (out == NULL)
  {
    return NULL;
  }
  memcpy(out, a, alen);
  memcpy(out + alen, b, blen);
  out[alen + blen] = '\0';
  return out;
}

const char *aura_bytes_slice(const char *value, int64_t start, int64_t length)
{
  const char *source = value == NULL ? "" : value;
  size_t total = strlen(source);
  if (start < 0 || length < 0 || (uint64_t)start > (uint64_t)total ||
      (uint64_t)length > (uint64_t)total - (uint64_t)start)
  {
    return NULL;
  }
  return aura_bytes_copy_n(source + (size_t)start, (size_t)length);
}

_Bool aura_bytes_equals(const char *left, const char *right)
{
  const char *a = left == NULL ? "" : left;
  const char *b = right == NULL ? "" : right;
  size_t alen = strlen(a);
  size_t blen = strlen(b);
  return alen == blen && (alen == 0 || memcmp(a, b, alen) == 0);
}

typedef struct { uint32_t state[8]; uint64_t bits; size_t used; unsigned char block[64]; } AuraSha256;

static uint32_t aura_sha_rotr(uint32_t x, unsigned n) { return (x >> n) | (x << (32u - n)); }

static void aura_sha256_block(AuraSha256 *ctx, const unsigned char *block)
{
  static const uint32_t k[64] = {
      0x428a2f98u,0x71374491u,0xb5c0fbcfu,0xe9b5dba5u,0x3956c25bu,0x59f111f1u,0x923f82a4u,0xab1c5ed5u,
      0xd807aa98u,0x12835b01u,0x243185beu,0x550c7dc3u,0x72be5d74u,0x80deb1feu,0x9bdc06a7u,0xc19bf174u,
      0xe49b69c1u,0xefbe4786u,0x0fc19dc6u,0x240ca1ccu,0x2de92c6fu,0x4a7484aau,0x5cb0a9dcu,0x76f988dau,
      0x983e5152u,0xa831c66du,0xb00327c8u,0xbf597fc7u,0xc6e00bf3u,0xd5a79147u,0x06ca6351u,0x14292967u,
      0x27b70a85u,0x2e1b2138u,0x4d2c6dfcu,0x53380d13u,0x650a7354u,0x766a0abbu,0x81c2c92eu,0x92722c85u,
      0xa2bfe8a1u,0xa81a664bu,0xc24b8b70u,0xc76c51a3u,0xd192e819u,0xd6990624u,0xf40e3585u,0x106aa070u,
      0x19a4c116u,0x1e376c08u,0x2748774cu,0x34b0bcb5u,0x391c0cb3u,0x4ed8aa4au,0x5b9cca4fu,0x682e6ff3u,
      0x748f82eeu,0x78a5636fu,0x84c87814u,0x8cc70208u,0x90befffau,0xa4506cebu,0xbef9a3f7u,0xc67178f2u};
  uint32_t w[64];
  for (size_t i=0;i<16;i++) w[i]=((uint32_t)block[i*4]<<24)|((uint32_t)block[i*4+1]<<16)|((uint32_t)block[i*4+2]<<8)|block[i*4+3];
  for (size_t i=16;i<64;i++) { uint32_t s0=aura_sha_rotr(w[i-15],7)^aura_sha_rotr(w[i-15],18)^(w[i-15]>>3); uint32_t s1=aura_sha_rotr(w[i-2],17)^aura_sha_rotr(w[i-2],19)^(w[i-2]>>10); w[i]=w[i-16]+s0+w[i-7]+s1; }
  uint32_t a=ctx->state[0],b=ctx->state[1],c=ctx->state[2],d=ctx->state[3],e=ctx->state[4],f=ctx->state[5],g=ctx->state[6],h=ctx->state[7];
  for (size_t i=0;i<64;i++) { uint32_t s1=aura_sha_rotr(e,6)^aura_sha_rotr(e,11)^aura_sha_rotr(e,25); uint32_t t1=h+s1+((e&f)^(~e&g))+k[i]+w[i]; uint32_t s0=aura_sha_rotr(a,2)^aura_sha_rotr(a,13)^aura_sha_rotr(a,22); uint32_t t2=s0+((a&b)^(a&c)^(b&c)); h=g;g=f;f=e;e=d+t1;d=c;c=b;b=a;a=t1+t2; }
  ctx->state[0]+=a;ctx->state[1]+=b;ctx->state[2]+=c;ctx->state[3]+=d;ctx->state[4]+=e;ctx->state[5]+=f;ctx->state[6]+=g;ctx->state[7]+=h;
}

static void aura_sha256_init(AuraSha256 *ctx)
{ static const uint32_t s[8]={0x6a09e667u,0xbb67ae85u,0x3c6ef372u,0xa54ff53au,0x510e527fu,0x9b05688cu,0x1f83d9abu,0x5be0cd19u}; memcpy(ctx->state,s,sizeof(s));ctx->bits=0;ctx->used=0; }
static void aura_sha256_update(AuraSha256 *ctx,const unsigned char *data,size_t length)
{ ctx->bits+=(uint64_t)length*8u; while(length){size_t take=64u-ctx->used;if(take>length)take=length;memcpy(ctx->block+ctx->used,data,take);ctx->used+=take;data+=take;length-=take;if(ctx->used==64u){aura_sha256_block(ctx,ctx->block);ctx->used=0;}} }
static void aura_sha256_final(AuraSha256 *ctx,unsigned char digest[32])
{ size_t used=ctx->used;ctx->block[used++]=0x80u;if(used>56u){memset(ctx->block+used,0,64u-used);aura_sha256_block(ctx,ctx->block);used=0;}memset(ctx->block+used,0,56u-used);for(unsigned i=0;i<8;i++)ctx->block[56u+i]=(unsigned char)(ctx->bits>>(56u-i*8u));aura_sha256_block(ctx,ctx->block);for(unsigned i=0;i<8;i++)for(unsigned j=0;j<4;j++)digest[i*4+j]=(unsigned char)(ctx->state[i]>>(24u-j*8u)); }
static void aura_sha256_bytes(const unsigned char *data,size_t length,unsigned char digest[32])
{ AuraSha256 ctx;aura_sha256_init(&ctx);aura_sha256_update(&ctx,data,length);aura_sha256_final(&ctx,digest); }
static char *aura_digest_hex(const unsigned char digest[32])
{ static const char h[]="0123456789abcdef";char *out=(char *)malloc(65u);if(!out)return NULL;for(size_t i=0;i<32;i++){out[i*2]=h[digest[i]>>4];out[i*2+1]=h[digest[i]&15u];}out[64]='\0';return out; }

const char *aura_crypto_sha256(const char *value)
{ const char *source=value==NULL?"":value;unsigned char digest[32];aura_sha256_bytes((const unsigned char *)source,strlen(source),digest);return aura_digest_hex(digest); }
const char *aura_crypto_hmac_sha256(const char *key,const char *value)
{ unsigned char kb[64]={0},inner[64],outer[64],digest[32];const unsigned char *k=(const unsigned char *)(key==NULL?"":key);size_t kl=strlen((const char *)k);if(kl>64u)aura_sha256_bytes(k,kl,kb);else memcpy(kb,k,kl);for(size_t i=0;i<64;i++){inner[i]=kb[i]^0x36u;outer[i]=kb[i]^0x5cu;}AuraSha256 ctx;aura_sha256_init(&ctx);aura_sha256_update(&ctx,inner,64u);const char *source=value==NULL?"":value;aura_sha256_update(&ctx,(const unsigned char *)source,strlen(source));aura_sha256_final(&ctx,digest);aura_sha256_init(&ctx);aura_sha256_update(&ctx,outer,64u);aura_sha256_update(&ctx,digest,32u);aura_sha256_final(&ctx,digest);return aura_digest_hex(digest); }
_Bool aura_crypto_constant_time_equals(const char *left,const char *right)
{ const unsigned char *a=(const unsigned char *)(left==NULL?"":left),*b=(const unsigned char *)(right==NULL?"":right);size_t al=strlen((const char *)a),bl=strlen((const char *)b),n=al>bl?al:bl;unsigned diff=(unsigned)(al^bl);for(size_t i=0;i<n;i++)diff|=(unsigned)(i<al?a[i]:0)^(unsigned)(i<bl?b[i]:0);return diff==0; }
const char *aura_crypto_random_bytes(int64_t length)
{ if(length<0||(uint64_t)length>SIZE_MAX-1u)return NULL;size_t n=(size_t)length;unsigned char *out=(unsigned char *)malloc(n+1u);if(!out)return NULL;
#if defined(__unix__) || defined(__APPLE__)
  FILE *f=fopen("/dev/urandom","rb");if(f==NULL||(n!=0&&fread(out,1,n,f)!=n)){if(f)fclose(f);free(out);return NULL;}if(f)fclose(f);
#else
  free(out);return NULL;
#endif
  // Aura String is NUL-terminated; reject NUL bytes so the returned byte
  // string retains its requested length without truncating at the first byte.
  for (size_t i = 0; i < n; i++) if (out[i] == 0) out[i] = 1;
  out[n]='\0';return (const char *)out; }

static char *aura_binary_hex(const unsigned char *data, size_t length)
{
  static const char digits[] = "0123456789abcdef";
  if (length > (SIZE_MAX - 1u) / 2u) return NULL;
  char *out = (char *)malloc(length * 2u + 1u);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i++) { out[i * 2u] = digits[data[i] >> 4]; out[i * 2u + 1u] = digits[data[i] & 15u]; }
  out[length * 2u] = '\0';
  return out;
}

static int aura_hex_digit(unsigned char value)
{
  if (value >= '0' && value <= '9') return (int)(value - '0');
  if (value >= 'a' && value <= 'f') return (int)(value - 'a' + 10);
  if (value >= 'A' && value <= 'F') return (int)(value - 'A' + 10);
  return -1;
}

const char *aura_compress_text(const char *value, int64_t codec, int64_t level)
{
  const unsigned char *source = (const unsigned char *)(value == NULL ? "" : value);
  size_t source_len = strlen((const char *)source);
  uLong bound = compressBound((uLong)source_len);
  unsigned char *compressed = (unsigned char *)malloc((size_t)bound + 32u);
  if (compressed == NULL) return NULL;
  z_stream stream;
  memset(&stream, 0, sizeof(stream));
  int window_bits = codec == 0 ? 15 + 16 : 15;
  int normalized_level = level < 0 ? Z_DEFAULT_COMPRESSION : (level > 9 ? 9 : (int)level);
  if (deflateInit2(&stream, normalized_level, Z_DEFLATED, window_bits, 8, Z_DEFAULT_STRATEGY) != Z_OK) { free(compressed); return NULL; }
  stream.next_in = (Bytef *)source; stream.avail_in = (uInt)source_len;
  stream.next_out = compressed; stream.avail_out = (uInt)(bound + 32u);
  int result = deflate(&stream, Z_FINISH);
  size_t written = (size_t)stream.total_out;
  deflateEnd(&stream);
  if (result != Z_STREAM_END) { free(compressed); return NULL; }
  char *encoded = aura_binary_hex(compressed, written);
  free(compressed);
  return encoded;
}

const char *aura_decompress_text(const char *value, int64_t codec)
{
  const char *encoded = value == NULL ? "" : value;
  size_t encoded_len = strlen(encoded);
  if ((encoded_len & 1u) != 0 || encoded_len > 128u * 1024u * 1024u) return NULL;
  size_t compressed_len = encoded_len / 2u;
  unsigned char *compressed = (unsigned char *)malloc(compressed_len == 0 ? 1u : compressed_len);
  if (compressed == NULL) return NULL;
  for (size_t i = 0; i < compressed_len; i++) { int hi = aura_hex_digit((unsigned char)encoded[i * 2u]); int lo = aura_hex_digit((unsigned char)encoded[i * 2u + 1u]); if (hi < 0 || lo < 0) { free(compressed); return NULL; } compressed[i] = (unsigned char)((hi << 4) | lo); }
  size_t capacity = 4096u;
  unsigned char *output = (unsigned char *)malloc(capacity + 1u);
  if (output == NULL) { free(compressed); return NULL; }
  z_stream stream; memset(&stream, 0, sizeof(stream));
  int window_bits = codec == 0 ? 15 + 16 : 15;
  if (inflateInit2(&stream, window_bits) != Z_OK) { free(compressed); free(output); return NULL; }
  stream.next_in = compressed; stream.avail_in = (uInt)compressed_len;
  int result = Z_OK;
  while (result == Z_OK) {
    if (stream.total_out == capacity) { if (capacity >= 64u * 1024u * 1024u) { result = Z_MEM_ERROR; break; } capacity *= 2u; unsigned char *grown = (unsigned char *)realloc(output, capacity + 1u); if (grown == NULL) { result = Z_MEM_ERROR; break; } output = grown; }
    stream.next_out = output + stream.total_out; stream.avail_out = (uInt)(capacity - stream.total_out);
    result = inflate(&stream, Z_FINISH);
  }
  size_t written = (size_t)stream.total_out;
  inflateEnd(&stream); free(compressed);
  if (result != Z_STREAM_END || written > 64u * 1024u * 1024u || memchr(output, 0, written) != NULL) { free(output); return NULL; }
  output[written] = '\0';
  return (const char *)output;
}

static const char *aura_fs_text(const char *path)
{
  return path == NULL ? "" : path;
}

const char *aura_fs_join(const char *base, const char *child)
{
  const char *a = aura_fs_text(base);
  const char *b = aura_fs_text(child);
  size_t alen = strlen(a);
  size_t blen = strlen(b);
  while (alen > 0 && a[alen - 1] == '/')
  {
    alen--;
  }
  while (blen > 0 && *b == '/')
  {
    b++;
    blen--;
  }
  if (alen == 0)
  {
    return aura_bytes_copy_n(b, blen);
  }
  if (blen == 0)
  {
    return aura_bytes_copy_n(a, alen);
  }
  if (alen > SIZE_MAX - blen || alen + blen > SIZE_MAX - 2u)
  {
    return NULL;
  }
  char *out = (char *)malloc(alen + blen + 2u);
  if (out == NULL)
  {
    return NULL;
  }
  memcpy(out, a, alen);
  out[alen] = '/';
  memcpy(out + alen + 1u, b, blen);
  out[alen + blen + 1u] = '\0';
  return out;
}

const char *aura_fs_basename(const char *path)
{
  const char *p = aura_fs_text(path);
  size_t len = strlen(p);
  if (len == 1 && p[0] == '/')
  {
    return aura_bytes_copy_n("/", 1);
  }
  while (len > 0 && p[len - 1] == '/')
  {
    len--;
  }
  size_t start = len;
  while (start > 0 && p[start - 1] != '/')
  {
    start--;
  }
  return aura_bytes_copy_n(p + start, len - start);
}

const char *aura_fs_dirname(const char *path)
{
  const char *p = aura_fs_text(path);
  size_t len = strlen(p);
  if (len == 1 && p[0] == '/')
  {
    return aura_bytes_copy_n("/", 1);
  }
  while (len > 1 && p[len - 1] == '/')
  {
    len--;
  }
  size_t slash = len;
  while (slash > 0 && p[slash - 1] != '/')
  {
    slash--;
  }
  if (slash == 0)
  {
    return aura_bytes_copy_n(".", 1);
  }
  while (slash > 1 && p[slash - 1] == '/')
  {
    slash--;
  }
  return aura_bytes_copy_n(p, slash);
}

const char *aura_fs_extension(const char *path)
{
  const char *name = aura_fs_text(path);
  size_t len = strlen(name);
  while (len > 0 && name[len - 1] == '/')
  {
    len--;
  }
  size_t start = len;
  while (start > 0 && name[start - 1] != '/')
  {
    start--;
  }
  size_t dot = len;
  while (dot > start && name[dot - 1] != '.')
  {
    dot--;
  }
  if (dot == start || dot == len || (dot == start + 1u && len == start + 1u))
  {
    return NULL;
  }
  return aura_bytes_copy_n(name + dot - 1u, len - dot + 1u);
}

_Bool aura_fs_is_absolute(const char *path)
{
  const char *p = aura_fs_text(path);
  return p[0] == '/';
}

const char *aura_os_get_env(const char *name)
{
  const char *key = name == NULL ? "" : name;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return NULL;
  }
  const char *value = getenv(key);
  return value == NULL ? NULL : aura_bytes_copy(value);
}

_Bool aura_os_set_env(const char *name, const char *value)
{
  const char *key = name == NULL ? "" : name;
  const char *text = value == NULL ? "" : value;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return false;
  }
#if defined(__unix__) || defined(__APPLE__)
  return setenv(key, text, 1) == 0;
#else
  (void)text;
  return false;
#endif
}

_Bool aura_os_unset_env(const char *name)
{
  const char *key = name == NULL ? "" : name;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return false;
  }
#if defined(__unix__) || defined(__APPLE__)
  return unsetenv(key) == 0;
#else
  return false;
#endif
}

const char *aura_os_cwd(void)
{
#if defined(__unix__) || defined(__APPLE__)
  size_t capacity = 256;
  for (;;)
  {
    char *buffer = (char *)malloc(capacity);
    if (buffer == NULL)
    {
      return NULL;
    }
    if (getcwd(buffer, capacity) != NULL)
    {
      return buffer;
    }
    free(buffer);
    if (errno != ERANGE || capacity > SIZE_MAX / 2u)
    {
      return NULL;
    }
    capacity *= 2u;
  }
#else
  return NULL;
#endif
}

int64_t aura_os_pid(void)
{
#if defined(__unix__) || defined(__APPLE__)
  return (int64_t)getpid();
#else
  return -1;
#endif
}

const char *aura_os_platform(void)
{
#if defined(__APPLE__)
  return aura_bytes_copy("macos");
#elif defined(__linux__)
  return aura_bytes_copy("linux");
#elif defined(_WIN32)
  return aura_bytes_copy("windows");
#else
  return aura_bytes_copy("unknown");
#endif
}

/* Bounded numeric DNS lookup: return one address in presentation form. The
 * resolver result is copied into Aura-owned storage and the addrinfo chain is
 * released before returning. */
const char *aura_dns_resolve_host(const char *host, int prefer_ipv6)
{
#if defined(__unix__) || defined(__APPLE__)
  struct addrinfo hints;
  struct addrinfo *results = NULL;
  struct addrinfo *entry;
  char address[INET6_ADDRSTRLEN];
  int families[2];
  int i;

  if (host == NULL || host[0] == '\0') return NULL;
  memset(&hints, 0, sizeof(hints));
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_ADDRCONFIG;
  families[0] = prefer_ipv6 ? AF_INET6 : AF_INET;
  families[1] = prefer_ipv6 ? AF_INET : AF_INET6;
  for (i = 0; i < 2; i++)
  {
    hints.ai_family = families[i];
    if (getaddrinfo(host, NULL, &hints, &results) != 0) continue;
    for (entry = results; entry != NULL; entry = entry->ai_next)
    {
      const void *source = NULL;
      if (entry->ai_family == AF_INET)
      {
        source = &((const struct sockaddr_in *)entry->ai_addr)->sin_addr;
      }
      else if (entry->ai_family == AF_INET6)
      {
        source = &((const struct sockaddr_in6 *)entry->ai_addr)->sin6_addr;
      }
      if (source != NULL && inet_ntop(entry->ai_family, source, address,
                                      sizeof(address)) != NULL)
      {
        const char *copy = aura_bytes_copy(address);
        freeaddrinfo(results);
        return copy;
      }
    }
    freeaddrinfo(results);
    results = NULL;
  }
  return NULL;
#else
  (void)host;
  (void)prefer_ipv6;
  return NULL;
#endif
}

/* Return a bounded, preference-ordered address snapshot. Each line contains
 * one numeric address; the result is Aura-owned and capped at 64 KiB. */
const char *aura_dns_resolve_host_list(const char *host, int prefer_ipv6)
{
#if defined(__unix__) || defined(__APPLE__)
  struct addrinfo hints;
  struct addrinfo *results = NULL;
  struct addrinfo *entry;
  char address[INET6_ADDRSTRLEN];
  int families[2];
  int i;
  size_t used = 0;
  size_t capacity = 64u * 1024u;
  char *output;

  if (host == NULL || host[0] == '\0') return NULL;
  output = (char *)malloc(capacity);
  if (output == NULL) return NULL;
  output[0] = '\0';
  memset(&hints, 0, sizeof(hints));
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_ADDRCONFIG;
  families[0] = prefer_ipv6 ? AF_INET6 : AF_INET;
  families[1] = prefer_ipv6 ? AF_INET : AF_INET6;
  for (i = 0; i < 2; i++)
  {
    hints.ai_family = families[i];
    if (getaddrinfo(host, NULL, &hints, &results) != 0) continue;
    for (entry = results; entry != NULL; entry = entry->ai_next)
    {
      const void *source = NULL;
      size_t length;
      if (entry->ai_family == AF_INET)
      {
        source = &((const struct sockaddr_in *)entry->ai_addr)->sin_addr;
      }
      else if (entry->ai_family == AF_INET6)
      {
        source = &((const struct sockaddr_in6 *)entry->ai_addr)->sin6_addr;
      }
      if (source == NULL || inet_ntop(entry->ai_family, source, address,
                                      sizeof(address)) == NULL)
        continue;
      length = strlen(address);
      if (used + length + (used == 0 ? 0u : 1u) + 1u >= capacity) break;
      if (used != 0) output[used++] = '\n';
      memcpy(output + used, address, length);
      used += length;
      output[used] = '\0';
    }
    freeaddrinfo(results);
    results = NULL;
  }
  if (used == 0)
  {
    free(output);
    return NULL;
  }
  return output;
#else
  (void)host;
  (void)prefer_ipv6;
  return NULL;
#endif
}

_Bool aura_url_is_origin_form(const char *target)
{
  return aura_url_origin_parts(target, NULL, NULL) != 0;
}

const char *aura_url_path(const char *target)
{
  size_t path_length = 0;
  if (!aura_url_origin_parts(target, &path_length, NULL)) return NULL;
  return aura_url_copy_range(target, path_length);
}

const char *aura_url_normalize_path(const char *path)
{
  size_t length = path == NULL ? 0 : strlen(path);
  if (length == 0 || path[0] != '/') return NULL;
  for (size_t i = 0; i < length; i++)
  {
    unsigned char c = (unsigned char)path[i];
    if (!aura_url_target_byte_allowed(c) || c == '?' || c == '#') return NULL;
  }
  char *out = (char *)malloc(length + 2);
  if (out == NULL) return NULL;
  size_t used = 1;
  out[0] = '/';
  size_t segment_start = 1;
  for (size_t i = 1; i <= length; i++)
  {
    if (i != length && path[i] != '/') continue;
    size_t segment_length = i - segment_start;
    if (segment_length == 0 || (segment_length == 1 && path[segment_start] == '.'))
    {
      /* Repeated separators and dot segments do not add output. */
    }
    else if (segment_length == 2 && path[segment_start] == '.' && path[segment_start + 1] == '.')
    {
      if (used > 1)
      {
        used--;
        while (used > 1 && out[used - 1] != '/') used--;
      }
    }
    else
    {
      if (used > 1 && out[used - 1] != '/') out[used++] = '/';
      memcpy(out + used, path + segment_start, segment_length);
      used += segment_length;
    }
    segment_start = i + 1;
  }
  if (length > 1 && path[length - 1] == '/' && used > 1 && out[used - 1] != '/') out[used++] = '/';
  out[used] = '\0';
  return out;
}

const char *aura_url_query(const char *target)
{
  size_t query_start = SIZE_MAX;
  if (!aura_url_origin_parts(target, NULL, &query_start) || query_start == SIZE_MAX) return NULL;
  return aura_url_copy_range(target + query_start + 1, strlen(target) - query_start - 1);
}

_Bool aura_url_is_absolute(const char *target)
{
  return aura_url_absolute_parts(target, NULL, NULL) != 0;
}

const char *aura_url_authority(const char *target)
{
  size_t start = 0;
  size_t length = 0;
  if (!aura_url_absolute_parts(target, &start, &length)) return NULL;
  return aura_url_copy_range(target + start, length);
}

static int aura_url_authority_bounds(const char *target, size_t *start,
                                     size_t *length)
{
  size_t authority_start = 0;
  size_t authority_length = 0;
  if (!aura_url_absolute_parts(target, &authority_start, &authority_length)) return 0;
  size_t end = authority_start + authority_length;
  size_t userinfo = SIZE_MAX;
  for (size_t i = authority_start; i < end; i++) {
    if (target[i] == '@') userinfo = i;
  }
  if (userinfo != SIZE_MAX) authority_start = userinfo + 1;
  if (authority_start >= end) return 0;
  if (start != NULL) *start = authority_start;
  if (length != NULL) *length = end - authority_start;
  return 1;
}

const char *aura_url_authority_host(const char *target)
{
  size_t start = 0, length = 0;
  if (!aura_url_authority_bounds(target, &start, &length)) return NULL;
  size_t end = start + length;
  size_t host_end = end;
  if (target[start] == '[') {
    size_t close = start + 1;
    while (close < end && target[close] != ']') close++;
    if (close >= end || close == start + 1) return NULL;
    host_end = close;
    return aura_url_copy_range(target + start + 1, host_end - start - 1);
  }
  size_t colon = SIZE_MAX;
  for (size_t i = start; i < end; i++) {
    if (target[i] == ':') {
      if (colon != SIZE_MAX) return NULL;
      colon = i;
    }
  }
  if (colon != SIZE_MAX) host_end = colon;
  if (host_end == start) return NULL;
  return aura_url_copy_range(target + start, host_end - start);
}

const char *aura_url_authority_port(const char *target)
{
  size_t start = 0, length = 0;
  if (!aura_url_authority_bounds(target, &start, &length)) return NULL;
  size_t end = start + length;
  size_t port_start = SIZE_MAX;
  if (target[start] == '[') {
    size_t close = start + 1;
    while (close < end && target[close] != ']') close++;
    if (close >= end || close + 1 >= end || target[close + 1] != ':') return NULL;
    port_start = close + 2;
  } else {
    for (size_t i = start; i < end; i++) {
      if (target[i] == ':') {
        if (port_start != SIZE_MAX) return NULL;
        port_start = i + 1;
      }
    }
  }
  if (port_start == SIZE_MAX || port_start >= end) return NULL;
  for (size_t i = port_start; i < end; i++) {
    if (!isdigit((unsigned char)target[i])) return NULL;
  }
  return aura_url_copy_range(target + port_start, end - port_start);
}

const char *aura_url_query_value(const char *target, const char *key)
{
  if (target == NULL || key == NULL || key[0] == '\0') return NULL;
  const char *question = strchr(target, '?');
  if (question == NULL) return NULL;
  const char *cursor = question + 1;
  size_t key_length = strlen(key);
  while (*cursor != '\0' && *cursor != '#') {
    const char *amp = strchr(cursor, '&');
    const char *end = amp == NULL ? cursor + strlen(cursor) : amp;
    const char *equals = memchr(cursor, '=', (size_t)(end - cursor));
    const char *value = equals == NULL ? end : equals + 1;
    size_t candidate_length = equals == NULL ? (size_t)(end - cursor) : (size_t)(equals - cursor);
    if (candidate_length == key_length && memcmp(cursor, key, key_length) == 0)
      return aura_url_copy_range(value, (size_t)(end - value));
    if (amp == NULL) break;
    cursor = amp + 1;
  }
  return NULL;
}

static int aura_mime_token_byte(unsigned char c)
{
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
         (c >= '0' && c <= '9') || strchr("!#$%&'*+-.^_`|~", (int)c) != NULL;
}

_Bool aura_mime_is_valid_type(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  size_t i = 0;
  if (length == 0) return false;
  size_t type_start = i;
  while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
  if (i == type_start || i >= length || value[i++] != '/') return false;
  size_t subtype_start = i;
  while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
  if (i == subtype_start) return false;
  while (i < length) {
    if (value[i++] != ';') return false;
    while (i < length && (value[i] == ' ' || value[i] == '\t')) i++;
    size_t key_start = i;
    while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
    if (i == key_start || i >= length || value[i++] != '=') return false;
    if (i >= length) return false;
    if (value[i] == '"') {
      i++;
      while (i < length && value[i] != '"') {
        if ((unsigned char)value[i] < 0x20 || value[i] == '\\') return false;
        i++;
      }
      if (i >= length || value[i++] != '"') return false;
    } else {
      size_t value_start = i;
      while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
      if (i == value_start) return false;
    }
    while (i < length && (value[i] == ' ' || value[i] == '\t')) i++;
  }
  return true;
}

const char *aura_mime_sanitize_filename(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if (length == 0 || (length == 1 && value[0] == '.') ||
      (length == 2 && value[0] == '.' && value[1] == '.')) return NULL;
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  size_t start = 0, out_length = 0;
  for (size_t i = 0; i <= length; i++) {
    if (i == length || value[i] == '/' || value[i] == '\\') {
      if (i > start) {
        if (out_length != 0) out[out_length++] = '_';
        for (size_t j = start; j < i; j++) {
          unsigned char c = (unsigned char)value[j];
          if (c < 0x20 || c == 0x7f) { free(out); return NULL; }
          out[out_length++] = (char)c;
        }
      }
      start = i + 1;
    }
  }
  if (out_length == 0) { free(out); return NULL; }
  out[out_length] = '\0';
  return out;
}

const char *aura_mime_disposition_filename(const char *value)
{
  if (value == NULL) return NULL;
  const char *cursor = value;
  while (*cursor != '\0') {
    while (*cursor == ';' || isspace((unsigned char)*cursor)) cursor++;
    const char *name = cursor;
    while (*cursor != '\0' && *cursor != '=' && *cursor != ';') cursor++;
    size_t name_length = (size_t)(cursor - name);
    while (name_length > 0 && isspace((unsigned char)name[name_length - 1])) name_length--;
    if (*cursor != '=') {
      while (*cursor != '\0' && *cursor != ';') cursor++;
      continue;
    }
    cursor++;
    while (isspace((unsigned char)*cursor)) cursor++;
    const char *raw = cursor;
    size_t raw_length = 0;
    if (*cursor == '"') {
      raw = ++cursor;
      while (*cursor != '\0' && *cursor != '"') cursor++;
      raw_length = (size_t)(cursor - raw);
      if (*cursor == '"') cursor++;
    } else {
      while (*cursor != '\0' && *cursor != ';') cursor++;
      raw_length = (size_t)(cursor - raw);
      while (raw_length > 0 && isspace((unsigned char)raw[raw_length - 1])) raw_length--;
    }
    if (name_length == 8) {
      static const char filename_name[] = "filename";
      int matches = 1;
      for (size_t i = 0; i < 8; i++) {
        unsigned char c = (unsigned char)name[i];
        if ((unsigned char)tolower(c) != (unsigned char)filename_name[i]) {
          matches = 0;
          break;
        }
      }
      if (matches) {
        char *raw_copy = aura_url_copy_range(raw, raw_length);
        const char *safe = aura_mime_sanitize_filename(raw_copy);
        free(raw_copy);
        return safe;
      }
    }
  }
  return NULL;
}

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
    aura_gc_mark_scan(n);
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
