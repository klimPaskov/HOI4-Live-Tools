"""The decision points of hoi4_advance (phase 5): which changes between two telemetry days make the agent stop
running time, and which tool calls answer an attention item. Pure functions of telemetry day records and briefing
attention items, so they are unit tested without the game.

A day record is the telemetry mod's `day` line (see telemetry/modgen.py); `watch()` reduces it to the fields the
stop conditions look at.
"""

STOP_CONDITIONS = {
    "tech_done": "a technology was researched (a research slot is free again)",
    "focus_idle": "requires verified UI idle evidence; campaign fast-clock callers should use focus governor preflight",
    "research_idle": "a research slot is idle (more slots than technologies in progress; from the telemetry)",
    "focus_done": "a national focus was completed (closed own-country telemetry in campaigns)",
    "construction_done": "civilian factories became free (a construction finished or the queue ran dry)",
    "production_idle": "military factories became free (a production line ended or was removed)",
    "war_started": "the country is at war with more countries than before",
    "war_ended": "the country is at war with fewer countries than before",
    "territory_lost": "fewer states are controlled than before",
    "territory_gained": "more states are controlled than before",
    "losing": "surrender progress crossed the threshold (default 20%)",
    "low_stability": "stability fell below the threshold (default 40%)",
    "pp_high": "political power reached the threshold (default 150)",
    "low_fuel": "fuel fell below 10% of the capacity",
    "front_changed": "the number of enemy-held states touching ours changed",
    "peace_conference": "a war was won and the peace conference is open (always reported; hoi4_peace)",
}
DEFAULT_STOP = ["tech_done", "focus_done", "construction_done", "production_idle", "war_started", "war_ended",
                "territory_lost", "losing"]
DEFAULTS = {"losing": 0.2, "low_stability": 0.4, "pp_high": 150.0, "low_fuel": 0.1}


def watch(day):
    """The fields stop conditions compare, from a telemetry `day` record (None when there is no record)."""
    if not day:
        return None
    g = day.get
    return {"date": day.get("_date"), "n": g("n"), "pp": g("pp"), "stability": g("stab"), "war_support": g("ws"),
            "civ_free": g("civ_free"), "mil_free": g("mil_free"), "nav_free": g("nav_free"),
            "enemies": g("enemies") or 0, "surrender": g("surrender"), "techs": g("techs"),
            "controlled": g("states_ctrl"), "owned": g("states"), "front": g("front"), "fuel_ratio": g("fuel_ratio"),
            "divisions": g("div"), "casualties_k": g("cas_k")}


def _num(v):
    return v if isinstance(v, (int, float)) and not isinstance(v, bool) else None


def evaluate(prev, cur, stop_on, thresholds=None, notices=()):
    """Reasons to stop between two watch dicts: [{"code", "text"}]. Level conditions (losing, low_stability,
    pp_high, low_fuel) fire when the value crosses the threshold, not while it stays beyond it."""
    th = dict(DEFAULTS)
    th.update({k: v for k, v in (thresholds or {}).items() if v is not None})
    out = []

    def hit(code, text):
        if code in stop_on:
            out.append({"code": code, "text": text})
    for title in notices:
        if "focus_done" in stop_on and "focus" in (title or "").lower():
            out.append({"code": "focus_done", "text": f"notice: {title}"})
    if not prev or not cur:
        return out

    def rose(k):
        a, b = _num(prev.get(k)), _num(cur.get(k))
        return a is not None and b is not None and b > a

    def fell(k):
        a, b = _num(prev.get(k)), _num(cur.get(k))
        return a is not None and b is not None and b < a

    def crossed_up(k, limit):
        a, b = _num(prev.get(k)), _num(cur.get(k))
        return a is not None and b is not None and a < limit <= b

    def crossed_down(k, limit):
        a, b = _num(prev.get(k)), _num(cur.get(k))
        return a is not None and b is not None and a >= limit > b
    if rose("techs"):
        hit("tech_done", f"technologies {prev['techs']} -> {cur['techs']}")
    if rose("civ_free"):
        hit("construction_done", f"free civilian factories {prev['civ_free']} -> {cur['civ_free']}")
    if rose("mil_free"):
        hit("production_idle", f"free military factories {prev['mil_free']} -> {cur['mil_free']}")
    if rose("enemies"):
        hit("war_started", f"enemies {prev['enemies']} -> {cur['enemies']}")
    if fell("enemies"):
        hit("war_ended", f"enemies {prev['enemies']} -> {cur['enemies']}")
    if fell("controlled"):
        hit("territory_lost", f"controlled states {prev['controlled']} -> {cur['controlled']}")
    if rose("controlled"):
        hit("territory_gained", f"controlled states {prev['controlled']} -> {cur['controlled']}")
    if crossed_up("surrender", th["losing"]):
        hit("losing", f"surrender progress {prev['surrender']:.0%} -> {cur['surrender']:.0%}")
    if crossed_down("stability", th["low_stability"]):
        hit("low_stability", f"stability {prev['stability']:.0%} -> {cur['stability']:.0%}")
    if crossed_up("pp", th["pp_high"]):
        hit("pp_high", f"political power {prev['pp']:.0f} -> {cur['pp']:.0f}")
    if crossed_down("fuel_ratio", th["low_fuel"]):
        hit("low_fuel", f"fuel {prev['fuel_ratio']:.0%} -> {cur['fuel_ratio']:.0%}")
    a, b = _num(prev.get("front")), _num(cur.get("front"))
    if a is not None and b is not None and a != b:
        hit("front_changed", f"enemy-held states touching ours {int(a)} -> {int(b)}")
    return out


