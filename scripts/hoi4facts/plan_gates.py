"""Game-file gates under a plan's own conditions: can a focus start when the plan schedules it?

A focus has prerequisites, exclusions and an `available` trigger in the game files. The plan says what
the world looks like over time: the date, whether the country is at war and with whom, the focuses done
so far, the flags its history sets at the start. `evaluate` reads a trigger against those facts with
three answers (True, False, None for "cannot tell from the plan"), and `focus_schedule` walks the focus
slot day by day: the first listed focus that can start is started, and a stretch where no listed focus
can start is an idle slot.

Nothing here is country specific: every gate comes from the parsed game files and every fact from the
plan. Stdlib only, Python 3.9+.
"""
from .script import Ctx, _and, _or, _pairs, trigger_one

# keys that only wrap other triggers for the tooltip
WRAPPERS = ("custom_trigger_tooltip", "hidden_trigger", "custom_override_tooltip")
WRAPPER_TEXT_KEYS = ("tooltip",)


class Facts:
    """What a plan says about one day: `date` (YYYY.M.D), `war` (True, False or None when the plan does not
    say), `enemies` (tags it is at war with when at war), `focuses` (completed), `flags` (global and country
    flags known to be set that day), `states` (state ids the country owns at the start: a plan keeps its own
    land unless it says otherwise), `tag`, `dlcs`, `government` (the ruling ideology, None when not known)."""

    def __init__(self, tag, dlcs, date, war=None, enemies=(), focuses=(), flags=(), states=(), government=None):
        self.ctx = Ctx(tag, dlcs, date)
        self.government = government
        self.war = war
        self.enemies = set(enemies)
        self.focuses = set(focuses)
        self.flags = set(flags)
        self.states = set(states)


def evaluate(block, facts):
    """True / False / None of a trigger block (implicit AND) under the plan's facts."""
    if not isinstance(block, list):
        return None
    return _and(r for r, _ in _units(_pairs(block), facts))


def _units(pairs, facts):
    """[(result, [(key, value, op), ...])] of a trigger block's lines, an `if` with its `else_if` / `else`
    siblings taken as one unit: its value is the branch its `limit` picks; when the limit cannot be decided,
    the unit is False only if every branch (a missing `else` is an open one) is False, True only if every one
    is True."""
    pairs = list(pairs)
    out, i = [], 0
    while i < len(pairs):
        k, v, op = pairs[i]
        if (k or "").lower() != "if" or not isinstance(v, list):
            out.append((_one(k, v, op, facts), [pairs[i]]))
            i += 1
            continue
        chain = [pairs[i]]
        i += 1
        while i < len(pairs) and (pairs[i][0] or "").lower() in ("else_if", "else") and isinstance(pairs[i][1], list):
            chain.append(pairs[i])
            i += 1
            if (chain[-1][0] or "").lower() == "else":
                break
        out.append((_chain(chain, facts), chain))
    return out


def _chain(chain, facts):
    """An if / else_if / else chain: the value of the branch its limits pick; while a limit cannot be decided,
    every branch it may pick counts (False only if all of them are False, True only if all are True)."""
    possible = []
    for k, v, _ in chain:
        lim = [b for a, b, _ in _pairs(v) if (a or "").lower() == "limit"]
        body = [(a, b) for a, b, _ in _pairs(v) if (a or "").lower() != "limit"]
        took = True if (k or "").lower() == "else" else (evaluate(lim[0], facts) if lim else True)
        if took is False:
            continue
        possible.append(evaluate(body, facts) if body else True)
        if took is True:
            break
    else:
        possible.append(True)  # no branch surely taken and no else: the chain may ask nothing
    if all(r is False for r in possible):
        return False
    if all(r is True for r in possible):
        return True
    return None


def _yes(value, fact):
    """Compare a yes/no trigger value with a three-valued fact."""
    if fact is None or value not in ("yes", "no"):
        return None
    return fact == (value == "yes")


