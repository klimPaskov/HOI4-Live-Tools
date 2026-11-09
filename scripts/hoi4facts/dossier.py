"""Country dossier: deterministic facts for one tag at one start date (design section 2).

    build(tag, date, game_dir, mods, dlcs, mode="fair", enemies=None) -> dict
    render_md(dossier) -> str          the digest a model reads in full (sections 0-9, ~30k tokens cap)
    write(dossier, out_dir)            RUN/dossier/{dossier.json, dossier.md, manifest.json, graph.json.gz}

Every fact carries its source ({"v", "src": "file:line", "mod"}) or a "src" field on its row;
derived numbers carry "d": "<method>". Enemy facts are tagged fair public | aggregate | private:
the FAIR profile (default) keeps public and aggregate (division count and template names at start
are aggregate) and drops private ones (per-division positions, common/ai_strategy); META keeps all.

Section budgets are in bytes (about 4 bytes per token); long lists end with a "see facts ..." pointer.
"""
import hashlib
import json
import os
import re
import shutil

from . import focus as focusmod
from . import mapgraph
from . import research as researchmod
from . import scenario as scenariomod
from . import theatre as theatremod
from .game import Game
from .script import get1
from .world import World, gate_text, xp_in

GENERATOR = "hoi4facts.dossier 1"
SECTIONS = [
    (0, "run", "Run context", 1200),
    (1, "country", "Country at start", 9000),
    (2, "states", "Own states", 12000),
    (3, "neighbours", "Neighbours and enemies", 16000),
    (4, "hooks", "Scenario hooks", 16000),
    (5, "focus", "Focus tree", 24000),
    (6, "research", "Research calendar and doctrines", 12000),
    (7, "politics", "Laws, advisors, decisions", 10500),
    (8, "oob", "Order of battle and commanders", 9000),
    (9, "theatres", "Theatres", 12000),
]
HARD_CAP = 120_000


# ---------------------------------------------------------------- run context

def game_version(game_dir):
    try:
        with open(os.path.join(game_dir, "launcher-settings.json"), encoding="utf-8-sig") as fh:
            data = json.load(fh)
        return {"raw": data.get("rawVersion"), "version": data.get("version"), "src": "launcher-settings.json"}
    except (OSError, ValueError):
        return {"raw": None, "version": None, "src": None}


def dlcs_auto(game_dir, userdir=None):
    """Active DLC display names: the user folder's logs/system.log "Active DLC:" lines when present,
    else every installed dlc/*/*.dlc name minus dlc_load.json disabled_dlcs. Integrated DLCs are
    added by World. Returns (names, source)."""
    if userdir:
        log = os.path.join(userdir, "logs", "system.log")
        try:
            with open(log, encoding="utf-8", errors="replace") as fh:
                names = re.findall(r"Active DLC: (.+?)\s*$", fh.read(), re.M)
            if names:
                return list(dict.fromkeys(names)), "logs/system.log"
        except OSError:
            pass
    names = []
    base = os.path.join(game_dir, "dlc")
    disabled = set()
    if userdir:
        try:
            with open(os.path.join(userdir, "dlc_load.json"), encoding="utf-8-sig") as fh:
                disabled = set(json.load(fh).get("disabled_dlcs") or [])
        except (OSError, ValueError):
            pass
    for d in sorted(os.listdir(base)) if os.path.isdir(base) else []:
        folder = os.path.join(base, d)
        if not os.path.isdir(folder):
            continue
        for f in sorted(os.listdir(folder)):
            if f.endswith(".dlc"):
                with open(os.path.join(folder, f), encoding="utf-8-sig", errors="replace") as fh:
                    m = re.search(r'^name\s*=\s*"([^"]+)"', fh.read(), re.M)
                if m and f"dlc/{d}/{f}" not in disabled and m.group(1) not in disabled:
                    names.append(m.group(1))
    return names, "installed dlc folders"


def content_hash(world):
    h = hashlib.sha1()
    h.update(world.content.signature().encode())
    h.update(json.dumps([sorted(world.dlcs), world.tag, world.date, world.mode, GENERATOR]).encode())
    return h.hexdigest()[:20]


# ---------------------------------------------------------------- helpers

def idea_exits(world, tag):
    """{idea: [where it is removed or swapped: "focus:ID", "decision:ID"]} over the loaded focus tree
    and the tag's decisions."""
    out = {}

    def scan(block, label):
        for k, v in block:
            kl = (k or "").lower()
            if kl in ("remove_ideas", "remove_idea"):
                for x in ([v] if isinstance(v, str) else [b for a, b in v if a is None]):
                    out.setdefault(x, []).append(label)
            elif kl == "swap_ideas" and isinstance(v, list):
                x = get1(v, "remove_idea")
                if x:
                    out.setdefault(x, []).append(label + " (swap to %s)" % get1(v, "add_idea"))
            elif isinstance(v, list):
                scan(v, label)

    tid = focusmod.tree_id(world, tag)
    trees, shared = focusmod._focus_blocks(world)
    for b, p, ln in (trees.get(tid) or {}).get("focuses", []):
        fid = get1(b, "id")
        scan(b, f"focus:{fid}")
    for d in world.decision_index():
        if d["id"].startswith(tag + "_"):
            scan(d["block"], f"decision:{d['id']}")
    return out


def _vp_short(vp):
    return {"total": vp["total"], "capital": vp["capital"], "thresholds": vp["thresholds"],
            "base_limit": vp["base_limit"], "modifiers": vp["modifiers"], "d": vp["d"]}


def _enemy_oob(world, tag):
    """Aggregate OOB facts (FAIR aggregate) and per-division positions (FAIR private)."""
    o = world.oob(tag)
    by_t = {}
    for d in o["divisions"]:
        by_t[d["template"]] = by_t.get(d["template"], 0) + 1
    agg = {"divisions": len(o["divisions"]), "templates": sorted(t for t in o["templates"] if t),
           "per_template": by_t, "air_planes": sum(a["amount"] or 0 for a in o["air"]),
           "ships": sum(sum(f["ships"].values()) for f in o["fleets"]), "files": o["files"], "fair": "aggregate"}
    out = {"aggregate": agg}
    if world.visible("private"):
        out["private"] = {"divisions": o["divisions"], "air": o["air"], "fair": "private"}
    return out


STATES_METHOD = ("rank: core first, then a free static slot, infrastructure, free static slots (state_category "
                 "local_building_slots minus shared-slot buildings), enemy distance in land province steps "
                 "(derive.distances BFS over passable borders and straits)")


