static int aura_mime_token_byte(unsigned char c)
{
  return (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z') ||
         (c >= '0' && c <= '9') || strchr("!#$%&'*+-.^_`|~", (int)c) != NULL;
}

_Bool aura_mime_is_valid_type(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  size_t i = 0;
  if (length == 0) return false;
  size_t type_start = i;
  while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
  if (i == type_start || i >= length || value[i++] != '/') return false;
  size_t subtype_start = i;
  while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
  if (i == subtype_start) return false;
  while (i < length) {
    if (value[i++] != ';') return false;
    while (i < length && (value[i] == ' ' || value[i] == '\t')) i++;
    size_t key_start = i;
    while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
    if (i == key_start || i >= length || value[i++] != '=') return false;
    if (i >= length) return false;
    if (value[i] == '"') {
      i++;
      while (i < length && value[i] != '"') {
        if ((unsigned char)value[i] < 0x20 || value[i] == '\\') return false;
        i++;
      }
      if (i >= length || value[i++] != '"') return false;
    } else {
      size_t value_start = i;
      while (i < length && aura_mime_token_byte((unsigned char)value[i])) i++;
      if (i == value_start) return false;
    }
    while (i < length && (value[i] == ' ' || value[i] == '\t')) i++;
  }
  return true;
}

const char *aura_mime_sanitize_filename(const char *value)
{
  size_t length = value == NULL ? 0 : strlen(value);
  if (length == 0 || (length == 1 && value[0] == '.') ||
      (length == 2 && value[0] == '.' && value[1] == '.')) return NULL;
  char *out = (char *)malloc(length + 1);
  if (out == NULL) return NULL;
  size_t start = 0, out_length = 0;
  for (size_t i = 0; i <= length; i++) {
    if (i == length || value[i] == '/' || value[i] == '\\') {
      if (i > start) {
        if (out_length != 0) out[out_length++] = '_';
        for (size_t j = start; j < i; j++) {
          unsigned char c = (unsigned char)value[j];
          if (c < 0x20 || c == 0x7f) { free(out); return NULL; }
          out[out_length++] = (char)c;
        }
      }
      start = i + 1;
    }
  }
  if (out_length == 0) { free(out); return NULL; }
  out[out_length] = '\0';
  return out;
}

const char *aura_mime_disposition_filename(const char *value)
{
  if (value == NULL) return NULL;
  const char *cursor = value;
  while (*cursor != '\0') {
    while (*cursor == ';' || isspace((unsigned char)*cursor)) cursor++;
    const char *name = cursor;
    while (*cursor != '\0' && *cursor != '=' && *cursor != ';') cursor++;
    size_t name_length = (size_t)(cursor - name);
    while (name_length > 0 && isspace((unsigned char)name[name_length - 1])) name_length--;
    if (*cursor != '=') {
      while (*cursor != '\0' && *cursor != ';') cursor++;
      continue;
    }
    cursor++;
    while (isspace((unsigned char)*cursor)) cursor++;
    const char *raw = cursor;
    size_t raw_length = 0;
    if (*cursor == '"') {
      raw = ++cursor;
      while (*cursor != '\0' && *cursor != '"') cursor++;
      raw_length = (size_t)(cursor - raw);
      if (*cursor == '"') cursor++;
    } else {
      while (*cursor != '\0' && *cursor != ';') cursor++;
      raw_length = (size_t)(cursor - raw);
      while (raw_length > 0 && isspace((unsigned char)raw[raw_length - 1])) raw_length--;
    }
    if (name_length == 8) {
      static const char filename_name[] = "filename";
      int matches = 1;
      for (size_t i = 0; i < 8; i++) {
        unsigned char c = (unsigned char)name[i];
        if ((unsigned char)tolower(c) != (unsigned char)filename_name[i]) {
          matches = 0;
          break;
        }
      }
      if (matches) {
        char *raw_copy = aura_url_copy_range(raw, raw_length);
        const char *safe = aura_mime_sanitize_filename(raw_copy);
        free(raw_copy);
        return safe;
      }
    }
  }
  return NULL;
}

