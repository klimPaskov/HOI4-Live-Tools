//! Durable preparation toward an explicit production goal. No input transport,
//! memory reader, pause, strategy choice or MCP registration lives here.
//! The owner must hold the shared controller/lease and use production_step.
use crate::{
    construction_stage::{Diagnostic, FailureEvidence},
    daily_governor::{Attempt, EconomyTarget, Entry, Intent, Scope},
    error::{Error, Result},
    game::ClockContext,
    production_step::{
        Execution, FactoryState, FactoryTarget, LineIdentity, PanelTarget, State, Target,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

mod model;
use model::*;
#[cfg(all(test, unix))]
mod tests;

#[derive(Debug, PartialEq)]
pub enum Next {
    Prepare(Target),
    /// Only a governor attempt may issue this final step or matching-count no-op.
    Finalize(FactoryTarget),
    BudgetExhausted,
    Deferred,
    /// Historical progress only; never substitute for a governor completion.
    Finished,
}

#[derive(Clone, Debug)]
pub struct Request {
    pub id: String,
    pub target: Target,
}
pub struct Progress {
    store: crate::daily_governor::store::Store,
    path: PathBuf,
    stamp: Option<ProgressStamp>,
    data: Data,
    poisoned: bool,
}
#[derive(PartialEq, Eq)]
struct ProgressStamp {
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    unix: (u64, u64, u64, u32, u32, i64, i64),
}
fn progress_stamp(path: &Path) -> Result<Option<ProgressStamp>> {
    let metadata = match path.symlink_metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        return Err(invalid("Production progress must retain a regular file"));
    }
    #[cfg(unix)]
    let unix = {
        use std::os::unix::fs::MetadataExt;
        (
            metadata.dev(),
            metadata.ino(),
            metadata.nlink(),
            metadata.mode(),
            metadata.uid(),
            metadata.ctime(),
            metadata.ctime_nsec(),
        )
    };
    Ok(Some(ProgressStamp {
        len: metadata.len(),
        modified: metadata.modified()?,
        #[cfg(unix)]
        unix,
    }))
}
fn invalid(message: &str) -> Error {
    Error::new("invalid_production_progress", message)
}
fn day(date: i32) -> Result<i64> {
    crate::calendar::decode(date)?;
    Ok((i64::from(date) - 43_800_000).div_euclid(24))
}
fn target(scope: &Scope, entry: &Entry) -> Result<FactoryTarget> {
    entry.validate()?;
    let Intent::Economy {
        target:
            EconomyTarget::Production {
                line_id,
                equipment_id,
                total_factories,
            },
    } = &entry.intent
    else {
        return Err(invalid("Explicit production allocation entry required"));
    };
    let t = FactoryTarget {
        epoch: scope.epoch.clone(),
        line_handle: line_id.clone(),
        equipment_id: equipment_id.clone(),
        factories: i32::from(*total_factories),
    };
    t.validate()?;
    Ok(t)
}
fn budget(limit: u8) -> Result<()> {
    if !(1..=16).contains(&limit) {
        return Err(invalid("Production step budget must be 1..16 per day"));
    }
    Ok(())
}
impl Progress {
    pub fn open(path: &Path, scope: Scope, entries: &[Entry]) -> Result<Self> {
        scope.validate()?;
        let (store, bytes) = crate::daily_governor::store::Store::open(path)?;
        let data = match bytes.as_ref() {
            Some(bytes) => serde_json::from_slice::<Data>(bytes)
                .map_err(|_| invalid("Malformed durable production progress"))?,
            None => Data {
                version: 1,
                scope: scope.clone(),
                bindings: vec![],
                steps: vec![],
            },
        };
        data.validate()?;
        if data.scope != scope || data.bindings.iter().any(|b| !entries.contains(&b.entry)) {
            return Err(invalid(
                "Original production scope/entries cannot be replaced on restart",
            ));
        }
        store.check()?;
        let stamp = progress_stamp(path)?;
        if stamp.is_some() != bytes.is_some() {
            return Err(invalid("Production progress changed while opening"));
        }
        Ok(Self {
            store,
            path: path.to_owned(),
            stamp,
            data,
            poisoned: false,
        })
    }
    pub fn blocked(&self) -> bool {
        self.poisoned || self.data.steps.iter().any(|s| s.status == Status::Intent)
    }
    /// Retained storage must be checked even when no production permission is
    /// enabled. A restored file never clears uncertainty in this owner.
    pub(crate) fn check_storage(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(Error::new(
                "production_progress_blocked",
                "Production progress persistence is uncertain",
            ));
        }
        let result = (|| {
            if progress_stamp(&self.path)? != self.stamp {
                return Err(Error::new(
                    "plan_changed",
                    "Retained production progress identity changed",
                ));
            }
            self.store.check()
        })();
        if let Err(error) = result {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
    fn check(&self) -> Result<()> {
        if self.blocked() {
            return Err(Error::new(
                "production_progress_blocked",
                "Unresolved production preparation must not be replayed or rebased",
            ));
        }
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        let result = (|| {
            self.check_storage()?;
            self.data.validate()?;
            self.store.replace(
                &serde_json::to_vec(&self.data)
                    .map_err(|_| invalid("Cannot encode production progress"))?,
            )?;
            self.stamp = Some(
                progress_stamp(&self.path)?
                    .ok_or_else(|| invalid("Saved production progress disappeared"))?,
            );
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    fn binding(&self, entry: &Entry) -> Result<&Binding> {
        self.data
            .bindings
            .iter()
            .find(|b| b.entry == *entry)
            .ok_or_else(|| invalid("Original production binding required"))
    }
    fn state(&self, entry: &Entry, fresh: &FactoryState, context: &ClockContext) -> Result<()> {
        let b = self.binding(entry)?;
        b.check(&self.data.scope, fresh, context)?;
        let expected = self
            .data
            .steps
            .iter()
            .rev()
            .find(|s| s.entry_id == entry.id && s.status == Status::Verified)
            .and_then(|s| s.after)
            .unwrap_or(b.initial.requested);
        if fresh.requested != expected {
            return Err(Error::new(
                "production_progress_changed",
                "Allocation changed outside the original verified progression",
            ));
        }
        if self
            .data
            .steps
            .last()
            .is_some_and(|s| context.date_raw < s.after_date.unwrap_or(s.date_raw))
        {
            return Err(invalid("Production preparation timeline regressed"));
        }
        Ok(())
    }
    /// Persist the exact original own line without assuming a panel mode or
    /// planning an input stage. Rebinding an existing entry is never allowed.
    pub fn bind_original(
        &mut self,
        entry: &Entry,
        fresh: &FactoryState,
        context: &ClockContext,
    ) -> Result<()> {
        self.check()?;
        self.check_storage()?;
        target(&self.data.scope, entry)?;
        if !self.data.bindings.iter().any(|b| b.entry.id == entry.id) {
            let binding = Binding {
                entry: entry.clone(),
                initial: fresh.clone(),
                context: context.clone(),
            };
            binding.check(&self.data.scope, fresh, context)?;
            self.data.bindings.push(binding);
            self.save()?;
        }
        self.state(entry, fresh, context)
    }
    /// Persist the original own line before planning any step. Reads supplied by
    /// the caller must be fresh, witnessed and accepted by its owned guard.
    pub fn next(
        &mut self,
        entry: &Entry,
        fresh: &FactoryState,
        context: &ClockContext,
        panel_open: bool,
        limit: u8,
    ) -> Result<Next> {
        self.check()?;
        budget(limit)?;
        let goal = target(&self.data.scope, entry)?;
        self.bind_original(entry, fresh, context)?;
        let day = day(context.date_raw)?;
        if self.data.steps.iter().any(|s| {
            s.entry_id == entry.id && s.completion.is_some() && s.status == Status::Verified
        }) {
            return Ok(Next::Finished);
        }
        if self
            .data
            .steps
            .iter()
            .any(|s| s.entry_id == entry.id && s.day == day && s.status == Status::NotQueued)
        {
            return Ok(Next::Deferred);
        }
        if self.used_steps(day) >= usize::from(limit) {
            return Ok(Next::BudgetExhausted);
        }
        let difference = goal.factories - fresh.requested;
        if difference != 0 && !panel_open {
            return Ok(Next::Prepare(Target::Panel(PanelTarget::new(
                self.data.scope.epoch.clone(),
            ))));
        }
        if difference.abs() <= 1 {
            return Ok(Next::Finalize(goal));
        }
        Ok(Next::Prepare(Target::Factories(FactoryTarget {
            factories: fresh.requested + difference.signum(),
            ..goal
        })))
    }
    pub fn used_steps(&self, day: i64) -> usize {
        self.data.steps.iter().filter(|s| s.day == day).count()
    }
    /// Scheduling hint only. Current own state must still pass `next`.
    pub fn refused_today(&self, entry: &Entry, day: i64) -> bool {
        self.data
            .steps
            .iter()
            .any(|s| s.entry_id == entry.id && s.day == day && s.status == Status::NotQueued)
    }
    pub fn original(&self, entry: &Entry) -> Result<(&LineIdentity, &ClockContext)> {
        let b = self.binding(entry)?;
        Ok((&b.initial.line, &b.context))
    }
    /// Persist before the owner invokes any ordinary adapter. Preparations can
    /// open the panel or move one count toward the goal, but never finish it.
    pub fn begin_preparation(
        &mut self,
        entry: &Entry,
        desired: Target,
        fresh: &FactoryState,
        context: &ClockContext,
        limit: u8,
    ) -> Result<Request> {
        self.begin(entry, desired, fresh, context, limit, None)
    }
    /// The final action uses the governor's already durable request identity.
    pub fn begin_completion(
        &mut self,
        attempt: &Attempt,
        fresh: &FactoryState,
        context: &ClockContext,
        limit: u8,
    ) -> Result<Request> {
        if attempt.scope != self.data.scope
            || attempt.before_sequence == 0
            || attempt.day != day(context.date_raw)?
        {
            return Err(invalid("Exact current governor attempt required"));
        }
        self.begin(
            &attempt.entry,
            Target::Factories(target(&self.data.scope, &attempt.entry)?),
            fresh,
            context,
            limit,
            Some(attempt.clone()),
        )
    }
    fn begin(
        &mut self,
        entry: &Entry,
        desired: Target,
        fresh: &FactoryState,
        context: &ClockContext,
        limit: u8,
        completion: Option<Attempt>,
    ) -> Result<Request> {
        self.check()?;
        budget(limit)?;
        self.state(entry, fresh, context)?;
        let today = day(context.date_raw)?;
        if self.used_steps(today) >= usize::from(limit) {
            return Err(Error::new(
                "production_progress_budget",
                "Daily production step budget exhausted",
            ));
        }
        if self.data.steps.iter().any(|s| {
            s.entry_id == entry.id
                && ((s.completion.is_some() && s.status == Status::Verified)
                    || (s.day == today && s.status == Status::NotQueued))
        }) {
            return Err(invalid(
                "Completed or refused production entry cannot issue another stage",
            ));
        }
        let mut s = Step {
            entry_id: entry.id.clone(),
            ordinal: self.data.steps.len() + 1,
            id: String::new(),
            target: desired,
            date_raw: context.date_raw,
            day: today,
            before: fresh.requested,
            after: None,
            after_date: None,
            status: Status::Intent,
            completion,
            failure: None,
        };
        s.id = match &s.completion {
            Some(a) => a.request_id.clone(),
            None => self.data.request_id(&s)?,
        };
        let request = Request {
            id: s.id.clone(),
            target: s.target.clone(),
        };
        self.data.steps.push(s);
        self.save()?;
        Ok(request)
    }
    /// Only the typed, fresh journal proof can settle a stage. The owner must
    /// provide a fresh canonical line read after the action, including opening.
    pub fn verified(
        &mut self,
        id: &str,
        execution: &Execution,
        fresh: &FactoryState,
        context: &ClockContext,
    ) -> Result<()> {
        if self.poisoned {
            return Err(invalid("Production progress writer failed"));
        }
        let index = self
            .data
            .steps
            .iter()
            .position(|s| s.id == id && s.status == Status::Intent)
            .ok_or_else(|| invalid("Original unresolved preparation required"))?;
        let s = &self.data.steps[index];
        let b = self
            .data
            .bindings
            .iter()
            .find(|b| b.entry.id == s.entry_id)
            .ok_or_else(|| invalid("Original binding missing"))?;
        b.check(&self.data.scope, fresh, context)?;
        if context.date_raw < s.date_raw {
            return Err(invalid("Production completion timeline regressed"));
        }
        let (rid, target, before, after) = execution.verified_step()?;
        if rid != id || *target != s.target {
            return Err(invalid("Receipt targets a different preparation"));
        }
        match (before, after) {
            (State::Factories(before), State::Factories(after))
                if before.line == b.initial.line
                    && before.requested == s.before
                    && after == fresh => {}
            (State::Panel(before), State::Panel(after))
                if before.panel_identity == after.panel_identity
                    && before.panel_identity.first() == Some(&b.initial.line.country)
                    && before.panel_identity.get(1) == Some(&b.initial.line.production)
                    && before.panel_identity.get(2) == Some(&self.data.scope.anchors[1])
                    && fresh.requested == s.before => {}
            _ => {
                return Err(invalid(
                    "Receipt lost original production identity or count",
                ))
            }
        }
        self.data.steps[index].status = Status::Verified;
        self.data.steps[index].after = Some(fresh.requested);
        self.data.steps[index].after_date = Some(context.date_raw);
        self.save()
    }
    /// `evidence` must come from the same journal immediately after execute.
    /// Unknown leaves the original intent blocking; no ID/token reset clears it.
    pub fn failed(
        &mut self,
        id: &str,
        cause: &Error,
        evidence: FailureEvidence,
        lease_token: &str,
    ) -> Result<()> {
        if self.poisoned {
            return Err(invalid("Production progress writer failed"));
        }
        let s = self
            .data
            .steps
            .iter_mut()
            .find(|s| s.id == id && s.status == Status::Intent)
            .ok_or_else(|| invalid("Original unresolved preparation required"))?;
        if let Some(prior) = &s.failure {
            if prior.evidence != evidence {
                return Err(invalid(
                    "Original uncertainty cannot be replaced by a no-input claim",
                ));
            }
        } else {
            s.failure = Some(Failure {
                cause: Diagnostic::from_error(cause, lease_token),
                evidence,
            });
        }
        if matches!(
            evidence,
            FailureEvidence::BeforeIntent | FailureEvidence::ConfirmedNotQueued
        ) {
            s.status = Status::NotQueued;
        }
        self.save()
    }
    pub fn summary(&self) -> Value {
        let latest = self.data.steps.last();
        json!({"historical":true,"blocked":self.blocked(),"steps":self.data.steps.len(),
            "bound_entries":self.data.bindings.len(),"latest":latest.map(|s| json!({
                "entry_id":s.entry_id,"request_id":s.id,"day":s.day,"before":s.before,"after":s.after,"after_date_raw":s.after_date,
                "status":s.status,"completion_stage":s.completion.is_some(),"failure":s.failure}))})
    }
}

#[cfg(all(test, unix))]
mod original_binding_tests {
    use super::*;
    use crate::{actions::Epoch, platform::Identity};
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "private-production-binding-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("daily-production.json")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn scope() -> Scope {
        Scope {
            epoch: Epoch {
                game: Identity {
                    pid: 71,
                    start_ticks: "123.000456".into(),
                    exe: "/generated/hoi4".into(),
                    uid: 501,
                },
                campaign: "original:fixture".into(),
                player: "GER".into(),
            },
            anchors: [0x10000, 0x20000, 0x30000],
            own_country: 0x40000,
            own_status: 0x50000,
        }
    }
    fn entry() -> Entry {
        Entry {
            id: "one-factory".into(),
            intent: Intent::Economy {
                target: EconomyTarget::Production {
                    line_id: "0x0000000100000038".into(),
                    equipment_id: "infantry_equipment_1".into(),
                    total_factories: 11,
                },
            },
            navigation: None,
            focus_search: None,
        }
    }
    fn current() -> FactoryState {
        FactoryState {
            epoch: scope().epoch,
            line: LineIdentity {
                country: scope().own_country,
                production: 0x60000,
                line: 0x70000,
                variant: 0x80000,
                base_type: 0x90000,
                handle: 0x0000000100000038,
                equipment_id: "infantry_equipment_1".into(),
            },
            requested: 10,
        }
    }
    fn context() -> ClockContext {
        ClockContext {
            playthrough_id: "fixture".into(),
            player: "GER".into(),
            anchors: scope().anchors,
            date_raw: crate::calendar::encode(1936, 1, 2).unwrap(),
            difficulty: 2,
            historical_raw: 1,
            paused: false,
            speed: 1,
        }
    }
    #[test]
    fn bind_only_is_durable_before_preparation_and_same_binding_never_rewrites() {
        let temp = Temp::new();
        let e = entry();
        let before = current();
        let context = context();
        let mut progress = Progress::open(&temp.path(), scope(), std::slice::from_ref(&e)).unwrap();
        progress.bind_original(&e, &before, &context).unwrap();
        assert!(progress.data.steps.is_empty());
        let bytes = fs::read(temp.path()).unwrap();
        let metadata = fs::metadata(temp.path()).unwrap();
        progress.bind_original(&e, &before, &context).unwrap();
        assert_eq!(fs::read(temp.path()).unwrap(), bytes);
        assert_eq!(fs::metadata(temp.path()).unwrap().ino(), metadata.ino());
        drop(progress);
        let mut reopened = Progress::open(&temp.path(), scope(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(reopened.original(&e).unwrap(), (&before.line, &context));
        let attempt = Attempt {
            request_id: "a".repeat(64),
            entry: e,
            scope: scope(),
            day: day(context.date_raw).unwrap(),
            before_sequence: 4,
        };
        let request = reopened
            .begin_completion(&attempt, &before, &context, 1)
            .unwrap();
        assert!(matches!(request.target, Target::Factories(_)));
        assert!(reopened.blocked());
        drop(reopened);
        assert!(
            Progress::open(&temp.path(), scope(), std::slice::from_ref(&attempt.entry))
                .unwrap()
                .blocked()
        );
    }
    #[test]
    fn restart_cannot_rebase_line_identity_count_or_chronology() {
        let temp = Temp::new();
        let e = entry();
        let before = current();
        let context = context();
        let mut progress = Progress::open(&temp.path(), scope(), std::slice::from_ref(&e)).unwrap();
        progress.bind_original(&e, &before, &context).unwrap();
        drop(progress);
        let bytes = fs::read(temp.path()).unwrap();
        let mut progress = Progress::open(&temp.path(), scope(), std::slice::from_ref(&e)).unwrap();
        for alter in [
            |s: &mut FactoryState| s.line.line += 8,
            |s: &mut FactoryState| s.line.variant += 8,
            |s: &mut FactoryState| s.line.base_type += 8,
            |s: &mut FactoryState| s.line.country += 8,
            |s: &mut FactoryState| s.line.production += 8,
            |s: &mut FactoryState| s.requested += 1,
        ] {
            let mut changed = before.clone();
            alter(&mut changed);
            assert!(progress.bind_original(&e, &changed, &context).is_err());
        }
        let mut earlier = context.clone();
        earlier.date_raw -= 24;
        assert!(progress.bind_original(&e, &before, &earlier).is_err());
        assert_eq!(fs::read(temp.path()).unwrap(), bytes);
        progress.bind_original(&e, &before, &context).unwrap();
    }
    #[test]
    fn identical_progress_replacement_is_refused_and_uncertainty_survives_restoration() {
        let temp = Temp::new();
        let e = entry();
        let mut progress = Progress::open(&temp.path(), scope(), std::slice::from_ref(&e)).unwrap();
        progress.bind_original(&e, &current(), &context()).unwrap();
        let bytes = fs::read(temp.path()).unwrap();
        let original = temp.0.join("original");
        fs::rename(temp.path(), &original).unwrap();
        fs::write(temp.path(), bytes).unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(progress.check_storage().unwrap_err().code, "plan_changed");
        fs::remove_file(temp.path()).unwrap();
        fs::rename(original, temp.path()).unwrap();
        assert!(progress.blocked());
        assert_eq!(
            progress.check_storage().unwrap_err().code,
            "production_progress_blocked"
        );
    }
    #[test]
    fn an_external_initial_file_cannot_become_a_new_binding() {
        let temp = Temp::new();
        let e = entry();
        let mut progress = Progress::open(&temp.path(), scope(), std::slice::from_ref(&e)).unwrap();
        fs::write(temp.path(), b"{}").unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            progress
                .bind_original(&e, &current(), &context())
                .unwrap_err()
                .code,
            "plan_changed"
        );
        assert!(progress.blocked());
        assert_eq!(fs::read(temp.path()).unwrap(), b"{}");
    }
}
