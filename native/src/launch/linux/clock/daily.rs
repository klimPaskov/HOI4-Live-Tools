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
mod production_plan;
use production_plan::{ProductionAttempt, ProductionPlan};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    version: u8,
    revision: u64,
    entries: Vec<core::Entry>,
    max_actions_per_day: u8,
    max_observation_age_ms: u64,
    review_day: Option<i64>,
    #[serde(default, skip_serializing_if = "is_false")]
    focus_permission: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    production_permission: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    max_production_steps_per_day: Option<u8>,
}
fn is_false(value: &bool) -> bool {
    !*value
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
    focus_permission: bool,
    production_permission: bool,
    max_production_steps_per_day: Option<u8>,
    details: Mutex<crate::daily_digest::Details>,
}
pub(super) struct Controller {
    enabled: bool,
    research_enabled: bool,
    focus_enabled: bool,
    production_enabled: bool,
    owner: Weak<Shared>,
    directory: PathBuf,
    active: Mutex<Option<Active>>,
    blocked: Arc<AtomicBool>,
}
impl Controller {
    pub(super) fn new(
        research_enabled: bool,
        focus_enabled: bool,
        production_enabled: bool,
        owner: Weak<Shared>,
        directory: &Path,
    ) -> Self {
        Self {
            enabled: research_enabled || focus_enabled || production_enabled,
            research_enabled,
            focus_enabled,
            production_enabled,
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
    pub(super) fn focus_enabled(&self) -> bool {
        self.focus_enabled
    }
    pub(super) fn research_enabled(&self) -> bool {
        self.research_enabled
    }
    pub(super) fn production_enabled(&self) -> bool {
        self.production_enabled
    }
    pub(super) fn tool() -> Value {
        json!({"name":"hoi4_daily_governor","description":"Optional owned daily automatic research refill worker and fresh compact digest. Start requires the existing exclusive lease and explicit typed plan. Navigation uses separately journaled verified stages; only fresh Research or separately opted-in Focus Start completes an assignment. Focus requires default-off owner daily_focus plus plan.focus_permission and existing profile. With both domain permissions, one journaled ClosePolitics stage can bridge a completed Focus to an empty research slot; a separately typed research-to-Politics shortcut requires both permissions and positive own panel/tree facts. Both routes passed bounded run049; later runs verified research completion/refill and actual Rhineland completion. An optional Focus entry focus_search string uses the verified manual search stages with one durable bounded match chain; existing entries retain direct node navigation. Query replacement and worker search live validation are pending. Successive Focus automation and full campaigns remain unverified. Production allocation has a default-off preview requiring owner daily_production, plan production_permission and max_production_steps_per_day (1..16); durable one-factory steps precede exact goal completion. Production worker live validation and cross-panel handoffs are pending. Other economy, army and naval domains remain unavailable. Unknown/uncertain input is never replayed. Stop does not pause the game. Compacted history includes a snapshot_id; details retrieves an exact historical section without game reads or input. The Focus section reports latest-per-kind history, not the full navigation ledger.","inputSchema":{"type":"object","oneOf":[{"properties":{"op":{"enum":["start","update"]},"lease_token":{"type":"string"},"plan":{"type":"object"}},"required":["op","lease_token","plan"],"additionalProperties":false},{"properties":{"op":{"enum":["digest","stop"]}},"required":["op"],"additionalProperties":false},{"properties":{"op":{"enum":["details"]},"snapshot_id":{"type":"string","minLength":64,"maxLength":64},"section":{"enum":crate::daily_digest::SECTIONS}},"required":["op","snapshot_id","section"],"additionalProperties":false}]}})
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
                crate::daily_focus_navigation::capability(
                    self.focus_enabled,
                    spec.focus_permission,
                    state.focus.is_some(),
                )?;
                production_plan::permission(
                    self.production_enabled,
                    spec.production_permission,
                    state.production.is_some(),
                    spec.max_production_steps_per_day,
                )?;
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
                // Always load Focus uncertainty, even when permission is off.
                let focus_navigation = crate::daily_focus_navigation::Navigator::open(
                    &self.directory.join("daily-focus-navigation.json"),
                    state.guard.epoch.clone(),
                    &governor.plan().entries,
                )?;
                if focus_navigation.blocked() {
                    self.blocked.store(true, Ordering::Release);
                    return Err(Error::new(
                        "action_uncertain",
                        "Unresolved Focus preparation cannot restart",
                    ));
                }
                let focus_initial = focus_navigation.latest();
                // Always load durable uncertainty, even when opt-in is absent.
                let production_plan = ProductionPlan {
                    progress: crate::production_progress::Progress::open(
                        &self.directory.join("daily-production.json"),
                        governor.plan().scope.clone(),
                        &governor.plan().entries,
                    )
                    .inspect_err(|_| self.blocked.store(true, Ordering::Release))?,
                    limit: spec.max_production_steps_per_day,
                };
                if production_plan.progress.blocked() {
                    self.blocked.store(true, Ordering::Release);
                    return Err(Error::new(
                        "action_uncertain",
                        "Unresolved production preparation cannot restart",
                    ));
                }
                let production_initial = production_plan
                    .summary((i64::from(first.observation.date_raw) - 43_800_000).div_euclid(24));
                let focus_permission = spec.focus_permission;
                let research_permission = self.research_enabled;
                let governor = Arc::new(Mutex::new(governor));
                drop(state);
                let stop = Arc::new(AtomicBool::new(false));
                let wake = Arc::new(core::Wakeup::default());
                let report = Arc::new(Mutex::new(
                    json!({"running":true,"last_batch":null,"last_error":navigation.latest_refusal(),"last_preparation_refusal":navigation.latest_refusal(),"last_stage_failure":navigation.latest_stage_failure(),"focus":focus_initial,"focus_wait":focus_initial["wait"],"focus_permission":focus_permission,"production":production_initial}),
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
 let outcome=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||worker(&g,navigation,focus_navigation,production_plan,focus_permission,research_permission,&p,&control,&gate,&weak,&token,&s,&w,&r,&blocked)));
 if outcome.is_err(){blocked.store(true,Ordering::Release);if let Ok(mut report)=r.lock(){report["last_error"]=json!({"code":"daily_quarantined","reason":"Worker panic; durable intent requires review"});}}
 if crate::daily_worker::retained_quarantine(&g,|gov|gov.blocks_work()){blocked.store(true,Ordering::Release);}s.store(true,Ordering::Release);if let Ok(mut report)=r.lock(){report["running"]=json!(false);}
 })?;
                *active = Some(Active {
                    life: crate::daily_worker::Lifecycle::new(stop, wake, handle),
                    producer,
                    report,
                    governor,
                    focus_permission,
                    production_permission: spec.production_permission,
                    max_production_steps_per_day: spec.max_production_steps_per_day,
                    details: Mutex::new(crate::daily_digest::Details::default()),
                });
                Ok(
                    json!({"started":true,"scope":"owned assignments within durable shared daily budget","focus_implemented":focus_permission,"navigation_implemented":true,"production_implemented":spec.production_permission,"production_worker_live_validation":false,"other_action_domains_implemented":false,"full_campaign_acceptance":false}),
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
                if spec.focus_permission != active.focus_permission {
                    return Err(Error::new("plan_conflict","Focus permission is fixed for this worker; stop/restart requires explicit reviewed capability"));
                }
                if spec.production_permission != active.production_permission
                    || spec.max_production_steps_per_day != active.max_production_steps_per_day
                {
                    return Err(Error::new(
                        "plan_conflict",
                        "Production permission and step limit are fixed for this worker",
                    ));
                }
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
                let (last_error, last_preparation_refusal, last_stage_failure, clock_probe) =
                    active
                        .as_ref()
                        .and_then(|a| {
                            a.report.try_lock().ok().map(|r| {
                                (
                                    r["last_error"].clone(),
                                    r["last_preparation_refusal"].clone(),
                                    r["last_stage_failure"].clone(),
                                    r["clock_probe_historical"].clone(),
                                )
                            })
                        })
                        .unwrap_or((Value::Null, Value::Null, Value::Null, Value::Null));
                let focus_wait = active
                    .as_ref()
                    .and_then(|a| a.report.try_lock().ok().map(|r| r["focus_wait"].clone()))
                    .unwrap_or(Value::Null);
                let focus = active
                    .as_ref()
                    .and_then(|a| a.report.try_lock().ok().map(|r| r["focus"].clone()))
                    .unwrap_or(Value::Null);
                let production = active
                    .as_ref()
                    .and_then(|a| a.report.try_lock().ok().map(|r| r["production"].clone()))
                    .unwrap_or(Value::Null);
                let focus_readiness = active
                    .as_ref()
                    .and_then(|a| {
                        a.report
                            .try_lock()
                            .ok()
                            .map(|r| r["focus_readiness_historical"].clone())
                    })
                    .unwrap_or(Value::Null);
                let core_state = active
                    .as_ref()
                    .map(|a| -> Result<Value> {
                        let governor = try_state(&a.governor)?;
                        let plan = governor.plan();
                        let pending = plan
                            .entries
                            .iter()
                            .filter_map(|e| match governor.entry_state(&e.id) {
                                Some(core::EntryState::Pending { .. }) => Some(
                                    json!({"entry_id":e.id,"state":governor.entry_state(&e.id)}),
                                ),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        // Saved chronology is reported through core digest without a game read.
                        Ok(json!({"pending":pending,"historical":true}))
                    })
                    .transpose()?
                    .unwrap_or(Value::Null);
                let (assignments, budget) = active
                    .as_ref()
                    .map(|a| -> Result<(Value, Value)> {
                        let mut governor = try_state(&a.governor)?;
                        // STOP must still return its uncertainty evidence if a
                        // durable file can no longer be verified.
                        let (assignments, budget) = stopped_history_or_unavailable(&mut governor);
                        if assignments["available"] == false {
                            self.blocked.store(true, Ordering::Release);
                        }
                        Ok((assignments, budget))
                    })
                    .transpose()?
                    .unwrap_or((Value::Null, Value::Null));
                Ok(
                    json!({"assignments":assignments,"daily_budget":budget,"production":production,"core_pending_historical":core_state,"focus":focus,"latest_focus_wait_historical":focus_wait,"focus_readiness_historical":focus_readiness,"worker_stopped":true,"game_time_changed":false,"uncertainty_retained":self.blocked.load(Ordering::Acquire),"last_error":last_error,"last_preparation_refusal":last_preparation_refusal,"last_stage_failure":last_stage_failure,"clock_probe_historical":clock_probe}),
                )
            }
            "details" => {
                allowed(args, &["op", "snapshot_id", "section"])?;
                let active = try_state(&self.active)?;
                let a = active
                    .as_ref()
                    .ok_or_else(|| Error::new("capability_unavailable", "No daily plan started"))?;
                let result = try_state(&a.details)?
                    .get(string(args, "snapshot_id")?, string(args, "section")?);
                result
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
                    report["assignments"] = governor.assignment_summary().inspect_err(|_| {
                        self.blocked.store(true, Ordering::Release);
                    })?;
                    compact(
                        &mut *try_state(&a.details)?,
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
    state
        .production
        .as_ref()
        .is_some_and(|p| p.journal.blocks_game(&state.guard.epoch.game))
        || state.journal.blocks_game(&state.guard.epoch.game)
        || state
            .construction
            .as_ref()
            .is_some_and(|c| c.journal.blocks_game(&state.guard.epoch.game))
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
    mut focus_navigation: crate::daily_focus_navigation::Navigator,
    mut production_plan: ProductionPlan,
    focus_permission: bool,
    research_permission: bool,
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
    let mut clock_probe = crate::daily_worker::ClockProbeHistory::default();
    while !stop.load(Ordering::Acquire) {
        let Some(shared) = owner.upgrade() else { break };
        if !shared.alive.load(Ordering::Acquire)
            || !shared.accepting_reads.load(Ordering::Acquire)
            || gate.try_read().map_or(true, |g| !*g)
        {
            break;
        }
        let mut refusal_entry = None;
        let mut production_failed = false;
        let mut at_clock_probe = false;
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
                if blocked.load(Ordering::Acquire)
                    || unresolved(&state)
                    || navigation.blocked()
                    || focus_navigation.blocked()
                    || production_plan.progress.blocked()
                {
                    blocked.store(true, Ordering::Release);
                    return Err(Error::new(
                        "action_uncertain",
                        "Shared input unresolved before idle probe",
                    ));
                }
                state.authority.authorize(token, &state.guard.epoch)?;
            }
            at_clock_probe = true;
            let raw = producer.probe_clock(process, Instant::now() + Duration::from_secs(1))?;
            at_clock_probe = false;
            clock_probe.verified(raw);
            let mut governor = try_state(governor)?;
            let day = (i64::from(raw) - 43_800_000).div_euclid(24);
            let heads: Vec<_> = if research_permission {
                governor
                    .pending_research(day)
                    .into_iter()
                    .filter(|e| !navigation.refused_today(&e.id, day))
                    .collect()
            } else {
                vec![]
            };
            let focus_entry = if focus_permission {
                governor
                    .pending_focus(day)
                    .filter(|e| !focus_navigation.refused_today(e, day))
            } else {
                None
            };
            let production_heads: Vec<_> = governor
                .pending_production(day)
                .into_iter()
                .filter(|entry| production_plan.eligible(entry, day))
                .collect();
            if heads.is_empty() && focus_entry.is_none() && production_heads.is_empty() {
                return Ok(None);
            }
            let mut state = try_state(state)?;
            let lease_end = state.authority.authorize(token, &state.guard.epoch)?;
            // Separate absolute ceilings share one batch origin. Focus matches
            // the existing manual ceiling; Research never inherits that time.
            let batch_started = Instant::now();
            let until = (batch_started + Duration::from_secs(5)).min(lease_end);
            let focus_until = (batch_started + Duration::from_secs(10)).min(lease_end);
            if unresolved(&state)
                || navigation.blocked()
                || focus_navigation.blocked()
                || production_plan.progress.blocked()
            {
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
            if !governor.budget_available(day) {
                let observation = (|| -> Result<()> {
                    if let Some(entry) = &focus_entry {
                        let core::Intent::Focus { focus_id } = &entry.intent else {
                            unreachable!()
                        };
                        let target = crate::focus_stage::Target {
                            query: None,
                            epoch: state.guard.epoch.clone(),
                            stage: crate::focus_stage::Stage::Start,
                            focus_id: focus_id.clone(),
                        };
                        let (own, context, binding) =
                            focus::observed_bound(process, &mut state.guard, &target)?;
                        focus_scope(&context, &governor.plan().scope)?;
                        if Instant::now() >= focus_until {
                            return Err(Error::new(
                                "focus_deadline",
                                "Read-only Focus wait exceeded original deadline",
                            ));
                        }
                        if matches!(
                            crate::daily_focus_navigation::next(&own, &target)?,
                            crate::daily_focus_navigation::Next::Wait
                        ) {
                            report_focus_wait(
                                report,
                                &mut focus_navigation,
                                entry,
                                &own,
                                &context,
                                binding,
                            )?;
                        }
                    }
                    Ok(())
                })();
                if focus_navigation.blocked() || governor.blocks_work() {
                    blocked.store(true, Ordering::Release);
                }
                observation?;
                return Ok(None);
            }
            let mut focus_waited = false;
            // At most one production step per batch, bounded across all entries
            // and restarts by the separate durable daily step budget. Existing
            // Research/Focus ordering is unchanged when production is disabled.
            for entry in &production_heads {
                let result = production_plan::run(
                    &mut production_plan,
                    &mut governor,
                    producer,
                    &mut state,
                    process,
                    gate,
                    token,
                    focus_until,
                    stop,
                    day,
                    entry,
                );
                if production_plan.progress.blocked() || governor.blocks_work() {
                    blocked.store(true, Ordering::Release);
                }
                if let Err(e) = &result {
                    production_failed = true;
                    refusal_entry = Some(entry.id.clone());
                    if e.code == "action_uncertain" {
                        blocked.store(true, Ordering::Release);
                    }
                }
                if let Ok(mut r) = report.lock() {
                    r["production"] = production_plan.summary(day);
                }
                match result? {
                    production_plan::Run::Advanced(value) => return Ok(value),
                    production_plan::Run::Deferred => {}
                }
            }
            if focus_precedes(
                &governor.plan().entries,
                focus_entry.as_ref(),
                heads.first(),
            ) {
                let result = run_focus(
                    &mut governor,
                    &mut focus_navigation,
                    producer,
                    &mut state,
                    process,
                    gate,
                    token,
                    focus_until,
                    research_permission,
                    stop,
                    day,
                    focus_entry.as_ref().expect("focus head"),
                    report,
                );
                if focus_navigation.blocked() || governor.blocks_work() {
                    blocked.store(true, Ordering::Release);
                }
                match result {
                    Ok(FocusRun::Wait) => {
                        focus_waited = true;
                    }
                    Ok(FocusRun::Done(value)) => return Ok(value),
                    Err(e) => return Err(e),
                }
            }
            if heads.is_empty() {
                return Ok(None);
            }
            // Re-read complete own canonical/UI state under the common lock.
            // Occupied queue heads defer without a permanent day refusal. Later
            // entries for that slot remain blocked; other slots can progress.
            let probe = producer.probe_research(process, until)?;
            state
                .guard
                .accept_context(process, &probe.context().campaign)?;
            // A day change defers this batch; original budget/day anchors remain.
            if (i64::from(probe.context().campaign.date_raw) - 43_800_000).div_euclid(24) != day {
                return Ok(None);
            }
            if probe.all_heads_occupied(&heads)? {
                if let Some(entry) = focus_entry.as_ref().filter(|_| !focus_waited) {
                    let result = run_focus(
                        &mut governor,
                        &mut focus_navigation,
                        producer,
                        &mut state,
                        process,
                        gate,
                        token,
                        focus_until,
                        research_permission,
                        stop,
                        day,
                        entry,
                        report,
                    );
                    if focus_navigation.blocked() || governor.blocks_work() {
                        blocked.store(true, Ordering::Release);
                    }
                    return result.map(|r| match r {
                        FocusRun::Wait => None,
                        FocusRun::Done(value) => value,
                    });
                }
                return Ok(None);
            }
            // Probe is never reused as UI/action/completion authority.
            let (queue_ui, _) = research::observed_state(process, &mut state.guard)?;
            let entry =
                crate::daily_navigation::available_research_head(&heads, &queue_ui)?.cloned();
            if entry.is_none() {
                return Ok(None);
            }
            if let Some(entry) = &entry {
                let retry_target =
                    navigation.retry_target(entry, day, crate::daily_observation::unix_ms()?)?;
                if crate::daily_focus_navigation::needs_research_handoff(
                    focus_permission,
                    retry_target.is_some(),
                    entry,
                    &queue_ui,
                ) {
                    let result = run_research_handoff(
                        &mut governor,
                        &mut focus_navigation,
                        &mut state,
                        process,
                        gate,
                        token,
                        focus_until,
                        day,
                        entry,
                        &queue_ui,
                        report,
                    );
                    if focus_navigation.blocked() || governor.blocks_work() {
                        blocked.store(true, Ordering::Release);
                    }
                    if let Err(e) = &result {
                        if crate::daily_navigation::known_preparation_refusal(e.code) {
                            refusal_entry = Some(entry.id.clone());
                            let saved = navigation.refuse_preparation(entry, day, e);
                            if navigation.blocked() {
                                blocked.store(true, Ordering::Release);
                            }
                            if let Ok(mut report) = report.lock() {
                                report["last_preparation_refusal"] =
                                    json!(navigation.latest_refusal());
                            }
                            saved?;
                        }
                    }
                    result?;
                    // One navigation stage per batch. Research gets a new
                    // five-second ceiling on the next batch, not Focus's ten.
                    return Ok(None);
                }
                let prepared = (|| -> Result<Option<crate::research_stage::Target>> {
                    let mut ui = queue_ui;
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
                    let target = crate::daily_navigation::next(entry, &ui)?;
                    if retry_target
                        .as_ref()
                        .is_some_and(|expected| target.as_ref() != Some(expected))
                    {
                        return Err(Error::new(
                            "research_precondition",
                            "Fresh own UI changed the pending retry target; no input",
                        ));
                    }
                    Ok(target)
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
                    let id = match navigation.begin(
                        entry,
                        target.clone(),
                        day,
                        crate::daily_observation::unix_ms()?,
                    ) {
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
                        &mut state,
                        process,
                        gate,
                        token,
                        &id,
                        target.clone(),
                        until,
                    );
                    let saved = match result {
                        Ok((receipt, _)) => navigation.verified(&receipt),
                        Err(e) => {
                            refusal_entry = Some(entry.id.clone());
                            let evidence = if unresolved(&state) {
                                crate::research_stage::FailureEvidence::Unknown
                            } else {
                                research::failure_evidence(&state, &id, &target, &e)
                            };
                            let saved = navigation.stage_failed(
                                &e,
                                evidence,
                                crate::daily_observation::unix_ms()?,
                            );
                            if navigation.blocked() {
                                blocked.store(true, Ordering::Release);
                            }
                            if let Ok(mut report) = report.lock() {
                                report["last_stage_failure"] =
                                    json!(navigation.latest_stage_failure());
                            }
                            saved?;
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
                focus: None,
                production: None,
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
        if let Ok(mut r) = report.lock() {
            r["focus"] = focus_navigation.latest();
            // History only; digest dates do not turn this into a fresh factory read.
            r["production"]["progress"] = production_plan.progress.summary();
        }
        let possible_input = blocked.load(Ordering::Acquire)
            || navigation.blocked()
            || focus_navigation.blocked()
            || production_plan.progress.blocked()
            || crate::daily_worker::retained_quarantine(governor, |g| g.blocks_work());
        let mut disposition = crate::daily_worker::disposition(run.as_ref().err(), possible_input);
        if production_failed && disposition == crate::daily_worker::Disposition::Continue {
            disposition = crate::daily_worker::Disposition::Stop;
        }
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
                    if !clock_probe.rejected(&e, at_clock_probe, disposition) {
                        report["last_error"] = json!({"code":e.code,"entry_id":refusal_entry,"reason":crate::daily_navigation::bounded_reason(&e.message),"historical":true,
                            "phase":if at_clock_probe {"clock_probe"} else {"worker_batch"}});
                    }
                }
            }
            report["clock_probe_historical"] = clock_probe.summary();
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
    focus: Option<FocusAttempt<'a>>,
    production: Option<ProductionAttempt<'a>>,
}
struct FocusAttempt<'a> {
    navigation: &'a mut crate::daily_focus_navigation::Navigator,
    revision: u64,
}
impl core::Backend for DailyBackend<'_> {
    fn unresolved_input(&mut self) -> Result<bool> {
        Ok(unresolved(self.state)
            || self
                .production
                .as_ref()
                .is_some_and(|p| p.plan.progress.blocked()))
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
    fn readiness(
        &mut self,
        entry: &core::Entry,
        observed: &core::Observation,
    ) -> Result<core::Readiness> {
        if matches!(
            entry.intent,
            core::Intent::Economy {
                target: core::EconomyTarget::Production { .. }
            }
        ) {
            return self.production_ready(entry, observed);
        }
        if let core::Intent::Focus { focus_id } = &entry.intent {
            if self.focus.is_none() {
                return Ok(core::Readiness::Unavailable(
                    "Daily Focus not enabled".into(),
                ));
            }
            let target = crate::focus_stage::Target {
                query: None,
                epoch: self.state.guard.epoch.clone(),
                stage: crate::focus_stage::Stage::Start,
                focus_id: focus_id.clone(),
            };
            let (own, context, _) =
                focus::observed_bound(self.process, &mut self.state.guard, &target)?;
            if let Some(p) = &self.last {
                join_focus_canonical(&context, p)?;
                if context.campaign.date_raw < p.context.campaign.date_raw {
                    return Err(Error::new(
                        "torn_read",
                        "Focus readiness chronology regressed",
                    ));
                }
            }
            return match crate::daily_focus_navigation::next(&own, &target)? {
                crate::daily_focus_navigation::Next::Wait => Ok(core::Readiness::WaitingForFocus),
                crate::daily_focus_navigation::Next::Start => Ok(core::Readiness::Ready),
                _ => Ok(core::Readiness::Deferred(
                    "Own Focus Start detail must be prepared".into(),
                )),
            };
        }
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
        if state.researching.get(*slot as usize).is_none() {
            return Ok(core::Readiness::Unavailable(
                "Requested own slot is unavailable".into(),
            ));
        }
        if state.researching.get(*slot as usize) != Some(&None) {
            return Ok(core::Readiness::WaitingForResearchSlot);
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
        before: &core::Observation,
        until: Instant,
    ) -> Result<core::Outcome> {
        if matches!(
            attempt.entry.intent,
            core::Intent::Economy {
                target: core::EconomyTarget::Production { .. }
            }
        ) {
            return self.attempt_production(attempt, before, until.min(self.until));
        }
        if matches!(attempt.entry.intent, core::Intent::Focus { .. }) {
            return self.attempt_focus(attempt, until.min(self.until));
        }
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
fn focus_scope(context: &crate::own_focus::FocusContext, scope: &core::Scope) -> Result<()> {
    if context.campaign.player != scope.epoch.player
        || context.campaign.anchors != scope.anchors
        || [context.anchors[0], context.anchors[6]] != [scope.own_country, scope.own_status]
    {
        return Err(Error::new(
            "ownership_changed",
            "Full Focus own country/status/campaign scope changed",
        ));
    }
    Ok(())
}
fn report_focus_wait(
    report: &Mutex<Value>,
    navigation: &mut crate::daily_focus_navigation::Navigator,
    entry: &core::Entry,
    own: &crate::focus_stage::OwnState,
    context: &crate::own_focus::FocusContext,
    binding: crate::daily_focus_navigation::OwnBinding,
) -> Result<()> {
    let crate::focus_stage::ActiveFocus::Active(active) = &own.active_focus else {
        return Err(Error::new(
            "focus_unavailable",
            "Positive active Focus wait required",
        ));
    };
    navigation.record_wait(entry, active, context.campaign.date_raw, binding)?;
    let stamp = crate::daily_observation::unix_ms()?;
    if let Ok(mut r) = report.lock() {
        r["focus_wait"] = json!({"entry_id":entry.id,"active_focus_id":active,"date_raw":context.campaign.date_raw,"observed_unix_ms":stamp,"source":"native witnessed own focus","historical":true,"navigation_attempted":false,"assignment_budget_consumed":false});
    }
    Ok(())
}
fn focus_precedes(
    entries: &[core::Entry],
    focus: Option<&core::Entry>,
    research: Option<&core::Entry>,
) -> bool {
    let Some(focus) = focus else { return false };
    let position = |entry: &core::Entry| {
        entries
            .iter()
            .position(|e| e.id == entry.id)
            .unwrap_or(usize::MAX)
    };
    research.is_none_or(|r| position(focus) < position(r))
}
enum FocusRun {
    Wait,
    Done(Option<core::DailyResult>),
}
#[allow(clippy::too_many_arguments)]
fn run_focus_preparation(
    navigation: &mut crate::daily_focus_navigation::Navigator,
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    until: Instant,
    day: i64,
    entry: &core::Entry,
    revision: u64,
    target: crate::focus_stage::Target,
    binding: crate::daily_focus_navigation::OwnBinding,
    report: &Mutex<Value>,
) -> Result<()> {
    let id = navigation.begin(entry.clone(), target.clone(), day, revision, binding)?;
    let result = focus::execute_internal(state, process, gate, token, &id, target.clone(), until);
    match result {
        Ok(result) => {
            let execution = result.execution;
            let after = if target.stage == crate::focus_stage::Stage::SearchField
                || target.stage.uses_query()
            {
                result.binding.ok_or_else(|| {
                    Error::new(
                        "action_uncertain",
                        "Current search receipt binding is absent",
                    )
                })?
            } else {
                focus::observed_bound(process, &mut state.guard, &target)?.2
            };
            navigation.verified(&execution, &after)?;
            if Instant::now() >= until {
                let error = Error::new(
                    "action_uncertain",
                    "Focus preparation receipt exceeded original deadline",
                );
                if let Err(storage) = navigation.quarantine_after_verified(&error) {
                    return Err(Error::new(
                        "action_uncertain",
                        format!(
                            "{}; terminal uncertainty persistence failed: {}",
                            error.message, storage
                        ),
                    ));
                }
                return Err(error);
            }
            if let Ok(mut r) = report.lock() {
                r["focus"] = navigation.latest();
            }
            Ok(())
        }
        Err(e) => {
            let evidence = focus::failure_evidence(state, &id, &target, &e);
            navigation.failed(&e, evidence)?;
            Err(e)
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn run_research_handoff(
    governor: &mut core::Governor,
    navigation: &mut crate::daily_focus_navigation::Navigator,
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    until: Instant,
    day: i64,
    research_entry: &core::Entry,
    research: &crate::research_stage::OwnState,
    report: &Mutex<Value>,
) -> Result<()> {
    // Called only with explicit Focus permission, an eligible research head
    // and available shared budget, under the common controller lock.
    let entry = governor.completed_focus_hint()?.ok_or_else(|| {
        Error::new(
            "research_precondition",
            "A durable completed own Focus is required as handoff key hint",
        )
    })?;
    navigation.check_handoff(&entry, day)?;
    let core::Intent::Focus { focus_id } = &entry.intent else {
        unreachable!()
    };
    let target = crate::focus_stage::Target {
        query: None,
        epoch: state.guard.epoch.clone(),
        stage: crate::focus_stage::Stage::ClosePolitics,
        focus_id: focus_id.clone(),
    };
    let prepared = (|| -> Result<_> {
        state.authority.authorize(token, &state.guard.epoch)?;
        let (own, context, binding) = focus::observed_bound(process, &mut state.guard, &target)?;
        focus_scope(&context, &governor.plan().scope)?;
        if Instant::now() >= until {
            return Err(Error::new(
                "focus_deadline",
                "Focus handoff observation exceeded original deadline",
            ));
        }
        if (i64::from(context.campaign.date_raw) - 43_800_000).div_euclid(24) != day {
            return Ok(None);
        }
        let target = crate::daily_focus_navigation::research_handoff_target(
            &entry,
            research_entry,
            &own,
            research,
        )?;
        Ok(Some((target, binding)))
    })();
    let (target, binding) = match prepared {
        Ok(Some(v)) => v,
        Ok(None) => return Ok(()),
        Err(e) => {
            navigation.refuse_read_only(&entry, day, &e)?;
            return Err(e);
        }
    };
    run_focus_preparation(
        navigation,
        state,
        process,
        gate,
        token,
        until,
        day,
        &entry,
        governor.plan().revision,
        target,
        binding,
        report,
    )
}
#[allow(clippy::too_many_arguments)]
fn run_focus(
    governor: &mut core::Governor,
    navigation: &mut crate::daily_focus_navigation::Navigator,
    producer: &Producer,
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    until: Instant,
    research_permission: bool,
    stop: &AtomicBool,
    day: i64,
    entry: &core::Entry,
    report: &Mutex<Value>,
) -> Result<FocusRun> {
    let core::Intent::Focus { focus_id } = &entry.intent else {
        return Err(Error::new("invalid_plan", "Focus head required"));
    };
    state.authority.authorize(token, &state.guard.epoch)?;
    if unresolved(state) || navigation.blocked() {
        return Err(Error::new(
            "action_uncertain",
            "Shared/Focus preparation input unresolved",
        ));
    }
    let target = crate::focus_stage::Target {
        query: None,
        epoch: state.guard.epoch.clone(),
        stage: crate::focus_stage::Stage::Start,
        focus_id: focus_id.clone(),
    };
    let mut rejected = Vec::new();
    let result = (|| -> Result<_> {
        let ready = crate::daily_focus_navigation::observed_ready(
            &target,
            until,
            || {
                if stop.load(Ordering::Acquire) {
                    return Ok(None);
                }
                drop(input_permit(gate)?);
                state.authority.authorize(token, &state.guard.epoch)?;
                if unresolved(state) || navigation.blocked() {
                    return Err(Error::new(
                        "action_uncertain",
                        "Input unresolved during Focus readiness",
                    ));
                }
                let (mut own, mut context, mut binding) =
                    focus::observed_bound(process, &mut state.guard, &target)?;
                focus_scope(&context, &governor.plan().scope)?;
                if entry.focus_search.is_some()
                    && matches!(
                        crate::daily_focus_navigation::next(&own, &target),
                        Ok(crate::daily_focus_navigation::Next::Stage(
                            crate::focus_stage::Stage::View
                        ))
                    )
                {
                    let search = crate::focus_stage::Target {
                        stage: crate::focus_stage::Stage::SearchField,
                        ..target.clone()
                    };
                    (own, context, binding) =
                        focus::observed_bound(process, &mut state.guard, &search)?;
                    focus_scope(&context, &governor.plan().scope)?;
                }
                if stop.load(Ordering::Acquire)
                    || (i64::from(context.campaign.date_raw) - 43_800_000).div_euclid(24) != day
                {
                    return Ok(None);
                }
                Ok(Some((own, (context, binding))))
            },
            &mut rejected,
        )?;
        let Some(ready) = ready else { return Ok(None) };
        let (context, binding) = ready.observed;
        if matches!(ready.next, crate::daily_focus_navigation::Next::Wait) {
            report_focus_wait(
                report,
                navigation,
                entry,
                &ready.own,
                &context,
                binding.clone(),
            )?;
        }
        Ok(Some((ready.next, binding, ready.own)))
    })();
    if !rejected.is_empty() {
        // Latest diagnostic for this worker lifetime. This is neither durable
        // action history nor permission to retry a navigation/input operation.
        if let Ok(mut r) = report.lock() {
            r["focus_readiness_historical"] = json!({
                "entry_id":entry.id,"day":day,"rejected_reads":rejected,
                "outcome":match &result { Ok(Some(_)) => "ready", Ok(None) => "deferred", Err(_) => "failed" },
                "historical":true,"worker_lifetime_only":true,"input_authorized":false,
                "navigation_attempted_during_reads":false,"assignment_budget_consumed_during_reads":false
            });
        }
    }
    let (next, mut binding, own) = match result {
        Ok(Some(value)) => value,
        Ok(None) => return Ok(FocusRun::Wait),
        Err(e) => {
            navigation.refuse_read_only(entry, day, &e)?;
            return Err(e);
        }
    };
    if stop.load(Ordering::Acquire) {
        return Ok(FocusRun::Wait);
    }
    match next {
        crate::daily_focus_navigation::Next::Wait => Ok(FocusRun::Wait),
        crate::daily_focus_navigation::Next::Stage(stage) => {
            if let Err(error) =
                crate::daily_focus_navigation::preparation_permission(stage, research_permission)
            {
                navigation.refuse_read_only(entry, day, &error)?;
                return Err(error);
            }
            let target = if stage == crate::focus_stage::Stage::View && entry.focus_search.is_some()
            {
                match navigation.search_target(entry, &own, &binding) {
                    Ok(target) => {
                        if target.stage == crate::focus_stage::Stage::View {
                            binding = binding.without_search();
                        }
                        target
                    }
                    Err(error) => {
                        navigation.refuse_read_only(entry, day, &error)?;
                        return Err(error);
                    }
                }
            } else {
                crate::focus_stage::Target { stage, ..target }
            };
            run_focus_preparation(
                navigation,
                state,
                process,
                gate,
                token,
                until,
                day,
                entry,
                governor.plan().revision,
                target,
                binding,
                report,
            )?;
            Ok(FocusRun::Done(None))
        }
        crate::daily_focus_navigation::Next::Start => {
            let revision = governor.plan().revision;
            let mut backend = DailyBackend {
                process,
                state,
                producer,
                gate,
                token,
                until,
                last: None,
                focus: Some(FocusAttempt {
                    navigation,
                    revision,
                }),
                production: None,
            };
            governor
                .continue_entry(
                    &mut backend,
                    &entry.id,
                    crate::daily_observation::unix_ms()?,
                    until,
                )
                .map(|value| {
                    if let Ok(mut r) = report.lock() {
                        r["focus"] = backend.focus.as_ref().expect("checked").navigation.latest();
                        r["core_pending_historical"] = match governor.entry_state(&entry.id) {
                            Some(core::EntryState::Pending { attempts, reason, reason_day }) => json!({"entry_id":entry.id,"attempts":attempts,"reason":reason,"reason_day":reason_day,"historical":true}),
                            _ => Value::Null,
                        };
                        r["daily_budget"] = json!({"day":day,"used_assignment_attempts":governor.used_actions(day),"limit":governor.plan().max_actions_per_day,"remaining":usize::from(governor.plan().max_actions_per_day).saturating_sub(governor.used_actions(day)),"navigation_spends_assignment_budget":false});
                    }
                    FocusRun::Done(Some(value))
                })
        }
    }
}
fn join_focus_canonical(context: &crate::own_focus::FocusContext, p: &Produced) -> Result<()> {
    let a = &context.campaign;
    let b = &p.context.campaign;
    if a.player != b.player
        || a.playthrough_id != b.playthrough_id
        || a.anchors != b.anchors
        || a.difficulty != b.difficulty
        || a.historical_raw != b.historical_raw
        || [context.anchors[0], context.anchors[6]] != p.context.own
        || context.slots != p.context.slots
    {
        return Err(Error::new(
            "torn_read",
            "Full Focus and canonical daily own scopes disagree",
        ));
    }
    Ok(())
}
impl DailyBackend<'_> {
    fn focus_start_failure(
        &mut self,
        attempt: &core::Attempt,
        target: &crate::focus_stage::Target,
        error: &Error,
        failures: &mut u8,
        until: Instant,
    ) -> Result<bool> {
        let evidence = focus::failure_evidence(self.state, &attempt.request_id, target, error);
        *failures += 1;
        let backoff = Duration::from_millis(100 * u64::from(*failures));
        let retry = !unresolved(self.state)
            && crate::daily_focus_navigation::retry_before_intent(error, evidence, *failures)
            && until.saturating_duration_since(Instant::now()) > backoff;
        let f = self.focus.as_mut().expect("checked Focus permission");
        f.navigation
            .record_start_failure(attempt, f.revision, error, evidence, *failures, retry)
            .map_err(|storage| {
                Error::new(
                    "action_uncertain",
                    format!(
                        "{}: {}; Focus failure persistence failed: {}",
                        error.code, error.message, storage
                    ),
                )
            })?;
        if evidence == crate::focus_stage::FailureEvidence::Unknown || unresolved(self.state) {
            return Err(Error::new(error.code, error.message.clone()));
        }
        if retry {
            std::thread::sleep(backoff);
        }
        Ok(retry)
    }
    fn attempt_focus(&mut self, attempt: &core::Attempt, until: Instant) -> Result<core::Outcome> {
        let core::Intent::Focus { focus_id } = &attempt.entry.intent else {
            return Err(Error::new("invalid_plan", "Focus attempt required"));
        };
        if self.focus.is_none() {
            return Ok(core::Outcome::NotQueued(
                "Daily Focus permission unavailable".into(),
            ));
        }
        let target = crate::focus_stage::Target {
            query: None,
            epoch: attempt.scope.epoch.clone(),
            stage: crate::focus_stage::Stage::Start,
            focus_id: focus_id.clone(),
        };
        let mut failures = 0u8;
        let (before, before_binding, execution) = loop {
            if unresolved(self.state) || self.focus.as_ref().expect("checked").navigation.blocked()
            {
                return Err(Error::new(
                    "action_uncertain",
                    "Shared Focus retry input unresolved",
                ));
            }
            let prepared = (|| -> Result<_> {
                self.state
                    .authority
                    .authorize(self.token, &attempt.scope.epoch)?;
                drop(super::input_permit(self.gate)?);
                if Instant::now() >= until {
                    return Err(Error::new(
                        "focus_deadline",
                        "Original Focus deadline expired before input",
                    ));
                }
                let fresh = self.producer.observe(self.process, until)?;
                if fresh.observation.scope != attempt.scope
                    || fresh.observation.sequence <= attempt.before_sequence
                    || (i64::from(fresh.observation.date_raw) - 43_800_000).div_euclid(24)
                        != attempt.day
                {
                    return Err(Error::new(
                        "ownership_changed",
                        "Focus retry scope/day changed",
                    ));
                }
                self.state
                    .guard
                    .accept_context(self.process, &fresh.context.campaign)?;
                let (own, before, binding) =
                    focus::observed_bound(self.process, &mut self.state.guard, &target)?;
                focus_scope(&before, &attempt.scope)?;
                join_focus_canonical(&before, &fresh)?;
                if before.campaign.date_raw < fresh.context.campaign.date_raw
                    || (i64::from(before.campaign.date_raw) - 43_800_000).div_euclid(24)
                        != attempt.day
                {
                    return Err(Error::new("ownership_changed", "Focus retry date changed"));
                }
                if Instant::now() >= until {
                    return Err(Error::new(
                        "focus_deadline",
                        "Focus readiness exceeded original deadline",
                    ));
                }
                if !matches!(
                    crate::daily_focus_navigation::next(&own, &target)?,
                    crate::daily_focus_navigation::Next::Start
                ) {
                    return Err(Error::new(
                        "focus_precondition",
                        "Fresh own Focus Start precondition changed",
                    ));
                }
                Ok((before, binding))
            })();
            let (before, binding) = match prepared {
                Ok(v) => v,
                Err(e) => {
                    if self.focus_start_failure(attempt, &target, &e, &mut failures, until)? {
                        continue;
                    }
                    return Ok(core::Outcome::NotQueued(e.code.into()));
                }
            };
            match focus::execute_internal(
                self.state,
                self.process,
                self.gate,
                self.token,
                &attempt.request_id,
                target.clone(),
                until,
            ) {
                Ok(result) => break (before, binding, result.execution),
                Err(e) => {
                    if self.focus_start_failure(attempt, &target, &e, &mut failures, until)? {
                        continue;
                    }
                    return Ok(core::Outcome::NotQueued(e.code.into()));
                }
            }
        };
        // After successful execution, all later failures are possible-input errors.
        let (_, after, after_binding) =
            focus::observed_bound(self.process, &mut self.state.guard, &target)?;
        let fresh = self.producer.observe(self.process, until)?;
        self.state
            .guard
            .accept_context(self.process, &fresh.context.campaign)?;
        join_focus_canonical(&after, &fresh)?;
        if after.campaign.date_raw > fresh.context.campaign.date_raw {
            return Err(Error::new(
                "torn_read",
                "Canonical Focus Start chronology regressed",
            ));
        }
        let receipt = core::VerifiedReceipt::from_focus(
            attempt.clone(),
            &execution,
            &before,
            &after,
            fresh.observation.clone(),
        )?;
        if Instant::now() >= until {
            return Err(Error::new(
                "action_uncertain",
                "Focus final canonical observation exceeded original deadline",
            ));
        }
        let focus = self.focus.as_mut().expect("checked Focus permission");
        focus.navigation.record_start(
            attempt,
            focus.revision,
            before_binding,
            after_binding,
            execution.receipt,
            fresh.observation,
        )?;
        Ok(core::Outcome::Verified(Box::new(receipt)))
    }
}

fn stopped_history_or_unavailable(governor: &mut core::Governor) -> (Value, Value) {
    governor.stopped_history().unwrap_or_else(|e| {
        let unavailable = json!({"available":false,"source":"durable_plan",
            "historical":true,"reason":e.code,"current_state_verified":false,"input_authorized":false});
        (unavailable.clone(), unavailable)
    })
}
fn compact(
    details: &mut crate::daily_digest::Details,
    p: Produced,
    report: &Value,
    blocked: bool,
    revision: u64,
) -> Result<Value> {
    compact_observation_with_details(details, p.observation, report, blocked, revision)
}
#[cfg(test)]
fn compact_observation(
    observation: core::Observation,
    report: &Value,
    blocked: bool,
    revision: u64,
) -> Result<Value> {
    compact_observation_with_details(
        &mut crate::daily_digest::Details::default(),
        observation,
        report,
        blocked,
        revision,
    )
}
fn compact_observation_with_details(
    details: &mut crate::daily_digest::Details,
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
    digest["assignments"] = report["assignments"].clone();
    digest["core_pending_historical"] = report["core_pending_historical"].clone();
    digest["running"] = report["running"].clone();
    digest["running_is_input_authority"] = json!(false);
    digest["stop_reason"] = report["stop_reason"].clone();
    digest["last_error"] = report["last_error"].clone();
    if report["clock_probe_historical"].is_object() {
        digest["clock_probe_historical"] = report["clock_probe_historical"].clone();
    }
    digest["last_preparation_refusal"] = report["last_preparation_refusal"].clone();
    digest["last_stage_failure"] = report["last_stage_failure"].clone();
    digest["focus"] = report["focus"].clone();
    digest["focus_permission"] = report["focus_permission"].clone();
    digest["production"] = report["production"].clone();
    digest["latest_focus_wait_historical"] = report["focus_wait"].clone();
    digest["focus_readiness_historical"] = report["focus_readiness_historical"].clone();
    digest["quarantined"] = json!(blocked);
    digest["input_authorized"] = json!(false);
    // Retain the complete sanitized report before any optional-history pruning.
    // The exact hash prevents a later digest from silently replacing its causes.
    let full = digest.clone();
    let snapshot_id = details.retain(&full)?;
    // Publish a duplicated wait only once. Absent optional history records do
    // not establish current game state; their freshness remains historical.
    let wait_duplicated = digest["focus"]["wait"].is_object()
        && digest["focus"]["wait"] == digest["latest_focus_wait_historical"];
    if let Some(focus) = digest.get_mut("focus").and_then(Value::as_object_mut) {
        if wait_duplicated {
            focus.remove("wait");
        }
        focus.retain(|_, value| !value.is_null());
    }
    for key in ["latest_preparation_historical", "core_pending_historical"] {
        if digest[key].is_null() {
            digest.as_object_mut().unwrap().remove(key);
        }
    }
    // A matching durable Complete already carries the original Start identity.
    // Keep a reference rather than duplicating it, and omit only the successful
    // preparation; failure/refusal/uncertainty history is never removed here.
    let latest = &digest["assignments"]["latest_completion"];
    let start = &digest["focus"]["start"];
    let completed_start = digest["assignments"]["available"] == true
        && latest["state"] == "Complete"
        && latest["entry_id"].is_string()
        && latest["request_id"].is_string()
        && start["entry_id"] == latest["entry_id"]
        && start["request_id"] == latest["request_id"];
    if completed_start {
        let completed_id = latest["entry_id"].clone();
        let completed_request = latest["request_id"].clone();
        digest["focus"]["start"] = json!({"stage":"Start","historical":true,
            "completion_ref":"assignments.latest_completion"});
        let preparation = &digest["focus"]["preparation"];
        if preparation["entry_id"] == completed_id
            && preparation["verified"] == true
            && matches!(
                preparation["stage"].as_str(),
                Some("OpenPolitics" | "OpenFocus" | "View")
            )
        {
            digest["focus"]
                .as_object_mut()
                .unwrap()
                .remove("preparation");
            digest["focus"]["preparation_omitted_after_completion"] = json!(true);
        }
        let failure = &digest["focus"]["start_failure"];
        if failure["entry_id"] == completed_id && failure["request_id"] == completed_request {
            // The failure belongs to this very same completed reservation.
            // Preserve every cause/evidence field, sharing only its identity.
            let failure = digest["focus"]["start_failure"].as_object_mut().unwrap();
            failure.remove("entry_id");
            failure.remove("request_id");
            failure.insert(
                "completion_ref".into(),
                json!("assignments.latest_completion"),
            );
        }
    }
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
            } else if let Some(ids) = digest
                .get_mut("assignments")
                .and_then(|assignments| assignments.get_mut("completed_ids"))
                .and_then(Value::as_array_mut)
                .filter(|v| !v.is_empty())
            {
                ids.pop();
                digest["assignments"]["completed_ids_truncated"] = json!(true);
            } else {
                return crate::daily_digest::overflow(&full, &snapshot_id);
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
    fn recovered_readiness_diagnostics_survive_digest_compaction_as_history_only() {
        let history = json!({"entry_id":"queued-focus","day":706640,"rejected_reads":[{"code":"torn_read","message":"game clock changed"}],"outcome":"ready","historical":true,"worker_lifetime_only":true,"input_authorized":false});
        let report = json!({"running":true,"focus_readiness_historical":history,"last_error":null,
            "last_batch":{"digest":{"verified":[],"retained":(0..256).map(|i|json!({"id":i,"reason":"x".repeat(160)})).collect::<Vec<_>>()}}});
        let mut details = crate::daily_digest::Details::default();
        let out =
            compact_observation_with_details(&mut details, digest_sample(), &report, false, 1)
                .unwrap();
        assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
        let saved = if out["history"]["compacted"] == true {
            details
                .get(
                    out["history"]["snapshot_id"].as_str().unwrap(),
                    "focus_readiness_historical",
                )
                .unwrap()["value"]
                .clone()
        } else {
            out["focus_readiness_historical"].clone()
        };
        assert_eq!(saved, history);
        assert_eq!(out["quarantined"], false);
        assert_eq!(out["input_authorized"], false);
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
    fn digest_keeps_durable_completion_when_batch_is_empty_and_rows_are_trimmed() {
        let id = format!("focus-{}", "x".repeat(110));
        let latest = json!({"entry_id":id,"state":"Complete","request_id":"a".repeat(64),"observed_sequence":6,"date_raw":60759371});
        let assignments = json!({"available":true,"source":"durable_plan","plan_revision":2,
            "historical":true,"current_state_verified":false,"input_authorized":false,
            "total":256,"completed":255,"pending":1,"in_flight":0,
            "completed_ids":[id,"b".repeat(128),"c".repeat(128),"d".repeat(128)],
            "completed_ids_truncated":true,"latest_completion":latest});
        let report = json!({"running":true,"last_error":null,"assignments":assignments,
            "daily_budget":{"day":706640,"used_assignment_attempts":1,"limit":1,"remaining":0},
            "last_batch":{"digest":{"date_raw":60759371,"plan_revision":2,"verified":[],"retained":[],"retained_count":0}},
            "focus":{"preparation":{"entry_id":id,"stage":"View","request_id":"b".repeat(64),"verified":true,"goal_completed":false,"historical":true},
                "start":{"entry_id":id,"stage":"Start","request_id":"a".repeat(64),"historical":true},
                "start_failure":{"entry_id":id,"request_id":"a".repeat(64),"original_code":"torn_read","original_reason":"x".repeat(160),"code":"torn_read","reason":"y".repeat(160),"evidence":"BeforeIntent","failures":2,"historical":true,"retry_authority":false}}});
        let out = compact_observation(digest_sample(), &report, false, 2).unwrap();
        assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
        assert!(
            out["assignments"]["completed_ids"]
                .as_array()
                .unwrap()
                .len()
                < 4
        );
        assert_eq!(out["assignments"]["latest_completion"], latest);
        assert_eq!(out["assignments"]["completed"], 255);
        assert_eq!(out["assignments"]["completed_ids_truncated"], true);
        assert_eq!(out["assignments"]["current_state_verified"], false);
        assert_eq!(out["focus"]["preparation_omitted_after_completion"], true);
        assert_eq!(
            out["focus"]["start"]["completion_ref"],
            "assignments.latest_completion"
        );
        assert_eq!(out["verified"], json!([]));
        assert_eq!(out["daily_budget"]["used_assignment_attempts"], 1);
        assert_eq!(out["input_authorized"], false);
    }
    #[test]
    fn completed_focus_plus_positive_active_wait_fits_digest_without_replay_authority() {
        let wait = json!({"entry_id":"focus-next","active_focus_id":"GER_remilitarize_the_rhineland",
            "date_raw":60759371,"observed_unix_ms":12345,"source":"native witnessed own focus",
            "historical":true,"navigation_attempted":false,"assignment_budget_consumed":false});
        let latest = json!({"entry_id":"focus-rhineland","state":"Complete","request_id":"a".repeat(64),"observed_sequence":6,"date_raw":60759371});
        let report = json!({"running":true,"last_error":null,"focus_permission":true,
            "assignments":{"available":true,"source":"durable_plan","plan_revision":2,"historical":true,
                "current_state_verified":false,"input_authorized":false,"total":2,"completed":1,"pending":1,"in_flight":0,
                "completed_ids":["focus-rhineland"],"completed_ids_truncated":false,"latest_completion":latest},
            "daily_budget":{"day":706640,"used_assignment_attempts":1,"limit":1,"remaining":0,"navigation_spends_assignment_budget":false},
            "last_batch":{"digest":{"date_raw":60759371,"plan_revision":1,"country":"GER","counts_scope":"observed own categories; null is unknown","review_due":false,"blocking_decision":null,"verified":["focus-rhineland"],"retained":[],"retained_count":0}},
            "focus":{"preparation":{"entry_id":"focus-rhineland","stage":"View","request_id":"b".repeat(64),"verified":true,"goal_completed":false,"historical":true},
                "start":{"entry_id":"focus-rhineland","stage":"Start","request_id":"a".repeat(64),"historical":true},
                "wait":wait,"refusal":null,"failure":null,"uncertainty":null,"start_failure":null},"focus_wait":wait});
        for restarted in [false, true] {
            let mut report = report.clone();
            if restarted {
                report["last_batch"] = Value::Null;
            }
            let out = compact_observation(digest_sample(), &report, false, 2).unwrap();
            assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
            assert_eq!(out["assignments"]["latest_completion"], latest);
            assert_eq!(out["assignments"]["completed"], 1);
            assert_eq!(out["assignments"]["pending"], 1);
            assert_eq!(out["latest_focus_wait_historical"], wait);
            assert!(out["focus"].get("wait").is_none());
            assert_eq!(out["daily_budget"]["used_assignment_attempts"], 1);
            assert_eq!(out["input_authorized"], false);
            assert_eq!(out["verified"], json!([]));
        }
    }
    #[test]
    fn completed_focus_does_not_hide_a_later_close_navigation_receipt() {
        let latest = json!({"entry_id":"focus-head","state":"Complete","request_id":"a".repeat(64),"observed_sequence":6,"date_raw":60759371});
        let preparation = json!({"entry_id":"focus-head","stage":"ClosePolitics","request_id":"b".repeat(64),"verified":true,"goal_completed":false,"historical":true});
        let report = json!({"running":true,
            "assignments":{"available":true,"completed":1,"latest_completion":latest},
            "focus":{"preparation":preparation,"start":{"entry_id":"focus-head","stage":"Start","request_id":"a".repeat(64),"historical":true}}});
        let out = compact_observation(digest_sample(), &report, false, 1).unwrap();
        assert_eq!(out["focus"]["preparation"], preparation);
        assert!(out["focus"]["preparation_omitted_after_completion"].is_null());
        assert_eq!(
            out["focus"]["start"]["completion_ref"],
            "assignments.latest_completion"
        );
        assert_eq!(out["assignments"]["completed"], 1);
    }
    #[test]
    fn recovered_start_failure_digest_preserves_cause_completion_budget_under_bound() {
        let request = "a".repeat(64);
        let latest = json!({"entry_id":"focus-rhineland","state":"Complete","request_id":request,"observed_sequence":7,"date_raw":60759371});
        let failure = json!({"entry_id":"focus-rhineland","request_id":request,"day":706640,
            "original_code":"torn_read","original_reason":"Focus witness changed","code":"torn_read","reason":"Focus witness changed",
            "evidence":"BeforeIntent","failures":1,"retry_planned_historical":true,"historical":true,"retry_authority":false});
        let report = json!({"running":true,"last_error":null,"focus_permission":true,
            "assignments":{"available":true,"source":"durable_plan","plan_revision":1,"historical":true,
                "current_state_verified":false,"input_authorized":false,"total":1,"completed":1,"pending":0,"in_flight":0,
                "completed_ids":["focus-rhineland"],"completed_ids_truncated":false,"latest_completion":latest},
            "daily_budget":{"day":706640,"used_assignment_attempts":1,"limit":1,"remaining":0,"navigation_spends_assignment_budget":false},
            "last_batch":{"digest":{"date_raw":60759371,"plan_revision":1,"country":"GER","counts_scope":"observed own categories; null is unknown","review_due":false,"blocking_decision":null,"verified":["focus-rhineland"],"retained":[],"retained_count":0}},
            "focus":{"preparation":{"entry_id":"focus-rhineland","stage":"View","request_id":"b".repeat(64),"verified":true,"goal_completed":false,"historical":true},
                "start":{"entry_id":"focus-rhineland","stage":"Start","request_id":request,"historical":true},
                "start_failure":failure,"failure":null,"refusal":null,"uncertainty":null,"wait":null}});
        let out = compact_observation(digest_sample(), &report, false, 1).unwrap();
        assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
        assert_eq!(out["assignments"]["latest_completion"], latest);
        let mut compact_failure = failure.clone();
        compact_failure.as_object_mut().unwrap().remove("entry_id");
        compact_failure
            .as_object_mut()
            .unwrap()
            .remove("request_id");
        compact_failure["completion_ref"] = json!("assignments.latest_completion");
        assert_eq!(out["focus"]["start_failure"], compact_failure);
        assert_eq!(
            out["focus"]["start"]["completion_ref"],
            "assignments.latest_completion"
        );
        assert_eq!(out["focus"]["preparation_omitted_after_completion"], true);
        assert_eq!(out["daily_budget"]["used_assignment_attempts"], 1);
        assert_eq!(out["input_authorized"], false);
        let mut mismatch = report;
        mismatch["assignments"]["latest_completion"]["request_id"] = json!("c".repeat(64));
        // A mismatched completion retains both identities in exact details.
        let mut details = crate::daily_digest::Details::default();
        let out =
            compact_observation_with_details(&mut details, digest_sample(), &mismatch, false, 1)
                .unwrap();
        let focus = if let Some(id) = out["history"]["snapshot_id"].as_str() {
            details.get(id, "focus").unwrap()["value"].clone()
        } else {
            out["focus"].clone()
        };
        assert_eq!(focus["start"]["request_id"], request);
        assert_eq!(focus["preparation"]["stage"], "View");
        assert_eq!(focus["start_failure"], failure);
    }
    #[test]
    fn pending_nonqueued_start_has_bounded_digest_and_exact_historical_details() {
        let failure = json!({"entry_id":"x".repeat(128),"request_id":"a".repeat(64),"day":706640,
            "original_code":"input_not_queued","original_reason":"Ordinary attempt was refused before possible input; plan remains unfinished",
            "code":"input_not_queued","reason":"Ordinary attempt was refused before possible input; plan remains unfinished",
            "evidence":"ConfirmedNotQueued","failures":1,"retry_planned_historical":false,"historical":true,"retry_authority":false});
        let report = json!({"running":true,"focus_permission":true,
            "assignments":{"available":true,"source":"durable_plan","plan_revision":1,"historical":true,
                "current_state_verified":false,"input_authorized":false,"total":1,"completed":0,"pending":1,"in_flight":0,
                "completed_ids":[],"completed_ids_truncated":false,"latest_completion":null},
            "daily_budget":{"day":706640,"used_assignment_attempts":1,"limit":1,"remaining":0,"navigation_spends_assignment_budget":false},
            "core_pending_historical":{"entry_id":"x".repeat(128),"attempts":1,"reason":"input_not_queued","reason_day":706640,"historical":true},
            "last_batch":{"digest":{"date_raw":60759371,"plan_revision":1,"country":"GER","counts_scope":"observed own categories; null is unknown","review_due":false,"blocking_decision":null,"verified":[],"retained":[],"retained_count":1}},
            "focus":{"preparation":{"entry_id":"x".repeat(128),"stage":"View","request_id":"b".repeat(64),"verified":true,"goal_completed":false,"historical":true},
                "start_failure":failure,"start":null,"failure":null,"refusal":null,"uncertainty":null,"wait":null}});
        let mut details = crate::daily_digest::Details::default();
        let out =
            compact_observation_with_details(&mut details, digest_sample(), &report, false, 1)
                .unwrap();
        assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
        assert_eq!(out["assignments"]["completed"], 0);
        assert_eq!(out["assignments"]["pending"], 1);
        assert_eq!(out["daily_budget"]["remaining"], 0);
        assert_eq!(out["input_authorized"], false);
        let id = out["history"]["snapshot_id"]
            .as_str()
            .expect("overflow history lookup");
        assert_eq!(
            details.get(id, "focus").unwrap()["value"]["start_failure"],
            failure
        );
        assert_eq!(
            details.get(id, "core_pending_historical").unwrap()["value"],
            report["core_pending_historical"]
        );
        assert_eq!(
            details.get(id, "assignments").unwrap()["value"]["latest_completion"],
            Value::Null
        );
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
    #[test]
    fn stage_original_failure_is_not_masked_by_later_probe_contention() {
        let original = json!({"entry_id":"research-support","stage":"SelectSlot","code":"research_precondition","reason":"actual original reason","known_no_input":true,"retry_count":0,"historical":true});
        let report = json!({"running":true,"last_error":{"code":"input_busy","reason":"later producer probe busy"},"last_stage_failure":original});
        let out = compact_observation(digest_sample(), &report, false, 2).unwrap();
        assert_eq!(out["last_error"]["code"], "input_busy");
        assert_eq!(
            out["last_stage_failure"]["reason"],
            "actual original reason"
        );
        assert_eq!(out["input_authorized"], false);
        assert!(serde_json::to_vec(&out).unwrap().len() <= 2048);
    }
    #[test]
    fn focus_plan_permission_is_default_off_omitted_and_plan_order_is_explicit() {
        let value = json!({"version":1,"revision":1,"entries":[{"id":"focus-a","intent":{"kind":"Focus","focus_id":"GER_rhineland"}}],"max_actions_per_day":1,"max_observation_age_ms":5000,"review_day":null});
        let spec: Spec = serde_json::from_value(value.clone()).unwrap();
        assert!(!spec.focus_permission);
        assert!(serde_json::to_value(&spec)
            .unwrap()
            .get("focus_permission")
            .is_none());
        let mut enabled = value;
        enabled["focus_permission"] = json!(true);
        let spec: Spec = serde_json::from_value(enabled).unwrap();
        assert!(spec.focus_permission);
        assert!(
            crate::daily_focus_navigation::capability(false, spec.focus_permission, true).is_err()
        );
        let f = spec.entries[0].clone();
        let r = core::Entry {
            id: "research-a".into(),
            intent: core::Intent::Research {
                technology_id: "support_weapons".into(),
                slot: 0,
            },
            focus_search: None,
            navigation: None,
        };
        assert!(focus_precedes(&[f.clone(), r.clone()], Some(&f), Some(&r)));
        assert!(!focus_precedes(&[r.clone(), f.clone()], Some(&f), Some(&r)));
        let controller = Controller::new(false, true, false, Weak::new(), Path::new("/generated"));
        assert!(controller.enabled());
        assert!(!controller.research_enabled);
        assert!(controller.focus_enabled());
    }
    #[test]
    fn production_spec_defaults_off_and_requires_an_explicit_bounded_budget() {
        let mut value = json!({"version":1,"revision":1,"entries":[],"max_actions_per_day":1,"max_observation_age_ms":5000,"review_day":null});
        let spec: Spec = serde_json::from_value(value.clone()).unwrap();
        assert!(!spec.production_permission);
        assert_eq!(spec.max_production_steps_per_day, None);
        assert_eq!(serde_json::to_value(&spec).unwrap(), value);
        value["production_permission"] = json!(true);
        let spec: Spec = serde_json::from_value(value.clone()).unwrap();
        assert!(production_plan::permission(
            true,
            spec.production_permission,
            true,
            spec.max_production_steps_per_day
        )
        .is_err());
        value["max_production_steps_per_day"] = json!(2);
        let spec: Spec = serde_json::from_value(value.clone()).unwrap();
        assert!(production_plan::permission(
            true,
            spec.production_permission,
            true,
            spec.max_production_steps_per_day
        )
        .is_ok());
        assert_eq!(serde_json::to_value(&spec).unwrap(), value);
        let controller = Controller::new(false, false, true, Weak::new(), Path::new("/generated"));
        assert!(controller.enabled() && controller.production_enabled());
        assert!(!controller.focus_enabled() && !controller.research_enabled());
    }
    #[test]
    fn production_progress_is_historical_in_digest_and_exact_details() {
        let production = json!({"permission":true,"budget":{"day":706640,"used_steps":2,"limit":2,"historical":true},"progress":{"blocked":true,"latest":{"entry_id":"allocation","failure":{"cause":{"code":"action_uncertain","message":"original failure"}}}}});
        let mut details = crate::daily_digest::Details::default();
        let out = compact_observation_with_details(
            &mut details,
            digest_sample(),
            &json!({"production":production,"running":false}),
            true,
            1,
        )
        .unwrap();
        assert_eq!(out["production"], production);
        assert_eq!(out["input_authorized"], false);
        assert_eq!(out["quarantined"], true);
        let full = json!({"production":production});
        let id = details.retain(&full).unwrap();
        let overflow = crate::daily_digest::overflow(&full, &id).unwrap();
        assert_eq!(overflow["production"]["budget"]["used_steps"], 2);
        assert_eq!(overflow["production"]["blocked"], true);
        assert_eq!(details.get(&id, "production").unwrap()["value"], production);
    }
    #[test]
    fn clock_probe_recovery_is_separate_from_action_failures_in_digest() {
        let mut history = crate::daily_worker::ClockProbeHistory::default();
        assert!(history.rejected(
            &Error::new("torn_read", "rejected scheduling witness"),
            true,
            crate::daily_worker::Disposition::Continue,
        ));
        history.verified(60759371);
        let original = json!({"code":"production_precondition","reason":"original action failure","historical":true});
        let mut details = crate::daily_digest::Details::default();
        let out = compact_observation_with_details(
            &mut details, digest_sample(),
            &json!({"clock_probe_historical":history.summary(),"last_error":original,"running":true}),
            false, 1,
        ).unwrap();
        assert_eq!(out["last_error"], original);
        assert_eq!(out["clock_probe_historical"]["recovered"], true);
        assert_eq!(
            out["clock_probe_historical"]["last_failure"]["reason"],
            "rejected scheduling witness"
        );
        assert_eq!(out["clock_probe_historical"]["input_authorized"], false);
    }
}
