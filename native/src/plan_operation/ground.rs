//! The operation's ground model (pure): what the provinces of a war let each division do.
//!
//! Every number comes from reads the project already has: `hoi4_province_graph` (terrain with
//! its combat width, support width per extra direction, attack modifier and movement cost from
//! `common/terrain`; rivers on each border with the river defines' penalty; the forts of the
//! state history), `hoi4_template_get` (a template's width), `hoi4_battles` (the width, the
//! directions and the divisions engaged and waiting in reserve of every battle we fight),
//! `hoi4_states` (infrastructure of the states our country sees) and `hoi4_wars` (the hostile
//! divisions standing in the border states, what the map shows beside our line). Nothing names a
//! country, a state or a terrain: a mod's terrain with its own widths is read the same way.
//!
//! - WIDTH FIT (`best_fit`): how many divisions of one width a battle takes with profit. The
//!   game's over-width penalty is one percent of attack and defence per percent over the width,
//!   and a division that would push the line more than a third over does not join
//!   (COMBAT_OVER_WIDTH_PENALTY -1, COMBAT_OVER_WIDTH_PENALTY_MAX -0.33); the stacking penalty is
//!   2% a division above 5 plus 3 per extra direction (COMBAT_STACKING_*). The fit is the number
//!   of divisions whose line strength (divisions times what the penalties leave) is highest.
//! - PROVINCE CAPS (`province_caps`): for each own province facing the enemy, the divisions that
//!   can fight from it: every enemy province it touches takes `best_fit` of its width with as many
//!   directions as own provinces touch it, shared among those provinces.
//! - BATTLE SURPLUS (`battle_surplus`): the divisions waiting in a battle's reserve beyond what
//!   the plan keeps there (the measured overstack: 19 of 23 in one forest attack).
//! - THE AXIS (`axis`): the cheapest province path from a formation's start to its objective by a
//!   general cost per step: movement cost, the assault penalty of the step (terrain attack, the
//!   river crossed, the forts), how narrow the terrain is against the widest of the theatre, the
//!   infrastructure the state has and the hostile divisions our country sees there. Armour pays
//!   more for rough, narrow and fortified ground than infantry does.
//! - LINE PROVINCES (`line_provinces`): the provinces of a line of states as one chain of
//!   neighbours, in drawing order (the in-process set-path and spearhead commands take provinces).
use super::{ints, list, text};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

/// The over-width penalty per unit of overshoot (COMBAT_OVER_WIDTH_PENALTY -1 per percent).
pub(super) const OVER_WIDTH_PENALTY: f64 = 1.0;
/// The overshoot past which no division joins (COMBAT_OVER_WIDTH_PENALTY_MAX -0.33).
pub(super) const OVER_WIDTH_MAX: f64 = 0.33;
/// Divisions in one battle before the stacking penalty (COMBAT_STACKING_START), the extra per
/// direction (COMBAT_STACKING_EXTRA) and the penalty per division over (COMBAT_STACKING_PENALTY).
pub(super) const STACKING_START: f64 = 5.0;
pub(super) const STACKING_EXTRA: f64 = 3.0;
pub(super) const STACKING_PENALTY: f64 = 0.02;
/// The attack penalty counted per level of a fort in the province's history (as the assault
/// guard counts it).
pub(super) const FORT_STEP: f64 = -0.15;
/// The width a division is taken to have when no read gives it (a common line infantry width).
pub(super) const WIDTH_UNKNOWN: f64 = 20.0;

/// One land province as the graph read gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Prov {
    pub state: i64,
    pub terrain: String,
    /// combat width and support width per extra direction (0 when the read has none)
    pub width: f64,
    pub support: f64,
    /// the terrain's attack modifier (0 or below)
    pub attack: f64,
    pub movement: f64,
    /// the highest bunker level of its state history
    pub forts: i64,
    /// (neighbour province, the attack penalty of the river on that border, 0 or below)
    pub borders: Vec<(i64, f64)>,
}

