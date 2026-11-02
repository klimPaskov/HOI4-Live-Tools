//! One future ordinary placement's typed queue delta, separate from input and
//! from selection's strict unchanged-queue proof. No coordinates or UI payload.
use super::{canonical_with_queue, same_context, QueueRead, SelectionAnchor, SelectionState};
use crate::{
    construction::{mac_ui::Binding, QueueLine},
    error::{Error, Result},
    platform::Process,
};
use serde_json::Value;
use std::collections::BTreeSet;

#[cfg(test)]
#[path = "../../../tests/support/mac_construction_placement_state.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacementDelta {
    /// A complete unchanged queue is pending, never a successful placement.
    Pending,
    ExistingLineIncremented,
    NewLineAdded,
}

#[derive(Clone, Debug)]
pub(crate) struct PlacementReadback {
    delta: PlacementDelta,
    original: PlacementAnchor,
    observed: SelectionState,
}
impl PlacementReadback {
    pub(crate) fn delta(&self) -> PlacementDelta {
        self.delta
    }
    pub(crate) fn placed(&self) -> bool {
        self.delta != PlacementDelta::Pending
    }
    /// Promote only the complete classified +1 observation, without another
    /// read/rebind. Pending has no handoff proof and cannot seed clearing.
    pub(crate) fn proof(&self) -> Option<PlacementProof> {
        self.placed().then(|| PlacementProof {
            original: self.original.clone(),
            placed: SelectionAnchor {
                original: self.observed.clone(),
            },
            delta: self.delta,
        })
    }
}

/// Exact +1 proof with the full proven post-placement queue retained in memory.
/// No public constructor/serde or JSON receipt can create this transition.
#[derive(Clone, Debug)]
pub(crate) struct PlacementProof {
    original: PlacementAnchor,
    placed: SelectionAnchor,
    delta: PlacementDelta,
}
impl PlacementProof {
    pub(crate) fn original_anchor(&self) -> &SelectionAnchor {
        &self.original.selected
    }
    pub(crate) fn placed_anchor(&self) -> &SelectionAnchor {
        &self.placed
    }
    pub(crate) fn delta(&self) -> PlacementDelta {
        self.delta
    }
    pub(crate) fn context(&self) -> &crate::game::ClockContext {
        self.placed.context()
    }
    /// The expected new queue is taken only from this exact +1 proof. The old
    /// pre-placement queue remains retained by original_anchor().
    pub(crate) fn clear_anchor(&self) -> Result<super::ClearSelectionAnchor> {
        self.placed.clear_selection_anchor()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PlacementAnchor {
    selected: SelectionAnchor,
}

fn changed(message: &str) -> Error {
    Error::new("construction_placement_changed", message)
}

impl SelectionAnchor {
    /// Bind only an already selected exact canonical target. This neither
    /// selects a state nor authorizes a map click or any other input.
    pub(crate) fn placement_anchor(&self) -> Result<PlacementAnchor> {
        if !self.initially_selected() {
            return Err(Error::new(
                "construction_placement_unavailable",
                "Original exact requested ordinary template must already be selected",
            ));
        }
        Ok(PlacementAnchor {
            selected: self.clone(),
        })
    }
}

impl PlacementAnchor {
    /// Caller supplies the original deadline/lease/stop-fenced Process. No
    /// observation retry, deadline renewal or possible input happens here.
    pub(crate) fn observe(&self, process: &dyn Process) -> Result<PlacementReadback> {
        let b = &self.selected.original.binding;
        let state = canonical_with_queue(
            process,
            &b.target.key,
            b.state_id,
            Some(&self.selected),
            QueueRead::Placement,
        )?;
        Ok(PlacementReadback {
            delta: self.classify(&state)?,
            original: self.clone(),
            observed: state,
        })
    }

    pub(crate) fn private_baseline(&self) -> Result<Value> {
        self.selected.private_baseline()
    }

    fn classify(&self, state: &SelectionState) -> Result<PlacementDelta> {
        let before = &self.selected.original;
        same_context(&before.context, &state.context)?;
        if !same_scope(&before.binding, &state.binding) || before.selection != state.selection {
            return Err(changed(
                "Original placement scope, catalog, target or selection changed",
            ));
        }
        let original = &before.binding.queue_lines;
        let current = &state.binding.queue_lines;
        let original_members = &before.binding.queue_members;
        let current_members = &state.binding.queue_members;
        if current.len() == original.len() {
            if current_members != original_members {
                return Err(changed(
                    "Original placement queue rows were reordered or replaced",
                ));
            }
            let mut incremented = false;
            for (old, new) in original.iter().zip(current) {
                if old == new {
                    continue;
                }
                if incremented
                    || !same_row_identity(old, new)
                    || !requested_row(new, &before.binding)
                    || old.amount.checked_add(1) != Some(new.amount)
                {
                    return Err(changed(
                        "Placement must increment exactly one matching original line by one",
                    ));
                }
                incremented = true;
            }
            return Ok(if incremented {
                PlacementDelta::ExistingLineIncremented
            } else {
                PlacementDelta::Pending
            });
        }
        if current.len() != original.len() + 1 {
            return Err(changed(
                "Placement must retain every original queue row and add at most one",
            ));
        }
        let known: BTreeSet<_> = original_members.iter().copied().collect();
        let mut old_index = 0;
        let mut added = false;
        for row in current {
            if original
                .get(old_index)
                .is_some_and(|old| old.address == row.address)
            {
                if original[old_index] != *row {
                    return Err(changed(
                        "Every original row must remain unchanged when a new line is added",
                    ));
                }
                old_index += 1;
            } else if added
                || known.contains(&row.address)
                || row.amount != 1
                || !requested_row(row, &before.binding)
            {
                return Err(changed(
                    "Only one new matching amount-one queue line is permitted",
                ));
            } else {
                added = true;
            }
        }
        if !added || old_index != original.len() {
            return Err(changed(
                "Placement removed or reordered an original queue row",
            ));
        }
        Ok(PlacementDelta::NewLineAdded)
    }
}

fn same_scope(a: &Binding, b: &Binding) -> bool {
    a.process == b.process
        && a.base == b.base
        && a.own_tag == b.own_tag
        && a.country == b.country
        && a.production == b.production
        && a.state == b.state
        && a.state_id == b.state_id
        && a.registry == b.registry
        && a.manager == b.manager
        && a.catalog == b.catalog
        && a.target == b.target
}

fn same_row_identity(a: &QueueLine, b: &QueueLine) -> bool {
    a.address == b.address
        && a.handle == b.handle
        && a.building == b.building
        && a.key == b.key
        && a.building_identity == b.building_identity
        && a.location == b.location
}

fn requested_row(row: &QueueLine, binding: &Binding) -> bool {
    row.building
        && row.key.as_deref() == Some(binding.target.key.as_str())
        && row.building_identity.is_some_and(|(_, template, ordinal)| {
            template == binding.target.address && ordinal == binding.target.ordinal
        })
        && row
            .location
            .as_ref()
            .is_some_and(|l| l.state == binding.state && l.state_id == binding.state_id)
}
