//! Logistics (`logistics`, off unless the plan has the key): trains made from the start and the
//! supply hubs of attacking or starving armies motorised before they need it.
//!
//! ```json
//! "logistics": {"trains": {"factories": 1}, "hubs": true, "floor": 50, "why": "..."}
//! ```
//!
//! `trains`: a train line from the first day. A country starts with exactly the trains its
//! network needs (DEFAULT_STARTING_TRAIN_RATIO 1): every railway province a front adds between
//! the capital and a hub (NUM_RAILWAYS_TRAIN_FACTOR), every captured network and every train a
//! logistics strike kills is a shortage, and short trains cut supply by up to half
//! (MIN_TRAIN_SUPPLY_FACTOR 0.5). A train line is opened with `factories` (1 by default: a
//! train costs tens of military factory days, so one factory makes a steady trickle without
//! taking the army's guns) when no line makes trains. Its growth belongs to the stockpile rule:
//! `stockpile_targets.train_equipment` raises the line when the stock falls.
//!
//! `hubs`: the supply-hub motorisation of each army (hoi4_army_motorization: 0 none, 1
//! partial, 2 full; the hubs the army draws from carry supply farther with trucks,
//! SUPPLY_HUB_FULL_MOTORIZATION_BONUS on top of the hub's flow, for
//! SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST trucks a hub). The wanted level of an army:
//! - full when an operation under way (prepared and not ended) names it in its `formations`,
//!   or that operation's `supply.motorization` when it names a level: the trucks stand before
//!   the operation executes, never after the advance has outrun the railways;
//! - full when its divisions' mean supply is below `floor` (default `war.supply_floor`, else
//!   half): trucks are the one lever that acts at once, railways and hubs take months;
//! - otherwise the plan's standing level (`war.motorization`, else none): trucks on a quiet
//!   hub are trucks a starving front lacks, and every level costs trucks from the stockpile.
//!
//! A raise waits until the truck stock covers it (half a hub's full cost a level for each state
//! the army's divisions stand in, a hub per state at most); while it waits the manage digest
//! says how many trucks are missing. A lowering is held back while the army plans or executes
//! an attack. One change per army in a week, one action per tick.

