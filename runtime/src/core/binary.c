/* Length-aware binary values used by protocol and stream APIs. */

struct AuraByteBuffer {
  uint8_t *data;
  size_t length;
  size_t capacity;
};

static int aura_byte_buffer_grow(AuraByteBuffer *buffer, size_t needed)
{
  if (needed <= buffer->capacity) return 1;
  size_t capacity = buffer->capacity == 0 ? 16u : buffer->capacity;
  while (capacity < needed)
  {
    if (capacity > SIZE_MAX / 2u) { capacity = needed; break; }
    capacity *= 2u;
  }
  uint8_t *grown = (uint8_t *)realloc(buffer->data, capacity);
  if (grown == NULL) return 0;
  buffer->data = grown;
  buffer->capacity = capacity;
  return 1;
}

uint8_t aura_byte_from_u8(uint64_t value, int *valid)
{
  if (valid != NULL) *valid = value <= UINT8_MAX;
  return (uint8_t)value;
}

AuraByteBuffer *aura_byte_buffer_new(void)
{
  return (AuraByteBuffer *)calloc(1u, sizeof(AuraByteBuffer));
}

AuraByteBuffer *aura_byte_buffer_from_bytes(const void *data, size_t length)
{
  if (length != 0 && data == NULL) return NULL;
  AuraByteBuffer *buffer = aura_byte_buffer_new();
  if (buffer == NULL) return NULL;
  if (!aura_byte_buffer_grow(buffer, length)) {
    aura_byte_buffer_destroy(buffer);
    return NULL;
  }
  if (length != 0) memcpy(buffer->data, data, length);
  buffer->length = length;
  return buffer;
}

void aura_byte_buffer_destroy(AuraByteBuffer *buffer)
{
  if (buffer == NULL) return;
  free(buffer->data);
  free(buffer);
}

size_t aura_byte_buffer_length(const AuraByteBuffer *buffer)
{
  return buffer == NULL ? 0u : buffer->length;
}

AuraByteBufferStatus aura_byte_buffer_append_byte(AuraByteBuffer *buffer,
                                                  uint8_t value)
{
  if (buffer == NULL) return AURA_BYTE_BUFFER_INVALID;
  if (buffer->length == SIZE_MAX || !aura_byte_buffer_grow(buffer, buffer->length + 1u))
    return AURA_BYTE_BUFFER_OOM;
  buffer->data[buffer->length++] = value;
  return AURA_BYTE_BUFFER_OK;
}

AuraByteBufferStatus aura_byte_buffer_read_byte(const AuraByteBuffer *buffer,
                                                size_t index, uint8_t *out)
{
  if (buffer == NULL || out == NULL) return AURA_BYTE_BUFFER_INVALID;
  if (index >= buffer->length) return AURA_BYTE_BUFFER_EOF;
  *out = buffer->data[index];
  return AURA_BYTE_BUFFER_OK;
}

AuraByteBuffer *aura_byte_buffer_slice(const AuraByteBuffer *buffer,
                                       size_t start, size_t length)
{
  if (buffer == NULL || start > buffer->length || length > buffer->length - start)
    return NULL;
  return aura_byte_buffer_from_bytes(buffer->data + start, length);
}

AuraByteBuffer *aura_byte_buffer_concat(const AuraByteBuffer *left,
                                         const AuraByteBuffer *right)
{
  if (left == NULL || right == NULL || left->length > SIZE_MAX - right->length)
    return NULL;
  AuraByteBuffer *buffer = aura_byte_buffer_new();
  if (buffer == NULL) return NULL;
  if (!aura_byte_buffer_grow(buffer, left->length + right->length)) {
    aura_byte_buffer_destroy(buffer);
    return NULL;
  }
  if (left->length != 0) memcpy(buffer->data, left->data, left->length);
  if (right->length != 0) memcpy(buffer->data + left->length, right->data, right->length);
  buffer->length = left->length + right->length;
  return buffer;
}

const uint8_t *aura_byte_buffer_data(const AuraByteBuffer *buffer)
{
  return buffer == NULL ? NULL : buffer->data;
}

uint16_t aura_read_int16_be(const uint8_t *data)
{
  return data == NULL ? 0u : (uint16_t)(((uint16_t)data[0] << 8) | data[1]);
}

uint32_t aura_read_int32_be(const uint8_t *data)
{
  return data == NULL ? 0u : ((uint32_t)data[0] << 24) | ((uint32_t)data[1] << 16) |
      ((uint32_t)data[2] << 8) | data[3];
}

void aura_write_int16_be(uint8_t *data, uint16_t value)
{
  if (data == NULL) return;
  data[0] = (uint8_t)(value >> 8); data[1] = (uint8_t)value;
}

void aura_write_int32_be(uint8_t *data, uint32_t value)
{
  if (data == NULL) return;
  data[0] = (uint8_t)(value >> 24); data[1] = (uint8_t)(value >> 16);
  data[2] = (uint8_t)(value >> 8); data[3] = (uint8_t)value;
}
