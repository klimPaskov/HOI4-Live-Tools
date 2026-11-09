"""Country-level facts at a start date, with sources: the shared core of the dossier and the facts CLI.

World(game, tag, date, dlcs, mode) evaluates the effective content (install plus mods, through
game.Game and content.Content) for one country at one bookmark:

    country(tag)        history at the start: capital, slots, stability, war support, politics,
                        ideas, OOB names, variables, flags, wars, guarantees, characters, doctrines
    states()            every state at the start: owner, controller, cores, claims, category and
                        static slots, buildings (state and province level), VPs, manpower, resources
    ideas(), laws(tag)  common/ideas and common/idea_tags (slots, costs, modifiers, conditions)
    characters(tag)     common/characters roles (DLC `instance` blocks evaluated)
    oob(tag)            history/units: templates, divisions with location and state, air wings,
                        fleets, production lines from instant_effect
    decisions(tag)      common/decisions filtered by static `allowed`, with timers and the state /
                        province control lists of `available`
    vp(tag)             victory points, total and the surrender threshold with its modifiers

A fact is {"v": value, "src": "common/decisions/ETH.txt:3", "mod": None}; derived numbers carry
"d": "<method>". Enemy facts carry "fair": public | aggregate | private (design 2.3): the FAIR
profile drops private ones. Nothing here is country specific.

Stdlib only, Python 3.9+.
"""
import math
import os
import re
from collections import defaultdict

from . import derive
from .defines import Defines
from .script import (Block, Ctx, _sub, bare, date_tuple, get, get1, history_walk, line_of, parse_file, render,
                     run_effects, trigger)

FAIR_LEVELS = ("public", "aggregate", "private")


# ---------------------------------------------------------------- 365-day calendar (derive.py)

def day_number(date):
    """Days since 1.1.1 of a date in the game's 365-day calendar (derive.day)."""
    n = derive.day(date)
    if n is None:
        raise ValueError(f"not a date: {date!r}")
    return n


def from_day(n):
    y, m, d = derive.date_of(n)
    return f"{y}.{m}.{d}"


def add_days(date, days):
    return from_day(day_number(date) + int(math.ceil(days)))


class Loc:
    """English localisation through derive.loc (one table per Game), cleaned of colour codes and icons."""

    FORMAT = re.compile(r"§[A-Za-z0-9!]|£[A-Za-z0-9_|]+£?|\$[A-Za-z0-9_|.]+\$")

    def __init__(self, game):
        self.game = game

    def get(self, key, default=None):
        if not key:
            return default
        v = derive.loc(self.game, key)
        if v is None:
            return default
        v = self.FORMAT.sub("", v.replace("\\n", " ").replace('\\"', '"'))
        return re.sub(r"\s+", " ", v).strip() or default

    def name(self, key):
        return self.get(key, key)


def norm_date(date):
    t = date_tuple(date)
    return f"{t[0]}.{t[1]}.{t[2]}" if t else date


def num(v, default=None):
    try:
        f = float(v)
        return int(f) if f.is_integer() and re.match(r"^-?\d+$", str(v).strip()) else f
    except (TypeError, ValueError):
        return default


# ---------------------------------------------------------------- the world at a start date

