//! Bounded settlement shared by the stage journals (focus, research, construction, production).
//!
//! Every intent ends in a terminal receipt before its call returns. After the one dispatch the
//! journal re-observes the game, reading only, until the action's own deadline or for `window()`
//! after the dispatch, whichever ends later, and records what it saw: the target reached, a
//! settled contradiction, or nothing provable (`Settled::Unconfirmed`). A request closed without
//! proof is never replayed and blocks nothing: the caller reads the state again and each stage's
//! own preflight refuses a duplicate. Reading needs no input lease, so the window may run past
//! the lease; only a closing session ends it early.
//!
//! An intent can stay open only when the server stopped in the middle of an action. Its input
//! may still be in flight, so it holds its game for `ORPHAN_MS` from the time it was written;
//! after that the next call closes it. A record without a time (written by an older build) is
//! old by definition.
use crate::error::Error;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How long an intent left open by a stopped server holds its game.
pub(crate) const ORPHAN_MS: u64 = 10_000;

/// Least read-only observation after a dispatch, whatever is left of the action's deadline.
pub(crate) fn window() -> Duration {
    // Every stage test runs through this loop: keep outcomes that stay unknown fast.
    if cfg!(test) {
        Duration::from_millis(60)
    } else {
        Duration::from_millis(3_000)
    }
}
fn poll() -> Duration {
    Duration::from_millis(if cfg!(test) { 5 } else { 20 })
}
pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}
/// Whether an open intent written at `unix_ms` still holds its game at `now_ms`. A time in the
/// future (the wall clock was set back) counts by its distance, so it cannot hold for good.
pub(crate) fn holds(unix_ms: Option<u64>, now_ms: u64) -> bool {
    unix_ms.is_some_and(|at| now_ms.abs_diff(at) < ORPHAN_MS)
}
/// End of the read-only observation that follows a dispatch finished at `dispatched`.
pub(crate) fn end(deadline: Instant, dispatched: Instant) -> Instant {
    deadline.max(dispatched + window())
}

