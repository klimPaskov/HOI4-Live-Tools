//! Stationary ordinary-key clock input, bound after verified normal menu setup.
//! No engine posting, arbitrary keys, activation or cursor operations.
use super::*;
use crate::{
    actions::{Action, Backend, Epoch, Journal, State},
    game::{self, ClockContext},
    input_lease::InputAuthority,
    platform::{
        x11::{ControlKey, X11Controls},
        Process,
    },
    protocol::Commands,
};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, TryLockError};

mod daily;
mod focus;
mod readback;
mod research;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Binding {
    game: Identity,
    controller: String,
    player: String,
    window: u32,
    window_class: Vec<u8>,
    /// IDs of the owned recording/journal's ordinary singleplayer setup evidence.
    /// This is operator evidence, not a native proof of multiplayer flags.
    menu_evidence_ids: Vec<String>,
    /// Owner-only opt-in for the narrow recorded client/origin profile.
    #[serde(default)]
    research_profile: Option<research::Profile>,
    /// Independent explicit owner opt-in; no focus action is enabled by default.
    #[serde(default)]
    focus_profile: Option<focus::Profile>,
    #[serde(default)]
    daily_prepared_research: bool,
    #[serde(default)]
    daily_focus: bool,
}
impl Binding {
    fn validate_daily_focus(&self) -> Result<()> {
        if self.daily_focus && self.focus_profile.is_none() {
            return Err(Error::new(
                "invalid_input_binding",
                "daily_focus requires the existing owned Focus profile",
            ));
        }
        Ok(())
    }
}
struct Guard {
    epoch: Epoch,
    anchors: [u64; 3],
    playthrough: String,
    latest_date: i32,
    invalidated: bool,
}
impl Guard {
    fn observe(&mut self, process: &dyn Process) -> Result<State> {
        self.observe_scoped(process, None)
    }
    fn observe_until(&mut self, process: &dyn Process, deadline: Instant) -> Result<State> {
        readback::observe_until(deadline, || self.observe_scoped(process, Some(deadline)))
    }
    fn observe_scoped(
        &mut self,
        process: &dyn Process,
        deadline: Option<Instant>,
    ) -> Result<State> {
        if self.invalidated {
            return Err(Error::new(
                "input_binding_invalidated",
                "Input context requires a fresh owned run",
            ));
        }
        if process.identity() != Some(&self.epoch.game) {
            self.invalidated = true;
            return Err(Error::new("ownership_changed", "Clock process differs"));
        }
        let mut retries = 0;
        let observed = loop {
            let observed = match deadline {
                Some(until) => readback::observe_until(until, || game::clock_context(process)),
                None => game::clock_context(process),
            };
            match observed {
                Err(e) if e.code == "torn_read" && retries < 2 => {
                    retries += 1;
                    let delay = deadline.map_or(Duration::from_millis(2), |until| {
                        Duration::from_millis(2)
                            .min(until.saturating_duration_since(Instant::now()))
                    });
                    thread::sleep(delay);
                }
                result => break result?,
            }
        };
        self.accept_context(process, &observed)
    }
    fn accept_context(&mut self, process: &dyn Process, observed: &ClockContext) -> Result<State> {
        if self.invalidated || process.identity() != Some(&self.epoch.game) {
            self.invalidated = true;
            return Err(Error::new(
                "input_binding_invalidated",
                "Owned campaign context is unavailable",
            ));
        }
        if observed.playthrough_id != self.playthrough
            || observed.player != self.epoch.player
            || observed.anchors != self.anchors
            || observed.date_raw < self.latest_date
            || observed.difficulty != 2
            || observed.historical_raw != 1
        {
            self.invalidated = true;
            return Err(Error::new(
                "input_binding_invalidated",
                "Campaign, player, setup or observed timeline changed",
            ));
        }
        self.latest_date = observed.date_raw;
        Ok(State {
            epoch: self.epoch.clone(),
            paused: observed.paused,
            speed: observed.speed,
        })
    }
}
struct ControlState {
    // One mutex covers lease, campaign guard and both keyboard/pointer access.
    // Ordinary research and focus adapters borrow this exact controller.
    controls: X11Controls,
    guard: Guard,
    authority: InputAuthority,
    journal: Journal,
    research: Option<research::Service>,
    focus: Option<focus::Service>,
}
pub(super) struct ClockService {
    state: Arc<Mutex<ControlState>>,
    input_gate: Arc<RwLock<bool>>,
    research_enabled: bool,
    focus_enabled: bool,
    daily: daily::Controller,
}
impl ClockService {
    fn input_state(&self) -> Result<MutexGuard<'_, ControlState>> {
        crate::daily_worker::input_state(&self.state, || self.daily.input_check())
    }
    pub(super) fn bind(
        process: &dyn Process,
        display: &Display,
        binding: Binding,
        directory: &Path,
        input_gate: Arc<RwLock<bool>>,
        owner: std::sync::Weak<Shared>,
    ) -> Result<Self> {
        drop(input_permit(&input_gate)?);
        if process.identity() != Some(&binding.game)
            || binding.menu_evidence_ids.is_empty()
            || binding.menu_evidence_ids.len() > 32
            || binding.menu_evidence_ids.iter().any(|v| {
                v.is_empty()
                    || v.len() > 128
                    || !v
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
            })
        {
            return Err(Error::new(
                "invalid_input_binding",
                "Owned game and recorded ordinary singleplayer setup evidence required",
            ));
        }
        binding.validate_daily_focus()?;
        let observed: ClockContext = game::clock_context(process)?;
        if observed.player != binding.player
            || observed.difficulty != 2
            || observed.historical_raw != 1
        {
            return Err(Error::new(
                "invalid_input_binding",
                "Normal historical own-player context required",
            ));
        }
        let controls = X11Controls::open(
            process,
            display.number,
            &display.server,
            &display.xauthority,
            binding.window,
            &binding.window_class,
        )?;
        let epoch = Epoch {
            game: binding.game.clone(),
            campaign: format!("{}:{}", nonce()?, observed.playthrough_id),
            player: binding.player.clone(),
        };
        let authority = InputAuthority::new(binding.controller.clone(), epoch.clone())?;
        let journal = Journal::open(&directory.join("clock-actions.jsonl"))?;
        let research = binding
            .research_profile
            .map(|profile| research::Service::bind(profile, binding.window, display, directory))
            .transpose()?;
        let focus = binding
            .focus_profile
            .map(|profile| focus::Service::bind(profile, binding.window, display, directory))
            .transpose()?;
        json_new(
            &directory.join("input-binding.json"),
            json!({"version":1,"game":binding.game,"controller":binding.controller,"player":binding.player,"window":binding.window,"menu_evidence_ids":binding.menu_evidence_ids,"epoch":epoch,"scope":"ordinary clock keys; optional ID research/focus previews","research_profile":binding.research_profile,"focus_profile":binding.focus_profile,"daily_prepared_research":binding.daily_prepared_research,"daily_focus":binding.daily_focus,"load_generation":"unverified: persisted identity, anchor and timeline checks only"}),
        )?;
        Ok(Self {
            daily: daily::Controller::new(
                binding.daily_prepared_research,
                binding.daily_focus,
                owner,
                directory,
            ),
            input_gate,
            research_enabled: research.is_some(),
            focus_enabled: focus.is_some(),
            state: Arc::new(Mutex::new(ControlState {
                controls,
                guard: Guard {
                    epoch,
                    anchors: observed.anchors,
                    playthrough: observed.playthrough_id,
                    latest_date: observed.date_raw,
                    invalidated: false,
                },
                authority,
                journal,
                research,
                focus,
            })),
        })
    }
}
fn try_state<T>(state: &Mutex<T>) -> Result<MutexGuard<'_, T>> {
    state.try_lock().map_err(|error| match error {
        TryLockError::WouldBlock => Error::new(
            "input_busy",
            "Another ordinary input operation is in progress; request was not queued",
        ),
        TryLockError::Poisoned(_) => Error::new("input_unavailable", "Clock state poisoned"),
    })
}
fn input_permit(gate: &RwLock<bool>) -> Result<RwLockReadGuard<'_, bool>> {
    let permit = gate.try_read().map_err(|_| {
        Error::new(
            "session_closed",
            "Owned game is closing or input gate is unavailable",
        )
    })?;
    if !*permit {
        return Err(Error::new(
            "session_closed",
            "Owned game is closing or has exited",
        ));
    }
    Ok(permit)
}
fn with_input_gate<T>(gate: &RwLock<bool>, deliver: impl FnOnce() -> Result<T>) -> Result<T> {
    let permit = input_permit(gate)?;
    let result = deliver();
    // Shutdown may proceed as soon as a complete pair returns, independently
    // of its subsequent bounded game-state verification.
    drop(permit);
    result
}
fn string<'a>(args: &'a serde_json::Map<String, Value>, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", format!("{name} must be a string")))
}
fn allowed(args: &serde_json::Map<String, Value>, keys: &[&str]) -> Result<()> {
    if args.keys().any(|v| !keys.contains(&v.as_str())) {
        return Err(Error::new("invalid_arguments", "Unexpected input argument"));
    }
    Ok(())
}
impl Commands for ClockService {
    fn tools(&self) -> Vec<Value> {
        let mut tools = vec![
            json!({
                "name":"hoi4_input_lease",
                "description":"Exclusive configured-controller lease for ordinary clock input. Acquire uses holder; renew/release use only lease_token, without holder. Status takes only op. Observations do not need a lease.",
                "inputSchema":{
                    "type":"object",
                    "oneOf":[
                        {"properties":{"op":{"const":"acquire"},"holder":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","holder"],"additionalProperties":false},
                        {"properties":{"op":{"const":"renew"},"lease_token":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","lease_token"],"additionalProperties":false},
                        {"properties":{"op":{"const":"release"},"lease_token":{"type":"string"}},"required":["op","lease_token"],"additionalProperties":false},
                        {"properties":{"op":{"const":"status"}},"required":["op"],"additionalProperties":false}
                    ]
                }
            }),
            json!({"name":"hoi4_time","description":"Set desired pause or internal speed 0..4 through ordinary stationary keys and fresh memory readback. Supply exactly one target and a stable request_id; uncertain dispatches are never replayed.","inputSchema":{"type":"object","properties":{"paused":{"type":"boolean"},"speed":{"type":"integer","minimum":0,"maximum":4},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        ];
        if self.research_enabled {
            tools.push(research::tool());
        }
        if self.focus_enabled {
            tools.push(focus::tool());
            tools.push(crate::focus_ui_observation::tool());
        }
        if self.daily.enabled() {
            tools.push(daily::Controller::tool());
        }
        tools
    }
    fn supports(&self, name: &str) -> bool {
        matches!(name, "hoi4_input_lease" | "hoi4_time")
            || (name == "hoi4_research_attempt" && self.research_enabled)
            || (matches!(name, "hoi4_focus_attempt" | "hoi4_focus_ui") && self.focus_enabled)
            || (name == "hoi4_daily_governor" && self.daily.enabled())
    }
    fn capabilities(&self) -> Value {
        json!({"transport":"ordinary authenticated X11 input","actions":["set_paused","set_speed"],"research_preview":{"enabled":self.research_enabled,"scope":"one OpenPanel, SelectSlot, SelectCategory, View or Research stage for own empty slot","installed_game_validation":"Clock cycles, four assignments and advancement to 5 January passed bounded run025; run024 uncertainty unexplained; full campaign acceptance pending"},"focus_preview":{"enabled":self.focus_enabled,"scope":"one own OpenPolitics/ClosePolitics/OpenFocus/View/Start ordinary stage; exact Sprite/zero-angle Text/unmasked Frame branches","installed_game_validation":"Run045 verified Rust ClosePolitics, identical-request historical nonreplay and subsequent research; worker Focus-to-research handoff awaits live validation"},"focus_ui":{"enabled":self.focus_enabled,"read_only":true,"lease_required":false,"installed_game_validation":"Run042 verified pre-lease and post-Start observations; complete own local flags only"},"daily_focus":{"owner_enabled":self.daily.focus_enabled(),"plan_opt_in_required":true,"research_handoff":{"implemented":true,"direction":"Focus to research only","requires":"Focus and research owner permissions, plan Focus opt-in, remaining assignment budget, fresh own UI and completed Focus key hint","live_validation":false},"full_campaign_acceptance":false},"daily_prepared_research":{"enabled":self.daily.research_enabled(),"navigation_implemented":true,"scope":"Run030 verified four-slot assignment, update/restart and counter advancement; completed-technology refill pending"},"lease":"exclusive configured controller","native_posting":false,"cursor_movement":self.research_enabled||self.focus_enabled,"load_generation":"unverified; campaign identity, anchors and observed timeline checked","full_campaign_acceptance":false})
    }
    fn call(&self, process: &dyn Process, name: &str, arguments: &Value) -> Result<Value> {
        let call_started = Instant::now();
        let args = arguments
            .as_object()
            .ok_or_else(|| Error::new("invalid_arguments", "Input arguments must be an object"))?;
        if name == "hoi4_daily_governor" {
            return self.daily.call(self, process, args);
        }
        if name == "hoi4_research_attempt" {
            return research::call(self, process, args);
        }
        if name == "hoi4_focus_attempt" {
            return focus::call(self, process, args);
        }
        if name == "hoi4_focus_ui" {
            return focus::observe_ui(self, process, args);
        }
        // Validate all shapes before process access or lease mutation.
        if name == "hoi4_input_lease" {
            let op = string(args, "op")?;
            match op {
                "acquire" => {
                    allowed(args, &["op", "holder", "seconds"])?;
                    string(args, "holder")?;
                }
                "renew" => {
                    allowed(args, &["op", "lease_token", "seconds"])?;
                    string(args, "lease_token")?;
                }
                "release" => {
                    allowed(args, &["op", "lease_token"])?;
                    string(args, "lease_token")?;
                }
                "status" => allowed(args, &["op"])?,
                _ => return Err(Error::new("invalid_arguments", "Unknown lease operation")),
            }
            let seconds = args
                .get("seconds")
                .map(|v| {
                    v.as_u64().ok_or_else(|| {
                        Error::new("invalid_arguments", "Lease seconds must be an integer")
                    })
                })
                .transpose()?
                .unwrap_or(300);
            if !(30..=900).contains(&seconds) {
                return Err(Error::new(
                    "invalid_arguments",
                    "Lease seconds must be 30..900",
                ));
            }
            drop(input_permit(&self.input_gate)?);
            let mut state = try_state(&self.state)?;
            let observed = state.guard.observe(process)?;
            let epoch = observed.epoch;
            let reply = match op {
                "acquire" => {
                    let token = nonce()?;
                    state.authority.acquire(
                        string(args, "holder")?,
                        token.clone(),
                        seconds,
                        &epoch,
                    )?;
                    json!({"lease_token":token,"epoch":epoch,"seconds":seconds})
                }
                "renew" => {
                    state
                        .authority
                        .renew(string(args, "lease_token")?, seconds, &epoch)?;
                    json!({"renewed":true,"seconds":seconds})
                }
                "release" => {
                    state
                        .authority
                        .release(string(args, "lease_token")?, &epoch)?;
                    json!({"released":true})
                }
                _ => {
                    let (holder, remaining) = state.authority.status();
                    json!({"holder":holder,"remaining_seconds":remaining.as_secs_f64(),"epoch":epoch})
                }
            };
            return Ok(reply);
        }
        if name != "hoi4_time" {
            return Err(Error::new(
                "capability_unavailable",
                "Unknown ordinary input tool",
            ));
        }
        allowed(args, &["paused", "speed", "request_id", "lease_token"])?;
        let id = string(args, "request_id")?;
        let token = string(args, "lease_token")?;
        let action = match (args.get("paused"), args.get("speed")) {
            (Some(v), None) => Action::SetPaused(
                v.as_bool()
                    .ok_or_else(|| Error::new("invalid_arguments", "Paused must be a boolean"))?,
            ),
            (None, Some(v)) => Action::SetSpeed(
                v.as_u64()
                    .filter(|v| *v <= 4)
                    .ok_or_else(|| Error::new("invalid_arguments", "Speed must be integer 0..4"))?
                    as u8,
            ),
            _ => {
                return Err(Error::new(
                    "invalid_arguments",
                    "Supply exactly one clock target",
                ))
            }
        };
        self.daily.input_check()?;
        drop(input_permit(&self.input_gate)?);
        let mut state = self.input_state()?;
        if state
            .focus
            .as_ref()
            .is_some_and(|f| f.journal.blocks_game(&state.guard.epoch.game))
            || state
                .research
                .as_ref()
                .is_some_and(|r| r.journal.blocks_game(&state.guard.epoch.game))
        {
            return Err(Error::new(
                "action_uncertain",
                "Unresolved research/focus attempt blocks clock input",
            ));
        }
        let epoch = state.guard.epoch.clone();
        let deadline =
            readback::operation_deadline(call_started, state.authority.authorize(token, &epoch)?);
        state.guard.observe_until(process, deadline)?;
        let ControlState {
            guard,
            journal,
            controls,
            ..
        } = &mut *state;
        let mut backend = ClockBackend {
            process,
            controls,
            guard,
            deadline,
            input_gate: &self.input_gate,
        };
        let receipt = journal.execute(
            id,
            &epoch,
            action,
            &mut backend,
            readback::OPERATION_TIMEOUT,
        )?;
        serde_json::to_value(receipt).map_err(|e| Error::new("receipt_failed", e.to_string()))
    }
}
struct ClockBackend<'a> {
    process: &'a dyn Process,
    controls: &'a X11Controls,
    guard: &'a mut Guard,
    deadline: Instant,
    input_gate: &'a RwLock<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClockStep {
    Pause(bool),
    Faster(u8),
    Slower(u8),
}
impl ClockStep {
    fn plan(action: &Action, state: &State) -> Option<Self> {
        match *action {
            Action::SetPaused(target) if target != state.paused => Some(Self::Pause(target)),
            Action::SetSpeed(target) if target > state.speed => Some(Self::Faster(state.speed + 1)),
            Action::SetSpeed(target) if target < state.speed => Some(Self::Slower(state.speed - 1)),
            _ => None,
        }
    }
    fn key(self) -> ControlKey {
        match self {
            Self::Pause(_) => ControlKey::Pause,
            Self::Faster(_) => ControlKey::Faster,
            Self::Slower(_) => ControlKey::Slower,
        }
    }
    fn same_key(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Pause(_), Self::Pause(_))
                | (Self::Faster(_), Self::Faster(_))
                | (Self::Slower(_), Self::Slower(_))
        )
    }
    fn reached(self, state: &State) -> bool {
        match self {
            Self::Pause(target) => state.paused == target,
            Self::Faster(next) | Self::Slower(next) => state.speed == next,
        }
    }
}

/// Decide once before UI checks, then derive the actual next-state witness
/// from a guarded observation immediately before either key event is queued.
/// The injected delivery callback also makes the avoidable state race testable
/// without an X server, process attachment or actual keyboard input.
fn deliver_clock_step(
    action: &Action,
    mut observe: impl FnMut() -> Result<State>,
    deliver: impl FnOnce(ControlKey, &mut dyn FnMut() -> Result<bool>) -> Result<bool>,
) -> Result<Option<ClockStep>> {
    let initial_state = observe()?;
    let Some(initial_step) = ClockStep::plan(action, &initial_state) else {
        return Ok(None);
    };
    let mut delivered_step = None;
    let sent = deliver(initial_step.key(), &mut || {
        let fresh = observe()?;
        if fresh.epoch != initial_state.epoch {
            return Err(Error::new(
                "ownership_changed",
                "Clock epoch changed before delivery",
            ));
        }
        let Some(fresh_step) = ClockStep::plan(action, &fresh) else {
            return Ok(false); // Automatic pause/another change already reached the target.
        };
        if !initial_step.same_key(fresh_step) {
            return Err(Error::new(
                "clock_changed_before_delivery",
                "Clock change requires a different key; no input was queued",
            ));
        }
        delivered_step = Some(fresh_step);
        Ok(true)
    })?;
    if sent != delivered_step.is_some() {
        return Err(Error::new(
            "input_readback_failed",
            "Key delivery has no checked step",
        ));
    }
    Ok(delivered_step)
}

impl ClockBackend<'_> {
    fn live_lease(&self) -> Result<()> {
        readback::ensure_live(self.deadline)
    }
    fn step(&mut self, action: &Action) -> Result<bool> {
        self.live_lease()?;
        let process = self.process;
        let controls = self.controls;
        let deadline = self.deadline;
        let delivered = with_input_gate(self.input_gate, || {
            deliver_clock_step(
                action,
                || {
                    self.live_lease()?;
                    self.guard.observe_until(process, deadline)
                },
                |key, before_queue| {
                    controls.send_until_checked(process, key, deadline, before_queue)
                },
            )
        })?;
        if let Some(step) = delivered {
            let observed = self.wait_change(|state| step.reached(state))?;
            Ok(ClockStep::plan(action, &observed).is_none())
        } else {
            Ok(true)
        }
    }
    fn wait_change(&mut self, reached: impl Fn(&State) -> bool) -> Result<State> {
        let deadline = readback::step_deadline(self.deadline, Instant::now());
        readback::wait_change(deadline, || self.observe(), reached)
    }
}
impl Backend for ClockBackend<'_> {
    fn observe(&mut self) -> Result<State> {
        self.guard.observe_until(self.process, self.deadline)
    }
    fn preflight(&mut self, action: &Action, before: &State) -> Result<()> {
        self.live_lease()?;
        drop(input_permit(self.input_gate)?);
        let fresh = self.observe()?;
        if fresh.epoch != before.epoch {
            return Err(Error::new("ownership_changed", "Clock epoch changed"));
        }
        let key = match action {
            Action::SetPaused(target) if *target != fresh.paused => Some(ControlKey::Pause),
            Action::SetSpeed(target) if *target != fresh.speed => Some(if *target > fresh.speed {
                ControlKey::Faster
            } else {
                ControlKey::Slower
            }),
            _ => None,
        };
        if let Some(key) = key {
            self.controls.preflight(self.process, &key)?;
        }
        self.live_lease()?;
        drop(input_permit(self.input_gate)?);
        Ok(())
    }
    fn dispatch(&mut self, action: &Action, _before: &State) -> Result<()> {
        match action {
            Action::SetPaused(_) => {
                self.step(action)?;
            }
            Action::SetSpeed(target) => {
                for _ in 0..4 {
                    if self.step(action)? {
                        return Ok(());
                    }
                }
                if self.observe()?.speed != *target {
                    return Err(Error::new(
                        "input_readback_failed",
                        "Clock speed did not reach target",
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn state(paused: bool, speed: u8) -> State {
        State {
            epoch: Epoch {
                game: Identity {
                    pid: 123,
                    start_ticks: "5".into(),
                    exe: "/fixture".into(),
                    uid: 1000,
                },
                campaign: "generated-clock-context".into(),
                player: "GER".into(),
            },
            paused,
            speed,
        }
    }

    #[test]
    fn final_queue_observation_cannot_return_a_late_noop_or_send_a_key() {
        let start = Instant::now();
        let deadline = start + Duration::from_secs(10);
        let now = std::cell::Cell::new(start);
        let observations = std::cell::Cell::new(0);
        let pairs = std::cell::Cell::new(0);
        let error = deliver_clock_step(
            &Action::SetPaused(false),
            || {
                readback::fixture_observe_until(
                    deadline,
                    || {
                        observations.set(observations.get() + 1);
                        if observations.get() == 2 {
                            now.set(deadline);
                            Ok(state(false, 0)) // Target reached too late in final callback.
                        } else {
                            Ok(state(true, 0))
                        }
                    },
                    || now.get(),
                )
            },
            |_, before_queue| {
                let ready = before_queue()?;
                if ready {
                    pairs.set(pairs.get() + 1);
                }
                Ok(ready)
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "input_lease_required");
        assert_eq!(observations.get(), 2);
        assert_eq!(pairs.get(), 0);
    }

    #[test]
    fn final_action_target_does_not_replace_delivered_intermediate_field_witness() {
        let start = Instant::now();
        let now = std::cell::Cell::new(start);
        let pairs = std::cell::Cell::new(0);
        let mut states = VecDeque::from([state(true, 0), state(true, 0)]);
        let step = deliver_clock_step(
            &Action::SetSpeed(4),
            || Ok(states.pop_front().unwrap()),
            |_, before_queue| {
                let ready = before_queue()?;
                if ready {
                    pairs.set(pairs.get() + 1);
                }
                Ok(ready)
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(step, ClockStep::Faster(1));
        let error = readback::fixture_wait_change(
            start + Duration::from_millis(1500),
            || {
                now.set(now.get() + Duration::from_millis(800));
                Ok(state(true, 4))
            },
            |fresh| step.reached(fresh),
            || now.get(),
            |d| now.set(now.get() + d),
        )
        .unwrap_err();
        assert_eq!(error.code, "input_readback_failed");
        assert_eq!(pairs.get(), 1); // No attempt to deliver a replacement key.
    }

    #[test]
    fn busy_clock_state_refuses_without_a_pending_operation() {
        let state = Arc::new(Mutex::new(0_u8));
        let held = state.lock().unwrap();
        let other = Arc::clone(&state);
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            let result = try_state(&other).map(|mut value| *value = 42);
            sender.send(result.map_err(|error| error.code)).unwrap();
        });
        let refused = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(refused.unwrap_err(), "input_busy");
        assert_eq!(*held, 0);
        drop(held);
        worker.join().unwrap();
        assert_eq!(*try_state(&state).unwrap(), 0);
        *try_state(&state).unwrap() = 1;
        assert_eq!(*state.lock().unwrap(), 1);
    }

    #[test]
    fn shutdown_gate_refuses_before_decision_and_delivery() {
        let gate = RwLock::new(true);
        let mut decisions = 0;
        let writer = gate.write().unwrap();
        let error = with_input_gate(&gate, || {
            decisions += 1;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.code, "session_closed");
        drop(writer);
        *gate.write().unwrap() = false;
        let error = with_input_gate(&gate, || {
            decisions += 1;
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.code, "session_closed");
        assert_eq!(decisions, 0);
    }

    #[test]
    fn complete_key_pair_holds_gate_but_readback_does_not() {
        let gate = RwLock::new(true);
        let mut states = VecDeque::from([state(false, 1), state(false, 2)]);
        let mut events = Vec::new();
        let step = with_input_gate(&gate, || {
            deliver_clock_step(
                &Action::SetSpeed(3),
                || {
                    assert!(matches!(gate.try_write(), Err(TryLockError::WouldBlock)));
                    Ok(states.pop_front().unwrap())
                },
                |_, before_queue| {
                    let send = before_queue()?;
                    assert!(send);
                    events.push("press");
                    assert!(matches!(gate.try_write(), Err(TryLockError::WouldBlock)));
                    events.push("release");
                    assert!(matches!(gate.try_write(), Err(TryLockError::WouldBlock)));
                    Ok(true)
                },
            )
        })
        .unwrap()
        .unwrap();
        assert_eq!(events, ["press", "release"]);
        // The owner can disable input before this step's memory readback.
        *gate.try_write().unwrap() = false;
        assert!(step.reached(&state(false, 3)));
        let error = with_input_gate(&gate, || {
            events.push("unexpected next pair");
            Ok(())
        })
        .unwrap_err();
        assert_eq!(error.code, "session_closed");
        assert_eq!(events, ["press", "release"]);
    }

    #[test]
    fn automatic_pause_before_queue_never_sends_an_opposite_toggle() {
        let mut states = VecDeque::from([state(false, 1), state(true, 1)]);
        let mut keys = Vec::new();
        let step = deliver_clock_step(
            &Action::SetPaused(true),
            || Ok(states.pop_front().unwrap()),
            |key, before_queue| {
                assert!(matches!(key, ControlKey::Pause));
                let send = before_queue()?;
                if send {
                    keys.push("Space");
                }
                Ok(send)
            },
        )
        .unwrap();
        assert!(states.is_empty());
        assert!(keys.is_empty());
        assert_eq!(step, None);
    }

    #[test]
    fn fresh_speed_before_queue_sets_the_actual_next_witness() {
        let mut states = VecDeque::from([state(false, 1), state(false, 2)]);
        let mut keys = Vec::new();
        let step = deliver_clock_step(
            &Action::SetSpeed(3),
            || Ok(states.pop_front().unwrap()),
            |key, before_queue| {
                assert!(matches!(key, ControlKey::Faster));
                let send = before_queue()?;
                if send {
                    keys.push("KP+");
                }
                Ok(send)
            },
        )
        .unwrap()
        .unwrap();
        assert!(states.is_empty());
        assert_eq!(keys, ["KP+"]);
        assert_eq!(step, ClockStep::Faster(3));
        assert!(step.reached(&state(false, 3)));
        assert!(!step.reached(&state(false, 2)));
    }

    #[test]
    fn reversed_speed_before_queue_refuses_the_preflighted_key() {
        let mut states = VecDeque::from([state(false, 1), state(false, 3)]);
        let mut sends = 0;
        let error = deliver_clock_step(
            &Action::SetSpeed(2),
            || Ok(states.pop_front().unwrap()),
            |_, before_queue| {
                let send = before_queue()?;
                sends += usize::from(send);
                Ok(send)
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "clock_changed_before_delivery");
        assert!(states.is_empty());
        assert_eq!(sends, 0);
    }
    #[test]
    fn daily_focus_binding_is_default_off_and_profile_is_required() {
        let value = json!({"game":{"pid":123,"start_ticks":"456","exe":"/generated/game","uid":1000},"controller":"generated","player":"GER","window":1,"window_class":[],"menu_evidence_ids":["setup"]});
        let b: Binding = serde_json::from_value(value.clone()).unwrap();
        assert!(!b.daily_focus);
        assert!(b.validate_daily_focus().is_ok());
        let mut enabled = value.clone();
        enabled["daily_focus"] = json!(true);
        let b: Binding = serde_json::from_value(enabled.clone()).unwrap();
        assert_eq!(
            b.validate_daily_focus().unwrap_err().code,
            "invalid_input_binding"
        );
        enabled["focus_profile"] = json!("linux_builtin_absolute_512x288_v1");
        let b: Binding = serde_json::from_value(enabled).unwrap();
        assert!(b.daily_focus);
        assert!(b.validate_daily_focus().is_ok());
        let mut invalid = value;
        invalid["daily_focus"] = json!(1);
        assert!(serde_json::from_value::<Binding>(invalid).is_err());
    }
}
