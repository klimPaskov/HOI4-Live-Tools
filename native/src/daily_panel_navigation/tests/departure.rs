use super::*;
use crate::{
    construction_stage::{DispatchOutcome, FailureEvidence},
    daily_governor::Governor,
    input_lease::InputAuthority,
    production_step as ordinary, research_stage as research_adapter,
};
use std::{
    sync::RwLock,
    time::{Duration, Instant},
};

const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn production_panel(mode: i32, shown: bool) -> PanelState {
    PanelState {
        epoch: scope().epoch,
        panel_identity: vec![
            0x40000, 0xb0000, 0x20000, 0x60000, 0x70000, 0xe0000, 0xf0000,
        ],
        managed_mode: mode,
        panel_shown: shown,
    }
}
fn production_destination() -> Destination {
    Destination::Production {
        state: FactoryState {
            epoch: scope().epoch,
            line: ordinary::LineIdentity {
                country: scope().own_country,
                production: 0xb0000,
                line: 0x60000,
                variant: 0x70000,
                base_type: 0x80000,
                handle: 0x0000000100000038,
                equipment_id: "infantry_equipment_1".into(),
            },
            requested: 10,
        },
        panel: production_panel(5, false),
    }
}
fn research_source() -> Source {
    let Destination::Research { mut state } = original() else {
        unreachable!()
    };
    state.active_mode = Some(5);
    state.panel_visible = Some(true);
    Source::Research { state }
}
fn source_production() -> Source {
    Source::Production {
        state: production_panel(2, true),
    }
}
fn closed_destination(mut destination: Destination) -> Destination {
    match &mut destination {
        Destination::Research { state } => state.active_mode = Some(23),
        Destination::Production { panel, .. } => panel.managed_mode = 23,
    }
    destination
}
fn completed_research() -> Destination {
    let Destination::Research { mut state } = original() else {
        unreachable!()
    };
    state.researching[0] = Some("basic_machine_tools".into());
    state.active_mode = Some(5);
    state.panel_visible = Some(true);
    Destination::Research { state }
}
fn setup(t: &Temp, entry: &Entry, destination: Destination, limit: u8) -> (Navigator, Governor) {
    let mut governor = Governor::open(&t.0.join("governor.json"), plan()).unwrap();
    let mut navigator = Navigator::open(&t.file()).unwrap();
    navigator.configure(&plan(), Some(limit)).unwrap();
    navigator
        .select(&mut governor, entry, destination, date())
        .unwrap();
    (navigator, governor)
}
fn research_execution(id: &str) -> research_adapter::Execution {
    let Source::Research { mut state } = research_source() else {
        unreachable!()
    };
    state.active_mode = Some(23);
    state.panel_visible = Some(false);
    research_adapter::Execution {
        receipt: research_adapter::Receipt {
            version: 1,
            request_id: id.into(),
            target: research_adapter::Target {
                epoch: scope().epoch,
                stage: research_adapter::Stage::ClosePanel,
                technology: String::new(),
                folder_id: None,
                slot: 0,
            },
            status: research_adapter::Status::Verified,
            delivery: Some(research_adapter::Delivery::ServerConfirmed),
            observed: state,
        },
        historical: false,
        current_state_verified: true,
        event_causality_verified: false,
    }
}

#[test]
fn reservation_is_durable_consumes_budget_and_quarantines_restart() {
    let t = Temp::new();
    let (mut n, mut governor) = setup(&t, &research(), original(), 2);
    let id = n
        .begin_departure(source_production(), &original(), date())
        .unwrap();
    assert_eq!(id.len(), 64);
    assert!(n.blocked());
    assert_eq!(n.used_steps(day(date()).unwrap()), 1);
    let before = fs::read(t.file()).unwrap();
    assert!(n
        .begin_departure(source_production(), &original(), date())
        .is_err());
    assert!(n.finish(&mut governor, &original(), date()).is_err());
    drop(n);
    let mut n = Navigator::open(&t.file()).unwrap();
    assert!(n.blocked());
    assert_eq!(n.active(), Some(&research()));
    assert_eq!(n.used_steps(day(date()).unwrap()), 1);
    for limit in [None, Some(1), Some(2), Some(3)] {
        assert!(n.configure(&plan(), limit).is_err());
    }
    assert!(n
        .begin_departure(source_production(), &original(), date() + 24)
        .is_err());
    assert_eq!(fs::read(t.file()).unwrap(), before);
}

