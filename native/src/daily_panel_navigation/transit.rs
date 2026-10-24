//! Volatile navigation through Research while retaining another domain's goal.
//!
//! No Research technology/slot target, input, journal or governor completion is
//! created here. The caller supplies same-attempt typed owned observations under
//! its original authority/witness bracket. Existing destination/departure v1
//! journals cannot deserialize these types and are not migrated.
use crate::{
    daily_governor::{EconomyTarget, Entry, Intent, Scope},
    error::{Error, Result},
    game::ClockContext,
    research_stage::OwnState,
    research_ui::UiContext,
};
use std::{sync::Arc, time::Instant};

fn invalid(message: &str) -> Error {
    Error::new("invalid_panel_transit", message)
}
fn deadline() -> Error {
    Error::new(
        "panel_transit_deadline",
        "Research transit exceeded original caller deadline",
    )
}
fn current_context(scope: &Scope, context: &ClockContext) -> Result<()> {
    scope.validate()?;
    crate::calendar::decode(context.date_raw)?;
    if context.anchors != scope.anchors
        || context.player != scope.epoch.player
        || context.difficulty != 2
        || context.historical_raw != 1
        || context.playthrough_id.len() != 36
        || !scope
            .epoch
            .campaign
            .ends_with(&format!(":{}", context.playthrough_id))
    {
        return Err(Error::new(
            "ownership_changed",
            "Research transit own campaign/setup differs",
        ));
    }
    Ok(())
}

/// Selected Linux managed panel only; this is not a visibility/input proof for
/// the source domain. Its own ordinary adapter must qualify departure/opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ManagedPanel {
    Idle,
    Production,
    Politics,
    Focus,
    Research,
}
impl ManagedPanel {
    fn from_state(state: &OwnState) -> Result<Self> {
        let panel = match state.active_mode {
            Some(23) => Self::Idle,
            Some(2) => Self::Production,
            Some(12) => Self::Politics,
            Some(13) => Self::Focus,
            Some(5) => Self::Research,
            _ => return Err(invalid("Unknown/unsupported Research transit source panel")),
        };
        if state.panel_visible != Some(panel == Self::Research)
            || state.tree_visible
            || !state.visible_info.is_empty()
            || state
                .panel_binding
                .as_ref()
                .is_none_or(|binding| binding.tab != 0)
        {
            return Err(invalid(
                "Settled Research panel, no tree/Info and ordinary tab required",
            ));
        }
        Ok(panel)
    }
}

