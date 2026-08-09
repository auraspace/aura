#define _POSIX_C_SOURCE 200809L

#include <assert.h>
#include <pthread.h>
#include <stdatomic.h>

#define AURA_RUNTIME_NO_MAIN
#include "../runtime.c"

typedef struct
{
  _Atomic(void *) child;
} ManagedNode;

typedef struct
{
  ManagedNode *owner;
  atomic_int stop;
} Mutator;

static void trace_node(void *raw)
{
  ManagedNode *node = (ManagedNode *)raw;
  aura_gc_mark_ptr(atomic_load_explicit(&node->child, memory_order_acquire));
}

static int pause_noop(void *context)
{
  (void)context;
  return 1;
}

static void resume_noop(void *context)
{
  (void)context;
}

static void *mutate_node(void *context)
{
  Mutator *mutator = (Mutator *)context;
  for (int i = 0; i < 10000 && !atomic_load(&mutator->stop); i++)
  {
    void *child = aura_gc_alloc_full(sizeof(uint64_t), NULL, NULL);
    assert(child != NULL);
    atomic_store_explicit(&mutator->owner->child, child, memory_order_release);
    aura_gc_write_barrier(mutator->owner, child);
  }
  return NULL;
}

int main(void)
{
  ManagedNode *owner = (ManagedNode *)aura_gc_alloc_typed(
      sizeof(*owner), NULL, trace_node);
  assert(owner != NULL);
  atomic_init(&owner->child, NULL);
  void *root = owner;
  aura_gc_add_root(&root);

  Mutator mutator = {owner, 0};
  pthread_t thread;
  assert(pthread_create(&thread, NULL, mutate_node, &mutator) == 0);
  assert(aura_gc_start_concurrent(NULL, pause_noop, resume_noop) == 1);
  aura_gc_wait_background();

  atomic_store(&mutator.stop, 1);
  assert(pthread_join(thread, NULL) == 0);
  aura_gc_remove_root(&root);
  aura_gc_collect();
  aura_gc_shutdown();
  return 0;
}
