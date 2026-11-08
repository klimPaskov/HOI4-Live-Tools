//! Take own decisions with ordinary input: the topbar decisions button, the
//! decision's row (scrolled into view only while the panel is open) and its
//! select button. Verified when the decision view shows the decision taken:
//! a timed entry for it appears, its entry disappears or its button turns
//! disabled. Lists the offered decisions with the game's enabled state.
use super::glide::{glide, span_y, Glide, Glided};
use super::ui::{clear_notices, decline_stale, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{game_input::GameInput, Process},
    xlayout::{self, fld},
};
use serde_json::{json, Value};

struct Ui {
    manager: u64,
    view: u64,
    view_root: u64,
    topbar: u64,
    topbar_root: u64,
    grid_items: u64,
    item_container: u64,
    item_decision: u64,
    targeted_decision: u64,
    timed_decision: u64,
    inner_decision: u64,
    decision_key: u64,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run).
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0xe0,
    view_root: 0x490,
    topbar: 0x188,
    topbar_root: 0x68,
    grid_items: 0x190,
    item_container: 0x10,
    item_decision: 0xd38,
    targeted_decision: 0xd40,
    timed_decision: 0x908,
    inner_decision: 0x10,
    decision_key: 0x108,
};
const GAME: (u32, u32) = (1920, 1080);
const LIST: (i32, i32) = (140, 1040);

/// Owners of the [`MACOS`] fields (macOS 1.19.4.1 RTTI names and vtables; 0
/// when no vtable is reviewed).
const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
const MANAGER: xlayout::Class = xlayout::Class::new("idler.manager", 0);
const VIEW: xlayout::Class = xlayout::Class::new("decision.view", 0);
const TOPBAR: xlayout::Class = xlayout::Class::new("topbar", 0);
const GRID: xlayout::Class = xlayout::Class::new("CStandardGridBox", 0x36cca80);
const GRID_ITEM: xlayout::Class = xlayout::Class::new("grid.item", 0);
const ITEM: xlayout::Class = xlayout::Class::new("CDecisionViewDecisionItem", 0);
const TARGETED_ITEM: xlayout::Class = xlayout::Class::new("CDecisionViewTargetedDecisionItem", 0);
const TIMED_ITEM: xlayout::Class = xlayout::Class::new("CDecisionViewTimedDecisionItem", 0);
const INSTANCE: xlayout::Class = xlayout::Class::new("decision.instance", 0);
const DECISION: xlayout::Class = xlayout::Class::new("CDecision", 0);

