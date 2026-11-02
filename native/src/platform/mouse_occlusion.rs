//! Point-specific OS window admission. This proves only the observed ordered
//! rectangle hit, never atomic event delivery or an in-game callback.
use crate::error::{Error, Result};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Window {
    pub id: u32,
    pub owner: i32,
    pub layer: i32,
    pub bounds: [f64; 4],
    pub alpha: f64,
    pub onscreen: bool,
    pub owner_name: Option<String>,
    pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CursorOwner {
    pid: i32,
    started: String,
    path: String,
}

/// Parse the closed /bin/ps field selection, not arbitrary process arguments.
/// Fresh kernel UID/path/start metadata is required; a cached PID or a Quartz
/// display name alone cannot establish the system cursor exception.
pub(crate) fn cursor_owner(pid: i32, text: &str) -> Result<CursorOwner> {
    let fields: Vec<_> = text.split_whitespace().collect();
    let valid_path = |p: &str| {
        matches!(
            p,
            "/System/Library/PrivateFrameworks/SkyLight.framework/Resources/WindowServer"
                | "/System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/Resources/WindowServer"
        )
    };
    if pid <= 0
        || fields.len() != 8
        || fields[0].parse::<i32>().ok() != Some(pid)
        || fields[1] != "88"
        || !matches!(
            fields[2],
            "Mon" | "Tue" | "Wed" | "Thu" | "Fri" | "Sat" | "Sun"
        )
        || !matches!(
            fields[3],
            "Jan"
                | "Feb"
                | "Mar"
                | "Apr"
                | "May"
                | "Jun"
                | "Jul"
                | "Aug"
                | "Sep"
                | "Oct"
                | "Nov"
                | "Dec"
        )
        || !fields[4].parse::<u8>().is_ok_and(|n| (1..=31).contains(&n))
        || !clock(fields[5])
        || !fields[6]
            .parse::<u16>()
            .is_ok_and(|n| (1970..=9999).contains(&n))
        || !valid_path(fields[7])
    {
        return Err(metadata(
            "System cursor owner identity unavailable or unsupported",
        ));
    }
    Ok(CursorOwner {
        pid,
        started: fields[2..7].join(" "),
        path: fields[7].into(),
    })
}

fn clock(s: &str) -> bool {
    let parts: Vec<_> = s.split(':').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_digit()))
        && parts[0].parse::<u8>().is_ok_and(|n| n < 24)
        && parts[1..]
            .iter()
            .all(|p| p.parse::<u8>().is_ok_and(|n| n < 60))
}

pub(crate) struct Cursor<'a> {
    pub level: i32,
    pub point: [f64; 2],
    pub owner: &'a CursorOwner,
}

fn metadata(message: &str) -> Error {
    Error::new("input_window_metadata", message)
}

fn valid_bounds(b: [f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite() && v.abs() <= 10_000_000.0) && b[2] > 0.0 && b[3] > 0.0
}

fn contains(b: [f64; 4], p: [f64; 2]) -> bool {
    p[0] >= b[0] && p[1] >= b[1] && p[0] < b[0] + b[2] && p[1] < b[1] + b[3]
}

/// Complete front-to-back OnScreenOnly metadata, with no layer filtering.
/// All positive-alpha and transparent foreign rectangles remain potential
/// receivers: alpha alone does not establish mouse click-through.
pub(crate) fn require_target(
    point: [f64; 2],
    id: u32,
    owner: i32,
    bounds: [f64; 4],
    windows: &[Window],
    cursor: Option<Cursor<'_>>,
) -> Result<()> {
    if !point.iter().all(|v| v.is_finite()) || !valid_bounds(bounds) || !contains(bounds, point) {
        return Err(Error::new(
            "invalid_arguments",
            "Pointer point is outside the pinned window",
        ));
    }
    if id == 0 || owner <= 0 || windows.is_empty() || windows.len() > 4096 {
        return Err(metadata("Invalid or incomplete ordered window collection"));
    }
    let mut ids = BTreeSet::new();
    for w in windows {
        if w.id == 0
            || w.owner <= 0
            || !ids.insert(w.id)
            || !valid_bounds(w.bounds)
            || !w.alpha.is_finite()
            || !(0.0..=1.0).contains(&w.alpha)
            || !w.onscreen
        {
            return Err(metadata(
                "Malformed, duplicate or off-screen window metadata",
            ));
        }
    }
    let selected = windows
        .iter()
        .find(|w| w.id == id)
        .ok_or_else(|| metadata("Pinned window absent"))?;
    if selected.owner != owner
        || selected.bounds != bounds
        || selected.layer != 0
        || selected.alpha != 1.0
    {
        return Err(metadata(
            "Pinned window identity, frame, level or opacity changed",
        ));
    }
    let mut ignored_cursor = false;
    for w in windows.iter().filter(|w| contains(w.bounds, point)) {
        if let Some(c) = &cursor {
            if w.layer == c.level
                && w.owner == c.owner.pid
                && w.owner_name.as_deref() == Some("Window Server")
                && w.name.as_deref() == Some("Cursor")
                && w.alpha == 1.0
                && w.bounds[2] <= 64.0
                && w.bounds[3] <= 64.0
                && c.point.iter().all(|v| v.is_finite())
                && contains(w.bounds, c.point)
                && !ignored_cursor
            {
                ignored_cursor = true;
                continue;
            }
        }
        if w.id == id && w.owner == owner {
            return Ok(());
        }
        return Err(Error::new(
            "input_point_occluded",
            format!(
                "Window {} owned by PID {} at level {} covers the pointer target",
                w.id, w.owner, w.layer
            ),
        ));
    }
    Err(metadata("No ordered window contains the pointer point"))
}

#[cfg(test)]
mod tests;
