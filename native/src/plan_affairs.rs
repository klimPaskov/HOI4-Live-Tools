//! State, economy and diplomacy affairs: the one-shot and ordered work a strong player does
//! besides the war and the standing rules (coverage audit section 2 rows B14, C3-C8, D4, D8,
//! M1-M5, O1-O2; section 4 rules 29, 30, 34; wave 2 items W2-2, W2-3, W2-6, W2-7, W2-8, W2-10,
//! W2-12). A child module of `plan_run` (declared there with `#[path]`), like plan_manage.rs: it
//! shares the runner's `State`, its failure ledger (`State::act`: an order refused 3 times in 10
//! game days is not sent again), the done-memory `State::done` and the skip timers `State::bad`,
//! both kept in plan_state.json, so nothing done is done again after a restart. At most one
//! action per tick; every cadence is game time.
//!
//! Hooks in plan_run.rs: [`step`] runs right after plan_manage's step (tick step 8a''),
//! [`bonus_first`] reorders the research step's priority list while a slot is idle, and `run`
//! adds [`take_alarms`] to its `alarms` and [`digest`] as `affairs` (one status line per
//! section).
//!
//! `when` is a `crate::plan_predicate` predicate judged with the tick's reads (`plan_run::judge`):
//! missing or empty holds; an atom that cannot be read is Unknown, the item waits and the atom is
//! listed in the run's `predicates_unknown`. A wake is the alarm {"code":"wake_player","rule",
//! "reason","message","evidence"} (the shape plan_manage uses), once per reason per
//! `manage.wake_repeat_days` (7). Refusals wait on a skip timer chosen by the refusal code.
//!
//! Plan keys:
//!   designs      [{"id","kind":"tank"|"plane"|"ship"|"other" ("type" is read too),"base":design id or exact
//!                name/type key,"name"?,"modules"?:{slot:module|null},"upgrades"?:{row:level},"when"?,"xp"?,
//!                "production"?:"<line equipment>"}] equipment designs made once per `id` (else kind and
//!                name/base) through hoi4_design_create (`modules` for the module designers and ships,
//!                `upgrades` for tanks/planes without them). XP guard: the held experience of the kind's
//!                branch (hoi4_doctrines: tank army, plane air, ship navy; `other` none) must reach `xp`
//!                (default 5 per module or upgrade row named: a floor, not the price; the price is in
//!                hoi4_design_options); an entry short of XP waits and blocks the later entries of the same
//!                branch; an unreadable XP leaves the check to the tool (insufficient_experience). A design
//!                `name` that already exists (hoi4_designs) counts as done. Tool: tank and plane live 1-2 Nov
//!                with 10 DLC, ship 31 Oct with 3 DLC
//!   production[].design  a design (a `designs` id, a design name or a design id number) the line of
//!                that `equipment` switches to, once, through hoi4_design_set_production (line index from
//!                hoi4_production_lines, design id from hoi4_designs; `accept_progress_loss` default true).
//!                Mature-line guard: when the design's type (hoi4_designs hull_or_chassis) differs from the
//!                line's equipment (a tier or chassis change keeps 30% or less of the line's efficiency),
//!                a line whose efficiency (hoi4_production_lines output.efficiency, percent) is at least
//!                `manage.mature_line_efficiency` (30) is mature; without that read, a line the executor
//!                did not add in the last `manage.mature_line_days` (30) game days is. A mature line
//!                is never switched: the design starts as a
//!                separate line (hoi4_production_add with `design`, 10% of the old line's factories,
//!                1-5) on free factories and the old line keeps building; "switch_mature": true on the
//!                entry (or on the `designs` item) switches the whole line anyway. A same-type variant
//!                (90% kept) switches as before;
//!                `designs[].production` asks the same from the design's side. It waits until the design
//!                and the line exist. The production step (tick step 7) adds lines without a design
//!                (hoi4_production_add takes `design`, step 7 does not pass it), so a new line starts on
//!                the first listed design and is switched here. Tool: 31 Oct with 3 DLC; with Arms
//!                Against Tyranny the design click opens the MIO chooser (not live-verified)
//!   diplomacy    [{"id","action","target":TAG,"when"?,"world_tension_max"?,..}] in list order, each done once
//!                per `id` (else action and target) through hoi4_diplomacy_act. `action`: "justify"
//!                (justify_war_goal; needs "war_goal" (a type, e.g. "Conquer") and "states" (1-5 names as
//!                hoi4_diplomacy lists them); a goal against the target in hoi4_diplomacy_state
//!                own_war_goals, or a war with it, counts as done), "declare" (declare_war, below),
//!                "guarantee", "non_aggression" (or "nap"), "access" (ask_military_access),
//!                "offer_access", "docking", "improve_relations", "send_attache", "invite_to_faction",
//!                or any other canonical key or exact row name of hoi4_diplomacy; "try_anyway":true sends a
//!                row the AI would refuse. `world_tension_max` (fraction or percent): only while world
//!                tension is at most that (rule 30; unknown waits). A declaration is sent only when
//!                (1) every justify item of the plan for that target is done and, with "justify_days" on
//!                either item, that many game days have passed since; (2) the target's diplomacy view
//!                (hoi4_diplomacy, at most every 3 game days while it is not) offers Declare war enabled
//!                (the justification is complete: no read has its progress); (3) the widening check passes:
//!                hoi4_war_risk names who would join, and the joiners the plan does not list (item
//!                "joiners_ok", `war.enemies`, the target) may together own fewer than "widen_ratio"
//!                (default 1.0) times the player's states (hoi4_diplomacy_countries; state count is the
//!                only strength a player sees) -- otherwise no declaration and a wake; a joiner or count
//!                that cannot be read waits and wakes; borders hoi4_war_risk reports not held wait (and
//!                wake) unless the item says "accept_widening":true, which is passed on. Already at war
//!                with the target (`at_war_with`) is done. Rule 29 (war goal expiry): no tool reads
//!                justification progress or expiry, so the step remembers when it first saw the
//!                declaration possible (done key `dipready:<item key>`; while the item's own gates keep
//!                it from looking, the justify items' done hour plus `justify_days` stands in) and wakes
//!                the player "war_goal_expiring" "expiry_warn_days" (default 50) game days later while the item still
//!                waits; a declaration that was possible and no longer is wakes "war_goal_expired", and
//!                with "rejustify":true the plan's justify items for the target run again. Tools: justify
//!                and declare live 1-2 Nov with 10 DLC; guarantee, NAP, access 1.19.3 only; hoi4_war_risk
//!                never live-verified; hoi4_diplomacy_countries and hoi4_diplomacy_state read live
//!   faction      {"join"?:TAG|"ANY","create"?:{"name","template"?,"invite"?:[country names]},"when"?}: when
//!                `when` holds and hoi4_diplomacy_state says the player is in no faction (in one: both
//!                done; unreadable: waits), `join` first: an open faction invitation from that country
//!                (hoi4_diplomacy_incoming, class faction_invitation, its text naming the tag or the
//!                country's name; "ANY" takes any) is accepted with hoi4_faction_invite_accept, else
//!                hoi4_diplomacy_act join_faction is sent; a refusal by the AI or the game lets `create`
//!                go ahead and retries the join after 30 game days. `create` founds the faction once
//!                through hoi4_faction_create (already_in_faction counts as done). Tools: faction_create
//!                opened live 6 Nov (descriptor UNVALIDATED); invite_accept and diplomacy_incoming never
//!   intelligence {"agency":true|false,"upgrades":[ordered],"operatives"?:N|[{"mission","target"?:TAG,
//!                "state_id"?}],"when"?}: "agency":true creates the agency once (hoi4_agency_create;
//!                agency_exists and agency_creation_in_progress count as done), then nothing waits for
//!                AGENCY_CREATION_DAYS (hoi4_intelligence defines, default 30). `upgrades` (rule 34): upgrade
//!                keys in order ("upgrade_army_department", "<branch>:<upgrade>" or {"branch","upgrade"}; a
//!                key twice is a second level), the branch from hoi4_intelligence when not given; when no
//!                upgrade runs the next not done is started (hoi4_agency_upgrade), and the next waits
//!                AGENCY_UPGRADE_DAYS (default 30); upgrade_in_progress retries in 5 game days,
//!                upgrade_unavailable leaves that entry 30 days and the next in order goes first. The
//!                agency's state is not readable (hoi4_intelligence state_read:false), so the refusals
//!                are the only state. `operatives`: that many operatives recruited (hoi4_operative_recruit,
//!                first candidate), and a list entry's `mission` assigned to operative i
//!                (hoi4_operative_assign, country `target` or `state_id`). Tools: agency_upgrade live 1-2
//!                Nov; agency_create live 6 Nov (descriptor UNVALIDATED); operatives fixed live round w13
//!                (descriptors UNVALIDATED)
//!   mio          [{"organisation" ("organization" too),"traits":[trait_index,..],"when"?,"slot"?,
//!                "design_team"?:true|slot,"policy"?}] per organisation, in order: `design_team` puts it into
//!                that design-team slot of the politics view once (hoi4_mio_design_team, slot `slot` or 0),
//!                `policy` attaches that policy once (hoi4_mio_policy), then the traits are queued in list
//!                order, one per action (hoi4_mio_trait). Trait entries carry no text in the game, so a trait
//!                is its 0-based trait_index as hoi4_mio_trait list:true numbers it (a numeric string is
//!                read too; another string stops that organisation's traits with a note).
//!                trait_already_queued counts as done; trait_unavailable/trait_not_visible leave the
//!                organisation 14 game days; mio_system_absent leaves the whole key 30 days. No tool reads
//!                trait points (hoi4_mios state_read:false), and the game's `mio_traits` alert still wakes
//!                the player (plan_manage). Tools: mio_trait live 6 Nov with 10 DLC (descriptor UNVALIDATED);
//!                mio_design_team and mio_policy never live
//!   trade        [{"id"?,"resource","amount":N|"max"|"needed","from"? ("partner" too),"when"?}] (or
//!                {"deals":[..]}) purchases made once per `id` (else resource and partner) through
//!                hoi4_trade_set (amount default "needed"); without a partner the one with the most units
//!                available in hoi4_trade {resource} is taken. While automatic importing is on the game
//!                blocks manual trades (auto_import_enabled): the item waits 30 game days with a note;
//!                `auto_import` stays plan_manage's resource rule. Tool: trade_set live 31 Oct with 3 DLC
//!   research_bonuses  {"<bonus category or bonus name>":[technology keys],..} or true: while a research
//!                slot is idle, an active limited-use research bonus (hoi4_research_extras tech_bonuses,
//!                uses_left > 0) moves its listed technologies, and the plan's research_path and
//!                research_priority keys holding a category word (e.g. "infantry_weapons" in
//!                "infantry_weapons1"), to the front of the research priority, never one ahead of its
//!                start year. Technology categories are not exposed by hoi4_technology_list, so the
//!                strategist lists the techs per category (facts.py tech has them); `true` uses the word
//!                match only. Tool: research_extras read live 31 Oct. Not covered: assigning an MIO design
//!                team to one research (D8): no tool does it (hoi4_research_start has no MIO argument,
//!                hoi4_mio_design_team fills the politics view's design-team slot: `mio[].design_team`)
use super::{item_id, list, strings, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use crate::plan_predicate::Truth;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What the runner keeps for these sections besides `done` and the skip timers.
#[derive(Default)]
pub(super) struct Memory {
    /// Wake alarms raised since the last run result.
    alarms: Vec<Value>,
    /// The last status line of each section (the run result's `affairs`).
    notes: BTreeMap<String, String>,
    /// hoi4_research_extras, read at most once a game day.
    extras: Option<(GameHour, Value)>,
    /// hoi4_intelligence, read at most once a game day.
    intel: Option<(GameHour, Value)>,
}

/// The wake alarms raised since the last call (the run result's `alarms`).
pub(super) fn take_alarms(state: &mut State) -> Vec<Value> {
    std::mem::take(&mut state.affairs.alarms)
}

/// The last status of each section (the run result's `affairs`), or None when nothing ran.
pub(super) fn digest(state: &State) -> Option<Value> {
    (!state.affairs.notes.is_empty()).then(|| json!(state.affairs.notes))
}

fn note(state: &mut State, section: &str, line: impl Into<String>) {
    state.affairs.notes.insert(section.to_string(), line.into());
}

/// Raise a `wake_player` alarm once per `reason` per `manage.wake_repeat_days` (7).
fn wake(
    state: &mut State,
    plan: &Value,
    now: GameHour,
    rule: u8,
    reason: &str,
    message: String,
    evidence: Value,
) {
    if state.skipped(now, "wake", reason) {
        return;
    }
    let days = plan["manage"]["wake_repeat_days"]
        .as_i64()
        .filter(|d| *d > 0)
        .unwrap_or(7);
    state.skip(now, "wake", reason, days * DAY);
    state
        .affairs
        .alarms
        .retain(|a| a["reason"] != json!(reason));
    state.affairs.alarms.push(
        json!({"code": "wake_player", "rule": rule, "reason": reason,
        "message": message, "evidence": evidence}),
    );
}

/// Whether an item's `when` holds: missing or empty holds, else a plan predicate.
fn when_holds(state: &mut State, tools: &dyn Tools, when: &Value, now: GameHour) -> Truth {
    match when {
        Value::Null => Truth::True,
        Value::String(s) if s.trim().is_empty() => Truth::True,
        _ => super::judge(state, tools, when, now),
    }
}

/// The item's `when` as a wait reason, None when it holds.
fn gate(state: &mut State, tools: &dyn Tools, when: &Value, now: GameHour) -> Option<&'static str> {
    match when_holds(state, tools, when, now) {
        Truth::True => None,
        Truth::False => Some("when not met"),
        Truth::Unknown => Some("when unknown"),
    }
}