class World:
    def __init__(self, game, tag, date="1936.1.1", dlcs=(), mode="fair", language="english"):
        self.game = game
        self.content = game.content
        self.tag = tag
        self.date = norm_date(date or "1936.1.1")
        names = list(dict.fromkeys(list(dlcs) + list(game.integrated_dlcs())))
        self.dlcs = names
        self.active_dlcs = [d for d in dlcs]
        self.mode = mode
        self.ctx = Ctx(tag, names, self.date)
        self.loc = Loc(game)
        self.defines = Defines(self.content)
        self._cache = {}
        self._rel = None

    # ------------------------------------------------------------ sources
    def cached(self, name, fn):
        if name not in self._cache:
            self._cache[name] = fn()
        return self._cache[name]

    def ctx_for(self, tag):
        return Ctx(tag, self.dlcs, self.date)

    def rel(self, path):
        if self._rel is None:
            self._rel = {p: r for r, p, _ in self.content.merged().values()}
        return self._rel.get(path) or os.path.basename(path or "")

    def src(self, path, line=None):
        if not path:
            return None
        r = self.rel(path)
        return f"{r}:{line}" if line else r

    def fact(self, v, path=None, line=None, d=None, fair=None):
        out = {"v": v, "src": self.src(path, line), "mod": self.content.source(path) if path else None}
        if d:
            out["d"] = d
        if fair:
            out["fair"] = fair
        return out

    def define(self, key, default=None):
        """(value, fact) of a define (fact None when missing)."""
        full, row = self.defines.row(key)
        if row is None:
            return default, None
        return row["v"], dict(self.fact(row["v"], row["file"], row["line"]), key=full)

    def name(self, key):
        return self.loc.name(key)

    def state_name(self, sid):
        return self.loc.get(f"STATE_{sid}") or f"state {sid}"

    def province_name(self, pid):
        return self.loc.get(f"VICTORY_POINTS_{pid}") or None

    def country_name(self, tag):
        return self.loc.get(tag) or self.loc.get(f"{tag}_DEF") or self.country(tag)["name"]

    def visible(self, fair):
        """True when a fact of this FAIR level is shown in the current mode."""
        return self.mode != "fair" or fair != "private"

    def text(self, items, limit=300, tag=None):
        """One-line trigger text with the static parts (has_dlc, tag, start date) already decided."""
        if not isinstance(items, list):
            return items
        return render(reduce_trigger(items, self.ctx_for(tag or self.tag)), limit)

    def variable(self, name, tags=None):
        """The start value fact of a scenario variable set in a country history ("ETH.x" or "x")."""
        short = name.split(".")[-1]
        scope = name.split(".")[0] if "." in name else None
        order = [scope] if scope and re.match(r"^[A-Z][A-Z0-9]{2}$", scope) else []
        order += list(tags or []) + [self.tag] + self.at_war_with(self.tag)
        for t in dict.fromkeys(order):
            for v in self.country(t)["variables"]:
                if v["name"].split(".")[-1] == short:
                    return dict(v, tag=t)
        return None

    # ------------------------------------------------------------ country history
    def country(self, tag=None):
        tag = tag or self.tag
        return self.cached("country:" + tag, lambda: self._country(tag))

    def _country(self, tag):
        rel = self.game.history_file(tag)
        out = {"tag": tag, "name": self.loc.get(tag) or self.loc.get(f"{tag}_DEF") or tag,
               "history_file": rel, "capital": None,
               "research_slots": None, "stability": None, "war_support": None, "convoys": None,
               "ideas": [], "removed_ideas": [], "oobs": [], "politics": {}, "popularities": {},
               "variables": [], "flags": [], "global_flags": [], "wars": [], "guarantees": [],
               "faction": [], "access": [], "subjects": [], "characters": [], "doctrines": [],
               "techs": [], "events": [], "completed_focuses": [], "dynamic_modifiers": [],
               "political_power": None, "command_power": None}
        path = self.game.file(rel) if rel else None
        if not path:
            return out
        F = lambda v, line, **kw: self.fact(v, path, line, **kw)  # noqa: E731
        ideas = {}

        def visit(k, v, line):
            kl = (k or "").lower()
            if kl == "capital" and isinstance(v, str):
                out["capital"] = F(num(v), line)
            elif kl == "set_research_slots":
                out["research_slots"] = F(num(v), line)
            elif kl == "set_stability":
                out["stability"] = F(num(v), line)
            elif kl == "set_war_support":
                out["war_support"] = F(num(v), line)
            elif kl == "set_convoys":
                out["convoys"] = F(num(v), line)
            elif kl == "add_political_power":
                out["political_power"] = F(num(v), line)
            elif kl == "add_command_power":
                out["command_power"] = F(num(v), line)
            elif kl == "add_ideas":
                for idea in (bare(v) if isinstance(v, list) else [v]):
                    ideas[idea] = F(idea, line)
            elif kl == "remove_ideas":
                for idea in (bare(v) if isinstance(v, list) else [v]):
                    ideas.pop(idea, None)
                    out["removed_ideas"].append(F(idea, line))
            elif kl in ("set_oob", "set_naval_oob", "set_air_oob", "load_oob") and isinstance(v, str):
                out["oobs"].append(dict(F(v, line), kind=kl))
            elif kl == "set_politics" and isinstance(v, list):
                for a, b in v:
                    if a and isinstance(b, str):
                        out["politics"][a] = F(b, line)
            elif kl == "set_popularities" and isinstance(v, list):
                for a, b in v:
                    if a:
                        out["popularities"][a] = F(num(b), line)
            elif kl == "set_variable" and isinstance(v, list):
                var, val = None, None
                if get1(v, "var") is not None:
                    var, val = get1(v, "var"), get1(v, "value")
                else:
                    for a, b in v:
                        if a and isinstance(b, str):
                            var, val = a, b
                            break
                if var is not None and num(val) is not None:
                    out["variables"].append(dict(F(num(val), line), name=var))
            elif kl in ("set_country_flag", "set_global_flag"):
                flag = get1(v, "flag") if isinstance(v, list) else v
                if flag:
                    out["flags" if kl == "set_country_flag" else "global_flags"].append(F(flag, line))
            elif kl == "declare_war_on" and isinstance(v, list):
                out["wars"].append(dict(F(get1(v, "target"), line), type=get1(v, "type"), attacker=tag))
            elif kl == "give_guarantee":
                out["guarantees"].append(F(v, line))
            elif kl in ("create_faction", "add_to_faction"):
                out["faction"].append(dict(F(v, line), kind=kl))
            elif kl == "give_military_access":
                out["access"].append(F(v, line))
            elif kl in ("set_autonomy", "puppet", "release_puppet"):
                target = get1(v, "target") if isinstance(v, list) else v
                out["subjects"].append(dict(F(target, line), kind=kl,
                                            level=get1(v, "autonomy_state") if isinstance(v, list) else None))
            elif kl == "recruit_character" and isinstance(v, str):
                out["characters"].append(F(v, line))
            elif kl in ("set_grand_doctrine", "add_doctrine", "set_sub_doctrine", "add_subdoctrine"):
                out["doctrines"].append(dict(F(v if isinstance(v, str) else render(v, 120), line), kind=kl))
            elif kl == "set_technology" and isinstance(v, list):
                out["techs"].extend(a for a, b in v if a and b == "1")
            elif kl in ("country_event", "news_event"):
                eid = get1(v, "id") if isinstance(v, list) else v
                days = num(get1(v, "days")) if isinstance(v, list) else None
                out["events"].append(dict(F(eid, line), days=days,
                                          random_days=num(get1(v, "random_days")) if isinstance(v, list) else None))
            elif kl in ("complete_national_focus",):
                out["completed_focuses"].append(F(v, line))
            elif kl == "add_dynamic_modifier" and isinstance(v, list):
                out["dynamic_modifiers"].append(F(get1(v, "modifier"), line))

        history_walk(parse_file(path), self.ctx_for(tag), self.date, visit, with_line=True)
        out["ideas"] = list(ideas.values())
        party = out["politics"].get("ruling_party")
        if out["name"] == tag and party:
            out["name"] = self.loc.get(f"{tag}_{party['v']}") or self.loc.get(f"{tag}_{party['v']}_DEF") or tag
        out["techs"] = sorted(set(out["techs"]))
        return out

    def ruling_party(self, tag=None):
        p = self.country(tag)["politics"].get("ruling_party")
        return p["v"] if p else None

    # ------------------------------------------------------------ wars at start
    def wars(self):
        """Every declare_war_on in the country histories at the start date: [{attacker, target, type, src}]."""

        def load():
            out = []
            for p in self.game.files("history/countries"):
                try:
                    with open(p, encoding="utf-8-sig", errors="replace") as fh:
                        if "declare_war_on" not in fh.read():
                            continue
                except OSError:
                    continue
                m = re.match(r"^([A-Z][A-Z0-9]{2})", os.path.basename(p))
                if not m:
                    continue
                for w in self.country(m.group(1))["wars"]:
                    out.append({"attacker": m.group(1), "target": w["v"], "type": w.get("type"),
                                "src": w["src"], "mod": w["mod"]})
            return out

        return self.cached("wars", load)

    def at_war_with(self, tag=None):
        tag = tag or self.tag
        enemies = set()
        for w in self.wars():
            if w["attacker"] == tag:
                enemies.add(w["target"])
            elif w["target"] == tag:
                enemies.add(w["attacker"])
        return sorted(e for e in enemies if e)

    # ------------------------------------------------------------ states
    def state_categories(self):
        def load():
            out = {}
            for p in self.game.files("common/state_category"):
                items = parse_file(p)
                for i, (k, v) in enumerate(items):
                    if k != "state_categories" or not isinstance(v, list):
                        continue
                    for j, (name, body) in enumerate(v):
                        if name and isinstance(body, list):
                            out[name] = {"slots": num(get1(body, "local_building_slots"), 0),
                                         "src": self.src(p, line_of(v, j))}
            return out

        return self.cached("statecats", load)

    def building_types(self):
        """{building: {"cost", "shares_slots", "state_max", "province_max", "src"}} from common/buildings."""

        def load():
            out = {}
            for p in self.game.files("common/buildings"):
                items = parse_file(p)
                for k, v in items:
                    if k != "buildings" or not isinstance(v, list):
                        continue
                    for j, (name, body) in enumerate(v):
                        if not name or not isinstance(body, list):
                            continue
                        cap = get1(body, "level_cap", [])
                        cap = cap if isinstance(cap, list) else []
                        out[name] = {"cost": num(get1(body, "base_cost")),
                                     "shares_slots": get1(cap, "shares_slots") == "yes",
                                     "state_max": num(get1(cap, "state_max")),
                                     "province_max": num(get1(cap, "province_max")),
                                     "infra_effect": get1(body, "infrastructure_construction_effect") == "yes",
                                     "src": self.src(p, line_of(v, j))}
            return out

        return self.cached("buildingtypes", load)

    def states(self):
        """{state id: row} at the start date (see the module doc)."""
        return self.cached("states", self._states)

    def _states(self):
        cats = self.state_categories()
        btypes = self.building_types()
        out = {}
        for sid, (path, st) in self.game._state_blocks().items():
            cat = get1(st, "state_category", "")
            res = get1(st, "resources", [])
            row = {"id": sid, "name": self.state_name(sid), "file": self.src(path, getattr(st, "line", None)),
                   "mod": self.content.source(path),
                   "category": cat, "slots": cats.get(cat, {}).get("slots"),
                   "manpower": num(get1(st, "manpower"), 0),
                   "resources": {a: num(b, 0) for a, b in (res if isinstance(res, list) else []) if a},
                   "provinces": [int(x) for x in bare(get1(st, "provinces", [])) if x.isdigit()],
                   "impassable": get1(st, "impassable") == "yes",
                   "owner": None, "controller": None, "cores": [], "claims": [],
                   "buildings": {}, "province_buildings": {}, "vps": {}, "vp_src": {}}
            hist = get1(st, "history", [])
            if isinstance(hist, list):

                def visit(k, v, line, row=row, path=path):
                    if k in ("owner", "transfer_state_to") and isinstance(v, str):
                        row["owner"] = row["controller"] = v
                    elif k == "controller" and isinstance(v, str):
                        row["controller"] = v
                    elif k == "add_core_of" and isinstance(v, str):
                        if v not in row["cores"]:
                            row["cores"].append(v)
                    elif k == "remove_core_of" and isinstance(v, str):
                        row["cores"] = [c for c in row["cores"] if c != v]
                    elif k == "add_claim_by" and isinstance(v, str):
                        row["claims"].append(v)
                    elif k == "victory_points" and isinstance(v, list):
                        vals = bare(v)
                        for i in range(0, len(vals) - 1, 2):
                            if vals[i].isdigit() and num(vals[i + 1]) is not None:
                                row["vps"][int(vals[i])] = num(vals[i + 1])
                                row["vp_src"][int(vals[i])] = self.src(path, line)
                    elif k == "buildings" and isinstance(v, list):
                        for bk, bv in v:
                            if bk and bk.isdigit() and isinstance(bv, list):
                                pb = row["province_buildings"].setdefault(int(bk), {})
                                for a, b in bv:
                                    if a and num(b) is not None:
                                        pb[a] = num(b)
                            elif bk and num(bv) is not None:
                                row["buildings"][bk] = num(bv)

                history_walk(hist, self.ctx, self.date, visit, with_line=True)
            used = sum(n for b, n in row["buildings"].items() if btypes.get(b, {}).get("shares_slots"))
            row["used_slots"] = used
            row["free_slots"] = (row["slots"] - used) if row["slots"] is not None else None
            row["infrastructure"] = row["buildings"].get("infrastructure", 0)
            out[sid] = row
        return out

    def owned_states(self, tag=None):
        tag = tag or self.tag
        return sorted(sid for sid, r in self.states().items() if r["owner"] == tag)

    def province_state(self):
        return self.game.province_state()

    # ------------------------------------------------------------ map
    def graph(self):
        return self.game.map_graph()

    def owner_of_province(self, pid):
        sid = self.province_state().get(pid)
        st = self.states().get(sid) if sid else None
        return st["owner"] if st else None

    def state_borders(self):
        """{state: {neighbour state: [(own province, other province)]}} over passable land edges."""

        def load():
            g = self.graph()
            if g is None:
                return {}
            ps = self.province_state()
            out = defaultdict(lambda: defaultdict(list))
            for a, row in g["adj"].items():
                if g["provinces"].get(a, {}).get("type") != "land":
                    continue
                for b, e in row.items():
                    if e.get("impassable") or g["provinces"].get(b, {}).get("type") != "land":
                        continue
                    sa, sb = ps.get(a), ps.get(b)
                    if sa and sb and sa != sb:
                        out[sa][sb].append((a, b))
            return {k: dict(v) for k, v in out.items()}

        return self.cached("stateborders", load)

    def neighbours(self, tag=None):
        """{neighbour tag: {"states": [their states on the border], "ours": [our states], "pairs": n}}."""
        tag = tag or self.tag
        states = self.states()
        out = {}
        for sid in self.owned_states(tag):
            for nb, pairs in self.state_borders().get(sid, {}).items():
                owner = states.get(nb, {}).get("owner")
                if not owner or owner == tag:
                    continue
                row = out.setdefault(owner, {"states": set(), "ours": set(), "pairs": 0})
                row["states"].add(nb)
                row["ours"].add(sid)
                row["pairs"] += len(pairs)
        return {t: {"states": sorted(r["states"]), "ours": sorted(r["ours"]), "pairs": r["pairs"]}
                for t, r in sorted(out.items())}

    def province_steps(self, sources):
        """{province: land steps to the nearest source province} (derive.distances: BFS over passable
        land borders and straits)."""
        return derive.distances(self.game, list(sources))

    def provinces_of(self, tags):
        tags = set(tags)
        out = []
        for sid, r in self.states().items():
            if r["owner"] in tags:
                out.extend(r["provinces"])
        return out

    # ------------------------------------------------------------ ideas and laws
    def ideas(self):
        """{idea: {"category", "law", "cost", "removal_cost", "modifier", "allowed", "available",
        "block", "path", "line"}} from common/ideas."""

        def load():
            out = {}
            for p in self.game.files("common/ideas", recursive=True):
                for k, v in parse_file(p):
                    if k != "ideas" or not isinstance(v, list):
                        continue
                    for cat, body in v:
                        if not cat or not isinstance(body, list):
                            continue
                        law = get1(body, "law") == "yes"
                        for j, (name, idea) in enumerate(body):
                            if not name or not isinstance(idea, list):
                                continue
                            mod = get1(idea, "modifier")
                            out[name] = {"id": name, "category": cat, "law": law,
                                         "cost": num(get1(idea, "cost")),
                                         "removal_cost": num(get1(idea, "removal_cost")),
                                         "modifier": mod if isinstance(mod, list) else [],
                                         "allowed": get1(idea, "allowed"), "available": get1(idea, "available"),
                                         "default": get1(idea, "default") == "yes",
                                         "block": idea, "path": p, "line": line_of(body, j)}
            return out

        return self.cached("ideas", load)

    def idea_slots(self):
        """{slot or category: {"group", "cost", "removal_cost"}} from common/idea_tags."""

        def load():
            out = {}
            for p in self.game.files("common/idea_tags"):
                for k, v in parse_file(p):
                    if k != "idea_categories" or not isinstance(v, list):
                        continue
                    for group, body in v:
                        if not group or not isinstance(body, list):
                            continue
                        row = {"group": group, "cost": num(get1(body, "cost")),
                               "removal_cost": num(get1(body, "removal_cost"))}
                        out.setdefault(group, row)
                        for kind in ("slot", "character_slot"):
                            for s in get(body, kind):
                                if isinstance(s, str):
                                    out.setdefault(s, dict(row, character=kind == "character_slot"))
            return out

        return self.cached("ideaslots", load)

    def idea_cost(self, idea):
        i = self.ideas().get(idea)
        if not i:
            return None
        if i["cost"] is not None:
            return i["cost"]
        return self.idea_slots().get(i["category"], {}).get("cost")

    def modifier_value(self, idea, key):
        i = self.ideas().get(idea)
        if not i:
            return None
        for a, b in i["modifier"]:
            if a == key:
                return num(b)
        return None

    def idea_row(self, idea, tag=None):
        i = self.ideas().get(idea)
        if not i:
            return None
        ctx = self.ctx_for(tag or self.tag)
        allowed = trigger(i["allowed"], ctx) if isinstance(i["allowed"], list) else True
        return {"id": idea, "name": self.name(idea), "category": i["category"], "law": i["law"],
                "cost": self.idea_cost(idea), "removal_cost": i["removal_cost"],
                "allowed": allowed,
                "available": self.text(i["available"], 240, tag) if isinstance(i["available"], list) else None,
                "modifier": render(i["modifier"], 300) if i["modifier"] else None,
                "src": self.src(i["path"], i["line"]), "mod": self.content.source(i["path"])}

    def laws(self, tag=None):
        """{category: {"current", "options": [idea rows]}} for the law categories."""
        tag = tag or self.tag
        current = {f["v"] for f in self.country(tag)["ideas"]}
        out = {}
        for iid, i in self.ideas().items():
            if not i["law"]:
                continue
            row = self.idea_row(iid, tag)
            if row["allowed"] is False:
                continue
            cat = out.setdefault(i["category"], {"current": None, "options": []})
            cat["options"].append(row)
            if iid in current:
                cat["current"] = iid
        return out

    # ------------------------------------------------------------ characters
    def characters(self, tag=None, role=None):
        """Characters of the tag's character files plus those it recruits at the start, with their
        roles (DLC `instance`/`allowed` blocks evaluated): [{id, name, roles: [...], recruited, src}]."""
        tag = tag or self.tag
        recruited = {f["v"] for f in self.country(tag)["characters"]}
        ctx = self.ctx_for(tag)
        roles_wanted = {"general": ("corps_commander", "field_marshal"), "marshal": ("field_marshal",),
                        "admiral": ("navy_leader",), "advisor": ("advisor",), "leader": ("country_leader",)}
        want = roles_wanted.get(role, (role,) if role else None)
        out = []
        for p in self.game.files("common/characters"):
            base = os.path.basename(p)
            file_tag = base[:3] if re.match(r"^[A-Z][A-Z0-9]{2}", base) else None
            for k, v in parse_file(p):
                if k != "characters" or not isinstance(v, list):
                    continue
                for j, (cid, body) in enumerate(v):
                    if not cid or not isinstance(body, list):
                        continue
                    if cid not in recruited and file_tag != tag and not cid.startswith(tag + "_"):
                        continue
                    blocks = [body]
                    for inst in get(body, "instance"):
                        if isinstance(inst, list) and trigger(get1(inst, "allowed", []), ctx) is not False:
                            blocks.append(inst)
                    roles = []
                    for b in blocks:
                        for rk, rv in b:
                            if rk not in ("country_leader", "corps_commander", "field_marshal", "navy_leader",
                                          "advisor", "operative") or not isinstance(rv, list):
                                continue
                            if trigger(get1(rv, "allowed", []), ctx) is False:
                                continue
                            r = {"role": rk}
                            traits = get1(rv, "traits")
                            if isinstance(traits, list):
                                r["traits"] = bare(traits)
                            for sk in ("skill", "attack_skill", "defense_skill", "planning_skill",
                                       "logistics_skill", "maneuvering_skill", "coordination_skill"):
                                if get1(rv, sk) is not None:
                                    r[sk.replace("_skill", "")] = num(get1(rv, sk))
                            if rk == "advisor":
                                slot = get1(rv, "slot")
                                cost = num(get1(rv, "cost"))
                                r.update({"slot": slot, "idea_token": get1(rv, "idea_token"),
                                          "cost": cost if cost is not None else
                                          self.idea_slots().get(slot, {}).get("cost")})
                                av = get1(rv, "available")
                                if isinstance(av, list):
                                    r["available"] = self.text(av, 160, tag)
                            if rk == "country_leader":
                                r["ideology"] = get1(rv, "ideology")
                            vis = get1(rv, "visible")
                            if isinstance(vis, list):
                                r["visible"] = render(vis, 120)
                            roles.append(r)
                    if want and not any(r["role"] in want for r in roles):
                        continue
                    out.append({"id": cid, "name": self.name(cid), "recruited": cid in recruited,
                                "roles": roles, "src": self.src(p, line_of(v, j)), "mod": self.content.source(p)})
        return out

    # ------------------------------------------------------------ orders of battle
    def oob(self, tag=None):
        """Templates, divisions (with state), air wings, fleets and production lines of the OOBs the
        tag's history loads at the start."""
        tag = tag or self.tag
        return self.cached("oob:" + tag, lambda: self._oob(tag))

    def _oob(self, tag):
        ctx = self.ctx_for(tag)
        ps = self.province_state()
        out = {"files": [], "templates": {}, "divisions": [], "air": [], "fleets": [], "production": []}
        for o in self.country(tag)["oobs"]:
            path = self.game.file(f"history/units/{o['v']}.txt")
            if not path:
                out["files"].append({"name": o["v"], "kind": o["kind"], "src": None, "missing": True})
                continue
            out["files"].append({"name": o["v"], "kind": o["kind"], "src": self.src(path), "set_by": o["src"],
                                 "mod": self.content.source(path)})

            def visit(k, v, line, path=path):
                if not isinstance(v, list):
                    return
                if k == "division_template":
                    name = get1(v, "name")
                    regs = defaultdict(int)
                    sup = []
                    for rk, rv in v:
                        if rk == "regiments" and isinstance(rv, list):
                            for a, _ in rv:
                                if a:
                                    regs[a] += 1
                        elif rk == "support" and isinstance(rv, list):
                            sup.extend(a for a, _ in rv if a)
                    out["templates"][name] = {"name": name, "regiments": dict(regs), "support": sup,
                                              "division_cap": num(get1(v, "division_cap")),
                                              "src": self.src(path, line)}
                elif k == "division":
                    loc = num(get1(v, "location"))
                    out["divisions"].append({"template": get1(v, "division_template"), "location": loc,
                                             "state": ps.get(loc), "src": self.src(path, line)})
                elif k == "units":
                    run_effects(v, ctx, visit, with_line=True)
                elif k == "air_wings":
                    for sk, sv in v:
                        if not (sk and sk.isdigit() and isinstance(sv, list)):
                            continue
                        for ek, ev in sv:
                            if isinstance(ev, list) and get1(ev, "amount") is not None:
                                out["air"].append({"state": int(sk), "type": ek, "amount": num(get1(ev, "amount")),
                                                   "version": get1(ev, "version_name"), "src": self.src(path, line)})
                elif k == "navy" or k == "fleet":
                    self._fleet(v, out, path, line)
                elif k == "instant_effect":
                    def prod(a, b, ln):
                        if a == "add_equipment_production" and isinstance(b, list):
                            eq = get1(b, "equipment", [])
                            out["production"].append({"type": get1(eq, "type") if isinstance(eq, list) else None,
                                                      "version": get1(eq, "version_name") if isinstance(eq, list) else None,
                                                      "factories": num(get1(b, "requested_factories")),
                                                      "src": self.src(path, ln)})
                    run_effects(v, ctx, prod, with_line=True)

            run_effects(parse_file(path), ctx, visit, with_line=True)
        return out

    def _fleet(self, v, out, path, line):
        if get1(v, "fleet") is not None or get1(v, "task_force") is not None or get1(v, "ship") is not None:
            pass
        ships = defaultdict(int)

        def count(block):
            for a, b in block:
                if a == "ship" and isinstance(b, list):
                    eq = get1(b, "equipment", [])
                    kind = eq[0][0] if isinstance(eq, list) and eq else get1(b, "definition")
                    ships[kind or "ship"] += 1
                elif isinstance(b, list):
                    count(b)

        count(v)
        out["fleets"].append({"name": get1(v, "name"), "base": num(get1(v, "naval_base")),
                              "ships": dict(ships), "src": self.src(path, line)})

    # ------------------------------------------------------------ decisions and missions
    def decision_index(self):
        """[{id, category, block, path, line, cat_allowed}] of every decision."""

        def load():
            out = []
            for p in self.game.files("common/decisions"):
                for i, (cat, body) in enumerate(parse_file(p)):
                    if not cat or not isinstance(body, list) or cat.startswith("@"):
                        continue
                    for j, (did, d) in enumerate(body):
                        if not did or not isinstance(d, list):
                            continue
                        if did in ("allowed", "visible", "available", "icon", "priority", "picture",
                                   "on_map_area", "scripted_gui", "visibility_type", "target_root_trigger"):
                            continue
                        out.append({"id": did, "category": cat, "block": d, "path": p, "line": line_of(body, j)})
            return out

        return self.cached("decisions", load)

    def decision_categories(self):
        def load():
            out = {}
            for p in self.game.files("common/decisions/categories"):
                for i, (cat, body) in enumerate(parse_file(p)):
                    if cat and isinstance(body, list):
                        out[cat] = {"allowed": get1(body, "allowed"), "visible": get1(body, "visible"),
                                    "src": self.src(p, line_of(parse_file(p), i))}
            return out

        return self.cached("decisioncats", load)

    def decisions(self, tag=None, missions=None):
        """Decisions the tag may take: static `allowed` (decision and category) not false. Each row has
        cost, timers, conditions (one-line text) and the control lists of `available`.
        missions=True keeps only missions (days_mission_timeout), False drops them."""
        tag = tag or self.tag
        ctx = self.ctx_for(tag)
        cats = self.decision_categories()
        out = []
        for d in self.decision_index():
            b = d["block"]
            cat = cats.get(d["category"], {})
            if isinstance(cat.get("allowed"), list) and trigger(cat["allowed"], ctx) is False:
                continue
            allowed = get1(b, "allowed")
            if isinstance(allowed, list):
                if trigger(allowed, ctx) is False:
                    continue
            elif not _names_tag(b, tag) and not _names_tag(cat.get("visible") or [], tag) and \
                    not _names_tag(cat.get("allowed") or [], tag):
                continue  # no allowed block and nothing names the tag: a generic decision, left out
            raw = get1(b, "days_mission_timeout")
            timeout = num(raw) if num(raw) is not None else raw  # a number or a variable name
            if missions is True and timeout is None:
                continue
            if missions is False and timeout is not None:
                continue
            row = self.decision_row(d, timeout)
            row["specific"] = _names_tag(allowed if isinstance(allowed, list) else [], tag) or \
                _names_tag(cat.get("allowed") or [], tag) or _names_tag(cat.get("visible") or [], tag)
            out.append(row)
        return out

    def decision_row(self, d, timeout=None):
        b = d["block"]
        row = {"id": d["id"], "name": self.name(d["id"]), "category": d["category"],
               "cost": num(get1(b, "cost")) if get1(b, "cost") is not None else get1(b, "cost"),
               "days_remove": num(get1(b, "days_remove")), "days_re_enable": num(get1(b, "days_re_enable")),
               "mission_days": timeout if timeout is not None else num(get1(b, "days_mission_timeout")),
               "fire_only_once": get1(b, "fire_only_once") == "yes",
               "src": self.src(d["path"], d["line"]), "mod": self.content.source(d["path"])}
        if row["mission_days"] is None and get1(b, "days_mission_timeout") is not None:
            row["mission_days"] = get1(b, "days_mission_timeout")  # a variable name
        if isinstance(row["mission_days"], str):
            var = self.variable(row["mission_days"])
            if var:
                row["mission_days_value"] = var
        for key in ("available", "visible", "activation", "cancel_trigger"):
            v = get1(b, key)
            if isinstance(v, list) and v:
                row[key] = self.text(v, 300)
        for key in ("complete_effect", "remove_effect", "timeout_effect", "cancel_effect"):
            v = get1(b, key)
            if isinstance(v, list) and v:
                row[key] = effect_summary(v)
        av = get1(b, "available")
        if isinstance(av, list):
            ctl = control_lists(av)
            if ctl:
                row["controls"] = ctl
        targets = get1(b, "target_trigger") or get1(b, "targets")
        if targets is not None:
            row["targets"] = render(targets, 160) if isinstance(targets, list) else targets
        return row

    def find_decision(self, did):
        for d in self.decision_index():
            if d["id"] == did:
                return d
        return None

    # ------------------------------------------------------------ victory points and surrender
    def vp(self, tag=None):
        """VP list, total, capital, and the surrender threshold: NCountry.BASE_SURRENDER_LIMIT plus
        the surrender_limit modifiers of the tag's ideas (applied when held at the start, possible
        otherwise), needed VP = ceil(limit x total)."""
        tag = tag or self.tag
        states = self.states()
        rows = []
        for sid in self.owned_states(tag):
            st = states[sid]
            for pid, val in sorted(st["vps"].items(), key=lambda kv: -kv[1]):
                rows.append({"province": pid, "name": self.province_name(pid), "state": sid,
                             "state_name": st["name"], "vp": val, "src": st["vp_src"].get(pid)})
        total = sum(r["vp"] for r in rows)
        base, base_fact = self.define("BASE_SURRENDER_LIMIT", 0.8)
        mn, mn_fact = self.define("MIN_SURRENDER_LIMIT", 0.2)
        held = {f["v"] for f in self.country(tag)["ideas"]}
        mods = []
        for iid in self._tag_ideas(tag) | held:
            val = self.modifier_value(iid, "surrender_limit")
            if val is None:
                continue
            i = self.ideas()[iid]
            mods.append({"idea": iid, "name": self.name(iid), "value": val, "held_at_start": iid in held,
                         "src": self.src(i["path"], i["line"]), "mod": self.content.source(i["path"])})
        mods.sort(key=lambda m: (not m["held_at_start"], m["idea"]))
        # the start threshold is derive.surrender's (shared with the lint); the possible modifiers and
        # the sources are added here
        sur = derive.surrender(self.game, tag, self.ctx_for(tag), self.date)
        start_limit = sur["limit"]

        def need(limit):
            return int(math.ceil(round(limit * total, 6)))

        thresholds = [{"limit": start_limit, "vp_needed": sur["vp_needed"], "with": "start ideas",
                       "d": "derive.surrender"}]
        for m in mods:
            if not m["held_at_start"]:
                lim = max(mn, start_limit + m["value"])
                thresholds.append({"limit": round(lim, 4), "vp_needed": need(lim), "with": m["idea"]})
        cap = self.country(tag)["capital"]
        return {"tag": tag, "provinces": rows, "total": total, "capital": cap["v"] if cap else None,
                "base_limit": base_fact, "min_limit": mn_fact, "modifiers": mods, "thresholds": thresholds,
                "d": "vp_needed = ceil(limit x total VP of owned states at start); limit = BASE_SURRENDER_LIMIT "
                     "+ surrender_limit of held ideas, floored at MIN_SURRENDER_LIMIT",
                "fair": "public"}

    def _tag_ideas(self, tag):
        """Ideas that belong to a tag: key prefix TAG_, or defined in a file named after the country."""
        name = self.country_name(tag).lower()
        out = set()
        for iid, i in self.ideas().items():
            base = os.path.basename(i["path"]).lower()
            if iid.startswith(tag + "_") or (name and base.startswith(name.split()[0][:5]) and len(name) > 3):
                out.add(iid)
        return out


