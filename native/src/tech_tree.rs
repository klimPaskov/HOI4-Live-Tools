//! Technology prerequisites from the game's own files (common/technologies/*.txt): a tech's
//! `path = { leads_to_tech = X }` makes it a prerequisite of X. Read from the running game's
//! install, so mods with their own trees are covered.
use crate::error::{Error, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Technology -> the techs that lead to it.
pub type Prerequisites = Arc<HashMap<String, Vec<String>>>;

/// Edges (parent, child) of one technology file.
pub fn parse(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current: Option<String> = None;
    let mut prev: Vec<String> = Vec::new();
    // Tokenise: words, '=', '{', '}' (comments to the end of the line removed).
    let mut tokens = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let mut word = String::new();
        for ch in line.chars() {
            match ch {
                '{' | '}' | '=' => {
                    if !word.is_empty() {
                        tokens.push(std::mem::take(&mut word));
                    }
                    tokens.push(ch.to_string());
                }
                c if c.is_whitespace() => {
                    if !word.is_empty() {
                        tokens.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        if !word.is_empty() {
            tokens.push(word);
        }
    }
    for (i, t) in tokens.iter().enumerate() {
        match t.as_str() {
            "{" => {
                // `name = {` one level inside `technologies = {` opens a technology.
                if depth == 1 && prev.len() >= 2 && prev[prev.len() - 1] == "=" {
                    current = Some(prev[prev.len() - 2].clone());
                }
                depth += 1;
            }
            "}" => {
                depth -= 1;
                if depth <= 1 {
                    current = None;
                }
            }
            "leads_to_tech" => {
                if let (Some(parent), Some(eq), Some(child)) =
                    (current.as_ref(), tokens.get(i + 1), tokens.get(i + 2))
                {
                    if eq == "=" && child != "{" {
                        out.push((parent.clone(), child.clone()));
                    }
                }
            }
            _ => {}
        }
        prev.push(t.clone());
        if prev.len() > 4 {
            prev.remove(0);
        }
    }
    out
}

/// `start_year` of each technology block (researching before it costs the ahead-of-time penalty).
pub fn parse_years(text: &str) -> Vec<(String, i32)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current: Option<String> = None;
    let mut last_word = String::new();
    let mut words: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let spaced = line
            .replace('{', " { ")
            .replace('}', " } ")
            .replace('=', " = ");
        for t in spaced.split_whitespace() {
            match t {
                "{" => {
                    if depth == 1 && words.len() >= 2 && words[words.len() - 1] == "=" {
                        current = Some(words[words.len() - 2].clone());
                    }
                    depth += 1;
                }
                "}" => {
                    depth -= 1;
                    if depth <= 1 {
                        current = None;
                    }
                }
                _ => {
                    if depth == 2
                        && last_word == "="
                        && words.len() >= 2
                        && words[words.len() - 2] == "start_year"
                    {
                        if let (Some(c), Ok(y)) = (current.as_ref(), t.parse::<i32>()) {
                            out.push((c.clone(), y));
                        }
                    }
                }
            }
            last_word = t.to_string();
            words.push(t.to_string());
            if words.len() > 4 {
                words.remove(0);
            }
        }
    }
    out
}

/// `start_year` of every technology in `dir/common/technologies`, cached per install.
/// Technology -> start year.
pub type Years = Arc<HashMap<String, i32>>;
pub fn years(dir: &Path) -> Result<Years> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Years>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let entries = std::fs::read_dir(dir.join("common/technologies"))
        .map_err(|_| Error::new("map_data_unavailable", "Technology files unreadable"))?;
    let mut map = HashMap::new();
    for entry in entries.flatten() {
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            map.extend(parse_years(&text));
        }
    }
    let map = Arc::new(map);
    cache.lock().unwrap().insert(dir.to_path_buf(), map.clone());
    Ok(map)
}

/// Conditions of a technology besides its `path` parents.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Meta {
    /// `dependencies = { a = 1 }`: all required.
    pub dependencies: Vec<String>,
    /// `XOR = { a b }`: closed once any of them is researched.
    pub xor: Vec<String>,
    /// An `allow` block (special projects, scripted unlocks): not for automatic choice.
    pub gated: bool,
}

/// Meta of each technology block in one file.
pub fn parse_meta(text: &str) -> Vec<(String, Meta)> {
    let mut out: Vec<(String, Meta)> = Vec::new();
    let mut depth = 0i32;
    let mut current: Option<(String, Meta)> = None;
    let mut block: Option<String> = None; // key of the depth-3 block inside a tech
    let mut words: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("");
        let spaced = line
            .replace('{', " { ")
            .replace('}', " } ")
            .replace('=', " = ");
        for t in spaced.split_whitespace() {
            match t {
                "{" => {
                    let key = (words.len() >= 2 && words[words.len() - 1] == "=")
                        .then(|| words[words.len() - 2].clone());
                    if depth == 1 {
                        if let Some(k) = key.clone() {
                            current = Some((k, Meta::default()));
                        }
                    } else if depth == 2 {
                        block = key.clone();
                        if let (Some(k), Some((_, m))) = (key.as_deref(), current.as_mut()) {
                            if k == "allow" || k == "allow_branch" {
                                m.gated = true;
                            }
                        }
                    }
                    depth += 1;
                }
                "}" => {
                    depth -= 1;
                    if depth == 2 {
                        block = None;
                    }
                    if depth == 1 {
                        if let Some(done) = current.take() {
                            out.push(done);
                        }
                    }
                }
                "=" => {}
                word => {
                    if depth == 3 {
                        if let Some((_, m)) = current.as_mut() {
                            match block.as_deref() {
                                Some("dependencies")
                                    if words.last().map(String::as_str) != Some("=") =>
                                {
                                    m.dependencies.push(word.to_string())
                                }
                                Some("XOR") => m.xor.push(word.to_string()),
                                _ => {}
                            }
                        }
                    }
                }
            }
            words.push(t.to_string());
            if words.len() > 4 {
                words.remove(0);
            }
        }
    }
    out
}

