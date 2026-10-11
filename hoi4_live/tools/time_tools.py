"""Time and popups: hoi4_time and hoi4_popups."""

from ..errors import ToolError
from ..game import popups as game_popups
from ..game import timectl
from ..perception import popups as popups_mod
from .common import attach, result
from .registry import ToolResult, tool


@tool("hoi4_time",
      "Game time. status: date and whether time runs. pause / run: set the state (verified by the clock). "
      "speed: 1-5. step: run `days`/`hours` forward and stop paused; popups that appear are handled by `popups` "
      "(first = answer each with its first option, choices = answer news and one-option events, stop at a real choice, stop = stop and report them, ignore). In test mode step arms "
      "`pause_in_hours` so the game stops itself on time. Returns every popup seen with title, text and options.",
      {"properties": {
          "action": {"type": "string", "enum": ["status", "pause", "run", "speed", "step"], "default": "status"},
          "days": {"type": "integer", "minimum": 0, "maximum": 3650, "default": 0},
          "hours": {"type": "integer", "minimum": 0, "maximum": 8760, "default": 0},
          "speed": {"type": "integer", "minimum": 1, "maximum": 5},
          "popups": {"type": "string", "enum": ["first", "choices", "stop", "ignore"], "default": "first"},
          "max_seconds": {"type": "integer", "minimum": 10, "maximum": 3600, "default": 600},
          "screenshot": {"type": "boolean", "default": True}}},
      family="time", lock="acting",
      examples=[{"args": {"action": "status"}}, {"args": {"action": "step", "days": 10}},
                {"args": {"action": "step", "days": 30, "speed": 4, "popups": "stop"}},
                {"args": {"action": "pause"}}])
def hoi4_time(session, args):
    a = args["action"]
    if a == "status":
        running, d, ev = timectl.probe_running(session)
        if running is None:
            raise ToolError("cannot read the in-game date (no game loaded, or the interface is hidden)", ev)
        from ..perception import topbar
        speed = topbar.speed_level(session.grab())
        return result(f"{d['text']} ({'running' if running else 'paused'}, speed {speed})", date=d["hoi4"],
                      text=d["text"], running=running, speed=speed)
    if a == "pause":
        d = timectl.set_paused(session, True)
        return result(f"paused at {d['text']}", date=d["hoi4"], running=False)
    if a == "run":
        if args.get("speed"):
            timectl.set_speed(session, args["speed"])
        d = timectl.set_paused(session, False)
        return result(f"running from {d['text']}", date=d["hoi4"], running=True)
    if a == "speed":
        if not args.get("speed"):
            raise ToolError("speed needs speed (1-5)", {"field": "speed"})
        s = timectl.set_speed(session, args["speed"])
        return result(f"speed set to {s}", speed=s)
    if args["days"] == 0 and args["hours"] == 0:
        raise ToolError("step needs days or hours", {"field": "days"})
    rep = timectl.run_days(session, args["days"], args["hours"], args["popups"], args.get("speed"),
                           args["max_seconds"])
    res = ToolResult(f"ran {rep['start']} -> {rep['end']} ({rep['elapsed_hours']} h); "
                     f"{len(rep['popups'])} popup(s) handled", rep)
    if args["screenshot"]:
        attach(res, session, session.grab(), "after_step")
    return res


@tool("hoi4_popups",
      "Event, news and super-event windows on screen. list: kinds, boxes and option buttons (fast, no OCR). read: "
      "also OCR title, text and option labels. answer: click one option (popup index, option index or option_text "
      "regex) and verify the window closed. answer_all: answer every popup with its first option. Screenshots of "
      "answered popups are saved.",
      {"properties": {
          "action": {"type": "string", "enum": ["list", "read", "answer", "answer_all"], "default": "read"},
          "popup": {"type": "integer", "minimum": 0, "default": 0},
          "option": {"type": "integer", "minimum": 0, "default": 0},
          "option_text": {"type": "string"},
          "screenshot": {"type": "boolean", "default": False}}},
      family="popups", lock="self",
      examples=[{"args": {"action": "read"}}, {"args": {"action": "answer", "option": 1}},
                {"args": {"action": "answer", "option_text": "near miss"}}, {"args": {"action": "answer_all"}}])
def hoi4_popups(session, args):
    a = args["action"]
    if a in ("list", "read"):
        img = session.grab(focus=False)
        found = popups_mod.detect(img)
        if a == "read":
            data = game_popups.describe(session, img, found)
        else:
            data = [p.to_dict(i) for i, p in enumerate(found)]
        res = ToolResult(f"{len(data)} popup(s)", {"popups": data})
        if args["screenshot"]:
            attach(res, session, img, "popups")
        return res
    with session.lock.hold(f"hoi4_popups {a}"):
        if a == "answer":
            rec = game_popups.answer(session, args["popup"], args["option"], args.get("option_text"))
            res = ToolResult(f"answered '{rec.get('title')}' with '{rec.get('chosen_text')}' "
                             f"({'closed' if rec['closed'] else 'STILL OPEN'})", rec)
        else:
            recs = game_popups.handle_all(session, "first", phase="answer_all")
            res = ToolResult(f"answered {len(recs)} popup(s)", {"answered": recs})
        if args["screenshot"]:
            attach(res, session, session.grab(), "after_answer")
        return res