use super::{list, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Trucks a hub needs for full motorisation (game define SUPPLY_HUB_FULL_MOTORIZATION_TRUCK_COST).
const HUB_FULL_TRUCKS: f64 = 60.0;
const FULL: u64 = 2;

/// What one army is wanted at and why.
#[derive(Debug, PartialEq)]
pub(super) struct Want {
    pub army: String,
    pub level: u64,
    pub wanted: u64,
    pub trucks: f64,
    pub why: String,
}

/// Mean supply percent and the states of each army's divisions (`hoi4_supply` divisions).
fn army_supply(read: &Value) -> BTreeMap<String, (f64, BTreeSet<i64>)> {
    let mut sum: BTreeMap<String, (f64, usize, BTreeSet<i64>)> = BTreeMap::new();
    for d in list(read, "divisions") {
        let name = text(&d["army"]);
        let Some(pct) = d["supply_percent"].as_f64() else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let e = sum.entry(name.to_string()).or_default();
        e.0 += pct;
        e.1 += 1;
        if let Some(s) = d["state_id"].as_i64() {
            e.2.insert(s);
        }
    }
    sum.into_iter()
        .map(|(k, (t, n, s))| (k, (t / n as f64, s)))
        .collect()
}

/// The armies of the operations under way, with the level each operation asks for.
fn operation_armies(state: &State, plan: &Value) -> BTreeMap<String, (u64, String)> {
    let mut out = BTreeMap::new();
    for op in list(plan, "operations") {
        let id = text(&op["id"]);
        if id.is_empty()
            || !state.done.contains_key(&format!("op:{id}:prep"))
            || state.done.contains_key(&format!("op:{id}:end"))
        {
            continue;
        }
        let level = op["supply"]["motorization"]
            .as_u64()
            .unwrap_or(FULL)
            .min(FULL);
        for f in list(op, "formations") {
            let army = text(&f["army"]);
            if !army.is_empty() {
                out.insert(army.to_string(), (level, id.to_string()));
            }
        }
    }
    out
}

/// The wanted level of every army of the roster against its current one.
pub(super) fn hub_wants(
    plan: &Value,
    roster: &Value,
    supply: &Value,
    ops: &BTreeMap<String, (u64, String)>,
) -> Vec<Want> {
    let cfg = &plan["logistics"];
    let floor = cfg["floor"]
        .as_f64()
        .or_else(|| plan["war"]["supply_floor"].as_f64())
        .unwrap_or(50.0);
    let standing = plan["war"]["motorization"].as_u64().unwrap_or(0).min(FULL);
    let supplies = army_supply(supply);
    let mut out = Vec::new();
    for a in list(roster, "armies") {
        let army = text(&a["army"]).to_string();
        let Some(level) = a["motorization"].as_u64() else {
            continue;
        };
        let attacking =
            a["plan_executing"] == json!(true) || a["battle_plan_orders"].as_u64().unwrap_or(0) > 0;
        let (mean, states) = supplies
            .get(&army)
            .cloned()
            .unwrap_or((100.0, BTreeSet::new()));
        let (wanted, why) = if let Some((l, op)) = ops.get(&army) {
            (*l, format!("{op} is under way"))
        } else if mean < floor {
            (FULL, format!("supply {mean:.0}% below {floor:.0}%"))
        } else {
            (standing, "standing level".to_string())
        };
        if wanted == level || (wanted < level && attacking) {
            continue;
        }
        let hubs = states.len().max(1) as f64;
        let trucks = if wanted > level {
            HUB_FULL_TRUCKS / FULL as f64 * (wanted - level) as f64 * hubs
        } else {
            0.0
        };
        out.push(Want {
            army,
            level,
            wanted,
            trucks,
            why,
        });
    }
    // raises first, the hungriest (most trucks) last among them so a short stock serves more
    out.sort_by(|a, b| {
        (b.wanted > b.level)
            .cmp(&(a.wanted > a.level))
            .then(a.trucks.total_cmp(&b.trucks))
    });
    out
}

/// The truck stock (`hoi4_stockpile` motorized_equipment).
fn trucks(read: &Value) -> Option<f64> {
    list(read, "equipment")
        .iter()
        .find(|r| text(&r["archetype"]).starts_with("motorized_equipment"))
        .and_then(|r| r["stockpile"].as_f64())
}

fn train_line_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["logistics"]["trains"];
    if cfg.is_null() || *cfg == json!(false) || state.skipped(now, "logi", "trains") {
        return Ok(None);
    }
    state.skip(now, "logi", "trains", DAY);
    let read = tools.call("hoi4_stockpile", &json!({"equipment": "train_equipment"}))?;
    let Some(row) = list(&read, "equipment")
        .iter()
        .find(|r| text(&r["archetype"]).starts_with("train_equipment"))
    else {
        return Ok(None);
    };
    if row["lines"].as_u64().unwrap_or(0) > 0 {
        return Ok(None);
    }
    let factories = cfg["factories"].as_u64().unwrap_or(1).max(1);
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_production_add",
        json!({"equipment": "train_equipment", "factories": factories}),
        "lg",
    )?;
    if !ok {
        state.skip(now, "logi", "trains", 3 * DAY);
    }
    let stock = row["stockpile"].as_f64().unwrap_or(0.0);
    Ok(Some(format!(
        "logistics: train line with {factories} factories ({stock:.0} trains in stock, none made): {ok} {why}"
    )))
}

