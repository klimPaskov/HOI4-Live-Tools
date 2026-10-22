//! Durable mixed-plan destination selection and departure accounting. This module sends no
//! input, observes no process and cannot complete a governor goal. Linux wiring
//! must supply fresh typed destination proofs under the shared controller.
use crate::{
    daily_governor::{Entry, Plan, Scope},
    error::{Error, Result},
    production_step::{FactoryState, PanelState},
    research_stage::OwnState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
mod departure;
#[cfg(any(target_os = "linux", all(test, unix)))]
pub(crate) use departure::Source;
use departure::Step;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "domain", deny_unknown_fields)]
pub(crate) enum Destination {
    Research {
        state: OwnState,
    },
    Production {
        state: FactoryState,
        panel: PanelState,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Policy {
    plan: Plan,
    limit: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
    policy: usize,
    entry: Entry,
    original: Destination,
    date: i32,
    after_sequence: u64,
    terminal: Option<Terminal>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "reason", deny_unknown_fields)]
enum Terminal {
    Completed {
        attempt: crate::daily_governor::Attempt,
        observed: crate::daily_governor::Observation,
        destination: Box<Destination>,
        date: i32,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    version: u8,
    policies: Vec<Policy>,
    selections: Vec<Selection>,
    departures: Vec<Step>,
}

pub(crate) struct Navigator {
    store: crate::daily_governor::store::Store,
    data: Data,
    poisoned: bool,
}
fn invalid(reason: &str) -> Error {
    Error::new("invalid_panel_navigation", reason)
}
fn day(date: i32) -> Result<i64> {
    crate::calendar::decode(date)?;
    Ok((i64::from(date) - 43_800_000).div_euclid(24))
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(value).map_err(|_| invalid("Cannot bind panel selection"))?
        )
    ))
}
impl Destination {
    fn validate(&self, scope: &Scope, entry: &Entry) -> Result<()> {
        use crate::daily_governor::{EconomyTarget, Intent};
        entry.validate()?;
        match (self, &entry.intent) {
            (
                Self::Research { state },
                Intent::Research {
                    technology_id,
                    slot,
                },
            ) => {
                let target = crate::research_stage::Target {
                    epoch: scope.epoch.clone(),
                    stage: crate::research_stage::Stage::Research,
                    technology: technology_id.clone(),
                    slot: *slot,
                    folder_id: None,
                };
                state.validate(&target)?;
                let binding = state
                    .panel_binding
                    .as_ref()
                    .ok_or_else(|| invalid("Original Research roots required"))?;
                if binding.anchors[..2] != [scope.own_country, scope.own_status]
                    || state.researching[*slot as usize].is_some()
                {
                    return Err(invalid(
                        "Research destination must retain own empty original slot",
                    ));
                }
            }
            (
                Self::Production { state, panel },
                Intent::Economy {
                    target:
                        EconomyTarget::Production {
                            line_id,
                            equipment_id,
                            total_factories,
                        },
                },
            ) => {
                let target = crate::production_step::FactoryTarget {
                    epoch: scope.epoch.clone(),
                    line_handle: line_id.clone(),
                    equipment_id: equipment_id.clone(),
                    factories: i32::from(*total_factories),
                };
                state.validate(&target)?;
                crate::production_step::State::Panel(panel.clone()).validate(
                    &crate::production_step::Target::Panel(
                        crate::production_step::PanelTarget::new(scope.epoch.clone()),
                    ),
                )?;
                if state.line.country != scope.own_country
                    || panel.panel_identity[..3]
                        != [scope.own_country, state.line.production, scope.anchors[1]]
                    || state.requested == target.factories
                {
                    return Err(invalid(
                        "Production destination must retain original own unsatisfied line",
                    ));
                }
            }
            _ => {
                return Err(invalid(
                    "Only exact Research and production destinations are supported",
                ))
            }
        }
        Ok(())
    }
}
impl Selection {
    fn verify_current(&self, current: &Destination, date: i32, completed: bool) -> Result<()> {
        day(date)?;
        if date < self.date {
            return Err(invalid("Destination chronology regressed"));
        }
        match (&self.original, current, &self.entry.intent) {
            (
                Destination::Research { state: original },
                Destination::Research { state },
                crate::daily_governor::Intent::Research {
                    technology_id,
                    slot,
                },
            ) => {
                let target = crate::research_stage::Target {
                    epoch: original.epoch.clone(),
                    stage: crate::research_stage::Stage::Research,
                    technology: technology_id.clone(),
                    slot: *slot,
                    folder_id: None,
                };
                state.validate(&target)?;
                let a = original
                    .panel_binding
                    .as_ref()
                    .ok_or_else(|| invalid("Original Research binding absent"))?;
                let b = state
                    .panel_binding
                    .as_ref()
                    .ok_or_else(|| invalid("Current Research binding absent"))?;
                if a.anchors != b.anchors
                    || a.slots != b.slots
                    || a.roots != b.roots
                    || a.facilities_parent != b.facilities_parent
                    || state.researching[*slot as usize].as_deref()
                        != if completed {
                            Some(technology_id.as_str())
                        } else {
                            None
                        }
                {
                    return Err(invalid("Selected Research identity or assignment changed"));
                }
            }
            (
                Destination::Production {
                    state: original,
                    panel: original_panel,
                },
                Destination::Production { state, panel },
                crate::daily_governor::Intent::Economy {
                    target:
                        crate::daily_governor::EconomyTarget::Production {
                            total_factories, ..
                        },
                },
            ) => {
                let target = crate::production_step::FactoryTarget {
                    epoch: original.epoch.clone(),
                    line_handle: format!("0x{:016x}", original.line.handle),
                    equipment_id: original.line.equipment_id.clone(),
                    factories: i32::from(*total_factories),
                };
                state.validate(&target)?;
                crate::production_step::State::Panel(panel.clone()).validate(
                    &crate::production_step::Target::Panel(
                        crate::production_step::PanelTarget::new(original.epoch.clone()),
                    ),
                )?;
                if state.line != original.line
                    || panel.panel_identity != original_panel.panel_identity
                    || (completed && state.requested != i32::from(*total_factories))
                {
                    return Err(invalid(
                        "Selected production identity or final count changed",
                    ));
                }
            }
            _ => return Err(invalid("Current destination domain differs")),
        }
        Ok(())
    }
}
impl Data {
    fn selection_id(&self, selection: &Selection, index: usize) -> Result<String> {
        digest(&(
            "panel-selection-v1",
            index,
            self.policies.get(selection.policy),
            &selection.entry,
            &selection.original,
            selection.date,
            selection.after_sequence,
        ))
    }
    fn validate(&self) -> Result<()> {
        if self.version != 1 || self.policies.len() > 256 || self.selections.len() > 4096 {
            return Err(invalid("Panel selection version/capacity bound"));
        }
        for (index, policy) in self.policies.iter().enumerate() {
            policy.plan.validate()?;
            if !(1..=16).contains(&policy.limit) {
                return Err(invalid("Panel navigation limit must be 1..16"));
            }
            if index > 0
                && (self.policies[index - 1].plan.scope != policy.plan.scope
                    || self.policies[index - 1].plan.revision >= policy.plan.revision)
            {
                return Err(invalid("Panel policy scope/revision regressed"));
            }
        }
        let mut last_date = None;
        let mut last_policy = 0;
        for (index, selection) in self.selections.iter().enumerate() {
            let policy = self
                .policies
                .get(selection.policy)
                .ok_or_else(|| invalid("Selection policy absent"))?;
            day(selection.date)?;
            selection
                .original
                .validate(&policy.plan.scope, &selection.entry)?;
            if !policy.plan.entries.contains(&selection.entry)
                || selection.id != self.selection_id(selection, index)?
                || last_date.is_some_and(|d| selection.date < d)
                || selection.policy < last_policy
            {
                return Err(invalid("Original selection binding/order changed"));
            }
            last_policy = selection.policy;
            last_date = Some(selection.date);
            match &selection.terminal {
                None if index + 1 == self.selections.len()
                    && selection.policy + 1 == self.policies.len() => {}
                Some(Terminal::Completed {
                    attempt,
                    observed,
                    destination,
                    date,
                }) => {
                    selection.verify_current(destination, *date, true)?;
                    if *date < observed.date_raw {
                        return Err(invalid("Completion destination predates governor witness"));
                    }
                    day(observed.date_raw)?;
                    if attempt.entry != selection.entry
                        || attempt.scope != policy.plan.scope
                        || observed.scope != policy.plan.scope
                        || attempt.before_sequence <= selection.after_sequence
                        || observed.sequence <= attempt.before_sequence
                        || observed.date_raw < selection.date
                        || day(observed.date_raw)? < attempt.day
                        || attempt.request_id.len() != 64
                        || !attempt.request_id.bytes().all(|b| b.is_ascii_hexdigit())
                        || observed.source.is_empty()
                        || observed.source.len() > 96
                        || observed.source.chars().any(char::is_control)
                    {
                        return Err(invalid("Selection completion lost governor evidence"));
                    }
                    last_date = Some(*date);
                }
                _ => return Err(invalid("Overlapping or malformed active destination")),
            }
        }
        departure::validate(self)?;
        Ok(())
    }
}
impl Navigator {
    /// Load even when policy/permissions are absent. An active destination is
    /// retained before worker startup; it may not be replaced with another goal.
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let (store, bytes) = crate::daily_governor::store::Store::open(path)?;
        let data = match bytes {
            Some(b) => serde_json::from_slice(&b)
                .map_err(|_| invalid("Malformed panel selection journal"))?,
            None => Data {
                version: 1,
                policies: vec![],
                selections: vec![],
                departures: vec![],
            },
        };
        data.validate()?;
        Ok(Self {
            store,
            data,
            poisoned: false,
        })
    }
    fn check(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(invalid("Panel selection storage is quarantined"));
        }
        if let Err(e) = self.store.check() {
            self.poisoned = true;
            return Err(e);
        }
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        let result = (|| {
            self.data.validate()?;
            self.store.replace(
                &serde_json::to_vec(&self.data)
                    .map_err(|_| invalid("Cannot serialize panel selection"))?,
            )
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub(crate) fn configure(&mut self, plan: &Plan, limit: Option<u8>) -> Result<()> {
        self.ensure_ready()?;
        if let Some(existing) = self.data.policies.last() {
            if existing.plan == *plan && Some(existing.limit) == limit {
                return Ok(());
            }
            if self.active().is_some()
                || existing.plan.scope != plan.scope
                || existing.plan.revision >= plan.revision
            {
                return Err(invalid(
                    "Active destination or exact revision/policy cannot be replaced",
                ));
            }
        }
        let Some(limit) = limit else {
            if self.data.policies.is_empty() {
                return Ok(());
            }
            return Err(invalid("Persisted panel policy cannot silently disappear"));
        };
        let policy = Policy {
            plan: plan.clone(),
            limit,
        };
        // Validate candidate before mutation; malformed proposals do not poison storage.
        let mut next = self.data.clone();
        next.policies.push(policy);
        next.validate()?;
        self.data = next;
        self.save()
    }
    pub(crate) fn original(&mut self) -> Result<Option<Destination>> {
        self.ensure_ready()?;
        Ok(self
            .data
            .selections
            .last()
            .filter(|s| s.terminal.is_none())
            .map(|s| s.original.clone()))
    }
    pub(crate) fn active(&self) -> Option<&Entry> {
        self.data
            .selections
            .last()
            .filter(|s| s.terminal.is_none())
            .map(|s| &s.entry)
    }
    #[cfg(any(target_os = "linux", all(test, unix)))]
    pub(crate) fn select(
        &mut self,
        governor: &mut crate::daily_governor::Governor,
        entry: &Entry,
        original: Destination,
        date: i32,
    ) -> Result<String> {
        self.ensure_ready()?;
        if self.active().is_some() {
            return Err(invalid(
                "Continue the existing destination through its governor attempt",
            ));
        }
        let policy = self
            .data
            .policies
            .len()
            .checked_sub(1)
            .ok_or_else(|| invalid("Explicit panel navigation policy required"))?;
        governor.preparation_check()?;
        if governor.plan() != self.data.policies[policy].plan
            || !matches!(
                governor.entry_state(&entry.id),
                Some(crate::daily_governor::EntryState::Pending { .. })
            )
        {
            return Err(invalid("Original pending governor entry required"));
        }
        let mut selection = Selection {
            id: String::new(),
            policy,
            entry: entry.clone(),
            original,
            date,
            after_sequence: governor.last_sequence(),
            terminal: None,
        };
        selection.id = self
            .data
            .selection_id(&selection, self.data.selections.len())?;
        let id = selection.id.clone();
        let mut next = self.data.clone();
        next.selections.push(selection);
        next.validate()?;
        self.data = next;
        self.save()?;
        Ok(id)
    }
    /// Caller supplies a fresh full producer snapshot under the shared owner lock.
    /// This proves identity only; it never authorizes input or completes a goal.
    pub(crate) fn verify_current(&mut self, current: &Destination, date: i32) -> Result<()> {
        self.ensure_ready()?;
        self.data
            .selections
            .last()
            .filter(|s| s.terminal.is_none())
            .ok_or_else(|| invalid("No active destination"))?
            .verify_current(current, date, false)
    }
    /// Static identities across ordinary Research navigation. UI state is checked
    /// separately by the ordinary adapter; this read confers no input authority.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn verify_research_identity(
        &mut self,
        epoch: &crate::actions::Epoch,
        anchors: &[u64; 6],
        slots: &[u64],
        roots: &[u64; 4],
        facilities_parent: u64,
        date: i32,
    ) -> Result<()> {
        self.ensure_ready()?;
        day(date)?;
        let selection = self
            .data
            .selections
            .last()
            .filter(|s| s.terminal.is_none())
            .ok_or_else(|| invalid("No active destination"))?;
        let Destination::Research { state } = &selection.original else {
            return Err(invalid("Selected destination is not Research"));
        };
        let original = state
            .panel_binding
            .as_ref()
            .ok_or_else(|| invalid("Original Research binding absent"))?;
        if epoch != &state.epoch
            || anchors != &original.anchors
            || slots != original.slots
            || roots != &original.roots
            || facilities_parent != original.facilities_parent
            || date < selection.date
        {
            return Err(invalid(
                "Selected Research identity changed during navigation",
            ));
        }
        Ok(())
    }
    /// Completion comes from the validated governor, never a panel-close receipt.
    #[cfg(any(target_os = "linux", all(test, unix)))]
    pub(crate) fn finish(
        &mut self,
        governor: &mut crate::daily_governor::Governor,
        current: &Destination,
        date: i32,
    ) -> Result<()> {
        self.ensure_ready()?;
        let selection = self
            .data
            .selections
            .last()
            .filter(|s| s.terminal.is_none())
            .ok_or_else(|| invalid("No active destination"))?;
        let policy = &self.data.policies[selection.policy];
        governor.preparation_check()?;
        if governor.plan() != policy.plan || governor.blocks_work() {
            return Err(invalid("Original governor plan must be settled"));
        }
        let Some(crate::daily_governor::EntryState::Complete { attempt, observed }) =
            governor.entry_state(&selection.entry.id)
        else {
            return Err(invalid("Preparation does not complete a destination"));
        };
        selection.verify_current(current, date, true)?;
        let terminal = Terminal::Completed {
            attempt: attempt.clone(),
            observed: *observed.clone(),
            destination: Box::new(current.clone()),
            date,
        };
        let mut next = self.data.clone();
        next.selections.last_mut().expect("active").terminal = Some(terminal);
        next.validate()?;
        self.data = next;
        self.save()
    }
    pub(crate) fn summary(&self) -> Value {
        let latest=self.data.selections.last().map(|s|json!({"entry_id":s.entry.id,"selected_date":s.date,"active":s.terminal.is_none(),"plan_revision":self.data.policies[s.policy].plan.revision}));
        json!({"historical":true,"input_authorized":false,"selection_count":self.data.selections.len(),"storage_quarantined":self.poisoned,"blocked":self.blocked(),"latest":latest,"departures":self.departure_summary()})
    }
}

#[cfg(all(test, unix))]
mod tests;
