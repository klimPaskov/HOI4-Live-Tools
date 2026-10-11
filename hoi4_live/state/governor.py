"""Offline, deterministic standing-plan reconciliation; this module never executes tools.

Plan JSON (all lists retain their declared order)::

    {"version": 1, "research": ["basic_machine_tools"], "focus": ["GER_rhineland"],
     "construction": [{"building": "industrial_complex", "state": "64", "target": 4}],
     "production": [{"line": "1", "max": 8}],
     "army": [{"state": "64", "army": "Home"}],
     "events": {"mode": "stop", "rules": [{"match": "^News", "option": 0}]},
     "retry": {"max_attempts": 3, "backoff_days": 2}, "max_actions": 4,
     "max_age_days": 0}

Normalized observations have integer ``day`` (absolute game day), optional ``date``
and ``tag``, and sections with explicit ``source`` and ``as_of_day``. Sections can
inherit those two fields from the snapshot. Section schemas:

* research: idle (count), completed/active/available (lists of technology IDs).
* focus: idle (bool), completed/available (lists of focus IDs).
* metrics: idle_military_factories, idle_civilian_factories, unassigned_divisions,
  equipment_deficit, resource_deficit (nonnegative counts; absent means unknown).
* construction: items [{building,state,built,queued,available,free_slots}]. Targets
  mean total built + queued levels, never a consumable number of extra clicks.
* production: lines [{line,factories}]. Line identifiers must match the existing
  production tool's line number or equipment name; IDs of hidden game objects
  cannot be used here.
* army: unassigned_by_state {state:count}, armies [name], selection_safe_by_state
  {state:bool}. True certifies the tool's entire selectable stack is unassigned
  or already belongs to that target army. No division-level targeting.
* events: open [{id,index,title,text,options:[label]}]. Explicit policy only.
* shortages: observed equipment [{id,army_deficit,...}] and resources
  [{id,balance,deficit}], with scoped army_deficit/resource_deficit totals.
  These observed-category totals do not establish complete national deficits.

Unknown availability never authorizes a choice. No enemy state is inferred.
``reconcile`` proposes bounded calls; the owner executes under its own locks and
ownership checks, then calls ``record(id, MCP_result, day)``. Success waits for a
later fresh observation before another attempt. Failures never consume a plan.
This is a reducer and API foundation, not automatic runtime integration.
"""

import copy
import hashlib
import json
import math
import re

from ..errors import ToolError
from ..protocol.schema import validate


METRICS = ("idle_research_slots", "idle_military_factories", "idle_civilian_factories",
           "unassigned_divisions", "equipment_deficit", "resource_deficit")
KINDS = ("events", "research", "focus", "construction", "production", "army")
OBSERVATIONS = KINDS + ("shortages",)


def _number(value):
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        return None
    try:
        return value if math.isfinite(value) else None
    except OverflowError:
        return None


def _day(value):
    return value if isinstance(value, int) and not isinstance(value, bool) and value >= 0 else None


def fresh_section(snapshot, name, max_age_days=0):
    """Return a provenance-bearing observation, or None (including future data)."""
    day = _day(snapshot.get("day"))
    section = snapshot.get(name)
    if not isinstance(section, dict) or day is None:
        return None
    observed = _day(section.get("as_of_day", snapshot.get("as_of_day")))
    source = section.get("source", snapshot.get("source"))
    if not isinstance(source, str) or not source or observed is None or not 0 <= day - observed <= max_age_days:
        return None
    return section


def _call(tool, args):
    """Check every emitted suggestion against the actual registered tool schema."""
    from ..tools import registry
    entry = registry.get(tool)
    if entry is None or entry.test_only:
        raise ToolError("governor requires an existing campaign tool: " + tool)
    validate(entry.schema, args)
    return {"tool": tool, "args": copy.deepcopy(args)}


def _key(kind, value):
    encoded = json.dumps(value, sort_keys=True, ensure_ascii=True, separators=(",", ":"))
    return kind + ":" + hashlib.sha256(encoded.encode("utf-8")).hexdigest()[:16]


