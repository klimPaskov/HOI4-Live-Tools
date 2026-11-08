//! `hoi4_war_stats`: casualties of every country in each war, from the war
//! relations in memory. Pure read: no input, no lease, no cursor, no pause.
//!
//! Provenance (macOS 1.19.4.1, found 5 Nov 2026 by static analysis of the
//! game's own `casualties` trigger and live reads on an Italy 1936-37 run):
//! - `CCasualtiesInThousandsTrigger` evaluates `CCountry+0xd30`
//!   (`CDiplomacyStatus`) through the function at image `+0xf6f510`: for each
//!   relation of the diplomacy status whose relation type (`+0x8`) is the war
//!   type `0x380a` it adds `combat(country)` and keeps the maximum of
//!   `extra(country)`, returning `sum(combat) + max(extra)`.
//! - `combat` (`+0xfd8350`) returns the i64 at `CWarRelation+0x50` when the
//!   country is the relation's first party (`+0xc`), else `+0x58`; `extra`
//!   (`+0xfd8410`) the i64 at `+0x60` / `+0x68` the same way. Plain counts of
//!   men (the trigger divides by 1000 for `casualties_k`).
//! - Live (Italy vs Ethiopia): ITA `+0x50` = 38715, ETH `+0x58` = 54373 in
//!   March 1937, 40300 and 61916 on 21 Aug, 47410 and 135193 on 19 Nov 1937
//!   (only ever growing, Ethiopia losing far more); extra 0 for both. The Sino-Japanese war read at the same
//!   time: China and the warlords 70115 on their side, Japan, Mengkukuo and
//!   Manchukuo 20210. Wars are grouped here as connected components of
//!   the war relations, sides by two-colouring (an inference, not the game's
//!   own war object).
use super::{
    diplomacy_state::{self, date_json, Names, WarRelation},
    ui::read,
};
use crate::{
    error::Result,
    game,
    memory::Reader,
    platform::Process,
    xlayout::{fld, Class},
};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Instant,
};

const WAR_RELATION: Class = Class::new("CWarRelation", 0x354a170);
const CASUALTIES_FIRST: u64 = 0x50;
const CASUALTIES_SECOND: u64 = 0x58;
const EXTRA_FIRST: u64 = 0x60;
const EXTRA_SECOND: u64 = 0x68;
/// Larger than any plausible war loss count; a value outside 0..LIMIT is
/// reported unreadable instead of passed on.
const CASUALTY_LIMIT: i64 = 1_000_000_000;

/// One war relation with the counts of its two holders.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    ptr: u64,
    start: i32,
    /// the two countries holding the relation (pointer identity)
    holders: [i32; 2],
    /// casualties of holders[0], holders[1]; `None` when the holders are not
    /// exactly the relation's first and second party (civil war ids)
    casualties: Option<[i64; 2]>,
    extra: Option<[i64; 2]>,
    raw: RawCounts,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct RawCounts {
    a: i32,
    b: i32,
    first: i64,
    second: i64,
    extra_first: i64,
    extra_second: i64,
}

/// Pair the raw counts with the holders: the first-party count belongs to the
/// holder whose tag is the relation's first party, as the game's getter does.
fn attribute(holders: [i32; 2], raw: &RawCounts) -> (Option<[i64; 2]>, Option<[i64; 2]>) {
    let ok = |v: i64| (0..CASUALTY_LIMIT).contains(&v);
    if ![raw.first, raw.second, raw.extra_first, raw.extra_second]
        .into_iter()
        .all(ok)
    {
        return (None, None);
    }
    if holders == [raw.a, raw.b] {
        (
            Some([raw.first, raw.second]),
            Some([raw.extra_first, raw.extra_second]),
        )
    } else if holders == [raw.b, raw.a] {
        (
            Some([raw.second, raw.first]),
            Some([raw.extra_second, raw.extra_first]),
        )
    } else {
        (None, None)
    }
}

/// Casualties per country by the game's formula: the sum of the combat
/// counts over its war relations plus the largest extra count.
fn country_totals(rows: &[Row]) -> BTreeMap<i32, i64> {
    let mut sum: BTreeMap<i32, i64> = BTreeMap::new();
    let mut extra: BTreeMap<i32, i64> = BTreeMap::new();
    for row in rows {
        let (Some(c), Some(e)) = (row.casualties, row.extra) else {
            continue;
        };
        for k in 0..2 {
            *sum.entry(row.holders[k]).or_default() += c[k];
            let m = extra.entry(row.holders[k]).or_default();
            *m = (*m).max(e[k]);
        }
    }
    for (tag, e) in extra {
        *sum.entry(tag).or_default() += e;
    }
    sum
}

/// A war: connected relations, the two sides (two-colouring) and whether the
/// colouring was consistent.
#[derive(Debug, PartialEq)]
struct Group {
    rows: Vec<usize>,
    sides: [BTreeSet<i32>; 2],
    consistent: bool,
}