# ---------------------------------------------------------------- script summaries

def _extend(out, block):
    for i, item in enumerate(block):
        out.add(item, line_of(block, i), getattr(block, "ops", [None] * len(block))[i] or "=")


def reduce_trigger(items, ctx):
    """A trigger block with the statically known parts removed: true has_dlc/tag/date atoms are
    dropped, if/else chains whose limit is static are replaced by the branch that applies, and
    NOT/OR/AND wrappers that become empty are dropped. A false atom stays (the block is false)."""
    from .script import trigger_one  # local: keep the module import list short

    out = Block(getattr(items, "line", 1))
    ops = getattr(items, "ops", None)
    taken = None
    for i, (k, v) in enumerate(items):
        kl = (k or "").lower()
        op = (ops[i] if ops is not None and i < len(ops) else "=") or "="
        if kl == "if" and isinstance(v, list):
            cond = trigger(get1(v, "limit", []), ctx)
            if cond is True:
                body = _sub(v, ("limit", "else", "else_if"))
                _extend(out, reduce_trigger(body, ctx))
                taken = True
                continue
            if cond is False:
                taken = False  # the else/else_if siblings that follow decide
                continue
            taken = None
            out.add((k, v), line_of(items, i), op)
            continue
        if kl in ("else", "else_if") and isinstance(v, list):
            if taken is False:
                cond = trigger(get1(v, "limit", []), ctx) if kl == "else_if" else True
                if cond is True:
                    _extend(out, reduce_trigger(_sub(v, ("limit",)), ctx))
                    taken = True
                elif cond is None:
                    out.add(("if", v), line_of(items, i), op)
                    taken = None
            elif taken is None:
                out.add((k, v), line_of(items, i), op)
            continue
        taken = None
        if kl in ("has_dlc", "tag", "original_tag", "has_start_date", "always") and isinstance(v, str):
            if trigger_one(k, v, ctx, op) is True:
                continue
        if kl in ("and", "not", "or", "hidden_trigger") and isinstance(v, list):
            r = trigger_one(k, v, ctx, op) if kl != "hidden_trigger" else None
            if r is True:
                continue
            inner = reduce_trigger(v, ctx) if kl in ("and", "hidden_trigger") else v
            if not inner:
                continue
            out.add((k, inner), line_of(items, i), op)
            continue
        out.add((k, v), line_of(items, i), op)
    return out


