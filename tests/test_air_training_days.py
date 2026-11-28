"""facts.py air-training: how long air training pays, from the game's own defines (no number in code)."""
import contextlib
import io
import json
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))
from hoi4facts import facts  # noqa: E402

FIXTURE = os.path.join(ROOT, "tests", "fixtures", "dossier", "game")


class AirTrainingDays(unittest.TestCase):
    def test_days_are_the_training_cap_over_the_daily_gain_read_from_the_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            game = os.path.join(tmp, "game")
            shutil.copytree(FIXTURE, game)
            with open(os.path.join(game, "common", "defines", "zz_air.lua"), "w", encoding="utf-8") as fh:
                fh.write("NDefines.NAir.AIR_WING_XP_TRAINING_MAX = 240.0\n"
                         "NDefines.NAir.AIR_WING_XP_TRAINING_MISSION_GAIN_DAILY = 8.0\n")
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = facts.main(["air-training", "--json", "--game", game, "--dlc", "none", "--userdir", game])
            self.assertEqual(rc, 0)
            data = json.loads(buf.getvalue())
            self.assertEqual(data["days"], 30.0)
            self.assertEqual(data["defines"]["AIR_WING_XP_TRAINING_MAX"]["v"], 240.0)
            self.assertIn("zz_air.lua", data["defines"]["AIR_WING_XP_TRAINING_MAX"]["src"])


if __name__ == "__main__":
    unittest.main()
