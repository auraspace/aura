/* ---- Bounded HTTP/1.1 request parser (transport-independent) ----
 *
 * This parser consumes one complete request from a byte buffer.  It does not
 * read from a socket and does not retain the input buffer: every field exposed
 * by AuraHttpRequest is heap-owned and is released by
 * aura_http_request_destroy.  A caller can use out_consumed to leave a
 * following keep-alive request in the input buffer.
 */

#define AURA_HTTP_MAX_REQUEST_LINE_BYTES ((size_t)8 * 1024)
#define AURA_HTTP_MAX_HEADERS ((size_t)64)
#define AURA_HTTP_MAX_HEADER_BYTES ((size_t)16 * 1024)
#define AURA_HTTP_MAX_BODY_BYTES ((size_t)8 * 1024 * 1024)
#define AURA_HTTP_MAX_TOTAL_BYTES ((size_t)16 * 1024 * 1024)

typedef enum
{
  AURA_HTTP_PARSE_ERROR = -1,
  AURA_HTTP_PARSE_OK = 0,
  AURA_HTTP_PARSE_INCOMPLETE = 1,
  AURA_HTTP_PARSE_BAD_REQUEST = 400,
  AURA_HTTP_PARSE_METHOD_NOT_ALLOWED = 405,
  AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE = 413
} AuraHttpParseStatus;

typedef struct
{
  char *name;
  char *value;
} AuraHttpHeader;

typedef struct AuraHttpContentLengthReader AuraHttpContentLengthReader;

typedef struct AuraHttpRequest
{
  char *method;
  char *target;
  char *version;
  AuraHttpHeader *headers;
  size_t header_count;
  unsigned char *body;
  size_t body_length;
  size_t total_length;
  /* Borrowed from an active async connection; never owned by the request. */
  AuraHttpContentLengthReader *body_reader;
} AuraHttpRequest;

static int aura_http_is_token(unsigned char c)
{
  if ((c >= (unsigned char)'A' && c <= (unsigned char)'Z') ||
      (c >= (unsigned char)'a' && c <= (unsigned char)'z') ||
      (c >= (unsigned char)'0' && c <= (unsigned char)'9'))
  {
    return 1;
  }
  switch (c)
  {
  case '!':
  case '#':
  case '$':
  case '%':
  case '&':
  case '\'':
  case '*':
  case '+':
  case '-':
  case '.':
  case '^':
  case '_':
  case '`':
  case '|':
  case '~':
    return 1;
  default:
    return 0;
  }
}

static int aura_http_method_allowed(const char *method)
{
  /* Keep the bounded server contract aligned with common web-framework verbs. */
  return method != NULL &&
         (strcmp(method, "GET") == 0 || strcmp(method, "HEAD") == 0 ||
          strcmp(method, "POST") == 0 || strcmp(method, "PUT") == 0 ||
          strcmp(method, "PATCH") == 0 || strcmp(method, "DELETE") == 0 ||
          strcmp(method, "OPTIONS") == 0);
}

static int aura_http_ascii_equal_ci(const unsigned char *left, size_t left_len,
                                    const char *right)
{
  size_t right_len = right == NULL ? 0 : strlen(right);
  size_t i;
  if (right == NULL || left_len != right_len)
  {
    return 0;
  }
  for (i = 0; i < left_len; i++)
  {
    unsigned char a = left[i];
    unsigned char b = (unsigned char)right[i];
    if (a >= (unsigned char)'A' && a <= (unsigned char)'Z')
    {
      a = (unsigned char)(a + ((unsigned char)'a' - (unsigned char)'A'));
    }
    if (b >= (unsigned char)'A' && b <= (unsigned char)'Z')
    {
      b = (unsigned char)(b + ((unsigned char)'a' - (unsigned char)'A'));
    }
    if (a != b)
    {
      return 0;
    }
  }
  return 1;
}

static char *aura_http_copy_string(const unsigned char *data, size_t length)
{
  char *copy;
  if (length == SIZE_MAX)
  {
    return NULL;
  }
  copy = (char *)malloc(length + 1);
  if (copy == NULL)
  {
    return NULL;
  }
  if (length != 0)
  {
    memcpy(copy, data, length);
  }
  copy[length] = '\0';
  return copy;
}

static unsigned char *aura_http_copy_body(const unsigned char *data, size_t length)
{
  unsigned char *copy;
  if (length == 0)
  {
    return NULL;
  }
  copy = (unsigned char *)malloc(length);
  if (copy == NULL)
  {
    return NULL;
  }
  memcpy(copy, data, length);
  return copy;
}

