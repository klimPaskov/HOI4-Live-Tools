//! A small, tolerant reader for Clausewitz script files (`key = value`,
//! `key = { ... }`, bare list items, `#` comments, quoted strings) used to
//! name what the live memory reads only identify by number or loc key:
//! doctrines, military-industrial organisations, agency upgrades.
//!
//! The reader never fails: malformed input yields whatever parsed up to the
//! problem, so a game update can degrade a catalogue but never a live read.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Val {
    Scalar(String),
    Block(Vec<Entry>),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    /// Empty for anonymous blocks (`{ ... }` inside a list of blocks).
    pub key: String,
    /// `=` for assignments; `>`, `<`, `>=`, `<=`, `!=` for comparisons;
    /// empty for bare list items.
    pub op: String,
    pub val: Val,
}

impl Val {
    pub(super) fn block(&self) -> &[Entry] {
        match self {
            Val::Block(entries) => entries,
            Val::Scalar(_) => &[],
        }
    }
    pub(super) fn scalar(&self) -> Option<&str> {
        match self {
            Val::Scalar(s) => Some(s),
            Val::Block(_) => None,
        }
    }
}

/// Lookups on a parsed block.
pub(super) trait Block {
    fn find_val(&self, key: &str) -> Option<&Val>;
    fn all<'a>(&'a self, key: &'a str) -> Box<dyn Iterator<Item = &'a Val> + 'a>;
    fn text(&self, key: &str) -> Option<&str>;
    fn number(&self, key: &str) -> Option<f64>;
    /// Bare words and scalar values listed in a `key = { a b c }` block.
    fn words(&self, key: &str) -> Vec<String>;
}

impl Block for [Entry] {
    fn find_val(&self, key: &str) -> Option<&Val> {
        self.iter().find(|e| e.key == key).map(|e| &e.val)
    }
    fn all<'a>(&'a self, key: &'a str) -> Box<dyn Iterator<Item = &'a Val> + 'a> {
        Box::new(self.iter().filter(move |e| e.key == key).map(|e| &e.val))
    }
    fn text(&self, key: &str) -> Option<&str> {
        self.find_val(key).and_then(Val::scalar)
    }
    fn number(&self, key: &str) -> Option<f64> {
        self.text(key).and_then(|t| t.parse().ok())
    }
    fn words(&self, key: &str) -> Vec<String> {
        let Some(Val::Block(entries)) = self.find_val(key) else {
            return Vec::new();
        };
        entries
            .iter()
            .filter(|e| !e.key.is_empty() && matches!(e.val, Val::Scalar(_)))
            .filter_map(|e| {
                if e.op.is_empty() {
                    Some(e.key.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    Open,
    Close,
    Op(String),
    Word(String),
}

fn lex(text: &str) -> Vec<Tok> {
    let mut out = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '#' => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            c if c.is_whitespace() || c == '\u{feff}' => i += 1,
            '{' => {
                out.push(Tok::Open);
                i += 1;
            }
            '}' => {
                out.push(Tok::Close);
                i += 1;
            }
            '=' | '<' | '>' | '!' => {
                let mut op = String::new();
                while i < chars.len() && matches!(chars[i], '=' | '<' | '>' | '!') {
                    op.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Op(op));
            }
            '"' => {
                i += 1;
                let mut word = String::new();
                while i < chars.len() && chars[i] != '"' {
                    word.push(chars[i]);
                    i += 1;
                }
                i += 1;
                out.push(Tok::Word(word));
            }
            _ => {
                let mut word = String::new();
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], '{' | '}' | '=' | '<' | '>' | '!' | '#' | '"')
                {
                    word.push(chars[i]);
                    i += 1;
                }
                out.push(Tok::Word(word));
            }
        }
    }
    out
}