/// The provinces of a `hoi4_province_graph` read (impassable borders left out).
pub(super) fn read_graph(read: &Value) -> HashMap<i64, Prov> {
    let mut out = HashMap::new();
    for row in list(read, "provinces") {
        let Some(id) = row["province"].as_i64() else {
            continue;
        };
        let forts = list(row, "forts_in_history")
            .iter()
            .filter(|f| text(&f["building"]) == "bunker")
            .filter_map(|f| f["level"].as_i64())
            .max()
            .unwrap_or(0);
        let borders = list(row, "borders")
            .iter()
            .filter(|b| {
                !list(b, "special")
                    .iter()
                    .any(|s| text(&s["kind"]) == "impassable")
            })
            .filter_map(|b| {
                let q = b["province"].as_i64()?;
                let river = if b["river_share"].as_f64().unwrap_or(0.0) >= 0.5 {
                    b["river_attack_penalty"].as_f64().unwrap_or(0.0).min(0.0)
                } else {
                    0.0
                };
                Some((q, river))
            })
            .collect();
        out.insert(
            id,
            Prov {
                state: row["state_id"].as_i64().unwrap_or(0),
                terrain: text(&row["terrain"]).to_string(),
                width: row["combat_width"].as_f64().unwrap_or(0.0),
                support: row["combat_support_width"].as_f64().unwrap_or(0.0),
                attack: row["attack_modifier"].as_f64().unwrap_or(0.0).min(0.0),
                movement: row["movement_cost"].as_f64().unwrap_or(1.0).max(0.1),
                forts,
                borders,
            },
        );
    }
    out
}

/// The attacker's width of a battle in `p` from `directions` sides.
pub(super) fn battle_width(p: &Prov, directions: usize) -> f64 {
    p.width + p.support * (directions.max(1) as f64 - 1.0)
}

/// The number of divisions of width `w` a battle of width `width` from `directions` sides takes
/// with profit: the k whose line strength k x (1 - over-width penalty) x (1 - stacking penalty)
/// is highest, no division past the over-width cap; at least one. Pure.
pub(super) fn best_fit(width: f64, directions: usize, w: f64) -> i64 {
    if width <= 0.0 || w <= 0.0 {
        return 1;
    }
    let free = STACKING_START + STACKING_EXTRA * (directions.max(1) as f64 - 1.0);
    let (mut best, mut value) = (1, 0.0);
    for k in 1..=64 {
        let total = k as f64 * w;
        let over = ((total - width) / width).max(0.0);
        if k > 1 && over > OVER_WIDTH_MAX + 1e-9 {
            break;
        }
        let stack = (1.0 - STACKING_PENALTY * (k as f64 - free).max(0.0)).max(0.0);
        let v = k as f64 * (1.0 - OVER_WIDTH_PENALTY * over).max(0.0) * stack;
        if v > value + 1e-9 {
            best = k;
            value = v;
        }
    }
    best
}

/// What one own front province can feed into battles: the divisions (`cap`) and the enemy
/// provinces it faces.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Cap {
    pub cap: i64,
    pub facing: Vec<i64>,
}

/// The divisions each own front province can fight with: for every enemy province touching one
/// of `front` (own provinces), `best_fit` of its width with as many directions as own front
/// provinces touch it, shared equally among those provinces (rounded up), plus `keep` (reserve
/// the plan keeps in each province). `enemy` says whether a province is the enemy's, `width_at`
/// the division width that stands in an own province. Provinces of `front` with no enemy
/// neighbour are left out (the supply carry alone limits them). Pure.
pub(super) fn province_caps(
    g: &HashMap<i64, Prov>,
    front: &HashSet<i64>,
    enemy: &dyn Fn(i64) -> bool,
    width_at: &dyn Fn(i64) -> f64,
    keep: i64,
) -> HashMap<i64, Cap> {
    // enemy province -> the own front provinces it can be attacked from
    let mut from: HashMap<i64, Vec<i64>> = HashMap::new();
    for p in front {
        let Some(row) = g.get(p) else { continue };
        for (q, _) in &row.borders {
            if enemy(*q) && g.contains_key(q) {
                from.entry(*q).or_default().push(*p);
            }
        }
    }
    let mut share: HashMap<i64, (f64, Vec<i64>)> = HashMap::new();
    let mut targets: Vec<&i64> = from.keys().collect();
    targets.sort();
    for e in targets {
        let sides = &from[e];
        let mut sides = sides.clone();
        sides.sort_unstable();
        sides.dedup();
        let w = sides.iter().map(|p| width_at(*p)).sum::<f64>() / sides.len() as f64;
        let fit = best_fit(battle_width(&g[e], sides.len()), sides.len(), w) as f64;
        for p in &sides {
            let s = share.entry(*p).or_default();
            s.0 += fit / sides.len() as f64;
            s.1.push(*e);
        }
    }
    share
        .into_iter()
        .map(|(p, (n, facing))| {
            (
                p,
                Cap {
                    cap: (n - 1e-9).ceil().max(1.0) as i64 + keep.max(0),
                    facing,
                },
            )
        })
        .collect()
}

