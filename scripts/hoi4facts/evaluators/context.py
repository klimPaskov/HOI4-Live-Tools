"""The situation an evaluator judges: one country at one date, from the game's files and an optional live snapshot.

Situation.load(game_dir, tag, date, mods, dlcs, state) builds hoi4facts.world.World over the effective content
(install + mods, later files overriding, content.Content) and keeps the live-state snapshot `state` beside it. Every
reader below prefers the snapshot and falls back to the files at the bookmark:

    techs()          researched technologies: state["techs"], else the history's start techs; for a date after the
                     bookmark year (assume="auto"/"on-time") also every technology of a year up to the date's whose
                     folder is available (on-time research, the first of each XOR group), said so in `assumed`
    doctrines()      grand and sub doctrines held (state["doctrines"] or the history)
    ideas()          ideas held (state["ideas"] or the history's), without the start-screen proxies of dynamic modifiers
    modifiers()      {modifier: total} and the per-source list: ideas, dynamic modifiers (their variables resolved with
                     the history's set_variable values or state["variables"]) and state["modifiers"] (a live read)
    factories()      civilian, military and naval factories (state["factories"] or the owned states' buildings)
    enemies(given)   the enemies to judge against: given tags, else state["enemies"], else the wars at the start, else
                     the two neighbours with the most military factories (a prior, printed as such)
    define(key)      a define's value with lists kept (defines.Defines), and its source

The snapshot is plain JSON written by the live tools or by hand: {"date", "techs", "doctrines", "ideas",
"variables": {name: value}, "modifiers": {key: value}, "factories": {"civilian", "military", "naval"},
"stockpile": {equipment: n}, "research_bonuses": [{"categories": [...], "bonus", "uses", "ahead_reduction"}],
"completed_focuses": [...], "enemies": [...], "manpower": n, "enemy_designs": {TAG: [{"type", "modules",
"upgrades"}]}}. Every key is optional.
"""
import os

from .. import derive
from ..defines import Defines
from ..game import Game
from ..world import World

NUMBER = (int, float)


def _year(date):
    try:
        return int(str(date).split(".")[0])
    except ValueError:
        return 1936


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


