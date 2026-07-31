#include <assert.h>
#include <pthread.h>
#include <stdlib.h>

#define AURA_RUNTIME_NO_MAIN
#include "../../runtime/aura_rt.c"

static void destroy_int(void *data, size_t size)
{
  assert(size == sizeof(int));
  free(data);
}

typedef struct
{
  AuraTaskChannel *channel;
  int base;
} Producer;

static void *produce(void *arg)
{
  Producer *producer = (Producer *)arg;
  for (int i = 0; i < 1000; i++)
  {
    int *value = (int *)malloc(sizeof(*value));
    assert(value != NULL);
    *value = producer->base + i;
    AuraTaskChannelValue payload = {value, sizeof(*value), destroy_int};
    assert(aura_task_channel_send(producer->channel, NULL, payload) == AURA_CHANNEL_OK);
  }
  return NULL;
}

int main(void)
{
  enum { PRODUCERS = 4, VALUES_PER_PRODUCER = 1000 };
  AuraTaskChannel *channel = aura_task_channel_new(PRODUCERS * VALUES_PER_PRODUCER);
  assert(channel != NULL);
  pthread_t threads[PRODUCERS];
  Producer producers[PRODUCERS];
  for (int i = 0; i < PRODUCERS; i++)
  {
    producers[i] = (Producer){channel, i * VALUES_PER_PRODUCER};
    assert(pthread_create(&threads[i], NULL, produce, &producers[i]) == 0);
  }
  for (int i = 0; i < PRODUCERS; i++) assert(pthread_join(threads[i], NULL) == 0);
  assert(aura_task_channel_count(channel) == PRODUCERS * VALUES_PER_PRODUCER);
  for (int i = 0; i < PRODUCERS * VALUES_PER_PRODUCER; i++)
  {
    AuraTaskChannelValue value = {NULL, 0, NULL};
    assert(aura_task_channel_receive(channel, NULL, &value) == AURA_CHANNEL_OK);
    aura_task_channel_value_destroy(&value);
  }
  assert(aura_task_channel_count(channel) == 0);
  aura_task_channel_destroy(channel);
  return 0;
}
