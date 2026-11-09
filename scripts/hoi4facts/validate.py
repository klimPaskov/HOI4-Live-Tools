"""Plan checks against the effective game content (scripts/validate_plan.py is the CLI).

validate(plan, game) -> (Report, counts). Errors are things the executor (native/src/plan_run.rs)
cannot do or will do wrongly; warnings are legal but doubtful choices, or player-only text the
executor never reads. See validate_plan.py for the list of checks. Coverage-build checks (W1-2):
every predicate the schema types (definitions.predicate) parses with parse_predicate (the x-grammar /
x-atoms of the schema, as native/src/plan_predicate.rs reads them) and names known provinces, states,
tags, focuses, techs, phases, equipment and missions; templates (units in the right rows, one group a
column, width <= 40, regimental supports under 3-battalion columns); production remove/replace; training
lists; stockpile_targets, xp_budget refs, force_posture phases and sums, occupation laws, peace state ids,
sea_transfer states, event_rules fallback_index.
"""
import json
import os
import re
from collections import defaultdict

from . import derive, schema_check
from . import tech as techrule
from .script import DEFAULT_START, Ctx, date_tuple

ROLES = ("strike", "guard")
RAILWAY = ("rail_way", "railway")


class Report:
    def __init__(self):
        self.errors = []
        self.warnings = []

    def error(self, msg):
        self.errors.append(msg)

    def warn(self, msg):
        self.warnings.append(msg)


def strings(plan, key):
    v = plan.get(key, [])
    return [x for x in v if isinstance(x, str)] if isinstance(v, list) else []


def dup_check(rep, label, keys):
    seen = set()
    for k in keys:
        if k in seen:
            rep.error(f"{label}: duplicate key {k}")
        seen.add(k)


def date_year(text, default):
    m = re.match(r"(\d{4})", str(text or ""))
    return int(m.group(1)) if m else default


def ctx_of(meta, game=None):
    """Evaluation context when the plan declares its active DLCs, else None
    (legacy mode: every has_dlc branch is read). DLCs integrated into the base
    game (integrated_dlc/) are always active, though the game log does not list them."""
    dlcs = meta.get("dlcs")
    if not isinstance(dlcs, list):
        return None
    names = [d for d in dlcs if isinstance(d, str)]
    if game is not None:
        names += game.integrated_dlcs()
    return Ctx(meta.get("tag", ""), names)


def start_of(meta):
    return meta.get("start_date") or DEFAULT_START


# --------------------------------------------------------------------------
# Schema (skills/hoi4-campaign-playbook/plan.schema.json through schema_check)

SCHEMA_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
                           "skills", "hoi4-campaign-playbook", "plan.schema.json")
