//! The learned-skill store: what an agent found out by trying, kept as plain JSON files a person
//! can read, edit and share. One file per skill in `<store>/skills/<name>.json`, the facts in
//! `<store>/facts.json`. Nothing here talks to the game: the tool layer runs a skill's steps
//! through the ordinary tools and records the result with `record_run`.
//!
//! A skill names its parameters, the windows and widgets that must be on screen before it starts
//! (`requires`), its steps (tool calls; `{param}` in a string is replaced by the argument), what
//! proves it worked (`verify`), its cost and whether one click can undo it, and the game version
//! and mod set it was learned on. A skill whose required widgets are missing when it is run is
//! marked stale and refused from then on; one learned on another game version or mod set only
//! runs with `revalidate:true` (a successful run then moves it to the current one).
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub const MAX_STEPS: usize = 12;
/// Tools a skill step may call: the generic interface tools and every typed tool, never the
/// store itself (no skill runs another skill) and never a debug tool.
pub fn step_tool_allowed(tool: &str) -> bool {
    tool.starts_with("hoi4_")
        && !tool.starts_with("hoi4_skill_")
        && !tool.starts_with("hoi4_fact_")
        && !matches!(
            tool,
            "hoi4_navy_ui" | "hoi4_input_lease" | "hoi4_plan" | "hoi4_plan_run" | "hoi4_save"
        )
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Window ids that must be open (hoi4_ui_inspect `window`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub windows: Vec<String>,
    /// Widget ids (or unique names) that must be shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub widgets: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub tool: String,
    #[serde(default)]
    pub args: Map<String, Value>,
    /// What this step is for, in a few words (shown in run reports).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// One proof that the skill worked, checked after the last step.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// One of: window_opens, window_closes, widget_changes, read_changes.
    pub check: String,
    /// The window or widget id (window_*, widget_changes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// read_changes: the read tool and the dotted path of a number in its answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// read_changes: "up", "down" or "any".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LearnedOn {
    #[serde(default)]
    pub game_version: String,
    /// Stable id of the enabled mod set ("plain" without mods).
    #[serde(default)]
    pub mod_set: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Runs {
    #[serde(default)]
    pub ok: u32,
    #[serde(default)]
    pub failed: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_unix: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    pub name: String,
    pub purpose: String,
    /// Parameter name -> what to pass ("the state id to build in").
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub parameters: Map<String, Value>,
    #[serde(default)]
    pub requires: Requires,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verify: Vec<Check>,
    /// What one run spends, in words ("25 political power"), or "nothing".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<String>,
    /// false when one click cannot undo it (the run then needs confirm_irreversible).
    #[serde(default = "yes")]
    pub reversible: bool,
    /// How it was learned: the calls, what was seen (free text).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default)]
    pub learned_on: LearnedOn,
    #[serde(default)]
    pub runs: Runs,
    /// active, stale or retired.
    #[serde(default = "active")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
}
fn yes() -> bool {
    true
}
fn active() -> String {
    "active".into()
}

fn invalid(msg: impl Into<String>) -> crate::error::Error {
    crate::error::Error::new("invalid_skill", msg)
}

pub fn valid_name(name: &str) -> bool {
    (3..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn is_param(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// `{name}` placeholders in a step's strings.
fn placeholders(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            let mut rest = s.as_str();
            while let Some(a) = rest.find('{') {
                let Some(b) = rest[a..].find('}') else { break };
                let inner = &rest[a + 1..a + b];
                if is_param(inner) {
                    out.push(inner.to_string());
                }
                rest = &rest[a + b + 1..];
            }
        }
        Value::Array(a) => a.iter().for_each(|x| placeholders(x, out)),
        Value::Object(o) => o.values().for_each(|x| placeholders(x, out)),
        _ => {}
    }
}

impl Skill {
    /// Refusals a small model can act on: what is wrong and what to write instead.
    pub fn validate(&self) -> crate::error::Result<()> {
        if !valid_name(&self.name) {
            return Err(invalid(
                "name: 3-64 characters, lower-case letters, digits and _ (e.g. buy_water_caps)",
            ));
        }
        if self.purpose.trim().is_empty() || self.purpose.len() > 400 {
            return Err(invalid("purpose: one sentence, at most 400 characters"));
        }
        if self.steps.is_empty() || self.steps.len() > MAX_STEPS {
            return Err(invalid(format!("steps: 1 to {MAX_STEPS} tool calls")));
        }
        let mut used = Vec::new();
        for (i, s) in self.steps.iter().enumerate() {
            if !step_tool_allowed(&s.tool) {
                return Err(invalid(format!(
                    "steps[{i}].tool {:?} cannot be a skill step (use hoi4_ui_act or a typed tool)",
                    s.tool
                )));
            }
            if s.args.contains_key("lease_token") || s.args.contains_key("request_id") {
                return Err(invalid(format!(
                    "steps[{i}].args: leave out lease_token and request_id (the run adds them)"
                )));
            }
            placeholders(&Value::Object(s.args.clone()), &mut used);
        }
        for p in &used {
            if !self.parameters.contains_key(p) {
                return Err(invalid(format!(
                    "steps use {{{p}}} but parameters does not name {p:?}"
                )));
            }
        }
        for (i, c) in self.verify.iter().enumerate() {
            let ok = match c.check.as_str() {
                "window_opens" | "window_closes" | "widget_changes" => c.id.is_some(),
                "read_changes" => {
                    c.tool.is_some()
                        && c.path.is_some()
                        && matches!(c.direction.as_deref(), None | Some("up" | "down" | "any"))
                }
                _ => false,
            };
            if !ok {
                return Err(invalid(format!(
                    "verify[{i}]: one of {{\"check\":\"window_opens\"|\"window_closes\"|\"widget_changes\",\"id\":...}} or {{\"check\":\"read_changes\",\"tool\":\"hoi4_economy\",\"path\":\"political_power\",\"direction\":\"down\"}}"
                )));
            }
        }
        if !matches!(self.status.as_str(), "active" | "stale" | "retired") {
            return Err(invalid("status: active, stale or retired"));
        }
        Ok(())
    }

    /// The steps with `{param}` replaced by the given arguments (every parameter is required).
    pub fn bind(&self, args: &Map<String, Value>) -> crate::error::Result<Vec<Step>> {
        for p in self.parameters.keys() {
            if !args.contains_key(p) {
                return Err(crate::error::Error::new(
                    "invalid_arguments",
                    format!(
                        "skill {} needs args.{p} ({})",
                        self.name, self.parameters[p]
                    ),
                ));
            }
        }
        if let Some(extra) = args.keys().find(|k| !self.parameters.contains_key(*k)) {
            return Err(crate::error::Error::new(
                "invalid_arguments",
                format!("skill {} has no parameter {extra:?}", self.name),
            ));
        }
        fn fill(v: &Value, args: &Map<String, Value>) -> Value {
            match v {
                Value::String(s) => {
                    // A string that is exactly one placeholder takes the argument's own type.
                    if let Some(inner) = s.strip_prefix('{').and_then(|x| x.strip_suffix('}')) {
                        if let Some(a) = args.get(inner) {
                            return a.clone();
                        }
                    }
                    let mut out = s.clone();
                    for (k, a) in args {
                        let text = a
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| a.to_string());
                        out = out.replace(&format!("{{{k}}}"), &text);
                    }
                    Value::String(out)
                }
                Value::Array(a) => Value::Array(a.iter().map(|x| fill(x, args)).collect()),
                Value::Object(o) => {
                    Value::Object(o.iter().map(|(k, x)| (k.clone(), fill(x, args))).collect())
                }
                other => other.clone(),
            }
        }
        Ok(self
            .steps
            .iter()
            .map(|s| Step {
                tool: s.tool.clone(),
                args: match fill(&Value::Object(s.args.clone()), args) {
                    Value::Object(o) => o,
                    _ => Map::new(),
                },
                note: s.note.clone(),
            })
            .collect())
    }

    /// Whether this skill may run on `now` (game version and mod set) and why not.
    pub fn runnable(&self, now: &LearnedOn, revalidate: bool) -> std::result::Result<(), String> {
        match self.status.as_str() {
            "retired" => {
                return Err(format!(
                    "retired: {}",
                    self.status_reason.clone().unwrap_or_default()
                ))
            }
            "stale" if !revalidate => {
                return Err(format!(
                    "stale: {} (fix the skill and save it again, or run with revalidate:true)",
                    self.status_reason.clone().unwrap_or_default()
                ))
            }
            _ => {}
        }
        let other = (!self.learned_on.game_version.is_empty()
            && self.learned_on.game_version != now.game_version)
            || (!self.learned_on.mod_set.is_empty() && self.learned_on.mod_set != now.mod_set);
        if other && !revalidate {
            return Err(format!(
                "learned on game {} with mod set {} but this is game {} with mod set {}: run with revalidate:true once to check it here",
                self.learned_on.game_version, self.learned_on.mod_set, now.game_version, now.mod_set
            ));
        }
        Ok(())
    }
}

/// Where the store lives: `HOI4_LIVE_LEARNED`, else next to the run folders (`<install>/learned`
/// when the run folder sits in `<install>/runs`, else `<run folder>/../learned`).
pub fn store_dir(run_dir: &Path) -> PathBuf {
    if let Ok(d) = std::env::var("HOI4_LIVE_LEARNED") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    let parent = run_dir.parent().unwrap_or(run_dir);
    if parent.file_name().is_some_and(|n| n == "runs") {
        parent.parent().unwrap_or(parent).join("learned")
    } else {
        parent.join("learned")
    }
}

fn io(e: std::io::Error) -> crate::error::Error {
    crate::error::Error::new("skill_store_unavailable", e.to_string())
}

pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Store { dir }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.join("skills").join(format!("{name}.json"))
    }
    fn write(&self, path: &Path, value: &Value) -> crate::error::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap_or(&self.dir)).map_err(io)?;
        let tmp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(value).unwrap_or_default() + "\n";
        std::fs::write(&tmp, text).map_err(io)?;
        std::fs::rename(&tmp, path).map_err(io)
    }
    pub fn load(&self, name: &str) -> crate::error::Result<Skill> {
        if !valid_name(name) {
            return Err(invalid("name: lower-case letters, digits and _"));
        }
        let text = std::fs::read_to_string(self.path(name)).map_err(|_| {
            crate::error::Error::new(
                "skill_not_found",
                format!("No skill {name:?}; hoi4_skill_list shows the saved ones"),
            )
        })?;
        serde_json::from_str(&text).map_err(|e| {
            invalid(format!(
                "{} is not a valid skill file ({e}); fix it by hand or save it again",
                self.path(name).display()
            ))
        })
    }
    pub fn save(&self, skill: &Skill, replace: bool) -> crate::error::Result<PathBuf> {
        skill.validate()?;
        let path = self.path(&skill.name);
        if path.exists() && !replace {
            return Err(crate::error::Error::new(
                "skill_exists",
                format!(
                    "A skill {:?} exists; pass replace:true to overwrite it",
                    skill.name
                ),
            ));
        }
        self.write(&path, &serde_json::to_value(skill).unwrap_or(Value::Null))?;
        Ok(path)
    }
    /// Every readable skill (files that do not parse are listed with their error).
    pub fn list(&self) -> Vec<std::result::Result<Skill, (String, String)>> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(self.dir.join("skills")) else {
            return out;
        };
        let mut names: Vec<String> = entries
            .flatten()
            .filter_map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.strip_suffix(".json").map(str::to_string)
            })
            .collect();
        names.sort();
        for n in names {
            out.push(self.load(&n).map_err(|e| (n.clone(), e.message)));
        }
        out
    }
    pub fn set_status(
        &self,
        name: &str,
        status: &str,
        reason: &str,
    ) -> crate::error::Result<Skill> {
        let mut s = self.load(name)?;
        s.status = status.into();
        s.status_reason = Some(reason.chars().take(300).collect());
        self.write(
            &self.path(name),
            &serde_json::to_value(&s).unwrap_or(Value::Null),
        )?;
        Ok(s)
    }
    /// Count one run; a successful revalidation moves the skill to `now` and makes it active.
    pub fn record_run(
        &self,
        name: &str,
        ok: bool,
        error: Option<&str>,
        now: &LearnedOn,
        unix: u64,
    ) -> crate::error::Result<Skill> {
        let mut s = self.load(name)?;
        if ok {
            s.runs.ok += 1;
            s.runs.last_error = None;
            if s.status == "stale"
                || s.learned_on.game_version != now.game_version
                || s.learned_on.mod_set != now.mod_set
            {
                s.status = "active".into();
                s.status_reason = None;
                s.learned_on = now.clone();
            }
        } else {
            s.runs.failed += 1;
            s.runs.last_error = error.map(|e| e.chars().take(300).collect());
        }
        s.runs.last_run_unix = Some(unix);
        self.write(
            &self.path(name),
            &serde_json::to_value(&s).unwrap_or(Value::Null),
        )?;
        Ok(s)
    }

    pub fn facts(&self) -> Vec<Value> {
        std::fs::read_to_string(self.dir.join("facts.json"))
            .ok()
            .and_then(|t| serde_json::from_str::<Vec<Value>>(&t).ok())
            .unwrap_or_default()
    }
    pub fn add_fact(
        &self,
        statement: &str,
        source: &str,
        now: &LearnedOn,
        unix: u64,
    ) -> crate::error::Result<Value> {
        let statement = statement.trim();
        if statement.is_empty()
            || statement.len() > 500
            || source.trim().is_empty()
            || source.len() > 300
        {
            return Err(crate::error::Error::new(
                "invalid_arguments",
                "statement (at most 500 characters) and source (a file path or what was seen, at most 300) are both required",
            ));
        }
        let mut facts = self.facts();
        if let Some(same) = facts.iter().find(|f| f["statement"] == statement) {
            return Ok(same.clone());
        }
        let id = facts
            .iter()
            .filter_map(|f| f["id"].as_u64())
            .max()
            .unwrap_or(0)
            + 1;
        let fact = json!({"id": id, "statement": statement, "source": source.trim(),
            "game_version": now.game_version, "mod_set": now.mod_set, "mods": now.mods, "saved_unix": unix});
        facts.push(fact.clone());
        self.write(&self.dir.join("facts.json"), &Value::Array(facts))?;
        Ok(fact)
    }
}

