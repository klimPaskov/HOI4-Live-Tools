//! One guard, shared by every input lane, against a pause our own input causes.
//!
//! Two ordinary keys can change the pause state behind an action's back. Escape with nothing
//! left to close opens the game menu, which pauses the game and leaves it paused when the menu
//! closes. Space toggles pause whenever no text box holds the keyboard, so a typed space that
//! misses its box stops (or starts) the game. A real campaign never pauses itself, and a pause
//! given by the player or a multiplayer host is not ours to undo, so the guard has to know
//! which change of the pause state came from our own key.
//!
//! [`Watched`] wraps the input binding once, where it is bound. Every tool, the executor and
//! the daily worker send their input through it, so no tool needs its own patch: each Escape
//! and each Space is noted together with the clock read just before it, and a change of the
//! pause state that follows the key is attributed to it. A [`Scope`] brackets one action: it
//! reads the pause state before and after and puts back the state the action started with
//! when the change was ours. Deliberate clock keys go through `press_clock` and are never
//! counted.
use crate::error::Result;
use crate::platform::game_input::{vk, Capture, GameInput, Key, Point};
use crate::platform::{key_once, Process};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The part of the game clock the guard reads: the pause flag and the date in hours.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Clock {
    pub paused: bool,
    pub hour: i32,
}

/// A change of the pause state this long after a noted key counts as caused by it.
const WINDOW: Duration = Duration::from_millis(300);
/// A pause found later still counts as ours while the game date stands within this many hours
/// of the key: the game stopped right after the key, however late the next read came.
const HOURS: i32 = 6;
/// How long the pause key gets to put the state back.
const RESTORE_WAIT: Duration = Duration::from_millis(1500);
/// How long one Escape gets to close what it was pressed for before the next is considered.
pub(crate) const ESCAPE_WAIT: Duration = Duration::from_millis(1200);

type Probe = dyn Fn() -> Option<Clock> + Send + Sync;

struct Armed {
    /// The pause state the key would produce.
    expect: bool,
    at: Instant,
    hour: i32,
}

#[derive(Default)]
struct State {
    armed: Option<Armed>,
    /// The pause state our own key last produced, and how many such changes there were: a
    /// scope only answers for the changes after its start.
    caused: Option<bool>,
    changes: u64,
}

/// Notes pause-capable keys and attributes pause changes to them.
pub(crate) struct Watch {
    probe: Box<Probe>,
    window: Duration,
    state: Mutex<State>,
}

impl Watch {
    pub(crate) fn new(probe: Box<Probe>) -> Self {
        Self::with_window(probe, WINDOW)
    }
    fn with_window(probe: Box<Probe>, window: Duration) -> Self {
        Self {
            probe,
            window,
            state: Mutex::new(State::default()),
        }
    }
    /// The watch for one owned game: the pause flag and date from a fresh clock read.
    pub(crate) fn for_process(process: Arc<dyn Process + Send + Sync>) -> Self {
        Self::new(Box::new(move || {
            crate::game::clock_context(process.as_ref())
                .ok()
                .map(|context| Clock {
                    paused: context.paused,
                    hour: context.date_raw,
                })
        }))
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Decide an open suspicion from one fresh read. Reads nothing when no key is pending.
    fn judge(&self) {
        let mut state = self.lock();
        let Some(armed) = state.armed.as_ref() else {
            return;
        };
        let age = armed.at.elapsed();
        match (self.probe)() {
            Some(clock) if clock.paused == armed.expect => {
                // An unpause is only ours inside the window: putting a pause back on a game
                // somebody else started would itself be a pause of a running game.
                let ours = age <= self.window || (armed.expect && clock.hour - armed.hour <= HOURS);
                if ours {
                    state.caused = Some(armed.expect);
                    state.changes += 1;
                }
                state.armed = None;
            }
            _ if age >= self.window => state.armed = None,
            _ => {}
        }
    }
    /// The clock just before a key, read only for a key that can change the pause state.
    fn before_key(&self, code: u16) -> Option<Clock> {
        self.judge();
        if code == vk::ESCAPE || code == vk::SPACE {
            (self.probe)()
        } else {
            None
        }
    }
    fn after_key(&self, code: u16, before: Option<Clock>) {
        let Some(before) = before else { return };
        // Space toggles either way; Escape can only pause (through the game menu).
        let expect = match code {
            vk::SPACE => !before.paused,
            vk::ESCAPE if !before.paused => true,
            _ => return,
        };
        self.lock().armed = Some(Armed {
            expect,
            at: Instant::now(),
            hour: before.hour,
        });
    }
    /// A deliberate clock key ends any open suspicion: the change that follows is wanted.
    fn deliberate(&self) {
        self.judge();
        self.lock().armed = None;
    }
    /// Wait out the window of a key still pending, so a key sent as the last input of an
    /// action cannot pause the game after the action's final read.
    pub(crate) fn settle(&self) {
        loop {
            self.judge();
            if self.lock().armed.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
    }
    /// Bracket one action: the pause state now is the state the action must leave behind.
    pub(crate) fn begin(&self) -> Scope<'_> {
        self.settle();
        Scope {
            watch: self,
            since: self.lock().changes,
            before: (self.probe)(),
        }
    }
}

/// What the guard found and did for one action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Verdict {
    pub before: Option<bool>,
    pub after: Option<bool>,
    /// The pause state changed during the action and the change came from our own input.
    pub ours: bool,
    /// Whether the pause key put the starting state back; `None` when nothing was ours.
    pub restored: Option<bool>,
}