class StandingPlan:
    """In-memory deterministic policy state. Serialize ``state`` alongside ``plan`` if needed."""

    def __init__(self, plan, state=None):
        if not isinstance(plan, dict):
            raise ToolError("standing plan must be a JSON object")
        self.plan = copy.deepcopy(plan)
        self._validate()
        self.state = copy.deepcopy(state) if state is not None else {
            "attempts": {}, "first_day": None, "last_day": None, "observed_days": 0,
            "tool_failures": 0, "stop_reasons": {}, "metric_samples": {},
            "idle_research_slot_days": 0, "idle_military_factory_days": 0,
            "idle_civilian_factory_days": 0}
        self._proposed = {}

    def _validate(self):
        allowed = {"version", "research", "focus", "construction", "production", "army", "events",
                   "retry", "max_actions", "max_age_days"}
        if set(self.plan) - allowed or type(self.plan.get("version", 1)) is not int or self.plan.get("version", 1) != 1:
            raise ToolError("unknown standing plan fields or unsupported version")
        for name in ("research", "focus", "construction", "production", "army"):
            rows = self.plan.setdefault(name, [])
            if not isinstance(rows, list) or len(rows) > 256:
                raise ToolError(name + " must be a list of at most 256 choices")
        for name in ("research", "focus"):
            if any(not isinstance(x, str) or not x or len(x) > 200 for x in self.plan[name]):
                raise ToolError(name + " needs explicit nonempty IDs")
            if len(set(self.plan[name])) != len(self.plan[name]):
                raise ToolError(name + " contains duplicate IDs")
        for name, required in (("construction", ("building", "state", "target")),
                               ("production", ("line", "max")), ("army", ("state", "army"))):
            identities = set()
            for row in self.plan[name]:
                if not isinstance(row, dict) or set(row) != set(required):
                    raise ToolError(name + " rows require exactly " + ", ".join(required))
                for key in required:
                    if key in ("target", "max"):
                        if _day(row[key]) is None or not 1 <= row[key] <= 200:
                            raise ToolError(name + "." + key + " must be an integer 1..200")
                    elif not isinstance(row[key], str) or not row[key] or len(row[key]) > 200:
                        raise ToolError(name + "." + key + " must be a nonempty string")
                identity = tuple(row[k] for k in required if k not in ("max", "target", "army"))
                if identity in identities:
                    raise ToolError(name + " contains conflicting duplicate targets")
                identities.add(identity)
        self.plan.setdefault("max_actions", 4)
        self.plan.setdefault("max_age_days", 0)
        for key, lower, upper in (("max_actions", 1, 16), ("max_age_days", 0, 30)):
            if _day(self.plan[key]) is None or not lower <= self.plan[key] <= upper:
                raise ToolError(key + " is outside its bounded range")
        retry = self.plan.setdefault("retry", {})
        if not isinstance(retry, dict) or set(retry) - {"max_attempts", "backoff_days"}:
            raise ToolError("retry needs max_attempts and backoff_days")
        retry.setdefault("max_attempts", 3)
        retry.setdefault("backoff_days", 2)
        for key, upper in (("max_attempts", 10), ("backoff_days", 365)):
            if _day(retry[key]) is None or not 1 <= retry[key] <= upper:
                raise ToolError("retry." + key + " is outside its bounded range")
        events = self.plan.setdefault("events", {"mode": "stop", "rules": []})
        if not isinstance(events, dict) or set(events) - {"mode", "rules"}:
            raise ToolError("events needs mode and rules")
        events.setdefault("mode", "stop")
        events.setdefault("rules", [])
        if events["mode"] not in ("stop", "choices", "first") or not isinstance(events["rules"], list):
            raise ToolError("event mode is stop, choices or explicitly requested first")
        if len(events["rules"]) > 64:
            raise ToolError("at most 64 event rules")
        for rule in events["rules"]:
            if not isinstance(rule, dict) or set(rule) - {"match", "option", "stop"} or not isinstance(rule.get("match"), str):
                raise ToolError("event rule needs match and option or stop")
            try:
                re.compile(rule["match"], re.I)
            except re.error as exc:
                raise ToolError("invalid event match: " + str(exc))
            if rule.get("stop") is not True and (_day(rule.get("option")) is None):
                raise ToolError("event rule must explicitly stop or choose a zero-based option")

    def _eligible(self, ident, day, observed_day):
        record = self.state["attempts"].get(ident, {})
        if record.get("status") in ("succeeded", "skipped") and observed_day <= record["last_day"]:
            return False, None
        if record.get("count", 0) >= self.plan["retry"]["max_attempts"]:
            return False, "attempt_limit"
        if day < record.get("next_day", day):
            return False, None
        return True, None

    def record(self, action_id, result, day):
        """Record one executed proposal; failures/deferred never remove preferences.

        Exceptions must be converted by the executor to {isError:true}. Only
        registered proposals can be recorded, at most once per absolute day.
        """
        if action_id not in self._proposed or _day(day) is None or not isinstance(result, dict):
            raise ToolError("record needs a proposed action, MCP result and absolute game day")
        if day < self._proposed[action_id]["day"]:
            raise ToolError("cannot record a result before its proposed game day")
        previous = self.state["attempts"].get(action_id, {})
        if previous.get("last_day") is not None and day <= previous["last_day"]:
            raise ToolError("an action outcome can be recorded only once per game day")
        data = result.get("structuredContent") or {}
        failure = bool(result.get("isError")) or data.get("closed") is False
        deferred = bool(data.get("deferred"))
        status = "failed" if failure else "deferred" if deferred else "skipped" if data.get("skip") else "succeeded"
        count = previous.get("count", 0) + 1
        gap = self.plan["retry"]["backoff_days"] * (2 ** (count - 1)) if failure or deferred else 1
        self.state["attempts"][action_id] = {"count": count, "last_day": day,
                                             "next_day": day + gap, "status": status,
                                             "kind": self._proposed[action_id]["kind"],
                                             "before": self._proposed[action_id]["observation"]}
        if failure:
            self.state["tool_failures"] += 1
        return status

    def reconcile(self, snapshot):
        """Propose routine repairs from fresh own-country observations only."""
        snapshot = snapshot if isinstance(snapshot, dict) else {}
        day = _day(snapshot.get("day"))
        out = {"day": day, "actions": [], "alerts": [], "stop_reasons": [], "metrics": self.metrics(snapshot)}
        if day is None:
            out["stop_reasons"].append("day_unknown")
            return out
        if self.state["last_day"] is not None and day < self.state["last_day"]:
            out["stop_reasons"].append("day_regressed")
            return out
        fresh = {k: fresh_section(snapshot, k, self.plan["max_age_days"]) for k in KINDS}
        self._confirm(snapshot, fresh)
        candidates = []

        def propose(kind, identity, tool, args, reason):
            ident = _key(kind, identity)
            observation_day = fresh[kind].get("as_of_day", snapshot.get("as_of_day"))
            allowed, stop = self._eligible(ident, day, observation_day)
            if stop:
                out["stop_reasons"].append(stop + ":" + ident)
            call = _call(tool, args)
            alert = {"priority": 2, "code": reason, "fix": call}
            if allowed:
                candidates.append(dict(call, id=ident, reason=reason, day=day))
            else:
                alert["deferred"] = True
            out["alerts"].append(alert)

        events = fresh["events"]
        open_events = events.get("open") if events else None
        if isinstance(open_events, list) and open_events:
            # Answer one at a time: closing a popup changes every later UI index.
            event = open_events[0]
            options = event.get("options") if isinstance(event, dict) else None
            choice = None
            policy = self.plan["events"]
            if isinstance(options, list) and options:
                for rule in policy["rules"]:
                    if re.search(rule["match"], str(event.get("title", "")) + "\n" + str(event.get("text", "")), re.I):
                        choice = None if rule.get("stop") else rule["option"]
                        break
                else:
                    if policy["mode"] == "first" or (policy["mode"] == "choices" and len(options) == 1):
                        choice = 0
            if choice is not None and choice < len(options) and _day(event.get("index")) is not None:
                propose("events", event.get("id", event), "hoi4_popups",
                        {"action": "answer", "popup": event["index"], "option": choice}, "event_policy")
            else:
                out["stop_reasons"].append("event_decision")
                out["alerts"].append({"priority": 0, "code": "event_decision",
                                      "inspect": _call("hoi4_popups", {"action": "read"})})
            out["actions"] = candidates[:1]
            return self._finish(out, snapshot)

        for kind, idle_type, tool, action, field in (("research", "count", "hoi4_research", "start", "tech"),
                                                      ("focus", "bool", "hoi4_focus", "select", "focus")):
            observation = fresh[kind]
            idle = observation.get("idle") if observation else None
            can_act = (_number(idle) is not None and idle > 0) if idle_type == "count" else idle is True
            if not can_act or not self.plan[kind]:
                continue
            available = observation.get("available")
            completed, active = observation.get("completed"), observation.get("active")
            if not isinstance(available, list):
                out["alerts"].append({"priority": 1, "code": kind + "_availability_unknown",
                                      "inspect": _call(tool, {"action": "list"})})
                continue
            excluded = set(completed if isinstance(completed, list) else []) | set(active if isinstance(active, list) else [])
            for choice in self.plan[kind]:
                if choice in excluded or choice not in available:
                    continue
                args = {"action": action, field: choice}
                if kind == "focus":
                    args["if_idle"] = True
                propose(kind, choice, tool, args, kind + "_idle")
                # Research UI picks its first idle slot; refresh after one fill.
                break

        observation = fresh["construction"]
        rows = observation.get("items") if observation else None
        if isinstance(rows, list):
            for target in self.plan["construction"]:
                row = next((r for r in rows if isinstance(r, dict) and r.get("building") == target["building"] and str(r.get("state")) == target["state"]), None)
                if not row or _number(row.get("built")) is None or _number(row.get("queued")) is None:
                    continue
                if row["built"] < 0 or row["queued"] < 0:
                    continue
                missing = target["target"] - row["built"] - row["queued"]
                if missing <= 0:
                    self.state["attempts"].pop(_key("construction", target), None)
                    continue
                if row.get("available") is not True or _number(row.get("free_slots")) is None:
                    continue
                count = min(10, int(missing), int(row["free_slots"]))
                if count > 0:
                    propose("construction", target, "hoi4_construction", dict(action="build", building=target["building"],
                            state=target["state"], count=count), "construction_target")

        observation = fresh["production"]
        rows = observation.get("lines") if observation else None
        if isinstance(rows, list) and (_number(out["metrics"]["idle_military_factories"]) or 0) > 0:
            targets = [t for t in self.plan["production"] if any(isinstance(r, dict) and str(r.get("line")) == t["line"] and
                       _number(r.get("factories")) is not None and r["factories"] < t["max"] for r in rows)]
            if targets:
                propose("production", self.plan["production"], "hoi4_production", {"action": "fill", "targets": targets}, "production_idle")

        observation = fresh["army"]
        states, armies = (observation.get("unassigned_by_state"), observation.get("armies")) if observation else (None, None)
        safe_states = observation.get("selection_safe_by_state") if observation else None
        if isinstance(states, dict) and isinstance(armies, list):
            for target in self.plan["army"]:
                if target["army"] in armies and (_number(states.get(target["state"])) or 0) > 0:
                    if not isinstance(safe_states, dict) or safe_states.get(target["state"]) is not True:
                        out["alerts"].append({"priority": 1, "code": "army_selection_unknown",
                                              "inspect": _call("hoi4_army", {"action": "list"})})
                        continue
                    propose("army", target, "hoi4_army", dict(action="assign", state=target["state"], army=target["army"], zoom="province"), "unassigned_divisions")

        for metric in ("equipment_deficit", "resource_deficit"):
            if (_number(out["metrics"][metric]) or 0) > 0:
                out["alerts"].append({"priority": 1, "code": metric, "value": out["metrics"][metric],
                                      "inspect": _call("hoi4_state", {"sections": ["economy", "production"], "screen": True})})
        shortages = fresh_section(snapshot, "shortages", self.plan["max_age_days"])
        if shortages:
            for section, field, code in (("equipment", "army_deficit", "observed_army_equipment_deficit"),
                                         ("resources", "deficit", "observed_resource_deficit")):
                rows = shortages.get(section)
                if not isinstance(rows, list):
                    continue
                for row in rows:
                    if not isinstance(row, dict) or not isinstance(row.get("id"), str):
                        continue
                    value = _number(row.get(field))
                    if value is not None and value > 0:
                        out["alerts"].append({"priority": 1, "code": code, "id": row["id"], "value": value,
                                              "inspect": _call("hoi4_state", {"sections": ["economy", "production"], "screen": True})})
        out["actions"] = candidates[:self.plan["max_actions"]]
        return self._finish(out, snapshot)

    def metrics(self, snapshot):
        """Unknown and stale values are null, never silently counted as zero."""
        observation = fresh_section(snapshot, "metrics", self.plan["max_age_days"])
        research = fresh_section(snapshot, "research", self.plan["max_age_days"])
        out = {name: None for name in METRICS}
        if observation:
            for name in METRICS[1:]:
                value = _number(observation.get(name))
                out[name] = value if value is not None and value >= 0 else None
        if research:
            idle = _number(research.get("idle"))
            out["idle_research_slots"] = idle if idle is not None and idle >= 0 else None
        return out

    def _finish(self, out, snapshot):
        day = out["day"]
        for action in out["actions"]:
            kind = action["id"].split(":", 1)[0]
            self._proposed[action["id"]] = dict(action, kind=kind,
                observation=copy.deepcopy(fresh_section(snapshot, kind, self.plan["max_age_days"])))
        out["stop_reasons"] = sorted(set(out["stop_reasons"]))
        out["alerts"].sort(key=lambda alert: (alert["priority"], alert["code"]))
        if self.state["last_day"] != day:
            if self.state["first_day"] is None:
                self.state["first_day"] = day
            self.state["last_day"] = day
            self.state["observed_days"] += 1
            sections = self.state.setdefault("section_samples", {})
            for kind in OBSERVATIONS:
                samples = sections.setdefault(kind, {"fresh": 0, "unknown_or_stale": 0})
                samples["fresh" if fresh_section(snapshot, kind, 0) is not None else "unknown_or_stale"] += 1
            sample_metrics = self.metrics(snapshot)
            # Accepted old observations may guide policy, but cannot count as
            # measurements of an unobserved new day in campaign metrics.
            for metric in sample_metrics:
                section = "research" if metric == "idle_research_slots" else "metrics"
                if fresh_section(snapshot, section, 0) is None:
                    sample_metrics[metric] = None
            for metric, value in sample_metrics.items():
                samples = self.state["metric_samples"].setdefault(metric, {"known": 0, "unknown": 0})
                samples["known" if value is not None else "unknown"] += 1
            for metric, total in (("idle_research_slots", "idle_research_slot_days"),
                                  ("idle_military_factories", "idle_military_factory_days"),
                                  ("idle_civilian_factories", "idle_civilian_factory_days")):
                self.state[total] += sample_metrics[metric] or 0
        stop_days = self.state.setdefault("stop_reason_days", {})
        for reason in out["stop_reasons"]:
            if stop_days.get(reason) != day:
                self.state["stop_reasons"][reason] = self.state["stop_reasons"].get(reason, 0) + 1
                stop_days[reason] = day
        out["report"] = self.report()
        return out

    def report(self):
        out = {k: copy.deepcopy(v) for k, v in self.state.items() if k not in ("attempts", "stop_reason_days")}
        out["game_days_elapsed"] = (self.state["last_day"] - self.state["first_day"]
                                    if self.state["first_day"] is not None else None)
        out["missing_game_days"] = (out["game_days_elapsed"] + 1 - out["observed_days"]
                                    if out["game_days_elapsed"] is not None else None)
        out["coverage_note"] = "idle totals sum observed daily samples; missing days are not extrapolated"
        out["section_coverage_note"] = "fresh counts require dated observations; they do not establish complete fields or verified automation"
        return out

    def _confirm(self, snapshot, fresh):
        """Positive evidence resets consecutive retry counts, even for partial progress."""
        for ident, attempt in list(self.state["attempts"].items()):
            kind = attempt.get("kind")
            now, before = fresh.get(kind), attempt.get("before")
            if not now or not before or now.get("as_of_day", snapshot.get("as_of_day")) <= attempt["last_day"]:
                continue
            confirmed = False
            if kind in ("research", "focus"):
                for field in ("completed", "active"):
                    known = now.get(field)
                    if isinstance(known, list):
                        confirmed = confirmed or any(_key(kind, choice) == ident for choice in known)
            elif kind == "production":
                old_lines, new_lines = before.get("lines"), now.get("lines")
                if isinstance(old_lines, list) and isinstance(new_lines, list):
                    for target in self.plan["production"]:
                        old = next((r for r in old_lines if isinstance(r, dict) and str(r.get("line")) == target["line"]), {})
                        new = next((r for r in new_lines if isinstance(r, dict) and str(r.get("line")) == target["line"]), {})
                        if _number(old.get("factories")) is not None and _number(new.get("factories")) is not None:
                            confirmed = confirmed or new["factories"] > old["factories"]
            elif kind == "construction":
                for target in self.plan["construction"]:
                    if _key(kind, target) != ident:
                        continue
                    totals = []
                    for observation in (before, now):
                        rows = observation.get("items")
                        row = next((r for r in rows if isinstance(r, dict) and r.get("building") == target["building"]
                                    and str(r.get("state")) == target["state"]), {}) if isinstance(rows, list) else {}
                        totals.append(row["built"] + row["queued"] if _number(row.get("built")) is not None
                                      and _number(row.get("queued")) is not None else None)
                    confirmed = all(v is not None for v in totals) and totals[1] > totals[0]
            elif kind == "army":
                for target in self.plan["army"]:
                    states = now.get("unassigned_by_state")
                    old = before.get("unassigned_by_state")
                    if _key(kind, target) == ident and isinstance(states, dict) and isinstance(old, dict):
                        a, b = _number(old.get(target["state"])), _number(states.get(target["state"]))
                        confirmed = a is not None and b is not None and b < a
            elif kind == "events":
                opened = now.get("open")
                if isinstance(opened, list):
                    confirmed = not any(isinstance(e, dict) and _key(kind, e.get("id", e)) == ident for e in opened)
            if confirmed:
                self.state["attempts"].pop(ident, None)


