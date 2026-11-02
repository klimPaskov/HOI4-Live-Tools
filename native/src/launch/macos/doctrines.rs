//! `hoi4_doctrines`: the own doctrine state (grand doctrine per folder, the
//! subdoctrine and mastery of every track, army/navy/air experience, the
//! temporary doctrine cost reductions) read from game memory, joined with the
//! install's doctrine scripts so ids, costs and the adoptable lists are named.
//! Pure read: no input, no lease, no cursor movement, never pauses.
//!
//! Layout (macOS Mach-O 089EA519, mapped live on 26 October 2026 from the
//! RTTI vtable map; every field names how it was checked):
//!
//! * `GS+0x3c0` -> `NDoctrines::CDoctrineSystem`; its country-status array
//!   (`+0x8`, stride 0xa0, one per country index) holds
//!   `CCountryDoctrineStatus` (`+0x8` country index).
//!   CHECKED: type names, index join, and a back pointer from every folder
//!   status (`+0x30`) and track status (`+0x50`/`+0x58`).
//! * status `+0x10` -> array of 4 `CFolderStatus` (stride 0x50: land, naval,
//!   air, special_forces; `+0x8` folder template, `+0x10` grand doctrine
//!   template or null, `+0x18` track array of stride 0x60).
//!   CHECKED live: Germany shows new_mobile_warfare / new_convoy_raiding /
//!   new_battlefield_support, exactly the history file's `set_grand_doctrine`
//!   lines, special forces empty.
//! * template: `+0x18` i32 xp cost, `+0x1c` i32 xp type (1 army, 2 navy, 3
//!   air), `+0x20` loc-key string. CHECKED: 100 / 1,2,3 for the three above.
//! * track status: `+0x8` subdoctrine template or null, `+0x10` a flag of
//!   unknown meaning (only AI countries at war showed it set; it is not
//!   "completed": a flagged Japanese track held 310 mastery against 500 of
//!   rewards), `+0x18` mastery and `+0x20` banked mastery (raw / 32768).
//!   NOT CHECKED against the screen (no track has a subdoctrine for Germany
//!   yet): the scale is inferred (banked values of every country stay under
//!   the 200 cap of MASTERY_BANK_MAX, a flagged track holds 549 mastery
//!   against 500 of rewards), so these fields are `validated:false`.
//! * status `+0x40` -> `STemporaryCostReduction` array (stride 0x38: `+0x8`
//!   i32 uses left, `+0x10` name, `+0x28` raw/100000 reduction, `+0x30`
//!   folder template). CHECKED live against the focus scripts (-0.5, one use).
//! * `country+0x12e8` -> `CCountryExperienceStatus`: army `+0x10`, navy
//!   `+0x28`, air `+0x40` (raw / 32768). CHECKED live against the top bar.
use super::{
    mgmt::{self, FIXED_SCALE, XP_SCALE},
    script::{self, Block},
    ui::read,
};
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Map, Value};
use std::{path::Path, time::Instant};

