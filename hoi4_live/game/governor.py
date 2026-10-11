"""Owned-game governor integration with the existing fast clock.

The telemetry adapter uses a completed own-country daily batch, not the newest
partially emitted research records. No saves, process attachment, commands or UI
are invented here: actions dispatch the registered campaign tools. Explicit
events use the clock's popup policy. Other plan components are refused until
their required live observations exist. A loaded durable plan never auto-starts.
"""

import copy
import json
import os
import threading
import time

from ..errors import ToolError
from ..state.governor import StandingPlan, digest, wait_matches


STRATEGIC_STOP = ["war_started", "war_ended", "territory_lost", "losing"]
UNSUPPORTED = ("focus", "construction", "production", "army")


def _require_owned(session):
    if session.mode != "campaign":
        raise ToolError("the governor is available only in campaign mode")
    session.require_owned_game()
    if not session.lock.held_by_us() or not getattr(session.lock, "sticky", False):
        raise ToolError("governor requires our sticky desktop lease (hoi4_lock action=acquire)")


def _integer(value):
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else None


def _ids(rows, catalog, tag):
    """Decode only this player's complete, valid technology records."""
    result = set()
    for row in rows:
        if not isinstance(row, dict) or row.get("t") != tag:
            return None
        index = _integer(row.get("tid"))
        if index is None or index >= len(catalog):
            return None
        result.add(catalog[index])
    return result


def telemetry_snapshot(session, tag=None):
    """Private runtime adapter. The caller has already verified ownership/lease.

    A later day is the completion marker for rs and weekly kt scans. Missing
    research state stays unknown unless the schema/catalog/generation and a
    single human-country stream establish that an empty completed batch means
    zero busy slots. Untagged rs_counts is never used in a mixed-country stream.
    """
    from ..gamedata import tech as tech_data
    from ..telemetry.modgen import SCHEMA
    _require_owned(session)
    store = session.telemetry
    tag = tag or store.player()
    histories = getattr(store, "history", {})
    history = list(histories.get(tag) or ())
    if len(history) < 2:
        raise ToolError("governor needs two telemetry day records to confirm a completed daily batch")
    latest = history[-1]
    complete = history[-2]
    n, latest_n = _integer(complete.get("n")), _integer(latest.get("n"))
    if n is None or latest_n is None or n >= latest_n or latest.get("t") != tag or complete.get("t") != tag:
        raise ToolError("governor telemetry timeline is incomplete or ambiguous")
    snapshot = {"day": latest_n - 1, "latest_day": latest_n, "completed_record_day": n,
                "date": complete.get("_date"), "tag": tag,
                "source": "telemetry", "as_of_day": n, "metrics": {}}
    for target, source in (("idle_military_factories", "mil_free"), ("idle_civilian_factories", "civ_free")):
        value = _integer(complete.get(source))
        if value is not None:
            snapshot["metrics"][target] = value
    if n != latest_n - 1:
        snapshot["research_unknown"] = "latest completed daily batch is missing; older fields are stale"
        return snapshot
    # Economy equipment/resource records do not contain all requested deficit
    # semantics; absence remains null in the reconciler rather than invented 0.
    catalog = getattr(store, "techs", None)
    generation = latest.get("g")
    hello = store.get("hello", tag)
    own_stream = all(t == tag or not rows for t, rows in histories.items())
    groups = getattr(store, "groups", {})
    rs_group = groups.get("rs")
    tagged_rs = not rs_group or all(isinstance(r, dict) and r.get("t") == tag for r in rs_group.get("rows", []))
    validated_stream = (own_stream and tagged_rs and isinstance(catalog, list) and bool(catalog)
                        and isinstance(generation, str) and bool(generation) and complete.get("g") == generation
                        and latest.get("v") == SCHEMA and complete.get("v") == SCHEMA
                        and isinstance(hello, dict) and hello.get("g") == generation and hello.get("t") == tag
                        and hello.get("v") == SCHEMA)
    if not validated_stream:
        snapshot["research_unknown"] = "catalog, generation or own-country stream is unverified"
        return snapshot
    active_raw = getattr(store, "rs_counts", {}).get(n)
    # Completed schema batches emit one rs per busy slot. Only the validated
    # single-country stream makes absence a proven empty list.
    active = sorted(active_raw) if active_raw is not None else []
    if any(not isinstance(t, str) or t not in catalog for t in active):
        snapshot["research_unknown"] = "research technology IDs are unresolved"
        return snapshot
    slots = _integer(complete.get("rslots"))
    if slots is None or len(active) > slots:
        snapshot["research_unknown"] = "research slot count is unavailable or inconsistent"
        return snapshot
    research = {"idle": slots - len(active), "active": active}
    snapshot["research"] = research
    kt = groups.get("kt")
    known = None
    scan_day = None
    count = _integer(complete.get("techs"))
    if count == 0:
        known, scan_day = set(), n
    elif isinstance(kt, dict):
        scan_day = _integer(kt.get("n"))
        if scan_day is not None and scan_day <= n and n - scan_day <= 8:
            known = _ids(kt.get("rows", []), catalog, tag)
            if known is not None and len(known) != count:
                known = None
    if known is None:
        snapshot["research_unknown"] = "known technology scan incomplete, stale or count mismatch"
        return snapshot
    research.update(completed=sorted(known), known_as_of_day=scan_day)
    idx = tech_data.for_session(session)
    _require_owned(session)
    width, height = session.window.size()
    try:
        year = int(str(complete.get("_date")).split(".")[0])
    except (ValueError, TypeError):
        snapshot["research_unknown"] = "telemetry year is unknown"
        return snapshot
    # Folder script conditions and technology allow/allow_branch are unknown.
    # The current UI action needs a visible tile, so no offscreen guess is sent.
    folders = [name for name, folder in idx.get("folders", {}).items()
               if "doctrine" not in name and not folder.get("available")
               and idx.get("layout", {}).get(name, {}).get("gridboxes")]
    available = []
    for row in tech_data.plan(idx, known, active, year, folders=folders):
        tech = idx.get("techs", {}).get(row["id"], {})
        if row.get("folder") not in folders or row.get("ahead_years") or tech.get("allow") or tech.get("allow_branch") or tech.get("doctrine"):
            continue
        offset = tech_data.tile_offset(idx, row["id"], row["folder"], width)
        if offset and 0 <= offset[0] + 12 <= width - 90 and 0 <= offset[1] + 138 <= height - 90:
            available.append(row["id"])
    research["available"] = available
    return snapshot