fn group_wars(rows: &[Row]) -> Vec<Group> {
    let mut adjacency: BTreeMap<i32, Vec<(i32, usize)>> = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let [x, y] = row.holders;
        adjacency.entry(x).or_default().push((y, i));
        adjacency.entry(y).or_default().push((x, i));
    }
    let mut colour: BTreeMap<i32, usize> = BTreeMap::new();
    let mut groups = Vec::new();
    for &start in adjacency.keys() {
        if colour.contains_key(&start) {
            continue;
        }
        let mut group = Group {
            rows: Vec::new(),
            sides: [BTreeSet::new(), BTreeSet::new()],
            consistent: true,
        };
        let mut seen_rows = BTreeSet::new();
        let mut queue = VecDeque::from([start]);
        colour.insert(start, 0);
        while let Some(country) = queue.pop_front() {
            let c = colour[&country];
            group.sides[c].insert(country);
            for &(other, row) in &adjacency[&country] {
                if seen_rows.insert(row) {
                    group.rows.push(row);
                }
                match colour.get(&other) {
                    Some(&oc) if oc == c => group.consistent = false,
                    Some(_) => {}
                    None => {
                        colour.insert(other, 1 - c);
                        queue.push_back(other);
                    }
                }
            }
        }
        group.rows.sort_unstable();
        groups.push(group);
    }
    groups
}

fn read_row(r: &Reader<'_>, rel: &WarRelation) -> Result<Option<Row>> {
    if rel.holders.len() != 2 {
        return Ok(None);
    }
    let abi = r.string_abi();
    let i64_at =
        |off: u64| -> Result<i64> { Ok(r.u64(rel.ptr + fld(abi, &WAR_RELATION, off)?)? as i64) };
    let raw = RawCounts {
        a: rel.a,
        b: rel.b,
        first: i64_at(CASUALTIES_FIRST)?,
        second: i64_at(CASUALTIES_SECOND)?,
        extra_first: i64_at(EXTRA_FIRST)?,
        extra_second: i64_at(EXTRA_SECOND)?,
    };
    let holders = [rel.holders[0], rel.holders[1]];
    let (casualties, extra) = attribute(holders, &raw);
    Ok(Some(Row {
        ptr: rel.ptr,
        start: rel.start,
        holders,
        casualties,
        extra,
        raw,
    }))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_war_stats","description":"Read-only (memory, no input, no pause): casualties per country in each war, as the war overview counts them. Each war is a group of war relations (sides inferred by who fights whom); per relation the losses of both countries (men), per war the sum per side, and per country the game's own total (the `casualties` trigger formula: sum over its wars plus the largest extra count). By default only the wars the player fights; all:true lists every war in the world. War score, leaders and goals are not read here (see hoi4_diplomacy_state).","inputSchema":{"type":"object","properties":{"all":{"type":"boolean"}},"additionalProperties":false}})
}

