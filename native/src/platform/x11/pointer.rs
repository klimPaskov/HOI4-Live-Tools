//! Bounded one-attempt ordinary pointer transport building block.
//! This is neither hit authorization nor game-action verification. A future
//! typed adapter must hold its owned session/input gate and exclusive lease.
use super::*;
use serde::Serialize;
use std::cell::{Cell, RefCell};
use x11rb::protocol::xproto::{
    GetGeometryReply, WindowClass, BUTTON_PRESS_EVENT, BUTTON_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerDelivery {
    NotQueued,
    Unknown,
    ServerConfirmed,
}

#[derive(Debug, Serialize)]
pub struct PointerDispatch {
    pub delivery: PointerDelivery,
    pub error: Option<Error>,
    pub game_action_verified: bool,
}

/// Explicit owned display configuration from the future caller's session.
/// The actual peer must still equal the server pinned by X11Controls::open.
#[derive(Clone, Copy)]
pub struct PointerDisplay<'a> {
    pub display: u16,
    pub authority: &'a Path,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WindowWitness {
    parent: u32,
    depth: u8,
    geometry: [i32; 5], // Parent-relative x/y, client width/height, border width.
    origin: [i16; 2],   // Client (0,0), translated onto the configured root.
    root_dimensions: [u16; 2], // Fresh drawable geometry, never cached setup size.
}

fn refused(code: &'static str, message: &'static str) -> Error {
    Error::new(code, message)
}

fn absolute_point(witness: &WindowWitness, point: [i32; 2]) -> Result<[i16; 2]> {
    let mut absolute = [0; 2];
    for axis in 0..2 {
        if point[axis] < 0 || point[axis] >= witness.geometry[axis + 2] {
            return Err(refused(
                "invalid_client_point",
                "Pixel outside owned client",
            ));
        }
        absolute[axis] = i16::try_from(i32::from(witness.origin[axis]) + point[axis])
            .map_err(|_| refused("invalid_client_point", "X11 absolute coordinate overflow"))?;
        if absolute[axis] < 0 || absolute[axis] as u16 >= witness.root_dimensions[axis] {
            return Err(refused(
                "invalid_client_point",
                "Pixel outside configured root screen",
            ));
        }
    }
    Ok(absolute)
}

impl X11Controls {
    fn pointer_fields(&self) -> Result<WindowWitness> {
        let pid = self.property(self.window, self.pid_atom, AtomEnum::CARDINAL, 1)?;
        if pid.format != 32 || pid.value.as_slice() != (self.game.pid as u32).to_ne_bytes() {
            return Err(refused(
                "ownership_changed",
                "Owned pointer window PID changed",
            ));
        }
        let class = self.property(self.window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING, 64)?;
        if class.format != 8 || class.value != self.class {
            return Err(refused(
                "ownership_changed",
                "Owned pointer window class changed",
            ));
        }
        let attributes = self
            .connection
            .get_window_attributes(self.window)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let geometry: GetGeometryReply = self
            .connection
            .get_geometry(self.window)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let root_geometry = self
            .connection
            .get_geometry(self.root)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let tree = self
            .connection
            .query_tree(self.window)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if attributes.class != WindowClass::INPUT_OUTPUT
            || attributes.map_state != MapState::VIEWABLE
            || geometry.root != self.root
            || root_geometry.root != self.root
            || root_geometry.width == 0
            || root_geometry.height == 0
            || tree.root != self.root
            || geometry.width == 0
            || geometry.height == 0
        {
            return Err(refused(
                "input_unavailable",
                "Owned visible client on configured root required",
            ));
        }
        let active = self.property(self.root, self.active_atom, AtomEnum::WINDOW, 1)?;
        if active.format != 32
            || active.value.as_slice() != self.window.to_ne_bytes()
            || self
                .connection
                .get_input_focus()
                .map_err(xerror)?
                .reply()
                .map_err(xerror)?
                .focus
                != self.window
        {
            return Err(refused(
                "input_unavailable",
                "Owned window lost focus; no activation",
            ));
        }
        let translated = self
            .connection
            .translate_coordinates(self.window, self.root, 0, 0)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if !translated.same_screen {
            return Err(refused(
                "input_geometry_changed",
                "Owned client translation unavailable",
            ));
        }
        Ok(WindowWitness {
            parent: tree.parent,
            depth: geometry.depth,
            geometry: [
                i32::from(geometry.x),
                i32::from(geometry.y),
                i32::from(geometry.width),
                i32::from(geometry.height),
                i32::from(geometry.border_width),
            ],
            origin: [translated.dst_x, translated.dst_y],
            root_dimensions: [root_geometry.width, root_geometry.height],
        })
    }

