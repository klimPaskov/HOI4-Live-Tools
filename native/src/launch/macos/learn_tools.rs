//! The learning tools: inspect any open window, act on one of its widgets by id with a read-back
//! of what changed, and keep what worked as a named skill (and plain facts) for later sessions
//! and plans. Built for panels no typed tool knows: a total-conversion mod's mechanics, a new DLC,
//! a patch. See crate::learn for the model, the .gui index and the store.
//!
//! Windows are enumerated the way the probe readers `debug_popups` and `debug_widget_anywhere`
//! found them: every pointer in the first 0x400 bytes of the interface manager (the views) and
//! every window of the game GUI's pop-up list, each with its root container at one of the root
//! offsets those probes saw (0x490 first). A root found under another root is a sub-window and
//! stays part of the outer window. LIVE SEAMS (see the learning plan's live probes): a scripted
//! window that the game keeps outside these two lists (CScriptedWindowManager) is not seen yet;
//! tooltips are given from the files (`pdx_tooltip` text), the game's live tooltip is not read;
//! rectangles exist for icons, buttons and check boxes only (other widgets carry no `at`).
//!
//! Fair play: only shown widgets with a rectangle are clicked, with ordinary input under the
//! input lease; a click whose scripted effect spends something or cannot be undone, and any
//! answer to a pop-up, is refused before input unless the caller says so.
use super::{alerts, country_status, kit, loc, panelkit, resources, ui::read};
use crate::{
    error::{Error, Result},
    game, gui_tree,
    learn::{
        gui_index::{self, GuiIndex},
        skills::{self, LearnedOn, Skill, Store},
        uitree::{self, Ids, Widget, Window},
    },
    memory::{Reader, StringAbi},
    platform::{game_input::GameInput, Process},
    xlayout::{self, fld},
};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub(super) const INSPECT: &str = "hoi4_ui_inspect";
pub(super) const ACT: &str = "hoi4_ui_act";
pub(super) const RUN: &str = "hoi4_skill_run";
/// Tools that only read the game or the store: no lease, no input.
pub(super) const READS: [&str; 7] = [
    INSPECT,
    "hoi4_skill_list",
    "hoi4_skill_show",
    "hoi4_skill_save",
    "hoi4_skill_retire",
    "hoi4_fact_save",
    "hoi4_fact_list",
];

const MANAGER_SCAN: usize = 0x400;
/// Root container offsets seen on window objects (debug_widget_anywhere, debug_popups).
const ROOT_OFFSETS: [u64; 13] = [
    0x490, 0x68, 0x50, 0x60, 0x70, 0x78, 0x430, 0x20, 0x28, 0x30, 0x8, 0x10, 0x18,
];
const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
const GAME_GUI: xlayout::Class = xlayout::Class::new("idler.gui", 0);
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0xa70;
const GRID_VTABLE: u64 = 0x36cca80;
const MAX_DEPTH: u32 = 14;
const WIDGET_BUDGET: usize = 6000;

fn plausible(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}

/// Window objects: the interface manager's views and the pop-up list's windows.
fn holders(r: &Reader<'_>, image: u64) -> Result<Vec<(u64, &'static str)>> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "capability_unavailable",
            "Window enumeration is mapped for the macOS build only",
        ));
    }
    let mut out = Vec::new();
    let manager = kit::manager(r, image)?;
    for c in r.read(manager, MANAGER_SCAN)?.chunks_exact(8) {
        let p = u64::from_le_bytes(c.try_into().unwrap());
        if plausible(p) {
            out.push((p, "view"));
        }
    }
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + fld(abi, &IDLER, GUI_FROM_IDLER)?)?;
    if plausible(gui) {
        if let Ok((_, data, count)) = r.array_header(gui + fld(abi, &GAME_GUI, POPUP_LIST)?) {
            for i in 0..count.min(64) as u64 {
                if let Ok(w) = r.u64(data + i * 8) {
                    if plausible(w) {
                        out.push((w, "popup"));
                    }
                }
            }
        }
    }
    Ok(out)
}

