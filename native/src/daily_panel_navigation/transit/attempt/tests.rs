use super::*;
use crate::{
    actions::Epoch,
    daily_governor::{EconomyTarget, Intent},
    game::ClockContext,
    research_stage::PanelBinding,
    research_ui::UiContext,
};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

const PLAY: &str = "11111111-1111-1111-1111-111111111111";
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "research-transit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        Self(path)
    }
    fn path(&self) -> PathBuf {
        self.0.join("transits.json")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn scope() -> Scope {
    Scope {
        epoch: Epoch {
            game: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/fixture/owned-game".into(),
                uid: 1000,
            },
            campaign: format!("fixture:{PLAY}"),
            player: "GER".into(),
        },
        anchors: [0x10000, 0x20000, 0x30000],
        own_country: 0x40000,
        own_status: 0x50000,
    }
}
fn request() -> Request {
    Request {
        id: "transit-01".into(),
        scope: scope(),
        entry: Entry {
            id: "retained-focus".into(),
            intent: Intent::Focus {
                focus_id: "GER_rhineland".into(),
            },
            navigation: None,
            focus_search: None,
        },
    }
}
fn snapshot(mode: i32, completed: bool) -> Snapshot {
    let s = scope();
    let binding = PanelBinding {
        anchors: [
            s.own_country,
            s.own_status,
            0x60000,
            0x70000,
            0x80000,
            0x90000,
        ],
        slots: vec![0xa0000, 0xb0000, 0xc0000, 0xd0000],
        roots: [0xe0000, 0xf0000, 0x100000, 0x110000],
        facilities_parent: 0x120000,
        selected_slot_pointer: 0,
        folder_index: 0,
        folder_windows: vec![],
        tab: 0,
    };
    let own = OwnState {
        epoch: s.epoch.clone(),
        selected_slot: None,
        selected_folder: None,
        tree_visible: false,
        panel_visible: Some(mode == 5),
        active_mode: Some(mode),
        researching: [
            "electronic_mechanical_engineering",
            "basic_machine_tools",
            "construction1",
            "support_weapons",
        ]
        .iter()
        .map(|v| if completed { None } else { Some((*v).into()) })
        .collect(),
        visible_info: vec![],
        panel_binding: Some(binding.clone()),
    };
    let ui = UiContext {
        campaign: ClockContext {
            playthrough_id: PLAY.into(),
            player: "GER".into(),
            anchors: s.anchors,
            date_raw: crate::calendar::encode(1936, 1, if completed { 2 } else { 1 }).unwrap(),
            difficulty: 2,
            historical_raw: 1,
            paused: false,
            speed: 5,
        },
        anchors: binding.anchors,
        slots: binding.slots,
    };
    Snapshot::from_observed(&s, own, &ui).unwrap()
}
struct Fake {
    path: PathBuf,
    now: Rc<Cell<Instant>>,
    calls: Rc<RefCell<Vec<&'static str>>>,
    reads: VecDeque<Result<Snapshot>>,
    initial: i32,
    delivery: Delivery,
    deadlines: Vec<Instant>,
    late_phase: Option<&'static str>,
    corrupt_at_idle: bool,
    fail_idle: bool,
}
impl Fake {
    fn new(temp: &Temp, now: Instant) -> Self {
        Self {
            path: temp.path(),
            now: Rc::new(Cell::new(now)),
            calls: Rc::new(RefCell::new(vec![])),
            reads: VecDeque::from([
                Ok(snapshot(23, false)),
                Ok(snapshot(5, true)),
                Ok(snapshot(5, true)),
            ]),
            initial: 23,
            delivery: Delivery::ServerConfirmed,
            deadlines: vec![],
            late_phase: None,
            corrupt_at_idle: false,
            fail_idle: false,
        }
    }
    fn event(&mut self, phase: &'static str, until: Instant) {
        self.calls.borrow_mut().push(phase);
        self.deadlines.push(until);
        if self.late_phase == Some(phase) {
            self.now.set(until);
        }
    }
}
impl Backend for Fake {
    fn begin(&mut self, _: &Scope, until: Instant) -> Result<Snapshot> {
        self.event("begin", until);
        Ok(snapshot(self.initial, false))
    }
    fn observe(&mut self, until: Instant) -> Result<Snapshot> {
        self.event("read", until);
        self.reads
            .pop_front()
            .expect("No unbounded extra observations")
    }
    fn preflight(&mut self, until: Instant) -> Result<()> {
        self.event("preflight", until);
        Ok(())
    }
    fn dispatch_once(&mut self, until: Instant) -> Delivery {
        self.event("W", until);
        let data: Data = serde_json::from_slice(&fs::read(&self.path).unwrap()).unwrap();
        assert!(
            matches!(data.records.last().unwrap().outcome, Outcome::Intent),
            "Intent must be durable before input"
        );
        self.delivery
    }
    fn confirm_input_idle(&mut self, until: Instant) -> Result<()> {
        self.event("idle", until);
        if self.corrupt_at_idle {
            fs::write(&self.path, b"external change").unwrap();
        }
        if self.fail_idle {
            return Err(Error::new(
                "fixture_idle_failure",
                "No current input-idle proof",
            ));
        }
        Ok(())
    }
    fn wait_readback(&mut self, delay: Duration) {
        self.calls.borrow_mut().push("wait");
        self.now.set(self.now.get() + delay);
    }
}
fn authority() -> InputAuthority {
    let mut a = InputAuthority::new("luna-player".into(), scope().epoch.clone()).unwrap();
    a.acquire("luna-player", TOKEN.into(), 30, &scope().epoch)
        .unwrap();
    a
}
pub(super) fn fixture_intent(path: &Path) -> (Journal, Identity) {
    let mut j = Journal::open(path).unwrap();
    j.persist(Record {
        request: request(),
        before: Facts::observed(&snapshot(23, false)),
        outcome: Outcome::Intent,
    })
    .unwrap();
    (j, scope().epoch.game)
}

