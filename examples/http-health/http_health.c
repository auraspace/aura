#include <assert.h>
#include <stdio.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static AuraHttpHandlerResult health_handler(const AuraHttpRequest *request,
                                            AuraHttpResponse *response,
                                            void *user_data)
{
  (void)user_data;
  assert(strcmp(request->method, "GET") == 0);
  assert(strcmp(request->target, "/health") == 0);
  assert(aura_http_response_set_body(response, "ok\n", 3) == AURA_HTTP_RESPONSE_OK);
  return AURA_HTTP_HANDLER_CLOSE;
}

typedef struct
{
  AuraHttpConnection *connection;
} HealthTask;

static AuraTaskPollState poll_health(AuraTaskFrame *frame)
{
  HealthTask *task = (HealthTask *)aura_task_frame_data(frame);
  return aura_http_connection_poll_async(frame, task->connection, health_handler, NULL);
}

static void write_all(AuraTcpStream *stream, const char *request)
{
  size_t sent = 0;
  size_t length = strlen(request);
  while (sent < length)
  {
    size_t written = 0;
    assert(aura_tcp_stream_write(stream, request + sent, length - sent, &written, 1000) ==
           AURA_TCP_OK);
    assert(written > 0);
    sent += written;
  }
}

static AuraTaskFrame *new_health_task(AuraHttpConnection *connection)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(HealthTask), poll_health, NULL);
  assert(frame != NULL);
  ((HealthTask *)aura_task_frame_data(frame))->connection = connection;
  return frame;
}

int main(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraHttpConnection *connection = NULL;
  AuraHttpConnection *bad_connection = NULL;
  AuraTcpListener *listener = NULL;
  AuraTcpStream *client = NULL;
  AuraTcpStream *bad_client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  AuraTaskFrame *bad_frame = NULL;
  uint16_t port = 0;
  char response[512] = {0};
  size_t used = 0;

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  assert(aura_tcp_listener_bind(0, &port, &listener) == AURA_TCP_OK);
  assert(aura_http_server_create(listener, 2, &config, &server) == AURA_HTTP_CONNECTION_OK);
  printf("http-health: listening on 127.0.0.1:%u\n", (unsigned)port);
  fflush(stdout);

  assert(aura_tcp_stream_connect(port, 1000, &client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &connection) == AURA_HTTP_CONNECTION_OK);
  executor = aura_task_executor_new();
  assert(executor != NULL);
  frame = new_health_task(connection);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_PENDING);
  write_all(client, "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n");
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(frame) == AURA_TASK_COMPLETE);
  while (used + 1 < sizeof(response))
  {
    size_t received = 0;
    AuraTcpStatus status = aura_tcp_stream_read(client, response + used,
                                                sizeof(response) - used - 1, &received, 1000);
    if (status != AURA_TCP_OK || received == 0)
    {
      break;
    }
    used += received;
    response[used] = '\0';
    if (strstr(response, "\r\n\r\nok\n") != NULL)
    {
      break;
    }
  }
  assert(strstr(response, "HTTP/1.1 200 OK") != NULL);
  assert(strstr(response, "\r\n\r\nok\n") != NULL);
  printf("http-health: client received 200\n");

  assert(aura_tcp_stream_connect(port, 1000, &bad_client) == AURA_TCP_OK);
  assert(aura_http_server_accept(server, 1000, &bad_connection) == AURA_HTTP_CONNECTION_OK);
  bad_frame = new_health_task(bad_connection);
  assert(aura_task_executor_submit(executor, bad_frame) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  write_all(bad_client, "NOT HTTP\r\n\r\n");
  assert(aura_task_executor_poll_waiting(executor, 1000) == 1);
  assert(aura_task_executor_run_one(executor) == 1);
  assert(aura_task_frame_state(bad_frame) == AURA_TASK_COMPLETE);
  memset(response, 0, sizeof(response));
  used = 0;
  while (used + 1 < sizeof(response))
  {
    size_t received = 0;
    AuraTcpStatus status = aura_tcp_stream_read(bad_client, response + used,
                                                sizeof(response) - used - 1,
                                                &received, 1000);
    if (status != AURA_TCP_OK || received == 0)
    {
      break;
    }
    used += received;
    response[used] = '\0';
    if (strstr(response, "\r\n\r\n") != NULL)
    {
      break;
    }
  }
  assert(strstr(response, "HTTP/1.1 400 Bad Request") != NULL);
  printf("http-health: malformed request received 400\n");

  assert(aura_task_executor_release(executor, &frame) == 1);
  assert(aura_task_executor_release(executor, &bad_frame) == 1);
  aura_http_connection_destroy(connection);
  aura_http_connection_destroy(bad_connection);
  aura_tcp_stream_destroy(client);
  aura_tcp_stream_destroy(bad_client);
  aura_task_executor_shutdown(executor);
  assert(aura_http_server_shutdown(server) == 1);
  assert(aura_http_server_destroy(server) == 1);
  puts("http-health: shutdown complete");
  return 0;
}