/// Root containers held by window objects, once each (the first holder names the source).
pub(super) fn roots(
    r: &Reader<'_>,
    image: u64,
    holders: &[(u64, &'static str)],
) -> Vec<(u64, &'static str)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for (holder, source) in holders {
        for off in ROOT_OFFSETS {
            let Ok(p) = r.u64(holder + off) else { continue };
            if !plausible(p) {
                continue;
            }
            if let Ok(Some(root)) = gui_tree::container(r, p, image) {
                if seen.insert(root) {
                    out.push((root, *source));
                }
            }
        }
    }
    out
}

fn is_grid(r: &Reader<'_>, image: u64, base: u64) -> bool {
    xlayout::img(r.string_abi(), GRID_VTABLE)
        .ok()
        .and_then(|vt| r.u64(base).ok().map(|p| p == image + vt))
        .unwrap_or(false)
}

/// Every widget below one root, depth first, with ids from `uitree::Ids`. `kind` gives the
/// `.gui` kind of an element name in this window; containers walked are added to `inner`.
pub(super) fn walk(
    r: &Reader<'_>,
    image: u64,
    root: u64,
    window: &str,
    kind: &dyn Fn(&str, &str) -> Option<String>,
    inner: &mut HashSet<u64>,
    budget: &mut usize,
) -> Vec<Widget> {
    let mut ids = Ids::default();
    let mut out = Vec::new();
    let mut stack = vec![(root, 0u32, window.to_string())];
    while let Some((obj, depth, parent)) = stack.pop() {
        let Ok(children) = gui_tree::children(r, obj) else {
            continue;
        };
        for (k, child) in children.into_iter().enumerate() {
            if *budget == 0 {
                return out;
            }
            *budget -= 1;
            let name = gui_tree::name(r, child, image)
                .ok()
                .flatten()
                .unwrap_or_default();
            let id = ids.child(&parent, &name, k);
            let class = gui_tree::class_of(r, image, child)
                .or_else(|| is_grid(r, image, child).then_some("grid"));
            let file_kind = kind(window, &name);
            let state = matches!(class, Some("button" | "checkbox"))
                .then(|| gui_tree::button_state(r, child).ok())
                .flatten();
            let text = (file_kind.as_deref() == Some("text"))
                .then(|| kit::long_text(r, child).ok())
                .flatten()
                .filter(|t| !t.trim().is_empty())
                .map(|t| t.chars().take(300).collect());
            out.push(Widget {
                id: id.clone(),
                name: name.clone(),
                kind: class.map(str::to_string).or(file_kind.clone()),
                at: gui_tree::rect(r, image, child, kit::GAME)
                    .ok()
                    .map(|x| [x.x, x.y, x.w, x.h]),
                shown: gui_tree::shown(r, child).unwrap_or(false),
                state,
                text,
                script: None,
            });
            if depth >= MAX_DEPTH {
                continue;
            }
            if let Ok(Some(c)) = gui_tree::container(r, child, image) {
                inner.insert(c);
                stack.push((c, depth + 1, id.clone()));
            }
            if class == Some("grid") || file_kind.as_deref() == Some("grid") {
                if let Ok(items) = panelkit::items(r, image, child) {
                    for (j, item) in items.iter().enumerate() {
                        inner.insert(item.frame);
                        stack.push((item.frame, depth + 1, format!("{id}/#{j}")));
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// Every open window (with `hidden`, also the ones the game keeps hidden) as widgets.
pub(super) fn scan(
    r: &Reader<'_>,
    image: u64,
    index: &GuiIndex,
    hidden: bool,
) -> Result<Vec<Window>> {
    let found = roots(r, image, &holders(r, image)?);
    let kind = |w: &str, n: &str| index.element(w, n).map(|e| e.kind.clone());
    let mut budget = WIDGET_BUDGET;
    let mut inner = HashSet::new();
    let mut out = Vec::new();
    for (root, source) in found {
        let visible = gui_tree::visible(r, root).unwrap_or(false);
        if !visible && !hidden {
            continue;
        }
        let id = panelkit::container_name(r, image, root)
            .ok()
            .flatten()
            .unwrap_or_else(|| "_unnamed".into());
        let widgets = walk(r, image, root, &id, &kind, &mut inner, &mut budget);
        out.push((
            root,
            Window {
                id,
                source: source.into(),
                visible,
                widgets,
            },
        ));
    }
    let mut windows: Vec<Window> = out
        .into_iter()
        .filter(|(root, _)| !inner.contains(root))
        .map(|(_, w)| w)
        .collect();
    uitree::unique_windows(&mut windows);
    Ok(windows)
}

fn content(process: &dyn Process) -> Result<std::path::PathBuf> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    Ok(super::game_content(exe)?.content)
}

fn index(process: &dyn Process) -> Result<(Arc<GuiIndex>, std::path::PathBuf)> {
    let dir = content(process)?;
    Ok((
        crate::script::cached(&dir, "learn_gui_index", gui_index::load),
        dir,
    ))
}

/// The open windows with what the files say about each widget (tooltip text, scripted effect,
/// cost, irreversibility).
pub(super) fn windows(process: &dyn Process, hidden: bool) -> Result<Vec<Window>> {
    let (index, dir) = index(process)?;
    let mut ws = read(process, |r, i| scan(r, i, &index, hidden))?;
    let keys: Vec<String> = ws
        .iter()
        .flat_map(|w| {
            w.widgets.iter().filter_map(|x| {
                index
                    .element(gui_index::plain(&w.id), &x.name)
                    .and_then(|e| e.tooltip_key.clone())
            })
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let texts = if refs.is_empty() {
        HashMap::new()
    } else {
        loc::lookup(&dir, &refs)
    };
    for w in &mut ws {
        let wid = w.id.clone();
        for x in &mut w.widgets {
            x.script = index.note(&wid, &x.id, &x.name, &texts);
        }
    }
    Ok(ws)
}

fn opt_bool(args: &Map<String, Value>, key: &str) -> Result<bool> {
    match args.get(key) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(Error::new(
            "invalid_arguments",
            format!("{key} must be true or false"),
        )),
    }
}
fn opt_str<'a>(args: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>> {
    match args.get(key) {
        None => Ok(None),
        Some(Value::String(s)) if !s.trim().is_empty() && s.len() <= 400 => Ok(Some(s.trim())),
        Some(_) => Err(Error::new(
            "invalid_arguments",
            format!("{key} must be a non-empty string"),
        )),
    }
}
fn need_str<'a>(args: &'a Map<String, Value>, key: &str, example: &str) -> Result<&'a str> {
    opt_str(args, key)?.ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            format!("{key} is required (e.g. {example})"),
        )
    })
}
fn opt_int(args: &Map<String, Value>, key: &str, lo: i64, hi: i64, default: i64) -> Result<i64> {
    match args.get(key) {
        None => Ok(default),
        Some(v) => v.as_i64().filter(|n| (lo..=hi).contains(n)).ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                format!("{key} must be a whole number from {lo} to {hi}"),
            )
        }),
    }
}

