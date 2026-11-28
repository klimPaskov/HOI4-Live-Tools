//! CAPACITY (pure): what each stretch of a unit's line can fight with and feed, and where the
//! divisions it cannot use there should go instead.
//!
//! A STRETCH is one plan root (a front order of the unit) with the own provinces of its line.
//! Its ROOM is read from the same facts the ground model uses:
//! - WIDTH: the divisions its battles take with profit (`ground::province_caps`: every enemy
//!   province facing the line takes `best_fit` of its combat width from as many directions as
//!   own provinces touch it, shared among those provinces), summed over the stretch's provinces;
//! - SUPPLY: the divisions the supply reaching the stretch feeds at `floor` of their need
//!   (`hoi4_supply` provinces: the supply each province receives, the demand of one division as
//!   the read's own mean); provinces without a read count at the mean of the read ones;
//! - HOLD: the hostile divisions our country sees facing it (`hoi4_wars` border rows, spread over
//!   the facing provinces of each state), times `hold_ratio`: a stretch the enemy masses against
//!   is never left with one division (o96a, 28 Nov 2026: one German division in Luxembourg
//!   against 18 French ones).
//!
//! cap = max(min(width, supply), hold), at least one while it faces an enemy province. The caps
//! are the weights the unit's divisions are spread over its fronts by (`hoi4_army_plan
//! distribute {weights}`), so a narrow forest stretch with no railway gets what it can fight
//! with and feed, and the rest goes to the stretches with room.
//!
//! SIDEWAYS (`sideways`): at war a battle whose reserve outnumbers its engaged divisions holds
//! divisions that fight nowhere; the surplus (reserve beyond the engaged) moves to the stretch
//! with the most room (cap minus its divisions) that is not the battle's own (`hoi4_army_plan
//! assign {order, divisions}`), the freshest first. o96a (28 Nov 2026): 20 of 24 reserve
//! divisions sat behind two full Ardennes forest battles (3488: 9 engaged and 11 in reserve) while
//! 7-18 fit divisions stood on the line beside enemy provinces nobody attacked.
//!
//! BREADTH (`breadth`): the enemy provinces touching ours, how many have a battle of ours, and
//! how many of those are attacked from fewer sides than we border them: the reading the note
//! carries every war day (o96a: 3-7 of 7-11 attacked; 5 of 6 battles from one direction).
use super::ground::{self, Prov};
use super::{list, text};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The supply share a division is fed at for the stretch's carry (`capacity.supply_floor`): half,
/// as the stacking carry (STACK_FLOOR).
pub(super) const FEED_FLOOR: f64 = 0.5;
/// The divisions a stretch keeps against each hostile division it faces (`capacity.hold_ratio`).
pub(super) const HOLD_RATIO: f64 = 1.0;
/// A division is fit for a direct attack at this share of the best organisation of its template
/// among ours (a fresh division stands at its maximum).
pub(super) const FIT_ORG: f64 = 0.7;

/// One stretch: a front order of the unit with the own provinces of its line and its divisions.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Stretch {
    pub id: i64,
    pub provinces: Vec<i64>,
    pub divisions: i64,
}

/// What a stretch can use.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Room {
    pub id: i64,
    pub width: i64,
    /// None: no province of the stretch has a supply read
    pub supply: Option<i64>,
    pub hold: i64,
    pub cap: i64,
    pub divisions: i64,
    /// enemy provinces the stretch faces
    pub facing: usize,
}

impl Room {
    /// Divisions it can still take (negative: over its cap).
    pub(super) fn free(&self) -> i64 {
        self.cap - self.divisions
    }
}

/// The supply a `hoi4_supply` read shows: province -> supply received, and the demand of one
/// division (the read's total demand over its divisions).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Feed {
    pub supply: HashMap<i64, f64>,
    pub per_division: Option<f64>,
}

pub(super) fn feed(read: &Value) -> Feed {
    let mut out = Feed::default();
    let (mut demand, mut divisions) = (0.0, 0.0);
    for p in list(read, "provinces") {
        let Some(id) = p["province_id"].as_i64().or(p["province"].as_i64()) else {
            continue;
        };
        if let Some(s) = p["supply"].as_f64() {
            out.supply.insert(id, s.max(0.0));
        }
        let n = p["divisions"].as_f64().unwrap_or(0.0);
        if let Some(d) = p["demand"].as_f64().filter(|d| *d > 0.0 && n > 0.0) {
            demand += d;
            divisions += n;
        }
    }
    if divisions > 0.0 {
        out.per_division = Some(demand / divisions);
    }
    out
}

