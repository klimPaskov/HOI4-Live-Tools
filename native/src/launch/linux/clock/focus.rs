//! Explicit owner opt-in for one ordinary own-focus stage; no engine calls.
use super::*;
use crate::{
    focus_candidate_observation as candidate, focus_stage as stage,
    platform::x11::{PointerDelivery, PointerDisplay},
    research_attempt::{self, PointCandidate},
    research_candidate_observation::{codec, control_surface, OwnedClientProfile},
};
mod state;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Profile {
    #[serde(rename = "linux_builtin_absolute_512x288_v1")]
    LinuxBuiltinAbsolute512x288V1,
}
pub(super) struct Service {
    pub(super) journal: stage::Journal,
    profile: OwnedClientProfile,
    display: u16,
    authority: PathBuf,
}
impl Service {
    pub(super) fn bind(
        _profile: Profile,
        window: u32,
        display: &Display,
        directory: &Path,
    ) -> Result<Self> {
        Ok(Self {
            journal: stage::Journal::open(&directory.join("focus-actions.jsonl"))?,
            profile: OwnedClientProfile {
                window,
                client: [512, 288],
                logical: [2560, 1440],
                os_binding_verified: true,
                reviewed_builtin_client_origin: true,
            },
            display: display.number,
            authority: display.xauthority.clone(),
        })
    }
}
pub(super) fn tool() -> Value {
    json!({"name":"hoi4_focus_attempt","description":"Preview: one ordinary OpenPolitics Q, OpenFocus named-button, View or Start stage for an exact own canonical focus ID. Requires explicit owned profile, exclusive lease, settled own roots and complete own scoped blockers. Supported exact Sprite, zero-angle Text or unmasked Frame geometry; unknown branches refuse. Canonical readback verifies current state, not event causality or eligibility. Uncertain input is quarantined; no replay, coordinates, activation or engine calls.",
        "inputSchema":{"type":"object","properties":{"stage":{"enum":["open_politics","open_focus","view","start"]},"focus_id":{"type":"string","minLength":1,"maxLength":128},"request_id":{"type":"string","minLength":1,"maxLength":128},"lease_token":{"type":"string"}},"required":["stage","focus_id","request_id","lease_token"],"additionalProperties":false}})
}
struct Arguments<'a> {
    stage: stage::Stage,
    key: &'a str,
    id: &'a str,
    token: &'a str,
}
fn parse(args: &serde_json::Map<String, Value>) -> Result<Arguments<'_>> {
    allowed(args, &["stage", "focus_id", "request_id", "lease_token"])?;
    let selected = match string(args, "stage")? {
        "open_politics" => stage::Stage::OpenPolitics,
        "open_focus" => stage::Stage::OpenFocus,
        "view" => stage::Stage::View,
        "start" => stage::Stage::Start,
        _ => return Err(Error::new("invalid_arguments", "Unknown focus stage")),
    };
    let key = string(args, "focus_id")?;
    crate::own_focus::validate_key(key)
        .map_err(|_| Error::new("invalid_arguments", "Exact own focus ID required"))?;
    let id = string(args, "request_id")?;
    if id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
    {
        return Err(Error::new(
            "invalid_arguments",
            "Stable bounded stage request ID required",
        ));
    }
    Ok(Arguments {
        stage: selected,
        key,
        id,
        token: string(args, "lease_token")?,
    })
}
pub(super) fn call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let args = parse(args)?;
    owner.daily.input_check()?;
    drop(input_permit(&owner.input_gate)?);
    let mut state = owner.input_state()?;
    let target = stage::Target {
        epoch: state.guard.epoch.clone(),
        stage: args.stage,
        focus_id: args.key.into(),
    };
    let (execution, transport) = execute_internal(
        &mut state,
        process,
        &owner.input_gate,
        args.token,
        args.id,
        target,
        Instant::now() + Duration::from_secs(10),
    )?;
    Ok(
        json!({"execution":execution,"transport":transport,"global_modal_clear_available":false,"effective_routing_verified":false,"full_campaign_acceptance":false}),
    )
}
fn remaining(until: Instant) -> Result<Duration> {
    let timeout = until
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(10));
    if timeout.is_zero() {
        return Err(Error::new(
            "focus_deadline",
            "Focus deadline expired before dispatch",
        ));
    }
    Ok(timeout)
}
/// Borrow the common controller exactly once; never re-enter ClockService.call.
/// Manual preview only. Daily Focus remains capability-unavailable.
pub(super) fn execute_internal(
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    id: &str,
    target: stage::Target,
    until: Instant,
) -> Result<(stage::Execution, Option<Value>)> {
    drop(input_permit(gate)?);
    state.authority.authorize(token, &state.guard.epoch)?;
    if target.epoch != state.guard.epoch {
        return Err(Error::new(
            "ownership_changed",
            "Focus target epoch differs from controller",
        ));
    }
    if state.journal.blocks_game(&target.epoch.game)
        || state
            .research
            .as_ref()
            .is_some_and(|r| r.journal.blocks_game(&target.epoch.game))
        || state
            .focus
            .as_ref()
            .is_some_and(|f| f.journal.blocks_game(&target.epoch.game))
    {
        return Err(Error::new(
            "action_uncertain",
            "Unresolved clock/research/focus input blocks focus",
        ));
    }
    let ControlState {
        controls,
        guard,
        authority,
        focus,
        ..
    } = state;
    let service = focus.as_mut().ok_or_else(|| {
        Error::new(
            "capability_unavailable",
            "Owner did not opt in to focus preview",
        )
    })?;
    let mut backend = FocusBackend {
        process,
        controls,
        guard,
        profile: &service.profile,
        display: service.display,
        authority: &service.authority,
        target: target.clone(),
        scope: None,
        candidate_selection: None,
        last_transport: None,
        until,
    };
    let execution = service.journal.execute(
        id,
        target,
        token,
        authority,
        gate,
        &mut backend,
        remaining(until)?,
    )?;
    service.journal.finish_before_deadline(&execution, until)?;
    Ok((execution, backend.last_transport))
}

