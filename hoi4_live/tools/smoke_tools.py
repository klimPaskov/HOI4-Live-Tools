"""Smoke test of a namespace (test mode): hoi4_smoke_namespace.

Fires every event of a namespace through the console, answers each option of its window (one firing per option),
takes the decisions of a category or id prefix through the decision panel, and reads the new error.log lines after
every step. One report (Markdown and JSON under artifacts/) lists, per event and decision, what appeared, what was
answered and which error.log lines it caused.

Events: `event <id> [TAG]` runs for country and news events. State, unit leader, operative and character events need
a scope the console cannot give: they are listed as skipped. The windows are read and answered with the same code as
hoi4_fire_event / hoi4_popups, so a window that does not close is reported as stuck.
Decisions: each one is first checked with the game (allowed / visible / available through hoi4_check_trigger's
route) and only taken when those hold, so the report names the unmet condition of the others. Targeted decisions and
missions are skipped. Taking decisions needs political power: `pp` is added once with eval_effect first.
"""

import datetime
import json
import os
import re
import time

from ..errors import ToolError
from ..game import console, popups as game_popups
from ..game import trigger_check as tc
from ..gamedata import decisions as dec_data
from ..gamedata import events as ev_data
from ..gamedata import scriptdb
from ..gamedata import triggertext as tt
from ..perception import popups as popups_mod
from ..session import Cancelled as _Cancelled
from . import dispatch
from .registry import ToolResult, tool

MAX_ERRORS_SHOWN = 8


# ------------------------------------------------------------------------------------------------ events
ID_RX = re.compile(r"[A-Za-z0-9_.]+")             # what the console command takes (as hoi4_fire_event)


def _errors(session, mark):
    """The first MAX_ERRORS_SHOWN distinct new error lines (the first one is usually the cause) and the line count."""
    rows, info = session.logs.delta("error", mark, limit=500)
    return rows[:MAX_ERRORS_SHOWN], info.get("lines", 0)


def fire_once(session, ev, tag, option, wait_s):
    """Fire the event once and answer button `option` of its first window. Returns the record of this firing."""
    mark = f"smoke_{int(time.time() * 1000) % 10_000_000}"
    session.logs.mark(mark, ("error",))
    cmd = f"event {ev['id']}" + (f" {tag}" if tag else "")
    out = console.run(session, [cmd])
    session.park_cursor()
    session.sleep(wait_s)
    img = session.grab()
    found = popups_mod.detect(img)
    if not found and not ev["hidden"]:
        session.sleep(1.0)                              # a window can open late on a busy game
        img = session.grab()
        found = popups_mod.detect(img)
    fire = {"option": option, "response": out[0].get("response") if out else None, "popup": None}
    if found:
        try:
            ans = game_popups.answer(session, 0, option=option, img=img)
            fire["popup"] = {"kind": ans["kind"], "title": ans["title"], "text": (ans.get("text") or "")[:300],
                             "options": ans["options"]}
            fire["chosen_text"] = ans["chosen_text"]
            fire["closed"] = ans["closed"]
        except ToolError as exc:
            fire["answer_error"] = str(exc)
            fire["popup"] = {"options": (exc.details or {}).get("options") or [], "kind": None, "title": None}
            try:                                        # the window is still open: close it with its first option
                game_popups.answer(session, 0, option=0)
            except ToolError:
                pass
        rest = game_popups.handle_all(session, "first", phase="smoke")
        if rest:
            fire["chained"] = [r.get("title") or r.get("kind") for r in rest]
            if any(r.get("closed") is False for r in rest):
                fire["chained_not_closed"] = True
    if found:
        left = popups_mod.detect(session.grab())
        if left:
            fire["left_open"] = len(left)               # the next firing would click this window by mistake
    rows, lines = _errors(session, mark)
    fire["new_error_lines"] = lines
    if rows:
        fire["new_errors"] = rows
    return fire


