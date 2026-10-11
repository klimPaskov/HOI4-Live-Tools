"""The peace conference screen (HOI4 1.19, verified live on 1.19.2: Treaty of Wien after Austria capitulated).

When a war is won the game opens a full-screen conference (no top bar, so the clock cannot read its speed icon):

    top right        "Confirm and Exit" (asks "Exit Conference?" -> OK), below the Victors / Beneficiaries tabs
    top centre       "Make your Demands", the score ("Score: 610"), "Conference turn: 1"
    left             "Demands" icons (take state, ...) and, below, one block per loser: its name, then after
                     the yellow arrow its states as rows "<cost> <state name>" with a "Select All" row on top
    right            "Current Demands": our flag with the score, "Un-submitted Demands" with each demand's count
    bottom right     "Submit Demands <cost>" (or "Pass")
    bottom left      "Reset"

A click on a row selects the demand (a flag shows on the row); Submit makes them final for this turn.
"""

import re
import time

from ..errors import ToolError
from . import panels

CONFIRM_EXIT = (1768, 54)
EXIT_OK = (1053, 677)
SUBMIT = (1750, 1038)
SCORE_RX = re.compile(r"score\W*(\d+)", re.I)
TURN_RX = re.compile(r"conference\s*turn\W*(\d+)", re.I)


def _scaled(session, x, y):
    """Reference coordinates are 1920x1080: scale to the client."""
    w, h = session.window.size()
    return int(x * w / 1920), int(y * h / 1080)


def is_open(session, img=None):
    """True when the conference screen is up ('Confirm and Exit' at the top right)."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    res = session.ocr.recognize(img, region=[int(w * 0.84), 0, int(w * 0.16), int(h * 0.1)], scale=2.0)
    return any(re.search(r"confirm\s*and\s*exit", l.text, re.I) for l in res.lines)


def read(session, img=None):
    """{open, title, score, turn, loser, rows: [{name, cost, selected?, y}], select_all, submit, current_demands}."""
    img = session.grab() if img is None else img
    h, w = img.shape[:2]
    if not is_open(session, img):
        return {"open": False}
    out = {"open": True}
    top = session.ocr.recognize(img, region=[0, 0, w, int(h * 0.17)], scale=1.5)
    text = " ".join(l.text for l in top.lines)
    m = SCORE_RX.search(text)
    out["score"] = int(m.group(1)) if m else None
    m = TURN_RX.search(text)
    out["turn"] = int(m.group(1)) if m else None
    t = next((l for l in top.lines if l.box[0] < w * 0.3 and l.box[1] < h * 0.06), None)
    out["title"] = t.text if t else None
    left = session.ocr.recognize(img, region=[0, int(h * 0.34), int(w * 0.18), int(h * 0.62)], scale=1.5)
    rows = []
    for l in sorted(left.lines, key=lambda l: l.box[1]):
        mm = re.match(r"^\W*(\d{1,4})\s+(.+)$", l.text)
        if mm:
            rows.append({"cost": int(mm.group(1)), "name": mm.group(2).strip(), "y": l.box[1] + l.box[3] // 2,
                         "x": l.box[0] + l.box[2] // 2})
    out["rows"] = [r for r in rows if not re.match(r"^(select|cancel)\s*all", r["name"], re.I)]
    sa = next((r for r in rows if re.match(r"^(select|cancel)\s*all", r["name"], re.I)), None)
    out["select_all"] = sa
    heads = [l for l in left.lines if not re.match(r"^\W*\d", l.text) and 20 < l.box[0] < 200]
    out["loser"] = heads[0].text if heads else None
    right = session.ocr.recognize(img, region=[int(w * 0.83), int(h * 0.16), int(w * 0.17), int(h * 0.82)], scale=1.5)
    out["current_demands"] = [l.text for l in right.lines][:12]
    bottom = session.ocr.recognize(img, region=[int(w * 0.8), int(h * 0.93), int(w * 0.2), int(h * 0.07)], scale=2.0)
    out["submit_label"] = " ".join(l.text for l in bottom.lines)
    return out


def expand_loser(session, state):
    """The loser's block is collapsed: its yellow arrow (x 287, y 422 at 1080p) opens the state list."""
    if state.get("rows"):
        return state
    session.input.click(*_scaled(session, 287, 422))
    time.sleep(1.0)
    session.park_cursor()
    return read(session)


def take(session, names, all_states=False):
    """Select demands: every state (Select All) or the named ones. Returns (clicked, state)."""
    st = expand_loser(session, read(session))
    if not st.get("open"):
        raise ToolError("the peace conference is not open", {"screen": panels.current(session)})
    clicked = []
    if all_states:
        sa = st.get("select_all")
        if not sa:
            raise ToolError("no 'Select All' row in the demand list", {"rows": st.get("rows")})
        if re.match(r"^select", sa["name"], re.I):
            session.input.click(sa["x"], sa["y"])
            time.sleep(0.8)
        clicked = ["all"]
    else:
        for want in names:
            row = next((r for r in st["rows"] if panels.similar(r["name"], want) >= 0.8), None)
            if not row:
                raise ToolError(f"no state '{want}' in the demand list", {"rows": [r["name"] for r in st["rows"]]})
            session.input.click(row["x"], row["y"])
            time.sleep(0.6)
            clicked.append(row["name"])
    session.park_cursor()
    return clicked, read(session)


def submit(session):
    session.input.click(*_scaled(session, *SUBMIT))
    time.sleep(2.0)
    session.park_cursor()
    return read(session)


def leave(session):
    """Confirm and Exit, then OK on 'Exit Conference'. Returns True when the conference is closed."""
    session.input.click(*_scaled(session, *CONFIRM_EXIT))
    time.sleep(1.2)
    d = panels.answer_dialog(session, True, attempts=4)
    time.sleep(1.5)
    return not is_open(session), d
