"""What was tried and how it ended: the executor's failures and given-up items, and every update.

Entries are {"day", "what", "key", "outcome", "why"}; `key` names the thing tried (an order, a plan item, a
patch operation) so a later update can see it failed and the default picks can leave it alone for a while.
The ledger is a JSON-lines file; `from_executor` reads the executor's own memory (plan_state.json: `failures`
with the game hours of each failure, `plan_blocked` items given up).
"""

import json
import os

HOURS_PER_DAY = 24
COOLDOWN_DAYS = 45


class Ledger:
    def __init__(self, path=None):
        self.path = path
        self.entries = []
        self._keys = set()
        if path and os.path.exists(path):
            with open(path, encoding="utf-8") as f:
                for line in f:
                    try:
                        self._remember(json.loads(line))
                    except ValueError:
                        continue

    def _remember(self, entry):
        sig = (entry.get("key"), entry.get("outcome"), entry.get("day"))
        if sig in self._keys:
            return False
        self._keys.add(sig)
        self.entries.append(entry)
        return True

    def add(self, day, what, key, outcome, why=""):
        entry = {"day": day, "what": what, "key": key, "outcome": outcome, "why": str(why)[:300]}
        if self._remember(entry) and self.path:
            os.makedirs(os.path.dirname(self.path), exist_ok=True)
            with open(self.path, "a", encoding="utf-8") as f:
                f.write(json.dumps(entry, ensure_ascii=False) + "\n")
        return entry

    def from_executor(self, plan_state, hour_to_day):
        """Add the executor's failures (3 or more of one order) and given-up plan items."""
        if not isinstance(plan_state, dict):
            return
        for order, hours in (plan_state.get("failures") or {}).items():
            if isinstance(hours, list) and len(hours) >= 2:
                self.add(hour_to_day(max(hours)), "order", order, "failed", "%d failures" % len(hours))
        for item, info in (plan_state.get("plan_blocked") or {}).items():
            if isinstance(info, dict):
                self.add(hour_to_day(info.get("at") or 0), "plan_item", item, "given_up", info.get("why", ""))

    def failed(self, key, day, cooldown=COOLDOWN_DAYS):
        """True when `key` (or an order naming it) failed or was rejected within the cooldown."""
        for e in reversed(self.entries):
            if e.get("outcome") in ("failed", "given_up", "rejected", "rolled_back") and key and key in str(e.get("key")):
                if day is None or e.get("day") is None or day - e["day"] <= cooldown:
                    return True
        return False

    def tail(self, n=20):
        return self.entries[-n:]


def hour_to_day_from(reference_hour, reference_day):
    """A converter from the executor's game hours to day numbers, anchored on one known (hour, day) pair."""
    def convert(hour):
        return reference_day + (int(hour) - int(reference_hour)) // HOURS_PER_DAY
    return convert
