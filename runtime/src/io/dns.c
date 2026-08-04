/* Bounded numeric DNS lookup: return one address in presentation form. The
 * resolver result is copied into Aura-owned storage and the addrinfo chain is
 * released before returning. */
const char *aura_dns_resolve_host(const char *host, int prefer_ipv6)
{
#if defined(__unix__) || defined(__APPLE__)
  struct addrinfo hints;
  struct addrinfo *results = NULL;
  struct addrinfo *entry;
  char address[INET6_ADDRSTRLEN];
  int families[2];
  int i;

  if (host == NULL || host[0] == '\0') return NULL;
  memset(&hints, 0, sizeof(hints));
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_ADDRCONFIG;
  families[0] = prefer_ipv6 ? AF_INET6 : AF_INET;
  families[1] = prefer_ipv6 ? AF_INET : AF_INET6;
  for (i = 0; i < 2; i++)
  {
    hints.ai_family = families[i];
    if (getaddrinfo(host, NULL, &hints, &results) != 0) continue;
    for (entry = results; entry != NULL; entry = entry->ai_next)
    {
      const void *source = NULL;
      if (entry->ai_family == AF_INET)
      {
        source = &((const struct sockaddr_in *)entry->ai_addr)->sin_addr;
      }
      else if (entry->ai_family == AF_INET6)
      {
        source = &((const struct sockaddr_in6 *)entry->ai_addr)->sin6_addr;
      }
      if (source != NULL && inet_ntop(entry->ai_family, source, address,
                                      sizeof(address)) != NULL)
      {
        const char *copy = aura_bytes_copy(address);
        freeaddrinfo(results);
        return copy;
      }
    }
    freeaddrinfo(results);
    results = NULL;
  }
  return NULL;
#else
  (void)host;
  (void)prefer_ipv6;
  return NULL;
#endif
}

/* Return a bounded, preference-ordered address snapshot. Each line contains
 * one numeric address; the result is Aura-owned and capped at 64 KiB. */
const char *aura_dns_resolve_host_list(const char *host, int prefer_ipv6)
{
#if defined(__unix__) || defined(__APPLE__)
  struct addrinfo hints;
  struct addrinfo *results = NULL;
  struct addrinfo *entry;
  char address[INET6_ADDRSTRLEN];
  int families[2];
  int i;
  size_t used = 0;
  size_t capacity = 64u * 1024u;
  char *output;

  if (host == NULL || host[0] == '\0') return NULL;
  output = (char *)malloc(capacity);
  if (output == NULL) return NULL;
  output[0] = '\0';
  memset(&hints, 0, sizeof(hints));
  hints.ai_socktype = SOCK_STREAM;
  hints.ai_flags = AI_ADDRCONFIG;
  families[0] = prefer_ipv6 ? AF_INET6 : AF_INET;
  families[1] = prefer_ipv6 ? AF_INET : AF_INET6;
  for (i = 0; i < 2; i++)
  {
    hints.ai_family = families[i];
    if (getaddrinfo(host, NULL, &hints, &results) != 0) continue;
    for (entry = results; entry != NULL; entry = entry->ai_next)
    {
      const void *source = NULL;
      size_t length;
      if (entry->ai_family == AF_INET)
      {
        source = &((const struct sockaddr_in *)entry->ai_addr)->sin_addr;
      }
      else if (entry->ai_family == AF_INET6)
      {
        source = &((const struct sockaddr_in6 *)entry->ai_addr)->sin6_addr;
      }
      if (source == NULL || inet_ntop(entry->ai_family, source, address,
                                      sizeof(address)) == NULL)
        continue;
      length = strlen(address);
      if (used + length + (used == 0 ? 0u : 1u) + 1u >= capacity) break;
      if (used != 0) output[used++] = '\n';
      memcpy(output + used, address, length);
      used += length;
      output[used] = '\0';
    }
    freeaddrinfo(results);
    results = NULL;
  }
  if (used == 0)
  {
    free(output);
    return NULL;
  }
  return output;
#else
  (void)host;
  (void)prefer_ipv6;
  return NULL;
#endif
}
