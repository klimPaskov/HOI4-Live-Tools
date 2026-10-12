//! Ordinary keyboard fallback building block, not an enabled MCP command tool.
//! The owned launcher must supply an exclusive input lease and verified bindings;
//! the action transaction must verify the resulting game state through memory.
use super::{Identity, Process};
use crate::error::{Error, Result};
use std::{
    io,
    os::fd::AsRawFd,
    sync::Mutex,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{AtomEnum, ConnectionExt as _, MapState, KEY_PRESS_EVENT, KEY_RELEASE_EVENT},
        xtest::ConnectionExt as _,
    },
    rust_connection::{DefaultStream, PollMode, RustConnection, Stream},
    utils::RawFdContainer,
};

pub enum ControlKey {
    Pause,
    Slower,
    Faster,
}

impl ControlKey {
    fn symbol(&self) -> u32 {
        match self {
            Self::Pause => 0x20,
            Self::Slower => 0xffad,
            Self::Faster => 0xffab,
        }
    }
}
struct BoundedStream {
    stream: DefaultStream,
    deadline: Mutex<Instant>,
}
impl BoundedStream {
    fn remaining(&self) -> io::Result<Duration> {
        let remaining = self
            .deadline
            .lock()
            .map_err(|_| io::Error::other("X11 deadline poisoned"))?
            .saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "X11 operation deadline exceeded",
            ));
        }
        Ok(remaining)
    }
}
impl Stream for BoundedStream {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        loop {
            let ms = self.remaining()?.as_millis().clamp(1, 2000) as i32;
            let mut fd = libc::pollfd {
                fd: self.stream.as_raw_fd(),
                events: (if mode.readable() { libc::POLLIN } else { 0 })
                    | (if mode.writable() { libc::POLLOUT } else { 0 }),
                revents: 0,
            };
            // SAFETY: poll receives one live socket descriptor and valid pollfd.
            let n = unsafe { libc::poll(&mut fd, 1, ms) };
            if n > 0 {
                return Ok(());
            }
            if n == 0 {
                continue;
            }
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
    }
    fn read(&self, buf: &mut [u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.remaining()?;
        self.stream.read(buf, fds)
    }
    fn write(&self, buf: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.remaining()?;
        self.stream.write(buf, fds)
    }
}
fn xerror(e: impl std::fmt::Display) -> Error {
    Error::new("input_unavailable", e.to_string())
}
pub struct X11Controls {
    connection: RustConnection<BoundedStream>,
    root: u32,
    window: u32,
    game: Identity,
    class: Vec<u8>,
    pid_atom: u32,
    active_atom: u32,
}
impl X11Controls {
    /// Bind one already-focused, owned window. Never activates windows, moves
    /// the cursor, changes the map, or discovers/takes over an arbitrary game.
    pub fn open(
        process: &dyn Process,
        display: u16,
        window: u32,
        verified_class: &[u8],
    ) -> Result<Self> {
        process.check()?;
        let game = process
            .identity()
            .ok_or_else(|| {
                Error::new(
                    "input_unavailable",
                    "Owned native process identity required",
                )
            })?
            .clone();
        if window == 0 || verified_class.is_empty() || verified_class.len() > 256 {
            return Err(Error::new(
                "input_unavailable",
                "Verified owned window class required",
            ));
        }
        let socket =
            super::connect_socket(std::path::Path::new(&format!("/tmp/.X11-unix/X{display}")))?;
        let (stream, (family, address)) =
            DefaultStream::from_unix_stream(socket).map_err(xerror)?;
        let auth = x11rb_protocol::xauth::get_auth(family, &address, display)
            .map_err(xerror)?
            .ok_or_else(|| {
                Error::new(
                    "input_unavailable",
                    "Private Xauthority authentication required",
                )
            })?;
        let stream = BoundedStream {
            stream,
            deadline: Mutex::new(Instant::now() + Duration::from_secs(2)),
        };
        let connection =
            RustConnection::connect_to_stream_with_auth_info(stream, 0, auth.0, auth.1)
                .map_err(xerror)?;
        let root = connection.setup().roots[0].root;
        connection
            .xtest_get_version(2, 2)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let atom = |name: &[u8]| -> Result<u32> {
            Ok(connection
                .intern_atom(false, name)
                .map_err(xerror)?
                .reply()
                .map_err(xerror)?
                .atom)
        };
        let pid_atom = atom(b"_NET_WM_PID")?;
        let active_atom = atom(b"_NET_ACTIVE_WINDOW")?;
        let controls = Self {
            connection,
            root,
            window,
            game,
            class: verified_class.to_vec(),
            pid_atom,
            active_atom,
        };
        controls.check(process, &ControlKey::Pause)?;
        Ok(controls)
    }
    fn begin(&self) -> Result<()> {
        *self.connection.stream().deadline.lock().map_err(xerror)? =
            Instant::now() + Duration::from_secs(2);
        Ok(())
    }
    fn property(
        &self,
        window: u32,
        atom: u32,
        kind: AtomEnum,
        size: u32,
    ) -> Result<x11rb::protocol::xproto::GetPropertyReply> {
        let p = self
            .connection
            .get_property(false, window, atom, kind, 0, size)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if p.bytes_after != 0 || p.type_ != u32::from(kind) {
            return Err(Error::new(
                "input_unavailable",
                "Owned window property invalid",
            ));
        }
        Ok(p)
    }
    fn check(&self, process: &dyn Process, key: &ControlKey) -> Result<u8> {
        self.begin()?;
        process.check()?;
        if process.identity() != Some(&self.game) {
            return Err(Error::new(
                "ownership_changed",
                "Input process identity changed",
            ));
        }
        let p = self.property(self.window, self.pid_atom, AtomEnum::CARDINAL, 1)?;
        if p.format != 32 || p.value32().and_then(|mut v| v.next()) != Some(self.game.pid as u32) {
            return Err(Error::new(
                "ownership_changed",
                "Window PID is not the owned game",
            ));
        }
        let p = self.property(self.window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING, 64)?;
        if p.format != 8 || p.value != self.class {
            return Err(Error::new(
                "ownership_changed",
                "Owned window class changed",
            ));
        }
        if self
            .connection
            .get_window_attributes(self.window)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?
            .map_state
            != MapState::VIEWABLE
        {
            return Err(Error::new(
                "input_unavailable",
                "Owned window is not viewable",
            ));
        }
        let p = self.property(self.root, self.active_atom, AtomEnum::WINDOW, 1)?;
        if p.format != 32
            || p.value32().and_then(|mut v| v.next()) != Some(self.window)
            || self
                .connection
                .get_input_focus()
                .map_err(xerror)?
                .reply()
                .map_err(xerror)?
                .focus
                != self.window
        {
            return Err(Error::new(
                "input_unavailable",
                "Owned window lost focus; no activation fallback",
            ));
        }
        let setup = self.connection.setup();
        let count = setup
            .max_keycode
            .checked_sub(setup.min_keycode)
            .and_then(|v| v.checked_add(1))
            .ok_or_else(|| Error::new("input_unavailable", "Keyboard mapping bound"))?;
        let mapping = self
            .connection
            .get_keyboard_mapping(setup.min_keycode, count)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let stride = usize::from(mapping.keysyms_per_keycode);
        if stride == 0 || mapping.keysyms.len() != usize::from(count) * stride {
            return Err(Error::new(
                "input_unavailable",
                "Malformed keyboard mapping",
            ));
        }
        let matches: Vec<_> = mapping
            .keysyms
            .chunks_exact(stride)
            .enumerate()
            .filter(|(_, symbols)| symbols.contains(&key.symbol()))
            .map(|(i, _)| setup.min_keycode + i as u8)
            .collect();
        if matches.len() != 1 {
            return Err(Error::new(
                "input_unavailable",
                "Keyboard binding absent or ambiguous",
            ));
        }
        let code = matches[0];
        let keys = self
            .connection
            .query_keymap()
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?
            .keys;
        // Do not release or overwrite anybody's held key.
        if keys.iter().any(|v| *v != 0) {
            return Err(Error::new(
                "input_unavailable",
                "A keyboard key is held; input refused",
            ));
        }
        let pointer = self
            .connection
            .query_pointer(self.root)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        // Latched modifiers and buttons are refused too; modifier mappings vary.
        if u16::from(pointer.mask) != 0 {
            return Err(Error::new(
                "input_unavailable",
                "Active input modifiers/buttons are not clear",
            ));
        }
        process.check()?;
        Ok(code)
    }
    pub fn send(&self, process: &dyn Process, key: ControlKey) -> Result<()> {
        let code = self.check(process, &key)?;
        // Both events are queued before waiting for any response, preventing a
        // synchronous error check from leaving the ordinary key held down.
        let press = self
            .connection
            .xtest_fake_input(KEY_PRESS_EVENT, code, 0, self.root, 0, 0, 0)
            .map_err(xerror)?;
        let release = self
            .connection
            .xtest_fake_input(KEY_RELEASE_EVENT, code, 0, self.root, 0, 0, 0)
            .map_err(xerror)?;
        self.connection.flush().map_err(xerror)?;
        press.check().map_err(xerror)?;
        release.check().map_err(xerror)?;
        process.check()?;
        Ok(()) // Transport delivery is not a verified game action receipt.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    #[test]
    fn unresponsive_display_has_a_bounded_deadline() {
        let (socket, _peer) = UnixStream::pair().unwrap();
        let (stream, _) = DefaultStream::from_unix_stream(socket).unwrap();
        let stream = BoundedStream {
            stream,
            deadline: Mutex::new(Instant::now() + Duration::from_millis(20)),
        };
        let start = Instant::now();
        assert_eq!(
            stream.poll(PollMode::Readable).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