#[test]
fn known_and_unknown_failures_keep_original_cause_and_never_retry() {
    for evidence in [
        FailureEvidence::BeforeIntent,
        FailureEvidence::ConfirmedNotQueued,
        FailureEvidence::Unknown,
    ] {
        let t = Temp::new();
        let destination = production_destination();
        let (mut n, _) = setup(&t, &production(), destination.clone(), 2);
        let id = n
            .begin_departure(research_source(), &destination, date())
            .unwrap();
        let error = Error::new("fixture_refusal", format!("original cause {TOKEN}"));
        n.failed_departure(&id, &error, evidence, TOKEN).unwrap();
        assert_eq!(n.blocked(), evidence == FailureEvidence::Unknown);
        let before = fs::read(t.file()).unwrap();
        assert!(!String::from_utf8(before.clone()).unwrap().contains(TOKEN));
        assert!(n
            .failed_departure(&id, &Error::new("replacement", "changed"), evidence, TOKEN)
            .is_err());
        assert!(n
            .verified_research(
                &id,
                &research_execution(&id),
                closed_destination(destination.clone()),
                date()
            )
            .is_err());
        assert!(n
            .begin_departure(research_source(), &destination, date() + 24)
            .is_err());
        drop(n);
        let mut n = Navigator::open(&t.file()).unwrap();
        assert_eq!(n.blocked(), evidence == FailureEvidence::Unknown);
        assert_eq!(n.used_steps(day(date()).unwrap()), 1);
        assert_eq!(
            n.summary()["departures"]["latest"]["cause"]["code"],
            "fixture_refusal"
        );
        assert!(n.configure(&plan(), Some(2)).is_err());
        assert!(n
            .begin_departure(research_source(), &destination, date() + 24)
            .is_err());
        assert_eq!(fs::read(t.file()).unwrap(), before);
    }
}

#[test]
fn changed_source_destination_bindings_or_date_do_not_reserve_or_poison() {
    for mutation in 0..9 {
        let t = Temp::new();
        let (mut n, _) = setup(&t, &research(), original(), 2);
        let before = fs::read(t.file()).unwrap();
        let mut source = source_production();
        let mut destination = original();
        let mut at = date();
        match mutation {
            0 => {
                let Source::Production { state } = &mut source else {
                    unreachable!()
                };
                state.panel_identity[0] += 8;
            }
            1 => {
                let Source::Production { state } = &mut source else {
                    unreachable!()
                };
                state.panel_identity[2] += 8;
            }
            2 => {
                let Source::Production { state } = &mut source else {
                    unreachable!()
                };
                state.panel_identity[3] += 8;
            }
            3 => {
                let Source::Production { state } = &mut source else {
                    unreachable!()
                };
                state.managed_mode = 23;
                state.panel_shown = false;
            }
            4 => {
                let Destination::Research { state } = &mut destination else {
                    unreachable!()
                };
                state.panel_binding.as_mut().unwrap().slots[0] += 8;
            }
            5 => {
                let Destination::Research { state } = &mut destination else {
                    unreachable!()
                };
                state.researching[0] = Some("support_weapons".into());
            }
            6 => {
                let Destination::Research { state } = &mut destination else {
                    unreachable!()
                };
                state.active_mode = Some(23);
            }
            7 => at -= 1,
            _ => source = research_source(),
        }
        assert!(
            n.begin_departure(source, &destination, at).is_err(),
            "mutation {mutation}"
        );
        assert_eq!(n.used_steps(day(date()).unwrap()), 0);
        assert!(!n.blocked());
        assert_eq!(fs::read(t.file()).unwrap(), before);
        n.begin_departure(source_production(), &original(), date())
            .unwrap();
    }
}

