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

/* Structured std.io wrappers use heap-owned String payloads for errors.  Keep
 * this helper separate from the throwing path: the throw path borrows the
 * static buffer above, while Result errors must survive the call boundary. */
char *aura_io_owned_error(const char *op, const char *path)
{
  const char *safe_op = op ? op : "io";
  const char *safe_path = path ? path : "(null)";
  const char *err = strerror(errno);
  if (err == NULL)
  {
    err = "unknown error";
  }
  int needed = snprintf(NULL, 0, "io %s failed: %s: %s", safe_op, safe_path, err);
  if (needed < 0)
  {
    return NULL;
  }
  char *message = (char *)malloc((size_t)needed + 1);
  if (message == NULL)
  {
    return NULL;
  }
  snprintf(message, (size_t)needed + 1, "io %s failed: %s: %s", safe_op, safe_path, err);
  return message;
}

void aura_io_owned_error_free(char *message)
{
  free(message);
}

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

/* Soft write: true on success; false on empty path / open / write / flush / close fail.
 * Does not throw (unlike aura_write_file). */
bool aura_try_write_file(const char *path, const char *content)
{
  if (path == NULL || path[0] == '\0')
  {
    return false;
  }
  FILE *f = fopen(path, "wb");
  if (f == NULL)
  {
    return false;
  }
  const char *s = content ? content : "";
  size_t n = strlen(s);
  if (n > 0)
  {
    size_t wrote = fwrite(s, 1, n, f);
    if (wrote != n)
    {
      fclose(f);
      return false;
    }
  }
  if (fflush(f) != 0)
  {
    fclose(f);
    return false;
  }
  if (fclose(f) != 0)
  {
    return false;
  }
  return true;
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

/* C12k/C12l/C13e: Fun capture env header (must match codegen layout).
 * Layout of every capturing env:
 *   void (*__drop)(void *);
 *   int32_t __refs;
 *   … capture slots (class GC roots, boxes, nested Fun fat pointers, …)
 * Array capture slots are non-owning header views — drop must not free buffers.
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

/* ---- C22j task-frame ABI (single-threaded MVP) ----
 *
 * A task frame is an opaque, heap-owned state machine object.  The poll
 * callback owns the state transition; it may retain frame_data across a
 * pending return.  A frame owns its result payload and invokes result_destroy
 * exactly once when the frame is destroyed.  The optional frame_destroy hook
 * runs before frame_data is freed and is the place for state-machine-specific
 * cleanup.  The context pointer is borrowed by the runtime and is never
 * freed.
 *
 * This ABI deliberately has no scheduler or channel dependency.  C22k adds
 * the executor that drives these callbacks.
 */

#define AURA_RT_ABI_VERSION 1u
#define AURA_RT_ABI_ID "aura-c-abi/1.0;task=1;value=1;exception=1;channel=1;gc=1;io=1;ffi=1"

uint32_t aura_runtime_abi_version(void)
{
  return AURA_RT_ABI_VERSION;
}

const char *aura_runtime_abi_identity(void)
{
  return AURA_RT_ABI_ID;
}

int aura_runtime_check_abi(uint32_t expected_version, const char *expected_identity)
{
  const char *available = aura_runtime_abi_identity();
  if (expected_version == aura_runtime_abi_version() &&
      expected_identity != NULL && strcmp(expected_identity, available) == 0)
  {
    return 1;
  }
  fprintf(stderr,
          "aura: runtime ABI mismatch: expected version %u identity %s, available version %u identity %s\n",
          expected_version,
          expected_identity ? expected_identity : "(missing)",
          aura_runtime_abi_version(),
          available);
  return 0;
}

/* ---- R1/R2 deterministic race event model ----
 *
 * The current executor is single-threaded, so this tracker records the total
 * order that a future concurrent detector will refine into vector clocks.
 * Every event carries task, address, and source identity for stable reports.
 */
typedef enum
{
  AURA_RACE_READ = 0,
  AURA_RACE_WRITE = 1,
  AURA_RACE_TASK_SPAWN = 2,
  AURA_RACE_TASK_JOIN = 3,
  AURA_RACE_SYNC_ACQUIRE = 4,
  AURA_RACE_SYNC_RELEASE = 5
} AuraRaceEventKind;

typedef struct
{
  uint64_t sequence;
  uint64_t task_id;
  uintptr_t address;
  uint32_t source_id;
  AuraRaceEventKind kind;
} AuraRaceEvent;

typedef struct
{
  AuraRaceEvent *events;
  size_t count;
  size_t capacity;
  uint64_t clock;
} AuraRaceTracker;

AuraRaceTracker *aura_race_tracker_new(void)
{
  AuraRaceTracker *tracker = (AuraRaceTracker *)calloc(1, sizeof(*tracker));
  if (tracker == NULL)
  {
    return NULL;
  }
  tracker->capacity = 16;
  tracker->events = (AuraRaceEvent *)calloc(tracker->capacity, sizeof(*tracker->events));
  if (tracker->events == NULL)
  {
    free(tracker);
    return NULL;
  }
  return tracker;
}

void aura_race_tracker_destroy(AuraRaceTracker *tracker)
{
  if (tracker != NULL)
  {
    free(tracker->events);
    free(tracker);
  }
}

void aura_race_tracker_reset(AuraRaceTracker *tracker)
{
  if (tracker != NULL)
  {
    tracker->count = 0;
    tracker->clock = 0;
  }
}

int aura_race_tracker_record(AuraRaceTracker *tracker,
                             uint64_t task_id,
                             uintptr_t address,
                             uint32_t source_id,
                             AuraRaceEventKind kind,
                             AuraRaceEvent *out)
{
  if (tracker == NULL)
  {
    return 0;
  }
  if (tracker->count == tracker->capacity)
  {
    size_t next_capacity = tracker->capacity * 2;
    AuraRaceEvent *next = (AuraRaceEvent *)realloc(
        tracker->events, next_capacity * sizeof(*tracker->events));
    if (next == NULL)
    {
      return 0;
    }
    tracker->events = next;
    tracker->capacity = next_capacity;
  }
  AuraRaceEvent event = {++tracker->clock, task_id, address, source_id, kind};
  tracker->events[tracker->count++] = event;
  if (out != NULL)
  {
    *out = event;
  }
  return 1;
}

size_t aura_race_tracker_count(const AuraRaceTracker *tracker)
{
  return tracker != NULL ? tracker->count : 0;
}

const AuraRaceEvent *aura_race_tracker_event(const AuraRaceTracker *tracker, size_t index)
{
  if (tracker == NULL || index >= tracker->count)
  {
    return NULL;
  }
  return &tracker->events[index];
}

int aura_race_happens_before(const AuraRaceEvent *before, const AuraRaceEvent *after)
{
  return before != NULL && after != NULL && before->sequence < after->sequence;
}

typedef struct AuraTaskFrame AuraTaskFrame;
typedef struct AuraTaskExecutor AuraTaskExecutor;
typedef struct AuraTaskChannel AuraTaskChannel;

typedef enum
{
  AURA_TASK_READY = 0,
  AURA_TASK_PENDING = 1,
  AURA_TASK_COMPLETE = 2,
  AURA_TASK_FAILED = 3,
  AURA_TASK_CANCELLED = 4
} AuraTaskPollState;

typedef void (*AuraTaskResultDestroyFn)(void *data, size_t size);
typedef AuraTaskPollState (*AuraTaskPollFn)(AuraTaskFrame *frame);
typedef void (*AuraTaskFrameDestroyFn)(AuraTaskFrame *frame);

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

typedef struct
{
  void *data;
  size_t size;
  AuraTaskResultDestroyFn destroy;
  AuraTaskOwnership ownership;
  int rooted;
} AuraTaskFrameStorage;

struct AuraTaskFrame
{
  uint32_t abi_version;
  uint64_t task_id;
  AuraTaskPollFn poll;
  AuraTaskFrameDestroyFn destroy;
  void *data;
  size_t data_size;
  AuraTaskResult result;
  AuraTaskResultDestroyFn result_destroy;
  int result_rooted;
  AuraTaskFrameStorage captures;
  AuraTaskFrameStorage pending;
  AuraTaskResult error;
  AuraTaskResultDestroyFn error_destroy;
  int error_rooted;
  uint32_t resume_state;
  AuraTaskPollState state;
  int cancel_requested;
  int queued;
  AuraTaskExecutor *executor;
  AuraTaskFrame *queue_next;
  AuraTaskFrame *owned_next;
  AuraTaskChannel *waiting_channel;
  void *waiting_node;
};

static uint64_t aura_task_next_id = 1;

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
    frame->data = calloc(1, data_size);
    if (frame->data == NULL)
    {
      free(frame);
      return NULL;
    }
  }
  frame->abi_version = AURA_RT_ABI_VERSION;
  frame->task_id = aura_task_next_id++;
  frame->poll = poll;
  frame->destroy = destroy;
  frame->data_size = data_size;
  frame->resume_state = 0;
  frame->state = AURA_TASK_READY;
  return frame;
}

