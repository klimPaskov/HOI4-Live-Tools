"""The game state the planner works from, built from the runtime's read tools and its change feed.

`StateBuilder.feed(tool, body)` takes one tool body (what the supervisor reads live, or a recorded call in a
replay); `snapshot()` returns the compact state:

    player             the player's tag
    date, day          the game date ("1936.03.14") and its day number
    paused             the game's pause flag
    slots, researching research slots and what each researches (None when idle)
    research_done      techs known researched (hoi4_research, plus a slot's tech that left it while the date ran)
    available_techs    {tech: start year} the game lets start now (hoi4_technology_list available:true)
    focus, focuses_done the running focus and the completed ones (feed focus_finished, hoi4_focus_available)
    focus_states       {focus: available | unavailable | unknown} from the last hoi4_focus_available
    pp, pp_gain        political power and its gain per day (from two hoi4_economy reads, or the feed)
    civ_idle           idle civilian factories
    wars               enemy tags at war with the player
    controllers        {state id: controller tag}
    vp                 {state id: {"owner", "name", "provinces": [{"province", "vp", "controller"}]}} (hoi4_states)
    armies             {army: {divisions, planning, fill}}
    shortages          {equipment family: {"day", "days_of_cover"}} from the feed's stockpile_low
    fill_low           {army: {"day", "fill"}} from the feed's equipment_fill_low
    signals            {reason: last day} for every other feed reason the triggers use
    phase              the executor's current phase id
    events             feed changes not yet taken by take_events()

Nothing is guessed: a value no read has given stays None.
"""

import datetime

EPOCH = datetime.date(1, 1, 1).toordinal()


def day_of(text):
    """Day number of "1936.1.1", "1936.01.01.12" or {"year","month","day"}; None when not a date."""
    if isinstance(text, dict):
        try:
            return datetime.date(int(text["year"]), int(text["month"]), int(text["day"])).toordinal() - EPOCH
        except (KeyError, TypeError, ValueError):
            return None
    if not isinstance(text, str):
        return None
    parts = text.strip().split(".")
    if len(parts) < 3:
        return None
    try:
        return datetime.date(int(parts[0]), int(parts[1]), int(parts[2])).toordinal() - EPOCH
    except ValueError:
        return None


def date_text(day):
    d = datetime.date.fromordinal(int(day) + EPOCH)
    return "%d.%02d.%02d" % (d.year, d.month, d.day)


def year_of(day):
    return datetime.date.fromordinal(int(day) + EPOCH).year


FEED_TOOLS = ("hoi4_observe", "hoi4_wait_change")