def _one(key, value, op, facts):
    k = (key or "").lower()
    if k in WRAPPERS:
        if not isinstance(value, list):
            return None
        return _and(r for r, _ in _units([(a, b, o) for a, b, o in _pairs(value)
                                           if (a or "").lower() not in WRAPPER_TEXT_KEYS], facts))
    if k == "and":
        return evaluate(value, facts)
    if k == "or":
        if not isinstance(value, list):
            return None
        return _or(_one(a, b, o, facts) for a, b, o in _pairs(value))
    if k == "not":  # NOT is NOR in the game
        if not isinstance(value, list):
            return None
        r = _or(_one(a, b, o, facts) for a, b, o in _pairs(value))
        return None if r is None else not r
    if k == "has_war":
        return _yes(value, facts.war)
    if k == "has_war_with" and isinstance(value, str):
        if facts.war is False:
            return False
        if facts.war and value in facts.enemies:
            return True
        return None  # a war the plan does not name can still start
    if k == "has_government" and isinstance(value, str):
        return None if getattr(facts, "government", None) is None else value == facts.government
    if k == "has_completed_focus" and isinstance(value, str):
        return value in facts.focuses
    if k in ("has_global_flag", "has_country_flag") and isinstance(value, str):
        return True if value in facts.flags else None  # a flag not set at the start may be set later
    if k in ("owns_state", "controls_state", "has_full_control_of_state") and isinstance(value, str):
        return True if value.isdigit() and int(value) in facts.states else None  # other land may be taken later
    return trigger_one(key, value, facts.ctx, op)


def blocking(block, facts):
    """The trigger lines that are False under the facts (the reason a gate is closed), outermost first."""
    out = []
    if not isinstance(block, list):
        return out
    from .script import render
    for res, unit in _units(_pairs(block), facts):
        if res is not False:
            continue
        if len(unit) > 1 or (unit[0][0] or "").lower() == "if":
            for _, v, _ in unit:  # a closed if / else chain: what closes each branch
                out += blocking([(a, b) for a, b, _ in _pairs(v) if (a or "").lower() != "limit"], facts)
            continue
        k, v, op = unit[0]
        if True:
            kl = (k or "").lower()
            if kl in WRAPPERS + ("and",) and isinstance(v, list):
                inner = blocking([(a, b) for a, b in v if (a or "").lower() not in WRAPPER_TEXT_KEYS], facts)
                out += inner or [render([(k, v)], 120)]
            else:
                out.append(render([(k, v)], 120) if isinstance(v, list) else f"{k} {op} {v}")
    return out


