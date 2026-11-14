//! Script names of an event's options, joined to the texts the event window shows.
//!
//! The event window carries display texts only ("We need to be methodical and deliberate"),
//! while a plan names an option the way the event script does (`<event>.b`). The script names
//! come from the content's `events/*.txt` (the event's `option = { name = ... }` blocks in file
//! order) and their texts from the English localisation; each shown option gets the one script
//! name whose text it carries. Pure file reads, cached per event; nothing touches the game.
use super::loc;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

#[derive(Clone, Copy, PartialEq)]
enum Token<'a> {
    Word(&'a str),
    Equals,
    Open,
    Close,
}

/// Clausewitz tokens: words (quotes removed), `=`, `{`, `}`; comments run to the line end.
fn tokens(text: &str) -> Vec<Token<'_>> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'=' => {
                out.push(Token::Equals);
                i += 1;
            }
            b'{' => {
                out.push(Token::Open);
                i += 1;
            }
            b'}' => {
                out.push(Token::Close);
                i += 1;
            }
            b'"' => {
                let start = i + 1;
                i = start;
                while i < bytes.len() && bytes[i] != b'"' {
                    i += 1;
                }
                out.push(Token::Word(&text[start..i.min(bytes.len())]));
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'=' | b'{' | b'}' | b'#' | b'"')
                {
                    i += 1;
                }
                // `<`, `>` comparisons never name an event or an option; keep them as words
                out.push(Token::Word(&text[start..i]));
            }
        }
    }
    out
}

/// The option names of event `event` in one events file, in file order; None when the file
/// does not define the event.
fn options_in(text: &str, event: &str) -> Option<Vec<String>> {
    // blocks open above the current token: their keys ("country_event", "option", ...)
    let mut stack: Vec<&str> = Vec::new();
    let (mut key, mut assigned) = (None::<&str>, false);
    let (mut id, mut names): (Option<&str>, Vec<String>) = (None, Vec::new());
    let mut named = false;
    for token in tokens(text) {
        match token {
            Token::Word(word) => {
                if let (Some(k), true) = (key, assigned) {
                    match (stack.len(), k) {
                        (1, "id") => id = Some(word),
                        (2, "name") if stack[1] == "option" && !named => {
                            names.push(word.to_string());
                            named = true;
                        }
                        _ => {}
                    }
                    key = None;
                } else {
                    key = Some(word);
                }
                assigned = false;
            }
            Token::Equals => assigned = true,
            Token::Open => {
                stack.push(if assigned { key.unwrap_or("") } else { "" });
                if stack.len() == 2 {
                    named = false;
                }
                (key, assigned) = (None, false);
            }
            Token::Close => {
                // an option without a name still takes its place in the order
                if stack.len() == 2 && stack[1] == "option" && !named {
                    names.push(String::new());
                }
                stack.pop();
                if stack.is_empty() {
                    if id == Some(event) {
                        return Some(names);
                    }
                    (id, names) = (None, Vec::new());
                }
                (key, assigned) = (None, false);
            }
        }
    }
    None
}

/// Letters and digits of a text, lower case.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether the shown text is this localisation text. A `[Scope.GetName]` command stands for
/// any text: the parts around the commands must appear in order, the first at the start and the
/// last at the end.
fn shows(localised: &str, shown: &str) -> bool {
    let shown = plain(shown);
    let mut parts: Vec<String> = Vec::new();
    let mut rest = localised;
    let mut commands = 0;
    while let Some(open) = rest.find('[') {
        let Some(close) = rest[open..].find(']') else {
            break;
        };
        parts.push(plain(&rest[..open]));
        rest = &rest[open + close + 1..];
        commands += 1;
    }
    parts.push(plain(rest));
    if commands == 0 {
        return !shown.is_empty() && parts[0] == shown;
    }
    if parts.iter().map(String::len).sum::<usize>() < 4 {
        return false;
    }
    let last = parts.len() - 1;
    let mut at = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let found = if i == 0 {
            shown.starts_with(part.as_str()).then_some(0)
        } else if i == last {
            (shown.len() >= at + part.len() && shown.ends_with(part.as_str()))
                .then(|| shown.len() - part.len())
        } else {
            shown[at..].find(part.as_str()).map(|p| p + at)
        };
        match found {
            Some(p) => at = p + part.len(),
            None => return false,
        }
    }
    true
}