void aura_http_request_destroy(AuraHttpRequest *request)
{
  size_t i;
  if (request == NULL)
  {
    return;
  }
  free(request->method);
  free(request->target);
  free(request->version);
  if (request->headers != NULL)
  {
    for (i = 0; i < request->header_count; i++)
    {
      free(request->headers[i].name);
      free(request->headers[i].value);
    }
  }
  free(request->headers);
  free(request->body);
  memset(request, 0, sizeof(*request));
}

const AuraHttpHeader *aura_http_request_find_header(const AuraHttpRequest *request,
                                                    const char *name)
{
  size_t i;
  if (request == NULL || name == NULL)
  {
    return NULL;
  }
  for (i = 0; i < request->header_count; i++)
  {
    if (aura_http_ascii_equal_ci((const unsigned char *)request->headers[i].name,
                                 strlen(request->headers[i].name), name))
    {
      return &request->headers[i];
    }
  }
  return NULL;
}

const char *aura_http_request_method(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->method;
}

const char *aura_http_request_target(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->target;
}

const char *aura_http_request_version(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->version;
}

size_t aura_http_request_header_count(const AuraHttpRequest *request)
{
  return request == NULL ? 0 : request->header_count;
}

const char *aura_http_request_header_name(const AuraHttpRequest *request,
                                          size_t index)
{
  if (request == NULL || index >= request->header_count)
  {
    return NULL;
  }
  return request->headers[index].name;
}

const char *aura_http_request_header_value(const AuraHttpRequest *request,
                                           size_t index)
{
  if (request == NULL || index >= request->header_count)
  {
    return NULL;
  }
  return request->headers[index].value;
}

const unsigned char *aura_http_request_body(const AuraHttpRequest *request)
{
  return request == NULL ? NULL : request->body;
}

size_t aura_http_request_body_length(const AuraHttpRequest *request)
{
  return request == NULL ? 0 : request->body_length;
}

typedef enum
{
  AURA_HTTP_LINE_FOUND,
  AURA_HTTP_LINE_INCOMPLETE,
  AURA_HTTP_LINE_BAD,
  AURA_HTTP_LINE_TOO_LARGE
} AuraHttpLineResult;

static AuraHttpLineResult aura_http_find_line(const unsigned char *data,
                                              size_t length, size_t start,
                                              size_t limit, size_t *out_end)
{
  size_t i;
  if (start > length)
  {
    return AURA_HTTP_LINE_INCOMPLETE;
  }
  for (i = start; i < length; i++)
  {
    unsigned char c = data[i];
    if (c == (unsigned char)'\n')
    {
      if (i == start || data[i - 1] != (unsigned char)'\r')
      {
        return AURA_HTTP_LINE_BAD;
      }
      if (i + 1 - start > limit)
      {
        return AURA_HTTP_LINE_TOO_LARGE;
      }
      *out_end = i + 1;
      return AURA_HTTP_LINE_FOUND;
    }
    if (c == (unsigned char)'\r' &&
        i + 1 < length && data[i + 1] != (unsigned char)'\n')
    {
      return AURA_HTTP_LINE_BAD;
    }
    if (i + 1 - start > limit)
    {
      return AURA_HTTP_LINE_TOO_LARGE;
    }
  }
  return length - start > limit ? AURA_HTTP_LINE_TOO_LARGE : AURA_HTTP_LINE_INCOMPLETE;
}

static AuraHttpParseStatus aura_http_line_status(AuraHttpLineResult result)
{
  switch (result)
  {
  case AURA_HTTP_LINE_TOO_LARGE:
    return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
  case AURA_HTTP_LINE_BAD:
    return AURA_HTTP_PARSE_BAD_REQUEST;
  case AURA_HTTP_LINE_INCOMPLETE:
    return AURA_HTTP_PARSE_INCOMPLETE;
  case AURA_HTTP_LINE_FOUND:
    return AURA_HTTP_PARSE_OK;
  default:
    return AURA_HTTP_PARSE_ERROR;
  }
}

static int aura_http_header_name_equal(const unsigned char *name, size_t length,
                                       const char *expected)
{
  return aura_http_ascii_equal_ci(name, length, expected);
}

static int aura_http_parse_content_length(const unsigned char *value, size_t length,
                                          size_t *out_length)
{
  size_t i;
  size_t parsed = 0;
  if (length == 0)
  {
    return 0;
  }
  for (i = 0; i < length; i++)
  {
    unsigned char c = value[i];
    size_t digit;
    if (c < (unsigned char)'0' || c > (unsigned char)'9')
    {
      return 0;
    }
    digit = (size_t)(c - (unsigned char)'0');
    if (parsed > (SIZE_MAX - digit) / 10)
    {
      return 0;
    }
    parsed = parsed * 10 + digit;
  }
  *out_length = parsed;
  return 1;
}

static int aura_http_header_value_valid(const unsigned char *value,
                                        size_t length);

/* Decode a bounded chunked body into the request-owned snapshot. Trailers are
 * retained as request headers after validation. */
