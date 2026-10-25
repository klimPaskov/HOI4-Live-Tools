//! `hoi4_country_status`, first slice: the player's top-bar numbers read from
//! the top bar's text boxes in GUI memory. Pure read: no input, no lease, no
//! cursor movement, no game pause. Everything here is the UI-rounded text the
//! game shows, not a modifier-exact value, and every field says so with
//! `validated:false` until the live checks V0/V1/V2/V3/V5/V11 of
//! `docs/COUNTRY_STATUS_WARS_DESIGN.md` are recorded.
//!
//! Provenance of the paths used (nothing new is guessed):
//! - `idler + 0x670` (interface manager) `+0x188` (top bar) `+0x68` (root
//!   container) and descent by widget name are the validated path of
//!   `decisions::topbar_button` (live clicks, 18 October 2026).
//! - the text string of a text box is the libc++ string at widget base
//!   `+0x100`, validated for event, diplomacy and politics windows but NOT yet
//!   for the top bar text boxes (design 3.1, check V0). A widget that is not a
//!   text box, or whose string fails the bounded libc++ checks, is reported
//!   unreadable instead of guessed.
use super::ui::read;
use crate::{
    economy,
    error::{Error, Result},
    game, gui_tree,
    memory::{Reader, StringAbi},
    platform::Process,
};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Interface manager in the idler, top bar in the manager, root in the top
/// bar: same values and evidence as `decisions::MACOS`.
const MANAGER: u64 = 0x670;
const TOPBAR: u64 = 0x188;
const TOPBAR_ROOT: u64 = 0x68;
/// libc++ string of a text box (unvalidated on the top bar, see module docs).
const TEXT: u64 = 0x100;
const TEXT_LIMIT: usize = 128;
const NODE_BUDGET: usize = 4096;
const DEPTH_LIMIT: u32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Shown as a percentage; returned as a 0..1 fraction.
    Fraction,
    /// Whole count; magnitude suffixes are expanded.
    Count,
    /// Any number with an optional unit (`29 d`, `1.0`, `+0.5`).
    Number,
}

/// Result field, the top-bar widget that shows it, and how to interpret it.
const FIELDS: &[(&str, &str, Kind)] = &[
    ("political_power", "pol_power", Kind::Number),
    ("stability", "stability", Kind::Fraction),
    ("war_support", "war_support", Kind::Fraction),
    ("manpower", "manpower", Kind::Count),
    ("fuel", "fuel_value", Kind::Number),
    ("supply", "supply_value", Kind::Number),
    ("convoys", "convoys_count", Kind::Count),
    ("world_tension", "threat_value", Kind::Fraction),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Plain,
    Percent,
    Days,
    Years,
}

#[derive(Clone, Debug, PartialEq)]
struct Parsed {
    /// Number as written, magnitude suffix applied, sign applied.
    value: f64,
    unit: Unit,
    /// Smallest step the shown digits resolve, in the same scale as `value`.
    precision: f64,
}

