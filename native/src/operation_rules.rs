//! Intelligence operations from the game's own files and whether one can be launched against a
//! target now, without opening a window.
//!
//! The definitions come from `common/operations` (and the phase costs from
//! `common/operation_phases`) of the running game's content, so mods and every operation of the
//! install are covered the same way; nothing is tied to one operation or country. An operation is
//! listed in the agency when `visible` holds (ROOT the player, FROM the target), may be prepared when
//! `allowed` and `available` hold, the network in the target reaches `network_strength`, enough
//! free operatives exist and the `required_tokens` are held; its equipment and civilian factories
//! are taken when it starts (more for the phases the game draws).
//!
//! Every condition is judged with three values: `Unavailable` only when certain. A trigger the
//! server cannot read (flags, focuses, variables, other countries' affairs, national coverage)
//! makes the answer unknown and is named in the reasons, so a caller can look at the game once.
use crate::plan_predicate::Truth;
use crate::script::{self, Block as _, Entry, Val};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

pub mod live;

/// One operation of the content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Operation {
    pub key: String,
    pub name: String,
    pub priority: f64,
    pub days: f64,
    pub network_strength: f64,
    pub operatives: usize,
    pub risk_chance: f64,
    /// Equipment archetype keys with amounts, taken at the start.
    pub equipment: Vec<(String, f64)>,
    /// (factories, days) of civilian factories the operation uses.
    pub factories: Option<(f64, f64)>,
    pub required_tokens: Vec<String>,
    pub awarded_tokens: Vec<String>,
    /// Allowed targets (`operation_target`), empty for any.
    pub targets: Vec<String>,
    /// Phase groups: the game draws one phase of each group.
    pub phases: Vec<Vec<String>>,
    visible: Vec<Entry>,
    allowed: Vec<Entry>,
    available: Vec<Entry>,
    requirements: Vec<Entry>,
}

/// Costs a phase adds when the game draws it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Phase {
    pub equipment: Vec<(String, f64)>,
    pub factories: Option<(f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub operations: Vec<Operation>,
    pub phases: Vec<(String, Phase)>,
    /// `rules` of every ideology group (`can_create_collaboration_government = no` ...).
    pub ideology_rules: Vec<(String, Vec<(String, bool)>)>,
}

fn costs(block: &[Entry]) -> (Vec<(String, f64)>, Option<(f64, f64)>) {
    let mut equipment = Vec::new();
    let mut factories = None;
    if let Some(Val::Block(items)) = block.find_val("equipment") {
        for e in items {
            match &e.val {
                Val::Scalar(v) => {
                    if let Ok(n) = v.parse::<f64>() {
                        equipment.push((e.key.clone(), n));
                    }
                }
                Val::Block(b) if e.key == "civilian_factories" => {
                    factories = Some((b.number("amount").unwrap_or(0.0), b.number("days").unwrap_or(0.0)));
                }
                Val::Block(_) => {}
            }
        }
    }
    (equipment, factories)
}

fn block(b: &[Entry], key: &str) -> Vec<Entry> {
    b.iter()
        .filter(|e| e.key == key)
        .flat_map(|e| e.val.block().to_vec())
        .collect()
}

impl Catalog {
    pub(crate) fn add_operations(&mut self, entries: &[Entry]) {
        for e in entries {
            let Val::Block(b) = &e.val else { continue };
            // Only blocks shaped like an operation (they all name their days and network).
            if b.find_val("days").is_none() || b.find_val("network_strength").is_none() {
                continue;
            }
            let (equipment, factories) = costs(b);
            let phases = b
                .all("phases")
                .map(|p| p.block().iter().map(|x| x.key.clone()).collect())
                .collect();
            let targets = b
                .find_val("operation_target")
                .map(|t| t.block().words("targets"))
                .unwrap_or_default();
            self.operations.retain(|o| o.key != e.key);
            self.operations.push(Operation {
                key: e.key.clone(),
                name: b.text("name").unwrap_or(&e.key).to_string(),
                priority: b.number("priority").unwrap_or(0.0),
                days: b.number("days").unwrap_or(0.0),
                network_strength: b.number("network_strength").unwrap_or(0.0),
                operatives: b.number("operatives").unwrap_or(0.0).max(0.0) as usize,
                risk_chance: b.number("risk_chance").unwrap_or(0.0),
                equipment,
                factories,
                required_tokens: b.words("required_tokens"),
                awarded_tokens: b.words("awarded_tokens"),
                targets,
                phases,
                visible: block(b, "visible"),
                allowed: block(b, "allowed"),
                available: block(b, "available"),
                requirements: block(b, "requirements"),
            });
        }
        self.operations
            .sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
    }

