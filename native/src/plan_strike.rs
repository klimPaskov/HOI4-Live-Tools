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
//!               stood its whole `max_days`, or after two that wore the divisions out. A strike
//!               that wears its divisions out keeps its attack (the defender wears too): no new
//!               order is sent until the army has rested (`resume` or `rest_days`), then it
//!               chooses again, usually the same objective
//!   worn_gate   0.5: an army under this share of its best organisation opens no new strike
//!               until `rest_days` after its last order (retaking own land does not wait)
//!   feed        true: a strike goes with as many divisions as the land at the objective feeds
//!               (the supply of the best fed own province beside it over what one division
//!               draws, less what other armies already send there; those beside it and the
//!               freshest first, by hoi4_army_move `divisions`), the rest keep where they stand;
//!               and a stack its province cannot feed (`floor` 0.35 supply) sends its most worn
//!               divisions to held land near it with room for them, where they rest. Armies may
//!               converge on one objective while its land feeds them
//!   garrison    true: a taken victory point at the front keeps the most worn division of the
//!               army standing in it; the army's orders go to the others until the point lies
//!               behind the front (an advance that leaves its towns empty loses them)
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
//!   3. the nearest enemy victory point over land (an objective whose land feeds no more
//!      divisions than other armies already send there is passed over).
//!
//! A row that was held and is lost again comes first: the army drops its strike or march and
//! retakes it (an advance that leaves its victory points empty is cut off behind its back:
//! live 15 Nov 2026 two towns behind three armies fell and their supply with them). Keep a
//! guard army on the border the advance leaves behind. A province of the player's own states
//! that an enemy holds beside the army comes before every row.
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
    /// the divisions the last order went to (empty: the whole army)
    group: Vec<String>,
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
    /// enemy victory points the player has held at some time (a row lost again is retaken first)
    had: HashSet<i64>,
    /// division id -> the taken victory point it holds for its army (left out of its orders)
    garrison: HashMap<String, i64>,
    /// division id -> until when it rests on fed land after leaving a starving stack
    parked: HashMap<String, GameHour>,
    /// province -> when its stack may be thinned again
    thinned: HashMap<i64, GameHour>,
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
    feed: bool,
    floor: f64,
    garrison: bool,
    worn_gate: f64,
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
        feed: k["feed"] != json!(false),
        floor: number("floor", 0.35),
        garrison: k["garrison"] != json!(false),
        worn_gate: number("worn_gate", 0.5),
    })
}

struct Division {
    id: String,
    army: String,
    province: i64,
    organisation: f64,
    heading: Option<i64>,
    /// supply percent (0-100), negative when unread
    supply: f64,
    /// supply the division draws
    need: f64,
}

struct World<'a> {
    memory: &'a Memory,
    /// enemy-owned victory-point province -> (held by the player, points)
    vp: BTreeMap<i64, (bool, f64)>,
    own_states: HashSet<i64>,
    enemy_states: HashSet<i64>,
    /// provinces of the player's own states an enemy holds
    lost_own: HashSet<i64>,
    /// province with own divisions -> (supply it receives, supply its divisions draw)
    supply: HashMap<i64, (f64, f64)>,
}

impl World<'_> {
    fn state_of(&self, province: i64) -> Option<i64> {
        self.memory.graph.get(&province).map(|row| row.0)
    }
    fn held(&self, province: i64) -> bool {
        if self.lost_own.contains(&province) {
            return false;
        }
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
            || self.lost_own.contains(&province)
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
    /// A held province beside land the enemy still holds: what is taken there can be lost.
    fn front(&self, province: i64) -> bool {
        self.held(province)
            && self
                .memory
                .graph
                .get(&province)
                .is_some_and(|row| row.1.iter().any(|n| self.passable(*n) && !self.held(*n)))
    }
    /// How many divisions drawing `need` each the land at `target` feeds: the supply of the best
    /// fed own province beside it (the objective's own supply is read only once own divisions
    /// stand in it). None when no own division stands beside it.
    fn feeds(&self, target: i64, need: f64) -> Option<usize> {
        if need <= 0.0 {
            return None;
        }
        let here = self.supply.get(&target).map(|s| s.0);
        let beside = self.memory.graph.get(&target).and_then(|row| {
            row.1
                .iter()
                .filter_map(|n| self.supply.get(n).map(|s| s.0))
                .max_by(f64::total_cmp)
        });
        let supply = here.or(beside)?;
        Some(((supply / need).floor() as usize).max(1))
    }
    /// Held provinces within `steps` of `from` over held land, nearest first.
    fn near(&self, from: i64, steps: usize) -> Vec<i64> {
        let mut seen: HashSet<i64> = [from].into();
        let mut ring = vec![from];
        let mut out = Vec::new();
        for _ in 0..steps {
            let mut next = Vec::new();
            for p in ring {
                for n in self
                    .memory
                    .graph
                    .get(&p)
                    .map(|r| r.1.as_slice())
                    .unwrap_or(&[])
                {
                    if self.held(*n) && seen.insert(*n) {
                        next.push(*n);
                        out.push(*n);
                    }
                }
            }
            ring = next;
        }
        out
    }
}

