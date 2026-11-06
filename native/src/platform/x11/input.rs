//! General ordinary input for the owned game window on the owned X11 display:
//! the Linux backend of [`GameInput`]. XTEST keys, buttons, motion and wheel
//! only; every input re-checks the owned process, the window's PID and class,
//! that it is viewable, focused and active, and its fixed client size. Keys and
//! buttons held by anybody refuse input. Delivery is never acknowledged by the
//! transport; callers verify the game state through memory, as on macOS.
use super::{connect_owned, require_client_dimensions, xerror, BoundedStream};
use crate::{
    error::{Error, Result},
    platform::{
        game_input::{find_digit_code, vk, Capture, GameInput, Key, Point},
        key_once, Identity, Process,
    },
};
use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
    thread::sleep,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            AtomEnum, ConnectionExt as _, ImageFormat, MapState, BUTTON_PRESS_EVENT,
            BUTTON_RELEASE_EVENT, KEY_PRESS_EVENT, KEY_RELEASE_EVENT, MOTION_NOTIFY_EVENT,
        },
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

const XK_SHIFT_L: u32 = 0xffe1;
/// The game samples modifier state once per frame; on a software-rendered
/// display a frame can take 100 ms or more, so a modifier is held well before
/// and after the press, and a press lasts more than one fast frame.
const MODIFIER_LEAD: Duration = Duration::from_millis(150);
const PRESS_HOLD: Duration = Duration::from_millis(60);
const XK_CONTROL_L: u32 = 0xffe3;
const LEFT: u8 = 1;
const RIGHT: u8 = 3;
const WHEEL_UP: u8 = 4;
const WHEEL_DOWN: u8 = 5;

/// X11 keysym of a platform-neutral virtual key code.
pub(crate) fn keysym(code: u16) -> Option<u32> {
    if let Some(i) = vk::LETTERS.iter().position(|c| *c == code) {
        return Some(0x61 + i as u32);
    }
    if let Some(i) = vk::DIGITS.iter().position(|c| *c == code) {
        return Some(0x30 + i as u32);
    }
    if let Some((c, _)) = vk::PUNCTUATION.iter().find(|(_, c)| *c == code) {
        return Some(*c as u32);
    }
    Some(match code {
        vk::RETURN => 0xff0d,
        vk::TAB => 0xff09,
        vk::SPACE => 0x20,
        vk::DELETE => 0xff08,
        vk::ESCAPE => 0xff1b,
        vk::KEYPAD_PLUS => 0xffab,
        vk::KEYPAD_MINUS => 0xffad,
        vk::F1 => 0xffbe,
        vk::F2 => 0xffbf,
        vk::F3 => 0xffc0,
        vk::F4 => 0xffc1,
        vk::F5 => 0xffc2,
        vk::HOME => 0xff50,
        vk::END => 0xff57,
        vk::FORWARD_DELETE => 0xffff,
        vk::LEFT => 0xff51,
        vk::UP => 0xff52,
        vk::RIGHT => 0xff53,
        vk::DOWN => 0xff54,
        _ => return None,
    })
}

/// Wheel notches for a macOS-style pixel scroll: one notch per 120 pixels, at
/// least one. Positive scrolls content up (wheel up).
pub(crate) fn notches(pixels: i32) -> (u8, u32) {
    let button = if pixels > 0 { WHEEL_UP } else { WHEEL_DOWN };
    (button, pixels.unsigned_abs().div_ceil(120).max(1))
}

/// Client-relative game pixel to an absolute root pixel, refusing points
/// outside the fixed client area.
pub(crate) fn absolute(origin: (i16, i16), client: [u16; 2], x: f64, y: f64) -> Result<(i16, i16)> {
    if !(x.is_finite() && y.is_finite())
        || x < 0.0
        || y < 0.0
        || x >= f64::from(client[0])
        || y >= f64::from(client[1])
    {
        return Err(Error::new(
            "invalid_arguments",
            "Point outside the owned game client area",
        ));
    }
    let ax = i32::from(origin.0) + x.round() as i32;
    let ay = i32::from(origin.1) + y.round() as i32;
    Ok((
        i16::try_from(ax).map_err(|_| Error::new("input_unavailable", "Point out of range"))?,
        i16::try_from(ay).map_err(|_| Error::new("input_unavailable", "Point out of range"))?,
    ))
}

