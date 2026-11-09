"""Technologies with folders and DLC gates; a research calendar and a research timeline.

    techs(world)                 {tech: {cost, year, folders, parents, deps, xor, categories, allow,
                                 src}} for the techs whose folder is available with the active DLCs
                                 (common/technology_tags `technology_folders` `available`, evaluated
                                 statically) and whose `allow` is not statically false
    days(world, tech, date)      estimated days in one slot: research_cost x NTechnology.BASE_TECH_COST,
                                 times (1 + BASE_YEAR_AHEAD_PENALTY_FACTOR x years ahead); no research
                                 speed bonuses (label "d" says so)
    chain(world, tech, done, rule)   the missing technologies needed before `tech` can start,
                                 under rule "all" (every parent) or "any" (one parent; the cheapest)
    calendar(world, tag, date, years)   per year: the techs of that start year, the chain from the
                                 start techs under both readings where they differ
    timeline(world, plan)        a plan's research_path (or research_priority) run through N slots

Both readings of multi-parent techs are reported because the live rule is not verified yet (tech.py).
"""
import json

from . import tech as techrule
from .script import bare, get, get1, line_of, parse_file, trigger
from .world import day_number, num


def folders(world):
    def load():
        out = {}
        for p in world.game.files("common/technology_tags"):
            for k, v in parse_file(p):
                if k != "technology_folders" or not isinstance(v, list):
                    continue
                for j, (name, body) in enumerate(v):
                    if not name:
                        continue
                    av = get1(body, "available") if isinstance(body, list) else None
                    ok = trigger(av, world.ctx) if isinstance(av, list) else True
                    out[name] = {"available": ok, "src": world.src(p, line_of(v, j))}
        return out

    return world.cached("techfolders", load)


def techs(world, all_folders=False):
    key = "techs:all" if all_folders else "techs"
    return world.cached(key, lambda: _techs(world, all_folders))


def _techs(world, all_folders):
    fold = folders(world)
    raw = {}
    leads = {}
    for p in world.game.files("common/technologies"):
        for k, v in parse_file(p):
            if k != "technologies" or not isinstance(v, list):
                continue
            for j, (name, body) in enumerate(v):
                if not name or not isinstance(body, list) or name.startswith("@"):
                    continue
                fl = [get1(f, "name") for f in get(body, "folder") if isinstance(f, list)]
                deps = get1(body, "dependencies")
                xor = []
                for x in get(body, "XOR") + get(body, "xor"):
                    if isinstance(x, list):
                        xor += bare(x)
                allow = get1(body, "allow")
                raw[name] = {"id": name, "cost": num(get1(body, "research_cost"), 1),
                             "year": num(get1(body, "start_year"), 0), "folders": [f for f in fl if f],
                             "deps": sorted(a for a, _ in deps) if isinstance(deps, list) else [],
                             "xor": sorted(set(xor)),
                             "categories": bare(get1(body, "categories", [])),
                             "doctrine": get1(body, "doctrine") == "yes",
                             "allow": world.text(allow, 160) if isinstance(allow, list) else None,
                             "allow_static": trigger(allow, world.ctx) if isinstance(allow, list) else True,
                             "src": world.src(p, line_of(v, j)), "mod": world.content.source(p)}
                for path in get(body, "path"):
                    if isinstance(path, list) and isinstance(get1(path, "leads_to_tech"), str):
                        leads.setdefault(get1(path, "leads_to_tech"), set()).add(name)
    out = {}
    for name, t in raw.items():
        t["parents"] = sorted(leads.get(name, ()))
        if not all_folders:
            if t["folders"] and not any(fold.get(f, {}).get("available", True) is not False for f in t["folders"]):
                continue
            if t["allow_static"] is False:
                continue
        out[name] = t
    return out


def start_techs(world, tag=None):
    return set(world.country(tag or world.tag)["techs"])


def days(world, tid, date=None, t=None):
    t = t or techs(world).get(tid)
    if not t:
        return None
    base, _ = world.define("BASE_TECH_COST", 100)
    pen, _ = world.define("BASE_YEAR_AHEAD_PENALTY_FACTOR", 2)
    year = int(str(date or world.date).split(".")[0])
    ahead = max(0, (t["year"] or 0) - year)
    return round(t["cost"] * base * (1 + pen * ahead), 1)


DAYS_METHOD = ("research_cost x NTechnology.BASE_TECH_COST x (1 + BASE_YEAR_AHEAD_PENALTY_FACTOR x years ahead "
               "of start_year); one slot, no research speed bonuses")


def chain(world, tid, done, rule="all"):
    """Sorted missing technologies needed before tid can start (tid excluded)."""
    ts = techs(world)
    memo = {}

    def need(x, stack=()):
        if x in done:
            return set()
        if x in memo:
            return memo[x]
        if x in stack or x not in ts:
            return set()
        t = ts[x]
        req = set()
        for d in t["deps"]:
            if d not in done:
                req |= {d} | need(d, stack + (x,))
        parents = [p for p in t["parents"] if p in ts]
        if parents:
            if rule == "all":
                for p in parents:
                    if p not in done:
                        req |= {p} | need(p, stack + (x,))
            elif not any(p in done for p in parents):
                best = min(parents, key=lambda p: (len(need(p, stack + (x,))), p))
                req |= {best} | need(best, stack + (x,))
        memo[x] = req
        return req

    return sorted(need(tid))


