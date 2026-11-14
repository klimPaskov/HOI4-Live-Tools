"""One plan update: menus, a proposal, the gates, the atomic apply, the journal line.

    Updater(run_dir, store, ...).run(triggers, snap, h, plan_state) -> journal entry

The proposal comes from one of three proposers, tried in this order, each only when it may run:
  model    a planning model through scripts/strategist/run.py (any configured launcher; role `planner`, or
           `strategist` for an escalated update), inside the budget, with a wall timeout
  agent    the user's agent: the request is written to RUN/planner/update-<n>/request.md and the answer is the
           patch it submits (the supervisor writes it to patch.json) before the deadline
  default  the menus' default picks (always available, never waits)
A proposal that fails the gates is recorded in the ledger and the next one is tried; the default pick is the
last. Nothing here waits for the game or pauses it.
"""

import json
import os
import subprocess
import sys
import time

from . import gates as gt
from . import menus as mn
from . import packet as pk
from . import patch as pt
from .budget import tokens_of
from .store import Conflict

MODEL_TIMEOUT = 180
AGENT_TIMEOUT = 120


def write_json(path, obj):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=1, ensure_ascii=False, default=sorted)
        f.write("\n")
    os.replace(tmp, path)


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def done_set(snap, plan_state):
    out = set(snap.get("research_done") or []) | set(snap.get("focuses_done") or [])
    out |= {t for t in snap.get("researching") or [] if t}
    if snap.get("focus"):
        out.add(snap["focus"])
    for key in ((plan_state or {}).get("plan_built") or {}):
        out.add(key.split(":", 1)[1] if key.startswith("id:") else key)
    return out


class ModelProposer:
    """A planning model through the strategist launcher (provider-neutral: run.py picks the adapter)."""

    name = "model"

    def __init__(self, tools_dir, provider=None, timeout=MODEL_TIMEOUT, runner=None):
        self.tools, self.provider, self.timeout = tools_dir, provider, timeout
        self.runner = runner or self._run

    def _run(self, argv, timeout):
        try:
            return subprocess.run(argv, capture_output=True, text=True, timeout=timeout + 30).returncode
        except subprocess.TimeoutExpired:
            return 124

    def propose(self, folder, escalated):
        role = "strategist" if escalated else "planner"
        argv = [sys.executable, os.path.join(self.tools, "strategist", "run.py"), "--role", role, "--run", folder,
                "--prompt", os.path.join(folder, "prompt.md"), "--timeout", str(self.timeout)]
        if escalated:
            argv += ["--revision", "full"]
        if self.provider:
            argv += ["--provider", self.provider]
        t0 = time.time()
        code = self.runner(argv, self.timeout)
        secs = time.time() - t0
        manifest = read_json(os.path.join(folder, "agents-manifest.json")) or {}
        entry = (manifest.get("roles") or {}).get(role) or {}
        usage = entry.get("usage") or {}
        return {"patch": read_json(os.path.join(folder, "patch.json")), "seconds": round(secs, 1), "exit": code,
                "role": role, "model": entry.get("resolved_model") or entry.get("model_alias"),
                "tokens": tokens_of(usage), "usage": usage, "cost_usd": entry.get("cost_usd")}


class AgentProposer:
    """The user's agent answers the request through the supervisor (hoi4_run plan_patch)."""

    name = "agent"

    def __init__(self, notify, timeout=AGENT_TIMEOUT, clock=time.time, sleep=time.sleep):
        self.notify, self.timeout, self.clock, self.sleep = notify, timeout, clock, sleep

    def propose(self, folder, escalated):
        t0 = self.clock()
        self.notify(folder)
        answer = os.path.join(folder, "patch.json")
        while self.clock() - t0 < self.timeout and not os.path.exists(answer):
            self.sleep(1.0)
        return {"patch": read_json(answer), "seconds": round(self.clock() - t0, 1), "role": "agent", "tokens": 0}


