"""The fast clock: run game time from the telemetry, not from the screen.

The old loop (timectl.run_days) paused the game every two days, read the date by OCR and polled the screen about
once a second; on a busy machine that cost as much wall time as the simulation itself. This loop unpauses once and
then only reads the telemetry file (a byte delta of game.log, no screen, no OCR):

    unpause, set the speed             speed icon and speed bar by pixels
    every poll (0.3 s)                 new `day` records -> stop conditions per day (state/advance.evaluate)
    every ~1.5 s                       one screen grab: popups (the parchment detector) and the paused icon; the
                                       game pauses itself for event and notice windows, so a pause nobody asked
                                       for means a window to answer
    at a return                        keep running; pause only for blocking decisions or explicit stop

A run can also go on in the background (`start()`): the agent keeps issuing actions (research, production,
construction ...) while days pass, and collects the result later (`join()`). Keyboard and mouse sections of the
loop take `session.ui_mutex`, the same lock acting tools hold, so a pause or an answered event never interleaves
with a half-finished click sequence.

Standing orders (state/standing.py) hook in through `on_reasons`: reasons they can serve (a free research slot, a
finished focus, free factories) are answered by the engine without ending the run.
"""

import threading
import time

from ..errors import ToolError
from ..perception import popups as popups_mod
from ..perception import topbar
from ..state import advance as adv
from ..state import focustimer
from . import guard
from . import popups as game_popups
from . import timectl

POLL = 0.3
# QA retains the legacy estimated timer. Campaign completion uses closed
# catalog-bound telemetry batches; elapsed duration never proves completion.
FOCUS_TIMER_TRIGGER = True
LOOK_EVERY = 1.5
NO_PROGRESS_S = 120


def _records_since(store, tag, last_n):
    """Read a stable queue snapshot while the telemetry producer keeps appending."""
    history = store.history.get(tag)
    if history is None:
        return []
    # deque.copy() copies the queue before Python record predicates run. An
    # append during those predicates belongs to the next poll, not this iterator.
    snapshot = history.copy()
    if not snapshot or snapshot[-1].get("n", -1) <= last_n:
        return []
    return [record for record in snapshot if record.get("n", -1) > last_n]


def focus_completion_event(previous, current):
    """Return a completion transition only inside one observed own-tree timeline."""
    if not isinstance(previous, dict) or not isinstance(current, dict):
        return None
    for key in ("tag", "tree", "generation"):
        if not isinstance(current.get(key), str) or not current[key] or previous.get(key) != current[key]:
            return None
    if (type(previous.get("day")) is not int or type(current.get("day")) is not int
            or previous["day"] >= current["day"]
            or previous.get("source") != "telemetry" or current.get("source") != "telemetry"):
        return None
    old, new = previous.get("completed"), current.get("completed")
    if (not isinstance(old, list) or not isinstance(new, list)
            or any(not isinstance(fid, str) or not fid for fid in old + new)
            or not set(old).issubset(new)):
        return None
    completed = sorted(set(new) - set(old))
    if completed:
        return {"code": "focus_done", "text": "completed focuses: " + ", ".join(completed),
                "focus_ids": completed, "source": "telemetry", "as_of_day": current["day"]}
    return None


def icon_state(session, img=None):
    """'running' | 'paused' | 'unknown' from the speed icon (pixels only)."""
    img = session.grab() if img is None else img
    return topbar.pause_state(img)[0]


def set_running(session, want_running, tries=3, settle=4.0):
    """Space until the speed icon shows the wanted state. True when it does."""
    want = "running" if want_running else "paused"
    for _ in range(tries):
        state = icon_state(session)
        # A transition frame can obscure the icon. Resample briefly without
        # input; only a known opposite state may authorize a Space toggle.
        for _sample in range(2):
            if state in ("running", "paused"):
                break
            time.sleep(0.1)
            state = icon_state(session)
        if state == want:
            return True
        if state not in ("running", "paused"):
            return False  # An obscured clock cannot authorize a toggle.
        session.input.key("space")
        t0 = time.monotonic()
        while time.monotonic() - t0 < settle:
            time.sleep(0.25)
            if icon_state(session) == want:
                return True
    return icon_state(session) == want


def ensure_speed(session, speed):
    img = session.grab()
    if speed and topbar.speed_level(img) != speed:
        timectl.set_speed(session, speed)


