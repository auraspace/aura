#include <assert.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

int main(void)
{
  AuraTcpListener *listener = NULL;
  AuraTcpStream *client = NULL;
  AuraTcpStream *server = NULL;
  size_t count = 0;
  uint16_t port = 0;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_tcp_listener_accept(listener, 1000, &server) == AURA_TCP_OK);

  unsigned char timeout_byte = 0;
  assert(aura_tcp_stream_read_exactly(server, &timeout_byte, 1, &count, 10) == AURA_TCP_TIMEOUT);
  assert(count == 0);

  const unsigned char payload[] = {0x00, 0x01, 0xff, 0x7f};
  assert(aura_tcp_stream_write_all(client, payload, sizeof(payload), &count, 1000) == AURA_TCP_OK);
  assert(count == sizeof(payload));
  unsigned char received[sizeof(payload)] = {0};
  assert(aura_tcp_stream_read_exactly(server, received, sizeof(received), &count, 1000) == AURA_TCP_OK);
  assert(count == sizeof(received) && memcmp(received, payload, sizeof(payload)) == 0);

  assert(aura_tcp_stream_write_all(client, payload, 2, &count, 1000) == AURA_TCP_OK);
  assert(aura_tcp_stream_close(client) == 1);
  assert(aura_tcp_stream_read_exactly(server, received, 3, &count, 1000) == AURA_TCP_PARTIAL_EOF);
  assert(count == 2 && received[0] == 0x00 && received[1] == 0x01);
  aura_tcp_stream_destroy(client);
  aura_tcp_stream_destroy(server);
  aura_tcp_listener_destroy(listener);
  return 0;
}
