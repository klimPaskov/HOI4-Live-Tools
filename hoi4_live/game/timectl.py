"""Game time: read the date, pause/unpause, set the speed, and run a number of days while capturing popups.

Running state is decided from the screen: the top-bar date (which shows the hour) changes within a second at any
speed while time runs. The pixel cues in perception.topbar (stripes, speed icon) are used when calibrated.

run_days() in test mode arms the console command `pause_in_hours N` so the game stops itself exactly on time;
in campaign mode (no console) it polls the date and pauses when the target is reached. Popups that appear
while time runs are handled by the popup policy (see game.popups).
"""

import time

from ..errors import ToolError
from ..perception import popups as popups_mod
from ..perception import topbar
from . import console


def read_date(session, img=None):
    img = session.grab() if img is None else img
    d = topbar.read_date(img, session.ocr)
    return d, img


def probe_running(session, wait=0.9, tries=5):
    """(running: bool|None, date, evidence). The speed icon decides: a green play arrow while time runs, an
    orange pause sign over hazard stripes while paused. A frame whose date cannot be read (the clock changes
    under the capture) is retried; only when the icon stays ambiguous is the clock compared across `wait` s."""
    last = None
    d = None
    for _ in range(tries):
        img = session.grab()
        state, ev = topbar.pause_state(img)
        d = topbar.read_date(img, session.ocr)
        if "error" in d:
            last = {"error": "date not readable", "date": d, "icon": ev}
            session.sleep(0.4)
            continue
        if state in ("paused", "running") and ev.get("icon") != "unknown":
            return state == "running", d, {"by": "icon", **ev}
        last = None
        session.sleep(0.3)
    if d is None or "error" in d:
        return None, d or {}, last or {"error": "date not readable"}
    session.sleep(wait)
    d2, img2 = read_date(session)
    if "error" in d2:
        return None, d2, {"error": "date not readable"}
    return topbar.date_key(d2) != topbar.date_key(d), d2, {"by": "clock", "before": d.get("hoi4"),
                                                           "after": d2.get("hoi4")}


def set_paused(session, paused=True, attempts=3, settle=6.0):
    """Toggle with Space until the state matches; verified by the speed icon. A busy game can take seconds to
    react, so after each press the icon is watched for `settle` seconds before pressing again (a second press
    while the first is still queued would undo it). Returns the final date dict."""
    want = "paused" if paused else "running"
    for _ in range(attempts):
        running, d, ev = probe_running(session)
        if running is None:
            raise ToolError("cannot read the in-game date; is a game loaded and the interface shown?", ev)
        if running != paused:
            return d
        session.input.key("space")
        t0 = time.time()
        while time.time() - t0 < settle:
            session.sleep(0.4)
            state, _ = topbar.pause_state(session.grab())
            if state == want:
                d2, _ = read_date(session)
                return d2 if "error" not in d2 else d
    running, d, ev = probe_running(session)
    if running == paused:
        raise ToolError(f"could not {'pause' if paused else 'unpause'} the game", ev)
    return d


def set_speed(session, speed):
    """Speed 1-5 through the speed keys (numpad minus down to 1, then numpad plus), verified on the speed bar
    under the date and corrected once if a key press was dropped."""
    speed = int(speed)
    if not 1 <= speed <= 5:
        raise ToolError("speed must be 1-5")
    session.input.key("subtract", times=5, delay=0.06)
    if speed > 1:
        session.input.key("add", times=speed - 1, delay=0.06)
    for _ in range(2):
        session.sleep(0.4)
        got = topbar.speed_level(session.grab())
        if got is None or got == speed:
            break
        session.input.key("add" if got < speed else "subtract", times=abs(speed - got), delay=0.08)
    session.calibration["speed"] = speed
    return speed


def _arm(session, hours, report):
    """Console `pause_in_hours N`: runs the clock now and stops it after N hours. False when the console failed."""
    try:
        console.run(session, [f"pause_in_hours {max(1, int(hours))}"], read=False)
        report["armed"] = report.get("armed", 0) + 1
        return True
    except ToolError as e:
        report["pause_in_hours_error"] = str(e)
        return False


def run_days(session, days=0, hours=0, popup_policy="first", speed=None, max_seconds=600, poll=None,
             use_console=True, on_popup=None):
    """Run the clock forward and stop paused. Returns a report dict with dates and handled popups. Whatever goes
    wrong while time runs (cancellation, an error), the game is paused again before the error is passed on."""
    try:
        return _run_days(session, days, hours, popup_policy, speed, max_seconds, poll, use_console, on_popup)
    except BaseException:
        try:
            set_paused(session, True)
        except Exception:
            pass
        raise


