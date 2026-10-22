//! Ordinary keyboard fallback building block, not an enabled MCP command tool.
//! The owned launcher must supply an exclusive input lease and verified bindings;
//! the action transaction must verify the resulting game state through memory.
use super::{xauthority, Identity, Process};
use crate::error::{Error, Result};
use std::{
    io,
    os::fd::AsRawFd,
    path::Path,
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

mod pointer;
mod text;
pub use pointer::{LeftClick, PointerDelivery, PointerDispatch, PointerDisplay};

pub enum ControlKey {
    /// The installed player-flag shortcut opens politics, never direct focus.
    PoliticsPanel,
    ResearchPanel,
    Pause,
    Slower,
    Faster,
    /// The game's own `open_find_view_button` shortcut ("f" in mapmodes_interface.gui).
    FindView,
    /// One Return; the Find view activates its first result row.
    Return,
}

impl ControlKey {
    fn symbol(&self) -> u32 {
        match self {
            Self::PoliticsPanel => 0x71,
            Self::ResearchPanel => 0x77,
            Self::Pause => 0x20,
            Self::Slower => 0xffad,
            Self::Faster => 0xffab,
            Self::FindView => 0x66,
            Self::Return => 0xff0d,
        }
    }
}
pub(super) struct BoundedStream {
    stream: DefaultStream,
    deadline: Mutex<Instant>,
    server: Identity,
}
impl BoundedStream {
    pub(super) fn verify_peer(&self) -> Result<()> {
        verify_display_peer(&self.stream, &self.server)
    }
    pub(super) fn remaining(&self) -> io::Result<Duration> {
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
        // Check the actual connected peer for every transport write, including
        // authorization setup, queued requests and partial-write retries.
        self.verify_peer()
            .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
        // A fresh /proc peer check may itself consume the remaining budget.
        self.remaining()?;
        self.stream.write(buf, fds)
    }
}
fn verify_display_server(expected: &Identity) -> Result<()> {
    // SAFETY: a side-effect-free real UID query; privileged display peers are
    // never accepted by this ordinary-user private-display adapter.
    let uid = unsafe { libc::getuid() };
    if uid == 0 || expected.uid != uid || expected.pid <= 0 {
        return Err(Error::new(
            "ownership_changed",
            "Configured display must belong to the same ordinary user",
        ));
    }
    if super::capture_identity(expected.pid)? != *expected {
        return Err(Error::new(
            "ownership_changed",
            "Configured display server identity changed",
        ));
    }
    Ok(())
}
fn verify_display_peer(socket: &impl AsRawFd, expected: &Identity) -> Result<()> {
    // SAFETY: initialized credential output and exact socket-option length;
    // the borrowed live descriptor is neither closed nor changed.
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&credentials) as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0 {
        return Err(Error::new(
            "ownership_changed",
            "Display peer credentials unavailable",
        ));
    }
    if length as usize != std::mem::size_of_val(&credentials)
        || credentials.pid != expected.pid
        || credentials.uid != expected.uid
    {
        return Err(Error::new(
            "ownership_changed",
            "Connected display is not the configured server",
        ));
    }
    verify_display_server(expected)
}
fn xerror(e: impl std::fmt::Display) -> Error {
    Error::new("input_unavailable", e.to_string())
}
/// Shared private-display authentication. This does not check keyboard focus,
/// load XTEST, bind a game window or reset the caller's absolute deadline.
pub(super) fn connect_owned(
    display: u16,
    display_server: &Identity,
    authority: &Path,
    until: Instant,
) -> Result<(RustConnection<BoundedStream>, u32)> {
    if Instant::now() >= until {
        return Err(Error::new("transport_timeout", "X11 deadline expired"));
    }
    verify_display_server(display_server)?;
    let socket =
        super::connect_socket_until(Path::new(&format!("/tmp/.X11-unix/X{display}")), until)?;
    verify_display_peer(&socket, display_server)?;
    let (stream, (family, address)) = DefaultStream::from_unix_stream(socket).map_err(xerror)?;
    if family != x11rb_protocol::xauth::Family::LOCAL {
        return Err(Error::new(
            "input_unavailable",
            "Local X11 transport required",
        ));
    }
    let cookie = xauthority::load_cookie(authority, &address, display)?;
    let stream = BoundedStream {
        stream,
        deadline: Mutex::new(until),
        server: display_server.clone(),
    };
    stream.remaining().map_err(xerror)?;
    let connection = RustConnection::connect_to_stream_with_auth_info(
        stream,
        0,
        xauthority::MIT_COOKIE_NAME.to_vec(),
        cookie.to_vec(),
    )
    .map_err(xerror)?;
    let root = connection
        .setup()
        .roots
        .first()
        .ok_or_else(|| Error::new("input_unavailable", "X11 screen unavailable"))?
        .root;
    Ok((connection, root))
}
/// Reuse the strict loader only in the private generated forwarding-display
/// fixture. This helper is absent from production; it does not connect a display
/// or weaken the peer/auth/process checks on either real transport.
#[cfg(test)]
pub(crate) fn fixture_local_cookie(authority: &Path, display: u16) -> Result<(Vec<u8>, [u8; 16])> {
    let (socket, _peer) = std::os::unix::net::UnixStream::pair()?;
    let (_stream, (family, hostname)) = DefaultStream::from_unix_stream(socket).map_err(xerror)?;
    if family != x11rb_protocol::xauth::Family::LOCAL {
        return Err(Error::new(
            "fixture_failed",
            "Local fixture address required",
        ));
    }
    Ok((
        hostname.clone(),
        xauthority::load_cookie(authority, &hostname, display)?,
    ))
}
fn require_client_dimensions(expected: [u16; 2], actual: [u16; 2], same_root: bool) -> Result<()> {
    if !same_root || actual != expected {
        return Err(Error::new(
            "input_geometry_changed",
            "Owned client dimensions differ from immutable profile",
        ));
    }
    Ok(())
}
pub struct X11Controls {
    connection: RustConnection<BoundedStream>,
    root: u32,
    window: u32,
    game: Identity,
    class: Vec<u8>,
    pid_atom: u32,
    active_atom: u32,
    expected_client: Option<[u16; 2]>,
}
impl X11Controls {
    /// Bind one already-focused, owned window. Never activates windows, moves
    /// the cursor, changes the map, or discovers/takes over an arbitrary game.
    pub fn open(
        process: &dyn Process,
        display: u16,
        display_server: &Identity,
        authority: &Path,
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
        let (connection, root) = connect_owned(
            display,
            display_server,
            authority,
            Instant::now() + Duration::from_secs(2),
        )?;
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
            expected_client: None,
        };
        controls.check(process, &ControlKey::Pause)?;
        Ok(controls)
    }
    /// Owner-only immutable client-size pin. No activation or pointer movement.
    /// Clock-only bindings may omit this; opted-in domain profiles must supply it.
    pub fn bind_client_dimensions(
        &mut self,
        process: &dyn Process,
        expected: [u16; 2],
    ) -> Result<()> {
        if expected.iter().any(|n| !(1..=16384).contains(n))
            || self.expected_client.is_some_and(|prior| prior != expected)
        {
            return Err(Error::new(
                "invalid_input_binding",
                "Immutable bounded owned client dimensions required",
            ));
        }
        self.expected_client = Some(expected);
        self.check(process, &ControlKey::Pause).map(|_| ())
    }
    fn check_client_dimensions(&self) -> Result<()> {
        if let Some(expected) = self.expected_client {
            let geometry = self
                .connection
                .get_geometry(self.window)
                .map_err(xerror)?
                .reply()
                .map_err(xerror)?;
            require_client_dimensions(
                expected,
                [geometry.width, geometry.height],
                geometry.root == self.root,
            )?;
        }
        Ok(())
    }
    fn begin(&self) -> Result<()> {
        self.connection.stream().verify_peer()?;
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
        self.check_client_dimensions()?;
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
    pub fn preflight(&self, process: &dyn Process, key: &ControlKey) -> Result<()> {
        self.check(process, key).map(|_| ())
    }
    pub fn send(&self, process: &dyn Process, key: ControlKey) -> Result<()> {
        self.send_until(process, key, Instant::now() + Duration::from_secs(5))
    }
    /// The supervisor reserves the bounded transport window inside its lease.
    /// Expired authority must fail before either ordinary key event is queued.
    pub fn send_until(
        &self,
        process: &dyn Process,
        key: ControlKey,
        lease_deadline: Instant,
    ) -> Result<()> {
        self.send_until_checked(process, key, lease_deadline, || Ok(true))
            .map(|_| ())
    }
    /// Refresh the typed action decision after transport preflight, immediately
    /// before queuing input. A completed target skips both key events.
    pub fn send_until_checked(
        &self,
        process: &dyn Process,
        key: ControlKey,
        lease_deadline: Instant,
        before_queue: impl FnOnce() -> Result<bool>,
    ) -> Result<bool> {
        let code = self.check(process, &key)?;
        self.connection.stream().verify_peer()?;
        if !before_queue()? {
            return Ok(false);
        }
        process.check()?;
        self.connection.stream().verify_peer()?;
        // The typed final reader may block or the client may resize meanwhile.
        // Refresh the immutable size after it, before either key is queued.
        self.check_client_dimensions()?;
        if lease_deadline.saturating_duration_since(Instant::now()) < Duration::from_secs(2) {
            return Err(Error::new(
                "input_lease_required",
                "Insufficient live lease for bounded key delivery",
            ));
        }
        *self.connection.stream().deadline.lock().map_err(xerror)? =
            lease_deadline.min(Instant::now() + Duration::from_secs(2));
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
        self.connection.stream().verify_peer()?;
        Ok(true) // Transport delivery is not a verified game action receipt.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;
    #[test]
    fn immutable_owned_client_size_refuses_resize_or_another_root() {
        require_client_dimensions([1920, 1080], [1920, 1080], true).unwrap();
        for (size, same_root) in [
            ([512, 288], true),
            ([1921, 1080], true),
            ([1920, 1079], true),
            ([1920, 1080], false),
        ] {
            assert_eq!(
                require_client_dimensions([1920, 1080], size, same_root)
                    .unwrap_err()
                    .code,
                "input_geometry_changed"
            );
        }
    }
    #[test]
    fn unresponsive_display_has_a_bounded_deadline() {
        let (socket, _peer) = UnixStream::pair().unwrap();
        let (stream, _) = DefaultStream::from_unix_stream(socket).unwrap();
        let stream = BoundedStream {
            stream,
            deadline: Mutex::new(Instant::now() + Duration::from_millis(20)),
            server: super::super::capture_identity(std::process::id() as i32).unwrap(),
        };
        let start = Instant::now();
        assert_eq!(
            stream.poll(PollMode::Readable).unwrap_err().kind(),
            io::ErrorKind::TimedOut
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn connected_display_peer_requires_exact_ordinary_identity() {
        // SAFETY: read-only UID query; an ordinary private-display fixture.
        if unsafe { libc::getuid() } == 0 {
            eprintln!("SKIP: display identity fixture requires ordinary UID");
            return;
        }
        let (socket, _peer) = UnixStream::pair().unwrap();
        let identity = super::super::capture_identity(std::process::id() as i32).unwrap();
        verify_display_peer(&socket, &identity).unwrap();
        let mut wrong = identity.clone();
        wrong.pid += 1;
        assert_eq!(
            verify_display_peer(&socket, &wrong).unwrap_err().code,
            "ownership_changed"
        );
        let mut wrong = identity.clone();
        wrong.uid += 1;
        assert_eq!(
            verify_display_peer(&socket, &wrong).unwrap_err().code,
            "ownership_changed"
        );
        let mut stale = identity;
        stale.start_ticks = "0".into();
        assert_eq!(
            verify_display_peer(&socket, &stale).unwrap_err().code,
            "ownership_changed"
        );
    }
    #[test]
    fn changed_display_identity_blocks_a_later_transport_write() {
        use std::io::Read;
        // SAFETY: read-only UID query, no real display or keyboard input.
        if unsafe { libc::getuid() } == 0 {
            eprintln!("SKIP: display transport fixture requires ordinary UID");
            return;
        }
        let (socket, mut peer) = UnixStream::pair().unwrap();
        let (stream, _) = DefaultStream::from_unix_stream(socket).unwrap();
        let mut stream = BoundedStream {
            stream,
            deadline: Mutex::new(Instant::now() + Duration::from_secs(2)),
            server: super::super::capture_identity(std::process::id() as i32).unwrap(),
        };
        assert_eq!(stream.write(b"first", &mut Vec::new()).unwrap(), 5);
        let mut bytes = [0; 5];
        peer.read_exact(&mut bytes).unwrap();
        assert_eq!(&bytes, b"first");
        stream.server.start_ticks = "0".into();
        assert_eq!(
            stream.write(b"second", &mut Vec::new()).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        peer.set_nonblocking(true).unwrap();
        assert_eq!(
            peer.read(&mut bytes).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
}