struct Shared {
    connection: RustConnection<BoundedStream>,
    root: u32,
    window: u32,
    game: Identity,
    class: Vec<u8>,
    pid_atom: u32,
    active_atom: u32,
    client: [u16; 2],
    /// Unshifted keysym -> unique keycode.
    keycodes: HashMap<u32, u8>,
}

enum Binding<'p> {
    Borrowed(&'p (dyn Process + Send + Sync)),
    Owned(Arc<dyn Process + Send + Sync>),
}
impl Binding<'_> {
    fn get(&self) -> &(dyn Process + Send + Sync) {
        match self {
            Self::Borrowed(p) => *p,
            Self::Owned(p) => p.as_ref(),
        }
    }
}

pub struct X11Input<'p> {
    shared: Arc<Shared>,
    process: Binding<'p>,
    guards: Vec<&'p (dyn Fn() -> Result<()> + Send + Sync)>,
}

fn unique_keycodes(min: u8, stride: usize, keysyms: &[u32]) -> HashMap<u32, u8> {
    let mut seen: HashMap<u32, Option<u8>> = HashMap::new();
    for (i, symbols) in keysyms.chunks_exact(stride).enumerate() {
        if let Some(&sym) = symbols.first() {
            if sym != 0 {
                let code = min.wrapping_add(i as u8);
                seen.entry(sym)
                    .and_modify(|v| {
                        if *v != Some(code) {
                            *v = None
                        }
                    })
                    .or_insert(Some(code));
            }
        }
    }
    seen.into_iter()
        .filter_map(|(sym, code)| code.map(|c| (sym, c)))
        .collect()
}

impl X11Input<'static> {
    /// Bind the already-focused owned game window on the owned display. Never
    /// activates or maps windows and never moves the pointer while binding.
    pub fn open_owned(
        process: Arc<dyn Process + Send + Sync>,
        display: u16,
        display_server: &Identity,
        authority: &Path,
        window: u32,
        verified_class: &[u8],
        client: [u16; 2],
    ) -> Result<Self> {
        let shared = open_shared(
            process.as_ref(),
            display,
            display_server,
            authority,
            window,
            verified_class,
            client,
        )?;
        let input = Self {
            shared: Arc::new(shared),
            process: Binding::Owned(process),
            guards: Vec::new(),
        };
        input.check()?;
        Ok(input)
    }
}

