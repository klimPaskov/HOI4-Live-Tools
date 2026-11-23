"""Publishing the game clock to a file (hoi4_live/play/clockfile.py): off by default, the run's setting or the
environment switches it on, the file is replaced whole and removed at the end, and the supervisor starts it."""
from __future__ import annotations

import json
import os
import sys
import threading
import time
import unittest

sys.path.insert(0, os.path.dirname(__file__))

from hoi4_live.play import clockfile, runs  # noqa: E402
from hoi4_live.play import supervisor  # noqa: E402
from test_play_flow import Clock, FakeNative, Folder  # noqa: E402


class Calls:
    def __init__(self, overview):
        self.overview, self.calls = overview, []

    def __call__(self, name, args):
        self.calls.append(name)
        if isinstance(self.overview, Exception):
            raise self.overview
        return False, self.overview


class ClockFileTests(unittest.TestCase):
    def setUp(self):
        import tempfile
        import shutil
        self.dir = tempfile.mkdtemp(prefix="hoi4live_clock_")
        self.addCleanup(shutil.rmtree, self.dir, True)
        self.run_dir = os.path.join(self.dir, "run")
        os.makedirs(self.run_dir)
        self.user_dir = os.path.join(self.dir, "user")

    def settings(self, **values):
        runs.write_json(os.path.join(self.run_dir, runs.SETTINGS), values)

    def test_off_unless_a_path_is_set(self):
        self.assertIsNone(clockfile.target(self.run_dir, self.user_dir, env={}))
        self.settings(clock_file="")
        self.assertIsNone(clockfile.target(self.run_dir, self.user_dir, env={clockfile.ENV: "/x/clock.json"}))
        calls = Calls({"paused": False, "speed": 2})
        pub = clockfile.ClockPublisher(self.run_dir, calls, self.user_dir, env={})
        self.assertFalse(pub.step())
        self.assertEqual(calls.calls, [])          # off: not even a read

    def test_the_run_setting_wins_and_relative_paths_land_in_the_user_folder(self):
        env = {clockfile.ENV: os.path.join(self.dir, "env.json")}
        self.assertEqual(clockfile.target(self.run_dir, self.user_dir, env=env), os.path.normpath(env[clockfile.ENV]))
        self.settings(clock_file="sub/clock.json")
        self.assertEqual(clockfile.target(self.run_dir, self.user_dir, env=env), os.path.join(self.user_dir, "sub", "clock.json"))
        self.assertIsNone(clockfile.target(self.run_dir, None, env=env))   # relative without a user folder: off
        self.settings(clock_file="~/clock.json")
        self.assertEqual(clockfile.target(self.run_dir, None, env={}), os.path.normpath(os.path.expanduser("~/clock.json")))

    def test_record_counts_speed_like_the_game_and_keeps_the_internal_value(self):
        body = clockfile.record({"paused": False, "speed": 2, "date": {"text": "1936.03.14.12"}}, 100.0)
        self.assertEqual(body, {"paused": False, "speed": 3, "speed_internal": 2, "date": "1936.03.14.12",
                                "at": 100.0, "by": "hoi4-live memory"})
        self.assertEqual(clockfile.record({"paused": True}, 1)["speed"], None)
        self.assertIsNone(clockfile.record({"speed": 9, "paused": "yes"}, 1))
        self.assertIsNone(clockfile.record({"speed": True}, 1))
        self.assertIsNone(clockfile.record(None, 1))

    def test_a_step_writes_the_whole_file_and_switching_off_removes_it(self):
        path = os.path.join(self.dir, "out", "clock.json")
        self.settings(clock_file=path)
        pub = clockfile.ClockPublisher(self.run_dir, Calls({"paused": True, "speed": 4}), self.user_dir,
                                       clock=lambda: 5.0, env={})
        self.assertTrue(pub.step())
        with open(path, encoding="utf-8") as f:
            self.assertEqual(json.load(f)["speed"], 5)
        self.assertEqual([n for n in os.listdir(os.path.dirname(path)) if n.endswith(".tmp")], [])
        self.settings(clock_file=None)
        self.assertFalse(pub.step())
        self.assertFalse(os.path.exists(path))

    def test_a_failed_read_writes_nothing(self):
        path = os.path.join(self.dir, "clock.json")
        self.settings(clock_file=path)
        pub = clockfile.ClockPublisher(self.run_dir, lambda n, a: (True, {"error": "gone"}), env={})
        self.assertFalse(pub.step())
        self.assertFalse(os.path.exists(path))
        pub = clockfile.ClockPublisher(self.run_dir, Calls({"speed": 1}), env={}, ready=lambda: False)
        self.assertFalse(pub.step())

    def test_the_thread_publishes_until_stopped_then_removes_the_file(self):
        path = os.path.join(self.dir, "clock.json")
        self.settings(clock_file=path)
        stopped = threading.Event()
        thread = clockfile.ClockPublisher(self.run_dir, Calls({"paused": False, "speed": 1}), env={}).start(stopped, every=0.01)
        deadline = time.time() + 5
        while not os.path.exists(path) and time.time() < deadline:
            time.sleep(0.01)
        self.assertTrue(os.path.exists(path))
        stopped.set()
        thread.join(5)
        self.assertFalse(os.path.exists(path))


class SupervisorClockFileTests(Folder):
    def test_the_supervisor_publishes_the_clock_when_the_run_asks_and_not_otherwise(self):
        run = self.make_run()
        runs.record_user_files(self.user_dir, run)
        clock = Clock()
        s = supervisor.Supervisor(run, self.prefix, lambda: FakeNative(), clock=clock, sleep=clock.sleep,
                                  swap=lambda: None, game_memory=lambda: None)
        self.addCleanup(s.close_records)
        self.addCleanup(s.stopped.set)
        self.assertTrue(s.start())
        self.assertIsNotNone(s.clock_file)
        self.assertFalse(s.clock_file.step())                     # no setting: nothing read, nothing written
        settings = runs.read_json(os.path.join(run, runs.SETTINGS)) or {}
        runs.write_json(os.path.join(run, runs.SETTINGS), dict(settings, clock_file="clock/now.json"))
        self.assertTrue(s.clock_file.step())                      # relative: inside the game's user folder
        with open(os.path.join(self.user_dir, "clock", "now.json"), encoding="utf-8") as f:
            body = json.load(f)
        self.assertEqual((body["paused"], body["date"]), (True, "1936.01.01.12"))


if __name__ == "__main__":
    unittest.main()
