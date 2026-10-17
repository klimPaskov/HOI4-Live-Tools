"""Menus: the in-game pause menu, the Load/Save dialogs, the main menu and quitting. OCR finds every button by
its label, so the code does not depend on resolution or on where a mod moves the buttons."""

import re
import time

from ..errors import ToolError
from ..perception import screen as screen_mod
from . import ui

ESC_RX = re.compile(r"resume|save\s*game|load\s*game|game\s*options|playthrough|exit\s*to\s*menu|exit\s*game|"
                    r"^\W*close\W*$|settings|quit|surrender", re.I)


def esc_menu_open(session, img=None):
    img, res = ui.ocr_screen(session, img, region=_center(session, img), scale=1.5)
    hits = [l.text for l in res.lines if ESC_RX.search(l.text)]
    return len(hits) >= 3, hits


def _center(session, img=None):
    w, h = session.window.size() if img is None else (img.shape[1], img.shape[0])
    return [int(w * 0.3), int(h * 0.15), int(w * 0.4), int(h * 0.7)]


def open_esc_menu(session, max_presses=6):
    """Press Esc until the pause menu shows (each press first closes an open window)."""
    for _ in range(max_presses):
        ok, hits = esc_menu_open(session)
        if ok:
            return hits
        session.input.key("esc")
        session.sleep(0.9)
    ok, hits = esc_menu_open(session)
    if not ok:
        raise ToolError("the pause menu did not open", {"seen": hits})
    return hits


def close_esc_menu(session):
    ok, _ = esc_menu_open(session)
    if ok:
        session.input.key("esc")
        session.sleep(0.8)
    ok, hits = esc_menu_open(session)
    if ok:
        raise ToolError("the pause menu did not close", {"seen": hits})


def clear_windows(session, max_presses=8):
    """Close every open window: Esc until the pause menu appears, then Esc once more to close it."""
    open_esc_menu(session, max_presses)
    close_esc_menu(session)


def main_menu_load(session):
    """Main menu -> Single Player -> Load (button labels found by OCR)."""
    img, lines, res = ui.find_text(session, r"^\W*load(\s*game)?\W*$")
    if not lines:
        ui.click_text(session, r"single\s*player")
        session.sleep(2.5)
    ui.click_text(session, r"^\W*load(\s*game)?\W*$", retries=4)
    session.sleep(3.0)


SAVE_DATE_RX = re.compile(r"\d{1,2}:\d{2},?\s*\d{1,2}\s+[A-Za-z]{3,}")     # the year may wrap to the next line


MONTHS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
          "November", "December"]


def _date_rx(save_date):
    """The save list's date line for a header date '1936.1.1.12': '12:00, 1 January, 1936' (year may wrap)."""
    try:
        y, m, d = [int(x) for x in str(save_date).split(".")[:3]]
        h = int(str(save_date).split(".")[3]) if str(save_date).count(".") >= 3 else 0
    except (ValueError, IndexError):
        return None
    return re.compile(rf"\b{h}:00,?\s*{d}\s+{MONTHS[m - 1]}", re.I)


def _row_filename(session, x, y):
    """The file name in the tooltip of a save row ("Saved game filename: qa_show_cn.hoi4")."""
    from ..perception import tooltip
    d, _, _ = tooltip.read(session, x, y, wait=0.8)
    text = " ".join(d.get("lines") or [])
    m = re.search(r"([A-Za-z0-9_\-. ]+)\.ho[il1]4", text)
    return m.group(1).strip() if m else None, text