/// Strip the engine's colour (`\u{11}` + code) and icon (`\u{13}`) markup and
/// collapse all whitespace (including no-break and thin spaces).
fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\u{11}' => {
                chars.next();
            }
            '\u{13}' => {}
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse a top-bar string such as `81%`, `81 %`, `1.33M`, `261.62K`, `1 234`,
/// `1,234`, `1,5K`, `+0.5`, `-12`, `29 d`. Separator rules: with both `.` and
/// `,` the last one is the decimal mark; one kind repeated is a thousands
/// mark; a single separator is decimal when a magnitude suffix follows or
/// when it is not followed by exactly three digits; with exactly three digits
/// and no suffix a `,` is thousands and a `.` is decimal (English default).
fn parse(text: &str) -> Option<Parsed> {
    let text = clean(text).replace('\u{2212}', "-");
    let text = text.trim();
    let (negative, body) = match text.chars().next()? {
        '-' => (true, &text[1..]),
        '+' => (false, &text[1..]),
        _ => (false, text),
    };
    let body = body.trim_start();
    // Numeric part: digits and separators; a space joins digit groups only
    // when exactly three digits follow it.
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    let mut number = String::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' || c == ',' {
            number.push(c);
            i += 1;
        } else if c == ' '
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.len() >= i + 4
            && chars[i + 1..i + 4].iter().all(char::is_ascii_digit)
            && !chars.get(i + 4).is_some_and(|n| n.is_ascii_digit())
        {
            // Thousands group separator; keep as a marker for the rules below.
            number.push(' ');
            i += 1;
        } else {
            break;
        }
    }
    if number.is_empty() || !number.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let rest: String = chars[i..].iter().collect();
    let rest = rest.trim();
    let (multiplier, rest) = match rest.chars().next() {
        Some('K') | Some('k') => (1e3, rest[1..].trim_start()),
        Some('M') => (1e6, rest[1..].trim_start()),
        Some('B') => (1e9, rest[1..].trim_start()),
        _ => (1.0, rest),
    };
    let unit = match rest.to_ascii_lowercase().as_str() {
        "" => Unit::Plain,
        "%" => Unit::Percent,
        "d" | "day" | "days" => Unit::Days,
        "y" | "yr" | "yrs" | "year" | "years" => Unit::Years,
        _ => return None,
    };
    let (digits, decimals) = normalise(&number, multiplier != 1.0)?;
    let value: f64 = digits.parse().ok()?;
    let sign = if negative { -1.0 } else { 1.0 };
    Some(Parsed {
        value: sign * value * multiplier,
        unit,
        precision: 10f64.powi(-(decimals as i32)) * multiplier,
    })
}

/// `(plain decimal string, number of decimals)` of a separator-laden number.
fn normalise(number: &str, has_suffix: bool) -> Option<(String, usize)> {
    let spaced = number.contains(' ');
    let number = number.replace(' ', "");
    let dots = number.matches('.').count();
    let commas = number.matches(',').count();
    let (thousands, decimal): (Option<char>, Option<char>) = if dots > 0 && commas > 0 {
        let last_dot = number.rfind('.')?;
        let last_comma = number.rfind(',')?;
        let (decimal, thousands) = if last_dot > last_comma {
            ('.', ',')
        } else {
            (',', '.')
        };
        if number.matches(decimal).count() != 1 {
            return None;
        }
        (Some(thousands), Some(decimal))
    } else if dots + commas == 0 {
        (spaced.then_some(' '), None)
    } else {
        let sep = if dots > 0 { '.' } else { ',' };
        let count = dots + commas;
        if count > 1 {
            (Some(sep), None)
        } else {
            let after = number.len() - number.find(sep)? - 1;
            let before = number.find(sep)?;
            let grouped = after == 3 && !has_suffix && sep == ',' && (1..=3).contains(&before);
            if grouped || (spaced && after == 3) {
                (Some(sep), None)
            } else {
                (None, Some(sep))
            }
        }
    };
    let mut out = String::new();
    let mut decimals = 0;
    let mut seen_decimal = false;
    for c in number.chars() {
        if Some(c) == thousands {
            continue;
        }
        if Some(c) == decimal {
            out.push('.');
            seen_decimal = true;
            continue;
        }
        if seen_decimal {
            decimals += 1;
        }
        out.push(c);
    }
    // Thousands groups after the first must be exactly three digits.
    if let Some(t) = thousands.filter(|t| *t != ' ') {
        let groups: Vec<&str> = number.split(t).collect();
        let tail_ok = groups.iter().skip(1).enumerate().all(|(index, g)| {
            let digits = if index + 2 == groups.len() {
                g.split(decimal.unwrap_or('\0')).next().unwrap_or("")
            } else {
                g
            };
            digits.len() == 3
        });
        if !tail_ok || groups[0].is_empty() || groups[0].len() > 3 {
            return None;
        }
    }
    if out.starts_with('.') || out.ends_with('.') || out.is_empty() {
        return None;
    }
    Some((out, decimals))
}

/// What the memory read produced for one widget.
#[derive(Clone, Debug, PartialEq)]
enum Source {
    Missing,
    Unreadable,
    Text(String),
}

fn round_to(value: f64, precision: f64) -> f64 {
    let step = precision.clamp(1e-9, 1.0);
    let digits = (-step.log10()).ceil().clamp(0.0, 9.0) as i32;
    let scale = 10f64.powi(digits);
    (value * scale).round() / scale
}