/// The decision joins of the running build ([`MACOS`] translated by `xlayout`).
fn ui(r: &Reader<'_>) -> Result<Ui> {
    let abi = r.string_abi();
    let m = &MACOS;
    Ok(Ui {
        manager: fld(abi, &IDLER, m.manager)?,
        view: fld(abi, &MANAGER, m.view)?,
        view_root: fld(abi, &VIEW, m.view_root)?,
        topbar: fld(abi, &MANAGER, m.topbar)?,
        topbar_root: fld(abi, &TOPBAR, m.topbar_root)?,
        grid_items: fld(abi, &GRID, m.grid_items)?,
        item_container: fld(abi, &GRID_ITEM, m.item_container)?,
        item_decision: fld(abi, &ITEM, m.item_decision)?,
        targeted_decision: fld(abi, &TARGETED_ITEM, m.targeted_decision)?,
        timed_decision: fld(abi, &TIMED_ITEM, m.timed_decision)?,
        inner_decision: fld(abi, &INSTANCE, m.inner_decision)?,
        decision_key: fld(abi, &DECISION, m.decision_key)?,
    })
}
fn manager(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(idler + ui(r)?.manager)
}
fn root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    let view = r.u64(manager(r, image)? + u.view)?;
    gui_tree::container(r, r.u64(view + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Decision view root unavailable"))
}
fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, root(r, image)?)
}
fn find(r: &Reader<'_>, image: u64, object: u64, name: &str, depth: u32) -> Result<Option<u64>> {
    for child in gui_tree::children(r, object)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(name) {
            return Ok(Some(child));
        }
        if depth < 6 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                if let Some(found) = find(r, image, inner, name, depth + 1)? {
                    return Ok(Some(found));
                }
            }
        }
    }
    Ok(None)
}
fn topbar_button(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let u = ui(r)?;
    let topbar = r.u64(manager(r, image)? + u.topbar)?;
    let root = gui_tree::container(r, r.u64(topbar + u.topbar_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Topbar root unavailable"))?;
    let button = find(r, image, root, "decisionview_button", 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Decisions button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
fn key_of(r: &Reader<'_>, decision: u64) -> Result<Option<String>> {
    if decision == 0 {
        return Ok(None);
    }
    let key = r.string_bounded(decision + ui(r)?.decision_key, 128)?;
    Ok((!key.is_empty()).then_some(key))
}
#[derive(Clone, Debug, PartialEq)]
struct Row {
    key: String,
    /// The row's title (name_text), e.g. naming the target of a targeted decision.
    title: String,
    timed: bool,
    enabled: bool,
    rect: Option<Rect>,
}
fn rows(r: &Reader<'_>, image: u64) -> Result<Vec<Row>> {
    let u = ui(r)?;
    let grid = find(r, image, root(r, image)?, "decision_grid", 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Decision list not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(512) as u64 {
        let item = r.u64(data + index * 8)?;
        let name =
            crate::research_ui::primary_type_encoding(r, image, r.u64(item)?).unwrap_or_default();
        let (decision, timed) = match name.as_str() {
            "25CDecisionViewDecisionItem" => (r.u64(item + u.item_decision)?, false),
            "33CDecisionViewTargetedDecisionItem" => {
                let targeted = r.u64(item + u.targeted_decision)?;
                (r.u64(targeted + u.inner_decision)?, false)
            }
            "30CDecisionViewTimedDecisionItem" => {
                let timed = r.u64(item + u.timed_decision)?;
                (r.u64(timed + u.inner_decision)?, true)
            }
            _ => continue,
        };
        let Some(key) = key_of(r, decision)? else {
            continue;
        };
        let (enabled, rect) = match gui_tree::container(r, r.u64(item + u.item_container)?, image)?
            .map(|frame| gui_tree::child(r, image, frame, "btn_select"))
            .transpose()?
            .flatten()
        {
            Some(button) => (
                !gui_tree::disabled(r, button)?,
                gui_tree::rect_unclipped(r, image, button, GAME).ok(),
            ),
            None => (false, None),
        };
        let title = gui_tree::container(r, r.u64(item + u.item_container)?, image)?
            .map(|frame| super::panelkit::text(r, image, frame, "name_text", 1))
            .transpose()?
            .flatten()
            .unwrap_or_default();
        out.push(Row {
            key,
            title,
            timed,
            enabled,
            rect,
        });
    }
    Ok(out)
}
fn ensure_open(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if !read(process, open)? {
        let rect = settled(process, "decisions button", topbar_button)?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        wait(process, "open decisions", open)?;
        // Entry states are refreshed shortly after the panel opens.
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(())
}
/// Own political power in raw engine units.
pub(super) fn political_power(process: &dyn Process) -> Result<i64> {
    crate::economy::snapshot(process)?["political_power"]["raw_signed"]
        .as_i64()
        .ok_or_else(|| Error::new("invalid_economy_value", "Political power unavailable"))
}

/// Offered decisions (opens the panel; changes nothing).
pub(super) fn list(process: &dyn Process, keys: &dyn GameInput) -> Result<Value> {
    clear_notices(process, keys)?;
    ensure_open(process, keys)?;
    let found = settled(process, "decisions", rows)?;
    Ok(json!(found
        .into_iter()
        .map(
            |r| json!({"decision":r.key,"title":r.title,"active_timer":r.timed,"enabled":r.enabled})
        )
        .collect::<Vec<_>>()))
}

/// Whether the decision view shows `key` taken relative to `before`: a timer
/// for it appeared or its entry left the list. A button merely turning
/// disabled is not proof (the panel refreshes entry states lazily).
fn taken(now: &[Row], before: &[Row], key: &str) -> bool {
    let was = before.iter().any(|r| r.key == key && !r.timed);
    let timed_before = before.iter().any(|r| r.key == key && r.timed);
    let timed_now = now.iter().any(|r| r.key == key && r.timed);
    let row_now = now.iter().any(|r| r.key == key && !r.timed);
    (timed_now && !timed_before) || (was && !row_now)
}
pub(super) fn state(process: &dyn Process) -> Result<Vec<Value>> {
    let found = read(process, |r, i| {
        if open(r, i)? {
            rows(r, i)
        } else {
            Ok(Vec::new())
        }
    })?;
    Ok(found
        .into_iter()
        .map(|r| json!({"decision":r.key,"title":r.title,"active_timer":r.timed,"enabled":r.enabled}))
        .collect())
}

/// Take decision `key`; the caller has journaled intent.
pub(super) fn take(
    process: &dyn Process,
    keys: &dyn GameInput,
    key: &str,
    target: Option<&str>,
) -> Result<()> {
    decline_stale(process, keys)?;
    clear_notices(process, keys)?;
    ensure_open(process, keys)?;
    let before = settled(process, "decisions", rows)?;
    let pp_before = political_power(process)?;
    let wanted = target.map(|t| t.trim().to_lowercase());
    let fits = |r: &Row| {
        r.key == key
            && !r.timed
            && wanted
                .as_ref()
                .is_none_or(|t| r.title.to_lowercase().contains(t))
    };
    let row = before.iter().find(|r| fits(r)).cloned().ok_or_else(|| {
        let titles: Vec<&str> = before
            .iter()
            .filter(|r| r.key == key)
            .map(|r| r.title.as_str())
            .collect();
        Error::new(
            "invalid_target",
            if titles.is_empty() {
                "This decision is not offered now".to_string()
            } else {
                format!(
                    "No row of this decision matches the target; its rows: {}",
                    titles.join(" | ")
                )
            },
        )
    })?;
    if before.iter().any(|r| r.key == key && r.timed) {
        return Err(Error::new(
            "invalid_target",
            "This decision is already in effect (its timer is running)",
        ));
    }
    if !row.enabled {
        return Err(Error::new(
            "invalid_target",
            "The game disables this decision (cost, cooldown or conditions)",
        ));
    }
    // Glide the row into the list area in one movement, only while the panel is open
    // (a row not laid out yet is searched downwards one event at a time).
    let outcome = glide(
        process,
        keys,
        &Glide {
            at: (175.0, 600.0),
            pixels: 240,
            band: (f64::from(LIST.0), f64::from(LIST.1)),
            search: -1,
        },
        || {
            if read(process, open)? {
                Ok(())
            } else {
                Err(Error::new("input_readback_failed", "Decision panel closed"))
            }
        },
        |r, i| {
            Ok(rows(r, i)?
                .into_iter()
                .find(|x| fits(x))
                .and_then(|x| x.rect)
                .map(|x| span_y(&x)))
        },
    )?;
    // At either end the list cannot scroll; a row then may sit up to the
    // panel's edge, just outside the comfortable band.
    let band = match outcome {
        Glided::Stuck(_) | Glided::Short(_) => (LIST.0 - 20, LIST.1 + 22),
        _ => LIST,
    };
    let rect = settled(process, "decision button", |r, i| {
        rows(r, i)?
            .into_iter()
            .find(|x| fits(x))
            .and_then(|x| x.rect)
            .filter(|r| r.y >= band.0 && r.y + r.h <= band.1)
            .ok_or_else(|| Error::new("gui_target_unavailable", "Decision row not in view"))
    })?;
    clear_notices(process, keys)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    // A confirmation appears only for some decisions: stop waiting for it once one of the
    // proofs below shows.
    super::ui::accept_command_until(process, keys, |r, i| Ok(taken(&rows(r, i)?, &before, key)))?;
    // Proof: political power spent, a timer started or the entry left the list.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if political_power(process)? < pp_before
            || read(process, |r, i| Ok(taken(&rows(r, i)?, &before, key)))?
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "decision taken: no political power spent, timer or removal observed",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(key: &str, timed: bool, enabled: bool) -> Row {
        Row {
            key: key.into(),
            title: String::new(),
            timed,
            enabled,
            rect: None,
        }
    }
    #[test]
    fn taken_needs_a_change_for_this_decision() {
        let before = vec![row("a", false, true), row("b", false, true)];
        assert!(!taken(&before, &before, "a"));
        assert!(taken(&[row("b", false, true)], &before, "a"));
        assert!(!taken(
            &[row("a", false, false), row("b", false, true)],
            &before,
            "a"
        ));
        assert!(taken(
            &[
                row("a", false, true),
                row("a", true, false),
                row("b", false, true)
            ],
            &before,
            "a"
        ));
        assert!(!taken(&[row("a", false, true)], &before, "a"));
    }
}
