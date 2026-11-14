//! Whether a national focus can be started now, decided from the game's own files and the state
//! the server already reads, without opening the focus tree.
//!
//! The rules come from `common/national_focus` of the running game's content (the install with
//! the enabled mods layered over it), read with the shared script reader: each focus's
//! `prerequisite` groups (one member of every group must be completed), its `mutually_exclusive`
//! focuses, and its `available`, `bypass` and `allow_branch` trigger blocks. Scripted triggers
//! (`common/scripted_triggers`) are expanded. Nothing is tied to a country or to the base game.
//!
//! A focus can be started when it is not completed, every prerequisite group has a completed
//! member, no focus it excludes is completed, its branch is loaded, and `available` or `bypass`
//! holds (a focus whose bypass holds is taken at once, without its reward).
//!
//! Every trigger is judged with three values. `Unavailable` is only said when it is certain;
//! a trigger the server cannot read (a country flag, an idea, the ruling ideology, another
//! country's affairs, any `any_`/`all_` scope) makes the answer `Unknown`, and the caller may then
//! look at the game once. `allow_branch` is judged by the game when the tree is laid out, not
//! every day: only its static part (tag, DLC) can make a focus certainly unavailable.
//!
//! What can be judged: `tag`, `original_tag`, `has_dlc`, `always`, `date`, `exists`,
//! `has_global_flag` and the player's `has_country_flag` (only when the facts name the flag as set),
//! `country_exists`, `has_war`, `has_war_with`, the war-only triggers while at peace,
//! `has_completed_focus`, `has_government` (when the ruling ideology is known), `is_subject`,
//! `is_subject_of`, `has_subject`, `is_puppet` (when no overlord), `is_in_faction`,
//! `is_in_faction_with`, `is_ai`, `owns_state`, `controls_state`, `has_full_control_of_state`,
//! `has_tech`, the numeric triggers `has_stability`, `has_war_support`, `has_political_power`,
//! `has_manpower`, `threat`, `num_of_military_factories`, `num_of_civilian_factories`,
//! `num_of_naval_factories`; inside a state scope `is_owned_by`, `is_controlled_by`,
//! `is_fully_controlled_by`, `is_owned_and_controlled_by`, `state`, `owner`, `controller`; the
//! scopes `TAG = { }`, `ROOT`, `THIS`, `PREV`, `<state id> = { }`; and `AND`, `OR`, `NOT`,
//! `if`/`else_if`/`else`, `count_triggers`, `hidden_trigger`, `custom_trigger_tooltip`,
//! `custom_override_tooltip`.
use crate::plan_predicate::Truth;
use crate::script::{self, Block as _, Entry, Val};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

pub mod live;

/// Scripted triggers are expanded this many levels deep.
const SCRIPTED_DEPTH: u32 = 4;
/// Reasons listed per focus.
const REASONS: usize = 6;
/// Characters of one rendered trigger.
const RENDER: usize = 120;

/// The rules of one focus.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Focus {
    pub id: String,
    /// Each group needs one completed member.
    pub prerequisites: Vec<Vec<String>>,
    pub exclusive: Vec<String>,
    available: Vec<Vec<Entry>>,
    bypass: Vec<Vec<Entry>>,
    allow_branch: Vec<Vec<Entry>>,
}

/// Every focus of the game content, by id.
#[derive(Debug, Default)]
pub struct Catalog {
    focuses: HashMap<String, Focus>,
    /// Ids defined more than once with different rules: which one the game keeps is not known.
    ambiguous: HashSet<String>,
    /// focus -> the focuses that list it as mutually exclusive.
    excluded_by: HashMap<String, Vec<String>>,
    scripted: HashMap<String, Vec<Entry>>,
}

fn scalars(block: &[Entry], key: &str) -> Vec<String> {
    block
        .iter()
        .filter(|e| e.key == key)
        .filter_map(|e| e.val.scalar())
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect()
}

fn blocks(block: &[Entry], key: &str) -> Vec<Vec<Entry>> {
    block
        .iter()
        .filter(|e| e.key == key)
        .filter_map(|e| match &e.val {
            Val::Block(inner) => Some(inner.clone()),
            Val::Scalar(_) => None,
        })
        .collect()
}