#[test]
fn research_departure_requires_exact_shown_own_panel_without_tree_or_info() {
    for mutation in 0..8 {
        let t = Temp::new();
        let destination = production_destination();
        let (mut n, _) = setup(&t, &production(), destination.clone(), 2);
        let before = fs::read(t.file()).unwrap();
        let Source::Research { mut state } = research_source() else {
            unreachable!()
        };
        match mutation {
            0 => state.active_mode = Some(23),
            1 => state.panel_visible = Some(false),
            2 => state.tree_visible = true,
            3 => state.visible_info = vec![Some("support_weapons".into())],
            4 => state.panel_binding.as_mut().unwrap().tab = 1,
            5 => state.panel_binding.as_mut().unwrap().anchors[1] += 8,
            6 => state.panel_binding.as_mut().unwrap().anchors[2] += 8,
            _ => state.panel_binding.as_mut().unwrap().selected_slot_pointer = 0xa0000,
        }
        assert!(
            n.begin_departure(Source::Research { state }, &destination, date())
                .is_err(),
            "mutation {mutation}"
        );
        assert!(!n.blocked());
        assert_eq!(n.used_steps(day(date()).unwrap()), 0);
        assert_eq!(fs::read(t.file()).unwrap(), before);
        n.begin_departure(research_source(), &destination, date())
            .unwrap();
    }
}

#[test]
fn research_proof_rejects_replay_wrong_id_state_destination_and_chronology() {
    let t = Temp::new();
    let destination = production_destination();
    let (mut n, mut governor) = setup(&t, &production(), destination.clone(), 2);
    let id = n
        .begin_departure(research_source(), &destination, date())
        .unwrap();
    let before = fs::read(t.file()).unwrap();
    for mutation in 0..12 {
        let mut execution = research_execution(&id);
        let mut after = closed_destination(destination.clone());
        let mut at = date();
        match mutation {
            0 => execution.historical = true,
            1 => execution.current_state_verified = false,
            2 => execution.receipt.request_id = "0".repeat(64),
            3 => execution.receipt.target.stage = research_adapter::Stage::OpenPanel,
            4 => execution.receipt.status = research_adapter::Status::Intent,
            5 => execution.receipt.delivery = None,
            6 => execution.receipt.delivery = Some(research_adapter::Delivery::NotQueued),
            7 => {
                execution
                    .receipt
                    .observed
                    .panel_binding
                    .as_mut()
                    .unwrap()
                    .slots[0] += 8
            }
            8 => execution.receipt.observed.researching[0] = Some("support_weapons".into()),
            9 => execution.receipt.observed.active_mode = Some(5),
            10 => {
                let Destination::Production { state, .. } = &mut after else {
                    unreachable!()
                };
                state.requested += 1;
            }
            _ => at -= 1,
        }
        assert!(
            n.verified_research(&id, &execution, after, at).is_err(),
            "mutation {mutation}"
        );
        assert!(n.blocked());
        assert_eq!(fs::read(t.file()).unwrap(), before);
    }
    let mut execution = research_execution(&id);
    execution.receipt.delivery = Some(research_adapter::Delivery::Unknown);
    n.verified_research(
        &id,
        &execution,
        closed_destination(destination.clone()),
        date(),
    )
    .unwrap();
    assert!(!n.blocked());
    assert_eq!(n.active(), Some(&production()));
    assert!(
        n.finish(&mut governor, &destination, date()).is_err(),
        "close must not complete allocation"
    );
    assert!(n
        .verified_research(
            &id,
            &execution,
            closed_destination(destination.clone()),
            date()
        )
        .is_err());
    assert!(n
        .begin_departure(research_source(), &destination, date() + 24)
        .is_err());
    drop(n);
    let n = Navigator::open(&t.file()).unwrap();
    assert!(!n.blocked());
    assert_eq!(n.active(), Some(&production()));
    assert_eq!(n.used_steps(day(date()).unwrap()), 1);
}

#[test]
fn reopened_verified_history_revalidates_original_identity_proof_and_dates() {
    for mutation in 0..6 {
        let t = Temp::new();
        let destination = production_destination();
        let (mut n, _) = setup(&t, &production(), destination.clone(), 2);
        let id = n
            .begin_departure(research_source(), &destination, date())
            .unwrap();
        n.verified_research(
            &id,
            &research_execution(&id),
            closed_destination(destination),
            date(),
        )
        .unwrap();
        drop(n);
        let mut journal: Value = serde_json::from_slice(&fs::read(t.file()).unwrap()).unwrap();
        let step = &mut journal["departures"][0];
        match mutation {
            0 => {
                step["status"]["proof"]["receipt"]["observed"]["panel_binding"]["slots"][0] =
                    json!(0xa0008)
            }
            1 => step["status"]["proof"]["receipt"]["request_id"] = json!("0".repeat(64)),
            2 => step["status"]["proof"]["receipt"]["delivery"] = json!("NotQueued"),
            3 => step["status"]["destination"]["state"]["requested"] = json!(11),
            4 => step["status"]["date"] = json!(date() - 1),
            _ => step["day"] = json!(day(date()).unwrap() + 1),
        }
        let changed = serde_json::to_vec(&journal).unwrap();
        fs::write(t.file(), &changed).unwrap();
        assert!(Navigator::open(&t.file()).is_err(), "mutation {mutation}");
        assert_eq!(fs::read(t.file()).unwrap(), changed);
    }
}

