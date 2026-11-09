"""Facts from the effective game content (install + enabled mods), for plan checks and the dossier.

Game(root, mods) reads every folder through content.Content, so a mod's file overrides and
replace_path apply everywhere. Readers are memoised per Game. Country history is applied as the
game does at the start date (dated blocks up to it, if-branches by the active DLCs when a Ctx is
given). Nothing here is country specific: tags, states, templates and ids all come from files.
"""
import os
import re
from collections import defaultdict

from . import adjacency
from .content import Content, integrated_dlcs
from .script import (DEFAULT_START, walk_all, bare, get, get1, history_walk, parse, parse_file, read,
                     run_effects, trigger)

EVENT_TYPES = ("country_event", "news_event", "state_event", "unit_leader_event",
               "operative_leader_event", "character_event")


class Game:
    def __init__(self, root, mods=(), content=None):
        self.root = root
        self.content = content or Content(root, mods)
        self._cache = {}

    # ------------------------------------------------------------------ files
    def path(self, *parts):
        """Install path (no mods); prefer file() for content."""
        return os.path.join(self.root, *parts)

    def file(self, rel):
        """Effective file of a content-relative path, or None."""
        return self.content.file(rel) if rel else None

    def files(self, folder, ext=(".txt",), recursive=False):
        return self.content.files(folder, ext, recursive)

    def cached(self, name, fn):
        if name not in self._cache:
            self._cache[name] = fn()
        return self._cache[name]

    def integrated_dlcs(self):
        """Display names of DLCs shipped inside the base game (always active)."""
        return self.cached("integrated_dlcs", lambda: integrated_dlcs(self.root))

    # ------------------------------------------------------------------ countries
    def country_tags(self):
        """{tag: country file} from common/country_tags."""

        def load():
            out = {}
            for f in self.files("common/country_tags"):
                for k, v in parse_file(f):
                    if k and isinstance(v, str) and re.match(r"^[A-Z][A-Z0-9]{2}$", k):
                        out[k] = v
            return out

        return self.cached("tags", load)

    def history_file(self, tag):
        """Content-relative history file of a tag ("history/countries/ITA - Italy.txt"), found by
        the tag at the start of the file name, or None."""

        def load():
            for p in self.files("history/countries"):
                name = os.path.basename(p)
                if re.match(rf"^{re.escape(tag)}(\s*-|\s|\.txt$)", name, re.I):
                    return "history/countries/" + name
            return None

        return self.cached("histfile:" + tag, load)

    def start_techs(self, history_file, start_date=DEFAULT_START):
        """Techs set by a history file at start_date, every if-branch read (no DLC list)."""

        def load():
            p = self.file(history_file)
            if not p:
                return set()
            have = set()

            def visit(k, v):
                if k == "set_technology" and isinstance(v, list):
                    have.update(a for a, b in v if a and b == "1")

            history_walk(parse_file(p), None, start_date, visit)
            return have

        return self.cached(f"start:{history_file}:{start_date}", load)

    def history_effects(self, history_file, ctx, start_date=DEFAULT_START):
        """DLC-aware start of a country history file at start_date (dated blocks up to it applied):
        {"techs": set, "oobs": [names], "capital": int or None, "research_slots": int or None}."""

        def load():
            out = {"techs": set(), "oobs": [], "capital": None, "research_slots": None}
            p = self.file(history_file)
            if not p:
                return out

            def visit(k, v):
                if k == "set_technology" and isinstance(v, list):
                    out["techs"].update(a for a, b in v if a and b == "1")
                elif k == "set_oob" and isinstance(v, str):
                    out["oobs"].append(v)
                elif k == "capital" and isinstance(v, str) and v.isdigit():
                    out["capital"] = int(v)
                elif k == "set_research_slots" and isinstance(v, str) and v.isdigit():
                    out["research_slots"] = int(v)

            history_walk(parse_file(p), ctx, start_date, visit)
            return out

        dl = sorted(ctx.dlcs) if ctx else None
        key = f"hist:{history_file}:{ctx.tag if ctx else ''}:{dl}:{start_date}"
        return self.cached(key, load)

    # ------------------------------------------------------------------ technologies
    def techs(self):
        """key -> {"parents": set, "deps": set, "year": int, "cost": str, "file": name}"""

        def load():
            techs = {}
            leads = defaultdict(set)
            for f in self.files("common/technologies"):
                for k, v in parse_file(f):
                    if k != "technologies" or not isinstance(v, list):
                        continue
                    for name, body in v:
                        if not name or not isinstance(body, list) or name.startswith("@"):
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
                        techs[name] = {
                            "deps": deps,
                            "year": int(year) if str(year).isdigit() else 0,
                            "cost": get1(body, "research_cost", "0"),
                            "file": os.path.basename(f),
                        }
            for name, info in techs.items():
                info["parents"] = leads.get(name, set())
            return techs

        return self.cached("techs", load)

    # ------------------------------------------------------------------ focus trees
    def tree_weights(self, ctx):
        """Focus tree id -> (weight for ctx, default flag), like the game's
        `country = { factor modifier = { add/factor ... } }` selection."""

        def load():
            out = {}
            for f in self.files("common/national_focus"):
                for k, v in parse_file(f):
                    if k != "focus_tree" or not isinstance(v, list):
                        continue
                    tid = get1(v, "id", os.path.basename(f))
                    weight = 0.0
                    country = get1(v, "country")
                    if isinstance(country, list):
                        try:
                            weight = float(get1(country, "factor", "0"))
                        except ValueError:
                            weight = 0.0
                        for mk, mv in country:
                            if mk != "modifier" or not isinstance(mv, list):
                                continue
                            conds = [(a, b) for a, b in mv if a not in ("add", "factor")]
                            result = trigger(conds, ctx)
                            if result is None:
                                # Unknown runtime conditions (e.g. "no other country with this
                                # original tag exists") hold at the start; the tag and DLC
                                # conditions decide which tree a country gets.
                                decisive = [(a, b) for a, b in conds
                                            if (a or "").lower() in ("tag", "original_tag", "has_dlc")]
                                result = True if decisive and trigger(decisive, ctx) is True else None
                            if result is not True:
                                continue  # unknown conditions do not pick a tree
                            try:
                                if get1(mv, "add") is not None:
                                    weight += float(get1(mv, "add"))
                                if get1(mv, "factor") is not None:
                                    weight *= float(get1(mv, "factor"))
                            except ValueError:
                                pass
                    out[tid] = (weight, get1(v, "default") == "yes")
            return out

        return self.cached(f"weights:{ctx.tag}:{sorted(ctx.dlcs)}", load)

    def loaded_tree(self, ctx):
        """The focus tree the game gives ctx.tag with these DLCs (highest positive
        weight, else the default tree)."""
        weights = self.tree_weights(ctx)
        best = max(weights.items(), key=lambda kv: (kv[1][0], kv[1][1]), default=None)
        if best and best[1][0] > 0:
            return best[0]
        for tid, (_, default) in weights.items():
            if default:
                return tid
        return None

    def focus_trees(self):
        """tree id -> {focus id -> {"prereq": [set], "exclusive": set, "cost": str}}"""

        def load():
            trees = {}
            for f in self.files("common/national_focus"):
                for k, v in parse_file(f):
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
                        focuses[fid] = {"prereq": prereq, "exclusive": exclusive, "cost": get1(fv, "cost", "1")}
                    trees[tid] = focuses
            return trees

        return self.cached("focus", load)

    # ------------------------------------------------------------------ economy
    def equipment(self):
        def load():
            keys = set()
            for f in self.files("common/units/equipment"):
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
            for f in self.files("common/buildings"):
                for k, v in parse_file(f):
                    if k == "buildings" and isinstance(v, list):
                        keys.update(n for n, b in v if n and isinstance(b, list))
            return keys

        return self.cached("buildings", load)

    # ------------------------------------------------------------------ states and provinces
    def _state_blocks(self):
        def load():
            out = {}
            for f in self.files("history/states"):
                st = get1(parse_file(f), "state")
                if not isinstance(st, list):
                    continue
                sid = get1(st, "id")
                if not sid or not str(sid).isdigit():
                    continue
                out[int(sid)] = (f, st)
            return out

        return self.cached("stateblocks", load)

    def state_names(self):
        """id -> set of localised names (STATE_<id> and tag-variant keys)."""

        def load():
            names = defaultdict(set)
            for p in self.files("localisation", ext=(".yml",), recursive=True):
                base = os.path.basename(p).lower()
                if "state_names" not in base or "english" not in base:
                    continue
                for line in read(p).splitlines():
                    m = re.match(r'\s*(?:[A-Z]{2,4}_)?STATE_(\d+):\d*\s+"(.*)"', line)
                    if m:
                        names[int(m.group(1))].add(m.group(2))
            return names

        return self.cached("statenames", load)

    def states(self):
        """id -> {"owner": first undated owner, "category", "names": set, "provinces": [int], "file"}"""

        def load():
            names = self.state_names()
            out = {}
            for sid, (f, st) in self._state_blocks().items():
                hist = get1(st, "history", [])
                provs = get1(st, "provinces", [])
                out[sid] = {
                    "owner": get1(hist, "owner", "") if isinstance(hist, list) else "",
                    "category": get1(st, "state_category", ""),
                    "names": names.get(sid, set()),
                    "provinces": [int(x) for x in bare(provs) if x.isdigit()],
                    "file": os.path.basename(f),
                }
            return out

        return self.cached("states", load)

    def province_state(self):
        """province id -> state id."""

        def load():
            out = {}
            for sid, s in self.states().items():
                for p in s["provinces"]:
                    out[p] = sid
            return out

        return self.cached("provstate", load)

    def state_facts(self, ctx, start_date=DEFAULT_START):
        """state id -> {"owner", "controller", "vps": {province: value}} at start_date (dated
        history blocks up to it applied; if-branches by ctx, every branch with ctx None)."""

        def load():
            out = {}
            for sid, (f, st) in self._state_blocks().items():
                hist = get1(st, "history", [])
                row = {"owner": None, "controller": None, "vps": {}}
                if isinstance(hist, list):

                    def visit(k, v, row=row):
                        if k in ("owner", "transfer_state_to") and isinstance(v, str):
                            row["owner"] = v
                            row["controller"] = v
                        elif k == "controller" and isinstance(v, str):
                            row["controller"] = v
                        elif k == "victory_points" and isinstance(v, list):
                            vals = bare(v)
                            for i in range(0, len(vals) - 1, 2):
                                if vals[i].isdigit():
                                    try:
                                        row["vps"][int(vals[i])] = float(vals[i + 1])
                                    except ValueError:
                                        pass

                    history_walk(hist, ctx, start_date, visit)
                out[sid] = row
            return out

        dl = sorted(ctx.dlcs) if ctx else None
        return self.cached(f"statefacts:{ctx.tag if ctx else ''}:{dl}:{start_date}", load)

    def state_owners(self, ctx, start_date=DEFAULT_START):
        """state id -> owner at start_date with ctx's DLCs."""
        return {sid: r["owner"] for sid, r in self.state_facts(ctx, start_date).items()}

    def neighbours(self, straits=True):
        """{province: set(provinces)} from the effective map, or None (see adjacency.py)."""
        return self.cached(f"adj:{straits}", lambda: adjacency.neighbours(self.content, straits))

    # ------------------------------------------------------------------ politics and decisions
    def ideas(self, ctx=None):
        """Known idea keys (ideas files), character ids and advisor idea tokens.
        With ctx, advisor roles whose `instance`/`allowed` block is false for the
        plan's tag and DLCs are left out."""

        def load():
            keys = set()
            for f in self.files("common/ideas"):
                for k, v in parse_file(f):
                    if k != "ideas" or not isinstance(v, list):
                        continue
                    for cat, body in v:
                        if isinstance(body, list):
                            keys.update(n for n, b in body if n and isinstance(b, list))
            chars = set()
            tokens = set()
            for f in self.files("common/characters"):
                for k, v in parse_file(f):
                    if k != "characters" or not isinstance(v, list):
                        continue
                    for cid, body in v:
                        if not cid or not isinstance(body, list):
                            continue
                        # 1.19 characters may wrap roles in per-DLC `instance` blocks
                        advs = list(get(body, "advisor"))
                        for inst in get(body, "instance"):
                            if not isinstance(inst, list):
                                continue
                            if ctx and trigger(get1(inst, "allowed", []), ctx) is False:
                                continue
                            advs.extend(get(inst, "advisor"))
                        for adv in advs:
                            if not isinstance(adv, list):
                                continue
                            if ctx and trigger(get1(adv, "allowed", []), ctx) is False:
                                continue
                            chars.add(cid)
                            tok = get1(adv, "idea_token")
                            if tok:
                                tokens.add(tok)
            return keys, chars, tokens

        key = "ideas" if ctx is None else f"ideas:{ctx.tag}:{sorted(ctx.dlcs)}"
        return self.cached(key, load)

    def decisions(self):
        def load():
            keys = set()
            for f in self.files("common/decisions"):
                for k, v in parse_file(f):
                    if isinstance(v, list) and k and not k.startswith("@"):
                        keys.update(n for n, b in v if n and isinstance(b, list))
            return keys

        return self.cached("decisions", load)

    # ------------------------------------------------------------------ doctrines (1.19)
    def doctrines(self, ctx=None):
        """{"folders": {name: allowed (True/False/None)}, "grand": {id: {folder, xp_cost, tracks}},
        "tracks": set, "sub": {id: {track, xp_cost, xp_type}}} from common/doctrines/**."""

        def load():
            out = {"folders": {}, "grand": {}, "tracks": set(), "sub": {}}

            def xp(body):
                try:
                    return float(get1(body, "xp_cost", "0"))
                except ValueError:
                    return 0.0

            for f in self.files("common/doctrines/folders"):
                for k, v in parse_file(f):
                    if k and isinstance(v, list):
                        allowed = get1(v, "allowed")
                        out["folders"][k] = trigger(allowed, ctx) if isinstance(allowed, list) else True
            for f in self.files("common/doctrines/grand_doctrines"):
                for k, v in parse_file(f):
                    if k and isinstance(v, list):
                        out["grand"][k] = {"folder": get1(v, "folder"), "xp_cost": xp(v),
                                           "tracks": bare(get1(v, "tracks", []))}
            for f in self.files("common/doctrines/tracks"):
                for k, v in parse_file(f):
                    if k and isinstance(v, list):
                        out["tracks"].add(k)
            for f in self.files("common/doctrines/subdoctrines", recursive=True):
                for k, v in parse_file(f):
                    if k and isinstance(v, list):
                        out["sub"][k] = {"track": get1(v, "track"), "xp_cost": xp(v),
                                         "xp_type": get1(v, "xp_type")}
            return out

        key = "doctrines" if ctx is None else f"doctrines:{ctx.tag}:{sorted(ctx.dlcs)}"
        return self.cached(key, load)

    # ------------------------------------------------------------------ orders of battle
    def oob(self, name, ctx=None):
        """{"templates": set(names), "divisions": [{"template", "location"}], "file"} of
        history/units/<name>.txt, with if-branches by ctx (all branches with ctx None)."""

        def load():
            out = {"templates": set(), "divisions": [], "file": None}
            p = self.file(f"history/units/{name}.txt")
            if not p:
                return out
            out["file"] = p

            def visit(k, v):
                if not isinstance(v, list):
                    return
                if k == "division_template":
                    n = get1(v, "name")
                    if n:
                        out["templates"].add(n)
                elif k == "division":
                    loc = get1(v, "location")
                    out["divisions"].append({
                        "template": get1(v, "division_template"),
                        "location": int(loc) if isinstance(loc, str) and loc.isdigit() else None,
                    })
                elif k in ("units",) or k is None:
                    walk(v)

            def walk(items):
                if ctx is None:
                    walk_all(items, visit)
                else:
                    run_effects(items, ctx, visit)

            walk(parse_file(p))
            return out

        dl = sorted(ctx.dlcs) if ctx else None
        return self.cached(f"oob:{name}:{ctx.tag if ctx else ''}:{dl}", load)

    def template_names(self):
        """Every division template name in history/units (any OOB)."""

        def load():
            names = set()
            for p in self.files("history/units"):
                names.update(re.findall(r'division_template\s*=\s*\{[^{}]*?name\s*=\s*"([^"]+)"', read(p)))
            return names

        return self.cached("templatenames", load)

    # ------------------------------------------------------------------ events
    def event_files(self):
        return self.files("events")

    def event(self, eid):
        """{"id", "type", "options": [option names], "file"} of one event, or None. Only the
        files whose text names the id are parsed."""
        cache = self.cached("events", dict)
        if eid in cache:
            return cache[eid]
        texts = self.cached("eventtexts", lambda: {p: read(p) for p in self.event_files()})
        needle = re.compile(r"\bid\s*=\s*\"?" + re.escape(eid) + r"\"?(?=[\s}])")
        found = None
        for p, text in texts.items():
            if eid not in text or not needle.search(text):
                continue
            for k, v in parse_file(p):
                if k not in EVENT_TYPES or not isinstance(v, list) or get1(v, "id") != eid:
                    continue
                options = []
                for o in get(v, "option"):
                    if isinstance(o, list):
                        options.append(get1(o, "name"))
                found = {"id": eid, "type": k, "options": options, "file": p,
                         "hidden": get1(v, "hidden") == "yes"}
        cache[eid] = found
        return found