def smoke_event(session, ev, tag, answer_each, wait_s):
    """Fire an event once per option. Whatever fails after the first firing is recorded on the record, so the
    firings that did run are not lost."""
    t0 = time.time()
    rec = {"id": ev["id"], "kind": "event", "type": ev["type"], "hidden": ev["hidden"], "fires": []}
    if ev["type"] not in ev_data.FIREABLE:
        rec.update(status="skipped", reason=f"{ev['type']} needs a scope the console cannot give; "
                                            f"use hoi4_eval_effect with a {ev['type']} effect")
        return rec
    if not ID_RX.fullmatch(ev["id"]):
        rec.update(status="skipped", reason="the id has characters the console command does not take")
        return rec
    try:
        first = fire_once(session, ev, tag, 0, wait_s)
        rec["fires"].append(first)
        count = len((first.get("popup") or {}).get("options") or [])
        rec["options_seen"] = count
        if answer_each and not first.get("left_open"):
            for k in range(1, count):
                session.check_cancel()
                again = fire_once(session, ev, tag, k, wait_s)
                rec["fires"].append(again)
                if again.get("left_open"):
                    break
    except _Cancelled:
        raise
    except ToolError as exc:
        rec["error"] = str(exc)
        rec["error_code"] = (exc.details or {}).get("error") or type(exc).__name__
    rec["status"], rec["notes"] = _event_status(ev, rec)
    mark_repeated_errors(rec["fires"])
    rec["seconds"] = round(time.time() - t0, 1)
    return rec


def mark_repeated_errors(fires):
    """An error in the event's `immediate` block repeats in every firing: mark the repeats, so they are not read as
    caused by each option."""
    first = {r["line"] for r in (fires[0].get("new_errors") or [])} if fires else set()
    for f in fires[1:]:
        for row in f.get("new_errors") or []:
            if row["line"] in first:
                row["repeats_first_firing"] = True


def _event_status(ev, rec):
    """ok | errors | stuck | partial | no_window. A run problem (a ToolError) counts as errors."""
    notes = []
    fires = rec["fires"]
    status = "ok"
    if rec.get("error"):
        notes.append("the run stopped on this event: " + rec["error"])
        status = "errors"
    if any(f.get("new_error_lines") for f in fires):
        notes.append("error.log gained lines")
        status = "errors"
    if any(f.get("left_open") or f.get("chained_not_closed")
           or (f.get("popup") and f.get("closed") is False) for f in fires):
        notes.append("a window stayed open after its option was clicked")
        if status == "ok":
            status = "stuck"
    if not fires:
        return status, notes
    if not ev["hidden"] and not fires[0].get("popup"):
        notes.append("no window appeared (an event the console did not fire, or a window the detector does not see)")
        if status == "ok":
            status = "no_window"
    if ev["hidden"] and fires[0].get("popup"):
        notes.append("a hidden event showed a window")
    # an option that could not be tested is not a pass
    missing = [f["option"] for f in fires[1:] if not f.get("popup")]
    short = [f["option"] for f in fires[1:] if f.get("answer_error")]
    if (missing or short) and status == "ok":
        status = "partial"
    if missing:
        notes.append("no window on the re-firing for option(s) " + ", ".join(str(m) for m in missing)
                     + (" (fire_only_once)" if ev.get("once") else ""))
    if short:
        notes.append("option(s) " + ", ".join(str(m) for m in short)
                     + " were not on the window any more (an earlier option changed the game)")
    return status, notes


# ------------------------------------------------------------------------------------------------ decisions
def decision_nodes(raw):
    """Conditions that must hold for a decision to be takable: allowed, visible, available."""
    nodes = []
    if raw is None:
        return nodes
    for key in ("allowed", "visible", "available"):
        for b in tt.child_blocks(raw, key):
            for s in tt.unwrap([b]):
                nodes.append(tc.Node(s, kind=key))
    return nodes


def _raw_decision(d, did):
    path = d.get("file")
    if not path or not os.path.isfile(path):
        return None
    try:
        return tt.find_decision(tt.resolve_constants(scriptdb.read_bytes(path).decode("utf-8", "replace")), did)
    except (OSError, tt.ScriptError):
        return None


