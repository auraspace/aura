/* Bounded POSIX UDP transport used by std.udp.  Socket ownership is kept in
 * the runtime table because the locked Aura Socket shape carries its endpoint
 * but not a public foreign handle. */

#if defined(__unix__) || defined(__APPLE__)
#include <arpa/inet.h>
#include <netdb.h>
#include <netinet/in.h>
#include <poll.h>
#include <sys/socket.h>

typedef enum {
  AURA_UDP_OK = 0,
  AURA_UDP_PENDING = 1,
  AURA_UDP_ERROR = -1,
  AURA_UDP_CLOSED = -2
} AuraUdpStatus;

typedef struct {
  int fd;
  char host[256];
  uint16_t port;
  int active;
} AuraUdpEntry;

static AuraUdpEntry aura_udp_entries[64];
static char aura_udp_errbuf[256] = "no error";

static void aura_udp_error(const char *operation) {
  snprintf(aura_udp_errbuf, sizeof(aura_udp_errbuf), "udp %s failed: %s",
           operation, strerror(errno));
}

static int aura_udp_port(int64_t value, int allow_zero) {
  return value >= 0 && value <= UINT16_MAX && (allow_zero || value != 0);
}

static AuraUdpEntry *aura_udp_find(const char *host, uint16_t port) {
  for (size_t i = 0; i < sizeof(aura_udp_entries) / sizeof(aura_udp_entries[0]); i++) {
    if (aura_udp_entries[i].active && aura_udp_entries[i].port == port &&
        strcmp(aura_udp_entries[i].host, host) == 0) return &aura_udp_entries[i];
  }
  return NULL;
}

static AuraUdpEntry *aura_udp_slot(void) {
  for (size_t i = 0; i < sizeof(aura_udp_entries) / sizeof(aura_udp_entries[0]); i++) {
    if (!aura_udp_entries[i].active) return &aura_udp_entries[i];
  }
  return NULL;
}

static int aura_udp_resolve(const char *host, uint16_t port, int passive, int *out_fd) {
  struct addrinfo hints;
  memset(&hints, 0, sizeof(hints));
  hints.ai_family = AF_UNSPEC;
  hints.ai_socktype = SOCK_DGRAM;
  hints.ai_flags = passive ? AI_PASSIVE : 0;
  char service[16];
  snprintf(service, sizeof(service), "%u", (unsigned)port);
  struct addrinfo *results = NULL;
  int rc = getaddrinfo((host == NULL || host[0] == '\0') ? NULL : host,
                       service, &hints, &results);
  if (rc != 0) {
    snprintf(aura_udp_errbuf, sizeof(aura_udp_errbuf), "udp resolve failed: %s",
             gai_strerror(rc));
    return 0;
  }
  for (struct addrinfo *it = results; it != NULL; it = it->ai_next) {
    int fd = socket(it->ai_family, it->ai_socktype, it->ai_protocol);
    if (fd < 0) continue;
    int flags = fcntl(fd, F_GETFL, 0);
    if (flags < 0 || fcntl(fd, F_SETFL, flags | O_NONBLOCK) < 0) { close(fd); continue; }
    if ((passive ? bind(fd, it->ai_addr, it->ai_addrlen) : connect(fd, it->ai_addr, it->ai_addrlen)) == 0) {
      freeaddrinfo(results);
      *out_fd = fd;
      return 1;
    }
    close(fd);
  }
  freeaddrinfo(results);
  aura_udp_error(passive ? "bind" : "connect");
  return 0;
}

int aura_udp_bind(const char *host, int64_t port) {
  if (!aura_udp_port(port, 1) || host == NULL || host[0] == '\0') return 0;
  uint16_t value = (uint16_t)port;
  if (aura_udp_find(host, value) != NULL) return 1;
  AuraUdpEntry *entry = aura_udp_slot();
  if (entry == NULL) return 0;
  int fd = -1;
  if (!aura_udp_resolve(host, value, 1, &fd)) return 0;
  entry->fd = fd;
  snprintf(entry->host, sizeof(entry->host), "%s", host);
  entry->port = value;
  entry->active = 1;
  return 1;
}

