void aura_print(const char *s)
{
  if (s == NULL)
  {
    fputs("null", stdout);
  }
  else
  {
    fputs(s, stdout);
  }
  fflush(stdout);
}

void aura_println(const char *s)
{
  if (s == NULL)
  {
    puts("null");
  }
  else
  {
    puts(s);
  }
}

void aura_eprint(const char *s)
{
  if (s == NULL)
  {
    fputs("null", stderr);
  }
  else
  {
    fputs(s, stderr);
  }
  fflush(stderr);
}

void aura_eprintln(const char *s)
{
  if (s == NULL)
  {
    fputs("null\n", stderr);
  }
  else
  {
    fputs(s, stderr);
    fputc('\n', stderr);
  }
  fflush(stderr);
}

static int aura_log_min_level = 0;

int aura_log_set_min_level(int level)
{
  if (level < 0 || level > 3) return 0;
  aura_log_min_level = level;
  return 1;
}

int aura_log_get_min_level(void)
{
  return aura_log_min_level;
}

void aura_log(int level, const char *message)
{
  static const char *const names[] = {"DEBUG", "INFO", "WARN", "ERROR"};
  if (level < aura_log_min_level) return;
  const char *name = (level >= 0 && level < 4) ? names[level] : "INFO";
  fprintf(stderr, "[%s] %s\n", name, message != NULL ? message : "null");
  fflush(stderr);
}

