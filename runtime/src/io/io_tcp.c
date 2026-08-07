/* ---- Bounded TCP I/O (std.net, POSIX alpha slice) ----
 *
 * The handles below are opaque at the API boundary.  A handle owns its file
 * descriptor until close/destroy; close transitions it to a permanently
 * closed state, so repeated close calls are harmless.  Sockets are
 * nonblocking.  Every operation that may wait accepts a timeout in
 * milliseconds (zero means do not wait, and a positive value is the maximum
 * poll interval for that operation).  Port-only wrappers remain localhost
 * compatible; endpoint-aware entry points accept host:port strings.
 */

typedef struct AuraTcpListener AuraTcpListener;
typedef struct AuraTcpStream AuraTcpStream;

typedef enum
{
  AURA_TCP_OK = 0,
  AURA_TCP_PENDING = 1,
  AURA_TCP_EOF = 2,
  AURA_TCP_TIMEOUT = 3,
  AURA_TCP_PARTIAL_EOF = 4,
  AURA_TCP_ERROR = -1,
  AURA_TCP_CLOSED = -2,
  AURA_TCP_UNSUPPORTED = -3
} AuraTcpStatus;

struct AuraTcpListener
{
  int fd;
};

struct AuraTcpStream
{
  int fd;
};

static char aura_tcp_errbuf[256] = "no error";

const char *aura_tcp_last_error(void)
{
  return aura_tcp_errbuf;
}

#if AURA_TCP_POSIX

static void aura_tcp_clear_error(void)
{
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "no error");
}

static void aura_tcp_error_errno(const char *op)
{
  int saved = errno;
  const char *detail = strerror(saved);
  if (detail == NULL)
  {
    detail = "unknown error";
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp %s failed: %s", op, detail);
}

static void aura_tcp_error_text(const char *text)
{
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp %s", text ? text : "error");
}

static int aura_tcp_set_nonblocking(int fd)
{
  int flags = fcntl(fd, F_GETFL, 0);
  if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0)
  {
    return -1;
  }
  return 0;
}

static void aura_tcp_disable_sigpipe(int fd)
{
#if defined(SO_NOSIGPIPE)
  int enabled = 1;
  (void)setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &enabled, sizeof(enabled));
#else
  (void)fd;
#endif
}

static AuraTcpStatus aura_tcp_wait(int fd, short events, int timeout_ms)
{
  if (timeout_ms < 0)
  {
    errno = EINVAL;
    aura_tcp_error_errno("timeout");
    return AURA_TCP_ERROR;
  }
  struct pollfd descriptor = {fd, events, 0};
  int result = poll(&descriptor, 1, timeout_ms);
  if (result < 0)
  {
    aura_tcp_error_errno("poll");
    return AURA_TCP_ERROR;
  }
  if (result == 0)
  {
    return AURA_TCP_TIMEOUT;
  }
  if ((descriptor.revents & (POLLERR | POLLNVAL)) != 0)
  {
    errno = descriptor.revents & POLLNVAL ? EBADF : EIO;
    aura_tcp_error_errno("poll");
    return AURA_TCP_ERROR;
  }
  if ((descriptor.revents & events) == 0)
  {
    errno = ECONNRESET;
    aura_tcp_error_errno("poll");
    return AURA_TCP_ERROR;
  }
  return AURA_TCP_OK;
}

static AuraTcpStatus aura_tcp_wait_or_pending(int fd, short events, int timeout_ms)
{
  AuraTcpStatus status = aura_tcp_wait(fd, events, timeout_ms);
  return status == AURA_TCP_TIMEOUT && timeout_ms == 0 ? AURA_TCP_PENDING : status;
}

static AuraTcpStream *aura_tcp_stream_from_fd(int fd)
{
  AuraTcpStream *stream = (AuraTcpStream *)malloc(sizeof(*stream));
  if (stream == NULL)
  {
    errno = ENOMEM;
    aura_tcp_error_errno("allocate stream");
    close(fd);
    return NULL;
  }
  stream->fd = fd;
  return stream;
}

