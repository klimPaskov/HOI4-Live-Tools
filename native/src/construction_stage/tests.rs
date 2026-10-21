use super::*;
use std::{
    cell::Cell,
    collections::VecDeque,
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "hoi4-private-construction-draft-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
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
fn target(stage: Stage) -> Target {
    Target {
        epoch: epoch(),
        stage,
        building_id: matches!(stage, Stage::SelectBuilding | Stage::Place)
            .then(|| "dockyard".into()),
        state_id: matches!(stage, Stage::FindQuery | Stage::FindActivate | Stage::Place)
            .then_some(64),
        amount: (stage == Stage::Place).then_some(2),
    }
}
fn find_before(stage: Stage) -> Option<FindState> {
    let query = |q: &str| Some(q.to_owned());
    match stage {
        Stage::FindOpen => Some(FindState {
            visible: Some(false),
            text_focused: Some(false),
            query: query(""),
            first_kind: None,
            first_state_id: None,
            target_owned: None,
            camera_centered: None,
        }),
        Stage::FindQuery => Some(FindState {
            visible: Some(true),
            text_focused: Some(true),
            query: query(""),
            first_kind: None,
            first_state_id: None,
            target_owned: Some(true),
            camera_centered: Some(false),
        }),
        Stage::Place => Some(FindState {
            visible: Some(false),
            text_focused: Some(true),
            query: query("64"),
            first_kind: Some(2),
            first_state_id: Some(64),
            target_owned: Some(true),
            camera_centered: Some(true),
        }),
        Stage::FindActivate => Some(FindState {
            visible: Some(true),
            text_focused: Some(true),
            query: query("64"),
            first_kind: Some(2),
            first_state_id: Some(64),
            target_owned: Some(true),
            camera_centered: Some(false),
        }),
        _ => None,
    }
}
fn authority() -> InputAuthority {
    let mut a = InputAuthority::new("luna".into(), epoch()).unwrap();
    a.acquire("luna", TOKEN.into(), 300, &epoch()).unwrap();
    a
}
fn before(stage: Stage) -> OwnState {
    OwnState {
        epoch: epoch(),
        managed_mode: Some(if stage == Stage::EnsureOpen { 23 } else { 17 }),
        panel: if stage == Stage::EnsureOpen {
            PanelState::Hidden
        } else {
            PanelState::Shown
        },
        selected_mode: Some(if stage == Stage::Place { 1 } else { 0 }),
        selected_building: (stage == Stage::Place).then(|| "dockyard".into()),
        catalog_coverage: if stage == Stage::EnsureOpen {
            Coverage::NotChecked
        } else {
            Coverage::Complete
        },
        choice_coverage: if stage == Stage::EnsureOpen {
            Coverage::NotChecked
        } else {
            Coverage::Complete
        },
        requested_choice_present: stage == Stage::SelectBuilding,
        scoped_blockers_clear: Some(true),
        local_dispatch_ready: Some(true),
        find: find_before(stage),
        placement: (stage == Stage::Place).then_some(PlacementState {
            in_state: 1,
            elsewhere: 3,
            complete: true,
        }),
    }
}
fn reached(stage: Stage) -> OwnState {
    let mut state = before(stage);
    if let Some(p) = state.placement.as_mut() {
        p.in_state = 2;
        return state;
    }
    if let Some(find) = state.find.as_mut() {
        match stage {
            Stage::FindOpen => {
                find.visible = Some(true);
                find.text_focused = Some(true);
            }
            Stage::FindQuery => {
                find.query = Some("64".into());
                find.first_kind = Some(2);
                find.first_state_id = Some(64);
            }
            _ => {
                find.visible = Some(false);
                find.text_focused = Some(false);
                find.camera_centered = Some(true);
            }
        }
        return state;
    }
    state.managed_mode = Some(17);
    state.panel = PanelState::Shown;
    if stage == Stage::SelectBuilding {
        state.selected_mode = Some(1);
        state.selected_building = Some("dockyard".into());
    }
    state
}
fn observation(state: OwnState, scope: u64) -> Observation<u64> {
    Observation { state, scope }
}
fn point_mut(candidate: &mut Candidate<u64>) -> &mut PointCandidate {
    match &mut candidate.input {
        Input::Point(point) => point,
        _ => panic!("pointer fixture"),
    }
}
fn point() -> PointCandidate {
    PointCandidate {
        ideal_logical: [30, 30],
        quantized_logical: [30, 30],
        client_pixel: [10, 10],
        expected_client: [512, 288],
        current_frame_verified: false,
        effective_routing_verified: false,
        input_authorized: false,
    }
}
#[derive(Default)]
struct Domains {
    value: DomainBlocks,
    calls: Cell<usize>,
    block_on: Option<usize>,
    unavailable: bool,
}
impl UnresolvedInputs for Domains {
    fn observe(&self, game: &Identity) -> Result<DomainBlocks> {
        assert_eq!(game, &epoch().game);
        self.calls.set(self.calls.get() + 1);
        if self.unavailable {
            return Err(error("journal_failed", "Domain unavailable"));
        }
        if self.block_on == Some(self.calls.get()) {
            return Ok(DomainBlocks {
                focus: true,
                ..DomainBlocks::default()
            });
        }
        Ok(self.value)
    }
}
struct Fake {
    states: VecDeque<Result<Observation<u64>>>,
    last: Observation<u64>,
    observations: usize,
    candidates: usize,
    dispatches: usize,
    possible_inputs: usize,
    idle_checks: usize,
    candidate: Candidate<u64>,
    delivery: Delivery,
    panic_dispatch: bool,
    idle_error: bool,
    exhaust_on_dispatch: bool,
    exhaust_on_idle: bool,
    dispatch_error: bool,
    deadline: Option<Instant>,
    preinput_phases: Vec<read_recovery::Phase>,
}
impl Fake {
    fn new(stage: Stage) -> Self {
        Self {
            states: VecDeque::new(),
            last: observation(before(stage), 1),
            observations: 0,
            candidates: 0,
            dispatches: 0,
            possible_inputs: 0,
            idle_checks: 0,
            candidate: Candidate {
                scope: 1,
                input: Input::Point(point()),
            },
            delivery: Delivery::Unknown,
            panic_dispatch: false,
            idle_error: false,
            exhaust_on_dispatch: false,
            exhaust_on_idle: false,
            dispatch_error: false,
            deadline: None,
            preinput_phases: Vec::new(),
        }
    }
    fn success(stage: Stage) -> Self {
        let mut f = Self::new(stage);
        f.states = [
            before(stage),
            before(stage),
            before(stage),
            reached(stage),
            reached(stage),
        ]
        .into_iter()
        .map(|s| Ok(observation(s, 1)))
        .collect();
        f
    }
    fn same_deadline(&self, value: Instant) {
        assert_eq!(self.deadline.unwrap(), value);
    }
}
impl Backend for Fake {
    type Scope = u64; // Fixture identity token only; not an actual game pointer.
    fn observe(&mut self, _: &Target) -> Result<Observation<u64>> {
        self.observations += 1;
        if let Some(result) = self.states.pop_front() {
            if let Ok(value) = &result {
                self.last = value.clone();
            }
            result
        } else {
            Ok(self.last.clone())
        }
    }
    fn observe_before_input(
        &mut self,
        target: &Target,
        phase: read_recovery::Phase,
        deadline: Instant,
    ) -> Result<Observation<u64>> {
        if let Some(prior) = self.deadline {
            assert_eq!(prior, deadline);
        } else {
            self.deadline = Some(deadline);
        }
        self.preinput_phases.push(phase);
        self.observe(target)
    }
    fn candidate(
        &mut self,
        _: &Target,
        expected: &u64,
        deadline: Instant,
    ) -> Result<Candidate<u64>> {
        assert_eq!(*expected, 1);
        self.candidates += 1;
        self.same_deadline(deadline);
        Ok(self.candidate.clone())
    }
    fn dispatch_once(
        &mut self,
        _: &Target,
        c: &Candidate<u64>,
        deadline: Instant,
    ) -> DispatchOutcome {
        assert_eq!(c.scope, 1);
        self.same_deadline(deadline);
        self.dispatches += 1;
        if self.delivery != Delivery::NotQueued {
            self.possible_inputs += 1;
        }
        if self.panic_dispatch {
            panic!("fixture connection lost after possible dispatch");
        }
        if self.exhaust_on_dispatch {
            thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
        }
        match self.delivery {
            Delivery::NotQueued => DispatchOutcome::NotQueued(error(
                "input_geometry_changed",
                "Final owned control geometry changed",
            )),
            Delivery::Unknown => DispatchOutcome::Unknown(self.dispatch_error.then(|| {
                error(
                    "transport_lost",
                    "Ordinary dispatcher connection lost after possible event",
                )
            })),
            Delivery::ServerConfirmed => DispatchOutcome::ServerConfirmed,
        }
    }
    fn confirm_input_idle(&mut self, _: &Candidate<u64>, deadline: Instant) -> Result<()> {
        self.same_deadline(deadline);
        self.idle_checks += 1;
        if self.exhaust_on_idle {
            thread::sleep(
                deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
            );
        }
        if self.idle_error {
            Err(error("input_unavailable", "Held input unverified"))
        } else {
            Ok(())
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn execute(
    j: &mut Journal,
    id: &str,
    t: Target,
    a: &InputAuthority,
    g: &RwLock<bool>,
    domains: &Domains,
    f: &mut Fake,
    timeout: Duration,
) -> Result<Execution> {
    j.execute(id, t, TOKEN, a, g, domains, f, timeout)
}
const TIME: Duration = Duration::from_secs(2);

struct RecoveringFake {
    inner: Fake,
    report: read_recovery::Report,
    persistent: Option<read_recovery::Phase>,
}
impl RecoveringFake {
    fn producer(&mut self, phase: read_recovery::Phase, deadline: Instant) -> Result<()> {
        let mut calls = 0;
        read_recovery::observe(
            phase,
            deadline,
            || {
                calls += 1;
                if calls == 1 || self.persistent == Some(phase) {
                    Err(error("torn_read", "Snapshot witness changed; section=opener; read=171; size=4; changed_byte=1"))
                } else {
                    Ok(())
                }
            },
            &mut self.report,
        )
    }
}
impl Backend for RecoveringFake {
    type Scope = u64;
    fn observe(&mut self, target: &Target) -> Result<Observation<u64>> {
        // Post-input reads never enter the pre-input recovery path.
        self.inner.observe(target)
    }
    fn observe_before_input(
        &mut self,
        target: &Target,
        phase: read_recovery::Phase,
        deadline: Instant,
    ) -> Result<Observation<u64>> {
        self.producer(phase, deadline)?;
        self.inner.observe_before_input(target, phase, deadline)
    }
    fn candidate(
        &mut self,
        target: &Target,
        scope: &u64,
        deadline: Instant,
    ) -> Result<Candidate<u64>> {
        self.producer(read_recovery::Phase::Candidate, deadline)?;
        self.inner.candidate(target, scope, deadline)
    }
    fn dispatch_once(
        &mut self,
        target: &Target,
        point: &Candidate<u64>,
        deadline: Instant,
    ) -> DispatchOutcome {
        if let Err(cause) = self.producer(read_recovery::Phase::DispatchCheck, deadline) {
            return DispatchOutcome::NotQueued(cause);
        }
        self.inner.dispatch_once(target, point, deadline)
    }
    fn confirm_input_idle(&mut self, point: &Candidate<u64>, deadline: Instant) -> Result<()> {
        self.inner.confirm_input_idle(point, deadline)
    }
}

#[test]
fn explicit_read_phases_preserve_single_input_and_historical_replay() {
    use read_recovery::Phase::*;
    for stage in [Stage::EnsureOpen, Stage::SelectBuilding] {
        let s = Scratch::new();
        let a = authority();
        let gate = RwLock::new(true);
        let domains = Domains::default();
        let mut f = RecoveringFake {
            inner: Fake::success(stage),
            report: Default::default(),
            persistent: None,
        };
        f.inner.delivery = Delivery::ServerConfirmed;
        let mut j = Journal::open(&s.path()).unwrap();
        let result = j
            .execute(
                "recover-once",
                target(stage),
                TOKEN,
                &a,
                &gate,
                &domains,
                &mut f,
                TIME,
            )
            .unwrap();
        assert_eq!(result.receipt.status, Status::Verified);
        assert_eq!(
            f.inner.preinput_phases,
            [Initial, BeforeIntent, AfterIntent]
        );
        assert_eq!(f.inner.dispatches, 1);
        assert_eq!(f.inner.possible_inputs, 1);
        let report = f.report.value(TOKEN);
        let phases: Vec<_> = report
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["phase"].as_str().unwrap())
            .collect();
        assert_eq!(
            phases,
            [
                "initial",
                "candidate",
                "before_intent",
                "after_intent",
                "dispatch_check"
            ]
        );
        let observed = f.inner.observations;
        let replay = j
            .execute(
                "recover-once",
                target(stage),
                TOKEN,
                &a,
                &gate,
                &domains,
                &mut f,
                TIME,
            )
            .unwrap();
        assert!(replay.historical);
        assert_eq!(replay.receipt, result.receipt);
        assert_eq!(f.inner.observations, observed);
        assert_eq!(f.inner.dispatches, 1);
        assert_eq!(f.report.value(TOKEN), report);
    }
}

#[test]
fn persistent_read_refusal_preserves_before_intent_or_durable_not_queued_boundary() {
    use read_recovery::Phase::*;
    for phase in [Initial, Candidate, BeforeIntent, AfterIntent, DispatchCheck] {
        let s = Scratch::new();
        let a = authority();
        let gate = RwLock::new(true);
        let domains = Domains::default();
        let mut f = RecoveringFake {
            inner: Fake::success(Stage::EnsureOpen),
            report: Default::default(),
            persistent: Some(phase),
        };
        let mut j = Journal::open(&s.path()).unwrap();
        let t = target(Stage::EnsureOpen);
        let error = j
            .execute(
                "persistent",
                t.clone(),
                TOKEN,
                &a,
                &gate,
                &domains,
                &mut f,
                TIME,
            )
            .unwrap_err();
        assert_eq!(f.inner.dispatches, 0);
        assert_eq!(f.inner.possible_inputs, 0);
        if matches!(phase, AfterIntent | DispatchCheck) {
            assert_eq!(error.code, "input_not_queued");
            assert_eq!(
                j.failure_evidence("persistent", &t, &error),
                FailureEvidence::ConfirmedNotQueued
            );
            let before = f.report.value(TOKEN);
            let error = j
                .execute("persistent", t, TOKEN, &a, &gate, &domains, &mut f, TIME)
                .unwrap_err();
            assert_eq!(error.code, "input_not_queued");
            assert_eq!(f.report.value(TOKEN), before);
        } else {
            assert_eq!(error.code, "torn_read");
            assert_eq!(
                j.failure_evidence("persistent", &t, &error),
                FailureEvidence::BeforeIntent
            );
            assert!(j.receipts.is_empty());
        }
    }
}

#[test]
fn already_reached_stages_noop_and_restart_replays_without_observation_or_input() {
    for stage in [Stage::EnsureOpen, Stage::SelectBuilding] {
        let s = Scratch::new();
        let a = authority();
        let gate = RwLock::new(true);
        let domains = Domains::default();
        let mut f = Fake::new(stage);
        f.last.state = reached(stage);
        let mut j = Journal::open(&s.path()).unwrap();
        let result = execute(
            &mut j,
            "same",
            target(stage),
            &a,
            &gate,
            &domains,
            &mut f,
            TIME,
        )
        .unwrap();
        assert!(result.current_state_verified && !result.historical);
        assert_eq!(result.receipt.delivery, None);
        assert_eq!((f.candidates, f.dispatches, f.idle_checks), (0, 0, 0));
        drop(j);
        let mut j = Journal::open(&s.path()).unwrap();
        let mut retry = Fake::new(stage);
        let historical = execute(
            &mut j,
            "same",
            target(stage),
            &a,
            &gate,
            &domains,
            &mut retry,
            TIME,
        )
        .unwrap();
        assert!(
            historical.historical
                && !historical.current_state_verified
                && !historical.event_causality_verified
        );
        assert_eq!((retry.observations, retry.dispatches), (0, 0));
    }
}
#[test]
fn each_stage_dispatches_one_point_only_after_durable_intent_then_confirms_canonical_and_idle() {
    for stage in [Stage::EnsureOpen, Stage::SelectBuilding] {
        for delivery in [Delivery::Unknown, Delivery::ServerConfirmed] {
            let s = Scratch::new();
            let mut j = Journal::open(&s.path()).unwrap();
            let mut f = Fake::success(stage);
            f.delivery = delivery;
            let out = execute(
                &mut j,
                "one",
                target(stage),
                &authority(),
                &RwLock::new(true),
                &Domains::default(),
                &mut f,
                TIME,
            )
            .unwrap();
            assert_eq!((f.dispatches, f.possible_inputs, f.idle_checks), (1, 1, 1));
            assert_eq!(out.receipt.status, Status::Verified);
            assert_eq!(out.receipt.delivery, Some(delivery));
            assert!(!out.event_causality_verified);
            assert!(!j.blocks_game(&epoch().game));
            // Windows enforces the exclusive writer lock against other handles.
            // Inspect the durable file only after releasing the journal.
            drop(j);
            let lines: Vec<Receipt> = fs::read_to_string(s.path())
                .unwrap()
                .lines()
                .map(|line| serde_json::from_str(line).unwrap())
                .collect();
            assert_eq!(
                lines.iter().map(|r| r.status).collect::<Vec<_>>(),
                [Status::Intent, Status::Verified]
            );
        }
    }
}
#[test]
fn open_toggle_rejects_mode_visibility_disagreement_and_unknown_or_transitional_panel() {
    let mut states = Vec::new();
    for panel in [
        PanelState::Shown,
        PanelState::Unknown,
        PanelState::Unmaterialized,
        PanelState::Transitional,
    ] {
        let mut v = before(Stage::EnsureOpen);
        v.panel = panel;
        states.push(v);
    }
    for mode in [None, Some(17), Some(5)] {
        let mut v = before(Stage::EnsureOpen);
        v.managed_mode = mode;
        states.push(v);
    }
    for mode in [None, Some(1), Some(2), Some(3)] {
        let mut v = before(Stage::EnsureOpen);
        v.selected_mode = mode;
        states.push(v);
    }
    for state in states {
        assert_preinput_refusal(Stage::EnsureOpen, state);
    }
}
fn assert_preinput_refusal(stage: Stage, state: OwnState) {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::new(stage);
    f.last.state = state;
    assert!(execute(
        &mut j,
        "refuse",
        target(stage),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME
    )
    .is_err());
    assert_eq!((f.candidates, f.dispatches), (0, 0));
    assert_eq!(fs::metadata(s.path()).unwrap().len(), 0);
}
#[test]
fn selection_requires_full_unique_catalog_choice_and_known_ordinary_mode_and_gates() {
    for i in 0..13 {
        let mut state = before(Stage::SelectBuilding);
        match i {
            0 => state.catalog_coverage = Coverage::NotChecked,
            1 => state.choice_coverage = Coverage::Unavailable,
            2 => state.requested_choice_present = false,
            3 => state.selected_mode = Some(2),
            4 => state.selected_mode = Some(3),
            5 => state.selected_mode = None,
            6 => state.selected_mode = Some(1), // missing canonical selected key
            7 => state.managed_mode = Some(23),
            8 => state.panel = PanelState::Hidden,
            9 => state.panel = PanelState::Transitional,
            10 => state.scoped_blockers_clear = None,
            11 => state.local_dispatch_ready = Some(false),
            _ => state.scoped_blockers_clear = Some(false),
        }
        assert_preinput_refusal(Stage::SelectBuilding, state);
    }
}
#[test]
fn changing_a_different_canonical_ordinary_selection_is_supported() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::SelectBuilding);
    for state in f.states.iter_mut().take(3) {
        let v = &mut state.as_mut().unwrap().state;
        v.selected_mode = Some(1);
        v.selected_building = Some("arms_factory".into());
    }
    execute(
        &mut j,
        "change",
        target(Stage::SelectBuilding),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME,
    )
    .unwrap();
    assert_eq!(f.possible_inputs, 1);
}
#[test]
fn invalid_request_target_timeout_and_unsupported_railway_never_observe_or_persist() {
    for i in 0..8 {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::new(Stage::SelectBuilding);
        let mut t = target(Stage::SelectBuilding);
        let mut id = "valid".to_owned();
        let mut timeout = TIME;
        match i {
            0 => id = "bad request".into(),
            1 => id = "x".repeat(129),
            2 => t.building_id = Some("rail_way".into()),
            3 => t.building_id = Some("localized name".into()),
            4 => t.building_id = None,
            5 => t.stage = Stage::EnsureOpen,
            6 => timeout = Duration::ZERO,
            _ => timeout = Duration::from_secs(11),
        }
        assert_eq!(
            execute(
                &mut j,
                &id,
                t,
                &authority(),
                &RwLock::new(true),
                &Domains::default(),
                &mut f,
                timeout
            )
            .unwrap_err()
            .code,
            "invalid_arguments"
        );
        assert_eq!(f.observations, 0);
        assert_eq!(fs::metadata(s.path()).unwrap().len(), 0);
    }
}
#[test]
fn same_looking_identity_changes_in_candidate_preintent_and_final_observation_refuse() {
    for phase in 0..3 {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let stage = Stage::SelectBuilding;
        let mut f = Fake::success(stage);
        match phase {
            0 => f.candidate.scope = 2,
            1 => f.states[1].as_mut().unwrap().scope = 2,
            _ => f.states[2].as_mut().unwrap().scope = 2,
        }
        let err = execute(
            &mut j,
            "scope",
            target(stage),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME,
        )
        .unwrap_err();
        assert_eq!(f.dispatches, 0);
        if phase == 2 {
            assert_eq!(err.code, "input_not_queued");
            assert_eq!(j.receipts["scope"].status, Status::NotQueued);
        } else {
            assert_eq!(err.code, "ownership_changed");
            assert!(j.receipts.is_empty());
        }
    }
}
#[test]
fn scope_or_foreign_epoch_change_after_input_never_commits_replaced_payload() {
    for foreign in [false, true] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let stage = Stage::SelectBuilding;
        let mut f = Fake::success(stage);
        if foreign {
            f.states[3].as_mut().unwrap().state.epoch.player = "FRA".into();
        } else {
            f.states[3].as_mut().unwrap().scope = 2;
        }
        assert_eq!(
            execute(
                &mut j,
                "changed",
                target(stage),
                &authority(),
                &RwLock::new(true),
                &Domains::default(),
                &mut f,
                TIME
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        let r = &j.receipts["changed"];
        assert_eq!(r.observed, before(stage));
        assert_eq!(r.status, Status::Uncertain);
        assert_eq!(
            r.uncertainty,
            Some(if foreign {
                Uncertainty::Ownership
            } else {
                Uncertainty::Scope
            })
        );
        assert!(j.blocks_game(&epoch().game));
        assert_eq!(f.possible_inputs, 1);
    }
}
#[test]
fn already_reached_during_preintent_refresh_becomes_noop_without_click() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::EnsureOpen;
    let mut f = Fake::new(stage);
    f.states = [before(stage), reached(stage)]
        .into_iter()
        .map(|s| Ok(observation(s, 1)))
        .collect();
    let out = execute(
        &mut j,
        "fresh-noop",
        target(stage),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME,
    )
    .unwrap();
    assert_eq!(out.receipt.delivery, None);
    assert_eq!(f.dispatches, 0);
}
#[test]
fn changed_canonical_postcondition_persists_actual_own_state_and_never_replays() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::SelectBuilding;
    let mut f = Fake::new(stage);
    let mut wrong = reached(stage);
    wrong.selected_building = Some("arms_factory".into());
    f.states = [before(stage), before(stage), before(stage), wrong.clone()]
        .into_iter()
        .map(|s| Ok(observation(s, 1)))
        .collect();
    let a = authority();
    let gate = RwLock::new(true);
    let domains = Domains::default();
    assert_eq!(
        execute(
            &mut j,
            "mismatch",
            target(stage),
            &a,
            &gate,
            &domains,
            &mut f,
            Duration::from_millis(200)
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(j.receipts["mismatch"].observed, wrong);
    assert_eq!(f.possible_inputs, 1);
    drop(j);
    let mut j = Journal::open(&s.path()).unwrap();
    let mut retry = Fake::success(stage);
    for id in ["mismatch", "fresh-id"] {
        assert_eq!(
            execute(
                &mut j,
                id,
                target(stage),
                &a,
                &gate,
                &domains,
                &mut retry,
                TIME
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
    }
    assert_eq!((retry.observations, retry.possible_inputs), (0, 0));
}
#[test]
fn transport_error_after_possible_input_is_durable_uncertainty() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::EnsureOpen;
    let mut f = Fake::success(stage);
    f.states[3] = Err(error("io_error", "transport lost"));
    assert_eq!(
        execute(
            &mut j,
            "lost",
            target(stage),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(
        j.receipts["lost"].uncertainty,
        Some(Uncertainty::Observation)
    );
    drop(j);
    assert!(Journal::open(&s.path()).unwrap().blocks_game(&epoch().game));
}
#[test]
fn crash_after_durable_intent_survives_restart_and_blocks_other_stage_and_id() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    f.panic_dispatch = true;
    let a = authority();
    let gate = RwLock::new(true);
    let domains = Domains::default();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute(
            &mut j,
            "crash",
            target(Stage::EnsureOpen),
            &a,
            &gate,
            &domains,
            &mut f,
            TIME,
        )
    }))
    .is_err());
    assert_eq!(f.possible_inputs, 1);
    drop(j);
    let mut j = Journal::open(&s.path()).unwrap();
    assert_eq!(j.receipts["crash"].status, Status::Intent);
    let mut retry = Fake::success(Stage::SelectBuilding);
    assert_eq!(
        execute(
            &mut j,
            "new-stage",
            target(Stage::SelectBuilding),
            &a,
            &gate,
            &domains,
            &mut retry,
            TIME
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(retry.observations, 0);
}
#[test]
fn positive_not_queued_refusal_is_terminal_and_same_id_never_dispatches_again() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::SelectBuilding;
    let mut f = Fake::success(stage);
    f.delivery = Delivery::NotQueued;
    let a = authority();
    let gate = RwLock::new(true);
    let domains = Domains::default();
    assert_eq!(
        execute(
            &mut j,
            "refused",
            target(stage),
            &a,
            &gate,
            &domains,
            &mut f,
            TIME
        )
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    assert_eq!(f.possible_inputs, 0);
    assert!(!j.blocks_game(&epoch().game));
    drop(j);
    let mut j = Journal::open(&s.path()).unwrap();
    let mut retry = Fake::success(stage);
    assert_eq!(
        execute(
            &mut j,
            "refused",
            target(stage),
            &a,
            &gate,
            &domains,
            &mut retry,
            TIME
        )
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    assert_eq!(retry.observations, 0);
}
#[test]
fn all_unresolved_domains_and_unknown_domain_state_block_before_observation() {
    for i in 0..6 {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::success(Stage::EnsureOpen);
        let mut d = Domains::default();
        match i {
            0 => d.value.clock = true,
            1 => d.value.research = true,
            2 => d.value.focus = true,
            3 => d.value.construction = true,
            4 => d.value.production = true,
            _ => d.unavailable = true,
        }
        assert!(execute(
            &mut j,
            "blocked",
            target(Stage::EnsureOpen),
            &authority(),
            &RwLock::new(true),
            &d,
            &mut f,
            TIME
        )
        .is_err());
        assert_eq!(f.observations, 0);
    }
}
#[test]
fn unresolved_domain_appearing_after_intent_records_no_input_refusal() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    let d = Domains {
        block_on: Some(4),
        ..Domains::default()
    };
    assert_eq!(
        execute(
            &mut j,
            "race",
            target(Stage::EnsureOpen),
            &authority(),
            &RwLock::new(true),
            &d,
            &mut f,
            TIME
        )
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    assert_eq!(f.dispatches, 0);
    assert_eq!(j.receipts["race"].status, Status::NotQueued);
}
#[test]
fn revoked_lease_and_closed_gate_refuse_before_observation() {
    for release in [true, false] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::new(Stage::EnsureOpen);
        let mut a = authority();
        if release {
            a.release(TOKEN, &epoch()).unwrap();
        }
        assert!(execute(
            &mut j,
            "closed",
            target(Stage::EnsureOpen),
            &a,
            &RwLock::new(release),
            &Domains::default(),
            &mut f,
            TIME
        )
        .is_err());
        assert_eq!(f.observations, 0);
    }
}
#[test]
fn original_deadline_is_not_extended_after_dispatch_or_input_idle() {
    for idle in [false, true] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let stage = Stage::EnsureOpen;
        let mut f = Fake::success(stage);
        f.exhaust_on_dispatch = !idle;
        f.exhaust_on_idle = idle;
        assert_eq!(
            execute(
                &mut j,
                "late",
                target(stage),
                &authority(),
                &RwLock::new(true),
                &Domains::default(),
                &mut f,
                Duration::from_millis(200)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert_eq!(f.possible_inputs, 1);
        assert_eq!(
            j.receipts["late"].uncertainty,
            Some(Uncertainty::DeadlineOrAuthority)
        );
    }
}
#[test]
fn canonical_readback_without_idle_confirmation_remains_uncertain() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    f.idle_error = true;
    assert_eq!(
        execute(
            &mut j,
            "held",
            target(Stage::EnsureOpen),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert!(j.receipts["held"]
        .observed
        .reached(&target(Stage::EnsureOpen)));
    assert_eq!(j.receipts["held"].uncertainty, Some(Uncertainty::InputIdle));
}
#[test]
fn torn_terminal_witness_is_reobserved_within_the_original_deadline() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    // Run090: the read after input-idle confirmation tore once.
    f.states[4] = Err(error(
        "torn_read",
        "Snapshot witness changed; section=choice; read=598; size=72; changed_byte=16",
    ));
    f.states
        .extend((0..2).map(|_| Ok(observation(reached(Stage::EnsureOpen), 1))));
    let e = execute(
        &mut j,
        "torn-terminal",
        target(Stage::EnsureOpen),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME,
    )
    .unwrap();
    assert_eq!(e.receipt.status, Status::Verified);
    assert_eq!(f.dispatches, 1);
    assert_eq!(f.idle_checks, 2);
}
#[test]
fn persistently_torn_terminal_witness_never_verifies() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    f.states.truncate(4);
    f.states.extend(
        std::iter::repeat_with(|| Err(error("torn_read", "Snapshot witness changed"))).take(4000),
    );
    assert_eq!(
        execute(
            &mut j,
            "always-torn",
            target(Stage::EnsureOpen),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            Duration::from_millis(300)
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(f.dispatches, 1);
    assert_eq!(
        j.receipts["always-torn"].uncertainty,
        Some(Uncertainty::DeadlineOrAuthority)
    );
}
#[test]
fn final_post_idle_scope_change_cannot_be_verified() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    f.states[4].as_mut().unwrap().scope = 2;
    assert_eq!(
        execute(
            &mut j,
            "idle-race",
            target(Stage::EnsureOpen),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(
        j.receipts["idle-race"].uncertainty,
        Some(Uncertainty::Scope)
    );
}
#[test]
fn candidate_cannot_assert_authority_or_escape_its_client_bounds() {
    for i in 0..5 {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::new(Stage::EnsureOpen);
        match i {
            0 => point_mut(&mut f.candidate).input_authorized = true,
            1 => point_mut(&mut f.candidate).current_frame_verified = true,
            2 => point_mut(&mut f.candidate).effective_routing_verified = true,
            3 => point_mut(&mut f.candidate).client_pixel[0] = 512,
            _ => point_mut(&mut f.candidate).expected_client[0] = 0,
        }
        assert_eq!(
            execute(
                &mut j,
                "point",
                target(Stage::EnsureOpen),
                &authority(),
                &RwLock::new(true),
                &Domains::default(),
                &mut f,
                TIME
            )
            .unwrap_err()
            .code,
            "construction_precondition"
        );
        assert!(j.receipts.is_empty());
        assert_eq!(f.dispatches, 0);
    }
}
#[test]
fn journal_has_exclusive_writer_and_refuses_partial_or_invalid_transition() {
    let s = Scratch::new();
    let j = Journal::open(&s.path()).unwrap();
    assert_eq!(Journal::open(&s.path()).err().unwrap().code, "journal_busy");
    drop(j);
    fs::write(s.path(), b"{\"partial\":true}").unwrap();
    assert_eq!(
        Journal::open(&s.path()).err().unwrap().code,
        "invalid_journal"
    );
    fs::write(s.path(), b"").unwrap();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::new(Stage::EnsureOpen);
    f.last.state = reached(Stage::EnsureOpen);
    execute(
        &mut j,
        "done",
        target(Stage::EnsureOpen),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME,
    )
    .unwrap();
    drop(j);
    let original = fs::read(s.path()).unwrap();
    let mut file = OpenOptions::new().append(true).open(s.path()).unwrap();
    file.write_all(&original).unwrap();
    drop(file);
    assert_eq!(
        Journal::open(&s.path()).err().unwrap().code,
        "invalid_journal"
    );
}
#[cfg(unix)]
#[test]
fn journal_refuses_symlink_hardlink_and_nonprivate_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let s = Scratch::new();
    let journal = Journal::open(&s.path()).unwrap();
    drop(journal);
    assert_eq!(
        fs::metadata(s.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let link = s.0.join("link");
    symlink(s.path(), &link).unwrap();
    assert!(Journal::open(&link).is_err());
    let hard = s.0.join("hard");
    fs::hard_link(s.path(), &hard).unwrap();
    assert_eq!(
        Journal::open(&s.path()).err().unwrap().code,
        "invalid_journal"
    );
    fs::remove_file(hard).unwrap();
    fs::set_permissions(s.path(), fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(
        Journal::open(&s.path()).err().unwrap().code,
        "invalid_journal"
    );
}
#[test]
fn insufficient_capacity_refuses_before_intent_and_possible_input() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    // The production file is append-only; Windows correctly rejects set_len.
    // Grow it through the locked writer to exercise the real metadata boundary.
    let length = LIMIT - (2 * LINE) as u64 + 1;
    std::io::copy(
        &mut std::io::Read::take(std::io::repeat(b' '), length),
        &mut j.file,
    )
    .unwrap();
    assert_eq!(j.file.metadata().unwrap().len(), length);
    let mut f = Fake::new(Stage::EnsureOpen);
    assert_eq!(
        execute(
            &mut j,
            "full",
            target(Stage::EnsureOpen),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME
        )
        .unwrap_err()
        .code,
        "journal_full"
    );
    assert!(j.receipts.is_empty());
    assert_eq!(f.dispatches, 0);
}
#[test]
fn failed_uncertainty_append_poison_preserves_durable_intent_across_restart() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let intent = j
        .append(Receipt {
            version: 1,
            request_id: "intent".into(),
            target: target(Stage::EnsureOpen),
            status: Status::Intent,
            delivery: None,
            observed: before(Stage::EnsureOpen),
            uncertainty: None,
            cause: None,
        })
        .unwrap();
    // Deliberately make the writer read-only only in this isolated fixture.
    j.file = File::open(s.path()).unwrap();
    assert_eq!(
        j.uncertain(
            TOKEN,
            &intent,
            Delivery::Unknown,
            reached(Stage::EnsureOpen),
            Uncertainty::Observation,
            error("io_error", "lost readback")
        )
        .code,
        "action_uncertain"
    );
    assert!(j.poisoned && j.blocks_game(&epoch().game));
    drop(j);
    let j = Journal::open(&s.path()).unwrap();
    assert_eq!(j.receipts["intent"].status, Status::Intent);
    assert!(j.blocks_game(&epoch().game));
}

#[test]
fn same_request_id_cannot_change_stage_or_canonical_target_even_after_noop() {
    for stage in [Stage::EnsureOpen, Stage::SelectBuilding] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::new(stage);
        f.last.state = reached(stage);
        let a = authority();
        let gate = RwLock::new(true);
        let domains = Domains::default();
        execute(
            &mut j,
            "bound",
            target(stage),
            &a,
            &gate,
            &domains,
            &mut f,
            TIME,
        )
        .unwrap();
        let mut replacement = target(Stage::SelectBuilding);
        if stage == Stage::SelectBuilding {
            replacement.building_id = Some("arms_factory".into());
        }
        let mut retry = Fake::new(Stage::SelectBuilding);
        assert_eq!(
            execute(
                &mut j,
                "bound",
                replacement,
                &a,
                &gate,
                &domains,
                &mut retry,
                TIME
            )
            .unwrap_err()
            .code,
            "request_id_conflict"
        );
        assert_eq!((retry.observations, retry.dispatches), (0, 0));
    }
}

#[test]
fn unsupported_mode_cannot_smuggle_selection_payload_into_journal() {
    let mut state = before(Stage::SelectBuilding);
    state.selected_mode = Some(3);
    state.selected_building = Some("dockyard".into());
    assert_preinput_refusal(Stage::SelectBuilding, state);
    let mut state = before(Stage::SelectBuilding);
    state.epoch.game.start_ticks = "replaced-process".into();
    assert_preinput_refusal(Stage::SelectBuilding, state);
}

#[test]
fn uncertain_terminal_record_cannot_be_rewritten_to_verified_on_reopen() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::EnsureOpen;
    let intent = j
        .append(Receipt {
            version: 1,
            request_id: "once".into(),
            target: target(stage),
            status: Status::Intent,
            delivery: None,
            observed: before(stage),
            uncertainty: None,
            cause: None,
        })
        .unwrap();
    let terminal = Receipt {
        status: Status::Uncertain,
        delivery: Some(Delivery::Unknown),
        observed: reached(stage),
        uncertainty: Some(Uncertainty::InputIdle),
        cause: Some(Diagnostic::from_error(
            &error("input_unavailable", "Held input unverified"),
            TOKEN,
        )),
        ..intent
    };
    j.append(terminal.clone()).unwrap();
    drop(j);
    let forged = Receipt {
        status: Status::Verified,
        uncertainty: None,
        cause: None,
        ..terminal
    };
    let mut file = OpenOptions::new().append(true).open(s.path()).unwrap();
    let mut line = serde_json::to_vec(&forged).unwrap();
    line.push(b'\n');
    file.write_all(&line).unwrap();
    file.sync_all().unwrap();
    drop(file);
    assert_eq!(
        Journal::open(&s.path()).err().unwrap().code,
        "invalid_journal"
    );
}

#[test]
fn not_queued_keeps_actual_backend_predicate_on_disk_and_historical_error() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::EnsureOpen;
    let mut f = Fake::success(stage);
    f.delivery = Delivery::NotQueued;
    let a = authority();
    let gate = RwLock::new(true);
    let domains = Domains::default();
    let err = execute(
        &mut j,
        "reason",
        target(stage),
        &a,
        &gate,
        &domains,
        &mut f,
        TIME,
    )
    .unwrap_err();
    assert!(err
        .message
        .contains("cause=input_geometry_changed: Final owned control geometry changed"));
    assert_eq!(
        j.receipts["reason"].cause.as_ref().unwrap().code,
        "input_geometry_changed"
    );
    drop(j);
    let mut j = Journal::open(&s.path()).unwrap();
    let mut retry = Fake::new(stage);
    let err = execute(
        &mut j,
        "reason",
        target(stage),
        &a,
        &gate,
        &domains,
        &mut retry,
        TIME,
    )
    .unwrap_err();
    assert_eq!(err.code, "input_not_queued");
    assert!(err.message.contains("Final owned control geometry changed"));
    assert_eq!(retry.observations, 0);
}

#[test]
fn final_readonly_refusal_keeps_exact_producer_cause_with_no_dispatch() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::EnsureOpen;
    let mut f = Fake::success(stage);
    f.states[2] = Err(error(
        "construction_owner_predicate",
        "Named opener container belongs to another registry",
    ));
    let err = execute(
        &mut j,
        "final-refusal",
        target(stage),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME,
    )
    .unwrap_err();
    assert_eq!(err.code, "input_not_queued");
    assert!(err.message.contains(
        "cause=construction_owner_predicate: Named opener container belongs to another registry"
    ));
    let r = &j.receipts["final-refusal"];
    assert_eq!(
        r.cause.as_ref().unwrap().code,
        "construction_owner_predicate"
    );
    assert_eq!(f.dispatches, 0);
}

#[test]
fn uncertain_readback_keeps_exact_bounded_sanitized_producer_cause_on_restart() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let stage = Stage::SelectBuilding;
    let mut f = Fake::success(stage);
    let message = format!(
        "read denied\n pointer=0x7fff12345678 other=7fff12345678 lease=prefix{}suffix {}",
        TOKEN.to_uppercase(),
        "é".repeat(600)
    );
    f.states[3] = Err(Error::new("permission_denied", message));
    let a = authority();
    let gate = RwLock::new(true);
    let domains = Domains::default();
    let err = execute(
        &mut j,
        "diagnostic",
        target(stage),
        &a,
        &gate,
        &domains,
        &mut f,
        TIME,
    )
    .unwrap_err();
    let d = j.receipts["diagnostic"].cause.as_ref().unwrap();
    assert_eq!(d.code, "permission_denied");
    assert!(d.message.starts_with("read denied pointer=[redacted]"));
    assert!(d.message.len() <= 512 && !d.message.chars().any(char::is_control));
    drop(j);
    let data = fs::read_to_string(s.path()).unwrap();
    for prohibited in [
        TOKEN,
        &TOKEN.to_uppercase(),
        "0x7fff12345678",
        "7fff12345678",
    ] {
        assert!(!data.contains(prohibited));
        assert!(!err.message.contains(prohibited));
    }
    assert!(err.message.contains("cause=permission_denied:"));
    let mut j = Journal::open(&s.path()).unwrap();
    let mut retry = Fake::new(stage);
    let err = execute(
        &mut j,
        "diagnostic",
        target(stage),
        &a,
        &gate,
        &domains,
        &mut retry,
        TIME,
    )
    .unwrap_err();
    assert!(err.message.contains("cause=permission_denied:"));
    assert_eq!(retry.observations, 0);
}

