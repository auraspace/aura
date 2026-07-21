/* Aura runtime — linked into every binary produced by aura build. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <setjmp.h>
#include <stdint.h>
#include <stdbool.h>
#include <errno.h>
#include <sys/stat.h>

/* Forward decls for throw (defined below) */
void aura_throw_string(const char *s);
void aura_throw_int(int64_t v);
void aura_throw_bool(bool v);

/* ---- Console I/O ---- */

void aura_print(const char *s)
{
  if (s == NULL)
  {
    fputs("null", stdout);
  }
  else
  {
    fputs(s, stdout);
  }
  fflush(stdout);
}

void aura_println(const char *s)
{
  if (s == NULL)
  {
    puts("null");
  }
  else
  {
    puts(s);
  }
}

void aura_eprint(const char *s)
{
  if (s == NULL)
  {
    fputs("null", stderr);
  }
  else
  {
    fputs(s, stderr);
  }
  fflush(stderr);
}

void aura_eprintln(const char *s)
{
  if (s == NULL)
  {
    fputs("null\n", stderr);
  }
  else
  {
    fputs(s, stderr);
    fputc('\n', stderr);
  }
  fflush(stderr);
}

/* ---- File I/O (std.io) ----
 * Text errors throw String messages (single-threaded; static errbuf).
 * Strings are UTF-8 byte sequences; binary with embedded NUL is not supported
 * for the String path (matches the rest of the String surface).
 */

#define AURA_IO_MAX_FILE ((int64_t)256 * 1024 * 1024)

static char aura_io_errbuf[1024];

static void aura_io_throw(const char *op, const char *path)
{
  const char *p = path ? path : "(null)";
  const char *err = strerror(errno);
  if (err == NULL)
  {
    err = "unknown error";
  }
  snprintf(aura_io_errbuf, sizeof(aura_io_errbuf), "io %s failed: %s: %s", op, p, err);
  aura_throw_string(aura_io_errbuf);
}

static void aura_io_throw_msg(const char *msg)
{
  snprintf(aura_io_errbuf, sizeof(aura_io_errbuf), "%s", msg ? msg : "io error");
  aura_throw_string(aura_io_errbuf);
}

bool aura_file_exists(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    return false;
  }
  struct stat st;
  return stat(path, &st) == 0 && S_ISREG(st.st_mode);
}

int64_t aura_file_size(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    errno = EINVAL;
    aura_io_throw("file_size", path);
  }
  struct stat st;
  if (stat(path, &st) != 0)
  {
    aura_io_throw("file_size", path);
  }
  if (!S_ISREG(st.st_mode))
  {
    errno = EISDIR;
    aura_io_throw("file_size", path);
  }
  return (int64_t)st.st_size;
}

