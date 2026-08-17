/* Bounded OpenSSL TLS client used by std.tls and std.crypto. */
#if defined(AURA_TLS_ENABLE) && defined(__has_include)
#if __has_include(<openssl/ssl.h>)
#define AURA_TLS_OPENSSL 1
#endif
#endif

#if defined(AURA_TLS_OPENSSL) && (defined(__unix__) || defined(__APPLE__))
#include <openssl/err.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>

typedef struct
{
  char *endpoint;
  AuraTcpStream *stream;
  AuraFfiOpaqueHandle *owner;
  SSL_CTX *context;
  SSL *ssl;
  short pending_events;
  int active;
} AuraTlsEntry;

/* The FFI implementation is included later in runtime.c, so declare the
 * owner-release operation before this translation-unit slice uses it. */
extern AuraFfiStatus aura_ffi_handle_drop(AuraFfiOpaqueHandle **handle);

static AuraTlsEntry aura_tls_entries[32];
static char aura_tls_error[256] = "tls error";
static char *aura_tls_certificate_subject_value;
static char *aura_tls_certificate_issuer_value;
static int aura_tls_initialized;

static void aura_tls_set_error(const char *message)
{
  unsigned long code = ERR_get_error();
  if (code != 0)
  {
    ERR_error_string_n(code, aura_tls_error, sizeof(aura_tls_error));
    return;
  }
  snprintf(aura_tls_error, sizeof(aura_tls_error), "%s", message ? message : "tls error");
}

const char *aura_tls_last_error(void) { return aura_tls_error; }

static void aura_tls_init(void)
{
  if (!aura_tls_initialized)
  {
    OPENSSL_init_ssl(0, NULL);
    aura_tls_initialized = 1;
  }
}

static AuraTlsEntry *aura_tls_find(const char *endpoint)
{
  size_t i;
  for (i = 0; i < sizeof(aura_tls_entries) / sizeof(aura_tls_entries[0]); i++)
  {
    if (aura_tls_entries[i].active && strcmp(aura_tls_entries[i].endpoint, endpoint) == 0)
      return &aura_tls_entries[i];
  }
  return NULL;
}

static int aura_tls_wait(AuraTcpStream *stream, short events, int timeout_ms)
{
  AuraPlatformPollFd descriptor;
  int result;
  if (stream == NULL || stream->fd < 0) return 0;
  descriptor.fd = stream->fd;
  descriptor.events = events;
  descriptor.revents = 0;
  do { result = aura_platform_poll(&descriptor, 1, timeout_ms); } while (result < 0 && errno == EINTR);
  return result > 0 && (descriptor.revents & events) != 0;
}

static const char *aura_tls_endpoint_target(const char *endpoint)
{
  if (endpoint != NULL && strncmp(endpoint, "tls://", 6) == 0) return endpoint + 6;
  if (endpoint != NULL && strncmp(endpoint, "https://", 8) == 0) return endpoint + 8;
  return endpoint;
}

static int aura_tls_activate(AuraTlsEntry *slot, const char *endpoint,
                             AuraTcpStream *stream, AuraFfiOpaqueHandle *owner,
                             const char *server_name, int verify_peer)
{
  SSL_CTX *context = NULL;
  SSL *ssl = NULL;
  if (slot == NULL || endpoint == NULL || endpoint[0] == '\0' ||
      stream == NULL || stream->fd < 0) return 0;
  context = SSL_CTX_new(TLS_client_method());
  if (context == NULL) goto fail;
  if (verify_peer)
  {
    SSL_CTX_set_verify(context, SSL_VERIFY_PEER, NULL);
    if (SSL_CTX_set_default_verify_paths(context) != 1) goto fail;
  }
  else SSL_CTX_set_verify(context, SSL_VERIFY_NONE, NULL);
  ssl = SSL_new(context);
  if (ssl == NULL) goto fail;
  if (server_name != NULL && server_name[0] != '\0')
  {
    if (SSL_set_tlsext_host_name(ssl, server_name) != 1) goto fail;
    if (verify_peer && X509_VERIFY_PARAM_set1_host(SSL_get0_param(ssl), server_name, 0) != 1)
      goto fail;
  }
  if (SSL_set_fd(ssl, stream->fd) != 1) goto fail;
  for (;;)
  {
    int result = SSL_connect(ssl);
    if (result == 1) break;
    int error = SSL_get_error(ssl, result);
    if (error == SSL_ERROR_WANT_READ && aura_tls_wait(stream, POLLIN, 1000)) continue;
    if (error == SSL_ERROR_WANT_WRITE && aura_tls_wait(stream, POLLOUT, 1000)) continue;
    goto fail;
  }
  if (verify_peer && SSL_get_verify_result(ssl) != X509_V_OK) { aura_tls_set_error("certificate rejected"); goto fail; }
  slot->endpoint = strdup(endpoint);
  slot->stream = stream;
  slot->owner = owner;
  slot->context = context;
  slot->ssl = ssl;
  slot->pending_events = 0;
  slot->active = slot->endpoint != NULL;
  if (!slot->active) goto fail;
  return 1;
fail:
  aura_tls_set_error("TLS handshake failed");
  if (ssl != NULL) { SSL_shutdown(ssl); SSL_free(ssl); }
  if (context != NULL) SSL_CTX_free(context);
  if (owner != NULL) (void)aura_ffi_handle_drop(&owner);
  else if (stream != NULL) aura_tcp_stream_destroy(stream);
  return 0;
}

