"""scripts/hoi4facts/force.py and lint_force.py: the force the plan's wars need, from the plan's own wars and the
game files, and the plan's force table, training, production and air plan held to it. Reads the installed game;
skipped without it."""

import copy
import json
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import force, lint_plan  # noqa: E402

PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")
GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))


def load(name):
    with open(os.path.join(PLANS, name), encoding="utf-8") as fh:
        return copy.deepcopy(json.load(fh))


def germany_at_war():
    """The Germany hand plan as a strategist plan that declares on Poland on 1939.9.1 and trains one infantry
    template: the shape of the plan that reached the AI's division count with no armour for its war."""
    plan = load("germany-fair-1936-1938.plan.json")
    plan["_meta"] = {"version": 1, "source": "strategist"}
    plan["goal"] = {"summary": "test", "deadline": "1939.10.1"}
    plan["_validator"]["stop_date"] = "1939.9.1"
    plan.pop("stop_on_date", None)
    plan["diplomacy"] = [{"id": "pol", "action": "declare", "target": "POL", "when": "date >= 1939.9.1"}]
    plan["training"] = [{"template": "Infanterie-Division", "queue": 8, "batch": 2, "cap": 125}]
    plan["invasions"] = [{"id": "inv_eng", "target_state": 127, "from_state": 15, "when": "date >= 1939.9.1"}]
    return plan


POLISH_STATE = 10  # Warszawa (history/states)