#[test]
fn successful_terminal_write_that_finishes_late_grants_no_current_or_restart_admission() {
    let t = Temp::new();
    let start = Instant::now();
    let until = start + BUDGET;
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    // Clock crosses the original boundary immediately after the actual durable
    // terminal replacement. This is not a fictitious serialization-only test.
    let now = || {
        if fs::read(t.path())
            .ok()
            .and_then(|v| serde_json::from_slice::<Data>(&v).ok())
            .is_some_and(|d| {
                matches!(
                    d.records.first().map(|r| &r.outcome),
                    Some(Outcome::Verified { .. })
                )
            })
        {
            until
        } else {
            start
        }
    };
    assert_eq!(
        j.execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut b,
            until,
            &now
        )
        .unwrap_err()
        .code,
        "panel_transit_deadline"
    );
    assert!(matches!(
        j.data.records[0].outcome,
        Outcome::Verified { .. }
    ));
    assert!(j.admitted.is_empty());
    assert!(j.blocks_game(&scope().epoch.game));
    drop(j);
    assert!(Journal::open(&t.path())
        .unwrap()
        .blocks_game(&scope().epoch.game));
}

#[test]
fn concrete_read_error_is_preserved_as_cause_even_when_read_finishes_late() {
    let t = Temp::new();
    let start = Instant::now();
    let until = start + BUDGET;
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    b.reads = VecDeque::from([
        Ok(snapshot(23, false)),
        Err(Error::new(
            "fixture_concrete_read_error",
            "owned read failure",
        )),
    ]);
    let calls = Rc::clone(&b.calls);
    let now = || {
        if calls.borrow().iter().filter(|s| **s == "read").count() >= 2 {
            until
        } else {
            start
        }
    };
    let e = j
        .execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut b,
            until,
            &now,
        )
        .unwrap_err();
    assert_eq!(e.code, "action_uncertain");
    assert!(e.message.contains("fixture_concrete_read_error"));
    assert!(j.blocks_game(&scope().epoch.game));
}