const char *aura_read_file(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    errno = EINVAL;
    aura_io_throw("read_file", path);
  }
  FILE *f = fopen(path, "rb");
  if (f == NULL)
  {
    aura_io_throw("read_file", path);
  }
  if (fseek(f, 0, SEEK_END) != 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  long end = ftell(f);
  if (end < 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  if ((int64_t)end > AURA_IO_MAX_FILE)
  {
    fclose(f);
    aura_io_throw_msg("io read_file failed: file exceeds 256 MiB limit");
  }
  if (fseek(f, 0, SEEK_SET) != 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  size_t n = (size_t)end;
  char *buf = (char *)malloc(n + 1);
  if (buf == NULL)
  {
    fclose(f);
    aura_io_throw_msg("io read_file failed: out of memory");
  }
  size_t got = fread(buf, 1, n, f);
  if (got != n)
  {
    int e = ferror(f) ? errno : EIO;
    free(buf);
    fclose(f);
    errno = e;
    aura_io_throw("read_file", path);
  }
  fclose(f);
  buf[n] = '\0';
  if (memchr(buf, '\0', n) != NULL)
  {
    free(buf);
    aura_io_throw_msg("io read_file failed: file contains embedded NUL (not a String)");
  }
  return buf;
}

/* C12p: soft read — same constraints as aura_read_file, but returns NULL on
 * missing path / I/O error / oversize / OOM / embedded NUL (never throws). */
const char *aura_try_read_file(const char *path)
{
  if (path == NULL || path[0] == '\0')
  {
    return NULL;
  }
  FILE *f = fopen(path, "rb");
  if (f == NULL)
  {
    return NULL;
  }
  if (fseek(f, 0, SEEK_END) != 0)
  {
    fclose(f);
    return NULL;
  }
  long end = ftell(f);
  if (end < 0)
  {
    fclose(f);
    return NULL;
  }
  if ((int64_t)end > AURA_IO_MAX_FILE)
  {
    fclose(f);
    return NULL;
  }
  if (fseek(f, 0, SEEK_SET) != 0)
  {
    fclose(f);
    return NULL;
  }
  size_t n = (size_t)end;
  char *buf = (char *)malloc(n + 1);
  if (buf == NULL)
  {
    fclose(f);
    return NULL;
  }
  size_t got = fread(buf, 1, n, f);
  if (got != n)
  {
    free(buf);
    fclose(f);
    return NULL;
  }
  fclose(f);
  buf[n] = '\0';
  if (memchr(buf, '\0', n) != NULL)
  {
    free(buf);
    return NULL;
  }
  return buf;
}

static void aura_write_file_mode(const char *path, const char *content, const char *mode, const char *op)
{
  if (path == NULL || path[0] == '\0')
  {
    errno = EINVAL;
    aura_io_throw(op, path);
  }
  FILE *f = fopen(path, mode);
  if (f == NULL)
  {
    aura_io_throw(op, path);
  }
  const char *s = content ? content : "";
  size_t n = strlen(s);
  if (n > 0)
  {
    size_t wrote = fwrite(s, 1, n, f);
    if (wrote != n)
    {
      int e = errno;
      fclose(f);
      errno = e;
      aura_io_throw(op, path);
    }
  }
  if (fflush(f) != 0)
  {
    int e = errno;
    fclose(f);
    errno = e;
    aura_io_throw(op, path);
  }
  if (fclose(f) != 0)
  {
    aura_io_throw(op, path);
  }
}

void aura_write_file(const char *path, const char *content)
{
  aura_write_file_mode(path, content, "wb", "write_file");
}

void aura_append_file(const char *path, const char *content)
{
  aura_write_file_mode(path, content, "ab", "append_file");
}

/* ---- Stdin (std.io.readLine / readAllStdin) ----
 * readLine: one line without trailing \n or \r\n; NULL on EOF; empty line is "".
 * Oversized line / whole-stdin throws String (MVP caps).
 */

#define AURA_IO_MAX_LINE ((int64_t)1 * 1024 * 1024)

const char *aura_read_line(void)
{
  size_t cap = 128;
  size_t n = 0;
  char *buf = (char *)malloc(cap);
  if (buf == NULL)
  {
    aura_io_throw_msg("io read_line failed: out of memory");
  }
  int c = EOF;
  for (;;)
  {
    c = fgetc(stdin);
    if (c == EOF)
    {
      break;
    }
    if (c == '\n')
    {
      break;
    }
    /* Treat \r or \r\n as end of line (strip CR). */
    if (c == '\r')
    {
      int next = fgetc(stdin);
      if (next != '\n' && next != EOF)
      {
        ungetc(next, stdin);
      }
      break;
    }
    if ((int64_t)n >= AURA_IO_MAX_LINE)
    {
      free(buf);
      aura_io_throw_msg("io read_line failed: line exceeds 1 MiB limit");
    }
    if (n + 1 >= cap)
    {
      size_t ncap = cap * 2;
      if ((int64_t)ncap > AURA_IO_MAX_LINE + 1)
      {
        ncap = (size_t)AURA_IO_MAX_LINE + 1;
      }
      char *nb = (char *)realloc(buf, ncap);
      if (nb == NULL)
      {
        free(buf);
        aura_io_throw_msg("io read_line failed: out of memory");
      }
      buf = nb;
      cap = ncap;
    }
    buf[n++] = (char)c;
  }
  if (ferror(stdin))
  {
    free(buf);
    aura_io_throw_msg("io read_line failed: stdin read error");
  }
  /* Immediate EOF with no bytes → null (String?). */
  if (c == EOF && n == 0)
  {
    free(buf);
    return NULL;
  }
  buf[n] = '\0';
  return buf;
}

const char *aura_read_all_stdin(void)
{
  size_t cap = 4096;
  size_t n = 0;
  char *buf = (char *)malloc(cap);
  if (buf == NULL)
  {
    aura_io_throw_msg("io read_all_stdin failed: out of memory");
  }
  for (;;)
  {
    if (n + 1 >= cap)
    {
      if ((int64_t)cap >= AURA_IO_MAX_FILE)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
      }
      size_t ncap = cap * 2;
      if ((int64_t)ncap > AURA_IO_MAX_FILE)
      {
        ncap = (size_t)AURA_IO_MAX_FILE;
      }
      if (ncap <= cap)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
      }
      char *nb = (char *)realloc(buf, ncap);
      if (nb == NULL)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: out of memory");
      }
      buf = nb;
      cap = ncap;
    }
    size_t want = cap - n - 1; /* leave room for NUL */
    if (want == 0)
    {
      free(buf);
      aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
    }
    size_t got = fread(buf + n, 1, want, stdin);
    n += got;
    if (got < want)
    {
      if (ferror(stdin))
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: stdin read error");
      }
      break; /* EOF */
    }
    if ((int64_t)n >= AURA_IO_MAX_FILE)
    {
      int extra = fgetc(stdin);
      if (extra != EOF)
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: input exceeds 256 MiB limit");
      }
      if (ferror(stdin))
      {
        free(buf);
        aura_io_throw_msg("io read_all_stdin failed: stdin read error");
      }
      break;
    }
  }
  if (memchr(buf, '\0', n) != NULL)
  {
    free(buf);
    aura_io_throw_msg("io read_all_stdin failed: input contains embedded NUL (not a String)");
  }
  buf[n] = '\0';
  return buf;
}

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

