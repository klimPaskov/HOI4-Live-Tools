//! Typed readback for leaving one ordinary construction selection. No input,
//! GUI traversal, panel closing, menu inference or later receipt settlement.
use super::{changed, Selection, SelectionAnchor, SelectionState};
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

/// A single finished inactive observation with its exact expected full queue
/// and original clear scope retained. Never a historical receipt or new bind.
#[derive(Clone, Debug)]
pub(crate) struct ClearedProof {
    original: ClearSelectionAnchor,
    cleared: SelectionAnchor,
}
impl ClearedProof {
    pub(crate) fn original_anchor(&self) -> &SelectionAnchor {
        &self.original.original
    }
    pub(crate) fn cleared_anchor(&self) -> &SelectionAnchor {
        &self.cleared
    }
    pub(crate) fn context(&self) -> &crate::game::ClockContext {
        self.cleared.context()
    }
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
        self.classify(&now)
    }

    /// Preserve the same canonical reader/classifier as observe(); no extra
    /// reads, input, retry or replacement baseline. Pending cannot promote.
    pub(crate) fn observe_proof(&self, process: &dyn Process) -> Result<Option<ClearedProof>> {
        let now = self.original.observe(process)?;
        match self.classify(&now)? {
            ClearSelectionReadback::Pending => Ok(None),
            ClearSelectionReadback::Cleared => Ok(Some(ClearedProof {
                original: self.clone(),
                cleared: SelectionAnchor { original: now },
            })),
        }
    }

    fn classify(&self, now: &SelectionState) -> Result<ClearSelectionReadback> {
        self.original.same_scope_and_queue(now)?;
        if now.selection == Selection::Inactive {
            return Ok(ClearSelectionReadback::Cleared);
        }
        if !self.initially_inactive() && self.original.is_selected(now) {
            return Ok(ClearSelectionReadback::Pending);
        }
        Err(changed(
            "Construction selection changed to another target while clearing",
        ))
    }
}
