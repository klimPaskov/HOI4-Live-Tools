//! What the executor's own reads find assignable and still unassigned, for the omissions list
//! the player and the gates get (`hoi4_wait_change` `now.omissions`, beside the game's alerts).
//! The game raises no alert for a field marshal left in the pool while a group has none, for
//! command power or political power piling up with something to buy, or for a doctrine track
//! left empty with the experience on hand; a strong player sees them all (owner, 23 Nov 2026:
//! "generals without field marshals, free construction slots, too much free pp, no doctrine").
//!
//! Each row is a fact from a read, never a bare threshold: a spend is named only when the
//! thing it buys is on offer and affordable now, and the row carries the owning rule's last
//! note so a deliberate wait (power saved for a costlier pick, a slot kept free for a specific
//! technology) reads as that reason. Rows: army_group_without_field_marshal,
//! army_without_general, command_power_at_cap, political_power_unspent, doctrine_affordable,
//! research_slot_idle. Each read is a pure memory read (no input), once a game day with the
//! alerts.
use super::{list, text, GameHour, State, Tools, DAY};
use serde_json::{json, Value};
use std::collections::HashMap;

/// One row in the shape of the alert rows (plan_manage.rs `omission_rows`).
fn row(item: &str, days: i64, todo: String, owner: &str, note: Option<&str>) -> Value {
    json!({"item": item, "days_open": days, "todo": todo, "owner": owner,
        "owner_note": note.map_or(Value::Null, |n| json!(n)),
        "note_contradicts_read": false, "source": "executor_read"})
}

/// Field marshals and generals free in a commander read: (field marshals, generals); pure.
pub(super) fn free_leaders(rows: &[Value], tag: &str) -> (Vec<String>, Vec<String>) {
    let mut fms = Vec::new();
    let mut gens = Vec::new();
    for c in super::plan_formations::free_commanders(rows, tag) {
        let name = text(&c["name"]).to_string();
        if name.is_empty() {
            continue;
        }
        if super::plan_formations::is_field_marshal(c) {
            fms.push(name);
        } else {
            gens.push(name);
        }
    }
    (fms, gens)
}

/// Command rows from an armies read and a commander read; pure. A group without a field
/// marshal is named while a field marshal is free (or, with none free, while a general could be
/// promoted); an army without a general while any commander is free.
pub(super) fn command_rows(roster: &Value, rows: &[Value], tag: &str) -> Vec<(String, String)> {
    let (fms, gens) = free_leaders(rows, tag);
    let mut out = Vec::new();
    let groups = super::plan_military::groups_without_marshal(roster);
    if !groups.is_empty() && !(fms.is_empty() && gens.is_empty()) {
        let names: Vec<String> = groups
            .iter()
            .map(|(g, d)| format!("{g} ({d} divisions)"))
            .collect();
        let todo = if fms.is_empty() {
            format!(
                "no field marshal is free: promote a free general (hoi4_army_commander on the group promotes him for 40 command power): {}",
                gens.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
            )
        } else {
            format!(
                "assign a free field marshal with hoi4_army_commander {{army: <group>, general}} (once; never move one who leads): free {}",
                fms.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
            )
        };
        out.push((
            "army_group_without_field_marshal".to_string(),
            format!("{}: {todo}", names.join(", ")),
        ));
    }
    let leaderless: Vec<String> = list(roster, "armies")
        .iter()
        .filter(|a| a["commander"].as_str().is_none_or(|c| c.trim().is_empty()))
        .filter(|a| a["division_count"].as_u64().unwrap_or(0) > 0)
        .map(|a| text(&a["army"]).to_string())
        .filter(|a| !a.is_empty())
        .collect();
    if !leaderless.is_empty() && !(gens.is_empty() && fms.is_empty()) {
        let pool = if gens.is_empty() { &fms } else { &gens };
        out.push((
            "army_without_general".to_string(),
            format!(
                "{}: assign a free general with hoi4_army_commander (once): free {}",
                leaderless.join(", "),
                pool.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
            ),
        ));
    }
    out
}