/// The supply one division of `divisions` draws (median of those read), 0 when none is read.
fn need_of(divisions: &[&Division]) -> f64 {
    let mut needs: Vec<f64> = divisions
        .iter()
        .map(|d| d.need)
        .filter(|n| *n > 0.0)
        .collect();
    if needs.is_empty() {
        return 0.0;
    }
    needs.sort_by(f64::total_cmp);
    needs[needs.len() / 2]
}

/// A stack of the army that starves: the province its divisions stand in feeds fewer of them
/// than stand there and their supply is under `floor`. The divisions that leave (the most worn
/// first, the ones that feed stay) and the held province near it with room for them: (source,
/// division ids, destination).
fn thin(
    world: &World,
    members: &[&Division],
    floor: f64,
    blocked: &HashMap<i64, GameHour>,
    now: GameHour,
) -> Option<(i64, Vec<String>, i64)> {
    let mut by: BTreeMap<i64, Vec<&Division>> = BTreeMap::new();
    for d in members {
        by.entry(d.province).or_default().push(d);
    }
    for (province, mut here) in by {
        if here.len() < 2 || blocked.get(&province).is_some_and(|until| *until > now) {
            continue;
        }
        let read: Vec<f64> = here
            .iter()
            .map(|d| d.supply)
            .filter(|s| *s >= 0.0)
            .collect();
        if read.is_empty() || read.iter().sum::<f64>() / (read.len() as f64) >= floor * 100.0 {
            continue;
        }
        let Some((supply, demand)) = world.supply.get(&province).copied() else {
            continue;
        };
        let need = need_of(&here);
        if demand <= supply || need <= 0.0 {
            continue;
        }
        let keep = ((supply / need).floor() as usize).max(1);
        if keep >= here.len() {
            continue;
        }
        here.sort_by(|a, b| world.share(a).total_cmp(&world.share(b)));
        let out: Vec<&Division> = here[..here.len() - keep].to_vec();
        let wanted: f64 = out.iter().map(|d| d.need.max(need)).sum();
        let Some(to) = world
            .near(province, 3)
            .into_iter()
            .filter_map(|p| world.supply.get(&p).map(|s| (p, s.0 - s.1)))
            .filter(|(_, room)| *room >= wanted)
            .max_by(|a, b| a.1.total_cmp(&b.1))
        else {
            continue;
        };
        return Some((province, out.iter().map(|d| d.id.clone()).collect(), to.0));
    }
    None
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
) -> Option<Order> {
    let here: HashSet<i64> = divisions.iter().map(|d| d.province).collect();
    // Own land an enemy holds beside the army comes before every row.
    let mut own: Vec<i64> = world.lost_own.iter().copied().collect();
    own.sort_unstable();
    for province in own {
        if !closed.contains(&province)
            && !world.beside(divisions, province, knobs.resume).1.is_empty()
        {
            return Some(Order::Strike(
                province,
                "own land the enemy holds".to_string(),
            ));
        }
    }
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
        if closed.contains(&leg) {
            continue;
        }
        let score = points / (1.0 + cost as f64);
        if best.as_ref().is_none_or(|b| score > b.0) {
            best = Some((score, leg, format!("the nearest victory point {province}")));
        }
    }
    best.map(|(_, leg, why)| Order::Strike(leg, why))
}

