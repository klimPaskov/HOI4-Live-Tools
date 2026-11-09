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
bop(world, tag)    balance of power: definition, value and range at start, focuses/decisions that gain or
                   move it (BOP_METHOD)
never_take(world, tag)   decisions that end a war, cede states or withdraw (NEVER_METHOD)
tag_sources(world, tag)  effect blocks of the tag's loaded focuses and its decisions; find_effects() searches them
factions(world, tag)     faction templates the tag can create (by its focuses/decisions or selectable), with
                         manifest, goals, default rules; rule groups; member upgrades (FACTION_METHOD)
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
            cond = world.text(limit, None, tag) if limit else ""
            if kl == "else" and prev_limit is not None:
                cond = "NOT (" + world.text(prev_limit, None, tag) + ")"
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
            rows.append({"on_action": action, "condition": " AND ".join(c for c in conds + (cond,) if c),
                         "effects": summary, "thresholds": [_cmp_text(t) for t in th],
                         "events": [s.split(":", 1)[1] for s in summary if s.startswith("event:")],
                         "src": world.src(path, line_of(block, i)), "mod": world.content.source(path)})
        elif re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") and (own or k == tag or _mentions(render(v), tag)):
            summary = effect_summary(v, 10)
            if summary:
                rows.append({"on_action": action, "condition": " AND ".join(conds) or None,
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


# ---------------------------------------------------------------- references (event and effect callers)

EVENT_KEYS = ("country_event", "news_event", "state_event", "unit_leader_event", "operative_leader_event")
REF_ROOTS = ("events/", "common/", "history/")
SCOPE_WORDS = re.compile(r"^(ROOT|FROM|PREV|THIS|owner|controller|overlord|faction_leader|capital_scope|"
                         r"(every|random|all|any)_\w+|event_target:\w+|var:\S+|[A-Z][A-Z0-9]{2}|\d+)$")
DELAY_KEYS = ("days", "hours", "random", "random_days", "random_hours")
SKIP_BLOCKS = ("limit", "trigger", "available", "allowed", "visible", "ai_will_do", "ai_chance", "bypass",
               "cancel_if_invalid", "allow_branch")


def _label(k, v):
    ident = (get1(v, "id") or get1(v, "name")) if isinstance(v, list) else None
    return f"{k}({ident})" if isinstance(ident, str) else (k or "{}")


def find_refs(world, needle, match):
    """Every place in the effective script files (events, common, history; FAIR leaves out common/ai_*)
    where match(key, value) holds: {src, mod, key, via (block chain), scope (tags, ROOT, FROM, every_* ...
    on the way, "self" when none), conditions (if/else_if limits on the way), delay (days/hours keys of
    the hit block)}. `needle` filters the files by their raw text first."""
    raw = needle.encode("utf-8")
    skip_ai = getattr(world, "mode", "fair") == "fair"
    out = []
    merged = world.content.merged()
    for key in sorted(merged):
        if not key.endswith(".txt") or not key.startswith(REF_ROOTS) or (skip_ai and ai_only(key)):
            continue
        rel, path, mod = merged[key]
        try:
            with open(path, "rb") as fh:
                if raw not in fh.read():
                    continue
        except OSError:
            continue

        def walk(block, chain, scopes, conds, path=path, mod=mod):
            for i, (k, v) in enumerate(block):
                if match(k, v):
                    row = {"src": world.src(path, line_of(block, i)), "mod": mod, "key": k,
                           "via": " > ".join(chain), "scope": " > ".join(scopes) or "self",
                           "conditions": list(conds)}
                    if isinstance(v, list):
                        delay = {d: get1(v, d) for d in DELAY_KEYS if isinstance(get1(v, d), str)}
                        if delay:
                            row["delay"] = delay
                    out.append(row)
                if not isinstance(v, list):
                    continue
                kl = (k or "").lower()
                c = conds
                if kl in ("if", "else_if"):
                    lim = get1(v, "limit")
                    if isinstance(lim, list) and lim:
                        c = conds + (world.text(lim, None),)
                elif kl == "else":
                    c = conds + ("else",)
                elif kl in SKIP_BLOCKS:
                    continue
                sc = scopes + ((k,) if k and SCOPE_WORDS.match(k) else ())
                walk(v, chain + (_label(k, v),), sc, c)

        walk(parse_file(path), (), (), ())
    return out


def _event_match(eid):
    def match(k, v):
        if k in EVENT_KEYS:
            return v == eid or (isinstance(v, list) and get1(v, "id") == eid)
        # on_actions: events = { id } and random_events = { 100 = id }
        return v == eid and (k is None or re.match(r"^\d+$", k) is not None)
    return match


def event_callers(world, eid):
    """[{src, via, scope, conditions, delay, key, effect?, effect_called_from?}] of every place that fires
    the event: focus rewards, decision effects, other events' options and immediates, on_actions, scripted
    effects, history. `scope` is the scope chain at the call, so the receiver is that scope's country (an
    explicit tag, FROM, every_enemy_country ...) or the running country ("self"). A hit inside a
    scripted effect also lists where that effect is called."""
    rows = [r for r in find_refs(world, eid, _event_match(eid))
            if not (r["via"] == "" and r["src"].startswith("events/"))]
    effects = scripted_effects(world)
    for r in rows:
        top = r["via"].split(" > ")[0] if r["via"] else ""
        if top in effects and r["src"].startswith("common/scripted_effects/"):
            r["effect"] = top
            r["effect_called_from"] = [f"{c['src']} ({c['via'] or 'top'}; scope {c['scope']})"
                                       for c in effect_callers(world, top)[:20]]
    return rows


def effect_callers(world, name):
    """Places that call a scripted effect (`name = yes`)."""
    return find_refs(world, name, lambda k, v: k == name and v == "yes")


def effect_body(world, name):
    """{name, kind, src, mod, text, summary} of a scripted effect (else a scripted trigger) with its full
    source text; None when unknown."""
    hit = scripted_effects(world).get(name)
    kind = "scripted_effect"
    if not hit:
        for p in world.game.files("common/scripted_triggers"):
            items = parse_file(p)
            for i, (k, v) in enumerate(items):
                if k == name and isinstance(v, list):
                    hit, kind = (v, p, line_of(items, i)), "scripted_trigger"
                    break
            if hit:
                break
    if not hit:
        return None
    block, path, line = hit
    try:
        with open(path, encoding="utf-8-sig", errors="replace") as fh:
            lines = fh.read().split("\n")
    except OSError:
        lines = []
    body, depth, started = [], 0, False
    for ln in lines[(line or 1) - 1:]:
        body.append(ln.rstrip())
        code = ln.split("#", 1)[0]
        depth += code.count("{") - code.count("}")
        started = started or "{" in code
        if started and depth <= 0:
            break
    return {"name": name, "kind": kind, "src": world.src(path, line), "mod": world.content.source(path),
            "text": "\n".join(body), "summary": effect_summary(block, 30) if kind == "scripted_effect" else []}


# ---------------------------------------------------------------- grep

GREP_EXT = (".txt", ".lua", ".yml", ".csv", ".gui", ".gfx", ".asset")


# Folders only the AI reads (strategy plans, front and template preferences, peace weights): FAIR private.
AI_ONLY_PREFIX = "common/ai_"


def ai_only(key):
    """True for a content path in an AI-only folder (common/ai_strategy, ai_strategy_plans, ai_templates ...)."""
    return key.lower().replace("\\", "/").startswith(AI_ONLY_PREFIX)


def grep(world, token, limit=400, folders=None, languages=("english",), context=0, skip_ai=None):
    """[{src, mod, text}] of every line holding `token` as a word in the effective text files
    (localisation only in `languages`; None for all). A token that never stands as a word (a suffix
    such as `italo_ethiopian_war_events.110` of `BBA_italo_ethiopian_war_events.110`) matches as a
    substring instead. `context` adds that many lines before and after each hit ("context" list).
    skip_ai (default: FAIR mode) leaves out the AI-only folders (common/ai_*); the result is then
    marked by a final row {"hidden_ai_only": n} when hits there were left out."""
    if skip_ai is None:
        skip_ai = getattr(world, "mode", "fair") == "fair"
    needle = token.encode("utf-8")
    word = re.compile(rb"(?<![A-Za-z0-9_])" + re.escape(needle) + rb"(?![A-Za-z0-9_])")
    merged = world.content.merged()
    files = []
    hidden = 0
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
        if skip_ai and ai_only(key):
            hidden += 1
            continue
        files.append((rel, mod, data.split(b"\n")))

    def scan(match):
        out = []
        for rel, mod, lines in files:
            for n, line in enumerate(lines, 1):
                if needle in line and match(line):
                    row = {"src": f"{rel}:{n}", "mod": mod, "text": line.decode("utf-8", "replace").strip()[:200]}
                    if context:
                        lo, hi = max(0, n - 1 - context), min(len(lines), n + context)
                        row["context"] = [f"{k + 1}: " + lines[k].decode("utf-8", "replace").rstrip()
                                          for k in range(lo, hi)]
                    out.append(row)
                    if len(out) >= limit:
                        return out
        return out

    out = scan(word.search)
    if not out:
        out = [dict(r, substring=True) for r in scan(lambda line: True)]
    if hidden:
        out.append({"hidden_ai_only": hidden, "fair": "private",
                    "note": "FAIR: hits in AI-only folders (common/ai_*) left out; --mode meta shows them"})
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


# ---------------------------------------------------------------- balance of power

BOP_EFFECTS = ("set_power_balance", "add_power_balance_value", "add_power_balance_modifier",
               "remove_power_balance_modifier", "remove_power_balance")
_NOT_RUN = ("limit", "ai_will_do", "available", "visible", "allowed", "trigger", "effect_tooltip",
            "custom_trigger_tooltip", "ai_chance", "modifier")
BOP_METHOD = ("value at start = initial_value of common/bop when the history's set_power_balance has set_default (or "
              "no set_value), else set_value, plus add_power_balance_value; movers = the effects set_power_balance, "
              "add_power_balance_value, add_power_balance_modifier, remove_power_balance(_modifier) in the loaded "
              "focus tree's completion rewards and the tag's decisions (complete/remove effects), scripted effects "
              "followed two levels; effect_tooltip blocks (display only) are not counted; a modifier's daily drift is "
              "its power_balance_daily (common/modifiers; power_balance_weekly / 7)")


def static_modifiers(world):
    """{modifier name: {key: value}} of common/modifiers (numbers only)."""

    def load():
        out = {}
        for p in world.game.files("common/modifiers"):
            for k, v in parse_file(p):
                if k and isinstance(v, list):
                    out[k] = {a: num(b) for a, b in v if a and num(b) is not None}
        return out

    return world.cached("staticmods", load)


def bop_moves(world, block, depth=0, via=None):
    """[{"effect", "id", "value", "modifier", "via"}] of the power-balance effects an effect block runs
    (if/else branches all counted; scripted effects followed two levels; effect_tooltip skipped)."""
    scripted = scripted_effects(world)
    out = []
    if not isinstance(block, list):
        return out
    for k, v in block:
        kl = (k or "").lower()
        if kl in BOP_EFFECTS:
            if isinstance(v, list):
                val = get1(v, "value") if get1(v, "value") is not None else get1(v, "set_value")
                out.append({"effect": kl, "id": get1(v, "id"), "value": num(val), "modifier": get1(v, "modifier"),
                            "set_default": get1(v, "set_default") == "yes", "via": via})
            else:
                out.append({"effect": kl, "id": v, "value": None, "modifier": None, "via": via})
        elif isinstance(v, list):
            if kl not in _NOT_RUN:
                out.extend(bop_moves(world, v, depth, via))
        elif v == "yes" and k in scripted and depth < 2:
            out.extend(bop_moves(world, scripted[k][0], depth + 1, via or k))
    return out


def bop(world, tag=None):
    """Balance of power of a tag (BOP_METHOD): [{id, definition, start, gained_by, movers, mover_counts}]."""
    from . import focus as focusmod  # local: keeps scenario importable without the focus module loaded

    tag = tag or world.tag
    return world.cached("bop:" + tag, lambda: _bop(world, tag, focusmod))


def _bop(world, tag, focusmod):
    defs = world.power_balances()
    mods = static_modifiers(world)
    rows = {}

    def row(bid):
        if bid not in rows:
            d = defs.get(bid)
            rows[bid] = {"id": bid, "definition": d, "start": None, "gained_by": [], "movers": [],
                         "src": d["src"] if d else None, "fair": "public", "d": BOP_METHOD}
        return rows[bid]

    for f in world.country(tag)["power_balance"]:
        r = row(f["v"])
        d = r["definition"] or {}
        if f["kind"] == "set":
            value = d.get("initial_value") if (f.get("set_default") or f.get("value") is None) else f["value"]
            r["start"] = {"active": True, "value": value, "src": f["src"]}
        elif r["start"]:
            r["start"]["value"] = round((r["start"]["value"] or 0) + (f.get("value") or 0), 4)
    sources = []
    tid = focusmod.tree_id(world, tag)
    trees, shared = focusmod._focus_blocks(world)
    loaded = set(focusmod.tree(world, tag)["focuses"])
    for b, p, ln in (trees.get(tid) or {}).get("focuses", []) + [shared[x] for x in sorted(loaded) if x in shared]:
        fid = get1(b, "id")
        if fid in loaded:
            sources.append(("focus", fid, get1(b, "completion_reward"), world.src(p, ln)))
    for dd in world.decision_index():
        if dd["id"].startswith(tag + "_") or _names_tag_text(dd, tag):
            for key in ("complete_effect", "remove_effect"):
                sources.append(("decision", dd["id"], get1(dd["block"], key), world.src(dd["path"], dd["line"])))
    for kind, sid, block, src in sources:
        for m in bop_moves(world, block):
            bid = m["id"]
            if not bid or (bid not in defs and not bid.startswith(tag + "_")):
                continue
            r = row(bid)
            daily = None
            if m.get("modifier"):
                mm = mods.get(m["modifier"], {})
                daily = mm.get("power_balance_daily")
                if daily is None and mm.get("power_balance_weekly") is not None:
                    daily = round(mm["power_balance_weekly"] / 7.0, 5)
            mv = {"kind": kind, "id": sid, "effect": m["effect"], "value": m["value"], "modifier": m.get("modifier"),
                  "daily": daily, "via": m["via"], "src": src}
            r["movers"].append(mv)
            if m["effect"] == "set_power_balance" and not (r["start"] or {}).get("active"):
                r["gained_by"].append(mv)
    for bid in defs:
        if bid.startswith(tag + "_"):
            row(bid)
    out = []
    for bid, r in sorted(rows.items()):
        if r["start"] is None:
            r["start"] = {"active": False, "value": None}
        val = r["start"].get("value")
        if val is not None and r["definition"]:
            rng = [x for x in r["definition"]["ranges"] if x["min"] is not None and x["max"] is not None
                   and x["min"] <= val <= x["max"]]
            r["start"]["range"] = rng[0]["id"] if rng else None
        counts = {}
        for mv in r["movers"]:
            counts[mv["kind"]] = counts.get(mv["kind"], 0) + 1
        r["mover_counts"] = counts
        out.append(r)
    return out


def _names_tag_text(d, tag):
    allowed = get1(d["block"], "allowed")
    return isinstance(allowed, list) and _mentions(render(allowed), tag)


# ---------------------------------------------------------------- never-take decisions

NEVER_NAME = re.compile(r"(withdraw|surrender|capitulat|abandon|evacuat|white_peace)", re.I)
NEVER_METHOD = ("never-take = a decision whose complete or remove effect (effect_tooltip included: it states the "
                "outcome; scripted effects followed two levels) makes white_peace, transfers a state to another "
                "country (transfer_state inside another country's scope, transfer_state_to / set_state_owner to "
                "another tag), releases a country from our states (release, release_puppet), or whose id names a "
                "withdrawal or surrender; whether a listed decision is ever wanted is the planner's call [?]")
_SELF = ("ROOT", "THIS", "PREV", "FROM")
_NEVER_SKIP = ("limit", "ai_will_do", "available", "visible", "allowed", "trigger", "ai_chance", "modifier")


def _never_reasons(world, block, tag, scope=None, depth=0):
    scripted = scripted_effects(world)
    out = []
    if not isinstance(block, list):
        return out
    for k, v in block:
        kl = (k or "").lower()
        target = get1(v, "target") if isinstance(v, list) else v
        if kl == "white_peace":
            out.append(f"white_peace:{target}")
        elif kl == "transfer_state" and scope and scope != tag and scope not in _SELF and not scope.isdigit():
            out.append(f"transfer_state:{target}->{scope}")
        elif kl in ("transfer_state_to", "set_state_owner") and isinstance(target, str) and target != tag and \
                target not in _SELF:
            out.append(f"{kl}:{scope or '?'}->{target}")
        elif kl in ("release", "release_puppet", "release_on_controlled"):
            out.append(f"{kl}:{target}")
        elif isinstance(v, list):
            if kl in _NEVER_SKIP:
                continue
            sub = k if (re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") or (k or "").isdigit()) else scope
            out.extend(_never_reasons(world, v, tag, sub, depth))
        elif v == "yes" and k in scripted and depth < 2:
            out.extend(f"{x} (via {k})" for x in _never_reasons(world, scripted[k][0], tag, scope, depth + 1))
    return out


def never_take(world, tag=None):
    """Decisions of the tag that end a war, cede states or withdraw (NEVER_METHOD): [{id, name, reasons, src}]."""
    tag = tag or world.tag

    def load():
        out = []
        for d in world.decision_index():
            if not (d["id"].startswith(tag + "_") or _names_tag_text(d, tag)):
                continue
            reasons = []
            for key in ("complete_effect", "remove_effect"):
                reasons += _never_reasons(world, get1(d["block"], key), tag)
            if NEVER_NAME.search(d["id"]):
                reasons.append("name")
            reasons = list(dict.fromkeys(reasons))
            if reasons:
                out.append({"id": d["id"], "name": world.name(d["id"]), "category": d["category"],
                            "reasons": reasons[:6], "src": world.src(d["path"], d["line"]),
                            "mod": world.content.source(d["path"])})
        return out

    return world.cached("nevertake:" + tag, load)


# ---------------------------------------------------------------- effects of the tag's focuses and decisions

def tag_sources(world, tag=None):
    """[(kind, id, effect block, src)] of the tag's loaded focuses (completion_reward) and its decisions
    (complete_effect, remove_effect): the blocks a player of the tag can run."""
    from . import focus as focusmod  # local: keeps scenario importable without the focus module loaded

    tag = tag or world.tag

    def load():
        out = []
        tid = focusmod.tree_id(world, tag)
        trees, shared = focusmod._focus_blocks(world)
        loaded = set(focusmod.tree(world, tag)["focuses"])
        for b, p, ln in (trees.get(tid) or {}).get("focuses", []) + [shared[x] for x in sorted(loaded) if x in shared]:
            fid = get1(b, "id")
            if fid in loaded:
                out.append(("focus", fid, get1(b, "completion_reward"), world.src(p, ln)))
        for dd in world.decision_index():
            if dd["id"].startswith(tag + "_") or _names_tag_text(dd, tag):
                for key in ("complete_effect", "remove_effect"):
                    out.append(("decision", dd["id"], get1(dd["block"], key), world.src(dd["path"], dd["line"])))
        return out

    return world.cached("tagsources:" + tag, load)


def find_effects(block, keys, scripted=None, depth=0):
    """[(key, value)] of the effects named in `keys` anywhere in an effect block (scripted effects followed
    two levels when `scripted` is given; trigger and tooltip blocks skipped)."""
    out = []
    if not isinstance(block, list):
        return out
    for k, v in block:
        if k in keys:
            out.append((k, v))
        elif isinstance(v, list):
            if k not in _NOT_RUN:
                out.extend(find_effects(v, keys, scripted, depth))
        elif scripted and v == "yes" and k in scripted and depth < 2:
            out.extend(find_effects(scripted[k][0], keys, scripted, depth + 1))
    return out


# ---------------------------------------------------------------- factions (F-17)

FACTION_METHOD = ("templates = common/factions/templates the tag can create: named by create_faction_from_template in "
                  "its focuses or decisions, or selectable (static `visible` not false; an empty visible hides a "
                  "template); each with its manifest, goals and default rules (common/factions/goals, rules); rule "
                  "groups list the rules a leader may switch to (FACTION_INITIATIVE_CHANGE_RULE_COST)")
FACTION_DEFINES = ("FACTION_INITIATIVE_CHANGE_RULE_COST", "FACTION_DOCTRINE_SHARING_UNLOCK_COST",
                   "FACTION_UNLOCK_COMMANDER_COST", "ASSIGN_FACILITY_TO_FACTION_INITIATIVE_COST",
                   "FACTION_ASSIGN_SCIENTIST_COST", "FACTION_THEATER_COMMANDER_COUNTRY_LIMIT_BASE", "TENSION_FACTION_JOIN")


def factions(world, tag=None):
    """Faction facts for a tag (FACTION_METHOD): faction at the start, templates it can create with their manifest,
    goals and default rules, the effects that create or join a faction, rule groups, member upgrades, defines."""
    tag = tag or world.tag
    fc = world.faction_content()
    scripted = scripted_effects(world)
    by, joins = {}, []
    for kind, sid, block, src in tag_sources(world, tag):
        for k, v in find_effects(block, ("create_faction_from_template", "create_faction", "add_to_faction",
                                         "join_faction"), scripted):
            if k == "create_faction_from_template":
                tid = get1(v, "template") if isinstance(v, list) else v
                if any(x["id"] == sid for x in by.get(tid, [])):
                    continue
                by.setdefault(tid, []).append({"kind": kind, "id": sid, "src": src,
                                               "name": get1(v, "name") if isinstance(v, list) else None})
            else:
                joins.append({"effect": k, "target": v if isinstance(v, str) else render(v, 80), "kind": kind,
                              "id": sid, "src": src})
    rows = []
    for tid, t in sorted(fc["templates"].items()):
        vis = t["visible"]
        st = world.static(vis, tag) if isinstance(vis, list) and vis else False
        if tid not in by and st is False:
            continue
        rows.append({"id": tid, "name": t["name"], "created_by": by.get(tid, []), "selectable": st,
                     "available": world.text(t["available"], 160, tag) if isinstance(t["available"], list) else None,
                     "manifest": fc["goals"].get(t["manifest"]) or {"id": t["manifest"]},
                     "goals": [fc["goals"].get(g) or {"id": g} for g in t["goals"]],
                     "default_rules": [fc["rules"].get(r) or {"id": r} for r in t["default_rules"]],
                     "can_leader_join_other_factions": t["can_leader_join_other_factions"],
                     "src": t["src"], "mod": t["mod"]})
    rows.sort(key=lambda r: (not r["created_by"], r["selectable"] is False, r["id"]))
    return {"tag": tag, "faction_at_start": world.country(tag)["faction"], "templates": rows,
            "missing_templates": sorted(t for t in by if t and t not in fc["templates"]), "joins": joins,
            "rule_groups": list(fc["rule_groups"].values()),
            "member_upgrades": {"groups": list(fc["member_groups"].values()),
                                "upgrades": list(fc["member_upgrades"].values())},
            "defines": world.defines_rows(FACTION_DEFINES), "fair": "public", "d": FACTION_METHOD}