int aura_tls_connect(const char *endpoint, const char *server_name, int verify_peer)
{
  AuraTlsEntry *slot = NULL;
  AuraTcpStream *stream = NULL;
  const char *target = aura_tls_endpoint_target(endpoint);
  size_t i;
  if (endpoint == NULL || endpoint[0] == '\0' || aura_tls_find(endpoint) != NULL)
  {
    aura_tls_set_error("tls endpoint is invalid or already connected");
    return 0;
  }
  aura_tls_init();
  for (i = 0; i < sizeof(aura_tls_entries) / sizeof(aura_tls_entries[0]); i++)
    if (!aura_tls_entries[i].active) { slot = &aura_tls_entries[i]; break; }
  if (slot == NULL || target == NULL ||
      aura_tcp_stream_connect_endpoint(target, 1000, &stream) != AURA_TCP_OK || stream == NULL)
  {
    aura_tls_set_error("tcp connection failed");
    return 0;
  }
  return aura_tls_activate(slot, endpoint, stream, NULL, server_name, verify_peer);
}

int aura_tls_wrap_stream(const char *endpoint, AuraTcpStream *stream,
                         AuraFfiOpaqueHandle *owner, const char *server_name,
                         int verify_peer)
{
  AuraTlsEntry *slot = NULL;
  size_t i;
  if (endpoint == NULL || endpoint[0] == '\0' || stream == NULL ||
      aura_tls_find(endpoint) != NULL)
  {
    aura_tls_set_error("tls stream endpoint is invalid or already connected");
    if (owner != NULL) (void)aura_ffi_handle_drop(&owner);
    return 0;
  }
  aura_tls_init();
  for (i = 0; i < sizeof(aura_tls_entries) / sizeof(aura_tls_entries[0]); i++)
    if (!aura_tls_entries[i].active) { slot = &aura_tls_entries[i]; break; }
  if (slot == NULL || !aura_tls_activate(slot, endpoint, stream, owner,
                                           server_name, verify_peer))
  {
    if (slot == NULL && owner != NULL) (void)aura_ffi_handle_drop(&owner);
    return 0;
  }
  return 1;
}

AuraTcpStream *aura_tls_stream(const char *endpoint)
{
  AuraTlsEntry *entry = aura_tls_find(endpoint);
  return entry != NULL ? entry->stream : NULL;
}

short aura_tls_pending_events(const char *endpoint)
{
  AuraTlsEntry *entry = aura_tls_find(endpoint);
  return entry != NULL && entry->pending_events != 0 ? entry->pending_events : POLLIN;
}