/// The names a plan row may use for an army: its own and the layout roles bound to it.
fn army_names(name: &str, layout: &[(String, String)]) -> Vec<String> {
    let mut names = vec![name.to_string()];
    names.extend(
        layout
            .iter()
            .filter(|(_, army)| army == name)
            .map(|(hint, _)| hint.clone()),
    );
    names
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
    for (province, (mine, _)) in &vp {
        if *mine {
            state.strike.had.insert(*province);
        }
    }
    if !state.strike.active {
        if held_points < knobs.after_vp {
            return Ok(None);
        }
        state.strike.active = true;
    }
    let mine = tools.call("hoi4_states", &json!({"query": me}))?;
    let mut own_states: HashSet<i64> = HashSet::new();
    let mut lost_own: HashSet<i64> = HashSet::new();
    for row in list(&mine, "states") {
        if let Some(id) = row["state_id"].as_i64() {
            all_states.push(id);
            if text(&row["controller"]) == me {
                own_states.insert(id);
            }
            for other in list(row, "provinces_held_by_others") {
                if enemies.contains(text(&other["controller"])) {
                    lost_own.extend(other["province"].as_i64());
                }
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
                    supply: row["supply_percent"].as_f64().unwrap_or(-1.0),
                    need: row["supply_need"].as_f64().unwrap_or(0.0),
                })
        })
        .collect();
    let fed: HashMap<i64, (f64, f64)> = list(&supply, "provinces")
        .iter()
        .filter_map(|row| {
            Some((
                row["province_id"].as_i64()?,
                (
                    row["supply"].as_f64()?,
                    row["demand"].as_f64().unwrap_or(0.0),
                ),
            ))
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
    let mut act: Option<Act> = None;
    let mut stop: Option<String> = None;
    {
        let world = World {
            memory: &memory,
            vp,
            own_states,
            enemy_states,
            lost_own,
            supply: fed,
        };
        // A garrison holds its victory point while that point is at the front and the division
        // stands in it; then it goes back to its army's orders.
        let at: HashMap<&str, i64> = divisions
            .iter()
            .map(|d| (d.id.as_str(), d.province))
            .collect();
        let released: Vec<String> = memory
            .garrison
            .iter()
            .filter(|(id, p)| at.get(id.as_str()) != Some(*p) || !world.front(**p))
            .map(|(id, _)| id.clone())
            .collect();
        // Divisions out of a starving stack rest on fed land until rested or their time is up.
        let rested: Vec<String> = memory
            .parked
            .iter()
            .filter(|(id, until)| {
                **until <= now
                    || divisions
                        .iter()
                        .find(|d| &d.id == *id)
                        .is_none_or(|d| world.share(d) >= knobs.resume)
            })
            .map(|(id, _)| id.clone())
            .collect();
        let kept = |id: &str| {
            (memory.garrison.contains_key(id) && !released.iter().any(|r| r == id))
                || (memory.parked.contains_key(id) && !rested.iter().any(|r| r == id))
        };
        // Divisions other armies send on each objective (a strike sized to its supply leaves
        // room for another army's divisions, and only that much).
        let mut committed: HashMap<i64, Vec<(&str, usize)>> = HashMap::new();
        for (n, a) in memory.armies.iter().filter(|(_, a)| a.mode == Mode::Strike) {
            let size = if a.group.is_empty() {
                by_army.get(n).map_or(0, Vec::len)
            } else {
                a.group.len()
            };
            committed
                .entry(a.objective)
                .or_default()
                .push((n.as_str(), size));
        }
        let others_on = |p: i64, me: &str| -> usize {
            committed.get(&p).map_or(0, |v| {
                v.iter().filter(|(n, _)| *n != me).map(|(_, s)| s).sum()
            })
        };
        let mut changes: Vec<Change> = Vec::new();
        for (name, all) in &by_army {
            if all.len() < 2 || reserve.contains(name) || (!knobs.guards && guards.contains(name)) {
                continue;
            }
            let members: Vec<&Division> = all.iter().copied().filter(|d| !kept(&d.id)).collect();
            if members.is_empty() {
                continue;
            }
            let default = Army::default();
            let army = memory.armies.get(name).unwrap_or(&default);
            let mean = members.iter().map(|d| world.share(d)).sum::<f64>() / members.len() as f64;
            let mut mode = army.mode;
            let since = army.since;
            let objective = army.objective;
            // The divisions the last order went to.
            let group: Vec<&Division> = {
                let g: Vec<&Division> = members
                    .iter()
                    .copied()
                    .filter(|d| army.group.is_empty() || army.group.contains(&d.id))
                    .collect();
                if g.is_empty() {
                    members.clone()
                } else {
                    g
                }
            };
            // An executing battle plan would send the divisions back to their line.
            if executing.contains(name) && memory.stopped.get(name).is_none_or(|at| now - at >= DAY)
            {
                stop = Some(name.clone());
                act = Some(Act {
                    tool: "hoi4_army_plan",
                    args: json!({"army": name, "execute": false}),
                    line: format!(
                        "strike {name}: battle plan stopped, the army goes by objectives"
                    ),
                    army: name.clone(),
                    mode,
                    objective,
                    group: None,
                    garrison: None,
                    park: Vec::new(),
                    province: None,
                });
                break;
            }
            let mut ended: Option<(i64, u8)> = None;
            // The objective a strike has just turned away from (not chosen again this tick).
            let mut turned: Option<i64> = None;
            if mode == Mode::Strike {
                let taken = world.held(objective) || group.iter().any(|d| d.province == objective);
                let age = now - since;
                if taken {
                    mode = Mode::Idle;
                } else {
                    let (_, near) = world.beside(&group, objective, knobs.resume);
                    let worn = !near.is_empty()
                        && near.iter().map(|d| world.share(d)).sum::<f64>() / (near.len() as f64)
                            < knobs.rest;
                    let over = (worn && age >= 3 * DAY) || age >= knobs.max_days * DAY;
                    if over && !knobs.halt && worn {
                        // Worn out, no halt order: the attack goes on as it stands (the enemy
                        // wears too) and no new order is sent while the divisions recover; then
                        // the army chooses again. A second worn strike on it closes the
                        // objective (live 15 Nov 2026: the script that won kept its attacks;
                        // the step that turned away at once took nothing after March).
                        changes.push((
                            name.clone(),
                            Mode::Rest,
                            objective,
                            Some(now),
                            Some((objective, 1)),
                        ));
                        continue;
                    } else if over && !knobs.halt {
                        // Stood its whole time with divisions to spare: the objective holds;
                        // it is closed for the army and its next order replaces this one.
                        ended = Some((objective, 2));
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
                        act = Some(Act {
                            tool: "hoi4_army_move",
                            args: json!({"army": name, "halt": true}),
                            line: format!(
                                "strike {name}: halt before province {objective} after {} days (organisation {:.0}% of its best)",
                                age / DAY,
                                mean * 100.0
                            ),
                            army: name.clone(),
                            mode: Mode::Rest,
                            objective,
                            group: None,
                            garrison: None,
                            park: Vec::new(),
                            province: None,
                        });
                        break;
                    }
                    // The order was lost (a retreat, a battle plan): choose and send again.
                    if !over
                        && age >= 2 * DAY
                        && !group.iter().any(|d| d.heading == Some(objective))
                    {
                        mode = Mode::Idle;
                    }
                }
            }
            // A row of this army that was held and is lost again comes before everything else:
            // the army drops its strike or march and its first open row is that province.
            if matches!(mode, Mode::Strike | Mode::March) {
                let lost = rows(plan, &army_names(name, &layout))
                    .into_iter()
                    .map(|(p, _)| p)
                    .find(|p| world.memory.had.contains(p) && !world.held(*p));
                let own_beside = world.lost_own.iter().any(|p| {
                    *p != objective && !world.beside(&members, *p, knobs.resume).1.is_empty()
                });
                if lost.is_some_and(|p| p != objective)
                    || (own_beside && !world.lost_own.contains(&objective))
                {
                    mode = Mode::Idle;
                }
            }
            if mode == Mode::March {
                let there = group.iter().filter(|d| d.province == objective).count();
                if there * 2 >= group.len() || now - since >= 25 * DAY {
                    mode = Mode::Idle;
                }
            }
            if mode == Mode::Rest
                && (mean >= knobs.resume
                    || now - since >= knobs.rest_days * DAY
                    || world.held(objective))
            {
                mode = Mode::Idle;
                ended = Some((objective, 0));
            }
            if mode != army.mode {
                changes.push((name.clone(), mode, objective, None, ended));
            }
            // A stack the province cannot feed: the most worn divisions go to held land near it
            // with room for them and rest there; the ones it feeds stay (in any mode).
            if knobs.feed {
                if let Some((from, ids, to)) =
                    thin(&world, &members, knobs.floor, &memory.thinned, now)
                {
                    if let Some(to_state) = world.state_of(to) {
                        act = Some(Act {
                            tool: "hoi4_army_move",
                            args: json!({"army": name, "state_id": to_state, "province_id": to, "divisions": ids}),
                            line: format!(
                                "strike {name}: {} of its divisions out of province {from} (it feeds fewer than stand there) to province {to}",
                                ids.len()
                            ),
                            army: name.clone(),
                            mode,
                            objective,
                            group: None,
                            garrison: None,
                            park: ids,
                            province: Some(from),
                        });
                        break;
                    }
                }
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
            // A garrison for a taken victory point at the front where the army stands: its most
            // worn division there stays (it remains in the army; the order goes to the others).
            let garrison = if knobs.garrison && members.len() >= 3 {
                let mut by_share = members.clone();
                by_share.sort_by(|a, b| world.share(a).total_cmp(&world.share(b)));
                by_share
                    .iter()
                    .find(|d| {
                        world.vp.get(&d.province).is_some_and(|v| v.0)
                            && world.front(d.province)
                            && !memory.garrison.values().any(|p| *p == d.province)
                            && !members.iter().any(|o| {
                                o.id != d.id
                                    && o.province == d.province
                                    && o.heading.is_none()
                                    && kept(&o.id)
                            })
                    })
                    .map(|d| (d.id.clone(), d.province))
            } else {
                None
            };
            let movers: Vec<&Division> = members
                .iter()
                .copied()
                .filter(|d| garrison.as_ref().is_none_or(|(id, _)| id != &d.id))
                .collect();
            let names = army_names(name, &layout);
            let need = need_of(&movers);
            // An objective whose land already feeds no more divisions is full for this army.
            let mut full: HashSet<i64> = HashSet::new();
            let mut order = None;
            for _ in 0..4 {
                let shut: HashSet<i64> = closed.union(&full).copied().collect();
                let Some(o) = choose(&world, &knobs, &rows(plan, &names), &movers, &shut) else {
                    break;
                };
                if let Order::Strike(p, _) = &o {
                    let others = others_on(*p, name);
                    if knobs.feed
                        && others > 0
                        && world.feeds(*p, need).is_some_and(|cap| others >= cap)
                    {
                        full.insert(*p);
                        continue;
                    }
                }
                order = Some(o);
                break;
            }
            let Some(order) = order else {
                continue;
            };
            let (province, why, next) = match order {
                Order::Strike(p, why) => (p, why, Mode::Strike),
                Order::March(p, why) => (p, why, Mode::March),
            };
            // Too worn to open a new strike: the army waits for its organisation (at most
            // rest_days); retaking own land does not wait.
            if next == Mode::Strike
                && mean < knobs.worn_gate
                && now - since < knobs.rest_days * DAY
                && !why.starts_with("own land")
            {
                continue;
            }
            let Some(target_state) = world.state_of(province) else {
                continue;
            };
            // As many divisions as the land at the objective feeds: those beside it and the
            // freshest first; the rest keep where they stand.
            let mut sent = movers.clone();
            if knobs.feed && next == Mode::Strike {
                let others = others_on(province, name);
                if let Some(cap) = world.feeds(province, need) {
                    let room = cap.saturating_sub(others).max(1);
                    if room < sent.len() {
                        sent.sort_by(|a, b| {
                            let near = |d: &Division| world.touches(province, d.province);
                            near(b)
                                .cmp(&near(a))
                                .then(world.share(b).total_cmp(&world.share(a)))
                        });
                        sent.truncate(room);
                    }
                }
            }
            let verb = if next == Mode::Strike {
                "on"
            } else {
                "marches to"
            };
            let mut args = json!({"army": name, "state_id": target_state, "province_id": province});
            let part = sent.len() < all.len();
            if part {
                args["divisions"] = json!(sent.iter().map(|d| d.id.clone()).collect::<Vec<_>>());
            }
            act = Some(Act {
                tool: "hoi4_army_move",
                args,
                line: format!(
                    "strike {name} {verb} province {province} ({why}; {} of {} divisions{}, organisation {:.0}% of their best)",
                    sent.len(),
                    all.len(),
                    garrison
                        .as_ref()
                        .map(|(_, p)| format!(", one holds {p}"))
                        .unwrap_or_default(),
                    mean * 100.0
                ),
                army: name.clone(),
                mode: next,
                objective: province,
                group: Some(if part {
                    sent.iter().map(|d| d.id.clone()).collect()
                } else {
                    Vec::new()
                }),
                garrison,
                park: Vec::new(),
                province: None,
            });
            break;
        }
        for id in released {
            memory.garrison.remove(&id);
        }
        for id in rested {
            memory.parked.remove(&id);
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
                    }
                }
            }
        }
    }
    if let Some(name) = stop {
        memory.stopped.insert(name, now);
    }
    state.strike = memory;
    let Some(a) = act else {
        return Ok(Some(None));
    };
    let (ok, why) = state.act(tools, lease, a.tool, a.args, "ws")?;
    if let Some(from) = a.province {
        state.strike.thinned.insert(from, now + 3 * DAY);
        if ok {
            for id in &a.park {
                state
                    .strike
                    .parked
                    .insert(id.clone(), now + knobs.rest_days * DAY);
            }
        }
        return Ok(Some(Some(format!("{}: {ok} {why}", a.line))));
    }
    let army = state.strike.armies.entry(a.army).or_default();
    if a.tool == "hoi4_army_move" && a.mode != Mode::Rest {
        if ok {
            army.mode = a.mode;
            army.objective = a.objective;
            army.since = now;
            army.group = a.group.unwrap_or_default();
            if let Some((id, p)) = a.garrison {
                state.strike.garrison.insert(id, p);
            }
        } else {
            // An order the game did not take: another objective for two days.
            army.closed.insert(a.objective, now + 2 * DAY);
        }
    }
    Ok(Some(Some(format!("{}: {ok} {why}", a.line))))
}