fn field(widget: &str, kind: Kind, source: &Source) -> Value {
    let unavailable = |reason: &str, raw: Option<&str>| {
        json!({"value":null,"raw_text":raw,"reason":reason,"widget":widget,
            "source":"topbar_text","validated":false})
    };
    let raw = match source {
        Source::Missing => return unavailable("widget_not_found", None),
        Source::Unreadable => return unavailable("text_unreadable", None),
        Source::Text(raw) => raw.as_str(),
    };
    if clean(raw).is_empty() {
        return unavailable("text_empty", Some(raw));
    }
    let Some(p) = parse(raw) else {
        return unavailable("topbar_text_unparsed", Some(raw));
    };
    let display = clean(raw);
    let (value, unit, precision) = match kind {
        Kind::Fraction => {
            if p.unit != Unit::Percent {
                return unavailable("topbar_text_unparsed", Some(raw));
            }
            (
                round_to(p.value / 100.0, p.precision / 100.0),
                "fraction",
                p.precision / 100.0,
            )
        }
        Kind::Count => {
            if p.unit != Unit::Plain || p.value < 0.0 {
                return unavailable("topbar_text_unparsed", Some(raw));
            }
            (p.value.round(), "count", p.precision.max(1.0))
        }
        Kind::Number => {
            let unit = match p.unit {
                Unit::Plain => "number",
                Unit::Percent => "percent",
                Unit::Days => "days",
                Unit::Years => "years",
            };
            (round_to(p.value, p.precision), unit, p.precision)
        }
    };
    json!({"value":value,"unit":unit,"display":display,"raw_text":raw,"precision":precision,
        "source":"topbar_text","validated":false})
}

/// All requested fields plus the list of those without a value.
fn build_fields(texts: &BTreeMap<&str, Source>) -> (Map<String, Value>, Vec<Value>) {
    let mut fields = Map::new();
    let mut unavailable = Vec::new();
    for (name, widget, kind) in FIELDS {
        let source = texts.get(widget).unwrap_or(&Source::Missing);
        let value = field(widget, *kind, source);
        if value["value"].is_null() {
            unavailable
                .push(json!({"field":name,"code":"field_unavailable","reason":value["reason"]}));
        }
        fields.insert((*name).to_string(), value);
    }
    (fields, unavailable)
}

/// Widgets by name below `root`, breadth-bounded; first match wins.
fn locate(
    r: &Reader<'_>,
    image: u64,
    root: u64,
    wanted: &[&'static str],
) -> Result<(BTreeMap<&'static str, u64>, usize)> {
    let mut found = BTreeMap::new();
    let mut stack = vec![(root, 0u32)];
    let mut visited = 0;
    while let Some((container, depth)) = stack.pop() {
        for child in gui_tree::children(r, container)? {
            visited += 1;
            if visited > NODE_BUDGET {
                return Ok((found, visited));
            }
            if let Some(name) = gui_tree::name(r, child, image)? {
                if let Some(w) = wanted.iter().find(|w| **w == name) {
                    found.entry(*w).or_insert(child);
                    if found.len() == wanted.len() {
                        return Ok((found, visited));
                    }
                    continue;
                }
            }
            if depth < DEPTH_LIMIT {
                if let Some(inner) = gui_tree::container(r, child, image)? {
                    stack.push((inner, depth + 1));
                }
            }
        }
    }
    Ok((found, visited))
}

/// Text of one text box; only a torn read is transient. Anything else that
/// stops the bounded string checks means this widget is not a readable text.
fn text_of(r: &Reader<'_>, widget: u64) -> Result<Source> {
    match r.string_bounded(widget + TEXT, TEXT_LIMIT) {
        Ok(text) => Ok(Source::Text(text)),
        Err(e) if e.code == "torn_read" => Err(e),
        Err(_) => Ok(Source::Unreadable),
    }
}

fn topbar_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Top bar text is not reviewed for this build",
        ));
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + MANAGER)?;
    let topbar = if manager == 0 {
        0
    } else {
        r.u64(manager + TOPBAR)?
    };
    let root = if topbar == 0 {
        0
    } else {
        r.u64(topbar + TOPBAR_ROOT)?
    };
    if root == 0 {
        return Err(Error::new(
            "topbar_unmaterialized",
            "Top bar is not built (menu or game not loaded)",
        ));
    }
    gui_tree::container(r, root, image)?.ok_or_else(|| {
        Error::new(
            "topbar_unmaterialized",
            "Top bar root container unavailable",
        )
    })
}

