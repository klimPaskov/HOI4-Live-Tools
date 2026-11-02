//! `hoi4_intelligence`: the own intelligence agency and what can be read of
//! espionage. Pure read: no input, no lease, no cursor movement, never pauses.
//!
//! What was established on 26 October 2026 (Germany, January 1938), and what
//! was not:
//!
//! * CHECKED live: `country+0xd68` is the `CIntelligenceAgency` (owner
//!   `+0x10` is the country) and its agency name is the libc++ string at
//!   `+0x80` ("Abwehr" for Germany, "MI6" United Kingdom, "NKVD" Soviet Union,
//!   as the install's `intelligence_agencies` script defines them). Every
//!   country has such an object and a default name, so the name does not say
//!   whether the agency has been created.
//! * CHECKED live: the loaded upgrade database (`image+0x3704b10`,
//!   `CAgencyUpgradeDatabase`, branch array `+0x70`, upgrades in each branch's
//!   array `+0x30`) names 5 branches and 21 upgrades; the install's
//!   `intelligence_agency_upgrades` script supplies levels and modifiers.
//! * NOT FOUND: where the agency keeps created / upgrade levels / progress,
//!   the operative roster (no operative is linked from any country's
//!   character arrays on this build state), and the civilian / army / navy /
//!   air intelligence network values towards other countries
//!   (`country+0xd90` `CCountryIntel` holds undecoded per-country records).
//!   These are reported unavailable and never guessed.
//!
//! The mission list comes from the game's own mission classes
//! (`NOperativeMissions::*`), the operation list from the install's
//! `operations` scripts.
use super::{
    mgmt,
    script::{self, Block, Entry, Val},
    ui::read,
};
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::{path::Path, time::Instant};

const AGENCY: u64 = 0xd68;
const AGENCY_TYPE: &str = "19CIntelligenceAgency";
const AGENCY_OWNER: u64 = 0x10;
const AGENCY_NAME: u64 = 0x80;
const UPGRADE_DB: u64 = 0x3704b10;
const UPGRADE_DB_TYPE: &str = "22CAgencyUpgradeDatabase";
const DB_BRANCHES: u64 = 0x70;
const BRANCH_TYPE: &str = "20CAgencyUpgradeBranch";
const BRANCH_KEY: u64 = 0x10;
const BRANCH_UPGRADES: u64 = 0x30;
const UPGRADE_TYPE: &str = "14CAgencyUpgrade";
const UPGRADE_KEY: u64 = 0x10;
const NULL_BRANCH_TYPE: &str = "11TNullObjectI20CAgencyUpgradeBranchS0_E";
const LIMIT: usize = 64;

/// Mission classes of the game binary (`NOperativeMissions::*`), as keys.
pub(super) const MISSIONS: &[&str] = &[
    "boost_ideology",
    "build_intel_network",
    "control_trade",
    "counter_intelligence",
    "diplomatic_pressure",
    "propaganda",
    "quiet_intel_network",
    "root_out_resistance",
];

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Live {
    pub agency_name: Option<String>,
    pub branches: Vec<(String, Vec<String>)>,
}

fn branch_name(r: &Reader<'_>, branch: u64) -> Result<String> {
    Ok(mgmt::text(r, branch + BRANCH_KEY, 64)?.unwrap_or_default())
}

pub(super) fn read_live(r: &Reader<'_>, image: u64, country: u64) -> Result<Live> {
    let agency = mgmt::pointer(r, country + AGENCY)?;
    mgmt::expect_type(r, agency, AGENCY_TYPE)?;
    if r.u64(agency + AGENCY_OWNER)? != country {
        return Err(Error::new(
            "invalid_owner",
            "Own intelligence agency country join required",
        ));
    }
    let agency_name = mgmt::text(r, agency + AGENCY_NAME, 96)?.filter(|n| !n.is_empty());
    let db = mgmt::pointer(r, image + UPGRADE_DB)?;
    mgmt::expect_type(r, db, UPGRADE_DB_TYPE)?;
    let items = r.pointer_array(db + DB_BRANCHES)?;
    if items.len() > LIMIT {
        return Err(Error::new("invalid_array", "Implausible agency branches"));
    }
    let mut branches = Vec::new();
    for item in items {
        if !mgmt::is_pointer(item) {
            return Err(Error::new("invalid_pointer", "Stale agency branch"));
        }
        // Index 0 is the database's null object.
        if mgmt::type_name(r, item)? == NULL_BRANCH_TYPE {
            continue;
        }
        mgmt::expect_type(r, item, BRANCH_TYPE)?;
        let upgrades = r.pointer_array(item + BRANCH_UPGRADES)?;
        if upgrades.len() > LIMIT {
            return Err(Error::new("invalid_array", "Implausible branch upgrades"));
        }
        let mut keys = Vec::new();
        for upgrade in upgrades {
            if !mgmt::is_pointer(upgrade) {
                return Err(Error::new("invalid_pointer", "Stale agency upgrade"));
            }
            mgmt::expect_type(r, upgrade, UPGRADE_TYPE)?;
            keys.push(mgmt::text(r, upgrade + UPGRADE_KEY, 96)?.unwrap_or_default());
        }
        branches.push((branch_name(r, item)?, keys));
    }
    Ok(Live {
        agency_name,
        branches,
    })
}

