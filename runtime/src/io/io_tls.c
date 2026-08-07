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
  struct pollfd descriptor;
  int result;
  if (stream == NULL || stream->fd < 0) return 0;
  descriptor.fd = stream->fd;
  descriptor.events = events;
  descriptor.revents = 0;
  do { result = poll(&descriptor, 1, timeout_ms); } while (result < 0 && errno == EINTR);
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
#endif