class Situation:
    def __init__(self, world, state=None, assume="auto"):
        self.world = world
        self.game = world.game
        self.tag = world.tag
        self.date = (state or {}).get("date") or world.date
        self.year = _year(self.date)
        self.state = dict(state or {})
        self.assume = assume
        self._defines = Defines(self.game.content)
        self._cache = {}

    @classmethod
    def load(cls, game_dir, tag, date="1936.1.1", mods=(), dlcs=None, mode="fair", state=None, assume="auto",
             bookmark=None):
        """`date` is the date judged; the history is applied up to `bookmark` (default: the date itself, so dated
        history blocks up to it count). dlcs None: the installed DLCs (dossier.dlcs_auto)."""
        game = game_dir if isinstance(game_dir, Game) else Game(game_dir, list(mods))
        if dlcs is None:
            from .. import dossier
            dlcs, _ = dossier.dlcs_auto(game.root, None)
        world = World(game, tag, bookmark or date, list(dlcs or ()), mode)
        sit = cls(world, state, assume)
        sit.date = (state or {}).get("date") or date
        sit.year = _year(sit.date)
        return sit

    def cached(self, key, fn):
        if key not in self._cache:
            self._cache[key] = fn()
        return self._cache[key]

    def other(self, tag):
        """The same files and date for another country (an enemy as our country sees it: files only, FAIR)."""
        def build():
            w = World(self.game, tag, self.world.date, self.world.dlcs, self.world.mode)
            s = Situation(w, {"date": self.date}, self.assume)
            return s
        return self.cached("other:" + tag, build)

    # ------------------------------------------------------------------ defines
    def define(self, key, default=None):
        v = self._defines.get(key, None)
        return default if v is None else v

    def define_src(self, key):
        full, row = self._defines.row(key)
        if not row:
            return f"{key} (not in defines)"
        return f"{full} = {row['v']} ({self.world.rel(row['file'])}:{row['line']})"

    # ------------------------------------------------------------------ technology
    def tech_table(self):
        from .. import research as base_research
        return self.cached("techtable", lambda: base_research.techs(self.world))

    def start_techs(self):
        return set(self.world.country(self.tag)["techs"])

    def techs(self):
        def build():
            if self.state.get("techs") is not None:
                return set(self.state["techs"]), "live snapshot"
            have = self.start_techs()
            bookmark = _year(self.world.date)
            if self.assume == "start" or (self.assume == "auto" and self.year <= bookmark):
                return have, f"history at {self.world.date}"
            table = self.tech_table()
            taken_xor = set()
            for tid in sorted(table, key=lambda t: (table[t]["year"] or 0, t)):
                t = table[tid]
                if t.get("doctrine") or not t["year"] or t["year"] > self.year or tid in have:
                    continue
                if set(t.get("xor") or ()) & (have | taken_xor):
                    continue
                have.add(tid)
                taken_xor.add(tid)
            return have, f"history at {self.world.date} + on-time research of every technology up to {self.year}"
        return self.cached("techs", build)[0]

    def techs_source(self):
        self.techs()
        return self._cache["techs"][1]

    def doctrines(self):
        if self.state.get("doctrines") is not None:
            return list(self.state["doctrines"])
        return [d["v"] for d in self.world.country(self.tag)["doctrines"]]

    # ------------------------------------------------------------------ ideas and modifiers
    def dynamic_modifier_names(self):
        if self.state.get("dynamic_modifiers") is not None:
            return list(self.state["dynamic_modifiers"])
        return [d["v"] for d in self.world.country(self.tag)["dynamic_modifiers"] if d.get("v")]

    def ideas(self):
        if self.state.get("ideas") is not None:
            return list(self.state["ideas"])
        from ..economy import idea_table
        table = idea_table(self.game)
        dms = set(self.dynamic_modifier_names())
        out = []
        for f in self.world.country(self.tag)["ideas"]:
            name = f.get("v")
            if (table.get(name) or {}).get("shown_as") in dms:
                continue  # the start screen's proxy of a dynamic modifier, counted below from the modifier
            out.append(name)
        return out

    def variables(self):
        out = {}
        for r in self.world.country(self.tag).get("variables") or []:
            if r.get("name") and isinstance(r.get("v"), NUMBER):
                out[r["name"]] = float(r["v"])
        out.update({k: float(v) for k, v in (self.state.get("variables") or {}).items() if isinstance(v, NUMBER)})
        return out

    def dynamic_modifier_table(self):
        def load():
            from ..script import parse_file
            out = {}
            for p in self.game.files("common/dynamic_modifiers"):
                for k, v in parse_file(p):
                    if k and isinstance(v, list):
                        out[k] = [(a, b) for a, b in v if a and isinstance(b, str)]
            return out
        return self.game.cached("eval:dynmods", load)

    def modifiers(self):
        """({modifier: total}, [(source, modifier, value)])."""
        def build():
            rows = []
            mods = derive.idea_modifiers(self.game)
            for idea in self.ideas():
                for k, v in (mods.get(idea) or {}).items():
                    rows.append((f"idea {idea}", k, v))
            var = self.variables()
            table = self.dynamic_modifier_table()
            for dm in self.dynamic_modifier_names():
                for k, raw in table.get(dm) or []:
                    if k in ("icon", "enable", "remove_trigger", "custom_modifier_tooltip", "attacker_modifier"):
                        continue
                    v = _num(raw)
                    if v is None:
                        v = var.get(raw)
                    if v is not None:
                        rows.append((f"dynamic modifier {dm}" + (f" ({raw})" if _num(raw) is None else ""), k, v))
            for tech, mods_t in self.tech_modifiers().items():
                for k, v in mods_t.items():
                    rows.append((f"tech {tech}", k, v))
            for k, v in (self.state.get("modifiers") or {}).items():
                if isinstance(v, NUMBER):
                    rows.append(("live read", k, float(v)))
            total = {}
            for _, k, v in rows:
                total[k] = total.get(k, 0.0) + v
            return total, rows
        return self.cached("modifiers", build)

    def tech_modifiers(self):
        """{tech: {modifier: value}} of the researched technologies: the country-wide numeric keys at a
        technology's top level (production_factory_max_efficiency_factor, research_speed_factor, ...). Unit and
        category blocks are the divisions' (divstats), not country modifiers."""
        table = tech_country_modifiers(self.game)
        return {t: table[t] for t in sorted(self.techs()) if table.get(t)}

    def modifier(self, key):
        return self.modifiers()[0].get(key, 0.0)

    # ------------------------------------------------------------------ economy
    def factories(self):
        if self.state.get("factories"):
            f = self.state["factories"]
            return {"civilian": float(f.get("civilian", 0)), "military": float(f.get("military", 0)),
                    "naval": float(f.get("naval", 0)), "source": "live snapshot"}

        def build():
            kinds = production_buildings(self.game)
            civ = mil = dock = 0.0
            states = self.world.states()
            for sid in self.world.owned_states(self.tag):
                for b, lvl in (states[sid].get("buildings") or {}).items():
                    k = kinds.get(b)
                    if k == "general":
                        civ += lvl
                    elif k == "military":
                        mil += lvl
                    elif k == "naval":
                        dock += lvl
            return {"civilian": civ, "military": mil, "naval": dock, "source": f"owned states at {self.world.date}"}
        return self.cached("factories", build)

    def research_slots(self):
        if self.state.get("research_slots"):
            return int(self.state["research_slots"])
        rs = self.world.country(self.tag).get("research_slots") or {}
        return int(rs.get("v") or 2)

    # ------------------------------------------------------------------ enemies
    def enemies(self, given=None):
        """([tags], how chosen)."""
        if given:
            return list(given), "given"
        if self.state.get("enemies"):
            return list(self.state["enemies"]), "live snapshot"
        wars = []
        try:
            wars = list(self.world.at_war_with(self.tag) or [])
        except Exception:  # noqa: BLE001 - the start wars are advisory
            wars = []
        if wars:
            return sorted(wars), "at war at the start"
        rows = []
        for nb in self.world.neighbours(self.tag):
            f = self.other(nb).factories()
            rows.append((f["military"] + f["civilian"] * 0.5, nb))
        rows.sort(reverse=True)
        return [t for _, t in rows[:2]], "prior: the two neighbours with the most factories (no war, no goal given)"


