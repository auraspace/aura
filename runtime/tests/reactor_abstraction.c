#define _POSIX_C_SOURCE 200809L

#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

static int custom_poll(void *data, AuraTaskExecutor *executor, int timeout_ms)
{
  int *calls = (int *)data;
  assert(executor != NULL);
  assert(timeout_ms == 17);
  (*calls)++;
  return 0;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  AuraReactor *posix;
  AuraReactor *custom;
  int calls = 0;

  assert(executor != NULL);
  posix = aura_reactor_posix_new();
  assert(posix != NULL);
  aura_reactor_destroy(posix);
  custom = aura_reactor_new(custom_poll, &calls, NULL);
  assert(custom != NULL);
  assert(aura_task_executor_set_reactor(executor, custom) == 1);
  assert(aura_task_executor_poll_waiting(executor, 17) == 0);
  assert(calls == 1);
  assert(aura_task_executor_set_reactor(executor, NULL) == 1);
  aura_task_executor_shutdown(executor);
  return 0;
}