/// `hoi4_ui_inspect`.
pub(super) fn inspect(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    super::allowed(args, &["window", "all", "hidden_windows", "max"])?;
    let under = opt_str(args, "window")?;
    let all = opt_bool(args, "all")?;
    let hidden = opt_bool(args, "hidden_windows")?;
    let max = opt_int(args, "max", 20, 3000, 400)? as usize;
    let ws = windows(process, hidden)?;
    if let Some(u) = under {
        let top = u.split('/').next().unwrap_or(u);
        if !ws.iter().any(|w| w.id == top) {
            return Err(Error::new(
                "window_not_open",
                format!(
                    "No open window {top:?}; open ones: {}",
                    ws.iter()
                        .map(|w| w.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }
    let mut out = uitree::view(&ws, all, under, max);
    out["note"] = json!("Widget ids are stable while the panel's layout stays: pass one to hoi4_ui_act. `at` is the screen rectangle (only for icons, buttons and check boxes); `enabled:false` marks a disabled button; `script` is what the interface files say (tooltip text, the scripted effect of a mod's button, what it spends, what cannot be undone). The game's live tooltip is not read; windows a mod opens outside the game's window lists may be missing.");
    Ok(out)
}

/// A light reading of the country for an action's read-back: political power, stability, war
/// support, manpower, factories, command power, resources and the alert row.
pub(super) fn quick_state(process: &dyn Process) -> Value {
    json!({
        "country": country_status::status(process).ok(),
        "industry": super::read_only_call(process, "hoi4_industry").ok(),
        "resources": resources::resources(process).ok(),
        "alerts": alerts::alerts(process).ok(),
    })
}

/// Why no input may reach this widget at all: the console and debug controls are not the player's
/// interface (FAIR play), even when a mod draws a debug button in its own panel.
pub(super) fn forbidden(window: &Window, widget: &Widget) -> Option<String> {
    let effect = widget
        .script
        .as_ref()
        .and_then(|s| s["effect"].as_str())
        .unwrap_or("");
    [window.id.as_str(), widget.id.as_str(), effect]
        .iter()
        .find(|t| {
            let t = t.to_ascii_lowercase();
            t.contains("console") || t.contains("debug")
        })
        .map(|t| format!("{t} is a console or debug control: not the player's interface"))
}

/// Why a click on this widget needs the caller's say-so, or None.
pub(super) fn needs_consent(window: &Window, widget: &Widget) -> Option<(&'static str, String)> {
    if window.source == "popup" {
        return Some((
            "answer_dialog",
            format!(
                "{} is in the pop-up {}: a click answers it",
                widget.id, window.id
            ),
        ));
    }
    let s = widget.script.as_ref()?;
    let spends: Vec<&str> = s["spends"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let irreversible: Vec<&str> = s["irreversible"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if spends.is_empty() && irreversible.is_empty() {
        return None;
    }
    Some((
        "confirm_irreversible",
        format!(
            "{}'s scripted effect {} spends [{}] and does [{}] (cannot be undone with one click)",
            widget.id,
            s["effect"].as_str().unwrap_or("?"),
            spends.join(", "),
            irreversible.join(", ")
        ),
    ))
}

#[derive(Clone, Debug)]
struct Planned {
    window: String,
    widget: String,
    x: f64,
    y: f64,
}

/// `hoi4_ui_act`: one ordinary input on a widget by id, then what changed.
#[allow(clippy::too_many_arguments)]
pub(super) fn act(
    args: &Map<String, Value>,
    process: &dyn Process,
    keys: &dyn GameInput,
    guard: &mut super::Guard,
    authority: &crate::input_lease::InputAuthority,
    actions: &mut super::journal::GameJournal,
) -> Result<Value> {
    super::allowed(
        args,
        &[
            "widget",
            "action",
            "times",
            "confirm_irreversible",
            "answer_dialog",
            "wait_ms",
            "request_id",
            "lease_token",
        ],
    )?;
    let wanted = need_str(args, "widget", "\"topbar/decisionview_button\"")?;
    let action = opt_str(args, "action")?.unwrap_or("click");
    if !matches!(
        action,
        "click"
            | "right_click"
            | "shift_click"
            | "ctrl_click"
            | "hover"
            | "scroll_up"
            | "scroll_down"
    ) {
        return Err(Error::new(
            "invalid_arguments",
            "action is one of click, right_click, shift_click, ctrl_click, hover, scroll_up, scroll_down",
        ));
    }
    let times = opt_int(args, "times", 1, 10, 1)? as u32;
    if times > 1 && action != "click" {
        return Err(Error::new(
            "invalid_arguments",
            "times > 1 is only for action click",
        ));
    }
    let confirm = opt_bool(args, "confirm_irreversible")?;
    let answer = opt_bool(args, "answer_dialog")?;
    let wait = opt_int(args, "wait_ms", 200, 5000, 700)? as u64;
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    let epoch = guard.observe(process)?.epoch;
    authority.authorize(token, &epoch)?;
    let retry = actions.receipt_exists(id);
    // Refusals before any input: the widget, its state and the caller's consent.
    let (before, planned) = if retry {
        (Vec::new(), None)
    } else {
        let before = windows(process, false)?;
        let (w, x) = uitree::find(&before, wanted).map_err(|e| {
            Error::new(
                if e["code"] == "widget_ambiguous" {
                    "widget_ambiguous"
                } else {
                    "widget_not_found"
                },
                format!(
                    "{} {}",
                    e["message"].as_str().unwrap_or(""),
                    e.get("candidates")
                        .or_else(|| e.get("windows"))
                        .cloned()
                        .unwrap_or(Value::Null)
                ),
            )
        })?;
        if let Some(why) = forbidden(w, x) {
            return Err(Error::new("widget_forbidden", why));
        }
        if !x.shown {
            return Err(Error::new(
                "widget_hidden",
                format!("{} is not shown now", x.id),
            ));
        }
        let Some(at) = x.at else {
            return Err(Error::new(
                "widget_without_rectangle",
                format!(
                    "{} ({}) has no screen rectangle the reader knows; act on a button or icon inside it",
                    x.id,
                    x.kind.as_deref().unwrap_or("unknown kind")
                ),
            ));
        };
        if !x.enabled() && action != "hover" {
            return Err(Error::new(
                "widget_disabled",
                format!("{} is disabled (the game greys it out)", x.id),
            ));
        }
        if action != "hover" && !action.starts_with("scroll") {
            match needs_consent(w, x) {
                Some(("answer_dialog", why)) if !answer => {
                    return Err(Error::new(
                        "dialog_needs_answer_flag",
                        format!("{why}; pass answer_dialog:true to click it"),
                    ))
                }
                Some(("confirm_irreversible", why)) if !confirm => {
                    return Err(Error::new(
                        "irreversible_needs_confirmation",
                        format!("{why}; pass confirm_irreversible:true only when you mean it"),
                    ))
                }
                _ => {}
            }
        }
        keys.preflight()?;
        let planned = Planned {
            window: w.id.clone(),
            widget: x.id.clone(),
            x: f64::from(at[0]) + f64::from(at[2]) / 2.0,
            y: f64::from(at[1]) + f64::from(at[3]) / 2.0,
        };
        (before, Some(planned))
    };
    let state_before = if retry {
        Value::Null
    } else {
        quick_state(process)
    };
    let observed = std::cell::RefCell::new(None::<(Value, Value)>);
    let mut settle = || {
        Ok(match observed.borrow().as_ref() {
            Some((d, s)) if d["nothing_changed"] == false || s["count"].as_u64() > Some(0) => {
                super::journal::Outcome::Reached(
                    json!({"widgets": d["count"], "state": s["count"]}),
                )
            }
            _ => super::journal::Outcome::Unobservable,
        })
    };
    let mut dispatch = || {
        authority.authorize(token, &epoch)?;
        let Some(p) = &planned else {
            return Err(Error::new(
                "action_uncertain",
                "A retried request is never dispatched again",
            ));
        };
        for n in 0..times {
            match action {
                "click" => keys.click(kit::GAME, p.x, p.y).map(|_| ())?,
                "right_click" => keys.right_click(kit::GAME, p.x, p.y).map(|_| ())?,
                "shift_click" => keys.shift_click(kit::GAME, p.x, p.y).map(|_| ())?,
                "ctrl_click" => keys.ctrl_click(kit::GAME, p.x, p.y).map(|_| ())?,
                // The cursor rests on the widget (a zero wheel event moves it there).
                "hover" => keys.scroll(kit::GAME, (p.x, p.y), 0)?,
                "scroll_up" => keys.scroll(kit::GAME, (p.x, p.y), 3)?,
                _ => keys.scroll(kit::GAME, (p.x, p.y), -3)?,
            }
            if n + 1 < times {
                crate::phase::sleep(Duration::from_millis(200));
            }
        }
        crate::phase::sleep(Duration::from_millis(wait));
        let after = windows(process, false)?;
        let d = uitree::diff(&before, &after, 40);
        let s = uitree::state_diff(&state_before, &quick_state(process), 30);
        *observed.borrow_mut() = Some((d, s));
        Ok(())
    };
    let action_value =
        json!({"kind": "ui_act", "widget": wanted, "action": action, "times": times});
    let receipt = actions.execute(id, &epoch, action_value, &mut settle, &mut dispatch)?;
    let (changed, state_changed) = observed.into_inner().unwrap_or((Value::Null, Value::Null));
    let dialog = changed["windows_opened"]
        .as_array()
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    Ok(json!({
        "receipt": receipt,
        "widget": planned.as_ref().map(|p| p.widget.clone()),
        "window": planned.as_ref().map(|p| p.window.clone()),
        "action": action,
        "changed": changed,
        "state_changed": state_changed,
        "next": if dialog {
            "A window opened: hoi4_ui_inspect {\"window\": <its id>} shows it; a pop-up is answered with hoi4_ui_act answer_dialog:true."
        } else if changed["nothing_changed"] == true && state_changed["count"] == 0 {
            "Nothing visible changed: the click may need another state (read the widget's script and tooltip) or the game may act later."
        } else {
            "If this did what you wanted, save the steps with hoi4_skill_save so it can be run again."
        },
        "validated": false,
    }))
}

// ---- the store -------------------------------------------------------------------------------

fn mods_now(process: &dyn Process) -> Vec<(String, String)> {
    let Some(exe) = process.identity().map(|i| i.exe.clone()) else {
        return Vec::new();
    };
    let Some(user) = super::modlayer::userdir(&exe) else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(user.join("dlc_load.json")) else {
        return Vec::new();
    };
    super::modlayer::parse_enabled_mods(&text)
        .iter()
        .filter_map(|d| std::fs::read_to_string(user.join(d)).ok())
        .map(|t| {
            let field = |k: &str| {
                t.lines()
                    .filter_map(|l| l.split('#').next())
                    .filter_map(|l| l.split_once('='))
                    .find(|(key, _)| key.trim() == k)
                    .map(|(_, v)| v.trim().trim_matches('"').to_string())
                    .unwrap_or_default()
            };
            (field("name"), field("version"))
        })
        .collect()
}

/// The game version and mod set a skill is learned or run on.
pub(super) fn learned_on_now(process: &dyn Process) -> LearnedOn {
    let mods = mods_now(process);
    LearnedOn {
        game_version: crate::builds::SUPPORTED
            .iter()
            .find(|b| b.system == std::env::consts::OS)
            .map(|b| b.game_version.to_string())
            .unwrap_or_default(),
        mod_set: skills::mod_set_id(&mods),
        mods: mods.into_iter().map(|(n, _)| n).collect(),
    }
}

fn unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The store tools that touch no input: list, show, save, retire, facts.
pub(super) fn store_call(
    name: &str,
    args: &Map<String, Value>,
    process: &dyn Process,
    run_dir: &Path,
) -> Result<Value> {
    let store = Store::new(skills::store_dir(run_dir));
    let now = learned_on_now(process);
    match name {
        "hoi4_skill_list" => {
            super::allowed(args, &[])?;
            let rows: Vec<Value> = store
                .list()
                .into_iter()
                .map(|s| match s {
                    Ok(s) => json!({"name": s.name, "purpose": s.purpose, "status": s.status,
                        "parameters": s.parameters, "runs_ok": s.runs.ok, "runs_failed": s.runs.failed,
                        "reversible": s.reversible, "runs_here": s.runnable(&now, false).is_ok()}),
                    Err((n, e)) => json!({"name": n, "status": "unreadable", "error": e}),
                })
                .collect();
            Ok(
                json!({"store": store.dir, "skills": rows, "game_version": now.game_version, "mod_set": now.mod_set, "mods": now.mods}),
            )
        }
        "hoi4_skill_show" => {
            super::allowed(args, &["name"])?;
            let s = store.load(need_str(args, "name", "\"buy_water\"")?)?;
            let why = s.runnable(&now, false).err();
            Ok(json!({"skill": s, "runs_here": why.is_none(), "why_not": why}))
        }
        "hoi4_skill_save" => {
            super::allowed(args, &["skill", "replace"])?;
            let raw = args.get("skill").cloned().ok_or_else(|| {
                Error::new(
                    "invalid_arguments",
                    "skill is required (an object; see the tool's example)",
                )
            })?;
            let mut skill: Skill = serde_json::from_value(raw)
                .map_err(|e| Error::new("invalid_skill", format!("skill: {e}")))?;
            if skill.learned_on.game_version.is_empty() {
                skill.learned_on = now.clone();
            }
            let path = store.save(&skill, opt_bool(args, "replace")?)?;
            Ok(json!({"saved": skill.name, "file": path, "learned_on": skill.learned_on}))
        }
        "hoi4_skill_retire" => {
            super::allowed(args, &["name", "reason"])?;
            let s = store.set_status(
                need_str(args, "name", "\"buy_water\"")?,
                "retired",
                need_str(args, "reason", "\"the mod removed this panel\"")?,
            )?;
            Ok(json!({"retired": s.name}))
        }
        "hoi4_fact_save" => {
            super::allowed(args, &["statement", "source"])?;
            let fact = store.add_fact(
                need_str(
                    args,
                    "statement",
                    "\"Water is bought in the caps panel for 25 political power\"",
                )?,
                need_str(args, "source", "\"common/scripted_guis/caps.txt\"")?,
                &now,
                unix(),
            )?;
            Ok(json!({"fact": fact}))
        }
        "hoi4_fact_list" => {
            super::allowed(args, &["contains"])?;
            let filter = opt_str(args, "contains")?.map(str::to_lowercase);
            let facts: Vec<Value> = store
                .facts()
                .into_iter()
                .filter(|f| {
                    filter.as_ref().is_none_or(|q| {
                        f["statement"]
                            .as_str()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(q.as_str())
                    })
                })
                .collect();
            Ok(json!({"facts": facts, "mod_set_now": now.mod_set}))
        }
        "hoi4_ui_inspect" => inspect(process, args),
        _ => Err(Error::new(
            "capability_unavailable",
            "Unknown learning tool",
        )),
    }
}

/// Whether a listed tool takes `key` (its schema names it).
fn takes(listed: &[Value], tool: &str, key: &str) -> bool {
    listed
        .iter()
        .find(|t| t["name"] == tool)
        .is_some_and(|t| t["inputSchema"]["properties"].get(key).is_some())
}

/// The number at a dotted path of a read's answer.
fn number_at(v: &Value, path: &str) -> Option<f64> {
    let mut flat = std::collections::BTreeMap::new();
    uitree::flatten("", v, 0, &mut flat);
    flat.get(path).and_then(Value::as_f64)
}

/// `hoi4_skill_run`: check the skill may run here and its required widgets are on screen, run
/// its steps through the ordinary tools (`call`), check its proofs and count the run.
pub(super) fn skill_run(
    run_dir: &Path,
    process: &dyn Process,
    listed: &[Value],
    call: &dyn Fn(&str, &Value) -> Result<Value>,
    args: &Value,
) -> Result<Value> {
    let args = args
        .as_object()
        .ok_or_else(|| Error::new("invalid_arguments", "Input arguments must be an object"))?;
    super::allowed(
        args,
        &[
            "name",
            "args",
            "revalidate",
            "confirm_irreversible",
            "request_id",
            "lease_token",
        ],
    )?;
    let name = need_str(args, "name", "\"buy_water\"")?;
    let rid = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    let revalidate = opt_bool(args, "revalidate")?;
    let confirm = opt_bool(args, "confirm_irreversible")?;
    let params = match args.get("args") {
        None => Map::new(),
        Some(Value::Object(o)) => o.clone(),
        Some(_) => {
            return Err(Error::new(
                "invalid_arguments",
                "args must be an object of the skill's parameters",
            ))
        }
    };
    let store = Store::new(skills::store_dir(run_dir));
    let skill = store.load(name)?;
    let now = learned_on_now(process);
    skill
        .runnable(&now, revalidate)
        .map_err(|why| Error::new("skill_refused", format!("{name}: {why}")))?;
    if !skill.reversible && !confirm {
        return Err(Error::new(
            "irreversible_needs_confirmation",
            format!(
                "{name} cannot be undone with one click (cost: {}); pass confirm_irreversible:true only when you mean it",
                skill.cost.as_deref().unwrap_or("not stated")
            ),
        ));
    }
    let steps = skill.bind(&params)?;
    // Required windows and widgets: missing ones mean the panel changed; the skill turns stale.
    if !skill.requires.windows.is_empty() || !skill.requires.widgets.is_empty() {
        let ws = windows(process, false)?;
        let mut missing: Vec<String> = skill
            .requires
            .windows
            .iter()
            .filter(|w| !ws.iter().any(|x| &x.id == *w))
            .cloned()
            .collect();
        missing.extend(
            skill
                .requires
                .widgets
                .iter()
                .filter(|w| !matches!(uitree::find(&ws, w), Ok((_, x)) if x.shown))
                .cloned(),
        );
        if !missing.is_empty() {
            let why = format!("required but not on screen: {}", missing.join(", "));
            if ws.iter().any(|w| w.source == "view") {
                store.set_status(name, "stale", &why)?;
            }
            let _ = store.record_run(name, false, Some(&why), &now, unix());
            return Err(Error::new("skill_stale", format!("{name}: {why}; it is marked stale (open the panel first if it was only closed, then save the skill again)")));
        }
    }
    let reads_before: Vec<Option<f64>> = skill
        .verify
        .iter()
        .map(|c| match (c.check.as_str(), &c.tool, &c.path) {
            ("read_changes", Some(t), Some(p)) => {
                call(t, &json!({})).ok().and_then(|v| number_at(&v, p))
            }
            _ => None,
        })
        .collect();
    let mut done = Vec::new();
    let mut changed_ids: HashSet<String> = HashSet::new();
    let mut opened: HashSet<String> = HashSet::new();
    for (i, step) in steps.iter().enumerate() {
        let mut a = step.args.clone();
        if takes(listed, &step.tool, "lease_token") {
            a.insert("lease_token".into(), json!(token));
        }
        if takes(listed, &step.tool, "request_id") {
            a.insert("request_id".into(), json!(format!("{rid}.{i}")));
        }
        if step.tool == ACT && !skill.reversible && confirm {
            a.insert("confirm_irreversible".into(), json!(true));
        }
        match call(&step.tool, &Value::Object(a)) {
            Ok(v) => {
                for key in ["widgets_changed", "widgets_appeared", "widgets_disappeared"] {
                    for c in v["changed"][key].as_array().into_iter().flatten() {
                        changed_ids
                            .insert(c["id"].as_str().or(c.as_str()).unwrap_or("").to_string());
                    }
                }
                for w in v["changed"]["windows_opened"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    opened.insert(w.as_str().unwrap_or("").to_string());
                }
                done.push(
                    json!({"step": i, "tool": step.tool, "ok": true, "note": step.note,
                    "changed": v["changed"]["count"], "receipt": v["receipt"]["status"]}),
                );
            }
            Err(e) => {
                let why = format!("step {i} ({}) failed: {} {}", step.tool, e.code, e.message);
                let stale = matches!(
                    e.code,
                    "widget_not_found" | "widget_ambiguous" | "window_not_open"
                );
                if stale && i == 0 {
                    store.set_status(name, "stale", &why)?;
                }
                let _ = store.record_run(name, false, Some(&why), &now, unix());
                return Err(Error::new("skill_step_failed", format!("{name}: {why}")));
            }
        }
    }
    let final_windows: Option<Vec<Window>> = skill
        .verify
        .iter()
        .any(|c| c.check.starts_with("window_"))
        .then(|| windows(process, false).ok())
        .flatten();
    let mut checks = Vec::new();
    let mut ok = true;
    for (i, c) in skill.verify.iter().enumerate() {
        let id = c.id.clone().unwrap_or_default();
        let open_now = final_windows
            .as_ref()
            .map(|ws| ws.iter().any(|w| w.id == id));
        let (pass, seen) = match c.check.as_str() {
            "window_opens" => (
                opened.contains(&id) || open_now == Some(true),
                json!(open_now),
            ),
            "window_closes" => (open_now == Some(false), json!(open_now)),
            "widget_changes" => (
                changed_ids
                    .iter()
                    .any(|x| x == &id || x.ends_with(&format!("/{id}"))),
                json!(changed_ids.len()),
            ),
            _ => {
                let after = match (&c.tool, &c.path) {
                    (Some(t), Some(p)) => call(t, &json!({})).ok().and_then(|v| number_at(&v, p)),
                    _ => None,
                };
                let pass = match (reads_before[i], after, c.direction.as_deref()) {
                    (Some(b), Some(a), Some("up")) => a > b,
                    (Some(b), Some(a), Some("down")) => a < b,
                    (Some(b), Some(a), _) => a != b,
                    _ => false,
                };
                (pass, json!({"before": reads_before[i], "after": after}))
            }
        };
        ok &= pass;
        checks.push(
            json!({"check": c.check, "id": c.id, "path": c.path, "passed": pass, "seen": seen}),
        );
    }
    let error = (!ok).then(|| "a verify check did not pass".to_string());
    let recorded = store.record_run(name, ok, error.as_deref(), &now, unix())?;
    Ok(
        json!({"skill": name, "ok": ok, "steps": done, "verify": checks,
        "runs": recorded.runs, "status": recorded.status, "validated": false}),
    )
}

// ---- descriptors -------------------------------------------------------------------------------

pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({"name": "hoi4_ui_inspect", "description": "Read-only: every open window and its widgets from the game's interface memory, for panels no other tool knows (a mod's own mechanics, a new DLC). Each widget has a stable `id` (the path of .gui element names, e.g. \"topbar/decisionview_button\"; `#n` numbers repeated names, `#n` below a grid numbers its entries), `kind` (button, icon, text, container, grid, ...), `at` [x,y,w,h] on screen when known, `enabled:false` when the game greys it out, `text` for text boxes, and `script`: what the game's and the mod's interface files say (tooltip text, the scripted effect a mod's button runs, what it spends, what cannot be undone). Pass `window` (a window id or a widget id) to read one panel; `all:true` adds hidden widgets. No input, no lease, never pauses. Example: {\"window\": \"topbar\"}", "inputSchema": {"type": "object", "properties": {"window": {"type": "string", "maxLength": 400, "description": "a window id from an earlier reading, or a widget id to read only that subtree"}, "all": {"type": "boolean", "description": "also list hidden widgets (default false)"}, "hidden_windows": {"type": "boolean", "description": "also list windows the game keeps hidden (default false)"}, "max": {"type": "integer", "minimum": 20, "maximum": 3000, "default": 400}}, "additionalProperties": false}, "annotations": {"readOnlyHint": true}}),
        json!({"name": "hoi4_ui_act", "description": "One ordinary input on a widget by its id from hoi4_ui_inspect, then what changed: `changed` (windows opened or closed, widgets shown, hidden, enabled, disabled, new text) and `state_changed` (political power, stability, war support, manpower, factories, resources, alerts with their deltas). Refused before any input: widget_not_found / widget_ambiguous (with candidates), widget_forbidden (console and debug controls), widget_hidden, widget_disabled, widget_without_rectangle; irreversible_needs_confirmation when the files say the click spends something or cannot be undone (pass confirm_irreversible:true only when you mean it); dialog_needs_answer_flag for a pop-up's buttons (answer_dialog:true). action: click (default), right_click, shift_click, ctrl_click, hover (rest the cursor on it), scroll_up, scroll_down; times repeats a click (1-10). When it worked, keep it with hoi4_skill_save. Example: {\"widget\": \"topbar/decisionview_button\", \"action\": \"click\", \"request_id\": \"r1\", \"lease_token\": \"...\"}", "inputSchema": {"type": "object", "properties": {"widget": {"type": "string", "maxLength": 400, "description": "widget id from hoi4_ui_inspect (or a name only one shown widget has)"}, "action": {"type": "string", "enum": ["click", "right_click", "shift_click", "ctrl_click", "hover", "scroll_up", "scroll_down"], "default": "click"}, "times": {"type": "integer", "minimum": 1, "maximum": 10, "default": 1}, "confirm_irreversible": {"type": "boolean"}, "answer_dialog": {"type": "boolean"}, "wait_ms": {"type": "integer", "minimum": 200, "maximum": 5000, "default": 700, "description": "how long to wait before reading what changed"}, "request_id": {"type": "string"}, "lease_token": {"type": "string"}}, "required": ["widget", "request_id", "lease_token"], "additionalProperties": false}}),
        json!({"name": "hoi4_skill_save", "description": "Keep something that worked as a named skill (a plain JSON file in the learned/skills folder, one per skill, readable and editable): name, purpose, parameters, the windows/widgets that must be on screen before it starts (requires), its steps (tool calls; \"{param}\" in a string is replaced by the run's argument; leave out request_id and lease_token), the proofs that it worked (verify), cost and whether one click can undo it (reversible). The game version and mod set are stamped on it. Refused with what to fix: invalid_skill; skill_exists (replace:true overwrites). Example: {\"skill\": {\"name\": \"open_caps_trade\", \"purpose\": \"Open the mod's caps trade panel\", \"steps\": [{\"tool\": \"hoi4_ui_act\", \"args\": {\"widget\": \"topbar/caps_button\"}}], \"verify\": [{\"check\": \"window_opens\", \"id\": \"caps_window\"}], \"cost\": \"nothing\", \"reversible\": true}}", "inputSchema": {"type": "object", "properties": {"skill": {"type": "object", "properties": {"name": {"type": "string", "pattern": "^[a-z0-9_]{3,64}$"}, "purpose": {"type": "string", "maxLength": 400}, "parameters": {"type": "object", "description": "parameter name -> what to pass"}, "requires": {"type": "object", "properties": {"windows": {"type": "array", "items": {"type": "string"}}, "widgets": {"type": "array", "items": {"type": "string"}}}, "additionalProperties": false}, "steps": {"type": "array", "minItems": 1, "maxItems": 12, "items": {"type": "object", "properties": {"tool": {"type": "string"}, "args": {"type": "object"}, "note": {"type": "string"}}, "required": ["tool"], "additionalProperties": false}}, "verify": {"type": "array", "items": {"type": "object", "properties": {"check": {"type": "string", "enum": ["window_opens", "window_closes", "widget_changes", "read_changes"]}, "id": {"type": "string"}, "tool": {"type": "string"}, "path": {"type": "string"}, "direction": {"type": "string", "enum": ["up", "down", "any"]}}, "required": ["check"], "additionalProperties": false}}, "cost": {"type": "string"}, "reversible": {"type": "boolean"}, "evidence": {"type": "string"}}, "required": ["name", "purpose", "steps"]}, "replace": {"type": "boolean"}}, "required": ["skill"], "additionalProperties": false}}),
        json!({"name": "hoi4_skill_list", "description": "Read-only: the saved skills (name, purpose, parameters, status active/stale/retired, runs that worked and failed, whether it can run on this game version and mod set) and the current mod set. No input. Example: {}", "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}, "annotations": {"readOnlyHint": true}}),
        json!({"name": "hoi4_skill_show", "description": "Read-only: one saved skill in full (steps, requires, verify, cost, where it was learned, its runs) and why it cannot run here if it cannot. Example: {\"name\": \"open_caps_trade\"}", "inputSchema": {"type": "object", "properties": {"name": {"type": "string", "pattern": "^[a-z0-9_]{3,64}$"}}, "required": ["name"], "additionalProperties": false}, "annotations": {"readOnlyHint": true}}),
        json!({"name": "hoi4_skill_run", "description": "Run a saved skill: checks it may run here (not retired or stale, learned on this game version and mod set unless revalidate:true), that its required windows and widgets are on screen (a missing one marks it stale and refuses), runs its steps through the ordinary tools with this lease, then its verify checks; the run is counted in the skill file. A skill with reversible:false needs confirm_irreversible:true. Typed refusals: skill_not_found, skill_refused, skill_stale, irreversible_needs_confirmation, invalid_arguments (a missing parameter); skill_step_failed names the step. Example: {\"name\": \"open_caps_trade\", \"args\": {}, \"request_id\": \"r2\", \"lease_token\": \"...\"}", "inputSchema": {"type": "object", "properties": {"name": {"type": "string", "pattern": "^[a-z0-9_]{3,64}$"}, "args": {"type": "object", "description": "the skill's parameters by name"}, "revalidate": {"type": "boolean"}, "confirm_irreversible": {"type": "boolean"}, "request_id": {"type": "string"}, "lease_token": {"type": "string"}}, "required": ["name", "request_id", "lease_token"], "additionalProperties": false}}),
        json!({"name": "hoi4_skill_retire", "description": "Mark a saved skill retired (kept on disk, never run again) with the reason. Example: {\"name\": \"open_caps_trade\", \"reason\": \"the mod removed this panel\"}", "inputSchema": {"type": "object", "properties": {"name": {"type": "string", "pattern": "^[a-z0-9_]{3,64}$"}, "reason": {"type": "string", "maxLength": 300}}, "required": ["name", "reason"], "additionalProperties": false}}),
        json!({"name": "hoi4_fact_save", "description": "Keep one fact learned about this game or mod (a statement and where it comes from: a file path or what was seen) in learned/facts.json with the game version and mod set. A repeated statement is kept once. Example: {\"statement\": \"Water is bought in the caps panel for 25 political power\", \"source\": \"common/scripted_guis/caps.txt\"}", "inputSchema": {"type": "object", "properties": {"statement": {"type": "string", "maxLength": 500}, "source": {"type": "string", "maxLength": 300}}, "required": ["statement", "source"], "additionalProperties": false}}),
        json!({"name": "hoi4_fact_list", "description": "Read-only: the saved facts, optionally only those whose statement contains a word. Example: {\"contains\": \"water\"}", "inputSchema": {"type": "object", "properties": {"contains": {"type": "string", "maxLength": 100}}, "additionalProperties": false}, "annotations": {"readOnlyHint": true}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ReadMemory;
    use std::collections::BTreeMap;

    /// Sparse little-endian memory with the macOS string ABI (the gui_tree test layout).
    #[derive(Default)]
    struct Fake(BTreeMap<u64, u8>);
    impl Fake {
        fn put(&mut self, a: u64, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.0.insert(a + i as u64, *b);
            }
        }
        fn q(&mut self, a: u64, v: u64) {
            self.put(a, &v.to_le_bytes());
        }
        fn short(&mut self, a: u64, s: &str) {
            let mut b = vec![(s.len() as u8) << 1];
            b.extend_from_slice(s.as_bytes());
            b.resize(24, 0);
            self.put(a, &b);
        }
        /// A container object at `obj` named `name` holding `children` (GUI-object bases).
        fn container(&mut self, obj: u64, name: &str, children: &[u64], visible: bool) {
            self.q(obj, IMAGE + 0x36ba978);
            self.put(obj + 0x95, &[if visible { 8 } else { 0 }]);
            self.named(obj + 0x30, name, visible);
            let list = obj + 0x8000;
            self.q(obj + 0xe50, list);
            let n = children.len() as u32;
            let mut header = n.to_le_bytes().to_vec();
            header.extend(n.to_le_bytes());
            self.put(obj + 0xe58, &header);
            for (i, c) in children.iter().enumerate() {
                self.q(list + i as u64 * 8, *c);
            }
        }
        /// GUI type and shown flag of a GUI-object base.
        fn named(&mut self, base: u64, name: &str, shown: bool) {
            let ty = base + 0x4000;
            self.q(base + 0x50, ty);
            self.q(ty, IMAGE + 0x100);
            self.short(ty + 0x28, name);
            self.put(base + 0x65, &[if shown { 8 } else { 0 }]);
        }
        fn button(&mut self, base: u64, name: &str, state: i32, x: f32, y: f32) {
            self.q(base, IMAGE + 0x36b9738);
            self.named(base, name, true);
            self.put(base + 0x9c, &state.to_le_bytes());
            let sprite = base + 0x2000;
            self.q(base + 0xe0, sprite);
            self.q(sprite, IMAGE + 0x36cc528);
            self.put(sprite + 0xb0, &x.to_le_bytes());
            self.put(sprite + 0xb4, &y.to_le_bytes());
            let kind = sprite + 0x400;
            self.q(sprite + 8, kind);
            self.put(kind + 0x78, &[1, 0, 0, 0]);
            self.put(kind + 0x8c, &[100, 0, 0, 0, 30, 0, 0, 0]);
        }
    }
    impl ReadMemory for Fake {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            Ok((0..n as u64)
                .map(|i| *self.0.get(&(a + i)).unwrap_or(&0))
                .collect())
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
    const IMAGE: u64 = 0x1_0000_0000;

    /// A mod's caps panel (a view) holding a sub-window, plus a confirmation pop-up.
    fn game() -> Fake {
        let mut m = Fake::default();
        let (view, popup) = (0x10_0000u64, 0x11_0000u64);
        let (caps, sub, pop) = (0x20_0000u64, 0x30_0000u64, 0x40_0000u64);
        m.q(view + 0x490, caps);
        m.q(view + 0x68, sub); // a sub-window also held by the view: stays inside caps
        m.q(popup + 0x490, pop);
        m.container(
            caps,
            "caps_window",
            &[0x21_0000, 0x22_0000, sub + 0x30],
            true,
        );
        m.button(0x21_0000, "buy_button", 0, -100.0, 0.0);
        m.button(0x22_0000, "buy_button", 3, 0.0, 0.0);
        m.container(sub, "caps_sub", &[0x31_0000], true);
        m.button(0x31_0000, "info_button", 0, 100.0, 0.0);
        m.container(pop, "confirm_popup", &[0x41_0000], true);
        m.button(0x41_0000, "accept_button", 0, 0.0, -200.0);
        m
    }

    fn read_windows(m: &Fake) -> Vec<Window> {
        let r = Reader::new(m);
        let found = roots(&r, IMAGE, &[(0x10_0000, "view"), (0x11_0000, "popup")]);
        assert_eq!(found.len(), 3);
        let mut index = GuiIndex::default();
        index.add_gui(
            r#"guiTypes = { containerWindowType = { name = "caps_window" buttonType = { name = "buy_button" pdx_tooltip = "BUY_TT" } } }"#,
            "interface/caps.gui",
        );
        index.add_scripted(
            r#"scripted_gui = { caps = { window_name = "caps_window" effects = { buy_button_click = { add_political_power = -25 } } } }"#,
        );
        let kind = |w: &str, n: &str| index.element(w, n).map(|e| e.kind.clone());
        let (mut inner, mut budget) = (HashSet::new(), 100);
        let mut out = Vec::new();
        for (root, source) in found {
            let id = panelkit::container_name(&r, IMAGE, root).unwrap().unwrap();
            let widgets = walk(&r, IMAGE, root, &id, &kind, &mut inner, &mut budget);
            out.push((
                root,
                Window {
                    id,
                    source: source.into(),
                    visible: true,
                    widgets,
                },
            ));
        }
        let mut ws: Vec<Window> = out
            .into_iter()
            .filter(|(r, _)| !inner.contains(r))
            .map(|(_, w)| w)
            .collect();
        uitree::unique_windows(&mut ws);
        let loc = HashMap::from([("BUY_TT".to_string(), "Buy water".to_string())]);
        for w in &mut ws {
            let wid = w.id.clone();
            for x in &mut w.widgets {
                x.script = index.note(&wid, &x.id, &x.name, &loc);
            }
        }
        ws
    }

    #[test]
    fn windows_are_enumerated_once_with_sub_windows_inside_and_ids_stable() {
        let ws = read_windows(&game());
        let ids: Vec<&str> = ws.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, vec!["caps_window", "confirm_popup"]);
        let caps = &ws[0];
        let widget_ids: Vec<&str> = caps.widgets.iter().map(|x| x.id.as_str()).collect();
        assert_eq!(
            widget_ids,
            vec![
                "caps_window/buy_button",
                "caps_window/buy_button#1",
                "caps_window/caps_sub",
                "caps_window/caps_sub/info_button"
            ]
        );
        let buy = &caps.widgets[0];
        assert_eq!(buy.kind.as_deref(), Some("button"));
        assert_eq!(buy.at, Some([860, 510, 100, 30]));
        assert!(buy.clickable());
        assert!(!caps.widgets[1].enabled(), "state 3 is disabled");
        assert_eq!(buy.script.as_ref().unwrap()["tooltip"], "Buy water");
    }

    #[test]
    fn consent_is_needed_for_costly_scripted_clicks_and_popup_answers() {
        let ws = read_windows(&game());
        let (w, x) = uitree::find(&ws, "caps_window/buy_button").unwrap();
        assert_eq!(needs_consent(w, x).unwrap().0, "confirm_irreversible");
        let (w, x) = uitree::find(&ws, "info_button").unwrap();
        assert!(needs_consent(w, x).is_none());
        let (w, x) = uitree::find(&ws, "accept_button").unwrap();
        assert_eq!(needs_consent(w, x).unwrap().0, "answer_dialog");
        assert!(forbidden(w, x).is_none());
        let mut debug = x.clone();
        debug.id = "roach_nest_window/button_debug".into();
        assert!(forbidden(w, &debug).unwrap().contains("debug"));
        let mut console = w.clone();
        console.id = "console".into();
        assert!(forbidden(&console, x).is_some());
    }

    #[test]
    fn every_descriptor_has_an_example_and_a_closed_schema() {
        for t in tools() {
            let d = t["description"].as_str().unwrap();
            assert!(d.contains("Example: {"), "{} needs an example", t["name"]);
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
        }
        let listed = tools();
        assert!(takes(&listed, ACT, "lease_token"));
        assert!(!takes(&listed, INSPECT, "lease_token"));
        assert_eq!(number_at(&json!({"a": {"b": 3}}), "a.b"), Some(3.0));
    }
}
