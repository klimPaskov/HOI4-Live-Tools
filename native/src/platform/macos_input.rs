//! Ordinary macOS key and mouse input to one owned game window. An event is
//! posted only while the front-most on-screen window belongs to the owned
//! process; anything else refuses before sending. Needs the Accessibility
//! permission of the responsible app. Clicks target game-logical points inside
//! the owned window's content rectangle. No text, activation or engine call.
use crate::error::{Error, Result};
use std::ffi::c_void;

type CFTypeRef = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFArrayGetCount(array: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CFTypeRef, index: isize) -> CFTypeRef;
    fn CFDictionaryGetValue(dict: CFTypeRef, key: CFTypeRef) -> CFTypeRef;
    fn CFNumberGetValue(number: CFTypeRef, kind: isize, out: *mut c_void) -> bool;
    fn CFRelease(value: CFTypeRef);
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGWindowOwnerPID: CFTypeRef;
    static kCGWindowLayer: CFTypeRef;
    static kCGWindowBounds: CFTypeRef;
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
    fn CGRectMakeWithDictionaryRepresentation(dict: CFTypeRef, rect: *mut Rect) -> bool;
    fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
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

/// Content rectangle (global points) of the owned process's largest normal
/// on-screen window, assuming its client area is `game` logical pixels with a
/// title bar above it. Refuses unexpected scales or decorations.
pub fn owned_content(pid: i32, game: (u32, u32)) -> Result<Rect> {
    // SAFETY: the returned array is released below; entries are borrowed.
    let bounds = unsafe {
        let list = CGWindowListCopyWindowInfo(ON_SCREEN_ONLY | EXCLUDE_DESKTOP, 0);
        if list.is_null() {
            return Err(Error::new("input_unavailable", "Window list unavailable"));
        }
        let mut best: Option<Rect> = None;
        for index in 0..CFArrayGetCount(list) {
            let window = CFArrayGetValueAtIndex(list, index);
            if number(window, kCGWindowLayer) != Some(0)
                || number(window, kCGWindowOwnerPID) != Some(pid)
            {
                continue;
            }
            let mut rect = Rect::default();
            let dict = CFDictionaryGetValue(window, kCGWindowBounds);
            if !dict.is_null()
                && CGRectMakeWithDictionaryRepresentation(dict, &mut rect)
                && best.is_none_or(|b| rect.size.x * rect.size.y > b.size.x * b.size.y)
            {
                best = Some(rect);
            }
        }
        CFRelease(list);
        best
    };
    let bounds = bounds.ok_or_else(|| {
        Error::new(
            "input_target_unavailable",
            "Owned game window is not on screen",
        )
    })?;
    content_from_bounds(bounds, game)
}
fn content_from_bounds(bounds: Rect, game: (u32, u32)) -> Result<Rect> {
    let (w, h) = (f64::from(game.0), f64::from(game.1));
    let title = bounds.size.y - h;
    if bounds.size.x != w || !(0.0..=40.0).contains(&title) {
        return Err(Error::new(
            "input_geometry_unverified",
            "Owned window is not an unscaled windowed game of the expected size",
        ));
    }
    Ok(Rect {
        origin: Point {
            x: bounds.origin.x,
            y: bounds.origin.y + title,
        },
        size: Point { x: w, y: h },
    })
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

pub struct MacKeys {
    pid: i32,
}
impl MacKeys {
    pub fn new(pid: i32) -> Self {
        Self { pid }
    }
    /// Refuse unless input permission exists and the owned game is in front.
    pub fn preflight(&self) -> Result<()> {
        // SAFETY: no arguments; reads this process's accessibility trust.
        if !unsafe { AXIsProcessTrusted() } {
            return Err(Error::new(
                "input_permission_denied",
                "Accessibility permission is required for ordinary key input",
            ));
        }
        if frontmost_window_owner()? != Some(self.pid) {
            return Err(Error::new(
                "input_target_not_frontmost",
                "The owned game window is not the front-most window; no key was sent",
            ));
        }
        Ok(())
    }
    /// One left click at a game-logical point after a fresh front-most check:
    /// move, settle, press, release. The cursor stays where it clicked.
    pub fn click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.preflight()?;
        let point = client_point(owned_content(self.pid, game)?, x, y)?;
        for (kind, pause) in [
            (MOUSE_MOVED, 60),
            (LEFT_MOUSE_DOWN, 50),
            (LEFT_MOUSE_UP, 40),
        ] {
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
        self.preflight()?;
        let content = owned_content(self.pid, game)?;
        let a = client_point(content, from.0, from.1)?;
        let b = client_point(content, to.0, to.1)?;
        let post = |kind: u32, point: Point| -> Result<()> {
            // SAFETY: a null source is allowed; the event is released after posting.
            unsafe {
                let event = CGEventCreateMouseEvent(std::ptr::null(), kind, point, LEFT_BUTTON);
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
        post(LEFT_MOUSE_DOWN, a)?;
        std::thread::sleep(pause * 4);
        for step in 1..=12 {
            let t = f64::from(step) / 12.0;
            let p = Point {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            };
            post(LEFT_MOUSE_DRAGGED, p)?;
            std::thread::sleep(pause);
        }
        std::thread::sleep(pause * 4);
        post(LEFT_MOUSE_UP, b)
    }
    /// One vertical scroll-wheel step (pixel units, positive scrolls up) with the
    /// cursor at a game-logical point, after a fresh front-most check.
    pub fn scroll(&self, game: (u32, u32), at: (f64, f64), pixels: i32) -> Result<()> {
        self.preflight()?;
        let point = client_point(owned_content(self.pid, game)?, at.0, at.1)?;
        // SAFETY: a null source is allowed; events are released after posting.
        unsafe {
            let moved = CGEventCreateMouseEvent(std::ptr::null(), MOUSE_MOVED, point, LEFT_BUTTON);
            if moved.is_null() {
                return Err(Error::new("input_unavailable", "Mouse event unavailable"));
            }
            CGEventPost(HID_EVENT_TAP, moved);
            CFRelease(moved);
            std::thread::sleep(std::time::Duration::from_millis(60));
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
    /// One key press (down, then up) after a fresh front-most check.
    pub fn press(&self, key: Key) -> Result<()> {
        self.preflight()?;
        for down in [true, false] {
            // SAFETY: a null source is allowed; the event is released after posting.
            unsafe {
                let event = CGEventCreateKeyboardEvent(std::ptr::null(), key.code(), down);
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
    fn content_sits_below_the_title_bar_and_refuses_scaling() {
        let bounds = Rect {
            origin: Point { x: 0.0, y: 53.0 },
            size: Point {
                x: 1920.0,
                y: 1108.0,
            },
        };
        let content = content_from_bounds(bounds, (1920, 1080)).unwrap();
        assert_eq!(content.origin, Point { x: 0.0, y: 81.0 });
        assert_eq!(
            client_point(content, 10.0, 20.0).unwrap(),
            Point { x: 10.0, y: 101.0 }
        );
        assert!(client_point(content, 1920.0, 0.0).is_err());
        let scaled = Rect {
            size: Point { x: 960.0, y: 568.0 },
            ..bounds
        };
        assert!(content_from_bounds(scaled, (1920, 1080)).is_err());
    }
    #[test]
    fn window_list_query_is_safe_without_permissions() {
        // The owner may be any app (or none); the query itself must not fail.
        let _ = frontmost_window_owner().unwrap();
    }
}
