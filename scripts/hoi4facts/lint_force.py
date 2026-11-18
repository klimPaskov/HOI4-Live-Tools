"""Force design: the plan's army, air force and navy against what its own wars need (force.py), run by lint_plan.py
(Lint.run) after lint_economy.

The plan states its force in `force_design`: the wars with their tasks, a table of land rows (template, role,
divisions fielded by a ready date, the war and phase it serves), air rows (kind, equipment, planes by a date) and
navy rows, and the factory budget by date. The lint holds that table to the wars the plan dates and to the
plan's own economy (the army forecast of the executor's training, equipment counted, and the line output):

  force.required          (error) the plan dates a war and has no force_design, or a war with no
                          force_design.wars entry (matched by its targets).
  force.breakthrough      (error) an offensive war against a defended line whose first day finds fewer armoured
                          or mobile divisions in the army forecast than its strike axes need (force.py: armour per
                          axis, armoured or motorised per axis); the message names the mobile templates, the day
                          each can exist and what holds their training.
  force.single_template   (error while the plan plans an offensive war, else a warning that
                          force_design.single_template_why answers) one template is more than SINGLE_SHARE of the
                          divisions on a war's first day.
  force.air               (error) fighters on a war's first day (start planes plus the fighter lines' output) below
                          the fighters force.py requires: FIGHTER_RATIO x the front's start fighters, as far as
                          AIR_SHARE of the military output pays; an air row or air_plan entry that asks for more planes of a kind by its date than
                          the start wings and the lines give.
  force.air_support       (error) an offensive war against a defended line whose close air support (one wing an
                          axis, once the fighters are paid) the start wings and lines do not give by its first day.
  force.air_*             the air force as an arm (lint_force_air.py): use by phase, deployment, losses, bases,
                          research, designs, doctrine, command, air defence, fuel and resources.
  force.anti_tank         (warning) the enemy side fields tanks and no line template fielded in that war carries
                          anti-tank; force.anti_air (warning) likewise for anti-air where the plan cannot contest
                          the air.
  force.invasion          (error) a landing whose target owner's start fighters outnumber ours on its date
                          (x FIGHTER_RATIO), or whose surface fleet outweighs ours SEA_RATIO times while the plan
                          has no naval strike planes (a naval bomber line or start wings, and an air row or air_plan
                          entry on naval_strike or port_strike).
  force.sea_*, force.navy_*, force.carrier_air
                          the navy as an arm (lint_force_navy.py, navy.py): every sea operation the plan
                          schedules held to the sea control, screens, lift, landing techs, port, patrols and
                          tools it needs by its date; the building programme, designs, doctrine and admirals.
  force.unused_line       (error) a tank or plane line that no template (start or planned), start wing, air_plan
                          entry or force_design air row uses; and the reverse, a force_design row whose equipment no
                          line makes and no start stock holds.
  force.template_late     (error) a force_design land row whose template cannot exist (its `when`, the techs of its
                          units) plus its training time by the row's ready date or the start of its war.
  force.readiness         (error) a force_design row the plan's own arithmetic does not field: land rows against the
                          army forecast by template (90% of the count by the ready date), air rows against start
                          planes plus line output, navy rows against start ships plus dockyard output.
  force.bombers           (error) a war planned past LOGISTICS_DAYS whose tactical bombers (one wing an axis on
                          logistics strike, once fighters and CAS are paid) the start wings and lines do not give,
                          with no force_design.bomber_why; (warning) a war of a year or more with no strategic
                          bombers and no reason.
  force.design_cost       (error) a tank, plane or ship design a line builds with no modules: costed on the bare
                          chassis, so the table counts too many units.
  force.decisive          (error) an offensive war not planned to be won fast: no planned duration or reasons
                          (force_design.wars[].duration_days, decisive), the breakthrough force or the fighters short
                          on its first day, fewer divisions than the game's own AI fields then (a floor), or under
                          half the enemy's estimated divisions (force.py grown: the start OOB grown at the AI's
                          peace rate).
  force.budget            (error) no factory budget by date (force_design.budget); (warning) a budget row whose land
                          / air shares are more than BUDGET_POINTS off the plan's lines on that date.

The division count against the game's own AI stays in facts.ai_baseline (plan.ai_baseline, info): the floor is the
force table's readiness, not the count. facts.force: the wars, the needs per war (force.py), the army mix by war,
the table's readiness rows and what the lines make.
"""
from collections import Counter, defaultdict

