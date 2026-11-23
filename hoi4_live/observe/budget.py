"""The API budget: a ledger of every model call billed to an API key, across runs, and the caps
checked before a call (docs/MONITORING.md, "Model calls and the budget").

Files (beside the key file, or where HOI4_LIVE_API_LEDGER points):
  api-ledger.json  {"runs": {run: {role: {model: {calls, input, output, cache_read, cache_write, usd}}}},
                    "days": {YYYY-MM-DD: usd}, "spent": usd, "prices": {...}, "updated": unix}
  budget.json      optional caps: {"total": 200, "run_cap": 15, "day_cap": 40, "floor": 20}
                   total: everything billed to the key (no total: no total cap); run_cap and day_cap:
                   a run or a day stops calling models when it has spent that much; floor: when the
                   remaining total falls under it, every call on the key stops.

Money is computed from the published per-token prices in costs.PRICES (marked "to be verified"
with the date they were taken). The key itself is never read here.
"""
from __future__ import annotations

import json
import os
import time
from contextlib import contextmanager

from . import costs

DEFAULTS = {"total": None, "run_cap": 15.0, "day_cap": 40.0, "floor": 20.0}
OK, RUN_CAP, DAY_CAP, FLOOR = 0, 3, 3, 4   # exit codes of `observe.py budget check`


def key_file():
    """The API key file: HOI4_LIVE_ANTHROPIC_KEY_FILE, if it names a non-empty file."""
    p = os.environ.get("HOI4_LIVE_ANTHROPIC_KEY_FILE")
    return p if p and os.path.isfile(p) and os.path.getsize(p) > 0 else None


def ledger_dir():
    d = os.environ.get("HOI4_LIVE_API_LEDGER")
    if d:
        return os.path.dirname(d) if d.endswith(".json") else d
    k = key_file()
    return os.path.dirname(k) if k else None


@contextmanager
def _locked(path):
    """A lock file beside the ledger (several processes write it: the monitor, the launchers)."""
    lock = path + ".lock"
    t0 = time.time()
    while True:
        try:
            fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            break
        except FileExistsError:
            try:
                if time.time() - os.path.getmtime(lock) > 60:
                    os.unlink(lock)  # a writer that died holding it
                    continue
            except OSError:
                pass
            if time.time() - t0 > 30:
                raise TimeoutError(f"budget ledger locked: {lock}")
            time.sleep(0.05)
    try:
        yield
    finally:
        os.close(fd)
        try:
            os.unlink(lock)
        except OSError:
            pass


