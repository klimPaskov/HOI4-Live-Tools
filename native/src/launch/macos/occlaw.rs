//! Default occupation law (the "Territory Management" law of the Occupied Territories view),
//! set the way a player does it: an occupied or resisting state's view, its occupation-law
//! button (opens the Occupied Territories view), the view's own law button (opens the
//! Default Law list), then the law's row. Verified from the country's occupation law in memory
//! (`CCountryOccupationStatus +0x108`, the key `hoi4_occupation` reports).
//!
//! The Default Law list is not reachable from the interface manager; its rows are clicked by
//! their fixed place at 1920x1080 and GUI scale 1 (mapped live 5 November 2026: the list's
//! first row at y 187, rows 38 apart). The laws and their order come from the game's own files
//! (`common/occupation_laws` of the content the game loaded, mods included, by `gui_order` and
//! then file order), and a row holds a law only when its `visible` trigger holds for the player:
//! a law's row is the number of shown laws before it. `visible` is judged with the focus rules'
//! trigger reader for the player's tag, the DLCs and the ruling ideology the country's history
//! gives (no memory read has the ruling party yet; the readback catches a change of government).
//! A law whose own row, or a row before it, cannot be judged (a trigger on another country, a
//! flag, a focus) is refused before any input. The readback decides.
use super::{
    stateslots,
    ui::{clear_notices, read, wait},
};
use crate::{
    error::{Error, Result},
    focus_rules::{live::Dlcs, Catalog, Snapshot},
    game, gui_tree,
    memory::Reader,
    plan_predicate::Truth,
    platform::{
        game_input::{GameInput, Key},
        Process,
    },
    script::{Block as _, Entry, Val},
    xlayout::{fld, Class},
};
use serde_json::{json, Value};
use std::path::Path;

/// A history date (year, month, day).
type Ymd = (i64, i64, i64);

const GAME: (u32, u32) = super::panelkit::GAME;

/// One occupation law of the game files.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Law {
    pub key: String,
    gui_order: f64,
    /// The `visible` trigger (None: always shown).
    visible: Option<Vec<Entry>>,
    /// state_modifier numbers the executor ranks the laws by.
    pub modifiers: Vec<(String, f64)>,
}

