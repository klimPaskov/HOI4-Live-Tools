"""Scenario hooks of a tag: the scripted rules around it, found by reference, with sources.

hooks(world, tag) collects, from the effective files:

    wars          declare_war_on at the start in every country history (both directions)
    on_actions    every conditional block of common/on_actions that names the tag (a `TAG = {` scope,
                  tag/original_tag = TAG, has_war_with = TAG, a TAG_ flag or variable) or sits in an
                  on_action of the tag (on_daily_TAG, on_weekly_TAG ...); each with its condition
                  text, effect summary, events fired, and the numeric thresholds of its condition
                  (`count > 24`, `ETH_war_escalation_level > 4`)
    outcomes      win / defeat / victory / peace flags set by scripted effects and on_actions for
                  the tag, with the condition chain that leads to each (an ELSE is "NOT (limit)")
    missions      decisions with days_mission_timeout of the tag (timer resolved from variables)
    goals         decisions whose `available` asks for control of states or provinces
    timers        events the history schedules (days), and numeric start variables named *_days,
                  *_time, *_length, *_cd, *_timer
    escalation    the numeric thresholds above, flattened

event(world, id)   options with name, localised text, trigger, ai_chance and effect summary
grep(world, token) every line of the effective text files that holds the token, with file:line, mod
ai_strategy(world, tag)  common/ai_strategy blocks enabled for the tag (FAIR: private)
"""
import os
import re

from . import derive
from .script import bare, get1, line_of, parse_file, render
from .world import control_lists, effect_summary, num

OUTCOME = re.compile(r"(victory|victorious|defeat|_win|_won|_lost|_lose|_loss|fail|success|surrender|capitulat|"
                     r"peace|humiliat|triumph)", re.I)
TIMER_VAR = re.compile(r"(_days|_time|_length|_cd|_timer|_cooldown|_duration)$", re.I)
CMP_OPS = ("<", ">", "<=", ">=")


def _mentions(text, tag):
    return bool(re.search(rf"(?<![A-Za-z0-9_]){tag}(?![A-Za-z0-9])", text))


def thresholds(items, path=()):
    """[(path text, op, value)] of every numeric comparison in a trigger block."""
    out = []
    ops = getattr(items, "ops", None)
    for i, (k, v) in enumerate(items):
        op = ops[i] if ops is not None and i < len(ops) else "="
        if isinstance(v, list):
            out.extend(thresholds(v, path + (k or "{}",)))
        elif op in CMP_OPS and num(v) is not None and k:
            if k in ("date", "has_start_date"):
                continue
            out.append((" ".join(p for p in path if p not in ("limit", "AND", "and")) + (" " if path else "") + k,
                        op, num(v)))
    return out


def _cmp_text(t):
    return f"{t[0]} {t[1]} {t[2]}"


def _on_action_hooks(world, tag):
    rows = []
    for p in world.game.files("common/on_actions"):
        for k, v in parse_file(p):
            if k != "on_actions" or not isinstance(v, list):
                continue
            for j, (action, body) in enumerate(v):
                if not action or not isinstance(body, list):
                    continue
                own = action.endswith("_" + tag)
                for bk, bv in body:
                    if bk == "effect" and isinstance(bv, list):
                        _scan_effect(world, tag, action, bv, p, own, rows, ())
                    elif bk in ("events", "random_events") and own and isinstance(bv, list):
                        for e in bare(bv):
                            rows.append({"on_action": action, "kind": "event", "events": [e],
                                         "src": world.src(p, line_of(body, 0)), "condition": None})
    return rows


