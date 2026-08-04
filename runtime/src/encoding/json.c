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