def pick_save_in_dialog(session, base, save_date=None, max_scrolls=60):
    """Select the row of save `base` in the save list, scrolling when needed. Returns the clicked row.

    Windows OCR drops many save names (thin lowercase with underscores) but reads the dates well, so rows are
    found by the save's date (from its header) and confirmed by the row's tooltip, which names the file; an OCR'd
    name that matches exactly is clicked directly. The in-game dialog keeps its scroll position and may list
    hundreds of saves, so its scrollbar knob is dragged to the top first. Clicks go to the name, never to the
    row's right side (delete button)."""
    fold = str.maketrans({"o": "0", "i": "1", "l": "1", "|": "1"})
    norm = lambda t: re.sub(r"[^a-z0-9]", "", (t or "").lower()).translate(fold)
    target = norm(base)
    want = _date_rx(save_date) if save_date else None
    seen_pages = []
    checked = set()
    for i in range(max_scrolls + 1):
        session.check_cancel()
        session.park_cursor()                              # a row's tooltip covers the names under it
        session.sleep(0.5)
        img = session.grab()
        res = session.ocr.recognize(img, scale=1.5)
        local = next((l for l in res.lines if re.fullmatch(r"\W*local\W*", l.text, re.I)), None)
        back = next((l for l in res.lines if re.fullmatch(r"\W*back\W*", l.text, re.I)), None)
        if local and back and back.box[1] > local.box[1] + 100:
            # the in-game dialog: read the list between its Local/Cloud tabs and its Back/Load buttons, closer
            x0 = max(0, local.box[0] - 60)
            y0 = local.box[1] + local.box[3] + 8
            res = session.ocr.recognize(img, region=[x0, y0, 330, back.box[1] - y0 - 8], scale=2.5)
        dates = [l for l in res.lines if SAVE_DATE_RX.search(l.text)]
        if i == 0 and local and back:
            bx = local.box[0] + 262
            t0, t1 = local.box[1] + 45, back.box[1] - 35
            col = img[t0:t1, bx - 6:bx + 7].max(2).mean(1)
            if col.size and float(col.max()) > 80 and int(col.argmax()) > 6:
                session.input.drag(bx, t0 + int(col.argmax()), bx, t0 - 10, seconds=0.5)
                session.park_cursor()
                session.sleep(0.6)
                continue
        if not dates:
            if i >= 5:
                raise ToolError("no save list found on screen", {"seen": [l.text for l in res.lines][:30]})
            session.sleep(1.0)
            continue
        # fast path: a name read exactly
        for l in res.lines:
            if l in dates:
                continue
            t = norm(l.text)
            below = [d for d in dates if 0 < d.box[1] - l.box[1] < 40 and abs(d.box[0] - l.box[0]) < 40]
            if below and (t == target or (t.startswith(target) and len(t) - len(target) <= 2)):
                session.input.click(*l.center)
                session.park_cursor()
                session.sleep(0.8)
                return {"text": l.text, "box": l.box, "scrolls": i, "by": "name"}
        # rows with the save's date: confirm by the tooltip's file name
        for d in dates:
            if want and not want.search(d.text):
                continue
            key = (i, d.box[1])
            if key in checked:
                continue
            checked.add(key)
            x, y = d.box[0] + 40, d.box[1] - 14
            fname, _ = _row_filename(session, x, y)
            if fname and norm(fname) == target:
                session.input.click(x, y)
                session.park_cursor()
                session.sleep(0.8)
                return {"text": fname, "box": [x - 40, y - 8, 160, 16], "scrolls": i, "by": "date and tooltip"}
        page = tuple(norm(d.text) for d in dates)
        if seen_pages and page == seen_pages[-1]:
            break                                           # the list stopped moving: end reached
        seen_pages.append(page)
        x = min(d.box[0] for d in dates) + 20
        y = int(sum(d.center[1] for d in dates) / len(dates))
        session.input.scroll(x, y, -3)
        session.sleep(0.5)
    raise ToolError(f"save '{base}' not found in the save list",
                    {"last_page": list(seen_pages[-1]) if seen_pages else []})


def press_load_button(session):
    """Confirm the picked save. On the front-end load screen the save becomes the scenario and START begins it
    (the Load button there only toggles the list); the in-game dialog confirms with its Load button."""
    img, lines, res = ui.find_text(session, r"start", exact=True)
    if lines:
        return ui.click_text(session, r"start", exact=True, prefer="bottom", retries=2)
    return ui.click_text(session, r"load", exact=True, prefer="bottom", retries=3)


