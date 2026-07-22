#include <assert.h>
#include <stdio.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  int calls;
  int fail;
} HandlerState;

static AuraHttpHandlerResult test_handler(const AuraHttpRequest *request,
                                          AuraHttpResponse *response,
                                          void *user_data)
{
  HandlerState *state = (HandlerState *)user_data;
  const char *body = state->calls == 0 ? "first" : "second";
  (void)request;
  state->calls++;
  if (state->fail)
  {
    return AURA_HTTP_HANDLER_ERROR;
  }
  assert(aura_http_response_set_body(response, body, strlen(body)) ==
         AURA_HTTP_RESPONSE_OK);
  return AURA_HTTP_HANDLER_KEEP_ALIVE;
}

static void write_request(AuraTcpStream *client, const char *request)
{
  size_t written = 0;
  assert(aura_tcp_stream_write(client, request, strlen(request), &written, 1000) ==
         AURA_TCP_OK);
  assert(written == strlen(request));
}

static size_t read_until(AuraTcpStream *client, char *output, size_t capacity,
                         const char *needle)
{
  size_t used = 0;
  size_t needle_length = strlen(needle);
  while (used + 1 < capacity)
  {
    size_t read = 0;
    AuraTcpStatus status = aura_tcp_stream_read(client, output + used,
                                                capacity - used - 1, &read, 1000);
    if (status != AURA_TCP_OK || read == 0)
    {
      break;
    }
    used += read;
    output[used] = '\0';
    if (used >= needle_length && strstr(output, needle) != NULL)
    {
      break;
    }
  }
  return used;
}

static void make_server(AuraHttpServer **server, AuraTcpListener **listener,
                        AuraHttpConnectionConfig *config, uint16_t *port,
                        size_t max_connections)
{
  assert(aura_tcp_listener_bind(0, port, listener) == AURA_TCP_OK);
  assert(aura_http_server_create(*listener, max_connections, config, server) ==
         AURA_HTTP_CONNECTION_OK);
}

static void test_single_request_close(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server;
  AuraTcpListener *listener;
  AuraHttpConnection *connection;
  AuraTcpStream *client;
  HandlerState state = {0, 0};
  char response[512] = {0};
  uint16_t port;

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  make_server(&server, &listener, &config, &port, 2);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  write_request(client, "GET /health HTTP/1.1\r\nHost: local\r\n\r\n");
  assert(aura_http_connection_run(connection, test_handler, &state) ==
         AURA_HTTP_CONNECTION_OK);
  assert(state.calls == 1);
  assert(read_until(client, response, sizeof(response), "first") > 0);
  assert(strstr(response, "Connection: close\r\n") != NULL);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  assert(aura_http_server_active_connections(server) == 0);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_persistent_requests_and_limit(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server;
  AuraTcpListener *listener;
  AuraHttpConnection *connection;
  AuraTcpStream *client;
  HandlerState state = {0, 0};
  char response[1024] = {0};
  uint16_t port;

  aura_http_connection_config_init(&config);
  config.max_requests = 2;
  make_server(&server, &listener, &config, &port, 1);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  write_request(client, "GET /one HTTP/1.1\r\nHost: local\r\n\r\n"
                       "GET /two HTTP/1.1\r\nHost: local\r\n\r\n");
  assert(aura_http_connection_run(connection, test_handler, &state) ==
         AURA_HTTP_CONNECTION_OK);
  assert(state.calls == 2);
  assert(read_until(client, response, sizeof(response), "second") > 0);
  assert(strstr(response, "first") != NULL);
  assert(strstr(response, "second") != NULL);
  assert(strstr(response, "Connection: close\r\n") != NULL);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  assert(aura_http_server_active_connections(server) == 0);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_timeout_disconnect_and_shutdown(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server;
  AuraTcpListener *listener;
  AuraHttpConnection *connection;
  AuraHttpConnection *active_connection;
  AuraTcpStream *client;
  AuraTcpStream *second_client;
  uint16_t port;

  aura_http_connection_config_init(&config);
  config.read_timeout_ms = 10;
  config.idle_timeout_ms = 10;
  make_server(&server, &listener, &config, &port, 1);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  assert(aura_http_connection_run(connection, test_handler, NULL) ==
         AURA_HTTP_CONNECTION_TIMEOUT);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);

  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &active_connection) ==
         AURA_HTTP_CONNECTION_OK);
  aura_tcp_stream_close(client);
  assert(aura_http_connection_run(active_connection, test_handler, NULL) ==
         AURA_HTTP_CONNECTION_DISCONNECTED);
  aura_http_connection_destroy(active_connection);
  aura_tcp_stream_destroy(client);

  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &active_connection) ==
         AURA_HTTP_CONNECTION_OK);
  assert(aura_tcp_stream_connect(port, 1000, &second_client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 0, &connection) == AURA_HTTP_CONNECTION_LIMIT);
  aura_tcp_stream_destroy(second_client);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_accept(server, 0, &connection) == AURA_HTTP_CONNECTION_SHUTDOWN);
  aura_http_connection_close(active_connection);
  aura_http_connection_destroy(active_connection);
  aura_tcp_stream_destroy(client);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_handler_failure_maps_to_500(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server;
  AuraTcpListener *listener;
  AuraHttpConnection *connection;
  AuraTcpStream *client;
  HandlerState state = {0, 1};
  char response[512] = {0};
  uint16_t port;

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  make_server(&server, &listener, &config, &port, 1);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  write_request(client, "GET /failure HTTP/1.1\r\nHost: local\r\n\r\n");
  assert(aura_http_connection_run(connection, test_handler, &state) ==
         AURA_HTTP_CONNECTION_OK);
  assert(read_until(client, response, sizeof(response), "handler_failure") > 0);
  assert(strstr(response, "500 Internal Server Error") != NULL);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

int main(void)
{
  test_single_request_close();
  test_persistent_requests_and_limit();
  test_timeout_disconnect_and_shutdown();
  test_handler_failure_maps_to_500();
  puts("http connection tests passed");
  return 0;
}