/// Per army the divisions to take out, and the worst battle (province, engaged, reserve).
pub(super) type BattleSurplus = (Vec<(String, Vec<String>)>, Option<(i64, i64, i64)>);

/// The surplus a battle read shows: in each battle we attack, the divisions waiting in its
/// reserve beyond `keep` (the freshest are kept), per army of `pick` (division id -> army from
/// `army_of`). Returns (army, division ids) and the worst battle (province, engaged, reserve). Pure.
pub(super) fn battle_surplus(
    read: &Value,
    army_of: &HashMap<String, String>,
    pick: &HashSet<String>,
    keep: i64,
) -> BattleSurplus {
    let mut per: HashMap<String, Vec<String>> = HashMap::new();
    let mut worst: Option<(i64, i64, i64)> = None;
    for b in list(read, "list") {
        if text(&b["our_side"]) != "attacker" {
            continue;
        }
        let side = &b["attacker"];
        let reserve = side["reserve"].as_i64().unwrap_or(0);
        if reserve > worst.map_or(0, |w| w.2) {
            worst = Some((
                b["province"].as_i64().unwrap_or(0),
                side["engaged"].as_i64().unwrap_or(0),
                reserve,
            ));
        }
        let mut rows: Vec<(String, f64)> = list(side, "reserve_divisions")
            .iter()
            .filter_map(|d| {
                let id = d["id"]
                    .as_str()
                    .map(String::from)
                    .or_else(|| d["id"].as_i64().map(|n| n.to_string()))?;
                Some((id, d["org"].as_f64().unwrap_or(0.0)))
            })
            .collect();
        // the freshest stay to relieve the line
        rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
        for (id, _) in rows.into_iter().skip(keep.max(0) as usize) {
            if let Some(a) = army_of.get(&id).filter(|a| pick.contains(*a)) {
                per.entry(a.clone()).or_default().push(id);
            }
        }
    }
    let mut out: Vec<(String, Vec<String>)> = per.into_iter().collect();
    out.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    (out, worst)
}

/// How much each part of a step costs a formation on its axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Weights {
    /// per unit of the terrain's movement cost
    pub movement: f64,
    /// per unit of the step's assault penalty (terrain attack + river + forts)
    pub assault: f64,
    /// per unit of narrowness (1 - width / the widest terrain of the read)
    pub narrow: f64,
    /// per unit of missing infrastructure (1 - level / the best level of the read)
    pub infrastructure: f64,
    /// per hostile division our country sees in the step's state
    pub enemy: f64,
}

/// Armour and motorised formations: rough, fortified and narrow ground and poor roads cost most.
pub(super) const ARMOUR: Weights = Weights {
    movement: 1.0,
    assault: 6.0,
    narrow: 3.0,
    infrastructure: 1.5,
    enemy: 0.3,
};
/// Infantry: the same ground costs it less (it fixes and widens where the armour does not go).
pub(super) const INFANTRY: Weights = Weights {
    movement: 1.0,
    assault: 3.0,
    narrow: 1.0,
    infrastructure: 0.5,
    enemy: 0.3,
};