static void aura_tcp_endpoint_error(const char *text)
{
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp endpoint: %s", text);
}

static int aura_tcp_endpoint_parts(const char *endpoint, char *host,
                                   size_t host_capacity, char *service,
                                   size_t service_capacity)
{
  if (endpoint == NULL || endpoint[0] == '\0')
  {
    return 0;
  }
  size_t length = strlen(endpoint);
  if (endpoint[0] == '[')
  {
    const char *closing = strchr(endpoint, ']');
    if (closing == NULL || closing[1] != ':')
    {
      return 0;
    }
    size_t host_length = (size_t)(closing - endpoint - 1);
    size_t service_length = strlen(closing + 2);
    if (host_length == 0 || host_length >= host_capacity ||
        service_length == 0 || service_length >= service_capacity)
    {
      return 0;
    }
    memcpy(host, endpoint + 1, host_length);
    host[host_length] = '\0';
    memcpy(service, closing + 2, service_length + 1);
    return 1;
  }
  size_t digits = 0;
  while (digits < length && endpoint[digits] >= '0' && endpoint[digits] <= '9')
  {
    digits++;
  }
  if (digits == length)
  {
    if (strlen("127.0.0.1") >= host_capacity || length >= service_capacity)
    {
      return 0;
    }
    strcpy(host, "127.0.0.1");
    strcpy(service, endpoint);
    return 1;
  }
  const char *separator = strrchr(endpoint, ':');
  if (separator == NULL || separator == endpoint || separator[1] == '\0' ||
      strchr(endpoint, ':') != separator)
  {
    return 0;
  }
  size_t host_length = (size_t)(separator - endpoint);
  size_t service_length = strlen(separator + 1);
  if (host_length >= host_capacity || service_length >= service_capacity)
  {
    return 0;
  }
  memcpy(host, endpoint, host_length);
  host[host_length] = '\0';
  memcpy(service, separator + 1, service_length + 1);
  return 1;
}

static int aura_tcp_endpoint_valid_service(const char *service, int allow_zero)
{
  if (service == NULL || service[0] == '\0')
  {
    return 0;
  }
  char *end = NULL;
  unsigned long value = strtoul(service, &end, 10);
  return end != service && *end == '\0' && value <= UINT16_MAX &&
         (allow_zero || value != 0);
}

AuraTcpStatus aura_tcp_listener_bind_endpoint(const char *endpoint,
                                              uint16_t *out_port,
                                              AuraTcpListener **out_listener)
{
  aura_tcp_clear_error();
  if (out_port == NULL || out_listener == NULL)
  {
    errno = EINVAL;
    aura_tcp_error_errno("bind");
    return AURA_TCP_ERROR;
  }
  *out_port = 0;
  *out_listener = NULL;
  char host[256];
  char service[32];
  if (!aura_tcp_endpoint_parts(endpoint, host, sizeof(host), service,
                               sizeof(service)) ||
      !aura_tcp_endpoint_valid_service(service, 1))
  {
    aura_tcp_endpoint_error("expected PORT, HOST:PORT, or [IPv6]:PORT");
    return AURA_TCP_ERROR;
  }
  struct addrinfo hints;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_UNSPEC;
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_PASSIVE;
  struct addrinfo *addresses = NULL;
  int resolved = getaddrinfo(host, service, &hints, &addresses);
  if (resolved != 0)
  {
    aura_tcp_endpoint_error(gai_strerror(resolved));
    return AURA_TCP_ERROR;
  }

  int fd = -1;
  for (struct addrinfo *candidate = addresses; candidate != NULL;
       candidate = candidate->ai_next)
  {
    fd = socket(candidate->ai_family, candidate->ai_socktype,
                candidate->ai_protocol);
    if (fd < 0)
    {
      continue;
    }
    int reuse = 1;
    if (setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, sizeof(reuse)) != 0 ||
        bind(fd, candidate->ai_addr, candidate->ai_addrlen) != 0 ||
        listen(fd, 16) != 0 || aura_tcp_set_nonblocking(fd) != 0)
    {
      close(fd);
      fd = -1;
      continue;
    }
    break;
  }
  freeaddrinfo(addresses);
  if (fd < 0)
  {
    aura_tcp_error_errno("listen");
    return AURA_TCP_ERROR;
  }
  struct sockaddr_storage bound;
  socklen_t bound_size = (socklen_t)sizeof(bound);
  if (getsockname(fd, (struct sockaddr *)&bound, &bound_size) != 0)
  {
    aura_tcp_error_errno("read bound port");
    close(fd);
    return AURA_TCP_ERROR;
  }
  AuraTcpListener *listener = (AuraTcpListener *)malloc(sizeof(*listener));
  if (listener == NULL)
  {
    errno = ENOMEM;
    aura_tcp_error_errno("allocate listener");
    close(fd);
    return AURA_TCP_ERROR;
  }
  listener->fd = fd;
  if (bound.ss_family == AF_INET)
  {
    *out_port = ntohs(((struct sockaddr_in *)&bound)->sin_port);
  }
  else if (bound.ss_family == AF_INET6)
  {
    *out_port = ntohs(((struct sockaddr_in6 *)&bound)->sin6_port);
  }
  else
  {
    aura_tcp_error_text("unsupported bound address");
    close(listener->fd);
    free(listener);
    return AURA_TCP_ERROR;
  }
  *out_listener = listener;
  return AURA_TCP_OK;
}

