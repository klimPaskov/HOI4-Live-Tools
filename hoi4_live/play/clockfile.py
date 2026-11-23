"""Publish the game clock to a file (opt-in, off by default).

While a run is attached to the game, the supervisor can write the game's pause flag, speed and date to one small
JSON file about once a second, for other local programs that want the real clock without reading the game
themselves (an overlay, a stream widget, a timer). Nothing is written unless a path is set:

    the run's settings.json   {"clock_file": "/path/to/clock.json"}   (re-read every second; "" or null: off)
    or the environment        HOI4_LIVE_CLOCK_FILE=/path/to/clock.json

`~` is expanded; a relative path is taken inside the game's user folder (the folder with `logs/` and
`settings.txt`). The file is replaced in one step (never half written) and removed when the run ends:

    {"paused": false, "speed": 3, "speed_internal": 2, "date": "1936.03.14.12", "at": 1763900000.12,
     "by": "hoi4-live memory"}

`speed` is the game's own setting 1-5, `speed_internal` the runtime's 0-4 (`hoi4_time`), `at` the wall clock of
the reading in seconds since 1970. A reader should treat a file older than a few seconds as stale (the game or
the run has stopped). Only the read lane is used (`hoi4_overview`); nothing is sent to the game.
"""
from __future__ import annotations

import json
import os
import tempfile
import threading
import time

from . import runs

SETTING = "clock_file"
ENV = "HOI4_LIVE_CLOCK_FILE"
EVERY = 1.0


def target(run, user_dir=None, env=None):
    """The file to publish to, or None when publishing is off. The run's setting wins over the environment."""
    env = os.environ if env is None else env
    settings = (runs.read_json(os.path.join(run, runs.SETTINGS)) or {}) if run else {}
    if SETTING in settings:
        value = settings.get(SETTING)
    else:
        value = env.get(ENV)
    if not isinstance(value, str) or not value.strip():
        return None
    path = os.path.expanduser(value.strip())
    if not os.path.isabs(path):
        if not user_dir:
            return None
        path = os.path.join(user_dir, path)
    return path


def record(overview, now):
    """The file's body from one hoi4_overview answer, or None when it holds neither pause nor speed."""
    if not isinstance(overview, dict):
        return None
    paused = overview.get("paused")
    paused = paused if isinstance(paused, bool) else None
    internal = overview.get("speed")
    if isinstance(internal, bool) or not isinstance(internal, int) or not 0 <= internal <= 4:
        internal = None
    if paused is None and internal is None:
        return None
    body = {"paused": paused, "speed": None if internal is None else internal + 1, "speed_internal": internal,
            "date": (overview.get("date") or {}).get("text") if isinstance(overview.get("date"), dict) else None,
            "at": round(float(now), 3), "by": "hoi4-live memory"}
    return body


def write(path, body):
    """Replace the file in one step: a reader never sees half a file."""
    folder = os.path.dirname(path) or "."
    os.makedirs(folder, exist_ok=True)
    fd, tmp = tempfile.mkstemp(prefix=".clock-", suffix=".tmp", dir=folder)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(body, f)
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


class ClockPublisher:
    """`step()` reads the clock once and writes it when publishing is on; `start(stopped)` does that every
    EVERY seconds on a daemon thread until `stopped` is set, then removes the file it wrote."""

    def __init__(self, run, call, user_dir=None, clock=time.time, env=None, ready=lambda: True):
        self.run, self.call, self.user_dir, self.clock, self.env, self.ready = run, call, user_dir, clock, env, ready
        self.written = None
        self.errors = 0

    def step(self):
        path = target(self.run, self.user_dir, self.env)
        if path != self.written and self.written:
            self.remove()                     # the setting moved or was switched off: no stale file is left behind
        if path is None or not self.ready():
            return False
        err, overview = self.call("hoi4_overview", {})
        body = None if err else record(overview, self.clock())
        if body is None:
            return False
        try:
            write(path, body)
        except OSError:
            self.errors += 1
            return False
        self.written = path
        return True

    def remove(self):
        if self.written:
            try:
                os.unlink(self.written)
            except OSError:
                pass
            self.written = None

    def start(self, stopped, every=EVERY):
        def loop():
            while not stopped.wait(every):
                try:
                    self.step()
                except Exception:  # noqa: BLE001 - a failed reading is skipped; the next one runs
                    self.errors += 1
            self.remove()
        thread = threading.Thread(target=loop, name="clock-file", daemon=True)
        thread.start()
        return thread