fn parse_block(tokens: &[Tok], pos: &mut usize, top: bool) -> Vec<Entry> {
    let mut entries = Vec::new();
    while *pos < tokens.len() {
        match &tokens[*pos] {
            Tok::Close => {
                *pos += 1;
                if !top {
                    return entries;
                }
            }
            Tok::Open => {
                *pos += 1;
                let inner = parse_block(tokens, pos, false);
                entries.push(Entry {
                    key: String::new(),
                    op: String::new(),
                    val: Val::Block(inner),
                });
            }
            Tok::Op(_) => *pos += 1,
            Tok::Word(key) => {
                let key = key.clone();
                *pos += 1;
                if let Some(Tok::Op(op)) = tokens.get(*pos) {
                    let op = op.clone();
                    *pos += 1;
                    match tokens.get(*pos) {
                        Some(Tok::Open) => {
                            *pos += 1;
                            let inner = parse_block(tokens, pos, false);
                            entries.push(Entry {
                                key,
                                op,
                                val: Val::Block(inner),
                            });
                        }
                        Some(Tok::Word(value)) => {
                            let value = value.clone();
                            *pos += 1;
                            entries.push(Entry {
                                key,
                                op,
                                val: Val::Scalar(value),
                            });
                        }
                        _ => {}
                    }
                } else {
                    entries.push(Entry {
                        key,
                        op: String::new(),
                        val: Val::Scalar(String::new()),
                    });
                }
            }
        }
    }
    entries
}

pub(super) fn parse(text: &str) -> Vec<Entry> {
    let tokens = lex(text);
    let mut pos = 0;
    parse_block(&tokens, &mut pos, true)
}

/// Every `.txt` file below `dir` (sorted, recursive), parsed and concatenated.
pub(super) fn parse_dir(dir: &Path) -> Vec<Entry> {
    let mut files = Vec::new();
    collect(dir, &mut files, 0);
    files.sort();
    let mut out = Vec::new();
    for file in files {
        if let Ok(bytes) = std::fs::read(&file) {
            out.extend(parse(&String::from_utf8_lossy(&bytes)));
        }
    }
    out
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>, depth: u32) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                collect(&path, files, depth + 1);
            }
        } else if path.extension().is_some_and(|e| e == "txt") {
            files.push(path);
        }
    }
}

/// Parse a directory once per install and key; callers build their own
/// catalogue type from the entries.
pub(super) fn cached<T: Send + Sync + 'static>(
    dir: &Path,
    key: &'static str,
    build: impl FnOnce(&Path) -> T,
) -> Arc<T> {
    type Slot = Arc<dyn std::any::Any + Send + Sync>;
    static CACHE: OnceLock<Mutex<HashMap<(PathBuf, &'static str), Slot>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let slot = (dir.to_path_buf(), key);
    if let Some(found) = cache.lock().unwrap().get(&slot) {
        if let Ok(typed) = found.clone().downcast::<T>() {
            return typed;
        }
    }
    let built = Arc::new(build(dir));
    cache.lock().unwrap().insert(slot, built.clone());
    built
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_assignments_blocks_and_lists() {
        let text = r#"
        # comment
        mobile = {
            folder = land   # trailing
            xp_cost = 100
            tracks = { infantry armor }
            milestones = { { a = 1 } { b = 2 } }
            available = { date > 1937.1.1 }
        }
        "#;
        let top = parse(text);
        assert_eq!(top.len(), 1);
        let body = top[0].val.block();
        assert_eq!(body.text("folder"), Some("land"));
        assert_eq!(body.number("xp_cost"), Some(100.0));
        assert_eq!(body.words("tracks"), ["infantry", "armor"]);
        let milestones = body.find_val("milestones").unwrap().block();
        assert_eq!(milestones.len(), 2);
        assert_eq!(milestones[1].val.block().number("b"), Some(2.0));
        let cond = &body.find_val("available").unwrap().block()[0];
        assert_eq!((cond.key.as_str(), cond.op.as_str()), ("date", ">"));
        assert_eq!(cond.val.scalar(), Some("1937.1.1"));
    }

    #[test]
    fn quoted_strings_and_unspaced_operators() {
        let top = parse("a={b=\"x y\" c=2}");
        let body = top[0].val.block();
        assert_eq!(body.text("b"), Some("x y"));
        assert_eq!(body.number("c"), Some(2.0));
    }

    #[test]
    fn malformed_input_degrades_without_panicking() {
        for bad in ["", "}", "{", "a =", "a = {", "= = =", "a = { b = { c"] {
            let _ = parse(bad);
        }
        let top = parse("a = { b = 1 } } c = 2");
        assert_eq!(
            top.iter().find(|e| e.key == "c").unwrap().val.scalar(),
            Some("2")
        );
    }

    #[test]
    fn repeated_keys_are_all_reachable() {
        let top = parse("x = { add_trait = { token = a } add_trait = { token = b } }");
        let body = top[0].val.block();
        let tokens: Vec<_> = body
            .all("add_trait")
            .filter_map(|v| v.block().text("token"))
            .collect();
        assert_eq!(tokens, ["a", "b"]);
    }
}
