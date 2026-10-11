"""Saves: list, write (console `save` in test mode, the Save Game dialog in campaign mode), and load through the
game's own Load Game dialog (from the pause menu or the main menu), verified by the in-game date afterwards.

Only saves named with the configured prefix (hoi4live_) are ever written; other saves are only read or loaded.
"""

import os
import re
import time

from ..errors import ToolError
from ..perception import popups as popups_mod
from ..perception import topbar
from ..state.savefile import SaveFile
from . import console, menu, ui


def saves_dir(session):
    return session.cfg.saves_dir


def save_path(session, name):
    base = name[:-5] if name.lower().endswith(".hoi4") else name
    return os.path.join(saves_dir(session), base + ".hoi4")


def list_saves(session, pattern=None, limit=50):
    d = saves_dir(session)
    rx = re.compile(pattern, re.I) if pattern else None
    rows = []
    for f in os.listdir(d):
        if not f.lower().endswith(".hoi4"):
            continue
        if rx and not rx.search(f):
            continue
        st = os.stat(os.path.join(d, f))
        rows.append({"name": f[:-5], "size_mb": round(st.st_size / 1048576, 1), "mtime": st.st_mtime,
                     "modified": time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(st.st_mtime))})
    rows.sort(key=lambda r: -r["mtime"])
    return rows[:limit]


def prefixed(session, name):
    """Apply the hoi4live_ prefix; refuse names that would leave the save folder."""
    if not re.match(r"^[A-Za-z0-9_\-]+$", name):
        raise ToolError("save names may only use letters, digits, '_' and '-'", {"field": "name"})
    prefix = session.cfg.save_prefix
    return name if name.startswith(prefix) else prefix + name


def wait_written(path, t0, timeout=180, session=None, stable=2.5):
    """Wait until the file exists, is newer than t0 and its size is stable."""
    last = None
    stable_since = None
    deadline = time.time() + timeout
    while time.time() < deadline:
        if session:
            session.check_cancel()
            session.report(f"waiting for {os.path.basename(path)}")
        try:
            st = os.stat(path)
            if st.st_mtime >= t0 - 1:
                sig = (st.st_size, st.st_mtime)
                if sig == last:
                    stable_since = stable_since or time.time()
                    if time.time() - stable_since >= stable and st.st_size > 0:
                        return st
                else:
                    stable_since = None
                last = sig
        except OSError:
            pass
        time.sleep(0.5)
    raise ToolError(f"the save {os.path.basename(path)} was not written within {timeout} s")


def save(session, name, overwrite=True):
    name = prefixed(session, name)
    path = save_path(session, name)
    if os.path.exists(path) and not overwrite:
        raise ToolError(f"{name}.hoi4 exists and overwrite is false")
    t0 = time.time()
    if session.mode == "test":
        console.run(session, [f"save {name}"], read=False)
        method = "console"
    else:
        menu.save_via_dialog(session, name)
        method = "dialog"
    st = wait_written(path, t0, session.cfg["timeouts"]["save_seconds"], session)
    out = {"name": name, "path": path, "size_mb": round(st.st_size / 1048576, 1), "method": method}
    try:
        with SaveFile(path, use_cache=False) as sf:
            h = sf.header()
        out.update({"player": h.get("player"), "date": h.get("date")})
    except ToolError as e:
        out["header_error"] = str(e)
    return out


def load(session, name, timeout=None):
    """Load a save through the Load Game dialog and verify the date matches the save header."""
    path = save_path(session, name)
    if not os.path.exists(path):
        raise ToolError(f"save not found: {path}")
    base = os.path.basename(path)[:-5]
    want = None
    try:
        with SaveFile(path, use_cache=False) as sf:
            hdr = sf.header()
        want = hdr.get("date")
    except ToolError:
        hdr = {}
    timeout = timeout or session.cfg["timeouts"]["load_seconds"]
    steps = []
    session.park_cursor()                      # a tooltip over the screen hides what it is
    session.sleep(0.5)
    from . import panels
    panels.dismiss_notices(session)
    state, img = ui.classify(session)
    steps.append({"screen": state["screen"]})
    scr = state["screen"]
    if scr in ("map", "esc_menu"):
        if state.get("console_open"):
            console.close_console(session)
        if popups_mod.detect(img):
            steps.append({"note": "popups open; the pause menu opens over them"})
        menu.open_esc_menu(session)
        ui.click_text(session, r"load\s*game")
        session.sleep(2.0)
    elif scr in ("main_menu", "single_player_menu"):
        menu.main_menu_load(session)
    elif scr != "load_screen":
        raise ToolError(f"cannot load from the current screen ({scr})", {"evidence": state.get("evidence")})
    picked = menu.pick_save_in_dialog(session, base, want)
    steps.append({"picked": picked})
    session.logs.mark("load", ("error", "game", "system"))
    t0 = time.time()
    menu.press_load_button(session)
    session.park_cursor()
    session.report("loading save")
    # The old map can stay visible for a moment; then a loading screen hides the date; then the loaded map
    # shows the save's date. game.log stays silent while loading, so only the screen tells.
    deadline = t0 + timeout
    got = None
    saw_loading = False
    while time.time() < deadline:
        session.check_cancel()
        img = session.grab()
        d = topbar.read_date(img, session.ocr)
        if "error" in d:
            saw_loading = True
        else:
            matches = want is None or _same_date(want, d)
            if (saw_loading and time.time() - t0 > 3) or (matches and time.time() - t0 > 20):
                got = d
                break
        session.report(f"loading ({int(time.time() - t0)} s)")
        session.sleep(2)
    if got is None:
        raise ToolError("the game did not return to the map after loading", {"steps": steps})
    steps.append({"saw_loading_screen": saw_loading})
    session.window.close_notepad_logs()
    result = {"loaded": base, "date_on_screen": got["hoi4"], "save_date": want, "player": hdr.get("player"),
              "seconds": round(time.time() - t0, 1), "steps": steps}
    if want:
        result["date_matches"] = _same_date(want, got)
    return result


def _same_date(save_date, screen_date):
    """Save header date '1936.1.1.12' vs the OCR'd map date, within one day (the clock may tick before the
    screenshot, and HOI4 shows midnight as 24:00 of the previous day)."""
    import datetime
    try:
        y, m, d = [int(x) for x in save_date.split(".")[:3]]
        h = int(save_date.split(".")[3]) if save_date.count(".") >= 3 else 0
    except ValueError:
        return None
    saved = datetime.datetime(y, m, d) + datetime.timedelta(hours=h)
    shown = topbar.to_datetime(screen_date)
    return abs((shown - saved).total_seconds()) <= 24 * 3600
