//! Default-off one-click placement qualification with typed recipient checks.
//! Terrain and cached SDL coordinates alone are not GUI admission.
//! No automatic opening, Find, selection, Escape, pause or placement retry.
use super::{
    action_fence::Fence,
    construction_open::read_recovery,
    journal::{GameJournal, StrictOutcome},
};
use crate::{
    actions::Epoch,
    construction::mac_ui::{self, PlacementAnchor, PlacementDelta},
    error::{Error, Result},
    platform::macos_input::MacKeys,
};
use serde_json::{json, Value};
use std::{
    thread,
    time::{Duration, Instant},
};

mod point;
pub(super) struct Request<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub state_id: i32,
}
impl Request<'_> {
    fn action(&self) -> Value {
        json!({"kind":"construction_place","building_id":self.key,"state_id":self.state_id,"amount":1,"transport":"ordinary_center_left_click"})
    }
}
trait Attempt {
    fn prepare(&mut self) -> Result<()>;
    /// Entered once. Even a pre-down error may follow the single pointer move.
    fn click_once(&mut self) -> Result<()>;
    fn readback(&mut self) -> Result<PlacementDelta>;
    fn platform_idle(&mut self) -> Result<()>;
    fn boundary(&mut self) -> Result<()>;
    fn route_diagnostic(&self) -> Value {
        Value::Null
    }
    fn until(&self) -> Instant;
    fn now(&self) -> Instant;
    fn wait(&mut self, duration: Duration);
}
fn changed(message: &str) -> Error {
    Error::new("construction_placement_changed", message)
}
fn require_pending(delta: PlacementDelta) -> Result<()> {
    if delta != PlacementDelta::Pending {
        return Err(changed("Original queue changed before placement input"));
    }
    Ok(())
}
fn delta_name(delta: PlacementDelta) -> &'static str {
    match delta {
        PlacementDelta::Pending => "pending",
        PlacementDelta::ExistingLineIncremented => "existing_line_incremented",
        PlacementDelta::NewLineAdded => "new_line_added",
    }
}
fn boundary(a: &mut impl Attempt) -> Result<()> {
    if a.now() >= a.until() {
        return Err(Error::new(
            "action_deadline",
            "Original placement deadline expired",
        ));
    }
    a.boundary()?;
    if a.now() >= a.until() {
        return Err(Error::new(
            "action_deadline",
            "Original placement deadline expired",
        ));
    }
    Ok(())
}
fn finish(a: &mut impl Attempt) -> Result<PlacementDelta> {
    loop {
        boundary(a)?;
        let observed = a.readback();
        let after = boundary(a);
        let delta = observed?;
        after?;
        if delta != PlacementDelta::Pending {
            a.platform_idle()?;
            boundary(a)?;
            let final_read = a.readback();
            let after = boundary(a);
            if final_read? != delta {
                return Err(changed("Placement delta changed after platform checks"));
            }
            after?;
            return Ok(delta);
        }
        a.wait(Duration::from_millis(10).min(a.until().saturating_duration_since(a.now())));
        boundary(a)?;
    }
}
fn dispatch(a: &mut impl Attempt) -> StrictOutcome {
    if let Err(error) = boundary(a)
        .and_then(|_| a.prepare())
        .and_then(|_| boundary(a))
    {
        return StrictOutcome::NotQueued(error);
    }
    // No subsequent branch can classify this request as NotQueued. The real
    // transport may already have moved the cursor before its down guard fails.
    if let Err(error) = a.click_once() {
        return StrictOutcome::Uncertain(error);
    }
    match finish(a) {
        Ok(delta) => StrictOutcome::Verified(json!({"delta":delta_name(delta),"amount_added":1,
            "ordinary_clicks":1,"cursor_moves":1,"same_attempt_readback":true,
            "all_original_rows_preserved":true,"complete_queue_verified":true,
            "full_picker_verified":false,"model_hit_verified":false,
            "current_event_verified":false,"eligibility_verified":false,"game_paused_by_action":false,"narrow_gui_route":a.route_diagnostic()})),
        Err(error) => StrictOutcome::Uncertain(error),
    }
}

/// A bounded cached-coordinate mismatch is pending; owner/focus/type errors
/// are refusals. Both the one-second subwindow and original Fence stay fixed.
fn correlate(
    until: Instant,
    mut check: impl FnMut() -> Result<()>,
    mut os: impl FnMut() -> Result<()>,
    mut source: impl FnMut() -> Result<[i32; 2]>,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(Duration),
) -> Result<()> {
    loop {
        if now() >= until {
            return Err(Error::new(
                "input_cursor_pending",
                "Original cursor correlation window expired",
            ));
        }
        check()?;
        os()?;
        let observed = source();
        let after = check();
        let position = observed?;
        after?;
        os()?;
        check()?;
        if now() >= until {
            return Err(Error::new(
                "input_cursor_pending",
                "Late cached cursor correlation discarded",
            ));
        }
        if position == [960, 540] {
            return Ok(());
        }
        wait(Duration::from_millis(2).min(until.saturating_duration_since(now())));
        check()?;
    }
}