void *aura_task_frame_data(AuraTaskFrame *frame)
{
  return frame != NULL ? frame->data : NULL;
}

uint64_t aura_task_frame_task_id(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->task_id : 0;
}

AuraTaskPollState aura_task_frame_state(const AuraTaskFrame *frame)
{
  return frame != NULL ? frame->state : AURA_TASK_FAILED;
}

int aura_task_frame_cancel_requested(const AuraTaskFrame *frame)
{
  return frame != NULL && frame->cancel_requested;
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
  if (storage == NULL)
  {
    return;
  }
  if (storage->rooted)
  {
    aura_gc_remove_root(&storage->data);
  }
  if (storage->destroy != NULL && storage->data != NULL)
  {
    storage->destroy(storage->data, storage->size);
  }
  *storage = (AuraTaskFrameStorage){NULL, 0, NULL, AURA_TASK_OWNED, 0};
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

void aura_task_frame_set_error(AuraTaskFrame *frame,
                               void *data,
                               size_t size,
                               AuraTaskResultDestroyFn destroy)
{
  if (frame == NULL)
  {
    return;
  }
  if (frame->error_destroy != NULL && frame->error.data != NULL)
  {
    frame->error_destroy(frame->error.data, frame->error.size);
  }
  if (frame->error_rooted)
  {
    aura_gc_remove_root(&frame->error.data);
    frame->error_rooted = 0;
  }
  frame->error = (AuraTaskResult){data, size};
  frame->error_destroy = destroy;
  if (data != NULL)
  {
    aura_gc_add_root(&frame->error.data);
    frame->error_rooted = 1;
  }
  if (data != NULL)
  {
    frame->state = AURA_TASK_FAILED;
  }
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
  if (frame->result_destroy != NULL && frame->result.data != NULL)
  {
    frame->result_destroy(frame->result.data, frame->result.size);
  }
  if (frame->result_rooted)
  {
    aura_gc_remove_root(&frame->result.data);
    frame->result_rooted = 0;
  }
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

void aura_task_frame_destroy(AuraTaskFrame *frame)
{
  if (frame == NULL)
  {
    return;
  }
  if (frame->destroy != NULL)
  {
    frame->destroy(frame);
  }
  if (frame->result_destroy != NULL && frame->result.data != NULL)
  {
    frame->result_destroy(frame->result.data, frame->result.size);
  }
  if (frame->result_rooted)
  {
    aura_gc_remove_root(&frame->result.data);
  }
  aura_task_frame_storage_release(&frame->captures);
  aura_task_frame_storage_release(&frame->pending);
  if (frame->error_destroy != NULL && frame->error.data != NULL)
  {
    frame->error_destroy(frame->error.data, frame->error.size);
  }
  if (frame->error_rooted)
  {
    aura_gc_remove_root(&frame->error.data);
  }
  free(frame->data);
  free(frame);
}

/* ---- C22k deterministic single-threaded executor ----
 *
 * Submission transfers frame ownership to the executor.  The executor keeps
 * terminal frames alive so generated code can read their result until
 * shutdown; aura_task_executor_shutdown destroys every submitted frame once.
 * A poll callback returning READY is immediately queued at the FIFO tail.
 * PENDING parks the frame until aura_task_executor_wake is called.  No OS
 * threads, blocking waits, or implicit polling are used.
 */

struct AuraTaskExecutor
{
  AuraTaskFrame *ready_head;
  AuraTaskFrame *ready_tail;
  AuraTaskFrame *owned_head;
  size_t ready_count;
  size_t owned_count;
  int shutdown;
  AuraRaceTracker *race_tracker;
};

int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame);
static void aura_task_channel_cancel_wait(AuraTaskFrame *frame);

AuraTaskPollState aura_task_frame_poll_once(AuraTaskFrame *frame)
{
  if (frame == NULL || frame->poll == NULL)
  {
    return AURA_TASK_FAILED;
  }
  if (frame->state == AURA_TASK_COMPLETE || frame->state == AURA_TASK_FAILED ||
      frame->state == AURA_TASK_CANCELLED)
  {
    return frame->state;
  }
  if (frame->cancel_requested)
  {
    frame->state = AURA_TASK_CANCELLED;
    return frame->state;
  }
  AuraTaskPollState state = frame->poll(frame);
  if (state < AURA_TASK_READY || state > AURA_TASK_CANCELLED)
  {
    state = AURA_TASK_FAILED;
  }
  frame->state = state;
  return state;
}

AuraTaskExecutor *aura_task_executor_new(void)
{
  return (AuraTaskExecutor *)calloc(1, sizeof(AuraTaskExecutor));
}

void aura_task_executor_set_race_tracker(AuraTaskExecutor *executor,
                                         AuraRaceTracker *tracker)
{
  if (executor != NULL && !executor->shutdown)
  {
    executor->race_tracker = tracker;
  }
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
  if (executor == NULL || frame == NULL || executor->shutdown || frame->executor != NULL)
  {
    return 0;
  }
  aura_task_executor_push_owned(executor, frame);
  if (executor->race_tracker != NULL)
  {
    (void)aura_race_tracker_record(executor->race_tracker,
                                   frame->task_id,
                                   0,
                                   0,
                                   AURA_RACE_TASK_SPAWN,
                                   NULL);
  }
  frame->state = AURA_TASK_READY;
  return aura_task_executor_wake(executor, frame);
}

int aura_task_executor_wake(AuraTaskExecutor *executor, AuraTaskFrame *frame)
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
  aura_task_channel_cancel_wait(frame);
  if (!frame->queued)
  {
    aura_task_executor_wake(executor, frame);
  }
  return 1;
}

