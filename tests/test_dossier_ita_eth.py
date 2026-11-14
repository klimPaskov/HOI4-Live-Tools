"""Golden dossier test: Italy 1 January 1936 against the installed game (skipped without it).

Facts from skills/hoi4-campaign-playbook/references/italo-ethiopian-war.md section 0, with the DLC
list of skills/hoi4-campaign-playbook/plans/italy-ethiopia-1936.plan.json `_validator.dlcs`.
"""
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import dossier, mapgraph  # noqa: E402

GAME = os.environ.get("HOI4_DIR", mapgraph.DEFAULT_GAME_DIR)
PLAN = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", "italy-ethiopia-1936.plan.json")


@unittest.skipUnless(os.path.isfile(os.path.join(GAME, "history", "countries", "ITA - Italy.txt")),
                     "HOI4 game folder not found")
class ItalyEthiopia1936(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(PLAN, encoding="utf-8") as fh:
            dlcs = json.load(fh)["_validator"]["dlcs"]
        cls.d = dossier.build("ITA", "1936.1.1", GAME, [], dlcs)
        cls.md = dossier.render_md(cls.d)

    def test_war_at_start(self):
        wars = self.d["hooks"]["own"]["wars"]
        self.assertIn(("ITA", "ETH", "annex_everything"), [(w["attacker"], w["target"], w["type"]) for w in wars])
        self.assertTrue(wars[0]["src"].startswith("history/countries/ITA - Italy.txt:"))
        self.assertEqual([e["tag"] for e in self.d["enemies"]], ["ETH"])

    def test_ethiopia_vp_and_surrender(self):
        eth = self.d["enemies"][0]
        vp = eth["vp"]
        self.assertEqual(vp["total"], 32)
        self.assertEqual(vp["base_limit"]["v"], 0.8)
        self.assertTrue(vp["base_limit"]["src"].startswith("common/defines/00_defines.lua:"))
        mod = {m["idea"]: m for m in vp["modifiers"]}["ETH_demoralized_ras_idea"]
        self.assertEqual(mod["value"], -0.15)
        self.assertEqual([(t["limit"], t["vp_needed"]) for t in vp["thresholds"]], [(0.8, 26), (0.65, 21)])
        self.assertIn("limit 0.8 -> 26 VP", self.md)
        # Ethiopia starts at war support 30%: the low-war-support modifier brings the limit to 0.68
        # there and to 0.5 (16 VP) at zero
        ws = vp["war_support"]
        self.assertEqual((ws["start"], ws["full_malus"]), (0.3, -0.3))
        self.assertEqual((ws["at_start"]["limit"], ws["at_start"]["vp_needed"]), (0.68, 22))
        self.assertEqual((ws["at_zero"]["limit"], ws["at_zero"]["vp_needed"]), (0.5, 16))
        self.assertIn("war support 0% (the floor): limit 0.5 -> 16 VP", self.md)

    def test_four_east_african_hubs(self):
        th = self.d["theatres"][0]
        self.assertEqual(sorted(h["province"] for h in th["hubs"]), [5010, 7980, 8124, 13250])

    def test_conquer_ethiopia_170_days(self):
        v = {x["name"]: x for x in self.d["country"]["variables"]}["ITA_missiolinis_conquer_ethiopia_days"]
        self.assertEqual(v["v"], 170)
        self.assertTrue(v["src"].startswith("history/countries/ITA - Italy.txt:"))
        deadlines = {m["mission"]: m for m in self.d["hooks"]["own"]["deadlines"]}
        self.assertEqual(deadlines["ITA_missiolini_conquer_ethiopia"]["timeout"], 170)

    def test_eth_defeat_italy_states(self):
        goals = {g["id"]: g for g in self.d["hooks"]["enemies"]["ETH"]["goals"]}
        self.assertEqual(goals["ETH_defeat_italy"]["controls"]["states"], [550, 559, 844])
        self.assertTrue(goals["ETH_defeat_italy"]["src"].startswith("common/decisions/ETH.txt:"))

    def test_italy_loads_the_bba_tree(self):
        f = self.d["focus"]
        self.assertEqual(f["id"], "italian_focus")
        ids = {x["id"] for x in f["focuses"]}
        self.assertIn("ITA_ethiopian_war_logistics_bba", ids)
        self.assertGreater(sum(1 for i in ids if i.endswith("_bba")), 5)
        days = {x["id"]: x["days"] for x in f["focuses"]}
        self.assertEqual(days["ITA_ethiopian_war_logistics_bba"], 35)

    def test_nsb_oob(self):
        self.assertIn("No Step Back", self.d["run"]["dlcs"])
        files = [o["name"] for o in self.d["oob"]["files"]]
        self.assertIn("ITA_1936_nsb", files)
        self.assertNotIn("ITA_1936", files)
        self.assertEqual(len(self.d["oob"]["divisions"]), 46)

    def test_victory_rule_needs_shewa(self):
        rows = {o["flag"]: o for o in self.d["hooks"]["own"]["outcomes"]}
        self.assertEqual(rows["ITA_victory_in_ethiopia_flag"]["requires_control"]["states"], [271])

    def test_fair_hides_private(self):
        eth = self.d["enemies"][0]
        self.assertEqual(eth["oob"]["aggregate"]["fair"], "aggregate")
        self.assertNotIn("private", eth["oob"])
        self.assertNotIn("ai_strategy", eth)

    def test_calc_spot_checks(self):
        from hoi4facts import calc
        from hoi4facts.game import Game
        from hoi4facts.world import World
        with open(PLAN, encoding="utf-8") as fh:
            dlcs = json.load(fh)["_validator"]["dlcs"]
        w = World(Game(GAME), "ITA", "1936.1.1", dlcs)
        self.assertEqual(calc.consumer_goods(w, "ITA")[0], 0.25)  # partial_economic_mobilisation
        eth_civ = sum(w.states()[s]["buildings"].get("industrial_complex", 0) for s in w.owned_states("ETH"))
        self.assertEqual(eth_civ, 2)  # the Ethiopia plan's "2 civilian factories"
        t = w.oob("ITA")["templates"]["Divisione di Fanteria"]
        r = calc.equipment(w, t["regiments"], t["support"], divisions=2, year=1936)
        rows = {x["equipment"]: x for x in r["rows"]}
        self.assertEqual(rows["artillery_equipment"]["per_division"], 12)
        self.assertGreaterEqual(rows["infantry_equipment"]["per_division"], 600)
        b = calc.build_order(w, ["civ"], 15, 0, 0.0, "1936.1.1", infra=0)
        self.assertEqual(b["rows"][0]["date"], "1936.5.25")  # 10800 / (15 x 5) = 144 days

    def test_digest_size_and_write(self):
        size = len(self.md.encode("utf-8"))
        self.assertLess(size, dossier.HARD_CAP)
        for n in range(10):
            self.assertIn(f"\n## {n}. ", self.md)
        with tempfile.TemporaryDirectory() as tmp:
            r = dossier.write(self.d, os.path.join(tmp, "dossier"))
            with open(os.path.join(r["dir"], "manifest.json"), encoding="utf-8") as fh:
                man = json.load(fh)
            self.assertEqual(man["tag"], "ITA")
            self.assertEqual(man["content_hash"], self.d["_meta"]["content_hash"])


if __name__ == "__main__":
    unittest.main()
