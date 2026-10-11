"""Phase 5, the campaign loop: hoi4_advance (run time until something needs the agent) and hoi4_journal (the
campaign log and its run report). Phase 3 and 4 tools do the acting; this module decides when to stop running time,
what to answer on its own, and keeps the record.

Every verified campaign action (a tool of the campaign or military family that reports changed=true) is also
written to the journal by journal_action() below, which the dispatcher calls after the tool returns.
"""

import datetime
import os
import re
import time

from ..errors import ToolError
from ..game import clock as clock_mod
from ..game import timectl
from ..game.popups import PopupPolicy
from ..perception import topbar
from ..state import advance as adv
from ..state import journal as journal_mod
from .registry import ToolResult, tool

MONTHS = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"]
MIN_ACTION_TEXT = 8
LOGGED_FAMILIES = ("campaign", "military")
NOT_LOGGED_ARGS = ("screen", "screenshot", "keep_open", "limit", "save", "rows", "filter")


def parse_date(text):
    """'1936.3.1', '1936-03-01', '1 Mar 1936', '1 March 1936' -> (year, month, day), or None."""
    t = (text or "").strip()
    m = re.match(r"^(\d{4})[.\-/](\d{1,2})[.\-/](\d{1,2})(?:[.\-/ ]\d{1,2})?$", t)
    if m:
        return int(m.group(1)), int(m.group(2)), int(m.group(3))
    m = re.match(r"^(\d{1,2})\s+([A-Za-z]{3})[a-z]*\.?,?\s+(\d{4})$", t)
    if m and m.group(2).lower() in MONTHS:
        return int(m.group(3)), MONTHS.index(m.group(2).lower()) + 1, int(m.group(1))
    return None


def days_until(now_text, target):
    """Whole days from the in-game date text (1936.1.8.12) to target (y, m, d); None when unreadable."""
    try:
        p = [int(x) for x in str(now_text).split(".")]
        a = datetime.date(p[0], p[1], p[2])
        b = datetime.date(*target)
    except (ValueError, IndexError, TypeError):
        return None
    return (b - a).days


def journal_for(session, tag=None):
    tag = tag or session.telemetry.player() or getattr(session, "last_tag", None)
    return journal_mod.Journal(os.path.join(session.cfg.artifacts_dir, "journal"), tag)


def current_date(session):
    """The in-game date text: the top bar when the game is on screen (right after a save was loaded the
    telemetry still describes the timeline that was left), else the telemetry's latest day, else None."""
    d = session.telemetry.day()
    if d and d.get("_date") and not getattr(session, "date_uncertain", False):
        return d["_date"]
    try:
        d2 = topbar.read_date(session.grab(focus=False), session.ocr)
        if "error" not in d2 and d2.get("hoi4"):
            return d2["hoi4"]
    except Exception:
        pass
    return d.get("_date") if d else None


def journal_action(session, name, family, args, result):
    """Dispatcher hook: record a verified campaign action. Never raises."""
    try:
        data = result.data if isinstance(result.data, dict) else {}
        if family not in LOGGED_FAMILIES or result.is_error or not data.get("changed"):
            return
        a = {k: v for k, v in (args or {}).items() if k not in NOT_LOGGED_ARGS and v not in (None, "", [], False)}
        text = (result.summary or "").strip().splitlines()[0][:300] if result.summary else ""
        if len(text) < MIN_ACTION_TEXT:
            text = f"{name} {a.get('action', '')}".strip()
        journal_for(session, (args or {}).get("tag")).add("action", text, current_date(session),
                                 {"tool": name, "args": a, "verified_by": data.get("verified_by")})
    except Exception:           # the journal must never break an action
        pass