_STATE_KEYS = ("has_full_control_of_state", "controls_state", "owns_state", "is_owned_and_controlled_by")
_PROV_KEYS = ("controls_province",)


def control_lists(items):
    """{"states": [...], "provinces": [...], "how": [...]} of the state and province control conditions
    in a trigger (has_full_control_of_state, controls_state, owns_state, controls_province,
    `<state> = { is_controlled_by = ... }`, count_triggers amount)."""
    states, provs, how = [], [], set()

    def walk(block, inside_not=False):
        for k, v in block:
            kl = (k or "").lower()
            if kl == "not" and isinstance(v, list):
                walk(v, True)
                continue
            if kl in _STATE_KEYS and isinstance(v, str) and v.isdigit():
                if not inside_not:
                    states.append(int(v))
                    how.add(kl)
            elif kl in _PROV_KEYS and isinstance(v, str) and v.isdigit():
                if not inside_not:
                    provs.append(int(v))
                    how.add(kl)
            elif k and k.isdigit() and isinstance(v, list):
                if any((a or "").lower() in ("is_controlled_by", "is_fully_controlled_by", "is_owned_by",
                                             "is_owned_and_controlled_by") for a, _ in v) and not inside_not:
                    states.append(int(k))
                    how.add("state scope")
                walk(v, inside_not)
            elif kl == "count_triggers" and isinstance(v, list):
                amount = get1(v, "amount")
                if amount:
                    how.add(f"count_triggers amount {amount}")
                walk(v, inside_not)
            elif isinstance(v, list):
                walk(v, inside_not)

    walk(items)
    if not states and not provs:
        return None
    return {"states": sorted(set(states)), "provinces": sorted(set(provs)), "how": sorted(how)}


