#!/usr/bin/env python3
"""Validate a HOI4 Live Tools standing plan against the installed game files.

Usage:
    validate_plan.py PLAN.json [--game-dir DIR] [--tag GER] [--strict]

Checks (errors unless marked warning):
  * research_priority: every key is a technology, no duplicates, prerequisites are
    researched earlier in the list or at the start (warning: the game simply refuses
    a locked key and the runner moves on, but a long locked head wastes days);
    techs more than one year ahead of the plan's stop date are flagged (warning).
  * focus_path / focus_fallback: every key is a focus of one tree, no duplicates,
    each prerequisite block has a member earlier in the path, and no two mutually
    exclusive focuses are both listed (the second can never start).
  * production / production_priority: equipment keys exist (production_priority
    entries are prefixes).
  * construction: building keys exist, states are known ids or exact state names,
    no duplicates inside a list.
  * politics: category/slot shape and that the idea is a known idea (laws, designers
    from common/ideas) or a known character id / idea_token (advisors).
  * decisions: every key exists in common/decisions.
  * event options: shape only.

Stdlib only, Python 3.9+. Exit status 0 when there are no errors.

Optional plan key "_validator" (ignored by the runner):
  {"tag": "GER", "start_date": "1936.1.1", "stop_date": "1938.1.1",
   "completed_techs": [...], "completed_focuses": [...],
   "history_file": "history/countries/GER - Germany.txt"}
"""
import argparse
import json
import os
import re
import sys
from collections import defaultdict

DEFAULT_GAME_DIR = os.path.expanduser(
    "~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV"
)

# --------------------------------------------------------------------------
# Clausewitz script reader: tiny, enough for definitions (no scripting logic).

_TOKEN = re.compile(r'"[^"]*"|[{}]|[<>=]=?|[^\s{}=<>"#]+|#[^\n]*')


def tokenize(text):
    out = []
    for m in _TOKEN.finditer(text):
        t = m.group(0)
        if t.startswith("#"):
            continue
        out.append(t)
    return out


def parse(text):
    """Return a block: list of (key, value) where value is str or block (list).
    Bare values inside a block are stored as (None, token)."""
    toks = tokenize(text.lstrip("﻿"))
    pos = 0

    def block(top):
        nonlocal pos
        items = []
        while pos < len(toks):
            t = toks[pos]
            if t == "}":
                pos += 1
                if top:
                    continue  # tolerate a stray brace at top level
                return items
            pos += 1
            if t in ("=", "<", ">", "<=", ">=", "{"):
                if t == "{":  # anonymous nested block
                    items.append((None, block(False)))
                continue
            if pos < len(toks) and toks[pos] in ("=", "<", ">", "<=", ">="):
                pos += 1
                if pos < len(toks) and toks[pos] == "{":
                    pos += 1
                    items.append((t.strip('"'), block(False)))
                elif pos < len(toks):
                    items.append((t.strip('"'), toks[pos].strip('"')))
                    pos += 1
            else:
                items.append((None, t.strip('"')))
        return items

    return block(True)


def get(items, key):
    return [v for k, v in items if k == key]


def get1(items, key, default=None):
    for k, v in items:
        if k == key:
            return v
    return default


def read(path):
    with open(path, encoding="utf-8-sig", errors="replace") as fh:
        return fh.read()


def txt_files(directory):
    if not os.path.isdir(directory):
        return []
    return sorted(
        os.path.join(directory, f)
        for f in os.listdir(directory)
        if f.endswith(".txt")
    )


# --------------------------------------------------------------------------
# Game data