/// Enabled decisions the power can pay now, cheapest first, that the plan neither takes nor
/// forbids: (key, cost); pure.
pub(super) fn affordable_decisions(read: &Value, budget: f64, plan: &Value) -> Vec<(String, f64)> {
    let planned: Vec<String> = list(plan, "decisions")
        .iter()
        .chain(list(plan, "decision_rules"))
        .map(|d| text(&d["decision"]).to_string())
        .collect();
    let mut out: Vec<(String, f64)> = list(read, "decisions")
        .iter()
        .filter(|d| d["enabled"] == json!(true) && d["active_timer"] != json!(true))
        .filter_map(|d| {
            let cost = d["cost"].as_f64()?;
            let key = text(&d["decision"]).to_string();
            (cost > 0.0 && cost <= budget && !key.is_empty() && !planned.contains(&key))
                .then_some((key, cost))
        })
        .collect();
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    out
}

/// Doctrine picks the banked experience pays now: an empty track of a folder whose grand
/// doctrine is taken, or a folder with no grand doctrine, where the folder's experience covers
/// the cost the game shows for that folder's picks; pure.
pub(super) fn affordable_doctrines(read: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for f in list(read, "folders") {
        let folder = text(&f["folder"]);
        let xp = f["experience"].as_f64().unwrap_or(0.0);
        let grand = &f["grand_doctrine"];
        // the cheapest cost the read shows in this folder (the game files give each pick its
        // xp_cost; a folder with none shown is not judged)
        let costs: Vec<f64> = std::iter::once(grand["xp_cost"].as_f64())
            .chain(
                list(f, "tracks")
                    .iter()
                    .map(|t| t["subdoctrine"]["xp_cost"].as_f64()),
            )
            .flatten()
            .collect();
        let Some(cost) = costs.iter().copied().reduce(f64::min) else {
            continue;
        };
        if xp < cost {
            continue;
        }
        if grand.is_null() || grand["id"].is_null() {
            out.push(format!("{folder}: no grand doctrine ({xp:.0} experience)"));
            continue;
        }
        for t in list(f, "tracks") {
            if t["subdoctrine"].is_null() || t["subdoctrine"]["id"].is_null() {
                out.push(format!(
                    "{folder}/{}: empty track ({xp:.0} experience, picks cost {cost:.0})",
                    text(&t["track"])
                ));
            }
        }
    }
    out
}

/// Keep a command power reading of the cp_spend rule: the last two a game day or more apart.
pub(super) fn record_cp(state: &mut State, now: GameHour, cp: f64) {
    let samples = &mut state.manage.cp_samples;
    match samples.last() {
        Some((at, _)) if now - at < DAY => {
            samples.pop();
        }
        _ => {}
    }
    samples.push((now, cp));
    if samples.len() > 2 {
        samples.remove(0);
    }
}

/// The command power when the last two readings, a day or more apart, are the same; pure.
pub(super) fn cp_stuck(samples: &[(GameHour, f64)]) -> Option<f64> {
    match samples {
        [.., (a, x), (b, y)] if b - a >= DAY && (x - y).abs() < 0.01 && *y > 0.0 => Some(*y),
        _ => None,
    }
}

