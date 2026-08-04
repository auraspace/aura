void aura_assert(bool cond)
{
  if (!cond)
  {
    aura_throw_string("assertion failed");
  }
}

void aura_assert_eq_int(int64_t a, int64_t b)
{
  if (a != b)
  {
    aura_throw_string("assert_eq failed (Int)");
  }
}

void aura_assert_eq_string(const char *a, const char *b)
{
  if (a == NULL && b == NULL)
  {
    return;
  }
  if (a == NULL || b == NULL || strcmp(a, b) != 0)
  {
    aura_throw_string("assert_eq failed (String)");
  }
}

void aura_assert_eq_bool(bool a, bool b)
{
  if (a != b)
  {
    aura_throw_string("assert_eq failed (Bool)");
  }
}

/* ---- Unchecked exceptions (setjmp / longjmp) ---- */

#define AURA_EX_MAX 64

typedef struct AuraExCause AuraExCause;

struct AuraExCause
{
  char *type_name;
  uint32_t source_span_start;
  uint32_t source_span_end;
  AuraExCause *next;
};

typedef struct
{
  jmp_buf *buf;
  const char *type_name; /* "String" | "Int" | "Bool" | class name */
  uint32_t source_span_start;
  uint32_t source_span_end;
  int owns_obj;          /* payload.as_obj is owned by the exception frame */
  void (*destroy_obj)(void *);
  AuraExCause *cause_head;
  AuraExCause *cause_tail;
  union
  {
    const char *as_string;
    int64_t as_int;
    bool as_bool;
    void *as_obj; /* heap copy of class/struct value (C3g) */
  } payload;
} AuraExFrame;

static AuraExFrame aura_ex_stack[AURA_EX_MAX];
static int aura_ex_sp = 0;
static int aura_ex_pending = 0;
static uint32_t aura_ex_unhandled_span_start = 0;
static uint32_t aura_ex_unhandled_span_end = 0;
static AuraExCause *aura_ex_cleared_causes_head = NULL;
static AuraExCause *aura_ex_cleared_causes_tail = NULL;
static uint32_t aura_ex_cleared_span_start = 0;
static uint32_t aura_ex_cleared_span_end = 0;

static char *aura_ex_copy_string(const char *text)
{
  size_t len;
  char *copy;
  if (text == NULL)
  {
    return NULL;
  }
  len = strlen(text);
  copy = (char *)malloc(len + 1);
  if (copy != NULL)
  {
    memcpy(copy, text, len + 1);
  }
  return copy;
}

static void aura_ex_dispose_causes(AuraExFrame *f)
{
  AuraExCause *cause;
  AuraExCause *next;
  if (f == NULL)
  {
    return;
  }
  cause = f->cause_head;
  f->cause_head = NULL;
  f->cause_tail = NULL;
  while (cause != NULL)
  {
    next = cause->next;
    free(cause->type_name);
    free(cause);
    cause = next;
  }
}

static void aura_ex_dispose_cleared_causes(void)
{
  AuraExCause *cause = aura_ex_cleared_causes_head;
  aura_ex_cleared_causes_head = NULL;
  aura_ex_cleared_causes_tail = NULL;
  while (cause != NULL)
  {
    AuraExCause *next = cause->next;
    free(cause->type_name);
    free(cause);
    cause = next;
  }
}

static AuraExCause *aura_ex_query_causes(void)
{
  if (aura_ex_sp > 0 && aura_ex_stack[aura_ex_sp - 1].cause_head != NULL)
  {
    return aura_ex_stack[aura_ex_sp - 1].cause_head;
  }
  return aura_ex_cleared_causes_head;
}

/* Compiler-generated throws set this before transferring control. Runtime
 * helpers leave it at zero, preserving a stable unknown location. */
void aura_ex_set_source_span(uint32_t start, uint32_t end)
{
  aura_ex_unhandled_span_start = start;
  aura_ex_unhandled_span_end = end;
  if (aura_ex_sp > 0)
  {
    aura_ex_stack[aura_ex_sp - 1].source_span_start = start;
    aura_ex_stack[aura_ex_sp - 1].source_span_end = end;
  }
}

