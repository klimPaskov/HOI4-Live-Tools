//! THE EXECUTION MODE BY VALUE: what the operation's attackers' battles and losses say about how
//! hard to press. Pure functions over the game's own reads (hoi4_battles, hoi4_war_stats); the
//! step that sends the order is `war_modes` in plan_operation.rs.
//!
//! What the game's modes change (common/defines, NMilitary and NAI): the plan attacks a province
//! only when its score reaches PLAN_EXECUTE_CAREFUL_LIMIT / BALANCED_LIMIT / RUSH (25 / 0 / -200
//! in vanilla), and a division attacks only above PLAN_ATTACK_MIN_ORG_FACTOR LOW / MED / HIGH of
//! its organisation (0.85 / 0.70 / 0.45) and PLAN_ATTACK_MIN_STRENGTH_FACTOR (0.60 / 0.50 / 0.30);
//! only careful waits for its supply (PLAN_EXECUTE_SUPPLY_CHECK) and spares forts. So balanced
//! leaves every province whose odds read below even (forest, a town, a river, one direction) and
//! stops a division at 70% organisation; rush attacks every province of the plan.
//!
//! The rule: press (rush) while each loss of ours costs the enemy at least as much; ease to
//! balanced when our losses outrun theirs; careful only when they cost us twice what they cost
//! the enemy and most battles are lost. With too few losses to judge, the battles decide: not
//! mostly lost, press. o96a (28 Nov 2026, the Low Countries by hand): balanced from W to W+21
//! took 7 of Holland's 91 points in 3-7 battles on 7-11 enemy provinces in reach, its window
//! losses 0.92 enemy to 1 own; rush from W+21 took Holland in 6 days at 1.10, then Belgium at 4.5.

use super::{list, text};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, HashSet};

/// The modes in the order their index is kept in the done-memory.
pub(super) const MODES: [&str; 3] = ["careful", "balanced", "rush"];

/// What the battles of one attacker read: its battles, those it wins (the enemy side breaks
/// first), those it loses, and the countries it fights there.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Odds {
    pub battles: usize,
    pub won: usize,
    pub lost: usize,
    pub tags: BTreeSet<String>,
}

/// The rule's numbers (`execution_rule` in the operation; every key optional).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Rule {
    /// enemy losses per own loss at or above which the attack presses (rush)
    pub rush_ratio: f64,
    /// enemy losses per own loss under which, with most battles lost, it turns careful
    pub careful_ratio: f64,
    /// own plus enemy losses a window needs before its ratio is read
    pub min_losses: f64,
    /// game days between two readings of one attacker
    pub days: i64,
}

impl Rule {
    pub(super) fn of(op: &Value) -> Rule {
        let r = &op["execution_rule"];
        let num = |k: &str, d: f64| {
            r[k].as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .unwrap_or(d)
        };
        Rule {
            rush_ratio: num("rush_ratio", 1.0),
            careful_ratio: num("careful_ratio", 0.5),
            min_losses: num("min_losses", 1500.0),
            days: r["days"].as_i64().filter(|d| *d >= 1).unwrap_or(2),
        }
    }
}

/// Whether the operation chooses its attackers' mode itself: no `execution`, or "auto". A plan
/// that names careful, balanced or rush keeps that mode (sent once, `execution_modes`).
pub(super) fn auto(op: &Value) -> bool {
    match op["execution"].as_str() {
        None => op["execution"].is_null(),
        Some(m) => m == "auto",
    }
}

fn id_of(d: &Value) -> Option<String> {
    d["id"]
        .as_str()
        .map(String::from)
        .or_else(|| d["id"].as_i64().map(|n| n.to_string()))
}

