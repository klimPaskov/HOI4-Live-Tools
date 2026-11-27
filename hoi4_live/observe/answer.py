"""The monitor's own model calls: Tier 1 (an escalation packet, a small model) and Tier 2 (a gate
review, a strong model), billed to an API key and bounded (docs/MONITORING.md).

The key is read from the file HOI4_LIVE_ANTHROPIC_KEY_FILE names, for the call only (it is handed to
the client, never put in the environment, a log or an error message). Before every call the budget
is checked (budget.py: the run's cap, the day's cap, the floor of the remaining total); after it the
usage the response reports is written to the ledger and to the run's cost log.

Models: MONITOR_TIER1_MODEL (default claude-haiku-5-5), MONITOR_TIER2_MODEL (default
claude-opus-5-5). Needs the `anthropic` package; without it the call is skipped with a reason.

As a command (the MONITOR_ANSWER_CMD form): `python -m hoi4_live.observe.answer --tier 1 --run NAME`
reads the packet on stdin and prints {"result", "usage", "model", "total_cost_usd"} or {"error"}.
"""
from __future__ import annotations

import argparse
import json
import sys
import time

from . import budget as B

MODELS = {1: "claude-haiku-5-5", 2: "claude-opus-5-5", 3: "claude-opus-5-5"}
LIMITS = {1: {"max_tokens": 2000, "effort": "low"}, 2: {"max_tokens": 6000, "effort": "medium"},
          3: {"max_tokens": 8000, "effort": "high"}}
TIMEOUT = {1: 60.0, 2: 180.0, 3: 300.0}
SYSTEM = {
    1: ("You answer escalations of a rule-based monitor of an autonomous Hearts of Iron IV campaign. "
        "Read the packet and reply with exactly one JSON object from its ALLOWED RESPONSES, nothing else. "
        "Prefer the smallest action that clears the issue; give the reason in one sentence."),
    2: ("You review a gate packet of an autonomous Hearts of Iron IV campaign (a declaration of war, a "
        "capitulation, a new year, 60 game days, a failure or the end of a run). In at most 300 words: first "
        "TOP 3 ROOT CAUSES, numbered: for each the cause behind the symptoms (the executor rule, plan key or "
        "tool behaviour that produces them, not the symptom itself), the lane or plan key that owns it, the "
        "packet lines that show it, and one concrete remedy; then whether the plan must change before the next "
        "phase, and how. No preamble."),
    3: ("You review one finished segment of an autonomous Hearts of Iron IV campaign for its engineer: the "
        "issue list, the last gate packet and the lessons the run wrote. Propose at most six changes, each "
        "either a PLAN change (the plan key and the new value) or an EXECUTOR RULE change (the lane, the rule "
        "as it is and as it should be, said generally: for any country and any war, never a scripted move), "
        "with the issue ids it removes and the evidence. Order them by how much of the campaign they decide. "
        "Then one line: what the next segment must measure. No preamble."),
}


def model_for(tier):
    import os
    return os.environ.get(f"MONITOR_TIER{tier}_MODEL") or MODELS[tier]


def available():
    """Why a call cannot be made, or None."""
    if not B.key_file():
        return "no API key file (HOI4_LIVE_ANTHROPIC_KEY_FILE)"
    try:
        import anthropic  # noqa: F401
    except ImportError:
        return "the anthropic package is not installed"
    return None


def ask(packet, tier=1, run=None, role=None, system=None):
    """One bounded call. Returns {"result", "usage", "model", "usd", "seconds"} or {"error", ...}.
    Tier 1 the small model, 2 the strong one, 3 the strong one at high effort (a segment review);
    `system` overrides the tier's instructions (a Tier 2 escalation answers in the Tier 1 format)."""
    role = role or ("escalation" if tier == 1 else "review")
    why = available()
    if why:
        return {"error": why, "skipped": True}
    bud = B.Budget()
    code, why = bud.check(run, role)
    if code != B.OK:
        return {"error": f"budget: {why}", "skipped": True, "budget_code": code}
    import anthropic
    with open(B.key_file(), encoding="utf-8") as f:
        key = f.read().strip()
    model = model_for(tier)
    t0 = time.time()
    try:
        client = anthropic.Anthropic(api_key=key, timeout=TIMEOUT[tier], max_retries=2)
        resp = client.messages.create(
            model=model,
            max_tokens=LIMITS[tier]["max_tokens"],
            system=system or SYSTEM[tier],
            output_config={"effort": LIMITS[tier]["effort"]},
            messages=[{"role": "user", "content": packet}],
        )
    except anthropic.APIStatusError as ex:
        return {"error": f"api status {ex.status_code}", "seconds": round(time.time() - t0, 1)}
    except anthropic.APIError as ex:
        return {"error": f"api error {type(ex).__name__}", "seconds": round(time.time() - t0, 1)}
    finally:
        key = None
    u = resp.usage
    usage = {"input_tokens": u.input_tokens or 0, "output_tokens": u.output_tokens or 0,
             "cache_read_input_tokens": getattr(u, "cache_read_input_tokens", 0) or 0,
             "cache_creation_input_tokens": getattr(u, "cache_creation_input_tokens", 0) or 0}
    usd = bud.record(run, role, model, usage)
    text = "".join(getattr(b, "text", "") for b in resp.content if getattr(b, "type", "") == "text")
    return {"result": text, "usage": usage, "model": model, "usd": usd, "stop_reason": resp.stop_reason,
            "seconds": round(time.time() - t0, 1)}


def main(argv=None):
    ap = argparse.ArgumentParser(description="one bounded monitor model call (packet on stdin)")
    ap.add_argument("--tier", type=int, choices=(1, 2, 3), default=1)
    ap.add_argument("--run")
    a = ap.parse_args(argv)
    out = ask(sys.stdin.read(), a.tier, a.run)
    if "error" in out:
        print(json.dumps(out))
        return 2
    print(json.dumps({"result": out["result"], "usage": out["usage"], "model": out["model"],
                      "total_cost_usd": out["usd"], "ledger": True}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
