//! One ordinary selection attempt, retaining its original typed target across
//! movement, click and readback. Selection is not state targeting or placement.
use super::{
    action_fence::Fence,
    construction_open::read_recovery,
    journal::{GameJournal, StrictOutcome},
};
use crate::{
    actions::Epoch,
    construction::mac_ui::{self, Observation, SelectionAnchor},
    error::{Error, Result},
    platform::macos_input::MacKeys,
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
        json!({"kind":"construction_select","building_id":self.key,"state_id":self.state_id,"transport":"ordinary_left_click"})
    }
}
trait Attempt {
    /// Read-only preparation. True means the original target is selected.
    fn prepare(&mut self) -> Result<bool>;
    /// At most one move/down/up. Even an error can follow a posted move.
    fn click_once(&mut self) -> Result<()>;
    fn finish(&mut self, clicked: bool) -> Result<Value>;
}
fn dispatch(attempt: &mut impl Attempt) -> StrictOutcome {
    let selected = match attempt.prepare() {
        Ok(value) => value,
        Err(error) => return StrictOutcome::NotQueued(error),
    };
    let clicked = !selected;
    if clicked {
        if let Err(error) = attempt.click_once() {
            return StrictOutcome::Uncertain(error);
        }
    }
    match attempt.finish(clicked) {
        Ok(detail) => StrictOutcome::Verified(detail),
        Err(error) if clicked => StrictOutcome::Uncertain(error),
        Err(error) => StrictOutcome::NotQueued(error),
    }
}

struct Live<'a, 'b> {
    request: &'a Request<'a>,
    anchor: &'a SelectionAnchor,
    before: Option<&'a Observation>,
    fence: &'a Fence<'b>,
    // Always retain the original unguarded keys for idle/cursor observations;
    // calling them on a view guarded by this same closure would recurse.
    keys: &'a MacKeys<'b>,
}
impl Live<'_, '_> {
    fn observe_same_choice(&self, require_route: bool) -> Result<()> {
        read_recovery::observe(self.fence, "selection_control", || {
            let now = mac_ui::observe_choice(
                self.fence,
                self.request.key,
                self.request.state_id,
                require_route,
            )?;
            self.before
                .ok_or_else(changed)?
                .require_same_choice_geometry(&now, require_route)
        })
    }
    fn selected(&self) -> Result<bool> {
        read_recovery::observe(self.fence, "selection_readback", || {
            let now = self.anchor.observe(self.fence)?;
            self.anchor.same_scope_and_queue(&now)?;
            Ok(self.anchor.is_selected(&now))
        })
    }
}
fn changed() -> Error {
    Error::new(
        "construction_selection_changed",
        "Original construction selection or requested control changed",
    )
}
impl Attempt for Live<'_, '_> {
    fn prepare(&mut self) -> Result<bool> {
        self.fence.input_check()?;
        if self.anchor.initially_selected() {
            if !self.selected()? {
                return Err(changed());
            }
            return Ok(true);
        }
        self.observe_same_choice(false)?;
        self.keys.confirm_input_idle()?;
        Ok(false)
    }
    fn click_once(&mut self) -> Result<()> {
        let (x, y) = self.before.ok_or_else(changed)?.selection_control_point()?;
        let check = || {
            self.fence.input_check()?;
            self.observe_same_choice(false)
        };
        let before_down = || {
            self.keys.confirm_input_idle()?;
            self.keys.confirm_cursor_at((1920, 1080), x, y)?;
            self.observe_same_choice(true)
        };
        let guarded = self.keys.guarded(self.fence, &check)?;
        guarded.click_checked((1920, 1080), x, y, &before_down)?;
        Ok(())
    }
    fn finish(&mut self, clicked: bool) -> Result<Value> {
        loop {
            if self.selected()? {
                self.keys.confirm_input_idle()?;
                // Certify after potentially slow platform checks, within the
                // same original lease/deadline and original canonical scope.
                if !self.selected()? {
                    return Err(changed());
                }
                return Ok(json!({"building_id":self.request.key,"selection_mode":1,
                    "ordinary_clicks":usize::from(clicked),"cursor_moves":usize::from(clicked),
                    "same_attempt_readback":true,"complete_queue_unchanged":true,
                    "state_selected":false,"construction_placed":false,"game_paused_by_action":false}));
            }
            if !clicked {
                return Err(changed());
            }
            thread::sleep(
                Duration::from_millis(10)
                    .min(self.fence.until.saturating_duration_since(Instant::now())),
            );
        }
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
    if let Some(receipt) = journal.strict_receipt(request.id, epoch, &action)? {
        return Ok(
            json!({"receipt":receipt,"historical_receipt":true,"current_selection_verified":false}),
        );
    }
    if !enabled {
        return Err(Error::new(
            "capability_unavailable",
            "Mac construction_selection qualification permission is disabled",
        ));
    }
    fence.input_check()?;
    let anchor = read_recovery::observe(fence, "initial_selection", || {
        mac_ui::bind_selection(fence, request.key, request.state_id)
    })?;
    let before = if anchor.initially_selected() {
        None
    } else {
        Some(read_recovery::observe(
            fence,
            "initial_selection_control",
            || {
                let before = mac_ui::observe_choice(fence, request.key, request.state_id, false)?;
                before.selection_control_point()?;
                // The initial control read may not silently replace the original
                // scope/queue/selection acquired by the lightweight anchor.
                let other = before.selection_anchor()?;
                anchor.require_same_initial(&other)?;
                Ok(before)
            },
        )?)
    };
    let baseline = anchor.private_baseline()?;
    let mut live = Live {
        request: &request,
        anchor: &anchor,
        before: before.as_ref(),
        fence,
        keys,
    };
    let receipt = journal.execute_strict(request.id, epoch, action, baseline, &mut || {
        dispatch(&mut live)
    })?;
    Ok(json!({"receipt":receipt,"historical_receipt":false,"current_selection_verified":true}))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_construction_select","description":"Default-off Mac qualification: select one own ordinary building template on an already-open construction panel using at most one unmodified cursor move and left click. Revalidates original callback/geometry and fresh target routing after cursor settle; verifies exact canonical mode1 selection with the complete own queue unchanged. An already-selected target uses no input. state_id binds an owned state for scope checking; this action does not select that state or place construction. No pause, screenshot, navigation, retry or later settlement of uncertain input. Verified replays are historical. Requires input.json construction_selection=true and exclusive lease; campaign use remains unqualified.","inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string","maxLength":96},"lease_token":{"type":"string"}},"required":["building_id","state_id","request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
#[path = "construction_select/tests.rs"]
mod tests;
