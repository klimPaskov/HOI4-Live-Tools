//! Default-off qualification for one ordinary named Find button click.
use super::{
    action_fence::Fence,
    construction_open::read_recovery,
    journal::{GameJournal, StrictOutcome},
};
use crate::{
    actions::Epoch,
    construction::{
        mac_find::opener::{OpenAnchor, ReadyProof},
        mac_ui::{self, SelectionAnchor},
    },
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
        json!({"kind":"construction_find_open","building_id":self.key,"state_id":self.state_id,"transport":"ordinary_left_click"})
    }
}
pub(super) fn historical_receipt(
    request: &Request<'_>,
    epoch: &Epoch,
    journal: &GameJournal,
) -> Result<Option<Value>> {
    Ok(journal.strict_receipt(request.id,epoch,&request.action())?.map(|receipt|json!({"receipt":receipt,"historical_receipt":true,"current_find_verified":false})))
}
trait Attempt {
    /// True only if the ORIGINAL Find was already ready and remains ready.
    fn prepare(&mut self) -> Result<bool>;
    fn click_once(&mut self) -> Result<()>;
    fn finish(&mut self, sent: bool) -> Result<Value>;
}
fn dispatch(attempt: &mut impl Attempt) -> StrictOutcome {
    let already_ready = match attempt.prepare() {
        Ok(v) => v,
        Err(e) => return StrictOutcome::NotQueued(e),
    };
    let sent = !already_ready;
    if sent {
        if let Err(error) = attempt.click_once() {
            // Result may follow a posted move even when mouse-down was refused.
            return StrictOutcome::Uncertain(error);
        }
    }
    match attempt.finish(sent) {
        Ok(v) => StrictOutcome::Verified(v),
        Err(e) if sent => StrictOutcome::Uncertain(e),
        Err(e) => StrictOutcome::NotQueued(e),
    }
}

