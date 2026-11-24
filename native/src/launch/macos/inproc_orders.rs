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
}
