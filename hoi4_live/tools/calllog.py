"""Call recorder: one JSON line per tool call, for overlaying what the agent did onto a screen recording.

`hoi4_lock action=record path=<file.jsonl>` starts appending to a file (an empty path stops); the environment
variable HOI4_LIVE_CALL_LOG or the config key `call_log` starts it with the server. Each line carries the wall
clock start and end (epoch seconds and local ISO time), the tool, a short summary of the arguments, whether the
call succeeded, the first line of the result and a few fields that say what happened (the date, why a run
stopped, whether anything changed). Images are never written. A failing write never fails the call.
"""

import datetime
import json
import logging
import os
import threading

log = logging.getLogger("hoi4_live.calllog")

_WRITE = threading.Lock()
KEEP = ("date", "stopped_by", "changed", "from", "to", "days_run", "mode", "path", "state", "ok", "error",
        "action", "events_answered", "play_mode", "policy_sha256", "agent_pause_count",
        "agent_pause_seconds_in_run", "agent_pause_ongoing", "wall_seconds_per_game_year")


def _short(v, limit=120):
    if isinstance(v, str):
        return v if len(v) <= limit else v[:limit - 1] + "…"
    if isinstance(v, (bool, int, float)) or v is None:
        return v
    if isinstance(v, (list, tuple)):
        if len(v) <= 3 and all(isinstance(x, (str, int, float, bool)) for x in v):
            return [_short(x, 60) for x in v]
        return f"[{len(v)} items]"
    if isinstance(v, dict):
        text = json.dumps(v, ensure_ascii=False, default=str)
        return text if len(text) <= limit else text[:limit - 1] + "…"
    return _short(str(v), limit)


def summarize_args(args):
    return {k: _short(v) for k, v in (args or {}).items()}


def summarize_result(mcp_result):
    """First text line (at most 260 characters) plus the KEEP fields of the structured data."""
    text = ""
    for part in mcp_result.get("content", []):
        if part.get("type") == "text":
            text = part.get("text", "")
            break
    first = next((ln.strip() for ln in text.splitlines() if ln.strip() and ln.strip() not in "{}"), "")
    out = {"summary": _short(first, 260)}
    data = mcp_result.get("structuredContent")
    if isinstance(data, dict):
        extra = {k: _short(data[k], 100) for k in KEEP if k in data}
        if extra:
            out["data"] = extra
    out["images"] = sum(1 for p in mcp_result.get("content", []) if p.get("type") == "image")
    return out


def record(path, name, args, mcp_result, started, ended, perf=None):
    """Append one line to `path`. Returns True when written."""
    if not path:
        return False
    try:
        entry = {
            "ts": datetime.datetime.fromtimestamp(started).isoformat(timespec="milliseconds"),
            "t0": round(started, 3), "t1": round(ended, 3), "elapsed_s": round(ended - started, 2),
            "tool": name, "args": summarize_args(args),
            "ok": not bool(mcp_result.get("isError")),
        }
        entry.update(summarize_result(mcp_result))
        if perf:
            entry["perf"] = perf
        os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
        line = json.dumps(entry, ensure_ascii=False, default=str) + "\n"
        with _WRITE, open(path, "a", encoding="utf-8") as f:
            f.write(line)
        return True
    except Exception as e:     # recording must never break a game action
        log.warning("call log write failed: %s", e)
        return False
