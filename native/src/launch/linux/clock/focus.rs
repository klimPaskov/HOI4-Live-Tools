//! Explicit owner opt-in for one ordinary own-focus stage; no engine calls.
use super::*;
use crate::{
    focus_candidate_observation as candidate, focus_stage as stage,
    platform::x11::{PointerDelivery, PointerDisplay},
    research_attempt::{self, PointCandidate},
    research_candidate_observation::{codec, control_surface, BuiltinClient, OwnedClientProfile},
};
mod state;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
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
    pub(super) journal: stage::Journal,
    profile: OwnedClientProfile,
    display: u16,
    authority: PathBuf,
}
impl Service {
    pub(super) fn bind(
        profile: Profile,
        window: u32,
        display: &Display,
        directory: &Path,
    ) -> Result<Self> {
        Ok(Self {
            journal: stage::Journal::open(&directory.join("focus-actions.jsonl"))?,
            profile: profile.client().owned(window),
            display: display.number,
            authority: display.xauthority.clone(),
        })
    }
}
pub(super) fn tool() -> Value {
    json!({"name":"hoi4_focus_attempt","description":"Preview: one ordinary OpenPolitics/ClosePolitics/OpenPoliticsFromResearch Q, OpenFocus named-button, SearchField empty-field click, SearchQuery bounded text, View or Start stage for an exact own canonical focus ID. ClosePolitics requires that exact focus active and preserves it through settled closed-root readback. OpenPoliticsFromResearch requires both profiles, an own settled research panel and hidden tree; its live gate is pending. Requires explicit owned profile, exclusive lease, settled own roots and complete own scoped blockers. SearchField preserves the active Focus and requires both keyboard/text receiver readback; SearchQuery requires an already focused empty field and explicit lowercase ASCII query; it verifies the exact text and selected canonical key. SearchText verifies exact text and complete bounded canonical matches without claiming the requested selection; SearchNext sends one Tab only when focus_id is the immediate next match, and verifies unchanged order and the expected selected key. All search stages preserve the active Focus and never press Enter. Supported exact Sprite, zero-angle Text, InstantText or unmasked Frame geometry; unknown branches refuse. Canonical readback verifies current state, not event causality or eligibility. Uncertain input is quarantined; no replay, coordinates, activation or engine calls.",
        "inputSchema":{"type":"object","properties":{"stage":{"enum":["open_politics","open_politics_from_research","close_politics","open_focus","search_field","search_query","search_text","search_next","view","start"]},"focus_id":{"type":"string","minLength":1,"maxLength":128},"query":{"type":"string","minLength":1,"maxLength":48,"description":"Required for search_query, search_text and search_next: lowercase ASCII letters and spaces, no surrounding spaces"},"request_id":{"type":"string","minLength":1,"maxLength":128},"lease_token":{"type":"string"}},"required":["stage","focus_id","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn observe_ui(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let key = crate::focus_ui_observation::key(args)?;
    if !owner.focus_enabled {
        return Err(Error::new(
            "capability_unavailable",
            "Owned Focus binding required",
        ));
    }
    let start = Instant::now();
    // Serialize with ordinary input but do not require or mutate a lease, pause
    // the game, clear quarantine, touch journals, or enter an input dispatcher.
    let mut control = try_state(&owner.state)?;
    let observation = state::observe(process, key, &control.guard.epoch, false)?;
    accept(&mut control.guard, process, &observation)?;
    process.check()?;
    if start.elapsed() >= Duration::from_secs(2) {
        return Err(Error::new(
            "focus_observation_deadline",
            "Own Focus UI observation exceeded two seconds",
        ));
    }
    Ok(crate::focus_ui_observation::report(
        &observation.state,
        observation.context.campaign.date_raw,
        observation.politics_context.campaign().date_raw,
        crate::daily_observation::unix_ms()?,
        start.elapsed().as_secs_f64() * 1000.0,
    ))
}
struct Arguments<'a> {
    stage: stage::Stage,
    key: &'a str,
    id: &'a str,
    token: &'a str,
    query: Option<&'a str>,
}
fn parse(args: &serde_json::Map<String, Value>) -> Result<Arguments<'_>> {
    allowed(
        args,
        &["stage", "focus_id", "request_id", "lease_token", "query"],
    )?;
    let selected = match string(args, "stage")? {
        "open_politics" => stage::Stage::OpenPolitics,
        "open_politics_from_research" => stage::Stage::OpenPoliticsFromResearch,
        "close_politics" => stage::Stage::ClosePolitics,
        "open_focus" => stage::Stage::OpenFocus,
        "search_field" => stage::Stage::SearchField,
        "search_query" => stage::Stage::SearchQuery,
        "search_text" => stage::Stage::SearchText,
        "search_next" => stage::Stage::SearchNext,
        "view" => stage::Stage::View,
        "start" => stage::Stage::Start,
        _ => return Err(Error::new("invalid_arguments", "Unknown focus stage")),
    };
    let query = if selected.uses_query() {
        let query = string(args, "query")?;
        stage::validate_query(query)?;
        Some(query)
    } else if args.contains_key("query") {
        return Err(Error::new(
            "invalid_arguments",
            "Query is only valid for search_query, search_text or search_next",
        ));
    } else {
        None
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
        query,
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
        query: args.query.map(str::to_owned),
    };
    let result = execute_internal(
        &mut state,
        process,
        &owner.input_gate,
        args.token,
        args.id,
        target,
        Instant::now() + Duration::from_secs(10),
    )?;
    Ok(
        json!({"execution":result.execution,"transport":result.transport,"global_modal_clear_available":false,"effective_routing_verified":false,"full_campaign_acceptance":false}),
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
/// Used by manual previews and the separately opted-in daily Focus worker.
pub(super) struct Executed {
    pub(super) execution: stage::Execution,
    pub(super) transport: Option<Value>,
    pub(super) binding: Option<crate::daily_focus_navigation::OwnBinding>,
}
pub(super) fn execute_internal(
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    id: &str,
    target: stage::Target,
    until: Instant,
) -> Result<Executed> {
    // Each fixed Q route has its own typed preconditions and receipt.
    // Closing Politics and returning from research also preserve active Focus.
    // The opted-in daily worker shares this executor; no raw key API exists.
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
            .construction
            .as_ref()
            .is_some_and(|c| c.journal.blocks_game(&target.epoch.game))
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
    if target.stage == stage::Stage::OpenPoliticsFromResearch && state.research.is_none() {
        return Err(Error::new(
            "capability_unavailable",
            "Research-to-Politics requires both owned profiles",
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
        search_selection: None,
        preintent_observations: 0,
        preintent_rejected_reads: Vec::new(),
        last_observed_binding: None,
        preserved_focus: None,
        last_transport: None,
        until,
    };
    let result = service.journal.execute(
        id,
        target,
        token,
        authority,
        gate,
        &mut backend,
        remaining(until)?,
    );
    if !backend.preintent_rejected_reads.is_empty() {
        let transport = backend
            .last_transport
            .get_or_insert_with(|| json!({"delivery":"NotAttempted","game_action_verified":false}));
        transport["preintent_rejected_reads"] = json!(backend.preintent_rejected_reads);
    }
    let execution = result
        .map_err(|error| stage::with_transport_cause(error, backend.last_transport.as_ref()))?;
    service.journal.finish_before_deadline(&execution, until)?;
    let binding = crate::daily_focus_navigation::receipt_binding(
        &execution,
        backend.last_observed_binding.as_ref(),
    )?;
    Ok(Executed {
        execution,
        transport: backend.last_transport,
        binding,
    })
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
    preserved_focus: Option<stage::ActiveFocus>,
    candidate_selection: Option<Selection>,
    search_selection: Option<stage::SearchMatches>,
    preintent_observations: usize,
    preintent_rejected_reads: Vec<Value>,
    last_observed_binding: Option<(stage::OwnState, crate::daily_focus_navigation::OwnBinding)>,
    last_transport: Option<Value>,
    until: Instant,
}
impl FocusBackend<'_> {
    fn preintent_observation(
        &mut self,
        phase: stage::read_recovery::PreintentPhase,
    ) -> Result<state::Observation> {
        if !self.target.stage.uses_query() {
            return observed(
                self.process,
                self.guard,
                &self.target,
                &mut self.scope,
                false,
            );
        }
        // Run067 refused before Intent, outside the old final queue check.
        // Retry only the wholly discarded producer call, never accept_context,
        // scope pinning, state predicates, journal execution or input dispatch.
        let o = stage::read_recovery::before_intent(
            phase,
            self.until,
            || {
                state::observe_with_search(
                    self.process,
                    &self.target.focus_id,
                    &self.target.epoch,
                    false,
                    true,
                )
            },
            &mut self.preintent_rejected_reads,
        )?;
        accept(self.guard, self.process, &o)?;
        state::pin(&mut self.scope, o.scope.clone())?;
        Ok(o)
    }
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
    let observation = state::observe_with_search(
        process,
        &target.focus_id,
        &target.epoch,
        geometry,
        target.stage == stage::Stage::SearchField || target.stage.uses_query(),
    )?;
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
            .construction
            .as_ref()
            .is_some_and(|c| c.journal.blocks_game(&target.epoch.game))
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
        stage::Stage::SearchField => candidate::Stage::SearchField,
        _ => return Err(Error::new("focus_precondition", "Point stage required")),
    };
    // Existing adapter proves exact own callback/renderer joins, conditional
    // cache/hierarchy and owned SDL mapping; it never claims current frame.
    let (point, context) = candidate::observe(process, &target.focus_id, which, profile)?;
    guard.accept_context(process, &context.campaign)?;
    let o = observed(process, guard, target, pin, false)?;
    if context.anchors != o.context.anchors
        || context.slots != o.context.slots
        || context.search_anchors != o.context.search_anchors
    {
        return Err(Error::new(
            "torn_read",
            "Point and own focus scopes disagree",
        ));
    }
    Ok((point, context))
}
impl stage::Backend for FocusBackend<'_> {
    fn observe(&mut self) -> Result<stage::OwnState> {
        let result = if self.last_transport.is_none() {
            let phase = match self.preintent_observations {
                0 => stage::read_recovery::PreintentPhase::Initial,
                1 => stage::read_recovery::PreintentPhase::Final,
                _ => {
                    return Err(Error::new(
                        "focus_precondition",
                        "Unexpected pre-intent observation phase",
                    ))
                }
            };
            self.preintent_observations += 1;
            self.preintent_observation(phase)
        } else {
            observed(
                self.process,
                self.guard,
                &self.target,
                &mut self.scope,
                false,
            )
        };
        let result = result.and_then(|o| {
            let binding = crate::daily_focus_navigation::OwnBinding::from_context(
                &o.context,
                &o.politics_context,
            )?;
            self.last_observed_binding = Some((o.state.clone(), binding));
            Ok(o)
        });
        if self.target.stage.uses_query() {
            if let Some(transport) = &mut self.last_transport {
                stage::query_readback::record(
                    transport,
                    result
                        .as_ref()
                        .map(|o| (&o.state, o.search_selection.as_ref())),
                );
            }
        }
        result.map(|o| o.state)
    }
    fn candidate(&mut self, target: &stage::Target) -> Result<stage::Candidate> {
        if target != &self.target {
            return Err(Error::new("ownership_changed", "Focus target changed"));
        }
        let o = self.preintent_observation(stage::read_recovery::PreintentPhase::Candidate)?;
        o.state.validate(target)?;
        o.state.preflight(target)?;
        if target.stage.preserves_focus() {
            self.preserved_focus = Some(o.state.active_focus.clone());
        }
        if target.stage.types_query() {
            let query = target
                .query
                .as_deref()
                .ok_or_else(|| Error::new("invalid_arguments", "Search query required"))?;
            self.controls.preflight_query(self.process, query)?;
            return Ok(stage::Candidate::SearchQuery(query.into()));
        }
        if target.stage == stage::Stage::SearchNext {
            let matches = o.state.search_matches().cloned().ok_or_else(|| {
                Error::new("focus_precondition", "Complete search order required")
            })?;
            self.controls.preflight_search_next(self.process)?;
            self.search_selection = Some(matches.clone());
            return Ok(stage::Candidate::SearchNext(matches));
        }
        if target.stage.is_shortcut() {
            self.controls
                .preflight(self.process, &ControlKey::PoliticsPanel)?;
            return Ok(match target.stage {
                stage::Stage::ClosePolitics => stage::Candidate::ClosePolitics,
                stage::Stage::OpenPoliticsFromResearch => {
                    stage::Candidate::OpenPoliticsFromResearch
                }
                _ => stage::Candidate::OpenPolitics,
            });
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
        let preserved_focus = self.preserved_focus.clone();
        let search_selection = self.search_selection.clone();
        let mut rejected_reads = Vec::new();
        let mut fresh_check = |guard: &mut Guard, pin: &mut Option<state::Scope>| -> Result<()> {
            let o = if target.stage.uses_query() {
                // Only complete native reads may retry, before possible input.
                // No guard/pin acceptance, journal operation or X11 dispatch is
                // inside the recovery loop. Preserve the original lease ceiling.
                let o = stage::read_recovery::before_query(
                    deadline,
                    || {
                        state::observe_with_search(
                            process,
                            &target.focus_id,
                            &target.epoch,
                            false,
                            true,
                        )
                    },
                    &mut rejected_reads,
                )?;
                accept(guard, process, &o)?;
                state::pin(pin, o.scope.clone())?;
                o
            } else {
                observed(process, guard, &target, pin, false)?
            };
            o.state.validate(&target)?;
            o.state.preflight(&target)?;
            if preserved_focus
                .as_ref()
                .is_some_and(|expected| expected != &o.state.active_focus)
            {
                return Err(Error::new(
                    "torn_read",
                    "Pinned canonical active focus changed before navigation input",
                ));
            }
            if let Some(expected) = &search_selection {
                if o.state.search_matches() != Some(expected) {
                    return Err(Error::new(
                        "torn_read",
                        "Pinned search order or selection changed before Tab",
                    ));
                }
            }
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
            stage::Candidate::SearchQuery(query)
                if target.stage.types_query() && target.query.as_ref() == Some(query) =>
            {
                let mut may_queue = false;
                let result =
                    self.controls
                        .send_query_until_checked(process, query, deadline, || {
                            fresh_check(guard, &mut pin)?;
                            may_queue = true;
                            Ok(())
                        });
                let delivery = match &result {
                    Ok(()) => stage::Delivery::ServerConfirmed,
                    Err(_) if !may_queue => stage::Delivery::NotQueued,
                    Err(_) => stage::Delivery::Unknown,
                };
                self.last_transport = Some(
                    json!({"delivery":delivery,"ordinary_text":true,"characters":query.len(),"error":result.err(),"preinput_rejected_reads":rejected_reads,"game_action_verified":false}),
                );
                delivery
            }
            stage::Candidate::SearchNext(matches)
                if target.stage == stage::Stage::SearchNext
                    && self.search_selection.as_ref() == Some(matches) =>
            {
                let mut may_queue = false;
                let result =
                    self.controls
                        .send_search_next_until_checked(process, deadline, || {
                            fresh_check(guard, &mut pin)?;
                            may_queue = true;
                            Ok(())
                        });
                let delivery = match &result {
                    Ok(()) => stage::Delivery::ServerConfirmed,
                    Err(_) if !may_queue => stage::Delivery::NotQueued,
                    Err(_) => stage::Delivery::Unknown,
                };
                self.last_transport = Some(json!({"delivery":delivery,"ordinary_key":"Tab",
                    "error":result.err(),"preinput_rejected_reads":rejected_reads,"game_action_verified":false}));
                delivery
            }
            candidate @ (stage::Candidate::OpenPolitics
            | stage::Candidate::ClosePolitics
            | stage::Candidate::OpenPoliticsFromResearch)
                if matches!(
                    (candidate, target.stage),
                    (stage::Candidate::OpenPolitics, stage::Stage::OpenPolitics)
                        | (stage::Candidate::ClosePolitics, stage::Stage::ClosePolitics)
                        | (
                            stage::Candidate::OpenPoliticsFromResearch,
                            stage::Stage::OpenPoliticsFromResearch
                        )
                ) =>
            {
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
            stage::Candidate::Point(original)
                if !target.stage.is_shortcut() && !target.stage.uses_query() =>
            {
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
            stage::Candidate::SearchQuery(query) => {
                self.controls.preflight_query(self.process, query)
            }
            stage::Candidate::SearchNext(_) => self.controls.preflight_search_next(self.process),
            stage::Candidate::OpenPolitics
            | stage::Candidate::ClosePolitics
            | stage::Candidate::OpenPoliticsFromResearch => self
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
    fn search_text_and_next_require_query_but_accept_no_caller_match_order() {
        for (name, expected) in [
            ("search_text", stage::Stage::SearchText),
            ("search_next", stage::Stage::SearchNext),
        ] {
            let value = json!({"stage":name,"focus_id":"GER_the_four_year_plan","request_id":"once","lease_token":"t","query":"four year plan"});
            assert_eq!(parse(value.as_object().unwrap()).unwrap().stage, expected);
            for extra in ["matches", "selected", "key", "repeat", "x", "y"] {
                let mut forged = value.clone();
                forged[extra] = json!(true);
                assert!(parse(forged.as_object().unwrap()).is_err());
            }
            let mut missing = value;
            missing.as_object_mut().unwrap().remove("query");
            assert!(parse(missing.as_object().unwrap()).is_err());
        }
    }
    #[test]
    fn search_query_is_explicit_bounded_and_rejected_on_other_stages() {
        let value = json!({"stage":"search_query","focus_id":"GER_the_four_year_plan","request_id":"query-once","lease_token":"t","query":"four year plan"});
        let p = parse(value.as_object().unwrap()).unwrap();
        assert_eq!(p.stage, stage::Stage::SearchQuery);
        assert_eq!(p.query, Some("four year plan"));
        for query in [
            Value::Null,
            json!(""),
            json!("Four Year Plan"),
            json!("a\n"),
            json!("a".repeat(49)),
        ] {
            let mut bad = value.clone();
            bad["query"] = query;
            assert!(parse(bad.as_object().unwrap()).is_err());
        }
        let mut bad = value.clone();
        bad.as_object_mut().unwrap().remove("query");
        assert!(parse(bad.as_object().unwrap()).is_err());
        let mut bad = value;
        bad["stage"] = json!("search_field");
        assert!(parse(bad.as_object().unwrap()).is_err());
    }
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
    fn close_politics_is_typed_and_rejects_caller_supplied_input_or_state() {
        let value = json!({"stage":"close_politics","focus_id":"GER_rhineland","request_id":"close-once","lease_token":"t"});
        let parsed = parse(value.as_object().unwrap()).unwrap();
        assert_eq!(parsed.stage, stage::Stage::ClosePolitics);
        assert_eq!(parsed.key, "GER_rhineland");
        for extra in [
            "key",
            "x",
            "y",
            "managed_mode",
            "active_focus",
            "input_authorized",
        ] {
            let mut forged = value.clone();
            forged[extra] = json!(true);
            assert_eq!(
                parse(forged.as_object().unwrap()).err().unwrap().code,
                "invalid_arguments"
            );
        }
        assert!(tool()["inputSchema"]["properties"]["stage"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("close_politics")));
    }
    #[test]
    fn research_return_parser_has_no_caller_supplied_panel_authority() {
        let value = json!({"stage":"open_politics_from_research","focus_id":"GER_rhineland","request_id":"return-once","lease_token":"t"});
        assert_eq!(
            parse(value.as_object().unwrap()).unwrap().stage,
            stage::Stage::OpenPoliticsFromResearch
        );
        for extra in [
            "research_panel_root",
            "research_tree_root",
            "managed_mode",
            "key",
            "x",
            "input_authorized",
        ] {
            let mut forged = value.clone();
            forged[extra] = json!(true);
            assert_eq!(
                parse(forged.as_object().unwrap()).err().unwrap().code,
                "invalid_arguments"
            );
        }
        assert!(tool()["inputSchema"]["properties"]["stage"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("open_politics_from_research")));
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
