//! Typed ordinary-production receipt conversion. This grants no input and does
//! not enable the runtime worker; its owner must still bind and execute stages.
use super::{Attempt, EconomyTarget, Intent, Observation, VerifiedReceipt};
use crate::{
    error::{Error, Result},
    game::ClockContext,
    production_step::{Execution, FactoryState, LineIdentity},
};

impl VerifiedReceipt {
    /// Complete only the requested allocation on the originally bound own line.
    /// `original` must come from the durable preparation scope, never a new
    /// baseline adopted after input. `fresh` and `context` must be a witnessed
    /// own-line read after the ordinary action and before `observed` is issued.
    /// Opening, intermediate steps and historical replays cannot complete it.
    pub fn from_production(
        attempt: Attempt,
        execution: &Execution,
        original: &LineIdentity,
        fresh: &FactoryState,
        before_context: &ClockContext,
        context: &ClockContext,
        observed: Observation,
    ) -> Result<Self> {
        let mismatch = || {
            Error::new(
                "receipt_mismatch",
                "Current exact original-line production allocation proof required",
            )
        };
        attempt.entry.validate()?;
        attempt.scope.validate()?;
        let Intent::Economy {
            target:
                EconomyTarget::Production {
                    line_id,
                    equipment_id,
                    total_factories,
                },
        } = &attempt.entry.intent
        else {
            return Err(mismatch());
        };
        let (id, target, before, after) = execution.verified_factory().map_err(|_| mismatch())?;
        fresh.validate(target)?;
        if id != attempt.request_id
            || target.epoch != attempt.scope.epoch
            || target.line_handle != *line_id
            || target.equipment_id != *equipment_id
            || target.factories != i32::from(*total_factories)
            || before.line != *original
            || after.line != *original
            || fresh != after
            || original.country != attempt.scope.own_country
            || observed.scope != attempt.scope
            || observed.sequence <= attempt.before_sequence
            || context.player != attempt.scope.epoch.player
            || context.anchors != attempt.scope.anchors
            || context.difficulty != 2
            || context.historical_raw != 1
            || context.playthrough_id.is_empty()
            || context.playthrough_id != before_context.playthrough_id
            || context.player != before_context.player
            || context.anchors != before_context.anchors
            || context.difficulty != before_context.difficulty
            || context.historical_raw != before_context.historical_raw
            || context.date_raw < before_context.date_raw
            || context.date_raw > observed.date_raw
            || (i64::from(context.date_raw) - 43_800_000).div_euclid(24) < attempt.day
        {
            return Err(mismatch());
        }
        crate::calendar::decode(context.date_raw)?;
        crate::calendar::decode(observed.date_raw)?;
        Ok(Self {
            canonical_intent: attempt.entry.intent.clone(),
            attempt,
            current: true,
            observed,
        })
    }
}