#[test]
fn late_positive_final_read_cannot_become_current_or_terminal() {
    let t = Temp::new();
    let start = Instant::now();
    let until = start + BUDGET;
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    let calls = Rc::clone(&b.calls);
    let now = || {
        if calls.borrow().iter().filter(|s| **s == "read").count() >= 3 {
            until
        } else {
            start
        }
    };
    assert_eq!(
        j.execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut b,
            until,
            &now
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert!(matches!(j.data.records[0].outcome, Outcome::Intent));
    assert!(j.blocks_game(&scope().epoch.game));
}
fn run(
    journal: &mut Journal,
    req: Request,
    backend: &mut Fake,
    caller: Instant,
) -> Result<Execution> {
    let now = Rc::clone(&backend.now);
    journal.execute_with_clock(
        req,
        TOKEN,
        &authority(),
        &RwLock::new(true),
        backend,
        caller,
        &|| now.get(),
    )
}

#[test]
fn busy_slots_one_w_retains_goal_and_final_read_follows_idle() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    let Execution::Current { proof, delivery } =
        run(&mut j, request(), &mut b, start + Duration::from_secs(30)).unwrap()
    else {
        panic!()
    };
    assert_eq!(delivery, Some(Delivery::ServerConfirmed));
    assert_eq!(proof.goal().entry(), &request().entry);
    assert_eq!(proof.observed().research().researching, vec![None; 4]);
    assert_eq!(
        *b.calls.borrow(),
        ["begin", "preflight", "read", "W", "read", "idle", "read"]
    );
    assert!(b.deadlines.iter().all(|v| *v == start + BUDGET));
    assert!(!j.blocks_game(&scope().epoch.game));
}
#[test]
fn production_goal_is_retained_without_research_assignment() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    let mut r = request();
    r.entry.intent = Intent::Economy {
        target: EconomyTarget::Production {
            line_id: "0x0000000100000038".into(),
            equipment_id: "infantry_equipment_1".into(),
            total_factories: 14,
        },
    };
    let Execution::Current { proof, .. } = run(&mut j, r.clone(), &mut b, start + BUDGET).unwrap()
    else {
        panic!()
    };
    assert_eq!(proof.goal().entry(), &r.entry);
}
#[test]
fn already_shown_confirms_idle_then_fresh_original_scope_without_w() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    b.initial = 5;
    b.reads = VecDeque::from([
        Ok(snapshot(5, false)),
        Ok(snapshot(5, true)),
        Ok(snapshot(5, true)),
    ]);
    assert!(matches!(
        run(&mut j, request(), &mut b, start + BUDGET).unwrap(),
        Execution::Current { delivery: None, .. }
    ));
    assert!(!b.calls.borrow().contains(&"W"));
    drop(j);
    assert!(!Journal::open(&t.path())
        .unwrap()
        .blocks_game(&scope().epoch.game));
}
#[test]
fn historical_verified_never_reads_or_dispatches_or_reconstructs_current_proof() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    run(&mut j, request(), &mut b, start + BUDGET).unwrap();
    let bytes = fs::read(t.path()).unwrap();
    b.calls.borrow_mut().clear();
    assert!(matches!(
        run(&mut j, request(), &mut b, start).unwrap(),
        Execution::Historical(_)
    ));
    assert!(b.calls.borrow().is_empty());
    assert_eq!(bytes, fs::read(t.path()).unwrap());
    drop(j);
    let mut reopened = Journal::open(&t.path()).unwrap();
    assert!(reopened.blocks_game(&scope().epoch.game));
    assert!(matches!(
        run(&mut reopened, request(), &mut b, start).unwrap(),
        Execution::Historical(_)
    ));
    assert!(b.calls.borrow().is_empty());
}
#[test]
fn restarted_verified_input_quarantines_new_epoch_and_new_id_for_same_game() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    run(&mut j, request(), &mut b, start + BUDGET).unwrap();
    drop(j);
    let mut j = Journal::open(&t.path()).unwrap();
    let mut r = request();
    r.id = "other-id".into();
    r.scope.epoch.campaign = format!("new-controller:{PLAY}");
    b.calls.borrow_mut().clear();
    assert_eq!(
        run(&mut j, r, &mut b, start + BUDGET).unwrap_err().code,
        "action_uncertain"
    );
    assert!(b.calls.borrow().is_empty());
    let mut other = scope().epoch.game;
    other.start_ticks = "457".into();
    assert!(!j.blocks_game(&other));
}
#[test]
fn historical_id_cannot_change_retained_goal() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    run(&mut j, request(), &mut b, start + BUDGET).unwrap();
    let mut r = request();
    r.entry.id = "different".into();
    assert_eq!(
        run(&mut j, r, &mut b, start + BUDGET).unwrap_err().code,
        "request_id_conflict"
    );
}
#[test]
fn confirmed_not_queued_is_terminal_but_never_replayed_or_promoted() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    b.delivery = Delivery::NotQueued;
    assert_eq!(
        run(&mut j, request(), &mut b, start + BUDGET)
            .unwrap_err()
            .code,
        "input_not_queued"
    );
    assert!(!j.blocks_game(&scope().epoch.game));
    drop(j);
    let mut j = Journal::open(&t.path()).unwrap();
    b.calls.borrow_mut().clear();
    assert_eq!(
        run(&mut j, request(), &mut b, start + BUDGET)
            .unwrap_err()
            .code,
        "input_not_queued"
    );
    assert!(b.calls.borrow().is_empty());
}
#[test]
fn uncertain_input_keeps_intent_across_reopen_and_new_ids() {
    for delivery in [Delivery::Unknown, Delivery::ServerConfirmed] {
        let t = Temp::new();
        let start = Instant::now();
        let mut j = Journal::open(&t.path()).unwrap();
        let mut b = Fake::new(&t, start);
        b.delivery = delivery;
        b.reads = VecDeque::from([
            Ok(snapshot(23, false)),
            Err(Error::new("ownership_changed", "fixture")),
        ]);
        let e = run(&mut j, request(), &mut b, start + BUDGET).unwrap_err();
        assert_eq!(e.code, "action_uncertain");
        assert!(e.message.contains("ownership_changed"));
        assert!(j.blocks_game(&scope().epoch.game));
        drop(j);
        let mut j = Journal::open(&t.path()).unwrap();
        b.calls.borrow_mut().clear();
        assert_eq!(
            run(&mut j, request(), &mut b, start + BUDGET)
                .unwrap_err()
                .code,
            "action_uncertain"
        );
        let mut r = request();
        r.id = "new".into();
        assert_eq!(
            run(&mut j, r, &mut b, start + BUDGET).unwrap_err().code,
            "action_uncertain"
        );
        assert!(b.calls.borrow().is_empty());
    }
}
#[test]
fn original_deadline_applies_before_input_and_after_idle() {
    for phase in ["begin", "preflight", "read", "idle"] {
        let t = Temp::new();
        let start = Instant::now();
        let mut j = Journal::open(&t.path()).unwrap();
        let mut b = Fake::new(&t, start);
        b.late_phase = Some(phase);
        assert!(run(&mut j, request(), &mut b, start + Duration::from_secs(2)).is_err());
        assert!(b
            .deadlines
            .iter()
            .all(|d| *d == start + Duration::from_secs(2)));
        assert_eq!(j.blocks_game(&scope().epoch.game), phase == "idle");
    }
}
#[test]
fn final_post_idle_panel_change_cannot_verify_or_complete_original_goal() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    b.reads = VecDeque::from([
        Ok(snapshot(23, false)),
        Ok(snapshot(5, false)),
        Ok(snapshot(23, false)),
    ]);
    assert_eq!(
        run(&mut j, request(), &mut b, start + BUDGET)
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    assert!(j.blocks_game(&scope().epoch.game));
    assert!(matches!(j.data.records[0].outcome, Outcome::Intent));
}
#[test]
fn final_original_root_change_is_uncertain_without_retry() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    let mut changed = snapshot(5, false);
    changed.research.panel_binding.as_mut().unwrap().roots[0] += 8;
    b.reads = VecDeque::from([Ok(snapshot(23, false)), Ok(snapshot(5, false)), Ok(changed)]);
    assert_eq!(
        run(&mut j, request(), &mut b, start + BUDGET)
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    assert_eq!(b.calls.borrow().iter().filter(|v| **v == "W").count(), 1);
}
#[test]
fn input_idle_failure_preserves_intent_and_does_not_take_final_read() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    b.fail_idle = true;
    assert_eq!(
        run(&mut j, request(), &mut b, start + BUDGET)
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    assert_eq!(b.calls.borrow().last(), Some(&"idle"));
    assert!(j.blocks_game(&scope().epoch.game));
}
#[test]
fn terminal_storage_failure_never_returns_current_proof_and_poison_is_sticky() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    b.corrupt_at_idle = true;
    assert!(run(&mut j, request(), &mut b, start + BUDGET).is_err());
    assert!(j.blocks_game(&scope().epoch.game));
    assert!(j.poisoned);
    drop(j);
    assert!(Journal::open(&t.path()).is_err());
}
#[test]
fn read_only_transition_retry_uses_same_deadline_and_never_another_w() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    b.reads = VecDeque::from([
        Ok(snapshot(23, false)),
        Err(Error::new("research_panel_transition", "animation")),
        Err(Error::new("torn_read", "fixture")),
        Ok(snapshot(5, false)),
        Ok(snapshot(5, false)),
    ]);
    assert!(matches!(
        run(&mut j, request(), &mut b, start + BUDGET).unwrap(),
        Execution::Current { .. }
    ));
    assert_eq!(b.calls.borrow().iter().filter(|v| **v == "W").count(), 1);
    assert!(b.deadlines.iter().all(|d| *d == start + BUDGET));
}
#[test]
fn expired_caller_and_closed_gate_send_no_input_or_intent() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    let mut b = Fake::new(&t, start);
    assert!(run(&mut j, request(), &mut b, start).is_err());
    assert!(b.calls.borrow().is_empty());
    assert!(j
        .execute(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(false),
            &mut b,
            start + BUDGET
        )
        .is_err());
    assert!(j.data.records.is_empty());
}
#[test]
fn format_kind_version_unknown_fields_and_duplicate_ids_are_rejected() {
    for mutation in [0, 1, 2, 3] {
        let t = Temp::new();
        let start = Instant::now();
        let mut j = Journal::open(&t.path()).unwrap();
        let mut b = Fake::new(&t, start);
        b.delivery = Delivery::NotQueued;
        let _ = run(&mut j, request(), &mut b, start + BUDGET);
        drop(j);
        let mut data: serde_json::Value =
            serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
        match mutation {
            0 => data["kind"] = "research_actions".into(),
            1 => data["version"] = 2.into(),
            2 => data["current_proof"] = true.into(),
            _ => {
                let record = data["records"][0].clone();
                data["records"].as_array_mut().unwrap().push(record);
            }
        }
        fs::write(t.path(), serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(Journal::open(&t.path()).is_err());
    }
}
#[test]
fn capacity_refuses_before_any_new_snapshot_or_input() {
    let t = Temp::new();
    let start = Instant::now();
    let mut j = Journal::open(&t.path()).unwrap();
    for i in 0..MAX_RECORDS {
        let mut r = request();
        r.id = format!("refused-{i}");
        j.persist(Record {
            request: r,
            before: Facts::observed(&snapshot(23, false)),
            outcome: Outcome::NotQueued,
        })
        .unwrap();
    }
    let mut b = Fake::new(&t, start);
    b.delivery = Delivery::NotQueued;
    assert!(run(&mut j, request(), &mut b, start + BUDGET).is_err());
    assert!(!b.calls.borrow().contains(&"W"));
}
#[test]
fn journal_has_one_writer_and_external_change_blocks_other_domains() {
    let t = Temp::new();
    let j = Journal::open(&t.path()).unwrap();
    assert!(Journal::open(&t.path()).is_err());
    fs::write(t.path(), b"replaced").unwrap();
    assert!(j.blocks_game(&scope().epoch.game));
}

fn failure_diagnostic(error: &Error) -> serde_json::Value {
    assert_eq!(error.code, "action_uncertain");
    serde_json::from_str(error.message.split_once("; diagnostics=").unwrap().1).unwrap()
}

#[cfg(target_os = "linux")]
pub(super) fn readback_failure() -> Error {
    let temp = Temp::new();
    let start = Instant::now();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let mut backend = Fake::new(&temp, start);
    backend.reads = VecDeque::from([Ok(snapshot(23, false))]);
    backend.reads.extend((0..200).map(|_| {
        Err(Error::new(
        "research_panel_transition",
        "Closing root is in a known animation transition; transitions=[0, 1]; root=research_panel",
    ))
    }));
    let clock = backend.now.clone();
    journal
        .execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut backend,
            start + BUDGET,
            &|| clock.get(),
        )
        .unwrap_err()
}
#[test]
fn dispatch_expiry_reports_phase_without_reading_after_deadline_or_clearing_intent() {
    let temp = Temp::new();
    let start = Instant::now();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let mut backend = Fake::new(&temp, start);
    backend.late_phase = Some("W");
    let clock = backend.now.clone();
    let error = journal
        .execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut backend,
            start + BUDGET,
            &|| clock.get(),
        )
        .unwrap_err();
    let diagnostic = failure_diagnostic(&error);
    assert_eq!(diagnostic["historical_diagnostics_only"], true);
    assert_eq!(diagnostic["dispatch_ms"], 5000);
    assert_eq!(diagnostic["readback_started"], 0);
    assert_eq!(diagnostic["readback_ms"], 0);
    assert_eq!(diagnostic["last_pending_idle_ms"], serde_json::Value::Null);
    assert_eq!(diagnostic["last_transition_ms"], serde_json::Value::Null);
    assert_eq!(
        diagnostic["research_panel_pairs"],
        serde_json::json!([0, 0, 0, 0, 0, 0, 0, 0])
    );
    assert_eq!(diagnostic["research_panel_pair_changes"], 0);
    assert_eq!(
        backend
            .calls
            .borrow()
            .iter()
            .filter(|v| **v == "read")
            .count(),
        1
    );
    assert_eq!(
        backend.calls.borrow().iter().filter(|v| **v == "W").count(),
        1
    );
    assert!(journal.blocks_game(&scope().epoch.game));
    assert!(matches!(journal.data.records[0].outcome, Outcome::Intent));
}
#[test]
fn retryable_sample_diagnostics_are_bounded_codes_without_error_text_or_current_state() {
    let temp = Temp::new();
    let start = Instant::now();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let mut backend = Fake::new(&temp, start);
    backend.reads = VecDeque::from([Ok(snapshot(23, false))]);
    for i in 0..200 {
        backend.reads.push_back(Err(Error::new(
            if i % 2 == 0 {
                "research_panel_transition"
            } else {
                "torn_read"
            },
            "private raw pointer 0xdeadbeef\nnot retained",
        )));
    }
    let clock = backend.now.clone();
    let error = journal
        .execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut backend,
            start + BUDGET,
            &|| clock.get(),
        )
        .unwrap_err();
    let diagnostic = failure_diagnostic(&error);
    assert_eq!(diagnostic["readback_started"], 200);
    assert_eq!(diagnostic["transition_errors"], 100);
    assert_eq!(diagnostic["torn_errors"], 100);
    assert_eq!(diagnostic["pending_idle"], 0);
    assert_eq!(diagnostic["shown_research"], 0);
    assert_eq!(diagnostic["last_retryable_code"], "torn_read");
    assert_eq!(diagnostic["dispatch_ms"], 0);
    assert_eq!(diagnostic["readback_ms"], 5000);
    assert!(!error.message.contains("deadbeef") && !error.message.contains("not retained"));
    assert!(error.message.len() < 768 && !error.message.contains('\n'));
    assert!(journal.blocks_game(&scope().epoch.game));
    assert_eq!(
        backend.calls.borrow().iter().filter(|v| **v == "W").count(),
        1
    );
}
#[test]
fn pending_idle_diagnostics_never_claim_research_or_add_an_input_retry() {
    let temp = Temp::new();
    let start = Instant::now();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let mut backend = Fake::new(&temp, start);
    backend.reads = (0..201).map(|_| Ok(snapshot(23, false))).collect();
    let clock = backend.now.clone();
    let error = journal
        .execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut backend,
            start + BUDGET,
            &|| clock.get(),
        )
        .unwrap_err();
    let diagnostic = failure_diagnostic(&error);
    assert_eq!(diagnostic["readback_started"], 200);
    assert_eq!(diagnostic["pending_idle"], 200);
    assert_eq!(diagnostic["shown_research"], 0);
    assert_eq!(diagnostic["transition_errors"], 0);
    assert_eq!(diagnostic["last_retryable_code"], serde_json::Value::Null);
    assert!(journal.blocks_game(&scope().epoch.game));
    assert_eq!(
        backend.calls.borrow().iter().filter(|v| **v == "W").count(),
        1
    );
}

