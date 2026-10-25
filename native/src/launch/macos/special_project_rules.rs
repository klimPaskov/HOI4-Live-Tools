//! Static rules of the vanilla special projects, read from the owned game's own
//! `common/special_projects/projects/*.txt` (public install data, no hidden
//! information): breakthrough cost, parent projects, the technology a project
//! needs and the resources a prototype consumes. The game screen only greys
//! an unavailable project out; this says why in the game's own words.
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Atom(String),
    Block(Vec<Item>),
}
#[derive(Clone, Debug, PartialEq)]
enum Item {
    Pair(String, Node),
    Bare(String),
}

/// Rules of one project.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Rules {
    pub specialization: Option<String>,
    pub breakthrough_cost: BTreeMap<String, i64>,
    pub parents: Vec<String>,
    /// Technologies named inside `available`, `allowed` and `visible`.
    pub technologies: Vec<String>,
    /// The `available` trigger with whitespace collapsed (bounded).
    pub available: String,
    pub resources: BTreeMap<String, i64>,
    pub time: Option<String>,
    pub complexity: Option<String>,
}

fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '#' => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
            }
            '{' | '}' | '=' => out.push(c.to_string()),
            '"' => {
                let mut s = String::new();
                for n in chars.by_ref() {
                    if n == '"' {
                        break;
                    }
                    s.push(n);
                }
                out.push(s);
            }
            c if c.is_whitespace() => {}
            c => {
                let mut s = c.to_string();
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || matches!(n, '{' | '}' | '=' | '#') {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                out.push(s);
            }
        }
    }
    out
}
fn block(tokens: &[String], at: &mut usize) -> Vec<Item> {
    let mut items = Vec::new();
    while *at < tokens.len() {
        let t = tokens[*at].clone();
        *at += 1;
        match t.as_str() {
            "}" => break,
            "{" => {
                // anonymous nested block: flatten
                items.extend(block(tokens, at));
            }
            "=" => {}
            _ => {
                if tokens.get(*at).map(String::as_str) == Some("=") {
                    *at += 1;
                    if tokens.get(*at).map(String::as_str) == Some("{") {
                        *at += 1;
                        items.push(Item::Pair(t, Node::Block(block(tokens, at))));
                    } else if let Some(v) = tokens.get(*at) {
                        items.push(Item::Pair(t, Node::Atom(v.clone())));
                        *at += 1;
                    }
                } else {
                    items.push(Item::Bare(t));
                }
            }
        }
    }
    items
}
fn parse(text: &str) -> Vec<Item> {
    let t = tokens(text);
    let mut at = 0;
    block(&t, &mut at)
}
fn find<'a>(items: &'a [Item], key: &str) -> Option<&'a Node> {
    items.iter().find_map(|i| match i {
        Item::Pair(k, n) if k == key => Some(n),
        _ => None,
    })
}
fn atom(items: &[Item], key: &str) -> Option<String> {
    match find(items, key) {
        Some(Node::Atom(a)) => Some(a.clone()),
        _ => None,
    }
}
fn technologies(items: &[Item], out: &mut Vec<String>) {
    for i in items {
        match i {
            Item::Pair(k, Node::Atom(v)) if k == "has_tech" => {
                if !out.contains(v) {
                    out.push(v.clone());
                }
            }
            Item::Pair(_, Node::Block(b)) => technologies(b, out),
            _ => {}
        }
    }
}
fn render(items: &[Item], out: &mut String) {
    for i in items {
        match i {
            Item::Pair(k, Node::Atom(v)) => out.push_str(&format!("{k} = {v} ")),
            Item::Pair(k, Node::Block(b)) => {
                out.push_str(&format!("{k} = {{ "));
                render(b, out);
                out.push_str("} ");
            }
            Item::Bare(b) => out.push_str(&format!("{b} ")),
        }
    }
}
fn strip(prefix: &str, name: &str) -> String {
    name.strip_prefix(prefix).unwrap_or(name).to_string()
}
fn ints(items: &[Item]) -> BTreeMap<String, i64> {
    items
        .iter()
        .filter_map(|i| match i {
            Item::Pair(k, Node::Atom(v)) => v.parse().ok().map(|n| (strip("specialization_", k), n)),
            _ => None,
        })
        .collect()
}
/// Rules by project key out of one project file's text.
pub(super) fn parse_projects(text: &str) -> BTreeMap<String, Rules> {
    let mut out = BTreeMap::new();
    for item in parse(text) {
        let Item::Pair(key, Node::Block(body)) = item else {
            continue;
        };
        if !key.starts_with("sp_") {
            continue;
        }
        let mut rules = Rules {
            specialization: atom(&body, "specialization").map(|s| strip("specialization_", &s)),
            time: atom(&body, "prototype_time").map(|s| strip("sp_time.prototype.", &s)),
            complexity: atom(&body, "complexity").map(|s| strip("sp_complexity.", &s)),
            ..Rules::default()
        };
        if let Some(Node::Block(b)) = find(&body, "breakthrough_cost") {
            rules.breakthrough_cost = ints(b);
        }
        if let Some(Node::Block(b)) = find(&body, "special_project_parent") {
            rules.parents = b
                .iter()
                .filter_map(|i| match i {
                    Item::Bare(p) => Some(p.clone()),
                    _ => None,
                })
                .collect();
        }
        for gate in ["available", "allowed", "visible"] {
            if let Some(Node::Block(b)) = find(&body, gate) {
                technologies(b, &mut rules.technologies);
                if gate == "available" {
                    let mut text = String::new();
                    render(b, &mut text);
                    rules.available = text.trim().chars().take(400).collect();
                }
            }
        }
        if let Some(Node::Block(r)) = find(&body, "resource_cost") {
            if let Some(Node::Block(res)) = find(r, "resources") {
                rules.resources = ints(res);
            }
        }
        out.insert(key, rules);
    }
    out
}
/// All vanilla project rules of an install directory (unreadable files skip).
pub(super) fn load(game_dir: &Path) -> BTreeMap<String, Rules> {
    let mut out = BTreeMap::new();
    let Ok(dir) = fs::read_dir(game_dir.join("common/special_projects/projects")) else {
        return out;
    };
    let mut files: Vec<_> = dir.flatten().map(|e| e.path()).collect();
    files.sort();
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("txt") {
            continue;
        }
        if let Ok(text) = fs::read_to_string(&path) {
            out.extend(parse_projects(&text));
        }
    }
    out
}
impl Rules {
    pub(super) fn to_json(&self) -> Value {
        json!({
            "breakthrough_cost": self.breakthrough_cost,
            "requires_projects": self.parents,
            "requires_technologies": self.technologies,
            "available_trigger": self.available,
            "prototype_resources": self.resources,
            "prototype_time": self.time,
            "complexity": self.complexity,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RADAR: &str = r#"
sp_air_radar = {
	specialization = specialization_air
	prototype_time = sp_time.prototype.medium # comment { with brace
	complexity = sp_complexity.medium
	breakthrough_cost = {
		specialization_air = 1
	}
	available = {
		FROM = { has_tech = electronic_mechanical_engineering }
	}
	resource_cost = {
		resources = {
			steel = 6
			aluminium = 6
		 }
	}
	project_output = { country_effects = { set_technology = { radio_detection = 1 } } }
}
sp_air_jet_engine = {
	specialization = specialization_air
	special_project_parent = {
		sp_air_radar
		sp_other
	}
	breakthrough_cost = { specialization_air = 3 }
	allowed = { OR = { has_tech = engines_2 has_tech = engines_3 } }
	available = { has_tech = engines_2 }
}
not_a_project = { x = 1 }
"#;
    #[test]
    fn radar_rules_are_extracted() {
        let rules = parse_projects(RADAR);
        let r = &rules["sp_air_radar"];
        assert_eq!(r.specialization.as_deref(), Some("air"));
        assert_eq!(r.breakthrough_cost["air"], 1);
        assert_eq!(r.technologies, vec!["electronic_mechanical_engineering"]);
        assert_eq!(r.resources["steel"], 6);
        assert_eq!(r.resources["aluminium"], 6);
        assert_eq!(r.time.as_deref(), Some("medium"));
        assert_eq!(r.complexity.as_deref(), Some("medium"));
        assert!(r.available.contains("has_tech = electronic_mechanical_engineering"));
        assert!(!rules.contains_key("not_a_project"));
    }
    #[test]
    fn parents_and_alternative_technologies() {
        let rules = parse_projects(RADAR);
        let j = &rules["sp_air_jet_engine"];
        assert_eq!(j.parents, vec!["sp_air_radar", "sp_other"]);
        assert_eq!(j.breakthrough_cost["air"], 3);
        assert_eq!(j.technologies, vec!["engines_2", "engines_3"]);
        let v = j.to_json();
        assert_eq!(v["requires_projects"][1], "sp_other");
        assert_eq!(v["breakthrough_cost"]["air"], 3);
    }
}
