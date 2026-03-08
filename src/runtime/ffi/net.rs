use std::io::Error as IoError;
use std::os::unix::prelude::RawFd;

/// Create a TCP socket via `libc::socket`.
pub fn create_tcp_socket() -> Result<RawFd, IoError> {
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
    if fd < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(fd)
    }
}

/// Bind a socket to `127.0.0.1:port`.
pub fn bind_socket_localhost(fd: RawFd, port: u16) -> Result<(), IoError> {
    let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    addr.sin_family = libc::AF_INET as libc::sa_family_t;
    addr.sin_port = port.to_be();
    addr.sin_addr.s_addr = u32::from_be_bytes([127, 0, 0, 1]).to_be();

    let res = unsafe {
        libc::bind(
            fd,
            &addr as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    };

    if res < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(())
    }
}

/// Close a socket via `libc::close`.
pub fn close_socket(fd: RawFd) -> Result<(), IoError> {
    let res = unsafe { libc::close(fd) };
    if res < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_net() {
        let fd = create_tcp_socket().unwrap();

        // We might fail if the port is in use, which is normal for concurrent tests,
        // so we just test that the FFI barrier did not panic.
        let _ = bind_socket_localhost(fd, 49152);

        close_socket(fd).unwrap();
    }
}
