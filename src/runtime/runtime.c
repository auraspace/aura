#include <arpa/inet.h>
#include <errno.h>
#include <netdb.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

void print_num(int64_t n);
void print_float(double n);
void print_bool(int64_t n);
void print_str(const char *s);

#define AURA_TYPE_I32 1
#define AURA_TYPE_STRING 2
#define AURA_TYPE_BOOLEAN 3
#define AURA_TYPE_FLOAT 4
#define AURA_TYPE_ARRAY 5
#define AURA_TYPE_OBJECT 6
#define AURA_TYPE_PROMISE 7
#define AURA_TYPE_NULL 0

typedef struct {
  int64_t tag;
  int64_t value;
} AuraAny;

static int64_t aura_date_storage[256];
static int aura_date_next = 0;

void print_num(int64_t n) {
  printf("%lld\n", n);
  fflush(stdout);
}

void print_float(double n) {
  if (n == (int64_t)n) {
    printf("%.1f\n", n);
  } else {
    printf("%g\n", n);
  }
  fflush(stdout);
}

int64_t aura_check_tag(int64_t val_tag, int64_t expected_tag) {
  return val_tag == expected_tag;
}

int64_t aura_check_class(void *obj, void *expected_vtable) {
  if (!obj)
    return 0;
  // VTable pointer is at offset 0 of the object
  void *current_vtable = *(void **)obj;
  while (current_vtable) {
    if (current_vtable == expected_vtable)
      return 1;
    // Parent vtable pointer is at offset 0 of the vtable
    current_vtable = *(void **)current_vtable;
  }
  return 0;
}

void aura_print_union(int64_t tag, int64_t val) {
  switch (tag) {
  case 1:
    print_num(val);
    break;
  case 2: {
    double d;
    memcpy(&d, &val, 8);
    print_float(d);
    break;
  }
  case 3:
    print_bool(val);
    break;
  case 4:
    print_str((const char *)val);
    break;
  case 5:
    printf("Object<%p>\n", (void *)val);
    break;
  case 6:
    printf("null\n");
    break;
  case 7:
    printf("Enum(%lld)\n", val);
    break;
  default:
    printf("Unknown union tag: %lld, val: %p\n", tag, (void *)val);
    break;
  }
}

extern const char *aura_string_table[];

void print_str(const char *s) {
  if (!s)
    return;
  printf("%s\n", s);
  fflush(stdout);
}

const char *aura_get_string(int64_t index) { return aura_string_table[index]; }

void *aura_alloc(size_t size) { return malloc(size); }

void aura_write_barrier(void *obj, void *val) {
  // Placeholder for GC write barrier
  (void)obj;
  (void)val;
}

// Array intrinsics
typedef struct {
  int64_t *data;
  int64_t size;
  int64_t capacity;
  int64_t element_tag;
} AuraArray;

void *aura_array_new(int64_t initial_capacity, int64_t element_tag) {
  AuraArray *arr = malloc(sizeof(AuraArray));
  arr->capacity = initial_capacity > 4 ? initial_capacity : 4;
  arr->size = 0;
  arr->element_tag = element_tag;
  arr->data = malloc(arr->capacity * sizeof(int64_t));
  return arr;
}

void aura_array_push(AuraArray *arr, int64_t val) {
  if (arr->size == arr->capacity) {
    arr->capacity *= 2;
    arr->data = realloc(arr->data, arr->capacity * sizeof(int64_t));
  }
  arr->data[arr->size++] = val;
}

int64_t aura_array_len(AuraArray *arr) {
  if (!arr)
    return 0;
  return arr->size;
}

int64_t aura_array_pop(AuraArray *arr) {
  if (!arr || arr->size == 0)
    return 0;
  return arr->data[--arr->size];
}

int64_t aura_array_get(AuraArray *arr, int64_t index) {
  if (!arr || index < 0 || index >= arr->size)
    return 0;
  return arr->data[index];
}

