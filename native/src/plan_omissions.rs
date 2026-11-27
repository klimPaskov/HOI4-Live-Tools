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
//! research_slot_idle, weak_formation_fielded (plan_divisions' judgement on the own templates),
//! special_project_facility_idle (a facility running nothing while a project of its kind is
//! open or a scientist is free), trains_falling (the train stock falls and no line makes
//! trains: logistics strikes or captures eat it; with fewer trains than the network needs
//! supply drops to MIN_TRAIN_SUPPLY_FACTOR 0.5), train_line_missing (no line makes trains at
//! all), starving_hubs_low:<army> (an army below the supply floor whose hubs are not fully
//! motorised). Each read is a pure memory read (no input),
//! once a game day with the alerts.
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

/// Doctrine picks open now: an empty track of a folder whose grand doctrine is taken, or a folder
/// with no grand doctrine, while the game itself says a doctrine can be taken (`game_open`: its
/// doctrines_available alert). No price is compared here: the read's cost is the files' before
/// the game's cost modifiers (owner, 27 Nov 2026: costs are the game's to judge); pure.
pub(super) fn affordable_doctrines(read: &Value, game_open: bool) -> Vec<String> {
    let mut out = Vec::new();
    if !game_open {
        return out;
    }
    for f in list(read, "folders") {
        let folder = text(&f["folder"]);
        let xp = f["experience"].as_f64().unwrap_or(0.0);
        let grand = &f["grand_doctrine"];
        if grand.is_null() || grand["id"].is_null() {
            out.push(format!("{folder}: no grand doctrine ({xp:.0} experience)"));
            continue;
        }
        for t in list(f, "tracks") {
            if t["subdoctrine"].is_null() || t["subdoctrine"]["id"].is_null() {
                out.push(format!(
                    "{folder}/{}: empty track ({xp:.0} experience)",
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

/// Facilities that run no project while a project of their specialisation is open (no
/// facility runs it), and facilities with no scientist at all; pure. A scientist with nothing
/// open does basic research, which counts as running.
pub(super) fn idle_facilities(read: &Value) -> Vec<String> {
    let free_scientists = list(read, "scientists")
        .iter()
        .filter(|c| c["assigned_facility_id"].is_null())
        .count();
    let mut out = Vec::new();
    for f in list(read, "facilities") {
        if !f["project"].is_null() || f["basic_research"] == json!(true) {
            continue;
        }
        let spec = text(&f["specialization"]);
        let open = list(read, "projects")
            .iter()
            .filter(|p| text(&p["specialization"]) == spec && p["running_in_facility_id"].is_null())
            .count();
        // a scientist with no project open does basic research (breakthrough points, a
        // research bonus): that is work; a facility with nobody in it does nothing
        if open == 0 && !f["scientist"].is_null() {
            continue;
        }
        let id = f["facility_id"].as_i64().unwrap_or(0);
        out.push(format!(
            "facility {id} ({}) runs nothing: {open} open {} project(s), {free_scientists} free scientist(s)",
            spec.replace("specialization_", ""),
            spec.replace("specialization_", "")
        ));
    }
    out
}

/// The trains row of a `hoi4_stockpile` read: the stock is falling and no line makes trains;
/// pure. A falling stock with a line is the stockpile rule's (rule 11, `stockpile_targets`).
pub(super) fn trains_falling(read: &Value) -> Option<String> {
    let row = list(read, "equipment")
        .iter()
        .find(|r| text(&r["archetype"]) == "train_equipment")?;
    if row["depleting"] != json!(true) {
        return None;
    }
    let made = row["produced_per_day"].as_f64().unwrap_or(0.0);
    if made > 0.0 {
        return None;
    }
    let stock = row["stockpile"].as_f64().unwrap_or(0.0);
    let net = row["net_per_day"].as_f64().unwrap_or(0.0);
    let days = row["days_of_cover"]
        .as_f64()
        .map_or_else(|| "unknown".to_string(), |d| format!("{d:.0}"));
    Some(format!(
        "train stock {stock:.0}, {net:.1} a day, {days} days of cover, no line makes trains: logistics strikes or captures eat them and short trains cut supply up to half (MIN_TRAIN_SUPPLY_FACTOR 0.5); add a train line (hoi4_production_add train_equipment) or stockpile_targets.train_equipment, and fighters or anti-air over the own rail"
    ))
}

/// The trains row of a `hoi4_stockpile` read when no line makes trains at all: the start
/// stock equals the start need (DEFAULT_STARTING_TRAIN_RATIO 1), so every railway province a
/// front adds and every train a logistics strike kills is a shortage nothing replaces.
pub(super) fn trains_no_line(read: &Value) -> Option<String> {
    let row = list(read, "equipment")
        .iter()
        .find(|r| text(&r["archetype"]) == "train_equipment")?;
    if row["lines"].as_u64().unwrap_or(0) > 0
        || row["produced_per_day"].as_f64().unwrap_or(0.0) > 0.0
    {
        return None;
    }
    let stock = row["stockpile"].as_f64().unwrap_or(0.0);
    Some(format!(
        "{stock:.0} trains in stock and no line makes trains: the start stock only covers the start network; a train line from the start (logistics.trains, or hoi4_production_add train_equipment) and stockpile_targets.train_equipment to grow it with the fronts"
    ))
}

/// Armies whose divisions' mean supply (`hoi4_supply` divisions) is below `floor` while their
/// hubs are not fully motorised (`hoi4_armies` motorization below 2): trucks are the supply
/// lever that acts at once.
pub(super) fn hubs_low(roster: &Value, supply: &Value, floor: f64) -> Vec<(String, String)> {
    let mut sum: HashMap<String, (f64, usize)> = HashMap::new();
    for d in list(supply, "divisions") {
        if let Some(p) = d["supply_percent"].as_f64() {
            let e = sum.entry(text(&d["army"]).to_string()).or_default();
            e.0 += p;
            e.1 += 1;
        }
    }
    let mut out = Vec::new();
    for a in list(roster, "armies") {
        let name = text(&a["army"]);
        let (Some(level), Some((t, n))) = (a["motorization"].as_u64(), sum.get(name)) else {
            continue;
        };
        let mean = t / *n as f64;
        if level < 2 && mean < floor {
            out.push((
                name.to_string(),
                format!(
                    "{name}: supply {mean:.0}% below {floor:.0}% with hub motorization {level} of 2: raise it (hoi4_army_motorization level 2, logistics.hubs) while trucks are in stock, and a railway or hub where it starves"
                ),
            ));
        }
    }
    out
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
            let mut named = false;
            if let Some(read) = seen {
                let open_now = affordable_decisions(&read, pp - reserve, plan);
                named = !open_now.is_empty();
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
                        pol.clone(),
                    ));
                }
            }
            // the standing order found nothing to buy while power above the plan's reserve sits
            // unspent: a judgement for the player (an adviser, a law, a decision, improve
            // relations or a justification the plan wants), with what the order saw (cmps1,
            // 27 Nov 2026: 480 power banked for weeks and no row, no monitor issue)
            let idle = pol.as_deref().filter(|n| n.contains("nothing to buy"));
            if let (false, Some(note)) = (named, idle) {
                open.push((
                    "political_power_unspent".to_string(),
                    format!(
                        "political power {pp:.0} ({reserve:.0} held for the plan's picks) buys nothing: the standing order has no entry to buy ({}); choose by value: an adviser, a better law (hoi4_politics_options / hoi4_politics_choose), a decision that pays back (hoi4_decisions_list), improve relations or a war goal the plan wants",
                        note.chars().take(400).collect::<String>()
                    ),
                    "politics",
                    None,
                ));
            }
        }
    }
    // doctrine
    if let Ok(read) = tools.call("hoi4_doctrines", &json!({})) {
        let open_now = state.manage.alert_since.contains_key("doctrines_available");
        let picks = affordable_doctrines(&read, open_now);
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
    // formations: fielded templates worse than the plan's own (plan_divisions)
    if let Ok(read) = tools.call("hoi4_template_get", &json!({})) {
        let verdicts = super::plan_divisions::weak_templates(&read, plan);
        if !verdicts.is_empty() {
            let acting = plan["formations"]["act"] == json!(true);
            open.push((
                "weak_formation_fielded".to_string(),
                format!(
                    "{}; {}",
                    verdicts
                        .iter()
                        .map(super::plan_divisions::describe)
                        .collect::<Vec<_>>()
                        .join("; "),
                    if acting {
                        "the formations rule acts on them"
                    } else {
                        "the plan's formations.act is off: act, or name a template with a role in formations.keep"
                    }
                ),
                "formations",
                super::plan_manage::note_of(state, "formations").map(String::from),
            ));
        }
    }
    // special projects: a facility running nothing while something could run there
    if let Ok(read) = tools.call("hoi4_special_projects", &json!({})) {
        let idle = idle_facilities(&read);
        if !idle.is_empty() {
            open.push((
                "special_project_facility_idle".to_string(),
                format!(
                    "{}: assign (hoi4_special_project_assign; the plan's special_projects.priority first)",
                    idle.join("; ")
                ),
                "special_projects",
                super::plan_manage::note_of(state, "special_projects").map(String::from),
            ));
        }
    }
    // trains: the stock falls and nothing makes trains (not read when the plan switched the
    // stockpile rule off: the plan then owns production alone)
    let stock_off = super::strings(&plan["manage"], "off")
        .iter()
        .any(|r| r == "stockpile");
    if let Some(read) = (!stock_off)
        .then(|| {
            tools
                .call("hoi4_stockpile", &json!({"equipment": "train_equipment"}))
                .ok()
        })
        .flatten()
    {
        if let Some(todo) = trains_falling(&read) {
            open.push((
                "trains_falling".to_string(),
                todo,
                "stockpile",
                super::plan_manage::note_of(state, "stockpile").map(String::from),
            ));
        } else if let Some(todo) = trains_no_line(&read) {
            open.push((
                "train_line_missing".to_string(),
                todo,
                "logistics",
                super::plan_manage::note_of(state, "logistics").map(String::from),
            ));
        }
    }
    // supply: a starving army whose hubs are not fully motorised
    if let (Ok(roster), Ok(supply)) = (
        tools.call("hoi4_armies", &json!({"threat": false})),
        tools.call("hoi4_supply", &json!({})),
    ) {
        let floor = plan["logistics"]["floor"]
            .as_f64()
            .or_else(|| plan["war"]["supply_floor"].as_f64())
            .unwrap_or(50.0);
        for (army, todo) in hubs_low(&roster, &supply, floor) {
            open.push((
                format!("starving_hubs_low:{army}"),
                todo,
                "logistics",
                super::plan_manage::note_of(state, "logistics").map(String::from),
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
    fn an_idle_facility_is_named_only_while_something_could_run_there() {
        let read = json!({"facilities": [
            {"facility_id": 1, "specialization": "specialization_land", "project": {"key": "a"}},
            {"facility_id": 2, "specialization": "specialization_air", "project": null,
             "scientist": {"name": "x"}},
            {"facility_id": 3, "specialization": "specialization_naval", "project": null,
             "scientist": null}],
            "projects": [{"key": "sp_air_radar", "specialization": "specialization_air",
                "running_in_facility_id": null}],
            "scientists": [{"name": "x", "assigned_facility_id": 2}]});
        let rows = idle_facilities(&read);
        assert_eq!(rows.len(), 2, "{rows:?}");
        assert!(rows[0].starts_with("facility 2 (air) runs nothing: 1 open"));
        // the naval facility has no scientist: named although no naval project is open
        assert!(rows[1].starts_with("facility 3 (naval)"));
        let busy = json!({"facilities": [{"facility_id": 2, "specialization": "specialization_air",
            "project": null, "scientist": {"name": "x"}}], "projects": [],
            "scientists": [{"name": "x", "assigned_facility_id": 2}]});
        assert!(idle_facilities(&busy).is_empty());
    }

    #[test]
    fn a_missing_train_line_and_a_starving_army_on_low_trucks_are_named() {
        let read = |lines: u64| {
            json!({"equipment": [{"archetype": "train_equipment",
            "stockpile": 71.0, "lines": lines, "produced_per_day": 0.0}]})
        };
        assert!(trains_no_line(&read(0))
            .unwrap()
            .contains("71 trains in stock"));
        assert_eq!(trains_no_line(&read(1)), None);
        let roster = json!({"armies": [{"army": "A", "motorization": 1},
            {"army": "B", "motorization": 2}, {"army": "C", "motorization": 0}]});
        let supply = json!({"divisions": [{"army": "A", "supply_percent": 30.0},
            {"army": "A", "supply_percent": 50.0}, {"army": "B", "supply_percent": 20.0},
            {"army": "C", "supply_percent": 90.0}]});
        let low = hubs_low(&roster, &supply, 50.0);
        assert_eq!(low.len(), 1);
        assert!(low[0]
            .1
            .contains("A: supply 40% below 50% with hub motorization 1 of 2"));
    }

    #[test]
    fn falling_trains_with_no_line_are_named() {
        let read = |made: f64, depleting: bool| {
            json!({"equipment": [{"archetype": "train_equipment", "stockpile": 1450.0,
                "produced_per_day": made, "net_per_day": -6.5, "days_of_cover": 223.0,
                "depleting": depleting}]})
        };
        assert!(trains_falling(&read(0.0, true))
            .unwrap()
            .starts_with("train stock 1450, -6.5 a day, 223 days"));
        assert!(trains_falling(&read(1.2, true)).is_none());
        assert!(trains_falling(&read(0.0, false)).is_none());
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
        // the game says a doctrine can be taken: every empty track is named, whatever the files'
        // base cost says (the game's price carries its modifiers)
        let out = affordable_doctrines(&read, true);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(out[0].starts_with("land/armor: empty track (79 experience"));
        assert!(out[1].starts_with("air/strike_aircraft: empty track (176 experience"));
        // without the game's check nothing is judged
        assert!(affordable_doctrines(&read, false).is_empty());
    }
}