/// The typed code of a refusal (`code: message` from `State::act`), else the receipt status.
fn code(why: &str) -> &str {
    why.split(':').next().unwrap_or("").trim()
}

/// One section's view of the tick.
struct Run<'a> {
    tools: &'a dyn Tools,
    plan: &'a Value,
    lease: &'a str,
    now: GameHour,
}

type Section = fn(&mut State, &Run<'_>) -> Result<Option<String>>;

/// Everything after plan_manage's step, at most one action: diplomacy, faction, trade, designs
/// and line switches, MIOs, intelligence.
pub fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let run = Run {
        tools,
        plan,
        lease,
        now,
    };
    let sections: [Section; 7] = [
        diplomacy_step,
        faction_step,
        trade_step,
        designs_step,
        line_design_step,
        mio_step,
        intelligence_step,
    ];
    for section in sections {
        if let Some(done) = section(state, &run)? {
            return Ok(Some(done));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-2: designs and the line switch (C3-C6).

fn xp_branch(kind: &str) -> Option<&'static str> {
    match kind {
        "tank" => Some("army"),
        "plane" => Some("air"),
        "ship" => Some("navy"),
        _ => None,
    }
}

/// The own design called exactly `name` (hoi4_designs), if one exists.
fn design_named(tools: &dyn Tools, name: &str) -> Option<Value> {
    let read = tools.call("hoi4_designs", &json!({"name": name})).ok()?;
    list(&read, "designs")
        .iter()
        .find(|d| text(&d["name"]).trim().eq_ignore_ascii_case(name))
        .cloned()
}

fn designs_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let entries = list(plan, "designs");
    if entries.is_empty() || state.skipped(now, "mdes", "x") {
        return Ok(None);
    }
    let mut xp: Option<Option<Value>> = None;
    let mut blocked: HashSet<&'static str> = HashSet::new();
    let mut waits: Vec<String> = Vec::new();
    for entry in entries {
        let kind = entry["kind"]
            .as_str()
            .or(entry["type"].as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase();
        let base = &entry["base"];
        let name = text(&entry["name"]).trim().to_string();
        let what = if name.is_empty() {
            base.to_string()
        } else {
            name.clone()
        };
        let label = item_id(entry).unwrap_or_else(|| what.clone());
        let key = item_id(entry).map_or_else(
            || format!("design:{kind}:{what}"),
            |id| format!("design:id:{id}"),
        );
        if state.done.contains_key(&key) {
            continue;
        }
        if !["tank", "plane", "ship", "other"].contains(&kind.as_str())
            || !(base.is_string() || base.is_u64())
        {
            waits.push(format!(
                "{label}: needs kind tank|plane|ship|other and base"
            ));
            continue;
        }
        if state.skipped(now, "des", &key) {
            waits.push(format!("{label}: waiting after a refusal"));
            continue;
        }
        let branch = xp_branch(&kind);
        if branch.is_some_and(|b| blocked.contains(b)) {
            continue;
        }
        if let Some(why) = gate(state, tools, &entry["when"], now) {
            waits.push(format!("{label}: {why}"));
            continue;
        }
        if !name.is_empty() && design_named(tools, &name).is_some() {
            state.done.insert(key, now);
            continue;
        }
        let named = entry["modules"].as_object().map_or(0, Map::len)
            + entry["upgrades"].as_object().map_or(0, Map::len);
        let need = entry["xp"].as_f64().unwrap_or(5.0 * named as f64);
        if let Some(b) = branch.filter(|_| need > 0.0) {
            let read = xp.get_or_insert_with(|| tools.call("hoi4_doctrines", &json!({})).ok());
            // an unreadable XP leaves the check to the tool's own insufficient_experience
            if let Some(have) = read
                .as_ref()
                .and_then(|d| d["experience"][b]["value"].as_f64())
            {
                if have < need {
                    waits.push(format!("{label}: {b} XP {have} < {need}"));
                    blocked.insert(b);
                    continue;
                }
            }
        }
        let mut args = Map::new();
        args.insert("kind".into(), json!(kind));
        args.insert("base".into(), base.clone());
        if !name.is_empty() {
            args.insert("name".into(), json!(name));
        }
        for field in ["modules", "upgrades"] {
            if entry[field].is_object() {
                args.insert(field.into(), entry[field].clone());
            }
        }
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_design_create",
            Value::Object(args),
            "dz",
        )?;
        let c = code(&why);
        if ok || c == "name_taken" || c == "nothing_to_change" {
            state.done.insert(key, now);
        } else {
            let days = match c {
                "insufficient_experience" => 7,
                "kind_mismatch"
                | "base_unknown"
                | "base_ambiguous"
                | "design_not_offered"
                | "slot_unknown"
                | "name_not_typable"
                | "designer_unsupported"
                | "upgrade_unknown"
                | "upgrade_level_max"
                | "slot_locked"
                | "module_unavailable" => {
                    note(state, "designs", format!("{label}: {why}"));
                    30
                }
                _ => 3,
            };
            state.skip(now, "des", &key, days * DAY);
        }
        return Ok(Some(format!("design {kind} {label}: {ok} {why}")));
    }
    if !waits.is_empty() {
        note(state, "designs", waits.join("; "));
        state.skip(now, "mdes", "x", DAY);
    }
    Ok(None)
}

/// One wanted switch: (line equipment, design label, design by name or id, the `designs` item
/// key it waits for, accept_progress_loss, switch_mature).
type Switch = (String, String, Value, Option<String>, bool, bool);

fn wanted_switches(plan: &Value) -> Vec<Switch> {
    let designs = list(plan, "designs");
    let mut out: Vec<Switch> = Vec::new();
    for line in list(plan, "production") {
        let equipment = text(&line["equipment"]).trim().to_string();
        let design = &line["design"];
        if equipment.is_empty() || design.is_null() {
            continue;
        }
        let accept = line["accept_progress_loss"] != json!(false);
        let mature = line["switch_mature"] == json!(true);
        // a `designs` id names the design that item makes
        let item = design.as_str().and_then(|d| {
            designs
                .iter()
                .find(|e| item_id(e).as_deref() == Some(d.trim()))
        });
        match (item, design) {
            (Some(e), _) => out.push((
                equipment,
                text(&e["name"]).trim().to_string(),
                json!(text(&e["name"]).trim()),
                item_id(e).map(|id| format!("design:id:{id}")),
                accept,
                mature || e["switch_mature"] == json!(true),
            )),
            (None, Value::String(name)) => out.push((
                equipment,
                name.trim().to_string(),
                json!(name.trim()),
                None,
                accept,
                mature,
            )),
            (None, Value::Number(id)) => out.push((
                equipment,
                id.to_string(),
                design.clone(),
                None,
                accept,
                mature,
            )),
            _ => {}
        }
    }
    for entry in designs {
        let equipment = text(&entry["production"]).trim().to_string();
        let name = text(&entry["name"]).trim().to_string();
        if equipment.is_empty() || out.iter().any(|w| w.0 == equipment && w.1 == name) {
            continue;
        }
        out.push((
            equipment,
            name.clone(),
            json!(name),
            item_id(entry).map(|id| format!("design:id:{id}")),
            entry["accept_progress_loss"] != json!(false),
            entry["switch_mature"] == json!(true),
        ));
    }
    out
}

fn line_design_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let wanted = wanted_switches(plan);
    if wanted.is_empty() || state.skipped(now, "mpdes", "x") {
        return Ok(None);
    }
    let mut lines: Option<Vec<Value>> = None;
    let mut all_designs: Option<Vec<Value>> = None;
    let mut waits: Vec<String> = Vec::new();
    for (equipment, label, design, gate_key, accept, mature) in wanted {
        let key = format!("pdes:{equipment}:{label}");
        if state.done.contains_key(&key) {
            continue;
        }
        if label.is_empty() {
            waits.push(format!("{equipment}: the design has no name to switch to"));
            continue;
        }
        if state.skipped(now, "pdes", &key) {
            waits.push(format!("{equipment} -> {label}: waiting after a refusal"));
            continue;
        }
        if gate_key
            .as_ref()
            .is_some_and(|k| !state.done.contains_key(k))
        {
            waits.push(format!("{equipment} -> {label}: design not made yet"));
            continue;
        }
        let rows = lines.get_or_insert_with(|| {
            tools
                .call("hoi4_production_lines", &json!({"limit": 128}))
                .map(|r| list(&r, "lines").to_vec())
                .unwrap_or_default()
        });
        let Some(line) = rows
            .iter()
            .find(|l| {
                let e = text(&l["equipment_id"]);
                !e.is_empty() && (e == equipment || e.starts_with(&equipment))
            })
            .cloned()
        else {
            waits.push(format!("{equipment} -> {label}: no line yet"));
            continue;
        };
        let found = match &design {
            Value::String(name) => design_named(tools, name),
            _ => all_designs
                .get_or_insert_with(|| {
                    tools
                        .call("hoi4_designs", &json!({}))
                        .map(|r| list(&r, "designs").to_vec())
                        .unwrap_or_default()
                })
                .iter()
                .find(|d| d["design_id"].as_i64() == design.as_i64())
                .cloned(),
        };
        let Some(found) = found else {
            waits.push(format!("{equipment} -> {label}: design not found"));
            continue;
        };
        let handle = &line["line_handle"];
        if handle.is_string()
            && list(&found, "production_lines")
                .iter()
                .any(|p| p["line_handle"] == *handle)
        {
            state.done.insert(key, now);
            continue;
        }
        let target = found
            .get("design_id")
            .filter(|d| d.is_u64())
            .cloned()
            .unwrap_or(design);
        // Mature-line guard: a switch to another equipment type (a tier or chassis change keeps
        // 30% or less of the line's efficiency, a same-type variant 90%) never takes the
        // factories of a mature line; the new design starts as a separate line on free factories
        // and the old line keeps building. Only `switch_mature: true` switches the whole line.
        // The line's efficiency is read (hoi4_production_lines output.efficiency, percent): at
        // or above `manage.mature_line_efficiency` (30) the line is mature. Without the read a
        // line counts as young only when the executor added it less than
        // `manage.mature_line_days` (30) game days ago.
        let line_type = text(&line["equipment_id"]).to_string();
        let design_type = text(&found["hull_or_chassis"]).to_string();
        if !mature && !line_type.is_empty() && !design_type.is_empty() && line_type != design_type {
            let mature_days = plan["manage"]["mature_line_days"]
                .as_i64()
                .filter(|d| *d >= 0)
                .unwrap_or(30);
            let mature_efficiency = plan["manage"]["mature_line_efficiency"]
                .as_f64()
                .filter(|e| *e >= 0.0)
                .unwrap_or(30.0);
            let efficiency = line["output"]["efficiency"].as_f64();
            let added = state.done.get(&format!("prodadd:{equipment}")).copied();
            let is_mature = match efficiency {
                Some(e) => e >= mature_efficiency,
                None => !added.is_some_and(|h| now - h < mature_days * DAY),
            };
            if is_mature {
                let factories = line["factory_counters"]["requested_factories_raw_signed"]
                    .as_i64()
                    .unwrap_or(0);
                let start = ((factories as f64 * 0.1).round() as i64).clamp(1, 5);
                let (ok, why) = state.act(
                    tools,
                    run.lease,
                    "hoi4_production_add",
                    json!({"equipment": design_type, "design": target, "factories": start}),
                    "dn",
                )?;
                if ok {
                    state.done.insert(key, now);
                } else {
                    state.skip(now, "pdes", &key, 3 * DAY);
                }
                let age = match efficiency {
                    Some(e) => {
                        format!("efficiency {e:.0}% read, mature from {mature_efficiency:.0}%")
                    }
                    None => added.map_or_else(
                        || "not added by the executor; no line efficiency read".to_string(),
                        |h| format!("added {} game days ago", (now - h) / DAY),
                    ),
                };
                note(
                    state,
                    "production_tier_guard",
                    format!("{equipment} -> {label}: {line_type} to {design_type} changes the type and the line is mature ({age}): it keeps its factories, the new design starts on a separate line with {start}: {ok}"),
                );
                return Ok(Some(format!(
                    "production {label} on a separate line ({start} factories), mature line {equipment} kept: {ok} {why}"
                )));
            }
        }
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_design_set_production",
            json!({"line": line["entry_index"], "design": target, "accept_progress_loss": accept}),
            "dl",
        )?;
        let c = code(&why);
        if ok || c == "already_building" {
            state.done.insert(key, now);
        } else {
            let days = match c {
                "design_wrong_type" | "progress_loss_unconfirmed" => {
                    note(
                        state,
                        "production_designs",
                        format!("{equipment} -> {label}: {why}"),
                    );
                    90
                }
                "design_unknown" => 7,
                "line_unknown" => 1,
                _ => 3,
            };
            state.skip(now, "pdes", &key, days * DAY);
        }
        return Ok(Some(format!(
            "production line {equipment} -> design {label}: {ok} {why}"
        )));
    }
    if !waits.is_empty() {
        note(state, "production_designs", waits.join("; "));
        state.skip(now, "mpdes", "x", DAY);
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-3: diplomacy (M1-M4, rules 29 and 30).

/// The canonical hoi4_diplomacy_act key of a plan action (other text is passed on as written).
fn dip_action(action: &str) -> Option<String> {
    let key = match action.trim().to_lowercase().as_str() {
        "" => return None,
        "justify" | "justify_war_goal" | "war_goal" => "justify_war_goal",
        "declare" | "declare_war" | "war" => "declare_war",
        "guarantee" => "guarantee",
        "non_aggression" | "non_aggression_pact" | "nap" => "non_aggression_pact",
        "access" | "military_access" | "ask_military_access" => "ask_military_access",
        "offer_access" | "offer_military_access" => "offer_military_access",
        "docking" | "docking_rights" | "ask_docking_rights" => "ask_docking_rights",
        "offer_docking" | "offer_docking_rights" => "offer_docking_rights",
        "improve" | "improve_relations" => "improve_relations",
        "attache" | "send_attache" => "send_attache",
        "invite" | "invite_to_faction" => "invite_to_faction",
        _ => return Some(action.trim().to_string()),
    };
    Some(key.to_string())
}

fn dip_target(item: &Value) -> String {
    text(&item["target"]).trim().to_uppercase()
}

fn dip_key(item: &Value, action: &str, target: &str) -> String {
    item_id(item).map_or_else(
        || format!("dip:{action}:{target}"),
        |id| format!("dip:id:{id}"),
    )
}

fn diplomacy_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let items = list(plan, "diplomacy");
    if items.is_empty() || state.skipped(now, "mdip", "x") {
        return Ok(None);
    }
    let mut waits: Vec<String> = Vec::new();
    for item in items {
        let Some(action) = dip_action(text(&item["action"])) else {
            continue;
        };
        let target = dip_target(item);
        let key = dip_key(item, &action, &target);
        if state.done.contains_key(&key) {
            continue;
        }
        let label = item_id(item).unwrap_or_else(|| format!("{action} {target}"));
        if target.len() != 3 {
            waits.push(format!("{label}: no target tag"));
            continue;
        }
        if action == "declare_war" || action == "justify_war_goal" {
            // at war with the target already: nothing left to justify or declare
            let at_war = json!(format!("at_war_with:{target}"));
            if super::judge(state, tools, &at_war, now) == Truth::True {
                state.done.insert(key, now);
                continue;
            }
        }
        if action == "declare_war" {
            expiry_watch(state, plan, now, item, &key, &target);
        }
        if state.skipped(now, "dip", &key) {
            waits.push(format!("{label}: waiting"));
            continue;
        }
        if let Some(why) = gate(state, tools, &item["when"], now) {
            waits.push(format!("{label}: {why}"));
            continue;
        }
        if let Some(max) = item["world_tension_max"].as_f64() {
            let wt = json!(format!("world_tension <= {max}"));
            match super::judge(state, tools, &wt, now) {
                Truth::True => {}
                Truth::False => {
                    waits.push(format!("{label}: world tension above {max}"));
                    continue;
                }
                Truth::Unknown => {
                    waits.push(format!("{label}: world tension unknown"));
                    continue;
                }
            }
        }
        let acted = match action.as_str() {
            "justify_war_goal" => justify(state, run, item, &key, &target, &mut waits)?,
            "declare_war" => declare(state, run, item, &key, &target, &mut waits)?,
            _ => relation(state, run, item, &action, &key, &target)?,
        };
        if acted.is_some() {
            return Ok(acted);
        }
    }
    if !waits.is_empty() {
        note(state, "diplomacy", waits.join("; "));
        state.skip(now, "mdip", "x", DAY);
    }
    Ok(None)
}

