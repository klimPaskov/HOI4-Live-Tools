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
//! Plan key: `air_situation` true or {"enabled":true,"fighter_ratio":1.5,"min_coverage":0.5,
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
pub(super) fn step(
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
    let wings: Vec<Wing> = list(&air, "wings")
        .iter()
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
    let (orders, notes) = allocate(&wings, &regions, &mut geo, &cfg);
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
