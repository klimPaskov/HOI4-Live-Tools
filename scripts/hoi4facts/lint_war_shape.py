"""The shape of each war the plan fights, as the executor will run it on the day the war starts (run by
lint_force.check after the force rules, on the same war list from force.plan_wars).

  war.axes_missing        an offensive war the plan starts whose phase on its first day names no war.axes (absent, or an
                          explicit empty list, with or without posture attack) and no operations: every army
                          then picks its own layout axis, victory-point row or nearest target, so the war has
                          no main effort and armies wander to rows of other wars.
  war.objective_not_at_war  an objective the executor can pick in a country the plan is not at war with on that
                          day: a vp_targets row dated (by) before its owner's war starts; an army's first open
                          row on the first day of a war it fights in (a row or a layout axis in the target's
                          land: an armour army sent to Liege in the Polish war); a
                          state in the phase's war.axes.
  war.hold_sector         on the first day of a war, an own border state facing a country the war does not
                          attack but that is at war with the plan or joins it (a guarantor, an ally of the
                          target, an enemy carried from an earlier war) with fewer than HOLD_MIN
                          divisions of a named formation (army_layout count over the states the entry covers,
                          war.guards) that is not in war.reserve: the executor never drives a reserve army, so
                          a border held only by one is open on the first day.
  war.mobile_axis         an army_layout strike formation with armour or motorised templates and no axis of its
                          own (none, or the same states as another strike formation): the mobile arm is spread
                          along the infantry's line instead of breaking through on its own sector.
  war.recovery_phase      a contingency that switches phase during an offensive war on single provinces
                          (province_controller / province_controlled atoms) instead of a state lost or a share
                          of land, or whose phase attacks nothing (war.axes [] or no axes over an empty plan
                          war.axes): one lost border province then holds the whole war.
  war.guarantor           a declaration on a country another country guarantees at the start (give_guarantee in
                          history/countries) while the plan has no war with the guarantor that day.
  war.declaration_gate    a focus in a declaration's `when` that the focus timeline completes less than GATE_SLACK
                          days before the declaration's date (or never): the war starts late.
  war.attack_force        on an offensive war's first day: the layout in force (entries whose `when` holds by then,
                          the latest per formation) places less than PLACED_SHARE of the forecast army; the strike
                          formations on the war's axes carry less than ATTACK_RATIO (or force_design.wars[]
                          .attack_ratio) x the enemy front estimate; or an armour or motorised formation is not
                          among them.
"""
from . import derive
from . import force as fm

GATE_SLACK = 14     # days a focus that gates a declaration finishes before the declaration's date [H]
ATTACK_RATIO = 1.0  # divisions on the war's axes per enemy division on the front, unless the war sets attack_ratio [H]
PLACED_SHARE = 0.8  # the layout in force on a war day places at least this share of the forecast army [H]
HOLD_MIN = 2   # divisions per border state: the executor's war-risk read wants at least 2 (borders_not_held)
RULES = ("war.axes_missing", "war.objective_not_at_war", "war.hold_sector", "war.mobile_axis", "war.recovery_phase",
         "war.guarantor", "war.declaration_gate", "war.attack_force")


def _fmt(d):
    return derive.fmt(derive.date_of(d)) if d is not None else "undated"


def _ints(xs):
    return [int(x) for x in xs or [] if isinstance(x, int) or (isinstance(x, str) and x.isdigit())]