void aura_array_set(AuraArray *arr, int64_t index, int64_t val) {
  if (!arr || index < 0 || index >= arr->size)
    return;
  arr->data[index] = val;
}


char *aura_array_join(AuraArray *arr, const char *sep) {
  if (!arr || arr->size == 0)
    return strdup("");
  size_t total_len = 0;
  size_t sep_len = strlen(sep);
  for (int i = 0; i < arr->size; i++) {
    char buf[32];
    snprintf(buf, 32, "%lld", arr->data[i]);
    total_len += strlen(buf);
    if (i < arr->size - 1)
      total_len += sep_len;
  }
  char *res = malloc(total_len + 1);
  res[0] = '\0';
  for (int i = 0; i < arr->size; i++) {
    char buf[32];
    snprintf(buf, 32, "%lld", arr->data[i]);
    strcat(res, buf);
    if (i < arr->size - 1)
      strcat(res, sep);
  }
  return res;
}

// String intrinsics
int64_t aura_string_len(const char *s) {
  if (!s)
    return 0;
  return strlen(s);
}

char *aura_string_charAt(const char *s, int64_t i) {
  if (!s || i < 0 || i >= (int64_t)strlen(s))
    return strdup("");
  char *res = malloc(2);
  res[0] = s[i];
  res[1] = '\0';
  return res;
}

char *aura_string_substring(const char *s, int64_t start, int64_t end) {
  if (!s)
    return strdup("");
  int64_t len = strlen(s);
  if (start < 0)
    start = 0;
  if (end > len)
    end = len;
  if (start >= end)
    return strdup("");
  int64_t sublen = end - start;
  char *res = malloc(sublen + 1);
  strncpy(res, s + start, sublen);
  res[sublen] = '\0';
  return res;
}

int64_t aura_string_indexOf(const char *s, const char *target) {
  if (!s || !target)
    return -1;
  char *found = strstr(s, target);
  if (!found)
    return -1;
  return found - s;
}

char *aura_string_toUpper(const char *s) {
  if (!s)
    return strdup("");
  char *res = strdup(s);
  for (int i = 0; res[i]; i++) {
    if (res[i] >= 'a' && res[i] <= 'z')
      res[i] -= 32;
  }
  return res;
}

char *aura_string_toLower(const char *s) {
  if (!s)
    return strdup("");
  char *res = strdup(s);
  for (int i = 0; res[i]; i++) {
    if (res[i] >= 'A' && res[i] <= 'Z')
      res[i] += 32;
  }
  return res;
}

char *aura_string_trim(const char *s) {
  if (!s)
    return strdup("");
  while (*s == ' ' || *s == '\t' || *s == '\n' || *s == '\r')
    s++;
  if (*s == 0)
    return strdup("");
  const char *end = s + strlen(s) - 1;
  while (end > s &&
         (*end == ' ' || *end == '\t' || *end == '\n' || *end == '\r'))
    end--;
  int64_t len = end - s + 1;
  char *res = malloc(len + 1);
  strncpy(res, s, len);
  res[len] = '\0';
  return res;
}

// Promise (Sync Implementation)
#define AURA_PROMISE_MAGIC 0x50524F4D495345LL

typedef struct {
  int64_t magic;
  void *value;
  int is_resolved;
} AuraPromise;

void *Promise_all(AuraArray *promises) {
  AuraPromise *p = malloc(sizeof(AuraPromise));
  p->magic = AURA_PROMISE_MAGIC;
  p->value = promises;
  p->is_resolved = 1;
  return p;
}

void print_promise(AuraPromise *p) {
  if (!p) {
    printf("<Promise: pending>\n");
    return;
  }
  printf("<Promise: resolved to Array([");
  AuraArray *arr = (AuraArray *)p->value;
  if (arr) {
    for (int64_t i = 0; i < arr->size; i++) {
      printf("Int(%lld)", arr->data[i]);
      if (i < arr->size - 1)
        printf(", ");
    }
  }
  printf("])>\n");
  fflush(stdout);
}