static AuraHttpParseStatus aura_http_decode_chunked_body(
    const unsigned char *data, size_t input_length, size_t start,
    unsigned char **out_body, size_t *out_length, size_t *out_consumed,
    AuraHttpHeader *headers, size_t *header_count)
{
  unsigned char *body = NULL;
  size_t body_length = 0;
  size_t capacity = 0;
  size_t cursor = start;

  for (;;)
  {
    AuraHttpLineResult line_result;
    unsigned char *next_body;
    size_t line_end = 0;
    size_t chunk_length = 0;
    size_t i;
    int saw_digit = 0;

    if (cursor > AURA_HTTP_MAX_TOTAL_BYTES)
    {
      free(body);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    line_result = aura_http_find_line(data, input_length, cursor,
                                      AURA_HTTP_MAX_REQUEST_LINE_BYTES, &line_end);
    if (line_result != AURA_HTTP_LINE_FOUND)
    {
      free(body);
      return aura_http_line_status(line_result);
    }
    if (line_end > AURA_HTTP_MAX_TOTAL_BYTES)
    {
      free(body);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    for (i = cursor; i + 2 < line_end; i++)
    {
      unsigned char c = data[i];
      size_t digit;
      if (c >= (unsigned char)'0' && c <= (unsigned char)'9')
      {
        digit = (size_t)(c - (unsigned char)'0');
      }
      else if (c >= (unsigned char)'a' && c <= (unsigned char)'f')
      {
        digit = (size_t)(c - (unsigned char)'a') + 10;
      }
      else if (c >= (unsigned char)'A' && c <= (unsigned char)'F')
      {
        digit = (size_t)(c - (unsigned char)'A') + 10;
      }
      else
      {
        free(body);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      if (chunk_length > (SIZE_MAX - digit) / 16)
      {
        free(body);
        return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
      }
      chunk_length = chunk_length * 16 + digit;
      saw_digit = 1;
    }
    if (!saw_digit)
    {
      free(body);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    cursor = line_end;
    if (chunk_length == 0)
    {
      size_t trailer_bytes = 0;
      for (;;) {
        size_t trailer_end = 0;
        size_t line_content_end;
        size_t colon = SIZE_MAX;
        size_t name_length;
        size_t value_start;
        size_t value_end;
        size_t value_length;

        line_result = aura_http_find_line(data, input_length, cursor,
                                          AURA_HTTP_MAX_HEADER_BYTES,
                                          &trailer_end);
        if (line_result != AURA_HTTP_LINE_FOUND)
        {
          free(body);
          return aura_http_line_status(line_result);
        }
        if (trailer_end < cursor + 2 ||
            trailer_end - cursor > AURA_HTTP_MAX_HEADER_BYTES - trailer_bytes)
        {
          free(body);
          return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
        }
        trailer_bytes += trailer_end - cursor;
        if (trailer_end > AURA_HTTP_MAX_TOTAL_BYTES)
        {
          free(body);
          return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
        }
        line_content_end = trailer_end - 2;
        if (line_content_end == cursor)
        {
          *out_body = body;
          *out_length = body_length;
          *out_consumed = trailer_end;
          return AURA_HTTP_PARSE_OK;
        }
        if (headers == NULL || header_count == NULL ||
            *header_count >= AURA_HTTP_MAX_HEADERS)
        {
          free(body);
          return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
        }
        for (i = cursor; i < line_content_end; i++)
        {
          if (data[i] == (unsigned char)':')
          {
            colon = i;
            break;
          }
        }
        if (colon == SIZE_MAX || colon == cursor)
        {
          free(body);
          return AURA_HTTP_PARSE_BAD_REQUEST;
        }
        name_length = colon - cursor;
        for (i = cursor; i < colon; i++)
        {
          if (!aura_http_is_token(data[i]))
          {
            free(body);
            return AURA_HTTP_PARSE_BAD_REQUEST;
          }
        }
        /* Framing fields are never valid trailers. */
        if (aura_http_header_name_equal(data + cursor, name_length,
                                        "Content-Length") ||
            aura_http_header_name_equal(data + cursor, name_length,
                                        "Transfer-Encoding") ||
            aura_http_header_name_equal(data + cursor, name_length,
                                        "Trailer"))
        {
          free(body);
          return AURA_HTTP_PARSE_BAD_REQUEST;
        }
        value_start = colon + 1;
        value_end = line_content_end;
        while (value_start < value_end &&
               (data[value_start] == (unsigned char)' ' ||
                data[value_start] == (unsigned char)'\t'))
        {
          value_start++;
        }
        while (value_end > value_start &&
               (data[value_end - 1] == (unsigned char)' ' ||
                data[value_end - 1] == (unsigned char)'\t'))
        {
          value_end--;
        }
        value_length = value_end - value_start;
        if (!aura_http_header_value_valid(data + value_start, value_length))
        {
          free(body);
          return AURA_HTTP_PARSE_BAD_REQUEST;
        }
        headers[*header_count].name =
            aura_http_copy_string(data + cursor, name_length);
        headers[*header_count].value =
            aura_http_copy_string(data + value_start, value_length);
        if (headers[*header_count].name == NULL ||
            headers[*header_count].value == NULL)
        {
          (*header_count)++;
          free(body);
          return AURA_HTTP_PARSE_ERROR;
        }
        (*header_count)++;
        cursor = trailer_end;
      }
    }
    if (chunk_length > AURA_HTTP_MAX_BODY_BYTES - body_length)
    {
      free(body);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    if (cursor > AURA_HTTP_MAX_TOTAL_BYTES ||
        chunk_length > AURA_HTTP_MAX_TOTAL_BYTES - cursor ||
        cursor > input_length || chunk_length > input_length - cursor ||
        input_length - cursor - chunk_length < 2)
    {
      free(body);
      return AURA_HTTP_PARSE_INCOMPLETE;
    }
    if (data[cursor + chunk_length] != (unsigned char)'\r' ||
        data[cursor + chunk_length + 1] != (unsigned char)'\n')
    {
      free(body);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    if (body_length + chunk_length > capacity)
    {
      size_t next_capacity = capacity == 0 ? 256 : capacity;
      while (next_capacity < body_length + chunk_length)
      {
        if (next_capacity > AURA_HTTP_MAX_BODY_BYTES / 2)
        {
          next_capacity = AURA_HTTP_MAX_BODY_BYTES;
          break;
        }
        next_capacity *= 2;
      }
      next_body = (unsigned char *)realloc(body, next_capacity);
      if (next_body == NULL)
      {
        free(body);
        return AURA_HTTP_PARSE_ERROR;
      }
      body = next_body;
      capacity = next_capacity;
    }
    memcpy(body + body_length, data + cursor, chunk_length);
    body_length += chunk_length;
    cursor += chunk_length + 2;
  }
}

static int aura_http_header_value_valid(const unsigned char *value, size_t length)
{
  size_t i;
  for (i = 0; i < length; i++)
  {
    unsigned char c = value[i];
    if (c == 0 || c == (unsigned char)'\r' || c == (unsigned char)'\n' ||
        (c < 0x20 && c != (unsigned char)'\t') || c == 0x7f)
    {
      return 0;
    }
  }
  return 1;
}

static AuraHttpParseStatus aura_http_request_parse_impl(
    const void *input, size_t input_length, AuraHttpRequest *out_request,
    size_t *out_consumed, int headers_only, size_t *out_header_end,
    size_t *out_content_length, int *out_chunked)
{
  const unsigned char *data = (const unsigned char *)input;
  AuraHttpRequest parsed;
  AuraHttpLineResult line_result;
  size_t request_line_end = 0;
  size_t request_line_length;
  size_t first_space = SIZE_MAX;
  size_t second_space = SIZE_MAX;
  size_t i;
  size_t cursor;
  size_t header_start;
  size_t header_end = 0;
  size_t content_length = 0;
  int has_content_length = 0;
  int chunked = 0;
  int method_allowed = 0;

  if (out_request == NULL || (input == NULL && input_length != 0))
  {
    return AURA_HTTP_PARSE_ERROR;
  }
  memset(out_request, 0, sizeof(*out_request));
  if (out_consumed != NULL)
  {
    *out_consumed = 0;
  }
  if (out_header_end != NULL)
  {
    *out_header_end = 0;
  }
  if (out_content_length != NULL)
  {
    *out_content_length = 0;
  }
  if (out_chunked != NULL)
  {
    *out_chunked = 0;
  }
  memset(&parsed, 0, sizeof(parsed));

  if (input_length == 0)
  {
    return AURA_HTTP_PARSE_INCOMPLETE;
  }
  line_result = aura_http_find_line(data, input_length, 0,
                                    AURA_HTTP_MAX_REQUEST_LINE_BYTES,
                                    &request_line_end);
  if (line_result != AURA_HTTP_LINE_FOUND)
  {
    return aura_http_line_status(line_result);
  }
  request_line_length = request_line_end - 2;
  for (i = 0; i < request_line_length; i++)
  {
    if (data[i] == (unsigned char)' ')
    {
      if (first_space == SIZE_MAX)
      {
        first_space = i;
      }
      else if (second_space == SIZE_MAX)
      {
        second_space = i;
      }
    }
  }
  if (first_space == SIZE_MAX || second_space == SIZE_MAX || first_space == 0 ||
      second_space <= first_space + 1 || second_space + 1 >= request_line_length)
  {
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }
  for (i = second_space + 1; i < request_line_length; i++)
  {
    if (data[i] == (unsigned char)' ' || data[i] == (unsigned char)'\t')
    {
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
  }
  for (i = 0; i < first_space; i++)
  {
    if (!aura_http_is_token(data[i]))
    {
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
  }
  if (data[first_space + 1] != (unsigned char)'/' ||
      second_space - first_space - 1 == 0)
  {
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }
  for (i = first_space + 1; i < second_space; i++)
  {
    unsigned char c = data[i];
    if (c < 0x21 || c == 0x7f)
    {
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
  }
  if (request_line_length - second_space - 1 != strlen("HTTP/1.1") ||
      memcmp(data + second_space + 1, "HTTP/1.1", strlen("HTTP/1.1")) != 0)
  {
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }

  parsed.method = aura_http_copy_string(data, first_space);
  parsed.target = aura_http_copy_string(data + first_space + 1,
                                        second_space - first_space - 1);
  parsed.version = aura_http_copy_string(data + second_space + 1,
                                         request_line_length - second_space - 1);
  if (parsed.method == NULL || parsed.target == NULL || parsed.version == NULL)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_ERROR;
  }
  method_allowed = aura_http_method_allowed(parsed.method);

  parsed.headers = (AuraHttpHeader *)calloc(AURA_HTTP_MAX_HEADERS,
                                             sizeof(*parsed.headers));
  if (parsed.headers == NULL)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_ERROR;
  }
  header_start = request_line_end;
  cursor = header_start;
  for (;;)
  {
    size_t line_end = 0;
    size_t line_content_end;
    size_t colon = SIZE_MAX;
    size_t name_length;
    size_t value_start;
    size_t value_end;
    size_t value_length;
    line_result = aura_http_find_line(data, input_length, cursor,
                                      AURA_HTTP_MAX_HEADER_BYTES, &line_end);
    if (line_result != AURA_HTTP_LINE_FOUND)
    {
      AuraHttpParseStatus status = aura_http_line_status(line_result);
      aura_http_request_destroy(&parsed);
      return status;
    }
    if (line_end - header_start > AURA_HTTP_MAX_HEADER_BYTES)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    line_content_end = line_end - 2;
    if (line_content_end == cursor)
    {
      header_end = line_end;
      break;
    }
    if (parsed.header_count == AURA_HTTP_MAX_HEADERS)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
    }
    for (i = cursor; i < line_content_end; i++)
    {
      if (data[i] == (unsigned char)':')
      {
        colon = i;
        break;
      }
    }
    if (colon == SIZE_MAX || colon == cursor)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    name_length = colon - cursor;
    for (i = cursor; i < colon; i++)
    {
      if (!aura_http_is_token(data[i]))
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
    }
    value_start = colon + 1;
    value_end = line_content_end;
    while (value_start < value_end &&
           (data[value_start] == (unsigned char)' ' ||
            data[value_start] == (unsigned char)'\t'))
    {
      value_start++;
    }
    while (value_end > value_start &&
           (data[value_end - 1] == (unsigned char)' ' ||
            data[value_end - 1] == (unsigned char)'\t'))
    {
      value_end--;
    }
    value_length = value_end - value_start;
    if (!aura_http_header_value_valid(data + value_start, value_length))
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_BAD_REQUEST;
    }
    if (aura_http_header_name_equal(data + cursor, name_length,
                                    "Transfer-Encoding"))
    {
      if (chunked || !aura_http_ascii_equal_ci(data + value_start, value_length,
                                                "chunked"))
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      chunked = 1;
    }
    if (aura_http_header_name_equal(data + cursor, name_length, "Content-Length"))
    {
      size_t candidate = 0;
      if (!aura_http_parse_content_length(data + value_start, value_length,
                                          &candidate))
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      if (has_content_length && candidate != content_length)
      {
        aura_http_request_destroy(&parsed);
        return AURA_HTTP_PARSE_BAD_REQUEST;
      }
      has_content_length = 1;
      content_length = candidate;
    }
    parsed.headers[parsed.header_count].name =
        aura_http_copy_string(data + cursor, name_length);
    parsed.headers[parsed.header_count].value =
        aura_http_copy_string(data + value_start, value_length);
    if (parsed.headers[parsed.header_count].name == NULL ||
        parsed.headers[parsed.header_count].value == NULL)
    {
      parsed.header_count++;
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_ERROR;
    }
    parsed.header_count++;
    cursor = line_end;
  }

  if (chunked && has_content_length)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_BAD_REQUEST;
  }
  if (!chunked && (content_length > AURA_HTTP_MAX_BODY_BYTES ||
                   header_end > AURA_HTTP_MAX_TOTAL_BYTES - content_length))
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
  }
  if (headers_only)
  {
    if (parsed.header_count == 0)
    {
      free(parsed.headers);
      parsed.headers = NULL;
    }
    else
    {
      AuraHttpHeader *shrunk = (AuraHttpHeader *)realloc(
          parsed.headers, parsed.header_count * sizeof(*parsed.headers));
      if (shrunk != NULL)
      {
        parsed.headers = shrunk;
      }
    }
    if (!method_allowed)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_METHOD_NOT_ALLOWED;
    }
    parsed.total_length = header_end;
    *out_request = parsed;
    if (out_consumed != NULL)
    {
      *out_consumed = header_end;
    }
    if (out_header_end != NULL)
    {
      *out_header_end = header_end;
    }
    if (out_content_length != NULL)
    {
      *out_content_length = content_length;
    }
    if (out_chunked != NULL)
    {
      *out_chunked = chunked;
    }
    return AURA_HTTP_PARSE_OK;
  }
  if (chunked)
  {
    AuraHttpParseStatus chunk_status = aura_http_decode_chunked_body(
        data, input_length, header_end, &parsed.body, &parsed.body_length,
        &parsed.total_length, parsed.headers, &parsed.header_count);
    if (chunk_status != AURA_HTTP_PARSE_OK)
    {
      aura_http_request_destroy(&parsed);
      return chunk_status;
    }
  }
  else
  {
    parsed.total_length = header_end + content_length;
    if (input_length < parsed.total_length)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_INCOMPLETE;
    }
    parsed.body_length = content_length;
    parsed.body = aura_http_copy_body(data + header_end, content_length);
    if (content_length != 0 && parsed.body == NULL)
    {
      aura_http_request_destroy(&parsed);
      return AURA_HTTP_PARSE_ERROR;
    }
  }
  if (parsed.header_count == 0)
  {
    free(parsed.headers);
    parsed.headers = NULL;
  }
  else
  {
    AuraHttpHeader *shrunk = (AuraHttpHeader *)realloc(
        parsed.headers, parsed.header_count * sizeof(*parsed.headers));
    if (shrunk != NULL)
    {
      parsed.headers = shrunk;
    }
  }
  if (!method_allowed)
  {
    aura_http_request_destroy(&parsed);
    return AURA_HTTP_PARSE_METHOD_NOT_ALLOWED;
  }
  *out_request = parsed;
  if (out_consumed != NULL)
  {
    *out_consumed = parsed.total_length;
  }
  return AURA_HTTP_PARSE_OK;
}

AuraHttpParseStatus aura_http_request_parse(const void *input, size_t input_length,
                                            AuraHttpRequest *out_request,
                                            size_t *out_consumed)
{
  return aura_http_request_parse_impl(input, input_length, out_request,
                                      out_consumed, 0, NULL, NULL, NULL);
}

/* Header-first parsing is the foundation for an async request-body reader.
 * It owns the same request metadata as the full parser but deliberately does
 * not consume or copy any body bytes. */
static AuraHttpParseStatus aura_http_request_parse_headers(
    const void *input, size_t input_length, AuraHttpRequest *out_request,
    size_t *out_header_end, size_t *out_content_length, int *out_chunked)
{
  return aura_http_request_parse_impl(input, input_length, out_request, NULL,
                                      1, out_header_end, out_content_length,
                                      out_chunked);
}

/* One Content-Length-framed body reader borrows the connection's unread
 * buffer. It never reads more than `remaining`, so bytes for a pipelined next
 * request remain untouched in the socket/buffer. The caller owns the output
 * buffer and waits for POLLIN when this returns AURA_TCP_PENDING. */
struct AuraHttpContentLengthReader
{
  AuraTcpStream *stream;
  unsigned char *buffer;
  size_t *used;
  size_t remaining;
  int timeout_ms;
  int read_active;
  int chunked;
  int chunk_state;
  unsigned char line[AURA_HTTP_MAX_HEADER_BYTES];
  size_t line_length;
};

static int aura_http_content_length_reader_init(
    AuraHttpContentLengthReader *reader, AuraTcpStream *stream,
    unsigned char *buffer, size_t *used, size_t content_length, int timeout_ms)
{
  if (reader == NULL || stream == NULL || used == NULL ||
      (*used != 0 && buffer == NULL) || timeout_ms < 0)
  {
    return 0;
  }
  reader->stream = stream;
  reader->buffer = buffer;
  reader->used = used;
  reader->remaining = content_length;
  reader->timeout_ms = timeout_ms;
  reader->chunked = 0;
  reader->chunk_state = 0;
  reader->line_length = 0;
  return 1;
}

static int aura_http_chunked_reader_init(
    AuraHttpContentLengthReader *reader, AuraTcpStream *stream,
    unsigned char *buffer, size_t *used, int timeout_ms)
{
  if (reader == NULL || stream == NULL || used == NULL ||
      (*used != 0 && buffer == NULL) || timeout_ms < 0)
  {
    return 0;
  }
  memset(reader, 0, sizeof(*reader));
  reader->stream = stream;
  reader->buffer = buffer;
  reader->used = used;
  reader->timeout_ms = timeout_ms;
  reader->chunked = 1;
  return 1;
}

static AuraTcpStatus aura_http_body_reader_byte(
    AuraHttpContentLengthReader *reader, unsigned char *out)
{
  size_t count = 0;
  AuraTcpStatus status;
  if (reader == NULL || out == NULL || reader->stream == NULL ||
      reader->used == NULL || (*reader->used != 0 && reader->buffer == NULL))
  {
    return AURA_TCP_ERROR;
  }
  if (*reader->used != 0)
  {
    *out = reader->buffer[0];
    memmove(reader->buffer, reader->buffer + 1, *reader->used - 1);
    *reader->used -= 1;
    return AURA_TCP_OK;
  }
  status = aura_tcp_stream_read(reader->stream, out, 1, &count, 0);
  if (status == AURA_TCP_OK && count != 1)
  {
    return AURA_TCP_ERROR;
  }
  return status;
}

static int aura_http_hex_value(unsigned char value)
{
  if (value >= (unsigned char)'0' && value <= (unsigned char)'9')
  {
    return (int)(value - (unsigned char)'0');
  }
  if (value >= (unsigned char)'a' && value <= (unsigned char)'f')
  {
    return (int)(value - (unsigned char)'a') + 10;
  }
  if (value >= (unsigned char)'A' && value <= (unsigned char)'F')
  {
    return (int)(value - (unsigned char)'A') + 10;
  }
  return -1;
}

static AuraTcpStatus aura_http_chunked_reader_read(
    AuraHttpContentLengthReader *reader, unsigned char *out, size_t capacity,
    size_t *out_bytes)
{
  size_t count = 0;
  if (out_bytes == NULL || out == NULL || capacity == 0 || reader == NULL ||
      !reader->chunked)
  {
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  for (;;)
  {
    unsigned char byte = 0;
    AuraTcpStatus status;
    if (reader->chunk_state == 4)
    {
      return AURA_TCP_EOF;
    }
    if (reader->chunk_state == 0)
    {
      status = aura_http_body_reader_byte(reader, &byte);
      if (status != AURA_TCP_OK)
      {
        return status;
      }
      if (reader->line_length >= sizeof(reader->line))
      {
        return AURA_TCP_ERROR;
      }
      reader->line[reader->line_length++] = byte;
      if (reader->line_length >= 2 &&
          reader->line[reader->line_length - 2] == (unsigned char)'\r' &&
          reader->line[reader->line_length - 1] == (unsigned char)'\n')
      {
        size_t i;
        size_t digits = reader->line_length - 2;
        size_t size = 0;
        if (digits == 0 || digits > 16)
        {
          return AURA_TCP_ERROR;
        }
        for (i = 0; i < digits; i++)
        {
          int value = aura_http_hex_value(reader->line[i]);
          if (value < 0 || size > (SIZE_MAX - (size_t)value) / 16)
          {
            return AURA_TCP_ERROR;
          }
          size = size * 16 + (size_t)value;
        }
        reader->line_length = 0;
        reader->remaining = size;
        reader->chunk_state = size == 0 ? 3 : 1;
      }
      continue;
    }
    if (reader->chunk_state == 1)
    {
      size_t want = reader->remaining < capacity ? reader->remaining : capacity;
      if (*reader->used != 0)
      {
        count = *reader->used < want ? *reader->used : want;
        memcpy(out, reader->buffer, count);
        memmove(reader->buffer, reader->buffer + count, *reader->used - count);
        *reader->used -= count;
        status = AURA_TCP_OK;
      }
      else
      {
        status = aura_tcp_stream_read(reader->stream, out, want, &count, 0);
      }
      if (status != AURA_TCP_OK)
      {
        return status;
      }
      if (count == 0 || count > reader->remaining)
      {
        return AURA_TCP_ERROR;
      }
      reader->remaining -= count;
      *out_bytes = count;
      if (reader->remaining == 0)
      {
        reader->chunk_state = 2;
      }
      return AURA_TCP_OK;
    }
    if (reader->chunk_state == 2)
    {
      status = aura_http_body_reader_byte(reader, &byte);
      if (status != AURA_TCP_OK || byte != (unsigned char)'\r')
      {
        return AURA_TCP_ERROR;
      }
      status = aura_http_body_reader_byte(reader, &byte);
      if (status != AURA_TCP_OK || byte != (unsigned char)'\n')
      {
        return AURA_TCP_ERROR;
      }
      reader->chunk_state = 0;
      continue;
    }
    /* Consume and validate trailers before publishing EOF. Trailer fields
     * are not exposed by the streaming reader, but framing fields must still
     * be rejected instead of being silently accepted. */
    status = aura_http_body_reader_byte(reader, &byte);
    if (status != AURA_TCP_OK)
    {
      return status;
    }
    if (reader->line_length >= sizeof(reader->line))
    {
      return AURA_TCP_ERROR;
    }
    reader->line[reader->line_length++] = byte;
    if (reader->line_length >= 2 &&
        reader->line[reader->line_length - 2] == (unsigned char)'\r' &&
        reader->line[reader->line_length - 1] == (unsigned char)'\n')
    {
      size_t line_end = reader->line_length - 2;
      size_t colon = SIZE_MAX;
      size_t value_start;
      size_t value_end = line_end;
      size_t i;
      if (line_end == 0)
      {
        reader->line_length = 0;
        reader->chunk_state = 4;
        return AURA_TCP_EOF;
      }
      for (i = 0; i < line_end; i++)
      {
        if (reader->line[i] == (unsigned char)':')
        {
          colon = i;
          break;
        }
      }
      if (colon == SIZE_MAX || colon == 0)
      {
        return AURA_TCP_ERROR;
      }
      for (i = 0; i < colon; i++)
      {
        if (!aura_http_is_token(reader->line[i]))
        {
          return AURA_TCP_ERROR;
        }
      }
      if (aura_http_header_name_equal(reader->line, colon,
                                      "Content-Length") ||
          aura_http_header_name_equal(reader->line, colon,
                                      "Transfer-Encoding") ||
          aura_http_header_name_equal(reader->line, colon, "Trailer"))
      {
        return AURA_TCP_ERROR;
      }
      value_start = colon + 1;
      while (value_start < value_end &&
             (reader->line[value_start] == (unsigned char)' ' ||
              reader->line[value_start] == (unsigned char)'\t'))
      {
        value_start++;
      }
      while (value_end > value_start &&
             (reader->line[value_end - 1] == (unsigned char)' ' ||
              reader->line[value_end - 1] == (unsigned char)'\t'))
      {
        value_end--;
      }
      if (!aura_http_header_value_valid(reader->line + value_start,
                                        value_end - value_start))
      {
        return AURA_TCP_ERROR;
      }
      reader->line_length = 0;
    }
  }
}

static AuraTcpStatus aura_http_content_length_reader_read(
    AuraHttpContentLengthReader *reader, unsigned char *out, size_t capacity,
    size_t *out_bytes)
{
  size_t available;
  size_t count;
  AuraTcpStatus status;

  if (out_bytes == NULL || (out == NULL && capacity != 0) || reader == NULL ||
      reader->stream == NULL || reader->used == NULL ||
      (*reader->used != 0 && reader->buffer == NULL) || capacity == 0)
  {
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  if (reader->remaining == 0)
  {
    return AURA_TCP_EOF;
  }
  available = *reader->used;
  if (available != 0)
  {
    count = available;
    if (count > reader->remaining)
    {
      count = reader->remaining;
    }
    if (count > capacity)
    {
      count = capacity;
    }
    memcpy(out, reader->buffer, count);
    memmove(reader->buffer, reader->buffer + count, available - count);
    *reader->used = available - count;
    reader->remaining -= count;
    *out_bytes = count;
    return AURA_TCP_OK;
  }
  count = capacity < reader->remaining ? capacity : reader->remaining;
  status = aura_tcp_stream_read(reader->stream, out, count, &count, 0);
  if (status == AURA_TCP_OK)
  {
    if (count == 0 || count > reader->remaining)
    {
      return AURA_TCP_ERROR;
    }
    reader->remaining -= count;
    *out_bytes = count;
  }
  return status;
}

/* The reader is connection-owned and exists only while its handler runs. */
int aura_http_request_read_body(
    const AuraHttpRequest *request, unsigned char *out, size_t capacity,
    size_t *out_bytes)
{
  if (request == NULL || request->body_reader == NULL)
  {
    return AURA_TCP_ERROR;
  }
  if (request->body_reader->chunked)
  {
    return aura_http_chunked_reader_read(request->body_reader, out, capacity,
                                         out_bytes);
  }
  return aura_http_content_length_reader_read(request->body_reader, out,
                                              capacity, out_bytes);
}

int aura_http_request_body_read_begin(const AuraHttpRequest *request)
{
  AuraHttpContentLengthReader *reader;
  if (request == NULL || request->body_reader == NULL)
  {
    return 0;
  }
  reader = request->body_reader;
  if (reader->read_active)
  {
    return 0;
  }
  reader->read_active = 1;
  return 1;
}

void aura_http_request_body_read_end(const AuraHttpRequest *request)
{
  if (request != NULL && request->body_reader != NULL)
  {
    request->body_reader->read_active = 0;
  }
}