_EFFECT_SKIP = {"custom_effect_tooltip", "effect_tooltip", "hidden_effect", "log", "tooltip", "limit"}


def effect_summary(items, limit=12, depth=0):
    """Short list of what an effect block does: ["idea:X", "build:industrial_complex x2@161", "event:Y",
    "pp:+120", "wargoal:ETH", "flag:Z", "annex:ETH", ...] (scopes walked; at most `limit` entries)."""
    out = []

    def add(s):
        if s not in out:
            out.append(s)

    def walk(block, scope=None, d=0):
        for k, v in block:
            if len(out) >= limit:
                return
            kl = (k or "").lower()
            if kl in ("add_ideas", "swap_ideas", "remove_ideas", "add_timed_idea"):
                if isinstance(v, list):
                    if kl == "swap_ideas":
                        add(f"swap:{get1(v, 'remove_idea')}->{get1(v, 'add_idea')}")
                    elif kl == "add_timed_idea":
                        add(f"idea:{get1(v, 'idea')}({get1(v, 'days')}d)")
                    else:
                        for x in bare(v):
                            add(("idea:" if kl == "add_ideas" else "remove:") + x)
                else:
                    add(("idea:" if kl == "add_ideas" else "remove:") + str(v))
            elif kl == "add_building_construction" and isinstance(v, list):
                lvl = get1(v, "level")
                add(f"build:{get1(v, 'type')} x{lvl}" + (f"@{scope}" if scope else ""))
            elif kl in ("add_extra_state_shared_building_slots",):
                add(f"slots:+{v}" + (f"@{scope}" if scope else ""))
            elif kl == "add_research_slot":
                add(f"research_slot:+{v}")
            elif kl == "add_tech_bonus" and isinstance(v, list):
                what = get1(v, "category") or get1(v, "technology") or get1(v, "name")
                add(f"tech_bonus:{what} {get1(v, 'bonus')}x{get1(v, 'uses', '1')}")
            elif kl in ("add_political_power", "add_stability", "add_war_support", "add_manpower",
                        "army_experience", "navy_experience", "air_experience", "add_command_power"):
                add(f"{kl.replace('add_', '')}:{v}" if not isinstance(v, list) else kl)
            elif kl == "create_wargoal" and isinstance(v, list):
                add(f"wargoal:{get1(v, 'target')}({get1(v, 'type')})")
            elif kl in ("declare_war_on",) and isinstance(v, list):
                add(f"war:{get1(v, 'target')}")
            elif kl in ("annex_country", "puppet", "white_peace", "transfer_state", "set_state_owner",
                        "add_state_core", "add_core_of", "give_military_access", "release", "add_to_faction",
                        "create_faction", "set_capital", "add_state_claim", "transfer_state_to"):
                tv = get1(v, "target") if isinstance(v, list) else v
                add(f"{kl}:{tv}" + (f"@{scope}" if scope and kl in ('add_core_of', 'add_state_core') else ""))
            elif kl in ("country_event", "news_event", "state_event", "unit_leader_event"):
                add("event:" + str(get1(v, "id") if isinstance(v, list) else v))
            elif kl in ("set_country_flag", "set_global_flag", "clr_country_flag", "clr_global_flag"):
                f = get1(v, "flag") if isinstance(v, list) else v
                add(("flag:" if kl.startswith("set") else "clear_flag:") + str(f))
            elif kl in ("set_variable", "add_to_variable", "subtract_from_variable"):
                if isinstance(v, list) and v:
                    a, b = v[0]
                    if a == "var":
                        a, b = get1(v, "var"), get1(v, "value")
                    add(f"var:{a}{'=' if kl == 'set_variable' else ('+' if kl.startswith('add') else '-')}{b}")
            elif kl in ("unlock_decision_tooltip", "unlock_decision_category_tooltip", "activate_mission",
                        "activate_decision", "activate_targeted_decision"):
                add(f"{kl.replace('_tooltip', '')}:{get1(v, 'decision') if isinstance(v, list) else v}")
            elif kl in ("complete_national_focus", "unlock_national_focus", "load_focus_tree"):
                add(f"{kl}:{v if not isinstance(v, list) else get1(v, 'tree')}")
            elif kl in ("add_dynamic_modifier", "remove_dynamic_modifier") and isinstance(v, list):
                add(f"{kl.replace('_dynamic_modifier', '')}_dynmod:{get1(v, 'modifier')}")
            elif kl == "set_rule":
                add("set_rule")
            elif kl in ("add_equipment_to_stockpile",) and isinstance(v, list):
                add(f"equipment:{get1(v, 'type')} {get1(v, 'amount')}")
            elif kl in ("add_offsite_building", "add_resource"):
                add(kl)
            elif kl == "if" and isinstance(v, list) and d < 4:
                walk([(a, b) for a, b in v if (a or "").lower() not in ("limit",)], scope, d + 1)
            elif isinstance(v, list) and d < 4 and kl not in ("limit", "tooltip"):
                if k and k.isdigit():
                    walk(v, k, d + 1)
                elif kl.startswith(("random_", "every_", "any_")) or kl in _EFFECT_SKIP or \
                        re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") or kl in ("else", "else_if", "capital_scope",
                                                                          "owner", "controller", "root", "from"):
                    walk(v, scope if not re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") else k, d + 1)
                elif kl not in ("ai_will_do", "modifier") and re.match(r"^[a-z]", k or "") and \
                        kl.endswith("_effect"):
                    add(f"effect:{k}")
            elif isinstance(v, str) and v == "yes" and kl.endswith("_effect"):
                add(f"effect:{k}")

    walk(items)
    return out


def _names_tag(items, tag):
    """True when a parsed block names the tag (tag = X, original_tag = X, X = {...})."""
    if not isinstance(items, list):
        return False
    for k, v in items:
        if k in ("tag", "original_tag") and v == tag:
            return True
        if k == tag:
            return True
        if isinstance(v, list) and _names_tag(v, tag):
            return True
    return False
