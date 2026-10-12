//! Bounded local connection establishment; failed connects never retry requests.
use crate::error::{Error, Result};
use std::{
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixStream},
    },
    path::Path,
    time::{Duration, Instant},
};

pub(crate) fn connect(path: &Path) -> Result<UnixStream> {
    // SAFETY: the complete address is zero initialized before filling the path.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    let bytes = path.as_os_str().as_bytes();
    if !path.is_absolute()
        || bytes.is_empty()
        || bytes.len() >= address.sun_path.len()
        || bytes.contains(&0)
    {
        return Err(Error::new("invalid_endpoint", "Invalid local socket path"));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (target, byte) in address.sun_path.iter_mut().zip(bytes) {
        *target = *byte as libc::c_char;
    }
    // SAFETY: new local socket, atomically nonblocking and close-on-exec.
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: socket returned a new descriptor owned only by this function.
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    // SAFETY: initialized sockaddr_un of the exact supplied size; no borrowed
    // descriptor is closed or modified. An unsuccessful path has no game input.
    let result = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as libc::socklen_t,
        )
    };
    if result < 0 {
        let error = std::io::Error::last_os_error();
        // AF_UNIX backlog exhaustion may return EAGAIN directly. Refuse it;
        // retrying connect or a later accepted request is not implicit policy.
        if error.raw_os_error() == Some(libc::EAGAIN) {
            return Err(Error::new("endpoint_busy", "Local socket backlog is full"));
        }
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error.into());
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::new(
                    "transport_timeout",
                    "Local connection timed out",
                ));
            }
            let mut poll = libc::pollfd {
                fd: fd.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            // SAFETY: one owned descriptor and initialized live poll buffer.
            let result =
                unsafe { libc::poll(&mut poll, 1, remaining.as_millis().clamp(1, 2000) as i32) };
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error.into());
            }
            if result > 0 {
                break;
            }
        }
    }
    let socket = UnixStream::from(fd);
    if let Some(error) = socket.take_error()? {
        return Err(error.into());
    }
    // Writable readiness alone need not mean an asynchronous connect succeeded.
    socket.peer_addr()?;
    socket.set_nonblocking(false)?;
    Ok(socket)
}
