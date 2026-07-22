#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static void test_deterministic_binary_keep_alive_response(void)
{
  const unsigned char body[] = {0x00, 0xff, 'A'};
  const char expected[] =
      "HTTP/1.1 200 OK\r\n"
      "X-Trace: stable\r\n"
      "Content-Length: 3\r\n"
      "Connection: keep-alive\r\n"
      "\r\n"
      "\0\xff" "A";
  AuraHttpResponse response;
  char first[256] = {0};
  char second[256] = {0};
  size_t first_length = 0;
  size_t second_length = 0;

  aura_http_response_init(&response);
  assert(aura_http_response_set_connection(&response,
                                           AURA_HTTP_RESPONSE_KEEP_ALIVE) ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_add_header(&response, "X-Trace", "stable") ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_set_body(&response, body, sizeof(body)) ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_serialize(&response, first, sizeof(first),
                                      &first_length) == AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_serialize(&response, second, sizeof(second),
                                      &second_length) == AURA_HTTP_RESPONSE_OK);
  assert(first_length == sizeof(expected) - 1);
  assert(second_length == first_length);
  assert(memcmp(first, expected, first_length) == 0);
  assert(memcmp(first, second, first_length) == 0);
  aura_http_response_destroy(&response);
  aura_http_response_destroy(&response);
}

static void test_header_validation_and_automatic_headers(void)
{
  AuraHttpResponse response;
  size_t length = 0;
  char output[256];

  aura_http_response_init(&response);
  assert(aura_http_response_add_header(&response, "Content-Length", "4") ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_add_header(&response, "connection", "close") ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_add_header(&response, "X-Bad", "ok\r\nX: bad") ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_add_header(&response, "X-One", "one") ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_add_header(&response, "x-one", "duplicate") ==
         AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_serialize(&response, NULL, 0, &length) ==
         AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL);
  assert(length > 0);
  assert(aura_http_response_serialize(&response, output, length - 1, &length) ==
         AURA_HTTP_RESPONSE_BUFFER_TOO_SMALL);
  assert(aura_http_response_serialize(&response, output, sizeof(output), &length) ==
         AURA_HTTP_RESPONSE_OK);
  assert(strstr(output, "Content-Length: 0\r\n") != NULL);
  assert(strstr(output, "Connection: close\r\n") != NULL);
  aura_http_response_destroy(&response);
}

static void test_status_body_and_size_validation(void)
{
  AuraHttpResponse response;
  unsigned char *large_body;

  aura_http_response_init(&response);
  assert(aura_http_response_set_status(&response, 204) == AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_set_body(&response, "x", 1) == AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_set_status(&response, 199) == AURA_HTTP_RESPONSE_INVALID);
  assert(aura_http_response_set_status(&response, 600) == AURA_HTTP_RESPONSE_INVALID);
  large_body = (unsigned char *)malloc(AURA_HTTP_MAX_RESPONSE_BODY_BYTES + 1);
  assert(large_body != NULL);
  assert(aura_http_response_set_body(&response, large_body,
                                     AURA_HTTP_MAX_RESPONSE_BODY_BYTES + 1) ==
         AURA_HTTP_RESPONSE_TOO_LARGE);
  free(large_body);
  aura_http_response_destroy(&response);
}

static void test_stable_error_response(void)
{
  AuraHttpResponse response;
  char output[256] = {0};
  size_t length = 0;
  const char expected[] =
      "HTTP/1.1 400 Bad Request\r\n"
      "Content-Type: application/json\r\n"
      "Content-Length: 23\r\n"
      "Connection: close\r\n"
      "\r\n"
      "{\"error\":\"bad_request\"}";

  aura_http_response_init(&response);
  assert(aura_http_response_set_error(&response, 400, "bad_request") ==
         AURA_HTTP_RESPONSE_OK);
  assert(aura_http_response_serialize(&response, output, sizeof(output), &length) ==
         AURA_HTTP_RESPONSE_OK);
  assert(length == sizeof(expected) - 1);
  assert(memcmp(output, expected, length) == 0);
  assert(aura_http_response_set_error(&response, 418, "teapot") ==
         AURA_HTTP_RESPONSE_INVALID);
  aura_http_response_destroy(&response);
}

int main(void)
{
  test_deterministic_binary_keep_alive_response();
  test_header_validation_and_automatic_headers();
  test_status_body_and_size_validation();
  test_stable_error_response();
  puts("http response tests passed");
  return 0;
}
