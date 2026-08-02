/* Small POSIX descriptor bridge used by compiler-generated std.io frames.
 * Encode errno as a negative result so generated C can retry EAGAIN without
 * exposing a process-global error slot across suspension. */
int64_t aura_io_read_fd(int fd, void *buffer, uint64_t capacity)
{
  ssize_t result;
  if (fd < 0 || (capacity != 0 && buffer == NULL) || capacity > SIZE_MAX)
  {
    return -EINVAL;
  }
  result = read(fd, buffer, (size_t)capacity);
  if (result < 0)
  {
    return -(int64_t)errno;
  }
  return (int64_t)result;
}

/* Encode errno as a negative result so generated C can retry EAGAIN after
 * suspension without exposing a process-global error slot. */
int64_t aura_io_write_fd(int fd, const void *buffer, uint64_t length)
{
  ssize_t result;
  if (fd < 0 || (length != 0 && buffer == NULL) || length > SIZE_MAX)
  {
    return -EINVAL;
  }
  result = write(fd, buffer, (size_t)length);
  if (result < 0)
  {
    return -(int64_t)errno;
  }
  return (int64_t)result;
}

/* ---- G3 asynchronous file/TCP operation handles ----
 *
 * File and TCP adapters need a lifetime-bearing token in addition to the
 * frame's borrowed readiness registration.  The token is deliberately
 * small: the task owns its buffer and performs the bounded read/write after
 * the token is completed.  Cancellation owns the resource cleanup callback;
 * completion never invokes it, because the task still has to consume the
 * operation result.  This makes cancellation and completion mutually
 * exclusive and gives adapters one place to enforce exactly-once cleanup.
 */

struct AuraIoOperationHandle
{
  AuraTaskExecutor *executor;
  AuraTaskFrame *frame;
  AuraIoOperationKind kind;
  AuraIoOperationState state;
  int fd;
  short events;
  void *resource;
  AuraIoOperationCleanupFn cleanup;
  int cleanup_done;
  void *buffer;
  uint64_t length;
  uint64_t offset;
  int typed;
  AuraIoOperationResult result;
};

static void aura_io_operation_cleanup_once(AuraIoOperationHandle *operation)
{
  if (operation == NULL || operation->cleanup_done)
  {
    return;
  }
  operation->cleanup_done = 1;
  if (operation->cleanup != NULL && operation->resource != NULL)
  {
    operation->cleanup(operation->resource);
  }
}

static void aura_io_operation_frame_cleanup(void *data)
{
  AuraIoOperationHandle *operation = (AuraIoOperationHandle *)data;
  if (operation == NULL)
  {
    return;
  }
  if (operation->state == AURA_IO_OPERATION_PENDING)
  {
    operation->state = AURA_IO_OPERATION_CANCELLED;
    operation->result.state = AURA_IO_OPERATION_CANCELLED;
    operation->result.outcome = AURA_IO_OUTCOME_CANCELLED;
  }
  aura_io_operation_cleanup_once(operation);
  operation->frame = NULL;
  operation->executor = NULL;
}

static AuraIoOperationHandle *aura_io_operation_handle_new(
    AuraIoOperationKind kind, int fd, short events, void *resource,
    AuraIoOperationCleanupFn cleanup)
{
  AuraIoOperationHandle *operation;
  if (fd < 0 || events == 0 || resource == NULL)
  {
    return NULL;
  }
  operation = (AuraIoOperationHandle *)calloc(1, sizeof(*operation));
  if (operation == NULL)
  {
    return NULL;
  }
  operation->kind = kind;
  operation->state = AURA_IO_OPERATION_PENDING;
  operation->fd = fd;
  operation->events = events;
  operation->resource = resource;
  operation->cleanup = cleanup;
  operation->result.kind = kind;
  operation->result.state = AURA_IO_OPERATION_PENDING;
  operation->result.outcome = AURA_IO_OUTCOME_OK;
  return operation;
}

static AuraIoOperationHandle *aura_io_typed_operation_new(
    AuraIoOperationKind kind, int fd, short events, void *resource,
    void *buffer, uint64_t length, AuraIoOperationCleanupFn cleanup)
{
  AuraIoOperationHandle *operation;
  if (length > 0 && buffer == NULL)
  {
    return NULL;
  }
  operation = aura_io_operation_handle_new(kind, fd, events, resource, cleanup);
  if (operation != NULL)
  {
    operation->buffer = buffer;
    operation->length = length;
    operation->typed = 1;
  }
  return operation;
}

