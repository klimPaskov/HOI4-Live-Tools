"""The country experience pool is forecast from its own defines, and convoys are sized against the
plan's sea supply and sea trade.

The same define names exist once per branch (NMilitary, NAir, NNavy) and next to them stand the
UNIT_EXPERIENCE_* defines of a unit's veterancy; the forecast reads the pool defines by full key, mods
included. The convoy rule multiplies the supply use of the divisions that stand overseas by
SUPPLY_CONVOY_FACTOR and adds the planned imports. Runs on a copy of the small dossier fixture with a
mod-style defines file; the Italy checks read the game files.
"""
import contextlib
import copy
import io
import json
import os
import shutil
import sys
import tempfile
import unittest
from unittest import mock

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import calc, derive, dossier, lint_plan  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.world import World  # noqa: E402

FIXTURE = os.path.join(ROOT, "tests", "fixtures", "dossier", "game")
GAME_DIR = os.environ.get("HOI4_GAME_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))
ITA = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", "italy-ethiopia-1936.plan.json")

# One name, four meanings: the unit's veterancy and the three country pools. The values differ from the
# game's so a hardcoded number cannot pass, and the last line is how a mod overrides one.
DEFINES = """NDefines = {
NMilitary = {
	MAX_ARMY_EXPERIENCE = 300,
	UNIT_EXPERIENCE_PER_COMBAT_HOUR = 0.0001,
	FIELD_EXPERIENCE_SCALE = 0.002,
	FIELD_EXPERIENCE_MAX_PER_DAY = 3,
	TRAINING_MAX_DAILY_COUNTRY_EXP = 0.5,
	MIN_SUPPLY_CONSUMPTION = 0.05,
},
NAir = {
	FIELD_EXPERIENCE_SCALE = 0.0004,
	FIELD_EXPERIENCE_MAX_PER_DAY = 7,
},
NNavy = {
	UNIT_EXPERIENCE_PER_COMBAT_HOUR = 10,
	FIELD_EXPERIENCE_SCALE = 0.075,
	FIELD_EXPERIENCE_MAX_PER_DAY = 50,
	TRAINING_MAX_DAILY_COUNTRY_EXP = 1,
},
NCountry = {
	SUPPLY_CONVOY_FACTOR = 0.25,
	CONVOY_RANGE_FACTOR = 1,
},
}
NDefines.NMilitary.FIELD_EXPERIENCE_MAX_PER_DAY = 2
NDefines.NCountry.SUPPLY_CONVOY_FACTOR = 0.5
"""


def modded_fixture():
    """A copy of the fixture game with the defines above, supply use on its sub-units and 3 convoys."""
    root = tempfile.mkdtemp(prefix="xpconvoys")
    game = os.path.join(root, "game")
    shutil.copytree(FIXTURE, game)
    with open(os.path.join(game, "common", "defines", "zzz_pools.lua"), "w", encoding="utf-8") as fh:
        fh.write(DEFINES)
    units = os.path.join(game, "common", "units", "infantry.txt")
    with open(units, encoding="utf-8") as fh:
        text = fh.read()
    text = text.replace("infantry = { group = infantry", "infantry = { supply_consumption = 0.5 group = infantry", 1)
    text = text.replace("artillery = { group = support", "artillery = { supply_consumption = 1 group = support", 1)
    with open(units, "w", encoding="utf-8") as fh:
        fh.write(text)
    with open(os.path.join(game, "history", "countries", "AAA - Alpha.txt"), "a", encoding="utf-8") as fh:
        fh.write("\nset_convoys = 3\n")
    return root, Game(game, cache_dir=os.path.join(root, "cache"))


class Fixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root, cls.game = modded_fixture()
        cls.world = World(cls.game, "AAA", "1936.1.1", (), "fair")

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.root, ignore_errors=True)


