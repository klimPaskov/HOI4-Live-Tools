//! Shared fresh-read helpers for ordinary macOS UI actions.
use crate::platform::game_input::GameInput;
use crate::{
    error::{Error, Result},
    game, gui_tree,
    memory::Reader,
    platform::Process,
    xlayout::{self, fld, img},
};
use std::{
    thread,
    time::{Duration, Instant},
};

const STAGE_WAIT: Duration = Duration::from_secs(3);
const SETTLE_WAIT: Duration = Duration::from_secs(2);

/// One fresh read bracket; transient UI churn is retried a few times.
pub(super) fn read<T>(
    process: &dyn Process,
    f: impl Fn(&Reader<'_>, u64) -> Result<T>,
) -> Result<T> {
    let mut attempt = 0;
    loop {
        process.begin_snapshot()?;
        let r = Reader::new(process);
        match f(&r, process.image_base()) {
            Err(e)
                if attempt < 8
                    && matches!(
                        e.code,
                        "torn_read" | "short_read" | "invalid_gui" | "invalid_array"
                    ) =>
            {
                // A busy game day can rewrite the same data for a while.
                attempt += 1;
                thread::sleep(Duration::from_millis(5 * attempt));
            }
            result => {
                process.check()?;
                return result;
            }
        }
    }
}

/// HOI4_TRACE=1: every named wait and settle logs its duration to stderr (finding where an
/// action spends its time).
pub(super) fn trace(stage: &str, since: Instant) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("HOI4_TRACE").is_some()) {
        eprintln!("[trace] {stage} {} ms", since.elapsed().as_millis());
    }
}

/// Bounded wait for a memory predicate after an input.
pub(super) fn wait(
    process: &dyn Process,
    stage: &str,
    f: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<()> {
    let started = Instant::now();
    let deadline = started + STAGE_WAIT;
    loop {
        if read(process, &f)? {
            trace(stage, started);
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                format!("{stage}: expected game change not observed"),
            ));
        }
        thread::sleep(Duration::from_millis(30));
    }
}

/// No-decision notices the server may acknowledge (only an OK button): research
/// finished, focus finished, peace summary and world tension pop-ups (macOS
/// 089EA519). Decisions such as events are never answered here.
/// 0x36805d0 CConfirmationPopUpWindow (doctrine milestones), 0x3695460 CSurrenderedPopupWindow
/// (seized equipment after a capitulation).
/// 0x36803a0 CSpecialProjectFinishedPopUpWindow (title "Special Project Completed", Details and
/// OK, seen live 4 October 1938), 0x36804b8 CSpecialProjectCapturedPopUpWindow (same family; its
/// `ok_button` is not seen live, a window without one is skipped).
const NOTICES: [xlayout::Class; 8] = [
    xlayout::Class::new("CResearchFinishedPopUpWindow", 0x3680170),
    xlayout::Class::new("CFocusFinishedPopUpWindow", 0x36806e8),
    xlayout::Class::new("CPeaceSummaryPopUpWindow", 0x3680800),
    xlayout::Class::new("CWorldTensionPopUpWindow", 0x36a1f88),
    xlayout::Class::new("CConfirmationPopUpWindow", 0x36805d0),
    xlayout::Class::new("CSurrenderedPopupWindow", 0x3695460),
    xlayout::Class::new("CSpecialProjectFinishedPopUpWindow", 0x36803a0),
    xlayout::Class::new("CSpecialProjectCapturedPopUpWindow", 0x36804b8),
];
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0xa70;
const POPUP_ROOT: u64 = 0x490;
/// Owners of the fields used here (macOS 1.19.4.1 RTTI names and vtables; 0
/// where the owning class has no reviewed vtable).
const IDLER: xlayout::Class = xlayout::Class::new("CInGameIdler", 0x35089d8);
const GAME_GUI: xlayout::Class = xlayout::Class::new("idler.gui", 0);
const IFACE_MANAGER: xlayout::Class = xlayout::Class::new("idler.manager", 0);
const CONTAINER: xlayout::Class = xlayout::Class::new("CContainerWindow", 0x36ba978);
const CONTAINER_BASE: u64 = 0x30;
const MENU_CLASS: xlayout::Class = xlayout::Class::new("CInGameMenu", 0);
const TOPBAR: xlayout::Class = xlayout::Class::new("manager.topbar", 0);
const STATE_VIEW: xlayout::Class = xlayout::Class::new("manager.state_view", 0);
const DIPLOMACY_VIEW: xlayout::Class = xlayout::Class::new("manager.diplomacy_view", 0);
const AIR_REGION_VIEW: xlayout::Class = xlayout::Class::new("manager.air_region_view", 0);
const FIND: xlayout::Class = xlayout::Class::new("manager.find_view", 0);
const FIND_ROW: xlayout::Class = xlayout::Class::new("find.result_row", 0);

