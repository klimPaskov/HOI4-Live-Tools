"""When the plan needs an update: horizon low, checkpoints, a schedule, deviations, wars and peace, requests.

`Triggers.evaluate(plan, snap, horizon, events)` returns the triggers that fire now, each a dict
{"id", "kind", "domain", "class": "detail" | "strategy", "why", "evidence"}. The engine's memory (cooldowns, which
measures are disarmed until they recover, checkpoints already handled, strategy triggers waiting for their
confirmation) is a plain dict, so the worker can keep it in a file across restarts.

Stability (docs/ROLLING_PLAN.md D9):
  * cooldown   a trigger id fires at most once per its cooldown in game days;
  * hysteresis a days-based horizon trigger is disarmed when it fires and re-armed only when its queue has
               recovered past RECOVER x the low-water mark (an update that did not help cannot re-fire it at
               once; the ledger says so instead);
  * confirmation a strategy trigger other than a war change escalates only when it is seen again at least
               CONFIRM_DAYS later; before that it is handled as a detail update.
"""

COOLDOWN = {"horizon_low": 20, "horizon_low:production": 15, "checkpoint_due": 0, "schedule": 0,
            "deviation:supply": 15, "deviation:casualties": 15, "war_changed": 0, "goal_at_risk": 30,
            "player_request": 10, "blocked": 10, "contingency": 10}
SCHEDULE = {"peace": 60, "war": 30}
RECOVER = 1.5
CONFIRM_DAYS = 14
BLOCKED_REASONS = ("order_given_up", "refused")


def fresh_memory():
    return {"last_fired": {}, "disarmed": {}, "checkpoints": {}, "pending_strategy": {}, "last_update": None,
            "seen_missed": []}