class Budget:
    def __init__(self, directory=None):
        self.dir = directory or ledger_dir()
        self.path = os.path.join(self.dir, "api-ledger.json") if self.dir else None

    @property
    def active(self):
        return self.path is not None

    def caps(self):
        caps = dict(DEFAULTS)
        if self.dir:
            try:
                with open(os.path.join(self.dir, "budget.json"), encoding="utf-8") as f:
                    caps.update({k: v for k, v in json.load(f).items() if k in DEFAULTS})
            except (OSError, ValueError):
                pass
        return caps

    def load(self):
        try:
            with open(self.path, encoding="utf-8") as f:
                d = json.load(f)
        except (OSError, ValueError, TypeError):
            d = {}
        d.setdefault("runs", {})
        d.setdefault("days", {})
        d.setdefault("spent", 0.0)
        return d

    def _save(self, d):
        d["updated"] = int(time.time())
        d["prices"] = {"taken": costs.PRICES_TAKEN, "verified": False, "per_million": costs.PRICES}
        d["spent"] = round(sum(_run_total(r) for r in d["runs"].values()), 6)
        tmp = self.path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(d, f, indent=1, sort_keys=True)
        os.replace(tmp, self.path)

    # --- reading -------------------------------------------------------------------------------

    def status(self, run=None):
        d = self.load() if self.active else {"runs": {}, "days": {}, "spent": 0.0}
        caps = self.caps()
        spent = sum(_run_total(r) for r in d["runs"].values())
        today = d["days"].get(time.strftime("%Y-%m-%d"), 0.0)
        run_spent = _run_total(d["runs"].get(run, {})) if run else 0.0
        remaining = (caps["total"] - spent) if caps["total"] is not None else None
        return {"spent": spent, "today": today, "run": run_spent, "remaining": remaining, "caps": caps}

    def line(self, run=None):
        """The one line at the top of COSTS.md and every gate packet."""
        if not self.active:
            return "API budget: no key configured (model calls go to the subscription or nowhere)"
        s = self.status(run)
        rem = f"${s['remaining']:.2f} of ${s['caps']['total']:.0f}" if s["remaining"] is not None else "no total set"
        return (f"API budget: this run ${s['run']:.2f} (cap ${s['caps']['run_cap']:.0f}), today ${s['today']:.2f} "
                f"(cap ${s['caps']['day_cap']:.0f}), all runs ${s['spent']:.2f}; remaining {rem}")

    def check(self, run=None, role=None):
        """(code, why) before a model call billed to the key: OK, RUN_CAP/DAY_CAP (this run or day
        stops calling models), FLOOR (everything on the key stops)."""
        if not self.active:
            return OK, "no ledger: nothing billed to a key"
        s = self.status(run)
        c = s["caps"]
        if s["remaining"] is not None and s["remaining"] < (c["floor"] or 0):
            return FLOOR, f"remaining ${s['remaining']:.2f} is under the floor ${c['floor']:.0f}: no more calls on the key"
        if run and c["run_cap"] is not None and s["run"] >= c["run_cap"]:
            return RUN_CAP, f"run {run} spent ${s['run']:.2f} of its ${c['run_cap']:.0f} cap"
        if c["day_cap"] is not None and s["today"] >= c["day_cap"]:
            return DAY_CAP, f"today ${s['today']:.2f} of the ${c['day_cap']:.0f} day cap"
        return OK, "within budget"

    # --- writing -------------------------------------------------------------------------------

    def record(self, run, role, model, usage):
        """One call made here (an escalation, a review): added to the ledger. Returns its USD."""
        usd = costs.price_of(model, _api_usage(usage)) or 0.0
        if not self.active:
            return usd
        with _locked(self.path):
            d = self.load()
            row = d["runs"].setdefault(run or "?", {}).setdefault(role, {}).setdefault(model or "?", _empty())
            _add(row, usage, usd)
            day = time.strftime("%Y-%m-%d")
            d["days"][day] = round(d["days"].get(day, 0.0) + usd, 6)
            self._save(d)
        return usd

    def set_role(self, run, role, by_model):
        """A role measured from its own full log (the player's stream): replaces that run's role and
        moves today's total by the difference."""
        if not self.active:
            return
        with _locked(self.path):
            d = self.load()
            r = d["runs"].setdefault(run, {})
            before = sum(m.get("usd", 0.0) for m in (r.get(role) or {}).values())
            r[role] = by_model
            after = sum(m.get("usd", 0.0) for m in by_model.values())
            day = time.strftime("%Y-%m-%d")
            if after != before:
                d["days"][day] = round(d["days"].get(day, 0.0) + after - before, 6)
            self._save(d)


def _empty():
    return {"calls": 0, "input": 0, "output": 0, "cache_read": 0, "cache_write": 0, "usd": 0.0}


def _api_usage(u):
    return {"input_tokens": u.get("input_tokens", u.get("input")) or 0,
            "output_tokens": u.get("output_tokens", u.get("output")) or 0,
            "cache_read_input_tokens": u.get("cache_read_input_tokens", u.get("cache_read")) or 0,
            "cache_creation_input_tokens": u.get("cache_creation_input_tokens", u.get("cache_write")) or 0}


def _add(row, usage, usd):
    u = _api_usage(usage)
    row["calls"] += 1
    row["input"] += u["input_tokens"]
    row["output"] += u["output_tokens"]
    row["cache_read"] += u["cache_read_input_tokens"]
    row["cache_write"] += u["cache_creation_input_tokens"]
    row["usd"] = round(row["usd"] + usd, 6)


def _run_total(r):
    return sum(m.get("usd", 0.0) for role in r.values() for m in role.values())


def player_rows(run_dir, name="player.out"):
    """The player's usage by model from its stream log, in the ledger's row form."""
    out = {}
    for model, r in costs.player_usage(run_dir, name).items():
        out[model] = {"calls": r["calls"], "input": r["input_tokens"], "output": r["output_tokens"],
                      "cache_read": r["cache_read_input_tokens"], "cache_write": r["cache_creation_input_tokens"],
                      "usd": round(r["usd_micro"] / 1e6, 6)}
    return out


def player_billed_to_key(run_dir, name="player.out"):
    """True when the player's stream says it ran on an API key (Claude Code's init line names the
    key source), False when it names another source, None when the stream does not say."""
    try:
        with open(os.path.join(run_dir, name), encoding="utf-8", errors="replace") as f:
            for _, line in zip(range(50), f):
                try:
                    d = json.loads(line)
                except ValueError:
                    continue
                src = d.get("apiKeySource") or d.get("api_key_source")
                if src:
                    return src not in ("none", "oauth", "subscription")
    except OSError:
        pass
    return None
