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
    land unless it says otherwise), `tag`, `dlcs`."""

    def __init__(self, tag, dlcs, date, war=None, enemies=(), focuses=(), flags=(), states=()):
        self.ctx = Ctx(tag, dlcs, date)
        self.war = war
        self.enemies = set(enemies)
        self.focuses = set(focuses)
        self.flags = set(flags)
        self.states = set(states)


def evaluate(block, facts):
    """True / False / None of a trigger block (implicit AND) under the plan's facts."""
    if not isinstance(block, list):
        return None
    return _and(_one(k, v, op, facts) for k, v, op in _pairs(block))


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
        return _and(_one(a, b, o, facts) for a, b, o in _pairs(value) if (a or "").lower() not in WRAPPER_TEXT_KEYS)
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
    for k, v, op in _pairs(block):
        if _one(k, v, op, facts) is False:
            kl = (k or "").lower()
            if kl in WRAPPERS + ("and",) and isinstance(v, list):
                inner = blocking([(a, b) for a, b in v if (a or "").lower() not in WRAPPER_TEXT_KEYS], facts)
                out += inner or [render([(k, v)], 120)]
            else:
                out.append(render([(k, v)], 120) if isinstance(v, list) else f"{k} {op} {v}")
    return out


def focus_schedule(path, fallback, tree, meta, days_per, start, horizon, facts_at, done=(), changes=()):
    """Walk the focus slot from day `start` to `horizon` (day numbers).

    path / fallback: focus ids in plan order; tree: {focus: {"prereq": [set], "exclusive": set}}; meta:
    {focus: {"available": block or None, "cost": float}}; facts_at(day, completed) -> Facts; changes: days on
    which the plan's conditions change (a war starts or ends), so a blocked slot is tried again then.

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
    listed = [(k, "path") for k in path] + [(k, "fallback") for k in fallback]
    rows, idle = [], []
    changes = sorted(d for d in set(changes) if start < d < horizon)
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
        gate = (meta.get(k) or {}).get("available")
        if not gate:
            return True, None
        facts = facts_at(day, done)
        r = evaluate(gate, facts)
        if r is False:
            return False, "not available: " + "; ".join(blocking(gate, facts) or ["its gate is closed"])
        return r, None

    t = start
    while t < horizon:
        pick = None
        for k, src in listed:
            if k in done:
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
        left = [k for k, _ in listed if k not in done]
        nxt = next((d for d in changes if d > t), horizon) if left else horizon
        idle.append({"from": t, "to": nxt, "exhausted": not left,
                     "blocked": [{"focus": k, "why": last_why.get(k, "")} for k in left]})
        t = nxt
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


def scripted(focus, meta):
    """A focus the player can never start: its `available` gate is closed by `always = no` whatever the
    plan's facts are, so only a script completes it."""
    gate = (meta.get(focus) or {}).get("available")
    if not gate:
        return False
    open_facts = Facts("", (), None)
    return evaluate(gate, open_facts) is False and all("always" in line for line in blocking(gate, open_facts))
