"""The game's own conditions for what a plan schedules, read against the plan's own timeline.

A plan dates focuses, decisions, advisors, laws and declarations. The game files say when each can be
taken: a focus's `available`, prerequisites and mutually exclusive focuses; a decision's (and its
category's) `visible`, `allowed` and `available`; an advisor's or law's `allowed`, `visible`, `available`
and cost. `plan_gates` evaluates a trigger under what the plan says about one day (date, war, focuses
done, start flags). This module adds what the plan's own timeline decides as well:

* land the plan expects to own by a day (the start plus every dated checkpoint `owner:` /
  `controls_state:` / `state_fully_controlled:` / `state_controller:` atom for the plan's tag);
* the war: at peace until the first war the plan dates (a `declare` entry or a dated `at_war` atom);
* the start government, subject status and ideas, and the ideas the plan buys by then;
* the army: start divisions minus the disband list plus what the plan's training lines can field by a day
  (the executor's queues, caps and training time, each division started only with its equipment in stock
  from the start stockpile and the planned lines), and the army manpower of those
  divisions. Numeric army gates become numbers by date: `has_army_manpower` and
  `fighting_army_strength_ratio` (a division count against the other side's start divisions, with the
  ratio read from the global variable the game sets at start when the gate names one).

Every trigger line the plan cannot decide is returned as an "unknown" line with its kind (stability,
war_support, factories, other_country, ...) so the lint can ask for a dated checkpoint instead of passing
the item silently.

Nothing here is country specific: gates come from the parsed game files (mods included) and facts from the
plan. Stdlib only, Python 3.9+.
"""
import os
import re
from collections import Counter

from . import derive, plan_gates
from .script import _pairs, render

WRAPPERS = ("custom_trigger_tooltip", "hidden_trigger", "custom_override_tooltip")
TEXT_KEYS = ("tooltip",)
STATE_KEYS = ("owns_state", "controls_state", "has_full_control_of_state")
NUMERIC_KINDS = {
    "has_stability": "stability", "has_war_support": "war_support",
    "num_of_factories": "factories", "num_of_civilian_factories": "civilian_factories",
    "num_of_military_factories": "military_factories", "num_of_naval_factories": "naval_factories",
    "industrial_complex": "civilian_factories", "arms_factory": "military_factories",
    "has_political_power": "political_power", "threat": "world_tension", "has_manpower": "manpower",
    "num_divisions": "divisions", "has_army_size": "divisions", "amount_research_slots": "research_slots",
    "has_army_experience": "army_xp", "has_navy_experience": "navy_xp", "has_air_experience": "air_xp",
    "command_power": "command_power",
}
# plan predicate atoms that check a quantity (a dated checkpoint on the same quantity covers the line)
ATOM_OF_KIND = {"stability": ("stability",), "war_support": ("war_support",), "factories": ("factories",),
                # no checkpoint atom counts civilian or military factories alone (factories:<TAG> is the total):
                # a horizon target of that kind, or a checkpoint naming the focus, covers such a gate
                "civilian_factories": ("civilian_factories",), "military_factories": ("military_factories",),
                "political_power": ("pp",), "world_tension": ("world_tension",), "manpower": ("manpower",),
                "army_xp": ("xp",), "navy_xp": ("xp",), "air_xp": ("xp",)}
_SET_GLOBAL = re.compile(r"^global\.([A-Za-z0-9_]+)$")
_NUM = re.compile(r"^-?\d+(\.\d+)?$")
_STATE_ATOM = re.compile(r"^(owner|controller|state_controller):(\d+)\s*==?\s*([A-Z][A-Z0-9]{2})$")
_STATE_BOOL = re.compile(r"^(controls_state|state_fully_controlled):(\d+)$")
_AT_WAR = re.compile(r"^(at_war|at_war_with):([A-Z][A-Z0-9]{2})$")


def _num(v):
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