#[test]
fn transition_origin_accepts_only_complete_known_internal_messages() {
    let prefix = "Closing root is in a known animation transition; transitions=";
    for (origin, name) in TRANSITION_ORIGINS[..5].iter().enumerate() {
        for a in 0..=2 {
            for b in 0..=2 {
                if a == 0 && b == 0 {
                    continue;
                }
                assert_eq!(
                    transition_origin(&format!("{prefix}[{a}, {b}]; root={name}")),
                    (origin, Some([a, b]))
                );
            }
        }
    }
    assert_eq!(
        transition_origin("Research transit mode/literal visibility not settled"),
        (5, None)
    );
    for text in [
        "[0, 0]; root=research_panel",
        "[0, 3]; root=research_panel",
        "[-1, 1]; root=research_panel",
        "[0,1]; root=research_panel",
        "[0, 1]",
        "[0, 1]; root=unrecognized",
        "[0, 1]; root=research_info; root=research_panel",
        "[0, 1]; root=research_panel\n",
        "[0, 1]; root=research_panel; pointer=0xdeadbeef",
    ] {
        assert_eq!(transition_origin(&format!("{prefix}{text}")), (6, None));
    }
    for text in [
        "Research mode and literal panel visibility have not settled".into(),
        "Research transit mode/literal visibility not settled; pointer=0xdeadbeef".into(),
        "x".repeat(129),
    ] {
        assert_eq!(transition_origin(&text), (6, None));
    }
}

