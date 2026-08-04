#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <stdint.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct
{
  uintptr_t disguised_pointer;
} TypedPayload;

static void *child_target;
static int child_drops;

static void drop_payload(void *value)
{
  if (value == child_target)
  {
    child_drops++;
  }
}

static void trace_no_fields(void *value)
{
  (void)value;
}

int main(void)
{
  TypedPayload *root = (TypedPayload *)aura_gc_alloc_typed(
      sizeof(TypedPayload), NULL, trace_no_fields);
  assert(root != NULL);
  aura_gc_add_root((void **)&root);

  child_target = aura_gc_alloc_full(sizeof(uint64_t), drop_payload, NULL);
  assert(child_target != NULL);
  root->disguised_pointer = (uintptr_t)child_target;

  aura_gc_collect();
  assert(child_drops == 1);

  aura_gc_remove_root((void **)&root);
  aura_gc_collect();
  aura_gc_shutdown();
  return 0;
}