def state_rows(w, tag, enemies=None):
    """Own states at the start ranked for factory building (STATES_METHOD)."""
    states = w.states()
    enemies = w.at_war_with(tag) if enemies is None else enemies
    enemy_steps = w.province_steps(w.provinces_of(enemies)) if enemies else {}
    foreign = [p for sid, r in states.items() if r["owner"] and r["owner"] != tag for p in r["provinces"]]
    border_steps = w.province_steps(foreign)
    occ = w.occupation_start(tag)
    own = []
    for sid in w.owned_states(tag):
        r = states[sid]
        es = [enemy_steps[p] for p in r["provinces"] if p in enemy_steps]
        bs = [border_steps[p] for p in r["provinces"] if p in border_steps]
        nbs = sorted({states[n]["owner"] for n in w.state_borders().get(sid, {}) if states.get(n, {}).get("owner")
                      and states[n]["owner"] != tag})
        own.append({"id": sid, "name": r["name"], "category": r["category"], "slots": r["slots"],
                    "free_slots": r["free_slots"], "infrastructure": r["infrastructure"],
                    "civ": r["buildings"].get("industrial_complex", 0), "mil": r["buildings"].get("arms_factory", 0),
                    "dock": r["buildings"].get("dockyard", 0), "core": tag in r["cores"],
                    "resources": r["resources"], "vp": sum(r["vps"].values()), "manpower": r["manpower"],
                    "naval_base": max([b.get("naval_base", 0) for b in r["province_buildings"].values()] or [0]),
                    "enemy_steps": min(es) if es else None, "border_steps": min(bs) if bs else None,
                    "borders": nbs, "occupation": occ.get(sid), "src": r["file"], "mod": r["mod"]})
    own.sort(key=lambda s: (not s["core"], not (s["free_slots"] or 0) > 0, -s["infrastructure"],
                            -(s["free_slots"] or 0), -(s["enemy_steps"] if s["enemy_steps"] is not None else 99),
                            s["id"]))
    for i, s in enumerate(own, 1):
        s["rank"] = i
    return own


# ---------------------------------------------------------------- build

def build(tag, date="1936.1.1", game_dir=None, mods=(), dlcs=None, mode="fair", enemies=None, userdir=None,
          cache_dir=None, horizon_days=None):
    game_dir = game_dir or mapgraph.DEFAULT_GAME_DIR
    dlc_src = "given"
    if dlcs is None or dlcs == "auto":
        dlcs, dlc_src = dlcs_auto(game_dir, userdir)
    game = Game(game_dir, mods, cache_dir=cache_dir)
    w = World(game, tag, date, dlcs, mode)
    return build_world(w, enemies=enemies, dlc_src=dlc_src, horizon_days=horizon_days)


