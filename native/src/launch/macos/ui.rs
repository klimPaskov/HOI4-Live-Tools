//! Shared fresh-read helpers for ordinary macOS UI actions.
use crate::{
    error::{Error, Result},
    game, gui_tree,
    memory::{Reader, StringAbi},
    platform::Process,
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

/// Bounded wait for a memory predicate after an input.
pub(super) fn wait(
    process: &dyn Process,
    stage: &str,
    f: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<()> {
    let deadline = Instant::now() + STAGE_WAIT;
    loop {
        if read(process, &f)? {
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
const NOTICE_VTABLES: [u64; 4] = [0x3472580, 0x3472af8, 0x3472c10, 0x3493e80];
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0x9e0;
const POPUP_ROOT: u64 = 0x430;

/// Shown notice pop-ups (list order) with their OK button rectangles.
fn notices(r: &Reader<'_>, image: u64) -> Result<Vec<gui_tree::Rect>> {
    if r.string_abi() != StringAbi::LibCxx {
        return Ok(Vec::new());
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    let (_, data, count) = r.array_header(gui + POPUP_LIST)?;
    let mut out = Vec::new();
    for index in 0..count.min(32) as u64 {
        let window = r.u64(data + index * 8)?;
        let kind = r.u64(window)?;
        if !NOTICE_VTABLES.iter().any(|vt| image + vt == kind) {
            continue;
        }
        let Some(root) = gui_tree::container(r, r.u64(window + POPUP_ROOT)?, image)? else {
            continue;
        };
        if !gui_tree::visible(r, root)? {
            continue;
        }
        if let Some(ok) = gui_tree::child(r, image, root, "ok_button")? {
            // A notice whose button is laid out off screen cannot block a click.
            if gui_tree::shown(r, ok)? {
                if let Ok(rect) = gui_tree::rect(r, image, ok, (1920, 1080)) {
                    out.push(rect);
                }
            }
        }
    }
    Ok(out)
}
/// Acknowledge shown notice pop-ups so they cannot swallow the next click.
/// Stacking order is not in the list, so candidates are tried from the end;
/// a click counts when any notice closes (a covered OK lands harmlessly on the
/// covering pop-up's body).
pub(super) fn clear_notices(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
) -> Result<usize> {
    close_menu(process, keys)?;
    close_state_view(process, keys)?;
    // A construction panel left open turns later map clicks into placements.
    super::construction::leave(process, keys);
    // A confirmation left open by an interrupted action blocks every click.
    decline_stale(process, keys)?;
    // An event window is drawn over notices and panels; clicks would land on
    // it, so the player answers events before anything else.
    let open = read(process, super::events::open_ids)?;
    if !open.is_empty() {
        return Err(Error::new(
            "events_open",
            format!("Answer the open event(s) first: {}", open.join(", ")),
        ));
    }
    let mut cleared = 0;
    let mut misses = 0;
    loop {
        let shown = settled(process, "notice", notices)?;
        let Some(&target) = shown.iter().rev().nth(misses) else {
            return if shown.is_empty() {
                Ok(cleared)
            } else {
                Err(Error::new("ui_blocked", "Notice pop-ups did not close"))
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
const MENU_WINDOW: u64 = 0x2678;
fn menu_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    if r.string_abi() != StringAbi::LibCxx {
        return Ok(false);
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let menu = r.u64(r.u64(idler + MANAGER)? + MENU)?;
    let window = r.u64(menu + MENU_WINDOW)?;
    match gui_tree::container(r, window + 0x30, image)? {
        Some(object) => gui_tree::visible(r, object),
        None => Err(Error::new("invalid_gui", "Game menu window required")),
    }
}
/// Close the game menu if an earlier input left it open (Escape closes it).
pub(super) fn close_menu(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
) -> Result<()> {
    if read(process, menu_open)? {
        keys.press(crate::platform::macos_input::Key::Escape)?;
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
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let topbar = r.u64(r.u64(idler + MANAGER)? + 0x188)?;
    let root = gui_tree::container(r, r.u64(topbar + 0x68)?, image)?
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
    keys: &crate::platform::macos_input::MacKeys,
    open: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<()> {
    use crate::platform::macos_input::Key;
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
    view_close(r, image, 0xc8)
}
/// The strategic air region window (manager +0x198) that a right click on a
/// region leaves open over the middle of the map.
fn air_region_close(r: &Reader<'_>, image: u64) -> Result<Option<gui_tree::Rect>> {
    view_close(r, image, 0x198)
}
fn view_close(r: &Reader<'_>, image: u64, slot: u64) -> Result<Option<gui_tree::Rect>> {
    if r.string_abi() != StringAbi::LibCxx {
        return Ok(None);
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + MANAGER)? + slot)?;
    let Some(root) = gui_tree::container(r, r.u64(view + 0x430)?, image)? else {
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
pub(super) fn close_state_view(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
) -> Result<()> {
    for (what, probe) in [
        ("state view closed", state_view_close as ViewClose),
        ("air region window closed", air_region_close as ViewClose),
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
const ACCEPT_DIALOG_VTABLE: u64 = 0x342f1d0;
/// The game's reassign-commander confirmation (CChangeGroupLeaderDialog); it
/// has the same accept and decline buttons.
const REASSIGN_DIALOG_VTABLE: u64 = 0x3437028;
/// "Reassign Armies": taking every division out of an army would delete it
/// and its orders (CReassignFullOrdersGroupDialog); same accept/decline buttons.
const REASSIGN_ARMIES_DIALOG_VTABLE: u64 = 0x347dba8;

fn command_dialog(r: &Reader<'_>, image: u64, button: &str) -> Result<Option<gui_tree::Rect>> {
    if r.string_abi() != StringAbi::LibCxx {
        return Ok(None);
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    let (_, data, count) = r.array_header(gui + POPUP_LIST)?;
    for index in 0..count.min(32) as u64 {
        let window = r.u64(data + index * 8)?;
        let kind = r.u64(window)?;
        if kind != image + ACCEPT_DIALOG_VTABLE
            && kind != image + REASSIGN_DIALOG_VTABLE
            && kind != image + REASSIGN_ARMIES_DIALOG_VTABLE
        {
            continue;
        }
        let Some(root) = gui_tree::container(r, r.u64(window + POPUP_ROOT)?, image)? else {
            continue;
        };
        if !gui_tree::visible(r, root)? {
            continue;
        }
        if let Some(widget) = gui_tree::child(r, image, root, button)? {
            return Ok(Some(gui_tree::rect(r, image, widget, (1920, 1080))?));
        }
    }
    Ok(None)
}
/// Whether a "Reassign Armies" confirmation is open.
pub(super) fn reassign_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    if r.string_abi() != StringAbi::LibCxx {
        return Ok(false);
    }
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    let (_, data, count) = r.array_header(gui + POPUP_LIST)?;
    for index in 0..count.min(32) as u64 {
        if r.u64(r.u64(data + index * 8)?)? == image + REASSIGN_ARMIES_DIALOG_VTABLE {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Decline a command dialog left open by an earlier, interrupted action.
pub(super) fn decline_stale(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
) -> Result<()> {
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
pub(super) fn accept_command(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
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
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(40));
    }
}
/// The game's Find view (interface manager +0x190) on macOS 089EA519: own
/// idler +0xc30, window +0xc38, query text +0xc48, highlighted result
/// +0xc88 whose state is at +0x3f8 (state ID +0x40).
const MANAGER: u64 = 0x670;
const FIND_VIEW: u64 = 0x190;
const FIND_IDLER: u64 = 0xc30;
const FIND_WINDOW: u64 = 0xc38;
const FIND_TEXT: u64 = 0xc48;
const FIND_SELECTED: u64 = 0xc88;
const RESULT_STATE: u64 = 0x3f8;
const STATE_ID: u64 = 0x40;
const DIGITS: [u16; 10] = [29, 18, 19, 20, 21, 23, 22, 26, 28, 25];
const BACKSPACE: u16 = 51;

fn find_view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let find = r.u64(r.u64(idler + MANAGER)? + FIND_VIEW)?;
    if r.u64(find + FIND_IDLER)? != idler {
        return Err(Error::new("invalid_gui", "Own Find view required"));
    }
    Ok(find)
}
pub(super) fn find_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let find = find_view(r, image)?;
    match gui_tree::container(r, r.u64(find + FIND_WINDOW)?, image)? {
        Some(window) => gui_tree::visible(r, window),
        None => Ok(false),
    }
}
/// (query text, highlighted state ID if the highlighted result is a state).
fn find_result(r: &Reader<'_>, image: u64) -> Result<(String, Option<i32>)> {
    let find = find_view(r, image)?;
    let text = r.string_bounded(find + FIND_TEXT, 64)?;
    let row = r.u64(find + FIND_SELECTED)?;
    if row == 0 {
        return Ok((text, None));
    }
    let state = r.u64(row + RESULT_STATE)?;
    let id = if state != 0 && r.u64(state)? == image + crate::construction::layout(r).state_vtable {
        Some(r.i32(state + STATE_ID)?)
    } else {
        None
    };
    Ok((text, id))
}
/// Centre the camera on own or foreign state `state` with the game's Find
/// view; Return is pressed only when the query and the highlighted state both
/// match, otherwise Find is closed and the step refused.
pub(super) fn find_state(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
    state: i32,
) -> Result<()> {
    use crate::platform::macos_input::Key;
    keys.press(Key::letter('f')?)?;
    wait(process, "open find", find_open)?;
    thread::sleep(Duration::from_millis(150));
    // The box keeps the previous query; erase it before typing.
    let previous = read(process, |r, i| Ok(find_result(r, i)?.0.chars().count()))?;
    for _ in 0..previous.min(64) {
        keys.press_code(BACKSPACE)?;
        thread::sleep(Duration::from_millis(30));
    }
    wait(process, "find query cleared", |r, i| {
        Ok(find_result(r, i)?.0.is_empty())
    })?;
    let query = state.to_string();
    for digit in query.bytes() {
        keys.press_code(DIGITS[(digit - b'0') as usize])?;
        thread::sleep(Duration::from_millis(60));
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
    thread::sleep(Duration::from_millis(400));
    Ok(())
}
/// Press Escape (at most three times) until `closed` holds; best effort, used
/// to leave the UI tidy after the game refuses a target.
pub(super) fn dismiss(
    process: &dyn Process,
    keys: &crate::platform::macos_input::MacKeys,
    closed: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) {
    for _ in 0..3 {
        if read(process, &closed).unwrap_or(true) {
            return;
        }
        if keys
            .press(crate::platform::macos_input::Key::Escape)
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
    let mut last = attempt()?;
    loop {
        thread::sleep(Duration::from_millis(60));
        let now = attempt()?;
        if now == last {
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
