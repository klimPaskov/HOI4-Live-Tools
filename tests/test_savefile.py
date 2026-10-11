"""Plain-text save parsing: header, structural index, paths and globs, repeated keys, strings with braces and
escaped quotes, raw find mapped to paths, binary/zip handling, the query layer and (when available) a real save."""

import os
import shutil
import tempfile
import unittest
import zipfile

from helpers import FIXTURES

from hoi4_live.state import queries
from hoi4_live.state.savefile import SaveError, SaveFile, clean, flatten, parse_block

MINI = os.path.join(FIXTURES, "saves", "mini_text.txt")
REAL = os.environ.get("HOI4_LIVE_REAL_SAVE", os.path.join(
    os.path.expanduser("~"), "OneDrive", "Documents", "Paradox Interactive", "Hearts of Iron IV", "save games",
    "qa_001_fresh_head.hoi4"))


class ParseBlockTests(unittest.TestCase):
    def test_scalars_lists_and_repeats(self):
        v = parse_block(b'a=1 b=2.5 c="x y" d=yes e={ 1 2 3 } f={ g=1 } f={ g=2 } h={ { x=1 } { x=2 } }')
        self.assertEqual(v["a"], 1)
        self.assertEqual(v["b"], 2.5)
        self.assertEqual(v["c"], "x y")
        self.assertEqual(v["d"], "yes")
        self.assertEqual(v["e"], [1, 2, 3])
        self.assertEqual(v["f"], [{"g": 1}, {"g": 2}])
        self.assertEqual(v["_repeated"], ["f"])
        self.assertEqual(v["h"], [{"x": 1}, {"x": 2}])
        self.assertEqual(clean(v)["f"][1]["g"], 2)

    def test_dates_and_operators_stay_strings(self):
        v = parse_block(b'date="1936.1.1.12" when=1936.1.1 x>5')
        self.assertEqual(v["date"], "1936.1.1.12")
        self.assertEqual(v["when"], "1936.1.1")
        self.assertEqual(v["x"], {"op": ">", "value": 5})

    def test_flatten(self):
        leaves = flatten(clean(parse_block(b"a={ b=1 c={ d=2 } } e={ 1 2 }")))
        self.assertEqual(leaves, {"a/b": 1, "a/c/d": 2, "e": [1, 2]})


class SaveFileTests(unittest.TestCase):
    def setUp(self):
        self.sf = SaveFile(MINI, use_cache=False)

    def tearDown(self):
        self.sf.close()

    def test_header(self):
        h = self.sf.header()
        self.assertEqual(h["player"], "USA")
        self.assertEqual(h["date"], "1936.1.11.12")
        self.assertEqual(h["mods"], ["Example Mod"])
        self.assertEqual(h["player_countries"]["USA"]["id"], 1)

    def test_index_paths(self):
        idx = self.sf.index()
        for p in [("countries",), ("countries", "USA"), ("countries", "USA", "variables"), ("states", "64"),
                  ("saved_event_target",), ("saved_event_target[1]",), ("variables",)]:
            self.assertIn(p, idx, p)
        # the brace inside the quoted string and the escaped quote do not break the structure
        self.assertIn(("countries", "GER", "variables"), idx)
        self.assertIn(("states", "65"), idx)

    def test_get_variables(self):
        got = self.sf.get("countries/USA/variables")
        v = got["countries/USA/variables"]
        self.assertEqual(v["test_meter"], 17.5)
        self.assertEqual(v["random"], [375223538, 2982095187])

    def test_get_deeper_than_index(self):
        got = self.sf.get("states/64/buildings/arms_factory/level")
        self.assertEqual(list(got.values()), [2])
        got = self.sf.get("countries/USA/focus/current")
        self.assertEqual(list(got.values()), ["USA_great_depression"])

    def test_glob(self):
        got = self.sf.get("countries/*/variables")
        self.assertEqual(sorted(got), ["countries/GER/variables", "countries/USA/variables"])
        got = self.sf.get("countries/*/variables/test_meter")
        self.assertEqual(got, {"countries/GER/variables/test_meter": 3.0, "countries/USA/variables/test_meter": 17.5})

    def test_string_with_braces(self):
        self.assertEqual(self.sf.get("states/65")["states/65"]["name"], "A { brace } in a string")

    def test_keys(self):
        k = self.sf.keys("countries/USA")
        self.assertIn("variables", k["blocks"])
        self.assertIn("note", k["scalars"])
        top = self.sf.keys("")
        self.assertEqual(top["blocks"]["saved_event_target"], 2)

    def test_find_maps_to_path(self):
        hits = self.sf.find(r"hoi4live_probe")
        self.assertEqual(hits[0]["path"], "countries/USA/flags")
        hits = self.sf.find(r"test_meter=", inside="countries/GER")
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0]["path"], "countries/GER/variables")
        self.assertEqual(hits[0]["line"], "test_meter=3.000")

    def test_cache_roundtrip(self):
        tmp = tempfile.mkdtemp()
        try:
            p = os.path.join(tmp, "copy.hoi4")
            shutil.copy(MINI, p)
            with SaveFile(p) as a:
                i1 = a.index()
            with SaveFile(p) as b:
                i2 = b.index()
            self.assertEqual(i1, i2)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


