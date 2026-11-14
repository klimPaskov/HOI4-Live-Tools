"""The planner worker: runs in the play supervisor beside the executor and keeps the plan filled and correct.

Each `tick()` (the supervisor calls it every few wall seconds; it reads at most every `read_every` seconds):
  1. reads the state through the runtime's read tools (no input lease, no input, never a pause) and the change
     feed since its last look;
  2. estimates each queue's horizon and evaluates the triggers;
  3. when triggers fire and no update is running, starts one update in a background thread (the executor keeps
     following the plan in force; the game keeps running);
  4. after an update, watches the executor for `ROLLBACK_DAYS` game days and rolls the update back when the
     failures in that time exceed the ones before it by `ROLLBACK_MARGIN`.
The update's version, summary and cost are passed to the player as a supervisor note; everything goes to
RUN/planner/journal.jsonl and RUN/planner/summary.json (read by the run report).
"""

import json
import os
import threading
import time

from . import budget as bd
from . import horizon as hz
from . import ledger as ld
from . import store as st
from . import triggers as tg
from . import update as up
from .state import StateBuilder

READ_EVERY = 15
ROLLBACK_DAYS = 10
ROLLBACK_MARGIN = 3
FAILURE_KINDS = ("autopilot_step_failed",)
FAILURE_REASONS = ("refused", "order_given_up")


def _reads(plan, snap):
    enemies = sorted(set((plan.get("war") or {}).get("enemies") or []) | set(snap.get("wars") or []))
    out = [("hoi4_overview", {}), ("hoi4_economy", {}), ("hoi4_armies", {"threat": False}), ("hoi4_war_stats", {}),
           ("hoi4_production_lines", {}), ("hoi4_technology_list", {"available": True, "limit": 100}),
           ("hoi4_research", {"limit": 100})]
    out += [("hoi4_states", {"query": tag}) for tag in enemies[:4]]
    keys = hz.planned_focuses(plan)[:32]
    if keys:
        out.append(("hoi4_focus_available", {"focuses": keys}))
    return out


