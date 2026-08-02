/* ---- C22n bounded FIFO channels (single-threaded MVP) ----
 *
 * A channel owns every value accepted by aura_task_channel_send.  A queued
 * value is delivered by moving the value record to the receiver's output;
 * after that point the receiver owns it and must invoke its destroy callback.
 * Values rejected by a closed channel, or held by a waiting sender when the
 * channel closes, are destroyed exactly once by the channel.  A send that
 * returns AURA_CHANNEL_PENDING transfers ownership to the channel.
 *
 * Waiting frames are borrowed references.  The executor owns their lifetime;
 * cancellation and executor shutdown unlink waiters before destroying the
 * frame.  Wakeups are FIFO and use the frame's executor, with no OS threads.
 */

#ifndef AURA_TASK_CHANNEL_ABI_DEFINED
typedef void (*AuraTaskChannelValueDestroyFn)(void *data, size_t size);

typedef struct
{
  void *data;
  size_t size;
  AuraTaskChannelValueDestroyFn destroy;
} AuraTaskChannelValue;

typedef enum
{
  AURA_CHANNEL_OK = 0,
  AURA_CHANNEL_PENDING = 1,
  AURA_CHANNEL_CLOSED = 2,
  AURA_CHANNEL_ERROR = 3
} AuraTaskChannelStatus;
#define AURA_TASK_CHANNEL_ABI_DEFINED 1
#endif

typedef struct AuraTaskChannelWaiter AuraTaskChannelWaiter;

struct AuraTaskChannelWaiter
{
  AuraTaskFrame *frame;
  AuraTaskChannelValue value;
  AuraTaskChannelValue *out;
  AuraTaskSelect *select;
  size_t select_index;
  AuraTaskChannelWaiter *next;
};

struct AuraTaskSelect
{
  AuraTaskChannel **channels;
  size_t count;
  size_t capacity;
  size_t next_index;
  AuraTaskFrame *frame;
  AuraTaskChannelValue value;
  size_t selected_index;
  AuraTaskChannelStatus selected_status;
  int selected;
};

AuraTaskChannelStatus aura_task_channel_receive(AuraTaskChannel *channel,
                                                 AuraTaskFrame *receiver,
                                                 AuraTaskChannelValue *out);

struct AuraTaskChannel
{
  AuraTaskChannelValue *values;
  size_t capacity;
  size_t head;
  size_t tail;
  size_t count;
  int closed;
  AuraTaskChannelWaiter *send_head;
  AuraTaskChannelWaiter *send_tail;
  AuraTaskChannelWaiter *receive_head;
  AuraTaskChannelWaiter *receive_tail;
  AuraRaceTracker *race_tracker;
  size_t refs;
};

#if defined(__unix__) || defined(__APPLE__)
static pthread_once_t aura_task_channel_lock_once = PTHREAD_ONCE_INIT;
static pthread_mutex_t aura_task_channel_lock;

static void aura_task_channel_lock_init(void)
{
  pthread_mutexattr_t attributes;
  pthread_mutexattr_init(&attributes);
  pthread_mutexattr_settype(&attributes, PTHREAD_MUTEX_RECURSIVE);
  pthread_mutex_init(&aura_task_channel_lock, &attributes);
  pthread_mutexattr_destroy(&attributes);
}

static void aura_task_channel_lock_enter(void)
{
  pthread_once(&aura_task_channel_lock_once, aura_task_channel_lock_init);
  pthread_mutex_lock(&aura_task_channel_lock);
}

static void aura_task_channel_lock_leave(void)
{
  pthread_mutex_unlock(&aura_task_channel_lock);
}
#else
static void aura_task_channel_lock_enter(void) {}
static void aura_task_channel_lock_leave(void) {}
#endif

void aura_task_channel_set_race_tracker(AuraTaskChannel *channel,
                                         AuraRaceTracker *tracker)
{
  if (channel != NULL)
  {
    channel->race_tracker = tracker;
  }
}

