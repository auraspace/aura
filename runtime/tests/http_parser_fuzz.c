#include <assert.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static uint32_t next_seed(uint32_t *state)
{
  *state = (*state * UINT32_C(1664525)) + UINT32_C(1013904223);
  return *state;
}

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

int main(void)
{
  static const unsigned char *const seed_requests[] = {
      (const unsigned char *)"POST /submit HTTP/1.1\r\n"
                             "Host: example.test\r\n"
                             "Content-Length: 5\r\n"
                             "X-Trace: stable\r\n"
                             "\r\n"
                             "hello",
      (const unsigned char *)"POST /stream HTTP/1.1\r\n"
                             "Host: example.test\r\n"
                             "Transfer-Encoding: chunked\r\n"
                             "\r\n"
                             "4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n",
      (const unsigned char *)"GET /health?ready=1 HTTP/1.1\r\n"
                             "Host: example.test\r\n"
                             "Connection: keep-alive\r\n"
                             "\r\n"};
  unsigned char mutated[192];
  uint32_t state = UINT32_C(0);
  size_t iteration;

  for (iteration = 0; iteration < 4096; iteration++)
  {
    const unsigned char *seed = seed_requests[next_seed(&state) %
                                              (sizeof(seed_requests) / sizeof(seed_requests[0]))];
    size_t seed_length = strlen((const char *)seed);
    AuraHttpRequest parsed = {0};
    size_t consumed = 123;
    size_t mutations = (size_t)(next_seed(&state) % 4U) + 1U;
    size_t mutation;
    AuraHttpParseStatus status;

    assert(seed_length < sizeof(mutated));
    memcpy(mutated, seed, seed_length);
    for (mutation = 0; mutation < mutations; mutation++)
    {
      size_t index = (size_t)(next_seed(&state) % seed_length);
      mutated[index] = (unsigned char)(next_seed(&state) & UINT32_C(0xff));
    }

    status = aura_http_request_parse(mutated, seed_length, &parsed, &consumed);
    assert(status == AURA_HTTP_PARSE_OK || status == AURA_HTTP_PARSE_INCOMPLETE ||
           status == AURA_HTTP_PARSE_BAD_REQUEST ||
           status == AURA_HTTP_PARSE_METHOD_NOT_ALLOWED ||
           status == AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE);
    if (status == AURA_HTTP_PARSE_OK)
    {
      assert(consumed > 0 && consumed <= seed_length);
      aura_http_request_destroy(&parsed);
    }
    else
    {
      assert_empty_request(&parsed, consumed);
    }
  }

  return 0;
}