const char *aura_tls_read(const char *endpoint, int64_t capacity)
{
  AuraTlsEntry *entry = aura_tls_find(endpoint);
  char *result;
  int count;
  if (entry == NULL || entry->ssl == NULL || capacity <= 0 || capacity > 65536) return NULL;
  result = (char *)malloc((size_t)capacity + 1u);
  if (result == NULL) return NULL;
  for (;;)
  {
    count = SSL_read(entry->ssl, result, (int)capacity);
    if (count > 0) { result[count] = '\0'; return result; }
    int error = SSL_get_error(entry->ssl, count);
    if (error == SSL_ERROR_WANT_READ && aura_tls_wait(entry->stream, POLLIN, 1000)) continue;
    if (error == SSL_ERROR_WANT_WRITE && aura_tls_wait(entry->stream, POLLOUT, 1000)) continue;
    free(result);
    if (error == SSL_ERROR_ZERO_RETURN) return strdup("");
    aura_tls_set_error("TLS read failed");
    return NULL;
  }
}

int64_t aura_tls_write(const char *endpoint, const char *content)
{
  AuraTlsEntry *entry = aura_tls_find(endpoint);
  size_t offset = 0;
  size_t length;
  if (entry == NULL || entry->ssl == NULL || content == NULL) return -1;
  length = strlen(content);
  if (length > 65536) return -1;
  while (offset < length)
  {
    int count = SSL_write(entry->ssl, content + offset, (int)(length - offset));
    if (count > 0) { offset += (size_t)count; continue; }
    int error = SSL_get_error(entry->ssl, count);
    if (error == SSL_ERROR_WANT_READ && aura_tls_wait(entry->stream, POLLIN, 1000)) continue;
    if (error == SSL_ERROR_WANT_WRITE && aura_tls_wait(entry->stream, POLLOUT, 1000)) continue;
    aura_tls_set_error("TLS write failed");
    return -1;
  }
  return (int64_t)length;
}

int aura_tls_read_bytes(const char *endpoint, void *output, size_t capacity,
                        size_t *out_bytes, int timeout_ms)
{
  AuraTlsEntry *entry = aura_tls_find(endpoint);
  int64_t deadline = 0;
  if (out_bytes == NULL || (output == NULL && capacity != 0) || timeout_ms < 0) return -1;
  *out_bytes = 0;
  if (entry == NULL || entry->ssl == NULL) return -2;
  if (capacity == 0) return 0;
  if (timeout_ms > 0) deadline = aura_time_monotonic_millis() + timeout_ms;
  for (;;)
  {
    int count = SSL_read(entry->ssl, output, (int)(capacity > INT_MAX ? INT_MAX : capacity));
    if (count > 0) { entry->pending_events = 0; *out_bytes = (size_t)count; return 0; }
    int error = SSL_get_error(entry->ssl, count);
    if (error == SSL_ERROR_ZERO_RETURN) return 1;
    if (error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE)
    {
      entry->pending_events = error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT;
      int wait_ms = timeout_ms;
      if (deadline > 0)
      {
        int64_t remaining = deadline - aura_time_monotonic_millis();
        if (remaining <= 0) { aura_tls_set_error("TLS binary read timeout"); return 3; }
        wait_ms = remaining > INT_MAX ? INT_MAX : (int)remaining;
      }
      if (aura_tls_wait(entry->stream,
                        error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT,
                        wait_ms)) continue;
      aura_tls_set_error("TLS binary read timeout");
      return 3;
    }
    entry->pending_events = 0;
    aura_tls_set_error("TLS binary read failed");
    return error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE ? 3 : -1;
  }
}

int aura_tls_write_bytes(const char *endpoint, const void *input, size_t length,
                         size_t *out_bytes, int timeout_ms)
{
  AuraTlsEntry *entry = aura_tls_find(endpoint);
  int64_t deadline = 0;
  if (out_bytes == NULL || (input == NULL && length != 0) || timeout_ms < 0) return -1;
  *out_bytes = 0;
  if (entry == NULL || entry->ssl == NULL) return -2;
  if (timeout_ms > 0) deadline = aura_time_monotonic_millis() + timeout_ms;
  while (*out_bytes < length)
  {
    size_t remaining = length - *out_bytes;
    int count = SSL_write(entry->ssl, (const unsigned char *)input + *out_bytes,
                          (int)(remaining > INT_MAX ? INT_MAX : remaining));
    if (count > 0) { entry->pending_events = 0; *out_bytes += (size_t)count; continue; }
    int error = SSL_get_error(entry->ssl, count);
    if (error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE)
    {
      entry->pending_events = error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT;
      int wait_ms = timeout_ms;
      if (deadline > 0)
      {
        int64_t remaining = deadline - aura_time_monotonic_millis();
        if (remaining <= 0) { aura_tls_set_error("TLS binary write timeout"); return 3; }
        wait_ms = remaining > INT_MAX ? INT_MAX : (int)remaining;
      }
      if (aura_tls_wait(entry->stream,
                        error == SSL_ERROR_WANT_READ ? POLLIN : POLLOUT,
                        wait_ms)) continue;
      aura_tls_set_error("TLS binary write timeout");
      return 3;
    }
    entry->pending_events = 0;
    aura_tls_set_error("TLS binary write failed");
    return error == SSL_ERROR_WANT_READ || error == SSL_ERROR_WANT_WRITE ? 3 : -1;
  }
  return 0;
}

