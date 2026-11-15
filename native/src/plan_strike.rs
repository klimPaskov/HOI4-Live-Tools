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
//!   feed        true: outside a strike, a stack its province cannot feed (`floor` 0.35 of
//!               the divisions' need) sends its most worn divisions to held land near it with
//!               room for them, where they rest (hoi4_army_move `divisions`); the army's next
//!               strike goes without them. A strike goes with the whole army (resting and
//!               garrison divisions apart); armies converge on one objective, but an army joins
//!               one that others strike only with as many divisions as the own land around it
//!               still feeds at the floor
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
//!   stack       4: at most this many divisions of the army strike from one province (the
//!               freshest, those beside the objective first). Combat width lets only a few fight
//!               from one direction and a deep stack starves: whole-army strikes stood 7-15
//!               deep at 40-60% supply while the game's own AI never put more than four in one
//!               province and kept 75-100% (live 15 Nov 2026). 0: no limit
//!   widen       true: while a strike runs, the divisions it left over take the enemy
//!               provinces beside the army, two at a time (those touching the objective first:
//!               a second direction for the next strike), and rejoin their army when the
//!               province is held, after `max_days`, or when the order was lost
//!   state_weight 2.0: outside the plan rows, a victory point that decides its state's control
//!               (the largest of a state the enemy controls) counts this many times its points.
//!               Taking states, not only points, is what turns some wars: Ethiopia 1936 gets a
//!               lower surrender limit once it has held half of its states or fewer for 90 days,
//!               which is why the game's own AI saw it capitulate at 16-18 of 32 points
//!   guards      true: guard armies are driven too (the border they held is no longer the
//!               front). Default false: they keep their fronts
//!   cover       1: divisions a formation keeps standing on each own state it covers (layout
//!               `covers`, `guard_states_in_order`, the states of its tasks, sized by
//!               `cover_counts`) while that state touches enemy land; a guard formation holds
//!               them with every division and stands still where nothing faces it (a front line
//!               follows the advance: a guard walked into unfed land and starved, and a strike
//!               army left two colonies empty and lost one, live 15 Nov 2026). 0: no posts
//!
//! Land that fed fewer than two divisions when own divisions last stood in it is remembered:
//! left-over divisions do not widen into it, an assembly there takes two divisions, and the way
//! to the nearest victory point counts it dearer.
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
    /// division id -> (the enemy province beside the army it was sent to take, since when): a
    /// division a strike left over widens the army's land instead of crowding the stack
    flank: HashMap<String, (i64, GameHour)>,
    /// the army the last order went to
    turn: Option<String>,
    /// division id -> (its army, the covered own state it holds, since when): a division kept on
    /// a state its formation covers while the rest of the army strikes elsewhere
    post: HashMap<String, (String, i64, GameHour)>,
    /// (army, state) -> until when a post order the game refused is not sent again
    post_wait: HashMap<(String, i64), GameHour>,
    /// province -> the supply it gave own divisions the last time they stood in it
    fed_seen: HashMap<i64, f64>,
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
    stack: usize,
    widen: bool,
    state_weight: f64,
    cover: usize,
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
        stack: number("stack", 4.0).max(0.0) as usize,
        widen: k["widen"] != json!(false),
        state_weight: number("state_weight", 2.0).max(1.0),
        cover: number("cover", 1.0).max(0.0) as usize,
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
    /// victory points that decide their state's control (the largest of a state the enemy
    /// controls): taking one turns the whole state
    decisive: HashSet<i64>,
    /// provinces that fed own divisions under the floor the last time they stood there
    unfed: HashSet<i64>,
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
                let step =
                    here + if held { 1 } else { 3 } + if self.unfed.contains(n) { 3 } else { 0 };
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
    /// How many divisions drawing `need` each the land at `target` feeds with at least `floor`
    /// of their need (under it they lose strength): the supply of the own provinces around it,
    /// where attackers stand (the objective's own once own divisions stand in it); at least
    /// two, the fewest that can attack. None when no own division stands beside it.
    fn feeds(&self, target: i64, need: f64, floor: f64) -> Option<usize> {
        if need <= 0.0 || floor <= 0.0 {
            return None;
        }
        // Attackers stand in the provinces around the objective, each fed by its own.
        let around: Vec<f64> = self
            .memory
            .graph
            .get(&target)
            .map(|row| {
                row.1
                    .iter()
                    .filter_map(|n| self.supply.get(n).map(|s| s.0))
                    .collect()
            })
            .unwrap_or_default();
        let supply = match self.supply.get(&target) {
            Some(s) => s.0,
            None if !around.is_empty() => around.iter().sum(),
            None => return None,
        };
        Some(((supply / (need * floor)).floor() as usize).max(2))
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

impl World<'_> {
    /// A covered own state the enemy can reach: the player controls it (or the enemy holds part
    /// of it) and one of its provinces touches land the enemy holds, or is held by the enemy.
    fn exposed(&self, state: i64) -> bool {
        let mine = self.own_states.contains(&state)
            || self
                .lost_own
                .iter()
                .any(|p| self.state_of(*p) == Some(state));
        mine && self
            .memory
            .graph
            .iter()
            .any(|(p, row)| row.0 == state && (self.lost_own.contains(p) || self.front(*p)))
    }
    /// Where a post of `state` stands: an own province the enemy holds there first (it is
    /// retaken), else the fed province of the state with the most enemy land around it.
    fn post_target(&self, state: i64) -> Option<i64> {
        let mut lost: Vec<i64> = self
            .lost_own
            .iter()
            .copied()
            .filter(|p| self.state_of(*p) == Some(state))
            .collect();
        lost.sort_unstable();
        if let Some(p) = lost.first() {
            return Some(*p);
        }
        self.memory
            .graph
            .iter()
            .filter(|(p, row)| row.0 == state && self.front(**p) && !self.unfed.contains(*p))
            .map(|(p, row)| {
                let open = row
                    .1
                    .iter()
                    .filter(|n| self.passable(**n) && !self.held(**n))
                    .count();
                (open, std::cmp::Reverse(*p))
            })
            .max()
            .map(|(_, p)| p.0)
    }
    /// Steps over held land from `from` to the nearest province of `state` (None: no way).
    fn steps_to_state(&self, from: i64, state: i64) -> Option<usize> {
        if self.state_of(from) == Some(state) {
            return Some(0);
        }
        let mut seen: HashSet<i64> = [from].into();
        let mut ring = vec![from];
        for step in 1..=40 {
            let mut next = Vec::new();
            for p in ring {
                for n in self
                    .memory
                    .graph
                    .get(&p)
                    .map(|r| r.1.as_slice())
                    .unwrap_or(&[])
                {
                    if !(self.held(*n) || self.lost_own.contains(n)) || !seen.insert(*n) {
                        continue;
                    }
                    if self.state_of(*n) == Some(state) {
                        return Some(step);
                    }
                    next.push(*n);
                }
            }
            if next.is_empty() {
                return None;
            }
            ring = next;
        }
        None
    }
}

