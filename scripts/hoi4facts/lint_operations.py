"""Operations: a large land war as the plan writes it (plan key `operations`), run by lint_war_shape.check.

An operation is one army-group front on the enemy border, one group offensive whose target line spans the enemy
country, the armour in an army of its own, a hold front on every other hostile border, and the war-day placement
(which army holds how many divisions of which template).

  war.operation_required  an offensive war over land (at least LAND_SHARE of a target's victory points reached from
                          our land or a friend's) against more than LARGE_WAR enemy divisions, with
                          more than that in the forecast, and no operation: per-army orders do not carry a war of
                          that size.
  war.operation_refs      an operation names a war, group, army, template or next operation the plan does not
                          have; a line formation outside the group or with orders of its own; a mobile or wing
                          formation without its own front and offensive; a hold on the operation's own group; more
                          armies in a group or divisions in an army than the game's caps; an army placed twice.
  war.operation_timing    execute_on is not the war's first day; prepare_from is less than PREPARE_DAYS before it
                          (the planning bonus is not full), or the group forms after it.
  war.operation_line      a front state that is not ours or a friend's; an axis or target-line state outside the
                          enemies' land; a target line that is not a chain of neighbouring states, that can be
                          walked around (it does not span the enemy's land), or that, with the follow-on phase
                          lines, leaves an enemy of the operation above its surrender threshold.
  war.operation_support   air.regions miss a strategic region of the target line; no air_plan row flies air
                          superiority (and close air support when asked) over them from the war day.
Several operations may fight one war (one per army group, each with its own `war` id the same): the lines, the
surrender, the placement, the attack ratio, the armour and the open borders are judged over all of them, and
war.operation_refs refuses two operations of a war on one group, an army in two of them, or a hold on a sibling's
attacking group.
The war-day rules of lint_war_shape read the operation's placement instead of army_layout for a war that has one:
war.attack_force (placement against the forecast by template, the attack against the enemy estimate, the armour in
the mobile formations), war.hold_sector (every other hostile border in a hold with enough divisions a border
province) and war.objective_not_at_war (an enemy of the operation that is not at war that day).
"""
from . import derive
from . import force as fm

LARGE_WAR = 30      # divisions: above this a land war is fought as an operation [D input budget, war beds]
LAND_SHARE = 0.5    # share of a target's victory points reached over land that makes its war a land war [H]
PREPARE_DAYS = 30   # days of drawn plans before the war day: the planning bonus reached its cap in ~25 days [bed]
PLACED_SHARE = 0.8  # the placement names at least this share of the forecast army [H]
HOLD_PER_PROVINCE = 2.0  # divisions a border province of a hold, at least [H; the game's AI held France with 3.75]
RULES = ("war.operation_required", "war.operation_refs", "war.operation_timing", "war.operation_line",
         "war.operation_support")


def _ints(xs):
    return [x for x in xs or [] if isinstance(x, int) and not isinstance(x, bool)]


def _fmt(d):
    return derive.fmt(derive.date_of(d)) if d is not None else "undated"


def _divs(e):
    d = e.get("divisions") if isinstance(e, dict) else None
    return {t: n for t, n in d.items() if isinstance(n, int)} if isinstance(d, dict) else {}


def operations(plan):
    v = plan.get("operations")
    return [o for o in v if isinstance(o, dict)] if isinstance(v, list) else []


def owned_armies(plan):
    """Armies an operation places (formations, holds, reserve): no layout axis or victory-point row steers them."""
    out = set()
    for o in operations(plan):
        for f in o.get("formations") or []:
            out.add(f.get("army"))
        for h in o.get("holds") or []:
            out |= {a.get("army") for a in h.get("armies") or [] if isinstance(a, dict)}
        out |= {r.get("army") for r in o.get("reserve") or [] if isinstance(r, dict)}
    return out - {None}


