//! Objective strikes: once the opening battle plans have done their work, every driven army is
//! sent at one province at a time with a single march order of the whole army, and the next
//! objective is ordered the tick the last one falls. A child module of `plan_run` (declared
//! there with `#[path]`), like plan_war.rs: every order goes through `State::act`; this
//! module's memory (`State::strike`) is this server life's.
//!
//! Why: a front line spreads an army along the whole border and a battle plan cannot gather it
//! on one province, so a planned offensive takes the border states and then stands (Italy 1936:
//! seven of Ethiopia's victory points by March, nothing more to July). One march order on a
//! victory-point province the army stands beside took each of them in days, and Ethiopia
//! capitulated four weeks ahead of the game's own AI (live 15 Nov 2026).
//!
//! Plan key `war.strike` (absent or false: the war step is unchanged). `true` or an object:
//!   after_vp    victory points of the enemies the player must hold before the strike step takes
//!               the armies over (default 0: at once). Until then the battle plans run; a phase
//!               override can also switch the key on.
//!   rest        0.35: a strike ends when the divisions beside the objective fall under this
//!               share of the best organisation each has shown
//!   resume      0.72: a rested army strikes again at this share (army mean)
//!   rest_days   10: or after this many game days
//!   max_days    18: a strike that has not taken its objective ends after this many days
//!   close_days  25: an objective is closed for the army for this long after a strike that
//!               stood its whole `max_days`, or after two that wore the divisions out
//!   beside      2.0: fresh divisions that must stand beside a victory point for the army to
//!               turn on it without a plan row
//!   halt        true: a strike that ends is halted (hoi4_army_move halt) and the army rests
//!               where it stands before it chooses again. Default false: no halt order. The
//!               objective is closed for the army (`rest_days` after a strike that wore its
//!               divisions out, `close_days` after one that stood or a second worn one) and its
//!               next march order replaces the attack; with no other objective the attack goes
//!               on. A halted division returns to the front line its army still holds, which
//!               lies far behind an advance (live 15 Nov 2026: halted armies walked back to the
//!               border they had started from)
//!   guards      true: guard armies are driven too (the border they held is no longer the
//!               front). Default false: they keep their fronts
//!
//! The objective of an army, in this order:
//!   1. its first open `vp_targets` row (rows naming the army, in plan order): the leg is the
//!      first province of the row's `path` not yet held, else the row's province. A row is
//!      open when its leg lies in land the army may enter, touches land already held and is
//!      not closed for the army. Beside the leg or on the path the army strikes it; from afar
//!      it marches to the path's first province when that is held (the plan's assembly point);
//!   2. an enemy victory point with `beside` fresh divisions next to it (most value first);
//!   3. the nearest enemy victory point over land no other army is already striking.
//!
//! A province is held when the player controls it (victory points), when its state is under the
//! player's control, or when own divisions have stood in it. Land of a country the player is
//! not at war with is closed.
//!
//! While the step is on, the war step's battle plans, pocket fall-back, rests and stall answers
//! do not run for these armies, the overdrawn rule and the command merges leave them alone, and
//! an army the player ordered in the last `player_claim_seconds` or named in `reserve` is not
//! touched. Guard armies keep their fronts.
use super::{list, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

#[derive(Clone, Copy, PartialEq, Debug, Default)]
enum Mode {
    #[default]
    Idle,
    Strike,
    March,
    Rest,
}

#[derive(Default)]
struct Army {
    mode: Mode,
    objective: i64,
    since: GameHour,
    ended: HashMap<i64, u8>,
    closed: HashMap<i64, GameHour>,
}

/// What the strike step keeps (this server life).
#[derive(Default)]
pub(super) struct Memory {
    /// The step has taken the armies over.
    pub(super) active: bool,
    armies: HashMap<String, Army>,
    /// division id -> the best organisation it has shown
    best: HashMap<String, f64>,
    /// provinces own divisions have stood in
    seen: HashSet<i64>,
    /// province -> (state, land neighbours), read once for `graph_states`
    graph: HashMap<i64, (i64, Vec<i64>)>,
    graph_states: Vec<i64>,
    /// army -> when its executing battle plan was last told to stop
    stopped: HashMap<String, GameHour>,
}

struct Knobs {
    after_vp: f64,
    rest: f64,
    resume: f64,
    rest_days: GameHour,
    max_days: GameHour,
    close_days: GameHour,
    beside: f64,
    guards: bool,
    halt: bool,
}

fn knobs(cfg: &Value) -> Option<Knobs> {
    let k = &cfg["strike"];
    if !(k == &json!(true) || k.is_object()) {
        return None;
    }
    let number = |key: &str, default: f64| k[key].as_f64().unwrap_or(default);
    Some(Knobs {
        after_vp: number("after_vp", 0.0),
        rest: number("rest", 0.35),
        resume: number("resume", 0.72),
        rest_days: number("rest_days", 10.0) as GameHour,
        max_days: number("max_days", 18.0) as GameHour,
        close_days: number("close_days", 25.0) as GameHour,
        beside: number("beside", 2.0),
        guards: k["guards"] == json!(true),
        halt: k["halt"] == json!(true),
    })
}

struct Division {
    id: String,
    army: String,
    province: i64,
    organisation: f64,
    heading: Option<i64>,
}

struct World<'a> {
    memory: &'a Memory,
    /// enemy-owned victory-point province -> (held by the player, points)
    vp: BTreeMap<i64, (bool, f64)>,
    own_states: HashSet<i64>,
    enemy_states: HashSet<i64>,
}

