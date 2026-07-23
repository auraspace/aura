#include <assert.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

typedef struct
{
  AuraHttpConnection *connection;
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

static AuraTaskPollState poll_http(AuraTaskFrame *frame)
{
  AsyncHttpTask *task = (AsyncHttpTask *)aura_task_frame_data(frame);
  return aura_http_connection_poll_async(frame, task->connection, ok_handler, NULL);
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

int main(void)
{
  test_two_pending_connections_progress_independently();
  test_pending_connection_cancels_and_closes();
  aura_gc_shutdown();
  puts("http async: passed");
  return 0;
}
