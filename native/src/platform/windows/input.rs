//! Ordinary input for the owned game window on Windows: the Windows backend of [`GameInput`],
//! modelled on the Linux/X11 backend. `SendInput` keys, buttons, absolute motion and wheel
//! only; every input re-checks the owned process, that the window still belongs to the owned
//! game PID, is visible and not minimised, keeps its fixed client size at 100% scaling and is
//! the foreground window. Keys and buttons held by anybody refuse input. Delivery is never
//! acknowledged by the transport; callers verify the game state through memory, as on macOS
//! and Linux.
//!
//! Nothing here activates, raises, moves or resizes a window: a window that is not already the
//! foreground window refuses with `input_target_not_frontmost` (the macOS code), and
//! `SetForegroundWindow` is never called.
//!
//! Keys. The platform-neutral [`vk`] codes are mapped to a Windows virtual-key code and a US
//! layout set-1 scan code. Events go out with `KEYEVENTF_SCANCODE` (plus
//! `KEYEVENTF_EXTENDEDKEY` for the E0 keys: arrows, Home, End, Delete): HOI4 reads hardware-like
//! scan codes, and the Python Windows runtime drove the game live this way on 1.19.4.x. With
//! `KEYEVENTF_SCANCODE` Windows takes the key from `wScan` and ignores `wVk`; `wVk` is filled
//! anyway so a recorded event names the key, and so nothing changes if the flag is ever dropped.
//! Because the scan code names a physical position, a non-US keyboard layout types other
//! characters for letters and punctuation (the same assumption as the Linux and Python paths).
//!
//! Coordinates. Game coordinates are client-area pixels. This process does not change its DPI
//! awareness (that is process-global); a DPI-unaware caller sees virtualised coordinates when
//! the window is scaled, so the window must report 96 DPI (100% scaling) from `GetDpiForWindow`
//! and its client rectangle must equal the bound size, or input refuses with
//! `input_geometry_unverified` / `input_geometry_changed`. Absolute moves are normalised over
//! the virtual desktop (`MOUSEEVENTF_VIRTUALDESK`) to the centre of the target pixel.
//!
//! Status: compiled and unit-tested (key tables, coordinate maths, the event sequences as data);
//! never run against the game. See docs/PORTING_STATUS.md.
use crate::{
    error::{Error, Result},
    platform::{
        game_input::{find_digit_code, vk, Capture, GameInput, Key, Point},
        key_once, Identity, Process,
    },
};
use std::{ffi::c_void, mem::size_of, sync::Arc, thread::sleep, time::Duration};

/// The game samples modifier state once per frame; a modifier is held well before and after
/// the press, and a press lasts more than one fast frame (the Linux backend's timing).
const MODIFIER_LEAD: u64 = 150;
const PRESS_HOLD: u64 = 60;
/// Pointer settle after a move, before anything is pressed.
const SETTLE: u64 = 60;
const KEY_AFTER: u64 = 40;
const CLICK_AFTER: u64 = 60;
const DRAG_STEP: u64 = 20;
const DRAG_AFTER: u64 = 80;
const PATH_STEP: u64 = 30;
const WHEEL_DELTA: i32 = 120;
const MAX_PATH_POINTS: usize = 512;
const MAX_BURST: u32 = 64;
const MAX_CLIENT: u32 = 16384;
const DEFAULT_DPI: u32 = 96;

// ---------------------------------------------------------------------------------------------
// Pure part: key table, coordinates and event sequences as data.
// ---------------------------------------------------------------------------------------------

/// A Windows key: virtual-key code, set-1 scan code and whether it is an E0 (extended) key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyCode {
    pub vk: u16,
    pub scan: u16,
    pub extended: bool,
}
const fn key(vk: u16, scan: u16) -> KeyCode {
    KeyCode {
        vk,
        scan,
        extended: false,
    }
}
const fn extended(vk: u16, scan: u16) -> KeyCode {
    KeyCode {
        vk,
        scan,
        extended: true,
    }
}
/// Left Shift, left Control and left Alt.
pub(crate) const SHIFT: KeyCode = key(0xA0, 0x2A);
pub(crate) const CONTROL: KeyCode = key(0xA2, 0x1D);
pub(crate) const ALT: KeyCode = key(0xA4, 0x38);

/// US set-1 scan codes of a..z.
const LETTER_SCANS: [u16; 26] = [
    0x1E, 0x30, 0x2E, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31, 0x18, 0x19,
    0x10, 0x13, 0x1F, 0x14, 0x16, 0x2F, 0x11, 0x2D, 0x15, 0x2C,
];
/// US set-1 scan codes of 0..9.
const DIGIT_SCANS: [u16; 10] = [0x0B, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A];
/// Unshifted punctuation: (character, VK_OEM_*, scan code).
const PUNCTUATION_KEYS: [(char, u16, u16); 11] = [
    ('-', 0xBD, 0x0C),
    ('=', 0xBB, 0x0D),
    ('[', 0xDB, 0x1A),
    (']', 0xDD, 0x1B),
    (';', 0xBA, 0x27),
    ('\'', 0xDE, 0x28),
    (',', 0xBC, 0x33),
    ('.', 0xBE, 0x34),
    ('/', 0xBF, 0x35),
    ('\\', 0xDC, 0x2B),
    ('`', 0xC0, 0x29),
];

/// The Windows key of a platform-neutral virtual key code.
pub(crate) fn key_code(code: u16) -> Option<KeyCode> {
    if let Some(i) = vk::LETTERS.iter().position(|c| *c == code) {
        return Some(key(0x41 + i as u16, LETTER_SCANS[i]));
    }
    if let Some(i) = vk::DIGITS.iter().position(|c| *c == code) {
        return Some(key(0x30 + i as u16, DIGIT_SCANS[i]));
    }
    if let Some((c, _)) = vk::PUNCTUATION.iter().find(|(_, c)| *c == code) {
        let (_, v, s) = PUNCTUATION_KEYS.iter().find(|(p, _, _)| p == c)?;
        return Some(key(*v, *s));
    }
    Some(match code {
        vk::RETURN => key(0x0D, 0x1C),
        vk::TAB => key(0x09, 0x0F),
        vk::SPACE => key(0x20, 0x39),
        vk::DELETE => key(0x08, 0x0E),
        vk::ESCAPE => key(0x1B, 0x01),
        vk::KEYPAD_PLUS => key(0x6B, 0x4E),
        vk::KEYPAD_MINUS => key(0x6D, 0x4A),
        vk::F1 => key(0x70, 0x3B),
        vk::F2 => key(0x71, 0x3C),
        vk::F3 => key(0x72, 0x3D),
        vk::F4 => key(0x73, 0x3E),
        vk::F5 => key(0x74, 0x3F),
        vk::HOME => extended(0x24, 0x47),
        vk::END => extended(0x23, 0x4F),
        vk::FORWARD_DELETE => extended(0x2E, 0x53),
        vk::LEFT => extended(0x25, 0x4B),
        vk::UP => extended(0x26, 0x48),
        vk::RIGHT => extended(0x27, 0x4D),
        vk::DOWN => extended(0x28, 0x50),
        _ => return None,
    })
}

