"""Whose land a country's units may enter, and routes kept to it (World.land_access, World.land_route).

A unit cannot cross a neutral border, so a route for a plan runs over own, friendly and enemy land only.
The relations come from a small set of country histories; the map is a five-province strip.
"""
import contextlib
import io
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import facts, lint_plan  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.world import World  # noqa: E402

HISTORIES = {
    "AAA - Alpha.txt": "capital = 1\nadd_to_faction = BBB\n"
                       "set_autonomy = { target = EEE autonomy_state = autonomy_puppet }\n"
                       "declare_war_on = { target = DDD type = annex_everything }\n",
    "BBB - Beta.txt": "capital = 2\n",
    "CCC - Gamma.txt": "capital = 3\ngive_military_access = AAA\n",
    "DDD - Delta.txt": "capital = 4\n",
    "EEE - Epsilon.txt": "capital = 5\n",
    "FFF - Zeta.txt": "capital = 6\n",
}

# 1 (AAA) - 2 (FFF) - 3 (DDD) is the short way; 1 - 4 (DDD) - 5 (DDD) - 3 the way round the neutral
OWNER = {1: "AAA", 2: "FFF", 3: "DDD", 4: "DDD", 5: "DDD"}
EDGES = [(1, 2), (2, 3), (1, 4), (4, 5), (5, 3)]


def graph():
    adj = {p: {} for p in OWNER}
    for a, b in EDGES:
        adj[a][b] = adj[b][a] = {"kind": "land", "border": 1, "river_crossing": False}
    return {"provinces": {p: {"type": "land", "terrain": "plains", "state": p} for p in OWNER}, "adj": adj,
            "states": {p: {"owner": t, "infrastructure": 0} for p, t in OWNER.items()}, "terrain_costs": {}}


class LandAccess(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.dir = tempfile.mkdtemp(prefix="landaccess")
        os.makedirs(os.path.join(cls.dir, "history", "countries"))
        os.makedirs(os.path.join(cls.dir, "common", "country_tags"))
        with open(os.path.join(cls.dir, "common", "country_tags", "00_countries.txt"), "w") as fh:
            fh.write("".join(f'{name[:3]} = "countries/{name[:3]}.txt"\n' for name in HISTORIES))
        for name, text in HISTORIES.items():
            with open(os.path.join(cls.dir, "history", "countries", name), "w") as fh:
                fh.write(text)

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.dir, ignore_errors=True)

    def world(self, tag="AAA"):
        w = World(Game(self.dir, cache_dir=tempfile.gettempdir()), tag, "1936.1.1", (), "fair")
        w.graph = graph
        w.owner_of_province = OWNER.get
        return w

    def test_relations_from_the_histories(self):
        acc = self.world().land_access("AAA")
        self.assertEqual((acc["friends"], acc["access"], acc["enemies"]), (["BBB", "EEE"], ["CCC"], ["DDD"]))
        self.assertEqual(acc["tags"], ["AAA", "BBB", "CCC", "DDD", "EEE"])  # FFF is neutral
        self.assertEqual(self.world().land_access("EEE")["friends"], ["AAA"])  # a subject and its overlord
        self.assertEqual(self.world().land_access("BBB")["friends"], ["AAA"])  # faction members
        self.assertEqual(self.world().land_access("AAA", through=["FFF"])["through"], ["FFF"])
        # access is one way: CCC lets AAA in, AAA does not let CCC in
        self.assertEqual(self.world().land_access("CCC")["tags"], ["CCC"])

    def test_route_goes_round_neutral_land(self):
        w = self.world()
        r = w.land_route(1, 3)
        self.assertEqual(r["path"], [1, 4, 5, 3])
        self.assertEqual(r["neutral"], {"FFF": [2]})  # the short way, and whose land it needs
        self.assertEqual(w.land_route(1, 3, any_land=True)["path"], [1, 2, 3])
        self.assertEqual(w.land_route(1, 3, through=["FFF"])["path"], [1, 2, 3])
        self.assertEqual(w.land_route(1, 3, enemies=["FFF"])["neutral"], {})

    def test_no_route_names_the_neutral(self):
        w = self.world("EEE")  # EEE fights nobody: DDD and FFF are both closed
        self.assertIsNone(w.land_route(1, 3, "EEE"))
        self.assertEqual(w.land_route_blockers(1, 3, "EEE"), {"FFF": [2], "DDD": [3]})

    def test_route_command(self):
        w = self.world()
        orig = facts.world
        facts.world = lambda args, *a, **k: w
        try:
            out, err = io.StringIO(), io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                facts.main(["route", "1", "3", "--game", self.dir, "--tag", "AAA"])
            text = out.getvalue()
            self.assertIn("1(plai,AAA) -> 4(plai,DDD) -> 5(plai,DDD) -> 3(plai,DDD)", text)
            self.assertIn("kept to land AAA may enter: own AAA; friends BBB EEE; access CCC; enemies DDD", text)
            self.assertIn("the shorter open path crosses neutral FFF (2): not used", text)
            w.tag = "EEE"
            out = io.StringIO()
            with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                code = facts.main(["route", "1", "3", "--game", self.dir, "--tag", "EEE"])
            self.assertTrue(code)
            self.assertIn("the open path crosses neutral", out.getvalue() + err.getvalue())
        finally:
            facts.world = orig


GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class ItalyRoutes(unittest.TestCase):
    """Italy in 1936 fights Ethiopia only: British Kenya and Aussa are closed land."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.w = lint_plan.world_of(cls.game, {"tag": "ITA", "date": "1936.1.1"})

    def test_moyale_without_kenya(self):
        r = self.w.land_route(11014, 13247, "ITA")
        self.assertNotIn(8133, r["path"])
        self.assertEqual({self.w.owner_of_province(p) for p in r["path"]}, {"ITA", "ETH"})
        self.assertEqual(r["neutral"], {"ENG": [8133]})

    def test_dessie_without_aussa(self):
        r = self.w.land_route(2072, 5097, "ITA")
        self.assertFalse({13236, 5041} & set(r["path"]))
        self.assertEqual(self.w.land_route(2072, 5097, "ITA", any_land=True)["path"], [2072, 13236, 5041, 5097])

    def test_lint_flags_a_leg_through_neutral_land(self):
        import copy
        import json
        with open(os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", "italy-ethiopia-1936.plan.json")) as fh:
            plan = json.load(fh)
        plan["_meta"] = {"version": 1, "source": "strategist"}
        rule = lambda p: [f for f in lint_plan.lint(p, self.game)["findings"]  # noqa: E731
                          if f["rule"] == "conquest.neutral_land"]
        # the hand plan sends two legs over the Afar road, which is Aussa's land at the start
        hit = rule(plan)
        self.assertEqual([(f["severity"], f["where"]) for f in hit],
                         [("error", "vp_targets[7] 5097"), ("error", "vp_targets[8] 5010")])
        self.assertIn("crosses AFA's land (13236 5041)", hit[0]["message"])
        # a predicate that waits for Aussa to be gone turns them into warnings to answer
        later = copy.deepcopy(plan)
        later.setdefault("checkpoints", []).append({"id": "aussa_gone", "by": "1936.6.1", "predicate": "not_exists:AFA"})
        self.assertEqual([f["severity"] for f in rule(later)], ["warning", "warning"])
        # a plan that fights Aussa may cross it
        war = copy.deepcopy(plan)
        war["war"]["enemies"] = list(war["war"].get("enemies") or []) + ["AFA"]
        self.assertEqual(rule(war), [])

if __name__ == "__main__":
    unittest.main()
