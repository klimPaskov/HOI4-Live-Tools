//! Airborne (plan key `airborne`): a standing opportunity rule for the paratroopers.
//!
//! {"army":"<the paratrooper army>","from_states":[air base states]?,"targets":[{"province","state","why"?,
//!  "when"?}],"min_air_share":0.7,"link_up_km":300,"cooldown_days":14,"min_transports":50,
//!  "transports_per_division":45,"enabled"?}
//!
//! At war, once a day, while the transport planes carry the army (at least min_transports and transports_per_division
//! a division: the game's MIN_PLANE_COUNT_PARADROP and BASE_UNIT_WEIGHT_IN_TRANSPORT_PLANES, which the strategist
//! copies from the defines of the game and mod in use): the first target whose `when` holds, which a transport wing reaches from one of the own air
//! bases (the transport variant's air range against the distance between the base state's centre and the target
//! state's, at hoi4_air km_per_pixel), whose region the enemy does not hold in the air (the region window's estimate
//! of the enemy share, hoi4_air_region_probe, at most 1 - min_air_share; an unread region waits; one window a turn,
//! each read kept a week), gets hoi4_paradrop with the army; then the rule rests `cooldown_days`. FAIR (the plan's
//! mode): the target must lie within `link_up_km` of one of our battles, a drop the front can reach (MX12 bans capital
//! snipes without a supportable operation, MX43 the retreat trick); META lifts it. The game's own checks
//! (paratroopers, transports, PARADROP_AIR_SUPERIORITY_RATIO) still decide. The strategist writes the targets
//! (victory points, ports, air fields) from the game files for any country; a roleplay goal switches the rule off
//! (`enabled: false`). Drops are off by default in FAIR (the owner's rule of 28 Nov) and on in META; in FAIR a player's
//! request (`roleplay_request: true`) turns them on. hoi4_paradrop itself is off in auto until S21-PARADROP passes; the rule then waits on its
//! refusal (paradrop_unvalidated) with the reason in the digest.

use std::collections::HashMap;

use serde_json::{json, Value};

use super::{list, text, GameHour, State, Tools, DAY};
use crate::error::Result;

/// A drop candidate.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Target {
    pub province: i64,
    pub state: i64,
    pub region: Option<i64>,
}

