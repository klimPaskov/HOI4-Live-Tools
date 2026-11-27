//! Battle plans through the in-process module (engineer 73, family `orders`): the module's order-tree read in the
//! external read's model, the four order commands (delete one order, execute/stop, a new province path for an
//! existing order, a new front under a parent order), and the province list of a front stated as "this own border
//! against country X". The family is off under `HOI4_BACKEND=auto` until its live session (S4-ORDERS) passes.
use super::armies::orders::{List, OrderNode};
use super::inproc::Channel;
use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The module's order tree (`{"lists":[{"list":"top|fallback|area","orders":[node]}]}`) as the external read's
/// nodes, so `orders::order_json` renders the same rows whichever reader ran. A fallback or area-defence list maps
/// to `List::Defensive`, as the external reader does.
pub(super) fn tree_nodes(tree: &Value) -> Result<Vec<(List, OrderNode)>> {
    fn node(v: &Value) -> Result<OrderNode> {
        let ints = |k: &str| -> Vec<i32> {
            v[k].as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_i64())
                        .map(|x| x as i32)
                        .collect()
                })
                .unwrap_or_default()
        };
        let kids = |k: &str| -> Result<Vec<OrderNode>> {
            v[k].as_array()
                .map(|a| a.iter().map(node).collect())
                .unwrap_or(Ok(Vec::new()))
        };
        let kind = v["kind"]
            .as_i64()
            .ok_or_else(|| Error::new("inproc_channel", "order without a kind"))?;
        // the external reader keeps one child list (+0x1c8); the target-line copies (+0x2a0) are not in its model
        let children = kids("children")?;
        Ok(OrderNode {
            kind: kind as i32,
            id: v["id"].as_i64().unwrap_or(0) as i32,
            executing: v["executing"].as_bool() == Some(true),
            provinces: ints("provinces"),
            states: Vec::new(),
            divisions: v["divisions"].as_u64().unwrap_or(0) as usize,
            children,
        })
    }
    if tree["truncated"].as_bool() == Some(true) {
        return Err(Error::new(
            "inproc_channel",
            "the plan is larger than one module reply",
        ));
    }
    let mut out = Vec::new();
    for list in tree["lists"].as_array().into_iter().flatten() {
        let which = if list["list"] == "top" {
            List::Top
        } else {
            List::Defensive
        };
        for o in list["orders"].as_array().into_iter().flatten() {
            out.push((which, node(o)?));
        }
    }
    Ok(out)
}

/// The id of the order a new front hangs under when the caller names none: the group's one top-level order (its
/// plan's root line). None when the plan has no top order or several (the caller must name one).
pub(super) fn default_parent(nodes: &[(List, OrderNode)]) -> Option<i32> {
    let top: Vec<&OrderNode> = nodes
        .iter()
        .filter(|(l, _)| *l == List::Top)
        .map(|(_, n)| n)
        .collect();
    (top.len() == 1).then(|| top[0].id)
}

/// Every order id in the tree (for "replace, never add": delete exactly these, or check a new id appeared).
pub(super) fn all_ids(nodes: &[(List, OrderNode)]) -> BTreeSet<i32> {
    fn walk(n: &OrderNode, out: &mut BTreeSet<i32>) {
        out.insert(n.id);
        for c in &n.children {
            walk(c, out);
        }
    }
    let mut out = BTreeSet::new();
    for (_, n) in nodes {
        walk(n, &mut out);
    }
    out
}

/// The orders request: one group, the order id, the provinces in drawing order (set_path, new_front), the drawing
/// group (new_front, optional), the execute flag (execute only).
pub(super) fn request(
    op: &str,
    group: u64,
    order_id: i32,
    provinces: &[i32],
    drawer: Option<u64>,
    flag: Option<i64>,
) -> Result<Value> {
    let needs = matches!(op, "orders.set_path" | "orders.new_front");
    if !matches!(
        op,
        "orders.delete" | "orders.execute" | "orders.set_path" | "orders.new_front"
    ) {
        return Err(Error::new("invalid_arguments", "unknown orders op"));
    }
    if needs != !provinces.is_empty() || provinces.len() > 1024 {
        return Err(Error::new(
            "invalid_arguments",
            "provinces: 1..1024 for set_path and new_front, none otherwise",
        ));
    }
    if provinces.iter().any(|p| *p <= 0) {
        return Err(Error::new("invalid_arguments", "province ids are positive"));
    }
    let mut req =
        json!({"op":"post","cmd":op,"objs":format!("0x{group:x}"),"vals":order_id.to_string()});
    if needs {
        req["provs"] = json!(provinces
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(","));
    }
    if let Some(d) = drawer {
        req["obj2"] = json!(format!("0x{d:x}"));
    }
    if let Some(f) = flag {
        req["flag"] = json!(f);
    }
    Ok(req)
}