size_t aura_task_executor_ready_count(const AuraTaskExecutor *executor)
{
  return executor != NULL ? executor->ready_count : 0;
}

size_t aura_task_executor_task_count(const AuraTaskExecutor *executor)
{
  return executor != NULL ? executor->owned_count : 0;
}

int aura_task_executor_run_one(AuraTaskExecutor *executor)
{
  if (executor == NULL || executor->shutdown || executor->ready_head == NULL)
  {
    return 0;
  }
  AuraTaskFrame *frame = executor->ready_head;
  executor->ready_head = frame->queue_next;
  if (executor->ready_head == NULL)
  {
    executor->ready_tail = NULL;
  }
  frame->queue_next = NULL;
  frame->queued = 0;
  executor->ready_count--;

  AuraTaskPollState state = aura_task_frame_poll_once(frame);
  if (state == AURA_TASK_READY)
  {
    aura_task_executor_wake(executor, frame);
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

void aura_task_executor_shutdown(AuraTaskExecutor *executor)
{
  if (executor == NULL || executor->shutdown)
  {
    return;
  }
  executor->shutdown = 1;
  AuraTaskFrame *frame = executor->owned_head;
  while (frame != NULL)
  {
    AuraTaskFrame *next = frame->owned_next;
    frame->executor = NULL;
    frame->queued = 0;
    aura_task_channel_cancel_wait(frame);
    aura_task_frame_destroy(frame);
    frame = next;
  }
  free(executor);
}

/* ---- C22n bounded FIFO channels (single-threaded MVP) ----
 *
 * A channel owns every value accepted by aura_task_channel_send.  A queued
 * value is delivered by moving the value record to the receiver's output;
 * after that point the receiver owns it and must invoke its destroy callback.
 * Values rejected by a closed channel, or held by a waiting sender when the
 * channel closes, are destroyed exactly once by the channel.  A send that
 * returns AURA_CHANNEL_PENDING transfers ownership to the channel.
 *
 * Waiting frames are borrowed references.  The executor owns their lifetime;
 * cancellation and executor shutdown unlink waiters before destroying the
 * frame.  Wakeups are FIFO and use the frame's executor, with no OS threads.
 */

typedef void (*AuraTaskChannelValueDestroyFn)(void *data, size_t size);

typedef struct
{
  void *data;
  size_t size;
  AuraTaskChannelValueDestroyFn destroy;
} AuraTaskChannelValue;

typedef enum
{
  AURA_CHANNEL_OK = 0,
  AURA_CHANNEL_PENDING = 1,
  AURA_CHANNEL_CLOSED = 2,
  AURA_CHANNEL_ERROR = 3
} AuraTaskChannelStatus;

typedef struct AuraTaskChannelWaiter AuraTaskChannelWaiter;

struct AuraTaskChannelWaiter
{
  AuraTaskFrame *frame;
  AuraTaskChannelValue value;
  AuraTaskChannelValue *out;
  AuraTaskChannelWaiter *next;
};

struct AuraTaskChannel
{
  AuraTaskChannelValue *values;
  size_t capacity;
  size_t head;
  size_t tail;
  size_t count;
  int closed;
  AuraTaskChannelWaiter *send_head;
  AuraTaskChannelWaiter *send_tail;
  AuraTaskChannelWaiter *receive_head;
  AuraTaskChannelWaiter *receive_tail;
};

static void aura_task_channel_value_destroy(AuraTaskChannelValue *value)
{
  if (value != NULL && value->destroy != NULL && value->data != NULL)
  {
    value->destroy(value->data, value->size);
  }
  if (value != NULL)
  {
    value->data = NULL;
    value->size = 0;
    value->destroy = NULL;
  }
}

/* C22o glue: generated send/receive expressions use these stable callbacks.
 * The class form also releases the temporary GC root held by the payload box. */
void aura_task_channel_value_destroy_free(void *data, size_t size)
{
  (void)size;
  free(data);
}

void aura_task_channel_value_destroy_class(void *data, size_t size)
{
  (void)size;
  if (data != NULL)
  {
    aura_gc_remove_root((void **)data);
    free(data);
  }
}

static void aura_task_channel_wake(AuraTaskFrame *frame)
{
  if (frame != NULL && frame->executor != NULL)
  {
    (void)aura_task_executor_wake(frame->executor, frame);
  }
}

static void aura_task_channel_unlink(AuraTaskChannel *channel,
                                     AuraTaskChannelWaiter *target,
                                     int receiver)
{
  AuraTaskChannelWaiter **link = receiver ? &channel->receive_head : &channel->send_head;
  AuraTaskChannelWaiter *tail = receiver ? channel->receive_tail : channel->send_tail;
  while (*link != NULL && *link != target)
  {
    link = &(*link)->next;
  }
  if (*link == NULL)
  {
    return;
  }
  *link = target->next;
  if (tail == target)
  {
    if (receiver)
    {
      channel->receive_tail = NULL;
      for (AuraTaskChannelWaiter *w = channel->receive_head; w != NULL; w = w->next)
        channel->receive_tail = w;
    }
    else
    {
      channel->send_tail = NULL;
      for (AuraTaskChannelWaiter *w = channel->send_head; w != NULL; w = w->next)
        channel->send_tail = w;
    }
  }
  target->next = NULL;
}

static void aura_task_channel_cancel_wait(AuraTaskFrame *frame)
{
  if (frame == NULL || frame->waiting_channel == NULL || frame->waiting_node == NULL)
  {
    return;
  }
  AuraTaskChannel *channel = frame->waiting_channel;
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)frame->waiting_node;
  int receiver = waiter->out != NULL;
  aura_task_channel_unlink(channel, waiter, receiver);
  if (!receiver)
  {
    aura_task_channel_value_destroy(&waiter->value);
  }
  free(waiter);
  frame->waiting_channel = NULL;
  frame->waiting_node = NULL;
}

AuraTaskChannel *aura_task_channel_new(size_t capacity)
{
  if (capacity == 0)
  {
    return NULL;
  }
  AuraTaskChannel *channel = (AuraTaskChannel *)calloc(1, sizeof(*channel));
  if (channel == NULL)
  {
    return NULL;
  }
  channel->values = (AuraTaskChannelValue *)calloc(capacity, sizeof(*channel->values));
  if (channel->values == NULL)
  {
    free(channel);
    return NULL;
  }
  channel->capacity = capacity;
  return channel;
}

size_t aura_task_channel_capacity(const AuraTaskChannel *channel)
{
  return channel != NULL ? channel->capacity : 0;
}

size_t aura_task_channel_count(const AuraTaskChannel *channel)
{
  return channel != NULL ? channel->count : 0;
}

int aura_task_channel_is_closed(const AuraTaskChannel *channel)
{
  return channel != NULL && channel->closed;
}

AuraTaskChannelStatus aura_task_channel_send(AuraTaskChannel *channel,
                                              AuraTaskFrame *sender,
                                              AuraTaskChannelValue value)
{
  if (channel == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  if (channel->closed)
  {
    aura_task_channel_value_destroy(&value);
    return AURA_CHANNEL_CLOSED;
  }
  if (channel->receive_head != NULL)
  {
    AuraTaskChannelWaiter *receiver = channel->receive_head;
    aura_task_channel_unlink(channel, receiver, 1);
    *receiver->out = value;
    AuraTaskFrame *receiver_frame = receiver->frame;
    receiver_frame->waiting_channel = NULL;
    receiver_frame->waiting_node = NULL;
    free(receiver);
    aura_task_channel_wake(receiver_frame);
    return AURA_CHANNEL_OK;
  }
  if (channel->count < channel->capacity)
  {
    channel->values[channel->tail] = value;
    channel->tail = (channel->tail + 1) % channel->capacity;
    channel->count++;
    return AURA_CHANNEL_OK;
  }
  if (sender == NULL)
  {
    return AURA_CHANNEL_PENDING;
  }
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
  if (waiter == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  waiter->frame = sender;
  waiter->value = value;
  if (channel->send_tail == NULL)
    channel->send_head = waiter;
  else
    channel->send_tail->next = waiter;
  channel->send_tail = waiter;
  sender->waiting_channel = channel;
  sender->waiting_node = waiter;
  return AURA_CHANNEL_PENDING;
}

AuraTaskChannelStatus aura_task_channel_receive(AuraTaskChannel *channel,
                                                 AuraTaskFrame *receiver,
                                                 AuraTaskChannelValue *out)
{
  if (channel == NULL || out == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  if (channel->count != 0)
  {
    *out = channel->values[channel->head];
    channel->values[channel->head] = (AuraTaskChannelValue){NULL, 0, NULL};
    channel->head = (channel->head + 1) % channel->capacity;
    channel->count--;
    if (channel->send_head != NULL)
    {
      AuraTaskChannelWaiter *sender = channel->send_head;
      aura_task_channel_unlink(channel, sender, 0);
      channel->values[channel->tail] = sender->value;
      channel->tail = (channel->tail + 1) % channel->capacity;
      channel->count++;
      AuraTaskFrame *sender_frame = sender->frame;
      sender_frame->waiting_channel = NULL;
      sender_frame->waiting_node = NULL;
      free(sender);
      aura_task_channel_wake(sender_frame);
    }
    return AURA_CHANNEL_OK;
  }
  if (channel->closed)
  {
    *out = (AuraTaskChannelValue){NULL, 0, NULL};
    return AURA_CHANNEL_CLOSED;
  }
  if (receiver == NULL)
  {
    return AURA_CHANNEL_PENDING;
  }
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
  if (waiter == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  waiter->frame = receiver;
  waiter->out = out;
  if (channel->receive_tail == NULL)
    channel->receive_head = waiter;
  else
    channel->receive_tail->next = waiter;
  channel->receive_tail = waiter;
  receiver->waiting_channel = channel;
  receiver->waiting_node = waiter;
  return AURA_CHANNEL_PENDING;
}

int aura_task_channel_close(AuraTaskChannel *channel)
{
  if (channel == NULL)
  {
    return 0;
  }
  if (channel->closed)
  {
    return 0;
  }
  channel->closed = 1;
  while (channel->send_head != NULL)
  {
    AuraTaskChannelWaiter *waiter = channel->send_head;
    aura_task_channel_unlink(channel, waiter, 0);
    aura_task_channel_value_destroy(&waiter->value);
    waiter->frame->waiting_channel = NULL;
    waiter->frame->waiting_node = NULL;
    AuraTaskFrame *frame = waiter->frame;
    free(waiter);
    aura_task_channel_wake(frame);
  }
  while (channel->receive_head != NULL)
  {
    AuraTaskChannelWaiter *waiter = channel->receive_head;
    aura_task_channel_unlink(channel, waiter, 1);
    waiter->frame->waiting_channel = NULL;
    waiter->frame->waiting_node = NULL;
    AuraTaskFrame *frame = waiter->frame;
    free(waiter);
    aura_task_channel_wake(frame);
  }
  return 1;
}

void aura_task_channel_destroy(AuraTaskChannel *channel)
{
  if (channel == NULL)
  {
    return;
  }
  (void)aura_task_channel_close(channel);
  while (channel->count != 0)
  {
    aura_task_channel_value_destroy(&channel->values[channel->head]);
    channel->head = (channel->head + 1) % channel->capacity;
    channel->count--;
  }
  free(channel->values);
  free(channel);
}

/* ---- Process argv (std.io.args) ----
 * Stashed from C main before aura_main so user programs keep fun main().
 * Each returned string is an owned copy because Array<String> frees its
 * elements when the array is dropped.
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
  const char *s = aura_saved_argv[i] != NULL ? aura_saved_argv[i] : "";
  size_t n = strlen(s);
  char *copy = (char *)malloc(n + 1);
  if (copy == NULL)
  {
    aura_throw_string("args allocation failed");
  }
  memcpy(copy, s, n + 1);
  return copy;
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

#ifndef AURA_RUNTIME_NO_MAIN
int main(int argc, char **argv)
{
  aura_set_args(argc, argv);
  int rc = aura_main();
  aura_gc_shutdown();
  return rc;
}
#endif