int aura_udp_wait(const char *host, int64_t port, int timeout_ms) {
  if (timeout_ms < 0) return 0;
  AuraUdpEntry *entry = aura_udp_find(host, (uint16_t)port);
  if (entry == NULL) return 0;
  struct pollfd descriptor = {entry->fd, POLLIN, 0};
  int result = poll(&descriptor, 1, timeout_ms);
  return result > 0 && (descriptor.revents & POLLIN) != 0 ? 1 : 0;
}

const char *aura_udp_receive(const char *host, int64_t port, int64_t capacity,
                             int64_t *source_port, const char **source_host) {
  if (capacity <= 0 || (uint64_t)capacity > SIZE_MAX - 1) return NULL;
  AuraUdpEntry *entry = aura_udp_find(host, (uint16_t)port);
  if (entry == NULL) return NULL;
  char *payload = (char *)malloc((size_t)capacity + 1);
  if (payload == NULL) return NULL;
  struct sockaddr_storage address;
  socklen_t length = sizeof(address);
  ssize_t count = recvfrom(entry->fd, payload, (size_t)capacity, 0,
                           (struct sockaddr *)&address, &length);
  if (count < 0) { free(payload); return NULL; }
  payload[count] = '\0';
  char host_buffer[256];
  char service[16];
  if (getnameinfo((struct sockaddr *)&address, length, host_buffer, sizeof(host_buffer),
                  service, sizeof(service), NI_NUMERICHOST | NI_NUMERICSERV) != 0) {
    free(payload); return NULL;
  }
  *source_port = strtoll(service, NULL, 10);
  *source_host = strdup(host_buffer);
  if (*source_host == NULL) { free(payload); return NULL; }
  return payload;
}

int64_t aura_udp_send(const char *host, int64_t port, const char *target_host,
                      int64_t target_port, const char *payload) {
  if (host == NULL || target_host == NULL || payload == NULL ||
      !aura_udp_port(port, 1) || !aura_udp_port(target_port, 0)) return -1;
  AuraUdpEntry *entry = aura_udp_find(host, (uint16_t)port);
  if (entry == NULL) return -1;
  struct addrinfo hints;
  memset(&hints, 0, sizeof(hints)); hints.ai_family = AF_UNSPEC; hints.ai_socktype = SOCK_DGRAM;
  char service[16]; snprintf(service, sizeof(service), "%u", (unsigned)target_port);
  struct addrinfo *results = NULL;
  if (getaddrinfo(target_host, service, &hints, &results) != 0) return -1;
  ssize_t count = -1;
  for (struct addrinfo *it = results; it != NULL; it = it->ai_next) {
    count = sendto(entry->fd, payload, strlen(payload), 0, it->ai_addr, it->ai_addrlen);
    if (count >= 0) break;
  }
  freeaddrinfo(results);
  return count;
}

int aura_udp_close(const char *host, int64_t port) {
  AuraUdpEntry *entry = aura_udp_find(host, (uint16_t)port);
  if (entry == NULL) return 1;
  close(entry->fd);
  memset(entry, 0, sizeof(*entry));
  return 1;
}

const char *aura_udp_last_error(void) { return aura_udp_errbuf; }

#else
int aura_udp_bind(const char *host, int64_t port) { (void)host; (void)port; return 0; }
int aura_udp_wait(const char *host, int64_t port, int timeout_ms) { (void)host; (void)port; (void)timeout_ms; return 0; }
const char *aura_udp_receive(const char *host, int64_t port, int64_t capacity, int64_t *source_port, const char **source_host) { (void)host; (void)port; (void)capacity; (void)source_port; (void)source_host; return NULL; }
int64_t aura_udp_send(const char *host, int64_t port, const char *target_host, int64_t target_port, const char *payload) { (void)host; (void)port; (void)target_host; (void)target_port; (void)payload; return -1; }
int aura_udp_close(const char *host, int64_t port) { (void)host; (void)port; return 0; }
const char *aura_udp_last_error(void) { return "udp unsupported on this target"; }
#endif
