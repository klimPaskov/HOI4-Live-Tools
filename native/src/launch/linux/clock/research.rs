//! Narrow ordinary research preview using the parent's exact controller/lease.
use super::*;
use crate::{
    platform::x11::{PointerDelivery, PointerDisplay},
    research, research_candidate_observation as candidate, research_stage as stage, research_ui,
};

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Profile {
    #[serde(rename = "linux_builtin_absolute_512x288_v1")]
    LinuxBuiltinAbsolute512x288V1,
}
pub(super) struct Service {
    pub(super) journal: stage::Journal,
    profile: candidate::OwnedClientProfile,
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
            journal: stage::Journal::open(&directory.join("research-actions.jsonl"))?,
            profile: candidate::OwnedClientProfile {
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
    json!({"name":"hoi4_research_attempt","description":"Preview: one ordinary ID-derived SelectSlot, View or Research stage for an own empty slot. SelectSlot requires an open own research panel; View/Research require its selected tree. Requires explicit owner profile, exclusive lease and stable stage request ID. Canonical readback verifies state; uncertain attempts are never replayed. No coordinates or engine calls.",
    "inputSchema":{"type":"object","properties":{"stage":{"enum":["select_slot","view","research"]},"technology_id":{"type":"string","minLength":1,"maxLength":256},"slot":{"type":"integer","minimum":0,"maximum":31},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["stage","technology_id","slot","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
) -> Result<Value> {
    allowed(
        args,
        &[
            "stage",
            "technology_id",
            "slot",
            "request_id",
            "lease_token",
        ],
    )?;
    let selected_stage = match string(args, "stage")? {
        "select_slot" => stage::Stage::SelectSlot,
        "view" => stage::Stage::View,
        "research" => stage::Stage::Research,
        _ => return Err(Error::new("invalid_arguments", "Unknown research stage")),
    };
    let key = string(args, "technology_id")?;
    research_ui::validate_detail_key(key)?;
    let slot = args
        .get("slot")
        .and_then(Value::as_u64)
        .filter(|v| *v <= 31)
        .ok_or_else(|| Error::new("invalid_arguments", "Own slot index 0..31 required"))?
        as u32;
    let id = string(args, "request_id")?;
    let token = string(args, "lease_token")?;
    drop(input_permit(&owner.input_gate)?);
    let mut state = try_state(&owner.state)?;
    let epoch = state.guard.epoch.clone();
    state.authority.authorize(token, &epoch)?;
    if state.journal.blocks_game(&epoch.game) {
        return Err(Error::new(
            "action_uncertain",
            "Unresolved clock input blocks pointer input",
        ));
    }
    let ControlState {
        controls,
        guard,
        authority,
        research,
        ..
    } = &mut *state;
    let service = research.as_mut().ok_or_else(|| {
        Error::new(
            "capability_unavailable",
            "Owner did not opt in to research preview",
        )
    })?;
    let target = stage::Target {
        epoch,
        stage: selected_stage,
        technology: key.into(),
        slot,
    };
    let mut backend = ResearchBackend {
        process,
        controls,
        guard,
        profile: &service.profile,
        display: service.display,
        authority: &service.authority,
        last_transport: None,
        target: None,
        scope: None,
    };
    let execution = service.journal.execute(
        id,
        target,
        token,
        authority,
        &owner.input_gate,
        &mut backend,
        Duration::from_secs(5),
    )?;
    Ok(
        json!({"execution":execution,"transport":backend.last_transport,"full_campaign_acceptance":false}),
    )
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
        tree_visible: active == 6 && tree && !panel,
        panel_visible: Some(active == 5 && panel && !tree),
        researching,
        visible_info,
    })
}
impl stage::Backend for ResearchBackend<'_> {
    fn confirm_input_idle(
        &mut self,
        point: &crate::research_attempt::PointCandidate,
        deadline: Instant,
    ) -> Result<()> {
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
        let (state, context) = observed_state(self.process, self.guard)?;
        pin_scope(&mut self.scope, &context)?;
        Ok(state)
    }
    fn candidate(
        &mut self,
        target: &stage::Target,
    ) -> Result<crate::research_attempt::PointCandidate> {
        self.target = Some(target.clone());
        self.guard.observe(self.process)?;
        let (point, context) = match target.stage {
            stage::Stage::SelectSlot => {
                candidate::observe_slot_owned(self.process, target.slot, self.profile)?
            }
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
        if self.scope.as_ref() != Some(&([context.anchors[0], context.anchors[1]], context.slots)) {
            return Err(Error::new(
                "torn_read",
                "Candidate own status or full slot identities changed",
            ));
        }
        Ok(point)
    }
    fn dispatch_once(
        &mut self,
        point: &crate::research_attempt::PointCandidate,
        deadline: Instant,
    ) -> stage::Delivery {
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
                    let (fresh, context) = observed_state(process, guard)?;
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

fn observed_state(
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
}
