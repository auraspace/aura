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
  static const unsigned char seed_request[] =
      "POST /submit HTTP/1.1\r\n"
      "Host: example.test\r\n"
      "Content-Length: 5\r\n"
      "X-Trace: stable\r\n"
      "\r\n"
      "hello";
  unsigned char mutated[sizeof(seed_request) - 1];
  uint32_t state = UINT32_C(0);
  size_t iteration;

  for (iteration = 0; iteration < 4096; iteration++)
  {
    AuraHttpRequest parsed = {0};
    size_t consumed = 123;
    size_t mutations = (size_t)(next_seed(&state) % 4U) + 1U;
    size_t mutation;
    AuraHttpParseStatus status;

    memcpy(mutated, seed_request, sizeof(mutated));
    for (mutation = 0; mutation < mutations; mutation++)
    {
      size_t index = (size_t)(next_seed(&state) % sizeof(mutated));
      mutated[index] = (unsigned char)(next_seed(&state) & UINT32_C(0xff));
    }

    status = aura_http_request_parse(mutated, sizeof(mutated), &parsed, &consumed);
    assert(status == AURA_HTTP_PARSE_OK || status == AURA_HTTP_PARSE_INCOMPLETE ||
           status == AURA_HTTP_PARSE_BAD_REQUEST ||
           status == AURA_HTTP_PARSE_METHOD_NOT_ALLOWED ||
           status == AURA_HTTP_PARSE_PAYLOAD_TOO_LARGE);
    if (status == AURA_HTTP_PARSE_OK)
    {
      assert(consumed > 0 && consumed <= sizeof(mutated));
      aura_http_request_destroy(&parsed);
    }
    else
    {
      assert_empty_request(&parsed, consumed);
    }
  }

  return 0;
}
