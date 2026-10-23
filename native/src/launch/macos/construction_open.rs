//! Default-off qualification of one ordinary T opener. The observer reports
//! current GUI routing, not guaranteed physical delivery; only this attempt's
//! fresh reached-panel observation can produce a verified receipt.
use super::{
    action_fence::Fence,
    journal::{GameJournal, StrictOutcome},
};
use crate::{
    actions::Epoch,
    construction::mac_panel::{self, Observation},
    error::{Error, Result},
    platform::{
        key_once::Dispatch as KeyDispatch,
        macos_input::{Key, MacKeys},
    },
};
use serde_json::{json, Value};
use std::{thread, time::Duration};

#[cfg(test)]
#[path = "construction_open/delivery_tests.rs"]
mod delivery_tests;

mod read_recovery;

trait Attempt {
    /// True means already selected: wait without ever toggling it.
    fn prepare(&mut self) -> Result<bool>;
    fn press_once(&mut self) -> KeyDispatch;
    fn finish(&mut self, sent: bool) -> Result<Value>;
}
fn dispatch(attempt: &mut impl Attempt) -> StrictOutcome {
    let already_selected = match attempt.prepare() {
        Ok(value) => value,
        Err(e) => return StrictOutcome::NotQueued(e),
    };
    let sent = !already_selected;
    if sent {
        match attempt.press_once() {
            KeyDispatch::NotQueued(e) => return StrictOutcome::NotQueued(e),
            KeyDispatch::Unknown(Some(e)) => return StrictOutcome::Uncertain(e),
            // Successful posting is unacknowledged. Only this original attempt's
            // fresh panel readback can verify the durable action.
            KeyDispatch::Unknown(None) => {}
        }
    }
    match attempt.finish(sent) {
        Ok(detail) => StrictOutcome::Verified(detail),
        Err(e) if sent => StrictOutcome::Uncertain(e),
        Err(e) => StrictOutcome::NotQueued(e),
    }
}

struct Live<'a, 'b> {
    before: &'a Observation,
    fence: &'a Fence<'b>,
    keys: &'a MacKeys<'b>,
}
impl Live<'_, '_> {
    fn same_before(&self) -> Result<()> {
        read_recovery::observe(self.fence, "before_key", || {
            let now = mac_panel::observe_open(self.fence)?;
            if !self.before.same_before_input(&now) {
                return Err(Error::new(
                    "construction_opener_changed",
                    "Original panel/shortcut binding changed before input",
                ));
            }
            now.require_shortcut()
        })
    }
}
impl Attempt for Live<'_, '_> {
    fn prepare(&mut self) -> Result<bool> {
        self.fence.input_check()?;
        if self.before.selected() {
            read_recovery::observe(self.fence, "already_selected", || {
                let now = mac_panel::observe_open(self.fence)?;
                if !self.before.same_panel(&now) || !now.selected() {
                    return Err(Error::new(
                        "construction_opener_changed",
                        "Already-selected panel binding changed",
                    ));
                }
                Ok(())
            })?;
            return Ok(true);
        }
        self.same_before()?;
        self.keys.confirm_input_idle()?;
        Ok(false)
    }
    fn press_once(&mut self) -> KeyDispatch {
        let check = || self.same_before();
        let keys = match self.keys.guarded(self.fence, &check) {
            Ok(keys) => keys,
            Err(e) => return KeyDispatch::NotQueued(e),
        };
        keys.press_once(Key::Letter('t'))
    }
    fn finish(&mut self, sent: bool) -> Result<Value> {
        loop {
            let now = read_recovery::observe(self.fence, "panel_readback", || {
                let now = mac_panel::observe_open(self.fence)?;
                if !self.before.same_panel(&now) {
                    return Err(Error::new(
                        "construction_opener_changed",
                        "Original panel ownership changed during readback",
                    ));
                }
                Ok(now)
            })?;
            if now.opened() {
                self.keys.confirm_input_idle()?;
                // Geometry/idle checks may take time; certify a fresh final panel
                // under the same original scope and deadline after those checks.
                return read_recovery::observe(self.fence, "final_panel_readback", || {
                    let final_state = mac_panel::observe_open(self.fence)?;
                    if !self.before.same_panel(&final_state) || !final_state.opened() {
                        return Err(Error::new(
                            "construction_opener_changed",
                            "Panel changed during final verification",
                        ));
                    }
                    Ok(
                        json!({"panel_open":true,"ordinary_key_presses":usize::from(sent),
                        "same_attempt_readback":true,"cursor_moved":false,"game_paused_by_action":false}),
                    )
                });
            }
            thread::sleep(
                Duration::from_millis(10).min(
                    self.fence
                        .until
                        .saturating_duration_since(std::time::Instant::now()),
                ),
            );
        }
    }
}

