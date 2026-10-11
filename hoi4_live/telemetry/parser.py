"""Parse the telemetry mod's `HOI4LIVE` lines from game.log into a store.

A game.log line written by the `log` effect looks like

    [03:09:06][1936.01.11.23][effectbase.cpp:1783]: HOI4LIVE "k":"day","t":"USA","n":706650,...

(the members of a flat JSON object: HOI4 localisation drops `{`, so the braces are added here; a body that does
start with `{` is accepted too)
so every record carries the wall clock (hh:mm:ss), the in-game date and hour, and its own fields (`k` = kind,
`t` = the player's tag, `n` = the global day counter). Records are grouped into ticks by `n`: one daily tick gives
one `day`, `pol`, `res`, `eq` record plus one `war` record per enemy; a weekly tick adds `wc` (one per listed
country), `we` (enemy pairs), `wld`, `front` and `lost` records. `ev` records are events (wars, peace, factions,
control changes).

The store reads game.log incrementally (byte offset), notices when the game restarted (the file shrank or its head
changed) and starts over, and handles loading an older save (the day counter goes back): history and events after
that point belong to an abandoned timeline and are dropped.
"""

import collections
import datetime
import json
import math
import os
import re
import time

from .focus import FocusLedger, KINDS as FOCUS_KINDS

LINE_RX = re.compile(r"^\[(\d\d):(\d\d):(\d\d)\]\[(\d{1,4})\.(\d{1,2})\.(\d{1,2})\.(\d{1,2})\]\[[^\]]*\]:\s*HOI4LIVE\s+(.+?)\s*$")
STRING_VALUE = re.compile(r'(?<=":")(.*?)(?="(?:,"[A-Za-z0-9_]+":|\}))')
DAILY_GROUPS = ("war",)
WEEKLY_GROUPS = ("wc", "we", "front", "lost", "st", "kt")
SINGLE = ("day", "pol", "fac", "subject", "res", "eq", "watch", "wld", "hello")


def lenient_loads(text):
    """json.loads, then a repair pass for string values that contain quotes or backslashes (country, party and
    leader names are printed verbatim by the game)."""
    try:
        return json.loads(text)
    except ValueError:
        pass
    fixed = STRING_VALUE.sub(lambda m: m.group(1).replace("\\", "\\\\").replace('"', '\\"'), text)
    try:
        return json.loads(fixed)
    except ValueError:
        return None


def parse_body(body):
    """`"k":"day",...` (or a full `{...}` object) -> dict or None."""
    body = body.strip()
    for cand in ([body] if body.startswith("{") else ["{" + body + "}", "{" + body]):
        rec = lenient_loads(cand)
        if isinstance(rec, dict):
            return rec
    return None


def unflatten(rec, kind):
    """Group the flat fields of res / eq / watch records for presentation."""
    meta = {k: v for k, v in rec.items() if k in ("k", "t", "n", "v", "g", "_date")}
    if kind == "res":
        out = {}
        for k, v in rec.items():
            if k + "_prod" in rec:
                out[k] = {"surplus": v, "produced": rec.get(k + "_prod"), "consumed": rec.get(k + "_used"),
                          "exported": rec.get(k + "_exp"), "imported": rec.get(k + "_imp")}
        return dict(meta, resources=out)
    if kind == "eq":
        out = {}
        for k, v in rec.items():
            if k + "_army" in rec:
                army, need = rec.get(k + "_army") or 0, rec.get(k + "_need") or 0
                out[k] = {"stockpile": v, "in_armies": army, "needed": need,
                          "deficit": max(0, need - army - (v or 0))}
        return dict(meta, equipment=out)
    if kind == "watch":
        out = {}
        for k, v in rec.items():
            if "." in k:
                group, name = k.split(".", 1)
                out.setdefault(group, {})[name] = v
        return dict(meta, **out)
    return rec