/// The laws of `common/occupation_laws` texts (files in name order), in the Default Law list's
/// order: `gui_order`, then file order. A key defined again later keeps its first place and
/// takes the later definition (the game reads the files in that order).
pub(super) fn parse_laws(files: &[String]) -> Vec<Law> {
    let mut out: Vec<Law> = Vec::new();
    for text in files {
        for entry in crate::script::parse(text) {
            let Val::Block(body) = &entry.val else {
                continue;
            };
            if entry.key.is_empty() {
                continue;
            }
            let law = Law {
                key: entry.key.clone(),
                gui_order: body.number("gui_order").unwrap_or(0.0),
                visible: body.find_val("visible").map(|v| v.block().to_vec()),
                modifiers: body
                    .find_val("state_modifier")
                    .map(|m| {
                        m.block()
                            .iter()
                            .filter_map(|e| Some((e.key.clone(), e.val.scalar()?.parse().ok()?)))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            match out.iter_mut().find(|l| l.key == law.key) {
                Some(old) => *old = law,
                None => out.push(law),
            }
        }
    }
    out.sort_by(|a, b| a.gui_order.total_cmp(&b.gui_order));
    out
}

/// The laws of the content folder (mods included), read once per folder.
pub(super) fn laws(dir: &Path) -> std::sync::Arc<Vec<Law>> {
    crate::script::cached(dir, "occupation_laws", |dir| {
        let mut paths: Vec<_> = std::fs::read_dir(dir.join("common/occupation_laws"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "txt"))
            .collect();
        paths.sort();
        let texts: Vec<String> = paths
            .iter()
            .filter_map(|p| std::fs::read(p).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .collect();
        parse_laws(&texts)
    })
}

/// The ruling ideology a country's history file gives at `date` (its top-level `set_politics`,
/// then the dated blocks up to the date; all of them when the date is unknown).
pub(super) fn history_government(text: &str, date: Option<(i64, i64, i64)>) -> Option<String> {
    let root = crate::script::parse(text);
    let party = |block: &[Entry]| {
        block
            .find_val("set_politics")
            .and_then(|p| p.block().text("ruling_party"))
            .map(str::to_string)
    };
    let mut found = party(&root);
    let mut dated: Vec<(Ymd, &[Entry])> = root
        .iter()
        .filter_map(|e| Some((crate::focus_rules::date(&e.key)?, e.val.block())))
        .collect();
    dated.sort_by_key(|(d, _)| *d);
    for (at, block) in dated {
        if date.is_some_and(|now| at > now) {
            break;
        }
        if let Some(p) = party(block) {
            found = Some(p);
        }
    }
    found
}

/// The ruling ideology of `tag` from `history/countries/<TAG> - <name>.txt` of the content.
fn government(dir: &Path, tag: &str, date: Option<(i64, i64, i64)>) -> Option<String> {
    let file = std::fs::read_dir(dir.join("history/countries"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| n.split(['-', ' ', '.']).next())
                .is_some_and(|t| t.trim() == tag)
        })?;
    history_government(&String::from_utf8_lossy(&std::fs::read(file).ok()?), date)
}

/// What the `visible` triggers can be judged with: the player's tag, the DLCs and the ruling
/// ideology; everything else is unknown.
struct Facts<'a> {
    known: Snapshot,
    dlcs: Option<&'a Dlcs>,
}
impl crate::focus_rules::Facts for Facts<'_> {
    fn player(&self) -> Option<String> {
        self.known.player()
    }
    fn date(&self) -> Option<(i64, i64, i64)> {
        self.known.date()
    }
    fn dlc(&self, name: &str) -> Truth {
        self.dlcs.map_or(Truth::Unknown, |d| d.has(name))
    }
    fn completed(&self, focus: &str) -> Truth {
        self.known.completed(focus)
    }
    fn in_tree(&self, focus: &str) -> Truth {
        self.known.in_tree(focus)
    }
    fn at_war(&self) -> Truth {
        self.known.at_war()
    }
    fn at_war_with(&self, tag: &str) -> Truth {
        self.known.at_war_with(tag)
    }
    fn exists(&self, tag: &str) -> Truth {
        self.known.exists(tag)
    }
    fn state(&self, id: i64) -> Option<crate::focus_rules::Holders> {
        self.known.state(id)
    }
    fn number(&self, name: &str) -> Option<(f64, f64)> {
        self.known.number(name)
    }
    fn tech(&self, key: &str) -> Truth {
        self.known.tech(key)
    }
    fn faction(&self) -> Option<Vec<String>> {
        self.known.faction()
    }
    fn overlord(&self) -> Option<Option<String>> {
        self.known.overlord()
    }
    fn subjects(&self) -> Option<Vec<String>> {
        self.known.subjects()
    }
    fn government(&self) -> Option<String> {
        self.known.government()
    }
}

/// Each law in list order with whether the game shows it to the player and what could not be
/// judged.
pub(super) fn shown(
    laws: &[Law],
    rules: &Catalog,
    known: Value,
    dlcs: Option<&Dlcs>,
) -> Vec<(String, Truth, Vec<String>)> {
    let facts = Facts {
        known: Snapshot(known),
        dlcs,
    };
    laws.iter()
        .map(|law| match &law.visible {
            None => (law.key.clone(), Truth::True, Vec::new()),
            Some(block) => {
                let (truth, notes) = rules.judge(block, &facts);
                (law.key.clone(), truth, notes)
            }
        })
        .collect()
}

/// The row of `law` in the Default Law list, or why it cannot be placed (before any input).
pub(super) fn row_of(rows: &[(String, Truth, Vec<String>)], law: &str) -> Result<usize> {
    let mut row = 0;
    for (key, truth, notes) in rows {
        let why = || {
            if notes.is_empty() {
                String::new()
            } else {
                format!(" ({})", notes.join("; "))
            }
        };
        if key == law {
            return match truth {
                Truth::True => Ok(row),
                Truth::False => Err(Error::new(
                    "law_not_shown",
                    format!("The game does not show {law} to this country (its visible trigger does not hold)"),
                )),
                Truth::Unknown => Err(Error::new(
                    "law_row_unknown",
                    format!("Whether the game shows {law} cannot be judged{}; nothing was clicked", why()),
                )),
            };
        }
        match truth {
            Truth::True => row += 1,
            Truth::False => {}
            Truth::Unknown => {
                return Err(Error::new(
                    "law_row_unknown",
                    format!("{law}'s row cannot be placed: whether the game shows {key} above it cannot be judged{}; nothing was clicked", why()),
                ))
            }
        }
    }
    Err(Error::new(
        "invalid_arguments",
        format!("No occupation law {law} in common/occupation_laws"),
    ))
}

/// The laws as hoi4_occupation lists them: list order, `shown` (true, false, or null when it
/// cannot be judged) and the state modifiers the ladder ranks by.
pub(super) fn law_rows(laws: &[Law], rows: &[(String, Truth, Vec<String>)]) -> Value {
    json!(laws
        .iter()
        .zip(rows)
        .map(|(law, (_, truth, notes))| {
            let mut v = json!({"key": law.key, "shown": match truth {
                Truth::True => json!(true),
                Truth::False => json!(false),
                Truth::Unknown => Value::Null,
            }});
            for (k, x) in &law.modifiers {
                if matches!(
                    k.as_str(),
                    "resistance_target" | "compliance_gain" | "required_garrison_factor"
                ) {
                    v[k.as_str()] = json!(x);
                }
            }
            if !notes.is_empty() {
                v["unknown"] = json!(notes);
            }
            v
        })
        .collect::<Vec<_>>())
}

/// The facts for `shown`: the player's tag, the game date and the ruling ideology the history
/// gives.
pub(super) fn known(dir: &Path, player: &str, date: Option<(i64, i64, i64)>) -> Value {
    let mut v = json!({"tag": player});
    if let Some((y, m, d)) = date {
        v["date"] = json!(format!("{y}.{m}.{d}"));
    }
    if let Some(g) = government(dir, player, date) {
        v["government"] = json!(g);
    }
    v
}
const ROW_X: f64 = 700.0;
const FIRST_ROW_Y: f64 = 187.0;
const ROW_STEP: f64 = 37.8;

const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
const MANAGER: Class = Class::new("idler.manager", 0);
const OCCUPATION_VIEW: Class = Class::new("occupation.view", 0);
const IDLER_MANAGER: u64 = 0x670;
const VIEW_SLOT: u64 = 0x120;
const VIEW_ROOT: u64 = 0x490;

/// The shown `select_law_button` of the Occupied Territories view's header, when the view is
/// on screen.
fn header_law_button(r: &Reader<'_>, image: u64) -> Result<Option<gui_tree::Rect>> {
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + fld(abi, &IDLER, IDLER_MANAGER)?)?;
    let view = r.u64(manager + fld(abi, &MANAGER, VIEW_SLOT)?)?;
    if view == 0 {
        return Ok(None);
    }
    let Some(root) = gui_tree::container(
        r,
        r.u64(view + fld(abi, &OCCUPATION_VIEW, VIEW_ROOT)?)?,
        image,
    )?
    else {
        return Ok(None);
    };
    let Some(header) = super::panelkit::find(r, image, root, "header", 1)? else {
        return Ok(None);
    };
    let Some(button) = super::panelkit::find(r, image, header, "select_law_button", 1)? else {
        return Ok(None);
    };
    if !gui_tree::shown(r, button)? {
        return Ok(None);
    }
    match gui_tree::rect(r, image, button, GAME) {
        Ok(rect) if rect.x >= 0 => Ok(Some(rect)),
        _ => Ok(None),
    }
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_occupation_law","description":"Set the DEFAULT occupation law for all occupied territories (the Occupied Territories view's Territory Management law) with ordinary input: an occupied or resisting state's view, its occupation-law button, the default-law button, the law's row. Verified from the country's occupation law in memory (hoi4_occupation player_occupation_law). `law` is a key of the game's common/occupation_laws (mods included); hoi4_occupation `laws` lists them in the list's order with whether the game shows each to you (its visible trigger: ideology, tag, DLC). The row is found from the laws shown above it: refused before any input with law_not_shown (the game hides it from you) or law_row_unknown (a visible trigger on that row or above it reads something no tool reads yet, such as another country, a flag or a focus; the ruling ideology is taken from the country's history). Harsher laws cut resistance and raise the garrison need; milder ones raise compliance slowly and need fewer garrison troops. A law the game disables for you is not applied: the readback refuses it with action_not_applied. `via_state` is an occupied or resisting state id (default: the first one hoi4_occupation lists). `state` (the law of that one state, not the default) is not yet built: refused with not_yet_built.","inputSchema":{"type":"object","properties":{"law":{"type":"string","pattern":"^[A-Za-z0-9_]+$","maxLength":96},"via_state":{"type":"integer","minimum":1,"maximum":4095},"state":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["law","request_id","lease_token"],"additionalProperties":false}})
}

/// Open the Default Law list and click row `row` (`row_of`). The caller verifies the law.
pub(super) fn set_default(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    row: usize,
) -> Result<()> {
    if read(process, header_law_button)?.is_none() {
        // The game's resistance alert opens the view of a resisting state with its law button
        // shown; a state opened from the map does not always show it.
        match read(process, |r, i| super::alerts::rect_of(r, i, "resistance"))? {
            Some([x, y, w, h]) => {
                clear_notices(process, keys)?;
                keys.click(GAME, (x + w / 2) as f64, (y + h / 2) as f64)?;
                std::thread::sleep(std::time::Duration::from_millis(900));
            }
            None => {
                stateslots::open_state_view(process, keys, state)?;
            }
        }
        let mut button = None;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while button.is_none() && std::time::Instant::now() < deadline {
            button = read(process, |r, i| {
                Ok(
                    match stateslots::state_view_widget(r, i, "select_law_button")? {
                        Some(w) => gui_tree::rect(r, i, w, GAME).ok().filter(|x| x.x >= 0),
                        None => None,
                    },
                )
            })?;
            if button.is_none() {
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
        }
        let Some(button) = button else {
            let shown = read(process, stateslots::state_view_name).ok().flatten();
            return Err(Error::new(
                "invalid_target",
                format!("State {state} shows no occupation law (not an occupied or resisting state?); state view shows {shown:?}"),
            ));
        };
        // Let the slide-in finish before clicking.
        std::thread::sleep(std::time::Duration::from_millis(300));
        let button = read(process, |r, i| {
            let w = stateslots::state_view_widget(r, i, "select_law_button")?
                .ok_or_else(|| Error::new("input_readback_failed", "State view closed"))?;
            gui_tree::rect(r, i, w, GAME)
        })
        .unwrap_or(button);
        let (x, y) = button.center();
        keys.click(GAME, x, y)?;
        wait(process, "occupied territories view", |r, i| {
            Ok(header_law_button(r, i)?.is_some())
        })?;
        std::thread::sleep(std::time::Duration::from_millis(400));
    }
    let header = read(process, header_law_button)?
        .ok_or_else(|| Error::new("input_readback_failed", "Occupied territories view closed"))?;
    let (x, y) = header.center();
    keys.click(GAME, x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(600));
    keys.click(GAME, ROW_X, FIRST_ROW_Y + ROW_STEP * row as f64)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}

/// Close the Occupied Territories view (Escape closes it with its list).
pub(super) fn close(process: &dyn Process, keys: &dyn GameInput) {
    if read(process, header_law_button).ok().flatten().is_some() {
        let _ = keys.press(Key::Escape);
    }
    let _ = clear_notices(process, keys);
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
hidden_law = { visible = { always = no } state_modifier = { resistance_target = 0.1 } }
no_garrison = { state_modifier = { resistance_target = 0.4 required_garrison_factor = -1 }
    visible = { if = { limit = { original_tag = BBB } NOT = { has_country_flag = x } } } }
democratic_law = { visible = { has_government = democratic } state_modifier = { resistance_target = 0.1 } }
oversight = { state_modifier = { compliance_gain = 0.0 } }
dlc_law = { visible = { has_dlc = "Some Pack" } }
fascist_law = { visible = { has_government = fascism } state_modifier = { resistance_target = -0.75 } }
neighbour_law = { visible = { FROM = { original_tag = CCC } } }
last_law = { }
"#;

    fn known(government: &str) -> Value {
        json!({"tag": "AAA", "government": government})
    }

    #[test]
    fn laws_come_from_the_files_in_gui_order_and_a_redefinition_keeps_its_place() {
        let mod_file = "early = { gui_order = -1 }\noversight = { state_modifier = { compliance_gain = 0.02 } }";
        let laws = parse_laws(&[FILE.to_string(), mod_file.to_string()]);
        let keys: Vec<&str> = laws.iter().map(|l| l.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "early",
                "hidden_law",
                "no_garrison",
                "democratic_law",
                "oversight",
                "dlc_law",
                "fascist_law",
                "neighbour_law",
                "last_law"
            ]
        );
        let oversight = laws.iter().find(|l| l.key == "oversight").unwrap();
        assert_eq!(
            oversight.modifiers,
            vec![("compliance_gain".to_string(), 0.02)]
        );
    }

    #[test]
    fn a_row_counts_the_laws_shown_above_it_and_unknown_rows_refuse_before_input() {
        let laws = parse_laws(&[FILE.to_string()]);
        let rules = Catalog::default();
        // a fascist country without the DLC: no_garrison, oversight, fascist_law
        let rows = shown(&laws, &rules, known("fascism"), None);
        assert_eq!(row_of(&rows, "no_garrison").unwrap(), 0);
        assert_eq!(row_of(&rows, "oversight").unwrap(), 1);
        assert_eq!(
            row_of(&rows, "democratic_law").unwrap_err().code,
            "law_not_shown"
        );
        assert_eq!(
            row_of(&rows, "hidden_law").unwrap_err().code,
            "law_not_shown"
        );
        // the DLC is not known: everything below it cannot be placed
        assert_eq!(
            row_of(&rows, "fascist_law").unwrap_err().code,
            "law_row_unknown"
        );
        assert_eq!(
            row_of(&rows, "no_such_law").unwrap_err().code,
            "law_row_unknown"
        );
        // without the DLC the fascist law is the third row; a law visible only for another
        // country's states stays unknown, and so does every row below it
        // an install whose dlc folder is listed and holds nothing: every DLC is off
        let install = std::env::temp_dir().join(format!("occlaw-dlcs-{}", std::process::id()));
        std::fs::create_dir_all(install.join("dlc")).unwrap();
        let none = Dlcs::read(&install, Some(&[]));
        let _ = std::fs::remove_dir_all(&install);
        let rows = shown(&laws, &rules, known("fascism"), Some(&none));
        assert_eq!(row_of(&rows, "fascist_law").unwrap(), 2);
        assert_eq!(
            row_of(&rows, "neighbour_law").unwrap_err().code,
            "law_row_unknown"
        );
        assert_eq!(
            row_of(&rows, "last_law").unwrap_err().code,
            "law_row_unknown"
        );
        // a democracy sees its own law in second place
        let rows = shown(&laws, &rules, known("democratic"), Some(&none));
        assert_eq!(row_of(&rows, "democratic_law").unwrap(), 1);
        assert_eq!(row_of(&rows, "oversight").unwrap(), 2);
        assert_eq!(
            row_of(&rows, "fascist_law").unwrap_err().code,
            "law_not_shown"
        );
        // the ruling ideology unknown: no row past the first ideology law can be placed
        let rows = shown(&laws, &rules, json!({"tag": "AAA"}), Some(&none));
        assert_eq!(row_of(&rows, "no_garrison").unwrap(), 0);
        assert_eq!(
            row_of(&rows, "oversight").unwrap_err().code,
            "law_row_unknown"
        );
        let listed = law_rows(&laws, &rows);
        // the file order: hidden_law, no_garrison, democratic_law, oversight, ...
        assert_eq!(listed[1]["key"], "no_garrison");
        assert_eq!(listed[1]["shown"], json!(true));
        assert_eq!(listed[1]["resistance_target"], json!(0.4));
        // the ideology law's trigger cannot be judged without the ruling ideology
        assert_eq!(listed[2]["shown"], Value::Null);
        assert_eq!(listed[0]["shown"], json!(false));
    }

    #[test]
    fn the_ruling_ideology_follows_the_history_up_to_the_date() {
        let text = "capital = 1\nset_politics = { ruling_party = neutrality last_election = \"1936.1.1\" }\n\
                    1939.1.1 = { set_politics = { ruling_party = fascism } }";
        assert_eq!(
            history_government(text, Some((1936, 1, 1))).as_deref(),
            Some("neutrality")
        );
        assert_eq!(
            history_government(text, Some((1939, 8, 14))).as_deref(),
            Some("fascism")
        );
        assert_eq!(history_government(text, None).as_deref(), Some("fascism"));
        assert_eq!(history_government("capital = 1", None), None);
    }
}
