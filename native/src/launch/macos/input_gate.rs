//! Both ordinary action journals share one input quarantine. Settlement is
//! read-only and remains confined to the journal that recorded the request.
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
    let blocked = if is_clock {
        game.blocked()
    } else {
        clock.blocks_game(identity) || (game.blocked() && !game.permits_recovery(request_id))
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
    use super::super::journal::Outcome;
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
    struct LostClock;
    impl Backend for LostClock {
        fn observe(&mut self) -> Result<State> {
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
    #[test]
    fn unresolved_game_blocks_clock_and_unjournaled_ui_but_allows_own_settlement() {
        let p = dir("game");
        let clock = Journal::open(&p.join("clock")).unwrap();
        let mut game = GameJournal::open(&p.join("game")).unwrap();
        let action = json!({"kind":"research_start"});
        game.execute(
            "lost",
            &epoch(),
            action.clone(),
            &mut || Ok(Outcome::Unknown),
            &mut || Ok(()),
        )
        .unwrap_err();
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
        check(&clock, &game, &epoch().game, false, Some("lost")).unwrap();
        game.execute(
            "lost",
            &epoch(),
            action,
            &mut || Ok(Outcome::Reached(json!({}))),
            &mut || panic!("Settlement cannot dispatch"),
        )
        .unwrap();
        check(&clock, &game, &epoch().game, true, Some("new-clock")).unwrap();
        drop((clock, game));
        std::fs::remove_dir_all(p).unwrap();
    }
    #[test]
    fn unresolved_clock_blocks_all_game_paths_and_reopen_retains_it() {
        let p = dir("clock");
        let mut clock = Journal::open(&p.join("clock")).unwrap();
        let game = GameJournal::open(&p.join("game")).unwrap();
        clock
            .execute(
                "lost-clock",
                &epoch(),
                Action::SetPaused(false),
                &mut LostClock,
                Duration::from_millis(1),
            )
            .unwrap_err();
        drop(clock);
        let clock = Journal::open(&p.join("clock")).unwrap();
        for id in [None, Some("lost-clock"), Some("new-game")] {
            assert_eq!(
                check(&clock, &game, &epoch().game, false, id)
                    .unwrap_err()
                    .code,
                "action_uncertain"
            );
        }
        check(&clock, &game, &epoch().game, true, Some("lost-clock")).unwrap();
        drop((clock, game));
        std::fs::remove_dir_all(p).unwrap();
    }
}
