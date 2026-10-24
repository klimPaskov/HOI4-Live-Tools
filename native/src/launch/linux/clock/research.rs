//! Narrow ordinary research preview using the parent's exact controller/lease.
use super::*;
#[allow(dead_code)] // Internal qualification; no tool/worker route is enabled.
mod transit;
use crate::{
    platform::x11::{PointerDelivery, PointerDisplay},
    research,
    research_candidate_observation::{self as candidate, BuiltinClient},
    research_stage as stage, research_ui,
};

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
    profile: candidate::OwnedClientProfile,
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
            journal: stage::Journal::open(&directory.join("research-actions.jsonl"))?,
            profile: profile.client().owned(window),
            display: display.number,
            authority: display.xauthority.clone(),
        })
    }
}
pub(super) fn tool() -> Value {
    json!({"name":"hoi4_research_attempt","description":"Preview: one ordinary OpenPanel shortcut or ID-derived SelectSlot, SelectCategory, View or Research stage for an own empty slot. OpenPanel requires known closed views in the no-managed-view sentinel; SelectSlot requires an open own panel; SelectCategory requires an explicit public folder ID and its selected empty own tree with no Info; it makes no technology-to-folder claim. View/Research require its selected tree. Requires explicit owner profile, exclusive lease and stable stage request ID. Canonical readback verifies state; uncertain attempts are never replayed. No coordinates or engine calls.",
    "inputSchema":{"type":"object","properties":{"stage":{"enum":["open_panel","select_slot","select_category","view","research"]},"folder_id":{"type":"string","minLength":1,"maxLength":256},"technology_id":{"type":"string","minLength":1,"maxLength":256},"slot":{"type":"integer","minimum":0,"maximum":31},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["stage","slot","request_id","lease_token"],"oneOf":[{"properties":{"stage":{"const":"select_category"}},"required":["folder_id"],"not":{"required":["technology_id"]}},{"properties":{"stage":{"enum":["open_panel","select_slot","view","research"]}},"required":["technology_id"],"not":{"required":["folder_id"]}}],"additionalProperties":false}})
}
pub(super) fn close_tool() -> Value {
    json!({"name":"hoi4_research_close","description":"Preview: close the ordinary own Research panel with one stationary W shortcut, preserving original panel roots, own slot identities, assignments and selected folder. Requires settled research tab, hidden tree/facilities/Info and clear input gates. A fresh already-closed request verifies without input; successful same-ID replay returns only its historical receipt. Uncertain attempts remain quarantined and are never replayed. Uses Research permission only; no Focus assignment or permission. ResearchClose002 verified bounded closing, no-ops and historical replay on the exact Linux build/profile; mixed-worker routing and full campaigns remain unverified.",
    "inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn close_call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
) -> Result<Value> {
    owner.daily.input_check()?;
    allowed(args, &["request_id", "lease_token"])?;
    run_call(owner, process, args, stage::Stage::ClosePanel, "", None, 0)
}
pub(super) fn call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
) -> Result<Value> {
    owner.daily.input_check()?;
    allowed(
        args,
        &[
            "folder_id",
            "stage",
            "technology_id",
            "slot",
            "request_id",
            "lease_token",
        ],
    )?;
    let selected_stage = match string(args, "stage")? {
        "open_panel" => stage::Stage::OpenPanel,
        "select_slot" => stage::Stage::SelectSlot,
        "select_category" => stage::Stage::SelectCategory,
        "view" => stage::Stage::View,
        "research" => stage::Stage::Research,
        _ => return Err(Error::new("invalid_arguments", "Unknown research stage")),
    };
    let folder_id = if selected_stage == stage::Stage::SelectCategory {
        if args.contains_key("technology_id") {
            return Err(Error::new(
                "invalid_arguments",
                "SelectCategory accepts only folder ID",
            ));
        }
        let f = string(args, "folder_id")?;
        research_ui::validate_detail_key(f)?;
        Some(f.to_owned())
    } else {
        if args.contains_key("folder_id") {
            return Err(Error::new("invalid_arguments", "Unexpected folder ID"));
        }
        None
    };
    let key = if selected_stage == stage::Stage::SelectCategory {
        ""
    } else {
        let k = string(args, "technology_id")?;
        research_ui::validate_detail_key(k)?;
        k
    };
    let slot = args
        .get("slot")
        .and_then(Value::as_u64)
        .filter(|v| *v <= 31)
        .ok_or_else(|| Error::new("invalid_arguments", "Own slot index 0..31 required"))?
        as u32;
    run_call(owner, process, args, selected_stage, key, folder_id, slot)
}
fn run_call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
    selected_stage: stage::Stage,
    key: &str,
    folder_id: Option<String>,
    slot: u32,
) -> Result<Value> {
    let id = string(args, "request_id")?;
    let token = string(args, "lease_token")?;
    drop(input_permit(&owner.input_gate)?);
    let mut state = owner.input_state()?;
    let epoch = state.guard.epoch.clone();
    state.authority.authorize(token, &epoch)?;
    if state.journal.blocks_game(&epoch.game) {
        return Err(Error::new(
            "action_uncertain",
            "Unresolved clock input blocks pointer input",
        ));
    }
    let target = stage::Target {
        epoch,
        stage: selected_stage,
        technology: key.into(),
        folder_id,
        slot,
    };
    let (execution, transport) = execute_internal(
        &mut state,
        process,
        &owner.input_gate,
        token,
        id,
        target,
        Instant::now() + Duration::from_secs(5),
    )?;
    Ok(json!({"execution":execution,"transport":transport,"full_campaign_acceptance":false}))
}

/// Borrow the already-held shared controller once. Never re-enter RPC/call.
pub(super) fn execute_internal(
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    id: &str,
    target: stage::Target,
    until: Instant,
) -> Result<(stage::Execution, Option<Value>)> {
    execute_bound(state, process, gate, token, id, target, until, None)
}

/// Mixed departures retain the controller's complete original shown state.
/// The ordinary journal and transport remain the sole input path.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_bound(
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    id: &str,
    target: stage::Target,
    until: Instant,
    expected: Option<&stage::OwnState>,
) -> Result<(stage::Execution, Option<Value>)> {
    if state
        .production
        .as_ref()
        .is_some_and(|p| p.journal.blocks_game(&target.epoch.game))
        || state.journal.blocks_game(&target.epoch.game)
        || state.research_transits.blocks_game(&target.epoch.game)
        || state
            .construction
            .as_ref()
            .is_some_and(|c| c.journal.blocks_game(&target.epoch.game))
        || state
            .focus
            .as_ref()
            .is_some_and(|f| f.journal.blocks_game(&target.epoch.game))
    {
        return Err(Error::new(
            "action_uncertain",
            "Unresolved clock/focus input blocks research",
        ));
    }
    validate_expected_close(expected, &target)?;
    let until = until.min(state.authority.authorize(token, &target.epoch)?);
    let ControlState {
        controls,
        guard,
        authority,
        research,
        ..
    } = state;
    let service = research
        .as_mut()
        .ok_or_else(|| Error::new("capability_unavailable", "Research profile unavailable"))?;
    let mut backend = ResearchBackend {
        process,
        controls,
        guard,
        profile: &service.profile,
        display: service.display,
        authority: &service.authority,
        last_transport: None,
        target: Some(target.clone()),
        scope: None,
        close_original: expected.cloned(),
        open_original: None,
        expected_source: expected.cloned(),
        read_deadline: until,
        before_observations: 0,
        dispatch_started: false,
        rejected_close_reads: Vec::new(),
    };
    let timeout = until
        .saturating_duration_since(Instant::now())
        .min(Duration::from_secs(5));
    if timeout.is_zero() {
        return Err(Error::new(
            "input_not_queued",
            "Research budget ended before journal execution",
        ));
    }
    let result = service
        .journal
        .execute(id, target, token, authority, gate, &mut backend, timeout);
    if !backend.rejected_close_reads.is_empty() {
        let transport = backend
            .last_transport
            .get_or_insert_with(|| json!({"delivery":"not_attempted"}));
        transport["discarded_close_reads"] = json!(backend.rejected_close_reads);
    }
    let execution = result.map_err(|mut error| {
        if !backend.rejected_close_reads.is_empty() {
            error.message.push_str(&format!(
                "; discarded_close_reads={}",
                json!(backend.rejected_close_reads)
            ));
        }
        transport_failure(error, backend.last_transport.as_ref())
    })?;
    Ok((execution, backend.last_transport))
}

