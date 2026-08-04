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