/// Chords the shell or the window manager takes instead of the game: Alt+Tab, Alt+Space
/// (system menu), Alt+Escape, Alt+F4 (closes the game), Alt+Return (fullscreen toggle) and
/// Ctrl+Escape (Start menu). They are refused before any input.
pub(crate) fn refuse_chord(modifier: Option<KeyCode>, code: u16) -> Result<()> {
    let refused = match modifier {
        Some(m) if m == ALT => {
            matches!(code, vk::TAB | vk::SPACE | vk::ESCAPE | vk::F4 | vk::RETURN)
        }
        Some(m) if m == CONTROL => code == vk::ESCAPE,
        _ => false,
    };
    if refused {
        return Err(Error::new(
            "invalid_arguments",
            "This chord is a Windows shell shortcut, not a game key",
        ));
    }
    Ok(())
}

/// Wheel events for a macOS-style pixel scroll: one notch of 120 per 120 pixels, at least
/// one. Positive scrolls content up (wheel away from the user, a positive delta).
pub(crate) fn notches(pixels: i32) -> (i32, u32) {
    let delta = if pixels > 0 {
        WHEEL_DELTA
    } else {
        -WHEEL_DELTA
    };
    (delta, pixels.unsigned_abs().div_ceil(120).max(1))
}

/// Where the owned client area is: its screen origin and size, and the virtual desktop
/// (`SM_XVIRTUALSCREEN`, `SM_YVIRTUALSCREEN`, `SM_CXVIRTUALSCREEN`, `SM_CYVIRTUALSCREEN`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Geometry {
    pub origin: (i32, i32),
    pub client: (u32, u32),
    pub desk: (i32, i32, i32, i32),
}

/// One pointer target: the screen pixel and its normalised virtual-desktop coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub screen: (i32, i32),
    pub absolute: (i32, i32),
}

/// Normalised 0..=65535 coordinate of the centre of pixel `p` on an axis of `size` pixels.
/// Windows maps a normalised `n` back to pixel `floor(n * size / 65536)`, so the centre always
/// lands on `p` itself.
pub(crate) fn normalise_axis(p: i64, size: i64) -> i32 {
    ((2 * p + 1) * 65536 / (2 * size)) as i32
}

/// The pixel Windows moves to for a normalised coordinate (the inverse, for tests and checks).
pub(crate) fn denormalise_axis(n: i32, size: i64) -> i64 {
    i64::from(n) * size / 65536
}

impl Geometry {
    /// A client point (game pixels) as a screen target, refusing points outside the client
    /// area or the virtual desktop.
    pub(crate) fn target(&self, x: f64, y: f64) -> Result<Target> {
        if !(x.is_finite() && y.is_finite())
            || x < 0.0
            || y < 0.0
            || x.round() >= f64::from(self.client.0)
            || y.round() >= f64::from(self.client.1)
        {
            return Err(Error::new(
                "invalid_arguments",
                "Point outside the owned game client area",
            ));
        }
        self.screen_target(
            self.origin.0 + x.round() as i32,
            self.origin.1 + y.round() as i32,
        )
    }
    fn screen_target(&self, sx: i32, sy: i32) -> Result<Target> {
        let (vx, vy, vw, vh) = self.desk;
        let (dx, dy) = (i64::from(sx) - i64::from(vx), i64::from(sy) - i64::from(vy));
        if !(0..i64::from(vw)).contains(&dx) || !(0..i64::from(vh)).contains(&dy) {
            return Err(Error::new(
                "input_unavailable",
                "Point is off the virtual desktop",
            ));
        }
        Ok(Target {
            screen: (sx, sy),
            absolute: (
                normalise_axis(dx, i64::from(vw)),
                normalise_axis(dy, i64::from(vh)),
            ),
        })
    }
    /// The target a fraction `t` of the way from `a` to `b` (screen pixels, rounded).
    fn between(&self, a: Target, b: Target, t: f64) -> Result<Target> {
        let lerp =
            |p: i32, q: i32| (f64::from(p) + (f64::from(q) - f64::from(p)) * t).round() as i32;
        self.screen_target(lerp(a.screen.0, b.screen.0), lerp(a.screen.1, b.screen.1))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    Left,
    Right,
}

/// One `SendInput` event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// Absolute move to a normalised virtual-desktop coordinate.
    Move(i32, i32),
    Button(Button, bool),
    Wheel(i32),
    Key(KeyCode, bool),
}

/// A check run between events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fence {
    /// Ownership, window state, size, scaling and foreground (plus action guards).
    Ready,
    /// No key or button held by anybody.
    Idle,
    /// `Ready`, and the window under this screen pixel is the owned window.
    Point(i32, i32),
    /// The caller's check after the cursor settled and before the press.
    BeforeDown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    Fence(Fence),
    Send(Event),
    Wait(u64),
}

fn modifier_down(plan: &mut Vec<Step>, modifier: Option<KeyCode>) {
    if let Some(m) = modifier {
        plan.push(Step::Send(Event::Key(m, true)));
        plan.push(Step::Wait(MODIFIER_LEAD));
    }
}
fn modifier_up(plan: &mut Vec<Step>, modifier: Option<KeyCode>) {
    if let Some(m) = modifier {
        plan.push(Step::Wait(MODIFIER_LEAD));
        plan.push(Step::Send(Event::Key(m, false)));
    }
}

/// One key press, optionally inside a held modifier.
pub(crate) fn tap_plan(key: KeyCode, modifier: Option<KeyCode>) -> Vec<Step> {
    let mut plan = vec![Step::Fence(Fence::Ready), Step::Fence(Fence::Idle)];
    modifier_down(&mut plan, modifier);
    plan.extend([
        Step::Send(Event::Key(key, true)),
        Step::Wait(PRESS_HOLD),
        Step::Send(Event::Key(key, false)),
    ]);
    modifier_up(&mut plan, modifier);
    plan.push(Step::Wait(KEY_AFTER));
    plan
}

/// Move, settle, checks, optional modifier, press, release. The cursor stays at the point.
pub(crate) fn click_plan(at: Target, button: Button, modifier: Option<KeyCode>) -> Vec<Step> {
    let mut plan = vec![
        Step::Fence(Fence::Ready),
        Step::Fence(Fence::Idle),
        Step::Fence(Fence::Point(at.screen.0, at.screen.1)),
        Step::Send(Event::Move(at.absolute.0, at.absolute.1)),
        Step::Wait(SETTLE),
        Step::Fence(Fence::Ready),
        Step::Fence(Fence::BeforeDown),
        Step::Fence(Fence::Point(at.screen.0, at.screen.1)),
    ];
    modifier_down(&mut plan, modifier);
    plan.extend([
        Step::Send(Event::Button(button, true)),
        Step::Wait(PRESS_HOLD),
        Step::Send(Event::Button(button, false)),
    ]);
    modifier_up(&mut plan, modifier);
    plan.push(Step::Wait(CLICK_AFTER));
    plan
}

/// How a drag stroke is cut into moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stroke {
    /// Ten moves 20 ms apart (the two-point drag of the Linux backend).
    Straight,
    /// One move per about 48 px of each leg, 2 to 16 per leg, 30 ms apart, with a pause at
    /// every bend (the path drag of the macOS backend).
    Path,
}