class Game:
    def __init__(self, root):
        self.root = root
        self._cache = {}

    def path(self, *parts):
        return os.path.join(self.root, *parts)

    def cached(self, name, fn):
        if name not in self._cache:
            self._cache[name] = fn()
        return self._cache[name]

    # technologies: key -> {"parents": set, "deps": set, "year": int, "cost": float}
    def techs(self):
        def load():
            techs = {}
            leads = defaultdict(set)
            for f in txt_files(self.path("common", "technologies")):
                for k, v in parse(read(f)):
                    if k != "technologies" or not isinstance(v, list):
                        continue
                    for name, body in v:
                        if not name or not isinstance(body, list):
                            continue
                        deps = set()
                        dep = get1(body, "dependencies")
                        if isinstance(dep, list):
                            deps = {a for a, _ in dep if a}
                        for path in get(body, "path"):
                            if isinstance(path, list):
                                t = get1(path, "leads_to_tech")
                                if isinstance(t, str):
                                    leads[t].add(name)
                        year = get1(body, "start_year", "0")
                        cost = get1(body, "research_cost", "0")
                        techs[name] = {
                            "deps": deps,
                            "year": int(year) if str(year).isdigit() else 0,
                            "cost": cost,
                            "file": os.path.basename(f),
                        }
            for name, info in techs.items():
                info["parents"] = leads.get(name, set())
            return techs

        return self.cached("techs", load)

    def start_techs(self, history_file):
        def load():
            p = self.path(history_file)
            if not os.path.exists(p):
                return set()
            have = set()

            def walk(items):
                for k, v in items:
                    if k and re.match(r"^\d{4}\.\d+\.\d+$", k):
                        continue  # later bookmark blocks (1939.1.1 ...)
                    if k == "set_technology" and isinstance(v, list):
                        have.update(a for a, b in v if a and b == "1")
                    elif isinstance(v, list):
                        walk(v)

            walk(parse(read(p)))
            return have

        return self.cached("start:" + history_file, load)

    # focuses: tree id -> {focus id -> info}
    def focus_trees(self):
        def load():
            trees = {}
            for f in txt_files(self.path("common", "national_focus")):
                for k, v in parse(read(f)):
                    if k != "focus_tree" or not isinstance(v, list):
                        continue
                    tid = get1(v, "id", os.path.basename(f))
                    focuses = {}
                    for fk, fv in v:
                        if fk != "focus" or not isinstance(fv, list):
                            continue
                        fid = get1(fv, "id")
                        if not fid:
                            continue
                        prereq = []
                        for pb in get(fv, "prerequisite"):
                            if isinstance(pb, list):
                                prereq.append({x for a, x in pb if a == "focus"})
                        excl = get1(fv, "mutually_exclusive")
                        exclusive = set()
                        if isinstance(excl, list):
                            exclusive = {x for a, x in excl if a == "focus"}
                        focuses[fid] = {
                            "prereq": prereq,
                            "exclusive": exclusive,
                            "cost": get1(fv, "cost", "1"),
                        }
                    trees[tid] = focuses
            return trees

        return self.cached("focus", load)

    def equipment(self):
        def load():
            keys = set()
            for f in txt_files(self.path("common", "units", "equipment")):
                text = read(f)
                for k, v in parse(text):
                    if k == "equipments" and isinstance(v, list):
                        for name, body in v:
                            if name and isinstance(body, list):
                                keys.add(name)
                    elif k == "duplicate_archetypes" and isinstance(v, list):
                        # generated families, e.g. small_plane_cas_airframe_1..N
                        for name, body in v:
                            if not name or not isinstance(body, list):
                                continue
                            keys.add(name)
                            names = get1(body, "variant_name")
                            if isinstance(names, list):
                                keys.update(a for a, _ in names if a)
                # variant names produced by the game (derived_variant_name)
                keys.update(re.findall(r"derived_variant_name\s*=\s*(\w+)", text))
            return keys

        return self.cached("equipment", load)

    def buildings(self):
        def load():
            keys = set()
            for f in txt_files(self.path("common", "buildings")):
                for k, v in parse(read(f)):
                    if k == "buildings" and isinstance(v, list):
                        keys.update(n for n, b in v if n and isinstance(b, list))
            return keys

        return self.cached("buildings", load)

    def states(self):
        """id -> {"name": english, "names": set(all localised), "owner": str}"""

        def load():
            names = defaultdict(set)
            loc = self.path("localisation", "english", "state_names_l_english.yml")
            if os.path.exists(loc):
                for line in read(loc).splitlines():
                    m = re.match(r'\s*(?:[A-Z]{2,4}_)?STATE_(\d+):\d*\s+"(.*)"', line)
                    if m:
                        names[int(m.group(1))].add(m.group(2))
            out = {}
            sdir = self.path("history", "states")
            if os.path.isdir(sdir):
                for f in sorted(os.listdir(sdir)):
                    if not f.endswith(".txt"):
                        continue
                    body = parse(read(os.path.join(sdir, f)))
                    st = get1(body, "state")
                    if not isinstance(st, list):
                        continue
                    sid = get1(st, "id")
                    if not sid or not str(sid).isdigit():
                        continue
                    hist = get1(st, "history", [])
                    out[int(sid)] = {
                        "owner": get1(hist, "owner", "") if isinstance(hist, list) else "",
                        "category": get1(st, "state_category", ""),
                        "names": names.get(int(sid), set()),
                        "file": f,
                    }
            return out

        return self.cached("states", load)

    def ideas(self):
        """Known idea keys (ideas files), character ids and advisor idea tokens."""

        def load():
            keys = set()
            for f in txt_files(self.path("common", "ideas")):
                for k, v in parse(read(f)):
                    if k != "ideas" or not isinstance(v, list):
                        continue
                    for cat, body in v:
                        if isinstance(body, list):
                            keys.update(n for n, b in body if n and isinstance(b, list))
            chars = set()
            tokens = set()
            for f in txt_files(self.path("common", "characters")):
                for k, v in parse(read(f)):
                    if k != "characters" or not isinstance(v, list):
                        continue
                    for cid, body in v:
                        if not cid or not isinstance(body, list):
                            continue
                        adv = get1(body, "advisor")
                        if isinstance(adv, list):
                            chars.add(cid)
                            tok = get1(adv, "idea_token")
                            if tok:
                                tokens.add(tok)
            return keys, chars, tokens

        return self.cached("ideas", load)

    def decisions(self):
        def load():
            keys = set()
            for f in txt_files(self.path("common", "decisions")):
                for k, v in parse(read(f)):
                    if isinstance(v, list) and k and not k.startswith("@"):
                        keys.update(n for n, b in v if n and isinstance(b, list))
            for f in txt_files(self.path("common", "decisions", "categories")):
                pass
            return keys

        return self.cached("decisions", load)