/// What the axis search knows besides the graph.
pub(super) struct Ground<'a> {
    pub g: &'a HashMap<i64, Prov>,
    /// the states the path may cross (own front states and the enemies' states)
    pub allowed: &'a dyn Fn(i64) -> bool,
    /// a state's infrastructure level when read
    pub infrastructure: &'a dyn Fn(i64) -> Option<f64>,
    /// the hostile divisions our country sees in a state (0 when unseen)
    pub enemy: &'a dyn Fn(i64) -> f64,
}

impl Ground<'_> {
    fn widest(&self) -> f64 {
        self.g
            .values()
            .map(|p| p.width)
            .fold(0.0, f64::max)
            .max(1.0)
    }
    fn best_infrastructure(&self) -> f64 {
        let mut states: Vec<i64> = self.g.values().map(|p| p.state).collect();
        states.sort_unstable();
        states.dedup();
        states
            .into_iter()
            .filter_map(|s| (self.infrastructure)(s))
            .fold(0.0, f64::max)
    }
    /// The cost of the step from `p` into `q` for weights `w` (pure).
    pub(super) fn step(&self, p: i64, q: i64, w: &Weights, widest: f64, infra: f64) -> f64 {
        let Some(to) = self.g.get(&q) else {
            return f64::INFINITY;
        };
        let river = self
            .g
            .get(&p)
            .and_then(|r| r.borders.iter().find(|(n, _)| *n == q))
            .map_or(0.0, |(_, r)| *r);
        let assault = -(to.attack + river + FORT_STEP * to.forts as f64).min(0.0);
        let narrow = if to.width > 0.0 {
            (1.0 - to.width / widest).max(0.0)
        } else {
            0.0
        };
        let roads = match (infra > 0.0, (self.infrastructure)(to.state)) {
            (true, Some(level)) => (1.0 - level / infra).max(0.0),
            _ => 0.0,
        };
        w.movement * to.movement
            + w.assault * assault
            + w.narrow * narrow
            + w.infrastructure * roads
            + w.enemy * (self.enemy)(to.state)
    }
}

#[derive(PartialEq)]
struct Node(f64, i64);
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .partial_cmp(&self.0)
            .unwrap_or(Ordering::Equal)
            .then(other.1.cmp(&self.1))
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The cheapest province path (Dijkstra) from any of `starts` to a province of a `goal` state,
/// through allowed states only, with its cost; None when no way exists. Pure.
pub(super) fn axis(
    ground: &Ground<'_>,
    starts: &[i64],
    goal: &HashSet<i64>,
    w: &Weights,
) -> Option<(Vec<i64>, f64)> {
    let widest = ground.widest();
    let infra = ground.best_infrastructure();
    let mut dist: HashMap<i64, f64> = HashMap::new();
    let mut prev: HashMap<i64, i64> = HashMap::new();
    let mut heap = BinaryHeap::new();
    let mut starts: Vec<i64> = starts
        .iter()
        .copied()
        .filter(|p| ground.g.contains_key(p))
        .collect();
    starts.sort_unstable();
    starts.dedup();
    for s in &starts {
        dist.insert(*s, 0.0);
        heap.push(Node(0.0, *s));
    }
    while let Some(Node(d, p)) = heap.pop() {
        if d > dist.get(&p).copied().unwrap_or(f64::INFINITY) + 1e-12 {
            continue;
        }
        if ground.g.get(&p).is_some_and(|r| goal.contains(&r.state)) {
            let mut path = vec![p];
            let mut at = p;
            while let Some(b) = prev.get(&at) {
                path.push(*b);
                at = *b;
            }
            path.reverse();
            return Some((path, d));
        }
        let mut next: Vec<i64> = ground.g[&p].borders.iter().map(|(q, _)| *q).collect();
        next.sort_unstable();
        for q in next {
            let Some(row) = ground.g.get(&q) else {
                continue;
            };
            if !(ground.allowed)(row.state) {
                continue;
            }
            let nd = d + ground.step(p, q, w, widest, infra);
            if nd + 1e-12 < dist.get(&q).copied().unwrap_or(f64::INFINITY) {
                dist.insert(q, nd);
                prev.insert(q, p);
                heap.push(Node(nd, q));
            }
        }
    }
    None
}