impl World<'_> {
    fn state_of(&self, province: i64) -> Option<i64> {
        self.memory.graph.get(&province).map(|row| row.0)
    }
    fn held(&self, province: i64) -> bool {
        if let Some((mine, _)) = self.vp.get(&province) {
            return *mine;
        }
        self.memory.seen.contains(&province)
            || self
                .state_of(province)
                .is_some_and(|s| self.own_states.contains(&s))
    }
    fn passable(&self, province: i64) -> bool {
        self.held(province)
            || self.vp.contains_key(&province)
            || self
                .state_of(province)
                .is_some_and(|s| self.enemy_states.contains(&s))
    }
    /// A province that touches land already held: an objective the army can reach.
    fn frontier(&self, province: i64) -> bool {
        self.memory
            .graph
            .get(&province)
            .is_some_and(|row| row.1.iter().any(|n| self.held(*n)))
    }
    fn touches(&self, a: i64, b: i64) -> bool {
        self.memory
            .graph
            .get(&a)
            .is_some_and(|row| row.1.contains(&b))
    }
    fn share(&self, d: &Division) -> f64 {
        match self.memory.best.get(&d.id) {
            Some(best) if *best > 0.0 => d.organisation / best,
            _ => 1.0,
        }
    }
    /// The army's divisions standing beside `target`, and their weight in fresh divisions.
    fn beside<'d>(
        &self,
        divisions: &[&'d Division],
        target: i64,
        resume: f64,
    ) -> (f64, Vec<&'d Division>) {
        let near: Vec<&Division> = divisions
            .iter()
            .copied()
            .filter(|d| self.touches(target, d.province))
            .collect();
        let weight = near.iter().map(|d| (self.share(d) / resume).min(1.0)).sum();
        (weight, near)
    }
    /// Steps over land from any of `from` to `target` (a step into land not held counts
    /// three; `closed` provinces are not entered), and the first province on that way that is
    /// not held.
    fn way(&self, from: &HashSet<i64>, target: i64, closed: &HashSet<i64>) -> Option<(i64, i64)> {
        let mut cost: HashMap<i64, i64> = from.iter().map(|p| (*p, 0)).collect();
        let mut first: HashMap<i64, i64> = HashMap::new();
        let mut queue: VecDeque<i64> = from.iter().copied().collect();
        while let Some(p) = queue.pop_front() {
            let here = cost[&p];
            let Some((_, next)) = self.memory.graph.get(&p) else {
                continue;
            };
            for n in next {
                if !self.passable(*n) || closed.contains(n) {
                    continue;
                }
                let held = self.held(*n);
                let step = here + if held { 1 } else { 3 };
                if cost.get(n).is_none_or(|c| step < *c) {
                    cost.insert(*n, step);
                    let leg = match first.get(&p) {
                        Some(leg) => *leg,
                        None if held => 0,
                        None => *n,
                    };
                    if leg == 0 {
                        first.remove(n);
                    } else {
                        first.insert(*n, leg);
                    }
                    queue.push_back(*n);
                }
            }
        }
        cost.get(&target)
            .map(|c| (*c, first.get(&target).copied().unwrap_or(target)))
    }
}