pub(super) fn stats(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    let started = Instant::now();
    let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
    let mut out = read(process, |r, image| {
        let w = diplomacy_state::world(r, image)?;
        let mut rows = Vec::new();
        let mut skipped = 0;
        for rel in diplomacy_state::war_relations(r, image, &w)? {
            match read_row(r, &rel)? {
                Some(row) => rows.push(row),
                None => skipped += 1,
            }
        }
        let totals = country_totals(&rows);
        let mut names = Names::new(r, image, &w);
        let date = r.i32(w.gs + game::layout(r).date)?;
        let mut wars = Vec::new();
        for group in group_wars(&rows) {
            let has_player = group.sides.iter().any(|s| s.contains(&w.player_tag));
            if !all && !has_player {
                continue;
            }
            // the player's side first
            let order = if group.sides[1].contains(&w.player_tag) {
                [1, 0]
            } else {
                [0, 1]
            };
            let mut side_sum = [0i64; 2];
            let mut side_known = [true; 2];
            let mut relations = Vec::new();
            for &i in &group.rows {
                let row = &rows[i];
                let mut entry = Vec::new();
                for k in 0..2 {
                    let tag = row.holders[k];
                    let side = order
                        .iter()
                        .position(|&s| group.sides[s].contains(&tag))
                        .unwrap_or(0);
                    match row.casualties {
                        Some(c) => side_sum[side] += c[k],
                        None => side_known[side] = false,
                    }
                    let mut who = names.who(tag);
                    who["casualties"] = json!(row.casualties.map(|c| c[k]));
                    who["extra"] = json!(row.extra.map(|e| e[k]));
                    entry.push(who);
                }
                let mut rel = json!({"relation_id":format!("0x{:x}",row.ptr),
                    "since":date_json(row.start),"countries":entry,
                    "attributed":row.casualties.is_some()});
                if row.casualties.is_none() {
                    rel["raw"] = json!({"first_party":names.who(row.raw.a),
                        "second_party":names.who(row.raw.b),
                        "first_casualties":row.raw.first,"second_casualties":row.raw.second,
                        "first_extra":row.raw.extra_first,"second_extra":row.raw.extra_second,
                        "reason":"holders are not the relation's first/second party (civil war ids) or a count is out of range"});
                }
                relations.push(rel);
            }
            let sides: Vec<Value> = order
                .iter()
                .enumerate()
                .map(|(n, &s)| {
                    let members: Vec<Value> = group.sides[s]
                        .iter()
                        .map(|t| {
                            let mut v = names.who(*t);
                            v["casualties_total"] = json!(totals.get(t));
                            v
                        })
                        .collect();
                    json!({"countries":members,
                        "casualties_in_this_war":if side_known[n] { json!(side_sum[n]) } else { Value::Null },
                        "includes_player":group.sides[s].contains(&w.player_tag)})
                })
                .collect();
            wars.push(json!({"includes_player":has_player,"sides":sides,
                "sides_consistent":group.consistent,"relations":relations}));
        }
        Ok(json!({
            "source":"native war relations (memory)",
            "player":names.who(w.player_tag),
            "date":date_json(date),
            "scope":if all { "all" } else { "player" },
            "count":wars.len(),
            "wars":wars,
            "skipped_relations":skipped,
            "skipped_reason":"war relations not held by exactly two listed countries (countries created in game, e.g. civil war sides)",
            "units":"men (plain count; the game's casualties_k is this / 1000)",
            "validated":{"casualties":true,"extra":false,"sides":false},
            "validation":{
                "offsets":"CWarRelation +0x50/+0x58 (+0x60/+0x68 extra) by the game's casualties trigger code (static, image +0xf6f510, +0xfd8350, +0xfd8410)",
                "live":"Italy vs Ethiopia: ITA 38715 / ETH 54373 in March 1937, 40300 / 61916 on 21 Aug, 46836 / 123848 on 7 Nov, 47410 / 135193 on 19 Nov 1937 (only growing, Ethiopia losing far more during its offensive); extra 0 everywhere so far, its meaning is not known; not yet compared with the war overview screen",
                "sides":"inferred by two-colouring the relation graph, not read from the game's war object",
                "casualties_total":"per-country total over all its wars, by the trigger's formula"},
            "observation_paused_game":false,
            "input_authorized":false,
        }))
    })?;
    out["freshness"] = json!({"elapsed_ms": started.elapsed().as_secs_f64() * 1000.0});
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(a: i32, b: i32, first: i64, second: i64) -> RawCounts {
        RawCounts {
            a,
            b,
            first,
            second,
            extra_first: 0,
            extra_second: 0,
        }
    }

    fn row(holders: [i32; 2], raw: RawCounts) -> Row {
        let (casualties, extra) = attribute(holders, &raw);
        Row {
            ptr: 0x1000 + holders[0] as u64 * 16 + holders[1] as u64,
            start: 0,
            holders,
            casualties,
            extra,
            raw,
        }
    }

    #[test]
    fn attributes_counts_by_first_party() {
        let r = raw(16, 44, 38715, 54373);
        assert_eq!(attribute([16, 44], &r).0, Some([38715, 54373]));
        assert_eq!(attribute([44, 16], &r).0, Some([54373, 38715]));
        // civil war ids: holders are not the parties
        assert_eq!(attribute([16, 440], &r).0, None);
        // out of range counts are not passed on
        assert_eq!(attribute([16, 44], &raw(16, 44, -5, 3)).0, None);
    }

    #[test]
    fn totals_follow_the_trigger_formula() {
        let mut a = raw(1, 2, 100, 200);
        a.extra_first = 7;
        let mut b = raw(1, 3, 10, 20);
        b.extra_first = 5;
        let rows = vec![row([1, 2], a), row([1, 3], b)];
        let totals = country_totals(&rows);
        // sum of combat (110) plus the largest extra (7), not 7 + 5
        assert_eq!(totals[&1], 117);
        assert_eq!(totals[&2], 200);
        assert_eq!(totals[&3], 20);
    }

    #[test]
    fn groups_and_sides() {
        // ITA-ETH, and a separate two-against-one war
        let rows = vec![
            row([16, 44], raw(16, 44, 1, 2)),
            row([5, 7], raw(5, 7, 3, 4)),
            row([6, 7], raw(6, 7, 5, 6)),
        ];
        let groups = group_wars(&rows);
        assert_eq!(groups.len(), 2);
        let g = groups.iter().find(|g| g.rows == vec![1, 2]).unwrap();
        assert!(g.consistent);
        let sides: Vec<Vec<i32>> = g
            .sides
            .iter()
            .map(|s| s.iter().copied().collect())
            .collect();
        assert!(sides.contains(&vec![5, 6]) && sides.contains(&vec![7]));
        // a triangle cannot be two-coloured
        let tri = vec![
            row([1, 2], raw(1, 2, 0, 0)),
            row([2, 3], raw(2, 3, 0, 0)),
            row([1, 3], raw(1, 3, 0, 0)),
        ];
        assert!(!group_wars(&tri)[0].consistent);
    }
}
