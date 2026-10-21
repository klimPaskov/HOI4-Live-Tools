use super::*;
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hoi4-production-step-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("journal")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn epoch() -> Epoch {
    Epoch {
        game: Identity {
            pid: 120,
            start_ticks: "456".into(),
            exe: "/fixture/game".into(),
            uid: 1000,
        },
        campaign: "fixture-generation".into(),
        player: "GER".into(),
    }
}
fn target(factories: i32) -> Target {
    Target::Factories(FactoryTarget {
        epoch: epoch(),
        line_handle: "0xffff000100000038".into(),
        equipment_id: "infantry_equipment_1".into(),
        factories,
    })
}
fn state(requested: i32) -> State {
    State::Factories(factory_state(requested))
}
fn factory_state(requested: i32) -> FactoryState {
    FactoryState {
        epoch: epoch(),
        line: LineIdentity {
            country: 0x20000,
            production: 0x30000,
            line: 0x40000,
            variant: 0x50000,
            base_type: 0x60000,
            handle: 0xffff000100000038,
            equipment_id: "infantry_equipment_1".into(),
        },
        requested,
    }
}
fn authority() -> InputAuthority {
    let mut a = InputAuthority::new("player".into(), epoch()).unwrap();
    a.acquire("player", TOKEN.into(), 900, &epoch()).unwrap();
    a
}
struct Domains(bool);
impl UnresolvedInputs for Domains {
    fn clear(&self, _: &Identity) -> Result<()> {
        if self.0 {
            Err(error("action_uncertain", "Other input unresolved"))
        } else {
            Ok(())
        }
    }
}
struct Fake {
    current: FactoryState,
    reads: usize,
    candidates: usize,
    dispatches: usize,
    idles: usize,
    changes: BTreeMap<usize, State>,
    fail_read: Option<usize>,
    outcome: u8,
    apply: bool,
    path: std::path::PathBuf,
}
impl Fake {
    fn new(path: std::path::PathBuf, n: i32) -> Self {
        Self {
            current: factory_state(n),
            reads: 0,
            candidates: 0,
            dispatches: 0,
            idles: 0,
            changes: BTreeMap::new(),
            fail_read: None,
            outcome: 0,
            apply: true,
            path,
        }
    }
}
impl Backend for Fake {
    type Candidate = ();
    fn observe(&mut self, _: &Target, _: Instant) -> Result<State> {
        self.reads += 1;
        if self.fail_read == Some(self.reads) {
            return Err(error("fixture_read_failure", TOKEN));
        }
        if let Some(s) = self.changes.get(&self.reads) {
            self.current = s.factory().unwrap().clone();
        }
        Ok(State::Factories(self.current.clone()))
    }
    fn candidate(&mut self, _: &Target, before: &State, _: Instant) -> Result<()> {
        self.candidates += 1;
        assert_eq!(before.factory().unwrap(), &self.current);
        Ok(())
    }
    fn dispatch_once(&mut self, t: &Target, before: &State, _: &(), _: Instant) -> DispatchOutcome {
        assert!(
            fs::metadata(&self.path).unwrap().len() > 0,
            "intent must precede possible input"
        );
        assert_eq!(before.factory().unwrap(), &self.current);
        self.dispatches += 1;
        if self.apply {
            self.current.requested = t.factory().unwrap().factories;
        }
        match self.outcome {
            1 => DispatchOutcome::NotQueued(error("fixture_not_queued", "No delivery")),
            2 => DispatchOutcome::Unknown(Some(error("fixture_transport", TOKEN))),
            3 => DispatchOutcome::Unknown(None),
            _ => DispatchOutcome::ServerConfirmed,
        }
    }
    fn confirm_input_idle(&mut self, _: Instant) -> Result<()> {
        self.idles += 1;
        Ok(())
    }
}
fn execute(j: &mut Journal, f: &mut Fake, id: &str, n: i32) -> Result<Execution> {
    j.execute(
        id,
        target(n),
        TOKEN,
        &authority(),
        &RwLock::new(true),
        &Domains(false),
        f,
        Duration::from_millis(50),
    )
}

