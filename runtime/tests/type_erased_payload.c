#include <assert.h>
#include <stdlib.h>

#include "../../runtime/aura_ffi.h"
#include "../../runtime/aura_rt.c"

static int drops;
static int marks;
static AuraTypeErasedValue task_source;

static void *clone_int(const void *data, size_t size, size_t *out_size)
{
  int *copy;
  assert(size == sizeof(int));
  copy = (int *)malloc(sizeof(*copy));
  assert(copy != NULL);
  *copy = *(const int *)data;
  *out_size = sizeof(*copy);
  return copy;
}

static void drop_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  drops++;
  free(data);
}

static void mark_int(const void *data, size_t size)
{
  assert(data != NULL);
  assert(size == sizeof(int));
  marks++;
}

static AuraTaskPollState publish_erased(AuraTaskFrame *frame)
{
  assert(aura_task_frame_set_erased_result(frame, &task_source) == AURA_FFI_OK);
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  const AuraTypeErasedOps ops = {
      AURA_TYPE_ERASED_ABI_VERSION, clone_int, drop_int, mark_int};
  AuraTypeErasedValue source;
  AuraTypeErasedValue copy = {0};
  int *value = (int *)malloc(sizeof(*value));
  assert(value != NULL);
  *value = 41;
  source.data = value;
  source.size = sizeof(*value);
  source.ops = &ops;

  assert(aura_type_erased_clone(&source, &copy) == AURA_FFI_OK);
  assert(copy.data != source.data);
  assert(*(int *)copy.data == 41);
  aura_type_erased_mark(&copy);
  assert(marks == 1);
  aura_type_erased_drop(&source);
  aura_type_erased_drop(&copy);
  assert(drops == 2);

  task_source.data = malloc(sizeof(int));
  assert(task_source.data != NULL);
  *(int *)task_source.data = 99;
  task_source.size = sizeof(int);
  task_source.ops = &ops;
  AuraTaskFrame *frame = aura_task_frame_new(0, publish_erased, NULL);
  assert(frame != NULL);
  assert(aura_task_frame_poll_once(frame) == AURA_TASK_COMPLETE);
  AuraTypeErasedValue retrieved = {0};
  assert(aura_task_frame_result_erased(frame, &retrieved) == AURA_FFI_OK);
  assert(retrieved.data != task_source.data);
  assert(*(int *)retrieved.data == 99);
  aura_type_erased_drop(&retrieved);
  aura_task_frame_destroy(frame);
  aura_type_erased_drop(&task_source);

  return 0;
}