class Shape:
    def __init__(self, lint, tl, wars):
        self.lint, self.tl, self.wars = lint, tl, [w for w in wars if w.get("start") is not None]
        self.caps = fm.capitulation_days(lint)
        self.owner0 = {sid: r.get("owner") for sid, r in lint.states().items()}
        self.pstate = lint.game.province_state()
        base = lint.plan.get("war") if isinstance(lint.plan.get("war"), dict) else {}
        self.base = base
        self.phases = []
        for i, p in enumerate(lint.list_of("phases")):
            if not isinstance(p, dict):
                continue
            d = fm.predicate_day(lint, tl, p.get("enter"), wars) if p.get("enter") is not None else None
            ov = (p.get("overrides") or {}).get("war") if isinstance(p.get("overrides"), dict) else None
            self.phases.append((d, i, p.get("id") or f"phases[{i}]", ov if isinstance(ov, dict) else {}))

    # ------------------------------------------------------------------ the plan on a day
    def war_at(self, day):
        """(phase id or None, the war block in force on that day: war with the latest entered phase's override)."""
        best = None
        for d, i, pid, ov in self.phases:
            if d is not None and d <= day and (best is None or (d, i) >= (best[0], best[1])):
                best = (d, i, pid, ov)
        eff = dict(self.base)
        if best:
            eff.update(best[3])
            return best[2], eff
        return None, eff

    def hostile(self, day):
        """Countries at war with the plan on a day (targets, joiners and those carried from earlier wars), less
        those the checkpoints capitulate before it."""
        out = set()
        for w in self.wars:
            if w["start"] <= day:
                out |= set(w["enemy_tags"])
        return {t for t in out if not (self.caps.get(t) is not None and self.caps[t] < day)}

    def taken(self, day):
        """Own states on a day under the plan: the timeline's gains plus the states of vp_targets rows due by then."""
        out = set(self.tl.owned(day))
        for e in self.lint.targets():
            by = derive.day(e["by"]) if isinstance(e, dict) and isinstance(e.get("by"), str) else None
            if by is not None and by <= day:
                out |= {self.pstate.get(p) for p in _ints(e.get("path")) + [e.get("province")]} - {None}
        return out

    def gone(self, day):
        return {t for t, d in self.caps.items() if d is not None and d < day}

    def guarantors(self, target):
        """Countries that guarantee `target` at the start (give_guarantee in history/countries, conditions decided
        for the start date). A guarantee the AI gives or drops later is not read."""
        if not hasattr(self, "_guar"):
            self._guar = {}
            w = self.lint.world
            for t in self.lint.game.country_tags():
                try:
                    for g in w.country(t)["guarantees"]:
                        self._guar.setdefault(g.get("v"), set()).add(t)
                except Exception:  # noqa: BLE001  (a tag with no history file)
                    continue
        return self._guar.get(target, set()) - {self.lint.tag}

    def likely(self, w):
        """Countries expected to join a war: every joiners_ok of the declarations on its first day and the
        phase's war.enemies on that day (guarantors and allies, whether or not they border the target)."""
        out = set(w["enemy_tags"])
        for e in self.lint.list_of("diplomacy"):
            if isinstance(e, dict) and e.get("action") in ("declare", "declare_war") and e.get("target") in w["targets"]:
                out |= {j for j in e.get("joiners_ok") or [] if isinstance(j, str)}
        out |= {t for t in self.war_at(w["start"])[1].get("enemies") or [] if isinstance(t, str)}
        for t in w["targets"]:
            out |= self.guarantors(t)
        return out - {self.lint.tag}

    def guarantees(self):
        """war.guarantor: a declaration on a country another country guarantees at the start, with the guarantor in
        none of the plan's wars on that day."""
        lint = self.lint
        joiners = set()
        for e in lint.list_of("diplomacy"):
            if isinstance(e, dict):
                joiners |= {j for j in e.get("joiners_ok") or [] if isinstance(j, str)}
        for w in self.wars:
            if "at war at the start" in w["source"]:
                continue
            day = w["start"]
            known = self.hostile(day) | set(w["enemy_tags"]) | self.gone(day)
            known |= {j for e in lint.list_of("diplomacy") if isinstance(e, dict) and e.get("target") in w["targets"]
                      for j in e.get("joiners_ok") or [] if isinstance(j, str)}
            known |= {t for t in self.war_at(day)[1].get("enemies") or [] if isinstance(t, str)}
            for t in w["targets"]:
                for g in sorted(self.guarantors(t) - known - set(w["targets"])):
                    later = self.first_war_day(g)
                    lint.add("error", "war.guarantor", f"war {w['id']}",
                             f"{w['id']} declares on {t} on {_fmt(day)}, and {g} guarantees {t} at the start "
                             "(history/countries give_guarantee): the guarantor enters that day, but the plan "
                             + (f"fights {g} only from {_fmt(later)}" if later else f"has no war with {g}")
                             + f". Plan {g} into this war (joiners_ok, its force and its border held) or end the "
                               "guarantee first (unconfirmed offline: whether the guarantee still stands that day)")

    def first_war_day(self, tag):
        ds = [w["start"] for w in self.wars if tag in w["enemy_tags"]]
        return min(ds) if ds else None

    def layout(self):
        return [e for e in self.lint.list_of("army_layout") if isinstance(e, dict)]

    # ------------------------------------------------------------------ rules
    def axes(self):
        for w in self.wars:
            # a war already running at the start may open with a hold (the plan did not choose its first day)
            if w["kind"] != "offensive" or "at war at the start" in w["source"]:
                continue
            pid, eff = self.war_at(w["start"])
            if eff.get("operations"):
                continue
            ax = eff.get("axes")
            if isinstance(ax, list) and any(_ints(a) for a in ax if isinstance(a, list)):
                continue
            how = ("no war.axes" if ax is None else "war.axes [] (a hold phase)" if eff.get("posture") != "attack"
                   else "posture attack with war.axes []")
            self.lint.add("error", "war.axes_missing", f"phases {pid}" if pid else "war",
                          f"{w['id']} on {', '.join(w['targets'])} starts {_fmt(w['start'])} under "
                          f"{pid or 'the top-level war block'} with {how}: name the axes (ordered state lists, one per "
                          "main effort, the armour's sector first) or operations, so no army picks its own target")

    def objectives(self):
        lint, tag = self.lint, self.lint.tag
        rows = [(i, e) for i, e in enumerate(lint.targets()) if isinstance(e, dict)]
        for i, e in rows:
            by = derive.day(e["by"]) if isinstance(e.get("by"), str) else None
            sid = self.pstate.get(e.get("province"))
            owner = self.owner0.get(sid)
            if by is None or owner in (None, tag) or sid in self.tl.owned(by) or owner in self.gone(by):
                continue
            first = self.first_war_day(owner)
            if first is not None and by < first:
                lint.add("error", "war.objective_not_at_war", f"vp_targets[{i}] {e['province']}",
                         f"due {_fmt(by)} in {owner}'s state {sid}, but the war with {owner} starts {_fmt(first)}: an "
                         "army sent there earlier walks into a neutral border; date it after the declaration")
        for w in self.wars:
            day = w["start"]
            hostile = self.hostile(day) | set(w["enemy_tags"])
            ok = hostile | {tag} | self.gone(day)
            taken = self.taken(day)
            pid, eff = self.war_at(day)
            reserve = set(eff.get("reserve") or [])
            # the armies this war uses: a row in the target's land, or a layout axis into it
            targets = set(w["targets"])
            fighting = {e.get("army") for _, e in rows
                        if self.owner0.get(self.pstate.get(e.get("province"))) in targets}
            fighting |= {e.get("name_hint") for e in self.layout()
                         if any(self.owner0.get(x) in targets for x in _ints(e.get("axis")))}
            seen = set()
            for i, e in sorted(rows, key=lambda r: (derive.day(r[1].get("by")) or 10 ** 9, r[0])):
                army = e.get("army")
                by = derive.day(e["by"]) if isinstance(e.get("by"), str) else None
                if not army or army not in fighting or army in seen or army in reserve or by is None or by < day:
                    continue
                seen.add(army)
                sid = self.pstate.get(e.get("province"))
                owner = self.owner0.get(sid)
                if owner and owner not in ok and sid not in taken:
                    lint.add("error", "war.objective_not_at_war", f"vp_targets[{i}] {e['province']}",
                             f"on {_fmt(day)}, the first day of {w['id']}, {army}'s first open row is province "
                             f"{e['province']} in {owner}'s state {sid}, a country not at war then: the army leaves the "
                             "war for it. Give the army its rows of this war first")
            for n, ax in enumerate(eff.get("axes") or []):
                bad = [s for s in _ints(ax) if self.owner0.get(s) not in ok and s not in taken]
                if bad:
                    lint.add("error", "war.objective_not_at_war", f"phases {pid}" if pid else "war",
                             f"war.axes[{n}] on {_fmt(day)} ({w['id']}) runs through "
                             + ", ".join(f"{s} ({self.owner0.get(s)})" for s in bad)
                             + ", land of countries not at war then")

    def holds(self):
        lint = self.lint
        nb = None
        for w in self.wars:
            day = w["start"]
            # the sectors the war does not attack: borders with the joiners, the likely joiners and enemies
            # carried from earlier wars; the target's own border is the attack front (diplomacy.borders_unheld)
            hostile = (self.hostile(day) | self.likely(w)) - set(w["targets"]) - self.gone(day)
            mine = self.taken(day)
            if nb is None:
                from .lint_conditions import _state_neighbours
                nb = _state_neighbours(lint)
            border = {s for s in mine if s in self.tl.owned(day)
                      and any(self.owner0.get(n) in hostile and n not in mine for n in nb.get(s, ()))}
            if not border:
                continue
            pid, eff = self.war_at(day)
            reserve = set(eff.get("reserve") or [])
            held = {}
            names = {}
            for e in self.layout():
                own = e.get("task_of") or e.get("name_hint")
                if own in reserve:
                    continue
                states = ([e["state"]] if isinstance(e.get("state"), int) else []) + _ints(e.get("covers")) + \
                    _ints(e.get("guard_states_in_order"))
                states = list(dict.fromkeys(states))
                if not states or not isinstance(e.get("count"), int):
                    continue
                share = e["count"] / len(states)
                for s in states:
                    held[s] = held.get(s, 0.0) + share
                    names.setdefault(s, []).append(e.get("name_hint") or e.get("id"))
            guards = eff.get("guards") if isinstance(eff.get("guards"), dict) else {}
            counts = {}
            for e in self.layout():
                n = e.get("task_of") or e.get("name_hint")
                if isinstance(e.get("count"), int):
                    counts[n] = counts.get(n, 0) + e["count"]
            for army, s in guards.items():
                if army not in reserve and isinstance(s, int):
                    held[s] = held.get(s, 0.0) + counts.get(army, 0)
                    names.setdefault(s, []).append(army)
            thin = sorted(s for s in border if held.get(s, 0.0) < HOLD_MIN)
            if not thin:
                continue
            res = sorted(a for a in reserve if any(
                (e.get("task_of") or e.get("name_hint")) == a and s in ([e.get("state")] + _ints(e.get("covers"))
                                                                     + _ints(e.get("guard_states_in_order")))
                for e in self.layout() for s in thin))
            facing = sorted({self.owner0.get(n) for s in thin for n in nb.get(s, ())
                             if self.owner0.get(n) in hostile and n not in mine})
            lint.add("error", "war.hold_sector", f"phases {pid}" if pid else "war",
                     f"{w['id']} ({_fmt(day)}): own border states facing {', '.join(facing)} with fewer than {HOLD_MIN} "
                     "divisions of a named formation outside war.reserve: "
                     + ", ".join(f"{s} ({held.get(s, 0):.1f})" for s in thin)
                     + (f"; held only by the reserve {', '.join(res)}, which the executor never drives" if res else "")
                     + ". Name a hold formation (army_layout guard entry or war.guards) with its divisions for each")

    def mobile(self):
        lint = self.lint
        strike = [e for e in self.layout() if e.get("role") == "strike" and not e.get("task_of")]
        prof = {}

        def kind(name):
            if name not in prof:
                prof[name] = fm.template_profile(lint.game, self.tl.template_units(name))["kind"]
            return prof[name]
        for i, e in enumerate(strike):
            mob = [t for t in e.get("templates") or [] if isinstance(t, str) and fm.is_mobile(kind(t))]
            if not mob:
                continue
            ax = tuple(_ints(e.get("axis")))
            name = e.get("name_hint") or e.get("id")
            if not ax:
                lint.add("error", "war.mobile_axis", f"army_layout {e.get('id') or name}",
                         f"{name} carries {', '.join(mob)} but has no axis: give the mobile formation its own "
                         "breakthrough axis (the sector it attacks, the province chain it drives to)")
                continue
            twins = [o.get("name_hint") or o.get("id") for o in strike
                     if o is not e and tuple(_ints(o.get("axis"))) == ax]
            if twins:
                lint.add("error", "war.mobile_axis", f"army_layout {e.get('id') or name}",
                         f"{name} carries {', '.join(mob)} on the same axis as {', '.join(twins)} "
                         f"({', '.join(map(str, ax))}): the mobile arm needs an axis of its own")


    def recovery(self):
        """war.recovery_phase: a contingency that switches phase while an offensive war runs."""
        lint = self.lint
        if not any(w["kind"] == "offensive" for w in self.wars):
            return
        phases = {p.get("id"): p for p in lint.list_of("phases") if isinstance(p, dict)}
        for i, c in enumerate(lint.list_of("contingencies")):
            if not isinstance(c, dict):
                continue
            then = c.get("then") if isinstance(c.get("then"), dict) else {}
            pid = c.get("phase") or then.get("phase")
            if not pid:
                continue
            where = f"contingencies[{i}] {c.get('id') or ''}".strip()
            when = c.get("when", c.get("trigger"))
            single = sorted({a for a in fm._atoms(when) if isinstance(a, str)
                             and a.split(":")[0] in ("province_controller", "province_controlled")})
            if single:
                lint.add("error", "war.recovery_phase", where,
                         f"switches to {pid} on single provinces ({', '.join(single[:4])}"
                         f"{', ...' if len(single) > 4 else ''}): one border province lost for a day stops the "
                         "offensive. Fire on a state lost ({\"not\": \"controls_state:<state>\"}: its controller is no longer "
                         "ours) or a share of land")
            ph = phases.get(pid) or {}
            ov = ((ph.get("overrides") or {}).get("war") if isinstance(ph.get("overrides"), dict) else None) or {}
            cov = (c.get("overrides") or then.get("overrides") or {})
            cov = cov.get("war") if isinstance(cov, dict) and isinstance(cov.get("war"), dict) else {}
            ov = dict(ov, **cov)
            if ov.get("operations"):
                continue
            ax = ov.get("axes")
            if isinstance(ax, list) and any(_ints(a) for a in ax if isinstance(a, list)):
                continue
            base = self.base.get("axes")
            inherits = ax is None and isinstance(base, list) and any(_ints(a) for a in base if isinstance(a, list))
            if inherits:
                continue
            lint.add("error", "war.recovery_phase", f"phases {pid}",
                     f"{pid} (entered by {c.get('id') or where}) attacks nothing: "
                     + ("war.axes []" if ax is not None else "no war.axes and the plan's war.axes are empty")
                     + ". A recovery phase of an offensive war carries a retake axis (the lost own states first) "
                       "followed by the attack axes, so the war goes on while the land is retaken")


    def declaration_gates(self):
        """war.declaration_gate: a focus in a declaration's `when` that the focus timeline completes later than
        GATE_SLACK days before the declaration's own date: the war starts on the focus, not on the plan's day."""
        lint = self.lint
        rows = (lint.facts.get("focus") or {}).get("schedule") or (lint.facts.get("focus") or {}).get("timeline") or []
        end = {}
        for r in rows:
            d = r.get("end")
            d = derive.day(d) if isinstance(d, str) else d
            if r.get("focus") and d is not None:
                end[r["focus"]] = d
        for i, e in enumerate(lint.list_of("diplomacy")):
            if not isinstance(e, dict) or e.get("action") not in ("declare", "declare_war"):
                continue
            atoms = [a for a in fm._atoms(e.get("when")) if isinstance(a, str)]
            dates = [derive.day(a.split(">=")[1].strip()) for a in atoms
                     if a.replace(" ", "").startswith("date>=")]
            dates = [d for d in dates if d is not None]
            if not dates:
                continue
            day = max(dates)
            for a in atoms:
                if not (a.startswith("focus_completed:") or a.startswith("focus_done:")):
                    continue
                f = a.split(":", 1)[1].strip()
                done = end.get(f)
                where = f"diplomacy[{i}] {e.get('id') or e.get('target')}"
                if done is None and end and day > max(end.values()):
                    continue  # past the detailed focus timeline: a later horizon schedules it
                if done is None:
                    lint.add("error", "war.declaration_gate", where,
                             f"the declaration on {e.get('target')} waits for {f}, which the focus timeline never "
                             "completes: the war never starts")
                elif done > day - GATE_SLACK:
                    lint.add("error", "war.declaration_gate", where,
                             f"the declaration on {e.get('target')} is dated {_fmt(day)} but waits for {f}, which the "
                             f"focus timeline completes {_fmt(done)}: the war starts on the focus, not on the plan's day. "
                             f"Schedule {f} to finish at least {GATE_SLACK} days before (move it earlier in "
                             "focus_path, or a shorter route to it)")

    def attack_force(self):
        """war.attack_force: on an offensive war's first day the layout places the forecast army, the formations
        on the war's axes carry at least ATTACK_RATIO x the enemy front estimate, and every armour or motorised
        formation is among them."""
        lint, needs = self.lint, getattr(self, "needs", None) or []
        rows = self.tl.forecast()["rows"]
        prof = {}

        def mobile(e):
            for t in e.get("templates") or []:
                if isinstance(t, str):
                    if t not in prof:
                        prof[t] = fm.template_profile(lint.game, self.tl.template_units(t))["kind"]
                    if fm.is_mobile(prof[t]):
                        return True
            return False
        for w in self.wars:
            if w["kind"] != "offensive" or "at war at the start" in w["source"]:
                continue
            day = w["start"]
            live = {}
            for e in self.layout():
                when = e.get("when")
                d = fm.predicate_day(lint, self.tl, when, self.wars) if when is not None else None
                if when is not None and (d is None or d > day):
                    continue
                key = (e.get("task_of"), e.get("name_hint") or e.get("id"))
                if key not in live or (d or 0) >= live[key][0]:
                    live[key] = (d or 0, e)
            entries = [e for _, e in live.values()]
            pid, eff = self.war_at(day)
            reserve = set(eff.get("reserve") or [])
            targets = set(w["targets"])
            attack = [e for e in entries if e.get("role") == "strike" and not e.get("task_of")
                      and (e.get("name_hint") or e.get("id")) not in reserve
                      and any(self.owner0.get(x) in targets for x in _ints(e.get("axis")))]
            placed = sum(e["count"] for e in entries if isinstance(e.get("count"), int))
            on_axes = sum(e["count"] for e in attack if isinstance(e.get("count"), int))
            fielded = next((n for d, n, _ in reversed(rows) if d <= day), None)
            need = next((x for x in needs if x.get("war") == w["id"]), {})
            enemy = (need.get("front") or {}).get("divisions_est") or (need.get("front") or {}).get("divisions") or 0
            fdw = next((x for x in (lint.plan.get("force_design") or {}).get("wars") or []
                        if isinstance(x, dict) and set(x.get("enemies") or []) & targets), {})
            ratio = fdw.get("attack_ratio") if isinstance(fdw.get("attack_ratio"), (int, float)) else ATTACK_RATIO
            where = f"army_layout ({w['id']})"
            problems = []
            if fielded and placed < PLACED_SHARE * fielded:
                problems.append(f"the layout in force places {placed} divisions of the {round(fielded)} the forecast "
                                "fields: add war-day entries (when: a date before the war) with each formation's count")
            if enemy and on_axes < ratio * enemy:
                problems.append(f"the formations on the war's axes ({', '.join(e.get('name_hint') or e.get('id') for e in attack) or 'none'}) "
                                f"carry {on_axes} divisions against about {round(enemy)} enemy divisions on the front "
                                f"(ratio {ratio:g} wanted: force_design.wars[].attack_ratio, default {ATTACK_RATIO:g})")
            idle = [e.get("name_hint") or e.get("id") for e in entries
                    if not e.get("task_of") and mobile(e) and e not in attack]
            if idle:
                problems.append(f"armour or motorised formations not on the war's axes: {', '.join(idle)} (the mobile "
                                "arm belongs on the operation's main axis)")
            if problems:
                lint.add("error", "war.attack_force", where,
                         f"{w['id']} ({_fmt(day)}): " + "; ".join(problems))


def check(ctx):
    s = Shape(ctx.lint, ctx.tl, ctx.wars)
    s.needs = ctx.needs
    s.declaration_gates()
    s.attack_force()
    s.recovery()
    s.guarantees()
    s.axes()
    s.objectives()
    s.holds()
    s.mobile()
