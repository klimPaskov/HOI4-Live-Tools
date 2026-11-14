"""The plan file of a run: versions, compare-and-swap, history and rollback.

The executor re-reads RUN/plan.json every tick, so replacing the file atomically (write, then rename) is an
atomic plan change. `apply` writes only when the file still has the hash the candidate was built from (the
caller holds the supervisor's lock, which the player's `hoi4_plan set` also takes); the plan in force is kept
as RUN/plans/plan.v<N>.json before the new one goes in; `_meta.version` and `_meta.parent` record the chain.
"""

import copy
import datetime
import hashlib
import json
import os
import threading


class Conflict(RuntimeError):
    """The plan changed since the candidate was built from it."""


def digest(data):
    return hashlib.sha256(data).hexdigest()


def _now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def version_of(plan):
    v = (plan.get("_meta") or {}).get("version") if isinstance(plan, dict) else None
    return v if isinstance(v, int) and v > 0 else 1


class PlanStore:
    def __init__(self, run_dir, lock=None, clock=_now):
        self.run = run_dir
        self.path = os.path.join(run_dir, "plan.json")
        self.history = os.path.join(run_dir, "plans")
        self.lock = lock or threading.RLock()
        self.clock = clock

    def read(self):
        """(plan, sha256 of the file bytes)."""
        with open(self.path, "rb") as f:
            data = f.read()
        return json.loads(data.decode("utf-8")), digest(data)

    def _write(self, plan):
        tmp = self.path + ".planner.tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(plan, f, indent=1, ensure_ascii=False)
            f.write("\n")
        os.replace(tmp, self.path)

    def keep(self, plan):
        os.makedirs(self.history, exist_ok=True)
        path = os.path.join(self.history, "plan.v%d.json" % version_of(plan))
        if not os.path.exists(path):
            with open(path, "w", encoding="utf-8") as f:
                json.dump(plan, f, indent=1, ensure_ascii=False)
                f.write("\n")
        return path

    def apply(self, candidate, expect_sha, summary, **meta):
        """Install `candidate` as version N+1 when the file still hashes to `expect_sha`; returns N+1."""
        with self.lock:
            current, sha = self.read()
            if sha != expect_sha:
                raise Conflict("the plan changed since the update was built (version %d)" % version_of(current))
            self.keep(current)
            new = copy.deepcopy(candidate)
            m = dict(new.get("_meta") or {})
            n = version_of(current)
            m.update({"version": n + 1, "parent": n, "updated": self.clock(), "update_summary": summary})
            m.update({k: v for k, v in meta.items() if v is not None})
            new["_meta"] = m
            self._write(new)
            return n + 1

    def rollback(self, reason):
        """Re-install the parent of the plan in force as a new version; returns (version, parent) or None."""
        with self.lock:
            current, _ = self.read()
            parent = (current.get("_meta") or {}).get("parent")
            path = os.path.join(self.history, "plan.v%s.json" % parent)
            if not parent or not os.path.exists(path):
                return None
            with open(path, encoding="utf-8") as f:
                old = json.load(f)
            self.keep(current)
            m = dict(old.get("_meta") or {})
            n = version_of(current)
            m.update({"version": n + 1, "parent": n, "updated": self.clock(), "rolled_back_from": n,
                      "update_summary": "rollback to v%s: %s" % (parent, reason)})
            old["_meta"] = m
            self._write(old)
            return n + 1, parent