const SYSTEM: u64 = 0x3c0;
const SYSTEM_TYPE: &str = "N10NDoctrines15CDoctrineSystemE";
const STATUS_TYPE: &str = "N10NDoctrines22CCountryDoctrineStatusE";
const GRAND_TYPE: &str = "N10NDoctrines22CGrandDoctrineTemplateE";
const SUB_TYPE: &str = "N10NDoctrines20CSubDoctrineTemplateE";
const EXPERIENCE: u64 = 0x12e8;
const EXPERIENCE_TYPE: &str = "24CCountryExperienceStatus";
const STATUS_STRIDE: usize = 0xa0;
const FOLDER_STRIDE: usize = 0x50;
const TRACK_STRIDE: usize = 0x60;
const REDUCTION_STRIDE: usize = 0x38;
const FOLDER_LIMIT: usize = 8;
const TRACK_LIMIT: usize = 16;
const REDUCTION_LIMIT: usize = 32;
const DEFAULT_FOLDERS: [&str; 4] = ["land", "naval", "air", "special_forces"];

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RawTemplate {
    pub name_key: String,
    pub xp_cost: i32,
    pub xp_type: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RawTrack {
    pub subdoctrine: Option<RawTemplate>,
    /// Raw `+0x10` flag: meaning not established (see the layout notes).
    pub flag: bool,
    pub mastery: f64,
    pub banked: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RawFolder {
    pub template: u64,
    pub grand: Option<RawTemplate>,
    pub tracks: Vec<RawTrack>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct RawReduction {
    pub name: String,
    pub uses_left: i32,
    pub reduction: f64,
    pub folder_template: u64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Snapshot {
    pub xp: [f64; 3],
    pub folders: Vec<RawFolder>,
    pub reductions: Vec<RawReduction>,
    pub mastery_bonuses: usize,
}

// ---------------------------------------------------------------- catalogue

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct FolderDef {
    pub key: String,
    pub ledger: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct GrandDef {
    pub id: String,
    pub folder: String,
    pub name_key: String,
    pub xp_cost: Option<f64>,
    pub xp_type: String,
    pub tracks: Vec<String>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SubDef {
    pub id: String,
    pub folder: String,
    pub name_key: String,
    pub tracks: Vec<String>,
    pub xp_cost: Option<f64>,
    pub xp_type: String,
    /// Mastery each reward needs, in unlock order.
    pub rewards: Vec<f64>,
    pub xor: Vec<String>,
    pub multi_track: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Catalog {
    pub folders: Vec<FolderDef>,
    pub grand: Vec<GrandDef>,
    pub sub: Vec<SubDef>,
    pub default_reward: f64,
}

fn folder_for_dir(dir: &str) -> &str {
    match dir {
        "sea" => "naval",
        other => other,
    }
}

/// Build the catalogue from already parsed script entries; `subs` pairs a
/// subdoctrine directory name (land, sea, air, special_forces) with its entries.
pub(super) fn catalog_from(
    folders: &[script::Entry],
    grand: &[script::Entry],
    subs: &[(String, Vec<script::Entry>)],
    default_reward: f64,
) -> Catalog {
    let mut out = Catalog {
        default_reward,
        ..Catalog::default()
    };
    for e in folders {
        if let script::Val::Block(body) = &e.val {
            out.folders.push(FolderDef {
                key: e.key.clone(),
                ledger: body.text("ledger").unwrap_or("").to_string(),
            });
        }
    }
    for e in grand {
        let script::Val::Block(body) = &e.val else {
            continue;
        };
        if body.text("folder").is_none() {
            continue;
        }
        out.grand.push(GrandDef {
            id: e.key.clone(),
            folder: body.text("folder").unwrap_or("").to_string(),
            name_key: body.text("name").unwrap_or("").to_string(),
            xp_cost: body.number("xp_cost"),
            xp_type: body.text("xp_type").unwrap_or("army").to_string(),
            tracks: body.words("tracks"),
        });
    }
    for (dir, entries) in subs {
        for e in entries {
            let script::Val::Block(body) = &e.val else {
                continue;
            };
            let Some(track) = body.find_val("track") else {
                continue;
            };
            let tracks = match track {
                script::Val::Scalar(s) => vec![s.clone()],
                script::Val::Block(_) => body.words("track"),
            };
            let rewards = body
                .find_val("rewards")
                .map(|v| {
                    v.block()
                        .iter()
                        .filter(|r| matches!(r.val, script::Val::Block(_)))
                        .map(|r| r.val.block().number("mastery").unwrap_or(default_reward))
                        .collect()
                })
                .unwrap_or_default();
            out.sub.push(SubDef {
                id: e.key.clone(),
                folder: folder_for_dir(dir).to_string(),
                name_key: body.text("name").unwrap_or("").to_string(),
                tracks,
                xp_cost: body.number("xp_cost"),
                xp_type: body.text("xp_type").unwrap_or("army").to_string(),
                rewards,
                xor: body.words("xor"),
                multi_track: body.text("allow_in_multiple_tracks") == Some("yes"),
            });
        }
    }
    out
}

fn default_reward_mastery(dir: &Path) -> f64 {
    let Ok(text) = std::fs::read_to_string(dir.join("common/defines/00_defines.lua")) else {
        return 100.0;
    };
    text.lines()
        .find(|l| l.trim_start().starts_with("DEFAULT_REWARD_MASTERY"))
        .and_then(|l| l.split('=').nth(1))
        .and_then(|v| {
            v.trim()
                .trim_end_matches(',')
                .split_whitespace()
                .next()
                .map(str::to_string)
        })
        .and_then(|v| v.trim_end_matches(',').parse().ok())
        .unwrap_or(100.0)
}

fn load_catalog(dir: &Path) -> Catalog {
    let base = dir.join("common/doctrines");
    let folders = script::parse_dir(&base.join("folders"));
    let grand = script::parse_dir(&base.join("grand_doctrines"));
    let mut subs = Vec::new();
    for sub in ["land", "sea", "air", "special_forces"] {
        subs.push((
            sub.to_string(),
            script::parse_dir(&base.join("subdoctrines").join(sub)),
        ));
    }
    catalog_from(&folders, &grand, &subs, default_reward_mastery(dir))
}

pub(super) fn catalog(dir: &Path) -> std::sync::Arc<Catalog> {
    script::cached(dir, "doctrines", load_catalog)
}

// ------------------------------------------------------------------ memory

fn template(r: &Reader<'_>, address: u64, mangled: &str) -> Result<RawTemplate> {
    mgmt::expect_type(r, address, mangled)?;
    let head = r.read(address, 0x20)?;
    let name_key = mgmt::text(r, address + 0x20, 128)?
        .ok_or_else(|| Error::new("invalid_string", "Doctrine template name unreadable"))?;
    Ok(RawTemplate {
        name_key,
        xp_cost: mgmt::i32_at(&head, 0x18),
        xp_type: mgmt::i32_at(&head, 0x1c),
    })
}

pub(super) fn snapshot(r: &Reader<'_>, image: u64) -> Result<Snapshot> {
    let own = mgmt::own(r, image)?;
    let system = mgmt::pointer(r, own.gs + SYSTEM)?;
    mgmt::expect_type(r, system, SYSTEM_TYPE)?;
    let (_, data, size) = r.array_header(system + 8)?;
    if own.index as usize >= size {
        return Err(Error::new(
            "invalid_array",
            "Own doctrine status outside the country array",
        ));
    }
    let status = data + own.index as u64 * STATUS_STRIDE as u64;
    let experience = mgmt::pointer(r, own.country + EXPERIENCE)?;
    read_status_at(r, status, own.index, experience)
}

/// Read one country's doctrine status, its folders, tracks, reductions and the
/// experience object. Every object is type-checked and every back pointer
/// joined, so a stale or torn structure is an error, never a guess.
pub(super) fn read_status_at(
    r: &Reader<'_>,
    status: u64,
    index: i32,
    experience: u64,
) -> Result<Snapshot> {
    mgmt::expect_type(r, status, STATUS_TYPE)?;
    let head = r.read(status, STATUS_STRIDE)?;
    if mgmt::i32_at(&head, 8) != index {
        return Err(Error::new(
            "invalid_array",
            "Doctrine status country index mismatch",
        ));
    }
    let (_, folder_data, folder_count) = r.array_header(status + 0x10)?;
    if folder_count > FOLDER_LIMIT {
        return Err(Error::new("invalid_array", "Implausible doctrine folders"));
    }
    let folder_bytes = if folder_count == 0 {
        Vec::new()
    } else {
        r.read(folder_data, folder_count * FOLDER_STRIDE)?
    };
    let mut folders = Vec::new();
    for f in 0..folder_count {
        let base = f * FOLDER_STRIDE;
        let this = folder_data + base as u64;
        if mgmt::u64_at(&folder_bytes, base + 0x30) != status {
            return Err(Error::new("torn_read", "Folder status owner changed"));
        }
        let template_ptr = mgmt::u64_at(&folder_bytes, base + 8);
        let grand_ptr = mgmt::u64_at(&folder_bytes, base + 0x10);
        let grand = if grand_ptr == 0 {
            None
        } else {
            Some(template(r, grand_ptr, GRAND_TYPE)?)
        };
        let (_, track_data, track_count) = r.array_header(this + 0x18)?;
        if track_count > TRACK_LIMIT {
            return Err(Error::new("invalid_array", "Implausible doctrine tracks"));
        }
        let track_bytes = if track_count == 0 {
            Vec::new()
        } else {
            r.read(track_data, track_count * TRACK_STRIDE)?
        };
        let mut tracks = Vec::new();
        for t in 0..track_count {
            let tb = t * TRACK_STRIDE;
            if mgmt::u64_at(&track_bytes, tb + 0x50) != status
                || mgmt::u64_at(&track_bytes, tb + 0x58) != this
            {
                return Err(Error::new("torn_read", "Track status owner changed"));
            }
            let sub_ptr = mgmt::u64_at(&track_bytes, tb + 8);
            tracks.push(RawTrack {
                subdoctrine: if sub_ptr == 0 {
                    None
                } else {
                    Some(template(r, sub_ptr, SUB_TYPE)?)
                },
                flag: mgmt::i32_at(&track_bytes, tb + 0x10) != 0,
                mastery: mgmt::i64_at(&track_bytes, tb + 0x18) as f64 / XP_SCALE,
                banked: mgmt::i64_at(&track_bytes, tb + 0x20) as f64 / XP_SCALE,
            });
        }
        folders.push(RawFolder {
            template: template_ptr,
            grand,
            tracks,
        });
    }
    let (_, red_data, red_count) = r.array_header(status + 0x40)?;
    if red_count > REDUCTION_LIMIT {
        return Err(Error::new("invalid_array", "Implausible cost reductions"));
    }
    let red_bytes = if red_count == 0 {
        Vec::new()
    } else {
        r.read(red_data, red_count * REDUCTION_STRIDE)?
    };
    let mut reductions = Vec::new();
    for i in 0..red_count {
        let base = i * REDUCTION_STRIDE;
        reductions.push(RawReduction {
            name: mgmt::text(r, red_data + base as u64 + 0x10, 128)?.unwrap_or_default(),
            uses_left: mgmt::i32_at(&red_bytes, base + 8),
            reduction: -(mgmt::i64_at(&red_bytes, base + 0x28) as f64) / FIXED_SCALE,
            folder_template: mgmt::u64_at(&red_bytes, base + 0x30),
        });
    }
    let (_, _, bonus_count) = r.array_header(status + 0x58)?;
    mgmt::expect_type(r, experience, EXPERIENCE_TYPE)?;
    let xp = r.read(experience, 0x48)?;
    let xp = [0x10, 0x28, 0x40].map(|o| mgmt::i64_at(&xp, o) as f64 / XP_SCALE);
    // Re-read the status header: a changed folder array means a torn bracket.
    if r.read(status, STATUS_STRIDE)? != head {
        return Err(Error::new("torn_read", "Doctrine status changed"));
    }
    Ok(Snapshot {
        xp,
        folders,
        reductions,
        mastery_bonuses: bonus_count,
    })
}

// ---------------------------------------------------------------- assembly

pub(super) fn xp_type_name(raw: i32) -> &'static str {
    match raw {
        1 => "army",
        2 => "navy",
        3 => "air",
        _ => "unknown",
    }
}
pub(super) fn xp_index(name: &str) -> Option<usize> {
    match name {
        "army" => Some(0),
        "navy" => Some(1),
        "air" => Some(2),
        _ => None,
    }
}

/// Mastery thresholds are cumulative: reward k unlocks once the total of the
/// first k thresholds is reached.
pub(super) fn rewards_unlocked(thresholds: &[f64], mastery: f64) -> usize {
    let mut total = 0.0;
    let mut n = 0;
    for t in thresholds {
        total += t;
        if mastery + 1e-9 >= total {
            n += 1;
        } else {
            break;
        }
    }
    n
}

fn grand_json(t: &RawTemplate, catalog: &Catalog) -> Value {
    let def = catalog.grand.iter().find(|g| g.name_key == t.name_key);
    json!({
        "id": def.map(|d| d.id.as_str()),
        "name_key": t.name_key,
        "xp_cost": t.xp_cost,
        "xp_type": xp_type_name(t.xp_type),
        "tracks": def.map(|d| d.tracks.clone()),
    })
}

fn pick_sub<'a>(catalog: &'a Catalog, name_key: &str, track: Option<&str>) -> Option<&'a SubDef> {
    let mut matches = catalog.sub.iter().filter(|s| s.name_key == name_key);
    let found: Vec<&SubDef> = matches.by_ref().collect();
    found
        .iter()
        .find(|s| track.is_some_and(|t| s.tracks.iter().any(|x| x == t)))
        .or(found.first())
        .copied()
}

pub(super) fn best_reduction(snapshot: &Snapshot, folder_template: u64) -> f64 {
    snapshot
        .reductions
        .iter()
        .filter(|r| r.folder_template == folder_template && r.uses_left > 0)
        .map(|r| r.reduction)
        .fold(0.0, f64::max)
}

pub(super) fn assemble(snapshot: &Snapshot, catalog: &Catalog, include_catalog: bool) -> Value {
    let folder_keys: Vec<String> = if catalog.folders.is_empty() {
        DEFAULT_FOLDERS.iter().map(|s| s.to_string()).collect()
    } else {
        catalog.folders.iter().map(|f| f.key.clone()).collect()
    };
    let chosen_subs: Vec<String> = snapshot
        .folders
        .iter()
        .flat_map(|f| f.tracks.iter())
        .filter_map(|t| t.subdoctrine.as_ref().map(|s| s.name_key.clone()))
        .collect();
    let mut folders = Vec::new();
    for (index, raw) in snapshot.folders.iter().enumerate() {
        let key = folder_keys
            .get(index)
            .cloned()
            .unwrap_or_else(|| format!("folder_{index}"));
        let ledger = catalog
            .folders
            .iter()
            .find(|f| f.key == key)
            .map(|f| f.ledger.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| "army".to_string());
        let xp_type = raw
            .grand
            .as_ref()
            .map(|g| xp_type_name(g.xp_type).to_string())
            .unwrap_or(ledger);
        let xp_have = xp_index(&xp_type).map(|i| snapshot.xp[i]);
        let reduction = best_reduction(snapshot, raw.template);
        let grand = raw.grand.as_ref().map(|g| grand_json(g, catalog));
        let grand_def = raw
            .grand
            .as_ref()
            .and_then(|g| catalog.grand.iter().find(|d| d.name_key == g.name_key));
        let mut tracks = Vec::new();
        for (t, track) in raw.tracks.iter().enumerate() {
            let track_key = grand_def.and_then(|d| d.tracks.get(t)).cloned();
            let sub_def = track
                .subdoctrine
                .as_ref()
                .and_then(|s| pick_sub(catalog, &s.name_key, track_key.as_deref()));
            let thresholds = sub_def.map(|d| d.rewards.clone()).unwrap_or_default();
            let unlocked = rewards_unlocked(&thresholds, track.mastery);
            let completed_estimate = !thresholds.is_empty() && unlocked >= thresholds.len();
            let next_at = if unlocked >= thresholds.len() {
                None
            } else {
                Some(thresholds[..=unlocked].iter().sum::<f64>())
            };
            let mut entry = Map::new();
            entry.insert("index".into(), json!(t));
            entry.insert("track".into(), json!(track_key));
            entry.insert(
                "subdoctrine".into(),
                match &track.subdoctrine {
                    None => Value::Null,
                    Some(s) => json!({
                        "id": sub_def.map(|d| d.id.as_str()),
                        "name_key": s.name_key,
                        "xp_cost": s.xp_cost,
                        "xp_type": xp_type_name(s.xp_type),
                    }),
                },
            );
            entry.insert("completed_estimate".into(), json!(completed_estimate));
            entry.insert("flag_0x10".into(), json!(track.flag));
            entry.insert("mastery".into(), json!(mgmt::round(track.mastery, 2)));
            entry.insert("banked_mastery".into(), json!(mgmt::round(track.banked, 2)));
            entry.insert(
                "rewards".into(),
                json!({
                    "total": thresholds.len(),
                    "unlocked_estimate": unlocked,
                    "next_unlock_at_mastery": next_at,
                    "default_reward_mastery": catalog.default_reward,
                }),
            );
            if include_catalog && track.subdoctrine.is_none() {
                let options: Vec<Value> = catalog
                    .sub
                    .iter()
                    .filter(|s| {
                        track_key.as_ref().is_some_and(|k| s.tracks.contains(k))
                            && (s.folder == key || s.folder.is_empty())
                    })
                    .map(|s| {
                        let cost = s.xp_cost.map(|c| c * (1.0 - reduction));
                        json!({
                            "id": s.id,
                            "name_key": s.name_key,
                            "xp_type": s.xp_type,
                            "xp_cost": s.xp_cost,
                            "estimated_cost_after_reduction": cost.map(|c| mgmt::round(c, 1)),
                            "already_chosen_elsewhere": !s.multi_track && chosen_subs.contains(&s.name_key),
                            "xor": s.xor,
                        })
                    })
                    .collect();
                entry.insert("available_subdoctrines".into(), json!(options));
                entry.insert("availability_conditions_checked".into(), json!(false));
            }
            tracks.push(Value::Object(entry));
        }
        let mut item = Map::new();
        item.insert("folder".into(), json!(key));
        item.insert("xp_type".into(), json!(xp_type));
        item.insert(
            "experience".into(),
            json!(xp_have.map(|x| mgmt::round(x, 2))),
        );
        item.insert("grand_doctrine".into(), json!(grand));
        item.insert("best_cost_reduction".into(), json!(reduction));
        item.insert("tracks".into(), json!(tracks));
        if raw.grand.is_none() && include_catalog {
            let options: Vec<Value> = catalog
                .grand
                .iter()
                .filter(|g| g.folder == key)
                .map(|g| {
                    let cost = g.xp_cost.map(|c| c * (1.0 - reduction));
                    json!({
                        "id": g.id,
                        "name_key": g.name_key,
                        "xp_type": g.xp_type,
                        "xp_cost": g.xp_cost,
                        "estimated_cost_after_reduction": cost.map(|c| mgmt::round(c, 1)),
                        "tracks": g.tracks,
                    })
                })
                .collect();
            item.insert("available_grand_doctrines".into(), json!(options));
            item.insert("availability_conditions_checked".into(), json!(false));
        }
        folders.push(Value::Object(item));
    }
    let reductions: Vec<Value> = snapshot
        .reductions
        .iter()
        .map(|r| {
            let folder = snapshot
                .folders
                .iter()
                .position(|f| f.template == r.folder_template)
                .and_then(|i| folder_keys.get(i).cloned());
            json!({"name":r.name,"folder":folder,"reduction":mgmt::round(r.reduction,4),"uses_left":r.uses_left})
        })
        .collect();
    json!({
        "experience": {
            "army": {"value": mgmt::round(snapshot.xp[0], 2), "floor": snapshot.xp[0].floor()},
            "navy": {"value": mgmt::round(snapshot.xp[1], 2), "floor": snapshot.xp[1].floor()},
            "air": {"value": mgmt::round(snapshot.xp[2], 2), "floor": snapshot.xp[2].floor()},
            "source": "CCountryExperienceStatus raw/32768",
            "validated": true,
        },
        "folders": folders,
        "cost_reductions": {"entries": reductions, "source": "STemporaryCostReduction", "validated": true},
        "mastery_bonuses_active": snapshot.mastery_bonuses,
        "validated": {
            "grand_doctrines": true,
            "experience": true,
            "cost_reductions": true,
            "track_subdoctrine_ptr": false,
            "track_mastery_scale": false,
            "reward_progress_estimate": false,
        },
        "not_read": [
            "doctrine cost modifiers (<folder>_doctrine_cost_factor): costs shown are script base costs scaled by the best temporary reduction only",
            "availability triggers of grand doctrines and subdoctrines are not evaluated",
        ],
    })
}

pub(super) fn doctrines(process: &dyn Process, include_catalog: bool) -> Result<Value> {
    let started = Instant::now();
    let dir = mgmt::game_dir(process).ok();
    let cat = dir
        .as_deref()
        .map(catalog)
        .unwrap_or_else(|| std::sync::Arc::new(Catalog::default()));
    let (snap, context, stamp, build_id) = read(process, |r, image| {
        let snap = snapshot(r, image)?;
        Ok((
            snap,
            mgmt::clock(r, image)?,
            mgmt::stamp(started, r),
            game::layout(r).build_id,
        ))
    })?;
    let mut body = assemble(&snap, &cat, include_catalog);
    let object = body.as_object_mut().expect("object");
    object.insert("source".into(), json!("native own doctrine read"));
    object.insert("build_id".into(), json!(build_id));
    object.insert("owned".into(), json!(true));
    object.insert("game_time".into(), context);
    object.insert("freshness".into(), stamp);
    object.insert(
        "catalog".into(),
        json!({"loaded": !cat.grand.is_empty(), "grand_doctrines": cat.grand.len(), "subdoctrines": cat.sub.len()}),
    );
    object.insert("observation_paused_game".into(), json!(false));
    object.insert("input_authorized".into(), json!(false));
    Ok(body)
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_doctrines","description":"Own doctrines read from game memory (no input, no lease, never pauses). Per folder (land, naval, air, special_forces): the grand doctrine (id, loc key, XP cost and type), every track with its subdoctrine, mastery, banked mastery and an estimate of unlocked rewards; army/navy/air experience; the temporary doctrine cost reductions with uses left. include_catalog:true adds, per empty slot, the adoptable grand doctrines or the subdoctrines of the track with their script costs and costs after the best reduction (availability triggers are not evaluated). Validated live: grand doctrines, experience and cost reductions; track mastery fields are validated:false until a track holds a subdoctrine on screen.","inputSchema":{"type":"object","properties":{"include_catalog":{"type":"boolean"}},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::mgmt::fake::Mem;
    use super::*;

    const GRAND: &str = r#"
        new_mobile_warfare = {
            folder = land
            name = GRAND_DOCTRINE_MOBILE_WARFARE
            xp_cost = 100
            xp_type = army
            tracks = { infantry combat_support armor operations }
        }
        new_convoy_raiding = { folder = naval name = GRAND_DOCTRINE_CONVOY_RAIDING xp_cost = 100 xp_type = navy tracks = { screens } }
        "#;
    const SUB: &str = r#"
        mobile_infantry = {
            track = infantry
            name = SUBDOCTRINE_MOBILE_INFANTRY
            xp_cost = 100
            xp_type = army
            rewards = { a = { x = 1 } b = { mastery = 150 } c = { x = 1 } }
        }
        bicycle = { track = { infantry armor } allow_in_multiple_tracks = yes name = SUBDOCTRINE_BICYCLE xp_cost = 50 }
        "#;
    const FOLDERS: &str = "land = { ledger = army } naval = { ledger = navy } air = { ledger = air } special_forces = { ledger = army }";

    fn cat() -> Catalog {
        catalog_from(
            &script::parse(FOLDERS),
            &script::parse(GRAND),
            &[("land".to_string(), script::parse(SUB))],
            100.0,
        )
    }

    #[test]
    fn catalogue_names_grand_doctrines_subdoctrines_and_rewards() {
        let c = cat();
        assert_eq!(c.folders.len(), 4);
        assert_eq!(c.grand[0].id, "new_mobile_warfare");
        assert_eq!(c.grand[0].tracks.len(), 4);
        assert_eq!(c.sub[0].rewards, vec![100.0, 150.0, 100.0]);
        assert_eq!(c.sub[1].tracks, ["infantry", "armor"]);
        assert!(c.sub[1].multi_track);
        assert_eq!(c.sub[0].folder, "land");
    }

    #[test]
    fn default_reward_mastery_comes_from_the_defines_with_a_safe_fallback() {
        let dir =
            std::env::temp_dir().join(format!("hoi4-doctrine-defines-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("common/defines")).unwrap();
        assert_eq!(default_reward_mastery(&dir), 100.0);
        std::fs::write(
            dir.join("common/defines/00_defines.lua"),
            "NDoctrines = {\n        DEFAULT_REWARD_MASTERY = 120.5,   -- comment\n}\n",
        )
        .unwrap();
        assert_eq!(default_reward_mastery(&dir), 120.5);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rewards_unlock_cumulatively() {
        let t = [100.0, 150.0, 100.0];
        assert_eq!(rewards_unlocked(&t, 0.0), 0);
        assert_eq!(rewards_unlocked(&t, 99.9), 0);
        assert_eq!(rewards_unlocked(&t, 100.0), 1);
        assert_eq!(rewards_unlocked(&t, 249.0), 1);
        assert_eq!(rewards_unlocked(&t, 250.0), 2);
        assert_eq!(rewards_unlocked(&t, 450.0), 3);
        assert_eq!(rewards_unlocked(&[], 500.0), 0);
    }

    fn snap() -> Snapshot {
        let tpl = |key: &str, ty: i32| RawTemplate {
            name_key: key.into(),
            xp_cost: 100,
            xp_type: ty,
        };
        Snapshot {
            xp: [166.19, 76.5, 50.0],
            folders: vec![
                RawFolder {
                    template: 0xaa,
                    grand: Some(tpl("GRAND_DOCTRINE_MOBILE_WARFARE", 1)),
                    tracks: vec![
                        RawTrack {
                            subdoctrine: Some(tpl("SUBDOCTRINE_MOBILE_INFANTRY", 1)),
                            flag: false,
                            mastery: 260.0,
                            banked: 0.0,
                        },
                        RawTrack {
                            subdoctrine: None,
                            flag: false,
                            mastery: 0.0,
                            banked: 33.5,
                        },
                    ],
                },
                RawFolder {
                    template: 0xbb,
                    grand: None,
                    tracks: vec![],
                },
            ],
            reductions: vec![
                RawReduction {
                    name: "GER_adopt_new_panzer_doctrine".into(),
                    uses_left: 1,
                    reduction: 0.5,
                    folder_template: 0xaa,
                },
                RawReduction {
                    name: "spent".into(),
                    uses_left: 0,
                    reduction: 0.75,
                    folder_template: 0xaa,
                },
            ],
            mastery_bonuses: 2,
        }
    }

    #[test]
    fn assembly_joins_memory_with_the_catalogue() {
        let v = assemble(&snap(), &cat(), true);
        assert_eq!(v["experience"]["army"]["floor"], json!(166.0));
        let land = &v["folders"][0];
        assert_eq!(land["folder"], json!("land"));
        assert_eq!(land["grand_doctrine"]["id"], json!("new_mobile_warfare"));
        assert_eq!(land["best_cost_reduction"], json!(0.5));
        let t0 = &land["tracks"][0];
        assert_eq!(t0["track"], json!("infantry"));
        assert_eq!(t0["subdoctrine"]["id"], json!("mobile_infantry"));
        assert_eq!(t0["rewards"]["total"], json!(3));
        assert_eq!(t0["rewards"]["unlocked_estimate"], json!(2));
        assert_eq!(t0["completed_estimate"], json!(false));
        let t1 = &land["tracks"][1];
        assert_eq!(t1["subdoctrine"], Value::Null);
        assert_eq!(t1["banked_mastery"], json!(33.5));
        // Empty track lists the subdoctrines of its own track only.
        assert!(t1["available_subdoctrines"].as_array().unwrap().is_empty());
        // The naval folder has no grand doctrine: it lists the adoptable ones.
        let naval = &v["folders"][1];
        assert_eq!(naval["xp_type"], json!("navy"));
        assert_eq!(naval["grand_doctrine"], Value::Null);
        assert_eq!(
            naval["available_grand_doctrines"][0]["id"],
            json!("new_convoy_raiding")
        );
        let reductions = v["cost_reductions"]["entries"].as_array().unwrap();
        assert_eq!(reductions[0]["folder"], json!("land"));
        assert_eq!(v["validated"]["track_mastery_scale"], json!(false));
    }

    #[test]
    fn spent_reductions_do_not_lower_costs() {
        let s = snap();
        assert_eq!(best_reduction(&s, 0xaa), 0.5);
        assert_eq!(best_reduction(&s, 0xbb), 0.0);
    }

    #[test]
    fn assembly_without_a_catalogue_still_reports_memory() {
        let v = assemble(&snap(), &Catalog::default(), false);
        assert_eq!(v["folders"][0]["folder"], json!("land"));
        assert_eq!(v["folders"][0]["grand_doctrine"]["id"], Value::Null);
        assert_eq!(
            v["folders"][0]["grand_doctrine"]["name_key"],
            json!("GRAND_DOCTRINE_MOBILE_WARFARE")
        );
        assert!(v["folders"][1].get("available_grand_doctrines").is_none());
    }

    // ------------------------------------------------------ memory layout

    const IMG: u64 = 0x1_0000_0000;

    /// Put a polymorphic object of a named class at `at` (vptr/typeinfo/name
    /// live in the scratch area below 0x40000).
    fn object(m: &Mem, at: u64, class: &str, slot: u64) {
        let vt = 0x41000 + slot * 0x100;
        m.q(at, vt);
        m.q(vt - 8, vt + 0x40);
        m.q(vt + 0x40 + 8, vt + 0x80);
        m.patch(vt + 0x80, format!("{class}\0").as_bytes());
    }

    #[test]
    fn snapshot_reads_the_own_status_and_rejects_foreign_back_pointers() {
        let m = Mem::default();
        // Scratch for type metadata; status array; folders; tracks; templates.
        m.alloc(0x40000, 0x5000);
        m.alloc(0x100000, 0x1000); // system
        m.alloc(0x110000, 0xa0 * 4); // statuses (index 2 is own)
        m.alloc(0x120000, 0x50 * 2); // folders
        m.alloc(0x130000, 0x60 * 2); // tracks of folder 0
        m.alloc(0x140000, 0x100); // grand template
        m.alloc(0x150000, 0x100); // sub template
        m.alloc(0x160000, 0x38); // reduction
        m.alloc(0x170000, 0x100); // experience
        let own = 2u64;
        let status = 0x110000 + own * 0xa0;
        object(&m, 0x100000, SYSTEM_TYPE, 0);
        m.q(0x100008, 0x110000);
        m.i(0x100010, 4);
        m.i(0x100014, 4);
        object(&m, status, STATUS_TYPE, 1);
        m.i(status + 8, own as i32);
        m.q(status + 0x10, 0x120000);
        m.i(status + 0x18, 4);
        m.i(status + 0x1c, 2);
        m.q(status + 0x40, 0x160000);
        m.i(status + 0x48, 1);
        m.i(status + 0x4c, 1);
        // Folder 0: land with grand doctrine and one track holding a subdoctrine.
        m.q(0x120000 + 8, 0xaa);
        m.q(0x120000 + 0x10, 0x140000);
        m.q(0x120000 + 0x18, 0x130000);
        m.i(0x120000 + 0x20, 4);
        m.i(0x120000 + 0x24, 1);
        m.q(0x120000 + 0x30, status);
        // Folder 1: naval, empty.
        m.q(0x120050 + 8, 0xbb);
        m.q(0x120050 + 0x30, status);
        m.i(0x120050 + 0x20, 4);
        m.i(0x120050 + 0x24, 0);
        m.q(0x120050 + 0x18, 0x130000);
        object(&m, 0x140000, GRAND_TYPE, 2);
        m.i(0x140018, 100);
        m.i(0x14001c, 1);
        m.sso(0x140020, "GRAND_DOCTRINE_MOBILE_WARFARE");
        object(&m, 0x150000, SUB_TYPE, 3);
        m.i(0x150018, 100);
        m.i(0x15001c, 1);
        m.sso(0x150020, "SUBDOCTRINE_MOBILE_INFANTRY");
        m.q(0x130000 + 8, 0x150000);
        m.i(0x130000 + 0x10, 0);
        m.l(0x130000 + 0x18, (260.0 * XP_SCALE) as i64);
        m.q(0x130000 + 0x50, status);
        m.q(0x130000 + 0x58, 0x120000);
        // Reduction.
        m.i(0x160000 + 8, 1);
        m.sso(0x160000 + 0x10, "GER_adopt_new_panzer_doctrine");
        m.l(0x160000 + 0x28, -50_000);
        m.q(0x160000 + 0x30, 0xaa);
        // Experience.
        object(&m, 0x170000, EXPERIENCE_TYPE, 4);
        m.l(0x170000 + 0x10, (166.19 * XP_SCALE) as i64);
        m.l(0x170000 + 0x28, (76.5 * XP_SCALE) as i64);
        m.l(0x170000 + 0x40, (50.0 * XP_SCALE) as i64);
        let _ = (&m, IMG);
        // The own-country join is covered by economy tests; here only the
        // doctrine layer is exercised, so read it through a thin wrapper.
        let r = Reader::new(&m);
        let snap = read_status_at(&r, status, own as i32, 0x170000).unwrap();
        assert_eq!(snap.folders.len(), 2);
        assert_eq!(
            snap.folders[0].grand.as_ref().unwrap().name_key,
            "GRAND_DOCTRINE_MOBILE_WARFARE"
        );
        assert_eq!(snap.folders[0].tracks.len(), 1);
        let t = &snap.folders[0].tracks[0];
        assert_eq!(t.subdoctrine.as_ref().unwrap().xp_type, 1);
        assert!((t.mastery - 260.0).abs() < 1e-3);
        assert_eq!(snap.reductions[0].reduction, 0.5);
        assert!((snap.xp[0] - 166.19).abs() < 1e-3);
        // A track whose owner back pointer does not match is a torn read.
        m.q(0x130000 + 0x50, 0xdead_0000);
        let r = Reader::new(&m);
        assert_eq!(
            read_status_at(&r, status, own as i32, 0x170000)
                .unwrap_err()
                .code,
            "torn_read"
        );
    }
}
