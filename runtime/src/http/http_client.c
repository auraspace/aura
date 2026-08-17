/* Bounded one-shot HTTP client used by the binary fetch intrinsic. */

static void aura_http_client_error(char **out_error, const char *message)
{
  if (out_error == NULL) return;
  *out_error = NULL;
  if (message == NULL) return;
  size_t length = strlen(message);
  char *copy = (char *)malloc(length + 1);
  if (copy == NULL) return;
  memcpy(copy, message, length + 1);
  *out_error = copy;
}

static int aura_http_client_header_name_equal(const char *value, size_t length,
                                              const char *expected)
{
  size_t expected_length = strlen(expected);
  if (length != expected_length) return 0;
  for (size_t i = 0; i < length; i++)
  {
    char left = value[i];
    char right = expected[i];
    if (left >= 'A' && left <= 'Z') left = (char)(left + ('a' - 'A'));
    if (right >= 'A' && right <= 'Z') right = (char)(right + ('a' - 'A'));
    if (left != right) return 0;
  }
  return 1;
}

static int aura_http_client_content_length(const char *headers, size_t length,
                                           size_t *out_length)
{
  const char *cursor = headers;
  const char *end = headers + length;
  while (cursor < end)
  {
    const char *line_end = strstr(cursor, "\r\n");
    if (line_end == NULL || line_end > end) line_end = end;
    const char *colon = memchr(cursor, ':', (size_t)(line_end - cursor));
    if (colon != NULL)
    {
      size_t name_length = (size_t)(colon - cursor);
      if (name_length == 14 && aura_http_client_header_name_equal(cursor, name_length, "Content-Length"))
      {
        const char *value = colon + 1;
        while (value < line_end && (*value == ' ' || *value == '\t')) value++;
        if (value == line_end) return 0;
        errno = 0;
        char *parsed_end = NULL;
        unsigned long long parsed = strtoull(value, &parsed_end, 10);
        while (parsed_end < line_end && (*parsed_end == ' ' || *parsed_end == '\t')) parsed_end++;
        if (errno == ERANGE || parsed_end != line_end || parsed > SIZE_MAX) return 0;
        *out_length = (size_t)parsed;
        return 1;
      }
    }
    if (line_end == end) break;
    cursor = line_end + 2;
  }
  return 0;
}

static int aura_http_client_is_chunked(const char *headers, size_t length)
{
  const char *cursor = headers;
  const char *end = headers + length;
  while (cursor < end)
  {
    const char *line_end = strstr(cursor, "\r\n");
    if (line_end == NULL || line_end > end) line_end = end;
    const char *colon = memchr(cursor, ':', (size_t)(line_end - cursor));
    if (colon != NULL && (size_t)(colon - cursor) == 17 &&
        aura_http_client_header_name_equal(cursor, 17, "Transfer-Encoding"))
    {
      const char *value = colon + 1;
      while (value < line_end && (*value == ' ' || *value == '\t')) value++;
      size_t value_length = (size_t)(line_end - value);
      return value_length == 7 && strncasecmp(value, "chunked", 7) == 0;
    }
    if (line_end == end) break;
    cursor = line_end + 2;
  }
  return 0;
}

static int aura_http_client_read_line(AuraTcpStream *stream, char *line, size_t capacity)
{
  size_t used = 0;
  while (used + 1 < capacity)
  {
    unsigned char byte = 0;
    size_t count = 0;
    if (aura_tcp_stream_read(stream, &byte, 1, &count, 1000) != AURA_TCP_OK || count != 1)
      return 0;
    line[used++] = (char)byte;
    if (used >= 2 && line[used - 2] == '\r' && line[used - 1] == '\n')
    {
      line[used] = '\0';
      return 1;
    }
  }
  return 0;
}

static int aura_http_client_read_chunked(AuraTcpStream *stream, size_t max_bytes,
                                         unsigned char **out_body, size_t *out_length)
{
  unsigned char *body = NULL;
  size_t length = 0;
  char line[128];
  for (;;)
  {
    char *end = NULL;
    unsigned long long chunk = 0;
    if (!aura_http_client_read_line(stream, line, sizeof(line))) goto fail;
    errno = 0;
    chunk = strtoull(line, &end, 16);
    while (end != NULL && *end != '\0' && *end != ';' && *end != '\r' && *end != '\n') end++;
    if (errno == ERANGE || end == line || chunk > SIZE_MAX || chunk > max_bytes - length) goto fail;
    if (chunk == 0)
    {
      do { if (!aura_http_client_read_line(stream, line, sizeof(line))) goto fail; }
      while (strcmp(line, "\r\n") != 0);
      if (body == NULL) body = (unsigned char *)malloc(1);
      if (body == NULL) goto fail;
      *out_body = body; *out_length = length; return 1;
    }
    if (length > SIZE_MAX - (size_t)chunk) goto fail;
    unsigned char *next = (unsigned char *)realloc(body, length + (size_t)chunk);
    if (next == NULL) goto fail;
    body = next;
    size_t received = 0;
    if (aura_tcp_stream_read_exactly(stream, body + length, (size_t)chunk, &received, 1000) != AURA_TCP_OK || received != (size_t)chunk)
      goto fail;
    length += (size_t)chunk;
    unsigned char crlf[2];
    if (aura_tcp_stream_read_exactly(stream, crlf, 2, &received, 1000) != AURA_TCP_OK || received != 2 || crlf[0] != '\r' || crlf[1] != '\n') goto fail;
  }
fail:
  free(body);
  return 0;
}

