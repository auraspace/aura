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

/// Listen for incoming TCP connections on a port.
pub fn listen_tcp(port: u16) -> Result<RawFd, IoError> {
    let fd = create_tcp_socket()?;

    // Set SO_REUSEADDR
    let on: libc::c_int = 1;
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_REUSEADDR,
            &on as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
    }

    bind_socket_localhost(fd, port)?;

    let res = unsafe { libc::listen(fd, 128) };
    if res < 0 {
        let err = IoError::last_os_error();
        let _ = close_socket(fd);
        Err(err)
    } else {
        Ok(fd)
    }
}

/// Accept a new TCP connection.
pub fn accept_tcp(fd: RawFd) -> Result<RawFd, IoError> {
    let res = unsafe { libc::accept(fd, std::ptr::null_mut(), std::ptr::null_mut()) };
    if res < 0 {
        Err(IoError::last_os_error())
    } else {
        Ok(res)
    }
}

/// Connect to a remote TCP host.
pub fn connect_tcp(host: &str, port: u16) -> Result<RawFd, IoError> {
    let fd = create_tcp_socket()?;

    let mut addr: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    addr.sin_family = libc::AF_INET as libc::sa_family_t;
    addr.sin_port = port.to_be();

    let addr_ipv4: std::net::Ipv4Addr = host.parse().map_err(|_| {
        let _ = close_socket(fd);
        IoError::new(std::io::ErrorKind::InvalidInput, "Invalid IP address")
    })?;
    addr.sin_addr.s_addr = u32::from_be_bytes(addr_ipv4.octets()).to_be();

    let res = unsafe {
        libc::connect(
            fd,
            &addr as *const _ as *const libc::sockaddr,
            std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
        )
    };

    if res < 0 {
        let err = IoError::last_os_error();
        let _ = close_socket(fd);
        Err(err)
    } else {
        Ok(fd)
    }
}

/// Resolves a hostname to an IP address.
pub fn resolve_host(host: &str) -> Result<String, IoError> {
    use std::ffi::CString;
    use std::ptr;

    let host_cstr = CString::new(host)
        .map_err(|_| IoError::new(std::io::ErrorKind::InvalidInput, "Invalid hostname"))?;

    let mut hints: libc::addrinfo = unsafe { std::mem::zeroed() };
    hints.ai_family = libc::AF_INET;
    hints.ai_socktype = libc::SOCK_STREAM;

    let mut res: *mut libc::addrinfo = ptr::null_mut();
    let err = unsafe { libc::getaddrinfo(host_cstr.as_ptr(), ptr::null(), &hints, &mut res) };

    if err != 0 {
        return Err(IoError::new(
            std::io::ErrorKind::Other,
            format!("DNS resolution failed: {}", err),
        ));
    }

    let ip = unsafe {
        let addr_in = (*res).ai_addr as *const libc::sockaddr_in;
        let s_addr = (*addr_in).sin_addr.s_addr;
        let ip_bytes = s_addr.to_ne_bytes();
        format!(
            "{}.{}.{}.{}",
            ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]
        )
    };

    unsafe {
        libc::freeaddrinfo(res);
    }

    Ok(ip)
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
