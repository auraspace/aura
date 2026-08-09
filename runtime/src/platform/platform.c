#include "platform.h"

#if defined(_WIN32)
#include <windows.h>

int64_t aura_platform_monotonic_millis(void)
{
  static LARGE_INTEGER frequency;
  static int initialized = 0;
  LARGE_INTEGER counter;
  if (!initialized)
  {
    if (!QueryPerformanceFrequency(&frequency) || frequency.QuadPart <= 0)
    {
      return 0;
    }
    initialized = 1;
  }
  if (!QueryPerformanceCounter(&counter))
  {
    return 0;
  }
  return (int64_t)((counter.QuadPart * 1000) / frequency.QuadPart);
}
#else
#include <time.h>

int64_t aura_platform_monotonic_millis(void)
{
  struct timespec now;
  if (clock_gettime(CLOCK_MONOTONIC, &now) != 0)
  {
    return 0;
  }
  return (int64_t)now.tv_sec * 1000 + now.tv_nsec / 1000000;
}
#endif