fn hubs_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if plan["logistics"]["hubs"] != json!(true) || state.skipped(now, "logi", "hubs") {
        return Ok(None);
    }
    state.skip(now, "logi", "hubs", DAY / 2);
    let roster = tools.call("hoi4_armies", &json!({"threat": false}))?;
    let supply = tools.call("hoi4_supply", &json!({}))?;
    let ops = operation_armies(state, plan);
    let wants = hub_wants(plan, &roster, &supply, &ops);
    if wants.is_empty() {
        super::plan_manage::note(state, "logistics", "hubs: every army at its wanted level");
        return Ok(None);
    }
    let stock_read = tools.call(
        "hoi4_stockpile",
        &json!({"equipment": "motorized_equipment"}),
    )?;
    let stock = trucks(&stock_read).unwrap_or(0.0);
    let mut short = Vec::new();
    for w in wants {
        if state.skipped(now, "logi-army", &w.army) {
            continue;
        }
        if w.trucks > stock {
            short.push(format!(
                "{} {} -> {} ({}) needs about {:.0} trucks, {:.0} in stock",
                w.army, w.level, w.wanted, w.why, w.trucks, stock
            ));
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_motorization",
            json!({"army": w.army, "level": w.wanted}),
            "lg",
        )?;
        state.skip(now, "logi-army", &w.army, if ok { 7 * DAY } else { DAY });
        if !short.is_empty() {
            super::plan_manage::note(state, "logistics", short.join("; "));
        }
        return Ok(Some(format!(
            "logistics: {} hub motorization {} -> {} ({}): {ok} {why}",
            w.army, w.level, w.wanted, w.why
        )));
    }
    if !short.is_empty() {
        super::plan_manage::note(
            state,
            "logistics",
            format!("trucks short: {}", short.join("; ")),
        );
    }
    Ok(None)
}