#[test]
fn transition_origins_and_first_return_times_preserve_uncertainty_and_one_dispatch() {
    let temp = Temp::new();
    let start = Instant::now();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let mut backend = Fake::new(&temp, start);
    backend.reads = VecDeque::from([Ok(snapshot(23, false))]);
    for i in 0..200 {
        backend.reads.push_back(if i < 10 {
            Ok(snapshot(23, false))
        } else {
            Err(Error::new("research_panel_transition", match i {
                10 => "Closing root is in a known animation transition; transitions=[0, 1]; root=research_panel",
                11 => "Research transit mode/literal visibility not settled",
                12 => "Closing root is in a known animation transition; transitions=[2, 1]; root=research_info",
                _ => "private raw pointer 0xdeadbeef\nnever retain",
            }))
        });
    }
    let clock = backend.now.clone();
    let error = journal
        .execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut backend,
            start + BUDGET,
            &|| clock.get(),
        )
        .unwrap_err();
    let diagnostic = failure_diagnostic(&error);
    assert_eq!(diagnostic["readback_started"], 200);
    assert_eq!(diagnostic["pending_idle"], 10);
    assert_eq!(diagnostic["transition_errors"], 190);
    assert_eq!(diagnostic["transition_origins"]["research_panel"], 1);
    assert_eq!(diagnostic["transition_origins"]["mode_visibility"], 1);
    assert_eq!(diagnostic["transition_origins"]["research_info"], 1);
    assert_eq!(diagnostic["transition_origins"]["unknown"], 187);
    assert_eq!(
        diagnostic["last_transition"],
        serde_json::json!({"origin":"unknown","pair":null})
    );
    assert_eq!(diagnostic["first_pending_idle_ms"], 0);
    assert_eq!(diagnostic["first_transition_ms"], 250);
    assert_eq!(diagnostic["last_pending_idle_ms"], 225);
    assert_eq!(diagnostic["last_transition_ms"], 4975);
    assert_eq!(
        diagnostic["research_panel_pairs"],
        serde_json::json!([1, 0, 0, 0, 0, 0, 0, 0])
    );
    assert_eq!(diagnostic["research_panel_pair_changes"], 0);
    assert_eq!(diagnostic["shown_research"], 0);
    assert!(!error.message.contains("deadbeef") && !error.message.contains('\n'));
    assert!(error.message.len() < 1024);
    assert!(matches!(journal.data.records[0].outcome, Outcome::Intent));
    assert!(journal.blocks_game(&scope().epoch.game));
    assert_eq!(
        backend.calls.borrow().iter().filter(|v| **v == "W").count(),
        1
    );
    assert!(!backend.calls.borrow().contains(&"idle"));
}

