#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static AuraHttpHandlerResult close_handler(const AuraHttpRequest *request,
                                           AuraHttpResponse *response,
                                           void *user_data)
{
  (void)request;
  (void)user_data;
  assert(aura_http_response_set_body(response, "ok", 2) == AURA_HTTP_RESPONSE_OK);
  return AURA_HTTP_HANDLER_CLOSE;
}

static void make_server(AuraHttpServer **server, AuraTcpListener **listener,
                        AuraHttpConnectionConfig *config, uint16_t *port,
                        size_t max_connections)
{
  assert(aura_tcp_listener_bind(0, port, listener) == AURA_TCP_OK);
  assert(aura_http_server_create(*listener, max_connections, config, server) ==
         AURA_HTTP_CONNECTION_OK);
}

static void write_all(AuraTcpStream *stream, const void *data, size_t length)
{
  const unsigned char *bytes = (const unsigned char *)data;
  size_t sent = 0;
  while (sent < length)
  {
    size_t written = 0;
    assert(aura_tcp_stream_write(stream, bytes + sent, length - sent, &written, 1000) ==
           AURA_TCP_OK);
    assert(written > 0);
    sent += written;
  }
}

static size_t read_response(AuraTcpStream *client, char *output, size_t capacity)
{
  size_t used = 0;
  assert(capacity > 1);
  while (used + 1 < capacity)
  {
    size_t received = 0;
    AuraTcpStatus status = aura_tcp_stream_read(client, output + used,
                                                capacity - used - 1, &received, 1000);
    if (status != AURA_TCP_OK || received == 0)
    {
      break;
    }
    used += received;
    output[used] = '\0';
    if (strstr(output, "\r\n\r\n") != NULL)
    {
      break;
    }
  }
  output[used] = '\0';
  return used;
}

static void close_connection(AuraHttpConnection *connection, AuraTcpStream *client)
{
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
}

static void test_oversized_request_rejected_with_413(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  uint16_t port = 0;
  size_t line_length = AURA_HTTP_MAX_REQUEST_LINE_BYTES + 2;
  size_t request_length = line_length + 2;
  char *request = (char *)malloc(request_length);
  char response[512] = {0};
  size_t i;

  assert(request != NULL);
  memcpy(request, "GET /", 5);
  for (i = 5; i < line_length; i++)
  {
    request[i] = 'x';
  }
  request[line_length] = '\r';
  request[line_length + 1] = '\n';

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  make_server(&server, &listener, &config, &port, 1);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  write_all(client, request, request_length);
  assert(aura_http_connection_run(connection, close_handler, NULL) ==
         AURA_HTTP_CONNECTION_OK);
  assert(read_response(client, response, sizeof(response)) > 0);
  assert(strstr(response, "413 Payload Too Large") != NULL);
  assert(strstr(response, "payload_too_large") != NULL);
  close_connection(connection, client);
  free(request);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_malformed_framing_rejected_with_400(void)
{
  const char request[] = "POST /upload HTTP/1.1\r\n"
                         "Content-Length: 3\r\n"
                         "Content-Length: 4\r\n\r\n";
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  uint16_t port = 0;
  char response[512] = {0};

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  make_server(&server, &listener, &config, &port, 1);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  write_all(client, request, sizeof(request) - 1);
  assert(aura_http_connection_run(connection, close_handler, NULL) ==
         AURA_HTTP_CONNECTION_OK);
  assert(read_response(client, response, sizeof(response)) > 0);
  assert(strstr(response, "400 Bad Request") != NULL);
  assert(strstr(response, "bad_request") != NULL);
  close_connection(connection, client);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_slow_partial_client_times_out_without_leaking(void)
{
  const char partial[] = "POST /slow HTTP/1.1\r\nContent-Length: 4\r\n\r\nx";
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  uint16_t port = 0;

  aura_http_connection_config_init(&config);
  config.read_timeout_ms = 20;
  config.idle_timeout_ms = 20;
  make_server(&server, &listener, &config, &port, 1);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  write_all(client, partial, sizeof(partial) - 1);
  assert(aura_http_connection_run(connection, close_handler, NULL) ==
         AURA_HTTP_CONNECTION_TIMEOUT);
  assert(aura_http_server_active_connections(server) == 0);
  close_connection(connection, client);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_concurrent_connections_are_bounded(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *first = NULL;
  AuraHttpConnection *second = NULL;
  AuraHttpConnection *third = NULL;
  AuraTcpStream *clients[3] = {NULL, NULL, NULL};
  uint16_t port = 0;
  size_t i;

  aura_http_connection_config_init(&config);
  make_server(&server, &listener, &config, &port, 2);
  for (i = 0; i < 3; i++)
  {
    assert(aura_tcp_stream_connect(port, 1000, &clients[i]) == AURA_TCP_OK);
  }
  assert(aura_http_server_accept(server, 1000, &first) == AURA_HTTP_CONNECTION_OK);
  assert(aura_http_server_accept(server, 1000, &second) == AURA_HTTP_CONNECTION_OK);
  assert(aura_http_server_active_connections(server) == 2);
  assert(aura_http_server_accept(server, 0, &third) == AURA_HTTP_CONNECTION_LIMIT);
  assert(third == NULL);
  aura_http_connection_destroy(first);
  aura_http_connection_destroy(second);
  for (i = 0; i < 3; i++)
  {
    aura_tcp_stream_destroy(clients[i]);
  }
  assert(aura_http_server_active_connections(server) == 0);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_forced_shutdown_stops_accept_and_releases_active(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *active = NULL;
  AuraHttpConnection *rejected = NULL;
  AuraTcpStream *client = NULL;
  AuraTcpStream *pending_client = NULL;
  uint16_t port = 0;

  aura_http_connection_config_init(&config);
  make_server(&server, &listener, &config, &port, 1);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &active) == AURA_HTTP_CONNECTION_OK);
  assert(aura_tcp_stream_connect(port, 1000, &pending_client) == AURA_TCP_OK);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_accept(server, 0, &rejected) == AURA_HTTP_CONNECTION_SHUTDOWN);
  assert(rejected == NULL);
  aura_http_connection_destroy(active);
  aura_tcp_stream_destroy(client);
  aura_tcp_stream_destroy(pending_client);
  assert(aura_http_server_active_connections(server) == 0);
  assert(aura_http_server_destroy(server) == 1);
}

int main(void)
{
  test_oversized_request_rejected_with_413();
  test_malformed_framing_rejected_with_400();
  test_slow_partial_client_times_out_without_leaking();
  test_concurrent_connections_are_bounded();
  test_forced_shutdown_stops_accept_and_releases_active();
  puts("http hardening tests passed");
  return 0;
}
