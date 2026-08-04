static int aura_http_body_reader_complete(
    const AuraHttpContentLengthReader *reader)
{
  if (reader == NULL)
  {
    return 0;
  }
  return reader->chunked ? reader->chunk_state == 4 : reader->remaining == 0;
}

/* ---- Bounded HTTP/1.1 response builder (transport-independent) ----
 *
 * The builder owns copies of caller-provided headers and body bytes.  It is
 * deliberately a final-response API: informational (1xx) responses,
 * transfer encoding, and caller-supplied Content-Length/Connection headers
 * are outside the alpha contract.  Content-Length and Connection are always
 * emitted by the serializer in a fixed order.
 */

#define AURA_HTTP_MAX_RESPONSE_HEADERS ((size_t)64)
#define AURA_HTTP_MAX_RESPONSE_HEADER_BYTES ((size_t)16 * 1024)
#define AURA_HTTP_MAX_RESPONSE_BODY_BYTES ((size_t)8 * 1024 * 1024)
#define AURA_HTTP_MAX_RESPONSE_BYTES ((size_t)16 * 1024 * 1024)

typedef enum
{
  AURA_HTTP_RESPONSE_OK = 0,
  AURA_HTTP_RESPONSE_INVALID = -1,
  AURA_HTTP_RESPONSE_TOO_LARGE = -2,
  AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL = -3,
  AURA_HTTP_RESPONSE_ALLOCATION = -4
} AuraHttpResponseStatus;

typedef enum
{
  AURA_HTTP_RESPONSE_CLOSE = 0,
  AURA_HTTP_RESPONSE_KEEP_ALIVE = 1
} AuraHttpResponseConnection;

typedef struct AuraHttpResponse
{
  int status_code;
  AuraHttpHeader *headers;
  size_t header_count;
  unsigned char *body;
  size_t body_length;
  AuraHttpResponseConnection connection;
  int streaming_committed;
} AuraHttpResponse;

static int aura_http_response_status_valid(int status_code)
{
  return status_code >= 200 && status_code <= 599;
}

static int aura_http_response_forbids_body(int status_code)
{
  return status_code == 204 || status_code == 304;
}

static const char *aura_http_response_reason(int status_code)
{
  switch (status_code)
  {
  case 200: return "OK";
  case 201: return "Created";
  case 202: return "Accepted";
  case 203: return "Non-Authoritative Information";
  case 204: return "No Content";
  case 205: return "Reset Content";
  case 206: return "Partial Content";
  case 300: return "Multiple Choices";
  case 301: return "Moved Permanently";
  case 302: return "Found";
  case 304: return "Not Modified";
  case 400: return "Bad Request";
  case 401: return "Unauthorized";
  case 403: return "Forbidden";
  case 404: return "Not Found";
  case 405: return "Method Not Allowed";
  case 408: return "Request Timeout";
  case 409: return "Conflict";
  case 413: return "Payload Too Large";
  case 415: return "Unsupported Media Type";
  case 429: return "Too Many Requests";
  case 500: return "Internal Server Error";
  case 501: return "Not Implemented";
  case 502: return "Bad Gateway";
  case 503: return "Service Unavailable";
  case 504: return "Gateway Timeout";
  default: return "Unknown";
  }
}

static int aura_http_response_size_add(size_t *total, size_t value)
{
  if (value > SIZE_MAX - *total)
  {
    return 0;
  }
  *total += value;
  return 1;
}

static int aura_http_response_header_bytes(const AuraHttpResponse *response,
                                           size_t *out_bytes)
{
  size_t i;
  size_t total = 0;
  if (response == NULL || out_bytes == NULL)
  {
    return 0;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (response->headers[i].name == NULL || response->headers[i].value == NULL)
    {
      return 0;
    }
    size_t name_length = strlen(response->headers[i].name);
    size_t value_length = strlen(response->headers[i].value);
    if (!aura_http_response_size_add(&total, name_length) ||
        !aura_http_response_size_add(&total, 2) ||
        !aura_http_response_size_add(&total, value_length) ||
        !aura_http_response_size_add(&total, 2))
    {
      return 0;
    }
  }
  *out_bytes = total;
  return 1;
}

