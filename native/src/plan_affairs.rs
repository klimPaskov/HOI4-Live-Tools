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
//!                is never switched at once: the design starts as a
//!                separate line (hoi4_production_add with `design`, 10% of the old line's factories,
//!                1-5) on free factories and the old line keeps building until its old gear covers
//!                `switch_at_cover_days` (30; on the entry or the `designs` item; false: never) game days
//!                of field demand (hoi4_stockpile: no division short of it and days_of_cover at least
//!                that, or a stock that is not falling), then switches whole; "switch_mature": true on the
//!                entry (or on the `designs` item) switches the whole line at once. A same-type variant
//!                (90% kept) switches as before;
//!   production[].replace  the equipment type the line moves to from `from`: the newest own design of
//!                that type (hoi4_designs hull_or_chassis, not decommissioned; `design` names one), with the
//!                guard above; a line the game cannot switch to that type (design_wrong_type) moves its
//!                factories to a line of the new type (hoi4_production_set, else hoi4_production_add) and
//!                then closes (factories 0). The production step adds no line for a `replace` entry
//!   designs[].decommission  older designs (names or ids) decommissioned once each through
//!                hoi4_design_obsolete after the item's design (by `name`) runs on a production line;
//!                already_obsolete counts as done;
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
//!                "call_to_arms" (a faction member into our war; the ally decides),
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
//!   faction.call_allies  "all" | [TAG..] | "none": while at war, faction members outside our war are
//!                called to arms (hoi4_diplomacy_act call_to_arms), each again after 30 days; absent: no
//!                call and a weekly note naming the members left out
//!   incoming_proposals  [{"id"?,"class":"any"|<class>,"from"?:[TAG..],"answer":"accept"|"refuse"|"player",
//!                "when"?}] the answers to the open diplomatic pop-ups (hoi4_diplomacy_incoming: faction
//!                invitation, faction_join_request (a country asks into ours; standing answer: refuse a
//!                planned war target, accept the rest), join_war_offer (accepted), guarantee, non-aggression pact, military access, peace offer, call to arms,
//!                lend-lease request, manpower request, other), once a game day while none is open: the
//!                first rule matching a proposal's `class` (any: every class) and sender (`from`: the
//!                pop-up names the tag or the country's name; absent: anyone) whose `when` holds answers it
//!                (a rule whose `when` is unknown holds the proposal), "player" and no rule wake the player
//!                once per proposal; an answer the pop-up does not offer wakes too. A faction invitation
//!                `faction.join` still wants is left to the faction step. Tool never live-verified
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
//!                (hoi4_operative_assign, country `target` or `state_id`); an operative back from an
//!                operation (idle, on none) gets its mission again. `networks` [{"target","state_id"?,
//!                "operative"?,"until"?}]: an operative builds a network in each target until its
//!                strength reaches `until` (default 100). `operations` [{"operation","target","when"?,
//!                "operatives"?,"auto_commence"?,"runs"?,"repeat_until"?:{"collaboration"}}]: started
//!                with hoi4_operation_start when hoi4_operations judges it ready and it does not run
//!                already; `runs` times (default 1) or until the target's collaboration reaches the
//!                goal (at most `runs`, default 5). Refusals and the agency state come from memory
//!                (receipts verified by memory readback, live 14 Nov)
//!   mio          [{"organisation" ("organization" too),"traits":[trait_index,..],"when"?,"slot"?,
//!                "line"?,"design_team"?,"policy"?}] per organisation, in order: `line` (an equipment key)
//!                puts it on the own production line of that equipment family (hoi4_mio_line; the
//!                politics view's slots only browse, so `design_team` assigns nothing and is settled),
//!                and every other line that can take an organisation gets the first its roster offers
//!                (rule mio_lines, manage.off "mio_lines"); an empty `traits` list (or "auto") keeps one
//!                trait queued (rule mio_traits, monthly listing);
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

/// A section's last status line (the alerts rule reads the owning step's reason here).
pub(super) fn note_of<'a>(state: &'a State, key: &str) -> Option<&'a str> {
    state.affairs.notes.get(key).map(String::as_str)
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
/// The wait names the atoms that read false, so a status line says what holds the item back
/// (g17j3: "when not met" for months with no way to tell which condition failed).
fn gate(state: &mut State, tools: &dyn Tools, when: &Value, now: GameHour) -> Option<String> {
    match when_holds(state, tools, when, now) {
        Truth::True => None,
        Truth::False => {
            let mut ctx = crate::plan_predicate::Ctx::new(tools, now);
            ctx.phase = state.strategy.phase_id();
            let false_atoms = crate::plan_predicate::false_atoms(when, &mut ctx);
            Some(if false_atoms.is_empty() {
                "when not met".to_string()
            } else {
                format!("when not met ({} false)", false_atoms.join(", "))
            })
        }
        Truth::Unknown => Some("when unknown".to_string()),
    }
}

/// The typed code of a refusal (`code: message` from `State::act`), else the receipt status.
fn code(why: &str) -> &str {
    why.split(':').next().unwrap_or("").trim()
}

/// How long a due declaration waits for its operation's prepare to read back.
const OP_DECLARE_WAIT: GameHour = 10 * DAY;

/// One section's view of the tick.
struct Run<'a> {
    tools: &'a dyn Tools,
    plan: &'a Value,
    lease: &'a str,
    now: GameHour,
}