/// A button drag through every point, held all the way. Every vertex is checked for
/// occlusion before the button goes down; the button is released whatever happens after.
pub(crate) fn drag_plan(
    geometry: &Geometry,
    points: &[(f64, f64)],
    button: Button,
    stroke: Stroke,
) -> Result<Vec<Step>> {
    if points.len() < 2 || points.len() > MAX_PATH_POINTS {
        return Err(Error::new(
            "invalid_arguments",
            "A drag needs 2 to 512 points",
        ));
    }
    let targets = points
        .iter()
        .map(|p| geometry.target(p.0, p.1))
        .collect::<Result<Vec<_>>>()?;
    let start = targets[0];
    let mut plan = vec![Step::Fence(Fence::Ready), Step::Fence(Fence::Idle)];
    plan.extend(
        targets
            .iter()
            .map(|t| Step::Fence(Fence::Point(t.screen.0, t.screen.1))),
    );
    plan.extend([
        Step::Send(Event::Move(start.absolute.0, start.absolute.1)),
        Step::Wait(SETTLE),
        Step::Fence(Fence::Ready),
        Step::Fence(Fence::BeforeDown),
        Step::Fence(Fence::Point(start.screen.0, start.screen.1)),
        Step::Send(Event::Button(button, true)),
        Step::Wait(SETTLE),
    ]);
    for (n, leg) in targets.windows(2).enumerate() {
        let (a, b) = (leg[0], leg[1]);
        let (steps, pause) = match stroke {
            Stroke::Straight => (10, DRAG_STEP),
            Stroke::Path => {
                let (dx, dy) = (
                    f64::from(b.screen.0 - a.screen.0),
                    f64::from(b.screen.1 - a.screen.1),
                );
                (
                    ((dx.hypot(dy) / 48.0).ceil() as u32).clamp(2, 16),
                    PATH_STEP,
                )
            }
        };
        if n > 0 {
            plan.push(Step::Wait(pause));
        }
        for step in 1..=steps {
            let p = geometry.between(a, b, f64::from(step) / f64::from(steps))?;
            plan.extend([
                Step::Fence(Fence::Ready),
                Step::Send(Event::Move(p.absolute.0, p.absolute.1)),
                Step::Wait(pause),
            ]);
        }
    }
    plan.extend([
        Step::Wait(SETTLE),
        Step::Send(Event::Button(button, false)),
        Step::Wait(DRAG_AFTER),
    ]);
    Ok(plan)
}

/// Wheel scroll at a point: `notches(pixels)` events 40 ms apart.
pub(crate) fn scroll_plan(at: Target, pixels: i32) -> Vec<Step> {
    let (delta, count) = notches(pixels);
    wheel_plan(at, delta, count, (SETTLE, 40, 120))
}

/// `count` (at most 64) one-notch wheel events in one quick flick.
pub(crate) fn burst_plan(at: Target, pixels: i32, count: u32) -> Vec<Step> {
    let (delta, _) = notches(pixels);
    wheel_plan(at, delta, count.min(MAX_BURST), (40, 8, 250))
}

fn wheel_plan(
    at: Target,
    delta: i32,
    count: u32,
    (settle, gap, after): (u64, u64, u64),
) -> Vec<Step> {
    let mut plan = vec![
        Step::Fence(Fence::Ready),
        Step::Fence(Fence::Idle),
        Step::Fence(Fence::Point(at.screen.0, at.screen.1)),
        Step::Send(Event::Move(at.absolute.0, at.absolute.1)),
        Step::Wait(settle),
        Step::Fence(Fence::Point(at.screen.0, at.screen.1)),
    ];
    for _ in 0..count {
        plan.extend([
            Step::Fence(Fence::Ready),
            Step::Send(Event::Wheel(delta)),
            Step::Wait(gap),
        ]);
    }
    plan.push(Step::Wait(after));
    plan
}

/// Something down that must come back up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    Key(KeyCode),
    Button(Button),
}
fn release_event(held: Held) -> Event {
    match held {
        Held::Key(k) => Event::Key(k, false),
        Held::Button(b) => Event::Button(b, false),
    }
}

/// Run a plan. A down whose send was attempted counts as held even if the send failed (it may
/// have been delivered). On any failure every held key and button is released in reverse
/// order (Shift/Ctrl/Alt after `MODIFIER_LEAD`), each up retried once, and the first error is
/// returned. A delivery is never retried.
pub(crate) fn run(
    plan: &[Step],
    mut fence: impl FnMut(Fence) -> Result<()>,
    mut send: impl FnMut(Event) -> Result<()>,
    mut wait: impl FnMut(Duration),
) -> Result<()> {
    fn up(send: &mut impl FnMut(Event) -> Result<()>, event: Event) -> Result<()> {
        send(event).or_else(|_| send(event))
    }
    let mut held: Vec<Held> = Vec::new();
    let mut failure = None;
    for step in plan {
        let result = match *step {
            Step::Fence(f) => fence(f),
            Step::Wait(ms) => {
                wait(Duration::from_millis(ms));
                Ok(())
            }
            Step::Send(event) => {
                let pressed = match event {
                    Event::Key(k, down) => Some((Held::Key(k), down)),
                    Event::Button(b, down) => Some((Held::Button(b), down)),
                    _ => None,
                };
                match pressed {
                    Some((h, true)) => {
                        held.push(h);
                        send(event)
                    }
                    Some((h, false)) => {
                        held.retain(|v| *v != h);
                        up(&mut send, event)
                    }
                    None => send(event),
                }
            }
        };
        if let Err(e) = result {
            failure = Some(e);
            break;
        }
    }
    let Some(error) = failure else {
        return Ok(());
    };
    let mut released_any = false;
    while let Some(h) = held.pop() {
        if released_any && matches!(h, Held::Key(k) if k == SHIFT || k == CONTROL || k == ALT) {
            wait(Duration::from_millis(MODIFIER_LEAD));
        }
        let _ = up(&mut send, release_event(h));
        released_any = true;
    }
    Err(error)
}

// ---------------------------------------------------------------------------------------------
// Win32.
// ---------------------------------------------------------------------------------------------

