"""Game-file facts the sink and slot detectors need, read from the files the running game loaded:
the game folder of the run and the mods enabled in its user folder, with mod overrides and
replace_path. No value is kept in code: with no files a fact is unknown (None or empty) and the
finding that needs it says so or stays silent (owner, 27 Nov 2026: "don't hardcode anything").
A file's cost is the base cost before modifiers (ideas, laws, traits and mods change prices), so
no finding judges affordability from it: that is the game's own check (its alerts, its enabled
state, its refusal).

- experience caps per pool (common/defines NMilitary MAX_ARMY/NAVY/AIR_EXPERIENCE);
- spirit slot costs per branch (common/idea_tags: academy, branch and command slots);
- the mastery a subdoctrine's rewards need in total (common/doctrines/subdoctrines: each reward's
  `mastery`, else NDoctrines DEFAULT_REWARD_MASTERY), and the reserve rules of a completed track
  (MASTERY_BANK_CONVERSION_RATE, MASTERY_BANK_MAX);
- the laws of each law group (common/ideas: mobilization_laws, economy, trade_laws): cost, the
  modifiers and the availability conditions a snapshot can judge (war support, war, surrender).

`facts(game_dir)` is cached per folder. Nothing here plays: the detectors compare these facts with
a snapshot and say what is lost.
"""
from __future__ import annotations

import os
import re

# the facts when no files can be read: every one unknown
UNKNOWN = {
    "xp_cap": {},
    "spirit_costs": {},
    "reward_mastery": None,
    "bank_rate": None,
    "bank_max": None,
}

_TOKEN = re.compile(r'"[^"]*"|[{}]|[<>]=?|=|[^\s{}=<>"#]+|#[^\n]*')


def parse(text):
    """Paradox script as nested lists of (key, op, value); a block value is a list."""
    toks = [t for t in _TOKEN.findall(text) if not t.startswith("#")]
    pos = 0

    def block():
        nonlocal pos
        out = []
        while pos < len(toks):
            t = toks[pos]
            if t == "}":
                pos += 1
                return out
            if pos + 1 < len(toks) and toks[pos + 1] in ("=", "<", ">", "<=", ">="):
                op = toks[pos + 1]
                pos += 2
                if pos < len(toks) and toks[pos] == "{":
                    pos += 1
                    out.append((t, op, block()))
                else:
                    out.append((t, op, toks[pos].strip('"') if pos < len(toks) else None))
                    pos += 1
            elif t == "{":
                pos += 1
                out.append(("", "=", block()))
            else:
                out.append((t.strip('"'), None, None))
                pos += 1
        return out

    return block()


def get(node, key, default=None):
    for k, _, v in node or []:
        if k == key:
            return v
    return default


def _num(v, default=None):
    try:
        return float(v)
    except (TypeError, ValueError):
        return default


def _read(path):
    try:
        with open(path, encoding="utf-8-sig", errors="replace") as f:
            return f.read()
    except OSError:
        return ""


def _define(text, name):
    """The last value of define `name` in `text` (a mod's defines file loads after the game's and
    overrides it)."""
    m = re.findall(rf"\b{name}\s*=\s*(-?[\d.]+)", text)
    return float(m[-1]) if m else None


# (game folder, mods) of the facts being read: set by `facts` for the helpers below
_ROOT = ("", ())


def _files(folder, ext=(".txt",)):
    """The effective files of `folder` (relative to the game folder, e.g. "common/ideas") for the
    game and its enabled mods; a path that is already absolute is listed as it is."""
    if os.path.isabs(folder):
        try:
            names = sorted(os.listdir(folder))
        except OSError:
            return []
        return [os.path.join(folder, n) for n in names if n.endswith(ext)]
    from ..gamedata import scriptdb
    game_dir, mods = _ROOT
    return scriptdb.effective_files(folder, game_dir, list(mods), ext=ext)


def _file(folder, name, ext=(".txt",)):
    return next((p for p in _files(folder, ext) if os.path.basename(p).lower() == name.lower()), "")


