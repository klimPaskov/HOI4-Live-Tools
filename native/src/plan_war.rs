//! War-step rules: the gates and thresholds the war step in plan_run.rs consults before it
//! orders a driven army (planning before an offensive, supply and organisation limits, the
//! pocket fall-back, the stall response, the bound pause after a captured supply node) and the
//! disband step's land-route guard. A child module of `plan_run` (declared there with
//! `#[path]`), like plan_military.rs: every order still goes through `State::act` and its
//! failure ledger; this module's memory (`State::war_rules`) is this server life's, like the
//! war step's own stages. All cadences are game time.
//!
//! Hooks in plan_run.rs: `war_step` reads [`Limits`] once per tick and feeds [`observe`] the
//! armies' organisation; [`execution_step`] sets an army's execution mode and [`stall_rush`]
//! answers a stall with it; [`line_behind`] says when the offensive line must follow the plan's
//! next row; [`drawn`] marks an offensive line, [`execute_gate`] holds its execute
//! order; [`captured`] starts a bound when the army's target falls; [`relocation`] and
//! [`second_direction`] are the stall response before the direct move; [`pocket_risk`] decides
//! `pocket_fall_back`; `disband_step` keeps only divisions [`home_land`] reaches; `run` adds
//! [`digest`] as `war`. The way to a victory-point target (a plan `path`, leg by leg) is
//! plan_route.rs.
//!
//! Plan keys (all under `war`, all optional; percentages may be written 0.5 or 50):
//!   planning_wait      false: execute an offensive the tick after its line is drawn (the old
//!                      behaviour). Default on: after `hoi4_army_offensive` the execute order waits
//!                      until the army's planning bonus reaches "planning_share" (0.9) of
//!                      "planning_cap" (0.30, the game's base; techs, doctrines and traits raise it)
//!                      at "planning_gain" (0.02 a game day), at most "planning_max_days" (15).
//!                      The army's planning bonus is read (hoi4_armies planning.mean, the mean of
//!                      its divisions): it executes once the bonus reaches that share of the cap,
//!                      or once it has stopped rising for a game day at half that level or more
//!                      (it stands at the army's own cap, which techs, doctrines, traits and the HQ
//!                      distance set and no tool reads; a lower bonus that stands still belongs to
//!                      divisions still fighting or moving: the wait goes on for "planning_flat_days"
//!                      (3) and then ends, since nothing is being built). Without the read the bonus is estimated from the game days since
//!                      the line was drawn and the digest says so; the 1.19 x0.8 without a deployed
//!                      HQ scales cap and speed alike, so the wait is the same. No wait when the
//!                      goal deadline is less than "planning_skip_days" (20) away. A rest resumes at
//!                      once (planning rebuilt while resting)
//!   execution          "careful" | "balanced" | "rush": how the driven armies' battle plans are
//!                      carried out (the three buttons of the battle plan bar; hoi4_army_plan mode).
//!                      Sent once per army when it has its lines, before its first execute order,
//!                      and for an army formed later; kept in plan_state.json and under a new name.
//!                      Absent: the game's default (balanced) stands and nothing is sent. Rush
//!                      attacks every province of the plan: it suits a weak or broken enemy line,
//!                      lengthens the front and can gather an army in one province without supply,
//!                      so it is a plan decision. A set that fails is tried again after 3 game days
//!                      and under the failure ledger; the execute order never waits for it
//!   stall_rush         true: an executing plan that took no ground for stall_days is first set
//!                      to rush (unless `execution` is rush already): the plan keeps executing and
//!                      keeps its planning bonus, the stall clock starts again, and only the next
//!                      stall stops the plan to plan again. Once per army; it stays on rush until
//!                      the plan names another mode. Default off (absent or false): a stall goes
//!                      straight to its other answers. Rush did not make a standing plan attack
//!                      (live 15 Nov 2026: an army executing on rush stood three weeks with a flat
//!                      planning bonus, and thirteen divisions ended in one province without
//!                      supply), so the step costs a stall period only where the plan asks for it
//!   hold_prepares      false: a hold phase (`axes: []`) draws no line on enemy land (the old hold).
//!                      Default: in a hold phase the armies still draw their front and offensive
//!                      line toward their next target and plan on them; the execute order waits for
//!                      a phase with axes, and a plan still executing on enemy land is stopped. Off
//!                      with `direct_moves`. Lines are never held back by the limits below: an army
//!                      short of organisation, supply or equipment draws them and plans while it
//!                      recovers; the limits hold back the execute order and every move
//!   supply_floor       50: an executing army below this supply percent rests (plan stopped), and no
//!                      army below it gets a new attack order (execute, front, offensive, direct
//!                      or stall move), except a move to retake an own state
//!   supply_hold        35: below it nothing attacks, own states included, and a rest does not
//!                      end at rest_max_days (attrition starts at 35%)
//!   supply_resume      70: a rested army resumes at or above it (with the organisation below)
//!   rest_below_pct     0.30 / resume_above_pct 0.80: rest below, resume at, this share of the
//!                      army's maximum organisation. No tool reads maximum organisation: it is the
//!                      highest organisation_mean (hoi4_supply) seen for the army this server life.
//!                      An absolute "rest_below" / "resume_above" in the plan replaces the share
//!                      (the old behaviour; rest_max_days 20 still resumes when supply allows)
//!   pocket_share       0.5: an army falls back when this share of the land provinces around it is
//!                      held by others (hoi4_armies threat.share) and it stands outside a fully held
//!                      own state; the game-side flag (encirclement_risk, 0.7) falls back anywhere.
//!                      Either only when at most one land province around the army is still the
//!                      player's (threat.neighbour_provinces - held_by_others): with more own land
//!                      around it the army stands on a front, not in a pocket
//!   second_direction   false: a stall goes straight to the direct move. Default: the first stall
//!                      on a target (no progress for stall_days) brings a second direction from
//!                      the target's own neighbours (hoi4_wars enemy_held_adjacent_states): an
//!                      idle driven army standing in another own state next to the target is sent
//!                      at it too (it draws its own front and offensive); with none, the stalled
//!                      army stops its plan, moves to the nearest other own state next to the
//!                      target and draws its front and offensive again from there (planning wait
//!                      included). The next stall on the same target is the direct move, with the
//!                      back-off. State adjacency is read, not province adjacency
//!   bound              false: no bound pause. Default: when a driven army's target falls and the
//!                      state holds an own supply hub or port (hoi4_supply supply_nodes, by the
//!                      provinces of hoi4_states) or railway, the army's next execute (or direct
//!                      move) waits until a captured node feeds the provinces where own divisions
//!                      stand (a flow above 0) or "bound_rail_days" (10; "bound_core_days" 5 in an
//!                      own state) have passed, and its supply is at the floor; at most
//!                      "bound_max_days" (12). Fronts and offensive lines are drawn meanwhile, so
//!                      the bound builds planning
//!   supply_relief      true: an army below supply_floor for "relief_after_days" (2) gets the
//!                      executor's own supply levers: a railway to the own railway node nearest the
//!                      province where it starves, from the nearest node the network feeds (once per
//!                      province in 30 days), then one more motorization level. After
//!                      "relief_report_days" (10) below the floor the urgent change and alarm
//!                      army_out_of_supply report the spell and what was tried. false: report only
//!
//! The disband guard has no key: `disband` takes a division only where the capital reaches it
//! over own-controlled land (hoi4_wars capital_land; overseas 75% of the equipment is lost) and
//! never one of an army at encirclement risk; without that read it disbands nothing.
use super::{list, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// What these rules keep (this server life).
#[derive(Default)]
pub(super) struct Memory {
    /// The last status line per rule and army (the run result's `war`).
    notes: BTreeMap<String, String>,
    /// army -> (target, game hour its offensive line was drawn)
    drawn: HashMap<String, (i32, GameHour)>,
    /// army -> highest organisation_mean seen (the maximum organisation estimate)
    peak: HashMap<String, f64>,
    /// army -> the bound pause after a captured supply node or railway
    bound: HashMap<String, Bound>,
    /// army -> the target a second direction was already tried for
    second: HashMap<String, i32>,
    /// army -> the move to another side of the target (stall response)
    via: HashMap<String, Via>,
    /// army -> (highest planning bonus read, game hour it was first read at that level)
    planning: HashMap<String, (f64, GameHour)>,
    /// army -> its current spell below the supply floor (supply relief)
    starved: HashMap<String, Starved>,
    /// target province -> game hour a relief railway was last asked for it
    relief_rail: HashMap<i32, GameHour>,
    /// armies planning again on a line they already attacked along (a stopped offensive)
    replanned: HashSet<String>,
    /// army -> the `war.execution` mode last set for it (kept in plan_state.json)
    exec: HashMap<String, String>,
    /// armies a stall set to rush (kept in plan_state.json)
    rushed: HashSet<String>,
    /// army -> (target state, row province) its offensive line was last drawn for (province 0:
    /// through the middle of the state); kept in plan_state.json
    line: HashMap<String, (i32, i64)>,
    /// army -> game hour its line was drawn again for the next row while its plan executed
    redrawn: HashMap<String, GameHour>,
}

/// One spell of an army below the supply floor: what the executor tried for it.
#[derive(Clone, Default)]
struct Starved {
    since: GameHour,
    lowest: f64,
    levers: Vec<String>,
    motorized: bool,
    reported: bool,
}

#[derive(Clone)]
struct Bound {
    state: i32,
    since: GameHour,
    /// provinces of own supply hubs and ports in the captured state
    nodes: Vec<i64>,
    rail: bool,
    core: bool,
}

#[derive(Clone, Copy)]
struct Via {
    target: i32,
    state: i32,
    /// the move was sent (else the plan was stopped and the move is next)
    moving: bool,
    at: GameHour,
}

/// An army was renamed: what is kept for it follows the name.
pub(super) fn army_renamed(memory: &mut Memory, old: &str, new: &str) {
    fn rekey<V>(map: &mut HashMap<String, V>, old: &str, new: &str) {
        if let Some(v) = map.remove(old) {
            map.insert(new.to_string(), v);
        }
    }
    rekey(&mut memory.drawn, old, new);
    rekey(&mut memory.peak, old, new);
    rekey(&mut memory.bound, old, new);
    rekey(&mut memory.second, old, new);
    rekey(&mut memory.via, old, new);
    rekey(&mut memory.starved, old, new);
    if memory.replanned.remove(old) {
        memory.replanned.insert(new.to_string());
    }
    rekey(&mut memory.exec, old, new);
    if memory.rushed.remove(old) {
        memory.rushed.insert(new.to_string());
    }
    rekey(&mut memory.line, old, new);
    rekey(&mut memory.redrawn, old, new);
}

/// What of these rules survives a server restart (plan_state.json `war.rules`).
pub(super) fn memory(memory: &Memory) -> Value {
    let mut rushed: Vec<&String> = memory.rushed.iter().collect();
    rushed.sort();
    let line: HashMap<&String, [i64; 2]> = memory
        .line
        .iter()
        .map(|(army, (target, province))| (army, [i64::from(*target), *province]))
        .collect();
    json!({"execution": memory.exec, "rushed": rushed, "line": line})
}

/// Take back a saved memory; what is already known for an army stands.
pub(super) fn restore(memory: &mut Memory, saved: &Value) {
    if let Some(modes) = saved["execution"].as_object() {
        for (army, mode) in modes {
            if let Some(mode) = mode.as_str().filter(|m| EXECUTION_MODES.contains(m)) {
                memory
                    .exec
                    .entry(army.clone())
                    .or_insert_with(|| mode.to_string());
            }
        }
    }
    for army in list(saved, "rushed").iter().filter_map(Value::as_str) {
        memory.rushed.insert(army.to_string());
    }
    if let Some(lines) = saved["line"].as_object() {
        for (army, at) in lines {
            let target = at[0].as_i64().and_then(|t| i32::try_from(t).ok());
            if let (Some(target), Some(province)) = (target, at[1].as_i64()) {
                memory
                    .line
                    .entry(army.clone())
                    .or_insert((target, province));
            }
        }
    }
}

/// Armies that no longer exist are forgotten: a new army that takes an old name starts afresh.
pub(super) fn forget_absent(state: &mut State, present: &HashSet<String>) {
    state
        .war_rules
        .exec
        .retain(|army, _| present.contains(army));
    state.war_rules.rushed.retain(|army| present.contains(army));
    state
        .war_rules
        .line
        .retain(|army, _| present.contains(army));
    state
        .war_rules
        .redrawn
        .retain(|army, _| present.contains(army));
}

/// The last status of each rule (the run result's `war`), or None when nothing was said.
pub(super) fn digest(state: &State) -> Option<Value> {
    (!state.war_rules.notes.is_empty()).then(|| json!(state.war_rules.notes))
}

fn note(state: &mut State, key: &str, line: impl Into<String>) {
    state.war_rules.notes.insert(key.to_string(), line.into());
}

/// Set a status line of the run result's `war` block, or clear it with None (plan_route.rs).
pub(super) fn set_note(state: &mut State, key: &str, line: Option<String>) {
    match line {
        Some(line) => note(state, key, line),
        None => {
            state.war_rules.notes.remove(key);
        }
    }
}

/// A share: 0.3 or 30 (percent) both mean 0.3.
fn share(v: &Value, default: f64) -> f64 {
    v.as_f64()
        .map_or(default, |f| if f > 1.0 { f / 100.0 } else { f })
}

/// A supply percentage: 50 or 0.5 both mean 50%.
fn percent(v: &Value, default: f64) -> f64 {
    v.as_f64()
        .map_or(default, |f| if f <= 1.0 { f * 100.0 } else { f })
}

fn days(v: &Value, default: f64) -> f64 {
    v.as_f64().filter(|d| *d >= 0.0).unwrap_or(default)
}

// ---------------------------------------------------------------------------------------------
// Organisation and supply limits.

/// The organisation and supply limits of the war step.
pub(super) struct Limits {
    rest_abs: Option<f64>,
    resume_abs: Option<f64>,
    rest_pct: f64,
    resume_pct: f64,
    supply_floor: f64,
    supply_hold: f64,
    supply_resume: f64,
    fill_floor: f64,
}

impl Limits {
    pub(super) fn of(plan: &Value) -> Self {
        let cfg = &plan["war"];
        let supply_floor = percent(&cfg["supply_floor"], 50.0);
        let supply_hold = percent(&cfg["supply_hold"], 35.0).min(supply_floor);
        Self {
            rest_abs: cfg["rest_below"].as_f64(),
            resume_abs: cfg["resume_above"].as_f64(),
            rest_pct: share(&cfg["rest_below_pct"], 0.30),
            resume_pct: share(&cfg["resume_above_pct"], 0.80),
            supply_floor,
            supply_hold,
            supply_resume: percent(&cfg["supply_resume"], 70.0).max(supply_floor),
            // Every missing 1% of equipment costs about 1% attack: no new offensive below this mean
            // fill (playbook OP20; sk9 and sk10 attacked at 0.08-0.3 and lost 4-6 men per enemy).
            fill_floor: share(&cfg["fill_floor"], 0.9),
        }
    }
    /// Mean equipment fill below the floor: no new attack, and no resume after a rest.
    pub(super) fn underfilled(&self, fill: Option<f64>) -> bool {
        fill.is_some_and(|f| f < self.fill_floor)
    }
    fn line(&self, state: &State, army: &str, abs: Option<f64>, pct: f64) -> f64 {
        abs.unwrap_or_else(|| pct * state.war_rules.peak.get(army).copied().unwrap_or(0.0))
    }
    /// Organisation below the rest line.
    pub(super) fn tired(&self, state: &State, army: &str, org: f64) -> bool {
        org < self.line(state, army, self.rest_abs, self.rest_pct)
    }
    /// Organisation at or above the resume line.
    pub(super) fn fresh(&self, state: &State, army: &str, org: f64) -> bool {
        org >= self.line(state, army, self.resume_abs, self.resume_pct)
    }
    /// Supply below the floor: no new attack.
    pub(super) fn starving(&self, supply: f64) -> bool {
        supply < self.supply_floor
    }
    /// Supply below the hard hold: no attack at all.
    pub(super) fn held(&self, supply: f64) -> bool {
        supply < self.supply_hold
    }
    /// Supply good enough to resume after a rest.
    pub(super) fn supplied(&self, supply: f64) -> bool {
        supply >= self.supply_resume
    }
    pub(super) fn floor(&self) -> f64 {
        self.supply_floor
    }
    /// Supply below which nothing attacks (attrition starts).
    pub(super) fn hold(&self) -> f64 {
        self.supply_hold
    }
    /// Supply a rested army resumes at.
    pub(super) fn resume(&self) -> f64 {
        self.supply_resume
    }
}

/// Mean equipment fill per army (hoi4_supply divisions[].equipment_fill_min, 0..1); an army none
/// of whose divisions reads a fill is absent (the gate then does not hold it).
pub(super) fn army_fill(supply: Option<&Value>) -> HashMap<String, f64> {
    let mut sums: HashMap<String, (f64, f64)> = HashMap::new();
    for d in supply
        .and_then(|s| s["divisions"].as_array())
        .into_iter()
        .flatten()
    {
        if let (Some(army), Some(fill)) = (d["army"].as_str(), d["equipment_fill_min"].as_f64()) {
            let e = sums.entry(army.to_string()).or_insert((0.0, 0.0));
            e.0 += fill;
            e.1 += 1.0;
        }
    }
    sums.into_iter().map(|(a, (s, n))| (a, s / n)).collect()
}

/// Record each army's organisation: the highest value seen is its maximum estimate.
pub(super) fn observe(state: &mut State, condition: &HashMap<String, (f64, f64)>) {
    for (army, (org, _)) in condition {
        let peak = state.war_rules.peak.entry(army.clone()).or_insert(0.0);
        if *org > *peak {
            *peak = *org;
        }
    }
    if !condition.is_empty() {
        note(
            state,
            "rest",
            "organisation shares are of the highest organisation_mean seen per army (no maximum-organisation read)",
        );
    }
}

/// Say why an army gets no attack order now.
pub(super) fn held_note(state: &mut State, army: &str, why: String) {
    note(state, &format!("hold:{army}"), why);
}

// ---------------------------------------------------------------------------------------------
// Supply relief: low supply feeds back into construction and the army's trucks.

/// The province where the army's divisions stand worst supplied (most divisions there, then the
/// lowest supply), with its state, from hoi4_supply divisions.
fn starving_province(supply: Option<&Value>, army: &str) -> Option<(i64, i64)> {
    let mut at: BTreeMap<(i64, i64), (usize, f64)> = BTreeMap::new();
    for d in list(supply?, "divisions") {
        if text(&d["army"]) != army {
            continue;
        }
        let (Some(p), Some(s)) = (d["province_id"].as_i64(), d["state_id"].as_i64()) else {
            continue;
        };
        let pct = d["supply_percent"].as_f64().unwrap_or(100.0);
        let e = at.entry((p, s)).or_insert((0, 100.0));
        e.0 += 1;
        e.1 = e.1.min(pct);
    }
    at.into_iter()
        .filter(|(_, (_, pct))| *pct < 100.0)
        .max_by(|a, b| a.1 .0.cmp(&b.1 .0).then(b.1 .1.total_cmp(&a.1 .1)))
        .map(|(k, _)| k)
}

/// An army below the supply floor does not just wait: the executor pulls its own supply levers
/// first, then asks for a decision. Below the floor for `relief_after_days` (2): a railway to the
/// own railway node nearest to where the army starves, from the nearest node the network
/// already feeds (it joins a captured or cut-off hub, or raises the level of the line that feeds
/// it; at most once per province in 30 days); then one more level of supply motorization
/// (trucks carry supply farther from the hubs). After `relief_report_days` (10) below the floor
/// the spell is reported once as the urgent change `army_out_of_supply` with what was tried, so
/// the player or a plan revision changes the pace, the axis or the supply plan (a port, a hub, a
/// shorter line). `war.supply_relief: false` keeps the levers off (the report stays). The spell
/// ends when the supply is back at the floor.
#[allow(clippy::too_many_arguments)]
pub(super) fn supply_relief(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    plan: &Value,
    army: &Value,
    supply_read: Option<&Value>,
    supply: f64,
    limits: &Limits,
    now: GameHour,
) -> Result<Option<String>> {
    let name = text(&army["army"]).to_string();
    if !limits.starving(supply) {
        state.war_rules.starved.remove(&name);
        return Ok(None);
    }
    let cfg = &plan["war"];
    let spell = state
        .war_rules
        .starved
        .entry(name.clone())
        .or_insert_with(|| Starved {
            since: now,
            lowest: supply,
            ..Default::default()
        });
    spell.lowest = spell.lowest.min(supply);
    let elapsed = (now - spell.since) as f64 / DAY as f64;
    let motorized = spell.motorized;
    let levers_on = cfg["supply_relief"] != json!(false);
    if levers_on && elapsed >= days(&cfg["relief_after_days"], 2.0) {
        let place = starving_province(supply_read, &name);
        let rail_due = place.is_some_and(|(p, _)| {
            state
                .war_rules
                .relief_rail
                .get(&(p as i32))
                .is_none_or(|at| now - at >= 30 * DAY)
        });
        if let (Some((p, s)), true) = (place, rail_due) {
            state.war_rules.relief_rail.insert(p as i32, now);
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_railway_build",
                json!({"to_province": p, "to_state": s, "to_nearest_node": true,
                    "from_nearest_node": true}),
                "ws",
            )?;
            let line = format!("railway to the node nearest province {p}: {ok} {why}");
            if let Some(spell) = state.war_rules.starved.get_mut(&name) {
                spell.levers.push(line.clone());
            }
            return Ok(Some(format!(
                "supply relief {name} (supply {supply:.0}%, state {s}): {line}"
            )));
        }
        let level = army["motorization"].as_u64();
        if let (Some(level), false) = (level, motorized) {
            if level < 2 {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_motorization",
                    json!({"army": name, "level": level + 1}),
                    "ws",
                )?;
                let line = format!("motorization {level} -> {}: {ok} {why}", level + 1);
                if let Some(spell) = state.war_rules.starved.get_mut(&name) {
                    spell.motorized = true;
                    spell.levers.push(line.clone());
                }
                return Ok(Some(format!(
                    "supply relief {name} (supply {supply:.0}%): {line}"
                )));
            }
        }
    }
    let Some(spell) = state.war_rules.starved.get_mut(&name) else {
        return Ok(None);
    };
    if elapsed >= days(&cfg["relief_report_days"], 10.0) && !spell.reported {
        spell.reported = true;
        let levers = if spell.levers.is_empty() {
            "nothing (no starving province read, or supply_relief off)".to_string()
        } else {
            spell.levers.join("; ")
        };
        let detail = json!({"army": name, "state": army["main_state"],
            "supply": (supply * 10.0).round() / 10.0, "lowest": (spell.lowest * 10.0).round() / 10.0,
            "days": elapsed.round(), "levers": levers,
            "next": "the axis outruns its supply: pace the offensive to the supply reach, bring \
                     supply forward (a port, a hub, railway levels on the feeding line, trucks) or \
                     pull part of the army back to supplied provinces"});
        note(
            state,
            &format!("supply:{name}"),
            format!("{name} below the supply floor for {elapsed:.0} days: {levers}"),
        );
        crate::watch::feed().note(crate::watch::ARMY_OUT_OF_SUPPLY, true, detail.clone());
        state
            .strategy
            .news
            .push(json!({"kind": crate::watch::ARMY_OUT_OF_SUPPLY, "detail": detail}));
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------
// The execution mode of the battle plans.

/// The modes `war.execution` may name (hoi4_army_plan mode).
pub(super) const EXECUTION_MODES: [&str; 3] = ["careful", "balanced", "rush"];
/// Game hours a failed mode order is left alone (the failure ledger counts on top).
const EXECUTION_RETRY: GameHour = 3 * DAY;

/// `war.execution`: the mode the plan names, or None (absent, or no mode: nothing is sent).
pub(super) fn execution(plan: &Value) -> Option<&'static str> {
    let wanted = plan["war"]["execution"].as_str()?;
    EXECUTION_MODES.iter().copied().find(|m| *m == wanted)
}