uint32_t aura_ex_source_span_start(void)
{
  if (aura_ex_sp > 0 && aura_ex_stack[aura_ex_sp - 1].source_span_end != 0)
  {
    return aura_ex_stack[aura_ex_sp - 1].source_span_start;
  }
  return aura_ex_cleared_span_start;
}

uint32_t aura_ex_source_span_end(void)
{
  if (aura_ex_sp > 0 && aura_ex_stack[aura_ex_sp - 1].source_span_end != 0)
  {
    return aura_ex_stack[aura_ex_sp - 1].source_span_end;
  }
  return aura_ex_cleared_span_end;
}

void aura_throw_obj_with_destructor(const char *type_name, void *obj,
                                    void (*destroy_obj)(void *));

static void aura_ex_dispose_frame(AuraExFrame *f)
{
  if (f == NULL)
  {
    return;
  }
  if (f->owns_obj && f->payload.as_obj != NULL)
  {
    if (f->destroy_obj != NULL)
    {
      f->destroy_obj(f->payload.as_obj);
    }
    else
    {
      free(f->payload.as_obj);
    }
    f->payload.as_obj = NULL;
  }
  aura_ex_dispose_causes(f);
  f->owns_obj = 0;
  f->destroy_obj = NULL;
  f->type_name = NULL;
}

/* An uncaught object still owns its payload until the process terminates.
 * Dispose it before aborting so custom destructors release nested resources
 * even when there is no catch frame to perform the final cleanup. */
static void aura_ex_abort_uncaught(const char *type_name, void *obj,
                                   void (*destroy_obj)(void *),
                                   uint32_t source_span_start,
                                   uint32_t source_span_end,
                                   AuraExCause *causes)
{
  if (source_span_end > source_span_start)
  {
    fprintf(stderr, "uncaught exception (%s) at source span [%u,%u)\n",
            type_name ? type_name : "object", source_span_start,
            source_span_end);
  }
  else
  {
    fprintf(stderr, "uncaught exception (%s)\n",
            type_name ? type_name : "object");
  }
  for (AuraExCause *cause = causes; cause != NULL; cause = cause->next)
  {
    if (cause->source_span_end > cause->source_span_start)
    {
      fprintf(stderr, "  caused by (%s) at source span [%u,%u)\n",
              cause->type_name ? cause->type_name : "exception",
              cause->source_span_start, cause->source_span_end);
    }
    else
    {
      fprintf(stderr, "  caused by (%s)\n",
              cause->type_name ? cause->type_name : "exception");
    }
  }
  if (obj != NULL)
  {
    if (destroy_obj != NULL)
    {
      destroy_obj(obj);
    }
    else
    {
      free(obj);
    }
  }
  while (causes != NULL)
  {
    AuraExCause *next = causes->next;
    free(causes->type_name);
    free(causes);
    causes = next;
  }
  abort();
}

static void aura_ex_replace_payload(AuraExFrame *f)
{
  aura_ex_dispose_frame(f);
  if (f != NULL)
  {
    f->payload.as_obj = NULL;
  }
}

void aura_try_enter(jmp_buf *buf)
{
  if (aura_ex_sp >= AURA_EX_MAX)
  {
    fputs("aura: exception stack overflow\n", stderr);
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp++];
  aura_ex_dispose_cleared_causes();
  aura_ex_cleared_span_start = 0;
  aura_ex_cleared_span_end = 0;
  f->buf = buf;
  f->type_name = NULL;
  f->source_span_start = 0;
  f->source_span_end = 0;
  f->cause_head = NULL;
  f->cause_tail = NULL;
  f->owns_obj = 0;
  f->destroy_obj = NULL;
  f->payload.as_obj = NULL;
}