CHECKED_LISTS = ("research_path", "construction_plan", "vp_targets", "army_layout", "doctrines", "event_rules")


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def load_schema(path=None):
    path = path or SCHEMA_PATH
    try:
        with open(path, encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return None


def check_schema(plan, rep, schema=None):
    """Schema errors (as errors) and {key: [indexes of entries without schema errors]} for the
    lists the game-file checks below read; those checks skip malformed entries."""
    schema = schema if schema is not None else load_schema()
    good = {}
    if schema is None:
        rep.warn(f"plan schema not found ({SCHEMA_PATH}); shapes not checked, executor-key checks skipped")
        return {k: [] for k in CHECKED_LISTS}
    errors = schema_check.validate(plan, schema)
    bad = defaultdict(set)
    for path, message in errors:
        rep.error(f"schema: {path}: {message}")
        m = re.match(r"^([A-Za-z_]+)(?:\[(\d+)\])?", path)
        if m:
            bad[m.group(1)].add(int(m.group(2)) if m.group(2) is not None else None)
    for key in CHECKED_LISTS:
        v = plan.get(key)
        if not isinstance(v, list) or None in bad[key]:
            good[key] = []
            continue
        good[key] = [i for i, e in enumerate(v) if isinstance(e, dict) and i not in bad[key]]
    return good


def _entries(plan, good, key):
    v = plan.get(key)
    if not isinstance(v, list):
        return []
    return [(i, v[i]) for i in good.get(key, [])]


# --------------------------------------------------------------------------
# Research


def start_tech_set(game, meta):
    have = set(meta.get("completed_techs", []))
    ctx = ctx_of(meta, game)
    if ctx:
        have |= game.history_effects(meta.get("history_file", ""), ctx, start_of(meta))["techs"]
    else:
        have |= game.start_techs(meta.get("history_file", ""), start_of(meta))
    return have


def _prereq(rep, label, k, info, done):
    """Error when no reading allows `k` after `done`; warning when only the any-parent reading
    does (the two readers disagree for multi-parent techs). Returns True when it reported."""
    any_miss = techrule.missing(info, done, "any")
    if any_miss:
        rep.error(f"{label} {k}: prerequisite not earlier in list or at start (needs {' / '.join(any_miss)})")
        return True
    all_miss = techrule.missing(info, done, "all")
    if all_miss:
        parents = ", ".join(sorted(info["parents"]))
        rep.warn(
            f"{label} {k}: the two readings of its parents ({parents}) differ: startable when ANY "
            f"parent is done (hoi4_live/gamedata/tech.py), not when ALL are (tech_tree.rs startable "
            f"still needs {', '.join(all_miss)})"
        )
        return True
    return False


def check_research(game, plan, meta, rep):
    keys = strings(plan, "research_priority")
    if "research_priority" in plan and not isinstance(plan["research_priority"], list):
        rep.error("research_priority must be a list")
    dup_check(rep, "research_priority", keys)
    techs = game.techs()
    have = start_tech_set(game, meta)
    stop_year = date_year(meta.get("stop_date"), 1938)
    done = set(have)
    unknown = 0
    window = int(meta.get("ahead_check_first", 45))
    for i, k in enumerate(keys):
        info = techs.get(k)
        if info is None:
            rep.error(f"research_priority[{i}]: unknown technology {k}")
            unknown += 1
            continue
        if k in have:
            rep.warn(f"research_priority[{i}] {k}: already researched at start")
        _prereq(rep, f"research_priority[{i}]", k, info, done)
        limit = stop_year + (1 if i < window else 2)
        if info["year"] > limit:
            rep.warn(
                f"research_priority[{i}] {k}: start_year {info['year']} is far ahead "
                f"of {stop_year} (ahead-of-time penalty)"
            )
        done.add(k)
    return len(keys) - unknown


def check_research_path(game, plan, meta, rep, good):
    """research_path: years in order, not_before a date in that year, every tech known, no
    duplicates, prerequisites at start / earlier in the path / in research_priority (the executor
    tries due path entries first, then research_priority), start years not ahead of the entry."""
    entries = _entries(plan, good, "research_path")
    techs = game.techs()
    have = start_tech_set(game, meta)
    priority = set(strings(plan, "research_priority"))
    done = set(have) | priority
    seen = {}
    last_year = None
    n = 0
    for i, e in entries:
        year = e.get("year")
        if year is None:
            year = date_year(e.get("not_before"), None)
        if year is None:
            rep.warn(f"research_path[{i}]: no year or not_before; the executor treats it as due at once")
            year = 0
        elif not 1900 <= year <= 2100:
            rep.error(f"research_path[{i}].year {year} is not a game year")
        if year and last_year and year < last_year:
            rep.warn(f"research_path[{i}]: year {year} comes after {last_year} (entries count from their year)")
        last_year = year
        nb = e.get("not_before")
        if nb is not None:
            d = date_tuple(nb)
            if not d or not re.match(r"^\d{4}[.-]\d{1,2}[.-]\d{1,2}$", nb) or not (1 <= d[1] <= 12 and 1 <= d[2] <= 31):
                rep.error(f"research_path[{i}].not_before {nb!r} is not a date like \"1936.01.01\"")
            elif year and d[0] != year:
                rep.warn(f"research_path[{i}]: not_before {nb} is not in year {year}")
        for k in e["techs"]:
            label = f"research_path[{i}]"
            n += 1
            if k in seen:
                rep.error(f"{label}: duplicate tech {k} (also in research_path[{seen[k]}])")
                continue
            seen[k] = i
            info = techs.get(k)
            if info is None:
                rep.error(f"{label}: unknown technology {k}")
                continue
            if k in have:
                rep.warn(f"{label} {k}: already researched at start")
            _prereq(rep, label, k, info, done)
            if year and info["year"] > year:
                rep.warn(
                    f"{label} {k}: start_year {info['year']} is after the entry's year {year} "
                    f"(ahead-of-time penalty)"
                )
            done.add(k)
    return n


# --------------------------------------------------------------------------
# Focus, production, construction, politics, decisions, training (unchanged rules)


def check_focus(game, plan, meta, rep):
    path = strings(plan, "focus_path")
    fallback = strings(plan, "focus_fallback")
    dup_check(rep, "focus_path", path)
    trees = game.focus_trees()
    if not path and not fallback:
        return
    ctx = ctx_of(meta, game)
    loaded = game.loaded_tree(ctx) if ctx else None
    if loaded:
        # the tree the game actually gives this tag with these DLCs
        best = loaded
        dl = ", ".join(sorted(ctx.dlcs)) or "none"
        for label, keys in (("focus_path", path), ("focus_fallback", fallback)):
            for i, k in enumerate(keys):
                if k not in trees.get(best, {}):
                    owners = sorted(t for t, fs in trees.items() if k in fs)
                    where = f" (it is in {', '.join(owners)})" if owners else ""
                    rep.error(
                        f"{label}[{i}]: {k} is not in {best}, the focus tree that loads "
                        f"for {ctx.tag} with active DLCs [{dl}]{where}"
                    )
    else:
        # choose the tree that contains most keys
        best, best_n = None, -1
        for tid, fs in trees.items():
            n = sum(1 for k in path + fallback if k in fs)
            if n > best_n:
                best, best_n = tid, n
    tree = trees.get(best, {})
    done = set(meta.get("completed_focuses", []))
    listed = set(path)
    for i, k in enumerate(path):
        info = tree.get(k)
        if info is None:
            if not loaded:
                rep.error(f"focus_path[{i}]: unknown focus {k} (tree {best})")
            continue
        for block in info["prereq"]:
            if not (block & (done | set(path[:i]))):
                rep.error(
                    f"focus_path[{i}] {k}: prerequisite {' or '.join(sorted(block))} "
                    f"not earlier in path"
                )
        bad = info["exclusive"] & listed
        for other in sorted(bad):
            rep.error(f"focus_path: {k} is mutually exclusive with {other}, both listed")
        done.add(k)
    dup_check(rep, "focus_fallback", fallback)
    for i, k in enumerate(fallback):
        info = tree.get(k)
        if info is None:
            if not loaded:
                rep.error(f"focus_fallback[{i}]: unknown focus {k} (tree {best})")
            continue
        if k in listed:
            rep.error(f"focus_fallback[{i}]: {k} is already in focus_path")
        for other in sorted(info["exclusive"] & listed):
            rep.error(f"focus_fallback[{i}] {k}: mutually exclusive with {other} in focus_path")
        for other in sorted(info["exclusive"] & set(fallback)):
            rep.error(f"focus_fallback[{i}] {k}: mutually exclusive with fallback {other}")
        if info["prereq"] and not all(
            b & (set(meta.get("completed_focuses", [])) | listed | set(fallback)) for b in info["prereq"]
        ):
            rep.warn(f"focus_fallback[{i}] {k}: a prerequisite is neither in focus_path nor fallback")


def check_production(game, plan, rep):
    equip = game.equipment()
    lines = plan.get("production", [])
    if lines and not isinstance(lines, list):
        rep.error("production must be a list")
        lines = []
    seen = []
    for i, line in enumerate(lines):
        if not isinstance(line, dict):
            rep.error(f"production[{i}]: must be an object")
            continue
        k = line.get("equipment")
        if k not in equip:
            rep.error(f"production[{i}]: unknown equipment {k}")
        if "replace" in line and line["replace"] not in equip:
            rep.error(f"production[{i}] {k}: replace {line['replace']} is not an equipment key")
        if line.get("remove") or "replace" in line:
            if line.get("remove") and "replace" in line:
                rep.error(f"production[{i}] {k}: give remove or replace, not both")
            continue  # a change to a line, not a new line (factories optional, no duplicate)
        n = line.get("factories")
        if not isinstance(n, int) or not 1 <= n <= 150:
            rep.error(f"production[{i}] {k}: factories must be an integer 1..150")
        seen.append(k)
    dup_check(rep, "production", seen)
    for i, prefix in enumerate(strings(plan, "production_priority")):
        if not any(e.startswith(prefix) for e in equip):
            rep.error(f"production_priority[{i}]: no equipment starts with {prefix}")
    dup_check(rep, "production_priority", strings(plan, "production_priority"))


def check_construction(game, plan, rep, meta=None):
    meta = meta or {}
    ctx = ctx_of(meta, game)
    owners = game.state_owners(ctx, start_of(meta)) if ctx else {}
    buildings = game.buildings()
    states = game.states()
    by_name = {}
    for sid, s in states.items():
        for n in s["names"]:
            by_name.setdefault(n, set()).add(sid)
    entries = plan.get("construction", [])
    if entries and not isinstance(entries, list):
        rep.error("construction must be a list")
        entries = []
    for i, e in enumerate(entries):
        if not isinstance(e, dict):
            rep.error(f"construction[{i}]: must be an object")
            continue
        b = e.get("building")
        if b not in buildings:
            rep.error(f"construction[{i}]: unknown building {b}")
        ids = []
        for s in e.get("states", []):
            if isinstance(s, int) or (isinstance(s, str) and s.isdigit()):
                sid = int(s)
                if sid not in states:
                    rep.error(f"construction[{i}] {b}: unknown state id {s}")
                ids.append(sid)
            elif isinstance(s, str):
                hit = by_name.get(s)
                if not hit:
                    rep.error(f"construction[{i}] {b}: unknown state name {s!r}")
                elif len(hit) > 1:
                    rep.error(f"construction[{i}] {b}: ambiguous state name {s!r}")
                else:
                    ids.append(next(iter(hit)))
            else:
                rep.error(f"construction[{i}] {b}: bad state {s!r}")
        dup_check(rep, f"construction[{i}] {b} states", [str(x) for x in ids])
        for sid in ids:
            own = owners.get(sid)
            if ctx and own and own != ctx.tag:
                rep.warn(
                    f"construction[{i}] {b}: state {sid} is owned by {own}, not "
                    f"{ctx.tag}, at the start with the active DLCs"
                )
        c = e.get("count", 1)
        if not isinstance(c, int) or not 1 <= c <= 10:
            rep.error(f"construction[{i}] {b}: count must be an integer 1..10")


def check_politics(game, plan, meta, rep):
    picks = plan.get("politics", [])
    if picks and not isinstance(picks, list):
        rep.error("politics must be a list")
        return
    ideas, chars, tokens = game.ideas(ctx_of(meta, game))
    seen = set()
    for i, p in enumerate(picks):
        if not isinstance(p, dict):
            rep.error(f"politics[{i}]: must be an object")
            continue
        cat, slot, idea = p.get("category"), p.get("slot"), p.get("idea")
        if cat not in ("government", "research_production", "military_staff"):
            rep.error(f"politics[{i}]: unknown category {cat}")
        if not isinstance(slot, int) or not 0 <= slot <= 7:
            rep.error(f"politics[{i}]: slot must be 0..7")
        if not isinstance(idea, str) or not re.match(r"^[A-Za-z0-9_]+$", idea or ""):
            rep.error(f"politics[{i}]: invalid idea key {idea!r}")
        elif idea not in ideas and idea not in chars and idea not in tokens:
            rep.error(f"politics[{i}]: unknown idea/character {idea}")
        elif idea in tokens and idea not in ideas and idea not in chars:
            rep.warn(
                f"politics[{i}] {idea}: matches an advisor idea_token, not a character "
                f"id; check hoi4_politics_options for the key the UI reports"
            )
        if (cat, slot, idea) in seen:
            rep.error(f"politics[{i}]: duplicate pick")
        seen.add((cat, slot, idea))
    # two different ideas for one slot is legal (an upgrade path) but worth a note
    slots = defaultdict(list)
    for p in picks:
        if isinstance(p, dict):
            slots[(p.get("category"), p.get("slot"))].append(p.get("idea"))
    for (cat, slot), names in slots.items():
        if len(names) > 1:
            rep.warn(f"politics {cat}[{slot}]: sequence {' -> '.join(map(str, names))}")


def check_decisions(game, plan, rep):
    keys = strings(plan, "decisions")
    dup_check(rep, "decisions", keys)
    known = game.decisions()
    for i, k in enumerate(keys):
        if k not in known:
            rep.error(f"decisions[{i}]: unknown decision {k}")


def start_oobs(game, meta):
    """The OOB names the history file loads at the start (ctx-aware), or []."""
    ctx = ctx_of(meta, game)
    if not meta.get("history_file"):
        return []
    return game.history_effects(meta["history_file"], ctx, start_of(meta))["oobs"]


def check_training(game, plan, rep, meta=None):
    """training: one line (object) or several (list of lines; templates[] names count as known)."""
    meta = meta or {}
    tr = plan.get("training")
    if tr is None:
        return
    if isinstance(tr, list):
        planned_names = {t.get("name") for t in plan.get("templates") or [] if isinstance(t, dict)}
        for i, line in enumerate(tr):
            if not isinstance(line, dict):
                rep.error(f"training[{i}] must be an object")
                continue
            _check_training_line(game, line, f"training[{i}]", rep, meta, planned_names)
        return
    if not isinstance(tr, dict):
        rep.error("training must be an object or a list of lines")
        return
    _check_training_line(game, tr, "training", rep, meta, set())


def _check_training_line(game, tr, label, rep, meta, planned_names):
    ctx = ctx_of(meta, game)
    for key in ("queue", "batch"):
        if key in tr and (not isinstance(tr[key], int) or tr[key] < 1):
            rep.error(f"{label}.{key} must be a positive integer")
    dep = tr.get("deploy_state")
    if dep is None:
        rep.error(f"{label}.deploy_state is required (a line without a location never deploys)")
    elif dep not in game.states():
        rep.error(f"{label}.deploy_state: unknown state id {dep}")
    elif ctx:
        own = game.state_owners(ctx, start_of(meta)).get(dep)
        if own and own != ctx.tag:
            rep.warn(f"{label}.deploy_state {dep} is owned by {own}, not {ctx.tag}, at the start")
    name = tr.get("template")
    if not isinstance(name, str) or not name:
        rep.error(f"{label}.template is required")
        return
    if name in planned_names:
        return
    oobs = start_oobs(game, meta) if ctx else []
    if oobs:
        # only the order of battle the history file loads with these DLCs
        if not any(name in game.oob(o, ctx)["templates"] for o in oobs):
            rep.warn(
                f"{label}.template {name!r} is not in the start OOB "
                f"{', '.join(oobs)} loaded with the active DLCs (fine if you create "
                f"it in game before the plan runs)"
            )
        return
    if name not in game.template_names():
        rep.warn(
            f"{label}.template {name!r} is not a template name in history/units "
            f"(fine if you create it in game before the plan runs)"
        )


# --------------------------------------------------------------------------
# construction_plan


_DATE = re.compile(r"^\s*(\d{4})[.-](\d{1,2})[.-](\d{1,2})\s*$")
_PREFIXES = ("province_controlled:", "state_fully_controlled:", "owner:", "date:")


def parse_when(text):
    """A construction_plan / army_layout trigger as the executor reads it:
    ("now",), ("date", (y, m, d)), ("province", id), ("state", id), ("owner", state, tag),
    ("date_prefix", (y, m, d)), ("bad", reason) for a malformed trigger, or ("text", text) for
    free text the executor never treats as due."""
    t = (text or "").strip()
    if not t:
        return ("now",)
    m = _DATE.match(t)
    if m:
        y, mo, d = (int(x) for x in m.groups())
        if 1 <= mo <= 12 and 1 <= d <= 31:
            return ("date", (y, mo, d))
        return ("bad", f"{t!r} is not a valid date")
    low = t.lower()
    if low.startswith("date:"):
        m = _DATE.match(t[5:])
        if m:
            return ("date_prefix", tuple(int(x) for x in m.groups()))
        return ("bad", f"{t!r}: date: needs a date like 1936.01.01")
    for prefix, kind in (("province_controlled:", "province"), ("state_fully_controlled:", "state")):
        if low.startswith(prefix):
            rest = t[len(prefix):].strip()
            if rest.isdigit():
                return (kind, int(rest))
            return ("bad", f"{t!r}: {prefix} needs a numeric id")
    if low.startswith("owner:"):
        m = re.match(r"^\s*(\d+)\s*=\s*([A-Z][A-Z0-9]{2})\s*$", t[6:])
        if m:
            return ("owner", int(m.group(1)), m.group(2))
        return ("bad", f"{t!r}: owner: needs <state id>=<TAG>")
    return ("text", t)


def _check_when(rep, label, when, game, home):
    """Report a trigger problem; returns the parsed trigger."""
    w = parse_when(when)
    states = game.states()
    kind = w[0]
    if kind == "bad":
        rep.error(f"{label}: trigger {w[1]}")
    elif kind == "text":
        rep.warn(f"{label}: when {w[1]!r} is not an executor trigger (date, province_controlled:, "
                 f"state_fully_controlled:, owner:S=TAG); the executor never queues it (player-only)")
    elif kind == "date_prefix":
        rep.warn(f"{label}: 'date:' prefix; the executor reads a bare date such as \"1936.01.01\"")
    elif kind == "province":
        sid = game.province_state().get(w[1])
        if sid is None:
            rep.error(f"{label}: province_controlled:{w[1]} is not a province of any state")
        elif home and sid != home:
            rep.error(f"{label}: province_controlled:{w[1]} is in state {sid}, but the executor looks "
                      f"for it in the item's state {home}")
    elif kind == "state":
        if w[1] not in states:
            rep.error(f"{label}: state_fully_controlled:{w[1]} is not a state id")
    elif kind == "owner":
        if w[1] not in states:
            rep.error(f"{label}: owner:{w[1]}=… is not a state id")
        tags = game.country_tags()
        if tags and w[2] not in tags:
            rep.error(f"{label}: owner:…={w[2]} is not a country tag")
    return w


def check_construction_plan(game, plan, meta, rep, good):
    entries = _entries(plan, good, "construction_plan")
    if not entries:
        return 0
    states = game.states()
    pstate = game.province_state()
    buildings = game.buildings()
    ctx = ctx_of(meta, game)
    owners = game.state_owners(ctx, start_of(meta)) if ctx else {}
    orders = defaultdict(list)
    adj = None
    for i, e in entries:
        key = e.get("id", e.get("order"))
        label = f"construction_plan[{i}] {key} {e['building']}"
        orders[str(key)].append(i)
        b = e["building"]
        rail = b in RAILWAY
        if not rail and b not in buildings:
            rep.error(f"{label}: unknown building {b}")
        home = e.get("state", e.get("to_state"))
        w = _check_when(rep, label, e.get("when", ""), game, home)
        player_only = w[0] == "text"
        for key in ("state", "from_state", "to_state"):
            if key in e and e[key] not in states:
                rep.error(f"{label}: {key} {e[key]} is not a state id")
        for s in e.get("states", []):
            if s not in states:
                rep.error(f"{label}: states: {s} is not a state id")
        if "states" in e and "state" not in e and not player_only:
            rep.error(f"{label}: the executor reads one `state`, not `states`")
        if rail:
            path = e.get("path")
            if path is None:
                if "from_state" not in e and "to_state" not in e and "state" not in e:
                    rep.error(f"{label}: a railway needs from_state/to_state or a path")
                continue
            if len(path) < 2:
                rep.error(f"{label}: path needs at least two provinces")
                continue
            unknown = [p for p in path if p not in pstate]
            for p in unknown:
                rep.error(f"{label}: path province {p} is not in any state")
            if unknown:
                continue
            fs = e.get("from_state", home)
            ts = e.get("to_state", home)
            if fs is not None and pstate[path[0]] != fs:
                rep.error(f"{label}: path starts at {path[0]} in state {pstate[path[0]]}, not from_state {fs}")
            if ts is not None and pstate[path[-1]] != ts:
                rep.error(f"{label}: path ends at {path[-1]} in state {pstate[path[-1]]}, not to_state {ts}")
            if adj is None:
                adj = game.neighbours(straits=False) or {}
                if not adj:
                    rep.warn("construction_plan: no map adjacency (map/provinces.bmp, definition.csv); "
                             "railway paths not checked for contiguity")
            if adj:
                for a, c in zip(path, path[1:]):
                    if c not in adj.get(a, ()):
                        rep.error(f"{label}: path provinces {a} and {c} do not touch")
        else:
            if home is None:
                if not player_only and "states" not in e:
                    rep.error(f"{label}: state is required (the executor builds in item.state)")
                continue
            if "province" in e:
                if e["province"] not in pstate:
                    rep.error(f"{label}: province {e['province']} is not in any state")
                elif pstate[e["province"]] != home:
                    rep.error(f"{label}: province {e['province']} is in state {pstate[e['province']]}, "
                              f"not {home}")
            if ctx and w[0] in ("now", "date") and owners.get(home) not in (None, ctx.tag):
                rep.warn(f"{label}: state {home} is owned by {owners.get(home)}, not {ctx.tag}, at the start")
    for order, idx in orders.items():
        if len(idx) > 1:
            rep.error(f"construction_plan: id {order} is used by entries {idx} (done-memory is keyed by id)")
    return len(entries)


# --------------------------------------------------------------------------
# vp_targets, army_layout, war


def check_vp_targets(game, plan, meta, rep, good):
    entries = _entries(plan, good, "vp_targets")
    if not entries:
        return 0
    states = game.states()
    pstate = game.province_state()
    facts = game.state_facts(ctx_of(meta, game), start_of(meta))
    for i, e in entries:
        p, s = e["province"], e["state"]
        label = f"vp_targets[{i}] {p}"
        if s not in states:
            rep.error(f"{label}: state {s} is not a state id")
            continue
        if p not in pstate:
            rep.error(f"{label}: province {p} is not in any state")
            continue
        if pstate[p] != s:
            rep.error(f"{label}: province {p} is in state {pstate[p]}, not {s}")
            continue
        if "vp" in e:
            have = facts.get(s, {}).get("vps", {}).get(p, 0)
            if float(e["vp"]) != float(have):
                rep.error(f"{label}: vp {e['vp']} but the state history gives {have:g} at the start")
    return len(entries)


def check_army_layout(game, plan, meta, rep, good):
    entries = _entries(plan, good, "army_layout")
    if not entries:
        return 0
    states = game.states()
    pstate = game.province_state()
    ctx = ctx_of(meta, game)
    oobs = start_oobs(game, meta)
    templates = set()
    per_state = defaultdict(lambda: defaultdict(int))  # state -> template -> divisions
    for o in oobs:
        ob = game.oob(o, ctx)
        if ob["file"] is None:
            rep.warn(f"army_layout: start OOB {o} has no history/units file")
        templates |= ob["templates"]
        for d in ob["divisions"]:
            sid = pstate.get(d["location"])
            if sid is not None and d["template"]:
                per_state[sid][d["template"]] += 1
    if not oobs:
        rep.warn("army_layout: no start OOB found (set _validator.history_file/tag); templates not checked")
    owners = game.state_owners(ctx, start_of(meta)) if ctx else {}
    hints = set()
    for idx, (i, e) in enumerate(entries):
        hint, home, role = e["name_hint"], e["state"], e["role"]
        label = f"army_layout[{i}] {hint!r}"
        if not hint:
            rep.error(f"{label}: name_hint must not be empty")
        if hint in hints:
            rep.error(f"{label}: duplicate name_hint")
        hints.add(hint)
        if role not in ROLES:
            rep.error(f"{label}: role must be strike or guard, not {role!r}")
        if home not in states:
            rep.error(f"{label}: state {home} is not a state id")
            continue
        later = "when" in e
        if later:
            w = parse_when(e["when"])
            if w[0] != "owner":
                rep.error(f"{label}: when {e['when']!r}: the layout reads only owner:<state>=<TAG>")
            else:
                _check_when(rep, label, e["when"], game, None)
        if not e["templates"]:
            rep.error(f"{label}: templates is empty (the role stays empty)")
        if oobs:
            for t in e["templates"]:
                if t not in templates:
                    msg = f"{label}: template {t!r} is not in the start OOB {', '.join(oobs)}"
                    (rep.warn if later else rep.error)(msg + (" (the entry waits for its when)" if later else ""))
        for s in e.get("guard_states_in_order", []):
            if s not in states:
                rep.error(f"{label}: guard_states_in_order {s} is not a state id")
        if role == "guard" and not e.get("guard_states_in_order"):
            rep.warn(f"{label}: guard without guard_states_in_order is bound to no post")
        if ctx and not later and owners.get(home) not in (None, ctx.tag):
            rep.warn(f"{label}: state {home} is owned by {owners.get(home)}, not {ctx.tag}, at the start")
        if not oobs or later:
            continue
        first_strike = role == "strike" and not any(
            x["state"] == home and x["role"] == "strike" for _, x in entries[:idx]
        )
        here = per_state.get(home, {})
        if first_strike:
            if not sum(here.values()):
                rep.error(f"{label}: no division starts in state {home}, so there is no army to take")
            continue
        n = sum(here.get(t, 0) for t in e["templates"])
        count = e.get("count", 1)
        if n == 0:
            rep.error(f"{label}: no division of {', '.join(e['templates'])} starts in state {home} "
                      f"(the role stays empty)")
        elif count > n:
            rep.warn(f"{label}: count {count} but only {n} division(s) of these templates start in state {home}")
        if count < 1:
            rep.error(f"{label}: count must be at least 1")
    return len(entries)


def _war_ids(rep, label, war, states, tags):
    if not isinstance(war, dict):
        return
    ids = []
    for a, axis in enumerate(war.get("axes") or []):
        if isinstance(axis, list):
            ids += [(f"axes[{a}]", s) for s in axis]
    for key in ("theatre", "guard_states"):
        v = war.get(key)
        if isinstance(v, list):
            ids += [(key, s) for s in v]
    g = war.get("guards")
    if isinstance(g, dict):
        ids += [(f"guards[{k}]", s) for k, s in g.items()]
    for where, s in ids:
        if _is_int(s) and s not in states:
            rep.error(f"{label}.{where}: {s} is not a state id")
    for t in war.get("enemies") or []:
        if isinstance(t, str) and tags and t not in tags:
            rep.error(f"{label}.enemies: {t} is not a country tag")
    me = war.get("me")
    if isinstance(me, str) and tags and me not in tags:
        rep.error(f"{label}.me: {me} is not a country tag")


def check_war(game, plan, rep):
    states = game.states()
    tags = game.country_tags()
    if isinstance(plan.get("war"), dict):
        _war_ids(rep, "war", plan["war"], states, tags)
    phases = plan.get("war_by_phase")
    if isinstance(phases, dict):
        for name, payload in phases.items():
            if not isinstance(payload, dict):
                rep.error(f"war_by_phase.{name} must be an object")
                continue
            _war_ids(rep, f"war_by_phase.{name}", payload, states, tags)


# --------------------------------------------------------------------------
# doctrines, event_rules

_XP_TYPE = {"land": "army", "naval": "navy", "navy": "navy", "air": "air"}


def check_doctrines(game, plan, meta, rep, good):
    """doctrines: [{"id", "folder", "grand_doctrine"? | "track" + "subdoctrine", "xp"}] against
    common/doctrines (folders, grand doctrines and their tracks, subdoctrines and their track)."""
    raw = plan.get("doctrines")
    if isinstance(raw, list):
        for i, e in enumerate(raw):
            if isinstance(e, dict) and "when" in e and "xp" not in e:
                rep.error(f"doctrines[{i}]: old shape with 'when'; write xp (army XP to wait for) and an id")
    entries = _entries(plan, good, "doctrines")
    if not entries:
        return 0
    ctx = ctx_of(meta, game)
    d = game.doctrines(ctx)
    ids = set()
    for i, e in entries:
        label = f"doctrines[{i}] {e['id']}"
        if str(e["id"]) in ids:
            rep.error(f"{label}: duplicate id")
        ids.add(str(e["id"]))
        folder = e["folder"]
        if d["folders"] and folder not in d["folders"] and not (folder == "navy" and "naval" in d["folders"]):
            rep.error(f"{label}: unknown doctrine folder {folder} (have {', '.join(sorted(d['folders']))})")
        elif d["folders"].get(folder) is False:
            rep.warn(f"{label}: folder {folder} is not allowed with the active DLCs")
        gd, sub, track = e.get("grand_doctrine"), e.get("subdoctrine"), e.get("track")
        if not gd and not sub:
            rep.error(f"{label}: give grand_doctrine or subdoctrine (or both: the grand doctrine is adopted first)")
            continue
        costs = []
        if gd:
            info = d["grand"].get(gd)
            if info is None:
                rep.error(f"{label}: unknown grand doctrine {gd}")
            else:
                if info["folder"] and info["folder"] != folder:
                    rep.error(f"{label}: grand doctrine {gd} is in folder {info['folder']}, not {folder}")
                if track and not sub:
                    rep.warn(f"{label}: track is ignored for a grand doctrine")
                costs.append(info["xp_cost"])
        if sub:
            info = d["sub"].get(sub)
            if info is None:
                rep.error(f"{label}: unknown subdoctrine {sub}")
                continue
            if not track:
                rep.error(f"{label}: subdoctrine {sub} needs its track ({info['track']})")
            elif info["track"] and info["track"] != track:
                rep.error(f"{label}: subdoctrine {sub} is on track {info['track']}, not {track}")
            elif d["tracks"] and track not in d["tracks"]:
                rep.error(f"{label}: unknown doctrine track {track}")
            pool = [g for k, g in d["grand"].items() if g["folder"] == folder and (not gd or k == gd)]
            in_folder = {t for g in pool for t in g["tracks"]}
            if track and in_folder and track not in in_folder:
                where = f"grand doctrine {gd}" if gd else f"{folder} grand doctrine"
                rep.error(f"{label}: track {track} belongs to no {where}")
            want = _XP_TYPE.get(folder)
            if want and info["xp_type"] and info["xp_type"] != want:
                rep.error(f"{label}: subdoctrine {sub} costs {info['xp_type']} XP, folder {folder} gives {want}")
            costs.append(info["xp_cost"])
        cost = max(costs) if costs else 0
        if cost and e["xp"] < cost:
            rep.warn(f"{label}: xp {e['xp']:g} is below the base cost {cost:g} (the adopt is refused until then)")
    return len(entries)


def check_event_rules(game, plan, rep, good):
    entries = _entries(plan, good, "event_rules")
    for i, e in entries:
        eid = e["event"]
        label = f"event_rules[{i}] {eid}"
        ev = game.event(eid)
        if ev is None:
            rep.error(f"{label}: no event with this id in events/")
            continue
        names = [n for n in ev["options"] if n]
        for key in ("option_name", "fallback_option_name", "else_option_name"):
            name = e.get(key)
            if name is not None and name not in names:
                rep.error(f"{label}: {key} {name} is not an option of the event (has {', '.join(names) or 'none'})")
        if "else_option_name" in e and "condition" not in e and "when" not in e:
            rep.warn(f"{label}: else_option_name without a when")
        if _is_int(e.get("fallback_index")) and e["fallback_index"] >= len(ev["options"]):
            rep.error(f"{label}: fallback_index {e['fallback_index']} but the event has {len(ev['options'])} options")
    return len(entries)


# --------------------------------------------------------------------------
# Predicates (plan.schema.json definitions.predicate: x-grammar is the grammar, x-atoms the atoms)

_TAG_RE = r"[A-Z][A-Z0-9]{2}"
PRED_ARGS = {
    "province": r"\d+", "state": r"\d+", "tag": _TAG_RE, "key": r"[A-Za-z0-9_.\-]+", "id": r"[A-Za-z0-9_]+",
    "branch": r"army|air|navy|land|naval", "army": r"[^<>=!\s](?:[^<>=!]*[^<>=!\s])?", "equipment": r"[A-Za-z0-9_]+",
    "name": r"[A-Za-z0-9_.@^:]+",
}
PRED_OPS = ("<=", ">=", "==", "!=", "<", ">")
_PRED_CMP = re.compile(r"^(?P<name>[a-z_]+)(?::(?P<arg>.*?))?\s*(?P<op><=|>=|==|!=|=|<|>)\s*(?P<value>[^<>=!\s][^<>=!]*)$")
_PRED_FLAG = re.compile(r"^(?P<name>[a-z_]+)(?::(?P<arg>[^<>=!]+))?$")
_PRED_DATE = re.compile(r"^(?:date:)?(\d{4}[.-]\d{1,2}[.-]\d{1,2})$")
_PRED_NUM = re.compile(r"^-?\d+(\.\d+)?%?$")


def predicate_table(schema=None):
    """The atom table (x-atoms of definitions.predicate) of the plan schema, or {} without one."""
    schema = schema if schema is not None else load_schema()
    return (((schema or {}).get("definitions") or {}).get("predicate") or {}).get("x-atoms") or {}


def _pred_date(text):
    m = _PRED_DATE.match(text or "")
    d = date_tuple(m.group(1)) if m else None
    return d if d and 1 <= d[1] <= 12 and 1 <= d[2] <= derive.MONTH_DAYS[d[1] - 1] else None


def parse_atom(text, table):
    """(atom, None) or (None, reason) for one predicate string or JSON boolean. atom = {"name",
    "arg", "op", "value", "kind", "text", "readable"}; a bare date (or date:<date>) is the date atom
    with op >=; for kind tag, = reads as ==; a ratio value above 1 or with % is a percent (/100);
    "" and true are the constant true, false the constant false (name "const")."""
    if isinstance(text, bool) or text in ("", "true", "false"):
        value = text if isinstance(text, bool) else text != "false"
        return {"name": "const", "arg": None, "op": None, "value": value, "kind": "const", "text": str(text),
                "readable": True}, None
    if not isinstance(text, str):
        return None, f"expected a string, a list or an all/any/not object, got {type(text).__name__}"
    if text != text.strip():
        return None, f"{text!r}: no leading or trailing space"
    if _PRED_DATE.match(text):
        d = _pred_date(text)
        if d is None:
            return None, f"{text!r} is not a valid date"
        return {"name": "date", "arg": None, "op": ">=", "value": d, "kind": "date", "text": text,
                "readable": True}, None
    m = _PRED_CMP.match(text) or _PRED_FLAG.match(text)
    if not m:
        return None, f"{text!r} is neither `atom op value` nor a bare atom"
    name, arg = m.group("name"), m.group("arg")
    op = m.groupdict().get("op")
    value = m.groupdict().get("value")
    spec = table.get(name)
    if spec is None:
        return None, f"{text!r}: unknown atom {name} (atoms: {', '.join(sorted(table))})"
    want = spec.get("arg")
    if want and arg is None:
        return None, f"{text!r}: {name} needs an argument ({name}:<{want}>)"
    if not want and arg is not None:
        return None, f"{text!r}: {name} takes no argument"
    if want and not re.fullmatch(PRED_ARGS.get(want, r".+"), arg):
        return None, f"{text!r}: {arg!r} is not a {want}"
    kind = spec.get("kind")
    atom = {"name": name, "arg": arg, "op": op, "value": value, "kind": kind, "text": text,
            "readable": spec.get("readable", True)}
    if kind == "bool":
        if op is not None:
            return None, f"{text!r}: {name} is a yes/no atom; write it bare (negate with {{\"not\": ...}})"
        return atom, None
    if op is None:
        return None, f"{text!r}: {name} needs a comparison ({name}{':' + arg if arg else ''} >= value)"
    if kind == "tag":
        if op not in ("=", "==", "!="):
            return None, f"{text!r}: {name} compares a tag with == or !="
        if not re.fullmatch(_TAG_RE, value):
            return None, f"{text!r}: {value!r} is not a country tag"
        atom["op"] = "==" if op == "=" else op
        return atom, None
    if op == "=":
        return None, f"{text!r}: use == (= is only the owner:<state>=<TAG> form)"
    if kind == "date":
        d = _pred_date(value)
        if d is None:
            return None, f"{text!r}: {value!r} is not a date like 1936.05.01"
        atom["value"] = d
        return atom, None
    if not _PRED_NUM.match(value):
        return None, f"{text!r}: {value!r} is not a number"
    percent = value.endswith("%")
    v = float(value.rstrip("%"))
    if percent and not spec.get("ratio"):
        return None, f"{text!r}: {name} is not a ratio; write a plain number"
    if spec.get("ratio") and (percent or v > 1):
        if not 0 <= v <= 100:
            return None, f"{text!r}: {name} as a percent runs from 0 to 100"
        v = v / 100.0
    lo, hi = spec.get("min"), spec.get("max")
    if (lo is not None and v < lo) or (hi is not None and v > hi):
        unit = f" ({spec['unit']})" if spec.get("unit") else ""
        return None, f"{text!r}: {name} runs from {lo if lo is not None else '-inf'} to {hi if hi is not None else 'inf'}{unit}"
    atom["value"] = v
    return atom, None


def parse_predicate(pred, table, path=""):
    """([(path, atom)], [(path, problem)]) of a predicate tree (a plain list is all); NOT branches
    included."""
    atoms, problems = [], []
    if isinstance(pred, list):
        if not pred:
            return atoms, problems  # an empty list holds (all of nothing)
        pred = {"all": pred}
    if isinstance(pred, dict):
        if len(pred) != 1 or next(iter(pred)) not in ("all", "any", "not"):
            problems.append((path, f"an object predicate has exactly one key all, any or not (got {sorted(pred)})"))
            return atoms, problems
        key, val = next(iter(pred.items()))
        items = [val] if key == "not" else val
        if key != "not" and (not isinstance(val, list) or not val):
            problems.append((path, f"{key} needs a non-empty list"))
            return atoms, problems
        for i, p in enumerate(items):
            a, b = parse_predicate(p, table, f"{path}.{key}" + ("" if key == "not" else f"[{i}]"))
            atoms += a
            problems += b
        return atoms, problems
    atom, why = parse_atom(pred, table)
    if why:
        problems.append((path, why))
    else:
        atoms.append((path, atom))
    return atoms, problems


def plan_predicates(plan, schema):
    """[(path, predicate)] of every place the schema types as a predicate."""
    return schema_check.instances_of(plan, schema, "#/definitions/predicate")


def predicate_dates(pred, table):
    """Dates a predicate waits for (date atoms with >= or > outside NOT), as day-tuples."""
    out = []
    if isinstance(pred, list):
        pred = {"all": pred}
    if isinstance(pred, dict):
        for k in ("all", "any"):
            for p in pred.get(k) or []:
                out += predicate_dates(p, table)
        return out
    atom, _ = parse_atom(pred, table)
    if atom and atom["name"] == "date" and atom["op"] in (">=", ">", "=="):
        out.append(atom["value"])
    return out


def predicate_can_hold(pred, table):
    """(can be true, can be false) of a predicate when every atom with readable: false stays
    unknown (three-valued: all needs every part true, any one part, not swaps)."""
    if isinstance(pred, list):
        pred = {"all": pred}
    if isinstance(pred, dict) and len(pred) == 1:
        key, val = next(iter(pred.items()))
        if key == "not":
            t, f = predicate_can_hold(val, table)
            return f, t
        parts = [predicate_can_hold(p, table) for p in val or []] if isinstance(val, list) else []
        if key == "all":
            return all(t for t, _ in parts), any(f for _, f in parts)
        if key == "any":
            return any(t for t, _ in parts), all(f for _, f in parts)
        return False, False
    atom, _ = parse_atom(pred, table)
    if atom is None or not atom["readable"]:
        return False, False
    if atom["kind"] == "const":
        return atom["value"], not atom["value"]
    return True, True


def check_predicates(game, plan, rep, schema=None):
    """Every predicate parses (x-grammar) and names known provinces, states, tags, focuses, techs,
    phases, equipment and missions."""
    schema = schema if schema is not None else load_schema()
    table = predicate_table(schema)
    if not table:
        return 0
    states, pstate = game.states(), game.province_state()
    tags = game.country_tags()
    phases = {str(p.get("id")) for p in plan.get("phases") or [] if isinstance(p, dict)}
    focus = None
    n = 0
    for path, pred in plan_predicates(plan, schema):
        atoms, problems = parse_predicate(pred, table, path)
        for where, why in problems:
            rep.error(f"predicate {where}: {why}")
        for where, a in atoms:
            n += 1
            name, arg = a["name"], a["arg"]
            if name == "const":
                continue
            want = table[name].get("arg")
            label = f"predicate {where}: {a['text']!r}"
            if want == "province" and int(arg) not in pstate:
                rep.error(f"{label}: province {arg} is in no state")
            elif want == "state" and int(arg) not in states:
                rep.error(f"{label}: {arg} is not a state id")
            elif want == "tag" and tags and arg not in tags:
                rep.error(f"{label}: {arg} is not a country tag")
            if a["kind"] == "tag" and tags and a["value"] not in tags:
                rep.error(f"{label}: {a['value']} is not a country tag")
            if name in ("focus_completed", "focus_done"):
                if focus is None:
                    focus = set().union(*game.focus_trees().values()) if game.focus_trees() else set()
                if focus and arg not in focus:
                    rep.error(f"{label}: no focus {arg} in common/national_focus")
            elif name in ("tech_researched", "tech_done") and arg not in game.techs():
                rep.error(f"{label}: no technology {arg}")
            elif name == "phase" and arg not in phases:
                rep.error(f"{label}: no phases[] entry with id {arg}")
            elif name == "stockpile_days" and arg not in game.equipment() and arg not in derive.equipment_archetypes(game):
                rep.error(f"{label}: no equipment {arg} in common/units/equipment")
            elif name == "mission_days" and arg not in game.decisions():
                rep.error(f"{label}: no decision or mission {arg} in common/decisions")
    return n


# --------------------------------------------------------------------------
# Coverage-build keys: templates, stockpile_targets, xp_budget, force_posture, occupation, peace,
# auto_import, garrison_template, production remove/replace


def template_columns(regiments):
    """Columns of a templates[].regiments value as the executor reads it (plan_manage.rs columns):
    a list of columns as is, a flat list of keys cut into runs of one key, five per column; None
    for a mixed or empty list."""
    if not isinstance(regiments, list) or not regiments:
        return None
    if all(isinstance(c, list) for c in regiments):
        return regiments
    if not all(isinstance(k, str) for k in regiments):
        return None
    out = []
    for k in regiments:
        if out and len(out[-1]) < 5 and out[-1][0] == k:
            out[-1].append(k)
        else:
            out.append([k])
    return out


def _template_columns(t):
    return template_columns(t.get("regiments")) or []


def check_templates(game, plan, rep):
    """Unit keys from common/units in the right rows, one group per column, width at most 40,
    regimental supports only under columns of 3 battalions, unique ids and names."""
    entries = [(i, t) for i, t in enumerate(plan.get("templates") or []) if isinstance(t, dict)]
    if not entries:
        return 0
    units = derive.unit_types(game)
    need = int(derive.define(game, "REGIMENTAL_SUPPORT_REQUIRED_BATTALIONS", 3) or 3)
    ids, names = set(), set()
    for i, t in entries:
        label = f"templates[{i}] {t.get('id')}"
        for key, seen in (("id", ids), ("name", names)):
            v = str(t.get(key))
            if v in seen:
                rep.error(f"{label}: duplicate {key} {v}")
            seen.add(v)
        if template_columns(t.get("regiments")) is None:
            rep.error(f"{label}: regiments is a list of columns or a flat list of battalion keys, not a mix")
        cols = _template_columns(t)
        if len(cols) > 5:
            rep.error(f"{label}: {len(cols)} columns; the designer has 5")
        for c, col in enumerate(cols):
            groups = set()
            for u in col:
                info = units.get(u)
                if info is None:
                    rep.error(f"{label}: regiments[{c}] {u} is not a sub-unit in common/units")
                elif info["support"]:
                    rep.error(f"{label}: regiments[{c}] {u} is a support company (put it in support)")
                else:
                    groups.add(info["group"])
            if len(groups) > 1:
                rep.error(f"{label}: regiments[{c}] mixes groups {', '.join(sorted(groups))} (one group per column)")
        for u in t.get("support") or []:
            info = units.get(u)
            if info is None:
                rep.error(f"{label}: support {u} is not a sub-unit in common/units")
            elif not info["support"]:
                rep.error(f"{label}: support {u} is a battalion, not a support company")
        dup = [u for u in set(t.get("support") or []) if (t.get("support") or []).count(u) > 1]
        for u in dup:
            rep.error(f"{label}: support {u} listed twice")
        for c, u in enumerate(t.get("regimental_support") or []):
            info = units.get(u)
            if info is None:
                rep.error(f"{label}: regimental_support[{c}] {u} is not a sub-unit in common/units")
            elif not info["regimental"]:
                rep.error(f"{label}: regimental_support[{c}] {u} is not a regimental support battalion")
            if c >= len(cols) or len(cols[c]) < need:
                rep.error(f"{label}: regimental_support[{c}] needs {need} battalions in regiments[{c}]")
        width = derive.template_width(game, cols)
        if width is not None and width > 40:
            rep.error(f"{label}: combat width {width:g} is above 40 (the template tool refuses it)")
    return len(entries)


def check_coverage_keys(game, plan, meta, rep):
    """The new player keys against the game files and the plan's own ids."""
    equip = game.equipment()
    arche = derive.equipment_archetypes(game)
    tags = game.country_tags()
    states = game.states()
    sp = plan.get("stockpile_targets")
    if isinstance(sp, dict):
        for k in sp:
            if k not in ("why", "note") and k not in equip and k not in arche:
                rep.error(f"stockpile_targets.{k}: no equipment {k} in common/units/equipment")
    tids = {str(t.get("id")) for t in plan.get("templates") or [] if isinstance(t, dict)}
    tnames = {t.get("name") for t in plan.get("templates") or [] if isinstance(t, dict)}
    dids = {str(d.get("id")) for d in plan.get("doctrines") or [] if isinstance(d, dict)}
    xb = plan.get("xp_budget")
    if isinstance(xb, dict):
        for branch in ("army", "air", "navy"):
            for j, s in enumerate(xb.get(branch) or []):
                ref = s.get("ref") if isinstance(s, dict) else None
                if not isinstance(ref, str) or ":" not in ref:
                    continue
                kind, rid = ref.split(":", 1)
                if (kind == "template" and rid not in tids) or (kind == "doctrine" and rid not in dids):
                    rep.error(f"xp_budget.{branch}[{j}]: ref {ref} names no {kind}s[] id of this plan")
    fp = plan.get("force_posture")
    if isinstance(fp, dict):
        phases = {str(p.get("id")) for p in plan.get("phases") or [] if isinstance(p, dict)} | {"base"}
        for j, p in enumerate(fp.get("phases") or []):
            if not isinstance(p, dict):
                continue
            if p.get("phase") not in phases:
                rep.error(f"force_posture.phases[{j}]: phase {p.get('phase')} is neither base nor a phases[] id")
            total = sum(p.get(k, 0) for k in ("land", "air", "navy") if isinstance(p.get(k), (int, float)))
            if abs(total - 100) > 1:
                rep.error(f"force_posture.phases[{j}]: land + air + navy is {total:g}, not 100")
    occ = plan.get("occupation")
    if isinstance(occ, dict):
        laws = derive.occupation_laws(game)
        named = [(f"occupation.{k}", occ.get(k)) for k in ("default_law", "law", "mildest", "harshest")]
        named += [(f"occupation.ladder[{j}]", x) for j, x in enumerate(occ.get("ladder") or [])]
        named += [(f"occupation.per_state.{s}", x) for s, x in (occ.get("per_state") or {}).items()]
        for where, law in named:
            if laws and isinstance(law, str) and law not in laws:
                rep.error(f"{where}: no occupation law {law} in common/occupation_laws")
        ladder = occ.get("ladder") or []
        for k in ("mildest", "harshest"):
            if ladder and isinstance(occ.get(k), str) and occ[k] not in ladder:
                rep.error(f"occupation.{k}: {occ[k]} is not on occupation.ladder")
        if "default_law" in occ and "law" in occ:
            rep.warn("occupation: default_law and law both given; the executor uses default_law")
    gt = plan.get("garrison_template")
    if isinstance(gt, dict) and isinstance(gt.get("template"), str):
        name = gt["template"]
        if name not in tids and name not in tnames and name not in game.template_names():
            rep.warn(f"garrison_template.template {name!r} is neither a templates[] entry nor a history template "
                     f"(fine if it is created in game first)")
    peace = plan.get("peace")
    if isinstance(peace, dict):
        for key in ("take", "puppet", "liberate"):
            for j, s in enumerate(peace.get(key) or []):
                if _is_int(s) and s not in states:
                    rep.error(f"peace.{key}[{j}]: {s} is not a state id")
                elif isinstance(s, str):
                    if tags and s not in tags:
                        rep.error(f"peace.{key}[{j}]: {s!r} is neither a state id nor a country tag")
                    elif key == "take":
                        rep.error(f"peace.take[{j}]: {s} is a tag; the executor takes state ids")
                    else:
                        rep.warn(f"peace.{key}[{j}]: tag {s} is only noted (hoi4_peace_take takes state ids); "
                                 f"list the states")
    st = plan.get("sea_transfer")
    if isinstance(st, dict):
        for j, r in enumerate(st.get("routes") or []):
            if isinstance(r, dict):
                for key in ("from_state", "to_state"):
                    if isinstance(r.get(key), int) and r[key] not in states:
                        rep.error(f"sea_transfer.routes[{j}].{key}: {r[key]} is not a state id")
        for s in st.get("ports") or []:
            if s not in states:
                rep.error(f"sea_transfer.ports: {s} is not a state id")


# --------------------------------------------------------------------------


def check_misc(plan, rep):
    al = plan.get("alarms")
    if al is not None and not isinstance(al, dict):
        rep.error("alarms must be an object")
    elif isinstance(al, dict):
        for k, v in al.items():
            if not isinstance(v, (bool, int, float)) and v is not None:
                rep.error(f"alarms.{k}: must be a number, boolean or null")
    eo = plan.get("event_options", {})
    if eo and not isinstance(eo, dict):
        rep.error("event_options must be an object")
    elif isinstance(eo, dict):
        for k, v in eo.items():
            if not isinstance(v, int) or v < 0:
                rep.error(f"event_options[{k}]: must be a non-negative integer")
    ep = plan.get("event_prefix_options", {})
    if ep and not isinstance(ep, dict):
        rep.error("event_prefix_options must be an object")
    for key in ("active", "stop_on_failure", "research_fallback"):
        if key in plan and not isinstance(plan[key], bool):
            rep.error(f"{key} must be boolean")
    if "stop_on_date" in plan and not re.match(r"^\d{4}\.\d{1,2}\.\d{1,2}$", str(plan["stop_on_date"])):
        rep.error('stop_on_date must look like "1938.01.01"')
    arm = plan.get("armies")
    if arm is not None:
        if not isinstance(arm, dict):
            rep.error("armies must be an object")
        elif not isinstance(arm.get("max", 24), int):
            rep.error("armies.max must be an integer")


def resolve_meta(plan, game, rep):
    """_validator with defaults: tag (GER), start_date, the tag's history file by glob."""
    meta = dict(plan.get("_validator", {}) or {})
    tag = meta.setdefault("tag", "GER")
    meta.setdefault("start_date", DEFAULT_START)
    hf = meta.get("history_file")
    if hf and not game.file(hf):
        found = game.history_file(tag)
        rep.warn(f"_validator.history_file {hf} does not exist"
                 + (f"; using {found}" if found else ""))
        meta["history_file"] = found or hf
    elif not hf:
        meta["history_file"] = game.history_file(tag) or ""
    return meta


def validate(plan, game, schema=None):
    """(Report, counts). schema: the plan schema dict (default: plan.schema.json in the repo)."""
    rep = Report()
    meta = resolve_meta(plan, game, rep)
    good = check_schema(plan, rep, schema)
    counts = {"research": check_research(game, plan, meta, rep)}
    check_focus(game, plan, meta, rep)
    check_production(game, plan, rep)
    check_construction(game, plan, rep, meta)
    check_politics(game, plan, meta, rep)
    check_decisions(game, plan, rep)
    check_training(game, plan, rep, meta)
    check_misc(plan, rep)
    counts.update(
        focus=len(strings(plan, "focus_path")),
        production=len(plan.get("production", []) or []),
        construction=len(plan.get("construction", []) or []),
        politics=len(plan.get("politics", []) or []),
        decisions=len(strings(plan, "decisions")),
    )
    counts["research_path"] = check_research_path(game, plan, meta, rep, good)
    counts["construction_plan"] = check_construction_plan(game, plan, meta, rep, good)
    counts["vp_targets"] = check_vp_targets(game, plan, meta, rep, good)
    counts["army_layout"] = check_army_layout(game, plan, meta, rep, good)
    check_war(game, plan, rep)
    counts["doctrines"] = check_doctrines(game, plan, meta, rep, good)
    counts["event_rules"] = check_event_rules(game, plan, rep, good)
    counts["predicates"] = check_predicates(game, plan, rep, schema)
    counts["templates"] = check_templates(game, plan, rep)
    check_coverage_keys(game, plan, meta, rep)
    for k in ("research_path", "construction_plan", "vp_targets", "army_layout", "doctrines", "event_rules",
              "predicates", "templates"):
        if not counts[k] and k not in plan:
            del counts[k]
    return rep, counts