/// The plan's execution mode for an army that has its lines: sent once per army (and again when
/// the plan names another mode). Some(line) when an order went out. A failed order is left
/// alone for EXECUTION_RETRY, so the army's execute order goes out on the next tick.
pub(super) fn execution_step(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    plan: &Value,
    army: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let Some(mode) = execution(plan) else {
        return Ok(None);
    };
    if state.war_rules.exec.get(army).map(String::as_str) == Some(mode)
        || state.skipped(now, "exec", army)
    {
        return Ok(None);
    }
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_army_plan",
        json!({"army": army, "mode": mode}),
        "wx",
    )?;
    if super::held_back(&why) {
        return Ok(None);
    }
    if ok {
        state
            .war_rules
            .exec
            .insert(army.to_string(), mode.to_string());
        state.war_rules.rushed.remove(army);
    } else {
        state.skip(now, "exec", army, EXECUTION_RETRY);
    }
    Ok(Some(format!("war execution {army} {mode}: {ok} {why}")))
}

/// With `war.stall_rush: true`, the first answer to an executing plan that took no ground for
/// `days`: the rush mode. The plan keeps executing and keeps its planning bonus (a stop gives
/// both up), and rush attacks every province of the plan where balanced waits for a shorter
/// front. Once per army, and not when the plan asks for rush already. Off unless the plan asks
/// for it: rush lengthens the front and can gather an army in one unsupplied province, and it
/// did not make a standing plan attack (live 15 Nov 2026).
/// Some((ok, line)) when the order went out; the caller restarts the stall clock on ok.
#[allow(clippy::too_many_arguments)]
pub(super) fn stall_rush(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    plan: &Value,
    army: &str,
    target: i32,
    days: GameHour,
    now: GameHour,
) -> Result<Option<(bool, String)>> {
    if plan["war"]["stall_rush"] != json!(true)
        || execution(plan) == Some("rush")
        || state.war_rules.rushed.contains(army)
        || state.skipped(now, "rush", army)
    {
        return Ok(None);
    }
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_army_plan",
        json!({"army": army, "mode": "rush"}),
        "wx",
    )?;
    if super::held_back(&why) {
        return Ok(None);
    }
    if ok {
        state.war_rules.rushed.insert(army.to_string());
    } else {
        state.skip(now, "rush", army, EXECUTION_RETRY);
    }
    Ok(Some((
        ok,
        format!(
            "war stalled {army} {days} days on state {target}: execution set to rush: {ok} {why}"
        ),
    )))
}