    pub(crate) fn add_phases(&mut self, entries: &[Entry]) {
        for e in entries {
            let Val::Block(b) = &e.val else { continue };
            let (equipment, factories) = costs(b);
            self.phases.retain(|(k, _)| *k != e.key);
            self.phases.push((e.key.clone(), Phase { equipment, factories }));
        }
    }

    pub(crate) fn add_ideologies(&mut self, entries: &[Entry]) {
        for top in entries.iter().filter(|e| e.key == "ideologies") {
            for group in top.val.block() {
                let rules = group
                    .val
                    .block()
                    .find_val("rules")
                    .map(|r| {
                        r.block()
                            .iter()
                            .filter_map(|x| x.val.scalar().map(|v| (x.key.clone(), v != "no")))
                            .collect()
                    })
                    .unwrap_or_default();
                self.ideology_rules.retain(|(k, _)| *k != group.key);
                self.ideology_rules.push((group.key.clone(), rules));
            }
        }
    }

    pub fn load(content: &Path) -> Arc<Catalog> {
        script::cached(content, "operation_rules", |dir| {
            let mut c = Catalog::default();
            c.add_operations(&script::parse_dir(&dir.join("common/operations")));
            c.add_phases(&script::parse_dir(&dir.join("common/operation_phases")));
            c.add_ideologies(&script::parse_dir(&dir.join("common/ideologies")));
            c
        })
    }

    pub fn operation(&self, key: &str) -> Option<&Operation> {
        self.operations.iter().find(|o| o.key == key)
    }

    /// A rule of the ruling ideology group: true unless its `rules` say no.
    pub fn rule(&self, ideology: Option<&str>, rule: &str) -> Truth {
        let Some(ideology) = ideology else { return Truth::Unknown };
        match self.ideology_rules.iter().find(|(k, _)| k == ideology) {
            None => Truth::Unknown,
            Some((_, rules)) => Truth::from_bool(
                rules.iter().find(|(k, _)| k == rule).map_or(true, |(_, v)| *v),
            ),
        }
    }

    /// (min, max) extra cost of the phases one group can draw, per equipment key and factories.
    pub fn phase_costs(&self, op: &Operation) -> Value {
        let mut groups = Vec::new();
        for group in &op.phases {
            let options: Vec<Value> = group
                .iter()
                .map(|k| {
                    let p = self.phases.iter().find(|(x, _)| x == k).map(|(_, p)| p.clone()).unwrap_or_default();
                    json!({
                        "phase": k,
                        "equipment": p.equipment.iter().map(|(e, n)| json!({"key": e, "amount": n})).collect::<Vec<_>>(),
                        "civilian_factories": p.factories.map(|(a, d)| json!({"amount": a, "days": d})),
                    })
                })
                .collect();
            groups.push(json!(options));
        }
        json!(groups)
    }
}