    fn pointer_check(
        &self,
        process: &dyn Process,
        until: Instant,
        expected: Option<&WindowWitness>,
        allow_own_left: bool,
    ) -> Result<WindowWitness> {
        // One absolute deadline, never the keyboard begin()/deadline reset.
        *self.connection.stream().deadline.lock().map_err(xerror)? = until;
        self.connection.stream().remaining().map_err(xerror)?;
        self.connection.stream().verify_peer()?;
        process.check()?;
        if process.identity() != Some(&self.game) {
            return Err(refused(
                "ownership_changed",
                "Owned pointer process identity changed",
            ));
        }
        let before = self.pointer_fields()?;
        if expected.is_some_and(|expected| *expected != before) || self.pointer_fields()? != before
        {
            return Err(refused(
                "input_geometry_changed",
                "Owned client geometry/translation changed",
            ));
        }
        if self
            .connection
            .query_keymap()
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?
            .keys
            .iter()
            .any(|byte| *byte != 0)
        {
            return Err(refused(
                "input_unavailable",
                "Held keyboard key; pointer refused",
            ));
        }
        let pointer = self
            .connection
            .query_pointer(self.root)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let allowed = if allow_own_left { 1 << 8 } else { 0 };
        if !pointer.same_screen || u16::from(pointer.mask) & !allowed != 0 {
            return Err(refused(
                "input_unavailable",
                "Active modifiers/buttons; pointer refused",
            ));
        }
        process.check()?;
        self.connection.stream().verify_peer()?;
        self.connection.stream().remaining().map_err(xerror)?;
        Ok(before)
    }

    /// An immutable client pixel and expected-before dimensions are supplied
    /// by a future verified action adapter, never by a public raw-click tool.
    /// This borrow excludes simultaneous keyboard use through this instance.
    pub fn prepare_left_click_until<'a>(
        &'a mut self,
        process: &'a dyn Process,
        transport: PointerDisplay<'a>,
        point: [i32; 2],
        expected_dimensions: [u16; 2],
        deadline: Instant,
    ) -> Result<LeftClick<'a>> {
        if expected_dimensions
            .iter()
            .any(|size| !(1..=16384).contains(size))
            || point
                .iter()
                .zip(expected_dimensions)
                .any(|(value, size)| *value < 0 || *value >= i32::from(size))
            || !super::super::x11_close::valid_class(&self.class)
        {
            return Err(refused(
                "invalid_client_point",
                "Bounded explicit owned client pixel/dimensions required",
            ));
        }
        let until = deadline.min(Instant::now() + Duration::from_secs(2));
        let witness = self.pointer_check(process, until, None, false)?;
        if witness.geometry[2..4] != expected_dimensions.map(i32::from) {
            return Err(refused(
                "input_geometry_changed",
                "Expected-before client dimensions changed",
            ));
        }
        let absolute = absolute_point(&witness, point)?;
        Ok(LeftClick {
            controls: self,
            process,
            witness,
            absolute,
            transport,
            until,
            attempted: false,
            delivery: PointerDelivery::NotQueued,
        })
    }
}

/// Pointer-only setup reuses the original peer/auth/deadline primitives while
/// leaving every keyboard and shared authentication implementation unchanged.
fn connect_pointer<'a>(
    controls: &'a X11Controls,
    process: &'a dyn Process,
    witness: &'a WindowWitness,
    transport: PointerDisplay<'a>,
    until: Instant,
) -> Result<(RustConnection<PointerStream<'a>>, u32)> {
    if Instant::now() >= until {
        return Err(refused("transport_timeout", "X11 deadline expired"));
    }
    let server = &controls.connection.stream().server;
    verify_display_server(server)?;
    let socket = super::super::connect_socket_until(
        Path::new(&format!("/tmp/.X11-unix/X{}", transport.display)),
        until,
    )?;
    verify_display_peer(&socket, server)?;
    let (stream, (family, address)) = DefaultStream::from_unix_stream(socket).map_err(xerror)?;
    if family != x11rb_protocol::xauth::Family::LOCAL {
        return Err(refused("input_unavailable", "Local X11 transport required"));
    }
    let cookie = xauthority::load_cookie(transport.authority, &address, transport.display)?;
    let inner = BoundedStream {
        stream,
        deadline: Mutex::new(until),
        server: server.clone(),
    };
    inner.remaining().map_err(xerror)?;
    let connection = RustConnection::connect_to_stream_with_auth_info(
        PointerStream {
            inner,
            controls,
            process,
            witness,
            until,
            queueing: Cell::new(false),
            armed: Cell::new(false),
            attempted: Cell::new(false),
            written: Cell::new(0),
            queue: RefCell::new(Vec::new()),
        },
        0,
        xauthority::MIT_COOKIE_NAME.to_vec(),
        cookie.to_vec(),
    )
    .map_err(xerror)?;
    let root = connection
        .setup()
        .roots
        .first()
        .ok_or_else(|| refused("input_unavailable", "X11 screen unavailable"))?
        .root;
    Ok((connection, root))
}