// ---------------------------------------------------------------------------------------------
// The offensive line follows the plan's rows.

/// Game hours a refused redraw of the offensive line is left alone (the ledger counts on top).
pub(super) const LINE_RETRY: GameHour = DAY;
/// Game hours after a redraw in which a plan found stopped is executed again at once.
const REDRAW_RESUME: GameHour = 3 * DAY;

/// The army's offensive line into `target` was drawn at the depth of `row` (the province of
/// its victory-point row; None: through the middle of the state).
pub(super) fn line_drawn(state: &mut State, army: &str, target: i32, row: Option<i64>) {
    state
        .war_rules
        .line
        .insert(army.to_string(), (target, row.unwrap_or(0)));
}

/// The row the army's offensive line into `target` stands at when the plan is after another
/// row of the same state now (`row`): the line is to be drawn again for it. The offensive line
/// is drawn across the target at the depth of one row, and the game holds the army on it: with
/// the row held and the next one deeper in the same state the target state does not change,
/// so nothing drew a new line and the plan went on executing with every division standing on
/// the old one (live 15 Nov 2026: Aksum and Adwa held, Macalle in the same state never
/// attacked; a line drawn by hand at Macalle's depth took it). None when the line stands at
/// `row`. Lines whose row is not known (drawn before a restart without this memory, or by the
/// player) are taken as drawn for the current row.
pub(super) fn line_behind(state: &mut State, army: &str, target: i32, row: i64) -> Option<i64> {
    match state.war_rules.line.get(army) {
        Some((t, at)) if *t == target => (*at != row).then_some(*at),
        _ => {
            line_drawn(state, army, target, Some(row));
            None
        }
    }
}

/// The line was drawn again for the next row. With the plan executing the front is the same
/// and the game keeps executing; should it stop the plan, [`redrawn_lately`] lets the execute
/// order go out again without a planning wait.
pub(super) fn line_redrawn(
    state: &mut State,
    army: &str,
    target: i32,
    row: i64,
    executing: bool,
    now: GameHour,
) {
    line_drawn(state, army, target, Some(row));
    if executing {
        state.war_rules.redrawn.insert(army.to_string(), now);
    } else {
        state.war_rules.redrawn.remove(army);
    }
}

/// The army's plan was executing when its line was drawn again less than REDRAW_RESUME ago.
pub(super) fn redrawn_lately(state: &mut State, army: &str, now: GameHour) -> bool {
    let rules = &mut state.war_rules;
    if rules
        .redrawn
        .get(army)
        .is_some_and(|at| now - at > REDRAW_RESUME)
    {
        rules.redrawn.remove(army);
    }
    rules.redrawn.contains_key(army)
}

/// The execute order after a redraw went out (or the plan is back before its planning wait).
pub(super) fn redraw_settled(state: &mut State, army: &str) {
    state.war_rules.redrawn.remove(army);
}

// ---------------------------------------------------------------------------------------------
// Planning before execution, and the bound pause.

/// The army's offensive line into `target` was drawn now: its planning starts.
pub(super) fn drawn(state: &mut State, army: &str, target: i32, now: GameHour) {
    state.war_rules.replanned.remove(army);
    state
        .war_rules
        .drawn
        .insert(army.to_string(), (target, now));
}

/// A stalled offensive is planned again: the planning wait starts over on the same line.
pub(super) fn replan(state: &mut State, army: &str, target: i32, now: GameHour) {
    state.war_rules.planning.remove(army);
    drawn(state, army, target, now);
    state.war_rules.replanned.insert(army.to_string());
}

/// Why the army's execute order waits (None: execute now): the bound pause, then planning.
/// `planning` is the army's planning bonus read now (hoi4_armies planning.mean), if any.
#[allow(clippy::too_many_arguments)]
pub(super) fn execute_gate(
    state: &mut State,
    plan: &Value,
    army: &str,
    target: i32,
    supply: Option<&Value>,
    supply_mean: Option<f64>,
    planning: Option<f64>,
    now: GameHour,
) -> Option<String> {
    if let Some(why) = bound_wait(state, plan, army, supply, supply_mean, now) {
        return Some(why);
    }
    planning_wait(state, plan, army, target, planning, now)
}

fn planning_wait(
    state: &mut State,
    plan: &Value,
    army: &str,
    target: i32,
    read: Option<f64>,
    now: GameHour,
) -> Option<String> {
    let cfg = &plan["war"];
    let key = format!("planning:{army}");
    if cfg["planning_wait"] == json!(false) {
        return None;
    }
    if let Some(deadline) = plan["goal"]["deadline"].as_str().and_then(super::date_hour) {
        let skip = days(&cfg["planning_skip_days"], 20.0);
        if ((deadline - now) as f64) < skip * DAY as f64 {
            note(
                state,
                &key,
                format!("no planning wait: the goal deadline is less than {skip} days away"),
            );
            return None;
        }
    }
    let cap = share(&cfg["planning_cap"], 0.30);
    let wanted = share(&cfg["planning_share"], 0.9);
    let gain = share(&cfg["planning_gain"], 0.02).max(1e-4);
    let max_days = days(&cfg["planning_max_days"], 15.0);
    let need = (wanted * cap / gain).min(max_days);
    let since = match state.war_rules.drawn.get(army) {
        Some((t, at)) if *t == target => *at,
        _ => {
            drawn(state, army, target, now);
            now
        }
    };
    let elapsed = (now - since) as f64 / DAY as f64;
    if let Some(bonus) = read {
        return planning_read(
            state,
            &key,
            army,
            target,
            bonus,
            cap * wanted,
            elapsed,
            max_days,
            // only for an offensive planned again: a first offensive waits for its divisions
            // to reach their places on the line, however long they walk
            if state.war_rules.replanned.contains(army) {
                days(&cfg["planning_flat_days"], FLAT_GIVES_UP_DAYS)
            } else {
                f64::INFINITY
            },
            now,
        );
    }
    if elapsed >= need {
        note(
            state,
            &key,
            format!("{army} executes on state {target} after {elapsed:.1} game days of planning (estimated)"),
        );
        return None;
    }
    let why = format!(
        "{army} plans its offensive on state {target}: {elapsed:.1} of {need:.1} game days, about {:.0}% of the {:.0}% cap (estimated from +{:.1}%/day: no tool reads planning)",
        (elapsed * gain).min(cap) * 100.0,
        cap * 100.0,
        gain * 100.0
    );
    note(state, &key, why.clone());
    Some(why)
}

/// The planning wait on the bonus read from the army: execute at `wanted`, once the bonus has
/// not risen for a game day (it stands at the army's own cap) or after `max_days`.
/// Share of the wanted planning bonus from which a bonus that stopped rising is taken for the
/// army's own cap.
const FLAT_CAP_SHARE: f64 = 0.5;
/// Game days the bonus of an offensive planned again (a stopped plan, a rest) may stand still
/// below that level before the wait ends: divisions in contact,
/// on the move or out of position build no planning, and waiting for a bonus that does not come
/// only gives the enemy the time (live 15 Nov 2026: three armies stopped as stalled stood 16 days
/// at 0-5% and then went in as they were).
const FLAT_GIVES_UP_DAYS: f64 = 3.0;