AuraIoOperationHandle *aura_file_async_read_handle_new(
    AuraFile *file, AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_FILE_READ, file->fd,
                                     POLLIN, file, cleanup);
}

AuraIoOperationHandle *aura_file_async_write_handle_new(
    AuraFile *file, AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_FILE_WRITE, file->fd,
                                     POLLOUT, file, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_accept_handle_new(
    AuraTcpListener *listener, AuraIoOperationCleanupFn cleanup)
{
  if (listener == NULL || listener->fd < 0)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_TCP_ACCEPT,
                                      listener->fd, POLLIN, listener, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_read_handle_new(
    AuraTcpStream *stream, AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_TCP_READ, stream->fd,
                                     POLLIN, stream, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_write_handle_new(
    AuraTcpStream *stream, AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0)
  {
    return NULL;
  }
  return aura_io_operation_handle_new(AURA_IO_OPERATION_TCP_WRITE,
                                     stream->fd, POLLOUT, stream, cleanup);
}

AuraIoOperationHandle *aura_file_async_read_operation_new(
    AuraFile *file, void *buffer, uint64_t capacity,
    AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_FILE_READ, file->fd,
                                     POLLIN, file, buffer, capacity, cleanup);
}

AuraIoOperationHandle *aura_file_async_write_operation_new(
    AuraFile *file, const void *buffer, uint64_t length,
    AuraIoOperationCleanupFn cleanup)
{
  if (file == NULL || file->closed)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_FILE_WRITE, file->fd,
                                     POLLOUT, file, (void *)buffer, length,
                                     cleanup);
}

AuraIoOperationHandle *aura_tcp_async_read_operation_new(
    AuraTcpStream *stream, void *buffer, uint64_t capacity,
    AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0 || capacity > SIZE_MAX)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_TCP_READ, stream->fd,
                                     POLLIN, stream, buffer, capacity, cleanup);
}

AuraIoOperationHandle *aura_tcp_async_write_operation_new(
    AuraTcpStream *stream, const void *buffer, uint64_t length,
    AuraIoOperationCleanupFn cleanup)
{
  if (stream == NULL || stream->fd < 0 || length > SIZE_MAX)
  {
    return NULL;
  }
  return aura_io_typed_operation_new(AURA_IO_OPERATION_TCP_WRITE, stream->fd,
                                     POLLOUT, stream, (void *)buffer, length,
                                     cleanup);
}

int aura_io_operation_handle_start(AuraIoOperationHandle *operation,
                                   AuraTaskExecutor *executor,
                                   AuraTaskFrame *frame)
{
  if (operation == NULL || executor == NULL || frame == NULL ||
      operation->state != AURA_IO_OPERATION_PENDING ||
      frame->executor != executor || operation->frame != NULL)
  {
    return 0;
  }
  if (!aura_task_frame_wait_fd(frame, operation->fd, operation->events))
  {
    return 0;
  }
  operation->executor = executor;
  operation->frame = frame;
  /* wait_fd owns the inline descriptor registration.  Replace only its
   * borrowed token; set_waiting would intentionally disable fd polling. */
  frame->waiting_node = operation;
  aura_task_frame_set_cleanup(frame, operation, aura_io_operation_frame_cleanup);
  return 1;
}

AuraIoOperationState aura_io_operation_handle_state(
    const AuraIoOperationHandle *operation)
{
  return operation != NULL ? operation->state : AURA_IO_OPERATION_FAILED;
}

AuraIoOperationKind aura_io_operation_handle_kind(
    const AuraIoOperationHandle *operation)
{
  return operation != NULL ? operation->kind : 0;
}

int aura_io_operation_handle_result(const AuraIoOperationHandle *operation,
                                    AuraIoOperationResult *out)
{
  if (operation == NULL || out == NULL ||
      operation->state == AURA_IO_OPERATION_PENDING)
  {
    return 0;
  }
  *out = operation->result;
  return 1;
}

