//! Shared plumbing for the tools built offline from the game's GUI definitions (fleet toolbar,
//! naval invasion order, faction windows, incoming proposals). None of them has a live test yet:
//! every result carries `validated:false`, every window is found by NAME (never by an offset
//! copied from another platform) and every step is verified from memory before the next one.
//!
//! * `run`: the common action harness (journaled intent, fixed up-front fence deadline, guarded
//!   keys, settle by pure rule, idempotent retry of an unresolved request id, no input retry);
//! * window discovery: a view is found by its class vtable among the interface manager slots, a
//!   window inside a view by scanning the view's pointer fields for a container with that GUI
//!   name, a widget below a window by a bounded depth-first search;
//! * pop-ups: the game GUI's pop-up list (idler +0x4c0, list +0x9e0, root +0x430, the same list the
//!   notice and command dialogs of `ui.rs` use) read for diplomacy proposals and confirmations.
use super::{
    action_fence,
    journal::{GameJournal, Outcome},
    navalui as nui,
    ui::{read, wait},
};
use crate::{
    actions::Epoch,
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    input_lease::InputAuthority,
    memory::Reader,
    platform::{macos_input::MacKeys, Process},
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

pub(super) const GAME: (u32, u32) = (1920, 1080);
const MANAGER: u64 = 0x670;
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0x9e0;
const POPUP_ROOT: u64 = 0x430;
const CONTAINER_BASE: u64 = 0x30;
/// Prefix of every description: nothing here has been seen working on a live game yet.
pub(super) const UNVALIDATED: &str = "UNVALIDATED (no live test yet): ";

/// What every dispatcher needs; borrowed from the controller state in `MacClock::call`.
pub(super) struct Env<'a> {
    pub process: &'a (dyn Process + Send + Sync),
    pub keys: &'a MacKeys<'static>,
    pub guard: &'a mut super::Guard,
    pub authority: &'a InputAuthority,
    pub actions: &'a mut GameJournal,
}
/// Identity and bounds of one journaled action.
pub(super) struct Request<'a> {
    pub id: &'a str,
    pub token: &'a str,
    /// Up-front fence deadline in seconds (never renewed).
    pub seconds: u64,
    pub action: Value,
}
pub(super) type PlanFn<'a, P> = &'a dyn Fn(&dyn Process) -> Result<P>;
pub(super) type SettleFn<'a, P> = &'a dyn Fn(&dyn Process, Option<&P>) -> Result<Outcome>;
pub(super) type DispatchFn<'a, P> = &'a dyn Fn(&dyn Process, &MacKeys<'_>, &P) -> Result<Value>;
/// Run one journaled action. `plan` reads the world and refuses before any input; on a retry of
/// an unresolved request id no plan exists (`None`) and nothing is ever dispatched again.
/// `settle` is a pure rule over fresh reads; `dispatch` drives the ordinary UI once.
pub(super) fn run<P>(
    env: Env<'_>,
    request: Request<'_>,
    plan: PlanFn<'_, P>,
    settle: SettleFn<'_, P>,
    dispatch: DispatchFn<'_, P>,
) -> Result<Value> {
    let Request {
        id,
        token,
        seconds,
        action,
    } = request;
    let Env {
        process,
        keys,
        guard,
        authority,
        actions,
    } = env;
    let expected = guard.observe_context(process)?;
    let epoch: Epoch = guard.epoch.clone();
    let lease_end = authority.authorize(token, &epoch)?;
    let fence = action_fence::Fence {
        process,
        epoch: &epoch,
        expected: &expected,
        authority,
        token,
        until: (Instant::now() + Duration::from_secs(seconds)).min(lease_end),
        stop: None,
    };
    let check_input = || fence.input_check();
    let bounded = keys.guarded(&fence, &check_input)?;
    let fenced: &dyn Process = &fence;
    let retry = actions.receipt_exists(id);
    let planned = if retry { None } else { Some(plan(fenced)?) };
    if !retry {
        bounded.preflight()?;
    }
    let info = RefCell::new(Value::Null);
    let mut settle_fn = || settle(fenced, planned.as_ref());
    let mut dispatch_fn = || {
        authority.authorize(token, &epoch)?;
        let Some(planned) = &planned else {
            return Err(Error::new(
                "action_uncertain",
                "A retried request is never dispatched again",
            ));
        };
        *info.borrow_mut() = dispatch(fenced, &bounded, planned)?;
        Ok(())
    };
    let receipt = actions.execute_with(id, &epoch, action, &mut settle_fn, &mut dispatch_fn, true)?;
    Ok(json!({"receipt":receipt,"result":*info.borrow(),"validated":false}))
}

// ---- baselines for idempotent retries ---------------------------------------------------------------
fn baselines() -> &'static Mutex<HashMap<String, Value>> {
    static MAP: OnceLock<Mutex<HashMap<String, Value>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}
