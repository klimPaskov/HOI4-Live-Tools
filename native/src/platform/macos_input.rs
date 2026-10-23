//! Ordinary macOS key and mouse input to one owned game window. An event is
//! posted only while the front-most on-screen window belongs to the owned
//! process; anything else refuses before sending. Needs the Accessibility
//! permission of the responsible app. Clicks target game-logical points inside
//! the owned window's content rectangle. No text, activation or engine call.
use super::key_once;
use crate::{
    error::{Error, Result},
    gui::macos::{self, Geometry},
    platform::Process,
};
use std::ffi::c_void;

type CFTypeRef = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayGetCount(array: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
    fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFNumberGetValue(number: CFTypeRef, kind: isize, out: *mut c_void) -> bool;
    fn CFRelease(value: CFTypeRef);
    fn CFDataGetLength(data: CFTypeRef) -> isize;
    fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGWindowOwnerPID: CFTypeRef;
    static kCGWindowLayer: CFTypeRef;
    static kCGWindowBounds: CFTypeRef;
    static kCGWindowNumber: CFTypeRef;
    fn CGWindowListCreateImage(rect: Rect, option: u32, window: u32, image: u32) -> CFTypeRef;
    fn CGImageGetWidth(image: CFTypeRef) -> usize;
    fn CGImageGetHeight(image: CFTypeRef) -> usize;
    fn CGImageGetBytesPerRow(image: CFTypeRef) -> usize;
    fn CGImageGetBitsPerPixel(image: CFTypeRef) -> usize;
    fn CGImageGetDataProvider(image: CFTypeRef) -> CFTypeRef;
    fn CGDataProviderCopyData(provider: CFTypeRef) -> CFTypeRef;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
    fn CGRectMakeWithDictionaryRepresentation(dict: CFTypeRef, rect: *mut Rect) -> bool;
    fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
    fn CGEventCreate(source: CFTypeRef) -> CFTypeRef;
    fn CGEventGetLocation(event: CFTypeRef) -> Point;
    fn CGEventSetFlags(event: CFTypeRef, flags: u64);
    fn CGEventSourceFlagsState(state: i32) -> u64;
    fn CGEventSourceButtonState(state: i32, button: u32) -> bool;
    fn CGEventSourceKeyState(state: i32, key: u16) -> bool;
    fn CGEventSetType(event: CFTypeRef, kind: u32);
    fn CGEventCreateMouseEvent(
        source: CFTypeRef,
        kind: u32,
        point: Point,
        button: u32,
    ) -> CFTypeRef;
    fn CGEventPost(tap: u32, event: CFTypeRef);
    fn CGEventCreateScrollWheelEvent(
        source: CFTypeRef,
        units: u32,
        wheel_count: u32,
        wheel1: i32,
        ...
    ) -> CFTypeRef;
}
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}
const ON_SCREEN_ONLY: u32 = 1;
const EXCLUDE_DESKTOP: u32 = 1 << 4;
const NUMBER_SINT32: isize = 3;
const HID_EVENT_TAP: u32 = 0;
const LEFT_MOUSE_DOWN: u32 = 1;
const LEFT_MOUSE_UP: u32 = 2;
const MOUSE_MOVED: u32 = 5;
const LEFT_MOUSE_DRAGGED: u32 = 6;
const LEFT_BUTTON: u32 = 0;
const RIGHT_MOUSE_DOWN: u32 = 3;
const RIGHT_MOUSE_UP: u32 = 4;
const RIGHT_MOUSE_DRAGGED: u32 = 7;
const RIGHT_BUTTON: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub origin: Point,
    pub size: Point,
}

/// Stationary game keys used by the clock adapter (macOS virtual key codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Space toggles pause.
    Pause,
    /// Keypad plus raises game speed.
    Faster,
    /// Keypad minus lowers game speed.
    Slower,
    /// Escape closes the current panel.
    Escape,
    /// Return confirms (e.g. the Find view).
    Return,
    /// An ordinary letter hotkey, `a`..=`z`.
    Letter(char),
}
impl Key {
    fn code(self) -> u16 {
        // ANSI virtual key codes indexed a..z.
        const LETTERS: [u16; 26] = [
            0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7,
            16, 6,
        ];
        match self {
            Self::Pause => 49,
            Self::Faster => 69,
            Self::Slower => 78,
            Self::Escape => 53,
            Self::Return => 36,
            Self::Letter(c) => LETTERS[(c as u8 - b'a') as usize],
        }
    }
    /// Only lowercase ASCII letters are ordinary hotkeys here.
    pub fn letter(c: char) -> Result<Self> {
        if c.is_ascii_lowercase() {
            Ok(Self::Letter(c))
        } else {
            Err(Error::new("invalid_arguments", "Unsupported hotkey"))
        }
    }
}

