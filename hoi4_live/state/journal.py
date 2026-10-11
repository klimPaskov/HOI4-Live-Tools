"""The campaign journal (phase 5): a JSON-lines log of what the agent planned, did and learned, one file per country
under artifacts/journal/. Pure file handling, so it is unit tested without the game.

Entry:  {"time": epoch, "date": "1936.1.8.12" | None, "kind": ..., "text": ..., "data": {...}, "tags": [...]}
Kinds:  note (free text), plan (what the agent intends next: the latest plan is shown first), decision (a choice and
        its reason), lesson (something that went wrong or right), action (written by the server after a verified
        campaign action), advance (written by hoi4_advance: the period run and why it stopped), popup (an answered
        event with its chosen option).
"""

import datetime
import json
import os
import re
import time

KINDS = ("note", "plan", "decision", "lesson", "action", "advance", "popup")
AUTO_KINDS = ("action", "advance", "popup")
MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]


def fmt_date(text):
    try:
        p = [int(x) for x in str(text).split(".")]
        return f"{p[2]} {MONTHS[p[1] - 1]} {p[0]}"
    except (ValueError, IndexError, TypeError):
        return str(text) if text else "?"


def date_key(text):
    """Sortable (year, month, day, hour) of a HOI4 date text, None when unreadable."""
    try:
        p = [int(x) for x in str(text).split(".")]
        return tuple(p + [0] * (4 - len(p)))[:4]
    except (ValueError, TypeError):
        return None


class Journal:
    def __init__(self, directory, tag):
        self.directory = directory
        self.tag = tag or "campaign"
        self.path = os.path.join(directory, f"journal_{self.tag}.jsonl")

    # ------------------------------------------------------------------ write
    def add(self, kind, text, date=None, data=None, tags=None):
        if kind not in KINDS:
            raise ValueError(f"unknown journal kind {kind}")
        entry = {"time": round(time.time(), 2), "date": date, "kind": kind, "text": str(text).strip()}
        if data:
            entry["data"] = data
        if tags:
            entry["tags"] = list(tags)
        os.makedirs(self.directory, exist_ok=True)
        with open(self.path, "a", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(entry, ensure_ascii=False, default=str) + "\n")
        return entry

    # ------------------------------------------------------------------ read
    def entries(self):
        out = []
        try:
            with open(self.path, encoding="utf-8") as f:
                for line in f:
                    try:
                        e = json.loads(line)
                    except ValueError:
                        continue
                    if isinstance(e, dict) and "kind" in e:
                        out.append(e)
        except OSError:
            pass
        return out

    def read(self, limit=20, kinds=None, since=None, query=None, auto=True):
        """Newest `limit` entries (oldest first) matching the filters. since = HOI4 date text; query = regex over
        text and tags; auto=False drops the entries the server wrote itself."""
        rx = re.compile(query, re.I) if query else None
        since_k = date_key(since) if since else None
        out = []
        for e in self.entries():
            if kinds and e["kind"] not in kinds:
                continue
            if not auto and e["kind"] in AUTO_KINDS:
                continue
            if since_k and (date_key(e.get("date")) or (0,)) < since_k:
                continue
            if rx and not rx.search(e.get("text", "") + " " + " ".join(e.get("tags") or [])):
                continue
            out.append(e)
        return out[-limit:]

    def latest(self, kind):
        found = [e for e in self.entries() if e["kind"] == kind]
        return found[-1] if found else None

    def summary(self):
        ents = self.entries()
        counts = {}
        for e in ents:
            counts[e["kind"]] = counts.get(e["kind"], 0) + 1
        dated = [e["date"] for e in ents if e.get("date")]
        return {"entries": len(ents), "counts": counts, "first_date": dated[0] if dated else None,
                "last_date": dated[-1] if dated else None, "latest_plan": (self.latest("plan") or {}).get("text")}


def line(e):
    when = fmt_date(e.get("date")) if e.get("date") else "-"
    return f"- {when} [{e['kind']}] {e.get('text', '')}"


def report(journal, tail=40):
    """A markdown run report from the journal: period, counts, the standing plan, the decisions and lessons the
    agent wrote, a timeline of actions, advances and answered events."""
    ents = journal.entries()
    if not ents:
        return f"# Campaign report ({journal.tag})\n\nThe journal is empty."
    s = journal.summary()
    L = [f"# Campaign report ({journal.tag})", ""]
    first, last = s["first_date"], s["last_date"]
    span = ""
    a, b = date_key(first), date_key(last)
    if a and b:
        try:
            d0 = datetime.date(a[0], a[1], a[2])
            d1 = datetime.date(b[0], b[1], b[2])
            span = f" ({(d1 - d0).days} days)"
        except ValueError:
            pass
    L.append(f"Period: {fmt_date(first)} to {fmt_date(last)}{span}; {s['entries']} entries: "
             + ", ".join(f"{k} {v}" for k, v in sorted(s["counts"].items())))
    if s["latest_plan"]:
        L += ["", "## Current plan", s["latest_plan"]]
    for title, kinds in (("Decisions and lessons", ("decision", "lesson")), ("Notes", ("note",)),
                         ("Plans", ("plan",))):
        rows = [e for e in ents if e["kind"] in kinds]
        if rows:
            L += ["", f"## {title}"] + [line(e) for e in rows[-tail:]]
    acts = [e for e in ents if e["kind"] in AUTO_KINDS]
    if acts:
        L += ["", "## Timeline (actions, advances, answered events)"] + [line(e) for e in acts[-tail:]]
    return "\n".join(L)