/// Keep the pre-action facts of request `id` in this process, so that a retry of an unresolved
/// request can still be settled against them (the journal stores the intent, not the baseline).
/// After a restart the baseline is gone and a retry settles only by what the arguments imply.
pub(super) fn remember<T: Serialize>(id: &str, plan: &T) {
    if let (Ok(value), Ok(mut map)) = (serde_json::to_value(plan), baselines().lock()) {
        if map.len() > 256 {
            map.clear();
        }
        map.insert(id.to_string(), value);
    }
}
pub(super) fn recall<T: DeserializeOwned>(id: &str) -> Option<T> {
    let map = baselines().lock().ok()?;
    serde_json::from_value(map.get(id)?.clone()).ok()
}

// ---- argument helpers ---------------------------------------------------------------------------
pub(super) fn text_arg(args: &serde_json::Map<String, Value>, key: &str, max: usize) -> Result<String> {
    let v = args
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|v| !v.is_empty() && v.len() <= max && !v.chars().any(char::is_control))
        .ok_or_else(|| Error::new("invalid_arguments", format!("{key} must be a non-empty string of at most {max} characters")))?;
    Ok(v.to_string())
}
pub(super) fn opt_text_arg(args: &serde_json::Map<String, Value>, key: &str, max: usize) -> Result<Option<String>> {
    if args.contains_key(key) {
        text_arg(args, key, max).map(Some)
    } else {
        Ok(None)
    }
}
pub(super) fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

