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
    }
}
