"""The ceiling on planning model use per wall hour of play, and the record the run report reads.

`allow(escalated)` says whether a model call may run now; when it may not, the update uses the default pick.
`record(...)` adds one finished call. The window is a rolling hour of wall time.
"""

import time

DEFAULTS = {"max_calls": 6, "max_seconds": 600, "max_tokens": 400000, "max_escalations": 2, "window": 3600}


class Budget:
    def __init__(self, clock=time.time, **limits):
        self.clock = clock
        self.limits = dict(DEFAULTS, **{k: v for k, v in limits.items() if v is not None})
        self.calls = []
        self.started = clock()

    def _window(self):
        since = self.clock() - self.limits["window"]
        return [c for c in self.calls if c["t"] >= since]

    def allow(self, escalated=False):
        """(True, "") or (False, why)."""
        w = self._window()
        if len(w) >= self.limits["max_calls"]:
            return False, "%d model calls in the last hour (ceiling %d)" % (len(w), self.limits["max_calls"])
        secs = sum(c["seconds"] for c in w)
        if secs >= self.limits["max_seconds"]:
            return False, "%.0f s of model time in the last hour (ceiling %d)" % (secs, self.limits["max_seconds"])
        tokens = sum(c["tokens"] for c in w)
        if tokens >= self.limits["max_tokens"]:
            return False, "%d tokens in the last hour (ceiling %d)" % (tokens, self.limits["max_tokens"])
        if escalated and sum(1 for c in w if c["escalated"]) >= self.limits["max_escalations"]:
            return False, "%d escalations in the last hour (ceiling %d)" % (
                self.limits["max_escalations"], self.limits["max_escalations"])
        return True, ""

    def record(self, seconds, tokens=0, escalated=False, role=None, ok=True):
        self.calls.append({"t": self.clock(), "seconds": float(seconds or 0), "tokens": int(tokens or 0),
                           "escalated": bool(escalated), "role": role, "ok": ok})

    def summary(self):
        hours = max((self.clock() - self.started) / 3600.0, 1e-9)
        secs = sum(c["seconds"] for c in self.calls)
        tokens = sum(c["tokens"] for c in self.calls)
        return {"model_calls": len(self.calls), "model_seconds": round(secs, 1), "tokens": tokens,
                "escalations": sum(1 for c in self.calls if c["escalated"]),
                "per_hour": {"calls": round(len(self.calls) / hours, 2), "seconds": round(secs / hours, 1),
                             "tokens": round(tokens / hours)},
                "limits": dict(self.limits)}


def tokens_of(usage):
    """Total tokens of a launcher's usage record (input, output and cache), 0 when unknown."""
    if not isinstance(usage, dict):
        return 0
    return int(sum(v for k, v in usage.items() if isinstance(v, (int, float)) and k.endswith("okens")))
