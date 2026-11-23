//! Game speed by the war the country manages (rule `speed`, manage.off "speed"; plan key
//! `speed` {"peace": 3, "war": 2} in the game's own speeds 1-5, defaults 3 and 2).
//!
//! Owner, 23 Nov 2026: while the country only builds its economy and army, the game's speed 3
//! is the default; while it has a war to manage, speed 2. Value judgement: at war an order and
//! its read-back cost about a game day each at speed 3, and the war's orders are the ones whose
//! timing decides battles; in peace the same delay costs nothing that compounds. In multiplayer
//! the human sets the pace: the rule sets the speed only when the phase changes (a war to manage
//! begins or ends), remembers what it set, and when a human changes the speed inside the phase
//! (higher or lower) it never sets it again for that phase: it tells the player once (a note in
//! the run result's `manage.speed` and a `wake_player` alarm; the server has no in-game chat
//! tool) and plays on. It never pauses or unpauses and never sets speed 4 or 5.
//!
//! A war to manage (value judgement): a war with a country that borders us (hoi4_wars borders,
//! a hostile neighbour), a country one of the plan's operations names (`operations[].enemies`),
//! or any war while an own state is held by the enemy. A distant paper war alone (a dominion of
//! an enemy across the sea that shares no border and no operation) changes nothing the executor
//! orders, so it keeps the peace speed.
//!
//! Speeds: `hoi4_time` and `hoi4_overview` count from 0 (internal speed 0-4 = the game's 1-5):
//! the game's speed 3 is internal 2. Done-memory: `spd:phase:<peace|war>`,
//! `spd:set:<internal>` (what the rule set in this phase), `spd:human` (a human changed it).
use super::{list, strings, text, GameHour, State, Tools};
use crate::error::Result;
use serde_json::{json, Value};

const RULE: &str = "speed";
/// How often the phase is read (game hours): a declaration is seen within a quarter of a day.
const READ_HOURS: GameHour = 6;

/// The phase and the enemies that make it a war to manage; pure.
pub(super) fn managed_enemies(wars: &Value, plan: &Value) -> Vec<String> {
    let won: Vec<&str> = list(&wars["wars"], "wars_won")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let enemies: Vec<String> = list(&wars["wars"], "enemies")
        .iter()
        .filter_map(|t| t.as_str().or_else(|| t["tag"].as_str()).map(str::to_string))
        .filter(|t| !won.contains(&t.as_str()))
        .collect();
    if enemies.is_empty() {
        return Vec::new();
    }
    if !list(wars, "occupied_own_states").is_empty() {
        return enemies;
    }
    let bordering: Vec<&str> = list(wars, "borders")
        .iter()
        .map(|b| text(&b["neighbour_tag"]))
        .collect();
    let planned: Vec<String> = list(plan, "operations")
        .iter()
        .flat_map(|op| strings(op, "enemies"))
        .collect();
    enemies
        .into_iter()
        .filter(|t| bordering.contains(&t.as_str()) || planned.contains(t))
        .collect()
}

/// The game speed (1-5) the plan wants for a phase, never above 3; pure.
pub(super) fn wanted(plan: &Value, war: bool) -> i64 {
    let (key, default) = if war { ("war", 2) } else { ("peace", 3) };
    plan["speed"][key].as_i64().unwrap_or(default).clamp(1, 3)
}

/// What the rule does now; pure.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Decision {
    /// Nothing (the speed is what the phase wants, the game is paused, or a human chose).
    Keep,
    /// Set this internal speed (0-4).
    Set(i64),
    /// A human changed the speed this phase: tell the player once.
    Human,
}

/// `set` is what the rule set in this phase (None: it has not acted in this phase), `human`
/// whether a human change was already noted, `fresh` whether the phase just began; pure.
pub(super) fn decide(
    current: i64,
    paused: bool,
    want_internal: i64,
    set: Option<i64>,
    human: bool,
    fresh: bool,
) -> Decision {
    if paused || human {
        return Decision::Keep;
    }
    match set {
        // the rule set it this phase and the game shows another speed: a human chose it
        Some(s) if s != current => Decision::Human,
        Some(_) => Decision::Keep,
        // the phase began: the phase's speed once (the run's first reading too)
        None if fresh && current != want_internal => Decision::Set(want_internal),
        None => Decision::Keep,
    }
}