#[test]
fn explicit_ambiguous_dispatch_error_stops_without_extra_observation_and_retains_cause() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    f.dispatch_error = true;
    let err = execute(
        &mut j,
        "ambiguous",
        target(Stage::EnsureOpen),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME,
    )
    .unwrap_err();
    assert_eq!(f.observations, 3);
    assert_eq!(f.possible_inputs, 1);
    assert_eq!(err.code, "action_uncertain");
    assert!(err.message.contains("cause=transport_lost:"));
    assert_eq!(
        j.receipts["ambiguous"].uncertainty,
        Some(Uncertainty::Dispatch)
    );
    assert!(j.blocks_game(&epoch().game));
}

#[test]
fn diagnostic_redacts_before_utf8_byte_bound_including_embedded_token() {
    let d = Diagnostic::from_error(
        &Error::new(
            "producer_reason",
            format!("{}prefix{}suffix", "é".repeat(245), TOKEN),
        ),
        TOKEN,
    );
    assert!(d.message.len() <= 512);
    assert!(!d.message.contains(TOKEN));
    assert!(!d.message.contains("aaaa"));
    assert!(d.validate());
}

#[test]
fn failure_evidence_never_downgrades_unresolved_intent_or_terminal_uncertainty() {
    for (delivery, dispatch_error, expected) in [
        (
            Delivery::NotQueued,
            false,
            FailureEvidence::ConfirmedNotQueued,
        ),
        (Delivery::Unknown, true, FailureEvidence::Unknown),
    ] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let t = target(Stage::EnsureOpen);
        assert_eq!(
            j.failure_evidence("none", &t, &error("torn_read", "before intent")),
            FailureEvidence::BeforeIntent
        );
        let mut f = Fake::success(Stage::EnsureOpen);
        f.delivery = delivery;
        f.dispatch_error = dispatch_error;
        let err = execute(
            &mut j,
            "attempt",
            t.clone(),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME,
        )
        .unwrap_err();
        assert_eq!(j.failure_evidence("attempt", &t, &err), expected);
        if expected == FailureEvidence::Unknown {
            assert_eq!(
                j.failure_evidence(
                    "different-id",
                    &t,
                    &error("torn_read", "retry-looking cause")
                ),
                FailureEvidence::Unknown
            );
        }
    }
}

