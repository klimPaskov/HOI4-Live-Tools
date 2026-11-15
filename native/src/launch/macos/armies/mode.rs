//! The execution mode of an army's battle plan: careful, balanced (the game's default) or rush.
//!
//! The battle plan bar that shows while an army is selected (interface/battleplantools.gui,
//! `battleplantools_window`) has the button `btn_priority` in its `content`; the window
//! `planning_speed_window` beside it holds the three choices `btn_execute_0` (careful),
//! `btn_execute_1` (balanced) and `btn_execute_2` (rush). The chosen button shows another frame
//! of its sprite than the other two (the .gui file gives the balanced button frame 2 and the
//! others frame 1).
//!
//! One call: the army's card selects the army, the bar slides in, the window is opened with a
//! click on `btn_priority` when it is not shown, the wanted button is clicked and the three
//! frames are read back: exactly the wanted button must show the frame the others do not. No
//! memory field of the mode is known, so the frames are the only readback, and they can be read
//! only while the army is selected. Built from the game's files, not yet seen live: every
//! failure names what was found (bar, window, rectangles, frames before and after, the words
//! of the widgets that changed with the click), so one live call says what to correct.
use super::super::{
    invasion::BATTLE_PLAN_TOOLS,
    journal::Outcome,
    kit,
    ui::{clear_notices, read},
};
use super::{clear_selection, select_army_quick, GAME};
use crate::{
    error::{Error, Result},
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{game_input::GameInput, Process},
    xlayout::{fld, Class},
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// The modes in the order of their buttons (`btn_execute_0..2`).
pub(in super::super) const MODES: [&str; 3] = ["careful", "balanced", "rush"];
const BUTTONS: [&str; 3] = ["btn_execute_0", "btn_execute_1", "btn_execute_2"];
const BAR: &str = "battleplantools_window";
const OPENER: &str = "btn_priority";
const WINDOW: &str = "planning_speed_window";
/// The small icon on the bar that shows the mode (three frames).
const ICON: &str = "icon_execute_type";
/// The frame a sprite shows (the field the air mission icons are read from).
const SPRITE: Class = Class::new("CSprite", 0x36cc528);
const SPRITE_FRAME: u64 = 0x34;
/// The bar slides in over 300 ms and the window over 200 ms: how long a stable picture is
/// waited for, and how long the frames may take to follow the click.
const SLIDE_MS: u64 = 2000;
const FOLLOW_MS: u64 = 1500;
/// Bytes of a widget and of its sprite compared before and after the click (failure detail).
const WIDGET_SPAN: usize = 0x120;
const SPRITE_SPAN: usize = 0xc0;
/// Changed words listed per widget.
const WORDS_LISTED: usize = 10;

/// The index of a mode name.
pub(in super::super) fn index(mode: &str) -> Option<usize> {
    MODES.iter().position(|m| *m == mode)
}

/// What the live GUI shows of the mode controls. Every part is optional: a part that is
/// missing or unreadable is reported, never an error.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Seen {
    /// the battle plan bar: found, and shown
    bar: Option<bool>,
    /// `btn_priority` when shown and on screen, and its sprite frame
    opener: Option<Rect>,
    opener_frame: Option<i32>,
    /// the window of the three buttons: found, and shown
    window: Option<bool>,
    rects: [Option<Rect>; 3],
    frames: [Option<i32>; 3],
    states: [Option<i32>; 3],
    icon_frame: Option<i32>,
}
impl Seen {
    /// The window is shown with all three buttons on screen.
    fn pickable(&self) -> bool {
        self.window == Some(true) && self.rects.iter().all(Option::is_some)
    }
    fn json(&self) -> Value {
        let shown = |v: Option<bool>| match v {
            None => "not found",
            Some(true) => "shown",
            Some(false) => "hidden",
        };
        let rect = |r: &Option<Rect>| r.map(|r| json!([r.x, r.y, r.w, r.h]));
        let buttons: Vec<Value> = (0..3)
            .map(|n| {
                json!({"name": BUTTONS[n], "rect": rect(&self.rects[n]),
                    "frame": self.frames[n], "state": self.states[n]})
            })
            .collect();
        json!({BAR: shown(self.bar),
            OPENER: {"rect": rect(&self.opener), "frame": self.opener_frame},
            WINDOW: shown(self.window), "buttons": buttons, ICON: self.icon_frame})
    }
}

