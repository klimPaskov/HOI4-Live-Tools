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
/// The longest window label decoded, in UTF-16 units. A window at the point may carry a long title (a terminal's
/// title holds its whole command line: 259 units on 28 Nov ended every click with input_window_metadata); the bound
/// keeps the read finite, and the buffer holds it in UTF-8 (at most 4 bytes a unit) with its terminator.
const LABEL_UNITS: isize = 1024;
unsafe fn optional_text(dict: CFTypeRef, key: CFTypeRef) -> Result<Option<String>> {
    let value = super::CFDictionaryGetValue(dict, key);
    if value.is_null() {
        return Ok(None);
    }
    label(value).map(Some)
}
unsafe fn label(value: CFTypeRef) -> Result<String> {
    if CFGetTypeID(value) != CFStringGetTypeID()
        || !(0..=LABEL_UNITS).contains(&CFStringGetLength(value))
    {
        return Err(invalid());
    }
    let mut buffer = vec![0u8; LABEL_UNITS as usize * 4 + 1];
    if !CFStringGetCString(
        value,
        buffer.as_mut_ptr(),
        buffer.len() as isize,
        0x08000100,
    ) {
        return Err(invalid());
    }
    let end = buffer.iter().position(|b| *b == 0).ok_or_else(invalid)?;
    Ok(std::str::from_utf8(&buffer[..end])
        .map_err(|_| invalid())?
        .into())
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
        crate::phase::sleep(Duration::from_millis(2));
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
            // Names are read for every window at the point: the cursor rule and the
            // Notification Center overlay rule in require_target both need them.
            let at_point = {
                let b = rect(bounds);
                p[0] >= b[0] && p[1] >= b[1] && p[0] < b[0] + b[2] && p[1] < b[1] + b[3]
            };
            let read_level = if at_point { level } else { item_level };
            let (owner_name, name) = candidate_names(read_level, level, rect(bounds), p, || {
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

/// Owners of the system's modal prompts (permission requests, keychain and security prompts,
/// alerts posted through CFUserNotification). One on screen takes the keyboard focus and clicks
/// meant for the game; only the machine's owner may answer it.
pub(super) const SYSTEM_PROMPTS: &[&str] = &[
    "UserNotificationCenter",
    "CoreServicesUIAgent",
    "SecurityAgent",
];

/// The first visible system prompt among `windows` (owner name), if any.
pub(super) fn prompt_among(windows: &[(Option<String>, bool, f64)]) -> Option<String> {
    windows
        .iter()
        .find(|(owner, onscreen, alpha)| {
            *onscreen
                && *alpha > 0.0
                && owner
                    .as_deref()
                    .is_some_and(|o| SYSTEM_PROMPTS.contains(&o))
        })
        .and_then(|(owner, _, _)| owner.clone())
}

/// A visible system prompt on screen (its owner), read from window owner names only, at most
/// once a second.
pub(super) fn system_prompt() -> Result<Option<String>> {
    use std::sync::Mutex;
    static RECENT: Mutex<Option<(Instant, Option<String>)>> = Mutex::new(None);
    if let Ok(cache) = RECENT.lock() {
        if let Some((at, found)) = cache.as_ref() {
            if at.elapsed() < Duration::from_secs(1) {
                return Ok(found.clone());
            }
        }
    }
    let rows: Vec<_> = owner_rows()?
        .into_iter()
        .map(|(owner, onscreen, alpha, _)| (owner, onscreen, alpha))
        .collect();
    let found = prompt_among(&rows);
    if let Ok(mut cache) = RECENT.lock() {
        *cache = Some((Instant::now(), found.clone()));
    }
    Ok(found)
}

/// Owner name, on-screen flag, alpha and level of one window.
pub(super) type OwnerRow = (Option<String>, bool, f64, i32);

/// Every window's row, front to back. Metadata only.
fn owner_rows() -> Result<Vec<OwnerRow>> {
    let list = Owned(unsafe { super::CGWindowListCopyWindowInfo(1, 0) });
    if list.0.is_null() || unsafe { CFGetTypeID(list.0) != CFArrayGetTypeID() } {
        return Err(invalid());
    }
    let count = unsafe { super::CFArrayGetCount(list.0) }.clamp(0, 4096);
    let mut rows = Vec::with_capacity(count as usize);
    for index in 0..count {
        unsafe {
            let item = super::CFArrayGetValueAtIndex(list.0, index);
            if item.is_null() || CFGetTypeID(item) != CFDictionaryGetTypeID() {
                continue;
            }
            let owner = optional_text(item, kCGWindowOwnerName).unwrap_or(None);
            let onscreen = super::CFDictionaryGetValue(item, kCGWindowIsOnscreen);
            let onscreen = !onscreen.is_null()
                && CFGetTypeID(onscreen) == CFBooleanGetTypeID()
                && CFBooleanGetValue(onscreen);
            let mut alpha = 1f64;
            let a = super::CFDictionaryGetValue(item, kCGWindowAlpha);
            if !a.is_null() && CFGetTypeID(a) == CFNumberGetTypeID() {
                super::CFNumberGetValue(a, 6, (&mut alpha as *mut f64).cast::<c_void>());
            }
            let level = integer(item, super::kCGWindowLayer).unwrap_or(0);
            rows.push((owner, onscreen, alpha, level));
        }
    }
    Ok(rows)
}

/// Whether the Dock is shown. The Dock draws through one display-sized window at the Dock
/// level, on screen only while the Dock shows; with automatic hiding it shows whenever the
/// pointer has rested at the Dock's screen edge. The occlusion check then refuses every point
/// of the game window (live 16 Nov 2026: 40 refusals in one run, "Window 11 owned by" the
/// Dock "at level 20").
pub(super) fn dock_shown() -> Result<bool> {
    // Public kCGDockWindowLevelKey ordinal 7.
    let level = unsafe { CGWindowLevelForKey(7) };
    Ok(dock_among(&owner_rows()?, level))
}

pub(super) fn dock_among(rows: &[OwnerRow], dock_level: i32) -> bool {
    rows.iter().any(|(owner, onscreen, alpha, level)| {
        *onscreen && *alpha > 0.0 && *level == dock_level && owner.as_deref() == Some("Dock")
    })
}

#[cfg(test)]
mod tests;
