"""The dossier cache: one build per game build, mod set, DLCs, tag, date, mode and enemies."""

import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import dossier  # noqa: E402


class Content:
    def __init__(self, sig):
        self.sig = sig

    def signature(self):
        return self.sig


class Game:
    def __init__(self, sig):
        self.content = Content(sig)


class DossierCacheTests(unittest.TestCase):
    def test_key_follows_every_input(self):
        base = dossier.cache_key(Game("a"), "ITA", "1936.1.1", ["d1"], "fair")
        self.assertEqual(base, dossier.cache_key(Game("a"), "ITA", "1936.1.1", ["d1"], "fair"))
        for other in (dossier.cache_key(Game("b"), "ITA", "1936.1.1", ["d1"], "fair"),      # game build or mods
                      dossier.cache_key(Game("a"), "ETH", "1936.1.1", ["d1"], "fair"),
                      dossier.cache_key(Game("a"), "ITA", "1939.8.14", ["d1"], "fair"),
                      dossier.cache_key(Game("a"), "ITA", "1936.1.1", [], "fair"),
                      dossier.cache_key(Game("a"), "ITA", "1936.1.1", ["d1"], "meta"),
                      dossier.cache_key(Game("a"), "ITA", "1936.1.1", ["d1"], "fair", ["ETH"])):
            self.assertNotEqual(base, other)

    def test_keep_then_copy(self):
        with tempfile.TemporaryDirectory() as d:
            built = os.path.join(d, "run1", "dossier")
            os.makedirs(built)
            with open(os.path.join(built, "dossier.md"), "w") as f:
                f.write("# dossier\n")
            with open(os.path.join(built, "manifest.json"), "w") as f:
                json.dump({"content_hash": "h", "files": {"dossier.md": 11}}, f)
            cache = os.path.join(d, "cache")
            self.assertIsNone(dossier.cached("k1", os.path.join(d, "run2"), cache))
            self.assertTrue(dossier.keep("k1", built, cache))
            r = dossier.cached("k1", os.path.join(d, "run2", "dossier"), cache)
            self.assertEqual(r["manifest"]["content_hash"], "h")
            self.assertEqual(r["md_bytes"], 11)
            with open(os.path.join(d, "run2", "dossier", "dossier.md")) as f:
                self.assertEqual(f.read(), "# dossier\n")
            self.assertTrue(dossier.keep("k1", built, cache))     # a second keep leaves the first in place


if __name__ == "__main__":
    unittest.main()
