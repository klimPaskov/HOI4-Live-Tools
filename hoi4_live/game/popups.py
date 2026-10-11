"""Popup handling on the live game: list, read and answer event/news/super-event windows, with verification.

Every answer is verified: the screen is re-read after the click and the result reports which popup disappeared.
A screenshot of each popup is saved before it is answered (artifacts/<run>/popup_*.png).

Policies (time running and hoi4_popups action=answer_all):
    first    answer every popup with its first option (the default for tests)
    choices  answer news and one-option events with their only option; stop at an event with a real choice, so the
             agent decides (the campaign default of hoi4_advance)
    stop     do not answer; stop time and report the popups
    pause    same as stop
    ignore   leave popups alone and keep running (time keeps going while popups stack up)
A policy can also be an object {"mode": <one of the above>, "rules": [{"match": regex on title and text,
"option": index or regex on the option text, "stop": true to hand the popup to the agent}]}: the first matching rule
decides, the mode handles the rest. See PopupPolicy.
"""

import re
import time

from ..desktop import images
from ..errors import ToolError
from ..perception import popups as popups_mod


def describe(session, img, found=None, read=True):
    found = popups_mod.detect(img) if found is None else found
    out = []
    for i, p in enumerate(found):
        if read:
            try:
                popups_mod.read_popup(img, p, session.ocr)
            except ToolError as e:
                p.text = f"(OCR failed: {e})"
        out.append(p.to_dict(i))
    session.last_popups = out
    return out