/// The distinct states of a province path in order, without `skip` (the own start states). Pure.
pub(super) fn path_states(g: &HashMap<i64, Prov>, path: &[i64], skip: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for p in path {
        if let Some(s) = g.get(p).map(|r| r.state) {
            if !skip.contains(&s) && !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// The provinces of the states `line` as one walk of neighbours in drawing order: it starts at
/// an end of the first state (the province with the fewest neighbours inside the line) and goes
/// on to the nearest province not yet taken (by steps inside the line's provinces), the states in
/// the order given. Pure.
pub(super) fn line_provinces(g: &HashMap<i64, Prov>, line: &[i64]) -> Vec<i64> {
    let inside: HashSet<i64> = g
        .iter()
        .filter(|(_, r)| line.contains(&r.state))
        .map(|(p, _)| *p)
        .collect();
    let rank = |p: i64| -> usize {
        g.get(&p)
            .and_then(|r| line.iter().position(|s| *s == r.state))
            .unwrap_or(usize::MAX)
    };
    let inner = |p: i64| -> usize {
        g.get(&p).map_or(0, |r| {
            r.borders.iter().filter(|(q, _)| inside.contains(q)).count()
        })
    };
    let mut left: Vec<i64> = inside.iter().copied().collect();
    left.sort_by_key(|p| (rank(*p), inner(*p), *p));
    let mut out: Vec<i64> = Vec::new();
    let mut taken: HashSet<i64> = HashSet::new();
    let Some(first) = left.first().copied() else {
        return out;
    };
    let mut at = first;
    loop {
        out.push(at);
        taken.insert(at);
        // the nearest province not taken: breadth first inside the line, the earliest state first
        let mut seen: HashSet<i64> = HashSet::from([at]);
        let mut ring = vec![at];
        let mut found: Option<i64> = None;
        while found.is_none() && !ring.is_empty() {
            let mut next = Vec::new();
            for p in &ring {
                for (q, _) in g.get(p).map(|r| r.borders.as_slice()).unwrap_or(&[]) {
                    if inside.contains(q) && seen.insert(*q) {
                        next.push(*q);
                    }
                }
            }
            let mut cands: Vec<i64> = next
                .iter()
                .copied()
                .filter(|q| !taken.contains(q))
                .collect();
            cands.sort_by_key(|q| (rank(*q), *q));
            found = cands.first().copied();
            ring = next;
        }
        // a line in pieces goes on at the next piece
        let found = found.or_else(|| left.iter().copied().find(|p| !taken.contains(p)));
        match found {
            Some(p) => at = p,
            None => break,
        }
    }
    out
}

/// The provinces of `g` that are the enemy's: in a state of `enemy_states` and not one of
/// `ours` (provinces where our divisions stand, or a state we hold). Pure helper for callers.
pub(super) fn enemy_provinces(
    g: &HashMap<i64, Prov>,
    enemy_states: &HashSet<i64>,
    our_provinces: &HashSet<i64>,
    our_states: &HashSet<i64>,
) -> HashSet<i64> {
    g.iter()
        .filter(|(p, r)| {
            enemy_states.contains(&r.state)
                && !our_states.contains(&r.state)
                && !our_provinces.contains(p)
        })
        .map(|(p, _)| *p)
        .collect()
}

/// The hostile divisions our country sees per state: the `hoi4_wars` border rows' hostile
/// divisions in each neighbour's border states, spread evenly over those states. Pure.
pub(super) fn hostile_by_state(wars: &Value, tags: &[String]) -> HashMap<i64, f64> {
    let mut out: HashMap<i64, f64> = HashMap::new();
    for b in list(wars, "borders") {
        if !tags.iter().any(|t| t == text(&b["neighbour_tag"])) {
            continue;
        }
        let states = ints(&b["neighbour_states"]);
        let n = b["hostile_divisions_in_border_states"]
            .as_f64()
            .or(b["neighbour_divisions_in_border_states"].as_f64())
            .unwrap_or(0.0);
        if states.is_empty() || n <= 0.0 {
            continue;
        }
        for s in &states {
            *out.entry(*s).or_default() += n / states.len() as f64;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prov(id: i64, state: i64, terrain: &str, borders: &[(i64, f64)]) -> (i64, Prov) {
        let (width, support, attack, movement) = match terrain {
            "forest" => (60.0, 30.0, -0.15, 1.5),
            "hills" => (70.0, 35.0, -0.25, 1.5),
            "urban" => (80.0, 40.0, -0.3, 1.2),
            "marsh" => (50.0, 25.0, -0.4, 2.0),
            _ => (70.0, 35.0, 0.0, 1.0),
        };
        (
            id,
            Prov {
                state,
                terrain: terrain.into(),
                width,
                support,
                attack,
                movement,
                forts: 0,
                borders: borders.to_vec(),
            },
        )
    }

    #[test]
    fn the_fit_takes_the_divisions_whose_line_is_strongest_under_the_width_penalties() {
        // 24 wide into a forest from one side: 3 (72/60, 20% over, 2.4 lines) beats 2
        assert_eq!(best_fit(60.0, 1, 24.0), 3);
        // from two sides (90): 4 (96/90) beats 5 (120/90: past the third over the width)
        assert_eq!(best_fit(90.0, 2, 24.0), 4);
        // plains: 3 at 72/70
        assert_eq!(best_fit(70.0, 1, 24.0), 3);
        // armour 38 wide on plains: 2 (76/70)
        assert_eq!(best_fit(70.0, 1, 38.0), 2);
        // 20 wide on plains from three sides (140): 7, under the stacking start of 11
        assert_eq!(best_fit(140.0, 3, 20.0), 7);
        // narrow divisions on a wide front stop at the stacking penalty's balance
        assert!(best_fit(400.0, 1, 2.0) < 200);
        assert_eq!(best_fit(0.0, 1, 24.0), 1);
    }

    #[test]
    fn caps_share_each_enemy_province_among_the_own_provinces_facing_it() {
        // own 1 and 2 both face the enemy forest 10; own 2 also faces the enemy plains 11
        let g: HashMap<i64, Prov> = [
            prov(1, 100, "plains", &[(10, 0.0), (2, 0.0)]),
            prov(2, 100, "plains", &[(10, 0.0), (11, 0.0), (1, 0.0)]),
            prov(10, 200, "forest", &[(1, 0.0), (2, 0.0), (11, 0.0)]),
            prov(11, 200, "plains", &[(2, 0.0), (10, 0.0)]),
        ]
        .into_iter()
        .collect();
        let front: HashSet<i64> = [1, 2].into();
        let caps = province_caps(&g, &front, &|p| p >= 10, &|_| 24.0, 0);
        // forest from two sides: 60 + 30 = 90 -> 4, two each; plains from one side -> 3
        assert_eq!(caps[&1].cap, 2);
        assert_eq!(caps[&2].cap, 5);
        let mut facing = caps[&2].facing.clone();
        facing.sort_unstable();
        assert_eq!(facing, vec![10, 11]);
        // the plan's kept reserve is added to each
        let caps = province_caps(&g, &front, &|p| p >= 10, &|_| 24.0, 1);
        assert_eq!(caps[&1].cap, 3);
    }

    #[test]
    fn the_battle_read_gives_the_reserve_beyond_what_is_kept_freshest_kept() {
        let read = json!({"list": [
            {"province": 3488, "our_side": "attacker",
             "attacker": {"engaged": 4, "reserve": 3, "reserve_divisions": [
                {"id": "a", "org": 50.0}, {"id": "b", "org": 60.0}, {"id": "c", "org": 40.0}]}},
            {"province": 9, "our_side": "defender",
             "attacker": {"engaged": 2, "reserve": 9, "reserve_divisions": [{"id": "z", "org": 1.0}]}}]});
        let army_of: HashMap<String, String> =
            [("a", "Pommern"), ("b", "Pommern"), ("c", "Pommern")]
                .into_iter()
                .map(|(d, a)| (d.to_string(), a.to_string()))
                .collect();
        let pick: HashSet<String> = ["Pommern".to_string()].into();
        let (out, worst) = battle_surplus(&read, &army_of, &pick, 1);
        assert_eq!(
            out,
            vec![(
                "Pommern".to_string(),
                vec!["a".to_string(), "c".to_string()]
            )]
        );
        assert_eq!(worst, Some((3488, 4, 3)));
        // an army not picked (the armour) keeps its reserve
        let none: HashSet<String> = HashSet::new();
        assert!(battle_surplus(&read, &army_of, &none, 0).0.is_empty());
    }

    /// own 1 -> two ways to the goal state 300: through the forest behind a river (11 -> 21) or
    /// round over `round` plains provinces (12 -> 22 -> ... -> 23).
    fn theatre(round: usize) -> HashMap<i64, Prov> {
        let mut g: HashMap<i64, Prov> = [
            prov(1, 100, "plains", &[(11, -0.6), (12, 0.0)]),
            prov(11, 200, "forest", &[(1, -0.6), (21, 0.0)]),
            prov(21, 300, "forest", &[(11, 0.0)]),
        ]
        .into_iter()
        .collect();
        // 12 (state 200), then 22, 24, 25 ... in state 201, then 23 in the goal state
        let mut chain: Vec<i64> = vec![12];
        chain.extend((0..round.saturating_sub(2)).map(|i| if i == 0 { 22 } else { 23 + i as i64 }));
        chain.push(23);
        for (i, p) in chain.iter().enumerate() {
            let mut b: Vec<(i64, f64)> = vec![(if i == 0 { 1 } else { chain[i - 1] }, 0.0)];
            if let Some(n) = chain.get(i + 1) {
                b.push((*n, 0.0));
            }
            let state = if i == 0 {
                200
            } else if *p == 23 {
                300
            } else {
                201
            };
            g.insert(*p, prov(*p, state, "plains", &b).1);
        }
        g
    }

    #[test]
    fn the_armour_axis_goes_round_the_forest_and_the_river_the_infantry_may_go_straight() {
        let g = theatre(3);
        let ground = Ground {
            g: &g,
            allowed: &|_| true,
            infrastructure: &|_| None,
            enemy: &|_| 0.0,
        };
        let goal: HashSet<i64> = [300].into();
        let (path, _) = axis(&ground, &[1], &goal, &ARMOUR).unwrap();
        assert_eq!(path, vec![1, 12, 22, 23]);
        assert_eq!(path_states(&g, &path, &[100]), vec![200, 201, 300]);
        // without the river the infantry takes the short way through the forest when the way
        // round is long (5 plains steps against two forest steps); the armour still goes round
        let mut g2 = theatre(5);
        g2.get_mut(&1).unwrap().borders = vec![(11, 0.0), (12, 0.0)];
        let ground = Ground {
            g: &g2,
            allowed: &|_| true,
            infrastructure: &|_| None,
            enemy: &|_| 0.0,
        };
        let (path, _) = axis(&ground, &[1], &goal, &INFANTRY).unwrap();
        assert_eq!(path, vec![1, 11, 21]);
        assert_eq!(axis(&ground, &[1], &goal, &ARMOUR).unwrap().0.len(), 6);
        // forts on the way round turn the armour back to the straight way
        let mut g3 = g2.clone();
        g3.get_mut(&22).unwrap().forts = 10;
        g3.get_mut(&24).unwrap().forts = 10;
        let ground = Ground {
            g: &g3,
            allowed: &|_| true,
            infrastructure: &|_| None,
            enemy: &|_| 0.0,
        };
        assert_eq!(
            axis(&ground, &[1], &goal, &ARMOUR).unwrap().0,
            vec![1, 11, 21]
        );
        // a state not allowed is never crossed
        let ground = Ground {
            g: &g,
            allowed: &|s| s != 201,
            infrastructure: &|_| None,
            enemy: &|_| 0.0,
        };
        assert_eq!(
            axis(&ground, &[1], &goal, &ARMOUR).unwrap().0,
            vec![1, 11, 21]
        );
    }

    #[test]
    fn seen_enemies_and_poor_roads_raise_a_way_s_cost() {
        let mut g = theatre(3);
        g.get_mut(&1).unwrap().borders = vec![(11, 0.0), (12, 0.0)];
        for p in [11, 21] {
            let r = g.get_mut(&p).unwrap();
            r.terrain = "plains".into();
            r.width = 70.0;
            r.attack = 0.0;
            r.movement = 1.0;
        }
        let goal: HashSet<i64> = [300].into();
        let plain = Ground {
            g: &g,
            allowed: &|_| true,
            infrastructure: &|_| None,
            enemy: &|_| 0.0,
        };
        assert_eq!(
            axis(&plain, &[1], &goal, &ARMOUR).unwrap().0,
            vec![1, 11, 21]
        );
        let crowded = Ground {
            g: &g,
            allowed: &|_| true,
            infrastructure: &|_| None,
            enemy: &|s| if s == 200 { 10.0 } else { 0.0 },
        };
        // both ways start in 200 (11 and 12): the enemy there costs both; the short way stays
        assert_eq!(
            axis(&crowded, &[1], &goal, &ARMOUR).unwrap().0,
            vec![1, 11, 21]
        );
        let roads = Ground {
            g: &g,
            allowed: &|_| true,
            infrastructure: &|s| Some(if s == 300 { 1.0 } else { 5.0 }),
            enemy: &|_| 0.0,
        };
        let (_, cost) = axis(&roads, &[1], &goal, &ARMOUR).unwrap();
        let (_, base) = axis(&plain, &[1], &goal, &ARMOUR).unwrap();
        assert!(cost > base);
    }

    #[test]
    fn line_provinces_walk_the_line_from_one_end() {
        // a chain 5-6-7 in state 1 and 8-9 in state 2 (8 touches 7)
        let g: HashMap<i64, Prov> = [
            prov(5, 1, "plains", &[(6, 0.0)]),
            prov(6, 1, "plains", &[(5, 0.0), (7, 0.0)]),
            prov(7, 1, "plains", &[(6, 0.0), (8, 0.0)]),
            prov(8, 2, "plains", &[(7, 0.0), (9, 0.0)]),
            prov(9, 2, "plains", &[(8, 0.0)]),
            prov(4, 3, "plains", &[(5, 0.0)]),
        ]
        .into_iter()
        .collect();
        assert_eq!(line_provinces(&g, &[1, 2]), vec![5, 6, 7, 8, 9]);
        assert_eq!(line_provinces(&g, &[2]), vec![8, 9]);
        assert!(line_provinces(&g, &[77]).is_empty());
    }

    #[test]
    fn the_graph_read_keeps_rivers_forts_and_drops_impassable_borders() {
        let read = json!({"provinces": [
            {"province": 1, "state_id": 7, "terrain": "forest", "combat_width": 60.0,
             "combat_support_width": 30.0, "attack_modifier": -0.15, "movement_cost": 1.5,
             "forts_in_history": [{"building": "bunker", "level": 3}, {"building": "coastal_bunker", "level": 9}],
             "borders": [{"province": 2, "river_share": 0.8, "river_attack_penalty": -0.6},
                         {"province": 3, "river_share": 0.1, "river_attack_penalty": -0.3},
                         {"province": 4, "special": [{"kind": "impassable"}]}]}]});
        let g = read_graph(&read);
        let p = &g[&1];
        assert_eq!((p.state, p.forts, p.width, p.support), (7, 3, 60.0, 30.0));
        assert_eq!(p.borders, vec![(2, -0.6), (3, 0.0)]);
        let wars = json!({"borders": [{"neighbour_tag": "BEL", "neighbour_states": [6, 34],
            "hostile_divisions_in_border_states": 10}]});
        let seen = hostile_by_state(&wars, &["BEL".to_string()]);
        assert_eq!(seen[&6], 5.0);
        let enemy = enemy_provinces(&g, &[7].into(), &HashSet::new(), &HashSet::new());
        assert!(enemy.contains(&1));
    }
}