impl Catalog {
    /// Add the focus trees and shared focuses of one `common/national_focus` file.
    pub fn add_focus_file(&mut self, text: &str) {
        for top in script::parse(text) {
            match top.key.as_str() {
                "focus_tree" => {
                    for entry in top.val.block().iter().filter(|e| e.key == "focus") {
                        self.add_focus(entry.val.block());
                    }
                }
                "shared_focus" | "joint_focus" => self.add_focus(top.val.block()),
                _ => {}
            }
        }
    }
    /// Add the scripted triggers of one `common/scripted_triggers` file.
    pub fn add_scripted_file(&mut self, text: &str) {
        for top in script::parse(text) {
            if let Val::Block(body) = top.val {
                if !top.key.is_empty() {
                    self.scripted.insert(top.key, body);
                }
            }
        }
    }
    fn add_focus(&mut self, body: &[Entry]) {
        let Some(id) = body.text("id").filter(|id| !id.is_empty()) else {
            return;
        };
        let focus = Focus {
            id: id.to_string(),
            prerequisites: blocks(body, "prerequisite")
                .iter()
                .map(|group| scalars(group, "focus"))
                .filter(|group| !group.is_empty())
                .collect(),
            exclusive: blocks(body, "mutually_exclusive")
                .iter()
                .flat_map(|group| scalars(group, "focus"))
                .collect(),
            available: blocks(body, "available"),
            bypass: blocks(body, "bypass"),
            allow_branch: blocks(body, "allow_branch"),
        };
        match self.focuses.get(id) {
            Some(known) if *known != focus => {
                self.ambiguous.insert(id.to_string());
            }
            Some(_) => {}
            None => {
                for other in &focus.exclusive {
                    self.excluded_by
                        .entry(other.clone())
                        .or_default()
                        .push(focus.id.clone());
                }
                self.focuses.insert(id.to_string(), focus);
            }
        }
    }
    /// The catalogue of a game content folder (`common/national_focus` and
    /// `common/scripted_triggers` below it), read once per folder.
    pub fn load(dir: &Path) -> Arc<Catalog> {
        script::cached(dir, "focus_rules", |dir| {
            let mut catalog = Catalog::default();
            for (folder, focus) in [
                ("common/national_focus", true),
                ("common/scripted_triggers", false),
            ] {
                let Ok(entries) = std::fs::read_dir(dir.join(folder)) else {
                    continue;
                };
                let mut files: Vec<_> = entries
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "txt"))
                    .collect();
                files.sort();
                for file in files {
                    let Ok(bytes) = std::fs::read(&file) else {
                        continue;
                    };
                    let text = String::from_utf8_lossy(&bytes);
                    if focus {
                        catalog.add_focus_file(&text);
                    } else {
                        catalog.add_scripted_file(&text);
                    }
                }
            }
            catalog
        })
    }
    pub fn len(&self) -> usize {
        self.focuses.len()
    }
    pub fn is_empty(&self) -> bool {
        self.focuses.is_empty()
    }
    pub fn focus(&self, id: &str) -> Option<&Focus> {
        self.focuses.get(id)
    }
}

/// Who holds a state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Holders {
    pub owner: Option<String>,
    pub controller: Option<String>,
    /// `Some(Some(tag))`: one country holds every province; `Some(None)`: several do.
    pub full_controller: Option<Option<String>>,
}

/// What is known about the player's country right now. Every answer may be unknown.
pub trait Facts {
    /// The player's tag.
    fn player(&self) -> Option<String>;
    /// The game date as (year, month, day).
    fn date(&self) -> Option<(i64, i64, i64)>;
    fn dlc(&self, name: &str) -> Truth;
    /// The player completed the focus.
    fn completed(&self, focus: &str) -> Truth;
    /// The player's loaded focus tree holds the focus.
    fn in_tree(&self, focus: &str) -> Truth;
    /// The player is at war with anyone.
    fn at_war(&self) -> Truth;
    /// The player is at war with `tag`.
    fn at_war_with(&self, tag: &str) -> Truth;
    /// The country owns at least one state.
    fn exists(&self, tag: &str) -> Truth;
    fn state(&self, id: i64) -> Option<Holders>;
    /// A number of the player's country with the margin its reading may be off by: `stability`,
    /// `war_support`, `world_tension` (fractions), `political_power`, `manpower`,
    /// `military_factories`, `civilian_factories`, `naval_factories`.
    fn number(&self, name: &str) -> Option<(f64, f64)>;
    /// The player researched the technology.
    fn tech(&self, key: &str) -> Truth;
    /// The other members of the player's faction (empty: in no faction).
    fn faction(&self) -> Option<Vec<String>>;
    /// The player's overlord (`Some(None)`: none).
    fn overlord(&self) -> Option<Option<String>>;
    fn subjects(&self) -> Option<Vec<String>>;
    /// The player's ruling ideology group (`fascism`, `democratic`, ...).
    fn government(&self) -> Option<String>;
    /// A global flag, or a country flag of the player, is set. Nothing reads flags from the
    /// game; a plan can know the ones its start sets.
    fn flag(&self, _name: &str) -> Truth {
        Truth::Unknown
    }
}

/// Only what never changes during a game: the tag and the DLCs.
struct Static<'a>(&'a dyn Facts);
impl Facts for Static<'_> {
    fn player(&self) -> Option<String> {
        self.0.player()
    }
    fn date(&self) -> Option<(i64, i64, i64)> {
        None
    }
    fn dlc(&self, name: &str) -> Truth {
        self.0.dlc(name)
    }
    fn completed(&self, _: &str) -> Truth {
        Truth::Unknown
    }
    fn in_tree(&self, _: &str) -> Truth {
        Truth::Unknown
    }
    fn at_war(&self) -> Truth {
        Truth::Unknown
    }
    fn at_war_with(&self, _: &str) -> Truth {
        Truth::Unknown
    }
    fn exists(&self, _: &str) -> Truth {
        Truth::Unknown
    }
    fn state(&self, _: i64) -> Option<Holders> {
        None
    }
    fn number(&self, _: &str) -> Option<(f64, f64)> {
        None
    }
    fn tech(&self, _: &str) -> Truth {
        Truth::Unknown
    }
    fn faction(&self) -> Option<Vec<String>> {
        None
    }
    fn overlord(&self) -> Option<Option<String>> {
        None
    }
    fn subjects(&self) -> Option<Vec<String>> {
        None
    }
    fn government(&self) -> Option<String> {
        None
    }
}

