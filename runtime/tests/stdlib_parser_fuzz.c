#include <assert.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

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
  const char *child = aura_json_object_get("{\"name\":\"aura\",\"items\":[1]}", "name");
  assert(child != NULL && strcmp(child, "\"aura\"") == 0);
  free_result(child);
  const char *decoded = aura_json_decode_string("\"A\\u00e9\\ud83d\\ude00\"");
  assert(decoded != NULL && strcmp(decoded, "A\xc3\xa9\xf0\x9f\x98\x80") == 0);
  free_result(decoded);
  const char *item = aura_json_array_at("[0,{\"ok\":true}]", 1);
  assert(item != NULL && strcmp(item, "{\"ok\":true}") == 0);
  free_result(item);
  assert(aura_json_array_count("[0,{\"ok\":true}]") == 2);
  const char *keys = aura_json_object_keys("{\"a\":1,\"b\":2}");
  assert(keys != NULL && strcmp(keys, "[\"a\",\"b\"]") == 0);
  free_result(keys);
  const char *duplicate = aura_json_duplicate_key("{\"x\":1,\"x\":2}");
  assert(duplicate != NULL && strcmp(duplicate, "x") == 0);
  free_result(duplicate);
  assert(aura_json_duplicate_key("{\"x\":1,\"y\":2}") == NULL);
  assert(aura_url_is_origin_form("/health"));
  assert(aura_mime_is_valid_type("text/plain"));
  return 0;
}
