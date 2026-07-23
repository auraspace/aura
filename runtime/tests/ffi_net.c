#include <assert.h>
#include <stdint.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

static void test_public_tcp_header_and_lifetime(void)
{
  AuraTcpListener *listener = NULL;
  AuraTcpStream *client = NULL;
  AuraTcpStream *peer = NULL;
  uint16_t port = 0;
  size_t written = 0;
  size_t read = 0;
  char buffer[8] = {0};

  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(port != 0 && listener != NULL);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_tcp_listener_accept(listener, 1000, &peer) == AURA_TCP_OK);
  assert(aura_tcp_stream_write(client, "aura", 4, &written, 1000) == AURA_TCP_OK);
  assert(written == 4);
  assert(aura_tcp_stream_read(peer, buffer, sizeof(buffer), &read, 1000) == AURA_TCP_OK);
  assert(read == 4 && memcmp(buffer, "aura", 4) == 0);
  assert(aura_tcp_stream_close(peer) == 1);
  assert(aura_tcp_stream_close(peer) == 0);
  aura_tcp_stream_destroy(peer);
  aura_tcp_stream_destroy(client);
  assert(aura_tcp_listener_close(listener) == 1);
  aura_tcp_listener_destroy(listener);
}

int main(void)
{
  test_public_tcp_header_and_lifetime();
  aura_gc_shutdown();
  return 0;
}