class Governor:
    def __init__(self, session, path=None):
        self.session = session
        self.path = path
        self.condition = threading.Condition(threading.RLock())
        self.policy = None
        self.snapshot = None
        self.result = None
        self._active = False
        self._revision = 0
        self._tag = None
        self._timeline = None
        self._run = None
        self.last_stop = None
        self.persistence_error = None
        self._load()

    def _campaign(self):
        if self.session.mode != "campaign":
            raise ToolError("the governor is available only in campaign mode")

    def _guard(self):
        _require_owned(self.session)

    def active(self):
        with self.condition:
            return self._active

    def _load(self):
        if not self.path:
            return
        try:
            with open(self.path, encoding="utf-8") as handle:
                data = json.load(handle)
            self.policy = StandingPlan(data["plan"], data.get("state"))
            self._timeline = data.get("timeline")
        except FileNotFoundError:
            pass
        except (OSError, ValueError, KeyError, TypeError, ToolError) as exc:
            self.persistence_error = str(exc)

    def _save(self):
        if not self.path or self.policy is None:
            return
        try:
            os.makedirs(os.path.dirname(os.path.abspath(self.path)), exist_ok=True)
            temporary = self.path + ".tmp"
            with open(temporary, "w", encoding="utf-8") as handle:
                json.dump({"version": 1, "plan": self.policy.plan, "state": self.policy.state,
                           "timeline": self._timeline, "active": False}, handle, ensure_ascii=False, indent=1)
            os.replace(temporary, self.path)
            self.persistence_error = None
        except (OSError, TypeError, ValueError) as exc:
            self.persistence_error = str(exc)

    def set_plan(self, plan):
        self._campaign()
        proposed = StandingPlan(plan)
        with self.session.ui_mutex, self.condition:
            if self._active:
                self._guard()
                self._supported(proposed)
            self.policy = proposed
            self._revision += 1
            if self._active and self._run is not None:
                from .popups import PopupPolicy
                event_policy = proposed.plan["events"]
                self._run.policy = PopupPolicy.parse(event_policy["mode"], event_policy["rules"])
            self._save()
            self.condition.notify_all()
            return self.status()

    @staticmethod
    def _supported(policy):
        unsupported = [name for name in UNSUPPORTED if policy.plan[name]]
        if unsupported:
            raise ToolError("live governor state is not implemented for: " + ", ".join(unsupported),
                            {"unsupported_state": unsupported, "supported": ["research", "events"]})

    def _read(self):
        self._guard()
        snapshot = telemetry_snapshot(self.session, self._tag)
        if self._tag is not None and self.session.telemetry.player() != self._tag:
            raise ToolError("player country changed; declare a new governor run")
        return snapshot

    def start(self, days=90, speed=5, max_seconds=900):
        from ..tools import dispatch
        with self.session.ui_mutex:
            self._guard()
            with self.condition:
                if self._active:
                    raise ToolError("governor is already active")
                if self.policy is None:
                    raise ToolError("declare a standing plan before starting the governor")
                self._supported(self.policy)
            clock = getattr(self.session, "clock", None)
            if clock is not None and not clock.done.is_set():
                raise ToolError("collect or stop the existing background advance first")
            from ..state import standing
            if standing.get(self.session).active():
                raise ToolError("clear legacy standing orders before starting the governor; the two policies conflict")
            snapshot = self._read()
            if self.policy.plan["research"] and "available" not in snapshot.get("research", {}):
                raise ToolError("research governor needs a complete own-country technology scan", {"reason": snapshot.get("research_unknown")})
            store = self.session.telemetry
            timeline = {"tag": snapshot["tag"], "generation": store.generation,
                        "restarts": store.restarts, "rewinds": store.rewinds}
            with self.condition:
                if self._timeline != timeline or (self.policy.state["last_day"] is not None and snapshot["day"] < self.policy.state["last_day"]):
                    self.policy = StandingPlan(self.policy.plan)
                self._timeline = timeline
                self._tag = snapshot["tag"]
                self.snapshot = snapshot
                self.result = self.policy.reconcile(snapshot)
                self.last_stop = None
                self._active = True
                self._revision += 1
                event_policy = self.policy.plan["events"]
                args = {"engine": "fast", "background": True, "days": days, "speed": speed,
                        "max_seconds": max_seconds, "stop_on": STRATEGIC_STOP,
                        "popups": event_policy["mode"], "popup_rules": event_policy["rules"]}
            result = dispatch.call(self.session, "hoi4_advance", args)
            with self.condition:
                if result.get("isError"):
                    self._active = False
                    self.last_stop = [{"code": "governor_start_failed", "text": "background advance failed"}]
                else:
                    self._run = self.session.clock
                self._save()
                self.condition.notify_all()
            return result

    def on_day(self, run, latest_day_record):
        """Fast-clock hook. Ignore the callback's data for observation sourcing."""
        from ..tools import dispatch
        with self.condition:
            if not self._active:
                return []
            revision = self._revision
            self._run = run
        try:
            snapshot = self._read()
            store = self.session.telemetry
            timeline = {"tag": snapshot["tag"], "generation": store.generation,
                        "restarts": store.restarts, "rewinds": store.rewinds}
            if timeline != self._timeline:
                raise ToolError("telemetry timeline changed; governor stopped")
            with self.condition:
                if revision != self._revision or not self._active:
                    return []
                self.snapshot = snapshot
                self.result = self.policy.reconcile(snapshot)
                actions = list(self.result["actions"])
                reasons = [{"code": "governor_" + reason.split(":", 1)[0], "text": reason}
                           for reason in self.result["stop_reasons"]]
                self.condition.notify_all()
            for action in actions:
                # Only one research action is currently proposed. This also
                # prevents stale replay when future supported tools take days.
                with self.session.ui_mutex:
                    self._guard()
                    with self.condition:
                        if revision != self._revision or not self._active or run.cancel.is_set():
                            break
                    # Popup answers belong to the explicitly declared clock
                    # policy. Let its normal popup handler run before a repair.
                    from ..perception import popups
                    if popups.detect(self.session.grab(focus=False)):
                        break
                    args = dict(action["args"])
                    if action["tool"] == "hoi4_research":
                        args["save"] = "telemetry"
                    outcome = dispatch.call(self.session, action["tool"], args)
                    self._guard()
                    latest = self.session.telemetry.day(self._tag)
                    actual_day = _integer((latest or {}).get("n"))
                    if actual_day is None:
                        raise ToolError("cannot date governor tool outcome")
                    with self.condition:
                        self.policy.record(action["id"], outcome, max(actual_day, action["day"]))
                    # A UI call may consume several game days. Replace its
                    # cached proposal with newly dated telemetry immediately;
                    # no second action is executed from this refreshed batch.
                    refreshed = self._read()
                    with self.condition:
                        self.snapshot = refreshed
                        self.result = self.policy.reconcile(refreshed)
                        self._save()
                        self.condition.notify_all()
                    run.run.setdefault("auto", []).append({"governor": True, "tool": action["tool"],
                        "args": args, "ok": not outcome.get("isError"), "day": actual_day})
                    # Any slow action invalidates the rest of this batch.
                    break
            with self.condition:
                self._save()
            if reasons:
                self.on_stop(run, reasons)
            return reasons
        except ToolError as exc:
            reasons = [{"code": "governor_stopped", "text": str(exc)}]
            self.on_stop(run, reasons)
            return reasons

    def on_stop(self, run, reasons=None):
        """Clock completion/error hook; never accesses the game after lease loss."""
        with self.condition:
            if self._run is not None and run is not self._run:
                return
            self._active = False
            self.last_stop = copy.deepcopy(reasons or [{"code": "done", "text": "clock stopped"}])
            if self.snapshot is not None and getattr(run, "run", {}).get("left_open"):
                self.snapshot["events"] = {"source": "clock-popup", "as_of_day": self.snapshot["day"],
                    "open": [{"id": str(index), "index": index, "title": item.get("title"),
                              "text": item.get("text"), "options": item.get("options", [])}
                             for index, item in enumerate(run.run["left_open"])]}
            self._save()
            self.condition.notify_all()

    def stop(self):
        self._campaign()
        with self.condition:
            self._active = False
            self.last_stop = [{"code": "governor_cancelled", "text": "governor stopped by caller"}]
            run = self._run
            if run is not None:
                run.stop()
            self._save()
            self.condition.notify_all()
            return self.status()

    def status(self):
        self._campaign()
        with self.condition:
            return {"active": self._active, "configured": self.policy is not None,
                    "supported": ["research", "events"], "unsupported_state": list(UNSUPPORTED),
                    "plan": copy.deepcopy(self.policy.plan) if self.policy else None,
                    "snapshot": copy.deepcopy(self.snapshot), "last_stop": copy.deepcopy(self.last_stop),
                    "report": self.policy.report() if self.policy else None,
                    "persistence_error": self.persistence_error}

    def compact_digest(self):
        self._campaign()
        with self.condition:
            result = self.result or {"day": None, "metrics": {}, "alerts": [], "stop_reasons": ["not_observed"]}
            return digest(result, self.snapshot)

    def wait_for(self, predicates, timeout_s=60, cancel=None):
        """Block on daily/stop notifications; no UI, saves, or client polling."""
        self._campaign()
        if isinstance(timeout_s, bool) or not isinstance(timeout_s, (int, float)) or not 0 <= timeout_s <= 60:
            raise ToolError("wait timeout must be 0..60 seconds")
        deadline = time.monotonic() + timeout_s
        with self.condition:
            baseline = copy.deepcopy(self.snapshot or {})
            # Validate shape before waiting, including when no day exists yet.
            wait_matches(baseline, predicates, baseline)
            while True:
                current = copy.deepcopy(self.snapshot or {})
                matched = wait_matches(current, predicates, baseline)
                if cancel is not None and cancel.is_set():
                    state = "cancelled"
                elif matched:
                    state = "matched"
                elif not self._active:
                    state = "stopped"
                elif time.monotonic() >= deadline:
                    state = "timeout"
                else:
                    # A bounded server cancellation check complements condition
                    # wakeups; clients make a single wait call.
                    self.condition.wait(min(0.25 if cancel is not None else 60, max(0, deadline - time.monotonic())))
                    continue
                return {"state": state, "matched": matched, "snapshot": current,
                        "stopped_by": copy.deepcopy(self.last_stop), "digest": self.compact_digest()}


def get(session):
    governor = getattr(session, "governor", None)
    if governor is None:
        governor = Governor(session, os.path.join(session.cfg.artifacts_dir, "state", "governor.json"))
        session.governor = governor
    return governor
