use super::*;
use crate::{launch::macos::journal::Status, platform::Identity};
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
    Move,
    BeforeDown,
    Down,
    Up,
    Readback,
}
fn error() -> Error {
    Error::new("torn_read", "fixture refusal around possible movement")
}
struct Actor {
    ready: bool,
    fail: Fail,
    trace: Rc<RefCell<Vec<&'static str>>>,
    clicks: usize,
    finishes: usize,
}
impl Actor {
    fn new(ready: bool, fail: Fail) -> Self {
        Self {
            ready,
            fail,
            trace: Rc::new(RefCell::new(vec![])),
            clicks: 0,
            finishes: 0,
        }
    }
}
impl Attempt for Actor {
    fn prepare(&mut self) -> Result<bool> {
        if matches!(self.fail, Fail::Prepare) {
            Err(error())
        } else {
            Ok(self.ready)
        }
    }
    fn click_once(&mut self) -> Result<()> {
        self.clicks += 1;
        self.trace.borrow_mut().push("move");
        if matches!(self.fail, Fail::Move | Fail::BeforeDown) {
            return Err(error());
        }
        self.trace.borrow_mut().push("down");
        // This injected click result models the shared producer's unconditional release.
        self.trace.borrow_mut().push("up");
        if matches!(self.fail, Fail::Down | Fail::Up) {
            return Err(error());
        }
        Ok(())
    }
    fn finish(&mut self, sent: bool) -> Result<Value> {
        self.finishes += 1;
        assert_eq!(sent, !self.ready);
        self.trace.borrow_mut().push("readback");
        if matches!(self.fail, Fail::Readback) {
            Err(error())
        } else {
            Ok(
                json!({"ordinary_clicks":usize::from(sent),"same_attempt_readback":true,"complete_queue_unchanged":true}),
            )
        }
    }
}
#[test]
fn already_ready_is_zero_input_even_with_broken_click_transport() {
    for fail in [Fail::None, Fail::Down, Fail::Move, Fail::Up] {
        let mut a = Actor::new(true, fail);
        assert!(matches!(dispatch(&mut a), StrictOutcome::Verified(_)));
        assert_eq!(a.clicks, 0);
        assert_eq!(*a.trace.borrow(), vec!["readback"]);
    }
}
#[test]
fn one_move_down_up_then_same_attempt_proof() {
    let mut a = Actor::new(false, Fail::None);
    assert!(matches!(dispatch(&mut a), StrictOutcome::Verified(_)));
    assert_eq!(a.clicks, 1);
    assert_eq!(*a.trace.borrow(), vec!["move", "down", "up", "readback"]);
}
#[test]
fn possible_movement_or_down_refusal_never_polls_or_repeats() {
    for fail in [Fail::Move, Fail::BeforeDown, Fail::Down, Fail::Up] {
        let mut a = Actor::new(false, fail);
        assert!(matches!(dispatch(&mut a), StrictOutcome::Uncertain(_)));
        assert_eq!(a.clicks, 1);
        assert_eq!(a.finishes, 0);
        assert_eq!(a.trace.borrow().iter().filter(|&&x| x == "move").count(), 1);
        assert!(a.trace.borrow().iter().filter(|&&x| x == "down").count() <= 1);
        assert_eq!(
            a.trace.borrow().iter().filter(|&&x| x == "up").count(),
            usize::from(matches!(fail, Fail::Down | Fail::Up))
        );
    }
}
#[test]
fn read_only_prepare_refusal_is_notqueued_without_transport() {
    let mut a = Actor::new(false, Fail::Prepare);
    assert!(matches!(dispatch(&mut a), StrictOutcome::NotQueued(_)));
    assert_eq!(a.clicks, 0);
    assert_eq!(a.finishes, 0);
    assert!(a.trace.borrow().is_empty());
}
#[test]
fn changed_final_readback_is_uncertain_only_after_possible_movement() {
    for ready in [false, true] {
        let mut a = Actor::new(ready, Fail::Readback);
        let o = dispatch(&mut a);
        assert_eq!(matches!(o, StrictOutcome::NotQueued(_)), ready);
        assert_eq!(matches!(o, StrictOutcome::Uncertain(_)), !ready);
        assert_eq!(a.clicks, usize::from(!ready));
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
        "hoi4-find-click030-{}-{}",
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
fn read_only_preparation_refusal_is_durably_notqueued() {
    let fail = Fail::Prepare;
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
    assert_eq!(replay["current_find_verified"], false);
    assert_eq!(a.clicks, 1);
    assert_eq!(a.finishes, 1);
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn unknown_keeps_intent_and_reopen_never_settles_or_retries() {
    for fail in [
        Fail::Move,
        Fail::BeforeDown,
        Fail::Down,
        Fail::Up,
        Fail::Readback,
    ] {
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
        assert_eq!(a.clicks, 1);
        drop(j);
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[test]
fn pending_wait_uses_one_original_bound_and_no_click_loop() {
    let start = Instant::now();
    let time = Cell::new(start);
    let reads = Cell::new(0);
    let waits = RefCell::new(vec![]);
    let e = finish_readback(
        start + Duration::from_millis(25),
        true,
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Ok(None::<u64>)
        },
        || panic!("not reached"),
        |_| panic!("not reached"),
        || time.get(),
        |d| {
            waits.borrow_mut().push(d);
            time.set(time.get() + d)
        },
    )
    .unwrap_err();
    assert_eq!(e.code, "action_deadline");
    assert_eq!(reads.get(), 3);
    assert_eq!(*waits.borrow(), [10, 10, 5].map(Duration::from_millis));
}
#[test]
fn post_platform_read_uses_exact_first_proof_and_cannot_repoll_on_change() {
    let start = Instant::now();
    let reads = Cell::new(0);
    let platform = Cell::new(false);
    let final_reads = Cell::new(0);
    let e = finish_readback(
        start + Duration::from_secs(1),
        true,
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Ok(Some(73))
        },
        || {
            platform.set(true);
            Ok(())
        },
        |p| {
            assert!(platform.get());
            assert_eq!(*p, 73);
            final_reads.set(final_reads.get() + 1);
            Err(Error::new(
                "construction_state_changed",
                "original control replaced",
            ))
        },
        || start,
        |_| panic!("no retry"),
    )
    .unwrap_err();
    assert_eq!(e.code, "construction_state_changed");
    assert_eq!(reads.get(), 1);
    assert_eq!(final_reads.get(), 1);
}
#[test]
fn expired_start_and_late_ready_never_reach_platform_or_success() {
    let start = Instant::now();
    let time = Cell::new(start);
    let reads = Cell::new(0);
    for late in [false, true] {
        time.set(start);
        reads.set(0);
        let until = if late {
            start + Duration::from_millis(1)
        } else {
            start
        };
        let e = finish_readback(
            until,
            true,
            || Ok(()),
            || {
                reads.set(reads.get() + 1);
                time.set(until);
                Ok(Some(7))
            },
            || panic!("late"),
            |_| panic!("late"),
            || time.get(),
            |_| panic!("late"),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_deadline");
        assert_eq!(reads.get(), usize::from(late));
    }
}
#[test]
fn stop_or_platform_overrun_cannot_verify_or_renew_the_bound() {
    let start = Instant::now();
    let until = start + Duration::from_millis(1);
    let time = Cell::new(start);
    let e = finish_readback(
        until,
        true,
        || Err(Error::new("stopped", "cancel")),
        || panic!("stopped"),
        || panic!("stopped"),
        |_: &u64| panic!("stopped"),
        || time.get(),
        |_| panic!("stopped"),
    )
    .unwrap_err();
    assert_eq!(e.code, "stopped");
    let e = finish_readback(
        until,
        true,
        || Ok(()),
        || Ok(Some(7)),
        || {
            time.set(until);
            Ok(())
        },
        |_| panic!("late final"),
        || time.get(),
        |_| panic!("late"),
    )
    .unwrap_err();
    assert_eq!(e.code, "action_deadline");
}
#[test]
fn no_input_pending_refuses_without_waiting_or_click_retry() {
    let start = Instant::now();
    let e = finish_readback(
        start + Duration::from_secs(1),
        false,
        || Ok(()),
        || Ok(None::<u64>),
        || panic!("not reached"),
        |_| panic!("not reached"),
        || start,
        |_| panic!("zero input must not wait"),
    )
    .unwrap_err();
    assert_eq!(e.code, "construction_find_open_changed");
}
#[test]
fn old_f_transport_id_cannot_be_reused_for_a_click_action() {
    let d = dir();
    let p = d.join("actions");
    let mut j = GameJournal::open(&p).unwrap();
    let mut old = request().action();
    old["transport"] = json!("ordinary_f_once");
    j.execute_strict(request().id, &epoch(), old, json!({}), &mut || {
        StrictOutcome::Verified(json!({"historical":true}))
    })
    .unwrap();
    let bytes = std::fs::read(&p).unwrap();
    assert_eq!(
        historical_receipt(&request(), &epoch(), &j)
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
    assert_eq!(std::fs::read(&p).unwrap(), bytes);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn mouse_opener_permission_is_independent_default_off_and_strict_boolean() {
    use super::super::Config;
    let minimal: Config = serde_json::from_value(json!({"controller":"fixture","player":"GER","construction_navigation":true,"construction_cancellation":true})).unwrap();
    assert!(!minimal.construction_find_open);
    assert!(minimal.find_open_deadline_epoch.is_none());
    for value in [json!(1), json!("true"), Value::Null] {
        assert!(serde_json::from_value::<Config>(
            json!({"controller":"fixture","player":"GER","construction_find_open":value})
        )
        .is_err());
    }
    let enabled: Config = serde_json::from_value(json!({"controller":"fixture","player":"GER","construction_find_open":true,"find_open_deadline_epoch":1000.})).unwrap();
    assert!(enabled.construction_find_open);
    assert!(
        !enabled.construction_navigation
            && !enabled.construction_cancellation
            && !enabled.construction_panel
            && !enabled.construction_selection
            && !enabled.construction_placement
    );
    assert!(
        super::super::qualification_remaining(enabled.construction_find_open, None, 999.).is_err()
    );
    assert_eq!(
        super::super::qualification_remaining(
            enabled.construction_find_open,
            enabled.find_open_deadline_epoch,
            999.
        )
        .unwrap(),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        super::super::qualification_remaining(false, Some(1.), 999.).unwrap(),
        None
    );
}
#[test]
fn find_click_schema_keeps_exact_once_only_surface() {
    let t = tool();
    assert_eq!(t["name"], "hoi4_construction_find_open");
    let s = &t["inputSchema"];
    assert_eq!(s["additionalProperties"], false);
    assert_eq!(
        s["required"],
        json!(["building_id", "state_id", "request_id", "lease_token"])
    );
    assert_eq!(s["properties"].as_object().unwrap().len(), 4);
    assert_eq!(s["properties"]["state_id"]["maximum"], 4095);
    assert_eq!(request().action()["transport"], "ordinary_left_click");
}

fn pending_route() -> Error {
    Error::new(
        "construction_route_pending",
        "waiting for original move correlation",
    )
}
#[test]
fn post_move_route_wait_reads_until_positive_then_checks_again_after_platform() {
    let start = Instant::now();
    let time = Cell::new(start);
    let reads = Cell::new(0);
    let final_reads = Cell::new(0);
    let platform = Cell::new(0);
    let waits = RefCell::new(Vec::new());
    await_route(
        start + Duration::from_millis(50),
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            if reads.get() < 3 {
                Err(pending_route())
            } else {
                Ok(())
            }
        },
        || {
            platform.set(platform.get() + 1);
            Ok(())
        },
        || {
            final_reads.set(final_reads.get() + 1);
            Ok(())
        },
        || time.get(),
        |d| {
            waits.borrow_mut().push(d);
            time.set(time.get() + d)
        },
    )
    .unwrap();
    assert_eq!(reads.get(), 3);
    assert_eq!(platform.get(), 1);
    assert_eq!(final_reads.get(), 1);
    assert_eq!(*waits.borrow(), vec![Duration::from_millis(10); 2]);
    assert_eq!(time.get(), start + Duration::from_millis(20));
}
#[test]
fn route_wait_only_handles_explicit_pending_and_preserves_other_failures() {
    for code in [
        "construction_route_unavailable",
        "construction_state_changed",
        "torn_read",
        "input_busy",
    ] {
        let start = Instant::now();
        let result = await_route(
            start + Duration::from_secs(1),
            || Ok(()),
            || Err(Error::new(code, "original refusal")),
            || panic!("no platform"),
            || panic!("no final"),
            || start,
            |_| panic!("no wait on other refusal"),
        );
        assert_eq!(result.unwrap_err().code, code);
    }
}
#[test]
fn permanent_route_pending_exhausts_original_bound_without_repeating_input() {
    let start = Instant::now();
    let time = Cell::new(start);
    let reads = Cell::new(0);
    let waits = RefCell::new(Vec::new());
    let result = await_route(
        start + Duration::from_millis(25),
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Err(pending_route())
        },
        || panic!("no positive route"),
        || panic!("no final"),
        || time.get(),
        |d| {
            waits.borrow_mut().push(d);
            time.set(time.get() + d)
        },
    );
    assert_eq!(result.unwrap_err().code, "action_deadline");
    assert_eq!(reads.get(), 3);
    assert_eq!(
        *waits.borrow(),
        vec![
            Duration::from_millis(10),
            Duration::from_millis(10),
            Duration::from_millis(5)
        ]
    );
}
#[test]
fn route_wait_refuses_stop_and_late_positive_before_button_admission() {
    let start = Instant::now();
    let time = Cell::new(start);
    let checks = Cell::new(0);
    let reads = Cell::new(0);
    let result = await_route(
        start + Duration::from_millis(10),
        || {
            checks.set(checks.get() + 1);
            Ok(())
        },
        || {
            reads.set(reads.get() + 1);
            time.set(start + Duration::from_millis(10));
            Ok(())
        },
        || panic!("late positive discarded"),
        || panic!("no final"),
        || time.get(),
        |_| panic!("no wait"),
    );
    assert_eq!(result.unwrap_err().code, "action_deadline");
    assert_eq!(reads.get(), 1);
    let result = await_route(
        start + Duration::from_secs(1),
        || Err(Error::new("session_closed", "original stop")),
        || panic!("stop before observation"),
        || panic!(),
        || panic!(),
        || start,
        |_| panic!(),
    );
    assert_eq!(result.unwrap_err().code, "session_closed");
}
#[test]
fn positive_route_cannot_adopt_a_changed_final_route_or_platform_gap() {
    let start = Instant::now();
    let reads = Cell::new(0);
    let result = await_route(
        start + Duration::from_secs(1),
        || Ok(()),
        || {
            reads.set(reads.get() + 1);
            Ok(())
        },
        || Ok(()),
        || Err(pending_route()),
        || start,
        |_| panic!("no return to polling after final route changes"),
    );
    assert_eq!(result.unwrap_err().code, "construction_route_pending");
    assert_eq!(reads.get(), 1);
    let result = await_route(
        start + Duration::from_secs(1),
        || Ok(()),
        || Ok(()),
        || Err(Error::new("input_geometry_changed", "cursor moved")),
        || panic!("platform failure cannot become success"),
        || start,
        |_| panic!(),
    );
    assert_eq!(result.unwrap_err().code, "input_geometry_changed");
}
