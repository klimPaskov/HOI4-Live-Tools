"""scripts/hoi4facts/navy.py and lint_force_navy.py: the navy as an arm, sized from the plan's sea operations and the
start fleets in the game files. Reads the installed game; skipped without it."""

import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_plan, navy  # noqa: E402

PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")
GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))
KENT = 127        # South-east England (history/states), naval bases at Dover and Portsmouth
GIBRALTAR = 118


def germany_against_britain():
    """The Germany hand plan as a strategist plan at war with Poland and Britain from 1939.9.1 that lands in Kent on
    1940.9.1 and takes it by 1940.12.1: a landing against a fleet several times its own."""
    with open(os.path.join(PLANS, "germany-fair-1936-1938.plan.json"), encoding="utf-8") as fh:
        plan = copy.deepcopy(json.load(fh))
    plan["_meta"] = {"version": 1, "source": "strategist"}
    plan["goal"] = {"summary": "test", "deadline": "1941.1.1"}
    plan["_validator"]["stop_date"] = "1940.9.1"
    plan.pop("stop_on_date", None)
    plan["diplomacy"] = [{"id": "pol", "action": "declare", "target": "POL", "when": "date >= 1939.9.1",
                          "joiners_ok": ["ENG", "FRA"]}]
    plan["invasions"] = [{"id": "inv_kent", "target_state": KENT, "from_state": 15, "armies": ["Armee West"],
                          "when": "date >= 1940.9.1"}]
    plan["checkpoints"] = list(plan.get("checkpoints") or []) + [
        {"id": "cp_kent", "by": "1940.12.1", "must": [f"controls_state:{KENT}"]},
        {"id": "cp_gib", "by": "1941.1.1", "must": [f"controls_state:{GIBRALTAR}"]}]
    return plan


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class GameFacts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)

    def test_roles_by_unit(self):
        g = self.game
        self.assertEqual(navy.role_of(g, "ship_hull_cruiser_1", "heavy_cruiser"), "capital")
        self.assertEqual(navy.role_of(g, "ship_hull_cruiser_1", "light_cruiser"), "screen")
        self.assertEqual(navy.role_of(g, "ship_hull_cruiser_1"), "screen")  # the hull alone
        self.assertEqual(navy.role_of(g, "ship_hull_carrier_1"), "carrier")
        self.assertEqual(navy.role_of(g, "ship_hull_submarine_1"), "submarine")

    def test_carrier_planes_and_sea_zones(self):
        g = self.game
        self.assertTrue(navy.carrier_plane(g, "cv_small_plane_naval_bomber_airframe_1"))
        self.assertFalse(navy.carrier_plane(g, "small_plane_naval_bomber_airframe_1"))
        mins = navy.seazone_minimum(g)
        self.assertEqual(mins.get("water_deep_ocean"), 250)
        self.assertEqual(mins.get("water_shallow_sea"), 100)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Britain(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.bare = lint_plan.lint(germany_against_britain(), cls.game)
        plan = germany_against_britain()
        plan["force_design"] = {
            "wars": [{"id": "war_pol", "enemies": ["POL"], "kind": "offensive", "axes": 3}],
            "sea": [{"id": "sea_lion", "kind": "invasion", "invasion": "inv_kent", "war": "war_pol",
                     "means": ["air"], "divisions": 30},
                    {"id": "home_waters", "kind": "sea_control", "by": "1940.6.1", "war": "war_pol",
                     "regions": ["North Sea"], "means": ["surface"], "fallback": "patrols on the North Sea"}],
            "navy": [{"kind": "carrier", "count": 2, "ready": "1940.6.1", "war": "war_pol"}]}
        plan["production"] = list(plan.get("production") or []) + [
            {"equipment": "ship_hull_carrier_1", "factories": 5, "from": "1936.1.1"}]
        cls.planned = lint_plan.lint(plan, cls.game)

    def rules(self, out, rule, severity="error"):
        return [f for f in out["findings"] if f["rule"] == rule and f["severity"] == severity]

    def test_the_enemy_fleet_comes_from_the_history_files(self):
        op = next(o for o in self.bare["facts"]["force"]["navy"]["operations"] if o["id"] == "inv_kent")
        self.assertEqual(op["kind"], "invasion")
        self.assertIn("ENG", op["enemies"])
        ours = self.bare["facts"]["force"]["navy"]["ours"]
        self.assertGreater(op["enemy_fleet"]["surface_ic"], 4 * sum(ours["hull_ic"].get(r, 0)
                                                                    for r in ("capital", "screen", "carrier")))
        self.assertTrue(op["regions"])  # the route from Normandy crosses the Channel

    def test_the_landing_needs_the_sea(self):
        f = self.rules(self.bare, "force.sea_control")
        self.assertTrue(f)
        msg = f[0]["message"]
        for means in ("surface:", "air:", "carrier:"):
            self.assertIn(means, msg)  # what each means takes, for the plan to provide
        self.assertIn("force.required", [x["rule"] for x in self.bare["findings"]])  # no force_design at all

    def test_an_island_the_checkpoints_take_needs_a_landing(self):
        req = self.rules(self.bare, "force.sea_required")
        # Kent has its landing; Gibraltar lies on the mainland: neither is reported as land across the sea
        self.assertFalse(any(f"state {KENT}" in x["message"] for x in req), req)
        self.assertFalse(any(f"state {GIBRALTAR}" in x["message"] for x in req), req)

    def test_declared_operations(self):
        ops = {o["id"]: o for o in self.planned["facts"]["force"]["navy"]["operations"]}
        self.assertEqual(ops["sea_lion"]["divisions"], 30)
        lift = next(r for r in ops["sea_lion"]["needs"] if r["need"] == "landing")
        self.assertFalse(lift["ok"])  # 30 divisions in one landing are more than the techs allow
        self.assertTrue(self.rules(self.planned, "force.sea_lift"))
        # the North Sea has no patrol in the hand plan
        self.assertTrue(any("home_waters" in x["where"] for x in self.rules(self.planned, "force.sea_patrol")))
        # carriers with no carrier planes
        self.assertTrue(self.rules(self.planned, "force.carrier_air"))
        # an unexecutable lever with a stated fallback is not an error for that operation
        tools = [x["where"] for x in self.rules(self.planned, "force.navy_tools")]
        self.assertFalse(any("home_waters" in w for w in tools), tools)
        self.assertTrue(any("sea_lion" in w for w in tools), tools)

    def test_a_sea_won_from_the_air_needs_wings_over_every_region(self):
        cover = [x for x in self.rules(self.planned, "force.sea_air_cover") if "sea_lion" in x["where"]]
        self.assertTrue(cover)  # the hand plan flies no naval strike over the route

    def test_tools_are_ordered_by_reliance(self):
        tools = self.planned["facts"]["force"]["navy"]["tools"]
        self.assertTrue(tools)
        statuses = [t["status"] for t in tools]
        self.assertEqual(statuses, sorted(statuses, key=lambda s: s != "not executable"))


if __name__ == "__main__":
    unittest.main()