/// Facts given as JSON (tests, fixtures, offline checks). A key that is absent is unknown; a
/// list that is present is complete:
/// `{"tag","date":"Y.M.D","dlcs":[..],"completed":[..],"tree":[..],"wars":[..],
///   "maybe_wars":[tags that may or may not be at war with the player; "*" is any other country],
///   "flags":[global and own country flags known to be set],"exists":{"TAG":bool},"states":{"<id>":{"owner","controller","full_controller"}},
///   "numbers":{"stability":0.5},"techs":[..],"faction":[..],"overlord":"TAG"|null,
///   "subjects":[..],"government":"fascism"}`
pub struct Snapshot(pub Value);
impl Snapshot {
    fn member(&self, key: &str, item: &str) -> Truth {
        match self.0[key].as_array() {
            Some(list) => Truth::from_bool(list.iter().any(|v| v.as_str() == Some(item))),
            None => Truth::Unknown,
        }
    }
    fn list(&self, key: &str) -> Option<Vec<String>> {
        Some(
            self.0[key]
                .as_array()?
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        )
    }
}
impl Facts for Snapshot {
    fn player(&self) -> Option<String> {
        self.0["tag"].as_str().map(str::to_string)
    }
    fn date(&self) -> Option<(i64, i64, i64)> {
        date(self.0["date"].as_str()?)
    }
    fn dlc(&self, name: &str) -> Truth {
        self.member("dlcs", name)
    }
    fn completed(&self, focus: &str) -> Truth {
        self.member("completed", focus)
    }
    fn in_tree(&self, focus: &str) -> Truth {
        self.member("tree", focus)
    }
    fn at_war(&self) -> Truth {
        match self.0["wars"].as_array() {
            Some(list) if !list.is_empty() => Truth::True,
            Some(_)
                if self.0["maybe_wars"]
                    .as_array()
                    .is_some_and(|m| !m.is_empty()) =>
            {
                Truth::Unknown
            }
            Some(_) => Truth::False,
            None => Truth::Unknown,
        }
    }
    fn at_war_with(&self, tag: &str) -> Truth {
        match self.member("wars", tag) {
            Truth::False
                if self.member("maybe_wars", tag) == Truth::True
                    || self.member("maybe_wars", "*") == Truth::True =>
            {
                Truth::Unknown
            }
            known => known,
        }
    }
    fn exists(&self, tag: &str) -> Truth {
        match self.0["exists"][tag].as_bool() {
            Some(b) => Truth::from_bool(b),
            None => Truth::Unknown,
        }
    }
    fn state(&self, id: i64) -> Option<Holders> {
        let row = self.0["states"].get(id.to_string())?;
        let text = |key: &str| row[key].as_str().map(str::to_string);
        Some(Holders {
            owner: text("owner"),
            controller: text("controller"),
            full_controller: match row.get("full_controller") {
                Some(Value::String(tag)) => Some(Some(tag.clone())),
                Some(Value::Null) => Some(None),
                _ => None,
            },
        })
    }
    fn number(&self, name: &str) -> Option<(f64, f64)> {
        self.0["numbers"][name].as_f64().map(|v| (v, 0.0))
    }
    fn tech(&self, key: &str) -> Truth {
        self.member("techs", key)
    }
    fn faction(&self) -> Option<Vec<String>> {
        self.list("faction")
    }
    fn overlord(&self) -> Option<Option<String>> {
        match self.0.get("overlord")? {
            Value::String(tag) => Some(Some(tag.clone())),
            Value::Null => Some(None),
            _ => None,
        }
    }
    fn subjects(&self) -> Option<Vec<String>> {
        self.list("subjects")
    }
    fn government(&self) -> Option<String> {
        self.0["government"].as_str().map(str::to_string)
    }
    fn flag(&self, name: &str) -> Truth {
        // Flags known to be set; one that is not listed may be set all the same.
        match self.member("flags", name) {
            Truth::True => Truth::True,
            _ => Truth::Unknown,
        }
    }
}

/// "1936.1.1" (dots or dashes) as (year, month, day).
pub fn date(text: &str) -> Option<(i64, i64, i64)> {
    let mut parts = text.trim().split(['.', '-']);
    let mut next = || parts.next()?.trim().parse::<i64>().ok();
    Some((next()?, next()?, next()?))
}