fn km(centers: &HashMap<i32, (f64, f64)>, scale: f64, a: i64, b: i64) -> Option<f64> {
    let p = centers.get(&(a as i32))?;
    let q = centers.get(&(b as i32))?;
    Some(((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt() * scale)
}

/// What a drop is judged by.
pub(super) struct Sky<'a> {
    pub centers: &'a HashMap<i32, (f64, f64)>,
    pub km_per_pixel: f64,
    pub transport_km: f64,
    /// Enemy share of the air per region (the region window's estimate).
    pub enemy_share: &'a HashMap<i64, f64>,
    pub battle_states: &'a [i64],
    pub min_air_share: f64,
    /// FAIR: the most a target may lie from our nearest battle; None in META.
    pub link_up_km: Option<f64>,
}

/// The first target that passes, with the departure state; and the reasons the others did not.
pub(super) fn pick_drop(
    targets: &[Target],
    bases: &[i64],
    sky: &Sky<'_>,
) -> (Option<(Target, i64)>, Vec<String>) {
    let mut why = Vec::new();
    for t in targets {
        let Some((d, base)) = bases
            .iter()
            .filter_map(|b| km(sky.centers, sky.km_per_pixel, *b, t.state).map(|d| (d, *b)))
            .min_by(|a, b| a.0.total_cmp(&b.0))
        else {
            why.push(format!(
                "province {}: no base or target position",
                t.province
            ));
            continue;
        };
        if d > sky.transport_km {
            why.push(format!(
                "province {}: {d:.0} km from state {base}, transports reach {:.0}",
                t.province, sky.transport_km
            ));
            continue;
        }
        match t.region.and_then(|r| sky.enemy_share.get(&r)) {
            None => {
                why.push(format!(
                    "province {}: the air over its region is not read",
                    t.province
                ));
                continue;
            }
            Some(e) if 1.0 - e < sky.min_air_share => {
                why.push(format!(
                    "province {}: our air share {:.0}% below {:.0}%",
                    t.province,
                    (1.0 - e) * 100.0,
                    sky.min_air_share * 100.0
                ));
                continue;
            }
            _ => {}
        }
        if let Some(link) = sky.link_up_km {
            let near = sky
                .battle_states
                .iter()
                .filter_map(|s| km(sky.centers, sky.km_per_pixel, *s, t.state))
                .fold(f64::MAX, f64::min);
            if near > link {
                why.push(format!(
                    "province {}: {near:.0} km from our nearest battle (FAIR: a drop the front can reach, {link:.0} km)",
                    t.province
                ));
                continue;
            }
        }
        return (Some((t.clone(), base)), why);
    }
    (None, why)
}

/// Whether the rule may drop: the plan's rule sets decide (fair_guard `paradrops`: off by default in FAIR, on in META,
/// on when `_rules.roleplay.paradrops` is true and the multiplayer rules do not forbid it); `roleplay_request` on the
/// rule itself also turns drops on in FAIR.
pub(super) fn drops_allowed(run: &crate::fair_guard::Run, cfg: &Value) -> bool {
    run.allows("paradrops") || cfg["roleplay_request"].as_bool() == Some(true)
}

/// Transport planes a drop of `divisions` needs: the game's minimum, else the weight of each division.
pub(super) fn transports_needed(divisions: u64, min: u64, per_division: u64) -> u64 {
    (divisions * per_division).max(min)
}

/// The lane step (plan_military LANES `airborne`).
pub(super) fn airborne_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["airborne"];
    if !cfg.is_object() || cfg["enabled"] == json!(false) || state.skipped(now, "airborne", "x") {
        return Ok(None);
    }
    // the owner's rule (28 Nov): drops are off by default in FAIR, on in META, and on in FAIR when the player asks for
    // them (a roleplay request: `roleplay_request`)
    if !drops_allowed(&crate::fair_guard::Run::from_plan(plan), cfg) {
        return Ok(None);
    }
    let army = text(&cfg["army"]).to_string();
    if army.is_empty() {
        return Ok(None);
    }
    state.skip(now, "airborne", "x", DAY);
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
    let transports: Vec<&Value> = list(&air, "wings")
        .iter()
        .filter(|w| {
            text(&w["aircraft_type"])
                .to_lowercase()
                .contains("transport")
        })
        .collect();
    let transport_km = transports
        .iter()
        .filter_map(|w| w["air_range_km"].as_f64())
        .fold(0.0, f64::max);
    if transport_km <= 0.0 {
        super::plan_manage::note(
            state,
            "airborne",
            "no transport wing with a range read: no drop",
        );
        return Ok(None);
    }
    let planes: u64 = transports
        .iter()
        .filter_map(|w| w["aircraft"].as_u64())
        .sum();
    let divisions = tools
        .call("hoi4_armies", &json!({}))
        .ok()
        .and_then(|v| {
            list(&v, "armies")
                .iter()
                .find(|a| text(&a["army"]) == army)
                .and_then(|a| a["division_count"].as_u64())
        })
        .unwrap_or(0);
    let need = transports_needed(
        divisions,
        cfg["min_transports"].as_u64().unwrap_or(50),
        cfg["transports_per_division"].as_u64().unwrap_or(45),
    );
    if divisions == 0 || planes < need {
        super::plan_manage::note(
            state,
            "airborne",
            format!(
                "{army}: {divisions} divisions need {need} transport planes, {planes} in wings: no drop"
            ),
        );
        return Ok(None);
    }
    let me = super::plan_operation::player(tools, plan);
    let mut tags = enemies.clone();
    tags.push(me);
    super::plan_operation::learn_centres(state, tools, &tags);
    let wanted: Vec<i64> = super::ints(&cfg["from_states"]);
    let bases: Vec<i64> = list(&air, "air_bases")
        .iter()
        .filter_map(|b| b["state_id"].as_i64())
        .filter(|s| wanted.is_empty() || wanted.contains(s))
        .collect();
    let mut targets = Vec::new();
    for t in list(cfg, "targets") {
        let (Some(p), Some(s)) = (t["province"].as_i64(), t["state"].as_i64()) else {
            continue;
        };
        if state.done.contains_key(&format!("airborne:{p}")) {
            continue;
        }
        if !t["when"].is_null()
            && super::judge(state, tools, &t["when"], now) != crate::plan_predicate::Truth::True
        {
            continue;
        }
        targets.push(Target {
            province: p,
            state: s,
            region: state.regions.get(&(s as i32)).copied(),
        });
    }
    if targets.is_empty() {
        return Ok(None);
    }
    let battle_states: Vec<i64> = tools
        .call("hoi4_battles", &json!({}))
        .map(|read| {
            list(&read, "list")
                .iter()
                .filter_map(|b| b["state"].as_i64())
                .collect()
        })
        .unwrap_or_default();
    // the region window's estimate for the targets' regions: one window a turn, each read kept a week
    let mut share: HashMap<i64, f64> = HashMap::new();
    let mut probed = false;
    for t in &targets {
        let Some(r) = t.region else { continue };
        if share.contains_key(&r) {
            continue;
        }
        if let Some((e, at)) = state.military.air_share.get(&r) {
            if now.saturating_sub(*at) < 7 * DAY {
                share.insert(r, *e);
                continue;
            }
        }
        if probed {
            continue;
        }
        probed = true;
        if let Ok(seen) = tools.call(
            "hoi4_air_region_probe",
            &json!({"region": r.to_string(), "lease_token": lease}),
        ) {
            let e = seen["enemy_air_superiority_estimate"].as_f64().or_else(|| {
                (seen["enemy_active"]["fighters"].as_f64() == Some(0.0)).then_some(0.0)
            });
            if let Some(e) = e {
                share.insert(r, e);
                state.military.air_share.insert(r, (e, now));
            }
        }
    }
    let fair = crate::fair_guard::Run::from_plan(plan).fair;
    let sky = Sky {
        centers: &state.centers,
        km_per_pixel: air["km_per_pixel"].as_f64().unwrap_or(7.114),
        transport_km,
        enemy_share: &share,
        battle_states: &battle_states,
        min_air_share: cfg["min_air_share"].as_f64().unwrap_or(0.7).clamp(0.0, 1.0),
        link_up_km: fair.then(|| cfg["link_up_km"].as_f64().unwrap_or(300.0)),
    };
    let (pick, why) = pick_drop(&targets, &bases, &sky);
    super::plan_manage::note(state, "airborne", why.join("; "));
    let Some((t, base)) = pick else {
        return Ok(None);
    };
    let (ok, reply) = state.act(
        tools,
        lease,
        "hoi4_paradrop",
        json!({"army": army, "from_state": base, "target_province": t.province}),
        "pd",
    )?;
    let cool = cfg["cooldown_days"].as_u64().unwrap_or(14).max(1) as GameHour * DAY;
    state.skip(now, "airborne", "x", if ok { cool } else { 3 * DAY });
    if ok {
        state.done.insert(format!("airborne:{}", t.province), now);
    }
    Ok(Some(format!(
        "airborne: {army} from state {base} onto province {}: {ok} {reply}",
        t.province
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_needs_reach_air_and_in_fair_a_front_to_link_up() {
        // states 1 (base) at x 0, 3 at x 100, 10 at x 100, 20 at x 300 (1 km per pixel)
        let c: HashMap<i32, (f64, f64)> = [
            (1, (0.0, 0.0)),
            (3, (100.0, 0.0)),
            (10, (100.0, 0.0)),
            (20, (300.0, 0.0)),
        ]
        .into_iter()
        .collect();
        let near = Target {
            province: 900,
            state: 3,
            region: Some(6),
        };
        let far = Target {
            province: 901,
            state: 20,
            region: Some(7),
        };
        let mut share = HashMap::new();
        share.insert(6, 0.2);
        share.insert(7, 0.1);
        let sky = |share: &HashMap<i64, f64>,
                   battles: &'static [i64],
                   link: Option<f64>|
         -> (Option<(Target, i64)>, Vec<String>) {
            let s = Sky {
                centers: &c,
                km_per_pixel: 1.0,
                transport_km: 150.0,
                enemy_share: share,
                battle_states: battles,
                min_air_share: 0.7,
                link_up_km: link,
            };
            pick_drop(&[far.clone(), near.clone()], &[1], &s)
        };
        // transports reach 150 km: the far target (300 km) is out of reach, the near one drops
        let (pick, why) = sky(&share, &[10], Some(50.0));
        assert_eq!(pick, Some((near.clone(), 1)));
        assert!(why[0].contains("transports reach"));
        // the enemy holds 40% over region 6: our 60% is below 70%
        share.insert(6, 0.4);
        assert!(sky(&share, &[10], None).0.is_none());
        // FAIR: no battle within 50 km of the target: no drop; META (None) drops
        share.insert(6, 0.1);
        let (pick, why) = sky(&share, &[20], Some(50.0));
        assert!(pick.is_none() && why.iter().any(|w| w.contains("FAIR")));
        assert!(sky(&share, &[20], None).0.is_some());
        // unread air waits
        let (pick, why) = sky(&HashMap::new(), &[10], None);
        assert!(pick.is_none() && why.iter().any(|w| w.contains("not read")));
    }

    #[test]
    fn drops_are_off_in_fair_unless_the_player_asks() {
        use crate::fair_guard::Run;
        let meta = Run::from_plan(&json!({"_meta": {"mode": "meta"}}));
        assert!(drops_allowed(&meta, &json!({})));
        assert!(!drops_allowed(&Run::default_fair(), &json!({})));
        assert!(drops_allowed(
            &Run::default_fair(),
            &json!({"roleplay_request": true})
        ));
        let rp = Run::from_plan(&json!({"_rules": {"roleplay": {"paradrops": true}}}));
        assert!(drops_allowed(&rp, &json!({})));
    }

    #[test]
    fn transports_cover_the_minimum_and_each_division() {
        assert_eq!(transports_needed(1, 50, 45), 50);
        assert_eq!(transports_needed(2, 50, 45), 90);
        assert_eq!(transports_needed(3, 50, 45), 135);
    }

    #[test]
    fn off_unless_named() {
        assert!(!json!({}).get("airborne").is_some_and(Value::is_object));
    }
}
