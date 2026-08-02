#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

static AuraTaskPollState fail_task(AuraTaskFrame *frame)
{
  aura_task_frame_set_error(frame, "child failure", 13, NULL);
  return AURA_TASK_FAILED;
}

int main(void)
{
  AuraTaskExecutor *executor = aura_task_executor_new();
  assert(executor != NULL);
  AuraTaskScope *scope = aura_task_scope_begin(executor);
  assert(scope != NULL);
  AuraTaskFrame *frame = aura_task_frame_new(0, fail_task, NULL);
  assert(frame != NULL);
  assert(aura_task_executor_submit(executor, frame) == 1);
  assert(aura_task_scope_end(scope) == 1);
  aura_task_executor_shutdown(executor);
  return 0;
}