def build_world(w, enemies=None, dlc_src="given", horizon_days=None):
    tag = w.tag
    game = w.game
    c = w.country(tag)
    states = w.states()
    at_war = w.at_war_with(tag)
    enemies = list(enemies) if enemies else at_war
    d = {"_meta": {"generator": GENERATOR, "tag": tag, "date": w.date, "mode": w.mode,
                   "fair_levels_shown": ["public", "aggregate"] + (["private"] if w.mode != "fair" else []),
                   "content_hash": content_hash(w)}}

    # 0 run context
    graph = w.graph()
    d["run"] = {"game_version": game_version(game.root), "dlcs": w.active_dlcs, "dlc_source": dlc_src,
                "integrated_dlcs": list(game.integrated_dlcs()),
                "mods": [{"name": m.name, "path": m.path, "replace_path": m.replace_paths} for m in game.content.mods],
                "tag": tag, "name": w.country_name(tag), "date": w.date, "mode": w.mode,
                "graph": {"signature": graph.get("signature") if graph else None}}

    # 1 country at start
    exits = idea_exits(w, tag)
    spirits = []
    for f in c["ideas"]:
        row = w.idea_row(f["v"], tag) or {"id": f["v"], "name": w.name(f["v"]), "category": None, "law": False}
        row["set_by"] = f["src"]
        if exits.get(f["v"]):
            row["exits"] = exits[f["v"]][:6]
        spirits.append(row)
    slots_def, slots_fact = w.define("BASE_RESEARCH_SLOTS", 2)
    d["country"] = {
        "name": c["name"], "history_file": c["history_file"], "capital": c["capital"],
        "capital_state": w.state_name(c["capital"]["v"]) if c["capital"] else None,
        "research_slots": c["research_slots"] or dict(slots_fact or {}, d="BASE_RESEARCH_SLOTS default"),
        "stability": c["stability"], "war_support": c["war_support"], "convoys": c["convoys"],
        "political_power": c["political_power"], "politics": c["politics"], "popularities": c["popularities"],
        "laws": [s for s in spirits if s.get("law")], "spirits": [s for s in spirits if not s.get("law")],
        "variables": c["variables"], "flags": c["flags"], "global_flags": c["global_flags"],
        "dynamic_modifiers": c["dynamic_modifiers"], "doctrines": c["doctrines"], "oobs": c["oobs"],
        "guarantees": c["guarantees"], "faction": c["faction"], "subjects": c["subjects"],
        "events_scheduled": c["events"], "start_techs": c["techs"], "fair": "public"}
    xp = w.experience(tag)
    xp["focuses"] = focusmod.xp_rewards(w, tag)
    xp["decisions"] = []
    for dd in w.decision_index():
        if dd["id"].startswith(tag + "_"):
            got = xp_in(get1(dd["block"], "complete_effect"))
            if got:
                xp["decisions"].append(dict(got, decision=dd["id"], src=w.src(dd["path"], dd["line"])))
    xp["d"] = ("start = history army/air/navy_experience up to the start; spirits = experience_gain_* modifiers of the "
               "start ideas (daily when the key has no _factor); focus/decision rewards = army/air/navy_experience in "
               "the completion reward / complete_effect, every branch counted (upper bound)")
    d["country"]["experience"] = xp

    # 2 own states, ranked for factories
    d["states"] = {"rows": state_rows(w, tag, enemies), "d": STATES_METHOD, "fair": "public"}

    # 3 neighbours and enemies
    nbs = w.neighbours(tag)
    guaranteed = {g["v"] for g in c["guarantees"]}
    rows = []
    for t, r in nbs.items():
        rel = []
        if t in at_war:
            rel.append("war")
        if t in guaranteed:
            rel.append("we guarantee")
        if tag in {g["v"] for g in w.country(t)["guarantees"]}:
            rel.append("guarantees us")
        vp = w.vp(t)
        rows.append({"tag": t, "name": w.country_name(t), "relation": rel, "border_states": r["states"],
                     "our_states": r["ours"], "vp_total": vp["total"], "capital": vp["capital"],
                     "ruling_party": w.ruling_party(t), "fair": "public"})
    enemy_rows = []
    for t in enemies:
        vp = w.vp(t)
        ec = w.country(t)
        row = {"tag": t, "name": w.country_name(t), "at_war": t in at_war, "vp": vp, "surrender": _vp_short(vp),
               "capital": ec["capital"], "stability": ec["stability"], "war_support": ec["war_support"],
               "ideas": [dict(w.idea_row(i["v"], t) or {"id": i["v"]}, set_by=i["src"]) for i in ec["ideas"]],
               "oob": _enemy_oob(w, t), "fair": "public"}
        # An enemy's research is not visible to a player (FAIR: no hidden enemy intel), so its tech
        # level is private: shown in meta mode only.
        if w.visible("private"):
            row["tech_level"] = w.tech_level(t)
        row["bop"] = [{"id": b["id"], "value": b["start"].get("value"), "range": b["start"].get("range"),
                       "src": b["start"].get("src")} for b in scenariomod.bop(w, t) if b["start"].get("active")]
        if w.visible("private"):
            row["ai_strategy"] = scenariomod.ai_strategy(w, t)
        enemy_rows.append(row)
    d["neighbours"] = {"rows": rows, "fair": "public"}
    d["enemies"] = enemy_rows

    # 4 scenario hooks (ours and each enemy's)
    d["hooks"] = {"own": scenariomod.hooks(w, tag), "enemies": {t: scenariomod.hooks(w, t) for t in enemies},
                  "never_take": scenariomod.never_take(w, tag), "never_take_d": scenariomod.NEVER_METHOD}

    # 5 focus tree
    tree = focusmod.tree(w, tag)
    early = focusmod.earliest(tree)
    bo = focusmod.branch_of(tree)
    d["focus"] = {"id": tree["id"], "src": tree["src"], "mod": tree["mod"], "days_per_point": tree["days_per_point"],
                  "hidden": tree["hidden"], "roots": focusmod.roots(tree), "count": len(tree["focuses"]),
                  "focuses": [dict(r, earliest_end_days=early.get(fid), branch=bo.get(fid))
                              for fid, r in tree["focuses"].items()],
                  "d": "earliest_end_days: lower bound if each prerequisite chain ran alone; days = cost x FOCUS_POINT_DAYS",
                  "fair": "public"}

    # 6 research
    y0 = int(w.date.split(".")[0])
    cal = researchmod.calendar(w, tag, w.date, (y0, y0 + 2))
    doct = game.doctrines(w.ctx)
    d["research"] = {"slots": d["country"]["research_slots"], "start_techs": c["techs"],
                     "calendar": cal, "multi_parent_note": "chain_any differs from chain_all where a tech has several "
                     "parents (design finding 3: live rule not verified)",
                     "doctrines": {"current": c["doctrines"],
                                   "grand": [{"id": k, "folder": v["folder"], "xp_cost": v["xp_cost"],
                                              "name": w.name(k)} for k, v in sorted(doct["grand"].items())
                                             if doct["folders"].get(v["folder"]) is not False]},
                     "fair": "public"}

    # 7 laws, advisors, decisions
    advisors = []
    for ch in w.characters(tag, "advisor"):
        for r in ch["roles"]:
            if r["role"] == "advisor":
                advisors.append({"id": ch["id"], "name": ch["name"], "slot": r.get("slot"), "cost": r.get("cost"),
                                 "traits": r.get("traits", []), "available": r.get("available"),
                                 "idea_token": r.get("idea_token"), "src": ch["src"]})
    decs = [x for x in w.decisions(tag, missions=False) if x.get("specific")]
    never = {x["id"]: x["reasons"] for x in d["hooks"]["never_take"]}
    for x in decs:
        if x["id"] in never:
            x["never_take"] = never[x["id"]]
    bops = []
    for b in scenariomod.bop(w, tag):
        if b["start"].get("active") or b["gained_by"]:
            b = dict(b, movers=b["movers"][:30])
            bops.append(b)
    d["politics"] = {"laws": w.laws(tag), "advisors": advisors, "decisions": decs,
                     "missions": w.decisions(tag, missions=True), "bop": bops, "fair": "public"}

    # 8 OOB and commanders
    o = w.oob(tag)
    per_state = {}
    for dv in o["divisions"]:
        key = dv["state"]
        per_state.setdefault(key, {})
        per_state[key][dv["template"]] = per_state[key].get(dv["template"], 0) + 1
    cmd = []
    for ch in w.characters(tag, "general") + w.characters(tag, "admiral"):
        for r in ch["roles"]:
            if r["role"] in ("corps_commander", "field_marshal", "navy_leader"):
                cmd.append({"id": ch["id"], "name": ch["name"], "role": r["role"], "recruited": ch["recruited"],
                            "skill": r.get("skill"), "attack": r.get("attack"), "defense": r.get("defense"),
                            "planning": r.get("planning"), "logistics": r.get("logistics"),
                            "maneuvering": r.get("maneuvering"), "coordination": r.get("coordination"),
                            "traits": r.get("traits", []), "visible": r.get("visible"), "src": ch["src"]})
    d["oob"] = {"files": o["files"], "templates": list(o["templates"].values()),
                "divisions_per_state": [{"state": s, "name": w.state_name(s) if s else None, "templates": t}
                                        for s, t in sorted(per_state.items(), key=lambda kv: (kv[0] is None, kv[0] or 0))],
                "divisions": o["divisions"], "air": o["air"], "fleets": o["fleets"], "production": o["production"],
                "commanders": cmd, "fair": "public"}

    # 9 theatres
    d["theatres"] = [theatremod.theatre(w, tag, t) for t in enemies]
    return d


# ---------------------------------------------------------------- markdown

class Section:
    def __init__(self, num, title, budget):
        self.num, self.title, self.budget = num, title, budget
        self.lines = []
        self.size = 0
        self.dropped = {}
        self.counts = {}

    def add(self, line, group=None, more=None, cap=None):
        """Add a line unless the budget is spent (or `group` already has `cap` lines); a dropped line
        counts toward `group`'s "... N more: see <more>" pointer."""
        if group:
            self.counts[group] = self.counts.get(group, 0) + 1
        if self.size + len(line) + 1 > self.budget or (cap is not None and self.counts[group] > cap):
            if group:
                n, ptr = self.dropped.get(group, (0, more))
                self.dropped[group] = (n + 1, more or ptr)
            return False
        self.lines.append(line)
        self.size += len(line) + 1
        return True

    def body(self):
        out = list(self.lines)
        for group, (n, ptr) in self.dropped.items():
            out.append(f"- ... {n} more {group}" + (f": see `{ptr}`" if ptr else ""))
        return out

    def text(self):
        return "\n".join([f"## {self.num}. {self.title}", ""] + self.body()).rstrip() + "\n"


def _s(f):
    """Source suffix of a fact or row."""
    if not f:
        return ""
    src = f.get("src") if isinstance(f, dict) else None
    mod = f.get("mod") if isinstance(f, dict) else None
    if not src:
        return ""
    return f" [{src}{' mod ' + mod if mod else ''}]"