/// The army's plan rows in order: (province, path).
fn rows(plan: &Value, names: &[String]) -> Vec<(i64, Vec<i64>)> {
    list(plan, "vp_targets")
        .iter()
        .filter(|row| row["reserve"] != json!(true))
        .filter(|row| names.iter().any(|n| n.as_str() == text(&row["army"])))
        .filter_map(|row| {
            let path = list(row, "path").iter().filter_map(Value::as_i64).collect();
            row["province"].as_i64().map(|p| (p, path))
        })
        .collect()
}

/// A change of an army's memory: (army, mode, objective, since, an ended strike and its weight).
type Change = (String, Mode, i64, Option<GameHour>, Option<(i64, u8)>);

enum Order {
    Strike(i64, String),
    March(i64, String),
}

#[allow(clippy::too_many_arguments)]
fn choose(
    world: &World,
    knobs: &Knobs,
    rows: &[(i64, Vec<i64>)],
    divisions: &[&Division],
    closed: &HashSet<i64>,
    taken_by_others: &HashSet<i64>,
) -> Option<Order> {
    let here: HashSet<i64> = divisions.iter().map(|d| d.province).collect();
    // The army's first open row, in plan order: its leg is the first province of the path (or
    // the row's province) not yet held. A row is open when its leg is not closed for the army,
    // lies in land the army may enter and touches land already held; a row that is not open
    // yet (a neutral's land on the way, a leg behind another objective) is passed over.
    let label = |leg: i64, target: i64| {
        if leg == target {
            format!("row {target}")
        } else {
            format!("row {target}, leg {leg}")
        }
    };
    for (target, path) in rows.iter().filter(|(p, _)| !world.held(*p)) {
        let mut legs: Vec<i64> = path.clone();
        if legs.last() != Some(target) {
            legs.push(*target);
        }
        let Some(leg) = legs.iter().copied().find(|p| !world.held(*p)) else {
            continue;
        };
        if closed.contains(&leg) || !world.passable(leg) || !world.frontier(leg) {
            continue;
        }
        let on_path = path.iter().any(|p| here.contains(p));
        if on_path || !world.beside(divisions, leg, knobs.resume).1.is_empty() {
            return Some(Order::Strike(leg, label(leg, *target)));
        }
        // From afar: assemble on the path's first province when it is held (the plan's fed
        // assembly point; the game routes from there), else go for the leg.
        return Some(match path.first() {
            Some(first) if world.held(*first) => {
                Order::March(*first, format!("assembly for row {target}"))
            }
            _ => Order::Strike(leg, label(leg, *target)),
        });
    }
    let mut best: Option<(f64, i64, String)> = None;
    for (province, (mine, points)) in &world.vp {
        if *mine || closed.contains(province) {
            continue;
        }
        let (weight, near) = world.beside(divisions, *province, knobs.resume);
        if weight >= knobs.beside {
            let score = points * weight;
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((
                    score,
                    *province,
                    format!("{} divisions beside it", near.len()),
                ));
            }
        }
    }
    if let Some((_, province, why)) = best {
        return Some(Order::Strike(province, why));
    }
    let mut best: Option<(f64, i64, String)> = None;
    for (province, (mine, points)) in &world.vp {
        if *mine || closed.contains(province) {
            continue;
        }
        let Some((cost, leg)) = world.way(&here, *province, closed) else {
            continue;
        };
        if closed.contains(&leg) || taken_by_others.contains(&leg) {
            continue;
        }
        let score = points / (1.0 + cost as f64);
        if best.as_ref().is_none_or(|b| score > b.0) {
            best = Some((score, leg, format!("the nearest victory point {province}")));
        }
    }
    best.map(|(_, leg, why)| Order::Strike(leg, why))
}

/// The provinces of `states` with their neighbours, read once per state list.
fn graph(memory: &mut Memory, tools: &dyn Tools, states: &[i64]) {
    if memory.graph_states == states && !memory.graph.is_empty() {
        return;
    }
    let Ok(read) = tools.call("hoi4_province_graph", &json!({"states": states})) else {
        return;
    };
    memory.graph = list(&read, "provinces")
        .iter()
        .filter_map(|row| {
            Some((
                row["province"].as_i64()?,
                (
                    row["state_id"].as_i64()?,
                    list(row, "neighbours")
                        .iter()
                        .filter_map(Value::as_i64)
                        .collect(),
                ),
            ))
        })
        .collect();
    memory.graph_states = states.to_vec();
}