type Hwnd = isize;
type Hdc = isize;
type Hgdi = isize;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Rect32 {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Point32 {
    x: i32,
    y: i32,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct MouseInput {
    dx: i32,
    dy: i32,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct KeybdInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
union InputData {
    mouse: MouseInput,
    key: KeybdInput,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Input {
    kind: u32,
    data: InputData,
}
#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_per_meter: i32,
    y_per_meter: i32,
    used: u32,
    important: u32,
}
#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 1],
}

const INPUT_MOUSE: u32 = 0;
const INPUT_KEYBOARD: u32 = 1;
const MOUSEEVENTF_MOVE: u32 = 0x0001;
const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
const MOUSEEVENTF_WHEEL: u32 = 0x0800;
const MOUSEEVENTF_VIRTUALDESK: u32 = 0x4000;
const MOUSEEVENTF_ABSOLUTE: u32 = 0x8000;
const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_SCANCODE: u32 = 0x0008;
const SM_XVIRTUALSCREEN: i32 = 76;
const SM_YVIRTUALSCREEN: i32 = 77;
const SM_CXVIRTUALSCREEN: i32 = 78;
const SM_CYVIRTUALSCREEN: i32 = 79;
const GA_ROOT: u32 = 2;
const PW_CLIENTONLY: u32 = 1;
const PW_RENDERFULLCONTENT: u32 = 2;
const SRCCOPY: u32 = 0x00CC_0020;
const DIB_RGB_COLORS: u32 = 0;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowThreadProcessId(window: Hwnd, pid: *mut u32) -> u32;
    fn IsWindow(window: Hwnd) -> i32;
    fn IsWindowVisible(window: Hwnd) -> i32;
    fn IsIconic(window: Hwnd) -> i32;
    fn GetAncestor(window: Hwnd, flags: u32) -> Hwnd;
    fn GetClientRect(window: Hwnd, rect: *mut Rect32) -> i32;
    fn ClientToScreen(window: Hwnd, point: *mut Point32) -> i32;
    fn GetForegroundWindow() -> Hwnd;
    fn WindowFromPoint(point: Point32) -> Hwnd;
    fn GetDpiForWindow(window: Hwnd) -> u32;
    fn GetAsyncKeyState(key: i32) -> i16;
    fn GetCursorPos(point: *mut Point32) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn GetDC(window: Hwnd) -> Hdc;
    fn ReleaseDC(window: Hwnd, dc: Hdc) -> i32;
    fn PrintWindow(window: Hwnd, dc: Hdc, flags: u32) -> i32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(dc: Hdc) -> Hdc;
    fn CreateCompatibleBitmap(dc: Hdc, width: i32, height: i32) -> Hgdi;
    fn SelectObject(dc: Hdc, object: Hgdi) -> Hgdi;
    fn DeleteObject(object: Hgdi) -> i32;
    fn DeleteDC(dc: Hdc) -> i32;
    #[allow(clippy::too_many_arguments)]
    fn BitBlt(
        dst: Hdc,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        src: Hdc,
        src_x: i32,
        src_y: i32,
        rop: u32,
    ) -> i32;
    fn GetDIBits(
        dc: Hdc,
        bitmap: Hgdi,
        start: u32,
        lines: u32,
        bits: *mut c_void,
        info: *mut BitmapInfo,
        usage: u32,
    ) -> i32;
}

/// The native `INPUT` of an event.
fn native_input(event: Event) -> Input {
    let mouse = |dx: i32, dy: i32, mouse_data: u32, flags: u32| Input {
        kind: INPUT_MOUSE,
        data: InputData {
            mouse: MouseInput {
                dx,
                dy,
                mouse_data,
                flags,
                time: 0,
                extra: 0,
            },
        },
    };
    match event {
        Event::Move(x, y) => mouse(
            x,
            y,
            0,
            MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
        ),
        Event::Button(button, down) => mouse(
            0,
            0,
            0,
            match (button, down) {
                (Button::Left, true) => MOUSEEVENTF_LEFTDOWN,
                (Button::Left, false) => MOUSEEVENTF_LEFTUP,
                (Button::Right, true) => MOUSEEVENTF_RIGHTDOWN,
                (Button::Right, false) => MOUSEEVENTF_RIGHTUP,
            },
        ),
        // A negative delta travels as its two's complement, as Win32 expects.
        Event::Wheel(delta) => mouse(0, 0, delta as u32, MOUSEEVENTF_WHEEL),
        Event::Key(k, down) => Input {
            kind: INPUT_KEYBOARD,
            data: InputData {
                key: KeybdInput {
                    vk: k.vk,
                    scan: k.scan,
                    flags: KEYEVENTF_SCANCODE
                        | if k.extended { KEYEVENTF_EXTENDEDKEY } else { 0 }
                        | if down { 0 } else { KEYEVENTF_KEYUP },
                    time: 0,
                    extra: 0,
                },
            },
        },
    }
}

fn os_error(code: &'static str, label: &str) -> Error {
    Error::new(
        code,
        format!("{label}: {}", std::io::Error::last_os_error()),
    )
}

fn send_event(event: Event) -> Result<()> {
    let input = native_input(event);
    // SAFETY: one initialised INPUT whose size is passed as cbSize.
    let sent = unsafe { SendInput(1, &input, size_of::<Input>() as i32) };
    if sent != 1 {
        // UIPI (a higher-integrity foreground window) or a locked desktop blocks the event.
        return Err(os_error("input_unavailable", "SendInput refused the event"));
    }
    Ok(())
}

fn window_pid(window: Hwnd) -> u32 {
    let mut pid = 0u32;
    // SAFETY: any window value is accepted; an invalid one returns 0 and leaves pid at 0.
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    pid
}

fn client_size(window: Hwnd) -> Result<(u32, u32)> {
    let mut rect = Rect32::default();
    // SAFETY: one writable RECT; an invalid window fails and is reported.
    if unsafe { GetClientRect(window, &mut rect) } == 0 {
        return Err(os_error("input_unavailable", "GetClientRect"));
    }
    let width = u32::try_from(rect.right - rect.left).unwrap_or(0);
    let height = u32::try_from(rect.bottom - rect.top).unwrap_or(0);
    Ok((width, height))
}

/// Frees the capture's GDI objects in reverse order of creation.
struct Gdi {
    window: Hwnd,
    window_dc: Hdc,
    memory_dc: Hdc,
    bitmap: Hgdi,
    previous: Hgdi,
}
impl Drop for Gdi {
    fn drop(&mut self) {
        // SAFETY: each handle is either 0 (skipped) or was created above and is released once.
        unsafe {
            if self.memory_dc != 0 && self.previous != 0 {
                SelectObject(self.memory_dc, self.previous);
            }
            if self.bitmap != 0 {
                DeleteObject(self.bitmap);
            }
            if self.memory_dc != 0 {
                DeleteDC(self.memory_dc);
            }
            if self.window_dc != 0 {
                ReleaseDC(self.window, self.window_dc);
            }
        }
    }
}

/// Top-down 32-bit BGRA pixels of the client area. `PrintWindow` with
/// `PW_RENDERFULLCONTENT` asks the compositor for the window's own content (a DirectX
/// flip-model swap chain reads black through a plain `BitBlt`); `BitBlt` from the client DC is
/// the fallback. An all-black frame is refused rather than returned.
fn capture_client(window: Hwnd, width: u32, height: u32) -> Result<Vec<u8>> {
    let (w, h) = (width as i32, height as i32);
    let mut gdi = Gdi {
        window,
        window_dc: 0,
        memory_dc: 0,
        bitmap: 0,
        previous: 0,
    };
    // SAFETY: the owned window handle was verified by the caller; a failure returns 0.
    gdi.window_dc = unsafe { GetDC(window) };
    if gdi.window_dc == 0 {
        return Err(os_error("capture_unavailable", "GetDC"));
    }
    // SAFETY: a valid window DC; failures return 0 and are checked.
    unsafe {
        gdi.memory_dc = CreateCompatibleDC(gdi.window_dc);
        if gdi.memory_dc == 0 {
            return Err(os_error("capture_unavailable", "CreateCompatibleDC"));
        }
        gdi.bitmap = CreateCompatibleBitmap(gdi.window_dc, w, h);
        if gdi.bitmap == 0 {
            return Err(os_error("capture_unavailable", "CreateCompatibleBitmap"));
        }
        gdi.previous = SelectObject(gdi.memory_dc, gdi.bitmap);
    }
    // SAFETY: the memory DC holds a w x h bitmap; both calls only draw into it.
    let drawn = unsafe {
        PrintWindow(window, gdi.memory_dc, PW_CLIENTONLY | PW_RENDERFULLCONTENT) != 0
            || BitBlt(gdi.memory_dc, 0, 0, w, h, gdi.window_dc, 0, 0, SRCCOPY) != 0
    };
    if !drawn {
        return Err(os_error("capture_unavailable", "PrintWindow and BitBlt"));
    }
    // SAFETY: GetDIBits needs the bitmap deselected; the previous object goes back first.
    unsafe { SelectObject(gdi.memory_dc, gdi.previous) };
    gdi.previous = 0;
    let mut info = BitmapInfo {
        header: BitmapInfoHeader {
            size: size_of::<BitmapInfoHeader>() as u32,
            width: w,
            height: -h,
            planes: 1,
            bit_count: 32,
            compression: 0,
            size_image: 0,
            x_per_meter: 0,
            y_per_meter: 0,
            used: 0,
            important: 0,
        },
        colors: [0],
    };
    let mut data = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: the buffer holds exactly `height` top-down rows of `width` 32-bit pixels, the
    // format the header requests.
    let lines = unsafe {
        GetDIBits(
            gdi.memory_dc,
            gdi.bitmap,
            0,
            height,
            data.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    if lines != h {
        return Err(os_error("capture_unavailable", "GetDIBits"));
    }
    if data.chunks_exact(4).all(|p| p[..3] == [0, 0, 0]) {
        return Err(Error::new(
            "capture_unavailable",
            "The window returned a black frame (its DirectX content was not captured)",
        ));
    }
    Ok(data)
}

// ---------------------------------------------------------------------------------------------
// The backend.
// ---------------------------------------------------------------------------------------------

struct Shared {
    window: Hwnd,
    pid: u32,
    client: (u32, u32),
    identity: Option<Identity>,
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

pub(crate) struct WinInput<'p> {
    shared: Arc<Shared>,
    process: Binding<'p>,
    guards: Vec<&'p (dyn Fn() -> Result<()> + Send + Sync)>,
}

impl WinInput<'static> {
    /// Bind the owned game's top-level window. Checks the window belongs to `game_pid`, is
    /// visible, not minimised, unscaled and has exactly the `client` size. Never activates,
    /// moves or resizes the window and never moves the pointer.
    pub(crate) fn open_owned(
        process: Arc<dyn Process + Send + Sync>,
        window: isize,
        game_pid: u32,
        client: (u32, u32),
    ) -> Result<Self> {
        if window == 0 || game_pid == 0 {
            return Err(Error::new(
                "invalid_input_binding",
                "Owned window and game PID required",
            ));
        }
        if !(1..=MAX_CLIENT).contains(&client.0) || !(1..=MAX_CLIENT).contains(&client.1) {
            return Err(Error::new(
                "invalid_input_binding",
                "Bounded owned client dimensions required",
            ));
        }
        process.check()?;
        // The Windows process exposes no Unix-style identity (its owned reader holds the
        // creation time and SID and checks them in `check`); when one is present it must name
        // the same PID.
        let identity = process.identity().cloned();
        if identity
            .as_ref()
            .is_some_and(|i| u32::try_from(i.pid).ok() != Some(game_pid))
        {
            return Err(Error::new(
                "ownership_changed",
                "Input process identity is not the game PID",
            ));
        }
        // SAFETY: any window value is accepted; an invalid one returns 0.
        if unsafe { GetAncestor(window, GA_ROOT) } != window {
            return Err(Error::new(
                "invalid_input_binding",
                "The bound window must be a top-level window",
            ));
        }
        let input = Self {
            shared: Arc::new(Shared {
                window,
                pid: game_pid,
                client,
                identity,
            }),
            process: Binding::Owned(process),
            guards: Vec::new(),
        };
        input.bound()?;
        Ok(input)
    }
}

impl WinInput<'_> {
    /// Ownership and window state, without the foreground requirement.
    fn bound(&self) -> Result<()> {
        let s = &self.shared;
        let process = self.process.get();
        process.check()?;
        if process.identity() != s.identity.as_ref() {
            return Err(Error::new(
                "ownership_changed",
                "Input process identity changed",
            ));
        }
        // SAFETY: plain window queries; an invalid window returns 0.
        let (alive, visible, minimised) = unsafe {
            (
                IsWindow(s.window),
                IsWindowVisible(s.window),
                IsIconic(s.window),
            )
        };
        if alive == 0 || window_pid(s.window) != s.pid {
            return Err(Error::new(
                "ownership_changed",
                "Window is not owned by the game process",
            ));
        }
        if visible == 0 || minimised != 0 {
            return Err(Error::new(
                "input_unavailable",
                "Owned window is hidden or minimised",
            ));
        }
        if client_size(s.window)? != s.client {
            return Err(Error::new(
                "input_geometry_changed",
                "Owned client dimensions differ from the bound size",
            ));
        }
        // SAFETY: plain window query; 0 for an invalid window.
        if unsafe { GetDpiForWindow(s.window) } != DEFAULT_DPI {
            return Err(Error::new(
                "input_geometry_unverified",
                "Owned window is not at 100% scaling; coordinates would be virtualised",
            ));
        }
        Ok(())
    }
    /// Ownership, window state and foreground, then every action fence.
    fn check(&self) -> Result<()> {
        self.bound()?;
        // SAFETY: no arguments; returns 0 when no window is foreground.
        if unsafe { GetForegroundWindow() } != self.shared.window {
            return Err(Error::new(
                "input_target_not_frontmost",
                "Owned window is not the foreground window; no activation fallback",
            ));
        }
        for guard in &self.guards {
            guard()?;
        }
        Ok(())
    }
    /// No key or mouse button is held, by anybody.
    fn idle(&self) -> Result<()> {
        for key in 1..=0xFE {
            // SAFETY: any virtual-key value is accepted.
            if unsafe { GetAsyncKeyState(key) } < 0 {
                return Err(Error::new(
                    "input_busy",
                    "A key, button or modifier is held",
                ));
            }
        }
        Ok(())
    }
    /// The window under a screen pixel is the owned window or one of its children.
    fn unoccluded(&self, x: i32, y: i32) -> Result<()> {
        // SAFETY: POINT by value; returns 0 when no window is there.
        let hit = unsafe { WindowFromPoint(Point32 { x, y }) };
        // SAFETY: any window value is accepted; an invalid one returns 0.
        if hit == 0
            || (hit != self.shared.window
                && unsafe { GetAncestor(hit, GA_ROOT) } != self.shared.window)
        {
            return Err(Error::new(
                "input_point_occluded",
                "Another window covers the target point",
            ));
        }
        Ok(())
    }
    fn geometry(&self, game: (u32, u32)) -> Result<Geometry> {
        let s = &self.shared;
        if game != s.client {
            return Err(Error::new(
                "input_unavailable",
                "Game size differs from the owned client size",
            ));
        }
        let mut origin = Point32::default();
        // SAFETY: one writable POINT.
        if unsafe { ClientToScreen(s.window, &mut origin) } == 0 {
            return Err(os_error("input_unavailable", "ClientToScreen"));
        }
        // SAFETY: plain metric queries.
        let desk = unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN),
            )
        };
        Ok(Geometry {
            origin: (origin.x, origin.y),
            client: s.client,
            desk,
        })
    }
    fn fence(&self, fence: Fence, before_down: &dyn Fn() -> Result<()>) -> Result<()> {
        match fence {
            Fence::Ready => self.check(),
            Fence::Idle => self.idle(),
            Fence::Point(x, y) => {
                self.check()?;
                self.unoccluded(x, y)
            }
            Fence::BeforeDown => before_down(),
        }
    }
    fn execute(&self, plan: &[Step], before_down: &dyn Fn() -> Result<()>) -> Result<()> {
        run(plan, |f| self.fence(f, before_down), send_event, sleep)
    }
    fn tap(&self, code: u16, modifier: Option<KeyCode>) -> Result<()> {
        refuse_chord(modifier, code)?;
        let key = key_code(code)
            .ok_or_else(|| Error::new("invalid_arguments", "Key has no Windows equivalent"))?;
        self.execute(&tap_plan(key, modifier), &|| Ok(()))
    }
    fn click_with(
        &self,
        game: (u32, u32),
        x: f64,
        y: f64,
        button: Button,
        modifier: Option<KeyCode>,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<Point> {
        self.check()?;
        let at = self.geometry(game)?.target(x, y)?;
        self.execute(&click_plan(at, button, modifier), before_down)?;
        Ok(Point { x, y })
    }
    fn drag_with(
        &self,
        game: (u32, u32),
        points: &[(f64, f64)],
        button: Button,
        stroke: Stroke,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.check()?;
        let plan = drag_plan(&self.geometry(game)?, points, button, stroke)?;
        self.execute(&plan, before_down)
    }
    fn once(&self, code: u16) -> key_once::Dispatch {
        struct Pair {
            key: KeyCode,
        }
        impl key_once::Pair for Pair {
            fn down(&mut self) -> Result<()> {
                send_event(Event::Key(self.key, true))
            }
            fn up(&mut self) -> Result<()> {
                send_event(Event::Key(self.key, false))
            }
        }
        key_once::dispatch(
            || self.confirm_input_idle(),
            || {
                Ok(Pair {
                    key: key_code(code).ok_or_else(|| {
                        Error::new("invalid_arguments", "Key has no Windows equivalent")
                    })?,
                })
            },
            sleep,
        )
    }
}