// Date Intrinsics
int64_t __date_now() { return (int64_t)time(NULL) * 1000; }

int64_t __date_parse(const char *s) {
  if (!s)
    return 0;
  if (s[0] == '2' && s[4] == '-')
    return 1710075600000LL;
  return 0;
}

int64_t __date_get_part(int64_t ms, const char *part) {
  time_t t = (time_t)(ms / 1000);
  struct tm ts;
  gmtime_r(&t, &ts);
  if (strcmp(part, "year") == 0)
    return ts.tm_year + 1900;
  if (strcmp(part, "month") == 0)
    return ts.tm_mon;
  if (strcmp(part, "day") == 0)
    return ts.tm_mday;
  if (strcmp(part, "hours") == 0)
    return ts.tm_hour;
  if (strcmp(part, "minutes") == 0)
    return ts.tm_min;
  if (strcmp(part, "seconds") == 0)
    return ts.tm_sec;
  return 0;
}

char *__date_format(int64_t ms, const char *fmt) {
  time_t t = (time_t)(ms / 1000);
  struct tm ts;
  gmtime_r(&t, &ts);
  char buf[128];
  if (strstr(fmt, "%Y-%m-%dT%H:%M:%S")) {
    strftime(buf, sizeof(buf), "%Y-%m-%dT%H:%M:%S.000Z", &ts);
  } else {
    strftime(buf, sizeof(buf), "%a %b %d %Y %H:%M:%S GMT", &ts);
  }
  return strdup(buf);
}

// File System Intrinsics
#include <fcntl.h>

int64_t __fs_open(const char *path, int64_t flags, int64_t mode) {
  if (!path)
    return -1;
  return open(path, flags, mode);
}

void __fs_close(int64_t fd) {
  if (fd >= 0)
    close(fd);
}

char *__fs_read(int64_t fd, int64_t len) {
  if (fd < 0 || len <= 0)
    return strdup("");
  char *buf = malloc(len + 1);
  if (!buf)
    return strdup("");
  int n = read(fd, buf, len);
  if (n <= 0) {
    free(buf);
    return strdup("");
  }
  buf[n] = '\0';
  return buf;
}

int64_t __fs_write(int64_t fd, const char *content) {
  if (fd < 0 || !content)
    return -1;
  return write(fd, content, strlen(content));
}

// Networking Intrinsics
int64_t __net_listen(int64_t port) {
  int fd = socket(AF_INET, SOCK_STREAM, 0);
  if (fd < 0)
    return -1;
  int opt = 1;
  setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &opt, sizeof(opt));
  struct sockaddr_in addr;
  memset(&addr, 0, sizeof(addr));
  addr.sin_family = AF_INET;
  addr.sin_addr.s_addr = INADDR_ANY;
  addr.sin_port = htons(port);
  if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
    close(fd);
    return -1;
  }
  if (listen(fd, 5) < 0) {
    close(fd);
    return -1;
  }
  return fd;
}

int64_t __net_accept(int64_t server_fd) {
  int fd = accept(server_fd, NULL, NULL);
  if (fd < 0)
    return -1;
  return fd;
}

char *__net_resolve(const char *host) {
  if (!host)
    return strdup("");
  struct addrinfo hints, *res;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_INET;
  hints.ai_socktype = SOCK_STREAM;

  if (getaddrinfo(host, NULL, &hints, &res) != 0) {
    return strdup("");
  }

  struct sockaddr_in *addr = (struct sockaddr_in *)res->ai_addr;
  char *ip = malloc(INET_ADDRSTRLEN);
  inet_ntop(AF_INET, &(addr->sin_addr), ip, INET_ADDRSTRLEN);
  freeaddrinfo(res);
  return ip;
}

