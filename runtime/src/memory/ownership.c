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