AuraTcpStatus aura_tcp_listener_bind(uint16_t port, uint16_t *out_port,
                                     AuraTcpListener **out_listener)
{
  char endpoint[32];
  snprintf(endpoint, sizeof(endpoint), "127.0.0.1:%u", port);
  return aura_tcp_listener_bind_endpoint(endpoint, out_port, out_listener);
}

AuraTcpStatus aura_tcp_listener_accept(AuraTcpListener *listener, int timeout_ms,
                                       AuraTcpStream **out_stream)
{
  aura_tcp_clear_error();
  if (out_stream == NULL)
  {
    errno = EINVAL;
    aura_tcp_error_errno("accept");
    return AURA_TCP_ERROR;
  }
  *out_stream = NULL;
  if (listener == NULL || listener->fd < 0)
  {
    aura_tcp_error_text("accept on closed listener");
    return AURA_TCP_CLOSED;
  }
  AuraTcpStatus waited = aura_tcp_wait_or_pending(listener->fd, POLLIN, timeout_ms);
  if (waited != AURA_TCP_OK)
  {
    return waited;
  }
  int fd = accept(listener->fd, NULL, NULL);
  if (fd < 0)
  {
    if (errno == EAGAIN || errno == EWOULDBLOCK)
    {
      return AURA_TCP_PENDING;
    }
    aura_tcp_error_errno("accept");
    return AURA_TCP_ERROR;
  }
  if (aura_tcp_set_nonblocking(fd) != 0)
  {
    aura_tcp_error_errno("nonblocking stream");
    close(fd);
    return AURA_TCP_ERROR;
  }
  aura_tcp_disable_sigpipe(fd);
  *out_stream = aura_tcp_stream_from_fd(fd);
  return *out_stream == NULL ? AURA_TCP_ERROR : AURA_TCP_OK;
}

