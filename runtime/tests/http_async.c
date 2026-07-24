#include <assert.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

typedef struct
{
  AuraHttpConnection *connection;
  AuraHttpHandler handler;
  void *user_data;
} AsyncHttpTask;

static AuraHttpHandlerResult ok_handler(const AuraHttpRequest *request,
                                        AuraHttpResponse *response,
                                        void *user_data)
{
  (void)user_data;
  assert(strcmp(request->method, "GET") == 0);
  assert(strcmp(request->target, "/health") == 0);
  assert(aura_http_response_set_body(response, "ok", 2) == AURA_HTTP_RESPONSE_OK);
  return AURA_HTTP_HANDLER_CLOSE;
}

typedef struct
{
  int calls;
  unsigned char *body;
  size_t body_length;
} AsyncKeepAliveState;

static AuraHttpHandlerResult keep_alive_handler(const AuraHttpRequest *request,
                                                AuraHttpResponse *response,
                                                void *user_data)
{
  AsyncKeepAliveState *state = (AsyncKeepAliveState *)user_data;
  const char *expected = state->calls == 0 ? "/one" : "/two";
  const void *body = state->body != NULL ? (const void *)state->body : (const void *)"ok";
  size_t body_length = state->body != NULL ? state->body_length : 2;
  assert(strcmp(request->target, expected) == 0);
  state->calls++;
  assert(aura_http_response_set_body(response, body, body_length) ==
         AURA_HTTP_RESPONSE_OK);
  return AURA_HTTP_HANDLER_KEEP_ALIVE;
}

static AuraHttpHandlerResult large_response_handler(const AuraHttpRequest *request,
                                                    AuraHttpResponse *response,
                                                    void *user_data)
{
  AsyncKeepAliveState *state = (AsyncKeepAliveState *)user_data;
  assert(strcmp(request->target, "/large") == 0);
  state->calls++;
  assert(aura_http_response_set_body(response, state->body, state->body_length) ==
         AURA_HTTP_RESPONSE_OK);
  return AURA_HTTP_HANDLER_KEEP_ALIVE;
}

static AuraTaskPollState poll_http(AuraTaskFrame *frame)
{
  AsyncHttpTask *task = (AsyncHttpTask *)aura_task_frame_data(frame);
  return aura_http_connection_poll_async(frame, task->connection,
                                         task->handler != NULL ? task->handler : ok_handler,
                                         task->user_data);
}

static void write_request(AuraTcpStream *client)
{
  const char request[] = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
  size_t written = 0;
  assert(aura_tcp_stream_write(client, request, sizeof(request) - 1, &written, 1000) ==
         AURA_TCP_OK);
  assert(written == sizeof(request) - 1);
}

static void read_ok(AuraTcpStream *client)
{
  char response[512] = {0};
  size_t used = 0;
  while (used + 1 < sizeof(response))
  {
    size_t received = 0;
    AuraTcpStatus status = aura_tcp_stream_read(client, response + used,
                                                sizeof(response) - used - 1,
                                                &received, 1000);
    assert(status == AURA_TCP_OK);
    assert(received > 0);
    used += received;
    response[used] = '\0';
    if (strstr(response, "\r\n\r\nok") != NULL)
    {
      break;
    }
  }
  assert(strstr(response, "HTTP/1.1 200 OK") != NULL);
  assert(strstr(response, "\r\n\r\nok") != NULL);
}

static AuraTaskFrame *new_http_task(AuraHttpConnection *connection)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(AsyncHttpTask), poll_http, NULL);
  assert(frame != NULL);
  ((AsyncHttpTask *)aura_task_frame_data(frame))->connection = connection;
  ((AsyncHttpTask *)aura_task_frame_data(frame))->handler = ok_handler;
  return frame;
}