impl GameInput for WinInput<'_> {
    fn capture(&self, game: (u32, u32)) -> Result<Capture> {
        // A capture sends no input, so it needs ownership and window state but not the
        // foreground (PrintWindow reads the window's own content).
        self.bound()?;
        let (w, h) = self.shared.client;
        if game != (w, h) {
            return Err(Error::new(
                "input_unavailable",
                "Game size differs from the owned client size",
            ));
        }
        let data = capture_client(self.shared.window, w, h)?;
        Ok(Capture {
            width: w as usize,
            height: h as usize,
            stride: w as usize * 4,
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
        self.click_with(game, x, y, Button::Left, None, &|| Ok(()))
    }
    fn click_checked(
        &self,
        game: (u32, u32),
        x: f64,
        y: f64,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<Point> {
        self.click_with(game, x, y, Button::Left, None, before_down)
    }
    fn confirm_cursor_at(&self, game: (u32, u32), x: f64, y: f64) -> Result<()> {
        self.check()?;
        let expected = self.geometry(game)?.target(x, y)?.screen;
        let mut cursor = Point32::default();
        // SAFETY: one writable POINT.
        if unsafe { GetCursorPos(&mut cursor) } == 0 {
            return Err(os_error("input_unavailable", "GetCursorPos"));
        }
        if (cursor.x - expected.0).abs() > 1 || (cursor.y - expected.1).abs() > 1 {
            return Err(Error::new(
                "cursor_moved",
                "Pointer is not at the expected game point",
            ));
        }
        Ok(())
    }
    fn drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.drag_with(
            game,
            &[from, to],
            Button::Left,
            Stroke::Straight,
            &|| Ok(()),
        )
    }
    fn drag_checked(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.drag_with(
            game,
            &[from, to],
            Button::Left,
            Stroke::Straight,
            before_down,
        )
    }
    fn shift_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.click_with(game, x, y, Button::Left, Some(SHIFT), &|| Ok(()))
    }
    fn ctrl_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.click_with(game, x, y, Button::Left, Some(CONTROL), &|| Ok(()))
    }
    // cmd_click keeps the trait's refusal: Windows has no Command key.
    fn right_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.click_with(game, x, y, Button::Right, None, &|| Ok(()))
    }
    fn right_drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.drag_with(game, &[from, to], Button::Right, Stroke::Straight, &|| {
            Ok(())
        })
    }
    fn right_drag_path(&self, game: (u32, u32), points: &[(f64, f64)]) -> Result<()> {
        self.drag_with(game, points, Button::Right, Stroke::Path, &|| Ok(()))
    }
    fn drag_path(&self, game: (u32, u32), points: &[(f64, f64)]) -> Result<()> {
        self.drag_with(game, points, Button::Left, Stroke::Path, &|| Ok(()))
    }
    fn scroll(&self, game: (u32, u32), at: (f64, f64), pixels: i32) -> Result<()> {
        self.check()?;
        let target = self.geometry(game)?.target(at.0, at.1)?;
        self.execute(&scroll_plan(target, pixels), &|| Ok(()))
    }
    fn scroll_burst(
        &self,
        game: (u32, u32),
        at: (f64, f64),
        pixels: i32,
        count: u32,
    ) -> Result<()> {
        self.check()?;
        let target = self.geometry(game)?.target(at.0, at.1)?;
        self.execute(&burst_plan(target, pixels, count), &|| Ok(()))
    }
    fn press_code(&self, code: u16) -> Result<()> {
        self.tap(code, None)
    }
    fn press_shifted(&self, code: u16) -> Result<()> {
        self.tap(code, Some(SHIFT))
    }
    fn press_control(&self, code: u16) -> Result<()> {
        self.tap(code, Some(CONTROL))
    }
    /// Alt held around one key. Windows has no Option dead keys: Alt+U reaches the game as
    /// Alt+U and types no umlaut. Shell chords (Alt+F4, Alt+Tab, ...) are refused.
    fn press_option(&self, code: u16) -> Result<()> {
        self.tap(code, Some(ALT))
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
        Ok(Box::new(WinInput {
            shared: self.shared.clone(),
            process: Binding::Borrowed(process),
            guards,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn every_neutral_key_has_a_windows_key_and_scan_code() {
        let named = [
            (vk::RETURN, key(0x0D, 0x1C)),
            (vk::TAB, key(0x09, 0x0F)),
            (vk::SPACE, key(0x20, 0x39)),
            (vk::DELETE, key(0x08, 0x0E)),
            (vk::ESCAPE, key(0x1B, 0x01)),
            (vk::KEYPAD_PLUS, key(0x6B, 0x4E)),
            (vk::KEYPAD_MINUS, key(0x6D, 0x4A)),
            (vk::F1, key(0x70, 0x3B)),
            (vk::F2, key(0x71, 0x3C)),
            (vk::F3, key(0x72, 0x3D)),
            (vk::F4, key(0x73, 0x3E)),
            (vk::F5, key(0x74, 0x3F)),
            (vk::HOME, extended(0x24, 0x47)),
            (vk::END, extended(0x23, 0x4F)),
            (vk::FORWARD_DELETE, extended(0x2E, 0x53)),
            (vk::LEFT, extended(0x25, 0x4B)),
            (vk::RIGHT, extended(0x27, 0x4D)),
            (vk::DOWN, extended(0x28, 0x50)),
            (vk::UP, extended(0x26, 0x48)),
        ];
        for (code, expected) in named {
            assert_eq!(key_code(code), Some(expected), "{code}");
        }
        // Letters: VK 'A'..'Z' and the US scan row positions.
        let rows = ["qwertyuiop", "asdfghjkl", "zxcvbnm"];
        let starts = [0x10u16, 0x1E, 0x2C];
        for (row, start) in rows.iter().zip(starts) {
            for (i, c) in row.chars().enumerate() {
                let (code, shift) = vk::for_char(c).unwrap();
                assert!(!shift);
                let k = key_code(code).unwrap();
                assert_eq!(k.vk, c.to_ascii_uppercase() as u16, "{c}");
                assert_eq!(k.scan, start + i as u16, "{c}");
                assert!(!k.extended);
            }
        }
        assert_eq!(key_code(vk::A), Some(key(0x41, 0x1E)));
        // Digits: VK '0'..'9', scan 1 = 0x02 .. 9 = 0x0A, 0 = 0x0B.
        for d in 0..10u16 {
            let k = key_code(vk::DIGITS[d as usize]).unwrap();
            assert_eq!(k.vk, 0x30 + d);
            assert_eq!(k.scan, if d == 0 { 0x0B } else { 0x01 + d });
        }
        for (c, code) in vk::PUNCTUATION {
            let k = key_code(code).unwrap_or_else(|| panic!("{c}"));
            assert!((0xBA..=0xDE).contains(&k.vk) && !k.extended, "{c}");
        }
        assert_eq!(
            key_code(vk::for_char('/').unwrap().0),
            Some(key(0xBF, 0x35))
        );
        // Every code the neutral table can produce maps, and the scan codes are distinct.
        let mut scans = std::collections::HashSet::new();
        for c in ('a'..='z').chain('0'..='9').chain("-=[];',./\\` ".chars()) {
            let k = key_code(vk::for_char(c).unwrap().0).unwrap();
            assert!(scans.insert(k.scan), "{c}");
        }
        assert_eq!(key_code(200), None);
        assert_eq!(key_code(vk::KEYPAD_PLUS).map(|k| k.extended), Some(false));
    }

    #[test]
    fn shell_chords_are_refused_before_input() {
        for code in [vk::TAB, vk::SPACE, vk::ESCAPE, vk::F4, vk::RETURN] {
            assert!(refuse_chord(Some(ALT), code).is_err(), "{code}");
        }
        assert!(refuse_chord(Some(CONTROL), vk::ESCAPE).is_err());
        assert!(refuse_chord(Some(ALT), vk::LETTERS[20]).is_ok());
        assert!(refuse_chord(Some(SHIFT), vk::TAB).is_ok());
        assert!(refuse_chord(None, vk::ESCAPE).is_ok());
    }

    #[test]
    fn wheel_notches() {
        assert_eq!(notches(-240), (-120, 2));
        assert_eq!(notches(240), (120, 2));
        assert_eq!(notches(30), (120, 1));
        assert_eq!(notches(-121), (-120, 2));
        assert_eq!(notches(0), (-120, 1));
    }

    #[test]
    fn absolute_coordinates_land_on_the_pixel() {
        for size in [1i64, 2, 3, 1080, 1920, 2560, 3840, 5760, 7680] {
            for p in (0..size)
                .step_by((size as usize / 97).max(1))
                .chain([size - 1])
            {
                let n = normalise_axis(p, size);
                assert!((0..=65535).contains(&n), "{size} {p}");
                assert_eq!(denormalise_axis(n, size), p, "{size} {p}");
            }
        }
        // A second monitor left of the primary: the virtual desktop starts at -1920.
        let g = Geometry {
            origin: (100, 50),
            client: (1920, 1080),
            desk: (-1920, 0, 3840, 1080),
        };
        let t = g.target(0.0, 0.0).unwrap();
        assert_eq!(t.screen, (100, 50));
        assert_eq!(denormalise_axis(t.absolute.0, 3840) - 1920, 100);
        assert_eq!(denormalise_axis(t.absolute.1, 1080), 50);
        let g = Geometry {
            origin: (0, 0),
            client: (1920, 1080),
            desk: (0, 0, 1920, 1080),
        };
        assert_eq!(g.target(1919.4, 1079.0).unwrap().screen, (1919, 1079));
        assert_eq!(g.target(1919.0, 0.0).unwrap().absolute.0, 65518);
        for (x, y) in [
            (1920.0, 10.0),
            (1919.6, 10.0),
            (-1.0, 10.0),
            (f64::NAN, 10.0),
            (10.0, 1080.0),
        ] {
            assert_eq!(g.target(x, y).unwrap_err().code, "invalid_arguments");
        }
        // A client partly off the desktop refuses the off-screen part.
        let g = Geometry {
            origin: (1000, 0),
            client: (1920, 1080),
            desk: (0, 0, 1920, 1080),
        };
        assert!(g.target(919.0, 0.0).is_ok());
        assert_eq!(g.target(920.0, 0.0).unwrap_err().code, "input_unavailable");
    }

    #[test]
    fn a_shifted_key_press_holds_shift_around_the_key() {
        let k = key_code(vk::LETTERS[0]).unwrap();
        assert_eq!(
            tap_plan(k, Some(SHIFT)),
            vec![
                Step::Fence(Fence::Ready),
                Step::Fence(Fence::Idle),
                Step::Send(Event::Key(SHIFT, true)),
                Step::Wait(150),
                Step::Send(Event::Key(k, true)),
                Step::Wait(60),
                Step::Send(Event::Key(k, false)),
                Step::Wait(150),
                Step::Send(Event::Key(SHIFT, false)),
                Step::Wait(40),
            ]
        );
        assert_eq!(
            tap_plan(k, None),
            vec![
                Step::Fence(Fence::Ready),
                Step::Fence(Fence::Idle),
                Step::Send(Event::Key(k, true)),
                Step::Wait(60),
                Step::Send(Event::Key(k, false)),
                Step::Wait(40),
            ]
        );
    }

    #[test]
    fn a_shift_click_moves_checks_then_holds_shift_around_the_button() {
        let at = Target {
            screen: (10, 20),
            absolute: (341, 1214),
        };
        assert_eq!(
            click_plan(at, Button::Left, Some(SHIFT)),
            vec![
                Step::Fence(Fence::Ready),
                Step::Fence(Fence::Idle),
                Step::Fence(Fence::Point(10, 20)),
                Step::Send(Event::Move(341, 1214)),
                Step::Wait(60),
                Step::Fence(Fence::Ready),
                Step::Fence(Fence::BeforeDown),
                Step::Fence(Fence::Point(10, 20)),
                Step::Send(Event::Key(SHIFT, true)),
                Step::Wait(150),
                Step::Send(Event::Button(Button::Left, true)),
                Step::Wait(60),
                Step::Send(Event::Button(Button::Left, false)),
                Step::Wait(150),
                Step::Send(Event::Key(SHIFT, false)),
                Step::Wait(60),
            ]
        );
    }

    #[test]
    fn drags_check_every_vertex_before_the_button_and_release_last() {
        let g = Geometry {
            origin: (0, 0),
            client: (1920, 1080),
            desk: (0, 0, 1920, 1080),
        };
        let plan = drag_plan(
            &g,
            &[(100.0, 100.0), (200.0, 100.0)],
            Button::Right,
            Stroke::Straight,
        )
        .unwrap();
        let down = plan
            .iter()
            .position(|s| *s == Step::Send(Event::Button(Button::Right, true)))
            .unwrap();
        assert!(plan[..down].contains(&Step::Fence(Fence::Point(200, 100))));
        assert!(plan[..down].contains(&Step::Fence(Fence::BeforeDown)));
        let moves: Vec<_> = plan[down..]
            .iter()
            .filter_map(|s| match s {
                Step::Send(Event::Move(x, _)) => Some(denormalise_axis(*x, 1920)),
                _ => None,
            })
            .collect();
        assert_eq!(moves, (1..=10).map(|i| 100 + 10 * i).collect::<Vec<_>>());
        assert_eq!(
            plan[plan.len() - 2],
            Step::Send(Event::Button(Button::Right, false))
        );
        // A path: one move per ~48 px of each leg (2..16), a pause at the bend.
        let plan = drag_plan(
            &g,
            &[(0.0, 0.0), (96.0, 0.0), (96.0, 1000.0)],
            Button::Left,
            Stroke::Path,
        )
        .unwrap();
        let moves = plan
            .iter()
            .filter(|s| matches!(s, Step::Send(Event::Move(..))))
            .count();
        assert_eq!(moves, 1 + 2 + 16);
        assert!(drag_plan(&g, &[(0.0, 0.0)], Button::Left, Stroke::Path).is_err());
        assert!(drag_plan(&g, &[(0.0, 0.0), (5000.0, 0.0)], Button::Left, Stroke::Path).is_err());
    }

    #[test]
    fn scrolls_send_one_notch_of_120_per_120_pixels() {
        let at = Target {
            screen: (5, 5),
            absolute: (7, 7),
        };
        let wheels = |plan: Vec<Step>| -> Vec<i32> {
            plan.into_iter()
                .filter_map(|s| match s {
                    Step::Send(Event::Wheel(d)) => Some(d),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(wheels(scroll_plan(at, 300)), vec![120; 3]);
        assert_eq!(wheels(scroll_plan(at, -120)), vec![-120]);
        assert_eq!(wheels(burst_plan(at, -10, 500)), vec![-120; 64]);
    }

    #[test]
    fn native_events_carry_scan_codes_and_absolute_flags() {
        assert_eq!(
            size_of::<Input>(),
            if cfg!(target_pointer_width = "64") {
                40
            } else {
                28
            }
        );
        let up = native_input(Event::Key(key_code(vk::LEFT).unwrap(), false));
        assert_eq!(up.kind, INPUT_KEYBOARD);
        // SAFETY: a keyboard input carries the key variant.
        let k = unsafe { up.data.key };
        assert_eq!((k.vk, k.scan), (0x25, 0x4B));
        assert_eq!(
            k.flags,
            KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP
        );
        let down = native_input(Event::Key(SHIFT, true));
        // SAFETY: a keyboard input carries the key variant.
        assert_eq!(unsafe { down.data.key }.flags, KEYEVENTF_SCANCODE);
        let moved = native_input(Event::Move(100, 200));
        // SAFETY: a mouse input carries the mouse variant.
        let m = unsafe { moved.data.mouse };
        assert_eq!((moved.kind, m.dx, m.dy), (INPUT_MOUSE, 100, 200));
        assert_eq!(m.flags, 0x0001 | 0x8000 | 0x4000);
        // SAFETY: a mouse input carries the mouse variant.
        let wheel = unsafe { native_input(Event::Wheel(-120)).data.mouse };
        assert_eq!(
            (wheel.mouse_data, wheel.flags),
            (0xFFFF_FF88, MOUSEEVENTF_WHEEL)
        );
        // SAFETY: a mouse input carries the mouse variant.
        let right = unsafe { native_input(Event::Button(Button::Right, true)).data.mouse };
        assert_eq!(
            (right.flags, right.dx, right.dy),
            (MOUSEEVENTF_RIGHTDOWN, 0, 0)
        );
    }

    /// Runs a plan against recorders: returns the events sent and the waits taken.
    fn record(
        plan: &[Step],
        fail_fence: Option<Fence>,
        fail_send: &dyn Fn(Event, usize) -> bool,
    ) -> (Result<()>, Vec<Event>, Vec<u64>) {
        let sent = RefCell::new(Vec::new());
        let waits = RefCell::new(Vec::new());
        let result = run(
            plan,
            |f| {
                if Some(f) == fail_fence {
                    Err(Error::new("input_target_not_frontmost", "test"))
                } else {
                    Ok(())
                }
            },
            |e| {
                let n = sent.borrow().iter().filter(|s| **s == e).count();
                sent.borrow_mut().push(e);
                if fail_send(e, n) {
                    Err(Error::new("input_unavailable", "test"))
                } else {
                    Ok(())
                }
            },
            |d| waits.borrow_mut().push(d.as_millis() as u64),
        );
        (result, sent.into_inner(), waits.into_inner())
    }

    #[test]
    fn a_complete_plan_runs_in_order_with_its_waits() {
        let k = key_code(vk::LETTERS[1]).unwrap();
        let (result, sent, waits) = record(&tap_plan(k, Some(CONTROL)), None, &|_, _| false);
        assert!(result.is_ok());
        assert_eq!(
            sent,
            vec![
                Event::Key(CONTROL, true),
                Event::Key(k, true),
                Event::Key(k, false),
                Event::Key(CONTROL, false),
            ]
        );
        assert_eq!(waits, vec![150, 60, 150, 40]);
    }

    #[test]
    fn a_refused_check_releases_only_what_went_down() {
        let at = Target {
            screen: (1, 1),
            absolute: (1, 1),
        };
        // Refuse before anything is sent: nothing goes out.
        let (result, sent, _) = record(
            &click_plan(at, Button::Left, Some(SHIFT)),
            Some(Fence::BeforeDown),
            &|_, _| false,
        );
        assert_eq!(result.unwrap_err().code, "input_target_not_frontmost");
        assert_eq!(sent, vec![Event::Move(1, 1)]);
        // A drag that loses the foreground mid-stroke releases the button.
        let g = Geometry {
            origin: (0, 0),
            client: (100, 100),
            desk: (0, 0, 100, 100),
        };
        let plan = drag_plan(
            &g,
            &[(0.0, 0.0), (50.0, 50.0)],
            Button::Left,
            Stroke::Straight,
        )
        .unwrap();
        let checks = std::cell::Cell::new(0);
        let sent = RefCell::new(Vec::new());
        let result = run(
            &plan,
            |f| {
                if f == Fence::Ready {
                    checks.set(checks.get() + 1);
                    if checks.get() == 5 {
                        return Err(Error::new("input_target_not_frontmost", "test"));
                    }
                }
                Ok(())
            },
            |e| {
                sent.borrow_mut().push(e);
                Ok(())
            },
            |_| {},
        );
        assert!(result.is_err());
        let sent = sent.into_inner();
        assert_eq!(sent.last(), Some(&Event::Button(Button::Left, false)));
        assert_eq!(
            sent.iter()
                .filter(|e| **e == Event::Button(Button::Left, false))
                .count(),
            1
        );
    }

    #[test]
    fn a_failed_down_is_still_released_and_a_failed_up_is_retried_once() {
        let k = key_code(vk::LETTERS[2]).unwrap();
        // The key down fails: it may have been delivered, so key up and then Shift up follow
        // (Shift after its lead), and the down is never retried.
        let (result, sent, waits) = record(&tap_plan(k, Some(SHIFT)), None, &|e, _| {
            e == Event::Key(k, true)
        });
        assert_eq!(result.unwrap_err().code, "input_unavailable");
        assert_eq!(
            sent,
            vec![
                Event::Key(SHIFT, true),
                Event::Key(k, true),
                Event::Key(k, false),
                Event::Key(SHIFT, false),
            ]
        );
        assert_eq!(waits, vec![150, 150]);
        // The first key up fails once: it is retried once and the plan carries on.
        let (result, sent, _) = record(&tap_plan(k, None), None, &|e, n| {
            e == Event::Key(k, false) && n == 0
        });
        assert!(result.is_ok());
        assert_eq!(
            sent,
            vec![
                Event::Key(k, true),
                Event::Key(k, false),
                Event::Key(k, false)
            ]
        );
    }
}