fn snapshot(r: &Reader<'_>, image: u64) -> Result<Live> {
    let own = mgmt::own(r, image)?;
    read_live(r, image, own.country)
}

// ------------------------------------------------------------- catalogue

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct UpgradeDef {
    pub key: String,
    pub levels: usize,
    pub factories_during_progress: Option<f64>,
    pub requires: Vec<String>,
    pub level_modifiers: Vec<Vec<(String, String)>>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct OperationDef {
    pub key: String,
    pub operatives: Option<f64>,
    pub network_strength: Option<f64>,
    pub days: Option<f64>,
    pub risk_chance: Option<f64>,
    pub experience: Option<f64>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Catalog {
    pub upgrades: Vec<(String, UpgradeDef)>,
    pub operations: Vec<OperationDef>,
    pub defines: Vec<(&'static str, f64)>,
}

pub(super) fn upgrades_from(entries: &[Entry]) -> Vec<(String, UpgradeDef)> {
    let mut out = Vec::new();
    for branch in entries {
        let Val::Block(upgrades) = &branch.val else {
            continue;
        };
        for u in upgrades {
            let Val::Block(body) = &u.val else {
                continue;
            };
            let levels: Vec<Vec<(String, String)>> = body
                .all("level")
                .map(|l| {
                    l.block()
                        .find_val("modifier")
                        .map(|m| {
                            m.block()
                                .iter()
                                .filter_map(|e| match &e.val {
                                    Val::Scalar(v) => Some((e.key.clone(), v.clone())),
                                    Val::Block(_) => None,
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                })
                .collect();
            let requires = body
                .find_val("available")
                .map(|a| {
                    a.block()
                        .iter()
                        .filter(|e| e.key == "has_done_agency_upgrade")
                        .filter_map(|e| e.val.scalar().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            out.push((
                branch.key.clone(),
                UpgradeDef {
                    key: u.key.clone(),
                    levels: levels.len(),
                    factories_during_progress: body
                        .find_val("modifiers_during_progress")
                        .and_then(|m| m.block().number("civilian_factory_use")),
                    requires,
                    level_modifiers: levels,
                },
            ));
        }
    }
    out
}

pub(super) fn operations_from(entries: &[Entry]) -> Vec<OperationDef> {
    entries
        .iter()
        .filter(|e| e.key.starts_with("operation_"))
        .filter_map(|e| match &e.val {
            Val::Block(b) => Some(OperationDef {
                key: e.key.clone(),
                operatives: b.number("operatives"),
                network_strength: b.number("network_strength"),
                days: b.number("days"),
                risk_chance: b.number("risk_chance"),
                experience: b.number("experience"),
            }),
            Val::Scalar(_) => None,
        })
        .collect()
}

fn define(text: &str, name: &str) -> Option<f64> {
    text.lines()
        .map(str::trim_start)
        .find(|l| l.starts_with(name) && l[name.len()..].trim_start().starts_with('='))
        .and_then(|l| l.split('=').nth(1))
        .and_then(|v| {
            v.trim()
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .next()
                .and_then(|n| n.parse().ok())
        })
}

fn load(dir: &Path) -> Catalog {
    let common = dir.join("common");
    let defines =
        std::fs::read_to_string(common.join("defines/00_defines.lua")).unwrap_or_default();
    Catalog {
        upgrades: upgrades_from(&script::parse_dir(
            &common.join("intelligence_agency_upgrades"),
        )),
        operations: operations_from(&script::parse_dir(&common.join("operations"))),
        defines: [
            "AGENCY_CREATION_DAYS",
            "AGENCY_UPGRADE_DAYS",
            "AGENCY_CREATION_FACTORIES",
            "AGENCY_UPGRADE_PER_OPERATIVE_SLOT",
            "MAX_OPERATIVE_SLOT_FROM_AGENCY_UPGRADES",
        ]
        .into_iter()
        .filter_map(|n| define(&defines, n).map(|v| (n, v)))
        .collect(),
    }
}

pub(super) fn catalog(dir: &Path) -> std::sync::Arc<Catalog> {
    script::cached(dir, "intelligence", load)
}

// -------------------------------------------------------------- assembly

pub(super) fn assemble(live: &Live, catalog: &Catalog, include_operations: bool) -> Value {
    let branches: Vec<Value> =
        live.branches
            .iter()
            .map(|(branch, keys)| {
                let upgrades: Vec<Value> = keys
                .iter()
                .map(|key| {
                    let def = catalog.upgrades.iter().find(|(_, d)| d.key == *key).map(|(_, d)| d);
                    json!({
                        "key": key,
                        "levels": def.map(|d| d.levels),
                        "factories_during_progress": def.and_then(|d| d.factories_during_progress),
                        "requires_upgrades": def.map(|d| d.requires.clone()),
                        "level_modifiers": def.map(|d| d.level_modifiers.iter().map(|l| {
                            l.iter().map(|(k, v)| json!({k: v})).collect::<Vec<_>>()
                        }).collect::<Vec<_>>()),
                        "level_read": false,
                    })
                })
                .collect();
                json!({"branch": branch, "upgrades": upgrades})
            })
            .collect();
    let total: usize = live.branches.iter().map(|(_, k)| k.len()).sum();
    let operations: Value = if include_operations {
        json!(catalog
            .operations
            .iter()
            .map(|o| json!({
                "key": o.key,
                "operatives": o.operatives,
                "network_strength": o.network_strength,
                "days": o.days,
                "risk_chance": o.risk_chance,
                "experience": o.experience,
            }))
            .collect::<Vec<_>>())
    } else {
        Value::Null
    };
    let defines: serde_json::Map<String, Value> = catalog
        .defines
        .iter()
        .map(|(k, v)| ((*k).to_string(), json!(v)))
        .collect();
    json!({
        "agency": {
            "name": live.agency_name,
            "name_validated": true,
            "created": null,
            "created_read": false,
            "note": "every country has an agency object with a default name; whether it has been created is not mapped",
        },
        "upgrades": {
            "branch_count": live.branches.len(),
            "upgrade_count": total,
            "branches": branches,
            "levels_and_progress_read": false,
            "validated": "branch and upgrade keys read from the loaded database; levels from scripts",
        },
        "operatives": {
            "state_read": false,
            "reason": "no operative roster is linked from the country on this build state (name, nationality, skill, traits, location, status and operation have no mapped source)",
            "missions": MISSIONS,
            "missions_source": "NOperativeMissions classes of the game binary",
        },
        "intel_network": {
            "state_read": false,
            "reason": "civilian, army, navy and air network values towards other countries are in undecoded per-country records",
        },
        "operations": {"catalog_count": catalog.operations.len(), "entries": operations, "include_with": "include_operations:true"},
        "defines": defines,
    })
}

pub(super) fn intelligence(process: &dyn Process, include_operations: bool) -> Result<Value> {
    let started = Instant::now();
    let dir = mgmt::game_dir(process).ok();
    let cat = dir
        .as_deref()
        .map(catalog)
        .unwrap_or_else(|| std::sync::Arc::new(Catalog::default()));
    let (live, context, stamp, build_id) = read(process, |r, image| {
        let live = snapshot(r, image)?;
        Ok((
            live,
            mgmt::clock(r, image)?,
            mgmt::stamp(started, r),
            game::layout(r).build_id,
        ))
    })?;
    let mut body = assemble(&live, &cat, include_operations);
    let object = body.as_object_mut().expect("object");
    object.insert("source".into(), json!("native own intelligence read"));
    object.insert("build_id".into(), json!(build_id));
    object.insert("owned".into(), json!(true));
    object.insert("game_time".into(), context);
    object.insert("freshness".into(), stamp);
    object.insert("catalog_loaded".into(), json!(!cat.upgrades.is_empty()));
    object.insert("observation_paused_game".into(), json!(false));
    object.insert("input_authorized".into(), json!(false));
    Ok(body)
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_intelligence","description":"Own intelligence agency, read-only (no input, no lease, never pauses). Returns the agency name (validated live: Abwehr for Germany), the 5 upgrade branches with their upgrades from the loaded database joined with the install's scripts (levels, factory use during progress, prerequisite upgrades, per-level modifiers), the operative mission keys, the agency defines (creation/upgrade days and factories) and, with include_operations:true, the operations catalogue (operatives, network strength, days, risk). Whether the agency is created, upgrade levels and progress, the operative roster and the intel network values are NOT mapped on this build state and are reported as state_read:false rather than guessed.","inputSchema":{"type":"object","properties":{"include_operations":{"type":"boolean"}},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::mgmt::fake::Mem;
    use super::*;

    const UPGRADES: &str = r#"
    branch_intelligence = {
        upgrade_economy_civilian = {
            modifiers_during_progress = { civilian_factory_use = 5 }
            level = { modifier = { civilian_intel_factor = 0.25 } }
        }
        upgrade_army_department = {
            available = { has_done_agency_upgrade = upgrade_economy_civilian }
            modifiers_during_progress = { civilian_factory_use = 15 }
            level = { modifier = { army_intel_factor = 0.25 } }
            level = { modifier = { army_intel_factor = 0.5 intel_network_strength = 1 } }
        }
    }
    branch_defense = { upgrade_passive_defense = { level = { modifier = { intelligence_agency_defense = 1.5 } } } }
    "#;
    const OPERATIONS: &str = r#"
    operation_rescue_operative = { days = 35 network_strength = 30 operatives = 1 risk_chance = 0.1 experience = 2 }
    not_an_operation = { days = 1 }
    "#;

    #[test]
    fn upgrade_and_operation_scripts_are_summarised() {
        let ups = upgrades_from(&script::parse(UPGRADES));
        assert_eq!(ups.len(), 3);
        let army = &ups
            .iter()
            .find(|(_, d)| d.key == "upgrade_army_department")
            .unwrap()
            .1;
        assert_eq!(army.levels, 2);
        assert_eq!(army.factories_during_progress, Some(15.0));
        assert_eq!(army.requires, ["upgrade_economy_civilian"]);
        assert_eq!(army.level_modifiers[1].len(), 2);
        let ops = operations_from(&script::parse(OPERATIONS));
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].network_strength, Some(30.0));
    }

    #[test]
    fn defines_are_read_from_lua_lines() {
        let lua = "\tAGENCY_CREATION_DAYS = 30,\t\t-- days\n AGENCY_UPGRADE_DAYS=45,\nOTHER = 1,\n";
        assert_eq!(define(lua, "AGENCY_CREATION_DAYS"), Some(30.0));
        assert_eq!(define(lua, "AGENCY_UPGRADE_DAYS"), Some(45.0));
        assert_eq!(define(lua, "MISSING"), None);
    }

    fn object(m: &Mem, at: u64, class: &str, slot: u64) {
        let vt = 0x41000 + slot * 0x100;
        m.q(at, vt);
        m.q(vt - 8, vt + 0x40);
        m.q(vt + 0x40 + 8, vt + 0x80);
        m.patch(vt + 0x80, format!("{class}\0").as_bytes());
    }
    fn array(m: &Mem, at: u64, data: u64, items: &[u64]) {
        m.q(at, data);
        m.i(at + 8, items.len() as i32 + 1);
        m.i(at + 12, items.len() as i32);
        for (i, v) in items.iter().enumerate() {
            m.q(data + 8 * i as u64, *v);
        }
    }
    const IMAGE: u64 = 0x1_0000_0000;

    fn build() -> Mem {
        let m = Mem::default();
        m.alloc(0x40000, 0x5000);
        m.alloc(0x100000, 0x1000); // country
        m.alloc(0x110000, 0x200); // agency
        m.alloc(IMAGE + UPGRADE_DB, 0x10);
        m.alloc(0x120000, 0x100); // db
        m.alloc(0x130000, 0x40); // branch pointer storage
        m.alloc(0x140000, 0x100); // null branch
        m.alloc(0x150000, 0x100); // branch 1
        m.alloc(0x160000, 0x100); // branch 2
        m.alloc(0x170000, 0x40); // upgrade pointer storage
        m.alloc(0x180000, 0x100);
        m.alloc(0x181000, 0x100);
        m.alloc(0x182000, 0x100);
        m.q(0x100000 + AGENCY, 0x110000);
        object(&m, 0x110000, AGENCY_TYPE, 0);
        m.q(0x110000 + AGENCY_OWNER, 0x100000);
        m.sso(0x110000 + AGENCY_NAME, "Abwehr");
        m.q(IMAGE + UPGRADE_DB, 0x120000);
        object(&m, 0x120000, UPGRADE_DB_TYPE, 1);
        array(
            &m,
            0x120000 + DB_BRANCHES,
            0x130000,
            &[0x140000, 0x150000, 0x160000],
        );
        object(&m, 0x140000, NULL_BRANCH_TYPE, 2);
        object(&m, 0x150000, BRANCH_TYPE, 3);
        m.sso(0x150000 + BRANCH_KEY, "branch_intelligence");
        array(
            &m,
            0x150000 + BRANCH_UPGRADES,
            0x170000,
            &[0x180000, 0x181000],
        );
        object(&m, 0x160000, BRANCH_TYPE, 3);
        m.sso(0x160000 + BRANCH_KEY, "branch_defense");
        array(&m, 0x160000 + BRANCH_UPGRADES, 0x170020, &[0x182000]);
        for (at, key) in [
            (0x180000u64, "upgrade_economy_civilian"),
            (0x181000, "upgrade_army_department"),
            (0x182000, "upgrade_passive_defense"),
        ] {
            object(&m, at, UPGRADE_TYPE, 4);
            m.sso(at + UPGRADE_KEY, key);
        }
        m
    }

    #[test]
    fn live_read_names_agency_branches_and_upgrades() {
        let m = build();
        let r = Reader::new(&m);
        let live = read_live(&r, IMAGE, 0x100000).unwrap();
        assert_eq!(live.agency_name.as_deref(), Some("Abwehr"));
        assert_eq!(live.branches.len(), 2);
        assert_eq!(live.branches[0].0, "branch_intelligence");
        assert_eq!(live.branches[0].1.len(), 2);
        assert_eq!(live.branches[1].1, ["upgrade_passive_defense"]);
    }

    #[test]
    fn live_read_rejects_foreign_agency_and_wrong_types() {
        let m = build();
        m.q(0x110000 + AGENCY_OWNER, 0x999000);
        let r = Reader::new(&m);
        assert_eq!(
            read_live(&r, IMAGE, 0x100000).unwrap_err().code,
            "invalid_owner"
        );
        let m = build();
        object(&m, 0x181000, "11CTechnology", 5);
        let r = Reader::new(&m);
        assert_eq!(
            read_live(&r, IMAGE, 0x100000).unwrap_err().code,
            "invalid_type"
        );
    }

    #[test]
    fn assembly_joins_scripts_and_marks_unmapped_state() {
        let m = build();
        let r = Reader::new(&m);
        let live = read_live(&r, IMAGE, 0x100000).unwrap();
        let cat = Catalog {
            upgrades: upgrades_from(&script::parse(UPGRADES)),
            operations: operations_from(&script::parse(OPERATIONS)),
            defines: vec![("AGENCY_CREATION_DAYS", 30.0)],
        };
        let v = assemble(&live, &cat, true);
        assert_eq!(v["agency"]["name"], json!("Abwehr"));
        assert_eq!(v["agency"]["created"], Value::Null);
        assert_eq!(v["upgrades"]["upgrade_count"], json!(3));
        let army = &v["upgrades"]["branches"][0]["upgrades"][1];
        assert_eq!(army["levels"], json!(2));
        assert_eq!(
            army["requires_upgrades"],
            json!(["upgrade_economy_civilian"])
        );
        assert_eq!(v["operatives"]["state_read"], json!(false));
        assert_eq!(v["intel_network"]["state_read"], json!(false));
        assert_eq!(
            v["operations"]["entries"][0]["key"],
            json!("operation_rescue_operative")
        );
        assert_eq!(v["defines"]["AGENCY_CREATION_DAYS"], json!(30.0));
        let compact = assemble(&live, &cat, false);
        assert_eq!(compact["operations"]["entries"], Value::Null);
    }

    #[test]
    fn missions_cover_the_binary_mission_classes() {
        assert_eq!(MISSIONS.len(), 8);
        assert!(MISSIONS.contains(&"build_intel_network"));
    }
}
