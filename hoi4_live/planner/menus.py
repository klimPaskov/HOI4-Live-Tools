"""Menus with the calculators' numbers, and the default pick of each (docs/ROLLING_PLAN.md D5).

`build(plan, snap, horizon, triggers, facts, ledger)` returns {domain: menu} for the domains the triggers touch
(every domain for a checkpoint, schedule or strategy trigger). A menu is
{"domain", "need", "options": [{"key", ...numbers}], "default": patch or None}. The default pick is what the
planner applies when no model answers in time; the model sees the same options and may choose otherwise.

Ranking rules are general (no country or mod is named):
  research   in-year techs before next-year ones; industry, production and construction folders first, then
             electronics, then land equipment and support, then the rest; naval and air folders only when the
             plan has naval or air work; enough techs to give every slot `target_days` of work
  focus      startable focuses of the plan's own tree (prerequisites done or planned, no exclusive partner
             done or planned), economy and research focuses first, then cheapest
  politics   vacant advisor and high-command slots the plan's order does not cover yet
  production every short equipment family gets a line (or goes first in production_priority when it has one)
  construction more civilian or military factories in the states the plan already builds in, any own state
             as the fallback
  army       victory-point progress against the share needed; when the remaining targets cannot reach it,
             the most valuable enemy states the plan does not list
"""

import math
import re

from . import horizon as hz
from .state import date_text, year_of

DOMAINS = ("research", "focus", "politics", "production", "construction", "army")
FOLDER_RANK = (("industry", 0), ("electronic", 1), ("infantry", 2), ("support", 2), ("artillery", 2),
               ("armou?r", 3), ("land_doctrine", 4))
KEY_RANK = ((r"industr|production|construction|machine_tool|excavation|fuel|oil|synth|rubber|concentrated|dispersed", 0),
            (r"computing|radio|electronic|radar|encryption|decryption", 1),
            (r"infantry|support|artillery|recon|engineer|hospital|logistics|maintenance|signal|motori|mechani|weapons", 2))
SEA = r"naval|ship|hull|carrier|submarine|destroyer|cruiser|battleship|torpedo|convoy|mine"
AIR = r"air|plane|fighter|bomber|cas|aircraft|rocket|jet"
FOCUS_ECONOMY = r"industr|factor|research|construct|steel|infrastructure|railway|highway|econom|tech|resource|oil|synth"
ADVISOR_SLOTS = (("government", (3, 4, 5), 150), ("military_staff", (0, 1, 2, 3, 4, 5), 100))


def _rank_tech(key, folder):
    for pattern, r in FOLDER_RANK:
        if folder and re.search(pattern, folder):
            return r
    for pattern, r in KEY_RANK:
        if re.search(pattern, key):
            return r
    return 5


def _domain_work(plan):
    air = bool(plan.get("air_plan")) or bool((plan.get("air") or {}).get("missions"))
    sea = bool((plan.get("navy") or {}).get("fleets")) or bool(plan.get("invasions"))
    return air, sea


def research(plan, snap, facts, ledger, target_days):
    day = snap.get("day")
    slots = snap.get("slots") or 0
    done = set(snap.get("research_done") or []) | {t for t in snap.get("researching") or [] if t}
    planned = set(hz.planned_techs(plan))
    air, sea = _domain_work(plan)
    costs = facts.costs() if facts else hz.Costs()
    year = year_of(day) if day is not None else None
    have = done | planned
    pool = dict(snap.get("available_techs") or {})
    picks, total = [], 0.0
    need = max(0.0, target_days * max(slots, 1) - (hz.research(plan, snap, costs, hz.LOW)["days"] or 0) * max(slots, 1))
    for _ in range(3):  # the live list, then what the picks and the plan open in the game files
        options = []
        candidates = set(pool)
        if facts:
            candidates |= {t for t in facts.meta if t not in have and not facts.gated(t) and facts.startable(t, have)}
        for t in sorted(candidates - have):
            folder = facts.folder(t) if facts else None
            text = "%s %s" % (t, folder or "")
            if (re.search(SEA, text) and not sea) or (re.search(AIR, text) and not air):
                continue
            if ledger and ledger.failed(t, day):
                continue
            y = pool.get(t) if pool.get(t) is not None else (facts.tech_year(t) if facts else None)
            ahead = 0 if hz.in_time(y, day) else (y - year if year and y else 1)
            if ahead > 1:
                continue
            options.append({"key": t, "year": y, "days": round(costs.tech(t)), "folder": folder,
                            "rank": (ahead, _rank_tech(t, folder), y or 0, costs.tech(t))})
        options.sort(key=lambda o: o["rank"])
        added = False
        for o in options:
            if total >= need:
                break
            picks.append(o)
            have.add(o["key"])
            total += o["days"]
            added = True
        if total >= need or not added or not facts:
            break
    menu = [dict({k: v for k, v in o.items() if k != "rank"}, in_time=o["rank"][0] == 0) for o in picks]
    default = None
    if picks:
        default = {"summary": "research: %d techs added (%.0f base days)" % (len(picks), total),
                   "ops": [{"op": "extend", "key": "research_path", "items": [{
                       "year": year, "not_before": date_text(day), "techs": [o["key"] for o in picks],
                       "why": "rolling update %s: in-time techs, industry first, to keep %d slots busy %d days"
                              % (date_text(day), slots, target_days)}]}]}
    return {"domain": "research", "need": "%.0f slot-days" % need, "options": menu, "default": default}