/// The orders2 request (engineer 74, the rest of the battle plan): one group, the op's values, the provinces (fallback,
/// spearhead), the division addresses (assign, unassign; optional for new_root and fallback) and, for new_root, the
/// CFront. The arity rules are the module's own (`be_post2`), checked here first so a bad call never reaches it.
/// vals: new_root [section, from, to, split]; assign [order]; unassign []; fallback []; edit_root [order, from, to];
/// invasion_target / paradrop_target [order, province]; spearhead [parent order].
pub(super) fn request2(
    op: &str,
    group: u64,
    vals: &[i64],
    provinces: &[i32],
    divisions: &[u64],
    front: Option<u64>,
) -> Result<Value> {
    let bad = |m: &str| Err(Error::new("invalid_arguments", m.to_string()));
    let (arity, provs, divs_need, divs_may) = match op {
        "orders2.new_root" => (4, false, false, true),
        "orders2.assign" => (1, false, true, true),
        "orders2.unassign" => (0, false, true, true),
        "orders2.fallback" => (0, true, false, true),
        "orders2.edit_root" => (3, false, false, false),
        "orders2.invasion_target" | "orders2.paradrop_target" => (2, false, false, false),
        "orders2.spearhead" => (1, true, false, false),
        _ => return bad("unknown orders2 op"),
    };
    if vals.len() != arity {
        return bad("the wrong number of values for this op");
    }
    if provs != !provinces.is_empty() || provinces.len() > 1024 || provinces.iter().any(|p| *p <= 0)
    {
        return bad("provinces: 1..1024 positive ids for fallback and spearhead, none otherwise");
    }
    let mut seen = BTreeSet::new();
    if !provinces.iter().all(|p| seen.insert(*p)) {
        return bad("a province is listed twice");
    }
    if (divs_need && divisions.is_empty())
        || (!divs_may && !divisions.is_empty())
        || divisions.len() > 512
    {
        return bad(
            "divisions: needed for assign/unassign, allowed for new_root/fallback, at most 512",
        );
    }
    let range = |a: i64, b: i64| (0..=100_000).contains(&a) && (0..=100_000).contains(&b) && a < b;
    match op {
        "orders2.new_root"
            if !(vals[0] >= 0 && range(vals[1], vals[2]) && (vals[3] == 0 || vals[3] == 1)) =>
        {
            return bad("new_root: section, from < to in 0..100000, split 0/1")
        }
        "orders2.edit_root" if !range(vals[1], vals[2]) => {
            return bad("edit_root: from < to in 0..100000")
        }
        "orders2.invasion_target" | "orders2.paradrop_target" if vals[1] <= 0 => {
            return bad("a province id")
        }
        _ => {}
    }
    if op != "orders2.new_root" && arity >= 1 && vals[0] <= 0 {
        return bad("vals[0]: an order id");
    }
    if (op == "orders2.new_root") != front.is_some() {
        return bad("new_root needs the CFront, no other op takes one");
    }
    let join = |v: Vec<String>| v.join(",");
    let mut req = json!({"op":"post","cmd":op,"objs":format!("0x{group:x}")});
    if !vals.is_empty() {
        req["vals"] = json!(join(vals.iter().map(i64::to_string).collect()));
    }
    if !provinces.is_empty() {
        req["provs"] = json!(join(provinces.iter().map(i32::to_string).collect()));
    }
    if !divisions.is_empty() {
        req["divs"] = json!(join(divisions.iter().map(|d| format!("0x{d:x}")).collect()));
    }
    if let Some(f) = front {
        req["obj2"] = json!(format!("0x{f:x}"));
    }
    Ok(req)
}

