//! Owner-held production preparation and final governor attempt. All input uses
//! the existing ordinary adapter; no new transport or panel handoff is implied.
use super::*;
use crate::{
    construction_stage::FailureEvidence,
    production_progress::{Next, Progress},
    production_step::{FactoryState, FactoryTarget, PanelTarget, State as ProductionState, Target},
};

pub(super) struct ProductionPlan {
    pub progress: Progress,
    pub limit: Option<u8>,
}
pub(super) enum Run {
    Deferred,
    Advanced(Option<core::DailyResult>),
}
pub(super) fn permission(owner: bool, plan: bool, profile: bool, limit: Option<u8>) -> Result<()> {
    if plan && (!owner || !profile || !limit.is_some_and(|n| (1..=16).contains(&n))) {
        return Err(Error::new("invalid_plan", "Production requires owner daily_production, plan production_permission, owned profile and max_production_steps_per_day in 1..16"));
    }
    if !plan && limit.is_some() {
        return Err(Error::new(
            "invalid_plan",
            "Production step budget requires explicit production_permission",
        ));
    }
    Ok(())
}
pub(super) fn goal(entry: &core::Entry, scope: &core::Scope) -> Result<Target> {
    let core::Intent::Economy {
        target:
            core::EconomyTarget::Production {
                line_id,
                equipment_id,
                total_factories,
            },
    } = &entry.intent
    else {
        return Err(Error::new("invalid_plan", "Production allocation required"));
    };
    let target = Target::Factories(FactoryTarget {
        epoch: scope.epoch.clone(),
        line_handle: line_id.clone(),
        equipment_id: equipment_id.clone(),
        factories: i32::from(*total_factories),
    });
    target.validate()?;
    Ok(target)
}
pub(super) fn observed(
    process: &dyn Process,
    state: &mut ControlState,
    entry: &core::Entry,
    scope: &core::Scope,
    until: Instant,
) -> Result<(FactoryState, ClockContext)> {
    let (o, own) = production::observed(
        process,
        &mut state.guard,
        &goal(entry, scope)?,
        false,
        until,
    )?;
    let ProductionState::Factories(own) = own else {
        unreachable!()
    };
    if own.epoch != scope.epoch
        || own.line.country != scope.own_country
        || o.campaign.anchors != scope.anchors
        || o.campaign.player != scope.epoch.player
    {
        return Err(Error::new(
            "ownership_changed",
            "Production differs from the original daily own scope",
        ));
    }
    Ok((own, o.campaign))
}
fn next(
    plan: &mut ProductionPlan,
    process: &dyn Process,
    state: &mut ControlState,
    entry: &core::Entry,
    scope: &core::Scope,
    until: Instant,
) -> Result<(Next, FactoryState, ClockContext)> {
    let limit = plan
        .limit
        .ok_or_else(|| Error::new("capability_unavailable", "Daily production is disabled"))?;
    let (own, context) = observed(process, state, entry, scope, until)?;
    let mut shown = true; // A matching-count no-op requires no panel observation.
    if own.requested != goal(entry, scope)?.factory()?.factories {
        let target = Target::Panel(PanelTarget::new(scope.epoch.clone()));
        let (_, panel) =
            production::panel_observed(process, &mut state.guard, &target, false, until)?;
        let ProductionState::Panel(panel) = panel else {
            unreachable!()
        };
        if panel.panel_identity.first() != Some(&own.line.country)
            || panel.panel_identity.get(1) != Some(&own.line.production)
            || panel.panel_identity.get(2) != Some(&scope.anchors[1])
        {
            return Err(Error::new(
                "ownership_changed",
                "Panel and canonical production roots differ",
            ));
        }
        shown = panel.panel_shown && panel.managed_mode == 2;
    }
    let step = plan.progress.next(entry, &own, &context, shown, limit)?;
    Ok((step, own, context))
}
pub(super) fn failure(
    state: &ControlState,
    id: &str,
    target: &Target,
    error: &Error,
) -> FailureEvidence {
    if unresolved(state) {
        return FailureEvidence::Unknown;
    }
    state
        .production
        .as_ref()
        .map_or(FailureEvidence::Unknown, |s| {
            s.journal.failure_evidence(id, target, error)
        })
}
impl ProductionPlan {
    pub fn eligible(&self, entry: &core::Entry, day: i64) -> bool {
        self.limit
            .is_some_and(|n| self.progress.used_steps(day) < usize::from(n))
            && !self.progress.refused_today(entry, day)
    }
    pub fn summary(&self, day: i64) -> Value {
        let mut out = json!({"progress":self.progress.summary()});
        out["permission"] = json!(self.limit.is_some());
        out["budget"] = json!({"day":day,"used_steps":self.progress.used_steps(day),"limit":self.limit,"historical":true});
        out
    }
}
#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    plan: &mut ProductionPlan,
    governor: &mut core::Governor,
    producer: &Producer,
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    until: Instant,
    stop: &AtomicBool,
    day: i64,
    entry: &core::Entry,
) -> Result<Run> {
    if stop.load(Ordering::Acquire) {
        return Ok(Run::Deferred);
    }
    state.authority.authorize(token, &state.guard.epoch)?;
    let (step, own, context) = next(plan, process, state, entry, &governor.plan().scope, until)?;
    if stop.load(Ordering::Acquire)
        || (i64::from(context.date_raw) - 43_800_000).div_euclid(24) != day
    {
        return Ok(Run::Deferred);
    }
    match step {
        Next::Deferred | Next::BudgetExhausted => Ok(Run::Deferred),
        Next::Finished => Err(Error::new(
            "action_uncertain",
            "Production completion lacks its governor terminal record",
        )),
        Next::Prepare(target) => {
            let request = plan.progress.begin_preparation(
                entry,
                target,
                &own,
                &context,
                plan.limit.expect("enabled"),
            )?;
            let execution = match production::execute_internal(
                state,
                process,
                gate,
                token,
                &request.id,
                request.target.clone(),
                until,
            ) {
                Ok(e) => e,
                Err(e) => {
                    let evidence = failure(state, &request.id, &request.target, &e);
                    plan.progress.failed(&request.id, &e, evidence, token)?;
                    return Err(e);
                }
            };
            let (fresh, context) = observed(process, state, entry, &governor.plan().scope, until)?;
            plan.progress
                .verified(&request.id, &execution, &fresh, &context)?;
            Ok(Run::Advanced(None))
        }
        Next::Finalize(_) => {
            let mut backend = DailyBackend {
                process,
                state,
                producer,
                gate,
                token,
                until,
                last: None,
                focus: None,
                production: Some(ProductionAttempt { plan, stop }),
            };
            governor
                .continue_entry(
                    &mut backend,
                    &entry.id,
                    crate::daily_observation::unix_ms()?,
                    until,
                )
                .map(|v| Run::Advanced(Some(v)))
        }
    }
}
pub(super) struct ProductionAttempt<'a> {
    pub plan: &'a mut ProductionPlan,
    pub stop: &'a AtomicBool,
}
impl DailyBackend<'_> {
    pub(super) fn production_ready(
        &mut self,
        entry: &core::Entry,
        observed: &core::Observation,
    ) -> Result<core::Readiness> {
        let Some(p) = &mut self.production else {
            return Ok(core::Readiness::Unavailable(
                "Daily production is not enabled".into(),
            ));
        };
        if p.stop.load(Ordering::Acquire) {
            return Ok(core::Readiness::Deferred("Worker stopped".into()));
        }
        let (step, _, context) = next(
            p.plan,
            self.process,
            self.state,
            entry,
            &observed.scope,
            self.until,
        )?;
        if context.date_raw < observed.date_raw {
            return Err(Error::new(
                "torn_read",
                "Production readiness chronology regressed",
            ));
        }
        Ok(match step {
            Next::Finalize(_) => core::Readiness::Ready,
            _ => core::Readiness::Deferred(
                "Production preparation or daily step budget pending".into(),
            ),
        })
    }
    pub(super) fn attempt_production(
        &mut self,
        attempt: &core::Attempt,
        before: &core::Observation,
        until: Instant,
    ) -> Result<core::Outcome> {
        let p = self
            .production
            .as_mut()
            .ok_or_else(|| Error::new("capability_unavailable", "Daily production disabled"))?;
        // Before begin_completion, this method cannot have sent input. A day
        // change or STOP consumes only the governor's known no-input reservation.
        if p.stop.load(Ordering::Acquire) {
            return Ok(core::Outcome::NotQueued("Worker stopped".into()));
        }
        let (step, own, context) = match next(
            p.plan,
            self.process,
            self.state,
            &attempt.entry,
            &attempt.scope,
            until,
        ) {
            Ok(v) => v,
            Err(e) => return Ok(core::Outcome::NotQueued(e.code.into())),
        };
        if !matches!(step, Next::Finalize(_))
            || context.date_raw < before.date_raw
            || (i64::from(context.date_raw) - 43_800_000).div_euclid(24) != attempt.day
        {
            return Ok(core::Outcome::NotQueued(
                "Production final prerequisites changed".into(),
            ));
        }
        let request = p.plan.progress.begin_completion(
            attempt,
            &own,
            &context,
            p.plan.limit.expect("enabled"),
        )?;
        let execution = match production::execute_internal(
            self.state,
            self.process,
            self.gate,
            self.token,
            &request.id,
            request.target.clone(),
            until,
        ) {
            Ok(e) => e,
            Err(e) => {
                let evidence = failure(self.state, &request.id, &request.target, &e);
                p.plan
                    .progress
                    .failed(&request.id, &e, evidence, self.token)?;
                return if evidence == FailureEvidence::Unknown {
                    Err(e)
                } else {
                    Ok(core::Outcome::NotQueued(e.code.into()))
                };
            }
        };
        let (fresh, context) = observed(
            self.process,
            self.state,
            &attempt.entry,
            &attempt.scope,
            until,
        )?;
        let daily = self.producer.observe(self.process, until)?;
        self.state
            .guard
            .accept_context(self.process, &daily.context.campaign)?;
        let (original, original_context) = p.plan.progress.original(&attempt.entry)?;
        let receipt = core::VerifiedReceipt::from_production(
            attempt.clone(),
            &execution,
            original,
            &fresh,
            original_context,
            &context,
            daily.observation,
        )?;
        if Instant::now() >= until {
            return Err(Error::new(
                "action_uncertain",
                "Production final observation exceeded original deadline",
            ));
        }
        p.plan
            .progress
            .verified(&request.id, &execution, &fresh, &context)?;
        Ok(core::Outcome::Verified(Box::new(receipt)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_owner_plan_profile_and_bounded_step_budget_are_all_required() {
        assert!(permission(false, false, false, None).is_ok());
        for owner in [false, true] {
            for profile in [false, true] {
                for limit in [None, Some(0), Some(1), Some(16), Some(17)] {
                    assert_eq!(
                        permission(owner, true, profile, limit).is_ok(),
                        owner && profile && matches!(limit, Some(1..=16))
                    );
                    assert_eq!(
                        permission(owner, false, profile, limit).is_ok(),
                        limit.is_none()
                    );
                }
            }
        }
    }
}
