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
    focus_view_vtable: u64,
    view_root: u64,
    country_override: u64,
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
/// panel list (+0x1938) and country override (+0x1990) are the Linux
/// reader's offsets translated by aligned methods and confirmed live; the zoom
/// float matched a measured node-spacing ratio, and drag panning is 1:1.
/// Exact Mac getter 0x1be2620 resolves a null override through the current tag;
/// only its positive own-player path is admitted here. Constructor 0x1be0610
/// and manager store 0x20e94f5 establish the exact primary in the focus slot.
const MACOS: Ui = Ui {
    manager: 0x670,
    politics_view: 0x128,
    focus_view: 0x130,
    focus_view_vtable: 0x3405d08,
    view_root: 0x430,
    country_override: 0x1990,
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
mod pan;
mod probe;
use binding::{Control, Owner};
pub(crate) use probe::snapshot as input_probe;
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
fn wait_for<T>(until: Instant, probe: impl FnMut() -> Result<Option<T>>) -> Result<T> {
    wait_window(
        until,
        Duration::from_secs(3),
        "input_readback_failed",
        Duration::from_millis(30),
        probe,
    )
}
/// Wait for the exact own-bound view to become visible inside the original
/// deadline without clicking the opener again. A foreign override remains an
/// immediate refusal; a null override uses the witnessed positive own player.
fn wait_for_tree(
    until: Instant,
    mut probe: impl FnMut() -> Result<binding::FocusViewState>,
) -> Result<()> {
    wait_for(until, || {
        Ok((probe()? == binding::FocusViewState::Ready).then_some(()))
    })
}
fn wait_window<T>(
    until: Instant,
    window: Duration,
    timeout_code: &'static str,
    interval: Duration,
    mut probe: impl FnMut() -> Result<Option<T>>,
) -> Result<T> {
    let end = until.min(Instant::now() + window);
    let check = || {
        deadline(until)?;
        if Instant::now() >= end {
            return Err(Error::new(
                timeout_code,
                "Focus stage observation window expired",
            ));
        }
        Ok(())
    };
    loop {
        check()?;
        // A known ownership/lease/cancellation refusal remains the cause even
        // if its native read also consumed the remaining time. Only discarded
        // torn reads and successful payloads reach the elapsed-time check.
        let result = match probe() {
            Err(error) if error.code != "torn_read" => return Err(error),
            result => result,
        };
        check()?;
        match result {
            Ok(Some(value)) => return Ok(value),
            Ok(None) => {}
            Err(error) if error.code == "torn_read" => {}
            Err(error) => return Err(error),
        }
        thread::sleep(interval.min(end.saturating_duration_since(Instant::now())));
    }
}
fn settle<T: PartialEq>(until: Instant, mut probe: impl FnMut() -> Result<T>) -> Result<T> {
    let mut previous = None;
    wait_window(
        until,
        Duration::from_secs(2),
        "gui_target_unsettled",
        Duration::from_millis(60),
        || {
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
            Ok(None)
        },
    )
}
/// Diagnostics describe this invocation only. Stage labels are static and no
/// address, journal token or game payload is included. A requested drag is not
/// evidence that mouse-down was delivered.
struct Trace {
    stage: &'static str,
    started: Instant,
    stage_started: Instant,
    pan_requests: u32,
}
impl Trace {
    fn new() -> Self {
        let started = Instant::now();
        Self {
            stage: "initial",
            started,
            stage_started: started,
            pan_requests: 0,
        }
    }
    fn stage(&mut self, stage: &'static str) {
        self.stage = stage;
        self.stage_started = Instant::now();
    }
    fn failure(&self, error: Error) -> Error {
        Error::new(
            error.code,
            format!(
                "stage={}; stage_elapsed_ms={:.3}; total_elapsed_ms={:.3}; pan_requests={}; {}",
                self.stage,
                self.stage_started.elapsed().as_secs_f64() * 1000.0,
                self.started.elapsed().as_secs_f64() * 1000.0,
                self.pan_requests,
                error.message
            ),
        )
    }
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
#[derive(Clone, Copy, PartialEq, Debug)]
enum ClickStage {
    Opener,
    Node,
    Start,
}
/// A disabled control is refused before its click. The detail window's Start
/// button is disabled by the game when the focus is not available now.
fn disabled_error(stage: ClickStage) -> Error {
    match stage {
        ClickStage::Start => Error::new(
            "focus_not_available",
            "The game shows Start disabled for this focus: its requirements are unmet or a mutually \
             exclusive focus was taken. Observed: the detail window is open and Start is disabled; the \
             Start button was not clicked. Not read: the game's reason text, so the specific unmet \
             requirement is unknown. Choose another focus or satisfy its requirements first.",
        ),
        ClickStage::Node | ClickStage::Opener => Error::new(
            "invalid_target",
            "The game disables this Focus control",
        ),
    }
}
/// The node click was sent but the detail window never read as open. Only the
/// readback timeout is retyped; ownership, lease and deadline causes stay.
fn detail_not_opened(error: Error) -> Error {
    if error.code != "input_readback_failed" {
        return error;
    }
    Error::new(
        "focus_detail_not_opened",
        "Observed: one click on the focus node was sent but its detail window was not read as open \
         within 3 s; Start was not clicked. Not inferred: whether the click missed, a pop-up took it \
         or the game was still animating. Re-observe with hoi4_focus_selected_detail before calling \
         hoi4_focus_start again.",
    )
}
fn click(
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys,
    candidate: &Control,
    until: Instant,
    probe: impl Fn() -> Result<Control> + Send + Sync,
    stage: ClickStage,
) -> Result<()> {
    if candidate.disabled {
        return Err(disabled_error(stage));
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
    let mut trace = Trace::new();
    start_inner(process, keys, key, until, &mut trace, None).map_err(|error| trace.failure(error))
}
pub(super) fn start_bound(
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys,
    key: &str,
    until: Instant,
    expected: &own_focus::AssignmentSnapshot,
) -> Result<()> {
    let mut trace = Trace::new();
    start_inner(process, keys, key, until, &mut trace, Some(expected))
        .map_err(|error| trace.failure(error))
}
fn start_inner(
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys,
    key: &str,
    until: Instant,
    trace: &mut Trace,
    expected: Option<&own_focus::AssignmentSnapshot>,
) -> Result<()> {
    trace.stage("bind_own_target");
    deadline(until)?;
    let owner = match expected {
        Some(before) => binding::assignment_owner(process, key, before)?,
        None => binding::observe(process, key, None, |_, _, _| Ok(()))?.0,
    };
    trace.stage("clear_notices");
    clear_notices(process, keys)?;
    trace.stage("inspect_open_views");
    if sample(process, &owner, key, binding::view_state)? == binding::FocusViewState::Hidden {
        if !sample(process, &owner, key, |r, i, o| {
            binding::open(r, i, o, MACOS.politics_view)
        })? {
            // No fixed-coordinate fallback after a failed key or read.
            trace.stage("open_politics");
            keys.press(Key::letter('q')?)?;
            wait_for(until, || {
                Ok(sample(process, &owner, key, |r, i, o| {
                    binding::open(r, i, o, MACOS.politics_view)
                })?
                .then_some(()))
            })?;
        }
        trace.stage("focus_opener");
        let probe = || sample(process, &owner, key, binding::focus_button);
        let candidate = settle(until, probe)?;
        click(process, keys, &candidate, until, probe, ClickStage::Opener)?;
    }
    trace.stage("open_tree_readback");
    wait_for_tree(until, || sample(process, &owner, key, binding::view_state))?;
    trace.stage("tree_layout");
    wait_for(until, || {
        Ok(sample(process, &owner, key, binding::node)?.map(|_| ()))
    })?;
    let mut previous: Option<Rect> = None;
    let mut revealed = false;
    for _ in 0..12 {
        trace.stage("pan_candidate");
        deadline(until)?;
        let ready = |rect: &Rect| {
            let stuck = previous.is_some_and(|p| {
                let (a, b) = (p.center(), rect.center());
                (a.0 - b.0).abs() < 5.0 && (a.1 - b.1).abs() < 5.0
            });
            inside(rect) || (stuck && on_tree(rect))
        };
        // Deciding that no drag is needed requires only the exact target.
        // Node settlement and the final click still re-observe it below.
        let node = sample(process, &owner, key, binding::node)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Own Focus node disappeared"))?;
        if ready(&node.rect) {
            revealed = true;
            break;
        }
        // A drag always uses a newly settled complete obstacle snapshot. Never
        // use the lighter node result to select or authorize a drag origin.
        let tree = settle(until, || sample(process, &owner, key, binding::symbols))?;
        let rect = tree
            .target
            .as_ref()
            .ok_or_else(|| Error::new("gui_target_unavailable", "Own Focus node disappeared"))?
            .rect;
        if ready(&rect) {
            revealed = true;
            break;
        }
        previous = Some(rect);
        let all: Vec<_> = tree.obstacles.iter().map(|(_, r)| *r).collect();
        let Some(pan) = pan::plan(&rect, &all)? else {
            revealed = true;
            break;
        };
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
        trace.stage("pan_transport");
        trace.pan_requests += 1;
        guarded.drag_checked(GAME, pan.from, pan.to, &before_down)?;
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
    trace.stage("node_candidate_and_click");
    let node = || {
        sample(process, &owner, key, binding::node)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Own Focus node disappeared"))
    };
    let candidate = settle(until, node)?;
    if !on_tree(&candidate.rect) {
        return Err(Error::new(
            "gui_target_unavailable",
            "Focus node drifted outside the tree",
        ));
    }
    click(process, keys, &candidate, until, node, ClickStage::Node)?;
    trace.stage("detail_open_readback");
    wait_for(until, || sample(process, &owner, key, binding::detail)).map_err(detail_not_opened)?;
    trace.stage("start_candidate_and_click");
    let detail = || {
        sample(process, &owner, key, binding::detail)?
            .ok_or_else(|| Error::new("input_readback_failed", "Own Focus detail closed"))
    };
    let candidate = settle(until, detail)?;
    click(process, keys, &candidate, until, detail, ClickStage::Start)?;
    trace.stage("activation_readback");
    wait_for(until, || {
        Ok((progress(process)?.active_key() == Some(key)).then_some(()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_start_is_typed_with_the_limits_of_the_evidence() {
        let e = disabled_error(ClickStage::Start);
        assert_eq!(e.code, "focus_not_available");
        assert!(e.message.contains("not clicked"));
        assert!(e.message.contains("unmet") && e.message.contains("exclusive"));
        for stage in [ClickStage::Opener, ClickStage::Node] {
            assert_eq!(disabled_error(stage).code, "invalid_target");
        }
    }
    #[test]
    fn detail_timeout_is_typed_but_other_causes_survive() {
        let e = detail_not_opened(Error::new("input_readback_failed", "window expired"));
        assert_eq!(e.code, "focus_detail_not_opened");
        assert!(e.message.contains("hoi4_focus_selected_detail"));
        for code in [
            "action_deadline",
            "ownership_changed",
            "foreign_focus_scope",
        ] {
            assert_eq!(detail_not_opened(Error::new(code, "x")).code, code);
        }
    }
    #[test]
    fn tree_readiness_waits_for_positive_own_binding_without_preparation_or_input() {
        use binding::FocusViewState::{Hidden, Ready};
        let mut states = [Ok(Hidden), Ok(Hidden), Ok(Ready)].into_iter();
        let mut observations = 0;
        wait_for_tree(Instant::now() + Duration::from_secs(1), || {
            observations += 1;
            states.next().unwrap()
        })
        .unwrap();
        assert_eq!(observations, 3);
        assert!(states.next().is_none());
    }
    #[test]
    fn tree_readiness_does_not_retry_foreign_country_or_extend_original_deadline() {
        let mut observations = 0;
        let error = wait_for_tree(Instant::now() + Duration::from_secs(1), || {
            observations += 1;
            Err(Error::new(
                "foreign_focus_scope",
                "different displayed country",
            ))
        })
        .unwrap_err();
        assert_eq!(error.code, "foreign_focus_scope");
        assert_eq!(observations, 1);
        let until = Instant::now() + Duration::from_millis(10);
        let error = wait_for_tree(until, || Ok(binding::FocusViewState::Hidden)).unwrap_err();
        assert_eq!(error.code, "action_deadline");
    }
    #[test]
    fn stage_window_expiry_is_distinct_from_the_original_action_deadline() {
        let future = Instant::now() + Duration::from_secs(10);
        for code in ["input_readback_failed", "gui_target_unsettled"] {
            let result: Result<()> = wait_window(
                future,
                Duration::ZERO,
                code,
                Duration::from_millis(30),
                || panic!("expired stage must not probe"),
            );
            assert_eq!(result.unwrap_err().code, code);
        }
        let result: Result<()> = wait_window(
            Instant::now(),
            Duration::from_secs(10),
            "input_readback_failed",
            Duration::from_millis(30),
            || panic!("expired action must not probe"),
        );
        assert_eq!(result.unwrap_err().code, "action_deadline");
    }
    #[test]
    fn diagnostic_trace_retains_failure_code_and_labels_requests_without_claiming_input() {
        let mut trace = Trace::new();
        trace.stage("pan_transport");
        trace.pan_requests = 2;
        let error = trace.failure(Error::new("input_lease_required", "original cause"));
        assert_eq!(error.code, "input_lease_required");
        assert!(error
            .message
            .starts_with("stage=pan_transport; stage_elapsed_ms="));
        assert!(error.message.contains("total_elapsed_ms="));
        assert!(error.message.ends_with("pan_requests=2; original cause"));
        trace.stage("activation_readback");
        assert!(trace
            .failure(Error::new("action_deadline", "late"))
            .message
            .starts_with("stage=activation_readback;"));
    }
    #[test]
    fn late_native_refusal_retains_its_ownership_cause() {
        let until = Instant::now() + Duration::from_secs(1);
        let result: Result<()> = wait_for(until, || {
            thread::sleep(until.saturating_duration_since(Instant::now()));
            Err(Error::new("ownership_changed", "original native refusal"))
        });
        let error = result.unwrap_err();
        assert_eq!(error.code, "ownership_changed");
        assert_eq!(error.message, "original native refusal");
    }
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
    fn node_visibility_uses_safe_center_bounds() {
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
