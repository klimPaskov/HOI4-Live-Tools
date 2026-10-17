"""Save metadata must belong to this run before it is called current."""
import os
from pathlib import Path
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.game import gamemode


class GameModeFreshnessTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.session = SimpleNamespace(cfg=SimpleNamespace(saves_dir=self.tmp.name))
        self.path = Path(self.tmp.name) / "old.hoi4"
        self.path.write_text('HOI4txt\nplayer="GER"\ndifficulty="normal"\n ironman=1\n')

    def test_recent_save_before_launch_is_not_current(self):
        self.assertFalse(gamemode.current(self.session)["known"])
        gamemode.begin_run(self.session, time.time())
        self.assertFalse(gamemode.current(self.session)["known"])

    def test_equal_timestamp_prelaunch_save_is_not_current(self):
        written = os.path.getmtime(self.path)
        gamemode.begin_run(self.session, written)
        with mock.patch.object(gamemode.time, 'time', return_value=written + 1):
            self.assertFalse(gamemode.current(self.session)["known"])

    def test_unavailable_prelaunch_inventory_requires_verified_binding(self):
        with mock.patch.object(gamemode.os, 'scandir', side_effect=PermissionError):
            gamemode.begin_run(self.session, time.time() - 10)
        self.assertFalse(gamemode.current(self.session)["known"])
        gamemode.bind_save(self.session, str(self.path))
        self.assertTrue(gamemode.current(self.session)["known"])

    def test_new_save_is_evidence_but_new_campaign_clears_it(self):
        gamemode.begin_run(self.session, time.time() - 10)
        self.path.write_text('HOI4txt\nplayer="GER"\ndifficulty="normal"\n ironman=1\n# written after launch\n')
        now = time.time() - 1
        os.utime(self.path, (now, now))
        result = gamemode.current(self.session)
        self.assertTrue(result["known"])
        result["ironman"] = False
        self.assertTrue(gamemode.current(self.session)["ironman"])
        gamemode.begin_run(self.session)
        self.assertFalse(gamemode.current(self.session)["known"])

    def test_verified_load_can_bind_old_file_and_detect_replacement(self):
        old = time.time() - 24 * 3600
        os.utime(self.path, (old, old))
        gamemode.begin_run(self.session)
        gamemode.bind_save(self.session, str(self.path))
        self.assertEqual(gamemode.current(self.session)["source"], "verified_save_or_load")
        self.path.write_text('HOI4txt\nplayer="POL"\n')
        os.utime(self.path, (old, old))
        self.assertFalse(gamemode.current(self.session)["known"])

    def test_missing_save_or_future_timestamp_is_unknown(self):
        gamemode.begin_run(self.session, time.time() - 10)
        future = time.time() + 100
        os.utime(self.path, (future, future))
        self.assertFalse(gamemode.current(self.session)["known"])
        self.path.unlink()
        self.assertFalse(gamemode.current(self.session)["known"])

    def test_torn_read_is_rejected(self):
        gamemode.begin_run(self.session, time.time() - 10)
        self.path.write_text('HOI4txt\nplayer="GER"\n# new save\n')
        now = time.time() - 1
        os.utime(self.path, (now, now))
        original = gamemode.read_header
        def changed(path):
            result = original(path)
            self.path.write_text('HOI4txt\nplayer="POL"\n')
            return result
        with mock.patch.object(gamemode, "read_header", side_effect=changed):
            self.assertFalse(gamemode.current(self.session)["known"])


if __name__ == "__main__":
    unittest.main()