/// The logistics rule: the train line first, then the hubs.
pub(super) fn logistics_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if !plan["logistics"].is_object() {
        return Ok(None);
    }
    if let Some(line) = train_line_step(state, tools, plan, lease, now)? {
        return Ok(Some(line));
    }
    hubs_step(state, tools, plan, lease, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const START: GameHour = 1_000_000;

    struct Fake {
        replies: BTreeMap<&'static str, Value>,
        sent: RefCell<Vec<(String, Value)>>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if args.get("request_id").is_some() {
                self.sent
                    .borrow_mut()
                    .push((name.to_string(), args.clone()));
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            Ok(self.replies.get(name).cloned().unwrap_or(json!({})))
        }
    }

    fn roster() -> Value {
        json!({"armies": [
            {"army": "Ost", "motorization": 1, "battle_plan_orders": 0},
            {"army": "West", "motorization": 2, "battle_plan_orders": 0},
            {"army": "Sud", "motorization": 0, "battle_plan_orders": 0},
            {"army": "Nord", "motorization": 2, "battle_plan_orders": 3}]})
    }

    fn supply() -> Value {
        json!({"divisions": [
            {"army": "Ost", "supply_percent": 80.0, "state_id": 10},
            {"army": "Ost", "supply_percent": 70.0, "state_id": 11},
            {"army": "West", "supply_percent": 90.0, "state_id": 42},
            {"army": "Sud", "supply_percent": 30.0, "state_id": 7},
            {"army": "Nord", "supply_percent": 95.0, "state_id": 5}]})
    }

    #[test]
    fn an_operation_and_a_starving_army_get_full_trucks_a_quiet_one_gives_them_back() {
        let plan = json!({"logistics": {"hubs": true}, "war": {"supply_floor": 50, "motorization": 1},
            "operations": [{"id": "op_east", "supply": {"motorization": 2},
                "formations": [{"army": "Ost"}]}]});
        let mut state = State::default();
        state.done.insert("op:op_east:prep".into(), START);
        let ops = operation_armies(&state, &plan);
        let wants = hub_wants(&plan, &roster(), &supply(), &ops);
        let got: Vec<(&str, u64, u64)> = wants
            .iter()
            .map(|w| (w.army.as_str(), w.level, w.wanted))
            .collect();
        // Sud starves (30% < 50%): full; Ost: its operation is under way; West is quiet and
        // goes back to the standing level; Nord attacks and keeps its trucks
        // (raises before lowerings; equal truck needs keep the roster order)
        assert_eq!(got, vec![("Ost", 1, 2), ("Sud", 0, 2), ("West", 2, 1)]);
        assert_eq!(wants[0].trucks, 60.0, "two states, half a hub's cost each");
        assert_eq!(wants[1].trucks, 60.0, "one state, two levels");
        // an ended operation asks nothing
        state.done.insert("op:op_east:end".into(), START);
        let ops = operation_armies(&state, &plan);
        assert!(hub_wants(&plan, &roster(), &supply(), &ops)
            .iter()
            .all(|w| w.army != "Ost"));
    }

    #[test]
    fn a_raise_waits_for_the_trucks_and_says_how_many_are_missing() {
        let plan = json!({"logistics": {"hubs": true}, "war": {"supply_floor": 50}});
        let fake = Fake {
            replies: BTreeMap::from([
                (
                    "hoi4_armies",
                    json!({"armies": [{"army": "Sud", "motorization": 0}]}),
                ),
                ("hoi4_supply", supply()),
                (
                    "hoi4_stockpile",
                    json!({"equipment": [
                    {"archetype": "motorized_equipment", "stockpile": 20.0}]}),
                ),
            ]),
            sent: RefCell::new(Vec::new()),
        };
        let mut state = State::default();
        assert_eq!(
            hubs_step(&mut state, &fake, &plan, "l", START).unwrap(),
            None
        );
        assert!(fake.sent.borrow().is_empty());
        let note = super::super::plan_manage::digest(&state).unwrap()["logistics"].to_string();
        assert!(
            note.contains("needs about 60 trucks, 20 in stock"),
            "{note}"
        );
        // with the trucks in stock the hub is motorised
        let fake = Fake {
            replies: BTreeMap::from([
                (
                    "hoi4_armies",
                    json!({"armies": [{"army": "Sud", "motorization": 0}]}),
                ),
                ("hoi4_supply", supply()),
                (
                    "hoi4_stockpile",
                    json!({"equipment": [
                    {"archetype": "motorized_equipment", "stockpile": 300.0}]}),
                ),
            ]),
            sent: RefCell::new(Vec::new()),
        };
        let line = hubs_step(&mut state, &fake, &plan, "l", START + DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("Sud hub motorization 0 -> 2"), "{line}");
        let sent = fake.sent.borrow();
        assert_eq!(sent[0].0, "hoi4_army_motorization");
        assert_eq!(sent[0].1["level"], 2);
    }

    #[test]
    fn a_train_line_opens_when_nothing_makes_trains_and_only_with_the_key() {
        let stock = |lines: u64| json!({"equipment": [{"archetype": "train_equipment", "stockpile": 71.0, "lines": lines}]});
        let fake = Fake {
            replies: BTreeMap::from([("hoi4_stockpile", stock(0))]),
            sent: RefCell::new(Vec::new()),
        };
        let mut state = State::default();
        assert_eq!(
            logistics_step(&mut state, &fake, &json!({}), "l", START).unwrap(),
            None
        );
        let plan = json!({"logistics": {"trains": {"factories": 1}}});
        let line = logistics_step(&mut state, &fake, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(line.contains("train line with 1 factories"), "{line}");
        assert_eq!(fake.sent.borrow()[0].1["equipment"], "train_equipment");
        // a line already makes trains: nothing
        let fake = Fake {
            replies: BTreeMap::from([("hoi4_stockpile", stock(1))]),
            sent: RefCell::new(Vec::new()),
        };
        let mut state = State::default();
        assert_eq!(
            logistics_step(&mut state, &fake, &plan, "l", START).unwrap(),
            None
        );
        assert!(fake.sent.borrow().is_empty());
    }
}