# ---------------------------------------------------------------------------------------------- globals
def global_variables(game):
    """{name: {"value": number, "src": "file:line"}} of the global variables the game files set to a literal
    number (`set_variable = { global.X = N }` or `{ var = global.X value = N }`), the first setting found.
    Many are set once at the game start (on_actions); scripts can change them later, which the lint says."""

    def load():
        out = {}
        try:
            rels = {p: r for r, p, _ in game.content.merged().values()}
        except (AttributeError, TypeError, ValueError):
            rels = {}
        for folder in ("common/on_actions", "common/scripted_effects", "history/countries", "common/national_focus",
                       "events"):
            for path in game.files(folder):
                try:
                    with open(path, encoding="utf-8-sig", errors="replace") as fh:
                        if "global." not in fh.read():
                            continue
                except OSError:
                    continue
                rel = rels.get(path) or os.path.basename(path)

                def walk(block):
                    lines = getattr(block, "lines", None)
                    for i, (k, v) in enumerate(block):
                        if not isinstance(v, list):
                            continue
                        if (k or "").lower() == "set_variable":
                            name, value = None, None
                            for a, b in v:
                                m = _SET_GLOBAL.match(a or "")
                                if m and isinstance(b, str) and _NUM.match(b):
                                    name, value = m.group(1), float(b)
                                elif (a or "") == "var" and isinstance(b, str) and _SET_GLOBAL.match(b):
                                    name = _SET_GLOBAL.match(b).group(1)
                                elif (a or "") == "value" and isinstance(b, str) and _NUM.match(b):
                                    value = float(b)
                            if name and value is not None and name not in out:
                                line = lines[i] if lines is not None and i < len(lines) else None
                                out[name] = {"value": value, "src": f"{rel}:{line}" if line else rel}
                        else:
                            walk(v)
                walk(derive.parse_file(path))
        return out

    return game.cached("conditions:globals", load)


# ---------------------------------------------------------------------------------------------- the timeline
FORECAST_YEARS = 8  # the army forecast reaches at most this far past the start