int aura_io_operation_handle_complete(AuraIoOperationHandle *operation,
                                      int success)
{
  int already_ready;
  if (operation == NULL || operation->state != AURA_IO_OPERATION_PENDING ||
      operation->executor == NULL || operation->frame == NULL ||
      operation->executor->shutdown)
  {
    return 0;
  }
  already_ready = operation->frame->queued;
  operation->state = success ? AURA_IO_OPERATION_COMPLETE
                             : AURA_IO_OPERATION_FAILED;
  operation->result.state = operation->state;
  if (!operation->typed)
  {
    operation->result.outcome =
        success ? AURA_IO_OUTCOME_OK : AURA_IO_OUTCOME_ERROR;
  }
  aura_task_frame_clear_waiting(operation->frame);
  aura_task_frame_clear_cleanup(operation->frame);
  if (!already_ready &&
      !aura_task_executor_wake(operation->executor, operation->frame))
  {
    operation->state = AURA_IO_OPERATION_FAILED;
    return 0;
  }
  operation->frame = NULL;
  operation->executor = NULL;
  return 1;
}

static AuraIoOutcome aura_io_file_outcome(AuraFileStatus status)
{
  switch (status)
  {
  case AURA_FILE_OK:
    return AURA_IO_OUTCOME_OK;
  case AURA_FILE_EOF:
    return AURA_IO_OUTCOME_EOF;
  case AURA_FILE_CLOSED:
    return AURA_IO_OUTCOME_CLOSED;
  case AURA_FILE_PERMISSION:
    return AURA_IO_OUTCOME_PERMISSION;
  case AURA_FILE_UNSUPPORTED:
    return AURA_IO_OUTCOME_UNSUPPORTED;
  default:
    return AURA_IO_OUTCOME_ERROR;
  }
}

static AuraIoOutcome aura_io_tcp_outcome(AuraTcpStatus status)
{
  switch (status)
  {
  case AURA_TCP_OK:
    return AURA_IO_OUTCOME_OK;
  case AURA_TCP_EOF:
    return AURA_IO_OUTCOME_EOF;
  case AURA_TCP_CLOSED:
    return AURA_IO_OUTCOME_CLOSED;
  case AURA_TCP_TIMEOUT:
    return AURA_IO_OUTCOME_TIMEOUT;
  case AURA_TCP_UNSUPPORTED:
    return AURA_IO_OUTCOME_UNSUPPORTED;
  default:
    return AURA_IO_OUTCOME_ERROR;
  }
}

static int aura_io_operation_perform(AuraIoOperationHandle *operation)
{
  uint64_t file_bytes = 0;
  size_t tcp_bytes = 0;
  uint64_t remaining = operation->length;
  void *buffer = operation->buffer;
  int32_t status;

  if (operation->kind == AURA_IO_OPERATION_FILE_WRITE ||
      operation->kind == AURA_IO_OPERATION_TCP_WRITE)
  {
    if (operation->offset > operation->length)
    {
      operation->result.outcome = AURA_IO_OUTCOME_ERROR;
      operation->result.native_status = AURA_FILE_ERROR;
      return 0;
    }
    remaining = operation->length - operation->offset;
    if (buffer != NULL)
    {
      buffer = (unsigned char *)buffer + operation->offset;
    }
  }

  switch (operation->kind)
  {
  case AURA_IO_OPERATION_FILE_READ:
    status = aura_file_read((AuraFile *)operation->resource, operation->buffer,
                            operation->length, &file_bytes);
    operation->result.outcome = aura_io_file_outcome((AuraFileStatus)status);
    operation->result.bytes_transferred = file_bytes;
    break;
  case AURA_IO_OPERATION_FILE_WRITE:
    status = aura_file_write((AuraFile *)operation->resource, buffer, remaining,
                             &file_bytes);
    operation->result.outcome = aura_io_file_outcome((AuraFileStatus)status);
    operation->offset += file_bytes;
    operation->result.bytes_transferred = operation->offset;
    break;
  case AURA_IO_OPERATION_TCP_READ:
    status = aura_tcp_stream_read((AuraTcpStream *)operation->resource,
                                  operation->buffer, (size_t)operation->length,
                                  &tcp_bytes, 0);
    operation->result.outcome = aura_io_tcp_outcome((AuraTcpStatus)status);
    operation->result.bytes_transferred = (uint64_t)tcp_bytes;
    break;
  case AURA_IO_OPERATION_TCP_WRITE:
    status = aura_tcp_stream_write((AuraTcpStream *)operation->resource, buffer,
                                   (size_t)remaining,
                                   &tcp_bytes, 0);
    operation->result.outcome = aura_io_tcp_outcome((AuraTcpStatus)status);
    operation->offset += (uint64_t)tcp_bytes;
    operation->result.bytes_transferred = operation->offset;
    break;
  default:
    status = AURA_FILE_ERROR;
    operation->result.outcome = AURA_IO_OUTCOME_ERROR;
    break;
  }
  operation->result.native_status = status;
  if (status == AURA_FILE_PENDING || status == AURA_TCP_PENDING)
  {
    return 0;
  }
  if ((operation->kind == AURA_IO_OPERATION_FILE_WRITE ||
       operation->kind == AURA_IO_OPERATION_TCP_WRITE) &&
      operation->offset < operation->length &&
      operation->result.outcome == AURA_IO_OUTCOME_OK)
  {
    operation->result.native_status =
        operation->kind == AURA_IO_OPERATION_FILE_WRITE ? AURA_FILE_PENDING
                                                        : AURA_TCP_PENDING;
    return 0;
  }
  return operation->result.outcome == AURA_IO_OUTCOME_OK ||
         operation->result.outcome == AURA_IO_OUTCOME_EOF;
}

