//! The facts of the running game for the focus rules, read through the server's ordinary read
//! tools (the cached reads of the plan predicates): nothing here sends input or opens a view.
//!
//! Read: the player's tag and the date (`hoi4_overview`), completed focuses
//! (`hoi4_focus_completed`), the loaded tree's focus keys (`hoi4_focus_list`), wars (`hoi4_wars`,
//! `hoi4_diplomacy_state`), state owners and controllers (`hoi4_states`), stability, war support,
//! manpower (`hoi4_country_status`), political power and factory counts (`hoi4_economy`), world
//! tension, the faction, the overlord and the subjects (`hoi4_diplomacy_state`), researched
//! technologies (`hoi4_research`), and the DLCs of the install. The ruling ideology, ideas,
//! flags, variables, leaders and other countries' affairs are not read: triggers on them stay
//! unknown.
use super::{Facts, Holders};
use crate::plan_predicate::{Ctx, Truth};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;

/// The DLCs of an install: on, off, or present with an unknown switch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dlcs {
    on: HashSet<String>,
    maybe: HashSet<String>,
    /// The DLC folders were listed, so a name that is absent is off.
    listed: bool,
}

/// `name = "..."` of a `.dlc` descriptor.
fn descriptor_name(text: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (key, value) = line.split('#').next()?.split_once('=')?;
        (key.trim() == "name")
            .then(|| value.trim().trim_matches('"').to_string())
            .filter(|name| !name.is_empty())
    })
}

impl Dlcs {
    /// The DLCs below `install` (`integrated_dlc/` is always on; `dlc/` is on unless the user
    /// folder's `dlc_load.json` lists it in `disabled_dlcs`). `disabled` is None when that file
    /// could not be read: a DLC in `dlc/` is then neither surely on nor off.
    pub fn read(install: &Path, disabled: Option<&[String]>) -> Self {
        let mut out = Self::default();
        for (folder, switchable) in [("integrated_dlc", false), ("dlc", true)] {
            let Ok(entries) = std::fs::read_dir(install.join(folder)) else {
                continue;
            };
            out.listed = true;
            for dir in entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
                let Ok(files) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for file in files.flatten().map(|e| e.path()) {
                    if file.extension().is_none_or(|e| e != "dlc") {
                        continue;
                    }
                    let Some(name) = std::fs::read(&file)
                        .ok()
                        .and_then(|b| descriptor_name(&String::from_utf8_lossy(&b)))
                    else {
                        continue;
                    };
                    // dlc_load.json names a DLC by its descriptor path ("dlc/dlc028_x.dlc" or
                    // "dlc/dlc028_x/dlc028.dlc"): match the folder or the file stem.
                    let stems: Vec<String> = [dir.file_stem(), file.file_stem()]
                        .into_iter()
                        .flatten()
                        .map(|s| s.to_string_lossy().to_ascii_lowercase())
                        .collect();
                    let off = |list: &[String]| {
                        list.iter().any(|entry| {
                            let entry = entry.replace('\\', "/").to_ascii_lowercase();
                            entry
                                .trim_end_matches(".dlc")
                                .split('/')
                                .any(|part| stems.iter().any(|s| s == part))
                        })
                    };
                    match (switchable, disabled) {
                        (false, _) => {
                            out.on.insert(name);
                        }
                        (true, Some(list)) => {
                            if !off(list) {
                                out.on.insert(name);
                            }
                        }
                        (true, None) => {
                            out.maybe.insert(name);
                        }
                    }
                }
            }
        }
        out
    }
    pub fn has(&self, name: &str) -> Truth {
        if self.on.contains(name) {
            Truth::True
        } else if self.maybe.contains(name) || !self.listed {
            Truth::Unknown
        } else {
            Truth::False
        }
    }
}

/// (year, month, day) of a game hour (hours since the year -5000 on the 365-day calendar).
pub fn ymd(hour: i64) -> Option<(i64, i64, i64)> {
    const DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if hour < 0 {
        return None;
    }
    let day = hour / 24;
    let mut rest = day % 365;
    for (month, length) in DAYS.iter().enumerate() {
        if rest < *length {
            return Some((day / 365 - 5000, month as i64 + 1, rest + 1));
        }
        rest -= length;
    }
    None
}

/// Live facts over one predicate context (its reads are made once and shared).
pub struct Live<'c, 'a> {
    ctx: RefCell<&'c mut Ctx<'a>>,
    dlcs: Option<&'c Dlcs>,
}

impl<'c, 'a> Live<'c, 'a> {
    pub fn new(ctx: &'c mut Ctx<'a>, dlcs: Option<&'c Dlcs>) -> Self {
        Self {
            ctx: RefCell::new(ctx),
            dlcs,
        }
    }
    fn diplomacy(&self) -> Option<Value> {
        self.ctx
            .borrow_mut()
            .read("hoi4_diplomacy_state", &json!({}))
            .cloned()
    }
}

fn truth(known: Option<bool>) -> Truth {
    known.map_or(Truth::Unknown, Truth::from_bool)
}