def calendar(world, tag=None, date=None, years=(1936, 1938), limit_per_year=None):
    """[{year, techs: [{id, folder, cost, days, startable, chain_all, chain_any}]}]."""
    tag = tag or world.tag
    done = start_techs(world, tag)
    ts = techs(world)
    out = []
    for y in range(years[0], years[1] + 1):
        rows = []
        for tid, t in sorted(ts.items(), key=lambda kv: ((kv[1]["folders"] or [""])[0], kv[0])):
            if t["year"] != y or tid in done or t["doctrine"] or not t["folders"]:
                continue
            ca = chain(world, tid, done, "all")
            cn = chain(world, tid, done, "any")
            row = {"id": tid, "name": world.name(tid), "folder": (t["folders"] or [None])[0], "cost": t["cost"],
                   "days": days(world, tid, f"{y}.1.1", t), "days_at_start": days(world, tid, date or world.date, t),
                   "startable": not ca,
                   "chain_all": ca, "src": t["src"]}
            if cn != ca:
                row["chain_any"] = cn
            if t["xor"]:
                row["xor"] = t["xor"]
            if t["allow"]:
                row["allow"] = t["allow"]
            rows.append(row)
        if limit_per_year:
            rows = rows[:limit_per_year]
        out.append({"year": y, "techs": rows, "d": DAYS_METHOD})
    return out


def plan_order(plan):
    """[(tech, not_before)] of a plan: research_path entries in order, else research_priority."""
    out = []
    for e in plan.get("research_path") or []:
        if not isinstance(e, dict):
            continue
        nb = e.get("not_before") or (f"{e['year']}.1.1" if e.get("year") else None)
        for t in e.get("techs") or []:
            if isinstance(t, str):
                out.append((t, nb))
    if not out:
        out = [(t, None) for t in plan.get("research_priority") or [] if isinstance(t, str)]
    return out


def timeline(world, plan, rule="all", slots=None, date=None):
    """Simulate a plan's research order in N slots: [{id, slot, start, end, days, ahead, flags}].
    A free slot takes the first remaining tech whose not_before has passed and whose prerequisites
    are researched (rule "all" or "any"); it waits when none can start."""
    meta = plan.get("_validator") or {}
    tag = meta.get("tag") or world.tag
    date = date or meta.get("start_date") or world.date
    if slots is None:
        s = world.country(tag)["research_slots"]
        slots = s["v"] if s else 2
    ts = techs(world)
    done = set(start_techs(world, tag))
    queue = plan_order(plan)
    rows, unknown = [], []
    for tid, _ in queue:
        if tid not in ts:
            unknown.append(tid)
    queue = [(t, nb) for t, nb in queue if t in ts and t not in done]
    start_day = day_number(date)
    free = [start_day] * slots  # day each slot becomes free
    running = []  # (end day, slot, tech)
    now = start_day
    guard = 0
    while queue and guard < 10000:
        guard += 1
        # finish everything that ends by now
        for end, slot, tid in sorted(running):
            if end <= now:
                done.add(tid)
        running = [r for r in running if r[0] > now]
        busy = {r[1] for r in running}
        idle = [i for i in range(slots) if i not in busy]
        started = False
        for slot in idle:
            pick = None
            for i, (tid, nb) in enumerate(queue):
                if nb and day_number(nb) > now:
                    continue
                if techrule.missing(ts[tid], done, rule):
                    continue
                pick = i
                break
            if pick is None:
                break
            tid, nb = queue.pop(pick)
            d = days(world, tid, _date_of(now), ts[tid])
            ahead = max(0, ts[tid]["year"] - int(_date_of(now).split(".")[0]))
            running.append((now + int(round(d)), slot, tid))
            rows.append({"id": tid, "slot": slot + 1, "start": _date_of(now), "end": _date_of(now + int(round(d))),
                         "days": d, "ahead_years": ahead})
            started = True
        if started:
            continue
        # advance to the next event: a slot finishing or a not_before date
        events = [r[0] for r in running]
        events += [day_number(nb) for _, nb in queue if nb and day_number(nb) > now]
        if not events:
            break  # nothing can ever start: prerequisites missing from the plan
        now = min(events)
    blocked = []
    for tid, nb in queue:
        blocked.append({"id": tid, "missing": techrule.missing(ts[tid], done, rule)})
    return {"rows": rows, "blocked": blocked, "unknown": unknown, "slots": slots, "rule": rule,
            "d": DAYS_METHOD + "; sequential per slot"}


def _date_of(n):
    from .world import from_day
    return from_day(n)


def load_plan(path):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)