fn phase_set(state: &State) -> Option<i64> {
    state
        .done
        .keys()
        .find_map(|k| k.strip_prefix("spd:set:").and_then(|v| v.parse().ok()))
}

/// The fast lane: at most one `hoi4_time` a phase.
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if strings(&plan["manage"], "off").iter().any(|r| r == RULE)
        || plan["speed"] == json!(false)
        || state.skipped(now, "spd", "x")
    {
        return Ok(None);
    }
    state.skip(now, "spd", "x", READ_HOURS);
    let Ok(overview) = tools.call("hoi4_overview", &json!({})) else {
        return Ok(None);
    };
    let (Some(current), paused) = (
        overview["speed"].as_i64(),
        overview["paused"].as_bool().unwrap_or(false),
    ) else {
        return Ok(None);
    };
    let Ok(wars) = tools.call("hoi4_wars", &json!({})) else {
        return Ok(None);
    };
    let enemies = managed_enemies(&wars, plan);
    let war = !enemies.is_empty();
    let phase = if war { "war" } else { "peace" };
    let fresh = !state.done.contains_key(&format!("spd:phase:{phase}"));
    if fresh {
        state.done.retain(|k, _| !k.starts_with("spd:"));
        state.done.insert(format!("spd:phase:{phase}"), now);
    }
    let game = wanted(plan, war);
    let want = game - 1;
    let set = phase_set(state);
    let human = state.done.contains_key("spd:human");
    let against = if war {
        format!(" while it manages the war with {}", enemies.join(", "))
    } else {
        " in peace".to_string()
    };
    match decide(current, paused, want, set, human, fresh) {
        Decision::Keep => Ok(None),
        Decision::Human => {
            state.done.insert("spd:human".into(), now);
            let message = format!(
                "the game runs at speed {} (set by a human); the agent prefers speed {game}{against}: at speed 3 an order and its read-back cost about a game day each. It keeps the human's speed for this {phase} and does not change it again",
                current + 1
            );
            super::plan_manage::tell_player(state, plan, now, RULE, "speed:human", message);
            Ok(None)
        }
        Decision::Set(speed) => {
            let (ok, why) = state.act(tools, lease, "hoi4_time", json!({"speed": speed}), "spd")?;
            if ok {
                state.done.insert(format!("spd:set:{speed}"), now);
            } else {
                // one try a phase: a refused change is not re-sent on a beat
                state.done.insert(format!("spd:set:{current}"), now);
            }
            let line = format!("speed {} -> {game}{against}: {ok} {why}", current + 1);
            super::plan_manage::note(state, RULE, line.clone());
            Ok(Some(line))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wars(enemies: &[&str], borders: &[&str]) -> Value {
        json!({"wars": {"enemies": enemies, "wars_won": []},
            "borders": borders.iter().map(|t| json!({"neighbour_tag": t, "hostile": true})).collect::<Vec<_>>(),
            "occupied_own_states": []})
    }

    #[test]
    fn a_war_to_manage_borders_us_or_is_an_operations_enemy() {
        let plan = json!({"operations": [{"enemies": ["HOL"]}]});
        assert!(
            managed_enemies(&wars(&[], &["FRA"]), &plan).is_empty(),
            "peace"
        );
        // a distant paper war alone: peace speed
        assert!(managed_enemies(&wars(&["AST", "NZL"], &["FRA"]), &plan).is_empty());
        assert_eq!(
            managed_enemies(&wars(&["AST", "POL"], &["POL"]), &plan),
            vec!["POL"]
        );
        assert_eq!(managed_enemies(&wars(&["HOL"], &[]), &plan), vec!["HOL"]);
        // an own state held by the enemy makes any war one to manage
        let mut held = wars(&["AST"], &[]);
        held["occupied_own_states"] = json!([12]);
        assert_eq!(managed_enemies(&held, &plan), vec!["AST"]);
    }

    use std::cell::RefCell;
    use std::collections::HashMap;

    struct Game {
        replies: RefCell<HashMap<&'static str, Value>>,
        sent: RefCell<Vec<Value>>,
    }
    impl Tools for Game {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if args.get("request_id").is_some() {
                self.sent.borrow_mut().push(args.clone());
                if let Some(s) = args["speed"].as_i64() {
                    self.replies.borrow_mut().get_mut("hoi4_overview").unwrap()["speed"] = json!(s);
                }
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            Ok(self
                .replies
                .borrow()
                .get(name)
                .cloned()
                .unwrap_or_else(|| json!({})))
        }
    }

    #[test]
    fn peace_war_a_humans_speed_and_the_end_of_the_war() {
        let plan = json!({});
        let tools = Game {
            replies: RefCell::new(
                [
                    ("hoi4_overview", json!({"speed": 2, "paused": false})),
                    ("hoi4_wars", wars(&[], &["POL"])),
                ]
                .into_iter()
                .collect(),
            ),
            sent: RefCell::new(Vec::new()),
        };
        let mut st = State::default();
        // peace at the game's speed 3: nothing sent
        assert!(step(&mut st, &tools, &plan, "l", 1000).unwrap().is_none());
        // war with Poland: the game's speed 2 (internal 1)
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&["POL"], &["POL"]));
        let out = step(&mut st, &tools, &plan, "l", 1006).unwrap().unwrap();
        assert!(
            out.contains("speed 3 -> 2 while it manages the war with POL"),
            "{out}"
        );
        assert_eq!(tools.sent.borrow()[0]["speed"], json!(1));
        // a human raises it to 3: one note, never set again in this war
        tools.replies.borrow_mut().get_mut("hoi4_overview").unwrap()["speed"] = json!(2);
        assert!(step(&mut st, &tools, &plan, "l", 1012).unwrap().is_none());
        assert!(st.done.contains_key("spd:human"));
        assert!(step(&mut st, &tools, &plan, "l", 1018).unwrap().is_none());
        assert_eq!(
            tools.sent.borrow().len(),
            1,
            "no retry after a human's change"
        );
        // the war ends: back to 3 only if it is not 3 already (the human's 3 stays)
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&[], &["POL"]));
        assert!(step(&mut st, &tools, &plan, "l", 1024).unwrap().is_none());
        assert_eq!(tools.sent.borrow().len(), 1);
        // a new war while a human had it at 3: the war speed again (a new phase)
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&["FRA"], &["FRA"]));
        assert!(step(&mut st, &tools, &plan, "l", 1030).unwrap().is_some());
        // a human lowers it to 1: kept, told once
        tools.replies.borrow_mut().get_mut("hoi4_overview").unwrap()["speed"] = json!(0);
        assert!(step(&mut st, &tools, &plan, "l", 1036).unwrap().is_none());
        assert!(step(&mut st, &tools, &plan, "l", 1042).unwrap().is_none());
        assert_eq!(tools.sent.borrow().len(), 2);
        // the switch
        let off = json!({"manage": {"off": ["speed"]}});
        let mut fresh = State::default();
        assert!(step(&mut fresh, &tools, &off, "l", 1000).unwrap().is_none());
    }

    #[test]
    fn peace_three_war_two_never_above_three() {
        assert_eq!(wanted(&json!({}), false), 3);
        assert_eq!(wanted(&json!({}), true), 2);
        assert_eq!(wanted(&json!({"speed": {"peace": 5}}), false), 3);
    }

    #[test]
    fn the_phase_speed_is_set_once_and_a_humans_choice_is_kept() {
        // peace, the run's first reading at the game's speed 3 (internal 2): nothing
        assert_eq!(decide(2, false, 2, None, false, true), Decision::Keep);
        // war starts: speed 2 (internal 1)
        assert_eq!(decide(2, false, 1, None, false, true), Decision::Set(1));
        // it holds: nothing
        assert_eq!(decide(1, false, 1, Some(1), false, false), Decision::Keep);
        // a human raised it to 3: told once, never set again this war
        assert_eq!(decide(2, false, 1, Some(1), false, false), Decision::Human);
        assert_eq!(decide(2, false, 1, Some(1), true, false), Decision::Keep);
        // a human lowered it to 1: kept too
        assert_eq!(decide(0, false, 1, Some(1), false, false), Decision::Human);
        // the war ends: back to 3 (a new phase)
        assert_eq!(decide(1, false, 2, None, false, true), Decision::Set(2));
        // paused: never touched
        assert_eq!(decide(2, true, 1, None, false, true), Decision::Keep);
    }
}
