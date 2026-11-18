//! Order read-back: every battle-plan order of an own army or army group, read from the same
//! structures the battle-plan bar and the map arrows draw from (COrderInstance).
//!
//! Mapped live on 18 November 2026 (1.19.4.1, the AI's own 1 September 1939 save loaded as
//! Germany, then our own pre-war save): an orders group keeps its top orders at +0x98 and its
//! fallback and area-defence orders in two lists of their own. A COrderInstance holds its kind
//! (+0x30: 2 a line, 1 an offensive arrow, 5 an area defence), the owning group (+0x38), the
//! provinces of a line or arrow (+0x70, i32 ids in drawing order), the states of an area defence
//! (+0xe0), the divisions assigned (+0x1e0, CArmy pointers), whether it executes (+0x218), its id
//! (+0x214) and two child lists: +0x1c8 (a front's offensive arrows; an arrow's target line, the
//! line drawn in enemy land) and +0x2a0 (a copy of the target line that carries no divisions).
//!
//! The read is the player's own: own orders only, as the battle-plan bar shows them.
use super::{group_named, ui};
use crate::{
    error::{Error, Result},
    memory::Reader,
    xlayout::{self, fld},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

const ORDER: xlayout::Class = xlayout::Class::new("COrderInstance", 0x3512bf0);
const KIND: u64 = 0x30;
const PROVINCES: u64 = 0x70;
const STATES: u64 = 0xe0;
const DIVISIONS: u64 = 0x1e0;
const ID: u64 = 0x214;
const EXECUTING: u64 = 0x218;
const CHILDREN: u64 = 0x1c8;

/// One order as read: its kind code, id, executing flag, provinces (lines and arrows), states
/// (area defence), divisions assigned and child orders (a front's arrows, an arrow's target).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct OrderNode {
    pub kind: i32,
    pub id: i32,
    pub executing: bool,
    pub provinces: Vec<i32>,
    pub states: Vec<i32>,
    pub divisions: usize,
    pub children: Vec<OrderNode>,
}

/// The list an order sits in: the group's top orders or one of its two defensive lists.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum List {
    Top,
    Defensive,
}

/// The kind name of an order: a line in the top list is a front, in a defensive list a fallback
/// line; an arrow is an offensive; 5 is an area defence.
pub(crate) fn kind_name(kind: i32, list: List) -> String {
    match (kind, list) {
        (2, List::Top) => "front".into(),
        (2, List::Defensive) => "fallback".into(),
        (1, _) => "offensive".into(),
        (5, _) => "area_defense".into(),
        (k, _) => format!("kind_{k}"),
    }
}

