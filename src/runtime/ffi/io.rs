use std::ffi::CString;
use std::io::Error as IoError;
use std::os::unix::prelude::RawFd;

/// Open a file via `libc::open`.
///
/// # Safety
/// This function performs an unsafe call to `libc::open`. The caller must ensure
/// `path` is a valid null-terminated C string.
pub fn open_file(path: &str, flags: libc::c_int, mode: libc::mode_t) -> Result<RawFd, IoError> {
    let c_path = CString::new(path).map_err(|_| IoError::from_raw_os_error(libc::EINVAL))?;
    let fd = unsafe { libc::open(c_path.as_ptr(), flags, mode as libc::c_int) };
    if fd < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(fd)
    }
}

/// Close a file descriptor via `libc::close`.
pub fn close_file(fd: RawFd) -> Result<(), IoError> {
    let res = unsafe { libc::close(fd) };
    if res < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(())
    }
}

/// Read from a file descriptor into a buffer via `libc::read`.
pub fn read_file(fd: RawFd, buf: &mut [u8]) -> Result<usize, IoError> {
    let res = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
    if res < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(res as usize)
    }
}

/// Write a buffer to a file descriptor via `libc::write`.
pub fn write_file(fd: RawFd, buf: &[u8]) -> Result<usize, IoError> {
    let res = unsafe { libc::write(fd, buf.as_ptr() as *const libc::c_void, buf.len()) };
    if res < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(res as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_ffi_io() {
        let path = "/tmp/aura_ffi_test.txt";

        let fd = open_file(path, libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC, 0o644).unwrap();
        let msg = b"Hello from Aura FFI!";
        let written = write_file(fd, msg).unwrap();
        assert_eq!(written, msg.len());
        close_file(fd).unwrap();

        let fd = open_file(path, libc::O_RDONLY, 0).unwrap();
        let mut buf = [0u8; 32];
        let read = read_file(fd, &mut buf).unwrap();
        assert_eq!(&buf[..read], b"Hello from Aura FFI!");
        close_file(fd).unwrap();

        fs::remove_file(path).unwrap();
    }
}