/// The room of every stretch (in the order given). `enemy` holds the enemy provinces, `width_at`
/// the division width that stands in an own province, `seen` the hostile divisions our country
/// sees per enemy state. Pure.
#[allow(clippy::too_many_arguments)]
pub(super) fn rooms(
    g: &HashMap<i64, Prov>,
    stretches: &[Stretch],
    enemy: &HashSet<i64>,
    width_at: &dyn Fn(i64) -> f64,
    feed: &Feed,
    seen: &HashMap<i64, f64>,
    floor: f64,
    hold_ratio: f64,
) -> Vec<Room> {
    let front: HashSet<i64> = stretches
        .iter()
        .flat_map(|s| s.provinces.iter().copied())
        .collect();
    // a province on two stretches counts half on each
    let mut shared: HashMap<i64, f64> = HashMap::new();
    for s in stretches {
        let mut mine: Vec<i64> = s.provinces.clone();
        mine.sort_unstable();
        mine.dedup();
        for p in mine {
            *shared.entry(p).or_default() += 1.0;
        }
    }
    let caps = ground::province_caps(g, &front, &|q| enemy.contains(&q), width_at, 0);
    // the own front provinces touching each enemy province, and the enemy provinces of each
    // state that face our front (the seen divisions of a state are spread over them)
    let mut sides: HashMap<i64, Vec<i64>> = HashMap::new();
    for p in &front {
        for (q, _) in g.get(p).map(|r| r.borders.as_slice()).unwrap_or(&[]) {
            if enemy.contains(q) && g.contains_key(q) {
                sides.entry(*q).or_default().push(*p);
            }
        }
    }
    let mut facing_of_state: HashMap<i64, f64> = HashMap::new();
    for e in sides.keys() {
        if let Some(r) = g.get(e) {
            *facing_of_state.entry(r.state).or_default() += 1.0;
        }
    }
    stretches
        .iter()
        .map(|s| {
            let mine: HashSet<i64> = s.provinces.iter().copied().collect();
            let width: f64 = mine
                .iter()
                .filter_map(|p| Some(caps.get(p)?.cap as f64 / shared.get(p).copied()?))
                .sum();
            // supply: the read provinces as read, the stretch's others at its read mean
            let read: Vec<f64> = mine
                .iter()
                .filter_map(|p| feed.supply.get(p).copied())
                .collect();
            let supply = match feed.per_division {
                Some(need) if need > 0.0 && !read.is_empty() => {
                    let mean = read.iter().sum::<f64>() / read.len() as f64;
                    let total: f64 = mine
                        .iter()
                        .map(|p| feed.supply.get(p).copied().unwrap_or(mean) / shared[p])
                        .sum();
                    Some((total / (floor.max(0.05) * need)).floor() as i64)
                }
                _ => None,
            };
            let mut facing = 0;
            let mut hold = 0.0;
            for (e, from) in &sides {
                let here = from.iter().filter(|p| mine.contains(*p)).count();
                if here == 0 {
                    continue;
                }
                facing += 1;
                let state = g.get(e).map_or(0, |r| r.state);
                let per = seen.get(&state).copied().unwrap_or(0.0)
                    / facing_of_state.get(&state).copied().unwrap_or(1.0);
                hold += per * here as f64 / from.len() as f64;
            }
            let width = (width - 1e-9).ceil().max(0.0) as i64;
            let hold = (hold * hold_ratio - 1e-9).ceil().max(0.0) as i64;
            let fight = supply.map_or(width, |c| width.min(c));
            let mut cap = fight.max(hold);
            if facing > 0 {
                cap = cap.max(1);
            }
            Room {
                id: s.id,
                width,
                supply,
                hold,
                cap,
                divisions: s.divisions,
                facing,
            }
        })
        .collect()
}