static void aura_task_channel_record(AuraTaskChannel *channel,
                                     AuraTaskFrame *frame,
                                     AuraRaceEventKind kind)
{
  if (channel != NULL && channel->race_tracker != NULL)
  {
    (void)aura_race_tracker_record(channel->race_tracker,
                                   frame != NULL ? frame->task_id : 0,
                                   (uintptr_t)channel,
                                   0,
                                   kind,
                                   NULL);
  }
}

static void aura_task_channel_value_destroy(AuraTaskChannelValue *value)
{
  if (value != NULL && value->destroy != NULL && value->data != NULL)
  {
    value->destroy(value->data, value->size);
  }
  if (value != NULL)
  {
    value->data = NULL;
    value->size = 0;
    value->destroy = NULL;
  }
}

/* C22o glue: generated send/receive expressions use these stable callbacks.
 * The class form also releases the temporary GC root held by the payload box. */
void aura_task_channel_value_destroy_free(void *data, size_t size)
{
  (void)size;
  free(data);
}

void aura_task_channel_value_destroy_class(void *data, size_t size)
{
  (void)size;
  if (data != NULL)
  {
    aura_gc_remove_root((void **)data);
    free(data);
  }
}

typedef struct
{
  AuraTaskExecutor *executor;
  AuraTaskFrame *frame;
} AuraTaskPayloadRef;

typedef struct
{
  AuraTaskChannel *channel;
} AuraChannelPayloadRef;

/* Forward declaration because the channel payload callback is defined beside
 * the other value destructors, before the channel implementation. */
void aura_task_channel_destroy(AuraTaskChannel *channel);

void aura_task_channel_value_destroy_task(void *data, size_t size)
{
  (void)size;
  AuraTaskPayloadRef *payload = (AuraTaskPayloadRef *)data;
  if (payload != NULL)
  {
    if (payload->frame != NULL)
    {
      (void)aura_task_executor_release_payload(payload->executor,
                                                &payload->frame);
    }
    free(payload);
  }
}

AuraTaskFrame *aura_task_channel_value_take_task(void *data, size_t size)
{
  (void)size;
  AuraTaskPayloadRef *payload = (AuraTaskPayloadRef *)data;
  if (payload == NULL)
  {
    return NULL;
  }
  AuraTaskFrame *frame = payload->frame;
  payload->frame = NULL;
  free(payload);
  return frame;
}

void aura_task_channel_value_destroy_channel(void *data, size_t size)
{
  (void)size;
  AuraChannelPayloadRef *payload = (AuraChannelPayloadRef *)data;
  if (payload != NULL)
  {
    if (payload->channel != NULL)
    {
      AuraTaskChannel *channel = payload->channel;
      payload->channel = NULL;
      aura_task_channel_destroy(channel);
    }
    free(payload);
  }
}

AuraTaskChannel *aura_task_channel_value_take_channel(void *data, size_t size)
{
  (void)size;
  AuraChannelPayloadRef *payload = (AuraChannelPayloadRef *)data;
  if (payload == NULL)
  {
    return NULL;
  }
  AuraTaskChannel *channel = payload->channel;
  payload->channel = NULL;
  free(payload);
  return channel;
}

static void aura_task_channel_wake(AuraTaskFrame *frame)
{
  if (frame != NULL && frame->executor != NULL)
  {
    (void)aura_task_executor_wake(frame->executor, frame);
  }
}

static void aura_task_channel_unlink(AuraTaskChannel *channel,
                                     AuraTaskChannelWaiter *target,
                                     int receiver)
{
  AuraTaskChannelWaiter **link = receiver ? &channel->receive_head : &channel->send_head;
  AuraTaskChannelWaiter *tail = receiver ? channel->receive_tail : channel->send_tail;
  while (*link != NULL && *link != target)
  {
    link = &(*link)->next;
  }
  if (*link == NULL)
  {
    return;
  }
  *link = target->next;
  if (tail == target)
  {
    if (receiver)
    {
      channel->receive_tail = NULL;
      for (AuraTaskChannelWaiter *w = channel->receive_head; w != NULL; w = w->next)
        channel->receive_tail = w;
    }
    else
    {
      channel->send_tail = NULL;
      for (AuraTaskChannelWaiter *w = channel->send_head; w != NULL; w = w->next)
        channel->send_tail = w;
    }
  }
  target->next = NULL;
}