struct Snapshot {
    texts: BTreeMap<&'static str, Source>,
    nodes: usize,
    context: game::ClockContext,
    reads: (u64, usize),
    build_id: &'static str,
}

fn snapshot(r: &Reader<'_>, image: u64) -> Result<Snapshot> {
    // Own-country join first: fails explicitly in menus and for unknown builds.
    economy::own_production(r, image)?;
    let context = game::context_from_reader(r, image)?;
    let root = topbar_root(r, image)?;
    let wanted: Vec<&'static str> = FIELDS.iter().map(|f| f.1).collect();
    let (found, nodes) = locate(r, image, root, &wanted)?;
    let mut texts = BTreeMap::new();
    for widget in wanted {
        let source = match found.get(widget) {
            Some(base) => text_of(r, *base)?,
            None => Source::Missing,
        };
        texts.insert(widget, source);
    }
    Ok(Snapshot {
        texts,
        nodes,
        context,
        reads: (r.calls.get(), r.bytes.get()),
        build_id: game::layout(r).build_id,
    })
}

pub(super) fn status(process: &dyn Process) -> Result<Value> {
    let started = Instant::now();
    let s = read(process, snapshot)?;
    let (fields, unavailable) = build_fields(&s.texts);
    Ok(json!({
        "source":"native own country status",
        "build_id":s.build_id,
        "owned":true,
        "player":s.context.player,
        "game_time":{"date_raw":s.context.date_raw,"paused":s.context.paused,"speed":s.context.speed},
        "fields":fields,
        "unavailable":unavailable,
        "topbar_nodes_visited":s.nodes,
        "text_offset_validated":false,
        "precision_note":"UI-rounded top-bar text; not modifier-exact values",
        "freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms":started.elapsed().as_secs_f64()*1000.0,"read_calls":s.reads.0,"read_bytes":s.reads.1},
        "observation_paused_game":false,
        "input_authorized":false
    }))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_country_status","description":"Own country status read from the top bar's text boxes in GUI memory: political power, stability, war support, manpower (free), fuel, supply, convoys and world tension. Pure read; no lease needed, no input, no cursor movement, never pauses. Every field is {value, unit, display, raw_text, precision, source:'topbar_text', validated:false}; values are the UI-rounded text the game shows (stability, war support and world tension as 0..1 fractions, manpower and convoys as counts). A field that cannot be read is value:null with a reason (widget_not_found, text_unreadable, text_empty, topbar_text_unparsed) and is listed in 'unavailable'. Fails with topbar_unmaterialized in menus. Unvalidated until the live checks of docs/COUNTRY_STATUS_WARS_DESIGN.md are recorded.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> Parsed {
        parse(text).unwrap_or_else(|| panic!("unparsed {text:?}"))
    }

    #[test]
    fn percent_forms() {
        assert_eq!(p("81%").value, 81.0);
        assert_eq!(p("81%").unit, Unit::Percent);
        assert_eq!(p("81 %").unit, Unit::Percent);
        assert_eq!(p("94,5 %").value, 94.5);
        assert_eq!(p("-5%").value, -5.0);
        assert_eq!(p("\u{2212}5%").value, -5.0);
    }

    #[test]
    fn magnitude_suffixes() {
        let m = p("1.33M");
        assert_eq!(m.value, 1_330_000.0);
        assert!((m.precision - 10_000.0).abs() < 1e-6);
        assert_eq!(p("261.62K").value, 261_620.0);
        assert_eq!(p("261,62K").value, 261_620.0);
        assert_eq!(p("12K").value, 12_000.0);
        assert_eq!(p("1,234K").value, 1234.0);
    }

    #[test]
    fn signs_and_plain_numbers() {
        assert_eq!(p("+0.5").value, 0.5);
        assert_eq!(p("-12").value, -12.0);
        assert_eq!(p("641").value, 641.0);
        assert_eq!(p("1.0").value, 1.0);
        assert_eq!(p("29 d").unit, Unit::Days);
        assert_eq!(p("29 days").value, 29.0);
        assert_eq!(p("1.3 y").value, 1.3);
        assert_eq!(p("\u{11}G1.3 y\u{11}!").value, 1.3);
    }

    #[test]
    fn localisation_separators() {
        assert_eq!(p("1,234").value, 1234.0);
        assert_eq!(p("1 234").value, 1234.0);
        assert_eq!(p("1\u{a0}234").value, 1234.0);
        assert_eq!(p("1\u{202f}234").value, 1234.0);
        assert_eq!(p("1,234,567").value, 1_234_567.0);
        assert_eq!(p("1.234,5").value, 1234.5);
        assert_eq!(p("1,234.5").value, 1234.5);
        assert_eq!(p("12,5").value, 12.5);
        assert_eq!(p("0,5").value, 0.5);
        assert_eq!(p("1.234").value, 1.234);
    }

    #[test]
    fn markup_is_stripped() {
        assert_eq!(p("\u{11}G81%").value, 81.0);
        assert_eq!(p("\u{13}12").value, 12.0);
    }

    #[test]
    fn rejects_unparseable_text() {
        for bad in [
            "",
            "   ",
            "abc",
            "%",
            "--5",
            "1,2,3.4,5",
            "12 apples",
            ".5",
            "5.",
            "1..2",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
    }

    fn texts(items: &[(&'static str, Source)]) -> BTreeMap<&'static str, Source> {
        items.iter().cloned().collect()
    }

    #[test]
    fn fields_carry_value_raw_source_and_unvalidated() {
        let t = texts(&[
            ("stability", Source::Text("81%".into())),
            ("war_support", Source::Text("5 %".into())),
            ("manpower", Source::Text("261.62K".into())),
            ("convoys_count", Source::Text("641".into())),
            ("fuel_value", Source::Text("29 d".into())),
            ("supply_value", Source::Text("1.0".into())),
            ("threat_value", Source::Text("12%".into())),
            ("pol_power", Source::Text("+13".into())),
        ]);
        let (fields, unavailable) = build_fields(&t);
        assert!(unavailable.is_empty());
        assert_eq!(fields["stability"]["value"], json!(0.81));
        assert_eq!(fields["stability"]["raw_text"], json!("81%"));
        assert_eq!(fields["war_support"]["value"], json!(0.05));
        assert_eq!(fields["manpower"]["value"], json!(261_620.0));
        assert_eq!(fields["convoys"]["value"], json!(641.0));
        assert_eq!(fields["fuel"]["unit"], json!("days"));
        assert_eq!(fields["world_tension"]["value"], json!(0.12));
        assert_eq!(fields["political_power"]["value"], json!(13.0));
        for (_, v) in &fields {
            assert_eq!(v["validated"], json!(false));
            assert_eq!(v["source"], json!("topbar_text"));
        }
    }

    #[test]
    fn missing_unreadable_and_unparsed_fields_are_null_with_a_reason() {
        let t = texts(&[
            ("stability", Source::Unreadable),
            ("war_support", Source::Text("n/a".into())),
            ("manpower", Source::Text("".into())),
            ("fuel_value", Source::Text("81".into())),
            ("threat_value", Source::Text("12".into())),
        ]);
        let (fields, unavailable) = build_fields(&t);
        assert_eq!(fields["stability"]["reason"], json!("text_unreadable"));
        assert_eq!(
            fields["war_support"]["reason"],
            json!("topbar_text_unparsed")
        );
        assert_eq!(fields["war_support"]["raw_text"], json!("n/a"));
        assert_eq!(fields["manpower"]["reason"], json!("text_empty"));
        assert_eq!(fields["convoys"]["reason"], json!("widget_not_found"));
        // A fraction field without a percent sign is not guessed.
        assert_eq!(
            fields["world_tension"]["reason"],
            json!("topbar_text_unparsed")
        );
        assert_eq!(fields["fuel"]["value"], json!(81.0));
        assert_eq!(fields["stability"]["value"], Value::Null);
        assert!(fields.values().all(|v| v["validated"] == json!(false)));
        assert!(unavailable.iter().any(|u| u["field"] == json!("convoys")));
        assert_eq!(unavailable.len(), 7);
    }

    #[test]
    fn tool_is_read_only_without_lease() {
        let tool = tool();
        assert_eq!(tool["name"], json!("hoi4_country_status"));
        assert!(tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .is_empty());
    }
}
