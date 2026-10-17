"""Open windows block every click: before a campaign action the guard closes what a player would close first.

News and one-option events are answered, "National Focus Completed" and similar notices are dismissed (their titles
go to `session.notice_queue`, so a focus that finished while the clock was stopped still reaches the standing
orders), and an event with a real choice stops the action with an error that names the event: the agent decides
(hoi4_popups action=read / answer), the guard never picks an option that matters. Cheap when nothing is open: one
screenshot and the parchment detector; the notice check (one OCR of the screen centre) runs only when the
speed icon says the clock is stopped.
"""

import logging

from ..errors import ToolError
from ..perception import popups as popups_mod
from ..perception import topbar
from . import panels
from . import popups as game_popups

log = logging.getLogger("hoi4_live.guard")

# tools that look at or answer windows themselves, and tools that never click in the game
SKIP = ("hoi4_popups", "hoi4_screenshot", "hoi4_ocr", "hoi4_status", "hoi4_screen", "hoi4_time", "hoi4_advance",
        "hoi4_clock", "hoi4_launch", "hoi4_new_game", "hoi4_load", "hoi4_save", "hoi4_shutdown", "hoi4_lock",
        "hoi4_click", "hoi4_key", "hoi4_type", "hoi4_scroll", "hoi4_drag", "hoi4_hover", "hoi4_scenario")


def queue(session):
    q = getattr(session, "notice_queue", None)
    if q is None:
        q = session.notice_queue = []
    return q


def drain(session):
    q = queue(session)
    out, q[:] = list(q), []
    return out


def clear_windows(session, rounds=3):
    """Close open windows. Returns {"answered": [...], "notices": [...]}; raises ToolError for an open event that
    needs a decision."""
    done = {"answered": [], "notices": []}
    policy = game_popups.PopupPolicy("choices")
    governor = getattr(session, "governor", None)
    if governor is not None and governor.active():
        policy = game_popups.PopupPolicy.parse(governor.policy.plan["events"])
    for _ in range(rounds):
        img = session.grab(focus=False)
        found = popups_mod.detect(img)
        if found:
            recs = game_popups.handle_all(session, policy, img=img, phase="guard")
            for r in recs:
                if r.get("answered"):
                    done["answered"].append({k: r.get(k) for k in ("kind", "title", "chosen_text")})
                else:
                    raise ToolError(f"{len(found)} event window(s) are open and need a decision "
                                    f"(hoi4_popups action=read, then answer): {r.get('title') or r.get('kind')}",
                                    {"popups": recs, "error": "popup_open"})
            continue
        state = topbar.pause_state(img)[0]
        if state == "running":
            break
        titles = panels.dismiss_notices(session)
        if not titles:
            break
        for t in titles:
            done["notices"].append(t)
            queue(session).append({"title": t})
    return done