/// Error-only diagnostics. Delivery is not a game effect, receipt or retry right.
fn transport_failure(mut error: Error, transport: Option<&Value>) -> Error {
    let delivery = match transport.and_then(|v| v["delivery"].as_str()) {
        Some("server_confirmed" | "ServerConfirmed") => "server_confirmed",
        Some("not_queued" | "NotQueued") => "not_queued",
        Some("unknown" | "Unknown") => "unknown",
        Some("not_attempted") => "not_attempted",
        _ => return error,
    };
    let mut message = format!("transport_delivery={delivery}; game_action_verified=false; ");
    for c in error.message.chars().filter(|c| !c.is_control()) {
        if message.len() + c.len_utf8() > 512 {
            break;
        }
        message.push(c);
    }
    error.message = message;
    error
}

/// Called only immediately after execute_internal returned its actual error.
pub(super) fn failure_evidence(
    state: &ControlState,
    id: &str,
    target: &stage::Target,
    error: &Error,
) -> stage::FailureEvidence {
    if error.code == "action_uncertain"
        || state
            .production
            .as_ref()
            .is_some_and(|p| p.journal.blocks_game(&target.epoch.game))
        || state.journal.blocks_game(&target.epoch.game)
        || state.research_transits.blocks_game(&target.epoch.game)
        || state
            .construction
            .as_ref()
            .is_some_and(|c| c.journal.blocks_game(&target.epoch.game))
        || state
            .focus
            .as_ref()
            .is_some_and(|f| f.journal.blocks_game(&target.epoch.game))
    {
        return stage::FailureEvidence::Unknown;
    }
    match &state.research {
        Some(service) => service.journal.failure_evidence(id, target, error),
        None if error.code == "capability_unavailable" => stage::FailureEvidence::BeforeIntent,
        None => stage::FailureEvidence::Unknown,
    }
}
struct ResearchBackend<'a> {
    process: &'a dyn Process,
    controls: &'a mut X11Controls,
    guard: &'a mut Guard,
    profile: &'a candidate::OwnedClientProfile,
    display: u16,
    authority: &'a Path,
    last_transport: Option<Value>,
    target: Option<stage::Target>,
    scope: Option<([u64; 2], Vec<u64>)>,
    close_original: Option<stage::OwnState>,
    open_original: Option<research_ui::opening::Binding>,
    expected_source: Option<stage::OwnState>,
    read_deadline: Instant,
    before_observations: usize,
    dispatch_started: bool,
    rejected_close_reads: Vec<Value>,
}

