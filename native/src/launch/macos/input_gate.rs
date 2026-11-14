//! Both ordinary action journals share one input gate. Every request closes itself
//! with a terminal receipt, so the gate only holds input while a request interrupted
//! moments ago may still have input in flight (game journal, bounded) or while an open
//! clock request could not be read back yet (settled by the next readable call).
use super::journal::GameJournal;
use crate::{
    actions::Journal,
    error::{Error, Result},
    platform::Identity,
};

/// The quarantine only guards tools that can send input, i.e. those that take a
/// controller `lease_token`. Pure reads (no lease, no input) stay available while an
/// action is unresolved, so the player can still observe the game and settle it.
pub(super) fn applies(name: &str, args: &serde_json::Map<String, serde_json::Value>) -> bool {
    name != "hoi4_input_lease" && args.contains_key("lease_token")
}

pub(super) fn check(
    clock: &Journal,
    game: &GameJournal,
    identity: &Identity,
    is_clock: bool,
    request_id: Option<&str>,
) -> Result<()> {
    check_in(clock, game, identity, is_clock, request_id, "*")
}

/// `check` for a tool of one input domain: an unresolved game action blocks only its own
/// domain (and global ones block all). The clock journal still blocks every game path.
pub(super) fn check_in(
    clock: &Journal,
    game: &GameJournal,
    identity: &Identity,
    is_clock: bool,
    request_id: Option<&str>,
    domain: &str,
) -> Result<()> {
    let blocked = if is_clock {
        game.blocked()
    } else {
        clock.blocks_game(identity)
            || (game.blocked_for(domain) && !game.permits_recovery(request_id))
    };
    if blocked {
        return Err(Error::new(
            "action_uncertain",
            "Shared Mac input journal is unresolved; only its own read-only settlement is allowed",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_tools_that_can_send_input_are_quarantined() {
        let with: serde_json::Map<String, serde_json::Value> =
            serde_json::from_value(serde_json::json!({"lease_token": "t"})).unwrap();
        let without = serde_json::Map::new();
        assert!(super::applies("hoi4_time", &with));
        assert!(!super::applies("hoi4_peace_options", &without));
        assert!(!super::applies("hoi4_input_lease", &with));
    }
    use super::*;
    use crate::actions::{Action, Backend, Epoch, State};
    use serde_json::json;
    use std::time::Duration;

    fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 7,
                start_ticks: "1.000001".into(),
                exe: "/fixture".into(),
                uid: 501,
            },
            campaign: "nonce:playthrough".into(),
            player: "GER".into(),
        }
    }
    /// A clock whose key never takes effect. `readable: false`: once the key is out the game
    /// cannot be read at all.
    struct LostClock {
        readable: bool,
        sent: bool,
    }
    impl LostClock {
        fn new(readable: bool) -> Self {
            Self {
                readable,
                sent: false,
            }
        }
    }
    impl Backend for LostClock {
        fn observe(&mut self) -> Result<State> {
            if self.sent && !self.readable {
                return Err(Error::new("read_failed", "Game not readable"));
            }
            Ok(State {
                epoch: epoch(),
                paused: true,
                speed: 1,
            })
        }
        fn preflight(&mut self, _: &Action, _: &State) -> Result<()> {
            Ok(())
        }
        fn dispatch(&mut self, _: &Action, _: &State) -> Result<()> {
            self.sent = true;
            Err(Error::new("fixture_transport", "Possible input"))
        }
    }
    fn dir(label: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "hoi4-mac-cross-journal-{}-{label}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir(&p).unwrap();
        p
    }
    fn interrupted(game_file: &std::path::Path, kind: &str, age_ms: u64) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let line = json!({
            "version": 1, "id": "lost", "epoch": epoch(), "action": {"kind": kind},
            "status": "Dispatching", "detail": null, "unix_ms": now - age_ms,
        });
        std::fs::write(game_file, format!("{line}\n")).unwrap();
    }
    #[test]
    fn a_completed_uncertain_game_action_blocks_neither_clock_nor_game_input() {
        let p = dir("closed");
        let clock = Journal::open(&p.join("clock")).unwrap();
        let mut game = GameJournal::open(&p.join("game")).unwrap();
        // w25: a focus action ran out of time and nothing could be read back.
        let error = game
            .execute(
                "lost",
                &epoch(),
                json!({"kind":"focus_start"}),
                &mut || {
                    Err(Error::new(
                        "action_deadline",
                        "Original action deadline expired",
                    ))
                },
                &mut || {
                    Err(Error::new(
                        "action_deadline",
                        "Original action deadline expired",
                    ))
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "action_unconfirmed");
        for (is_clock, id, domain) in [
            (true, None, "*"),
            (false, None, "*"),
            (false, Some("event-answer"), "*"),
            (false, Some("next-focus"), "focus"),
            (false, Some("army"), "army"),
        ] {
            check_in(&clock, &game, &epoch().game, is_clock, id, domain).unwrap();
        }
        drop((clock, game));
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn an_interrupted_game_action_holds_input_only_while_its_input_may_be_in_flight() {
        let p = dir("game");
        let clock = Journal::open(&p.join("clock")).unwrap();
        interrupted(&p.join("game"), "event_choose", 0);
        let game = GameJournal::open(&p.join("game")).unwrap();
        for (is_clock, id) in [
            (true, None),
            (true, Some("lost")),
            (false, None),
            (false, Some("replacement")),
        ] {
            assert_eq!(
                check(&clock, &game, &epoch().game, is_clock, id)
                    .unwrap_err()
                    .code,
                "action_uncertain"
            );
        }
        // Its own request may always take the read-only settlement path.
        check(&clock, &game, &epoch().game, false, Some("lost")).unwrap();
        drop(game);
        // Ten seconds later nothing is held any more.
        interrupted(&p.join("game"), "event_choose", 10_000);
        let game = GameJournal::open(&p.join("game")).unwrap();
        for (is_clock, id) in [(true, None), (false, None), (false, Some("replacement"))] {
            check(&clock, &game, &epoch().game, is_clock, id).unwrap();
        }
        drop((clock, game));
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn an_unreadable_clock_request_holds_game_input_until_one_fresh_read_settles_it() {
        let p = dir("clock");
        let mut clock = Journal::open(&p.join("clock")).unwrap();
        let game = GameJournal::open(&p.join("game")).unwrap();
        // The key went out and the game could not be read afterwards.
        clock
            .execute(
                "lost-clock",
                &epoch(),
                Action::SetPaused(false),
                &mut LostClock::new(false),
                Duration::from_millis(1),
            )
            .unwrap_err();
        drop(clock);
        let mut clock = Journal::open(&p.join("clock")).unwrap();
        for id in [None, Some("lost-clock"), Some("new-game")] {
            assert_eq!(
                check(&clock, &game, &epoch().game, false, id)
                    .unwrap_err()
                    .code,
                "action_uncertain"
            );
        }
        check(&clock, &game, &epoch().game, true, Some("lost-clock")).unwrap();
        // One fresh read (what the dispatcher does before the gate) closes it.
        clock
            .settle_open(&epoch(), &mut || {
                Ok(State {
                    epoch: epoch(),
                    paused: true,
                    speed: 1,
                })
            })
            .unwrap();
        for id in [None, Some("new-game")] {
            check(&clock, &game, &epoch().game, false, id).unwrap();
        }
        drop((clock, game));
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn a_clock_key_without_effect_is_a_clean_failure_that_holds_nothing() {
        let p = dir("clock-clean");
        let mut clock = Journal::open(&p.join("clock")).unwrap();
        let game = GameJournal::open(&p.join("game")).unwrap();
        let error = clock
            .execute(
                "no-effect",
                &epoch(),
                Action::SetPaused(false),
                &mut LostClock::new(true),
                Duration::from_millis(1),
            )
            .unwrap_err();
        assert_eq!(error.code, "fixture_transport");
        assert!(error.message.contains("NotApplied"));
        for id in [None, Some("new-game")] {
            check(&clock, &game, &epoch().game, false, id).unwrap();
        }
        drop((clock, game));
        std::fs::remove_dir_all(p).unwrap();
    }
}