def save_via_dialog(session, name):
    """Pause menu -> Save Game -> the filename field (under "Saved game filename:") -> type the name -> Save
    (campaign mode, no console). Verified live on 1.19.2: the field must be clicked before typing."""
    def find_label():
        # the dialog sits in the centre; a crop reads its small label more reliably than the whole busy screen
        w, h = session.window.size()
        region = [int(w * 0.2), int(h * 0.1), int(w * 0.6), int(h * 0.8)]
        for scale in (1.5, 2.0):
            img, lines, _ = ui.find_text(session, r"saved\s*game\s*file\s*name", region=region, scale=scale)
            if lines:
                return lines
        return []

    lines = find_label()
    if not lines:
        from . import panels
        panels.dismiss_notices(session)         # a "National Focus Completed" window ignores Esc
        open_esc_menu(session)
        try:
            ui.click_text(session, r"save\s*game")
        except ToolError:
            # OCR missed the label of the open pause menu (seen live, twice): the button sits at a fixed place
            w, h = session.window.size()
            session.input.click(w // 2, int(h * 0.35))
        session.sleep(1.5)
        for _ in range(4):                      # a busy game takes a few seconds to fill the save list
            lines = find_label()
            if lines:
                break
            session.sleep(1.0)
    if not lines:
        raise ToolError("the Save Game dialog did not open (no 'Saved game filename' label)")
    label = lines[0]
    fx, fy = label.center[0], label.box[1] + label.box[3] + 22
    session.input.click(fx, fy)
    session.sleep(0.3)
    session.input.key("end")
    session.input.key("backspace", times=60, delay=0.008)
    session.input.type_text(name)
    session.sleep(0.4)
    # the Save button sits under the field, right of Back
    region = [label.box[0] - 160, fy + 12, label.box[2] + 320, 60]
    ui.click_text(session, r"save", exact=True, prefer="bottom", retries=3, region=region, scale=2.0)
    session.sleep(1.0)
    _confirm_if_asked(session, [])            # "overwrite?" when the name exists
    close_esc_menu_if_open(session)


def close_esc_menu_if_open(session):
    ok, _ = esc_menu_open(session)
    if ok:
        session.input.key("esc")
        session.sleep(0.8)


CONFIRM_RX = r"^\W*(yes|[o0]k|confirm|exit|quit|exit\s*game|exit\s*to\s*menu)\W*$"


def _confirm_if_asked(session, before_texts):
    """Click the confirmation button of a dialog that appeared after an exit button, if any."""
    session.sleep(1.2)
    img, res = ui.ocr_screen(session, scale=1.5)
    new = [l for l in res.lines if l.text not in before_texts and re.search(CONFIRM_RX, l.text, re.I)]
    if not new:
        # short button labels ("OK") are missed at full-screen scale: read the centre of the screen closer
        h, w = img.shape[:2]
        region = [int(w * 0.25), int(h * 0.2), int(w * 0.5), int(h * 0.6)]
        img, res = ui.ocr_screen(session, img, region=region, scale=2.5)
        new = [l for l in res.lines if re.search(r"^\W*([o0]k|yes|confirm)\W*$", l.text, re.I)]
    if new:
        x, y = new[-1].center
        session.input.click(x, y)
        session.sleep(1.0)
        return new[-1].text
    return None


def quit_to_main_menu(session, timeout=240):
    """Pause menu -> Exit to Menu -> OK (Resign Game) -> the statistics screen's Quit -> main menu.
    Never touches the pause menu's Wiki button (it opens a browser)."""
    hits = open_esc_menu(session)
    ui.click_text(session, r"exit\s*to\s*menu")
    confirmed = _confirm_if_asked(session, hits)
    t0 = time.time()
    clicked_quit = False
    while time.time() - t0 < timeout:
        session.check_cancel()
        ok, texts = main_menu_visible(session)
        if ok:
            return {"confirmed": confirmed, "statistics_quit": clicked_quit, "seconds": round(time.time() - t0)}
        if not clicked_quit:
            img, lines, res = ui.find_text(session, r"quit", exact=True, scale=1.5)
            if lines:
                x, y = sorted(lines, key=lambda l: -l.box[1])[0].center
                session.input.click(x, y)
                clicked_quit = True
        session.sleep(3)
    raise ToolError("the main menu did not appear after exiting the game")


def _click_ok(session):
    """Click the OK / Yes button of a modal dialog in the centre of the screen. Returns its label or None."""
    img = session.grab()
    h, w = img.shape[:2]
    region = [int(w * 0.25), int(h * 0.2), int(w * 0.5), int(h * 0.6)]
    img, res = ui.ocr_screen(session, img, region=region, scale=2.5)
    hit = [l for l in res.lines if re.search(r"^\W*([o0]k|yes|confirm)\W*$", l.text, re.I)]
    if not hit:
        return None
    session.input.click(*hit[-1].center)
    session.sleep(1.0)
    return hit[-1].text


def quit_to_desktop(session):
    """Pause menu -> Exit Game (-> confirm), the game's own exit flow. An exit confirmation that is already open
    (a previous attempt) is confirmed directly: that dialog ignores Esc."""
    state, _ = ui.classify(session)
    if state["screen"] == "main_menu":
        clicked = ui.click_text(session, r"quit", exact=True)
        return {"exit_requested": True, "from": "main_menu", "clicked": clicked}
    img, res = ui.ocr_screen(session, region=_center(session), scale=1.5)
    if any(re.search(r"sure you want to exit", l.text, re.I) for l in res.lines):
        return {"confirmed": _click_ok(session), "dialog_was_open": True}
    hits = open_esc_menu(session)
    ui.click_text(session, r"exit\s*game")
    confirmed = _confirm_if_asked(session, hits)
    return {"confirmed": confirmed}


def main_menu_visible(session, img=None):
    img, res = ui.ocr_screen(session, img, scale=1.0)
    texts = [l.text for l in res.lines]
    return any(re.search(r"single\s*player", t, re.I) for t in texts), texts


def _wait_text(session, pattern, timeout=60, exact=False, scale=1.5):
    t0 = time.time()
    while time.time() - t0 < timeout:
        session.check_cancel()
        img, lines, res = ui.find_text(session, pattern, exact=exact, scale=scale)
        if lines:
            return lines
        session.sleep(1.5)
    raise ToolError(f"'{pattern}' did not appear within {timeout} s")


def review_setup_rules(session, reset=False):
    """Open only the ready screen's rules dialog; optionally use its normal Reset.

    Keep the dialog evidence and return to ready. A button click is not a
    complete structured readback of every rule, so report that limit explicitly.
    """
    _, ready, _ = ui.find_text(session, r"start", exact=True)
    _, back, _ = ui.find_text(session, r"back\s*to\s*menu")
    if not ready or not back:
        raise ToolError("game rules inspection requires the pre-game ready screen")
    steps = [ui.click_text(session, r"custom\s*game\s*rules", exact=True)]
    _wait_text(session, r"load\s*preset", 15)
    if reset:
        steps.append(ui.click_text(session, r"reset", exact=True))
        session.sleep(0.5)
    img, res = ui.ocr_screen(session, scale=1.5)
    evidence = session.artifact(img, "setup_rules")
    steps.append(ui.click_text(session, r"apply", exact=True))
    _wait_text(session, r"start", 15, exact=True)
    return {"steps": steps, "reset_requested": reset, "screen": evidence,
            "visible_text": [line.text for line in res.lines],
            "all_rule_values_verified": False}


def new_game(session, country=None, bookmark=None, timeout=600, start=True):
    """Main menu -> Single Player -> New Game -> Select Scenario (optional bookmark by its label, e.g. '1939')
    -> Select Country (a major by its name; the default selection otherwise) -> START -> map.

    start=False returns at the ready screen for rules review before starting.
    Minor countries have no name on that screen (flags only); start as a major and switch with the console
    `tag` command in test mode, or pick on the map in a later phase."""
    from ..perception import topbar
    steps = []

    def stage():
        state, img = ui.classify(session)
        texts = state.get("evidence", []) + [""]
        img, res = ui.ocr_screen(session, img, scale=1.5)
        all_text = [l.text for l in res.lines]
        has = lambda rx: any(re.search(rx, t, re.I) for t in all_text)
        if state["screen"] in ("main_menu", "single_player_menu"):
            return state["screen"]
        if has(r"select\s*country") and has(r"other\s*countries|not\s*played"):
            return "select_country"
        if has(r"select\s*scenario") and has(r"brief\s*history"):
            return "select_scenario"
        if has(r"^\W*start\W*$") and has(r"back\s*to\s*menu"):
            return "ready"
        return state["screen"]

    st = stage()
    steps.append({"stage": st})
    if st == "main_menu":
        steps.append(ui.click_text(session, r"single\s*player"))
        session.sleep(2.5)
        st = "single_player_menu"
    if st == "single_player_menu":
        steps.append(ui.click_text(session, r"new\s*game"))
        _wait_text(session, r"select\s*scenario", 90)
        st = "select_scenario"
    if st == "select_scenario":
        if bookmark:
            steps.append(ui.click_text(session, bookmark, retries=3))
            session.sleep(1.0)
        steps.append(ui.click_text(session, r"select\s*scenario", prefer="bottom"))
        _wait_text(session, r"other\s*countries|not\s*played", 120)
        st = "select_country"
    if st == "select_country":
        if country:
            rx = re.escape(country) if not any(c in country for c in "\^$[]()|*+?") else country
            steps.append(ui.click_text(session, rx, retries=3))
            session.sleep(1.0)
        steps.append(ui.click_text(session, r"select\s*country", prefer="bottom"))
        _wait_text(session, r"start", 120, exact=True)
        st = "ready"
    if st != "ready":
        raise ToolError(f"new_game starts from the main menu or a pre-game screen (now: {st})")
    if not start:
        return {"steps": steps, "prepared": True, "started": False, "stage": "ready"}
    from . import gamemode
    gamemode.begin_run(session)
    steps.append(ui.click_text(session, r"start", exact=True, prefer="bottom"))
    t0 = time.time()
    while time.time() - t0 < timeout:
        session.check_cancel()
        d = topbar.read_date(session.grab(), session.ocr)
        if "error" not in d:
            return {"steps": steps, "date": d["hoi4"], "seconds": round(time.time() - t0)}
        session.report(f"starting the game ({int(time.time() - t0)} s)")
        session.sleep(3)
    raise ToolError("the new game did not reach the map", {"steps": steps})