/// The battles in which a division of `armies` stands on our side (engaged or in reserve),
/// from a hoi4_battles read with divisions; `army_of` maps a division id to its army (the
/// hoi4_supply divisions). A battle is won when the enemy side's hours until broken are fewer
/// than ours, else by the read's `winning` side; one that reads neither is counted, not judged.
pub(super) fn battle_odds(
    read: &Value,
    armies: &HashSet<String>,
    army_of: &HashMap<String, String>,
) -> Odds {
    let mut odds = Odds::default();
    for b in list(read, "list") {
        let ours = text(&b["our_side"]);
        let theirs = match ours {
            "attacker" => "defender",
            "defender" => "attacker",
            _ => continue,
        };
        let side = &b[ours];
        let mine = list(side, "engaged_divisions")
            .iter()
            .chain(list(side, "reserve_divisions").iter())
            .filter_map(id_of)
            .any(|d| army_of.get(&d).is_some_and(|a| armies.contains(a)));
        if !mine {
            continue;
        }
        odds.battles += 1;
        for t in list(&b[theirs], "tags").iter().filter_map(Value::as_str) {
            odds.tags.insert(t.to_string());
        }
        let us = side["hours_until_broken"].as_f64();
        let them = b[theirs]["hours_until_broken"].as_f64();
        let won = match (us, them) {
            (Some(u), Some(t)) if u != t => Some(t < u),
            _ => match text(&b["winning"]) {
                "" | "even" => None,
                w => Some(w == ours),
            },
        };
        match won {
            Some(true) => odds.won += 1,
            Some(false) => odds.lost += 1,
            None => {}
        }
    }
    odds
}

/// Own and enemy losses against `tags` from a losses-by-enemy table (plan_military).
pub(super) fn losses_against(
    table: &super::super::plan_military::Losses,
    tags: &BTreeSet<String>,
) -> (f64, f64) {
    table
        .iter()
        .filter(|(t, _)| tags.contains(*t))
        .fold((0.0, 0.0), |(o, e), (_, (lo, le))| (o + lo, e + le))
}

/// The mode for an attacker: `window` = (own, enemy) losses since its last reading, `total` the
/// same since the war began (the window is read once it holds `min_losses`, else the total).
/// Returns the mode and its reason.
pub(super) fn choose(
    odds: &Odds,
    window: (f64, f64),
    total: (f64, f64),
    rule: &Rule,
) -> (&'static str, String) {
    let pick = if window.0 + window.1 >= rule.min_losses {
        Some(("the last days", window))
    } else if total.0 + total.1 >= rule.min_losses {
        Some(("the war", total))
    } else {
        None
    };
    let battles = format!(
        "{} of {} battles won, {} lost",
        odds.won, odds.battles, odds.lost
    );
    match pick {
        Some((over, (own, enemy))) => {
            let ratio = enemy / own.max(1.0);
            let losses = format!("losses {ratio:.2} enemy to 1 own over {over}");
            if ratio < rule.careful_ratio && odds.lost > odds.won {
                ("careful", format!("{losses}, {battles}: the attack bleeds"))
            } else if ratio < rule.rush_ratio {
                ("balanced", format!("{losses}: our losses outrun theirs"))
            } else {
                (
                    "rush",
                    format!("{losses}, {battles}: every province of the plan is attacked"),
                )
            }
        }
        None if odds.lost > odds.won => (
            "balanced",
            format!("too few losses to judge, {battles}: most battles are lost"),
        ),
        None => (
            "rush",
            format!("too few losses to judge, {battles}: every province of the plan is attacked"),
        ),
    }
}