/// A stable id of a mod set: FNV-1a over the mods' names and versions in load order.
pub fn mod_set_id(mods: &[(String, String)]) -> String {
    if mods.is_empty() {
        return "plain".into();
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for (name, version) in mods {
        for b in name
            .bytes()
            .chain([0u8])
            .chain(version.bytes())
            .chain([1u8])
        {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill() -> Skill {
        serde_json::from_value(json!({
            "name": "buy_water",
            "purpose": "Buy water for caps in the mod's trade panel",
            "parameters": {"amount": "how many units (1-10)"},
            "requires": {"widgets": ["topbar/trade_button"]},
            "steps": [
                {"tool": "hoi4_ui_act", "args": {"widget": "topbar/trade_button", "action": "click"}},
                {"tool": "hoi4_ui_act", "args": {"widget": "caps_window/buy_button", "action": "click", "times": "{amount}"}}
            ],
            "verify": [{"check": "read_changes", "tool": "hoi4_economy", "path": "political_power", "direction": "down"}],
            "cost": "25 political power",
            "reversible": false
        }))
        .unwrap()
    }

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hoi4-learn-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn a_skill_file_round_trips_and_binds_its_parameters() {
        let store = Store::new(tmp("rt"));
        let s = skill();
        let path = store.save(&s, false).unwrap();
        assert!(path.ends_with("skills/buy_water.json"));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"purpose\""), "readable JSON on disk");
        assert_eq!(store.load("buy_water").unwrap(), s);
        assert_eq!(store.save(&s, false).unwrap_err().code, "skill_exists");
        let steps = s
            .bind(&Map::from_iter([("amount".to_string(), json!(3))]))
            .unwrap();
        assert_eq!(
            steps[1].args["times"],
            json!(3),
            "a whole-string placeholder keeps the type"
        );
        assert_eq!(s.bind(&Map::new()).unwrap_err().code, "invalid_arguments");
        let _ = std::fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn validation_refuses_with_what_to_write() {
        let mut s = skill();
        s.name = "Buy Water".into();
        assert!(s.validate().unwrap_err().message.contains("lower-case"));
        let mut s = skill();
        s.steps[0].tool = "hoi4_skill_run".into();
        assert!(s.validate().is_err());
        let mut s = skill();
        s.parameters.clear();
        assert!(s.validate().unwrap_err().message.contains("{amount}"));
        let mut s = skill();
        s.steps[0].args.insert("lease_token".into(), json!("x"));
        assert!(s.validate().is_err());
        let mut s = skill();
        s.verify[0].direction = Some("sideways".into());
        assert!(s.validate().is_err());
    }

    #[test]
    fn stale_retired_and_other_builds_are_refused_unless_revalidated() {
        let store = Store::new(tmp("st"));
        let mut s = skill();
        s.learned_on = LearnedOn {
            game_version: "1.19.4.1".into(),
            mod_set: "plain".into(),
            mods: vec![],
        };
        store.save(&s, false).unwrap();
        let here = s.learned_on.clone();
        assert!(s.runnable(&here, false).is_ok());
        let modded = LearnedOn {
            mod_set: mod_set_id(&[("Old World Blues".into(), "1".into())]),
            ..here.clone()
        };
        assert!(s
            .runnable(&modded, false)
            .unwrap_err()
            .contains("revalidate"));
        assert!(s.runnable(&modded, true).is_ok());
        let stale = store
            .set_status(
                "buy_water",
                "stale",
                "widget caps_window/buy_button missing",
            )
            .unwrap();
        assert!(stale
            .runnable(&here, false)
            .unwrap_err()
            .starts_with("stale"));
        // A successful revalidation run makes it active on the new mod set.
        let fixed = store
            .record_run("buy_water", true, None, &modded, 1)
            .unwrap();
        assert_eq!((fixed.status.as_str(), fixed.runs.ok), ("active", 1));
        assert_eq!(fixed.learned_on.mod_set, modded.mod_set);
        let failed = store
            .record_run("buy_water", false, Some("widget_not_found"), &modded, 2)
            .unwrap();
        assert_eq!(failed.runs.failed, 1);
        let retired = store
            .set_status("buy_water", "retired", "the mod removed the panel")
            .unwrap();
        assert!(retired
            .runnable(&modded, true)
            .unwrap_err()
            .starts_with("retired"));
        assert_eq!(store.list().len(), 1);
        let _ = std::fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn facts_are_kept_once_with_their_scope() {
        let store = Store::new(tmp("fa"));
        let now = LearnedOn {
            game_version: "1.19.4.1".into(),
            mod_set: "plain".into(),
            mods: vec![],
        };
        let f = store
            .add_fact(
                "Water is bought in the caps panel",
                "common/scripted_guis/caps.txt",
                &now,
                5,
            )
            .unwrap();
        assert_eq!(f["id"], 1);
        store
            .add_fact("Water is bought in the caps panel", "again", &now, 6)
            .unwrap();
        assert_eq!(store.facts().len(), 1);
        assert!(store.add_fact("", "x", &now, 7).is_err());
        let _ = std::fs::remove_dir_all(&store.dir);
    }

    #[test]
    fn store_sits_beside_the_runs_and_mod_sets_have_stable_ids() {
        std::env::remove_var("HOI4_LIVE_LEARNED");
        assert_eq!(
            store_dir(Path::new("/i/runs/2026-ITA")),
            PathBuf::from("/i/learned")
        );
        assert_eq!(store_dir(Path::new("/x/RUN")), PathBuf::from("/x/learned"));
        assert_eq!(mod_set_id(&[]), "plain");
        let a = mod_set_id(&[("A".into(), "1".into()), ("B".into(), "".into())]);
        assert_eq!(
            a,
            mod_set_id(&[("A".into(), "1".into()), ("B".into(), "".into())])
        );
        assert_ne!(
            a,
            mod_set_id(&[("B".into(), "".into()), ("A".into(), "1".into())])
        );
    }
}
