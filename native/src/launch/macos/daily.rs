//! Thin Mac owner of the portable durable plan. The game clock is never changed
//! here. Pending work and possible input survive worker/client lifetimes.
mod focus;
mod production;
mod research;
use super::{action_fence::Fence, allowed, string, Config, Control, MacClock};
use crate::{
    daily_governor as core,
    daily_observation::{self, Producer},
    daily_worker::{self, Lifecycle},
    error::{Error, Result},
    platform::{macos_input::MacKeys, Process},
    protocol::worker_events::{Disposition, Events, Terminal},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Spec {
    version: u8,
    revision: u64,
    entries: Vec<core::Entry>,
    max_actions_per_day: u8,
    max_observation_age_ms: u64,
    review_day: Option<i64>,
    #[serde(default)]
    production_permission: bool,
}
impl Spec {
    fn plan(
        self,
        scope: core::Scope,
        research: bool,
        production: bool,
        focus: bool,
    ) -> Result<core::Plan> {
        for e in &self.entries {
            if e.navigation.is_some() || e.focus_search.is_some() {
                return Err(Error::new(
                    "invalid_plan",
                    "Mac navigation uses canonical live targets; hints are not supported",
                ));
            }
            match e.intent {
                core::Intent::Research { slot, .. } if research && slot <= 15 => (),
                core::Intent::Focus { ref focus_id } if focus => crate::own_focus::validate_key(focus_id)?,
                core::Intent::Economy {
                    target: core::EconomyTarget::Production { .. },
                } if production && self.production_permission => (),
                _ => return Err(Error::new(
                    "capability_unavailable",
                    "Only explicitly enabled Mac Research, Focus and single factory steps are supported",
                )),
            }
        }
        let plan = core::Plan {
            version: self.version,
            revision: self.revision,
            scope,
            entries: self.entries,
            max_actions_per_day: self.max_actions_per_day,
            max_observation_age_ms: self.max_observation_age_ms,
            review_day: self.review_day,
        };
        plan.validate()?;
        Ok(plan)
    }
}
struct Retained {
    scope: core::Scope,
    governor: Option<core::Governor>,
    production: Option<production::Journals>,
}
impl Retained {
    fn check(&mut self) -> Result<()> {
        if let Some(g) = &mut self.governor {
            g.preparation_check()?;
            if self.production.is_none() {
                return Err(Error::new(
                    "action_uncertain",
                    "Retained plan journals failed to bind",
                ));
            }
        }
        if let Some(p) = &mut self.production {
            p.check(&self.scope.epoch.game)?;
        }
        Ok(())
    }
}
struct Active {
    life: Lifecycle,
    producer: Arc<Producer>,
    report: Arc<Mutex<Value>>,
}
pub(super) struct Controller {
    directory: PathBuf,
    research: bool,
    production: bool,
    focus: bool,
    retained: Arc<Mutex<Retained>>,
    active: Mutex<Option<Active>>,
    blocked: Arc<AtomicBool>,
    events: Arc<Events>,
}
fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    m.try_lock().map_err(|e| match e {
        std::sync::TryLockError::WouldBlock => {
            Error::new("input_busy", "Mac daily owner is in use")
        }
        std::sync::TryLockError::Poisoned(_) => {
            Error::new("action_uncertain", "Mac daily owner is poisoned")
        }
    })
}
fn unsupported_artifacts(directory: &Path, core_present: bool) -> Result<()> {
    for name in [
        "daily-plan.json",
        "daily-sequence.json",
        "daily-production.json",
        "production-actions.jsonl",
        "daily-navigation.json",
        "daily-focus-navigation.json",
        "daily-panel-navigation.json",
    ] {
        let metadata = match directory.join(name).symlink_metadata() {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if !metadata.is_file()
            || (!core_present && metadata.len() != 0)
            || (name.ends_with("navigation.json") && metadata.len() != 0)
        {
            return Err(Error::new(
                "action_uncertain",
                "Unmatched or unsupported daily history must be retained; cannot bootstrap",
            ));
        }
    }
    Ok(())
}
impl Controller {
    pub(super) fn bind(
        directory: &Path,
        config: &Config,
        scope: core::Scope,
        date_raw: i32,
    ) -> Result<Self> {
        let mut governor = core::Governor::load_existing(&directory.join("daily-plan.json"))?;
        unsupported_artifacts(directory, governor.is_some())?;
        let production = if let Some(g) = &mut governor {
            if g.plan().scope != scope {
                return Err(Error::new(
                    "ownership_changed",
                    "Daily plan belongs to another owned campaign",
                ));
            }
            // Do not make quarantine conditional on today's permission flags.
            g.verify_live_scope(&scope, date_raw)?;
            Some(production::Journals::open(
                directory,
                &scope,
                &g.plan().entries,
            )?)
        } else {
            None
        };
        let mut retained = Retained {
            scope,
            governor,
            production,
        };
        retained.check()?;
        Ok(Self {
            directory: directory.into(),
            research: config.daily_research,
            production: config.daily_production,
            focus: config.daily_focus,
            retained: Arc::new(Mutex::new(retained)),
            active: Mutex::new(None),
            blocked: Arc::new(AtomicBool::new(false)),
            events: Arc::new(Events::default()),
        })
    }
    pub(super) fn input_check(&self) -> Result<()> {
        if self.blocked.load(Ordering::Acquire) {
            return Err(Error::new(
                "action_uncertain",
                "Mac daily work is quarantined",
            ));
        }
        let result = (|| {
            let mut retained = lock(&self.retained)?;
            if retained.governor.is_none() {
                unsupported_artifacts(&self.directory, false)?;
            }
            retained.check()
        })();
        if result.as_ref().is_err_and(|e| e.code != "input_busy") {
            self.blocked.store(true, Ordering::Release);
        }
        result
    }
    pub(super) fn events(&self) -> Arc<Events> {
        self.events.clone()
    }
    pub(super) fn capabilities(&self) -> Value {
        json!({"research_enabled":self.research,"production_enabled":self.production,"focus_enabled":self.focus,
            "durable":true,"observation_pauses":0,"production_steps_per_day":1,
            "full_campaign_acceptance":false})
    }
    pub(super) fn tool() -> Value {
        json!({"name":"hoi4_daily_governor","description":"Owned Mac background plan preview. Research queues refill the same slot after natural completion; production supports one exact existing-line factory step per entry. Focus queues wait for fresh completed membership before the next Start. Default-off owner daily_research/daily_production/daily_focus; production also needs plan.production_permission. Start/update require an existing lease and explicit plan. Update requires a stopped worker. Digest reads fresh own state; stop joins without pausing the game. Uncertain input survives reconnect and never replays. Long campaigns remain unverified.","inputSchema":{"type":"object","oneOf":[{"properties":{"op":{"enum":["start","update"]},"lease_token":{"type":"string"},"plan":{"type":"object"}},"required":["op","lease_token","plan"],"additionalProperties":false},{"properties":{"op":{"enum":["digest","stop"]}},"required":["op"],"additionalProperties":false}]}})
    }
    pub(super) fn call(
        &self,
        owner: &MacClock,
        args: &serde_json::Map<String, Value>,
    ) -> Result<Value> {
        match string(args, "op")? {
            "start" | "update" => {
                allowed(args, &["op", "lease_token", "plan"])?;
                let spec: Spec =
                    serde_json::from_value(args.get("plan").cloned().ok_or_else(|| {
                        Error::new("invalid_arguments", "Explicit plan required")
                    })?)
                    .map_err(|e| Error::new("invalid_arguments", e.to_string()))?;
                self.start(
                    owner,
                    string(args, "lease_token")?.to_owned(),
                    spec,
                    string(args, "op")? == "update",
                )
            }
            "stop" => {
                allowed(args, &["op"])?;
                self.stop()?;
                self.digest(owner, false)
            }
            "digest" => {
                allowed(args, &["op"])?;
                self.digest(owner, true)
            }
            _ => Err(Error::new("invalid_arguments", "Unknown daily operation")),
        }
    }
    fn stop(&self) -> Result<()> {
        // Workers never lock active. Join holds neither retained nor Control.
        let mut active = lock(&self.active)?;
        if let Some(a) = active.as_mut() {
            if let Err(e) = a.life.stop() {
                self.blocked.store(true, Ordering::Release);
                return Err(e);
            }
        }
        Ok(())
    }
    fn start(&self, owner: &MacClock, token: String, spec: Spec, update: bool) -> Result<Value> {
        self.input_check()?;
        let mut active = lock(&self.active)?;
        if active.as_ref().is_some_and(|a| a.life.running()) {
            return Err(Error::new(
                "input_busy",
                "Stop the existing worker before start/update",
            ));
        }
        if let Some(a) = active.as_mut() {
            a.life.stop()?;
        }
        let mut retained = lock(&self.retained)?;
        retained.check()?;
        let plan = spec.plan(
            retained.scope.clone(),
            self.research,
            self.production,
            self.focus,
        )?;
        let mut state = lock(&owner.state)?;
        if foreign(&state) {
            return Err(Error::new(
                "action_uncertain",
                "Shared input journals are unresolved",
            ));
        }
        let until = (Instant::now() + Duration::from_secs(5))
            .min(state.authority.authorize(&token, &retained.scope.epoch)?);
        let expected = state.guard.observe_context(owner.process.as_ref())?;
        if expected.anchors != retained.scope.anchors {
            return Err(Error::new("ownership_changed", "Daily roots changed"));
        }
        if let Some(g) = &mut retained.governor {
            if update {
                g.update_plan(plan.clone())?;
            } else if g.plan() != plan {
                return Err(Error::new(
                    "invalid_plan",
                    "Start must match retained plan; use stopped update",
                ));
            }
        } else {
            if update {
                return Err(Error::new("invalid_plan", "No retained plan to update"));
            }
            unsupported_artifacts(&self.directory, false)?;
            retained.governor = Some(core::Governor::open(
                &self.directory.join("daily-plan.json"),
                plan.clone(),
            )?);
        }
        if retained.production.is_none() {
            retained.production = Some(production::Journals::open(
                &self.directory,
                &plan.scope,
                &plan.entries,
            )?);
        }
        if retained.governor.as_ref().unwrap().last_sequence() > 0
            && !self.directory.join("daily-sequence.json").exists()
        {
            return Err(Error::new(
                "sequence_changed",
                "Persisted daily sequence is missing",
            ));
        }
        // Reuse the single writer across stop/start within this owner lifetime.
        let producer = match active.as_ref() {
            Some(a) => a.producer.clone(),
            None => Arc::new(Producer::open(
                owner.process.as_ref(),
                plan.scope.epoch.clone(),
                expected,
                &self.directory.join("daily-sequence.json"),
                until,
            )?),
        };
        let first = producer.observe(owner.process.as_ref(), until)?;
        if first.observation.scope != plan.scope
            || first.observation.sequence <= retained.governor.as_ref().unwrap().last_sequence()
        {
            return Err(Error::new(
                "sequence_changed",
                "Fresh producer must exceed retained plan history",
            ));
        }
        drop(state);
        drop(retained);
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(core::Wakeup::default());
        let report = Arc::new(Mutex::new(json!({"running":true,"observation_pauses":0})));
        let instance = self.events.start(plan.revision)?;
        let worker = Worker {
            process: owner.process.clone(),
            keys: owner.keys.clone(),
            control: owner.state.clone(),
            retained: self.retained.clone(),
            producer: producer.clone(),
            stop: stop.clone(),
            wake: wake.clone(),
            blocked: self.blocked.clone(),
            report: report.clone(),
            events: self.events.clone(),
            token,
            revision: plan.revision,
            instance,
        };
        let handle = match thread::Builder::new()
            .name("hoi4-mac-daily".into())
            .spawn(move || worker.run())
        {
            Ok(handle) => handle,
            Err(error) => {
                self.events.finish(
                    instance,
                    plan.revision,
                    Terminal::new(
                        Disposition::Stopped,
                        "worker_start_failed",
                        "Worker thread was not started; no input",
                    ),
                )?;
                return Err(error.into());
            }
        };
        *active = Some(Active {
            life: Lifecycle::new(stop, wake, handle),
            producer,
            report,
        });
        Ok(
            json!({"running":true,"plan_revision":plan.revision,"worker_instance":instance,"observation_pauses":0}),
        )
    }
    fn digest(&self, owner: &MacClock, fresh: bool) -> Result<Value> {
        let active = lock(&self.active)?;
        crate::daily_digest_lock::governed(&self.retained, |retained| {
            let (assignments, budget) = match &mut retained.governor {
                Some(g) => {
                    let (history, budget) = g.stopped_history()?;
                    (
                        if fresh {
                            g.assignment_summary()?
                        } else {
                            history
                        },
                        budget,
                    )
                }
                None => (json!([]), Value::Null),
            };
            let observed = if fresh {
                match active.as_ref() {
                    Some(a) => Some(
                        a.producer
                            .observe_digest(
                                owner.process.as_ref(),
                                Instant::now() + Duration::from_secs(5),
                            )?
                            .observation,
                    ),
                    None => None,
                }
            } else {
                None
            };
            let history = active
                .as_ref()
                .map(|a| lock(&a.report).map(|v| v.clone()))
                .transpose()?;
            Ok(
                json!({"running":active.as_ref().is_some_and(|a|a.life.running()),"quarantined":self.blocked.load(Ordering::Acquire),"observation":observed,"assignments":assignments,"budget":budget,"worker_history":history,"historical":!fresh,"input_authorized":false,"observation_pauses":0}),
            )
        })
    }
}
impl Drop for Controller {
    fn drop(&mut self) {
        let _ = self.events.retire();
        let _ = self.stop();
    }
}
fn foreign(control: &Control) -> bool {
    control.journal.blocks_game(&control.guard.epoch.game) || control.actions.blocked()
}
fn retain_terminal(mut terminal: Terminal, blocked: &AtomicBool, retained: Result<()>) -> Terminal {
    if retained.is_err() {
        blocked.store(true, Ordering::Release);
    }
    if blocked.load(Ordering::Acquire) {
        if terminal.disposition == Disposition::RequestedStop || terminal.code == "plan_complete" {
            let reason = retained
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Retained worker uncertainty".into());
            terminal = Terminal::new(Disposition::Quarantined, "action_uncertain", &reason);
        } else {
            terminal.disposition = Disposition::Quarantined;
        }
    }
    terminal
}
struct Worker {
    process: Arc<dyn Process + Send + Sync>,
    keys: Arc<MacKeys<'static>>,
    control: Arc<Mutex<Control>>,
    retained: Arc<Mutex<Retained>>,
    producer: Arc<Producer>,
    stop: Arc<AtomicBool>,
    wake: Arc<core::Wakeup>,
    blocked: Arc<AtomicBool>,
    report: Arc<Mutex<Value>>,
    events: Arc<Events>,
    token: String,
    revision: u64,
    instance: u64,
}
impl Worker {
    fn run(self) {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.body()));
        let terminal = match result {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => {
                let d = daily_worker::disposition(Some(&e), self.blocked.load(Ordering::Acquire));
                if d == daily_worker::Disposition::Quarantine {
                    self.blocked.store(true, Ordering::Release);
                }
                daily_worker::terminal_event(d, Some(&e))
                    .unwrap_or_else(|| Terminal::new(Disposition::Stopped, e.code, &e.message))
            }
            Err(_) => {
                self.blocked.store(true, Ordering::Release);
                Terminal::new(
                    Disposition::Quarantined,
                    "daily_quarantined",
                    "Mac worker panic; retained state requires review",
                )
            }
        };
        self.stop.store(true, Ordering::Release);
        let _ = daily_worker::publish_terminal(&self.events, self.instance, || {
            // No Control lock is held. Publication cannot race past retained
            // journal/progress poison, including a stop after a transient error.
            let retained = self
                .retained
                .lock()
                .map_err(|_| Error::new("action_uncertain", "Retained worker state poisoned"))
                .and_then(|mut retained| retained.check());
            let terminal = retain_terminal(terminal, &self.blocked, retained);
            if let Ok(mut report) = self.report.lock() {
                report["running"] = json!(false);
                report["terminal"] = json!(terminal);
            }
            (self.revision, terminal)
        });
    }
    fn body(&self) -> Result<Terminal> {
        let mut last_day = None;
        let mut generation = 0;
        loop {
            if self.stop.load(Ordering::Acquire) {
                return Ok(Terminal::new(
                    Disposition::RequestedStop,
                    "worker_stopped",
                    "Stop requested; game clock unchanged",
                ));
            }
            match self.cycle(&mut last_day) {
                Ok(Some(terminal)) => return Ok(terminal),
                Ok(None) => (),
                Err(e)
                    if !self.blocked.load(Ordering::Acquire)
                        && matches!(e.code, "input_busy" | "torn_read") => {}
                Err(e) => return Err(e),
            }
            generation = match self.wake.wait_after(generation, Duration::from_secs(1)) {
                Ok(notification) => notification.generation,
                Err(error) => {
                    return Ok(daily_worker::scheduling_failure(
                        &error,
                        &self.blocked,
                        &self.report,
                    ))
                }
            };
        }
    }
    fn cycle(&self, last_day: &mut Option<i64>) -> Result<Option<Terminal>> {
        let mut retained = lock(&self.retained)?;
        retained.check()?;
        let mut control = lock(&self.control)?;
        let until = (Instant::now() + Duration::from_secs(5)).min(
            control
                .authority
                .authorize(&self.token, &retained.scope.epoch)?,
        );
        if foreign(&control) {
            return Err(Error::new("action_uncertain", "Shared input is unresolved"));
        }
        let date = self.producer.probe_clock(self.process.as_ref(), until)?;
        let day = (i64::from(date) - 43_800_000).div_euclid(24);
        let g = retained
            .governor
            .as_mut()
            .ok_or_else(|| Error::new("invalid_plan", "Retained worker plan missing"))?;
        let plan = g.plan();
        if plan.review_day.is_some_and(|d| day >= d) {
            return Ok(Some(Terminal::new(
                Disposition::Stopped,
                "review_deadline",
                "Plan reached its review day",
            )));
        }
        if plan.entries.iter().all(|e| {
            matches!(
                g.entry_state(&e.id),
                Some(core::EntryState::Complete { .. })
            )
        }) {
            return Ok(Some(Terminal::new(
                Disposition::Stopped,
                "plan_complete",
                "All explicit assignments verified",
            )));
        }
        let previous_focus = g
            .completed_focus_hint()?
            .and_then(|entry| match entry.intent {
                core::Intent::Focus { focus_id } => Some(focus_id),
                _ => None,
            });
        let mut continuation = None;
        if *last_day == Some(day) {
            if !g.budget_available(day) {
                return Ok(None);
            }
            let pending = g.pending_research(day);
            if !pending.is_empty() {
                let probe = self.producer.probe_research(self.process.as_ref(), until)?;
                for entry in &pending {
                    if !probe.all_heads_occupied(std::slice::from_ref(entry))? {
                        continuation = Some(entry.id.clone());
                        break;
                    }
                }
            }
            if continuation.is_none() {
                continuation = g.pending_production(day).first().map(|e| e.id.clone());
            }
            if continuation.is_none() {
                if let Some(entry) = g.pending_focus(day) {
                    let core::Intent::Focus { focus_id } = &entry.intent else {
                        unreachable!()
                    };
                    let probe =
                        self.producer
                            .probe_focus(self.process.as_ref(), focus_id, until)?;
                    if !matches!(
                        focus::readiness_at(
                            &probe,
                            &entry,
                            &g.plan().scope,
                            date,
                            previous_focus.as_deref()
                        )?,
                        core::Readiness::WaitingForFocus
                    ) {
                        continuation = Some(entry.id);
                    }
                }
            }
            if continuation.is_none() {
                return Ok(None);
            }
        }
        let Retained {
            governor,
            production,
            ..
        } = &mut *retained;
        let governor = governor.as_mut().unwrap();
        let mut backend = DailyBackend {
            process: self.process.as_ref(),
            keys: &self.keys,
            control: &mut control,
            producer: &self.producer,
            production: production.as_mut().unwrap(),
            token: &self.token,
            stop: &self.stop,
            until,
            last_context: None,
            previous_focus,
        };
        let result = match continuation {
            Some(id) => {
                governor.continue_entry(&mut backend, &id, daily_observation::unix_ms()?, until)
            }
            None => governor.run_day(&mut backend, daily_observation::unix_ms()?, until),
        };
        if governor.blocks_work() {
            self.blocked.store(true, Ordering::Release);
        }
        let result = result?;
        *last_day = Some(day);
        if let Ok(mut report) = self.report.lock() {
            *report = json!({"running":true,"last_batch":result,"historical":true,"observation_pauses":0});
        }
        if let Some(reason) = result.blocking_decision {
            return Ok(Some(Terminal::new(
                Disposition::Stopped,
                "blocking_decision",
                &reason,
            )));
        }
        Ok(None)
    }
}
struct DailyBackend<'a> {
    process: &'a (dyn Process + Send + Sync),
    keys: &'a MacKeys<'a>,
    control: &'a mut Control,
    producer: &'a Producer,
    production: &'a mut production::Journals,
    token: &'a str,
    stop: &'a AtomicBool,
    until: Instant,
    last_context: Option<(u64, crate::game::ClockContext)>,
    previous_focus: Option<String>,
}
impl core::Backend for DailyBackend<'_> {
    fn unresolved_input(&mut self) -> Result<bool> {
        Ok(foreign(self.control) || self.production.unresolved(&self.control.guard.epoch.game)?)
    }
    fn observe(&mut self) -> Result<core::Observation> {
        self.control
            .authority
            .authorize(self.token, &self.control.guard.epoch)?;
        if self.stop.load(Ordering::Acquire) {
            return Err(Error::new("session_closed", "Worker stop requested"));
        }
        let p = self.producer.observe(self.process, self.until)?;
        self.control
            .guard
            .persisted
            .observe(&self.control.guard.epoch, &p.context.campaign)?;
        self.control.guard.latest_date = p.context.campaign.date_raw;
        self.last_context = Some((p.observation.sequence, p.context.campaign));
        Ok(p.observation)
    }
    fn readiness(
        &mut self,
        entry: &core::Entry,
        observed: &core::Observation,
    ) -> Result<core::Readiness> {
        if matches!(entry.intent, core::Intent::Research { .. }) {
            research::readiness(&research::observe(self.process)?, entry, observed)
        } else if let core::Intent::Focus { focus_id } = &entry.intent {
            focus::readiness(
                &self
                    .producer
                    .probe_focus(self.process, focus_id, self.until)?,
                entry,
                observed,
                self.previous_focus.as_deref(),
            )
        } else {
            production::readiness(self.process, entry, observed)
        }
    }
    fn attempt(
        &mut self,
        attempt: &core::Attempt,
        before: &core::Observation,
        deadline: Instant,
    ) -> Result<core::Outcome> {
        // The core has persisted Intent before entering this method.
        let expected = self
            .last_context
            .as_ref()
            .filter(|(sequence, _)| *sequence == before.sequence)
            .map(|(_, context)| context.clone())
            .ok_or_else(|| {
                Error::new(
                    "stale_observation",
                    "Attempt has no matching producer context",
                )
            })?;
        let epoch = self.control.guard.epoch.clone();
        let until = deadline.min(self.control.authority.authorize(self.token, &epoch)?);
        let fence = Fence {
            process: self.process,
            epoch: &epoch,
            expected: &expected,
            authority: &self.control.authority,
            token: self.token,
            until,
            stop: Some(self.stop),
        };
        let check = || fence.input_check();
        let keys = self.keys.guarded(&fence, &check)?;
        let outcome = if let core::Intent::Research {
            technology_id,
            slot,
        } = &attempt.entry.intent
        {
            let prior = research::observe(&fence)?;
            if !matches!(
                research::readiness(&prior, &attempt.entry, before)?,
                core::Readiness::Ready
            ) {
                return Ok(core::Outcome::NotQueued(
                    "Research precondition changed before preparation".into(),
                ));
            }
            keys.confirm_input_idle()?;
            // From the first UI preparation onward every failure is uncertain.
            super::research::start(&fence, &keys, technology_id, *slot as usize)?;
            let after = research::observe(&fence)?;
            let observed = self.producer.observe(&fence, until)?.observation;
            check()?;
            Ok(core::Outcome::Verified(Box::new(research::complete(
                attempt, prior, after, observed,
            )?)))
        } else if let core::Intent::Focus { focus_id } = &attempt.entry.intent {
            let prior = self.producer.probe_focus(&fence, focus_id, until)?;
            if !matches!(
                focus::readiness(
                    &prior,
                    &attempt.entry,
                    before,
                    self.previous_focus.as_deref()
                )?,
                core::Readiness::Ready
            ) {
                return Ok(core::Outcome::NotQueued(
                    "Focus precondition changed before preparation".into(),
                ));
            }
            keys.confirm_input_idle()?;
            // The ordinary path must retain this exact typed pre-input target.
            // Every failure after preparation preserves the core Intent.
            super::focus::start_bound(&fence, &keys, focus_id, until, &prior)?;
            let after = self.producer.probe_focus(&fence, focus_id, until)?;
            let observed = self.producer.observe(&fence, until)?.observation;
            check()?;
            Ok(core::Outcome::Verified(Box::new(focus::complete(
                attempt, before, prior, after, observed,
            )?)))
        } else {
            self.production.attempt(
                &fence,
                &keys,
                &self.control.authority,
                self.token,
                attempt,
                self.producer,
                until,
            )
        }?;
        if matches!(outcome, core::Outcome::Verified(_)) {
            self.control.guard.observe_context(&fence)?;
            check()?;
            if let core::Intent::Focus { focus_id } = &attempt.entry.intent {
                // Later entries in this same batch must see the new assignment,
                // not the historical key captured before run_day began.
                self.previous_focus = Some(focus_id.clone());
            }
        }
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actions::Epoch, platform::Identity};
    use std::sync::atomic::AtomicUsize;
    fn scope() -> core::Scope {
        core::Scope {
            epoch: Epoch {
                game: Identity {
                    pid: 7,
                    start_ticks: "1.000001".into(),
                    exe: "/fixture".into(),
                    uid: 501,
                },
                campaign: "nonce:fixture".into(),
                player: "GER".into(),
            },
            anchors: [0x10000, 0x20000, 0x30000],
            own_country: 0x40000,
            own_status: 0x50000,
        }
    }
    fn spec() -> Spec {
        serde_json::from_value(json!({"version":1,"revision":1,"entries":[{"id":"one","intent":{"kind":"Research","technology_id":"tech","slot":0}}],"max_actions_per_day":2,"max_observation_age_ms":5000,"review_day":null})).unwrap()
    }
    pub(super) fn dir() -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "hoi4-mac-daily-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir(&p).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
        p
    }
    fn config() -> Config {
        Config {
            controller: "fixture".into(),
            player: "GER".into(),
            daily_research: false,
            daily_production: false,
            daily_focus: false,
        }
    }
    #[test]
    fn supported_domains_require_explicit_permissions_and_hints_never_silently_drop() {
        assert_eq!(
            spec().plan(scope(), false, false, false).unwrap_err().code,
            "capability_unavailable"
        );
        spec().plan(scope(), true, false, false).unwrap();
        let mut s = spec();
        s.entries[0].navigation = Some(core::ResearchNavigation {
            folder_id: "industry".into(),
        });
        assert!(s.plan(scope(), true, false, false).is_err());
        for granted in [false, true] {
            let mut s = spec();
            s.entries[0].intent = core::Intent::Focus {
                focus_id: "focus".into(),
            };
            assert!(s.plan(scope(), true, granted, false).is_err());
        }
        let mut s = spec();
        s.entries[0].intent = core::Intent::Focus {
            focus_id: "focus".into(),
        };
        s.plan(scope(), false, false, true).unwrap();
        let mut s = spec();
        s.entries[0].intent = core::Intent::Focus {
            focus_id: "GER-bad".into(),
        };
        assert!(s.plan(scope(), false, false, true).is_err());
        let c: Config =
            serde_json::from_value(json!({"controller":"fixture","player":"GER"})).unwrap();
        assert!(!c.daily_focus && !c.daily_research && !c.daily_production);
    }
    #[test]
    fn missing_plan_never_bootstraps_over_any_nonempty_daily_artifact() {
        for name in [
            "daily-sequence.json",
            "daily-production.json",
            "production-actions.jsonl",
            "daily-navigation.json",
        ] {
            let p = dir();
            std::fs::write(p.join(name), b"retained evidence").unwrap();
            assert!(Controller::bind(&p, &config(), scope(), 60759624).is_err());
            assert_eq!(std::fs::read(p.join(name)).unwrap(), b"retained evidence");
            assert!(!p.join("daily-plan.json").exists());
            std::fs::remove_dir_all(p).unwrap();
        }
    }
    #[test]
    fn focus_capability_reports_explicit_permission() {
        for enabled in [false, true] {
            let p = dir();
            let mut config = config();
            config.daily_focus = enabled;
            let controller = Controller::bind(&p, &config, scope(), 60759624).unwrap();
            assert_eq!(controller.capabilities()["focus_enabled"], enabled);
            drop(controller);
            std::fs::remove_dir_all(p).unwrap();
        }
    }
    struct Uncertain {
        scope: core::Scope,
        n: u64,
        wait: bool,
    }
    impl core::Backend for Uncertain {
        fn unresolved_input(&mut self) -> Result<bool> {
            Ok(false)
        }
        fn observe(&mut self) -> Result<core::Observation> {
            self.n += 1;
            Ok(core::Observation {
                scope: self.scope.clone(),
                date_raw: 60759624,
                sequence: self.n,
                observed_unix_ms: 1,
                source: "generated".into(),
                counts: Default::default(),
            })
        }
        fn readiness(&mut self, _: &core::Entry, _: &core::Observation) -> Result<core::Readiness> {
            Ok(if self.wait {
                core::Readiness::WaitingForResearchSlot
            } else {
                core::Readiness::Ready
            })
        }
        fn attempt(
            &mut self,
            _: &core::Attempt,
            _: &core::Observation,
            _: Instant,
        ) -> Result<core::Outcome> {
            Ok(core::Outcome::Uncertain("Generated possible input".into()))
        }
    }
    #[test]
    fn persisted_uncertainty_blocks_binding_even_with_every_permission_disabled() {
        let p = dir();
        let plan = spec().plan(scope(), true, false, false).unwrap();
        let mut g = core::Governor::open(&p.join("daily-plan.json"), plan).unwrap();
        g.run_day(
            &mut Uncertain {
                scope: scope(),
                n: 0,
                wait: false,
            },
            1,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap_err();
        drop(g);
        let before = std::fs::read(p.join("daily-plan.json")).unwrap();
        assert_eq!(
            Controller::bind(&p, &config(), scope(), 60759624)
                .err()
                .unwrap()
                .code,
            "governor_quarantined"
        );
        assert_eq!(std::fs::read(p.join("daily-plan.json")).unwrap(), before);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn healthy_reopen_keeps_single_writer_and_replacement_blocks_manual_gate() {
        let p = dir();
        let plan = spec().plan(scope(), true, false, false).unwrap();
        drop(core::Governor::open(&p.join("daily-plan.json"), plan).unwrap());
        let controller = Controller::bind(&p, &config(), scope(), 60759624).unwrap();
        controller.input_check().unwrap();
        assert_eq!(
            Controller::bind(&p, &config(), scope(), 60759624)
                .err()
                .unwrap()
                .code,
            "plan_busy"
        );
        std::fs::write(p.join("daily-plan.json"), b"replacement").unwrap();
        assert!(controller.input_check().is_err());
        assert!(controller.blocked.load(Ordering::Acquire));
        drop(controller);
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn retained_scope_mismatch_cannot_be_rebased_by_disabling_permissions() {
        let p = dir();
        drop(
            core::Governor::open(
                &p.join("daily-plan.json"),
                spec().plan(scope(), true, false, false).unwrap(),
            )
            .unwrap(),
        );
        let mut changed = scope();
        changed.epoch.game.start_ticks = "1.000002".into();
        assert_eq!(
            Controller::bind(&p, &config(), changed, 60759624)
                .err()
                .unwrap()
                .code,
            "ownership_changed"
        );
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn stop_or_completion_never_hides_retained_production_poison() {
        for code in ["worker_stopped", "plan_complete", "torn_read"] {
            let blocked = AtomicBool::new(false);
            let result = retain_terminal(
                Terminal::new(Disposition::RequestedStop, code, "Generated stop"),
                &blocked,
                Err(Error::new(
                    "journal_changed",
                    "Original production journal was replaced",
                )),
            );
            assert_eq!(result.disposition, Disposition::Quarantined);
            assert!(result.reason.contains("journal_changed"));
            assert!(blocked.load(Ordering::Acquire));
        }
    }
    #[test]
    fn bind_refuses_rewound_plan_history_before_any_manual_input() {
        let p = dir();
        let mut governor = core::Governor::open(
            &p.join("daily-plan.json"),
            spec().plan(scope(), true, false, false).unwrap(),
        )
        .unwrap();
        governor
            .run_day(
                &mut Uncertain {
                    scope: scope(),
                    n: 0,
                    wait: true,
                },
                1,
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap();
        drop(governor);
        assert_eq!(
            Controller::bind(&p, &config(), scope(), 60759623)
                .err()
                .unwrap()
                .code,
            "ownership_changed"
        );
        std::fs::remove_dir_all(p).unwrap();
    }
}
