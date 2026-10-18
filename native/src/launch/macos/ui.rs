//! Shared fresh-read helpers for ordinary macOS UI actions.
use crate::{
    error::{Error, Result},
    game, gui_tree,
    memory::{Reader, StringAbi},
    platform::Process,
};
use std::{
    thread,
    time::{Duration, Instant},
};

const STAGE_WAIT: Duration = Duration::from_secs(3);
const SETTLE_WAIT: Duration = Duration::from_secs(2);

/// One fresh read bracket; transient UI churn is retried a few times.
pub(super) fn read<T>(
    process: &dyn Process,
    f: impl Fn(&Reader<'_>, u64) -> Result<T>,
) -> Result<T> {
    let mut attempt = 0;
    loop {
        process.begin_snapshot()?;
        let r = Reader::new(process);
        match f(&r, process.image_base()) {
            Err(e)
                if attempt < 3
                    && matches!(
                        e.code,
                        "torn_read" | "short_read" | "invalid_gui" | "invalid_array"
                    ) =>
            {
                attempt += 1;
                thread::sleep(Duration::from_millis(5));
            }
            result => {
                process.check()?;
                return result;
            }
        }
    }
}

/// Bounded wait for a memory predicate after an input.
pub(super) fn wait(
    process: &dyn Process,
    stage: &str,
    f: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<()> {
    let deadline = Instant::now() + STAGE_WAIT;
    loop {
        if read(process, &f)? {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                format!("{stage}: expected game change not observed"),
            ));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

/// No-decision notices the server may acknowledge (only an OK button): research
/// finished, focus finished, peace summary and world tension pop-ups (macOS
/// 089EA519). Decisions such as events are never answered here.
const NOTICE_VTABLES: [u64; 4] = [0x3472580, 0x3472af8, 0x3472c10, 0x3493e80];
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0x9e0;
const POPUP_ROOT: u64 = 0x430;

/// Shown notice pop-ups (list order) with their OK button rectangles.
fn notices(r: &Reader<'_>, image: u64) -> Result<Vec<gui_tree::Rect>> {
    if r.string_abi() != StringAbi::LibCxx {
        return Ok(Vec::new());
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    let (_, data, count) = r.array_header(gui + POPUP_LIST)?;
    let mut out = Vec::new();
    for index in 0..count.min(32) as u64 {
        let window = r.u64(data + index * 8)?;
        let kind = r.u64(window)?;
        if !NOTICE_VTABLES.iter().any(|vt| image + vt == kind) {
            continue;
        }
        let Some(root) = gui_tree::container(r, r.u64(window + POPUP_ROOT)?, image)? else {
            continue;
        };
        if !gui_tree::visible(r, root)? {
            continue;
        }
        if let Some(ok) = gui_tree::child(r, image, root, "ok_button")? {
            // A notice whose button is laid out off screen cannot block a click.
            if gui_tree::shown(r, ok)? {
                if let Ok(rect) = gui_tree::rect(r, image, ok, (1920, 1080)) {
                    out.push(rect);
                }
            }
        }
    }
    Ok(out)
}
/// Acknowledge shown notice pop-ups so they cannot swallow the next click.
/// Stacking order is not in the list, so candidates are tried from the end;
/// a click counts when any notice closes (a covered OK lands harmlessly on the
/// covering pop-up's body).
pub(super) fn clear_notices(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
) -> Result<usize> {
    let mut cleared = 0;
    let mut misses = 0;
    loop {
        let shown = settled(process, "notice", notices)?;
        let Some(&target) = shown.iter().rev().nth(misses) else {
            return if shown.is_empty() {
                Ok(cleared)
            } else {
                Err(Error::new("ui_blocked", "Notice pop-ups did not close"))
            };
        };
        if cleared + misses >= 12 {
            return Err(Error::new("ui_blocked", "Too many notice pop-ups"));
        }
        let (x, y) = target.center();
        keys.click((1920, 1080), x, y)?;
        let before = shown.len();
        match wait(process, "dismiss notice", |r, i| {
            Ok(notices(r, i)?.len() < before)
        }) {
            Ok(()) => {
                cleared += 1;
                misses = 0;
            }
            Err(_) => misses += 1,
        }
    }
}
/// Press Escape (at most three times) until `closed` holds; best effort, used
/// to leave the UI tidy after the game refuses a target.
pub(super) fn dismiss(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
    closed: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) {
    for _ in 0..3 {
        if read(process, &closed).unwrap_or(true) {
            return;
        }
        if keys
            .press(crate::platform::macos_input::Key::Escape)
            .is_err()
        {
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
}
/// A click target whose rectangle is identical in two reads 60 ms apart, so
/// sliding panels and animated windows have come to rest before input.
pub(super) fn settled<T: PartialEq>(
    process: &dyn Process,
    stage: &str,
    f: impl Fn(&Reader<'_>, u64) -> Result<T>,
) -> Result<T> {
    let deadline = Instant::now() + SETTLE_WAIT;
    let mut last = read(process, &f)?;
    loop {
        thread::sleep(Duration::from_millis(60));
        let now = read(process, &f)?;
        if now == last {
            return Ok(now);
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "gui_target_unsettled",
                format!("{stage}: click target kept moving"),
            ));
        }
        last = now;
    }
}