type Selection = (Vec<[u64; 2]>, Vec<[u64; 3]>);
struct FocusBackend<'a> {
    process: &'a dyn Process,
    controls: &'a mut X11Controls,
    guard: &'a mut Guard,
    profile: &'a OwnedClientProfile,
    display: u16,
    authority: &'a Path,
    target: stage::Target,
    scope: Option<state::Scope>,
    candidate_selection: Option<Selection>,
    last_transport: Option<Value>,
    until: Instant,
}
fn accept(guard: &mut Guard, process: &dyn Process, o: &state::Observation) -> Result<()> {
    guard.accept_context(process, &o.context.campaign)?;
    guard.accept_context(process, o.politics_context.campaign())?;
    Ok(())
}
fn observed(
    process: &dyn Process,
    guard: &mut Guard,
    target: &stage::Target,
    pin: &mut Option<state::Scope>,
    geometry: bool,
) -> Result<state::Observation> {
    let observation = state::observe(process, &target.focus_id, &target.epoch, geometry)?;
    accept(guard, process, &observation)?;
    state::pin(pin, observation.scope.clone())?;
    Ok(observation)
}
#[allow(dead_code)] // Prospective daily observer; worker input is unavailable.
pub(super) fn observed_state(
    process: &dyn Process,
    guard: &mut Guard,
    target: &stage::Target,
) -> Result<(stage::OwnState, crate::own_focus::FocusContext)> {
    let o = observed(process, guard, target, &mut None, false)?;
    Ok((o.state, o.context))
}

pub(super) fn observed_bound(
    process: &dyn Process,
    guard: &mut Guard,
    target: &stage::Target,
) -> Result<(
    stage::OwnState,
    crate::own_focus::FocusContext,
    crate::daily_focus_navigation::OwnBinding,
)> {
    let o = observed(process, guard, target, &mut None, false)?;
    let binding =
        crate::daily_focus_navigation::OwnBinding::from_context(&o.context, &o.politics_context)?;
    Ok((o.state, o.context, binding))
}
#[allow(dead_code)] // Prospective caller must classify immediately after execute.
pub(super) fn failure_evidence(
    state: &ControlState,
    id: &str,
    target: &stage::Target,
    error: &Error,
) -> stage::FailureEvidence {
    if state.journal.blocks_game(&target.epoch.game)
        || state
            .research
            .as_ref()
            .is_some_and(|r| r.journal.blocks_game(&target.epoch.game))
    {
        return stage::FailureEvidence::Unknown;
    }
    state
        .focus
        .as_ref()
        .map_or(stage::FailureEvidence::Unknown, |s| {
            s.journal.failure_evidence(id, target, error)
        })
}

