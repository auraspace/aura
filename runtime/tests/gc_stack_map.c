#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <stddef.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct {
  void *child;
  int64_t value;
} FrameData;

static int child_drops;

static void drop_child(void *value)
{
  (void)value;
  child_drops++;
}

static AuraTaskPollState poll_frame(AuraTaskFrame *frame)
{
  (void)frame;
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  AuraTaskFrame *frame = aura_task_frame_new(sizeof(FrameData), poll_frame, NULL);
  assert(frame != NULL);
  void *sentinel = aura_gc_alloc_full(sizeof(int), NULL, NULL);
  aura_gc_add_root(&sentinel);
  FrameData *data = (FrameData *)aura_task_frame_data(frame);
  data->child = aura_gc_alloc_full(sizeof(int), drop_child, NULL);
  assert(data->child != NULL);
  const AuraTaskFrameGcSlot slots[] = {{(uint32_t)offsetof(FrameData, child)}};
  aura_task_frame_set_gc_stack_map(frame, slots, 1);
  aura_gc_collect();
  assert(child_drops == 0);
  aura_task_frame_destroy(frame);
  aura_gc_collect();
  assert(child_drops == 1);
  aura_gc_remove_root(&sentinel);
  aura_gc_shutdown();
  return 0;
}
