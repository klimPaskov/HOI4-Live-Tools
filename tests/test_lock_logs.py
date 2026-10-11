"""Desktop lock (exclusive file, re-entrancy, sticky holds, stale takeover) and log marks/deltas."""

import json
import os
import shutil
import tempfile
import time
import unittest
from unittest import mock

import helpers  # noqa: F401

from hoi4_live.desktop.lock import DesktopLock
from hoi4_live.errors import LockBusy
from hoi4_live.game.logs import Logs


class LockTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.path = os.path.join(self.dir, "desk.lock")

    def tearDown(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_acquire_release_contents(self):
        lk = DesktopLock(self.path, wait_seconds=0)
        with lk.hold("unit test"):
            with open(self.path, encoding="utf-8") as handle:
                data = json.load(handle)
            self.assertEqual(data["pid"], os.getpid())
            self.assertEqual(data["purpose"], "unit test")
            self.assertIn("time", data)
        self.assertFalse(os.path.exists(self.path))

    def test_reentrant(self):
        lk = DesktopLock(self.path, wait_seconds=0)
        with lk.hold("outer"):
            with lk.hold("inner"):
                self.assertTrue(os.path.exists(self.path))
            self.assertTrue(os.path.exists(self.path), "inner release must not drop the outer hold")
        self.assertFalse(os.path.exists(self.path))

    def test_busy_by_live_other_process(self):
        # Inject liveness rather than assume a platform-specific system PID.
        with open(self.path, "w", encoding="utf-8") as f:
            json.dump({"pid": 4, "purpose": "peer", "epoch": time.time()}, f)
        backend = mock.Mock()
        backend.pid_alive.return_value = True
        lk = DesktopLock(self.path, wait_seconds=0, backend=backend)
        with self.assertRaises(LockBusy) as cm:
            lk.acquire("me")
        self.assertIn("peer", str(cm.exception))
        self.assertTrue(os.path.exists(self.path), "a live peer's lock must be left alone")

    def test_live_owner_never_expires_by_age(self):
        with open(self.path, "w", encoding="utf-8") as f:
            json.dump({"pid": 4, "purpose": "old", "epoch": time.time() - 3600}, f)
        backend = mock.Mock()
        backend.pid_alive.return_value = True
        lk = DesktopLock(self.path, stale_seconds=600, wait_seconds=0, backend=backend)
        with self.assertRaises(LockBusy):
            lk.acquire("me")

    def test_stale_by_dead_pid(self):
        with open(self.path, "w", encoding="utf-8") as f:
            json.dump({"pid": 999999, "purpose": "dead", "epoch": time.time()}, f)
        lk = DesktopLock(self.path, wait_seconds=0)
        lk.acquire("me")
        lk.release()
        self.assertFalse(os.path.exists(self.path))

    def test_sticky_hold_survives_calls(self):
        lk = DesktopLock(self.path, wait_seconds=0)
        lk.acquire("run", sticky=True)
        with lk.hold("tool call"):
            pass
        self.assertTrue(os.path.exists(self.path))
        lk.release(force_sticky=True)
        self.assertFalse(os.path.exists(self.path))

    def test_holder_report(self):
        lk = DesktopLock(self.path, wait_seconds=0)
        self.assertIsNone(lk.holder())
        with lk.hold("x"):
            h = lk.holder()
            self.assertTrue(h["ours"])
            self.assertFalse(h["stale"])


class LogTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.logs = Logs(self.dir)
        self.err = os.path.join(self.dir, "error.log")
        with open(self.err, "w", encoding="utf-8") as f:
            f.write("[10:00:00][no_game_date][a.cpp:1]: old line\n")

    def tearDown(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def append(self, text):
        with open(self.err, "a", encoding="utf-8") as f:
            f.write(text)

    def test_delta_since_mark_with_folding(self):
        self.logs.mark("m", ("error",))
        self.append("[10:00:01][1936.01.02.12][b.cpp:2]: repeated problem\n"
                    "[10:00:02][1936.01.03.12][b.cpp:2]: repeated problem\n"
                    "[10:00:03][1936.01.03.12][c.cpp:3]: other\n")
        rows, info = self.logs.delta("error", "m")
        self.assertEqual(info["lines"], 3)
        self.assertEqual(info["unique"], 2)
        self.assertEqual(rows[0]["count"], 2)
        self.assertFalse(info["restarted"])

    def test_restart_detected(self):
        self.logs.mark("m", ("error",))
        with open(self.err, "w", encoding="utf-8") as f:
            f.write("[11:00:00][no_game_date][d.cpp:4]: fresh\n")
        rows, info = self.logs.delta("error", "m")
        self.assertTrue(info["restarted"])
        self.assertEqual(rows[0]["line"].split(": ")[-1], "fresh")

    def test_grep_and_missing(self):
        self.logs.mark("m", ("error",))
        self.append("[x][y][z]: modx thing\n[x][y][z]: vanilla thing\n")
        rows, info = self.logs.delta("error", "m", grep="modx")
        self.assertEqual(len(rows), 1)
        text, info = self.logs.read_since("nope", "m")
        self.assertEqual(text, "")
        self.assertFalse(info["exists"])


if __name__ == "__main__":
    unittest.main()