/// The answer for one focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Available,
    Unavailable,
    Unknown,
}
impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Available => "available",
            State::Unavailable => "unavailable",
            State::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    pub state: State,
    /// Why it cannot start (when unavailable).
    pub why: Vec<String>,
    /// What could not be judged (when unknown).
    pub unknown: Vec<String>,
    /// Its bypass holds: taking it completes it at once without the reward.
    pub bypass: bool,
}
impl Verdict {
    pub fn json(&self, focus: &str) -> Value {
        let mut out = json!({"focus": focus, "state": self.state.label()});
        if !self.why.is_empty() {
            out["why"] = json!(self.why);
        }
        if !self.unknown.is_empty() {
            out["unknown"] = json!(self.unknown);
        }
        if self.bypass {
            out["bypass"] = json!(true);
        }
        out
    }
}

/// A truth with what stood in the way of knowing it.
#[derive(Clone, Debug)]
struct Out {
    truth: Truth,
    notes: Vec<String>,
}
impl Out {
    fn known(value: bool) -> Self {
        Self {
            truth: Truth::from_bool(value),
            notes: Vec::new(),
        }
    }
    fn of(truth: Truth, note: impl FnOnce() -> String) -> Self {
        Self {
            notes: if truth == Truth::Unknown {
                vec![note()]
            } else {
                Vec::new()
            },
            truth,
        }
    }
    fn unknown(note: String) -> Self {
        Self {
            truth: Truth::Unknown,
            notes: vec![note],
        }
    }
    fn not(self) -> Self {
        Self {
            truth: !self.truth,
            notes: self.notes,
        }
    }
    /// `truth == want` for a yes/no trigger.
    fn wanted(self, want: bool) -> Self {
        if want {
            self
        } else {
            self.not()
        }
    }
}

fn all(parts: Vec<Out>) -> Out {
    if parts.iter().any(|p| p.truth == Truth::False) {
        return Out::known(false);
    }
    let notes: Vec<String> = parts.into_iter().flat_map(|p| p.notes).collect();
    Out {
        truth: if notes.is_empty() {
            Truth::True
        } else {
            Truth::Unknown
        },
        notes,
    }
}

fn any(parts: Vec<Out>) -> Out {
    if parts.iter().any(|p| p.truth == Truth::True) {
        return Out::known(true);
    }
    let notes: Vec<String> = parts.into_iter().flat_map(|p| p.notes).collect();
    Out {
        truth: if notes.is_empty() {
            Truth::False
        } else {
            Truth::Unknown
        },
        notes,
    }
}

/// `limit ? then : otherwise`; an unknown limit still decides when both sides agree.
fn choose(limit: Out, then: Out, otherwise: Out) -> Out {
    match limit.truth {
        Truth::True => then,
        Truth::False => otherwise,
        Truth::Unknown if then.truth == otherwise.truth && then.truth != Truth::Unknown => then,
        Truth::Unknown => {
            let mut notes = limit.notes;
            notes.extend(then.notes);
            notes.extend(otherwise.notes);
            Out {
                truth: Truth::Unknown,
                notes,
            }
        }
    }
}

