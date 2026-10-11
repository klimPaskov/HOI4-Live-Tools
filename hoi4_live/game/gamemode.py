"""Ironman, multiplayer, difficulty and mods of the running game, read from the header of the newest save.

An Ironman save is binary (`HOI4bin`), a plain text save starts `HOI4txt`; the text header lists `player_countries`
(more than one entry or a named user means multiplayer), `difficulty`, `mods` and the `ironman=` flag of the
game rules block. Ironman and multiplayer change what the tools may do: no console, no manual saves, a shared
clock. `hoi4_status` reports the mode, and `check()` is what tools that need the console or a manual save call to
refuse with an explanation. The signals come from save files only: an Ironman launch has not been verified live.
"""

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
    return max(files, key=os.path.getmtime) if files else None


def current(session):
    """The mode read from the newest save of the running game; cached per file and modification time."""
    p = newest_save(session)
    if not p:
        return {"known": False}
    key = (p, os.path.getmtime(p))
    cache = getattr(session, "_mode_cache", None)
    if cache and cache[0] == key:
        return cache[1]
    try:
        h = read_header(p)
        h["known"] = True
    except OSError:
        h = {"known": False}
    session._mode_cache = (key, h)
    return h


FRESH_SECONDS = 6 * 3600


def check(session, what):
    """Raise ToolError when the game is Ironman or multiplayer (`what` is not possible there). Only a save written in
    the last six hours counts: an old Ironman save says nothing about the game that runs now."""
    m = current(session)
    if m.get("known") and time.time() - os.path.getmtime(os.path.join(session.cfg.saves_dir, m["file"])) > FRESH_SECONDS:
        return m
    if m.get("ironman"):
        raise ToolError(f"{what} is not possible in an Ironman game (no console, no manual saves)",
                        {"game_mode": m, "error": "ironman"})
    if m.get("multiplayer"):
        raise ToolError(f"{what} is not possible in a multiplayer game", {"game_mode": m, "error": "multiplayer"})
    return m