pub(crate) struct Scope<'a> {
    watch: &'a Watch,
    /// Changes of ours counted before this scope began.
    since: u64,
    before: Option<Clock>,
}

impl Scope<'_> {
    fn before(&self) -> Option<bool> {
        self.before.map(|clock| clock.paused)
    }
    /// Wait out a key still pending (see [`Watch::settle`]).
    pub(crate) fn settle(&self) {
        self.watch.settle();
    }
    /// Close the bracket. `menu` says the game menu was open after the action or was closed by
    /// our input during it (the menu pauses). `toggle` presses the game's own pause key once;
    /// it is given the attempt (0, then 1 when the first key changed nothing within
    /// `RESTORE_WAIT`, so the second can first clear whatever took the key).
    /// A change nobody of ours caused (the player, a multiplayer host, a deliberate clock
    /// action) is left alone.
    pub(crate) fn end(self, menu: bool, toggle: &mut dyn FnMut(u32) -> Result<()>) -> Verdict {
        self.watch.settle();
        let before = self.before();
        let after = (self.watch.probe)().map(|clock| clock.paused);
        let caused = {
            let state = self.watch.lock();
            state.caused.filter(|_| state.changes > self.since)
        };
        let ours = match (before, after) {
            (Some(b), Some(a)) if a != b => caused == Some(a) || (a && menu),
            _ => false,
        };
        let mut verdict = Verdict {
            before,
            after,
            ours,
            restored: None,
        };
        if !ours {
            return verdict;
        }
        let mut restored = false;
        'attempts: for attempt in 0..2 {
            if toggle(attempt).is_err() {
                break;
            }
            let deadline = Instant::now() + RESTORE_WAIT;
            loop {
                if (self.watch.probe)().map(|clock| clock.paused) == before {
                    restored = true;
                    break 'attempts;
                }
                if Instant::now() >= deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
        verdict.restored = Some(restored);
        verdict
    }
}

/// What one round of [`escape_close`] sees.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Seen<T> {
    /// Nothing of ours is open any more.
    Closed,
    /// Still open; `T` changes whenever an Escape closed one layer.
    Open(T),
}