# ---------------------------------------------------------------------------------------------- hoi4_advance
def _hours(date_text):
    try:
        p = [int(x) for x in str(date_text).split(".")]
        # HOI4 writes hour 24 for the last hour of a day: add the hours as a delta
        d = datetime.datetime(p[0], p[1], p[2]) + datetime.timedelta(hours=p[3] if len(p) > 3 else 0)
        return int((d - datetime.datetime(1900, 1, 1)).total_seconds() // 3600)
    except (ValueError, IndexError):
        return None


@tool("hoi4_advance",
      "Run game time until something needs a decision, then say what and what to do about it. Time runs in short "
      "chunks; after each the telemetry's daily numbers are compared with the start and the run stops on the first "
      "stop_on condition (a technology finished, a focus completed, free civilian or military factories, a war "
      "started or ended, states lost or gained, surrender progress, low stability, high political power, low fuel, "
      "a change at the front) or when `days` or the date `until` is reached. Popups are answered by the policy "
      "(popups: choices = news and one-option events are answered, an event with a real choice stops the run and is "
      "returned with its text and options for the agent to answer; rules can pre-decide events by title or text). "
      "Fast runs keep the simulation running when they return for routine events or deadlines. The result lists why it returned, what changed, the popups answered and still open, the attention items and "
      "the tool calls that answer them; each run is written to the journal. Without the telemetry mod only the date, "
      "popups and focus notices stop a run.",
      {"properties": {
          "days": {"type": "integer", "minimum": 1, "maximum": 730,
                   "description": "run at most this many days (default 14, or up to `until`)"},
          "until": {"type": "string", "description": "stop at this date: 1936.3.1, 1936-03-01 or '1 Mar 1936'"},
          "stop_on": {"type": "array", "items": {"type": "string", "enum": list(adv.STOP_CONDITIONS)},
                      "description": "conditions that stop the run ([] = only the date, popups and errors); "
                                     "default: " + ", ".join(adv.DEFAULT_STOP)},
          "thresholds": {"type": "object", "additionalProperties": False, "properties": {
              "losing": {"type": "number", "minimum": 0, "maximum": 1, "description": "surrender progress (0.2)"},
              "low_stability": {"type": "number", "minimum": 0, "maximum": 1, "description": "stability (0.4)"},
              "pp_high": {"type": "number", "minimum": 0, "description": "political power (150)"},
              "low_fuel": {"type": "number", "minimum": 0, "maximum": 1, "description": "fuel ratio (0.1)"}}},
          "popups": {"type": "string", "enum": ["choices", "first", "stop", "ignore"], "default": "choices"},
          "popup_rules": {"type": "array", "items": {"type": "object", "additionalProperties": False, "properties": {
              "match": {"type": "string", "description": "regex on the popup's title and text"},
              "option": {"description": "option index, or a regex on the option's text"},
              "stop": {"type": "boolean", "description": "hand this popup to the agent"}},
              "required": ["match"]},
              "description": "first matching rule decides; the popups policy handles the rest"},
          "speed": {"type": "integer", "minimum": 1, "maximum": 5, "default": 5},
          "chunk_days": {"type": "integer", "minimum": 1, "maximum": 30, "default": 2,
                         "description": "days between looks at the numbers (a stop is noticed at the end of a chunk)"},
          "max_seconds": {"type": "integer", "minimum": 30, "maximum": 7200, "default": 900,
                          "description": "wall-clock limit for the whole call"},
          "situation": {"type": "boolean", "default": False,
                        "description": "attention items from the full briefing (reads the newest save) instead of "
                                       "the telemetry alone"},
          "journal": {"type": "boolean", "default": True, "description": "write the run and answered events to the journal"},
          "screenshot": {"type": "boolean", "default": False},
          "background": {"type": "boolean", "default": False,
                         "description": "start the run and return at once: the game keeps running while you act "
                                        "(research, production, construction ...); collect the result with "
                                        "hoi4_clock action=join"},
          "engine": {"type": "string", "enum": ["auto", "fast", "screen"], "default": "auto",
                     "description": "fast: telemetry-driven clock (no OCR in the loop); screen: the older "
                                    "chunked loop that reads the date by OCR (no telemetry needed); auto: fast "
                                    "when telemetry day records exist"}}},
      family="campaign_loop", lock="acting", phase=5,
      examples=[{"args": {"days": 30}, "note": "a month, stopping at the first decision point"},
                {"args": {"days": 90, "stop_on": ["war_started", "losing"], "popups": "first"},
                 "note": "quiet period: only wars and a losing war interrupt"},
                {"args": {"until": "1 Jan 1937", "popup_rules": [{"match": "Earhart", "option": "No"}]},
                 "note": "run to a date; one event decided in advance"}])
def hoi4_advance(session, args):
    stop_on = list(args["stop_on"]) if args.get("stop_on") is not None else list(adv.DEFAULT_STOP)
    policy = PopupPolicy.parse(args["popups"], args.get("popup_rules"))
    store = session.telemetry
    tag0 = store.player()
    if args.get("engine", "auto") == "fast" or (args.get("engine", "auto") == "auto" and tag0 and store.day(tag0)):
        return _advance_fast(session, args, policy, stop_on)
    if args.get("background"):
        raise ToolError("background runs need the telemetry day records (engine fast)", {"field": "background"})
    if session.mode == "campaign":
        raise ToolError("campaign advances require fresh telemetry; the legacy screen loop pauses between chunks",
                        {"error": "capability_unavailable", "capability": "live advance",
                         "remedy": "Enable observational telemetry before launching, then run the clock normally until a day record arrives"})
    return _advance_screen(session, args, policy, stop_on)


def _budget(args, start_date):
    budget_days = args.get("days") or 14
    if args.get("until"):
        target = parse_date(args["until"])
        if not target:
            raise ToolError(f"cannot read the date '{args['until']}' (use 1936.3.1 or '1 Mar 1936')", {"field": "until"})
        left = days_until(start_date, target)
        if left is None or left <= 0:
            raise ToolError(f"{args['until']} is not after the current date {start_date}", {"field": "until"})
        budget_days = min(args["days"], left) if args.get("days") else left
    return budget_days


def _advance_fast(session, args, policy, stop_on):
    cur = getattr(session, "clock", None)
    if cur and not cur.done.is_set():
        raise ToolError("an advance is already running in the background (hoi4_clock action=join or stop)",
                        {"state": "running", "progress": cur.progress})
    store = session.telemetry
    # the screen's date says which timeline the game is on: days logged after it belong to a game that was left
    try:
        # Only this server's launch can use memory. The cheap clock reader avoids
        # both country traversal and an OCR pause at the start of each advance.
        memory_date = session.memory_snapshot(clock_only=True).get("date") if getattr(session, "launched_identity", None) else None
    except ToolError:
        memory_date = None  # unknown builds retain the existing screen fallback
    d0 = {"hoi4": memory_date} if memory_date else timectl.read_date(session)[0]
    start_text = d0.get("hoi4") if "error" not in d0 else None
    if start_text:
        store.drop_after(start_text)
        session.date_uncertain = False
    rec0 = store.day(store.player())
    if not rec0 and not start_text:
        raise ToolError("no telemetry day record yet and the date is unreadable (is the game on the map? "
                        "hoi4_telemetry action=enable and a restart are needed once)")
    a = dict(args)
    a["start_date"] = start_text or rec0["_date"]
    a["budget_days"] = _budget(args, a["start_date"])
    a["stop_on_used"] = stop_on
    a["stop_on"] = stop_on
    from ..state import standing
    governor = getattr(session, "governor", None)
    managed = governor is not None and governor.active()
    run = clock_mod.AdvanceRun(session, a, policy,
                              on_reasons=None if managed else standing.hook(session),
                              on_day=governor.on_day if managed else None)
    session.clock = run
    if args.get("background"):
        run.start()
        return ToolResult(f"advance running in the background from {a['start_date']} for up to {a['budget_days']} days "
                          "(hoi4_clock action=join collects it, action=stop ends it)",
                          {"background": True, "start": a["start_date"], "budget_days": a["budget_days"],
                           "stop_on": stop_on})
    try:
        res = run.execute()
    finally:
        run.done.set()
    run.collected = True
    return _finish(session, args, res, run)


def _finish(session, args, res, run=None):
    data = res["data"]
    if args.get("journal", True):
        try:
            j = journal_for(session, res.get("tag"))
            j.add("advance", res["summary"], res["end_date"], {"stopped_by": [r["code"] for r in res["reasons"]],
                                                                "changes": res["changes"], "days": res["days"]})
            for p in res["answered"]:
                j.add("popup", f"{p.get('title')}: {p.get('chosen_text')}", p.get("date") or res["end_date"])
        except OSError as e:
            data["journal_error"] = str(e)
    out = ToolResult(res["summary"], data)
    if args.get("screenshot"):
        from .common import attach
        attach(out, session, session.grab(), "advance", force=True)
    return out


@tool("hoi4_clock",
      "The background advance. status: date, whether a background run goes on and its progress, from the telemetry "
      "(no screen). join: wait up to wait_s for a background hoi4_advance to stop and return its result (the same "
      "as a foreground hoi4_advance); while it still runs the call returns its status. stop: end the run now (the "
      "game is paused) and return its result. pause / resume: the game clock alone.",
      {"properties": {
          "action": {"type": "string", "enum": ["status", "join", "stop", "pause", "resume"], "default": "status"},
          "wait_s": {"type": "integer", "minimum": 0, "maximum": 3600, "default": 60}}},
      family="campaign_loop", lock="none", phase=7,
      examples=[{"args": {"action": "status"}}, {"args": {"action": "join", "wait_s": 120}}])
def hoi4_clock(session, args):
    run = getattr(session, "clock", None)
    a = args["action"]
    store = session.telemetry
    rec = store.day(store.player())
    status = {"date": rec["_date"] if rec else None, "n": rec["n"] if rec else None,
              "age_s": round(time.time() - rec["_wall"], 1) if rec else None,
              "running_in_background": bool(run and not run.done.is_set())}
    if run and not run.done.is_set():
        status["progress"] = dict(run.progress)
    if a == "status":
        return ToolResult(f"clock: {status['date']}" + ("; a background advance is running"
                                                         if status["running_in_background"] else ""), status)
    if a in ("pause", "resume"):
        with session.lock.hold("clock"), session.ui_mutex:
            ok = clock_mod.set_running(session, a == "resume")
        return ToolResult(f"clock {'running' if a == 'resume' else 'paused'}" if ok else "the clock did not change",
                          dict(status, changed=ok))
    if not run:
        raise ToolError("no advance has been started in this session")
    if a == "stop":
        run.stop()
    run.join(args["wait_s"] if a == "join" else 60)
    if not run.done.is_set():
        return ToolResult(f"the background advance is still running ({status['date']})", status)
    if run.error:
        err, run.error = run.error, None
        raise err
    if getattr(run, "collected", False):
        return ToolResult("that advance was already collected", status)
    run.collected = True
    return _finish(session, run.args, run.result, run)


def _advance_screen(session, args, policy, stop_on):
    thresholds = args.get("thresholds") or {}
    d0, _ = timectl.read_date(session)
    if "error" in d0:
        raise ToolError("cannot read the in-game date (no game loaded, or the interface is hidden)", d0)
    start_date = d0["hoi4"]
    budget_days = args.get("days") or 14
    if args.get("until"):
        target = parse_date(args["until"])
        if not target:
            raise ToolError(f"cannot read the date '{args['until']}' (use 1936.3.1 or '1 Mar 1936')", {"field": "until"})
        left = days_until(start_date, target)
        if left is None or left <= 0:
            raise ToolError(f"{args['until']} is not after the current date {start_date}", {"field": "until"})
        budget_days = min(args["days"], left) if args.get("days") else left
    store = session.telemetry                  # refreshed on every access: take it again after each chunk
    tag = store.player()
    first = adv.watch(store.day(tag)) if tag else None
    prev = first
    run = {"chunks": 0, "hours": 0, "answered": [], "left_open": [], "notices": [], "reasons": []}
    t0 = time.time()
    hours_budget = budget_days * 24
    last_end = start_date
    while run["hours"] < hours_budget:
        session.check_cancel()
        session.report(f"advance: {last_end} ({run['hours'] // 24} of {budget_days} days)")
        chunk = max(1, min(args["chunk_days"] * 24, hours_budget - run["hours"]))
        rep = timectl.run_days(session, 0, chunk, policy, args["speed"],
                               max(30, int(args["max_seconds"] - (time.time() - t0))))
        run["chunks"] += 1
        run["hours"] += rep.get("elapsed_hours") or 0
        last_end = rep.get("end") or last_end
        notices = []
        for p in rep.get("popups", []):
            if p.get("kind") == "notice":
                run["notices"].append({"title": p.get("title"), "date": p.get("date")})
                notices.append(p.get("title"))
            elif p.get("answered"):
                run["answered"].append({k: p.get(k) for k in ("kind", "title", "chosen_text", "why", "date")})
            else:
                run["left_open"].append(p)
        reasons = []
        if rep.get("stopped_on_popup") and run["left_open"]:
            reasons.append({"code": "popup", "text": f"{len(run['left_open'])} event(s) wait for an answer"})
        store = session.telemetry
        tag = tag or store.player()
        cur = adv.watch(store.day(tag)) if tag else None
        if first is None and cur:
            first = cur
            prev = cur
        reasons += adv.evaluate(prev, cur, stop_on, thresholds, notices)
        if cur:
            prev = cur
        if reasons:
            run["reasons"] = reasons
            break
        if rep.get("timeout"):
            run["reasons"] = [{"code": "time_limit", "text": f"the {args['max_seconds']} s wall-clock limit was reached"}]
            break
        if not (rep.get("elapsed_hours") or 0) and not rep.get("stopped_on_popup"):
            run["reasons"] = [{"code": "no_progress", "text": "the game clock did not move"}]
            break
    if not run["reasons"]:
        run["reasons"] = [{"code": "done", "text": f"{budget_days} day(s) run" +
                           (f" to {args['until']}" if args.get("until") else "")}]
    end_d, _ = timectl.read_date(session)
    end_date = end_d.get("hoi4") if "error" not in end_d else last_end
    days_run = round(((_hours(end_date) or 0) - (_hours(start_date) or 0)) / 24.0, 1) if _hours(end_date) else None
    changes = adv.diff(first, prev)
    # attention: from the full briefing when asked (reads the newest save), else from the telemetry numbers
    attention = []
    if args["situation"]:
        from ..state import assemble, briefing as briefing_mod
        sections = ["overview", "politics", "economy", "military", "diplomacy", "focus", "research"]
        try:
            m = assemble.collect(session, tag, sections, "latest", False)
            m.pop("_raw", None)
            attention = briefing_mod.attention(m)
        except ToolError as e:
            attention = adv.telemetry_attention(prev)
            run["situation_error"] = str(e)
    else:
        attention = adv.telemetry_attention(prev)
    sugg = adv.suggestions(attention)
    why = "; ".join(r["text"] for r in run["reasons"][:3])
    summary = (f"advanced {start_date} -> {end_date} ({days_run if days_run is not None else '?'} days): "
               f"{run['reasons'][0]['code']}: {why}")
    data = {"start": start_date, "end": end_date, "days_run": days_run, "stopped_by": run["reasons"],
            "changes": changes, "answered": run["answered"], "events_waiting": run["left_open"],
            "notices": run["notices"], "attention": attention, "suggestions": sugg,
            "telemetry": bool(first or prev), "chunks": run["chunks"], "stop_on": stop_on,
            "popup_policy": policy.to_dict()}
    if not (first or prev):
        data["note"] = ("no telemetry numbers: only popups, focus notices and the date can stop a run "
                        "(hoi4_telemetry action=enable, then restart the game)")
    if args["journal"]:
        try:
            j = journal_for(session, tag)
            j.add("advance", summary, end_date, {"stopped_by": [r["code"] for r in run["reasons"]],
                                                  "changes": changes, "days": days_run})
            for p in run["answered"]:
                j.add("popup", f"{p.get('title')}: {p.get('chosen_text')}", p.get("date") or end_date)
        except OSError as e:
            data["journal_error"] = str(e)
    res = ToolResult(summary, data)
    if args["screenshot"]:
        from .common import attach
        attach(res, session, session.grab(), "advance")
    return res


# ---------------------------------------------------------------------------------------------- hoi4_journal
@tool("hoi4_journal",
      "The campaign journal of the country being played (artifacts/journal/journal_<TAG>.jsonl). add: write a note, "
      "plan, decision (a choice and why) or lesson, stamped with the in-game date. read: the newest entries, "
      "filtered by kind, date or a regex; the server itself writes `action` (every verified campaign action), "
      "`advance` (each hoi4_advance run) and `popup` (answered events) entries. plan: the standing plan (the newest "
      "plan entry). report: a markdown run report of the campaign so far.",
      {"properties": {
          "action": {"type": "string", "enum": ["add", "read", "plan", "report", "summary"], "default": "read"},
          "kind": {"type": "string", "enum": list(journal_mod.KINDS), "default": "note",
                   "description": "add: the entry kind (note, plan, decision, lesson)"},
          "text": {"type": "string", "description": "add: the entry"},
          "tags": {"type": "array", "items": {"type": "string"}},
          "kinds": {"type": "array", "items": {"type": "string", "enum": list(journal_mod.KINDS)},
                    "description": "read: only these kinds"},
          "since": {"type": "string", "description": "read: entries from this in-game date on (1936.3.1)"},
          "query": {"type": "string", "description": "read: regex over text and tags"},
          "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 20},
          "auto": {"type": "boolean", "default": True, "description": "read: include the entries the server wrote"},
          "date": {"type": "string", "description": "add: in-game date (default: the telemetry's latest day)"},
          "tag": {"type": "string", "pattern": "^[A-Z][A-Z0-9]{2}$", "description": "default: the player"}}},
      family="campaign_loop", lock="none", phase=5,
      examples=[{"args": {"action": "add", "kind": "plan", "text": "Industry first: 6 factories, then Army reform."}},
                {"args": {"action": "add", "kind": "decision",
                          "text": "Declined the Mexican oil concession: we need the political power for the draft law."}},
                {"args": {"action": "read", "limit": 10, "auto": False}},
                {"args": {"action": "report"}}])
def hoi4_journal(session, args):
    j = journal_for(session, args.get("tag"))
    a = args["action"]
    if a == "add":
        if not (args.get("text") or "").strip():
            raise ToolError("add needs text", {"field": "text"})
        if args["kind"] in journal_mod.AUTO_KINDS:
            raise ToolError(f"{args['kind']} entries are written by the server; add a note, plan, decision or lesson",
                            {"field": "kind"})
        e = j.add(args["kind"], args["text"], args.get("date") or current_date(session), None, args.get("tags"))
        return ToolResult(f"journal {j.tag}: {e['kind']} added" + (f" ({journal_mod.fmt_date(e['date'])})"
                                                                    if e.get("date") else ""),
                          {"entry": e, "path": j.path})
    if a == "plan":
        e = j.latest("plan")
        return ToolResult(e["text"] if e else "no plan yet (hoi4_journal action=add kind=plan)",
                          {"plan": e, "path": j.path})
    if a == "summary":
        s = j.summary()
        return ToolResult(f"journal {j.tag}: {s['entries']} entries", dict(s, path=j.path))
    if a == "report":
        md = journal_mod.report(j)
        return ToolResult(md, {"path": j.path, "entries": j.summary()["entries"]})
    ents = j.read(args["limit"], args.get("kinds"), args.get("since"), args.get("query"), args["auto"])
    text = "\n".join(journal_mod.line(e) for e in ents) or "the journal is empty"
    return ToolResult(f"journal {j.tag}: {len(ents)} entr{'y' if len(ents) == 1 else 'ies'}\n{text}",
                      {"entries": ents, "path": j.path})


# ---------------------------------------------------------------------------------------------- hoi4_standing
CALL = {"type": "object", "properties": {"tool": {"type": "string"}, "args": {"type": "object"}},
        "required": ["tool"]}


@tool("hoi4_standing",
      "Standing orders: decisions you declare once and the clock executes when their situation comes up, so game "
      "time never waits for your next turn. A rule says WHEN (research_idle, tech_done, focus_done, "
      "construction_done, production_idle, pp_high, low_stability, war_started, ...) and WHAT: `do` (tool calls run "
      "every time) or `queue` (a list of calls; each firing runs the next entry, so you write your priorities "
      "once; entries may be lists of calls). They run inside hoi4_advance (also a background one) while it "
      "watches the telemetry; time keeps running unless pause=true is explicitly requested. An empty queue or a failed call "
      "stops the run with standing_queue_empty / standing_failed and says why. set with append=true adds queue "
      "entries to an existing rule. Orders execute decisions; they choose nothing.",
      {"properties": {
          "action": {"type": "string", "enum": ["list", "set", "remove", "clear"], "default": "list"},
          "name": {"type": "string", "pattern": "^[A-Za-z0-9_-]{1,40}$"},
          "when": {"type": "array", "items": {"type": "string"}, "description": "stop-condition codes"},
          "do": {"type": "array", "items": CALL, "description": "calls run every time the rule fires"},
          "queue": {"type": "array", "items": {"anyOf": [CALL, {"type": "array", "items": CALL}]},
                    "description": "one entry (a call or a list of calls) per firing"},
          "append": {"type": "boolean", "default": False, "description": "set: add `queue` entries to the rule"},
          "pause": {"type": "boolean", "default": False, "description": "pause for a bounded action that requires it"},
          "max_fires": {"type": "integer", "minimum": 1},
          "on_fail": {"type": "string", "enum": ["stop", "skip"], "default": "stop",
                      "description": "queue rules: stop the run on a failed call (default), or skip the entry "
                                     "(kept under failed) and try the next in the same firing"},
          "min_gap_days": {"type": "integer", "minimum": 0, "maximum": 365, "default": 0}}},
      family="campaign_loop", lock="none", phase=7,
      examples=[{"args": {"action": "set", "name": "research", "when": ["research_idle"], "queue": [
          {"tool": "hoi4_research", "args": {"action": "start", "tech": "basic_machine_tools"}},
          {"tool": "hoi4_research", "args": {"action": "start", "tech": "construction1"}}]}},
                {"args": {"action": "list"}}, {"args": {"action": "remove", "name": "research"}}])
def hoi4_standing(session, args):
    from ..state import standing
    st = standing.get(session)
    a = args["action"]
    if a == "set":
        name = args.get("name")
        if args["append"] and name in st.rules and "queue" in st.rules[name]:
            st.extend(name, args.get("queue") or [])
        else:
            st.set(name, args.get("when"), args.get("do"), args.get("queue"), args["pause"], args.get("max_fires"),
                   args["min_gap_days"], on_fail=args.get("on_fail", "stop"))
    elif a == "remove":
        if not st.remove(args.get("name")):
            raise ToolError(f"no standing order '{args.get('name')}'", {"field": "name"})
    elif a == "clear":
        st.clear()
    rows = st.listing()
    text = "; ".join(f"{r['name']} on {','.join(r['when'])}" + (f" ({r['queue_left']} left)" if "queue_left" in r else "")
                     for r in rows) or "none"
    return ToolResult(f"standing orders: {text}", {"orders": rows, "changed": a != "list"})