static void test_two_pending_connections_progress_independently(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connections[2] = {NULL, NULL};
  AuraTcpStream *clients[2] = {NULL, NULL};
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frames[2] = {NULL, NULL};
  uint16_t port = 0;
  int i;

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_http_server_create(listener, 2, &config, &server) == AURA_HTTP_CONNECTION_OK);
  for (i = 0; i < 2; i++)
  {
    assert(aura_tcp_stream_connect(port, 1000, &clients[i]) == AURA_TCP_OK);
    assert(aura_http_server_accept(server, 1000, &connections[i]) ==
           AURA_HTTP_CONNECTION_OK);
  }

  executor = aura_task_executor_new();
  assert(executor != NULL);
  for (i = 0; i < 2; i++)
  {
    frames[i] = new_http_task(connections[i]);
    assert(aura_task_executor_submit(executor, frames[i]) == 1);
  }
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frames[0]) == AURA_TASK_PENDING);
  assert(aura_task_frame_state(frames[1]) == AURA_TASK_PENDING);

  write_request(clients[0]);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frames[0]) == AURA_TASK_COMPLETE);
  assert(aura_task_frame_state(frames[1]) == AURA_TASK_PENDING);
  read_ok(clients[0]);

  write_request(clients[1]);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frames[1]) == AURA_TASK_COMPLETE);
  read_ok(clients[1]);

  for (i = 0; i < 2; i++)
  {
    assert(aura_task_executor_release(executor, &frames[i]) == 1);
    aura_http_connection_destroy(connections[i]);
    aura_tcp_stream_destroy(clients[i]);
  }
  aura_task_executor_shutdown(executor);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_pending_connection_cancels_and_closes(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  uint16_t port = 0;

  aura_http_connection_config_init(&config);
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_http_server_create(listener, 1, &config, &server) == AURA_HTTP_CONNECTION_OK);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  executor = aura_task_executor_new();
  assert(executor != NULL);
  frame = new_http_task(connection);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_executor_cancel(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_CANCELLED);
  assert(aura_http_server_active_connections(server) == 0);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  aura_task_executor_shutdown(executor);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_peer_disconnect_completes_pending_request(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  AsyncKeepAliveState state = {0, NULL, 0};
  uint16_t port = 0;

  aura_http_connection_config_init(&config);
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_http_server_create(listener, 1, &config, &server) ==
         AURA_HTTP_CONNECTION_OK);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) ==
         AURA_HTTP_CONNECTION_OK);

  executor = aura_task_executor_new();
  assert(executor != NULL);
  frame = new_http_task(connection);
  ((AsyncHttpTask *)aura_task_frame_data(frame))->handler = keep_alive_handler;
  ((AsyncHttpTask *)aura_task_frame_data(frame))->user_data = &state;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_is_waiting(frame));

  /* A peer that disconnects before sending a complete request must wake the
   * async operation, close the server-side connection, and never invoke the
   * request handler with a partial request. */
  assert(aura_tcp_stream_close(client) == 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_FAILED);
  assert(state.calls == 0);
  assert(aura_http_server_active_connections(server) == 0);

  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  aura_task_executor_shutdown(executor);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_async_keep_alive_preserves_pipelined_requests(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  AsyncKeepAliveState state = {0, NULL, 0};
  char response[1024] = {0};
  size_t written = 0;
  size_t received = 0;
  uint16_t port = 0;
  const char requests[] =
      "GET /one HTTP/1.1\r\nHost: localhost\r\n\r\n"
      "GET /two HTTP/1.1\r\nHost: localhost\r\n\r\n";

  aura_http_connection_config_init(&config);
  config.max_requests = 2;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_http_server_create(listener, 1, &config, &server) ==
         AURA_HTTP_CONNECTION_OK);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) ==
         AURA_HTTP_CONNECTION_OK);
  executor = aura_task_executor_new();
  assert(executor != NULL);
  frame = new_http_task(connection);
  ((AsyncHttpTask *)aura_task_frame_data(frame))->handler = keep_alive_handler;
  ((AsyncHttpTask *)aura_task_frame_data(frame))->user_data = &state;
  /* The task starts with no request and must park on socket readability. */
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_is_waiting(frame));
  assert(aura_tcp_stream_write(client, requests, sizeof(requests) - 1, &written,
                               1000) == AURA_TCP_OK);
  assert(written == sizeof(requests) - 1);
  /* Both requests are already in the connection-owned buffer. One poll turn
   * must serve both and retain keep-alive only until max_requests is reached. */
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(state.calls == 2);
  while (received + 1 < sizeof(response) &&
         strstr(response, "Connection: close") == NULL)
  {
    size_t chunk = 0;
    assert(aura_tcp_stream_read(client, response + received,
                                sizeof(response) - received - 1, &chunk, 1000) ==
           AURA_TCP_OK);
    assert(chunk > 0);
    received += chunk;
    response[received] = '\0';
  }
  assert(strstr(response, "HTTP/1.1 200 OK") != NULL);
  assert(strstr(response, "Connection: keep-alive") != NULL);
  assert(strstr(response, "Connection: close") != NULL);
  assert(strstr(response, "\r\n\r\nok\r\n") == NULL);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_async_keep_alive_suspends_between_requests(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  AsyncKeepAliveState state = {0, NULL, 0};
  uint16_t port = 0;

  aura_http_connection_config_init(&config);
  config.max_requests = 2;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_http_server_create(listener, 1, &config, &server) ==
         AURA_HTTP_CONNECTION_OK);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) ==
         AURA_HTTP_CONNECTION_OK);

  executor = aura_task_executor_new();
  assert(executor != NULL);
  frame = new_http_task(connection);
  ((AsyncHttpTask *)aura_task_frame_data(frame))->handler = keep_alive_handler;
  ((AsyncHttpTask *)aura_task_frame_data(frame))->user_data = &state;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_is_waiting(frame));

  /* The first request arrives alone. After its response, the connection must
   * retain its typed stream/HTTP state and suspend again instead of closing. */
  {
    const char first[] = "GET /one HTTP/1.1\r\nHost: localhost\r\n\r\n";
    size_t written = 0;
    assert(aura_tcp_stream_write(client, first, sizeof(first) - 1, &written,
                                 1000) == AURA_TCP_OK);
    assert(written == sizeof(first) - 1);
  }
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_is_waiting(frame));
  assert(state.calls == 1);
  read_ok(client);

  /* A later request must wake the same task and reach the handler. */
  {
    const char second[] = "GET /two HTTP/1.1\r\nHost: localhost\r\n\r\n";
    size_t written = 0;
    assert(aura_tcp_stream_write(client, second, sizeof(second) - 1, &written,
                                 1000) == AURA_TCP_OK);
    assert(written == sizeof(second) - 1);
  }
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(state.calls == 2);
  read_ok(client);

  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

