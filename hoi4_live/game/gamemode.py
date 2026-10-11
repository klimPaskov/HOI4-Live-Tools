"""Game mode from a save associated with the current owned run.

An Ironman save is binary (`HOI4bin`), a plain text save starts `HOI4txt`; the text header lists `player_countries`
(more than one entry or a named user means multiplayer), `difficulty`, `mods` and the `ironman=` flag of the
game rules block. Ironman and multiplayer change what the tools may do: no console, no manual saves, a shared
clock. `hoi4_status` reports the mode, and `check()` is what tools that need the console or a manual save call to
refuse with an explanation. The signals come from save files only: an Ironman launch has not been verified live.
"""

import copy
import math
import os
import re
import time

from ..errors import ToolError

HEAD_BYTES = 400_000


def read_header(path):
    """{ironman, multiplayer, difficulty, mods, players, binary, version, date, player} from a save's first bytes."""
    with open(path, "rb") as f:
        data = f.read(HEAD_BYTES)
    out = {"file": os.path.basename(path), "binary": data[:7] == b"HOI4bin"}
    if out["binary"]:
        out.update({"ironman": True, "multiplayer": None, "difficulty": None, "mods": None, "players": None})
        return out
    text = data.decode("utf-8", "replace")
    m = re.search(r'^difficulty="([^"]*)"', text, re.M)
    out["difficulty"] = m.group(1) if m else None
    m = re.search(r'^version="([^"]*)"', text, re.M)
    out["version"] = m.group(1) if m else None
    m = re.search(r'^date="([^"]*)"', text, re.M)
    out["date"] = m.group(1) if m else None
    m = re.search(r'^player="([^"]*)"', text, re.M)
    out["player"] = m.group(1) if m else None
    block = re.search(r"player_countries=\{(.*?)\n\}", text, re.S)
    users = re.findall(r'user="([^"]*)"', block.group(1)) if block else []
    out["players"] = len(users) or None
    out["multiplayer"] = len(users) > 1
    m = re.search(r"mods=\{(.*?)\}", text, re.S)
    out["mods"] = re.findall(r'"([^"]+)"', m.group(1)) if m else []
    m = re.search(r"^\s*ironman=(\d)", text, re.M)
    out["ironman"] = bool(m and m.group(1) == "1")
    return out


def newest_save(session):
    d = session.cfg.saves_dir
    try:
        files = [os.path.join(d, f) for f in os.listdir(d) if f.lower().endswith(".hoi4")]
    except OSError:
        return None
    candidates = []
    for path in files:
        try:
            candidates.append((os.path.getmtime(path), path))
        except OSError:
            pass
    return max(candidates)[1] if candidates else None


def begin_run(session, started_wall=None):
    """Forget unrelated save metadata at owned launch or before a new campaign."""
    session._game_mode_started_wall = time.time() if started_wall is None else started_wall
    session._game_mode_save = None
    session._mode_cache = None


def _signature(path):
    st = os.stat(path)
    return (os.path.realpath(path), st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns)


def bind_save(session, path):
    """Called ONLY after a verified ordinary save/load, never for a guessed newest save."""
    signature = _signature(path)
    session._game_mode_save = (signature, time.time())
    session._mode_cache = None


def current(session):
    """Only verified loaded/saved files or saves newer than this owned run qualify.

    A recent save from another campaign is not the current game at the menu.
    No run anchor means unknown; file timestamps alone cannot establish a run.
    """
    now = time.time()
    started = getattr(session, "_game_mode_started_wall", None)
    bound = getattr(session, "_game_mode_save", None)
    p = None
    source = None
    if isinstance(bound, tuple) and len(bound) == 2:
        signature, observed = bound
        try:
            if (0 <= now - observed <= FRESH_SECONDS and _signature(signature[0]) == signature):
                p, source = signature[0], "verified_save_or_load"
        except (OSError, TypeError, ValueError, IndexError):
            pass
    if p is None:
        if (type(started) not in (int, float) or not math.isfinite(started) or not 0 < started <= now):
            return {"known": False, "reason": "no_current_run_save_evidence"}
        p = newest_save(session)
        try:
            written = os.path.getmtime(p) if p else None
        except OSError:
            written = None
        if written is None or written < started or not 0 <= now - written <= FRESH_SECONDS:
            return {"known": False, "reason": "no_fresh_save_from_current_run"}
        source = "save_written_during_current_run"
    try:
        key = (_signature(p), started, source)
    except OSError:
        return {"known": False, "reason": "save_unavailable"}
    cache = getattr(session, "_mode_cache", None)
    if isinstance(cache, tuple) and cache[0] == key:
        return copy.deepcopy(cache[1])
    try:
        h = read_header(p)
        if _signature(p) != key[0]:
            return {"known": False, "reason": "save_changed_while_reading"}
        h.update(known=True, source=source)
    except OSError:
        h = {"known": False}
    session._mode_cache = (key, h)
    return copy.deepcopy(h)


FRESH_SECONDS = 6 * 3600


def check(session, what):
    """Refuse incompatible actions only on fresh, current-run save evidence.

    A verified load can bind an older file; an unrelated recent save cannot.
    Unknown save metadata does not invent an Ironman or multiplayer claim.
    """
    m = current(session)
    if m.get("ironman"):
        raise ToolError(f"{what} is not possible in an Ironman game (no console, no manual saves)",
                        {"game_mode": m, "error": "ironman"})
    if m.get("multiplayer"):
        raise ToolError(f"{what} is not possible in a multiplayer game", {"game_mode": m, "error": "multiplayer"})
    return m