/// This stream queues exactly the three declared XTEST requests locally. It
/// then writes their ordered bytes as one buffer when possible. A partial
/// write is NOT atomic delivery; every continuation is guarded afresh.
struct PointerStream<'a> {
    inner: BoundedStream,
    controls: &'a X11Controls,
    process: &'a dyn Process,
    witness: &'a WindowWitness,
    until: Instant,
    queueing: Cell<bool>,
    armed: Cell<bool>,
    attempted: Cell<bool>,
    written: Cell<usize>,
    queue: RefCell<Vec<u8>>,
}
impl PointerStream<'_> {
    fn guard(&self) -> io::Result<()> {
        self.controls
            .pointer_check(
                self.process,
                self.until,
                Some(self.witness),
                self.written.get() >= 72,
            )
            .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
        self.inner.remaining()?;
        self.inner
            .verify_peer()
            .map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error))?;
        self.inner.remaining()?;
        Ok(())
    }
    fn send_queue(&self) -> Result<()> {
        self.queueing.set(false);
        let bytes = self.queue.borrow();
        if bytes.len() != 108 {
            return Err(refused(
                "input_unavailable",
                "Exactly one motion/button pair required",
            ));
        }
        let mut offset = 0;
        while offset < bytes.len() {
            match self.write(&bytes[offset..], &mut Vec::new()) {
                Ok(0) => return Err(refused("input_unavailable", "Pointer transport closed")),
                Ok(count) => offset += count,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    self.poll(PollMode::Writable).map_err(xerror)?
                }
                Err(error) => return Err(xerror(error)),
            }
        }
        Ok(())
    }
}
impl Stream for PointerStream<'_> {
    fn poll(&self, mode: PollMode) -> io::Result<()> {
        self.inner.poll(mode)
    }
    fn read(&self, bytes: &mut [u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        self.inner.read(bytes, fds)
    }
    fn write(&self, bytes: &[u8], fds: &mut Vec<RawFdContainer>) -> io::Result<usize> {
        if self.queueing.get() {
            self.inner.remaining()?;
            let mut queue = self.queue.borrow_mut();
            if !fds.is_empty() || queue.len() + bytes.len() > 108 {
                return Err(io::Error::other("Pointer request queue bound"));
            }
            queue.extend_from_slice(bytes);
            return Ok(bytes.len());
        }
        if self.armed.get() {
            self.guard()?;
            // Conservative uncertainty before the actual write attempt, even
            // if the kernel eventually reports WouldBlock or a broken socket.
            self.attempted.set(true);
        }
        let count = self.inner.write(bytes, fds)?;
        if self.armed.get() {
            self.written.set(self.written.get() + count);
        }
        Ok(count)
    }
}