def focus(plan, snap, facts, ledger, target_days):
    day = snap.get("day")
    done = set(snap.get("focuses_done") or []) | ({snap["focus"]} if snap.get("focus") else set())
    planned = hz.planned_focuses(plan)
    states = snap.get("focus_states") or {}
    options = []
    if facts:
        tree = facts.focus_tree(planned + sorted(done))
        have = done | set(planned)
        for key, row in sorted(tree.items()):
            if key in have or states.get(key) == "unavailable" or (ledger and ledger.failed(key, day)):
                continue
            if any(not (set(group) & have) for group in row.get("prereq") or [] if group):
                continue
            if set(row.get("exclusive") or ()) & have:
                continue
            econ = 0 if re.search(FOCUS_ECONOMY, key.lower()) else 1
            days = facts.focus_days(key, tree) or hz.FOCUS_DAYS
            options.append({"key": key, "days": round(days), "economy": econ == 0,
                            "available_now": states.get(key) == "available", "rank": (econ, days)})
    else:
        options = [{"key": k, "days": round(hz.FOCUS_DAYS), "available_now": True, "rank": (0, 0)}
                   for k, v in sorted(states.items()) if v == "available" and k not in done and k not in planned]
    options.sort(key=lambda o: o["rank"])
    picks, total = [], 0.0
    for o in options:
        if total >= target_days:
            break
        picks.append(o)
        total += o["days"]
    default = None
    if picks:
        default = {"summary": "focus: %d focuses added (%.0f days)" % (len(picks), total),
                   "ops": [{"op": "extend", "key": "focus_path", "items": [o["key"] for o in picks]}]}
    return {"domain": "focus", "need": "%d days" % target_days,
            "options": [{k: v for k, v in o.items() if k != "rank"} for o in options[:15]], "default": default}


def politics(plan, snap, facts, ledger):
    auto = plan.get("politics_auto") if isinstance(plan.get("politics_auto"), dict) else {}
    order = [e for e in auto.get("order") or [] if isinstance(e, dict)]
    covered = {(e.get("category"), e.get("slot")) for e in order + [p for p in plan.get("politics") or []
                                                                      if isinstance(p, dict)]}
    options = []
    for category, slots, cost in ADVISOR_SLOTS:
        for slot in slots:
            if (category, slot) not in covered:
                options.append({"category": category, "slot": slot, "cost": cost,
                                "why": "rolling update: a vacant slot takes the first idea the game offers"})
    default = None
    if options:
        default = {"summary": "politics: %d vacant slots added to politics_auto.order" % len(options),
                   "ops": [{"op": "set", "key": "politics_auto.order", "value": order + options}]}
    return {"domain": "politics", "need": "a use for %s PP (gain %s/day)" % (snap.get("pp"), snap.get("pp_gain")),
            "options": options, "default": default}


def _variant(family, snap, facts):
    """The equipment id a new line of `family` builds: the newest variant the researched techs unlock."""
    done = set(snap.get("research_done") or [])
    if facts:
        unlocks = facts.derive.equipment_unlocks(facts.game)
        best = None
        for eq, techs in unlocks.items():
            if re.fullmatch(re.escape(family) + r"_\d+", eq) and (techs & done or not techs):
                if best is None or int(eq.rsplit("_", 1)[1]) > int(best.rsplit("_", 1)[1]):
                    best = eq
        if best:
            return best
    lines = [eq for eq in snap.get("lines") or {} if eq.startswith(family)]
    return sorted(lines)[-1] if lines else family + "_1"


