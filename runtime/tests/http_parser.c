#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static void assert_empty_request(const AuraHttpRequest *request, size_t consumed)
{
  assert(request->method == NULL);
  assert(request->target == NULL);
  assert(request->version == NULL);
  assert(request->headers == NULL);
  assert(request->header_count == 0);
  assert(request->body == NULL);
  assert(request->body_length == 0);
  assert(request->total_length == 0);
  assert(consumed == 0);
}

static void test_valid_get_and_case_insensitive_headers(void)
{
  const char request[] =
      "GET /health?ready=1 HTTP/1.1\r\n"
      "hOsT: example.test\r\n"
      "X-Trace:  value \t\r\n"
      "\r\n"
      "NEXT";
  AuraHttpRequest parsed = {0};
  size_t consumed = 0;
  const AuraHttpHeader *host;
  const AuraHttpHeader *trace;

  assert(aura_http_request_parse(request, sizeof(request) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_OK);
  assert(consumed == sizeof(request) - 1 - 4);
  assert(strcmp(parsed.method, "GET") == 0);
  assert(strcmp(parsed.target, "/health?ready=1") == 0);
  assert(strcmp(parsed.version, "HTTP/1.1") == 0);
  assert(parsed.header_count == 2);
  assert(parsed.body == NULL);
  assert(parsed.body_length == 0);
  host = aura_http_request_find_header(&parsed, "HOST");
  trace = aura_http_request_find_header(&parsed, "x-trace");
  assert(host != NULL && strcmp(host->value, "example.test") == 0);
  assert(trace != NULL && strcmp(trace->value, "value") == 0);
  aura_http_request_destroy(&parsed);
  aura_http_request_destroy(&parsed);
}

static void test_valid_post_duplicate_equal_content_length(void)
{
  const unsigned char request[] =
      "POST /submit HTTP/1.1\r\n"
      "Content-Length: 5\r\n"
      "cOnTeNt-LeNgTh: 0005\r\n"
      "\r\n"
      "hello"
      "NEXT";
  AuraHttpRequest parsed = {0};
  size_t consumed = 0;
  const AuraHttpHeader *length;

  assert(aura_http_request_parse(request, sizeof(request) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_OK);
  assert(consumed == sizeof(request) - 1 - 4);
  assert(parsed.header_count == 2);
  assert(parsed.body_length == 5);
  assert(memcmp(parsed.body, "hello", 5) == 0);
  length = aura_http_request_find_header(&parsed, "content-length");
  assert(length != NULL && strcmp(length->value, "5") == 0);
  aura_http_request_destroy(&parsed);
}

static void test_incomplete_body_and_trailing_request_boundary(void)
{
  const char partial[] =
      "POST /submit HTTP/1.1\r\nContent-Length: 5\r\n\r\nhe";
  const char complete[] =
      "POST /submit HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello"
      "GET /next HTTP/1.1\r\n\r\n";
  AuraHttpRequest parsed = {0};
  size_t consumed = 99;

  assert(aura_http_request_parse(partial, sizeof(partial) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_INCOMPLETE);
  assert_empty_request(&parsed, consumed);
  assert(aura_http_request_parse(complete, sizeof(complete) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_OK);
  assert(consumed == strlen("POST /submit HTTP/1.1\r\nContent-Length: 5\r\n\r\nhello"));
  aura_http_request_destroy(&parsed);
}

static void test_malformed_and_rejected_framing(void)
{
  const char *bad_requests[] = {
      "GET / HTTP/1.1\n\n",
      "GET example HTTP/1.1\r\n\r\n",
      "GET / HTTP/1.0\r\n\r\n",
      "GET / HTTP/1.1\r\nMissingColon\r\n\r\n",
      "POST / HTTP/1.1\r\nContent-Length: nope\r\n\r\n",
      "POST / HTTP/1.1\r\nContent-Length: 3\r\nContent-Length: 4\r\n\r\n",
      "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
      "POST / HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n",
      "GET / HTTP/1.1\r\nX-Bad: good\001\r\n\r\n"};
  size_t i;
  for (i = 0; i < sizeof(bad_requests) / sizeof(bad_requests[0]); i++)
  {
    AuraHttpRequest parsed = {0};
    size_t consumed = 123;
    assert(aura_http_request_parse(bad_requests[i], strlen(bad_requests[i]), &parsed,
                                   &consumed) == AURA_HTTP_PARSE_BAD_REQUEST);
    assert_empty_request(&parsed, consumed);
  }

  {
    const char unsupported[] = "OPTIONS / HTTP/1.1\r\n\r\n";
    AuraHttpRequest parsed = {0};
    size_t consumed = 123;
    assert(aura_http_request_parse(unsupported, sizeof(unsupported) - 1, &parsed,
                                   &consumed) == AURA_HTTP_PARSE_METHOD_NOT_ALLOWED);
    assert_empty_request(&parsed, consumed);
  }
}

static void test_oversized_request_line_and_headers(void)
{
  size_t line_size = AURA_HTTP_MAX_REQUEST_LINE_BYTES + 2;
  unsigned char *long_line = (unsigned char *)malloc(line_size + 2);
  size_t i;
  AuraHttpRequest parsed = {0};
  size_t consumed = 123;
  char many_headers[4096];
  size_t used = 0;

  assert(long_line != NULL);
  memcpy(long_line, "GET /", 5);
  for (i = 5; i < line_size; i++)
  {
    long_line[i] = (unsigned char)'a';
  }
  long_line[line_size] = '\r';
  long_line[line_size + 1] = '\n';
  assert(aura_http_request_parse(long_line, line_size + 2, &parsed, &consumed) ==
         AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE);
  assert_empty_request(&parsed, consumed);
  free(long_line);

  used += (size_t)snprintf(many_headers + used, sizeof(many_headers) - used,
                           "GET / HTTP/1.1\r\n");
  for (i = 0; i < AURA_HTTP_MAX_HEADERS + 1; i++)
  {
    int written = snprintf(many_headers + used, sizeof(many_headers) - used,
                           "X-%zu: 1\r\n", i);
    assert(written > 0 && (size_t)written < sizeof(many_headers) - used);
    used += (size_t)written;
  }
  assert(used + 2 < sizeof(many_headers));
  memcpy(many_headers + used, "\r\n", 2);
  used += 2;
  assert(aura_http_request_parse(many_headers, used, &parsed, &consumed) ==
         AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE);
  assert_empty_request(&parsed, consumed);

  {
    size_t value_length = AURA_HTTP_MAX_HEADER_BYTES;
    size_t capacity = value_length + 64;
    unsigned char *large_header = (unsigned char *)malloc(capacity);
    size_t position = 0;
    const char prefix[] = "GET / HTTP/1.1\r\nX: ";
    assert(large_header != NULL);
    memcpy(large_header + position, prefix, sizeof(prefix) - 1);
    position += sizeof(prefix) - 1;
    memset(large_header + position, 'b', value_length);
    position += value_length;
    memcpy(large_header + position, "\r\n\r\n", 4);
    position += 4;
    assert(aura_http_request_parse(large_header, position, &parsed, &consumed) ==
           AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE);
    assert_empty_request(&parsed, consumed);
    free(large_header);
  }
}

static void test_oversized_body_and_ownership(void)
{
  const char oversized[] =
      "POST / HTTP/1.1\r\nContent-Length: 8388609\r\n\r\n";
  unsigned char request[] =
      "POST /binary HTTP/1.1\r\nContent-Length: 3\r\nX-Test: yes\r\n\r\n\000\377A";
  unsigned char original[sizeof(request)];
  AuraHttpRequest parsed = {0};
  size_t consumed = 123;

  assert(aura_http_request_parse(oversized, sizeof(oversized) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE);
  assert_empty_request(&parsed, consumed);

  memcpy(original, request, sizeof(request));
  assert(aura_http_request_parse(request, sizeof(request) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_OK);
  assert(parsed.body_length == 3);
  assert(parsed.body[0] == 0 && parsed.body[1] == 255 && parsed.body[2] == 'A');
  assert(memcmp(request, original, sizeof(request)) == 0);
  aura_http_request_destroy(&parsed);
  aura_http_request_destroy(&parsed);
}

int main(void)
{
  test_valid_get_and_case_insensitive_headers();
  test_valid_post_duplicate_equal_content_length();
  test_incomplete_body_and_trailing_request_boundary();
  test_malformed_and_rejected_framing();
  test_oversized_request_line_and_headers();
  test_oversized_body_and_ownership();
  return 0;
}