pub struct LeftClick<'a> {
    controls: &'a X11Controls,
    process: &'a dyn Process,
    witness: WindowWitness,
    absolute: [i16; 2],
    transport: PointerDisplay<'a>,
    until: Instant,
    attempted: bool,
    delivery: PointerDelivery,
}
impl LeftClick<'_> {
    /// One attempt, including refusal. Never retry, park, focus, compensate or
    /// release a possibly held button after uncertain transport/ownership.
    pub fn dispatch_once(&mut self) -> PointerDispatch {
        self.dispatch_once_checked(|| Ok(()))
    }
    /// Read-only stage check after preparation, before possible pointer input.
    pub fn dispatch_once_checked(
        &mut self,
        mut before_queue: impl FnMut() -> Result<()>,
    ) -> PointerDispatch {
        if self.attempted {
            return PointerDispatch {
                delivery: self.delivery,
                error: Some(refused(
                    "pointer_already_attempted",
                    "Never replay pointer delivery",
                )),
                game_action_verified: false,
            };
        }
        self.attempted = true;
        let mut attempted_write = false;
        let result = (|| -> Result<()> {
            self.controls
                .pointer_check(self.process, self.until, Some(&self.witness), false)?;
            let (connection, root) = connect_pointer(
                self.controls,
                self.process,
                &self.witness,
                self.transport,
                self.until,
            )?;
            if root != self.controls.root {
                return Err(refused(
                    "ownership_changed",
                    "Pointer dispatch root changed",
                ));
            }
            // Prime extension lookup and finish setup before locally queueing
            // the fixed batch, so no discovery request can enter that batch.
            connection
                .xtest_get_version(2, 2)
                .map_err(xerror)?
                .reply()
                .map_err(xerror)?;
            connection.flush().map_err(xerror)?;
            connection.stream().queueing.set(true);
            let motion = connection
                .xtest_fake_input(
                    MOTION_NOTIFY_EVENT,
                    0,
                    0,
                    root,
                    self.absolute[0],
                    self.absolute[1],
                    0,
                )
                .map_err(xerror)?;
            let press = connection
                .xtest_fake_input(BUTTON_PRESS_EVENT, 1, 0, root, 0, 0, 0)
                .map_err(xerror)?;
            let release = connection
                .xtest_fake_input(BUTTON_RELEASE_EVENT, 1, 0, root, 0, 0, 0)
                .map_err(xerror)?;
            // Drain x11rb's own output buffer into the local-only queue while
            // queueing is still enabled. No pointer bytes reach the socket.
            connection.flush().map_err(xerror)?;
            // Nothing above this point delivered pointer input.
            before_queue()?;
            // A domain's final memory observation may take time. Recheck held
            // keys/modifiers and the owned window AFTER it, before arming the
            // queued bytes. In particular Ctrl/Shift changes factory increments.
            self.controls
                .pointer_check(self.process, self.until, Some(&self.witness), false)?;
            connection.stream().armed.set(true);
            let delivery = (|| -> Result<()> {
                connection.stream().send_queue()?;
                motion.check().map_err(xerror)?;
                press.check().map_err(xerror)?;
                release.check().map_err(xerror)?;
                self.controls.pointer_check(
                    self.process,
                    self.until,
                    Some(&self.witness),
                    false,
                )?;
                connection.stream().inner.verify_peer()?;
                connection.stream().inner.remaining().map_err(xerror)?;
                Ok(())
            })();
            attempted_write = connection.stream().attempted.get();
            delivery
        })();
        self.delivery = if result.is_ok() {
            PointerDelivery::ServerConfirmed
        } else if attempted_write {
            PointerDelivery::Unknown
        } else {
            PointerDelivery::NotQueued
        };
        PointerDispatch {
            delivery: self.delivery,
            error: result.err(),
            game_action_verified: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_client_pixel_uses_client_origin_and_checks_signed_wire_range() {
        let mut witness = WindowWitness {
            parent: 1,
            depth: 24,
            geometry: [20, 30, 160, 120, 5],
            origin: [25, 35],
            root_dimensions: [u16::MAX, u16::MAX],
        };
        assert_eq!(absolute_point(&witness, [17, 19]).unwrap(), [42, 54]);
        for point in [[-1, 0], [0, -1], [160, 0], [0, 120]] {
            assert_eq!(
                absolute_point(&witness, point).unwrap_err().code,
                "invalid_client_point"
            );
        }
        witness.origin = [i16::MAX - 2, i16::MAX - 2];
        assert_eq!(
            absolute_point(&witness, [2, 2]).unwrap(),
            [i16::MAX, i16::MAX]
        );
        assert_eq!(
            absolute_point(&witness, [3, 2]).unwrap_err().code,
            "invalid_client_point"
        );
        assert_eq!(
            absolute_point(&witness, [2, 3]).unwrap_err().code,
            "invalid_client_point"
        );
        witness.origin = [25, 35];
        witness.root_dimensions = [42, 54];
        assert_eq!(absolute_point(&witness, [16, 18]).unwrap(), [41, 53]);
        assert_eq!(
            absolute_point(&witness, [17, 18]).unwrap_err().code,
            "invalid_client_point"
        );
        assert_eq!(
            absolute_point(&witness, [16, 19]).unwrap_err().code,
            "invalid_client_point"
        );
    }
}