/// The frame a widget's sprite shows.
fn frame(r: &Reader<'_>, image: u64, widget: u64) -> Result<i32> {
    let sprite = gui_tree::sprite(r, image, widget)?;
    r.i32(sprite + fld(r.string_abi(), &SPRITE, SPRITE_FRAME)?)
}
/// A widget's rectangle when it is shown and on screen.
fn shown_rect(r: &Reader<'_>, image: u64, widget: u64) -> Option<Rect> {
    if !gui_tree::shown(r, widget).ok()? {
        return None;
    }
    gui_tree::rect(r, image, widget, GAME).ok()
}
/// The battle plan bar's window (container object), if the view holds one.
fn bar(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    match kit::find_view(r, image, BATTLE_PLAN_TOOLS)? {
        Some(view) => kit::find_window(r, image, view, BAR),
        None => Ok(None),
    }
}
/// The three mode buttons below the bar (GUI-object bases), where found.
fn buttons(r: &Reader<'_>, image: u64, bar: u64) -> Result<[Option<u64>; 3]> {
    let Some(window) = kit::find_deep(r, image, bar, WINDOW, 6)? else {
        return Ok([None; 3]);
    };
    let Some(object) = gui_tree::container(r, window, image)? else {
        return Ok([None; 3]);
    };
    let mut out = [None; 3];
    for (n, name) in BUTTONS.iter().enumerate() {
        out[n] = gui_tree::child(r, image, object, name)?;
    }
    Ok(out)
}
/// The mode controls below the bar's window `bar` (None: the bar was not found).
fn seen_in(r: &Reader<'_>, image: u64, bar: Option<u64>) -> Result<Seen> {
    let mut seen = Seen::default();
    let Some(bar) = bar else {
        return Ok(seen);
    };
    let bar_shown = gui_tree::visible(r, bar)?;
    seen.bar = Some(bar_shown);
    if let Some(opener) = kit::find_deep(r, image, bar, OPENER, 6)? {
        seen.opener = shown_rect(r, image, opener).filter(|_| bar_shown);
        seen.opener_frame = frame(r, image, opener).ok();
    }
    if let Some(icon) = kit::find_deep(r, image, bar, ICON, 6)? {
        seen.icon_frame = frame(r, image, icon).ok();
    }
    let Some(window) = kit::find_deep(r, image, bar, WINDOW, 6)? else {
        return Ok(seen);
    };
    let window_shown = bar_shown && gui_tree::shown(r, window)?;
    seen.window = Some(window_shown);
    for (n, button) in buttons(r, image, bar)?.into_iter().enumerate() {
        let Some(button) = button else { continue };
        seen.rects[n] = shown_rect(r, image, button).filter(|_| window_shown);
        seen.frames[n] = frame(r, image, button).ok();
        seen.states[n] = gui_tree::button_state(r, button).ok();
    }
    Ok(seen)
}

/// The button the frames mark as chosen: the one whose frame differs from the frame the
/// other two share. None when a frame is unread or the three do not single one out.
pub(super) fn selected(frames: &[Option<i32>; 3]) -> Option<usize> {
    let [a, b, c] = [frames[0]?, frames[1]?, frames[2]?];
    match (a == b, b == c, a == c) {
        (false, true, false) => Some(0),
        (false, false, true) => Some(1),
        (true, false, false) => Some(2),
        _ => None,
    }
}