def smoke_decision(session, did, d, tag, wait_s):
    t0 = time.time()
    rec = {"id": did, "kind": "decision", "category": d["category"]}
    if d.get("mission"):
        rec.update(status="skipped", reason="a mission: it is activated by an effect, not taken")
        return rec
    if d.get("targeted"):
        rec.update(status="skipped", reason="a targeted decision: it needs a target chosen on the map")
        return rec
    try:
        nodes = decision_nodes(_raw_decision(d, did))
        if nodes:
            rep = tc.check(session, tag, nodes, depth=2)
            rec["conditions"] = rep["result"]
            if rep["result"] is False:
                rec.update(status="not_taken", reason="a condition is unmet", unmet=tc.unmet(nodes))
                rec["seconds"] = round(time.time() - t0, 1)
                return rec
        mark = f"smoke_{int(time.time() * 1000) % 10_000_000}"
        session.logs.mark(mark, ("error",))
        out = dispatch.call(session, "hoi4_decisions", {"action": "take", "decision": did})
        text = "\n".join(c["text"] for c in out["content"] if c["type"] == "text")
        rec["take"] = {"ok": not out.get("isError"), "message": text.splitlines()[0] if text else ""}
        session.sleep(wait_s)
        rest = game_popups.handle_all(session, "first", phase="smoke")
        if rest:
            rec["windows"] = [r.get("title") or r.get("kind") for r in rest]
            if any(r.get("closed") is False for r in rest):
                rec["left_open"] = True
        if popups_mod.detect(session.grab()):
            rec["left_open"] = True
        rows, lines = _errors(session, mark)
        rec["new_error_lines"] = lines
        if rows:
            rec["new_errors"] = rows
    except _Cancelled:
        raise
    except ToolError as exc:
        rec.update(status="errors", error=str(exc), error_code=(exc.details or {}).get("error") or type(exc).__name__)
        rec["seconds"] = round(time.time() - t0, 1)
        return rec
    if rec.get("new_error_lines"):
        rec["status"] = "errors"
    elif rec.get("left_open"):
        rec["status"] = "stuck"
        rec["reason"] = "a window stayed open after the decision was taken"
    elif not rec["take"]["ok"]:
        # conditions that held but a take that failed is a problem; with no conditions to go on it is only "not taken"
        rec["status"] = "take_failed" if rec.get("conditions") is True else "not_taken"
        rec["reason"] = rec["take"]["message"]
    else:
        rec["status"] = "ok"
    rec["seconds"] = round(time.time() - t0, 1)
    return rec


# ------------------------------------------------------------------------------------------------ the tool
def select_decisions(idx, category, prefix):
    out = {}
    for did, d in idx["decisions"].items():
        if (category and d["category"] == category) or (prefix and did.startswith(prefix)):
            out[did] = d
    return dict(sorted(out.items()))


def _write_report(session, name, report):
    stamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    base = session.artifact_path(f"smoke_{name}_{stamp}")
    with open(base + ".json", "w", encoding="utf-8") as f:
        json.dump(report, f, indent=1, default=str)
    t = report["totals"]
    with open(base + ".md", "w", encoding="utf-8") as f:
        f.write(f"# Smoke test {name}\n\n{datetime.datetime.now().isoformat(timespec='seconds')}, "
                f"{report['seconds']} s, {'PASS' if report['ok'] else 'FAIL'}\n\n")
        f.write("| " + " | ".join(f"{k} {v}" for k, v in sorted(t.items())) + " |\n\n")
        f.write("| id | kind | status | options answered | new error lines | note |\n|---|---|---|---|---|---|\n")
        for r in report["events"] + report["decisions"]:
            fires = r.get("fires") or []
            lines = sum(x.get("new_error_lines", 0) for x in fires) + r.get("new_error_lines", 0)
            note = (r.get("reason") or "; ".join(r.get("notes") or []))[:140].replace("|", "/")
            f.write(f"| {r['id']} | {r['kind']} | {r['status']} | {len(fires) or ''} | {lines} | {note} |\n")
        for r in report["events"] + report["decisions"]:
            for x in (r.get("fires") or [r]):
                for row in x.get("new_errors") or []:
                    f.write(f"\n- `{r['id']}`: `{row['line']}`" + (f" (x{row['count']})" if row.get("count") else ""))
        f.write("\n")
    return base + ".md", base + ".json"


