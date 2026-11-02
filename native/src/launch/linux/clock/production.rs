//! Opted-in single-factory ordinary clicks on the shared controller.
use super::*;
use crate::{
    construction_stage::{read_recovery, DispatchOutcome},
    platform::x11::{PointerDelivery, PointerDisplay},
    production_lines::control_snapshot as observation,
    production_step as step,
    research_attempt::{self, PointCandidate},
    research_candidate_observation::{self as geometry, BuiltinClient, OwnedClientProfile},
};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub(super) enum Profile {
    #[serde(rename = "linux_builtin_absolute_512x288_v1")]
    LinuxBuiltinAbsolute512x288V1,
    #[serde(rename = "linux_builtin_absolute_1920x1080_v1")]
    LinuxBuiltinAbsolute1920x1080V1,
}
impl Profile {
    pub(super) const fn client(self) -> BuiltinClient {
        match self {
            Self::LinuxBuiltinAbsolute512x288V1 => BuiltinClient::Absolute512x288,
            Self::LinuxBuiltinAbsolute1920x1080V1 => BuiltinClient::Absolute1920x1080,
        }
    }
}
pub(super) struct Service {
    pub(super) journal: step::Journal,
    profile: OwnedClientProfile,
    display: u16,
    xauthority: PathBuf,
}
impl Service {
    pub(super) fn bind(
        profile: Profile,
        window: u32,
        display: &Display,
        directory: &Path,
    ) -> Result<Self> {
        Ok(Self {
            journal: step::Journal::open(&directory.join("production-actions.jsonl"))?,
            profile: profile.client().owned(window),
            display: display.number,
            xauthority: display.xauthority.clone(),
        })
    }
}
pub(super) fn tool() -> Value {
    json!({"name":"hoi4_production_attempt","description":"Linux preview: request exactly one factory increase or decrease for an exact own current line_handle and equipment_id in an already-open production panel. A matching count is a no-op. One ordinary unmodified click maximum, original count/identity persisted before input, canonical own-line readback. Requires owned profile and exclusive lease. An unresolved retry only checks the original target and never replays input; uncertainty blocks all domains. Minimum target one. No new line, panel opening, scrolling, raw coordinates or engine posting. Bounded exact-line Production004 gate passed; broader management and campaigns remain unverified.",
        "inputSchema":{"type":"object","properties":{
            "line_handle":{"type":"string","pattern":"^0x[0-9a-f]{16}$"},
            "equipment_id":{"type":"string","minLength":1,"maxLength":128},
            "factories":{"type":"integer","minimum":1,"maximum":1000000},
            "request_id":{"type":"string","minLength":1,"maxLength":128},"lease_token":{"type":"string"}},
            "required":["line_handle","equipment_id","factories","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn open_tool() -> Value {
    json!({"name":"hoi4_production_open","description":"Linux preview: ensure the owned production panel is open through one verified ordinary topbar click. Already-open is a no-op, never a toggle. Requires idle managed view or the already-open production view, exact owned profile and exclusive lease. Shares the production journal and all unresolved-input blocks. Same-ID uncertain retries only observe the original panel target. No raw coordinates, hotkeys or engine calls. Bounded opening, no-op and replay gates passed in Production004/008.",
        "inputSchema":{"type":"object","properties":{
            "request_id":{"type":"string","minLength":1,"maxLength":128},
            "lease_token":{"type":"string"}},
            "required":["request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn close_tool() -> Value {
    let mut tool = open_tool();
    tool["name"] = json!("hoi4_production_close");
    tool["description"] = json!("Linux preview: ensure the owned production panel is closed through at most one verified ordinary topbar click. Only settled shown production mode2 may click; settled hidden production in idle mode23 is a no-op. Other active panels refuse. Exact original panel identity, exclusive lease, shared durable production journal and all unresolved-input blocks remain required. Same-ID uncertain retries only observe; never replay input. No raw coordinates, hotkeys or engine calls. Bounded closing, replay/no-op and manual Research/Focus handoff passed Production008; daily mixed-panel routing not enabled.");
    tool
}
fn parse_open(args: &serde_json::Map<String, Value>, epoch: Epoch) -> Result<step::Target> {
    allowed(args, &["request_id", "lease_token"])?;
    string(args, "request_id")?;
    string(args, "lease_token")?;
    Ok(step::Target::Panel(step::PanelTarget::new(epoch)))
}
fn parse(args: &serde_json::Map<String, Value>, epoch: Epoch) -> Result<step::Target> {
    allowed(
        args,
        &[
            "line_handle",
            "equipment_id",
            "factories",
            "request_id",
            "lease_token",
        ],
    )?;
    let target = step::Target::Factories(step::FactoryTarget {
        epoch,
        line_handle: string(args, "line_handle")?.into(),
        equipment_id: string(args, "equipment_id")?.into(),
        factories: args
            .get("factories")
            .and_then(Value::as_i64)
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| {
                Error::new("invalid_arguments", "Integer factory allocation required")
            })?,
    });
    target.validate()?;
    Ok(target)
}
pub(super) fn call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
    panel_stage: Option<step::PanelStage>,
) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(10);
    owner.daily.input_check()?;
    drop(input_permit(&owner.input_gate)?);
    let mut state = owner.input_state()?;
    let target = if let Some(stage) = panel_stage {
        let mut target = parse_open(args, state.guard.epoch.clone())?;
        if let step::Target::Panel(panel) = &mut target {
            panel.stage = stage;
        }
        target
    } else {
        parse(args, state.guard.epoch.clone())?
    };
    let token = string(args, "lease_token")?;
    let id = string(args, "request_id")?;
    let execution = execute_internal(
        &mut state,
        process,
        &owner.input_gate,
        token,
        id,
        target,
        until,
    )?;
    Ok(
        json!({"execution":execution,"live_validation":"bounded_linux_gate_passed","validation_runs":["Production004","Production008"],"full_campaign_acceptance":false}),
    )
}
/// Shared ordinary route for the manual tool and future owner-held plan stages.
/// The caller holds the controller mutex; this retains all journal, lease,
/// unresolved-domain and final transport checks and the original deadline.
pub(super) fn execute_internal(
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    id: &str,
    target: step::Target,
    until: Instant,
) -> Result<step::Execution> {
    execute_bound(state, process, gate, token, id, target, until, None)
}

/// Bind the first ordinary journal observation to the mixed controller's exact
/// source. Subsequent candidates and queue checks retain that same baseline.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_bound(
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    id: &str,
    target: step::Target,
    until: Instant,
    expected: Option<&step::State>,
) -> Result<step::Execution> {
    state.authority.authorize(token, target.epoch())?;
    validate_expected_source(expected, &target)?;
    let ControlState {
        controls,
        guard,
        authority,
        journal,
        research_transits,
        research,
        focus,
        construction,
        production,
    } = state;
    let service = production
        .as_mut()
        .ok_or_else(|| Error::new("capability_unavailable", "Production profile unavailable"))?;
    let domains = Domains {
        clock: journal,
        transits: research_transits,
        research: research.as_ref(),
        focus: focus.as_ref(),
        construction: construction.as_ref(),
    };
    let mut backend = ProductionBackend {
        process,
        controls,
        guard,
        profile: &service.profile,
        display: service.display,
        xauthority: &service.xauthority,
        expected_source: expected.cloned(),
        first_observation: true,
    };
    let timeout = until.saturating_duration_since(Instant::now());
    service.journal.execute(
        id,
        target,
        token,
        authority,
        gate,
        &domains,
        &mut backend,
        timeout,
    )
}
struct Domains<'a> {
    clock: &'a Journal,
    transits: &'a transit_stage::Journal,
    research: Option<&'a research::Service>,
    focus: Option<&'a focus::Service>,
    construction: Option<&'a construction::Service>,
}
impl step::UnresolvedInputs for Domains<'_> {
    fn clear(&self, game: &Identity) -> Result<()> {
        if self.transits.blocks_game(game)
            || self.clock.blocks_game(game)
            || self.research.is_some_and(|v| v.journal.blocks_game(game))
            || self.focus.is_some_and(|v| v.journal.blocks_game(game))
            || self
                .construction
                .is_some_and(|v| v.journal.blocks_game(game))
        {
            return Err(Error::new(
                "action_uncertain",
                "Unresolved clock/research/focus/construction input blocks production",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, PartialEq)]
struct Candidate {
    point: PointCandidate,
    scope: Vec<u64>,
    geometry: Vec<(u64, Vec<u8>)>,
}
struct ProductionBackend<'a> {
    process: &'a dyn Process,
    controls: &'a mut X11Controls,
    guard: &'a mut Guard,
    profile: &'a OwnedClientProfile,
    display: u16,
    xauthority: &'a Path,
    expected_source: Option<step::State>,
    first_observation: bool,
}

fn validate_expected_source(expected: Option<&step::State>, target: &step::Target) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    expected.validate(target)?;
    if matches!(target, step::Target::Panel(t) if t.stage == step::PanelStage::EnsureClosed)
        && !matches!(expected, step::State::Panel(s) if s.managed_mode == 2 && s.panel_shown)
    {
        return Err(Error::new(
            "production_precondition",
            "Mixed departure requires its original shown production source",
        ));
    }
    Ok(())
}

fn require_expected_source(expected: Option<&step::State>, current: &step::State) -> Result<()> {
    if expected.is_some_and(|original| original != current) {
        return Err(Error::new(
            "production_state_changed",
            "Original mixed departure production source changed before Intent",
        ));
    }
    Ok(())
}
pub(super) fn observed(
    process: &dyn Process,
    guard: &mut Guard,
    target: &step::Target,
    ui: bool,
    deadline: Instant,
) -> Result<(observation::Observation, step::State)> {
    if *target.epoch() != guard.epoch {
        return Err(Error::new(
            "ownership_changed",
            "Production target epoch differs",
        ));
    }
    let factory = target.factory()?;
    let mut report = read_recovery::Report::default();
    let o = read_recovery::observe(
        read_recovery::Phase::Candidate,
        deadline,
        || {
            observation::observe(
                process,
                &factory.line_handle,
                &factory.equipment_id,
                ui.then_some(true),
            )
        },
        &mut report,
    )?;
    guard.accept_context(process, &o.campaign)?;
    let state = step::State::Factories(step::FactoryState {
        epoch: guard.epoch.clone(),
        line: o.line.clone(),
        requested: o.requested,
    });
    state.validate(target)?;
    check_selected_production(process, guard, Some(&o.line), None, deadline)?;
    research::check_selected_research(process, guard)?;
    if Instant::now() >= deadline {
        return Err(Error::new(
            "production_deadline",
            "Production observation deadline ended",
        ));
    }
    Ok((o, state))
}
pub(super) fn panel_observed(
    process: &dyn Process,
    guard: &mut Guard,
    target: &step::Target,
    ui: bool,
    deadline: Instant,
) -> Result<(crate::production_panel::Observation, step::State)> {
    if !matches!(target, step::Target::Panel(_)) || target.epoch() != &guard.epoch {
        return Err(Error::new(
            "ownership_changed",
            "Original production panel target required",
        ));
    }
    let mut report = read_recovery::Report::default();
    let o = read_recovery::observe(
        read_recovery::Phase::Candidate,
        deadline,
        || crate::production_panel::observe(process, ui, ui),
        &mut report,
    )?;
    guard.accept_context(process, &o.campaign)?;
    let state = panel_state(&o.value, o.identity.clone(), guard.epoch.clone())?;
    state.validate(target)?;
    check_selected_production(process, guard, None, Some(&state), deadline)?;
    research::check_selected_research(process, guard)?;
    if Instant::now() >= deadline {
        return Err(Error::new(
            "production_deadline",
            "Production panel observation deadline ended",
        ));
    }
    Ok((o, state))
}

pub(super) fn check_selected_production(
    process: &dyn Process,
    guard: &mut Guard,
    line: Option<&step::LineIdentity>,
    panel: Option<&step::State>,
    deadline: Instant,
) -> Result<()> {
    let Some(original @ crate::daily_panel_navigation::Destination::Production { .. }) =
        guard.selected_target.clone()
    else {
        return Ok(());
    };
    if Instant::now() >= deadline {
        return Err(Error::new(
            "production_deadline",
            "Selected production identity deadline ended",
        ));
    }
    let observed_line = if line.is_none() {
        let crate::daily_panel_navigation::Destination::Production { state, .. } = &original else {
            unreachable!("Production destination matched above")
        };
        // Canonical-only raw reader; never recurse through observed() or any
        // Research adapter while checking the opposite domain's departure.
        let handle = format!("0x{:016x}", state.line.handle);
        let o = observation::observe(process, &handle, &state.line.equipment_id, None)?;
        guard.accept_context(process, &o.campaign)?;
        Some(o.line)
    } else {
        None
    };
    let line = line.or(observed_line.as_ref());
    let current_panel = match panel {
        Some(panel) => panel.clone(),
        None => {
            // This does not recurse through panel_observed. It obtains only
            // the retained original panel identity, with no geometry or input.
            let o = crate::production_panel::observe(process, false, false)?;
            guard.accept_context(process, &o.campaign)?;
            panel_state(&o.value, o.identity, guard.epoch.clone())?
        }
    };
    require_selected_production(&original, line, &current_panel)?;
    if Instant::now() >= deadline {
        return Err(Error::new(
            "production_deadline",
            "Selected production identity deadline ended",
        ));
    }
    Ok(())
}

fn require_selected_production(
    original: &crate::daily_panel_navigation::Destination,
    line: Option<&step::LineIdentity>,
    current_panel: &step::State,
) -> Result<()> {
    let crate::daily_panel_navigation::Destination::Production { state, panel } = original else {
        return Ok(());
    };
    let step::State::Panel(current) = current_panel else {
        return Err(Error::new(
            "ownership_changed",
            "Selected production panel identity required",
        ));
    };
    if state.epoch != current.epoch
        || panel.epoch != current.epoch
        || panel.panel_identity != current.panel_identity
        || line.is_some_and(|line| line != &state.line)
    {
        return Err(Error::new(
            "ownership_changed",
            "Original selected production identity changed before action",
        ));
    }
    Ok(())
}
fn panel_state(value: &Value, identity: Vec<u64>, epoch: Epoch) -> Result<step::State> {
    if value["available"] != true || value["root"]["materialized"] != true {
        return Err(Error::new(
            "production_panel_unavailable",
            "Exact materialized own production panel required",
        ));
    }
    let shown = match (
        value["root"]["settled"] == true,
        value["root"]["locally_shown_settled"].as_bool(),
        value["root"]["locally_hidden_settled"].as_bool(),
    ) {
        (true, Some(true), Some(false)) => true,
        (true, Some(false), Some(true)) => false,
        _ => {
            return Err(Error::new(
                "production_panel_transition",
                "Own panel is not positively settled",
            ))
        }
    };
    let mode = value["mode_raw"]
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| Error::new("invalid_production_panel", "Literal manager mode required"))?;
    Ok(step::State::Panel(step::PanelState {
        epoch,
        panel_identity: identity,
        managed_mode: mode,
        panel_shown: shown,
    }))
}
fn panel_candidate(
    process: &dyn Process,
    guard: &mut Guard,
    target: &step::Target,
    before: &step::State,
    profile: &OwnedClientProfile,
    deadline: Instant,
) -> Result<Candidate> {
    let (o, state) = panel_observed(process, guard, target, true, deadline)?;
    if state != *before {
        return Err(Error::new(
            "production_state_changed",
            "Original production panel changed before candidate",
        ));
    }
    match target {
        step::Target::Panel(t) if t.stage == step::PanelStage::EnsureClosed => {
            crate::production_panel::require_close_candidate(&o.value, !o.geometry.is_empty())?
        }
        step::Target::Panel(_) => {
            crate::production_panel::require_open_candidate(&o.value, !o.geometry.is_empty())?
        }
        _ => return Err(Error::new("invalid_arguments", "Panel target required")),
    }
    let surface = geometry::control_surface(&o.value["opener"]["render_diagnostic"])?;
    let codec = geometry::codec(&o.value["input_geometry"], profile)?;
    let point = research_attempt::point_candidate(&surface, &codec)?;
    Ok(Candidate {
        point,
        scope: o.scope,
        geometry: o.geometry,
    })
}
fn candidate(
    process: &dyn Process,
    guard: &mut Guard,
    target: &step::Target,
    before: &step::State,
    profile: &OwnedClientProfile,
    deadline: Instant,
) -> Result<Candidate> {
    if matches!(target, step::Target::Panel(_)) {
        return panel_candidate(process, guard, target, before, profile, deadline);
    }
    let (o, state) = observed(process, guard, target, true, deadline)?;
    if state != *before {
        return Err(Error::new(
            "production_state_changed",
            "Original line changed before candidate",
        ));
    }
    let direction = if target.factory()?.factories > before.factory()?.requested {
        "add"
    } else {
        "subtract"
    };
    let control = &o.value["controls"][direction];
    if o.value["build_id"] != game::BUILD_ID
        || o.value["engine_gates"]["conservative_engine_clear"] != true
        || o.value["locally_ready"] != true
        || o.value["common_gui_gates_clear"] != true
        || o.value["common_blockers"]["conservative_scoped_clear"] != true
        || control["binding_available"] != true
        || control["local_dispatch_ready"] != true
    {
        return Err(Error::new(
            "production_precondition",
            "Exact ready own production control and clear scoped blockers required",
        ));
    }
    let transcript = o
        .geometry
        .get(direction)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            Error::new(
                "production_geometry_unavailable",
                "Same-snapshot renderer identity transcript required",
            )
        })?;
    let surface = geometry::control_surface(&control["sprite_diagnostic"])?;
    let codec = geometry::codec(&o.value["input_geometry"], profile)?;
    let point = research_attempt::point_candidate(&surface, &codec)?;
    Ok(Candidate {
        point,
        scope: o.scope,
        geometry: transcript.clone(),
    })
}
impl step::Backend for ProductionBackend<'_> {
    type Candidate = Candidate;
    fn observe(&mut self, t: &step::Target, deadline: Instant) -> Result<step::State> {
        let state = if matches!(t, step::Target::Panel(_)) {
            panel_observed(self.process, self.guard, t, false, deadline)?.1
        } else {
            observed(self.process, self.guard, t, false, deadline)?.1
        };
        if self.first_observation {
            require_expected_source(self.expected_source.as_ref(), &state)?;
            self.first_observation = false;
        }
        Ok(state)
    }
    fn candidate(
        &mut self,
        t: &step::Target,
        before: &step::State,
        deadline: Instant,
    ) -> Result<Candidate> {
        candidate(self.process, self.guard, t, before, self.profile, deadline)
    }
    fn dispatch_once(
        &mut self,
        t: &step::Target,
        before: &step::State,
        original: &Candidate,
        deadline: Instant,
    ) -> DispatchOutcome {
        let prepared = self.controls.prepare_left_click_until(
            self.process,
            PointerDisplay {
                display: self.display,
                authority: self.xauthority,
            },
            original.point.client_pixel,
            original.point.expected_client,
            deadline,
        );
        let mut prepared = match prepared {
            Ok(v) => v,
            Err(e) => return DispatchOutcome::NotQueued(e),
        };
        let result = prepared.dispatch_once_checked(|| {
            let current = candidate(self.process, self.guard, t, before, self.profile, deadline)?;
            if current != *original {
                return Err(Error::new(
                    "production_geometry_changed",
                    "Original production point or identities changed before queueing",
                ));
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "production_deadline",
                    "Original production deadline ended before queueing",
                ));
            }
            Ok(())
        });
        match result.delivery {
            PointerDelivery::NotQueued => {
                DispatchOutcome::NotQueued(result.error.unwrap_or_else(|| {
                    Error::new("input_not_queued", "No production pointer input queued")
                }))
            }
            PointerDelivery::Unknown => DispatchOutcome::Unknown(result.error),
            PointerDelivery::ServerConfirmed => DispatchOutcome::ServerConfirmed,
        }
    }
    fn confirm_input_idle(&mut self, deadline: Instant) -> Result<()> {
        // Preparation is read-only: no click, release, parking or activation.
        let _probe = self.controls.prepare_left_click_until(
            self.process,
            PointerDisplay {
                display: self.display,
                authority: self.xauthority,
            },
            [0, 0],
            self.profile.client,
            deadline,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn transit_quarantine_blocks_internal_production_without_other_profiles() {
        use super::*;
        use crate::production_step::UnresolvedInputs;
        use std::os::unix::fs::PermissionsExt;
        let directory =
            std::env::temp_dir().join(format!("production-transit-domain-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        let (transits, game) = transit_stage::fixture_intent(&directory.join("transits.json"));
        let clock = Journal::open(&directory.join("clock.jsonl")).unwrap();
        let domains = Domains {
            clock: &clock,
            transits: &transits,
            research: None,
            focus: None,
            construction: None,
        };
        assert_eq!(domains.clear(&game).unwrap_err().code, "action_uncertain");
        let mut other = game;
        other.start_ticks.push('1');
        domains.clear(&other).unwrap();
        drop(transits);
        drop(clock);
        std::fs::remove_dir_all(directory).unwrap();
    }
    use super::*;
    fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 10,
                start_ticks: "1".into(),
                exe: "/fixture/game".into(),
                uid: 1000,
            },
            campaign: "fixture".into(),
            player: "GER".into(),
        }
    }
    fn shown_source() -> step::State {
        step::State::Panel(step::PanelState {
            epoch: epoch(),
            panel_identity: (2..9).map(|n| n * 0x10000).collect(),
            managed_mode: 2,
            panel_shown: true,
        })
    }
    #[test]
    fn mixed_production_source_requires_original_shown_state_before_intent() {
        let target = step::Target::Panel(step::PanelTarget::closed(epoch()));
        let shown = shown_source();
        validate_expected_source(Some(&shown), &target).unwrap();
        require_expected_source(Some(&shown), &shown).unwrap();
        let mut closed = shown.clone();
        if let step::State::Panel(s) = &mut closed {
            s.managed_mode = 23;
            s.panel_shown = false;
        }
        assert!(validate_expected_source(Some(&closed), &target).is_err());
        assert!(require_expected_source(Some(&shown), &closed).is_err());
        validate_expected_source(None, &target).unwrap();
        require_expected_source(None, &closed).unwrap();
        let mut changed = shown.clone();
        if let step::State::Panel(s) = &mut changed {
            s.panel_identity[0] += 8;
        }
        assert!(require_expected_source(Some(&shown), &changed).is_err());
        if let step::State::Panel(s) = &mut changed {
            s.epoch.campaign = "changed".into();
        }
        assert!(validate_expected_source(Some(&changed), &target).is_err());
    }
    #[test]
    fn selected_production_pin_rejects_changed_panel_or_line_without_pinning_counts() {
        let step::State::Panel(panel) = shown_source() else {
            unreachable!()
        };
        let line = step::LineIdentity {
            country: panel.panel_identity[0],
            production: panel.panel_identity[1],
            line: 0x90000,
            variant: 0xa0000,
            base_type: 0xb0000,
            handle: 0xffff000100000038,
            equipment_id: "infantry_equipment_1".into(),
        };
        let original = crate::daily_panel_navigation::Destination::Production {
            state: step::FactoryState {
                epoch: epoch(),
                line: line.clone(),
                requested: 10,
            },
            panel: panel.clone(),
        };
        let mut current_panel = step::State::Panel(panel);
        require_selected_production(&original, Some(&line), &current_panel).unwrap();
        if let step::State::Panel(s) = &mut current_panel {
            s.managed_mode = 23;
            s.panel_shown = false;
        }
        require_selected_production(&original, Some(&line), &current_panel).unwrap();
        for changed_line in [
            {
                let mut l = line.clone();
                l.country += 8;
                l
            },
            {
                let mut l = line.clone();
                l.production += 8;
                l
            },
            {
                let mut l = line.clone();
                l.line += 8;
                l
            },
            {
                let mut l = line.clone();
                l.variant += 8;
                l
            },
            {
                let mut l = line.clone();
                l.base_type += 8;
                l
            },
            {
                let mut l = line.clone();
                l.handle += 1;
                l
            },
            {
                let mut l = line.clone();
                l.equipment_id = "infantry_equipment_2".into();
                l
            },
        ] {
            assert!(
                require_selected_production(&original, Some(&changed_line), &current_panel)
                    .is_err()
            );
        }
        for index in 0..7 {
            let mut changed = current_panel.clone();
            if let step::State::Panel(s) = &mut changed {
                s.panel_identity[index] += 8;
            }
            assert!(require_selected_production(&original, None, &changed).is_err());
        }
    }
    #[test]
    fn production_arguments_never_accept_raw_input_or_zero() {
        let valid = json!({"line_handle":"0xffff000100000038","equipment_id":"infantry_equipment_1","factories":5,"request_id":"one","lease_token":"fixture"});
        assert!(parse(valid.as_object().unwrap(), epoch()).is_ok());
        for (key, value) in [
            ("factories", json!(0)),
            ("factories", json!(5.5)),
            ("factories", json!(null)),
            ("line_handle", json!("0x38")),
            ("equipment_id", json!("bad/key")),
            ("x", json!(5)),
            ("key", json!("Y")),
            ("force", json!(true)),
        ] {
            let mut args = valid.clone();
            args[key] = value;
            assert!(parse(args.as_object().unwrap(), epoch()).is_err());
        }
    }
    #[test]
    fn opening_has_no_raw_input_or_allocation_arguments() {
        let valid = json!({"request_id":"open","lease_token":"fixture"});
        assert!(matches!(
            parse_open(valid.as_object().unwrap(), epoch()).unwrap(),
            step::Target::Panel(_)
        ));
        for (key, value) in [
            ("factories", json!(1)),
            ("x", json!(10)),
            ("line_handle", json!("0xffff000100000038")),
            ("stage", json!("close")),
            ("key", json!("Y")),
        ] {
            let mut args = valid.clone();
            args[key] = value;
            assert!(parse_open(args.as_object().unwrap(), epoch()).is_err());
        }
        for key in ["request_id", "lease_token"] {
            let mut args = valid.clone();
            args.as_object_mut().unwrap().remove(key);
            assert!(parse_open(args.as_object().unwrap(), epoch()).is_err());
        }
        assert_eq!(open_tool()["inputSchema"]["additionalProperties"], false);
        assert_eq!(close_tool()["inputSchema"], open_tool()["inputSchema"]);
        assert_eq!(close_tool()["name"], "hoi4_production_close");
    }
    #[test]
    fn panel_state_requires_positive_settled_visibility_and_manager_mode() {
        let identity: Vec<u64> = (2..9).map(|n| n * 0x10000).collect();
        let valid = json!({"available":true,"mode_raw":2,"root":{"materialized":true,"settled":true,"locally_shown_settled":true,"locally_hidden_settled":false}});
        let state = panel_state(&valid, identity.clone(), epoch()).unwrap();
        state
            .validate(&step::Target::Panel(step::PanelTarget::new(epoch())))
            .unwrap();
        for field in ["settled", "locally_shown_settled", "locally_hidden_settled"] {
            let mut value = valid.clone();
            value["root"][field] = Value::Null;
            assert_eq!(
                panel_state(&value, identity.clone(), epoch())
                    .unwrap_err()
                    .code,
                "production_panel_transition"
            );
        }
        let mut value = valid.clone();
        value["root"]["locally_hidden_settled"] = json!(true);
        assert!(panel_state(&value, identity.clone(), epoch()).is_err());
        value = valid.clone();
        value["root"]["materialized"] = json!(false);
        assert_eq!(
            panel_state(&value, identity.clone(), epoch())
                .unwrap_err()
                .code,
            "production_panel_unavailable"
        );
        value = valid;
        value["mode_raw"] = Value::Null;
        assert!(panel_state(&value, identity, epoch()).is_err());
    }
}