fn politics_point(
    o: &state::Observation,
    process: &dyn Process,
    profile: &OwnedClientProfile,
) -> Result<PointCandidate> {
    let p = &o.politics;
    let flags = p["flags_raw"].as_u64().and_then(|v| u32::try_from(v).ok());
    let ac = p["disabled_raw"].as_i64();
    if p["available"] != true
        || p["callback_route_verified"] != true
        || p["renderer_control_join_verified"] != true
        || p["control_type_encoding"] != "15CButtonStandard"
        || p["target_managed_mode"] != 13
        || !matches!(ac, Some(0..=2))
        || !flags.is_some_and(|v| v & (1 << 11) != 0 && v & (1 << 12) == 0)
        || state::local_root(&p["active_goal"])? != stage::LocalRoot::Ready
    {
        return Err(Error::new(
            "focus_geometry_unavailable",
            "Exact settled own named politics control required",
        ));
    }
    let surface = control_surface(&p["render_diagnostic"])?;
    let geometry = crate::gui::input_geometry(process)?;
    let client = codec(&geometry, profile)?;
    process.check()?;
    research_attempt::point_candidate(&surface, &client)
}
fn point(
    process: &dyn Process,
    guard: &mut Guard,
    target: &stage::Target,
    profile: &OwnedClientProfile,
    pin: &mut Option<state::Scope>,
) -> Result<(PointCandidate, crate::own_focus::FocusContext)> {
    if target.stage == stage::Stage::OpenFocus {
        let o = observed(process, guard, target, pin, true)?;
        o.state.validate(target)?;
        o.state.preflight(target)?;
        return Ok((politics_point(&o, process, profile)?, o.context));
    }
    let which = match target.stage {
        stage::Stage::View => candidate::Stage::Node,
        stage::Stage::Start => candidate::Stage::Start,
        _ => return Err(Error::new("focus_precondition", "Point stage required")),
    };
    // Existing adapter proves exact own callback/renderer joins, conditional
    // cache/hierarchy and owned SDL mapping; it never claims current frame.
    let (point, context) = candidate::observe(process, &target.focus_id, which, profile)?;
    guard.accept_context(process, &context.campaign)?;
    let o = observed(process, guard, target, pin, false)?;
    if context.anchors != o.context.anchors || context.slots != o.context.slots {
        return Err(Error::new(
            "torn_read",
            "Point and own focus scopes disagree",
        ));
    }
    Ok((point, context))
}
impl stage::Backend for FocusBackend<'_> {
    fn observe(&mut self) -> Result<stage::OwnState> {
        Ok(observed(
            self.process,
            self.guard,
            &self.target,
            &mut self.scope,
            false,
        )?
        .state)
    }
    fn candidate(&mut self, target: &stage::Target) -> Result<stage::Candidate> {
        if target != &self.target {
            return Err(Error::new("ownership_changed", "Focus target changed"));
        }
        let o = observed(self.process, self.guard, target, &mut self.scope, false)?;
        o.state.validate(target)?;
        o.state.preflight(target)?;
        if target.stage == stage::Stage::OpenPolitics {
            self.controls
                .preflight(self.process, &ControlKey::PoliticsPanel)?;
            return Ok(stage::Candidate::OpenPolitics);
        }
        let (point, context) = point(
            self.process,
            self.guard,
            target,
            self.profile,
            &mut self.scope,
        )?;
        if target.stage == stage::Stage::Start {
            self.candidate_selection = Some((context.selected, context.matching_details));
        }
        Ok(stage::Candidate::Point(point))
    }
    fn dispatch_once(
        &mut self,
        candidate: &stage::Candidate,
        deadline: Instant,
    ) -> stage::Delivery {
        let deadline = deadline.min(self.until);
        if Instant::now() >= deadline {
            return stage::Delivery::NotQueued;
        }
        let process = self.process;
        let target = self.target.clone();
        let guard = &mut *self.guard;
        let mut pin = self.scope.clone();
        let profile = self.profile;
        let selected = self.candidate_selection.clone();
        let fresh_check = |guard: &mut Guard, pin: &mut Option<state::Scope>| -> Result<()> {
            let o = observed(process, guard, &target, pin, false)?;
            o.state.validate(&target)?;
            o.state.preflight(&target)?;
            if let Some(expected) = &selected {
                if expected != &(o.context.selected, o.context.matching_details) {
                    return Err(Error::new(
                        "focus_precondition",
                        "Selected own detail identity changed before input",
                    ));
                }
            }
            Ok(())
        };
        match candidate {
            stage::Candidate::OpenPolitics if target.stage == stage::Stage::OpenPolitics => {
                let mut may_queue = false;
                let result = self.controls.send_until_checked(
                    process,
                    ControlKey::PoliticsPanel,
                    deadline,
                    || {
                        fresh_check(guard, &mut pin)?;
                        may_queue = true;
                        Ok(true)
                    },
                );
                let (delivery, diagnostic) = match result {
                    Ok(true) => (
                        stage::Delivery::ServerConfirmed,
                        json!({"delivery":"server_confirmed","ordinary_key":"Q","game_action_verified":false}),
                    ),
                    Ok(false) => (stage::Delivery::NotQueued, json!({"delivery":"not_queued"})),
                    Err(error) if !may_queue => (
                        stage::Delivery::NotQueued,
                        json!({"delivery":"not_queued","error":error}),
                    ),
                    Err(error) => (
                        stage::Delivery::Unknown,
                        json!({"delivery":"unknown","error":error}),
                    ),
                };
                self.last_transport = Some(diagnostic);
                delivery
            }
            stage::Candidate::Point(original) if target.stage != stage::Stage::OpenPolitics => {
                match self.controls.prepare_left_click_until(
                    process,
                    PointerDisplay {
                        display: self.display,
                        authority: self.authority,
                    },
                    original.client_pixel,
                    original.expected_client,
                    deadline,
                ) {
                    Err(error) => {
                        self.last_transport = Some(json!({"delivery":"not_queued","error":error}));
                        stage::Delivery::NotQueued
                    }
                    Ok(mut prepared) => {
                        let result = prepared.dispatch_once_checked(|| {
                            fresh_check(guard, &mut pin)?;
                            let (current, _) = point(process, guard, &target, profile, &mut pin)?;
                            if current.client_pixel != original.client_pixel
                                || current.expected_client != original.expected_client
                                || current.ideal_logical != original.ideal_logical
                                || current.quantized_logical != original.quantized_logical
                            {
                                return Err(Error::new(
                                    "focus_geometry_unavailable",
                                    "Fresh candidate moved before input",
                                ));
                            }
                            // Recheck stage after the potentially blocking geometry reader.
                            fresh_check(guard, &mut pin)
                        });
                        let delivery = match result.delivery {
                            PointerDelivery::NotQueued => stage::Delivery::NotQueued,
                            PointerDelivery::Unknown => stage::Delivery::Unknown,
                            PointerDelivery::ServerConfirmed => stage::Delivery::ServerConfirmed,
                        };
                        self.last_transport = Some(json!(result));
                        delivery
                    }
                }
            }
            _ => stage::Delivery::NotQueued,
        }
    }
    fn confirm_input_idle(
        &mut self,
        candidate: &stage::Candidate,
        deadline: Instant,
    ) -> Result<()> {
        let deadline = deadline.min(self.until);
        if Instant::now() >= deadline {
            return Err(Error::new("focus_deadline", "Input-idle deadline ended"));
        }
        match candidate {
            stage::Candidate::OpenPolitics => self
                .controls
                .preflight(self.process, &ControlKey::PoliticsPanel),
            stage::Candidate::Point(point) => {
                let _probe = self.controls.prepare_left_click_until(
                    self.process,
                    PointerDisplay {
                        display: self.display,
                        authority: self.authority,
                    },
                    point.client_pixel,
                    point.expected_client,
                    deadline,
                )?;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parser_rejects_raw_input_and_unknown_stages_without_backend_access() {
        for value in [
            json!({"stage":"start","focus_id":"GER_rhineland","request_id":"x","lease_token":"t","x":1}),
            json!({"stage":"callback","focus_id":"GER_rhineland","request_id":"x","lease_token":"t"}),
            json!({"stage":"start","focus_id":"../foreign","request_id":"x","lease_token":"t"}),
            json!({"stage":"start","focus_id":"GER_rhineland","request_id":"bad token","lease_token":"t"}),
        ] {
            assert!(parse(value.as_object().unwrap()).is_err());
        }
        let value = json!({"stage":"open_politics","focus_id":"GER_rhineland","request_id":"fixed-q","lease_token":"t"});
        assert_eq!(
            parse(value.as_object().unwrap()).unwrap().stage,
            stage::Stage::OpenPolitics
        );
    }
    #[test]
    fn internal_deadline_is_absolute_expired_refuses_and_never_widens() {
        assert_eq!(
            remaining(Instant::now() - Duration::from_millis(1))
                .unwrap_err()
                .code,
            "focus_deadline"
        );
        let until = Instant::now() + Duration::from_millis(100);
        assert!(remaining(until).unwrap() <= Duration::from_millis(100));
        assert!(
            remaining(Instant::now() + Duration::from_secs(30)).unwrap() <= Duration::from_secs(10)
        );
    }
}