int aura_tls_close(const char *endpoint)
{
  AuraTlsEntry *entry = aura_tls_find(endpoint);
  if (entry == NULL) return 1;
  SSL_shutdown(entry->ssl);
  SSL_free(entry->ssl);
  SSL_CTX_free(entry->context);
  aura_tcp_stream_destroy(entry->stream);
  free(entry->endpoint);
  if (entry->owner != NULL) (void)aura_ffi_handle_drop(&entry->owner);
  memset(entry, 0, sizeof(*entry));
  return 1;
}

static const char *aura_tls_certificate_value(const char *path, int issuer)
{
  FILE *file;
  X509 *certificate;
  X509_NAME *name;
  char buffer[1024];
  char **target = issuer ? &aura_tls_certificate_issuer_value : &aura_tls_certificate_subject_value;
  free(*target); *target = NULL;
  if (path == NULL || (file = fopen(path, "rb")) == NULL) return NULL;
  certificate = PEM_read_X509(file, NULL, NULL, NULL);
  fclose(file);
  if (certificate == NULL) { aura_tls_set_error("certificate load failed"); return NULL; }
  name = issuer ? X509_get_issuer_name(certificate) : X509_get_subject_name(certificate);
  if (name == NULL || X509_NAME_oneline(name, buffer, sizeof(buffer)) == NULL)
  { X509_free(certificate); return NULL; }
  *target = strdup(buffer);
  X509_free(certificate);
  return *target;
}

const char *aura_tls_certificate_subject(const char *path) { return aura_tls_certificate_value(path, 0); }
const char *aura_tls_certificate_issuer(const char *path) { return aura_tls_certificate_value(path, 1); }

static void aura_tls_http_error(char **out_error, const char *message)
{
  if (out_error == NULL) return;
  *out_error = NULL;
  if (message == NULL) return;
  size_t length = strlen(message);
  *out_error = (char *)malloc(length + 1);
  if (*out_error != NULL) memcpy(*out_error, message, length + 1);
}

static int aura_tls_http_authority(const char *endpoint, char **out_key,
                                   char **out_connect, char **out_host)
{
  const char *authority = endpoint + (strncmp(endpoint, "https://", 8) == 0 ? 8 : 6);
  const char *end = strpbrk(authority, "/?#");
  if (end != NULL) return 0;
  size_t length = end == NULL ? strlen(authority) : (size_t)(end - authority);
  if (length == 0 || length > 240 || memchr(authority, '@', length) != NULL ||
      memchr(authority, '\r', length) != NULL || memchr(authority, '\n', length) != NULL)
    return 0;
  char *raw = (char *)malloc(length + 1);
  if (raw == NULL) return 0;
  memcpy(raw, authority, length);
  raw[length] = '\0';
  const char *host_start = raw;
  const char *host_end = raw + length;
  const char *port = NULL;
  if (raw[0] == '[')
  {
    char *closing = strchr(raw, ']');
    if (closing == NULL) { free(raw); return 0; }
    host_start = raw + 1;
    host_end = closing;
    if (closing[1] == ':') port = closing + 2;
    else if (closing[1] != '\0') { free(raw); return 0; }
  }
  else
  {
    const char *colon = strrchr(raw, ':');
    if (colon != NULL && strchr(raw, ':') == colon)
    {
      host_end = colon;
      port = colon + 1;
    }
  }
  if (host_end <= host_start || (port != NULL && port[0] == '\0')) { free(raw); return 0; }
  size_t host_length = (size_t)(host_end - host_start);
  char *host = (char *)malloc(host_length + 1);
  if (host == NULL) { free(raw); return 0; }
  memcpy(host, host_start, host_length);
  host[host_length] = '\0';
  char *connect = NULL;
  if (port != NULL)
  {
    size_t connect_length = strlen(raw);
    connect = (char *)malloc(connect_length + 1);
    if (connect != NULL) memcpy(connect, raw, connect_length + 1);
  }
  else
  {
    size_t connect_length = strlen(raw) + 5;
    connect = (char *)malloc(connect_length + 1);
    if (connect != NULL) snprintf(connect, connect_length + 1, "%s:443", raw);
  }
  free(raw);
  if (connect == NULL) { free(host); return 0; }
  static _Atomic unsigned long long sequence;
  char key[96];
  unsigned long long sequence_id = atomic_fetch_add(&sequence, 1) + 1;
  snprintf(key, sizeof(key), "aura-http-tls-%llu", sequence_id);
  char *key_copy = strdup(key);
  if (key_copy == NULL) { free(host); free(connect); return 0; }
  *out_key = key_copy;
  *out_connect = connect;
  *out_host = host;
  return 1;
}

