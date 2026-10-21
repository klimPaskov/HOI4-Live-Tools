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
    json!({"name":"hoi4_production_attempt","description":"Linux preview: request exactly one factory increase or decrease for an exact own current line_handle and equipment_id in an already-open production panel. A matching count is a no-op. One ordinary unmodified click maximum, original count/identity persisted before input, canonical own-line readback. Requires owned profile and exclusive lease. An unresolved retry only checks the original target and never replays input; uncertainty blocks all domains. Minimum target one. No new line, panel opening, scrolling, raw coordinates or engine posting. Live validation pending.",
        "inputSchema":{"type":"object","properties":{
            "line_handle":{"type":"string","pattern":"^0x[0-9a-f]{16}$"},
            "equipment_id":{"type":"string","minLength":1,"maxLength":128},
            "factories":{"type":"integer","minimum":1,"maximum":1000000},
            "request_id":{"type":"string","minLength":1,"maxLength":128},"lease_token":{"type":"string"}},
            "required":["line_handle","equipment_id","factories","request_id","lease_token"],"additionalProperties":false}})
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
    let target = step::Target {
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
    };
    target.validate()?;
    Ok(target)
}
pub(super) fn call(
    owner: &ClockService,
    process: &dyn Process,
    args: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(10);
    owner.daily.input_check()?;
    drop(input_permit(&owner.input_gate)?);
    let mut state = owner.input_state()?;
    let target = parse(args, state.guard.epoch.clone())?;
    let token = string(args, "lease_token")?;
    let id = string(args, "request_id")?;
    state.authority.authorize(token, &target.epoch)?;
    let ControlState {
        controls,
        guard,
        authority,
        journal,
        research,
        focus,
        construction,
        production,
    } = &mut *state;
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
    let execution = service.journal.execute(
        id,
        target,
        token,
        authority,
        &owner.input_gate,
        &domains,
        &mut backend,
        timeout,
    )?;
    Ok(json!({"execution":execution,"live_validation":"pending","full_campaign_acceptance":false}))
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
fn observed(
    process: &dyn Process,
    guard: &mut Guard,
    target: &step::Target,
    ui: bool,
    deadline: Instant,
) -> Result<(observation::Observation, step::State)> {
    if target.epoch != guard.epoch {
        return Err(Error::new(
            "ownership_changed",
            "Production target epoch differs",
        ));
    }
    let mut report = read_recovery::Report::default();
    let o = read_recovery::observe(
        read_recovery::Phase::Candidate,
        deadline,
        || {
            observation::observe(
                process,
                &target.line_handle,
                &target.equipment_id,
                ui.then_some(true),
            )
        },
        &mut report,
    )?;
    guard.accept_context(process, &o.campaign)?;
    let state = step::State {
        epoch: guard.epoch.clone(),
        line: o.line.clone(),
        requested: o.requested,
    };
    state.validate(target)?;
    if Instant::now() >= deadline {
        return Err(Error::new(
            "production_deadline",
            "Production observation deadline ended",
        ));
    }
    Ok((o, state))
}
fn candidate(
    process: &dyn Process,
    guard: &mut Guard,
    target: &step::Target,
    before: &step::State,
    profile: &OwnedClientProfile,
    deadline: Instant,
) -> Result<Candidate> {
    let (o, state) = observed(process, guard, target, true, deadline)?;
    if state != *before {
        return Err(Error::new(
            "production_state_changed",
            "Original line changed before candidate",
        ));
    }
    let direction = if target.factories > before.requested {
        "add"
    } else {
        "subtract"
    };
    let control = &o.value["controls"][direction];
    if o.value["build_id"] != game::BUILD_ID
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
}