#[test]
fn pair_history_distinguishes_interleaved_returns_without_retry_or_settlement() {
    let temp = Temp::new();
    let start = Instant::now();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let mut backend = Fake::new(&temp, start);
    backend.reads = VecDeque::from([Ok(snapshot(23, false))]);
    for i in 0..200 {
        backend.reads.push_back(match i {
            0 | 3 => Ok(snapshot(23, false)),
            5 => Err(Error::new("torn_read", "not a root sample")),
            _ => Err(Error::new("research_panel_transition", match i {
                2 => "Closing root is in a known animation transition; transitions=[2, 1]; root=research_tree",
                6 => "Closing root is in a known animation transition; transitions=[0, 2]; root=research_panel",
                7 => "Closing root is in a known animation transition; transitions=[1, 0]; root=research_panel",
                8 => "Closing root is in a known animation transition; transitions=[0, 0]; root=research_panel; pointer=0xdeadbeef",
                _ => "Closing root is in a known animation transition; transitions=[0, 1]; root=research_panel",
            })),
        });
    }
    let clock = backend.now.clone();
    let error = journal
        .execute_with_clock(
            request(),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &mut backend,
            start + BUDGET,
            &|| clock.get(),
        )
        .unwrap_err();
    let diagnostic = failure_diagnostic(&error);
    assert_eq!(
        diagnostic["research_panel_pairs"],
        serde_json::json!([193, 1, 1, 0, 0, 0, 0, 0])
    );
    assert_eq!(diagnostic["research_panel_pair_changes"], 3);
    assert_eq!(diagnostic["pending_idle"], 2);
    assert_eq!(diagnostic["first_pending_idle_ms"], 0);
    assert_eq!(diagnostic["last_pending_idle_ms"], 75);
    assert_eq!(diagnostic["first_transition_ms"], 25);
    assert_eq!(diagnostic["last_transition_ms"], 4975);
    assert_eq!(diagnostic["transition_origins"]["research_tree"], 1);
    assert_eq!(diagnostic["transition_origins"]["unknown"], 1);
    assert_eq!(diagnostic["shown_research"], 0);
    assert!(!error.message.contains("deadbeef") && error.message.len() < 1024);
    assert_eq!(clock.get(), start + BUDGET);
    assert_eq!(
        backend
            .calls
            .borrow()
            .iter()
            .filter(|v| **v == "read")
            .count(),
        201
    );
    assert_eq!(
        backend.calls.borrow().iter().filter(|v| **v == "W").count(),
        1
    );
    assert!(!backend.calls.borrow().contains(&"idle"));
    assert_eq!(journal.data.records.len(), 1);
    assert!(matches!(journal.data.records[0].outcome, Outcome::Intent));
    assert!(journal.blocks_game(&scope().epoch.game));
}