static int aura_tls_http_content_length(const char *headers, size_t length,
                                        size_t *out_length)
{
  const char *cursor = headers;
  const char *end = headers + length;
  while (cursor < end)
  {
    const char *line_end = strstr(cursor, "\r\n");
    if (line_end == NULL || line_end > end) line_end = end;
    const char *colon = memchr(cursor, ':', (size_t)(line_end - cursor));
    if (colon != NULL && (size_t)(colon - cursor) == 14 &&
        strncasecmp(cursor, "Content-Length", 14) == 0)
    {
      const char *value = colon + 1;
      while (value < line_end && (*value == ' ' || *value == '\t')) value++;
      errno = 0;
      char *parsed_end = NULL;
      unsigned long long parsed = strtoull(value, &parsed_end, 10);
      while (parsed_end < line_end && (*parsed_end == ' ' || *parsed_end == '\t')) parsed_end++;
      if (value == line_end || errno == ERANGE || parsed_end != line_end || parsed > SIZE_MAX) return 0;
      *out_length = (size_t)parsed;
      return 1;
    }
    if (line_end == end) break;
    cursor = line_end + 2;
  }
  return 0;
}

static int aura_tls_http_is_chunked(const char *headers, size_t length)
{
  const char *cursor = headers;
  const char *end = headers + length;
  while (cursor < end)
  {
    const char *line_end = strstr(cursor, "\r\n");
    if (line_end == NULL || line_end > end) line_end = end;
    const char *colon = memchr(cursor, ':', (size_t)(line_end - cursor));
    if (colon != NULL && (size_t)(colon - cursor) == 17 &&
        strncasecmp(cursor, "Transfer-Encoding", 17) == 0)
    {
      const char *value = colon + 1;
      while (value < line_end && (*value == ' ' || *value == '\t')) value++;
      return (size_t)(line_end - value) == 7 && strncasecmp(value, "chunked", 7) == 0;
    }
    if (line_end == end) break;
    cursor = line_end + 2;
  }
  return 0;
}

static int aura_tls_http_read_line(const char *key, char *line, size_t capacity)
{
  size_t used = 0;
  while (used + 1 < capacity)
  {
    unsigned char byte = 0;
    size_t count = 0;
    if (aura_tls_read_bytes(key, &byte, 1, &count, 1000) != 0 || count != 1) return 0;
    line[used++] = (char)byte;
    if (used >= 2 && line[used - 2] == '\r' && line[used - 1] == '\n')
    { line[used] = '\0'; return 1; }
  }
  return 0;
}