# --------------------------------------------------------------------------
# Validation


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


def check_research(game, plan, meta, rep):
    keys = strings(plan, "research_priority")
    if "research_priority" in plan and not isinstance(plan["research_priority"], list):
        rep.error("research_priority must be a list")
    dup_check(rep, "research_priority", keys)
    techs = game.techs()
    have = set(meta.get("completed_techs", []))
    have |= game.start_techs(meta.get("history_file", ""))
    stop_year = date_year(meta.get("stop_date"), 1938)
    done = set(have)
    unknown = 0
    for i, k in enumerate(keys):
        info = techs.get(k)
        if info is None:
            rep.error(f"research_priority[{i}]: unknown technology {k}")
            unknown += 1
            continue
        if k in have:
            rep.warn(f"research_priority[{i}] {k}: already researched at start")
        ready_parents = (not info["parents"]) or bool(info["parents"] & done)
        missing_deps = info["deps"] - done
        if not ready_parents or missing_deps:
            need = sorted(info["parents"]) if not ready_parents else []
            need += sorted(missing_deps)
            rep.error(
                f"research_priority[{i}] {k}: prerequisite not earlier in list or at "
                f"start (needs {' / '.join(need)})"
            )
        window = int(meta.get("ahead_check_first", 45))
        limit = stop_year + (1 if i < window else 2)
        if info["year"] > limit:
            rep.warn(
                f"research_priority[{i}] {k}: start_year {info['year']} is far ahead "
                f"of {stop_year} (ahead-of-time penalty)"
            )
        done.add(k)
    return len(keys) - unknown