int aura_http_client_get_bytes(const char *endpoint, const char *target,
                               size_t max_bytes, unsigned char **out_bytes,
                               size_t *out_length, char **out_error)
{
  if (out_bytes == NULL || out_length == NULL || endpoint == NULL || target == NULL ||
      endpoint[0] == '\0' || target[0] != '/' || strstr(endpoint, "\r") != NULL ||
      strstr(endpoint, "\n") != NULL || strstr(target, "\r") != NULL ||
      strstr(target, "\n") != NULL || max_bytes == 0)
  {
    aura_http_client_error(out_error, "invalid HTTP binary fetch arguments");
    return 0;
  }
  *out_bytes = NULL;
  *out_length = 0;
  if (out_error != NULL) *out_error = NULL;

  if (strncmp(endpoint, "https://", 8) == 0 || strncmp(endpoint, "tls://", 6) == 0)
  {
    return aura_tls_http_client_get_bytes(endpoint, target, max_bytes, out_bytes,
                                          out_length, out_error);
  }

  AuraTcpStream *stream = NULL;
  if (aura_tcp_stream_connect_endpoint(endpoint, 1000, &stream) != AURA_TCP_OK || stream == NULL)
  {
    aura_http_client_error(out_error, "upstream connection failed");
    return 0;
  }

  size_t request_length = strlen(target) + strlen(endpoint) + 64;
  char *request = (char *)malloc(request_length);
  if (request == NULL)
  {
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream request allocation failed");
    return 0;
  }
  int written = snprintf(request, request_length,
                         "GET %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n\r\n",
                         target, endpoint);
  size_t request_size = written < 0 ? 0 : (size_t)written;
  size_t sent = 0;
  AuraTcpStatus status = request_size == 0 ||
                         aura_tcp_stream_write_all(stream, request, request_size, &sent, 1000) != AURA_TCP_OK ||
                         sent != request_size ? AURA_TCP_ERROR : AURA_TCP_OK;
  free(request);
  if (status != AURA_TCP_OK)
  {
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream request failed");
    return 0;
  }

  const size_t header_limit = 65536;
  unsigned char *headers = (unsigned char *)malloc(header_limit + 1);
  if (headers == NULL)
  {
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream header allocation failed");
    return 0;
  }
  size_t header_length = 0;
  int complete = 0;
  while (header_length < header_limit)
  {
    size_t count = 0;
    status = aura_tcp_stream_read(stream, headers + header_length, 1, &count, 1000);
    if (status != AURA_TCP_OK || count != 1)
    {
      free(headers);
      aura_tcp_stream_destroy(stream);
      aura_http_client_error(out_error, "upstream response headers failed");
      return 0;
    }
    header_length++;
    headers[header_length] = '\0';
    if (header_length >= 4 && memcmp(headers + header_length - 4, "\r\n\r\n", 4) == 0)
    {
      complete = 1;
      break;
    }
  }
  if (!complete)
  {
    free(headers);
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream response headers exceed limit");
    return 0;
  }

  if (header_length < 12 || memcmp(headers, "HTTP/1.", 7) != 0 || headers[9] < '0' ||
      headers[9] > '9' || headers[10] < '0' || headers[10] > '9' || headers[11] < '0' ||
      headers[11] > '9' || (headers[9] - '0') * 100 + (headers[10] - '0') * 10 +
      (headers[11] - '0') != 200)
  {
    free(headers);
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream returned a non-success status");
    return 0;
  }

  size_t body_length = 0;
  int chunked = aura_http_client_is_chunked((const char *)headers, header_length);
  if (!chunked && (!aura_http_client_content_length((const char *)headers, header_length, &body_length) ||
                   body_length > max_bytes))
  {
    free(headers);
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream binary response has no bounded Content-Length");
    return 0;
  }
  free(headers);

  if (chunked)
  {
    int ok = aura_http_client_read_chunked(stream, max_bytes, out_bytes, out_length);
    aura_tcp_stream_destroy(stream);
    if (!ok) aura_http_client_error(out_error, "upstream chunked body failed");
    return ok;
  }

  unsigned char *body = (unsigned char *)malloc(body_length == 0 ? 1 : body_length);
  if (body == NULL)
  {
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream body allocation failed");
    return 0;
  }
  size_t received = 0;
  if (body_length != 0 &&
      aura_tcp_stream_read_exactly(stream, body, body_length, &received, 1000) != AURA_TCP_OK)
  {
    free(body);
    aura_tcp_stream_destroy(stream);
    aura_http_client_error(out_error, "upstream binary body failed");
    return 0;
  }
  aura_tcp_stream_destroy(stream);
  *out_bytes = body;
  *out_length = body_length;
  return 1;
}
