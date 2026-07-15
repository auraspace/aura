/* Aura runtime — linked into every binary produced by aura build. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdbool.h>

void aura_println(const char *s) {
  if (s == NULL) {
    puts("null");
  } else {
    puts(s);
  }
}

/* Forward decls for throw (defined below) */
void aura_throw_string(const char *s);
void aura_throw_int(int64_t v);
void aura_throw_bool(bool v);

void aura_assert(bool cond) {
  if (!cond) {
    aura_throw_string("assertion failed");
  }
}

void aura_assert_eq_int(int64_t a, int64_t b) {
  if (a != b) {
    aura_throw_string("assert_eq failed (Int)");
  }
}

void aura_assert_eq_string(const char *a, const char *b) {
  if (a == NULL && b == NULL) {
    return;
  }
  if (a == NULL || b == NULL || strcmp(a, b) != 0) {
    aura_throw_string("assert_eq failed (String)");
  }
}

void aura_assert_eq_bool(bool a, bool b) {
  if (a != b) {
    aura_throw_string("assert_eq failed (Bool)");
  }
}

/* ---- Unchecked exceptions (setjmp / longjmp) ---- */

#define AURA_EX_MAX 64

typedef struct {
  jmp_buf *buf;
  const char *type_name; /* "String" | "Int" | "Bool" | class name */
  int owns_obj;          /* C3s: payload.as_obj is malloc'd by throw_obj */
  union {
    const char *as_string;
    int64_t as_int;
    bool as_bool;
    void *as_obj; /* heap copy of class/struct value (C3g) */
  } payload;
} AuraExFrame;

static AuraExFrame aura_ex_stack[AURA_EX_MAX];
static int aura_ex_sp = 0;
static int aura_ex_pending = 0;

void aura_try_enter(jmp_buf *buf) {
  if (aura_ex_sp >= AURA_EX_MAX) {
    fputs("aura: exception stack overflow\n", stderr);
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp++];
  f->buf = buf;
  f->type_name = NULL;
  f->owns_obj = 0;
  f->payload.as_obj = NULL;
}

void aura_try_leave(void) {
  if (aura_ex_sp > 0) {
    aura_ex_sp--;
  }
}

static void aura_throw_uncaught(const char *type_name) {
  fprintf(stderr, "uncaught exception (%s)\n", type_name ? type_name : "?");
  abort();
}

void aura_throw_string(const char *s) {
  if (aura_ex_sp == 0) {
    fprintf(stderr, "uncaught exception: %s\n", s ? s : "null");
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  f->type_name = "String";
  f->owns_obj = 0;
  f->payload.as_string = s;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

void aura_throw_int(int64_t v) {
  if (aura_ex_sp == 0) {
    fprintf(stderr, "uncaught exception: Int(%lld)\n", (long long)v);
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  f->type_name = "Int";
  f->owns_obj = 0;
  f->payload.as_int = v;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

void aura_throw_bool(bool v) {
  if (aura_ex_sp == 0) {
    fprintf(stderr, "uncaught exception: Bool(%s)\n", v ? "true" : "false");
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  f->type_name = "Bool";
  f->owns_obj = 0;
  f->payload.as_bool = v;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

/* Throw a class/struct instance. `obj` must be a heap pointer owned by the exception
 * machinery for the duration of unwind (typically malloc + copy in generated code).
 * Freed on aura_ex_clear after a successful catch (C3s). */
void aura_throw_obj(const char *type_name, void *obj) {
  if (aura_ex_sp == 0) {
    fprintf(stderr, "uncaught exception: %s\n", type_name ? type_name : "object");
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  f->type_name = type_name;
  f->owns_obj = 1;
  f->payload.as_obj = obj;
  aura_ex_pending = 1;
  longjmp(*f->buf, 1);
}

int aura_ex_matches(const char *type_name) {
  if (aura_ex_sp == 0 || !aura_ex_pending) {
    return 0;
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  return f->type_name && type_name && strcmp(f->type_name, type_name) == 0;
}

const char *aura_ex_as_string(void) {
  if (aura_ex_sp == 0) {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_string;
}

int64_t aura_ex_as_int(void) {
  if (aura_ex_sp == 0) {
    return 0;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_int;
}

bool aura_ex_as_bool(void) {
  if (aura_ex_sp == 0) {
    return false;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_bool;
}

void *aura_ex_as_obj(void) {
  if (aura_ex_sp == 0) {
    return NULL;
  }
  return aura_ex_stack[aura_ex_sp - 1].payload.as_obj;
}

void aura_ex_clear(void) {
  if (aura_ex_sp > 0) {
    AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
    /* Catch path copies by value first; free the throw heap copy (C3s). */
    if (f->owns_obj && f->payload.as_obj != NULL) {
      free(f->payload.as_obj);
      f->payload.as_obj = NULL;
    }
    f->owns_obj = 0;
    f->type_name = NULL;
  }
  aura_ex_pending = 0;
}

void aura_ex_rethrow(void) {
  if (!aura_ex_pending || aura_ex_sp == 0) {
    abort();
  }
  /* Pop current frame and longjmp to outer, or uncaught. */
  AuraExFrame cur = aura_ex_stack[aura_ex_sp - 1];
  aura_ex_sp--;
  if (aura_ex_sp == 0) {
    /* Process aborts; skip free (payload dies with process). */
    aura_throw_uncaught(cur.type_name);
  }
  AuraExFrame *outer = &aura_ex_stack[aura_ex_sp - 1];
  outer->type_name = cur.type_name;
  outer->owns_obj = cur.owns_obj;
  outer->payload = cur.payload;
  longjmp(*outer->buf, 1);
}

/* Provided by generated code */
int aura_main(void);

int main(int argc, char **argv) {
  (void)argc;
  (void)argv;
  return aura_main();
}
