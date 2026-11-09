"""A history file may load its land order of battle with `oob = "..."` as well as `set_oob = "..."`.

Sweden, Denmark and Norway (Arms Against Tyranny histories) use the plain key: before the fix the
dossier showed 0 divisions for them. The fixture game is copied and BBB's `set_oob` is renamed.
"""
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts.game import Game  # noqa: E402
from hoi4facts.world import World  # noqa: E402

FIXTURE = os.path.join(ROOT, "tests", "fixtures", "dossier", "game")


class PlainOobKey(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp()
        self.game = os.path.join(self.tmp, "game")
        shutil.copytree(FIXTURE, self.game)
        path = os.path.join(self.game, "history", "countries", "BBB - Beta.txt")
        with open(path, encoding="utf-8-sig") as fh:
            text = fh.read()
        self.assertIn('set_oob = "BBB_1936"', text)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(text.replace('set_oob = "BBB_1936"', 'oob = "BBB_1936"'))

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_world_reads_the_plain_key(self):
        w = World(Game(self.game, cache_dir=self.tmp), "BBB", "1936.1.1", (), "meta")
        files = w.oob("BBB")["files"]
        self.assertEqual([(f["name"], f["kind"]) for f in files], [("BBB_1936", "oob")])
        self.assertTrue(w.oob("BBB")["templates"])

    def test_game_history_effects_read_the_plain_key(self):
        g = Game(self.game, cache_dir=self.tmp)
        start = g.history_effects("history/countries/BBB - Beta.txt", None, "1936.1.1")
        self.assertEqual(start["oobs"], ["BBB_1936"])


if __name__ == "__main__":
    unittest.main()