def _stop_reason(rec, stop_on_error):
    """Why the run must stop after this record, or None. A window left open ends it always: the next firing would
    click the wrong window. A game that is gone ends it too."""
    if rec.get("left_open") or rec["status"] == "stuck":
        return "a window stayed open; close it and run again"
    if rec.get("error_code") in ("GameNotRunning", "console_not_open"):
        return rec.get("error", "the game does not answer")
    if stop_on_error and rec["status"] in ("errors", "take_failed", "partial"):
        return "stop_on_error"
    return None


def _finish(session, ns, report, t0, problems):
    totals = {}
    for r in report["events"] + report["decisions"]:
        totals[r["status"]] = totals.get(r["status"], 0) + 1
    report["totals"] = totals
    report["ok"] = not any(s in totals for s in problems) and "stopped" not in report and not report.get("cancelled")
    report["seconds"] = round(time.time() - t0)
    md, js = _write_report(session, ns, report)
    report["report"], report["json"] = md, js
    return totals


@tool("hoi4_smoke_namespace",
      "Smoke test of a mod namespace (test mode). Fires every event `<namespace>.N` through the console, answers each "
      "option of its window (one firing per option, options=each), and takes the decisions of a category or id "
      "prefix through the decision panel after checking their conditions with the game. After every step the new "
      "error.log lines are read. Returns one report (Markdown and JSON under artifacts/): per event and decision "
      "the window seen, the options answered, the unmet condition of a decision that could not be taken, and the "
      "error lines. State, unit leader and character events, targeted decisions and missions are skipped with the "
      "reason. dry_run lists what would be fired without touching the game. A long namespace takes minutes (about "
      "6 s per firing); `only` and `limit` narrow it.",
      {"properties": {
          "namespace": {"type": "string", "pattern": r"^[A-Za-z0-9_]+$", "description": "event namespace, e.g. my_mod"},
          "tag": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$",
                  "description": "country the events fire for and the decisions are taken as: the player is "
                                 "switched to it first with the console `tag` command and stays switched; omit for "
                                 "the current player"},
          "what": {"type": "string", "enum": ["events", "decisions", "both"], "default": "events"},
          "decision_category": {"type": "string", "description": "decisions of this category"},
          "decision_prefix": {"type": "string", "description": "decisions whose id starts with this (default: the "
                                                               "namespace when what includes decisions)"},
          "options": {"type": "string", "enum": ["each", "first"], "default": "each",
                      "description": "each: fire again for every option; first: only the first option"},
          "only": {"type": "array", "items": {"type": "string"}, "maxItems": 200,
                   "description": "just these event or decision ids"},
          "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 60, "description": "events and "
                    "decisions to run, each, in id order"},
          "wait_s": {"type": "number", "minimum": 0.5, "maximum": 20, "default": 2.0,
                     "description": "seconds to wait for a window after firing"},
          "pp": {"type": "integer", "minimum": 0, "maximum": 100000, "default": 2000,
                 "description": "political power added once before taking decisions (0: none)"},
          "stop_on_error": {"type": "boolean", "default": False},
          "dry_run": {"type": "boolean", "default": False}},
       "required": ["namespace"]},
      family="testmode", lock="acting", test_only=True,
      examples=[{"args": {"namespace": "my_mod", "tag": "GER"}},
                {"args": {"namespace": "my_mod", "what": "both", "decision_category": "my_mod_decisions"}},
                {"args": {"namespace": "my_mod", "dry_run": True}}])