static void test_async_write_backpressure_resumes_without_blocking(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraTcpListener *listener = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpStream *client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  AsyncKeepAliveState state = {0, NULL, 4 * 1024 * 1024};
  unsigned char read_buffer[65536];
  char prefix[256] = {0};
  size_t prefix_used = 0;
  size_t total_received = 0;
  size_t written = 0;
  uint16_t port = 0;
  int small_buffer = 1024;
  const char request[] = "GET /large HTTP/1.1\r\nHost: localhost\r\n\r\n";

  state.body = (unsigned char *)malloc(state.body_length);
  assert(state.body != NULL);
  memset(state.body, 'B', state.body_length);
  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_http_server_create(listener, 1, &config, &server) ==
         AURA_HTTP_CONNECTION_OK);
  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) ==
         AURA_HTTP_CONNECTION_OK);
  assert(setsockopt(connection->stream->fd, SOL_SOCKET, SO_SNDBUF,
                    &small_buffer, sizeof(small_buffer)) == 0);

  executor = aura_task_executor_new();
  assert(executor != NULL);
  frame = new_http_task(connection);
  ((AsyncHttpTask *)aura_task_frame_data(frame))->handler = large_response_handler;
  ((AsyncHttpTask *)aura_task_frame_data(frame))->user_data = &state;
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_is_waiting(frame));
  assert(aura_tcp_stream_write(client, request, sizeof(request) - 1, &written,
                               1000) == AURA_TCP_OK);
  assert(written == sizeof(request) - 1);
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  /* A 4 MiB response and a deliberately small server send buffer must park
   * on POLLOUT before the client drains the response. */
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  assert(aura_task_frame_is_waiting(frame));

  for (size_t turn = 0; turn < 10000 &&
                         aura_task_frame_state(frame) == AURA_TASK_PENDING;
       turn++)
  {
    size_t received = 0;
    AuraTcpStatus status = aura_tcp_stream_read(client, read_buffer,
                                                sizeof(read_buffer), &received, 1000);
    assert(status == AURA_TCP_OK);
    assert(received > 0);
    if (prefix_used < sizeof(prefix) - 1)
    {
      size_t copy = received < sizeof(prefix) - 1 - prefix_used
                        ? received
                        : sizeof(prefix) - 1 - prefix_used;
      memcpy(prefix + prefix_used, read_buffer, copy);
      prefix_used += copy;
      prefix[prefix_used] = '\0';
    }
    total_received += received;
    if (aura_task_executor_poll_waiting(executor, 1000) == 1)
    {
      assert(aura_task_executor_run_one(executor) == 1);
    }
  }
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  assert(state.calls == 1);
  assert(strstr(prefix, "HTTP/1.1 200 OK") != NULL);
  while (total_received < state.body_length)
  {
    size_t received = 0;
    AuraTcpStatus status = aura_tcp_stream_read(client, read_buffer,
                                                sizeof(read_buffer), &received, 1000);
    if (status == AURA_TCP_EOF)
    {
      break;
    }
    assert(status == AURA_TCP_OK);
    assert(received > 0);
    total_received += received;
  }
  assert(total_received >= state.body_length);

  free(state.body);
  assert(aura_task_executor_release(executor, &frame) == 1);
  aura_task_executor_shutdown(executor);
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
}

int main(void)
{
  test_two_pending_connections_progress_independently();
  test_pending_connection_cancels_and_closes();
  test_peer_disconnect_completes_pending_request();
  test_async_keep_alive_preserves_pipelined_requests();
  test_async_keep_alive_suspends_between_requests();
  test_async_write_backpressure_resumes_without_blocking();
  aura_gc_shutdown();
  puts("http async: passed");
  return 0;
}