#[test]
fn increment_decrement_noop_and_historical_receipt_send_at_most_one_click() {
    for (before, after) in [(4, 5), (5, 4), (5, 5), (0, 1)] {
        let d = Scratch::new();
        let mut j = Journal::open(&d.path()).unwrap();
        let mut f = Fake::new(d.path(), before);
        let e = execute(&mut j, &mut f, "one", after).unwrap();
        assert_eq!(e.original_requested, Some(before));
        assert_eq!(e.status, Status::Verified);
        assert!(!j.blocks_game(&epoch().game));
        assert_eq!(f.dispatches, usize::from(before != after));
        let counts = (f.reads, f.candidates, f.dispatches, f.idles);
        drop(j);
        let mut j = Journal::open(&d.path()).unwrap();
        let replay = execute(&mut j, &mut f, "one", after).unwrap();
        assert!(replay.historical);
        assert!(!replay.current_state_verified);
        assert_eq!(counts, (f.reads, f.candidates, f.dispatches, f.idles));
        let public = serde_json::to_string(&e).unwrap();
        assert!(!public.contains("variant"));
        assert!(!public.contains("/fixture/game"));
    }
}
#[test]
fn lost_delivery_survives_restart_and_settles_only_original_target_without_replay() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    f.outcome = 2;
    f.apply = false;
    let e = execute(&mut j, &mut f, "one", 5).unwrap_err();
    assert_eq!(e.code, "action_uncertain");
    assert!(!e.message.contains(TOKEN));
    assert!(j.blocks_game(&epoch().game));
    drop(j);
    let mut j = Journal::open(&d.path()).unwrap();
    assert_eq!(
        execute(&mut j, &mut f, "one", 5).unwrap_err().code,
        "action_uncertain"
    );
    f.current.requested = 3;
    assert_eq!(
        execute(&mut j, &mut f, "one", 5).unwrap_err().code,
        "action_uncertain"
    );
    assert_eq!(
        execute(&mut j, &mut f, "new-id", 4).unwrap_err().code,
        "action_uncertain"
    );
    assert_eq!(f.dispatches, 1);
    f.current.requested = 5;
    let settled = execute(&mut j, &mut f, "one", 5).unwrap();
    assert_eq!(settled.original_requested, Some(4));
    assert_eq!(settled.delivery, Some(Delivery::Unknown));
    assert!(!settled.event_causality_verified);
    assert_eq!(f.dispatches, 1);
    assert!(!j.blocks_game(&epoch().game));
    drop(j);
    Journal::open(&d.path()).unwrap();
}
#[test]
fn reached_value_on_replaced_variant_or_campaign_never_settles() {
    for field in 0..3 {
        let d = Scratch::new();
        let mut j = Journal::open(&d.path()).unwrap();
        let mut f = Fake::new(d.path(), 4);
        f.outcome = 2;
        assert_eq!(
            execute(&mut j, &mut f, "one", 5).unwrap_err().code,
            "action_uncertain"
        );
        match field {
            0 => f.current.line.variant += 8,
            1 => f.current.line.line += 8,
            _ => f.current.epoch.campaign = "new".into(),
        }
        assert_eq!(
            execute(&mut j, &mut f, "one", 5).unwrap_err().code,
            "action_uncertain"
        );
        assert_eq!(f.dispatches, 1);
        assert!(j.blocks_game(&epoch().game));
    }
}
#[test]
fn intent_only_crash_recovery_uses_original_baseline() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let before = state(4);
    j.append(Record {
        version: 1,
        id: "one".into(),
        target: target(5),
        before: before.clone(),
        last: before,
        status: Status::Intent,
        delivery: None,
        cause: None,
    })
    .unwrap();
    drop(j);
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    assert_eq!(
        execute(&mut j, &mut f, "one", 5).unwrap_err().code,
        "action_uncertain"
    );
    f.current.requested = 5;
    assert_eq!(
        execute(&mut j, &mut f, "one", 5)
            .unwrap()
            .original_requested,
        Some(4)
    );
    assert_eq!(f.dispatches, 0);
    assert_eq!(f.candidates, 0);
}
#[test]
fn before_and_after_intent_changes_never_queue_input() {
    for read in [2, 3] {
        let d = Scratch::new();
        let mut j = Journal::open(&d.path()).unwrap();
        let mut f = Fake::new(d.path(), 4);
        f.changes.insert(read, state(5));
        let e = execute(&mut j, &mut f, "one", 5).unwrap_err();
        assert_eq!(
            e.code,
            if read == 2 {
                "production_state_changed"
            } else {
                "input_not_queued"
            }
        );
        assert_eq!(f.dispatches, 0);
        assert!(!j.blocks_game(&epoch().game));
        if read == 3 {
            assert_eq!(
                execute(&mut j, &mut f, "one", 5).unwrap_err().code,
                "input_not_queued"
            );
        }
    }
}
#[test]
fn proven_not_queued_never_replays_but_allows_a_new_request() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    f.outcome = 1;
    f.apply = false;
    assert_eq!(
        execute(&mut j, &mut f, "one", 5).unwrap_err().code,
        "input_not_queued"
    );
    assert!(!j.blocks_game(&epoch().game));
    drop(j);
    let mut j = Journal::open(&d.path()).unwrap();
    assert_eq!(
        execute(&mut j, &mut f, "one", 5).unwrap_err().code,
        "input_not_queued"
    );
    assert_eq!(f.dispatches, 1);
    f.outcome = 0;
    f.apply = true;
    execute(&mut j, &mut f, "two", 5).unwrap();
    assert_eq!(f.dispatches, 2);
}
#[test]
fn failed_post_input_read_preserves_real_cause_and_blocks_all_new_ids() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    f.fail_read = Some(4);
    let e = execute(&mut j, &mut f, "one", 5).unwrap_err();
    assert_eq!(e.code, "action_uncertain");
    assert!(e.message.contains("fixture_read_failure"));
    assert!(!e.message.contains(TOKEN));
    assert_eq!(
        execute(&mut j, &mut f, "two", 6).unwrap_err().code,
        "action_uncertain"
    );
    assert_eq!(f.dispatches, 1);
}
#[test]
fn unexpected_increment_does_not_count_as_success() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    f.changes.insert(4, state(9));
    assert_eq!(
        execute(&mut j, &mut f, "one", 5).unwrap_err().code,
        "action_uncertain"
    );
    assert!(j.blocks_game(&epoch().game));
    assert_eq!(f.dispatches, 1);
}
#[test]
fn malformed_targets_and_shared_blocks_never_reach_candidates() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    assert_eq!(
        execute(&mut j, &mut f, "one", 0).unwrap_err().code,
        "invalid_arguments"
    );
    assert_eq!(f.reads, 0);
    assert_eq!(
        execute(&mut j, &mut f, "one", 6).unwrap_err().code,
        "production_step_required"
    );
    assert_eq!(f.candidates, 0);
    assert_eq!(
        j.execute(
            "one",
            target(5),
            TOKEN,
            &authority(),
            &RwLock::new(true),
            &Domains(true),
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(
        j.execute(
            "one",
            target(5),
            TOKEN,
            &authority(),
            &RwLock::new(false),
            &Domains(false),
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "session_closed"
    );
    assert_eq!(f.candidates, 0);
}
#[test]
fn changed_target_id_and_forged_baseline_are_rejected() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    f.outcome = 2;
    f.apply = false;
    execute(&mut j, &mut f, "one", 5).unwrap_err();
    assert_eq!(
        execute(&mut j, &mut f, "one", 3).unwrap_err().code,
        "request_id_conflict"
    );
    let original = j.records["one"].clone();
    let mut forged = original.clone();
    let State::Factories(before) = &mut forged.before else {
        panic!("factory baseline")
    };
    before.requested = 6;
    forged.last = state(5);
    forged.status = Status::Verified;
    forged.cause = None;
    assert_eq!(
        Journal::transition(&original, &forged).unwrap_err().code,
        "invalid_journal"
    );
    assert_eq!(f.dispatches, 1);
}
#[test]
fn partial_tail_oversized_line_and_missing_intent_are_rejected() {
    for kind in 0..3 {
        let d = Scratch::new();
        let mut j = Journal::open(&d.path()).unwrap();
        match kind {
            0 => j.file.write_all(b"{").unwrap(),
            1 => j.file.write_all(&vec![b'x'; LINE + 1]).unwrap(),
            _ => {
                let r = Record {
                    version: 1,
                    id: "one".into(),
                    target: target(5),
                    before: state(4),
                    last: state(5),
                    status: Status::Verified,
                    delivery: Some(Delivery::Unknown),
                    cause: None,
                };
                let mut b = serde_json::to_vec(&r).unwrap();
                b.push(b'\n');
                j.file.write_all(&b).unwrap();
            }
        }
        drop(j);
        assert!(matches!(Journal::open(&d.path()),Err(e) if e.code=="invalid_journal"));
    }
}
#[test]
fn poisoned_writer_refuses_even_a_known_target_without_observation() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 5);
    j.poisoned = true;
    assert_eq!(
        execute(&mut j, &mut f, "one", 5).unwrap_err().code,
        "action_uncertain"
    );
    assert_eq!(f.reads, 0);
    assert!(j.blocks_game(&epoch().game));
}