from . import derive
from . import force as fm

READY_SLACK = 0.9
BUDGET_POINTS = 15


def _day(s):
    return derive.day(s) if s else None


def _fmt(d):
    return derive.fmt(derive.date_of(d)) if d is not None else "undated"


def check(lint):
    if not lint.tag or lint.world is None:
        return lint
    tl = getattr(lint, "_timeline", None)
    if tl is None:
        from . import conditions as cond
        tl = cond.Timeline(lint)
        lint._timeline = tl
    wars = fm.plan_wars(lint, tl)
    if not wars:
        return lint
    needs = fm.needs(lint, tl, wars)
    fd = lint.plan.get("force_design") if isinstance(lint.plan.get("force_design"), dict) else None
    ctx = Ctx(lint, tl, wars, needs, fd)
    ctx.facts()
    ctx.required()
    ctx.breakthrough()
    ctx.single_template()
    ctx.air()
    # the air force as an arm: use by phase, deployment, bases, research, designs, doctrine, fuel
    from . import lint_force_air
    lint_force_air.check(ctx)
    ctx.bombers()
    ctx.design_cost()
    ctx.support_companies()
    ctx.invasions()
    # the navy as an arm: the sea operations the plan schedules and what each needs (navy.py)
    from . import lint_force_navy
    lint_force_navy.check(ctx)
    ctx.unused_lines()
    ctx.table()
    ctx.budget()
    ctx.decisive()
    return lint


