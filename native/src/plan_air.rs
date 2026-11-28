//! The air war by situation (plan key `air_situation`): a standing rule that sends the free wings
//! where the fighting is, sized to win the air or stay out of it, instead of fixed regions by date.
//!
//! Every `every_days` (3) game days at war, from hoi4_air (wings with their variant's range, base,
//! coverage of the region they fly over; bases with free room), hoi4_battles (our battles by state:
//! the regions where our divisions fight, weighted by divisions engaged) and the enemy fighters the
//! region window showed (hoi4_air_region_probe, at most `probe_regions` (2) regions a turn, each
//! read kept `probe_days` (7) days; the enemy's air is read only as our own air map shows it):
//!
//! - fighters (wings whose plane flies air superiority): regions by battle weight; a region where the
//!   enemy's fighters are known gets wings until ours are `fighter_ratio` (1.5) times theirs, or none
//!   when the free fighters that reach it cannot make that ratio (stay out: a wing fed alone into a
//!   lost sky dies for nothing; the note names the shortfall); a region with no read gets a share by
//!   battle weight. Fighters left over go to the first region they already won (margin).
//! - close air support (wings whose plane flies CAS, tactical bombers included): only over regions
//!   with our battles where our fighters are not out-sized (the enemy unseen there, or seen and
//!   matched), by battle weight.
//! - reach: a wing is sent only where its range covers `min_coverage` (0.5) of the battles: the
//!   distance from its base to the region's battle states against its range; otherwise it is rebased
//!   to the own base with free room nearest those battles that does (hoi4_air_rebase), and flies on a
//!   later turn.
//! - never a wing over a region with nothing of ours: a free wing over a region without our battles
//!   is moved to the plan's regions, or stands down when none needs it (hoi4_air_hold is not sent; it
//!   flies there until a region needs it, the note says so).
//!
//! Naval strike, port strike, strategic bombing, interception and air supply stay with the plan's
//! `air_plan` entries and `air.war_missions` (the value of a strategic target is not read here).
//! Wings that `air_plan` names, attached wings and carrier wings are never touched. One order per
//! call: a mission batch for one (mission, region), or the rebases to one base.
//!
//! Mission entries (`air_situation.missions[]`, engineer 125): [{"id","mission","region"|"state","wings"|"share",
//! "when"?,"stop"?,"ignore_air"?,"why"}], `mission` one of hoi4_air_mission's missions (logistics_strike,
//! interception, strategic_bombing, port_strike, naval_strike, recon, air_supply, ...). Each turn, before the
//! fighters and CAS above, every entry whose `when` holds and whose `stop` does not claims free wings whose plane flies
//! the mission (by plane type; a refusal by the game teaches the rest: that wing is left out of that mission for
//! `every_days` x 10): `wings` N of them or `share` (0..1) of those free, the ones already flying it there first, then
//! the nearest; a wing short of reach is rebased as above. An entry other than interception or air superiority waits
//! while the enemy fighters our region window last showed there exceed ours there / `fighter_ratio`, unless
//! `ignore_air` (the rules engine fills entries from its opportunity rules: logistics strike, interception,
//! strategic bombing). A stopped entry's wings are free again. A `target` (or `region`) of `front:<army>` (engineer
//! 132) flies over that army's front: the air region holding most of its divisions (hoi4_armies `states`, the army's
//! standing states by division count; an army_layout name hint means the army playing that role), read each turn, so
//! the wings follow the army as it advances.
//!
//! Plan key: `air_situation` {} (all defaults) or {"enabled":true,"fighter_ratio":1.5,"min_coverage":0.5,
//! "every_days":3,"probe_regions":2,"probe_days":7,"stay_out":true,"roles":["fighter","cas"]}; a
//! roleplay goal can switch it off (`enabled:false`) or bias it (`fighter_ratio`, `stay_out:false`).
//! Off unless the plan names it: the live session in LIVE_VALIDATION_QUEUE (air-situation) gates the
//! strategist's default.

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value};

use super::{list, strings, text, GameHour, State, Tools, DAY};
use crate::error::Result;

/// Wings in one mission or rebase call (the air plan's batch).
const BATCH: usize = 12;

/// A free wing as the allocator sees it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Wing {
    pub name: String,
    /// "fighter" or "cas" (the plane's war mission), else None (left alone).
    pub role: Option<&'static str>,
    pub planes: f64,
    pub region: Option<i64>,
    pub base_state: Option<i64>,
    pub range_km: Option<f64>,
}

/// A region with our battles.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Region {
    pub id: i64,
    /// Our divisions engaged in its battles.
    pub weight: f64,
    /// The states of its battles (their centres measure reach).
    pub states: Vec<i64>,
    /// Enemy fighters active there as the region window last showed them; None: not read.
    pub enemy_fighters: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Order {
    Mission {
        wing: String,
        mission: &'static str,
        region: i64,
    },
    Rebase {
        wing: String,
        state: i64,
    },
}

pub(super) struct Config {
    pub fighter_ratio: f64,
    pub min_coverage: f64,
    pub stay_out: bool,
}

/// The geometry the allocator needs: state centre (map pixels), km per pixel, bases with free room.
pub(super) struct Geo<'a> {
    pub centers: &'a HashMap<i32, (f64, f64)>,
    pub km_per_pixel: f64,
    /// (base state, free room)
    pub bases: Vec<(i64, i64)>,
}

