static int aura_ffi_array_shape_ok(uint64_t len, uint64_t cap,
                                   uint64_t elem_size, AuraFfiArrayKind kind)
{
  if (len > cap || (cap != 0 && elem_size == 0))
  {
    return 0;
  }
  if (kind == AURA_FFI_ARRAY_BYTES)
  {
    return elem_size == 1;
  }
  if (kind == AURA_FFI_ARRAY_INT64)
  {
    return elem_size == sizeof(int64_t);
  }
  if (kind == AURA_FFI_ARRAY_BOOL)
  {
    return elem_size == sizeof(uint8_t);
  }
  return 0;
}

AuraFfiStatus aura_ffi_string_borrow(const char *data, uint64_t len,
                                     AuraFfiStringView *out)
{
  if (out == NULL || (data == NULL && len != 0))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_string_copy(AuraFfiStringView view, AuraFfiString *out)
{
  if (out == NULL || (view.data == NULL && view.len != 0) ||
      view.len > (uint64_t)(SIZE_MAX - 1u))
  {
    return AURA_FFI_INVALID;
  }
  char *copy = (char *)malloc((size_t)view.len + 1u);
  if (copy == NULL)
  {
    return AURA_FFI_OOM;
  }
  if (view.len != 0)
  {
    memcpy(copy, view.data, (size_t)view.len);
  }
  copy[view.len] = '\0';
  out->data = copy;
  out->len = view.len;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_string_transfer(char *data, uint64_t len,
                                       AuraFfiString *out)
{
  if (out == NULL || (data == NULL && len != 0))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  return AURA_FFI_OK;
}

void aura_ffi_string_destroy(AuraFfiString *value)
{
  if (value == NULL)
  {
    return;
  }
  free(value->data);
  value->data = NULL;
  value->len = 0;
}

AuraFfiStatus aura_ffi_array_borrow(const void *data, uint64_t len,
                                    uint64_t cap, uint64_t elem_size,
                                    AuraFfiArrayKind kind,
                                    AuraFfiArrayView *out)
{
  if (out == NULL || (data == NULL && len != 0) ||
      !aura_ffi_array_shape_ok(len, cap, elem_size, kind))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  out->cap = cap;
  out->elem_size = elem_size;
  out->kind = kind;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_array_copy(AuraFfiArrayView view, AuraFfiArray *out)
{
  if (out == NULL || (view.data == NULL && view.len != 0) ||
      !aura_ffi_array_shape_ok(view.len, view.len, view.elem_size, view.kind) ||
      (view.elem_size != 0 && view.len > (uint64_t)(SIZE_MAX / view.elem_size)))
  {
    return AURA_FFI_INVALID;
  }
  size_t bytes = (size_t)view.len * (size_t)view.elem_size;
  void *copy = bytes == 0 ? NULL : malloc(bytes);
  if (bytes != 0 && copy == NULL)
  {
    return AURA_FFI_OOM;
  }
  if (bytes != 0)
  {
    memcpy(copy, view.data, bytes);
  }
  out->data = copy;
  out->len = view.len;
  out->cap = view.len;
  out->elem_size = view.elem_size;
  out->kind = view.kind;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_array_transfer(void *data, uint64_t len, uint64_t cap,
                                      uint64_t elem_size, AuraFfiArrayKind kind,
                                      AuraFfiArray *out)
{
  if (out == NULL || (data == NULL && len != 0) ||
      !aura_ffi_array_shape_ok(len, cap, elem_size, kind))
  {
    return AURA_FFI_INVALID;
  }
  out->data = data;
  out->len = len;
  out->cap = cap;
  out->elem_size = elem_size;
  out->kind = kind;
  return AURA_FFI_OK;
}

void aura_ffi_array_destroy(AuraFfiArray *value)
{
  if (value == NULL)
  {
    return;
  }
  free(value->data);
  value->data = NULL;
  value->len = 0;
  value->cap = 0;
  value->elem_size = 0;
  value->kind = 0;
}

AuraFfiStatus aura_ffi_root_begin(AuraFfiRootGuard *guard, void **slot)
{
  if (guard == NULL || slot == NULL || guard->active)
  {
    return AURA_FFI_INVALID;
  }
  aura_gc_add_root(slot);
  guard->slot = slot;
  guard->active = 1;
  return AURA_FFI_OK;
}

void aura_ffi_root_end(AuraFfiRootGuard *guard)
{
  if (guard == NULL || !guard->active)
  {
    return;
  }
  aura_gc_remove_root(guard->slot);
  guard->slot = NULL;
  guard->active = 0;
}

/* ---- F4 opaque foreign-resource handle ABI ---- */

struct AuraFfiOpaqueHandle
{
  void *resource;
  AuraFfiHandleDestroyFn destroy;
  uint64_t generation;
  size_t pins;
  uint32_t owners;
  int nullable;
  int released;
  int destroyed;
};

static void aura_ffi_handle_finish(AuraFfiOpaqueHandle *handle)
{
  if (handle == NULL || !handle->released || handle->pins != 0 ||
      handle->destroyed)
  {
    return;
  }
  void *resource = handle->resource;
  handle->resource = NULL;
  handle->destroyed = 1;
  if (handle->destroy != NULL && resource != NULL)
  {
    handle->destroy(resource);
  }
}

static void aura_ffi_handle_free_if_unowned(AuraFfiOpaqueHandle *handle)
{
  if (handle != NULL && handle->released && handle->pins == 0 &&
      handle->owners == 0)
  {
    free(handle);
  }
}

static AuraFfiStatus aura_ffi_handle_new_impl(void *resource,
                                               AuraFfiHandleDestroyFn destroy,
                                               int nullable,
                                               AuraFfiOpaqueHandle **out)
{
  if (out == NULL || (!nullable && resource == NULL))
  {
    return AURA_FFI_INVALID;
  }
  *out = NULL;
  AuraFfiOpaqueHandle *handle =
      (AuraFfiOpaqueHandle *)calloc(1, sizeof(*handle));
  if (handle == NULL)
  {
    return AURA_FFI_OOM;
  }
  handle->resource = resource;
  handle->destroy = destroy;
  handle->generation = 1;
  handle->owners = 1;
  handle->nullable = nullable;
  *out = handle;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_new(void *resource,
                                  AuraFfiHandleDestroyFn destroy,
                                  AuraFfiOpaqueHandle **out)
{
  return aura_ffi_handle_new_impl(resource, destroy, 0, out);
}

AuraFfiStatus aura_ffi_handle_new_nullable(void *resource,
                                            AuraFfiHandleDestroyFn destroy,
                                            AuraFfiOpaqueHandle **out)
{
  return aura_ffi_handle_new_impl(resource, destroy, 1, out);
}

int aura_ffi_handle_is_null(const AuraFfiOpaqueHandle *handle)
{
  return handle == NULL || handle->released || handle->resource == NULL;
}

AuraFfiStatus aura_ffi_handle_pin(AuraFfiOpaqueHandle *handle,
                                  AuraFfiHandlePin *out)
{
  if (out == NULL)
  {
    return AURA_FFI_INVALID;
  }
  memset(out, 0, sizeof(*out));
  if (handle == NULL || handle->released || handle->resource == NULL ||
      handle->destroyed)
  {
    return AURA_FFI_INVALID;
  }
  handle->pins++;
  out->handle = handle;
  out->resource = handle->resource;
  out->generation = handle->generation;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_pin_for_boundary(AuraFfiOpaqueHandle *handle,
                                               AuraFfiBoundary boundary,
                                               AuraFfiHandlePin *out)
{
  if (boundary != AURA_FFI_BOUNDARY_SYNC &&
      boundary != AURA_FFI_BOUNDARY_TASK &&
      boundary != AURA_FFI_BOUNDARY_AWAIT)
  {
    if (out != NULL)
    {
      memset(out, 0, sizeof(*out));
    }
    return AURA_FFI_BOUNDARY_REJECTED;
  }
  return aura_ffi_handle_pin(handle, out);
}

AuraFfiStatus aura_ffi_handle_drop(AuraFfiOpaqueHandle **handle);

AuraFfiStatus aura_ffi_handle_retain(AuraFfiOpaqueHandle *handle)
{
  if (handle == NULL || handle->released || handle->destroyed ||
      handle->owners == UINT32_MAX)
  {
    return AURA_FFI_INVALID;
  }
  handle->owners++;
  return AURA_FFI_OK;
}

void aura_destroy_foreign_handle_payload(void *payload)
{
  AuraFfiOpaqueHandle **handle = (AuraFfiOpaqueHandle **)payload;
  if (handle != NULL)
  {
    if (*handle != NULL)
    {
      (void)aura_ffi_handle_drop(handle);
    }
    free(handle);
  }
}

AuraFfiStatus aura_ffi_handle_pin_resource(const AuraFfiHandlePin *pin,
                                           void **out_resource)
{
  if (out_resource == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = NULL;
  if (pin == NULL || pin->handle == NULL || pin->resource == NULL ||
      pin->handle->released || pin->handle->destroyed ||
      pin->handle->generation != pin->generation ||
      pin->handle->resource != pin->resource)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = pin->resource;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_unpin(AuraFfiHandlePin *pin)
{
  if (pin == NULL || pin->handle == NULL || pin->resource == NULL ||
      pin->generation != pin->handle->generation || pin->handle->pins == 0)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiOpaqueHandle *handle = pin->handle;
  handle->pins--;
  memset(pin, 0, sizeof(*pin));
  aura_ffi_handle_finish(handle);
  aura_ffi_handle_free_if_unowned(handle);
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_release(AuraFfiOpaqueHandle *handle)
{
  if (handle == NULL || handle->released || handle->destroyed)
  {
    return AURA_FFI_INVALID;
  }
  handle->released = 1;
  aura_ffi_handle_finish(handle);
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_invalidate(AuraFfiOpaqueHandle *handle)
{
  return aura_ffi_handle_release(handle);
}

AuraFfiStatus aura_ffi_handle_destroy(AuraFfiOpaqueHandle **handle)
{
  if (handle == NULL || *handle == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiOpaqueHandle *value = *handle;
  if (!value->released)
  {
    return AURA_FFI_INVALID;
  }
  if (value->pins != 0)
  {
    return AURA_FFI_BUSY;
  }
  if (value->owners == 0)
  {
    return AURA_FFI_INVALID;
  }
  value->owners--;
  if (value->owners == 0)
  {
    aura_ffi_handle_finish(value);
    free(value);
  }
  *handle = NULL;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_handle_drop(AuraFfiOpaqueHandle **handle)
{
  if (handle == NULL || *handle == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiOpaqueHandle *value = *handle;
  if (value->owners == 0)
  {
    return AURA_FFI_INVALID;
  }
  value->owners--;
  if (value->owners == 0)
  {
    value->released = 1;
    aura_ffi_handle_finish(value);
    /* A task pin may outlive the lexical owner; unpin performs the final free. */
    aura_ffi_handle_free_if_unowned(value);
  }
  *handle = NULL;
  return AURA_FFI_OK;
}

/* Consume the sole owner of a handle without running its destructor.  This is
 * used only to move an accepted stream into its owning HTTP connection. */
AuraFfiStatus aura_ffi_handle_take_owned(AuraFfiOpaqueHandle **handle,
                                         void **out_resource)
{
  AuraFfiOpaqueHandle *value;
  if (handle == NULL || *handle == NULL || out_resource == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = NULL;
  value = *handle;
  if (value->released || value->destroyed || value->resource == NULL ||
      value->pins != 0 || value->owners != 1)
  {
    return AURA_FFI_INVALID;
  }
  *out_resource = value->resource;
  value->resource = NULL;
  value->destroyed = 1;
  return aura_ffi_handle_drop(handle);
}

AuraFfiStatus aura_ffi_handle_check_boundary(const AuraFfiOpaqueHandle *handle,
                                             AuraFfiBoundary boundary)
{
  if (handle == NULL || handle->released || handle->destroyed)
  {
    return AURA_FFI_INVALID;
  }
  /* Nullable handles may cross a synchronous call as an explicit null value,
   * but task/await pinning requires a live resource to retain. */
  if (handle->resource == NULL &&
      (boundary == AURA_FFI_BOUNDARY_TASK ||
       boundary == AURA_FFI_BOUNDARY_AWAIT))
  {
    return AURA_FFI_INVALID;
  }
  return boundary == AURA_FFI_BOUNDARY_SYNC ||
                 boundary == AURA_FFI_BOUNDARY_TASK ||
                 boundary == AURA_FFI_BOUNDARY_AWAIT
             ? AURA_FFI_OK
             : AURA_FFI_BOUNDARY_REJECTED;
}

/* ---- F5 bounded callback and foreign-outcome ABI ---- */

struct AuraFfiCallbackFrame
{
  uint64_t owner_task;
  size_t registrations;
  int valid;
};

struct AuraFfiCallback
{
  AuraFfiCallbackFrame *frame;
  AuraFfiCallbackFn callback;
  void *environment;
  AuraFfiCallbackEnvDestroyFn environment_destroy;
  int registered;
  int dispatching;
};

AuraFfiOutcome aura_ffi_map_error(int32_t foreign_code)
{
  switch (foreign_code)
  {
  case 0:
    return AURA_FFI_OUTCOME_OK;
  case 1:
    return AURA_FFI_OUTCOME_CANCELLED;
  case 2:
    return AURA_FFI_OUTCOME_INVALID;
  case 3:
    return AURA_FFI_OUTCOME_NOT_FOUND;
  case 4:
    return AURA_FFI_OUTCOME_PERMISSION;
  case 5:
    return AURA_FFI_OUTCOME_UNAVAILABLE;
  case 6:
    return AURA_FFI_OUTCOME_TIMEOUT;
  default:
    return AURA_FFI_OUTCOME_FOREIGN_ERROR;
  }
}

AuraFfiStatus aura_ffi_callback_frame_new(uint64_t owner_task,
                                          AuraFfiCallbackFrame **out)
{
  if (out == NULL || owner_task == 0)
  {
    return AURA_FFI_INVALID;
  }
  *out = NULL;
  AuraFfiCallbackFrame *frame =
      (AuraFfiCallbackFrame *)calloc(1, sizeof(*frame));
  if (frame == NULL)
  {
    return AURA_FFI_OOM;
  }
  frame->owner_task = owner_task;
  frame->valid = 1;
  *out = frame;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_frame_invalidate(AuraFfiCallbackFrame *frame)
{
  if (frame == NULL || !frame->valid)
  {
    return AURA_FFI_INVALID;
  }
  frame->valid = 0;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_frame_destroy(AuraFfiCallbackFrame **frame)
{
  if (frame == NULL || *frame == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiCallbackFrame *value = *frame;
  if (value->registrations != 0)
  {
    return AURA_FFI_BUSY;
  }
  free(value);
  *frame = NULL;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_register(
    AuraFfiCallbackFrame *frame, AuraFfiCallbackFn callback, void *environment,
    AuraFfiCallbackEnvDestroyFn environment_destroy, AuraFfiCallback **out)
{
  if (out == NULL || frame == NULL || !frame->valid || callback == NULL ||
      environment == NULL || environment_destroy == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *out = NULL;
  AuraFfiCallback *registration =
      (AuraFfiCallback *)calloc(1, sizeof(*registration));
  if (registration == NULL)
  {
    return AURA_FFI_OOM;
  }
  registration->frame = frame;
  registration->callback = callback;
  registration->environment = environment;
  registration->environment_destroy = environment_destroy;
  registration->registered = 1;
  frame->registrations++;
  *out = registration;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_invoke(AuraFfiCallback *registration,
                                       uint64_t current_task,
                                       AuraFfiBoundary boundary,
                                       const void *payload,
                                       uint64_t payload_len,
                                       AuraFfiOutcome *outcome)
{
  if (outcome == NULL)
  {
    return AURA_FFI_INVALID;
  }
  *outcome = AURA_FFI_OUTCOME_FOREIGN_ERROR;
  if (registration == NULL || !registration->registered ||
      registration->frame == NULL || !registration->frame->valid ||
      registration->callback == NULL ||
      (payload == NULL && payload_len != 0))
  {
    return AURA_FFI_INVALID;
  }
  if (boundary != AURA_FFI_BOUNDARY_SYNC ||
      current_task != registration->frame->owner_task)
  {
    return AURA_FFI_BOUNDARY_REJECTED;
  }
  if (registration->dispatching)
  {
    return AURA_FFI_BUSY;
  }
  registration->dispatching = 1;
  int32_t foreign_code = registration->callback(
      registration->environment, payload, payload_len);
  registration->dispatching = 0;
  *outcome = aura_ffi_map_error(foreign_code);
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_invoke_owned(
    AuraFfiCallback *registration, uint64_t current_task,
    AuraFfiBoundary boundary, const void *payload, uint64_t payload_len,
    AuraFfiPayloadCloneFn clone, AuraFfiPayloadDestroyFn destroy,
    AuraFfiOwnedPayload *owned_payload, AuraFfiOutcome *outcome)
{
  if (owned_payload == NULL || outcome == NULL || clone == NULL ||
      destroy == NULL)
  {
    return AURA_FFI_INVALID;
  }
  owned_payload->data = NULL;
  owned_payload->len = 0;
  owned_payload->destroy = NULL;
  *outcome = AURA_FFI_OUTCOME_FOREIGN_ERROR;
  if (registration == NULL || !registration->registered ||
      registration->frame == NULL || !registration->frame->valid ||
      registration->callback == NULL ||
      (payload == NULL && payload_len != 0))
  {
    return AURA_FFI_INVALID;
  }
  if (boundary != AURA_FFI_BOUNDARY_SYNC ||
      current_task != registration->frame->owner_task)
  {
    return AURA_FFI_BOUNDARY_REJECTED;
  }
  if (registration->dispatching)
  {
    return AURA_FFI_BUSY;
  }

  uint64_t cloned_len = 0;
  void *copy = clone(payload, payload_len, &cloned_len);
  if (copy == NULL && cloned_len != 0)
  {
    return AURA_FFI_OOM;
  }
  if (cloned_len > AURA_FFI_MAX_OWNED_CALLBACK_PAYLOAD)
  {
    destroy(copy, cloned_len);
    return AURA_FFI_INVALID;
  }
  registration->dispatching = 1;
  int32_t foreign_code =
      registration->callback(registration->environment, copy, cloned_len);
  registration->dispatching = 0;
  *outcome = aura_ffi_map_error(foreign_code);
  if (*outcome != AURA_FFI_OUTCOME_OK)
  {
    destroy(copy, cloned_len);
    return AURA_FFI_OK;
  }
  owned_payload->data = copy;
  owned_payload->len = cloned_len;
  owned_payload->destroy = destroy;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_owned_payload_destroy(AuraFfiOwnedPayload *payload)
{
  if (payload == NULL)
  {
    return AURA_FFI_INVALID;
  }
  if (payload->data != NULL)
  {
    if (payload->destroy == NULL)
    {
      return AURA_FFI_INVALID;
    }
    payload->destroy(payload->data, payload->len);
  }
  payload->data = NULL;
  payload->len = 0;
  payload->destroy = NULL;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_deregister(AuraFfiCallback *registration)
{
  if (registration == NULL || !registration->registered)
  {
    return AURA_FFI_INVALID;
  }
  if (registration->dispatching)
  {
    return AURA_FFI_BUSY;
  }
  registration->registered = 0;
  if (registration->environment_destroy != NULL &&
      registration->environment != NULL)
  {
    registration->environment_destroy(registration->environment);
  }
  registration->environment = NULL;
  registration->environment_destroy = NULL;
  if (registration->frame != NULL && registration->frame->registrations != 0)
  {
    registration->frame->registrations--;
  }
  registration->frame = NULL;
  return AURA_FFI_OK;
}

AuraFfiStatus aura_ffi_callback_shutdown(AuraFfiCallback *registration)
{
  return aura_ffi_callback_deregister(registration);
}

AuraFfiStatus aura_ffi_callback_destroy(AuraFfiCallback **registration)
{
  if (registration == NULL || *registration == NULL)
  {
    return AURA_FFI_INVALID;
  }
  AuraFfiCallback *value = *registration;
  if (value->registered || value->dispatching)
  {
    return AURA_FFI_BUSY;
  }
  free(value);
  *registration = NULL;
  return AURA_FFI_OK;
}