static int aura_io_operation_ready(AuraTaskFrame *frame, short revents)
{
  AuraIoOperationHandle *operation;

  if (frame == NULL || frame->waiting_node == NULL ||
      frame->waiting_node == &frame->fd_wait_active)
  {
    return 0;
  }
  operation = (AuraIoOperationHandle *)frame->waiting_node;
  if (operation->frame != frame || operation->executor != frame->executor ||
      operation->state != AURA_IO_OPERATION_PENDING)
  {
    return 0;
  }
  if (operation->typed && (revents & POLLNVAL) == 0)
  {
    int success = aura_io_operation_perform(operation);
    if (operation->result.native_status == AURA_FILE_PENDING ||
        operation->result.native_status == AURA_TCP_PENDING)
    {
      return -1;
    }
    return aura_io_operation_handle_complete(operation, success);
  }
  /* POLLNVAL is a descriptor failure.  POLLERR/POLLHUP still wake the task so
   * its bounded read/write/accept call can publish EOF or the native error. */
  return aura_io_operation_handle_complete(operation,
                                           (revents & POLLNVAL) == 0);
}

int aura_io_operation_handle_cancel(AuraIoOperationHandle *operation)
{
  AuraTaskExecutor *executor;
  AuraTaskFrame *frame;
  if (operation == NULL || operation->state != AURA_IO_OPERATION_PENDING)
  {
    return 0;
  }
  executor = operation->executor;
  frame = operation->frame;
  operation->state = AURA_IO_OPERATION_CANCELLED;
  operation->result.state = AURA_IO_OPERATION_CANCELLED;
  operation->result.outcome = AURA_IO_OUTCOME_CANCELLED;
  if (frame != NULL)
  {
    aura_task_frame_clear_waiting(frame);
  }
  aura_io_operation_cleanup_once(operation);
  if (executor == NULL || frame == NULL)
  {
    return 1;
  }
  return aura_task_executor_cancel(executor, frame);
}

int aura_io_operation_handle_release(AuraIoOperationHandle **handle)
{
  AuraIoOperationHandle *operation;
  if (handle == NULL || *handle == NULL)
  {
    return 1;
  }
  operation = *handle;
  if (operation->state == AURA_IO_OPERATION_PENDING ||
      operation->frame != NULL)
  {
    return 0;
  }
  *handle = NULL;
  free(operation);
  return 1;
}

AuraFfiStatus aura_io_operation_handle_check_boundary(
    const AuraIoOperationHandle *operation, AuraFfiBoundary boundary)
{
  if (operation == NULL || operation->state != AURA_IO_OPERATION_PENDING)
  {
    return AURA_FFI_INVALID;
  }
  if (boundary == AURA_FFI_BOUNDARY_SYNC)
  {
    return operation->frame == NULL ? AURA_FFI_OK : AURA_FFI_BOUNDARY_REJECTED;
  }
  if (boundary == AURA_FFI_BOUNDARY_TASK)
  {
    return operation->frame != NULL ? AURA_FFI_OK
                                    : AURA_FFI_BOUNDARY_REJECTED;
  }
  return AURA_FFI_BOUNDARY_REJECTED;
}