fn open_shared(
    process: &dyn Process,
    display: u16,
    display_server: &Identity,
    authority: &Path,
    window: u32,
    verified_class: &[u8],
    client: [u16; 2],
) -> Result<Shared> {
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
    if client.iter().any(|n| !(1..=16384).contains(n)) {
        return Err(Error::new(
            "invalid_input_binding",
            "Bounded owned client dimensions required",
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
    let setup = connection.setup();
    let min = setup.min_keycode;
    let count = setup
        .max_keycode
        .checked_sub(min)
        .and_then(|v| v.checked_add(1))
        .ok_or_else(|| Error::new("input_unavailable", "Keyboard mapping bound"))?;
    let mapping = connection
        .get_keyboard_mapping(min, count)
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
    let keycodes = unique_keycodes(min, stride, &mapping.keysyms);
    Ok(Shared {
        connection,
        root,
        window,
        game,
        class: verified_class.to_vec(),
        pid_atom,
        active_atom,
        client,
        keycodes,
    })
}

impl X11Input<'_> {
    fn conn(&self) -> &RustConnection<BoundedStream> {
        &self.shared.connection
    }
    fn begin(&self) -> Result<()> {
        let stream = self.conn().stream();
        stream.verify_peer()?;
        *stream.deadline.lock().map_err(xerror)? = Instant::now() + Duration::from_secs(2);
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
            .conn()
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
    /// Ownership, window, focus and size checks, then every action fence.
    fn check(&self) -> Result<()> {
        let s = &self.shared;
        self.begin()?;
        let process = self.process.get();
        process.check()?;
        if process.identity() != Some(&s.game) {
            return Err(Error::new(
                "ownership_changed",
                "Input process identity changed",
            ));
        }
        let p = self.property(s.window, s.pid_atom, AtomEnum::CARDINAL, 1)?;
        if p.format != 32 || p.value32().and_then(|mut v| v.next()) != Some(s.game.pid as u32) {
            return Err(Error::new(
                "ownership_changed",
                "Window PID is not the owned game",
            ));
        }
        let p = self.property(s.window, AtomEnum::WM_CLASS.into(), AtomEnum::STRING, 64)?;
        if p.format != 8 || p.value != s.class {
            return Err(Error::new(
                "ownership_changed",
                "Owned window class changed",
            ));
        }
        if self
            .conn()
            .get_window_attributes(s.window)
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
        let geometry = self
            .conn()
            .get_geometry(s.window)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        require_client_dimensions(
            s.client,
            [geometry.width, geometry.height],
            geometry.root == s.root,
        )?;
        let p = self.property(s.root, s.active_atom, AtomEnum::WINDOW, 1)?;
        if p.format != 32
            || p.value32().and_then(|mut v| v.next()) != Some(s.window)
            || self
                .conn()
                .get_input_focus()
                .map_err(xerror)?
                .reply()
                .map_err(xerror)?
                .focus
                != s.window
        {
            return Err(Error::new(
                "input_unavailable",
                "Owned window lost focus; no activation fallback",
            ));
        }
        for guard in &self.guards {
            guard()?;
        }
        Ok(())
    }
    fn idle(&self) -> Result<()> {
        let keys = self
            .conn()
            .query_keymap()
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?
            .keys;
        let pointer = self
            .conn()
            .query_pointer(self.shared.root)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if keys.iter().any(|v| *v != 0) || u16::from(pointer.mask) != 0 {
            return Err(Error::new(
                "input_busy",
                "A key, button or modifier is held",
            ));
        }
        Ok(())
    }
    /// The on-screen part of the owned window read from the root window; pixels off screen stay
    /// black (0).
    fn capture_visible(&self, w: u16, h: u16) -> Result<Vec<u8>> {
        let (x, y) = self.origin()?;
        let screen = self
            .conn()
            .get_geometry(self.shared.root)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let left = i32::from(x).max(0);
        let top = i32::from(y).max(0);
        let right = (i32::from(x) + i32::from(w)).min(i32::from(screen.width));
        let bottom = (i32::from(y) + i32::from(h)).min(i32::from(screen.height));
        if right <= left || bottom <= top {
            return Err(Error::new(
                "capture_unavailable",
                "Owned window is not on screen",
            ));
        }
        let (vw, vh) = ((right - left) as u16, (bottom - top) as u16);
        let image = self
            .conn()
            .get_image(
                ImageFormat::Z_PIXMAP,
                self.shared.root,
                left as i16,
                top as i16,
                vw,
                vh,
                !0,
            )
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        let row = usize::from(vw) * 4;
        if image.data.len() != row * usize::from(vh) {
            return Err(Error::new(
                "capture_unavailable",
                "Root image is not 32-bit BGRX",
            ));
        }
        let stride = usize::from(w) * 4;
        let mut data = vec![0u8; stride * usize::from(h)];
        let (dx, dy) = (
            (left - i32::from(x)) as usize,
            (top - i32::from(y)) as usize,
        );
        for (i, line) in image.data.chunks_exact(row).enumerate() {
            let at = (dy + i) * stride + dx * 4;
            data[at..at + row].copy_from_slice(line);
        }
        Ok(data)
    }
    fn origin(&self) -> Result<(i16, i16)> {
        let t = self
            .conn()
            .translate_coordinates(self.shared.window, self.shared.root, 0, 0)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if !t.same_screen {
            return Err(Error::new(
                "input_unavailable",
                "Owned window on another screen",
            ));
        }
        Ok((t.dst_x, t.dst_y))
    }
    fn point(&self, game: (u32, u32), x: f64, y: f64) -> Result<(i16, i16)> {
        let client = self.shared.client;
        if game != (u32::from(client[0]), u32::from(client[1])) {
            return Err(Error::new(
                "input_unavailable",
                "Game size differs from the owned client size",
            ));
        }
        absolute(self.origin()?, client, x, y)
    }
    fn fake(&self, kind: u8, detail: u8, at: (i16, i16)) -> Result<()> {
        self.conn()
            .xtest_fake_input(kind, detail, 0, self.shared.root, at.0, at.1, 0)
            .map_err(xerror)?;
        self.conn().flush().map_err(xerror)?;
        Ok(())
    }
    fn motion(&self, at: (i16, i16)) -> Result<()> {
        self.fake(MOTION_NOTIFY_EVENT, 0, at)
    }
    fn button(&self, button: u8, down: bool) -> Result<()> {
        let kind = if down {
            BUTTON_PRESS_EVENT
        } else {
            BUTTON_RELEASE_EVENT
        };
        self.fake(kind, button, (0, 0))
    }
    fn key(&self, keycode: u8, down: bool) -> Result<()> {
        let kind = if down {
            KEY_PRESS_EVENT
        } else {
            KEY_RELEASE_EVENT
        };
        self.fake(kind, keycode, (0, 0))
    }
    fn keycode_of_sym(&self, sym: u32) -> Result<u8> {
        self.shared
            .keycodes
            .get(&sym)
            .copied()
            .ok_or_else(|| Error::new("input_unavailable", "Keyboard binding absent or ambiguous"))
    }
    fn keycode(&self, code: u16) -> Result<u8> {
        let sym = keysym(code)
            .ok_or_else(|| Error::new("invalid_arguments", "Key has no X11 equivalent"))?;
        self.keycode_of_sym(sym)
    }
    /// Press `keycode`, optionally inside a held modifier. A pressed key or
    /// modifier is always released, also after a failed step.
    fn tap(&self, keycode: u8, modifier: Option<u8>) -> Result<()> {
        self.check()?;
        self.idle()?;
        if let Some(m) = modifier {
            self.key(m, true)?;
            sleep(MODIFIER_LEAD);
        }
        let down = self.key(keycode, true);
        if down.is_ok() {
            sleep(PRESS_HOLD);
        }
        let up = self.key(keycode, false);
        let released = modifier.map_or(Ok(()), |m| {
            sleep(MODIFIER_LEAD);
            self.key(m, false)
        });
        sleep(Duration::from_millis(40));
        down.and(up).and(released)
    }
    /// Move, settle, optional fence, press, release. The cursor stays at the point.
    fn click_with(
        &self,
        game: (u32, u32),
        x: f64,
        y: f64,
        button: u8,
        modifier: Option<u8>,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<Point> {
        self.check()?;
        self.idle()?;
        let at = self.point(game, x, y)?;
        self.motion(at)?;
        sleep(Duration::from_millis(60));
        self.check()?;
        before_down()?;
        if let Some(m) = modifier {
            self.key(m, true)?;
            sleep(MODIFIER_LEAD);
        }
        let down = self.button(button, true);
        if down.is_ok() {
            sleep(PRESS_HOLD);
        }
        let up = self.button(button, false);
        let released = modifier.map_or(Ok(()), |m| {
            sleep(MODIFIER_LEAD);
            self.key(m, false)
        });
        sleep(Duration::from_millis(60));
        down.and(up).and(released)?;
        Ok(Point { x, y })
    }
    fn drag_with(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        button: u8,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.check()?;
        self.idle()?;
        let start = self.point(game, from.0, from.1)?;
        let end = self.point(game, to.0, to.1)?;
        self.motion(start)?;
        sleep(Duration::from_millis(60));
        self.check()?;
        before_down()?;
        let down = self.button(button, true);
        let mut moved = Ok(());
        if down.is_ok() {
            sleep(Duration::from_millis(60));
            for step in 1..=10 {
                let t = f64::from(step) / 10.0;
                let p = (
                    (f64::from(start.0) + (f64::from(end.0) - f64::from(start.0)) * t).round()
                        as i16,
                    (f64::from(start.1) + (f64::from(end.1) - f64::from(start.1)) * t).round()
                        as i16,
                );
                moved = self.motion(p);
                if moved.is_err() {
                    break;
                }
                sleep(Duration::from_millis(20));
            }
            sleep(Duration::from_millis(60));
        }
        let up = self.button(button, false);
        sleep(Duration::from_millis(80));
        down.and(moved).and(up)
    }
    fn once(&self, code: u16) -> key_once::Dispatch {
        struct Pair<'a, 'p> {
            input: &'a X11Input<'p>,
            keycode: u8,
        }
        impl key_once::Pair for Pair<'_, '_> {
            fn down(&mut self) -> Result<()> {
                self.input.key(self.keycode, true)
            }
            fn up(&mut self) -> Result<()> {
                self.input.key(self.keycode, false)
            }
        }
        key_once::dispatch(
            || self.confirm_input_idle(),
            || {
                Ok(Pair {
                    input: self,
                    keycode: self.keycode(code)?,
                })
            },
            sleep,
        )
    }
}

impl GameInput for X11Input<'_> {
    fn capture(&self, game: (u32, u32)) -> Result<Capture> {
        self.check()?;
        let [w, h] = self.shared.client;
        if game != (u32::from(w), u32::from(h)) {
            return Err(Error::new(
                "input_unavailable",
                "Game size differs from the owned client size",
            ));
        }
        let reply = self
            .conn()
            .get_image(ImageFormat::Z_PIXMAP, self.shared.window, 0, 0, w, h, !0)
            .map_err(xerror)?
            .reply();
        let (width, height) = (usize::from(w), usize::from(h));
        let data = match reply {
            Ok(image) => image.data,
            // X refuses a window's image with BadMatch while any part of it lies off screen. A
            // window manager frame can do that: openbox kept a 1 px border on the undecorated game
            // window, which sat at (1, 1) on a 1920x1080 screen (5 November 2026).
            Err(_) => self.capture_visible(w, h)?,
        };
        if data.len() != width * height * 4 {
            return Err(Error::new(
                "capture_unavailable",
                "Owned window image is not 32-bit BGRX",
            ));
        }
        Ok(Capture {
            width,
            height,
            stride: width * 4,
            scale: 1.0,
            data,
        })
    }
    fn preflight(&self) -> Result<()> {
        self.check()
    }
    fn confirm_input_idle(&self) -> Result<()> {
        self.check()?;
        self.idle()?;
        self.check()
    }
    fn click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.click_with(game, x, y, LEFT, None, &|| Ok(()))
    }
    fn click_checked(
        &self,
        game: (u32, u32),
        x: f64,
        y: f64,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<Point> {
        self.click_with(game, x, y, LEFT, None, before_down)
    }
    fn confirm_cursor_at(&self, game: (u32, u32), x: f64, y: f64) -> Result<()> {
        self.check()?;
        let expected = self.point(game, x, y)?;
        let pointer = self
            .conn()
            .query_pointer(self.shared.root)
            .map_err(xerror)?
            .reply()
            .map_err(xerror)?;
        if (pointer.root_x, pointer.root_y) != expected {
            return Err(Error::new(
                "cursor_moved",
                "Pointer is not at the expected game point",
            ));
        }
        Ok(())
    }
    fn drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.drag_with(game, from, to, LEFT, &|| Ok(()))
    }
    fn drag_checked(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.drag_with(game, from, to, LEFT, before_down)
    }
    fn shift_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        let shift = self.keycode_of_sym(XK_SHIFT_L)?;
        self.click_with(game, x, y, LEFT, Some(shift), &|| Ok(()))
    }
    fn right_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.click_with(game, x, y, RIGHT, None, &|| Ok(()))
    }
    fn right_drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.drag_with(game, from, to, RIGHT, &|| Ok(()))
    }
    fn scroll(&self, game: (u32, u32), at: (f64, f64), pixels: i32) -> Result<()> {
        self.check()?;
        self.idle()?;
        let point = self.point(game, at.0, at.1)?;
        self.motion(point)?;
        sleep(Duration::from_millis(60));
        let (button, count) = notches(pixels);
        for _ in 0..count {
            self.check()?;
            let down = self.button(button, true);
            let up = self.button(button, false);
            down.and(up)?;
            sleep(Duration::from_millis(40));
        }
        sleep(Duration::from_millis(120));
        Ok(())
    }
    fn scroll_burst(
        &self,
        game: (u32, u32),
        at: (f64, f64),
        pixels: i32,
        count: u32,
    ) -> Result<()> {
        self.check()?;
        self.idle()?;
        let point = self.point(game, at.0, at.1)?;
        self.motion(point)?;
        sleep(Duration::from_millis(40));
        let (button, _) = notches(pixels);
        for _ in 0..count.min(64) {
            self.check()?;
            let down = self.button(button, true);
            let up = self.button(button, false);
            down.and(up)?;
            sleep(Duration::from_millis(8));
        }
        sleep(Duration::from_millis(250));
        Ok(())
    }
    fn press_code(&self, code: u16) -> Result<()> {
        let keycode = self.keycode(code)?;
        self.tap(keycode, None)
    }
    fn press_shifted(&self, code: u16) -> Result<()> {
        let keycode = self.keycode(code)?;
        let shift = self.keycode_of_sym(XK_SHIFT_L)?;
        self.tap(keycode, Some(shift))
    }
    fn press_control(&self, code: u16) -> Result<()> {
        let keycode = self.keycode(code)?;
        let control = self.keycode_of_sym(XK_CONTROL_L)?;
        self.tap(keycode, Some(control))
    }
    fn press_once(&self, key: Key) -> key_once::Dispatch {
        self.once(key.code())
    }
    fn press_digit_once(&self, digit: char) -> key_once::Dispatch {
        match find_digit_code(digit) {
            Ok(code) => self.once(code),
            Err(e) => key_once::Dispatch::NotQueued(e),
        }
    }
    fn guarded<'a>(
        &'a self,
        process: &'a (dyn Process + Send + Sync),
        check: &'a (dyn Fn() -> Result<()> + Send + Sync),
    ) -> Result<Box<dyn GameInput + 'a>> {
        let bound = self.process.get();
        if process.identity() != bound.identity()
            || process.image_base() != bound.image_base()
            || process.string_abi() != bound.string_abi()
        {
            return Err(Error::new(
                "ownership_changed",
                "Guarded input reader differs from its binding",
            ));
        }
        let mut guards: Vec<&'a (dyn Fn() -> Result<()> + Send + Sync)> = self.guards.to_vec();
        guards.push(check);
        Ok(Box::new(X11Input {
            shared: self.shared.clone(),
            process: Binding::Borrowed(process),
            guards,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_named_key_has_a_keysym() {
        for code in [
            vk::RETURN,
            vk::TAB,
            vk::SPACE,
            vk::DELETE,
            vk::ESCAPE,
            vk::KEYPAD_PLUS,
            vk::KEYPAD_MINUS,
            vk::F1,
            vk::F5,
            vk::HOME,
            vk::END,
            vk::FORWARD_DELETE,
            vk::LEFT,
            vk::UP,
        ] {
            assert!(keysym(code).is_some(), "{code}");
        }
        assert_eq!(keysym(vk::LETTERS[0]), Some(0x61));
        assert_eq!(keysym(vk::LETTERS[25]), Some(0x7a));
        assert_eq!(keysym(vk::DIGITS[7]), Some(0x37));
        assert_eq!(keysym(27), Some('-' as u32));
        for c in ('a'..='z').chain('0'..='9').chain("-=[];',./\\` ".chars()) {
            let (code, shift) = vk::for_char(c).unwrap();
            assert!(!shift);
            assert_eq!(keysym(code), Some(c as u32), "{c}");
        }
    }

    #[test]
    fn wheel_notches() {
        assert_eq!(notches(-240), (WHEEL_DOWN, 2));
        assert_eq!(notches(240), (WHEEL_UP, 2));
        assert_eq!(notches(30), (WHEEL_UP, 1));
        assert_eq!(notches(-121), (WHEEL_DOWN, 2));
    }

    #[test]
    fn points_stay_inside_the_client() {
        assert_eq!(
            absolute((100, 50), [1920, 1080], 0.0, 0.0).unwrap(),
            (100, 50)
        );
        assert_eq!(
            absolute((0, 0), [1920, 1080], 1919.4, 1079.0).unwrap(),
            (1919, 1079)
        );
        assert!(absolute((0, 0), [1920, 1080], 1920.0, 10.0).is_err());
        assert!(absolute((0, 0), [1920, 1080], -1.0, 10.0).is_err());
        assert!(absolute((0, 0), [1920, 1080], f64::NAN, 10.0).is_err());
    }

    #[test]
    fn keycodes_are_unique_unshifted_symbols() {
        // keycode 8: a/A, 9: b/B, 10: a again (ambiguous), 11: Return.
        let syms = [0x61, 0x41, 0x62, 0x42, 0x61, 0x41, 0xff0d, 0];
        let map = unique_keycodes(8, 2, &syms);
        assert_eq!(map.get(&0x62), Some(&9));
        assert_eq!(map.get(&0xff0d), Some(&11));
        assert!(!map.contains_key(&0x61));
        assert!(!map.contains_key(&0x41));
    }
}