class Timeline:
    """What the plan says about its own country over time, for the gates (see the module doc)."""

    def __init__(self, lint):
        self.lint = lint
        self.game = lint.game
        self.tag = lint.tag
        self.world = lint.world if lint.tag else None
        self.start = derive.day(lint.start)
        self.dlcs = lint.ctx.dlcs if lint.ctx else ()
        country = self.world.country(self.tag) if self.world is not None else {}
        party = (country.get("politics") or {}).get("ruling_party") if country else None
        self.government = party["v"] if isinstance(party, dict) else None
        rel = self.world.relations() if self.world is not None else {"subjects": {}}
        self.subject = any(self.tag in subs for subs in rel["subjects"].values()) if self.tag else None
        self.start_ideas = {f["v"] for f in country.get("ideas") or [] if f.get("v")} if country else set()
        flags = set()
        if self.world is not None:
            flags = self.world.start_global_flags() | {f["v"] for f in country.get("flags") or [] if f.get("v")}
        self.flags = flags
        self.own_start = set(derive.tag_states(self.game, lint.ctx, self.tag, lint.start)) if self.tag else set()
        self.gains = self._planned_gains()
        self.war_day, self.war_why = self._first_war()
        self._forecast = {}

    # ---- land
    def _planned_gains(self):
        """[(day, state)] the plan's dated checkpoints and phase entries expect the tag to own or hold."""
        out = []
        for c in self.lint.list_of("checkpoints"):
            d = derive.day(c.get("by")) if c.get("by") else None
            if d is None:
                continue
            for atom in _atoms(c.get("must")):
                s = self.state_of_atom(atom)
                if s is not None:
                    out.append((d, s))
        return sorted(out)

    def state_of_atom(self, atom):
        a = atom.strip()
        m = _STATE_ATOM.match(a)
        if m and m.group(3) == self.tag:
            return int(m.group(2))
        m = _STATE_BOOL.match(a)
        if m:
            return int(m.group(2))
        return None

    def owned(self, day):
        """States the plan expects the tag to own on day (the start plus the dated checkpoint gains)."""
        return self.own_start | {s for d, s in self.gains if d <= day}

    # ---- war
    def _first_war(self):
        if self.world is not None and self.tag and self.world.at_war_with(self.tag):
            return self.start, "at war at the start"
        days = []
        for i, e in enumerate(self.lint.list_of("diplomacy")):
            if e.get("action") in ("declare", "declare_war") and e.get("when") is not None:
                ds = self.lint_dates(e["when"])
                if ds:
                    days.append((max(ds), f"diplomacy[{i}] {e.get('id') or e.get('target')}"))
        for c in self.lint.list_of("checkpoints"):
            if any(_AT_WAR.match(a.strip()) for a in _atoms(c.get("must"))) and c.get("by"):
                d = derive.day(c["by"])
                if d is not None:
                    days.append((d, f"checkpoint {c.get('id')}"))
        if not days:
            return None, None
        return min(days)

    def lint_dates(self, pred):
        from .validate import predicate_dates
        return [derive.day(d) for d in predicate_dates(pred, self.lint.table) if derive.day(d) is not None]

    def at_war(self, day):
        """True / False / None: at war on day under the plan (at peace until the first war it dates; an AI can
        still declare, which the plan does not date)."""
        if self.war_day is None:
            return None
        return day >= self.war_day

    # ---- ideas
    def ideas(self, day):
        out = set(self.start_ideas)
        for p in self.lint.list_of("politics"):
            ds = self.lint_dates(p["when"]) if p.get("when") is not None else []
            if isinstance(p.get("idea"), str) and (max(ds) if ds else self.start) <= day:
                out.add(p["idea"])
        return out

    # ---- army
    def forecast(self, equipment=True):
        """{"rows": [(day, divisions, manpower)] weekly, "start": divisions at the start, "per_template": ...}.
        The executor's training: entries of one template add their queues (entry i trains while the template's
        lines in training are below the queues of the active entries up to i, within its cap; a cap counts the
        template's divisions fielded and in training), each division takes the longest training_time of its
        units, and starts only with its whole equipment need in stock (the start stockpile less the start
        divisions' shortfall, plus the planned lines' output, lint.line_output). equipment=False: what the lines
        would field with every need in stock (the demand the production lines are sized against)."""
        if equipment in self._forecast:
            return self._forecast[equipment]
        lt, w = self.lint, self.world
        subs = w.sub_units() if w is not None else {}
        units_of = self.template_units

        def mp(name):
            return sum((subs.get(u) or {}).get("manpower", 0) or 0 for u in units_of(name))

        def train_days(name):
            times = [(subs.get(u) or {}).get("training_time") for u in units_of(name)]
            times = [t for t in times if isinstance(t, (int, float)) and t > 0]
            return max(times) if times else 90.0

        divs = Counter(d["template"] for d in (w.oob(self.tag)["divisions"] if w is not None else []))
        start_n = sum(divs.values())
        db = lt.plan.get("disband") if isinstance(lt.plan.get("disband"), dict) else {}
        left = db.get("max") if isinstance(db.get("max"), int) else sum(divs[t] for t in db.get("templates") or [])
        for t in db.get("templates") or []:
            k = min(divs.get(t, 0), left)
            divs[t] -= k
            left -= k
        tr = lt.plan.get("training")
        rules = lt.plan.get("training_rules") if isinstance(lt.plan.get("training_rules"), dict) else {}
        lines = []
        for e in tr if isinstance(tr, list) else [tr]:
            if not isinstance(e, dict) or not isinstance(e.get("template"), str):
                continue
            when = e.get("when")
            first = self.start
            last = None
            if when is not None:
                ds = self.lint_dates(when)
                first = max(ds) if ds else self.start
                last = _before_day(when)
            made = self.template_day(e["template"])
            first = max(first, made) if made is not None else None
            if first is None:
                continue  # the template is never made under the plan (a tech it waits on is not researched)
            q = e.get("queue") if isinstance(e.get("queue"), (int, float)) else 1
            cap = e.get("cap") if isinstance(e.get("cap"), int) else (
                rules.get("cap") if rules.get("cap_template") in (None, e["template"]) and
                isinstance(rules.get("cap"), int) else None)
            lines.append({"template": e["template"], "from": first, "until": last, "queue": float(q), "cap": cap,
                          "days": train_days(e["template"])})
        total_cap = rules.get("cap") if isinstance(rules.get("cap"), int) and not rules.get("cap_template") else None
        count = Counter({k: float(v) for k, v in divs.items()})
        # equipment: the start stockpile less the start divisions' shortfall (reinforcement comes first), plus
        # what the planned lines make; a division starts training only with its whole need in stock
        fam = lambda k: derive.equipment_family(self.game, k)  # noqa: E731
        stock = None
        if equipment and w is not None and hasattr(lt, "line_output"):
            stock = Counter()
            for st in w.country(self.tag).get("stockpile") or []:
                stock[fam(st.get("v"))] += st.get("amount") or 0
            for dv in w.oob(self.tag)["divisions"]:
                for eq, a in self.template_need(dv["template"]).items():
                    stock[fam(eq)] -= a * max(0.0, 1.0 - dv.get("start_equipment_factor", 1.0))
        need = {t: Counter({fam(k): a for k, a in self.template_need(t).items() if a > 0})
                for t in {ln["template"] for ln in lines}}
        # the executor's balance rule: military factories on no line go to the line the divisions lack most
        # (never taken from a line); modelled as the idle factories of the economy forecast split over the
        # training templates' equipment by its IC per division, from the day they stand idle
        mil_rows, idle, ic_cost = [], [], {}
        if stock is not None:
            try:
                from . import economy as econmod
                mil_rows = econmod.forecast(lt, self)["rows"]
            except Exception:  # noqa: BLE001 - advisory: no idle factories then
                mil_rows = []
            rows_eq = derive.equipment_rows(self.game)
            made = [r["type"] for r in lt.start_lines()] + [x.get("equipment") for x in lt.list_of("production")
                                                             if isinstance(x.get("equipment"), str)]
            from . import force as forcemod
            for k in made:
                c = forcemod.unit_cost(lt, k)[0] if hasattr(lt, "list_of") else (rows_eq.get(k) or {}).get("cost")
                if c and fam(k) not in ic_cost:
                    ic_cost[fam(k)] = c
        started = []  # (day it deploys, template, divisions)
        rows = []
        by_template = []
        limits = []
        end = self.forecast_end()
        day = self.start
        obs_day, obs = self.observed_army()
        while day <= end:
            for i in range(len(started) - 1, -1, -1):
                if started[i][0] <= day:
                    count[started[i][1]] += started.pop(i)[2]
            if obs_day is not None and day >= obs_day:
                # a plan continued from a save: the army read in that save replaces the forecast to its date
                # (fielded divisions by template; the divisions in training then are not read and start anew)
                count = Counter({t: float(n) for t, n in obs.items()})
                started.clear()
                obs_day = None
            if stock is not None:
                for k, units in lt.line_output(day).items():
                    stock[fam(k)] += units * 7
                r = econmod.at(mil_rows, day) if mil_rows else None
                spare = max(0, r["mil"] - lt.factories_on_lines(day)) if r else 0
                while sum(n for _, n in idle) > spare:
                    idle.pop()
                if spare > sum(n for _, n in idle):
                    idle.append((day, spare - sum(n for _, n in idle)))
                active = {ln["template"] for ln in lines
                          if ln["from"] <= day and (ln["until"] is None or day < ln["until"])}
                ic = Counter()
                for t in active:
                    for f, a in need[t].items():
                        if f in ic_cost:
                            ic[f] += a * ic_cost[f]
                if idle and ic:
                    _, _, _, e0 = lt.production_mods(day)
                    made_ic = sum(n * lt.factory_speed(day) * lt.efficiency(e0, b, day) / 100.0 for b, n in idle)
                    for f, x in ic.items():
                        stock[f] += made_ic * x / sum(ic.values()) / ic_cost[f] * 7
            training = Counter()
            for _, t, n in started:
                training[t] += n
            # the executor's semantics: entries of one template add up, entry i trains while the template's
            # queued lines are below the queues of the active entries up to i, each within its own cap
            target = Counter()
            limit = {}
            for ln in lines:
                t = ln["template"]
                if day < ln["from"] or (ln["until"] is not None and day >= ln["until"]):
                    continue
                target[t] += ln["queue"]
                room = target[t] - training[t]
                why = "queue"
                if ln["cap"] is not None and ln["cap"] - count[t] - training[t] < room:
                    room, why = ln["cap"] - count[t] - training[t], "cap"
                if total_cap is not None and total_cap - sum(count.values()) - sum(training.values()) < room:
                    room, why = total_cap - sum(count.values()) - sum(training.values()), "cap"
                if room <= 0:
                    limit.setdefault(t, why)
                    continue
                if stock is not None and need[t]:
                    short = min(need[t], key=lambda f: max(0.0, stock[f]) / need[t][f])
                    if max(0.0, stock[short]) / need[t][short] < room:
                        room, why = max(0.0, stock[short]) / need[t][short], short
                    for f, a in need[t].items():
                        stock[f] -= a * room
                limit[t] = why
                if room > 0:
                    started.append((day + ln["days"], t, room))
                    training[t] += room
            rows.append((day, sum(count.values()), sum(mp(t) * n for t, n in count.items())))
            by_template.append(dict(count))
            limits.append(limit)
            day += 7
        self._forecast[equipment] = {"rows": rows, "by_template": by_template, "start": start_n, "lines": lines,
                          "limits": limits, "equipment_counted": stock is not None,
                          "manpower_per_division": {t: mp(t) for t in count}}
        return self._forecast[equipment]

    def observed_army(self):
        """(day, {template: divisions}) of force_design.observed: the army a save of this plan fields on that
        day, which replaces the forecast from then on; (None, {}) when the plan sets none."""
        fd = self.lint.plan.get("force_design") if isinstance(self.lint.plan.get("force_design"), dict) else {}
        ob = fd.get("observed") if isinstance(fd.get("observed"), dict) else {}
        day = derive.day(ob["date"]) if isinstance(ob.get("date"), str) else None
        divs = {t: n for t, n in (ob.get("divisions") or {}).items() if isinstance(n, int) and n >= 0}
        if day is None or not divs or day < self.start:
            return None, {}
        return day, divs

    def forecast_end(self):
        """Last day the army forecast covers: the plan horizon, extended to the latest declaration date and the
        latest force_design ready date (at most FORECAST_YEARS after the start), so the force table of later
        wars is checked on the same arithmetic."""
        lt = self.lint
        end = max([d for d, _ in lt.horizon()] or [self.start + 365])
        more = []
        for e in lt.list_of("diplomacy"):
            if e.get("action") in ("declare", "declare_war") and e.get("when") is not None:
                more += self.lint_dates(e["when"])
        fd = lt.plan.get("force_design") if isinstance(lt.plan.get("force_design"), dict) else {}
        for part in ("land", "air", "navy"):
            for r in fd.get(part) or []:
                if isinstance(r, dict) and r.get("ready") and derive.day(r["ready"]) is not None:
                    more.append(derive.day(r["ready"]))
        return min(max([end] + more), self.start + FORECAST_YEARS * 365)

    def template_day(self, name):
        """Day the template can exist under the plan: the start for a start template; for a plan template the
        last of its `when` dates, the research finish of the techs its `when` waits on and of the techs that
        unlock its units; None when one of those techs is never researched (or for an unknown name: the start,
        validate_plan reports it)."""
        cache = self.__dict__.setdefault("_template_days", {})
        if name in cache:
            return cache[name]
        lt = self.lint
        out = self.start
        if name not in lt.start_templates():
            for x in lt.list_of("templates"):
                if name not in (x.get("name"), x.get("id")):
                    continue
                when = x.get("when")
                days = list(self.lint_dates(when)) if when is not None else []
                end = self.start + FORECAST_YEARS * 365
                sched = lt.research_schedule(end) if hasattr(lt, "research_schedule") else None
                techs = [a.split(":", 1)[1].strip() for a in _atoms(when) if a.startswith("tech_researched:")]
                if sched is not None:
                    subs = derive.subunit_unlocks(self.game)
                    units = derive.unit_types(self.game)
                    for u in set(self.template_units(name)):
                        if (units.get(u) or {}).get("active") or not subs.get(u):
                            continue
                        got = [sched["finish"].get(t) if t not in sched["have"] else self.start for t in subs[u]]
                        got = [d for d in got if d is not None]
                        if not got:
                            cache[name] = None
                            return None
                        days.append(min(got))
                    for t in techs:
                        if t in sched["have"]:
                            continue
                        if sched["finish"].get(t) is None:
                            cache[name] = None
                            return None
                        days.append(sched["finish"][t])
                out = max([self.start] + days)
                break
        cache[name] = out
        return out

    def limit_at(self, day):
        """{template: what held its training that week}: "queue", "cap" or the equipment family short."""
        fc = self.forecast()
        i = max(0, min(len(fc["limits"]) - 1, (day - self.start) // 7)) if fc["limits"] else None
        return fc["limits"][i] if i is not None else {}

    def growth(self, day):
        """{template: divisions a day} the training lines add around day (four weeks either side, caps
        respected)."""
        fc = self.forecast()
        rows, bt = fc["rows"], fc["by_template"]
        if not rows:
            return {}
        i = max(0, min(len(rows) - 1, (day - self.start) // 7))
        a, b = max(0, i - 4), min(len(rows) - 1, i + 4)
        if b == a:
            return {}
        span = rows[b][0] - rows[a][0]
        return {t: (bt[b].get(t, 0) - bt[a].get(t, 0)) / span for t in bt[b]
                if bt[b].get(t, 0) > bt[a].get(t, 0)}

    def template_units(self, name):
        """Battalions and support companies of a template the plan fields (a start template, else a plan
        template by name or id), one entry per unit."""
        # a plan template with `base` edits that template in place: its battalions replace the start ones
        for x in self.lint.list_of("templates"):
            if x.get("base") == name and x.get("regiments"):
                from .validate import template_columns
                return [u for c in template_columns(x.get("regiments")) or [] for u in c] + \
                    list(x.get("support") or [])
        t = self.lint.start_templates().get(name)
        if t:
            return [u for u, n in t["regiments"].items() for _ in range(n)] + list(t["support"])
        for x in self.lint.list_of("templates"):
            if name in (x.get("name"), x.get("id")):
                from .validate import template_columns
                return [u for c in template_columns(x.get("regiments")) or [] for u in c] + \
                    list(x.get("support") or [])
        return []

    def template_need(self, name):
        """{equipment archetype: units} one division of the template needs."""
        subs = self.world.sub_units() if self.world is not None else {}
        out = Counter()
        for u in self.template_units(name):
            for eq, a in ((subs.get(u) or {}).get("need") or {}).items():
                out[eq] += a
        return out

    def army_at(self, day):
        """(divisions, army manpower) the forecast gives on day (the last weekly row not after it)."""
        rows = self.forecast()["rows"]
        best = rows[0] if rows else (day, 0, 0)
        for r in rows:
            if r[0] <= day:
                best = r
            else:
                break
        return best[1], best[2]

    def first_day(self, divisions=None, manpower=None):
        """First forecast day with at least that many divisions / that much army manpower, or None."""
        for d, n, m in self.forecast()["rows"]:
            if (divisions is None or n >= divisions) and (manpower is None or m >= manpower):
                return d
        return None

    def start_army(self, tag):
        """(divisions, army manpower) of a country's start OOB (its templates' battalions and support companies)."""
        if self.world is None:
            return None
        try:
            oob = self.world.oob(tag)
        except (KeyError, TypeError):
            return None
        subs = self.world.sub_units()
        man = 0
        for d in oob["divisions"]:
            t = oob["templates"].get(d["template"]) or {}
            man += sum((subs.get(u) or {}).get("manpower", 0) * n for u, n in (t.get("regiments") or {}).items())
            man += sum((subs.get(u) or {}).get("manpower", 0) for u in t.get("support") or [])
        return len(oob["divisions"]), man

    def manpower_per_division(self):
        """Mean army manpower per division of the plan's last forecast row."""
        rows = self.forecast()["rows"]
        return rows[-1][2] / rows[-1][1] if rows and rows[-1][1] else None

    def facts(self, day, done):
        return plan_gates.Facts(self.tag, self.dlcs, derive.fmt(derive.date_of(day)), self.at_war(day), (), done,
                                self.flags, self.owned(day), self.government)


def _before_day(pred):
    """The day of a `date < D` atom at the top level of an all-predicate (a line's end), or None."""
    for a in _atoms(pred):
        m = re.match(r"^\s*date\s*<\s*(\d{4}\.\d{1,2}\.\d{1,2})\s*$", a)
        if m:
            return derive.day(m.group(1))
    return None


def _atoms(pred):
    if isinstance(pred, str):
        yield pred
    elif isinstance(pred, list):
        for p in pred:
            yield from _atoms(p)
    elif isinstance(pred, dict):
        for p in pred.values():
            yield from _atoms(p)


# ---------------------------------------------------------------------------------------------- evaluation
class Env:
    """One evaluation: the timeline, the day, the focuses done, and the scope (own country or a state)."""

    def __init__(self, tl, day, done, scope=None):
        self.tl, self.day, self.done = tl, day, set(done)
        self.facts = tl.facts(day, self.done)
        self.scope = scope  # None (the plan's country) | ("state", id) | ("country", tag)


def evaluate(block, env):
    """(True / False / None, unknown lines) of a trigger block (implicit AND) under the plan's timeline.
    An unknown line is {"kind", "key", "text", "value"?, "derived"?}; when the result is False the lines that
    close it are in the second item instead, as {"kind": "closed", ...}."""
    if not isinstance(block, list):
        return None, []
    return _block(block, env, "and")


def _block(block, env, how):
    results = []
    items = list(_pairs(block))
    i = 0
    while i < len(items):
        k, v, op = items[i]
        kl = (k or "").lower()
        if kl == "if" and isinstance(v, list):
            # an if / else_if / else chain: the first branch whose limit holds
            chain = [(k, v)]
            j = i + 1
            while j < len(items) and (items[j][0] or "").lower() in ("else_if", "else"):
                chain.append((items[j][0], items[j][1]))
                j += 1
            results.append(_chain(chain, env))
            i = j
            continue
        if kl in TEXT_KEYS:
            i += 1
            continue
        results.append(_one(k, v, op, env))
        i += 1
    if how == "and":
        if any(r is False for r, _ in results):
            return False, [x for r, ls in results if r is False for x in ls]
        unknown = [x for r, ls in results if r is None for x in ls]
        return (None if any(r is None for r, _ in results) else True), unknown
    # or
    if any(r is True for r, _ in results):
        return True, []
    if any(r is None for r, _ in results):
        unknown = [x for r, ls in results if r is None for x in ls]
        if len(unknown) > 1:
            unknown = [{"kind": "any_of", "key": "OR", "text": " OR ".join(u["text"] for u in unknown),
                        "parts": unknown}]
        return None, unknown
    return False, [x for _, ls in results for x in ls]


def _chain(chain, env):
    for k, v in chain:
        if not isinstance(v, list):
            continue
        kl = (k or "").lower()
        limit = None
        body = []
        for a, b in v:
            if (a or "") == "limit":
                limit = b
            else:
                body.append((a, b))
        if kl == "else":
            return _block(body, env, "and")
        r, ls = _block(limit, env, "and") if isinstance(limit, list) else (True, [])
        if r is True:
            return _block(body, env, "and")
        if r is None:
            # the branch taken is not known: the chain is known only when every candidate agrees
            rb, lb = _block(body, env, "and")
            return (None, ls + lb) if rb is not False else (None, ls + lb)
    return True, []


def _line(kind, k, v, op, **extra):
    text = f"{k} {op} {{ {render(v, 140)} }}" if isinstance(v, list) else f"{k} {op} {v}"
    return dict({"kind": kind, "key": k, "text": text}, **extra)


def _one(k, v, op, env):
    kl = (k or "").lower()
    tl = env.tl
    if kl in WRAPPERS and isinstance(v, list):
        return _block([(a, b) for a, b in v if (a or "").lower() not in TEXT_KEYS], env, "and")
    if kl == "and" and isinstance(v, list):
        return _block(v, env, "and")
    if kl == "or" and isinstance(v, list):
        return _block(v, env, "or")
    if kl == "not" and isinstance(v, list):
        r, ls = _block(v, env, "or")
        if r is None:
            return None, [{"kind": "not", "key": "NOT", "text": "NOT { " + " ".join(x["text"] for x in ls) + " }",
                           "parts": ls}] if ls else []
        return (not r), ([] if r is False else [_line("closed", k, v, op)])
    if kl == "count_triggers" and isinstance(v, list):
        need = _num(next((b for a, b in v if a == "amount"), None)) or 1
        parts = [_one(a, b, o, env) for a, b, o in _pairs(v) if a != "amount"]
        yes = sum(1 for r, _ in parts if r is True)
        maybe = sum(1 for r, _ in parts if r is None)
        if yes >= need:
            return True, []
        if yes + maybe < need:
            return False, [_line("closed", k, v, op)]
        return None, [_line("other", k, v, op)]
    if env.scope and env.scope[0] == "state":
        return _state_line(kl, k, v, op, env)
    # scopes
    if (k or "").isdigit() and isinstance(v, list):
        sub = Env(tl, env.day, env.done, ("state", int(k)))
        return _block(v, sub, "and")
    if kl in ("root", "this", "from") or k == tl.tag:
        return _block(v, env, "and") if isinstance(v, list) else (None, [_line("other", k, v, op)])
    if re.match(r"^[A-Z][A-Z0-9]{2}$", k or "") and isinstance(v, list):
        return None, [_line("other_country", k, v, op, country=k)]
    if kl == "has_war":
        r = plan_gates._yes(v, tl.at_war(env.day))
        if r is None:
            return None, [_line("war", k, v, op)]
        return r, ([] if r else [_line("closed", k, v, op, why=tl.war_why)])
    if kl in ("is_subject", "is_puppet") and v in ("yes", "no") and tl.subject is not None:
        r = tl.subject == (v == "yes")
        return r, ([] if r else [_line("closed", k, v, op)])
    if kl == "has_government" and isinstance(v, str) and tl.government:
        r = v == tl.government
        return r, ([] if r else [_line("closed", k, v, op)])
    if kl == "has_idea" and isinstance(v, str):
        if v in tl.ideas(env.day):
            return True, []
        return None, [_line("idea", k, v, op)]
    if kl in STATE_KEYS and isinstance(v, str) and v.isdigit():
        if int(v) in tl.owned(env.day):
            return True, []
        return None, [_line("land", k, v, op, state=int(v))]
    if kl == "has_army_manpower" and isinstance(v, list):
        size = _num(next((b for a, b in v if a == "size"), None))
        if size is None:
            return None, [_line("other", k, v, op)]
        divs, man = tl.army_at(env.day)
        need = size + 1 if op in ("=", ">") else size
        if man >= need:
            return True, []
        return False, [_line("army_manpower", k, v, op, need=need, have=round(man), divisions=round(divs),
                             reach=tl.first_day(manpower=need))]
    if kl == "fighting_army_strength_ratio" and isinstance(v, list):
        return _army_ratio(k, v, op, env)
    if kl in NUMERIC_KINDS:
        val = _num(v) if not isinstance(v, list) else None
        return None, [_line(NUMERIC_KINDS[kl], k, v, op, value=val, cmp=op)]
    r = plan_gates._one(k, v, op, env.facts)
    if r is None:
        kind = "flag" if kl in ("has_global_flag", "has_country_flag") else "other"
        return None, [_line(kind, k, v, op)]
    return r, ([] if r else [_line("closed", k, v, op)])


def _state_line(kl, k, v, op, env):
    sid = env.scope[1]
    tl = env.tl
    owned = sid in tl.owned(env.day)
    if kl in ("is_fully_controlled_by", "is_controlled_by", "is_owned_by", "is_owned_and_controlled_by") and \
            isinstance(v, str) and v in ("ROOT", "THIS", tl.tag):
        return (True, []) if owned else (None, [_line("land", f"{sid}.{k}", v, op, state=sid)])
    if kl in ("owner", "controller") and isinstance(v, list):
        r, ls = _block(v, Env(tl, env.day, env.done, ("country", None)), "and")
        return None, [_line("land", f"{sid}.{k}", v, op, state=sid)]
    return None, [_line("other", f"{sid}.{k}", v, op, state=sid)]


def _army_ratio(k, v, op, env):
    tl = env.tl
    other = next((b for a, b in v if a == "tag"), None)
    ratio_raw = next((b for a, b in v if a == "ratio"), None)
    rop = next((o for a, b, o in _pairs(v) if a == "ratio"), ">")
    var = None
    ratio = _num(ratio_raw)
    if ratio is None and isinstance(ratio_raw, str):
        m = _SET_GLOBAL.match(ratio_raw)
        g = global_variables(tl.game).get(m.group(1)) if m else None
        if g:
            ratio, var = g["value"], dict(g, name=m.group(1))
    theirs = tl.start_army(other) if isinstance(other, str) else None
    if ratio is None or not theirs or not theirs[1]:
        return None, [_line("army_ratio", k, v, op, country=other, ratio=ratio, var=var, theirs=theirs)]
    need = ratio * theirs[1]
    divs, man = tl.army_at(env.day)
    if rop in (">", ">=", "=") and man >= need:
        return True, []
    if rop in ("<", "<="):
        return (man < need), ([] if man < need else [_line("closed", k, v, op)])
    per = tl.manpower_per_division()
    return False, [_line("army_ratio", k, v, op, country=other, ratio=ratio, var=var, theirs=theirs,
                         need=int(need) + 1, need_divisions=int(need / per) + 1 if per else None, have=round(man),
                         divisions=round(divs), reach=tl.first_day(manpower=int(need) + 1))]


# ---------------------------------------------------------------------------------------------- readers
def focus_conditions(game, focus, tree):
    """{"available", "prereq", "exclusive", "cost", "bypass"} of a focus from the game files."""
    meta = derive.focus_meta(game).get(focus) or {}
    info = tree.get(focus) or {}
    return {"available": meta.get("available"), "prereq": info.get("prereq") or [],
            "exclusive": info.get("exclusive") or set(), "cost": meta.get("cost")}


def advisor_conditions(world, tag, idea):
    """[{"character", "slot", "idea_token", "cost", "allowed", "visible", "available"}] of the advisor roles
    an idea name stands for (a character id or an idea token)."""
    out = []
    if world is None or not isinstance(idea, str):
        return out
    chars = world.characters(tag, "advisor")
    # a character id names every advisor role of that character (Goering is both a hidden political advisor whose
    # idea_token equals his character id and an air chief): only a bare token selects roles by token
    by_token = not any(ch["id"] == idea for ch in chars) and \
        any(r["role"] == "advisor" and r.get("idea_token") == idea for ch in chars for r in ch["roles"])
    for ch in chars:
        for r in ch["roles"]:
            if r["role"] == "advisor" and (r.get("idea_token") == idea if by_token else ch["id"] == idea):
                out.append({"character": ch["id"], "slot": r.get("slot"), "idea_token": r.get("idea_token"),
                            "cost": r.get("cost"), "allowed": r.get("allowed_block"),
                            "visible": r.get("visible_block"), "available": r.get("available_block")})
    return out


def hired_exclusions(block):
    """Characters a trigger block needs NOT hired as advisors: `<char> = { is_hired_as_advisor = no }`."""
    out = []
    for k, v, _ in _pairs(block) if isinstance(block, list) else []:
        if isinstance(v, list):
            if any((a or "").lower() == "is_hired_as_advisor" and b == "no" for a, b in v):
                out.append(k)
            elif (k or "").lower() in ("and",) + WRAPPERS:
                out += hired_exclusions(v)
    return out


def describe(line):
    """One plain sentence for an unknown or closing line."""
    k = line.get("kind")
    if k == "army_ratio":
        var = line.get("var")
        src = f" (global {var['name']} = {var['value']:g}, {var['src']}; scripts can move it)" if var else ""
        if line.get("need") is not None:
            reach = line.get("reach")
            n, m = line["theirs"]
            return (f"fighting army strength above {line['ratio']:g} x {line['country']}{src}: {line['country']} starts "
                    f"with {n} divisions of {m:,.0f} men, so about {line['need']:,.0f} men under arms (about "
                    f"{line['need_divisions']} of the plan's divisions; estimated by army manpower, the game also weighs "
                    f"strength and equipment, so count only equipped divisions); the plan's training fields at most "
                    f"{line['have']:,.0f} ({line['divisions']} divisions) by then"
                    + (f" and the target first on {derive.fmt(derive.date_of(reach))}" if reach else
                       " and never that much before the horizon"))
        return f"{line['text']}{src}: not computable from the plan (the other side's start army is unknown)"
    if k == "army_manpower":
        reach = line.get("reach")
        return (f"army manpower of at least {line['need']:,.0f}: the plan's training fields at most "
                f"{line['have']:,.0f} ({line['divisions']} divisions) by then"
                + (f" and {line['need']:,.0f} first on {derive.fmt(derive.date_of(reach))}" if reach else
                   " and never that much before the horizon"))
    return line.get("text", "")