struct Clear;
impl ordinary::UnresolvedInputs for Clear {
    fn clear(&self, _: &Identity) -> Result<()> {
        Ok(())
    }
}
struct ClosingBackend {
    state: PanelState,
    dispatches: usize,
    navigation_path: std::path::PathBuf,
}
impl ordinary::Backend for ClosingBackend {
    type Candidate = ();
    fn observe(&mut self, _: &ordinary::Target, _: Instant) -> Result<ordinary::State> {
        Ok(ordinary::State::Panel(self.state.clone()))
    }
    fn candidate(
        &mut self,
        _: &ordinary::Target,
        before: &ordinary::State,
        _: Instant,
    ) -> Result<()> {
        assert_eq!(before, &ordinary::State::Panel(self.state.clone()));
        Ok(())
    }
    fn dispatch_once(
        &mut self,
        _: &ordinary::Target,
        _: &ordinary::State,
        _: &(),
        _: Instant,
    ) -> DispatchOutcome {
        let data: Value =
            serde_json::from_slice(&fs::read(&self.navigation_path).unwrap()).unwrap();
        assert_eq!(data["departures"].as_array().unwrap().len(), 1);
        assert_eq!(data["departures"][0]["status"]["status"], "Intent");
        self.dispatches += 1;
        self.state.managed_mode = 23;
        self.state.panel_shown = false;
        DispatchOutcome::ServerConfirmed
    }
    fn confirm_input_idle(&mut self, _: Instant) -> Result<()> {
        Ok(())
    }
}
fn close_production(
    t: &Temp,
    id: &str,
) -> (
    ordinary::Journal,
    ClosingBackend,
    InputAuthority,
    ordinary::Execution,
) {
    let mut journal = ordinary::Journal::open(&t.0.join("production.jsonl")).unwrap();
    let mut backend = ClosingBackend {
        state: production_panel(2, true),
        dispatches: 0,
        navigation_path: t.file(),
    };
    let mut authority = InputAuthority::new("player".into(), scope().epoch).unwrap();
    authority
        .acquire("player", TOKEN.into(), 900, &scope().epoch)
        .unwrap();
    let execution = journal
        .execute(
            id,
            ordinary::Target::Panel(ordinary::PanelTarget::closed(scope().epoch)),
            TOKEN,
            &authority,
            &RwLock::new(true),
            &Clear,
            &mut backend,
            Duration::from_secs(2),
        )
        .unwrap();
    (journal, backend, authority, execution)
}

#[test]
fn opaque_production_proof_ignores_mutable_public_summary_and_rejects_historical_replay() {
    let t = Temp::new();
    let (mut n, mut governor) = setup(&t, &research(), original(), 2);
    let id = n
        .begin_departure(source_production(), &original(), date())
        .unwrap();
    let (mut journal, mut backend, authority, mut execution) = close_production(&t, &id);
    assert_eq!(backend.dispatches, 1);
    let historical = journal
        .execute(
            &id,
            ordinary::Target::Panel(ordinary::PanelTarget::closed(scope().epoch)),
            TOKEN,
            &authority,
            &RwLock::new(true),
            &Clear,
            &mut backend,
            Duration::from_secs(2),
        )
        .unwrap();
    assert!(historical.historical);
    assert!(n
        .verified_production(&id, &historical, closed_destination(original()), date())
        .is_err());
    let mut changed = closed_destination(original());
    let Destination::Research { state } = &mut changed else {
        unreachable!()
    };
    state.panel_binding.as_mut().unwrap().slots[0] += 8;
    assert!(n
        .verified_production(&id, &execution, changed, date())
        .is_err());
    assert!(n
        .verified_production(&id, &execution, closed_destination(original()), date() - 1)
        .is_err());
    assert!(n
        .verified_production(
            &"0".repeat(64),
            &execution,
            closed_destination(original()),
            date()
        )
        .is_err());
    execution.request_id = "public-summary-cannot-rebind-proof".into();
    execution.status = ordinary::Status::Uncertain;
    execution.historical = true;
    execution.current_state_verified = false;
    n.verified_production(&id, &execution, closed_destination(original()), date())
        .unwrap();
    assert_eq!(backend.dispatches, 1);
    assert!(!n.blocked());
    assert_eq!(n.active(), Some(&research()));
    assert!(n.finish(&mut governor, &original(), date()).is_err());
    drop(n);
    let n = Navigator::open(&t.file()).unwrap();
    assert_eq!(n.summary()["departures"]["latest"]["status"], "Verified");
    assert_eq!(n.used_steps(day(date()).unwrap()), 1);
}