/// Distinct states of `provinces` in drawing order (`state_of` maps a province to its state).
fn states_along(provinces: &[i32], state_of: &dyn Fn(i32) -> Option<i32>) -> Vec<i32> {
    let mut out: Vec<i32> = Vec::new();
    for p in provinces {
        if let Some(s) = state_of(*p) {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// The JSON row of one top order: kind, id, executing, the provinces of the line (count, the two
/// ends, the states along it), divisions assigned, and for a front its offensives with the arrow's
/// states and the target line (count, ends, states). `standing` counts the owner's divisions per
/// province (for divisions on the line and the most in one province).
pub(crate) fn order_json(
    node: &OrderNode,
    list: List,
    state_of: &dyn Fn(i32) -> Option<i32>,
    standing: &BTreeMap<i32, usize>,
) -> Value {
    let line = |provinces: &[i32]| -> Value {
        let on: Vec<usize> = provinces
            .iter()
            .map(|p| standing.get(p).copied().unwrap_or(0))
            .collect();
        json!({"provinces": provinces.len(),
               "ends": provinces.first().zip(provinces.last()).map(|(a, b)| json!([a, b])),
               "states": states_along(provinces, state_of),
               "divisions_on_line": on.iter().sum::<usize>(),
               "most_in_one_province": on.iter().copied().max().unwrap_or(0)})
    };
    let kind = kind_name(node.kind, list);
    let mut row = json!({"kind": kind, "id": node.id, "executing": node.executing,
                         "divisions": node.divisions});
    if node.kind == 5 {
        row["states"] = json!(node.states);
    } else {
        row["line"] = line(&node.provinces);
    }
    let offensives: Vec<Value> = node
        .children
        .iter()
        .filter(|c| c.kind == 1)
        .map(|arrow| {
            let target = arrow.children.iter().find(|t| t.kind == 2);
            json!({"id": arrow.id, "executing": arrow.executing, "divisions": arrow.divisions,
                   "path_provinces": arrow.provinces.len(),
                   "path_states": states_along(&arrow.provinces, state_of),
                   "target": target.map(|t| json!({"provinces": t.provinces.len(),
                       "ends": t.provinces.first().zip(t.provinces.last()).map(|(a, b)| json!([a, b])),
                       "states": states_along(&t.provinces, state_of)}))})
        })
        .collect();
    if !offensives.is_empty() {
        row["offensives"] = json!(offensives);
    }
    row
}

fn i32_array(r: &Reader<'_>, at: u64, cap: usize) -> Result<Vec<i32>> {
    let (_, data, count) = r.array_header(at)?;
    let n = count.min(cap);
    if n == 0 {
        return Ok(Vec::new());
    }
    let bytes = r.read(data, n * 4)?;
    Ok(bytes
        .chunks_exact(4)
        .map(|c| i32::from_le_bytes(c.try_into().unwrap()))
        .collect())
}

fn node(r: &Reader<'_>, image: u64, order: u64, depth: usize) -> Result<OrderNode> {
    let u = ui(r)?;
    if r.u64(order)? != image + u.order_vtable {
        return Err(Error::new(
            "invalid_army_type",
            "Exact battle-plan order required",
        ));
    }
    let abi = r.string_abi();
    let f = |mac: u64| fld(abi, &ORDER, mac);
    let mut out = OrderNode {
        kind: r.i32(order + f(KIND)?)?,
        id: r.i32(order + f(ID)?)?,
        executing: r.read(order + f(EXECUTING)?, 1)?[0] != 0,
        provinces: i32_array(r, order + f(PROVINCES)?, 4096)?,
        states: i32_array(r, order + f(STATES)?, 1024)?,
        divisions: r.array_header(order + f(DIVISIONS)?)?.2,
        children: Vec::new(),
    };
    if depth < 3 {
        let (_, data, count) = r.array_header(order + f(CHILDREN)?)?;
        for index in 0..count.min(32) as u64 {
            out.children
                .push(node(r, image, r.u64(data + index * 8)?, depth + 1)?);
        }
    }
    Ok(out)
}

/// Every order of the orders group at `group`, with the list it sits in.
pub(crate) fn group_orders(
    r: &Reader<'_>,
    image: u64,
    group: u64,
) -> Result<Vec<(List, OrderNode)>> {
    let u = ui(r)?;
    let mut out = Vec::new();
    let lists = std::iter::once((List::Top, Some(u.group_orders))).chain(
        u.group_fallback_lists
            .into_iter()
            .map(|l| (List::Defensive, l)),
    );
    for (list, at) in lists {
        let Some(at) = at else { continue };
        let (_, data, count) = r.array_header(group + at)?;
        for index in 0..count.min(64) as u64 {
            out.push((list, node(r, image, r.u64(data + index * 8)?, 0)?));
        }
    }
    Ok(out)
}

/// Province id -> state id, from the game's province table.
pub(crate) fn state_of_province(r: &Reader<'_>, image: u64, province: i32) -> Option<i32> {
    let g = crate::game::layout(r);
    let l = crate::construction::layout(r);
    let gs = r.u64(image + g.gs_global).ok()?;
    let raw = r
        .array_entry(gs + l.gs_provinces, province as usize, 8)
        .ok()?;
    let p = u64::from_le_bytes(raw.try_into().ok()?);
    if p == 0 || r.u64(p).ok()? != image + l.province_vtable {
        return None;
    }
    let state = r.u64(p + l.province_state).ok()?;
    if state == 0 || r.u64(state).ok()? != image + l.state_vtable {
        return None;
    }
    r.i32(state + l.state_id).ok()
}

/// The orders of `group` as JSON rows; `standing` counts its divisions per province.
pub(crate) fn rows(
    r: &Reader<'_>,
    image: u64,
    group: u64,
    standing: &BTreeMap<i32, usize>,
) -> Result<Vec<Value>> {
    let state_of = |p: i32| state_of_province(r, image, p);
    Ok(group_orders(r, image, group)?
        .iter()
        .map(|(list, n)| order_json(n, *list, &state_of, standing))
        .collect())
}

/// The orders of the army or army group named `army` (see `rows`).
pub(crate) fn of_named(
    r: &Reader<'_>,
    image: u64,
    army: &str,
    standing: &BTreeMap<i32, usize>,
) -> Result<Vec<Value>> {
    rows(r, image, group_named(r, image, army)?, standing)
}

/// A short description of what is left on a group: "front x1, offensive x1" (for refusals).
pub(crate) fn summary(nodes: &[(List, OrderNode)]) -> String {
    let mut n: BTreeMap<String, usize> = BTreeMap::new();
    for (list, o) in nodes {
        *n.entry(kind_name(o.kind, *list)).or_default() += 1;
        let arrows = o.children.iter().filter(|c| c.kind == 1).count();
        if arrows > 0 {
            *n.entry("offensive".into()).or_default() += arrows;
        }
    }
    n.into_iter()
        .map(|(k, v)| format!("{k} x{v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(p: i32) -> Option<i32> {
        Some(p / 10)
    }

    #[test]
    fn a_group_front_with_a_deep_offensive_reads_its_line_arrow_and_target() {
        let target = OrderNode {
            kind: 2,
            id: 192,
            provinces: vec![851, 861, 871, 881],
            divisions: 47,
            ..Default::default()
        };
        let arrow = OrderNode {
            kind: 1,
            id: 193,
            executing: true,
            provinces: vec![671, 861],
            divisions: 47,
            children: vec![target],
            ..Default::default()
        };
        let front = OrderNode {
            kind: 2,
            id: 190,
            executing: true,
            provinces: vec![631, 632, 661, 671],
            divisions: 47,
            children: vec![arrow],
            ..Default::default()
        };
        let standing = BTreeMap::from([(631, 2), (661, 3)]);
        let row = order_json(&front, List::Top, &st, &standing);
        assert_eq!(row["kind"], "front");
        assert_eq!(row["executing"], true);
        assert_eq!(row["line"]["provinces"], 4);
        assert_eq!(row["line"]["ends"], json!([631, 671]));
        assert_eq!(row["line"]["states"], json!([63, 66, 67]));
        assert_eq!(row["line"]["divisions_on_line"], 5);
        assert_eq!(row["line"]["most_in_one_province"], 3);
        let off = &row["offensives"][0];
        assert_eq!(off["path_states"], json!([67, 86]));
        assert_eq!(off["target"]["provinces"], 4);
        assert_eq!(off["target"]["states"], json!([85, 86, 87, 88]));
    }

    #[test]
    fn area_defence_lists_states_and_a_defensive_line_is_a_fallback() {
        let area = OrderNode {
            kind: 5,
            states: vec![55, 56],
            divisions: 10,
            ..Default::default()
        };
        let row = order_json(&area, List::Top, &st, &BTreeMap::new());
        assert_eq!(row["kind"], "area_defense");
        assert_eq!(row["states"], json!([55, 56]));
        assert!(row.get("line").is_none());
        assert_eq!(kind_name(2, List::Defensive), "fallback");
        assert_eq!(kind_name(9, List::Top), "kind_9");
    }

    #[test]
    fn the_summary_counts_fronts_and_their_offensives() {
        let arrow = OrderNode {
            kind: 1,
            ..Default::default()
        };
        let front = OrderNode {
            kind: 2,
            children: vec![arrow],
            ..Default::default()
        };
        let fb = OrderNode {
            kind: 2,
            ..Default::default()
        };
        assert_eq!(
            summary(&[(List::Top, front), (List::Defensive, fb)]),
            "fallback x1, front x1, offensive x1"
        );
    }
}
