#include <assert.h>
#include <string.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

int main(void)
{
  int valid = 0;
  assert(aura_byte_from_u8(0, &valid) == 0 && valid);
  (void)aura_byte_from_u8(256, &valid);
  assert(!valid);

  AuraByteBuffer *buffer = aura_byte_buffer_new();
  assert(buffer != NULL);
  assert(aura_byte_buffer_append_byte(buffer, 0x00) == AURA_BYTE_BUFFER_OK);
  assert(aura_byte_buffer_append_byte(buffer, 0xff) == AURA_BYTE_BUFFER_OK);
  assert(aura_byte_buffer_length(buffer) == 2);
  uint8_t value = 0;
  assert(aura_byte_buffer_read_byte(buffer, 0, &value) == AURA_BYTE_BUFFER_OK && value == 0x00);
  assert(aura_byte_buffer_read_byte(buffer, 2, &value) == AURA_BYTE_BUFFER_EOF);

  AuraByteBuffer *slice = aura_byte_buffer_slice(buffer, 1, 1);
  assert(slice != NULL && aura_byte_buffer_data(slice)[0] == 0xff);
  AuraByteBuffer *joined = aura_byte_buffer_concat(buffer, slice);
  assert(joined != NULL && aura_byte_buffer_length(joined) == 3);
  assert(memcmp(aura_byte_buffer_data(joined), "\0\xff\xff", 3) == 0);

  uint8_t encoded[4];
  aura_write_int16_be(encoded, 0x1234);
  aura_write_int32_be(encoded, 0x89abcdefu);
  assert(aura_read_int16_be(encoded) == 0x89abu);
  assert(aura_read_int32_be(encoded) == 0x89abcdefu);
  aura_byte_buffer_destroy(joined);
  aura_byte_buffer_destroy(slice);
  aura_byte_buffer_destroy(buffer);
  return 0;
}