fn find_digit_code(digit: char) -> Result<u16> {
    const DIGITS: [u16; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
    if !digit.is_ascii_digit() {
        return Err(Error::new(
            "invalid_arguments",
            "Only an ordinary ASCII decimal digit is accepted",
        ));
    }
    Ok(DIGITS[(digit as u8 - b'0') as usize])
}

/// One capture of the owned game's client area: BGRA pixels at `scale`
/// physical pixels per game pixel.
pub struct Capture {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub scale: f64,
    pub data: Vec<u8>,
}
impl Capture {
    /// (r, g, b) at a game-logical pixel, if inside the capture.
    pub fn rgb(&self, x: f64, y: f64) -> Option<(u8, u8, u8)> {
        let px = (x * self.scale) as usize;
        let py = (y * self.scale) as usize;
        if px >= self.width || py >= self.height {
            return None;
        }
        let o = py * self.stride + px * 4;
        Some((self.data[o + 2], self.data[o + 1], self.data[o]))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Window {
    id: u32,
    owner: i32,
    bounds: Rect,
}
/// Normal on-screen windows in current front-to-back order.
fn windows() -> Result<Vec<Window>> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
        if list.is_null() {
            return Err(Error::new("input_unavailable", "Window list unavailable"));
        }
        let mut found = Vec::new();
        for index in 0..CFArrayGetCount(list) {
            let window = CFArrayGetValueAtIndex(list, index);
            if number(window, kCGWindowLayer) != Some(0) {
                continue;
            }
            let mut rect = Rect::default();
            let dict = CFDictionaryGetValue(window, kCGWindowBounds);
            if let (Some(id), Some(owner)) = (
                number(window, kCGWindowNumber),
                number(window, kCGWindowOwnerPID),
            ) {
                if id > 0
                    && !dict.is_null()
                    && CGRectMakeWithDictionaryRepresentation(dict, &mut rect)
                {
                    found.push(Window {
                        id: id as u32,
                        owner,
                        bounds: rect,
                    });
                }
            }
        }
        CFRelease(list);
        Ok(found)
    }
}
fn content(geometry: &Geometry) -> Rect {
    let [x, y, w, h] = geometry.client.map(f64::from);
    Rect {
        origin: Point { x, y },
        size: Point { x: w, y: h },
    }
}
/// Corroborate a memory-derived content rectangle, without deriving it from a
/// title-bar constant or selecting the largest window. Refuse ambiguous matches.
fn select_window(pid: i32, client: Rect, windows: &[Window]) -> Result<Window> {
    let matches: Vec<_> = windows
        .iter()
        .filter(|w| {
            let frame = w.bounds;
            let decoration = client.origin.y - frame.origin.y;
            w.owner == pid
                && frame.origin.x == client.origin.x
                && frame.size.x == client.size.x
                && (0.0..=40.0).contains(&decoration)
                && frame.origin.y + frame.size.y == client.origin.y + client.size.y
        })
        .copied()
        .collect();
    if matches.len() != 1 {
        return Err(Error::new(
            "input_geometry_unverified",
            "One owned OS window must match the fresh native content rectangle",
        ));
    }
    let selected = matches[0];
    // ScreenCaptureKit can add a same-owner normal window entirely within the
    // game's title decoration (the sharing indicator). It cannot cover a game
    // control. Permit only that narrow region; a foreign window, overlapping
    // child, modal elsewhere or invalid rectangle still refuses. SDL input
    // focus is independently required by the witnessed native profile.
    if windows.iter().take_while(|w| **w != selected).any(|w| {
        let b = w.bounds;
        w.owner != pid
            || ![b.origin.x, b.origin.y, b.size.x, b.size.y]
                .iter()
                .all(|v| v.is_finite())
            || b.size.x <= 0.0
            || b.size.y <= 0.0
            || b.origin.x < selected.bounds.origin.x
            || b.origin.y < selected.bounds.origin.y
            || b.origin.x + b.size.x > selected.bounds.origin.x + selected.bounds.size.x
            || b.origin.y + b.size.y > client.origin.y
    }) {
        return Err(Error::new(
            "input_target_not_frontmost",
            "The exact owned game client is not frontmost; no input was sent",
        ));
    }
    Ok(selected)
}
/// Capture the owned window's client area (needs Screen Recording permission).
fn capture_client(id: u32, content: Rect, game: (u32, u32)) -> Result<Capture> {
    // SAFETY: CoreGraphics objects are released below; the copied bytes are owned.
    unsafe {
        // kCGWindowListOptionIncludingWindow, kCGWindowImageBoundsIgnoreFraming.
        let image = CGWindowListCreateImage(content, 1 << 3, id, 1);
        if image.is_null() {
            return Err(Error::new(
                "capture_unavailable",
                "Window capture unavailable (Screen Recording permission or locked screen)",
            ));
        }
        let width = CGImageGetWidth(image);
        let height = CGImageGetHeight(image);
        let stride = CGImageGetBytesPerRow(image);
        let bits = CGImageGetBitsPerPixel(image);
        let data = CGDataProviderCopyData(CGImageGetDataProvider(image));
        CFRelease(image);
        if data.is_null() || bits != 32 {
            if !data.is_null() {
                CFRelease(data);
            }
            return Err(Error::new(
                "capture_unavailable",
                "Unexpected capture format",
            ));
        }
        let len = CFDataGetLength(data) as usize;
        let bytes = std::slice::from_raw_parts(CFDataGetBytePtr(data), len).to_vec();
        CFRelease(data);
        if len < stride * height {
            return Err(Error::new("capture_unavailable", "Short capture"));
        }
        Ok(Capture {
            width,
            height,
            stride,
            scale: width as f64 / f64::from(game.0),
            data: bytes,
        })
    }
}

/// Global point for a game-logical pixel; refuses points outside the client.
pub fn client_point(content: Rect, x: f64, y: f64) -> Result<Point> {
    if !(0.0..content.size.x).contains(&x) || !(0.0..content.size.y).contains(&y) {
        return Err(Error::new(
            "invalid_arguments",
            "Point outside the owned game client area",
        ));
    }
    Ok(Point {
        x: content.origin.x + x,
        y: content.origin.y + y,
    })
}

fn number(dict: CFTypeRef, key: CFTypeRef) -> Option<i32> {
    let mut out = 0i32;
    // SAFETY: dict is a live CFDictionary; the value, if any, is a CFNumber.
    unsafe {
        let value = CFDictionaryGetValue(dict, key);
        (!value.is_null() && CFNumberGetValue(value, NUMBER_SINT32, (&mut out as *mut i32).cast()))
            .then_some(out)
    }
}

/// Owner pid of the front-most normal (layer 0) on-screen window.
pub fn frontmost_window_owner() -> Result<Option<i32>> {
    // SAFETY: the returned array is released below; entries are borrowed.
    unsafe {
        let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
        if list.is_null() {
            return Err(Error::new("input_unavailable", "Window list unavailable"));
        }
        let mut owner = None;
        for index in 0..CFArrayGetCount(list) {
            let window = CFArrayGetValueAtIndex(list, index);
            if number(window, kCGWindowLayer) == Some(0) {
                owner = number(window, kCGWindowOwnerPID);
                break;
            }
        }
        CFRelease(list);
        Ok(owner)
    }
}

/// The release is cleanup, not new authorization. Preserve the first failure
/// while still attempting mouse-up at the last successfully posted position.
#[allow(clippy::too_many_arguments)]
fn drag_sequence(
    a: Point,
    b: Point,
    kinds: (u32, u32, u32),
    steps: u32,
    step_ms: u64,
    mut check: impl FnMut() -> Result<()>,
    mut before_down: impl FnMut() -> Result<()>,
    mut post: impl FnMut(u32, Point) -> Result<()>,
    mut pause: impl FnMut(std::time::Duration),
) -> Result<()> {
    let (down, dragged, up) = kinds;
    let settle = std::time::Duration::from_millis(64);
    check()?;
    post(MOUSE_MOVED, a)?;
    pause(settle);
    check()?;
    before_down()?;
    check()?;
    if let Err(error) = post(down, a) {
        // An attempted down can have been delivered despite a transport error.
        if post(up, a).is_err() {
            let _ = post(up, a);
        }
        return Err(error);
    }
    let mut last = a;
    let result = (|| {
        pause(settle);
        for step in 1..=steps {
            check()?;
            let t = f64::from(step) / f64::from(steps);
            let point = Point {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            };
            post(dragged, point)?;
            last = point;
            pause(std::time::Duration::from_millis(step_ms));
        }
        pause(settle);
        Ok(())
    })();
    let release = post(up, last);
    result.and(release)
}

/// A click can be cancelled after its pointer settle. Once down was attempted,
/// release is unconditional and a failed release gets one cleanup retry.
#[cfg(test)]
fn click_sequence(
    point: Point,
    down: u32,
    up: u32,
    check: impl FnMut() -> Result<()>,
    post: impl FnMut(u32, Point) -> Result<()>,
    pause: impl FnMut(std::time::Duration),
) -> Result<()> {
    click_sequence_checked(point, (down, up), check, || Ok(()), post, pause)
}
fn click_sequence_checked(
    point: Point,
    (down, up): (u32, u32),
    mut check: impl FnMut() -> Result<()>,
    mut before_down: impl FnMut() -> Result<()>,
    mut post: impl FnMut(u32, Point) -> Result<()>,
    mut pause: impl FnMut(std::time::Duration),
) -> Result<()> {
    check()?;
    post(MOUSE_MOVED, point)?;
    pause(std::time::Duration::from_millis(60));
    check()?;
    before_down()?;
    let dispatched = post(down, point);
    if dispatched.is_ok() {
        pause(std::time::Duration::from_millis(50));
    }
    let released = post(up, point);
    if released.is_err() {
        let _ = post(up, point);
    }
    pause(std::time::Duration::from_millis(40));
    dispatched.and(released)
}

/// Allocate the complete ordinary mouse sequence before any event is posted.
/// The final target/deadline check therefore follows down-event allocation,
/// and cleanup never needs a new allocation after possible delivery.
struct MouseEvents([CFTypeRef; 3]);
impl MouseEvents {
    fn new(point: Point) -> Result<Self> {
        let events = Self(
            [MOUSE_MOVED, LEFT_MOUSE_DOWN, LEFT_MOUSE_UP].map(|kind| unsafe {
                CGEventCreateMouseEvent(std::ptr::null(), kind, point, LEFT_BUTTON)
            }),
        );
        if events.0.iter().any(|event| event.is_null()) {
            return Err(Error::new(
                "input_unavailable",
                "Complete ordinary mouse event sequence unavailable",
            ));
        }
        for &event in &events.0 {
            unsafe {
                CGEventSetFlags(event, 0);
            }
        }
        Ok(events)
    }
    fn post(&self, kind: u32) -> Result<()> {
        let index = match kind {
            MOUSE_MOVED => 0,
            LEFT_MOUSE_DOWN => 1,
            LEFT_MOUSE_UP => 2,
            _ => {
                return Err(Error::new(
                    "input_unavailable",
                    "Unknown ordinary mouse event",
                ))
            }
        };
        unsafe {
            CGEventPost(HID_EVENT_TAP, self.0[index]);
        }
        Ok(())
    }
}
impl Drop for MouseEvents {
    fn drop(&mut self) {
        for &event in &self.0 {
            if !event.is_null() {
                unsafe {
                    CFRelease(event);
                }
            }
        }
    }
}

enum ProcessBinding<'p> {
    Borrowed(&'p (dyn Process + Send + Sync)),
    Owned(std::sync::Arc<dyn Process + Send + Sync>),
}
impl ProcessBinding<'_> {
    fn get(&self) -> &(dyn Process + Send + Sync) {
        match self {
            Self::Borrowed(process) => *process,
            Self::Owned(process) => process.as_ref(),
        }
    }
}
pub struct MacKeys<'p> {
    pid: i32,
    process: ProcessBinding<'p>,
    geometry: Geometry,
    window: Window,
    input_guards: Vec<&'p (dyn Fn() -> Result<()> + Send + Sync)>,
}
impl<'p> MacKeys<'p> {
    pub fn bind(process: &'p (dyn Process + Send + Sync)) -> Result<Self> {
        let pid = process
            .identity()
            .ok_or_else(|| Error::new("input_unavailable", "Owned process identity required"))?
            .pid;
        let geometry = macos::observe(process)?;
        geometry.require_profile()?;
        let window = select_window(pid, content(&geometry), &windows()?)?;
        let bound = Self {
            pid,
            process: ProcessBinding::Borrowed(process),
            geometry,
            window,
            input_guards: Vec::new(),
        };
        bound.preflight()?;
        Ok(bound)
    }
    /// Borrow the original binding with an additional per-action deadline/lease
    /// fence. A guarded view cannot outlive either its owner or the fence.
    pub(crate) fn guarded<'a>(
        &'a self,
        process: &'a (dyn Process + Send + Sync),
        check: &'a (dyn Fn() -> Result<()> + Send + Sync),
    ) -> Result<MacKeys<'a>> {
        if process.identity() != self.process.get().identity()
            || process.image_base() != self.process.get().image_base()
            || process.string_abi() != self.process.get().string_abi()
        {
            return Err(Error::new(
                "ownership_changed",
                "Guarded input reader differs from its binding",
            ));
        }
        let mut input_guards = self.input_guards.clone();
        input_guards.push(check);
        Ok(MacKeys {
            pid: self.pid,
            process: ProcessBinding::Borrowed(process),
            geometry: self.geometry.clone(),
            window: self.window,
            input_guards,
        })
    }
    pub fn capture(&self, game: (u32, u32)) -> Result<Capture> {
        let content = self.verified_content(game)?;
        let result = capture_client(self.window.id, content, game)?;
        self.preflight()?;
        Ok(result)
    }
    fn verified_content(&self, game: (u32, u32)) -> Result<Rect> {
        if game != (1920, 1080) {
            return Err(Error::new(
                "input_geometry_unverified",
                "Mac input profile differs from requested game dimensions",
            ));
        }
        self.preflight()?;
        Ok(content(&self.geometry))
    }
    /// Fresh process, SDL/graphics witnesses and exact OS window before input.
    pub fn preflight(&self) -> Result<()> {
        for check in &self.input_guards {
            check()?;
        }
        if !unsafe { AXIsProcessTrusted() } {
            return Err(Error::new(
                "input_permission_denied",
                "Accessibility permission is required for ordinary input",
            ));
        }
        let current = macos::observe(self.process.get())?;
        current.require_profile()?;
        if !current.same_binding(&self.geometry) {
            return Err(Error::new(
                "input_geometry_changed",
                "Native game geometry or window binding changed; rebind required",
            ));
        }
        if select_window(self.pid, content(&current), &windows()?)? != self.window {
            return Err(Error::new(
                "input_geometry_changed",
                "Owned OS window changed; rebind required",
            ));
        }
        self.process.get().check()?;
        for check in &self.input_guards {
            check()?;
        }
        Ok(())
    }
    /// Read-only combined-session state. Never release user-held input to make
    /// a production receipt pass. Recheck the owned geometry on both sides.
    pub(crate) fn confirm_input_idle(&self) -> Result<()> {
        self.preflight()?;
        // CoreGraphics combined session state (1), not just this source's events.
        let busy = unsafe {
            CGEventSourceFlagsState(1) & 0x00df_0000 != 0
                || (0..32).any(|button| CGEventSourceButtonState(1, button))
                || (0..128).any(|key| CGEventSourceKeyState(1, key))
        };
        if busy {
            return Err(Error::new(
                "input_busy",
                "A session key, button or modifier is held",
            ));
        }
        self.preflight()
    }
    /// One left click at a game-logical point after a fresh front-most check:
    /// move, settle, press, release. The cursor stays where it clicked.
    pub fn click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.click_checked(game, x, y, &|| Ok(()))
    }
    /// One ordinary unmodified click with an additional post-move check before
    /// down. Any failure may follow cursor motion; it is not proof of no input.
    pub(crate) fn click_checked(
        &self,
        game: (u32, u32),
        x: f64,
        y: f64,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<Point> {
        let point = client_point(self.verified_content(game)?, x, y)?;
        let events = MouseEvents::new(point)?;
        click_sequence_checked(
            point,
            (LEFT_MOUSE_DOWN, LEFT_MOUSE_UP),
            || self.preflight(),
            before_down,
            |kind, _| events.post(kind),
            std::thread::sleep,
        )?;
        Ok(point)
    }
    /// Read the current OS cursor in the same screen coordinate space used by
    /// CGEvent mouse posts. Creating this snapshot posts no event.
    pub(crate) fn confirm_cursor_at(&self, game: (u32, u32), x: f64, y: f64) -> Result<()> {
        let expected = client_point(self.verified_content(game)?, x, y)?;
        let actual = unsafe {
            let event = CGEventCreate(std::ptr::null());
            if event.is_null() {
                return Err(Error::new(
                    "input_unavailable",
                    "Current cursor snapshot unavailable",
                ));
            }
            let point = CGEventGetLocation(event);
            CFRelease(event);
            point
        };
        if actual != expected {
            return Err(Error::new(
                "input_cursor_changed",
                "OS cursor differs from the original construction target",
            ));
        }
        self.preflight()
    }
    /// One left-button drag between game-logical points in small steps, after a
    /// fresh front-most check. Used to pan views; never to move game objects.
    pub fn drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.drag_with(game, from, to, false, &|| Ok(()))
    }
    /// Revalidate the original drag origin after cursor settling and before
    /// mouse-down. Later motion retains the ordinary guards, while allowing the
    /// intended geometry change. No callback can suppress release cleanup.
    pub(crate) fn drag_checked(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.drag_with(game, from, to, false, before_down)
    }
    /// One left click with Shift held, after a fresh front-most check. The
    /// game reads Shift through a modifier (flags-changed) event carrying the
    /// device-dependent left-shift bit; a plain key-down or flags on the mouse
    /// events alone reach army cards but not the division list.
    pub fn shift_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        const SHIFT_KEY: u16 = 56;
        const SHIFT_FLAGS: u64 = 0x2_0002;
        const FLAGS_CHANGED: u32 = 12;
        let point = client_point(self.verified_content(game)?, x, y)?;
        let pause = |ms: u64| std::thread::sleep(std::time::Duration::from_millis(ms));
        // SAFETY: a null source is allowed; every event is released after posting.
        unsafe {
            let post = |event: CFTypeRef, flags: u64| -> Result<()> {
                if event.is_null() {
                    return Err(Error::new("input_unavailable", "Input event unavailable"));
                }
                CGEventSetFlags(event, flags);
                CGEventPost(HID_EVENT_TAP, event);
                CFRelease(event);
                Ok(())
            };
            let modifier = |down: bool| -> Result<()> {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), SHIFT_KEY, down);
                if !event.is_null() {
                    CGEventSetType(event, FLAGS_CHANGED);
                }
                post(event, if down { SHIFT_FLAGS } else { 0 })
            };
            post(
                CGEventCreateMouseEvent(std::ptr::null(), MOUSE_MOVED, point, LEFT_BUTTON),
                0,
            )?;
            pause(100);
            self.preflight()?;
            modifier(true)?;
            pause(250);
            let result = (|| {
                self.preflight()?;
                post(
                    CGEventCreateMouseEvent(std::ptr::null(), LEFT_MOUSE_DOWN, point, LEFT_BUTTON),
                    SHIFT_FLAGS,
                )?;
                pause(60);
                post(
                    CGEventCreateMouseEvent(std::ptr::null(), LEFT_MOUSE_UP, point, LEFT_BUTTON),
                    SHIFT_FLAGS,
                )
            })();
            pause(150);
            // Always release Shift, even when the click failed.
            modifier(false)?;
            result?;
        }
        Ok(point)
    }
    /// One right click at a game-logical point after a fresh front-most check.
    pub fn right_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        let point = client_point(self.verified_content(game)?, x, y)?;
        for (kind, button, pause) in [
            (MOUSE_MOVED, LEFT_BUTTON, 60),
            (RIGHT_MOUSE_DOWN, RIGHT_BUTTON, 50),
            (RIGHT_MOUSE_UP, RIGHT_BUTTON, 40),
        ] {
            if kind == LEFT_MOUSE_DOWN || kind == RIGHT_MOUSE_DOWN {
                self.preflight()?;
            }
            // SAFETY: a null source is allowed; the event is released after posting.
            unsafe {
                let event = CGEventCreateMouseEvent(std::ptr::null(), kind, point, button);
                if event.is_null() {
                    return Err(Error::new("input_unavailable", "Mouse event unavailable"));
                }
                CGEventPost(HID_EVENT_TAP, event);
                CFRelease(event);
            }
            std::thread::sleep(std::time::Duration::from_millis(pause));
        }
        Ok(point)
    }
    /// One right-button drag between game-logical points (battle-plan lines),
    /// after a fresh front-most check.
    pub fn right_drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.drag_with(game, from, to, true, &|| Ok(()))
    }
    fn drag_with(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        right: bool,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.preflight()?;
        let (down, dragged, up, button, steps, step_ms) = if right {
            (
                RIGHT_MOUSE_DOWN,
                RIGHT_MOUSE_DRAGGED,
                RIGHT_MOUSE_UP,
                RIGHT_BUTTON,
                24,
                30,
            )
        } else {
            (
                LEFT_MOUSE_DOWN,
                LEFT_MOUSE_DRAGGED,
                LEFT_MOUSE_UP,
                LEFT_BUTTON,
                12,
                16,
            )
        };
        let content = self.verified_content(game)?;
        let a = client_point(content, from.0, from.1)?;
        let b = client_point(content, to.0, to.1)?;
        let post = |kind: u32, point: Point| -> Result<()> {
            // SAFETY: a null source is allowed; the event is released after posting.
            unsafe {
                let event = CGEventCreateMouseEvent(std::ptr::null(), kind, point, button);
                if event.is_null() {
                    return Err(Error::new("input_unavailable", "Mouse event unavailable"));
                }
                CGEventPost(HID_EVENT_TAP, event);
                CFRelease(event);
            }
            Ok(())
        };
        drag_sequence(
            a,
            b,
            (down, dragged, up),
            steps,
            step_ms,
            || self.preflight(),
            before_down,
            post,
            std::thread::sleep,
        )
    }
    /// One vertical scroll-wheel step (pixel units, positive scrolls up) with the
    /// cursor at a game-logical point, after a fresh front-most check.
    pub fn scroll(&self, game: (u32, u32), at: (f64, f64), pixels: i32) -> Result<()> {
        let point = client_point(self.verified_content(game)?, at.0, at.1)?;
        // SAFETY: a null source is allowed; events are released after posting.
        unsafe {
            let moved = CGEventCreateMouseEvent(std::ptr::null(), MOUSE_MOVED, point, LEFT_BUTTON);
            if moved.is_null() {
                return Err(Error::new("input_unavailable", "Mouse event unavailable"));
            }
            CGEventPost(HID_EVENT_TAP, moved);
            CFRelease(moved);
            std::thread::sleep(std::time::Duration::from_millis(60));
            self.preflight()?;
            let wheel = CGEventCreateScrollWheelEvent(std::ptr::null(), 0, 1, pixels);
            if wheel.is_null() {
                return Err(Error::new("input_unavailable", "Scroll event unavailable"));
            }
            CGEventPost(HID_EVENT_TAP, wheel);
            CFRelease(wheel);
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        Ok(())
    }
    /// One key press by ANSI virtual code (digits for the Find view).
    pub fn press_code(&self, code: u16) -> Result<()> {
        self.press_raw(code)
    }
    /// One key press (down, then up) after a fresh front-most check.
    pub fn press(&self, key: Key) -> Result<()> {
        self.press_raw(key.code())
    }
    /// One short shortcut with a producer-owned delivery classification. Only
    /// checks/allocation before entering down can positively report NotQueued.
    pub(crate) fn press_once(&self, key: Key) -> key_once::Dispatch {
        self.press_code_once(key.code())
    }
    /// Decimal Find input uses the same once-only producer and unconditional
    /// release. Reject every other character before any native check or event.
    pub(crate) fn press_digit_once(&self, digit: char) -> key_once::Dispatch {
        match find_digit_code(digit) {
            Ok(code) => self.press_code_once(code),
            Err(error) => key_once::Dispatch::NotQueued(error),
        }
    }
    fn press_code_once(&self, code: u16) -> key_once::Dispatch {
        struct Events {
            down: CFTypeRef,
            up: CFTypeRef,
        }
        impl Drop for Events {
            fn drop(&mut self) {
                // SAFETY: each nonnull pointer is independently owned. Freeing
                // an unused pair never posts a release or any other input.
                unsafe {
                    for event in [self.down, self.up] {
                        if !event.is_null() {
                            CFRelease(event);
                        }
                    }
                }
            }
        }
        impl key_once::Pair for Events {
            fn down(&mut self) -> Result<()> {
                // SAFETY: allocated/configured pair; the sequence has just
                // rechecked ownership, the original binding and idle input.
                unsafe {
                    CGEventPost(HID_EVENT_TAP, self.down);
                }
                Ok(()) // CGEventPost returns no delivery acknowledgement.
            }
            fn up(&mut self) -> Result<()> {
                // SAFETY: release was allocated before down. Cleanup deliberately
                // does not recheck a now-expired lease or foreground window.
                unsafe {
                    CGEventPost(HID_EVENT_TAP, self.up);
                }
                Ok(())
            }
        }
        key_once::dispatch(
            || self.confirm_input_idle(),
            || {
                // SAFETY: null event sources are permitted; Events owns both
                // results, including any partial allocation on failure.
                let events = unsafe {
                    Events {
                        down: CGEventCreateKeyboardEvent(std::ptr::null(), code, true),
                        up: CGEventCreateKeyboardEvent(std::ptr::null(), code, false),
                    }
                };
                if events.down.is_null() || events.up.is_null() {
                    return Err(Error::new(
                        "input_unavailable",
                        "Complete key event pair unavailable",
                    ));
                }
                // A null source can inherit allocation-time modifiers. Zero
                // them before the sequence's final idle/original-binding check.
                unsafe {
                    CGEventSetFlags(events.down, 0);
                    CGEventSetFlags(events.up, 0);
                }
                Ok(events)
            },
            std::thread::sleep,
        )
    }
    fn press_raw(&self, code: u16) -> Result<()> {
        self.preflight()?;
        // A stuck Fn flag turns letter keys into system shortcuts (Fn+Q opens
        // Quick Note and steals focus), so refuse instead of typing into them.
        // SAFETY: no pointers; reads the HID system modifier state.
        if unsafe { CGEventSourceFlagsState(1) } & 0x80_0000 != 0 {
            // Release it with a flags-changed event for the Fn key; if it is
            // really held by a finger the flag returns and nothing is sent.
            // SAFETY: a null source is allowed; the event is released after posting.
            unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), 63, false);
                if !event.is_null() {
                    CGEventSetType(event, 12);
                    CGEventSetFlags(event, 0);
                    CGEventPost(HID_EVENT_TAP, event);
                    CFRelease(event);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            // SAFETY: no pointers; reads the HID system modifier state.
            if unsafe { CGEventSourceFlagsState(1) } & 0x80_0000 != 0 {
                return Err(Error::new(
                    "input_modifier_stuck",
                    "The Fn modifier is held or stuck; no key was sent",
                ));
            }
        }
        for down in [true, false] {
            if down {
                self.preflight()?;
            }
            // SAFETY: a null source is allowed; the event is released after posting.
            unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), code, down);
                if event.is_null() {
                    return Err(Error::new("input_unavailable", "Key event unavailable"));
                }
                CGEventPost(HID_EVENT_TAP, event);
                CFRelease(event);
            }
            std::thread::sleep(std::time::Duration::from_millis(40));
        }
        Ok(())
    }
}

