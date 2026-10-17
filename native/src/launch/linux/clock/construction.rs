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
        "description":"Preview: ensure the own construction panel is open, or select one canonical ordinary building type. Already-matching state is a no-op. At most one ordinary click per durable request, verified by own canonical readback. Requires the owned profile and exclusive lease. Unknown outcomes block all input; never replay uncertain requests. No map placement, coordinates or engine calls. Live action acceptance pending.",
        "inputSchema":{"type":"object","properties":{
            "stage":{"enum":["ensure_open","select_building"]},
            "building_id":{"type":"string","minLength":1,"maxLength":128},
            "request_id":{"type":"string","minLength":1,"maxLength":128},
            "lease_token":{"type":"string"}},
            "required":["stage","request_id","lease_token"],
            "oneOf":[{"properties":{"stage":{"const":"ensure_open"}},"not":{"required":["building_id"]}},
                {"properties":{"stage":{"const":"select_building"}},"required":["building_id"]}],
            "additionalProperties":false}})
}
struct Arguments<'a> {
    stage: stage::Stage,
    building: Option<&'a str>,
    id: &'a str,
    token: &'a str,
}
fn parse(args: &serde_json::Map<String, Value>) -> Result<Arguments<'_>> {
    allowed(args, &["stage", "building_id", "request_id", "lease_token"])?;
    let (selected, building) = match string(args, "stage")? {
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
            true,
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
        let point = state::point(&o.value, target, self.profile)?;
        Ok(stage::Candidate {
            point,
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
        let prepared = self.controls.prepare_left_click_until(
            process,
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
            if point != original.point {
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
        let _probe = self.controls.prepare_left_click_until(
            self.process,
            PointerDisplay {
                display: self.display,
                authority: self.xauthority,
            },
            original.point.client_pixel,
            original.point.expected_client,
            deadline,
        )?;
        Ok(())
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
