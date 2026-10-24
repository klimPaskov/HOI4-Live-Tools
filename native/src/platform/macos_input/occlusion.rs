//! OS metadata only. No activation, event posting, capture or game reads.
use super::{CFTypeRef, Point, Rect, Window};
use crate::{
    error::{Error, Result},
    platform::mouse_occlusion::{self, Cursor, CursorOwner},
};
use std::{
    ffi::c_void,
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFGetTypeID(value: CFTypeRef) -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFBooleanGetValue(value: CFTypeRef) -> bool;
    fn CFStringGetLength(value: CFTypeRef) -> isize;
    fn CFStringGetCString(value: CFTypeRef, out: *mut u8, size: isize, encoding: u32) -> bool;
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGWindowAlpha: CFTypeRef;
    static kCGWindowIsOnscreen: CFTypeRef;
    static kCGWindowOwnerName: CFTypeRef;
    static kCGWindowName: CFTypeRef;
    fn CGWindowLevelForKey(key: i32) -> i32;
}

struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { super::CFRelease(self.0) }
        }
    }
}
fn invalid() -> Error {
    Error::new(
        "input_window_metadata",
        "Complete typed OS window metadata unavailable",
    )
}
unsafe fn value(dict: CFTypeRef, key: CFTypeRef, kind: usize) -> Result<CFTypeRef> {
    let value = super::CFDictionaryGetValue(dict, key);
    if value.is_null() || CFGetTypeID(value) != kind {
        return Err(invalid());
    }
    Ok(value)
}
unsafe fn integer(dict: CFTypeRef, key: CFTypeRef) -> Result<i32> {
    let value = value(dict, key, CFNumberGetTypeID())?;
    let mut result = 0i32;
    if !super::CFNumberGetValue(value, 3, (&mut result as *mut i32).cast::<c_void>()) {
        return Err(invalid());
    }
    Ok(result)
}
unsafe fn optional_text(dict: CFTypeRef, key: CFTypeRef) -> Result<Option<String>> {
    let value = super::CFDictionaryGetValue(dict, key);
    if value.is_null() {
        return Ok(None);
    }
    if CFGetTypeID(value) != CFStringGetTypeID() || !(0..=256).contains(&CFStringGetLength(value)) {
        return Err(invalid());
    }
    let mut buffer = [0u8; 1025];
    if !CFStringGetCString(
        value,
        buffer.as_mut_ptr(),
        buffer.len() as isize,
        0x08000100,
    ) {
        return Err(invalid());
    }
    let end = buffer.iter().position(|b| *b == 0).ok_or_else(invalid)?;
    Ok(Some(
        std::str::from_utf8(&buffer[..end])
            .map_err(|_| invalid())?
            .into(),
    ))
}
fn cursor_point() -> Result<[f64; 2]> {
    let event = Owned(unsafe { super::CGEventCreate(std::ptr::null()) });
    if event.0.is_null() {
        return Err(invalid());
    }
    let p = unsafe { super::CGEventGetLocation(event.0) };
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err(invalid());
    }
    Ok([p.x, p.y])
}