/// The one order of a tick and what it changes in the step's memory.
struct Act {
    tool: &'static str,
    args: Value,
    line: String,
    army: String,
    mode: Mode,
    objective: i64,
    /// the divisions a strike or march went to (empty: the whole army)
    group: Option<Vec<String>>,
    /// a division left to hold a taken victory point
    garrison: Option<(String, i64)>,
    /// divisions sent out of a starving stack, and that stack's province
    park: Vec<String>,
    province: Option<i64>,
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
        /// province 10 of the player's own state is held by the enemy
        lost_own: Cell<bool>,
        calls: RefCell<Vec<(String, Value)>>,
        /// hoi4_supply province rows (supply, demand of own divisions there)
        provinces: RefCell<Vec<Value>>,
        /// every division's supply percent (negative: not read)
        pct: Cell<f64>,
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
                lost_own: Cell::new(false),
                calls: RefCell::new(vec![]),
                provinces: RefCell::new(vec![]),
                pct: Cell::new(-1.0),
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
                        if self.lost_own.get() {
                            rows[0]["provinces_held_by_others"] =
                                json!([{"province": 10, "controller": "ENE"}]);
                        }
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
                "hoi4_supply" => json!({"divisions": self.divisions.borrow().iter().map(|d| {
                    let mut row = json!({"id": d.0, "army": d.1, "province_id": d.2,
                        "organisation": d.3, "heading_province": d.4, "supply_need": 1.0});
                    if self.pct.get() >= 0.0 {
                        row["supply_percent"] = json!(self.pct.get());
                    }
                    row
                }).collect::<Vec<_>>(), "provinces": self.provinces.borrow().clone()}),
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
    fn a_worn_strike_keeps_its_attack_rests_and_closes_after_a_second_one() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!(true));
        run(&mut state, &map, &on, START);
        // worn out on day four: no new order, no halt; the attack goes on as it stands
        map.set(10.0, Some(20));
        assert_eq!(run(&mut state, &map, &on, START + 4 * DAY), Some(None));
        assert_eq!(map.actions().len(), 1);
        // rested (organisation back): the same objective again (the game takes an order the
        // divisions already follow without input)
        map.set(36.0, Some(20));
        let line = run(&mut state, &map, &on, START + 6 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("on province 20"), "{line}");
        // worn out a second time: closed, and the next choice is another objective
        map.set(10.0, Some(20));
        assert_eq!(run(&mut state, &map, &on, START + 11 * DAY), Some(None));
        map.set(36.0, None);
        let line = run(&mut state, &map, &on, START + 22 * DAY)
            .unwrap()
            .unwrap();
        assert!(!line.contains("on province 20"), "{line}");
        assert!(map.actions().iter().all(|(_, a)| a.get("halt").is_none()));
    }

    #[test]
    fn a_worn_army_opens_no_new_strike_until_it_has_rested() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!(true));
        map.set(40.0, None);
        run(&mut state, &map, &on, START);
        map.taken.borrow_mut().push(20);
        map.stand(20);
        // the objective fell, the divisions are at a quarter of their best: they wait
        map.set(10.0, None);
        assert_eq!(run(&mut state, &map, &on, START + DAY), Some(None));
        map.set(30.0, None);
        let line = run(&mut state, &map, &on, START + 2 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("on province 30"), "{line}");
    }

    #[test]
    fn a_strike_goes_with_as_many_divisions_as_the_land_beside_it_feeds() {
        let map = Map::new(11);
        // province 11 receives 2.0 supply; each division draws 1.0
        map.provinces
            .borrow_mut()
            .push(json!({"province_id": 11, "supply": 2.0, "demand": 3.0}));
        let mut state = State::default();
        let line = run(&mut state, &map, &plan(json!(true)), START)
            .unwrap()
            .unwrap();
        assert!(line.contains("2 of 3 divisions"), "{line}");
        let (_, args) = map.actions().last().cloned().unwrap();
        assert_eq!(args["divisions"].as_array().map(Vec::len), Some(2));
        // feed: false sends the whole army
        let mut other = State::default();
        run(&mut other, &map, &plan(json!({"feed": false})), START);
        assert!(map.actions().last().unwrap().1.get("divisions").is_none());
    }

    #[test]
    fn a_starving_stack_sends_its_most_worn_divisions_to_fed_land_near_it() {
        let map = Map::new(20);
        map.taken.borrow_mut().push(20);
        map.pct.set(12.0);
        map.provinces.borrow_mut().extend([
            json!({"province_id": 20, "supply": 1.2, "demand": 3.0}),
            json!({"province_id": 11, "supply": 4.0, "demand": 1.0}),
        ]);
        map.divisions.borrow_mut()[0].3 = 5.0;
        let mut state = State::default();
        let on = plan(json!({"garrison": false}));
        let line = run(&mut state, &map, &on, START).unwrap().unwrap();
        assert!(
            line.contains("2 of its divisions out of province 20"),
            "{line}"
        );
        let (_, args) = map.actions().last().cloned().unwrap();
        assert_eq!(args["province_id"], json!(11));
        assert!(args["divisions"].as_array().unwrap().contains(&json!("d0")));
    }

    #[test]
    fn a_taken_victory_point_at_the_front_keeps_one_division() {
        let map = Map::new(11);
        let mut state = State::default();
        run(&mut state, &map, &plan(json!(true)), START);
        map.taken.borrow_mut().push(20);
        map.stand(20);
        map.set(40.0, None);
        map.divisions.borrow_mut()[2].3 = 24.0;
        let line = run(&mut state, &map, &plan(json!(true)), START + 2 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 30") && line.contains("one holds 20"),
            "{line}"
        );
        let (_, args) = map.actions().last().cloned().unwrap();
        let sent = args["divisions"].as_array().cloned().unwrap();
        assert_eq!(sent.len(), 2);
        assert!(!sent.contains(&json!("d2")));
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

    #[test]
    fn a_row_lost_again_is_retaken_before_the_strike_goes_on() {
        let map = Map::new(11);
        let mut state = State::default();
        let two = json!({"war": {"enemies": ["ENE"], "theatre": [1, 4], "strike": true},
            "vp_targets": [{"province": 20, "state": 2, "army": "Army 1"},
                           {"province": 30, "state": 3, "army": "Army 1"}]});
        run(&mut state, &map, &two, START);
        map.taken.borrow_mut().push(20);
        map.stand(21);
        let line = run(&mut state, &map, &two, START + 5 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("on province 30"), "{line}");
        // the enemy takes 20 back behind the army: the strike on 30 is dropped for it
        map.taken.borrow_mut().clear();
        map.set(38.0, Some(30));
        let line = run(&mut state, &map, &two, START + 7 * DAY)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 20") && line.contains("row 20"),
            "{line}"
        );
    }

    #[test]
    fn own_land_the_enemy_holds_beside_the_army_comes_first() {
        let map = Map::new(11);
        map.lost_own.set(true);
        let mut state = State::default();
        let line = run(&mut state, &map, &plan(json!(true)), START)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 10") && line.contains("own land"),
            "{line}"
        );
    }
}