pub(super) fn index(mode: &str) -> Option<i64> {
    MODES.iter().position(|m| *m == mode).map(|i| i as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A battle of the o96a reads: province, our side, our and their hours until broken, our
    /// division ids, their tags.
    fn battle(p: i64, side: &str, us: f64, them: f64, ids: &[&str], tags: &[&str]) -> Value {
        let other = if side == "attacker" {
            "defender"
        } else {
            "attacker"
        };
        let mut b = json!({"province": p, "our_side": side});
        b[side] = json!({"hours_until_broken": us,
            "engaged_divisions": ids.iter().map(|i| json!({"id": i})).collect::<Vec<_>>(),
            "reserve_divisions": []});
        b[other] = json!({"hours_until_broken": them, "tags": tags});
        b
    }

    fn armies(ids: &[(&str, &str)]) -> HashMap<String, String> {
        ids.iter()
            .map(|(d, a)| (d.to_string(), a.to_string()))
            .collect()
    }

    #[test]
    fn the_battles_of_an_army_count_as_won_when_the_enemy_breaks_first() {
        // o96a 1940.05.06 15h: 3488 Ardennes (ours 786 h, theirs 139), 11562 (2120 / 90),
        // Luxembourg 6583 (549 / 1113: lost); one battle of another army
        let read = json!({"list": [
            battle(3488, "attacker", 786.0, 139.0, &["a"], &["BEL"]),
            battle(11562, "attacker", 2120.0, 90.0, &["b"], &["BEL", "HOL"]),
            battle(6583, "attacker", 549.0, 1113.0, &["c"], &["FRA", "LUX"]),
            battle(9999, "attacker", 10.0, 900.0, &["x"], &["POL"]),
        ]});
        let of = armies(&[
            ("a", "Army 3"),
            ("b", "Army 2"),
            ("c", "Army 9"),
            ("x", "Other"),
        ]);
        let group: HashSet<String> = ["Army 2", "Army 3", "Army 9"].map(String::from).into();
        let odds = battle_odds(&read, &group, &of);
        assert_eq!((odds.battles, odds.won, odds.lost), (3, 2, 1));
        assert_eq!(
            odds.tags,
            ["BEL", "FRA", "HOL", "LUX"].map(String::from).into()
        );
        // a defence counts by the same reading: they break first, we win it
        let read = json!({"list": [battle(6241, "defender", 19899.0, 19.0, &["a"], &["BEL"])]});
        let odds = battle_odds(&read, &group, &of);
        assert_eq!((odds.battles, odds.won, odds.lost), (1, 1, 0));
    }

    #[test]
    fn the_mode_presses_while_our_losses_cost_the_enemy_at_least_as_much() {
        let rule = Rule::of(&json!({}));
        // o96a W+1: no losses to judge yet, 2 of 3 battles won: rush from the first war day
        let o = Odds {
            battles: 3,
            won: 2,
            lost: 1,
            ..Odds::default()
        };
        assert_eq!(choose(&o, (0.0, 0.0), (324.0, 10019.0), &rule).0, "rush");
        // W+14 on the Ardennes: 1 of 5 won, but the war's losses 1.8:1 (HOL+BEL+LUX): rush stays
        let o = Odds {
            battles: 5,
            won: 1,
            lost: 3,
            ..Odds::default()
        };
        assert_eq!(
            choose(&o, (900.0, 400.0), (19626.0, 35046.0), &rule).0,
            "rush"
        );
        // W+10.5 -> W+18.5 on balanced: 13428 own against 12347: our losses outrun theirs
        assert_eq!(
            choose(&o, (13428.0, 12347.0), (19626.0, 35046.0), &rule).0,
            "balanced"
        );
        // twice their losses and most battles lost: careful
        assert_eq!(
            choose(&o, (8000.0, 3000.0), (8000.0, 3000.0), &rule).0,
            "careful"
        );
        // twice their losses but the battles are won: balanced, never careful
        let w = Odds {
            battles: 4,
            won: 3,
            lost: 1,
            ..Odds::default()
        };
        assert_eq!(
            choose(&w, (8000.0, 3000.0), (8000.0, 3000.0), &rule).0,
            "balanced"
        );
        // no losses and most battles lost: balanced until the losses speak
        assert_eq!(choose(&o, (0.0, 0.0), (0.0, 0.0), &rule).0, "balanced");
    }

    #[test]
    fn the_rule_is_on_unless_the_plan_names_a_mode_and_its_numbers_are_plan_keys() {
        assert!(auto(&json!({})));
        assert!(auto(&json!({"execution": "auto"})));
        assert!(!auto(&json!({"execution": "rush"})));
        assert!(!auto(&json!({"execution": "balanced"})));
        let r = Rule::of(&json!({"execution_rule": {"rush_ratio": 1.5, "days": 3}}));
        assert_eq!((r.rush_ratio, r.careful_ratio, r.days), (1.5, 0.5, 3));
        assert_eq!(index("rush"), Some(2));
    }

    #[test]
    fn losses_count_against_the_countries_the_attacker_fights() {
        let mut t = super::super::super::plan_military::Losses::new();
        t.insert("HOL".into(), (6834.0, 22431.0));
        t.insert("BEL".into(), (18486.0, 27413.0));
        t.insert("ENG".into(), (100.0, 5.0));
        let tags: BTreeSet<String> = ["HOL", "BEL"].map(String::from).into();
        assert_eq!(losses_against(&t, &tags), (25320.0, 49844.0));
    }
}