def _scan_effect(world, tag, action, block, path, own, rows, conds, depth=0):
    """Record each IF of an on_action effect that names the tag; recurse into the ones that do not."""
    prev_limit = None
    for i, (k, v) in enumerate(block):
        kl = (k or "").lower()
        if not isinstance(v, list):
            continue
        if kl in ("if", "else_if", "else"):
            limit = get1(v, "limit", [])
            cond = world.text(limit, 220, tag) if limit else ""
            if kl == "else" and prev_limit is not None:
                cond = "NOT (" + world.text(prev_limit, 160, tag) + ")"
            text = render(v)
            named = own or _mentions(text, tag)
            if kl != "else":
                prev_limit = limit
            if not named:
                continue
            inner_ifs = [b for a, b in v if (a or "").lower() == "if" and isinstance(b, list)]
            summary = effect_summary([(a, b) for a, b in v if (a or "").lower() != "limit"], 10)
            th = thresholds(limit) if limit else []
            if inner_ifs and depth < 2 and len(inner_ifs) > 1:
                # a wrapper of several rules: report the rules one by one
                _scan_effect(world, tag, action, [(a, b) for a, b in v if (a or "").lower() != "limit"],
                             path, own, rows, conds + ((cond,) if cond else ()), depth + 1)
                continue
            rows.append({"on_action": action, "condition": " AND ".join(c for c in conds + (cond,) if c)[:400],
                         "effects": summary, "thresholds": [_cmp_text(t) for t in th],
                         "events": [s.split(":", 1)[1] for s in summary if s.startswith("event:")],
                         "src": world.src(path, line_of(block, i)), "mod": world.content.source(path)})
        elif re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") and (own or k == tag or _mentions(render(v), tag)):
            summary = effect_summary(v, 10)
            if summary:
                rows.append({"on_action": action, "condition": " AND ".join(conds)[:400] or None,
                             "scope": k, "effects": summary, "thresholds": [],
                             "events": [s.split(":", 1)[1] for s in summary if s.startswith("event:")],
                             "src": world.src(path, line_of(block, i)), "mod": world.content.source(path)})


def scripted_effects(world):
    """{name: (block, path, line)} of common/scripted_effects."""

    def load():
        out = {}
        for p in world.game.files("common/scripted_effects"):
            items = parse_file(p)
            for i, (k, v) in enumerate(items):
                if k and isinstance(v, list):
                    out[k] = (v, p, line_of(items, i))
        return out

    return world.cached("scripted_effects", load)


def outcomes(world, tag):
    """Flags whose names mark an outcome (victory, defeat, peace ...) set for the tag, with the
    conditions that lead to them."""
    rows = []

    def walk(block, path, conds, scope, where):
        prev = None
        for i, (k, v) in enumerate(block):
            kl = (k or "").lower()
            if kl in ("set_country_flag", "set_global_flag"):
                flag = get1(v, "flag") if isinstance(v, list) else v
                if isinstance(flag, str) and OUTCOME.search(flag) and (flag.startswith(tag + "_") or scope == tag):
                    row = {"flag": flag, "scope": scope, "set_in": where,
                           "conditions": [c[0] for c in conds if c[0]][-3:],
                           "src": world.src(path, line_of(block, i)), "mod": world.content.source(path)}
                    ctl = [control_lists(c[1]) for c in conds if c[1]]
                    ctl = [x for x in ctl if x]
                    if ctl:
                        row["requires_control"] = {"states": sorted({s for x in ctl for s in x["states"]}),
                                                   "provinces": sorted({q for x in ctl for q in x["provinces"]})}
                    rows.append(row)
                continue
            if not isinstance(v, list):
                continue
            if kl in ("if", "else_if"):
                lim = get1(v, "limit", [])
                prev = lim
                c = world.text(lim, 200, tag) if lim else ""
                walk(v, path, conds + ((c, lim),), scope, where)
            elif kl == "else":
                c = "NOT (" + world.text(prev, 160, tag) + ")" if prev is not None else ""
                walk(v, path, conds + ((c, None),), scope, where)
            elif re.match(r"^[A-Z][A-Z0-9]{2}$", k or ""):
                walk(v, path, conds, k, where)
            elif kl != "limit":
                walk(v, path, conds, scope, where)

    for name, (b, p, ln) in scripted_effects(world).items():
        if name.startswith(tag + "_") or _mentions(name, tag):
            walk(b, p, (), None, name)
    for p in world.game.files("common/on_actions"):
        for k, v in parse_file(p):
            if k == "on_actions" and isinstance(v, list):
                for action, body in v:
                    if isinstance(body, list):
                        walk(body, p, (), tag if action.endswith("_" + tag) else None, action)
    # where each scripted effect with an outcome is called from (on_actions only)
    callers = {}
    names = {r["set_in"] for r in rows}
    for p in world.game.files("common/on_actions"):
        items = parse_file(p)
        for k, v in items:
            if k == "on_actions" and isinstance(v, list):
                for j, (action, body) in enumerate(v):
                    if not isinstance(body, list):
                        continue
                    text = render(body)
                    for n in names:
                        if re.search(rf"\b{re.escape(n)}\s*=\s*yes", text):
                            callers.setdefault(n, []).append(f"{action} ({world.src(p, line_of(v, j))})")
    for r in rows:
        if r["set_in"] in callers:
            r["called_from"] = callers[r["set_in"]]
    seen, out = set(), []
    for r in rows:
        key = (r["flag"], r["src"])
        if key not in seen:
            seen.add(key)
            out.append(r)
    return out