static AuraHttpResponseStatus aura_http_response_validate(
    const AuraHttpResponse *response)
{
  size_t i;
  size_t header_bytes;
  if (response == NULL || !aura_http_response_status_valid(response->status_code) ||
      response->header_count > AURA_HTTP_MAX_RESPONSE_HEADERS - 2 ||
      (response->body == NULL && response->body_length != 0) ||
      response->body_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES ||
      (aura_http_response_forbids_body(response->status_code) &&
       response->body_length != 0) ||
      (response->connection != AURA_HTTP_RESPONSE_CLOSE &&
       response->connection != AURA_HTTP_RESPONSE_KEEP_ALIVE) ||
      (response->headers == NULL && response->header_count != 0))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  if (!aura_http_response_header_bytes(response, &header_bytes) ||
      header_bytes > AURA_HTTP_MAX_RESPONSE_HEADER_BYTES)
  {
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  for (i = 0; i < response->header_count; i++)
  {
    size_t j;
    const char *name = response->headers[i].name;
    const char *value = response->headers[i].value;
    if (name == NULL || value == NULL || name[0] == '\0' ||
        !aura_http_is_token((unsigned char)name[0]) ||
        !aura_http_header_value_valid((const unsigned char *)value, strlen(value)) ||
        aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                    "Content-Length") ||
        aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                    "Connection"))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
    for (j = 0; name[j] != '\0'; j++)
    {
      if (!aura_http_is_token((unsigned char)name[j]))
      {
        return AURA_HTTP_RESPONSE_INVALID;
      }
    }
    for (j = i + 1; j < response->header_count; j++)
    {
      if (response->headers[j].name != NULL &&
          aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                      response->headers[j].name))
      {
        return AURA_HTTP_RESPONSE_INVALID;
      }
    }
  }
  return AURA_HTTP_RESPONSE_OK;
}

void aura_http_response_init(AuraHttpResponse *response)
{
  if (response == NULL)
  {
    return;
  }
  memset(response, 0, sizeof(*response));
  response->status_code = 200;
  response->connection = AURA_HTTP_RESPONSE_CLOSE;
}

void aura_http_response_destroy(AuraHttpResponse *response)
{
  size_t i;
  if (response == NULL)
  {
    return;
  }
  for (i = 0; i < response->header_count; i++)
  {
    free(response->headers[i].name);
    free(response->headers[i].value);
  }
  free(response->headers);
  free(response->body);
  memset(response, 0, sizeof(*response));
}

int aura_http_response_status(const AuraHttpResponse *response)
{
  return response == NULL ? 0 : response->status_code;
}

size_t aura_http_response_header_count(const AuraHttpResponse *response)
{
  return response == NULL ? 0 : response->header_count;
}

const char *aura_http_response_header_name(const AuraHttpResponse *response,
                                           size_t index)
{
  if (response == NULL || index >= response->header_count)
  {
    return NULL;
  }
  return response->headers[index].name;
}

const char *aura_http_response_header_value(const AuraHttpResponse *response,
                                            size_t index)
{
  if (response == NULL || index >= response->header_count)
  {
    return NULL;
  }
  return response->headers[index].value;
}

const unsigned char *aura_http_response_body(const AuraHttpResponse *response)
{
  return response == NULL ? NULL : response->body;
}

size_t aura_http_response_body_length(const AuraHttpResponse *response)
{
  return response == NULL ? 0 : response->body_length;
}

int aura_http_response_keep_alive(const AuraHttpResponse *response)
{
  return response != NULL && response->connection == AURA_HTTP_RESPONSE_KEEP_ALIVE;
}

int aura_http_response_stream_started(const AuraHttpResponse *response)
{
  return response != NULL && response->streaming_committed;
}