typedef struct
{
  jmp_buf *buf;
  const char *type_name; /* "String" | "Int" | "Bool" | class name */
  int owns_obj;          /* C3s: payload.as_obj is malloc'd by throw_obj */
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

void aura_try_enter(jmp_buf *buf)
{
  if (aura_ex_sp >= AURA_EX_MAX)
  {
    fputs("aura: exception stack overflow\n", stderr);
    abort();
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp++];
  f->buf = buf;
  f->type_name = NULL;
  f->owns_obj = 0;
  f->payload.as_obj = NULL;
}

void aura_try_leave(void)
{
  if (aura_ex_sp > 0)
  {
    aura_ex_sp--;
  }
}

static void aura_throw_uncaught(const char *type_name)
{
  fprintf(stderr, "uncaught exception (%s)\n", type_name ? type_name : "?");
  abort();
}

void aura_throw_string(const char *s)
{
  if (aura_ex_sp == 0)
  {
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

void aura_throw_int(int64_t v)
{
  if (aura_ex_sp == 0)
  {
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

void aura_throw_bool(bool v)
{
  if (aura_ex_sp == 0)
  {
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
void aura_throw_obj(const char *type_name, void *obj)
{
  if (aura_ex_sp == 0)
  {
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

int aura_ex_matches(const char *type_name)
{
  if (aura_ex_sp == 0 || !aura_ex_pending)
  {
    return 0;
  }
  AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
  return f->type_name && type_name && strcmp(f->type_name, type_name) == 0;
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

void aura_ex_clear(void)
{
  if (aura_ex_sp > 0)
  {
    AuraExFrame *f = &aura_ex_stack[aura_ex_sp - 1];
    /* Catch path copies by value first; free the throw heap copy (C3s). */
    if (f->owns_obj && f->payload.as_obj != NULL)
    {
      free(f->payload.as_obj);
      f->payload.as_obj = NULL;
    }
    f->owns_obj = 0;
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
  /* Pop current frame and longjmp to outer, or uncaught. */
  AuraExFrame cur = aura_ex_stack[aura_ex_sp - 1];
  aura_ex_sp--;
  if (aura_ex_sp == 0)
  {
    /* Process aborts; skip free (payload dies with process). */
    aura_throw_uncaught(cur.type_name);
  }
  AuraExFrame *outer = &aura_ex_stack[aura_ex_sp - 1];
  outer->type_name = cur.type_name;
  outer->owns_obj = cur.owns_obj;
  outer->payload = cur.payload;
  longjmp(*outer->buf, 1);
}

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
  if (slot == NULL)
  {
    return;
  }
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    if (aura_gc_roots[i] == slot)
    {
      return;
    }
  }
  if (aura_gc_root_n >= AURA_GC_MAX_ROOTS)
  {
    fputs("aura: GC root table full\n", stderr);
    abort();
  }
  aura_gc_roots[aura_gc_root_n++] = slot;
}

void aura_gc_remove_root(void **slot)
{
  if (slot == NULL)
  {
    return;
  }
  for (int i = 0; i < aura_gc_root_n; i++)
  {
    if (aura_gc_roots[i] == slot)
    {
      aura_gc_roots[i] = aura_gc_roots[aura_gc_root_n - 1];
      aura_gc_root_n--;
      return;
    }
  }
}

/* C6e: register Array.data / Array.len so collect marks element GC pointers. */
void aura_gc_add_array_root(void **data_slot, int64_t *len_slot)
{
  if (data_slot == NULL || len_slot == NULL)
  {
    return;
  }
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    if (aura_gc_array_roots[i].data_slot == data_slot)
    {
      aura_gc_array_roots[i].len_slot = len_slot;
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
  aura_gc_array_root_n++;
}

void aura_gc_remove_array_root(void **data_slot)
{
  if (data_slot == NULL)
  {
    return;
  }
  for (int i = 0; i < aura_gc_array_root_n; i++)
  {
    if (aura_gc_array_roots[i].data_slot == data_slot)
    {
      aura_gc_array_roots[i] = aura_gc_array_roots[aura_gc_array_root_n - 1];
      aura_gc_array_root_n--;
      return;
    }
  }
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
  return p;
}

void *aura_gc_alloc(size_t size)
{
  return aura_gc_alloc_full(size, NULL, NULL);
}

/* C7b: mark a GC object pointer (for generated mark_extras on Array fields). */
void aura_gc_mark_ptr(void *obj)
{
  if (obj == NULL)
  {
    return;
  }
  AuraGcNode *n = aura_gc_find(obj);
  if (n != NULL)
  {
    aura_gc_mark_push(n);
  }
}

/* C12k/C12l: free a Fun capture env. Every capturing env starts with a drop
 * fn ptr that unregisters GC roots for class capture slots, then free(env).
 * Array capture slots are non-owning header views — drop must not free buffers.
 * C12m: by-ref Int/Bool captures release their shared boxes in drop. */
void aura_fun_env_free(void *env)
{
  if (env == NULL)
  {
    return;
  }
  void (*drop)(void *) = *(void (**)(void *))env;
  if (drop != NULL)
  {
    drop(env);
  }
  else
  {
    free(env);
  }
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

/* C4z/C5f/C6a/C6e: stop-the-world deep mark + sweep when roots are registered. */
void aura_gc_collect(void)
{
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
    void **elems = (void **)*data_slot;
    int64_t len = *len_slot;
    if (elems == NULL || len <= 0)
    {
      continue;
    }
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
}

void aura_gc_shutdown(void)
{
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
}

/* ---- Process argv (std.io.args) ----
 * Stashed from C main before aura_main so user programs keep fun main().
 * argv string pointers are process-lifetime; Array of them does not free chars.
 */

static int aura_saved_argc = 0;
static char **aura_saved_argv = NULL;

void aura_set_args(int argc, char **argv)
{
  aura_saved_argc = argc > 0 ? argc : 0;
  aura_saved_argv = argv;
}

int64_t aura_args_count(void)
{
  return (int64_t)aura_saved_argc;
}

const char *aura_args_get(int64_t i)
{
  if (i < 0 || i >= (int64_t)aura_saved_argc || aura_saved_argv == NULL)
  {
    aura_throw_string("args index out of bounds");
  }
  const char *s = aura_saved_argv[i];
  return s != NULL ? s : "";
}

/* ---- Process exit (std.io.exit) ----
 * Flush stdio, then terminate with the given status (truncated to int).
 * Does not return. Prefer exit over _Exit so atexit/flush run.
 */

void aura_exit(int64_t code)
{
  fflush(stdout);
  fflush(stderr);
  exit((int)code);
}

/* Provided by generated code */
int aura_main(void);

int main(int argc, char **argv)
{
  aura_set_args(argc, argv);
  int rc = aura_main();
  aura_gc_shutdown();
  return rc;
}