def _v(f):
    return f.get("v") if isinstance(f, dict) else f


def _pct(x):
    return f"{round(x * 100)}%" if isinstance(x, (int, float)) else "?"


def render_md(d):
    tag = d["_meta"]["tag"]
    out = [f"# Dossier {tag} {d['run']['name']} at {d['_meta']['date']} ({d['_meta']['mode'].upper()})", "",
           "Facts from the effective game files, each with `[file:line]`. Derived numbers name their method. "
           "Deeper lookups: `python3 scripts/hoi4facts/facts.py <command>` (focus, tech, state, province, route, vp, "
           "oob, characters, laws, idea, decisions, missions, hooks, event, grep, defines, terrain-width, units, weather, "
           "bop, occupation-laws, ports; `calc.py equipment` for template width).", ""]
    secs = {num: Section(num, title, budget) for num, _, title, budget in SECTIONS}
    _md_run(d, secs[0])
    _md_country(d, secs[1])
    _md_states(d, secs[2])
    _md_neighbours(d, secs[3])
    _md_hooks(d, secs[4])
    _md_focus(d, secs[5])
    _md_research(d, secs[6])
    _md_politics(d, secs[7])
    _md_oob(d, secs[8])
    _md_theatres(d, secs[9])
    for num, _, _, _ in SECTIONS:
        out.append(secs[num].text())
    text = "\n".join(out)
    if len(text.encode("utf-8")) > HARD_CAP:
        text = text.encode("utf-8")[:HARD_CAP - 200].decode("utf-8", "ignore") + \
            "\n\n... cut at the hard cap; see dossier.json and `facts` for the rest\n"
    return text


def _md_run(d, s):
    r = d["run"]
    v = r["game_version"]
    s.add(f"- Game: {v.get('version') or v.get('raw')} [{v.get('src')}]")
    s.add(f"- Tag {r['tag']} ({r['name']}), start {r['date']}, mode {r['mode'].upper()} "
          f"(FAIR shows public + aggregate enemy facts; private hidden)")
    s.add(f"- Active DLC ({len(r['dlcs'])}, from {r['dlc_source']}): " + ", ".join(r["dlcs"]))
    s.add("- Integrated (always on): " + ", ".join(r["integrated_dlcs"]))
    s.add("- Mods: " + (", ".join(f"{m['name']} (replace_path {m['replace_path']})" if m["replace_path"] else m["name"]
                                  for m in r["mods"]) or "none"))
    s.add(f"- Content hash {d['_meta']['content_hash']}; map graph {r['graph']['signature']}")


def _md_country(d, s):
    c = d["country"]
    t = d["_meta"]["tag"]
    s.add(f"- Capital: state {_v(c['capital'])} {c['capital_state']}{_s(c['capital'])}")
    s.add(f"- Research slots {_v(c['research_slots'])}{_s(c['research_slots'])}; stability {_pct(_v(c['stability']))}"
          f"{_s(c['stability'])}; war support {_pct(_v(c['war_support']))}{_s(c['war_support'])}"
          + (f"; convoys {_v(c['convoys'])}" if c["convoys"] else ""))
    _md_xp(c.get("experience"), s)
    pol = c["politics"]
    if pol:
        s.add("- Politics: " + ", ".join(f"{k} {_v(v)}" for k, v in pol.items()) +
              (_s(next(iter(pol.values()))) if pol else "") +
              ("; popularity " + ", ".join(f"{k} {_v(v)}" for k, v in c["popularities"].items()) if c["popularities"] else ""))
    s.add("- Laws at start: " + "; ".join(f"{x['category']} = {x['name']} ({x['id']})" for x in c["laws"]))
    s.add("")
    s.add("Spirits at start (modifier; exits = what removes it):")
    for x in c["spirits"]:
        line = f"- {x['id']} \"{x.get('name')}\"" + (f": {x['modifier'][:160]}" if x.get("modifier") else "") + \
            (f"; exits: {', '.join(x['exits'][:3])}" if x.get("exits") else "") + f" [{x.get('src') or x.get('set_by')}]"
        s.add(line, "spirits", f"facts idea <id>")
    for f in c["guarantees"]:
        s.add(f"- Guarantees {f['v']}{_s(f)}")
    for f in c["faction"]:
        s.add(f"- {f['kind']} {f['v']}{_s(f)}")
    for f in c["doctrines"]:
        s.add(f"- Doctrine at start: {f['kind']} {f['v']}{_s(f)}", "doctrines")
    if c["events_scheduled"]:
        s.add("- Events the history schedules: " + "; ".join(
            f"{e['v']} in {e.get('days')}{'+' + str(e['random_days']) if e.get('random_days') else ''} days{_s(e)}"
            for e in c["events_scheduled"][:8]))
    s.add("")
    s.add("Scenario variables (numbers; timers first):")
    vs = sorted(c["variables"], key=lambda v: (not scenariomod.TIMER_VAR.search(v["name"]), v["src"]))
    for v in vs:
        s.add(f"- {v['name']} = {v['v']}{_s(v)}", "variables", f"facts grep <variable> / dossier.json country.variables")
    if c["flags"]:
        s.add("- Country flags: " + ", ".join(f["v"] for f in c["flags"][:20]), "flags")
    if c["global_flags"]:
        s.add("- Global flags: " + ", ".join(f["v"] for f in c["global_flags"][:20]), "flags")


def _md_xp(xp, s):
    if not xp:
        return
    st = xp["start"]
    cap = xp["max"].get("army") or {}
    set_ = [f"{b} {_v(f):g} [{f['src']}]" for b, f in st.items() if not f.get("d")]
    unset = [b for b, f in st.items() if f.get("d")]
    s.add("- Experience at start: " + ", ".join(set_ + ([" ".join(unset) + " 0 (no army/air/navy_experience in the "
                                                        "history up to the start; 0 assumed [?])"] if unset else []))
          + (f"; cap {cap.get('v')} each [{cap.get('src')}]" if cap else ""))
    if xp["spirits"]:
        s.add("  - spirits: " + "; ".join(f"{m['idea']} {m['key']} {m['value']:g}" for m in xp["spirits"][:4]))
    for b in ("army", "air", "navy"):
        fs = [f for f in xp["focuses"] if (f.get(b) or 0) > 0]
        ds = [x for x in xp["decisions"] if (x.get(b) or 0) > 0]  # negative values are XP costs
        if fs or ds:
            s.add(f"  - {b} XP rewards: focuses +{sum(f[b] for f in fs):g} over {len(fs)} ("
                  + ", ".join(f"{f['focus']} +{f[b]:g}" for f in fs[:4]) + ")"
                  + (f"; decisions +{sum(x[b] for x in ds):g} over {len(ds)} ("
                     + ", ".join(f"{x['decision']} +{x[b]:g}" for x in ds[:2]) + ")" if ds else "")
                  + " (every branch counted: upper bound)")