AuraTcpStatus aura_tcp_stream_connect_endpoint(const char *endpoint,
                                               int timeout_ms,
                                               AuraTcpStream **out_stream)
{
  aura_tcp_clear_error();
  if (out_stream == NULL)
  {
    errno = EINVAL;
    aura_tcp_error_errno("connect");
    return AURA_TCP_ERROR;
  }
  *out_stream = NULL;
  char host[256];
  char service[32];
  if (timeout_ms < 0 ||
      !aura_tcp_endpoint_parts(endpoint, host, sizeof(host), service,
                               sizeof(service)) ||
      !aura_tcp_endpoint_valid_service(service, 0))
  {
    aura_tcp_endpoint_error("expected PORT, HOST:PORT, or [IPv6]:PORT");
    return AURA_TCP_ERROR;
  }
  struct addrinfo hints;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_UNSPEC;
  hints.ai_socktype = SOCK_STREAM;
  struct addrinfo *addresses = NULL;
  int resolved = getaddrinfo(host, service, &hints, &addresses);
  if (resolved != 0)
  {
    aura_tcp_endpoint_error(gai_strerror(resolved));
    return AURA_TCP_ERROR;
  }

  AuraTcpStatus result = AURA_TCP_ERROR;
  for (struct addrinfo *candidate = addresses; candidate != NULL;
       candidate = candidate->ai_next)
  {
    int fd = socket(candidate->ai_family, candidate->ai_socktype,
                    candidate->ai_protocol);
    if (fd < 0 || aura_tcp_set_nonblocking(fd) != 0)
    {
      if (fd >= 0)
      {
        close(fd);
      }
      continue;
    }
    aura_tcp_disable_sigpipe(fd);
    if (connect(fd, candidate->ai_addr, candidate->ai_addrlen) != 0)
    {
      if (errno != EINPROGRESS && errno != EALREADY)
      {
        close(fd);
        continue;
      }
      AuraTcpStatus waited = aura_tcp_wait(fd, POLLOUT, timeout_ms);
      if (waited != AURA_TCP_OK)
      {
        result = waited;
        close(fd);
        continue;
      }
      int connect_error = 0;
      socklen_t error_size = (socklen_t)sizeof(connect_error);
      if (getsockopt(fd, SOL_SOCKET, SO_ERROR, &connect_error, &error_size) !=
              0 ||
          connect_error != 0)
      {
        if (connect_error != 0)
        {
          errno = connect_error;
        }
        close(fd);
        continue;
      }
    }
    *out_stream = aura_tcp_stream_from_fd(fd);
    result = *out_stream == NULL ? AURA_TCP_ERROR : AURA_TCP_OK;
    break;
  }
  freeaddrinfo(addresses);
  if (result == AURA_TCP_TIMEOUT)
  {
    aura_tcp_error_text("connect timed out");
  }
  return result;
}

AuraTcpStatus aura_tcp_stream_connect(uint16_t port, int timeout_ms,
                                      AuraTcpStream **out_stream)
{
  char endpoint[32];
  snprintf(endpoint, sizeof(endpoint), "127.0.0.1:%u", port);
  return aura_tcp_stream_connect_endpoint(endpoint, timeout_ms, out_stream);
}