#[test]
fn classified_pair_counters_saturate_without_retaining_foreign_or_unknown_pairs() {
    let mut diagnostic = ReadbackDiagnostic {
        research_panel_pairs: [u32::MAX; 8],
        research_panel_pair_changes: u32::MAX,
        ..Default::default()
    };
    for a in 0..=2 {
        for b in 0..=2 {
            if a == 0 && b == 0 {
                continue;
            }
            let error = Err(Error::new("research_panel_transition", format!(
                "Closing root is in a known animation transition; transitions=[{a}, {b}]; root=research_panel"
            )));
            diagnostic.result(&error, Duration::from_millis(4999));
        }
    }
    assert_eq!(diagnostic.research_panel_pairs, [u32::MAX; 8]);
    assert_eq!(diagnostic.research_panel_pair_changes, u32::MAX);
    let previous = diagnostic.previous_research_panel_pair;
    for message in [
        "Closing root is in a known animation transition; transitions=[0, 1]; root=research_info",
        "Closing root is in a known animation transition; transitions=[0, 1]; root=research_panel\n",
    ] {
        diagnostic.result(&Err(Error::new("research_panel_transition", message)), BUDGET);
    }
    assert_eq!(diagnostic.research_panel_pairs, [u32::MAX; 8]);
    assert_eq!(diagnostic.previous_research_panel_pair, previous);
    assert!(!diagnostic.report(None, BUDGET).to_string().contains('\n'));
}