/// What a formation keeps on the states it covers while it strikes elsewhere (a task such as a
/// border guard is divisions inside the formation, plan_formations.rs): a guard formation holds
/// them with all its divisions, any other with `cover_counts[state]` (the task's size) or the
/// `cover` knob per state. Army name -> (guard, [(state, divisions)]).
fn covers(
    plan: &Value,
    layout: &[(String, String)],
    knob: usize,
) -> HashMap<String, (bool, Vec<(i64, usize)>)> {
    let mut out: HashMap<String, (bool, Vec<(i64, usize)>)> = HashMap::new();
    for e in list(plan, "army_layout") {
        let hint = text(&e["name_hint"]);
        let Some((_, army)) = layout.iter().find(|(h, a)| h == hint && !a.is_empty()) else {
            continue;
        };
        let guard = text(&e["role"]) == "guard";
        let mut states: Vec<i64> = Vec::new();
        for key in ["guard_states_in_order", "covers"] {
            for s in list(e, key).iter().filter_map(Value::as_i64) {
                if !states.contains(&s) {
                    states.push(s);
                }
            }
        }
        if guard {
            if let Some(home) = e["state"].as_i64() {
                if !states.contains(&home) {
                    states.insert(0, home);
                }
            }
        }
        let entry = out.entry(army.clone()).or_insert((guard, Vec::new()));
        entry.0 |= guard;
        for s in states {
            let n = e["cover_counts"][s.to_string()]
                .as_u64()
                .map_or(knob, |n| n as usize);
            if n > 0 && !entry.1.iter().any(|(have, _)| *have == s) {
                entry.1.push((s, n));
            }
        }
    }
    out.retain(|_, (_, states)| !states.is_empty());
    out
}

