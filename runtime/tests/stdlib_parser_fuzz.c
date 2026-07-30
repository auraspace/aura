#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../aura_rt.c"

static uint32_t next_seed(uint32_t *state)
{
  *state = (*state * UINT32_C(1664525)) + UINT32_C(1013904223);
  return *state;
}

static void free_result(const char *value)
{
  free((void *)value);
}

int main(void)
{
  static const char *const seeds[] = {
      "{}", "[true,false,null]", "\"escape\\n\"", "1e+9", "{bad}",
      "/health?x=1", "http://example.test:8080/a?x=1", "text/plain; charset=utf-8",
      "filename=\"../../upload.txt\"", "%zz", "a b/c", "\xff"};
  char input[128];
  uint32_t state = UINT32_C(0x51eed);
  size_t i;

  for (i = 0; i < 4096; i++)
  {
    const char *seed = seeds[next_seed(&state) % (sizeof(seeds) / sizeof(seeds[0]))];
    size_t length = strlen(seed);
    size_t mutations = (size_t)(next_seed(&state) % 5U);
    size_t mutation;
    memcpy(input, seed, length + 1);
    for (mutation = 0; mutation < mutations && length > 0; mutation++)
    {
      size_t index = (size_t)(next_seed(&state) % length);
      input[index] = (char)(next_seed(&state) & UINT32_C(0x7f));
    }
    (void)aura_json_is_valid(input);
    free_result(aura_json_escape_string(input));
    free_result(aura_encoding_percent_encode(input));
    free_result(aura_encoding_percent_decode(input));
    (void)aura_url_is_origin_form(input);
    free_result(aura_url_path(input));
    free_result(aura_url_query(input));
    (void)aura_url_is_absolute(input);
    free_result(aura_url_authority(input));
    free_result(aura_url_authority_host(input));
    free_result(aura_url_authority_port(input));
    free_result(aura_url_query_value(input, "x"));
    (void)aura_mime_is_valid_type(input);
    free_result(aura_mime_sanitize_filename(input));
    free_result(aura_mime_disposition_filename(input));
  }
  assert(aura_json_is_valid("{\"ok\":true}"));
  assert(aura_url_is_origin_form("/health"));
  assert(aura_mime_is_valid_type("text/plain"));
  return 0;
}