/// The front section a new plan root binds to: of the group's theatre fronts (the module's `fronts` read), the section
/// holding the most of `provinces` (ties: the lower section id). None when no section shares a province: the game
/// binds a root to a front the map already has, it does not make one.
pub(super) fn root_section(fronts: &Value, provinces: &[i32]) -> Option<(u64, i32)> {
    let want: BTreeSet<i32> = provinces.iter().copied().collect();
    let mut best: Option<(usize, i32, u64)> = None;
    for f in fronts["fronts"].as_array().into_iter().flatten() {
        let addr = f["front"]
            .as_str()
            .and_then(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16).ok())?;
        for s in f["sections"].as_array().into_iter().flatten() {
            let id = s["id"].as_i64()? as i32;
            let hits = s["provinces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .filter(|p| want.contains(&(*p as i32)))
                .count();
            let better = match best {
                None => hits > 0,
                Some((h, i, _)) => hits > h || (hits == h && hits > 0 && id < i),
            };
            if better {
                best = Some((hits, id, addr));
            }
        }
    }
    best.map(|(_, id, addr)| (addr, id))
}

impl Channel {
    /// The fronts of an own group's theatre (engineer 74): `{"theatre","fronts":[{"front","sections":[{"id","provinces"}]}]}`.
    pub(super) fn fronts(&mut self, group: u64) -> Result<Value> {
        let reply = self.call_raw(json!({"op":"fronts","group":format!("0x{group:x}")}))?;
        if reply["ok"].as_bool() != Some(true) || reply["truncated"].as_bool() == Some(true) {
            return Err(Error::new(
                "inproc_refused",
                format!(
                    "fronts read refused: {}",
                    reply["reason"]
                        .as_str()
                        .or(reply["error"].as_str())
                        .unwrap_or("truncated")
                ),
            ));
        }
        Ok(reply)
    }

    /// The order tree of one own army or army group, read by the module between two frames.
    pub(super) fn orders_tree(&mut self, group: u64) -> Result<Value> {
        let reply = self.call_raw(json!({"op":"orders","group":format!("0x{group:x}")}))?;
        if reply["ok"].as_bool() != Some(true) {
            return Err(Error::new(
                "inproc_refused",
                format!(
                    "order read refused: {}",
                    reply["reason"]
                        .as_str()
                        .or(reply["error"].as_str())
                        .unwrap_or("?")
                ),
            ));
        }
        Ok(reply)
    }

    /// One orders command (see `request`); the reply carries the module's own read-back.
    pub(super) fn orders_post(&mut self, req: Value) -> Result<Value> {
        self.post_raw(req)
    }
}

/// The front line "along this own border against country X", in drawing order: the own provinces (controlled by
/// the player) that touch a province controlled by `enemy`, chained so that each province is adjacent to the next.
/// The game keeps runs of consecutive valid ids and does not check adjacency between them, so a border that falls
/// apart (a sea inlet, a third country) becomes several chains: one new front each. `own_states` (optional) keeps
/// only the provinces of those states: a field marshal's own piece of a border two groups share.
pub(super) fn border_chains(
    own: &BTreeSet<i32>,
    enemy: &BTreeSet<i32>,
    adjacent: &dyn Fn(i32) -> Vec<i32>,
    keep: &dyn Fn(i32) -> bool,
) -> Vec<Vec<i32>> {
    let border: BTreeSet<i32> = own
        .iter()
        .copied()
        .filter(|p| keep(*p) && adjacent(*p).iter().any(|q| enemy.contains(q)))
        .collect();
    let nb: BTreeMap<i32, Vec<i32>> = border
        .iter()
        .map(|p| {
            let mut v: Vec<i32> = adjacent(*p)
                .into_iter()
                .filter(|q| border.contains(q) && q != p)
                .collect();
            v.sort_unstable();
            v.dedup();
            (*p, v)
        })
        .collect();
    let mut left: BTreeSet<i32> = border.clone();
    let mut chains = Vec::new();
    while let Some(&first) = left.iter().next() {
        // the connected piece holding `first`, then start the walk at one of its ends (fewest border neighbours)
        let mut piece = BTreeSet::new();
        let mut stack = vec![first];
        while let Some(p) = stack.pop() {
            if piece.insert(p) {
                stack.extend(nb[&p].iter().copied().filter(|q| left.contains(q)));
            }
        }
        let start = *piece
            .iter()
            .min_by_key(|p| (nb[p].iter().filter(|q| piece.contains(q)).count(), **p))
            .unwrap();
        let mut chain = vec![start];
        let mut seen: BTreeSet<i32> = BTreeSet::from([start]);
        let mut at = start;
        loop {
            // the next unvisited neighbour, preferring the one with fewest unvisited neighbours (stay on the edge)
            let next = nb[&at]
                .iter()
                .copied()
                .filter(|q| piece.contains(q) && !seen.contains(q))
                .min_by_key(|q| {
                    (
                        nb[q]
                            .iter()
                            .filter(|r| piece.contains(r) && !seen.contains(r))
                            .count(),
                        *q,
                    )
                });
            match next {
                Some(q) => {
                    chain.push(q);
                    seen.insert(q);
                    at = q;
                }
                None => break,
            }
        }
        for p in &chain {
            left.remove(p);
        }
        // a branch the walk could not reach in one line stays in `left`: it forms chains of its own next rounds
        chains.push(chain);
    }
    chains.sort_by_key(|c| std::cmp::Reverse(c.len()));
    chains
}