def _conditions(node):
    """The availability terms a snapshot can judge: (kind, value) for war support, war and
    surrender; anything else is counted as unreadable."""
    judged, unread = [], 0
    for k, op, v in node or []:
        if k == "has_war_support" and op in (">", ">="):
            judged.append(("war_support_above", _num(v, 0.0)))
        elif k == "has_war" and v in ("yes", "no"):
            judged.append(("has_war", v == "yes"))
        elif k == "surrender_progress" and op in (">", ">="):
            judged.append(("surrender_above", _num(v, 0.0)))
        elif k in ("OR", "if", "IF", "custom_trigger_tooltip", "NOT", "AND"):
            unread += 1  # country- or mod-specific branches: not judged from a snapshot
        elif k:
            unread += 1
    return judged, unread


def _laws():
    groups = {}
    for path in _files("common/ideas"):
        base = os.path.basename(path)
        if not base.startswith("_"):
            continue
        tree = parse(_read(path))
        for k, _, body in tree:
            if k != "ideas" or not isinstance(body, list):
                continue
            for group, _, laws in body:
                if group not in ("mobilization_laws", "economy", "trade_laws") or not isinstance(laws, list):
                    continue
                rows = groups.setdefault(group, [])
                for name, _, law in laws:
                    if not isinstance(law, list) or name in ("law", "use_list_view"):
                        continue
                    mods = {mk: _num(mv) for mk, _, mv in get(law, "modifier", []) or []
                            if _num(mv) is not None}
                    judged, unread = _conditions(get(law, "available", []))
                    if get(law, "allowed") is not None:
                        unread += 1  # a country's own law (allowed = {...}): not judged from a snapshot
                    rows.append({"law": name, "cost": _num(get(law, "cost")),
                                 "country_specific": get(law, "allowed") is not None,
                                 "modifiers": mods, "conditions": judged, "unreadable_conditions": unread})
    return groups


def _subdoctrine_mastery(default_reward):
    out = {}
    for sub in ("land", "air", "sea", "special_forces"):
        for path in _files(f"common/doctrines/subdoctrines/{sub}"):
            for name, _, body in parse(_read(path)):
                if not isinstance(body, list):
                    continue
                rewards = get(body, "rewards", [])
                total = 0.0
                count = 0
                for _, _, r in rewards or []:
                    if isinstance(r, list):
                        count += 1
                        m = _num(get(r, "mastery"), default_reward)
                        total = None if (m is None or total is None) else total + m
                out[name] = {"xp_cost": _num(get(body, "xp_cost")), "rewards": count,
                             "mastery_to_complete": total}
    return out


def _spirit_costs():
    text = parse(_read(_file("common/idea_tags", "00_idea.txt")))
    slots = get(text, "idea_categories", []) or get(text, "slots", [])
    branch = {"army_spirit": "army", "navy_spirit": "navy", "air_spirit": "air"}
    out = {}
    for _, _, body in slots or []:
        if not isinstance(body, list):
            continue
        kind = branch.get(get(body, "type"))
        cost = _num(get(body, "cost"))
        if kind and cost is not None:
            out.setdefault(kind, []).append(cost)
    return {k: sorted(v) for k, v in out.items()}


_CACHE = {}
# the run's own game folder (set by the monitor from the run's launch record): without it the
# law ladder and the slot costs were the fallback's, and every law finding stayed silent
DEFAULT_DIR = ""
# the mods of the run (scriptdb roots: [{path, replace_path}]); set by the monitor with DEFAULT_DIR
DEFAULT_MODS = ()


def mods_of_run(run_dir):
    """The mods the run's game loaded: the enabled mods of the user folder named by its launch
    arguments (-userdir=...), in load order; () when the run does not say."""
    try:
        with open(os.path.join(run_dir, "launch.json"), encoding="utf-8") as f:
            args = __import__("json").load(f).get("arguments") or []
    except (OSError, ValueError, AttributeError):
        return ()
    user = next((a.split("=", 1)[1] for a in args if isinstance(a, str) and a.startswith("-userdir=")), "")
    if not user:
        return ()
    from ..telemetry.modgen import mod_paths
    return tuple(mod_paths(user))