def _run_days(session, days, hours, popup_policy, speed, max_seconds, poll, use_console, on_popup):
    from . import popups as game_popups
    policy = game_popups.PopupPolicy.parse(popup_policy)
    total_hours = int(days) * 24 + int(hours)
    if total_hours <= 0:
        raise ToolError("days or hours must be positive")
    poll = poll or session.cfg["time"]["poll_seconds"]
    start, img = read_date(session)
    if "error" in start:
        raise ToolError("cannot read the in-game date before running time", start)
    h_start = topbar.hours(start)
    target = h_start + total_hours
    report = {"start": start["hoi4"], "requested_hours": total_hours, "popups": [], "polls": 0}
    # popups already open would be answered first
    handled = game_popups.handle_all(session, policy, img=img, phase="before")
    report["popups"] += handled
    if any(not r.get("answered", True) for r in handled):
        # a popup the policy leaves to the agent is open: do not start the clock
        report["stopped_on_popup"] = True
        final = set_paused(session, True)
        report.update({"end": final.get("hoi4"), "elapsed_hours": 0, "armed_pause_in_hours": False, "wall_seconds": 0})
        return report
    armed = False
    if speed:
        set_speed(session, speed)
    if use_console and session.mode == "test":
        # pause_in_hours starts the clock itself and stops it N hours later (verified live on 1.19): pressing
        # Space afterwards would pause a short step that already ended, or restart one that the game stopped.
        armed = _arm(session, total_hours, report)
    if not armed:
        set_paused(session, False)
    t0 = time.time()
    last = start
    stalled_since = None
    while True:
        session.check_cancel()
        session.report(f"running time: {last.get('hoi4')}")
        if time.time() - t0 > max_seconds:
            set_paused(session, True)
            report["timeout"] = True
            break
        session.sleep(poll)
        img = session.grab()
        report["polls"] += 1
        found = popups_mod.detect(img)
        if found and policy.mode != "ignore":
            set_paused(session, True)
            if policy.mode == "stop" and not policy.rules:
                report["stopped_on_popup"] = True
                report["popups"] += game_popups.describe(session, img, found)
                break
            recs = game_popups.handle_all(session, policy, phase="running")
            report["popups"] += recs
            if any(not r.get("answered", True) for r in recs):
                report["stopped_on_popup"] = True
                break
            if on_popup:
                on_popup(report)
            img = session.grab()
            d = topbar.read_date(img, session.ocr)
            if "error" not in d and topbar.hours(d) >= target:
                break
            if armed and "error" not in d:
                armed = _arm(session, target - topbar.hours(d), report)     # re-arm for the hours left
            if not armed:
                set_paused(session, False)
            continue
        d = topbar.read_date(img, session.ocr)
        if "error" in d:
            continue
        if topbar.date_key(d) == topbar.date_key(last):
            stalled_since = stalled_since or time.time()
            if time.time() - stalled_since > 4:
                # a notification window (focus completed ...) may have stopped the clock
                from . import panels
                for title in panels.dismiss_notices(session):
                    report["popups"].append({"kind": "notice", "title": title, "answered": "ok",
                                             "date": d.get("hoi4")})
                # stopped: pause_in_hours fired, or something paused the game
                if topbar.hours(d) >= target or (armed and topbar.hours(d) >= target - 1):
                    break
                if armed:
                    armed = _arm(session, target - topbar.hours(d), report)
                if not armed:
                    set_paused(session, False)
                stalled_since = None
        else:
            stalled_since = None
        last = d
        if topbar.hours(d) >= target:
            if not armed:
                set_paused(session, True)
                break
            if topbar.hours(d) >= target + 12:
                # pause_in_hours was armed but did not stop the game (the console may have dropped it)
                set_paused(session, True)
                report["armed_pause_missed"] = True
                break
    # final state: paused, with the date read again
    final = set_paused(session, True)
    report["end"] = final.get("hoi4")
    report["elapsed_hours"] = topbar.hours(final) - h_start
    report["armed_pause_in_hours"] = armed
    report["wall_seconds"] = round(time.time() - t0, 1)
    # popups that arrived exactly at the stop
    after = game_popups.handle_all(session, policy, phase="after")
    report["popups"] += after
    if any(not r.get("answered", True) for r in after):
        report["stopped_on_popup"] = True
    from . import panels
    for title in panels.dismiss_notices(session):
        report["popups"].append({"kind": "notice", "title": title, "answered": "ok", "date": report["end"]})
    return report