def hooks(world, tag=None):
    tag = tag or world.tag
    return world.cached("hooks:" + tag, lambda: _hooks(world, tag))


def _hooks(world, tag):
    c = world.country(tag)
    wars = [w for w in world.wars() if tag in (w["attacker"], w["target"])]
    ona = _on_action_hooks(world, tag)
    startup = [r for r in ona if r["on_action"] == "on_startup"]
    missions = [m for m in world.decisions(tag, missions=True) if m.get("specific")]
    goals = [d for d in world.decisions(tag, missions=False) if d.get("controls") and d.get("specific") and
             (len(d["controls"]["states"]) + len(d["controls"]["provinces"]) >= 2 or d["id"].startswith("form_"))]
    timers = []
    for e in c["events"]:
        timers.append({"kind": "event", "id": e["v"], "days": e.get("days"), "random_days": e.get("random_days"),
                       "src": e["src"]})
    for v in c["variables"]:
        if TIMER_VAR.search(v["name"]):
            timers.append({"kind": "variable", "id": v["name"], "days": v["v"], "src": v["src"]})
    # mission deadlines the history sets in motion (derive.scenario_timers, shared with the lint)
    deadlines = []
    for m in derive.scenario_timers(world.game, tag, world.ctx_for(tag), world.date):
        d = world.find_decision(m["mission"])
        deadlines.append(dict(m, name=world.name(m["mission"]),
                              src=world.src(d["path"], d["line"]) if d else None,
                              d="history-scheduled events followed to the mission; deadline = activation + "
                                "days_mission_timeout"))
    esc = []
    for r in ona:
        for t in r.get("thresholds") or []:
            esc.append({"threshold": t, "on_action": r["on_action"], "effects": r["effects"][:4], "src": r["src"]})
    return {"tag": tag, "wars": wars, "global_flags": [f["v"] for f in c["global_flags"]],
            "flags": [f["v"] for f in c["flags"]], "on_startup": startup,
            "on_actions": [r for r in ona if r["on_action"] != "on_startup"], "outcomes": outcomes(world, tag),
            "missions": missions, "deadlines": deadlines, "goals": goals, "timers": timers, "escalation": esc,
            "fair": "public"}


# ---------------------------------------------------------------- events