/// For each shown option the script name it belongs to. `script` are the event's option names
/// in file order with their localised texts (None: no localisation, the name is the text).
fn assign(script: &[(String, Option<String>)], shown: &[String]) -> Vec<Option<String>> {
    let mut keys: Vec<Option<String>> = shown
        .iter()
        .map(|text| {
            let mut hits = script.iter().filter(|(name, localised)| {
                !name.is_empty() && shows(localised.as_deref().unwrap_or(name), text)
            });
            match (hits.next(), hits.next()) {
                (Some((name, _)), None) => Some(name.clone()),
                _ => None,
            }
        })
        .collect();
    // two shown options never share a name
    for i in 0..keys.len() {
        if keys[i].is_some() && keys.iter().filter(|k| **k == keys[i]).count() > 1 {
            let name = keys[i].clone();
            for k in keys.iter_mut().filter(|k| **k == name) {
                *k = None;
            }
        }
    }
    // Every scripted option is shown: the order is the file order, so a text with runtime
    // values keeps its place.
    if script.len() == shown.len() {
        for (i, (name, _)) in script.iter().enumerate() {
            if keys[i].is_none() && !name.is_empty() && !keys.contains(&Some(name.clone())) {
                keys[i] = Some(name.clone());
            }
        }
    }
    keys
}

fn script_options(dir: &Path, event: &str) -> Vec<(String, Option<String>)> {
    let Ok(entries) = std::fs::read_dir(dir.join("events")) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.sort();
    let names = files
        .iter()
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("txt"))
        .filter_map(|p| std::fs::read(p).ok())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .filter(|text| text.contains(event))
        .find_map(|text| options_in(&text, event))
        .unwrap_or_default();
    let wanted: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|n| !n.is_empty())
        .collect();
    let texts = loc::lookup(dir, &wanted);
    names
        .iter()
        .map(|n| (n.clone(), texts.get(n).cloned()))
        .collect()
}