impl World<'_> {
    /// How much more a victory point counts when it turns its whole state.
    fn state_weight(&self, province: i64, knobs: &Knobs) -> f64 {
        if self.decisive.contains(&province) {
            knobs.state_weight
        } else {
            1.0
        }
    }
    /// An enemy province for divisions a strike left over in `from`: one the army can attack
    /// from `from` or a held province beside it, not held, not `avoid`. Provinces touching the
    /// objective come first (a second direction for the next strike), then those touching `from`
    /// itself, then the one with the most held land around it (the shortest new front).
    fn widen_target(&self, from: i64, objective: i64, avoid: &HashSet<i64>) -> Option<i64> {
        let graph = &self.memory.graph;
        let mut bases = vec![from];
        bases.extend(self.near(from, 1));
        let mut best: Option<((bool, bool, usize), i64)> = None;
        for base in bases {
            for n in graph.get(&base).map(|r| r.1.as_slice()).unwrap_or(&[]) {
                if *n == objective
                    || self.held(*n)
                    || !self.passable(*n)
                    || avoid.contains(n)
                    || self.unfed.contains(n)
                {
                    continue;
                }
                let around = graph.get(n).map(|r| r.1.as_slice()).unwrap_or(&[]);
                let key = (
                    around.contains(&objective),
                    around.contains(&from),
                    around.iter().filter(|p| self.held(**p)).count(),
                );
                if best
                    .as_ref()
                    .is_none_or(|(k, p)| key > *k || (key == *k && *n < *p))
                {
                    best = Some((key, *n));
                }
            }
        }
        best.map(|(_, p)| p)
    }
}