/// Shown notice pop-ups (list order) with their OK button rectangles.
fn notices(r: &Reader<'_>, image: u64) -> Result<Vec<gui_tree::Rect>> {
    Ok(notices_with_kind(r, image)?
        .into_iter()
        .map(|(rect, _)| rect)
        .collect())
}
/// Shown notices with their window class (vtable offset in the image).
fn notices_with_kind(r: &Reader<'_>, image: u64) -> Result<Vec<(gui_tree::Rect, u64)>> {
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + fld(abi, &IDLER, GUI_FROM_IDLER)?)?;
    let (_, data, count) = r.array_header(gui + fld(abi, &GAME_GUI, POPUP_LIST)?)?;
    let mut notices = Vec::with_capacity(NOTICES.len());
    for class in &NOTICES {
        notices.push((image + img(abi, class.vtable)?, class));
    }
    let mut out = Vec::new();
    for index in 0..count.min(32) as u64 {
        let window = r.u64(data + index * 8)?;
        let kind = r.u64(window)?;
        let Some((_, class)) = notices.iter().find(|(vt, _)| *vt == kind) else {
            continue;
        };
        let root_at = fld(abi, class, POPUP_ROOT)?;
        let Some(root) = gui_tree::container(r, r.u64(window + root_at)?, image)? else {
            continue;
        };
        if !gui_tree::visible(r, root)? {
            continue;
        }
        if let Some(ok) = gui_tree::child(r, image, root, "ok_button")? {
            // A notice whose button is laid out off screen cannot block a click.
            if gui_tree::shown(r, ok)? {
                if let Ok(rect) = gui_tree::rect(r, image, ok, (1920, 1080)) {
                    out.push((rect, class.vtable));
                }
            }
        }
    }
    Ok(out)
}
/// What a notice window class is, as far as the build notes say. Window text is
/// not read, so most classes are only named by their offset.
fn notice_kind_name(vtable: u64) -> String {
    match vtable {
        0x36805d0 => "confirmation pop-up (CConfirmationPopUpWindow, e.g. a doctrine milestone)".into(),
        0x3695460 => "surrendered pop-up (CSurrenderedPopupWindow, seized equipment)".into(),
        0x36803a0 => "special project completed pop-up (CSpecialProjectFinishedPopUpWindow)".into(),
        0x36804b8 => "special project captured pop-up (CSpecialProjectCapturedPopUpWindow)".into(),
        other => format!("notice window class +{other:#x} (research/focus finished, peace summary or world tension)"),
    }
}
/// The ui_blocked refusal: what was still on screen, from memory only.
fn blocked_error(count: usize, shown: &[u64], events: &[String]) -> Error {
    let kinds = shown
        .iter()
        .map(|k| notice_kind_name(*k))
        .collect::<Vec<_>>()
        .join("; ");
    let kinds = if kinds.is_empty() {
        "classes unreadable".to_string()
    } else {
        kinds
    };
    let events = if events.is_empty() {
        "no event window read as open".to_string()
    } else {
        format!("open events: {}", events.join(", "))
    };
    Error::new(
        "ui_blocked",
        format!(
            "Notice pop-ups did not close: {} still shown ({kinds}); {events}. Window text is not read. \
             Clicking their OK buttons did not reduce the count; answer any listed event first \
             (hoi4_events), then retry once the pop-ups are gone.",
            count
        ),
    )
}
/// Acknowledge shown notice pop-ups so they cannot swallow the next click.
/// Stacking order is not in the list, so candidates are tried from the end;
/// a click counts when any notice closes (a covered OK lands harmlessly on the
/// covering pop-up's body).
pub(super) fn clear_notices(process: &dyn Process, keys: &dyn GameInput) -> Result<usize> {
    let started = Instant::now();
    let out = clear_notices_inner(process, keys);
    trace("clear_notices", started);
    out
}
fn clear_notices_inner(process: &dyn Process, keys: &dyn GameInput) -> Result<usize> {
    close_menu(process, keys)?;
    let _ = super::panelkit::close_focus_tree(process, keys);
    close_state_view(process, keys)?;
    // A construction panel left open turns later map clicks into placements.
    super::construction::leave(process, keys);
    // A confirmation left open by an interrupted action blocks every click.
    decline_stale(process, keys)?;
    acknowledge_news(process, keys)?;
    let cleared = match dismiss_notices(process, keys) {
        Ok(n) => n,
        Err(e) => {
            // A notice that will not close while an event is open is cleared by answering
            // the event: say so instead of ui_blocked (seen live in the Italy test campaign).
            let open = read(process, super::events::open_ids).unwrap_or_default();
            if !open.is_empty() {
                return Err(Error::new(
                    "events_open",
                    format!("Answer the open event(s) first: {} (a notice behind them stays until then)", open.join(", ")),
                ));
            }
            return Err(e);
        }
    };
    // One-option events (news) decide nothing: acknowledge them instead of refusing the action
    // (a news event refused every army action live, 6 Nov 2026). Real choices stay with the player.
    acknowledge_news(process, keys)?;
    // An event window is drawn over panels; clicks would land on it, so the
    // player answers events before anything else.
    let open = read(process, super::events::open_ids)?;
    if !open.is_empty() {
        return Err(Error::new(
            "events_open",
            format!("Answer the open event(s) first: {}", open.join(", ")),
        ));
    }
    Ok(cleared)
}
/// Answer one-option events on top (news: nothing to decide), up to eight in a row.
fn acknowledge_news(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    for _ in 0..8 {
        let Some(id) = read(process, super::events::single_option_top)? else {
            break;
        };
        if super::events::choose(process, keys, &id, 0).is_err() {
            break;
        }
    }
    Ok(())
}
/// Click away every notice pop-up (they can sit on top of an event window).
pub(super) fn dismiss_notices(process: &dyn Process, keys: &dyn GameInput) -> Result<usize> {
    let mut cleared = 0;
    let mut misses = 0;
    loop {
        // No notice at all needs no settling (the two reads 60 ms apart only matter for one
        // sliding in): this check runs before nearly every action.
        if read(process, notices)?.is_empty() {
            return Ok(cleared);
        }
        let shown = settled(process, "notice", notices)?;
        let Some(&target) = shown.iter().rev().nth(misses) else {
            return if shown.is_empty() {
                Ok(cleared)
            } else {
                // Best effort: the kinds and events are context only.
                let kinds = read(process, notices_with_kind)
                    .map(|v| v.into_iter().map(|(_, k)| k).collect::<Vec<_>>())
                    .unwrap_or_default();
                let events = read(process, super::events::open_ids).unwrap_or_default();
                Err(blocked_error(shown.len(), &kinds, &events))
            };
        };
        if cleared + misses >= 12 {
            return Err(Error::new("ui_blocked", "Too many notice pop-ups"));
        }
        let (x, y) = target.center();
        keys.click((1920, 1080), x, y)?;
        let before = shown.len();
        match wait(process, "dismiss notice", |r, i| {
            Ok(notices(r, i)?.len() < before)
        }) {
            Ok(()) => {
                cleared += 1;
                misses = 0;
            }
            Err(_) => misses += 1,
        }
    }
}
/// The in-game menu (Escape on a free map): interface manager +0x218 holds
/// CInGameMenu whose window (+0x2678) shows while the menu is open (macOS
/// 089EA519, mapped live on 18 October 2026).
const MENU: u64 = 0x218;
const MENU_WINDOW: u64 = 0x2a38;
fn menu_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let menu = menu(r, image)?;
    let window = r.u64(menu + fld(r.string_abi(), &MENU_CLASS, MENU_WINDOW)?)?;
    match gui_tree::container(r, window + container_base(r)?, image)? {
        Some(object) => gui_tree::visible(r, object),
        None => Err(Error::new("invalid_gui", "Game menu window required")),
    }
}
/// CInGameMenu's save window (CInGameMenuSaveWindow, 1.19.4: menu +0x2a48).
const MENU_SAVE_WINDOW: u64 = 0x2a48;
/// The interface manager (idler +0x670).
fn manager(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(idler + fld(r.string_abi(), &IDLER, MANAGER)?)
}
fn menu(r: &Reader<'_>, image: u64) -> Result<u64> {
    r.u64(manager(r, image)? + fld(r.string_abi(), &IFACE_MANAGER, MENU)?)
}
fn container_base(r: &Reader<'_>) -> Result<u64> {
    fld(r.string_abi(), &CONTAINER, CONTAINER_BASE)
}
fn menu_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let menu = menu(r, image)?;
    let window = r.u64(menu + fld(r.string_abi(), &MENU_CLASS, MENU_WINDOW)?)?;
    gui_tree::container(r, window + container_base(r)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Game menu window required"))
}
/// The shown save window's root (container named `ingamemenu_save`), found by name inside the
/// save-window object so no root offset is assumed.
fn save_root(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    let menu = menu(r, image)?;
    let window = r.u64(menu + fld(r.string_abi(), &MENU_CLASS, MENU_SAVE_WINDOW)?)?;
    if !(0x10000..=0x7fff_ffff_0000).contains(&window) {
        return Ok(None);
    }
    // The root sat at +0x140 (and +0xa80) on 1 Nov 2026; scan rather than assume.
    for offset in (0x100..0xb00).step_by(8) {
        let field = r.u64(window + offset)?;
        if !(0x10000..=0x7fff_ffff_0000).contains(&field) {
            continue;
        }
        let Ok(Some(root)) = gui_tree::container(r, field, image) else {
            continue;
        };
        if gui_tree::name(r, root + container_base(r)?, image)?.as_deref()
            == Some("ingamemenu_save")
        {
            return Ok(gui_tree::visible(r, root)?.then_some(root));
        }
    }
    Ok(None)
}
/// Save the running game with the game's own Save dialog (Escape, Save, Save) under the name the
/// game proposes, and verify that a save file in `saves` was written after the call started.
/// Used at the end of a campaign segment so the next segment continues exactly where it stopped.
pub(super) fn save_game(
    process: &dyn Process,
    keys: &dyn GameInput,
    saves: &std::path::Path,
) -> Result<serde_json::Value> {
    use crate::platform::game_input::Key;
    let newest = || -> Option<(std::time::SystemTime, String)> {
        std::fs::read_dir(saves)
            .ok()?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if !name.ends_with(".hoi4") {
                    return None;
                }
                Some((e.metadata().ok()?.modified().ok()?, name))
            })
            .max()
    };
    let before = newest().map(|(t, _)| t);
    clear_notices(process, keys)?;
    // Escape first closes whatever view is open (a trade or politics view left open took the
    // only Escape live, 6 Nov 2026), so press it until the menu itself is open.
    let mut presses = 0;
    while !read(process, menu_open)? {
        if presses == 4 {
            return Err(Error::new(
                "input_readback_failed",
                "game menu open: four Escape presses did not open the game menu",
            ));
        }
        keys.press(Key::Escape)?;
        presses += 1;
        let deadline = Instant::now() + Duration::from_millis(900);
        while Instant::now() < deadline && !read(process, menu_open)? {
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let button = settled(process, "save button", |r, i| {
        let root = menu_root(r, i)?;
        let b = gui_tree::child(r, i, root, "menu_save_button")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Save button not found"))?;
        gui_tree::rect(r, i, b, (1920, 1080))
    })?;
    // These buttons are drawn by a text sprite whose width is the text texture's (864 px for
    // "Save Game"), so its centre lies outside the button: click near the left edge instead.
    let near_left =
        |r: gui_tree::Rect| (f64::from(r.x) + 40.0, f64::from(r.y) + f64::from(r.h) / 2.0);
    let (x, y) = near_left(button);
    keys.click((1920, 1080), x, y)?;
    wait(
        process,
        "save window",
        |r, i| Ok(save_root(r, i)?.is_some()),
    )?;
    let confirm = settled(process, "save confirm", |r, i| {
        let root =
            save_root(r, i)?.ok_or_else(|| Error::new("invalid_gui", "Save window closed"))?;
        let b = gui_tree::child(r, i, root, "save")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Save confirm button not found"))?;
        gui_tree::rect(r, i, b, (1920, 1080))
    })?;
    let (x, y) = near_left(confirm);
    keys.click((1920, 1080), x, y)?;
    // Overwriting a save of the same name asks for confirmation.
    let _ = accept_command(process, keys);
    let deadline = Instant::now() + Duration::from_secs(60);
    let written = loop {
        if let Some((t, name)) = newest() {
            if before.is_none_or(|b| t > b) {
                break name;
            }
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "No save file was written within 60 s",
            ));
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    // Let the writer finish, then close the menu.
    std::thread::sleep(Duration::from_secs(3));
    close_menu(process, keys)?;
    Ok(serde_json::json!({"saved":written,"directory":saves.display().to_string()}))
}
/// Close the game menu if an earlier input left it open (Escape closes it).
pub(super) fn close_menu(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if read(process, menu_open)? {
        keys.press(crate::platform::game_input::Key::Escape)?;
        wait(process, "game menu closed", |r, i| Ok(!menu_open(r, i)?))?;
    }
    Ok(())
}
/// Top bar (interface manager +0x188, root +0x68) button `name` rectangle.
fn topbar_rect(r: &Reader<'_>, image: u64, name: &str) -> Result<gui_tree::Rect> {
    fn find(
        r: &Reader<'_>,
        image: u64,
        object: u64,
        name: &str,
        depth: u32,
    ) -> Result<Option<u64>> {
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
    let abi = r.string_abi();
    let topbar = r.u64(manager(r, image)? + fld(abi, &IFACE_MANAGER, 0x188)?)?;
    let root = gui_tree::container(r, r.u64(topbar + fld(abi, &TOPBAR, 0x68)?)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Topbar root unavailable"))?;
    let button = find(r, image, root, name, 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Top bar button not found"))?;
    gui_tree::rect(r, image, button, (1920, 1080))
}
/// Open the politics view: the Q hotkey, and when it does nothing (it was
/// seen to stop working after a focus completed) a click on the player flag,
/// which is the button that owns that shortcut.
pub(super) fn open_politics(
    process: &dyn Process,
    keys: &dyn GameInput,
    open: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<()> {
    use crate::platform::game_input::Key;
    keys.press(Key::letter('q')?)?;
    if wait(process, "open politics", &open).is_ok() {
        return Ok(());
    }
    // The flag is drawn with a sprite class without reviewed geometry; it sits
    // at the top-left corner of the top bar (topbar.gui: position 12,14).
    let (x, y) = match read(process, |r, i| topbar_rect(r, i, "player_flag")) {
        Ok(rect) => rect.center(),
        Err(_) => (45.0, 48.0),
    };
    keys.click((1920, 1080), x, y)?;
    wait(process, "open politics", &open)
}
/// The state view (manager +0xc8, root +0x430) that a stray map click can
/// leave open over the army panels: its close button when it is shown.
fn state_view_close(r: &Reader<'_>, image: u64) -> Result<Option<gui_tree::Rect>> {
    view_close(r, image, 0xc8, &STATE_VIEW)
}
/// The diplomacy view (manager +0x110) covers the army panel and cards.
fn diplomacy_close(r: &Reader<'_>, image: u64) -> Result<Option<gui_tree::Rect>> {
    view_close(r, image, 0x110, &DIPLOMACY_VIEW)
}
/// The strategic air region window (manager +0x198) that a right click on a
/// region leaves open over the middle of the map.
fn air_region_close(r: &Reader<'_>, image: u64) -> Result<Option<gui_tree::Rect>> {
    view_close(r, image, 0x198, &AIR_REGION_VIEW)
}
fn view_close(
    r: &Reader<'_>,
    image: u64,
    slot: u64,
    class: &xlayout::Class,
) -> Result<Option<gui_tree::Rect>> {
    let abi = r.string_abi();
    let view = r.u64(manager(r, image)? + fld(abi, &IFACE_MANAGER, slot)?)?;
    let Some(root) = gui_tree::container(r, r.u64(view + fld(abi, class, 0x490)?)?, image)? else {
        return Ok(None);
    };
    if !gui_tree::visible(r, root)? {
        return Ok(None);
    }
    match gui_tree::child(r, image, root, "close_button")? {
        Some(button) => Ok(gui_tree::rect(r, image, button, (1920, 1080)).ok()),
        None => Ok(None),
    }
}
/// Close the state view and the air region window if they are open.
pub(super) fn close_state_view(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    for (what, probe) in [
        ("state view closed", state_view_close as ViewClose),
        ("air region window closed", air_region_close as ViewClose),
        ("diplomacy view closed", diplomacy_close as ViewClose),
    ] {
        if let Some(rect) = read(process, probe)? {
            let (x, y) = rect.center();
            keys.click((1920, 1080), x, y)?;
            wait(process, what, |r, i| Ok(probe(r, i)?.is_none()))?;
        }
    }
    Ok(())
}
type ViewClose = fn(&Reader<'_>, u64) -> Result<Option<gui_tree::Rect>>;
/// The game's accept/decline dialog for a command (e.g. replacing a law).
const ACCEPT_DIALOG_VTABLE: u64 = 0x363b690;
/// The game's reassign-commander confirmation (CChangeGroupLeaderDialog); it
/// has the same accept and decline buttons.
const REASSIGN_DIALOG_VTABLE: u64 = 0x3643990;
/// The same confirmation for a fleet's admiral (CChangeNavyLeaderDialog, "Reassign the
/// Commander", Cancel/OK; seen live 26 October 2026 when an admiral of another fleet was chosen).
const NAVY_REASSIGN_DIALOG_VTABLE: u64 = 0x3643ac0;
/// "Reassign Armies": taking every division out of an army would delete it
/// and its orders (CReassignFullOrdersGroupDialog); same accept/decline buttons.
const REASSIGN_ARMIES_DIALOG_VTABLE: u64 = 0x368b8a8;
const COMMAND_DIALOGS: [xlayout::Class; 4] = [
    xlayout::Class::new("CAcceptCommandDialog", ACCEPT_DIALOG_VTABLE),
    xlayout::Class::new("CChangeGroupLeaderDialog", REASSIGN_DIALOG_VTABLE),
    xlayout::Class::new("CChangeNavyLeaderDialog", NAVY_REASSIGN_DIALOG_VTABLE),
    xlayout::Class::new(
        "CReassignFullOrdersGroupDialog",
        REASSIGN_ARMIES_DIALOG_VTABLE,
    ),
];
/// The game GUI's popup window list (data pointer and count).
fn popups(r: &Reader<'_>, image: u64) -> Result<(u64, usize)> {
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + fld(abi, &IDLER, GUI_FROM_IDLER)?)?;
    let (_, data, count) = r.array_header(gui + fld(abi, &GAME_GUI, POPUP_LIST)?)?;
    Ok((data, count))
}

fn command_dialog(r: &Reader<'_>, image: u64, button: &str) -> Result<Option<gui_tree::Rect>> {
    let abi = r.string_abi();
    let (data, count) = popups(r, image)?;
    let mut dialogs = Vec::with_capacity(COMMAND_DIALOGS.len());
    for class in &COMMAND_DIALOGS {
        dialogs.push((image + img(abi, class.vtable)?, class));
    }
    for index in 0..count.min(32) as u64 {
        let window = r.u64(data + index * 8)?;
        let kind = r.u64(window)?;
        let Some((_, class)) = dialogs.iter().find(|(vt, _)| *vt == kind) else {
            continue;
        };
        let root_at = fld(abi, class, POPUP_ROOT)?;
        let Some(root) = gui_tree::container(r, r.u64(window + root_at)?, image)? else {
            continue;
        };
        if !gui_tree::visible(r, root)? {
            continue;
        }
        if let Some(widget) = gui_tree::child(r, image, root, button)? {
            return Ok(Some(gui_tree::rect(r, image, widget, (1920, 1080))?));
        }
    }
    // The game's generic confirmation window by its .gui name (its class was missing from the
    // list above on 1.19.4.1: delete-task-force and lost-progress confirmations stayed open).
    for root in super::panelkit::popup_roots_named(r, image, &["default_confirmation_popup"])? {
        if let Some(widget) = gui_tree::child(r, image, root, button)? {
            return Ok(Some(gui_tree::rect(r, image, widget, (1920, 1080))?));
        }
    }
    Ok(None)
}
/// Whether a "Reassign Armies" confirmation is open.
pub(super) fn reassign_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let (data, count) = popups(r, image)?;
    let want = image + img(r.string_abi(), REASSIGN_ARMIES_DIALOG_VTABLE)?;
    for index in 0..count.min(32) as u64 {
        if r.u64(r.u64(data + index * 8)?)? == want {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Decline a command dialog left open by an earlier, interrupted action.
pub(super) fn decline_stale(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if let Some(rect) = read(process, |r, i| command_dialog(r, i, "decline_button"))? {
        let (x, y) = rect.center();
        keys.click((1920, 1080), x, y)?;
        wait(process, "decline dialog", |r, i| {
            Ok(command_dialog(r, i, "decline_button")?.is_none())
        })?;
    }
    Ok(())
}
/// After the requested click: accept the game's confirmation of exactly that
/// command if it appears within a short bound. Returns whether one was accepted.
pub(super) fn accept_command(process: &dyn Process, keys: &dyn GameInput) -> Result<bool> {
    accept_command_until(process, keys, |_, _| Ok(false))
}
/// `accept_command` that stops waiting for a dialog as soon as `done` holds (the order took
/// effect without one), instead of the full 1.2 s.
pub(super) fn accept_command_until(
    process: &dyn Process,
    keys: &dyn GameInput,
    done: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<bool> {
    let deadline = Instant::now() + Duration::from_millis(1200);
    loop {
        if let Some(rect) = read(process, |r, i| command_dialog(r, i, "accept_button"))? {
            let rect = settled(process, "accept", |r, i| {
                command_dialog(r, i, "accept_button")?
                    .ok_or_else(|| Error::new("input_readback_failed", "Dialog closed"))
            })
            .unwrap_or(rect);
            let (x, y) = rect.center();
            keys.click((1920, 1080), x, y)?;
            wait(process, "accept dialog", |r, i| {
                Ok(command_dialog(r, i, "accept_button")?.is_none())
            })?;
            return Ok(true);
        }
        if Instant::now() >= deadline || read(process, &done)? {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(40));
    }
}
/// The game's Find view (interface manager +0x190) on macOS 089EA519: own
/// idler +0xc30, window +0xc38, query text +0xc48, highlighted result
/// +0xc88 whose state is at +0x3f8 (state ID +0x40). 1.19.4: +0x120 for the view fields, state +0x458.
const MANAGER: u64 = 0x670;
const FIND_VIEW: u64 = 0x190;
const FIND_IDLER: u64 = 0xd50;
const FIND_WINDOW: u64 = 0xd58;
const FIND_TEXT: u64 = 0xd68;
const FIND_SELECTED: u64 = 0xda8;
const RESULT_STATE: u64 = 0x458;
const DIGITS: [u16; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
const BACKSPACE: u16 = 51;
/// ANSI virtual key code of F.
const FIND_KEY: u16 = 3;

fn find_view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let abi = r.string_abi();
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + fld(abi, &IDLER, MANAGER)?)?;
    let find = r.u64(manager + fld(abi, &IFACE_MANAGER, FIND_VIEW)?)?;
    if r.u64(find + fld(abi, &FIND, FIND_IDLER)?)? != idler {
        return Err(Error::new("invalid_gui", "Own Find view required"));
    }
    Ok(find)
}
pub(super) fn find_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let find = find_view(r, image)?;
    let window = r.u64(find + fld(r.string_abi(), &FIND, FIND_WINDOW)?)?;
    match gui_tree::container(r, window, image)? {
        Some(window) => gui_tree::visible(r, window),
        None => Ok(false),
    }
}
/// (query text, highlighted state ID if the highlighted result is a state).
fn find_result(r: &Reader<'_>, image: u64) -> Result<(String, Option<i32>)> {
    let find = find_view(r, image)?;
    let abi = r.string_abi();
    let text = r.string_bounded(find + fld(abi, &FIND, FIND_TEXT)?, 64)?;
    let row = r.u64(find + fld(abi, &FIND, FIND_SELECTED)?)?;
    if row == 0 {
        return Ok((text, None));
    }
    let state = r.u64(row + fld(abi, &FIND_ROW, RESULT_STATE)?)?;
    let states = crate::construction::layout(r);
    let id = if state != 0 && r.u64(state)? == image + states.state_vtable {
        Some(r.i32(state + states.state_id)?)
    } else {
        None
    };
    Ok((text, id))
}
/// Centre the camera on own or foreign state `state` with the game's Find
/// view; Return is pressed only when the query and the highlighted state both
/// match, otherwise Find is closed and the step refused.
pub(super) fn find_state(process: &dyn Process, keys: &dyn GameInput, state: i32) -> Result<()> {
    use crate::platform::game_input::Key;
    // The Recruit & Deploy panel covers the left half of the map, where the
    // box selections and map clicks that follow a Find would otherwise land.
    super::deployment::leave(process, keys);
    // 1.19.4 rebound the Find view from F to Ctrl+F.
    keys.press_control(FIND_KEY)?;
    wait(process, "open find", find_open)?;
    thread::sleep(Duration::from_millis(80));
    // The box keeps the previous query; erase it before typing.
    let previous = read(process, |r, i| Ok(find_result(r, i)?.0.chars().count()))?;
    for _ in 0..previous.min(64) {
        keys.press_code(BACKSPACE)?;
        thread::sleep(Duration::from_millis(15));
    }
    wait(process, "find query cleared", |r, i| {
        Ok(find_result(r, i)?.0.is_empty())
    })?;
    let query = state.to_string();
    for digit in query.bytes() {
        keys.press_code(DIGITS[(digit - b'0') as usize])?;
        thread::sleep(Duration::from_millis(25));
    }
    let matched = wait(process, "find result", |r, i| {
        let (text, id) = find_result(r, i)?;
        Ok(text == query && id == Some(state))
    });
    if let Err(error) = matched {
        let _ = keys.press(Key::Escape);
        return Err(Error::new(
            "input_readback_failed",
            format!("Find did not highlight state {state}: {}", error.message),
        ));
    }
    keys.press(Key::Return)?;
    wait(process, "find closes", |r, i| Ok(!find_open(r, i)?))?;
    // The camera glides to the state: wait for it to stop instead of a fixed 400 ms.
    thread::sleep(Duration::from_millis(60));
    super::screenmap::wait_camera_still(process, 700);
    Ok(())
}
/// Press Escape (at most three times) until `closed` holds; best effort, used
/// to leave the UI tidy after the game refuses a target.
pub(super) fn dismiss(
    process: &dyn Process,
    keys: &dyn GameInput,
    closed: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) {
    for _ in 0..3 {
        if read(process, &closed).unwrap_or(true) {
            return;
        }
        if keys
            .press(crate::platform::game_input::Key::Escape)
            .is_err()
        {
            return;
        }
        // A closing window can take a moment; an Escape too many would open
        // the game menu, so wait for this one to take effect first.
        let deadline = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < deadline {
            if read(process, &closed).unwrap_or(true) {
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
    }
}
/// A click target whose rectangle is identical in two reads 60 ms apart, so
/// sliding panels and animated windows have come to rest before input.
pub(super) fn settled<T: PartialEq>(
    process: &dyn Process,
    stage: &str,
    f: impl Fn(&Reader<'_>, u64) -> Result<T>,
) -> Result<T> {
    let deadline = Instant::now() + SETTLE_WAIT;
    // A panel sliding in reports its widgets off screen for a moment.
    let attempt = || loop {
        match read(process, &f) {
            Err(e)
                if Instant::now() < deadline
                    && matches!(e.code, "gui_geometry_unavailable" | "invalid_gui_geometry") =>
            {
                thread::sleep(Duration::from_millis(60));
            }
            result => return result,
        }
    };
    let started = Instant::now();
    let mut last = attempt()?;
    loop {
        thread::sleep(Duration::from_millis(60));
        let now = attempt()?;
        if now == last {
            trace(stage, started);
            return Ok(now);
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "gui_target_unsettled",
                format!("{stage}: click target kept moving"),
            ));
        }
        last = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocked_error_names_notice_classes_and_open_events() {
        let e = blocked_error(2, &[0x36805d0, 0x3680170], &["news.288".to_string()]);
        assert_eq!(e.code, "ui_blocked");
        assert!(e.message.contains("2 still shown"));
        assert!(e.message.contains("CConfirmationPopUpWindow"));
        assert!(e.message.contains("+0x3680170"));
        assert!(e.message.contains("open events: news.288"));
    }
    #[test]
    fn blocked_error_stays_useful_when_context_is_unreadable() {
        let e = blocked_error(1, &[], &[]);
        assert_eq!(e.code, "ui_blocked");
        assert!(e.message.contains("1 still shown"));
        assert!(e.message.contains("classes unreadable"));
        assert!(e.message.contains("no event window read as open"));
    }
}
