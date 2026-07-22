#include <assert.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static int capture_drops;
static int frame_destroys;

static void destroy_capture(void *data, size_t size)
{
  assert(size == sizeof(int));
  capture_drops++;
  free(data);
}

static void destroy_frame(AuraTaskFrame *frame)
{
  (void)frame;
  frame_destroys++;
}

static AuraTaskPollState poll_capture(AuraTaskFrame *frame)
{
  int *capture = (int *)aura_task_frame_captures(frame).data;
  assert(capture != NULL && *capture == 42);
  return AURA_TASK_COMPLETE;
}

int main(void)
{
  AuraTaskFrame *frame = aura_task_frame_new(0, poll_capture, destroy_frame);
  assert(frame != NULL);

  int *capture = (int *)malloc(sizeof(*capture));
  assert(capture != NULL);
  *capture = 42;

  int roots_before = aura_gc_root_n;
  assert(aura_task_frame_set_captures_with_ownership(
             frame, capture, sizeof(*capture), destroy_capture, AURA_TASK_OWNED) ==
         1);
  assert(aura_task_frame_capture_ownership(frame) == AURA_TASK_OWNED);
  assert(aura_gc_root_n == roots_before + 1);

  /* The frame, rather than the caller, keeps the capture alive while polling. */
  assert(aura_task_frame_poll_once(frame) == AURA_TASK_COMPLETE);
  assert(aura_task_frame_captures(frame).data == capture);
  assert(capture_drops == 0);

  int *borrowed = (int *)malloc(sizeof(*borrowed));
  assert(borrowed != NULL);
  *borrowed = 7;
  assert(aura_task_frame_set_captures_with_ownership(
             frame, borrowed, sizeof(*borrowed), destroy_capture, AURA_TASK_BORROWED) ==
         0);
  assert(aura_task_frame_captures(frame).data == capture);
  assert(aura_gc_root_n == roots_before + 1);
  free(borrowed);

  aura_task_frame_destroy(frame);
  assert(capture_drops == 1);
  assert(frame_destroys == 1);
  assert(aura_gc_root_n == roots_before);
  return 0;
}