/// The script option name of every shown option of `event` (None where it cannot be told),
/// from the content folder `dir` (the install, or the merged mod layer).
pub(super) fn option_keys(dir: &Path, event: &str, shown: &[String]) -> Vec<Option<String>> {
    type Cache = HashMap<(PathBuf, String), Vec<(String, Option<String>)>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (dir.to_path_buf(), event.to_string());
    let known = cache.lock().ok().and_then(|c| c.get(&key).cloned());
    let script = known.unwrap_or_else(|| {
        let read = script_options(dir, event);
        if let Ok(mut c) = cache.lock() {
            if c.len() > 512 {
                c.clear();
            }
            c.insert(key, read.clone());
        }
        read
    });
    assign(&script, shown)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
add_namespace = war_events
country_event = { # the first event
	id = war_events.01
	title = war_events.01.t
	trigger = { has_war_with = ETH name = not_an_option }
	option = { # by any means
		name = war_events.01.a
		ai_chance = { factor = 1 }
		hidden_effect = { country_event = { id = war_events.02 days = 1 } }
	}
	option = {
		trigger = { always = yes }
		name = "war_events.01.b"
	}
}
country_event = {
	id = war_events.02
	option = { name = war_events.02.a }
	option = { add_political_power = 5 }
	option = { name = war_events.02.c }
}
"#;

    fn script(rows: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        rows.iter()
            .map(|(n, t)| (n.to_string(), t.map(str::to_string)))
            .collect()
    }

    fn shown(rows: &[&str]) -> Vec<String> {
        rows.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn option_names_come_from_the_event_block_in_file_order() {
        assert_eq!(
            options_in(FILE, "war_events.01"),
            Some(vec!["war_events.01.a".into(), "war_events.01.b".into()])
        );
        // an option without a name keeps its place; a nested event id is not the block's id
        assert_eq!(
            options_in(FILE, "war_events.02"),
            Some(vec![
                "war_events.02.a".into(),
                String::new(),
                "war_events.02.c".into()
            ])
        );
        assert_eq!(options_in(FILE, "war_events.03"), None);
        assert_eq!(options_in(FILE, "war_events.0"), None);
    }

    #[test]
    fn shown_texts_get_the_script_name_whose_localisation_they_carry() {
        let script = script(&[
            ("e.1.a", Some("By any means necessary")),
            ("e.1.b", Some("We need to be methodical and deliberate")),
        ]);
        assert_eq!(
            assign(
                &script,
                &shown(&[
                    "By any means necessary",
                    "We need to be methodical and deliberate"
                ])
            ),
            vec![Some("e.1.a".to_string()), Some("e.1.b".to_string())]
        );
        // a hidden first option: the one shown text still gets its own name, not the first
        assert_eq!(
            assign(
                &script,
                &shown(&["We need to be methodical and deliberate."])
            ),
            vec![Some("e.1.b".to_string())]
        );
    }

    #[test]
    fn runtime_values_in_a_text_match_any_words_and_the_order_fills_the_rest() {
        let script = script(&[
            (
                "e.4.a",
                Some("Send an ultimatum, demanding total subjugation"),
            ),
            (
                "e.4.b",
                Some("We can deal with [AFA.GetNameDef] later. Now is not the time."),
            ),
        ]);
        assert_eq!(
            assign(
                &script,
                &shown(&[
                    "Send an ultimatum, demanding total subjugation",
                    "We can deal with the Sultanate of Aussa later. Now is not the time."
                ])
            ),
            vec![Some("e.4.a".to_string()), Some("e.4.b".to_string())]
        );
        assert!(shows("[ROOT.GetLeader] must go", "Mussolini must go"));
        assert!(!shows("[ROOT.GetLeader] must go", "Mussolini must stay"));
        assert!(!shows("[A] [B]", "anything"));
        // nothing matches by text, but every scripted option is shown: file order
        let dynamic = script_rows_without_text();
        assert_eq!(
            assign(&dynamic, &shown(&["Yes", "No"])),
            vec![Some("e.9.a".to_string()), Some("e.9.b".to_string())]
        );
        // one option hidden and no text match: nothing is guessed
        assert_eq!(assign(&dynamic, &shown(&["No"])), vec![None]);
    }

    fn script_rows_without_text() -> Vec<(String, Option<String>)> {
        script(&[("e.9.a", None), ("e.9.b", None)])
    }

    #[test]
    fn equal_texts_are_told_apart_by_order_only_when_all_are_shown() {
        let script = script(&[("e.2.a", Some("OK")), ("e.2.b", Some("OK"))]);
        assert_eq!(
            assign(&script, &shown(&["OK", "OK"])),
            vec![Some("e.2.a".to_string()), Some("e.2.b".to_string())]
        );
        assert_eq!(assign(&script, &shown(&["OK"])), vec![None]);
    }

    #[test]
    fn reads_the_event_and_its_localisation_from_a_content_folder() {
        let dir = std::env::temp_dir().join(format!("hoi4-event-options-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("events")).unwrap();
        std::fs::create_dir_all(dir.join("localisation/english")).unwrap();
        std::fs::write(dir.join("events/war.txt"), FILE).unwrap();
        std::fs::write(
            dir.join("localisation/english/war_l_english.yml"),
            "l_english:\n war_events.01.a:0 \"By any means necessary\"\n war_events.01.b:0 \"We need to be §Ymethodical§! and deliberate\"\n",
        )
        .unwrap();
        let keys = option_keys(
            &dir,
            "war_events.01",
            &shown(&[
                "By any means necessary",
                "We need to be methodical and deliberate",
            ]),
        );
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            keys,
            vec![
                Some("war_events.01.a".to_string()),
                Some("war_events.01.b".to_string())
            ]
        );
        assert_eq!(
            option_keys(Path::new("/nonexistent"), "x.1", &shown(&["A"])),
            vec![None]
        );
    }
}
