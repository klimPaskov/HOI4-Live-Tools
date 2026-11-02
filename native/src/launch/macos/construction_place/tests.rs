use super::*;
use crate::launch::macos::journal::Status;
use std::{
    cell::Cell,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
pub(super) mod fixture;
use fixture::*;

#[derive(Clone, Copy)]
enum Effect {
    Increment,
    Append,
    Pending,
    WrongRow,
}
struct Fake {
    memory: Fixture,
    anchor: PlacementAnchor,
    time: Instant,
    until: Instant,
    effect: Effect,
    fail: u8,
    clicks: usize,
    readbacks: usize,
    idle: usize,
}
impl Fake {
    fn new(effect: Effect, fail: u8) -> Self {
        let memory = memory();
        let anchor = anchor(&memory);
        let time = Instant::now();
        Self {
            memory,
            anchor,
            time,
            until: time + Duration::from_millis(50),
            effect,
            fail,
            clicks: 0,
            readbacks: 0,
            idle: 0,
        }
    }
}
impl Attempt for Fake {
    fn prepare(&mut self) -> Result<()> {
        require_pending(self.anchor.observe(&self.memory)?.delta())?;
        if self.fail == 1 {
            point::require_map_route(None)?;
        }
        Ok(())
    }
    fn click_once(&mut self) -> Result<()> {
        self.clicks += 1;
        if [2, 3].contains(&self.fail) {
            return Err(Error::new(
                "transport_refused",
                "fixture possible motion/down",
            ));
        }
        match self.effect {
            Effect::Increment => self.memory.int(LINE + 0x48, 3),
            Effect::Append => self.memory.array(PRODUCTION + 0x70, &[LINE, SECOND, NEW]),
            Effect::WrongRow => self.memory.int(SECOND + 0x48, 5),
            Effect::Pending => (),
        }
        Ok(())
    }
    fn readback(&mut self) -> Result<PlacementDelta> {
        self.readbacks += 1;
        let value = self.anchor.observe(&self.memory)?.delta();
        if self.fail == 5 {
            self.time = self.until;
        }
        Ok(value)
    }
    fn platform_idle(&mut self) -> Result<()> {
        self.idle += 1;
        if self.fail == 4 {
            self.memory.int(LINE + 0x48, 2);
        }
        Ok(())
    }
    fn boundary(&mut self) -> Result<()> {
        if self.fail == 6 && self.clicks > 0 {
            return Err(Error::new("session_closed", "fixture cancellation"));
        }
        if self.fail == 7 {
            return Err(Error::new("input_lease_required", "fixture revoked lease"));
        }
        Ok(())
    }
    fn until(&self) -> Instant {
        self.until
    }
    fn now(&self) -> Instant {
        self.time
    }
    fn wait(&mut self, d: Duration) {
        self.time += d;
    }
}
fn directory() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "hoi4-placement-037-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn request() -> Request<'static> {
    Request {
        id: "place-once",
        key: "arms_factory",
        state_id: 1,
    }
}
fn epoch(f: &Fixture) -> Epoch {
    Epoch {
        game: f.game(),
        campaign: "fixture:playthrough".into(),
        player: "GER".into(),
    }
}
#[test]
fn real_typed_increment_and_append_are_the_only_one_click_successes() {
    for effect in [Effect::Increment, Effect::Append] {
        let mut a = Fake::new(effect, 0);
        let result = dispatch(&mut a);
        let StrictOutcome::Verified(v) = result else {
            panic!("expected typed placement success")
        };
        assert_eq!(a.clicks, 1);
        assert_eq!(a.readbacks, 2);
        assert_eq!(a.idle, 1);
        assert_eq!(v["amount_added"], 1);
        assert_eq!(v["ordinary_clicks"], 1);
        assert_eq!(
            v["delta"],
            match effect {
                Effect::Increment => "existing_line_incremented",
                _ => "new_line_added",
            }
        );
        assert_eq!(v["full_picker_verified"], false);
        assert_eq!(v["current_event_verified"], false);
    }
}
#[test]
fn pre_input_route_or_lease_refusal_never_enters_transport() {
    for fail in [1, 7] {
        let mut a = Fake::new(Effect::Increment, fail);
        assert!(matches!(dispatch(&mut a), StrictOutcome::NotQueued(_)));
        assert_eq!((a.clicks, a.readbacks, a.idle), (0, 0, 0));
    }
}
#[test]
fn a_queue_delta_already_present_before_transport_is_not_credited() {
    let mut a = Fake::new(Effect::Increment, 0);
    a.memory.int(LINE + 0x48, 3);
    assert!(matches!(dispatch(&mut a), StrictOutcome::NotQueued(_)));
    assert_eq!(a.clicks, 0);
}
#[test]
fn possible_move_or_down_error_never_retries_or_reads_for_success() {
    for fail in [2, 3] {
        let mut a = Fake::new(Effect::Increment, fail);
        assert!(matches!(dispatch(&mut a), StrictOutcome::Uncertain(_)));
        assert_eq!(a.clicks, 1);
        assert_eq!(a.readbacks, 0);
    }
}
#[test]
fn unchanged_complete_queue_is_pending_until_the_original_deadline() {
    let mut a = Fake::new(Effect::Pending, 0);
    let until = a.until;
    let StrictOutcome::Uncertain(e) = dispatch(&mut a) else {
        panic!("pending is not success")
    };
    assert_eq!(e.code, "action_deadline");
    assert_eq!(a.clicks, 1);
    assert_eq!(a.until, until);
    assert_eq!(a.time, until);
    assert_eq!(a.idle, 0);
    assert_eq!(a.readbacks, 5);
}
#[test]
fn changed_other_row_and_post_platform_delta_change_are_uncertain() {
    for (effect, fail) in [(Effect::WrongRow, 0), (Effect::Increment, 4)] {
        let mut a = Fake::new(effect, fail);
        let StrictOutcome::Uncertain(e) = dispatch(&mut a) else {
            panic!("changed queue must refuse")
        };
        assert_eq!(e.code, "construction_placement_changed");
        assert_eq!(a.clicks, 1);
    }
}
#[test]
fn late_typed_success_and_post_click_stop_are_discarded() {
    for (fail, code) in [(5, "action_deadline"), (6, "session_closed")] {
        let mut a = Fake::new(Effect::Increment, fail);
        let StrictOutcome::Uncertain(e) = dispatch(&mut a) else {
            panic!("late/cancelled result must refuse")
        };
        assert_eq!(e.code, code);
        assert_eq!(a.clicks, 1);
        assert_eq!(a.idle, 0);
    }
}
#[test]
fn expired_start_never_prepares_or_posts() {
    let mut a = Fake::new(Effect::Increment, 0);
    a.time = a.until;
    assert!(matches!(dispatch(&mut a), StrictOutcome::NotQueued(_)));
    assert_eq!(a.clicks, 0);
}
#[test]
fn real_journal_intent_precedes_transport_and_verified_replay_is_historical() {
    let d = directory();
    let path = d.join("game-actions.jsonl");
    let mut j = GameJournal::open(&path).unwrap();
    let mut a = Fake::new(Effect::Increment, 0);
    let e = epoch(&a.memory);
    let baseline = a.anchor.private_baseline().unwrap();
    let r = j
        .execute_strict(request().id, &e, request().action(), baseline, &mut || {
            let rows: Vec<Value> = std::fs::read_to_string(&path)
                .unwrap()
                .lines()
                .map(|v| serde_json::from_str(v).unwrap())
                .collect();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["status"], "Dispatching");
            assert_eq!(a.clicks, 0);
            dispatch(&mut a)
        })
        .unwrap();
    assert_eq!(r.status, Status::Verified);
    assert_eq!(a.clicks, 1);
    drop(j);
    let bytes = std::fs::read(&path).unwrap();
    let mut j = GameJournal::open(&path).unwrap();
    assert_eq!(
        j.strict_receipt(request().id, &e, &request().action())
            .unwrap(),
        Some(r)
    );
    assert!(j
        .execute_strict(
            request().id,
            &e,
            request().action(),
            json!({}),
            &mut || panic!("no replay transport/readback")
        )
        .is_ok());
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn possible_input_failures_quarantine_after_reopen_and_block_every_new_id() {
    for (effect, fail) in [
        (Effect::Increment, 2),
        (Effect::Pending, 0),
        (Effect::WrongRow, 0),
    ] {
        let d = directory();
        let path = d.join("game-actions.jsonl");
        let mut j = GameJournal::open(&path).unwrap();
        let mut a = Fake::new(effect, fail);
        let e = epoch(&a.memory);
        let baseline = a.anchor.private_baseline().unwrap();
        assert!(j
            .execute_strict(request().id, &e, request().action(), baseline, &mut || {
                dispatch(&mut a)
            })
            .is_err());
        assert!(j.blocked());
        assert_eq!(a.clicks, 1);
        drop(j);
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(String::from_utf8(bytes.clone()).unwrap().lines().count(), 1);
        let mut j = GameJournal::open(&path).unwrap();
        for id in [request().id, "fresh-id"] {
            assert!(j
                .execute_strict(id, &e, request().action(), json!({}), &mut || panic!(
                    "no old settlement/new action"
                ))
                .is_err());
        }
        assert_eq!(bytes, std::fs::read(&path).unwrap());
        drop(j);
        std::fs::remove_dir_all(d).unwrap();
    }
}
#[test]
fn a_known_no_input_refusal_is_durable_and_cannot_be_replayed() {
    let d = directory();
    let path = d.join("game-actions.jsonl");
    let mut j = GameJournal::open(&path).unwrap();
    let mut a = Fake::new(Effect::Increment, 1);
    let e = epoch(&a.memory);
    let baseline = a.anchor.private_baseline().unwrap();
    assert!(j
        .execute_strict(request().id, &e, request().action(), baseline, &mut || {
            dispatch(&mut a)
        })
        .is_err());
    assert_eq!(a.clicks, 0);
    assert!(!j.blocked());
    drop(j);
    let bytes = std::fs::read(&path).unwrap();
    let rows: Vec<Value> = String::from_utf8(bytes.clone())
        .unwrap()
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["status"], "NotApplied");
    assert_eq!(
        rows[1]["detail"],
        json!({"delivery":"NotQueued","cause_code":"construction_map_route_unqualified"})
    );
    let j = GameJournal::open(&path).unwrap();
    assert!(!j.blocked());
    assert_eq!(
        j.strict_receipt(request().id, &e, &request().action())
            .unwrap_err()
            .code,
        "input_not_queued"
    );
    assert_eq!(bytes, std::fs::read(&path).unwrap());
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}
#[test]
fn cursor_wait_is_read_only_bounded_and_accepts_only_os_sdl_os_agreement() {
    let t = Instant::now();
    let time = Cell::new(t);
    let calls = Cell::new(0);
    let os = Cell::new(0);
    correlate(
        t + Duration::from_millis(10),
        || Ok(()),
        || {
            os.set(os.get() + 1);
            Ok(())
        },
        || {
            calls.set(calls.get() + 1);
            Ok(if calls.get() < 3 {
                [62, 10]
            } else {
                [960, 540]
            })
        },
        || time.get(),
        |d| time.set(time.get() + d),
    )
    .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(os.get(), 6);
    assert_eq!(time.get(), t + Duration::from_millis(4));
}
#[test]
fn cursor_pending_timeout_and_late_match_do_not_renew_the_window() {
    let t = Instant::now();
    let time = Cell::new(t);
    let calls = Cell::new(0);
    let until = t + Duration::from_millis(4);
    let e = correlate(
        until,
        || Ok(()),
        || Ok(()),
        || {
            calls.set(calls.get() + 1);
            Ok([62, 10])
        },
        || time.get(),
        |d| time.set(time.get() + d),
    )
    .unwrap_err();
    assert_eq!(e.code, "input_cursor_pending");
    assert_eq!(calls.get(), 2);
    assert_eq!(time.get(), until);
    time.set(t);
    let e = correlate(
        until,
        || Ok(()),
        || Ok(()),
        || {
            time.set(until);
            Ok([960, 540])
        },
        || time.get(),
        |_| panic!("no late wait"),
    )
    .unwrap_err();
    assert_eq!(e.code, "input_cursor_pending");
}
#[test]
fn foreign_source_and_os_cursor_refusal_are_never_polled_again() {
    let t = Instant::now();
    let calls = Cell::new(0);
    let e = correlate(
        t + Duration::from_secs(1),
        || Ok(()),
        || Ok(()),
        || {
            calls.set(calls.get() + 1);
            Err(Error::new("input_source_changed", "fixture"))
        },
        || t,
        |_| panic!("no foreign retry"),
    )
    .unwrap_err();
    assert_eq!(e.code, "input_source_changed");
    assert_eq!(calls.get(), 1);
    let e = correlate(
        t + Duration::from_secs(1),
        || Ok(()),
        || Err(Error::new("input_cursor_changed", "fixture")),
        || panic!("OS-before failed"),
        || t,
        |_| panic!(),
    )
    .unwrap_err();
    assert_eq!(e.code, "input_cursor_changed");
}
#[test]
fn cancellation_after_cursor_sample_discards_agreement_without_another_sample() {
    let t = Instant::now();
    let calls = Cell::new(0);
    let checks = Cell::new(0);
    let e = correlate(
        t + Duration::from_secs(1),
        || {
            checks.set(checks.get() + 1);
            if checks.get() == 2 {
                Err(Error::new(
                    "session_closed",
                    "fixture stop after source sample",
                ))
            } else {
                Ok(())
            }
        },
        || Ok(()),
        || {
            calls.set(calls.get() + 1);
            Ok([960, 540])
        },
        || t,
        |_| panic!("no wait after cancellation"),
    )
    .unwrap_err();
    assert_eq!(e.code, "session_closed");
    assert_eq!(calls.get(), 1);
}
#[test]
fn placement_config_defaults_off_and_proposed_schema_is_fixed_amount() {
    for v in [
        json!({"controller":"fixture","player":"GER"}),
        json!({"controller":"fixture","player":"GER","construction_selection":true}),
    ] {
        let c: super::super::Config = serde_json::from_value(v).unwrap();
        assert!(!c.construction_placement);
    }
    for value in [json!(null), json!(1), json!("true")] {
        assert!(serde_json::from_value::<super::super::Config>(
            json!({"controller":"fixture","player":"GER","construction_placement":value})
        )
        .is_err());
    }
    assert!(
        serde_json::from_value::<super::super::Config>(
            json!({"controller":"fixture","player":"GER","construction_placement":true})
        )
        .unwrap()
        .construction_placement
    );
    let schema = tool()["inputSchema"].clone();
    assert_eq!(schema["additionalProperties"], false);
    assert!(schema["properties"].get("amount").is_none());
    assert_eq!(request().action()["amount"], 1);
    assert_eq!(
        point::require_map_route(None).unwrap_err().code,
        "construction_map_route_unqualified"
    );
}