AuraTcpStatus aura_tcp_stream_read(AuraTcpStream *stream, void *buffer, size_t capacity,
                                   size_t *out_bytes, int timeout_ms)
{
  aura_tcp_clear_error();
  if (out_bytes == NULL || (buffer == NULL && capacity != 0) || timeout_ms < 0)
  {
    errno = EINVAL;
    aura_tcp_error_errno("read");
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  if (stream == NULL || stream->fd < 0)
  {
    aura_tcp_error_text("read on closed stream");
    return AURA_TCP_CLOSED;
  }
  if (capacity == 0)
  {
    return AURA_TCP_OK;
  }
  AuraTcpStatus waited = aura_tcp_wait_or_pending(stream->fd, POLLIN, timeout_ms);
  if (waited != AURA_TCP_OK)
  {
    return waited;
  }
  ssize_t count = recv(stream->fd, buffer, capacity, 0);
  if (count > 0)
  {
    *out_bytes = (size_t)count;
    return AURA_TCP_OK;
  }
  if (count == 0)
  {
    return AURA_TCP_EOF;
  }
  if (errno == EAGAIN || errno == EWOULDBLOCK)
  {
    return AURA_TCP_PENDING;
  }
  aura_tcp_error_errno("read");
  return AURA_TCP_ERROR;
}

AuraTcpStatus aura_tcp_stream_write(AuraTcpStream *stream, const void *buffer, size_t capacity,
                                    size_t *out_bytes, int timeout_ms)
{
  aura_tcp_clear_error();
  if (out_bytes == NULL || (buffer == NULL && capacity != 0) || timeout_ms < 0)
  {
    errno = EINVAL;
    aura_tcp_error_errno("write");
    return AURA_TCP_ERROR;
  }
  *out_bytes = 0;
  if (stream == NULL || stream->fd < 0)
  {
    aura_tcp_error_text("write on closed stream");
    return AURA_TCP_CLOSED;
  }
  if (capacity == 0)
  {
    return AURA_TCP_OK;
  }
  AuraTcpStatus waited = aura_tcp_wait_or_pending(stream->fd, POLLOUT, timeout_ms);
  if (waited != AURA_TCP_OK)
  {
    return waited;
  }
  int flags = 0;
#if defined(MSG_NOSIGNAL)
  flags |= MSG_NOSIGNAL;
#endif
  ssize_t count = send(stream->fd, buffer, capacity, flags);
  if (count >= 0)
  {
    *out_bytes = (size_t)count;
    return AURA_TCP_OK;
  }
  if (errno == EAGAIN || errno == EWOULDBLOCK)
  {
    return AURA_TCP_PENDING;
  }
  aura_tcp_error_errno("write");
  return AURA_TCP_ERROR;
}

AuraTcpStatus aura_tcp_stream_read_exactly(AuraTcpStream *stream, void *buffer,
                                           size_t length, size_t *out_bytes,
                                           int timeout_ms)
{
  if (out_bytes == NULL || (buffer == NULL && length != 0) || timeout_ms < 0)
    return AURA_TCP_ERROR;
  *out_bytes = 0;
  int64_t deadline = timeout_ms == 0 ? 0 : aura_time_monotonic_millis() + timeout_ms;
  while (*out_bytes < length)
  {
    int remaining = timeout_ms;
    if (deadline != 0)
    {
      int64_t left = deadline - aura_time_monotonic_millis();
      if (left <= 0) return AURA_TCP_TIMEOUT;
      remaining = left > INT_MAX ? INT_MAX : (int)left;
    }
    size_t count = 0;
    AuraTcpStatus status = aura_tcp_stream_read(stream,
        (unsigned char *)buffer + *out_bytes, length - *out_bytes, &count, remaining);
    *out_bytes += count;
    if (status == AURA_TCP_OK) continue;
    if (status == AURA_TCP_EOF)
      return *out_bytes == 0 ? AURA_TCP_EOF : AURA_TCP_PARTIAL_EOF;
    return status;
  }
  return AURA_TCP_OK;
}

AuraTcpStatus aura_tcp_stream_write_all(AuraTcpStream *stream, const void *buffer,
                                        size_t length, size_t *out_bytes,
                                        int timeout_ms)
{
  if (out_bytes == NULL || (buffer == NULL && length != 0) || timeout_ms < 0)
    return AURA_TCP_ERROR;
  *out_bytes = 0;
  int64_t deadline = timeout_ms == 0 ? 0 : aura_time_monotonic_millis() + timeout_ms;
  while (*out_bytes < length)
  {
    int remaining = timeout_ms;
    if (deadline != 0)
    {
      int64_t left = deadline - aura_time_monotonic_millis();
      if (left <= 0) return AURA_TCP_TIMEOUT;
      remaining = left > INT_MAX ? INT_MAX : (int)left;
    }
    size_t count = 0;
    AuraTcpStatus status = aura_tcp_stream_write(stream,
        (const unsigned char *)buffer + *out_bytes, length - *out_bytes, &count, remaining);
    *out_bytes += count;
    if (status == AURA_TCP_OK) continue;
    if (status == AURA_TCP_CLOSED) return AURA_TCP_CLOSED;
    return status;
  }
  return AURA_TCP_OK;
}

int aura_tcp_listener_close(AuraTcpListener *listener)
{
  if (listener == NULL || listener->fd < 0)
  {
    return 0;
  }
  int fd = listener->fd;
  listener->fd = -1;
  if (close(fd) != 0)
  {
    aura_tcp_error_errno("close listener");
  }
  return 1;
}

void aura_tcp_listener_destroy(AuraTcpListener *listener)
{
  if (listener == NULL)
  {
    return;
  }
  (void)aura_tcp_listener_close(listener);
  free(listener);
}

int aura_tcp_stream_close(AuraTcpStream *stream)
{
  if (stream == NULL || stream->fd < 0)
  {
    return 0;
  }
  int fd = stream->fd;
  stream->fd = -1;
  if (close(fd) != 0)
  {
    aura_tcp_error_errno("close stream");
  }
  return 1;
}

void aura_tcp_stream_destroy(AuraTcpStream *stream)
{
  if (stream == NULL)
  {
    return;
  }
  (void)aura_tcp_stream_close(stream);
  free(stream);
}

#else

AuraTcpStatus aura_tcp_listener_bind_endpoint(const char *endpoint,
                                              uint16_t *out_port,
                                              AuraTcpListener **out_listener)
{
  (void)endpoint;
  if (out_port != NULL)
  {
    *out_port = 0;
  }
  if (out_listener != NULL)
  {
    *out_listener = NULL;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_listener_bind(uint16_t port, uint16_t *out_port,
                                     AuraTcpListener **out_listener)
{
  (void)port;
  if (out_port != NULL)
  {
    *out_port = 0;
  }
  if (out_listener != NULL)
  {
    *out_listener = NULL;
  }
  (void)out_port;
  (void)out_listener;
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_listener_accept(AuraTcpListener *listener, int timeout_ms,
                                       AuraTcpStream **out_stream)
{
  (void)listener;
  (void)timeout_ms;
  if (out_stream != NULL)
  {
    *out_stream = NULL;
  }
  (void)out_stream;
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_connect(uint16_t port, int timeout_ms,
                                      AuraTcpStream **out_stream)
{
  (void)port;
  (void)timeout_ms;
  if (out_stream != NULL)
  {
    *out_stream = NULL;
  }
  (void)out_stream;
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_connect_endpoint(const char *endpoint,
                                               int timeout_ms,
                                               AuraTcpStream **out_stream)
{
  (void)endpoint;
  (void)timeout_ms;
  if (out_stream != NULL)
  {
    *out_stream = NULL;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_read(AuraTcpStream *stream, void *buffer, size_t capacity,
                                   size_t *out_bytes, int timeout_ms)
{
  (void)stream;
  (void)buffer;
  (void)capacity;
  (void)timeout_ms;
  if (out_bytes != NULL)
  {
    *out_bytes = 0;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_write(AuraTcpStream *stream, const void *buffer, size_t capacity,
                                    size_t *out_bytes, int timeout_ms)
{
  (void)stream;
  (void)buffer;
  (void)capacity;
  (void)timeout_ms;
  if (out_bytes != NULL)
  {
    *out_bytes = 0;
  }
  snprintf(aura_tcp_errbuf, sizeof(aura_tcp_errbuf), "tcp unsupported on this target");
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_read_exactly(AuraTcpStream *stream, void *buffer,
                                           size_t length, size_t *out_bytes,
                                           int timeout_ms)
{
  (void)stream; (void)buffer; (void)length; (void)timeout_ms;
  if (out_bytes != NULL) *out_bytes = 0;
  return AURA_TCP_UNSUPPORTED;
}

AuraTcpStatus aura_tcp_stream_write_all(AuraTcpStream *stream, const void *buffer,
                                        size_t length, size_t *out_bytes,
                                        int timeout_ms)
{
  (void)stream; (void)buffer; (void)length; (void)timeout_ms;
  if (out_bytes != NULL) *out_bytes = 0;
  return AURA_TCP_UNSUPPORTED;
}

int aura_tcp_listener_close(AuraTcpListener *listener)
{
  (void)listener;
  return 0;
}

void aura_tcp_listener_destroy(AuraTcpListener *listener)
{
  free(listener);
}

int aura_tcp_stream_close(AuraTcpStream *stream)
{
  (void)stream;
  return 0;
}

void aura_tcp_stream_destroy(AuraTcpStream *stream)
{
  free(stream);
}

#endif