def diff(first, last):
    """Changes between two watch dicts, for the report: {"field": [from, to]} of fields that moved."""
    out = {}
    if not first or not last:
        return out
    for k in ("pp", "stability", "war_support", "civ_free", "mil_free", "nav_free", "enemies", "surrender",
              "techs", "controlled", "owned", "front", "divisions", "casualties_k"):
        a, b = first.get(k), last.get(k)
        if a is None or b is None or a == b:
            continue
        if isinstance(a, float) or isinstance(b, float):
            if abs(b - a) < 0.005:
                continue
            a, b = round(a, 3), round(b, 3)
        out[k] = [a, b]
    return out


# ---------------------------------------------------------------------------------------------- suggestions
# attention code -> (tool, arguments, what to do with the result)
SUGGEST = {
    "peace_conference": ("hoi4_peace", {"action": "status"}, "take the states or demands, submit, exit"),
    "popups": ("hoi4_popups", {"action": "read"}, "read the open event windows, then answer one by option"),
    "no_focus": ("hoi4_focus", {"action": "list", "status": "available"},
                 "pick a focus that fits the plan, then hoi4_focus action=select"),
    "idle_research": ("hoi4_research", {"action": "list", "ahead": False},
                      "start a technology in each idle slot (hoi4_research action=start)"),
    "idle_construction": ("hoi4_construction", {"action": "queue"},
                          "queue civilian or military factories in states with free slots"),
    "idle_production": ("hoi4_production", {"action": "lines"},
                        "add or enlarge a production line for the equipment the divisions lack"),
    "idle_dockyards": ("hoi4_production", {"action": "lines"}, "start a ship line or leave the dockyards idle"),
    "spend_pp": ("hoi4_politics", {"action": "view"}, "advisors, laws, or decisions (hoi4_decisions)"),
    "equipment_short": ("hoi4_production", {"action": "lines"}, "put factories on the missing equipment"),
    "resource_deficit": ("hoi4_diplomacy", {"action": "trade"}, "trade for the missing resources"),
    "low_stability": ("hoi4_politics", {"action": "view"}, "advisors, laws or decisions that raise stability"),
    "low_fuel": ("hoi4_production", {"action": "lines"}, "synthetic refineries, trade, or fewer armoured units"),
    "losing_war": ("hoi4_army", {"action": "list"}, "shorten the front: front lines on defensible terrain, reserves"),
    "lost_states": ("hoi4_orders", {"action": "status"}, "plan an offensive to retake them"),
    "front": ("hoi4_army", {"action": "list"}, "armies with a front line along the border"),
    "mission_ending": ("hoi4_decisions", {"action": "list"}, "finish the mission or let it lapse"),
    "election": ("hoi4_politics", {"action": "view"}, "nothing to do unless a law change is wanted first"),
}


def suggestions(attention, limit=8):
    """[{"level", "why", "tool", "arguments", "then"}] for attention items that have a code, most urgent first."""
    out = []
    seen = set()
    for item in attention:
        code = item.get("code")
        if code not in SUGGEST or code in seen:
            continue
        seen.add(code)
        tool, args, then = SUGGEST[code]
        out.append({"level": item["level"], "why": item["text"], "tool": tool, "arguments": dict(args),
                    "then": then})
        if len(out) >= limit:
            break
    return out


def telemetry_attention(w):
    """Attention items (same shape as briefing.attention) from one watch dict alone, for when no save is read."""
    items = []
    if not w:
        return items

    def add(level, text, code, **data):
        d = {"level": level, "text": text, "code": code}
        d.update(data)
        items.append(d)
    if (w.get("civ_free") or 0) >= 1:
        add("soon", f"{w['civ_free']} civilian factories are free (construction or trade may not use them)",
            "idle_construction", free=w["civ_free"])
    if (w.get("mil_free") or 0) >= 1:
        add("now", f"{w['mil_free']} military factories not producing anything", "idle_production",
            free=w["mil_free"])
    if (w.get("nav_free") or 0) >= 1:
        add("info", f"{w['nav_free']} dockyards not producing anything", "idle_dockyards", free=w["nav_free"])
    if (w.get("enemies") or 0) and (w.get("surrender") or 0) >= DEFAULTS["losing"]:
        add("now", f"surrender progress {w['surrender']:.0%}: the war is going badly", "losing_war")
    if w.get("stability") is not None and w["stability"] < DEFAULTS["low_stability"]:
        add("soon", f"stability is low ({w['stability']:.0%})", "low_stability")
    if w.get("pp") is not None and w["pp"] >= 100:
        add("soon", f"political power {w['pp']:.0f}: spend it (advisors, laws, decisions)", "spend_pp")
    if w.get("front"):
        add("info", f"front: {int(w['front'])} enemy-held states touch ours", "front")
    order = {"now": 0, "soon": 1, "info": 2}
    items.sort(key=lambda i: order[i["level"]])
    return items