/// The victory point of each enemy-controlled state that decides who controls it: the state's
/// largest (live 15 Nov 2026: a state turned to the player with its largest victory point
/// alone, and stayed the enemy's while it held that point, however many smaller ones were
/// lost). Losing states, not only points, is what some countries' surrender rules count.
fn decisive(
    vp: &BTreeMap<i64, (bool, f64)>,
    state_of: impl Fn(i64) -> Option<i64>,
    enemy_states: &HashSet<i64>,
) -> HashSet<i64> {
    let mut best: BTreeMap<i64, (f64, i64)> = BTreeMap::new();
    for (province, (_, points)) in vp {
        let Some(state) = state_of(*province).filter(|s| enemy_states.contains(s)) else {
            continue;
        };
        let e = best.entry(state).or_insert((*points, *province));
        if *points > e.0 {
            *e = (*points, *province);
        }
    }
    best.values().map(|(_, p)| *p).collect()
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
        // as many as get `floor` of their need there
        let keep = ((supply / (need * floor)).floor() as usize).max(1);
        if keep >= here.len() {
            continue;
        }
        here.sort_by(|a, b| world.share(a).total_cmp(&world.share(b)));
        let out: Vec<&Division> = here[..here.len() - keep].to_vec();
        let wanted: f64 = out.iter().map(|d| d.need.max(need)).sum();
        // Beside the stack only: a destination three provinces back emptied the front and the
        // towns it had taken were lost (live 15 Nov 2026).
        // Beside the stack first; one step further only when nothing beside it has room (an
        // unfed stretch two provinces deep kept a whole army starving where it stood).
        let ring = {
            let one = world.near(province, 1);
            let roomy = |p: &i64| world.supply.get(p).is_some_and(|s| s.0 - s.1 >= wanted);
            if one.iter().any(roomy) {
                one
            } else {
                world.near(province, 2)
            }
        };
        let Some(to) = ring
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

/// Posts released, new posts taken where divisions stand, and at most one order that sends
/// divisions to a covered state still short: (army, division ids, province, state).
type PostOrder = (String, Vec<String>, i64, i64);
#[allow(clippy::type_complexity)]
fn posts(
    world: &World,
    memory: &Memory,
    cover: &HashMap<String, (bool, Vec<(i64, usize)>)>,
    by_army: &BTreeMap<String, Vec<&Division>>,
    kept_base: &dyn Fn(&str) -> bool,
    now: GameHour,
) -> (Vec<String>, Vec<(String, String, i64)>, Option<PostOrder>) {
    let mut released: Vec<String> = Vec::new();
    for (id, (army, st, since)) in &memory.post {
        let division = by_army
            .get(army)
            .and_then(|ds| ds.iter().find(|d| &d.id == id));
        let covered = cover
            .get(army)
            .is_some_and(|(_, states)| states.iter().any(|(s, _)| s == st));
        let guard = cover.get(army).is_some_and(|(g, _)| *g);
        let gone = match division {
            None => true,
            Some(d) => {
                // sent but never arrived (a lost order): free again after 20 days
                world.state_of(d.province) != Some(*st)
                    && d.heading.is_none()
                    && now - since >= 20 * DAY
            }
        };
        if gone || !covered || (!guard && !world.exposed(*st)) {
            released.push(id.clone());
        }
    }
    let mut new: Vec<(String, String, i64)> = Vec::new();
    let mut order: Option<PostOrder> = None;
    let mut armies: Vec<&String> = cover.keys().collect();
    armies.sort();
    for army in armies {
        let (guard, states) = &cover[army];
        let Some(divs) = by_army.get(army) else {
            continue;
        };
        let mut moved: Vec<String> = Vec::new();
        let post_of = |id: &str| -> Option<i64> {
            memory
                .post
                .get(id)
                .filter(|_| !released.iter().any(|r| r == id))
                .map(|(_, s, _)| *s)
        };
        let exposed: Vec<(i64, usize)> = states
            .iter()
            .copied()
            .filter(|(s, _)| world.exposed(*s))
            .collect();
        let mut free: Vec<&Division> = divs
            .iter()
            .copied()
            .filter(|d| !kept_base(&d.id) && post_of(&d.id).is_none())
            .collect();
        let mut held_count = divs.iter().filter(|d| post_of(&d.id).is_some()).count();
        if *guard && exposed.is_empty() {
            // nothing to face: a guard holds the covered own states where it stands
            for d in free {
                if let Some(s) = world.state_of(d.province) {
                    if states.iter().any(|(c, _)| *c == s) && world.own_states.contains(&s) {
                        new.push((d.id.clone(), army.clone(), s));
                    }
                }
            }
            continue;
        }
        // a strike army keeps at least two divisions to strike with
        let budget = if *guard {
            divs.len()
        } else {
            divs.len().saturating_sub(2)
        };
        let per_guard = divs.len().div_ceil(exposed.len().max(1)).max(1);
        // a guard's posts on states nothing faces any more are free for the exposed ones
        if *guard {
            for d in divs {
                if let Some(s) = post_of(&d.id) {
                    if !exposed.iter().any(|(e, _)| *e == s) {
                        free.push(d);
                        held_count -= 1;
                    }
                }
            }
        }
        let group: Vec<String> = memory
            .armies
            .get(army)
            .map(|a| a.group.clone())
            .unwrap_or_default();
        for (s, n) in exposed {
            let need = if *guard { per_guard } else { n };
            let mut have = divs.iter().filter(|d| post_of(&d.id) == Some(s)).count()
                + new
                    .iter()
                    .filter(|(_, a, st)| a == army && *st == s)
                    .count();
            // divisions already standing there take the post where they stand
            let mut i = 0;
            while i < free.len() && have < need && held_count < budget {
                if world.state_of(free[i].province) == Some(s) {
                    let d = free.remove(i);
                    if post_of(&d.id).is_some() {
                        moved.push(d.id.clone());
                    }
                    new.push((d.id.clone(), army.clone(), s));
                    have += 1;
                    held_count += 1;
                } else {
                    i += 1;
                }
            }
            if have >= need || held_count >= budget || order.is_some() {
                continue;
            }
            if memory
                .post_wait
                .get(&(army.clone(), s))
                .is_some_and(|until| *until > now)
            {
                continue;
            }
            let Some(target) = world.post_target(s) else {
                continue;
            };
            let mut near: Vec<(bool, usize, i64, &Division)> = free
                .iter()
                .filter_map(|d| {
                    world.steps_to_state(d.province, s).map(|steps| {
                        let worn = (world.share(d) * 100.0) as i64;
                        (group.contains(&d.id), steps, worn, *d)
                    })
                })
                .collect();
            near.sort_by(|a, b| (a.0, a.1, a.2, &a.3.id).cmp(&(b.0, b.1, b.2, &b.3.id)));
            let take = (need - have).min(budget - held_count);
            let ids: Vec<String> = near.iter().take(take).map(|n| n.3.id.clone()).collect();
            if ids.is_empty() {
                continue;
            }
            for id in &ids {
                if post_of(id).is_some() {
                    moved.push(id.clone());
                }
            }
            held_count += ids.len();
            free.retain(|d| !ids.contains(&d.id));
            order = Some((army.clone(), ids, target, s));
        }
        released.extend(moved);
    }
    (released, new, order)
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
            let score = points * weight * world.state_weight(*province, knobs);
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
        let score = points * world.state_weight(*province, knobs) / (1.0 + cost as f64);
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
    for (province, (supply, _)) in &fed {
        state.strike.fed_seen.insert(*province, *supply);
    }
    // Land that fed fewer than two divisions (the fewest that can attack) when own divisions
    // last stood in it: an advance, a widening or a post does not go there unless it must
    // (live 15-16 Nov 2026: three armies starved for months in a corridor at province supply 0).
    let one_need = {
        let mut needs: Vec<f64> = divisions
            .iter()
            .map(|d| d.need)
            .filter(|n| *n > 0.0)
            .collect();
        needs.sort_by(f64::total_cmp);
        needs.get(needs.len() / 2).copied().unwrap_or(0.5)
    };
    let unfed: HashSet<i64> = state
        .strike
        .fed_seen
        .iter()
        .filter(|(_, supply)| **supply < 2.0 * one_need)
        .map(|(p, _)| *p)
        .collect();
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
        let decisive = decisive(&vp, |p| memory.graph.get(&p).map(|r| r.0), &enemy_states);
        let world = World {
            memory: &memory,
            vp,
            own_states,
            enemy_states,
            lost_own,
            supply: fed,
            decisive,
            unfed,
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
        // A division sent to widen the army's land rejoins its army when the province is held,
        // when its time is up, or when it neither marches on it nor stands in it two days on.
        let rejoined: Vec<String> = memory
            .flank
            .iter()
            .filter(|(id, (p, since))| {
                divisions.iter().find(|d| &d.id == *id).is_none_or(|d| {
                    world.held(*p)
                        || now - since >= knobs.max_days * DAY
                        || (now - since >= 2 * DAY && d.heading != Some(*p) && d.province != *p)
                })
            })
            .map(|(id, _)| id.clone())
            .collect();
        let kept_base = |id: &str| {
            (memory.garrison.contains_key(id) && !released.iter().any(|r| r == id))
                || (memory.parked.contains_key(id) && !rested.iter().any(|r| r == id))
                || (memory.flank.contains_key(id) && !rejoined.iter().any(|r| r == id))
        };
        // Posts: each state a formation covers keeps its divisions while the rest strike (a
        // strike army that went north left Somaliland and Jubaland to the enemy, live 15 Nov
        // 2026; a guard's front line followed the advance into unfed land). A division already
        // standing in the state takes the post where it stands; else the nearest is sent.
        let cover = if knobs.cover > 0 {
            covers(plan, &layout, knobs.cover)
        } else {
            HashMap::new()
        };
        let (post_released, post_new, post_order) =
            posts(&world, &memory, &cover, &by_army, &kept_base, now);
        let kept = |id: &str| {
            kept_base(id)
                || (memory.post.contains_key(id) && !post_released.iter().any(|r| r == id))
                || post_new.iter().any(|(d, _, _)| d == id)
        };
        let flank_targets: HashSet<i64> = memory
            .flank
            .iter()
            .filter(|(id, _)| !rejoined.contains(id))
            .map(|(_, (p, _))| *p)
            .collect();
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
        if let Some((army, ids, province, post_state)) = post_order {
            act = Some(Act {
                tool: "hoi4_army_move",
                args: json!({"army": army, "state_id": post_state, "province_id": province, "divisions": ids}),
                line: format!(
                    "strike {army}: {} division(s) hold covered state {post_state} at province {province}",
                    ids.len()
                ),
                army: army.clone(),
                mode: Mode::Idle,
                objective: 0,
                group: None,
                garrison: None,
                park: Vec::new(),
                province: None,
                flank: None,
                post: Some((ids, post_state)),
            });
        }
        // One order a tick, the armies in turn after the one that had the last (an army whose
        // order keeps failing held every other army back for two weeks: live 15 Nov 2026).
        let names: Vec<&String> = by_army.keys().collect();
        let start = memory
            .turn
            .as_ref()
            .and_then(|t| names.iter().position(|n| *n > t))
            .unwrap_or(0);
        for name in names[start..].iter().chain(names[..start].iter()).copied() {
            if act.is_some() {
                break;
            }
            let all = &by_army[name];
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
                    flank: None,
                    post: None,
                });
                break;
            }
            let mut ended: Option<(i64, u8)> = None;
            // The objective a strike has just turned away from (not chosen again this tick).
            let mut turned: Option<i64> = None;
            if mode == Mode::Strike {
                // A victory point counts when it is held; another province when the army stands in
                // it (a division inside a victory point the enemy still holds is no capture, and
                // treating it as one re-sent the same order every few hours).
                let taken = world.held(objective)
                    || (!world.vp.contains_key(&objective)
                        && group.iter().any(|d| d.province == objective));
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
                            flank: None,
                            post: None,
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
            // Not while a strike runs: its attackers stand where they attack from.
            if knobs.feed && mode != Mode::Strike {
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
                            flank: None,
                            post: None,
                        });
                        break;
                    }
                }
            }
            // Divisions a strike left over (more than `stack` stood in one province) take the
            // enemy provinces beside the army, a few each, instead of starving in the stack: the
            // game's own AI held 11-15 provinces with 23-26 divisions, never more than four in
            // one, at 75-100% supply, while whole-army strikes stood 7-15 deep at 40-60%.
            if knobs.widen && matches!(mode, Mode::Strike | Mode::Rest) && !army.group.is_empty() {
                let mut spare: BTreeMap<i64, Vec<&Division>> = BTreeMap::new();
                for d in members.iter().copied().filter(|d| {
                    !army.group.contains(&d.id) && d.heading.is_none_or(|h| h == d.province)
                }) {
                    spare.entry(d.province).or_default().push(d);
                }
                let mut avoid: HashSet<i64> = army
                    .closed
                    .iter()
                    .filter(|(_, until)| **until > now)
                    .map(|(p, _)| *p)
                    .collect();
                avoid.extend(flank_targets.iter().copied());
                avoid.extend(committed.keys().copied());
                let mut widened = false;
                for (from, mut here) in spare {
                    let Some(to) = world.widen_target(from, objective, &avoid) else {
                        continue;
                    };
                    let Some(to_state) = world.state_of(to) else {
                        continue;
                    };
                    here.sort_by(|a, b| world.share(b).total_cmp(&world.share(a)));
                    here.truncate(2);
                    let ids: Vec<String> = here.iter().map(|d| d.id.clone()).collect();
                    act = Some(Act {
                        tool: "hoi4_army_move",
                        args: json!({"army": name, "state_id": to_state, "province_id": to, "divisions": ids}),
                        line: format!(
                            "strike {name}: {} left-over division(s) from province {from} take province {to} beside the army",
                            ids.len()
                        ),
                        army: name.clone(),
                        mode,
                        objective,
                        group: None,
                        garrison: None,
                        park: Vec::new(),
                        province: None,
                        flank: Some((ids, to)),
                        post: None,
                    });
                    widened = true;
                    break;
                }
                if widened {
                    break;
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
                        && world
                            .feeds(*p, need, knobs.floor)
                            .is_some_and(|cap| others >= cap)
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
            // The whole army strikes (a strike sized to the supply of one province beside the
            // objective sent two divisions at Harar for a month, live 15 Nov 2026; the stack is
            // thinned to what its province feeds once the strike is over). Another army joins
            // an objective others strike only with what the land around it still feeds.
            let mut sent = movers.clone();
            let others = others_on(province, name);
            if knobs.feed && next == Mode::Strike && others > 0 {
                if let Some(cap) = world.feeds(province, need, knobs.floor) {
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
            // At most `stack` divisions strike from one province (the freshest, those beside the
            // objective first); the rest widen the army's land once the strike runs.
            // A march to an assembly province takes at most `stack` divisions too (the nearest,
            // then the freshest): a whole army gathered on one province starved there (live 16
            // Nov 2026: ten divisions of one army in an unfed province for three months); the
            // others wait on the land they stand on until a strike sends them.
            if next == Mode::March && knobs.stack > 0 && sent.len() > knobs.stack {
                sent.sort_by(|a, b| {
                    let near = |d: &Division| {
                        world.touches(province, d.province) || d.province == province
                    };
                    near(b)
                        .cmp(&near(a))
                        .then(world.share(b).total_cmp(&world.share(a)))
                });
                sent.truncate(knobs.stack);
            }
            // An assembly province that fed fewer than two divisions last time takes two: the
            // rest wait on fed land until the strike sends them.
            if next == Mode::March && world.unfed.contains(&province) && sent.len() > 2 {
                sent.truncate(2);
            }
            if next == Mode::Strike && knobs.stack > 0 {
                sent.sort_by(|a, b| {
                    let near = |d: &Division| world.touches(province, d.province);
                    near(b)
                        .cmp(&near(a))
                        .then(world.share(b).total_cmp(&world.share(a)))
                });
                let mut per: HashMap<i64, usize> = HashMap::new();
                sent.retain(|d| {
                    let n = per.entry(d.province).or_insert(0);
                    *n += 1;
                    *n <= knobs.stack
                });
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
                flank: None,
                post: None,
            });
            break;
        }
        for id in released {
            memory.garrison.remove(&id);
        }
        for id in rested {
            memory.parked.remove(&id);
        }
        for id in rejoined {
            memory.flank.remove(&id);
        }
        for id in post_released {
            memory.post.remove(&id);
        }
        for (id, army, post_state) in post_new {
            memory.post.insert(id, (army, post_state, now));
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
    state.strike.turn = Some(a.army.clone());
    if let Some((ids, post_state)) = a.post {
        if ok {
            for id in ids {
                state
                    .strike
                    .post
                    .insert(id, (a.army.clone(), post_state, now));
            }
        } else {
            state
                .strike
                .post_wait
                .insert((a.army.clone(), post_state), now + 2 * DAY);
        }
        return Ok(Some(Some(format!("{}: {ok} {why}", a.line))));
    }
    if let Some((ids, to)) = a.flank {
        if ok {
            for id in ids {
                state.strike.flank.insert(id, (to, now));
            }
        } else {
            // A province the game would not send them to: another for two days.
            let army = state.strike.armies.entry(a.army).or_default();
            army.closed.insert(to, now + 2 * DAY);
        }
        return Ok(Some(Some(format!("{}: {ok} {why}", a.line))));
    }
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
    /// divisions sent to widen the army's land, and the enemy province they take
    flank: Option<(Vec<String>, i64)>,
    /// divisions sent to hold a covered state, and that state
    post: Option<(Vec<String>, i64)>,
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
        /// march orders of Army 1 are not applied
        refuse: Cell<bool>,
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
                refuse: Cell::new(false),
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
                if self.refuse.get() && name == "hoi4_army_move" && args["army"] == json!("Army 1")
                {
                    return Ok(json!({"receipt": {"status": "NotApplied"}}));
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
    fn an_army_joins_an_objective_others_strike_only_with_what_its_land_feeds() {
        let map = Map::new(11);
        map.divisions
            .borrow_mut()
            .extend((3..6).map(|n| (format!("d{n}"), "Army 2".to_string(), 11, 40.0, None)));
        // province 11 receives 1.05 supply; each division draws 1.0: three get 35% of it
        map.provinces
            .borrow_mut()
            .push(json!({"province_id": 11, "supply": 1.05, "demand": 6.0}));
        let mut state = State::default();
        let first = run(&mut state, &map, &plan(json!(true)), START)
            .unwrap()
            .unwrap();
        // the first army goes whole
        assert!(
            first.contains("Army 1 on province 20") && first.contains("3 of 3"),
            "{first}"
        );
        let second = run(&mut state, &map, &plan(json!(true)), START + 1)
            .unwrap()
            .unwrap();
        // the land feeds three, the first army sends three: the second turns elsewhere
        assert!(
            second.contains("Army 2") && !second.contains("on province 20"),
            "{second}"
        );
    }

    #[test]
    fn a_starving_stack_sends_its_most_worn_divisions_to_fed_land_near_it() {
        let map = Map::new(20);
        map.taken.borrow_mut().push(20);
        map.pct.set(12.0);
        map.provinces.borrow_mut().extend([
            json!({"province_id": 20, "supply": 0.5, "demand": 3.0}),
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
    fn an_army_whose_orders_fail_does_not_hold_the_others_back() {
        let map = Map::new(11);
        map.divisions
            .borrow_mut()
            .extend((3..5).map(|n| (format!("d{n}"), "Army 2".to_string(), 11, 40.0, None)));
        map.refuse.set(true);
        let mut state = State::default();
        let on = plan(json!(true));
        let first = run(&mut state, &map, &on, START).unwrap().unwrap();
        assert!(first.contains("strike Army 1"), "{first}");
        let second = run(&mut state, &map, &on, START + 1).unwrap().unwrap();
        assert!(second.contains("strike Army 2"), "{second}");
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

    #[test]
    fn a_strike_sends_at_most_stack_divisions_from_one_province_and_the_rest_widen() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!({"stack": 2}));
        let line = run(&mut state, &map, &on, START).unwrap().unwrap();
        assert!(
            line.contains("on province 20") && line.contains("2 of 3"),
            "{line}"
        );
        let (_, args) = map.actions().last().cloned().unwrap();
        let sent = args["divisions"].as_array().cloned().unwrap();
        assert_eq!(sent.len(), 2);
        let spare = ["d0", "d1", "d2"]
            .into_iter()
            .find(|d| !sent.contains(&json!(d)))
            .unwrap();
        // the strike runs: the division it left over takes the enemy province beside the army
        // that also touches the objective (a second direction)
        for d in map.divisions.borrow_mut().iter_mut() {
            if sent.contains(&json!(d.0)) {
                d.4 = Some(20);
            }
        }
        let line = run(&mut state, &map, &on, START + DAY).unwrap().unwrap();
        assert!(line.contains("take province 21"), "{line}");
        let (_, args) = map.actions().last().cloned().unwrap();
        assert_eq!(args["divisions"], json!([spare]));
        assert_eq!(args["province_id"], json!(21));
        // marching on it, nothing more is sent
        for d in map.divisions.borrow_mut().iter_mut() {
            if d.0 == spare {
                d.4 = Some(21);
            }
        }
        assert_eq!(run(&mut state, &map, &on, START + 2 * DAY), Some(None));
    }

    fn covered(role: &str) -> Value {
        let mut p = plan(json!(true));
        p["army_layout"] = json!([{"name_hint": "Army 1", "role": role, "covers": [1]}]);
        p
    }

    #[test]
    fn a_covered_state_keeps_a_division_where_it_stands_and_the_rest_strike() {
        let map = Map::new(11);
        let mut state = State::default();
        state.layout.insert("Army 1".into(), "Army 1".into());
        let line = run(&mut state, &map, &covered("strike"), START)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 20") && line.contains("2 of 3"),
            "{line}"
        );
        let (_, args) = map.actions().last().cloned().unwrap();
        let sent = args["divisions"].as_array().cloned().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(state.strike.post.len(), 1);
        let (army, post_state, _) = state.strike.post.values().next().cloned().unwrap();
        assert_eq!((army.as_str(), post_state), ("Army 1", 1));
    }

    #[test]
    fn an_army_that_left_its_covered_state_sends_a_division_back() {
        let map = Map::new(11);
        map.taken.borrow_mut().push(20);
        map.stand(20);
        let mut state = State::default();
        state.layout.insert("Army 1".into(), "Army 1".into());
        let line = run(&mut state, &map, &covered("strike"), START)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("hold covered state 1 at province 11"),
            "{line}"
        );
        let (_, args) = map.actions().last().cloned().unwrap();
        assert_eq!(args["divisions"].as_array().map(Vec::len), Some(1));
        assert_eq!(args["state_id"], json!(1));
        assert_eq!(state.strike.post.len(), 1);
        // the next order is the strike, without the post
        let line = run(&mut state, &map, &covered("strike"), START + DAY)
            .unwrap()
            .unwrap();
        assert!(
            line.contains("on province 30") && line.contains("2 of 3"),
            "{line}"
        );
    }

    #[test]
    fn a_guard_formation_holds_its_covered_state_with_every_division() {
        let map = Map::new(11);
        let mut state = State::default();
        state.layout.insert("Army 1".into(), "Army 1".into());
        assert_eq!(run(&mut state, &map, &covered("guard"), START), Some(None));
        assert!(map.actions().is_empty());
        assert_eq!(state.strike.post.len(), 3);
    }

    #[test]
    fn left_over_divisions_do_not_widen_into_land_that_fed_too_little() {
        let map = Map::new(11);
        map.provinces
            .borrow_mut()
            .push(json!({"province_id": 21, "supply": 0.2, "demand": 3.0}));
        let mut state = State::default();
        let on = plan(json!({"stack": 2}));
        let line = run(&mut state, &map, &on, START).unwrap().unwrap();
        assert!(line.contains("2 of 3"), "{line}");
        let (_, args) = map.actions().last().cloned().unwrap();
        let sent = args["divisions"].as_array().cloned().unwrap();
        for d in map.divisions.borrow_mut().iter_mut() {
            if sent.contains(&json!(d.0)) {
                d.4 = Some(20);
            }
        }
        assert_eq!(run(&mut state, &map, &on, START + DAY), Some(None));
    }

    #[test]
    fn a_flank_division_rejoins_its_army_when_its_province_is_held() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!({"stack": 2, "garrison": false}));
        run(&mut state, &map, &on, START);
        map.set(40.0, Some(20));
        map.divisions.borrow_mut()[2].4 = None;
        run(&mut state, &map, &on, START + DAY);
        assert!(state.strike.flank.contains_key("d2"));
        // it stands in 21 (held now) and the objective falls: the next strike takes it along
        map.divisions.borrow_mut()[2].2 = 21;
        map.divisions.borrow_mut()[2].4 = None;
        map.taken.borrow_mut().push(20);
        for d in map.divisions.borrow_mut().iter_mut().take(2) {
            d.2 = 20;
            d.4 = None;
        }
        let line = run(&mut state, &map, &on, START + 4 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("on province 30"), "{line}");
        assert!(!state.strike.flank.contains_key("d2"));
        let (_, args) = map.actions().last().cloned().unwrap();
        assert!(args.get("divisions").is_none(), "{args}");
    }

    #[test]
    fn without_widen_the_left_over_divisions_wait_in_their_province() {
        let map = Map::new(11);
        let mut state = State::default();
        let on = plan(json!({"stack": 2, "widen": false}));
        run(&mut state, &map, &on, START);
        map.set(40.0, Some(20));
        map.divisions.borrow_mut()[2].4 = None;
        assert_eq!(run(&mut state, &map, &on, START + DAY), Some(None));
    }

    #[test]
    fn the_largest_victory_point_of_an_enemy_state_decides_it() {
        let vp: BTreeMap<i64, (bool, f64)> = [
            (20, (false, 1.0)),
            (21, (false, 3.0)),
            (30, (true, 5.0)),
            (50, (false, 2.0)),
        ]
        .into_iter()
        .collect();
        let state = |p: i64| Some(p / 10);
        let enemy: HashSet<i64> = [2, 3].into_iter().collect();
        let got = decisive(&vp, state, &enemy);
        let want: HashSet<i64> = [21, 30].into_iter().collect();
        assert_eq!(got, want);
    }

    #[test]
    fn a_march_to_an_assembly_province_takes_at_most_stack_divisions() {
        let map = Map::new(10);
        let mut state = State::default();
        let on = json!({"war": {"enemies": ["ENE"], "theatre": [1, 4], "strike": {"stack": 2}},
            "vp_targets": [{"province": 30, "state": 3, "army": "Army 1", "path": [11, 21]}]});
        let line = run(&mut state, &map, &on, START).unwrap().unwrap();
        assert!(
            line.contains("marches to province 11") && line.contains("2 of 3"),
            "{line}"
        );
    }
}