/// What one fresh observation says about an open intent.
pub(crate) enum Seen<S> {
    /// The exact target is present (and whatever else the stage requires for it).
    Reached(S),
    /// A settled state that contradicts the stage (it cannot become the target).
    Diverged(S, String),
    /// Readable, not the target yet (or a torn read): observe again until the end. The text
    /// is a remark worth keeping for the close reason, or empty.
    Pending(String),
    /// Not readable. Tolerated for one `window()` in a row, then the request closes.
    Unreadable(String),
    /// Nothing more can be read (the session is closing): close at once.
    Stop(String),
}
/// How an intent closed.
pub(crate) enum Settled<S> {
    Reached(S),
    Diverged(S, String),
    Unconfirmed(String),
}
/// Observe until the game shows an outcome or `end` passes. Sends nothing. The close reason
/// names how the observation ended and the last remark any observation left.
pub(crate) fn settle<S>(end: Instant, mut probe: impl FnMut(Instant) -> Seen<S>) -> Settled<S> {
    let started = Instant::now();
    let mut polls = 0u32;
    let mut unreadable_since: Option<Instant> = None;
    let mut remark = String::new();
    let ended = loop {
        polls += 1;
        match probe(end) {
            Seen::Reached(state) => return Settled::Reached(state),
            Seen::Diverged(state, why) => return Settled::Diverged(state, bounded(&why, 200)),
            Seen::Pending(why) => {
                unreadable_since = None;
                if !why.is_empty() {
                    remark = why;
                }
            }
            Seen::Unreadable(why) => {
                remark = why;
                if unreadable_since.get_or_insert_with(Instant::now).elapsed() >= window() {
                    break "state unreadable";
                }
            }
            Seen::Stop(why) => {
                remark = why;
                break "observation stopped";
            }
        }
        let now = Instant::now();
        if now >= end {
            break "target not observed";
        }
        std::thread::sleep(poll().min(end - now));
    };
    Settled::Unconfirmed(bounded(
        &format!(
            "{ended} (polls={polls} observed_ms={}); {remark}",
            started.elapsed().as_millis()
        ),
        200,
    ))
}
/// One bounded line without control characters.
pub(crate) fn bounded(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for c in text.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > limit {
            break;
        }
        out.push(c);
    }
    out
}
/// `code: message` of a read error as a bounded readback line.
pub(crate) fn readback(error: &Error) -> String {
    bounded(&format!("{}: {}", error.code, error.message), 200)
}
/// The typed error of a request closed without proof: a clean failure, not a quarantine.
pub(crate) fn unconfirmed(readback: &str) -> Error {
    Error::new(
        "action_unconfirmed",
        bounded(
            &format!(
                "Input may have been sent but its effect was not observed; the request is closed, \
                 is never replayed and blocks nothing. Read the state again and send a fresh \
                 request only if the effect is absent; readback={readback}"
            ),
            512,
        ),
    )
}
/// The typed error of a request whose observed state contradicts its stage. Closed like
/// `unconfirmed`.
pub(crate) fn diverged(why: &str) -> Error {
    Error::new(
        "action_unconfirmed",
        bounded(
            &format!(
                "Input may have been sent and the observed state contradicts the stage; the \
                 request is closed, is never replayed and blocks nothing. Read the state again \
                 before any fresh request; diverged={why}"
            ),
            512,
        ),
    )
}
/// The error while an interrupted action may still have input in flight.
pub(crate) fn interrupted(domain: &str) -> Error {
    Error::new(
        "action_uncertain",
        format!(
            "A {domain} action was interrupted moments ago and its input may still be in \
             flight; new input waits at most {} s",
            ORPHAN_MS / 1000
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn an_open_intent_holds_only_inside_the_orphan_bound() {
        let now = 1_000_000;
        assert!(holds(Some(now), now));
        assert!(holds(Some(now - ORPHAN_MS + 1), now));
        assert!(!holds(Some(now - ORPHAN_MS), now));
        // A record of an older build has no time: old by definition.
        assert!(!holds(None, now));
        // A wall clock set back cannot make an intent hold for good.
        assert!(holds(Some(now + ORPHAN_MS - 1), now));
        assert!(!holds(Some(now + ORPHAN_MS), now));
    }
    #[test]
    fn the_window_outlasts_an_expired_deadline_and_never_shortens_a_longer_one() {
        let now = Instant::now();
        assert_eq!(end(now, now), now + window());
        let late = now + window() * 4;
        assert_eq!(end(late, now), late);
    }
    #[test]
    fn settle_returns_the_first_outcome_and_closes_when_nothing_is_proven() {
        let far = Instant::now() + Duration::from_secs(5);
        let mut calls = 0;
        assert!(matches!(
            settle(far, |_| {
                calls += 1;
                if calls < 3 {
                    Seen::Pending("not yet".into())
                } else {
                    Seen::Reached(calls)
                }
            }),
            Settled::Reached(3)
        ));
        assert!(matches!(
            settle(far, |_| Seen::Diverged(7, "changed\nunder it".into())),
            Settled::Diverged(7, why) if why == "changedunder it"
        ));
        // Readable but never the target: closes when the end passes.
        let Settled::Unconfirmed(why) = settle(Instant::now(), |_| Seen::<()>::Pending("x".into()))
        else {
            panic!("closed")
        };
        assert!(
            why.starts_with("target not observed (polls=1 ") && why.ends_with("; x"),
            "{why}"
        );
        // Unreadable closes after one window even with a far end; a stop closes at once.
        let started = Instant::now();
        assert!(matches!(
            settle(far, |_| Seen::<()>::Unreadable("io_error: gone".into())),
            Settled::Unconfirmed(why) if why.starts_with("state unreadable") && why.ends_with("io_error: gone")
        ));
        assert!(started.elapsed() < Duration::from_secs(4));
        let mut calls = 0;
        assert!(matches!(
            settle(far, |_| {
                calls += 1;
                Seen::<()>::Stop("session_closed".into())
            }),
            Settled::Unconfirmed(why) if why.starts_with("observation stopped")
        ));
        assert_eq!(calls, 1);
    }
    #[test]
    fn closed_requests_answer_the_unconfirmed_code_in_one_bounded_line() {
        for error in [
            unconfirmed(&"é\n".repeat(400)),
            diverged(&"x\t".repeat(400)),
        ] {
            assert_eq!(error.code, "action_unconfirmed");
            assert!(error.message.len() <= 512);
            assert!(!error.message.chars().any(char::is_control));
        }
        assert_eq!(interrupted("focus").code, "action_uncertain");
    }
}
