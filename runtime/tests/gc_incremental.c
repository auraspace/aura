#define _POSIX_C_SOURCE 200809L

#include <assert.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

static int drops;

static void drop_value(void *value)
{
  (void)value;
  drops++;
}

int main(void)
{
  void *root = aura_gc_alloc_full(sizeof(int), NULL, NULL);
  aura_gc_add_root(&root);
  for (int i = 0; i < 3; i++)
  {
    assert(aura_gc_alloc_full(sizeof(int), drop_value, NULL) != NULL);
  }

  int steps = 0;
  while (aura_gc_step(1) != 0)
  {
    steps++;
    assert(steps < 16);
  }
  assert(steps > 1);
  assert(drops == 3);

  aura_gc_remove_root(&root);
  aura_gc_collect();
  aura_gc_shutdown();
  return 0;
}