static int aura_tls_http_read_chunked(const char *key, size_t max_bytes,
                                      unsigned char **out_body, size_t *out_length)
{
  unsigned char *body = NULL;
  size_t length = 0;
  char line[128];
  for (;;)
  {
    char *end = NULL;
    unsigned long long chunk = 0;
    if (!aura_tls_http_read_line(key, line, sizeof(line))) goto fail;
    errno = 0;
    chunk = strtoull(line, &end, 16);
    while (end != NULL && *end != '\0' && *end != ';' && *end != '\r' && *end != '\n') end++;
    if (errno == ERANGE || end == line || chunk > SIZE_MAX || chunk > max_bytes - length) goto fail;
    if (chunk == 0)
    {
      do { if (!aura_tls_http_read_line(key, line, sizeof(line))) goto fail; }
      while (strcmp(line, "\r\n") != 0);
      if (body == NULL) body = (unsigned char *)malloc(1);
      if (body == NULL) goto fail;
      *out_body = body; *out_length = length; return 1;
    }
    unsigned char *next = (unsigned char *)realloc(body, length + (size_t)chunk);
    if (next == NULL) goto fail;
    body = next;
    size_t received = 0;
    while (received < (size_t)chunk)
    {
      size_t count = 0;
      if (aura_tls_read_bytes(key, body + length + received, (size_t)chunk - received, &count, 1000) != 0 || count == 0) goto fail;
      received += count;
    }
    length += (size_t)chunk;
    unsigned char crlf[2];
    size_t count = 0;
    if (aura_tls_read_bytes(key, crlf, 2, &count, 1000) != 0 || count != 2 || crlf[0] != '\r' || crlf[1] != '\n') goto fail;
  }
fail:
  free(body);
  return 0;
}

int aura_tls_http_client_get_bytes(const char *endpoint, const char *target,
                                   size_t max_bytes, unsigned char **out_bytes,
                                   size_t *out_length, char **out_error)
{
  if (out_bytes == NULL || out_length == NULL || endpoint == NULL || target == NULL ||
      (strncmp(endpoint, "https://", 8) != 0 && strncmp(endpoint, "tls://", 6) != 0) ||
      target[0] != '/' || max_bytes == 0)
  {
    aura_tls_http_error(out_error, "invalid HTTPS binary fetch arguments");
    return 0;
  }
  *out_bytes = NULL;
  *out_length = 0;
  if (out_error != NULL) *out_error = NULL;
  char *key = NULL;
  char *connect = NULL;
  char *host = NULL;
  if (!aura_tls_http_authority(endpoint, &key, &connect, &host))
  {
    aura_tls_http_error(out_error, "invalid HTTPS endpoint authority");
    return 0;
  }
  AuraTlsEntry *slot = NULL;
  AuraTcpStream *stream = NULL;
  for (size_t i = 0; i < sizeof(aura_tls_entries) / sizeof(aura_tls_entries[0]); i++)
    if (!aura_tls_entries[i].active) { slot = &aura_tls_entries[i]; break; }
  aura_tls_init();
  if (slot == NULL || aura_tcp_stream_connect_endpoint(connect, 1000, &stream) != AURA_TCP_OK ||
      stream == NULL || !aura_tls_activate(slot, key, stream, NULL, host, 1))
  {
    free(key); free(connect); free(host);
    aura_tls_http_error(out_error, aura_tls_last_error());
    return 0;
  }
  size_t request_length = strlen(target) + strlen(connect) + 64;
  char *request = (char *)malloc(request_length);
  if (request == NULL)
  {
    aura_tls_close(key); free(key); free(connect); free(host);
    aura_tls_http_error(out_error, "HTTPS request allocation failed");
    return 0;
  }
  int written = snprintf(request, request_length,
                         "GET %s HTTP/1.1\r\nHost: %s\r\nConnection: close\r\n\r\n",
                         target, host);
  size_t sent = 0;
  int status = written > 0 ? aura_tls_write_bytes(key, request, (size_t)written, &sent, 1000) : -1;
  free(request);
  if (status != 0 || sent != (size_t)(written > 0 ? written : 0))
  {
    aura_tls_close(key); free(key); free(connect); free(host);
    aura_tls_http_error(out_error, "HTTPS request failed");
    return 0;
  }
  const size_t header_limit = 65536;
  unsigned char *headers = (unsigned char *)malloc(header_limit + 1);
  size_t header_length = 0;
  int complete = 0;
  while (headers != NULL && header_length < header_limit)
  {
    size_t count = 0;
    status = aura_tls_read_bytes(key, headers + header_length, 1, &count, 1000);
    if (status != 0 || count != 1) break;
    header_length++;
    headers[header_length] = '\0';
    if (header_length >= 4 && memcmp(headers + header_length - 4, "\r\n\r\n", 4) == 0)
    { complete = 1; break; }
  }
  if (!complete || header_length < 12 || memcmp(headers, "HTTP/1.", 7) != 0 ||
      headers[9] != '2' || headers[10] != '0' || headers[11] != '0')
  {
    free(headers); aura_tls_close(key); free(key); free(connect); free(host);
    aura_tls_http_error(out_error, "HTTPS upstream returned an invalid response");
    return 0;
  }
  size_t body_length = 0;
  int chunked = aura_tls_http_is_chunked((const char *)headers, header_length);
  if (!chunked && (!aura_tls_http_content_length((const char *)headers, header_length, &body_length) ||
                   body_length > max_bytes))
  {
    free(headers); aura_tls_close(key); free(key); free(connect); free(host);
    aura_tls_http_error(out_error, "HTTPS response has no bounded Content-Length");
    return 0;
  }
  free(headers);
  if (chunked)
  {
    int ok = aura_tls_http_read_chunked(key, max_bytes, out_bytes, out_length);
    aura_tls_close(key); free(key); free(connect); free(host);
    if (!ok) aura_tls_http_error(out_error, "HTTPS chunked body failed");
    return ok;
  }
  unsigned char *body = (unsigned char *)malloc(body_length == 0 ? 1 : body_length);
  if (body == NULL)
  {
    aura_tls_close(key); free(key); free(connect); free(host);
    aura_tls_http_error(out_error, "HTTPS body allocation failed");
    return 0;
  }
  size_t received = 0;
  while (received < body_length)
  {
    size_t count = 0;
    status = aura_tls_read_bytes(key, body + received, body_length - received, &count, 1000);
    if (status != 0 || count == 0) break;
    received += count;
  }
  aura_tls_close(key); free(key); free(connect); free(host);
  if (received != body_length)
  {
    free(body);
    aura_tls_http_error(out_error, "HTTPS response body ended early");
    return 0;
  }
  *out_bytes = body;
  *out_length = body_length;
  return 1;
}