/// A closed, bounded metadata query uses ps's accessible kernel UID/start/path
/// fields. libproc denies WindowServer metadata on some supported desktops.
/// No shell, task port, privilege change, activation or cached PID exception.
fn system_cursor_owner(pid: i32) -> Result<CursorOwner> {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            if !matches!(self.0.try_wait(), Ok(Some(_))) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
    }
    let mut child = Child(
        Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "pid=,uid=,lstart=,comm="])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| invalid())?,
    );
    let until = Instant::now() + Duration::from_millis(250);
    loop {
        if let Some(status) = child.0.try_wait().map_err(|_| invalid())? {
            if !status.success() {
                return Err(invalid());
            }
            break;
        }
        if Instant::now() >= until {
            return Err(invalid());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let mut bytes = Vec::new();
    child
        .0
        .stdout
        .take()
        .ok_or_else(invalid)?
        .take(8193)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() > 8192 {
        return Err(invalid());
    }
    mouse_occlusion::cursor_owner(pid, std::str::from_utf8(&bytes).map_err(|_| invalid())?)
}

fn rect(r: Rect) -> [f64; 4] {
    [r.origin.x, r.origin.y, r.size.x, r.size.y]
}

/// Optional labels matter only for a hit at the public cursor level. Reading
/// unrelated titles is neither necessary for admission nor part of its proof.
fn candidate_names(
    level: i32,
    cursor_level: i32,
    bounds: [f64; 4],
    point: [f64; 2],
    read: impl FnOnce() -> Result<(Option<String>, Option<String>)>,
) -> Result<(Option<String>, Option<String>)> {
    if level == cursor_level
        && point[0] >= bounds[0]
        && point[1] >= bounds[1]
        && point[0] < bounds[0] + bounds[2]
        && point[1] < bounds[1] + bounds[3]
    {
        read()
    } else {
        Ok((None, None))
    }
}

/// The last cursor sample follows every potentially slow identity read. A
/// coherent owner refusal keeps its cause; it cannot be rescued by a new cursor.
fn finish_cursor_observation<T>(
    before: [f64; 2],
    read_cursor: impl FnOnce() -> Result<[f64; 2]>,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let result = body()?;
    if read_cursor()? != before {
        return Err(Error::new(
            "input_cursor_changed",
            "OS cursor moved during point occlusion observation",
        ));
    }
    Ok(result)
}

pub(super) fn require(target: Window, point: Point) -> Result<()> {
    let before = cursor_point()?;
    // Public kCGCursorWindowLevelKey ordinal19, not a captured level literal.
    let level = unsafe { CGWindowLevelForKey(19) };
    if level <= 0 {
        return Err(invalid());
    }
    let p = [point.x, point.y];
    // CGWindow.h specifies OnScreenOnly as complete front-to-back ordering.
    // Do not filter by level, alpha or owner before the point hit predicate.
    let list = Owned(unsafe { super::CGWindowListCopyWindowInfo(1, 0) });
    if list.0.is_null() || unsafe { CFGetTypeID(list.0) != CFArrayGetTypeID() } {
        return Err(invalid());
    }
    let count = unsafe { super::CFArrayGetCount(list.0) };
    if !(1..=4096).contains(&count) {
        return Err(invalid());
    }
    let mut windows = Vec::with_capacity(count as usize);
    for index in 0..count {
        // All borrowed values remain alive under the retained complete list.
        unsafe {
            let item = super::CFArrayGetValueAtIndex(list.0, index);
            if item.is_null() || CFGetTypeID(item) != CFDictionaryGetTypeID() {
                return Err(invalid());
            }
            let id = integer(item, super::kCGWindowNumber)?;
            if id <= 0 {
                return Err(invalid());
            }
            let mut bounds = Rect::default();
            let dict = value(item, super::kCGWindowBounds, CFDictionaryGetTypeID())?;
            if !super::CGRectMakeWithDictionaryRepresentation(dict, &mut bounds) {
                return Err(invalid());
            }
            let mut alpha = 0f64;
            let alpha_value = value(item, kCGWindowAlpha, CFNumberGetTypeID())?;
            if !super::CFNumberGetValue(alpha_value, 6, (&mut alpha as *mut f64).cast::<c_void>()) {
                return Err(invalid());
            }
            let onscreen = value(item, kCGWindowIsOnscreen, CFBooleanGetTypeID())?;
            let item_level = integer(item, super::kCGWindowLayer)?;
            let (owner_name, name) = candidate_names(item_level, level, rect(bounds), p, || {
                Ok((
                    optional_text(item, kCGWindowOwnerName)?,
                    optional_text(item, kCGWindowName)?,
                ))
            })?;
            windows.push(mouse_occlusion::Window {
                id: id as u32,
                owner: integer(item, super::kCGWindowOwnerPID)?,
                layer: item_level,
                bounds: rect(bounds),
                alpha,
                onscreen: CFBooleanGetValue(onscreen),
                owner_name,
                name,
            });
        }
    }
    let candidate = windows.iter().find(|w| {
        w.layer == level
            && p[0] >= w.bounds[0]
            && p[1] >= w.bounds[1]
            && p[0] < w.bounds[0] + w.bounds[2]
            && p[1] < w.bounds[1] + w.bounds[3]
    });
    let owner = finish_cursor_observation(before, cursor_point, || {
        let owner = candidate
            .map(|w| system_cursor_owner(w.owner))
            .transpose()?;
        if cursor_point()? != before {
            return Err(Error::new(
                "input_cursor_changed",
                "OS cursor moved during point occlusion observation",
            ));
        }
        if let (Some(w), Some(original)) = (candidate, &owner) {
            if system_cursor_owner(w.owner)? != *original {
                return Err(invalid());
            }
        }
        Ok(owner)
    })?;
    mouse_occlusion::require_target(
        p,
        target.id,
        target.owner,
        rect(target.bounds),
        &windows,
        owner.as_ref().map(|owner| Cursor {
            level,
            point: before,
            owner,
        }),
    )
}

#[cfg(test)]
mod tests;
