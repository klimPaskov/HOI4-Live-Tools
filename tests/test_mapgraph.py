"""Tests for scripts/hoi4facts/mapgraph.py.

A synthetic map (tiny BMPs with row padding, bottom-up rows) runs everywhere. The golden test checks the
Italo-Ethiopian table in skills/hoi4-campaign-playbook/references/italo-ethiopian-war.md section 7.1 against
the real game files and is skipped when the game folder is absent.
"""

import importlib.util
import os
import shutil
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
_spec = importlib.util.spec_from_file_location("mapgraph", os.path.join(ROOT, "scripts", "hoi4facts", "mapgraph.py"))
mg = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(mg)

GAME_DIR = os.environ.get("HOI4_DIR", mg.DEFAULT_GAME_DIR)


def _write_bmp(path, rows_top_down, bpp, palette=None):
    """Bottom-up BMP. 24 bpp rows hold (r, g, b) tuples, 8 bpp rows hold palette indices."""
    h, w = len(rows_top_down), len(rows_top_down[0])
    stride = ((w * bpp + 31) // 32) * 4
    pal = b""
    if bpp == 8:
        pal = b"".join(bytes((c[2], c[1], c[0], 0)) for c in (palette or [(i, i, i) for i in range(256)]))
    offset = 14 + 40 + len(pal)
    body = b""
    for row in reversed(rows_top_down):
        if bpp == 24:
            line = b"".join(bytes((b, g, r)) for r, g, b in row)
        else:
            line = bytes(row)
        body += line + b"\0" * (stride - len(line))
    head = b"BM" + (offset + len(body)).to_bytes(4, "little") + b"\0\0\0\0" + offset.to_bytes(4, "little")
    dib = (40).to_bytes(4, "little") + w.to_bytes(4, "little", signed=True) + h.to_bytes(4, "little", signed=True)
    dib += (1).to_bytes(2, "little") + bpp.to_bytes(2, "little") + b"\0" * 4 + len(body).to_bytes(4, "little")
    dib += b"\0" * 16
    with open(path, "wb") as f:
        f.write(head + dib + pal + body)


class SyntheticMap(unittest.TestCase):
    """3 x 4 map (rows padded to 12 and 4 bytes):

        A A B        river pixel at C (x 1, row 1)
        A C B
        D D B
        S S S        S is sea; E (6) has no pixels and reaches D through a strait
    """

    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp()
        d = cls.tmp
        for sub in ("map/strategicregions", "history/states", "common/terrain"):
            os.makedirs(os.path.join(d, *sub.split("/")))
        col = {1: (10, 0, 0), 2: (20, 0, 0), 3: (30, 0, 0), 4: (40, 0, 0), 5: (0, 0, 200), 6: (60, 0, 0)}
        A, B, C, D, S = col[1], col[2], col[3], col[4], col[5]
        _write_bmp(os.path.join(d, "map", "provinces.bmp"), [[A, A, B], [A, C, B], [D, D, B], [S, S, S]], 24)
        land, river, sea = 255, 3, 254
        _write_bmp(os.path.join(d, "map", "rivers.bmp"),
                   [[land, land, land], [land, river, land], [land, land, land], [sea, sea, sea]], 8)
        terr = {1: "plains", 2: "plains", 3: "plains", 4: "mountain", 5: "ocean", 6: "plains"}
        with open(os.path.join(d, "map", "definition.csv"), "w") as f:
            f.write("0;0;0;0;land;false;unknown;0\n")
            for pid, (r, g, b) in col.items():
                kind = "sea" if pid == 5 else "land"
                f.write("%d;%d;%d;%d;%s;%s;%s;1\n" % (pid, r, g, b, kind, "true" if pid != 3 else "false", terr[pid]))
        with open(os.path.join(d, "map", "adjacencies.csv"), "w") as f:
            f.write("From;To;Type;Through;start_x;start_y;stop_x;stop_y;adjacency_rule_name;Comment\n")
            f.write("1;2;impassable;-1;-1;-1;-1;-1;;wall\n4;6;sea;5;-1;-1;-1;-1;;strait\n-1;-1;;-1;-1;-1;-1;-1;-1\n")
        with open(os.path.join(d, "map", "supply_nodes.txt"), "w") as f:
            f.write("1 1\n")
        with open(os.path.join(d, "map", "railways.txt"), "w") as f:
            f.write("2 2 1 3\n")
        with open(os.path.join(d, "map", "strategicregions", "1-Test.txt"), "w") as f:
            f.write('strategic_region={\n id=1\n name="SR_1"\n provinces={\n 1 2 3 4 6 # land\n }\n}\n')
        with open(os.path.join(d, "history", "states", "1-Test.txt"), "w") as f:
            f.write("state={ id=1 name=\"S1\" history={ owner = AAA victory_points = { 1 5 } #note\n"
                    " buildings = { infrastructure = 5 3 = { naval_base = 2 } } 1939.1.1 = { owner = BBB } }\n"
                    " provinces={ 1 2 3 4 6 } }\n")
        with open(os.path.join(d, "common", "terrain", "00_terrain.txt"), "w") as f:
            f.write("categories = { plains = { movement_cost = 1.0 } mountain = { movement_cost = 2.0 }"
                    " ocean = { movement_cost = 1.0 is_water = yes } }\n")
        cls.cache = os.path.join(d, "cache")
        cls.g = mg.load(game_dir=d, cache_dir=cls.cache, rebuild=True)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def test_edges_and_rivers(self):
        adj = self.g["adj"]
        self.assertEqual(sorted(adj[1]), [2, 3, 4])
        self.assertEqual(sorted(adj[4]), [1, 2, 3, 5, 6])
        self.assertFalse(adj[1][2]["river_crossing"])  # A|B on row 0, no river pixel there
        self.assertTrue(adj[1][3]["river_crossing"])
        self.assertTrue(adj[2][3]["river_crossing"])
        self.assertTrue(adj[3][4]["river_crossing"])
        self.assertFalse(adj[1][4]["river_crossing"])
        self.assertEqual(adj[1][3]["border"], 2)  # one horizontal pair in row 1, one vertical pair at x 1
        self.assertEqual(adj[2][4]["border"], 1)  # only D|B in row 2
        self.assertTrue(adj[1][2].get("impassable"))
        self.assertEqual(adj[4][6]["kind"], "strait")
        self.assertEqual(adj[4][6]["through"], 5)

    def test_rows_are_top_down(self):
        p = self.g["provinces"]
        self.assertEqual(p[5]["xy"], [1.0, 3.0])
        self.assertEqual(p[1]["pixels"], 3)
        self.assertEqual(p[1]["xy"], [round(1 / 3, 1), round(1 / 3, 1)])

    def test_facts(self):
        p = self.g["provinces"]
        self.assertEqual(p[1]["state"], 1)
        self.assertEqual(p[1]["region"], 1)
        self.assertEqual(p[1]["supply_hub"], 1)
        self.assertEqual(p[3]["railway"], 2)
        self.assertEqual(p[3]["naval_base"], 2)
        self.assertFalse(p[3]["coastal"])
        st = self.g["states"][1]
        self.assertEqual((st["owner"], st["infrastructure"]), ("AAA", 5))  # dated block not applied
        self.assertEqual(st["victory_points"], {1: 5.0})
        self.assertEqual(self.g["terrain_costs"]["mountain"], 2.0)

    def test_route(self):
        r = mg.route(self.g, 1, 2, weights={"river": 0.2})
        self.assertEqual(r["path"], [1, 3, 2])
        self.assertEqual(r["rivers"], 2)
        r = mg.route(self.g, 1, 2, avoid_rivers=True, weights={"river": 0.2})
        self.assertEqual(r["path"], [1, 4, 2])
        self.assertEqual(r["river_crossings"], [])
        self.assertEqual(r["cost"], 3.0)
        r = mg.route(self.g, 1, 6)
        self.assertEqual(r["path"], [1, 4, 6])
        self.assertEqual(r["cost"], 2.0 + 1.0 + 3.0)
        self.assertIsNone(mg.route(self.g, 1, 5))  # sea is not a land destination

    def test_route_keeps_out_of_closed_provinces(self):
        # 4 is the dry way from 1 to 2; closed (a neutral's land), the route takes 3, and with both closed
        # there is none
        self.assertEqual(mg.route(self.g, 1, 2, avoid_rivers=True)["path"], [1, 4, 2])
        self.assertEqual(mg.route(self.g, 1, 2, avoid_rivers=True, allowed=lambda p: p != 4)["path"], [1, 3, 2])
        self.assertIsNone(mg.route(self.g, 1, 2, allowed=lambda p: p not in (3, 4)))
        # the start is never closed to the mover standing on it
        self.assertEqual(mg.route(self.g, 1, 2, allowed=lambda p: p != 1)["steps"], 2)

    def test_cache_hit(self):
        files = [f for f in os.listdir(self.cache) if f.endswith(".json.gz")]
        self.assertEqual(len(files), 1)
        mg._MEM.clear()
        g = mg.load(game_dir=self.tmp, cache_dir=self.cache)
        self.assertEqual(g["signature"], self.g["signature"])
        self.assertTrue(g["adj"][1][3]["river_crossing"])

    def test_content_path_layering(self):
        """A mod-style content_path that overrides one file changes the signature and the result."""
        mod = os.path.join(self.tmp, "mod")
        os.makedirs(os.path.join(mod, "map"), exist_ok=True)
        with open(os.path.join(mod, "map", "supply_nodes.txt"), "w") as f:
            f.write("1 4\n")

        def content_path(rel):
            p = os.path.join(mod, *rel.split("/"))
            return p if os.path.exists(p) else os.path.join(self.tmp, *rel.split("/"))

        g = mg.load(content_path=content_path, cache_dir=self.cache)
        self.assertNotEqual(g["signature"], self.g["signature"])
        self.assertEqual(g["supply_hubs"], {4: 1})


# Section 7.1 of italo-ethiopian-war.md: land neighbours, True = "~R" (river crossing).
TABLE_7_1 = {
    13250: {5091: False, 12723: False, 12766: False, 13237: False, 13238: False},
    12766: {5091: False, 8043: False, 13250: False, 13238: False},
    12723: {5017: False, 5091: False, 13250: False, 13251: False, 13237: False, 2088: False},
    13251: {12723: False, 1984: True, 13237: True, 2088: False},
    8043: {12766: False, 13238: False, 5041: False, 13236: False, 4985: False},
    13237: {12723: False, 13250: False, 13251: True, 1984: True, 2072: True, 13238: False},
    13238: {8043: False, 12766: False, 13250: False, 2072: False, 13237: False, 13236: False},
    2072: {1984: True, 13237: True, 13238: False, 7944: True, 13236: False},
    7944: {1984: True, 13241: True, 2072: True, 5097: True, 5041: False, 13236: False},
    5097: {13239: True, 13242: True, 13241: True, 7944: True, 5041: False, 13240: False},
    13236: {8043: False, 2072: False, 13238: False, 7944: False, 5041: False},
    5041: {4985: False, 8124: False, 8043: False, 5097: False, 7944: False, 13236: False, 13240: False},
    13240: {8124: False, 13239: False, 7980: False, 5097: False, 5041: False},
    13239: {5010: False, 8036: False, 7980: False, 13249: False, 13242: True, 5097: True, 13240: False},
    5010: {8036: False, 10734: False, 13239: False, 12856: True, 13242: True},
    8036: {5010: False, 10734: False, 13239: False, 13249: False, 10775: False, 13246: False},
}
# The 1984 Gondar row lists 13137 (Lake Tana, a lake) and leaves out two Sudanese land neighbours
# (12862: 11 border pairs, 2088: 1 pair, both river). Checked as a subset.
ROW_1984 = {13251: True, 2040: False, 13241: True, 2072: True, 13237: True, 7944: True}
STATE_TERRAIN = {13250: (550, "hills"), 12766: (550, "mountain"), 13251: (550, "mountain"), 8043: (550, "desert"),
                 13237: (842, "hills"), 2072: (842, "mountain"), 1984: (841, "mountain"), 7944: (843, "mountain"),
                 5097: (843, "mountain"), 13236: (908, "desert"), 13240: (908, "mountain"),
                 13239: (271, "mountain"), 5010: (271, "mountain")}


@unittest.skipUnless(os.path.exists(os.path.join(GAME_DIR, "map", "provinces.bmp")), "game folder absent")
class ItaloEthiopianGolden(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.g = mg.load(game_dir=GAME_DIR)

    def land(self, pid):
        return {nb: e["river_crossing"] for nb, e in mg.neighbours(self.g, pid)}

    def test_table_7_1(self):
        for pid, expected in TABLE_7_1.items():
            with self.subTest(province=pid):
                self.assertEqual(self.land(pid), expected)

    def test_gondar_row(self):
        got = self.land(1984)
        self.assertEqual({k: got.get(k) for k in ROW_1984}, ROW_1984)
        self.assertIn(13137, self.g["adj"][1984])
        self.assertEqual(self.g["provinces"][13137]["type"], "lake")

    def test_named_edges(self):
        self.assertTrue(self.g["adj"][13251][1984]["river_crossing"])
        self.assertFalse(self.g["adj"][13238][2072]["river_crossing"])

    def test_states_and_terrain(self):
        for pid, (sid, terrain) in STATE_TERRAIN.items():
            p = self.g["provinces"][pid]
            self.assertEqual((p["state"], p["terrain"]), (sid, terrain), pid)

    def test_routes(self):
        tigray = [13238, 2072, 7944, 5097, 13239, 5010]
        self.assertEqual(mg.path_crossings(self.g, tigray), [[2072, 7944], [7944, 5097], [5097, 13239]])
        self.assertEqual(mg.path_crossings(self.g, [8043, 13236, 5041, 13240, 13239, 5010]), [])
        r = mg.route(self.g, 13238, 5010, avoid_rivers=True)
        self.assertEqual(r["rivers"], 0)
        self.assertEqual(r["path"][-2:], [13239, 5010])

    def test_supply_regions_infrastructure(self):
        hubs = self.g["supply_hubs"]
        for pid in (13250, 5010, 7980, 8124):
            self.assertIn(pid, hubs)
        lines = [r["provinces"] for r in self.g["railways"]]
        self.assertTrue(any(set(l) == {5010, 13239, 7980, 8124} for l in lines))
        self.assertTrue(any(set(l) == {12766, 13250} for l in lines))
        p = self.g["provinces"]
        self.assertEqual((p[13250]["region"], p[5010]["region"], p[13249]["region"]), (273, 17, 274))
        infra = {sid: self.g["states"][sid]["infrastructure"] for sid in (550, 559, 844, 271, 908, 842)}
        self.assertEqual(infra, {550: 2, 559: 2, 844: 2, 271: 2, 908: 1, 842: 0})
        self.assertEqual(p[12766]["naval_base"], 3)
        self.assertEqual(self.g["states"][550]["owner"], "ITA")


if __name__ == "__main__":
    unittest.main()