fn visible(window: &Value) -> Result<bool> {
    match (
        window["materialized"].as_bool(),
        window["visible_predicate"].as_bool(),
    ) {
        (Some(false), None) if window["visible_predicate"].is_null() => Ok(false),
        (Some(false), Some(false)) => Ok(false),
        (Some(true), Some(value)) => Ok(value),
        _ => Err(Error::new(
            "research_precondition",
            "Typed window visibility unavailable",
        )),
    }
}

fn own_state(epoch: Epoch, research: &Value, ui: &Value) -> Result<stage::OwnState> {
    let bad = || {
        Error::new(
            "research_precondition",
            "Complete typed own research/UI state required",
        )
    };
    if research["owned"] != true
        || ui["owned"] != true
        || research["build_id"] != game::BUILD_ID
        || ui["build_id"] != game::BUILD_ID
    {
        return Err(bad());
    }
    let slots = research["slots"]
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or_else(bad)?;
    if ui["own_slot_count"].as_u64() != Some(slots.len() as u64) {
        return Err(bad());
    }
    let parse_key = |v: &Value| -> Result<Option<String>> {
        if v.is_null() {
            return Ok(None);
        }
        let key = v.as_str().ok_or_else(bad)?;
        research_ui::validate_detail_key(key)?;
        Ok(Some(key.into()))
    };
    let mut researching = Vec::new();
    for (index, slot) in slots.iter().enumerate() {
        if slot["slot"].as_u64() != Some(index as u64) {
            return Err(bad());
        }
        researching.push(parse_key(&slot["researching"])?);
    }
    let selected_slot = if ui["selected_slot"].is_null() {
        None
    } else {
        Some(
            ui["selected_slot"]
                .as_u64()
                .filter(|v| *v < slots.len() as u64)
                .ok_or_else(bad)? as u32,
        )
    };
    let tree = visible(&ui["ordinary_tree"])?;
    let panel = visible(&ui["panel"])?;
    let active = ui["active_mode_raw"].as_i64().ok_or_else(bad)?;
    if (panel && active != 5) || (tree && active != 6) || (panel && tree) {
        return Err(bad());
    }
    let windows = ui["technology_info"]["windows"]
        .as_array()
        .filter(|v| v.len() <= 64)
        .ok_or_else(bad)?;
    let mut visible_info = Vec::new();
    for window in windows {
        if visible(&window["window"])? {
            visible_info.push(parse_key(&window["technology"])?);
        }
    }
    if ui["technology_info"]["visible_count"].as_u64() != Some(visible_info.len() as u64) {
        return Err(bad());
    }
    Ok(stage::OwnState {
        epoch,
        selected_slot,
        selected_folder: None,
        tree_visible: active == 6 && tree && !panel,
        panel_visible: Some(active == 5 && panel && !tree),
        active_mode: Some(i32::try_from(active).map_err(|_| bad())?),
        researching,
        visible_info,
        panel_binding: None,
    })
}
impl stage::Backend for ResearchBackend<'_> {
    fn confirm_input_idle(
        &mut self,
        candidate: &stage::Candidate,
        deadline: Instant,
    ) -> Result<()> {
        let stage::Candidate::Point(point) = candidate else {
            if Instant::now() >= deadline {
                return Err(Error::new("research_deadline", "Input idle deadline ended"));
            }
            return self
                .controls
                .preflight(self.process, &ControlKey::ResearchPanel);
        };
        // Preparation is read-only and requires clear keys/modifiers/buttons.
        // Dropping the prepared value sends no pointer event.
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
    fn observe(&mut self) -> Result<stage::OwnState> {
        let (state, context) = if !self.dispatch_started {
            let phase = if self.before_observations == 0 {
                "initial"
            } else {
                "final"
            };
            self.before_observations += 1;
            preinput_state(
                self.process,
                self.guard,
                self.target.as_ref(),
                self.read_deadline,
                phase,
                &mut self.rejected_close_reads,
            )?
        } else {
            observed_target_state(self.process, self.guard, self.target.as_ref())?
        };
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.stage == stage::Stage::OpenPanel)
            && state.active_mode == Some(5)
        {
            // Generic visibility permits transition1. A fresh ordinary Open
            // result must instead prove its own literal root settled shown.
            // Pin even a known-pending whole snapshot before classifying it.
            let opening = research_ui::opening::observe(self.process, &context)?;
            self.guard
                .accept_context(self.process, &opening.context.campaign)?;
            pin_open(&mut self.open_original, &opening.binding)?;
            opening.require_settled_shown()?;
        }
        if !self.dispatch_started {
            require_expected_close(self.expected_source.as_ref(), &state)?;
        }
        pin_scope(&mut self.scope, &context)?;
        pin_close(&mut self.close_original, &state, self.target.as_ref())?;
        Ok(state)
    }
    fn candidate(&mut self, target: &stage::Target) -> Result<stage::Candidate> {
        self.target = Some(target.clone());
        self.guard.observe(self.process)?;
        if matches!(
            target.stage,
            stage::Stage::OpenPanel | stage::Stage::ClosePanel
        ) {
            let (fresh, context) = preinput_state(
                self.process,
                self.guard,
                Some(target),
                self.read_deadline,
                "candidate",
                &mut self.rejected_close_reads,
            )?;
            pin_scope(&mut self.scope, &context)?;
            pin_close(&mut self.close_original, &fresh, Some(target))?;
            fresh.validate(target)?;
            fresh.preflight(target)?;
            self.controls
                .preflight(self.process, &ControlKey::ResearchPanel)?;
            return Ok(if target.stage == stage::Stage::ClosePanel {
                stage::Candidate::ClosePanel
            } else {
                stage::Candidate::OpenPanel
            });
        }
        let (point, context) = match target.stage {
            stage::Stage::OpenPanel | stage::Stage::ClosePanel => {
                unreachable!("Fixed shortcut handled above")
            }
            stage::Stage::SelectSlot => {
                candidate::observe_slot_owned(self.process, target.slot, self.profile)?
            }
            stage::Stage::SelectCategory => candidate::observe_category_owned(
                self.process,
                target
                    .folder_id
                    .as_deref()
                    .ok_or_else(|| Error::new("invalid_arguments", "Folder ID required"))?,
                target.slot,
                self.profile,
            )?,
            stage::Stage::View | stage::Stage::Research => {
                let which = if target.stage == stage::Stage::View {
                    candidate::Stage::View
                } else {
                    candidate::Stage::Research
                };
                candidate::observe_owned(
                    self.process,
                    &target.technology,
                    target.slot,
                    which,
                    self.profile,
                )?
            }
        };
        self.guard.accept_context(self.process, &context.campaign)?;
        check_selected_destination(self.process, self.guard, self.read_deadline)?;
        if self.scope.as_ref() != Some(&([context.anchors[0], context.anchors[1]], context.slots)) {
            return Err(Error::new(
                "torn_read",
                "Candidate own status or full slot identities changed",
            ));
        }
        Ok(stage::Candidate::Point(point))
    }
    fn dispatch_once(
        &mut self,
        candidate: &stage::Candidate,
        deadline: Instant,
    ) -> stage::Delivery {
        self.dispatch_started = true;
        if let Err(error) = self.guard.observe(self.process) {
            self.last_transport = Some(json!({"delivery":"not_queued","error":error}));
            return stage::Delivery::NotQueued;
        }
        let Some(target) = self.target.clone() else {
            return stage::Delivery::NotQueued;
        };
        let guard = &mut *self.guard;
        let process = self.process;
        let mut scope = self.scope.clone();
        let mut close_original = self.close_original.clone();
        let rejected_close_reads = &mut self.rejected_close_reads;
        let stage::Candidate::Point(point) = candidate else {
            if !matches!(
                (target.stage, candidate),
                (stage::Stage::OpenPanel, stage::Candidate::OpenPanel)
                    | (stage::Stage::ClosePanel, stage::Candidate::ClosePanel)
            ) {
                return stage::Delivery::NotQueued;
            }
            if let Err(error) = self.controls.preflight(process, &ControlKey::ResearchPanel) {
                self.last_transport = Some(json!({"delivery":"not_queued","error":error}));
                return stage::Delivery::NotQueued;
            }
            let mut input_may_queue = false;
            let result = self.controls.send_until_checked(
                process,
                ControlKey::ResearchPanel,
                deadline,
                || {
                    let (fresh, context) = preinput_state(
                        process,
                        guard,
                        Some(&target),
                        deadline,
                        "pre_queue",
                        rejected_close_reads,
                    )?;
                    pin_scope(&mut scope, &context)?;
                    pin_close(&mut close_original, &fresh, Some(&target))?;
                    fresh.validate(&target)?;
                    fresh.preflight(&target)?;
                    input_may_queue = true;
                    Ok(true)
                },
            );
            let (delivery, diagnostic) = match result {
                Ok(true) => (
                    stage::Delivery::ServerConfirmed,
                    json!({"delivery":"server_confirmed","ordinary_key":"W","game_action_verified":false,"event_causality_verified":false}),
                ),
                Ok(false) => (stage::Delivery::NotQueued, json!({"delivery":"not_queued"})),
                Err(error) if !input_may_queue => (
                    stage::Delivery::NotQueued,
                    json!({"delivery":"not_queued","error":error}),
                ),
                Err(error) => (
                    stage::Delivery::Unknown,
                    json!({"delivery":"unknown","error":error}),
                ),
            };
            self.last_transport = Some(diagnostic);
            return delivery;
        };
        let prepared = self.controls.prepare_left_click_until(
            self.process,
            PointerDisplay {
                display: self.display,
                authority: self.authority,
            },
            point.client_pixel,
            point.expected_client,
            deadline,
        );
        match prepared {
            Err(error) => {
                self.last_transport = Some(json!({"delivery":"not_queued","error":error}));
                stage::Delivery::NotQueued
            }
            Ok(mut click) => {
                let result = click.dispatch_once_checked(|| {
                    let (fresh, context) =
                        observed_target_state_unpinned(process, guard, Some(&target))?;
                    check_selected_destination(process, guard, deadline)?;
                    pin_scope(&mut scope, &context)?;
                    fresh.validate(&target)?;
                    fresh.preflight(&target)
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
}

fn pin_open(
    original: &mut Option<research_ui::opening::Binding>,
    current: &research_ui::opening::Binding,
) -> Result<()> {
    if original.as_ref().is_some_and(|before| before != current) {
        return Err(Error::new(
            "ownership_changed",
            "Original own Research opening root identity changed; never rebind",
        ));
    }
    if original.is_none() {
        *original = Some(current.clone());
    }
    Ok(())
}

fn validate_expected_close(
    expected: Option<&stage::OwnState>,
    target: &stage::Target,
) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if target.stage != stage::Stage::ClosePanel {
        return Err(Error::new(
            "invalid_arguments",
            "Expected Research source belongs only to ClosePanel",
        ));
    }
    expected.validate(target)?;
    // An already-closed state is valid ordinary close no-op evidence, but may
    // never replace the mixed controller's originally shown departure source.
    expected.preflight(target)
}

fn require_expected_close(
    expected: Option<&stage::OwnState>,
    current: &stage::OwnState,
) -> Result<()> {
    if expected.is_some_and(|original| original != current) {
        return Err(Error::new(
            "research_precondition",
            "Original mixed departure Research source changed before Intent",
        ));
    }
    Ok(())
}

/// The first complete close observation remains the original throughout all
/// candidate and final pre-queue reads. Never silently rebind a same-shaped UI.
fn pin_close(
    original: &mut Option<stage::OwnState>,
    current: &stage::OwnState,
    target: Option<&stage::Target>,
) -> Result<()> {
    let Some(target) = target.filter(|t| t.stage == stage::Stage::ClosePanel) else {
        return Ok(());
    };
    current.validate(target)?;
    if let Some(before) = original {
        current.preserves_close(before)?;
    } else {
        *original = Some(current.clone());
    }
    Ok(())
}

fn pin_scope(
    pin: &mut Option<([u64; 2], Vec<u64>)>,
    context: &research_ui::UiContext,
) -> Result<()> {
    let actual = (
        [context.anchors[0], context.anchors[1]],
        context.slots.clone(),
    );
    if pin.as_ref().is_some_and(|previous| previous != &actual) {
        return Err(Error::new(
            "ownership_changed",
            "Pinned own status or full slot identities changed during attempt",
        ));
    }
    if pin.is_none() {
        *pin = Some(actual);
    }
    Ok(())
}

pub(super) fn observed_state(
    process: &dyn Process,
    guard: &mut Guard,
) -> Result<(stage::OwnState, research_ui::UiContext)> {
    let (research, research_context) = research::owned_snapshot(process)?;
    guard.accept_context(process, &research_context.campaign)?;
    let (ui, ui_context) = research_ui::wait_snapshot(process)?;
    guard.accept_context(process, &ui_context.campaign)?;
    if research_context.own != ui_context.anchors[..2] || research_context.slots != ui_context.slots
    {
        return Err(Error::new(
            "torn_read",
            "Canonical research and UI own-slot identities differ",
        ));
    }
    let state = own_state(guard.epoch.clone(), &research, &ui)?;
    Ok((state, ui_context))
}

pub(super) fn close_state(
    process: &dyn Process,
    guard: &mut Guard,
    observation: research_ui::closing::Observation,
) -> Result<(stage::OwnState, research_ui::UiContext)> {
    guard.accept_context(process, &observation.context.campaign)?;
    let mut state = own_state(
        guard.epoch.clone(),
        &observation.body["canonical_research"],
        &observation.body,
    )?;
    state.selected_folder = observation.selected_folder;
    state.panel_binding = Some(observation.binding);
    Ok((state, observation.context))
}
fn preinput_state(
    process: &dyn Process,
    guard: &mut Guard,
    target: Option<&stage::Target>,
    until: Instant,
    phase: &'static str,
    history: &mut Vec<Value>,
) -> Result<(stage::OwnState, research_ui::UiContext)> {
    let observed = if target.is_some_and(|t| t.stage == stage::Stage::ClosePanel) {
        // Only the complete raw producer is retried. Ownership acceptance,
        // original-state pinning and all predicates remain outside recovery.
        let observation = stage::close_read_recovery::observe(
            phase,
            until,
            || research_ui::closing::observe(process),
            history,
        )?;
        close_state(process, guard, observation)?
    } else {
        observed_target_state_unpinned(process, guard, target)?
    };
    check_selected_destination(process, guard, until)?;
    Ok(observed)
}

pub(super) fn observed_target_state(
    process: &dyn Process,
    guard: &mut Guard,
    target: Option<&stage::Target>,
) -> Result<(stage::OwnState, research_ui::UiContext)> {
    let observed = observed_target_state_unpinned(process, guard, target)?;
    check_selected_research(process, guard)?;
    Ok(observed)
}

fn check_selected_destination(
    process: &dyn Process,
    guard: &mut Guard,
    until: Instant,
) -> Result<()> {
    if Instant::now() >= until {
        return Err(Error::new(
            "research_deadline",
            "Selected destination identity deadline ended",
        ));
    }
    check_selected_research(process, guard)?;
    production::check_selected_production(process, guard, None, None, until)?;
    if Instant::now() >= until {
        return Err(Error::new(
            "research_deadline",
            "Selected destination identity deadline ended",
        ));
    }
    Ok(())
}

pub(super) fn check_selected_research(process: &dyn Process, guard: &mut Guard) -> Result<()> {
    let Some(crate::daily_panel_navigation::Destination::Research { state }) =
        &guard.selected_target
    else {
        return Ok(());
    };
    if state.epoch != guard.epoch {
        return Err(Error::new(
            "ownership_changed",
            "Selected Research epoch changed",
        ));
    }
    let original = state.panel_binding.clone().ok_or_else(|| {
        Error::new(
            "research_precondition",
            "Original selected Research binding required",
        )
    })?;
    let identity = research_ui::mixed_destination::observe_identity(process)?;
    guard.accept_context(process, &identity.context.campaign)?;
    require_selected_research(&original, &identity)
}

fn require_selected_research(
    original: &stage::PanelBinding,
    current: &research_ui::mixed_destination::Identity,
) -> Result<()> {
    if original.anchors != current.context.anchors
        || original.slots != current.context.slots
        || original.roots != current.roots
        || original.facilities_parent != current.facilities_parent
    {
        return Err(Error::new(
            "ownership_changed",
            "Original selected Research identity changed before action",
        ));
    }
    Ok(())
}

fn observed_target_state_unpinned(
    process: &dyn Process,
    guard: &mut Guard,
    target: Option<&stage::Target>,
) -> Result<(stage::OwnState, research_ui::UiContext)> {
    if target.is_some_and(|t| t.stage == stage::Stage::ClosePanel) {
        return close_state(process, guard, research_ui::closing::observe(process)?);
    }
    if let Some(t) = target.filter(|t| t.stage == stage::Stage::SelectCategory) {
        let (body, context) = research_ui::category_command_owned(
            process,
            t.folder_id
                .as_deref()
                .ok_or_else(|| Error::new("invalid_arguments", "Folder ID required"))?,
        )?;
        guard.accept_context(process, &context.campaign)?;
        if body["owned"] != true
            || body["build_id"] != game::BUILD_ID
            || body["selected_slot"].as_u64() != Some(u64::from(t.slot))
            || body["category"]["binding_available"] != true
        {
            return Err(Error::new(
                "research_precondition",
                "Exact own category association required",
            ));
        }
        let selected = body["category"]["selected_folder_key"]
            .as_str()
            .ok_or_else(|| Error::new("research_precondition", "Selected folder unknown"))?;
        let empty = body["category"]["selected_slot_empty"]
            .as_bool()
            .ok_or_else(|| {
                Error::new("research_precondition", "Selected slot emptiness unknown")
            })?;
        let latest = own_state(guard.epoch.clone(), &body["canonical_research"], &body)?;
        let state = stage::join_category_body(&latest, latest.clone(), t, empty, selected)?;
        return Ok((state, context));
    }
    observed_state(process, guard)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 1,
                start_ticks: "2".into(),
                exe: "/fixture/game".into(),
                uid: 1000,
            },
            campaign: "test".into(),
            player: "GER".into(),
        }
    }
    fn shown_source() -> stage::OwnState {
        stage::OwnState {
            epoch: epoch(),
            selected_slot: Some(0),
            selected_folder: None,
            tree_visible: false,
            panel_visible: Some(true),
            active_mode: Some(5),
            researching: vec![None, Some("support_weapons".into())],
            visible_info: vec![],
            panel_binding: Some(stage::PanelBinding {
                anchors: [0x10000, 0x20000, 0x30000, 0x40000, 0x50000, 0x60000],
                slots: vec![0x70000, 0x80000],
                roots: [0x90000, 0xa0000, 0xb0000, 0xc0000],
                facilities_parent: 0xd0000,
                selected_slot_pointer: 0x70000,
                folder_index: 0,
                folder_windows: vec![],
                tab: 0,
            }),
        }
    }
    #[test]
    fn uncertain_readback_keeps_delivery_diagnostic_without_success_or_retry() {
        let original = Error::after_possible_input(
            "Readback deadline/authority ended; never replay",
            Error::new("research_deadline", "Research attempt deadline reached"),
        );
        for delivery in ["server_confirmed", "ServerConfirmed", "unknown", "Unknown"] {
            let error = transport_failure(
                original_error(&original),
                Some(&json!({"delivery":delivery})),
            );
            assert_eq!(error.code, "action_uncertain");
            assert!(error.message.contains("cause=research_deadline"));
            assert!(error.message.contains("game_action_verified=false"));
            assert!(!error.message.contains("game_action_verified=true"));
            assert!(error.message.contains("never replay"));
        }
    }
    fn original_error(error: &Error) -> Error {
        Error::new(error.code, error.message.clone())
    }
    #[test]
    fn preinput_or_absent_transport_never_changes_error_category() {
        let error = transport_failure(
            Error::new("input_not_queued", "local refusal"),
            Some(&json!({"delivery":"not_queued"})),
        );
        assert_eq!(error.code, "input_not_queued");
        assert!(error.message.contains("transport_delivery=not_queued"));
        let error = transport_failure(Error::new("ownership_changed", "original scope"), None);
        assert_eq!(error.code, "ownership_changed");
        assert_eq!(error.message, "original scope");
        let error = transport_failure(
            Error::new("action_uncertain", "preserve"),
            Some(&json!({"delivery":"invented-success"})),
        );
        assert_eq!(error.message, "preserve");
    }
    #[test]
    fn transport_error_diagnostics_are_bounded_and_single_line() {
        let error = transport_failure(
            Error::new(
                "action_uncertain",
                "readback\n".to_owned() + &"é".repeat(600),
            ),
            Some(&json!({"delivery":"server_confirmed"})),
        );
        assert_eq!(error.code, "action_uncertain");
        assert!(error.message.len() <= 512);
        assert!(!error.message.chars().any(char::is_control));
        assert!(error.message.starts_with(
            "transport_delivery=server_confirmed; game_action_verified=false; readback"
        ));
    }
    #[test]
    fn opening_pending_root_binding_is_never_replaced_before_settlement() {
        let binding = research_ui::opening::Binding {
            anchors: [0x10000, 0x20000, 0x30000, 0x40000, 0x50000, 0x60000],
            slots: vec![0x70000, 0x80000],
            root: 0x90000,
        };
        let mut pin = None;
        pin_open(&mut pin, &binding).unwrap();
        for changed in [
            {
                let mut b = binding.clone();
                b.root += 8;
                b
            },
            {
                let mut b = binding.clone();
                b.anchors[4] += 8;
                b
            },
            {
                let mut b = binding.clone();
                b.slots[1] += 8;
                b
            },
        ] {
            assert_eq!(
                pin_open(&mut pin, &changed).unwrap_err().code,
                "ownership_changed"
            );
            assert_eq!(pin.as_ref(), Some(&binding));
        }
        pin_open(&mut pin, &binding).unwrap();
    }
    fn departure_target() -> stage::Target {
        stage::Target {
            epoch: epoch(),
            stage: stage::Stage::ClosePanel,
            technology: String::new(),
            folder_id: None,
            slot: 0,
        }
    }
    #[test]
    fn mixed_close_expected_source_requires_shown_original_before_noop_or_intent() {
        let shown = shown_source();
        let target = departure_target();
        validate_expected_close(Some(&shown), &target).unwrap();
        require_expected_close(Some(&shown), &shown).unwrap();
        let mut closed = shown.clone();
        closed.active_mode = Some(23);
        closed.panel_visible = Some(false);
        assert!(closed.reached(&target));
        assert!(validate_expected_close(Some(&closed), &target).is_err());
        assert!(require_expected_close(Some(&shown), &closed).is_err());
        // Readback may reach closed only after dispatch. The immutable pin
        // still accepts that documented transition without rebinding roots.
        let mut pin = Some(shown.clone());
        pin_close(&mut pin, &closed, Some(&target)).unwrap();
        assert_eq!(pin.as_ref(), Some(&shown));
        validate_expected_close(None, &target).unwrap();
        require_expected_close(None, &closed).unwrap();
        let mut wrong = target.clone();
        wrong.stage = stage::Stage::OpenPanel;
        wrong.technology = "support_weapons".into();
        assert_eq!(
            validate_expected_close(Some(&shown), &wrong)
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        let mut missing = shown.clone();
        missing.panel_binding = None;
        assert!(validate_expected_close(Some(&missing), &target).is_err());
        for changed in [
            {
                let mut s = shown.clone();
                s.researching[1] = None;
                s
            },
            {
                let mut s = shown.clone();
                s.selected_slot = None;
                s
            },
            {
                let mut s = shown.clone();
                s.panel_binding.as_mut().unwrap().roots[0] += 8;
                s
            },
            {
                let mut s = shown.clone();
                s.epoch.campaign = "changed".into();
                s
            },
        ] {
            assert!(require_expected_close(Some(&shown), &changed).is_err());
        }
    }
    #[test]
    fn selected_research_static_pin_preserves_all_anchors_slots_and_roots() {
        let shown = shown_source();
        let binding = shown.panel_binding.as_ref().unwrap();
        let current = research_ui::mixed_destination::Identity {
            context: research_ui::UiContext {
                campaign: ClockContext {
                    playthrough_id: "fixture".into(),
                    player: "GER".into(),
                    anchors: [0xe0000, 0xf0000, 0x100000],
                    date_raw: 1,
                    difficulty: 2,
                    historical_raw: 1,
                    paused: false,
                    speed: 3,
                },
                anchors: binding.anchors,
                slots: binding.slots.clone(),
            },
            roots: binding.roots,
            facilities_parent: binding.facilities_parent,
        };
        require_selected_research(binding, &current).unwrap();
        for index in 0..6 {
            let mut changed = current.clone();
            changed.context.anchors[index] += 8;
            assert!(require_selected_research(binding, &changed).is_err());
        }
        for index in 0..4 {
            let mut changed = current.clone();
            changed.roots[index] += 8;
            assert!(require_selected_research(binding, &changed).is_err());
        }
        let mut changed = current.clone();
        changed.context.slots[1] += 8;
        assert!(require_selected_research(binding, &changed).is_err());
        changed = current;
        changed.facilities_parent += 8;
        assert!(require_selected_research(binding, &changed).is_err());
    }
    #[test]
    fn attempt_pin_never_rebinds_to_a_same_count_slot_or_status() {
        let campaign = ClockContext {
            playthrough_id: "fixture".into(),
            player: "GER".into(),
            anchors: [10, 20, 30],
            date_raw: 1,
            difficulty: 2,
            historical_raw: 1,
            paused: true,
            speed: 0,
        };
        let context = research_ui::UiContext {
            campaign,
            anchors: [100, 200, 300, 400, 500, 600],
            slots: vec![700, 800],
        };
        let mut pin = None;
        pin_scope(&mut pin, &context).unwrap();
        let original = pin.clone();
        let mut changed = context.clone();
        changed.slots[0] = 701;
        assert_eq!(
            pin_scope(&mut pin, &changed).unwrap_err().code,
            "ownership_changed"
        );
        assert_eq!(pin, original);
        changed = context.clone();
        changed.anchors[1] = 201;
        assert_eq!(
            pin_scope(&mut pin, &changed).unwrap_err().code,
            "ownership_changed"
        );
        assert_eq!(pin, original);
        pin_scope(&mut pin, &context).unwrap();
    }
    fn values() -> (Value, Value) {
        (
            json!({"owned":true,"build_id":game::BUILD_ID,"slots":[{"slot":0,"researching":null}]}),
            json!({"owned":true,"build_id":game::BUILD_ID,"own_slot_count":1,"selected_slot":0,"active_mode_raw":6,
         "ordinary_tree":{"materialized":true,"visible_predicate":true},"panel":{"materialized":false,"visible_predicate":null},
         "technology_info":{"visible_count":0,"windows":[{"technology":null,"window":{"materialized":false,"visible_predicate":null}}]}}),
        )
    }
    #[test]
    fn unmaterialized_retained_windows_are_known_not_visible() {
        let (r, u) = values();
        let state = own_state(epoch(), &r, &u).unwrap();
        assert!(state.tree_visible);
        assert!(state.visible_info.is_empty());
        assert_eq!(state.researching, vec![None]);
        assert!(visible(&json!({"materialized":true,"visible_predicate":null})).is_err());
        assert!(visible(&json!({"visible_predicate":false})).is_err());
        assert!(visible(&json!({"materialized":false,"visible_predicate":true})).is_err());
    }
    #[test]
    fn all_visible_infos_and_complete_canonical_slot_indices_are_preserved() {
        let (mut r, mut u) = values();
        u["technology_info"] = json!({"visible_count":1,"windows":[{"technology":"support_weapons","window":{"materialized":true,"visible_predicate":true}}]});
        let s = own_state(epoch(), &r, &u).unwrap();
        assert_eq!(s.visible_info, vec![Some("support_weapons".into())]);
        r["slots"][0]["researching"] = json!("support_weapons");
        assert_eq!(
            own_state(epoch(), &r, &u).unwrap().researching,
            vec![Some("support_weapons".into())]
        );
        r["slots"][0]["slot"] = json!(1);
        assert!(own_state(epoch(), &r, &u).is_err());
        r["slots"][0]["slot"] = json!(0);
        u["own_slot_count"] = json!(2);
        assert!(own_state(epoch(), &r, &u).is_err());
    }
    #[test]
    fn malformed_window_count_and_unsupported_profile_are_not_defaults() {
        let (r, mut u) = values();
        u["technology_info"]["visible_count"] = json!(1);
        assert!(own_state(epoch(), &r, &u).is_err());
        assert_eq!(
            serde_json::to_value(Profile::LinuxBuiltinAbsolute512x288V1).unwrap(),
            json!("linux_builtin_absolute_512x288_v1")
        );
        assert!(serde_json::from_value::<Profile>(json!("generic")).is_err());
        assert!(tool()["inputSchema"]["properties"].get("x").is_none());
    }
    #[test]
    fn normal_map_panel_opening_requires_consistent_raw_visibility() {
        let (r, mut u) = values();
        u["active_mode_raw"] = json!(23);
        assert!(own_state(epoch(), &r, &u).is_err());
        u["ordinary_tree"]["visible_predicate"] = json!(false);
        let state = own_state(epoch(), &r, &u).unwrap();
        assert_eq!(state.active_mode, Some(23));
        assert_eq!(state.panel_visible, Some(false));
        assert!(!state.tree_visible);
        u["panel"] = json!({"materialized":true,"visible_predicate":true});
        assert!(own_state(epoch(), &r, &u).is_err());
    }
}
