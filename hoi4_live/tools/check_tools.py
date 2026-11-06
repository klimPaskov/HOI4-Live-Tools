"""Availability checks (test mode): hoi4_check_trigger and hoi4_check_focus.

Both ask the running game, through the console, whether a trigger holds for a country and name the condition that
does not (game/trigger_check.py). They build on hoi4_eval_effect's console route and read the answer from game.log,
so they work in a hoi4_scenario step like any other tool (`expect.text` can match "TRUE" or "FALSE").
"""

import difflib
import os

from ..errors import ToolError
from ..game import trigger_check as tc
from ..gamedata import focus as focus_data
from ..gamedata import triggertext as tt
from .registry import ToolResult, tool

SCOPE = {"type": "string", "pattern": r"^([A-Z][A-Z0-9]{2}|[0-9]{1,5})$",
         "description": "country tag, or a state id for a state trigger; omit for the player"}
DEPTH = {"type": "integer", "minimum": 1, "maximum": 3, "default": 2,
         "description": "how many levels of a false block to explain (1: only the top-level conditions)"}


def _short(nodes, limit=3):
    rows = [n.label or n.text() for n in nodes if n.result is False][:limit]
    return "; ".join(rows)


@tool("hoi4_check_trigger",
      "Evaluate a trigger block in the running game for a country (test mode) and return true or false with the "
      "unmet condition. Each top-level condition is asked on its own, and a false block is explained by its children "
      "(OR: all false, NOT: the child that is true). `trigger` is the conditions as written in a script, with or "
      "without an outer `limit = { }`. A condition longer than about 170 characters cannot go through the console "
      "in one piece and is split into its children. Nothing in the game changes. The answer is read from game.log.",
      {"properties": {
          "trigger": {"type": "string", "minLength": 1, "maxLength": 6000,
                      "description": "conditions, e.g. 'has_war = yes NOT = { is_in_faction = yes }'"},
          "scope": SCOPE,
          "depth": DEPTH},
       "required": ["trigger"]},
      family="testmode", lock="acting", test_only=True,
      examples=[{"args": {"trigger": "has_war = yes", "scope": "GER"}},
                {"args": {"trigger": "limit = { date > 1937.1.1 OR = { tag = GER tag = ITA } }"}, "note": "the player"}])
def hoi4_check_trigger(session, args):
    stmts = tc.require_trigger(args["trigger"])
    nodes = [tc.Node(s) for s in stmts]
    rep = tc.check(session, args.get("scope"), nodes, depth=args["depth"])
    return _result(nodes, rep, scope=args.get("scope"), what="trigger")


def _result(nodes, rep, scope, what, **extra):
    r = rep["result"]
    data = {"result": r, "scope": scope or "player", "checked": rep["checked"], "unmet": tc.unmet(nodes),
            "unknown": tc.unknown(nodes), "errors": rep["errors"]}
    if rep["truncated"]:
        data["truncated"] = "stopped explaining at the limit of checks"
    data.update(extra)
    if r is True:
        head = "TRUE"
    elif r is False:
        head = "FALSE: " + _short(nodes)
    else:
        head = "UNKNOWN: " + "; ".join((n.label or n.text()) + (f" ({n.note})" if n.note else "")
                                        for n in nodes if n.result is None)[:300]
    res = ToolResult(f"{head} ({what} for {scope or 'the player'}, {rep['checked']} checked)", data)
    return res


def _find_focus(idx, focus_id, tag):
    """(tree_id, entry) of the focus; the tag's own tree when the id is in several."""
    hits = [(tid, t["focuses"][focus_id]) for tid, t in idx["trees"].items() if focus_id in t["focuses"]]
    shared = idx.get("shared", {}).get(focus_id)
    if shared and not hits:
        hits = [(None, shared)]
    if not hits:
        names = [f for t in idx["trees"].values() for f in t["focuses"]] + list(idx.get("shared", {}))
        close = difflib.get_close_matches(focus_id, names, n=5, cutoff=0.6)
        raise ToolError(f"focus '{focus_id}' is not in the game files or the enabled mods"
                        + (f" (did you mean {', '.join(close)}?)" if close else ""),
                        {"error": "focus_unknown", "close": close})
    if len(hits) > 1 and tag:
        best = focus_data.tree_for_tag(idx, tag)
        for tid, e in hits:
            if tid == best:
                return tid, e
    return hits[0]


def _raw_focus(entry, tree_id, idx, focus_id):
    path = entry.get("file") or (idx["trees"].get(tree_id) or {}).get("file")
    if not path or not os.path.isfile(path):
        return None, path
    from ..gamedata import scriptdb
    try:
        text = scriptdb.read_bytes(path).decode("utf-8", "replace")
        return tt.find_focus(text, focus_id), path
    except (OSError, tt.ScriptError):
        return None, path


