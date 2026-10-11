"""Helpers shared by the campaign action tools (phase 3): the player's tag and deep state from the newest save,
localised names, screenshots for results and failures, and the verified-action result shape.

Every campaign action returns {"action", "target", "before", "after", "changed": bool, "verified_by", ...} plus a
screenshot of the screen after the action; a failure raises ToolError with the screenshot attached.
"""

import time

from ..desktop import images
from ..errors import ToolError
from ..gamedata import loc as loc_mod
from ..state import assemble
from .common import attach
from .registry import ToolResult

SAVE_ARG = {"type": "string", "default": "telemetry",
            "description": "where completed focuses, researched technologies and similar history come from: "
                           "'telemetry' (the telemetry mod's technology scan where a tool has one, else the newest "
                           "save), 'latest' (newest save of this country), 'none', or a save name"}


def names(session, keys):
    return loc_mod.for_session(session).get_many([k for k in keys if k])


def name_of(session, key, default=None):
    return names(session, [key]).get(key, default if default is not None else key)


def player_tag(session, tag=None):
    """The player's tag: the telemetry's, unless its last daily line is from a later date than the screen (a save
    was loaded since and no daily tick ran: the lines belong to the timeline that was left, possibly another
    country's), then the newest save dated no later than the screen."""
    if tag:
        return tag
    now = None
    t = session.telemetry.player()
    if t:
        snap = session.telemetry.snapshot(t)
        try:
            from ..game import timectl
            d, _ = timectl.read_date(session)
            now = d.get("hoi4") if "error" not in d else None
        except Exception:
            now = None
        gap = assemble.days_between(now, (snap or {}).get("date")) if now and snap else None
        if gap is None or gap <= 1.05:
            return t
    try:
        path, _ = assemble.find_save(session, None, "latest", now)
        from ..state.savefile import SaveFile
        with SaveFile(path, use_cache=False) as sf:
            return sf.header().get("player")
    except ToolError:
        return t or None


def save_model(session, tag, sections, save="latest"):
    """(model sections from the save or {}, source note). Never raises: missing saves give {}."""
    if save == "none":
        return {}, {"save": "none"}
    if save == "telemetry":
        save = "latest"
    try:
        now = None
        snap = session.telemetry.snapshot(tag) if tag else None
        if snap:
            now = snap.get("date")
        path, note = assemble.find_save(session, tag, save, now)
        sv = assemble.save_state(session, path, tag, sorted(set(sections) | {"overview"}), 60)
        if "error" in sv:
            return {}, {"save": sv.get("_file"), "error": sv["error"]}
        src = {"save": sv.get("_file"), "as_of": sv.get("save_date")}
        if note:
            src["note"] = note
        return sv, src
    except ToolError as e:
        return {}, {"save": None, "error": str(e)}


def game_year(session, sv=None):
    snap = session.telemetry.snapshot()
    d = (snap or {}).get("date") or (sv or {}).get("save_date")
    try:
        return int(str(d).split(".")[0])
    except (TypeError, ValueError):
        return None


def finish(session, summary, data, name, img=None, max_width=1280):
    """ToolResult with a screenshot of the current screen (or img)."""
    res = ToolResult(summary, data)
    shot = session.grab() if img is None else img
    return attach(res, session, shot, name, max_width=max_width)


def fail(session, message, details=None, name="failed"):
    img = session.grab()
    path = session.artifact(img, name)
    png, _, _ = images.encode(img, 960)
    d = dict(details or {})
    d["screenshot"] = path
    raise ToolError(message, d, image=png)


def wait(seconds):
    time.sleep(seconds)