fn justify(
    state: &mut State,
    run: &Run<'_>,
    item: &Value,
    key: &str,
    target: &str,
    waits: &mut Vec<String>,
) -> Result<Option<String>> {
    let now = run.now;
    let label = item_id(item).unwrap_or_else(|| format!("justify {target}"));
    // a goal against the target exists (being justified or justified): done; an unreadable
    // state leaves the check to the tool
    if let Ok(ds) = run.tools.call("hoi4_diplomacy_state", &json!({})) {
        if list(&ds, "own_war_goals")
            .iter()
            .any(|g| g["target"]["tag"].as_str() == Some(target))
        {
            state.done.insert(key.to_string(), now);
            return Ok(None);
        }
    }
    let war_goal = text(&item["war_goal"]).trim().to_string();
    let states = strings(item, "states");
    if war_goal.is_empty() || states.is_empty() {
        waits.push(format!(
            "{label}: justify needs war_goal and states (names as hoi4_diplomacy lists them)"
        ));
        state.skip(now, "dip", key, 30 * DAY);
        return Ok(None);
    }
    let (ok, why) = state.act(
        run.tools,
        run.lease,
        "hoi4_diplomacy_act",
        json!({"action": "justify_war_goal", "country": target, "war_goal": war_goal, "states": states}),
        "dp",
    )?;
    if ok || code(&why) == "nothing_to_change" {
        state.done.insert(key.to_string(), now);
    } else {
        let days = match code(&why) {
            "action_disabled" => 7,
            "action_not_offered" => 14,
            _ => 3,
        };
        state.skip(now, "dip", key, days * DAY);
    }
    Ok(Some(format!(
        "diplomacy justify_war_goal {target}: {ok} {why}"
    )))
}

/// Rule 29 without a timer read: the declaration has been possible for `expiry_warn_days`
/// (50) game days and still waits.
fn expiry_watch(
    state: &mut State,
    plan: &Value,
    now: GameHour,
    item: &Value,
    key: &str,
    target: &str,
) {
    // when the item's gates kept it from reading the game, the plan's justify_days estimate
    let estimate = || -> Option<GameHour> {
        let mut at: Option<GameHour> = None;
        for j in justify_items(plan, target) {
            let done = *state.done.get(&dip_key(j, "justify_war_goal", target))?;
            let days = j["justify_days"]
                .as_i64()
                .or(item["justify_days"].as_i64())?;
            let t = done + days * DAY;
            at = Some(at.map_or(t, |a| a.max(t)));
        }
        at
    };
    let Some(ready) = state
        .done
        .get(&format!("dipready:{key}"))
        .copied()
        .or_else(estimate)
    else {
        return;
    };
    let days = item["expiry_warn_days"].as_i64().unwrap_or(50);
    if now - ready >= days * DAY {
        wake(
            state,
            plan,
            now,
            29,
            &format!("war_goal_expiring:{key}"),
            format!(
                "the war goal against {target} has been usable for {} game days and lasts about 60 (wiki War goal): declare or justify again",
                (now - ready) / DAY
            ),
            json!({"target": target, "usable_since_hour": ready, "item": item}),
        );
    }
}

/// The plan's justify items for `target`.
fn justify_items<'a>(plan: &'a Value, target: &str) -> Vec<&'a Value> {
    list(plan, "diplomacy")
        .iter()
        .filter(|o| {
            dip_action(text(&o["action"])).as_deref() == Some("justify_war_goal")
                && dip_target(o) == target
        })
        .collect()
}

/// Whether the target's diplomacy view offers Declare war enabled (hoi4_diplomacy rows).
fn declare_ready(run: &Run<'_>, target: &str) -> Truth {
    let Ok(view) = run.tools.call(
        "hoi4_diplomacy",
        &json!({"country": target, "lease_token": run.lease}),
    ) else {
        return Truth::Unknown;
    };
    let Some(rows) = view["actions"].as_array() else {
        return Truth::Unknown;
    };
    let row = rows.iter().find(|r| {
        r["key"] == json!("declare_war")
            || text(&r["action"]).to_lowercase().contains("declare war")
    });
    Truth::from_bool(row.is_some_and(|r| r["enabled"] == json!(true)))
}

enum Widen {
    Clear,
    /// unlisted joiners at least `widen_ratio` of our size (why, evidence)
    Blocked(String, Value),
    /// borders with the target or a joiner not held
    Borders(String),
    Unknown(String),
}

fn widening(run: &Run<'_>, item: &Value, target: &str) -> Widen {
    let Ok(risk) = run.tools.call("hoi4_war_risk", &json!({"country": target})) else {
        return Widen::Unknown("hoi4_war_risk unreadable".into());
    };
    if !risk["would_join"].is_array() {
        return Widen::Unknown("hoi4_war_risk names no joiners list".into());
    }
    let mut listed: HashSet<String> = strings(item, "joiners_ok")
        .into_iter()
        .chain(strings(&run.plan["war"], "enemies"))
        .map(|t| t.trim().to_uppercase())
        .collect();
    listed.insert(target.to_string());
    let unlisted: Vec<String> = strings(&risk, "would_join")
        .into_iter()
        .filter(|t| !listed.contains(&t.to_uppercase()))
        .collect();
    if !unlisted.is_empty() {
        let Ok(countries) = run
            .tools
            .call("hoi4_diplomacy_countries", &json!({"limit": 200}))
        else {
            return Widen::Unknown("hoi4_diplomacy_countries unreadable".into());
        };
        let rows = list(&countries, "countries");
        let states_of = |tag: &str| {
            rows.iter()
                .find(|r| r["tag"].as_str() == Some(tag))
                .and_then(|r| r["states"].as_f64())
        };
        let own = rows
            .iter()
            .find(|r| r["is_player"] == json!(true))
            .and_then(|r| r["states"].as_f64());
        let theirs: Option<f64> = unlisted.iter().map(|t| states_of(t)).sum();
        let (Some(own), Some(theirs)) = (own, theirs) else {
            return Widen::Unknown(format!(
                "the state counts of {} or of the player are unreadable",
                unlisted.join(", ")
            ));
        };
        let ratio = item["widen_ratio"].as_f64().unwrap_or(1.0);
        if theirs >= ratio * own {
            return Widen::Blocked(
                format!(
                    "{} would join (the plan does not list them): {theirs} states against our {own}",
                    unlisted.join(", ")
                ),
                json!({"would_join": unlisted, "their_states": theirs, "own_states": own,
                    "widen_ratio": ratio}),
            );
        }
    }
    let borders = strings(&risk, "borders_not_held");
    if !borders.is_empty() && item["accept_widening"] != json!(true) {
        return Widen::Borders(borders.join("; "));
    }
    Widen::Clear
}

