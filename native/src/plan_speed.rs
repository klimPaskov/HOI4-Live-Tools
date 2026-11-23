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
//! A war to manage (value judgement): an operation of ours executing (from its execution
//! until it ends), a war an operation names declared within the last week (the declaration
//! before the operation executes), or being attacked: our casualties rose or an own state was
//! lost within the last week. A border with a country at war where nothing happens (holds
//! facing each other, no losses for a week: the phoney war with France from September 1939) is
//! peace for this rule: the speed of building the economy and the army. (w46c1, 23 Nov 2026:
//! the bordering enemy alone made the whole phoney war a "war" from the first tick.)
//!
//! A human's change is a speed read while the game runs that differs from what the rule set,
//! when the reading before it was also running at the rule's speed. A different speed after a
//! pause (the launcher's or the harness's own unpause, its start speed) is not a human's choice:
//! the rule sets its speed again. While paused the rule waits and its phase speed stays due.
//!
//! Speeds: `hoi4_time` and `hoi4_overview` count from 0 (internal speed 0-4 = the game's 1-5):
//! the game's speed 3 is internal 2. Done-memory (cleared on a phase change):
//! `spd:phase:<peace|war>`, `spd:set:<internal>` (the rule's speed this phase), `spd:human`,
//! `spd:last:<internal>:<1 running|0 paused>` (the reading before). Kept across phases:
//! `spdw:fight` (the last loss seen), `spdw:own` (own losses at the last read), `spdw:occ:<state>`,
//! `spdw:calm:<TAG>` (an operation's enemy seen not at war), `spdw:new:<TAG>` (its war began).
use super::{list, strings, GameHour, State, Tools};
use crate::error::Result;
use serde_json::{json, Value};

const RULE: &str = "speed";
/// How often the phase is read (game hours): a declaration is seen within a quarter of a day.
const READ_HOURS: GameHour = 6;
/// A loss, an own state lost or a planned war declared counts this long (a game week).
const QUIET: GameHour = 7 * 24;

/// The operations of the plan that execute now (executed, not ended), from the executor's
/// memory (`op:<id>:exec` without `op:<id>:end`); pure.
pub(super) fn executing_operations(state: &State) -> Vec<String> {
    let mut out: Vec<String> = state
        .done
        .keys()
        .filter_map(|k| k.strip_prefix("op:")?.strip_suffix(":exec"))
        .filter(|id| !id.contains(':'))
        .filter(|id| !state.done.contains_key(&format!("op:{id}:end")))
        .map(str::to_string)
        .collect();
    out.sort();
    out
}

/// The enemies at war (not won) from a hoi4_wars read; pure.
fn enemies_at_war(wars: &Value) -> Vec<String> {
    let won: Vec<&str> = list(&wars["wars"], "wars_won")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    list(&wars["wars"], "enemies")
        .iter()
        .filter_map(|t| t.as_str().or_else(|| t["tag"].as_str()).map(str::to_string))
        .filter(|t| !won.contains(&t.as_str()))
        .collect()
}

