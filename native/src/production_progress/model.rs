use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    pub entry: Entry,
    pub initial: FactoryState,
    pub context: ClockContext,
}
impl Binding {
    pub fn check(&self, scope: &Scope, fresh: &FactoryState, context: &ClockContext) -> Result<()> {
        let goal = target(scope, &self.entry)?;
        self.initial.validate(&goal)?;
        fresh.validate(&goal)?;
        day(self.context.date_raw)?;
        day(context.date_raw)?;
        if self.initial.line.country != scope.own_country
            || fresh.line != self.initial.line
            || self.context.player != scope.epoch.player
            || self.context.anchors != scope.anchors
            || self.context.difficulty != 2
            || self.context.historical_raw != 1
            || self.context.playthrough_id.is_empty()
            || self.context.playthrough_id.len() > 128
            || self.context.playthrough_id.chars().any(char::is_control)
            || self.context.speed > 5
            || context.speed > 5
            || context.player != self.context.player
            || context.anchors != self.context.anchors
            || context.playthrough_id != self.context.playthrough_id
            || context.difficulty != self.context.difficulty
            || context.historical_raw != self.context.historical_raw
            || context.date_raw < self.context.date_raw
        {
            return Err(Error::new(
                "production_progress_changed",
                "Original production line/campaign/roots/setup/timeline changed",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum Status {
    Intent,
    NotQueued,
    Verified,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Failure {
    pub cause: Diagnostic,
    pub evidence: FailureEvidence,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Step {
    pub entry_id: String,
    pub ordinal: usize,
    pub id: String,
    pub target: Target,
    pub date_raw: i32,
    pub day: i64,
    pub before: i32,
    pub after: Option<i32>,
    pub after_date: Option<i32>,
    pub status: Status,
    pub completion: Option<Attempt>,
    pub failure: Option<Failure>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Data {
    pub version: u8,
    pub scope: Scope,
    pub bindings: Vec<Binding>,
    pub steps: Vec<Step>,
}
impl Data {
    pub fn request_id(&self, s: &Step) -> Result<String> {
        let b = self
            .bindings
            .iter()
            .find(|b| b.entry.id == s.entry_id)
            .ok_or_else(|| invalid("Missing original production binding"))?;
        let raw = serde_json::to_vec(&(
            "production-progress-v1",
            &self.scope,
            b,
            s.ordinal,
            s.day,
            s.date_raw,
            s.before,
            &s.target,
        ))
        .map_err(|_| invalid("Cannot bind production request identity"))?;
        Ok(format!("{:x}", Sha256::digest(raw)))
    }
    pub fn validate(&self) -> Result<()> {
        self.scope.validate()?;
        if self.version != 1 || self.bindings.len() > 256 || self.steps.len() > 4096 {
            return Err(invalid("Production progress version/capacity bound"));
        }
        let mut bindings = BTreeMap::new();
        let mut counts = BTreeMap::new();
        for b in &self.bindings {
            b.check(&self.scope, &b.initial, &b.context)?;
            if bindings.insert(&b.entry.id, b).is_some() {
                return Err(invalid("Duplicate production binding"));
            }
            counts.insert(&b.entry.id, b.initial.requested);
        }
        let mut seen = BTreeSet::new();
        let mut finished = BTreeSet::new();
        let mut refusals = BTreeSet::new();
        let mut latest_date = None;
        for (index, s) in self.steps.iter().enumerate() {
            let b = bindings
                .get(&s.entry_id)
                .ok_or_else(|| invalid("Step has no original binding"))?;
            let goal = target(&self.scope, &b.entry)?;
            let expected = *counts
                .get(&s.entry_id)
                .ok_or_else(|| invalid("Missing progression baseline"))?;
            if s.ordinal != index + 1
                || s.id.len() != 64
                || !s
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || !seen.insert(&s.id)
                || finished.contains(&s.entry_id)
                || refusals.contains(&(&s.entry_id, s.day))
                || s.target.epoch() != &self.scope.epoch
                || s.day != day(s.date_raw)?
                || s.date_raw < b.context.date_raw
                || latest_date.is_some_and(|d| s.date_raw < d)
                || s.before != expected
                || !(0..=1_000_000).contains(&s.before)
                || s.failure.as_ref().is_some_and(|f| !f.cause.validate())
            {
                return Err(invalid(
                    "Production history identity/chronology/progression differs",
                ));
            }
            latest_date = Some(s.date_raw);
            let final_stage = s.completion.is_some();
            match &s.target {
                Target::Panel(t)
                    if t.stage == crate::production_step::PanelStage::EnsureOpen
                        && !final_stage
                        && s.before != goal.factories => {}
                Target::Factories(t) => {
                    t.validate()?;
                    let wanted = if final_stage {
                        goal.factories
                    } else {
                        s.before + (goal.factories - s.before).signum()
                    };
                    if t.line_handle != goal.line_handle
                        || t.equipment_id != goal.equipment_id
                        || t.factories != wanted
                        || (t.factories - s.before).abs() > 1
                        || (!final_stage
                            && ((goal.factories - s.before).abs() <= 1 || t.factories == s.before))
                    {
                        return Err(invalid("Preparation must be a single intermediate step; completion must reach the exact goal"));
                    }
                }
                _ => return Err(invalid("Opening cannot be a completion stage")),
            }
            if let Some(a) = &s.completion {
                if a.entry != b.entry
                    || a.scope != self.scope
                    || a.request_id != s.id
                    || a.before_sequence == 0
                    || a.day != s.day
                {
                    return Err(invalid("Final step lost original governor attempt"));
                }
            } else if self.request_id(s)? != s.id {
                return Err(invalid("Preparation request identity changed"));
            }
            match s.status {
                Status::Intent
                    if s.after.is_none()
                        && s.after_date.is_none()
                        && index + 1 == self.steps.len()
                        && s.failure
                            .as_ref()
                            .is_none_or(|f| f.evidence == FailureEvidence::Unknown) => {}
                Status::NotQueued
                    if s.after.is_none()
                        && s.after_date.is_none()
                        && s.failure
                            .as_ref()
                            .is_some_and(|f| f.evidence != FailureEvidence::Unknown) =>
                {
                    refusals.insert((&s.entry_id, s.day));
                }
                Status::Verified
                    if s.failure
                        .as_ref()
                        .is_none_or(|f| f.evidence == FailureEvidence::Unknown) =>
                {
                    let wanted = match &s.target {
                        Target::Panel(_) => s.before,
                        Target::Factories(t) => t.factories,
                    };
                    if s.after != Some(wanted) {
                        return Err(invalid("Verified step count differs from target"));
                    }
                    let after_date = s
                        .after_date
                        .ok_or_else(|| invalid("Verified step needs its observed date"))?;
                    day(after_date)?;
                    if after_date < s.date_raw {
                        return Err(invalid("Production completion chronology regressed"));
                    }
                    latest_date = Some(after_date);
                    counts.insert(&s.entry_id, wanted);
                    if final_stage {
                        finished.insert(&s.entry_id);
                    }
                }
                _ => {
                    return Err(invalid(
                        "Unresolved/terminal production history is malformed",
                    ))
                }
            }
        }
        Ok(())
    }
}
