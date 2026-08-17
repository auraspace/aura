static const char *aura_fs_text(const char *path)
{
  return path == NULL ? "" : path;
}

const char *aura_fs_join(const char *base, const char *child)
{
  const char *a = aura_fs_text(base);
  const char *b = aura_fs_text(child);
  size_t alen = strlen(a);
  size_t blen = strlen(b);
  while (alen > 0 && a[alen - 1] == '/')
  {
    alen--;
  }
  while (blen > 0 && *b == '/')
  {
    b++;
    blen--;
  }
  if (alen == 0)
  {
    return aura_bytes_copy_n(b, blen);
  }
  if (blen == 0)
  {
    return aura_bytes_copy_n(a, alen);
  }
  if (alen > SIZE_MAX - blen || alen + blen > SIZE_MAX - 2u)
  {
    return NULL;
  }
  char *out = (char *)malloc(alen + blen + 2u);
  if (out == NULL)
  {
    return NULL;
  }
  memcpy(out, a, alen);
  out[alen] = '/';
  memcpy(out + alen + 1u, b, blen);
  out[alen + blen + 1u] = '\0';
  return out;
}

const char *aura_fs_basename(const char *path)
{
  const char *p = aura_fs_text(path);
  size_t len = strlen(p);
  if (len == 1 && p[0] == '/')
  {
    return aura_bytes_copy_n("/", 1);
  }
  while (len > 0 && p[len - 1] == '/')
  {
    len--;
  }
  size_t start = len;
  while (start > 0 && p[start - 1] != '/')
  {
    start--;
  }
  return aura_bytes_copy_n(p + start, len - start);
}

const char *aura_fs_dirname(const char *path)
{
  const char *p = aura_fs_text(path);
  size_t len = strlen(p);
  if (len == 1 && p[0] == '/')
  {
    return aura_bytes_copy_n("/", 1);
  }
  while (len > 1 && p[len - 1] == '/')
  {
    len--;
  }
  size_t slash = len;
  while (slash > 0 && p[slash - 1] != '/')
  {
    slash--;
  }
  if (slash == 0)
  {
    return aura_bytes_copy_n(".", 1);
  }
  while (slash > 1 && p[slash - 1] == '/')
  {
    slash--;
  }
  return aura_bytes_copy_n(p, slash);
}

const char *aura_fs_extension(const char *path)
{
  const char *name = aura_fs_text(path);
  size_t len = strlen(name);
  while (len > 0 && name[len - 1] == '/')
  {
    len--;
  }
  size_t start = len;
  while (start > 0 && name[start - 1] != '/')
  {
    start--;
  }
  size_t dot = len;
  while (dot > start && name[dot - 1] != '.')
  {
    dot--;
  }
  if (dot == start || dot == len || (dot == start + 1u && len == start + 1u))
  {
    return NULL;
  }
  return aura_bytes_copy_n(name + dot - 1u, len - dot + 1u);
}

_Bool aura_fs_is_absolute(const char *path)
{
  const char *p = aura_fs_text(path);
  return p[0] == '/';
}

_Bool aura_fs_is_directory(const char *path);

_Bool aura_fs_ensure_directory(const char *path)
{
  return aura_platform_ensure_directory(path) != 0;
}

const char *aura_os_get_env(const char *name)
{
  const char *key = name == NULL ? "" : name;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return NULL;
  }
  const char *value = getenv(key);
  return value == NULL ? NULL : aura_bytes_copy(value);
}

_Bool aura_os_set_env(const char *name, const char *value)
{
  const char *key = name == NULL ? "" : name;
  const char *text = value == NULL ? "" : value;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return false;
  }
#if defined(__unix__) || defined(__APPLE__)
  return setenv(key, text, 1) == 0;
#else
  (void)text;
  return false;
#endif
}

_Bool aura_os_unset_env(const char *name)
{
  const char *key = name == NULL ? "" : name;
  if (*key == '\0' || strchr(key, '=') != NULL)
  {
    return false;
  }
#if defined(__unix__) || defined(__APPLE__)
  return unsetenv(key) == 0;
#else
  return false;
#endif
}

const char *aura_os_cwd(void)
{
#if defined(__unix__) || defined(__APPLE__)
  size_t capacity = 256;
  for (;;)
  {
    char *buffer = (char *)malloc(capacity);
    if (buffer == NULL)
    {
      return NULL;
    }
    if (getcwd(buffer, capacity) != NULL)
    {
      return buffer;
    }
    free(buffer);
    if (errno != ERANGE || capacity > SIZE_MAX / 2u)
    {
      return NULL;
    }
    capacity *= 2u;
  }
#else
  return NULL;
#endif
}

int64_t aura_os_pid(void)
{
#if defined(__unix__) || defined(__APPLE__)
  return (int64_t)getpid();
#else
  return -1;
#endif
}

const char *aura_os_platform(void)
{
#if defined(__APPLE__)
  return aura_bytes_copy("macos");
#elif defined(__linux__)
  return aura_bytes_copy("linux");
#elif defined(_WIN32)
  return aura_bytes_copy("windows");
#else
  return aura_bytes_copy("unknown");
#endif
}
