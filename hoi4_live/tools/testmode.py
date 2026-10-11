"""Test-mode tools: hoi4_console, hoi4_eval_effect, hoi4_fire_event, hoi4_logs, hoi4_save_query, hoi4_scenario.

Console-based tools are refused in campaign mode. hoi4_logs and hoi4_save_query are read-only and allowed in
every mode.
"""

import datetime
import json
import os
import re
import time

from ..errors import ToolError
from ..game import console, popups as game_popups, saves
from ..perception import popups as popups_mod
from ..state import queries
from . import dispatch
from .common import attach, result
from .registry import ToolResult, get as get_tool, tool


def _errors_since(session, mark):
    rows, info = session.logs.delta("error", mark, limit=60)
    return {"new_error_lines": info.get("lines", 0), "errors": rows[-25:]}


@tool("hoi4_console",
      "Run console commands in the game (test mode). Commands are pasted, executed in order, and the console "
      "output that appeared after each one is read back with OCR. The console is closed again afterwards and new "
      "error.log lines are reported.",
      {"properties": {
          "commands": {"type": "array", "items": {"type": "string", "minLength": 1}, "minItems": 1, "maxItems": 30},
          "read_output": {"type": "boolean", "default": True},
          "keep_open": {"type": "boolean", "default": False}},
       "required": ["commands"]},
      family="testmode", lock="acting", test_only=True,
      examples=[{"args": {"commands": ["time"]}}, {"args": {"commands": ["observe", "fow"]}}])
def hoi4_console(session, args):
    session.logs.mark("console")
    out = console.run(session, args["commands"], keep_open=args["keep_open"], read=args["read_output"])
    data = {"results": out}
    data.update(_errors_since(session, "console"))
    return result(f"ran {len(out)} command(s); {data['new_error_lines']} new error.log line(s)", **data)


@tool("hoi4_eval_effect",
      "Execute an inline effect through the console (test mode): `eval_effect <scope> = { <effect> }`, or without a "
      "scope on the player. Reports the console response and new error.log lines.",
      {"properties": {
          "effect": {"type": "string", "minLength": 1, "description": "effect script, e.g. 'add_political_power = 50'"},
          "scope": {"type": "string", "description": "country tag or state id; omit for the player"}},
       "required": ["effect"]},
      family="testmode", lock="acting", test_only=True,
      examples=[{"args": {"effect": "add_political_power = 100", "scope": "USA"}},
                {"args": {"effect": "set_country_flag = hoi4live_probe"}}])
def hoi4_eval_effect(session, args):
    eff = args["effect"].replace("\n", " ").strip()
    cmd = f"eval_effect {args['scope']} = {{ {eff} }}" if args.get("scope") else f"eval_effect {eff}"
    session.logs.mark("eval")
    out = console.run(session, [cmd])
    data = {"command": cmd, "response": out[0].get("response")}
    data.update(_errors_since(session, "eval"))
    return result(f"eval_effect sent; {data['new_error_lines']} new error.log line(s)", **data)


@tool("hoi4_fire_event",
      "Fire an event through the console (test mode): `event <id> [TAG]`. Then waits for popups, reads them (kind, "
      "title, text, options) and with answer='first' answers them. Reports new error.log lines.",
      {"properties": {
          "event": {"type": "string", "pattern": r"^[A-Za-z0-9_.]+$", "description": "event id, e.g. my_mod.1"},
          "tag": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$"},
          "answer": {"type": "string", "enum": ["none", "first"], "default": "none"},
          "wait_s": {"type": "number", "minimum": 0, "maximum": 30, "default": 2.0}},
       "required": ["event"]},
      family="testmode", lock="acting", test_only=True,
      examples=[{"args": {"event": "my_mod.1", "tag": "USA"}},
                {"args": {"event": "news.1", "answer": "first"}}])
def hoi4_fire_event(session, args):
    cmd = f"event {args['event']}" + (f" {args['tag']}" if args.get("tag") else "")
    session.logs.mark("event")
    out = console.run(session, [cmd])
    session.park_cursor()
    session.sleep(args["wait_s"])
    img = session.grab()
    found = popups_mod.detect(img)
    data = {"command": cmd, "response": out[0].get("response")}
    res = ToolResult("", data)
    if args["answer"] == "first" and found:
        data["answered"] = game_popups.handle_all(session, "first", img=img, phase="fire_event")
    else:
        data["popups"] = game_popups.describe(session, img, found)
    data.update(_errors_since(session, "event"))
    n = len(data.get("answered", data.get("popups", [])))
    res.summary = f"fired {args['event']}: {n} popup(s); {data['new_error_lines']} new error.log line(s)"
    return attach(res, session, img, "fire_event")


