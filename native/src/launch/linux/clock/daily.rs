//! Owner-only optional daily prepared-Research worker. No navigation/effects.
use super::*;
use crate::{
    daily_governor as core,
    daily_observation::{Produced, Producer},
};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Weak,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    version: u8,
    revision: u64,
    entries: Vec<core::Entry>,
    max_actions_per_day: u8,
    max_observation_age_ms: u64,
    review_day: Option<i64>,
}
impl Spec {
    fn plan(&self, scope: core::Scope) -> core::Plan {
        core::Plan {
            version: self.version,
            revision: self.revision,
            scope,
            entries: self.entries.clone(),
            max_actions_per_day: self.max_actions_per_day,
            max_observation_age_ms: self.max_observation_age_ms,
            review_day: self.review_day,
        }
    }
}
struct Active {
    life: crate::daily_worker::Lifecycle,
    producer: Arc<Producer>,
    report: Arc<Mutex<Value>>,
}
pub(super) struct Controller {
    enabled: bool,
    owner: Weak<Shared>,
    directory: PathBuf,
    active: Mutex<Option<Active>>,
    blocked: Arc<AtomicBool>,
}
impl Controller {
    pub(super) fn new(enabled: bool, owner: Weak<Shared>, directory: &Path) -> Self {
        Self {
            enabled,
            owner,
            directory: directory.to_owned(),
            active: Mutex::new(None),
            blocked: Arc::new(AtomicBool::new(false)),
        }
    }
    pub(super) fn input_check(&self) -> Result<()> {
        if self.blocked.load(Ordering::Acquire) {
            Err(Error::new(
                "action_uncertain",
                "Daily unresolved input blocks all controller work; no token rescue",
            ))
        } else {
            Ok(())
        }
    }
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }
    pub(super) fn tool() -> Value {
        json!({"name":"hoi4_daily_governor","description":"Optional owned daily prepared-Research worker and fresh compact digest. Start requires the existing exclusive lease and explicit typed plan. Only one verified Research stage may run when its exact own selected Info is already prepared; navigation and other action domains remain unavailable. Unknown/uncertain input is never replayed. Stop does not pause the game.","inputSchema":{"type":"object","oneOf":[{"properties":{"op":{"const":"start"},"lease_token":{"type":"string"},"plan":{"type":"object"}},"required":["op","lease_token","plan"],"additionalProperties":false},{"properties":{"op":{"enum":["digest","stop"]}},"required":["op"],"additionalProperties":false}]}})
    }
    pub(super) fn call(
        &self,
        owner: &ClockService,
        process: &dyn Process,
        args: &serde_json::Map<String, Value>,
    ) -> Result<Value> {
        if !self.enabled {
            return Err(Error::new(
                "capability_unavailable",
                "Daily worker was not explicitly enabled by the owned binding",
            ));
        }
        let op = string(args, "op")?;
        match op {
            "start" => {
                allowed(args, &["op", "lease_token", "plan"])?;
                let token = string(args, "lease_token")?.to_owned();
                let spec: Spec =
                    serde_json::from_value(args.get("plan").cloned().ok_or_else(|| {
                        Error::new("invalid_arguments", "Explicit plan required")
                    })?)
                    .map_err(|e| Error::new("invalid_arguments", e.to_string()))?;
                self.input_check()?;
                drop(input_permit(&owner.input_gate)?);
                let mut active = try_state(&self.active)?;
                if active.as_ref().is_some_and(|a| a.life.running()) {
                    return Err(Error::new("input_busy", "Daily worker already running"));
                }
                if let Some(mut old) = active.take() {
                    old.life.stop()?;
                }
                let until = Instant::now() + Duration::from_secs(5);
                let mut state = owner.input_state()?;
                let epoch = state.guard.epoch.clone();
                let lease_end = state.authority.authorize(&token, &epoch)?;
                let until = until.min(lease_end);
                if unresolved(&state) {
                    self.blocked.store(true, Ordering::Release);
                    return Err(Error::new("action_uncertain", "Shared input is unresolved"));
                }
                let expected = game::clock_context(process)?;
                state.guard.accept_context(process, &expected)?;
                let producer = Arc::new(Producer::open(
                    process,
                    epoch,
                    expected,
                    &self.directory.join("daily-sequence.json"),
                    until,
                )?);
                let mut governor = core::Governor::open(
                    &self.directory.join("daily-plan.json"),
                    spec.plan(producer.scope()?),
                )?;
                if governor.blocks_work() {
                    self.blocked.store(true, Ordering::Release);
                    return Err(Error::new(
                        "governor_quarantined",
                        "Persisted daily input uncertainty; no restart rescue",
                    ));
                }
                let first = producer.observe(process, until)?;
                if first.observation.sequence <= governor.last_sequence() {
                    return Err(Error::new(
                        "sequence_changed",
                        "Persisted producer sequence does not exceed governor history",
                    ));
                }
                drop(state);
                let stop = Arc::new(AtomicBool::new(false));
                let wake = Arc::new(core::Wakeup::default());
                let report = Arc::new(Mutex::new(
                    json!({"running":true,"last_batch":null,"last_error":null}),
                ));
                let control = owner.state.clone();
                let gate = owner.input_gate.clone();
                let weak = self.owner.clone();
                let blocked = self.blocked.clone();
                let p = producer.clone();
                let s = stop.clone();
                let w = wake.clone();
                let r = report.clone();
                let handle=thread::Builder::new().name("owned-daily-governor".into()).spawn(move||{
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||worker(&mut governor,&p,&control,&gate,&weak,&token,&s,&w,&r,&blocked)));
 if outcome.is_err(){blocked.store(true,Ordering::Release);if let Ok(mut report)=r.lock(){report["last_error"]=json!({"code":"daily_quarantined","reason":"Worker panic; durable intent requires review"});}}
 if governor.blocks_work(){blocked.store(true,Ordering::Release);}s.store(true,Ordering::Release);if let Ok(mut report)=r.lock(){report["running"]=json!(false);}
 })?;
                *active = Some(Active {
                    life: crate::daily_worker::Lifecycle::new(stop, wake, handle),
                    producer,
                    report,
                });
                Ok(
                    json!({"started":true,"scope":"prepared own Research only","navigation_implemented":false,"other_action_domains_implemented":false,"full_campaign_acceptance":false}),
                )
            }
            "stop" => {
                allowed(args, &["op"])?;
                let mut active = try_state(&self.active)?;
                if let Some(a) = active.as_mut() {
                    a.life.stop()?;
                }
                Ok(
                    json!({"worker_stopped":true,"game_time_changed":false,"uncertainty_retained":self.blocked.load(Ordering::Acquire)}),
                )
            }
            "digest" => {
                allowed(args, &["op"])?;
                let active = try_state(&self.active)?;
                let a = active
                    .as_ref()
                    .ok_or_else(|| Error::new("capability_unavailable", "No daily plan started"))?;
                let fresh = a
                    .producer
                    .observe(process, Instant::now() + Duration::from_secs(2))?;
                let report = a
                    .report
                    .try_lock()
                    .map_err(|_| Error::new("input_busy", "Daily digest update in progress"))?
                    .clone();
                compact(fresh, &report, self.blocked.load(Ordering::Acquire))
            }
            _ => Err(Error::new("invalid_arguments", "Unknown daily operation")),
        }
    }
}
fn unresolved(state: &ControlState) -> bool {
    state.journal.blocks_game(&state.guard.epoch.game)
        || state
            .focus
            .as_ref()
            .is_some_and(|f| f.journal.blocks_game(&state.guard.epoch.game))
        || state
            .research
            .as_ref()
            .is_some_and(|r| r.journal.blocks_game(&state.guard.epoch.game))
}
#[allow(clippy::too_many_arguments)]
fn worker(
    governor: &mut core::Governor,
    producer: &Producer,
    state: &Mutex<ControlState>,
    gate: &RwLock<bool>,
    owner: &Weak<Shared>,
    token: &str,
    stop: &AtomicBool,
    wake: &core::Wakeup,
    report: &Mutex<Value>,
    blocked: &AtomicBool,
) {
    let mut generation = 0;
    while !stop.load(Ordering::Acquire) {
        let Some(shared) = owner.upgrade() else { break };
        if !shared.alive.load(Ordering::Acquire)
            || !shared.accepting_reads.load(Ordering::Acquire)
            || gate.try_read().map_or(true, |g| !*g)
        {
            break;
        }
        let run = (|| -> Result<Option<core::DailyResult>> {
            let _read = shared
                .reads
                .try_read()
                .map_err(|_| Error::new("session_closed", "Read retirement in progress"))?;
            if !shared.accepting_reads.load(Ordering::Acquire) {
                return Err(Error::new("session_closed", "Owned read gate retired"));
            }
            let process = shared
                .server
                .process
                .as_ref()
                .ok_or_else(|| Error::new("session_closed", "Owned reader unavailable"))?
                .as_ref();
            let raw = producer.probe_clock(process, Instant::now() + Duration::from_secs(1))?;
            if governor.last_day() == Some((i64::from(raw) - 43_800_000).div_euclid(24)) {
                return Ok(None);
            }
            let mut state = try_state(state)?;
            state.authority.authorize(token, &state.guard.epoch)?;
            let until = Instant::now() + Duration::from_secs(5);
            let mut backend = DailyBackend {
                process,
                state: &mut state,
                producer,
                gate,
                token,
                until,
                last: None,
            };
            let result =
                governor.run_day(&mut backend, crate::daily_observation::unix_ms()?, until);
            // Publish quarantine while still holding the common controller.
            // A verified Research receipt followed by a failed daily witness
            // must not leave a window for manual clock/focus/research input.
            if governor.blocks_work() {
                blocked.store(true, Ordering::Release);
            }
            result.map(Some)
        })();
        if let Ok(mut report) = report.lock() {
            match run {
                Ok(Some(result)) => {
                    report["last_batch"] = json!(result);
                    report["last_error"] = Value::Null;
                }
                Ok(None) => {}
                Err(e) => {
                    report["last_error"] = json!({"code":e.code,"reason":"Daily work retained or quarantined; no automatic replay"});
                }
            }
        }
        if governor.blocks_work() {
            blocked.store(true, Ordering::Release);
            break;
        }
        drop(shared);
        match wake.wait_after(generation, Duration::from_millis(250)) {
            Ok(n) => generation = n.generation,
            Err(_) => {
                blocked.store(true, Ordering::Release);
                break;
            }
        }
    }
}
struct DailyBackend<'a> {
    process: &'a dyn Process,
    state: &'a mut ControlState,
    producer: &'a Producer,
    gate: &'a RwLock<bool>,
    token: &'a str,
    until: Instant,
    last: Option<Produced>,
}
impl core::Backend for DailyBackend<'_> {
    fn unresolved_input(&mut self) -> Result<bool> {
        Ok(unresolved(self.state))
    }
    fn observe(&mut self) -> Result<core::Observation> {
        let p = self.producer.observe(self.process, self.until)?;
        self.state
            .guard
            .accept_context(self.process, &p.context.campaign)?;
        let observation = p.observation.clone();
        self.last = Some(p);
        Ok(observation)
    }
    fn readiness(&mut self, entry: &core::Entry, _: &core::Observation) -> Result<core::Readiness> {
        let core::Intent::Research {
            technology_id,
            slot,
        } = &entry.intent
        else {
            return Ok(core::Readiness::Unavailable(format!(
                "{} has no verified daily adapter",
                entry.intent.capability()
            )));
        };
        if self.state.research.is_none() {
            return Ok(core::Readiness::Unavailable(
                "Owned Research profile was not enabled".into(),
            ));
        }
        let (state, _) = research::observed_state(self.process, &mut self.state.guard)?;
        if state
            .researching
            .get(*slot as usize)
            .and_then(Option::as_deref)
            == Some(technology_id.as_str())
        {
            return Ok(core::Readiness::Ready);
        }
        if state.researching.get(*slot as usize) != Some(&None) {
            return Ok(core::Readiness::Deferred(
                "Requested own slot is occupied or unavailable".into(),
            ));
        }
        if state.selected_slot != Some(*slot)
            || !state.tree_visible
            || state.visible_info.as_slice() != [Some(technology_id.clone())]
        {
            return Ok(core::Readiness::Deferred("Exact own slot and matching Info must already be prepared; automatic navigation unavailable".into()));
        }
        Ok(core::Readiness::Ready)
    }
    fn attempt(
        &mut self,
        attempt: &core::Attempt,
        _: &core::Observation,
        until: Instant,
    ) -> Result<core::Outcome> {
        let core::Intent::Research {
            technology_id,
            slot,
        } = &attempt.entry.intent
        else {
            return Ok(core::Outcome::NotQueued(
                "No implemented ordinary adapter".into(),
            ));
        };
        let target = crate::research_stage::Target {
            epoch: attempt.scope.epoch.clone(),
            stage: crate::research_stage::Stage::Research,
            technology: technology_id.clone(),
            folder_id: None,
            slot: *slot,
        };
        let execution = match research::execute_internal(
            self.state,
            self.process,
            self.gate,
            self.token,
            &attempt.request_id,
            target,
            until.min(self.until),
        ) {
            Ok((e, _)) => e,
            Err(e) if e.code != "action_uncertain" && !unresolved(self.state) => {
                return Ok(core::Outcome::NotQueued(e.code.into()))
            }
            Err(e) => return Err(e),
        };
        let fresh = self.producer.observe(self.process, until.min(self.until))?;
        self.state
            .guard
            .accept_context(self.process, &fresh.context.campaign)?;
        let r =
            core::VerifiedReceipt::from_research(attempt.clone(), &execution, fresh.observation)?;
        Ok(core::Outcome::Verified(Box::new(r)))
    }
}
fn compact(p: Produced, report: &Value, blocked: bool) -> Result<Value> {
    let mut digest = report["last_batch"]["digest"].clone();
    if !digest.is_object() {
        digest = json!({"verified":[],"retained":[]});
    }
    let olddate = digest["date_raw"].clone();
    let verified = digest["verified"].clone();
    digest["verified"] = json!([]);
    digest["latest_verified_ids_historical"] = verified;
    digest["last_batch_date_raw"] = olddate;
    digest["date_raw"] = json!(p.observation.date_raw);
    digest["sequence"] = json!(p.observation.sequence);
    digest["source"] = json!(p.observation.source);
    digest["observed_unix_ms"] = json!(p.observation.observed_unix_ms);
    digest["age_ms"] =
        json!(crate::daily_observation::unix_ms()?.saturating_sub(p.observation.observed_unix_ms));
    digest["counts"] = json!(p.observation.counts);
    digest["running"] = report["running"].clone();
    digest["last_error"] = report["last_error"].clone();
    digest["quarantined"] = json!(blocked);
    digest["input_authorized"] = json!(false);
    while serde_json::to_vec(&digest)
        .map_err(|e| Error::new("digest_error", e.to_string()))?
        .len()
        > 2048
    {
        let rows = digest["retained"].as_array_mut().filter(|v| !v.is_empty());
        if let Some(rows) = rows {
            rows.pop();
        } else {
            let rows = digest["latest_verified_ids_historical"]
                .as_array_mut()
                .filter(|v| !v.is_empty());
            if let Some(rows) = rows {
                rows.pop();
            } else {
                return Err(Error::new("digest_error", "Essential digest exceeds bound"));
            }
        }
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn player_json_cannot_supply_trusted_scope_or_process_pointers() {
        let value = json!({"version":1,"revision":1,"entries":[],"max_actions_per_day":1,"max_observation_age_ms":1000,"review_day":null,"scope":{"own_country":123}});
        assert!(serde_json::from_value::<Spec>(value).is_err());
    }
    #[test]
    fn stdio_schema_has_explicit_start_and_read_only_digest_stop_shapes() {
        let tool = Controller::tool();
        assert_eq!(tool["name"], "hoi4_daily_governor");
        assert_eq!(
            tool["inputSchema"]["oneOf"][0]["required"],
            json!(["op", "lease_token", "plan"])
        );
        assert_eq!(
            tool["inputSchema"]["oneOf"][1]["properties"]["op"]["enum"],
            json!(["digest", "stop"])
        );
    }
}
