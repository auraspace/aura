#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

static double elapsed_seconds(const struct timespec *start, const struct timespec *end)
{
  return (double)(end->tv_sec - start->tv_sec) + (double)(end->tv_nsec - start->tv_nsec) / 1000000000.0;
}

static volatile uint32_t benchmark_sink;

static double run(AuraSha256BlockFn backend, const unsigned char *data, size_t length, size_t iterations)
{
  struct timespec start, end;
  clock_gettime(CLOCK_MONOTONIC, &start);
  for (size_t i = 0; i < iterations; i++)
  {
    AuraSha256 ctx;
    aura_sha256_init(&ctx);
    for (size_t offset = 0; offset < length; offset += 64u) backend(&ctx, data + offset);
    benchmark_sink ^= ctx.state[0];
  }
  clock_gettime(CLOCK_MONOTONIC, &end);
  return elapsed_seconds(&start, &end);
}

int main(void)
{
  const size_t length = 1024u * 1024u;
  const size_t iterations = 32u;
  unsigned char *data = (unsigned char *)malloc(length);
  if (data == NULL) return 1;
  for (size_t i = 0; i < length; i++) data[i] = (unsigned char)(i * 13u + 7u);
  AuraSha256BlockFn selected = aura_sha256_get_block_fn();
  double portable_seconds = run(aura_sha256_block_portable, data, length, iterations);
  double selected_seconds = run(selected, data, length, iterations);
  printf("arch=%s compiler=%s input=%zu iterations=%zu portable=%.3fs selected=%.3fs backend=%s\n",
#if defined(__aarch64__)
         "arm64",
#elif defined(__x86_64__)
         "x86_64",
#else
         "other",
#endif
#if defined(__clang__)
         "clang",
#elif defined(__GNUC__)
         "gcc",
#else
         "unknown",
#endif
         length, iterations, portable_seconds, selected_seconds,
         selected == aura_sha256_block_portable ? "portable" : "intrinsic");
  free(data);
  return 0;
}