#[allow(clippy::too_many_arguments)]
fn planning_read(
    state: &mut State,
    key: &str,
    army: &str,
    target: i32,
    bonus: f64,
    wanted: f64,
    elapsed: f64,
    max_days: f64,
    flat_limit: f64,
    now: GameHour,
) -> Option<String> {
    let seen = state
        .war_rules
        .planning
        .entry(army.to_string())
        .or_insert((bonus, now));
    if bonus > seen.0 + 0.001 {
        *seen = (bonus, now);
    }
    let flat_days = (now - seen.1) as f64 / DAY as f64;
    let pct = |v: f64| v * 100.0;
    let go = if bonus + 1e-9 >= wanted {
        Some(format!(
            "planning {:.0}% reached {:.0}%",
            pct(bonus),
            pct(wanted)
        ))
    } else if bonus + 1e-9 >= wanted * FLAT_CAP_SHARE && flat_days >= 1.0 {
        // A bonus that stands still below half of the wanted level is not the army's cap:
        // the divisions are still fighting or moving and have not begun to plan (after a
        // stopped offensive the read stood at 10-16% for a day and the attack went in again
        // two days after it was stopped, live 14 Nov 2026). The wait goes on, max_days bounds it.
        Some(format!(
            "planning stopped rising at {:.0}% for {flat_days:.1} game days (the army's cap)",
            pct(bonus)
        ))
    } else if flat_days >= flat_limit && elapsed >= flat_limit {
        Some(format!(
            "planning stands at {:.0}% for {flat_days:.1} game days: the divisions are not building it",
            pct(bonus)
        ))
    } else if elapsed >= max_days {
        Some(format!(
            "planning {:.0}% after the {max_days} game days at most",
            pct(bonus)
        ))
    } else {
        None
    };
    if let Some(go) = go {
        state.war_rules.planning.remove(army);
        note(
            state,
            key,
            format!("{army} executes on state {target}: {go} (read)"),
        );
        return None;
    }
    let why = format!(
        "{army} plans its offensive on state {target}: planning {:.0}% of {:.0}% wanted after {elapsed:.1} game days (read from hoi4_armies)",
        pct(bonus),
        pct(wanted)
    );
    note(state, key, why.clone());
    Some(why)
}

/// The army's target `old` fell to the player: a bound starts when the state holds an own supply
/// hub, port or railway. `mine` is hoi4_states for the player; `supply` hoi4_supply when read.
#[allow(clippy::too_many_arguments)]
pub(super) fn captured(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    army: &str,
    old: i32,
    mine: &Value,
    me: &str,
    supply: Option<&Value>,
    now: GameHour,
) {
    let cfg = &plan["war"];
    if cfg["bound"] == json!(false) {
        return;
    }
    let Some(row) = list(mine, "states")
        .iter()
        .find(|r| r["state_id"].as_i64() == Some(i64::from(old)))
    else {
        return;
    };
    if text(&row["controller"]) != me {
        return;
    }
    let core = text(&row["owner"]) == me;
    let provinces: HashSet<i64> = tools
        .call("hoi4_states", &json!({"query": old.to_string()}))
        .ok()
        .and_then(|v| list(&v, "states").first().cloned())
        .map(|r| {
            list(&r, "provinces")
                .iter()
                .filter_map(|p| p["province"].as_i64())
                .collect()
        })
        .unwrap_or_default();
    let read;
    let supply = match supply {
        Some(s) => s,
        None => {
            read = tools.call("hoi4_supply", &json!({})).unwrap_or(Value::Null);
            &read
        }
    };
    let in_state = |p: &Value| {
        p["province_id"]
            .as_i64()
            .is_some_and(|id| provinces.contains(&id))
    };
    let nodes: Vec<i64> = list(supply, "supply_nodes")
        .iter()
        .filter(|n| in_state(n))
        .filter_map(|n| n["province_id"].as_i64())
        .collect();
    let rail = list(supply, "supply_nodes")
        .iter()
        .chain(list(supply, "provinces"))
        .any(|p| {
            (in_state(p) || p["state_id"].as_i64() == Some(i64::from(old)))
                && p["railway_level"].as_i64().unwrap_or(0) > 0
        });
    if nodes.is_empty() && !rail {
        return;
    }
    note(
        state,
        &format!("bound:{army}"),
        format!(
            "{army} took state {old} ({} supply node(s), railway {rail}): holds its next advance until the node feeds the front",
            nodes.len()
        ),
    );
    state.war_rules.bound.insert(
        army.to_string(),
        Bound {
            state: old,
            since: now,
            nodes,
            rail,
            core,
        },
    );
}

/// Why the army's advance waits after a captured supply node (None: no bound, or it is over).
pub(super) fn bound_wait(
    state: &mut State,
    plan: &Value,
    army: &str,
    supply: Option<&Value>,
    supply_mean: Option<f64>,
    now: GameHour,
) -> Option<String> {
    let b = state.war_rules.bound.get(army)?.clone();
    let cfg = &plan["war"];
    let key = format!("bound:{army}");
    let elapsed = (now - b.since) as f64 / DAY as f64;
    let max = days(&cfg["bound_max_days"], 12.0);
    let min = if b.core {
        days(&cfg["bound_core_days"], 5.0)
    } else {
        days(&cfg["bound_rail_days"], 10.0)
    };
    let connected = !b.nodes.is_empty()
        && supply.is_some_and(|s| {
            list(s, "provinces").iter().any(|p| {
                list(p, "flows").iter().any(|f| {
                    f["flow"].as_f64().unwrap_or(0.0) > 0.0
                        && f["node_province"]
                            .as_i64()
                            .is_some_and(|n| b.nodes.contains(&n))
                })
            })
        });
    let floor = Limits::of(plan).floor();
    let supplied = supply_mean.is_none_or(|s| s >= floor);
    if elapsed >= max || ((connected || elapsed >= min) && supplied) {
        state.war_rules.bound.remove(army);
        note(
            state,
            &key,
            format!(
                "{army} advances again {elapsed:.1} days after taking state {} (node connected {connected}, supply {})",
                b.state,
                supply_mean.map_or("unknown".into(), |s| format!("{s:.0}%"))
            ),
        );
        return None;
    }
    let why = format!(
        "{army} holds after taking state {} (supply node {}, railway {}): {elapsed:.1} of {min}-{max} days, node connected {connected}, supply {}",
        b.state,
        !b.nodes.is_empty(),
        b.rail,
        supply_mean.map_or("unknown".into(), |s| format!("{s:.0}%"))
    );
    note(state, &key, why.clone());
    Some(why)
}

// ---------------------------------------------------------------------------------------------
// Pocket fall-back.

/// Whether the army falls back from a forming pocket: the game-side flag (70%) anywhere, else
/// `war.pocket_share` (0.5) of the land around it held by others while it stands outside the
/// fully held own states (`safe`).
pub(super) fn pocket_risk(plan: &Value, army: &Value, safe: &[(i32, (f64, f64))]) -> bool {
    let threat = &army["threat"];
    // The share alone describes a front as well as a pocket: an army on a line of contact has
    // about half of the land around it held by others, and one advancing into enemy land far
    // more. What makes a pocket is the way back: with the counts read, the army falls back
    // only when at most POCKET_NECK land provinces around it are still the player's.
    if let (Some(around), Some(others)) = (
        threat["neighbour_provinces"].as_u64(),
        threat["held_by_others"].as_u64(),
    ) {
        if around.saturating_sub(others) > POCKET_NECK {
            return false;
        }
    }
    if threat["encirclement_risk"] == json!(true) {
        return true;
    }
    let Some(held) = threat["share"].as_f64() else {
        return false;
    };
    let limit = share(&plan["war"]["pocket_share"], 0.5);
    let here = army["main_state"].as_i64();
    held >= limit && !safe.iter().any(|(id, _)| Some(i64::from(*id)) == here)
}

/// Own land provinces around an army at or below which a high share of foreign land is a
/// pocket and not a front.
const POCKET_NECK: u64 = 1;

// ---------------------------------------------------------------------------------------------
// The stall response: a second direction before the direct move.

/// What a running move to another side of the target means for the war step this tick.
pub(super) enum Relocate {
    /// Nothing running.
    No,
    /// On its way: no other order.
    Wait,
    /// An order was sent.
    Acted(String),
}

/// The move to another side of the target (after the plan was stopped), then the wait for it.
#[allow(clippy::too_many_arguments)]
pub(super) fn relocation(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    army: &str,
    target: i32,
    here: Option<i32>,
    stall: GameHour,
    now: GameHour,
) -> Result<Relocate> {
    let Some(v) = state.war_rules.via.get(army).copied() else {
        return Ok(Relocate::No);
    };
    if v.target != target {
        state.war_rules.via.remove(army);
        return Ok(Relocate::No);
    }
    if !v.moving {
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_move",
            json!({"army": army, "state_id": v.state}),
            "w2",
        )?;
        if ok {
            state.war_rules.via.insert(
                army.to_string(),
                Via {
                    moving: true,
                    at: now,
                    ..v
                },
            );
            state.war_stage.insert(army.to_string(), 0);
            state.war_move.remove(army);
        } else {
            state.war_rules.via.remove(army);
        }
        return Ok(Relocate::Acted(format!(
            "war second direction {army} -> state {} beside state {target}: {ok} {why}",
            v.state
        )));
    }
    if here == Some(v.state) {
        state.war_rules.via.remove(army);
        state.war_stage.insert(army.to_string(), 0);
        state.war_move.remove(army);
        state
            .war_since
            .insert(army.to_string(), (target, here, now));
        note(
            state,
            &format!("stall:{army}"),
            format!("{army} reached state {} beside state {target}: front and offensive are drawn from there", v.state),
        );
        return Ok(Relocate::No);
    }
    if now - v.at > 2 * stall.max(DAY) {
        state.war_rules.via.remove(army);
        return Ok(Relocate::No);
    }
    Ok(Relocate::Wait)
}