fn yes_no(value: &str) -> Option<bool> {
    match value {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

fn is_tag(key: &str) -> bool {
    let b = key.as_bytes();
    b.len() == 3
        && b[0].is_ascii_uppercase()
        && b.iter()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// A tag the game makes up for a civil-war side or a released copy (D01, D02, ...).
fn is_dynamic_tag(tag: &str) -> bool {
    let b = tag.as_bytes();
    b.len() == 3 && b[0] == b'D' && b[1].is_ascii_digit() && b[2].is_ascii_digit()
}

/// One line of script for an entry, cut to a bound.
fn render(entry: &Entry) -> String {
    fn walk(entry: &Entry, out: &mut String) {
        match &entry.val {
            Val::Block(inner) => {
                if !entry.key.is_empty() {
                    out.push_str(&entry.key);
                    out.push_str(" = ");
                }
                out.push_str("{ ");
                for e in inner {
                    if out.len() > RENDER {
                        break;
                    }
                    walk(e, out);
                    out.push(' ');
                }
                out.push('}');
            }
            Val::Scalar(_) if entry.op.is_empty() => out.push_str(&entry.key),
            Val::Scalar(v) => {
                out.push_str(&entry.key);
                out.push(' ');
                out.push_str(&entry.op);
                out.push(' ');
                out.push_str(v);
            }
        }
    }
    let mut out = String::new();
    walk(entry, &mut out);
    if out.len() > RENDER {
        let mut cut = RENDER - 3;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out.push_str("...");
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
enum Scope {
    Country(String),
    State(i64),
}

struct Eval<'a> {
    catalog: &'a Catalog,
    facts: &'a dyn Facts,
    me: Option<String>,
}

impl Eval<'_> {
    /// The truths of a block's items, each with its script text. An `if` and the `else_if` /
    /// `else` that follow it (inside it or after it) count as one item; labels are left out.
    fn items(&self, entries: &[Entry], scopes: &mut Vec<Scope>, depth: u32) -> Vec<(String, Out)> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < entries.len() {
            let entry = &entries[i];
            i += 1;
            let key = entry.key.to_ascii_lowercase();
            if key == "tooltip" {
                continue;
            }
            if key == "if" {
                let mut chain: Vec<&Entry> = vec![entry];
                while let Some(next) = entries.get(i) {
                    let k = next.key.to_ascii_lowercase();
                    if k != "else_if" && k != "else" {
                        break;
                    }
                    chain.push(next);
                    i += 1;
                }
                out.push((render(entry), self.conditional(&chain, scopes, depth)));
                continue;
            }
            out.push((render(entry), self.item(entry, &key, scopes, depth)));
        }
        out
    }
    fn block(&self, entries: &[Entry], scopes: &mut Vec<Scope>, depth: u32) -> Out {
        all(self
            .items(entries, scopes, depth)
            .into_iter()
            .map(|(_, o)| o)
            .collect())
    }
    /// An `if` with its `else_if` / `else` branches, written inside it or after it.
    fn conditional(&self, chain: &[&Entry], scopes: &mut Vec<Scope>, depth: u32) -> Out {
        // (limit, body) per branch in order; the last `else` has no limit.
        let mut branches: Vec<(Option<Vec<Entry>>, Vec<Entry>)> = Vec::new();
        let mut add = |entry: &Entry, nested: &mut Vec<Entry>| {
            let is_else = entry.key.eq_ignore_ascii_case("else");
            let mut limit = Vec::new();
            let mut body = Vec::new();
            for e in entry.val.block() {
                let k = e.key.to_ascii_lowercase();
                match k.as_str() {
                    "limit" if !is_else => limit.extend(e.val.block().iter().cloned()),
                    "else_if" | "else" => nested.push(e.clone()),
                    _ => body.push(e.clone()),
                }
            }
            branches.push(((!is_else).then_some(limit), body));
        };
        let mut pending: Vec<Entry> = chain.iter().map(|e| (*e).clone()).collect();
        pending.reverse();
        while let Some(entry) = pending.pop() {
            let mut nested = Vec::new();
            add(&entry, &mut nested);
            // Branches written inside come before the ones written after.
            nested.reverse();
            pending.extend(nested);
        }
        let mut result = Out::known(true);
        for (limit, body) in branches.into_iter().rev() {
            let then = self.block(&body, scopes, depth);
            result = match limit {
                None => then,
                Some(limit) => choose(self.block(&limit, scopes, depth), then, result),
            };
        }
        result
    }
    /// The country a trigger value names (`ROOT`, `THIS`, `PREV` or a tag).
    fn tag_of(&self, value: &str, scopes: &[Scope]) -> Option<String> {
        match value.to_ascii_uppercase().as_str() {
            "ROOT" => self.me.clone(),
            "THIS" => match scopes.last() {
                Some(Scope::Country(tag)) => Some(tag.clone()),
                _ => None,
            },
            "PREV" => match scopes.len().checked_sub(2).and_then(|i| scopes.get(i)) {
                Some(Scope::Country(tag)) => Some(tag.clone()),
                _ => None,
            },
            _ if is_tag(value) => Some(value.to_string()),
            _ => None,
        }
    }
    fn scoped(&self, scope: Scope, body: &[Entry], scopes: &mut Vec<Scope>, depth: u32) -> Out {
        scopes.push(scope);
        let out = self.block(body, scopes, depth);
        scopes.pop();
        out
    }
    fn item(&self, entry: &Entry, key: &str, scopes: &mut Vec<Scope>, depth: u32) -> Out {
        let unknown = || Out::unknown(render(entry));
        let body = match &entry.val {
            Val::Block(body) => body,
            Val::Scalar(value) => return self.leaf(entry, key, value, scopes, depth),
        };
        match key {
            ""
            | "and"
            | "hidden_trigger"
            | "custom_trigger_tooltip"
            | "custom_override_tooltip" => self.block(body, scopes, depth),
            "or" => any(self
                .items(body, scopes, depth)
                .into_iter()
                .map(|(_, o)| o)
                .collect()),
            "not" => any(self
                .items(body, scopes, depth)
                .into_iter()
                .map(|(_, o)| o)
                .collect())
            .not(),
            "count_triggers" => {
                let Some(amount) = body.number("amount") else {
                    return unknown();
                };
                let rest: Vec<Entry> = body.iter().filter(|e| e.key != "amount").cloned().collect();
                let parts = self.items(&rest, scopes, depth);
                let yes = parts.iter().filter(|(_, o)| o.truth == Truth::True).count() as f64;
                let maybe = parts
                    .iter()
                    .filter(|(_, o)| o.truth == Truth::Unknown)
                    .count() as f64;
                if yes >= amount {
                    Out::known(true)
                } else if yes + maybe < amount {
                    Out::known(false)
                } else {
                    Out {
                        truth: Truth::Unknown,
                        notes: parts.into_iter().flat_map(|(_, o)| o.notes).collect(),
                    }
                }
            }
            "root" => match self.me.clone() {
                Some(me) => self.scoped(Scope::Country(me), body, scopes, depth),
                None => unknown(),
            },
            "this" => self.block(body, scopes, depth),
            "prev" => match scopes.len().checked_sub(2).and_then(|i| scopes.get(i)) {
                Some(scope) => self.scoped(scope.clone(), body, scopes, depth),
                None => unknown(),
            },
            "owner" | "controller" => {
                let Some(Scope::State(id)) = scopes.last() else {
                    return unknown();
                };
                let holders = self.facts.state(*id).unwrap_or_default();
                let holder = if key == "owner" {
                    holders.owner
                } else {
                    holders.controller
                };
                match holder {
                    Some(tag) => self.scoped(Scope::Country(tag), body, scopes, depth),
                    None => unknown(),
                }
            }
            _ if is_tag(&entry.key) => {
                self.scoped(Scope::Country(entry.key.clone()), body, scopes, depth)
            }
            _ => match entry.key.parse::<i64>() {
                Ok(id) if id > 0 => self.scoped(Scope::State(id), body, scopes, depth),
                // any_*/all_* scopes, capital_scope, overlord, FROM, variables, event targets
                _ => unknown(),
            },
        }
    }
    fn leaf(
        &self,
        entry: &Entry,
        key: &str,
        value: &str,
        scopes: &mut Vec<Scope>,
        depth: u32,
    ) -> Out {
        let note = || render(entry);
        let unknown = || Out::unknown(render(entry));
        if key == "always" {
            return yes_no(value).map_or_else(unknown, Out::known);
        }
        if key == "has_dlc" {
            return Out::of(self.facts.dlc(value), note);
        }
        if key == "date" {
            let (Some(now), Some(when)) = (self.facts.date(), date(value)) else {
                return unknown();
            };
            return match entry.op.as_str() {
                ">" => Out::known(now > when),
                "<" => Out::known(now < when),
                ">=" => Out::known(now >= when),
                "<=" => Out::known(now <= when),
                _ => unknown(),
            };
        }
        if key == "threat" {
            return self.number(entry, "world_tension", value);
        }
        if key == "has_global_flag" {
            return Out::of(self.facts.flag(value), note);
        }
        if key == "country_exists" {
            return match self.tag_of(value, scopes) {
                Some(tag) => Out::of(self.facts.exists(&tag), note),
                None => unknown(),
            };
        }
        // A scripted trigger: its body in the same scope.
        if let (Some(body), Some(want)) = (self.catalog.scripted.get(&entry.key), yes_no(value)) {
            if depth >= SCRIPTED_DEPTH {
                return unknown();
            }
            return self.block(body, scopes, depth + 1).wanted(want);
        }
        match scopes.last() {
            Some(Scope::Country(tag)) => {
                let tag = tag.clone();
                self.country(entry, key, value, &tag, scopes)
            }
            Some(Scope::State(id)) => self.state(entry, key, value, *id, scopes),
            None => unknown(),
        }
    }
    /// A numeric trigger of the player's own country (`key > 0.5`).
    fn number(&self, entry: &Entry, name: &str, value: &str) -> Out {
        let unknown = || Out::unknown(render(entry));
        let (Some((have, margin)), Ok(want)) = (self.facts.number(name), value.parse::<f64>())
        else {
            return unknown();
        };
        // A reading that may be off by its margin cannot settle a comparison that close.
        if (have - want).abs() <= margin {
            return unknown();
        }
        match entry.op.as_str() {
            ">" | ">=" => Out::known(have > want),
            "<" | "<=" => Out::known(have < want),
            _ => unknown(),
        }
    }
    fn country(&self, entry: &Entry, key: &str, value: &str, tag: &str, scopes: &[Scope]) -> Out {
        let note = || render(entry);
        let unknown = || Out::unknown(render(entry));
        let own = self.me.as_deref() == Some(tag);
        let named = || self.tag_of(value, scopes);
        let want = yes_no(value);
        let member = |list: Option<Vec<String>>, who: &str| match list {
            Some(list) => Out::known(list.iter().any(|t| t == who)),
            None => unknown(),
        };
        match key {
            "tag" => named().map_or_else(unknown, |other| Out::known(other == tag)),
            "original_tag" => match named() {
                Some(other) if other == tag => Out::known(true),
                Some(other) if is_dynamic_tag(tag) || is_dynamic_tag(&other) => unknown(),
                Some(_) => Out::known(false),
                None => unknown(),
            },
            "exists" => match want {
                Some(want) => Out::of(self.facts.exists(tag), note).wanted(want),
                None => unknown(),
            },
            "has_war" => match want {
                Some(want) if own => Out::of(self.facts.at_war(), note).wanted(want),
                // A country the player fights is at war; anything else about it is not read.
                Some(want) if self.facts.at_war_with(tag) == Truth::True => Out::known(want),
                _ => unknown(),
            },
            "has_war_with" => match named() {
                Some(other) if own => Out::of(self.facts.at_war_with(&other), note),
                Some(other) if self.me.as_deref() == Some(other.as_str()) => {
                    Out::of(self.facts.at_war_with(tag), note)
                }
                _ => unknown(),
            },
            // True only in a war: certainly false while the player is at peace.
            "has_defensive_war" | "has_offensive_war" | "has_civil_war" | "has_war_with_major" => {
                match want {
                    Some(want) if own && self.facts.at_war() == Truth::False => Out::known(!want),
                    _ => unknown(),
                }
            }
            "has_war_together_with" | "has_war_with_wargoal_against" => {
                if own && self.facts.at_war() == Truth::False {
                    Out::known(false)
                } else {
                    unknown()
                }
            }
            "has_completed_focus" if own => Out::of(self.facts.completed(value), note),
            "has_country_flag" if own => Out::of(self.facts.flag(value), note),
            "has_government" if own => match self.facts.government() {
                Some(ideology) => Out::known(ideology == value),
                None => unknown(),
            },
            "is_subject" if own => match (want, self.facts.overlord()) {
                (Some(want), Some(overlord)) => Out::known(overlord.is_some() == want),
                _ => unknown(),
            },
            // The autonomy level is not read: only a country without an overlord is surely no puppet.
            "is_puppet" if own => match (want, self.facts.overlord()) {
                (Some(want), Some(None)) => Out::known(!want),
                _ => unknown(),
            },
            "is_subject_of" => match named() {
                Some(other) if own => match self.facts.overlord() {
                    Some(overlord) => Out::known(overlord.as_deref() == Some(other.as_str())),
                    None => unknown(),
                },
                Some(other) if self.me.as_deref() == Some(other.as_str()) => {
                    member(self.facts.subjects(), tag)
                }
                _ => unknown(),
            },
            "is_puppet_of" => match named() {
                Some(other) if own => match self.facts.overlord() {
                    Some(overlord) if overlord.as_deref() != Some(other.as_str()) => {
                        Out::known(false)
                    }
                    _ => unknown(),
                },
                _ => unknown(),
            },
            "has_subject" if own => match named() {
                Some(other) => member(self.facts.subjects(), &other),
                None => unknown(),
            },
            "is_in_faction" if own => match (want, self.facts.faction()) {
                (Some(want), Some(members)) => Out::known(!members.is_empty() == want),
                _ => unknown(),
            },
            "is_in_faction_with" => match named() {
                Some(other) if own && other != tag => member(self.facts.faction(), &other),
                Some(other) if !own && self.me.as_deref() == Some(other.as_str()) => {
                    member(self.facts.faction(), tag)
                }
                _ => unknown(),
            },
            // Who leads a faction is not read: only a country in no faction surely leads none.
            "is_faction_leader" if own => match (want, self.facts.faction()) {
                (Some(want), Some(members)) if members.is_empty() => Out::known(!want),
                _ => unknown(),
            },
            // The player's own country is played, never the AI.
            "is_ai" if own => want.map_or_else(unknown, |want| Out::known(!want)),
            "owns_state" | "controls_state" | "has_full_control_of_state" => {
                let Ok(id) = value.parse::<i64>() else {
                    return unknown();
                };
                let holders = self.facts.state(id).unwrap_or_default();
                match key {
                    "owns_state" => holders.owner.map(|t| t == tag),
                    "controls_state" => holders.controller.map(|t| t == tag),
                    _ => holders.full_controller.map(|t| t.as_deref() == Some(tag)),
                }
                .map_or_else(unknown, Out::known)
            }
            "has_tech" if own => Out::of(self.facts.tech(value), note),
            "has_stability" if own => self.number(entry, "stability", value),
            "has_war_support" if own => self.number(entry, "war_support", value),
            "has_political_power" if own => self.number(entry, "political_power", value),
            "has_manpower" if own => self.number(entry, "manpower", value),
            "num_of_military_factories" if own => self.number(entry, "military_factories", value),
            "num_of_civilian_factories" if own => self.number(entry, "civilian_factories", value),
            "num_of_naval_factories" if own => self.number(entry, "naval_factories", value),
            _ => unknown(),
        }
    }
    fn state(&self, entry: &Entry, key: &str, value: &str, id: i64, scopes: &[Scope]) -> Out {
        let unknown = || Out::unknown(render(entry));
        if key == "state" {
            return value
                .parse::<i64>()
                .map_or_else(|_| unknown(), |n| Out::known(n == id));
        }
        // Inside a state scope PREV and ROOT name the country around it.
        let named = match value.to_ascii_uppercase().as_str() {
            "ROOT" => self.me.clone(),
            "PREV" => match scopes.len().checked_sub(2).and_then(|i| scopes.get(i)) {
                Some(Scope::Country(tag)) => Some(tag.clone()),
                _ => None,
            },
            _ if is_tag(value) => Some(value.to_string()),
            _ => None,
        };
        let Some(tag) = named else {
            return unknown();
        };
        let holders = self.facts.state(id).unwrap_or_default();
        let owner = holders.owner.map(|t| t == tag);
        let controller = holders.controller.map(|t| t == tag);
        match key {
            "is_owned_by" => owner,
            "is_controlled_by" => controller,
            "is_fully_controlled_by" => holders
                .full_controller
                .map(|t| t.as_deref() == Some(tag.as_str())),
            "is_owned_and_controlled_by" => match (owner, controller) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            },
            _ => None,
        }
        .map_or_else(unknown, Out::known)
    }
    /// A focus's trigger blocks of one kind in the player's own scope (`absent` when it has none).
    fn trigger(&self, blocks: &[Vec<Entry>], absent: bool) -> Out {
        match (blocks, self.me.clone()) {
            ([], _) => Out::known(absent),
            ([one], Some(me)) => self.block(one, &mut vec![Scope::Country(me)], 0),
            ([_], None) => Out::unknown("the player's tag".into()),
            // Which of several blocks the game keeps is not known.
            _ => Out::unknown("more than one block of the same kind".into()),
        }
    }
}

fn bounded(mut list: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    list.retain(|item| seen.insert(item.clone()));
    list.truncate(REASONS);
    list
}

impl Catalog {
    /// Whether the player can start `id` now.
    pub fn verdict(&self, id: &str, facts: &dyn Facts) -> Verdict {
        let done = |state, why: Vec<String>, unknown: Vec<String>| Verdict {
            state,
            why: bounded(why),
            unknown: bounded(unknown),
            bypass: false,
        };
        if facts.completed(id) == Truth::True {
            return done(State::Unavailable, vec!["already completed".into()], vec![]);
        }
        if facts.in_tree(id) == Truth::False {
            return done(
                State::Unavailable,
                vec!["not in the loaded focus tree".into()],
                vec![],
            );
        }
        let Some(focus) = self.focuses.get(id) else {
            return done(
                State::Unknown,
                vec![],
                vec!["not defined in common/national_focus".into()],
            );
        };
        if self.ambiguous.contains(id) {
            return done(
                State::Unknown,
                vec![],
                vec!["defined more than once with different rules".into()],
            );
        }
        let mut why = Vec::new();
        let mut unknown = Vec::new();
        for group in &focus.prerequisites {
            let parts: Vec<Truth> = group.iter().map(|p| facts.completed(p)).collect();
            if parts.contains(&Truth::True) {
                continue;
            }
            let text = group.join(" or ");
            if parts.contains(&Truth::Unknown) {
                unknown.push(format!("prerequisite {text}"));
            } else {
                why.push(format!("needs {text}"));
            }
        }
        for other in &focus.exclusive {
            match facts.completed(other) {
                Truth::True => why.push(format!("mutually exclusive with {other} (completed)")),
                Truth::Unknown => unknown.push(format!("mutually exclusive with {other}")),
                Truth::False => {}
            }
        }
        // A focus that only the other side names as exclusive: the game's reading is not certain.
        for other in self.excluded_by.get(id).into_iter().flatten() {
            if !focus.exclusive.contains(other) && facts.completed(other) != Truth::False {
                unknown.push(format!("{other} lists it as mutually exclusive"));
            }
        }
        let me = facts.player();
        // allow_branch is judged when the tree is laid out: only its static part is certain.
        let fixed = Static(facts);
        let branch = Eval {
            catalog: self,
            facts: &fixed,
            me: me.clone(),
        }
        .trigger(&focus.allow_branch, true);
        let eval = Eval {
            catalog: self,
            facts,
            me,
        };
        match branch.truth {
            Truth::True => {}
            Truth::False => why.push("its branch is not loaded (allow_branch)".into()),
            Truth::Unknown => {
                let now = eval.trigger(&focus.allow_branch, true);
                if now.truth != Truth::True {
                    unknown.push("allow_branch (judged when the tree was laid out)".into());
                    unknown.extend(now.notes);
                }
            }
        }
        let available = eval.trigger(&focus.available, true);
        let bypass = eval.trigger(&focus.bypass, false);
        match any(vec![available.clone(), bypass.clone()]).truth {
            Truth::True => {}
            Truth::Unknown => {
                unknown.extend(available.notes);
                unknown.extend(bypass.notes);
            }
            Truth::False => {
                // The parts of `available` that fail, in the game's own words.
                let failing: Vec<String> = match (focus.available.as_slice(), eval.me.clone()) {
                    ([one], Some(me)) => eval
                        .items(one, &mut vec![Scope::Country(me)], 0)
                        .into_iter()
                        .filter(|(_, o)| o.truth == Truth::False)
                        .map(|(text, _)| format!("needs {text}"))
                        .collect(),
                    _ => Vec::new(),
                };
                if failing.is_empty() {
                    why.push("available does not hold".into());
                } else {
                    why.extend(failing);
                }
            }
        }
        let state = if !why.is_empty() {
            State::Unavailable
        } else if !unknown.is_empty() {
            State::Unknown
        } else {
            State::Available
        };
        Verdict {
            state,
            why: bounded(why),
            unknown: if state == State::Unknown {
                bounded(unknown)
            } else {
                Vec::new()
            },
            bypass: state != State::Unavailable && bypass.truth == Truth::True,
        }
    }
}

#[cfg(test)]
mod tests;
