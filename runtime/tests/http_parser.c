#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

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
  assert(strcmp(aura_http_request_method(&parsed), "GET") == 0);
  assert(strcmp(aura_http_request_target(&parsed), "/health?ready=1") == 0);
  assert(strcmp(aura_http_request_version(&parsed), "HTTP/1.1") == 0);
  assert(aura_http_request_header_count(&parsed) == 2);
  assert(strcmp(aura_http_request_header_name(&parsed, 0), "hOsT") == 0);
  assert(strcmp(aura_http_request_header_value(&parsed, 0), "example.test") == 0);
  assert(aura_http_request_header_name(&parsed, 99) == NULL);
  assert(aura_http_request_body(&parsed) == NULL);
  assert(aura_http_request_body_length(&parsed) == 0);
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
  assert(aura_http_request_body_length(&parsed) == 5);
  assert(memcmp(aura_http_request_body(&parsed), "hello", 5) == 0);
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

static void test_header_first_content_length_request(void)
{
  const char partial[] =
      "POST /stream HTTP/1.1\r\nContent-Length: 5\r\nX-Mode: stream\r\n\r\nhe";
  AuraHttpRequest parsed = {0};
  size_t header_end = 0;
  size_t content_length = 0;
  int chunked = 1;

  assert(aura_http_request_parse_headers(partial, sizeof(partial) - 1, &parsed,
                                         &header_end, &content_length,
                                         &chunked) == AURA_HTTP_PARSE_OK);
  assert(header_end == strlen("POST /stream HTTP/1.1\r\nContent-Length: 5\r\n"
                              "X-Mode: stream\r\n\r\n"));
  assert(content_length == 5);
  assert(chunked == 0);
  assert(strcmp(parsed.method, "POST") == 0);
  assert(parsed.body == NULL);
  assert(parsed.body_length == 0);
  assert(parsed.total_length == header_end);
  aura_http_request_destroy(&parsed);
}

static void test_header_first_chunked_metadata(void)
{
  const char partial[] =
      "POST /stream HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWi";
  AuraHttpRequest parsed = {0};
  size_t header_end = 0;
  size_t content_length = 99;
  int chunked = 0;

  assert(aura_http_request_parse_headers(partial, sizeof(partial) - 1, &parsed,
                                         &header_end, &content_length,
                                         &chunked) == AURA_HTTP_PARSE_OK);
  assert(header_end == strlen("POST /stream HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"));
  assert(content_length == 0);
  assert(chunked == 1);
  assert(parsed.body == NULL);
  aura_http_request_destroy(&parsed);
}

static void test_content_length_reader_preserves_buffered_boundary(void)
{
  unsigned char buffered[32] = "helloNEXT";
  size_t used = strlen((const char *)buffered);
  AuraTcpStream stream = {-1};
  AuraHttpContentLengthReader reader;
  unsigned char chunk[8] = {0};
  size_t read = 0;

  assert(aura_http_content_length_reader_init(&reader, &stream, buffered, &used,
                                              5, 1000));
  assert(aura_http_content_length_reader_read(&reader, chunk, 2, &read) == AURA_TCP_OK);
  assert(read == 2 && memcmp(chunk, "he", 2) == 0);
  assert(aura_http_content_length_reader_read(&reader, chunk, sizeof(chunk), &read) ==
         AURA_TCP_OK);
  assert(read == 3 && memcmp(chunk, "llo", 3) == 0);
  assert(reader.remaining == 0);
  assert(used == 4 && memcmp(buffered, "NEXT", 4) == 0);
  assert(aura_http_content_length_reader_read(&reader, chunk, sizeof(chunk), &read) ==
         AURA_TCP_EOF);
}

static void test_content_length_reader_reads_socket_without_overread(void)
{
  int fds[2] = {-1, -1};
  AuraTcpStream stream;
  AuraHttpContentLengthReader reader;
  unsigned char chunk[8] = {0};
  unsigned char next[8] = {0};
  size_t used = 0;
  size_t read = 0;

  assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);
  stream.fd = fds[0];
  assert(write(fds[1], "bodyNEXT", 8) == 8);
  assert(aura_http_content_length_reader_init(&reader, &stream, NULL, &used,
                                              4, 1000));
  assert(aura_http_content_length_reader_read(&reader, chunk, sizeof(chunk), &read) ==
         AURA_TCP_OK);
  assert(read == 4 && memcmp(chunk, "body", 4) == 0);
  assert(reader.remaining == 0);
  assert(recv(fds[0], next, sizeof(next), 0) == 4);
  assert(memcmp(next, "NEXT", 4) == 0);
  assert(close(fds[0]) == 0);
  assert(close(fds[1]) == 0);
}

