/* ---- Bounded HTTP connection lifecycle (H4, synchronous alpha slice) ----
 *
 * This layer deliberately stops at a transport-backed, bounded request /
 * response loop.  The callback is an application-neutral response hook; it
 * is not a router and does not suspend.  H5 owns async integration, and H6
 * owns handler/path dispatch.
 */

typedef struct AuraHttpServer AuraHttpServer;
typedef struct AuraHttpConnection AuraHttpConnection;

typedef enum
{
  AURA_HTTP_CONNECTION_OK = 0,
  AURA_HTTP_CONNECTION_CLOSED = 1,
  AURA_HTTP_CONNECTION_TIMEOUT = 2,
  AURA_HTTP_CONNECTION_DISCONNECTED = 3,
  AURA_HTTP_CONNECTION_SHUTDOWN = 4,
  AURA_HTTP_CONNECTION_LIMIT = 5,
  AURA_HTTP_CONNECTION_ERROR = -1,
  AURA_HTTP_CONNECTION_UNSUPPORTED = -2
} AuraHttpConnectionStatus;

typedef enum
{
  AURA_HTTP_HANDLER_KEEP_ALIVE = 0,
  AURA_HTTP_HANDLER_CLOSE = 1,
  AURA_HTTP_HANDLER_ERROR = -1
} AuraHttpHandlerResult;

typedef AuraTaskPollState (*AuraHttpTaskHandler)(AuraTaskFrame *frame,
                                                  const AuraHttpRequest *request,
                                                  AuraHttpResponse *response,
                                                  void *user_data);

typedef struct
{
  size_t max_requests;
  int read_timeout_ms;
  int write_timeout_ms;
  int idle_timeout_ms;
} AuraHttpConnectionConfig;

typedef AuraHttpHandlerResult (*AuraHttpHandler)(const AuraHttpRequest *request,
                                                  AuraHttpResponse *response,
                                                  void *user_data);

typedef struct
{
  const char *method;
  const char *path;
  AuraHttpHandler handler;
  void *user_data;
} AuraHttpRoute;

/* Bounded synchronous route dispatch. The request and response remain owned
 * by the connection loop; handlers may only borrow them during this call and
 * may not suspend or transfer them across an async boundary. */
AuraHttpHandlerResult aura_http_dispatch_routes(const AuraHttpRequest *request,
                                                AuraHttpResponse *response,
                                                const AuraHttpRoute *routes,
                                                size_t route_count)
{
  int path_seen = 0;
  size_t i;
  if (request == NULL || response == NULL || routes == NULL ||
      request->method == NULL || request->target == NULL)
  {
    return AURA_HTTP_HANDLER_ERROR;
  }
  for (i = 0; i < route_count; i++)
  {
    if (routes[i].path == NULL || routes[i].method == NULL ||
        routes[i].handler == NULL || strcmp(routes[i].path, request->target) != 0)
    {
      continue;
    }
    path_seen = 1;
    if (strcmp(routes[i].method, request->method) != 0)
    {
      continue;
    }
    AuraHttpHandlerResult result =
        routes[i].handler(request, response, routes[i].user_data);
    if (result == AURA_HTTP_HANDLER_ERROR)
    {
      (void)aura_http_response_set_error(response, 500, "handler_failure");
      return AURA_HTTP_HANDLER_CLOSE;
    }
    return result;
  }
  if (aura_http_response_set_error(response, path_seen ? 405 : 404,
                                   path_seen ? "method_not_allowed" :
                                               "not_found") != AURA_HTTP_RESPONSE_OK)
  {
    return AURA_HTTP_HANDLER_ERROR;
  }
  return AURA_HTTP_HANDLER_CLOSE;
}