impl MacKeys<'static> {
    /// Retain the exact opened process for the controller/worker lifetime.
    pub fn bind_owned(process: std::sync::Arc<dyn Process + Send + Sync>) -> Result<Self> {
        let (pid, geometry, window) = {
            let bound = MacKeys::bind(process.as_ref())?;
            (bound.pid, bound.geometry, bound.window)
        };
        Ok(Self {
            pid,
            process: ProcessBinding::Owned(process),
            geometry,
            window,
            input_guards: Vec::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drag_cancellation_releases_at_last_point_and_sends_no_further_motion() {
        use std::cell::RefCell;
        let a = Point { x: 10.0, y: 20.0 };
        let b = Point { x: 50.0, y: 60.0 };
        for failing_check in 1..=7 {
            let sent = RefCell::new(Vec::new());
            let mut checks = 0;
            let result = drag_sequence(
                a,
                b,
                (LEFT_MOUSE_DOWN, LEFT_MOUSE_DRAGGED, LEFT_MOUSE_UP),
                4,
                0,
                || {
                    checks += 1;
                    if checks == failing_check {
                        Err(Error::new("action_deadline", "fixture"))
                    } else {
                        Ok(())
                    }
                },
                || Ok(()),
                |kind, p| {
                    sent.borrow_mut().push((kind, p));
                    Ok(())
                },
                |_| {},
            );
            assert_eq!(result.unwrap_err().code, "action_deadline");
            let sent = sent.into_inner();
            if failing_check <= 3 {
                assert!(!sent.iter().any(|(k, _)| *k == LEFT_MOUSE_DOWN));
            } else {
                let release = sent.last().unwrap();
                assert_eq!(release.0, LEFT_MOUSE_UP);
                assert_eq!(release.1, sent[sent.len() - 2].1);
            }
            assert_eq!(checks, failing_check);
        }
    }
    #[test]
    fn drag_transport_failure_attempts_release_and_preserves_original_cause() {
        let a = Point { x: 1.0, y: 2.0 };
        let b = Point { x: 5.0, y: 6.0 };
        let mut sent = Vec::new();
        let result = drag_sequence(
            a,
            b,
            (RIGHT_MOUSE_DOWN, RIGHT_MOUSE_DRAGGED, RIGHT_MOUSE_UP),
            4,
            0,
            || Ok(()),
            || Ok(()),
            |kind, p| {
                sent.push((kind, p));
                if kind == RIGHT_MOUSE_DRAGGED {
                    Err(Error::new("fixture_motion", "failed"))
                } else if kind == RIGHT_MOUSE_UP {
                    Err(Error::new("fixture_release", "failed"))
                } else {
                    Ok(())
                }
            },
            |_| {},
        );
        assert_eq!(result.unwrap_err().code, "fixture_motion");
        assert_eq!(sent.last(), Some(&(RIGHT_MOUSE_UP, a)));
    }
    #[test]
    fn drag_origin_check_runs_after_settle_and_refuses_down_on_change() {
        use std::cell::RefCell;
        let trace = RefCell::new(Vec::new());
        let p = Point { x: 1.0, y: 2.0 };
        let result = drag_sequence(
            p,
            p,
            (LEFT_MOUSE_DOWN, LEFT_MOUSE_DRAGGED, LEFT_MOUSE_UP),
            2,
            0,
            || {
                trace.borrow_mut().push("scope");
                Ok(())
            },
            || {
                trace.borrow_mut().push("origin");
                Err(Error::new("focus_state_changed", "fixture"))
            },
            |kind, _| {
                assert_eq!(kind, MOUSE_MOVED);
                trace.borrow_mut().push("move");
                Ok(())
            },
            |_| trace.borrow_mut().push("settle"),
        );
        assert_eq!(result.unwrap_err().code, "focus_state_changed");
        assert_eq!(
            trace.into_inner(),
            ["scope", "move", "settle", "scope", "origin"]
        );
    }
    #[test]
    fn drag_origin_is_not_rechecked_during_intended_motion_and_failed_down_releases() {
        let p = Point { x: 1.0, y: 2.0 };
        for fail in [false, true] {
            let mut origins = 0;
            let mut sent = vec![];
            let result = drag_sequence(
                p,
                p,
                (LEFT_MOUSE_DOWN, LEFT_MOUSE_DRAGGED, LEFT_MOUSE_UP),
                2,
                0,
                || Ok(()),
                || {
                    origins += 1;
                    Ok(())
                },
                |kind, _| {
                    sent.push(kind);
                    if fail && kind == LEFT_MOUSE_DOWN {
                        Err(Error::new("fixture_down", "unknown delivery"))
                    } else {
                        Ok(())
                    }
                },
                |_| {},
            );
            assert_eq!(origins, 1);
            assert_eq!(sent.last(), Some(&LEFT_MOUSE_UP));
            if fail {
                assert_eq!(result.unwrap_err().code, "fixture_down");
                assert!(!sent.contains(&LEFT_MOUSE_DRAGGED));
            } else {
                result.unwrap();
                assert_eq!(sent.iter().filter(|k| **k == LEFT_MOUSE_DRAGGED).count(), 2);
            }
        }
    }
    struct NoReads {
        id: crate::platform::Identity,
    }
    impl crate::memory::ReadMemory for NoReads {
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            panic!("Guard must run before native geometry read")
        }
    }
    impl Process for NoReads {
        fn check(&self) -> Result<()> {
            panic!("Guard must run before native process check")
        }
        fn image_base(&self) -> u64 {
            0x100000000
        }
        fn identity(&self) -> Option<&crate::platform::Identity> {
            Some(&self.id)
        }
    }
    #[test]
    fn nested_input_guards_preserve_original_fence_before_any_platform_work() {
        let process = NoReads {
            id: crate::platform::Identity {
                pid: 123,
                start_ticks: "1.000001".into(),
                exe: "/fixture".into(),
                uid: 501,
            },
        };
        let keys = MacKeys {
            pid: 123,
            process: ProcessBinding::Borrowed(&process),
            geometry: Geometry {
                objects: [0; 10],
                physical: [1920, 1080],
                logical: [1920, 1080],
                scale: 1.0,
                client: [0, 81, 1920, 1080],
                flags: 0x606,
            },
            window: Window {
                id: 7,
                owner: 123,
                bounds: Rect::default(),
            },
            input_guards: Vec::new(),
        };
        let expired = || Err(Error::new("action_deadline", "Original deadline"));
        let later = || panic!("Nested guard must not replace the original deadline");
        let first = keys.guarded(&process, &expired).unwrap();
        let nested = first.guarded(&process, &later).unwrap();
        assert_eq!(nested.preflight().unwrap_err().code, "action_deadline");
        let mut different = NoReads {
            id: process.id.clone(),
        };
        different.id.start_ticks = "2.000001".into();
        assert_eq!(
            keys.guarded(&different, &expired).err().unwrap().code,
            "ownership_changed"
        );
    }
    #[test]
    fn keys_map_to_stationary_virtual_codes() {
        assert_eq!(
            [Key::Pause, Key::Faster, Key::Slower].map(Key::code),
            [49, 69, 78]
        );
    }
    #[test]
    fn letters_map_to_ansi_codes() {
        assert_eq!(Key::letter('a').unwrap().code(), 0);
        assert_eq!(Key::letter('t').unwrap().code(), 17);
        assert_eq!(Key::letter('z').unwrap().code(), 6);
        assert!(Key::letter('A').is_err());
    }
    #[test]
    fn exact_native_content_requires_one_matching_frontmost_os_window() {
        let client = Rect {
            origin: Point { x: 0.0, y: 81.0 },
            size: Point {
                x: 1920.0,
                y: 1080.0,
            },
        };
        let w = Window {
            id: 7,
            owner: 123,
            bounds: Rect {
                origin: Point { x: 0.0, y: 53.0 },
                size: Point {
                    x: 1920.0,
                    y: 1108.0,
                },
            },
        };
        assert_eq!(select_window(123, client, &[w]).unwrap(), w);
        assert_eq!(
            client_point(client, 10.0, 20.0).unwrap(),
            Point { x: 10.0, y: 101.0 }
        );
        assert!(client_point(client, 1920.0, 0.0).is_err());
        assert!(select_window(124, client, &[w]).is_err());
        assert!(select_window(123, client, &[w, Window { id: 8, ..w }]).is_err());
        assert!(select_window(123, client, &[Window { owner: 9, ..w }, w]).is_err());
        assert!(select_window(
            123,
            client,
            &[Window {
                bounds: Rect {
                    origin: Point { x: 1.0, y: 53.0 },
                    ..w.bounds
                },
                ..w
            }]
        )
        .is_err());
        assert!(select_window(
            123,
            client,
            &[Window {
                bounds: Rect {
                    size: Point { x: 960.0, y: 568.0 },
                    ..w.bounds
                },
                ..w
            }]
        )
        .is_err());
        let indicator = Window {
            id: 8,
            owner: 123,
            bounds: Rect {
                origin: Point { x: 10.0, y: 57.0 },
                size: Point { x: 66.0, y: 20.0 },
            },
        };
        assert_eq!(select_window(123, client, &[indicator, w]).unwrap(), w);
        assert!(select_window(
            123,
            client,
            &[
                Window {
                    owner: 9,
                    ..indicator
                },
                w
            ]
        )
        .is_err());
        for (x, y, width, height) in [
            (10.0, 61.1, 66.0, 20.0),
            (10.0, 500.0, 66.0, 20.0),
            (-1.0, 57.0, 66.0, 20.0),
            (10.0, 57.0, 0.0, 20.0),
            (f64::NAN, 57.0, 66.0, 20.0),
        ] {
            let cover = Window {
                bounds: Rect {
                    origin: Point { x, y },
                    size: Point {
                        x: width,
                        y: height,
                    },
                },
                ..indicator
            };
            assert!(select_window(123, client, &[cover, w]).is_err());
        }
        let stale_client = Rect {
            origin: Point { x: 0.0, y: 82.0 },
            ..client
        };
        assert!(select_window(123, stale_client, &[w]).is_err());
    }
    #[test]
    fn window_list_query_is_safe_without_permissions() {
        // The owner may be any app (or none); the query itself must not fail.
        let _ = frontmost_window_owner().unwrap();
    }
}

#[cfg(test)]
mod click_cleanup_tests {
    use super::*;
    #[test]
    fn checked_click_validates_after_move_and_never_posts_down_after_refusal() {
        use std::cell::RefCell;
        for refuse in [false, true] {
            let trace = RefCell::new(Vec::new());
            let result = click_sequence_checked(
                Point { x: 10.0, y: 20.0 },
                (LEFT_MOUSE_DOWN, LEFT_MOUSE_UP),
                || {
                    trace.borrow_mut().push("check");
                    Ok(())
                },
                || {
                    trace.borrow_mut().push("target");
                    if refuse {
                        Err(Error::new("target_changed", "fixture"))
                    } else {
                        Ok(())
                    }
                },
                |kind, _| {
                    trace.borrow_mut().push(match kind {
                        MOUSE_MOVED => "move",
                        LEFT_MOUSE_DOWN => "down",
                        LEFT_MOUSE_UP => "up",
                        _ => panic!(),
                    });
                    Ok(())
                },
                |_| trace.borrow_mut().push("wait"),
            );
            let trace = trace.into_inner();
            assert_eq!(&trace[..5], &["check", "move", "wait", "check", "target"]);
            if refuse {
                assert_eq!(trace.len(), 5);
                assert_eq!(result.unwrap_err().code, "target_changed");
            } else {
                result.unwrap();
                assert_eq!(&trace[5..], &["down", "wait", "up", "wait"]);
            }
        }
    }
    #[test]
    fn stop_after_settle_prevents_down_and_all_later_motion() {
        let mut checks = 0;
        let mut sent = vec![];
        let result = click_sequence(
            Point { x: 10., y: 20. },
            LEFT_MOUSE_DOWN,
            LEFT_MOUSE_UP,
            || {
                checks += 1;
                if checks == 2 {
                    Err(Error::new("session_closed", "Generated stop"))
                } else {
                    Ok(())
                }
            },
            |kind, _| {
                sent.push(kind);
                Ok(())
            },
            |_| {},
        );
        assert_eq!(result.unwrap_err().code, "session_closed");
        assert_eq!(sent, vec![MOUSE_MOVED]);
    }
    #[test]
    fn possible_down_failure_and_release_failure_still_attempt_cleanup() {
        for fail_down in [true, false] {
            let mut sent = vec![];
            let mut first_release = true;
            let result = click_sequence(
                Point { x: 10., y: 20. },
                LEFT_MOUSE_DOWN,
                LEFT_MOUSE_UP,
                || Ok(()),
                |kind, _| {
                    sent.push(kind);
                    if fail_down && kind == LEFT_MOUSE_DOWN {
                        return Err(Error::new("down_failed", "Generated possible input"));
                    }
                    if kind == LEFT_MOUSE_UP && first_release {
                        first_release = false;
                        return Err(Error::new("release_failed", "Generated allocation failure"));
                    }
                    Ok(())
                },
                |_| {},
            );
            assert_eq!(
                result.unwrap_err().code,
                if fail_down {
                    "down_failed"
                } else {
                    "release_failed"
                }
            );
            assert_eq!(
                sent,
                vec![MOUSE_MOVED, LEFT_MOUSE_DOWN, LEFT_MOUSE_UP, LEFT_MOUSE_UP]
            );
        }
    }
}

#[cfg(test)]
mod find_digit_tests {
    use super::*;
    #[test]
    fn ansi_find_digits_are_exact_and_other_characters_refuse_before_input() {
        let codes: Vec<_> = "0123456789"
            .chars()
            .map(|v| find_digit_code(v).unwrap())
            .collect();
        assert_eq!(codes, vec![29, 18, 19, 20, 21, 23, 22, 26, 28, 25]);
        for c in ['f', '\n', ' ', '４', 'é'] {
            assert_eq!(find_digit_code(c).unwrap_err().code, "invalid_arguments");
        }
    }
}