// ---- window discovery ---------------------------------------------------------------------------
fn plausible(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}
/// The interface manager object.
pub(super) fn manager(r: &Reader<'_>, image: u64) -> Result<u64> {
    nui::libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(idler + MANAGER)
}
/// A view of the interface manager by its class vtable (image offset): the first manager slot
/// whose object starts with that vtable.
pub(super) fn find_view(r: &Reader<'_>, image: u64, vtable: u64) -> Result<Option<u64>> {
    let manager = manager(r, image)?;
    let slots = r.read(manager, 0x400)?;
    for chunk in slots.chunks_exact(8) {
        let p = u64::from_le_bytes(chunk.try_into().unwrap());
        if plausible(p) && r.u64(p).is_ok_and(|first| first == image + vtable) {
            return Ok(Some(p));
        }
    }
    Ok(None)
}
/// A container window with GUI name `wanted` held in a pointer field of `object` (a view): the
/// field is found by scanning, so no field offset has to be known beforehand.
pub(super) fn find_window(r: &Reader<'_>, image: u64, object: u64, wanted: &str) -> Result<Option<u64>> {
    nui::libcxx(r)?;
    const SPAN: u64 = 0x8000;
    const CHUNK: u64 = 0x1000;
    let mut offset = CONTAINER_BASE;
    while offset < SPAN {
        // An unmapped tail simply ends the scan of that chunk.
        if let Ok(bytes) = r.read(object + offset, CHUNK as usize) {
            for field in bytes.chunks_exact(8) {
                let p = u64::from_le_bytes(field.try_into().unwrap());
                if !plausible(p) {
                    continue;
                }
                let Ok(Some(window)) = gui_tree::container(r, p, image) else {
                    continue;
                };
                if matches!(gui_tree::name(r, window + CONTAINER_BASE, image), Ok(Some(n)) if n == wanted) {
                    return Ok(Some(window));
                }
            }
        }
        offset += CHUNK;
    }
    Ok(None)
}
/// First widget (GUI-object base) named `wanted` below container object `root`, depth bounded.
pub(super) fn find_deep(r: &Reader<'_>, image: u64, root: u64, wanted: &str, depth: u32) -> Result<Option<u64>> {
    for child in gui_tree::children(r, root)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(wanted) {
            return Ok(Some(child));
        }
        if depth > 0 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                if let Some(found) = find_deep(r, image, inner, wanted, depth - 1)? {
                    return Ok(Some(found));
                }
            }
        }
    }
    Ok(None)
}
/// Rectangle of the named widget below `root` when it is shown and on screen.
pub(super) fn rect_deep(r: &Reader<'_>, image: u64, root: u64, wanted: &str) -> Result<(Rect, i32)> {
    let widget = find_deep(r, image, root, wanted, 6)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{wanted} not found")))?;
    if !gui_tree::shown(r, widget)? {
        return Err(Error::new("gui_target_unavailable", format!("{wanted} is not shown")));
    }
    Ok((gui_tree::rect(r, image, widget, GAME)?, gui_tree::button_state(r, widget).unwrap_or(0)))
}
/// A top bar widget by name (the top bar view is interface manager slot +0x188, root +0x68, the
/// same route `ui.rs` uses for the player flag): rectangle when it is shown and on screen.
pub(super) fn topbar_button(r: &Reader<'_>, image: u64, name: &str) -> Result<Rect> {
    let topbar = r.u64(manager(r, image)? + 0x188)?;
    let root = gui_tree::container(r, r.u64(topbar + 0x68)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Top bar root unavailable"))?;
    let widget = find_deep(r, image, root, name, 6)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("Top bar button {name} not found")))?;
    if !gui_tree::shown(r, widget)? {
        return Err(Error::new("gui_target_unavailable", format!("Top bar button {name} is not shown")));
    }
    gui_tree::rect(r, image, widget, GAME)
}
/// Long text of an instant text box (descriptions exceed the 256 characters `text_at` accepts).
pub(super) fn long_text(r: &Reader<'_>, widget: u64) -> Result<String> {
    Ok(super::special_projects::plain(&r.string_bounded(widget + 0x100, 2000)?))
}
pub(super) fn click(keys: &MacKeys<'_>, rect: Rect) -> Result<()> {
    let (x, y) = rect.center();
    keys.click(GAME, x, y).map(|_| ())
}