impl<'a> Run<'a> {
    fn new(tools: &'a dyn Tools, plan: &'a Value, lease: &'a str, now: GameHour) -> Self {
        Run {
            tools,
            plan,
            lease,
            now,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
type Section = fn(&mut State, &Run<'_>) -> Result<Option<String>>;

/// The affairs sections as lanes of the pass (plan_run.rs `lanes`), in their old chain order;
/// the design companies are three lanes: lines first, the plan's organisations, then the traits
/// of every organisation on a line.
pub(super) const LANES: [super::Lane; 12] = [
    ("diplomacy", |s, t, p, l, n| {
        diplomacy_step(s, &Run::new(t, p, l, n))
    }),
    ("faction", |s, t, p, l, n| {
        faction_step(s, &Run::new(t, p, l, n))
    }),
    ("call_allies", |s, t, p, l, n| {
        call_allies_step(s, &Run::new(t, p, l, n))
    }),
    ("incoming", |s, t, p, l, n| {
        incoming_step(s, &Run::new(t, p, l, n))
    }),
    ("trade", |s, t, p, l, n| {
        trade_step(s, &Run::new(t, p, l, n))
    }),
    ("designs", |s, t, p, l, n| {
        designs_step(s, &Run::new(t, p, l, n))
    }),
    ("line_designs", |s, t, p, l, n| {
        line_design_step(s, &Run::new(t, p, l, n))
    }),
    ("decommission", |s, t, p, l, n| {
        decommission_step(s, &Run::new(t, p, l, n))
    }),
    ("mio", |s, t, p, l, n| mio_plan(s, &Run::new(t, p, l, n))),
    ("mio_lines", |s, t, p, l, n| {
        mio_lines(s, &Run::new(t, p, l, n))
    }),
    ("mio_traits", |s, t, p, l, n| {
        mio_auto_traits(s, &Run::new(t, p, l, n))
    }),
    ("intelligence", |s, t, p, l, n| {
        intelligence_step(s, &Run::new(t, p, l, n))
    }),
];

/// The affairs sections in one chain (the tests' view of the old domain).
#[cfg_attr(not(test), allow(dead_code))]
pub fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    // a direct call stands for a tick at `now` (the ledger counts game hours)
    state.clock = now;
    let run = Run::new(tools, plan, lease, now);
    let sections: [Section; 10] = [
        diplomacy_step,
        faction_step,
        call_allies_step,
        incoming_step,
        trade_step,
        designs_step,
        line_design_step,
        decommission_step,
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
            let have = read
                .as_ref()
                .and_then(|d| d["experience"][b]["value"].as_f64());
            // the spends the plan's xp_budget lists before this design are paid first
            let who = item_id(entry).map_or(super::plan_xp::Spender::Unlisted, |id| {
                super::plan_xp::Spender::Ref(format!("design:{id}"))
            });
            let (before, _) = super::plan_xp::ahead(state, tools, plan, b, &who, now);
            if let Some(have) = have {
                if have < need + before {
                    waits.push(format!(
                        "{label}: {b} XP {have} < {need} + {before:.0} held"
                    ));
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

/// One wanted switch of a production line to another design: `production[].design`,
/// `production[].replace` or `designs[].production`.
struct Switch {
    /// The plan's line equipment (a live line whose type is it or starts with it).
    equipment: String,
    /// The design's label in notes and in the done key.
    label: String,
    /// The design by name or id; Null with `by_type`: the newest own design of that type.
    design: Value,
    /// `replace`: the equipment type the line moves to (its factories go with it).
    by_type: Option<String>,
    /// The `designs` item key the switch waits for.
    gate: Option<String>,
    accept: bool,
    /// `switch_mature`: a mature line switches whole at once.
    mature: bool,
    /// `switch_at_cover_days`: a mature line kept beside the new tier switches whole once its old
    /// gear covers this many game days of field demand; None keeps it building for good.
    cover_days: Option<f64>,
    /// The entry's `from`: nothing happens before it.
    from: Value,
}

/// Game days of old-gear cover after which a mature line switches whole (the playbook's tier
/// rule: start the new tier beside it, switch the old line once its gear covers about a month).
const SWITCH_COVER_DAYS: f64 = 30.0;

/// `switch_at_cover_days`: a number of game days, false (never), or absent (`default`).
fn cover_days(v: &Value, default: Option<f64>) -> Option<f64> {
    match v {
        Value::Bool(false) => None,
        Value::Bool(true) => Some(SWITCH_COVER_DAYS),
        v => v.as_f64().filter(|d| *d >= 0.0).or(default),
    }
}

fn wanted_switches(plan: &Value) -> Vec<Switch> {
    let designs = list(plan, "designs");
    let mut out: Vec<Switch> = Vec::new();
    for line in list(plan, "production") {
        let equipment = text(&line["equipment"]).trim().to_string();
        let design = &line["design"];
        let replace = text(&line["replace"]).trim().to_string();
        if equipment.is_empty() || line["remove"] == json!(true) {
            continue;
        }
        let by_type = (!replace.is_empty() && replace != equipment).then_some(replace);
        if design.is_null() && by_type.is_none() {
            continue;
        }
        let accept = line["accept_progress_loss"] != json!(false);
        let mature = line["switch_mature"] == json!(true);
        let own_cover = &line["switch_at_cover_days"];
        // a `designs` id names the design that item makes
        let item = design.as_str().and_then(|d| {
            designs
                .iter()
                .find(|e| item_id(e).as_deref() == Some(d.trim()))
        });
        let (label, wanted, gate, mature, cover) = match (item, design) {
            (Some(e), _) => (
                text(&e["name"]).trim().to_string(),
                json!(text(&e["name"]).trim()),
                item_id(e).map(|id| format!("design:id:{id}")),
                mature || e["switch_mature"] == json!(true),
                if own_cover.is_null() {
                    cover_days(&e["switch_at_cover_days"], Some(SWITCH_COVER_DAYS))
                } else {
                    cover_days(own_cover, Some(SWITCH_COVER_DAYS))
                },
            ),
            (None, Value::String(name)) => (
                name.trim().to_string(),
                json!(name.trim()),
                None,
                mature,
                cover_days(own_cover, Some(SWITCH_COVER_DAYS)),
            ),
            (None, Value::Number(id)) => (
                id.to_string(),
                design.clone(),
                None,
                mature,
                cover_days(own_cover, Some(SWITCH_COVER_DAYS)),
            ),
            // `replace` without a design: the newest own design of the new type
            _ => match &by_type {
                Some(t) => (
                    t.clone(),
                    Value::Null,
                    None,
                    mature,
                    cover_days(own_cover, Some(SWITCH_COVER_DAYS)),
                ),
                None => continue,
            },
        };
        out.push(Switch {
            equipment,
            label,
            design: wanted,
            by_type,
            gate,
            accept,
            mature,
            cover_days: cover,
            from: line["from"].clone(),
        });
    }
    for e in designs {
        let equipment = text(&e["production"]).trim().to_string();
        let name = text(&e["name"]).trim().to_string();
        if equipment.is_empty()
            || out
                .iter()
                .any(|w| w.equipment == equipment && w.label == name)
        {
            continue;
        }
        out.push(Switch {
            equipment,
            label: name.clone(),
            design: json!(name),
            by_type: None,
            gate: item_id(e).map(|id| format!("design:id:{id}")),
            accept: e["accept_progress_loss"] != json!(false),
            mature: e["switch_mature"] == json!(true),
            cover_days: cover_days(&e["switch_at_cover_days"], Some(SWITCH_COVER_DAYS)),
            from: Value::Null,
        });
    }
    out
}

/// Requested factories of a live line.
fn line_factories(line: &Value) -> i64 {
    line["factory_counters"]["requested_factories_raw_signed"]
        .as_i64()
        .unwrap_or(0)
}

/// Whether the stock of a line's old gear lets the line switch whole (hoi4_stockpile, the
/// archetype or variant row of `line_type`): no division still short of it, and either a measured
/// stock that is not falling or at least `days` game days of cover. Ok(what holds) or Err(why it waits);
/// an unreadable or unmeasured stock waits.
fn old_gear_covers(
    tools: &dyn Tools,
    line_type: &str,
    days: f64,
) -> std::result::Result<String, String> {
    let read = tools
        .call("hoi4_stockpile", &json!({"equipment": line_type}))
        .map_err(|_| "hoi4_stockpile unreadable".to_string())?;
    let row = list(&read, "equipment")
        .iter()
        .find(|r| {
            let a = text(&r["archetype"]);
            (!a.is_empty() && line_type.starts_with(a))
                || list(r, "variants")
                    .iter()
                    .any(|v| text(&v["type"]) == line_type)
        })
        .ok_or_else(|| format!("no stockpile row for {line_type}"))?;
    let short = row["fielded_short"].as_f64().unwrap_or(0.0);
    if short > 0.0 {
        return Err(format!("the divisions still lack {short:.0}"));
    }
    // a trend the server has not measured yet says nothing
    if row["depleting"] == json!(false) && row["net_per_day"].is_number() {
        return Ok("the stock is not falling".into());
    }
    match row["days_of_cover"].as_f64() {
        Some(d) if d >= days => Ok(format!("{d:.0} days of cover")),
        Some(d) => Err(format!("{d:.0} days of cover < {days:.0}")),
        None => Err("cover not measured yet".into()),
    }
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
    for sw in wanted {
        let (equipment, label) = (sw.equipment.as_str(), sw.label.as_str());
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
        if !super::reached(&json!({"from": sw.from}), now) {
            waits.push(format!("{equipment} -> {label}: from {}", sw.from));
            continue;
        }
        if sw
            .gate
            .as_ref()
            .is_some_and(|k| !state.done.contains_key(k))
        {
            waits.push(format!("{equipment} -> {label}: design not made yet"));
            continue;
        }
        let found = match &sw.design {
            Value::String(name) => design_named(tools, name),
            design => {
                let all = all_designs.get_or_insert_with(|| {
                    tools
                        .call("hoi4_designs", &json!({}))
                        .map(|r| list(&r, "designs").to_vec())
                        .unwrap_or_default()
                });
                match &sw.by_type {
                    // `replace`: the newest own design of the new type that is not decommissioned
                    Some(t) if design.is_null() => all
                        .iter()
                        .filter(|d| {
                            text(&d["hull_or_chassis"]) == t && d["obsolete"] != json!(true)
                        })
                        .max_by_key(|d| d["design_id"].as_i64().unwrap_or(0))
                        .cloned(),
                    _ => all
                        .iter()
                        .find(|d| d["design_id"].as_i64() == design.as_i64())
                        .cloned(),
                }
            }
        };
        let Some(found) = found else {
            waits.push(match &sw.by_type {
                Some(t) if sw.design.is_null() => {
                    format!("{equipment} -> {label}: no own design of {t} yet")
                }
                _ => format!("{equipment} -> {label}: design not found"),
            });
            continue;
        };
        let design_type = text(&found["hull_or_chassis"]).to_string();
        let rows = lines.get_or_insert_with(|| {
            tools
                .call("hoi4_production_lines", &json!({"limit": 128}))
                .map(|r| list(&r, "lines").to_vec())
                .unwrap_or_default()
        });
        // The line to switch: one of the plan's equipment, a line of another type than the
        // design's first (a separate line of the new tier may already run beside it).
        let of_equipment = |l: &&Value| {
            let e = text(&l["equipment_id"]);
            !e.is_empty() && (e == equipment || e.starts_with(equipment))
        };
        let Some(line) = rows
            .iter()
            .filter(of_equipment)
            .find(|l| design_type.is_empty() || text(&l["equipment_id"]) != design_type)
            .or_else(|| rows.iter().find(of_equipment))
            .cloned()
        else {
            waits.push(format!("{equipment} -> {label}: no line yet"));
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
            .unwrap_or_else(|| sw.design.clone());
        let line_type = text(&line["equipment_id"]).to_string();
        let move_key = format!("pmove:{equipment}:{label}");
        if state.done.contains_key(&move_key) || state.done.contains_key(&format!("pwrong:{key}")) {
            // The game does not switch this line to the new type (another family): its factories
            // move to a line of the new type, then the old line closes.
            if !state.done.contains_key(&move_key) {
                let have = line_factories(&line);
                let new_line = rows
                    .iter()
                    .find(|l| text(&l["equipment_id"]) == design_type)
                    .cloned();
                let (tool, args) = match &new_line {
                    Some(n) => (
                        "hoi4_production_set",
                        json!({"line": n["entry_index"], "equipment": design_type,
                            "factories": (line_factories(n) + have).clamp(1, 150)}),
                    ),
                    None => (
                        "hoi4_production_add",
                        json!({"equipment": design_type, "design": target,
                            "factories": have.clamp(1, 150)}),
                    ),
                };
                let (ok, why) = state.act(tools, run.lease, tool, args, "dm")?;
                if ok {
                    state.done.insert(move_key, now);
                } else {
                    state.skip(now, "pdes", &key, 3 * DAY);
                }
                return Ok(Some(format!(
                    "production replace {equipment} -> {design_type}: {have} factories to the new line: {ok} {why}"
                )));
            }
            let (ok, why) = state.act(
                tools,
                run.lease,
                "hoi4_production_set",
                json!({"line": line["entry_index"], "equipment": line_type, "factories": 0}),
                "dr",
            )?;
            if ok {
                state.done.insert(key, now);
            } else {
                state.skip(now, "pdes", &key, 3 * DAY);
            }
            return Ok(Some(format!(
                "production replace {equipment} -> {design_type}: old line {line_type} closed: {ok} {why}"
            )));
        }
        // Mature-line guard: a switch to another equipment type (a tier or chassis change keeps
        // 30% or less of the line's efficiency, a same-type variant 90%) never takes the
        // factories of a mature line at once. The new design starts as a separate line on free
        // factories while the old line keeps building; the old line switches whole once its gear
        // covers `switch_at_cover_days` (30) game days of field demand (false: never). Only
        // `switch_mature: true` switches the whole line at once. The line's efficiency is read
        // (hoi4_production_lines output.efficiency, percent): at or above
        // `manage.mature_line_efficiency` (30) the line is mature. Without the read a line counts
        // as young only when the executor added it less than `manage.mature_line_days` (30) game
        // days ago.
        let mut covered: Option<String> = None;
        if !sw.mature
            && !line_type.is_empty()
            && !design_type.is_empty()
            && line_type != design_type
        {
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
                let age = match efficiency {
                    Some(e) => {
                        format!("efficiency {e:.0}% read, mature from {mature_efficiency:.0}%")
                    }
                    None => added.map_or_else(
                        || "not added by the executor; no line efficiency read".to_string(),
                        |h| format!("added {} game days ago", (now - h) / DAY),
                    ),
                };
                let separate_key = format!("pdesnew:{equipment}:{label}");
                let separate = state.done.contains_key(&separate_key)
                    || rows.iter().any(|l| text(&l["equipment_id"]) == design_type);
                if !separate {
                    let start = ((line_factories(&line) as f64 * 0.1).round() as i64).clamp(1, 5);
                    let (ok, why) = state.act(
                        tools,
                        run.lease,
                        "hoi4_production_add",
                        json!({"equipment": design_type, "design": target, "factories": start}),
                        "dn",
                    )?;
                    if ok {
                        state.done.insert(separate_key, now);
                    } else {
                        state.skip(now, "pdes", &key, 3 * DAY);
                    }
                    note(
                        state,
                        "production_tier_guard",
                        format!("{equipment} -> {label}: {line_type} to {design_type} changes the type and the line is mature ({age}): it keeps its factories, the new design starts on a separate line with {start}: {ok}"),
                    );
                    return Ok(Some(format!(
                        "production {label} on a separate line ({start} factories), mature line {equipment} kept: {ok} {why}"
                    )));
                }
                let Some(days) = sw.cover_days else {
                    state.done.insert(key, now);
                    note(
                        state,
                        "production_tier_guard",
                        format!("{equipment} -> {label}: {design_type} runs on its own line; the mature {line_type} line keeps building (switch_at_cover_days false)"),
                    );
                    continue;
                };
                match old_gear_covers(tools, &line_type, days) {
                    Ok(why) => covered = Some(why),
                    Err(why) => {
                        waits.push(format!(
                            "{equipment} -> {label}: mature {line_type} line kept until its gear covers {days:.0} days ({why})"
                        ));
                        note(
                            state,
                            "production_tier_guard",
                            format!("{equipment} -> {label}: {design_type} runs on its own line; the mature {line_type} line ({age}) switches whole at {days:.0} days of cover: {why}"),
                        );
                        continue;
                    }
                }
            }
        }
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_design_set_production",
            json!({"line": line["entry_index"], "design": target, "accept_progress_loss": sw.accept}),
            "dl",
        )?;
        let c = code(&why);
        if ok || c == "already_building" {
            state.done.insert(key, now);
        } else if c == "design_wrong_type" && sw.by_type.is_some() {
            // `replace` across families: the factories move instead (next ticks)
            state.done.insert(format!("pwrong:{key}"), now);
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
        let whole = covered.map_or_else(String::new, |why| {
            format!(" (mature line switched whole: old gear {why})")
        });
        return Ok(Some(format!(
            "production line {equipment} -> design {label}{whole}: {ok} {why}"
        )));
    }
    if !waits.is_empty() {
        note(state, "production_designs", waits.join("; "));
        state.skip(now, "mpdes", "x", DAY);
    }
    Ok(None)
}

/// `designs[].decommission`: once the item's design is made and a production line builds it,
/// each listed older design (a name or a design id) is decommissioned once through
/// hoi4_design_obsolete, so training and the selection lists stop offering it. already_obsolete
/// counts as done; an unknown design is noted and tried again after 30 game days.
fn decommission_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    if state.skipped(now, "mdecom", "x") {
        return Ok(None);
    }
    let mut waits: Vec<String> = Vec::new();
    let mut any = false;
    for entry in list(plan, "designs") {
        let old: Vec<&Value> = list(entry, "decommission")
            .iter()
            .filter(|d| d.is_string() || d.is_u64())
            .collect();
        if old.is_empty() {
            continue;
        }
        any = true;
        let name = text(&entry["name"]).trim().to_string();
        let label = item_id(entry).unwrap_or_else(|| name.clone());
        let pending: Vec<&Value> = old
            .into_iter()
            .filter(|d| {
                let k = format!("decom:{}", text_or_number(d));
                !state.done.contains_key(&k) && !state.skipped(now, "decom", &k)
            })
            .collect();
        if pending.is_empty() {
            continue;
        }
        let new = if name.is_empty() {
            None
        } else {
            design_named(tools, &name)
        };
        let Some(new) = new else {
            waits.push(format!("{label}: the new design is not made yet"));
            continue;
        };
        if list(&new, "production_lines").is_empty() {
            waits.push(format!("{label}: no line builds {name} yet"));
            continue;
        }
        let design = pending[0].clone();
        let k = format!("decom:{}", text_or_number(&design));
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_design_obsolete",
            json!({"design": design, "obsolete": true}),
            "do",
        )?;
        let c = code(&why);
        if ok || c == "already_obsolete" {
            state.done.insert(k, now);
        } else {
            let days = if c == "design_unknown" { 30 } else { 3 };
            note(
                state,
                "designs_decommission",
                format!("{label}: decommission {}: {why}", text_or_number(&design)),
            );
            state.skip(now, "decom", &k, days * DAY);
        }
        return Ok(Some(format!(
            "design {} decommissioned ({name} in production): {ok} {why}",
            text_or_number(&design)
        )));
    }
    if any {
        if !waits.is_empty() {
            note(state, "designs_decommission", waits.join("; "));
        }
        state.skip(now, "mdecom", "x", DAY);
    }
    Ok(None)
}

/// A design reference as text: a name, or a design id number.
fn text_or_number(v: &Value) -> String {
    v.as_str()
        .map_or_else(|| v.to_string(), |s| s.trim().to_string())
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
        "call" | "call_ally" | "call_to_arms" | "call_to_war" => "call_to_arms",
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
    diplomacy_items(state, run, false)
}

/// Whether a declaration of war against the enemy of an operation that is prepared and waits
/// for its war (see `plan_operation::awaiting_war`) is due now or within `ahead` game hours
/// (its `when` judged at that hour). Judged at most every six game hours. The slow peace-time
/// orders (air rebases and missions, 10-25 s each) wait while it holds: they ran past the war
/// day and the declaration came six to nine days late on the Poland bed (rs1-rs6, 19 Nov 2026).
pub fn declaration_near(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
    ahead: GameHour,
) -> bool {
    let at = state.done.get("declnear:at").copied();
    if at.is_some_and(|t| now >= t && now - t < DAY / 4) {
        return state.done.get("declnear:v") == Some(&1);
    }
    let mut near = false;
    for item in list(plan, "diplomacy") {
        if dip_action(text(&item["action"])).as_deref() != Some("declare_war") {
            continue;
        }
        let target = dip_target(item);
        if state
            .done
            .contains_key(&dip_key(item, "declare_war", &target))
            || !super::plan_operation::awaiting_war_on(state, plan, &target)
        {
            continue;
        }
        if when_holds(state, tools, &item["when"], now + ahead) == Truth::True {
            near = true;
            break;
        }
    }
    state.done.insert("declnear:at".to_string(), now);
    state.done.insert("declnear:v".to_string(), i64::from(near));
    near
}

/// The plan's war declarations whose condition holds, ahead of everything else: a declaration
/// rides the war-first block, never a domain's overdue turn (b1a, 18 Nov 2026: Danzig or War was
/// done and the date had come on 1 September, and the declaration went out on the 13th as an
/// overdue affairs turn). Nothing but `declare_war` items; their gates, waits and the
/// done-record are the diplomacy section's own.
pub fn declare_due_step(
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
    diplomacy_items(state, &run, true)
}

/// Game hours a read of the target's diplomacy view that offered Declare war stands for: the
/// declaration on its day does not open the view again (r3a, 19 Nov 2026: the read took 15 s,
/// three game days, on the war day; 2.5 s in r1a).
const DECLARE_READ_FRESH: GameHour = 3 * DAY;

/// In the last game days before a prepared operation's declaration (the ticks where only the war
/// lane runs and nothing is due), the target's diplomacy view is read once: whether Declare war
/// is offered. True when it read.
pub fn declaration_preread(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> bool {
    let run = Run {
        tools,
        plan,
        lease,
        now,
    };
    for item in list(plan, "diplomacy") {
        if dip_action(text(&item["action"])).as_deref() != Some("declare_war") {
            continue;
        }
        let target = dip_target(item);
        let key = dip_key(item, "declare_war", &target);
        let fresh = state
            .done
            .get(&format!("dipready:{key}"))
            .is_some_and(|t| now >= *t && now - t < DECLARE_READ_FRESH);
        if state.done.contains_key(&key)
            || fresh
            || state.skipped(now, "dippre", &key)
            || !super::plan_operation::awaiting_war_on(state, plan, &target)
        {
            continue;
        }
        state.skip(now, "dippre", &key, DAY);
        if declare_ready(&run, &target) == Truth::True {
            state.done.insert(format!("dipready:{key}"), now);
        }
        return true;
    }
    false
}

fn diplomacy_items(
    state: &mut State,
    run: &Run<'_>,
    declarations_only: bool,
) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let items = list(plan, "diplomacy");
    if items.is_empty() || (!declarations_only && state.skipped(now, "mdip", "x")) {
        return Ok(None);
    }
    if declarations_only && state.skipped(now, "mdecl", "x") {
        return Ok(None);
    }
    if declarations_only
        && !items
            .iter()
            .any(|i| dip_action(text(&i["action"])).as_deref() == Some("declare_war"))
    {
        return Ok(None);
    }
    let mut waits: Vec<String> = Vec::new();
    for item in items {
        let Some(action) = dip_action(text(&item["action"])) else {
            continue;
        };
        if declarations_only && action != "declare_war" {
            continue;
        }
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
            if let Some(line) = war_goal_watch(state, run, item, &key, &target)? {
                return Ok(Some(line));
            }
        }
        if state.skipped(now, "dip", &key) {
            waits.push(format!("{label}: waiting"));
            continue;
        }
        // a goal about to lapse before the war day with the operation prepared: no gate
        let early = action == "declare_war" && state.done.contains_key(&format!("dipearly:{key}"));
        if let Some(why) = (!early)
            .then(|| gate(state, tools, &item["when"], now))
            .flatten()
        {
            // a war goal on the target and the war day reached: the declaration does not wait
            // for a focus the goal did not come from (g17k: dip_declare_pol waited on
            // GER_danzig_or_war though the goal had been justified by hand)
            let goal_day = state.done.contains_key(&format!("dipgoal:{key}"))
                && planned_day(item).is_some_and(|d| now >= d);
            let without_focus = without_focus_atoms(&item["when"]);
            if !(action == "declare_war"
                && goal_day
                && gate(state, tools, &without_focus, now).is_none())
            {
                waits.push(format!("{label}: {why}"));
                continue;
            }
        }
        // An operation against the target still preparing holds the declaration, at most
        // OP_DECLARE_WAIT from the day the declaration itself was due. The wait used to count
        // from the first tick of the prepare, weeks before the declaration's own `when`, so it
        // had always run out: b1f (18 Nov 2026) declared on the day its focus completed with the
        // West hold undrawn, and France took four states while no attack executed.
        if action == "declare_war" {
            if let Some(op) = super::plan_operation::unready(state, plan, &target) {
                super::plan_operation::mark_due(state, &op, now);
                let first = *state.done.entry(format!("declwait:{key}")).or_insert(now);
                if now - first < OP_DECLARE_WAIT {
                    waits.push(format!(
                        "{label}: operation {op} still preparing ({} of {} days waited)",
                        (now - first) / DAY,
                        OP_DECLARE_WAIT / DAY
                    ));
                    continue;
                }
            }
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
    if declarations_only {
        // checked again in a quarter of a game day (the predicates are reads)
        state.skip(now, "mdecl", "x", DAY / 4);
    } else if !waits.is_empty() {
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

/// Game days before the war day the war-goal watch starts, beyond the justification itself.
const GOAL_LEAD_DAYS: i64 = 10;
/// Days a justification takes when the plan names none (`justify_days`).
const JUSTIFY_DAYS: i64 = 30;

/// The war day a declaration row plans: its `war_day`, else the date atom of its `when`
/// ("date >= 1939.9.1", also inside `all`).
fn planned_day(item: &Value) -> Option<GameHour> {
    if let Some(d) = item["war_day"].as_str().and_then(super::date_hour) {
        return Some(d);
    }
    fn find(v: &Value) -> Option<GameHour> {
        match v {
            Value::String(s) => {
                let s = s.trim();
                let rest = s.strip_prefix("date")?.trim_start();
                let rest = rest.strip_prefix(">=").or_else(|| rest.strip_prefix('>'))?;
                super::date_hour(rest.trim())
            }
            Value::Array(rows) => rows.iter().find_map(find),
            Value::Object(m) => m.get("all").and_then(find),
            _ => None,
        }
    }
    find(&item["when"])
}

/// `when` without its focus atoms (a `focus_completed:` atom holds).
fn without_focus_atoms(when: &Value) -> Value {
    match when {
        Value::String(s) if s.trim().starts_with("focus_completed:") => Value::Null,
        Value::Object(m) if m.contains_key("all") => {
            let kept: Vec<Value> = m["all"]
                .as_array()
                .into_iter()
                .flatten()
                .map(without_focus_atoms)
                .filter(|v| !v.is_null())
                .collect();
            if kept.is_empty() {
                Value::Null
            } else {
                json!({"all": kept})
            }
        }
        other => other.clone(),
    }
}

/// THE WAR-GOAL WATCH (read every two game days while the declaration is pending). A goal has a
/// kind and a life: `expire_days` (the type's `expire` in common/wargoals) from the day it was
/// first seen. (a) The declaration must fall inside a goal's life. (b) A goal that lapses before
/// the war day: a week before it lapses the war is declared at once when the operation against
/// the target has prepared (or none is planned), else the run is told loudly. (c) Only without
/// any goal, from (justification + GOAL_LEAD_DAYS) before the war day, one is justified from the
/// plan's justification row. g17j and g17k: Danzig or War's take_state goal (60 days) was made
/// in April-May for a 1 September war, lapsed, and both September wars were lost.
fn war_goal_watch(
    state: &mut State,
    run: &Run<'_>,
    item: &Value,
    key: &str,
    target: &str,
) -> Result<Option<String>> {
    let now = run.now;
    let Some(day) = planned_day(item) else {
        return Ok(None);
    };
    if state.skipped(now, "goalwatch", key) {
        return Ok(None);
    }
    state.skip(now, "goalwatch", key, 2 * DAY);
    let Ok(ds) = run.tools.call("hoi4_diplomacy_state", &json!({})) else {
        return Ok(None);
    };
    let goals: Vec<&Value> = list(&ds, "own_war_goals")
        .iter()
        .filter(|g| {
            g["target"]["tag"].as_str() == Some(target) || g["target"].as_str() == Some(target)
        })
        .collect();
    let justifies = justify_items(run.plan, target);
    if !goals.is_empty() {
        state.done.insert(format!("dipgoal:{key}"), now);
        // a goal from a focus or event stands for the plan's own justification row
        for j in &justifies {
            state
                .done
                .entry(dip_key(j, "justify_war_goal", target))
                .or_insert(now);
        }
        // the longest life among the goals (None: one never lapses)
        let mut end: Option<GameHour> = Some(GameHour::MIN);
        for g in &goals {
            let kind = text(&g["type"]).to_string();
            let seen = *state
                .done
                .entry(format!("goalseen:{target}:{kind}"))
                .or_insert(now);
            match g["expire_days"].as_i64() {
                Some(days) => end = end.map(|e| e.max(seen + days * DAY)),
                None => end = None,
            }
        }
        let Some(end) = end else {
            return Ok(None);
        };
        if day < end - DAY || now < end - 7 * DAY {
            return Ok(None);
        }
        // the war day lies beyond the goal's life and it lapses within a week
        if super::plan_operation::prepared_against(state, run.plan, target) {
            if state.done.contains_key(&format!("dipearly:{key}")) {
                return Ok(None);
            }
            state.done.insert(format!("dipearly:{key}"), now);
            return Ok(Some(format!(
                "war-goal watch: the war goal on {target} lapses in {} days, {} days before the war day: the operation is prepared, the war is declared now",
                (end - now) / DAY,
                (day - end) / DAY
            )));
        }
        wake(
            state,
            run.plan,
            now,
            29,
            &format!("war_goal_lapses:{key}"),
            format!(
                "the war goal on {target} lapses in {} days, {} days before the planned war day, and the operation against it has not prepared: declare earlier or justify a new goal",
                (end - now) / DAY,
                (day - end) / DAY
            ),
            json!({"target": target, "lapses_hour": end, "war_day_hour": day, "item": item}),
        );
        return Ok(None);
    }
    state.done.remove(&format!("dipgoal:{key}"));
    state.done.remove(&format!("dipearly:{key}"));
    let days = justifies
        .iter()
        .find_map(|j| j["justify_days"].as_i64())
        .or(item["justify_days"].as_i64())
        .unwrap_or(JUSTIFY_DAYS);
    if now < day - (days + GOAL_LEAD_DAYS) * DAY {
        return Ok(None);
    }
    let Some(j) = justifies
        .iter()
        .find(|j| !text(&j["war_goal"]).trim().is_empty() && !strings(j, "states").is_empty())
    else {
        note(
            state,
            "war_goal_watch",
            format!(
                "no war goal on {target} {} days before the war day and no justification row with war_goal and states to make one",
                (day - now) / DAY
            ),
        );
        return Ok(None);
    };
    let (ok, why) = state.act(
        run.tools,
        run.lease,
        "hoi4_diplomacy_act",
        json!({"action": "justify_war_goal", "country": target,
               "war_goal": text(&j["war_goal"]).trim(), "states": strings(j, "states")}),
        "dp",
    )?;
    if ok || code(&why) == "nothing_to_change" {
        // the plan's justification row counts as done from now (the declaration waits on it)
        state
            .done
            .insert(dip_key(j, "justify_war_goal", target), now);
    }
    Ok(Some(format!(
        "war-goal watch: no war goal on {target} {} days before the war day: justify_war_goal: {ok} {why}",
        (day - now) / DAY
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
    let read_fresh = state
        .done
        .get(&ready_key)
        .is_some_and(|t| now >= *t && now - t < DECLARE_READ_FRESH);
    match if read_fresh {
        Truth::True
    } else {
        declare_ready(run, target)
    } {
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
        // a declaration the game simply did not take (NotApplied: no dialog, nothing refused)
        // is tried again within the day: a war declared with others is one opening blow
        // (w47r1, 23 Nov 2026: Luxembourg's declaration came back NotApplied beside Holland's
        // and Belgium's and waited three days while the armour stood on its border); a typed
        // refusal keeps the three days
        let wait = if why.starts_with("NotApplied") || why.starts_with("Diverged") {
            DAY / 4
        } else {
            3 * DAY
        };
        state.skip(now, "dip", key, wait);
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

/// The country name hoi4_diplomacy_countries gives `tag` (lower case), None when unknown.
fn country_name(tools: &dyn Tools, tag: &str) -> Option<String> {
    tools
        .call(
            "hoi4_diplomacy_countries",
            &json!({"query": tag, "limit": 10}),
        )
        .ok()
        .and_then(|r| {
            list(&r, "countries")
                .iter()
                .find(|c| c["tag"].as_str() == Some(tag))
                .map(|c| text(&c["name"]).to_lowercase())
        })
        .filter(|n| !n.is_empty())
}

/// Whether a proposal of hoi4_diplomacy_incoming comes from `tag`: the pop-up carries no sender
/// tag, so its title or description must name the tag or the country's name.
fn names_sender(proposal: &Value, tag: &str, name: Option<&str>) -> bool {
    let words = format!(
        "{} {}",
        text(&proposal["title"]),
        text(&proposal["description"])
    );
    words.contains(tag) || name.is_some_and(|n| words.to_lowercase().contains(n))
}

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
    let name = country_name(tools, join);
    offers
        .iter()
        .find(|p| names_sender(p, join, name.as_deref()))
        .and_then(|p| p["index"].as_u64())
}

/// The members of our faction a call to arms may go to now: in the faction, not the player,
/// not already in our war, and allowed by `faction.call_allies` ("all" or a list of tags).
fn call_candidates(cfg: &Value, diplomacy: &Value) -> Vec<String> {
    let allowed: Option<Vec<String>> = match cfg {
        Value::String(s) if s.eq_ignore_ascii_case("all") => None,
        Value::Array(_) => Some(
            cfg.as_array()
                .into_iter()
                .flatten()
                .map(|t| text(t).trim().to_uppercase())
                .collect(),
        ),
        _ => return Vec::new(),
    };
    if diplomacy["wars"]["at_war"] != json!(true) {
        return Vec::new();
    }
    let own = text(&diplomacy["player"]["tag"]).to_uppercase();
    let fighting: Vec<String> = diplomacy["wars"]["allies_at_war"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|a| text(&a["tag"]).to_uppercase())
        .collect();
    diplomacy["faction"]["faction"]["members"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|m| text(&m["tag"]).to_uppercase())
        .filter(|t| !t.is_empty() && *t != own && !fighting.contains(t))
        .filter(|t| allowed.as_ref().is_none_or(|list| list.contains(t)))
        .collect()
}

/// `faction.call_allies` ("all" | [TAG..] | "none"): while we are at war, a faction member not
/// in the war is called to arms (hoi4_diplomacy_act call_to_arms; the ally decides), one call a
/// tick, each member again after 30 game days while it stays out. Absent: no call is sent and
/// the members left out are noted once a week, so leaving allies out is a written choice.
fn call_allies_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    let cfg = &plan["faction"]["call_allies"];
    if state.skipped(now, "mcall", "x") {
        return Ok(None);
    }
    let Ok(diplomacy) = tools.call("hoi4_diplomacy_state", &json!({})) else {
        state.skip(now, "mcall", "x", DAY);
        return Ok(None);
    };
    if cfg.is_null() {
        let out = call_candidates(&json!("all"), &diplomacy);
        if !out.is_empty() {
            note(
                state,
                "faction",
                format!(
                    "faction members not in our war: {} (faction.call_allies not set: no call sent)",
                    out.join(", ")
                ),
            );
        }
        state.skip(now, "mcall", "x", 7 * DAY);
        return Ok(None);
    }
    for tag in call_candidates(cfg, &diplomacy) {
        if state.skipped(now, "call", &tag) {
            continue;
        }
        let args = json!({"action": "call_to_arms", "country": tag});
        let (ok, why) = state.act(tools, run.lease, "hoi4_diplomacy_act", args, "ca")?;
        state.skip(now, "call", &tag, 30 * DAY);
        return Ok(Some(format!("call_to_arms {tag}: {ok} {why}")));
    }
    state.skip(now, "mcall", "x", 3 * DAY);
    Ok(None)
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
// Incoming proposals: alliance, guarantee, call to arms, lend-lease and manpower requests.

/// What a proposal's rule says: the answer to send, or why it waits / goes to the player.
enum Answer {
    Send(&'static str),
    Player(String),
    Wait(String),
}

/// The first `incoming_proposals` rule that matches a proposal's class and sender and whose
/// `when` holds decides it; a rule whose `when` is unknown stops the walk (a later rule must
/// not answer what this one may still claim); no rule leaves the proposal to the player.
/// The standing answers that follow the plan's `incoming_proposals`.
fn default_incoming(plan: &Value) -> Vec<Value> {
    // a plan that names a faction to join leaves invitations to the faction step and the player
    let joins = !text(&plan["faction"]["join"]).trim().is_empty();
    let refuse = [
        if joins { "" } else { "faction_invitation" },
        "military_access",
        "lend_lease_request",
        "manpower_request",
        "non_aggression_pact",
        "guarantee",
    ];
    let mut out: Vec<Value> = refuse
        .iter()
        .filter(|c| !c.is_empty())
        .map(|c| json!({"class": c, "answer": "refuse", "why": "standing answer: the plan did not ask for it"}))
        .collect();
    // A country asking into our faction: a decision either way, never silence. A planned war
    // target is refused (membership would block the war); anyone else is accepted, since its
    // divisions and factories join our wars (a plan that wants otherwise writes a rule first).
    let mut targets: Vec<String> = plan["diplomacy"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|i| {
                    matches!(
                        dip_action(text(&i["action"])).as_deref(),
                        Some("justify_war_goal" | "declare_war")
                    )
                })
                .map(dip_target)
                .collect()
        })
        .unwrap_or_default();
    targets.extend(
        strings(&plan["war"], "enemies")
            .into_iter()
            .map(|t| t.to_uppercase()),
    );
    targets.retain(|t| !t.is_empty());
    targets.sort();
    targets.dedup();
    if !targets.is_empty() {
        out.push(
            json!({"class": "faction_join_request", "from": targets, "answer": "refuse",
            "why": "standing answer: a planned war target cannot be a member"}),
        );
    }
    out.push(json!({"class": "faction_join_request", "answer": "accept",
        "why": "standing answer: a member that asks brings its divisions and factories into our wars"}));
    out.push(json!({"class": "join_war_offer", "answer": "accept", "why": "standing answer: help in our war"}));
    out.push(json!({"class": "call_to_arms", "answer": "accept", "why": "standing answer: a faction's call to arms"}));
    out.push(json!({"class": "any", "answer": "player", "why": "standing answer: peace and unclassed proposals are the player's"}));
    out
}

fn incoming_answer(
    state: &mut State,
    run: &Run<'_>,
    rules: &[Value],
    proposal: &Value,
    names: &mut HashMap<String, Option<String>>,
) -> Answer {
    let class = text(&proposal["class"]);
    for (i, rule) in rules.iter().enumerate() {
        let want = text(&rule["class"]);
        if !(want.is_empty() || want == "any" || want == class) {
            continue;
        }
        let from = strings(rule, "from");
        if !from.is_empty() {
            let sent_by = from.iter().any(|tag| {
                let name = names
                    .entry(tag.clone())
                    .or_insert_with(|| country_name(run.tools, tag))
                    .clone();
                names_sender(proposal, tag, name.as_deref())
            });
            if !sent_by {
                continue;
            }
        }
        let label = item_id(rule).unwrap_or_else(|| format!("{i}"));
        match when_holds(state, run.tools, &rule["when"], run.now) {
            Truth::False => continue,
            Truth::Unknown => return Answer::Wait(format!("rule {label}: when unknown")),
            Truth::True => {}
        }
        return match text(&rule["answer"]) {
            "accept" => Answer::Send("accept"),
            "refuse" => Answer::Send("refuse"),
            _ => Answer::Player(format!("rule {label} leaves it to the player")),
        };
    }
    Answer::Player("no incoming_proposals rule matches".into())
}

/// `incoming_proposals` (once a game day while nothing is open, every tick while one is): the
/// open diplomatic pop-ups of hoi4_diplomacy_incoming answered by the plan's rules through
/// hoi4_diplomacy_incoming {answer, index, which}. A faction invitation the `faction` key still
/// wants to join is left to the faction step. A proposal no rule answers, or one a rule gives to
/// the player, wakes the player once per proposal.
fn incoming_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    // The plan's rules first; then the standing answers, so no proposal waits unanswered:
    // invitations, access, lend-lease and manpower requests and pacts the plan did not ask for
    // are refused (they bind the plan's own wars and stock), a faction's call to arms is
    // accepted (refusing it costs the membership), and peace offers stay the player's.
    let mut rules: Vec<Value> = list(plan, "incoming_proposals").to_vec();
    rules.extend(default_incoming(plan));
    let rules = rules.as_slice();
    if state.skipped(now, "minc", "x") {
        return Ok(None);
    }
    let Ok(read) = tools.call("hoi4_diplomacy_incoming", &json!({})) else {
        note(state, "incoming", "hoi4_diplomacy_incoming unreadable");
        state.skip(now, "minc", "x", DAY);
        return Ok(None);
    };
    let proposals = list(&read, "proposals");
    if proposals.is_empty() {
        note(state, "incoming", "no open proposal");
        state.skip(now, "minc", "x", DAY);
        return Ok(None);
    }
    let join = text(&plan["faction"]["join"]).trim().to_uppercase();
    let joining = !join.is_empty() && !state.done.contains_key("faction:join");
    let mut names: HashMap<String, Option<String>> = HashMap::new();
    let mut lines: Vec<String> = Vec::new();
    for proposal in proposals {
        let (class, title) = (text(&proposal["class"]), text(&proposal["title"]));
        let key = format!("{class}:{title}");
        if state.skipped(now, "inc", &key) {
            lines.push(format!("{title}: waiting after a refusal"));
            continue;
        }
        if class == "faction_invitation" && joining {
            let wanted = join == "ANY" || {
                let name = names
                    .entry(join.clone())
                    .or_insert_with(|| country_name(tools, &join))
                    .clone();
                names_sender(proposal, &join, name.as_deref())
            };
            if wanted {
                lines.push(format!("{title}: the faction step joins"));
                continue;
            }
        }
        match incoming_answer(state, run, rules, proposal, &mut names) {
            Answer::Wait(why) => lines.push(format!("{title}: {why}")),
            Answer::Player(why) => {
                lines.push(format!("{title}: player ({why})"));
                wake(
                    state,
                    plan,
                    now,
                    22,
                    &format!("incoming:{key}"),
                    format!("an incoming {class} proposal is open ({title}): {why}"),
                    json!({"proposal": proposal}),
                );
            }
            Answer::Send(answer) => {
                let offered = if answer == "accept" {
                    "can_accept"
                } else {
                    "can_refuse"
                };
                if proposal[offered] == json!(false) {
                    lines.push(format!("{title}: the game does not offer {answer}"));
                    wake(
                        state,
                        plan,
                        now,
                        22,
                        &format!("incoming:{key}"),
                        format!("the plan answers {class} ({title}) with {answer}, which the pop-up does not offer"),
                        json!({"proposal": proposal}),
                    );
                    continue;
                }
                let mut args = json!({"answer": answer, "which": title});
                if let Some(index) = proposal["index"].as_u64() {
                    args["index"] = json!(index);
                }
                let (ok, why) =
                    state.act(tools, run.lease, "hoi4_diplomacy_incoming", args, "in")?;
                if !ok {
                    state.skip(now, "inc", &key, DAY);
                }
                note(
                    state,
                    "incoming",
                    format!("{class} {title}: {answer}: {ok}"),
                );
                return Ok(Some(format!(
                    "incoming {class} '{title}' {answer}: {ok} {why}"
                )));
            }
        }
    }
    note(state, "incoming", lines.join("; "));
    // the open proposals stay on screen: look again in a few game hours, not a day
    state.skip(now, "minc", "x", 6);
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

/// The skip after a refused MIO order. `kind` keeps the design-team, policy and trait orders of one
/// organisation apart: a refused policy no longer holds its traits (sk2, 10 Nov: 33 refused policies
/// and not one trait queued).
fn mio_refused(state: &mut State, now: GameHour, kind: &str, org: &str, why: &str) {
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
    state.skip(now, kind, org, days * DAY);
}

/// The design companies in the old chain order: the plan's organisations, lines, traits.
#[cfg_attr(not(test), allow(dead_code))]
fn mio_step(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    if let Some(done) = mio_plan(state, run)? {
        return Ok(Some(done));
    }
    if let Some(done) = mio_lines(state, run)? {
        return Ok(Some(done));
    }
    mio_auto_traits(state, run)
}

/// The plan's `mio` entries: design team flags, policies, listed traits in order.
fn mio_plan(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
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
        .filter(|_| !state.done.contains_key(&team_key) && !state.skipped(now, "miodt", &org));
        let policy = text(&entry["policy"]).trim().to_string();
        let policy_key = format!("miopol:{org}:{policy}");
        let want_policy = !policy.is_empty()
            && !state.done.contains_key(&policy_key)
            && !state.skipped(now, "miopol", &org);
        let mut next: Option<u64> = None;
        // with manage.bulk mio_queue_many: every listed trait still to queue, in ONE visit
        let mut rest: Vec<u64> = Vec::new();
        let mut bad: Option<String> = None;
        for t in list(entry, "traits") {
            match trait_index(t) {
                Some(i) if state.done.contains_key(&format!("mio:{org}:t{i}")) => {}
                Some(i) if next.is_some() => {
                    if rest.len() < 16 {
                        rest.push(i);
                    }
                }
                Some(i) => {
                    next = Some(i);
                    rest.push(i);
                    if !state.bulk_on("mio_queue_many") {
                        break;
                    }
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
        let next = next.filter(|_| !state.skipped(now, "mio", &org));
        if team.is_none() && !want_policy && next.is_none() {
            if state.skipped(now, "mio", &org)
                || state.skipped(now, "miopol", &org)
                || state.skipped(now, "miodt", &org)
            {
                waits.push(format!("{org}: waiting"));
            }
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
        if team.is_some() {
            // The politics view's organisation slots only browse the rosters (pp4, 17 Nov 2026:
            // the row click assigned nothing): the organisation reaches production through
            // `line` (mio_lines below), so the design-team flag is settled without input.
            state.done.insert(team_key, now);
        }
        if want_policy {
            args["policy"] = json!(policy);
            let (ok, why) = state.act(tools, run.lease, "hoi4_mio_policy", args, "mp")?;
            if ok {
                state.done.insert(policy_key, now);
            } else {
                mio_refused(state, now, "miopol", &org, &why);
            }
            return Ok(Some(format!("mio {org} policy {policy}: {ok} {why}")));
        }
        if next.is_some() && rest.len() > 1 {
            args["trait_indices"] = json!(rest);
            let (ok, why) = state.act(tools, run.lease, "hoi4_mio_trait", args, "mi")?;
            if ok {
                for i in &rest {
                    state.done.insert(format!("mio:{org}:t{i}"), now);
                }
            } else {
                mio_refused(state, now, "mio", &org, &why);
            }
            return Ok(Some(format!("mio {org} traits {rest:?} in one visit: {ok} {why}")));
        }
        if let Some(index) = next {
            args["trait_index"] = json!(index);
            let (ok, why) = state.act(tools, run.lease, "hoi4_mio_trait", args, "mi")?;
            if ok || code(&why) == "trait_already_queued" {
                state.done.insert(format!("mio:{org}:t{index}"), now);
            } else {
                mio_refused(state, now, "mio", &org, &why);
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

/// The organisation the plan names for a line's equipment family (`mio[].line`).
fn planned_line_org(plan: &Value, equipment: &str) -> Option<String> {
    let family = super::plan_manage::family(equipment);
    list(plan, "mio").iter().find_map(|e| {
        let org = e["organisation"]
            .as_str()
            .or(e["organization"].as_str())
            .unwrap_or("")
            .trim();
        let line = text(&e["line"]);
        (!org.is_empty() && !line.is_empty() && super::plan_manage::family(line) == family)
            .then(|| org.to_string())
    })
}

/// The organisation of the own country's catalogue (hoi4_mios, allowed for the tag or unknown)
/// whose equipment list names this line's equipment or its family, most traits first; None when
/// only a category names it (the roster's first row is taken then).
fn catalogue_line_org(tools: &dyn Tools, equipment: &str) -> Option<String> {
    let read = tools.call("hoi4_mios", &json!({})).ok()?;
    let family = super::plan_manage::family(equipment);
    list(&read["organisations"], "entries")
        .iter()
        .filter(|o| o["allowed_for_tag"] != json!(false))
        .filter(|o| {
            strings(o, "equipment")
                .iter()
                .any(|e| e == equipment || e == family)
        })
        .max_by_key(|o| {
            (
                o["allowed_for_tag"] == json!(true),
                o["traits"].as_u64().unwrap_or(0),
            )
        })
        .and_then(|o| o["key"].as_str().map(String::from))
}

/// Rule (manage.off "mio_lines"): every own production line that can take an organisation has
/// one. The organisation is the plan's for that equipment (`mio[].line`), else the first the
/// line's roster offers (a line costs no political power and task capacity is off in 1.19.4.1, so
/// an organisation on a line is pure gain: its traits apply and it earns funds toward trait
/// points). Each line is tried once (hoi4_mio_line answers already_attached for a line that has
/// one); a line no organisation makes is settled; a planned organisation the roster does not
/// offer falls back to the first row. New lines are looked for weekly.
fn mio_lines(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    if strings(&plan["manage"], "off")
        .iter()
        .any(|r| r == "mio_lines")
        || state.skipped(now, "miol", "x")
    {
        return Ok(None);
    }
    let Ok(read) = tools.call("hoi4_production_lines", &json!({"limit": 128})) else {
        state.skip(now, "miol", "x", DAY);
        return Ok(None);
    };
    for row in list(&read, "lines") {
        let handle = text(&row["line_handle"]).to_string();
        let equipment = text(&row["equipment_id"]).to_string();
        if handle.is_empty() || equipment.is_empty() || !row["entry_index"].is_u64() {
            continue;
        }
        let key = format!("miol:{handle}");
        if state.done.contains_key(&key) || state.skipped(now, "miol", &handle) {
            continue;
        }
        // the plan's organisation for the equipment, else the catalogue's that names the
        // equipment itself (its key is then known, so the trait rule can queue its traits)
        let org = planned_line_org(plan, &equipment)
            .or_else(|| catalogue_line_org(tools, &equipment))
            .filter(|_| !state.done.contains_key(&format!("miolany:{handle}")));
        let mut args = json!({"line": row["entry_index"], "equipment": equipment});
        if let Some(o) = &org {
            args["organization"] = json!(o);
        }
        let (ok, why) = state.act(tools, run.lease, "hoi4_mio_line", args, "ml")?;
        if ok {
            state.done.insert(key, now);
            match &org {
                // the organisation on the line: its traits are queued by the trait rule
                Some(o) => {
                    state.done.insert(format!("mioorg:{o}"), now);
                }
                None => note(
                    state,
                    "mio_traits",
                    format!(
                        "{equipment}: the line took the roster's first organisation, whose key no \
                         read gives: its traits are queued only when the plan's mio names it"
                    ),
                ),
            }
        } else {
            match code(&why) {
                "line_takes_no_organization" => {
                    state.done.insert(key, now);
                }
                "roster_unreadable" | "row_not_visible" => {
                    // The tool cannot pick from the line's roster: the line is settled and named
                    // once for the player (the alert stays the game's own reminder).
                    state.done.insert(key, now);
                    wake(
                        state,
                        plan,
                        now,
                        31,
                        &format!("mio_line:{handle}"),
                        format!(
                            "production line {equipment} has no organisation and the tools could \
                             not attach one: attach it in the game (the line's organisation \
                             button){}",
                            org.as_deref()
                                .map(|o| format!("; the plan names {o}"))
                                .unwrap_or_default()
                        ),
                        json!({"line": row["entry_index"], "equipment": equipment,
                               "refusal": why}),
                    );
                }
                "organization_not_offered"
                | "organization_not_allowed"
                | "organization_unknown"
                    if org.is_some() =>
                {
                    note(
                        state,
                        "mio",
                        format!("{equipment}: {why}; the first offered row next"),
                    );
                    state.done.insert(format!("miolany:{handle}"), now);
                    state.skip(now, "miol", &handle, DAY);
                }
                // Any other failure (the roster did not open, an input readback): one more try
                // two weeks later, then the line is settled and named once for the player, so a
                // line the tool cannot serve is not retried all game (g17e: the convoy line's
                // roster never opened, 19 tries in two game years).
                _ if state.done.contains_key(&format!("miolfail:{handle}")) => {
                    state.done.insert(key, now);
                    wake(
                        state,
                        plan,
                        now,
                        31,
                        &format!("mio_line:{handle}"),
                        format!(
                            "production line {equipment} has no organisation and two tries to \
                             attach one failed: attach it in the game if an organisation makes \
                             this equipment"
                        ),
                        json!({"line": row["entry_index"], "equipment": equipment,
                               "refusal": why}),
                    );
                }
                _ => {
                    state.done.insert(format!("miolfail:{handle}"), now);
                    state.skip(now, "miol", &handle, 14 * DAY);
                }
            }
        }
        return Ok(Some(format!(
            "mio line {equipment} -> {}: {ok} {why}",
            org.as_deref().unwrap_or("first offered")
        )));
    }
    state.skip(now, "miol", "x", 7 * DAY);
    Ok(None)
}

/// Rule (manage.off "mio_traits"): an organisation of the plan whose `traits` list is empty (or
/// "auto") always has one trait queued, so a trait point is spent the day it arrives (the game
/// unlocks queued traits in order as points come). Monthly per organisation: the trait tree is
/// listed (hoi4_mio_trait list:true) and, when no entry carries a queue number, the first
/// enabled entry (else the first visible one) is queued.
fn mio_auto_traits(state: &mut State, run: &Run<'_>) -> Result<Option<String>> {
    let (tools, plan, now) = (run.tools, run.plan, run.now);
    if strings(&plan["manage"], "off")
        .iter()
        .any(|r| r == "mio_traits")
    {
        return Ok(None);
    }
    // the plan's organisations, then every organisation the executor put on a line that the
    // plan does not list (each keeps one trait queued, like an "auto" entry)
    let mut entries: Vec<Value> = list(plan, "mio").to_vec();
    let listed: HashSet<String> = entries
        .iter()
        .filter_map(|e| e["organisation"].as_str().or(e["organization"].as_str()))
        .map(|o| o.trim().to_string())
        .collect();
    let mut attached: Vec<String> = state
        .done
        .keys()
        .filter_map(|k| k.strip_prefix("mioorg:"))
        .filter(|o| !listed.contains(*o))
        .map(String::from)
        .collect();
    attached.sort();
    entries.extend(attached.into_iter().map(|o| json!({"organisation": o})));
    for entry in &entries {
        let org = entry["organisation"]
            .as_str()
            .or(entry["organization"].as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        let auto = match &entry["traits"] {
            Value::Array(a) => a.is_empty(),
            Value::String(s) => s == "auto",
            Value::Null => true,
            _ => false,
        };
        if org.is_empty() || !auto || state.skipped(now, "mioauto", &org) {
            continue;
        }
        if gate(state, tools, &entry["when"], now).is_some() {
            continue;
        }
        // monthly; every 3 game days while the game's mio_traits alert is open (an organisation
        // with a trait point and nothing queued)
        let open = state.manage.alert_since.contains_key("mio_traits");
        state.skip(now, "mioauto", &org, if open { 3 * DAY } else { 30 * DAY });
        // with manage.bulk mio_queue_many: one visit lists the tree and keeps two enabled traits
        // queued (the listing visit and the queue visit were two GUI visits of ~8 s each)
        if state.bulk_on("mio_queue_many") {
            let (ok, why) = state.act(
                tools,
                run.lease,
                "hoi4_mio_trait",
                json!({"organization": org, "auto": 2}),
                "mq",
            )?;
            if !ok {
                mio_refused(state, now, "mio", &org, &why);
            }
            return Ok(Some(format!("mio {org} traits (auto, one visit): {ok} {why}")));
        }
        let listing = tools.call(
            "hoi4_mio_trait",
            &json!({"organization": org, "list": true,
                    "request_id": format!("plan-mtl-{}-{}", org.len(), now),
                    "lease_token": run.lease}),
        );
        let Ok(listing) = listing else {
            return Ok(Some(format!("mio {org} trait list: failed")));
        };
        let mut entries = list(&listing["observed"], "listed").to_vec();
        // the game's mio_traits alert open two weeks with a trait queued: the queued one is not
        // being unlocked (g17i, 18 Nov 2026: Mauser kept one queued trait under the alert for
        // 344 days), so another trait is queued beside it
        let stuck = state
            .manage
            .alert_since
            .get("mio_traits")
            .is_some_and(|since| now - *since >= 14 * DAY);
        if stuck {
            for e in entries.iter_mut() {
                if !text(&e["queue"]).trim().is_empty() {
                    e["queue"] = json!("");
                    e["owned"] = json!(true);
                }
            }
        }
        let tried = |i: u64| state.skipped(now, "miotrait", &format!("{org}#{i}"));
        let Some(index) = auto_trait_pick(&entries, &tried) else {
            note(
                state,
                "mio_traits",
                format!(
                    "{org}: no trait to queue (one is queued, or none is enabled, visible, not \
                     held and not tried in vain this year)"
                ),
            );
            return Ok(Some(format!(
                "mio {org} traits: one queued already or none open"
            )));
        };
        let (ok, why) = state.act(
            tools,
            run.lease,
            "hoi4_mio_trait",
            json!({"organization": org, "trait_index": index}),
            "mq",
        )?;
        if !ok {
            mio_refused(state, now, "mio", &org, &why);
            if matches!(
                code(&why),
                "action_not_applied" | "trait_owned" | "trait_unavailable"
            ) {
                // that trait did not queue: the next pick takes another one
                state.skip(now, "miotrait", &format!("{org}#{index}"), 365 * DAY);
            }
            if code(&why) == "organization_not_offered" {
                // the mio_traits alert's reason (plan_manage.rs alerts)
                note(
                    state,
                    "mio_traits",
                    format!(
                        "{org}: no trait queued, no organisation slot's list offers it ({why}); \
                         tried again in 30 days"
                    ),
                );
            }
        }
        return Ok(Some(format!("mio {org} trait {index} (auto): {ok} {why}")));
    }
    Ok(None)
}

/// The trait to queue from a listing (`index`, `enabled`, `queue`, `visible`): none while an
/// enabled entry is queued, else the first enabled visible entry. A queued entry the game shows
/// disabled (its prerequisite not met) is never unlocked when a point comes, so it holds the
/// organisation's points idle (g17i, 18 Nov 2026: Mauser read "one queued already" for 344
/// days under the game's mio_traits alert): another, enabled trait is queued behind it.
fn auto_trait_pick(entries: &[Value], tried: &dyn Fn(u64) -> bool) -> Option<u64> {
    if entries.iter().any(|e| {
        !text(&e["queue"]).trim().is_empty()
            && e["enabled"].as_bool().unwrap_or(true)
            && e["owned"] != json!(true)
    }) {
        return None;
    }
    // Only an enabled, visible trait the organisation does not hold and whose click did not
    // already come back unqueued: a disabled or held one never queues (g17c, 17 Nov 2026: trait
    // 0, held since the first month, was clicked every month).
    entries
        .iter()
        .filter(|e| e["visible"].as_bool().unwrap_or(false))
        .filter(|e| e["enabled"].as_bool().unwrap_or(false))
        .filter(|e| e["owned"] != json!(true))
        .filter_map(|e| e["index"].as_u64())
        .find(|i| !tried(*i))
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
    let has_ops = !list(cfg, "networks").is_empty() || !list(cfg, "operations").is_empty();
    if !want_agency && upgrades.is_empty() && operatives.is_empty() && !has_ops {
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
                        // the game would only queue it: waits until the factories are free
                        "upgrade_needs_factories" => {
                            note(state, "intelligence", format!("{upgrade}: {why}"));
                            state.skip(now, "intel", &key, 10 * DAY);
                        }
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
        // An operative back from an operation (idle, on none) gets its listed mission again.
        if let Some(read) = intel_read(state, tools, now) {
            for (i, live) in list(&read["operatives"], "list").iter().enumerate() {
                let key = format!("intel:opm:{i}");
                if live["idle"] == json!(true)
                    && live["operation"].is_null()
                    && state.done.get(&key).is_some_and(|at| now - *at > 3 * DAY)
                {
                    state.done.remove(&key);
                }
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
            // A network starts only in a state beside land the player controls (the game:
            // "Not a suitable state to start the mission", live 16 Nov 2026): a state further in
            // is replaced by the nearest one that is, and with none the mission waits.
            if mission == "build_intel_network" {
                if let Some(id) = op["state_id"].as_i64() {
                    match network_state(tools, id) {
                        Some(Some(s)) if s != id => {
                            note(
                                state,
                                "intelligence",
                                format!("operative {i}: state {id} is not beside own land; the network starts in {s}"),
                            );
                            args["state_id"] = json!(s);
                        }
                        Some(None) => {
                            note(
                                state,
                                "intelligence",
                                format!("operative {i}: no state of {id}'s country is beside own land yet; the network waits"),
                            );
                            state.skip(now, "intel", &key, 10 * DAY);
                            continue;
                        }
                        _ => {}
                    }
                }
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
        // 4. networks, then operations
        if let Some(done) = intel_networks(state, run, cfg, &mut waits)? {
            return Ok(Some(done));
        }
        if let Some(done) = intel_operations(state, run, cfg, &mut waits)? {
            return Ok(Some(done));
        }
    }
    if !waits.is_empty() {
        note(state, "intelligence", waits.join("; "));
    }
    state.skip(now, "mintel", "x", DAY);
    Ok(None)
}

/// Where an intel network for `wanted` can start: `wanted` itself when one of its provinces
/// touches a state the player controls, else the nearest state of the same controller that does
/// (provinces apart over the map), `Some(None)` when none does; `None` when the map cannot be read
/// (the order goes as written).
fn network_state(tools: &dyn Tools, wanted: i64) -> Option<Option<i64>> {
    let me = tools.call("hoi4_overview", &json!({})).ok()?["player"]
        .as_str()?
        .to_string();
    let row = tools
        .call("hoi4_states", &json!({"query": wanted.to_string()}))
        .ok()?;
    let tag = list(&row, "states")
        .iter()
        .find(|r| r["state_id"].as_i64() == Some(wanted))
        .map(|r| text(&r["controller"]).to_string())?;
    if tag.is_empty() || tag == me {
        return None;
    }
    let mut theirs: Vec<i64> = Vec::new();
    let mut mine: HashSet<i64> = HashSet::new();
    for query in [tag.as_str(), me.as_str()] {
        let read = tools.call("hoi4_states", &json!({"query": query})).ok()?;
        for r in list(&read, "states") {
            let Some(id) = r["state_id"].as_i64() else {
                continue;
            };
            let controller = text(&r["controller"]);
            if controller == me {
                mine.insert(id);
            } else if controller == tag && !theirs.contains(&id) {
                theirs.push(id);
            }
        }
    }
    let mut states: Vec<i64> = theirs.iter().chain(mine.iter()).copied().collect();
    states.sort_unstable();
    states.dedup();
    let graph = tools
        .call("hoi4_province_graph", &json!({"states": states}))
        .ok()?;
    let mut state_of: HashMap<i64, i64> = HashMap::new();
    let mut next: HashMap<i64, Vec<i64>> = HashMap::new();
    for r in list(&graph, "provinces") {
        let (Some(p), Some(st)) = (r["province"].as_i64(), r["state_id"].as_i64()) else {
            continue;
        };
        state_of.insert(p, st);
        next.insert(
            p,
            list(r, "neighbours")
                .iter()
                .filter_map(Value::as_i64)
                .collect(),
        );
    }
    if next.is_empty() {
        return None;
    }
    let suitable = |st: i64| {
        next.iter().any(|(p, ns)| {
            state_of.get(p) == Some(&st)
                && ns
                    .iter()
                    .any(|n| state_of.get(n).is_some_and(|o| mine.contains(o)))
        })
    };
    if suitable(wanted) {
        return Some(Some(wanted));
    }
    // provinces apart from `wanted`, over the mapped land
    let mut dist: HashMap<i64, usize> = HashMap::new();
    let mut ring: Vec<i64> = state_of
        .iter()
        .filter(|(_, st)| **st == wanted)
        .map(|(p, _)| *p)
        .collect();
    for p in &ring {
        dist.insert(*p, 0);
    }
    let mut step = 0;
    while !ring.is_empty() && step < 200 {
        step += 1;
        let mut out = Vec::new();
        for p in ring {
            for n in next.get(&p).map(Vec::as_slice).unwrap_or(&[]) {
                if state_of.contains_key(n) && !dist.contains_key(n) {
                    dist.insert(*n, step);
                    out.push(*n);
                }
            }
        }
        ring = out;
    }
    let best = theirs
        .iter()
        .copied()
        .filter(|st| suitable(*st))
        .filter_map(|st| {
            dist.iter()
                .filter(|(p, _)| state_of.get(p) == Some(&st))
                .map(|(_, d)| *d)
                .min()
                .map(|d| (d, st))
        })
        .min();
    Some(best.map(|(_, st)| st))
}

/// `intelligence.networks`: [{"target", "state_id"?, "operative"?, "until"?}] keeps an operative
/// building a network in each target until its strength (hoi4_intelligence intel_network, the
/// strongest covered state) reaches `until` (default 100). Without an operative already on
/// build_intel_network there, the named one (else the first idle one not on an operation) is sent.
fn intel_networks(
    state: &mut State,
    run: &Run<'_>,
    cfg: &Value,
    waits: &mut Vec<String>,
) -> Result<Option<String>> {
    let (tools, now) = (run.tools, run.now);
    let entries = list(cfg, "networks").to_vec();
    if entries.is_empty() {
        return Ok(None);
    }
    let Some(read) = intel_read(state, tools, now) else {
        return Ok(None);
    };
    for (k, net) in entries.iter().enumerate() {
        let target = text(&net["target"]).trim().to_uppercase();
        let key = format!("intel:net:{k}");
        if target.is_empty() || state.skipped(now, "intel", &key) {
            continue;
        }
        if gate(state, tools, &net["when"], now).is_some() {
            continue;
        }
        let until = net["until"].as_f64().unwrap_or(100.0);
        let strength = list(&read["intel_network"], "networks")
            .iter()
            .find(|n| text(&n["target"]) == target)
            .and_then(|n| n["strength"].as_f64())
            .unwrap_or(0.0);
        if strength >= until {
            continue;
        }
        let ops = list(&read["operatives"], "list");
        let building = ops.iter().any(|o| {
            text(&o["mission"]) == "build_intel_network" && text(&o["mission_target"]) == target
        });
        if building {
            continue;
        }
        let pick = match net["operative"].as_u64() {
            Some(i) => Some(i as usize),
            None => ops
                .iter()
                .position(|o| o["idle"] == json!(true) && o["operation"].is_null()),
        };
        let Some(i) = pick else {
            waits.push(format!("network {target}: no idle operative"));
            continue;
        };
        let mut args = json!({"operative": i, "mission": "build_intel_network"});
        match net["state_id"].as_u64() {
            Some(id) => args["state_id"] = json!(id),
            None => args["country"] = json!(target),
        }
        let (ok, why) = state.act(tools, run.lease, "hoi4_operative_assign", args, "in")?;
        state.affairs.intel = None;
        let days = if ok {
            3
        } else {
            match code(&why) {
                "already_on_mission" => 3,
                "operative_busy" | "operative_unknown" => 7,
                "invalid_arguments" | "target_required" | "unknown_country" => {
                    note(state, "intelligence", format!("network {target}: {why}"));
                    90
                }
                _ => 7,
            }
        };
        state.skip(now, "intel", &key, days * DAY);
        return Ok(Some(format!(
            "intelligence network {target} operative {i}: {ok} {why}"
        )));
    }
    Ok(None)
}

/// `intelligence.operations`: [{"operation","target","when"?,"operatives"?,"auto_commence"?,
/// "runs"?,"repeat_until"?:{"collaboration"}}] in order. An entry waits while its operation runs
/// against the target (hoi4_intelligence operations_running), is judged by hoi4_operations (a
/// failing verdict waits with its reasons) and is started with hoi4_operation_start. It runs
/// `runs` times (default 1), or while the target's collaboration stays below
/// repeat_until.collaboration (at most `runs`, default 5 then).
fn intel_operations(
    state: &mut State,
    run: &Run<'_>,
    cfg: &Value,
    waits: &mut Vec<String>,
) -> Result<Option<String>> {
    let (tools, now) = (run.tools, run.now);
    let entries = list(cfg, "operations").to_vec();
    if entries.is_empty() {
        return Ok(None);
    }
    let Some(read) = intel_read(state, tools, now) else {
        return Ok(None);
    };
    for (k, entry) in entries.iter().enumerate() {
        let operation = text(&entry["operation"]).trim().to_string();
        let target = text(&entry["target"]).trim().to_uppercase();
        let key = format!("intel:opn:{k}");
        if operation.is_empty() || target.is_empty() || state.skipped(now, "intel", &key) {
            continue;
        }
        if let Some(why) = gate(state, tools, &entry["when"], now) {
            waits.push(format!("{operation} {target}: {why}"));
            continue;
        }
        let running = list(&read["operations_running"], "list")
            .iter()
            .any(|o| text(&o["operation"]) == operation && text(&o["target"]) == target);
        if running {
            waits.push(format!("{operation} {target}: running"));
            continue;
        }
        let collab_goal = entry["repeat_until"]["collaboration"].as_f64();
        let runs = entry["runs"]
            .as_u64()
            .unwrap_or(if collab_goal.is_some() { 5 } else { 1 }) as usize;
        let done = (0..runs)
            .take_while(|n| state.done.contains_key(&format!("{key}:{n}")))
            .count();
        if done >= runs {
            continue;
        }
        let judged = tools
            .call(
                "hoi4_operations",
                &json!({"target": target, "operations": [operation]}),
            )
            .ok();
        let row = judged.as_ref().map(|j| &j["targets"][0]);
        if let (Some(goal), Some(have)) =
            (collab_goal, row.and_then(|r| r["collaboration"].as_f64()))
        {
            if have >= goal {
                continue;
            }
        }
        let verdict = row.map(|r| &r["operations"][0]["verdict"]);
        if verdict.is_some_and(|v| v["ready"] == json!("false")) {
            let v = verdict.unwrap();
            let codes: Vec<&str> = list(v, "reasons")
                .iter()
                .map(|r| text(&r["code"]))
                .collect();
            let days = if codes
                .iter()
                .any(|c| matches!(*c, "not_allowed" | "not_a_target"))
            {
                note(
                    state,
                    "intelligence",
                    format!("{operation} {target}: {codes:?} (game rules)"),
                );
                90
            } else if codes.contains(&"unavailable") {
                30
            } else {
                7
            };
            waits.push(format!("{operation} {target}: {}", codes.join(", ")));
            state.skip(now, "intel", &key, days * DAY);
            continue;
        }
        let mut args = json!({"target": target, "operation": operation});
        if let Some(list) = entry["operatives"].as_array() {
            args["operatives"] = json!(list);
        }
        if let Some(b) = entry["auto_commence"].as_bool() {
            args["auto_commence"] = json!(b);
        }
        let (ok, why) = state.act(tools, run.lease, "hoi4_operation_start", args, "iq")?;
        state.affairs.intel = None;
        if ok {
            state.done.insert(format!("{key}:{done}"), now);
            state.skip(now, "intel", &key, 3 * DAY);
        } else {
            let days = match code(&why) {
                "not_enough_operatives" | "operative_busy" | "operation_unavailable" => 7,
                "unknown_operation" | "operation_not_listed" | "display_name_unknown" => {
                    note(
                        state,
                        "intelligence",
                        format!("{operation} {target}: {why}"),
                    );
                    90
                }
                _ => 5,
            };
            state.skip(now, "intel", &key, days * DAY);
        }
        return Ok(Some(format!(
            "intelligence operation {operation} {target} run {}: {ok} {why}",
            done + 1
        )));
    }
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

// Test-only: other modules' tests set an affairs note through this (kept after the code so
// the plan-key reader, which stops at the first test item, reads every step).
#[cfg(test)]
pub(super) fn put_note(state: &mut State, key: &str, line: &str) {
    note(state, key, line);
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
            // a listing (list:true) answers from `replies` when the test gives one
            if args["list"] == json!(true) {
                if let Some(v) = self.replies.borrow().get(name) {
                    return Ok(v.clone());
                }
            }
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
            // like the server: a state query is text (a tag, a name or a state id written out)
            if name == "hoi4_states" && args.get("query").is_some_and(|q| !q.is_string()) {
                return Err(Error::new("invalid_arguments", "query must be a string"));
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
        let plan = json!({"manage": {"off": ["mio_lines"]},

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
        let plan = json!({"manage": {"off": ["mio_lines"]},
            "production": [{"equipment": "medium_tank_chassis_1", "design": "Panzer IV"}]});
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

    fn run<'a>(tools: &'a Fake, plan: &'a Value, now: GameHour) -> Run<'a> {
        Run {
            tools,
            plan,
            lease: "l",
            now,
        }
    }
    fn rifle_lines(efficiency: f64, with_new: bool) -> Value {
        let mut lines = vec![json!({"entry_index": 0, "line_kind": "military",
            "equipment_id": "infantry_equipment_1", "line_handle": "0x10",
            "factory_counters": {"requested_factories_raw_signed": 20},
            "output": {"efficiency": efficiency}})];
        if with_new {
            lines.push(json!({"entry_index": 1, "line_kind": "military",
                "equipment_id": "infantry_equipment_2", "line_handle": "0x11",
                "factory_counters": {"requested_factories_raw_signed": 2}}));
        }
        json!({ "lines": lines })
    }
    fn rifle_designs(building: &[&str]) -> Value {
        let lines: Vec<Value> = building.iter().map(|h| json!({"line_handle": h})).collect();
        json!({"designs": [
            {"design_id": 11, "name": "Rifle I", "hull_or_chassis": "infantry_equipment_1",
                "production_lines": [{"line_handle": "0x10"}]},
            {"design_id": 12, "name": "Rifle II", "hull_or_chassis": "infantry_equipment_2",
                "production_lines": lines},
            {"design_id": 9, "name": "Rifle II old", "hull_or_chassis": "infantry_equipment_2",
                "production_lines": []}]})
    }
    fn rifle_cover(days: Option<f64>, short: f64) -> Value {
        json!({"equipment": [{"archetype": "infantry_equipment", "stockpile": 3000.0,
            "fielded_short": short, "depleting": days.is_some(), "days_of_cover": days,
            "net_per_day": if days.is_some() { -5.0 } else { 4.0 },
            "variants": [{"type": "infantry_equipment_1", "amount": 3000.0}]}]})
    }

    #[test]
    fn a_replace_starts_the_new_tier_beside_a_mature_line_and_switches_it_whole_at_cover() {
        let tools = fake(vec![
            ("hoi4_production_lines", rifle_lines(40.0, false)),
            ("hoi4_designs", rifle_designs(&[])),
            ("hoi4_stockpile", rifle_cover(Some(12.0), 0.0)),
        ]);
        let plan = json!({"manage": {"off": ["mio_lines"]},
            "production": [{"equipment": "infantry_equipment_1",
            "replace": "infantry_equipment_2", "from": "1936.6.1"}]});
        let at = date_hour("1936.6.1").unwrap();
        let mut state = State::default();
        // before `from` nothing happens
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at - DAY).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
        assert!(state.affairs.notes["production_designs"].contains("from"));
        // the mature line is kept; the newest design of the new type starts beside it
        let out = step(&mut state, &tools, &plan, "l", at).unwrap().unwrap();
        assert!(out.contains("separate line (2 factories)"), "{out}");
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_production_add");
        assert_eq!(sent[0].1["equipment"], "infantry_equipment_2");
        assert_eq!(sent[0].1["design"], 12);
        // the old gear covers 12 days: the old line keeps building
        tools.reply("hoi4_production_lines", rifle_lines(40.0, true));
        tools.reply("hoi4_designs", rifle_designs(&["0x11"]));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        let note = &state.affairs.notes["production_tier_guard"];
        assert!(note.contains("12 days of cover < 30"), "{note}");
        // divisions still short of the gear: kept even with a stock that is not falling
        tools.reply("hoi4_stockpile", rifle_cover(None, 50.0));
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + 2 * DAY).unwrap(),
            None
        );
        assert!(state.affairs.notes["production_tier_guard"].contains("still lack 50"));
        // 40 days of cover: the old line switches whole to the new design (it keeps its factories)
        tools.reply("hoi4_stockpile", rifle_cover(Some(40.0), 0.0));
        let out = step(&mut state, &tools, &plan, "l", at + 3 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            out.contains("mature line switched whole: old gear 40 days of cover"),
            "{out}"
        );
        let sent = actions(&tools);
        assert_eq!(sent[1].0, "hoi4_design_set_production");
        assert_eq!(sent[1].1["line"], 0);
        assert_eq!(sent[1].1["design"], 12);
        assert!(state
            .done
            .contains_key("pdes:infantry_equipment_1:infantry_equipment_2"));
        // done once, also after a restart
        let mut state = restarted(&state, at + 4 * DAY);
        assert_eq!(
            step(&mut state, &tools, &plan, "l", at + 4 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
    }

    #[test]
    fn old_gear_cover_needs_full_divisions_and_a_measured_stock() {
        let tools = fake(vec![("hoi4_stockpile", rifle_cover(None, 0.0))]);
        let covers = |t: &Fake| old_gear_covers(t, "infantry_equipment_1", 30.0);
        assert_eq!(covers(&tools).unwrap(), "the stock is not falling");
        // a trend not measured yet is no cover
        tools.reply(
            "hoi4_stockpile",
            json!({"equipment": [{"archetype": "infantry_equipment", "fielded_short": 0.0,
                "depleting": false, "net_per_day": null, "days_of_cover": null}]}),
        );
        assert_eq!(covers(&tools).unwrap_err(), "cover not measured yet");
        tools.reply("hoi4_stockpile", json!({"equipment": []}));
        assert!(covers(&tools).unwrap_err().contains("no stockpile row"));
        tools
            .unreadable
            .borrow_mut()
            .insert("hoi4_stockpile".into());
        assert_eq!(covers(&tools).unwrap_err(), "hoi4_stockpile unreadable");
    }

    #[test]
    fn switch_at_cover_days_false_keeps_the_mature_line_building() {
        let tools = fake(vec![
            ("hoi4_production_lines", rifle_lines(40.0, true)),
            ("hoi4_designs", rifle_designs(&["0x11"])),
            ("hoi4_stockpile", rifle_cover(Some(90.0), 0.0)),
        ]);
        let plan = json!({"manage": {"off": ["mio_lines"]},
            "production": [{"equipment": "infantry_equipment_1",
            "replace": "infantry_equipment_2", "switch_at_cover_days": false}]});
        let mut state = State::default();
        // the new tier already runs on its own line: nothing to add, and the old line stays
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(actions(&tools).is_empty());
        assert!(state.affairs.notes["production_tier_guard"].contains("keeps building"));
        assert!(state
            .done
            .contains_key("pdes:infantry_equipment_1:infantry_equipment_2"));
    }

    #[test]
    fn a_replace_the_game_cannot_switch_moves_the_factories_then_closes_the_old_line() {
        let tools = fake(vec![
            ("hoi4_production_lines", rifle_lines(12.0, false)),
            ("hoi4_designs", rifle_designs(&[])),
        ]);
        tools.refuse("hoi4_design_set_production", "design_wrong_type");
        let plan = json!({"manage": {"off": ["mio_lines"]},
            "production": [{"equipment": "infantry_equipment_1",
            "replace": "infantry_equipment_2"}]});
        let mut state = State::default();
        // a young line (12%) switches at once; the game refuses the type
        step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(actions(&tools)[0].0, "hoi4_design_set_production");
        // its 20 factories go to a new line of the new type
        let out = step(&mut state, &tools, &plan, "l", START + DAY)
            .unwrap()
            .unwrap();
        assert!(out.contains("20 factories to the new line"), "{out}");
        let sent = actions(&tools);
        assert_eq!(sent[1].0, "hoi4_production_add");
        assert_eq!(sent[1].1["factories"], 20);
        assert_eq!(sent[1].1["design"], 12);
        // then the old line closes
        tools.reply("hoi4_production_lines", rifle_lines(12.0, true));
        let out = step(&mut state, &tools, &plan, "l", START + 2 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            out.contains("old line infantry_equipment_1 closed"),
            "{out}"
        );
        let sent = actions(&tools);
        assert_eq!(sent[2].0, "hoi4_production_set");
        assert_eq!(sent[2].1["line"], 0);
        assert_eq!(sent[2].1["factories"], 0);
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 3);
    }

    #[test]
    fn a_replace_waits_for_a_design_of_the_new_type_and_adds_no_line_of_the_old_one() {
        let tools = fake(vec![
            ("hoi4_production_lines", json!({"lines": []})),
            ("hoi4_designs", json!({"designs": []})),
        ]);
        let plan = json!({"production": [{"equipment": "infantry_equipment_1",
            "replace": "infantry_equipment_2", "factories": 10}]});
        let mut state = State::default();
        assert_eq!(step(&mut state, &tools, &plan, "l", START).unwrap(), None);
        assert!(state.affairs.notes["production_designs"]
            .contains("no own design of infantry_equipment_2 yet"));
        // the production step never adds a line for a replace entry
        assert_eq!(
            super::super::production_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert!(actions(&tools).is_empty());
    }

    #[test]
    fn old_designs_are_decommissioned_once_the_new_one_is_on_a_line() {
        let tools = fake(vec![("hoi4_designs", rifle_designs(&[]))]);
        let plan = json!({"designs": [{"id": "r2", "kind": "other", "base": "infantry_equipment_2",
            "name": "Rifle II", "decommission": ["Rifle I", 9]}]});
        let mut state = State::default();
        state.done.insert("design:id:r2".into(), START);
        // made, but no line builds it yet
        assert_eq!(
            decommission_step(&mut state, &run(&tools, &plan, START)).unwrap(),
            None
        );
        assert!(state.affairs.notes["designs_decommission"].contains("no line builds Rifle II"));
        tools.reply("hoi4_designs", rifle_designs(&["0x11"]));
        let out = decommission_step(&mut state, &run(&tools, &plan, START + DAY))
            .unwrap()
            .unwrap();
        assert!(out.starts_with("design Rifle I decommissioned"), "{out}");
        // one per tick; already_obsolete counts as done
        tools.refuse("hoi4_design_obsolete", "already_obsolete");
        decommission_step(&mut state, &run(&tools, &plan, START + 2 * DAY)).unwrap();
        let sent = actions(&tools);
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, "hoi4_design_obsolete");
        assert_eq!(sent[0].1["design"], "Rifle I");
        assert_eq!(sent[0].1["obsolete"], true);
        assert_eq!(sent[1].1["design"], 9);
        assert!(state.done.contains_key("decom:9"));
        assert_eq!(
            decommission_step(&mut state, &run(&tools, &plan, START + 3 * DAY)).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 2);
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
    fn a_due_declaration_goes_out_in_the_war_first_pass_and_nothing_else_does() {
        let tools = fake(vec![
            ("hoi4_diplomacy", declare_rows(true)),
            (
                "hoi4_war_risk",
                json!({"country": "POL", "would_join": [], "borders_not_held": []}),
            ),
        ]);
        let plan = json!({"diplomacy": [
            {"id": "imp", "action": "improve_relations", "target": "ITA"},
            {"id": "dow", "action": "declare", "target": "POL"}]});
        let mut state = State::default();
        let out = declare_due_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("diplomacy declare_war POL: true Verified")
        );
        let sent = actions(&tools);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1["action"], "declare_war");
        // done: the pass is quiet from then on (and never improves relations)
        assert_eq!(
            declare_due_step(&mut state, &tools, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        // a plan without a declaration reads nothing
        let quiet =
            json!({"diplomacy": [{"id": "imp", "action": "improve_relations", "target": "ITA"}]});
        let mut fresh = State::default();
        assert_eq!(
            declare_due_step(&mut fresh, &tools, &quiet, "l", START).unwrap(),
            None
        );
    }

    #[test]
    fn a_declaration_the_game_did_not_take_is_tried_again_within_the_day() {
        // w47r1 (23 Nov 2026): Luxembourg's declaration came back NotApplied and waited 3 days
        let tools = fake(vec![
            ("hoi4_diplomacy", declare_rows(true)),
            (
                "hoi4_war_risk",
                json!({"country": "POL", "would_join": [], "borders_not_held": []}),
            ),
        ]);
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_diplomacy_act".into(), "NotApplied");
        let plan = json!({"diplomacy": [{"id": "dow", "action": "declare", "target": "POL"}]});
        let mut state = State::default();
        let out = declare_due_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out.unwrap().contains("declare_war POL: false"));
        // the declaration's own wait is a quarter day, not three days
        assert!(state.skipped(START + DAY / 8, "dip", "dip:id:dow"));
        assert!(!state.skipped(START + DAY / 2, "dip", "dip:id:dow"));
        // a typed refusal keeps its three days
        let mut typed = State::default();
        tools
            .refuse
            .borrow_mut()
            .insert("hoi4_diplomacy_act".into(), "action_disabled");
        declare_due_step(&mut typed, &tools, &plan, "l", START).unwrap();
        assert!(typed.skipped(START + 2 * DAY, "dip", "dip:id:dow"));
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
            .contains("skipped: the same order failed 3 time(s)"));
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

    // ----- incoming proposals

    fn proposals() -> Value {
        json!({"proposals": [
            {"index": 0, "class": "lend_lease_request", "title": "Lend-Lease Request",
                "description": "Spain asks us for equipment", "can_accept": true, "can_refuse": true},
            {"index": 1, "class": "guarantee", "title": "Guarantee of Independence",
                "description": "The United Kingdom offers to guarantee our independence",
                "can_accept": true, "can_refuse": true}]})
    }

    #[test]
    fn allies_outside_our_war_are_the_call_to_arms_candidates() {
        let d = json!({"player": {"tag": "GER"},
            "wars": {"at_war": true, "allies_at_war": [{"tag": "SLO"}]},
            "faction": {"faction": {"members": [{"tag": "GER"}, {"tag": "SLO"}, {"tag": "ITA"}, {"tag": "HUN"}]}}});
        assert_eq!(call_candidates(&json!("all"), &d), vec!["ITA", "HUN"]);
        assert_eq!(call_candidates(&json!(["hun"]), &d), vec!["HUN"]);
        assert!(call_candidates(&json!("none"), &d).is_empty());
        let mut peace = d.clone();
        peace["wars"]["at_war"] = json!(false);
        assert!(call_candidates(&json!("all"), &peace).is_empty());
    }

    #[test]
    fn a_request_to_join_our_faction_is_always_decided_and_a_war_target_is_refused() {
        let plan = json!({"diplomacy": [{"action": "justify", "target": "pol"},
                                        {"action": "invite", "target": "HUN"}],
                          "war": {"enemies": ["FRA"]}});
        let rules = default_incoming(&plan);
        let join: Vec<&Value> = rules
            .iter()
            .filter(|r| r["class"] == json!("faction_join_request"))
            .collect();
        assert_eq!(join.len(), 2);
        assert_eq!(join[0]["answer"], json!("refuse"));
        assert_eq!(join[0]["from"], json!(["FRA", "POL"]));
        assert_eq!(join[1]["answer"], json!("accept"));
        assert!(join[1].get("from").is_none());
        assert!(rules
            .iter()
            .any(|r| r["class"] == json!("join_war_offer") && r["answer"] == json!("accept")));
        // no war target: one accepting rule, still a decision
        let rules = default_incoming(&json!({}));
        assert_eq!(
            rules
                .iter()
                .filter(|r| r["class"] == json!("faction_join_request"))
                .count(),
            1
        );
    }

    #[test]
    fn incoming_proposals_are_answered_by_class_and_sender_and_the_rest_wakes_the_player() {
        let tools = fake(vec![
            ("hoi4_diplomacy_incoming", proposals()),
            (
                "hoi4_diplomacy_countries",
                json!({"countries": [{"tag": "ENG", "name": "United Kingdom"},
                    {"tag": "SPR", "name": "Spain"}]}),
            ),
        ]);
        let plan = json!({"incoming_proposals": [
            {"id": "ll_ger", "class": "lend_lease_request", "from": ["GER"], "answer": "accept"},
            {"id": "ll", "class": "lend_lease_request", "answer": "refuse"},
            {"id": "uk", "class": "guarantee", "from": ["ENG"], "answer": "accept", "when": "stability > 0.9"}]});
        let mut state = State::default();
        // the lend-lease request is not from Germany: the second rule refuses it
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(
            out.starts_with("incoming lend_lease_request 'Lend-Lease Request' refuse: true"),
            "{out}"
        );
        let sent = actions(&tools);
        assert_eq!(sent[0].0, "hoi4_diplomacy_incoming");
        assert_eq!(sent[0].1["answer"], "refuse");
        assert_eq!(sent[0].1["index"], 0);
        assert_eq!(sent[0].1["which"], "Lend-Lease Request");
        // answered: the guarantee is left; its rule's `when` does not hold (stability unknown is
        // unknown, so it waits, not falls through)
        let mut left = proposals();
        left["proposals"].as_array_mut().unwrap().remove(0);
        tools.reply("hoi4_diplomacy_incoming", left.clone());
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 1).unwrap(),
            None
        );
        assert!(
            state.affairs.notes["incoming"].contains("when unknown"),
            "{:?}",
            state.affairs.notes
        );
        assert!(wake_reasons(&mut state).is_empty());
        // with the gate false, no rule is left: the player is woken, once
        tools.reply(
            "hoi4_country_status",
            json!({"fields": {"stability": {"value": 0.5}}}),
        );
        // with the gate false, no plan rule is left: the standing answer refuses the guarantee
        // (never silence, never a wake for a bind the plan did not ask for)
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap();
        assert!(
            out.as_deref()
                .unwrap_or("")
                .starts_with("incoming guarantee 'Guarantee of Independence' refuse"),
            "{out:?}"
        );
        assert!(wake_reasons(&mut state).is_empty());
        assert_eq!(actions(&tools).len(), 2);
        // nothing open: one read a game day
        tools.reply("hoi4_diplomacy_incoming", json!({"proposals": []}));
        let before = reads(&tools, "hoi4_diplomacy_incoming");
        let mut state = State::default();
        step(&mut state, &tools, &plan, "l", START).unwrap();
        step(&mut state, &tools, &plan, "l", START + 6).unwrap();
        assert_eq!(reads(&tools, "hoi4_diplomacy_incoming"), before + 1);
    }

    #[test]
    fn an_invitation_the_faction_key_joins_is_left_to_it_and_an_answer_not_offered_wakes() {
        let tools = fake(vec![
            (
                "hoi4_diplomacy_incoming",
                json!({"proposals": [
                {"index": 2, "class": "faction_invitation", "title": "Japan wants us to join their faction",
                    "can_accept": true, "can_refuse": true},
                {"index": 3, "class": "call_to_arms", "title": "Call to Arms",
                    "description": "Germany calls us to war", "can_accept": true, "can_refuse": false}]}),
            ),
            (
                "hoi4_diplomacy_countries",
                json!({"countries": [{"tag": "JAP", "name": "Japan"}]}),
            ),
            (
                "hoi4_diplomacy_state",
                json!({"faction": {"in_faction": false}}),
            ),
        ]);
        let plan = json!({"faction": {"join": "JAP"},
            "incoming_proposals": [{"class": "any", "answer": "refuse"}]});
        let mut state = State::default();
        // the faction step accepts the invitation; the refuse-all rule never touches it
        let out = step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(out.starts_with("faction invitation 2 accepted"), "{out}");
        state.done.remove("faction:join");
        state.skip(START, "mfac", "x", 30 * DAY);
        // the call to arms cannot be refused: the player is woken instead of a failing click
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 1).unwrap(),
            None
        );
        assert_eq!(actions(&tools).len(), 1);
        assert!(state.affairs.notes["incoming"].contains("the faction step joins"));
        assert_eq!(
            wake_reasons(&mut state),
            vec!["incoming:call_to_arms:Call to Arms"]
        );
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
        // the politics view's design-team slot only browses: settled without input
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio ITA_ansaldo_organization trait 3: true Verified")
        );
        assert!(state.done.contains_key("miodt:ITA_ansaldo_organization"));
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
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].1["trait_index"], 3);
        assert_eq!(sent[0].1["slot"], 1);
    }

    #[test]
    fn with_the_bulk_path_one_visit_queues_every_listed_trait_and_keeps_two_waiting() {
        let tools = fake(vec![]);
        let plan = json!({"manage": {"bulk": ["mio_queue_many"]},
            "mio": [{"organisation": "GER_krupp_organization", "traits": [3, 5, 7]},
                    {"organisation": "GER_mauser_organization", "traits": []}]});
        let mut state = State::default();
        state.bulk = super::super::bulk_paths(&plan);
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio GER_krupp_organization traits [3, 5, 7] in one visit: true Verified")
        );
        for i in [3, 5, 7] {
            assert!(state.done.contains_key(&format!("mio:GER_krupp_organization:t{i}")));
        }
        // the auto organisation: one visit lists and queues (no separate listing call)
        let out = step(&mut state, &tools, &plan, "l", START + 2).unwrap();
        assert!(out.unwrap().contains("GER_mauser_organization traits (auto, one visit)"));
        let calls = tools.calls.borrow();
        let mio: Vec<&Value> = calls
            .iter()
            .filter(|(n, _)| n == "hoi4_mio_trait")
            .map(|(_, a)| a)
            .collect();
        assert_eq!(mio.len(), 2, "two visits for both organisations");
        assert_eq!(mio[0]["trait_indices"], json!([3, 5, 7]));
        assert_eq!(mio[1]["auto"], 2);
        assert!(mio.iter().all(|a| a["bulk"] == true));
    }

    #[test]
    fn every_line_gets_an_organisation_the_planned_one_first() {
        let lines = json!({"lines": [
            {"entry_index": 0, "equipment_id": "infantry_equipment_1", "line_handle": "0xa"},
            {"entry_index": 1, "equipment_id": "support_equipment_1", "line_handle": "0xb"},
            {"entry_index": 2, "equipment_id": "convoy_1", "line_handle": "0xc"}]});
        let tools = fake(vec![("hoi4_production_lines", lines)]);
        let plan = json!({"mio": [{"organisation": "GER_mauser_organization",
            "line": "infantry_equipment_2", "design_team": true, "traits": [1]}]});
        let mut state = State::default();
        // the trait list goes first, then the lines one per tick
        step(&mut state, &tools, &plan, "l", START).unwrap();
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio line infantry_equipment_1 -> GER_mauser_organization: true Verified")
        );
        let out = step(&mut state, &tools, &plan, "l", START + 2).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio line support_equipment_1 -> first offered: true Verified")
        );
        tools.refuse("hoi4_mio_line", "line_takes_no_organization");
        let out = step(&mut state, &tools, &plan, "l", START + 3).unwrap();
        assert!(out.unwrap().contains("convoy_1 -> first offered: false"));
        // every line settled: nothing more this week, the convoy line is not tried again
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 4).unwrap(),
            None
        );
        assert!(state.done.contains_key("miol:0xc"));
        let sent: Vec<_> = actions(&tools)
            .into_iter()
            .filter(|(t, _)| t == "hoi4_mio_line")
            .collect();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0].1["organization"], "GER_mauser_organization");
        assert!(sent[1].1.get("organization").is_none());
        // no hoi4_mio_design_team is ever sent
        assert!(actions(&tools)
            .iter()
            .all(|(t, _)| t != "hoi4_mio_design_team"));
    }

    #[test]
    fn a_line_the_tool_cannot_serve_is_tried_twice_then_settled_with_one_wake() {
        let lines = json!({"lines": [
            {"entry_index": 0, "equipment_id": "convoy_1", "line_handle": "0xc"}]});
        let tools = fake(vec![("hoi4_production_lines", lines)]);
        tools.refuse("hoi4_mio_line", "input_readback_failed");
        let plan = json!({"manage": {"off": ["mio_traits"]}});
        let mut state = State::default();
        let mut at = START;
        for _ in 0..60 {
            let _ = step(&mut state, &tools, &plan, "l", at);
            at += DAY;
        }
        let sent = actions(&tools)
            .into_iter()
            .filter(|(t, _)| t == "hoi4_mio_line")
            .count();
        assert_eq!(sent, 2, "two tries in sixty days, then settled");
        assert!(state.done.contains_key("miol:0xc"));
    }

    #[test]
    fn a_planned_organisation_not_offered_falls_back_to_the_first_row() {
        let lines = json!({"lines": [
            {"entry_index": 0, "equipment_id": "artillery_equipment_1", "line_handle": "0xa"}]});
        let tools = fake(vec![("hoi4_production_lines", lines)]);
        tools.refuse("hoi4_mio_line", "organization_not_offered");
        let plan = json!({"mio": [{"organisation": "GER_rheinmetall_organization",
            "line": "artillery_equipment_1", "traits": [0]}],
            "manage": {"off": ["mio_traits"]}});
        let mut state = State::default();
        step(&mut state, &tools, &plan, "l", START).unwrap();
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert!(out
            .unwrap()
            .contains("-> GER_rheinmetall_organization: false"));
        tools.accept("hoi4_mio_line");
        let out = step(&mut state, &tools, &plan, "l", START + 2 * DAY).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio line artillery_equipment_1 -> first offered: true Verified")
        );
    }

    #[test]
    fn the_auto_trait_is_the_first_enabled_and_none_while_one_is_queued() {
        let e = |i: u64, enabled: bool, queue: &str, visible: bool| json!({"index": i, "enabled": enabled, "queue": queue, "visible": visible});
        let none = |_: u64| false;
        assert_eq!(
            auto_trait_pick(&[e(0, false, "", true), e(1, true, "", true)], &none),
            Some(1)
        );
        // a disabled trait is never picked
        assert_eq!(
            auto_trait_pick(&[e(0, false, "", false), e(1, false, "", true)], &none),
            None
        );
        assert_eq!(
            auto_trait_pick(&[e(0, true, "1", true), e(1, true, "", true)], &none),
            None
        );
        assert_eq!(auto_trait_pick(&[], &none), None);
        // a queued trait the game shows disabled never unlocks: the next enabled one is queued
        assert_eq!(
            auto_trait_pick(&[e(0, false, "1", true), e(1, true, "", true)], &none),
            Some(1)
        );
        // a held trait, and one whose click came back unqueued, are passed over
        let mut held = e(0, true, "", true);
        held["owned"] = json!(true);
        assert_eq!(
            auto_trait_pick(&[held, e(1, true, "", true), e(2, true, "", true)], &|i| i
                == 1),
            Some(2)
        );
    }

    #[test]
    fn a_refused_policy_does_not_hold_the_traits() {
        // sk2 (10 Nov): the policy was refused 33 times (organization_not_offered) and, because
        // the policy always went first under one skip timer, not one trait was queued.
        let tools = fake(vec![]);
        tools.refuse("hoi4_mio_policy", "policy_not_offered");
        let plan = json!({"mio": [{"organisation": "ITA_beretta_organization",
            "policy": "mio_policy_assembly_line_optimizers", "traits": [2, 4]}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out
            .unwrap()
            .contains("policy mio_policy_assembly_line_optimizers: false"));
        let out = step(&mut state, &tools, &plan, "l", START + 1).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio ITA_beretta_organization trait 2: true Verified")
        );
        let out = step(&mut state, &tools, &plan, "l", START + 2).unwrap();
        assert_eq!(
            out.as_deref(),
            Some("mio ITA_beretta_organization trait 4: true Verified")
        );
        // the policy is tried again after its own wait, not before
        assert_eq!(
            step(&mut state, &tools, &plan, "l", START + 3).unwrap(),
            None
        );
        tools.accept("hoi4_mio_policy");
        let out = step(&mut state, &tools, &plan, "l", START + 15 * DAY).unwrap();
        assert!(out
            .unwrap()
            .contains("policy mio_policy_assembly_line_optimizers: true"));
    }

    #[test]
    fn an_organisation_no_list_offers_waits_30_days_with_the_reason() {
        let tools = fake(vec![(
            "hoi4_mio_trait",
            json!({"observed": {"listed": [{"index": 0, "enabled": true, "queue": "", "visible": true}]}}),
        )]);
        tools.refuse("hoi4_mio_trait", "organization_not_offered");
        let plan = json!({"mio": [{"organisation": "GER_mauser_organization", "traits": []}]});
        let mut state = State::default();
        let out = step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(out
            .unwrap()
            .contains("(auto): false organization_not_offered"));
        assert!(note_of(&state, "mio_traits")
            .unwrap()
            .contains("no organisation slot's list offers it"));
        assert!(state.skipped(START + 29 * DAY, "mioauto", "GER_mauser_organization"));
        assert!(!state.skipped(START + 31 * DAY, "mioauto", "GER_mauser_organization"));
        let orders = actions(&tools)
            .into_iter()
            .filter(|(_, a)| a["list"] != json!(true))
            .count();
        assert_eq!(orders, 1);
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
    fn a_network_starts_in_the_nearest_enemy_state_beside_own_land() {
        let tools = fake(vec![
            ("hoi4_overview", json!({"player": "ME"})),
            (
                "hoi4_states",
                json!({"states": [
                    {"state_id": 1, "controller": "ME"},
                    {"state_id": 2, "controller": "ENE"},
                    {"state_id": 3, "controller": "ENE"},
                    {"state_id": 4, "controller": "ENE"}]}),
            ),
            (
                "hoi4_province_graph",
                json!({"provinces": [
                    {"province": 10, "state_id": 1, "neighbours": [20]},
                    {"province": 20, "state_id": 2, "neighbours": [10, 30]},
                    {"province": 30, "state_id": 3, "neighbours": [20, 40]},
                    {"province": 40, "state_id": 4, "neighbours": [30]}]}),
            ),
        ]);
        assert_eq!(network_state(&tools, 2), Some(Some(2)));
        assert_eq!(network_state(&tools, 4), Some(Some(2)));
        // own state: nothing to choose
        assert_eq!(network_state(&tools, 1), None);
    }

    #[test]
    fn an_upgrade_the_game_would_only_queue_waits_for_free_factories() {
        let tools = fake(vec![(
            "hoi4_intelligence",
            json!({
            "upgrades": {"branches": [
                {"branch": "branch_operative", "upgrades": [{"key": "upgrade_training_centers"}]}]},
            "defines": {"AGENCY_CREATION_DAYS": 30.0, "AGENCY_UPGRADE_DAYS": 30.0}}),
        )]);
        let plan =
            json!({"intelligence": {"agency": true, "upgrades": ["upgrade_training_centers"]}});
        let mut state = State::default();
        step(&mut state, &tools, &plan, "l", START).unwrap();
        tools.refuse("hoi4_agency_upgrade", "upgrade_needs_factories");
        let at = START + 30 * DAY;
        let out = step(&mut state, &tools, &plan, "l", at).unwrap().unwrap();
        assert!(out.contains("upgrade_needs_factories"), "{out}");
        assert!(state.affairs.notes["intelligence"].contains("upgrade_training_centers"));
        // not sent again for ten days
        let sent = actions(&tools).len();
        step(&mut state, &tools, &plan, "l", at + 9 * DAY).unwrap();
        assert_eq!(actions(&tools).len(), sent);
    }

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

    #[test]
    fn the_war_goal_watch_reads_the_war_day_and_drops_focus_gates() {
        let item = json!({"action": "declare_war", "target": "POL",
            "when": {"all": ["date >= 1939.9.1", "focus_completed:GER_danzig_or_war"]}});
        assert_eq!(planned_day(&item), date_hour("1939.9.1"));
        assert_eq!(
            without_focus_atoms(&item["when"]),
            json!({"all": ["date >= 1939.9.1"]})
        );
        assert_eq!(
            planned_day(&json!({"war_day": "1940.5.10", "when": "date >= 1939.9.1"})),
            date_hour("1940.5.10")
        );
        assert_eq!(planned_day(&json!({"when": "focus_completed:X"})), None);
        assert_eq!(
            without_focus_atoms(&json!("focus_completed:X")),
            Value::Null
        );
    }
}