/// What the rules need from the running game. ROOT is the player, FROM the target.
pub trait Facts {
    fn player(&self) -> String;
    /// Ruling ideology group of the player.
    fn ideology(&self) -> Option<String>;
    /// The player has an overlord (subject rules then also apply).
    fn subject(&self) -> Truth;
    fn at_war_with(&self, tag: &str) -> Truth;
    fn at_war(&self) -> Truth;
    /// Own network strength in `target` (strongest covered state).
    fn network(&self, target: &str) -> Option<f64>;
    /// Own collaboration in `target` (0..1).
    fn collaboration(&self, target: &str) -> Option<f64>;
    fn token(&self, target: &str, token: &str) -> Truth;
    fn dlc(&self, name: &str) -> Truth;
    /// Recruited operatives.
    fn operatives(&self) -> Option<usize>;
    /// Free operatives (no operation, not captured).
    fn free_operatives(&self) -> Option<usize>;
    /// Stockpiled amount of an equipment archetype.
    fn stockpile(&self, archetype: &str) -> Option<f64>;
}

#[derive(Clone, Copy, PartialEq)]
enum Who {
    Root,
    From,
}

fn and(items: impl Iterator<Item = Truth>) -> Truth {
    let mut out = Truth::True;
    for t in items {
        match t {
            Truth::False => return Truth::False,
            Truth::Unknown => out = Truth::Unknown,
            Truth::True => {}
        }
    }
    out
}

fn or(items: impl Iterator<Item = Truth>) -> Truth {
    let mut out = Truth::False;
    for t in items {
        match t {
            Truth::True => return Truth::True,
            Truth::Unknown => out = Truth::Unknown,
            Truth::False => {}
        }
    }
    out
}

fn not(t: Truth) -> Truth {
    match t {
        Truth::True => Truth::False,
        Truth::False => Truth::True,
        Truth::Unknown => Truth::Unknown,
    }
}

fn compare(value: f64, op: &str, limit: f64) -> bool {
    match op {
        ">" => value > limit,
        ">=" => value >= limit,
        "<" => value < limit,
        "<=" => value <= limit,
        "!=" => (value - limit).abs() > 1e-9,
        _ => (value - limit).abs() < 1e-9 || value >= limit,
    }
}

/// `{ target = FROM value > 25 }`: (target, op, limit).
fn target_value(b: &[Entry]) -> Option<(String, String, f64)> {
    let target = b.text("target")?.to_string();
    let v = b.iter().find(|e| e.key == "value")?;
    Some((target, v.op.clone(), v.val.scalar()?.parse().ok()?))
}

struct Judge<'a> {
    facts: &'a dyn Facts,
    catalog: &'a Catalog,
    target: &'a str,
    /// Names of triggers that could not be read.
    unknown: std::cell::RefCell<Vec<String>>,
}