/// Updates the memory of what happened (losses, own states lost, planned wars declared) and
/// says why there is a war to manage now, or None; `stats` is a hoi4_war_stats read (None when
/// unread).
pub(super) fn war_reason(
    state: &mut State,
    plan: &Value,
    wars: &Value,
    stats: Option<&Value>,
    now: GameHour,
) -> Option<String> {
    let at_war = enemies_at_war(wars);
    // a planned war declared: an operation's enemy seen not at war before, at war now
    let planned: Vec<String> = list(plan, "operations")
        .iter()
        .flat_map(|op| strings(op, "enemies"))
        .collect();
    for tag in &planned {
        let calm = format!("spdw:calm:{tag}");
        let new = format!("spdw:new:{tag}");
        if !at_war.contains(tag) {
            state.done.insert(calm, now);
            state.done.remove(&new);
        } else if state.done.contains_key(&calm) {
            state.done.entry(new).or_insert(now);
        }
    }
    // being attacked: own losses rose, an own state was lost
    if let Some(own) = stats
        .and_then(super::plan_military::losses_by_enemy)
        .map(|l| l.values().map(|(o, _)| *o).sum::<f64>())
    {
        let own = own.round() as GameHour;
        let before = state.done.insert("spdw:own".into(), own);
        if before.is_some_and(|b| own > b) {
            state.done.insert("spdw:fight".into(), now);
        }
    }
    for occ in list(wars, "occupied_own_states") {
        let Some(id) = occ["state_id"].as_i64().or_else(|| occ.as_i64()) else {
            continue;
        };
        if state.done.insert(format!("spdw:occ:{id}"), now).is_none() {
            state.done.insert("spdw:fight".into(), now);
        }
    }
    let ops = executing_operations(state);
    if !ops.is_empty() {
        return Some(format!("operation {} executes", ops.join(", ")));
    }
    let declared: Vec<&String> = planned
        .iter()
        .filter(|t| {
            state
                .done
                .get(&format!("spdw:new:{t}"))
                .is_some_and(|at| now - *at < QUIET)
        })
        .collect();
    if !declared.is_empty() {
        return Some(format!(
            "the war with {} was declared this week",
            declared
                .iter()
                .map(|t| t.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if state
        .done
        .get("spdw:fight")
        .is_some_and(|at| now - *at < QUIET)
        && !at_war.is_empty()
    {
        return Some(format!(
            "it is attacked (losses or an own state lost this week; at war with {})",
            at_war.join(", ")
        ));
    }
    None
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
    /// The game already runs at the phase's speed: remember it as the rule's.
    Holds,
    /// Set this internal speed (0-4).
    Set(i64),
    /// A human changed the speed this phase: tell the player once.
    Human,
}

/// `set` is the rule's speed in this phase (None: not yet), `human` whether a human's change
/// was noted, `before` the reading before this one (internal speed, running); pure.
pub(super) fn decide(
    current: i64,
    paused: bool,
    want_internal: i64,
    set: Option<i64>,
    human: bool,
    before: Option<(i64, bool)>,
) -> Decision {
    if paused || human {
        return Decision::Keep;
    }
    match set {
        None if current == want_internal => Decision::Holds,
        None => Decision::Set(want_internal),
        Some(s) if s == current => Decision::Keep,
        // changed while the game ran at the rule's speed: a human chose it
        Some(s) if before == Some((s, true)) => Decision::Human,
        // changed across a pause or before the rule saw it run (a launcher's unpause, its
        // start speed): the rule's speed again
        Some(s) => Decision::Set(s),
    }
}

fn phase_set(state: &State) -> Option<i64> {
    state
        .done
        .keys()
        .find_map(|k| k.strip_prefix("spd:set:").and_then(|v| v.parse().ok()))
}

fn last_read(state: &State) -> Option<(i64, bool)> {
    state.done.keys().find_map(|k| {
        let (speed, running) = k.strip_prefix("spd:last:")?.split_once(':')?;
        Some((speed.parse().ok()?, running == "1"))
    })
}

/// The fast lane: at most one `hoi4_time` a phase (and again after a launcher's unpause).
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
    let stats = tools.call("hoi4_war_stats", &json!({})).ok();
    let reason = war_reason(state, plan, &wars, stats.as_ref(), now);
    let war = reason.is_some();
    let phase = if war { "war" } else { "peace" };
    if !state.done.contains_key(&format!("spd:phase:{phase}")) {
        state.done.retain(|k, _| !k.starts_with("spd:"));
        state.done.insert(format!("spd:phase:{phase}"), now);
    }
    let game = wanted(plan, war);
    let want = game - 1;
    let set = phase_set(state);
    let human = state.done.contains_key("spd:human");
    let before = last_read(state);
    state.done.retain(|k, _| !k.starts_with("spd:last:"));
    state.done.insert(
        format!("spd:last:{current}:{}", if paused { 0 } else { 1 }),
        now,
    );
    let against = match &reason {
        Some(r) => format!(" while it manages a war ({r})"),
        None => " with no war to manage".to_string(),
    };
    match decide(current, paused, want, set, human, before) {
        Decision::Keep => Ok(None),
        Decision::Holds => {
            state.done.insert(format!("spd:set:{current}"), now);
            Ok(None)
        }
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
            state.done.retain(|k, _| !k.starts_with("spd:set:"));
            // a refused change is not re-sent on a beat: the game's speed counts as the rule's
            state
                .done
                .insert(format!("spd:set:{}", if ok { speed } else { current }), now);
            if ok {
                state.done.retain(|k, _| !k.starts_with("spd:last:"));
                state.done.insert(format!("spd:last:{speed}:1"), now);
            }
            let line = format!(
                "speed {} -> {}{against}: {ok} {why}",
                current + 1,
                speed + 1
            );
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

    fn stats(own: f64) -> Value {
        json!({"player": {"tag": "GER"}, "wars": [{"includes_player": true, "relations": [
            {"countries": [{"tag": "GER", "casualties": own}, {"tag": "FRA", "casualties": 10.0}]}]}]})
    }

    const DAY: GameHour = 24;

    #[test]
    fn the_phoney_war_is_peace_an_executing_operation_or_an_attack_is_war() {
        let plan = json!({"operations": [{"id": "op_low", "enemies": ["HOL", "BEL"]},
                                         {"id": "op_fra", "enemies": ["FRA"]}]});
        let mut st = State::default();
        // Germany borders France at war since 1939, nothing happens: peace for this rule
        let phoney = wars(&["FRA", "ENG", "POL"], &["FRA", "HOL"]);
        let t0 = 1000;
        assert_eq!(
            war_reason(&mut st, &plan, &phoney, Some(&stats(500.0)), t0),
            None
        );
        assert_eq!(
            war_reason(&mut st, &plan, &phoney, Some(&stats(500.0)), t0 + DAY),
            None
        );
        // Holland declared on: war at once (before the operation executes)
        let hol = wars(&["FRA", "ENG", "HOL"], &["FRA", "HOL"]);
        let r = war_reason(&mut st, &plan, &hol, Some(&stats(500.0)), t0 + 2 * DAY).unwrap();
        assert!(r.contains("HOL"), "{r}");
        // the operation executes until it ends, even past the week
        st.done.insert("op:op_low:exec".into(), t0 + 3 * DAY);
        let r = war_reason(&mut st, &plan, &hol, Some(&stats(500.0)), t0 + 12 * DAY).unwrap();
        assert!(r.contains("op_low executes"), "{r}");
        st.done.insert("op:op_low:end".into(), t0 + 20 * DAY);
        assert_eq!(executing_operations(&st), Vec::<String>::new());
        // ended, the French border quiet: peace again (France was never seen at peace)
        assert_eq!(
            war_reason(&mut st, &plan, &hol, Some(&stats(500.0)), t0 + 21 * DAY),
            None
        );
        // France attacks: own losses rise, war for a week after the last loss
        assert!(war_reason(&mut st, &plan, &hol, Some(&stats(900.0)), t0 + 22 * DAY).is_some());
        assert!(war_reason(&mut st, &plan, &hol, Some(&stats(900.0)), t0 + 28 * DAY).is_some());
        assert_eq!(
            war_reason(&mut st, &plan, &hol, Some(&stats(900.0)), t0 + 30 * DAY),
            None
        );
        // an own state lost: war
        let mut lost = hol.clone();
        lost["occupied_own_states"] = json!([{"state_id": 42, "controller": "FRA"}]);
        assert!(war_reason(&mut st, &plan, &lost, None, t0 + 31 * DAY).is_some());
        // still held a week later with nothing new: the attack is old news
        assert_eq!(war_reason(&mut st, &plan, &lost, None, t0 + 39 * DAY), None);
    }

    use std::cell::RefCell;
    use std::collections::HashMap;

    struct Game {
        replies: RefCell<HashMap<&'static str, Value>>,
        sent: RefCell<Vec<Value>>,
    }
    impl Game {
        fn new(speed: i64, paused: bool, wars: Value) -> Game {
            Game {
                replies: RefCell::new(
                    [
                        ("hoi4_overview", json!({"speed": speed, "paused": paused})),
                        ("hoi4_wars", wars),
                    ]
                    .into_iter()
                    .collect(),
                ),
                sent: RefCell::new(Vec::new()),
            }
        }
        fn screen(&self, speed: i64, paused: bool) {
            self.replies
                .borrow_mut()
                .insert("hoi4_overview", json!({"speed": speed, "paused": paused}));
        }
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
    fn a_phoney_war_runs_at_three_and_the_launchers_unpause_is_not_a_human() {
        // w46c1: the bed starts paused at the tool's speed 2 (the game's 3) with France at war
        let plan = json!({"operations": [{"id": "op_low", "enemies": ["HOL"]}]});
        let tools = Game::new(2, true, wars(&["FRA", "POL"], &["FRA", "HOL"]));
        let mut st = State::default();
        assert!(step(&mut st, &tools, &plan, "l", 1000).unwrap().is_none());
        // the launcher unpauses at the game's speed 4 (internal 3): the peace speed 3 is set
        tools.screen(3, false);
        let out = step(&mut st, &tools, &plan, "l", 1006).unwrap().unwrap();
        assert!(out.contains("speed 4 -> 3 with no war to manage"), "{out}");
        // the harness pauses and unpauses at the game's speed 2: not a human, set again
        tools.screen(1, true);
        assert!(step(&mut st, &tools, &plan, "l", 1012).unwrap().is_none());
        tools.screen(1, false);
        assert!(step(&mut st, &tools, &plan, "l", 1018).unwrap().is_some());
        assert!(!st.done.contains_key("spd:human"));
        assert_eq!(tools.sent.borrow().len(), 2);
        // Holland declared on: the game's speed 2
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&["FRA", "POL", "HOL"], &["FRA", "HOL"]));
        let out = step(&mut st, &tools, &plan, "l", 1024).unwrap().unwrap();
        assert!(out.contains("speed 3 -> 2") && out.contains("HOL"), "{out}");
    }

    #[test]
    fn a_humans_change_while_running_is_kept_for_the_phase() {
        let plan = json!({"operations": [{"id": "op_low", "enemies": ["POL"]}]});
        let tools = Game::new(2, false, wars(&[], &["POL"]));
        let mut st = State::default();
        // peace at the game's speed 3: nothing sent, remembered as the rule's
        assert!(step(&mut st, &tools, &plan, "l", 1000).unwrap().is_none());
        // war with Poland: the game's speed 2 (internal 1)
        tools
            .replies
            .borrow_mut()
            .insert("hoi4_wars", wars(&["POL"], &["POL"]));
        let out = step(&mut st, &tools, &plan, "l", 1006).unwrap().unwrap();
        assert!(out.contains("speed 3 -> 2"), "{out}");
        assert_eq!(tools.sent.borrow()[0]["speed"], json!(1));
        // a human raises it to 3 while it runs: one note, never set again in this war
        tools.screen(2, false);
        assert!(step(&mut st, &tools, &plan, "l", 1012).unwrap().is_none());
        assert!(st.done.contains_key("spd:human"));
        assert!(step(&mut st, &tools, &plan, "l", 1018).unwrap().is_none());
        assert_eq!(
            tools.sent.borrow().len(),
            1,
            "no retry after a human's change"
        );
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
    fn the_phase_speed_is_set_and_only_a_change_while_running_is_a_humans() {
        // the phase's speed already runs: remembered
        assert_eq!(decide(2, false, 2, None, false, None), Decision::Holds);
        // war starts: speed 2 (internal 1)
        assert_eq!(decide(2, false, 1, None, false, None), Decision::Set(1));
        // paused: waits, the speed stays due
        assert_eq!(decide(2, true, 1, None, false, None), Decision::Keep);
        // it holds
        assert_eq!(
            decide(1, false, 1, Some(1), false, Some((1, true))),
            Decision::Keep
        );
        // changed while it ran at the rule's speed: a human
        assert_eq!(
            decide(2, false, 1, Some(1), false, Some((1, true))),
            Decision::Human
        );
        assert_eq!(
            decide(2, false, 1, Some(1), true, Some((2, true))),
            Decision::Keep
        );
        // changed across a pause: the launcher's unpause, set again
        assert_eq!(
            decide(2, false, 1, Some(1), false, Some((1, false))),
            Decision::Set(1)
        );
    }
}