/// The rows of this read with their first-seen days.
pub(super) fn derived(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
) -> Vec<Value> {
    // (item, todo, owner, owner note key, from which notes)
    let mut open: Vec<(String, String, &str, Option<String>)> = Vec::new();
    // command
    if let Ok(roster) = tools.call("hoi4_armies", &json!({"threat": false})) {
        if let Ok(read) = tools.call("hoi4_commanders", &json!({})) {
            let tag = super::plan_formations::country_tag(state, tools, plan);
            for (item, todo) in command_rows(&roster, list(&read, "commanders"), &tag) {
                let note = super::plan_military::note_of(state, "group_marshals")
                    .or_else(|| super::plan_military::note_of(state, "army_group"))
                    .map(String::from);
                open.push((item, todo, "command", note));
            }
        }
    }
    // command power: the cp_spend rule's own readings (no read of its own here); two readings
    // a game day or more apart with the same value sit at the cap (the cap itself is not read
    // reliably: g17n read 210 of 80), so every day's gain is lost
    if let Some(cp) = cp_stuck(&state.manage.cp_samples) {
        open.push((
            "command_power_at_cap".to_string(),
            format!(
                "command power {cp:.0} gains nothing: a group without a field marshal, a general's assignable trait (hoi4_general_trait list:true), or a preferred tactic or army ability (no tool yet) would use it"
            ),
            "cp_spend",
            super::plan_seams::note_of(state, "cp_spend").map(String::from),
        ));
    }
    // political power: an enabled decision the power pays now that nothing in the plan takes,
    // while the standing politics order has nothing to buy (power saved for a pick is no row)
    if let Some(pp) = super::plan_manage::political_power(tools) {
        let (reserve, _) = super::plan_manage::politics_reserve(state, tools, plan, now);
        let pol = super::plan_manage::note_of(state, "politics_auto").map(String::from);
        let saving = pol
            .as_deref()
            .is_some_and(|n| n.contains("saved for") || n.contains("power is saved"));
        if !saving {
            // the decisions step's own last read, at most a month old (hoi4_decisions_list
            // opens the panel: this read never sends input)
            let seen = state
                .manage
                .decisions_seen
                .as_ref()
                .filter(|(at, _)| now - at <= 31 * DAY)
                .map(|(_, read)| read.clone());
            if let Some(read) = seen {
                let open_now = affordable_decisions(&read, pp - reserve, plan);
                if !open_now.is_empty() {
                    let names: Vec<String> = open_now
                        .iter()
                        .take(8)
                        .map(|(k, c)| format!("{k} ({c:.0})"))
                        .collect();
                    open.push((
                        "political_power_unspent".to_string(),
                        format!(
                            "political power {pp:.0} ({reserve:.0} held for the plan's picks) and enabled decisions it pays now that the plan neither takes nor forbids: {}; take the ones that pay back (hoi4_decision_take) or name them in decision_rules / politics_auto.never with the reason",
                            names.join(", ")
                        ),
                        "politics",
                        pol,
                    ));
                }
            }
        }
    }
    // doctrine
    if let Ok(read) = tools.call("hoi4_doctrines", &json!({})) {
        let picks = affordable_doctrines(&read);
        if !picks.is_empty() {
            open.push((
                "doctrine_affordable".to_string(),
                format!(
                    "{}: adopt with hoi4_doctrine_adopt (the plan's doctrines / doctrine_defaults first)",
                    picks.join("; ")
                ),
                "doctrines",
                super::plan_manage::note_of(state, "doctrines").map(String::from),
            ));
        }
    }
    // research
    if let Ok(read) = tools.call("hoi4_research", &json!({})) {
        let idle = list(&read, "slots")
            .iter()
            .filter(|s| s["researching"].is_null())
            .count();
        if idle > 0 {
            open.push((
                "research_slot_idle".to_string(),
                format!("{idle} research slot(s) idle: right only while a specific technology is waited for; else hoi4_research_start"),
                "research",
                None,
            ));
        }
    }
    let seen: Vec<String> = open.iter().map(|(i, ..)| i.clone()).collect();
    let since: &mut HashMap<String, GameHour> = &mut state.manage.derived_since;
    since.retain(|k, _| seen.contains(k));
    open.into_iter()
        .map(|(item, todo, owner, note)| {
            let first = *since.entry(item.clone()).or_insert(now);
            row(&item, (now - first) / DAY, todo, owner, note.as_deref())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commander(name: &str, assigned: Option<&str>, promote: bool) -> Value {
        json!({"name": name, "id": format!("GER_{name}"), "listed_in_panel_or_assigned": true,
            "assigned_to": assigned.map(|a| json!({"kind": "army", "name": a})),
            "available": assigned.is_none(), "promotion_needed_for_group": promote,
            "skill": 3, "planning": 2, "logistics": 2, "traits": []})
    }

    fn roster() -> Value {
        json!({"armies": [
            {"army": "Armee Nord", "army_group": "Army Group 2", "commander": "Hoth", "division_count": 20},
            {"army": "Armee Süd", "army_group": null, "commander": null, "division_count": 9},
            {"army": "Empty", "army_group": null, "commander": null, "division_count": 0}],
            "army_groups": [
                {"army_group": "Army Group 2", "field_marshal": null, "plan": []},
                {"army_group": "Army Group 1", "field_marshal": "Rundstedt", "plan": []},
                {"army_group": "Shell", "field_marshal": null, "plan": []}]})
    }

    #[test]
    fn a_group_without_a_marshal_and_an_army_without_a_general_are_named_while_leaders_are_free() {
        let rows = vec![
            commander("Bock", None, false),
            commander("Leeb", None, false),
            commander("Hoepner", None, true),
            commander("Hoth", Some("Armee Nord"), true),
        ];
        let out = command_rows(&roster(), &rows, "GER");
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(out[0].0, "army_group_without_field_marshal");
        assert!(out[0].1.contains("Army Group 2 (20 divisions)"));
        assert!(out[0].1.contains("free Bock, Leeb"), "{}", out[0].1);
        assert!(!out[0].1.contains("Shell"), "an empty group is no omission");
        assert_eq!(out[1].0, "army_without_general");
        assert!(out[1].1.starts_with("Armee Süd:"), "{}", out[1].1);
        assert!(
            out[1].1.contains("free Hoepner"),
            "plain generals for armies: {}",
            out[1].1
        );
        // nobody free: nothing to name
        let busy = vec![commander("Hoth", Some("Armee Nord"), true)];
        assert!(command_rows(&roster(), &busy, "GER").is_empty());
        // only generals free: the group row says a promotion is the way
        let gens = vec![commander("Hoepner", None, true)];
        let out = command_rows(&roster(), &gens, "GER");
        assert!(
            out[0].1.contains("no field marshal is free: promote"),
            "{}",
            out[0].1
        );
    }

    #[test]
    fn decisions_the_power_pays_and_the_plan_does_not_decide_are_named_cheapest_first() {
        let read = json!({"decisions": [
            {"decision": "GER_northern_state_development", "enabled": true, "cost": 50.0, "active_timer": false},
            {"decision": "war_propaganda_radio_industry", "enabled": true, "cost": 75.0, "active_timer": false},
            {"decision": "GER_import_tariffs", "enabled": true, "cost": 15.0, "active_timer": false},
            {"decision": "GER_cancel_mefos", "enabled": true, "cost": 0.0, "active_timer": false},
            {"decision": "GER_construct_the_ostwall", "enabled": false, "cost": null},
            {"decision": "big", "enabled": true, "cost": 300.0, "active_timer": false}]});
        let plan = json!({"decision_rules": [{"decision": "GER_import_tariffs"}]});
        let out = affordable_decisions(&read, 100.0, &plan);
        assert_eq!(
            out,
            vec![
                ("GER_northern_state_development".to_string(), 50.0),
                ("war_propaganda_radio_industry".to_string(), 75.0)
            ]
        );
        assert!(affordable_decisions(&read, 40.0, &plan).is_empty());
    }

    #[test]
    fn command_power_read_twice_a_day_apart_unchanged_sits_at_its_cap() {
        let mut st = State::default();
        record_cp(&mut st, 100, 80.0);
        record_cp(&mut st, 110, 80.0);
        assert_eq!(
            cp_stuck(&st.manage.cp_samples),
            None,
            "less than a day apart"
        );
        record_cp(&mut st, 100 + DAY, 80.0);
        assert_eq!(
            cp_stuck(&st.manage.cp_samples),
            None,
            "the sample at 110 replaced 100"
        );
        record_cp(&mut st, 110 + DAY, 80.0);
        record_cp(&mut st, 110 + 2 * DAY, 80.0);
        assert_eq!(cp_stuck(&st.manage.cp_samples), Some(80.0));
        record_cp(&mut st, 110 + 3 * DAY, 60.0);
        assert_eq!(cp_stuck(&st.manage.cp_samples), None, "spent");
    }

    #[test]
    fn empty_doctrine_tracks_the_experience_pays_are_named() {
        let read = json!({"folders": [
            {"folder": "land", "experience": 79.0,
             "grand_doctrine": {"id": "new_mobile_warfare", "xp_cost": 100},
             "tracks": [{"track": "armor", "subdoctrine": null}]},
            {"folder": "air", "experience": 176.0,
             "grand_doctrine": {"id": "new_battlefield_support", "xp_cost": 100},
             "tracks": [
                {"track": "fighter_aircraft", "subdoctrine": {"id": "x", "xp_cost": 100}},
                {"track": "strike_aircraft", "subdoctrine": null}]},
            {"folder": "naval", "experience": 167.0,
             "grand_doctrine": {"id": "new_convoy_raiding", "xp_cost": 100},
             "tracks": [{"track": "screens", "subdoctrine": {"id": "convoy_escort", "xp_cost": 100}}]}]});
        let out = affordable_doctrines(&read);
        assert_eq!(out.len(), 1, "{out:?}");
        assert!(out[0].starts_with("air/strike_aircraft: empty track (176 experience"));
    }
}