/// Press Escape only for something that reads as open, and never on a fixed sleep: after each
/// press, wait until what was open has changed or the game menu has come up. A view can read
/// as open for a moment after the Escape that closed it; a second Escape pressed on a timer
/// then lands on the free map and opens the game menu, which pauses the game.
///
/// `look` reads what is open and whether the game menu is shown. Returns the number of Escapes
/// pressed; a game menu found open is left for the caller to close.
pub(crate) fn escape_close<T: PartialEq>(
    presses: usize,
    wait: Duration,
    look: &mut dyn FnMut() -> Result<(Seen<T>, bool)>,
    press: &mut dyn FnMut() -> Result<()>,
) -> Result<usize> {
    let mut pressed = 0;
    while pressed < presses {
        let (before, menu) = look()?;
        // With the game menu up the next Escape would go to the menu, not to the view.
        if before == Seen::Closed || menu {
            break;
        }
        press()?;
        pressed += 1;
        let deadline = Instant::now() + wait;
        loop {
            let (now, menu) = look()?;
            if menu || now != before || Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
    Ok(pressed)
}

/// The input binding with the pause watch on every key.
pub(crate) struct Watched<'a> {
    inner: Box<dyn GameInput + 'a>,
    watch: Arc<Watch>,
}

impl<'a> Watched<'a> {
    pub(crate) fn new(inner: Box<dyn GameInput + 'a>, watch: Arc<Watch>) -> Self {
        Self { inner, watch }
    }
    fn key<R>(&self, code: u16, send: impl FnOnce() -> R) -> R {
        let before = self.watch.before_key(code);
        let result = send();
        self.watch.after_key(code, before);
        result
    }
}

impl GameInput for Watched<'_> {
    fn capture(&self, game: (u32, u32)) -> Result<Capture> {
        self.inner.capture(game)
    }
    fn preflight(&self) -> Result<()> {
        self.inner.preflight()
    }
    fn confirm_input_idle(&self) -> Result<()> {
        self.inner.confirm_input_idle()
    }
    fn click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.watch.judge();
        self.inner.click(game, x, y)
    }
    fn click_checked(
        &self,
        game: (u32, u32),
        x: f64,
        y: f64,
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<Point> {
        self.watch.judge();
        self.inner.click_checked(game, x, y, before_down)
    }
    fn confirm_cursor_at(&self, game: (u32, u32), x: f64, y: f64) -> Result<()> {
        self.inner.confirm_cursor_at(game, x, y)
    }
    fn drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.watch.judge();
        self.inner.drag(game, from, to)
    }
    fn drag_checked(
        &self,
        game: (u32, u32),
        from: (f64, f64),
        to: (f64, f64),
        before_down: &dyn Fn() -> Result<()>,
    ) -> Result<()> {
        self.watch.judge();
        self.inner.drag_checked(game, from, to, before_down)
    }
    fn shift_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.watch.judge();
        self.inner.shift_click(game, x, y)
    }
    fn right_click(&self, game: (u32, u32), x: f64, y: f64) -> Result<Point> {
        self.watch.judge();
        self.inner.right_click(game, x, y)
    }
    fn right_drag(&self, game: (u32, u32), from: (f64, f64), to: (f64, f64)) -> Result<()> {
        self.watch.judge();
        self.inner.right_drag(game, from, to)
    }
    fn scroll(&self, game: (u32, u32), at: (f64, f64), pixels: i32) -> Result<()> {
        self.inner.scroll(game, at, pixels)
    }
    fn scroll_burst(
        &self,
        game: (u32, u32),
        at: (f64, f64),
        pixels: i32,
        count: u32,
    ) -> Result<()> {
        self.inner.scroll_burst(game, at, pixels, count)
    }
    fn press_code(&self, code: u16) -> Result<()> {
        self.key(code, || self.inner.press_code(code))
    }
    fn press_shifted(&self, code: u16) -> Result<()> {
        self.key(code, || self.inner.press_shifted(code))
    }
    fn press_control(&self, code: u16) -> Result<()> {
        self.key(code, || self.inner.press_control(code))
    }
    fn press(&self, key: Key) -> Result<()> {
        self.key(key.code(), || self.inner.press(key))
    }
    fn press_clock(&self, key: Key) -> Result<()> {
        self.watch.deliberate();
        self.inner.press_clock(key)
    }
    fn press_once(&self, key: Key) -> key_once::Dispatch {
        self.key(key.code(), || self.inner.press_once(key))
    }
    fn press_digit_once(&self, digit: char) -> key_once::Dispatch {
        self.inner.press_digit_once(digit)
    }
    fn guarded<'b>(
        &'b self,
        process: &'b (dyn Process + Send + Sync),
        check: &'b (dyn Fn() -> Result<()> + Send + Sync),
    ) -> Result<Box<dyn GameInput + 'b>> {
        // A fenced binding sends the same keys: it keeps the same watch.
        Ok(Box::new(Watched {
            inner: self.inner.guarded(process, check)?,
            watch: self.watch.clone(),
        }))
    }
}

#[cfg(test)]
mod tests;
