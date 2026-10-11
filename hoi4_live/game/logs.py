"""Game log access: marks (byte offsets) and deltas for error.log, game.log and the other logs in logs/.

HOI4 truncates its logs at launch, so a mark records (size, mtime, first bytes) and a delta read detects a restart
(file shrank or its head changed) and then reads from the start. Repeated lines are folded with a count, which
keeps error.log deltas readable when one bad script line fires every day.
"""

import collections
import os
import re
import time

from . import log_hints

LOG_NAMES = ("error", "game", "system", "setup", "text", "exceptions", "messages", "ai", "graphics", "database")


class LogMark:
    __slots__ = ("size", "mtime", "head", "time")

    def __init__(self, size, mtime, head, t):
        self.size, self.mtime, self.head, self.time = size, mtime, head, t

    def to_dict(self):
        return {"size": self.size, "mtime": self.mtime, "time": self.time}


class Logs:
    def __init__(self, logs_dir):
        self.dir = logs_dir
        self.marks = {}                       # name -> {log -> LogMark}

    def path(self, log):
        name = log if log.endswith(".log") else log + ".log"
        return os.path.join(self.dir, name)

    def _stat(self, log):
        p = self.path(log)
        try:
            st = os.stat(p)
        except OSError:
            return LogMark(0, 0.0, b"", time.time())
        with open(p, "rb") as f:
            head = f.read(256)
        return LogMark(st.st_size, st.st_mtime, head, time.time())

    def mark(self, name, logs=("error", "game", "system")):
        self.marks[name] = {log: self._stat(log) for log in logs}
        return {log: m.to_dict() for log, m in self.marks[name].items()}

    def has_mark(self, name):
        return name in self.marks

    def read_since(self, log, mark_name=None, max_bytes=2 << 20):
        """Text appended to `log` since the named mark (whole file when the mark is missing or the game
        restarted). Returns (text, info)."""
        p = self.path(log)
        if not os.path.exists(p):
            return "", {"exists": False}
        cur = self._stat(log)
        start = 0
        restarted = False
        m = self.marks.get(mark_name, {}).get(log) if mark_name else None
        if m is not None:
            if cur.size < m.size or (m.head and cur.head[:len(m.head)] != m.head[:len(cur.head)]):
                restarted = True
            else:
                start = m.size
        truncated = False
        if cur.size - start > max_bytes:
            start = cur.size - max_bytes
            truncated = True
        with open(p, "rb") as f:
            f.seek(start)
            data = f.read(cur.size - start)
        info = {"exists": True, "from": start, "to": cur.size, "restarted": restarted, "truncated": truncated,
                "mtime": cur.mtime}
        return data.decode("utf-8", "replace"), info

    def delta(self, log, mark_name=None, grep=None, limit=200, fold=True):
        text, info = self.read_since(log, mark_name)
        lines = [l for l in text.splitlines() if l.strip()]
        if grep:
            rx = re.compile(grep, re.I)
            lines = [l for l in lines if rx.search(l)]
        total = len(lines)
        if fold:
            counts = collections.OrderedDict()
            for l in lines:
                key = _strip_time(l)
                if key in counts:
                    counts[key][1] += 1
                else:
                    counts[key] = [l, 1]
            rows = [{"line": v[0], "count": v[1]} if v[1] > 1 else {"line": v[0]} for v in counts.values()]
        else:
            rows = [{"line": l} for l in lines]
        if log == "error":
            for row in rows:
                hint = log_hints.hint_for(row["line"])
                if hint:
                    row["known_mistake"] = hint
        info.update({"log": log, "lines": total, "unique": len(rows), "shown": min(limit, len(rows))})
        return rows[-limit:] if len(rows) > limit else rows, info

    def tail(self, log, n=40):
        p = self.path(log)
        if not os.path.exists(p):
            return []
        size = os.path.getsize(p)
        with open(p, "rb") as f:
            f.seek(max(0, size - 256 * 1024))
            data = f.read().decode("utf-8", "replace")
        return data.splitlines()[-n:]

    def idle_seconds(self, log="game"):
        p = self.path(log)
        try:
            return time.time() - os.path.getmtime(p)
        except OSError:
            return None

    def wait_idle(self, log="game", idle=8.0, timeout=300.0, first=0.0, progress=None, check=None):
        """Wait until the log has not been written for `idle` seconds. check() may raise to abort."""
        t0 = time.time()
        if first:
            time.sleep(first)
        while time.time() - t0 < timeout:
            if check:
                check()
            s = self.idle_seconds(log)
            if s is not None and s >= idle:
                return True
            if progress:
                progress(f"waiting for {log}.log to settle ({int(time.time() - t0)} s)")
            time.sleep(1.0)
        return False


_TS = re.compile(r"^\[\d{2}:\d{2}:\d{2}\]\[[^\]]*\]")


def _strip_time(line):
    """Error lines start with [hh:mm:ss][file.cpp:123]; fold repeats regardless of the timestamp."""
    return _TS.sub("", line).strip()
