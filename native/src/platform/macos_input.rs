//! Ordinary macOS key and mouse input to one owned game window. An event is
//! posted only while the front-most on-screen window belongs to the owned
//! process; anything else refuses before sending. Needs the Accessibility
//! permission of the responsible app. Clicks target game-logical points inside
//! the owned window's content rectangle. No text, activation or engine call.
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
    fn CGEventSetFlags(event: CFTypeRef, flags: u64);
    fn CGEventSourceFlagsState(state: i32) -> u64;
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

pub struct MacKeys<'p> {
    pid: i32,
    process: &'p (dyn Process + Send + Sync),
    geometry: Geometry,
    window: Window,
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
            process,
            geometry,
            window,
        };
        bound.preflight()?;
        Ok(bound)
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
        if !unsafe { AXIsProcessTrusted() } {
            return Err(Error::new(
                "input_permission_denied",
                "Accessibility permission is required for ordinary input",
            ));
        }
        let current = macos::observe(self.process)?;
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
        self.process.check()?;
        Ok(())
    }
    /// One left click at a game-logical point after a fresh front-most check:
    /// move, settle, press, release. The cursor stays where it clicked.
    pub fn click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        let point = client_point(self.verified_content(game)?, x, y)?;
        for (kind, pause) in [
            (MOUSE_MOVED, 60),
            (LEFT_MOUSE_DOWN, 50),
            (LEFT_MOUSE_UP, 40),
        ] {
            if kind == LEFT_MOUSE_DOWN || kind == RIGHT_MOUSE_DOWN {
                self.preflight()?;
            }
            // SAFETY: a null source is allowed; the event is released after posting.
            unsafe {
                let event = CGEventCreateMouseEvent(std::ptr::null(), kind, point, LEFT_BUTTON);
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
    /// One left-button drag between game-logical points in small steps, after a
    /// fresh front-most check. Used to pan views; never to move game objects.
    pub fn drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.drag_with(game, from, to, false)
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
        self.drag_with(game, from, to, true)
    }
    fn drag_with(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        right: bool,
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
            Ok(())
        };
        let pause = std::time::Duration::from_millis(16);
        post(MOUSE_MOVED, a)?;
        std::thread::sleep(pause * 4);
        self.preflight()?;
        post(down, a)?;
        std::thread::sleep(pause * 4);
        for step in 1..=steps {
            let t = f64::from(step) / f64::from(steps);
            let p = Point {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            };
            post(dragged, p)?;
            std::thread::sleep(std::time::Duration::from_millis(step_ms));
        }
        std::thread::sleep(pause * 4);
        post(up, b)
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

#[cfg(test)]
mod tests {
    use super::*;
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