/// The 32-bit words that differ between two reads of the same bytes: "+0x34: 0x0 -> 0x1".
fn words_changed(before: &[u8], after: &[u8]) -> Vec<String> {
    before
        .chunks_exact(4)
        .zip(after.chunks_exact(4))
        .enumerate()
        .filter(|(_, (b, a))| b != a)
        .take(WORDS_LISTED)
        .map(|(n, (b, a))| {
            format!(
                "+{:#x}: {:#x} -> {:#x}",
                n * 4,
                u32::from_le_bytes(b.try_into().unwrap()),
                u32::from_le_bytes(a.try_into().unwrap())
            )
        })
        .collect()
}
/// The bytes of the mode widgets below the bar's window and of their sprites, by label.
fn snapshot(r: &Reader<'_>, image: u64, bar: Option<u64>) -> Result<Vec<(String, Vec<u8>)>> {
    let Some(bar) = bar else {
        return Ok(Vec::new());
    };
    let mut widgets: Vec<(&str, Option<u64>)> = BUTTONS
        .iter()
        .copied()
        .zip(buttons(r, image, bar)?)
        .collect();
    widgets.push((OPENER, kit::find_deep(r, image, bar, OPENER, 6)?));
    widgets.push((ICON, kit::find_deep(r, image, bar, ICON, 6)?));
    let mut out = Vec::new();
    for (name, widget) in widgets {
        let Some(widget) = widget else { continue };
        if let Ok(bytes) = r.read(widget, WIDGET_SPAN) {
            out.push((name.to_string(), bytes));
        }
        if let Some(bytes) = gui_tree::sprite(r, image, widget)
            .ok()
            .and_then(|sprite| r.read(sprite, SPRITE_SPAN).ok())
        {
            out.push((format!("{name}.sprite"), bytes));
        }
    }
    Ok(out)
}
/// What changed in the widgets between two snapshots: label -> changed words.
fn changes(before: &[(String, Vec<u8>)], after: &[(String, Vec<u8>)]) -> Value {
    let mut out = serde_json::Map::new();
    for (name, old) in before {
        let Some((_, new)) = after.iter().find(|(n, _)| n == name) else {
            continue;
        };
        let words = words_changed(old, new);
        if !words.is_empty() {
            out.insert(name.clone(), json!(words));
        }
    }
    Value::Object(out)
}

/// One attempt to set a mode: what was seen and sent, and how it ended.
pub(in super::super) struct Attempt {
    /// The receipt detail: {army, mode, frames_before, frames_after, ..}.
    pub detail: Value,
    /// A click went to the bar (the opening button or a mode button).
    pub input: bool,
    /// The button the frames marked before the first click and after the last read.
    pub before: Option<usize>,
    pub after: Option<usize>,
    /// The wanted button is the one marked.
    pub verified: bool,
    /// Why not, with everything found.
    pub error: Option<Error>,
}
impl Attempt {
    /// What the journal records. Only this attempt's own reads say what happened: verified
    /// when the frames mark the wanted button; not applied when no click went to the bar or
    /// the frames mark the same button as before; else a result that differs from the target.
    pub(in super::super) fn outcome(&self) -> Outcome {
        let detail = self.detail.clone();
        if self.verified {
            Outcome::Reached(detail)
        } else if !self.input || (self.after.is_some() && self.after == self.before) {
            Outcome::Unchanged(detail)
        } else {
            Outcome::Diverged(detail)
        }
    }
}
/// What was seen and sent while a mode was set.
#[derive(Default)]
struct Trail {
    /// before the opening click, when one was needed
    closed: Option<Seen>,
    /// with the window open, before the mode click
    before: Option<Seen>,
    after: Option<Seen>,
    opened: Option<(f64, f64)>,
    clicked: Option<(f64, f64)>,
    changed: Option<Value>,
}
impl Trail {
    fn frames(seen: &Option<Seen>) -> Value {
        json!(seen.as_ref().map(|s| s.frames))
    }
    fn input(&self) -> bool {
        self.opened.is_some() || self.clicked.is_some()
    }
    /// The receipt detail.
    fn detail(&self, army: &str, wanted: usize) -> Value {
        let first = self.before.as_ref().or(self.closed.as_ref());
        let last = self.after.as_ref().or(first);
        json!({"army": army, "mode": MODES[wanted],
            "frames_before": Self::frames(&first.cloned()),
            "frames_after": Self::frames(&last.cloned()),
            "icon_frame_before": first.and_then(|s| s.icon_frame),
            "icon_frame_after": last.and_then(|s| s.icon_frame),
            "window_opened_by_click": self.opened.is_some(),
            "mode_button_clicked": self.clicked.is_some()})
    }
    /// Everything found, for the failure message.
    fn found(&self) -> Value {
        let point = |p: &Option<(f64, f64)>| p.map(|(x, y)| json!([x.round(), y.round()]));
        json!({"before_opening": self.closed.as_ref().map(Seen::json),
            "before": self.before.as_ref().map(Seen::json),
            "after": self.after.as_ref().map(Seen::json),
            "clicked": {OPENER: point(&self.opened), "mode_button": point(&self.clicked)},
            "changed_words": self.changed})
    }
}
/// The typed failure: what was wanted, why it is not verified, and everything found.
fn failure(code: &'static str, army: &str, wanted: usize, why: &str, trail: &Trail) -> Error {
    Error::new(
        code,
        format!(
            "Execution mode {} of {army} is not verified: {why}. Found: {}",
            MODES[wanted],
            trail.found()
        ),
    )
}

