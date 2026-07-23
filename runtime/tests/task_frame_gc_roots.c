#include <assert.h>
#include <stdint.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

typedef struct
{
  void *root;
  int polls;
} FrameState;

static int drops;

static void drop_gc(void *data)
{
  (void)data;
  drops++;
}

static AuraTaskPollState poll_pending(AuraTaskFrame *frame)
{
  FrameState *state = (FrameState *)aura_task_frame_data(frame);
  state->polls++;
  return AURA_TASK_PENDING;
}

static void mark_frame(AuraTaskFrame *frame)
{
  FrameState *state = (FrameState *)aura_task_frame_data(frame);
  aura_gc_mark_ptr(state->root);
}

int main(void)
{
  void *sentinel = aura_gc_alloc(1);
  assert(sentinel != NULL);
  aura_gc_add_root(&sentinel);

  AuraTaskFrame *frame = aura_task_frame_new(sizeof(FrameState),
                                             poll_pending, NULL);
  assert(frame != NULL);
  aura_task_frame_set_gc_mark(frame, mark_frame);
  FrameState *state = (FrameState *)aura_task_frame_data(frame);

  void *child = aura_gc_alloc_full(sizeof(uint64_t), drop_gc, NULL);
  void *parent = aura_gc_alloc_full(sizeof(void *), drop_gc, NULL);
  assert(child != NULL && parent != NULL);
  *(void **)parent = child;
  *(uint64_t *)child = UINT64_C(0xfeedface);
  state->root = parent;

  assert(aura_task_frame_poll_once(frame) == AURA_TASK_PENDING);
  assert(state->polls == 1);
  for (int i = 0; i < 8; i++)
  {
    (void)aura_gc_alloc(16 + (size_t)i);
    aura_gc_collect();
    assert(*(uint64_t *)child == UINT64_C(0xfeedface));
    assert(*(void **)parent == child);
  }
  assert(drops == 0);

  aura_task_frame_destroy(frame);
  aura_gc_collect();
  assert(drops == 2);

  aura_gc_remove_root(&sentinel);
  aura_gc_shutdown();
  return 0;
}