class Triggers:
    def __init__(self, memory=None, cooldown=None, schedule=None):
        self.m = memory if memory is not None else fresh_memory()
        for k, v in fresh_memory().items():
            self.m.setdefault(k, v)
        self.cooldown = dict(COOLDOWN, **(cooldown or {}))
        self.schedule = dict(SCHEDULE, **(schedule or {}))

    # ------------------------------------------------------------------ helpers
    def _cool(self, tid, day):
        base = tid.split(":")[0]
        wait = self.cooldown.get(tid, self.cooldown.get(base, 10))
        last = self.m["last_fired"].get(tid)
        return last is None or day is None or day - last >= wait

    def _strategy(self, tid, day, immediate=False):
        """True when a strategy trigger escalates now; otherwise it waits for its confirmation."""
        if immediate:
            return True
        first = self.m["pending_strategy"].get(tid)
        if first is None:
            self.m["pending_strategy"][tid] = day
            return False
        return day - first >= CONFIRM_DAYS

    # ------------------------------------------------------------------ evaluate
    def evaluate(self, plan, snap, horizon, events, low=None):
        day = snap.get("day")
        if day is None:
            return []
        out = []

        def fire(tid, kind, why, domain=None, cls="detail", evidence=None, check_cool=True):
            if check_cool and not self._cool(tid, day):
                return
            out.append({"id": tid, "kind": kind, "domain": domain, "class": cls, "why": why,
                        "evidence": evidence or {}})

        # 1. horizon low per domain, with hysteresis on the days-based ones
        marks = {"research": (low or {}).get("research_days", 60), "focus": (low or {}).get("focus_days", 35)}
        for domain, h in (horizon or {}).items():
            tid = "horizon_low:%s" % domain
            if domain == "army":
                if h.get("low"):
                    fire("goal_unmet:army", "goal_at_risk", "every victory-point target is held and the goal is "
                         "not reached", domain, "strategy", {"targets_left": 0})
                continue
            if domain in marks and isinstance(h.get("days"), (int, float)) and h["days"] >= marks[domain] * RECOVER:
                self.m["disarmed"].pop(tid, None)
            if not h.get("low") or self.m["disarmed"].get(tid):
                continue
            fire(tid, "horizon_low", h.get("why"), domain,
                 evidence={k: v for k, v in h.items() if k not in ("why", "left")})
        # 2. checkpoints whose date has come
        for ck in plan.get("checkpoints") or []:
            if not isinstance(ck, dict) or not ck.get("id"):
                continue
            from .state import day_of
            by = day_of(ck.get("by"))
            if by is None or by > day or ck["id"] in self.m["checkpoints"]:
                continue
            fire("checkpoint:%s" % ck["id"], "checkpoint_due", "checkpoint %s (%s) is due" % (ck["id"], ck.get("by")),
                 evidence={"checkpoint": ck["id"], "by": ck.get("by"), "else": ck.get("else")}, check_cool=False)
        # 3. feed events
        enemies = set((plan.get("war") or {}).get("enemies") or [])
        for ev in events or []:
            kind, detail = ev.get("kind"), ev.get("detail") or {}
            if kind == "checkpoint_missed":
                ck = detail.get("checkpoint")
                self.m["checkpoints"][ck] = "missed"
                self.m["seen_missed"] = (self.m["seen_missed"] + [ck])[-4:]
                two = len(self.m["seen_missed"]) >= 2
                tid = "checkpoint:%s" % ck
                out[:] = [t for t in out if t["id"] != tid]
                cls = "strategy" if two and self._strategy("checkpoints_missed", day) else "detail"
                fire(tid, "checkpoint_due", "checkpoint %s missed: %s" % (ck, ", ".join(detail.get("failed") or [])),
                     cls=cls, evidence=detail, check_cool=False)
            elif kind in ("war_started", "war_ended", "peace_conference_opened"):
                tag = detail.get("with") or detail.get("tag")
                expected = kind == "war_started" and tag in enemies
                if not expected:
                    fire("war_changed:%s:%s" % (kind, tag), "war_changed", "%s %s not in the plan's war" % (kind, tag),
                         cls="strategy", evidence=detail, check_cool=False)
            elif kind == "phase_changed" and detail.get("revise_on_enter"):
                fire("phase:%s" % detail.get("to"), "war_changed", "phase %s asks for a revision" % detail.get("to"),
                     cls="strategy", evidence=detail, check_cool=False)
            elif kind == "goal_at_risk":
                cls = "strategy" if self._strategy("goal_at_risk", day) else "detail"
                fire("goal_at_risk", "goal_at_risk", detail.get("why") or "goal at risk", cls=cls, evidence=detail)
            elif kind == "army_out_of_supply":
                fire("deviation:supply", "deviation", "an army is out of supply on the advance", "war", evidence=detail)
            elif kind == "revision_requested":
                fire("player_request", "player_request", detail.get("reason") or "the player asked", evidence=detail)
            elif kind == "construction_item_blocked":
                fire("blocked:%s" % detail.get("item"), "blocked", "construction item %s given up: %s"
                     % (detail.get("item"), detail.get("why")), "construction", evidence=detail)
            elif kind == "contingency_triggered" and detail.get("revise"):
                fire("contingency:%s" % detail.get("contingency"), "contingency", "contingency %s asks for a %s "
                     "revision" % (detail.get("contingency"), detail.get("revise")), evidence=detail)
            elif kind == "wake_player":
                reason = str(detail.get("reason") or "")
                base = reason.split(":")[0]
                if base == "casualty_ratio":
                    fire("deviation:casualties", "deviation", detail.get("message") or reason, "war", evidence=detail)
                elif base in BLOCKED_REASONS:
                    fire("blocked:%s" % reason[:120], "blocked", detail.get("message") or reason, evidence=detail)
        # 4. the schedule
        last = self.m.get("last_update")
        every = self.schedule["war" if snap.get("wars") else "peace"]
        if last is None:
            self.m["last_update"] = last = day
        if day - last >= every:
            fire("schedule", "schedule", "%d game days since the last update" % (day - last), check_cool=False)
        # one trigger per id
        seen, unique = set(), []
        for t in out:
            if t["id"] not in seen:
                seen.add(t["id"])
                unique.append(t)
        return unique

    # ------------------------------------------------------------------ bookkeeping
    def fired(self, triggers, day):
        for t in triggers:
            self.m["last_fired"][t["id"]] = day
            if t["kind"] == "horizon_low" and t.get("domain") in ("research", "focus"):
                self.m["disarmed"][t["id"]] = day
            if t["kind"] == "checkpoint_due":
                self.m["checkpoints"].setdefault(t["evidence"].get("checkpoint"), "due")
            if t["class"] == "strategy":
                self.m["pending_strategy"].clear()

    def updated(self, day):
        self.m["last_update"] = day

    def rearm(self, tid):
        self.m["disarmed"].pop(tid, None)