class FormatTests(unittest.TestCase):
    def test_binary_refused(self):
        tmp = tempfile.mkdtemp()
        try:
            p = os.path.join(tmp, "bin.hoi4")
            with open(p, "wb") as f:
                f.write(b"HOI4bin\x00\x01\x02")
            with self.assertRaises(SaveError) as cm:
                SaveFile(p)
            self.assertIn("save_as_binary=no", str(cm.exception))
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    def test_zipped_text_save(self):
        tmp = tempfile.mkdtemp()
        try:
            p = os.path.join(tmp, "z.hoi4")
            with zipfile.ZipFile(p, "w") as z:
                z.write(MINI, "gamestate")
            with SaveFile(p, use_cache=False) as sf:
                self.assertEqual(sf.header()["player"], "USA")
                self.assertEqual(sf.get("variables/global_counter"), {"variables/global_counter": 42})
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    def test_not_a_save(self):
        tmp = tempfile.mkdtemp()
        try:
            p = os.path.join(tmp, "x.hoi4")
            with open(p, "wb") as f:
                f.write(b"hello")
            with self.assertRaises(SaveError):
                SaveFile(p)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


class QueryTests(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        shutil.copy(MINI, os.path.join(self.dir, "hoi4live_a.hoi4"))
        with open(MINI, encoding="utf-8") as f:
            txt = f.read().replace("test_meter=17.500", "test_meter=18.000")
        txt = txt.replace("global_counter=42", "global_counter=42\n\tnew_var=7")
        with open(os.path.join(self.dir, "hoi4live_b.hoi4"), "w", encoding="utf-8", newline="\n") as f:
            f.write(txt)

    def tearDown(self):
        queries.close_all()
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_resolve_and_info(self):
        p = queries.resolve_save(self.dir, "hoi4live_a")
        self.assertTrue(p.endswith("hoi4live_a.hoi4"))
        self.assertEqual(queries.info(p)["info"]["player"], "USA")
        latest = queries.resolve_save(self.dir, "latest")
        self.assertTrue(latest.endswith(".hoi4"))

    def test_variables_and_flags(self):
        p = queries.resolve_save(self.dir, "hoi4live_a")
        v = queries.script_state(p, "variables", "USA")
        self.assertEqual(v["values"]["test_meter"], 17.5)
        f = queries.script_state(p, "flags", "USA", pattern="probe")
        self.assertEqual(list(f["values"]), ["hoi4live_probe"])
        g = queries.script_state(p, "variables")
        self.assertEqual(g["values"]["last_fired_event_id"], 1)

    def test_diff(self):
        a = queries.resolve_save(self.dir, "hoi4live_a")
        b = queries.resolve_save(self.dir, "hoi4live_b")
        d = queries.diff(a, b, "countries/USA")
        self.assertEqual(d["changed"], {"countries/USA/variables/test_meter": [17.5, 18.0]})
        d = queries.diff(a, b, "variables")
        self.assertEqual(d["added"], {"variables/new_var": 7})


@unittest.skipUnless(os.path.exists(REAL), "real save not available")
class RealSaveTests(unittest.TestCase):
    def test_real_save(self):
        with SaveFile(REAL) as sf:
            h = sf.header()
            self.assertIn("player", h)
            self.assertIn(("countries",), sf.index())
            tag = h["player"]
            v = sf.get(f"countries/{tag}/variables")
            self.assertTrue(v)


if __name__ == "__main__":
    unittest.main()
