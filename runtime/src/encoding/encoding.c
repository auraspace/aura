static int aura_encoding_hex_value(unsigned char c)
{
  if (c >= '0' && c <= '9') return (int)(c - '0');
  if (c >= 'a' && c <= 'f') return (int)(c - 'a') + 10;
  if (c >= 'A' && c <= 'F') return (int)(c - 'A') + 10;
  return -1;
}

const char *aura_encoding_hex_encode(const char *value)
{
  static const char digits[] = "0123456789abcdef";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 1) / 2) return NULL;
  char *out = (char *)malloc(length * 2 + 1);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i++) {
    unsigned char byte = (unsigned char)value[i];
    out[i * 2] = digits[byte >> 4];
    out[i * 2 + 1] = digits[byte & 0x0f];
  }
  out[length * 2] = '\0';
  return out;
}

const char *aura_encoding_hex_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if ((length & 1u) != 0) return NULL;
  char *out = (char *)malloc(length / 2 + 1);
  if (out == NULL) return NULL;
  for (size_t i = 0; i < length; i += 2) {
    int hi = aura_encoding_hex_value((unsigned char)value[i]);
    int lo = aura_encoding_hex_value((unsigned char)value[i + 1]);
    if (hi < 0 || lo < 0 || (hi == 0 && lo == 0)) { free(out); return NULL; }
    out[i / 2] = (char)((hi << 4) | lo);
  }
  out[length / 2] = '\0';
  return out;
}

static int aura_encoding_base64_value(unsigned char c)
{
  if (c >= 'A' && c <= 'Z') return (int)(c - 'A');
  if (c >= 'a' && c <= 'z') return (int)(c - 'a') + 26;
  if (c >= '0' && c <= '9') return (int)(c - '0') + 52;
  if (c == '+') return 62;
  if (c == '/') return 63;
  return -1;
}

const char *aura_encoding_base64_encode(const char *value)
{
  static const char digits[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 4) / 4 * 3) return NULL;
  size_t output_length = ((length + 2) / 3) * 4;
  char *out = (char *)malloc(output_length + 1);
  if (out == NULL) return NULL;
  size_t i = 0, o = 0;
  while (i < length) {
    size_t remaining = length - i;
    unsigned int a = (unsigned char)value[i++];
    unsigned int b = remaining > 1 ? (unsigned char)value[i++] : 0;
    unsigned int c = remaining > 2 ? (unsigned char)value[i++] : 0;
    out[o++] = digits[a >> 2];
    out[o++] = digits[((a & 3u) << 4) | (b >> 4)];
    out[o++] = remaining > 1 ? digits[((b & 15u) << 2) | (c >> 6)] : '=';
    out[o++] = remaining > 2 ? digits[c & 63u] : '=';
  }
  out[o] = '\0';
  return out;
}

const char *aura_encoding_base64_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if (length == 0) {
    char *empty = (char *)malloc(1);
    if (empty != NULL) empty[0] = '\0';
    return empty;
  }
  if ((length & 3u) != 0) return NULL;
  size_t output_length = (length / 4) * 3;
  if (value[length - 1] == '=') output_length--;
  if (value[length - 2] == '=') output_length--;
  char *out = (char *)malloc(output_length + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i += 4) {
    int a = aura_encoding_base64_value((unsigned char)value[i]);
    int b = aura_encoding_base64_value((unsigned char)value[i + 1]);
    int c = value[i + 2] == '=' ? 0 : aura_encoding_base64_value((unsigned char)value[i + 2]);
    int d = value[i + 3] == '=' ? 0 : aura_encoding_base64_value((unsigned char)value[i + 3]);
    int last = i + 4 == length;
    if (a < 0 || b < 0 || c < 0 || d < 0 ||
        (!last && (value[i + 2] == '=' || value[i + 3] == '=')) ||
        (value[i + 2] == '=' && value[i + 3] != '=') ||
        (value[i + 2] == '=' && (b & 15) != 0) ||
        (value[i + 3] == '=' && value[i + 2] != '=' && (c & 3) != 0)) {
      free(out); return NULL;
    }
    unsigned int triple = ((unsigned int)a << 18) | ((unsigned int)b << 12) |
                          ((unsigned int)c << 6) | (unsigned int)d;
    if (o < output_length) out[o++] = (char)(triple >> 16);
    if (o < output_length) out[o++] = (char)(triple >> 8);
    if (o < output_length) out[o++] = (char)triple;
  }
  for (size_t i = 0; i < output_length; i++) if (out[i] == '\0') { free(out); return NULL; }
  out[output_length] = '\0';
  return out;
}

static int aura_encoding_percent_unreserved(unsigned char c)
{
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
         (c >= '0' && c <= '9') || c == '-' || c == '.' || c == '_' || c == '~';
}

const char *aura_encoding_percent_encode(const char *value)
{
  static const char digits[] = "0123456789ABCDEF";
  size_t length = value == NULL ? 0 : strlen(value);
  if (length > (SIZE_MAX - 1) / 3) return NULL;
  char *out = (char *)malloc(length * 3 + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)value[i];
    if (aura_encoding_percent_unreserved(c)) out[o++] = (char)c;
    else { out[o++] = '%'; out[o++] = digits[c >> 4]; out[o++] = digits[c & 15]; }
  }
  out[o] = '\0';
  return out;
}

const char *aura_encoding_percent_decode(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  size_t o = 0;
  for (size_t i = 0; i < length; i++) {
    unsigned char c = (unsigned char)value[i];
    if (c == '%') {
      if (i + 2 >= length) { free(out); return NULL; }
      int hi = aura_encoding_hex_value((unsigned char)value[++i]);
      int lo = aura_encoding_hex_value((unsigned char)value[++i]);
      if (hi < 0 || lo < 0 || (hi == 0 && lo == 0)) { free(out); return NULL; }
      out[o++] = (char)((hi << 4) | lo);
    } else out[o++] = (char)c;
  }
  out[o] = '\0';
  return out;
}

_Bool aura_encoding_is_valid_utf8(const char *value)
{
  const unsigned char *bytes = (const unsigned char *)(value == NULL ? "" : value);
  size_t length = strlen((const char *)bytes);
  size_t i = 0;
  while (i < length) {
    unsigned char lead = bytes[i++];
    uint32_t codepoint;
    size_t continuation;
    if (lead <= 0x7f) continue;
    if (lead >= 0xc2 && lead <= 0xdf) { codepoint = lead & 0x1f; continuation = 1; }
    else if (lead >= 0xe0 && lead <= 0xef) { codepoint = lead & 0x0f; continuation = 2; }
    else if (lead >= 0xf0 && lead <= 0xf4) { codepoint = lead & 0x07; continuation = 3; }
    else return false;
    if (i + continuation > length) return false;
    for (size_t j = 0; j < continuation; j++) {
      unsigned char tail = bytes[i++];
      if ((tail & 0xc0) != 0x80) return false;
      codepoint = (codepoint << 6) | (tail & 0x3f);
    }
    if ((continuation == 2 && codepoint < 0x800) ||
        (continuation == 3 && codepoint < 0x10000) ||
        codepoint > 0x10ffff || (codepoint >= 0xd800 && codepoint <= 0xdfff)) return false;
  }
  return true;
}