AuraHttpResponseStatus aura_http_response_set_status(AuraHttpResponse *response,
                                                      int status_code)
{
  if (response == NULL || !aura_http_response_status_valid(status_code) ||
      response->streaming_committed ||
      (aura_http_response_forbids_body(status_code) && response->body_length != 0))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  response->status_code = status_code;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_set_connection(
    AuraHttpResponse *response, AuraHttpResponseConnection connection)
{
  if (response == NULL ||
      response->streaming_committed ||
      (connection != AURA_HTTP_RESPONSE_CLOSE &&
       connection != AURA_HTTP_RESPONSE_KEEP_ALIVE))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  response->connection = connection;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_add_header(AuraHttpResponse *response,
                                                      const char *name,
                                                      const char *value)
{
  size_t i;
  size_t header_bytes;
  AuraHttpHeader *grown;
  char *name_copy;
  char *value_copy;
  if (response == NULL || name == NULL || value == NULL || name[0] == '\0' ||
      response->streaming_committed ||
      response->header_count >= AURA_HTTP_MAX_RESPONSE_HEADERS - 2 ||
      !aura_http_is_token((unsigned char)name[0]))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  for (i = 0; name[i] != '\0'; i++)
  {
    if (!aura_http_is_token((unsigned char)name[i]))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
  }
  if (!aura_http_header_value_valid((const unsigned char *)value, strlen(value)) ||
      aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                  "Content-Length") ||
      aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                  "Connection"))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (aura_http_header_name_equal((const unsigned char *)name, strlen(name),
                                    response->headers[i].name))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
  }
  if (!aura_http_response_header_bytes(response, &header_bytes) ||
      !aura_http_response_size_add(&header_bytes, strlen(name)) ||
      !aura_http_response_size_add(&header_bytes, 2) ||
      !aura_http_response_size_add(&header_bytes, strlen(value)) ||
      !aura_http_response_size_add(&header_bytes, 2) ||
      header_bytes > AURA_HTTP_MAX_RESPONSE_HEADER_BYTES)
  {
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  name_copy = aura_http_copy_string((const unsigned char *)name, strlen(name));
  value_copy = aura_http_copy_string((const unsigned char *)value, strlen(value));
  if (name_copy == NULL || value_copy == NULL)
  {
    free(name_copy);
    free(value_copy);
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  grown = (AuraHttpHeader *)realloc(response->headers,
                                    (response->header_count + 1) * sizeof(*grown));
  if (grown == NULL)
  {
    free(name_copy);
    free(value_copy);
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  response->headers = grown;
  response->headers[response->header_count].name = name_copy;
  response->headers[response->header_count].value = value_copy;
  response->header_count++;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_set_body(AuraHttpResponse *response,
                                                    const void *body,
                                                    size_t body_length)
{
  unsigned char *copy = NULL;
  if (response == NULL || (body == NULL && body_length != 0) ||
      response->streaming_committed ||
      body_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES ||
      (aura_http_response_forbids_body(response->status_code) && body_length != 0))
  {
    return body_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES
               ? AURA_HTTP_RESPONSE_TOO_LARGE
               : AURA_HTTP_RESPONSE_INVALID;
  }
  if (body_length != 0)
  {
    copy = (unsigned char *)malloc(body_length);
    if (copy == NULL)
    {
      return AURA_HTTP_RESPONSE_ALLOCATION;
    }
    memcpy(copy, body, body_length);
  }
  free(response->body);
  response->body = copy;
  response->body_length = body_length;
  return AURA_HTTP_RESPONSE_OK;
}

AuraHttpResponseStatus aura_http_response_set_error(AuraHttpResponse *response,
                                                     int status_code,
                                                     const char *error_code)
{
  char *body;
  int needed;
  AuraHttpResponseStatus result;
  if (response == NULL || error_code == NULL || error_code[0] == '\0' ||
      (status_code != 400 && status_code != 404 && status_code != 405 &&
       status_code != 408 && status_code != 413 && status_code != 500))
  {
    return AURA_HTTP_RESPONSE_INVALID;
  }
  for (size_t i = 0; error_code[i] != '\0'; i++)
  {
    if (!aura_http_is_token((unsigned char)error_code[i]))
    {
      return AURA_HTTP_RESPONSE_INVALID;
    }
  }
  needed = snprintf(NULL, 0, "{\"error\":\"%s\"}", error_code);
  if (needed < 0)
  {
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  body = (char *)malloc((size_t)needed + 1);
  if (body == NULL)
  {
    return AURA_HTTP_RESPONSE_ALLOCATION;
  }
  snprintf(body, (size_t)needed + 1, "{\"error\":\"%s\"}", error_code);
  result = aura_http_response_set_status(response, status_code);
  if (result == AURA_HTTP_RESPONSE_OK)
  {
    result = aura_http_response_set_connection(response, AURA_HTTP_RESPONSE_CLOSE);
  }
  if (result == AURA_HTTP_RESPONSE_OK)
  {
    result = aura_http_response_set_body(response, body, (size_t)needed);
  }
  free(body);
  if (result == AURA_HTTP_RESPONSE_OK)
  {
    result = aura_http_response_add_header(response, "Content-Type",
                                           "application/json");
  }
  return result;
}

static int aura_http_response_append(char *output, size_t capacity, size_t *cursor,
                                     const void *data, size_t length);

/* Streaming responses commit HTTP/1.1 headers before any body bytes. Their
 * chunk framing is owned by the connection task, never by application code. */
static int aura_http_response_has_transfer_encoding(const AuraHttpResponse *response)
{
  size_t i;
  for (i = 0; i < response->header_count; i++)
  {
    if (aura_http_header_name_equal(
            (const unsigned char *)response->headers[i].name,
            strlen(response->headers[i].name), "Transfer-Encoding"))
    {
      return 1;
    }
  }
  return 0;
}

int aura_http_response_stream_begin(
    AuraHttpResponse *response, void *output, size_t capacity, size_t *out_length)
{
  const char *reason;
  const char *connection;
  char status_line[64];
  size_t cursor = 0;
  size_t i;
  int status_size;

  if (response == NULL || out_length == NULL ||
      (output == NULL && capacity != 0) || response->streaming_committed ||
      response->body_length != 0 || aura_http_response_forbids_body(response->status_code) ||
      aura_http_response_has_transfer_encoding(response))
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  if (aura_http_response_validate(response) != AURA_HTTP_RESPONSE_OK)
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_INVALID;
  }
  reason = aura_http_response_reason(response->status_code);
  connection = response->connection == AURA_HTTP_RESPONSE_KEEP_ALIVE
                   ? "keep-alive" : "close";
  status_size = snprintf(status_line, sizeof(status_line), "HTTP/1.1 %d %s\r\n",
                         response->status_code, reason);
  if (status_size < 0 || (size_t)status_size >= sizeof(status_line) ||
      !aura_http_response_append((char *)output, capacity, &cursor, status_line,
                                 (size_t)status_size))
  {
    *out_length = cursor;
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (!aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].name,
                                   strlen(response->headers[i].name)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, ": ", 2) ||
        !aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].value,
                                   strlen(response->headers[i].value)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, "\r\n", 2))
    {
      *out_length = cursor;
      return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
    }
  }
  if (!aura_http_response_append((char *)output, capacity, &cursor,
                                 "Transfer-Encoding: chunked\r\n", 28) ||
      !aura_http_response_append((char *)output, capacity, &cursor,
                                 "Connection: ", 12) ||
      !aura_http_response_append((char *)output, capacity, &cursor, connection,
                                 strlen(connection)) ||
      !aura_http_response_append((char *)output, capacity, &cursor, "\r\n\r\n", 4))
  {
    *out_length = cursor;
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  *out_length = cursor;
  if (cursor > AURA_HTTP_MAX_RESPONSE_BYTES || output == NULL || capacity < cursor)
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  response->streaming_committed = 1;
  return AURA_HTTP_RESPONSE_OK;
}

