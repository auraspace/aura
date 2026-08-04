#include <assert.h>
#include <pthread.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

enum { THREADS = 4, ITERATIONS = 10000 };

static void *set_level(void *context)
{
  (void)context;
  for (int i = 0; i < ITERATIONS; i++)
  {
    assert(aura_log_set_min_level(i % 4) == 1);
    assert(aura_log_get_min_level() >= 0);
    assert(aura_log_get_min_level() <= 3);
  }
  return NULL;
}

int main(void)
{
  pthread_t threads[THREADS];
  for (int i = 0; i < THREADS; i++)
    assert(pthread_create(&threads[i], NULL, set_level, NULL) == 0);
  for (int i = 0; i < THREADS; i++)
    assert(pthread_join(threads[i], NULL) == 0);

  assert(aura_log_set_min_level(3) == 1);
  assert(aura_log_get_min_level() == 3);
  assert(aura_log_set_min_level(-1) == 0);
  assert(aura_log_set_min_level(4) == 0);
  return 0;
}
