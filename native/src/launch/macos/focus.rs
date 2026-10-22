//! Start one own national focus with ordinary input: politics panel (Q), its
//! national-focus button, panning the tree until the focus is in view, the
//! focus symbol, then the detail window's Start button. Targets come from the
//! live GUI tree (zoom-aware), each stage waits for its memory change, and
//! success is the own focus progress naming the focus as active.
use super::ui::clear_notices;
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    own_focus,
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use std::{
    collections::BTreeSet,
    thread,
    time::{Duration, Instant},
};

/// Focus UI joins of one inspected build.
struct Ui {
    manager: u64,
    politics_view: u64,
    focus_view: u64,
    view_root: u64,
    displayed_country: u64,
    items: u64,
    item_vtable: u64,
    item_focus: u64,
    item_container: u64,
    zoom: u64,
    panels: u64,
    panel_vtable: u64,
    panel_window: u64,
    panel_selected: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run).
/// Panel joins (+0xb90 idler, +0xba0 window, +0xba8 selected) and the view's
/// panel list (+0x1938) and displayed country (+0x1990) are the Linux
/// reader's offsets translated by aligned methods and confirmed live; the zoom
/// float matched a measured node-spacing ratio, and drag panning is 1:1.
const MACOS: Ui = Ui {
    manager: 0x670,
    politics_view: 0x128,
    focus_view: 0x130,
    view_root: 0x430,
    displayed_country: 0x1990,
    items: 0x18f0,
    item_vtable: 0x3405b78,
    item_focus: 0x20,
    item_container: 0x10,
    zoom: 0x19b0,
    panels: 0x1938,
    panel_vtable: 0x3405158,
    panel_window: 0xba0,
    panel_selected: 0xba8,
};
const GAME: (u32, u32) = (1920, 1080);
mod binding;
use binding::{Control, Owner};
/// Tree area clear of the title bar, filter buttons and the bottom scrollbar.
const SAFE: (f64, f64, f64, f64) = (260.0, 200.0, 1860.0, 1000.0);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Focus input is not reviewed for this build",
        )),
    }
}
fn inside(rect: &Rect) -> bool {
    let (x, y) = rect.center();
    (SAFE.0..SAFE.2).contains(&x) && (SAFE.1..SAFE.3).contains(&y)
}
/// The visible tree area, for a node the tree cannot scroll any further.
fn on_tree(rect: &Rect) -> bool {
    let (x, y) = rect.center();
    (240.0..1900.0).contains(&x) && (180.0..1040.0).contains(&y)
}
/// A tree-area point at least 24 px from every focus widget.
fn background(all: &[Rect]) -> Option<(f64, f64)> {
    let clear = |x: f64, y: f64| {
        all.iter().all(|r| {
            x < f64::from(r.x) - 24.0
                || x > f64::from(r.x + r.w) + 24.0
                || y < f64::from(r.y) - 24.0
                || y > f64::from(r.y + r.h) + 24.0
        })
    };
    let mut y = 600.0;
    while y < 960.0 {
        let mut x = 960.0;
        while x < 1700.0 {
            for px in [x, 1920.0 - x] {
                if clear(px, y) {
                    return Some((px, y));
                }
            }
            x += 37.0;
        }
        y += 29.0;
    }
    None
}
pub(super) type Progress = own_focus::ProgressSnapshot;
pub(super) fn progress(process: &dyn Process) -> Result<Progress> {
    own_focus::progress_snapshot(process)
}
/// Refusals that need no input.
pub(super) fn check(before: &Progress, key: &str) -> Result<()> {
    check_state(before.active_key(), before.completed(), key)
}
fn check_state(active: Option<&str>, completed: &BTreeSet<String>, key: &str) -> Result<()> {
    if completed.contains(key) {
        return Err(Error::new("invalid_target", "Focus already completed"));
    }
    match active {
        Some(active) if active == key => Err(Error::new("invalid_target", "Focus already active")),
        Some(_) => Err(Error::new(
            "invalid_target",
            "Another focus is active; it must finish or be cancelled first",
        )),
        None => Ok(()),
    }
}