/// One line of the rooms for a note: "id: N of cap C (width W, supply S, hold H)".
pub(super) fn describe(rooms: &[Room]) -> String {
    rooms
        .iter()
        .map(|r| {
            format!(
                "{}: {} of {} (width {}, supply {}, hold {}, {} enemy provinces)",
                r.id,
                r.divisions,
                r.cap,
                r.width,
                r.supply.map_or("unread".to_string(), |s| s.to_string()),
                r.hold,
                r.facing
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Whether the divisions stand far from the rooms: a stretch over its cap by at least `slack`
/// while another has at least `slack` free (the spread by capacity is worth its move). Pure.
pub(super) fn unbalanced(rooms: &[Room], slack: i64) -> bool {
    rooms.iter().any(|r| r.free() <= -slack) && rooms.iter().any(|r| r.free() >= slack)
}

/// The weight a stretch is spread by: its cap, at least one while it faces an enemy, none when it
/// faces no enemy province (a front the line has left behind keeps no division the game can move).
pub(super) fn weight(r: &Room) -> i64 {
    if r.facing == 0 {
        0
    } else {
        r.cap.max(1)
    }
}

/// A battle whose reserve outnumbers its engaged divisions, with our divisions waiting in it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Pile {
    pub province: i64,
    pub engaged: i64,
    pub reserve: i64,
    /// our reserve divisions there: (id, organisation, the province it stands in)
    pub waiting: Vec<(String, f64, Option<i64>)>,
}

fn id_of(v: &Value) -> Option<String> {
    v.as_str()
        .map(String::from)
        .or_else(|| v.as_i64().map(|n| n.to_string()))
}

/// The battles we attack whose reserve outnumbers the engaged, with the reserve divisions of
/// `ours` in them, the fullest first. Pure.
pub(super) fn piles(battles: &Value, ours: &HashSet<String>) -> Vec<Pile> {
    let mut out: Vec<Pile> = list(battles, "list")
        .iter()
        .filter(|b| text(&b["our_side"]) == "attacker")
        .filter_map(|b| {
            let side = &b["attacker"];
            let engaged = side["engaged"].as_i64().unwrap_or(0);
            let reserve = side["reserve"].as_i64().unwrap_or(0);
            if reserve <= engaged {
                return None;
            }
            let waiting: Vec<(String, f64, Option<i64>)> = list(side, "reserve_divisions")
                .iter()
                .filter_map(|d| {
                    let id = id_of(&d["id"])?;
                    ours.contains(&id)
                        .then(|| (id, d["org"].as_f64().unwrap_or(0.0), d["province"].as_i64()))
                })
                .collect();
            (!waiting.is_empty()).then(|| Pile {
                province: b["province"].as_i64().unwrap_or(0),
                engaged,
                reserve,
                waiting,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        (b.reserve - b.engaged)
            .cmp(&(a.reserve - a.engaged))
            .then(a.province.cmp(&b.province))
    });
    out
}

/// The own provinces holding more of `ours` than their battles take (`caps`: province -> the
/// divisions its battles take, `ground::province_caps`), as piles whose surplus is the standing
/// divisions beyond the cap (no march or attack under way: `heading_province` empty), from a
/// `hoi4_supply` read, the fullest first. w96a (28 Nov 2026, W+1): 10 divisions in 9522 for 4,
/// 9 in 11470 for 2. Pure.
pub(super) fn overstacked(
    read: &Value,
    caps: &HashMap<i64, i64>,
    ours: &HashSet<String>,
) -> Vec<Pile> {
    let mut at: HashMap<i64, Vec<(String, f64, Option<i64>)>> = HashMap::new();
    let mut count: HashMap<i64, i64> = HashMap::new();
    for d in list(read, "divisions") {
        let (Some(id), Some(p)) = (id_of(&d["id"]), d["province_id"].as_i64()) else {
            continue;
        };
        if !ours.contains(&id) {
            continue;
        }
        *count.entry(p).or_default() += 1;
        if d["heading_province"].is_null() {
            at.entry(p)
                .or_default()
                .push((id, d["organisation"].as_f64().unwrap_or(0.0), Some(p)));
        }
    }
    let mut out: Vec<Pile> = count
        .into_iter()
        .filter_map(|(p, n)| {
            let cap = *caps.get(&p)?;
            let waiting = at.remove(&p).unwrap_or_default();
            (n > cap && !waiting.is_empty()).then_some(Pile {
                province: p,
                engaged: cap,
                reserve: n,
                waiting,
            })
        })
        .collect();
    out.sort_by(|a, b| {
        (b.reserve - b.engaged)
            .cmp(&(a.reserve - a.engaged))
            .then(a.province.cmp(&b.province))
    });
    out
}

/// Our divisions waiting on a pile: (id, organisation, the province it stands in).
type Waiting = Vec<(String, f64, Option<i64>)>;

/// DEAD FRONTS: at war a stretch that faces no enemy province (`facing` 0) while our divisions
/// stand on it holds divisions that fight nowhere; they are piles whose whole count is surplus
/// (engaged 0), so `sideways` sends them to the stretch with most room. `root_of`: the stretch
/// of an own province. p101a (28 Nov 2026): four second fronts read 'width 0, hold 0, 0 enemy
/// provinces' and kept 1-9 divisions each (22 in all) through 33 days of the war on Poland,
/// while the fronts facing the Poles had room. Standing divisions only (no march under way),
/// the most divisions first. Pure.
pub(super) fn dead_fronts(
    read: &Value,
    rooms: &[Room],
    root_of: &dyn Fn(i64) -> Option<i64>,
    ours: &HashSet<String>,
) -> Vec<Pile> {
    let dead: HashSet<i64> = rooms
        .iter()
        .filter(|r| r.facing == 0 && r.divisions > 0)
        .map(|r| r.id)
        .collect();
    if dead.is_empty() {
        return Vec::new();
    }
    let mut at: HashMap<i64, (i64, Waiting)> = HashMap::new();
    for d in list(read, "divisions") {
        let (Some(id), Some(p)) = (id_of(&d["id"]), d["province_id"].as_i64()) else {
            continue;
        };
        if !ours.contains(&id) || !d["heading_province"].is_null() {
            continue;
        }
        let Some(root) = root_of(p).filter(|r| dead.contains(r)) else {
            continue;
        };
        let e = at.entry(root).or_insert((p, Vec::new()));
        e.1.push((id, d["organisation"].as_f64().unwrap_or(0.0), Some(p)));
    }
    let mut out: Vec<Pile> = at
        .into_values()
        .map(|(p, waiting)| Pile {
            province: p,
            engaged: 0,
            reserve: waiting.len() as i64,
            waiting,
        })
        .collect();
    out.sort_by(|a, b| b.reserve.cmp(&a.reserve).then(a.province.cmp(&b.province)));
    out
}

/// A direct attack on a stuck battle from a side the plan leaves unused.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Push {
    /// the enemy province of the battle, and its state (the move tool takes both)
    pub province: i64,
    pub state: i64,
    pub army: String,
    pub divisions: Vec<String>,
    /// the own provinces the new attackers stand in
    pub from: Vec<i64>,
    pub hours: i64,
    /// own provinces bordering the battle, and how many of them attack it now
    pub sides: usize,
    pub used: usize,
}

/// THE STUCK POINT (pure): a battle we attack that has lasted `min_hours` or more, is not being
/// won (`winning` is not the attacker) and has width to spare, bordered by own provinces whose
/// divisions do not attack it: the fit idle divisions there (standing, no march under way, in no
/// battle, organisation at least FIT_ORG of the best of their template among ours) of the one army
/// that has most of them, at most what the spare width takes and `most`. The battle that has
/// stood longest first. o96a (28 Nov 2026): 3-7 of the 7-11 enemy provinces in reach attacked,
/// fit divisions on the line beside full battles. `skip`: provinces pushed lately. `armies`: whose
/// divisions may go.
#[allow(clippy::too_many_arguments)]
pub(super) fn stuck_push(
    battles: &Value,
    g: &HashMap<i64, Prov>,
    read: &Value,
    armies: &HashSet<String>,
    min_hours: i64,
    most: usize,
    skip: &dyn Fn(i64) -> bool,
) -> Option<Push> {
    let rows = |side: &Value| -> Vec<Value> {
        list(side, "engaged_divisions")
            .iter()
            .chain(list(side, "reserve_divisions").iter())
            .cloned()
            .collect()
    };
    let mut fighting: HashSet<String> = HashSet::new();
    for b in list(battles, "list") {
        for side in ["attacker", "defender"] {
            fighting.extend(rows(&b[side]).iter().filter_map(|d| id_of(&d["id"])));
        }
    }
    let mine: Vec<&Value> = list(read, "divisions")
        .iter()
        .filter(|d| armies.contains(text(&d["army"])))
        .collect();
    let mut best_org: HashMap<String, f64> = HashMap::new();
    for d in &mine {
        let o = d["organisation"].as_f64().unwrap_or(0.0);
        let e = best_org
            .entry(text(&d["template"]).to_string())
            .or_insert(0.0);
        *e = e.max(o);
    }
    let mut standing: HashSet<i64> = HashSet::new();
    // own province -> (army, id) of the fit idle divisions there
    let mut idle: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    for d in &mine {
        let (Some(id), Some(p)) = (id_of(&d["id"]), d["province_id"].as_i64()) else {
            continue;
        };
        standing.insert(p);
        let top = best_org.get(text(&d["template"])).copied().unwrap_or(0.0);
        let fit = d["organisation"].as_f64().unwrap_or(0.0) >= FIT_ORG * top && top > 0.0;
        if fit && d["heading_province"].is_null() && !fighting.contains(&id) {
            idle.entry(p)
                .or_default()
                .push((text(&d["army"]).to_string(), id));
        }
    }
    let mut out: Option<Push> = None;
    for b in list(battles, "list") {
        let (Some(p), Some(hours)) = (b["province"].as_i64(), b["hours"].as_i64()) else {
            continue;
        };
        if text(&b["our_side"]) != "attacker"
            || hours < min_hours
            || text(&b["winning"]) == "attacker"
            || b["width"]["full"] == serde_json::json!(true)
            || skip(p)
        {
            continue;
        }
        let Some(row) = g.get(&p) else { continue };
        let from: HashSet<i64> = rows(&b["attacker"])
            .iter()
            .filter_map(|d| d["province"].as_i64())
            .collect();
        let sides: Vec<i64> = row
            .borders
            .iter()
            .map(|(q, _)| *q)
            .filter(|q| standing.contains(q) || from.contains(q))
            .collect();
        let used = sides.iter().filter(|q| from.contains(q)).count();
        // the unused sides, the ones without a river first
        let mut unused: Vec<(i64, f64)> = row
            .borders
            .iter()
            .filter(|(q, _)| standing.contains(q) && !from.contains(q))
            .copied()
            .collect();
        unused.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut by_army: HashMap<String, Vec<(String, i64)>> = HashMap::new();
        for (q, _) in &unused {
            for (army, id) in idle.get(q).into_iter().flatten() {
                by_army
                    .entry(army.clone())
                    .or_default()
                    .push((id.clone(), *q));
            }
        }
        let Some((army, cands)) = by_army
            .into_iter()
            .max_by(|a, b| a.1.len().cmp(&b.1.len()).then(b.0.cmp(&a.0)))
        else {
            continue;
        };
        // the spare width in divisions of the engaged ones' mean width
        let engaged = list(&b["attacker"], "engaged_divisions");
        let widths: Vec<f64> = engaged.iter().filter_map(|d| d["width"].as_f64()).collect();
        let mean = if widths.is_empty() {
            None
        } else {
            Some(widths.iter().sum::<f64>() / widths.len() as f64)
        };
        let spare = match (
            b["width"]["attacker_max"].as_f64(),
            b["width"]["attacker_engaged"].as_f64(),
            mean,
        ) {
            (Some(max), Some(used_w), Some(w)) if w > 0.0 => ((max - used_w) / w).floor() as usize,
            _ => most,
        };
        let n = cands.len().min(most).min(spare.max(1));
        let pick: Vec<(String, i64)> = cands.into_iter().take(n).collect();
        let mut from_q: Vec<i64> = pick.iter().map(|(_, q)| *q).collect();
        from_q.sort_unstable();
        from_q.dedup();
        let push = Push {
            province: p,
            state: b["state"].as_i64().unwrap_or(0),
            army,
            divisions: pick.into_iter().map(|(id, _)| id).collect(),
            from: from_q,
            hours,
            sides: sides.len(),
            used,
        };
        if out
            .as_ref()
            .is_none_or(|o| (push.hours, push.divisions.len()) > (o.hours, o.divisions.len()))
        {
            out = Some(push);
        }
    }
    out
}

/// SHED TO THE UNATTACKED (pure): the divisions that wait where they cannot fight (the reserve of
/// a battle whose width is full or whose reserve outnumbers its engaged, and the fit idle ones
/// standing on the line) attack an enemy province beside them that no battle of ours attacks, at
/// most what one direction of its width takes (`width` of a division) and `most`: the province
/// with the most of them first (the better ground on a tie), the one army with most of them
/// there. `enemy`: the enemy provinces; `skip`: provinces pushed lately. w101a (28 Nov 2026, W+10):
/// province 3488 held 21 divisions for a width of 7; 4 of the 7 enemy provinces in reach were
/// attacked; the stuck push never fired (every long battle read "winning: attacker" or full).
#[allow(clippy::too_many_arguments)]
pub(super) fn shed(
    battles: &Value,
    g: &HashMap<i64, Prov>,
    read: &Value,
    armies: &HashSet<String>,
    enemy: &HashSet<i64>,
    width: f64,
    most: usize,
    skip: &dyn Fn(i64) -> bool,
) -> Option<Push> {
    let army_of: HashMap<String, String> = list(read, "divisions")
        .iter()
        .filter(|d| armies.contains(text(&d["army"])))
        .filter_map(|d| Some((id_of(&d["id"])?, text(&d["army"]).to_string())))
        .collect();
    let org_of: HashMap<String, f64> = list(read, "divisions")
        .iter()
        .filter_map(|d| Some((id_of(&d["id"])?, d["organisation"].as_f64()?)))
        .collect();
    let mut attacked: HashSet<i64> = HashSet::new();
    let mut fighting: HashSet<String> = HashSet::new();
    // own province -> (army, id, organisation) of the divisions waiting there
    let mut waiting: HashMap<i64, Vec<(String, String, f64)>> = HashMap::new();
    for b in list(battles, "list") {
        for side in ["attacker", "defender"] {
            for d in list(&b[side], "engaged_divisions") {
                fighting.extend(id_of(&d["id"]));
            }
        }
        if text(&b["our_side"]) != "attacker" {
            for d in list(&b["defender"], "reserve_divisions") {
                fighting.extend(id_of(&d["id"]));
            }
            continue;
        }
        if let Some(p) = b["province"].as_i64() {
            attacked.insert(p);
        }
        let side = &b["attacker"];
        let engaged = side["engaged"].as_i64().unwrap_or(0);
        let reserve = side["reserve"].as_i64().unwrap_or(0);
        let stuck = b["width"]["full"] == serde_json::json!(true) || reserve > engaged;
        for d in list(side, "reserve_divisions") {
            let Some(id) = id_of(&d["id"]) else { continue };
            fighting.insert(id.clone());
            let (Some(army), Some(p), true) = (army_of.get(&id), d["province"].as_i64(), stuck)
            else {
                continue;
            };
            let org = org_of.get(&id).copied().unwrap_or(0.0);
            waiting.entry(p).or_default().push((army.clone(), id, org));
        }
    }
    // the fit idle divisions on the line (no battle, no march)
    let mut best_org: HashMap<String, f64> = HashMap::new();
    let mine: Vec<&Value> = list(read, "divisions")
        .iter()
        .filter(|d| armies.contains(text(&d["army"])))
        .collect();
    for d in &mine {
        let o = d["organisation"].as_f64().unwrap_or(0.0);
        let e = best_org
            .entry(text(&d["template"]).to_string())
            .or_insert(0.0);
        *e = e.max(o);
    }
    for d in &mine {
        let (Some(id), Some(p)) = (id_of(&d["id"]), d["province_id"].as_i64()) else {
            continue;
        };
        let top = best_org.get(text(&d["template"])).copied().unwrap_or(0.0);
        let org = d["organisation"].as_f64().unwrap_or(0.0);
        if top > 0.0
            && org >= FIT_ORG * top
            && d["heading_province"].is_null()
            && !fighting.contains(&id)
        {
            waiting
                .entry(p)
                .or_default()
                .push((text(&d["army"]).to_string(), id, org));
        }
    }
    let mut targets: Vec<i64> = enemy
        .iter()
        .copied()
        .filter(|e| !attacked.contains(e) && !skip(*e) && g.contains_key(e))
        .collect();
    targets.sort_unstable();
    let mut out: Option<(usize, f64, Push)> = None;
    for e in targets {
        let row = &g[&e];
        let mut by_army: HashMap<String, Vec<(String, f64, i64)>> = HashMap::new();
        let mut sides = 0;
        for (q, _) in &row.borders {
            if enemy.contains(q) {
                continue;
            }
            let Some(here) = waiting.get(q) else { continue };
            sides += 1;
            for (army, id, org) in here {
                by_army
                    .entry(army.clone())
                    .or_default()
                    .push((id.clone(), *org, *q));
            }
        }
        let Some((army, mut cands)) = by_army
            .into_iter()
            .max_by(|a, b| a.1.len().cmp(&b.1.len()).then(b.0.cmp(&a.0)))
        else {
            continue;
        };
        cands.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let takes = if width > 0.0 && row.width > 0.0 {
            ((row.width / width).floor() as usize).max(1)
        } else {
            most
        };
        let n = cands.len().min(most).min(takes);
        let pick: Vec<(String, f64, i64)> = cands.into_iter().take(n).collect();
        let mut from: Vec<i64> = pick.iter().map(|(_, _, q)| *q).collect();
        from.sort_unstable();
        from.dedup();
        let push = Push {
            province: e,
            state: row.state,
            army,
            divisions: pick.into_iter().map(|(id, _, _)| id).collect(),
            from,
            hours: 0,
            sides,
            used: 0,
        };
        let rank = (push.divisions.len(), row.attack);
        if out
            .as_ref()
            .is_none_or(|(n, a, _)| rank.0 > *n || (rank.0 == *n && rank.1 > *a))
        {
            out = Some((rank.0, rank.1, push));
        }
    }
    out.map(|(_, _, p)| p)
}

/// A move of divisions onto another stretch.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Move {
    pub to: i64,
    pub divisions: Vec<String>,
    pub why: String,
}

/// SIDEWAYS: from the fullest pile, its surplus (reserve beyond the engaged; at most `most`)
/// goes to the stretch with the most free room that is not the pile's own (`root_of` a
/// province), the freshest divisions first. None while no other stretch has room. Pure.
pub(super) fn sideways(
    piles: &[Pile],
    rooms: &[Room],
    root_of: &dyn Fn(i64) -> Option<i64>,
    most: usize,
) -> Option<Move> {
    for pile in piles {
        let from = pile
            .waiting
            .iter()
            .find_map(|(_, _, p)| p.and_then(root_of))
            .or_else(|| root_of(pile.province));
        let Some(target) = rooms
            .iter()
            .filter(|r| Some(r.id) != from && r.free() > 0 && r.facing > 0)
            .max_by(|a, b| a.free().cmp(&b.free()).then(b.id.cmp(&a.id)))
        else {
            continue;
        };
        let surplus = (pile.reserve - pile.engaged).max(0) as usize;
        let n = surplus
            .min(target.free().max(0) as usize)
            .min(pile.waiting.len())
            .min(most);
        if n == 0 {
            continue;
        }
        let mut fresh = pile.waiting.clone();
        fresh.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let dead = from
            .and_then(|f| rooms.iter().find(|r| r.id == f))
            .is_some_and(|r| r.facing == 0);
        let source = if dead {
            format!(
                "front {} faces no enemy province and holds {}",
                from.unwrap_or(0),
                pile.reserve
            )
        } else {
            format!(
                "province {} holds {} in reserve behind {} engaged",
                pile.province, pile.reserve, pile.engaged
            )
        };
        return Some(Move {
            to: target.id,
            divisions: fresh.into_iter().take(n).map(|(id, _, _)| id).collect(),
            why: format!(
                "{source}; front {} has room for {} ({} of {})",
                target.id,
                target.free(),
                target.divisions,
                target.cap
            ),
        });
    }
    None
}

/// The breadth of the attack: (enemy provinces touching ours, those with a battle of ours, those
/// attacked from fewer sides than we border them). `front` = own provinces where our divisions
/// stand. Pure.
pub(super) fn breadth(
    g: &HashMap<i64, Prov>,
    front: &HashSet<i64>,
    enemy: &HashSet<i64>,
    battles: &Value,
) -> (usize, usize, usize) {
    let mut sides: HashMap<i64, usize> = HashMap::new();
    for p in front {
        for (q, _) in g.get(p).map(|r| r.borders.as_slice()).unwrap_or(&[]) {
            if enemy.contains(q) {
                *sides.entry(*q).or_default() += 1;
            }
        }
    }
    let mut attacked = 0;
    let mut narrow = 0;
    for b in list(battles, "list") {
        if text(&b["our_side"]) != "attacker" {
            continue;
        }
        let Some(p) = b["province"].as_i64() else {
            continue;
        };
        let Some(n) = sides.get(&p) else { continue };
        attacked += 1;
        let directions = b["width"]["directions"].as_u64().unwrap_or(1) as usize;
        if directions < *n {
            narrow += 1;
        }
    }
    (sides.len(), attacked, narrow)
}

/// Whether a battle is fought on ground that slows armour: its own read names a terrain or river
/// penalty to the attack, or a fort (`lines`), else the province's terrain attack modifier in the
/// graph is below zero. Pure.
pub(super) fn rough_battle(b: &Value, g: &HashMap<i64, Prov>) -> bool {
    let named = list(b, "lines").iter().any(|l| {
        let line = text(&l["line"]);
        (line == "terrain" || line == "river" || line.starts_with("fort"))
            && l["value"].as_f64().is_some_and(|v| v < 0.0)
    });
    named
        || b["province"]
            .as_i64()
            .and_then(|p| g.get(&p))
            .is_some_and(|r| r.attack < 0.0 || r.forts > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prov(state: i64, width: f64, attack: f64, borders: &[i64]) -> Prov {
        Prov {
            state,
            terrain: if attack < 0.0 { "forest" } else { "plains" }.into(),
            width,
            support: width / 2.0,
            attack,
            movement: 1.0,
            forts: 0,
            borders: borders.iter().map(|b| (*b, 0.0)).collect(),
            coastal: false,
        }
    }

    /// The o96a Low Countries in small: own Rheinland 51 (provinces 1, 2) faces the Belgian
    /// Ardennes forest 980 (province 10, one direction), own Westfalen 57 (3, 4, 5) faces the
    /// Dutch plains of Gelderland 35 (11, 12, 13, each from one or two sides).
    fn theatre() -> HashMap<i64, Prov> {
        HashMap::from([
            (1, prov(51, 70.0, 0.0, &[10, 2])),
            (2, prov(51, 70.0, 0.0, &[1, 3])),
            (3, prov(57, 70.0, 0.0, &[2, 4, 11])),
            (4, prov(57, 70.0, 0.0, &[3, 5, 11, 12])),
            (5, prov(57, 70.0, 0.0, &[4, 12, 13])),
            (10, prov(980, 60.0, -0.15, &[1])),
            (11, prov(35, 70.0, 0.0, &[3, 4])),
            (12, prov(35, 70.0, 0.0, &[4, 5])),
            (13, prov(35, 70.0, 0.0, &[5])),
        ])
    }

    #[test]
    fn a_narrow_unfed_forest_stretch_gets_what_it_can_fight_with_and_feed_and_the_plains_the_rest()
    {
        let g = theatre();
        let enemy: HashSet<i64> = [10, 11, 12, 13].into();
        // o96a W+10: the Ardennes province fed 1.66 against a demand of 21.3 for 26 divisions
        // (0.82 a division); the Dutch side's provinces about 4.5 each
        let read = json!({"provinces": [
            {"province_id": 1, "divisions": 26, "supply": 1.66, "demand": 21.3},
            {"province_id": 3, "divisions": 4, "supply": 4.6, "demand": 3.3},
            {"province_id": 4, "divisions": 4, "supply": 4.5, "demand": 3.3},
            {"province_id": 5, "divisions": 4, "supply": 4.4, "demand": 3.3}]});
        let f = feed(&read);
        assert!((f.per_division.unwrap() - 0.82).abs() < 0.01);
        let stretches = vec![
            Stretch {
                id: 92,
                provinces: vec![1, 2],
                divisions: 26,
            },
            Stretch {
                id: 91,
                provinces: vec![3, 4, 5],
                divisions: 12,
            },
        ];
        let r = rooms(
            &g,
            &stretches,
            &enemy,
            &|_| 20.0,
            &f,
            &HashMap::new(),
            FEED_FLOOR,
            HOLD_RATIO,
        );
        let bel = &r[0];
        let hol = &r[1];
        // a 60-wide forest from one side takes 3 of 20 wide; 1.66 + 1.66 supply feeds 8 at half
        assert_eq!(bel.width, 3);
        assert_eq!(bel.supply, Some(8));
        assert_eq!(bel.cap, 3);
        assert_eq!(bel.facing, 1);
        // three plains provinces, two of them from two sides
        assert!(hol.width >= 10, "{hol:?}");
        assert!(hol.cap > bel.cap * 3, "{hol:?}");
        assert!(unbalanced(&r, 3));
        assert!(describe(&r).contains("92: 26 of 3"));
    }

    #[test]
    fn a_stretch_the_enemy_masses_against_is_never_left_with_one_division() {
        let g = theatre();
        let enemy: HashSet<i64> = [10, 11, 12, 13].into();
        // 18 hostile divisions seen in the forest state (the French in Luxembourg, o96a W+6)
        let seen = HashMap::from([(980, 18.0)]);
        let stretches = vec![
            Stretch {
                id: 92,
                provinces: vec![1, 2],
                divisions: 1,
            },
            Stretch {
                id: 91,
                provinces: vec![3, 4, 5],
                divisions: 40,
            },
        ];
        let r = rooms(
            &g,
            &stretches,
            &enemy,
            &|_| 20.0,
            &Feed::default(),
            &seen,
            FEED_FLOOR,
            HOLD_RATIO,
        );
        assert_eq!(r[0].hold, 18);
        assert_eq!(r[0].cap, 18);
        assert_eq!(r[0].supply, None);
    }

    fn battle(province: i64, engaged: i64, reserve: &[(&str, f64, i64)], directions: u64) -> Value {
        json!({"province": province, "our_side": "attacker",
               "width": {"directions": directions},
               "lines": [{"line": "terrain", "value": -0.15}],
               "attacker": {"engaged": engaged, "reserve": reserve.len(),
                 "reserve_divisions": reserve.iter().map(|(id, org, p)|
                     json!({"id": id, "org": org, "province": p})).collect::<Vec<_>>()}})
    }

    #[test]
    fn the_reserve_of_a_full_forest_battle_goes_sideways_to_the_front_with_room() {
        // o96a W+2.5: 3488 (Ardennes) 9 engaged and 11 waiting, the Dutch front with room
        let reserve: Vec<(String, f64, i64)> = (0..11)
            .map(|i| (format!("d{i}"), 40.0 + i as f64, 1))
            .collect();
        let rows: Vec<(&str, f64, i64)> = reserve
            .iter()
            .map(|(id, o, p)| (id.as_str(), *o, *p))
            .collect();
        let battles = json!({"list": [battle(10, 9, &rows, 1),
            battle(11, 3, &[("x1", 70.0, 3)], 2)]});
        let ours: HashSet<String> = reserve.iter().map(|r| r.0.clone()).collect();
        let p = piles(&battles, &ours);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].province, p[0].engaged, p[0].reserve), (10, 9, 11));
        let rooms = vec![
            Room {
                id: 92,
                cap: 3,
                divisions: 26,
                facing: 1,
                ..Default::default()
            },
            Room {
                id: 91,
                cap: 20,
                divisions: 12,
                facing: 3,
                ..Default::default()
            },
        ];
        let root_of = |p: i64| if p <= 2 { Some(92) } else { Some(91) };
        let m = sideways(&p, &rooms, &root_of, 18).unwrap();
        assert_eq!(m.to, 91);
        // the surplus beyond the engaged is 2: the two freshest go
        assert_eq!(m.divisions, vec!["d10".to_string(), "d9".to_string()]);
        assert!(m.why.contains("11 in reserve behind 9 engaged"));
        // no other front with room: nothing moves
        let full = vec![rooms[0].clone()];
        assert!(sideways(&p, &full, &root_of, 18).is_none());
    }

    #[test]
    fn at_war_the_divisions_of_a_front_facing_no_enemy_go_to_the_front_with_room() {
        // p101a: Army 2's front 169 'width 0, hold 0, 0 enemy provinces' kept 9 divisions all war
        // while its front 14 stood 14 of cap 51
        let read = json!({"divisions": [
            {"id": "a", "province_id": 5, "organisation": 50.0},
            {"id": "b", "province_id": 5, "organisation": 60.0},
            {"id": "c", "province_id": 6, "organisation": 70.0, "heading_province": 10},
            {"id": "d", "province_id": 7, "organisation": 40.0},
            {"id": "x", "province_id": 5, "organisation": 40.0}]});
        let ours: HashSet<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let root_of = |q: i64| match q {
            5 | 6 => Some(169),
            7 => Some(14),
            _ => None,
        };
        let rooms = vec![
            Room {
                id: 169,
                cap: 0,
                divisions: 3,
                facing: 0,
                ..Default::default()
            },
            Room {
                id: 14,
                cap: 51,
                divisions: 14,
                facing: 7,
                ..Default::default()
            },
        ];
        let p = dead_fronts(&read, &rooms, &root_of, &ours);
        assert_eq!(p.len(), 1);
        // the marching division and the one not ours stay; nothing of the live front moves
        assert_eq!((p[0].engaged, p[0].reserve), (0, 2));
        let m = sideways(&p, &rooms, &root_of, 18).unwrap();
        assert_eq!(m.to, 14);
        assert_eq!(m.divisions, vec!["b".to_string(), "a".to_string()]);
        assert!(
            m.why.contains("front 169 faces no enemy province"),
            "{}",
            m.why
        );
        // a front that faces an enemy is no dead front
        let mut live = rooms.clone();
        live[0].facing = 1;
        assert!(dead_fronts(&read, &live, &root_of, &ours).is_empty());
    }

    #[test]
    fn a_province_over_what_its_battles_take_is_a_pile_too() {
        let read = json!({"divisions": [
            {"id": "a", "province_id": 1, "organisation": 50.0},
            {"id": "b", "province_id": 1, "organisation": 60.0},
            {"id": "c", "province_id": 1, "organisation": 70.0, "heading_province": 10},
            {"id": "d", "province_id": 1, "organisation": 40.0},
            {"id": "e", "province_id": 3, "organisation": 40.0},
            {"id": "x", "province_id": 1, "organisation": 40.0}]});
        let ours: HashSet<String> = ["a", "b", "c", "d", "e"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let caps = HashMap::from([(1, 2), (3, 4)]);
        let p = overstacked(&read, &caps, &ours);
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].province, p[0].engaged, p[0].reserve), (1, 2, 4));
        // the marching division stays where it heads
        assert_eq!(p[0].waiting.len(), 3);
        let rooms = vec![
            Room {
                id: 92,
                cap: 2,
                divisions: 4,
                facing: 1,
                ..Default::default()
            },
            Room {
                id: 91,
                cap: 10,
                divisions: 1,
                facing: 2,
                ..Default::default()
            },
        ];
        let m = sideways(
            &p,
            &rooms,
            &|q| if q == 1 { Some(92) } else { Some(91) },
            18,
        )
        .unwrap();
        assert_eq!(m.to, 91);
        assert_eq!(m.divisions, vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn a_stuck_battle_is_attacked_from_the_side_the_plan_leaves_unused() {
        // the enemy province 10 borders our 1 (attacking it), 2 and 3 (idle divisions); the battle
        // has stood 4 days and is not won, with width for two more divisions
        let mut g = HashMap::new();
        g.insert(
            10,
            Prov {
                borders: vec![(1, 0.0), (2, 0.0), (3, -0.3), (11, 0.0)],
                ..Default::default()
            },
        );
        let battles = json!({"list": [
            {"province": 10, "state": 5, "hours": 96, "our_side": "attacker", "winning": "even",
             "width": {"attacker_max": 45.0, "attacker_engaged": 26.0, "full": false},
             "attacker": {"engaged_divisions": [
                 {"id": "e1", "province": 1, "width": 10.0}, {"id": "e2", "province": 1, "width": 10.0}]},
             "defender": {}}]});
        let d = |id: &str, army: &str, p: i64, org: f64| json!({"id": id, "army": army, "template": "Inf", "province_id": p, "organisation": org});
        let read = json!({"divisions": [
            d("e1", "A", 1, 30.0), d("e2", "A", 1, 30.0),
            d("x1", "A", 2, 60.0), d("x2", "A", 2, 58.0), d("x3", "A", 3, 59.0),
            d("tired", "A", 2, 20.0), d("y1", "B", 3, 60.0), d("other", "Z", 2, 60.0)]});
        let armies: HashSet<String> = ["A", "B"].iter().map(|s| s.to_string()).collect();
        let p = stuck_push(&battles, &g, &read, &armies, 72, 6, &|_| false).unwrap();
        assert_eq!((p.province, p.state), (10, 5));
        assert_eq!(p.army, "A");
        // the spare width (45 - 26) / 10 takes one division... at least one; here 1
        assert_eq!(p.divisions.len(), 1);
        assert_eq!(p.divisions[0], "x1");
        assert_eq!((p.sides, p.used), (3, 1));
        // the side without a river first
        assert_eq!(p.from, vec![2]);
        // too young, won, full, or pushed lately: nothing
        assert!(stuck_push(&battles, &g, &read, &armies, 120, 6, &|_| false).is_none());
        assert!(stuck_push(&battles, &g, &read, &armies, 72, 6, &|q| q == 10).is_none());
        let mut won = battles.clone();
        won["list"][0]["winning"] = json!("attacker");
        assert!(stuck_push(&won, &g, &read, &armies, 72, 6, &|_| false).is_none());
    }

    #[test]
    fn a_front_facing_no_enemy_weighs_nothing_in_a_spread() {
        // p101a W+10: Army 2's front 14 faces 7 enemy provinces (cap 51, 14 there), its front 169
        // none (9 there)
        let rooms = [
            Room {
                id: 14,
                cap: 51,
                divisions: 14,
                facing: 7,
                ..Default::default()
            },
            Room {
                id: 169,
                cap: 0,
                divisions: 9,
                facing: 0,
                ..Default::default()
            },
        ];
        assert_eq!(weight(&rooms[1]), 0);
        let mut thin = rooms[0].clone();
        thin.cap = 0;
        assert_eq!(weight(&thin), 1);
    }

    #[test]
    fn breadth_counts_the_provinces_attacked_and_those_attacked_from_too_few_sides() {
        let g = theatre();
        let enemy: HashSet<i64> = [10, 11, 12, 13].into();
        let front: HashSet<i64> = [1, 3, 4, 5].into();
        // 11 and 12 are bordered from two of our provinces; 12 is attacked from one side only
        let battles = json!({"list": [battle(11, 3, &[], 2), battle(12, 2, &[], 1)]});
        assert_eq!(breadth(&g, &front, &enemy, &battles), (4, 2, 1));
        assert!(rough_battle(&battles["list"][0], &g));
        let plain = json!({"province": 11, "lines": []});
        assert!(!rough_battle(&plain, &g));
        let forest = json!({"province": 10, "lines": []});
        assert!(rough_battle(&forest, &g));
    }
}