fn declare(
    state: &mut State,
    run: &Run<'_>,
    item: &Value,
    key: &str,
    target: &str,
    waits: &mut Vec<String>,
) -> Result<Option<String>> {
    let (plan, now) = (run.plan, run.now);
    let label = item_id(item).unwrap_or_else(|| format!("declare {target}"));
    // the plan's own justification for this target comes first
    let justifies = justify_items(plan, target);
    let mut not_before: Option<GameHour> = None;
    for j in &justifies {
        let Some(at) = state
            .done
            .get(&dip_key(j, "justify_war_goal", target))
            .copied()
        else {
            waits.push(format!("{label}: waits for its justification"));
            return Ok(None);
        };
        if let Some(days) = j["justify_days"].as_i64().or(item["justify_days"].as_i64()) {
            let t = at + days * DAY;
            not_before = Some(not_before.map_or(t, |n| n.max(t)));
        }
    }
    if not_before.is_some_and(|t| now < t) || state.skipped(now, "dipchk", key) {
        waits.push(format!("{label}: justification not complete"));
        return Ok(None);
    }
    let ready_key = format!("dipready:{key}");
    match declare_ready(run, target) {
        Truth::Unknown => {
            waits.push(format!(
                "{label}: whether war can be declared is unknown (hoi4_diplomacy unreadable)"
            ));
            state.skip(now, "dipchk", key, DAY);
            return Ok(None);
        }
        Truth::False => {
            if state.done.remove(&ready_key).is_some() {
                // the declaration was possible and is not any more: the goal expired (or was
                // cancelled); justify again when the plan says so
                let again = item["rejustify"] == json!(true);
                if again {
                    for j in &justifies {
                        state.done.remove(&dip_key(j, "justify_war_goal", target));
                    }
                }
                wake(
                    state,
                    plan,
                    now,
                    29,
                    &format!("war_goal_expired:{key}"),
                    format!(
                        "war on {target} can no longer be declared: the war goal expired or was cancelled{}",
                        if again { "; the plan's justification runs again" } else { "" }
                    ),
                    json!({"target": target, "item": item}),
                );
            }
            waits.push(format!("{label}: justification not complete"));
            state.skip(now, "dipchk", key, 3 * DAY);
            return Ok(None);
        }
        Truth::True => {
            state.done.entry(ready_key.clone()).or_insert(now);
        }
    }
    match widening(run, item, target) {
        Widen::Clear => {}
        Widen::Unknown(why) => {
            wake(
                state,
                plan,
                now,
                29,
                &format!("declaration_widening_unknown:{key}"),
                format!("declaring war on {target} waits: who would join is unknown ({why})"),
                json!({"target": target}),
            );
            waits.push(format!("{label}: widening unknown ({why})"));
            state.skip(now, "dip", key, DAY);
            return Ok(None);
        }
        Widen::Blocked(why, evidence) => {
            wake(
                state,
                plan,
                now,
                29,
                &format!("declaration_would_widen:{key}"),
                format!("war on {target} is not declared: {why}"),
                evidence,
            );
            waits.push(format!("{label}: would widen ({why})"));
            state.skip(now, "dip", key, 7 * DAY);
            return Ok(None);
        }
        Widen::Borders(borders) => {
            wake(
                state,
                plan,
                now,
                29,
                &format!("declaration_borders_not_held:{key}"),
                format!("war on {target} waits for held borders (hoi4_war_risk): {borders}"),
                json!({"target": target, "borders_not_held": borders}),
            );
            waits.push(format!("{label}: borders not held ({borders})"));
            state.skip(now, "dip", key, 2 * DAY);
            return Ok(None);
        }
    }
    let mut args = json!({"action": "declare_war", "country": target, "confirm": "declare_war"});
    if item["accept_widening"] == json!(true) {
        args["accept_widening"] = json!(true);
    }
    let (ok, why) = state.act(run.tools, run.lease, "hoi4_diplomacy_act", args, "dp")?;
    if ok {
        state.done.insert(key.to_string(), now);
    } else {
        if code(&why) == "war_goal_required" {
            state.done.remove(&ready_key);
        }
        state.skip(now, "dip", key, 3 * DAY);
    }
    Ok(Some(format!("diplomacy declare_war {target}: {ok} {why}")))
}

fn relation(
    state: &mut State,
    run: &Run<'_>,
    item: &Value,
    action: &str,
    key: &str,
    target: &str,
) -> Result<Option<String>> {
    let mut args = json!({"action": action, "country": target});
    if item["try_anyway"] == json!(true) {
        args["try_anyway"] = json!(true);
    }
    let (ok, why) = state.act(run.tools, run.lease, "hoi4_diplomacy_act", args, "dp")?;
    if ok || code(&why) == "nothing_to_change" {
        state.done.insert(key.to_string(), run.now);
    } else {
        let days = match code(&why) {
            "ai_would_refuse" => 30,
            "action_disabled" => 7,
            "action_not_offered" => 14,
            "dialog_unsupported" => 90,
            _ => 3,
        };
        state.skip(run.now, "dip", key, days * DAY);
    }
    Ok(Some(format!("diplomacy {action} {target}: {ok} {why}")))
}

// ---------------------------------------------------------------------------------------------
// W2-12: faction (M5).

/// The index of an open faction invitation from `join` ("ANY": any), from hoi4_diplomacy_incoming.
fn invitation(tools: &dyn Tools, join: &str) -> Option<u64> {
    let read = tools.call("hoi4_diplomacy_incoming", &json!({})).ok()?;
    let offers: Vec<&Value> = list(&read, "proposals")
        .iter()
        .filter(|p| p["class"] == json!("faction_invitation") && p["can_accept"] != json!(false))
        .collect();
    if offers.is_empty() {
        return None;
    }
    if join == "ANY" {
        return offers[0]["index"].as_u64();
    }
    let name = tools
        .call(
            "hoi4_diplomacy_countries",
            &json!({"query": join, "limit": 10}),
        )
        .ok()
        .and_then(|r| {
            list(&r, "countries")
                .iter()
                .find(|c| c["tag"].as_str() == Some(join))
                .map(|c| text(&c["name"]).to_lowercase())
        })
        .filter(|n| !n.is_empty());
    offers
        .iter()
        .find(|p| {
            let words = format!("{} {}", text(&p["title"]), text(&p["description"]));
            words.contains(join)
                || name
                    .as_ref()
                    .is_some_and(|n| words.to_lowercase().contains(n))
        })
        .and_then(|p| p["index"].as_u64())
}

fn faction_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let cfg = &plan["faction"];
    if !cfg.is_object() || state.skipped(now, "mfac", "x") {
        return Ok(None);
    }
    let join = text(&cfg["join"]).trim().to_uppercase();
    let create = &cfg["create"];
    let want_join = !join.is_empty() && !state.done.contains_key("faction:join");
    let want_create = create.is_object() && !state.done.contains_key("faction:create");
    if !want_join && !want_create {
        return Ok(None);
    }
    if let Some(why) = gate(state, tools, &cfg["when"], now) {
        note(state, "faction", why);
        state.skip(now, "mfac", "x", DAY);
        return Ok(None);
    }
    let member = tools
        .call("hoi4_diplomacy_state", &json!({}))
        .ok()
        .and_then(|ds| {
            let f = &ds["faction"];
            f["in_faction"]
                .as_bool()
                .map(|inside| (inside, text(&f["faction"]["name"]).to_string()))
        });
    match member {
        None => {
            note(
                state,
                "faction",
                "faction membership unreadable (hoi4_diplomacy_state): waits",
            );
            state.skip(now, "mfac", "x", DAY);
            return Ok(None);
        }
        Some((true, name)) => {
            for k in ["faction:join", "faction:create"] {
                state.done.entry(k.to_string()).or_insert(now);
            }
            note(state, "faction", format!("in faction {name}"));
            return Ok(None);
        }
        Some((false, _)) => {}
    }
    if want_join && !state.done.contains_key("faction:join_refused") {
        if state.skipped(now, "fac", "join") {
            note(state, "faction", format!("join {join}: waiting"));
            state.skip(now, "mfac", "x", DAY);
            return Ok(None);
        }
        if let Some(index) = invitation(tools, &join) {
            let (ok, why) = state.act(
                tools,
                run.lease,
                "hoi4_faction_invite_accept",
                json!({"index": index}),
                "fa",
            )?;
            if ok {
                state.done.insert("faction:join".into(), now);
            } else {
                state.skip(now, "fac", "join", 7 * DAY);
            }
            return Ok(Some(format!(
                "faction invitation {index} accepted ({join}): {ok} {why}"
            )));
        }
        if join == "ANY" {
            note(state, "faction", "join ANY: no invitation open");
            state.skip(now, "mfac", "x", DAY);
            return Ok(None);
        }
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_diplomacy_act",
            json!({"action": "join_faction", "country": join}),
            "fa",
        )?;
        if ok {
            state.done.insert("faction:join".into(), now);
        } else {
            let refused = matches!(
                code(&why),
                "ai_would_refuse" | "action_not_offered" | "action_disabled"
            );
            if refused {
                // `create` may go ahead; the join is tried again in 30 days
                state.done.insert("faction:join_refused".into(), now);
            }
            state.skip(now, "fac", "join", if refused { 30 } else { 7 } * DAY);
        }
        return Ok(Some(format!("faction join {join}: {ok} {why}")));
    }
    if want_join
        && state.done.contains_key("faction:join_refused")
        && !state.skipped(now, "fac", "join")
    {
        // the refusal's 30 days are over: the join is tried again first
        state.done.remove("faction:join_refused");
        return Ok(None);
    }
    if want_create && !state.skipped(now, "fac", "create") {
        let mut args = Map::new();
        for field in ["name", "template"] {
            if let Some(v) = create[field].as_str().filter(|v| !v.trim().is_empty()) {
                args.insert(field.into(), json!(v.trim()));
            }
        }
        let invite = strings(create, "invite");
        if !invite.is_empty() {
            args.insert("invite".into(), json!(invite));
        }
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_faction_create",
            Value::Object(args),
            "fc",
        )?;
        let c = code(&why);
        if ok || c == "already_in_faction" {
            state.done.insert("faction:create".into(), now);
        } else {
            let days = match c {
                "faction_creation_not_allowed"
                | "capability_unavailable"
                | "template_required"
                | "template_unknown" => {
                    note(state, "faction", format!("create: {why}"));
                    30
                }
                _ => 7,
            };
            state.skip(now, "fac", "create", days * DAY);
        }
        return Ok(Some(format!("faction create: {ok} {why}")));
    }
    note(state, "faction", "waiting after a refusal");
    state.skip(now, "mfac", "x", DAY);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-10: manual trade (B14).

/// The last number in a text such as "3/8".
fn last_number(s: &str) -> Option<i64> {
    s.rsplit(|c: char| !c.is_ascii_digit())
        .find(|p| !p.is_empty())?
        .parse()
        .ok()
}

/// The partner offering the most units of `resource` (hoi4_trade; the first listed on a tie).
fn best_partner(run: &Run<'_>, resource: &str) -> Option<String> {
    let read = run
        .tools
        .call(
            "hoi4_trade",
            &json!({"resource": resource, "lease_token": run.lease}),
        )
        .ok()?;
    let mut best: Option<(i64, String)> = None;
    for p in list(&read, "partners") {
        let country = text(&p["country"]).trim();
        let Some(units) = last_number(text(&p["available"])) else {
            continue;
        };
        if units > 0 && !country.is_empty() && best.as_ref().is_none_or(|(b, _)| units > *b) {
            best = Some((units, country.to_string()));
        }
    }
    best.map(|(_, c)| c)
}

