/* Bounded WebSocket client framing over the existing nonblocking TCP runtime. */
#if defined(__unix__) || defined(__APPLE__)
typedef struct {
  char *endpoint;
  AuraTcpStream *stream;
  int active;
} AuraWsEntry;

static AuraWsEntry aura_ws_entries[32];

static AuraWsEntry *aura_ws_find(const char *endpoint) {
  for (size_t i = 0; i < sizeof(aura_ws_entries) / sizeof(aura_ws_entries[0]); i++)
    if (aura_ws_entries[i].active && strcmp(aura_ws_entries[i].endpoint, endpoint) == 0) return &aura_ws_entries[i];
  return NULL;
}

static int aura_ws_read_exact(AuraTcpStream *stream, void *buffer, size_t length) {
  size_t offset = 0;
  while (offset < length) {
    size_t count = 0;
    AuraTcpStatus status = aura_tcp_stream_read(stream, (char *)buffer + offset, length - offset, &count, 1000);
    if (status != AURA_TCP_OK || count == 0) return 0;
    offset += count;
  }
  return 1;
}

static int aura_ws_write_all(AuraTcpStream *stream, const void *buffer, size_t length) {
  size_t offset = 0;
  while (offset < length) {
    size_t count = 0;
    AuraTcpStatus status = aura_tcp_stream_write(stream, (const char *)buffer + offset, length - offset, &count, 1000);
    if (status != AURA_TCP_OK || count == 0) return 0;
    offset += count;
  }
  return 1;
}

int aura_ws_connect(const char *endpoint) {
  if (endpoint == NULL || aura_ws_find(endpoint) != NULL) return 0;
  AuraWsEntry *slot = NULL;
  for (size_t i = 0; i < sizeof(aura_ws_entries) / sizeof(aura_ws_entries[0]); i++) if (!aura_ws_entries[i].active) { slot = &aura_ws_entries[i]; break; }
  if (slot == NULL) return 0;
  const char *prefix = strncmp(endpoint, "ws://", 5) == 0 ? endpoint + 5 : endpoint;
  AuraTcpStream *stream = NULL;
  if (aura_tcp_stream_connect_endpoint(prefix, 1000, &stream) != AURA_TCP_OK || stream == NULL) return 0;
  char request[1024];
  const char *host = strchr(prefix, '/') == NULL ? prefix : prefix;
  int length = snprintf(request, sizeof(request), "GET / HTTP/1.1\r\nHost: %s\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: YXVyYS13cy1rZXk=\r\nSec-WebSocket-Version: 13\r\n\r\n", host);
  if (length <= 0 || (size_t)length >= sizeof(request) || !aura_ws_write_all(stream, request, (size_t)length)) { aura_tcp_stream_destroy(stream); return 0; }
  char response[4096]; size_t used = 0;
  while (used + 1 < sizeof(response)) {
    size_t count = 0;
    AuraTcpStatus status = aura_tcp_stream_read(stream, response + used, sizeof(response) - used - 1, &count, 1000);
    if (status != AURA_TCP_OK || count == 0) { aura_tcp_stream_destroy(stream); return 0; }
    used += count; response[used] = '\0';
    if (strstr(response, "\r\n\r\n") != NULL) break;
  }
  if (strncmp(response, "HTTP/1.1 101", 12) != 0 || strstr(response, "Upgrade: websocket") == NULL) { aura_tcp_stream_destroy(stream); return 0; }
  slot->endpoint = strdup(endpoint); slot->stream = stream; slot->active = slot->endpoint != NULL;
  if (!slot->active) aura_tcp_stream_destroy(stream);
  return slot->active;
}

int64_t aura_ws_send(const char *endpoint, int64_t kind, const char *payload) {
  AuraWsEntry *entry = aura_ws_find(endpoint); if (entry == NULL || payload == NULL) return -1;
  size_t length = strlen(payload); if (length > 65535) return -1;
  unsigned char frame[4 + 65535]; size_t header = length < 126 ? 2 : 4;
  frame[0] = (unsigned char)(kind == 0 ? 0x81 : kind == 1 ? 0x82 : kind == 2 ? 0x89 : kind == 3 ? 0x8a : 0x88);
  frame[1] = (unsigned char)(0x80 | (length < 126 ? length : 126));
  if (length >= 126) { frame[2] = (unsigned char)(length >> 8); frame[3] = (unsigned char)length; }
  uint32_t mask = (uint32_t)time(NULL) ^ (uint32_t)(uintptr_t)entry;
  unsigned char *key = frame + header; memcpy(key, &mask, 4);
  for (size_t i = 0; i < length; i++) frame[header + 4 + i] = (unsigned char)payload[i] ^ key[i % 4];
  if (!aura_ws_write_all(entry->stream, frame, header + 4 + length)) return -1;
  return (int64_t)length;
}

const char *aura_ws_receive(const char *endpoint, int64_t *kind) {
  AuraWsEntry *entry = aura_ws_find(endpoint); if (entry == NULL) return NULL;
  unsigned char header[2]; if (!aura_ws_read_exact(entry->stream, header, 2)) return NULL;
  size_t length = header[1] & 0x7f; int masked = (header[1] & 0x80) != 0;
  if (length == 126) { unsigned char ext[2]; if (!aura_ws_read_exact(entry->stream, ext, 2)) return NULL; length = ((size_t)ext[0] << 8) | ext[1]; }
  if (length > 65535) return NULL;
  unsigned char mask[4]; if (masked && !aura_ws_read_exact(entry->stream, mask, 4)) return NULL;
  char *payload = (char *)malloc(length + 1); if (payload == NULL || !aura_ws_read_exact(entry->stream, payload, length)) { free(payload); return NULL; }
  if (masked) for (size_t i = 0; i < length; i++) payload[i] ^= mask[i % 4];
  payload[length] = '\0'; *kind = header[0] & 0x0f; return payload;
}

int aura_ws_close(const char *endpoint) {
  AuraWsEntry *entry = aura_ws_find(endpoint); if (entry == NULL) return 1;
  (void)aura_tcp_stream_close(entry->stream); aura_tcp_stream_destroy(entry->stream); free(entry->endpoint); memset(entry, 0, sizeof(*entry)); return 1;
}
#else
int aura_ws_connect(const char *endpoint) { (void)endpoint; return 0; }
int64_t aura_ws_send(const char *endpoint, int64_t kind, const char *payload) { (void)endpoint; (void)kind; (void)payload; return -1; }
const char *aura_ws_receive(const char *endpoint, int64_t *kind) { (void)endpoint; (void)kind; return NULL; }
int aura_ws_close(const char *endpoint) { (void)endpoint; return 0; }
#endif
