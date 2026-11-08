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
    connect_inner(path, None, false)
}

/// The local socket of X11 display `number`. When the pathname socket is absent
/// on Linux, the server's abstract socket of the same name is used: WSLg mounts
/// `/tmp/.X11-unix` read-only, so a private Xvfb listens only there. Callers
/// verify the peer's process identity either way.
pub(crate) fn connect_display(number: u16, deadline: Option<Instant>) -> Result<UnixStream> {
    let path = format!("/tmp/.X11-unix/X{number}");
    match connect_inner(Path::new(&path), deadline, false) {
        Err(e) if cfg!(target_os = "linux") && e.message.contains("os error 2") => {
            connect_inner(Path::new(&path), deadline, true)
        }
        other => other,
    }
}

fn address() -> libc::sockaddr_un {
    // SAFETY: this C ABI structure contains only an integer family and byte
    // array; an all-zero value is valid and performs no native operation.
    unsafe { std::mem::zeroed() }
}

/// Ordinary pathname sockets only. The native sun_path includes its terminating
/// NUL, so the whole raw absolute pathname must use fewer bytes than that array.
/// Validate before any directory creation, socket allocation or bind/connect.
pub(crate) fn validate_path(path: &Path) -> Result<()> {
    let bytes = path.as_os_str().as_bytes();
    if !path.is_absolute() || bytes.is_empty() || bytes.contains(&0) {
        return Err(Error::new(
            "invalid_endpoint",
            "Absolute NUL-free local socket pathname required",
        ));
    }
    let capacity = address().sun_path.len();
    if bytes.len() >= capacity {
        return Err(Error::new("invalid_endpoint",format!(
            "Local socket pathname uses {} bytes; maximum is {} bytes plus the terminating NUL in sun_path[{}]. Choose a shorter endpoint directory",
            bytes.len(),capacity-1,capacity
        )));
    }
    Ok(())
}

/// `abstract_name`: connect to the Linux abstract socket named by `path` (a
/// leading NUL, then the name, with the exact name length as address size).
fn connect_inner(
    path: &Path,
    absolute: Option<Instant>,
    abstract_name: bool,
) -> Result<UnixStream> {
    if absolute.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(Error::new(
            "transport_timeout",
            "Local connection deadline expired",
        ));
    }
    validate_path(path)?;
    let mut address = address();
    let bytes = path.as_os_str().as_bytes();
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let skip = usize::from(abstract_name);
    for (target, byte) in address.sun_path.iter_mut().skip(skip).zip(bytes) {
        *target = *byte as libc::c_char;
    }
    let length = if abstract_name {
        std::mem::size_of::<libc::sa_family_t>() + 1 + bytes.len()
    } else {
        std::mem::size_of_val(&address)
    };
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
            length as libc::socklen_t,
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
        let deadline = absolute.unwrap_or_else(|| Instant::now() + Duration::from_secs(2));
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
    if absolute.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(Error::new(
            "transport_timeout",
            "Local connection deadline expired",
        ));
    }
    if let Some(error) = socket.take_error()? {
        return Err(error.into());
    }
    // Writable readiness alone need not mean an asynchronous connect succeeded.
    socket.peer_addr()?;
    socket.set_nonblocking(false)?;
    Ok(socket)
}

#[cfg(test)]
mod tests;