def production_buildings(game):
    """{building: "general" | "military" | "naval"} from common/buildings (general_production, military_production,
    naval_production = 1): the factory classes of any mod, by what the building produces, not by its name."""
    def load():
        from ..script import get1, parse_file
        out = {}
        for p in game.files("common/buildings"):
            for k, v in parse_file(p):
                if k != "buildings" or not isinstance(v, list):
                    continue
                for name, b in v:
                    if not name or not isinstance(b, list):
                        continue
                    for key, kind in (("general_production", "general"), ("military_production", "military"),
                                      ("naval_production", "naval")):
                        if _num(get1(b, key)):
                            out[name] = kind
        return out
    return game.cached("eval:prodbuildings", load)


TECH_FIELDS = {"research_cost", "start_year", "folder", "position", "xp_research_type", "xp_research_bonus",
               "xp_boost_cost", "xp_research_cost", "doctrine", "show_effect_as_desc", "force_use_small_tech_layout",
               "desc", "sub_technologies", "dependencies", "path", "categories", "ai_will_do", "allow", "allow_branch",
               "on_research_complete", "enable_equipments", "enable_equipment_modules", "enable_subunits",
               "enable_building", "enable_tactic", "special_project_specialization", "doctrine_name", "is_special_project",
               "XOR", "xor", "research_cost_coeff", "leads_to_tech"}


def tech_country_modifiers(game):
    def load():
        from ..script import parse_file
        out = {}
        for p in game.files("common/technologies"):
            for k, v in parse_file(p):
                if k != "technologies" or not isinstance(v, list):
                    continue
                for name, body in v:
                    if not name or not isinstance(body, list) or name.startswith("@"):
                        continue
                    mods = {}
                    for a, b in body:
                        if a and a not in TECH_FIELDS and isinstance(b, str) and _num(b) is not None:
                            mods[a] = mods.get(a, 0.0) + float(b)
                    if mods:
                        out[name] = mods
        return out
    return game.cached("eval:techmods", load)


def game_dir_default():
    return os.environ.get("HOI4_DIR")
