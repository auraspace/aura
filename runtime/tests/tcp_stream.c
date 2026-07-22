#include <assert.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

int main(void)
{
  AuraTcpListener *listener = NULL;
  uint16_t port = 0;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(listener != NULL);
  assert(port != 0);

  AuraTcpStream *accepted = NULL;
  assert(aura_tcp_listener_accept(listener, 0, &accepted) == AURA_TCP_PENDING);
  assert(accepted == NULL);
  assert(aura_tcp_listener_accept(listener, 10, &accepted) == AURA_TCP_TIMEOUT);
  assert(accepted == NULL);

  AuraTcpStream *client = NULL;
  assert(aura_tcp_stream_connect(0, 0, &client) == AURA_TCP_ERROR);
  assert(client == NULL);
  assert(strstr(aura_tcp_last_error(), "connect") != NULL);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(client != NULL);
  assert(aura_tcp_listener_accept(listener, 1000, &accepted) == AURA_TCP_OK);
  assert(accepted != NULL);

  const char request[] = "hello";
  size_t written = 0;
  assert(aura_tcp_stream_write(client, request, sizeof(request) - 1, &written, 1000) == AURA_TCP_OK);
  assert(written == sizeof(request) - 1);

  char first[3] = {0};
  size_t read = 0;
  assert(aura_tcp_stream_read(accepted, first, sizeof(first), &read, 1000) == AURA_TCP_OK);
  assert(read == sizeof(first));
  assert(memcmp(first, "hel", sizeof(first)) == 0);

  char remainder[3] = {0};
  assert(aura_tcp_stream_read(accepted, remainder, sizeof(remainder), &read, 1000) == AURA_TCP_OK);
  assert(read == 2);
  assert(memcmp(remainder, "lo", 2) == 0);

  const char response[] = "world";
  assert(aura_tcp_stream_write(accepted, response, sizeof(response) - 1, &written, 1000) == AURA_TCP_OK);
  assert(written == sizeof(response) - 1);
  char received[sizeof(response)] = {0};
  assert(aura_tcp_stream_read(client, received, sizeof(received) - 1, &read, 1000) == AURA_TCP_OK);
  assert(read == sizeof(response) - 1);
  assert(strcmp(received, response) == 0);

  assert(aura_tcp_stream_close(client) == 1);
  assert(aura_tcp_stream_close(client) == 0);
  assert(aura_tcp_stream_read(client, received, sizeof(received) - 1, &read, 0) == AURA_TCP_CLOSED);
  aura_tcp_stream_destroy(client);

  assert(aura_tcp_stream_read(accepted, received, sizeof(received) - 1, &read, 1000) == AURA_TCP_EOF);
  assert(aura_tcp_stream_close(accepted) == 1);
  assert(aura_tcp_stream_close(accepted) == 0);
  aura_tcp_stream_destroy(accepted);

  assert(aura_tcp_listener_close(listener) == 1);
  assert(aura_tcp_listener_close(listener) == 0);
  assert(aura_tcp_listener_accept(listener, 0, &accepted) == AURA_TCP_CLOSED);
  aura_tcp_listener_destroy(listener);

  aura_tcp_listener_destroy(NULL);
  aura_tcp_stream_destroy(NULL);
  return 0;
}