def production(plan, snap, facts, ledger, h):
    short = dict((h.get("short") or {}))
    for army in (h.get("fill_low") or {}).values():
        for fam in army.get("short") or []:
            short.setdefault(fam, {"days_of_cover": None})
    lines = snap.get("lines") or {}
    entries = {e.get("equipment") for e in plan.get("production") or [] if isinstance(e, dict) and not e.get("remove")}
    priority = [p for p in plan.get("production_priority") or [] if isinstance(p, str)]
    step = int((plan.get("manage") or {}).get("rebalance_step") or 3)
    options, ops, added = [], [], []
    for fam in sorted(short):
        has_line = any(eq.startswith(fam) for eq in lines) or any((e or "").startswith(fam) for e in entries)
        variant = _variant(fam, snap, facts)
        options.append({"family": fam, "days_of_cover": short[fam].get("days_of_cover"), "line": has_line,
                        "variant": variant})
        if not has_line and not (ledger and ledger.failed(variant, snap.get("day"))):
            # added once by the executor from the factories idle now: `from` today, so the planning tools count
            # it from now and not from the scenario start
            item = {"equipment": variant, "factories": step,
                    "why": "rolling update: %s reported short and no line builds it" % fam}
            if snap.get("date"):
                item["from"] = str(snap["date"])[:10]
            added.append(item)
        if fam not in priority[:1]:
            priority = [fam] + [p for p in priority if p != fam]
    ops += [{"op": "add", "key": "production", "item": item} for item in added]
    if priority != (plan.get("production_priority") or []):
        ops.append({"op": "set", "key": "production_priority", "value": priority})
    default = {"summary": "production: " + ", ".join(sorted(short)), "ops": ops} if ops else None
    return {"domain": "production", "need": "lines for %s" % ", ".join(sorted(short)), "options": options,
            "default": default}


def construction(plan, snap, facts, ledger):
    entries = [e for e in plan.get("construction") or [] if isinstance(e, dict)]
    states = sorted({s for e in entries for s in e.get("states") or []}, key=str)
    at_war = bool(snap.get("wars"))
    building = "arms_factory" if at_war else "industrial_complex"
    failed = ledger and ledger.failed("construction_add", snap.get("day")) and ledger.failed(building, snap.get("day"))
    if failed:
        building = "industrial_complex" if building == "arms_factory" else "infrastructure"
    item = {"building": building, "states": states, "count": 2,
            "why": "rolling update: %s civilian factories idle" % snap.get("civ_idle")}
    same = any(e.get("building") == building and sorted(e.get("states") or [], key=str) == states for e in entries)
    ops = [{"op": "extend", "key": "construction", "items": [item]}] if states and not same else []
    if plan.get("construction_fallback_any_own_state") is not True:
        ops.append({"op": "set", "key": "construction_fallback_any_own_state", "value": True})
    if not ops:
        return {"domain": "construction", "need": "work for idle civilian factories",
                "options": [{"building": building, "states": states, "already_planned": True}], "default": None}
    return {"domain": "construction", "need": "work for idle civilian factories",
            "options": [{"building": building, "states": states}],
            "default": {"summary": "construction: %s, any own state as fallback" % building, "ops": ops}}


def _capitulation_needed(plan, total):
    """(VP needed, how it was set): the plan's capitulation aim when it has one, else a share of the enemy's VP."""
    aim = ((plan.get("goal") or {}).get("capitulation") or {}) if isinstance(plan.get("goal"), dict) else {}
    if isinstance(aim.get("vp_needed"), (int, float)) and aim["vp_needed"] > 0:
        return int(aim["vp_needed"]), "the plan's capitulation aim"
    share = (plan.get("war") or {}).get("capitulation_share")
    if not total:
        return None, ""
    if share:
        return math.ceil(total * float(share)), "share %.2f of %.0f enemy VP" % (float(share), total)
    return math.ceil(total * 0.8), "assumed share 0.80 of %.0f enemy VP" % total


def _difficulty(sid, snap, tag):
    """Map distance from the nearest state the player controls (state centres); 0 when unknown."""
    centers = snap.get("centers") or {}
    here = centers.get(sid)
    if not here:
        return 0.0
    mine = [c for s, c in centers.items() if (snap.get("controllers") or {}).get(int(s)) == tag
            or (snap.get("controllers") or {}).get(s) == tag]
    if not mine:
        return 0.0
    return min(math.hypot(here[0] - c[0], here[1] - c[1]) for c in mine)