/// A plan root of an orders group as the module reads it (engineer 95): a front line in the group's top list, its
/// provinces, the divisions it holds and its offensive arrows (kind 1) with theirs.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RootRow {
    pub id: i32,
    pub provinces: Vec<i32>,
    pub divisions: Vec<u64>,
    pub arrows: Vec<(i32, Vec<u64>)>,
}

impl RootRow {
    /// Every division the root carries: its own and its arrows'.
    pub(super) fn all_divisions(&self) -> BTreeSet<u64> {
        self.divisions
            .iter()
            .chain(self.arrows.iter().flat_map(|(_, d)| d.iter()))
            .copied()
            .collect()
    }
    /// The order an assignment to this front goes to: its one offensive arrow when it has exactly one (the attack
    /// carries the division), else the root itself.
    pub(super) fn assign_order(&self) -> i32 {
        match self.arrows.as_slice() {
            [(id, _)] => *id,
            _ => self.id,
        }
    }
}

fn hex_list(v: &Value) -> Vec<u64> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str())
        .filter_map(|x| u64::from_str_radix(x.trim_start_matches("0x"), 16).ok())
        .collect()
}

/// The group's plan roots from the module's order tree (top list, kind 2), in the tree's order.
pub(super) fn root_rows(tree: &Value) -> Vec<RootRow> {
    let mut out = Vec::new();
    for list in tree["lists"].as_array().into_iter().flatten() {
        if list["list"] != "top" {
            continue;
        }
        for o in list["orders"].as_array().into_iter().flatten() {
            if o["kind"].as_i64() != Some(2) {
                continue;
            }
            let arrows = o["children"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|c| c["kind"].as_i64() == Some(1))
                .map(|c| {
                    (
                        c["id"].as_i64().unwrap_or(0) as i32,
                        hex_list(&c["division_ptrs"]),
                    )
                })
                .collect();
            out.push(RootRow {
                id: o["id"].as_i64().unwrap_or(0) as i32,
                provinces: o["provinces"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_i64)
                    .map(|p| p as i32)
                    .collect(),
                divisions: hex_list(&o["division_ptrs"]),
                arrows,
            });
        }
    }
    out
}

