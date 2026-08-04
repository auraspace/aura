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