/// The strike step. `None`: the step is off (or not yet on) and the war step runs as before.
/// `Some(line)`: the step owns the armies this tick; `line` is the action it took, if any.
#[allow(clippy::too_many_arguments)]
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    me: &str,
    enemies: &HashSet<String>,
    reserve: &[String],
    now: GameHour,
) -> Result<Option<Option<String>>> {
    let cfg = &plan["war"];
    let Some(knobs) = knobs(cfg) else {
        state.strike.active = false;
        return Ok(None);
    };
    if enemies.is_empty() {
        return Ok(None);
    }
    // Victory points of the enemies and who holds them; states under each side's control.
    let mut vp: BTreeMap<i64, (bool, f64)> = BTreeMap::new();
    let mut enemy_states: HashSet<i64> = HashSet::new();
    let mut all_states: Vec<i64> = cfg["theatre"]
        .as_array()
        .map(|rows| rows.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    let mut tags: Vec<&String> = enemies.iter().collect();
    tags.sort();
    for tag in tags {
        let read = tools.call("hoi4_states", &json!({"query": tag}))?;
        for row in list(&read, "states") {
            let Some(id) = row["state_id"].as_i64() else {
                continue;
            };
            all_states.push(id);
            if enemies.contains(text(&row["controller"])) {
                enemy_states.insert(id);
            }
            for point in list(row, "victory_points") {
                if let Some(p) = point["province"].as_i64() {
                    vp.insert(
                        p,
                        (
                            text(&point["controller"]) == me,
                            point["victory_points"].as_f64().unwrap_or(0.0),
                        ),
                    );
                }
            }
        }
    }
    let held_points: f64 = vp.values().filter(|v| v.0).map(|v| v.1).sum();
    if !state.strike.active {
        if held_points < knobs.after_vp {
            return Ok(None);
        }
        state.strike.active = true;
    }
    let mine = tools.call("hoi4_states", &json!({"query": me}))?;
    let mut own_states: HashSet<i64> = HashSet::new();
    for row in list(&mine, "states") {
        if let Some(id) = row["state_id"].as_i64() {
            all_states.push(id);
            if text(&row["controller"]) == me {
                own_states.insert(id);
            }
        }
    }
    all_states.sort_unstable();
    all_states.dedup();
    graph(&mut state.strike, tools, &all_states);
    if state.strike.graph.is_empty() {
        // No map: the war step runs as before rather than standing still.
        state.strike.active = false;
        return Ok(None);
    }
    let supply = tools.call("hoi4_supply", &json!({"summary": false}))?;
    let divisions: Vec<Division> = list(&supply, "divisions")
        .iter()
        .filter_map(|row| {
            let province = row["province_id"].as_i64()?;
            state
                .strike
                .graph
                .contains_key(&province)
                .then(|| Division {
                    id: text(&row["id"]).to_string(),
                    army: text(&row["army"]).to_string(),
                    province,
                    organisation: row["organisation"].as_f64().unwrap_or(0.0),
                    heading: row["heading_province"].as_i64(),
                })
        })
        .collect();
    for d in &divisions {
        let best = state.strike.best.entry(d.id.clone()).or_insert(0.0);
        if d.organisation > *best {
            *best = d.organisation;
        }
        state.strike.seen.insert(d.province);
    }
    let roster = tools.call("hoi4_armies", &json!({"threat": false}))?;
    let executing: HashSet<String> = list(&roster, "armies")
        .iter()
        .filter(|a| a["plan_executing"] == json!(true))
        .map(|a| text(&a["army"]).to_string())
        .collect();
    let mut by_army: BTreeMap<String, Vec<&Division>> = BTreeMap::new();
    for d in divisions.iter().filter(|d| !d.army.is_empty()) {
        by_army.entry(d.army.clone()).or_default().push(d);
    }
    let guards: HashSet<String> = state
        .war_stage
        .keys()
        .filter_map(|k| k.strip_prefix("guard:").map(str::to_string))
        .collect();
    let layout: Vec<(String, String)> = state
        .layout
        .iter()
        .map(|(hint, army)| (hint.clone(), army.clone()))
        .collect();
    let mut memory = std::mem::take(&mut state.strike);
    let mut act: Option<(String, Value, String, String, Mode, i64)> = None;
    let mut stop: Option<String> = None;
    {
        let world = World {
            memory: &memory,
            vp,
            own_states,
            enemy_states,
        };
        // Objectives other armies are striking now (for the nearest-victory-point choice).
        let striking: HashMap<String, i64> = memory
            .armies
            .iter()
            .filter(|(_, a)| a.mode == Mode::Strike)
            .map(|(n, a)| (n.clone(), a.objective))
            .collect();
        let mut changes: Vec<Change> = Vec::new();
        for (name, members) in &by_army {
            if members.len() < 2
                || reserve.contains(name)
                || (!knobs.guards && guards.contains(name))
            {
                continue;
            }
            let default = Army::default();
            let army = memory.armies.get(name).unwrap_or(&default);
            let mean = members.iter().map(|d| world.share(d)).sum::<f64>() / members.len() as f64;
            let mut mode = army.mode;
            let since = army.since;
            let objective = army.objective;
            // An executing battle plan would send the divisions back to their line.
            if executing.contains(name) && memory.stopped.get(name).is_none_or(|at| now - at >= DAY)
            {
                stop = Some(name.clone());
                act = Some((
                    "hoi4_army_plan".into(),
                    json!({"army": name, "execute": false}),
                    format!("strike {name}: battle plan stopped, the army goes by objectives"),
                    name.clone(),
                    mode,
                    objective,
                ));
                break;
            }
            let mut ended: Option<(i64, u8)> = None;
            // The objective a strike has just turned away from (not chosen again this tick).
            let mut turned: Option<i64> = None;
            if mode == Mode::Strike {
                let taken =
                    world.held(objective) || members.iter().any(|d| d.province == objective);
                let age = now - since;
                if taken {
                    mode = Mode::Idle;
                } else {
                    let (_, near) = world.beside(members, objective, knobs.resume);
                    let worn = !near.is_empty()
                        && near.iter().map(|d| world.share(d)).sum::<f64>() / (near.len() as f64)
                            < knobs.rest;
                    let over = (worn && age >= 3 * DAY) || age >= knobs.max_days * DAY;
                    if over && !knobs.halt {
                        // The strike ends without a halt order: the objective is closed for
                        // the army and its next march order replaces this one.
                        ended = Some((objective, if worn { 1 } else { 2 }));
                        turned = Some(objective);
                        mode = Mode::Idle;
                    } else if over {
                        changes.push((
                            name.clone(),
                            Mode::Rest,
                            objective,
                            Some(now),
                            Some((objective, if worn { 1 } else { 2 })),
                        ));
                        act = Some((
                            "hoi4_army_move".into(),
                            json!({"army": name, "halt": true}),
                            format!(
                                "strike {name}: halt before province {objective} after {} days (organisation {:.0}% of its best)",
                                age / DAY,
                                mean * 100.0
                            ),
                            name.clone(),
                            Mode::Rest,
                            objective,
                        ));
                        break;
                    }
                    // The order was lost (a retreat, a battle plan): choose and send again.
                    if !over
                        && age >= 2 * DAY
                        && !members.iter().any(|d| d.heading == Some(objective))
                    {
                        mode = Mode::Idle;
                    }
                }
            }
            if mode == Mode::March {
                let there = members.iter().filter(|d| d.province == objective).count();
                if there * 2 >= members.len() || now - since >= 25 * DAY {
                    mode = Mode::Idle;
                }
            }
            if mode == Mode::Rest && (mean >= knobs.resume || now - since >= knobs.rest_days * DAY)
            {
                mode = Mode::Idle;
                ended = Some((objective, 0));
            }
            if mode != army.mode {
                changes.push((name.clone(), mode, objective, None, ended));
            }
            if mode != Mode::Idle {
                continue;
            }
            let mut closed: HashSet<i64> = army
                .closed
                .iter()
                .filter(|(_, until)| **until > now)
                .map(|(p, _)| *p)
                .collect();
            closed.extend(turned);
            let others: HashSet<i64> = striking
                .iter()
                .filter(|(n, _)| *n != name)
                .map(|(_, p)| *p)
                .collect();
            let mut names = vec![name.clone()];
            names.extend(
                layout
                    .iter()
                    .filter(|(_, a)| a == name)
                    .map(|(h, _)| h.clone()),
            );
            let Some(order) = choose(
                &world,
                &knobs,
                &rows(plan, &names),
                members,
                &closed,
                &others,
            ) else {
                continue;
            };
            let (province, why, next) = match order {
                Order::Strike(p, why) => (p, why, Mode::Strike),
                Order::March(p, why) => (p, why, Mode::March),
            };
            let Some(target_state) = world.state_of(province) else {
                continue;
            };
            let verb = if next == Mode::Strike {
                "on"
            } else {
                "marches to"
            };
            act = Some((
                "hoi4_army_move".into(),
                json!({"army": name, "state_id": target_state, "province_id": province}),
                format!(
                    "strike {name} {verb} province {province} ({why}; {} divisions, organisation {:.0}% of their best)",
                    members.len(),
                    mean * 100.0
                ),
                name.clone(),
                next,
                province,
            ));
            break;
        }
        for (name, mode, objective, at, ended) in changes {
            let army = memory.armies.entry(name).or_default();
            army.mode = mode;
            army.objective = objective;
            if let Some(at) = at {
                army.since = at;
            }
            if let Some((province, weight)) = ended {
                if weight > 0 {
                    // A strike that wore its divisions out counts once (rested, it is tried
                    // again); one that stood its whole time counts twice: the objective holds.
                    let n = army.ended.entry(province).or_insert(0);
                    *n += weight;
                    if *n >= 2 {
                        *n = 0;
                        army.closed.insert(province, now + knobs.close_days * DAY);
                    } else if !knobs.halt {
                        // Worn out once, no halt: another objective while the divisions that
                        // fought recover, then this one again.
                        army.closed.insert(province, now + knobs.rest_days * DAY);
                    }
                }
            }
        }
    }
    if let Some(name) = stop {
        memory.stopped.insert(name, now);
    }
    state.strike = memory;
    let Some((tool, args, line, name, mode, objective)) = act else {
        return Ok(Some(None));
    };
    let (ok, why) = state.act(tools, lease, &tool, args, "ws")?;
    let army = state.strike.armies.entry(name).or_default();
    if tool == "hoi4_army_move" && mode != Mode::Rest {
        if ok {
            army.mode = mode;
            army.objective = objective;
            army.since = now;
        } else {
            // An order the game did not take: another objective for two days.
            army.closed.insert(objective, now + 2 * DAY);
        }
    }
    Ok(Some(Some(format!("{line}: {ok} {why}"))))
}

/// The plan asks for objective strikes and the step has taken the armies over: the overdrawn
/// rule and the command merges leave the armies alone.
pub(super) fn owns_armies(state: &State, plan: &Value) -> bool {
    state.strike.active && knobs(&plan["war"]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    const START: GameHour = 1_000_000;
    /// A division: (id, army, province, organisation, heading).
    type Row = (String, String, i64, f64, Option<i64>);

    /// ME holds state 1 (provinces 10, 11) and is at war with ENE: state 2 (20 with 2 victory
    /// points, 21) and state 3 (30 with 5). State 4 (40) belongs to a neutral until `annexed`.
    /// 10-11, 11-20, 11-21, 20-21, 20-30, 21-30, 10-40, 40-30.
    struct Map {
        /// provinces of ENE the player holds
        taken: RefCell<Vec<i64>>,
        divisions: RefCell<Vec<Row>>,
        executing: Cell<bool>,
        annexed: Cell<bool>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl Map {
        fn new(province: i64) -> Self {
            let divisions = (0..3)
                .map(|n| (format!("d{n}"), "Army 1".to_string(), province, 40.0, None))
                .collect();
            Self {
                taken: RefCell::new(vec![]),
                divisions: RefCell::new(divisions),
                executing: Cell::new(false),
                annexed: Cell::new(false),
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
        fn set(&self, organisation: f64, heading: Option<i64>) {
            for d in self.divisions.borrow_mut().iter_mut() {
                d.3 = organisation;
                d.4 = heading;
            }
        }
        fn stand(&self, province: i64) {
            for d in self.divisions.borrow_mut().iter_mut() {
                d.2 = province;
            }
        }
    }
    impl Tools for Map {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if args.get("request_id").is_some() {
                if name == "hoi4_army_plan" {
                    self.executing.set(args["execute"] == json!(true));
                }
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            let who = |p: i64| {
                if self.taken.borrow().contains(&p) {
                    "ME"
                } else {
                    "ENE"
                }
            };
            Ok(match name {
                "hoi4_overview" => json!({"player": "ME"}),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "ME" => {
                        let mut rows =
                            vec![json!({"state_id": 1, "owner": "ME", "controller": "ME"})];
                        if self.annexed.get() {
                            rows.push(json!({"state_id": 4, "owner": "ME", "controller": "ME"}));
                        }
                        json!({"states": rows})
                    }
                    "ENE" => json!({"states": [
                        {"state_id": 2, "owner": "ENE", "controller": "ENE", "victory_points": [
                            {"province": 20, "controller": who(20), "victory_points": 2.0}]},
                        {"state_id": 3, "owner": "ENE", "controller": "ENE", "victory_points": [
                            {"province": 30, "controller": who(30), "victory_points": 5.0}]}]}),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_province_graph" => json!({"provinces": [
                    {"province": 10, "state_id": 1, "neighbours": [11, 40]},
                    {"province": 11, "state_id": 1, "neighbours": [10, 20, 21]},
                    {"province": 20, "state_id": 2, "neighbours": [11, 21, 30]},
                    {"province": 21, "state_id": 2, "neighbours": [11, 20, 30]},
                    {"province": 30, "state_id": 3, "neighbours": [20, 21, 40]},
                    {"province": 40, "state_id": 4, "neighbours": [10, 30]}]}),
                "hoi4_supply" => json!({"divisions": self.divisions.borrow().iter().map(|d| json!({
                    "id": d.0, "army": d.1, "province_id": d.2, "organisation": d.3,
                    "heading_province": d.4})).collect::<Vec<_>>()}),
                "hoi4_armies" => json!({"armies": [{"army": "Army 1", "division_count": 3,
                    "plan_executing": self.executing.get()}]}),
                _ => json!({}),
            })
        }
    }

    fn plan(strike: Value) -> Value {
        json!({"war": {"enemies": ["ENE"], "theatre": [1, 4], "strike": strike},
            "vp_targets": [{"province": 20, "state": 2, "army": "Army 1"}]})
    }

    fn run(state: &mut State, map: &Map, plan: &Value, now: GameHour) -> Option<Option<String>> {
        let enemies: HashSet<String> = ["ENE".to_string()].into_iter().collect();
        step(state, map, plan, "l", "ME", &enemies, &[], now).unwrap()
    }

    #[test]
    fn without_the_key_the_war_step_runs_as_before() {
        let map = Map::new(11);
        let mut state = State::default();
        assert!(run(&mut state, &map, &plan(json!(false)), START).is_none());
        assert!(map.actions().is_empty());
        assert!(!owns_armies(&state, &plan(json!(false))));
    }

    #[test]
    fn the_step_waits_for_the_victory_points_the_plan_names() {
        let map = Map::new(11);
        let mut state = State::default();
        let wait = plan(json!({"after_vp": 2}));
        assert!(run(&mut state, &map, &wait, START).is_none());
        map.taken.borrow_mut().push(20);
        map.stand(20);
        let line = run(&mut state, &map, &wait, START + DAY).unwrap().unwrap();
        assert!(line.contains("province 30"), "{line}");
        assert!(owns_armies(&state, &wait));
    }

    #[test]
    fn one_march_order_on_the_row_beside_the_army_and_the_next_when_it_falls() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!(true));
        let line = run(&mut state, &map, &on, START).unwrap().unwrap();
        assert!(
            line.contains("on province 20") && line.contains("row 20"),
            "{line}"
        );
        let (tool, args) = map.actions().last().cloned().unwrap();
        assert_eq!(tool, "hoi4_army_move");
        assert_eq!(
            (args["province_id"].as_i64(), args["state_id"].as_i64()),
            (Some(20), Some(2))
        );
        // the order stands: nothing is sent while the divisions march on it
        map.set(38.0, Some(20));
        assert_eq!(run(&mut state, &map, &on, START + 3 * DAY), Some(None));
        assert_eq!(map.actions().len(), 1);
        // the province falls: the victory point beside the army is ordered the same tick
        map.taken.borrow_mut().push(20);
        map.stand(20);
        map.set(30.0, None);
        let line = run(&mut state, &map, &on, START + 5 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 30") && line.contains("beside it"),
            "{line}"
        );
    }

    #[test]
    fn a_worn_strike_halts_rests_and_strikes_again() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!({"halt": true}));
        run(&mut state, &map, &on, START);
        map.set(10.0, Some(20));
        let line = run(&mut state, &map, &on, START + 4 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("halt before province 20"), "{line}");
        assert_eq!(map.actions().last().unwrap().1["halt"], json!(true));
        // resting: no order until the organisation is back
        map.set(20.0, None);
        assert_eq!(run(&mut state, &map, &on, START + 6 * DAY), Some(None));
        map.set(36.0, None);
        let line = run(&mut state, &map, &on, START + 8 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("on province 20"), "{line}");
        // a second strike that ends closes the objective: the army turns to another one
        map.set(8.0, Some(20));
        run(&mut state, &map, &on, START + 12 * DAY);
        map.set(36.0, None);
        let line = run(&mut state, &map, &on, START + 14 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("province 21") || line.contains("province 30"),
            "{line}"
        );
        assert!(!line.contains("on province 20"), "{line}");
    }

    #[test]
    fn an_executing_battle_plan_is_stopped_first() {
        let map = Map::new(11);
        map.executing.set(true);
        let mut state = State::default();
        let on = plan(json!(true));
        let line = run(&mut state, &map, &on, START).unwrap().unwrap();
        assert!(line.contains("battle plan stopped"), "{line}");
        assert_eq!(map.actions().last().unwrap().0, "hoi4_army_plan");
        let line = run(&mut state, &map, &on, START + 1).unwrap().unwrap();
        assert!(line.contains("on province 20"), "{line}");
    }

    #[test]
    fn a_row_from_afar_assembles_on_the_first_province_of_its_path() {
        let map = Map::new(10);
        map.annexed.set(true);
        let mut state = State::default();
        let far = json!({"war": {"enemies": ["ENE"], "theatre": [1, 4], "strike": true},
            "vp_targets": [{"province": 30, "state": 3, "army": "Army 1", "path": [40]}]});
        let line = run(&mut state, &map, &far, START).unwrap().unwrap();
        assert!(
            line.contains("marches to province 40") && line.contains("assembly"),
            "{line}"
        );
        map.stand(40);
        let line = run(&mut state, &map, &far, START + 6 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("on province 30"), "{line}");
    }

    #[test]
    fn neutral_land_is_closed_and_reserve_armies_are_left_alone() {
        let map = Map::new(10);
        let mut state = State::default();
        // the row's path runs through a neutral's province: the way to it is closed, and the
        // army goes for the nearest victory point over land it may enter
        let far = json!({"war": {"enemies": ["ENE"], "theatre": [1, 4], "strike": true},
            "vp_targets": [{"province": 30, "state": 3, "army": "Army 1", "path": [40]}]});
        let line = run(&mut state, &map, &far, START).unwrap().unwrap();
        assert!(
            line.contains("on province 20") && line.contains("nearest"),
            "{line}"
        );
        let mut other = State::default();
        let enemies: HashSet<String> = ["ENE".to_string()].into_iter().collect();
        let reserve = ["Army 1".to_string()];
        let done = step(&mut other, &map, &far, "l", "ME", &enemies, &reserve, START).unwrap();
        assert_eq!(done, Some(None));
    }

    #[test]
    fn a_strike_that_stood_its_whole_time_closes_the_objective_at_once() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!({"max_days": 6}));
        run(&mut state, &map, &on, START);
        // six days on, the divisions are not worn and the province still stands: the army
        // turns to another objective the same tick, without a halt order
        map.set(38.0, Some(20));
        let line = run(&mut state, &map, &on, START + 6 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 21") || line.contains("on province 30"),
            "{line}"
        );
        assert!(map.actions().iter().all(|(_, a)| a.get("halt").is_none()));
    }

    #[test]
    fn a_worn_strike_turns_to_another_objective_and_comes_back() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!(true));
        run(&mut state, &map, &on, START);
        map.set(10.0, Some(20));
        let line = run(&mut state, &map, &on, START + 4 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            !line.contains("on province 20") && line.contains("on province"),
            "{line}"
        );
        assert!(map.actions().iter().all(|(_, a)| a.get("halt").is_none()));
        // rest_days later the first objective is open again
        map.stand(11);
        map.set(36.0, None);
        let line = run(&mut state, &map, &on, START + 16 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("on province 20"), "{line}");
    }

    #[test]
    fn a_row_whose_leg_touches_no_held_land_waits_for_the_one_before_it() {
        let map = Map::new(11);
        let mut state = State::default();
        // row 30 lies behind 20 and 21: it is not open, the army takes what stands beside it
        let behind = json!({"war": {"enemies": ["ENE"], "theatre": [1, 4], "strike": true},
            "vp_targets": [{"province": 30, "state": 3, "army": "Army 1"}]});
        let line = run(&mut state, &map, &behind, START).unwrap().unwrap();
        assert!(
            line.contains("on province 20") && line.contains("beside it"),
            "{line}"
        );
        map.taken.borrow_mut().push(20);
        map.stand(20);
        let line = run(&mut state, &map, &behind, START + 5 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 30") && line.contains("row 30"),
            "{line}"
        );
    }
}