def event(world, eid):
    """Event detail: type, title/desc text, trigger, mean time, options (name, text, trigger,
    ai_chance, effects) with sources; None when unknown."""
    hit = world.game.event(eid)
    if not hit:
        return None
    path = hit["file"]
    items = parse_file(path)
    for i, (k, v) in enumerate(items):
        if not isinstance(v, list) or get1(v, "id") != eid:
            continue
        title = get1(v, "title")
        desc = get1(v, "desc")
        out = {"id": eid, "type": k, "title": world.loc.get(title) if isinstance(title, str) else None,
               "desc": (world.loc.get(desc) or "")[:300] if isinstance(desc, str) else None,
               "hidden": get1(v, "hidden") == "yes", "triggered_only": get1(v, "is_triggered_only") == "yes",
               "fire_only_once": get1(v, "fire_only_once") == "yes",
               "src": world.src(path, line_of(items, i)), "mod": world.content.source(path), "options": []}
        trig = get1(v, "trigger")
        if isinstance(trig, list) and trig:
            out["trigger"] = world.text(trig, 300)
        mtth = get1(v, "mean_time_to_happen")
        if isinstance(mtth, list):
            out["mean_time_to_happen"] = render(mtth, 160)
        imm = get1(v, "immediate")
        if isinstance(imm, list):
            out["immediate"] = effect_summary(imm, 10)
        for j, (ok, ov) in enumerate(v):
            if ok != "option" or not isinstance(ov, list):
                continue
            name = get1(ov, "name")
            row = {"name": name, "text": world.loc.get(name) if name else None,
                   "src": world.src(path, line_of(v, j)),
                   "effects": effect_summary([(a, b) for a, b in ov if a not in ("trigger", "ai_chance", "name")], 12)}
            ot = get1(ov, "trigger")
            if isinstance(ot, list):
                row["trigger"] = world.text(ot, 200)
            ai = get1(ov, "ai_chance")
            if isinstance(ai, list):
                row["ai_chance"] = render(ai, 200)
            out["options"].append(row)
        return out
    return None


# ---------------------------------------------------------------- grep

GREP_EXT = (".txt", ".lua", ".yml", ".csv", ".gui", ".gfx", ".asset")


def grep(world, token, limit=400, folders=None, languages=("english",)):
    """[{src, mod, text}] of every line holding `token` as a word in the effective text files
    (localisation only in `languages`; None for all)."""
    needle = token.encode("utf-8")
    word = re.compile(rb"(?<![A-Za-z0-9_])" + re.escape(needle) + rb"(?![A-Za-z0-9_])")
    out = []
    merged = world.content.merged()
    for key in sorted(merged):
        rel, path, mod = merged[key]
        if folders and not any(key.startswith(f.lower().rstrip("/") + "/") for f in folders):
            continue
        if not key.endswith(GREP_EXT):
            continue
        if languages and key.startswith("localisation/") and not any(lang in key for lang in languages):
            continue
        try:
            with open(path, "rb") as fh:
                data = fh.read()
        except OSError:
            continue
        if needle not in data:
            continue
        for n, line in enumerate(data.split(b"\n"), 1):
            if needle in line and word.search(line):
                out.append({"src": f"{rel}:{n}", "mod": mod,
                            "text": line.decode("utf-8", "replace").strip()[:200]})
                if len(out) >= limit:
                    return out
    return out


# ---------------------------------------------------------------- AI strategy (FAIR: private)

def ai_strategy(world, tag):
    """[{id, enable, strategies: ["type id value"], src}] of ai_strategy plans that can apply to the tag."""
    out = []
    for p in world.game.files("common/ai_strategy"):
        items = parse_file(p)
        for i, (k, v) in enumerate(items):
            if not k or not isinstance(v, list):
                continue
            enable = get1(v, "enable")
            allowed = get1(v, "allowed")
            text = render(enable) if isinstance(enable, list) else ""
            if not (_mentions(text, tag) or os.path.basename(p).upper().startswith(tag)):
                continue
            if isinstance(allowed, list):
                from .script import trigger
                if trigger(allowed, world.ctx_for(tag)) is False:
                    continue
            strategies = []
            for a, b in v:
                if a == "ai_strategy" and isinstance(b, list):
                    strategies.append(" ".join(str(x) for x in (get1(b, "type"), get1(b, "id"), get1(b, "target"),
                                                                 get1(b, "value")) if x is not None))
            out.append({"id": k, "enable": world.text(enable, 200, tag) if isinstance(enable, list) else None,
                        "strategies": strategies, "src": world.src(p, line_of(items, i)), "fair": "private"})
    return out
