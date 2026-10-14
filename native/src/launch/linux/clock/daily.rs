//! Owner-only optional research refill worker with verified navigation. No effects.
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
    governor: Arc<Mutex<core::Governor>>,
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
        json!({"name":"hoi4_daily_governor","description":"Optional owned daily automatic research refill worker and fresh compact digest. Start requires the existing exclusive lease and explicit typed plan. Navigation uses separately journaled verified stages; only Research completes an entry. Other action domains remain unavailable. Unknown/uncertain input is never replayed. Stop does not pause the game.","inputSchema":{"type":"object","oneOf":[{"properties":{"op":{"enum":["start","update"]},"lease_token":{"type":"string"},"plan":{"type":"object"}},"required":["op","lease_token","plan"],"additionalProperties":false},{"properties":{"op":{"enum":["digest","stop"]}},"required":["op"],"additionalProperties":false}]}})
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
                let mut state = try_state(&owner.state)?;
                self.input_check()?;
                let epoch = state.guard.epoch.clone();
                let lease_end = state.authority.authorize(&token, &epoch)?;
                let until = until.min(lease_end);
                if unresolved(&state) {
                    self.blocked.store(true, Ordering::Release);
                    return Err(Error::new("action_uncertain", "Shared input is unresolved"));
                }
                let expected = game::clock_context(process)?;
                state.guard.accept_context(process, &expected)?;
                prevalidate_navigation(process, &mut state.guard, &spec.entries, until)?;
                let producer = Arc::new(Producer::open(
                    process,
                    epoch,
                    expected,
                    &self.directory.join("daily-sequence.json"),
                    until,
                )?);
                let governor = core::Governor::open(
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
                let navigation = crate::daily_navigation::Navigator::open(
                    &self.directory.join("daily-navigation.json"),
                    state.guard.epoch.clone(),
                    &governor.plan().entries,
                )?;
                if navigation.blocked() {
                    self.blocked.store(true, Ordering::Release);
                    return Err(Error::new(
                        "action_uncertain",
                        "Unresolved navigation cannot restart",
                    ));
                }
                let governor = Arc::new(Mutex::new(governor));
                drop(state);
                let stop = Arc::new(AtomicBool::new(false));
                let wake = Arc::new(core::Wakeup::default());
                let report = Arc::new(Mutex::new(
                    json!({"running":true,"last_batch":null,"last_error":navigation.latest_refusal(),"last_preparation_refusal":navigation.latest_refusal()}),
                ));
                let control = owner.state.clone();
                let gate = owner.input_gate.clone();
                let weak = self.owner.clone();
                let blocked = self.blocked.clone();
                let p = producer.clone();
                let s = stop.clone();
                let w = wake.clone();
                let r = report.clone();
                let g = governor.clone();
                let handle=thread::Builder::new().name("owned-daily-governor".into()).spawn(move||{
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||worker(&g,navigation,&p,&control,&gate,&weak,&token,&s,&w,&r,&blocked)));
 if outcome.is_err(){blocked.store(true,Ordering::Release);if let Ok(mut report)=r.lock(){report["last_error"]=json!({"code":"daily_quarantined","reason":"Worker panic; durable intent requires review"});}}
 if crate::daily_worker::retained_quarantine(&g,|gov|gov.blocks_work()){blocked.store(true,Ordering::Release);}s.store(true,Ordering::Release);if let Ok(mut report)=r.lock(){report["running"]=json!(false);}
 })?;
                *active = Some(Active {
                    life: crate::daily_worker::Lifecycle::new(stop, wake, handle),
                    producer,
                    report,
                    governor,
                });
                Ok(
                    json!({"started":true,"scope":"own research refills within durable daily assignment budget","navigation_implemented":true,"other_action_domains_implemented":false,"full_campaign_acceptance":false}),
                )
            }
            "update" => {
                allowed(args, &["op", "lease_token", "plan"])?;
                self.input_check()?;
                let token = string(args, "lease_token")?;
                let spec: Spec =
                    serde_json::from_value(args.get("plan").cloned().ok_or_else(|| {
                        Error::new("invalid_arguments", "Explicit plan required")
                    })?)
                    .map_err(|e| Error::new("invalid_arguments", e.to_string()))?;
                let active = try_state(&self.active)?;
                let active = active
                    .as_ref()
                    .ok_or_else(|| Error::new("capability_unavailable", "No started plan"))?;
                let mut state = try_state(&owner.state)?;
                self.input_check()?;
                state.authority.authorize(token, &state.guard.epoch)?;
                if unresolved(&state) {
                    return Err(Error::new("action_uncertain", "Shared input unresolved"));
                }
                let mut governor = try_state(&active.governor)?;
                let until = Instant::now() + Duration::from_secs(2);
                prevalidate_navigation(process, &mut state.guard, &spec.entries, until)?;
                let fresh = active.producer.observe(process, until)?;
                state
                    .guard
                    .accept_context(process, &fresh.context.campaign)?;
                let updated = governor.update_plan(spec.plan(fresh.observation.scope))?;
                if Instant::now() >= until {
                    return Err(Error::new(
                        "daily_deadline",
                        "Update committed but reply exceeded deadline; exact retry is idempotent",
                    ));
                }
                Ok(
                    json!({"updated":updated,"plan_revision":governor.plan().revision,"day_consumption_reset":false}),
                )
            }
            "stop" => {
                allowed(args, &["op"])?;
                let mut active = try_state(&self.active)?;
                if let Some(a) = active.as_mut() {
                    a.life.stop()?;
                }
                let (last_error, last_preparation_refusal) = active
                    .as_ref()
                    .and_then(|a| {
                        a.report.try_lock().ok().map(|r| {
                            (
                                r["last_error"].clone(),
                                r["last_preparation_refusal"].clone(),
                            )
                        })
                    })
                    .unwrap_or((Value::Null, Value::Null));
                Ok(
                    json!({"worker_stopped":true,"game_time_changed":false,"uncertainty_retained":self.blocked.load(Ordering::Acquire),"last_error":last_error,"last_preparation_refusal":last_preparation_refusal}),
                )
            }
            "digest" => {
                allowed(args, &["op"])?;
                let active = try_state(&self.active)?;
                let a = active
                    .as_ref()
                    .ok_or_else(|| Error::new("capability_unavailable", "No daily plan started"))?;
                let until = Instant::now() + Duration::from_secs(2);
                crate::daily_digest_lock::governed(&a.governor, |governor| {
                    let fresh = a.producer.observe_digest(process, until)?;
                    let plan = governor.plan();
                    let revision = plan.revision;
                    let day = (i64::from(fresh.observation.date_raw) - 43_800_000).div_euclid(24);
                    let used = governor.used_actions(day);
                    let available = governor.budget_available(day);
                    let mut report = a
                        .report
                        .try_lock()
                        .map_err(|_| Error::new("input_busy", "Daily digest update in progress"))?
                        .clone();
                    report["daily_budget"] = json!({"day":day,"used_assignment_attempts":used,"limit":plan.max_actions_per_day,"remaining":if available {usize::from(plan.max_actions_per_day).saturating_sub(used)} else {0},"navigation_spends_assignment_budget":false});
                    compact(
                        fresh,
                        &report,
                        self.blocked.load(Ordering::Acquire),
                        revision,
                    )
                })
            }
            _ => Err(Error::new("invalid_arguments", "Unknown daily operation")),
        }
    }
}
fn prevalidate_navigation(
    process: &dyn Process,
    guard: &mut Guard,
    entries: &[core::Entry],
    until: Instant,
) -> Result<()> {
    if entries.len() > 256 {
        return Err(Error::new("invalid_plan", "Bounded plan entries required"));
    }
    if entries.iter().all(|e| e.navigation.is_none()) {
        return Ok(());
    }
    let (catalog, context) = crate::research_folders::owned_snapshot(process)?;
    if Instant::now() >= until {
        return Err(Error::new(
            "daily_deadline",
            "Folder validation exceeded original deadline; no plan change",
        ));
    }
    guard.accept_context(process, &context)?;
    if Instant::now() >= until {
        return Err(Error::new(
            "daily_deadline",
            "Folder context exceeded original deadline; no plan change",
        ));
    }
    validate_navigation_keys(entries, &catalog)
}
fn validate_navigation_keys(
    entries: &[core::Entry],
    catalog: &crate::research_folders::FolderCatalog,
) -> Result<()> {
    if entries.len() > 256 {
        return Err(Error::new("invalid_plan", "Bounded plan entries required"));
    }
    for entry in entries {
        let Some(hint) = &entry.navigation else {
            continue;
        };
        let bounded = |key: &str| {
            !key.is_empty()
                && key.len() <= 128
                && key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        };
        if !bounded(&entry.id) || !bounded(&hint.folder_id) {
            return Err(Error::new(
                "invalid_plan",
                "Bounded entry and folder IDs required",
            ));
        }
        if !catalog.contains(&hint.folder_id) {
            return Err(Error::new("invalid_plan", format!("Unknown public research folder '{}' for entry '{}'; inspect hoi4_research_folders", hint.folder_id, entry.id)));
        }
    }
    Ok(())
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
    governor: &Mutex<core::Governor>,
    mut navigation: crate::daily_navigation::Navigator,
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
        let mut refusal_entry = None;
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
            // Check even idle/exhausted workers before any clock probe. This is
            // released promptly; the existing pre-action check remains below.
            {
                let state = try_state(state)?;
                if blocked.load(Ordering::Acquire) || unresolved(&state) || navigation.blocked() {
                    blocked.store(true, Ordering::Release);
                    return Err(Error::new(
                        "action_uncertain",
                        "Shared input unresolved before idle probe",
                    ));
                }
                state.authority.authorize(token, &state.guard.epoch)?;
            }
            let raw = producer.probe_clock(process, Instant::now() + Duration::from_secs(1))?;
            let mut governor = try_state(governor)?;
            let day = (i64::from(raw) - 43_800_000).div_euclid(24);
            let entry = governor
                .pending_research(day)
                .into_iter()
                .find(|e| !navigation.refused_today(&e.id, day));
            if !governor.budget_available(day) || entry.is_none() {
                return Ok(None);
            }
            let mut state = try_state(state)?;
            state.authority.authorize(token, &state.guard.epoch)?;
            let until = Instant::now() + Duration::from_secs(5);
            if unresolved(&state) || navigation.blocked() {
                blocked.store(true, Ordering::Release);
                return Err(Error::new(
                    "action_uncertain",
                    "Shared or navigation input unresolved",
                ));
            }
            let day = (i64::from(raw) - 43_800_000).div_euclid(24);
            if let Err(e) = governor.preparation_check() {
                if governor.blocks_work() {
                    blocked.store(true, Ordering::Release);
                }
                return Err(e);
            }
            if let Some(entry) = &entry {
                let prepared = (|| -> Result<Option<crate::research_stage::Target>> {
                    let (mut ui, _) = research::observed_state(process, &mut state.guard)?;
                    if let Some(hint) = &entry.navigation {
                        if ui.tree_visible
                            && ui.visible_info.is_empty()
                            && ui.selected_slot
                                == match entry.intent {
                                    core::Intent::Research { slot, .. } => Some(slot),
                                    _ => None,
                                }
                        {
                            let core::Intent::Research { slot, .. } = &entry.intent else {
                                unreachable!()
                            };
                            let target = crate::research_stage::Target {
                                epoch: state.guard.epoch.clone(),
                                stage: crate::research_stage::Stage::SelectCategory,
                                technology: String::new(),
                                folder_id: Some(hint.folder_id.clone()),
                                slot: *slot,
                            };
                            ui = research::observed_target_state(
                                process,
                                &mut state.guard,
                                Some(&target),
                            )?
                            .0;
                        }
                    }
                    crate::daily_navigation::next(entry, &ui)
                })();
                let target = match prepared {
                    Ok(target) => target,
                    Err(e) => {
                        if crate::daily_navigation::known_preparation_refusal(e.code) {
                            refusal_entry = Some(entry.id.clone());
                            let saved = navigation.refuse_preparation(entry, day, &e);
                            if navigation.blocked() {
                                blocked.store(true, Ordering::Release);
                            }
                            saved?;
                            if let Ok(mut report) = report.lock() {
                                report["last_preparation_refusal"] =
                                    json!(navigation.latest_refusal());
                            }
                        }
                        return Err(e);
                    }
                };
                if let Some(target) = target {
                    let id = match navigation.begin(entry, target.clone(), day) {
                        Ok(id) => id,
                        Err(e) => {
                            if navigation.blocked() {
                                blocked.store(true, Ordering::Release);
                            }
                            return Err(e);
                        }
                    };
                    let preparation_stage = target.stage;
                    let result = research::execute_internal(
                        &mut state, process, gate, token, &id, target, until,
                    );
                    let saved = match result {
                        Ok((receipt, _)) => navigation.verified(&receipt),
                        Err(e) if e.code != "action_uncertain" && !unresolved(&state) => {
                            let saved = navigation.not_queued();
                            if navigation.blocked() {
                                blocked.store(true, Ordering::Release);
                            }
                            saved?;
                            return Err(e);
                        }
                        Err(e) => {
                            blocked.store(true, Ordering::Release);
                            return Err(e);
                        }
                    };
                    if navigation.blocked() {
                        blocked.store(true, Ordering::Release);
                    }
                    saved?;
                    if let Ok(mut report) = report.lock() {
                        report["preparation"] = json!({"entry_id":entry.id,"stage":preparation_stage,"request_id":id,"verified":true,"goal_completed":false});
                    }
                    return Ok(None);
                }
            }
            let mut backend = DailyBackend {
                process,
                state: &mut state,
                producer,
                gate,
                token,
                until,
                last: None,
            };
            let result = governor.continue_entry(
                &mut backend,
                &entry.as_ref().expect("eligible entry").id,
                crate::daily_observation::unix_ms()?,
                until,
            );
            // Publish quarantine while still holding the common controller.
            // A verified Research receipt followed by a failed daily witness
            // must not leave a window for manual clock/focus/research input.
            if governor.blocks_work() {
                blocked.store(true, Ordering::Release);
            }
            result.map(Some)
        })();
        let possible_input = blocked.load(Ordering::Acquire)
            || navigation.blocked()
            || crate::daily_worker::retained_quarantine(governor, |g| g.blocks_work());
        let disposition = crate::daily_worker::disposition(run.as_ref().err(), possible_input);
        if let Ok(mut report) = report.lock() {
            if disposition != crate::daily_worker::Disposition::Continue {
                report["stop_reason"] = json!(run
                    .as_ref()
                    .err()
                    .map(|e| e.code)
                    .unwrap_or("action_uncertain"));
                report["running"] = json!(false);
            }
            match run {
                Ok(Some(result)) => {
                    report["last_batch"] = json!(result);
                    report["last_error"] = Value::Null;
                }
                Ok(None) => {}
                Err(e) => {
                    report["last_error"] = json!({"code":e.code,"entry_id":refusal_entry,"reason":crate::daily_navigation::bounded_reason(&e.message),"historical":true});
                }
            }
        }
        match disposition {
            crate::daily_worker::Disposition::Quarantine => {
                blocked.store(true, Ordering::Release);
                break;
            }
            crate::daily_worker::Disposition::Stop => break,
            crate::daily_worker::Disposition::Continue => {}
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
fn compact(p: Produced, report: &Value, blocked: bool, revision: u64) -> Result<Value> {
    compact_observation(p.observation, report, blocked, revision)
}
fn compact_observation(
    observation: core::Observation,
    report: &Value,
    blocked: bool,
    revision: u64,
) -> Result<Value> {
    let mut digest = report["last_batch"]["digest"].clone();
    if !digest.is_object() {
        digest = json!({"verified":[],"retained":[]});
    }
    let olddate = digest["date_raw"].clone();
    let oldrevision = digest["plan_revision"].clone();
    let verified = digest["verified"].clone();
    digest["verified"] = json!([]);
    digest["latest_verified_ids_historical"] = verified;
    digest["last_batch_date_raw"] = olddate;
    digest["last_batch_plan_revision"] = oldrevision;
    digest["plan_revision"] = json!(revision);
    let preparation = &report["preparation"];
    digest["latest_preparation_historical"] = if preparation.is_object() {
        json!({"entry_id":preparation["entry_id"],"stage":preparation["stage"],"request_id":preparation["request_id"],"verified":preparation["verified"],"goal_completed":false})
    } else {
        Value::Null
    };
    digest["date_raw"] = json!(observation.date_raw);
    digest["sequence"] = json!(observation.sequence);
    digest["source"] = json!(observation.source);
    digest["observed_unix_ms"] = json!(observation.observed_unix_ms);
    digest["age_ms"] =
        json!(crate::daily_observation::unix_ms()?.saturating_sub(observation.observed_unix_ms));
    digest["counts"] = json!(observation.counts);
    digest["daily_budget"] = report["daily_budget"].clone();
    digest["running"] = report["running"].clone();
    digest["running_is_input_authority"] = json!(false);
    digest["stop_reason"] = report["stop_reason"].clone();
    digest["last_error"] = report["last_error"].clone();
    digest["last_preparation_refusal"] = report["last_preparation_refusal"].clone();
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
    fn plan_hints_require_exact_catalog_keys_and_preserve_specs() {
        let catalog = crate::research_folders::FolderCatalog {
            ids: vec!["infantry_folder".into(), "industry_folder".into()],
        };
        let entry: core::Entry = serde_json::from_value(json!({
            "id":"research-support", "intent":{"kind":"Research","technology_id":"support_weapons","slot":0},
            "navigation":{"folder_id":"infantry_folder"}
        })).unwrap();
        let bytes = serde_json::to_vec(&entry).unwrap();
        validate_navigation_keys(std::slice::from_ref(&entry), &catalog).unwrap();
        assert_eq!(serde_json::to_vec(&entry).unwrap(), bytes);
        for key in [
            "infantry",
            "INFANTRY_FOLDER",
            "infantry_folder ",
            "not_a_folder",
        ] {
            let mut wrong = entry.clone();
            wrong.navigation.as_mut().unwrap().folder_id = key.into();
            assert_eq!(
                validate_navigation_keys(&[wrong], &catalog)
                    .unwrap_err()
                    .code,
                "invalid_plan"
            );
        }
        let mut legacy = entry.clone();
        legacy.navigation = None;
        validate_navigation_keys(&[legacy], &catalog).unwrap();
        assert!(validate_navigation_keys(&vec![entry; 257], &catalog).is_err());
    }
    fn digest_sample() -> core::Observation {
        core::Observation {
            scope: core::Scope {
                epoch: Epoch {
                    game: crate::platform::Identity {
                        pid: 123,
                        start_ticks: "456".into(),
                        exe: "/owned/game".into(),
                        uid: 1000,
                    },
                    campaign: "fixture".into(),
                    player: "GER".into(),
                },
                anchors: [0x10000; 3],
                own_country: 0x20000,
                own_status: 0x30000,
            },
            date_raw: 60_759_371,
            sequence: 7,
            observed_unix_ms: crate::daily_observation::unix_ms().unwrap() - 5,
            source: "native generated producer fixture".into(),
            counts: core::OwnCounts {
                idle_research_slots: Some(3),
                ..Default::default()
            },
        }
    }
    #[test]
    fn fresh_digest_distinguishes_updated_plan_from_historical_stage_and_batch() {
        let report = json!({"running":true,"last_error":null,"daily_budget":{"day":706640,"used_assignment_attempts":2,"limit":4,"remaining":2,"navigation_spends_assignment_budget":false},
            "last_batch":{"digest":{"date_raw":60759347,"plan_revision":1,"verified":["old-goal"],"retained":[]}},
            "preparation":{"entry_id":"next-goal","stage":"View","request_id":"record-id","verified":true,"goal_completed":false,"private_scope":"must not copy"}});
        let out = compact_observation(digest_sample(), &report, false, 2).unwrap();
        assert_eq!(out["plan_revision"], 2);
        assert_eq!(out["last_batch_plan_revision"], 1);
        assert_eq!(out["last_batch_date_raw"], 60759347);
        assert_eq!(out["date_raw"], 60759371);
        assert_eq!(out["sequence"], 7);
        assert_eq!(out["counts"]["idle_research_slots"], 3);
        assert_eq!(out["daily_budget"]["used_assignment_attempts"], 2);
        assert_eq!(out["daily_budget"]["remaining"], 2);
        assert_eq!(out["verified"], json!([]));
        assert_eq!(out["latest_verified_ids_historical"], json!(["old-goal"]));
        assert_eq!(out["latest_preparation_historical"]["stage"], "View");
        assert_eq!(
            out["latest_preparation_historical"]["goal_completed"],
            false
        );
        assert!(out["latest_preparation_historical"]["private_scope"].is_null());
        assert_eq!(out["input_authorized"], false);
        assert!(!serde_json::to_string(&out).unwrap().contains("/owned/game"));
    }
    #[test]
    fn digest_size_trimming_preserves_fresh_counts_revision_and_history_labels() {
        let rows = (0..256)
            .map(|i| json!({"entry_id":format!("goal-{i}"),"reason":"x".repeat(160)}))
            .collect::<Vec<_>>();
        let report = json!({"running":true,"last_error":null,"preparation":{"entry_id":"pending","stage":"OpenPanel","request_id":"a".repeat(64),"verified":true},"last_batch":{"digest":{"plan_revision":1,"verified":(0..256).map(|i|format!("old-{i}")).collect::<Vec<_>>(),"retained":rows,"retained_count":256}}});
        let out = compact_observation(digest_sample(), &report, false, 3).unwrap();
        assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
        assert_eq!(out["retained_count"], 256);
        assert_eq!(out["plan_revision"], 3);
        assert_eq!(out["last_batch_plan_revision"], 1);
        assert_eq!(out["counts"]["idle_research_slots"], 3);
        assert_eq!(out["latest_preparation_historical"]["entry_id"], "pending");
        assert_eq!(out["verified"], json!([]));
        assert_eq!(out["input_authorized"], false);
    }
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
    #[test]
    fn terminal_stop_digest_labels_lifecycle_without_input_authority() {
        let report = json!({"running":false,"stop_reason":"input_lease_required","last_error":{"code":"input_lease_required"},"last_batch":null});
        let out = compact_observation(digest_sample(), &report, false, 1).unwrap();
        assert_eq!(out["running"], false);
        assert_eq!(out["running_is_input_authority"], false);
        assert_eq!(out["input_authorized"], false);
        assert_eq!(out["stop_reason"], "input_lease_required");
        assert_eq!(out["quarantined"], false);
    }
    #[test]
    fn concrete_refusal_reason_is_exposed_without_promoting_historical_authority() {
        let report = json!({"running":true,"last_error":{"code":"research_precondition","entry_id":"research-support","reason":"actual recorded bounded reason","historical":true}});
        let out = compact_observation(digest_sample(), &report, false, 1).unwrap();
        assert_eq!(
            out["last_error"]["reason"],
            "actual recorded bounded reason"
        );
        assert_eq!(out["input_authorized"], false);
        assert_eq!(out["running_is_input_authority"], false);
    }
    #[test]
    fn historical_refusal_survives_success_error_clear_within_digest_budget() {
        let historical = json!({"code":"research_precondition","entry_id":"a".repeat(128),"reason":crate::daily_navigation::bounded_reason(&"é".repeat(1000)),"historical":true});
        let report = json!({"running":true,"last_error":null,"last_preparation_refusal":historical,"last_batch":{"digest":{"verified":["successful-other-goal"],"retained":[]}}});
        let out = compact_observation(digest_sample(), &report, false, 2).unwrap();
        assert!(out["last_error"].is_null());
        assert_eq!(
            out["last_preparation_refusal"]["code"],
            "research_precondition"
        );
        assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
    }
}