/// The mode controls once `ready` holds in two reads 60 ms apart (a window that slides in
/// reports moving rectangles), or the last picture after `ms` (false).
fn stable(
    process: &dyn Process,
    bar: Option<u64>,
    ms: u64,
    ready: impl Fn(&Seen) -> bool,
) -> Result<(Seen, bool)> {
    let deadline = Instant::now() + Duration::from_millis(ms);
    let mut last: Option<Seen> = None;
    loop {
        let now = read(process, |r, i| seen_in(r, i, bar))?;
        if ready(&now) && last.as_ref() == Some(&now) {
            return Ok((now, true));
        }
        if Instant::now() >= deadline {
            return Ok((now, false));
        }
        last = Some(now);
        std::thread::sleep(Duration::from_millis(60));
    }
}

/// Set the execution mode `wanted` (index into `MODES`) of `army`'s battle plans. The selection
/// is cleared afterwards, which also hides the bar and its window.
pub(in super::super) fn set(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    wanted: usize,
) -> Attempt {
    let mut trail = Trail::default();
    let error = drive(process, keys, army, wanted, &mut trail).err();
    let _ = clear_selection(process, keys);
    let marked = |seen: &Option<Seen>| seen.as_ref().and_then(|s| selected(&s.frames));
    Attempt {
        detail: trail.detail(army, wanted),
        input: trail.input(),
        before: marked(&trail.before).or_else(|| marked(&trail.closed)),
        after: marked(&trail.after),
        verified: error.is_none(),
        error,
    }
}
fn drive(
    process: &dyn Process,
    keys: &dyn GameInput,
    army: &str,
    wanted: usize,
    trail: &mut Trail,
) -> Result<()> {
    clear_notices(process, keys)?;
    select_army_quick(process, keys, army)?;
    // The bar's window is found once (its view is searched for it); it slides in after the
    // selection.
    let bar = read(process, bar)?;
    let (mut now, _) = stable(process, bar, SLIDE_MS, |s| {
        s.pickable() || s.opener.is_some()
    })?;
    if !now.pickable() {
        trail.closed = Some(now.clone());
        let Some(opener) = now.opener else {
            let why = match now.bar {
                None => "the battle plan bar (battleplantools_window) was not found",
                Some(false) => "the battle plan bar is not shown though the army is selected",
                Some(true) => "the bar's execution button (btn_priority) is not shown on screen",
            };
            return Err(failure("gui_target_unavailable", army, wanted, why, trail));
        };
        let at = opener.center();
        keys.click(GAME, at.0, at.1)?;
        trail.opened = Some(at);
        let (opened, ok) = stable(process, bar, SLIDE_MS, Seen::pickable)?;
        if !ok {
            trail.after = Some(opened);
            return Err(failure(
                "gui_target_unavailable",
                army,
                wanted,
                "a click on btn_priority did not show the window planning_speed_window with its \
                 three buttons",
                trail,
            ));
        }
        now = opened;
    }
    trail.before = Some(now.clone());
    if selected(&now.frames) == Some(wanted) {
        trail.after = Some(now);
        return Ok(());
    }
    let Some(button) = now.rects[wanted] else {
        return Err(failure(
            "gui_target_unavailable",
            army,
            wanted,
            "the mode's button is not on screen",
            trail,
        ));
    };
    let memory_before = read(process, |r, i| snapshot(r, i, bar)).unwrap_or_default();
    let at = button.center();
    keys.click(GAME, at.0, at.1)?;
    trail.clicked = Some(at);
    let deadline = Instant::now() + Duration::from_millis(FOLLOW_MS);
    loop {
        let after = read(process, |r, i| seen_in(r, i, bar))?;
        let done = selected(&after.frames) == Some(wanted);
        trail.after = Some(after);
        if done {
            return Ok(());
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    let memory_after = read(process, |r, i| snapshot(r, i, bar)).unwrap_or_default();
    trail.changed = Some(changes(&memory_before, &memory_after));
    let after = trail.after.as_ref().map(|s| s.frames).unwrap_or_default();
    let why = if after.iter().any(Option::is_none) {
        "the frames of the three buttons could not be read after the click"
    } else if Some(after) == trail.before.as_ref().map(|s| s.frames) {
        "the frames of the three buttons did not change with the click"
    } else {
        "after the click the frames do not mark the wanted button alone"
    };
    Err(failure("input_readback_failed", army, wanted, why, trail))
}

#[cfg(test)]
mod tests {
    use super::super::super::panelkit::fixture::{Tree, IMAGE};
    use super::*;

    /// The bar as the game lays it out: content { btn_priority, icon_execute_type,
    /// planning_speed_window { btn_execute_0..2 } }. Returns the tree, the bar's window and the
    /// window of the three buttons.
    fn world(bar_shown: bool, window_shown: bool) -> (Tree, u64, u64) {
        let mut t = Tree::new();
        let bar = t.container(BAR, bar_shown);
        let content = t.container("content", true);
        let opener = t.button(OPENER, 694, 1000, 30, 30);
        let icon = t.button(ICON, 965, 1013, 12, 12);
        let window = t.container(WINDOW, window_shown);
        let buttons: Vec<u64> = (0..3)
            .map(|n| t.button(BUTTONS[n as usize], 655 + n * 40, 920, 32, 32))
            .collect();
        t.set_children(window, &buttons);
        t.set_children(content, &[opener, icon, window + 0x30]);
        t.set_children(bar, &[content + 0x30]);
        (t, bar, window)
    }
    /// Give the named widget's sprite a frame.
    fn set_frame(t: &Tree, bar: u64, name: &str, value: i32) {
        let r = Reader::new(&t.mem);
        let widget = kit::find_deep(&r, IMAGE, bar, name, 6).unwrap().unwrap();
        let sprite = gui_tree::sprite(&r, IMAGE, widget).unwrap();
        t.mem.i(sprite + SPRITE_FRAME, value);
    }
    fn frames(t: &Tree, bar: u64, values: [i32; 3]) {
        for (n, v) in values.into_iter().enumerate() {
            set_frame(t, bar, BUTTONS[n], v);
        }
    }

    #[test]
    fn mode_names_follow_the_buttons() {
        assert_eq!(index("careful"), Some(0));
        assert_eq!(index("balanced"), Some(1));
        assert_eq!(index("rush"), Some(2));
        assert_eq!(index("Rush"), None);
        assert_eq!(index(""), None);
    }

    #[test]
    fn the_button_with_the_frame_the_others_do_not_show_is_the_chosen_one() {
        let f = |a, b, c| [Some(a), Some(b), Some(c)];
        // the .gui default: balanced shows frame 2
        assert_eq!(selected(&f(1, 2, 1)), Some(1));
        assert_eq!(selected(&f(2, 1, 1)), Some(0));
        assert_eq!(selected(&f(1, 1, 2)), Some(2));
        // frames counted from 0 in memory mark the same button
        assert_eq!(selected(&f(0, 0, 1)), Some(2));
        // nothing singled out, or a frame unread
        assert_eq!(selected(&f(1, 1, 1)), None);
        assert_eq!(selected(&f(1, 2, 3)), None);
        assert_eq!(selected(&[Some(1), None, Some(1)]), None);
    }

    #[test]
    fn the_open_window_is_read_with_its_rectangles_and_frames() {
        let (t, bar, _) = world(true, true);
        frames(&t, bar, [1, 2, 1]);
        set_frame(&t, bar, ICON, 2);
        let r = Reader::new(&t.mem);
        let seen = seen_in(&r, IMAGE, Some(bar)).unwrap();
        assert!(seen.pickable());
        assert_eq!(seen.bar, Some(true));
        assert_eq!(seen.window, Some(true));
        assert_eq!(
            seen.opener,
            Some(Rect {
                x: 694,
                y: 1000,
                w: 30,
                h: 30
            })
        );
        assert_eq!(
            seen.rects[2],
            Some(Rect {
                x: 735,
                y: 920,
                w: 32,
                h: 32
            })
        );
        assert_eq!(seen.frames, [Some(1), Some(2), Some(1)]);
        assert_eq!(seen.icon_frame, Some(2));
        assert_eq!(selected(&seen.frames), Some(1));
        // the game marks rush: the frames follow
        frames(&t, bar, [1, 1, 2]);
        let seen = seen_in(&r, IMAGE, Some(bar)).unwrap();
        assert_eq!(selected(&seen.frames), Some(2));
        let shown = seen.json();
        assert_eq!(shown[BAR], "shown");
        assert_eq!(shown[WINDOW], "shown");
        assert_eq!(shown["buttons"][2]["name"], "btn_execute_2");
        assert_eq!(shown["buttons"][2]["rect"], json!([735, 920, 32, 32]));
        assert_eq!(shown["buttons"][2]["frame"], 2);
        assert_eq!(shown[OPENER]["rect"], json!([694, 1000, 30, 30]));
    }

    #[test]
    fn a_closed_window_offers_no_button_but_the_opening_one() {
        let (t, bar, _) = world(true, false);
        frames(&t, bar, [1, 2, 1]);
        let r = Reader::new(&t.mem);
        let seen = seen_in(&r, IMAGE, Some(bar)).unwrap();
        assert!(!seen.pickable());
        assert_eq!(seen.window, Some(false));
        assert_eq!(seen.rects, [None; 3]);
        assert!(seen.opener.is_some());
        // the frames of a hidden window are still read (they are reported, not trusted)
        assert_eq!(seen.frames, [Some(1), Some(2), Some(1)]);
        assert_eq!(seen.json()[WINDOW], "hidden");
        // a hidden bar (no army selected) offers nothing at all
        let (t, bar, _) = world(false, true);
        let r = Reader::new(&t.mem);
        let seen = seen_in(&r, IMAGE, Some(bar)).unwrap();
        assert_eq!(seen.bar, Some(false));
        assert!(seen.opener.is_none() && !seen.pickable());
        // and a bar that was not found says so
        let none = seen_in(&r, IMAGE, None).unwrap();
        assert_eq!(none, Seen::default());
        assert_eq!(none.json()[BAR], "not found");
        assert_eq!(none.json()[WINDOW], "not found");
    }

    #[test]
    fn an_attempt_settles_by_what_its_own_reads_show() {
        let attempt = |verified, input, before, after| Attempt {
            detail: json!({"mode": "rush"}),
            input,
            before,
            after,
            verified,
            error: None,
        };
        let kind = |a: Attempt| match a.outcome() {
            Outcome::Reached(_) => "verified",
            Outcome::Unchanged(_) => "not applied",
            Outcome::Diverged(_) => "diverged",
            _ => "other",
        };
        assert_eq!(kind(attempt(true, true, Some(1), Some(2))), "verified");
        // already chosen: nothing was clicked
        assert_eq!(kind(attempt(true, false, Some(2), Some(2))), "verified");
        // refused before any click on the bar
        assert_eq!(kind(attempt(false, false, None, None)), "not applied");
        // clicked, the frames still mark the old button
        assert_eq!(kind(attempt(false, true, Some(1), Some(1))), "not applied");
        // clicked, and the frames mark another button or none: not the target, not unchanged
        assert_eq!(kind(attempt(false, true, Some(1), Some(0))), "diverged");
        assert_eq!(kind(attempt(false, true, Some(1), None)), "diverged");
        assert_eq!(kind(attempt(false, true, None, None)), "diverged");
    }

    #[test]
    fn changed_words_are_listed_by_offset() {
        let before = [0u8; 16];
        let mut after = before;
        after[4] = 2;
        after[12] = 0xff;
        assert_eq!(
            words_changed(&before, &after),
            ["+0x4: 0x0 -> 0x2", "+0xc: 0x0 -> 0xff"]
        );
        assert!(words_changed(&before, &before).is_empty());
        let a = vec![("btn_execute_2.sprite".to_string(), before.to_vec())];
        let b = vec![("btn_execute_2.sprite".to_string(), after.to_vec())];
        assert_eq!(
            changes(&a, &b),
            json!({"btn_execute_2.sprite": ["+0x4: 0x0 -> 0x2", "+0xc: 0x0 -> 0xff"]})
        );
        assert_eq!(changes(&a, &a), json!({}));
    }

    #[test]
    fn a_failure_carries_everything_that_was_found() {
        let (t, bar, _) = world(true, true);
        frames(&t, bar, [1, 2, 1]);
        let r = Reader::new(&t.mem);
        let seen = seen_in(&r, IMAGE, Some(bar)).unwrap();
        let trail = Trail {
            before: Some(seen.clone()),
            after: Some(seen),
            clicked: Some((751.0, 936.0)),
            changed: Some(json!({"btn_execute_2": ["+0x9c: 0x0 -> 0x1"]})),
            ..Default::default()
        };
        let e = failure(
            "input_readback_failed",
            "Gondar Group",
            2,
            "the frames of the three buttons did not change with the click",
            &trail,
        );
        assert_eq!(e.code, "input_readback_failed");
        assert!(
            e.message.starts_with(
                "Execution mode rush of Gondar Group is not verified: the frames of the three \
                 buttons did not change with the click. Found: {"
            ),
            "{}",
            e.message
        );
        let found: Value =
            serde_json::from_str(e.message.split_once("Found: ").unwrap().1).unwrap();
        assert_eq!(found["before"][WINDOW], "shown");
        assert_eq!(found["after"]["buttons"][1]["frame"], 2);
        assert_eq!(found["clicked"]["mode_button"], json!([751.0, 936.0]));
        assert_eq!(
            found["changed_words"]["btn_execute_2"][0],
            "+0x9c: 0x0 -> 0x1"
        );
        // the receipt detail of the same attempt
        let detail = trail.detail("Gondar Group", 2);
        assert_eq!(
            detail,
            json!({"army": "Gondar Group", "mode": "rush",
                "frames_before": [1, 2, 1], "frames_after": [1, 2, 1],
                "icon_frame_before": 0, "icon_frame_after": 0,
                "window_opened_by_click": false, "mode_button_clicked": true})
        );
    }
}