class StateBuilder:
    def __init__(self):
        self.player = None
        self.date = None
        self.day = None
        self.paused = None
        self.researching = []
        self.research_done = set()
        self.available_techs = {}
        self.focus = None
        self.focuses_done = set()
        self.focus_states = {}
        self.pp = None
        self.pp_gain = None
        self._pp_sample = None
        self.civ_idle = None
        self.wars = set()
        self.wars_known = False
        self.controllers = {}
        self.vp = {}
        self.armies = {}
        self.shortages = {}
        self.fill_low = {}
        self.signals = {}
        self.phase = None
        self.lines = {}
        self._seen = set()
        self._events = []

    # ------------------------------------------------------------------ input
    def feed(self, tool, body, args=None):
        if not isinstance(body, dict):
            return
        handler = getattr(self, "_" + tool[len("hoi4_"):], None) if tool.startswith("hoi4_") else None
        if tool in FEED_TOOLS:
            self._changes(body)
        elif handler:
            handler(body, args or {})

    def _set_date(self, text):
        day = day_of(text)
        if day is not None and (self.day is None or day >= self.day):
            self.day, self.date = day, date_text(day)

    def _overview(self, body, args):
        date = body.get("date") or {}
        self._set_date(date.get("text") if isinstance(date, dict) else date)
        if "paused" in body:
            self.paused = body.get("paused")
        if body.get("player"):
            self.player = body["player"]
        slots = body.get("research")
        if isinstance(slots, list):
            now = [s.get("researching") if isinstance(s, dict) else None for s in slots]
            # A tech that left its slot while the game ran was finished (the executor never cancels research).
            if self.researching and self.paused is False:
                for tech in self.researching:
                    if tech and tech not in now:
                        self.research_done.add(tech)
            self.researching = now
        focus = body.get("focus")
        key = focus.get("key") if isinstance(focus, dict) else None
        if self.focus and key != self.focus and self.paused is False:
            self.focuses_done.add(self.focus)
        if "focus" in body:
            self.focus = key

    def _research(self, body, args):
        for tech in body.get("researched") or []:
            if isinstance(tech, str):
                self.research_done.add(tech)
            elif isinstance(tech, dict) and tech.get("id"):
                self.research_done.add(tech["id"])

    def _technology_list(self, body, args):
        if body.get("available_only") or args.get("available"):
            years = body.get("start_years") or {}
            techs = body.get("technologies") or list(years)
            self.available_techs = {t: years.get(t) for t in techs if isinstance(t, str)}

    def _focus_available(self, body, args):
        for row in body.get("focuses") or []:
            key, state = row.get("focus"), row.get("state")
            if not key:
                continue
            self.focus_states[key] = state
            if "already completed" in (row.get("why") or []):
                self.focuses_done.add(key)
        if isinstance(body.get("wars"), list):
            self.wars = set(body["wars"])
            self.wars_known = True

    def _economy(self, body, args):
        raw = (body.get("political_power") or {}).get("raw_signed")
        if isinstance(raw, (int, float)) and self.day is not None:
            pp = raw / 100000.0
            if self._pp_sample and self.day > self._pp_sample[0]:
                gain = (pp - self._pp_sample[1]) / (self.day - self._pp_sample[0])
                if gain >= 0:  # a purchase in between hides the gain; keep the last clean one
                    self.pp_gain = round(gain, 3)
            self._pp_sample = (self.day, pp)
            self.pp = round(pp, 1)
        if isinstance(body.get("civilian_factories_idle"), (int, float)):
            self.civ_idle = body["civilian_factories_idle"]

    def _states(self, body, args):
        for row in body.get("states") or []:
            sid, ctl = row.get("state_id"), row.get("controller")
            if isinstance(sid, int) and isinstance(ctl, str):
                self.controllers[sid] = ctl
            vps = row.get("victory_points")
            if isinstance(sid, int) and isinstance(vps, list):
                self.vp[sid] = {"owner": row.get("owner"), "name": row.get("name"),
                                "provinces": [{"province": v.get("province"), "vp": v.get("victory_points"),
                                               "controller": v.get("controller")} for v in vps if isinstance(v, dict)]}

    def _armies(self, body, args):
        out = {}
        for row in body.get("armies") or []:
            name = row.get("army")
            if not name:
                continue
            planning = row.get("planning") or {}
            out[name] = {"divisions": row.get("division_count"), "planning": planning.get("mean"),
                         "executing": row.get("plan_executing"), "states": row.get("states")}
            old = self.armies.get(name) or {}
            if "fill" in old:
                out[name]["fill"] = old["fill"]
        self.armies = out

    def _war_stats(self, body, args):
        tag = (body.get("player") or {}).get("tag")
        enemies = set()
        for war in body.get("wars") or []:
            for side in war.get("sides") or []:
                if side.get("includes_player"):
                    continue
                enemies.update(c.get("tag") for c in side.get("countries") or [] if c.get("tag"))
        if tag:
            enemies.discard(tag)
        if body.get("wars") is not None:
            self.wars = enemies
            self.wars_known = True

    def _production_lines(self, body, args):
        lines = {}
        for row in body.get("lines") or []:
            eq = row.get("equipment_id")
            if eq:
                counters = row.get("factory_counters") or {}
                lines[eq] = lines.get(eq, 0) + (counters.get("active_factories_raw_signed") or 0)
        if body.get("lines") is not None:
            self.lines = lines

    def _changes(self, body):
        for change in body.get("changes") or []:
            seq = change.get("seq")
            if seq is not None:
                if seq in self._seen:
                    continue
                self._seen.add(seq)
            self._change(change)

    def _change(self, change):
        kind, detail = change.get("kind"), change.get("detail") or {}
        if change.get("date"):
            self._set_date(change["date"])
        day = day_of(change.get("date")) if change.get("date") else self.day
        if kind == "focus_finished" and detail.get("focus"):
            self.focuses_done.add(detail["focus"])
            if self.focus == detail["focus"]:
                self.focus = None
        elif kind == "war_started" and detail.get("with"):
            self.wars.add(detail["with"])
        elif kind == "war_ended" and detail.get("with"):
            self.wars.discard(detail["with"])
        elif kind == "phase_changed":
            self.phase = detail.get("to")
        elif kind == "paused_changed":
            self.paused = detail.get("paused")
        elif kind == "civilian_factories_idle":
            self.civ_idle = detail.get("idle")
            self.signals["civilian_factories_idle"] = day
        elif kind == "wake_player":
            reason = str(detail.get("reason") or "")
            evidence = detail.get("evidence") or {}
            base = reason.split(":")[0]
            if base == "political_power_idle" and isinstance(evidence.get("political_power"), (int, float)):
                self.pp = evidence["political_power"]
            if base == "stockpile_low" and evidence.get("equipment"):
                self.shortages[evidence["equipment"]] = {"day": day, "days_of_cover": evidence.get("days_of_cover")}
            elif base == "equipment_fill_low":
                for army in evidence.get("armies") or []:
                    if army.get("army"):
                        self.fill_low[army["army"]] = {"day": day, "fill": army.get("equipment_fill_mean"),
                                                       "short": sorted(army.get("short_equipment") or {})}
                        self.armies.setdefault(army["army"], {})["fill"] = army.get("equipment_fill_mean")
            if base != "alert":
                self.signals[base] = day
        else:
            self.signals[kind] = day
        self._events.append({"kind": kind, "day": day, "detail": detail})

    # ------------------------------------------------------------------ output
    def take_events(self):
        out, self._events = self._events, []
        return out

    def snapshot(self):
        return {
            "player": self.player, "date": self.date, "day": self.day, "paused": self.paused,
            "slots": len(self.researching), "researching": list(self.researching),
            "research_done": sorted(self.research_done), "available_techs": dict(self.available_techs),
            "focus": self.focus, "focuses_done": sorted(self.focuses_done), "focus_states": dict(self.focus_states),
            "pp": self.pp, "pp_gain": self.pp_gain, "civ_idle": self.civ_idle,
            "wars": sorted(self.wars), "wars_known": self.wars_known,
            "controllers": {str(k): v for k, v in sorted(self.controllers.items())},
            "vp": {str(k): v for k, v in sorted(self.vp.items())},
            "armies": self.armies, "shortages": self.shortages, "fill_low": self.fill_low,
            "signals": dict(self.signals), "phase": self.phase, "lines": dict(self.lines),
        }