def _signature(p):
    return (p.kind, tuple(b.box[1] // 8 for b in p.buttons), p.box[0] // 8, p.box[1] // 8)


def answer(session, popup_index=0, option=0, option_text=None, img=None, save_shot=True):
    """Click one option of one popup and verify the popup went away. Returns a report dict."""
    img = session.grab() if img is None else img
    found = popups_mod.detect(img)
    if not found:
        raise ToolError("no popup is open")
    if popup_index >= len(found):
        raise ToolError(f"popup index {popup_index} out of range (0..{len(found) - 1})",
                        {"popups": [p.to_dict(i) for i, p in enumerate(found)]})
    p = found[popup_index]
    popups_mod.read_popup(img, p, session.ocr)
    if option_text:
        rx = re.compile(option_text, re.I) if _is_regex(option_text) else None
        idx = None
        for i, b in enumerate(p.buttons):
            t = b.text or ""
            if (rx and rx.search(t)) or (not rx and option_text.lower() in t.lower()):
                idx = i
                break
        if idx is None:
            raise ToolError(f"no option of popup {popup_index} matches '{option_text}'",
                            {"options": [b.text for b in p.buttons]})
        option = idx
    if option >= len(p.buttons):
        raise ToolError(f"option {option} out of range; the popup has {len(p.buttons)} option(s)",
                        {"options": [b.text for b in p.buttons]})
    shot = None
    if save_shot:
        x, y, w, h = p.box
        pad = 12
        crop = images.crop(img, [x - pad, y - pad, w + 2 * pad, h + 2 * pad])
        shot = session.artifact(crop, f"popup_{p.kind}_{_slug(p.title)}")
    before_sig = _signature(p)
    b = p.buttons[option]
    session.input.click(*b.center)
    session.park_cursor()
    time.sleep(0.9)
    after = session.grab()
    remaining = popups_mod.detect(after)
    still = any(_signature(q) == before_sig for q in remaining)
    rec = {"kind": p.kind, "title": p.title, "text": (p.text or "")[:600], "options": [x.text for x in p.buttons],
           "chosen": option, "chosen_text": b.text, "closed": not still, "remaining": len(remaining)}
    if shot:
        rec["screenshot"] = shot
    if still:
        # one more try: some windows need the click to land after their open animation
        session.input.click(*b.center)
        session.park_cursor()
        time.sleep(1.0)
        after = session.grab()
        remaining = popups_mod.detect(after)
        rec["closed"] = not any(_signature(q) == before_sig for q in remaining)
        rec["remaining"] = len(remaining)
        rec["retried"] = True
    remember(session, rec)
    return rec


def remember(session, rec):
    """Keep answered popups for briefings (in memory and in artifacts/state/popups.jsonl)."""
    import json
    import os
    entry = {k: rec.get(k) for k in ("kind", "title", "text", "options", "chosen_text", "closed")}
    entry["time"] = time.time()
    try:
        from ..perception import topbar
        d = topbar.read_date(session.grab(), session.ocr)
        entry["date"] = d.get("hoi4")
    except Exception:
        pass
    session.popup_log.append(entry)
    del session.popup_log[:-200]
    try:
        d = os.path.join(session.cfg.artifacts_dir, "state")
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "popups.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps(entry, ensure_ascii=False) + "\n")
    except OSError:
        pass


MODES = ("first", "choices", "stop", "pause", "ignore")


class PopupPolicy:
    """Decides what to do with a popup: answer it with an option, or hand it to the agent (stop)."""

    def __init__(self, mode="first", rules=None):
        if mode not in MODES:
            raise ToolError(f"unknown popup policy '{mode}' (one of {', '.join(MODES)})", {"field": "popups"})
        self.mode = "stop" if mode == "pause" else mode
        self.rules = []
        for r in rules or []:
            try:
                rx = re.compile(r.get("match") or ".", re.I | re.S)
            except re.error as e:
                raise ToolError(f"popup rule regex '{r.get('match')}' is invalid: {e}", {"field": "popup_rules"})
            self.rules.append({"rx": rx, "option": r.get("option", 0), "stop": bool(r.get("stop")),
                               "match": r.get("match") or "."})

    @classmethod
    def parse(cls, spec, rules=None):
        if isinstance(spec, cls):
            return spec
        if isinstance(spec, dict):
            return cls(spec.get("mode", "first"), spec.get("rules", rules))
        return cls(spec or "first", rules)

    @property
    def needs_text(self):
        return bool(self.rules)

    def to_dict(self):
        return {"mode": self.mode, "rules": [{"match": r["match"], "option": r["option"], "stop": r["stop"]}
                                             for r in self.rules]}

    def decide(self, p):
        """(option index, why) to answer, or (None, why) to stop. p is a read popup (title, text, buttons)."""
        n = len(p.buttons)
        hay = (p.title or "") + " " + (p.text or "")
        for r in self.rules:
            if not r["rx"].search(hay):
                continue
            if r["stop"]:
                return None, f"rule '{r['match']}' hands it to the agent"
            opt = r["option"]
            if isinstance(opt, str) and not opt.isdigit():
                rx = re.compile(opt, re.I)
                for i, b in enumerate(p.buttons):
                    if rx.search(b.text or ""):
                        return i, f"rule '{r['match']}' option '{opt}'"
                return None, f"rule '{r['match']}': no option matches '{opt}'"
            return min(int(opt), max(n - 1, 0)), f"rule '{r['match']}'"
        if self.mode == "stop":
            return None, "policy stop"
        if self.mode == "choices" and p.kind != "news" and n > 1:
            return None, f"{n} options: the agent decides"
        return 0, f"policy {self.mode}"


def handle_all(session, policy="first", img=None, limit=25, phase=None):
    """Answer every visible popup according to the policy. Returns the list of popup records: answered ones carry
    answered=True, popups left for the agent answered=False (the loop stops at the first of those)."""
    policy = PopupPolicy.parse(policy)
    if policy.mode == "ignore" and not policy.rules:
        return []
    done = []
    session.park_cursor()
    for _ in range(limit):
        session.check_cancel()
        img = session.grab() if img is None else img
        found = popups_mod.detect(img)
        if not found:
            break
        if policy.mode == "stop" and not policy.rules:
            done += [dict(d, answered=False, phase=phase) for d in describe(session, img, found)]
            break
        option, why = 0, None
        if policy.needs_text or policy.mode == "choices":
            if policy.needs_text:
                try:
                    popups_mod.read_popup(img, found[0], session.ocr)
                except ToolError as e:
                    found[0].text = f"(OCR failed: {e})"
            option, why = policy.decide(found[0])
            if option is None:
                done += [dict(d, answered=False, phase=phase, why=why) for d in describe(session, img, found)]
                break
        rec = answer(session, 0, option, img=img)
        if why:
            rec["why"] = why
        rec["phase"] = phase
        rec["answered"] = True
        done.append(rec)
        img = None
        if not rec["closed"]:
            # the top window did not react: try the next popup's option before giving up
            if len(found) > 1:
                rec2 = answer(session, 1, 0)
                rec2["phase"] = phase
                rec2["answered"] = True
                done.append(rec2)
                if not rec2["closed"]:
                    break
            else:
                break
    return done


def _slug(text):
    return re.sub(r"[^A-Za-z0-9]+", "_", (text or "untitled"))[:40].strip("_") or "untitled"


def _is_regex(s):
    return any(c in s for c in "^$[]()|*+?\\")