@tool("hoi4_logs",
      "Read the game logs (error.log, game.log, ...): the delta since a mark (session start, launch, load, or one "
      "you set), folded so repeated lines show once with a count, optionally filtered by regex. set_mark records a "
      "new mark now.",
      {"properties": {
          "log": {"type": "string", "enum": ["error", "game", "system", "setup", "text", "exceptions"], "default": "error"},
          "since": {"type": "string", "default": "session",
                    "description": "mark name: session | launch | load | console | event | eval | your own, or 'all'"},
          "grep": {"type": "string"},
          "limit": {"type": "integer", "minimum": 1, "maximum": 2000, "default": 100},
          "tail": {"type": "integer", "minimum": 1, "maximum": 2000, "description": "just the last N raw lines"},
          "set_mark": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$"}}},
      family="testmode", lock="none",
      examples=[{"args": {"log": "error", "since": "load"}}, {"args": {"set_mark": "before_fix"}},
                {"args": {"log": "game", "tail": 20}}])
def hoi4_logs(session, args):
    if args.get("set_mark"):
        m = session.logs.mark(args["set_mark"], ("error", "game", "system", "setup", "text", "exceptions"))
        return result(f"mark '{args['set_mark']}' set", mark=args["set_mark"], offsets=m)
    if args.get("tail"):
        lines = session.logs.tail(args["log"], args["tail"])
        return result(f"last {len(lines)} line(s) of {args['log']}.log", lines=lines)
    since = None if args["since"] == "all" else args["since"]
    if since and not session.logs.has_mark(since):
        raise ToolError(f"no mark named '{since}' (marks: {sorted(session.logs.marks)})", {"field": "since"})
    rows, info = session.logs.delta(args["log"], since, args.get("grep"), args["limit"])
    return result(f"{info['lines']} new line(s) in {args['log']}.log since {args['since']} ({info['unique']} unique)",
                  info=info, lines=rows)


@tool("hoi4_save_query",
      "Query a plain-text save (read only): info (player, date, version, mods), keys (children of a path), get "
      "(parsed subtree), find (regex over the raw text, hits mapped to paths), variables / flags (a country's or the "
      "global ones), diff (leaves that differ between two saves under a path). save may be a name, a path, or "
      "'latest'. Paths: countries/USA/variables, states/64, countries/*/flags.",
      {"properties": {
          "save": {"type": "string", "minLength": 1},
          "query": {"type": "string", "enum": ["info", "keys", "get", "find", "variables", "flags", "diff"], "default": "info"},
          "path": {"type": "string"},
          "tag": {"type": "string", "description": "country tag for variables/flags (omit for global)"},
          "pattern": {"type": "string", "description": "regex for find, or a name filter for variables/flags/diff"},
          "other": {"type": "string", "description": "second save for diff (the newer one)"},
          "limit": {"type": "integer", "minimum": 1, "maximum": 5000, "default": 200}},
       "required": ["save"]},
      family="testmode", lock="none",
      examples=[{"args": {"save": "hoi4live_smoke", "query": "info"}},
                {"args": {"save": "hoi4live_smoke", "query": "variables", "tag": "USA", "pattern": "my_flag"}},
                {"args": {"save": "latest", "query": "find", "pattern": "last_fired_event_id"}},
                {"args": {"save": "hoi4live_pre", "other": "hoi4live_post", "query": "diff", "path": "countries/USA"}}])
def hoi4_save_query(session, args):
    try:
        return _save_query(session, args)
    finally:
        queries.close_all()


def _save_query(session, args):
    path = queries.resolve_save(session.cfg.saves_dir, args["save"])
    q = args["query"]
    if q == "info":
        data = queries.info(path)
    elif q == "keys":
        data = queries.keys(path, args.get("path") or "")
    elif q == "get":
        if not args.get("path"):
            raise ToolError("get needs path", {"field": "path"})
        data = queries.get(path, args["path"], args["limit"])
    elif q == "find":
        if not args.get("pattern"):
            raise ToolError("find needs pattern", {"field": "pattern"})
        data = queries.find(path, args["pattern"], args.get("path"), args["limit"])
    elif q in ("variables", "flags"):
        data = queries.script_state(path, q, args.get("tag"), args.get("pattern"), args["limit"])
    else:
        if not args.get("other"):
            raise ToolError("diff needs other", {"field": "other"})
        other = queries.resolve_save(session.cfg.saves_dir, args["other"])
        data = queries.diff(path, other, args.get("path") or "", args.get("pattern"), args["limit"])
    data["save_file"] = os.path.basename(path)
    return result(f"{q} on {os.path.basename(path)}", **data)


@tool("hoi4_scenario",
      "Run a deterministic list of tool calls and write a report (Markdown + JSON) under artifacts/. Each step is "
      "{tool, args, label?, expect?, continue_on_error?}; expect.text / expect.not_text are regexes on the step's "
      "text output, expect.error=true asserts that the step fails. "
      "Images are kept as files. Holds the desktop lock for the whole run.",
      {"properties": {
          "name": {"type": "string", "pattern": "^[A-Za-z0-9_-]+$", "default": "scenario"},
          "steps": {"type": "array", "minItems": 1, "maxItems": 100, "items": {
              "type": "object",
              "properties": {
                  "tool": {"type": "string"},
                  "args": {"type": "object"},
                  "label": {"type": "string"},
                  "expect": {"type": "object", "properties": {
                      "text": {"type": "string", "description": "regex the result text must match"},
                      "not_text": {"type": "string", "description": "regex the result text must not match"},
                      "error": {"type": "boolean", "description": "true: the step must fail (refusals, guards)"}}},
                  "continue_on_error": {"type": "boolean", "default": False}},
              "required": ["tool"]}},
          "stop_on_error": {"type": "boolean", "default": True}},
       "required": ["steps"]},
      family="testmode", lock="acting",
      examples=[{"args": {"name": "smoke", "steps": [
          {"tool": "hoi4_status"},
          {"tool": "hoi4_fire_event", "args": {"event": "my_mod.1", "tag": "USA", "answer": "first"}},
          {"tool": "hoi4_time", "args": {"action": "step", "days": 10}},
          {"tool": "hoi4_save", "args": {"name": "smoke"}},
          {"tool": "hoi4_logs", "args": {"since": "session"}}]}}])
def hoi4_scenario(session, args):
    name = args["name"]
    steps = args["steps"]
    for i, s in enumerate(steps):
        if s["tool"] == "hoi4_scenario" or get_tool(s["tool"]) is None:
            raise ToolError(f"steps[{i}].tool: unknown or nested tool '{s['tool']}'", {"field": f"steps[{i}].tool"})
    rows = []
    t_all = time.time()
    failed = False
    for i, s in enumerate(steps):
        session.check_cancel()
        label = s.get("label") or s["tool"]
        session.report(f"step {i + 1}/{len(steps)}: {label}", i + 1, len(steps))
        t0 = time.time()
        mcp = dispatch.call(session, s["tool"], s.get("args") or {})
        text = "\n".join(c["text"] for c in mcp["content"] if c["type"] == "text")
        exp = s.get("expect") or {}
        ok = bool(mcp.get("isError")) if exp.get("error") else not mcp.get("isError")
        if ok and exp.get("text") and not re.search(exp["text"], text, re.I | re.S):
            ok = False
            text += f"\n[expectation failed: text !~ {exp['text']}]"
        if ok and exp.get("not_text") and re.search(exp["not_text"], text, re.I | re.S):
            ok = False
            text += f"\n[expectation failed: text ~ {exp['not_text']}]"
        sc = mcp.get("structuredContent") or {}
        rows.append({"step": i + 1, "label": label, "tool": s["tool"], "args": s.get("args") or {}, "ok": ok,
                     "seconds": round(time.time() - t0, 1), "summary": text.splitlines()[0] if text else "",
                     "screenshots": sc.get("screenshots", []), "result": sc})
        if not ok and not s.get("continue_on_error"):
            failed = True
            if args["stop_on_error"]:
                break
    stamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    base = session.artifact_path(f"scenario_{name}_{stamp}")
    with open(base + ".json", "w", encoding="utf-8") as f:
        json.dump({"name": name, "steps": rows, "ok": not failed}, f, indent=1, default=str)
    with open(base + ".md", "w", encoding="utf-8") as f:
        f.write(f"# Scenario {name}\n\n{datetime.datetime.now().isoformat(timespec='seconds')}, "
                f"{round(time.time() - t_all)} s, {'PASS' if not failed else 'FAIL'}\n\n")
        f.write("| # | step | tool | ok | s | summary |\n|---|---|---|---|---|---|\n")
        for r in rows:
            summ = r["summary"].replace("|", "/")[:160]
            f.write(f"| {r['step']} | {r['label']} | {r['tool']} | {'yes' if r['ok'] else 'NO'} | {r['seconds']} | {summ} |\n")
        f.write("\n")
        for r in rows:
            for sh in r["screenshots"]:
                f.write(f"- step {r['step']} screenshot: `{sh}`\n")
    return result(f"scenario {name}: {'PASS' if not failed else 'FAIL'} ({len(rows)}/{len(steps)} steps)",
                  ok=not failed, report=base + ".md", json=base + ".json",
                  steps=[{k: r[k] for k in ("step", "label", "ok", "seconds", "summary")} for r in rows])