def focus_nodes(entry, raw, focus_id):
    """The conditions that make a focus selectable, as nodes: already completed, each prerequisite group, each
    mutually exclusive focus, then the conditions of `available` and `allow_branch`; bypass is asked separately."""
    one = lambda text: tt.parse(text)[0]
    nodes = []
    done = tc.Node(one(f"has_completed_focus = {focus_id}"), kind="completed", label="already completed")
    done.explain = False
    gating = []
    for group in entry.get("prereq", []):
        text = (f"has_completed_focus = {group[0]}" if len(group) == 1 else
                "OR = { " + " ".join(f"has_completed_focus = {g}" for g in group) + " }")
        gating.append(tc.Node(one(text), kind="prerequisite",
                              label="prerequisite: " + (group[0] if len(group) == 1 else "one of " + ", ".join(group))))
    for m in entry.get("mutex", []):
        gating.append(tc.Node(one(f"NOT = {{ has_completed_focus = {m} }}"), kind="mutually_exclusive",
                              label=f"mutually exclusive with {m}, which is completed"))
    notes = []
    for key in ("available", "allow_branch"):
        blocks = tt.child_blocks(raw, key) if raw is not None else []
        if raw is None and entry.get(key):
            notes.append(f"{key} could not be read from the file, so it was not asked")
        for b in blocks:
            for s in tt.unwrap([b]):
                gating.append(tc.Node(s, kind=key))
    bypass = None
    if raw is not None:
        blocks = tt.child_blocks(raw, "bypass")
        if blocks and blocks[0].children:
            bypass = tc.Node(tt.Stmt("AND", "=", children=blocks[0].children), kind="bypass", label="bypass")
            bypass.explain = False
    nodes = [done] + gating + ([bypass] if bypass else [])
    return nodes, gating, done, bypass, notes


@tool("hoi4_check_focus",
      "Is a national focus selectable for a country, in the running game (test mode)? Returns true or false with the "
      "unmet condition. From the focus's script it asks the game about: already completed, each prerequisite group, "
      "each mutually exclusive focus, every condition of `available` and `allow_branch`; a bypass that holds is "
      "reported too. The files are the game's and the enabled mods' (common/national_focus). A focus already in "
      "progress, the focus slots and the political power are not checked. Nothing in the game changes.",
      {"properties": {
          "focus": {"type": "string", "minLength": 1, "pattern": r"^[A-Za-z0-9_.\-]+$", "description": "focus id"},
          "tag": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "country tag; omit for the player"},
          "depth": DEPTH},
       "required": ["focus"]},
      family="testmode", lock="acting", test_only=True,
      examples=[{"args": {"focus": "GER_rhineland", "tag": "GER"}}, {"args": {"focus": "my_mod_focus_2"}}])
def hoi4_check_focus(session, args):
    fid, tag = args["focus"], args.get("tag")
    idx = focus_data.for_session(session)
    tree_id, entry = _find_focus(idx, fid, tag)
    raw, path = _raw_focus(entry, tree_id, idx, fid)
    nodes, gating, done, bypass, notes = focus_nodes(entry, raw, fid)
    rep = tc.check(session, tag, nodes, depth=args["depth"])
    available = tc.overall(gating)
    if done.result is True:
        status, available = "completed", False
    else:
        status = {True: "available", False: "unavailable", None: "unknown"}[available]
    data = {"focus": fid, "tag": tag or "player", "tree": tree_id, "file": path, "status": status,
            "available": available, "completed": done.result, "bypass": None if bypass is None else bypass.result,
            "checked": rep["checked"], "unmet": tc.unmet(gating), "unknown": tc.unknown(gating),
            "errors": rep["errors"], "not_checked": ["a focus already in progress", "focus slots", "political power"]
            + notes}
    if rep["truncated"]:
        data["truncated"] = "stopped explaining at the limit of checks"
    if bypass is not None and bypass.result is True:
        data["bypass_note"] = "the bypass holds: the focus is completed without being taken"
    if status == "completed":
        head = f"focus {fid} for {tag or 'the player'}: already completed"
    elif available is True:
        head = f"focus {fid} for {tag or 'the player'}: AVAILABLE"
    elif available is False:
        head = f"focus {fid} for {tag or 'the player'}: NOT AVAILABLE, unmet: " + _short(gating)
    else:
        head = (f"focus {fid} for {tag or 'the player'}: UNKNOWN, not answered: "
                + "; ".join(n.label or n.text() for n in gating if n.result is None)[:300])
    return ToolResult(head, data)
