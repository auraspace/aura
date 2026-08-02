/* ---- Process argv (std.io.args) ----
 * Stashed from C main before aura_main so user programs keep fun main().
 * Each returned string is an owned copy because Array<String> frees its
 * elements when the array is dropped.
 */

static int aura_saved_argc = 0;
static char **aura_saved_argv = NULL;

void aura_set_args(int argc, char **argv)
{
  aura_saved_argc = argc > 0 ? argc : 0;
  aura_saved_argv = argv;
}

int64_t aura_args_count(void)
{
  return (int64_t)aura_saved_argc;
}

const char *aura_args_get(int64_t i)
{
  if (i < 0 || i >= (int64_t)aura_saved_argc || aura_saved_argv == NULL)
  {
    aura_throw_string("args index out of bounds");
  }
  const char *s = aura_saved_argv[i] != NULL ? aura_saved_argv[i] : "";
  size_t n = strlen(s);
  char *copy = (char *)malloc(n + 1);
  if (copy == NULL)
  {
    aura_throw_string("args allocation failed");
  }
  memcpy(copy, s, n + 1);
  return copy;
}

/* Bounded CLI companion used by examples/http-health-cli.  The public Aura
 * surface intentionally exposes this as one primitive smoke operation rather
 * than leaking opaque HTTP handles into the alpha stdlib. */
static AuraHttpHandlerResult aura_http_health_cli_handler(
    const AuraHttpRequest *request, AuraHttpResponse *response, void *user_data)
{
  (void)user_data;
  if (request == NULL || strcmp(request->method, "GET") != 0 ||
      strcmp(request->target, "/health") != 0 ||
      aura_http_response_set_body(response, "ok\n", 3) != AURA_HTTP_RESPONSE_OK)
  {
    return AURA_HTTP_HANDLER_ERROR;
  }
  return AURA_HTTP_HANDLER_CLOSE;
}

typedef struct
{
  AuraHttpConnection *connection;
} AuraHttpHealthCliTask;

static AuraTaskPollState aura_http_health_cli_poll(AuraTaskFrame *frame)
{
  AuraHttpHealthCliTask *task =
      (AuraHttpHealthCliTask *)aura_task_frame_data(frame);
  return aura_http_connection_poll_async(
      frame, task->connection, aura_http_health_cli_handler, NULL);
}

int64_t aura_http_health_smoke(void)
{
  AuraHttpConnectionConfig config;
  AuraHttpServer *server = NULL;
  AuraHttpConnection *connection = NULL;
  AuraTcpListener *listener = NULL;
  AuraTcpStream *client = NULL;
  AuraTaskExecutor *executor = NULL;
  AuraTaskFrame *frame = NULL;
  uint16_t port = 0;
  char response[512] = {0};
  size_t used = 0;
  int64_t status = 1;

  aura_http_connection_config_init(&config);
  config.max_requests = 1;
  if (aura_tcp_listener_bind(0, &port, &listener) != AURA_TCP_OK ||
      aura_http_server_create(listener, 1, &config, &server) !=
          AURA_HTTP_CONNECTION_OK ||
      aura_tcp_stream_connect(port, 1000, &client) != AURA_TCP_OK ||
      aura_http_server_accept(server, 1000, &connection) !=
          AURA_HTTP_CONNECTION_OK)
  {
    goto cleanup;
  }
  executor = aura_task_executor_new();
  if (executor == NULL)
  {
    goto cleanup;
  }
  frame = aura_task_frame_new(sizeof(AuraHttpHealthCliTask),
                               aura_http_health_cli_poll, NULL);
  if (frame == NULL)
  {
    goto cleanup;
  }
  ((AuraHttpHealthCliTask *)aura_task_frame_data(frame))->connection = connection;
  if (aura_task_executor_submit(executor, frame) != 1 ||
      aura_task_executor_run_one(executor) != 1 ||
      aura_task_frame_state(frame) != AURA_TASK_PENDING)
  {
    goto cleanup;
  }
  {
    const char request[] = "GET /health HTTP/1.1\r\nHost: localhost\r\n\r\n";
    size_t written = 0;
    if (aura_tcp_stream_write(client, request, sizeof(request) - 1,
                              &written, 1000) != AURA_TCP_OK ||
        written != sizeof(request) - 1 ||
        aura_task_executor_poll_waiting(executor, 1000) != 1 ||
        aura_task_executor_run_one(executor) != 1 ||
        aura_task_frame_state(frame) != AURA_TASK_COMPLETE)
    {
      goto cleanup;
    }
  }
  while (used + 1 < sizeof(response))
  {
    size_t received = 0;
    AuraTcpStatus read_status = aura_tcp_stream_read(
        client, response + used, sizeof(response) - used - 1, &received, 1000);
    if (read_status != AURA_TCP_OK || received == 0)
    {
      break;
    }
    used += received;
    response[used] = '\0';
    if (strstr(response, "HTTP/1.1 200 OK") != NULL &&
        strstr(response, "\r\n\r\nok\n") != NULL)
    {
      status = 0;
      break;
    }
  }

cleanup:
  if (frame != NULL)
  {
    if (aura_task_frame_state(frame) == AURA_TASK_COMPLETE ||
        aura_task_frame_state(frame) == AURA_TASK_FAILED ||
        aura_task_frame_state(frame) == AURA_TASK_CANCELLED)
    {
      (void)aura_task_executor_release(executor, &frame);
    }
  }
  if (executor != NULL)
  {
    aura_task_executor_shutdown(executor);
  }
  aura_http_connection_destroy(connection);
  aura_tcp_stream_destroy(client);
  if (server != NULL)
  {
    (void)aura_http_server_shutdown(server);
    (void)aura_http_server_destroy(server);
  }
  return status;
}

/* ---- Process exit (std.io.exit) ----
 * Flush stdio, then terminate with the given status (truncated to int).
 * Does not return. Prefer exit over _Exit so atexit/flush run.
 */

void aura_exit(int64_t code)
{
  fflush(stdout);
  fflush(stderr);
  exit((int)code);
}

/* Provided by generated code */
int aura_main(void);

#ifndef AURA_RUNTIME_NO_MAIN
int main(int argc, char **argv)
{
  aura_set_args(argc, argv);
  int rc = aura_main();
  aura_gc_shutdown();
  return rc;
}
#endif