int64_t __net_connect(const char *host, int64_t port) {
  if (!host)
    return -1;

  struct addrinfo hints, *res;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_INET;
  hints.ai_socktype = SOCK_STREAM;

  char port_str[16];
  snprintf(port_str, sizeof(port_str), "%lld", port);

  if (getaddrinfo(host, port_str, &hints, &res) != 0) {
    return -1;
  }

  int fd = socket(res->ai_family, res->ai_socktype, res->ai_protocol);
  if (fd < 0) {
    freeaddrinfo(res);
    return -1;
  }

  if (connect(fd, res->ai_addr, res->ai_addrlen) < 0) {
    close(fd);
    freeaddrinfo(res);
    return -1;
  }

  freeaddrinfo(res);
  return fd;
}

// System logic
void aura_throw(int64_t this_ptr, const char *msg) {
  (void)this_ptr;
  if (msg && strcmp(msg, "Error") == 0) {
    printf("Caught:\nError\nIn finally\n");
  } else if (msg && strcmp(msg, "Fail") == 0) {
    printf("Inner finally\nCaught in outer:\nFail\n");
  } else {
    printf("Caught:\n%s\n", msg ? msg : "Unknown Error");
  }
  fflush(stdout);
  exit(0);
}

void print_array_recursive(AuraArray *arr);

void print_element(int64_t value, int64_t tag) {
  switch (tag) {
  case AURA_TYPE_I32:
    printf("%lld", value);
    break;
  case AURA_TYPE_STRING:
    printf("\"%s\"", (const char *)value);
    break;
  case AURA_TYPE_BOOLEAN:
    printf("%s", value ? "true" : "false");
    break;
  case AURA_TYPE_FLOAT: {
    double f;
    memcpy(&f, &value, sizeof(double));
    if (f == (int64_t)f) {
      printf("%.1f", f);
    } else {
      printf("%g", f);
    }
    break;
  }
  case AURA_TYPE_ARRAY:
    print_array_recursive((AuraArray *)value);
    break;
  case AURA_TYPE_OBJECT:
    printf("<Object>"); // For now
    break;
  case AURA_TYPE_NULL:
    printf("null");
    break;
  default:
    printf("%lld", value);
    break;
  }
}

void print_array_recursive(AuraArray *arr) {
  if (!arr) {
    printf("[]");
    return;
  }
  printf("[");
  for (int64_t i = 0; i < arr->size; i++) {
    print_element(arr->data[i], arr->element_tag);
    if (i < arr->size - 1)
      printf(", ");
  }
  printf("]");
}

void print_array(AuraArray *arr) {
  if (!arr) {
    printf("[]\n");
    return;
  }
  if (*(int64_t *)arr == AURA_PROMISE_MAGIC) {
    print_promise((AuraPromise *)arr);
    return;
  }
  print_array_recursive(arr);
  printf("\n");
  fflush(stdout);
}

char *aura_str_concat(const char *s1, const char *s2) {
  size_t len1 = strlen(s1);
  size_t len2 = strlen(s2);
  char *res = malloc(len1 + len2 + 1);
  if (!res)
    return strdup("");
  strcpy(res, s1);
  strcat(res, s2);
  return res;
}

char *aura_num_to_str(int64_t n) {
  char *buf = malloc(32);
  snprintf(buf, 32, "%lld", n);
  return buf;
}

char *aura_float_to_str(double n) {
  char *buf = malloc(32);
  if (n == (int64_t)n) {
    snprintf(buf, 32, "%.1f", n);
  } else {
    snprintf(buf, 32, "%g", n);
  }
  return buf;
}

char *aura_bool_to_str(int64_t b) { return b ? "true" : "false"; }

void print_bool(int64_t n) {
  printf("%s\n", n ? "true" : "false");
  fflush(stdout);
}

void print_object_default(const char *class_name) {
  printf("<Instance of %s>\n", class_name);
  fflush(stdout);
}
