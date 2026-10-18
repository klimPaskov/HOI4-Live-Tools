//! Opted-in canonical construction stages on the shared ordinary controller.
use super::*;
use crate::{
    construction::controls as observation,
    construction_stage as stage,
    platform::x11::{PointerDelivery, PointerDisplay},
    research_candidate_observation::OwnedClientProfile,
};
mod state;

#[derive(Clone, Copy, Deserialize, Serialize)]
pub(super) enum Profile {
    #[serde(rename = "linux_builtin_absolute_512x288_v1")]
    LinuxBuiltinAbsolute512x288V1,
}
pub(super) struct Service {
    pub(super) journal: stage::Journal,
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
            journal: stage::Journal::open(&directory.join("construction-actions.jsonl"))?,
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
    json!({"name":"hoi4_construction_attempt",
        "description":"Preview: ensure the own construction panel is open, select one canonical ordinary building type, or use the game's own map Find view (find_open presses its f shortcut, find_query types an own state ID, find_activate presses Return to centre the camera on the verified first state row), or place: with the building selected and the camera centred on the own state, one left click at the client centre to reach the requested total queued amount of that building in that state. Already-matching state is a no-op. At most one ordinary input per durable request, verified by own canonical readback. Requires the owned profile and exclusive lease. Unknown outcomes block all input; never replay uncertain requests. The server derives every key and point itself; no caller coordinates or engine calls. Run086 accepted the Find stages and Run091 accepted place for a state building live; provincial placement is not yet live-tested.",
        "inputSchema":{"type":"object","properties":{
            "stage":{"enum":["ensure_open","select_building","find_open","find_query","find_activate","place"]},
            "amount":{"type":"integer","minimum":1,"maximum":1000},
            "building_id":{"type":"string","minLength":1,"maxLength":128},
            "state_id":{"type":"integer","minimum":1,"maximum":4095},
            "request_id":{"type":"string","minLength":1,"maxLength":128},
            "lease_token":{"type":"string"}},
            "required":["stage","request_id","lease_token"],
            "oneOf":[{"properties":{"stage":{"const":"ensure_open"}},"not":{"anyOf":[{"required":["building_id"]},{"required":["state_id"]}]}},
                {"properties":{"stage":{"const":"select_building"}},"required":["building_id"],"not":{"required":["state_id"]}},
                {"properties":{"stage":{"const":"find_open"}},"not":{"anyOf":[{"required":["building_id"]},{"required":["state_id"]}]}},
                {"properties":{"stage":{"enum":["find_query","find_activate"]}},"required":["state_id"],"not":{"anyOf":[{"required":["building_id"]},{"required":["amount"]}]}},
                {"properties":{"stage":{"const":"place"}},"required":["building_id","state_id","amount"]}],
            "additionalProperties":false}})
}
struct Arguments<'a> {
    stage: stage::Stage,
    building: Option<&'a str>,
    state: Option<i32>,
    amount: Option<u32>,
    id: &'a str,
    token: &'a str,
}
fn parse(args: &serde_json::Map<String, Value>) -> Result<Arguments<'_>> {
    allowed(
        args,
        &[
            "stage",
            "building_id",
            "state_id",
            "amount",
            "request_id",
            "lease_token",
        ],
    )?;
    let state_id = || -> Result<i32> {
        let id = args
            .get("state_id")
            .and_then(Value::as_i64)
            .ok_or_else(|| Error::new("invalid_arguments", "Integer state_id required"))?;
        crate::construction::find_view::validate_state_id(id)
    };
    let stage_name = string(args, "stage")?;
    if stage_name == "place" {
        let key = string(args, "building_id")?;
        observation::validate_key(key)?;
        let amount = args
            .get("amount")
            .and_then(Value::as_u64)
            .filter(|n| (1..=1000).contains(n))
            .ok_or_else(|| Error::new("invalid_arguments", "Bounded integer amount required"))?;
        return Ok(Arguments {
            stage: stage::Stage::Place,
            building: Some(key),
            state: Some(state_id()?),
            amount: Some(amount as u32),
            id: string(args, "request_id")?,
            token: string(args, "lease_token")?,
        });
    }
    if args.contains_key("amount") {
        return Err(Error::new("invalid_arguments", "Only place takes amount"));
    }
    if stage_name.starts_with("find_") {
        if args.contains_key("building_id") {
            return Err(Error::new(
                "invalid_arguments",
                "Find stages take no building_id",
            ));
        }
        let (stage, state) = match stage_name {
            "find_open" if !args.contains_key("state_id") => (stage::Stage::FindOpen, None),
            "find_query" => (stage::Stage::FindQuery, Some(state_id()?)),
            "find_activate" => (stage::Stage::FindActivate, Some(state_id()?)),
            _ => {
                return Err(Error::new(
                    "invalid_arguments",
                    "Exact construction stage target required",
                ))
            }
        };
        return Ok(Arguments {
            stage,
            building: None,
            state,
            amount: None,
            id: string(args, "request_id")?,
            token: string(args, "lease_token")?,
        });
    }
    if args.contains_key("state_id") {
        return Err(Error::new(
            "invalid_arguments",
            "Only Find stages take state_id",
        ));
    }
    let (selected, building) = match stage_name {
        "ensure_open" if !args.contains_key("building_id") => (stage::Stage::EnsureOpen, None),
        "select_building" => {
            let key = string(args, "building_id")?;
            observation::validate_key(key)?;
            if key == "rail_way" {
                return Err(Error::new(
                    "invalid_arguments",
                    "Railway selection is unsupported",
                ));
            }
            (stage::Stage::SelectBuilding, Some(key))
        }
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                "Exact construction stage target required",
            ))
        }
    };
    Ok(Arguments {
        stage: selected,
        building,
        state: None,
        amount: None,
        id: string(args, "request_id")?,
        token: string(args, "lease_token")?,
    })
}
pub(super) fn call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let args = parse(args)?;
    let until = Instant::now() + Duration::from_secs(10);
    owner.daily.input_check()?;
    drop(input_permit(&owner.input_gate)?);
    let mut controller = owner.input_state()?;
    let target = stage::Target {
        epoch: controller.guard.epoch.clone(),
        stage: args.stage,
        building_id: args.building.map(str::to_owned),
        state_id: args.state,
        amount: args.amount,
    };
    target.validate()?;
    controller.authority.authorize(args.token, &target.epoch)?;
    let ControlState {
        controls,
        guard,
        authority,
        journal,
        research,
        focus,
        construction,
    } = &mut *controller;
    let service = construction
        .as_mut()
        .ok_or_else(|| Error::new("capability_unavailable", "Construction profile unavailable"))?;
    let domains = Domains {
        clock: journal,
        research: research.as_ref(),
        focus: focus.as_ref(),
    };
    let mut backend = ConstructionBackend {
        process,
        controls,
        guard,
        profile: &service.profile,
        display: service.display,
        xauthority: &service.xauthority,
        until,
        rejected_reads: stage::read_recovery::Report::default(),
    };
    let timeout = until.saturating_duration_since(Instant::now());
    if timeout.is_zero() {
        return Err(Error::new(
            "input_not_queued",
            "Construction deadline ended before execution",
        ));
    }
    let result = service.journal.execute(
        args.id,
        target,
        args.token,
        authority,
        &owner.input_gate,
        &domains,
        &mut backend,
        timeout,
    );
    let execution = result.map_err(|error| backend.rejected_reads.with_error(error, args.token))?;
    Ok(
        json!({"execution":execution,"map_placement_available":false,
        "discarded_preinput_reads":backend.rejected_reads.value(args.token),
        "effective_routing_verified":false,"current_frame_verified":false,"global_modal_clear_verified":false,
        "full_campaign_acceptance":false}),
    )
}
struct Domains<'a> {
    clock: &'a Journal,
    research: Option<&'a research::Service>,
    focus: Option<&'a focus::Service>,
}
impl stage::UnresolvedInputs for Domains<'_> {
    fn observe(&self, game: &Identity) -> Result<stage::DomainBlocks> {
        Ok(stage::DomainBlocks {
            clock: self.clock.blocks_game(game),
            research: self.research.is_some_and(|v| v.journal.blocks_game(game)),
            focus: self.focus.is_some_and(|v| v.journal.blocks_game(game)),
            construction: false, // This service has the sole construction journal, checked by execute.
        })
    }
}
struct ConstructionBackend<'a> {
    process: &'a dyn Process,
    controls: &'a mut X11Controls,
    guard: &'a mut Guard,
    profile: &'a OwnedClientProfile,
    display: u16,
    xauthority: &'a Path,
    until: Instant,
    rejected_reads: stage::read_recovery::Report,
}
fn observed(
    process: &dyn Process,
    guard: &mut Guard,
    target: &stage::Target,
) -> Result<(
    observation::Observation,
    stage::Observation<observation::Scope>,
)> {
    observed_with(process, guard, target, |producer| producer())
}
fn observed_with(
    process: &dyn Process,
    guard: &mut Guard,
    target: &stage::Target,
    read: impl FnOnce(
        &mut dyn FnMut() -> Result<observation::Observation>,
    ) -> Result<observation::Observation>,
) -> Result<(
    observation::Observation,
    stage::Observation<observation::Scope>,
)> {
    if target.epoch != guard.epoch {
        return Err(Error::new(
            "ownership_changed",
            "Construction target epoch differs",
        ));
    }
    // EnsureOpen does not depend on choice materialization or this public key.
    let o = read(&mut || {
        observation::observe(
            process,
            target.building_id.as_deref().unwrap_or("dockyard"),
            !target.stage.is_find(),
            target
                .stage
                .uses_find_view()
                .then_some(observation::FindRead {
                    state_id: target.state_id,
                    placement: target.stage == stage::Stage::Place,
                }),
        )
    })?;
    // Only the producer above is recoverable. Ownership acceptance, state and
    // scope decisions always run once against the complete fresh observation.
    guard.accept_context(process, &o.campaign)?;
    let state = state::model(&o.value, target)?;
    let scope = o.scope(target.stage)?;
    Ok((o, stage::Observation { state, scope }))
}
impl ConstructionBackend<'_> {
    fn before_input(
        &mut self,
        target: &stage::Target,
        phase: stage::read_recovery::Phase,
        deadline: Instant,
    ) -> Result<(
        observation::Observation,
        stage::Observation<observation::Scope>,
    )> {
        let deadline = deadline.min(self.until);
        observed_with(self.process, self.guard, target, |producer| {
            stage::read_recovery::observe(phase, deadline, producer, &mut self.rejected_reads)
        })
    }
}
fn check_scope(expected: &observation::Scope, actual: &observation::Scope) -> Result<()> {
    if expected != actual {
        return Err(Error::new(
            "ownership_changed",
            "Pinned construction identity changed",
        ));
    }
    Ok(())
}
impl stage::Backend for ConstructionBackend<'_> {
    type Scope = observation::Scope;
    fn observe(&mut self, target: &stage::Target) -> Result<stage::Observation<Self::Scope>> {
        if Instant::now() >= self.until {
            return Err(Error::new(
                "construction_deadline",
                "Original construction deadline ended",
            ));
        }
        let (_, state) = observed(self.process, self.guard, target)?;
        if Instant::now() >= self.until {
            return Err(Error::new(
                "construction_deadline",
                "Original construction observation deadline ended",
            ));
        }
        Ok(state)
    }
    fn observe_before_input(
        &mut self,
        target: &stage::Target,
        phase: stage::read_recovery::Phase,
        deadline: Instant,
    ) -> Result<stage::Observation<Self::Scope>> {
        self.before_input(target, phase, deadline)
            .map(|(_, state)| state)
    }
    fn candidate(
        &mut self,
        target: &stage::Target,
        scope: &Self::Scope,
        deadline: Instant,
    ) -> Result<stage::Candidate<Self::Scope>> {
        let deadline = deadline.min(self.until);
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_not_queued",
                "Construction candidate deadline ended",
            ));
        }
        let (o, fresh) =
            self.before_input(target, stage::read_recovery::Phase::Candidate, deadline)?;
        check_scope(scope, &fresh.scope)?;
        let input = match target.stage {
            stage::Stage::FindOpen => stage::Input::FindKey,
            stage::Stage::FindQuery => {
                stage::Input::Digits(stage::state_digits(target.state_id.unwrap_or(0))?)
            }
            stage::Stage::FindActivate => stage::Input::Return,
            _ => stage::Input::Point(state::point(&o.value, target, self.profile)?),
        };
        if target.stage.is_find() {
            fresh.state.preflight(target)?;
        }
        Ok(stage::Candidate {
            input,
            scope: fresh.scope,
        })
    }
    fn dispatch_once(
        &mut self,
        target: &stage::Target,
        original: &stage::Candidate<Self::Scope>,
        deadline: Instant,
    ) -> stage::DispatchOutcome {
        let deadline = deadline.min(self.until);
        let process = self.process;
        let guard = &mut *self.guard;
        let profile = self.profile;
        let rejected_reads = &mut self.rejected_reads;
        let original_point = match &original.input {
            stage::Input::Point(point) if !target.stage.is_find() => point.clone(),
            stage::Input::Point(_) => {
                return stage::DispatchOutcome::NotQueued(Error::new(
                    "construction_precondition",
                    "Find stages never dispatch pointer input",
                ))
            }
            keyboard => {
                // Same final typed recheck as the pointer path, immediately before queueing.
                let mut may_queue = false;
                let mut fresh_check = || -> Result<()> {
                    let (_, fresh) = observed_with(process, guard, target, |producer| {
                        stage::read_recovery::observe(
                            stage::read_recovery::Phase::DispatchCheck,
                            deadline,
                            producer,
                            rejected_reads,
                        )
                    })?;
                    check_scope(&original.scope, &fresh.scope)?;
                    if fresh.state.reached(target) {
                        return Err(Error::new(
                            "construction_precondition",
                            "Target already reached before dispatch",
                        ));
                    }
                    fresh.state.preflight(target)?;
                    if Instant::now() >= deadline {
                        return Err(Error::new(
                            "input_not_queued",
                            "Construction original deadline ended",
                        ));
                    }
                    Ok(())
                };
                let result = match (keyboard, target.stage) {
                    (stage::Input::FindKey, stage::Stage::FindOpen)
                    | (stage::Input::Return, stage::Stage::FindActivate) => {
                        let key = if target.stage == stage::Stage::FindOpen {
                            crate::platform::x11::ControlKey::FindView
                        } else {
                            crate::platform::x11::ControlKey::Return
                        };
                        self.controls
                            .send_until_checked(process, key, deadline, || {
                                fresh_check()?;
                                may_queue = true;
                                Ok(true)
                            })
                            .and_then(|sent| {
                                if sent {
                                    Ok(())
                                } else {
                                    Err(Error::new("input_not_queued", "Key not queued"))
                                }
                            })
                    }
                    (stage::Input::Digits(digits), stage::Stage::FindQuery) => self
                        .controls
                        .send_digits_until_checked(process, digits, deadline, || {
                            fresh_check()?;
                            may_queue = true;
                            Ok(())
                        }),
                    _ => Err(Error::new(
                        "construction_precondition",
                        "Keyboard input does not match its Find stage",
                    )),
                };
                return match result {
                    Ok(()) => stage::DispatchOutcome::ServerConfirmed,
                    Err(error) if !may_queue => stage::DispatchOutcome::NotQueued(error),
                    Err(error) => stage::DispatchOutcome::Unknown(Some(error)),
                };
            }
        };
        let prepared = self.controls.prepare_left_click_until(
            process,
            PointerDisplay {
                display: self.display,
                authority: self.xauthority,
            },
            original_point.client_pixel,
            original_point.expected_client,
            deadline,
        );
        let mut prepared = match prepared {
            Ok(v) => v,
            Err(e) => return stage::DispatchOutcome::NotQueued(e),
        };
        let result = prepared.dispatch_once_checked(|| {
            let (o, fresh) = observed_with(process, guard, target, |producer| {
                stage::read_recovery::observe(
                    stage::read_recovery::Phase::DispatchCheck,
                    deadline,
                    producer,
                    rejected_reads,
                )
            })?;
            check_scope(&original.scope, &fresh.scope)?;
            if fresh.state.reached(target) {
                return Err(Error::new(
                    "construction_precondition",
                    "Target already reached before dispatch",
                ));
            }
            fresh.state.preflight(target)?;
            let point = state::point(&o.value, target, profile)?;
            if point != original_point {
                return Err(Error::new(
                    "construction_geometry_unavailable",
                    "Original construction candidate changed before dispatch",
                ));
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "input_not_queued",
                    "Construction original deadline ended",
                ));
            }
            Ok(())
        });
        match result.delivery {
            PointerDelivery::NotQueued => {
                stage::DispatchOutcome::NotQueued(result.error.unwrap_or_else(|| {
                    Error::new(
                        "input_not_queued",
                        "Ordinary pointer transport confirmed no queued input",
                    )
                }))
            }
            PointerDelivery::Unknown => stage::DispatchOutcome::Unknown(result.error),
            PointerDelivery::ServerConfirmed => stage::DispatchOutcome::ServerConfirmed,
        }
    }
    fn confirm_input_idle(
        &mut self,
        original: &stage::Candidate<Self::Scope>,
        deadline: Instant,
    ) -> Result<()> {
        let deadline = deadline.min(self.until);
        match &original.input {
            stage::Input::Point(point) => {
                let _probe = self.controls.prepare_left_click_until(
                    self.process,
                    PointerDisplay {
                        display: self.display,
                        authority: self.xauthority,
                    },
                    point.client_pixel,
                    point.expected_client,
                    deadline,
                )?;
                Ok(())
            }
            stage::Input::FindKey => self
                .controls
                .preflight(self.process, &crate::platform::x11::ControlKey::FindView),
            stage::Input::Return => self
                .controls
                .preflight(self.process, &crate::platform::x11::ControlKey::Return),
            stage::Input::Digits(digits) => self.controls.preflight_digits(self.process, digits),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn action_shapes_never_accept_coordinates_raw_keys_or_railway() {
        let open = json!({"stage":"ensure_open","request_id":"open-1","lease_token":"fixture"});
        assert_eq!(
            parse(open.as_object().unwrap()).unwrap().stage,
            stage::Stage::EnsureOpen
        );
        let select = json!({"stage":"select_building","building_id":"dockyard","request_id":"select-1","lease_token":"fixture"});
        assert_eq!(
            parse(select.as_object().unwrap()).unwrap().building,
            Some("dockyard")
        );
        for mut value in [open.clone(), select.clone()] {
            for (key, data) in [
                ("x", json!(10)),
                ("y", json!(10)),
                ("control_id", json!(123)),
                ("key", json!("b")),
                ("timeout", json!(60)),
                ("debug", json!(true)),
            ] {
                value[key] = data;
                assert!(parse(value.as_object().unwrap()).is_err());
                value.as_object_mut().unwrap().remove(key);
            }
        }
        for key in ["rail_way", "", "dockyard;exec", "Dock yard", "../dockyard"] {
            let mut value = select.clone();
            value["building_id"] = json!(key);
            assert!(parse(value.as_object().unwrap()).is_err(), "{key}");
        }
        let mut value = open;
        value["building_id"] = json!("dockyard");
        assert!(parse(value.as_object().unwrap()).is_err());
        let mut value = select;
        value.as_object_mut().unwrap().remove("building_id");
        assert!(parse(value.as_object().unwrap()).is_err());
    }
    #[test]
    fn find_stages_take_only_bounded_state_ids_and_no_keys_or_coordinates() {
        let open = json!({"stage":"find_open","request_id":"f-1","lease_token":"t"});
        assert_eq!(
            parse(open.as_object().unwrap()).unwrap().stage,
            stage::Stage::FindOpen
        );
        for name in ["find_query", "find_activate"] {
            let value = json!({"stage":name,"state_id":64,"request_id":"f-2","lease_token":"t"});
            assert_eq!(parse(value.as_object().unwrap()).unwrap().state, Some(64));
            for bad in [json!(0), json!(4096), json!("64"), json!(64.5), json!(-1)] {
                let mut changed = value.clone();
                changed["state_id"] = bad;
                assert!(parse(changed.as_object().unwrap()).is_err());
            }
            let mut changed = value.clone();
            changed.as_object_mut().unwrap().remove("state_id");
            assert!(parse(changed.as_object().unwrap()).is_err());
            for (key, data) in [
                ("building_id", json!("dockyard")),
                ("key", json!("f")),
                ("x", json!(1)),
                ("text", json!("64")),
            ] {
                let mut changed = value.clone();
                changed[key] = data;
                assert!(parse(changed.as_object().unwrap()).is_err(), "{key}");
            }
        }
        let mut open_with_state = open;
        open_with_state["state_id"] = json!(64);
        assert!(parse(open_with_state.as_object().unwrap()).is_err());
        let select = json!({"stage":"select_building","building_id":"dockyard","state_id":64,"request_id":"s","lease_token":"t"});
        assert!(parse(select.as_object().unwrap()).is_err());
    }
    #[test]
    fn place_requires_building_state_and_bounded_amount_only() {
        let value = json!({"stage":"place","building_id":"arms_factory","state_id":64,"amount":6,"request_id":"p","lease_token":"t"});
        let args = parse(value.as_object().unwrap()).unwrap();
        assert_eq!(
            (args.stage, args.building, args.state, args.amount),
            (stage::Stage::Place, Some("arms_factory"), Some(64), Some(6))
        );
        let coastal = json!({"stage":"place","building_id":"dockyard","state_id":63,"amount":1,"request_id":"d","lease_token":"t"});
        let args = parse(coastal.as_object().unwrap()).unwrap();
        assert_eq!(
            (args.stage, args.building, args.state, args.amount),
            (stage::Stage::Place, Some("dockyard"), Some(63), Some(1))
        );
        for (key, bad) in [
            ("amount", json!(0)),
            ("amount", json!(1001)),
            ("amount", json!("6")),
            ("state_id", json!(0)),
            ("building_id", json!("rail way")),
        ] {
            let mut changed = value.clone();
            changed[key] = bad;
            assert!(parse(changed.as_object().unwrap()).is_err(), "{key}");
        }
        for key in ["amount", "state_id", "building_id"] {
            let mut changed = value.clone();
            changed.as_object_mut().unwrap().remove(key);
            assert!(parse(changed.as_object().unwrap()).is_err(), "{key}");
        }
        for (key, data) in [("x", json!(256)), ("y", json!(144)), ("key", json!("f"))] {
            let mut changed = value.clone();
            changed[key] = data;
            assert!(parse(changed.as_object().unwrap()).is_err(), "{key}");
        }
        let select = json!({"stage":"select_building","building_id":"dockyard","amount":1,"request_id":"s","lease_token":"t"});
        assert!(parse(select.as_object().unwrap()).is_err());
    }
    #[test]
    fn construction_owner_permission_is_independent_and_default_off() {
        let mut value = json!({"game":{"pid":123,"start_ticks":"456","exe":"/fixture/game","uid":1000},"controller":"fixture","player":"GER","window":42,"window_class":[],"menu_evidence_ids":["setup"],"focus_profile":"linux_builtin_absolute_512x288_v1"});
        let b: Binding = serde_json::from_value(value.clone()).unwrap();
        assert!(b.construction_profile.is_none());
        value["construction_profile"] = json!("linux_builtin_absolute_512x288_v1");
        let b: Binding = serde_json::from_value(value.clone()).unwrap();
        assert!(b.construction_profile.is_some());
        assert!(!b.daily_focus && !b.daily_prepared_research);
        for invalid in [
            json!(true),
            json!("automatic"),
            json!("windows_builtin_absolute_512x288_v1"),
        ] {
            value["construction_profile"] = invalid;
            assert!(serde_json::from_value::<Binding>(value.clone()).is_err());
        }
    }
}