/// One order of the module's tree by id (any depth): its provinces and the divisions it holds.
pub(super) fn order_row(tree: &Value, id: i32) -> Option<(Vec<i32>, Vec<u64>)> {
    fn walk(o: &Value, id: i32) -> Option<(Vec<i32>, Vec<u64>)> {
        if o["id"].as_i64() == Some(i64::from(id)) {
            let provinces = o["provinces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .map(|p| p as i32)
                .collect();
            return Some((provinces, hex_list(&o["division_ptrs"])));
        }
        o["children"]
            .as_array()
            .into_iter()
            .flatten()
            .find_map(|c| walk(c, id))
    }
    tree["lists"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|l| l["orders"].as_array().cloned().unwrap_or_default())
        .find_map(|o| walk(&o, id))
}

/// The order a new path goes to (engineer 95, 28 Nov, from 1.20 0x100936950): SetPath writes the order's provinces
/// (0x1008f91f0, no filter) and then re-derives every offensive arrow touched (0x1008f9290: an arrow, kind 1, gets
/// its path computed again from its source front to its target line, 0x100b0f660 -> 0x1008ff270), so a path given to
/// an ARROW is overwritten by the game's own and never reads back (w92c: set_path posted 9 times, executed 0). A new
/// target line for an offensive goes to the arrow's target line (its kind-2 child); the arrow follows it. Any other
/// order (a front, a fallback line) takes the path itself.
pub(super) fn path_target(nodes: &[(List, OrderNode)], id: i32) -> i32 {
    fn find(n: &OrderNode, id: i32) -> Option<&OrderNode> {
        if n.id == id {
            return Some(n);
        }
        n.children.iter().find_map(|c| find(c, id))
    }
    nodes
        .iter()
        .find_map(|(_, n)| find(n, id))
        .filter(|n| n.kind == 1)
        .and_then(|n| n.children.iter().find(|c| c.kind == 2))
        .map_or(id, |line| line.id)
}

/// Shares of `n` divisions by `weights` (largest remainder; ties to the earlier target), every target at least one
/// while there are as many divisions as targets: no front that faces an enemy is left with none.
pub(super) fn quotas(n: usize, weights: &[f64]) -> Vec<usize> {
    let k = weights.len();
    if k == 0 {
        return Vec::new();
    }
    let w: Vec<f64> = weights.iter().map(|x| x.max(0.0)).collect();
    let total: f64 = w.iter().sum();
    let floor = usize::from(n >= k);
    let rest = n - floor * k;
    let raw: Vec<f64> = if total > 0.0 {
        w.iter().map(|x| x / total * rest as f64).collect()
    } else {
        vec![rest as f64 / k as f64; k]
    };
    let mut q: Vec<usize> = raw.iter().map(|x| x.floor() as usize).collect();
    let mut left = rest - q.iter().sum::<usize>();
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|a, b| {
        let fa = raw[*a] - raw[*a].floor();
        let fb = raw[*b] - raw[*b].floor();
        fb.partial_cmp(&fa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });
    for i in order {
        if left == 0 {
            break;
        }
        q[i] += 1;
        left -= 1;
    }
    q.iter().map(|x| x + floor).collect()
}

/// The divisions split over the targets by place (engineer 95): each target keeps the divisions it holds already that
/// stand nearer it than any other target, the nearest first, up to its quota; the rest go to the nearest target with room; a division with no
/// place read fills what room is left. `divs` = (division, its map position), `current` = the target holding it (if
/// any), `targets` = each target's line as map positions. The fewest divisions move: each move costs its planning.
pub(super) fn split_by_place(
    divs: &[(u64, Option<(f64, f64)>)],
    current: &[Option<usize>],
    targets: &[Vec<(f64, f64)>],
    quota: &[usize],
) -> Vec<Vec<u64>> {
    let dist = |d: usize, t: usize| -> f64 {
        match divs[d].1 {
            Some((x, z)) => targets[t]
                .iter()
                .map(|(a, b)| (a - x).powi(2) + (b - z).powi(2))
                .fold(f64::INFINITY, f64::min),
            None => f64::INFINITY,
        }
    };
    let mut out: Vec<Vec<u64>> = vec![Vec::new(); targets.len()];
    let mut taken = vec![false; divs.len()];
    // the target nearest a division (None without a place read)
    let nearest = |d: usize| -> Option<usize> {
        divs[d].1?;
        (0..targets.len()).min_by(|a, b| {
            dist(d, *a)
                .partial_cmp(&dist(d, *b))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        })
    };
    // 1. each target keeps its own that stand nearer it than any other target (or have no place read), nearest first
    for t in 0..targets.len() {
        let mut mine: Vec<usize> = (0..divs.len())
            .filter(|d| current.get(*d).copied().flatten() == Some(t))
            .filter(|d| nearest(*d).is_none_or(|n| n == t))
            .collect();
        mine.sort_by(|a, b| {
            dist(*a, t)
                .partial_cmp(&dist(*b, t))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        });
        for d in mine.into_iter().take(quota.get(t).copied().unwrap_or(0)) {
            out[t].push(divs[d].0);
            taken[d] = true;
        }
    }
    // 2. the rest to the nearest target with room
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for d in 0..divs.len() {
        if taken[d] || divs[d].1.is_none() {
            continue;
        }
        for t in 0..targets.len() {
            pairs.push((dist(d, t), d, t));
        }
    }
    pairs.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
            .then(a.2.cmp(&b.2))
    });
    for (_, d, t) in pairs {
        if !taken[d] && out[t].len() < quota.get(t).copied().unwrap_or(0) {
            out[t].push(divs[d].0);
            taken[d] = true;
        }
    }
    // 3. what is left fills the room left, in target order
    for d in 0..divs.len() {
        if taken[d] {
            continue;
        }
        if let Some(t) =
            (0..targets.len()).find(|t| out[*t].len() < quota.get(*t).copied().unwrap_or(0))
        {
            out[t].push(divs[d].0);
            taken[d] = true;
        }
    }
    out
}