def army(plan, snap, facts, ledger):
    """The victory-point target set re-evaluated from the live table: the cheapest set of enemy states still
    reaching the capitulation, ordered by value against difficulty, with progress "held X of the Y needed".
    Targets already taken drop out of the count, a moved threshold (the plan's aim revised) changes the set, and
    listed targets the set no longer needs are named so a strategy update can drop them."""
    tag = snap.get("player")
    enemies = set(snap.get("wars") or (plan.get("war") or {}).get("enemies") or [])
    table = snap.get("vp") or {}
    total = held = 0.0
    open_states = []
    for sid, row in table.items():
        if row.get("owner") not in enemies:
            continue
        vp_state = sum(float(p.get("vp") or 0) for p in row.get("provinces") or [])
        mine = sum(float(p.get("vp") or 0) for p in row.get("provinces") or [] if p.get("controller") == tag)
        total += vp_state
        held += mine
        if vp_state - mine > 0:
            open_states.append({"state": int(sid), "name": row.get("name"), "vp_open": vp_state - mine,
                                "distance": round(_difficulty(sid, snap, tag))})
    needed, basis = _capitulation_needed(plan, total)
    listed = {str(t.get("state")) for t in plan.get("vp_targets") or [] if isinstance(t, dict)}
    scale = max([o["distance"] for o in open_states] + [1.0])
    for o in open_states:
        o["listed"] = str(o["state"]) in listed
        o["value_per_difficulty"] = round(o["vp_open"] / (1.0 + o["distance"] / scale), 2)
    open_states.sort(key=lambda o: (-o["value_per_difficulty"], -o["vp_open"], o["state"]))
    chosen, got = [], held
    if needed is not None:
        # the listed targets first (the opening chose them), then the cheapest others
        for o in [o for o in open_states if o["listed"]] + [o for o in open_states if not o["listed"]]:
            if got >= needed:
                break
            chosen.append(o)
            got += o["vp_open"]
    extra = [o for o in chosen if not o["listed"]]
    spare = [o["state"] for o in open_states if o["listed"] and o not in chosen]
    default = None
    if extra:
        default = {"summary": "army: %d targets added to reach the capitulation (%s)" % (len(extra), basis),
                   "ops": [{"op": "extend", "key": "vp_targets", "items": [
                       {"state": o["state"], "why": "rolling update: held %.0f of the %d needed; the listed targets "
                        "fall short" % (held, needed)} for o in extra]}]}
    progress = "held %.0f of the %s needed (%s)" % (held, needed, basis or "no enemy VP read yet")
    menu = {"domain": "army", "need": progress, "options": open_states[:12], "default": default,
            "progress": progress}
    if spare:
        menu["not_needed_now"] = spare
    return menu


# A domain's default pick that failed the gates waits this long before it is offered again (the model or the
# player's agent may still pick from the menu).
WITHHOLD_DAYS = 30


def domains_for(triggers):
    out = set()
    for t in triggers:
        if t.get("domain") in DOMAINS:
            out.add(t["domain"])
        elif t["kind"] in ("checkpoint_due", "schedule", "war_changed", "goal_at_risk", "player_request",
                           "contingency", "deviation"):
            out.update(DOMAINS)
        elif t["kind"] == "blocked":
            out.add(t.get("domain") or "construction")
    return [d for d in DOMAINS if d in out]


def build(plan, snap, h, triggers, facts=None, ledger=None, research_days=120, focus_days=90):
    """{domain: menu} for the domains the triggers touch; a domain whose queue is not low gets its menu
    (the model may use it) but no default pick unless a trigger names that domain."""
    named = {t.get("domain") for t in triggers if t["kind"] == "horizon_low"}
    out = {}
    for d in domains_for(triggers):
        if d == "research":
            m = research(plan, snap, facts, ledger, research_days)
        elif d == "focus":
            m = focus(plan, snap, facts, ledger, focus_days)
        elif d == "politics":
            m = politics(plan, snap, facts, ledger)
        elif d == "production":
            m = production(plan, snap, facts, ledger, h.get("production") or {})
        elif d == "construction":
            m = construction(plan, snap, facts, ledger)
        else:
            m = army(plan, snap, facts, ledger)
        if d not in named and not (h.get(d) or {}).get("low"):
            m["default"] = None if d not in ("research", "focus") or not m.get("default") else m["default"]
            if d in ("research", "focus") and (h.get(d) or {}).get("days", 0) and h[d]["days"] >= (
                    research_days if d == "research" else focus_days):
                m["default"] = None
        if m.get("default") and ledger is not None and ledger.failed("default:" + d, snap.get("day"),
                                                                     cooldown=WITHHOLD_DAYS):
            m["default_withheld"] = m.pop("default")["summary"] + " (failed the gates in the last %d days)" % (
                WITHHOLD_DAYS)
            m["default"] = None
        out[d] = m
    return out
