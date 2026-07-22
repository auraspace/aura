#include <assert.h>
#include <stdlib.h>

#include "../../runtime/aura_rt.c"

static int destroyed;
static int result_destroyed;

static void destroy_frame(AuraTaskFrame *frame)
{
  (void)frame;
  destroyed++;
}

static void destroy_result(void *data, size_t size)
{
  assert(size == sizeof(int));
  result_destroyed++;
  free(data);
}

static AuraTaskPollState poll_once(AuraTaskFrame *frame)
{
  int *value = (int *)aura_task_frame_data(frame);
  *value = 42;
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(int), poll_once, destroy_frame);
  assert(frame != NULL);
  assert(frame->abi_version == AURA_RT_ABI_VERSION);
  assert(aura_task_frame_state(frame) == AURA_TASK_READY);
  assert(aura_task_frame_data(frame) != NULL);
  assert(aura_task_frame_cancel_requested(frame) == 0);

  int *result = (int *)malloc(sizeof(int));
  assert(result != NULL);
  *result = 7;
  aura_task_frame_set_result(frame, result, sizeof(*result), destroy_result);
  assert(aura_task_frame_result(frame).data == result);
  aura_task_frame_destroy(frame);
  assert(destroyed == 1);
  assert(result_destroyed == 1);

  assert(aura_task_frame_new(0, NULL, NULL) == NULL);
  return 0;
}