#[test]
fn consumed_day_budget_survives_completed_selection_and_restart() {
    let t = Temp::new();
    let (mut n, mut governor) = setup(&t, &research(), original(), 1);
    let id = n
        .begin_departure(source_production(), &original(), date())
        .unwrap();
    let (_, backend, _, execution) = close_production(&t, &id);
    assert_eq!(backend.dispatches, 1);
    n.verified_production(&id, &execution, closed_destination(original()), date())
        .unwrap();
    governor
        .continue_entry(
            &mut Fixture(0),
            &research().id,
            1000,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
    n.finish(&mut governor, &completed_research(), date())
        .unwrap();
    n.select(
        &mut governor,
        &production(),
        production_destination(),
        date(),
    )
    .unwrap();
    drop(n);
    let mut n = Navigator::open(&t.file()).unwrap();
    let before = fs::read(t.file()).unwrap();
    assert_eq!(n.used_steps(day(date()).unwrap()), 1);
    assert!(n
        .begin_departure(research_source(), &production_destination(), date())
        .is_err());
    assert!(!n.blocked());
    assert_eq!(fs::read(t.file()).unwrap(), before);
    n.begin_departure(research_source(), &production_destination(), date() + 24)
        .unwrap();
    assert_eq!(n.used_steps(day(date()).unwrap()), 1);
    assert_eq!(n.used_steps(day(date() + 24).unwrap()), 1);
    drop(n);
    let n = Navigator::open(&t.file()).unwrap();
    assert!(n.blocked());
    assert_eq!(n.active(), Some(&production()));
}

#[test]
fn historical_governor_completion_cannot_select_a_new_unsatisfied_destination() {
    let t = Temp::new();
    let mut governor = Governor::open(&t.0.join("governor.json"), plan()).unwrap();
    governor
        .continue_entry(
            &mut Fixture(0),
            &research().id,
            1000,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
    let mut n = Navigator::open(&t.file()).unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    let before = fs::read(t.file()).unwrap();
    assert!(n
        .select(&mut governor, &research(), original(), date())
        .is_err());
    assert!(n.active().is_none());
    assert!(!n.blocked());
    assert_eq!(fs::read(t.file()).unwrap(), before);
}

#[test]
fn continuation_rechecks_original_static_identity_after_restart() {
    for production_domain in [false, true] {
        let t = Temp::new();
        let (entry, destination) = if production_domain {
            (production(), production_destination())
        } else {
            (research(), original())
        };
        let (n, _) = setup(&t, &entry, destination.clone(), 2);
        drop(n);
        let mut n = Navigator::open(&t.file()).unwrap();
        let before = fs::read(t.file()).unwrap();
        n.verify_current(&destination, date()).unwrap();
        for mutation in 0..5 {
            let mut changed = destination.clone();
            match &mut changed {
                Destination::Research { state } => match mutation {
                    0 => state.panel_binding.as_mut().unwrap().anchors[0] += 8,
                    1 => state.panel_binding.as_mut().unwrap().slots[0] += 8,
                    2 => state.panel_binding.as_mut().unwrap().roots[0] += 8,
                    3 => state.panel_binding.as_mut().unwrap().facilities_parent += 8,
                    _ => state.researching[0] = Some("support_weapons".into()),
                },
                Destination::Production { state, panel } => match mutation {
                    0 => state.line.country += 8,
                    1 => state.line.line += 8,
                    2 => state.line.variant += 8,
                    3 => state.line.base_type += 8,
                    _ => panel.panel_identity[6] += 8,
                },
            }
            assert!(n.verify_current(&changed, date()).is_err());
        }
        assert!(n.verify_current(&destination, date() - 1).is_err());
        assert_eq!(n.active(), Some(&entry));
        assert!(!n.blocked());
        assert_eq!(fs::read(t.file()).unwrap(), before);
    }
}
