//! Localised display names from the install's English localisation files.
//!
//! The doctrine, agency and organisation windows show localised text only, so
//! a typed id (`new_mobile_warfare`, `upgrade_army_department`) has to be
//! joined to the text the screen carries before a row can be recognised.
//! Pure file reads; nothing here touches the game process.
use std::{collections::HashMap, path::Path};

/// Strip colour codes (`§Y`, `§!`), icon tokens (`£icon|1£`) and `$VAR$`
/// references from a localisation value, collapsing whitespace.
pub(super) fn clean(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '§' => {
                chars.next();
            }
            '£' => {
                for n in chars.by_ref() {
                    if n == '£' {
                        break;
                    }
                }
            }
            '$' => {
                for n in chars.by_ref() {
                    if n == '$' {
                        break;
                    }
                }
            }
            '\\' if chars.peek() == Some(&'n') => {
                chars.next();
                out.push(' ');
            }
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `KEY:0 "text"` lines of a localisation file, value not yet cleaned.
fn parse_raw(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    if t.starts_with('#') {
        return None;
    }
    let (key, rest) = t.split_once(':')?;
    if key.is_empty()
        || !key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-')
    {
        return None;
    }
    let rest = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.rfind('"')?;
    Some((key.to_string(), rest[..end].to_string()))
}
#[cfg(test)]
pub(super) fn parse_line(line: &str) -> Option<(String, String)> {
    parse_raw(line).map(|(k, v)| (k, clean(&v)))
}

/// `$name$` references of a raw value (plain identifiers only; formats such
/// as `$COUNT|H$` are runtime variables).
fn references(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = raw;
    while let Some(start) = rest.find('$') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('$') else {
            break;
        };
        let name = &after[..end];
        if !name.is_empty()
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
        {
            out.push(name.to_string());
        }
        rest = &after[end + 1..];
    }
    out
}
fn expand(raw: &str, table: &HashMap<String, String>, depth: u32) -> String {
    let mut out = raw.to_string();
    if depth == 0 {
        return out;
    }
    for name in references(raw) {
        if let Some(value) = table.get(&name) {
            out = out.replace(&format!("${name}$"), &expand(value, table, depth - 1));
        }
    }
    out
}

fn scan(dir: &Path, keys: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if keys.is_empty() {
        return out;
    }
    let Ok(entries) = std::fs::read_dir(dir.join("localisation/english")) else {
        return out;
    };
    let mut files: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    files.sort();
    for path in files {
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            if !keys.iter().any(|k| line.contains(k.as_str())) {
                continue;
            }
            if let Some((key, value)) = parse_raw(line) {
                if keys.contains(&key) {
                    out.entry(key).or_insert(value);
                }
            }
        }
        if out.len() == keys.len() {
            break;
        }
    }
    out
}

/// Display names of `keys` (first definition wins, `$other_key$` references
/// followed up to three levels) from `<game>/localisation/english`. Missing
/// keys are simply absent.
pub(super) fn lookup(dir: &Path, keys: &[&str]) -> HashMap<String, String> {
    let mut raw: HashMap<String, String> = HashMap::new();
    let mut asked: Vec<String> = Vec::new();
    let mut want: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
    for _ in 0..4 {
        want.retain(|k| !asked.contains(k));
        if want.is_empty() {
            break;
        }
        asked.extend(want.iter().cloned());
        let found = scan(dir, &want);
        let mut next = Vec::new();
        for (k, v) in found {
            for r in references(&v) {
                next.push(r);
            }
            raw.entry(k).or_insert(v);
        }
        want = next;
    }
    keys.iter()
        .filter_map(|k| {
            raw.get(*k)
                .map(|v| (k.to_string(), clean(&expand(v, &raw, 3))))
                .filter(|(_, v)| !v.is_empty())
        })
        .collect()
}

/// Case and punctuation insensitive comparison of an on-screen text with an
/// expected display name.
pub(super) fn same(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(|c| c.to_lowercase())
            .collect()
    }
    let (a, b) = (norm(a), norm(b));
    !a.is_empty() && a == b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_localisation_lines_and_strips_markup() {
        assert_eq!(
            parse_line(" GRAND_DOCTRINE_MOBILE_WARFARE: \"Mobile Warfare\""),
            Some(("GRAND_DOCTRINE_MOBILE_WARFARE".into(), "Mobile Warfare".into()))
        );
        assert_eq!(
            parse_line("  upgrade_economy_civilian:0 \"§HEconomy§!/civilian\\nsecond £x|1£ $V$ line\""),
            Some((
                "upgrade_economy_civilian".into(),
                "Economy/civilian second line".into()
            ))
        );
        assert_eq!(parse_line("# comment: \"x\""), None);
        assert_eq!(parse_line("l_english:"), None);
    }

    #[test]
    fn same_ignores_case_and_punctuation() {
        assert!(same("Mobile  Warfare", "mobile-warfare"));
        assert!(!same("", ""));
        assert!(!same("Mobile Warfare", "Mass Assault"));
    }

    #[test]
    fn references_are_followed() {
        let dir = std::env::temp_dir().join(format!("hoi4-loc-ref-{}", std::process::id()));
        let loc = dir.join("localisation/english");
        std::fs::create_dir_all(&loc).unwrap();
        std::fs::write(
            loc.join("a_l_english.yml"),
            "l_english:\n GER_porsche_organization: \"$porsche$\"\n NOR_x: \"$GER_porsche_organization$ Norge\"\n porsche: \"Porsche\"\n",
        )
        .unwrap();
        let got = lookup(&dir, &["GER_porsche_organization", "NOR_x"]);
        assert_eq!(got.get("GER_porsche_organization").map(String::as_str), Some("Porsche"));
        assert_eq!(got.get("NOR_x").map(String::as_str), Some("Porsche Norge"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lookup_reads_the_first_definition_of_each_key() {
        let dir = std::env::temp_dir().join(format!("hoi4-loc-{}", std::process::id()));
        let loc = dir.join("localisation/english");
        std::fs::create_dir_all(&loc).unwrap();
        std::fs::write(
            loc.join("a_l_english.yml"),
            "l_english:\n AAA: \"First\"\n BBB: \"Other\"\n",
        )
        .unwrap();
        std::fs::write(loc.join("b_l_english.yml"), "l_english:\n AAA: \"Second\"\n").unwrap();
        let got = lookup(&dir, &["AAA", "ZZZ"]);
        assert_eq!(got.get("AAA").map(String::as_str), Some("First"));
        assert!(!got.contains_key("ZZZ"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