fn deadline(until: Instant) -> Result<()> {
    if Instant::now() >= until {
        return Err(Error::new(
            "action_deadline",
            "Original Focus deadline expired",
        ));
    }
    Ok(())
}
fn sample<T>(
    process: &dyn Process,
    owner: &Owner,
    key: &str,
    f: impl FnOnce(&Reader<'_>, u64, &Owner) -> Result<T>,
) -> Result<T> {
    binding::observe(process, key, Some(owner), f).map(|(_, value)| value)
}
/// Only complete discarded torn snapshots may be retried, inside the original
/// deadline. Scope, input and ownership failures retain their exact cause.
fn wait_for<T>(until: Instant, mut probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    let end = until.min(Instant::now() + Duration::from_secs(3));
    loop {
        deadline(end)?;
        let result = probe();
        deadline(end)?;
        match result {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) if error.code == "torn_read" => {}
            Err(error) => return Err(error),
        }
        thread::sleep(Duration::from_millis(30).min(end.saturating_duration_since(Instant::now())));
    }
}
fn settle<T: PartialEq>(until: Instant, mut probe: impl FnMut() -> Result<T>) -> Result<T> {
    let end = until.min(Instant::now() + Duration::from_secs(2));
    let mut previous = None;
    wait_for(end, || {
        let current = match probe() {
            Ok(current) => current,
            Err(error) => {
                previous = None;
                return Err(error);
            }
        };
        if previous.as_ref() == Some(&current) {
            return Ok(Some(current));
        }
        previous = Some(current);
        thread::sleep(Duration::from_millis(30).min(end.saturating_duration_since(Instant::now())));
        Ok(None)
    })
}
fn unchanged<T: PartialEq>(expected: &T, current: &T) -> Result<()> {
    if expected != current {
        return Err(Error::new(
            "focus_state_changed",
            "Original Focus control changed before mouse-down",
        ));
    }
    Ok(())
}
fn click(
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys,
    candidate: &Control,
    until: Instant,
    probe: impl Fn() -> Result<Control> + Send + Sync,
) -> Result<()> {
    if candidate.disabled {
        return Err(Error::new(
            "invalid_target",
            "The game disables this Focus control",
        ));
    }
    let check = || {
        deadline(until)?;
        unchanged(candidate, &probe()?)?;
        deadline(until)
    };
    let guarded = keys.guarded(process, &check)?;
    let (x, y) = candidate.rect.center();
    guarded.click(GAME, x, y)?;
    Ok(())
}

