use super::*;
use crate::{
    launch::macos::journal::Status,
    platform::{key_once, Identity},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};
#[derive(Clone, Copy, Default)]
enum Fail {
    #[default]
    None,
    Prepare,
    InitialCheck,
    Allocate,
    FinalCheck,
    Down,
    Up,
    Readback,
}
fn error() -> Error {
    Error::new("torn_read", "fixture refusal around delivery")
}
struct Pair {
    fail: Fail,
    trace: Rc<RefCell<Vec<&'static str>>>,
}
impl key_once::Pair for Pair {
    fn down(&mut self) -> Result<()> {
        self.trace.borrow_mut().push("down");
        if matches!(self.fail, Fail::Down) {
            Err(error())
        } else {
            Ok(())
        }
    }
    fn up(&mut self) -> Result<()> {
        self.trace.borrow_mut().push("up");
        if matches!(self.fail, Fail::Up) {
            Err(error())
        } else {
            Ok(())
        }
    }
}
struct Actor {
    inactive: bool,
    fail: Fail,
    trace: Rc<RefCell<Vec<&'static str>>>,
    presses: usize,
    finishes: usize,
}
impl Actor {
    fn new(inactive: bool, fail: Fail) -> Self {
        Self {
            inactive,
            fail,
            trace: Rc::new(RefCell::new(vec![])),
            presses: 0,
            finishes: 0,
        }
    }
}
impl Attempt for Actor {
    fn prepare(&mut self) -> Result<bool> {
        if matches!(self.fail, Fail::Prepare) {
            Err(error())
        } else {
            Ok(self.inactive)
        }
    }
    fn press_once(&mut self) -> KeyDispatch {
        self.presses += 1;
        let count = Cell::new(0);
        key_once::dispatch(
            || {
                count.set(count.get() + 1);
                if (count.get() == 1 && matches!(self.fail, Fail::InitialCheck))
                    || (count.get() == 2 && matches!(self.fail, Fail::FinalCheck))
                {
                    Err(error())
                } else {
                    Ok(())
                }
            },
            || {
                if matches!(self.fail, Fail::Allocate) {
                    Err(error())
                } else {
                    Ok(Pair {
                        fail: self.fail,
                        trace: self.trace.clone(),
                    })
                }
            },
            |_| {},
        )
    }
    fn finish(&mut self, sent: bool) -> Result<Value> {
        self.finishes += 1;
        assert_eq!(sent, !self.inactive);
        self.trace.borrow_mut().push("readback");
        if matches!(self.fail, Fail::Readback) {
            Err(error())
        } else {
            Ok(
                json!({"selection_inactive":true,"ordinary_key_presses":usize::from(sent),"same_attempt_readback":true,"complete_queue_unchanged":true}),
            )
        }
    }
}
#[test]
fn already_inactive_is_zero_input_even_with_broken_key_producer() {
    for fail in [Fail::None, Fail::Down, Fail::Allocate, Fail::Up] {
        let mut a = Actor::new(true, fail);
        assert!(matches!(dispatch(&mut a), StrictOutcome::Verified(_)));
        assert_eq!(a.presses, 0);
        assert_eq!(*a.trace.borrow(), vec!["readback"]);
    }
}
#[test]
fn ordinary_pair_once_then_same_attempt_proof() {
    let mut a = Actor::new(false, Fail::None);
    assert!(matches!(dispatch(&mut a), StrictOutcome::Verified(_)));
    assert_eq!(a.presses, 1);
    assert_eq!(*a.trace.borrow(), vec!["down", "up", "readback"]);
}
#[test]
fn possible_down_and_up_refusals_never_poll_or_repeat_escape() {
    for fail in [Fail::Down, Fail::Up] {
        let mut a = Actor::new(false, fail);
        assert!(matches!(dispatch(&mut a), StrictOutcome::Uncertain(_)));
        assert_eq!(a.presses, 1);
        assert_eq!(a.finishes, 0);
        assert_eq!(*a.trace.borrow(), vec!["down", "up"]);
    }
}
#[test]
fn producer_positive_pre_down_refusals_are_not_unknown() {
    for fail in [
        Fail::Prepare,
        Fail::InitialCheck,
        Fail::Allocate,
        Fail::FinalCheck,
    ] {
        let mut a = Actor::new(false, fail);
        assert!(matches!(dispatch(&mut a), StrictOutcome::NotQueued(_)));
        assert!(a.trace.borrow().is_empty());
        assert_eq!(a.finishes, 0);
    }
}
#[test]
fn changed_final_readback_is_uncertain_only_after_possible_key() {
    for inactive in [false, true] {
        let mut a = Actor::new(inactive, Fail::Readback);
        let outcome = dispatch(&mut a);
        assert_eq!(matches!(outcome, StrictOutcome::NotQueued(_)), inactive);
        assert_eq!(matches!(outcome, StrictOutcome::Uncertain(_)), !inactive);
        assert_eq!(a.presses, usize::from(!inactive));
    }
}
#[test]
fn retained_guard_runs_after_observation_and_preserves_original_refusal() {
    let calls = Cell::new(0);
    let r = retained_read(
        || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err(error())
            } else {
                Ok(())
            }
        },
        || Ok(9),
    );
    assert!(r.is_err());
    assert_eq!(calls.get(), 2);
    let calls = Cell::new(0);
    let r = retained_read(
        || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                Err(error())
            } else {
                Ok(())
            }
        },
        || Err::<(), _>(Error::new("foreign_scope", "original")),
    );
    assert_eq!(r.unwrap_err().code, "foreign_scope");
    assert_eq!(calls.get(), 2);
}
#[test]
fn unchanged_pending_exhausts_original_deadline_without_renewal() {
    let start = Instant::now();
    let time = Cell::new(start);
    let reads = Cell::new(0);
    let waits = RefCell::new(vec![]);
    let r = finish_readback(
        start + Duration::from_millis(25),
        true,
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Ok(ClearSelectionReadback::Pending)
        },
        || panic!("no reached proof"),
        || time.get(),
        |d| {
            waits.borrow_mut().push(d);
            time.set(time.get() + d);
        },
    );
    assert!(r.is_err());
    assert_eq!(time.get(), start + Duration::from_millis(25));
    assert_eq!(
        *waits.borrow(),
        vec![
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(5)
        ]
    );
    assert_eq!(reads.get(), 3);
}
#[test]
fn clear_then_changed_after_platform_never_succeeds_or_repolls() {
    let reads = Cell::new(0);
    let platform = Cell::new(0);
    let start = Instant::now();
    let r = finish_readback(
        start + Duration::from_secs(1),
        true,
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Ok(if reads.get() == 1 {
                ClearSelectionReadback::Cleared
            } else {
                ClearSelectionReadback::Pending
            })
        },
        || {
            platform.set(platform.get() + 1);
            Ok(())
        },
        || start,
        |_| panic!("no retry after final changed proof"),
    );
    assert!(r.is_err());
    assert_eq!(reads.get(), 2);
    assert_eq!(platform.get(), 1);
}
#[test]
fn expired_start_and_late_clear_are_discarded() {
    let start = Instant::now();
    let reads = Cell::new(0);
    assert!(finish_readback(
        start,
        true,
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Ok(ClearSelectionReadback::Cleared)
        },
        || Ok(()),
        || start,
        |_| {}
    )
    .is_err());
    assert_eq!(reads.get(), 0);
    let time = Cell::new(start);
    assert!(finish_readback(
        start + Duration::from_millis(10),
        true,
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            time.set(start + Duration::from_millis(10));
            Ok(ClearSelectionReadback::Cleared)
        },
        || panic!("late proof"),
        || time.get(),
        |_| {}
    )
    .is_err());
    assert_eq!(reads.get(), 1);
}
#[test]
fn stop_or_platform_overrun_cannot_turn_clear_into_success() {
    let start = Instant::now();
    let stopped = Cell::new(false);
    let r = finish_readback(
        start + Duration::from_secs(1),
        true,
        || {
            if stopped.get() {
                Err(Error::new("session_closed", "stopped"))
            } else {
                Ok(())
            }
        },
        || Ok(ClearSelectionReadback::Cleared),
        || {
            stopped.set(true);
            Ok(())
        },
        || start,
        |_| {},
    );
    assert_eq!(r.unwrap_err().code, "session_closed");
    let now = Cell::new(start);
    let reads = Cell::new(0);
    assert!(finish_readback(
        start + Duration::from_millis(1),
        true,
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Ok(ClearSelectionReadback::Cleared)
        },
        || {
            now.set(start + Duration::from_millis(1));
            Ok(())
        },
        || now.get(),
        |_| {}
    )
    .is_err());
    assert_eq!(reads.get(), 1);
}
fn epoch() -> Epoch {
    Epoch {
        game: Identity {
            pid: 7,
            start_ticks: "1.000001".into(),
            exe: "/fixture".into(),
            uid: 501,
        },
        campaign: "nonce:playthrough".into(),
        player: "GER".into(),
    }
}
fn dir() -> std::path::PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "hoi4-cancel074-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn request() -> Request<'static> {
    Request {
        id: "clear-once",
        key: "arms_factory",
        state_id: 64,
    }
}
#[test]
fn actual_producer_refusal_has_durable_notqueued_without_quarantine() {
    for fail in [Fail::InitialCheck, Fail::Allocate, Fail::FinalCheck] {
        let d = dir();
        let p = d.join("actions");
        let mut j = GameJournal::open(&p).unwrap();
        let mut a = Actor::new(false, fail);
        assert!(j
            .execute_strict(
                request().id,
                &epoch(),
                request().action(),
                json!({"typed_queue":true}),
                &mut || dispatch(&mut a)
            )
            .is_err());
        assert!(!j.blocked());
        assert!(a.trace.borrow().is_empty());
        drop(j);
        let bytes = std::fs::read(&p).unwrap();
        let rows: Vec<Value> = String::from_utf8(bytes.clone())
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["version"], 2);
        assert_eq!(rows[1]["status"], "NotApplied");
        assert_eq!(
            rows[1]["detail"],
            json!({"delivery":"NotQueued","cause_code":"torn_read"})
        );
        let j = GameJournal::open(&p).unwrap();
        assert_eq!(
            j.strict_receipt(request().id, &epoch(), &request().action())
                .unwrap_err()
                .code,
            "input_not_queued"
        );
        assert_eq!(std::fs::read(&p).unwrap(), bytes);
        drop(j);
        std::fs::remove_dir_all(d).unwrap();
    }
}
#[test]
fn verified_replay_is_historical_with_no_read_or_new_append() {
    let d = dir();
    let p = d.join("actions");
    let mut j = GameJournal::open(&p).unwrap();
    let mut a = Actor::new(false, Fail::None);
    let first = j
        .execute_strict(
            request().id,
            &epoch(),
            request().action(),
            json!({"typed_queue":true}),
            &mut || dispatch(&mut a),
        )
        .unwrap();
    assert_eq!(first.status, Status::Verified);
    let bytes = std::fs::read(&p).unwrap();
    drop(j);
    let j = GameJournal::open(&p).unwrap();
    let receipt = j
        .strict_receipt(request().id, &epoch(), &request().action())
        .unwrap()
        .unwrap();
    assert_eq!(receipt, first);
    let replay = historical_receipt(&request(), &epoch(), &j)
        .unwrap()
        .unwrap();
    assert_eq!(replay["receipt"], serde_json::to_value(&first).unwrap());
    assert_eq!(replay["historical_receipt"], true);
    assert_eq!(replay["current_selection_verified"], false);
    assert_eq!(a.presses, 1);
    assert_eq!(a.finishes, 1);
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn unknown_keeps_intent_and_reopen_never_settles_or_retries() {
    for fail in [Fail::Down, Fail::Up, Fail::Readback] {
        let d = dir();
        let p = d.join("actions");
        let mut j = GameJournal::open(&p).unwrap();
        let mut a = Actor::new(false, fail);
        assert_eq!(
            j.execute_strict(
                request().id,
                &epoch(),
                request().action(),
                json!({"typed_queue":true}),
                &mut || dispatch(&mut a)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert!(j.blocked());
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(String::from_utf8(bytes.clone()).unwrap().lines().count(), 1);
        drop(j);
        let mut j = GameJournal::open(&p).unwrap();
        assert!(j.blocked());
        assert_eq!(
            historical_receipt(&request(), &epoch(), &j)
                .unwrap_err()
                .code,
            "action_uncertain"
        );
        assert_eq!(
            j.execute_strict(
                request().id,
                &epoch(),
                request().action(),
                json!({}),
                &mut || panic!("no later settlement/dispatch")
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert_eq!(std::fs::read(&p).unwrap(), bytes);
        assert_eq!(a.presses, 1);
        drop(j);
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[test]
fn cancellation_permission_is_independent_default_off_and_strictly_boolean() {
    use super::super::Config;
    let minimal: Config = serde_json::from_value(json!({"controller":"fixture","player":"GER","construction_panel":true,"construction_selection":true,"construction_placement":true})).unwrap();
    assert!(!minimal.construction_cancellation);
    assert!(minimal.cancellation_deadline_epoch.is_none());
    for value in [json!(1), json!("true"), Value::Null] {
        assert!(serde_json::from_value::<Config>(
            json!({"controller":"fixture","player":"GER","construction_cancellation":value})
        )
        .is_err());
    }
    let enabled: Config = serde_json::from_value(json!({"controller":"fixture","player":"GER","construction_cancellation":true,"cancellation_deadline_epoch":1000.})).unwrap();
    assert!(enabled.construction_cancellation);
    assert!(
        !enabled.construction_placement
            && !enabled.construction_navigation
            && !enabled.construction_selection
            && !enabled.construction_panel
    );
    assert_eq!(
        super::super::qualification_remaining(
            enabled.construction_cancellation,
            enabled.cancellation_deadline_epoch,
            999.
        )
        .unwrap(),
        Some(Duration::from_secs(1))
    );
    assert!(super::super::qualification_remaining(true, None, 999.).is_err());
}