def daily_events(previous, current, max_age_days=0):
    """Level events and transitions from explicitly known own-country sections."""
    events = []
    for kind in ("research", "focus"):
        now = fresh_section(current, kind, max_age_days)
        before = fresh_section(previous or {}, kind, max_age_days)
        if not now:
            continue
        idle = now.get("idle")
        if (kind == "focus" and idle is True) or (kind == "research" and _number(idle) is not None and idle > 0):
            events.append(kind + "_idle")
        monotonic = _day((previous or {}).get("day")) is not None and _day(current.get("day")) is not None and previous["day"] <= current["day"]
        if before and monotonic and isinstance(now.get("completed"), list) and isinstance(before.get("completed"), list):
            if set(now["completed"]) - set(before["completed"]):
                events.append("tech_done" if kind == "research" else "focus_done")
    metrics = fresh_section(current, "metrics", max_age_days)
    if metrics and (_number(metrics.get("idle_military_factories")) or 0) > 0:
        events.append("production_idle")
    observation = fresh_section(current, "events", max_age_days)
    if observation and isinstance(observation.get("open"), list) and observation["open"]:
        events.append("popup")
    return events


def wait_matches(snapshot, predicates, baseline=None, max_age_days=0):
    """OR wait predicates; missing data never satisfies a condition.

    Supported: {event:daily-event-code}, {after_days:positive-int},
    {section:name, field:direct-field, op:gte|lte|eq, value:JSON-scalar}.
    Returns matched predicate indices; this function does not poll or sleep.
    """
    if not isinstance(predicates, list) or len(predicates) > 64:
        raise ToolError("wait predicates must be a list of at most 64 entries")
    matches = []
    for index, predicate in enumerate(predicates):
        if not isinstance(predicate, dict):
            raise ToolError("wait predicate must be an object")
        matched = False
        if set(predicate) == {"event"}:
            if predicate["event"] not in {"research_idle", "focus_idle", "tech_done", "focus_done", "production_idle", "popup"}:
                raise ToolError("unknown wait event")
            matched = predicate["event"] in daily_events(baseline, snapshot, max_age_days)
        elif set(predicate) == {"after_days"}:
            days = _day(predicate["after_days"])
            if days is None or days <= 0:
                raise ToolError("after_days must be a positive integer")
            start, end = _day((baseline or {}).get("day")), _day(snapshot.get("day"))
            matched = start is not None and end is not None and end - start >= days
        elif set(predicate) == {"section", "field", "op", "value"}:
            if predicate["section"] not in OBSERVATIONS + ("metrics",) or not isinstance(predicate["field"], str):
                raise ToolError("unknown wait section or field")
            op = predicate["op"]
            if op not in ("gte", "lte", "eq"):
                raise ToolError("wait op is gte, lte or eq")
            observation = fresh_section(snapshot, predicate["section"], max_age_days)
            value = observation.get(predicate["field"]) if observation else None
            target = predicate["value"]
            if not isinstance(target, (str, int, float, bool)) and target is not None:
                raise ToolError("wait comparison value must be a JSON scalar")
            if value is not None and op == "eq":
                matched = ((_number(value) is not None and _number(target) is not None) or type(value) is type(target)) and value == target
            elif _number(value) is not None and _number(target) is not None:
                matched = value >= target if op == "gte" else value <= target
        else:
            raise ToolError("unknown wait predicate shape")
        if matched:
            matches.append(index)
    return matches


