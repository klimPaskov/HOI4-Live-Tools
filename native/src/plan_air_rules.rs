//! Air opportunity rules (MASTER-PLAN 3.2: trigger, check, stop) that fill `air_situation.missions[]`
//! in the shape engineer 122 decided: {id, mission, region, share, when, stop, why, source}.
//!
//! Re-derived every turn from what the executor already reads; an entry that is no longer produced
//! is its stop (the missions executor releases the wings of an `"source": "opportunity"` entry the
//! turn it disappears). Nothing here is a bare-number rule: sizes are shares of the planes left
//! free after the situation's own fighters and close air support (`Spare`), split by what was seen.
//!
//! - INTERCEPTION. Trigger: enemy bombers seen (our air map) over a region holding our industry.
//!   Check: fighters left free after superiority over the battles; the share follows the bombers
//!   seen per region. Stop: no enemy bombers seen there.
//! - LOGISTICS STRIKE. Trigger: an operation of ours runs past a bound it has not reached. Check:
//!   close air support left free after the battles; never where the enemy's fighters out-size ours
//!   by the situation's own ratio (the executor's wait, `ignore_air` false). Stop: the bound is
//!   reached.
//! - STRATEGIC BOMBING for war support (FAIR allows it, owner 28 Nov; a rule set may switch it off).
//!   Trigger: an enemy whose war support (as the game shows it to us) is above its surrender floor.
//!   Check: bombers left free and our fighters hold that sky by the situation's ratio. Stop: its war
//!   support at or below the floor.
//!
//! Wiring: pure today; the missions executor (plan_air.rs, air engineer) calls `opportunities` with
//! its turn's reads and appends the entries to the plan's own `missions[]`.

use serde_json::{json, Value};

/// A region as our air map shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Sight {
    pub region: i64,
    /// Enemy fighters seen there; None: not read.
    pub enemy_fighters: Option<f64>,
    /// Enemy bombers (strategic or tactical on bombing missions) seen there; None: not read.
    pub enemy_bombers: Option<f64>,
    /// The region holds own industry (factories in own states).
    pub own_industry: bool,
    /// Our fighters active there now.
    pub our_fighters: f64,
}

/// An operation of ours with a bound (an objective line) in a region.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Push {
    pub id: String,
    pub region: i64,
    pub bound_reached: bool,
}

/// An enemy whose war support we read (as the game shows it to us).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Target {
    pub tag: String,
    /// A region over its war industry within our bombers' reach.
    pub region: i64,
    pub war_support: Option<f64>,
    /// The war support below which it surrenders or its laws fall (read from the game).
    pub floor: Option<f64>,
}

/// Planes free after the situation's fighters and close air support this turn.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Spare {
    pub fighters: f64,
    pub cas: f64,
    pub bombers: f64,
}

pub(super) struct Rules {
    /// The situation's own fighter ratio (air_situation.fighter_ratio).
    pub fighter_ratio: f64,
    /// Strategic bombing for war support allowed by the rule set (`_rules`).
    pub bombing_allowed: bool,
}

/// The opportunity entries for this turn and the notes saying why one was not made.
pub(super) fn opportunities(
    rules: &Rules,
    sights: &[Sight],
    pushes: &[Push],
    targets: &[Target],
    spare: Spare,
) -> (Vec<Value>, Vec<String>) {
    let mut out = Vec::new();
    let mut notes = Vec::new();
    // INTERCEPTION
    let bombed: Vec<(&Sight, f64)> = sights
        .iter()
        .filter(|s| s.own_industry)
        .filter_map(|s| s.enemy_bombers.filter(|b| *b > 0.0).map(|b| (s, b)))
        .collect();
    let all_bombers: f64 = bombed.iter().map(|(_, b)| b).sum();
    if !bombed.is_empty() && spare.fighters <= 0.0 {
        notes.push(format!(
            "interception: {all_bombers:.0} enemy bombers over our industry, no fighter left free after the battles"
        ));
    }
    if spare.fighters > 0.0 {
        for (s, b) in &bombed {
            out.push(json!({"id": format!("opp_intercept_{}", s.region), "mission": "interception",
                "region": s.region, "share": round3(b / all_bombers), "when": "war",
                "stop": "no enemy bombers seen over the region", "source": "opportunity",
                "why": format!("{b:.0} enemy bombers seen over our industry in region {}", s.region)}));
        }
    }
    // LOGISTICS STRIKE
    let open: Vec<&Push> = pushes.iter().filter(|p| !p.bound_reached).collect();
    if !open.is_empty() && spare.cas <= 0.0 {
        notes.push("logistics strike: no close air support left free after the battles".into());
    }
    if spare.cas > 0.0 {
        let n = open.len() as f64;
        for p in &open {
            let sky = sights.iter().find(|s| s.region == p.region);
            if let Some(why) = sky.and_then(|s| sky_lost(s, rules.fighter_ratio)) {
                notes.push(format!("logistics strike {}: {why}", p.id));
                continue;
            }
            out.push(
                json!({"id": format!("opp_logistics_{}", p.id), "mission": "logistics_strike",
                "region": p.region, "share": round3(1.0 / n), "when": "war",
                "stop": format!("operation {} reaches its bound", p.id), "source": "opportunity",
                "why": format!("operation {} runs past its bound in region {}", p.id, p.region)}),
            );
        }
    }
    // STRATEGIC BOMBING for war support
    if rules.bombing_allowed && spare.bombers > 0.0 {
        let live: Vec<&Target> = targets
            .iter()
            .filter(|t| matches!((t.war_support, t.floor), (Some(w), Some(f)) if w > f))
            .collect();
        for t in targets
            .iter()
            .filter(|t| t.war_support.is_none() || t.floor.is_none())
        {
            notes.push(format!(
                "strategic bombing {}: war support or its floor unread",
                t.tag
            ));
        }
        let n = live.len() as f64;
        for t in &live {
            let sky = sights.iter().find(|s| s.region == t.region);
            let held = match sky {
                Some(s) => match s.enemy_fighters {
                    Some(e) => s.our_fighters >= e * rules.fighter_ratio,
                    None => false,
                },
                None => false,
            };
            if !held {
                notes.push(format!(
                    "strategic bombing {}: our fighters do not hold region {} by {}x (or it is unread)",
                    t.tag, t.region, rules.fighter_ratio
                ));
                continue;
            }
            out.push(
                json!({"id": format!("opp_bomb_{}", t.tag), "mission": "strategic_bombing",
                "region": t.region, "share": round3(1.0 / n), "when": "war",
                "stop": format!("{} war support at or below {:.2}", t.tag, t.floor.unwrap_or(0.0)),
                "source": "opportunity",
                "why": format!("{} war support {:.2} above its floor {:.2}", t.tag,
                    t.war_support.unwrap_or(0.0), t.floor.unwrap_or(0.0))}),
            );
        }
    }
    (out, notes)
}

