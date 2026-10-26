//! Server-side `stop_on_date` watcher. `plan_run` only checks the date when the
//! broker calls it, so a busy broker fired the pause days late. This thread owns
//! the check: once per real second it re-reads `plan.json`'s `stop_on_date`
//! (same formats as `plan_run`) and the own game date (a lock-free, pure
//! memory read that never pauses), and when game time reaches the target it
//! issues ONE `hoi4_time paused:true` through the ordinary journaled clock
//! path under the request id `worker-sd-<target hour>-<game pid>`.
//!
//! Safety rules, all enforced here:
//! * it acts only with the controller lease the owner armed it with (the same
//!   token, epoch and `Control` fence every clock action uses); without a
//!   valid lease it cannot act and says `stop_on_date_unarmed: no lease`;
//! * an already-paused game gets no input (the player is never fought);
//! * a request id with a journal receipt is never sent again, and an outcome
//!   that may have involved input (a receipt exists after an error) is final:
//!   it is recorded, raised as an alarm and never retried;
//! * errors raised before any input (busy controller, torn read, lease gap)
//!   are retried each second, capped at `MAX_ATTEMPTS`, then final.
use super::{retained_input_check, Retained};
use super::super::{input_gate, ClockBackend, Control};
use crate::{
    actions::{Action, Status as ReceiptStatus},
    daily_worker,
    error::{Error, Result},
    game,
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Pre-input retries (one per second) before the outcome becomes final.
pub(super) const MAX_ATTEMPTS: u32 = 30;
const UNARMED: &str = "stop_on_date_unarmed: no lease";

#[derive(Clone, Default, Debug)]
pub(super) struct Status {
    target: Option<(i64, String)>,
    state: String,
    fired_for: Option<i64>,
    in_flight: bool,
    outcome: Option<String>,
    /// the final outcome is a failure (refused, not applied or uncertain)
    failed: bool,
    attempts: u32,
    hour: Option<i64>,
    lease: Option<bool>,
    token: bool,
    note: Option<String>,
}
impl Status {
    pub(super) fn to_json(&self) -> Value {
        let armed = self.target.is_some()
            && self.token
            && self.lease != Some(false)
            && self.fired_for.is_none_or(|f| Some(f) != self.target.as_ref().map(|t| t.0));
        let mut value = json!({
            "armed": armed,
            "target": self.target.as_ref().map(|t| t.1.clone()),
            "target_hour": self.target.as_ref().map(|t| t.0),
            "state": self.state,
            "fired": self.fired_for.is_some() && self.fired_for == self.target.as_ref().map(|t| t.0),
            "in_flight": self.in_flight,
            "outcome": self.outcome,
            "attempts": self.attempts,
            "game_hour": self.hour,
            "note": self.note,
            "observation_pauses": 0,
        });
        if let Some(alarm) = self.alarm() {
            value["alarm"] = alarm;
        }
        value
    }
    fn alarm(&self) -> Option<Value> {
        let fired = self.fired_for.is_some() && self.fired_for == self.target.as_ref().map(|t| t.0);
        let message = if fired && self.failed {
            format!(
                "stop_on_date pause did not complete: {}",
                self.outcome.as_deref().unwrap_or("unknown")
            )
        } else if !fired && self.attempts >= 5 {
            format!(
                "stop_on_date reached but the pause is blocked ({}); retrying",
                self.note.as_deref().unwrap_or("unknown")
            )
        } else {
            return None;
        };
        Some(json!({"code": "date_stop_failed", "message": message,
            "fix": {"tool": "hoi4_time", "args_hint": {"paused": true}}}))
    }
}

/// What one pause attempt came to.
#[derive(Debug, PartialEq)]
pub(super) enum Pause {
    Verified,
    /// no input was sent; try again next second
    Retry(String),
    /// final: `failed` raises the alarm
    Final { text: String, failed: bool },
}

/// Everything the watcher touches; fakes drive the tests.
pub(super) trait Env: Send + Sync {
    fn plan(&self) -> Result<Value>;
    /// (game hour, paused): a pure read
    fn clock(&self) -> Result<(i64, bool)>;
    fn has_token(&self) -> bool;
    /// `Some(false)`: no valid lease; `None`: unknown right now (controller busy)
    fn lease(&self) -> Option<bool>;
    fn game_id(&self) -> String;
    fn pause(&self, id: &str) -> Pause;
}

fn upd(status: &Mutex<Status>, f: impl FnOnce(&mut Status)) {
    f(&mut status.lock().unwrap_or_else(|p| p.into_inner()))
}

enum Parsed {
    Off(&'static str),
    None,
    Invalid(String),
    Target { hour: i64, label: String, pause: bool },
}
fn parse(plan: &Value) -> Parsed {
    if plan["active"] == json!(false) {
        return Parsed::Off("plan inactive (active:false)");
    }
    let (hour, label) = match crate::alarms::stop_on_date(plan) {
        Err(e) => return Parsed::Invalid(e.message),
        Ok(None) => return Parsed::None,
        Ok(Some(t)) => t,
    };
    let pause = match plan.get("stop_on_date_pause") {
        None | Some(Value::Bool(true)) => true,
        Some(Value::Bool(false)) => false,
        _ => return Parsed::Invalid("stop_on_date_pause must be boolean".into()),
    };
    Parsed::Target { hour, label, pause }
}

/// One watch tick. Never blocks on the controller beyond `Env::pause`'s own
/// non-blocking attempts.
pub(super) fn step(status: &Mutex<Status>, env: &dyn Env) {
    let plan = match env.plan() {
        Ok(plan) => plan,
        Err(e) => {
            upd(status, |s| s.note = Some(format!("plan unreadable: {}", e.code)));
            return;
        }
    };
    let (hour, label, pause) = match parse(&plan) {
        Parsed::Off(why) => {
            upd(status, |s| {
                s.target = None;
                s.state = why.into();
            });
            return;
        }
        Parsed::None => {
            upd(status, |s| {
                s.target = None;
                s.state = "no stop_on_date in plan".into();
            });
            return;
        }
        Parsed::Invalid(why) => {
            upd(status, |s| {
                s.target = None;
                s.state = format!("invalid plan: {why}");
            });
            return;
        }
        Parsed::Target { hour, label, pause } => (hour, label, pause),
    };
    let lease = env.lease();
    let token = env.has_token();
    let fired = {
        let mut s = status.lock().unwrap_or_else(|p| p.into_inner());
        if s.target.as_ref().map(|t| t.0) != Some(hour) {
            s.attempts = 0;
            s.outcome = None;
            s.failed = false;
            s.note = None;
        }
        s.target = Some((hour, label.clone()));
        s.lease = lease;
        s.token = token;
        s.fired_for == Some(hour)
    };
    if fired {
        return;
    }
    let (now, paused) = match env.clock() {
        Ok(read) => read,
        Err(e) => {
            upd(status, |s| s.note = Some(format!("clock unreadable: {}", e.code)));
            return;
        }
    };
    let fire = |text: String, failed: bool| {
        upd(status, |s| {
            s.fired_for = Some(hour);
            s.in_flight = false;
            s.outcome = Some(text);
            s.failed = failed;
            s.state = "fired".into();
        })
    };
    upd(status, |s| s.hour = Some(now));
    if now < hour {
        let unarmed = !token || lease == Some(false);
        upd(status, |s| {
            s.state = if unarmed { UNARMED.into() } else { "waiting".into() };
        });
        return;
    }
    if !pause {
        return fire("not requested (stop_on_date_pause false)".into(), false);
    }
    if paused {
        return fire("already paused".into(), false);
    }
    if !token || lease == Some(false) {
        upd(status, |s| {
            s.state = UNARMED.into();
            s.attempts += 1;
            s.note = Some("no valid controller lease".into());
        });
        return;
    }
    let attempts = {
        let mut s = status.lock().unwrap_or_else(|p| p.into_inner());
        s.in_flight = true;
        s.state = "pausing".into();
        s.attempts += 1;
        s.attempts
    };
    let id = format!("worker-sd-{hour}-{}", env.game_id());
    match env.pause(&id) {
        Pause::Verified => fire("paused".into(), false),
        Pause::Final { text, failed } => fire(text, failed),
        Pause::Retry(why) if attempts >= MAX_ATTEMPTS => {
            fire(format!("pause refused after {attempts} attempts ({why}); not retried"), true)
        }
        Pause::Retry(why) => upd(status, |s| {
            s.in_flight = false;
            s.state = "retrying".into();
            s.note = Some(why);
        }),
    }
}

pub(super) struct Shared {
    pub(super) status: Mutex<Status>,
    token: Mutex<Option<String>>,
}
impl Shared {
    pub(super) fn token(&self) -> Option<String> {
        self.token.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// Owned by the daily controller; the thread starts at the first arming.
pub(super) struct DateStop {
    shared: Arc<Shared>,
    thread: Mutex<Option<(Arc<AtomicBool>, JoinHandle<()>)>>,
}
impl Default for DateStop {
    fn default() -> Self {
        Self {
            shared: Arc::new(Shared {
                status: Mutex::new(Status::default()),
                token: Mutex::new(None),
            }),
            thread: Mutex::new(None),
        }
    }
}
impl DateStop {
    pub(super) fn disarm(&self) {
        *self.shared.token.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
    pub(super) fn arm(&self, token: String, make: impl FnOnce(Arc<Shared>) -> Arc<dyn Env>) {
        *self.shared.token.lock().unwrap_or_else(|p| p.into_inner()) = Some(token);
        let mut thread = self.thread.lock().unwrap_or_else(|p| p.into_inner());
        if thread.as_ref().is_some_and(|(_, h)| !h.is_finished()) {
            return;
        }
        let env = make(self.shared.clone());
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, shared) = (stop.clone(), self.shared.clone());
        match thread::Builder::new().name("hoi4-mac-date-stop".into()).spawn(move || {
            while !flag.load(Ordering::Acquire) {
                let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    step(&shared.status, env.as_ref())
                }));
                if ran.is_err() {
                    upd(&shared.status, |s| {
                        s.state = "watcher panicked; stop_on_date_unarmed".into();
                    });
                    return;
                }
                for _ in 0..10 {
                    if flag.load(Ordering::Acquire) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }) {
            Ok(handle) => *thread = Some((stop, handle)),
            Err(e) => upd(&self.shared.status, |s| {
                s.state = format!("watcher not started: {e}");
            }),
        }
    }
    pub(super) fn stop(&self) {
        if let Some((flag, handle)) = self.thread.lock().unwrap_or_else(|p| p.into_inner()).take() {
            flag.store(true, Ordering::Release);
            if handle.thread().id() != thread::current().id() {
                let _ = handle.join();
            }
        }
    }
    /// Digest/plan view. Without a started watcher the plan file still says
    /// whether a date is waiting and that nothing can act on it.
    pub(super) fn status_json(&self, plan_path: &std::path::Path) -> Value {
        let running = self
            .thread
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .is_some_and(|(_, h)| !h.is_finished());
        if running {
            let status = self.shared.status.lock().unwrap_or_else(|p| p.into_inner());
            let mut value = status.to_json();
            value["running"] = json!(true);
            return value;
        }
        let plan = crate::plan_run::read_plan(plan_path).unwrap_or_else(|_| json!({}));
        match parse(&plan) {
            Parsed::Target { label, .. } => json!({"armed": false, "running": false, "target": label,
                "fired": false, "in_flight": false, "state": UNARMED, "outcome": null,
                "observation_pauses": 0}),
            _ => json!({"armed": false, "running": false, "target": null, "fired": false,
                "in_flight": false, "state": "no stop_on_date in plan", "outcome": null,
                "observation_pauses": 0}),
        }
    }
}
impl Drop for DateStop {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The real environment: plan file, own memory and the shared controller.
pub(super) struct Live {
    pub(super) shared: Arc<Shared>,
    pub(super) process: Arc<dyn Process + Send + Sync>,
    pub(super) keys: Arc<MacKeys<'static>>,
    pub(super) control: Arc<Mutex<Control>>,
    pub(super) retained: Arc<Mutex<Retained>>,
    pub(super) blocked: Arc<AtomicBool>,
    pub(super) directory: PathBuf,
    pub(super) epoch: crate::actions::Epoch,
}
fn classify(e: &Error) -> Pause {
    match e.code {
        "ownership_changed" | "idempotency_conflict" | "journal_failed" | "invalid_action" => {
            Pause::Final {
                text: format!("pause refused ({}: {}); not retried", e.code, e.message),
                failed: true,
            }
        }
        _ => Pause::Retry(format!("{}: {}", e.code, e.message)),
    }
}
impl Env for Live {
    fn plan(&self) -> Result<Value> {
        crate::plan_run::read_plan(&self.directory.join("plan.json"))
    }
    fn clock(&self) -> Result<(i64, bool)> {
        if self.process.identity() != Some(&self.epoch.game) {
            return Err(Error::new("ownership_changed", "Clock process differs"));
        }
        let mut retries = 0;
        let context = loop {
            match game::clock_context(self.process.as_ref()) {
                Err(e) if e.code == "torn_read" && retries < 3 => {
                    retries += 1;
                    thread::sleep(Duration::from_millis(2));
                }
                result => break result?,
            }
        };
        if context.player != self.epoch.player {
            return Err(Error::new("ownership_changed", "Campaign player differs"));
        }
        Ok((i64::from(context.date_raw), context.paused))
    }
    fn has_token(&self) -> bool {
        self.shared.token().is_some()
    }
    fn lease(&self) -> Option<bool> {
        let token = self.shared.token()?;
        match self.control.try_lock() {
            Ok(control) => Some(control.authority.authorize(&token, &self.epoch).is_ok()),
            Err(std::sync::TryLockError::WouldBlock) => None,
            Err(std::sync::TryLockError::Poisoned(_)) => Some(false),
        }
    }
    fn game_id(&self) -> String {
        self.epoch.game.pid.to_string()
    }
    fn pause(&self, id: &str) -> Pause {
        let Some(token) = self.shared.token() else {
            return Pause::Retry("no lease".into());
        };
        let mut control: MutexGuard<'_, Control> = match daily_worker::input_state(&self.control, || {
            retained_input_check(&self.directory, &self.blocked, &self.retained)
        }) {
            Ok(guard) => guard,
            Err(e) => return classify(&e),
        };
        let Control {
            guard,
            authority,
            journal,
            actions,
        } = &mut *control;
        if let Some(receipt) = journal.receipt(id) {
            return Pause::Final {
                text: format!("pause receipt exists ({:?}); not re-issued", receipt.status),
                failed: receipt.status != ReceiptStatus::Verified,
            };
        }
        if let Err(e) = input_gate::check(journal, actions, &guard.epoch.game, true, Some(id)) {
            return classify(&e);
        }
        let epoch = guard.epoch.clone();
        let mut backend = ClockBackend {
            process: self.process.as_ref(),
            keys: &self.keys,
            guard,
            authority,
            token: &token,
        };
        match journal.execute(id, &epoch, Action::SetPaused(true), &mut backend, Duration::from_secs(10)) {
            Ok(receipt) => match receipt.status {
                ReceiptStatus::Verified => Pause::Verified,
                ReceiptStatus::NotApplied => Pause::Final {
                    text: "pause not applied (clock target not observed); not retried".into(),
                    failed: true,
                },
                ReceiptStatus::Dispatching => Pause::Final {
                    text: "pause uncertain (receipt still dispatching); not retried, settle by hand".into(),
                    failed: true,
                },
            },
            // The journal records Dispatching before any key: a receipt means input may have happened.
            Err(e) if journal.receipt(id).is_some() => Pause::Final {
                text: format!("pause uncertain ({}: {}); not retried, settle by hand", e.code, e.message),
                failed: true,
            },
            Err(e) => classify(&e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;

    struct Fake {
        plan: Mutex<Value>,
        clock: Mutex<(i64, bool)>,
        token: AtomicBool,
        lease: Mutex<Option<bool>>,
        replies: Mutex<std::collections::VecDeque<Pause>>,
        pauses: AtomicU32,
        ids: Mutex<Vec<String>>,
    }
    impl Fake {
        fn new(plan: Value, hour: i64, paused: bool) -> Self {
            Self {
                plan: Mutex::new(plan),
                clock: Mutex::new((hour, paused)),
                token: AtomicBool::new(true),
                lease: Mutex::new(Some(true)),
                replies: Mutex::new([Pause::Verified].into()),
                pauses: AtomicU32::new(0),
                ids: Mutex::new(vec![]),
            }
        }
    }
    impl Env for Fake {
        fn plan(&self) -> Result<Value> {
            Ok(self.plan.lock().unwrap().clone())
        }
        fn clock(&self) -> Result<(i64, bool)> {
            Ok(*self.clock.lock().unwrap())
        }
        fn has_token(&self) -> bool {
            self.token.load(Ordering::SeqCst)
        }
        fn lease(&self) -> Option<bool> {
            *self.lease.lock().unwrap()
        }
        fn game_id(&self) -> String {
            "77".into()
        }
        fn pause(&self, id: &str) -> Pause {
            self.pauses.fetch_add(1, Ordering::SeqCst);
            self.ids.lock().unwrap().push(id.into());
            self.replies.lock().unwrap().pop_front().expect("unexpected extra pause")
        }
    }
    fn target() -> i64 {
        i64::from(crate::calendar::encode(1938, 1, 1).unwrap())
    }
    fn plan() -> Value {
        json!({"stop_on_date": "1938.01.01"})
    }
    fn state(s: &Mutex<Status>) -> Value {
        s.lock().unwrap().to_json()
    }

    #[test]
    fn waits_before_the_date_and_fires_one_pause_at_it_under_its_own_id() {
        let fake = Fake::new(plan(), target() - 1, false);
        let status = Mutex::new(Status::default());
        step(&status, &fake);
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 0);
        assert_eq!(state(&status)["state"], "waiting");
        assert_eq!(state(&status)["armed"], true);
        *fake.clock.lock().unwrap() = (target() + 3, false);
        for _ in 0..4 {
            step(&status, &fake);
        }
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 1);
        assert_eq!(fake.ids.lock().unwrap()[0], format!("worker-sd-{}-77", target()));
        let v = state(&status);
        assert_eq!(v["fired"], true);
        assert_eq!(v["outcome"], "paused");
        assert_eq!(v["armed"], false);
        assert!(v.get("alarm").is_none());
    }

    #[test]
    fn already_paused_game_gets_no_input_and_the_player_is_not_fought() {
        let fake = Fake::new(plan(), target() + 24, true);
        let status = Mutex::new(Status::default());
        step(&status, &fake);
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 0);
        assert_eq!(state(&status)["outcome"], "already paused");
        // the player resumes: still done, never paused again
        *fake.clock.lock().unwrap() = (target() + 48, false);
        step(&status, &fake);
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn without_a_lease_it_cannot_act_says_so_and_fires_once_the_lease_returns() {
        let fake = Fake::new(plan(), target() - 5, false);
        fake.token.store(false, Ordering::SeqCst);
        let status = Mutex::new(Status::default());
        step(&status, &fake);
        assert_eq!(state(&status)["state"], UNARMED);
        assert_eq!(state(&status)["armed"], false);
        *fake.clock.lock().unwrap() = (target() + 10, false);
        step(&status, &fake);
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 0);
        assert_eq!(state(&status)["state"], UNARMED);
        assert_eq!(state(&status)["fired"], false);
        fake.token.store(true, Ordering::SeqCst);
        *fake.lease.lock().unwrap() = Some(false); // expired
        step(&status, &fake);
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 0);
        *fake.lease.lock().unwrap() = Some(true);
        step(&status, &fake);
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 1);
        assert_eq!(state(&status)["fired"], true);
    }

    #[test]
    fn busy_controller_retries_but_an_uncertain_or_refused_outcome_is_final_and_alarmed() {
        let fake = Fake::new(plan(), target(), false);
        *fake.replies.lock().unwrap() = [
            Pause::Retry("input_busy: Shared controller is in use".into()),
            Pause::Final {
                text: "pause uncertain (action_uncertain: x); not retried, settle by hand".into(),
                failed: true,
            },
        ]
        .into();
        let status = Mutex::new(Status::default());
        step(&status, &fake);
        assert_eq!(state(&status)["fired"], false);
        assert_eq!(state(&status)["state"], "retrying");
        step(&status, &fake);
        let v = state(&status);
        assert_eq!(v["fired"], true);
        assert_eq!(v["alarm"]["code"], "date_stop_failed");
        assert!(v["alarm"]["message"].as_str().unwrap().contains("uncertain"));
        for _ in 0..3 {
            step(&status, &fake);
        }
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 2, "never re-issued after a final outcome");
    }

    #[test]
    fn pre_input_retries_are_capped_then_final() {
        let fake = Fake::new(plan(), target(), false);
        *fake.replies.lock().unwrap() =
            (0..MAX_ATTEMPTS).map(|_| Pause::Retry("torn_read: x".into())).collect();
        let status = Mutex::new(Status::default());
        for _ in 0..MAX_ATTEMPTS + 3 {
            step(&status, &fake);
        }
        assert_eq!(fake.pauses.load(Ordering::SeqCst), MAX_ATTEMPTS);
        let v = state(&status);
        assert_eq!(v["fired"], true);
        assert!(v["outcome"].as_str().unwrap().contains("not retried"));
        assert!(v["alarm"].is_object());
    }

    #[test]
    fn plan_formats_flags_and_changes_are_followed() {
        let status = Mutex::new(Status::default());
        let obj = Fake::new(json!({"stop_on_date": {"year": 1938, "month": 1, "day": 1}}), target(), false);
        step(&status, &obj);
        assert_eq!(obj.pauses.load(Ordering::SeqCst), 1);

        let off = Fake::new(json!({"stop_on_date": "1938.01.01", "stop_on_date_pause": false}), target(), false);
        let s2 = Mutex::new(Status::default());
        step(&s2, &off);
        assert_eq!(off.pauses.load(Ordering::SeqCst), 0);
        assert_eq!(state(&s2)["outcome"], "not requested (stop_on_date_pause false)");

        let inactive = Fake::new(json!({"stop_on_date": "1938.01.01", "active": false}), target(), false);
        let s3 = Mutex::new(Status::default());
        step(&s3, &inactive);
        assert_eq!(inactive.pauses.load(Ordering::SeqCst), 0);

        let bad = Fake::new(json!({"stop_on_date": "someday"}), target(), false);
        let s4 = Mutex::new(Status::default());
        step(&s4, &bad);
        assert_eq!(bad.pauses.load(Ordering::SeqCst), 0);
        assert!(state(&s4)["state"].as_str().unwrap().starts_with("invalid plan"));

        // a later date set after the first fired arms a fresh pause under a new id
        *obj.plan.lock().unwrap() = json!({"stop_on_date": "1938.02.01"});
        *obj.clock.lock().unwrap() = (target() + 24 * 40, false);
        obj.replies.lock().unwrap().push_back(Pause::Verified);
        step(&status, &obj);
        assert_eq!(obj.pauses.load(Ordering::SeqCst), 2);
        let ids = obj.ids.lock().unwrap();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn disarmed_watcher_reports_from_the_plan_file() {
        let dir = std::env::temp_dir().join(format!("date-stop-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.json");
        std::fs::write(&path, plan().to_string()).unwrap();
        let watcher = DateStop::default();
        let v = watcher.status_json(&path);
        assert_eq!(v["armed"], false);
        assert_eq!(v["state"], UNARMED);
        assert_eq!(v["target"], "1938.01.01");
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(watcher.status_json(&path)["target"], Value::Null);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn thread_fires_within_a_second_without_any_broker_call_and_joins_on_stop() {
        let fake = Arc::new(Fake::new(plan(), target() + 9 * 24, false));
        let watcher = DateStop::default();
        let env: Arc<dyn Env> = fake.clone();
        watcher.arm("tok".into(), |_| env);
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while fake.pauses.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(fake.pauses.load(Ordering::SeqCst), 1);
        let v = watcher.status_json(std::path::Path::new("/nonexistent/plan.json"));
        assert_eq!(v["running"], true);
        assert_eq!(v["fired"], true);
        watcher.stop();
        assert_eq!(watcher.status_json(std::path::Path::new("/nonexistent")) ["running"], false);
    }
}
