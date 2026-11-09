"""Wave-1 fact readers: terrain width, units, weather, balance of power, occupation, ports and overseas theatres,
never-take decisions, experience, law gates, enemy technology level, calc width and build costs.

Fixture tests use tests/fixtures/dossier/game (AAA at war with BBB; AAA's non-core state 4 with compliance 25,
a balance of power AAA_balance active at start and AAA_council gained by a focus, a withdraw decision with a
white peace in an effect_tooltip, a regimental support and a special-forces battalion, a strategic region with
weather, an adjacency rule) and a synthetic province graph for the overseas and port-candidate logic. The golden
tests read the installed game and are skipped without it.
"""
import contextlib
import io
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import calc, dossier, facts, mapgraph, scenario, theatre  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.script import parse  # noqa: E402
from hoi4facts.world import World, gates, state_limit, xp_in  # noqa: E402

GAME = os.path.join(ROOT, "tests", "fixtures", "dossier", "game")
REAL = os.environ.get("HOI4_DIR", mapgraph.DEFAULT_GAME_DIR)
PLAN = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", "italy-ethiopia-1936.plan.json")


def world(tag="AAA", date="1936.1.1", dlcs=(), mode="fair"):
    return World(Game(GAME, cache_dir=tempfile.gettempdir()), tag, date, dlcs, mode)


def run_cli(argv):
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc = facts.main(argv + ["--game", GAME, "--dlc", "none", "--userdir", GAME])
    return rc, buf.getvalue()


def _edge(kind="border", rule=None):
    e = {"kind": kind, "border": 1, "river": 0, "river_crossing": False}
    if rule:
        e["rule"] = rule
    return e


def fake_graph():
    """State 1 (provinces 10, 11; port at 11) is an island; state 4 (40, 41; port at 41) borders BBB's state 2
    (20, 21) at 40-20; 21-30 links state 3. Sea zones 90-91-92 (91-92 under FIXTURE_CANAL) and the longer
    90-93-94-92; 11 touches 90, 40 and 41 touch 92."""
    land = {10: "plains", 11: "plains", 40: "mountain", 41: "plains", 20: "mountain", 21: "mountain", 30: "plains"}
    xy = {10: (0, 0), 11: (1, 0), 40: (10, 0), 41: (10, 1), 20: (11, 0), 21: (12, 0), 30: (13, 0),
          90: (2, 0), 91: (5, 0), 92: (9, 0), 93: (2, 5), 94: (9, 5)}
    provs = {}
    for p, (x, y) in xy.items():
        provs[p] = {"type": "land" if p in land else "sea", "terrain": land.get(p, "ocean"),
                    "coastal": p in (11, 40, 41), "xy": [x, y]}
    adj = {}

    def link(a, b, **kw):
        e = _edge(**kw)
        adj.setdefault(a, {})[b] = e
        adj.setdefault(b, {})[a] = e

    link(10, 11)
    link(40, 41)
    link(40, 20)
    link(20, 21)
    link(21, 30)
    link(11, 90)
    link(40, 92)
    link(41, 92)
    link(90, 91)
    link(91, 92, kind="strait", rule="FIXTURE_CANAL")
    link(90, 93)
    link(93, 94)
    link(94, 92)
    return {"provinces": provs, "adj": adj, "states": {}, "supply_hubs": {}, "railways": [], "regions": {}}


def world_with_graph(tag="AAA"):
    w = world(tag)
    w.game._cache["mapgraph"] = fake_graph()
    return w