impl Judge<'_> {
    fn tag(&self, word: &str, who: Who) -> String {
        match word {
            "FROM" => self.target.to_string(),
            "ROOT" => self.facts.player(),
            "THIS" => match who {
                Who::Root => self.facts.player(),
                Who::From => self.target.to_string(),
            },
            other => other.to_string(),
        }
    }

    fn unknown(&self, name: &str) -> Truth {
        let mut u = self.unknown.borrow_mut();
        if !u.iter().any(|x| x == name) {
            u.push(name.to_string());
        }
        Truth::Unknown
    }

    fn all(&self, entries: &[Entry], who: Who) -> Truth {
        and(entries.iter().map(|e| self.one(e, who)))
    }

    fn one(&self, e: &Entry, who: Who) -> Truth {
        let scalar = e.val.scalar();
        let yes = scalar.map(|v| v != "no");
        match e.key.as_str() {
            "AND" => self.all(e.val.block(), who),
            "OR" => or(e.val.block().iter().map(|x| self.one(x, who))),
            "NOT" | "not" => not(or(e.val.block().iter().map(|x| self.one(x, who)))),
            "always" => Truth::from_bool(yes == Some(true)),
            "FROM" => self.all(e.val.block(), Who::From),
            "ROOT" => self.all(e.val.block(), Who::Root),
            "hidden_trigger" | "custom_trigger_tooltip" | "custom_override_tooltip" => {
                self.all(&e.val.block().iter().filter(|x| x.key != "tooltip").cloned().collect::<Vec<_>>(), who)
            }
            "tag" | "original_tag" => match scalar {
                Some(t) => {
                    let me = self.tag("THIS", who);
                    // original_tag differs from tag only for created countries, which the
                    // server names `?id`: those stay unknown.
                    if me.starts_with('?') {
                        self.unknown(&e.key)
                    } else {
                        Truth::from_bool(me == self.tag(t, who))
                    }
                }
                None => self.unknown(&e.key),
            },
            "has_war_with" => match (who, scalar) {
                (Who::Root, Some(t)) if self.tag(t, who) == self.target => self.facts.at_war_with(self.target),
                (Who::From, Some(t)) if self.tag(t, who) == self.facts.player() => self.facts.at_war_with(self.target),
                _ => self.unknown("has_war_with"),
            },
            "has_war" if who == Who::Root => {
                let t = self.facts.at_war();
                if yes == Some(false) { not(t) } else { t }
            }
            "has_government" if who == Who::Root => match (self.facts.ideology(), scalar) {
                (Some(i), Some(g)) => Truth::from_bool(i == g),
                _ => self.unknown("has_government"),
            },
            "has_rule" if who == Who::Root => match scalar {
                Some(rule) => {
                    let by_ideology = self.catalog.rule(self.facts.ideology().as_deref(), rule);
                    match (by_ideology, self.facts.subject()) {
                        (Truth::False, _) => Truth::False,
                        (Truth::True, Truth::False) => Truth::True,
                        _ => self.unknown("has_rule"),
                    }
                }
                None => self.unknown("has_rule"),
            },
            "has_dlc" => scalar.map_or_else(|| self.unknown("has_dlc"), |d| self.facts.dlc(d.trim_matches('"'))),
            "is_ai" if who == Who::Root => Truth::from_bool(yes == Some(false)),
            "network_strength" if who == Who::Root => match target_value(e.val.block()) {
                Some((t, op, limit)) => match self.facts.network(&self.tag(&t, who)) {
                    Some(v) => Truth::from_bool(compare(v, &op, limit)),
                    None => Truth::from_bool(compare(0.0, &op, limit)),
                },
                None => self.unknown("network_strength"),
            },
            "has_collaboration" if who == Who::Root => match target_value(e.val.block()) {
                Some((t, op, limit)) => match self.facts.collaboration(&self.tag(&t, who)) {
                    Some(v) => Truth::from_bool(compare(v, &op, limit)),
                    None => self.unknown("has_collaboration"),
                },
                None => self.unknown("has_collaboration"),
            },
            "has_operation_token" if who == Who::Root => {
                let b = e.val.block();
                match (b.text("tag"), b.text("token")) {
                    (Some(t), Some(tok)) => self.facts.token(&self.tag(t, who), tok),
                    _ => self.unknown("has_operation_token"),
                }
            }
            "num_of_operatives" if who == Who::Root => match (self.facts.operatives(), scalar.and_then(|v| v.parse::<f64>().ok())) {
                (Some(n), Some(limit)) => Truth::from_bool(compare(n as f64, &e.op, limit)),
                _ => self.unknown("num_of_operatives"),
            },
            other => self.unknown(other),
        }
    }
}

/// Why an operation cannot be prepared now.
#[derive(Clone, Debug, PartialEq)]
pub struct Reason {
    pub code: &'static str,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Verdict {
    /// The agency lists it against this target.
    pub visible: Truth,
    /// It can be prepared now (every requirement met).
    pub ready: Truth,
    pub reasons: Vec<Reason>,
    pub unknown_triggers: Vec<String>,
}

impl Verdict {
    pub fn json(&self) -> Value {
        json!({
            "visible": self.visible.label(),
            "ready": self.ready.label(),
            "reasons": self.reasons.iter().map(|r| json!({"code": r.code, "detail": r.detail})).collect::<Vec<_>>(),
            "unknown_triggers": self.unknown_triggers,
        })
    }
}