void aura_try_leave(void)
{
  if (aura_ex_sp > 0)
  {
    /* Leaving a catch is the final ownership boundary when the caller did
     * not explicitly clear the payload. Generated catches still clear first. */
    aura_ex_dispose_frame(&aura_ex_stack[aura_ex_sp - 1]);
    aura_ex_sp--;
    if (aura_ex_sp == 0)
    {
      aura_ex_pending = 0;
    }
  }
}

void aura_throw_string(const char *s)
{
  if (aura_ex_sp == 0)
  {
    fprintf(stderr, "uncaught exception: %s\n", s ? s : "null");
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = "String";
  f->owns_obj = 0;
  f->payload.as_string = s;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

void aura_throw_int(int64_t v)
{
  if (aura_ex_sp == 0)
  {
    fprintf(stderr, "uncaught exception: Int(%lld)\n", (long long)v);
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = "Int";
  f->owns_obj = 0;
  f->payload.as_int = v;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

void aura_throw_bool(bool v)
{
  if (aura_ex_sp == 0)
  {
    fprintf(stderr, "uncaught exception: Bool(%s)\n", v ? "true" : "false");
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = "Bool";
  f->owns_obj = 0;
  f->payload.as_bool = v;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

/* Throw a class/struct instance with the legacy malloc ownership contract. */
void aura_throw_obj(const char *type_name, void *obj)
{
  aura_throw_obj_with_destructor(type_name, obj, free);
}

/* Throw a class/struct instance and transfer its complete ownership to the
 * exception frame.  The destructor is invoked exactly once by clear, the
 * final try_leave, or after ownership is transferred by rethrow.  This is
 * required for payloads containing owned runtime resources (for example a
 * heap-backed String field), where a shallow free(obj) is insufficient. */
void aura_throw_obj_with_destructor(const char *type_name, void *obj,
                                    void (*destroy_obj)(void *))
{
  if (aura_ex_sp == 0)
  {
    aura_ex_abort_uncaught(type_name, obj,
                           destroy_obj != NULL ? destroy_obj : free,
                           aura_ex_unhandled_span_start,
                           aura_ex_unhandled_span_end, NULL);
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(f);
  f->type_name = type_name;
  f->owns_obj = 1;
  f->destroy_obj = destroy_obj != NULL ? destroy_obj : free;
  f->payload.as_obj = obj;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

int aura_ex_add_cause(const char *type_name, uint32_t source_span_start,
                      uint32_t source_span_end)
{
  AuraExCause *cause;
  AuraExCause **head;
  AuraExCause **tail;
  if (type_name == NULL)
  {
    return 0;
  }
  cause = (AuraExCause *)calloc(1, sizeof(*cause));
  if (cause == NULL)
  {
    return 0;
  }
  cause->type_name = aura_ex_copy_string(type_name);
  if (cause->type_name == NULL)
  {
    free(cause);
    return 0;
  }
  cause->source_span_start = source_span_start;
  cause->source_span_end = source_span_end;
  if (aura_ex_sp > 0)
  {
    head = &aura_ex_stack[aura_ex_sp - 1].cause_head;
    tail = &aura_ex_stack[aura_ex_sp - 1].cause_tail;
  }
  else
  {
    head = &aura_ex_cleared_causes_head;
    tail = &aura_ex_cleared_causes_tail;
  }
  if (*tail == NULL)
  {
    *head = cause;
  }
  else
  {
    (*tail)->next = cause;
  }
  *tail = cause;
  return 1;
}

size_t aura_ex_cause_count(void)
{
  size_t count = 0;
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      count++;
    }
  }
  return count;
}

const char *aura_ex_cause_type(size_t index)
{
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    size_t current = 0;
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      if (current++ == index)
      {
        return cause->type_name;
      }
    }
  }
  return NULL;
}

uint32_t aura_ex_cause_span_start(size_t index)
{
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    size_t current = 0;
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      if (current++ == index)
      {
        return cause->source_span_start;
      }
    }
  }
  return 0;
}

const char *aura_ex_cause_type_copy(size_t index)
{
  return aura_ex_copy_string(aura_ex_cause_type(index));
}

uint32_t aura_ex_cause_span_end(size_t index)
{
  AuraExCause *head = aura_ex_query_causes();
  if (head != NULL)
  {
    size_t current = 0;
    for (AuraExCause *cause = head; cause != NULL; cause = cause->next)
    {
      if (current++ == index)
      {
        return cause->source_span_end;
      }
    }
  }
  return 0;
}

int aura_ex_matches(const char *type_name)
{
  if (aura_ex_sp == 0 || !aura_ex_pending)
  {
    return 0;
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  return f->type_name && type_name && strcmp(f->type_name, type_name) == 0;
}

const char *aura_ex_type_name(void)
{
  if (aura_ex_sp == 0 || !aura_ex_pending)
  {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].type_name;
}

const char *aura_ex_as_string(void)
{
  if (aura_ex_sp == 0)
  {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_string;
}

int64_t aura_ex_as_int(void)
{
  if (aura_ex_sp == 0)
  {
    return 0;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_int;
}

bool aura_ex_as_bool(void)
{
  if (aura_ex_sp == 0)
  {
    return false;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_bool;
}

void *aura_ex_as_obj(void)
{
  if (aura_ex_sp == 0)
  {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_obj;
}

/* Transfer an object payload out of the active exception frame. */
void *aura_ex_take_obj(void)
{
  AuraExFrame *f;
  void *obj;

  if (aura_ex_sp == 0 || !aura_ex_pending)
  {
    return NULL;
  }
  f = &aura_ex_stack[aura_ex_sp - 1];
  if (!f->owns_obj)
  {
    return NULL;
  }
  obj = f->payload.as_obj;
  f->payload.as_obj = NULL;
  f->owns_obj = 0;
  f->destroy_obj = NULL;
  return obj;
}

void aura_ex_clear(void)
{
  if (aura_ex_sp > 0)
  {
    AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
    /* Keep causes queryable for the catch body after its frame is left. */
    aura_ex_dispose_cleared_causes();
    aura_ex_cleared_span_start = f->source_span_start;
    aura_ex_cleared_span_end = f->source_span_end;
    aura_ex_cleared_causes_head = f->cause_head;
    aura_ex_cleared_causes_tail = f->cause_tail;
    f->cause_head = NULL;
    f->cause_tail = NULL;
    if (f->owns_obj && f->payload.as_obj != NULL)
    {
      if (f->destroy_obj != NULL)
      {
        f->destroy_obj(f->payload.as_obj);
      }
      else
      {
        free(f->payload.as_obj);
      }
      f->payload.as_obj = NULL;
    }
    f->owns_obj = 0;
    f->destroy_obj = NULL;
    f->type_name = NULL;
  }
  aura_ex_pending = 0;
}

void aura_ex_rethrow(void)
{
  if (!aura_ex_pending || aura_ex_sp == 0)
  {
    abort();
  }
  aura_ex_dispose_cleared_causes();
  /* Pop current frame and longjmp to outer, or uncaught. */
  AuraExFrame cur = aura_ex_stack[aura_ex_sp - 1];
  aura_ex_sp--;
  if (aura_ex_sp == 0)
  {
    aura_ex_abort_uncaught(cur.type_name,
                           cur.owns_obj ? cur.payload.as_obj : NULL,
                           cur.owns_obj ? cur.destroy_obj : NULL,
                           cur.source_span_start, cur.source_span_end,
                           cur.cause_head);
  }
  AuraExFrame *outer = &aura_ex_stack[aura_ex_sp - 1];
  aura_ex_replace_payload(outer);
  outer->type_name = cur.type_name;
  outer->source_span_start = cur.source_span_start;
  outer->source_span_end = cur.source_span_end;
  outer->owns_obj = cur.owns_obj;
  outer->destroy_obj = cur.destroy_obj;
  outer->payload = cur.payload;
  outer->cause_head = cur.cause_head;
  outer->cause_tail = cur.cause_tail;
  cur.cause_head = NULL;
  cur.cause_tail = NULL;
  longjmp(*outer->buf, 1);
}