fn find_input(stage: Stage) -> Input {
    match stage {
        Stage::FindOpen => Input::FindKey,
        Stage::FindQuery => Input::Digits("64".into()),
        _ => Input::Return,
    }
}
#[test]
fn find_stages_send_one_fixed_keyboard_input_after_durable_intent() {
    for stage in [Stage::FindOpen, Stage::FindQuery, Stage::FindActivate] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::success(stage);
        f.candidate.input = find_input(stage);
        f.delivery = Delivery::ServerConfirmed;
        let out = execute(
            &mut j,
            "find-1",
            target(stage),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME,
        )
        .unwrap();
        assert_eq!((f.dispatches, f.possible_inputs), (1, 1), "{stage:?}");
        assert_eq!(out.receipt.status, Status::Verified);
        assert_eq!(out.receipt.target.state_id, target(stage).state_id);
        assert!(out.receipt.observed.find.is_some());
        drop(j);
        let lines: Vec<Receipt> = fs::read_to_string(s.path())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            lines.iter().map(|r| r.status).collect::<Vec<_>>(),
            [Status::Intent, Status::Verified]
        );
    }
}
#[test]
fn find_stages_refuse_mismatched_keyboard_or_pointer_input_before_intent() {
    for stage in [Stage::FindOpen, Stage::FindQuery, Stage::FindActivate] {
        for input in [
            Input::Point(point()),
            Input::FindKey,
            Input::Return,
            Input::Digits("65".into()),
            Input::Digits("64".into()),
        ] {
            if input == find_input(stage) {
                continue;
            }
            let s = Scratch::new();
            let mut j = Journal::open(&s.path()).unwrap();
            let mut f = Fake::success(stage);
            f.candidate.input = input.clone();
            let error = execute(
                &mut j,
                "find-x",
                target(stage),
                &authority(),
                &RwLock::new(true),
                &Domains::default(),
                &mut f,
                TIME,
            )
            .unwrap_err();
            assert_eq!(
                error.code, "construction_precondition",
                "{stage:?} {input:?}"
            );
            assert_eq!(f.dispatches, 0);
            assert!(!j.blocks_game(&epoch().game));
        }
    }
    // Pointer stages never accept keyboard candidates either.
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::EnsureOpen);
    f.candidate.input = Input::FindKey;
    assert!(execute(
        &mut j,
        "open-key",
        target(Stage::EnsureOpen),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME
    )
    .is_err());
    assert_eq!(f.dispatches, 0);
}
#[test]
fn find_preconditions_and_completion_are_exact() {
    // Open needs the view positively hidden; query needs focused empty query and own state.
    let open = target(Stage::FindOpen);
    let mut state = before(Stage::FindOpen);
    assert!(state.preflight(&open).is_ok() && !state.reached(&open));
    state.find.as_mut().unwrap().visible = None;
    assert!(state.preflight(&open).is_err());
    let query = target(Stage::FindQuery);
    let mut state = before(Stage::FindQuery);
    assert!(state.preflight(&query).is_ok());
    for change in 0..3 {
        let mut changed = state.clone();
        let f = changed.find.as_mut().unwrap();
        match change {
            0 => f.query = Some("6".into()),
            1 => f.text_focused = Some(false),
            _ => f.target_owned = Some(false),
        }
        assert!(changed.preflight(&query).is_err(), "{change}");
    }
    // Query completion is its own effect: exact digits AND a refreshed first row
    // of any kind. Which row comes first only gates FindActivate.
    state.find.as_mut().unwrap().query = Some("64".into());
    assert!(!state.reached(&query));
    state.find.as_mut().unwrap().first_kind = Some(3);
    assert!(state.reached(&query));
    state.find.as_mut().unwrap().query = Some("640".into());
    assert!(!state.reached(&query));
    // Activate completes only on a closed view with the camera on the own state.
    let activate = target(Stage::FindActivate);
    let mut state = before(Stage::FindActivate);
    assert!(state.preflight(&activate).is_ok());
    state.find.as_mut().unwrap().first_state_id = Some(65);
    assert!(state.preflight(&activate).is_err());
    // A province (or other kind) first row refuses before Return.
    let mut province_first = before(Stage::FindActivate);
    province_first.find.as_mut().unwrap().first_kind = Some(3);
    province_first.find.as_mut().unwrap().first_state_id = None;
    assert!(province_first.preflight(&activate).is_err());
    let mut done = reached(Stage::FindActivate);
    assert!(done.reached(&activate));
    done.find.as_mut().unwrap().camera_centered = Some(false);
    assert!(!done.reached(&activate));
    // Find facts on pointer stages, or missing on Find stages, are invalid.
    let mut pointer = before(Stage::EnsureOpen);
    pointer.find = find_before(Stage::FindOpen);
    assert!(pointer.validate(&target(Stage::EnsureOpen)).is_err());
    let mut missing = before(Stage::FindOpen);
    missing.find = None;
    assert!(missing.validate(&open).is_err());
}
#[test]
fn find_target_shapes_and_legacy_receipts_stay_compatible() {
    let mut t = target(Stage::FindQuery);
    t.state_id = None;
    assert!(t.validate().is_err());
    for id in [0, 4096, -1] {
        t.state_id = Some(id);
        assert!(t.validate().is_err(), "{id}");
    }
    let mut open = target(Stage::FindOpen);
    open.state_id = Some(64);
    assert!(open.validate().is_err());
    let mut select = target(Stage::SelectBuilding);
    select.state_id = Some(64);
    assert!(select.validate().is_err());
    // Legacy pointer receipts serialize exactly as before: no state_id or find keys.
    let text = serde_json::to_string(&target(Stage::EnsureOpen)).unwrap();
    assert!(!text.contains("state_id"));
    let state = serde_json::to_string(&before(Stage::EnsureOpen)).unwrap();
    assert!(!state.contains("find"));
    let parsed: Target = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed, target(Stage::EnsureOpen));
}

