"""The gate subset an update passes before it is applied (docs/ROLLING_PLAN.md section 4).

    shape      only allowed operations; strategy keys only in an escalated update with a reason
    invariants ids kept, nothing done listed again, strategy keys unchanged unless escalated, nothing the
               ledger shows failed inside its cooldown
    validate   validate_plan.py --strict: no more errors than the plan in force has
    lint       lint_plan.py: for every rule, no more errors than the plan in force has (the lint is anchored at
               the scenario start; errors that come from the start are in both plans and cancel)
    anchored   from today's state: each queue a trigger named low is no longer low (when the menu had options),
               every short equipment family has a line, a new line or the first place in production_priority

validate and lint need the planning tools and the game files (facts.tools_dir); without them those two steps
are skipped and the result says `partial`.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
import time

from . import horizon as hz
from . import patch as pt

STRATEGY_PATHS = ("goal", "phases", "checkpoints", "horizons", "contingencies", "force_posture", "army_layout",
                  "army_group", "armies", "war_by_phase", "peace", "diplomacy", "faction", "invasions",
                  "war.enemies", "war.axes", "war.theatre", "war.guard_states")


def _at(plan, dotted):
    node = plan
    for p in dotted.split("."):
        if not isinstance(node, dict):
            return None
        node = node.get(p)
    return node


def _ids(items):
    return {x.get("id") for x in items or [] if isinstance(x, dict) and x.get("id")}


def invariants(incumbent, candidate, patch, snap, ledger=None):
    errors = []
    if not patch.get("escalated"):
        for path in STRATEGY_PATHS:
            if json.dumps(_at(incumbent, path), sort_keys=True) != json.dumps(_at(candidate, path), sort_keys=True):
                errors.append("%s changed without an escalated update" % path)
    removed = {op.get("id") for op in patch.get("ops") or [] if op.get("op") == "remove"}
    for key, value in incumbent.items():
        if isinstance(value, list):
            lost = _ids(value) - _ids(candidate.get(key)) - removed
            if lost and key not in ("research_path",):
                errors.append("%s lost item ids %s (the executor keys its memory on them)" % (key, sorted(lost)[:5]))
    done_t = set(snap.get("research_done") or [])
    done_f = set(snap.get("focuses_done") or [])
    new_t = set(hz.planned_techs(candidate)) - set(hz.planned_techs(incumbent))
    new_f = set(hz.planned_focuses(candidate)) - set(hz.planned_focuses(incumbent))
    if new_t & done_t:
        errors.append("researched techs listed again: %s" % sorted(new_t & done_t)[:5])
    if new_f & done_f:
        errors.append("completed focuses listed again: %s" % sorted(new_f & done_f)[:5])
    if ledger:
        day = snap.get("day")
        new_eq = {e.get("equipment") for e in candidate.get("production") or [] if isinstance(e, dict)} - \
                 {e.get("equipment") for e in incumbent.get("production") or [] if isinstance(e, dict)}
        for key in sorted(new_t | new_f | {e for e in new_eq if e}):
            if ledger.failed(key, day):
                errors.append("%s failed recently (ledger); not re-proposed inside its cooldown" % key)
    return errors


def anchored(candidate, snap, triggers, menus, costs):
    errors = []
    h = hz.estimate(candidate, snap, costs)
    for t in triggers:
        d = t.get("domain")
        if t["kind"] != "horizon_low" or d not in ("research", "focus"):
            continue
        if h[d]["low"] and (menus.get(d) or {}).get("options"):
            errors.append("%s is still low after the update: %s" % (d, h[d]["why"]))
    short = set((snap.get("shortages") or {}))
    if any(t.get("domain") == "production" for t in triggers):
        lines = set(snap.get("lines") or {})
        entries = {e.get("equipment") or "" for e in candidate.get("production") or [] if isinstance(e, dict)
                   and not e.get("remove")}
        first = (candidate.get("production_priority") or [None])[0]
        for fam in sorted(short):
            if not any(x.startswith(fam) for x in lines | entries) and first != fam:
                errors.append("%s is short and the update gives it no line" % fam)
    return errors


class ToolGates:
    """validate_plan --strict and lint_plan through the planning tools, with the incumbent's counts cached."""

    def __init__(self, tools_dir, game_dir=None, mods=(), runner=None, timeout=120):
        self.tools, self.game, self.mods, self.timeout = tools_dir, game_dir, list(mods), timeout
        self.runner = runner or self._run
        self._cache = {}

    def _run(self, argv):
        p = subprocess.run(argv, capture_output=True, text=True, timeout=self.timeout, encoding="utf-8",
                           errors="replace")
        return p.returncode, (p.stdout or "") + (p.stderr or "")

    def _args(self):
        out = ["--game", self.game] if self.game else []
        for m in self.mods:
            out += ["--mod", m]
        return out

    def counts(self, plan, key=None):
        """{"validate": n, "lint": {rule: n}} for a plan (cached by `key`)."""
        if key and key in self._cache:
            return self._cache[key]
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "plan.json")
            with open(path, "w", encoding="utf-8") as f:
                json.dump(plan, f)
            _, vout = self.runner([sys.executable, "-I", os.path.join(self.tools, "validate_plan.py"), path,
                                   "--strict"] + self._args())
            _, lout = self.runner([sys.executable, "-I", os.path.join(self.tools, "hoi4facts", "lint_plan.py"), path,
                                   "--json"] + self._args())
        found = re.findall(r"(\d+) error\(s\)", vout)
        rules = {}
        try:
            lint = json.loads(lout[lout.index("{"):])
            for f in lint.get("findings") or []:
                if f.get("severity") == "error":
                    rules[f["rule"]] = rules.get(f["rule"], 0) + 1
        except ValueError:
            rules = {"_unreadable": 1}
        out = {"validate": int(found[-1]) if found else 0, "lint": rules}
        if key:
            self._cache[key] = out
        return out


def run(incumbent, candidate, patch, snap, triggers, menus, costs, ledger=None, tools=None, incumbent_key=None):
    """{"ok", "partial", "steps": [{"gate", "ok", "detail", "seconds"}]}; stops at the first failing step."""
    steps = []

    def step(name, fn):
        t0 = time.time()
        errors = fn()
        steps.append({"gate": name, "ok": not errors, "detail": errors[:8], "seconds": round(time.time() - t0, 2)})
        return not errors

    ok = step("shape", lambda: pt.shape_errors(patch)) and \
        step("invariants", lambda: invariants(incumbent, candidate, patch, snap, ledger))
    partial = tools is None
    if ok and tools is not None:
        before = tools.counts(incumbent, incumbent_key)
        after = tools.counts(candidate)

        def validate():
            return [] if after["validate"] <= before["validate"] else [
                "validate_plan --strict: %d errors, the plan in force has %d" % (after["validate"], before["validate"])]

        def lint():
            return ["lint %s: %d errors, the plan in force has %d" % (r, n, before["lint"].get(r, 0))
                    for r, n in sorted(after["lint"].items()) if n > before["lint"].get(r, 0)]
        ok = step("validate", validate) and step("lint", lint)
    if ok:
        ok = step("anchored", lambda: anchored(candidate, snap, triggers, menus, costs))
    return {"ok": ok, "partial": partial, "steps": steps}
