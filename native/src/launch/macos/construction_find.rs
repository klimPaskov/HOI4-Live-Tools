//! One decimal Find query and Return, on an already-open exact own Find box.
//! The original typed Find/queue identities and action deadline survive every
//! key. A possibly posted key is never repeated or settled by a later request.
use super::{
    action_fence::Fence,
    construction_open::read_recovery,
    journal::{GameJournal, StrictOutcome},
};
use crate::{
    actions::Epoch,
    construction::{mac_find::navigation, mac_ui},
    error::{Error, Result},
    platform::{
        key_once::Dispatch,
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
        json!({"kind":"construction_find_state","building_id":self.key,
            "state_id":self.state_id,"already_open_only":true,
            "transport":"ordinary_decimal_digits_then_return"})
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Digit(char),
    Return,
}

trait Attempt {
    fn query(&self) -> &str;
    fn prepare(&mut self) -> Result<()>;
    fn before(&mut self, prefix: &str, step: Step) -> Result<()>;
    fn post_once(&mut self, prefix: &str, step: Step) -> Dispatch;
    fn finish(&mut self) -> Result<Value>;
}

fn refusal(error: Error, possible: bool) -> StrictOutcome {
    if possible {
        StrictOutcome::Uncertain(error)
    } else {
        StrictOutcome::NotQueued(error)
    }
}

fn dispatch(attempt: &mut impl Attempt) -> StrictOutcome {
    if let Err(error) = attempt.prepare() {
        return StrictOutcome::NotQueued(error);
    }
    let query = attempt.query().to_string();
    if query.is_empty() || query.len() > 4 || !query.bytes().all(|v| v.is_ascii_digit()) {
        return StrictOutcome::NotQueued(changed("Bounded decimal Find query required"));
    }
    let mut possible = false;
    let mut prefix = String::new();
    for step in query.chars().map(Step::Digit).chain([Step::Return]) {
        if let Err(error) = attempt.before(&prefix, step) {
            return refusal(error, possible);
        }
        match attempt.post_once(&prefix, step) {
            Dispatch::NotQueued(error) => return refusal(error, possible),
            Dispatch::Unknown(Some(error)) => return StrictOutcome::Uncertain(error),
            Dispatch::Unknown(None) => possible = true,
        }
        if let Step::Digit(digit) = step {
            prefix.push(digit);
        }
    }
    match attempt.finish() {
        Ok(detail) => StrictOutcome::Verified(detail),
        Err(error) => refusal(error, possible),
    }
}

fn changed(message: &str) -> Error {
    Error::new("construction_find_changed", message)
}

fn guarded_read<T>(
    mut guard: impl FnMut() -> Result<()>,
    read: impl FnOnce() -> Result<T>,
) -> Result<T> {
    guard()?;
    let observed = read();
    let after = guard();
    // Always execute the post-read guard. Preserve an original read refusal;
    // an otherwise-positive result cannot escape a changed queue/selection.
    match observed {
        Err(error) => Err(error),
        Ok(value) => {
            after?;
            Ok(value)
        }
    }
}

struct Live<'a, 'b> {
    state_id: i32,
    nav: &'a navigation::NavigationAnchor,
    selection: &'a mac_ui::SelectionAnchor,
    fence: &'a Fence<'b>,
    keys: &'a MacKeys<'b>,
}
impl Live<'_, '_> {
    fn queue_unchanged(&self) -> Result<()> {
        read_recovery::observe(self.fence, "find_queue", || {
            self.selection
                .require_unchanged(&self.selection.observe(self.fence)?)
        })
    }
    fn prefix(&self, prefix: &str) -> Result<Option<navigation::PrefixProof>> {
        guarded_read(
            || self.queue_unchanged(),
            || {
                read_recovery::observe(self.fence, "find_prefix", || {
                    self.nav.observe_prefix(self.fence, prefix)
                })
            },
        )
    }
    fn returning(&self) -> Result<Option<navigation::ReturnProof>> {
        guarded_read(
            || self.queue_unchanged(),
            || {
                read_recovery::observe(self.fence, "find_return", || {
                    self.nav.observe_return(self.fence)
                })
            },
        )
    }
    fn finished(&self) -> Result<Option<navigation::FinishedProof>> {
        guarded_read(
            || self.queue_unchanged(),
            || {
                read_recovery::observe(self.fence, "find_camera", || {
                    self.nav.observe_finished(self.fence)
                })
            },
        )
    }
    fn wait<T>(&self, mut read: impl FnMut() -> Result<Option<T>>) -> Result<T> {
        loop {
            self.fence.input_check()?;
            if let Some(proof) = read()? {
                return Ok(proof);
            }
            thread::sleep(
                Duration::from_millis(10)
                    .min(self.fence.until.saturating_duration_since(Instant::now())),
            );
        }
    }
    fn immediate(&self, prefix: &str, step: Step) -> Result<()> {
        self.fence.input_check()?;
        match step {
            Step::Digit(digit) => {
                let proof = self
                    .prefix(prefix)?
                    .ok_or_else(|| changed("Original Find prefix not ready"))?;
                if proof.next_digit() != Some(digit) {
                    return Err(changed(
                        "Only the next original decimal digit may be posted",
                    ));
                }
            }
            Step::Return => {
                if self.returning()?.is_none() {
                    return Err(changed(
                        "Original requested Find state is not the current first row",
                    ));
                }
            }
        }
        self.fence.input_check()
    }
}
impl Attempt for Live<'_, '_> {
    fn query(&self) -> &str {
        self.nav.decimal_query()
    }
    fn prepare(&mut self) -> Result<()> {
        self.selection.require_inactive()?;
        self.fence.input_check()?;
        self.immediate(
            "",
            Step::Digit(
                self.query()
                    .chars()
                    .next()
                    .ok_or_else(|| changed("Empty Find target"))?,
            ),
        )?;
        self.keys.confirm_input_idle()
    }
    fn before(&mut self, prefix: &str, step: Step) -> Result<()> {
        self.wait(|| self.prefix(prefix))?;
        if step == Step::Return {
            self.wait(|| self.returning())?;
        }
        self.immediate(prefix, step)
    }
    fn post_once(&mut self, prefix: &str, step: Step) -> Dispatch {
        let check = || self.immediate(prefix, step);
        let guarded = match self.keys.guarded(self.fence, &check) {
            Ok(keys) => keys,
            Err(error) => return Dispatch::NotQueued(error),
        };
        match step {
            Step::Digit(digit) => guarded.press_digit_once(digit),
            Step::Return => guarded.press_once(Key::Return),
        }
    }
    fn finish(&mut self) -> Result<Value> {
        self.wait(|| self.finished())?;
        self.keys.confirm_input_idle()?;
        if self.finished()?.is_none() {
            return Err(changed(
                "Find camera verification changed after platform checks",
            ));
        }
        self.fence.input_check()?;
        Ok(json!({"camera_centered_state_id":self.state_id,
            "ordinary_keys":self.query().len()+1,"cursor_moves":0,"screenshots":0,
            "same_attempt_readback":true,"complete_queue_unchanged":true,
            "selection_unchanged":true,"construction_placed":false,
            "placement_point_verified":false,"game_paused_by_action":false}))
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
            json!({"receipt":receipt,"historical_receipt":true,"current_camera_verified":false}),
        );
    }
    if !enabled {
        return Err(Error::new(
            "capability_unavailable",
            "Mac construction_navigation qualification permission is disabled",
        ));
    }
    fence.input_check()?;
    let selection = read_recovery::observe(fence, "find_initial_queue", || {
        let selection = mac_ui::bind_selection(fence, request.key, request.state_id)?;
        selection.require_inactive()?;
        Ok(selection)
    })?;
    let nav = read_recovery::observe(fence, "find_initial_target", || {
        navigation::bind(fence, request.state_id)
    })?;
    if selection.context().anchors != nav.find_anchor().context().anchors
        || selection.context().playthrough_id != nav.find_anchor().context().playthrough_id
        || selection.context().player != nav.find_anchor().context().player
    {
        return Err(changed("Find and construction campaign scopes differ"));
    }
    selection.require_unchanged(&selection.observe(fence)?)?;
    let baseline =
        json!({"find":nav.private_baseline()?,"construction":selection.private_baseline()?});
    let mut live = Live {
        state_id: request.state_id,
        nav: &nav,
        selection: &selection,
        fence,
        keys,
    };
    let receipt = journal.execute_strict(request.id, epoch, action, baseline, &mut || {
        dispatch(&mut live)
    })?;
    Ok(json!({"receipt":receipt,"historical_receipt":false,"current_camera_verified":true}))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_construction_find_state","description":"Default-off Mac qualification: on an already-open, idle, exact focused Find box with empty query and inactive construction selection, type one own state ID and Return once. Retains original Find/control/state identity, full own construction queue and selection before each key; verifies the same requested camera anchor after Find closes. No F/opening, cursor movement, screenshot, pause, placement, worker or input retry. Possible delivery remains uncertain unless this same attempt verifies it; replays are historical. Requires construction_navigation=true and an exclusive input lease. Campaign use remains unqualified.","inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string","maxLength":96},"lease_token":{"type":"string"}},"required":["building_id","state_id","request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
#[path = "construction_find/tests.rs"]
mod tests;
