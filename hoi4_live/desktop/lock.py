"""Cross-process desktop lock: one file, %TEMP%/hoi4_live_desktop.lock, holding {pid, purpose, time}.

Every acting tool holds it for the duration of the action only, so peer agents on the shared desktop can take turns.
A holder that died, or that has not refreshed the file for `stale_seconds` (10 minutes), is considered stale and
the lock is taken over. An explicit hold (hoi4_lock action=acquire) keeps it across tool calls until released;
every acting call refreshes the timestamp.
"""

import contextlib
import datetime
import json
import os
import threading
import time

from ..errors import LockBusy
from . import win32


class DesktopLock:
    def __init__(self, path, stale_seconds=600, wait_seconds=20, pid=None, owner="hoi4-live-tools"):
        self.path = path
        self.stale_seconds = stale_seconds
        self.wait_seconds = wait_seconds
        self.pid = pid or os.getpid()
        self.owner = owner
        self.depth = 0
        self.sticky = False
        self.purpose = None
        self._mutex = threading.RLock()

    # ------------------------------------------------------------------ file helpers
    def read(self):
        try:
            with open(self.path, encoding="utf-8") as f:
                data = json.load(f)
            if not isinstance(data, dict):
                raise ValueError
            return data
        except FileNotFoundError:
            return None
        except (ValueError, OSError):
            # unreadable or half-written: treat as held now, stale after the usual timeout (by file mtime)
            try:
                return {"pid": None, "purpose": "unreadable lock file", "epoch": os.path.getmtime(self.path)}
            except OSError:
                return None

    def _payload(self, purpose):
        now = time.time()
        return {"pid": self.pid, "purpose": purpose, "owner": self.owner, "epoch": now,
                "time": datetime.datetime.fromtimestamp(now).isoformat(timespec="seconds")}

    def _write_new(self, purpose):
        fd = os.open(self.path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        with os.fdopen(fd, "w", encoding="utf-8") as f:
            json.dump(self._payload(purpose), f)

    def _rewrite(self, purpose):
        tmp = self.path + f".{self.pid}.tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(self._payload(purpose), f)
        os.replace(tmp, self.path)

    def is_stale(self, info):
        if info is None:
            return True
        age = time.time() - float(info.get("epoch") or 0)
        if age > self.stale_seconds:
            return True
        pid = info.get("pid")
        return bool(pid) and pid != self.pid and not win32.pid_alive(pid)

    def holder(self):
        info = self.read()
        if info is None:
            return None
        info = dict(info)
        info["age_s"] = round(time.time() - float(info.get("epoch") or 0), 1)
        info["stale"] = self.is_stale(info)
        info["ours"] = info.get("pid") == self.pid
        return info

    # ------------------------------------------------------------------ acquire / release
    def acquire(self, purpose, wait=None, sticky=False):
        wait = self.wait_seconds if wait is None else wait
        deadline = time.time() + wait
        with self._mutex:
            while True:
                info = self.read()
                if info is not None and info.get("pid") == self.pid:
                    # re-entrant: keep the outermost purpose (a sticky run or the enclosing tool call)
                    keep = self.purpose if self.sticky else (info.get("purpose") if self.depth > 0 else purpose)
                    self._rewrite(keep or purpose)
                    break
                if info is None:
                    try:
                        self._write_new(purpose)
                        break
                    except FileExistsError:
                        continue
                if self.is_stale(info):
                    # take over: rename first so two contenders cannot both delete-and-create
                    victim = self.path + f".stale.{self.pid}"
                    try:
                        os.replace(self.path, victim)
                        os.remove(victim)
                    except OSError:
                        pass
                    continue
                if time.time() >= deadline:
                    raise LockBusy(
                        f"the desktop lock is held by pid {info.get('pid')} ({info.get('purpose')}, "
                        f"{round(time.time() - float(info.get('epoch') or 0))} s ago); wait and retry",
                        {"lock": self.path, "holder": info})
                time.sleep(0.5)
            if sticky:
                self.sticky = True
                self.purpose = purpose
            self.depth += 1
        return True

    def release(self, force_sticky=False):
        with self._mutex:
            if self.depth > 0:
                self.depth -= 1
            if force_sticky:
                self.sticky = False
                self.depth = 0
            if self.depth == 0 and not self.sticky:
                info = self.read()
                if info is not None and info.get("pid") == self.pid:
                    try:
                        os.remove(self.path)
                    except OSError:
                        pass
                return True
        return False

    def refresh(self):
        """Heartbeat during long actions."""
        with self._mutex:
            info = self.read()
            if info is not None and info.get("pid") == self.pid:
                self._rewrite(info.get("purpose") or "refresh")

    @contextlib.contextmanager
    def hold(self, purpose, wait=None):
        self.acquire(purpose, wait)
        try:
            yield self
        finally:
            self.release()

    def held_by_us(self):
        info = self.read()
        return info is not None and info.get("pid") == self.pid

    def release_all(self):
        """Process exit: drop our lock whatever the depth."""
        with self._mutex:
            self.sticky = False
            self.depth = 0
            info = self.read()
            if info is not None and info.get("pid") == self.pid:
                try:
                    os.remove(self.path)
                except OSError:
                    pass