def focus_schedule(path, fallback, tree, meta, days_per, start, horizon, facts_at, done=(), changes=(), holds=None):
    """Walk the focus slot from day `start` to `horizon` (day numbers).

    path / fallback: focus ids in plan order; tree: {focus: {"prereq": [set], "exclusive": set}}; meta:
    {focus: {"available": block or None, "cost": float}}; facts_at(day, completed) -> Facts; changes: days on
    which the plan's conditions change (a war starts or ends), so a blocked slot is tried again then; holds:
    {focus: day} the plan's focus_when keeps a focus from starting before (the executor takes the next listed
    focus meanwhile); the slot is tried again on each such day.

    Each time the slot is free the first focus of the path, then of the fallback, that can start is taken:
    prerequisites done, no exclusive focus done, `available` not False. Returns
      rows:   [{"focus", "start", "end", "from": "path" | "fallback", "gate": True | None}] (None: the gate
              could not be decided from the plan; the row is an assumption)
      idle:   [{"from", "to", "blocked": [{"focus", "why"}], "exhausted": bool}] stretches with nothing to
              start (exhausted: every listed focus is done)
      never:  [{"focus", "why", "list"}] listed focuses that never start before the horizon
      waiting: [{"focus", "why", "list", "on"}] listed focuses that did not start because a prerequisite is a
              focus only the game completes (its own gate is `always = no`: a script finishes it, e.g. on a
              victory), directly or through another waiting focus; when that happens is not in the plan
    """
    done = set(done)
    holds = {k: d for k, d in (holds or {}).items() if d is not None}
    listed = [(k, "path") for k in path] + [(k, "fallback") for k in fallback]
    rows, idle = [], []
    changes = sorted(d for d in set(changes) | set(holds.values()) if start < d < horizon)
    last_why = {}

    def state(k, day):
        """(can start: True / None / False, why not)"""
        info = tree.get(k)
        if info is None:
            return False, "not in the focus tree"
        missing = [" or ".join(sorted(b)) for b in info["prereq"] if not b & done]
        if missing:
            return False, "needs " + " and ".join(missing) + " first"
        hit = sorted(info["exclusive"] & done)
        if hit:
            return False, "mutually exclusive with " + ", ".join(hit)
        if day < holds.get(k, day):
            return False, "held by the plan's focus_when until day %d" % holds[k]
        gate = (meta.get(k) or {}).get("available")
        if not gate:
            return True, None
        facts = facts_at(day, done)
        r = evaluate(gate, facts)
        if r is False:
            return False, "not available: " + "; ".join(blocking(gate, facts) or ["its gate is closed"])
        return r, None

    # a listed focus the game itself works on once its prerequisites are done (its gate is closed for the player
    # in every branch, the open branch only shows a script's in-progress flag: Germany's inner-circle focuses,
    # worked on by the ascended advisor) runs beside the slot for its own cost and takes no slot time
    side = {k for k, _ in listed if worked_by_script(k, meta)}
    side_rows = []

    def side_step(t_now):
        for k in sorted(side):
            if k in done or k not in tree:
                continue
            ends = []
            for block in tree[k]["prereq"]:
                hit = [r["end"] for r in rows + side_rows if r["focus"] in block]
                if not hit and not block & done:
                    break
                ends.append(min(hit) if hit else start)
            else:
                s0 = max(ends) if ends else start
                if s0 <= t_now:
                    dur = max(1.0, float((meta.get(k) or {}).get("cost", 1.0)) * days_per)
                    side_rows.append({"focus": k, "start": s0, "end": s0 + dur, "from": "script", "gate": None})
                    done.add(k)

    t = start
    while t < horizon:
        side_step(t)
        pick = None
        for k, src in listed:
            if k in done or k in side:
                continue
            ok, why = state(k, t)
            if ok is False:
                last_why[k] = why
                continue
            pick = (k, src, ok)
            break
        if pick:
            k, src, ok = pick
            dur = max(1.0, float((meta.get(k) or {}).get("cost", 1.0)) * days_per)
            rows.append({"focus": k, "start": t, "end": t + dur, "from": src, "gate": ok})
            done.add(k)
            t += dur
            continue
        left = [k for k, _ in listed if k not in done and k not in side]
        nxt = next((d for d in changes if d > t), horizon) if left else horizon
        idle.append({"from": t, "to": nxt, "exhausted": not left,
                     "blocked": [{"focus": k, "why": last_why.get(k, "")} for k in left]})
        t = nxt
    side_step(horizon)
    rows = sorted(rows + [r for r in side_rows if r["start"] < horizon], key=lambda r: r["start"])
    stuck = {k: src for k, src in listed if k not in done and k in last_why}
    waiting = {}
    grew = True
    while grew:  # a focus behind a scripted one waits too
        grew = False
        for k in stuck:
            if k in waiting or k not in tree:
                continue
            for block in tree[k]["prereq"]:
                if block & done:
                    continue
                on = sorted(p for p in block if scripted(p, meta) or p in waiting)
                if on:
                    waiting[k] = on
                    grew = True
                    break
    never = [{"focus": k, "list": src, "why": last_why[k]} for k, src in stuck.items() if k not in waiting]
    wait = [{"focus": k, "list": stuck[k], "why": last_why[k], "on": on} for k, on in waiting.items()]
    return {"rows": rows, "idle": idle, "never": never, "waiting": wait}


def worked_by_script(focus, meta):
    """A focus closed to the player whose gate's open branch is a script's in-progress flag (an `if` on
    has_country_flag): the game starts and completes it itself (Germany's inner-circle focuses: the ascended
    advisor works on them for the focus's cost)."""
    if not scripted(focus, meta):
        return False
    gate = (meta.get(focus) or {}).get("available") or []
    for k, v, _ in _pairs(gate):
        if (k or "").lower() == "if" and isinstance(v, list):
            for a, b, _ in _pairs(v):
                if (a or "").lower() == "limit" and "has_country_flag" in repr(b):
                    return True
    return False


def scripted(focus, meta):
    """A focus the player can never start: its `available` gate is closed by `always = no` whatever the
    plan's facts are, so only a script completes it."""
    gate = (meta.get(focus) or {}).get("available")
    if not gate:
        return False
    open_facts = Facts("", (), None)
    return evaluate(gate, open_facts) is False and all("always" in line for line in blocking(gate, open_facts))
