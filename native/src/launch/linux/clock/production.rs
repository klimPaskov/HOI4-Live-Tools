//! Opted-in single-factory ordinary clicks on the shared controller.
use super::*;
use crate::{
    construction_stage::{read_recovery, DispatchOutcome},
    platform::x11::{PointerDelivery, PointerDisplay},
    production_lines::control_snapshot as observation,
    production_step as step,
    research_attempt::{self, PointCandidate},
    research_candidate_observation::{self as geometry, OwnedClientProfile},
};

#[derive(Clone, Copy, Deserialize, Serialize)]
pub(super) enum Profile {
    #[serde(rename = "linux_builtin_absolute_512x288_v1")]
    LinuxBuiltinAbsolute512x288V1,
}
pub(super) struct Service {
    pub(super) journal: step::Journal,
    profile: OwnedClientProfile,
    display: u16,
    xauthority: PathBuf,
}
impl Service {
    pub(super) fn bind(
        _profile: Profile,
        window: u32,
        display: &Display,
        directory: &Path,
    ) -> Result<Self> {
        Ok(Self {
            journal: step::Journal::open(&directory.join("production-actions.jsonl"))?,
            profile: OwnedClientProfile {
                window,
                client: [512, 288],
                logical: [2560, 1440],
                os_binding_verified: true,
                reviewed_builtin_client_origin: true,
            },
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
    state.authority.authorize(token, target.epoch())?;
    let ControlState {
        controls,
        guard,
        authority,
        journal,
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
    research: Option<&'a research::Service>,
    focus: Option<&'a focus::Service>,
    construction: Option<&'a construction::Service>,
}
impl step::UnresolvedInputs for Domains<'_> {
    fn clear(&self, game: &Identity) -> Result<()> {
        if self.clock.blocks_game(game)
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
    if Instant::now() >= deadline {
        return Err(Error::new(
            "production_deadline",
            "Production panel observation deadline ended",
        ));
    }
    Ok((o, state))
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
        if matches!(t, step::Target::Panel(_)) {
            return panel_observed(self.process, self.guard, t, false, deadline).map(|(_, s)| s);
        }
        observed(self.process, self.guard, t, false, deadline).map(|(_, s)| s)
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
