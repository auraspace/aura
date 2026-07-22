#include <assert.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static AuraHttpHandlerResult health_handler(const AuraHttpRequest *request,
                                            AuraHttpResponse *response,
                                            void *user_data)
{
  int *calls = (int *)user_data;
  assert(strcmp(request->method, "GET") == 0);
  (*calls)++;
  assert(aura_http_response_set_body(response, "ok", 2) == AURA_HTTP_RESPONSE_OK);
  return AURA_HTTP_HANDLER_CLOSE;
}

static AuraHttpHandlerResult error_handler(const AuraHttpRequest *request,
                                           AuraHttpResponse *response,
                                           void *user_data)
{
  (void)request;
  (void)response;
  (void)user_data;
  return AURA_HTTP_HANDLER_ERROR;
}

static AuraHttpRequest request(const char *method, const char *target)
{
  AuraHttpRequest value = {0};
  value.method = (char *)method;
  value.target = (char *)target;
  return value;
}

static void assert_error(AuraHttpResponse *response, int status, const char *body)
{
  assert(response->status_code == status);
  assert(response->connection == AURA_HTTP_RESPONSE_CLOSE);
  assert(response->body != NULL);
  assert(response->body_length == strlen(body));
  assert(memcmp(response->body, body, response->body_length) == 0);
}

int main(void)
{
  int calls = 0;
  const AuraHttpRoute routes[] = {
      {"GET", "/health", health_handler, &calls},
      {"GET", "/error", error_handler, NULL},
  };
  AuraHttpResponse response;

  aura_http_response_init(&response);
  AuraHttpRequest health = request("GET", "/health");
  assert(aura_http_dispatch_routes(&health, &response, routes, 2) ==
         AURA_HTTP_HANDLER_CLOSE);
  assert(calls == 1 && response.status_code == 200);
  assert(response.body_length == 2 && memcmp(response.body, "ok", 2) == 0);
  aura_http_response_destroy(&response);

  aura_http_response_init(&response);
  AuraHttpRequest wrong_method = request("POST", "/health");
  assert(aura_http_dispatch_routes(&wrong_method, &response, routes, 2) ==
         AURA_HTTP_HANDLER_CLOSE);
  assert_error(&response, 405, "{\"error\":\"method_not_allowed\"}");
  aura_http_response_destroy(&response);

  aura_http_response_init(&response);
  AuraHttpRequest missing = request("GET", "/missing");
  assert(aura_http_dispatch_routes(&missing, &response, routes, 2) ==
         AURA_HTTP_HANDLER_CLOSE);
  assert_error(&response, 404, "{\"error\":\"not_found\"}");
  aura_http_response_destroy(&response);

  aura_http_response_init(&response);
  AuraHttpRequest failed = request("GET", "/error");
  assert(aura_http_dispatch_routes(&failed, &response, routes, 2) ==
         AURA_HTTP_HANDLER_CLOSE);
  assert_error(&response, 500, "{\"error\":\"handler_failure\"}");
  aura_http_response_destroy(&response);

  return 0;
}