class Constants(unittest.TestCase):
    def test_the_breakthrough_scales_with_axes_and_enemy_divisions(self):
        # germany-fair.md §7: 6-8 mobile divisions for the three Polish axes against 40 divisions
        self.assertEqual(max(force.MOBILE_PER_AXIS * 3, -(-force.MOBILE_PER_ENEMY * 40 // 1)), 6)
        # §10: 25+ against the Soviet Union's 138 at the start: 21 from the enemy share alone, armour half
        self.assertGreaterEqual(-(-force.MOBILE_PER_ENEMY * 138 // 1), 20)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Classification(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)

    def test_units_by_category(self):
        g = self.game
        self.assertIn("armour", force.unit_classes(g, "medium_armor"))
        self.assertIn("mobile", force.unit_classes(g, "motorized"))
        self.assertIn("line", force.unit_classes(g, "infantry"))
        self.assertIn("anti_tank", force.unit_classes(g, "anti_tank"))
        self.assertIn("support", force.unit_classes(g, "engineer"))
        self.assertEqual(force.template_profile(g, ["medium_armor"] * 5 + ["motorized"] * 3)["kind"], "armour")
        self.assertEqual(force.template_profile(g, ["motorized"] * 6 + ["engineer"])["kind"], "mobile")
        self.assertEqual(force.template_profile(g, ["infantry"] * 9 + ["artillery_brigade"] * 2)["kind"], "line")

    def test_planes_and_ships_by_equipment_type(self):
        g = self.game
        self.assertEqual(force.plane_kind(g, "small_plane_airframe_1"), "fighter")
        self.assertEqual(force.plane_kind(g, "small_plane_cas_airframe_1"), "cas")
        self.assertEqual(force.plane_kind(g, "small_plane_naval_bomber_airframe_1"), "naval_bomber")
        self.assertEqual(force.plane_kind(g, "medium_plane_airframe_1"), "tactical_bomber")
        self.assertIsNone(force.plane_kind(g, "infantry_equipment_1"))
        self.assertEqual(force.ship_class(g, "ship_hull_light_1"), "screen_ship")
        self.assertEqual(force.ship_class(g, "ship_hull_submarine_1"), "submarine")


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class ObservedArmy(unittest.TestCase):
    """A plan continued from a save: force_design.observed replaces the army forecast from the save's date."""

    @classmethod
    def setUpClass(cls):
        game = lint_plan.load_game(GAME_DIR)
        plan = germany_at_war()
        plan["force_design"] = {
            "wars": [{"id": "war_pol", "enemies": ["POL"], "kind": "offensive", "axes": 1}],
            "land": [{"template": "Panzer-Division", "role": "breakthrough", "count": 30, "ready": "1939.9.1",
                      "war": "war_pol"}],
            "observed": {"date": "1939.8.22", "divisions": {"Infanterie-Division": 150, "Panzer-Division": 30},
                         "source": "test save"}}
        cls.lt = lint_plan.Lint(plan, game, lint_plan.FRONT_K, None, None).run()

    def test_the_save_replaces_the_forecast(self):
        from hoi4facts import conditions
        tl = conditions.Timeline(self.lt)
        day, divs = tl.observed_army()
        self.assertEqual(divs["Panzer-Division"], 30)
        fc = tl.forecast()
        i = (day - tl.start) // 7 + 1
        self.assertEqual(round(fc["by_template"][i].get("Panzer-Division", 0)), 30)
        ready = [f for f in self.lt.findings if f["rule"] == "force.readiness" and "Panzer-Division" in f["where"]]
        self.assertFalse(ready, ready)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class ObservedEconomy(unittest.TestCase):
    """A plan continued from a save: force_design.observed.economy and army_xp anchor the economy and army
    experience forecasts on the save's date; the AI is compared from that date; a fielded template was paid."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)

    def run_plan(self, observed):
        plan = germany_at_war()
        plan["force_design"] = {"wars": [{"id": "war_pol", "enemies": ["POL"], "kind": "offensive", "axes": 1}],
                                "observed": observed}
        plan["templates"] = [{"id": "t_sich", "name": "Sicherungs-Division", "regiments": [["infantry"] * 4],
                              "when": "date >= 1938.10.1", "role": "line"}]
        plan.setdefault("xp_budget", {})["army"] = [
            {"id": "x_sich", "purpose": "template", "ref": "template:t_sich", "cost": 60,
             "when": "date >= 1938.10.1"}]
        return lint_plan.Lint(plan, self.game, lint_plan.FRONT_K, None, None).run()

    def test_the_save_economy_replaces_the_forecast_and_the_past_is_not_judged(self):
        from hoi4facts import economy
        lt = self.run_plan({"date": "1939.8.22", "divisions": {"Infanterie-Division": 100},
                            "economy": {"civilian": 120, "military": 110}})
        self.assertEqual(economy.observed_economy(lt)[1:], (120, 110))
        self.assertGreaterEqual(lt.military_on(lint_plan.derive.day("1939.9.1")), 110)
        past = [f for f in lt.findings if f["rule"] in ("force.dominance", "plan.ai_baseline")
                and any(y in f["message"] for y in ("1936.", "1937.", "1938."))]
        self.assertFalse(past, past)

    def test_a_template_the_save_fields_was_paid(self):
        unpaid = lambda lt: [f for f in lt.findings if f["rule"] == "templates.xp_cost" and "x_sich" in f["message"]]
        lt = self.run_plan({"date": "1939.8.22", "divisions": {"Sicherungs-Division": 10}, "army_xp": 0})
        self.assertFalse(unpaid(lt))
        from hoi4facts import conditions, lint_firepower
        pool = lint_firepower.xp_pool(lt, conditions.Timeline(lt), [], lint_plan.derive.day("1939.8.22"))
        self.assertIn("x_sich", pool["bought"])
        self.assertLess(pool["pool"], 1.0, "the save's experience replaces the pool on its date")


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Germany(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.lt = lint_plan.Lint(germany_at_war(), cls.game, lint_plan.FRONT_K, None, None).run()
        plan = germany_at_war()
        plan["templates"] = [
            {"id": "t_pz", "name": "Panzer 38", "regiments": [["medium_armor"] * 4, ["motorized"] * 2],
             "support": ["engineer", "recon"], "when": "date >= 1939.10.1", "role": "armour"}]
        plan["training"].append({"template": "Panzer-Division", "queue": 2, "cap": 12})
        plan["force_design"] = {
            "wars": [{"id": "war_pol", "enemies": ["POL"], "kind": "offensive", "axes": 3,
                      "tasks": ["break the line on three axes"]}],
            "land": [{"template": "Panzer-Division", "role": "breakthrough", "count": 40, "ready": "1939.6.1",
                      "war": "war_pol"},
                     {"template": "Panzer 38", "role": "breakthrough", "count": 3, "ready": "1939.9.1",
                      "war": "war_pol"}],
            "air": [{"kind": "fighter", "count": 5000, "ready": "1939.9.1", "war": "war_pol"},
                    {"kind": "fighter", "equipment": "small_plane_airframe_3", "count": 100, "ready": "1939.9.1",
                     "war": "war_pol"}]}
        # a wing based in Poland on the day of the declaration
        plan["air_plan"] = [{"id": "ap_pol", "airframe": "small_plane_airframe_1", "mission": "air_superiority",
                             "region": 1, "base_state": POLISH_STATE, "when": "date >= 1939.9.1"}]
        cls.tabled = lint_plan.lint(plan, cls.game)

    def rules(self, findings, rule, severity="error"):
        return [f for f in findings if f["rule"] == rule and f["severity"] == severity]

    def test_the_war_is_derived_from_the_plan(self):
        wars = self.lt.facts["force"]["wars"]
        self.assertEqual([(w["id"], w["kind"], w["start_date"]) for w in wars],
                         [("war_pol", "offensive", "1939.09.01")])
        need = self.lt.facts["force"]["needs"][0]
        self.assertEqual(need["front"]["tags"], ["POL"])
        self.assertEqual(need["front"]["divisions"], 40)  # Poland's start OOB
        row = next(r for r in need["rows"] if r["role"] == "breakthrough")
        self.assertTrue(row["required"], row)  # 40 divisions fill one axis' width twice over

    def test_one_infantry_template_fails(self):
        f = self.lt.findings
        self.assertTrue(self.rules(f, "force.required"), f)
        b = self.rules(f, "force.breakthrough")
        self.assertTrue(b)
        self.assertIn("Panzer-Division", b[0]["message"])
        s = self.rules(f, "force.single_template")
        self.assertTrue(s)
        self.assertIn("Infanterie-Division", s[0]["message"])
        # the medium tank line of the hand plan: no template uses medium_armor
        self.assertTrue(any("medium_tank_chassis_1" in x["message"] for x in self.rules(f, "force.unused_line")))

    def test_the_landing_needs_the_air_and_the_sea(self):
        inv = self.rules(self.lt.findings, "force.invasion")
        self.assertTrue(inv)
        self.assertIn("ENG", inv[0]["message"])
        self.assertIn("naval strike", inv[0]["message"])

    def test_the_division_count_is_no_floor(self):
        self.assertFalse([x for x in self.lt.findings if x["rule"] == "plan.ai_baseline"
                          and x["where"] == "training" and x["severity"] != "info"])

    def test_the_table_is_held_to_the_forecast(self):
        f = self.tabled["findings"]
        self.assertFalse(self.rules(f, "force.required"), f)
        ready = self.rules(f, "force.readiness")
        self.assertTrue(any("Panzer-Division" in x["where"] for x in ready), ready)
        late = self.rules(f, "force.template_late")
        self.assertTrue(any("Panzer 38" in x["where"] for x in late), late)
        # the medium tank line now has a template
        self.assertFalse(any("medium_tank_chassis_1" in x["message"] for x in self.rules(f, "force.unused_line")))
        air = self.rules(f, "force.air")
        self.assertTrue(any(x["where"] == "force_design.air[0]" for x in air), air)
        self.assertIn("force_design.budget", [x["where"] for x in self.rules(f, "force.budget")])

    def test_fielded_divisions_are_not_late(self):
        # a start template the army already fields in the row's number is not late, however long new
        # divisions train; the same row asking for more than are fielded still is
        plan = germany_at_war()
        plan["force_design"] = {"land": [
            {"template": "Infanterie-Division", "role": "line", "count": 10, "ready": "1936.2.1"},
            {"template": "Infanterie-Division", "role": "line", "count": 400, "ready": "1936.2.2"}]}
        late = self.rules(lint_plan.lint(plan, self.game)["findings"], "force.template_late")
        self.assertFalse([x for x in late if x["where"] == "force_design.land[0] Infanterie-Division"], late)
        self.assertTrue([x for x in late if x["where"] == "force_design.land[1] Infanterie-Division"], late)

    def test_the_air_force_is_an_arm(self):
        f = self.lt.findings
        use = self.rules(f, "force.air_use")
        self.assertTrue(any("air_superiority" in x["message"] for x in use), use)
        # the wings the lines fill need a deploy entry
        self.assertTrue(self.rules(f, "force.air_deploy"))
        need = {r["role"]: r for r in self.lt.facts["force"]["needs"][0]["rows"]}
        self.assertEqual(need["fighters"]["want"], 243)  # 1.5 x Poland's 162 start fighters
        self.assertTrue(need["fighters"]["required"])
        self.assertTrue(need["cas"]["required"])  # the fighters are paid: CAS over the three axes
        g = self.tabled["findings"]
        self.assertTrue(any("belongs to POL" in x["message"] for x in self.rules(g, "force.air_bases")))
        self.assertTrue(any(x["where"] == "force_design.air[1]" for x in self.rules(g, "force.air_research")))
        self.assertFalse(self.rules(g, "force.air_use") and
                         any("air_superiority by then" in x["message"] for x in self.rules(g, "force.air_use")))

    def test_unit_cost_counts_the_modules(self):
        cost, how = force.unit_cost(self.lt, "small_plane_airframe_1")
        row = lint_plan.derive.equipment_rows(self.game)["small_plane_airframe_1"]["cost"]
        self.assertGreater(cost, row)  # the airframe alone is a fraction of a fighter
        self.assertIn("start variant", how)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class EastAfrica(unittest.TestCase):
    """Ethiopia defends on mountains with what it can afford; Italy's war has no defended line to break."""

    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.eth = lint_plan.lint(load("ethiopia-italy-1936.plan.json"), cls.game)
        cls.ita = lint_plan.lint(load("italy-ethiopia-1936.plan.json"), cls.game)

    def test_ethiopia_says_why_it_has_no_air_arm(self):
        f = [x for x in self.eth["findings"] if x["rule"] == "force.air_none"]
        self.assertTrue(f)
        self.assertEqual(f[0]["severity"], "warning")  # a hand plan

    def test_ethiopia(self):
        need = self.eth["facts"]["force"]["needs"][0]
        self.assertEqual(need["kind"], "defensive")
        roles = {r["role"]: r for r in need["rows"]}
        self.assertNotIn("breakthrough", roles)
        self.assertFalse(roles["fighters"]["required"])  # no air contest it can pay for
        self.assertIn("anti_air", roles)
        self.assertIn("mountain", roles)
        self.assertFalse([x for x in self.eth["findings"] if x["rule"] in ("force.breakthrough", "force.air")])

    def test_italy(self):
        need = self.ita["facts"]["force"]["needs"][0]
        self.assertEqual(need["kind"], "offensive")
        row = next(r for r in need["rows"] if r["role"] == "breakthrough")
        self.assertFalse(row["required"], row)  # 21 Ethiopian divisions hold no line on the axes
        self.assertFalse([x for x in self.ita["findings"] if x["rule"] == "force.breakthrough"])
        # a hand plan: the new force rules report as warnings
        self.assertFalse([x for x in self.ita["findings"] if x["rule"].startswith("force.")
                          and x["severity"] == "error"])


if __name__ == "__main__":
    unittest.main()
