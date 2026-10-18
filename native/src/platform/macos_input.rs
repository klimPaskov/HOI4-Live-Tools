//! Ordinary macOS key input to one owned game window. A key is posted only while
//! the front-most on-screen window belongs to the owned process; anything else
//! refuses before sending. Needs the Accessibility permission of the
//! responsible app. No cursor movement, text, activation or engine call here.
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
    fn CGWindowListCopyWindowInfo(option: u32, relative_to: u32) -> CFTypeRef;
    fn CGEventCreateKeyboardEvent(source: CFTypeRef, key: u16, down: bool) -> CFTypeRef;
    fn CGEventPost(tap: u32, event: CFTypeRef);
}
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}
const ON_SCREEN_ONLY: u32 = 1;
const EXCLUDE_DESKTOP: u32 = 1 << 4;
const NUMBER_SINT32: isize = 3;
const HID_EVENT_TAP: u32 = 0;

/// Stationary game keys used by the clock adapter (macOS virtual key codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Space toggles pause.
    Pause,
    /// Keypad plus raises game speed.
    Faster,
    /// Keypad minus lowers game speed.
    Slower,
}
impl Key {
    fn code(self) -> u16 {
        match self {
            Self::Pause => 49,
            Self::Faster => 69,
            Self::Slower => 78,
        }
    }
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
    fn window_list_query_is_safe_without_permissions() {
        // The owner may be any app (or none); the query itself must not fail.
        let _ = frontmost_window_owner().unwrap();
    }
}