/// Validates a trusted internal typed UI pair, not an execution receipt. It
/// does not parse JSON or establish memory-producer authority: even typed caller
/// data need exact process, original owner-before-payload checks, encompassing
/// final witnesses and fresh same-attempt context supplied by the owner.
#[derive(Debug)]
pub(crate) struct Snapshot {
    scope: Scope,
    context: ClockContext,
    research: OwnState,
    panel: ManagedPanel,
}
impl Snapshot {
    pub(crate) fn from_observed(scope: &Scope, research: OwnState, ui: &UiContext) -> Result<Self> {
        current_context(scope, &ui.campaign)?;
        research.validate_navigation(&scope.epoch)?;
        let binding = research
            .panel_binding
            .as_ref()
            .ok_or_else(|| invalid("Research roots absent"))?;
        if binding.anchors != ui.anchors
            || binding.slots != ui.slots
            || binding.anchors[..2] != [scope.own_country, scope.own_status]
        {
            return Err(Error::new(
                "ownership_changed",
                "Typed Research scope/panel observations disagree",
            ));
        }
        let panel = ManagedPanel::from_state(&research)?;
        Ok(Self {
            scope: scope.clone(),
            context: ui.campaign.clone(),
            research,
            panel,
        })
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn panel(&self) -> ManagedPanel {
        self.panel
    }
    pub(crate) fn research(&self) -> &OwnState {
        &self.research
    }
    pub(crate) fn context(&self) -> &ClockContext {
        &self.context
    }
}

/// Original logical destination, sealed to the original observed scope. This
/// does not replace Focus/production's independent canonical target proof and
/// cannot be converted to a VerifiedReceipt. No Clone/serde constructor exists.
#[derive(Debug)]
pub(crate) struct Goal {
    scope: Scope,
    entry: Entry,
}
impl Goal {
    fn new(scope: Scope, entry: Entry) -> Result<Self> {
        entry.validate()?;
        match &entry.intent {
            Intent::Focus { focus_id }
                if focus_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'_') => {}
            Intent::Economy {
                target:
                    EconomyTarget::Production {
                        line_id,
                        equipment_id,
                        total_factories,
                    },
            } => {
                crate::production_step::FactoryTarget {
                    epoch: scope.epoch.clone(),
                    line_handle: line_id.clone(),
                    equipment_id: equipment_id.clone(),
                    factories: i32::from(*total_factories),
                }
                .validate()?;
            }
            _ => {
                return Err(invalid(
                    "Transit must retain an exact Focus or production goal",
                ))
            }
        }
        Ok(Self { scope, entry })
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn entry(&self) -> &Entry {
        &self.entry
    }
    fn require(&self, scope: &Scope, entry: &Entry) -> Result<()> {
        if scope != &self.scope || entry != &self.entry {
            return Err(Error::new(
                "ownership_changed",
                "Original transit scope/final goal changed",
            ));
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct Transit {
    goal: Arc<Goal>,
    original: Snapshot,
    last_date: i32,
    until: Instant,
}
/// Positive same-attempt panel observation only. There is deliberately no goal
/// completion/count, input-permission, serialization or historical conversion.
#[derive(Debug)]
pub(crate) struct AtResearch {
    goal: Arc<Goal>,
    observed: Snapshot,
    until: Instant,
}
impl AtResearch {
    pub(crate) fn goal(&self) -> &Goal {
        &self.goal
    }
    pub(crate) fn observed(&self) -> &Snapshot {
        &self.observed
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.until
    }
}
#[derive(Debug)]
pub(crate) enum Progress {
    Pending(ManagedPanel),
    AtResearch(Box<AtResearch>),
}
impl Transit {
    pub(crate) fn bind(entry: Entry, original: Snapshot, until: Instant) -> Result<Self> {
        Self::bind_at(entry, original, until, Instant::now())
    }
    fn bind_at(entry: Entry, original: Snapshot, until: Instant, now: Instant) -> Result<Self> {
        if now >= until {
            return Err(deadline());
        }
        let last_date = original.context.date_raw;
        let goal = Arc::new(Goal::new(original.scope.clone(), entry)?);
        Ok(Self {
            goal,
            original,
            last_date,
            until,
        })
    }
    pub(crate) fn goal(&self) -> &Goal {
        &self.goal
    }
    pub(crate) fn original_panel(&self) -> ManagedPanel {
        self.original.panel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.until
    }
    /// One fresh read, no loop/retry. The immutable goal/scope and original
    /// deadline are checked BEFORE invoking the caller's scoped observer. That
    /// observer receives the immutable original snapshot to reject original
    /// pointer replacements before their payload, within its own final witness.
    pub(crate) fn observe(
        &mut self,
        scope: &Scope,
        entry: &Entry,
        observe: impl FnOnce(Instant, &Snapshot) -> Result<Snapshot>,
    ) -> Result<Progress> {
        self.observe_with(scope, entry, observe, Instant::now)
    }
    fn observe_with(
        &mut self,
        scope: &Scope,
        entry: &Entry,
        observe: impl FnOnce(Instant, &Snapshot) -> Result<Snapshot>,
        mut now: impl FnMut() -> Instant,
    ) -> Result<Progress> {
        if now() >= self.until {
            return Err(deadline());
        }
        self.goal.require(scope, entry)?;
        // Preserve a concrete callback failure, even if that read was late.
        // No error produces progress or grants retry/settlement permission.
        let sample = observe(self.until, &self.original)?;
        if now() >= self.until {
            return Err(deadline());
        }
        self.goal.require(&sample.scope, entry)?;
        let before = &self.original.context;
        let current = &sample.context;
        let original = self
            .original
            .research
            .panel_binding
            .as_ref()
            .ok_or_else(|| invalid("Original roots absent"))?;
        let binding = sample
            .research
            .panel_binding
            .as_ref()
            .ok_or_else(|| invalid("Current roots absent"))?;
        if current.playthrough_id != before.playthrough_id
            || current.difficulty != before.difficulty
            || current.historical_raw != before.historical_raw
            || current.date_raw < self.last_date
            || binding.anchors != original.anchors
            || binding.slots != original.slots
            || binding.roots != original.roots
            || binding.facilities_parent != original.facilities_parent
        {
            return Err(Error::new(
                "ownership_changed",
                "Original Research roots/slots/setup or chronology changed",
            ));
        }
        // Research assignment strings are unrelated scheduling facts. Completion
        // (or another current own assignment) never becomes an empty-slot goal
        // dependency and never produces a governor assignment receipt here.
        // Slot pointers remain exact; current canonical keys were validated.
        if now() >= self.until {
            return Err(deadline());
        }
        self.last_date = current.date_raw;
        if sample.panel == ManagedPanel::Research {
            Ok(Progress::AtResearch(Box::new(AtResearch {
                goal: Arc::clone(&self.goal),
                observed: sample,
                until: self.until,
            })))
        } else {
            Ok(Progress::Pending(sample.panel))
        }
    }
}

#[cfg(test)]
mod tests;