/// Why the sky over a region is the enemy's (its fighters out-size ours by the ratio), or None.
fn sky_lost(s: &Sight, ratio: f64) -> Option<String> {
    let e = s.enemy_fighters?;
    (e > 0.0 && e * ratio > s.our_fighters).then(|| {
        format!(
            "{e:.0} enemy fighters against our {:.0} in region {}: the strike waits for the sky",
            s.our_fighters, s.region
        )
    })
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules {
            fighter_ratio: 1.5,
            bombing_allowed: true,
        }
    }

    #[test]
    fn interception_follows_the_bombers_seen_over_our_industry() {
        let sights = vec![
            Sight {
                region: 1,
                enemy_bombers: Some(30.0),
                own_industry: true,
                ..Default::default()
            },
            Sight {
                region: 2,
                enemy_bombers: Some(10.0),
                own_industry: true,
                ..Default::default()
            },
            Sight {
                region: 3,
                enemy_bombers: Some(50.0),
                own_industry: false,
                ..Default::default()
            },
        ];
        let spare = Spare {
            fighters: 200.0,
            ..Default::default()
        };
        let (out, _) = opportunities(&rules(), &sights, &[], &[], spare);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["mission"], "interception");
        assert_eq!(out[0]["share"], 0.75);
        assert_eq!(out[1]["share"], 0.25);
        assert_eq!(out[0]["source"], "opportunity");
        // no fighter free: no entry, a note
        let (out, notes) = opportunities(&rules(), &sights, &[], &[], Spare::default());
        assert!(out.is_empty());
        assert!(notes[0].contains("no fighter left free"));
    }

    #[test]
    fn a_logistics_strike_runs_until_the_bound_and_waits_for_a_lost_sky() {
        let pushes = vec![
            Push {
                id: "op_a".into(),
                region: 5,
                bound_reached: false,
            },
            Push {
                id: "op_b".into(),
                region: 6,
                bound_reached: false,
            },
            Push {
                id: "op_c".into(),
                region: 7,
                bound_reached: true,
            },
        ];
        let sights = vec![Sight {
            region: 6,
            enemy_fighters: Some(100.0),
            our_fighters: 120.0,
            ..Default::default()
        }];
        let spare = Spare {
            cas: 90.0,
            ..Default::default()
        };
        let (out, notes) = opportunities(&rules(), &sights, &pushes, &[], spare);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["id"], "opp_logistics_op_a");
        assert_eq!(out[0]["share"], 0.5);
        assert!(notes
            .iter()
            .any(|n| n.contains("op_b") && n.contains("waits for the sky")));
    }

    #[test]
    fn bombing_for_war_support_needs_the_rule_the_floor_and_a_held_sky() {
        let targets = vec![
            Target {
                tag: "ENG".into(),
                region: 9,
                war_support: Some(0.7),
                floor: Some(0.3),
            },
            Target {
                tag: "FRA".into(),
                region: 8,
                war_support: Some(0.2),
                floor: Some(0.3),
            },
            Target {
                tag: "SOV".into(),
                region: 4,
                war_support: None,
                floor: Some(0.3),
            },
        ];
        let sights = vec![Sight {
            region: 9,
            enemy_fighters: Some(40.0),
            our_fighters: 80.0,
            ..Default::default()
        }];
        let spare = Spare {
            bombers: 50.0,
            ..Default::default()
        };
        let (out, notes) = opportunities(&rules(), &sights, &[], &targets, spare);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["mission"], "strategic_bombing");
        assert_eq!(out[0]["region"], 9);
        assert!(notes
            .iter()
            .any(|n| n.contains("SOV") && n.contains("unread")));
        // the rule set switches it off
        let off = Rules {
            bombing_allowed: false,
            ..rules()
        };
        assert!(opportunities(&off, &sights, &[], &targets, spare)
            .0
            .is_empty());
        // an unread sky is not held
        let (out, _) = opportunities(&rules(), &[], &[], &targets, spare);
        assert!(out.is_empty());
    }
}