class Ops:
    def __init__(self, shape):
        self.s, self.lint, self.tl = shape, shape.lint, shape.tl
        self.ops = operations(self.lint.plan)
        self._nb = self._sf = self._dist = None
        self.facts = []

    # ------------------------------------------------------------------ helpers
    def nb(self):
        if self._nb is None:
            from .lint_conditions import _state_neighbours
            self._nb = _state_neighbours(self.lint)
        return self._nb

    def vps(self):
        if self._sf is None:
            sf = self.lint.game.state_facts(self.lint.ctx, self.lint.start)
            self._sf = {sid: sum((r.get("vps") or {}).values()) for sid, r in sf.items()}
        return self._sf

    def of_war(self, w):
        """The operation that fights a war: the first whose enemies include one of its targets."""
        for o in self.ops:
            if set(o.get("enemies") or []) & set(w["targets"]):
                return o
        return None

    def war_of(self, o):
        """The war an operation fights: the war its `war` names, else the first whose targets it attacks."""
        for w in self.s.wars:
            if o.get("war") is not None and w.get("id") == o.get("war"):
                return w
        for w in self.s.wars:
            if set(o.get("enemies") or []) & set(w["targets"]):
                return w
        return None

    def kind(self, template):
        return fm.template_profile(self.lint.game, self.tl.template_units(template))["kind"]

    def forecast(self, day):
        bt = self.tl.forecast()["by_template"]
        if not bt:
            return {}
        i = max(0, min(len(bt) - 1, (day - self.tl.start) // 7))
        return {t: n for t, n in bt[i].items() if n > 0.5}

    def land(self, o, day):
        """Enemy land of an operation on a day: states the enemies owned at the start that are not ours by then,
        and the land of countries gone by then that is not ours either (it may have passed to the enemy)."""
        mine = self.s.taken(day)
        gone = self.s.gone(day)
        tags = set(o.get("enemies") or [])
        return {sid for sid, ow in self.s.owner0.items()
                if sid not in mine and (ow in tags or (ow in gone and ow != self.lint.tag))}

    def reach(self, starts, land):
        """{state: steps} from the front's own states into the enemy land (1 = borders the front)."""
        nb = self.nb()
        dist, edge = {}, []
        for s in starts:
            for n in nb.get(s, ()):
                if n in land and n not in dist:
                    dist[n] = 1
                    edge.append(n)
        while edge:
            nxt = []
            for s in edge:
                for n in nb.get(s, ()):
                    if n in land and n not in dist:
                        dist[n] = dist[s] + 1
                        nxt.append(n)
            edge = nxt
        return dist

    def components(self, states):
        nb, left, out = self.nb(), set(states), []
        while left:
            seed = left.pop()
            comp, edge = {seed}, [seed]
            while edge:
                s = edge.pop()
                for n in nb.get(s, ()):
                    if n in left:
                        left.discard(n)
                        comp.add(n)
                        edge.append(n)
            out.append(comp)
        return out

    def border_provinces(self, states, facing, mine):
        """Provinces of own `states` that touch a province of a `facing` country's land not ours."""
        lint = self.lint
        adj = lint.adjacency(straits=False)
        ps, st = self.s.pstate, lint.states()
        out = set()
        for s in states:
            for p in (st.get(s) or {}).get("provinces") or []:
                for q in adj.get(p, ()):
                    t = ps.get(q)
                    if t is not None and t not in mine and self.s.owner0.get(t) in facing:
                        out.add(p)
                        break
        return out

    def land_targets(self, w):
        """Targets of a war fought over land: at least LAND_SHARE of the target's victory points lie on land
        reached from ours or a friend's on the war day (a colony beside an ally's land does not make the war with
        its overseas owner a land war)."""
        day = w["start"]
        mine = self.s.taken(day)
        friends = (self.lint.passable_tags() or set()) - set(w["enemy_tags"])
        ground = mine | {s for s, ow in self.s.owner0.items() if ow in friends}
        vps, out = self.vps(), []
        for t in w["targets"]:
            land = {s for s, ow in self.s.owner0.items() if ow == t and s not in mine}
            total = sum(vps.get(s, 0) for s in land)
            if total and sum(vps.get(s, 0) for s in self.reach(ground, land)) >= LAND_SHARE * total:
                out.append(t)
        return sorted(out)

    def landed(self, w):
        """Whether the plan lands in a target's land (an `invasions` entry into one of its states)."""
        targets = set(w["targets"])
        return any(self.s.owner0.get(i.get("target_state")) in targets for i in self.lint.list_of("invasions"))

    # ------------------------------------------------------------------ rules
    def required(self):
        for w in self.s.wars:
            if w["kind"] != "offensive" or "at war at the start" in w["source"] or self.of_war(w):
                continue
            day = w["start"]
            fielded = sum(self.forecast(day).values())
            need = next((x for x in self.s.needs or [] if x.get("war") == w["id"]), {})
            enemy = (need.get("front") or {}).get("divisions_est") or (need.get("front") or {}).get("divisions") or 0
            if enemy <= LARGE_WAR or fielded <= LARGE_WAR:
                continue  # a small theatre: the strike keys carry it
            touch = self.land_targets(w)
            if not touch:
                continue  # reached by sea: landings (invasions) and the strike keys carry it
            self.lint.add("error", "war.operation_required", f"operations ({w['id']})",
                          f"{w['id']} on {', '.join(w['targets'])} starts {_fmt(day)} over a land border "
                          f"({', '.join(touch)}) against about {round(enemy)} enemy divisions, with {round(fielded)} "
                          f"in the forecast, and no operation: above {LARGE_WAR} divisions a side the war is one army-group front, one group offensive with a "
                          "country-wide target line, the armour in its own army and a hold on every other hostile "
                          "border, with the war-day placement by template (operations[])")

    def refs(self, o, where):
        lint, plan = self.lint, self.lint.plan
        err = lambda m: lint.add("error", "war.operation_refs", where, m)  # noqa: E731
        fd = plan.get("force_design") if isinstance(plan.get("force_design"), dict) else {}
        if o.get("war") not in {x.get("id") for x in fd.get("wars") or [] if isinstance(x, dict)}:
            err(f"war {o.get('war')!r} is no force_design.wars id")
        groups = {g.get("id"): g for g in lint.list_of("army_group")}
        layout = {e.get("name_hint"): e for e in lint.list_of("army_layout") if not e.get("task_of")}
        templates = set(self.tl.forecast()["by_template"][-1]) if self.tl.forecast()["by_template"] else set()
        templates |= {t.get("name") for t in lint.list_of("templates")}
        g = groups.get(o.get("group"))
        if g is None:
            err(f"group {o.get('group')!r} is no army_group id")
        members = set((g or {}).get("armies") or [])
        cap_a = int(derive.define(lint.game, "CORPS_COMMANDER_DIVISIONS_CAP", 24))
        cap_g = int(derive.define(lint.game, "FIELD_MARSHAL_ARMIES_CAP", 5))
        seen = {}
        rows = [("formations", f) for f in o.get("formations") or []]
        rows += [(f"holds {h.get('id')}", a) for h in o.get("holds") or [] for a in h.get("armies") or []]
        rows += [("reserve", r) for r in o.get("reserve") or []]
        for part, f in rows:
            a = f.get("army")
            if a in seen:
                err(f"army {a!r} is placed twice ({seen[a]} and {part}): an army is in one place on the war day")
            seen[a] = part
            if a not in layout:
                err(f"{part}: army {a!r} is no army_layout formation (name_hint), so nothing creates it")
            elif not (layout[a].get("general") or f.get("commander")):
                lint.add("warning", "war.operation_refs", where, f"{part}: army {a!r} has no general "
                         "(army_layout.general or the formation's commander)")
            n = sum(_divs(f).values())
            if n > cap_a:
                err(f"{part}: army {a!r} holds {n} divisions, over the {cap_a} a general commands")
            bad = sorted(t for t in _divs(f) if t not in templates)
            if bad:
                err(f"{part}: army {a!r} places templates the plan does not have: {', '.join(bad)}")
        if g is not None and not g.get("marshal"):
            lint.add("warning", "war.operation_refs", where, f"group {o.get('group')} has no marshal")
        line = [f for f in o.get("formations") or [] if f.get("role") == "line"]
        if not line:
            err("no line formation: the group front and the group offensive need the group's armies")
        if len(line) > cap_g:
            err(f"{len(line)} line armies in one group, over the {cap_g} a field marshal commands")
        for f in o.get("formations") or []:
            a, role = f.get("army"), f.get("role")
            own = "front" in f or "offensive" in f
            if role == "line":
                if g is not None and a not in members:
                    err(f"line formation {a!r} is not in army_group {o.get('group')}: it would not follow the group front")
                if own:
                    err(f"line formation {a!r} has a front or offensive of its own: an army with its own orders "
                        "ignores the group's (role mobile or wing carries own orders)")
            elif not ("front" in f and "offensive" in f):
                err(f"{role} formation {a!r} needs its own front and offensive")
            if role == "mobile":
                slow = sorted(t for t in _divs(f) if not fm.is_mobile(self.kind(t)))
                if slow:
                    err(f"mobile formation {a!r} carries {', '.join(slow)}: the mobile army is armour and motorised only")
        for h in o.get("holds") or []:
            if h.get("group") is not None:
                if h["group"] not in groups:
                    err(f"holds {h.get('id')}: group {h['group']!r} is no army_group id")
                elif h["group"] == o.get("group"):
                    err(f"holds {h.get('id')} uses the operation's own group {h['group']}: a group has one front, and "
                        "executing it would attack from the hold")
                else:
                    out = sorted(a.get("army") for a in h.get("armies") or []
                                 if a.get("army") not in set(groups[h["group"]].get("armies") or []))
                    if out:
                        err(f"holds {h.get('id')}: {', '.join(map(str, out))} not in army_group {h['group']}")
        nxt = (o.get("on_capitulation") or {}).get("next")
        if nxt is not None and nxt not in {x.get("id") for x in self.ops}:
            err(f"on_capitulation.next {nxt!r} is no operation id")
        ids = [x.get("id") for x in self.ops]
        if ids.count(o.get("id")) > 1:
            err(f"operation id {o.get('id')!r} is used twice")

    def timing(self, o, w, where):
        lint = self.lint
        day = w["start"]
        ex = fm.predicate_day(lint, self.tl, o.get("execute_on"), self.s.wars)
        if ex is None or ex != day:
            lint.add("error", "war.operation_timing", where,
                     f"execute_on dates to {_fmt(ex)}, but {w['id']} starts {_fmt(day)}: the plans execute on the "
                     "declaration (at_war_with:<target>)")
        pr = fm.predicate_day(lint, self.tl, o.get("prepare_from"), self.s.wars)
        if pr is None or pr > day - PREPARE_DAYS:
            lint.add("error", "war.operation_timing", where,
                     f"prepare_from dates to {_fmt(pr)}, less than {PREPARE_DAYS} days before the war day {_fmt(day)}: "
                     "the fronts and lines are drawn in peace early enough for the full planning bonus")
        for g in lint.list_of("army_group"):
            used = g.get("id") == o.get("group") or any(h.get("group") == g.get("id") for h in o.get("holds") or [])
            if not used or g.get("when") is None:
                continue
            gd = fm.predicate_day(lint, self.tl, g["when"], self.s.wars)
            if gd is not None and pr is not None and gd > pr:
                lint.add("error", "war.operation_timing", where,
                         f"army_group {g.get('id')} forms {_fmt(gd)}, after prepare_from {_fmt(pr)}")

    def line(self, w, group):
        """war.operation_line, and the facts of what the lines take: per war, over all its operations (several
        army groups each draw their own sector; together the lines span the enemy land and take its surrender)."""
        lint = self.lint
        day = w["start"]
        drawn_all = [self.draw(o, w, where) for o, where in group]
        cut = set()
        for d in drawn_all:
            for _, _, ln in d["drawn"]:
                cut |= set(ln)
        covered = set(cut)
        for d in drawn_all:
            for label, front, ln in d["drawn"]:
                covered |= d["sweep"](f"{label} line", front, ln, cut)
        first = set(covered)
        for (o, where), d in zip(group, drawn_all):
            for i, ph in enumerate(o.get("phase_lines") or []):
                if not isinstance(ph, dict):
                    continue
                pl = d["check_line"](_ints(ph.get("target_line")), f"phase_lines[{i}] {ph.get('id')}")
                if ph.get("axis"):
                    d["check_line"](_ints(ph["axis"])[1:], f"phase_lines[{i}] axis", chain=False)
                covered |= set(pl)
                covered |= d["sweep"](f"phase_lines[{i}] {ph.get('id')}", d["gfront"], pl, set(pl))
        # what the lines take against each enemy's surrender threshold
        vps = self.vps()
        ids = [o.get("id") for o, _ in group]
        facts = [{"id": o.get("id"), "war": w["id"], "date": _fmt(day), "surrender": {}} for o, _ in group]
        if len(group) > 1:
            for f in facts:
                f["with"] = [x for x in ids if x != f["id"]]
        done = set()
        for o, where in group:
            for t in o.get("enemies") or []:
                if t in done:
                    continue
                done.add(t)
                own = {s for s, ow in self.s.owner0.items() if ow == t}
                if not sum(vps.get(s, 0) for s in own):
                    continue
                try:
                    sur = derive.surrender(lint.game, t, lint.ctx, lint.start)
                except Exception:  # noqa: BLE001  (a tag with no history)
                    continue
                need, total = sur["vp_needed"], sur["total_vp"]
                have1 = sum(vps.get(s, 0) for s in own & first)
                have = sum(vps.get(s, 0) for s in own & covered)
                row = {"total_vp": total, "vp_needed": need, "first_line": have1, "all_lines": have}
                for f, (oo, _) in zip(facts, group):
                    if t in (oo.get("enemies") or []):
                        f["surrender"][t] = row
                if have < need:
                    phases = any(oo.get("phase_lines") for oo, _ in group)
                    if len(group) > 1:
                        what = f"the target lines of {', '.join(map(str, ids))}"
                        verb = "take"
                    else:
                        what, verb = "the target line", "takes"
                    lint.add("error", "war.operation_line", where,
                             f"{what}{' and the phase lines' if phases else ''} {verb} {have:g} of {t}'s {total:g} "
                             f"victory points (in and in front of the lines); it capitulates at {need:g}. Carry the "
                             "line, or a follow-on phase line, deep enough to end the war")
        self.facts.extend(facts)
        return facts

    def draw(self, o, w, where):
        """One operation's drawn fronts and lines, checked; with the sweep and the line check the war applies over
        the union of its operations' lines."""
        lint, nb = self.lint, self.nb()
        day = w["start"]
        err = lambda m: lint.add("error", "war.operation_line", where, m)  # noqa: E731
        mine = self.s.taken(day)
        land = self.land(o, day)
        hostile = self.s.hostile(day) | set(w["enemy_tags"])
        friends = ((lint.passable_tags() or set()) | {lint.tag}) - hostile
        ok_front = mine | {s for s, ow in self.s.owner0.items() if ow in friends}

        def check_front(fr, label):
            bad = [s for s in _ints((fr or {}).get("own_states")) if s not in ok_front]
            if bad:
                err(f"{label} front: states {bad} are not ours or a friend's on {_fmt(day)}")
            none = [s for s in _ints((fr or {}).get("own_states")) if s in ok_front
                    and not any(n in land for n in nb.get(s, ()))]
            if none:
                err(f"{label} front: states {none} do not border the enemies' land")
            return [s for s in _ints((fr or {}).get("own_states")) if s in ok_front]

        def check_line(states, label, chain=True):
            out = [s for s in states if s not in land]
            if out:
                err(f"{label}: states " + ", ".join(f"{s} ({self.s.owner0.get(s)})" for s in out)
                    + f" are not land of {', '.join(o.get('enemies') or [])} on {_fmt(day)}")
            if chain:
                gaps = [f"{a}-{b}" for a, b in zip(states, states[1:]) if b not in nb.get(a, ())]
                if gaps:
                    err(f"{label}: not a chain of neighbouring states (no border between {', '.join(gaps)}); list "
                        "the line's states side by side, in order")
            return [s for s in states if s in land]

        def check_axis(ax, starts, label):
            if not ax:
                return
            if ax[0] not in starts:
                err(f"{label} axis starts in {ax[0]}, which is not a state of its front")
            check_line(ax[1:], f"{label} axis", chain=False)
            gaps = [f"{a}-{b}" for a, b in zip(ax, ax[1:]) if b not in nb.get(a, ())]
            if gaps:
                err(f"{label} axis: no border between {', '.join(gaps)}")

        # every drawn thing of the first phase
        gfront = check_front(o.get("front"), "group")
        off = o.get("offensive") if isinstance(o.get("offensive"), dict) else {}
        gline = check_line(_ints(off.get("target_line")), "offensive.target_line")
        check_axis(_ints(off.get("axis")), gfront, "group")
        drawn = [("group", gfront, gline)]
        for f in o.get("formations") or []:
            if f.get("role") not in ("mobile", "wing"):
                continue
            ffront = check_front(f.get("front"), f"{f.get('army')}")
            fo = f.get("offensive") if isinstance(f.get("offensive"), dict) else {}
            fl = check_line(_ints(fo.get("target_line")), f"{f.get('army')} target_line", chain=f.get("role") == "wing")
            check_axis(_ints(fo.get("axis")), ffront, f"{f.get('army')}")
            if f.get("role") == "wing":
                drawn.append((f.get("army"), ffront, fl))
        def sweep(label, front, ln, cut):
            """States in front of a line: the parts of the enemy land, cut by the lines, that touch the front, as
            deep as the line. When such a part reaches deeper than the line and the lines cut nothing off (no
            part of the land lies behind them), the line can be walked around: it does not span."""
            if not front or not ln:
                return set()
            dist = self.reach(front, land)
            lost = [s for s in ln if s not in dist]
            if lost:
                err(f"{label}: line states {lost} cannot be reached from the front over the enemies' land")
                return set()
            deep = max(dist[s] for s in ln)
            got = set()
            comps = self.components(land - cut)
            near = [c for c in comps if any(n in front for s in c for n in nb.get(s, ()))]
            behind = len(comps) > len(near)
            for comp in near:
                far = sorted(s for s in comp if dist.get(s, 0) > deep)
                if far and not behind:
                    err(f"{label}: the target line does not span the enemies' land: states "
                        f"{', '.join(map(str, far[:8]))}{', ...' if len(far) > 8 else ''} lie beyond it and are "
                        "reached around its ends. Run the line from one edge of the country to the other")
                else:
                    got |= {s for s in comp if dist.get(s, 0) <= deep}
            return got
        return {"drawn": drawn, "gfront": gfront, "sweep": sweep, "check_line": check_line}

    def at_war(self, o, w, where):
        day = w["start"]
        hostile = self.s.hostile(day) | set(w["enemy_tags"])
        cold = sorted(t for t in o.get("enemies") or [] if t not in hostile)
        if cold:
            self.lint.add("error", "war.objective_not_at_war", where,
                          f"enemies {', '.join(cold)} are not at war with the plan on {_fmt(day)} ({w['id']}): an operation "
                          "attacks only countries its war day brings in (a declaration, a joiner, a guarantor)")

    def force(self, w, group, facts):
        """war.attack_force for a war with operations: the placement of all its operations against the forecast,
        their attack together against the enemy, the armour in their mobile formations."""
        lint, day = self.lint, w["start"]
        where = group[0][1] if len(group) == 1 else "operations " + ", ".join(str(o.get("id")) for o, _ in group)
        fc = self.forecast(day)
        fielded = sum(fc.values())
        attack, holds, res = [], [], []
        for o, _ in group:
            attack += [f for f in o.get("formations") or [] if isinstance(f, dict)]
            holds += [a for h in o.get("holds") or [] for a in h.get("armies") or [] if isinstance(a, dict)]
            res += [r for r in o.get("reserve") or [] if isinstance(r, dict)]
        placed = {}
        for f in attack + holds + res:
            for t, n in _divs(f).items():
                placed[t] = placed.get(t, 0) + n
        problems = []
        over = sorted(f"{t} {n} of {round(fc.get(t, 0))}" for t, n in placed.items() if n > fc.get(t, 0) + 0.5)
        if over:
            problems.append("the placement holds more divisions than the forecast fields on that day: " + ", ".join(over)
                            + " (train them by then, or place what exists)")
        total = sum(placed.values())
        if fielded and total < PLACED_SHARE * fielded:
            problems.append(f"the placement names {total} divisions of the {round(fielded)} the forecast fields: "
                            "every formation stands on an operation, a hold or the reserve (a war larger than one "
                            "group's armies is fought by several operations of the same war, one per army group)")
        on = sum(sum(_divs(f).values()) for f in attack)
        need = next((x for x in self.s.needs or [] if x.get("war") == w["id"]), {})
        enemy = (need.get("front") or {}).get("divisions_est") or (need.get("front") or {}).get("divisions") or 0
        wars_fd = [x for x in (lint.plan.get("force_design") or {}).get("wars") or [] if isinstance(x, dict)]
        fdw = next((x for x in wars_fd if x.get("id") == group[0][0].get("war")), {})
        from .lint_war_shape import ATTACK_RATIO
        ratio = fdw.get("attack_ratio") if isinstance(fdw.get("attack_ratio"), (int, float)) else ATTACK_RATIO
        if enemy and on < ratio * enemy:
            who = "the operations attack" if len(group) > 1 else "the operation attacks"
            problems.append(f"{who} with {on} divisions against about {round(enemy)} enemy divisions on the front "
                            f"(ratio {ratio:g} wanted: force_design.wars[].attack_ratio)")
        mob_fc = sum(n for t, n in fc.items() if fm.is_mobile(self.kind(t)))
        mob_in = sum(n for f in attack if f.get("role") == "mobile" for t, n in _divs(f).items())
        stray = sorted({f.get("army") for f in attack + holds + res if f.get("role") != "mobile"
                        for t in _divs(f) if fm.is_mobile(self.kind(t))})
        if stray:
            problems.append(f"armour or motorised divisions outside the mobile formations ({', '.join(stray)}): the "
                            "mobile arm attacks on the main axis in an army of its own")
        if mob_fc >= 1 and mob_in < PLACED_SHARE * mob_fc:
            problems.append(f"the mobile formations carry {mob_in} of the {round(mob_fc)} armoured and motorised "
                            "divisions the forecast fields")
        for (o, _), fact in zip(group, facts):
            own = sum(sum(_divs(f).values()) for f in o.get("formations") or [] if isinstance(f, dict))
            fact.update(attack=own, enemy_est=round(enemy), placed=total, forecast=round(fielded), mobile=mob_in)
            if len(group) > 1:
                fact["war_attack"] = on
        if problems:
            lint.add("error", "war.attack_force", where, f"{w['id']} ({_fmt(day)}): " + "; ".join(problems))

    def holds(self, w, group, facts):
        """war.hold_sector for a war with operations: every hold checked, the open borders over all of them."""
        day = w["start"]
        mine = self.s.taken(day)
        hostile = (self.s.hostile(day) | self.s.likely(w)) - self.s.gone(day)
        attacked, fronts, cover = set(), set(), {}
        for o, _ in group:
            attacked |= set(o.get("enemies") or [])
            fronts |= set(_ints((o.get("front") or {}).get("own_states")))
            for f in o.get("formations") or []:
                fronts |= set(_ints((f.get("front") or {}).get("own_states")))
        for (o, where), fact in zip(group, facts):
            self.hold_rows(o, day, where, fact, mine, cover)
        where = group[0][1] if len(group) == 1 else "operations " + ", ".join(str(o.get("id")) for o, _ in group)
        self.open_borders(w, where, mine, hostile, attacked, fronts, cover)

    def hold_rows(self, o, day, where, fact, mine, cover):
        lint = self.lint
        fact["holds"] = []
        for h in o.get("holds") or []:
            if not isinstance(h, dict):
                continue
            facing = {t for t in h.get("facing") or [] if isinstance(t, str)}
            states = _ints(h.get("states"))
            not_mine = [s for s in states if s not in mine]
            if not_mine:
                lint.add("error", "war.hold_sector", where, f"holds {h.get('id')}: states {not_mine} are not ours on {_fmt(day)}")
            provs = self.border_provinces([s for s in states if s in mine], facing, mine)
            n = sum(sum(_divs(a).values()) for a in h.get("armies") or [] if isinstance(a, dict))
            per = n / len(provs) if provs else None
            want = max(HOLD_PER_PROVINCE, h.get("min_per_province") or 0)
            fact["holds"].append({"id": h.get("id"), "facing": sorted(facing), "divisions": n,
                                  "border_provinces": len(provs), "per_province": round(per, 2) if per else None})
            if isinstance(h.get("min_per_province"), (int, float)) and h["min_per_province"] < HOLD_PER_PROVINCE:
                lint.add("error", "war.hold_sector", where, f"holds {h.get('id')}: min_per_province "
                         f"{h['min_per_province']:g} is under {HOLD_PER_PROVINCE:g}, the least that holds a border province")
            if not provs:
                lint.add("error", "war.hold_sector", where, f"holds {h.get('id')}: states {states} have no border "
                         f"province facing {', '.join(sorted(facing))}")
            elif per < want:
                lint.add("error", "war.hold_sector", where,
                         f"holds {h.get('id')}: {n} divisions on {len(provs)} border provinces facing "
                         f"{', '.join(sorted(facing))} = {per:.1f} a province, under {want:g}")
            for s in states:
                cover.setdefault(s, set()).update(facing)

    def open_borders(self, w, where, mine, hostile, attacked, fronts, cover):
        nb, day = self.nb(), w["start"]
        lint = self.lint
        open_ = {}
        for s in mine:
            if s not in self.tl.owned(day):
                continue
            for n in nb.get(s, ()):
                t = self.s.owner0.get(n)
                if n in mine or t not in hostile:
                    continue
                if t in attacked and s in fronts:
                    continue
                if t in cover.get(s, ()):
                    continue
                open_.setdefault(s, set()).add(t)
        if open_:
            lint.add("error", "war.hold_sector", where,
                     f"{w['id']} ({_fmt(day)}): own border states in no hold and on no front of the operation: "
                     + ", ".join(f"{s} (facing {', '.join(sorted(ts))})" for s, ts in sorted(open_.items()))
                     + ". Every border with a country at war or likely to join has a named hold with its divisions")

    def support(self, o, w, where):
        lint, day = self.lint, w["start"]
        air = o.get("air") if isinstance(o.get("air"), dict) else {}
        regions = set(_ints(air.get("regions")))
        reg = {}
        for rid, r in lint.world.strategic_regions().items():
            for p in r.get("provinces") or []:
                reg[p] = rid
        st = lint.states()
        need = set()
        for s in _ints((o.get("offensive") or {}).get("target_line")) + _ints((o.get("offensive") or {}).get("axis"))[1:]:
            need |= {reg[p] for p in (st.get(s) or {}).get("provinces") or [] if p in reg}
        miss = sorted(need - regions)
        if miss:
            lint.add("error", "war.operation_support", where,
                     f"air.regions miss strategic regions {miss} of the target line and the axis: the air war is flown "
                     "over the operation")
        for mission, wanted in (("air_superiority", air.get("superiority")), ("close_air_support", air.get("cas"))):
            if not wanted:
                continue
            hit = False
            for e in lint.list_of("air_plan"):
                if e.get("mission") not in (mission, "cas" if mission == "close_air_support" else mission):
                    continue
                if e.get("region") not in regions and e.get("then_region") not in regions:
                    continue
                d = fm.predicate_day(lint, self.tl, e.get("when"), self.s.wars) if e.get("when") is not None else None
                if d is None or d <= day:
                    hit = True
                    break
            if not hit:
                lint.add("error", "war.operation_support", where,
                         f"no air_plan row flies {mission} over regions {sorted(regions)} by {_fmt(day)}")

    def peers(self, w, group):
        """Several operations of one war: each its own army group, each army in one place."""
        if len(group) < 2:
            return
        lint = self.lint
        seen_g, seen_a = {}, {}
        for o, where in group:
            g = o.get("group")
            if g in seen_g:
                lint.add("error", "war.operation_refs", where,
                         f"group {g!r} is also the group of {seen_g[g]} in {w['id']}: an army group has one front and "
                         "one offensive, so each operation of a war has its own group")
            seen_g.setdefault(g, o.get("id"))
            rows = [f for f in o.get("formations") or [] if isinstance(f, dict)]
            rows += [a for h in o.get("holds") or [] if isinstance(h, dict)
                     for a in h.get("armies") or [] if isinstance(a, dict)]
            rows += [r for r in o.get("reserve") or [] if isinstance(r, dict)]
            for f in rows:
                a = f.get("army")
                if a in seen_a and seen_a[a] != o.get("id"):
                    lint.add("error", "war.operation_refs", where,
                             f"army {a!r} is placed in {seen_a[a]} and {o.get('id')}: an army is in one place on the "
                             "war day")
                seen_a.setdefault(a, o.get("id"))
        mains = {o.get("group"): o.get("id") for o, _ in group}
        for o, where in group:
            for h in o.get("holds") or []:
                if isinstance(h, dict) and h.get("group") in mains and mains[h["group"]] != o.get("id"):
                    lint.add("error", "war.operation_refs", where,
                             f"holds {h.get('id')} uses group {h['group']}, the attacking group of "
                             f"{mains[h['group']]}")

    def run(self):
        self.required()
        wars = {}
        for i, o in enumerate(self.ops):
            where = f"operations[{i}] {o.get('id')}"
            self.refs(o, where)
            w = self.war_of(o)
            if w is None or w.get("start") is None:
                self.lint.add("error", "war.operation_refs", where,
                              f"no war of the plan attacks {', '.join(map(str, o.get('enemies') or []))}: an operation "
                              "belongs to a declaration (diplomacy) or a force_design.wars entry with a start date")
                continue
            self.timing(o, w, where)
            self.at_war(o, w, where)
            self.support(o, w, where)
            wars.setdefault(w["id"], (w, []))[1].append((o, where))
        for w, group in wars.values():
            self.peers(w, group)
            facts = self.line(w, group)
            self.force(w, group, facts)
            self.holds(w, group, facts)
        if self.facts:
            self.lint.facts.setdefault("force", {})["operations"] = self.facts

def check(shape):
    ops = Ops(shape)
    ops.run()
    return ops