class Ctx:
    def __init__(self, lint, tl, wars, needs, fd):
        self.lint, self.tl, self.wars, self.needs, self.fd = lint, tl, wars, needs, fd
        self.g = lint.game
        self.fc = tl.forecast()
        self._profiles = {}
        self._air_out = None

    # ------------------------------------------------------------------ helpers
    def profile(self, name):
        if name not in self._profiles:
            self._profiles[name] = fm.template_profile(self.g, self.tl.template_units(name))
        return self._profiles[name]

    def army(self, day):
        """{template: divisions} the forecast fields on day."""
        bt = self.fc["by_template"]
        if not bt:
            return {}
        i = max(0, min(len(bt) - 1, (day - self.tl.start) // 7))
        return {t: n for t, n in bt[i].items() if n > 0.5}

    def need_of(self, war, role):
        n = next((x for x in self.needs if x["war"] == war["id"]), None)
        return next((r for r in (n or {}).get("rows") or [] if r["role"] == role), None)

    def fd_list(self, key):
        v = (self.fd or {}).get(key)
        return [x for x in v if isinstance(x, dict)] if isinstance(v, list) else []

    def offensive(self):
        return [w for w in self.wars if w["kind"] == "offensive"]

    # air: planes of a kind on a day = start wings + the lines' output to that day
    def air_have(self, kind, day):
        if self._air_out is None:
            self._air_out = []
            d = self.tl.start
            end = self.tl.forecast_end()
            total = Counter()
            carrier = Counter()
            self._cv_out = []
            from . import navy as navymod
            while d <= end:
                for k, units in self.lint.line_output(d).items():
                    pk = fm.plane_kind(self.g, k)
                    if pk:
                        total[pk] += units * 7
                        if navymod.carrier_plane(self.g, k):
                            carrier[pk] += units * 7
                self._air_out.append((d, Counter(total)))
                self._cv_out.append((d, Counter(carrier)))
                d += 7
        start = Counter()
        for a in self.lint.world.oob(self.lint.tag)["air"]:
            start[fm.plane_kind(self.g, a["type"]) or "other"] += int(a.get("amount") or 0)
        made = 0.0
        for d, c in self._air_out:
            if d > day:
                break
            made = c.get(kind, 0.0)
        return start.get(kind, 0) + made, start.get(kind, 0), made

    def carrier_have(self, day):
        """{plane kind: carrier-capable planes the lines made by day} (start carriers fly no wings in history)."""
        if self._air_out is None:
            self.air_have("fighter", self.tl.start)
        made = Counter()
        for d, c in self._cv_out:
            if d > day:
                break
            made = c
        return made

    # ------------------------------------------------------------------ facts
    def facts(self):
        mix = []
        for w in self.wars:
            if w["start"] is None:
                continue
            army = self.army(w["start"])
            kinds = Counter()
            for t, n in army.items():
                kinds[self.profile(t)["kind"]] += n
            mix.append({"war": w["id"], "date": w["start_date"], "divisions": round(sum(army.values())),
                        "by_kind": {k: round(v) for k, v in kinds.most_common()},
                        "by_template": {t: round(n) for t, n in sorted(army.items(), key=lambda x: -x[1])}})
        self.lint.facts["force"] = {
            "wars": [{k: v for k, v in w.items() if k != "start"} for w in self.wars],
            "needs": self.needs, "army_mix": mix,
            "constants": {"mobile_per_axis": fm.MOBILE_PER_AXIS, "armour_per_axis": fm.ARMOUR_PER_AXIS,
                          "line_depth": fm.LINE_DEPTH, "fighter_ratio": fm.FIGHTER_RATIO,
                          "break_share": fm.BREAK_SHARE, "air_share": fm.AIR_SHARE, "min_planes": fm.MIN_PLANES, "single_share": fm.SINGLE_SHARE},
            "note": "force.py: needs per war from the plan's wars and the start OOBs (aggregates); army_mix from "
                    "the army forecast (the executor's training, equipment counted)"}

    # ------------------------------------------------------------------ rules
    def required(self):
        if self.fd is None:
            self.lint.add("error", "force.required", "force_design",
                          "the plan dates " + ", ".join(f"{w['id']} ({w['kind']}, {w['start_date']})" for w in
                                                       self.wars)
                          + " and has no force_design: per war and phase, what the force must do (break the line "
                            "and exploit, hold the rest of the front, win and use the air, cross water), then a "
                            "table of templates, air wings and ships with counts and ready dates "
                            "(facts.force.needs gives the arithmetic of each war)")
            return
        named = [set(x.get("enemies") or []) | {x.get("id")} for x in self.fd_list("wars")]
        # every country the goal defeats has its war in the force design, whatever horizon fights it
        for tag, where in sorted(fm.goal_enemies(self.lint).items()):
            if not any(tag in n for n in named):
                self.lint.add("error", "force.required", "force_design.wars",
                              f"the goal defeats {tag} ({where}) and no force_design.wars entry names it: give that "
                              "war its start, duration_days, decisive reasons, axes, the land, air and sea force by "
                              "date and the sea operations that reach it, even when a later horizon fights it")
        for w in self.wars:
            if not any(set(w["targets"]) & n for n in named):
                self.lint.add("error", "force.required", "force_design.wars",
                              f"{w['id']} ({', '.join(w['targets'])}, {w['kind']}, from {w['start_date']}) has no "
                              f"force_design.wars entry: name its enemies, tasks and the rows that serve it")

    def breakthrough(self):
        for w in self.offensive():
            row = self.need_of(w, "breakthrough")
            if not row or w["start"] is None:
                continue
            army = self.army(w["start"])
            armour = sum(n for t, n in army.items() if self.profile(t)["kind"] == "armour")
            mobile = sum(n for t, n in army.items() if fm.is_mobile(self.profile(t)["kind"]))
            if not row["required"]:
                continue
            if armour + 0.5 >= row["armour"] and mobile + 0.5 >= row["mobile"]:
                continue
            cands = []
            names = set(self.lint.start_templates()) | {x.get("name") for x in self.lint.list_of("templates")}
            for t in sorted(n for n in names if isinstance(n, str)):
                if not fm.is_mobile(self.profile(t)["kind"]):
                    continue
                made = self.tl.template_day(t)
                held = self.tl.limit_at(w["start"]).get(t)
                cands.append(f"{t} ({self.profile(t)['kind']}, {round(army.get(t, 0))} fielded, can exist from "
                             f"{_fmt(made) if made is not None else 'never (a tech it needs is not researched)'}"
                             + (f", training held by {held}" if held else "") + ")")
            self.lint.add("error", "force.breakthrough", "force_design",
                          f"{w['id']} starts {w['start_date']} against a defended line ({row['why']}) with "
                          f"{round(armour)} armoured and {round(mobile)} mobile divisions in the army forecast; its "
                          f"{w['axes'] or 1} strike axes need {row['armour']} armoured and {row['mobile']} armoured or "
                          f"motorised divisions by then. Mobile templates: {'; '.join(cands) or 'none'}. Create the "
                          "breakthrough template early enough (its techs, XP and 180 days of training before the "
                          "war), train it from its own lines (tank, truck and support factories) and put it in the "
                          "force table")

    def single_template(self):
        why = (self.fd or {}).get("single_template_why")
        offensive = bool(self.offensive())
        seen = set()
        for w in self.wars:
            if w["start"] is None:
                continue
            army = self.army(w["start"])
            total = sum(army.values())
            if not total:
                continue
            top, n = max(army.items(), key=lambda x: x[1])
            if n / total <= fm.SINGLE_SHARE or top in seen:
                continue
            seen.add(top)
            msg = (f"on {w['start_date']} ({w['id']}) {top} is {round(n)} of {round(total)} divisions "
                   f"({n / total:.0%}): one template cannot break a line, hold the rest of the front and exploit; "
                   "design the force per task (facts.force.needs) and train each template from its own entries")
            if offensive:
                self.lint.add("error", "force.single_template", "training", msg)
            elif not why:
                self.lint.add("warning", "force.single_template", "training",
                              msg + ", or say why one template is the whole army in "
                                    "force_design.single_template_why")

    def air(self):
        for w in self.wars:
            row = self.need_of(w, "fighters")
            if not row or w["start"] is None or not row["required"]:
                continue
            have, start, made = self.air_have("fighter", w["start"])
            if have < READY_SLACK * row["planes"]:
                self.lint.add("error", "force.air", "production",
                              f"{w['id']} starts {w['start_date']} with about {round(have)} fighters ({start} at the "
                              f"start, {round(made)} from the fighter lines) against {row['planes']} needed "
                              f"({row['why']}; {row['ic']} IC at {row['unit_cost']} IC a plane, "
                              f"{row['unit_cost_from']}): size the fighter line from the date the war needs them back "
                              "(a fighter line ramps up over months), a share of the military factories in "
                              "force_design.budget, and plan the wings in force_design.air")
            cas = self.need_of(w, "cas")
            if cas and cas["required"]:
                have_cas, start_cas, made_cas = self.air_have("cas", w["start"])
                if have_cas < READY_SLACK * cas["planes"]:
                    self.lint.add("error", "force.air_support", "production",
                                  f"{w['id']}: about {round(have_cas)} close air support planes on {w['start_date']} "
                                  f"({start_cas} at the start, {round(made_cas)} from the lines) against "
                                  f"{cas['planes']} ({cas['why']})")
        # air rows and air_plan entries against what the start wings and lines give
        asks = []
        for i, r in enumerate(self.fd_list("air")):
            kind = r.get("kind") or (fm.plane_kind(self.g, r.get("equipment")) if r.get("equipment") else None)
            if kind and isinstance(r.get("count"), int) and _day(r.get("ready")) is not None:
                asks.append((f"force_design.air[{i}]", kind, r["count"], _day(r["ready"])))
        per_kind = defaultdict(list)
        for i, a in enumerate(self.lint.list_of("air_plan")):
            kind = fm.plane_kind(self.g, a.get("airframe")) if a.get("airframe") else None
            day = fm.predicate_day(self.lint, self.tl, a.get("when"), self.wars) if a.get("when") is not None \
                else self.tl.start
            if kind and isinstance(a.get("count"), int) and day is not None:
                per_kind[(kind, day)].append((i, a["count"]))
        for (kind, day), rows in per_kind.items():
            asks.append((f"air_plan[{rows[0][0]}]", kind, sum(n for _, n in rows), day))
        for where, kind, count, day in asks:
            have, start, made = self.air_have(kind, day)
            if have < READY_SLACK * count:
                self.lint.add("error", "force.air", where,
                              f"{count} {kind} planes by {_fmt(day)}, but the start wings ({start}) and the lines "
                              f"({round(made)}) give about {round(have)}: give the line the factories that make them "
                              "by then, or plan fewer")

    def bombers(self):
        """force.bombers: the tactical bombers a long war's logistics campaign needs (force.py air_rows) against
        the start wings and the lines; a war of a year or more with no strategic bombers and no reason."""
        why = (self.fd or {}).get("bomber_why")
        for w in self.offensive():
            if w["start"] is None:
                continue
            tac = self.need_of(w, "tactical_bombers")
            if tac and tac["required"] and not why:
                have, start, made = self.air_have("tactical_bomber", w["start"])
                if have < READY_SLACK * tac["planes"]:
                    self.lint.add("error", "force.bombers", "production",
                                  f"{w['id']} ({w['start_date']}): about {round(have)} tactical bombers ({start} at "
                                  f"the start, {round(made)} from the lines) against {tac['planes']} ({tac['why']}): "
                                  "a tactical bomber line (a medium airframe design for logistics strike) sized "
                                  "from the war's date back, wings in air_plan on logistics_strike, or the reason "
                                  "in force_design.bomber_why")
            strat = self.need_of(w, "strategic_bombers")
            if strat and not why:
                rows = [r for r in self.fd_list("air") if r.get("kind") == "strategic_bomber"]
                if not rows and self.air_have("strategic_bomber", w["start"])[0] < fm.MIN_PLANES:
                    self.lint.add("warning", "force.bombers", "force_design.bomber_why",
                                  f"{w['id']} ({w['start_date']}): {strat['why']}")

    def design_cost(self):
        """force.design_cost: a tank, plane or ship design a line builds with no modules is costed on the bare
        chassis, hull or airframe; the force table and the lines' output then count too many units."""
        designs = {d.get("id"): d for d in self.lint.list_of("designs")}
        seen = set()
        for i, x in enumerate(self.lint.list_of("production")):
            d = designs.get(x.get("design"))
            if not d or d.get("id") in seen or d.get("kind") not in ("tank", "plane", "ship"):
                continue
            seen.add(d.get("id"))
            if isinstance(d.get("modules"), dict) and d["modules"]:
                continue
            cost, how = fm.unit_cost(self.lint, x.get("equipment"))
            self.lint.add("error", "force.design_cost", f"designs {d.get('id')}",
                          f"the line production[{i}] builds {d.get('id')} ({d.get('base')}), a design with no "
                          f"modules: it is costed at {cost} IC ({how}), not as designed. Give the design its modules "
                          "(slot -> module, from facts.py designs <TAG> --kind " + str(d.get("kind")) + ") so its "
                          "cost, the line's output and the force table count real units")

    def decisive(self):
        """force.decisive: each offensive war is planned to be won fast: a planned duration and its reasons
        (force_design.wars[].duration_days and decisive), the breakthrough force on the axes, air superiority
        over the front, at least the game's own AI's division count on that date (a sanity floor), and our
        divisions against the enemy's estimated strength. facts.force.decisive gives the numbers."""
        from . import lint_economy
        base = lint_economy.load_baseline(self.lint.tag, self.lint.start)
        named = self.fd_list("wars")
        rows = []
        for w in self.offensive():
            if w["start"] is None:
                continue
            need = next((x for x in self.needs if x["war"] == w["id"]), {})
            entry = next((x for x in named if set(x.get("enemies") or []) & set(w["targets"])), {})
            army = self.army(w["start"])
            ours = sum(army.values())
            front = need.get("front") or {}
            enemy = front.get("divisions_est") or front.get("divisions") or 0
            floor = _baseline_at(base, w["start"], "divisions")
            problems = []
            days = need.get("planned_days")
            if days is None:
                problems.append("no planned duration (force_design.wars[].duration_days, or a capitulation "
                                "checkpoint for its targets)")
            said, cp = fm.declared_days(self.lint, w), fm.checkpoint_days(self.lint, w)
            if said is not None and cp is not None and cp > said * 1.1 + 14:
                problems.append(f"duration_days {said} while the plan's own capitulation checkpoint for "
                                f"{', '.join(w['targets'])} falls {cp} days after the war starts: the timetable and "
                                "the force must make the shorter war true, or the label follows the timetable")
            if not entry.get("decisive"):
                problems.append("no reasons it is won fast (force_design.wars[].decisive: the force ratio on the "
                                "axes, the air, the supply)")
            bt = self.need_of(w, "breakthrough")
            if bt and bt["required"]:
                armour = sum(n for t, n in army.items() if self.profile(t)["kind"] == "armour")
                mobile = sum(n for t, n in army.items() if fm.is_mobile(self.profile(t)["kind"]))
                if armour + 0.5 < bt["armour"] or mobile + 0.5 < bt["mobile"]:
                    problems.append(f"{round(armour)} armoured and {round(mobile)} mobile divisions on the axes "
                                    f"against {bt['armour']} and {bt['mobile']}")
            f = self.need_of(w, "fighters")
            if f and f["want"]:
                have = self.air_have("fighter", w["start"])[0]
                if have < READY_SLACK * f["want"]:
                    problems.append(f"about {round(have)} fighters against the {f['want']} that win the air")
            if floor and ours < floor:
                problems.append(f"{round(ours)} divisions, under the game's own AI's {round(floor)} on that date "
                                "(a floor, not a target)")
            if enemy and ours < 0.5 * enemy:
                problems.append(f"{round(ours)} divisions against about {enemy} enemy divisions "
                                f"({front.get('growth_why', 'start OOB')}): the line cannot hold the rest of the "
                                "front while the axes break through")
            rows.append({"war": w["id"], "date": w["start_date"], "planned_days": days, "divisions": round(ours),
                         "enemy_divisions_est": enemy, "ai_divisions": floor, "problems": problems})
            if problems:
                self.lint.add("error", "force.decisive", f"force_design.wars {w['id']}",
                              f"{w['id']} ({w['start_date']}) is not planned to be won fast: " + "; ".join(problems)
                              + ". Plan the force, the air and the economy that crush it on the axes in weeks, "
                                "and say why")
        self.lint.facts.setdefault("force", {})["decisive"] = rows

    def support_companies(self):
        for w in self.wars:
            if w["start"] is None:
                continue
            army = self.army(w["start"])
            line = [t for t in army if self.profile(t)["kind"] in ("line", "mountain")]
            for role, cls, rule in (("anti_tank", "anti_tank", "force.anti_tank"),
                                    ("anti_air", "anti_air", "force.anti_air")):
                row = self.need_of(w, role)
                if not row or not line:
                    continue
                planned = {r.get("template") for r in self.fd_list("land") if r.get("role") == role}
                if any(self.profile(t)["classes"][cls] for t in line) or planned:
                    continue
                self.lint.add("warning", rule, "templates",
                              f"{w['id']} ({w['start_date']}): {row['why']}; no line template fielded then "
                              f"({', '.join(line[:4])}) carries {role.replace('_', '-')}")

    def invasions(self):
        g = self.g
        states = self.lint.world.states()
        own = fm.enemy_aggregate(self.lint, self.lint.tag)
        naval_air = any(r.get("kind") == "naval_bomber" or r.get("mission") in ("naval_strike", "port_strike")
                        for r in self.fd_list("air")) or \
            any(a.get("mission") in ("naval_strike", "port_strike") or a.get("then_mission") in
                ("naval_strike", "port_strike") for a in self.lint.list_of("air_plan"))
        for i, inv in enumerate(self.lint.list_of("invasions")):
            where = f"invasions[{i}] {inv.get('id', '')}".strip()
            owner = (states.get(inv.get("target_state")) or {}).get("owner")
            day = fm.predicate_day(self.lint, self.tl, inv.get("when"), self.wars)
            if not owner or owner == self.lint.tag or day is None:
                continue
            side = fm.enemy_aggregate(self.lint, owner)
            problems = []
            fighters = side["planes"].get("fighter", 0) + side["planes"].get("heavy_fighter", 0)
            have = self.air_have("fighter", day)[0]
            if fighters and have < fm.FIGHTER_RATIO * fighters * READY_SLACK:
                problems.append(f"about {round(have)} fighters against {owner}'s {fighters} start fighters (air "
                                f"superiority over the route and the beach needs {fm.FIGHTER_RATIO:g}x)")
            ours = own["surface_ic"] + self.dockyard_ic(day)
            if side["surface_ic"] >= fm.SEA_RATIO * max(ours, 1.0):
                nb = self.air_have("naval_bomber", day)[0]
                if not naval_air or nb < 1:
                    problems.append(f"{owner}'s start surface fleet ({side['surface']} ships, {side['surface_ic']} "
                                    f"hull IC) outweighs ours ({round(ours)} hull IC with the planned dockyards) "
                                    f"{side['surface_ic'] / max(ours, 1.0):.1f} times and the plan has "
                                    + ("no naval strike planes" if nb < 1 else "no naval strike mission")
                                    + " to win the sea regions from the air")
            if problems:
                self.lint.add("error", "force.invasion", where,
                              f"the landing on {_fmt(day)} at {owner}'s state {inv.get('target_state')}: "
                              + "; ".join(problems) + ". Put the fighters, naval bombers and escorts it needs in "
                                                      "force_design (air and navy rows dated before the landing)")

    def dockyard_ic(self, day):
        """Hull IC the plan's surface-ship lines add by day (dockyards x speed, chassis share of the variant)."""
        g = self.g
        speed = self.lint.define("POWERED_FACTORY_SPEED_NAV", self.lint.define("BASE_FACTORY_SPEED_NAV", 2.5))
        rows = derive.equipment_rows(g)
        total = 0.0
        d = self.tl.start
        while d < day:
            for eq, n in self.lint.pool_lines(d)["dockyards"].items():
                if fm.ship_class(g, eq) in fm.SURFACE:
                    base = (rows.get(eq) or {}).get("cost") or 0.0
                    full = fm.unit_cost(self.lint, eq)[0] or base
                    total += n * speed * 7 * (base / full if full else 0.5)
            d += 7
        return total

    def unused_lines(self):
        g = self.g
        lint = self.lint
        tank_fams, plane_fams = set(), set()
        names = set(lint.kept_start_templates()) | {x.get("name") for x in lint.list_of("templates")}
        subs = lint.world.sub_units()
        for t in names:
            if not isinstance(t, str):
                continue
            for u in self.tl.template_units(t):
                for eq in (subs.get(u) or {}).get("need") or {}:
                    tank_fams.add(derive.equipment_family(g, eq))
        for a in lint.world.oob(lint.tag)["air"]:
            plane_fams.add(derive.equipment_family(g, a["type"]))
        for a in lint.list_of("air_plan"):
            if a.get("airframe"):
                plane_fams.add(derive.equipment_family(g, a["airframe"]))
        for r in self.fd_list("air"):
            if r.get("equipment"):
                plane_fams.add(derive.equipment_family(g, r["equipment"]))
        kinds_used = {fm.plane_kind(g, f) for f in plane_fams} | {r.get("kind") for r in self.fd_list("air")}
        for i, x in enumerate(lint.list_of("production")):
            eq = x.get("equipment")
            if not isinstance(eq, str) or x.get("remove") or "replace" in x:
                continue
            fam = derive.equipment_family(g, eq)
            types = set(derive.equipment_types(g).get(eq) or ()) | set(derive.equipment_types(g).get(fam) or ())
            if "armor" in types and fam not in tank_fams:
                lint.add("error", "force.unused_line", f"production[{i}]",
                         f"{eq} has a line but no template the plan fields or creates uses {fam}: put the template "
                         "that uses it in the force table, or give the factories to what the force needs")
            elif fm.plane_kind(g, eq) and fam not in plane_fams and fm.plane_kind(g, eq) not in kinds_used:
                lint.add("error", "force.unused_line", f"production[{i}]",
                         f"{eq} ({fm.plane_kind(g, eq)}) has a line but no start wing, air_plan entry or "
                         "force_design.air row flies it: plan the wings it fills (kind, count, ready date, war), or "
                         "give the factories to what the force needs")
        made = {derive.equipment_family(g, x.get("equipment")) for x in lint.list_of("production")
                if isinstance(x.get("equipment"), str) and not x.get("remove")}
        made |= {derive.equipment_family(g, r["type"]) for r in lint.start_lines()}
        stock = {derive.equipment_family(g, s.get("v")) for s in lint.world.country(lint.tag).get("stockpile") or []
                 if (s.get("amount") or 0) > 0}
        for i, r in enumerate(self.fd_list("air")):
            eq = r.get("equipment")
            if isinstance(eq, str) and derive.equipment_family(g, eq) not in made | plane_fams_start(lint, g):
                lint.add("error", "force.unused_line", f"force_design.air[{i}]",
                         f"{eq} has no production line and no start wing: the row cannot be filled")
        for i, r in enumerate(self.fd_list("land")):
            t = r.get("template")
            if not isinstance(t, str):
                continue
            miss = sorted({derive.equipment_family(g, eq) for u in self.tl.template_units(t)
                           for eq in (subs.get(u) or {}).get("need") or {}} - made - stock)
            if miss:
                lint.add("error", "force.unused_line", f"force_design.land[{i}]",
                         f"{t} needs {', '.join(miss)}, which no line makes and no start stock holds")

    def table(self):
        lint = self.lint
        wars = {x.get("id"): x for x in self.fd_list("wars")}
        derived = {w["id"]: w for w in self.wars}

        def war_start(ref):
            fw = wars.get(ref)
            if fw is None:
                return None
            if fw.get("start") and _day(fw["start"]) is not None:
                return _day(fw["start"])
            for w in self.wars:
                if set(fw.get("enemies") or []) & set(w["targets"]):
                    return w["start"]
            return derived.get(ref, {}).get("start")

        rows_out = []
        target = defaultdict(list)
        for i, r in enumerate(self.fd_list("land")):
            t, n, ready = r.get("template"), r.get("count"), _day(r.get("ready"))
            if not isinstance(t, str) or not isinstance(n, int) or ready is None:
                continue
            target[t].append((ready, n, i, r))
        for t, rows in target.items():
            made = self.tl.template_day(t)
            days = max([((lint.world.sub_units().get(u) or {}).get("training_time") or 0)
                        for u in self.tl.template_units(t)] or [90])
            for ready, n, i, r in sorted(rows):
                where = f"force_design.land[{i}] {t}"
                have = self.army(ready).get(t, 0.0)
                rows_out.append({"row": where, "role": r.get("role"), "ready": _fmt(ready), "count": n,
                                 "forecast": round(have), "can_exist": _fmt(made) if made is not None else "never"})
                ws = war_start(r.get("war"))
                due = min([d for d in (ready, ws) if d is not None])
                if made is None or made + days > due:
                    lint.add("error", "force.template_late", where,
                             f"{t} can exist from {_fmt(made) if made is not None else 'never (a tech its units or '}"
                             f"{'' if made is not None else 'its `when` wait on is not in the research plan)'}"
                             f"{'' if made is None else f' and trains {days:g} days'}, after "
                             f"{_fmt(due)} ({'its war ' + str(r.get('war')) + ' starts' if ws == due else 'its ready date'}): "
                             "create it earlier (research its units first, its XP, a `when` before the war) or plan "
                             "another template for that war")
                    continue
                if have < READY_SLACK * n:
                    held = self.tl.limit_at(ready).get(t)
                    lint.add("error", "force.readiness", where,
                             f"{n} {t} by {_fmt(ready)}, but the army forecast fields {round(have)}"
                             + (f" (training held by {held})" if held else " (no training entry trains it then)")
                             + ": training entries with queues and caps that reach the count, and lines that make "
                               "every equipment it uses by then")
        for i, r in enumerate(self.fd_list("air")):
            kind = r.get("kind") or (fm.plane_kind(self.g, r.get("equipment")) if r.get("equipment") else None)
            if kind and isinstance(r.get("count"), int) and _day(r.get("ready")) is not None:
                have = self.air_have(kind, _day(r["ready"]))[0]
                rows_out.append({"row": f"force_design.air[{i}]", "role": kind, "ready": r["ready"],
                                 "count": r["count"], "forecast": round(have)})
        lint.facts.setdefault("force", {})["table"] = rows_out

    def budget(self):
        if self.fd is None:
            return
        rows = self.fd_list("budget")
        if not rows:
            self.lint.add("error", "force.budget", "force_design.budget",
                          "no factory budget by date: give the military factories' split between land, air and navy "
                          "(and the dockyards' between ships and convoys) from the force table's equipment, with the "
                          "reason")
            return
        for i, r in enumerate(rows):
            d = _day(r.get("from"))
            if d is None:
                continue
            mil = self.lint.pool_lines(d)["military"]
            total = sum(mil.values())
            if not total:
                continue
            share = Counter()
            for eq, n in mil.items():
                share[derive.equipment_domain(self.g, eq) or "land"] += 100.0 * n / total
            off = [f"{k} {share.get(k, 0):.0f}% against {r[k]}%" for k in ("land", "air")
                   if isinstance(r.get(k), (int, float)) and abs(share.get(k, 0) - r[k]) > BUDGET_POINTS]
            if off:
                self.lint.add("warning", "force.budget", f"force_design.budget[{i}]",
                              f"on {r.get('from')} the lines put " + ", ".join(off)
                              + " of the military factories: size the lines to the budget")


def _baseline_at(base, day, key):
    """The AI baseline's value of `key` on day, linear between its rows (None outside them or without one)."""
    if not base:
        return None
    pts = [(derive.day(r.get("date")), r.get(key)) for r in base.get("rows") or [] if r.get(key) is not None]
    pts = [(d, v) for d, v in pts if d is not None]
    for (d0, v0), (d1, v1) in zip(pts, pts[1:]):
        if d0 <= day <= d1:
            return v0 + (v1 - v0) * (day - d0) / max(1, d1 - d0)
    return None


def plane_fams_start(lint, g):
    return {derive.equipment_family(g, a["type"]) for a in lint.world.oob(lint.tag)["air"]}