def hoi4_smoke_namespace(session, args):
    ns, tag = args["namespace"], args.get("tag")
    want_events = args["what"] in ("events", "both")
    want_dec = args["what"] in ("decisions", "both")
    only = set(args.get("only") or [])
    events = []
    if want_events:
        events = ev_data.in_namespace(ev_data.for_session(session), ns)
        if only:
            events = [e for e in events if e["id"] in only]
    decisions = {}
    if want_dec:
        didx = dec_data.for_session(session)
        decisions = select_decisions(didx, args.get("decision_category"), args.get("decision_prefix") or ns)
        if only:
            decisions = {k: v for k, v in decisions.items() if k in only}
    events, skipped_by_limit_e = events[:args["limit"]], max(0, len(events) - args["limit"])
    keys = list(decisions)[:args["limit"]]
    skipped_by_limit_d = max(0, len(decisions) - len(keys))
    decisions = {k: decisions[k] for k in keys}
    if not events and not decisions:
        raise ToolError(f"nothing to fire: no event '{ns}.N'" + (" or matching decision" if want_dec else "")
                        + " in the game files or the enabled mods", {"error": "smoke_empty", "namespace": ns})
    plan = {"events": [{"id": e["id"], "type": e["type"], "hidden": e["hidden"], "options_in_file": len(e["options"])}
                       for e in events],
            "decisions": [{"id": k, "category": v["category"]} for k, v in decisions.items()],
            "not_run_because_of_limit": {"events": skipped_by_limit_e, "decisions": skipped_by_limit_d}}
    if args["dry_run"]:
        return ToolResult(f"dry run: {len(events)} event(s), {len(decisions)} decision(s)", plan)

    if not re.fullmatch(r"[A-Za-z0-9_]+", ns) or (tag and not re.fullmatch(r"[A-Z][A-Z0-9]{2}", tag)):
        raise ToolError("namespace and tag must be plain letters, digits and underscores", {"field": "namespace"})
    t0 = time.time()
    report = {"namespace": ns, "tag": tag or "player", "events": [], "decisions": [], "plan": plan}
    total = len(events) + len(decisions)
    problems = ("errors", "stuck", "no_window", "partial", "take_failed")
    done = 0
    try:
        if tag:
            # a window only opens for the player: switch to the country first (it stays the player afterwards)
            console.run(session, [f"tag {tag}"], read=False)
            report["tag_switched"] = True
        for ev in events:
            session.check_cancel()
            done += 1
            session.report(f"event {ev['id']} ({done}/{total})", done, total)
            rec = smoke_event(session, ev, tag, args["options"] == "each", args["wait_s"])
            report["events"].append(rec)
            reason = _stop_reason(rec, args["stop_on_error"])
            if reason:
                report["stopped"] = f"{ev['id']}: {reason}"
                break
        if decisions and "stopped" not in report:
            if args["pp"]:
                console.run(session, [f"eval_effect add_political_power = {args['pp']}"], read=False)
            for i, (did, d) in enumerate(decisions.items()):
                session.check_cancel()
                done += 1
                session.report(f"decision {did} ({done}/{total})", done, total)
                if args["pp"] and i and i % 5 == 0:
                    console.run(session, [f"eval_effect add_political_power = {args['pp']}"], read=False)
                rec = smoke_decision(session, did, d, tag, args["wait_s"])
                report["decisions"].append(rec)
                reason = _stop_reason(rec, args["stop_on_error"])
                if reason:
                    report["stopped"] = f"{did}: {reason}"
                    break
    except _Cancelled:
        # a cancel keeps what was done: the report is written, then the cancel goes on
        report["cancelled"] = True
        _finish(session, ns, report, t0, problems)
        raise
    except ToolError as exc:
        report["stopped"] = f"the run could not go on: {exc}"
    totals = _finish(session, ns, report, t0, problems)
    bad = sum(totals.get(s, 0) for s in problems)
    head = (f"smoke {ns}: {'PASS' if report['ok'] else 'FAIL'}, {len(report['events'])} event(s) and "
            f"{len(report['decisions'])} decision(s) run, {bad} with problems, "
            + ", ".join(f"{v} {k}" for k, v in sorted(totals.items()))
            + (f"; stopped: {report['stopped']}" if report.get("stopped") else ""))
    return ToolResult(head, report)