#[test]
fn place_clicks_once_and_completes_only_at_the_exact_target_amount() {
    let place = target(Stage::Place);
    let state = before(Stage::Place);
    assert!(state.preflight(&place).is_ok() && !state.reached(&place));
    assert!(reached(Stage::Place).reached(&place));
    // Wrong total, incomplete queue, camera away, open Find view or other
    // selection refuse before input.
    for change in 0..6 {
        let mut s = before(Stage::Place);
        match change {
            0 => s.placement.as_mut().unwrap().in_state = 0,
            1 => s.placement.as_mut().unwrap().complete = false,
            2 => s.find.as_mut().unwrap().camera_centered = Some(false),
            3 => s.find.as_mut().unwrap().visible = Some(true),
            4 => s.selected_building = Some("arms_factory".into()),
            _ => s.panel = PanelState::Hidden,
        }
        assert!(s.preflight(&place).is_err(), "{change}");
    }
    // One pointer input through the durable journal; keyboard inputs refuse.
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::success(Stage::Place);
    f.delivery = Delivery::ServerConfirmed;
    let out = execute(
        &mut j,
        "place-1",
        place.clone(),
        &authority(),
        &RwLock::new(true),
        &Domains::default(),
        &mut f,
        TIME,
    )
    .unwrap();
    assert_eq!((f.dispatches, f.possible_inputs), (1, 1));
    assert_eq!(out.receipt.status, Status::Verified);
    assert_eq!(out.receipt.observed.placement.as_ref().unwrap().in_state, 2);
    for input in [Input::FindKey, Input::Return, Input::Digits("64".into())] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::success(Stage::Place);
        f.candidate.input = input;
        assert!(execute(
            &mut j,
            "place-k",
            place.clone(),
            &authority(),
            &RwLock::new(true),
            &Domains::default(),
            &mut f,
            TIME
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
    }
}
#[test]
fn place_target_shape_requires_building_state_and_bounded_amount() {
    let mut t = target(Stage::Place);
    assert!(t.validate().is_ok());
    for amount in [None, Some(0), Some(1001)] {
        t.amount = amount;
        assert!(t.validate().is_err(), "{amount:?}");
    }
    let mut t = target(Stage::Place);
    t.building_id = Some("rail_way".into());
    assert!(t.validate().is_err());
    let mut t = target(Stage::Place);
    t.state_id = None;
    assert!(t.validate().is_err());
    let mut t = target(Stage::EnsureOpen);
    t.amount = Some(1);
    assert!(t.validate().is_err());
    // Placement facts only on Place.
    let mut s = before(Stage::SelectBuilding);
    s.placement = before(Stage::Place).placement;
    assert!(s.validate(&target(Stage::SelectBuilding)).is_err());
}