/// Meta of every technology in `dir/common/technologies`, cached per install.
pub type Metas = Arc<HashMap<String, Meta>>;
pub fn metas(dir: &Path) -> Result<Metas> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Metas>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let entries = std::fs::read_dir(dir.join("common/technologies"))
        .map_err(|_| Error::new("map_data_unavailable", "Technology files unreadable"))?;
    let mut map = HashMap::new();
    for entry in entries.flatten() {
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            map.extend(parse_meta(&text));
        }
    }
    let map = Arc::new(map);
    cache.lock().unwrap().insert(dir.to_path_buf(), map.clone());
    Ok(map)
}

/// `startable` plus the technology's own conditions: every dependency done, no exclusive
/// partner done, no scripted `allow` gate.
pub fn eligible(
    prereqs: &HashMap<String, Vec<String>>,
    metas: &HashMap<String, Meta>,
    done: &HashSet<String>,
    tech: &str,
) -> bool {
    startable(prereqs, done, tech)
        && metas.get(tech).is_none_or(|m| {
            !m.gated
                && m.dependencies.iter().all(|d| done.contains(d))
                && !m.xor.iter().any(|x| done.contains(x))
        })
}

/// Prerequisites of every technology in `dir/common/technologies`, cached per install.
pub fn prerequisites(dir: &Path) -> Result<Prerequisites> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Prerequisites>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let entries = std::fs::read_dir(dir.join("common/technologies"))
        .map_err(|_| Error::new("map_data_unavailable", "Technology files unreadable"))?;
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for entry in entries.flatten() {
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            for (parent, child) in parse(&text) {
                map.entry(child).or_default().push(parent);
            }
        }
    }
    let map = Arc::new(map);
    cache.lock().unwrap().insert(dir.to_path_buf(), map.clone());
    Ok(map)
}

/// The game install holding `common/technologies`, from the game executable's path.
pub fn install_dir(exe: &str) -> Option<PathBuf> {
    Path::new(exe)
        .ancestors()
        .find(|d| d.join("common/technologies").is_dir())
        .map(Path::to_path_buf)
}

/// Whether `tech` can be started now: not done and every prerequisite done.
pub fn startable(
    prereqs: &HashMap<String, Vec<String>>,
    done: &HashSet<String>,
    tech: &str,
) -> bool {
    !done.contains(tech)
        && prereqs
            .get(tech)
            .is_none_or(|parents| parents.iter().all(|p| done.contains(p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edges_come_from_paths_inside_technology_blocks() {
        let text = "technologies = {\n  artillery1 = {\n    path = { leads_to_tech = artillery2 # next\n research_cost_coeff = 1 }\n    path = { leads_to_tech = sp_art }\n    folder = { name = land }\n  }\n  artillery2 = { cost = 2 }\n}\n";
        let e = parse(text);
        assert_eq!(
            e,
            vec![
                ("artillery1".into(), "artillery2".into()),
                ("artillery1".into(), "sp_art".into())
            ]
        );
        let mut map = HashMap::new();
        for (p, c) in e {
            map.entry(c).or_insert_with(Vec::new).push(p);
        }
        let mut done = HashSet::new();
        assert!(!startable(&map, &done, "artillery2"));
        assert!(startable(&map, &done, "artillery1"));
        done.insert("artillery1".to_string());
        assert!(startable(&map, &done, "artillery2"));
        assert!(!startable(&map, &done, "artillery1"));
        let years = parse_years("technologies = {\n  a1 = {\n    start_year = 1936\n    path = { leads_to_tech = a2 }\n  }\n  a2 = { start_year = 1939 }\n}\n");
        assert_eq!(years, vec![("a1".into(), 1936), ("a2".into(), 1939)]);
        let metas = parse_meta("technologies = {\n  motor_rockets = {\n    dependencies = { motorised_infantry = 1 }\n    path = { leads_to_tech = x }\n  }\n  concentrated = {\n    XOR = { dispersed }\n  }\n  jet = {\n    allow = { ROOT = { is_special_project_completed = sp:jet } }\n  }\n}\n");
        let m: HashMap<String, Meta> = metas.into_iter().collect();
        assert_eq!(
            m["motor_rockets"].dependencies,
            vec!["motorised_infantry".to_string()]
        );
        assert_eq!(m["concentrated"].xor, vec!["dispersed".to_string()]);
        assert!(m["jet"].gated);
        let none = HashMap::new();
        let mut done = HashSet::new();
        assert!(!eligible(&none, &m, &done, "motor_rockets"));
        assert!(!eligible(&none, &m, &done, "jet"));
        assert!(eligible(&none, &m, &done, "concentrated"));
        done.insert("motorised_infantry".to_string());
        done.insert("dispersed".to_string());
        assert!(eligible(&none, &m, &done, "motor_rockets"));
        assert!(!eligible(&none, &m, &done, "concentrated"));
    }
}