pub fn verdict(catalog: &Catalog, op: &Operation, target: &str, facts: &dyn Facts) -> Verdict {
    let judge = Judge {
        facts,
        catalog,
        target,
        unknown: Default::default(),
    };
    let mut reasons = Vec::new();
    let target_ok = op.targets.is_empty() || op.targets.iter().any(|t| t == target);
    let visible = and([Truth::from_bool(target_ok), judge.all(&op.visible, Who::Root)].into_iter());
    if !target_ok {
        reasons.push(Reason { code: "not_a_target", detail: format!("{} only targets {}", op.key, op.targets.join(", ")) });
    }
    let allowed = judge.all(&op.allowed, Who::Root);
    if allowed == Truth::False {
        reasons.push(Reason { code: "not_allowed", detail: format!("{} is not allowed for {} (its allowed conditions fail)", op.key, facts.player()) });
    }
    let available = judge.all(&op.available, Who::Root);
    if available == Truth::False {
        reasons.push(Reason { code: "unavailable", detail: format!("{} is not available against {target} now (its available conditions fail)", op.key) });
    }
    let requirements = judge.all(&op.requirements, Who::Root);
    if requirements == Truth::False {
        reasons.push(Reason { code: "requirements_unmet", detail: format!("the requirements of {} fail against {target}", op.key) });
    }
    if visible == Truth::False && target_ok {
        reasons.push(Reason { code: "not_visible", detail: format!("{} is not listed against {target} (its visible conditions fail)", op.key) });
    }
    let strength = facts.network(target).unwrap_or(0.0);
    let network = Truth::from_bool(strength >= op.network_strength);
    if network == Truth::False {
        reasons.push(Reason {
            code: "network_too_weak",
            detail: format!("network in {target} is {strength:.1}, {} needs {}", op.key, op.network_strength),
        });
    }
    let operatives = match facts.free_operatives() {
        Some(free) if free >= op.operatives => Truth::True,
        Some(free) => {
            reasons.push(Reason {
                code: "not_enough_operatives",
                detail: format!("{free} free operatives, {} needs {}", op.key, op.operatives),
            });
            Truth::False
        }
        None => Truth::Unknown,
    };
    let mut equipment = Vec::new();
    for (key, amount) in &op.equipment {
        match facts.stockpile(key) {
            Some(have) if have >= *amount => equipment.push(Truth::True),
            Some(have) => {
                reasons.push(Reason { code: "missing_equipment", detail: format!("{key}: {have:.0} in stock, {} needs {amount}", op.key) });
                equipment.push(Truth::False);
            }
            None => equipment.push(judge.unknown(&format!("stockpile {key}"))),
        }
    }
    let mut tokens = Vec::new();
    for t in &op.required_tokens {
        let held = facts.token(target, t);
        if held == Truth::False {
            reasons.push(Reason { code: "missing_token", detail: format!("{} needs the {t} token against {target}", op.key) });
        }
        if held == Truth::Unknown {
            judge.unknown(&format!("token {t}"));
        }
        tokens.push(held);
    }
    let ready = and(
        [visible, allowed, available, requirements, network, operatives]
            .into_iter()
            .chain(equipment)
            .chain(tokens),
    );
    let unknown_triggers = judge.unknown.into_inner();
    Verdict { visible, ready, reasons, unknown_triggers }
}

/// The operation's costs and requirements as listed by the tools.
pub fn describe(catalog: &Catalog, op: &Operation) -> Value {
    json!({
        "key": op.key,
        "days": op.days,
        "network_strength": op.network_strength,
        "operatives": op.operatives,
        "risk_chance": op.risk_chance,
        "equipment": op.equipment.iter().map(|(k, n)| json!({"key": k, "amount": n})).collect::<Vec<_>>(),
        "civilian_factories": op.factories.map(|(a, d)| json!({"amount": a, "days": d})),
        "required_tokens": op.required_tokens,
        "awarded_tokens": op.awarded_tokens,
        "targets": op.targets,
        "phase_choices": catalog.phase_costs(op),
    })
}

#[cfg(test)]
mod tests;