class PlannerWorker:
    def __init__(self, run_dir, call, lock=None, facts=None, tools=None, proposer=None, note=None,
                 clock=time.time, read_every=READ_EVERY, limits=None, background=True):
        self.run_dir, self.call, self.note = run_dir, call, note or (lambda text: None)
        self.lock = lock or threading.RLock()
        self.clock, self.read_every, self.background = clock, read_every, background
        self.dir = os.path.join(run_dir, "planner")
        os.makedirs(self.dir, exist_ok=True)
        self.store = st.PlanStore(run_dir, self.lock)
        self.ledger = ld.Ledger(os.path.join(self.dir, "ledger.jsonl"))
        self.budget = bd.Budget(clock=clock, **(limits or {}))
        self.state = StateBuilder()
        self.memory_path = os.path.join(self.dir, "triggers.json")
        self.triggers = tg.Triggers(up.read_json(self.memory_path))
        self.updater = up.Updater(run_dir, self.store, self.ledger, self.budget, facts, tools, proposer, clock)
        self.costs = facts.costs() if facts else hz.Costs()
        self.seq = 0
        self.last_read = None
        self.thread = None
        self.watch = None
        self.events_log = []
        self.counts = {"ticks": 0, "updates": 0, "applied": 0, "rejected": 0, "no_change": 0, "rollbacks": 0, "triggers": {}}
        self.hour = None

    def attach(self, facts, tools=None):
        """Bind the game files' numbers and the planning tools once they are loaded (a background thread)."""
        self.costs = facts.costs()
        self.updater.facts, self.updater.costs, self.updater.tools = facts, self.costs, tools

    # ------------------------------------------------------------------ reads
    def read(self):
        plan, _ = self.store.read()
        snap = self.state.snapshot()
        for name, args in _reads(plan, snap):
            err, body = self.call(name, args)
            if not err:
                self.state.feed(name, body, args)
        err, feed = self.call("hoi4_observe", {"since": self.seq})
        if not err and isinstance(feed, dict):
            self.seq = feed.get("seq", self.seq)
            self.state.feed("hoi4_observe", feed)

    def plan_state(self):
        return up.read_json(os.path.join(self.run_dir, "plan_state.json"))

    # ------------------------------------------------------------------ tick
    def busy(self):
        return self.thread is not None and self.thread.is_alive()

    def tick(self):
        now = self.clock()
        if self.last_read is not None and now - self.last_read < self.read_every:
            return None
        self.last_read = now
        self.counts["ticks"] += 1
        self.read()
        return self.step()

    def step(self, wait=False):
        """Evaluate the triggers on the state read so far and start an update when one fires."""
        snap = self.state.snapshot()
        events = self.state.take_events()
        self.events_log = (self.events_log + events)[-400:]
        if snap.get("day") is None:
            return None
        pstate = self.plan_state()
        if pstate and isinstance(pstate.get("game_hour"), int):
            self.ledger.from_executor(pstate, ld.hour_to_day_from(pstate["game_hour"], snap["day"]))
        self.check_rollback(snap)
        plan, _ = self.store.read()
        h = hz.estimate(plan, snap, self.costs, pstate)
        fired = self.triggers.evaluate(plan, snap, h, events)
        self._save_memory()
        if not fired or self.busy():
            return None
        self.triggers.fired(fired, snap["day"])
        for t in fired:
            self.counts["triggers"][t["kind"]] = self.counts["triggers"].get(t["kind"], 0) + 1
        self._save_memory()
        if self.background and not wait:
            self.thread = threading.Thread(target=self._update, args=(fired, snap, h, pstate), daemon=True)
            self.thread.start()
            return fired
        return self._update(fired, snap, h, pstate)

    def _update(self, fired, snap, h, pstate):
        try:
            entry = self.updater.run(fired, snap, h, pstate)
        except Exception as exc:  # noqa: BLE001 - a failed update leaves the plan in force untouched
            entry = {"error": str(exc)[:300], "triggers": [t["id"] for t in fired], "date": snap.get("date")}
            with open(os.path.join(self.dir, "journal.jsonl"), "a", encoding="utf-8") as f:
                f.write(json.dumps(entry) + "\n")
        self.counts["updates"] += 1
        self.triggers.updated(snap["day"])          # the schedule counts from the last update, applied or not
        if entry.get("version"):
            self.counts["applied"] += 1
            self.watch = {"version": entry["version"], "day": snap["day"],
                          "before": self.failures(snap["day"] - ROLLBACK_DAYS, snap["day"])}
            self.note("plan updated to v%d (%s, %s): %s" % (entry["version"], ", ".join(entry["triggers"])[:120],
                                                          entry.get("applied"), entry.get("summary") or ""))
        elif any(str(p.get("result", "")).startswith(("refused", "gates", "conflict")) for p in entry.get("proposals") or []):
            self.counts["rejected"] += 1
        else:
            self.counts["no_change"] += 1
        self._save_memory()
        self.write_summary()
        return entry

    # ------------------------------------------------------------------ rollback
    def failures(self, start, end):
        n = 0
        for e in self.events_log:
            d = e.get("day")
            if d is None or not start <= d < end:
                continue
            reason = str((e.get("detail") or {}).get("reason") or "").split(":")[0]
            if e.get("kind") in FAILURE_KINDS or (e.get("kind") == "wake_player" and reason in FAILURE_REASONS):
                n += 1
        return n

    def check_rollback(self, snap):
        w = self.watch
        if not w or snap["day"] < w["day"] + ROLLBACK_DAYS:
            return None
        self.watch = None
        after = self.failures(w["day"], w["day"] + ROLLBACK_DAYS)
        if after <= w["before"] + ROLLBACK_MARGIN:
            return None
        plan, _ = self.store.read()
        if (plan.get("_meta") or {}).get("version") != w["version"]:
            return None
        why = "%d executor failures in the %d game days after v%d, %d before" % (after, ROLLBACK_DAYS, w["version"],
                                                                                w["before"])
        done = self.store.rollback(why)
        if done:
            self.counts["rollbacks"] += 1
            self.ledger.add(snap["day"], "update", "v%d" % w["version"], "rolled_back", why)
            self.note("plan rolled back to v%s as v%d: %s" % (done[1], done[0], why))
        return done

    # ------------------------------------------------------------------ records
    def _save_memory(self):
        up.write_json(self.memory_path, self.triggers.m)

    def summary(self):
        return dict(self.counts, budget=self.budget.summary(), state_date=self.state.date)

    def write_summary(self):
        up.write_json(os.path.join(self.dir, "summary.json"), self.summary())

    def submit(self, number, patch):
        """The user's agent answers update `number` (hoi4_run plan_patch)."""
        folder = os.path.join(self.dir, "update-%d" % int(number))
        if not os.path.isdir(folder) or not isinstance(patch, dict):
            return False, "no update %s is waiting, or the patch is not an object" % number
        up.write_json(os.path.join(folder, "patch.json"), patch)
        return True, "received; it is checked by the gates and applied when it passes"


def proposer_for(settings_loader=None, tools_dir=None, notify=None):
    """The model proposer when a planner model and the launcher are configured, else the user's agent."""
    try:
        from .. import config
        settings = (settings_loader or config.load_agents)()
        model = ((settings.get("roles") or {}).get("planner") or {}).get("model")
    except Exception:  # noqa: BLE001 - no agents section: the user's agent answers
        model = None
    if model and tools_dir and os.environ.get("HOI4_LIVE_PLANNER", "model") != "agent":
        return up.ModelProposer(tools_dir)
    return up.AgentProposer(notify) if notify else None
