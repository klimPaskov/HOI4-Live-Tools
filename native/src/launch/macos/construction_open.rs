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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Ordinary,
    AlreadyOpenOnly,
}
impl Mode {
    pub(super) fn parse(value: Option<&Value>) -> Result<Self> {
        match value {
            None | Some(Value::Bool(false)) => Ok(Self::Ordinary),
            Some(Value::Bool(true)) => Ok(Self::AlreadyOpenOnly),
            _ => Err(Error::new(
                "invalid_arguments",
                "already_open_only must be a boolean",
            )),
        }
    }
    fn action(self) -> Value {
        match self {
            Self::Ordinary => {
                // Preserve the exact original action identity for old receipts.
                json!({"kind":"construction_panel_open","transport":"ordinary_t"})
            }
            Self::AlreadyOpenOnly => {
                json!({"kind":"construction_panel_open","transport":"none","already_open_only":true})
            }
        }
    }
    fn require_initial(self, selected: bool, opened: bool) -> Result<()> {
        if self == Self::AlreadyOpenOnly && !(selected && opened) {
            return Err(not_open());
        }
        Ok(())
    }
}
fn not_open() -> Error {
    Error::new(
        "construction_panel_not_open",
        "already_open_only requires an open own panel and cannot send a key",
    )
}

trait Attempt {
    /// True means already selected: wait without ever toggling it.
    fn prepare(&mut self) -> Result<bool>;
    fn press_once(&mut self) -> KeyDispatch;
    fn finish(&mut self, sent: bool) -> Result<Value>;
}
fn dispatch(attempt: &mut impl Attempt, mode: Mode) -> StrictOutcome {
    let already_selected = match attempt.prepare() {
        Ok(value) => value,
        Err(e) => return StrictOutcome::NotQueued(e),
    };
    let sent = !already_selected;
    // Enforce the zero-input contract at the dispatch boundary as well as the
    // initial observation. A changed/prepared closed panel must never enable T.
    if sent && mode == Mode::AlreadyOpenOnly {
        return StrictOutcome::NotQueued(not_open());
    }
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
    mode: Mode,
    id: &str,
    epoch: &Epoch,
    journal: &mut GameJournal,
    fence: &Fence<'_>,
    keys: &MacKeys<'_>,
) -> Result<Value> {
    let action = mode.action();
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
        mode.require_initial(before.selected(), before.opened())?;
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
    let receipt = journal.execute_strict(id, epoch, action, baseline, &mut || {
        dispatch(&mut live, mode)
    })?;
    Ok(json!({"receipt":receipt,"historical_receipt":false,"current_panel_verified":true}))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_construction_open","description":"Default-off Mac qualification: open the own construction panel with at most one ordinary T down/up, using fresh typed named-button/current-shortcut checks and a durable original intent. An already-selected panel receives no key. Set already_open_only=true to require an initially open panel and prohibit any keypress even if it changes before dispatch; this mode has a distinct durable action identity. No cursor movement, menu dismissal, placement, pause or input retry. Uncertain attempts permanently quarantine this controller; later current UI cannot settle them. Verified replays are explicitly historical. Requires input.json construction_panel=true and the exclusive lease. One ordinary opener has verified live delivery; broader campaign use remains unqualified.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string","maxLength":96},"lease_token":{"type":"string"},"already_open_only":{"type":"boolean","default":false}},"required":["request_id","lease_token"],"additionalProperties":false}})
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
                let result = dispatch(&mut f, Mode::Ordinary);
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

    #[test]
    fn already_open_mode_requires_a_boolean_and_defaults_to_ordinary() {
        assert_eq!(Mode::parse(None).unwrap(), Mode::Ordinary);
        assert_eq!(Mode::parse(Some(&json!(false))).unwrap(), Mode::Ordinary);
        assert_eq!(
            Mode::parse(Some(&json!(true))).unwrap(),
            Mode::AlreadyOpenOnly
        );
        for value in [
            Value::Null,
            json!(0),
            json!(1),
            json!("true"),
            json!([]),
            json!({}),
        ] {
            assert_eq!(
                Mode::parse(Some(&value)).unwrap_err().code,
                "invalid_arguments"
            );
        }
    }

    #[test]
    fn already_open_mode_requires_selected_visible_idle_initial_state() {
        for selected in [false, true] {
            for opened in [false, true] {
                assert!(Mode::Ordinary.require_initial(selected, opened).is_ok());
                let result = Mode::AlreadyOpenOnly.require_initial(selected, opened);
                if selected && opened {
                    result.unwrap();
                } else {
                    assert_eq!(result.unwrap_err().code, "construction_panel_not_open");
                }
            }
        }
    }

    #[test]
    fn already_open_mode_never_reaches_key_producer_after_prepare_changes() {
        for selected in [false, true] {
            for failure in 0..=4 {
                let mut f = Fake {
                    selected,
                    failure,
                    presses: 0,
                    reads: 0,
                };
                let result = dispatch(&mut f, Mode::AlreadyOpenOnly);
                assert_eq!(f.presses, 0);
                assert_eq!(f.reads, usize::from(selected && failure != 1));
                match result {
                    StrictOutcome::Verified(_) => assert!(selected && ![1, 3].contains(&failure)),
                    StrictOutcome::NotQueued(error) => {
                        assert!(!selected || [1, 3].contains(&failure));
                        if !selected && failure != 1 {
                            assert_eq!(error.code, "construction_panel_not_open");
                        }
                    }
                    StrictOutcome::Uncertain(_) => panic!("Zero-input mode cannot dispatch"),
                }
            }
        }
    }

    #[test]
    fn ordinary_action_identity_is_preserved_and_zero_input_identity_is_distinct() {
        assert_eq!(
            Mode::Ordinary.action(),
            json!({"kind":"construction_panel_open","transport":"ordinary_t"})
        );
        assert_eq!(
            Mode::AlreadyOpenOnly.action(),
            json!({"kind":"construction_panel_open","transport":"none","already_open_only":true})
        );
        assert_ne!(Mode::Ordinary.action(), Mode::AlreadyOpenOnly.action());
    }

    #[test]
    fn already_open_mode_is_optional_in_the_published_tool_contract() {
        let schema = tool()["inputSchema"].clone();
        assert_eq!(
            schema["properties"]["already_open_only"],
            json!({"type":"boolean","default":false})
        );
        assert_eq!(schema["required"], json!(["request_id", "lease_token"]));
        assert_eq!(schema["additionalProperties"], false);
    }
}