impl Geo<'_> {
    fn km(&self, a: i64, b: i64) -> Option<f64> {
        let p = self.centers.get(&(a as i32))?;
        let q = self.centers.get(&(b as i32))?;
        Some(((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt() * self.km_per_pixel)
    }

    /// Share of the region's battle states within `range` km of base state `base`.
    fn reach(&self, base: i64, range: f64, r: &Region) -> Option<f64> {
        let d: Vec<f64> = r.states.iter().filter_map(|s| self.km(base, *s)).collect();
        if d.is_empty() {
            return None;
        }
        Some(d.iter().filter(|x| **x <= range).count() as f64 / d.len() as f64)
    }

    /// The base with room for `planes` nearest the region's battles from which `range` reaches
    /// `need` of them.
    fn forward_base(&self, range: f64, planes: f64, r: &Region, need: f64) -> Option<i64> {
        self.bases
            .iter()
            .filter(|(_, free)| *free as f64 >= planes)
            .filter(|(s, _)| self.reach(*s, range, r).is_some_and(|c| c >= need))
            .filter_map(|(s, _)| {
                let d = r
                    .states
                    .iter()
                    .filter_map(|t| self.km(*s, *t))
                    .fold(f64::MAX, f64::min);
                (d < f64::MAX).then_some((d, *s))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
            .map(|(_, s)| s)
    }
}

/// Orders (missions and rebases) and the notes saying why, from the situation. Pure.
pub(super) fn allocate(
    wings: &[Wing],
    regions: &[Region],
    geo: &mut Geo<'_>,
    cfg: &Config,
) -> (Vec<Order>, Vec<String>) {
    let mut orders = Vec::new();
    let mut notes = Vec::new();
    let mut regions: Vec<&Region> = regions.iter().filter(|r| r.weight > 0.0).collect();
    regions.sort_by(|a, b| b.weight.total_cmp(&a.weight).then(a.id.cmp(&b.id)));
    if regions.is_empty() {
        return (
            orders,
            vec!["no battle of ours: no region to fly over".into()],
        );
    }
    let total_weight: f64 = regions.iter().map(|r| r.weight).sum();
    let mut used: Vec<bool> = vec![false; wings.len()];
    // FIGHTERS: sized to win or stay out
    let fighters: Vec<usize> = (0..wings.len())
        .filter(|i| wings[*i].role == Some("fighter"))
        .collect();
    let all_f: f64 = fighters.iter().map(|i| wings[*i].planes).sum();
    let mut won: Vec<i64> = Vec::new();
    let mut lost: Vec<i64> = Vec::new();
    for r in &regions {
        let can: Vec<usize> = fighters
            .iter()
            .copied()
            .filter(|i| !used[*i])
            .filter(|i| reaches(geo, cfg, &wings[*i], r) != Some(false))
            .collect();
        let have: f64 = can.iter().map(|i| wings[*i].planes).sum();
        let want = match r.enemy_fighters {
            Some(e) => e * cfg.fighter_ratio,
            None => all_f * r.weight / total_weight,
        };
        if let Some(e) = r.enemy_fighters {
            if have < want && cfg.stay_out && e > 0.0 {
                notes.push(format!(
                    "region {}: {e:.0} enemy fighters need {want:.0} of ours ({}x) and {have:.0} reach it: fighters stay out",
                    r.id, cfg.fighter_ratio
                ));
                lost.push(r.id);
                continue;
            }
        }
        // nearest-first is not known without positions: wings already over the region first
        let mut pick = can.clone();
        pick.sort_by_key(|i| (wings[*i].region != Some(r.id), wings[*i].name.clone()));
        let mut got = 0.0;
        for i in pick {
            if got >= want && got > 0.0 {
                break;
            }
            if assign(
                &wings[i],
                r,
                "air_superiority",
                geo,
                cfg,
                &mut orders,
                &mut notes,
            ) {
                got += wings[i].planes;
            }
            used[i] = true;
        }
        if got > 0.0 {
            won.push(r.id);
            notes.push(format!(
                "region {}: {got:.0} fighters (enemy {})",
                r.id,
                r.enemy_fighters
                    .map_or("not read".into(), |e| format!("{e:.0}"))
            ));
        }
    }
    if let Some(first) = won
        .first()
        .and_then(|id| regions.iter().find(|r| r.id == *id))
    {
        let rest: Vec<usize> = fighters.iter().copied().filter(|i| !used[*i]).collect();
        for i in rest {
            if reaches(geo, cfg, &wings[i], first) != Some(false) {
                assign(
                    &wings[i],
                    first,
                    "air_superiority",
                    geo,
                    cfg,
                    &mut orders,
                    &mut notes,
                );
                used[i] = true;
            }
        }
    }
    // CLOSE AIR SUPPORT: battle regions not lost in the air, by battle weight
    let open: Vec<&&Region> = regions.iter().filter(|r| !lost.contains(&r.id)).collect();
    let open_w: f64 = open.iter().map(|r| r.weight).sum();
    let cas: Vec<usize> = (0..wings.len())
        .filter(|i| wings[*i].role == Some("cas"))
        .collect();
    let all_c: f64 = cas.iter().map(|i| wings[*i].planes).sum();
    for r in &open {
        let want = all_c * r.weight / open_w.max(1e-9);
        let mut pick: Vec<usize> = cas.iter().copied().filter(|i| !used[*i]).collect();
        pick.sort_by_key(|i| (wings[*i].region != Some(r.id), wings[*i].name.clone()));
        let mut got = 0.0;
        for i in pick {
            if got >= want && got > 0.0 {
                break;
            }
            if assign(&wings[i], r, "cas", geo, cfg, &mut orders, &mut notes) {
                got += wings[i].planes;
                used[i] = true;
            }
        }
    }
    if !lost.is_empty() {
        notes.push(format!(
            "close air support kept out of {:?}: the enemy holds the air there",
            lost
        ));
    }
    // left over over a region without our battles: said, not held (a later turn may need them)
    for (i, w) in wings.iter().enumerate() {
        if !used[i]
            && w.role.is_some()
            && w.region.is_some_and(|r| !regions.iter().any(|x| x.id == r))
        {
            notes.push(format!(
                "{}: over region {} with no battle of ours and no region needs it now",
                w.name,
                w.region.unwrap_or(0)
            ));
        }
    }
    (orders, notes)
}

/// Whether `w` reaches `min_coverage` of `r`'s battles from its base: Some(false) needs a base
/// nearer, None unknown (no range or no position: sent as asked).
/// hoi4_air_mission's missions (the entry's `mission` must be one of them).
const MISSIONS: [&str; 14] = [
    "air_superiority",
    "cas",
    "interception",
    "strategic_bombing",
    "naval_strike",
    "kamikaze",
    "port_strike",
    "logistics_strike",
    "air_supply",
    "training",
    "naval_mines",
    "naval_minesweeping",
    "recon",
    "naval_patrol",
];

/// Whether a wing's plane is one the game offers the mission: by its war role (plan_run war_mission of the plane
/// type) or, for the planes without one, by the plane type's family (scout, transport). Missions not listed take any
/// plane (the game's refusal teaches the rest).
fn flies_mission(mission: &str, w: &MWing) -> bool {
    let role = |r: &[&str]| w.war_role.is_some_and(|x| r.contains(&x));
    let kind = |k: &str| w.kind.contains(k);
    match mission {
        "air_superiority" | "interception" => role(&["air_superiority"]),
        "cas" | "logistics_strike" => role(&["cas"]),
        "strategic_bombing" => role(&["strategic_bombing", "cas"]),
        "naval_strike" | "port_strike" => role(&["port_strike", "cas"]),
        "recon" => kind("scout"),
        "naval_patrol" => kind("scout") || kind("naval_bomber") || kind("maritime_patrol"),
        "air_supply" => kind("transport"),
        _ => true,
    }
}

/// A free wing as the mission entries see it (any plane type).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MWing {
    pub wing: Wing,
    /// plan_run war_mission of its plane type
    pub war_role: Option<&'static str>,
    /// the missions it flies now (hoi4_air `missions`)
    pub flying: Vec<String>,
    /// its plane type, lower case (hoi4_air `aircraft_type`)
    pub kind: String,
}

/// One active mission entry, resolved for this turn.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MEntry {
    pub id: String,
    pub mission: &'static str,
    pub region: Region,
    pub count: Option<usize>,
    pub share: Option<f64>,
    pub ignore_air: bool,
}

/// The mission entries' claims: (orders, claimed wing names, notes). Entries in plan order; a wing serves one entry.
/// `refused` holds (wing, mission) pairs the game refused lately. Pure.
pub(super) fn claim(
    entries: &[MEntry],
    wings: &[MWing],
    refused: &[(String, String)],
    geo: &mut Geo<'_>,
    cfg: &Config,
) -> (Vec<Order>, Vec<String>, Vec<String>) {
    let (mut orders, mut taken, mut notes) = (Vec::new(), Vec::<String>::new(), Vec::new());
    for e in entries {
        // our fighters over the region against the enemy's last seen there
        let ours: f64 = wings
            .iter()
            .filter(|w| {
                w.wing.region == Some(e.region.id)
                    && w.flying.iter().any(|m| m == "air_superiority")
            })
            .map(|w| w.wing.planes)
            .sum();
        let contested = !e.ignore_air
            && !matches!(e.mission, "interception" | "air_superiority")
            && e.region
                .enemy_fighters
                .is_some_and(|f| f > ours / cfg.fighter_ratio.max(0.01));
        if contested {
            notes.push(format!(
                "{}: {} over region {} waits: enemy fighters {:.0} seen there against our {ours:.0} (ratio {})",
                e.id,
                e.mission,
                e.region.id,
                e.region.enemy_fighters.unwrap_or(0.0),
                cfg.fighter_ratio
            ));
            continue;
        }
        let mut cands: Vec<&MWing> = wings
            .iter()
            .filter(|w| !taken.contains(&w.wing.name))
            .filter(|w| flies_mission(e.mission, w))
            .filter(|w| {
                !refused
                    .iter()
                    .any(|(n, m)| *n == w.wing.name && m == e.mission)
            })
            .collect();
        // the ones already flying it there first, then the nearest base to the region's states
        let dist = |w: &MWing| {
            w.wing
                .base_state
                .and_then(|b| {
                    e.region
                        .states
                        .iter()
                        .filter_map(|s| geo.km(b, *s))
                        .reduce(f64::min)
                })
                .unwrap_or(f64::MAX)
        };
        cands.sort_by(|&a, &b| {
            let there = |w: &MWing| {
                !(w.wing.region == Some(e.region.id) && w.flying.iter().any(|m| m == e.mission))
            };
            there(a).cmp(&there(b)).then(dist(a).total_cmp(&dist(b)))
        });
        let want = match (e.count, e.share) {
            (Some(n), _) => n,
            (None, Some(f)) => (cands.len() as f64 * f.clamp(0.0, 1.0)).ceil() as usize,
            (None, None) => cands.len(),
        };
        if cands.is_empty() {
            notes.push(format!("{}: no free wing flies {}", e.id, e.mission));
            continue;
        }
        let mut mine = Vec::new();
        for w in cands.into_iter().take(want) {
            let flying =
                w.wing.region == Some(e.region.id) && w.flying.iter().any(|m| m == e.mission);
            if flying
                || assign(
                    &w.wing,
                    &e.region,
                    e.mission,
                    geo,
                    cfg,
                    &mut orders,
                    &mut notes,
                )
            {
                mine.push(w.wing.name.clone());
            }
        }
        let got = mine.len();
        taken.extend(mine.iter().cloned());
        // assign() skips the order when the wing holds the region with another mission: give it this one
        for w in wings.iter().filter(|w| mine.contains(&w.wing.name)) {
            let has = w.flying.iter().any(|m| m == e.mission);
            if w.wing.region == Some(e.region.id)
                && !has
                && !orders.iter().any(|o| matches!(o, Order::Mission { wing, .. } | Order::Rebase { wing, .. } if *wing == w.wing.name))
            {
                orders.push(Order::Mission {
                    wing: w.wing.name.clone(),
                    mission: e.mission,
                    region: e.region.id,
                });
            }
        }
        notes.push(format!(
            "{}: {got} of {want} wings on {} over region {}",
            e.id, e.mission, e.region.id
        ));
    }
    (orders, taken, notes)
}

/// The army a `front:<army>` target names (in `target` or `region`).
fn front_army(e: &Value) -> Option<&str> {
    [&e["target"], &e["region"]]
        .into_iter()
        .find_map(|v| v.as_str()?.strip_prefix("front:"))
        .map(str::trim)
        .filter(|a| !a.is_empty())
}

/// The air region over an army's front: the region holding most of its divisions (its standing
/// states by division count, hoi4_armies `states`); ties go to the lower region id.
fn front_region(state: &State, roster: &Value, hint: &str) -> std::result::Result<i64, String> {
    let army = state
        .layout
        .get(hint)
        .filter(|a| !a.is_empty())
        .map_or(hint, String::as_str);
    let Some(row) = list(roster, "armies")
        .iter()
        .find(|a| text(&a["army"]) == army)
    else {
        return Err(format!("front:{hint}: no army {army} in hoi4_armies"));
    };
    let mut by: BTreeMap<i64, f64> = BTreeMap::new();
    if let Some(states) = row["states"].as_object() {
        for (s, n) in states {
            let Some(r) = s
                .parse::<i32>()
                .ok()
                .and_then(|s| state.regions.get(&s).copied())
            else {
                continue;
            };
            *by.entry(r).or_default() += n.as_f64().unwrap_or(1.0).max(0.0);
        }
    }
    by.into_iter()
        .fold(None::<(i64, f64)>, |best, (r, n)| match best {
            Some((_, b)) if b >= n => best,
            _ => Some((r, n)),
        })
        .filter(|(_, n)| *n > 0.0)
        .map(|(r, _)| r)
        .ok_or_else(|| {
            format!("front:{hint}: the regions of army {army}'s states are not known yet")
        })
}

/// The entry's region: `region` (id), the region of `state`, or the region over an army's front
/// (`front:<army>`, from `roster`); None with the reason.
fn entry_region(
    state: &State,
    e: &Value,
    roster: Option<&Value>,
) -> std::result::Result<i64, String> {
    if let Some(army) = front_army(e) {
        return match roster {
            Some(roster) => front_region(state, roster, army),
            None => Err(format!("front:{army}: hoi4_armies unreadable")),
        };
    }
    if let Some(r) = e["region"]
        .as_i64()
        .or_else(|| e["region"].as_str().and_then(|s| s.parse().ok()))
    {
        return Ok(r);
    }
    if let Some(s) = e["state"].as_i64() {
        return state
            .regions
            .get(&(s as i32))
            .copied()
            .ok_or_else(|| format!("the region of state {s} is not known yet"));
    }
    Err("names no region, state or front:<army>".into())
}

fn reaches(geo: &Geo<'_>, cfg: &Config, w: &Wing, r: &Region) -> Option<bool> {
    let (b, range) = (w.base_state?, w.range_km?);
    geo.reach(b, range, r).map(|c| c >= cfg.min_coverage)
}

/// Sends `w` over `r` (no order when it is there already), or to a forward base with room when its
/// range falls short; false when neither can be done.
fn assign(
    w: &Wing,
    r: &Region,
    mission: &'static str,
    geo: &mut Geo<'_>,
    cfg: &Config,
    orders: &mut Vec<Order>,
    notes: &mut Vec<String>,
) -> bool {
    if reaches(geo, cfg, w, r) == Some(false) {
        let Some(range) = w.range_km else {
            return false;
        };
        if let Some(base) = geo.forward_base(range, w.planes, r, cfg.min_coverage) {
            for b in geo.bases.iter_mut().filter(|b| b.0 == base) {
                b.1 -= w.planes as i64;
            }
            orders.push(Order::Rebase {
                wing: w.name.clone(),
                state: base,
            });
            notes.push(format!(
                "{}: range {range:.0} km short of region {}: rebased to state {base}",
                w.name, r.id
            ));
            return true;
        }
        notes.push(format!(
            "{}: range {range:.0} km does not reach region {} and no own base with room does",
            w.name, r.id
        ));
        return false;
    }
    if w.region != Some(r.id) {
        orders.push(Order::Mission {
            wing: w.name.clone(),
            mission,
            region: r.id,
        });
    }
    true
}

fn cfg_of(plan: &Value) -> Option<(Config, GameHour, usize, GameHour)> {
    let v = &plan["air_situation"];
    let on = v == &json!(true) || (v.is_object() && v["enabled"] != json!(false));
    if !on {
        return None;
    }
    let f = |k: &str, d: f64| v[k].as_f64().filter(|x| *x >= 0.0).unwrap_or(d);
    Some((
        Config {
            fighter_ratio: f("fighter_ratio", 1.5),
            min_coverage: f("min_coverage", 0.5).min(1.0),
            stay_out: v["stay_out"] != json!(false),
        },
        (f("every_days", 3.0) as GameHour).max(1) * DAY,
        f("probe_regions", 2.0) as usize,
        (f("probe_days", 7.0) as GameHour).max(1) * DAY,
    ))
}

fn role(kind: &str) -> Option<&'static str> {
    if kind.to_lowercase().starts_with("cv_") {
        return None;
    }
    match super::war_mission(kind) {
        Some("air_superiority") => Some("fighter"),
        Some("cas") => Some("cas"),
        _ => None,
    }
}