def parse_line(line):
    """One game.log line -> record dict (with _date, _clock, _hours) or None."""
    if "HOI4LIVE" not in line:
        return None
    m = LINE_RX.match(line.strip())
    if not m:
        return None
    hh, mm, ss, y, mo, d, h, body = m.groups()
    rec = parse_body(body)
    if not isinstance(rec, dict) or "k" not in rec:
        return {"k": "_bad", "_raw": body[:300], "_date": f"{int(y)}.{int(mo)}.{int(d)}.{int(h)}"}
    rec["_date"] = f"{int(y)}.{int(mo)}.{int(d)}.{int(h)}"
    rec["_clock"] = f"{hh}:{mm}:{ss}"
    return rec


def _wall(clock, anchor):
    """Wall time (epoch) of an hh:mm:ss clock read in a file last written at `anchor` (the file mtime): the
    same day as the anchor, or the day before when the clock is later than the anchor."""
    a = datetime.datetime.fromtimestamp(anchor)
    hh, mm, ss = [int(x) for x in clock.split(":")]
    t = a.replace(hour=hh, minute=mm, second=ss, microsecond=0)
    if t > a + datetime.timedelta(seconds=90):
        t -= datetime.timedelta(days=1)
    return t.timestamp()


class TelemetryStore:
    def __init__(self, log_path=None, history_days=400, max_events=300):
        self.log_path = log_path
        self.history_days = history_days
        self.max_events = max_events
        self.reset()

    # ------------------------------------------------------------------ state
    def reset(self):
        self.offset = 0
        self.head = b""
        self.partial = ""
        self.latest = {}                         # kind -> {tag -> record}   (single-record kinds)
        self.groups = {}                         # kind -> {"n": n, "rows": [...]}
        self.history = collections.defaultdict(collections.deque)   # tag -> deque of day records
        self.daily_metrics = {}                  # eq/res -> tag -> bounded {day counter: record}
        self.events = collections.deque(maxlen=self.max_events)
        self.lines = 0
        self.bad = 0
        self.last = None                         # last record of any kind
        self.restarts = 0
        self.rewinds = 0
        self.generation = None
        self.schema = None
        self.rs_counts = {}                        # day counter -> set of technologies being researched
        self.techs = getattr(self, "techs", [])    # technology ids; the `tid` of kt and rs records indexes them
        if not hasattr(self, "focus_ledger"):
            self.focus_ledger = FocusLedger(self.history_days)
        self.focus_ledger.clear()

    # ------------------------------------------------------------------ input
    def refresh(self):
        """Read what game.log gained since the last call. Returns the number of new records."""
        if not self.log_path or not os.path.exists(self.log_path):
            return 0
        st = os.stat(self.log_path)
        with open(self.log_path, "rb") as f:
            head = f.read(256)
            if st.st_size < self.offset or (self.head and head[:len(self.head)] != self.head[:len(head)]):
                restarts = self.restarts + 1
                self.reset()
                self.restarts = restarts
            if st.st_size == self.offset:
                return 0
            f.seek(self.offset)
            data = f.read(st.st_size - self.offset)
        self.head = head
        self.offset += len(data)
        return self.feed(data.decode("utf-8", "replace"), anchor=st.st_mtime)

    def feed(self, text, anchor=None):
        """Parse text (whole lines; a trailing partial line is kept for the next call)."""
        anchor = anchor or time.time()
        text = self.partial + text
        lines = text.split("\n")
        self.partial = lines.pop() if not text.endswith("\n") else ""
        n = 0
        for line in lines:
            rec = parse_line(line)
            if rec is None:
                continue
            if rec["k"] == "_bad":
                self.bad += 1
                continue
            rec["_wall"] = _wall(rec["_clock"], anchor)
            self.add(rec)
            n += 1
        return n

    def add(self, rec):
        self.lines += 1
        # Log order resolves records sharing the game's one-second timestamp.
        # Never trust a supplied sequence or mutate a caller's fixture/record.
        # Reset also clears every observation, so sequence reuse cannot bridge
        # a file restart. Generation boundaries retain this monotonic counter.
        rec = dict(rec, _sequence=self.lines)
        k = rec["k"]
        n = rec.get("n")
        tag = rec.get("t")
        if k in ("day", "hello") and isinstance(rec.get("g"), str) and rec["g"]:
            schema = rec.get("v")
            schema = schema if type(schema) is int and schema > 0 else None
            if self.generation != rec["g"] or self.schema != schema:
                # No observation from the previous generation may supply a
                # country, day, queue or metric for this campaign. Preserve the
                # file cursor and configured catalogs, but discard all batches.
                self.latest.clear()
                self.groups.clear()
                self.history.clear()
                self.events.clear()
                self.rs_counts.clear()
                self.daily_metrics.clear()
                self.focus_ledger.clear()
            self.generation, self.schema = rec["g"], schema
        if k in FOCUS_KINDS:
            self.focus_ledger.add(rec, self.day(tag) if tag else None)
            self.last = rec
            return
        if k == "day":
            hist = self.history[tag]
            if hist and isinstance(n, (int, float)) and n <= hist[-1].get("n", -1):
                self._rewind(n)
                hist = self.history[tag]
            hist.append(rec)
            while len(hist) > self.history_days:
                hist.popleft()
            self._prune_daily_metrics(tag)
        if k == "rs" and isinstance(n, (int, float)):
            tid = rec.get("tid")
            name = self.techs[int(tid)] if isinstance(tid, (int, float)) and 0 <= int(tid) < len(self.techs) else tid
            self.rs_counts.setdefault(n, set()).add(name)      # a technology listed twice counts once
            for old in [x for x in self.rs_counts if x < n - 60]:
                del self.rs_counts[old]
        if k == "ev":
            self.events.append(rec)
        elif k in SINGLE:
            self.latest.setdefault(k, {})[tag] = rec
        else:
            g = self.groups.get(k)
            # one tick = the same day counter and the same moment (a reloaded save repeats a day counter later)
            if g is None or g["n"] != n or abs(rec.get("_wall", 0) - g.get("_wall", 0)) > 5:
                self.groups[k] = g = {"n": n, "rows": [], "_date": rec.get("_date")}
            g["rows"].append(rec)
            g["_wall"] = rec.get("_wall", 0)
        self.last = rec
        if k in ("eq", "res"):
            self._remember_daily_metrics(rec)

    def _prune_daily_metrics(self, tag):
        days = {record.get("n") for record in self.history.get(tag, ())}
        for by_tag in self.daily_metrics.values():
            records = by_tag.get(tag, {})
            for old in [counter for counter in records if counter not in days]:
                del records[old]

    def _remember_daily_metrics(self, rec):
        tag, counter = rec.get("t"), rec.get("n")
        anchor = self.day(tag) if tag else None
        if (anchor is None or not isinstance(counter, (int, float)) or isinstance(counter, bool)
                or not math.isfinite(counter) or counter != int(counter)
                or anchor.get("n") != counter or not self.generation or self.schema is None
                or anchor.get("g") != self.generation or anchor.get("v") != self.schema
                or anchor.get("_date") != rec.get("_date")
                or rec.get("g", self.generation) != self.generation
                or rec.get("v", self.schema) != self.schema
                or type(rec.get("v", self.schema)) is not int):
            return
        record = dict(rec, _generation=self.generation, _schema=self.schema)
        records = self.daily_metrics.setdefault(rec["k"], {}).setdefault(tag, {})
        records[counter] = record
        self._prune_daily_metrics(tag)

    def _rewind(self, n):
        """A save from before day n was loaded: forget history, events and ticks from day n on."""
        self.rewinds += 1
        self.focus_ledger.clear()
        for x in [x for x in self.rs_counts if x >= n]:
            del self.rs_counts[x]
        for tag, hist in self.history.items():
            self.history[tag] = collections.deque(r for r in hist if r.get("n", 0) < n)
            self._prune_daily_metrics(tag)
        self.events = collections.deque((e for e in self.events if e.get("n", 0) < n), maxlen=self.max_events)
        for k in [k for k, g in self.groups.items() if (g.get("n") or 0) >= n]:
            del self.groups[k]
        for k, by_tag in self.latest.items():
            for t in [t for t, r in by_tag.items() if (r.get("n") or 0) >= n and k != "day"]:
                del by_tag[t]

    def drop_after(self, date_text):
        """A save dated `date_text` was loaded: forget the records of later days (the timeline that was left), so a
        briefing or a stop condition does not read days that have not happened yet. Returns the days dropped."""
        def key(text):
            try:
                return tuple(int(x) for x in str(text).split("."))[:4]
            except ValueError:
                return None
        limit = key(date_text)
        if not limit:
            return 0
        cut = None
        for hist in self.history.values():
            for r in hist:
                k = key(r.get("_date"))
                if k and k > limit and isinstance(r.get("n"), (int, float)):
                    cut = r["n"] if cut is None else min(cut, r["n"])
        if cut is None:
            return 0
        before = sum(len(h) for h in self.history.values())
        self._rewind(cut)
        return before - sum(len(h) for h in self.history.values())

    # ------------------------------------------------------------------ technologies and states
    def set_catalog(self, techs):
        """The manifest's technology list (the telemetry's tid is an index into it)."""
        self.techs = list(techs or [])

    def set_focus_catalog(self, focuses, generation):
        self.focus_ledger.configure(focuses, generation)

    def focus_snapshot(self, tag, day):
        """Closed exact-day own-tree batch; active/current focus remains unknown."""
        if self.focus_ledger.generation != self.generation:
            return None
        return self.focus_ledger.get(tag, day)

    def _tech_ids(self, rows):
        out = []
        for r in rows:
            tid = r.get("tid")
            if isinstance(tid, (int, float)) and 0 <= int(tid) < len(self.techs):
                out.append(self.techs[int(tid)])
        return out

    def known_techs(self):
        """Ids of the technologies the player has, from the newest scan (None when no scan was logged yet)."""
        rows = self.group("kt")
        return set(self._tech_ids(rows)) if rows else None

    def researching_techs(self):
        """Ids of the technologies in the player's research slots today (None without today's day record)."""
        d = self.day()
        if not d:
            return None
        return self._tech_ids(self.group("rs", d.get("n")))

    def busy_slots(self, n):
        """Busy research slots on day n, valid once a later day record exists (the rs lines of a day follow its day
        record): the number of rs lines logged for that day."""
        return len(self.rs_counts.get(n, ()))

    def state_rows(self):
        """The newest weekly scan of the player's states: [{s, name, civ, mil, dock, infra, free, ctrl}]."""
        return [_public(r) for r in self.group("st")]

    # ------------------------------------------------------------------ queries
    def player(self):
        """Daily identity, or one current-generation human handshake before day one.

        The generated hello is scoped to human countries. Multiple handshakes
        cannot identify the local player, so that case stays unknown. This
        fallback establishes identity only; day/snapshot remain unavailable.
        Session callers separately verify the installed campaign manifest.
        """
        best = None
        for tag, hist in self.history.items():
            if hist and (best is None or hist[-1]["_wall"] >= best[1]):
                best = (tag, hist[-1]["_wall"])
        if best:
            return best[0]
        candidates = []
        for tag, rec in self.latest.get("hello", {}).items():
            counter, wall = rec.get("n"), rec.get("_wall")
            if (isinstance(tag, str) and re.fullmatch(r"[A-Z][A-Z0-9]{2}", tag)
                    and self.generation and self.schema is not None
                    and rec.get("g") == self.generation and rec.get("v") == self.schema
                    and type(rec.get("v")) is int
                    and type(counter) in (int, float) and math.isfinite(counter)
                    and counter == int(counter)
                    and type(wall) in (int, float) and math.isfinite(wall)):
                candidates.append(tag)
        return candidates[0] if len(candidates) == 1 else None

    def day(self, tag=None):
        tag = tag or self.player()
        hist = self.history.get(tag)
        return hist[-1] if hist else None

    def get(self, kind, tag=None):
        tag = tag or self.player()
        return (self.latest.get(kind) or {}).get(tag)

    def get_at(self, kind, tag, n):
        """Exact retained eq/res batch for one country/day, or None if unavailable.

        The caller establishes completion using a later day record. Never
        substitutes a newer record, another country, or another generation.
        Private provenance fields identify the bounding day's generation/schema.
        A copy protects the stored batch from caller mutation.
        """
        if kind not in ("eq", "res") or not isinstance(tag, str) or not tag:
            return None
        if (not isinstance(n, (int, float)) or isinstance(n, bool)
                or not math.isfinite(n) or n != int(n)):
            return None
        record = self.daily_metrics.get(kind, {}).get(tag, {}).get(n)
        if (record is None or record.get("_generation") != self.generation
                or record.get("_schema") != self.schema):
            return None
        return dict(record)

    def group(self, kind, current_n=None):
        """Rows of the latest tick of a multi-line kind; [] when that tick is older than current_n (for example
        no `war` rows today because the war ended)."""
        g = self.groups.get(kind)
        if not g:
            return []
        if current_n is not None and g["n"] != current_n:
            return []
        return list(g["rows"])

    def day_ago(self, tag=None, days=7):
        """The day record about `days` days before the latest (or the oldest one kept)."""
        tag = tag or self.player()
        hist = self.history.get(tag)
        if not hist:
            return None
        target = hist[-1].get("n", 0) - days
        best = hist[0]
        for r in hist:
            if r.get("n", 0) <= target:
                best = r
        return best

    def recent_events(self, since_n=None, limit=40):
        ev = [e for e in self.events if since_n is None or e.get("n", 0) >= since_n]
        return ev[-limit:]

    def status(self):
        d = self.day()
        out = {"log": self.log_path, "records": self.lines, "bad_lines": self.bad, "restarts_seen": self.restarts,
               "rewinds": self.rewinds, "player": self.player(), "generation": self.generation,
               "kinds": sorted(set(self.latest) | set(self.groups) | ({"ev"} if self.events else set()))}
        if d:
            out["last_day"] = d["_date"]
            out["last_n"] = d.get("n")
            out["age_s"] = round(time.time() - d["_wall"], 1)
        wld = self.get("wld")
        if wld:
            out["last_week"] = wld["_date"]
        return out

    def snapshot(self, tag=None):
        """Everything known about the player's country, grouped for hoi4_state."""
        tag = tag or self.player()
        d = self.day(tag)
        if not d:
            return None
        n = d.get("n")
        out = {"tag": tag, "date": d["_date"], "n": n, "wall": d["_wall"], "day": _public(d)}
        focus = self.focus_snapshot(tag, n)
        if focus is not None:
            out["focus"] = focus
        elif self.focus_ledger.trees:
            out["focus_unknown"] = "no complete focus batch for this country and day"
        for kind in ("pol", "fac", "subject", "res", "eq", "watch"):
            r = self.get(kind, tag)
            if r and r.get("n") == n:
                out[kind] = unflatten(_public(r), kind)
            elif r and kind in ("fac", "subject"):
                pass                               # no longer in a faction / no longer a subject
            elif r:
                out[kind + "_stale"] = unflatten(_public(r), kind)
        out["wars"] = [_public(r) for r in self.group("war", n) if r.get("t") == tag]
        wld = self.get("wld", tag)
        if wld:
            wn = wld.get("n")
            out["week"] = {"date": wld["_date"], "n": wn, "world": _public(wld),
                           "countries": list({r.get("c"): _public(r) for r in self.group("wc", wn)}.values()),
                           "enemy_pairs": _pairs(self.group("we", wn)),
                           "front": _fronts(self.group("front", wn)),
                           "lost": [_public(r) for r in self.group("lost", wn)]}
        out["events"] = [_public(e) for e in self.recent_events(limit=40)]
        return out


def _public(rec):
    return {k: v for k, v in rec.items() if not k.startswith("_") or k == "_date"}


def _pairs(rows):
    seen = set()
    out = []
    for r in rows:
        a, b = r.get("a"), r.get("b")
        key = tuple(sorted((a, b)))
        if a and b and key not in seen:
            seen.add(key)
            out.append(list(key))
    return out


def _fronts(rows):
    """Deduplicated front states split by side."""
    out = {"enemy": {}, "own": {}}
    for r in rows:
        side = r.get("side", "enemy")
        s = r.get("s")
        if s is None:
            continue
        cur = out.setdefault(side, {}).setdefault(s, {"s": s, "name": r.get("name"), "ctrl": r.get("ctrl"),
                                                       "owner": r.get("owner")})
        if r.get("vs") is not None:
            cur.setdefault("vs", [])
            if r["vs"] not in cur["vs"]:
                cur["vs"].append(r["vs"])
    return {k: sorted(v.values(), key=lambda x: x["s"]) for k, v in out.items()}
