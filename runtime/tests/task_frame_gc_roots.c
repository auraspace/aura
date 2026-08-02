#include <assert.h>
#include <stdint.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/runtime.c"

typedef struct
{
  void *root;
  int polls;
} FrameState;

static int drops;
static int capture_gc_drops;
static int outcome_gc_drops;
static int frame_data_drops;

static void drop_gc(void *data)
{
  (void)data;
  drops++;
}

static void drop_capture(void *data, size_t size)
{
  assert(size == sizeof(void *));
  free(data);
}

static void drop_capture_gc(void *data)
{
  (void)data;
  capture_gc_drops++;
}

static void drop_outcome(void *data, size_t size)
{
  (void)size;
  free(data);
}

static void drop_outcome_gc(void *data)
{
  (void)data;
  outcome_gc_drops++;
}

static void drop_frame_data(AuraTaskFrame *frame, void *data, size_t size)
{
  assert(frame != NULL);
  assert(data != NULL);
  assert(size == sizeof(FrameState));
  frame_data_drops++;
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

static void test_capture_storage_is_scanned_without_callback(void)
{
  void *sentinel = NULL;
  aura_gc_add_root(&sentinel);
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_pending, NULL);
  assert(frame != NULL);
  void *child = aura_gc_alloc_full(sizeof(uint64_t), drop_capture_gc, NULL);
  assert(child != NULL);
  *(uint64_t *)child = UINT64_C(0xcafebabe);
  void **capture = (void **)malloc(sizeof(*capture));
  assert(capture != NULL);
  *capture = child;
  aura_task_frame_set_captures(frame, capture, sizeof(*capture), drop_capture);
  aura_gc_collect();
  assert(*(uint64_t *)child == UINT64_C(0xcafebabe));
  aura_task_frame_destroy(frame);
  aura_gc_collect();
  assert(capture_gc_drops == 1);
  aura_gc_remove_root(&sentinel);
}

static void test_outcome_storage_is_scanned_without_callback(void)
{
  void *sentinel = NULL;
  aura_gc_add_root(&sentinel);
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_pending, NULL);
  assert(frame != NULL);

  void *result_child =
      aura_gc_alloc_full(sizeof(uint64_t), drop_outcome_gc, NULL);
  void *error_child =
      aura_gc_alloc_full(sizeof(uint64_t), drop_outcome_gc, NULL);
  assert(result_child != NULL && error_child != NULL);
  *(uint64_t *)result_child = UINT64_C(0xabcddcba);
  *(uint64_t *)error_child = UINT64_C(0x12344321);

  void **result = (void **)malloc(sizeof(*result));
  void **error = (void **)malloc(sizeof(*error));
  assert(result != NULL && error != NULL);
  *result = result_child;
  *error = error_child;
  aura_task_frame_set_result(frame, result, sizeof(*result), drop_outcome);
  aura_task_frame_set_error(frame, error, sizeof(*error), drop_outcome);

  aura_gc_collect();
  assert(*(uint64_t *)result_child == UINT64_C(0xabcddcba));
  assert(*(uint64_t *)error_child == UINT64_C(0x12344321));

  aura_task_frame_destroy(frame);
  aura_gc_collect();
  assert(outcome_gc_drops == 2);
  aura_gc_remove_root(&sentinel);
}

static void test_error_payload_storage_is_scanned_without_callback(void)
{
  void *sentinel = NULL;
  aura_gc_add_root(&sentinel);
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_pending, NULL);
  assert(frame != NULL);

  void *payload_child =
      aura_gc_alloc_full(sizeof(uint64_t), drop_outcome_gc, NULL);
  assert(payload_child != NULL);
  *(uint64_t *)payload_child = UINT64_C(0x55aa33cc);

  void **payload = (void **)malloc(sizeof(*payload));
  assert(payload != NULL);
  *payload = payload_child;
  aura_task_frame_set_error_payload_with_clone(
      frame, payload, sizeof(*payload), NULL, drop_outcome);

  aura_gc_collect();
  assert(*(uint64_t *)payload_child == UINT64_C(0x55aa33cc));

  aura_task_frame_destroy(frame);
  aura_gc_collect();
  assert(outcome_gc_drops == 3);
  aura_gc_remove_root(&sentinel);
}

static void test_result_release_removes_root_without_destroy_callback(void)
{
  void *sentinel = NULL;
  aura_gc_add_root(&sentinel);
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_pending, NULL);
  assert(frame != NULL);

  void *child = aura_gc_alloc_full(sizeof(uint64_t), drop_outcome_gc, NULL);
  assert(child != NULL);
  *(uint64_t *)child = UINT64_C(0x31415926);
  void **result = (void **)malloc(sizeof(*result));
  assert(result != NULL);
  *result = child;
  /* A NULL drop callback is allowed for externally managed storage, but it
   * must not leave the child rooted after the frame releases the result. */
  aura_task_frame_set_result(frame, result, sizeof(*result), NULL);
  aura_task_frame_destroy(frame);
  aura_gc_collect();
  assert(outcome_gc_drops == 4);
  free(result);
  aura_gc_remove_root(&sentinel);
}

int main(void)
{
  test_capture_storage_is_scanned_without_callback();
  test_outcome_storage_is_scanned_without_callback();
  test_error_payload_storage_is_scanned_without_callback();
  test_result_release_removes_root_without_destroy_callback();
  void *sentinel = aura_gc_alloc(1);
  assert(sentinel != NULL);
  aura_gc_add_root(&sentinel);

  AuraTaskFrame *frame = aura_task_frame_new(sizeof(FrameState),
                                             poll_pending, NULL);
  assert(frame != NULL);
  aura_task_frame_set_gc_mark(frame, mark_frame);
  aura_task_frame_set_data_drop(frame, drop_frame_data);
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
  assert(frame_data_drops == 1);
  aura_gc_collect();
  assert(drops == 2);

  aura_gc_remove_root(&sentinel);
  aura_gc_shutdown();
  return 0;
}