/// Ordinary input sequence under the caller's retained intent, lease and one
/// original Fence. New stages cannot extend that deadline.
pub(super) fn start(
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys,
    key: &str,
    until: Instant,
) -> Result<()> {
    deadline(until)?;
    let (owner, ()) = binding::observe(process, key, None, |_, _, _| Ok(()))?;
    clear_notices(process, keys)?;
    if !sample(process, &owner, key, |r, i, o| {
        binding::open(r, i, o, MACOS.focus_view)
    })? {
        if !sample(process, &owner, key, |r, i, o| {
            binding::open(r, i, o, MACOS.politics_view)
        })? {
            // No fixed-coordinate fallback after a failed key or read.
            keys.press(Key::letter('q')?)?;
            wait_for(until, || {
                Ok(sample(process, &owner, key, |r, i, o| {
                    binding::open(r, i, o, MACOS.politics_view)
                })?
                .then_some(()))
            })?;
        }
        let probe = || sample(process, &owner, key, binding::focus_button);
        let candidate = settle(until, probe)?;
        click(process, keys, &candidate, until, probe)?;
        wait_for(until, || {
            Ok(sample(process, &owner, key, |r, i, o| {
                binding::open(r, i, o, MACOS.focus_view)
            })?
            .then_some(()))
        })?;
    }
    wait_for(until, || {
        Ok(sample(process, &owner, key, binding::symbols)?
            .target
            .map(|_| ()))
    })?;
    let mut previous: Option<Rect> = None;
    let mut revealed = false;
    for _ in 0..12 {
        deadline(until)?;
        let tree = settle(until, || sample(process, &owner, key, binding::symbols))?;
        let rect = tree
            .target
            .as_ref()
            .ok_or_else(|| Error::new("gui_target_unavailable", "Own Focus node disappeared"))?
            .rect;
        let stuck = previous.is_some_and(|p| {
            let (a, b) = (p.center(), rect.center());
            (a.0 - b.0).abs() < 5.0 && (a.1 - b.1).abs() < 5.0
        });
        if inside(&rect) || (stuck && on_tree(&rect)) {
            revealed = true;
            break;
        }
        previous = Some(rect);
        let all: Vec<_> = tree.obstacles.iter().map(|(_, r)| *r).collect();
        let from = background(&all).ok_or_else(|| {
            Error::new(
                "gui_target_unavailable",
                "No verified empty tree area to pan",
            )
        })?;
        let (cx, cy) = rect.center();
        let to = (
            (from.0 + (960.0 - cx).clamp(-700.0, 700.0)).clamp(SAFE.0, SAFE.2),
            (from.1 + (600.0 - cy).clamp(-350.0, 350.0)).clamp(SAFE.1, SAFE.3),
        );
        let scope = || {
            deadline(until)?;
            sample(process, &owner, key, |r, i, o| {
                binding::same_view(r, i, o, &tree)
            })?;
            deadline(until)
        };
        let before_down = || {
            unchanged(&tree, &sample(process, &owner, key, binding::symbols)?)?;
            deadline(until)
        };
        let guarded = keys.guarded(process, &scope)?;
        guarded.drag_checked(GAME, from, to, &before_down)?;
        deadline(until)?;
        thread::sleep(
            Duration::from_millis(150).min(until.saturating_duration_since(Instant::now())),
        );
    }
    if !revealed {
        return Err(Error::new(
            "gui_target_unavailable",
            "Focus could not be panned into view",
        ));
    }
    let node = || {
        sample(process, &owner, key, binding::symbols)?
            .target
            .ok_or_else(|| Error::new("gui_target_unavailable", "Own Focus node disappeared"))
    };
    let candidate = settle(until, node)?;
    if !on_tree(&candidate.rect) {
        return Err(Error::new(
            "gui_target_unavailable",
            "Focus node drifted outside the tree",
        ));
    }
    click(process, keys, &candidate, until, node)?;
    wait_for(until, || sample(process, &owner, key, binding::detail))?;
    let detail = || {
        sample(process, &owner, key, binding::detail)?
            .ok_or_else(|| Error::new("input_readback_failed", "Own Focus detail closed"))
    };
    let candidate = settle(until, detail)?;
    click(process, keys, &candidate, until, detail)?;
    wait_for(until, || {
        Ok((progress(process)?.active_key() == Some(key)).then_some(()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_deadline_refuses_without_probe_and_scope_errors_keep_their_cause() {
        let mut called = false;
        let result: Result<()> = wait_for(Instant::now(), || {
            called = true;
            Ok(Some(()))
        });
        assert_eq!(result.unwrap_err().code, "action_deadline");
        assert!(!called);
        for code in [
            "input_lease_required",
            "session_closed",
            "ownership_changed",
            "foreign_focus_scope",
            "invalid_gui_geometry",
        ] {
            let mut probes = 0;
            let result: Result<()> = wait_for(Instant::now() + Duration::from_secs(1), || {
                probes += 1;
                Err(Error::new(code, "fixture"))
            });
            assert_eq!(result.unwrap_err().code, code);
            assert_eq!(probes, 1);
        }
    }
    #[test]
    fn only_discarded_torn_snapshots_recover_and_late_success_is_rejected() {
        let mut probes = 0;
        let value = wait_for(Instant::now() + Duration::from_secs(1), || {
            probes += 1;
            if probes == 1 {
                Err(Error::new("torn_read", "fixture"))
            } else {
                Ok(Some(7))
            }
        })
        .unwrap();
        assert_eq!((value, probes), (7, 2));
        let until = Instant::now() + Duration::from_millis(10);
        let result = wait_for(until, || {
            thread::sleep(until.saturating_duration_since(Instant::now()));
            Ok(Some(7))
        });
        assert_eq!(result.unwrap_err().code, "action_deadline");
    }
    #[test]
    fn refusals_need_no_input() {
        let completed = ["GER_a".to_string()].into();
        assert!(check_state(None, &completed, "GER_b").is_ok());
        assert_eq!(
            check_state(None, &completed, "GER_a").unwrap_err().code,
            "invalid_target"
        );
        assert_eq!(
            check_state(Some("GER_b"), &completed, "GER_b")
                .unwrap_err()
                .code,
            "invalid_target"
        );
        assert_eq!(
            check_state(Some("GER_c"), &completed, "GER_b")
                .unwrap_err()
                .code,
            "invalid_target"
        );
    }
    #[test]
    fn background_avoids_every_widget() {
        let busy = vec![Rect {
            x: 900,
            y: 560,
            w: 200,
            h: 100,
        }];
        let (x, y) = background(&busy).unwrap();
        assert!(!(876.0..=1124.0).contains(&x) || !(536.0..=684.0).contains(&y));
        assert!(inside(&Rect {
            x: 950,
            y: 590,
            w: 20,
            h: 20
        }));
        assert!(!inside(&Rect {
            x: 4000,
            y: 590,
            w: 20,
            h: 20
        }));
    }
}