fn tags(list: &Value) -> Option<Vec<String>> {
    list.as_array()?
        .iter()
        .map(|row| {
            row["with"]["tag"]
                .as_str()
                .or_else(|| row["tag"].as_str())
                .map(str::to_string)
        })
        .collect()
}

impl Facts for Live<'_, '_> {
    fn player(&self) -> Option<String> {
        self.ctx.borrow_mut().player()
    }
    fn date(&self) -> Option<(i64, i64, i64)> {
        ymd(self.ctx.borrow().now)
    }
    fn dlc(&self, name: &str) -> Truth {
        self.dlcs.map_or(Truth::Unknown, |d| d.has(name))
    }
    fn completed(&self, focus: &str) -> Truth {
        truth(
            self.ctx
                .borrow_mut()
                .completed_focuses()
                .map(|done| done.contains(focus)),
        )
    }
    fn in_tree(&self, focus: &str) -> Truth {
        // The key itself as the prefix filter: one small page holds it when the tree has it.
        let mut ctx = self.ctx.borrow_mut();
        let Some(page) = ctx.read("hoi4_focus_list", &json!({"filter": focus, "limit": 100}))
        else {
            return Truth::Unknown;
        };
        let Some(rows) = page["focuses"].as_array() else {
            return Truth::Unknown;
        };
        if rows.iter().any(|row| row["key"].as_str() == Some(focus)) {
            Truth::True
        } else if page["next_offset"].is_null() {
            Truth::False
        } else {
            Truth::Unknown
        }
    }
    fn at_war(&self) -> Truth {
        truth(self.ctx.borrow_mut().war_enemies().map(|e| !e.is_empty()))
    }
    fn at_war_with(&self, tag: &str) -> Truth {
        truth(
            self.ctx
                .borrow_mut()
                .war_enemies()
                .map(|e| e.iter().any(|t| t == tag)),
        )
    }
    fn exists(&self, tag: &str) -> Truth {
        truth(self.ctx.borrow_mut().owns_a_state(tag))
    }
    fn state(&self, id: i64) -> Option<Holders> {
        let (owner, controller, full_controller) = self.ctx.borrow_mut().state_holders(id)?;
        Some(Holders {
            owner,
            controller,
            full_controller,
        })
    }
    fn number(&self, name: &str) -> Option<(f64, f64)> {
        let mut ctx = self.ctx.borrow_mut();
        match name {
            // The top bar shows whole percent.
            "stability" | "war_support" => ctx.number(name).map(|v| (v, 0.01)),
            "world_tension" => ctx.number(name).map(|v| (v, 0.01)),
            "political_power" => ctx.number("pp").map(|v| (v, 1.0)),
            // The top bar rounds manpower to three figures.
            "manpower" => ctx.number(name).map(|v| (v, v.abs() * 0.01 + 1.0)),
            "military_factories" | "civilian_factories" | "naval_factories" => {
                let kind = name.trim_end_matches("_factories");
                ctx.read("hoi4_economy", &json!({}))
                    .and_then(|e| e["factory_trigger_count"][kind].as_f64())
                    .map(|v| (v, 0.0))
            }
            _ => None,
        }
    }
    fn tech(&self, key: &str) -> Truth {
        truth(
            self.ctx
                .borrow_mut()
                .researched()
                .map(|done| done.contains(key)),
        )
    }
    fn faction(&self) -> Option<Vec<String>> {
        let state = self.diplomacy()?;
        let faction = &state["faction"];
        if faction["in_faction"] == json!(false) {
            return Some(Vec::new());
        }
        // A member whose tag did not resolve could be anyone: the list is not complete.
        if faction["faction"]["unresolved_members"].as_u64() != Some(0) {
            return None;
        }
        let me = self.player()?;
        Some(
            tags(&faction["faction"]["members"])?
                .into_iter()
                .filter(|tag| *tag != me)
                .collect(),
        )
    }
    fn overlord(&self) -> Option<Option<String>> {
        Some(tags(&self.diplomacy()?["overlord"])?.into_iter().next())
    }
    fn subjects(&self) -> Option<Vec<String>> {
        tags(&self.diplomacy()?["subjects"])
    }
    fn government(&self) -> Option<String> {
        None
    }
}

/// The verdicts of `focuses` as the `hoi4_focus_available` reply. `wars` are the countries at war
/// with the player (None: not read).
pub fn report(
    catalog: &super::Catalog,
    facts: &dyn Facts,
    focuses: &[String],
    wars: Option<Vec<String>>,
) -> Value {
    let rows: Vec<Value> = focuses
        .iter()
        .map(|focus| catalog.verdict(focus, facts).json(focus))
        .collect();
    json!({
        "source": "common/national_focus of the running game's content, judged against the own state read from memory",
        "player": facts.player(),
        "date": facts.date().map(|(y, m, d)| format!("{y}.{m}.{d}")),
        "wars": wars,
        "focuses": rows,
        "catalog_focuses": catalog.len(),
        "not_read": "ruling ideology, ideas, country and global flags, variables, leaders, other countries' affairs, any_/all_ scopes: a trigger on them is unknown",
        "input_authorized": false,
        "observation_paused_game": false,
    })
}