static void aura_task_channel_cancel_wait(AuraTaskFrame *frame)
{
  if (frame == NULL || frame->waiting_channel == NULL || frame->waiting_node == NULL)
  {
    return;
  }
  AuraTaskChannel *channel = frame->waiting_channel;
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)frame->waiting_node;
  aura_task_channel_lock_enter();
  int receiver = waiter->out != NULL;
  aura_task_channel_unlink(channel, waiter, receiver);
  if (!receiver)
  {
    aura_task_channel_value_destroy(&waiter->value);
  }
  free(waiter);
  frame->waiting_channel = NULL;
  frame->waiting_node = NULL;
  aura_task_channel_lock_leave();
}

static void aura_task_select_unlink_waiters(AuraTaskSelect *select)
{
  if (select == NULL) return;
  for (size_t i = 0; i < select->count; i++)
  {
    AuraTaskChannel *channel = select->channels[i];
    if (channel == NULL) continue;
    AuraTaskChannelWaiter **link = &channel->receive_head;
    while (*link != NULL)
    {
      AuraTaskChannelWaiter *waiter = *link;
      if (waiter->select != select)
      {
        link = &waiter->next;
        continue;
      }
      *link = waiter->next;
      if (channel->receive_tail == waiter)
      {
        channel->receive_tail = NULL;
        for (AuraTaskChannelWaiter *tail = channel->receive_head; tail != NULL; tail = tail->next)
          channel->receive_tail = tail;
      }
      free(waiter);
    }
  }
}

static void aura_task_select_cancel_wait(AuraTaskFrame *frame)
{
  if (frame == NULL || frame->waiting_select == NULL) return;
  aura_task_channel_lock_enter();
  AuraTaskSelect *select = frame->waiting_select;
  aura_task_select_unlink_waiters(select);
  select->frame = NULL;
  frame->waiting_select = NULL;
  frame->waiting_node = NULL;
  aura_task_channel_lock_leave();
}

static void aura_task_select_finish(AuraTaskSelect *select,
                                    AuraTaskChannelStatus status,
                                    size_t index,
                                    AuraTaskChannelValue value)
{
  if (select == NULL || select->selected) return;
  select->selected = 1;
  select->selected_index = index;
  select->selected_status = status;
  select->value = value;
  AuraTaskFrame *frame = select->frame;
  aura_task_select_unlink_waiters(select);
  select->frame = NULL;
  if (frame != NULL)
  {
    frame->waiting_select = NULL;
    frame->waiting_node = NULL;
    aura_task_channel_wake(frame);
  }
  (void)status;
}

AuraTaskSelect *aura_task_select_new(void)
{
  return (AuraTaskSelect *)calloc(1, sizeof(AuraTaskSelect));
}

int aura_task_select_add(AuraTaskSelect *select, AuraTaskChannel *channel)
{
  if (select == NULL || channel == NULL) return 0;
  aura_task_channel_lock_enter();
  if (select->count == select->capacity)
  {
    size_t capacity = select->capacity == 0 ? 4 : select->capacity * 2;
    AuraTaskChannel **channels = (AuraTaskChannel **)realloc(select->channels, capacity * sizeof(*channels));
    if (channels == NULL) { aura_task_channel_lock_leave(); return 0; }
    select->channels = channels;
    select->capacity = capacity;
  }
  select->channels[select->count++] = channel;
  aura_task_channel_lock_leave();
  return 1;
}