int aura_http_response_stream_chunk(
    const void *chunk, size_t chunk_length, void *output, size_t capacity,
    size_t *out_length)
{
  char size_line[32];
  int size_length;
  size_t cursor = 0;
  if (out_length == NULL || (chunk == NULL && chunk_length != 0) ||
      (output == NULL && capacity != 0) || chunk_length == 0 ||
      chunk_length > AURA_HTTP_MAX_RESPONSE_BODY_BYTES)
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  size_length = snprintf(size_line, sizeof(size_line), "%zx\r\n", chunk_length);
  if (size_length < 0 || (size_t)size_length >= sizeof(size_line) ||
      !aura_http_response_append((char *)output, capacity, &cursor, size_line,
                                 (size_t)size_length) ||
      !aura_http_response_append((char *)output, capacity, &cursor, chunk,
                                 chunk_length) ||
      !aura_http_response_append((char *)output, capacity, &cursor, "\r\n", 2))
  {
    *out_length = cursor;
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  *out_length = cursor;
  return output == NULL || capacity < cursor ? AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL
                                               : AURA_HTTP_RESPONSE_OK;
}

int aura_http_response_stream_finish(
    const AuraHttpResponse *response, void *output, size_t capacity,
    size_t *out_length)
{
  static const char terminal[] = "0\r\n\r\n";
  if (out_length == NULL || response == NULL || !response->streaming_committed ||
      (output == NULL && capacity != 0))
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  *out_length = sizeof(terminal) - 1;
  if (output == NULL || capacity < sizeof(terminal) - 1)
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  memcpy(output, terminal, sizeof(terminal) - 1);
  return AURA_HTTP_RESPONSE_OK;
}

static int aura_http_response_append(char *output, size_t capacity, size_t *cursor,
                                     const void *data, size_t length)
{
  if (output != NULL)
  {
    if (*cursor > capacity || length > capacity - *cursor)
    {
      return 0;
    }
    if (length != 0)
    {
      memcpy(output + *cursor, data, length);
    }
  }
  *cursor += length;
  return 1;
}

AuraHttpResponseStatus aura_http_response_serialize(const AuraHttpResponse *response,
                                                     void *output, size_t capacity,
                                                     size_t *out_length)
{
  const char *reason;
  const char *connection;
  size_t required = 0;
  size_t i;
  size_t cursor = 0;
  char status_line[64];
  char content_length[64];
  int status_size;
  int length_size;
  if (out_length == NULL || (output == NULL && capacity != 0))
  {
    if (out_length != NULL)
    {
      *out_length = 0;
    }
    return AURA_HTTP_RESPONSE_INVALID;
  }
  {
    AuraHttpResponseStatus validation = aura_http_response_validate(response);
    if (validation != AURA_HTTP_RESPONSE_OK)
    {
      *out_length = 0;
      return validation;
    }
  }
  reason = aura_http_response_reason(response->status_code);
  connection = response->connection == AURA_HTTP_RESPONSE_KEEP_ALIVE
                   ? "keep-alive" : "close";
  status_size = snprintf(status_line, sizeof(status_line), "HTTP/1.1 %d %s\r\n",
                         response->status_code, reason);
  length_size = snprintf(content_length, sizeof(content_length),
                         "Content-Length: %zu\r\n", response->body_length);
  if (status_size < 0 || length_size < 0 ||
      (size_t)status_size >= sizeof(status_line) ||
      (size_t)length_size >= sizeof(content_length))
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_INVALID;
  }
  if (!aura_http_response_size_add(&required, (size_t)status_size))
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (!aura_http_response_size_add(&required, strlen(response->headers[i].name)) ||
        !aura_http_response_size_add(&required, 2) ||
        !aura_http_response_size_add(&required, strlen(response->headers[i].value)) ||
        !aura_http_response_size_add(&required, 2))
    {
      *out_length = 0;
      return AURA_HTTP_RESPONSE_TOO_LARGE;
    }
  }
  if (!aura_http_response_size_add(&required, (size_t)length_size) ||
      !aura_http_response_size_add(&required, strlen("Connection: ")) ||
      !aura_http_response_size_add(&required, strlen(connection)) ||
      !aura_http_response_size_add(&required, 2) ||
      !aura_http_response_size_add(&required, 2) ||
      !aura_http_response_size_add(&required, response->body_length) ||
      required > AURA_HTTP_MAX_RESPONSE_BYTES)
  {
    *out_length = 0;
    return AURA_HTTP_RESPONSE_TOO_LARGE;
  }
  *out_length = required;
  if (output == NULL || capacity < required)
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  if (!aura_http_response_append((char *)output, capacity, &cursor,
                                 status_line, (size_t)status_size))
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  for (i = 0; i < response->header_count; i++)
  {
    if (!aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].name,
                                   strlen(response->headers[i].name)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, ": ", 2) ||
        !aura_http_response_append((char *)output, capacity, &cursor,
                                   response->headers[i].value,
                                   strlen(response->headers[i].value)) ||
        !aura_http_response_append((char *)output, capacity, &cursor, "\r\n", 2))
    {
      return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
    }
  }
  if (!aura_http_response_append((char *)output, capacity, &cursor,
                                 content_length, (size_t)length_size) ||
      !aura_http_response_append((char *)output, capacity, &cursor,
                                 "Connection: ", strlen("Connection: ")) ||
      !aura_http_response_append((char *)output, capacity, &cursor, connection,
                                 strlen(connection)) ||
      !aura_http_response_append((char *)output, capacity, &cursor, "\r\n\r\n", 4) ||
      !aura_http_response_append((char *)output, capacity, &cursor,
                                 response->body, response->body_length))
  {
    return AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL;
  }
  return AURA_HTTP_RESPONSE_OK;
}