def digest(result, snapshot=None, max_bytes=2048):
    """A valid JSON UTF-8 digest capped at 2 KiB (even with Unicode labels).

    Size pressure drops low-priority whole alerts, preserving complete call
    arguments for every remaining fix. ``omitted_alerts`` signals truncation.
    """
    if not isinstance(max_bytes, int) or isinstance(max_bytes, bool) or not 512 <= max_bytes <= 2048:
        raise ToolError("digest byte budget must be 512..2048")
    snapshot = snapshot or {}
    out = {"day": result.get("day"), "date": str(snapshot.get("date", ""))[:32],
           "tag": str(snapshot.get("tag", ""))[:8], "source": str(snapshot.get("source", "unknown"))[:40],
           "as_of_day": snapshot.get("as_of_day"), "metrics": result.get("metrics", {}),
           "alerts": copy.deepcopy(result.get("alerts", [])),
           "stop": list(result.get("stop_reasons", [])), "omitted_alerts": 0}
    out["observations"] = {kind: "fresh" if fresh_section(snapshot, kind, 0) is not None
                           else "stale" if isinstance(snapshot.get(kind), dict) else "unknown"
                           for kind in OBSERVATIONS}

    def encoded():
        return json.dumps(out, ensure_ascii=False, separators=(",", ":"), allow_nan=False)

    while len(encoded().encode("utf-8")) > max_bytes and out["alerts"]:
        out["alerts"].pop()
        out["omitted_alerts"] += 1
    while len(encoded().encode("utf-8")) > max_bytes and out["stop"]:
        out["stop"].pop()
        out["omitted_stop_reasons"] = out.get("omitted_stop_reasons", 0) + 1
    if len(encoded().encode("utf-8")) > max_bytes:
        out["metrics"] = {key: result.get("metrics", {}).get(key) for key in METRICS}
    if len(encoded().encode("utf-8")) > max_bytes:
        out.pop("observations", None)
    if len(encoded().encode("utf-8")) > max_bytes:
        raise ToolError("digest essential fields exceed byte budget")
    return encoded()