class AdvanceRun:
    """One advance: the arguments of hoi4_advance, the loop, and its result."""

    def __init__(self, session, args, policy, on_reasons=None, mutex=None, on_day=None):
        self.s = session
        self.args = args
        self.policy = policy
        self.on_reasons = on_reasons
        self.on_day = on_day
        self.mutex = mutex or session.ui_mutex
        self.cancel = threading.Event()
        self.thread = None
        self.done = threading.Event()
        self.result = None
        self.error = None
        self.t_start = time.monotonic()
        self.progress = {"state": "starting", "date": None, "days": 0}
        self.run = {"answered": [], "left_open": [], "notices": [], "reasons": [], "auto": []}
        self.pause_events = []
        self._pause_started = None
        self._started_monotonic = time.monotonic()

    def _pause_for_decision(self, reason):
        """Only a confirmed transition counts as an agent-induced pause."""
        if icon_state(self.s) == "running" and set_running(self.s, False):
            self._pause_started = time.monotonic()
            self.pause_events.append({"reason": reason, "started_after_s":
                                      round(self._pause_started - self._started_monotonic, 3)})

    # ------------------------------------------------------------------ control
    def start(self):
        self.thread = threading.Thread(target=self._thread, name="advance", daemon=True)
        self.thread.start()
        return self

    def _thread(self):
        try:
            self.result = self.execute()
        except BaseException as e:           # reported on join
            self.error = e
        finally:
            self.done.set()

    def stop(self):
        self.cancel.set()

    def join(self, timeout=None):
        self.done.wait(timeout)
        return self.done.is_set()

    # ------------------------------------------------------------------ the loop
    def execute(self):
        # A background run outlives the dispatcher call which started it. Keep the
        # desktop lease until it ends, including while no UI actions are needed.
        with self.s.lock.hold("background campaign clock"):
            stop_on = self.args.get("stop_on")
            stop_on = adv.DEFAULT_STOP if stop_on is None else stop_on
            if (getattr(self.s, "mode", None) == "campaign"
                    and "focus_idle" in set(stop_on) | self._standing_codes()):
                raise ToolError("focus_idle needs a verified UI observation; use the focus governor's ordinary preflight")
            try:
                return self._execute()
            except BaseException:
                self.progress.update(state="failed")
                try:
                    if self.s.lock.held_by_us():
                        with self.mutex:
                            self._pause_for_decision("clock_error")
                except Exception:
                    pass
                raise
            finally:
                governor = getattr(self.s, "governor", None)
                if governor is not None:
                    governor.on_stop(self, self.run["reasons"] or
                                     [{"code": "governor_error", "text": "clock ended unexpectedly"}])

    def _execute(self):
        s, a = self.s, self.args
        store = s.telemetry
        tag = store.player()
        rec0 = store.day(tag) if tag else None
        if not rec0 and not a.get("start_date"):
            raise ToolError("the fast clock needs the telemetry mod's day records (hoi4_telemetry action=enable, "
                            "restart the game, let one day pass)", {"field": "telemetry"})
        stop_on = list(a["stop_on"]) if a.get("stop_on") is not None else list(adv.DEFAULT_STOP)
        thresholds = a.get("thresholds") or {}
        campaign = getattr(s, "mode", None) == "campaign"
        last_focus_batch = store.focus_snapshot(tag, rec0["n"] - 1) if campaign and rec0 else None
        # no record yet (a new game, or a loaded save): the first day record that arrives is the baseline
        n0 = rec0["n"] if rec0 else None
        budget = a["budget_days"]
        first = prev = adv.watch(rec0) if rec0 else None
        self.first = first
        last_n = n0 if n0 is not None else -1
        run = self.run
        run["start_date"] = a.get("start_date") or rec0["_date"]
        t0 = time.monotonic()
        speed = a.get("speed") or 5
        with self.mutex:
            ensure_speed(s, speed)
            if not set_running(s, True):
                raise ToolError("could not start the game clock (speed icon did not change)")
        last_day_wall = time.monotonic()
        last_look = 0.0
        last_rec = None
        last_idle = 0
        last_idle_n = -10
        last_focus_idle_n = -10
        paused_by_us = False
        handled_pause = 0
        last_heartbeat = time.monotonic()
        self.progress.update(state="running", date=run["start_date"])
        while True:
            if self.cancel.is_set():
                run["reasons"] = [{"code": "cancelled", "text": "stopped by the agent"}]
                break
            time.sleep(POLL)
            if time.monotonic() - last_heartbeat >= 15:
                if not s.lock.held_by_us():
                    raise ToolError("desktop lease lost; campaign clock stopped")
                s.lock.refresh()
                last_heartbeat = time.monotonic()
            store = s.telemetry
            tag = tag or store.player()
            new = _records_since(store, tag, last_n)
            reasons = []
            wanted = set(stop_on) | self._standing_codes()
            for rec in new:
                if n0 is None:
                    n0 = rec["n"]
                    first = prev = adv.watch(rec)
                    self.first = first
                    last_rec = rec
                    last_n = rec["n"]
                    last_day_wall = time.monotonic()
                    continue
                cur = adv.watch(rec)
                reasons += adv.evaluate(prev, cur, wanted, thresholds, ())
                prev = cur
                if "research_idle" in wanted:
                    idle = self._idle_slots(last_rec if last_rec is not None else rec0)
                    # again every other day while slots stay idle: a slot filled today shows as busy tomorrow
                    if idle > 0 and (idle > last_idle or rec["n"] - last_idle_n >= 2):
                        reasons.append({"code": "research_idle",
                                        "text": f"{idle} research slot(s) idle ({rec['_date']})"})
                        last_idle_n = rec["n"]
                    last_idle = idle
                if campaign:
                    batch = store.focus_snapshot(tag, rec["n"] - 1)
                    event = focus_completion_event(last_focus_batch, batch)
                    if event and "focus_done" in wanted:
                        reasons.append(event)
                    if batch is not None:
                        last_focus_batch = batch
                if not campaign and "focus_done" in wanted and FOCUS_TIMER_TRIGGER:
                    f = focustimer.due(self.s, rec["n"])
                    if f:
                        reasons.append({"code": "focus_done", "text": f"focus {f.get('name') or f.get('focus')} is "
                                                                      f"complete ({rec['_date']})"})
                if not campaign and "focus_idle" in wanted:
                    f = focustimer.get(self.s)
                    idle = f is None or f.get("fired") or f.get("due_n") is None or rec["n"] >= f["due_n"]
                    if idle and rec["n"] - last_focus_idle_n >= 5:
                        reasons.append({"code": "focus_idle", "text": f"no national focus is running ({rec['_date']})"})
                        last_focus_idle_n = rec["n"]
                last_rec = rec
                last_n = rec["n"]
                last_day_wall = time.monotonic()
                handled_pause = 0  # count consecutive unexplained pauses only
            queued = guard.drain(s)                    # notices closed by the guard before an action
            if queued:
                titles = [q["title"] for q in queued]
                for ttl in titles:
                    run["notices"].append({"title": ttl, "date": self.progress.get("date")})
                reasons += adv.evaluate(None, None, wanted - {"focus_done"} if campaign else wanted, thresholds, titles)
            if new and n0 is not None:
                self.progress.update(date=new[-1]["_date"], days=last_n - n0, n=last_n)
                if self.on_day:
                    # Reconcile once against the newest state, never replay old
                    # snapshots after a slow UI action has advanced several days.
                    reasons += self.on_day(self, new[-1]) or []
            if reasons and self.on_reasons:
                reasons = self.on_reasons(self, reasons) or []
            reasons = [r for r in reasons if r["code"] in stop_on
                       or r["code"].startswith(("standing_", "governor_"))]
            if reasons:
                run["reasons"] = reasons
                break
            if n0 is not None and last_n - n0 >= budget:
                run["reasons"] = [{"code": "done", "text": f"{budget} day(s) run" +
                                   (f" to {a['until']}" if a.get("until") else "")}]
                break
            if time.monotonic() - t0 > a["max_seconds"]:
                run["reasons"] = [{"code": "time_limit",
                                   "text": f"the {a['max_seconds']} s wall-clock limit was reached"}]
                break
            if time.monotonic() - last_look < LOOK_EVERY:
                continue
            last_look = time.monotonic()
            with self.mutex:
                img = s.grab(focus=False)
            found = popups_mod.detect(img) if self.policy.mode != "ignore" or self.policy.rules else []
            if found:
                with self.mutex:
                    # Declared choices are ordinary actions while time runs.
                    # Read/answer first; only an unresolved decision needs a pause.
                    recs = game_popups.handle_all(s, self.policy, phase="running")
                for p in recs:
                    if p.get("answered"):
                        run["answered"].append({k: p.get(k) for k in ("kind", "title", "chosen_text", "why", "date")})
                    else:
                        run["left_open"].append(p)
                if run["left_open"]:
                    run["reasons"] = [{"code": "popup", "text": f"{len(run['left_open'])} event(s) wait for an answer"}]
                    with self.mutex:
                        self._pause_for_decision("popup")
                    paused_by_us = True
                    break
                with self.mutex:
                    set_running(s, True)
                last_day_wall = time.monotonic()
                continue
            state = icon_state(s, img)
            if state == "paused":
                # the game paused itself: a notification window (focus completed ...) or nothing at all
                with self.mutex:
                    from . import panels
                    titles = panels.dismiss_notices(s)
                    for t in titles:
                        run["notices"].append({"title": t, "date": self.progress.get("date")})
                    if titles:
                        handled_pause = 0
                        r = adv.evaluate(prev, prev, set(stop_on) - {"focus_done"} if campaign else stop_on, thresholds, titles)
                        if r and self.on_reasons:
                            r = self.on_reasons(self, r) or []
                        if r:
                            run["reasons"] = r
                            # A routine completion notice must not leave the
                            # simulation paused while the model reads its result.
                            set_running(s, True)
                            break
                    if not titles:
                        handled_pause += 1
                    if handled_pause > 6:
                        run["reasons"] = [{"code": "paused_by_game",
                                           "text": "the clock keeps stopping and nothing on screen explains it"}]
                        paused_by_us = True
                        break
                    set_running(s, True)
                continue
            if state == "unknown":
                # no top bar: a full-screen window. The peace conference is one we can answer.
                from . import peaceview
                if peaceview.is_open(s, img):
                    run["reasons"] = [{"code": "peace_conference",
                                       "text": "a peace conference is open (hoi4_peace status / take)"}]
                    paused_by_us = True
                    break
            if time.monotonic() - last_day_wall > NO_PROGRESS_S:
                run["reasons"] = [{"code": "no_progress", "text": f"no game day passed in {NO_PROGRESS_S} s"}]
                break
        # A returned observation or deadline is not a reason to pause the game.
        blocking = {"cancelled", "popup", "governor_decision", "governor_error",
                    "standing_failed", "standing_queue_empty", "no_progress"}
        codes = [r["code"] for r in run["reasons"]]
        if not paused_by_us and any(code in blocking for code in codes):
            with self.mutex:
                self._pause_for_decision(next(code for code in codes if code in blocking))
        self.progress.update(state="stopped")
        return self._result(first, prev, tag)

    def _standing_codes(self):
        governor = getattr(self.s, "governor", None)
        if governor is not None and governor.active():
            return set()
        from ..state import standing
        out = set()
        for r in standing.get(self.s).rules.values():
            out.update(r["when"])
        return out

    def _idle_slots(self, rec):
        """Idle research slots on the day of `rec`: slots minus busy ones (needs the rs lines of that day, which
        are complete once the next day record has arrived)."""
        slots = (rec or {}).get("rslots")
        if not isinstance(slots, (int, float)):
            return 0
        return max(0, int(slots) - self.s.telemetry.busy_slots(rec["n"]))

    # ------------------------------------------------------------------ result
    def _result(self, first, prev, tag):
        s, a, run = self.s, self.args, self.run
        store = s.telemetry
        tag = tag or store.player()
        cur = adv.watch(store.day(tag)) if tag else None
        end_date = (cur or {}).get("date") or self.progress.get("date")
        start_date = run["start_date"]
        n0 = first["n"] if first else None
        days_run = (cur["n"] - n0) if cur and n0 is not None else None
        changes = adv.diff(first, cur)
        attention = adv.telemetry_attention(cur)
        why = "; ".join(r["text"] for r in run["reasons"][:3])
        code = run["reasons"][0]["code"] if run["reasons"] else "done"
        summary = f"advanced {start_date} -> {end_date} ({days_run if days_run is not None else '?'} days): {code}: {why}"
        data = {"start": start_date, "end": end_date, "days_run": days_run, "stopped_by": run["reasons"],
                "changes": changes, "answered": run["answered"], "events_waiting": run["left_open"],
                "notices": run["notices"], "attention": attention, "suggestions": adv.suggestions(attention),
                "stop_on": a.get("stop_on_used"), "popup_policy": self.policy.to_dict(),
                "wall_s": round(time.monotonic() - self._started_monotonic, 1),
                "agent_pause_count": len(self.pause_events),
                "agent_pause_events": list(self.pause_events),
                "agent_pause_seconds_in_run": round(time.monotonic() - self._pause_started, 3)
                    if self._pause_started is not None else 0.0,
                "agent_pause_ongoing": self._pause_started is not None}
        data["wall_seconds_per_game_year"] = (round(data["wall_s"] * 365 / days_run, 2)
                                             if days_run and days_run > 0 else None)
        if run["auto"]:
            data["standing_orders_done"] = run["auto"]
        return {"summary": summary, "data": data, "end_date": end_date, "start_date": start_date, "tag": tag,
                "changes": changes, "days": days_run, "reasons": run["reasons"], "answered": run["answered"]}
