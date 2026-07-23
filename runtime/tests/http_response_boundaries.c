#include <assert.h>
#include <stdio.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

static void test_owned_response_builder(void)
{
  AuraHttpResponse response;
  size_t required = 0;
  char output[256] = {0};
  const char *expected =
      "HTTP/1.1 201 Created\r\n"
      "X-Trace: boundary\r\n"
      "Content-Length: 5\r\n"
      "Connection: keep-alive\r\n\r\n"
      "hello";

  aura_http_response_init(&response);
  assert(aura_http_response_set_status(&response, 201) == AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_set_connection(
             &response, AURA_HTTP_RESPONSE_KEEP_ALIVE) == AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_add_header(&response, "X-Trace", "boundary") ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_set_body(&response, "hello", 5) ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_serialize(&response, NULL, 0, &required) ==
         AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL);
  assert(required == strlen(expected));
  assert(aura_http_response_serialize(&response, output, required - 1, &required) ==
         AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL);
  assert(aura_http_response_serialize(&response, output, sizeof(output), &required) ==
         AURA_HTTP_RESPONSE_OK);
  assert(required == strlen(expected));
  assert(memcmp(output, expected, required) == 0);
  aura_http_response_destroy(&response);
  aura_http_response_destroy(&response);
}

static void test_rejects_forbidden_and_ambiguous_fields(void)
{
  AuraHttpResponse response;
  size_t length = 0;
  char output[256];

  aura_http_response_init(&response);
  assert(aura_http_response_set_status(&response, 99) == AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_add_header(&response, "Content-Length", "1") ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_add_header(&response, "X\nBad", "value") ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_add_header(&response, "X-Test", "ok\r\n") ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_set_status(&response, 204) == AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_set_body(&response, "x", 1) ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_set_body(&response, NULL, 0) == AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_set_error(&response, 500, "bad_input") ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_serialize(&response, output, sizeof(output), &length) ==
         AURA_HTTP_RESPONSE_OK);
  assert(strstr(output, "500 Internal Server Error") != NULL);
  assert(strstr(output, "{\"error\":\"bad_input\"}") != NULL);
  aura_http_response_destroy(&response);
}

int main(void)
{
  test_owned_response_builder();
  test_rejects_forbidden_and_ambiguous_fields();
  puts("http response boundary coverage: passed");
  return 0;
}
