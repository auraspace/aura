/* ---- File I/O (std.io) ----
 * Text errors throw String messages (single-threaded; static errbuf).
 * Strings are UTF-8 byte sequences; binary with embedded NUL is not supported
 * for the String path (matches the rest of the String surface).
 */

#define AURA_IO_MAX_FILE ((int64_t)256 * 1024 * 1024)

static char aura_io_errbuf[1024];

/* ---- Bounded file handle I/O ----
 * Regular files do not provide portable readiness notifications: POSIX
 * O_NONBLOCK is ignored for them. Keep this API explicit about that fact.
 * Each operation owns no caller buffer beyond the call and performs at most
 * one read/write syscall, so an eventual async scheduler can resume around
 * this boundary without changing ownership semantics. */
struct AuraFile
{
  AuraPlatformFile fd;
  bool closed;
};

static char aura_file_errbuf[256] = "no error";

const char *aura_file_last_error(void)
{
  return aura_file_errbuf;
}

static AuraFileStatus aura_file_status_for_errno(int error)
{
  if (error == EACCES || error == EPERM || error == EROFS)
  {
    return AURA_FILE_PERMISSION;
  }
  if (error == EAGAIN || error == EWOULDBLOCK)
  {
    return AURA_FILE_PENDING;
  }
  return AURA_FILE_ERROR;
}

static AuraFileStatus aura_file_error(const char *op, int error)
{
  if (error == 0)
  {
    error = EIO;
  }
  snprintf(aura_file_errbuf, sizeof(aura_file_errbuf), "file %s failed: %s",
           op ? op : "operation", strerror(error));
  return aura_file_status_for_errno(error);
}

AuraFileStatus aura_file_open(const char *path, AuraFileMode mode, AuraFile **out)
{
  if (out == NULL)
  {
    return aura_file_error("open", EINVAL);
  }
  *out = NULL;
  if (path == NULL || path[0] == '\0')
  {
    return aura_file_error("open", EINVAL);
  }
  int flags = 0;
  switch (mode)
  {
    case AURA_FILE_READ: flags = 0; break;
    case AURA_FILE_WRITE: flags = 1; break;
    case AURA_FILE_READ_WRITE: flags = 2; break;
    case AURA_FILE_APPEND: flags = 3; break;
    default: return aura_file_error("open", EINVAL);
  }
  AuraPlatformFile fd = aura_platform_file_open(path, flags);
  if (fd == AURA_PLATFORM_FILE_INVALID)
  {
    return aura_file_error("open", errno);
  }
  AuraFile *file = (AuraFile *)calloc(1, sizeof(*file));
  if (file == NULL)
  {
    int error = errno ? errno : ENOMEM;
    aura_platform_file_close(fd);
    return aura_file_error("open", error);
  }
  file->fd = fd;
  *out = file;
  return AURA_FILE_OK;
}

AuraFileStatus aura_file_read(AuraFile *file, void *buffer, uint64_t capacity,
                              uint64_t *out_read)
{
  if (out_read != NULL) *out_read = 0;
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  if (out_read == NULL || (capacity > 0 && buffer == NULL))
    return aura_file_error("read", EINVAL);
  int64_t result = aura_platform_file_read(file->fd, buffer, (size_t)capacity);
  if (result > 0)
  {
    *out_read = (uint64_t)result;
    return AURA_FILE_OK;
  }
  if (result == 0) return AURA_FILE_EOF;
  return aura_file_error("read", errno);
}

AuraFileStatus aura_file_write(AuraFile *file, const void *buffer,
                               uint64_t length, uint64_t *out_written)
{
  if (out_written != NULL) *out_written = 0;
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  if (out_written == NULL || (length > 0 && buffer == NULL))
    return aura_file_error("write", EINVAL);
  int64_t result = aura_platform_file_write(file->fd, buffer, (size_t)length);
  if (result >= 0)
  {
    *out_written = (uint64_t)result;
    return result == 0 && length > 0 ? AURA_FILE_PENDING : AURA_FILE_OK;
  }
  return aura_file_error("write", errno);
}

AuraFileStatus aura_file_flush(AuraFile *file)
{
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  return aura_platform_file_flush(file->fd) == 0 ? AURA_FILE_OK : aura_file_error("flush", errno);
}

AuraFileStatus aura_file_close(AuraFile *file)
{
  if (file == NULL || file->closed) return AURA_FILE_CLOSED;
  file->closed = true;
  if (aura_platform_file_close(file->fd) != 0) return aura_file_error("close", errno);
  return AURA_FILE_OK;
}

AuraFileStatus aura_file_destroy(AuraFile **file)
{
  if (file == NULL || *file == NULL) return AURA_FILE_CLOSED;
  AuraFileStatus status = aura_file_close(*file);
  free(*file);
  *file = NULL;
  return status == AURA_FILE_CLOSED ? AURA_FILE_OK : status;
}
