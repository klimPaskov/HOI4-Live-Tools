"""scripts/hoi4facts/lint_fair.py: the FAIR multiplayer ruleset as plan checks (rule ids fair.*), run by
lint_plan.py when the plan's mode is fair.

The mode and action helpers are pure; the seeded plans read the installed game and are skipped without it."""

import copy
import json
import os
import re
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_fair, lint_plan  # noqa: E402

PLANS = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans")
RULES_DOC = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "references", "rules-and-modes.md")
GAME_DIR = os.environ.get("HOI4_DIR", lint_plan.DEFAULT_GAME_DIR)
HAVE_GAME = os.path.isfile(os.path.join(GAME_DIR, "map", "provinces.bmp"))
ITA = "italy-ethiopia-1936.plan.json"
ETH = "ethiopia-italy-1936.plan.json"


def load(name):
    with open(os.path.join(PLANS, name), encoding="utf-8") as fh:
        return json.load(fh)


DLCS = load(ITA)["_validator"]["dlcs"]


def plan(tag, start="1936.1.1", stop="1938.1.1", **keys):
    """A minimal plan for one tag (the history file is found by tag)."""
    p = {"_validator": {"tag": tag, "start_date": start, "stop_date": stop, "dlcs": DLCS}}
    p.update(keys)
    return p


def dip(action, target, when=None, **kw):
    d = {"id": f"d_{action}_{target}", "action": action, "target": target}
    if when is not None:
        d["when"] = when
    d.update(kw)
    return d


class Pure(unittest.TestCase):
    def test_mode_defaults_to_fair(self):
        self.assertEqual(lint_fair.mode_of({}), "fair")
        self.assertEqual(lint_fair.mode_of({"_validator": {"mode": "META"}}), "meta")
        self.assertEqual(lint_fair.mode_of({"_meta": {"mode": "meta"}}), "meta")
        self.assertEqual(lint_fair.mode_of({"run_spec": {"mode": "fair"}}), "fair")
        self.assertEqual(lint_fair.mode_of({"_validator": {"tag": "ITA"}}), "fair")

    def test_action_classes(self):
        cases = {"justify_war_goal": "justify", "Justify War Goal": "justify", "add_war_goal": "justify",
                 "cancel_war_goal": "other", "declare_war": "declare", "ask_military_access": "access",
                 "offer_military_access": "access", "revoke_military_access": "other",
                 "ask_docking_rights": "access", "Offer Docking Rights": "access", "join_faction": "join",
                 "invite_to_faction": "invite", "kick_from_faction": "other", "leave_faction": "leave_faction",
                 "send_volunteers": "volunteers", "expeditionary_force": "expedition",
                 "return_expeditions": "other", "start_lend_lease": "lend_lease", "request_lend_lease": "other",
                 "request_licence_production": "licence", "improve_relations": "other"}
        for action, kind in cases.items():
            self.assertEqual(lint_fair.kind_of(action), kind, action)

    def test_calls_walks_player_items(self):
        p = {"x": [{"call": {"tool": "hoi4_fleet_mission", "args": {"mission": "mine_laying"}}, "when": "1937.1.1"}]}
        found = list(lint_fair.calls(p))
        self.assertEqual([(w, t, a["mission"]) for w, t, a, _ in found], [("x[0]", "hoi4_fleet_mission", "mine_laying")])
        self.assertEqual(found[0][3]["when"], "1937.1.1")

    def test_rules_are_documented(self):
        doc = set(re.findall(r"^  (fair\.[a-z_]+)\s", lint_fair.__doc__, re.M))
        self.assertEqual(doc, set(lint_fair.RULES))
        self.assertIn("Executor guards", lint_fair.__doc__)
        with open(RULES_DOC, encoding="utf-8") as fh:
            text = fh.read()
        for word in ("Danzig", "space marines", "Submarine II", "18 paratrooper", "exploits.md", "_fair.exceptions",
                     "fair.*"):
            self.assertIn(word, text)

    def test_lint_plan_hook_is_one_line(self):
        with open(os.path.join(ROOT, "scripts", "hoi4facts", "lint_plan.py"), encoding="utf-8") as fh:
            self.assertEqual(fh.read().count("lint_fair"), 1)