struct AuraHttpConnection
{
  AuraTcpStream *stream;
  AuraHttpServer *server;
  AuraHttpConnectionConfig config;
  size_t requests_served;
  int closed;
  unsigned char *async_buffer;
  size_t async_used;
  size_t async_capacity;
  AuraHttpResponse async_response;
  int async_response_active;
  char *async_output;
  size_t async_output_length;
  size_t async_output_offset;
  AuraHttpHandler async_handler;
  AuraHttpTaskHandler async_task_handler;
  void *async_user_data;
  int async_active;
  int async_phase;
  int async_close_after_write;
  AuraHttpRequest async_request;
  int async_request_active;
  AuraHttpContentLengthReader async_body_reader;
  int async_body_reader_active;
  int async_handler_started;
  AuraFfiHandlePin async_handle_pin;
  int async_handle_pin_active;
  AuraTaskFrame *async_handle_frame;
};

struct AuraHttpServer
{
  AuraTcpListener *listener;
  AuraHttpConnectionConfig config;
  size_t max_connections;
  size_t active_connections;
  int stopping;
};

int aura_http_connection_stream_write(AuraHttpConnection *connection,
                                      const void *data, size_t length,
                                      size_t *out_written)
{
  if (connection == NULL || connection->closed || connection->stream == NULL)
  {
    if (out_written != NULL)
    {
      *out_written = 0;
    }
    return AURA_TCP_CLOSED;
  }
  return aura_tcp_stream_write(connection->stream, data, length, out_written, 0);
}

static AuraHttpConnectionConfig aura_http_connection_default_config(void)
{
  AuraHttpConnectionConfig config;
  config.max_requests = 100;
  config.read_timeout_ms = 30000;
  config.write_timeout_ms = 30000;
  config.idle_timeout_ms = 30000;
  return config;
}

void aura_http_connection_config_init(AuraHttpConnectionConfig *config)
{
  if (config != NULL)
  {
    *config = aura_http_connection_default_config();
  }
}

static int aura_http_timeout_valid(int timeout_ms)
{
  return timeout_ms >= 0 && timeout_ms <= 30000;
}

static int aura_http_connection_config_valid(const AuraHttpConnectionConfig *config)
{
  return config != NULL && config->max_requests > 0 && config->max_requests <= 1024 &&
         aura_http_timeout_valid(config->read_timeout_ms) &&
         aura_http_timeout_valid(config->write_timeout_ms) &&
         aura_http_timeout_valid(config->idle_timeout_ms) &&
         config->idle_timeout_ms > 0;
}

static int aura_http_min_timeout(int first, int second)
{
  return first < second ? first : second;
}

static int aura_http_connection_header_has(const AuraHttpRequest *request,
                                           const char *token)
{
  const AuraHttpHeader *header = aura_http_request_find_header(request, "Connection");
  const char *value;
  size_t token_length;
  size_t i = 0;
  if (header == NULL || token == NULL)
  {
    return 0;
  }
  value = header->value;
  token_length = strlen(token);
  while (value[i] != '\0')
  {
    size_t start;
    size_t end;
    while (value[i] == ' ' || value[i] == '\t' || value[i] == ',')
    {
      i++;
    }
    start = i;
    while (value[i] != '\0' && value[i] != ',' && value[i] != ' ' &&
           value[i] != '\t')
    {
      i++;
    }
    end = i;
    if (end - start == token_length &&
        aura_http_ascii_equal_ci((const unsigned char *)value + start,
                                  end - start, token))
    {
      return 1;
    }
  }
  return 0;
}

static int aura_http_connection_write_all(AuraHttpConnection *connection,
                                          const unsigned char *data, size_t length)
{
  size_t sent = 0;
  while (sent < length)
  {
    size_t written = 0;
    AuraTcpStatus status = aura_tcp_stream_write(
        connection->stream, data + sent, length - sent, &written,
        connection->config.write_timeout_ms);
    if (status != AURA_TCP_OK || written == 0)
    {
      return status == AURA_TCP_TIMEOUT ? AURA_HTTP_CONNECTION_TIMEOUT
                                        : AURA_HTTP_CONNECTION_DISCONNECTED;
    }
    sent += written;
  }
  return AURA_HTTP_CONNECTION_OK;
}