class Updater:
    def __init__(self, run_dir, store, ledger, budget, facts=None, tools=None, proposer=None, clock=time.time):
        self.run_dir, self.store, self.ledger, self.budget = run_dir, store, ledger, budget
        self.facts, self.tools, self.proposer, self.clock = facts, tools, proposer, clock
        self.costs = facts.costs() if facts else None
        self.dir = os.path.join(run_dir, "planner")
        os.makedirs(self.dir, exist_ok=True)
        self.journal = os.path.join(self.dir, "journal.jsonl")
        self.count = sum(1 for n in os.listdir(self.dir) if n.startswith("update-"))

    def _journal(self, entry):
        with open(self.journal, "a", encoding="utf-8") as f:
            f.write(json.dumps(entry, ensure_ascii=False, default=sorted) + "\n")

    def run(self, triggers, snap, h, plan_state=None):
        t0 = self.clock()
        self.count += 1
        folder = os.path.join(self.dir, "update-%d" % self.count)
        os.makedirs(folder, exist_ok=True)
        plan, sha = self.store.read()
        escalated = any(t["class"] == "strategy" for t in triggers)
        menus = mn.build(plan, snap, h, triggers, self.facts, self.ledger)
        default = pt.merge([m["default"] for m in menus.values() if m.get("default")],
                           summary="default: " + "; ".join(m["default"]["summary"] for m in menus.values()
                                                           if m.get("default")))
        packet = pk.build(plan, snap, h, triggers, menus, self.ledger, escalated)
        write_json(os.path.join(folder, "packet.json"), packet)
        with open(os.path.join(folder, "prompt.md"), "w", encoding="utf-8") as f:
            f.write(pk.prompt(os.path.join(folder, "packet.json"), os.path.join(folder, "patch.json"), escalated))
        entry = {"update": self.count, "date": snap.get("date"), "day": snap.get("day"),
                 "triggers": [t["id"] for t in triggers], "escalated": escalated, "proposals": []}
        proposals = []
        if self.proposer is not None:
            allowed, why = (True, "") if self.proposer.name == "agent" else self.budget.allow(escalated)
            if allowed:
                got = self.proposer.propose(folder, escalated)
                if self.proposer.name == "model":
                    self.budget.record(got["seconds"], got.get("tokens"), escalated, got.get("role"),
                                       ok=got.get("patch") is not None)
                info = {k: got.get(k) for k in ("role", "model", "seconds", "tokens", "usage", "cost_usd", "exit")
                        if got.get(k) is not None}
                if isinstance(got.get("patch"), dict):
                    proposals.append((self.proposer.name, got["patch"], info))
                else:
                    entry["proposals"].append(dict(info, proposer=self.proposer.name, result="no patch"))
            else:
                entry["proposals"].append({"proposer": self.proposer.name, "result": "skipped: " + why})
        if default["ops"]:
            proposals.append(("default", default, {}))
        done = done_set(snap, plan_state)
        for name, proposal, info in proposals:
            record = dict(info, proposer=name, summary=proposal.get("summary"), ops=len(proposal.get("ops") or []))
            entry["proposals"].append(record)
            if not proposal.get("ops"):
                record["result"] = "no change"
                continue
            try:
                candidate = pt.apply(plan, proposal, done=done)
            except pt.PatchError as exc:
                record["result"] = "refused: %s" % exc
                self.ledger.add(snap.get("day"), "update", "update-%d:%s" % (self.count, name), "rejected", exc)
                continue
            result = gt.run(plan, candidate, proposal, snap, triggers, menus, self.costs or _flat(), self.ledger,
                            self.tools, incumbent_key=sha)
            record["gates"] = result
            if not result["ok"]:
                failed = [s for s in result["steps"] if not s["ok"]][-1]
                record["result"] = "gates: %s" % failed["gate"]
                self.ledger.add(snap.get("day"), "update", "update-%d:%s" % (self.count, name), "rejected",
                                "%s: %s" % (failed["gate"], "; ".join(failed["detail"])))
                continue
            write_json(os.path.join(folder, "applied.json"), proposal)
            try:
                version = self.store.apply(candidate, sha, proposal.get("summary") or name, proposer=name,
                                           trigger=",".join(entry["triggers"])[:300], gates="partial" if result["partial"] else "full")
            except Conflict as exc:
                record["result"] = "conflict: %s" % exc
                break
            record["result"] = "applied v%d" % version
            entry["version"] = version
            entry["applied"] = name
            entry["summary"] = proposal.get("summary")
            for op in proposal.get("ops") or []:
                self.ledger.add(snap.get("day"), "patch", "%s:%s" % (op.get("op"), op.get("key")), "applied",
                                proposal.get("summary"))
            break
        entry["seconds"] = round(self.clock() - t0, 2)
        self._journal(entry)
        return entry


def _flat():
    from .horizon import Costs
    return Costs()
