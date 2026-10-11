"""When does the focus in progress finish? The game shows no modal window at completion, and the telemetry cannot
see national focuses, but a focus runs for the days its detail window showed when it was started. `start()` records
that after every successful hoi4_focus select (and `cancel` clears it); the fast clock asks `due()` for each day
record and reports the stop condition `focus_done` on the day it is due. The record is kept in
artifacts/state/focus_timer.json so a restarted server still knows.
"""

import json
import os


def _path(session):
    return os.path.join(session.cfg.artifacts_dir, "state", "focus_timer.json")


def _now_n(session):
    d = session.telemetry.day()
    return d.get("n") if d else None


def start(session, focus_id, name, days):
    """Called after a focus was started. days = the duration the game showed."""
    n = _now_n(session)
    rec = {"focus": focus_id, "name": name, "days": days, "start_n": n,
           "due_n": (n + int(days)) if n is not None and days else None, "fired": False}
    session.focus_timer = rec
    _write(session, rec)
    return rec


def clear(session):
    session.focus_timer = None
    _write(session, None)


def _write(session, rec):
    try:
        p = _path(session)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8") as f:
            json.dump(rec, f)
    except OSError:
        pass


def get(session):
    rec = getattr(session, "focus_timer", None)
    if rec is None and not getattr(session, "_focus_timer_loaded", False):
        session._focus_timer_loaded = True
        try:
            with open(_path(session), encoding="utf-8") as f:
                rec = json.load(f)
            session.focus_timer = rec
        except (OSError, ValueError):
            rec = None
    return rec


def rearm(session, days):
    """The standing order found the focus still running: look again in `days` days."""
    rec = get(session)
    if rec and rec.get("due_n") is not None:
        n = _now_n(session)
        rec["due_n"] = (n if n is not None else rec["due_n"]) + days
        rec["fired"] = False
        _write(session, rec)


def due(session, n):
    """The focus record when the running focus is due on day counter n (once), else None."""
    rec = get(session)
    if not rec or rec.get("fired") or rec.get("due_n") is None:
        return None
    if n >= rec["due_n"]:
        rec["fired"] = True
        _write(session, rec)
        return rec
    return None