static AuraHttpConnectionStatus aura_http_connection_send_error(
    AuraHttpConnection *connection, int status_code, const char *error_code)
{
  AuraHttpResponse response;
  AuraHttpResponseStatus response_status;
  size_t required = 0;
  char *serialized;
  AuraHttpConnectionStatus result;

  aura_http_response_init(&response);
  response_status = aura_http_response_set_error(&response, status_code, error_code);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    aura_http_response_destroy(&response);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  response_status = aura_http_response_serialize(&response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    aura_http_response_destroy(&response);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  serialized = (char *)malloc(required);
  if (serialized == NULL || aura_http_response_serialize(&response, serialized, required,
                                                          &required) != AURA_HTTP_RESPONSE_OK)
  {
    free(serialized);
    aura_http_response_destroy(&response);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  result = (AuraHttpConnectionStatus)aura_http_connection_write_all(
      connection, (const unsigned char *)serialized, required);
  free(serialized);
  aura_http_response_destroy(&response);
  return result;
}

static AuraHttpConnectionStatus aura_http_connection_send_response(
    AuraHttpConnection *connection, AuraHttpResponse *response)
{
  AuraHttpResponseStatus response_status;
  size_t required = 0;
  char *serialized;
  AuraHttpConnectionStatus result;

  response_status = aura_http_response_serialize(response, NULL, 0, &required);
  if (response_status != AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL || required == 0)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  serialized = (char *)malloc(required);
  if (serialized == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  response_status = aura_http_response_serialize(response, serialized, required, &required);
  if (response_status != AURA_HTTP_RESPONSE_OK)
  {
    free(serialized);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  result = (AuraHttpConnectionStatus)aura_http_connection_write_all(
      connection, (const unsigned char *)serialized, required);
  free(serialized);
  return result;
}

static void aura_http_connection_release_server(AuraHttpConnection *connection)
{
  if (connection->server != NULL && connection->server->active_connections > 0)
  {
    connection->server->active_connections--;
  }
  connection->server = NULL;
}

int aura_http_connection_close(AuraHttpConnection *connection)
{
  int changed;
  if (connection == NULL)
  {
    return 0;
  }
  changed = !connection->closed;
  if (!connection->closed)
  {
    connection->closed = 1;
    (void)aura_tcp_stream_close(connection->stream);
    aura_http_connection_release_server(connection);
  }
  return changed;
}

void aura_http_connection_destroy(AuraHttpConnection *connection)
{
  if (connection == NULL)
  {
    return;
  }
  (void)aura_http_connection_close(connection);
  aura_tcp_stream_destroy(connection->stream);
  free(connection);
}

void aura_http_connection_destroy_resource(void *resource)
{
  aura_http_connection_destroy((AuraHttpConnection *)resource);
}

AuraHttpConnectionStatus aura_http_connection_run(AuraHttpConnection *connection,
                                                   AuraHttpHandler handler,
                                                   void *user_data)
{
  unsigned char *buffer;
  size_t used = 0;
  AuraHttpConnectionStatus result = AURA_HTTP_CONNECTION_OK;

  if (connection == NULL || handler == NULL || connection->stream == NULL ||
      connection->closed)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  buffer = (unsigned char *)malloc(AURA_HTTP_MAX_TOTAL_BYTES);
  if (buffer == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  for (;;)
  {
    AuraHttpRequest request;
    size_t consumed = 0;
    AuraHttpParseStatus parse_status;
    while (1)
    {
      parse_status = aura_http_request_parse(buffer, used, &request, &consumed);
      if (parse_status != AURA_HTTP_PARSE_INCOMPLETE)
      {
        break;
      }
      if (used == AURA_HTTP_MAX_TOTAL_BYTES)
      {
        parse_status = AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE;
        break;
      }
      {
        size_t read = 0;
        int timeout = aura_http_min_timeout(connection->config.read_timeout_ms,
                                            connection->config.idle_timeout_ms);
        AuraTcpStatus read_status = aura_tcp_stream_read(connection->stream,
                                                          buffer + used,
                                                          AURA_HTTP_MAX_TOTAL_BYTES - used,
                                                          &read, timeout);
        if (read_status == AURA_TCP_TIMEOUT)
        {
          result = AURA_HTTP_CONNECTION_TIMEOUT;
          goto done;
        }
        if (read_status == AURA_TCP_EOF)
        {
          result = AURA_HTTP_CONNECTION_DISCONNECTED;
          goto done;
        }
        if (read_status != AURA_TCP_OK || read == 0)
        {
          result = AURA_HTTP_CONNECTION_DISCONNECTED;
          goto done;
        }
        used += read;
      }
    }

    if (parse_status == AURA_HTTP_PARSE_OK)
    {
      AuraHttpResponse response;
      AuraHttpHandlerResult handler_result;
      int request_close = aura_http_connection_header_has(&request, "close");
      int response_close;
      if (consumed == 0 || consumed > used)
      {
        aura_http_request_destroy(&request);
        result = AURA_HTTP_CONNECTION_ERROR;
        goto done;
      }
      memmove(buffer, buffer + consumed, used - consumed);
      used -= consumed;
      aura_http_response_init(&response);
      if (!request_close && aura_http_response_set_connection(
              &response, AURA_HTTP_RESPONSE_KEEP_ALIVE) != AURA_HTTP_RESPONSE_OK)
      {
        aura_http_request_destroy(&request);
        aura_http_response_destroy(&response);
        result = AURA_HTTP_CONNECTION_ERROR;
        goto done;
      }
      handler_result = handler(&request, &response, user_data);
      if (handler_result == AURA_HTTP_HANDLER_ERROR)
      {
        aura_http_response_destroy(&response);
        aura_http_response_init(&response);
        if (aura_http_response_set_error(&response, 500, "handler_failure") !=
            AURA_HTTP_RESPONSE_OK)
        {
          aura_http_request_destroy(&request);
          aura_http_response_destroy(&response);
          result = AURA_HTTP_CONNECTION_ERROR;
          goto done;
        }
      }
      if (handler_result == AURA_HTTP_HANDLER_CLOSE || request_close)
      {
        (void)aura_http_response_set_connection(&response, AURA_HTTP_RESPONSE_CLOSE);
      }
      if (connection->requests_served + 1 >= connection->config.max_requests)
      {
        (void)aura_http_response_set_connection(&response, AURA_HTTP_RESPONSE_CLOSE);
      }
      response_close = response.connection == AURA_HTTP_RESPONSE_CLOSE;
      result = aura_http_connection_send_response(connection, &response);
      aura_http_response_destroy(&response);
      aura_http_request_destroy(&request);
      if (result != AURA_HTTP_CONNECTION_OK)
      {
        goto done;
      }
      connection->requests_served++;
      if (response_close)
      {
        result = AURA_HTTP_CONNECTION_OK;
        goto done;
      }
      continue;
    }

    if (parse_status == AURA_HTTP_PARSE_INCOMPLETE)
    {
      result = AURA_HTTP_CONNECTION_ERROR;
      goto done;
    }
    {
      int status_code;
      const char *error_code;
      switch (parse_status)
      {
      case AURA_HTTP_PARSE_METHOD_NOT_ALLOWED:
        status_code = 405;
        error_code = "method_not_allowed";
        break;
      case AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE:
        status_code = 413;
        error_code = "payload_too_large";
        break;
      case AURA_HTTP_PARSE_BAD_REQUEST:
        status_code = 400;
        error_code = "bad_request";
        break;
      default:
        status_code = 500;
        error_code = "request_parse_failure";
        break;
      }
      (void)aura_http_connection_send_error(connection, status_code, error_code);
      result = AURA_HTTP_CONNECTION_OK;
      goto done;
    }
  }

done:
  free(buffer);
  (void)aura_http_connection_close(connection);
  return result;
}

AuraHttpConnectionStatus aura_http_server_create(
    AuraTcpListener *listener, size_t max_connections,
    const AuraHttpConnectionConfig *config, AuraHttpServer **out_server)
{
  AuraHttpServer *server;
  AuraHttpConnectionConfig defaults;
  if (out_server == NULL || listener == NULL || max_connections == 0 ||
      max_connections > 1024)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  defaults = aura_http_connection_default_config();
  if (config == NULL)
  {
    config = &defaults;
  }
  if (!aura_http_connection_config_valid(config))
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  server = (AuraHttpServer *)calloc(1, sizeof(*server));
  if (server == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  server->listener = listener;
  server->config = *config;
  server->max_connections = max_connections;
  *out_server = server;
  return AURA_HTTP_CONNECTION_OK;
}

AuraHttpConnectionStatus aura_http_server_accept(AuraHttpServer *server,
                                                  int timeout_ms,
                                                  AuraHttpConnection **out_connection)
{
  AuraTcpStream *stream = NULL;
  AuraHttpConnection *connection;
  AuraTcpStatus status;
  if (out_connection == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  *out_connection = NULL;
  if (server == NULL || server->stopping)
  {
    return AURA_HTTP_CONNECTION_SHUTDOWN;
  }
  if (server->active_connections >= server->max_connections)
  {
    return AURA_HTTP_CONNECTION_LIMIT;
  }
  status = aura_tcp_listener_accept(server->listener, timeout_ms, &stream);
  if (status == AURA_TCP_TIMEOUT || status == AURA_TCP_PENDING)
  {
    return AURA_HTTP_CONNECTION_TIMEOUT;
  }
  if (status == AURA_TCP_UNSUPPORTED)
  {
    return AURA_HTTP_CONNECTION_UNSUPPORTED;
  }
  if (status != AURA_TCP_OK || stream == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection = (AuraHttpConnection *)calloc(1, sizeof(*connection));
  if (connection == NULL)
  {
    aura_tcp_stream_destroy(stream);
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection->stream = stream;
  connection->server = server;
  connection->config = server->config;
  server->active_connections++;
  *out_connection = connection;
  return AURA_HTTP_CONNECTION_OK;
}

/* Wrap an accepted stream after its std.net handle transfers ownership. */
AuraHttpConnectionStatus aura_http_connection_create_from_stream(
    AuraTcpStream *stream, const AuraHttpConnectionConfig *config,
    AuraHttpConnection **out_connection)
{
  AuraHttpConnection *connection;
  AuraHttpConnectionConfig defaults;
  if (out_connection == NULL || stream == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  *out_connection = NULL;
  defaults = aura_http_connection_default_config();
  if (config == NULL)
  {
    config = &defaults;
  }
  if (!aura_http_connection_config_valid(config))
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection = (AuraHttpConnection *)calloc(1, sizeof(*connection));
  if (connection == NULL)
  {
    return AURA_HTTP_CONNECTION_ERROR;
  }
  connection->stream = stream;
  connection->config = *config;
  *out_connection = connection;
  return AURA_HTTP_CONNECTION_OK;
}

int aura_http_server_shutdown(AuraHttpServer *server)
{
  if (server == NULL || server->stopping)
  {
    return 0;
  }
  server->stopping = 1;
  (void)aura_tcp_listener_close(server->listener);
  return 1;
}

size_t aura_http_server_active_connections(const AuraHttpServer *server)
{
  return server == NULL ? 0 : server->active_connections;
}

int aura_http_server_destroy(AuraHttpServer *server)
{
  if (server == NULL || server->active_connections != 0)
  {
    return 0;
  }
  aura_tcp_listener_destroy(server->listener);
  free(server);
  return 1;
}