fn trade_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let items: &[Value] = match &plan["trade"] {
        Value::Array(items) => items,
        deals @ Value::Object(_) => list(deals, "deals"),
        _ => &[],
    };
    if items.is_empty() || state.skipped(now, "mtrade", "x") {
        return Ok(None);
    }
    let mut waits: Vec<String> = Vec::new();
    for item in items {
        let resource = text(&item["resource"]).trim().to_lowercase();
        if resource.is_empty() {
            continue;
        }
        let amount = match &item["amount"] {
            Value::String(s) if s == "max" || s == "needed" => json!(s),
            v => v
                .as_f64()
                .filter(|a| *a >= 0.0)
                .map_or(json!("needed"), |a| json!(a.round() as u64)),
        };
        let from = item["from"]
            .as_str()
            .or(item["partner"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let key = item_id(item).map_or_else(
            || {
                format!(
                    "trade:{resource}:{}",
                    if from.is_empty() { "any" } else { &from }
                )
            },
            |id| format!("trade:id:{id}"),
        );
        let label = item_id(item).unwrap_or_else(|| resource.clone());
        if state.done.contains_key(&key) {
            continue;
        }
        if state.skipped(now, "trade", &key) {
            waits.push(format!("{label}: waiting"));
            continue;
        }
        if let Some(why) = gate(state, tools, &item["when"], now) {
            waits.push(format!("{label}: {why}"));
            continue;
        }
        let partner = if from.is_empty() {
            match best_partner(run, &resource) {
                Some(p) => p,
                None => {
                    waits.push(format!("{label}: no partner offers {resource}"));
                    state.skip(now, "trade", &key, 7 * DAY);
                    continue;
                }
            }
        } else {
            from.clone()
        };
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_trade_set",
            json!({"resource": resource, "country": partner, "amount": amount}),
            "td",
        )?;
        if ok {
            state.done.insert(key, now);
        } else {
            let days = match code(&why) {
                "auto_import_enabled" => {
                    note(
                        state,
                        "trade",
                        format!(
                            "{label}: automatic importing is on, the game blocks manual trades"
                        ),
                    );
                    30
                }
                "amount_above_available" => {
                    note(state, "trade", format!("{label}: {why}"));
                    7
                }
                "partner_not_listed" => 7,
                "trade_unavailable" => 14,
                _ => 3,
            };
            state.skip(now, "trade", &key, days * DAY);
        }
        return Ok(Some(format!(
            "trade {resource} {amount} from {partner}: {ok} {why}"
        )));
    }
    if !waits.is_empty() {
        note(state, "trade", waits.join("; "));
        state.skip(now, "mtrade", "x", DAY);
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-8: MIO traits, design team and policy (C7, D8).

fn trait_index(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

/// The skip after a refused MIO order.
fn mio_refused(state: &mut State, now: GameHour, org: &str, why: &str) {
    let days = match code(why) {
        "mio_system_absent" => {
            note(
                state,
                "mio",
                "the organisation system is absent in this game (designers are politics ideas)",
            );
            state.skip(now, "mmio", "x", 30 * DAY);
            return;
        }
        "organization_unknown"
        | "organization_not_allowed"
        | "policy_unknown"
        | "trait_unknown" => {
            note(state, "mio", format!("{org}: {why}"));
            90
        }
        "organization_not_offered"
        | "slot_unknown"
        | "policy_not_offered"
        | "trait_unavailable"
        | "trait_not_visible" => 14,
        _ => 3,
    };
    state.skip(now, "mio", org, days * DAY);
}

fn mio_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let entries = list(plan, "mio");
    if entries.is_empty() || state.skipped(now, "mmio", "x") {
        return Ok(None);
    }
    let mut waits: Vec<String> = Vec::new();
    for entry in entries {
        let org = entry["organisation"]
            .as_str()
            .or(entry["organization"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if org.is_empty() {
            continue;
        }
        let team_key = format!("miodt:{org}");
        let team = match &entry["design_team"] {
            Value::Bool(true) => Some(entry["slot"].as_u64().unwrap_or(0)),
            Value::Number(n) => n.as_u64(),
            _ => None,
        }
        .filter(|_| !state.done.contains_key(&team_key));
        let policy = text(&entry["policy"]).trim().to_string();
        let policy_key = format!("miopol:{org}:{policy}");
        let want_policy = !policy.is_empty() && !state.done.contains_key(&policy_key);
        let mut next: Option<u64> = None;
        let mut bad: Option<String> = None;
        for t in list(entry, "traits") {
            match trait_index(t) {
                Some(i) if state.done.contains_key(&format!("mio:{org}:t{i}")) => {}
                Some(i) => {
                    next = Some(i);
                    break;
                }
                None => {
                    bad = Some(t.to_string());
                    break;
                }
            }
        }
        if let Some(b) = &bad {
            waits.push(format!(
                "{org}: trait {b} is not a trait_index (hoi4_mio_trait list:true numbers them)"
            ));
        }
        if team.is_none() && !want_policy && next.is_none() {
            continue;
        }
        if state.skipped(now, "mio", &org) {
            waits.push(format!("{org}: waiting"));
            continue;
        }
        if let Some(why) = gate(state, tools, &entry["when"], now) {
            waits.push(format!("{org}: {why}"));
            continue;
        }
        let mut args = json!({"organization": org});
        if let Some(slot) = entry["slot"].as_u64() {
            args["slot"] = json!(slot);
        }
        if let Some(slot) = team {
            args["slot"] = json!(slot);
            let (ok, why) = state.act(tools, run.lease, "hoi4_mio_design_team", args, "mt")?;
            if ok {
                state.done.insert(team_key, now);
            } else {
                mio_refused(state, now, &org, &why);
            }
            return Ok(Some(format!(
                "mio {org} design team slot {slot}: {ok} {why}"
            )));
        }
        if want_policy {
            args["policy"] = json!(policy);
            let (ok, why) = state.act(tools, run.lease, "hoi4_mio_policy", args, "mp")?;
            if ok {
                state.done.insert(policy_key, now);
            } else {
                mio_refused(state, now, &org, &why);
            }
            return Ok(Some(format!("mio {org} policy {policy}: {ok} {why}")));
        }
        if let Some(index) = next {
            args["trait_index"] = json!(index);
            let (ok, why) = state.act(tools, run.lease, "hoi4_mio_trait", args, "mi")?;
            if ok || code(&why) == "trait_already_queued" {
                state.done.insert(format!("mio:{org}:t{index}"), now);
            } else {
                mio_refused(state, now, &org, &why);
            }
            return Ok(Some(format!("mio {org} trait {index}: {ok} {why}")));
        }
    }
    if !waits.is_empty() {
        note(state, "mio", waits.join("; "));
        state.skip(now, "mmio", "x", DAY);
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-7: intelligence agency, upgrades in order, operatives (O1, O2, rule 34).

/// hoi4_intelligence, read at most once a game day.
fn intel_read(state: &mut State, tools: &dyn Tools, now: GameHour) -> Option<Value> {
    if let Some((at, read)) = &state.affairs.intel {
        if now - *at < DAY {
            return Some(read.clone());
        }
    }
    let read = tools.call("hoi4_intelligence", &json!({})).ok()?;
    state.affairs.intel = Some((now, read.clone()));
    Some(read)
}

/// An agency define in game days (hoi4_intelligence defines), else `default`.
fn intel_days(
    state: &mut State,
    tools: &dyn Tools,
    now: GameHour,
    name: &str,
    default: i64,
) -> i64 {
    intel_read(state, tools, now)
        .and_then(|r| r["defines"][name].as_f64())
        .filter(|d| *d > 0.0)
        .map_or(default, |d| d.round() as i64)
}

/// (branch if the plan names it, upgrade key) of one `upgrades` entry.
fn upgrade_ref(entry: &Value) -> (Option<String>, String) {
    match entry {
        Value::String(s) => match s.split_once(':') {
            Some((b, u)) => (Some(b.trim().to_string()), u.trim().to_string()),
            None => (None, s.trim().to_string()),
        },
        Value::Object(_) => (
            entry["branch"]
                .as_str()
                .map(|b| b.trim().to_string())
                .filter(|b| !b.is_empty()),
            text(&entry["upgrade"]).trim().to_string(),
        ),
        _ => (None, String::new()),
    }
}

/// The branch an upgrade belongs to (hoi4_intelligence upgrades.branches).
fn branch_of(state: &mut State, tools: &dyn Tools, now: GameHour, upgrade: &str) -> Option<String> {
    let read = intel_read(state, tools, now)?;
    list(&read["upgrades"], "branches")
        .iter()
        .find(|b| {
            list(b, "upgrades")
                .iter()
                .any(|u| text(&u["key"]) == upgrade)
        })
        .map(|b| text(&b["branch"]).to_string())
        .filter(|b| !b.is_empty())
}

fn intelligence_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let cfg = &plan["intelligence"];
    if !cfg.is_object() || state.skipped(now, "mintel", "x") {
        return Ok(None);
    }
    let upgrades = list(cfg, "upgrades");
    let operatives: Vec<Value> = match &cfg["operatives"] {
        Value::Number(n) => vec![Value::Null; n.as_u64().unwrap_or(0).min(16) as usize],
        Value::Array(entries) => entries.clone(),
        _ => Vec::new(),
    };
    let want_agency = cfg["agency"] == json!(true) && !state.done.contains_key("intel:agency");
    if !want_agency && upgrades.is_empty() && operatives.is_empty() {
        return Ok(None);
    }
    let mut waits: Vec<String> = Vec::new();
    if let Some(why) = gate(state, tools, &cfg["when"], now) {
        waits.push(why.to_string());
    } else if want_agency {
        // 1. the agency
        if state.skipped(now, "intel", "agency") {
            waits.push("agency: waiting after a refusal".into());
        } else {
            let (ok, why) = state.act(tools, run.lease, "hoi4_agency_create", json!({}), "ia")?;
            let c = code(&why);
            if ok || c == "agency_exists" || c == "agency_creation_in_progress" {
                state.done.insert("intel:agency".into(), now);
                if ok || c == "agency_creation_in_progress" {
                    let days = if ok {
                        intel_days(state, tools, now, "AGENCY_CREATION_DAYS", 30)
                    } else {
                        10
                    };
                    state.skip(now, "intelwait", "x", days * DAY);
                }
            } else {
                let days = if c == "agency_creation_unavailable" {
                    note(state, "intelligence", format!("agency: {why}"));
                    30
                } else {
                    7
                };
                state.skip(now, "intel", "agency", days * DAY);
            }
            return Ok(Some(format!("intelligence agency create: {ok} {why}")));
        }
    } else if state.skipped(now, "intelwait", "x") {
        waits.push("the agency is being formed".into());
    } else {
        // 2. upgrades in order: when none runs, the next not done (rule 34)
        if !upgrades.is_empty() && state.skipped(now, "intelup", "x") {
            waits.push("upgrades: one is running".into());
        } else if !upgrades.is_empty() {
            let mut seen: HashMap<String, u32> = HashMap::new();
            for entry in upgrades {
                let (branch, upgrade) = upgrade_ref(entry);
                if upgrade.is_empty() {
                    continue;
                }
                let level = {
                    let n = seen.entry(upgrade.clone()).or_insert(0);
                    *n += 1;
                    *n
                };
                let key = format!("intel:up:{upgrade}#{level}");
                if state.done.contains_key(&key) || state.skipped(now, "intel", &key) {
                    continue;
                }
                let Some(branch) = branch.or_else(|| branch_of(state, tools, now, &upgrade)) else {
                    note(
                        state,
                        "intelligence",
                        format!("{upgrade}: branch unknown (hoi4_intelligence)"),
                    );
                    state.skip(now, "intel", &key, 30 * DAY);
                    continue;
                };
                let (ok, why) = state.act(
                    tools,
                    run.lease,
                    "hoi4_agency_upgrade",
                    json!({"branch": branch, "upgrade": upgrade}),
                    "iu",
                )?;
                if ok {
                    state.done.insert(key, now);
                    let days = intel_days(state, tools, now, "AGENCY_UPGRADE_DAYS", 30);
                    state.skip(now, "intelup", "x", days * DAY);
                } else {
                    match code(&why) {
                        "upgrade_in_progress" => state.skip(now, "intelup", "x", 5 * DAY),
                        "agency_not_created" => state.skip(now, "intelup", "x", 15 * DAY),
                        "upgrade_unavailable" => state.skip(now, "intel", &key, 30 * DAY),
                        "branch_unknown" | "upgrade_unknown" => {
                            note(state, "intelligence", format!("{upgrade}: {why}"));
                            state.skip(now, "intel", &key, 90 * DAY);
                        }
                        _ => state.skip(now, "intel", &key, 3 * DAY),
                    }
                }
                return Ok(Some(format!(
                    "intelligence upgrade {branch}/{upgrade}: {ok} {why}"
                )));
            }
        }
        // 3. operatives: recruited in turn, then each listed mission assigned
        if let Some(i) =
            (0..operatives.len()).find(|i| !state.done.contains_key(&format!("intel:op:{i}")))
        {
            if state.skipped(now, "intelop", "x") {
                waits.push("operatives: no free slot".into());
            } else {
                let (ok, why) =
                    state.act(tools, run.lease, "hoi4_operative_recruit", json!({}), "ir")?;
                if ok {
                    state.done.insert(format!("intel:op:{i}"), now);
                } else {
                    let days = match code(&why) {
                        "no_free_operative_slot" => 30,
                        "agency_not_created" => 15,
                        _ => 7,
                    };
                    state.skip(now, "intelop", "x", days * DAY);
                }
                return Ok(Some(format!(
                    "intelligence operative {i} recruit: {ok} {why}"
                )));
            }
        }
        for (i, op) in operatives.iter().enumerate() {
            let mission = text(&op["mission"]).trim();
            let key = format!("intel:opm:{i}");
            if mission.is_empty()
                || !state.done.contains_key(&format!("intel:op:{i}"))
                || state.done.contains_key(&key)
                || state.skipped(now, "intel", &key)
            {
                continue;
            }
            let mut args = json!({"operative": i, "mission": mission});
            if let Some(tag) = op["target"].as_str().or(op["country"].as_str()) {
                args["country"] = json!(tag.trim().to_uppercase());
            }
            if let Some(id) = op["state_id"].as_u64() {
                args["state_id"] = json!(id);
            }
            let (ok, why) = state.act(tools, run.lease, "hoi4_operative_assign", args, "io")?;
            let c = code(&why);
            if ok || c == "operative_busy" {
                state.done.insert(key, now);
            } else {
                let days = match c {
                    "operative_unknown" => 7,
                    "mission_unavailable" => 30,
                    "mission_not_built"
                    | "operation_not_built"
                    | "target_required"
                    | "invalid_arguments" => {
                        note(state, "intelligence", format!("operative {i}: {why}"));
                        90
                    }
                    _ => 7,
                };
                state.skip(now, "intel", &key, days * DAY);
            }
            return Ok(Some(format!(
                "intelligence operative {i} mission {mission}: {ok} {why}"
            )));
        }
    }
    if !waits.is_empty() {
        note(state, "intelligence", waits.join("; "));
    }
    state.skip(now, "mintel", "x", DAY);
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// W2-6: research bonuses (D4).

/// While a research slot is idle: the techs an active limited-use bonus would speed (the plan's
/// `research_bonuses` lists, and priority keys holding a category word) go to the front of
/// `priority`, never one ahead of its start year (`available` start_years).
pub(super) fn bonus_first(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
    available: Option<&Value>,
    priority: &mut Vec<String>,
) {
    let cfg = &plan["research_bonuses"];
    if cfg.is_null() || *cfg == json!(false) {
        return;
    }
    let read = match &state.affairs.extras {
        Some((at, read)) if now - *at < DAY => read.clone(),
        _ => match tools.call("hoi4_research_extras", &json!({})) {
            Ok(read) => {
                state.affairs.extras = Some((now, read.clone()));
                read
            }
            Err(_) => {
                note(state, "research_bonuses", "hoi4_research_extras unreadable");
                return;
            }
        },
    };
    let year = super::game_year(now);
    let ahead = |t: &str| match (year, available.and_then(|a| a["start_years"][t].as_i64())) {
        (Some(y), Some(start)) => start > y,
        _ => false,
    };
    let mut first: Vec<String> = Vec::new();
    let mut used: Vec<String> = Vec::new();
    for bonus in list(&read["tech_bonuses"], "entries") {
        if bonus["uses_left"].as_i64().unwrap_or(0) <= 0 || bonus["exhausted"] == json!(true) {
            continue;
        }
        let name = text(&bonus["name"]).to_string();
        let categories = strings(bonus, "categories");
        let mut techs: Vec<String> = std::iter::once(&name)
            .chain(&categories)
            .flat_map(|k| strings(cfg, k))
            .collect();
        for category in &categories {
            let word = category.strip_prefix("cat_").unwrap_or(category);
            if word.len() >= 4 {
                techs.extend(priority.iter().filter(|t| t.contains(word)).cloned());
            }
        }
        let before = first.len();
        for tech in techs {
            if !ahead(&tech) && !first.contains(&tech) {
                first.push(tech);
            }
        }
        if first.len() > before {
            used.push(format!("{name} ({})", categories.join(",")));
        }
    }
    if first.is_empty() {
        return;
    }
    note(
        state,
        "research_bonuses",
        format!(
            "active bonus {}: first {}",
            used.join(", "),
            first.join(", ")
        ),
    );
    let rest: Vec<String> = priority.drain(..).filter(|t| !first.contains(t)).collect();
    *priority = first;
    priority.extend(rest);
}

#[cfg(test)]
mod tests {
    use super::super::{date_hour, tick, Tick};
    use super::*;
    use crate::error::Error;
    use std::cell::RefCell;

    const START: GameHour = 1_000_000;

    /// Reads answer from `replies`; actions are Verified unless `refuse` names their tool (or
    /// "tool action" for hoi4_diplomacy_act).
    struct Fake {
        calls: RefCell<Vec<(String, Value)>>,
        replies: RefCell<HashMap<String, Value>>,
        refuse: RefCell<HashMap<String, &'static str>>,
        unreadable: RefCell<HashSet<String>>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                let refuse = self.refuse.borrow();
                let by_action = format!("{name} {}", text(&args["action"]));
                if let Some(code) = refuse.get(&by_action).or(refuse.get(name)) {
                    return Err(Error::new(code, "refused by the fake"));
                }
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            if self.unreadable.borrow().contains(name) {
                return Err(Error::new("read_failed", "unreadable in the fake"));
            }
            Ok(self
                .replies
                .borrow()
                .get(name)
                .cloned()
                .unwrap_or_else(|| json!({})))
        }
    }
    fn fake(replies: Vec<(&str, Value)>) -> Fake {
        Fake {
            calls: RefCell::new(Vec::new()),
            replies: RefCell::new(
                replies
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ),
            refuse: RefCell::new(HashMap::new()),
            unreadable: RefCell::new(HashSet::new()),
        }
    }
    impl Fake {
        fn reply(&self, tool: &str, v: Value) {
            self.replies.borrow_mut().insert(tool.into(), v);
        }
        fn refuse(&self, what: &str, code: &'static str) {
            self.refuse.borrow_mut().insert(what.into(), code);
        }
        fn accept(&self, what: &str) {
            self.refuse.borrow_mut().remove(what);
        }
    }
    fn actions(f: &Fake) -> Vec<(String, Value)> {
        f.calls
            .borrow()
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .cloned()
            .collect()
    }
    fn reads(f: &Fake, tool: &str) -> usize {
        f.calls
            .borrow()
            .iter()
            .filter(|(n, a)| n == tool && a.get("request_id").is_none())
            .count()
    }
    fn wake_reasons(state: &mut State) -> Vec<String> {
        take_alarms(state)
            .iter()
            .map(|a| text(&a["reason"]).to_string())
            .collect()
    }
    /// A restart: the done-memory and timers go through plan_state.json's shape.
    fn restarted(state: &State, now: GameHour) -> State {
        let mut fresh = State::default();
        fresh.restore(&state.memory(now), now);
        fresh
    }

    // ----- designs

    #[test]
    fn designs_wait_for_xp_then_are_made_once_and_persist() {
        let tools = fake(vec![
            (
                "hoi4_doctrines",
                json!({"experience": {"army": {"value": 10.0}, "air": {"value": 80.0}}}),
            ),
            ("hoi4_designs", json!({"designs": []})),
        ]);
        let plan = json!({"designs": [
            {"id": "t1", "kind": "tank", "base": "Panzer III", "name": "Panzer III S",
                "modules": {"main_armament_slot": "tank_medium_cannon"}, "xp": 25},
            {"id": "t2", "kind": "tank", "base": 5194, "name": "Panzer III T"},
            {"id": "p1", "type": "plane", "base": "Bf 109", "name": "Bf 109 E N",
                "upgrades": {"range": 2, "engine": 3}}]});
        let mut state = State::default();
        // army XP 10 < 25: t1 waits and blocks t2 (same branch); the plane goes first
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(out.as_deref(), Some("design plane p1: true Verified"));
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_design_create");
        assert_eq!(sent[0].1["kind"], "plane");
        assert_eq!(sent[0].1["upgrades"], json!({"range": 2, "engine": 3}));
        assert_eq!(sent[0].1["lease_token"], "l");
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 1).unwrap(),
            None
        );
        assert!(state.affairs.notes["designs"].contains("t1: army XP 10 < 25"));
        assert_eq!(actions(&tools).len(), 1);
        tools.reply(
            "hoi4_doctrines",
            json!({"experience": {"army": {"value": 40.0}}}),
        );
        // the section waited: it looks again a game day later
        let out = step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap();
        assert_eq!(out.as_deref(), Some("design tank t1: true Verified"));
        // t2 names a design that already exists: done without an action
        tools.reply(
            "hoi4_designs",
            json!({"designs": [{"design_id": 5200, "name": "Panzer III T"}]}),
        );
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 2 * DAY + 1).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
        assert!(state.done.contains_key("design:id:t2"));
        // never again, also after a restart
        let mut again = restarted(&state, START + 3 * DAY);
        assert_eq!(
            step(&mut again, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn a_design_refusal_waits_and_an_unknown_when_waits() {
        let tools = fake(vec![(
            "hoi4_doctrines",
            json!({"experience": {"navy": {"value": 99.0}}}),
        )]);
        tools.refuse("hoi4_design_create", "module_unavailable");
        let plan = json!({"designs": [
            {"id": "s1", "kind": "ship", "base": "ship_hull_light_1", "name": "Navigatori B",
                "modules": {"fixed_ship_battery_slot": "ship_light_battery_2"}},
            {"id": "s2", "kind": "ship", "base": "ship_hull_cruiser_1", "name": "Zara B",
                "when": "bop:ITA_x > 0.5"}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("design ship s1: false module_unavailable: refused by the fake")
        );
        assert!(state.skipped(START + 29 * DAY, "des", "design:id:s1"));
        // s2 waits on an unreadable atom; s1 waits on its timer: nothing is sent
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        assert!(state.affairs.notes["designs"].contains("s2: when unknown"));
        assert!(state.strategy.unknown.contains_key("bop:ITA_x > 0.5"));
    }

    #[test]
    fn a_production_line_switches_to_its_design_once() {
        let tools = fake(vec![
            (
                "hoi4_production_lines",
                json!({"lines": [
                {"entry_index": 2, "line_kind": "military", "equipment_id": "medium_tank_chassis",
                    "line_handle": "0x10"}]}),
            ),
            ("hoi4_designs", json!({"designs": []})),
        ]);
        let plan = json!({
            "designs": [{"id": "t1", "kind": "tank", "base": "Panzer III", "name": "Panzer III S", "xp": 0}],
            "production": [{"equipment": "medium_tank_chassis", "factories": 5, "design": "t1"}]});
        let mut state = State::default();
        state.done.insert("design:id:t1".into(), START);
        // the design is made but not listed yet: waits
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(state.affairs.notes["production_designs"].contains("design not found"));
        tools.reply(
            "hoi4_designs",
            json!({"designs": [
            {"design_id": 5194, "name": "Panzer III S", "production_lines": []}]}),
        );
        let out = step(&mut state, &tools, &plan, "l", START + DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("production line medium_tank_chassis -> design Panzer III S: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_design_set_production");
        assert_eq!(sent[0].1["line"], 2);
        assert_eq!(sent[0].1["design"], 5194);
        assert_eq!(sent[0].1["accept_progress_loss"], true);
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
    }

    // ----- diplomacy

    #[test]
    fn a_tier_change_never_switches_a_mature_line() {
        let tools = fake(vec![
            (
                "hoi4_production_lines",
                json!({"lines": [
                {"entry_index": 2, "line_kind": "military", "equipment_id": "medium_tank_chassis_1",
                    "line_handle": "0x10", "factory_counters": {"requested_factories_raw_signed": 20}}]}),
            ),
            (
                "hoi4_designs",
                json!({"designs": [{"design_id": 6001, "name": "Panzer IV",
                    "hull_or_chassis": "medium_tank_chassis_2", "production_lines": []}]}),
            ),
        ]);
        let plan =
            json!({"production": [{"equipment": "medium_tank_chassis_1", "design": "Panzer IV"}]});
        // a starting line (age unknown): the new tier starts on its own line, the old one stays
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.contains("separate line (2 factories)"), "{out}");
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_production_add");
        assert_eq!(sent[0].1["design"], 6001);
        assert_eq!(sent[0].1["equipment"], "medium_tank_chassis_2");
        assert!(state.affairs.notes["production_tier_guard"].contains("no line efficiency read"));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        // a line the executor added 10 days ago is young: it switches
        let mut state = State::default();
        state
            .done
            .insert("prodadd:medium_tank_chassis_1".into(), START - 10 * DAY);
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("production line medium_tank_chassis_1 -> design Panzer IV: true"),
            "{out}"
        );
        // switch_mature: the whole line switches
        let mut state = State::default();
        let allowed = json!({"production": [{"equipment": "medium_tank_chassis_1",
            "design": "Panzer IV", "switch_mature": true}]});
        step(&mut state, &tools, &allowed, "l", START).unwrap();
        assert_eq!(
            actions(&tools).last().unwrap().0,
            "hoi4_design_set_production"
        );
    }

    #[test]
    fn the_line_efficiency_read_decides_whether_a_line_is_mature() {
        let tools_at = |efficiency: f64| {
            fake(vec![
                (
                    "hoi4_production_lines",
                    json!({"lines": [
                    {"entry_index": 2, "line_kind": "military", "equipment_id": "medium_tank_chassis_1",
                        "line_handle": "0x10", "factory_counters": {"requested_factories_raw_signed": 20},
                        "output": {"efficiency": efficiency, "efficiency_cap": 50.0}}]}),
                ),
                (
                    "hoi4_designs",
                    json!({"designs": [{"design_id": 6001, "name": "Panzer IV",
                        "hull_or_chassis": "medium_tank_chassis_2", "production_lines": []}]}),
                ),
            ])
        };
        let plan =
            json!({"production": [{"equipment": "medium_tank_chassis_1", "design": "Panzer IV"}]});
        // a starting line at 12% efficiency is young by the read: it switches
        let tools = tools_at(12.0);
        let mut state = State::default();
        step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(actions(&tools)[0].0, "hoi4_design_set_production");
        // a line the executor added 5 days ago but already at 45%: mature, kept
        let tools = tools_at(45.0);
        let mut state = State::default();
        state
            .done
            .insert("prodadd:medium_tank_chassis_1".into(), START - 5 * DAY);
        step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(actions(&tools)[0].0, "hoi4_production_add");
        let note = &state.affairs.notes["production_tier_guard"];
        assert!(note.contains("efficiency 45% read"), "{note}");
        // the threshold is the plan's
        let tools = tools_at(45.0);
        let mut state = State::default();
        let high = json!({"manage": {"mature_line_efficiency": 60},
            "production": [{"equipment": "medium_tank_chassis_1", "design": "Panzer IV"}]});
        step(&mut state, &tools, &high, "l", START).unwrap();
        assert_eq!(actions(&tools)[0].0, "hoi4_design_set_production");
    }

    fn declare_rows(enabled: bool) -> Value {
        json!({"country": "ETH", "actions": [
            {"action": "Declare war", "key": "declare_war", "enabled": enabled},
            {"action": "Guarantee independence", "key": "guarantee", "enabled": true}]})
    }

    #[test]
    fn justify_once_then_declare_when_complete_and_the_war_stays_narrow() {
        let tools = fake(vec![
            (
                "hoi4_diplomacy_state",
                json!({"own_war_goals": [], "world_tension": {"value": 0.1}}),
            ),
            ("hoi4_diplomacy", declare_rows(false)),
            (
                "hoi4_war_risk",
                json!({"country": "ETH", "would_join": [], "borders_not_held": []}),
            ),
        ]);
        let plan = json!({"war": {"enemies": ["ETH"]}, "diplomacy": [
            {"id": "wg", "action": "justify", "target": "ETH", "war_goal": "Conquer",
                "states": ["Tigray"], "world_tension_max": 30},
            {"id": "dow", "action": "declare", "target": "ETH"}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy justify_war_goal ETH: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].1["action"], "justify_war_goal");
        assert_eq!(sent[0].1["war_goal"], "Conquer");
        assert_eq!(sent[0].1["states"], json!(["Tigray"]));
        // the declaration waits for the game's Declare war row; read at most every 3 days
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 1).unwrap(),
            None
        );
        assert_eq!(reads(&tools, "hoi4_diplomacy"), 1);
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + DAY + 1).unwrap(),
            None
        );
        assert_eq!(reads(&tools, "hoi4_diplomacy"), 1);
        assert!(state.affairs.notes["diplomacy"].contains("dow: justification not complete"));
        tools.reply("hoi4_diplomacy", declare_rows(true));
        let out = step(&mut state, &tools, &plan, "l", START + 4 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy declare_war ETH: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[1].1["confirm"], "declare_war");
        assert!(sent[1].1.get("accept_widening").is_none());
        // never twice, also after a restart
        let mut again = restarted(&state, START + 6 * DAY);
        assert_eq!(
            step(&mut again, &tools, &plan, "l", START + 6 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn a_declaration_that_would_bring_in_a_stronger_unlisted_country_waits_and_wakes() {
        let tools = fake(vec![
            ("hoi4_diplomacy", declare_rows(true)),
            (
                "hoi4_war_risk",
                json!({"country": "POL", "would_join": ["ENG", "FRA"], "borders_not_held": []}),
            ),
            (
                "hoi4_diplomacy_countries",
                json!({"countries": [
                {"tag": "GER", "states": 30, "is_player": true},
                {"tag": "FRA", "states": 25}, {"tag": "ENG", "states": 20}]}),
            ),
        ]);
        let plan = json!({"diplomacy": [{"id": "dow", "action": "declare", "target": "POL",
            "joiners_ok": ["ENG"]}]});
        let mut state = State::default();
        // ENG is listed; FRA alone (25 states) is weaker than us (30): declared
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy declare_war POL: true Verified")
        );
        // without ENG listed the two own 45 states: no declaration, a wake
        let plan = json!({"diplomacy": [{"id": "dow2", "action": "declare", "target": "POL"}]});
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 1).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        assert_eq!(
            wake_reasons(&mut state),
            vec!["declaration_would_widen:dip:id:dow2"]
        );
        assert!(state.skipped(START + 6 * DAY, "dip", "dip:id:dow2"));
        // a joiner whose size cannot be read: wait and wake (the section looks again a day later)
        tools.reply(
            "hoi4_war_risk",
            json!({"would_join": ["ITA"], "borders_not_held": []}),
        );
        let plan = json!({"diplomacy": [{"id": "dow3", "action": "declare", "target": "POL"}]});
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap(),
            None
        );
        assert_eq!(
            wake_reasons(&mut state),
            vec!["declaration_widening_unknown:dip:id:dow3"]
        );
        // hoi4_war_risk unreadable: the same
        tools.unreadable.borrow_mut().insert("hoi4_war_risk".into());
        let plan = json!({"diplomacy": [{"id": "dow4", "action": "declare", "target": "POL"}]});
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 4 * DAY).unwrap(),
            None
        );
        assert_eq!(
            wake_reasons(&mut state),
            vec!["declaration_widening_unknown:dip:id:dow4"]
        );
        // borders not held wait unless accept_widening, which is passed on
        tools.unreadable.borrow_mut().clear();
        tools.reply("hoi4_war_risk", json!({"would_join": [], "borders_not_held": ["POL: 2 divisions in 3 border states, 1 empty"]}));
        let plan = json!({"diplomacy": [{"id": "dow5", "action": "declare", "target": "POL"}]});
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 6 * DAY).unwrap(),
            None
        );
        assert_eq!(
            wake_reasons(&mut state),
            vec!["declaration_borders_not_held:dip:id:dow5"]
        );
        let plan = json!({"diplomacy": [{"id": "dow6", "action": "declare", "target": "POL",
            "accept_widening": true}]});
        let out = step(&mut state, &tools, &plan, "l", START + 8 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy declare_war POL: true Verified")
        );
        assert_eq!(actions(&tools)[1].1["accept_widening"], true);
    }

    #[test]
    fn diplomacy_waits_on_world_tension_and_unknown_predicates() {
        let tools = fake(vec![(
            "hoi4_diplomacy_state",
            json!({"world_tension": {"value": 0.42}}),
        )]);
        let plan = json!({"diplomacy": [
            {"id": "g", "action": "guarantee", "target": "ALB", "world_tension_max": 0.3},
            {"id": "n", "action": "nap", "target": "YUG", "when": "var:ITA_escalation < 4"}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
        let line = &state.affairs.notes["diplomacy"];
        assert!(line.contains("g: world tension above 0.3"), "{line}");
        assert!(line.contains("n: when unknown"), "{line}");
        // world tension unreadable: unknown, waits
        tools
            .unreadable
            .borrow_mut()
            .insert("hoi4_diplomacy_state".into());
        tools
            .unreadable
            .borrow_mut()
            .insert("hoi4_country_status".into());
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert!(state.affairs.notes["diplomacy"].contains("g: world tension unknown"));
        // tension falls: the guarantee goes out once
        tools.unreadable.borrow_mut().clear();
        tools.reply(
            "hoi4_diplomacy_state",
            json!({"world_tension": {"value": 0.2}}),
        );
        let out = step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy guarantee ALB: true Verified")
        );
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
    }

    #[test]
    fn an_ai_refusal_waits_thirty_days_and_the_ledger_holds_a_failing_order() {
        let tools = fake(vec![]);
        tools.refuse("hoi4_diplomacy_act non_aggression_pact", "ai_would_refuse");
        let plan = json!({"diplomacy": [{"id": "n", "action": "nap", "target": "SOV"}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy non_aggression_pact SOV: false ai_would_refuse: refused by the fake")
        );
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 29 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        // the same order failed 3 times in 10 days: the ledger does not send it again
        let signature = format!(
            "hoi4_diplomacy_act {}",
            json!({"action": "non_aggression_pact", "country": "SOV"})
        );
        let at = START + 31 * DAY;
        state.clock = at;
        state.failures.insert(signature, vec![at - 2, at - 1, at]);
        tools.accept("hoi4_diplomacy_act non_aggression_pact");
        let out = step(&mut state, &tools, &plan, "l", at).unwrap();
        assert!(out
            .unwrap()
            .contains("skipped: the same order failed 3 times"));
        assert_eq!(actions(&tools).len(), 1);
        assert!(!state.done.contains_key("dip:id:n"));
    }

    #[test]
    fn a_usable_war_goal_left_waiting_wakes_and_an_expired_one_is_justified_again() {
        let tools = fake(vec![
            ("hoi4_diplomacy_state", json!({"own_war_goals": []})),
            ("hoi4_diplomacy", declare_rows(true)),
            (
                "hoi4_war_risk",
                json!({"would_join": ["ENG"], "borders_not_held": []}),
            ),
            (
                "hoi4_diplomacy_countries",
                json!({"countries": [
                {"tag": "ITA", "states": 10, "is_player": true}, {"tag": "ENG", "states": 40}]}),
            ),
        ]);
        let plan = json!({"diplomacy": [
            {"id": "wg", "action": "justify", "target": "ETH", "war_goal": "Conquer", "states": ["Tigray"]},
            {"id": "dow", "action": "declare", "target": "ETH", "rejustify": true}]});
        let mut state = State::default();
        state.done.insert("dip:id:wg".into(), START);
        // usable, but England would join: waits and wakes
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert_eq!(
            wake_reasons(&mut state),
            vec!["declaration_would_widen:dip:id:dow"]
        );
        assert!(state.done.contains_key("dipready:dip:id:dow"));
        // 50 game days later it still waits: rule 29 wakes
        let late = START + 51 * DAY;
        assert_eq!(step(&mut state, &tools, &plan, "l", late).unwrap(), None);
        let reasons = wake_reasons(&mut state);
        assert!(
            reasons.contains(&"war_goal_expiring:dip:id:dow".to_string()),
            "{reasons:?}"
        );
        // the goal expired: the declaration is no longer offered; justify again
        tools.reply("hoi4_diplomacy", declare_rows(false));
        let later = late + 8 * DAY;
        assert_eq!(step(&mut state, &tools, &plan, "l", later).unwrap(), None);
        assert!(wake_reasons(&mut state).contains(&"war_goal_expired:dip:id:dow".to_string()));
        assert!(!state.done.contains_key("dip:id:wg"));
        assert!(!state.done.contains_key("dipready:dip:id:dow"));
        let out = step(&mut state, &tools, &plan, "l", later + DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy justify_war_goal ETH: true Verified")
        );
    }

    #[test]
    fn a_gated_declaration_estimates_expiry_from_justify_days() {
        let tools = fake(vec![]);
        let plan = json!({"diplomacy": [
            {"id": "wg", "action": "justify", "target": "ETH", "justify_days": 30},
            {"id": "dow", "action": "declare", "target": "ETH", "when": "1937.1.1"}]});
        let mut state = State::default();
        let at = date_hour("1936.1.1").unwrap();
        state.done.insert("dip:id:wg".into(), at);
        // complete about day 30; 50 days after that the gated declaration wakes the player
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + 79 * DAY).unwrap(),
            None
        );
        assert!(wake_reasons(&mut state).is_empty());
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + 81 * DAY).unwrap(),
            None
        );
        assert_eq!(
            wake_reasons(&mut state),
            vec!["war_goal_expiring:dip:id:dow"]
        );
        // the gate kept it from reading the diplomacy view, and nothing was sent
        assert_eq!(reads(&tools, "hoi4_diplomacy"), 0);
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn at_war_with_the_target_or_an_existing_goal_counts_as_done() {
        let tools = fake(vec![
            ("hoi4_wars", json!({"wars": {"enemies": ["ETH"]}})),
            (
                "hoi4_diplomacy_state",
                json!({"own_war_goals": [{"type": "conquer", "target": {"tag": "ALB"}}]}),
            ),
        ]);
        let plan = json!({"diplomacy": [
            {"id": "dow", "action": "declare", "target": "ETH"},
            {"id": "wg", "action": "justify", "target": "ALB", "war_goal": "Conquer", "states": ["Tirana"]}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
        assert!(state.done.contains_key("dip:id:dow"));
        assert!(state.done.contains_key("dip:id:wg"));
    }

    // ----- faction

    #[test]
    fn faction_join_takes_an_invitation_else_asks_and_create_follows_a_refusal() {
        let tools = fake(vec![
            (
                "hoi4_diplomacy_state",
                json!({"faction": {"in_faction": false}}),
            ),
            (
                "hoi4_diplomacy_incoming",
                json!({"proposals": [
                {"index": 1, "class": "faction_invitation", "title": "Japan wants us to join their faction",
                    "can_accept": true}]}),
            ),
            (
                "hoi4_diplomacy_countries",
                json!({"countries": [{"tag": "JAP", "name": "Japan"}]}),
            ),
        ]);
        let plan = json!({"faction": {"join": "JAP", "create": {"name": "Pact"}}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("faction invitation 1 accepted (JAP): true Verified")
        );
        assert_eq!(actions(&tools)[0].0, "hoi4_faction_invite_accept");
        // now a member: nothing more, the create is done too
        tools.reply(
            "hoi4_diplomacy_state",
            json!({"faction": {"in_faction": true, "faction": {"name": "Co-Prosperity"}}}),
        );
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert!(state.done.contains_key("faction:create"));
        assert_eq!(actions(&tools).len(), 1);

        // no invitation: ask; the AI refuses; the faction is created instead, once
        let tools = fake(vec![(
            "hoi4_diplomacy_state",
            json!({"faction": {"in_faction": false}}),
        )]);
        tools.refuse("hoi4_diplomacy_act join_faction", "ai_would_refuse");
        let plan = json!({"faction": {"join": "GER", "create": {"name": "Pact", "template": "t1",
            "invite": ["Hungary"]}}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out
            .unwrap()
            .starts_with("faction join GER: false ai_would_refuse"));
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(out.as_deref(), Some("faction create: true Verified"));
        let sent = actions(&tools);
        assert_eq!(sent[1].0, "hoi4_faction_create");
        assert_eq!(sent[1].1["invite"], json!(["Hungary"]));
        assert_eq!(sent[1].1["template"], "t1");
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 2).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn faction_waits_on_an_unreadable_membership_and_its_when() {
        let tools = fake(vec![]);
        tools
            .unreadable
            .borrow_mut()
            .insert("hoi4_diplomacy_state".into());
        let plan = json!({"faction": {"create": {"name": "Pact"}}});
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(state.affairs.notes["faction"].contains("unreadable"));
        let plan = json!({"faction": {"create": {"name": "Pact"}, "when": "bop:x > 0.5"}});
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(state.affairs.notes["faction"], "when unknown");
        assert!(actions(&tools).is_empty());
    }

    // ----- trade

    #[test]
    fn trade_takes_the_best_partner_once_and_waits_while_auto_import_is_on() {
        let tools = fake(vec![(
            "hoi4_trade",
            json!({"partners": [
            {"country": "Romania", "available": "0/4"},
            {"country": "Soviet Union", "available": "2/12"},
            {"country": "United States", "available": "1/12"}]}),
        )]);
        let plan = json!({"trade": [
            {"id": "oil", "resource": "oil", "amount": 8},
            {"id": "steel", "resource": "steel", "amount": "needed", "from": "SWE"}]});
        let mut state = State::default();
        tools.refuse("hoi4_trade_set", "auto_import_enabled");
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out
            .unwrap()
            .starts_with("trade oil 8 from Soviet Union: false auto_import_enabled"));
        assert!(state.affairs.notes["trade"].contains("automatic importing is on"));
        assert!(state.skipped(START + 29 * DAY, "trade", "trade:id:oil"));
        tools.accept("hoi4_trade_set");
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("trade steel \"needed\" from SWE: true Verified")
        );
        let out = step(&mut state, &tools, &plan, "l", START + 31 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("trade oil 8 from Soviet Union: true Verified")
        );
        let mut again = restarted(&state, START + 32 * DAY);
        assert_eq!(
            step(&mut again, &tools, &plan, "l", START + 32 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 3);
        // the object shape {deals:[..]} with `partner` is read too
        let plan = json!({"trade": {"deals": [{"id": "r", "resource": "rubber", "amount": 3, "partner": "HOL"}]}});
        let out = step(&mut again, &tools, &plan, "l", START + 32 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("trade rubber 3 from HOL: true Verified")
        );
    }

    // ----- mio

    #[test]
    fn mio_traits_queue_in_order_once_and_a_refused_one_waits() {
        let tools = fake(vec![]);
        let plan = json!({"mio": [{"organisation": "ITA_ansaldo_organization",
            "traits": [3, "5", "armor_focus"], "design_team": true, "slot": 1}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio ITA_ansaldo_organization design team slot 1: true Verified")
        );
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio ITA_ansaldo_organization trait 3: true Verified")
        );
        tools.refuse("hoi4_mio_trait", "trait_unavailable");
        let out = step(&mut state, &tools, &plan, "l", START + 2).unwrap();
        assert!(out.unwrap().contains("trait 5: false trait_unavailable"));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 13 * DAY).unwrap(),
            None
        );
        tools.accept("hoi4_mio_trait");
        tools.refuse("hoi4_mio_trait", "trait_already_queued");
        let out = step(&mut state, &tools, &plan, "l", START + 15 * DAY).unwrap();
        assert!(out.unwrap().contains("trait 5: false trait_already_queued"));
        assert!(state.done.contains_key("mio:ITA_ansaldo_organization:t5"));
        // the text trait stops the list with a note; nothing more is sent, also after a restart
        let mut again = restarted(&state, START + 16 * DAY);
        assert_eq!(
            step(&mut again, &tools, &plan, "l", START + 16 * DAY).unwrap(),
            None
        );
        assert!(again.affairs.notes["mio"].contains("armor_focus"));
        let sent = actions(&tools);
        assert_eq!(sent.len(), 4);
        assert_eq!(sent[1].1["trait_index"], 3);
        assert_eq!(sent[1].1["slot"], 1);
    }

    #[test]
    fn an_absent_mio_system_leaves_the_whole_key_alone() {
        let tools = fake(vec![]);
        tools.refuse("hoi4_mio_trait", "mio_system_absent");
        let plan =
            json!({"mio": [{"organization": "GER_krupp", "traits": [0], "when": "1936.1.1"}]});
        let mut state = State::default();
        let at = date_hour("1936.2.1").unwrap();
        step(&mut state, &tools, &plan, "l", at).unwrap();
        assert!(state.skipped(at + 29 * DAY, "mmio", "x"));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
    }

    // ----- intelligence

    #[test]
    fn the_agency_is_made_then_upgrades_follow_in_order_when_none_runs() {
        let tools = fake(vec![(
            "hoi4_intelligence",
            json!({
            "upgrades": {"branches": [
                {"branch": "branch_intelligence", "upgrades": [{"key": "upgrade_army_department"}]},
                {"branch": "branch_cryptology", "upgrades": [{"key": "upgrade_decryption_tools"}]}]},
            "defines": {"AGENCY_CREATION_DAYS": 30.0, "AGENCY_UPGRADE_DAYS": 30.0}}),
        )]);
        let plan = json!({"intelligence": {"agency": true,
            "upgrades": ["upgrade_army_department", "upgrade_decryption_tools", "upgrade_army_department"],
            "operatives": 1}});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("intelligence agency create: true Verified")
        );
        // nothing while the agency forms
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 29 * DAY).unwrap(),
            None
        );
        assert!(state.affairs.notes["intelligence"].contains("being formed"));
        let at = START + 30 * DAY;
        let out = step(&mut state, &tools, &plan, "l", at).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("intelligence upgrade branch_intelligence/upgrade_army_department: true Verified")
        );
        // the next upgrade waits for this one; operatives are recruited meanwhile
        let out = step(&mut state, &tools, &plan, "l", at + DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("intelligence operative 0 recruit: true Verified")
        );
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + 2 * DAY).unwrap(),
            None
        );
        // 30 days on: the game still runs one (a slower upgrade): retry in 5 days
        tools.refuse("hoi4_agency_upgrade", "upgrade_in_progress");
        let out = step(&mut state, &tools, &plan, "l", at + 30 * DAY).unwrap();
        assert!(out
            .unwrap()
            .contains("upgrade_decryption_tools: false upgrade_in_progress"));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + 34 * DAY).unwrap(),
            None
        );
        // the decryption upgrade is unavailable: the second army level goes first
        tools.refuse("hoi4_agency_upgrade", "upgrade_unavailable");
        let out = step(&mut state, &tools, &plan, "l", at + 35 * DAY).unwrap();
        assert!(out
            .unwrap()
            .contains("upgrade_decryption_tools: false upgrade_unavailable"));
        tools.accept("hoi4_agency_upgrade");
        let out = step(&mut state, &tools, &plan, "l", at + 36 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("intelligence upgrade branch_intelligence/upgrade_army_department: true Verified")
        );
        assert!(state
            .done
            .contains_key("intel:up:upgrade_army_department#2"));
        let sent: Vec<String> = actions(&tools).iter().map(|(n, _)| n.clone()).collect();
        assert_eq!(
            sent.iter().filter(|n| *n == "hoi4_agency_create").count(),
            1
        );
        // persisted: the agency is not created again after a restart
        let mut again = restarted(&state, at + 37 * DAY);
        assert_eq!(
            step(&mut again, &tools, &plan, "l", at + 37 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), sent.len());
    }

    #[test]
    fn an_existing_agency_counts_and_operative_missions_follow_recruitment() {
        let tools = fake(vec![]);
        tools.refuse("hoi4_agency_create", "agency_exists");
        let plan = json!({"intelligence": {"agency": true,
            "operatives": [{"mission": "build_intel_network", "state_id": 64}]}});
        let mut state = State::default();
        step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(state.done.contains_key("intel:agency"));
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("intelligence operative 0 recruit: true Verified")
        );
        tools.refuse("hoi4_operative_assign", "operative_unknown");
        let out = step(&mut state, &tools, &plan, "l", START + 2).unwrap();
        assert!(out.unwrap().contains("false operative_unknown"));
        tools.accept("hoi4_operative_assign");
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 6 * DAY).unwrap(),
            None
        );
        let out = step(&mut state, &tools, &plan, "l", START + 8 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("intelligence operative 0 mission build_intel_network: true Verified")
        );
        let assigned = actions(&tools).last().cloned().unwrap();
        assert_eq!(assigned.1["state_id"], 64);
        assert_eq!(assigned.1["operative"], 0);
    }

    // ----- research bonuses

    fn idle_research() -> Value {
        json!({"slots": [{"researching": null}], "researched": []})
    }

    #[test]
    fn an_active_research_bonus_puts_its_techs_first_through_the_tick() {
        struct Tech(Fake);
        impl Tools for Tech {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                if name == "hoi4_technology_list" && args["available"] == json!(true) {
                    self.0.calls.borrow_mut().push((name.into(), args.clone()));
                    return Ok(json!({"available_only": true,
                        "technologies": ["infantry_weapons1", "basic_machine_tools", "radio", "jet_engines"],
                        "start_years": {"infantry_weapons1": 1936, "basic_machine_tools": 1936,
                            "radio": 1936, "jet_engines": 1939}}));
                }
                self.0.call(name, args)
            }
        }
        let extras = |uses: i64| {
            json!({"tech_bonuses": {"entries": [
                {"name": "ITA_industrial_expansion", "uses_left": uses, "bonus": 1.0,
                    "categories": ["industry"]},
                {"name": "ITA_arms", "uses_left": uses, "bonus": 0.5,
                    "categories": ["infantry_weapons"]}]}})
        };
        let tools = Tech(fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_research_extras", extras(1)),
        ]));
        let now = date_hour("1936.3.1").unwrap();
        let plan = json!({"research_priority": ["radio", "infantry_weapons1"],
            "research_bonuses": {"industry": ["basic_machine_tools", "jet_engines"]}});
        let mut state = State::default();
        let out = tick(&mut state, &tools, &plan, "l", now).unwrap();
        assert_eq!(
            out,
            Tick::Acted("research basic_machine_tools slot 0".into())
        );
        assert!(state.affairs.notes["research_bonuses"].contains("ITA_industrial_expansion"));
        // without an active bonus the plan's order stands
        let tools = Tech(fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_research_extras", extras(0)),
        ]));
        let mut state = State::default();
        let out = tick(&mut state, &tools, &plan, "l", now).unwrap();
        assert_eq!(out, Tick::Acted("research radio slot 0".into()));
    }

    #[test]
    fn bonus_first_matches_category_words_and_never_takes_techs_ahead_of_time() {
        let tools = fake(vec![(
            "hoi4_research_extras",
            json!({"tech_bonuses": {"entries": [
            {"name": "x", "uses_left": 2, "categories": ["cat_fortification"]},
            {"name": "y", "uses_left": 0, "categories": ["radio"]}]}}),
        )]);
        let available = json!({"start_years": {"fortification2": 1940}});
        let now = date_hour("1936.3.1").unwrap();
        let mut state = State::default();
        let mut priority: Vec<String> = ["radio", "fortification1", "fortification2"]
            .map(String::from)
            .to_vec();
        bonus_first(
            &mut state,
            &tools,
            &json!({"research_bonuses": true}),
            now,
            Some(&available),
            &mut priority,
        );
        assert_eq!(priority, vec!["fortification1", "radio", "fortification2"]);
        // no research_bonuses key: untouched and nothing read
        let mut untouched = vec!["radio".to_string()];
        bonus_first(&mut state, &tools, &json!({}), now, None, &mut untouched);
        assert_eq!(untouched, vec!["radio"]);
        assert_eq!(reads(&tools, "hoi4_research_extras"), 1);
    }

    #[test]
    fn the_affairs_step_runs_through_the_tick_and_reports_its_lines() {
        let tools = fake(vec![]);
        let plan = json!({"diplomacy": [{"id": "g", "action": "guarantee", "target": "ALB"}]});
        let mut state = State::default();
        let out = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out,
            Tick::Acted("diplomacy guarantee ALB: true Verified".into())
        );
        let plan = json!({"diplomacy": [{"id": "h", "action": "guarantee", "target": "ALB",
            "when": "bop:x > 1"}]});
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START + 1).unwrap(),
            Tick::Nothing
        );
        assert!(digest(&state).unwrap()["diplomacy"]
            .as_str()
            .unwrap()
            .contains("h: when unknown"));
    }
}
