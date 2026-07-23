/* Focused primitive bridge for std.net acceptance tests.
 *
 * It owns no Aura memory and retains no caller String. Returned text lives in
 * thread-local storage and is copied by Aura's String FFI conversion. This is
 * intentionally a companion library, not a replacement for runtime socket
 * handles; see std/net/README.md.
 */
#include <errno.h>
#include <netinet/in.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

#define AURA_NET_MAX_PAYLOAD (64u * 1024u)
#define AURA_NET_MAX_HTTP (16u * 1024u * 1024u)
#define AURA_NET_MAX_RESPONSE (16u * 1024u * 1024u)

static _Thread_local char aura_net_result[AURA_NET_MAX_RESPONSE + 1u];

static int wait_fd(int fd, short events, int timeout_ms)
{
  struct pollfd pfd;
  int result;
  if (timeout_ms < 0) return -1;
  pfd.fd = fd; pfd.events = events; pfd.revents = 0;
  do { result = poll(&pfd, 1, timeout_ms); } while (result < 0 && errno == EINTR);
  return result > 0 && (pfd.revents & events) != 0 ? 0 : -1;
}

const char *aura_net_loopback_echo(const char *payload, int64_t timeout_ms)
{
  int listener = -1, client = -1, peer = -1;
  struct sockaddr_in address;
  socklen_t address_len = (socklen_t)sizeof(address);
  size_t length, used = 0;
  ssize_t count;
  if (payload == NULL || timeout_ms < 0 || timeout_ms > INT32_MAX) return "";
  length = strlen(payload);
  if (length > AURA_NET_MAX_PAYLOAD) return "";
  listener = socket(AF_INET, SOCK_STREAM, 0);
  client = socket(AF_INET, SOCK_STREAM, 0);
  if (listener < 0 || client < 0) goto fail;
  memset(&address, 0, sizeof(address));
  address.sin_family = AF_INET;
  address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
  address.sin_port = htons(0);
  if (bind(listener, (struct sockaddr *)&address, sizeof(address)) != 0 ||
      listen(listener, 1) != 0 ||
      getsockname(listener, (struct sockaddr *)&address, &address_len) != 0 ||
      connect(client, (struct sockaddr *)&address, address_len) != 0)
    goto fail;
  peer = accept(listener, NULL, NULL);
  if (peer < 0 || (length != 0 && send(client, payload, length, 0) != (ssize_t)length))
    goto fail;
  while (used < length) {
    if (wait_fd(peer, POLLIN, (int)timeout_ms) != 0) goto fail;
    count = recv(peer, aura_net_result + used, length - used, 0);
    if (count <= 0) goto fail;
    used += (size_t)count;
  }
  aura_net_result[used] = '\0';
  close(peer); close(client); close(listener);
  return aura_net_result;
fail:
  if (peer >= 0) close(peer);
  if (client >= 0) close(client);
  if (listener >= 0) close(listener);
  aura_net_result[0] = '\0';
  return aura_net_result;
}

int64_t aura_net_http_request_status(const char *request)
{
  const char *line_end, *space1, *space2, *body;
  size_t length, line_length;
  if (request == NULL) return 400;
  length = strlen(request);
  if (length > AURA_NET_MAX_HTTP) return 413;
  line_end = strstr(request, "\r\n");
  if (line_end == NULL) return 400;
  line_length = (size_t)(line_end - request);
  space1 = memchr(request, ' ', line_length);
  space2 = space1 == NULL ? NULL : memchr(space1 + 1, ' ',
                                          line_length - (size_t)(space1 + 1 - request));
  if (space1 == NULL || space2 == NULL || space1 == request ||
      memcmp(space2 + 1, "HTTP/1.1", 8) != 0 ||
      (size_t)(line_end - (space2 + 1)) != 8) return 400;
  if ((size_t)(space1 - request) != 3 || memcmp(request, "GET", 3) != 0) return 405;
  body = strstr(line_end + 2, "\r\n\r\n");
  if (body == NULL) return 400;
  if ((size_t)(body + 4 - request) > AURA_NET_MAX_HTTP) return 413;
  return 200;
}

const char *aura_net_http_response(int64_t status, const char *body)
{
  const char *reason = "OK";
  int written;
  size_t body_length;
  if (body == NULL || status < 100 || status > 599) return "";
  body_length = strlen(body);
  if (body_length > AURA_NET_MAX_HTTP) return "";
  if (status == 400) reason = "Bad Request";
  else if (status == 404) reason = "Not Found";
  else if (status == 405) reason = "Method Not Allowed";
  else if (status == 500) reason = "Internal Server Error";
  written = snprintf(aura_net_result, sizeof(aura_net_result),
                      "HTTP/1.1 %lld %s\r\nContent-Length: %zu\r\n"
                      "Connection: close\r\n\r\n%s", (long long)status,
                      reason, body_length, body);
  if (written < 0 || (size_t)written >= sizeof(aura_net_result)) {
    aura_net_result[0] = '\0';
  }
  return aura_net_result;
}
