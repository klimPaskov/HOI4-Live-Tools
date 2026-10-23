//! Typed readback for leaving one ordinary construction selection. No input,
//! GUI traversal, panel closing, menu inference or later receipt settlement.
use super::{changed, Selection, SelectionAnchor};
use crate::{error::Result, platform::Process};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClearSelectionReadback {
    /// The original requested template remains selected; never success.
    Pending,
    /// Canonical inactive mode with the original own scope and complete queue.
    Cleared,
}

#[derive(Clone, Debug)]
pub(crate) struct ClearSelectionAnchor {
    original: SelectionAnchor,
}

impl SelectionAnchor {
    /// Bind only an inactive selection or the exact requested ordinary template.
    /// This does not authorize Escape, close a panel or select another target.
    pub(crate) fn clear_selection_anchor(&self) -> Result<ClearSelectionAnchor> {
        if self.original.selection != Selection::Inactive && !self.initially_selected() {
            return Err(changed(
                "Only the original requested ordinary template may be cleared",
            ));
        }
        Ok(ClearSelectionAnchor {
            original: self.clone(),
        })
    }
}

impl ClearSelectionAnchor {
    pub(crate) fn initially_inactive(&self) -> bool {
        self.original.original.selection == Selection::Inactive
    }

    pub(crate) fn private_baseline(&self) -> Result<Value> {
        self.original.private_baseline()
    }

    /// Caller owns the original process/build, lease, stop and action deadline.
    /// No rebind, retry, key event or queue change occurs here. The canonical
    /// reader guards original edges before following replacement payload.
    pub(crate) fn observe(&self, process: &dyn Process) -> Result<ClearSelectionReadback> {
        let now = self.original.observe(process)?;
        self.original.same_scope_and_queue(&now)?;
        if now.selection == Selection::Inactive {
            return Ok(ClearSelectionReadback::Cleared);
        }
        if !self.initially_inactive() && self.original.is_selected(&now) {
            return Ok(ClearSelectionReadback::Pending);
        }
        Err(changed(
            "Construction selection changed to another target while clearing",
        ))
    }
}
