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
"""
from . import derive
from . import force as fm

HOLD_MIN = 2   # divisions per border state: the executor's war-risk read wants at least 2 (borders_not_held)
RULES = ("war.axes_missing", "war.objective_not_at_war", "war.hold_sector", "war.mobile_axis")


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

    def likely(self, w):
        """Countries expected to join a war: every joiners_ok of the declarations on its first day and the
        phase's war.enemies on that day (guarantors and allies, whether or not they border the target)."""
        out = set(w["enemy_tags"])
        for e in self.lint.list_of("diplomacy"):
            if isinstance(e, dict) and e.get("action") in ("declare", "declare_war") and e.get("target") in w["targets"]:
                out |= {j for j in e.get("joiners_ok") or [] if isinstance(j, str)}
        out |= {t for t in self.war_at(w["start"])[1].get("enemies") or [] if isinstance(t, str)}
        return out - {self.lint.tag}

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


def check(ctx):
    s = Shape(ctx.lint, ctx.tl, ctx.wars)
    s.axes()
    s.objectives()
    s.holds()
    s.mobile()
