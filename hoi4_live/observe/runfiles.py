"""The executor's and the player's own record in a run directory (file reads only).

  plan_state.json  done work (operation keys op:<id>:...), the failure ledger,
                   blocked plan items, war stages, strategy checkpoints
  mcp.stderr       the server's [action] / [step] / [order] lines
  calls.jsonl      every tool call that came through the run's broker

Missing files give empty sections; nothing here talks to the game.
"""
from __future__ import annotations

import json
import os


def tail_lines(path, max_bytes=262144):
    try:
        with open(path, "rb") as f:
            f.seek(0, os.SEEK_END)
            size = f.tell()
            f.seek(max(0, size - max_bytes))
            data = f.read()
    except OSError:
        return []
    lines = data.decode("utf-8", "replace").splitlines()
    return lines[1:] if size > max_bytes else lines


def operations(done):
    """op id -> phase and what it has done, from plan_state done keys."""
    ops = {}
    for key in done or {}:
        parts = key.split(":")
        if len(parts) < 3 or parts[0] != "op":
            continue
        op, step = parts[1], parts[2]
        o = ops.setdefault(op, {"steps": 0, "fought": [], "ended": False, "executing": False,
                                "prepared": False, "lent": []})
        o["steps"] += 1
        if step == "end":
            o["ended"] = True
        elif step == "exec":
            o["executing"] = True
        elif step == "fought" and len(parts) > 3:
            o["fought"].append(parts[3])
        elif step == "lent" and len(parts) > 3:
            o["lent"].append(":".join(parts[3:]))
        elif step in ("draw", "asm", "fronts", "asmtry", "prep", "prepare"):
            o["prepared"] = True
    for o in ops.values():
        o["phase"] = ("ended" if o["ended"] else "executing" if o["executing"]
                      else "preparing" if o["prepared"] else "planned")
        o["fought"] = sorted(set(o["fought"]))
    return ops


def failure_ledger(failures, limit=12):
    rows = []
    for key, hours in (failures or {}).items():
        hours = hours if isinstance(hours, list) else [hours]
        rows.append({"what": key[:200], "count": len(hours), "last_hour": max(hours) if hours else None,
                     "first_hour": min(hours) if hours else None})
    rows.sort(key=lambda r: (-r["count"], -(r["last_hour"] or 0)))
    return rows[:limit]


def _json_after(line, tag):
    try:
        return json.loads(line[len(tag):].strip())
    except ValueError:
        return None


def server_log(run_dir):
    last_exec = last_player = last_step = last_order = None
    for line in tail_lines(os.path.join(run_dir, "mcp.stderr")):
        if line.startswith("[action]"):
            a = _json_after(line, "[action]")
            if not a:
                continue
            row = {"tool": a.get("tool"), "ok": a.get("ok"), "error": a.get("error"),
                   "unix_ms": a.get("unix_ms")}
            if a.get("executor"):
                last_exec = row
            else:
                last_player = row
        elif line.startswith("[step]"):
            s = _json_after(line, "[step]")
            if s:
                acts = s.get("actions")
                last_step = {"reason": s.get("reason"), "ms": s.get("ms"), "unix_ms": s.get("unix_ms"),
                             "actions": (acts if isinstance(acts, str) else json.dumps(acts))[:400]
                             if acts else None}
        elif line.startswith("[order]"):
            o = _json_after(line, "[order]")
            if o:
                last_order = {"kind": o.get("kind"), "error": o.get("error"),
                              "army": o.get("army"), "unix_ms": o.get("unix_ms")}
    return {"last_executor_action": last_exec, "last_outside_action": last_player,
            "last_step": last_step, "last_order": last_order}


def executor_state(run_dir):
    try:
        with open(os.path.join(run_dir, "plan_state.json")) as f:
            ps = json.load(f)
    except (OSError, ValueError):
        ps = {}
    done = ps.get("done") or {}
    blocked = []
    for k, v in (ps.get("plan_blocked") or {}).items():
        if isinstance(v, dict):
            blocked.append({"item": k, "tries": v.get("tries"), "at": v.get("at"),
                            "why": (v.get("why") or v.get("reason") or "")[:160]})
    cps = (ps.get("strategy") or {}).get("checkpoints") or {}
    out = {
        "game_hour": ps.get("game_hour"),
        "operations": operations(done),
        "failures": failure_ledger(ps.get("failures")),
        "blocked": blocked[:12],
        "war_stage": (ps.get("war") or {}).get("stage") or {},
        "checkpoints_passed": sum(1 for v in cps.values() if isinstance(v, list) and len(v) > 1 and v[1]),
        "checkpoints": len(cps),
        "done_count": len(done),
    }
    out.update(server_log(run_dir))
    return out


def player_calls(run_dir, limit=8):
    rows = []
    for line in tail_lines(os.path.join(run_dir, "calls.jsonl"), 1048576):
        try:
            c = json.loads(line)
        except ValueError:
            continue
        if c.get("source") in ("plan", "recorder"):
            continue
        body = c.get("body") if isinstance(c.get("body"), dict) else {}
        err = body.get("error")
        rows.append({"utc": c.get("utc"), "tool": c.get("tool"), "error": c.get("isError"),
                     "code": err.get("code") if isinstance(err, dict) else None,
                     "args": json.dumps(c.get("args") or {}, ensure_ascii=False)[:120]})
    return rows[-limit:]
