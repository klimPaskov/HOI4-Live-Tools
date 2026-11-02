//! Coherent own Focus evidence. A verified Start is distinct from completion.
use crate::{
    daily_governor::{Attempt, Entry, Intent, Observation, Readiness, Scope, VerifiedReceipt},
    error::{Error, Result},
    own_focus::AssignmentSnapshot,
};
#[cfg(test)]
mod tests;

fn mismatch(message: &str) -> Error {
    Error::new("receipt_mismatch", message)
}
fn target(entry: &Entry) -> Result<&str> {
    match &entry.intent {
        Intent::Focus { focus_id } => Ok(focus_id),
        _ => Err(mismatch("Focus entry required")),
    }
}
fn scope(snapshot: &AssignmentSnapshot, scope: &Scope) -> Result<()> {
    let c = snapshot.context();
    if snapshot.binding().game() != &scope.epoch.game
        || c.player != scope.epoch.player
        || c.anchors != scope.anchors
        || snapshot.own() != [scope.own_country, scope.own_status]
        || c.difficulty != 2
        || c.historical_raw != 1
        || !scope
            .epoch
            .campaign
            .ends_with(&format!(":{}", c.playthrough_id))
    {
        return Err(mismatch(
            "Exact owned Normal historical Focus scope required",
        ));
    }
    Ok(())
}
/// A prior assignment is only a historical key hint. Fresh completed membership
/// must prove it finished before another idle head may start, including reopen.
pub(super) fn readiness(
    snapshot: &AssignmentSnapshot,
    entry: &Entry,
    observed: &Observation,
    previous: Option<&str>,
) -> Result<Readiness> {
    readiness_at(
        snapshot,
        entry,
        &observed.scope,
        observed.date_raw,
        previous,
    )
}
/// Unpublished scheduling evidence uses the same predicate without creating an
/// Observation or allocating a durable sequence merely to poll a busy Focus.
pub(super) fn readiness_at(
    snapshot: &AssignmentSnapshot,
    entry: &Entry,
    expected: &Scope,
    date_raw: i32,
    previous: Option<&str>,
) -> Result<Readiness> {
    scope(snapshot, expected)?;
    if snapshot.context().date_raw < date_raw {
        return Err(mismatch("Focus readiness regressed"));
    }
    let key = target(entry)?;
    if snapshot.binding().keys().get(key) != Some(&snapshot.binding().target()) {
        return Err(mismatch("Focus observation belongs to another target"));
    }
    if snapshot.completed().contains(key) || snapshot.active_key() == Some(key) {
        return Ok(Readiness::BlockingDecision(
            "Planned Focus already completed or assigned outside this attempt".into(),
        ));
    }
    if snapshot.active_key().is_some() {
        return Ok(Readiness::WaitingForFocus);
    }
    if previous.is_some_and(|key| !snapshot.completed().contains(key)) {
        return Ok(Readiness::BlockingDecision(
            "Previous Focus assignment is idle without fresh completed membership".into(),
        ));
    }
    Ok(Readiness::Ready)
}
/// Only the current invocation's ordinary transport may call this converter.
pub(super) fn complete(
    attempt: &Attempt,
    before_observation: &Observation,
    before: AssignmentSnapshot,
    after: AssignmentSnapshot,
    observed: Observation,
) -> Result<VerifiedReceipt> {
    scope(&before, &attempt.scope)?;
    scope(&after, &attempt.scope)?;
    let key = target(&attempt.entry)?;
    if before_observation.scope != attempt.scope
        || before_observation.sequence != attempt.before_sequence
        || before.context().date_raw < before_observation.date_raw
        || before.active_key().is_some()
        || before.completed().contains(key)
        || before.binding().keys().get(key) != Some(&before.binding().target())
        || before.binding() != after.binding()
        || after.active_key() != Some(key)
        || after.completed().contains(key)
        || !before.completed().is_subset(after.completed())
        || after.context().playthrough_id != before.context().playthrough_id
        || after.context().date_raw < before.context().date_raw
        || observed.date_raw < after.context().date_raw
        || observed.scope != attempt.scope
        || observed.sequence <= attempt.before_sequence
    {
        return Err(mismatch(
            "Fresh exact idle-to-active Focus transition required",
        ));
    }
    Ok(VerifiedReceipt {
        attempt: attempt.clone(),
        current: true,
        canonical_intent: attempt.entry.intent.clone(),
        observed,
    })
}