static void test_content_length_reader_allows_one_active_read_task(void)
{
  AuraTcpStream stream = {0};
  AuraHttpContentLengthReader reader = {0};
  AuraHttpRequest request = {0};
  size_t used = 0;

  assert(aura_http_content_length_reader_init(&reader, &stream, NULL, &used,
                                              1, 1000));
  request.body_reader = &reader;
  assert(aura_http_request_body_read_begin(&request));
  assert(!aura_http_request_body_read_begin(&request));
  aura_http_request_body_read_end(&request);
  assert(aura_http_request_body_read_begin(&request));
  aura_http_request_body_read_end(&request);
}

static void test_chunked_reader_consumes_trailers(void)
{
  int fds[2] = {-1, -1};
  AuraTcpStream stream;
  AuraHttpContentLengthReader reader;
  unsigned char body[8] = {0};
  size_t used = 0;
  size_t read = 0;

  assert(socketpair(AF_UNIX, SOCK_STREAM, 0, fds) == 0);
  stream.fd = fds[0];
  {
    const char input[] = "4\r\nWiki\r\n0\r\nX-Trace: done\r\n\r\nNEXT";
    assert(write(fds[1], input, sizeof(input) - 1) == (ssize_t)(sizeof(input) - 1));
  }
  assert(aura_http_chunked_reader_init(&reader, &stream, NULL, &used, 1000));
  assert(aura_http_chunked_reader_read(&reader, body, sizeof(body), &read) ==
         AURA_TCP_OK);
  assert(read == 4 && memcmp(body, "Wiki", 4) == 0);
  assert(aura_http_chunked_reader_read(&reader, body, sizeof(body), &read) ==
         AURA_TCP_EOF);
  assert(recv(fds[0], body, sizeof(body), 0) == 4);
  assert(memcmp(body, "NEXT", 4) == 0);
  assert(close(fds[0]) == 0);
  assert(close(fds[1]) == 0);
}

static void test_chunked_body_and_keep_alive_boundary(void)
{
  const char partial[] =
      "POST /submit HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"
      "4\r\nWiki\r\n5\r\npedia\r\n";
  const char complete[] =
      "POST /submit HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"
      "4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n"
      "GET /next HTTP/1.1\r\n\r\n";
  AuraHttpRequest parsed = {0};
  size_t consumed = 99;

  assert(aura_http_request_parse(partial, sizeof(partial) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_INCOMPLETE);
  assert_empty_request(&parsed, consumed);
  assert(aura_http_request_parse(complete, sizeof(complete) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_OK);
  assert(consumed == strlen("POST /submit HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"
                            "4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n"));
  assert(parsed.body_length == 9);
  assert(memcmp(parsed.body, "Wikipedia", 9) == 0);
  aura_http_request_destroy(&parsed);
}

static void test_chunked_trailers_are_preserved(void)
{
  const char request[] =
      "POST /submit HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"
      "4\r\nWiki\r\n0\r\nX-Checksum: abc123\r\nX-Trace: done\r\n\r\n"
      "GET /next HTTP/1.1\r\n\r\n";
  AuraHttpRequest parsed = {0};
  size_t consumed = 0;
  const AuraHttpHeader *checksum;
  const AuraHttpHeader *trace;

  assert(aura_http_request_parse(request, sizeof(request) - 1, &parsed, &consumed) ==
         AURA_HTTP_PARSE_OK);
  assert(parsed.body_length == 4);
  assert(memcmp(parsed.body, "Wiki", 4) == 0);
  assert(parsed.header_count == 3);
  checksum = aura_http_request_find_header(&parsed, "x-checksum");
  trace = aura_http_request_find_header(&parsed, "X-TRACE");
  assert(checksum != NULL && strcmp(checksum->value, "abc123") == 0);
  assert(trace != NULL && strcmp(trace->value, "done") == 0);
  assert(consumed == strlen("POST /submit HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n"
                            "4\r\nWiki\r\n0\r\nX-Checksum: abc123\r\nX-Trace: done\r\n\r\n"));
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
      "POST / HTTP/1.1\r\nTransfer-Encoding: gzip\r\n\r\n",
      "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\nContent-Length: 1\r\n\r\n0\r\n\r\n",
      "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\nX\r\nbad\r\n0\r\n\r\n",
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
  test_header_first_content_length_request();
  test_header_first_chunked_metadata();
  test_content_length_reader_preserves_buffered_boundary();
  test_content_length_reader_reads_socket_without_overread();
  test_content_length_reader_allows_one_active_read_task();
  test_chunked_reader_consumes_trailers();
  test_chunked_body_and_keep_alive_boundary();
  test_chunked_trailers_are_preserved();
  test_malformed_and_rejected_framing();
  test_oversized_request_line_and_headers();
  test_oversized_body_and_ownership();
  return 0;
}