def check_focus(game, plan, meta, rep):
    path = strings(plan, "focus_path")
    fallback = strings(plan, "focus_fallback")
    dup_check(rep, "focus_path", path)
    trees = game.focus_trees()
    if not path and not fallback:
        return
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
        n = line.get("factories")
        if not isinstance(n, int) or not 1 <= n <= 150:
            rep.error(f"production[{i}] {k}: factories must be an integer 1..150")
        seen.append(k)
    dup_check(rep, "production", seen)
    for i, prefix in enumerate(strings(plan, "production_priority")):
        if not any(e.startswith(prefix) for e in equip):
            rep.error(f"production_priority[{i}]: no equipment starts with {prefix}")
    dup_check(rep, "production_priority", strings(plan, "production_priority"))


def check_construction(game, plan, rep):
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
        c = e.get("count", 1)
        if not isinstance(c, int) or not 1 <= c <= 10:
            rep.error(f"construction[{i}] {b}: count must be an integer 1..10")


def check_politics(game, plan, meta, rep):
    picks = plan.get("politics", [])
    if picks and not isinstance(picks, list):
        rep.error("politics must be a list")
        return
    ideas, chars, tokens = game.ideas()
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


def check_training(game, plan, rep):
    tr = plan.get("training")
    if tr is None:
        return
    if not isinstance(tr, dict):
        rep.error("training must be an object")
        return
    for key in ("queue", "batch"):
        if key in tr and (not isinstance(tr[key], int) or tr[key] < 1):
            rep.error(f"training.{key} must be a positive integer")
    dep = tr.get("deploy_state")
    if dep is None:
        rep.error("training.deploy_state is required (a line without a location never deploys)")
    elif dep not in game.states():
        rep.error(f"training.deploy_state: unknown state id {dep}")
    name = tr.get("template")
    if not isinstance(name, str) or not name:
        rep.error("training.template is required")
        return
    units = game.path("history", "units")
    found = False
    if os.path.isdir(units):
        needle = '"' + name + '"'
        for f in sorted(os.listdir(units)):
            if f.endswith(".txt") and needle in read(os.path.join(units, f)):
                found = True
                break
    if not found:
        rep.warn(
            f"training.template {name!r} is not a template name in history/units "
            f"(fine if you create it in game before the plan runs)"
        )


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


def validate(plan, game):
    rep = Report()
    meta = dict(plan.get("_validator", {}))
    tag = meta.get("tag", "GER")
    meta.setdefault("history_file", f"history/countries/{tag} - Germany.txt")
    if not os.path.exists(game.path(meta["history_file"])) and tag == "GER":
        meta["history_file"] = "history/countries/GER - Germany.txt"
    counts = {"research": check_research(game, plan, meta, rep)}
    check_focus(game, plan, meta, rep)
    check_production(game, plan, rep)
    check_construction(game, plan, rep)
    check_politics(game, plan, meta, rep)
    check_decisions(game, plan, rep)
    check_training(game, plan, rep)
    check_misc(plan, rep)
    counts.update(
        focus=len(strings(plan, "focus_path")),
        production=len(plan.get("production", []) or []),
        construction=len(plan.get("construction", []) or []),
        politics=len(plan.get("politics", []) or []),
        decisions=len(strings(plan, "decisions")),
    )
    return rep, counts


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("plan")
    ap.add_argument("--game-dir", default=os.environ.get("HOI4_DIR", DEFAULT_GAME_DIR))
    ap.add_argument("--strict", action="store_true", help="treat warnings as errors")
    args = ap.parse_args(argv)
    with open(args.plan, encoding="utf-8") as fh:
        plan = json.load(fh)
    if not isinstance(plan, dict):
        print("plan must be a JSON object", file=sys.stderr)
        return 2
    if not os.path.isdir(args.game_dir):
        print(f"game directory not found: {args.game_dir}", file=sys.stderr)
        return 2
    rep, counts = validate(plan, Game(args.game_dir))
    for w in rep.warnings:
        print("warning:", w)
    for e in rep.errors:
        print("error:", e)
    print(
        "checked: "
        + ", ".join(f"{k}={v}" for k, v in counts.items())
        + f"; {len(rep.errors)} error(s), {len(rep.warnings)} warning(s)"
    )
    bad = rep.errors or (args.strict and rep.warnings)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
