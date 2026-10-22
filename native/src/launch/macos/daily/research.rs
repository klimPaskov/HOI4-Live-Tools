//! Current typed Mac Research evidence. No legacy JSON journal is proof.
use crate::{
    daily_governor::{Attempt, Entry, Intent, Observation, Readiness, Scope, VerifiedReceipt},
    error::{Error, Result},
    memory::Reader,
    platform::Process,
    research::{self, ResearchContext},
    research_ui::Witnesses,
};
use std::collections::BTreeSet;

pub(super) struct Snapshot {
    context: ResearchContext,
    slots: Vec<Option<String>>,
    completed: BTreeSet<String>,
}
pub(super) fn observe(process: &dyn Process) -> Result<Snapshot> {
    process.begin_snapshot()?;
    let witnesses = Witnesses::new(process);
    let reader = Reader::new(&witnesses);
    let (_, context) = research::observe_from_reader(&reader, process.image_base(), 0, 1, true)?;
    let (slots, completed) = research::assignments(&reader, process.image_base())?;
    witnesses.finish()?;
    process.check()?;
    Ok(Snapshot {
        context: context.ok_or_else(|| mismatch("Own Research context missing"))?,
        slots,
        completed,
    })
}
fn mismatch(s: &str) -> Error {
    Error::new("receipt_mismatch", s)
}
pub(super) fn scope(snapshot: &Snapshot, scope: &Scope) -> Result<()> {
    let c = &snapshot.context;
    if c.campaign.player != scope.epoch.player
        || c.campaign.anchors != scope.anchors
        || c.own != [scope.own_country, scope.own_status]
        || c.campaign.difficulty != 2
        || c.campaign.historical_raw != 1
        || !scope
            .epoch
            .campaign
            .ends_with(&format!(":{}", c.campaign.playthrough_id))
        || c.slots.is_empty()
        || c.slots.len() != snapshot.slots.len()
        || c.slots.len() > 32
        || c.slots.iter().any(|p| *p < 0x10000)
        || c.slots.iter().collect::<BTreeSet<_>>().len() != c.slots.len()
    {
        return Err(mismatch(
            "Exact owned Normal historical Research scope required",
        ));
    }
    Ok(())
}
pub(super) fn readiness(
    snapshot: &Snapshot,
    entry: &Entry,
    observed: &Observation,
) -> Result<Readiness> {
    scope(snapshot, &observed.scope)?;
    if snapshot.context.campaign.date_raw < observed.date_raw {
        return Err(mismatch("Research readiness regressed"));
    }
    let Intent::Research {
        technology_id,
        slot,
    } = &entry.intent
    else {
        return Err(mismatch("Research entry required"));
    };
    let current = snapshot
        .slots
        .get(*slot as usize)
        .ok_or_else(|| mismatch("Own slot missing"))?;
    if snapshot.completed.contains(technology_id)
        || snapshot
            .slots
            .iter()
            .any(|s| s.as_ref() == Some(technology_id))
    {
        return Ok(Readiness::BlockingDecision(
            "Planned technology already completed or assigned outside this attempt".into(),
        ));
    }
    Ok(if current.is_some() {
        Readiness::WaitingForResearchSlot
    } else {
        Readiness::Ready
    })
}
/// Only called after this invocation's ordinary transport succeeds. The private
/// snapshots and exact postcondition cannot be supplied through MCP arguments.
pub(super) fn complete(
    attempt: &Attempt,
    before: Snapshot,
    after: Snapshot,
    observed: Observation,
) -> Result<VerifiedReceipt> {
    scope(&before, &attempt.scope)?;
    scope(&after, &attempt.scope)?;
    let Intent::Research {
        technology_id,
        slot,
    } = &attempt.entry.intent
    else {
        return Err(mismatch("Research attempt required"));
    };
    let slot = *slot as usize;
    let a = &before.context;
    let b = &after.context;
    if before.slots.get(slot) != Some(&None)
        || before.completed.contains(technology_id)
        || before
            .slots
            .iter()
            .any(|s| s.as_ref() == Some(technology_id))
        || after.slots.get(slot).and_then(Option::as_ref) != Some(technology_id)
        || a.slots != b.slots
        || a.campaign.playthrough_id != b.campaign.playthrough_id
        || b.campaign.date_raw < a.campaign.date_raw
        || observed.date_raw < b.campaign.date_raw
        || observed.scope != attempt.scope
        || observed.sequence <= attempt.before_sequence
        || !before.completed.is_subset(&after.completed)
        || after.completed.contains(technology_id)
    {
        return Err(mismatch(
            "Fresh exact empty-to-assigned Research transition required",
        ));
    }
    for (index, (old, new)) in before.slots.iter().zip(&after.slots).enumerate() {
        if index != slot
            && old != new
            && !matches!((old,new),(Some(key),None) if after.completed.contains(key))
        {
            return Err(mismatch("Unrelated Research assignment changed"));
        }
    }
    Ok(VerifiedReceipt {
        attempt: attempt.clone(),
        current: true,
        canonical_intent: attempt.entry.intent.clone(),
        observed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actions::Epoch, daily_governor::OwnCounts, game::ClockContext, platform::Identity,
    };
    fn fixture() -> (Attempt, Snapshot, Snapshot, Observation) {
        let scope = Scope {
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
        };
        let context = ResearchContext {
            campaign: ClockContext {
                playthrough_id: "fixture".into(),
                player: "GER".into(),
                anchors: scope.anchors,
                date_raw: 60759624,
                difficulty: 2,
                historical_raw: 1,
                paused: false,
                speed: 1,
            },
            own: [scope.own_country, scope.own_status],
            slots: vec![0x60000, 0x70000],
        };
        let attempt = Attempt {
            request_id: "current".into(),
            entry: Entry {
                id: "one".into(),
                intent: Intent::Research {
                    technology_id: "target".into(),
                    slot: 0,
                },
                navigation: None,
                focus_search: None,
            },
            scope: scope.clone(),
            day: 0,
            before_sequence: 2,
        };
        let before = Snapshot {
            context: context.clone(),
            slots: vec![None, Some("other".into())],
            completed: BTreeSet::new(),
        };
        let after = Snapshot {
            context,
            slots: vec![Some("target".into()), Some("other".into())],
            completed: BTreeSet::new(),
        };
        let observed = Observation {
            scope,
            date_raw: 60759624,
            sequence: 3,
            observed_unix_ms: 1,
            source: "fixture".into(),
            counts: OwnCounts::default(),
        };
        (attempt, before, after, observed)
    }
    #[test]
    fn fresh_transition_and_unrelated_natural_completion_pass() {
        let (a, b, c, o) = fixture();
        complete(&a, b, c, o).unwrap();
        let (a, b, mut c, o) = fixture();
        c.slots[1] = None;
        c.completed.insert("other".into());
        complete(&a, b, c, o).unwrap();
    }
    #[test]
    fn stale_scope_membership_rewind_and_unrelated_mutation_refuse() {
        for variant in 0..9 {
            let (a, mut b, mut c, mut o) = fixture();
            match variant {
                0 => o.sequence = 2,
                1 => c.context.slots[0] += 8,
                2 => c.context.campaign.date_raw -= 1,
                3 => c.slots[1] = Some("replacement".into()),
                4 => c.slots[1] = None,
                5 => b.slots[0] = Some("target".into()),
                6 => c.context.own[0] += 8,
                7 => c.context.campaign.difficulty = 1,
                _ => {
                    b.completed.insert("target".into());
                }
            }
            assert_eq!(complete(&a, b, c, o).unwrap_err().code, "receipt_mismatch");
        }
    }
    #[test]
    fn busy_slot_waits_but_external_assignment_is_a_decision() {
        let (a, mut b, _, o) = fixture();
        assert!(matches!(
            readiness(&b, &a.entry, &o).unwrap(),
            Readiness::Ready
        ));
        b.slots[0] = Some("busy".into());
        assert!(matches!(
            readiness(&b, &a.entry, &o).unwrap(),
            Readiness::WaitingForResearchSlot
        ));
        b.slots[0] = Some("target".into());
        assert!(matches!(
            readiness(&b, &a.entry, &o).unwrap(),
            Readiness::BlockingDecision(_)
        ));
    }
}