AuraTaskChannelStatus aura_task_select_next(AuraTaskSelect *select,
                                            AuraTaskFrame *frame,
                                            AuraTaskChannelValue *out,
                                            size_t *index)
{
  if (select == NULL || out == NULL || index == NULL || select->count == 0)
    return AURA_CHANNEL_ERROR;
  aura_task_channel_lock_enter();
  if (select->selected)
  {
    *out = select->value;
    select->value = (AuraTaskChannelValue){NULL, 0, NULL};
    *index = select->selected_index;
    select->selected = 0;
    AuraTaskChannelStatus status = select->selected_status;
    aura_task_channel_lock_leave();
    return status;
  }
  size_t start = select->next_index % select->count;
  for (size_t offset = 0; offset < select->count; offset++)
  {
    size_t i = (start + offset) % select->count;
    AuraTaskChannelValue value = {NULL, 0, NULL};
    AuraTaskChannelStatus status = aura_task_channel_receive(select->channels[i], NULL, &value);
    if (status == AURA_CHANNEL_OK)
    {
      select->next_index = (i + 1) % select->count;
      *out = value;
      *index = i;
      aura_task_channel_lock_leave();
      return status;
    }
    if (status == AURA_CHANNEL_CLOSED)
    {
      select->next_index = (i + 1) % select->count;
      *out = (AuraTaskChannelValue){NULL, 0, NULL};
      *index = i;
      aura_task_channel_lock_leave();
      return status;
    }
  }
  if (frame == NULL) { aura_task_channel_lock_leave(); return AURA_CHANNEL_PENDING; }
  select->frame = frame;
  frame->waiting_select = select;
  frame->waiting_node = select;
  for (size_t offset = 0; offset < select->count; offset++)
  {
    size_t i = (start + offset) % select->count;
    AuraTaskChannel *channel = select->channels[i];
    AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
    if (waiter == NULL)
    {
      aura_task_select_cancel_wait(frame);
      aura_task_channel_lock_leave();
      return AURA_CHANNEL_ERROR;
    }
    waiter->frame = frame;
    waiter->select = select;
    waiter->select_index = i;
    if (channel->receive_tail == NULL) channel->receive_head = waiter;
    else channel->receive_tail->next = waiter;
    channel->receive_tail = waiter;
  }
  aura_task_channel_lock_leave();
  return AURA_CHANNEL_PENDING;
}

size_t aura_task_select_selected_index(const AuraTaskSelect *select)
{
  return select != NULL ? select->selected_index : 0;
}

void aura_task_select_destroy(AuraTaskSelect *select)
{
  if (select == NULL) return;
  aura_task_channel_lock_enter();
  aura_task_select_unlink_waiters(select);
  free(select->channels);
  aura_task_channel_value_destroy(&select->value);
  aura_task_channel_lock_leave();
  free(select);
}

AuraTaskChannel *aura_task_channel_new(size_t capacity)
{
  if (capacity == 0)
  {
    return NULL;
  }
  AuraTaskChannel *channel = (AuraTaskChannel *)calloc(1, sizeof(*channel));
  if (channel == NULL)
  {
    return NULL;
  }
  channel->values = (AuraTaskChannelValue *)calloc(capacity, sizeof(*channel->values));
  if (channel->values == NULL)
  {
    free(channel);
    return NULL;
  }
  channel->capacity = capacity;
  channel->refs = 1;
  return channel;
}

int aura_task_channel_retain(AuraTaskChannel *channel)
{
  if (channel == NULL)
  {
    return 0;
  }
  aura_task_channel_lock_enter();
  if (channel->refs == 0)
  {
    aura_task_channel_lock_leave();
    return 0;
  }
  channel->refs++;
  aura_task_channel_lock_leave();
  return 1;
}

AuraTaskChannelValue aura_task_channel_value_from_task(AuraTaskExecutor *executor,
                                                        AuraTaskFrame *frame)
{
  AuraTaskChannelValue value = {NULL, 0, NULL};
  AuraTaskPayloadRef *payload = (AuraTaskPayloadRef *)calloc(1, sizeof(*payload));
  if (payload == NULL ||
      !aura_task_executor_retain_payload(executor, frame))
  {
    free(payload);
    return value;
  }
  payload->executor = executor;
  payload->frame = frame;
  value.data = payload;
  value.size = sizeof(*payload);
  value.destroy = aura_task_channel_value_destroy_task;
  return value;
}