// ---- pop-ups ------------------------------------------------------------------------------------
/// Confirmation dialogs with `accept_button` and `decline_button` that a fleet or faction action may
/// raise: `CAcceptCommandDialog`, `CConfirmCancelNavyActivityDialog`, `CDefaultConfirmationPopUpWindow`,
/// `CGenericDefaultConfirmationPopUpWindow`, `CDismantleFactionDialog` (vtables from the RTTI map).
pub(super) const CONFIRM_VTABLES: [u64; 5] = [0x342f1d0, 0x342f518, 0x3472150, 0x338d640, 0x3453cd0];
/// Diplomacy proposal pop-ups: base, standard, scripted, incoming lend-lease, foreign manpower.
pub(super) const DIPLOMACY_VTABLES: [u64; 5] = [0x344dfa8, 0x344e0d0, 0x344e1f8, 0x344e320, 0x344e448];

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Popup {
    /// Class vtable as an image offset.
    pub kind: u64,
    /// Root container object.
    pub root: u64,
    /// Position in the pop-up list (stable while nothing opens or closes).
    pub index: usize,
}
/// Shown pop-ups whose class is in `kinds`, in list order.
pub(super) fn popups(r: &Reader<'_>, image: u64, kinds: &[u64]) -> Result<Vec<Popup>> {
    nui::libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    let (_, data, count) = r.array_header(gui + POPUP_LIST)?;
    let mut out = Vec::new();
    for index in 0..count.min(32) {
        let window = r.u64(data + index as u64 * 8)?;
        let kind = r.u64(window)?.wrapping_sub(image);
        if !kinds.contains(&kind) {
            continue;
        }
        let Some(root) = gui_tree::container(r, r.u64(window + POPUP_ROOT)?, image)? else {
            continue;
        };
        if gui_tree::visible(r, root)? {
            out.push(Popup { kind, root, index });
        }
    }
    Ok(out)
}
/// Accept (or decline) the game's confirmation of the command just given, if one appears within
/// a short bound. Returns whether a dialog was answered. Only the classes in `CONFIRM_VTABLES`
/// are ever answered, so a diplomacy proposal can never be accepted by accident.
pub(super) fn answer_confirmation(process: &dyn Process, keys: &MacKeys<'_>, accept: bool) -> Result<bool> {
    let button = if accept { "accept_button" } else { "decline_button" };
    let find = |r: &Reader<'_>, i: u64| -> Result<Option<Rect>> {
        for p in popups(r, i, &CONFIRM_VTABLES)? {
            if let Some(w) = gui_tree::child(r, i, p.root, button)? {
                if gui_tree::shown(r, w)? {
                    return Ok(Some(gui_tree::rect(r, i, w, GAME)?));
                }
            }
        }
        Ok(None)
    };
    let deadline = Instant::now() + Duration::from_millis(1500);
    loop {
        if let Some(rect) = read(process, find)? {
            click(keys, rect)?;
            wait(process, "confirmation closed", |r, i| Ok(find(r, i)?.is_none()))?;
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[cfg(test)]
mod tests {
    use super::super::testmem::{Mem, IMAGE};
    use super::*;

    const CONTAINER_VT: u64 = 0x34ac0b0;
    /// A container object named `name` (GUI type name at base +0x50 -> type +0x28).
    fn window(mem: &Mem, name: &str) -> u64 {
        let object = mem.alloc(0x100);
        mem.u64(object, IMAGE + CONTAINER_VT);
        let kind = mem.alloc(0x100);
        mem.u64(kind, IMAGE + 0x100);
        mem.string(kind + 0x28, name);
        mem.u64(object + 0x30 + 0x50, kind);
        object
    }
    #[test]
    fn finds_a_window_by_name_inside_a_view() {
        let mem = Mem::new();
        let view = mem.alloc(0x3000);
        let a = window(&mem, "new_faction_window");
        let b = window(&mem, "faction_setup_window");
        mem.u64(view + 0x400, a);
        mem.u64(view + 0x1208, b);
        // A pointer to something that is not a container must be skipped.
        let junk = mem.alloc(0x40);
        mem.u64(view + 0x80, junk);
        let r = Reader::new(&mem);
        assert_eq!(find_window(&r, IMAGE, view, "faction_setup_window").unwrap(), Some(b));
        assert_eq!(find_window(&r, IMAGE, view, "new_faction_window").unwrap(), Some(a));
        assert_eq!(find_window(&r, IMAGE, view, "nope").unwrap(), None);
    }
    #[test]
    fn plausible_pointers_are_aligned_and_in_user_space() {
        assert!(plausible(0x7ff0_0000_0010));
        assert!(!plausible(0x7ff0_0000_0011));
        assert!(!plausible(0));
        assert!(!plausible(0xffff_ffff_ffff_fff8));
    }
    #[test]
    fn text_arguments_are_bounded_and_trimmed() {
        let args = json!({"a":"  Hochseeflotte ","b":"","c":"x\u{7}y"});
        let args = args.as_object().unwrap();
        assert_eq!(text_arg(args, "a", 20).unwrap(), "Hochseeflotte");
        assert_eq!(text_arg(args, "b", 20).unwrap_err().code, "invalid_arguments");
        assert_eq!(text_arg(args, "c", 20).unwrap_err().code, "invalid_arguments");
        assert_eq!(text_arg(args, "a", 5).unwrap_err().code, "invalid_arguments");
        assert_eq!(text_arg(args, "zz", 5).unwrap_err().code, "invalid_arguments");
        assert_eq!(opt_text_arg(args, "zz", 5).unwrap(), None);
        assert!(same_name(" a ", "A"));
    }
}