class ExperiencePool(Fixture):
    def test_each_pool_reads_its_own_defines(self):
        gain = self.world.experience("AAA")["gain"]
        army = gain["army"]
        self.assertEqual(army["combat_scale"]["key"], "NMilitary.FIELD_EXPERIENCE_SCALE")
        self.assertEqual(army["combat_scale"]["v"], 0.002)
        self.assertEqual(army["combat_max_per_day"]["v"], 2)  # the mod-style override, not 3
        self.assertIn("zzz_pools.lua", army["combat_max_per_day"]["src"])
        self.assertEqual(army["training_max_per_day"]["v"], 0.5)
        self.assertEqual(gain["air"]["combat_max_per_day"]["v"], 7)
        self.assertEqual(gain["navy"]["combat_scale"]["key"], "NNavy.FIELD_EXPERIENCE_SCALE")
        self.assertEqual(gain["navy"]["training_max_per_day"]["v"], 1)
        # the unit's veterancy define feeds no pool
        used = {f["key"] for rows in gain.values() for f in rows.values()}
        self.assertFalse([k for k in used if "UNIT_EXPERIENCE" in k])

    def test_forecast_floor_estimate_and_ceiling(self):
        r = calc.xp(self.world, "army", "1936.1.1", "1936.4.10", start_xp=0,
                    rewards=[("1936.2.1", 25, "focus")], war=[("1936.1.1", "1936.3.1")], combat_per_day=0.5,
                    costs=[100])
        war_days = derive.day("1936.3.1") - derive.day("1936.1.1")
        self.assertEqual(r["war_days"], war_days)
        self.assertEqual(r["end"]["floor"], 25)
        self.assertEqual(r["end"]["estimate"], 25 + 0.5 * war_days)
        self.assertEqual(r["end"]["ceiling"], 25 + 2 * war_days)  # 2 a day: the overridden army define
        hit = r["reach"][0]
        self.assertIsNone(hit["floor"])
        self.assertIsNone(hit["estimate"])  # 54.5 by the end of the war
        self.assertEqual(hit["ceiling"], "1936.2.8")  # 25 + 2 x 38 days = 101
        self.assertEqual(r["rows"][0], {"date": "1936.1.31", "floor": 0, "ceiling": 60, "estimate": 15})
        self.assertEqual(r["defines"]["combat_scale"]["key"], "NMilitary.FIELD_EXPERIENCE_SCALE")

    def test_forecast_stops_at_the_pool_cap_and_counts_training(self):
        r = calc.xp(self.world, "army", "1936.1.1", "1937.1.1", start_xp=0, war=[("1936.1.1", "1937.1.1")])
        self.assertEqual(r["end"]["ceiling"], 300)  # MAX_ARMY_EXPERIENCE of these files
        r = calc.xp(self.world, "army", "1936.1.1", "1936.3.1", start_xp=10, training_days=20)
        self.assertEqual(r["end"]["floor"], 10 + 20 * 0.5)
        self.assertNotIn("estimate", r["end"])
        # an assumed daily gain above the define is cut to it
        r = calc.xp(self.world, "navy", "1936.1.1", "1936.1.11", war=[("1936.1.1", "1936.1.11")], combat_per_day=80)
        self.assertEqual(r["combat_per_day"], 50)
        self.assertEqual(r["start"]["xp"], 5)  # navy_experience = 5 in the history
        with self.assertRaises(ValueError):
            calc.xp(self.world, "marines", "1936.1.1", "1936.2.1")

    def test_cli_and_dossier_name_the_pool_defines(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = calc.main(["xp", "--game", self.game.root, "--tag", "AAA", "--date", "1936.1.1", "--until",
                              "1936.3.1", "--war", "1936.1.1:1936.3.1", "--cost", "100", "--json"])
        self.assertEqual(code, 0)
        data = json.loads(out.getvalue())
        self.assertEqual(data["end"]["ceiling"], 2 * data["war_days"])
        self.assertEqual(data["reach"][0]["ceiling"], "1936.2.20")
        d = dossier.build_world(self.world)
        self.assertEqual(d["country"]["experience"]["gain"]["army"]["combat_max_per_day"]["v"], 2)
        line = [x for x in dossier.render_md(d).splitlines() if "army pool gain" in x]
        self.assertEqual(len(line), 1)
        self.assertIn("combat_scale 0.002 [NMilitary.FIELD_EXPERIENCE_SCALE", line[0])
        self.assertIn("UNIT_EXPERIENCE_* is a unit's veterancy", line[0])


class ConvoySizing(Fixture):
    """AAA's two "Line" divisions (2 infantry + artillery: 2 supply each) stand in state 1; the fixture has no
    map, so the test says which states are overseas."""

    def lint(self, plan, overseas=(1,)):
        plan = dict({"_validator": {"tag": "AAA", "start_date": "1936.1.1"}}, **plan)
        lt = lint_plan.Lint(plan, self.game)
        with mock.patch.object(derive, "overseas_states", return_value=set(overseas)):
            lt.convoy_size()
        return lt

    def test_sea_supply_and_trade_are_sized_from_the_defines(self):
        lt = self.lint({"auto_import": {"budget": 2}})
        c = lt.facts["convoys"]
        self.assertEqual(c["sea_supply"]["states"], [{"state": 1, "divisions": 2, "supply": 4.0}])
        self.assertEqual(c["sea_supply"]["convoys_per_supply"], 0.5)  # the mod's value, not the game's 0.25
        self.assertEqual(c["sea_supply"]["convoys"], 2.0)
        self.assertEqual(c["sea_trade"]["units"], 16)
        self.assertEqual(c["sea_trade"]["convoys"], 1.6)
        self.assertEqual((c["need"], c["start"], c["free"]), (4, 3, -1))
        errors = [f for f in lt.findings if f["rule"] == "supply.convoy_size"]
        self.assertEqual([f["severity"] for f in errors], ["error"])
        self.assertIn("needs about 4 convoys", errors[0]["message"])
        self.assertIn("starts with 3", errors[0]["message"])

    def test_enough_convoys_is_no_finding_but_the_sizing_is_reported(self):
        lt = self.lint({})
        self.assertEqual(lt.findings, [])
        self.assertEqual(lt.facts["convoys"]["need"], 2)
        self.assertEqual(lt.facts["convoys"]["free"], 1)

    def test_layout_count_trade_deals_and_the_stated_number(self):
        lt = self.lint({"army_layout": [{"id": "a", "state": 1, "templates": ["Line"], "count": 5}],
                        "trade": [{"id": "t1", "resource": "oil", "amount": 10}, {"id": "t2", "resource": "rubber"}],
                        "sea_transfer": {"convoys": 4}})
        c = lt.facts["convoys"]
        self.assertEqual(c["sea_supply"]["states"], [{"state": 1, "divisions": 5, "supply": 10.0}])
        self.assertEqual(c["sea_trade"]["units"], 10)
        self.assertEqual(c["sea_trade"]["not_counted"], ["trade deal t2 amount needed"])
        self.assertEqual(c["need"], 6)
        where = sorted(f["where"] for f in lt.findings if f["rule"] == "supply.convoy_size")
        self.assertEqual(where, ["production", "sea_transfer.convoys"])

    def test_nothing_overseas_and_no_imports_is_silent(self):
        lt = self.lint({}, overseas=())
        self.assertEqual(lt.findings, [])
        self.assertNotIn("convoys", lt.facts)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Italy(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        with open(ITA, encoding="utf-8") as fh:
            cls.plan = json.load(fh)
        cls.plan["_meta"] = {"version": 1, "source": "strategist"}

    def run_lint(self, plan):
        lt = lint_plan.Lint(plan, self.game)
        lt.convoy_size()
        lt.xp_unspent()
        return lt

    def test_army_pool_uses_the_country_define(self):
        lt = self.run_lint(copy.deepcopy(self.plan))
        xp = lt.facts["xp"]["army"]
        self.assertEqual(xp["define"], "NMilitary.FIELD_EXPERIENCE_MAX_PER_DAY")
        self.assertEqual(xp["combat_max_per_day"], 1.2)
        self.assertEqual(xp["combat_scale"], 0.0008)  # not the unit's 0.0001 an hour
        self.assertGreater(xp["ceiling"], xp["cheapest_land_doctrine"])

    def test_a_war_plan_with_no_army_xp_spend_is_flagged(self):
        plan = copy.deepcopy(self.plan)
        for key in ("doctrines", "templates", "designs", "xp_budget"):
            plan.pop(key, None)
        hit = [f for f in self.run_lint(plan).findings if f["rule"] == "xp.unspent"]
        self.assertEqual([f["severity"] for f in hit], ["warning"])
        self.assertIn("NMilitary.FIELD_EXPERIENCE_MAX_PER_DAY", hit[0]["message"])
        plan["doctrines"] = [{"id": "d1", "folder": "land", "grand_doctrine": "x", "xp": 100}]
        self.assertFalse([f for f in self.run_lint(plan).findings if f["rule"] == "xp.unspent"])

    def test_east_africa_and_libya_are_sized_against_the_start_convoys(self):
        lt = self.run_lint(copy.deepcopy(self.plan))
        c = lt.facts["convoys"]
        self.assertEqual(c["start"], 200)
        states = {r["state"]: r for r in c["sea_supply"]["states"]}
        self.assertTrue({550, 559, 844} <= set(states))  # Eritrea, Somaliland, Jubaland
        self.assertEqual(states[550]["divisions"], 18)
        self.assertEqual(c["sea_supply"]["convoys_per_supply"], 0.25)
        self.assertGreater(c["free"], 0)
        self.assertFalse([f for f in lt.findings if f["rule"] == "supply.convoy_size"])
        # the same theatre with next to no convoys is an error
        short = copy.deepcopy(self.plan)
        short["sea_transfer"] = {"convoys": 1}
        hit = [f for f in self.run_lint(short).findings if f["rule"] == "supply.convoy_size"]
        self.assertEqual([f["where"] for f in hit], ["sea_transfer.convoys"])


if __name__ == "__main__":
    unittest.main()