/// The first stall on a target: an idle army beside the target joins the attack, else the
/// stalled army goes round to another own state beside it. None: no second direction (tried
/// before, switched off or nothing beside the target): the war step sends the direct move.
#[allow(clippy::too_many_arguments)]
pub(super) fn second_direction(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    roster: &Value,
    army: &str,
    target: i32,
    here: Option<i32>,
    executing: bool,
    flanks: &HashSet<String>,
    enemies: &[String],
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["war"];
    if cfg["second_direction"] == json!(false)
        || cfg["direct_moves"] == json!(true)
        || state.war_rules.second.get(army) == Some(&target)
    {
        return Ok(None);
    }
    state.war_rules.second.insert(army.to_string(), target);
    let key = format!("stall:{army}");
    let mut args = json!({"max_states": 1100});
    let tags: Vec<&String> = enemies
        .iter()
        .filter(|t| {
            t.len() == 3
                && t.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
        .take(32)
        .collect();
    if !tags.is_empty() {
        args["enemies"] = json!(tags);
    }
    let beside: Vec<i32> = tools
        .call("hoi4_wars", &args)
        .ok()
        .and_then(|w| {
            list(&w, "enemy_held_adjacent_states")
                .iter()
                .find(|s| s["state_id"].as_i64() == Some(i64::from(target)))
                .map(|s| {
                    list(s, "adjacent_own_states")
                        .iter()
                        .filter_map(|v| v.as_i64().map(|v| v as i32))
                        .collect()
                })
        })
        .unwrap_or_default();
    if beside.is_empty() {
        note(
            state,
            &key,
            format!("{army} stalled on state {target}: no own state beside it is known (hoi4_wars): direct move"),
        );
        return Ok(None);
    }
    // 1. An idle army beside the target attacks it too.
    let flank = list(roster, "armies")
        .iter()
        .filter(|a| {
            let n = text(&a["army"]);
            n != army
                && flanks.contains(n)
                && a["plan_executing"] != json!(true)
                && !state.war_move.contains(n)
                && !state.war_rest.contains_key(n)
                && state.war_assign.get(n) != Some(&target)
                && a["main_state"]
                    .as_i64()
                    .is_some_and(|s| s as i32 != here.unwrap_or(-1) && beside.contains(&(s as i32)))
        })
        .max_by_key(|a| a["division_count"].as_u64().unwrap_or(0))
        .map(|a| {
            (
                text(&a["army"]).to_string(),
                a["main_state"].as_i64().unwrap_or(0),
            )
        });
    if let Some((other, from)) = flank {
        state.war_assign.insert(other.clone(), target);
        state.war_stage.remove(&other);
        state.war_move.remove(&other);
        state.war_since.remove(&other);
        state.war_rest.remove(&other);
        state.war_cool.remove(&other);
        state
            .war_since
            .insert(army.to_string(), (target, here, now));
        let line = format!(
            "war stalled {army} on state {target}: second direction, {other} attacks it from state {from}"
        );
        note(state, &key, line.clone());
        return Ok(Some(line));
    }
    // 2. Round to another own state beside the target.
    let position = here.and_then(|s| state.centers.get(&s).copied());
    let Some(via) = beside
        .iter()
        .copied()
        .filter(|s| Some(*s) != here)
        .min_by(|a, b| {
            let d = |s: &i32| {
                let (x, z) = state.centers.get(s).copied().unwrap_or((f64::MAX, 0.0));
                position.map_or(0.0, |(px, pz)| (x - px).powi(2) + (z - pz).powi(2))
            };
            d(a).total_cmp(&d(b))
        })
    else {
        note(
            state,
            &key,
            format!("{army} stalled on state {target}: no other own state beside it: direct move"),
        );
        return Ok(None);
    };
    if executing {
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_plan",
            json!({"army": army, "execute": false}),
            "w2",
        )?;
        if ok {
            state.war_rules.via.insert(
                army.to_string(),
                Via {
                    target,
                    state: via,
                    moving: false,
                    at: now,
                },
            );
        }
        let line = format!(
            "war stalled {army} on state {target}: plan stopped to go round by state {via}: {ok} {why}"
        );
        note(state, &key, line.clone());
        return Ok(Some(line));
    }
    state.war_rules.via.insert(
        army.to_string(),
        Via {
            target,
            state: via,
            moving: false,
            at: now,
        },
    );
    match relocation(state, tools, lease, army, target, here, DAY, now)? {
        Relocate::Acted(line) => {
            note(state, &key, line.clone());
            Ok(Some(line))
        }
        _ => Ok(None),
    }
}

// ---------------------------------------------------------------------------------------------
// The disband guard.

/// Own states the capital reaches over own-controlled land (hoi4_wars `capital_land.states`);
/// None when the read does not say.
pub(super) fn home_land(state: &mut State, tools: &dyn Tools) -> Option<HashSet<i64>> {
    let read = tools.call("hoi4_wars", &json!({})).ok();
    let states = read
        .as_ref()
        .and_then(|r| r["capital_land"]["states"].as_array());
    match states {
        Some(s) => Some(s.iter().filter_map(Value::as_i64).collect()),
        None => {
            let why = read
                .as_ref()
                .and_then(|r| r["capital_land_reason"].as_str())
                .unwrap_or("hoi4_wars gave no capital_land");
            note(
                state,
                "disband",
                format!("disband waits: the land route to the capital is unknown ({why})"),
            );
            None
        }
    }
}

/// Armies at encirclement risk (hoi4_armies threat): never thinned by disbanding.
pub(super) fn encircled(tools: &dyn Tools) -> HashSet<String> {
    tools
        .call("hoi4_armies", &json!({}))
        .map(|r| {
            list(&r, "armies")
                .iter()
                .filter(|a| a["threat"]["encirclement_risk"] == json!(true))
                .map(|a| text(&a["army"]).to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// The disband step's note on divisions it left because they stand overseas.
pub(super) fn overseas_note(state: &mut State, overseas: &[String]) {
    if !overseas.is_empty() {
        note(
            state,
            "disband",
            format!(
                "kept {} division(s) the capital does not reach by land (75% of their equipment would be lost): {}",
                overseas.len(),
                overseas.join(", ")
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::super::{tick, Tick};
    use super::*;
    use std::cell::{Cell, RefCell};

    const START: GameHour = 1_000_000;

    /// One driven army (Army 1 in own state 42) against FRA (states 28 and 18); Army 2 idles in
    /// own state 43 beside 28. Supply, organisation, the executing flag and the threat are set
    /// by the test.
    struct W {
        executing: Cell<bool>,
        /// battle plan orders of Army 1 (front and offensive line: 2)
        orders: Cell<u64>,
        org: Cell<f64>,
        supply: Cell<f64>,
        share: Cell<f64>,
        here: Cell<i64>,
        taken: Cell<bool>,
        flow: Cell<f64>,
        flank: Cell<bool>,
        /// Army 1's planning bonus as hoi4_armies reads it (None: not read)
        planning: Cell<Option<f64>>,
        /// the game refuses an execution mode order (hoi4_army_plan mode)
        refuse_mode: Cell<bool>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl W {
        fn new() -> Self {
            Self {
                executing: Cell::new(false),
                orders: Cell::new(2),
                org: Cell::new(50.0),
                supply: Cell::new(90.0),
                share: Cell::new(0.2),
                here: Cell::new(42),
                taken: Cell::new(false),
                flow: Cell::new(0.0),
                flank: Cell::new(false),
                planning: Cell::new(None),
                refuse_mode: Cell::new(false),
                calls: RefCell::new(vec![]),
            }
        }
        fn actions(&self) -> Vec<(String, Value)> {
            self.calls
                .borrow()
                .iter()
                .filter(|(_, a)| a.get("request_id").is_some())
                .cloned()
                .collect()
        }
    }
    impl Tools for W {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                if self.refuse_mode.get() && args.get("mode").is_some() {
                    return Err(super::super::Error::new(
                        "input_readback_failed",
                        "Execution mode rush of Army 1 is not verified",
                    ));
                }
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            let taken = self.taken.get();
            Ok(match name {
                "hoi4_research" => json!({"slots": [{"researching": "a"}]}),
                "hoi4_focus_completed" => json!({"active_key": "f"}),
                "hoi4_economy" => json!({"factory_trigger_count": {"military": 0}}),
                "hoi4_overview" => json!({"player": "GER"}),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "GER" => {
                        let mut rows = vec![
                            json!({"state_id": 42, "owner": "GER", "controller": "GER", "x": 0.0, "z": 0.0}),
                            json!({"state_id": 43, "owner": "GER", "controller": "GER", "x": 20.0, "z": 30.0}),
                            json!({"state_id": 44, "owner": "GER", "controller": "GER", "x": 30.0, "z": -10.0}),
                        ];
                        if taken {
                            rows.push(json!({"state_id": 28, "owner": "FRA", "controller": "GER", "x": 10.0, "z": 10.0}));
                        }
                        json!({"states": rows})
                    }
                    "FRA" => {
                        let mut rows = vec![
                            json!({"state_id": 18, "owner": "FRA", "controller": "FRA", "x": 50.0, "z": 50.0}),
                        ];
                        if !taken {
                            rows.push(json!({"state_id": 28, "owner": "FRA", "controller": "FRA", "x": 10.0, "z": 10.0}));
                        }
                        json!({"states": rows})
                    }
                    "28" => json!({"states": [{"state_id": 28, "provinces": [
                        {"province": 280, "controller": "GER"}, {"province": 281, "controller": "GER"}]}]}),
                    // a single state (the province count of a progress measurement): no rows
                    other if other.parse::<i32>().is_ok() => json!({"states": []}),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_armies" => {
                    let mut armies = vec![json!({"army": "Army 1", "division_count": 10,
                        "main_state": self.here.get(), "battle_plan_orders": self.orders.get(),
                        "plan_executing": self.executing.get(), "commander": "X",
                        "threat": {"share": self.share.get(), "encirclement_risk": self.share.get() >= 0.7},
                        "planning": self.planning.get().map(|p| json!({"mean": p, "min": p, "max": p}))})];
                    if self.flank.get() {
                        armies.push(
                            json!({"army": "Army 2", "division_count": 8, "main_state": 43,
                            "battle_plan_orders": 0, "plan_executing": false, "commander": "Y",
                            "threat": {"share": 0.1, "encirclement_risk": false}}),
                        );
                    }
                    json!({"unassigned_by_state": [], "armies": armies})
                }
                "hoi4_supply" => json!({
                    "armies": [{"army": "Army 1", "organisation_mean": self.org.get(),
                        "supply_percent_mean": self.supply.get()},
                        {"army": "Army 2", "organisation_mean": 50.0, "supply_percent_mean": 90.0}],
                    "supply_nodes": if taken { json!([{"node": 9, "kind": "hub", "province_id": 281,
                        "railway_level": 1}]) } else { json!([]) },
                    "provinces": [{"province_id": 280, "state_id": 28, "railway_level": 1,
                        "flows": [{"node": 9, "flow": self.flow.get(), "node_province": 281}]}]}),
                "hoi4_wars" => json!({"enemy_held_adjacent_states": [
                    {"state_id": 28, "adjacent_own_states": [42, 43, 44]}]}),
                _ => json!({}),
            })
        }
    }
    fn plan(extra: Value) -> Value {
        // "replans": 0: these tests are about what follows the planning cycles of a stall
        let mut war =
            json!({"enemies": ["FRA"], "axes": [[28, 18]], "cooldown_hours": 1, "replans": 0});
        for (k, v) in extra.as_object().unwrap() {
            war[k] = v.clone();
        }
        json!({"war": war})
    }
    fn acted(step: Tick) -> String {
        match step {
            Tick::Acted(s) => s,
            other => panic!("expected an action, got {other:?}"),
        }
    }

    #[test]
    fn an_army_below_the_supply_floor_gets_a_railway_then_trucks_then_a_report() {
        struct R(RefCell<Vec<(String, Value)>>);
        impl Tools for R {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                Ok(json!({"receipt": {"status": "Verified"}}))
            }
        }
        let read = json!({"divisions": [
            {"army": "S", "province_id": 2020, "state_id": 559, "supply_percent": 12.0},
            {"army": "S", "province_id": 2020, "state_id": 559, "supply_percent": 15.0},
            {"army": "S", "province_id": 12991, "state_id": 559, "supply_percent": 100.0},
            {"army": "N", "province_id": 13238, "state_id": 842, "supply_percent": 5.0}]});
        let army = json!({"army": "S", "main_state": 559, "motorization": 1});
        let plan = plan(json!({}));
        let limits = Limits::of(&plan);
        let tools = R(RefCell::new(vec![]));
        let mut state = State::default();
        let relief = |state: &mut State, supply: f64, at: GameHour| {
            supply_relief(
                state,
                &tools,
                "l",
                &plan,
                &army,
                Some(&read),
                supply,
                &limits,
                at,
            )
            .unwrap()
        };
        // just fell below the floor: nothing yet
        assert_eq!(relief(&mut state, 14.0, START), None);
        let line = relief(&mut state, 14.0, START + 2 * DAY).unwrap();
        assert!(line.contains("railway"), "{line}");
        let line = relief(&mut state, 14.0, START + 2 * DAY + 1).unwrap();
        assert!(line.contains("motorization 1 -> 2"), "{line}");
        // the railway to the same province waits 30 days, the trucks were raised once
        assert_eq!(relief(&mut state, 14.0, START + 5 * DAY), None);
        let sent = tools.0.borrow().clone();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].0, "hoi4_railway_build");
        assert_eq!(sent[0].1["to_province"], 2020);
        assert_eq!(sent[0].1["to_nearest_node"], true);
        assert_eq!(sent[0].1["from_nearest_node"], true);
        assert_eq!(sent[1].1["level"], 2);
        // ten days below the floor: reported once, with what was tried
        assert_eq!(relief(&mut state, 14.0, START + 10 * DAY), None);
        assert_eq!(relief(&mut state, 14.0, START + 11 * DAY), None);
        let news: Vec<&Value> = state
            .strategy
            .news
            .iter()
            .filter(|n| n["kind"] == crate::watch::ARMY_OUT_OF_SUPPLY)
            .collect();
        assert_eq!(news.len(), 1);
        assert!(news[0]["detail"]["levers"]
            .as_str()
            .unwrap()
            .contains("railway"));
        // back at the floor: the spell ends; a new spell starts its own count
        assert_eq!(relief(&mut state, 60.0, START + 12 * DAY), None);
        assert!(!state.war_rules.starved.contains_key("S"));
    }

    #[test]
    fn the_execute_order_waits_for_the_planning_bonus() {
        let w = W::new();
        let plan = plan(json!({}));
        let mut state = State::default();
        // the front and the offensive line are one step: the planning starts with them
        let step = acted(tick(&mut state, &w, &plan, "l", START).unwrap());
        assert!(
            step.starts_with("war front and offensive Army 1 -> state 28"),
            "{step}"
        );
        // 0.9 x 30% at +2%/day: 13.5 game days
        for day in 1..13 {
            assert_eq!(
                tick(&mut state, &w, &plan, "l", START + day * DAY).unwrap(),
                Tick::Nothing
            );
        }
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("no tool reads planning"), "{line}");
        let step = acted(tick(&mut state, &w, &plan, "l", START + 14 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        // switched off: the next tick after the lines, like before
        let mut state = State::default();
        let off = plan_with(json!({"planning_wait": false}));
        tick(&mut state, &w, &off, "l", START).unwrap();
        assert!(
            acted(tick(&mut state, &w, &off, "l", START + 3).unwrap()).starts_with("war execute")
        );
    }
    #[test]
    fn the_planning_read_decides_the_execute_order() {
        let w = W::new();
        let plan = plan(json!({}));
        let mut state = State::default();
        w.planning.set(Some(0.0));
        tick(&mut state, &w, &plan, "l", START).unwrap();
        tick(&mut state, &w, &plan, "l", START + 3).unwrap();
        // rising below 0.9 x 30%: waits, and says it was read
        for (day, bonus) in [(1, 0.05), (2, 0.1), (3, 0.15)] {
            w.planning.set(Some(bonus));
            assert_eq!(
                tick(&mut state, &w, &plan, "l", START + 3 + day * DAY).unwrap(),
                Tick::Nothing
            );
        }
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("read from hoi4_armies"), "{line}");
        // reached 27%: executes on day 4, long before the 13.5-day estimate
        w.planning.set(Some(0.27));
        let step = acted(tick(&mut state, &w, &plan, "l", START + 3 + 4 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        assert!(state.war_rules.notes["planning:Army 1"].contains("reached"));
    }

    #[test]
    fn a_planning_bonus_that_stops_rising_is_at_the_armys_cap() {
        let w = W::new();
        let plan = plan(json!({}));
        let mut state = State::default();
        tick(&mut state, &w, &plan, "l", START).unwrap();
        tick(&mut state, &w, &plan, "l", START + 3).unwrap();
        w.planning.set(Some(0.2));
        assert_eq!(
            tick(&mut state, &w, &plan, "l", START + 3 + DAY).unwrap(),
            Tick::Nothing
        );
        // the same 20% a game day later: the cap (a low-planning army), execute
        let step = acted(tick(&mut state, &w, &plan, "l", START + 3 + 2 * DAY + 1).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        assert!(state.war_rules.notes["planning:Army 1"].contains("stopped rising"));
    }

    #[test]
    fn a_low_bonus_that_stands_still_is_not_the_cap_and_the_wait_goes_on() {
        let w = W::new();
        let plan = plan(json!({}));
        let mut state = State::default();
        tick(&mut state, &w, &plan, "l", START).unwrap();
        tick(&mut state, &w, &plan, "l", START + 3).unwrap();
        // 10% for days: divisions still fighting after a stopped offensive, not a cap
        w.planning.set(Some(0.10));
        for day in 1..5 {
            assert_eq!(
                tick(&mut state, &w, &plan, "l", START + 3 + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        // from half of the wanted 27% a flat bonus is the cap
        w.planning.set(Some(0.14));
        tick(&mut state, &w, &plan, "l", START + 3 + 5 * DAY).unwrap();
        let step = acted(tick(&mut state, &w, &plan, "l", START + 3 + 6 * DAY + 1).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
    }

    fn plan_with(extra: Value) -> Value {
        plan(extra)
    }

    #[test]
    fn a_near_deadline_and_a_short_max_skip_the_long_wait() {
        let w = W::new();
        let mut p = plan(json!({"planning_max_days": 2}));
        let mut state = State::default();
        // both lines are drawn in the first step: the two days count from it
        tick(&mut state, &w, &p, "l", START).unwrap();
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + DAY).unwrap(),
            Tick::Nothing
        );
        assert!(
            acted(tick(&mut state, &w, &p, "l", START + 2 * DAY).unwrap())
                .starts_with("war execute")
        );
        // a goal deadline ten days away: no wait at all
        p["goal"] = json!({"deadline": "1936.01.10"});
        let now = super::super::date_hour("1936.01.01").unwrap();
        let mut state = State::default();
        tick(&mut state, &w, &p, "l", now).unwrap();
        assert!(acted(tick(&mut state, &w, &p, "l", now + 3).unwrap()).starts_with("war execute"));
    }

    #[test]
    fn supply_below_the_floor_rests_and_below_the_hold_nothing_resumes() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false, "rest_max_days": 10}));
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        w.supply.set(45.0);
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(
            step.starts_with("war rest Army 1") && step.contains("supply 45%"),
            "{step}"
        );
        w.executing.set(false);
        // below the hold: not even after rest_max_days
        w.supply.set(30.0);
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 11 * DAY).unwrap(),
            Tick::Nothing
        );
        // between hold and resume: rest_max_days resumes
        w.supply.set(40.0);
        let step = acted(tick(&mut state, &w, &p, "l", START + 12 * DAY).unwrap());
        assert!(step.starts_with("war resume Army 1"), "{step}");
        // supply 60 and fresh: resumes before rest_max (70 needed): rest again first
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        w.supply.set(45.0);
        tick(&mut state, &w, &p, "l", START).unwrap();
        w.executing.set(false);
        w.supply.set(60.0);
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + DAY).unwrap(),
            Tick::Nothing
        );
        w.supply.set(75.0);
        assert!(
            acted(tick(&mut state, &w, &p, "l", START + 2 * DAY).unwrap())
                .starts_with("war resume Army 1")
        );
    }

    #[test]
    fn a_starving_army_draws_its_lines_but_gets_no_execute_order_and_no_move() {
        let w = W::new();
        w.supply.set(45.0);
        // no planning wait: only the supply floor stands between the lines and the execute order
        let p = plan(json!({"planning_wait": false}));
        let mut state = State::default();
        // drawing lines is not an attack: the front and the offensive line go out below the floor
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(
            step.starts_with("war front and offensive Army 1 -> state 28"),
            "{step}"
        );
        // the execute order is what the floor holds back
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + DAY).unwrap(),
            Tick::Nothing
        );
        let sent: Vec<String> = w.actions().into_iter().map(|(tool, _)| tool).collect();
        assert_eq!(sent, ["hoi4_army_front", "hoi4_army_offensive"]);
        assert!(state.war_rules.notes["hold:Army 1"].contains("supply 45%"));
        w.supply.set(55.0);
        let step = acted(tick(&mut state, &w, &p, "l", START + 2 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        // with direct moves the army has no line to draw: below the floor it gets no move
        let w = W::new();
        w.supply.set(45.0);
        let direct = plan(json!({"direct_moves": true}));
        let mut state = State::default();
        assert_eq!(
            tick(&mut state, &w, &direct, "l", START).unwrap(),
            Tick::Nothing
        );
        assert!(w.actions().is_empty(), "{:?}", w.actions());
    }

    #[test]
    fn rest_and_resume_follow_the_share_of_the_highest_organisation_seen() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false}));
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        w.org.set(60.0);
        assert_eq!(tick(&mut state, &w, &p, "l", START).unwrap(), Tick::Nothing);
        // 17 < 0.3 x 60
        w.org.set(17.0);
        assert!(acted(tick(&mut state, &w, &p, "l", START + DAY).unwrap())
            .starts_with("war rest Army 1"));
        w.executing.set(false);
        // 45 < 0.8 x 60 = 48
        w.org.set(45.0);
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 2 * DAY).unwrap(),
            Tick::Nothing
        );
        w.org.set(49.0);
        assert!(
            acted(tick(&mut state, &w, &p, "l", START + 3 * DAY).unwrap())
                .starts_with("war resume Army 1")
        );
        assert!(state.war_rules.notes["rest"].contains("highest organisation_mean"));
    }

    #[test]
    fn half_the_land_held_by_others_falls_back_only_outside_safe_own_land() {
        let w = W::new();
        w.share.set(0.55);
        let p = plan(json!({}));
        // standing in a fully held own state: a normal front, no fall-back
        let mut state = State::default();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(step.starts_with("war front"), "{step}");
        // in enemy land at 55%: back to the nearest safe state
        w.here.set(28);
        let mut state = State::default();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(step.starts_with("war pocket Army 1: 55%"), "{step}");
        // the old threshold by key
        let old = plan(json!({"pocket_share": 0.7}));
        let mut state = State::default();
        assert!(!acted(tick(&mut state, &w, &old, "l", START).unwrap()).starts_with("war pocket"));
    }

    /// Army 1 with its front and offensive line drawn toward state 28, before its execute order.
    fn lined() -> State {
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 2);
        state
    }
    /// The execution mode orders sent: (army, mode); none of them carries an execute flag.
    fn mode_orders(w: &W) -> Vec<(String, String)> {
        w.actions()
            .iter()
            .filter(|(n, a)| n == "hoi4_army_plan" && a.get("mode").is_some())
            .map(|(_, a)| {
                assert!(a.get("execute").is_none(), "{a}");
                (text(&a["army"]).to_string(), text(&a["mode"]).to_string())
            })
            .collect()
    }

    #[test]
    fn the_plans_execution_mode_is_set_once_per_army_before_its_first_execute() {
        let w = W::new();
        let p = plan(json!({"execution": "rush", "planning_wait": false}));
        let mut state = lined();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert_eq!(step, "war execution Army 1 rush: true Verified");
        // then the execute order, and the mode is never sent again
        let step = acted(tick(&mut state, &w, &p, "l", START + 2).unwrap());
        assert!(step.starts_with("war execute Army 1 -> state 28"), "{step}");
        w.executing.set(true);
        for day in 1..=6 {
            assert_eq!(
                tick(&mut state, &w, &p, "l", START + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        assert_eq!(
            mode_orders(&w),
            [("Army 1".to_string(), "rush".to_string())]
        );
        let plans: Vec<bool> = w
            .actions()
            .iter()
            .filter(|(n, _)| n == "hoi4_army_plan")
            .map(|(_, a)| a.get("mode").is_some())
            .collect();
        assert_eq!(
            plans,
            [true, false],
            "the mode goes out before the execute order"
        );
        // the plan names another mode: it is sent once
        let careful = plan(json!({"execution": "careful", "planning_wait": false}));
        let step = acted(tick(&mut state, &w, &careful, "l", START + 7 * DAY).unwrap());
        assert_eq!(step, "war execution Army 1 careful: true Verified");
        assert_eq!(
            tick(&mut state, &w, &careful, "l", START + 8 * DAY).unwrap(),
            Tick::Nothing
        );
        // without the key, or with a word that is no mode, nothing is sent: the game's default
        for war in [
            json!({"planning_wait": false}),
            json!({"planning_wait": false, "execution": "fast"}),
        ] {
            let w = W::new();
            let p = plan(war);
            let mut state = lined();
            let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
            assert!(step.starts_with("war execute Army 1 -> state 28"), "{step}");
            assert!(mode_orders(&w).is_empty());
        }
    }

    #[test]
    fn an_army_without_its_lines_gets_them_before_its_execution_mode() {
        let w = W::new();
        w.orders.set(0);
        let p = plan(json!({"execution": "rush"}));
        let mut state = State::default();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(
            step.starts_with("war front and offensive Army 1 -> state 28"),
            "{step}"
        );
        assert!(mode_orders(&w).is_empty());
        let step = acted(tick(&mut state, &w, &p, "l", START + 3).unwrap());
        assert_eq!(step, "war execution Army 1 rush: true Verified");
    }

    #[test]
    fn a_refused_execution_mode_never_holds_the_execute_order_and_is_tried_again() {
        let w = W::new();
        w.refuse_mode.set(true);
        let p = plan(json!({"execution": "rush", "planning_wait": false}));
        let mut state = lined();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(
            step.starts_with("war execution Army 1 rush: false input_readback_failed"),
            "{step}"
        );
        // the next tick executes: the failed mode order is left alone for three game days
        let step = acted(tick(&mut state, &w, &p, "l", START + 2).unwrap());
        assert!(step.starts_with("war execute Army 1 -> state 28"), "{step}");
        w.executing.set(true);
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 2 * DAY).unwrap(),
            Tick::Nothing
        );
        assert_eq!(mode_orders(&w).len(), 1);
        // then it is tried again, and once it works it is not sent any more
        w.refuse_mode.set(false);
        let step = acted(tick(&mut state, &w, &p, "l", START + 3 * DAY + 1).unwrap());
        assert_eq!(step, "war execution Army 1 rush: true Verified");
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 4 * DAY).unwrap(),
            Tick::Nothing
        );
        assert_eq!(mode_orders(&w).len(), 2);
    }

    #[test]
    fn the_execution_mode_follows_a_rename_survives_a_restart_and_is_sent_to_a_new_army() {
        let w = W::new();
        let p = plan(json!({"execution": "careful", "planning_wait": false}));
        let mut state = lined();
        state.clock = START;
        let sent = execution_step(&mut state, &w, "l", &p, "Army 1", START).unwrap();
        assert_eq!(
            sent.as_deref(),
            Some("war execution Army 1 careful: true Verified")
        );
        assert_eq!(
            execution_step(&mut state, &w, "l", &p, "Army 1", START).unwrap(),
            None
        );
        // an army formed later has not been told
        let sent = execution_step(&mut state, &w, "l", &p, "Army 2", START).unwrap();
        assert_eq!(
            sent.as_deref(),
            Some("war execution Army 2 careful: true Verified")
        );
        // the memory follows a new name
        state.army_renamed("Army 1", "North");
        assert_eq!(
            execution_step(&mut state, &w, "l", &p, "North", START).unwrap(),
            None
        );
        state.war_rules.rushed.insert("Army 2".into());
        // plan_state.json keeps it: after a restart nothing is sent again
        let saved = state.memory(START);
        assert_eq!(
            saved["war"]["rules"]["execution"],
            json!({"North": "careful", "Army 2": "careful"})
        );
        assert_eq!(saved["war"]["rules"]["rushed"], json!(["Army 2"]));
        let mut again = State::default();
        again.restore(&saved, START);
        again.clock = START;
        assert_eq!(
            execution_step(&mut again, &w, "l", &p, "North", START).unwrap(),
            None
        );
        assert!(again.war_rules.rushed.contains("Army 2"));
        assert_eq!(mode_orders(&w).len(), 2);
        // an army that no longer exists is forgotten: a new army under its name is told again
        let present: HashSet<String> = ["North".to_string()].into_iter().collect();
        forget_absent(&mut again, &present);
        assert!(again.war_rules.rushed.is_empty());
        let sent = execution_step(&mut again, &w, "l", &p, "Army 2", START).unwrap();
        assert!(sent.is_some());
    }

    #[test]
    fn a_stall_never_sets_rush_unless_the_plan_asks_for_it() {
        // absent and false alike: the stall goes straight to the answers there were
        for war in [
            json!({"replans": 1}),
            json!({"replans": 1, "stall_rush": false}),
        ] {
            let w = W::new();
            let p = plan(war);
            let (mut state, now) = stalled(&w, &p);
            let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
            assert!(
                step.starts_with(
                    "war stalled Army 1 8 days on state 28: plan stopped to plan again (1 of 1): true"
                ),
                "{step}"
            );
            assert!(mode_orders(&w).is_empty());
        }
    }

    #[test]
    fn with_stall_rush_the_first_answer_to_a_stalled_executing_plan_is_the_rush_mode() {
        let w = W::new();
        let p = plan(json!({"replans": 1, "stall_rush": true}));
        let (mut state, now) = stalled(&w, &p);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert_eq!(
            step,
            "war stalled Army 1 8 days on state 28: execution set to rush: true Verified"
        );
        // the plan was not stopped: one mode order, no execute flag
        assert_eq!(
            mode_orders(&w),
            [("Army 1".to_string(), "rush".to_string())]
        );
        assert_eq!(w.actions().len(), 1);
        // the stall clock starts again: nothing for another stall time
        for day in 1..8 {
            assert_eq!(
                tick(&mut state, &w, &p, "l", now + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        // the next stall goes on to the answers there were: the plan stops to plan again
        let step = acted(tick(&mut state, &w, &p, "l", now + 8 * DAY).unwrap());
        assert!(
            step.starts_with(
                "war stalled Army 1 8 days on state 28: plan stopped to plan again (1 of 1): true"
            ),
            "{step}"
        );
        assert_eq!(mode_orders(&w).len(), 1);
    }

    #[test]
    fn a_plan_already_on_rush_and_a_refused_rush_go_on_to_the_other_answers() {
        // `war.execution` is rush already (set when the lines were drawn): the stall stops the plan
        let w = W::new();
        let p = plan(json!({"replans": 1, "stall_rush": true, "execution": "rush"}));
        let mut state = State::default();
        state.war_rules.exec.insert("Army 1".into(), "rush".into());
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        assert_eq!(tick(&mut state, &w, &p, "l", START).unwrap(), Tick::Nothing);
        let step = acted(tick(&mut state, &w, &p, "l", START + 8 * DAY).unwrap());
        assert!(
            step.contains("plan stopped to plan again (1 of 1)"),
            "{step}"
        );
        assert!(mode_orders(&w).is_empty());
        // a rush order the game refuses: reported, and the next tick stops the plan
        let w = W::new();
        w.refuse_mode.set(true);
        let p = plan(json!({"replans": 1, "stall_rush": true}));
        let (mut state, now) = stalled(&w, &p);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(
            step.starts_with(
                "war stalled Army 1 8 days on state 28: execution set to rush: false input_readback_failed"
            ),
            "{step}"
        );
        let step = acted(tick(&mut state, &w, &p, "l", now + 2).unwrap());
        assert!(
            step.contains("plan stopped to plan again (1 of 1)"),
            "{step}"
        );
        assert_eq!(mode_orders(&w).len(), 1);
    }

    fn stalled(w: &W, p: &Value) -> (State, GameHour) {
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        assert_eq!(tick(&mut state, w, p, "l", START).unwrap(), Tick::Nothing);
        (state, START + 8 * DAY)
    }

    #[test]
    fn a_stalled_offensive_is_planned_again_before_anything_else() {
        let w = W::new();
        let p = plan(json!({"replans": 1}));
        let (mut state, now) = stalled(&w, &p);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(
            step.starts_with(
                "war stalled Army 1 8 days on state 28: plan stopped to plan again (1 of 1): true"
            ),
            "{step}"
        );
        let (tool, args) = w.actions().last().unwrap().clone();
        assert_eq!(
            (tool.as_str(), &args["execute"]),
            ("hoi4_army_plan", &json!(false))
        );
        // back before the execute order: the planning wait holds it, no move is sent
        assert_eq!(state.war_stage.get("Army 1"), Some(&2));
        w.executing.set(false);
        assert_eq!(
            tick(&mut state, &w, &p, "l", now + 2).unwrap(),
            Tick::Nothing
        );
        assert_eq!(w.actions().last().unwrap().0, "hoi4_army_plan");
    }

    /// Army 1's offensive on state 28 stalled and was stopped to be planned again at the
    /// returned hour (the plan needs "replans": 1); its plan no longer executes.
    fn replanned(w: &W, p: &Value) -> (State, GameHour) {
        let (mut state, at) = stalled(w, p);
        let step = acted(tick(&mut state, w, p, "l", at).unwrap());
        assert!(step.contains("plan stopped to plan again"), "{step}");
        w.executing.set(false);
        (state, at)
    }

    #[test]
    fn an_offensive_planned_again_executes_once_its_bonus_stood_still_three_days() {
        let w = W::new();
        let p = plan(json!({"replans": 1}));
        let (mut state, at) = replanned(&w, &p);
        // 5%, far below half of the wanted 27%: the divisions are in contact and build nothing
        w.planning.set(Some(0.05));
        for day in 0..3 {
            assert_eq!(
                tick(&mut state, &w, &p, "l", at + 2 + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        // two hours short of three game days since the first read: still waiting
        assert_eq!(
            tick(&mut state, &w, &p, "l", at + 3 * DAY).unwrap(),
            Tick::Nothing
        );
        let step = acted(tick(&mut state, &w, &p, "l", at + 2 + 3 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(
            line.contains(
                "planning stands at 5% for 3.0 game days: the divisions are not building it"
            ),
            "{line}"
        );
    }

    #[test]
    fn a_first_offensive_with_a_flat_low_bonus_waits_on() {
        let w = W::new();
        let p = plan(json!({}));
        let mut state = State::default();
        let step = acted(tick(&mut state, &w, &p, "l", START).unwrap());
        assert!(step.starts_with("war front and offensive Army 1"), "{step}");
        // the same 5% for ten game days: its divisions are still walking to the line
        w.planning.set(Some(0.05));
        for day in 0..=10 {
            assert_eq!(
                tick(&mut state, &w, &p, "l", START + 3 + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("read from hoi4_armies"), "{line}");
        // the longest wait bounds it, as before
        let step = acted(tick(&mut state, &w, &p, "l", START + 15 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("after the 15 game days at most"), "{line}");
    }

    #[test]
    fn planning_flat_days_sets_how_long_a_flat_bonus_is_waited_for() {
        // one game day
        let w = W::new();
        let p = plan(json!({"replans": 1, "planning_flat_days": 1}));
        let (mut state, at) = replanned(&w, &p);
        w.planning.set(Some(0.05));
        assert_eq!(
            tick(&mut state, &w, &p, "l", at + 2).unwrap(),
            Tick::Nothing
        );
        let step = acted(tick(&mut state, &w, &p, "l", at + 2 + DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        // five: the wait goes on past the three days of the default
        let w = W::new();
        let p = plan(json!({"replans": 1, "planning_flat_days": 5}));
        let (mut state, at) = replanned(&w, &p);
        w.planning.set(Some(0.05));
        for day in 0..5 {
            assert_eq!(
                tick(&mut state, &w, &p, "l", at + 2 + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        let step = acted(tick(&mut state, &w, &p, "l", at + 2 + 5 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("stands at 5% for 5.0 game days"), "{line}");
    }

    #[test]
    fn a_bonus_that_keeps_rising_is_waited_for_after_a_stopped_offensive() {
        let w = W::new();
        let p = plan(json!({"replans": 1}));
        let (mut state, at) = replanned(&w, &p);
        // +1% a game day from 2% to 8%: below half of the wanted 27%, but being built
        for day in 0..7 {
            w.planning.set(Some(0.02 + 0.01 * day as f64));
            assert_eq!(
                tick(&mut state, &w, &p, "l", at + 2 + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        // it stops at 8% on day 6: the three flat days count from that read
        for day in 7..9 {
            assert_eq!(
                tick(&mut state, &w, &p, "l", at + 2 + day * DAY).unwrap(),
                Tick::Nothing,
                "day {day}"
            );
        }
        let step = acted(tick(&mut state, &w, &p, "l", at + 2 + 9 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
        let line = &state.war_rules.notes["planning:Army 1"];
        assert!(line.contains("stands at 8% for 3.0 game days"), "{line}");
    }

    #[test]
    fn a_plan_stopped_by_a_rest_goes_back_to_the_planning_wait_not_into_a_stall() {
        let w = W::new();
        let p = plan(json!({}));
        let (mut state, now) = stalled(&w, &p);
        // something else stopped the plan (a casualty rest): the read says it is not executing
        w.executing.set(false);
        assert_eq!(tick(&mut state, &w, &p, "l", now).unwrap(), Tick::Nothing);
        assert_eq!(state.war_stage.get("Army 1"), Some(&2));
        assert!(
            w.actions().iter().all(|(tool, _)| tool != "hoi4_army_move"),
            "no direct move"
        );
    }

    #[test]
    fn an_army_in_move_mode_without_lines_tries_its_front_again() {
        let w = W::new();
        let p = plan(json!({}));
        let (mut state, now) = stalled(&w, &p);
        // its front could not be drawn and it was sent on a move: no battle plan orders
        state.war_move.insert("Army 1".into());
        w.executing.set(false);
        w.orders.set(0);
        assert_eq!(tick(&mut state, &w, &p, "l", now).unwrap(), Tick::Nothing);
        assert!(!state.war_move.contains("Army 1"));
        assert_eq!(state.war_stage.get("Army 1"), Some(&0));
        let step = acted(tick(&mut state, &w, &p, "l", now + 2).unwrap());
        assert!(
            step.starts_with("war front and offensive Army 1 -> state 28"),
            "{step}"
        );
    }

    #[test]
    fn an_army_left_in_move_mode_with_its_lines_drawn_returns_to_its_plan() {
        let w = W::new();
        let p = plan(json!({}));
        let (mut state, now) = stalled(&w, &p);
        // a direct move was its last order; the plan is not executing, both lines stand
        state.war_move.insert("Army 1".into());
        w.executing.set(false);
        assert_eq!(tick(&mut state, &w, &p, "l", now).unwrap(), Tick::Nothing);
        assert!(!state.war_move.contains("Army 1"));
        assert_eq!(state.war_stage.get("Army 1"), Some(&2));
        // planning is at the wanted level: the execute order follows
        w.planning.set(Some(0.3));
        let step = acted(tick(&mut state, &w, &p, "l", now + 2).unwrap());
        assert!(step.starts_with("war execute Army 1"), "{step}");
    }

    #[test]
    fn a_stall_brings_an_idle_army_beside_the_target_first() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false, "reserve": [], "axes": [[28], [18]]}));
        let (mut state, now) = stalled(&w, &p);
        // Army 2 stands beside the target with its lines drawn on the other axis and no plan
        // executing (an army without lines is no idle army: the opening pass draws them at once)
        w.flank.set(true);
        state.war_assign.insert("Army 2".into(), 18);
        state.war_stage.insert("Army 2".into(), 2);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(
            step.contains("second direction, Army 2 attacks it from state 43"),
            "{step}"
        );
        assert_eq!(state.war_assign.get("Army 2"), Some(&28));
        // Army 2 draws its own front and offensive line at the target next
        let step = acted(tick(&mut state, &w, &p, "l", now + 1).unwrap());
        assert!(
            step.starts_with("war front and offensive Army 2 -> state 28"),
            "{step}"
        );
        // the next stall of Army 1 on 28 is the direct move
        let later = now + 8 * DAY + 2;
        let moves = |w: &W| {
            w.actions()
                .iter()
                .filter(|(n, a)| n == "hoi4_army_move" && a["army"] == "Army 1")
                .count()
        };
        for h in 0..4 {
            let _ = tick(&mut state, &w, &p, "l", later + h);
        }
        assert_eq!(moves(&w), 1);
    }

    #[test]
    fn without_a_flank_the_stalled_army_goes_round_then_draws_again() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false}));
        let (mut state, now) = stalled(&w, &p);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(
            step.contains("plan stopped to go round by state 44"),
            "{step}"
        );
        w.executing.set(false);
        let step = acted(tick(&mut state, &w, &p, "l", now + 1).unwrap());
        assert!(
            step.starts_with("war second direction Army 1 -> state 44"),
            "{step}"
        );
        // on the way: nothing
        assert_eq!(
            tick(&mut state, &w, &p, "l", now + DAY).unwrap(),
            Tick::Nothing
        );
        // arrived: the front and the offensive line are drawn from there
        w.here.set(44);
        let step = acted(tick(&mut state, &w, &p, "l", now + 2 * DAY).unwrap());
        assert!(
            step.starts_with("war front and offensive Army 1 -> state 28"),
            "{step}"
        );
    }

    #[test]
    fn switched_off_the_stall_is_the_direct_move_at_once() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false, "second_direction": false}));
        let (mut state, now) = stalled(&w, &p);
        let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
        assert!(step.starts_with("war stalled Army 1"), "{step}");
        assert_eq!(w.actions().last().unwrap().0, "hoi4_army_move");
    }

    #[test]
    fn a_stall_move_goes_to_the_armys_victory_point_province_in_the_target() {
        // the direct move of a stalled Army 1 on state 28, with these `vp_targets`
        let stall_move = |vp: Option<Value>| {
            let w = W::new();
            let mut p = plan(json!({"planning_wait": false, "second_direction": false}));
            if let Some(vp) = vp {
                p["vp_targets"] = vp;
            }
            let (mut state, now) = stalled(&w, &p);
            let step = acted(tick(&mut state, &w, &p, "l", now).unwrap());
            assert!(
                step.starts_with("war stalled Army 1 8 days -> move to state 28"),
                "{step}"
            );
            let (tool, args) = w.actions().last().unwrap().clone();
            assert_eq!(tool, "hoi4_army_move");
            assert_eq!(args["army"], "Army 1");
            assert_eq!(args["state_id"], 28);
            args.get("province_id").cloned()
        };
        // province 282 of the target is not held (the state read lists 280 and 281 as taken)
        assert_eq!(
            stall_move(Some(
                json!([{"province": 282, "state": 28, "army": "Army 1"}])
            )),
            Some(json!(282))
        );
        // no row: the move goes to the state, as before
        assert_eq!(stall_move(None), None);
        // a row whose province is held already is no target either
        assert_eq!(
            stall_move(Some(
                json!([{"province": 280, "state": 28, "army": "Army 1"}])
            )),
            None
        );
    }

    #[test]
    fn a_captured_hub_holds_the_next_execute_until_it_feeds_the_front() {
        let w = W::new();
        let p = plan(json!({"planning_wait": false}));
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        w.executing.set(true);
        tick(&mut state, &w, &p, "l", START).unwrap();
        // 28 falls: the army turns to 18 and the bound starts
        w.taken.set(true);
        w.executing.set(false);
        w.here.set(28);
        w.share.set(0.2);
        let step = acted(tick(&mut state, &w, &p, "l", START + DAY).unwrap());
        assert!(
            step.starts_with("war front and offensive Army 1 -> state 18"),
            "{step}"
        );
        assert!(state.war_rules.bound.contains_key("Army 1"));
        // the lines stand; the bound holds the execute order
        assert_eq!(
            tick(&mut state, &w, &p, "l", START + 3 * DAY).unwrap(),
            Tick::Nothing
        );
        assert!(state.war_rules.notes["bound:Army 1"].contains("holds after taking state 28"));
        // the hub feeds the army's province: execute
        w.flow.set(1.5);
        let step = acted(tick(&mut state, &w, &p, "l", START + 4 * DAY).unwrap());
        assert!(step.starts_with("war execute Army 1 -> state 18"), "{step}");
    }

    #[test]
    fn a_bound_ends_after_the_rail_days_or_the_max() {
        let mut state = State::default();
        let p = plan(json!({}));
        state.war_rules.bound.insert(
            "A".into(),
            Bound {
                state: 1,
                since: START,
                nodes: vec![],
                rail: true,
                core: false,
            },
        );
        assert!(bound_wait(&mut state, &p, "A", None, Some(80.0), START + 9 * DAY).is_some());
        assert!(bound_wait(&mut state, &p, "A", None, Some(40.0), START + 11 * DAY).is_some());
        assert!(bound_wait(&mut state, &p, "A", None, Some(40.0), START + 12 * DAY).is_none());
        state.war_rules.bound.insert(
            "B".into(),
            Bound {
                state: 1,
                since: START,
                nodes: vec![],
                rail: true,
                core: true,
            },
        );
        assert!(bound_wait(&mut state, &p, "B", None, Some(80.0), START + 5 * DAY).is_none());
    }

    #[test]
    fn disband_keeps_overseas_and_encircled_divisions() {
        struct D(RefCell<Vec<(String, Value)>>, bool);
        impl Tools for D {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                if args.get("request_id").is_some() {
                    return Ok(json!({"receipt": {"status": "Verified"}}));
                }
                Ok(match name {
                    "hoi4_overview" => json!({"player": "ITA"}),
                    "hoi4_states" => json!({"states": [
                        {"state_id": 158, "owner": "ITA", "controller": "ITA"},
                        {"state_id": 160, "owner": "ITA", "controller": "ITA"},
                        {"state_id": 550, "owner": "ITA", "controller": "ITA"}]}),
                    "hoi4_wars" if self.1 => {
                        json!({"capital_land": {"capital_state": 2, "states": [2, 158, 160]}})
                    }
                    "hoi4_wars" => {
                        json!({"capital_land": null, "capital_land_reason": "map_data_unavailable"})
                    }
                    "hoi4_armies" => json!({"armies": [
                        {"army": "Army 4", "threat": {"encirclement_risk": true}},
                        {"army": "Army 5", "threat": {"encirclement_risk": false}}]}),
                    "hoi4_supply" => json!({"divisions": [
                        {"army": "Army 3", "id": "o", "template": "CN", "state_id": 550},
                        {"army": "Army 4", "id": "a", "template": "CN", "state_id": 158},
                        {"army": "Army 5", "id": "d", "template": "CN", "state_id": 160}]}),
                    _ => json!({}),
                })
            }
        }
        let plan = json!({"disband": {"templates": ["CN"]}});
        let d = D(RefCell::new(vec![]), true);
        let mut state = State::default();
        let step = super::super::disband_step(&mut state, &d, &plan, "l", START)
            .unwrap()
            .unwrap();
        // Army 3 stands overseas (550), Army 4 is at encirclement risk: only Army 5
        assert!(step.starts_with("disband 1 from Army 5"), "{step}");
        assert!(state.war_rules.notes["disband"].contains("o"));
        // no land-route read: nothing is disbanded
        let d = D(RefCell::new(vec![]), false);
        let mut state = State::default();
        assert_eq!(
            super::super::disband_step(&mut state, &d, &plan, "l", START).unwrap(),
            None
        );
        assert!(state.war_rules.notes["disband"].contains("map_data_unavailable"));
        assert!(!d
            .0
            .borrow()
            .iter()
            .any(|(n, _)| n == "hoi4_division_disband"));
    }
}