fn changed(s: &str) -> Error {
    Error::new("construction_find_open_changed", s)
}
fn retained_read<T>(
    mut guard: impl FnMut() -> Result<()>,
    read: impl FnOnce() -> Result<T>,
) -> Result<T> {
    guard()?;
    let v = read();
    let after = guard();
    match v {
        Err(e) => Err(e),
        Ok(v) => {
            after?;
            Ok(v)
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn finish_readback<T>(
    until: Instant,
    sent: bool,
    mut check: impl FnMut() -> Result<()>,
    mut observe: impl FnMut() -> Result<Option<T>>,
    mut platform: impl FnMut() -> Result<()>,
    mut final_read: impl FnMut(&T) -> Result<()>,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(Duration),
) -> Result<()> {
    let boundary = |check: &mut dyn FnMut() -> Result<()>, now: &mut dyn FnMut() -> Instant| {
        if now() >= until {
            return Err(Error::new(
                "action_deadline",
                "Original click opener deadline expired",
            ));
        }
        check()?;
        if now() >= until {
            return Err(Error::new(
                "action_deadline",
                "Original click opener deadline expired",
            ));
        }
        Ok(())
    };
    loop {
        boundary(&mut check, &mut now)?;
        let proof = observe()?;
        boundary(&mut check, &mut now)?;
        if let Some(proof) = proof {
            platform()?;
            boundary(&mut check, &mut now)?;
            final_read(&proof)?;
            boundary(&mut check, &mut now)?;
            return Ok(());
        }
        if !sent {
            return Err(changed("Originally ready Find lost readiness"));
        }
        boundary(&mut check, &mut now)?;
        let left = until.saturating_duration_since(now());
        if left.is_zero() {
            return Err(Error::new(
                "action_deadline",
                "Original click opener deadline expired",
            ));
        }
        wait(left.min(Duration::from_millis(10)));
        boundary(&mut check, &mut now)?;
    }
}
/// Resident read-only readiness wait after the one ordinary move. No input is
/// posted here. Only the explicit correlation-pending error may wait, and the
/// final check must again be positive after platform validation.
#[allow(clippy::too_many_arguments)]
fn await_route(
    until: Instant,
    check: impl FnMut() -> Result<()>,
    mut observe: impl FnMut() -> Result<()>,
    platform: impl FnMut() -> Result<()>,
    mut final_read: impl FnMut() -> Result<()>,
    now: impl FnMut() -> Instant,
    wait: impl FnMut(Duration),
) -> Result<()> {
    finish_readback(
        until,
        true,
        check,
        || match observe() {
            Ok(()) => Ok(Some(())),
            Err(e) if e.code == "construction_route_pending" => Ok(None),
            Err(e) => Err(e),
        },
        platform,
        |_| final_read(),
        now,
        wait,
    )
}
struct Live<'a, 'b> {
    request: &'a Request<'a>,
    selection: &'a SelectionAnchor,
    anchor: &'a OpenAnchor,
    fence: &'a Fence<'b>,
    keys: &'a MacKeys<'b>,
}
impl Live<'_, '_> {
    fn route(&self) -> Result<()> {
        let (x, y) = self.anchor.control_point()?;
        self.keys.confirm_input_idle()?;
        self.keys.confirm_cursor_at((1920, 1080), x, y)?;
        retained_read(
            || self.queue(),
            || {
                read_recovery::observe(self.fence, "find_open_route", || {
                    self.anchor.require_before_down(self.fence)
                })
            },
        )
    }
    fn queue(&self) -> Result<()> {
        read_recovery::observe(self.fence, "find_open_queue", || {
            self.selection
                .require_unchanged(&self.selection.observe(self.fence)?)
        })
    }
    fn before_move(&self) -> Result<()> {
        retained_read(
            || self.queue(),
            || {
                read_recovery::observe(self.fence, "find_open_control", || {
                    self.anchor.require_before_move(self.fence)
                })
            },
        )
    }
    fn ready(&self) -> Result<Option<ReadyProof>> {
        retained_read(
            || self.queue(),
            || {
                read_recovery::observe(self.fence, "find_open_readback", || {
                    self.anchor.observe_ready(self.fence)
                })
            },
        )
    }
    fn final_read(&self, proof: &ReadyProof) -> Result<()> {
        retained_read(
            || self.queue(),
            || {
                read_recovery::observe(self.fence, "find_open_final_control", || {
                    proof.reobserve(self.fence, self.anchor)
                })
            },
        )
    }
}
impl Attempt for Live<'_, '_> {
    fn prepare(&mut self) -> Result<bool> {
        self.fence.input_check()?;
        self.queue()?;
        if self.anchor.initially_ready() {
            self.ready()?
                .ok_or_else(|| changed("Original ready Find changed"))?;
            return Ok(true);
        }
        self.before_move()?;
        self.keys.confirm_input_idle()?;
        self.fence.input_check()?;
        Ok(false)
    }
    fn click_once(&mut self) -> Result<()> {
        let (x, y) = self.anchor.control_point()?;
        let check = || self.before_move();
        let before_down = || {
            await_route(
                self.fence.until,
                || self.fence.input_check(),
                || self.route(),
                || {
                    self.keys.confirm_input_idle()?;
                    self.keys.confirm_cursor_at((1920, 1080), x, y)
                },
                || self.route(),
                Instant::now,
                thread::sleep,
            )
        };
        let guarded = self.keys.guarded(self.fence, &check)?;
        guarded.click_checked((1920, 1080), x, y, &before_down)?;
        Ok(())
    }
    fn finish(&mut self, sent: bool) -> Result<Value> {
        finish_readback(
            self.fence.until,
            sent,
            || self.fence.input_check(),
            || self.ready(),
            || self.keys.confirm_input_idle(),
            |proof| self.final_read(proof),
            Instant::now,
            thread::sleep,
        )?;
        Ok(
            json!({"building_id":self.request.key,"state_id":self.request.state_id,"find_visible":true,"empty_query":true,"exact_focused_edit":true,"ordinary_clicks":usize::from(sent),"ordinary_key_presses":0,"same_attempt_readback":true,"complete_queue_unchanged":true,"construction_selection_inactive":true,"cursor_moves":usize::from(sent),"screenshots":0,"game_paused_by_action":false}),
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
    if let Some(receipt) = historical_receipt(&request, epoch, journal)? {
        return Ok(receipt);
    }
    if !enabled {
        return Err(Error::new(
            "capability_unavailable",
            "Resident click opener qualification is disabled",
        ));
    }
    fence.input_check()?;
    let selection = read_recovery::observe(fence, "find_open_initial_queue", || {
        let s = mac_ui::bind_selection(fence, request.key, request.state_id)?;
        s.require_inactive()?;
        Ok(s)
    })?;
    let anchor = retained_read(
        || selection.require_unchanged(&selection.observe(fence)?),
        || {
            read_recovery::observe(fence, "find_open_initial_control", || {
                OpenAnchor::bind(fence, &selection, request.state_id)
            })
        },
    )?;
    let baseline = anchor.private_baseline()?;
    let mut live = Live {
        request: &request,
        selection: &selection,
        anchor: &anchor,
        fence,
        keys,
    };
    let receipt =
        journal.execute_strict(request.id, epoch, request.action(), baseline, &mut || {
            dispatch(&mut live)
        })?;
    Ok(json!({"receipt":receipt,"historical_receipt":false,"current_find_verified":true}))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_construction_find_open","description":"Default-off Mac qualification: open the materialized idle own Find view with at most one ordinary left click on its exact named MapModes button. Requires inactive construction selection and preserves the complete own queue. Retains original target, callback, native1920x1080 geometry and current typed mouse route; verifies an empty query and exact focused edit in the same attempt. An already-ready Find receives no input. No F, text, Return, screenshot, pause, worker or input retry. Successful replays are historical. Requires construction_find_open=true and its immutable qualification deadline; campaign use remains unqualified.","inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string","maxLength":96},"lease_token":{"type":"string"}},"required":["building_id","state_id","request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests;