/// The lane step (plan_military LANES `air_situation`).
pub(super) fn air_situation_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let Some((cfg, every, probes, probe_days)) = cfg_of(plan) else {
        return Ok(None);
    };
    if state.skipped(now, "airsit", "x") {
        return Ok(None);
    }
    state.skip(now, "airsit", "x", every);
    let roles: Vec<String> = strings(&plan["air_situation"], "roles");
    let Ok(wars) = tools.call("hoi4_wars", &json!({})) else {
        return Ok(None);
    };
    let enemies: Vec<String> = list(&wars["wars"], "enemies")
        .iter()
        .filter_map(|t| t.as_str().or_else(|| t["tag"].as_str()))
        .map(str::to_string)
        .collect();
    if enemies.is_empty() {
        return Ok(None);
    }
    let Ok(air) = tools.call("hoi4_air", &json!({})) else {
        return Ok(None);
    };
    let me = super::plan_operation::player(tools, plan);
    let mut tags = enemies.clone();
    tags.push(me);
    super::plan_operation::learn_centres(state, tools, &tags);
    // the battles by region
    let mut by: BTreeMap<i64, (f64, Vec<i64>)> = BTreeMap::new();
    if let Ok(read) = tools.call("hoi4_battles", &json!({})) {
        for b in list(&read, "list") {
            let Some(s) = b["state"].as_i64() else {
                continue;
            };
            let Some(r) = state.regions.get(&(s as i32)).copied() else {
                continue;
            };
            let side = if text(&b["our_side"]) == "defender" {
                &b["defender"]
            } else {
                &b["attacker"]
            };
            let e = by.entry(r).or_default();
            e.0 += side["engaged"].as_f64().unwrap_or(1.0).max(1.0);
            e.1.push(s);
        }
    }
    let mut regions: Vec<Region> = by
        .into_iter()
        .map(|(id, (weight, states))| Region {
            id,
            weight,
            states,
            enemy_fighters: state
                .military
                .air_seen
                .get(&id)
                .filter(|(_, at)| now.saturating_sub(*at) < probe_days)
                .map(|(e, _)| *e),
        })
        .collect();
    regions.sort_by(|a, b| b.weight.total_cmp(&a.weight));
    // the enemy's fighters as our region window shows them, for the heaviest regions unread
    let mut probed = 0;
    for r in regions.iter_mut() {
        if probed >= probes || r.enemy_fighters.is_some() {
            continue;
        }
        probed += 1;
        if let Ok(seen) = tools.call(
            "hoi4_air_region_probe",
            &json!({"region": r.id.to_string(), "lease_token": lease}),
        ) {
            if let Some(e) = seen["enemy_active"]["fighters"].as_f64() {
                r.enemy_fighters = Some(e);
                state.military.air_seen.insert(r.id, (e, now));
            }
        }
    }
    // engineer 125: the mission entries claim their wings first (the fighters and CAS below take the rest)
    let mut entries = Vec::new();
    let mut mnotes = Vec::new();
    // the armies' standing states, read once a turn when an entry flies over an army's front
    let roster = list(&plan["air_situation"], "missions")
        .iter()
        .any(|e| front_army(e).is_some())
        .then(|| tools.call("hoi4_armies", &json!({})).ok())
        .flatten();
    for (k, e) in list(&plan["air_situation"], "missions").iter().enumerate() {
        let id = e["id"]
            .as_str()
            .map_or_else(|| format!("missions[{k}]"), str::to_string);
        let Some(mission) = MISSIONS.iter().copied().find(|m| *m == text(&e["mission"])) else {
            mnotes.push(format!(
                "{id}: mission {} is not one of hoi4_air_mission's",
                e["mission"]
            ));
            continue;
        };
        if !e["when"].is_null()
            && super::judge(state, tools, &e["when"], now) != crate::plan_predicate::Truth::True
        {
            continue;
        }
        if !e["stop"].is_null()
            && super::judge(state, tools, &e["stop"], now) == crate::plan_predicate::Truth::True
        {
            continue;
        }
        let rid = match entry_region(state, e, roster.as_ref()) {
            Ok(r) => r,
            Err(why) => {
                mnotes.push(format!("{id}: {why}"));
                continue;
            }
        };
        let states: Vec<i64> = state
            .regions
            .iter()
            .filter(|(_, r)| **r == rid)
            .map(|(s, _)| i64::from(*s))
            .collect();
        entries.push(MEntry {
            id,
            mission,
            region: Region {
                id: rid,
                weight: 0.0,
                states,
                enemy_fighters: state
                    .military
                    .air_seen
                    .get(&rid)
                    .filter(|(_, at)| now.saturating_sub(*at) < probe_days)
                    .map(|(e, _)| *e),
            },
            count: e["wings"].as_u64().map(|n| n as usize),
            share: e["share"].as_f64(),
            ignore_air: e["ignore_air"] == json!(true),
        });
    }
    let all_free: Vec<MWing> = list(&air, "wings")
        .iter()
        .filter(|w| w["attached_army"].as_str().is_none_or(str::is_empty))
        .filter(|w| !super::plan_manage::air_plan_claims(plan, w))
        .filter(|w| w["transfer_to_base_id"].as_i64().is_none())
        .filter(|w| !state.skipped(now, "airsitw", text(&w["wing"])))
        .map(|w| MWing {
            wing: Wing {
                name: text(&w["wing"]).to_string(),
                role: None,
                planes: w["aircraft"].as_f64().unwrap_or(0.0),
                region: w["region"]["id"].as_i64(),
                base_state: w["air_base"]["state_id"].as_i64(),
                range_km: w["air_range_km"].as_f64(),
            },
            war_role: super::war_mission(text(&w["aircraft_type"])),
            flying: strings(w, "missions"),
            kind: text(&w["aircraft_type"]).to_lowercase(),
        })
        .filter(|w| !w.wing.name.is_empty())
        .collect();
    let refused: Vec<(String, String)> = state
        .military
        .air_refused
        .iter()
        .filter(|(_, at)| now.saturating_sub(**at) < 10 * every)
        .map(|(k, _)| k.clone())
        .collect();
    let mut mgeo = Geo {
        centers: &state.centers,
        km_per_pixel: air["km_per_pixel"].as_f64().unwrap_or(7.114),
        bases: list(&air, "air_bases")
            .iter()
            .filter_map(|b| Some((b["state_id"].as_i64()?, b["free"].as_i64().unwrap_or(0))))
            .collect(),
    };
    let (morders, claimed, cnotes) = claim(&entries, &all_free, &refused, &mut mgeo, &cfg);
    mnotes.extend(cnotes);
    let wings: Vec<Wing> = list(&air, "wings")
        .iter()
        .filter(|w| !claimed.iter().any(|c| c == text(&w["wing"])))
        .filter(|w| w["attached_army"].as_str().is_none_or(str::is_empty))
        .filter(|w| !super::plan_manage::air_plan_claims(plan, w))
        .filter(|w| w["transfer_to_base_id"].as_i64().is_none())
        .filter(|w| !state.skipped(now, "airsitw", text(&w["wing"])))
        .map(|w| Wing {
            name: text(&w["wing"]).to_string(),
            role: role(text(&w["aircraft_type"]))
                .filter(|r| roles.is_empty() || roles.iter().any(|x| x == r)),
            planes: w["aircraft"].as_f64().unwrap_or(0.0),
            region: w["region"]["id"].as_i64(),
            base_state: w["air_base"]["state_id"].as_i64(),
            range_km: w["air_range_km"].as_f64(),
        })
        .filter(|w| !w.name.is_empty())
        .collect();
    let mut geo = Geo {
        centers: &state.centers,
        km_per_pixel: air["km_per_pixel"].as_f64().unwrap_or(7.114),
        bases: list(&air, "air_bases")
            .iter()
            .filter_map(|b| Some((b["state_id"].as_i64()?, b["free"].as_i64().unwrap_or(0))))
            .collect(),
    };
    let (aorders, anotes) = allocate(&wings, &regions, &mut geo, &cfg);
    let mut orders = morders;
    orders.extend(aorders);
    let mut notes = mnotes;
    notes.extend(anotes);
    super::plan_manage::note(state, "air_situation", notes.join("; "));
    // one call: the rebases to the first base named, else the largest mission batch
    if let Some(Order::Rebase { state: base, .. }) =
        orders.iter().find(|o| matches!(o, Order::Rebase { .. }))
    {
        let base = *base;
        let names: Vec<String> = orders
            .iter()
            .filter_map(|o| match o {
                Order::Rebase { wing, state } if *state == base => Some(wing.clone()),
                _ => None,
            })
            .take(BATCH)
            .collect();
        let args = if names.len() == 1 {
            json!({"wing": names[0], "state_id": base})
        } else {
            json!({"wings": names, "state_id": base})
        };
        let (ok, why) = state.act(tools, lease, "hoi4_air_rebase", args, "as")?;
        for n in &names {
            state.skip(now, "airsitw", n, if ok { every } else { 3 * every });
        }
        return Ok(Some(format!(
            "air situation: {} rebased to state {base}: {ok} {why}",
            names.join(", ")
        )));
    }
    let mut groups: BTreeMap<(&'static str, i64), Vec<String>> = BTreeMap::new();
    for o in &orders {
        if let Order::Mission {
            wing,
            mission,
            region,
        } = o
        {
            groups
                .entry((mission, *region))
                .or_default()
                .push(wing.clone());
        }
    }
    let Some(((mission, region), mut names)) = groups.into_iter().max_by_key(|(_, v)| v.len())
    else {
        return Ok(None);
    };
    names.truncate(BATCH);
    let args = if names.len() == 1 {
        json!({"wing": names[0], "mission": mission, "region": region.to_string()})
    } else {
        json!({"wings": names, "mission": mission, "region": region.to_string()})
    };
    let (ok, why) = state.act(tools, lease, "hoi4_air_mission", args, "as")?;
    for n in &names {
        state.skip(now, "airsitw", n, if ok { every } else { 3 * every });
        // the game does not offer this mission to the wing's plane: leave it out of this mission a while
        if !ok && why.contains("invalid_target") {
            state
                .military
                .air_refused
                .insert((n.clone(), mission.to_string()), now);
        }
    }
    // more orders wait: come back on the next tick rather than in `every` days
    if orders.len() > names.len() {
        state.skip(now, "airsit", "x", 1);
    }
    Ok(Some(format!(
        "air situation: {} {mission} over region {region}: {ok} {why}",
        names.join(", ")
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wing(name: &str, role: &'static str, planes: f64, base: i64, range: f64) -> Wing {
        Wing {
            name: name.into(),
            role: Some(role),
            planes,
            region: None,
            base_state: Some(base),
            range_km: Some(range),
        }
    }

    fn centers() -> HashMap<i32, (f64, f64)> {
        // states 1..3 bases at x 0, 50, 100; battle states 10 at x 100, 20 at x 300 (1 km per pixel)
        [
            (1, (0.0, 0.0)),
            (2, (50.0, 0.0)),
            (3, (100.0, 0.0)),
            (10, (100.0, 0.0)),
            (20, (300.0, 0.0)),
        ]
        .into_iter()
        .collect()
    }

    fn cfg() -> Config {
        Config {
            fighter_ratio: 1.5,
            min_coverage: 0.5,
            stay_out: true,
        }
    }

    fn region(id: i64, weight: f64, state: i64, enemy: Option<f64>) -> Region {
        Region {
            id,
            weight,
            states: vec![state],
            enemy_fighters: enemy,
        }
    }

    fn mwing(
        name: &str,
        role: &'static str,
        base: i64,
        region: Option<i64>,
        flying: &[&str],
    ) -> MWing {
        MWing {
            wing: Wing {
                name: name.into(),
                role: None,
                planes: 50.0,
                region,
                base_state: Some(base),
                range_km: Some(500.0),
            },
            war_role: Some(role),
            flying: flying.iter().map(|m| m.to_string()).collect(),
            kind: String::new(),
        }
    }

    fn mentry(
        id: &str,
        mission: &'static str,
        region: Region,
        count: Option<usize>,
        share: Option<f64>,
    ) -> MEntry {
        MEntry {
            id: id.into(),
            mission,
            region,
            count,
            share,
            ignore_air: false,
        }
    }

    #[test]
    fn mission_entries_claim_wings_of_the_planes_that_fly_them_before_the_fighters_and_cas() {
        let c = centers();
        let mut geo = Geo {
            centers: &c,
            km_per_pixel: 1.0,
            bases: vec![],
        };
        let wings = vec![
            mwing("CAS far", "cas", 1, None, &[]),
            mwing("CAS near", "cas", 3, None, &[]),
            mwing("CAS there", "cas", 2, Some(7), &["logistics_strike"]),
            mwing("F1", "air_superiority", 1, None, &[]),
            mwing("F2", "air_superiority", 2, None, &[]),
        ];
        let entries = vec![
            mentry(
                "ls",
                "logistics_strike",
                region(7, 0.0, 10, None),
                Some(2),
                None,
            ),
            mentry(
                "int",
                "interception",
                region(7, 0.0, 10, Some(500.0)),
                None,
                Some(0.5),
            ),
        ];
        let (orders, taken, notes) = claim(&entries, &wings, &[], &mut geo, &cfg());
        // logistics strike: the wing already on it there, then the nearest; interception: half the fighters, despite
        // the enemy fighters (interception is never held back by them)
        assert_eq!(taken, vec!["CAS there", "CAS near", "F2"]);
        assert!(orders.contains(&Order::Mission {
            wing: "CAS near".into(),
            mission: "logistics_strike",
            region: 7
        }));
        assert!(!orders
            .iter()
            .any(|o| matches!(o, Order::Mission { wing, .. } if wing == "CAS there")));
        assert!(notes.iter().any(|n| n.contains("ls: 2 of 2")), "{notes:?}");
        // a strike over a sky the enemy holds waits; a refused wing is left out
        let held = vec![mentry(
            "ls",
            "logistics_strike",
            region(7, 0.0, 10, Some(400.0)),
            Some(1),
            None,
        )];
        let (o, t, n) = claim(&held, &wings, &[], &mut geo, &cfg());
        assert!(o.is_empty() && t.is_empty());
        assert!(n[0].contains("waits"), "{n:?}");
        let refused = vec![("CAS near".to_string(), "logistics_strike".to_string())];
        let one = vec![mentry(
            "ls",
            "logistics_strike",
            region(7, 0.0, 10, None),
            Some(2),
            None,
        )];
        let (_, t, _) = claim(&one, &wings, &refused, &mut geo, &cfg());
        assert_eq!(t, vec!["CAS there", "CAS far"]);
        // recon takes the scout plane only
        let mut scout = mwing("Aufkl 1", "cas", 1, None, &[]);
        scout.war_role = None;
        scout.kind = "scout_plane_equipment_1".into();
        let mut all = wings.clone();
        all.push(scout);
        let rec = vec![mentry("rc", "recon", region(7, 0.0, 10, None), None, None)];
        let (_, t, _) = claim(&rec, &all, &[], &mut geo, &cfg());
        assert_eq!(t, vec!["Aufkl 1"]);
    }

    #[test]
    fn fighters_are_sized_to_win_or_stay_out() {
        let c = centers();
        let mut geo = Geo {
            centers: &c,
            km_per_pixel: 1.0,
            bases: vec![],
        };
        let wings = vec![
            wing("F1", "fighter", 100.0, 1, 500.0),
            wing("F2", "fighter", 100.0, 1, 500.0),
        ];
        // region 5: 300 enemy fighters need 450; we have 200: stay out. Region 6 unseen: all go there
        let regions = vec![region(5, 10.0, 10, Some(300.0)), region(6, 5.0, 20, None)];
        let (orders, notes) = allocate(&wings, &regions, &mut geo, &cfg());
        assert!(notes.iter().any(|n| n.contains("stay out")), "{notes:?}");
        assert!(orders
            .iter()
            .all(|o| matches!(o, Order::Mission { region: 6, .. })));
        assert_eq!(orders.len(), 2);
        // the same sky with 100 enemy fighters: 150 wanted, both wings go to region 5
        let regions = vec![region(5, 10.0, 10, Some(100.0)), region(6, 5.0, 20, None)];
        let (orders, _) = allocate(&wings, &regions, &mut geo, &cfg());
        assert!(
            orders.iter().all(|o| matches!(
                o,
                Order::Mission {
                    region: 5,
                    mission: "air_superiority",
                    ..
                }
            )),
            "{orders:?}"
        );
    }

    #[test]
    fn cas_flies_only_where_the_air_is_not_lost_and_short_range_rebases() {
        let c = centers();
        let mut geo = Geo {
            centers: &c,
            km_per_pixel: 1.0,
            bases: vec![(3, 200)],
        };
        let wings = vec![
            wing("F1", "fighter", 50.0, 1, 500.0),
            wing("S1", "cas", 50.0, 1, 50.0), // 100 km from region 6's battle at state 10: short
            wing("S2", "cas", 50.0, 2, 500.0),
        ];
        let regions = vec![region(5, 10.0, 20, Some(300.0)), region(6, 5.0, 10, None)];
        let (orders, notes) = allocate(&wings, &regions, &mut geo, &cfg());
        assert!(!orders.iter().any(|o| matches!(
            o,
            Order::Mission {
                region: 5,
                mission: "cas",
                ..
            }
        )));
        assert!(orders.contains(&Order::Rebase {
            wing: "S1".into(),
            state: 3
        }));
        assert!(orders.contains(&Order::Mission {
            wing: "S2".into(),
            mission: "cas",
            region: 6
        }));
        assert!(notes.iter().any(|n| n.contains("kept out")), "{notes:?}");
    }

    #[test]
    fn no_battle_no_orders_and_wings_already_there_stay() {
        let c = centers();
        let mut geo = Geo {
            centers: &c,
            km_per_pixel: 1.0,
            bases: vec![],
        };
        let mut w = wing("F1", "fighter", 50.0, 1, 500.0);
        w.region = Some(6);
        let (orders, notes) = allocate(&[w.clone()], &[], &mut geo, &cfg());
        assert!(orders.is_empty());
        assert!(notes[0].contains("no battle"));
        let (orders, _) = allocate(&[w], &[region(6, 3.0, 10, None)], &mut geo, &cfg());
        assert!(orders.is_empty(), "already over the region: no order");
    }

    #[test]
    fn a_front_target_flies_over_the_region_holding_most_of_the_armys_divisions() {
        let mut state = State::default();
        for (s, r) in [(64, 7), (65, 7), (66, 9), (10, 12)] {
            state.regions.insert(s, r);
        }
        state.layout.insert("north".into(), "1. Armee".into());
        let roster = json!({"armies": [
            {"army": "1. Armee", "states": {"64": 3, "65": 4, "66": 6}},
            {"army": "4. Armee", "states": {"10": 2, "999": 9}}]});
        // 7 divisions in region 7 against 6 in region 9; a layout hint names the role's army
        let e = json!({"mission": "cas", "target": "front:north"});
        assert_eq!(entry_region(&state, &e, Some(&roster)), Ok(7));
        let e = json!({"mission": "cas", "region": "front:4. Armee"});
        assert_eq!(entry_region(&state, &e, Some(&roster)), Ok(12));
        let e = json!({"mission": "cas", "target": "front:Ghost"});
        assert!(entry_region(&state, &e, Some(&roster))
            .unwrap_err()
            .contains("no army Ghost"));
        assert!(entry_region(&state, &e, None).is_err());
        // a plain region id still wins without a roster
        assert_eq!(entry_region(&state, &json!({"region": "5"}), None), Ok(5));
    }

    #[test]
    fn config_is_off_unless_named() {
        assert!(cfg_of(&json!({})).is_none());
        assert!(cfg_of(&json!({"air_situation": {"enabled": false}})).is_none());
        let (c, every, probes, _) =
            cfg_of(&json!({"air_situation": {"fighter_ratio": 2.0}})).unwrap();
        assert_eq!(c.fighter_ratio, 2.0);
        assert_eq!(every, 3 * DAY);
        assert_eq!(probes, 2);
        assert!(cfg_of(&json!({"air_situation": true})).is_some());
    }
}