def game_dir_of_run(run_dir):
    """The game folder a run launched (the folder holding `common` above the executable named in
    the run's launch.json or identity.json), or "" when the run does not say."""
    exe = ""
    for name, path in (("launch.json", ("arguments", 0)), ("identity.json", ("game", "exe"))):
        try:
            with open(os.path.join(run_dir, name), encoding="utf-8") as f:
                v = __import__("json").load(f)
            for k in path:
                v = v[k]
            if isinstance(v, str) and v:
                exe = v
                break
        except (OSError, ValueError, KeyError, IndexError, TypeError):
            continue
    d = os.path.dirname(exe)
    for _ in range(6):
        if d and os.path.isdir(os.path.join(d, "common")):
            return d
        parent = os.path.dirname(d)
        if parent == d:
            break
        d = parent
    return ""


def facts(game_dir=None, mods=None):
    """The facts for `game_dir` (env HOI4_GAME_DIR, then the run's own game folder, when None) with
    `mods` (the run's enabled mods when None); every fact unknown without a game folder."""
    game_dir = game_dir or os.environ.get("HOI4_GAME_DIR") or DEFAULT_DIR or ""
    mods = tuple(DEFAULT_MODS if mods is None else mods)
    key = (game_dir, tuple(m.get("path") for m in mods))
    if key in _CACHE:
        return _CACHE[key]
    global _ROOT
    out = {k: (dict(v) if isinstance(v, dict) else v) for k, v in UNKNOWN.items()}
    out["source"] = "no game files"
    out["laws"], out["subdoctrines"] = {}, {}
    common = os.path.join(game_dir, "common") if game_dir else ""
    if common and os.path.isdir(common):
        _ROOT = (game_dir, mods)
        defines = "\n".join(_read(p) for p in _files("common/defines", (".lua",)))
        for pool, name in (("army", "MAX_ARMY_EXPERIENCE"), ("navy", "MAX_NAVY_EXPERIENCE"),
                           ("air", "MAX_AIR_EXPERIENCE")):
            v = _define(defines, name)
            if v:
                out["xp_cap"][pool] = v
        reward = _define(defines, "DEFAULT_REWARD_MASTERY")
        out["reward_mastery"] = reward
        out["bank_rate"] = _define(defines, "MASTERY_BANK_CONVERSION_RATE")
        out["bank_max"] = _define(defines, "MASTERY_BANK_MAX")
        out["spirit_costs"] = _spirit_costs()
        out["laws"] = _laws()
        out["subdoctrines"] = _subdoctrine_mastery(reward)
        out["source"] = game_dir + "".join(f" + {m.get('path')}" for m in mods)
    _CACHE[key] = out
    return out


def law_available(law, politics, at_war):
    """(True/False/None, why) for a law row of `facts()['laws']` against a snapshot's politics block;
    None when a condition the snapshot cannot judge decides it."""
    ws = politics.get("war_support")
    for kind, value in law.get("conditions") or []:
        if kind == "war_support_above":
            if ws is None:
                return None, "war support not read"
            if ws <= value:
                return False, f"war support {ws:.0%} not above {value:.0%}"
        elif kind == "has_war" and value != bool(at_war):
            return False, "needs a war" if value else "needs peace"
        elif kind == "surrender_above" and value > 0:
            return False, f"needs surrender progress above {value:.0%}"
    if law.get("unreadable_conditions"):
        return None, f"{law['unreadable_conditions']} condition(s) the snapshot cannot judge (government, country)"
    return True, "available"


def laws_affordable(politics, at_war, game_dir=None):
    """The derived read of the laws on offer: every law of each group above or below the one in
    force, with its base cost and modifiers from the files and whether the conditions a snapshot
    can judge hold. Whether the power pays is not judged here: the files' cost is before modifiers,
    and the game's own enabled state (hoi4_politics_options) decides. A pure function of the files
    and the snapshot: no input, no game read."""
    f = facts(game_dir)
    held = politics.get("laws") or {}
    group_of = {"mobilization_laws": "conscription", "economy": "economy", "trade_laws": "trade"}
    rows = []
    for group, laws in (f.get("laws") or {}).items():
        current = held.get(group_of.get(group, group))
        for law in laws:
            if law["law"] == current:
                continue
            ok, why = law_available(law, politics, at_war)
            if law.get("country_specific"):
                continue  # a country's own law (allowed = {...}) is the plan's to name
            rows.append({"group": group_of.get(group, group), "law": law["law"], "base_cost": law["cost"],
                         "available": ok, "why": why,
                         "modifiers": law["modifiers"], "current": current})
    return rows
