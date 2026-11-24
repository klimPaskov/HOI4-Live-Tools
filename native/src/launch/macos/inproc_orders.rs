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

impl Channel {
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