fn panel_state(mode: i32, shown: bool) -> PanelState {
    PanelState {
        epoch: epoch(),
        panel_identity: (2..9).map(|n| n * 0x10000).collect(),
        managed_mode: mode,
        panel_shown: shown,
    }
}
struct PanelFake {
    state: PanelState,
    dispatches: usize,
    reads: usize,
    fail: bool,
    uncertain: bool,
    transitions: usize,
}
impl Backend for PanelFake {
    type Candidate = ();
    fn observe(&mut self, _: &Target, _: Instant) -> Result<State> {
        self.reads += 1;
        if self.dispatches > 0 && self.transitions > 0 {
            self.transitions -= 1;
            return Err(error("production_panel_transition", "Panel animating"));
        }
        if self.fail {
            return Err(error("fixture_panel_read", "Unavailable panel"));
        }
        Ok(State::Panel(self.state.clone()))
    }
    fn candidate(&mut self, _: &Target, before: &State, _: Instant) -> Result<()> {
        assert_eq!(before, &State::Panel(self.state.clone()));
        Ok(())
    }
    fn dispatch_once(&mut self, _: &Target, before: &State, _: &(), _: Instant) -> DispatchOutcome {
        assert_eq!(before, &State::Panel(self.state.clone()));
        self.dispatches += 1;
        self.state.managed_mode = 2;
        self.state.panel_shown = true;
        if self.uncertain {
            DispatchOutcome::Unknown(Some(error("fixture_lost_reply", "Lost reply")))
        } else {
            DispatchOutcome::ServerConfirmed
        }
    }
    fn confirm_input_idle(&mut self, _: Instant) -> Result<()> {
        Ok(())
    }
}
fn open(j: &mut Journal, f: &mut PanelFake, id: &str) -> Result<Execution> {
    j.execute(
        id,
        Target::Panel(PanelTarget::new(epoch())),
        TOKEN,
        &authority(),
        &RwLock::new(true),
        &Domains(false),
        f,
        Duration::from_millis(50),
    )
}
#[test]
fn panel_opens_once_and_already_open_never_toggles() {
    for (mode, shown, clicks) in [(23, false, 1), (2, true, 0)] {
        let d = Scratch::new();
        let mut j = Journal::open(&d.path()).unwrap();
        let mut f = PanelFake {
            state: panel_state(mode, shown),
            dispatches: 0,
            reads: 0,
            fail: false,
            uncertain: false,
            transitions: 0,
        };
        let e = open(&mut j, &mut f, "open").unwrap();
        assert_eq!(e.stage, Some("ensure_open"));
        assert_eq!(e.status, Status::Verified);
        assert_eq!(f.dispatches, clicks);
        let public = serde_json::to_value(&e).unwrap();
        for absent in [
            "line_handle",
            "equipment_id",
            "requested_factories",
            "original_requested",
            "panel_identity",
        ] {
            assert!(public.get(absent).is_none());
        }
        drop(j);
        let mut j = Journal::open(&d.path()).unwrap();
        f.fail = true;
        assert!(open(&mut j, &mut f, "open").unwrap().historical);
        assert_eq!(f.dispatches, clicks);
    }
}
#[test]
fn panel_uncertainty_blocks_allocations_and_only_original_panel_can_settle() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = PanelFake {
        state: panel_state(23, false),
        dispatches: 0,
        reads: 0,
        fail: false,
        uncertain: true,
        transitions: 0,
    };
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap_err().code,
        "action_uncertain"
    );
    drop(j);
    let mut j = Journal::open(&d.path()).unwrap();
    let mut allocation = Fake::new(d.path(), 4);
    assert_eq!(
        execute(&mut j, &mut allocation, "factory", 5)
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    assert_eq!(allocation.reads, 0);
    assert_eq!(
        execute(&mut j, &mut allocation, "open", 5)
            .unwrap_err()
            .code,
        "request_id_conflict"
    );
    f.state = panel_state(5, true); // Constructor argument is not active production mode.
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap_err().code,
        "action_uncertain"
    );
    f.state = panel_state(23, false); // Unchanged does not mean input never applied.
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap_err().code,
        "action_uncertain"
    );
    f.state = panel_state(2, true);
    f.state.panel_identity[6] += 8;
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap_err().code,
        "action_uncertain"
    );
    f.state = panel_state(2, true);
    f.fail = true;
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap_err().code,
        "action_uncertain"
    );
    f.fail = false;
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap().status,
        Status::Verified
    );
    assert_eq!(f.dispatches, 1);
    assert!(!j.blocks_game(&epoch().game));
}
#[test]
fn allocation_uncertainty_blocks_panel_and_invalid_panel_states_never_dispatch() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut allocation = Fake::new(d.path(), 4);
    allocation.outcome = 2;
    execute(&mut j, &mut allocation, "factory", 5).unwrap_err();
    let mut f = PanelFake {
        state: panel_state(23, false),
        dispatches: 0,
        reads: 0,
        fail: false,
        uncertain: false,
        transitions: 0,
    };
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap_err().code,
        "action_uncertain"
    );
    assert_eq!(f.reads, 0);
    for (mode, shown) in [(5, true), (23, true), (2, false), (17, false), (24, false)] {
        let d = Scratch::new();
        let mut j = Journal::open(&d.path()).unwrap();
        f.state = panel_state(mode, shown);
        assert!(open(&mut j, &mut f, "open").is_err());
        assert_eq!(f.dispatches, 0);
    }
}
#[test]
fn original_v1_factory_records_still_reopen_with_the_same_public_receipt_shape() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    // Literal pre-navigation v1 target/state shapes, without enum tags.
    let t = serde_json::json!({"epoch":epoch(),"line_handle":"0xffff000100000038","equipment_id":"infantry_equipment_1","factories":4});
    let s = serde_json::json!({"epoch":epoch(),"line":factory_state(4).line,"requested":4});
    let r = serde_json::json!({"version":1,"id":"old","target":t,"before":s,"last":s,"status":"Verified","delivery":null,"cause":null});
    let mut bytes = serde_json::to_vec(&r).unwrap();
    bytes.push(b'\n');
    j.file.write_all(&bytes).unwrap();
    j.file.sync_all().unwrap();
    drop(j);
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = Fake::new(d.path(), 4);
    let v = serde_json::to_value(execute(&mut j, &mut f, "old", 4).unwrap()).unwrap();
    assert_eq!(v["original_requested"], 4);
    assert_eq!(v["requested_factories"], 4);
    assert!(v.get("stage").is_none());
    assert_eq!(f.reads, 0);
    assert_eq!(serde_json::to_value(target(4)).unwrap(), r["target"]);
    assert_eq!(serde_json::to_value(state(4)).unwrap(), r["before"]);
    let mut mixed = r["target"].clone();
    mixed["stage"] = serde_json::json!("ensure_open");
    assert!(serde_json::from_value::<Target>(mixed).is_err());
}

#[test]
fn panel_transition_readback_recovers_without_replaying_input() {
    let d = Scratch::new();
    let mut j = Journal::open(&d.path()).unwrap();
    let mut f = PanelFake {
        state: panel_state(23, false),
        dispatches: 0,
        reads: 0,
        fail: false,
        uncertain: false,
        transitions: 1,
    };
    assert_eq!(
        open(&mut j, &mut f, "open").unwrap().status,
        Status::Verified
    );
    assert_eq!(f.dispatches, 1);
    assert_eq!(f.transitions, 0);
    assert!(!j.blocks_game(&epoch().game));
}