AuraTaskChannelValue aura_task_channel_value_from_channel(AuraTaskChannel *channel)
{
  AuraTaskChannelValue value = {NULL, 0, NULL};
  AuraChannelPayloadRef *payload = (AuraChannelPayloadRef *)calloc(1, sizeof(*payload));
  if (payload == NULL || !aura_task_channel_retain(channel))
  {
    free(payload);
    return value;
  }
  payload->channel = channel;
  value.data = payload;
  value.size = sizeof(*payload);
  value.destroy = aura_task_channel_value_destroy_channel;
  return value;
}

size_t aura_task_channel_capacity(const AuraTaskChannel *channel)
{
  return channel != NULL ? channel->capacity : 0;
}

size_t aura_task_channel_count(const AuraTaskChannel *channel)
{
  size_t count = 0;
  if (channel != NULL)
  {
    aura_task_channel_lock_enter();
    count = channel->count;
    aura_task_channel_lock_leave();
  }
  return count;
}

int aura_task_channel_is_closed(const AuraTaskChannel *channel)
{
  int closed = 0;
  if (channel != NULL)
  {
    aura_task_channel_lock_enter();
    closed = channel->closed;
    aura_task_channel_lock_leave();
  }
  return closed;
}

AuraTaskChannelStatus aura_task_channel_send(AuraTaskChannel *channel,
                                              AuraTaskFrame *sender,
                                              AuraTaskChannelValue value)
{
  if (channel == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  aura_task_channel_lock_enter();
  aura_task_channel_record(channel, sender, AURA_RACE_CHANNEL_SEND);
  if (channel->closed)
  {
    aura_task_channel_value_destroy(&value);
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_CLOSED;
  }
  if (channel->receive_head != NULL)
  {
    AuraTaskChannelWaiter *receiver = channel->receive_head;
    aura_task_channel_unlink(channel, receiver, 1);
    if (receiver->select != NULL)
    {
      AuraTaskSelect *select = receiver->select;
      size_t index = receiver->select_index;
      free(receiver);
      aura_task_select_finish(select, AURA_CHANNEL_OK, index, value);
      aura_task_channel_lock_leave();
      return AURA_CHANNEL_OK;
    }
    *receiver->out = value;
    AuraTaskFrame *receiver_frame = receiver->frame;
    receiver_frame->waiting_channel = NULL;
    receiver_frame->waiting_node = NULL;
    free(receiver);
    aura_task_channel_wake(receiver_frame);
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_OK;
  }
  if (channel->count < channel->capacity)
  {
    channel->values[channel->tail] = value;
    channel->tail = (channel->tail + 1) % channel->capacity;
    channel->count++;
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_OK;
  }
  if (sender == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_PENDING;
  }
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
  if (waiter == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_ERROR;
  }
  waiter->frame = sender;
  waiter->value = value;
  if (channel->send_tail == NULL)
    channel->send_head = waiter;
  else
    channel->send_tail->next = waiter;
  channel->send_tail = waiter;
  sender->waiting_channel = channel;
  sender->waiting_node = waiter;
  aura_task_channel_lock_leave();
  return AURA_CHANNEL_PENDING;
}

AuraTaskChannelStatus aura_task_channel_receive(AuraTaskChannel *channel,
                                                 AuraTaskFrame *receiver,
                                                 AuraTaskChannelValue *out)
{
  if (channel == NULL || out == NULL)
  {
    return AURA_CHANNEL_ERROR;
  }
  aura_task_channel_lock_enter();
  aura_task_channel_record(channel, receiver, AURA_RACE_CHANNEL_RECEIVE);
  if (channel->count != 0)
  {
    *out = channel->values[channel->head];
    channel->values[channel->head] = (AuraTaskChannelValue){NULL, 0, NULL};
    channel->head = (channel->head + 1) % channel->capacity;
    channel->count--;
    if (channel->send_head != NULL)
    {
      AuraTaskChannelWaiter *sender = channel->send_head;
      aura_task_channel_unlink(channel, sender, 0);
      channel->values[channel->tail] = sender->value;
      channel->tail = (channel->tail + 1) % channel->capacity;
      channel->count++;
      AuraTaskFrame *sender_frame = sender->frame;
      sender_frame->waiting_channel = NULL;
      sender_frame->waiting_node = NULL;
      free(sender);
    aura_task_channel_wake(sender_frame);
    }
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_OK;
  }
  if (channel->closed)
  {
    *out = (AuraTaskChannelValue){NULL, 0, NULL};
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_CLOSED;
  }
  if (receiver == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_PENDING;
  }
  AuraTaskChannelWaiter *waiter = (AuraTaskChannelWaiter *)calloc(1, sizeof(*waiter));
  if (waiter == NULL)
  {
    aura_task_channel_lock_leave();
    return AURA_CHANNEL_ERROR;
  }
  waiter->frame = receiver;
  waiter->out = out;
  if (channel->receive_tail == NULL)
    channel->receive_head = waiter;
  else
    channel->receive_tail->next = waiter;
  channel->receive_tail = waiter;
  receiver->waiting_channel = channel;
  receiver->waiting_node = waiter;
  aura_task_channel_lock_leave();
  return AURA_CHANNEL_PENDING;
}

int aura_task_channel_close_from(AuraTaskChannel *channel, AuraTaskFrame *closer)
{
  if (channel == NULL)
  {
    return 0;
  }
  aura_task_channel_lock_enter();
  if (channel->closed)
  {
    aura_task_channel_lock_leave();
    return 0;
  }
  aura_task_channel_record(channel, closer, AURA_RACE_CHANNEL_CLOSE);
  channel->closed = 1;
  while (channel->send_head != NULL)
  {
    AuraTaskChannelWaiter *waiter = channel->send_head;
    aura_task_channel_unlink(channel, waiter, 0);
    aura_task_channel_value_destroy(&waiter->value);
    waiter->frame->waiting_channel = NULL;
    waiter->frame->waiting_node = NULL;
    AuraTaskFrame *frame = waiter->frame;
    free(waiter);
    aura_task_channel_wake(frame);
  }
  while (channel->receive_head != NULL)
  {
    AuraTaskChannelWaiter *waiter = channel->receive_head;
    aura_task_channel_unlink(channel, waiter, 1);
    AuraTaskFrame *frame = waiter->frame;
    if (waiter->select != NULL)
    {
      AuraTaskSelect *select = waiter->select;
      size_t index = waiter->select_index;
      free(waiter);
      aura_task_select_finish(select, AURA_CHANNEL_CLOSED, index,
                              (AuraTaskChannelValue){NULL, 0, NULL});
    }
    else
    {
      frame->waiting_channel = NULL;
      frame->waiting_node = NULL;
      free(waiter);
      aura_task_channel_wake(frame);
    }
  }
  aura_task_channel_lock_leave();
  return 1;
}

int aura_task_channel_close(AuraTaskChannel *channel)
{
  return aura_task_channel_close_from(channel, NULL);
}

void aura_task_channel_destroy(AuraTaskChannel *channel)
{
  if (channel == NULL)
  {
    return;
  }
  aura_task_channel_lock_enter();
  if (channel->refs == 0)
  {
    aura_task_channel_lock_leave();
    return;
  }
  channel->refs--;
  if (channel->refs != 0)
  {
    aura_task_channel_lock_leave();
    return;
  }
  aura_task_channel_lock_leave();
  (void)aura_task_channel_close(channel);
  aura_task_channel_lock_enter();
  while (channel->count != 0)
  {
    aura_task_channel_value_destroy(&channel->values[channel->head]);
    channel->head = (channel->head + 1) % channel->capacity;
    channel->count--;
  }
  free(channel->values);
  aura_task_channel_lock_leave();
  free(channel);
}