/// The pieces of `line` by the theatre front sections that hold them (engineer 95): a plan root binds to ONE section,
/// and a section is the border against one enemy (ctl92a, 28 Nov 2026: 12 own provinces over the Dutch, Belgian and
/// Luxembourg borders read back as the 8 on the Dutch one), so a line over several borders needs one root per
/// section. Returns (front, section, the line's provinces in it) in the line's order of first appearance, and the
/// provinces no section holds. A province two sections hold goes to the lower section id.
pub(super) fn section_pieces(
    fronts: &Value,
    line: &[i32],
) -> (Vec<(u64, i32, Vec<i32>)>, Vec<i32>) {
    let mut holder: BTreeMap<i32, (i32, u64)> = BTreeMap::new();
    for f in fronts["fronts"].as_array().into_iter().flatten() {
        let Some(addr) = f["front"]
            .as_str()
            .and_then(|a| u64::from_str_radix(a.trim_start_matches("0x"), 16).ok())
        else {
            continue;
        };
        for s in f["sections"].as_array().into_iter().flatten() {
            let Some(id) = s["id"].as_i64() else { continue };
            for p in s["provinces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
            {
                let e = holder.entry(p as i32).or_insert((id as i32, addr));
                if (id as i32) < e.0 {
                    *e = (id as i32, addr);
                }
            }
        }
    }
    let mut pieces: Vec<(u64, i32, Vec<i32>)> = Vec::new();
    let mut uncovered = Vec::new();
    for p in line {
        match holder.get(p) {
            Some((sec, addr)) => match pieces.iter_mut().find(|(a, s, _)| a == addr && s == sec) {
                Some(piece) => piece.2.push(*p),
                None => pieces.push((*addr, *sec, vec![*p])),
            },
            None => uncovered.push(*p),
        }
    }
    (pieces, uncovered)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders2_requests_follow_the_module_arity() {
        let r = request2(
            "orders2.new_root",
            0x10,
            &[7, 0, 100_000, 0],
            &[],
            &[0xa, 0xb],
            Some(0x99),
        )
        .unwrap();
        assert_eq!(r["cmd"], "orders2.new_root");
        assert_eq!(r["vals"], "7,0,100000,0");
        assert_eq!(r["divs"], "0xa,0xb");
        assert_eq!(r["obj2"], "0x99");
        let r = request2("orders2.fallback", 0x10, &[], &[3, 4, 5], &[], None).unwrap();
        assert_eq!(r["provs"], "3,4,5");
        assert!(r.get("vals").is_none());
        let r = request2("orders2.spearhead", 0x10, &[190], &[851, 861], &[], None).unwrap();
        assert_eq!(
            (r["vals"].as_str(), r["provs"].as_str()),
            (Some("190"), Some("851,861"))
        );
        assert!(
            request2("orders2.assign", 0x10, &[5], &[], &[], None).is_err(),
            "assign needs divisions"
        );
        assert!(request2("orders2.unassign", 0x10, &[], &[], &[0x1], None).is_ok());
        assert!(
            request2(
                "orders2.edit_root",
                0x10,
                &[5, 60_000, 50_000],
                &[],
                &[],
                None
            )
            .is_err(),
            "from < to"
        );
        assert!(
            request2(
                "orders2.new_root",
                0x10,
                &[7, 0, 100_000, 2],
                &[],
                &[],
                Some(1)
            )
            .is_err(),
            "split 0/1"
        );
        assert!(
            request2(
                "orders2.new_root",
                0x10,
                &[7, 0, 100_000, 1],
                &[],
                &[],
                None
            )
            .is_err(),
            "front needed"
        );
        assert!(
            request2("orders2.fallback", 0x10, &[], &[3, 3], &[], None).is_err(),
            "duplicate province"
        );
        assert!(request2("orders2.invasion_target", 0x10, &[5, 0], &[], &[], None).is_err());
        assert!(request2("orders2.invasion_target", 0x10, &[5, 9001], &[], &[], None).is_ok());
        assert!(
            request2("orders2.edit_root", 0x10, &[5, 0, 100], &[], &[0x1], None).is_err(),
            "no divisions"
        );
        assert!(request2("orders2.paint", 0x10, &[], &[], &[], None).is_err());
        // every request fits the module parser: flat members, short strings
        let big: Vec<i32> = (1..=1024).collect();
        let r = request2(
            "orders2.fallback",
            0x10,
            &[],
            &big,
            &[0x7fff_0000_1234; 512],
            None,
        )
        .unwrap();
        assert!(r.to_string().len() < 20_000);
    }

    #[test]
    fn a_root_binds_to_the_section_holding_most_of_the_line() {
        let fronts = json!({"theatre":"0x9","fronts":[
            {"front":"0x100","sections":[{"id":4,"provinces":[1,2,3]},{"id":7,"provinces":[4,5]}]},
            {"front":"0x200","sections":[{"id":2,"provinces":[5,6,7,8]}]}]});
        assert_eq!(root_section(&fronts, &[6, 7, 5]), Some((0x200, 2)));
        assert_eq!(root_section(&fronts, &[2, 3]), Some((0x100, 4)));
        assert_eq!(
            root_section(&fronts, &[4, 1]),
            Some((0x100, 4)),
            "a tie takes the lower section id"
        );
        assert_eq!(root_section(&fronts, &[99]), None);
    }

    fn line_adj(p: i32) -> Vec<i32> {
        // own provinces 1..=6 in a row, enemy 11..=16 opposite; 4 and 5 are not adjacent (a river mouth)
        let mut v = Vec::new();
        if (1..=6).contains(&p) {
            if p > 1 && p != 5 {
                v.push(p - 1);
            }
            if p < 6 && p != 4 {
                v.push(p + 1);
            }
            v.push(p + 10);
        }
        v
    }

    #[test]
    fn a_border_splits_into_adjacent_chains_in_drawing_order() {
        let own: BTreeSet<i32> = (1..=6).collect();
        let enemy: BTreeSet<i32> = (11..=16).collect();
        let chains = border_chains(&own, &enemy, &line_adj, &|_| true);
        assert_eq!(chains, vec![vec![1, 2, 3, 4], vec![5, 6]]);
        for c in &chains {
            for w in c.windows(2) {
                assert!(line_adj(w[0]).contains(&w[1]));
            }
        }
    }

    #[test]
    fn own_states_keep_only_their_piece() {
        let own: BTreeSet<i32> = (1..=6).collect();
        let enemy: BTreeSet<i32> = BTreeSet::from([12, 13]);
        let chains = border_chains(&own, &enemy, &line_adj, &|p| p >= 3);
        assert_eq!(chains, vec![vec![3]]);
    }

    #[test]
    fn the_module_tree_reads_as_the_external_model() {
        let tree = json!({"lists":[{"list":"top","orders":[{"order":"0x1","kind":2,"id":190,"executing":true,
            "provinces":[631,632],"divisions":4,"division_ptrs":[],"children":[{"order":"0x2","kind":1,"id":193,
            "executing":true,"provinces":[632,861],"divisions":2,"children":[{"order":"0x3","kind":2,"id":192,
            "executing":false,"provinces":[851,861],"divisions":2,"children":[],"target_copies":[]}],
            "target_copies":[]}],"target_copies":[]}]},{"list":"fallback","orders":[{"order":"0x4","kind":2,"id":200,
            "executing":false,"provinces":[101],"divisions":0,"children":[],"target_copies":[]}]},
            {"list":"area","orders":[]}],"truncated":false});
        let nodes = tree_nodes(&tree).unwrap();
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].0, List::Top);
        assert_eq!(nodes[1].0, List::Defensive);
        assert_eq!(nodes[0].1.children[0].children[0].id, 192);
        assert_eq!(default_parent(&nodes), Some(190));
        assert_eq!(all_ids(&nodes), BTreeSet::from([190, 192, 193, 200]));
        let row = super::super::armies::orders::order_json(
            &nodes[0].1,
            List::Top,
            &|p| Some(p / 10),
            &BTreeMap::new(),
        );
        assert_eq!(row["kind"], "front");
        assert_eq!(row["offensives"][0]["target"]["provinces"], 2);
        assert!(tree_nodes(&json!({"lists":[],"truncated":true})).is_err());
    }

    #[test]
    fn requests_name_one_group_one_order_and_the_provinces() {
        let r = request("orders.new_front", 0xabc, 190, &[1, 2], Some(0xdef), None).unwrap();
        assert_eq!(r["objs"], "0xabc");
        assert_eq!(r["vals"], "190");
        assert_eq!(r["provs"], "1,2");
        assert_eq!(r["obj2"], "0xdef");
        assert!(request("orders.delete", 0xabc, 190, &[1], None, None).is_err());
        assert!(request("orders.set_path", 0xabc, 190, &[], None, None).is_err());
        let e = request("orders.execute", 0xabc, 0, &[], None, Some(1)).unwrap();
        assert_eq!(e["flag"], 1);
        assert!(request("orders.move", 0xabc, 1, &[], None, None).is_err());
    }

    #[test]
    fn a_new_path_for_an_offensive_goes_to_its_target_line() {
        let line = OrderNode {
            kind: 2,
            id: 26,
            provinces: vec![9503, 1346],
            ..Default::default()
        };
        let arrow = OrderNode {
            kind: 1,
            id: 27,
            children: vec![line],
            ..Default::default()
        };
        let root = OrderNode {
            kind: 2,
            id: 19,
            children: vec![arrow],
            ..Default::default()
        };
        let nodes = vec![(List::Top, root)];
        assert_eq!(
            path_target(&nodes, 27),
            26,
            "the arrow's path is the game's own"
        );
        assert_eq!(path_target(&nodes, 26), 26);
        assert_eq!(path_target(&nodes, 19), 19, "a front takes its own path");
        assert_eq!(path_target(&nodes, 99), 99);
    }

    #[test]
    fn shares_follow_the_weights_and_leave_no_front_empty() {
        assert_eq!(quotas(68, &[8.0, 4.0]), vec![45, 23]);
        // a short front still gets one
        assert_eq!(quotas(10, &[30.0, 0.5]), vec![9, 1]);
        assert_eq!(quotas(1, &[1.0, 1.0]), vec![1, 0]);
        assert_eq!(quotas(7, &[0.0, 0.0, 0.0]).iter().sum::<usize>(), 7);
        assert!(quotas(5, &[]).is_empty());
    }

    #[test]
    fn the_split_keeps_divisions_where_they_stand_and_moves_the_nearest() {
        // two fronts: Holland at x 0, Belgium at x 100; six divisions all on the Belgian front (ctl92a), three of
        // them standing by Holland
        let divs = vec![
            (1, Some((1.0, 0.0))),
            (2, Some((2.0, 0.0))),
            (3, Some((3.0, 0.0))),
            (4, Some((99.0, 0.0))),
            (5, Some((98.0, 0.0))),
            (6, None),
        ];
        let current = vec![Some(1); 6];
        let targets = vec![vec![(0.0, 0.0)], vec![(100.0, 0.0)]];
        let split = split_by_place(&divs, &current, &targets, &[3, 3]);
        // Belgium keeps those standing by it (4, 5) and the one with no place read (6); the three by Holland go there
        assert_eq!(split[1], vec![4, 5, 6]);
        assert_eq!(split[0], vec![1, 2, 3]);
        // nothing moves when every front holds its share where it stands
        let current = vec![Some(0), Some(0), Some(0), Some(1), Some(1), Some(1)];
        let again = split_by_place(&divs, &current, &targets, &[3, 3]);
        assert_eq!(again, vec![vec![1, 2, 3], vec![4, 5, 6]]);
    }

    #[test]
    fn a_line_over_three_borders_is_one_piece_per_section() {
        let fronts = json!({"theatre":"0x9","fronts":[
            {"front":"0x100","sections":[{"id":4,"provinces":[1,2,3]},{"id":7,"provinces":[4,5]}]},
            {"front":"0x200","sections":[{"id":2,"provinces":[6,7]}]}]});
        let (pieces, uncovered) = section_pieces(&fronts, &[1, 2, 3, 4, 5, 6, 9]);
        assert_eq!(
            pieces,
            vec![
                (0x100, 4, vec![1, 2, 3]),
                (0x100, 7, vec![4, 5]),
                (0x200, 2, vec![6])
            ]
        );
        assert_eq!(uncovered, vec![9]);
    }

    #[test]
    fn roots_read_with_their_divisions_and_arrows() {
        let tree = json!({"lists":[{"list":"top","orders":[
            {"kind":2,"id":109,"provinces":[1,2],"division_ptrs":[],"children":[]},
            {"kind":2,"id":110,"provinces":[3],"division_ptrs":["0xa","0xb"],"children":[
                {"kind":1,"id":111,"division_ptrs":["0xa"],"children":[]}]}]},
            {"list":"fallback","orders":[{"kind":2,"id":5,"provinces":[9],"division_ptrs":[]}]}]});
        let roots = root_rows(&tree);
        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0].id, 109);
        assert!(roots[0].divisions.is_empty());
        assert_eq!(roots[1].all_divisions(), BTreeSet::from([0xa, 0xb]));
        assert_eq!(roots[1].assign_order(), 111);
        assert_eq!(roots[0].assign_order(), 109);
        assert_eq!(order_row(&tree, 111), Some((vec![], vec![0xa])));
        assert_eq!(order_row(&tree, 110).map(|r| r.0), Some(vec![3]));
        assert_eq!(order_row(&tree, 5).map(|r| r.0), Some(vec![9]));
        assert!(order_row(&tree, 77).is_none());
    }
}