@unittest.skipUnless(HAVE_GAME, f"game files not found at {GAME_DIR}")
class Seeded(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.game = lint_plan.load_game(GAME_DIR)
        cls.ita = load(ITA)

    def fair(self, p):
        lt = lint_plan.Lint(p, self.game)
        lint_fair.check(lt)
        return [f for f in lt.findings if f["rule"].startswith("fair.")]

    def hits(self, p, rule, severity="error", text=""):
        return [f for f in self.fair(p) if f["rule"] == rule and f["severity"] == severity
                and text in f["where"] + " " + f["message"]]

    def assertHit(self, p, rule, severity="error", text=""):
        got = self.hits(p, rule, severity, text)
        self.assertTrue(got, f"no {severity} {rule} with {text!r}; fair findings: {self.fair(p)}")
        self.assertIn("FAIR rule: ", got[0]["message"])
        return got

    def assertClean(self, p, rule=None):
        found = [f for f in self.fair(p) if rule is None or f["rule"] == rule]
        self.assertEqual(found, [])

    def italy(self, **keys):
        p = copy.deepcopy(self.ita)
        p.update(keys)
        return p

    # ------------------------------------------------------------------ army
    def test_width_at_war(self):
        small = {"id": "t_small", "name": "Small", "regiments": [["infantry", "infantry", "infantry"]]}
        self.assertHit(self.italy(templates=[small]), "fair.width", text="6 combat width")
        # a start template trained at war: new divisions of it are not starting divisions
        line = {"template": "Reggimento di Cavalleria", "deploy_state": 2, "only_if_stockpile": {"infantry_equipment": 1}}
        self.assertHit(self.italy(training=line), "fair.width", text="trains new divisions")
        # peace and no war planned: allowed
        self.assertClean(plan("GER", templates=[small]), "fair.width")
        wide = {"id": "t_wide", "name": "Wide", "regiments": [["infantry"] * 5]}
        self.assertClean(self.italy(templates=[wide]), "fair.width")

    def test_space_marines(self):
        sm = {"id": "t_sm", "name": "Marines", "regiments": [["infantry"] * 4, ["light_armor"]]}
        self.assertHit(self.italy(templates=[sm]), "fair.space_marines", text="tank battalions")
        cav = {"id": "t_cav", "name": "Cav", "regiments": [["cavalry"] * 4, ["light_tank_destroyer_brigade"]]}
        self.assertHit(self.italy(templates=[cav]), "fair.space_marines")
        tank = {"id": "t_tank", "name": "Tank", "regiments": [["light_armor"] * 3, ["motorized"] * 3]}
        support = {"id": "t_sup", "name": "Inf", "regiments": [["infantry"] * 5], "support": ["light_tank_recon"]}
        self.assertClean(self.italy(templates=[tank, support]), "fair.space_marines")
        line = {"template": "Divisione Celere", "deploy_state": 2, "only_if_stockpile": {"infantry_equipment": 1}}
        self.assertHit(self.italy(training=line), "fair.space_marines", text="Divisione Celere")

    def test_paratroopers(self):
        para8 = {"id": "t_p8", "name": "Para8", "regiments": [["paratrooper"] * 4, ["paratrooper"] * 4]}
        self.assertHit(self.italy(templates=[para8]), "fair.paratroopers", text="16 wide")
        para10 = {"id": "t_p10", "name": "Para10", "regiments": [["paratrooper"] * 5, ["paratrooper"] * 5]}
        line = {"template": "Para10", "deploy_state": 2, "cap": 19, "only_if_stockpile": {"infantry_equipment": 1}}
        self.assertHit(self.italy(templates=[para10], training=line), "fair.paratroopers", text="make 19")
        line = dict(line, cap=18)
        self.assertClean(self.italy(templates=[para10], training=line), "fair.paratroopers")
        uncapped = {k: v for k, v in line.items() if k != "cap"}
        self.assertHit(self.italy(templates=[para10], training=uncapped), "fair.paratroopers", "warning", "cap")

    def test_special_forces_conversion(self):
        alp = {"id": "t_alp", "name": "Divisione di Fanteria", "base": "Divisione di Fanteria",
               "regiments": [["mountaineers"] * 3, ["infantry"] * 3], "upgrade_fielded": True}
        self.assertHit(self.italy(templates=[alp]), "fair.special_forces", text="upgrade_fielded")
        self.assertClean(self.italy(templates=[dict(alp, upgrade_fielded=False)]), "fair.special_forces")

    def test_ghost_divisions(self):
        line = {"template": "Divisione di Fanteria", "deploy_state": 2}
        self.assertHit(self.italy(training=line), "fair.ghost_divisions", "warning")
        self.assertClean(self.italy(training=line, training_rules={"only_if_stockpile": {"infantry_equipment": 500}}),
                         "fair.ghost_divisions")

    # ------------------------------------------------------------------ navy and air
    def test_submarines(self):
        p = self.italy()
        p["research_path"][0]["techs"].append("improved_submarine")
        self.assertHit(p, "fair.submarines", text="improved_submarine")
        self.assertHit(self.italy(production=[{"equipment": "ship_hull_submarine_3", "factories": 1}]),
                       "fair.submarines", text="tier-3")
        design = {"id": "d_sub", "kind": "ship", "base": "ship_hull_submarine_2",
                  "modules": {"fixed_ship_engine_slot": "ship_engine_sub_aip_1", "rear_1_custom_slot":
                              "ship_mine_layer_sub"}}
        self.assertHit(self.italy(designs=[design]), "fair.submarines", text="special submarine module")
        self.assertHit(self.italy(designs=[design]), "fair.naval_mines", "warning", "mine-laying module")
        self.assertHit(self.italy(special_projects={"priority": ["sp_naval_anechoic_tiles"]}), "fair.submarines",
                       text="sp_naval_anechoic_tiles")
        self.assertHit(self.italy(research_fallback=True, research_fallback_prefixes=[], research_fallback_exclude=[]),
                       "fair.submarines", text="research_fallback_exclude")
        ok = {"id": "d_ok", "kind": "ship", "base": "ship_hull_submarine_2", "modules": {}}
        self.assertClean(self.italy(designs=[ok], production=[{"equipment": "ship_hull_submarine_2",
                                                               "factories": 2}]), "fair.submarines")

    def test_small_submarine_fleet(self):
        navy = {"posture": "coastal", "fleets": [{"fleet": "Latvijas Kara Flote", "missions": [{"mission": "patrol"}]}]}
        self.assertHit(plan("LAT", navy=navy), "fair.submarines", text="2 submarines")

    def test_naval_mines_and_blockade(self):
        navy = {"posture": "coastal", "fleets": [{"fleet": "Kriegsmarine", "missions": [
            {"mission": "mine_laying"}, {"mission": "convoy_raiding"},
            {"mission": "convoy_raiding", "when": "at_war_with:ENG"}]}]}
        p = plan("GER", navy=navy)
        self.assertHit(p, "fair.naval_mines", text="missions[0]")
        self.assertEqual([f["where"] for f in self.hits(p, "fair.blockade")], ["navy.fleets[0].missions[1]"])
        air = [{"wing": "W", "region": 17, "mission": "naval_mines"}]
        self.assertHit(plan("GER", air_plan=air), "fair.naval_mines", text="air_plan[0]")
        call = {"x": [{"call": {"tool": "hoi4_fleet_mission", "args": {"task_force": "TF", "mission": "mine_laying"}}}]}
        self.assertHit(plan("GER", **call), "fair.naval_mines", text="x[0]")

    def test_refinery_bombing(self):
        air = [{"wing": "B", "region": 17, "mission": "strategic_bombing", "rule": "hit the Ploiesti refineries"}]
        self.assertHit(plan("GER", air_plan=air), "fair.bombing")
        self.assertClean(plan("GER", air_plan=[dict(air[0], rule="industry")]), "fair.bombing")

    # ------------------------------------------------------------------ diplomacy
    def test_access_and_docking(self):
        self.assertHit(plan("GER", diplomacy=[dip("ask_military_access", "HUN")]), "fair.access")
        self.assertHit(plan("ITA", diplomacy=[dip("offer_docking_rights", "SPR")]), "fair.access")
        self.assertHit(plan("GER", diplomacy=[dip("ask_military_access", "JAP")]), "fair.access", "warning",
                       "Iran corridor")
        self.assertClean(plan("GER", diplomacy=[dip("revoke_military_access", "HUN")]), "fair.access")

    def test_exception_turns_a_rule_into_info(self):
        p = plan("GER", diplomacy=[dip("ask_military_access", "SPR")],
                 _fair={"exceptions": {"fair.access": "civil-war supplies to Nationalist Spain"}})
        self.assertHit(p, "fair.access", "info", "civil-war supplies")

    def test_justify_before_danzig(self):
        self.assertHit(plan("GER", diplomacy=[dip("justify_war_goal", "CZE", "1938.03.01")]), "fair.justify",
                       text="Danzig")
        self.assertClean(plan("GER", diplomacy=[dip("justify_war_goal", "CZE", "at_war_with:POL")]), "fair.justify")
        self.assertClean(plan("ENG", diplomacy=[dip("justify_war_goal", "ITA", "1939.09.01")]), "fair.justify")
        self.assertClean(plan("ITA", diplomacy=[dip("justify_war_goal", "YUG", "1937.1.1")]), "fair.justify")
        self.assertHit(plan("SOV", diplomacy=[dip("justify_war_goal", "PER", "1937.1.1")]), "fair.justify",
                       text="Molotov")
        self.assertClean(plan("SOV", diplomacy=[dip("justify_war_goal", "PER", "1939.08.24")]), "fair.justify")
        self.assertHit(plan("GER", diplomacy=[dip("justify_war_goal", "SOV", "1941.1.1")]), "fair.timing",
                       text="focus war goal")

    def test_factions(self):
        p = plan("GER", faction={"create": {"name": "Axis"}}, diplomacy=[dip("leave_faction", "GER")])
        self.assertHit(p, "fair.factions", text="dismantles")
        self.assertHit(plan("JAP", faction={"join": "GER"}), "fair.factions", text="joins GER")
        self.assertHit(plan("ITA", diplomacy=[dip("join_faction", "JAP")]), "fair.factions")

    def test_volunteers(self):
        self.assertHit(plan("GER", diplomacy=[dip("send_volunteers", "CHI")]), "fair.volunteers")
        self.assertClean(plan("GER", diplomacy=[dip("send_volunteers", "SPR")]), "fair.volunteers")
        self.assertHit(plan("USA", diplomacy=[dip("send_volunteers", "CHI")]), "fair.volunteers", "warning", "6 div")

    # ------------------------------------------------------------------ nation rules
    def test_war_timing(self):
        self.assertHit(plan("GER", diplomacy=[dip("declare_war", "SOV", "1940.06.01")]), "fair.timing", text="1941")
        self.assertClean(plan("GER", diplomacy=[dip("declare_war", "SOV", "1941.06.22")]), "fair.timing")
        self.assertHit(plan("JAP", diplomacy=[dip("declare_war", "USA", "1940.1.1")]), "fair.timing")
        self.assertHit(plan("JAP", diplomacy=[dip("declare_war", "ENG")]), "fair.timing", text="1936.01.01")
        self.assertClean(plan("JAP", diplomacy=[dip("declare_war", "USA", {"all": ["1941.12.07", "pp > 50"]})]),
                         "fair.timing")
        self.assertHit(plan("SOV", diplomacy=[dip("declare_war", "GER", "1941.06.01")]), "fair.timing", text="1942")

    def test_joins(self):
        self.assertHit(plan("USA", faction={"join": "ENG", "when": "1940.1.1"}), "fair.timing", text="Japan attacks")
        self.assertClean(plan("USA", faction={"join": "ENG", "when": "at_war_with:JAP"}), "fair.timing")
        self.assertClean(plan("USA", faction={"join": "ENG", "when": "controller:126 != ENG"}), "fair.timing")
        self.assertHit(plan("USA", diplomacy=[dip("declare_war", "GER", "1941.1.1")]), "fair.timing")
        self.assertHit(plan("SPR", faction={"join": "GER"}), "fair.timing", text="France falls")
        self.assertClean(plan("SPR", faction={"join": "GER", "when": "capitulated:FRA"}), "fair.timing")
        self.assertHit(plan("HOL", faction={"join": "ENG"}), "fair.timing", text="Germany declares")
        self.assertHit(plan("FIN", faction={"join": "GER"}), "fair.timing", "warning", "Winter War")
        self.assertClean(plan("FIN", faction={"join": "GER", "when": {"not": "at_war_with:SOV"}}), "fair.timing")

    def test_italy_joins_by_1941(self):
        late = plan("ITA", stop="1942.1.1", diplomacy=[dip("declare_war", "FRA", "1941.06.01")])
        self.assertHit(late, "fair.timing", text="by 1941.01.01")
        never = plan("ITA", stop="1942.1.1")
        self.assertHit(never, "fair.timing", "warning", "never joins")
        self.assertClean(plan("ITA", stop="1942.1.1", diplomacy=[dip("declare_war", "FRA", "1940.06.10")]),
                         "fair.timing")

    def test_usa_civilian_boost(self):
        p = plan("USA", trade=[{"id": "t1", "resource": "steel", "from": "ENG", "when": "1937.1.1"}])
        self.assertHit(p, "fair.timing", text="boost")
        p["trade"][0]["when"] = "1939.09.02"
        self.assertClean(p, "fair.timing")

    def test_invasion_prerequisites(self):
        inv = [{"id": "sealion", "from_state": 64, "target_state": 126, "armies": ["A"]}]
        self.assertHit(plan("GER", invasions=inv), "fair.invasions", text="Sea Lion")
        held = dict(inv[0], when=["controls_state:118", "controls_state:116"])
        self.assertClean(plan("GER", invasions=[held]), "fair.invasions")
        barb = dict(inv[0], when=["controls_state:446", "at_war_with:SOV"])
        self.assertClean(plan("GER", invasions=[barb]), "fair.invasions")
        france = [{"id": "bef", "from_state": 126, "target_state": 16, "armies": ["A"]}]
        self.assertHit(plan("ENG", invasions=france), "fair.invasions", text="capitulates")
        self.assertClean(plan("ENG", invasions=[dict(france[0], when="capitulated:FRA")]), "fair.invasions")
        aus = [{"id": "aus", "from_state": 282, "target_state": self.state("New South Wales"), "armies": ["A"]}]
        self.assertHit(plan("JAP", invasions=aus), "fair.invasions", text="Papua")
        japan = [{"id": "downfall", "from_state": 361, "target_state": self.state("Kanto"), "armies": ["A"]}]
        self.assertHit(plan("USA", invasions=japan), "fair.invasions", text="Iwo Jima")
        italy = [{"id": "avalanche", "from_state": 126, "target_state": 2, "armies": ["A"]}]
        self.assertHit(plan("ENG", invasions=italy), "fair.invasions", text="North Africa")
        self.assertClean(plan("ENG", invasions=[dict(italy[0], when=["capitulated:ITA", "capitulated:FRA"])]),
                         "fair.invasions")
        winter = [{"id": "w", "from_state": 195, "target_state": self.state("Uusimaa"), "armies": ["A"]}]
        self.assertHit(plan("SOV", invasions=winter), "fair.invasions", text="Winter War")

    def state(self, name):
        hit = [s for s, x in self.game.states().items() if name in x["names"]]
        self.assertEqual(len(hit), 1, name)
        return hit[0]

    def test_france(self):
        cp = [{"id": "cp_air", "order": 1, "building": "air_base", "state": 16, "when": "1936.1.1"}]
        self.assertHit(plan("FRA", construction_plan=cp), "fair.france", text="air_base")
        algeria = self.state("Algiers")
        line = {"template": "Division d'Infanterie", "deploy_state": algeria,
                "only_if_stockpile": {"infantry_equipment": 1}}
        self.assertHit(plan("FRA", training=line), "fair.france", text="outside mainland Europe")
        self.assertHit(plan("FRA", diplomacy=[dip("start_lend_lease", "ENG")]), "fair.france", text="lend-lease")
        self.assertClean(plan("FRA", diplomacy=[dip("start_lend_lease", "SPR")]), "fair.france")

    def test_licensed_plane_designs(self):
        design = {"id": "d_lic", "kind": "plane", "base": "small_plane_airframe_2", "modules": {}}
        p = plan("ITA", designs=[design], diplomacy=[dip("request_licence_production", "GER")])
        self.assertHit(p, "fair.licence", text="licence")
        self.assertClean(plan("ITA", designs=[design]), "fair.licence")

    def test_deletion(self):
        call = {"x": [{"call": {"tool": "hoi4_construction_demolish", "args": {"building": "industrial_complex"}}}]}
        self.assertHit(plan("GER", **call), "fair.deletion")
        dock = {"x": [{"call": {"tool": "hoi4_construction_demolish", "args": {"building": "dockyard"}}}]}
        self.assertHit(plan("GER", **dock), "fair.deletion", "warning")

    # ------------------------------------------------------------------ modes and the hand plans
    def test_meta_plan_has_no_fair_findings(self):
        p = plan("GER", diplomacy=[dip("ask_military_access", "HUN"), dip("justify_war_goal", "CZE", "1937.1.1")])
        p["_validator"]["mode"] = "meta"
        self.assertEqual(self.fair(p), [])

    def test_hand_plans_pass_through_lint_plan(self):
        for name in (ITA, ETH):
            out = lint_plan.lint(load(name), self.game)
            self.assertEqual([f for f in out["findings"] if f["rule"].startswith("fair.")], [], name)
            self.assertEqual(out["facts"]["fair"]["mode"], "fair")
            self.assertTrue(out["facts"]["fair"]["war_planned"])
        seeded = load(ITA)
        seeded["air_plan"].append({"wing": "W", "region": 273, "mission": "naval_mines"})
        out = lint_plan.lint(seeded, self.game)
        self.assertEqual([(f["rule"], f["severity"]) for f in out["findings"] if f["rule"].startswith("fair.")],
                         [("fair.naval_mines", "error")])


if __name__ == "__main__":
    unittest.main()