pub(super) fn run(
    enabled: bool,
    id: &str,
    epoch: &Epoch,
    journal: &mut GameJournal,
    fence: &Fence<'_>,
    keys: &MacKeys<'_>,
) -> Result<Value> {
    let action = json!({"kind":"construction_panel_open","transport":"ordinary_t"});
    if let Some(receipt) = journal.strict_receipt(id, epoch, &action)? {
        return Ok(
            json!({"receipt":receipt,"historical_receipt":true,"current_panel_verified":false}),
        );
    }
    if !enabled {
        return Err(Error::new(
            "capability_unavailable",
            "Mac construction_panel qualification permission is disabled",
        ));
    }
    fence.input_check()?;
    let before = read_recovery::observe(fence, "initial_binding", || {
        let before = mac_panel::observe_open(fence)?;
        if !before.selected() {
            before.require_shortcut()?;
        }
        Ok(before)
    })?;
    let baseline = before.private_baseline()?;
    let check = || fence.input_check();
    let keys = keys.guarded(fence, &check)?;
    let mut live = Live {
        before: &before,
        fence,
        keys: &keys,
    };
    let receipt =
        journal.execute_strict(id, epoch, action, baseline, &mut || dispatch(&mut live))?;
    Ok(json!({"receipt":receipt,"historical_receipt":false,"current_panel_verified":true}))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_construction_open","description":"Default-off Mac qualification: open the own construction panel with at most one ordinary T down/up, using fresh typed named-button/current-shortcut checks and a durable original intent. An already-selected panel receives no key. No cursor movement, menu dismissal, placement, pause or input retry. Uncertain attempts permanently quarantine this controller; later current UI cannot settle them. Verified replays are explicitly historical. Requires input.json construction_panel=true and the exclusive lease. Physical delivery and campaign use remain unqualified.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string","maxLength":96},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        selected: bool,
        failure: u8,
        presses: usize,
        reads: usize,
    }
    fn fail() -> Error {
        Error::new("torn_read", "fixture failure")
    }
    impl Attempt for Fake {
        fn prepare(&mut self) -> Result<bool> {
            if self.failure == 1 {
                Err(fail())
            } else {
                Ok(self.selected)
            }
        }
        fn press_once(&mut self) -> KeyDispatch {
            self.presses += 1;
            match self.failure {
                2 => KeyDispatch::Unknown(Some(fail())),
                4 => KeyDispatch::NotQueued(fail()),
                _ => KeyDispatch::Unknown(None),
            }
        }
        fn finish(&mut self, sent: bool) -> Result<Value> {
            self.reads += 1;
            assert_eq!(sent, !self.selected);
            if self.failure == 3 {
                Err(fail())
            } else {
                Ok(json!({"opened":true,"sent":sent}))
            }
        }
    }
    #[test]
    fn fresh_attempt_sends_at_most_one_key_and_never_toggles_selected_panels() {
        for selected in [false, true] {
            for failure in 0..=4 {
                let mut f = Fake {
                    selected,
                    failure,
                    presses: 0,
                    reads: 0,
                };
                let result = dispatch(&mut f);
                assert_eq!(f.presses, usize::from(!selected && failure != 1));
                assert_eq!(
                    f.reads,
                    usize::from(failure != 1 && (selected || ![2, 4].contains(&failure)))
                );
                match result {
                    StrictOutcome::Verified(_) => {
                        assert!(failure == 0 || (selected && [2, 4].contains(&failure)))
                    }
                    StrictOutcome::NotQueued(_) => {
                        assert!([1, 4].contains(&failure) || (selected && failure == 3))
                    }
                    StrictOutcome::Uncertain(_) => assert!(!selected && [2, 3].contains(&failure)),
                }
            }
        }
    }
}