def _md_states(d, s):
    rows = d["states"]["rows"]
    s.add(d["states"]["d"] + ".")
    s.add("")
    s.add("| # | id | state | cat/slots | free | infra | civ/mil/dock | port | VP | res | resist/comply | enemy/border steps "
          "| borders | src |")
    s.add("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for r in rows:
        res = ",".join(f"{k[:3]}{v}" for k, v in sorted(r["resources"].items()))
        s.add(f"| {r['rank']} | {r['id']} | {r['name']}{'' if r['core'] else ' (non-core)'} | {r['category']}/{r['slots']} | "
              f"{r['free_slots']} | {r['infrastructure']} | {r['civ']}/{r['mil']}/{r['dock']} | {r['naval_base'] or ''} | "
              f"{r['vp'] or ''} | {res} | {_occ(r.get('occupation'))} | "
              f"{r['enemy_steps'] if r['enemy_steps'] is not None else '-'}/"
              f"{r['border_steps'] if r['border_steps'] is not None else '-'} | {' '.join(r['borders'])} | "
              f"{r['src'].split('/')[-1]} |", "states", f"facts states {d['_meta']['tag']}")


def _occ(o):
    """resist/comply cell: "on 0/20" (resistance on, resistance 0, compliance 20); blank without settings."""
    if not o:
        return ""
    act = o.get("resistance_active")
    on = "on" if act == "yes" else (f"by {act}" if act and act != "no" else "off")
    return f"{on} {o.get('resistance', '-')}/{o.get('compliance', '-')}"


def _md_neighbours(d, s):
    s.add("Land neighbours at start:")
    for r in d["neighbours"]["rows"]:
        s.add(f"- {r['tag']} {r['name']}: {', '.join(r['relation']) or 'no war'}; {r['ruling_party'] or '?'}; "
              f"VP {r['vp_total']}; borders our {r['our_states']} with their {r['border_states']}", "neighbours")
    for e in d["enemies"]:
        vp = e["vp"]
        s.add("")
        s.add(f"### Enemy {e['tag']} {e['name']} ({'at war at start' if e['at_war'] else 'named enemy'})")
        s.add(f"- Capital state {_v(e['capital'])}; stability {_pct(_v(e['stability']))}; war support "
              f"{_pct(_v(e['war_support']))}{_s(e['war_support'])}")
        b = vp["base_limit"] or {}
        s.add(f"- Surrender: {vp['total']} VP in owned states; base limit {b.get('v')} [{b.get('src')}]; "
              + "; ".join(f"limit {t['limit']} -> {t['vp_needed']} VP ({t['with']})" for t in vp["thresholds"]))
        for m in vp["modifiers"]:
            s.add(f"  - {m['idea']} \"{m['name']}\" surrender_limit {m['value']} "
                  f"({'held at start' if m['held_at_start'] else 'possible'}) [{m['src']}]")
        s.add(f"  - method: {vp['d']}")
        s.add("- VP provinces: " + "; ".join(f"{r['province']} {r['name'] or '?'} {r['vp']:g} (st {r['state']})"
                                            for r in vp["provinces"]), "vp", f"facts vp {e['tag']}")
        agg = e["oob"]["aggregate"]
        s.add(f"- Strength at start (fair aggregate): {agg['divisions']} divisions; templates "
              + ", ".join(f"{k} x{v}" for k, v in sorted(agg["per_template"].items(), key=lambda kv: -kv[1]))
              + f"; planes {agg['air_planes']}; ships {agg['ships']}; OOB "
              + ", ".join(f"{f['name']}" for f in agg["files"]))
        tl = e.get("tech_level")
        if tl:
            s.add("- Tech level at start (meta only: classes reached and newest tech year): "
                  + "; ".join(f"{k.replace('_', '-')} " + (
                      f"{v['year']} ({', '.join(_kind(x) for x in v['kinds'][:3])})" if v["reached"] else "none")
                      for k, v in tl["classes"].items()))
        for b in e.get("bop") or []:
            s.add(f"- Balance of power at start: {b['id']} {b['value']} ({b['range']}) [{b['src']}]; "
                  f"`facts bop {e['tag']}`")
        s.add("- Spirits: " + "; ".join(f"{i['id']}" + (f" ({i['modifier'][:80]})" if i.get("modifier") else "")
                                       for i in e["ideas"]), "enemy spirits", f"facts idea <id>")
        if "private" in e["oob"]:
            s.add("- Division positions (private): " + ", ".join(
                f"{x['template']}@{x['location']}" for x in e["oob"]["private"]["divisions"][:40]), "positions")
        for a in e.get("ai_strategy") or []:
            s.add(f"- AI strategy (private) {a['id']}: {'; '.join(a['strategies'][:4])} [{a['src']}]", "ai strategies")


def _md_hooks(d, s):
    tags = [d["_meta"]["tag"]] + list(d["hooks"]["enemies"])
    blocks = [d["hooks"]["own"]] + list(d["hooks"]["enemies"].values())
    seen = set()
    wars = []
    for h in blocks:
        for w in h["wars"]:
            k = (w["attacker"], w["target"])
            if k not in seen:
                seen.add(k)
                wars.append(w)
    s.add("Wars at start: " + ("; ".join(f"{w['attacker']} -> {w['target']} ({w['type']}) [{w['src']}]" for w in wars)
                               or "none"))
    nt = sorted(d["hooks"].get("never_take") or [],
                key=lambda x: (not any(r.startswith("white_peace") for r in x["reasons"]), "name" not in x["reasons"],
                               x["id"]))
    if nt:
        s.add(f"NEVER-TAKE decisions of {d['_meta']['tag']} (end a war, cede states, withdraw; "
              f"`facts decisions {d['_meta']['tag']} --never`): " + "; ".join(
                  f"{x['id']} ({x['reasons'][0]}{', ...' if len(x['reasons']) > 1 else ''}) [{x['src']}]" for x in nt[:6])
              + (f"; ... {len(nt) - 6} more" if len(nt) > 6 else ""))
    # our tag gets 60% of the section, the enemies share the rest
    left = s.budget - s.size - 200
    shares = [0.6] + [0.4 / max(1, len(tags) - 1)] * (len(tags) - 1) if len(tags) > 1 else [1.0]
    for t, h, share in zip(tags, blocks, shares):
        sub = Section(s.num, t, int(left * share))
        _md_hooks_tag(sub, t, h)
        s.lines.extend(["", f"### {t}"] + sub.body())
        s.size += sub.size + 10


def _states_short(ids, n=12):
    ids = list(ids)
    return str(ids[:n])[:-1] + (f", ... {len(ids) - n} more]" if len(ids) > n else "]")


def _md_hooks_tag(s, t, h):
    if h["global_flags"]:
        s.add("- Global flags at start: " + ", ".join(h["global_flags"][:10]))
    for o in h["outcomes"]:
        ctl = o.get("requires_control")
        s.add(f"- Outcome flag {o['flag']} (set in {o['set_in']}): when {' AND '.join(o['conditions']) or 'always'}"
              + (f"; control states {ctl['states']}" if ctl and ctl["states"] else "")
              + (f"; called from {', '.join(o['called_from'][:3])}" if o.get("called_from") else "")
              + f" [{o['src']}]", f"{t} outcomes", f"facts hooks {t}", cap=8)
    for m in h.get("deadlines") or []:
        s.add(f"- Deadline: mission {m['mission']} \"{m['name']}\" ends {m['earliest']}..{m['latest']} "
              f"({m['timeout']} days" + (f" after {' -> '.join(m['via'])}" if m.get("via") else "") + ")"
              + (f" [{m['src']}]" if m.get("src") else ""), f"{t} deadlines", f"facts missions {t}", cap=6)
    for e in h["escalation"]:
        s.add(f"- Threshold {e['threshold']} in {e['on_action']} -> {', '.join(e['effects'])} [{e['src']}]",
              f"{t} thresholds", f"facts hooks {t}", cap=12)
    for m in h["missions"]:
        days = m["mission_days"]
        if m.get("mission_days_value"):
            days = f"{days}={m['mission_days_value']['v']} ({m['mission_days_value']['tag']})"
        s.add(f"- Mission {m['id']} \"{m['name']}\" {days} days"
              + (f"; goal {m['available'][:110]}" if m.get("available") else "")
              + (f"; on timeout {', '.join(m.get('timeout_effect', [])[:4])}" if m.get("timeout_effect") else "")
              + (f"; on success {', '.join(m.get('complete_effect', [])[:4])}" if m.get("complete_effect") else "")
              + f" [{m['src']}]", f"{t} missions", f"facts missions {t}", cap=10)
    goals = sorted(h["goals"], key=lambda g: (not g["id"].startswith(t + "_"), len(g["controls"]["states"]), g["id"]))
    for g in goals:
        ctl = g["controls"]
        s.add(f"- Goal decision {g['id']} \"{g['name']}\": control states {_states_short(ctl['states'])}"
              + (f" provinces {_states_short(ctl['provinces'])}" if ctl["provinces"] else "")
              + f" ({', '.join(ctl['how'])}); then {', '.join(g.get('complete_effect', [])[:5])} [{g['src']}]",
              f"{t} goal decisions", f"facts decisions {t}", cap=6)
    for r in h["on_startup"]:
        s.add(f"- on_startup: {('when ' + r['condition'][:110] + ': ') if r.get('condition') else ''}"
              f"{', '.join(r['effects'][:6])} [{r['src']}]", f"{t} on_startup hooks", f"facts hooks {t}", cap=5)
    for tm in h["timers"]:
        s.add(f"- Timer {tm['kind']} {tm['id']}: {tm['days']} days"
              + (f" +{tm['random_days']}" if tm.get("random_days") else "") + f" [{tm['src']}]",
              f"{t} timers", f"facts hooks {t}", cap=10)
    for r in h["on_actions"]:
        if r.get("thresholds"):
            continue  # already listed
        s.add(f"- {r['on_action']}: {('when ' + r['condition'][:120] + ': ') if r.get('condition') else ''}"
              f"{', '.join((r.get('effects') or [])[:5])} [{r['src']}]", f"{t} on_action hooks", f"facts hooks {t}",
              cap=10)


def _md_focus(d, s):
    f = d["focus"]
    dp = f["days_per_point"] or {}
    s.add(f"Tree `{f['id']}` [{f['src']}], {f['count']} focuses load with these DLCs"
          + (f" ({len(f['hidden'])} hidden by allow_branch)" if f["hidden"] else "")
          + f"; 1 cost = {dp.get('v')} days [{dp.get('src')}]. {f['d']}.")
    rows = {r["id"]: r for r in f["focuses"]}
    counts = {}
    for r in f["focuses"]:
        for b in r.get("branch") or []:
            counts[b] = counts.get(b, 0) + 1
    s.add("Branches (root: focuses below): " + "; ".join(f"{r} ({counts.get(r, 0)})" for r in f["roots"]))
    s.add("")
    s.add("One line per focus, by earliest end: id \"name\" cost/days | needs | excl | available/bypass | reward [src]")
    order = sorted(f["focuses"], key=lambda r: (r["earliest_end_days"] or 0, r["id"]))
    for r in order:
        needs = " & ".join("|".join(g) for g in r["prereq"]) or "-"
        bits = [f"{r['id']} \"{r['name']}\" {r['cost']:g}/{r['days']:g}d", "needs " + needs]
        if r["exclusive"]:
            bits.append("excl " + ",".join(r["exclusive"]))
        if r.get("available"):
            bits.append("avail " + r["available"][:90])
        if r.get("bypass"):
            bits.append("bypass " + r["bypass"][:60])
        if r.get("war_with"):
            bits.append("war " + r["war_with"])
        if r.get("reward"):
            bits.append("-> " + ", ".join(r["reward"][:5]))
        s.add("- " + " | ".join(bits) + f" [{r['src'].split(':')[-1]}]", "focuses",
              f"facts focus-tree {d['_meta']['tag']} --branch <root>")


def _md_research(d, s):
    r = d["research"]
    s.add(f"Slots {_v(r['slots'])}{_s(r['slots'])}; {len(r['start_techs'])} techs at start. {r['multi_parent_note']}.")
    if r["calendar"]:
        s.add(f"Days: {r['calendar'][0]['d']}.")
    for y in r["calendar"]:
        s.add("")
        s.add(f"### {y['year']} ({len(y['techs'])} techs)")
        for t in y["techs"]:
            chain = "now" if t["startable"] else "after " + ",".join(t["chain_all"][:4]) + \
                ("..." if len(t["chain_all"]) > 4 else "")
            alt = f"; any-reading after {','.join(t['chain_any'][:3]) or 'nothing'}" if "chain_any" in t else ""
            early = f" ({t['days_at_start']:g}d if started at {d['_meta']['date']})" \
                if t["days_at_start"] != t["days"] else ""
            s.add(f"- {t['id']} ({t['folder']}) {t['cost']:g} = {t['days']:g}d{early}, {chain}{alt}"
                  + (f"; xor {','.join(t['xor'])}" if t.get("xor") else ""), f"{y['year']} techs",
                  f"facts research-calendar {d['_meta']['tag']} --date {d['_meta']['date']} --years {y['year']}-{y['year']}")
    doc = r["doctrines"]
    s.add("")
    s.add("Doctrines at start: " + ", ".join(f"{x['v']}" for x in doc["current"]) +
          (_s(doc["current"][0]) if doc["current"] else ""))
    s.add("Grand doctrines available: " + "; ".join(f"{g['id']} ({g['folder']}, {g['xp_cost']:g} xp)" for g in doc["grand"]),
          "doctrines")


def _md_politics(d, s):
    p = d["politics"]
    for b in p.get("bop") or []:
        df = b["definition"] or {}
        st = b["start"]
        s.add(f"- Balance of power {b['id']} [{b['src']}]: {df.get('left_side')} (-1) vs {df.get('right_side')} (+1); "
              + (f"ACTIVE at start {st['value']} in {st.get('range')} [{st.get('src')}]" if st.get("active") else
                 "not active at start; gained by " + ", ".join(f"{g['kind']} {g['id']}" for g in b["gained_by"][:3]))
              + "; moved by " + ", ".join(f"{k} {n}" for k, n in b["mover_counts"].items())
              + f"; ranges, modifiers, movers: `facts bop {d['_meta']['tag']}`")
        rng = [r for r in df.get("ranges", []) if r["id"] == st.get("range")]
        if rng and rng[0]["modifier"]:
            s.add("  - start range modifiers: " + ", ".join(f"{k} {v}" for k, v in rng[0]["modifier"].items() if v))
    for cat, row in sorted(p["laws"].items()):
        opts = []
        for o in row["options"]:
            if o["allowed"] is False:
                continue
            mark = "*" if o["id"] == row["current"] else ""
            g = gate_text(o.get("gates") or [], 2)
            opts.append(f"{mark}{o['id']} {o['cost'] if o['cost'] is not None else '?'}PP" + (f" ({g})" if g else ""))
        s.add(f"- {cat}: " + ", ".join(opts), "law categories", f"facts laws {d['_meta']['tag']}")
    s.add("(* = at start; (gates) = numeric and named `available` conditions; full text: `facts laws`, "
          "`facts idea <id>`)")
    s.add("")
    s.add("Advisors (slot cost traits; available):")
    for a in p["advisors"]:
        s.add(f"- {a['idea_token'] or a['id']} \"{a['name']}\" {a['slot']} {a['cost'] if a['cost'] is not None else '?'}PP "
              f"{','.join(a['traits'][:3])}" + (f"; {a['available'][:80]}" if a.get("available") else ""),
              "advisors", f"facts characters {d['_meta']['tag']} --role advisor")
    s.add("")
    s.add("Decisions of the tag (cost, timers; missions are in section 4):")
    for x in p["decisions"]:
        s.add(f"- {x['id']} \"{x['name']}\" cost {x['cost']}" + (f", {x['days_remove']}d" if x.get("days_remove") else "")
              + (", once" if x["fire_only_once"] else "")
              + (f", NEVER-TAKE ({x['never_take'][0]})" if x.get("never_take") else "")
              + (f"; avail {x['available'][:90]}" if x.get("available") else "") + f" [{x['src']}]", "decisions",
              f"facts decisions {d['_meta']['tag']}")


def _md_oob(d, s):
    o = d["oob"]
    s.add("OOB files: " + ", ".join(f"{f['name']} ({f['kind']}) [{f.get('set_by')}]" for f in o["files"]))
    counts = {}
    for dv in o["divisions"]:
        counts[dv["template"]] = counts.get(dv["template"], 0) + 1
    s.add(f"{len(o['divisions'])} divisions. Templates:")
    for t in o["templates"]:
        regs = " ".join(f"{k}x{v}" for k, v in t["regiments"].items())
        s.add(f"- \"{t['name']}\" x{counts.get(t['name'], 0)}: {regs}; support {','.join(t['support']) or '-'}"
              + (f"; cap {t['division_cap']}" if t.get("division_cap") else "") + f" [{t['src']}]", "templates")
    s.add("Divisions per state: " + "; ".join(
        f"{r['state']} {r['name']}: " + ",".join(f"{k} {v}" for k, v in r["templates"].items())
        for r in o["divisions_per_state"]), "states", f"facts oob {d['_meta']['tag']}")
    if o["air"]:
        s.add("Air wings: " + "; ".join(f"{a['state']} {a['type']} {a['amount']}" + (f" ({a['version']})" if a.get("version") else "")
                                        for a in o["air"]), "air")
    if o["fleets"]:
        s.add("Fleets: " + "; ".join(f"{f['name']} base {f['base']}: " + ",".join(f"{k} {v}" for k, v in f["ships"].items())
                                     for f in o["fleets"]), "fleets")
    if o["production"]:
        s.add("Production lines at start (OOB instant_effect): " + "; ".join(
            f"{p['type']}{' ' + p['version'] if p.get('version') else ''} x{p['factories']}" for p in o["production"]))
    s.add("")
    s.add("Commanders (role skill atk/def/plan/log traits; * = recruited at start):")
    for c in sorted(o["commanders"], key=lambda c: (not c["recruited"], c["role"] != "field_marshal", -(c["skill"] or 0))):
        s.add(f"- {'*' if c['recruited'] else ''}{c['id']} {c['role']} {c['skill']} "
              f"{c['attack']}/{c['defense']}/{c['planning']}/{c['logistics']} {','.join(c['traits'][:4])}"
              + (f"; visible {c['visible'][:60]}" if c.get("visible") else ""), "commanders",
              f"facts characters {d['_meta']['tag']} --role general")


def _md_theatres(d, s):
    for t in d["theatres"]:
        s.add(f"### vs {t['enemy']} {t['name']} (strategic regions {t.get('strategic_regions')})")
        if t.get("error"):
            s.add("- " + t["error"])
            continue
        _md_theatre_extra(d, t, s)
        s.add("- Borders (our state | their state | touching provinces, river pairs): " + "; ".join(
            f"{b['ours']} {b['our_name']} | {b['theirs']} {b['their_name']} | {b['pairs']},{b['river_pairs']}"
            for b in t["borders"]), "borders")
        s.add("- Supply hubs: " + "; ".join(f"{h['province']} {h['name'] or ''} (st {h['state']} {h['owner']}, lvl {h['level']})"
                                           for h in t["hubs"]) + (f" [{t['hubs'][0]['src']}]" if t["hubs"] else ""))
        s.add("- Railways: " + "; ".join(f"lvl {r['level']} {r['provinces']} prov {r['ends'][0]}-{r['ends'][1]} states "
                                        f"{'-'.join(map(str, r['states']))} [{r['src']}]" for r in t["railways"]), "railways")
        if t["ports"]:
            s.add("- Ports: " + "; ".join(f"{p['province']} {p['name'] or ''} ({p['owner']}, lvl {p['level']})"
                                         for p in t["ports"]), "ports")
        s.add("- Infrastructure 0: " + ", ".join(f"{x['state']} {x['name']} ({x['owner']})" for x in t["infra0"]), "infra")
        if t.get("routes_note"):
            s.add("- " + t["routes_note"])
        for r in t["routes"]:
            if r.get("error"):
                s.add(f"- Route to {r['to']}: {r['error']}")
                continue
            s.add(f"- Route to {r['to']} {r['name']} ({r['vp']:g} VP): from {r['from']}, {r['steps']} steps, "
                  f"{r['rivers']} rivers {r['crossings']}, cost {r['cost']}; path {'-'.join(map(str, r['path']))}"
                  + (f"; dry: {r['dry']['steps']} steps {r['dry']['rivers']} rivers from {r['dry']['from']}" if r.get("dry") else "")
                  + (f"; {r['from_hub_steps']} steps from our nearest hub" if r.get("from_hub_steps") is not None else ""),
                  "routes", f"facts route <from> <to>")
        if t["routes"]:
            s.add(f"- Route method: {t['routes'][0].get('d', '')}")


def _kind(x):
    return x.replace("_equipment", "").replace("_chassis", "").replace("_", " ")


def _md_theatre_extra(d, t, s):
    tag = d["_meta"]["tag"]
    ov = t.get("overseas") or {}
    if ov.get("overseas"):
        r = ov.get("route")
        line = "- OVERSEAS: no land link from the capital through our, subject and enemy territory"
        if r:
            line += (f"; sea route port {r['from_port']} -> {r['to_port']}: {r['steps']} sea zones"
                     + (" via " + ", ".join(
                         f"{x['rule']} (navy closed when {'/'.join(x['navy_closed_when']) or 'never'}; disabled if "
                         f"{(x.get('is_disabled') or '-')[:110]}; owned by {sorted(set(x['required_provinces'].values()))}) "
                         f"[{x['src']}]" for x in ov.get("rules", [])) if ov.get("rules") else "")
                     + "".join(f"; without {a['avoid']}: {a['steps']} zones"
                               + (f" via {', '.join(a['rules'])}" if a.get("rules") else "")
                               for a in ov.get("without", []) if a.get("steps")))
        elif ov.get("note"):
            line += "; " + ov["note"]
        s.add(line + f"; `facts ports {tag}`")
    elif ov.get("overseas") is False:
        s.add("- Land-linked to the capital (not overseas)")
    c = t.get("port_candidates") or {}
    if c.get("rows"):
        s.add(f"- Port candidates (coast <= {c['steps']} steps from the front, naval base < {c['below_level']}; "
              f"{c['count']} found): " + "; ".join(
                  f"{x['province']}{' ' + x['name'] if x['name'] else ''} st {x['state']} lvl {x['level']} "
                  f"{x['next_level_cost']:g}CP done {x['finish']}" for x in c["rows"][:4])
              + f"; port vs hub vs railway costs and supply flow: `facts ports {tag}`")
    tr = t.get("terrain") or {}
    fm = tr.get("fighting") or {}
    wd = tr.get("width") or {}
    if fm.get("provinces"):
        s.add(f"- Terrain ({tr.get('basis')}, {fm['provinces']} provinces): "
              + ", ".join(f"{k} {round(v * 100)}%" for k, v in fm["shares"].items())
              + "; battle widths 1/2 directions " + ", ".join(
                  f"{k} {'/'.join(str(int(x)) for x in v.values())}" for k, v in (wd.get("battle_widths") or {}).items())
              + (f" [{(wd.get('sources') or {}).get(wd.get('dominant'))}]" if wd.get("dominant") else "")
              + (f"; width target {wd['target']} (dominant {wd['dominant']}), best for the mix "
                 + ", ".join(str(b["width"]) for b in wd["best_for_mix"]) + f" [D facts terrain-width {t['enemy']}]"
                 if wd.get("target") else ""))
    for wr in t.get("weather") or []:
        s.add(f"- Weather region {wr['region']} {wr['name']} ({wr['provinces']} prov): mud {wr['mud']} (heavy "
              f"{wr['heavy_mud']}); ground snow {wr['snow']}; extreme cold {wr['cold']} [{wr['src']}]", "weather",
              "facts weather <state>", cap=3)


# ---------------------------------------------------------------- write

def manifest(d, world=None):
    m = {"generator": GENERATOR, "tag": d["_meta"]["tag"], "date": d["_meta"]["date"], "mode": d["_meta"]["mode"],
         "content_hash": d["_meta"]["content_hash"], "game_version": d["run"]["game_version"],
         "dlcs": d["run"]["dlcs"], "integrated_dlcs": d["run"]["integrated_dlcs"], "mods": d["run"]["mods"],
         "graph": dict(d["run"]["graph"])}
    return m


def write(d, out_dir, cache_dir=None):
    """Write dossier.json, dossier.md, manifest.json (and graph.json.gz copied from mapgraph's cache)."""
    os.makedirs(out_dir, exist_ok=True)
    md = render_md(d)
    with open(os.path.join(out_dir, "dossier.json"), "w", encoding="utf-8") as fh:
        json.dump(d, fh, ensure_ascii=False, indent=1, default=_json_default)
    with open(os.path.join(out_dir, "dossier.md"), "w", encoding="utf-8") as fh:
        fh.write(md)
    m = manifest(d)
    sig = m["graph"].get("signature")
    if sig:
        cache = os.path.join(cache_dir or mapgraph.default_cache_dir(), "mapgraph_%s.json.gz" % sig)
        if os.path.isfile(cache):
            shutil.copyfile(cache, os.path.join(out_dir, "graph.json.gz"))
            m["graph"]["cache"] = cache
            m["graph"]["file"] = "graph.json.gz"
    m["files"] = {"dossier.md": len(md.encode("utf-8")), "dossier.md_tokens_estimate": len(md.encode("utf-8")) // 4}
    with open(os.path.join(out_dir, "manifest.json"), "w", encoding="utf-8") as fh:
        json.dump(m, fh, ensure_ascii=False, indent=1)
    return {"dir": out_dir, "md_bytes": len(md.encode("utf-8")), "manifest": m}


def _json_default(o):
    if isinstance(o, set):
        return sorted(o)
    return str(o)


def section_text(md, which):
    """One section of a dossier.md by number ("4") or name ("hooks")."""
    want = None
    for num, key, title, _ in SECTIONS:
        if str(which) in (str(num), key) or str(which).lower() == title.lower():
            want = num
    if want is None:
        return None
    parts = re.split(r"(?m)^## (?=\d+\. )", md)
    for p in parts:
        if p.startswith(f"{want}. "):
            return "## " + p.rstrip() + "\n"
    return None