#else
const char *aura_tls_last_error(void) { return "TLS provider unavailable"; }
int aura_tls_connect(const char *endpoint, const char *server_name, int verify_peer) { (void)endpoint; (void)server_name; (void)verify_peer; return 0; }
int aura_tls_wrap_stream(const char *endpoint, AuraTcpStream *stream, AuraFfiOpaqueHandle *owner, const char *server_name, int verify_peer) { (void)endpoint; (void)stream; (void)owner; (void)server_name; (void)verify_peer; return 0; }
AuraTcpStream *aura_tls_stream(const char *endpoint) { (void)endpoint; return NULL; }
short aura_tls_pending_events(const char *endpoint) { (void)endpoint; return 0; }
const char *aura_tls_read(const char *endpoint, int64_t capacity) { (void)endpoint; (void)capacity; return NULL; }
int64_t aura_tls_write(const char *endpoint, const char *content) { (void)endpoint; (void)content; return -1; }
int aura_tls_read_bytes(const char *endpoint, void *output, size_t capacity, size_t *out_bytes, int timeout_ms) { (void)endpoint; (void)output; (void)capacity; (void)timeout_ms; if (out_bytes) *out_bytes = 0; return -1; }
int aura_tls_write_bytes(const char *endpoint, const void *input, size_t length, size_t *out_bytes, int timeout_ms) { (void)endpoint; (void)input; (void)length; (void)timeout_ms; if (out_bytes) *out_bytes = 0; return -1; }
int aura_tls_close(const char *endpoint) { (void)endpoint; return 0; }
const char *aura_tls_certificate_subject(const char *path) { (void)path; return NULL; }
const char *aura_tls_certificate_issuer(const char *path) { (void)path; return NULL; }
int aura_tls_http_client_get_bytes(const char *endpoint, const char *target, size_t max_bytes, unsigned char **out_bytes, size_t *out_length, char **out_error) { (void)endpoint; (void)target; (void)max_bytes; if (out_bytes) *out_bytes = NULL; if (out_length) *out_length = 0; if (out_error) *out_error = NULL; return 0; }
#endif