class TerrainAndWidth(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_terrain_widths(self):
        t = self.w.terrains()
        self.assertEqual((t["plains"]["combat_width"], t["plains"]["combat_support_width"]), (70, 35))
        self.assertEqual(t["plains"]["width_src"], "common/terrain/00_terrain.txt:6")
        self.assertEqual(t["mountain"]["attack"], -0.5)
        self.assertTrue(t["ocean"]["is_water"])

    def test_battle_widths_and_fill(self):
        self.assertEqual(theatre.battle_widths(self.w.terrains()["mountain"]), {1: 50, 2: 75})
        stack = {"COMBAT_STACKING_START": {"v": 5}, "COMBAT_STACKING_EXTRA": {"v": 3},
                 "COMBAT_STACKING_PENALTY": {"v": -0.02}}
        self.assertEqual(theatre.fill(25, 50, 1, stack), (1.0, 2))
        self.assertEqual(theatre.fill(10, 70, 1, stack), (0.96, 7))  # 7 divisions, 2 above the stacking start
        self.assertEqual(theatre.fill(14, 105, 2, stack), (0.9333, 7))

    def test_width_target(self):
        r = theatre.width_target(self.w, {"mountain": 0.6, "plains": 0.4})
        self.assertEqual((r["target"], r["dominant"]), (25, "mountain"))
        self.assertEqual(r["battle_widths"]["plains"], {"1": 70, "2": 105})
        self.assertEqual(r["best_for_mix"][0]["width"], 10)
        self.assertEqual(theatre.width_target(self.w, {"plains": 1.0})["target"], 35)
        self.assertIsNone(theatre.width_target(self.w, {"ocean": 1.0})["target"])

    def test_terrain_mix(self):
        g = fake_graph()
        m = mapgraph.terrain_mix(g, [40, 41, 20, 21, 90, 999, 40])
        self.assertEqual(m["provinces"], 4)  # sea 90, unknown 999 and the repeated 40 are skipped
        self.assertEqual(m["counts"], {"mountain": 3, "plains": 1})
        self.assertEqual(m["shares"], {"mountain": 0.75, "plains": 0.25})


class Units(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_rows_and_unlocks(self):
        u = self.w.sub_units()
        self.assertEqual(u["field_guns"]["row"], "regimental support")
        self.assertEqual(u["field_guns"]["allowed_battalion_groups"], ["infantry", "mobile"])
        self.assertEqual(u["artillery"]["row"], "divisional support")
        self.assertEqual(u["infantry"]["row"], "battalion")
        self.assertTrue(u["mountaineers"]["special_forces"])
        self.assertEqual(u["mountaineers"]["terrain"], {"mountain": {"attack": 0.25}})
        self.assertEqual([x["tech"] for x in u["field_guns"]["unlocked_by"]], ["t_b"])

    def test_units_cli_unlocked_by(self):
        rc, out = run_cli(["units", "--unlocked-by", "1936.1.1", "--tag", "AAA", "--json"])
        self.assertEqual(rc, 0)
        rows = {r["id"]: r for r in json.loads(out)["rows"]}
        self.assertEqual(rows["mountaineers"]["unlocked_via"], ["t_a"])
        self.assertEqual(rows["light_armor"]["unlocked_via"], ["active = yes"])
        self.assertNotIn("field_guns", rows)  # t_b is BBB's, not AAA's
        rc, out = run_cli(["units", "--category", "regimental"])
        self.assertIn("field_guns", out)
        self.assertNotIn("mountaineers", out)

    def test_calc_width_and_template_checks(self):
        r = calc.equipment(self.w, {"infantry": 9}, ["artillery"], 1, 1936, ["field_guns"])
        self.assertEqual(r["width"], 18)
        self.assertEqual(r["width_fit"]["mountain"][1], {"fill": 0.72, "divisions": 2, "battle_width": 50})
        self.assertEqual(r["template_problems"], [])
        rows = {x["equipment"]: x for x in r["rows"]}
        self.assertEqual(rows["artillery_equipment"]["per_division"], 18)  # 12 support + 6 regimental
        problems, _ = calc.template_checks(self.w, {"infantry": 2}, ["field_guns"], ["artillery", "field_guns"])
        self.assertIn("field_guns is a regimental support (divisional = no): put it in the regimental row", problems)
        self.assertIn("artillery is not a regimental support (row divisional support)", problems)
        self.assertTrue(any("need 3 battalions" in p for p in problems))
        problems, _ = calc.template_checks(self.w, {"light_armor": 3}, [], ["field_guns"])
        self.assertEqual(problems, ["field_guns needs a battalion of group infantry/mobile"])

    def test_build_order_any_building(self):
        items = calc.parse_order("civ:1,naval_base:2@1,rail_way:1x12,supply_node", self.w)
        self.assertEqual(items, ["civ", ("naval_base", 1, 1), ("naval_base", 2, 1), ("rail_way", 0, 12), "supply_node"])
        r = calc.build_order(self.w, items[1:], 15, 0, 0.0, "1936.1.1", infra=0)
        costs = [(x["kind"], x.get("level"), x["cost"]) for x in r["rows"]]
        self.assertEqual(costs, [("naval_base", 2, 6000), ("naval_base", 3, 7000), ("rail_way", 1, 2040),
                                 ("supply_node", None, 20000)])
        self.assertEqual(r["rows"][0]["days"], 80)  # 6000 / (15 x 5)
        self.assertEqual(r["rows"][2]["provinces"], 12)


class WeatherAndRules(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_regions_and_months(self):
        reg = self.w.strategic_regions()[1]
        self.assertEqual(reg["periods"][1]["start"], (1, 7))
        months = {m["name"]: m for m in theatre.weather_months(self.w, 1)}
        self.assertEqual((months["Jan"]["ground_snow"], months["Jan"]["cold"], months["Jan"]["mud"]),
                         ("high", "extreme", "none"))
        self.assertEqual(months["Jul"]["mud"], "heavy")
        self.assertEqual(months["Aug"]["heat"], "extreme")
        s = theatre.weather_summary(self.w, 1)
        self.assertEqual((s["mud"], s["snow"], s["cold"], s["heat"]), ("Jul", "Jan", "Jan", "Aug"))

    def test_weather_cli_by_state(self):
        rc, out = run_cli(["weather", "4"])
        self.assertEqual(rc, 0)
        self.assertIn("region 1", out)
        self.assertIn("Jul 8..26C", out)
        rc, out = run_cli(["weather", "R1", "--json"])
        self.assertEqual(json.loads(out)["regions"][0]["summary"]["heavy_mud"], "Jul")

    def test_adjacency_rules(self):
        r = self.w.adjacency_rules()["FIXTURE_CANAL"]
        self.assertEqual(r["required_provinces"], [11, 20])
        self.assertFalse(r["enemy"]["navy"])
        self.assertTrue(r["neutral"]["trade"])
        self.assertEqual(r["is_disabled"], "has_global_flag = FIXTURE_CANAL_BLOCKED")


class BalanceOfPower(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_definitions(self):
        b = self.w.power_balances()["AAA_balance"]
        self.assertEqual((b["initial_value"], b["left_side"], b["right_side"]), (0.3, "AAA_left_side", "AAA_right_side"))
        self.assertEqual([r["id"] for r in b["ranges"]], ["AAA_left_range", "AAA_middle_range", "AAA_right_range"])
        right = b["ranges"][-1]
        self.assertEqual((right["side"], right["modifier"], right["on_activate"]),
                         ("AAA_right_side", {"political_power_factor": 0.1}, ["stability:0.05"]))

    def test_start_movers_and_gained(self):
        rows = {r["id"]: r for r in scenario.bop(self.w, "AAA")}
        bal = rows["AAA_balance"]
        self.assertEqual(bal["start"], {"active": True, "value": 0.3, "src": "history/countries/AAA - Alpha.txt:15",
                                        "range": "AAA_right_range"})
        moves = [(m["kind"], m["id"], m["effect"], m["value"], m["daily"], m["via"]) for m in bal["movers"]]
        self.assertIn(("focus", "AAA_army_focus", "add_power_balance_value", 0.1, None, None), moves)
        self.assertIn(("decision", "AAA_lean_left", "add_power_balance_modifier", None, -0.002, None), moves)
        self.assertIn(("decision", "AAA_lean_left", "add_power_balance_value", -0.05, None, "AAA_bop_shift_effect"),
                      moves)
        self.assertNotIn(0.5, [m["value"] for m in bal["movers"]])  # effect_tooltip is display only
        self.assertEqual(bal["mover_counts"], {"focus": 1, "decision": 2})
        council = rows["AAA_council"]
        self.assertFalse(council["start"]["active"])
        self.assertEqual([g["id"] for g in council["gained_by"]], ["AAA_army_focus"])
        self.assertEqual(scenario.bop(self.w, "BBB"), [])

    def test_bop_cli(self):
        rc, out = run_cli(["bop", "AAA"])
        self.assertEqual(rc, 0)
        self.assertIn("at start: ACTIVE value 0.3 range AAA_right_range", out)
        self.assertIn("gained by focus AAA_army_focus", out)


class DecisionsLawsXp(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_never_take(self):
        nt = {x["id"]: x["reasons"] for x in scenario.never_take(self.w, "AAA")}
        self.assertEqual(nt["AAA_withdraw_from_beta"], ["white_peace:BBB", "name"])
        self.assertEqual(nt["AAA_hand_over_one"], ["transfer_state:1->BBB"])
        self.assertNotIn("AAA_build", nt)
        rc, out = run_cli(["decisions", "AAA", "--never", "--tag", "AAA"])
        self.assertIn("AAA_withdraw_from_beta \"AAA_withdraw_from_beta\" cost 15 once NEVER-TAKE (white_peace:BBB", out)
        self.assertNotIn("AAA_lean_left", out)

    def test_gates(self):
        gs = gates(parse("has_war_support > 0.15 OR = { has_government = fascism has_war = yes } "
                         "NOT = { has_idea = x } if = { limit = { tag = Q } has_stability > 0.5 }"))
        self.assertEqual([(g["key"], g["op"], g["value"], g["group"]) for g in gs],
                         [("has_war_support", ">", 0.15, "all"), ("has_government", "=", "fascism", "any"),
                          ("has_war", "=", "yes", "any"), ("has_idea", "=", "x", "not"),
                          ("has_stability", ">", 0.5, "if")])
        war = [o for o in self.w.laws("AAA")["economy"]["options"] if o["id"] == "war_mob"][0]
        self.assertEqual(war["gates"][0]["text"], "war support > 0.5")
        rc, out = run_cli(["laws", "AAA"])
        self.assertIn("GATES war support > 0.5", out)

    def test_experience(self):
        xp = self.w.experience("AAA")
        self.assertEqual(xp["start"]["navy"]["v"], 5)
        self.assertEqual(xp["start"]["navy"]["src"], "history/countries/AAA - Alpha.txt:19")
        self.assertEqual(xp["start"]["army"]["v"], 0)
        self.assertIn("[?]", xp["start"]["army"]["d"])
        self.assertEqual(xp_in(parse("army_experience = 25 if = { limit = { a = b } air_experience = 10 }")),
                         {"army": 25, "air": 10})

    def test_tech_level(self):
        t = self.w.tech_level("BBB")
        self.assertEqual(t["fair"], "aggregate")
        self.assertEqual(t["classes"]["air"], {"reached": True, "year": 1936, "kinds": ["fighter", "interceptor"]})
        self.assertFalse(t["classes"]["anti_tank"]["reached"])
        self.assertEqual(self.w.tech_level("AAA")["classes"]["armour"]["kinds"], ["light_tank_chassis"])


class Occupation(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_state_limit(self):
        row = {"id": 4, "cores": ["BBB"], "claims": [], "owner": "AAA", "controller": "AAA"}
        self.assertTrue(state_limit(parse("is_core_of = BBB NOT = { state = 5 }"), row))
        self.assertFalse(state_limit(parse("is_core_of = AAA"), row))
        self.assertIsNone(state_limit(parse("has_resistance = yes"), row))
        self.assertTrue(state_limit(parse("OR = { state = 4 has_x = yes }"), row))

    def test_start_values(self):
        occ = self.w.occupation_start("AAA")
        self.assertEqual(list(occ), [4])
        self.assertEqual((occ[4]["compliance"], occ[4]["garrison"], occ[4]["resistance_active"], occ[4]["resistance"]),
                         (25, 0.5, "yes", 10))
        self.assertEqual(occ[4]["srcs"][0], "history/states/4-Four.txt:2")
        self.assertFalse(occ[4]["core"])

    def test_laws_and_cli(self):
        laws = {x["id"]: x for x in self.w.occupation_laws()}
        self.assertTrue(laws["military_governor_occupation"]["default"])
        self.assertTrue(laws["military_governor_occupation"]["starting"])
        self.assertEqual(laws["no_garrison"]["state_modifier"]["resistance_target"], 0.4)
        self.assertEqual(laws["autonomous_occupation"]["visible"], "has_government = democratic")
        self.assertEqual([m["id"] for m in self.w.resistance_modifiers()], ["compliance_25", "resistance_50"])
        rc, out = run_cli(["occupation-laws", "--tag", "AAA"])
        self.assertEqual(rc, 0)
        self.assertIn("*^military_governor_occupation", out)
        self.assertIn("- 4 state 4 (non-core): yes / 10 / 25 / 0.5", out)
        rc, out = run_cli(["state", "4", "--tag", "AAA"])
        self.assertIn("occupation at start: resistance_active yes, resistance 10, compliance 25, garrison 0.5", out)


class PortsAndOverseas(unittest.TestCase):
    def test_sea_route_and_rules(self):
        g = fake_graph()
        r = mapgraph.sea_route(g, [90], [92])
        self.assertEqual(r["path"], [90, 91, 92])
        self.assertEqual(r["rules"], [{"rule": "FIXTURE_CANAL", "a": 91, "b": 92}])
        alt = mapgraph.sea_route(g, [90], [92], avoid_rules=("FIXTURE_CANAL",))
        self.assertEqual((alt["path"], alt["rules"]), ([90, 93, 94, 92], []))
        self.assertEqual(mapgraph.land_reach(g, [40], allowed=lambda p: p != 21), {40, 41, 20})
        self.assertEqual(mapgraph.sea_neighbours(g, 41), [92])

    def test_overseas_and_candidates(self):
        w = world_with_graph()
        ov = theatre.overseas(w, "AAA", "BBB")
        self.assertTrue(ov["overseas"])
        self.assertEqual(ov["capital_provinces"], [10])
        self.assertEqual([p["province"] for p in ov["home_ports"]], [11])
        self.assertEqual([p["province"] for p in ov["theatre_ports"]], [41])
        self.assertEqual((ov["route"]["from_port"], ov["route"]["to_port"], ov["route"]["steps"]), (11, 41, 2))
        rule = ov["rules"][0]
        self.assertEqual((rule["rule"], rule["navy_closed_when"], rule["required_provinces"]),
                         ("FIXTURE_CANAL", ["enemy", "contested"], {11: "AAA", 20: "BBB"}))
        self.assertEqual(ov["without"], [{"avoid": "FIXTURE_CANAL", "steps": 3, "pixels": 17, "rules": []}])
        self.assertFalse(theatre.overseas(world_with_graph("BBB"), "BBB", "AAA")["overseas"])
        c = theatre.port_candidates(w, "AAA", "BBB")
        rows = {r["province"]: r for r in c["rows"]}
        self.assertEqual(sorted(rows), [40, 41])
        self.assertEqual((rows[40]["steps"], rows[40]["level"], rows[40]["next_level_cost"]), (0, 0, 5000))
        self.assertEqual((rows[41]["level"], rows[41]["next_level_cost"]), (2, 7000))
        self.assertIsNotNone(rows[40]["finish"])
        self.assertEqual(theatre.port_candidates(w, "AAA", "BBB", below=2)["count"], 1)

    def test_supply_compare(self):
        s = theatre.supply_compare(world())
        self.assertEqual(s["costs"]["naval_base"]["base_cost"], 5000)
        self.assertEqual(s["costs"]["naval_base"]["per_level_extra_cost"], 1000)
        self.assertEqual(s["costs"]["supply_node"]["base_cost"], 20000)

    def test_theatre_and_dossier_lines(self):
        w = world_with_graph()
        th = theatre.theatre(w, "AAA", "BBB")
        self.assertEqual(th["terrain"]["basis"], "border+routes")
        self.assertEqual(th["terrain"]["fighting"]["counts"]["mountain"] >= 1, True)
        self.assertTrue(th["overseas"]["overseas"])
        self.assertEqual(th["weather"][0]["region"], 1)
        md = dossier.render_md(dossier.build_world(w))
        self.assertIn("- OVERSEAS: no land link from the capital", md)
        self.assertIn("via FIXTURE_CANAL", md)
        self.assertIn("- Port candidates (coast <= 3 steps from the front, naval base < 3; 2 found): 40", md)
        self.assertIn("- Terrain (border+routes", md)
        self.assertIn("width target", md)
        self.assertIn("- Weather region 1", md)
        rc, out = run_cli(["ports", "AAA"])  # no map graph in the fixture: still answers
        self.assertEqual(rc, 0)
        self.assertIn("naval_base 5000 +1000/level", out)


class DossierSections(unittest.TestCase):
    def test_new_lines(self):
        w = world()
        d = dossier.build_world(w)
        md = dossier.render_md(d)
        self.assertIn("- Experience at start: navy 5 [history/countries/AAA - Alpha.txt:19], army air 0", md)
        self.assertIn("army XP rewards: focuses +25 over 1 (AAA_army_focus +25)", md)
        self.assertIn("| on 10/25 |", md)
        self.assertIn("- Tech level at start (fair aggregate", md)
        self.assertIn("air 1936 (fighter, interceptor)", md)
        self.assertIn("NEVER-TAKE decisions of AAA", md)
        self.assertIn("- Balance of power AAA_balance [common/bop/AAA.txt:1]", md)
        self.assertIn("ACTIVE at start 0.3 in AAA_right_range", md)
        self.assertIn("gained by focus AAA_army_focus", md)
        self.assertIn("war_mob 150PP (war support > 0.5)", md)
        self.assertIn("NEVER-TAKE (white_peace:BBB)", md)
        self.assertEqual(d["enemies"][0]["tech_level"]["fair"], "aggregate")

    def test_cli_commands_run(self):
        for c in (["units"], ["weather", "1"], ["bop", "AAA"], ["occupation-laws"], ["ports", "AAA"],
                  ["decisions", "AAA", "--never"], ["laws", "AAA"], ["state", "4"]):
            rc, out = run_cli(c + ["--tag", "AAA"])
            self.assertEqual(rc, 0, (c, out))
            self.assertTrue(out.strip(), c)
        rc, out = run_cli(["terrain-width", "BBB", "--tag", "AAA"])
        self.assertEqual(rc, 1)  # the fixture has no map
        self.assertIn("no map graph", out)


@unittest.skipUnless(os.path.isfile(os.path.join(REAL, "common", "terrain", "00_terrain.txt")),
                     "HOI4 game folder not found")
class Golden(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(PLAN, encoding="utf-8") as fh:
            dlcs = json.load(fh)["_validator"]["dlcs"]
        cls.game = Game(REAL)
        cls.w = World(cls.game, "ITA", "1936.1.1", dlcs)
        cls.dlcs = dlcs

    def test_terrain_widths(self):
        want = {"plains": (70, 35), "forest": (60, 30), "hills": (70, 35), "mountain": (50, 25), "urban": (80, 40),
                "jungle": (60, 30), "marsh": (50, 25), "desert": (70, 35)}
        t = self.w.terrains()
        path = os.path.join(REAL, "common", "terrain", "00_terrain.txt")
        with open(path, encoding="utf-8-sig") as fh:
            lines = fh.read().splitlines()
        for name, (cw, sw) in want.items():
            self.assertEqual((t[name]["combat_width"], t[name]["combat_support_width"]), (cw, sw), name)
            ln = int(t[name]["width_src"].rsplit(":", 1)[1])
            self.assertEqual(lines[ln - 1].strip(), f"combat_width = {cw}", name)

    def test_regimental_support_rows(self):
        u = self.w.sub_units()
        self.assertEqual(u["field_guns"]["row"], "regimental support")
        self.assertEqual(u["engineer"]["row"], "divisional support")
        self.assertEqual(u["infantry"]["combat_width"], 2)
        self.assertTrue(u["mountaineers"]["special_forces"])

    def test_italian_colony_compliance(self):
        occ = self.w.occupation_start("ITA")
        self.assertEqual((occ[550]["compliance"], occ[550]["resistance"], occ[550]["resistance_active"]), (20, 0, "yes"))
        self.assertTrue(occ[550]["srcs"][-1].startswith("history/countries/ITA - Italy.txt:"))
        self.assertFalse(occ[550]["core"])
        self.assertEqual(occ[448]["compliance"], 35)  # Libya: every_owned_state with is_core_of = LBA
        self.assertNotIn(273, occ)  # the limit's NOT = { state = 273 }

    def test_ethiopia_bop_at_start_italy_by_focus(self):
        eth = {b["id"]: b for b in scenario.bop(self.w, "ETH")}["ETH_centralization_balance"]
        self.assertTrue(eth["start"]["active"])
        self.assertEqual(eth["start"]["value"], -0.25)
        self.assertTrue(eth["start"]["src"].startswith("history/countries/ETH - Ethiopia.txt:"))
        ita = {b["id"]: b for b in scenario.bop(self.w, "ITA")}["ITA_power_balance"]
        self.assertFalse(ita["start"]["active"])
        self.assertTrue(any(g["kind"] == "focus" for g in ita["gained_by"]))

    def test_withdraw_from_ethiopia_never_take(self):
        nt = {x["id"]: x for x in scenario.never_take(self.w, "ITA")}
        self.assertIn("white_peace:ETH", nt["ITA_withdraw_from_ethiopia"]["reasons"])
        self.assertTrue(nt["ITA_withdraw_from_ethiopia"]["src"].startswith("common/decisions/ITA.txt:"))

    def test_east_africa_overseas_for_italy(self):
        ov = theatre.overseas(self.w, "ITA", "ETH")
        self.assertTrue(ov["overseas"])
        self.assertIn("SUEZ_CANAL", [r["rule"] for r in ov["rules"]])
        self.assertIn(12766, [p["province"] for p in ov["theatre_ports"]])  # Massaua
        alt = {a["avoid"]: a for a in ov["without"]}["SUEZ_CANAL"]
        self.assertGreater(alt["steps"], ov["route"]["steps"])
        self.assertFalse(theatre.overseas(self.w, "ETH", "ITA")["overseas"])

    def test_weather_and_width_in_ethiopia(self):
        s = theatre.weather_summary(self.w, 17)
        self.assertIn("Jul", s["heavy_mud"].split(","))
        self.assertEqual(s["snow"], "none")
        th = theatre.theatre(self.w, "ITA", "ETH")
        self.assertEqual(th["terrain"]["width"]["dominant"], "mountain")
        self.assertTrue(th["port_candidates"]["rows"])

    def test_dossier_size_and_lines(self):
        md = dossier.render_md(dossier.build_world(self.w))
        self.assertLess(len(md.encode("utf-8")), 120_000)  # ~30k tokens
        for text in ("- OVERSEAS:", "NEVER-TAKE decisions of ITA", "ITA_withdraw_from_ethiopia (white_peace:ETH",
                     "Balance of power at start: ETH_centralization_balance -0.25",
                     "- Balance of power ITA_power_balance", "| on 0/20 |", "- Experience at start:",
                     "- Tech level at start (fair aggregate"):
            self.assertIn(text, md)


if __name__ == "__main__":
    unittest.main()