struct Live<'a, 'b> {
    anchor: &'a PlacementAnchor,
    point: &'a point::Anchor,
    fence: &'a Fence<'b>,
    keys: &'a MacKeys<'b>,
    route: std::sync::Mutex<Value>,
}
impl Live<'_, '_> {
    fn pending(&self) -> Result<()> {
        read_recovery::observe(self.fence, "placement_original_queue", || {
            require_pending(self.anchor.observe(self.fence)?.delta())
        })
    }
    fn point(&self, check_scene: bool) -> Result<[i32; 2]> {
        read_recovery::observe(self.fence, "placement_original_point", || {
            self.point.observe(self.fence, check_scene)
        })
    }
    fn idle(&self) -> Result<()> {
        let pinned = self.point.pinned(self.fence);
        let check = || self.fence.input_check();
        self.keys.guarded(&pinned, &check)?.confirm_input_idle()?;
        self.fence.input_check()
    }
}
impl Attempt for Live<'_, '_> {
    fn prepare(&mut self) -> Result<()> {
        self.pending()?;
        self.point(true)?;
        self.idle()?;
        *self
            .route
            .lock()
            .map_err(|_| changed("Route diagnostic lock poisoned"))? =
            self.point.verify_route(self.fence)?;
        Ok(())
    }
    fn click_once(&mut self) -> Result<()> {
        self.pending()?;
        self.point(false)?;
        let pinned = self.point.pinned(self.fence);
        // Idle/cursor queries use a view with only the Fence boundary, avoiding
        // recursive calls through the later per-input queue guard.
        let check = || self.fence.input_check();
        let base = self.keys.guarded(&pinned, &check)?;
        let input = || {
            self.fence.input_check()?;
            self.pending()
        };
        let before_down = || {
            self.pending()?;
            self.point(true)?;
            base.confirm_input_idle()?;
            correlate(
                self.fence
                    .until
                    .min(Instant::now() + Duration::from_secs(1)),
                || self.fence.input_check(),
                || base.confirm_cursor_at((1920, 1080), 960., 540.),
                || {
                    self.pending()?;
                    self.point(false)
                },
                Instant::now,
                thread::sleep,
            )?;
            *self
                .route
                .lock()
                .map_err(|_| changed("Route diagnostic lock poisoned"))? =
                self.point.verify_route(self.fence)?;
            base.confirm_input_idle()?;
            self.pending()?;
            base.confirm_cursor_at((1920, 1080), 960., 540.)?;
            self.fence.input_check()
        };
        base.guarded(&pinned, &input)?
            .click_checked((1920, 1080), 960., 540., &before_down)?;
        Ok(())
    }
    fn readback(&mut self) -> Result<PlacementDelta> {
        read_recovery::observe(self.fence, "placement_delta", || {
            Ok(self.anchor.observe(self.fence)?.delta())
        })
    }
    fn platform_idle(&mut self) -> Result<()> {
        self.idle()
    }
    fn boundary(&mut self) -> Result<()> {
        self.fence.input_check()
    }
    fn route_diagnostic(&self) -> Value {
        self.route.lock().map(|v| v.clone()).unwrap_or(Value::Null)
    }
    fn until(&self) -> Instant {
        self.fence.until
    }
    fn now(&self) -> Instant {
        Instant::now()
    }
    fn wait(&mut self, d: Duration) {
        thread::sleep(d);
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
            json!({"receipt":receipt,"historical_receipt":true,"current_placement_verified":false}),
        );
    }
    if !enabled {
        return Err(Error::new(
            "capability_unavailable",
            "Mac construction_placement permission is disabled",
        ));
    }
    fence.input_check()?;
    let selected = read_recovery::observe(fence, "placement_initial_queue", || {
        mac_ui::bind_selection(fence, request.key, request.state_id)
    })?;
    let anchor = selected.placement_anchor()?;
    let point = read_recovery::observe(fence, "placement_initial_point", || {
        point::Anchor::bind(fence, request.state_id, selected.context())
    })?;
    require_pending(read_recovery::observe(
        fence,
        "placement_initial_recheck",
        || Ok(anchor.observe(fence)?.delta()),
    )?)?;
    let baseline =
        json!({"construction":anchor.private_baseline()?,"center_point":point.private_baseline()});
    let mut live = Live {
        anchor: &anchor,
        point: &point,
        fence,
        keys,
        route: std::sync::Mutex::new(Value::Null),
    };
    let receipt = journal.execute_strict(request.id, epoch, action, baseline, &mut || {
        dispatch(&mut live)
    })?;
    Ok(json!({"receipt":receipt,"historical_receipt":false,"current_placement_verified":true}))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_construction_place","description":"Default-off Mac single-placement qualification: requires the exact ordinary building already selected, current own construction panel open and idle, hidden Find and original owned state at the calibrated client center. At most one unmodified move/down/up, exact original full-queue +1 readback; no opening, navigation, selection, Escape, retry, pause or worker. Requires complete typed direct-GUI rejection, separately observed dispatch guards and final original-field witnesses. This narrow precondition is not atomic delivery, full picking or absence of GUI effects. Cached SDL correlation does not prove current-event or full-picker eligibility; verified replay is historical.","inputSchema":{"type":"object","properties":{"building_id":{"type":"string","minLength":1,"maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string","maxLength":96},"lease_token":{"type":"string"}},"required":["building_id","state_id","request_id","lease_token"],"additionalProperties":false}})
}
#[cfg(test)]
mod tests;
