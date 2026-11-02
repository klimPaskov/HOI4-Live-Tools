//! Default-off once-only Escape on an already-open own construction panel.
//! An inactive original selection is verified without sending a key.
use super::{
    action_fence::Fence,
    construction_open::read_recovery,
    journal::{GameJournal, StrictOutcome},
};
use crate::{
    actions::Epoch,
    construction::{
        mac_panel::cancel::CancelAnchor,
        mac_ui::{self, ClearSelectionAnchor, ClearSelectionReadback},
    },
    error::{Error, Result},
    platform::{
        key_once::Dispatch as KeyDispatch,
        macos_input::{Key, MacKeys},
    },
};
use serde_json::{json, Value};
use std::{
    thread,
    time::{Duration, Instant},
};

pub(super) struct Request<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub state_id: i32,
}
impl Request<'_> {
    fn action(&self) -> Value {
        json!({"kind":"construction_clear_selection","building_id":self.key,
            "state_id":self.state_id,"transport":"ordinary_escape_once"})
    }
}
/// The shared historical envelope takes no Process, keys or context loader.
/// Known refusal and sticky uncertainty retain the journal's exact semantics.
pub(super) fn historical_receipt(
    request: &Request<'_>,
    epoch: &Epoch,
    journal: &GameJournal,
) -> Result<Option<Value>> {
    Ok(journal.strict_receipt(request.id, epoch, &request.action())?.map(|receipt| {
        json!({"receipt":receipt,"historical_receipt":true,"current_selection_verified":false})
    }))
}
trait Attempt {
    /// True only if the ORIGINAL selection was inactive and is still inactive.
    fn prepare(&mut self) -> Result<bool>;
    fn press_once(&mut self) -> KeyDispatch;
    fn finish(&mut self, sent: bool) -> Result<Value>;
}
fn dispatch(attempt: &mut impl Attempt) -> StrictOutcome {
    let inactive = match attempt.prepare() {
        Ok(v) => v,
        Err(e) => return StrictOutcome::NotQueued(e),
    };
    let sent = !inactive;
    if sent {
        match attempt.press_once() {
            KeyDispatch::NotQueued(e) => return StrictOutcome::NotQueued(e),
            KeyDispatch::Unknown(Some(e)) => return StrictOutcome::Uncertain(e),
            KeyDispatch::Unknown(None) => {}
        }
    }
    match attempt.finish(sent) {
        Ok(v) => StrictOutcome::Verified(v),
        Err(e) if sent => StrictOutcome::Uncertain(e),
        Err(e) => StrictOutcome::NotQueued(e),
    }
}
fn changed(message: &str) -> Error {
    Error::new("construction_cancel_changed", message)
}
fn retained_read<T>(
    mut retained: impl FnMut() -> Result<()>,
    observe: impl FnOnce() -> Result<T>,
) -> Result<T> {
    retained()?;
    let read = observe();
    let after = retained();
    match read {
        Err(e) => Err(e),
        Ok(v) => {
            after?;
            Ok(v)
        }
    }
}
/// Same-attempt polling only. No input call, renewed bound or later settlement.
fn finish_readback(
    until: Instant,
    sent: bool,
    mut check: impl FnMut() -> Result<()>,
    mut observe: impl FnMut() -> Result<ClearSelectionReadback>,
    mut platform: impl FnMut() -> Result<()>,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(Duration),
) -> Result<()> {
    let boundary = |check: &mut dyn FnMut() -> Result<()>, now: &mut dyn FnMut() -> Instant| {
        if now() >= until {
            return Err(Error::new(
                "action_deadline",
                "Original clear-selection deadline expired",
            ));
        }
        check()?;
        if now() >= until {
            return Err(Error::new(
                "action_deadline",
                "Original clear-selection deadline expired",
            ));
        }
        Ok(())
    };
    loop {
        boundary(&mut check, &mut now)?;
        let observed = observe()?;
        boundary(&mut check, &mut now)?;
        if observed == ClearSelectionReadback::Cleared {
            platform()?;
            boundary(&mut check, &mut now)?;
            if observe()? != ClearSelectionReadback::Cleared {
                return Err(changed("Clear readback changed after platform checks"));
            }
            return boundary(&mut check, &mut now);
        }
        if !sent {
            return Err(changed("Originally inactive selection no longer inactive"));
        }
        boundary(&mut check, &mut now)?;
        wait(Duration::from_millis(10).min(until.saturating_duration_since(now())));
        boundary(&mut check, &mut now)?;
    }
}
struct Live<'a, 'b> {
    request: &'a Request<'a>,
    anchor: &'a ClearSelectionAnchor,
    ui: Option<&'a CancelAnchor>,
    fence: &'a Fence<'b>,
    // Retain unguarded keys for platform checks; recursive guarded checks refuse.
    keys: &'a MacKeys<'b>,
}
impl Live<'_, '_> {
    fn readback(&self) -> Result<ClearSelectionReadback> {
        read_recovery::observe(self.fence, "clear_selection_readback", || {
            self.anchor.observe(self.fence)
        })
    }
    fn pending(&self) -> Result<()> {
        if self.readback()? != ClearSelectionReadback::Pending {
            return Err(changed("Original ordinary selection changed before Escape"));
        }
        Ok(())
    }
    fn before_key(&self) -> Result<()> {
        retained_read(
            || self.pending(),
            || {
                read_recovery::observe(self.fence, "clear_selection_escape", || {
                    self.ui
                        .ok_or_else(|| changed("Original Escape binding required"))?
                        .require_before(self.fence)
                })
            },
        )
    }
}
impl Attempt for Live<'_, '_> {
    fn prepare(&mut self) -> Result<bool> {
        self.fence.input_check()?;
        if self.anchor.initially_inactive() {
            if self.readback()? != ClearSelectionReadback::Cleared {
                return Err(changed("Original inactive selection changed"));
            }
            return Ok(true);
        }
        self.before_key()?;
        self.keys.confirm_input_idle()?;
        self.fence.input_check()?;
        Ok(false)
    }
    fn press_once(&mut self) -> KeyDispatch {
        let check = || self.before_key();
        let keys = match self.keys.guarded(self.fence, &check) {
            Ok(k) => k,
            Err(e) => return KeyDispatch::NotQueued(e),
        };
        // The producer preallocates both events, rechecks before down, and
        // unconditionally releases. A possibly posted Escape is never retried.
        keys.press_once(Key::Escape)
    }
    fn finish(&mut self, sent: bool) -> Result<Value> {
        finish_readback(
            self.fence.until,
            sent,
            || self.fence.input_check(),
            || self.readback(),
            || self.keys.confirm_input_idle(),
            Instant::now,
            thread::sleep,
        )?;
        Ok(
            json!({"building_id":self.request.key,"selection_inactive":true,
            "ordinary_key_presses":usize::from(sent),"same_attempt_readback":true,
            "complete_queue_unchanged":true,"cursor_moves":0,"screenshots":0,
            "panel_may_close":sent,"ordinary_gui_invalidation_possible":sent,
            "game_paused_by_action":false}),
        )
    }
}
pub(super) fn run(
    enabled: bool,
    request: Request<'_>,
    epoch: &Epoch,
    journal: &mut GameJournal,
    fence: &Fence<'_>,
    keys: &MacKeys<'_>,
) -> Result<Value> {
    let action = request.action();
    if let Some(receipt) = historical_receipt(&request, epoch, journal)? {
        return Ok(receipt);
    }
    if !enabled {
        return Err(Error::new(
            "capability_unavailable",
            "Construction cancellation qualification is disabled",
        ));
    }
    fence.input_check()?;
    let selection = read_recovery::observe(fence, "initial_clear_selection", || {
        mac_ui::bind_selection(fence, request.key, request.state_id)
    })?;
    let anchor = selection.clear_selection_anchor()?;
    let ui = if anchor.initially_inactive() {
        None
    } else {
        Some(retained_read(
            || selection.require_unchanged(&selection.observe(fence)?),
            || {
                read_recovery::observe(fence, "initial_clear_escape", || {
                    CancelAnchor::bind(fence, &selection)
                })
            },
        )?)
    };
    let baseline = json!({"selection":anchor.private_baseline()?,
        "escape_binding":ui.as_ref().map(CancelAnchor::private_baseline).transpose()?});
    let mut live = Live {
        request: &request,
        anchor: &anchor,
        ui: ui.as_ref(),
        fence,
        keys,
    };
    let receipt = journal.execute_strict(request.id, epoch, action, baseline, &mut || {
        dispatch(&mut live)
    })?;
    Ok(json!({"receipt":receipt,"historical_receipt":false,"current_selection_verified":true}))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_construction_clear_selection","description":"Default-off Mac qualification: clear the exact original ordinary construction selection with at most one unmodified Escape down/up on its already-open idle own construction panel. Retains original own/campaign/catalog/full queue and current typed keyboard receiver/callback; no mouse, navigation, repeated Escape, input retry, pause or worker. An initially inactive selection receives no key. Ordinary panel closing and GUI invalidation may occur. Only same-attempt canonical inactive selection with the complete queue unchanged verifies this action. Identical successful replays are historical. Requires construction_cancellation=true and an immutable qualification deadline; full campaign acceptance remains false.","inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string","maxLength":96},"lease_token":{"type":"string"}},"required":["building_id","state_id","request_id","lease_token"],"additionalProperties":false}})
}
#[cfg(test)]
mod tests;
