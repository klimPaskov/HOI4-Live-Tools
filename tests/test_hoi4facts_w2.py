"""Wave-2 fact readers (F-12 to F-21): designers and modules, MIOs, special projects, world tension and ideology
gates, war goals, factions, peace-conference costs, officer-corps spirits / abilities / leader traits, the
intelligence agency, energy; the facts commands and the dossier section 10 / section 2 energy line.

Fixture tests use tests/fixtures/dossier/game (AAA fascist at war with BBB; a light tank chassis with module slots,
a duplicated AA family, modules, an MIO tree with include/override/remove, projects with script constants, faction
templates and goals, peace modifiers, spirits, abilities, traits, an agency branch and operations, a dam and coal
in AAA's state 1). The golden tests read the installed game and are skipped without it.
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

from hoi4facts import dossier, facts, mapgraph, scenario  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.script import parse  # noqa: E402
from hoi4facts.world import World, expand_scripted  # noqa: E402

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


class StaticTriggers(unittest.TestCase):
    def test_scripted_debug_and_aliases(self):
        w = world()
        self.assertTrue(w.static(parse("aaa_fix_trigger = yes")))
        self.assertFalse(w.static(parse("aaa_fix_trigger = no")))
        self.assertFalse(world("BBB").static(parse("aaa_fix_trigger = yes")))
        self.assertFalse(w.static(parse("is_debug = yes")))
        self.assertIsNone(w.static(parse("FROM = { tag = AAA }")))  # an unknown scope stays runtime
        self.assertTrue(w.static(parse("FROM = { tag = AAA }"), aliases=("FROM",)))
        self.assertTrue(w.static(None))
        b = expand_scripted(parse("NOT = { aaa_fix_trigger = yes }"), w.scripted_triggers())
        self.assertEqual(b[0][1][0][0], "AND")

    def test_gate_text(self):
        w = world()
        self.assertEqual(w.gate_text(parse("FROM = { has_tech = t_a }"), aliases=("FROM",)), "has_tech = t_a")
        self.assertTrue(w.gate_text(parse("FROM = { tag = AAA }"), aliases=("FROM",)))
        self.assertEqual(w.gate_text(parse("FROM = { if = { limit = { tag = BBB } has_war = yes } has_tech = t_b }"),
                                     aliases=("FROM",)), "has_tech = t_b")


class Designs(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_slots_inherit_copy_and_duplicates(self):
        slots = {s["slot"]: s for s in self.w.eq_slots("light_tank_chassis_1")}
        self.assertEqual(list(slots), ["turret_slot", "armament_slot", "extra_slot"])
        self.assertTrue(slots["turret_slot"]["required"])
        self.assertEqual(slots["extra_slot"]["categories"], ["fix_gun"])
        self.assertEqual(len(self.w.eq_slots("light_tank_chassis_2")), 3)  # module_slots = inherit
        defs = self.w.equipment_defs()
        self.assertEqual(defs["light_tank_aa_chassis_1"]["archetype"], "light_tank_aa_chassis")
        self.assertEqual(defs["light_tank_aa_chassis_2"]["parent"], "light_tank_aa_chassis_1")
        self.assertEqual(self.w.eq_kind("light_tank_aa_chassis_1"), "tank")
        self.assertEqual(self.w.eq_kind("fighter_equipment_1"), "plane")
        self.assertIsNone(self.w.eq_slots("infantry_equipment_1"))

    def test_designs_by_start_techs(self):
        d = self.w.designs("AAA")
        tank = d["kinds"]["tank"]
        self.assertTrue(tank["loaded"])
        self.assertEqual([b["id"] for b in tank["bases"]], ["light_tank_aa_chassis_1", "light_tank_chassis_1"])
        self.assertEqual({b["family"] for b in tank["bases"]}, {"light_tank_chassis"})
        self.assertIn("light_tank_chassis_2", [b["id"] for b in tank["next_bases"]])
        mods = {m["id"]: m for m in tank["modules"]}
        self.assertEqual(sorted(mods), ["fix_gun_mg", "fix_turret_small"])
        self.assertEqual(mods["fix_turret_small"]["xp_cost"], 2)
        self.assertIsNone(mods["fix_gun_mg"]["xp_cost"])  # the define's default applies
        self.assertEqual(mods["fix_gun_mg"]["can_convert_from"], ["fix_gun_cannon"])
        self.assertEqual(mods["fix_gun_mg"]["build_cost_resources"], {"steel": 1})
        self.assertEqual(tank["next_modules"][0]["id"], "fix_gun_cannon")
        self.assertEqual(tank["next_modules"][0]["year"], 1937)
        self.assertEqual(tank["variants"][0]["v"], "Alpha Tank")
        self.assertEqual(tank["variants"][0]["modules"]["turret_slot"], "fix_turret_small")
        up = [b for b in tank["bases"] if b["id"] == "light_tank_chassis_1"][0]["upgrades"][0]
        self.assertEqual((up["id"], up["max_level"], up["cost_by_level"]), ("fix_engine_upgrade", 5, 2))
        self.assertEqual(d["costs"]["EQUIPMENT_MODULE_ADD_XP_COST"]["v"], 5.0)
        self.assertEqual(d["costs"]["EQUIPMENT_MODULE_ADD_XP_COST"]["src"], "common/defines/01_wave2.lua:7")
        self.assertEqual(tank["xp"]["v"], 0)
        self.assertEqual(d["fair"], "public")

    def test_dlc_limit_on_modules(self):
        self.assertFalse(self.w.equipment_modules()["fix_turret_dlc"]["loaded"])
        self.assertTrue(world(dlcs=["Fixture DLC"]).equipment_modules()["fix_turret_dlc"]["loaded"])

    def test_date_changes_unlocks(self):
        d = world(date="1936.1.1").designs("BBB", "tank")
        self.assertEqual(d["kinds"].get("tank", {}).get("bases", []), [])  # BBB has no tank tech


class Mio(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_include_override_remove_add(self):
        o = self.w.mio_organisations()["AAA_fix_works_organization"]
        self.assertEqual(o["equipment_type"], ["armor"])
        tokens = [t["token"] for t in o["traits"]]
        self.assertEqual(tokens, ["fix_t1", "fix_t2", "AAA_special"])
        t2 = [t for t in o["traits"] if t["token"] == "fix_t2"][0]
        self.assertEqual(t2["production_bonus"], {"production_cost_factor": -0.1})
        self.assertEqual(o["initial_trait"]["equipment_bonus"], {"reliability": 0.05})

    def test_allowed_for_tag(self):
        m = self.w.mio("AAA")
        ids = [o["id"] for o in m["organisations"]]
        self.assertIn("AAA_fix_works_organization", ids)
        self.assertIn("AAA_scripted_organization", ids)  # scripted trigger expanded
        self.assertNotIn("generic_fix_tank_organization", ids)  # NOT original_tag = AAA
        self.assertNotIn("BBB_fix_organization", ids)
        self.assertNotIn("DEBUG_fix_organization", ids)
        works = [o for o in m["organisations"] if o["id"] == "AAA_fix_works_organization"][0]
        self.assertEqual(works["available"], "has_completed_focus = AAA_army_focus")
        self.assertTrue(works["visible"])
        scripted = [o for o in m["organisations"] if o["id"] == "AAA_scripted_organization"][0]
        self.assertFalse(scripted["visible"])  # FROM = { has_dlc } read as the country: no DLC
        pol = {p["id"]: p for p in m["policies"]}
        self.assertEqual((pol["mio_policy_fix_expensive"]["cost"], pol["mio_policy_fix_default"]["cost"]), (40, 25))
        self.assertEqual(pol["mio_policy_fix_default"]["cooldown"], 180)


class Projects(unittest.TestCase):
    def test_projects_and_scientists(self):
        p = world().projects("AAA")
        rows = {r["id"]: r for r in p["projects"]}
        self.assertEqual(sorted(rows), ["sp_fix_child", "sp_fix_tank"])  # sp_fix_dlc needs the DLC
        t = rows["sp_fix_tank"]
        self.assertEqual((t["prototype_days"], t["complexity"]), (45, {"min": 15, "max": 25}))
        self.assertEqual(t["available"], "has_tech = t_c")
        self.assertEqual(t["outputs"]["modules"], ["fix_turret_dlc"])
        self.assertEqual(t["outputs"]["techs"], ["t_dlc"])
        self.assertEqual(t["resources"], {"steel": 4})
        self.assertEqual(rows["sp_fix_child"]["parent"], ["sp_fix_tank"])
        self.assertEqual(rows["sp_fix_child"]["complexity"], {"min": 40, "max": 40})
        self.assertEqual(p["scientists"][0]["skills"], {"specialization_land": 2})
        self.assertIn("land_facility", p["facility_buildings"])
        self.assertEqual(p["scientist_traits"]["fix_sci_trait"]["modifier"]["special_project_speed_factor"], 0.1)
        self.assertIn("sp_fix_dlc", [r["id"] for r in world(dlcs=["Fixture DLC"]).projects("AAA")["projects"]])


class Diplomacy(unittest.TestCase):
    def test_gates_tension_wargoals(self):
        d = world().diplomacy("AAA")
        self.assertEqual(d["ideology"], "fascism")
        self.assertEqual(d["gates"]["lend_lease_tension"]["need"], 0.7)  # fascism 0.5 + aaa_spirit 0.2
        self.assertEqual(d["gates"]["join_faction_tension"]["need"], -0.1)  # hidden_modifier
        self.assertEqual(d["gates"]["generate_wargoal_tension"]["need"], 0)
        self.assertIn("justify_war_goal_time", [s["key"] for s in d["spirits"]])
        self.assertEqual(d["world_tension"]["total"], 5)  # the 1939 threat is after the start
        self.assertEqual(d["world_tension"]["named"][0]["src"], "history/countries/AAA - Alpha.txt:31")
        self.assertEqual(d["world_tension"]["wars"][0]["threat"], 0.5)
        goals = {g["id"]: g for g in d["wargoals"]}
        self.assertNotIn("annex_everything", goals)  # allowed = { always = no }
        self.assertNotIn("topple_government", goals)  # needs the DLC
        self.assertEqual((goals["take_state"]["generate_base_cost"], goals["take_state"]["threat"]), (200, 2))
        self.assertEqual(world(date="1939.1.1").world_tension()["total"], 35)
        b = world("BBB").diplomacy("BBB")
        self.assertEqual(b["gates"]["generate_wargoal_tension"]["need"], 1.0)
        self.assertEqual(b["rules"]["can_only_justify_war_on_threat_country"], "yes")


class Factions(unittest.TestCase):
    def test_templates_goals_rules(self):
        f = scenario.factions(world(), "AAA")
        t = {x["id"]: x for x in f["templates"]}
        self.assertEqual(sorted(t), ["faction_template_aaa"])  # open needs the DLC, hidden never shows
        aaa = t["faction_template_aaa"]
        self.assertEqual(aaa["created_by"][0]["id"], "AAA_build")
        self.assertEqual(aaa["manifest"]["sections"]["fulfilled"]["min"], 0.75)  # @fix_value resolved
        self.assertEqual(aaa["goals"][0]["category"], "medium_term")
        self.assertEqual(aaa["default_rules"][0]["modifier"], {"stability_factor": 0.05})
        self.assertEqual(f["rule_groups"][0]["default"], "fix_joining_rule")
        self.assertEqual(f["member_upgrades"]["groups"][0]["upgrades"], ["fix_share0", "fix_share1"])
        g = scenario.factions(world(dlcs=["Fixture DLC"]), "AAA")
        self.assertTrue([x for x in g["templates"] if x["id"] == "faction_template_open"][0]["selectable"])


class Peace(unittest.TestCase):
    def test_relations(self):
        p = world().peace("AAA")
        rel = {m["id"]: m["relation"] for m in p["modifiers"]}
        self.assertEqual(rel, {"aaa_core_cheap": "own", "bbb_defends": "enemy", "generic_claim": "generic"})
        m = [x for x in p["modifiers"] if x["id"] == "bbb_defends"][0]
        self.assertEqual((m["types"], m["cost_multiplier"]), (["take_states", "puppet"], 1.5))
        self.assertEqual(p["categories"]["is_core"], "is_core")  # no localisation in the fixture: the id
        self.assertEqual(p["defines"]["PEACE_CONTEST_REFUND_FACTOR"]["v"], [1.0, 0.92, 0.84, 0.76])
        self.assertNotIn("bbb_defends", [x["id"] for x in world().peace("AAA", enemies=[])["modifiers"]])


class Command(unittest.TestCase):
    def test_spirits_abilities_traits(self):
        c = world().command("AAA")
        sp = {s["id"]: s for s in c["spirits"]}
        self.assertEqual(sorted(sp), ["fix_bold_spirit", "fix_navy_spirit"])  # DLC, BBB-only and BBB focus left out
        self.assertEqual((sp["fix_bold_spirit"]["cost"], sp["fix_bold_spirit"]["branch"]), (20, "army"))
        self.assertEqual(sp["fix_navy_spirit"]["cost"], 40)  # own cost over the slot's 35
        ab = {a["id"]: a for a in c["abilities"]}
        self.assertEqual(sorted(ab), ["fix_force_attack"])
        self.assertEqual(ab["fix_force_attack"]["allowed"], "is_border_war = no")  # the DLC branch is not taken
        self.assertEqual(ab["fix_force_attack"]["modifiers"]["unit_modifiers"], {"org_loss_when_moving": -0.2})
        tr = {t["id"]: t for t in c["traits"]}
        self.assertEqual(sorted(tr), ["fix_assignable", "fix_child"])
        self.assertEqual(tr["fix_child"]["any_parent"], ["fix_assignable"])
        self.assertEqual(tr["fix_child"]["enable_ability"], "fix_force_attack")
        self.assertEqual(c["skills"][1]["cost"], 200)
        self.assertEqual(c["defines"]["PROMOTE_LEADER_CP_COST"]["v"], 40.0)
        d = world(dlcs=["Fixture DLC"]).command("AAA")
        self.assertIn("fix_dlc_ability", [a["id"] for a in d["abilities"]])
        self.assertIn("fix_dlc_spirit", [s["id"] for s in d["spirits"]])


class Agency(unittest.TestCase):
    def test_upgrades_operations(self):
        a = world().agency("AAA")
        up = {u["id"]: u for u in a["upgrades"]}
        self.assertEqual(up["upgrade_fix_army"]["levels"], [{"army_intel_factor": 0.25}, {"army_intel_factor": 0.15}])
        self.assertEqual(up["upgrade_fix_army"]["during_progress"], {"civilian_factory_use": 5})
        self.assertEqual(up["upgrade_fix_radio"]["available"], "has_tech = t_c")
        ops = {o["id"]: o for o in a["operations"]}
        self.assertEqual(sorted(ops), ["operation_fix"])  # BBB-only left out for AAA
        self.assertEqual(ops["operation_fix"]["phases"], [["fix_phase_1"], ["fix_phase_2"]])
        self.assertEqual((ops["operation_fix"]["days"], ops["operation_fix"]["operatives"]), (90, 2))
        self.assertEqual(a["phases"]["fix_phase_1"]["equipment"], {"infantry_equipment": 10})
        self.assertIsNone(a["agency_at_start"])
        self.assertIsNotNone(world("BBB").agency("BBB")["agency_at_start"])
        self.assertEqual(a["defines"]["AGENCY_CREATION_FACTORIES"]["v"], 5)


class Energy(unittest.TestCase):
    def test_energy_estimate(self):
        e = world().energy("AAA")
        s1 = [s for s in e["states"] if s["id"] == 1][0]
        self.assertEqual((s1["coal"], s1["factories"], s1["buildings"]), (2, 3, {"dam": 1}))
        self.assertEqual(s1["local_consumption"], -0.65)
        self.assertEqual(e["coal"], 2)
        self.assertEqual(e["estimate"]["produced"], 28.0)  # 10 + 2 x 9
        self.assertEqual(e["estimate"]["per_factory"], round(0.25 + 0.0225 * 3, 4))
        self.assertEqual(e["estimate"]["demand"], round(3 * (0.25 + 0.0225 * 3) * 0.35, 2))
        self.assertEqual(sorted(e["keystones"]), ["energy_infrastructure"])  # the dam is not buildable
        rows = dossier.state_rows(world(), "AAA")
        self.assertEqual([r["coal"] for r in rows if r["id"] == 1], [2])


class CliAndDossier(unittest.TestCase):
    def test_commands_run(self):
        for c in (["designs", "AAA"], ["designs", "AAA", "--kind", "tank"], ["mio", "AAA"],
                  ["mio", "AAA", "--id", "AAA_fix_works_organization"], ["projects", "AAA"], ["diplomacy", "AAA"],
                  ["factions", "AAA"], ["peace", "AAA"], ["command", "AAA"], ["agency", "--tag", "AAA"], ["agency"],
                  ["states", "AAA"], ["states", "AAA", "--energy"]):
            rc, out = run_cli(c + ["--tag", "AAA"] if c[0] != "agency" else c)
            self.assertEqual(rc, 0, (c, out))
            self.assertTrue(out.strip(), c)
        rc, out = run_cli(["designs", "AAA", "--kind", "tank", "--tag", "AAA"])
        self.assertIn("- light_tank_chassis: light_tank_chassis_1 (newest 1936, by t_a); same slots as "
                      "light_tank_aa_chassis; slots turret_slot*(fix_turret)", out)
        self.assertIn("start variant \"Alpha Tank\" light_tank_chassis_1", out)
        rc, out = run_cli(["diplomacy", "AAA", "--tag", "AAA"])
        self.assertIn("- send lend-lease: 70% (fascism 0.5, aaa_spirit +0.2", out)
        rc, out = run_cli(["states", "AAA", "--tag", "AAA"])
        self.assertIn("Energy: coal 2 in own states -> 28 produced", out)
        rc, out = run_cli(["states", "AAA", "--energy", "--json", "--tag", "AAA"])
        self.assertEqual(json.loads(out)["coal"], 2)

    def test_fair_hides_another_countrys_private_start(self):
        rc, out = run_cli(["designs", "BBB", "--tag", "AAA"])
        self.assertEqual(rc, 1)
        self.assertIn("hidden in FAIR mode", out)
        rc, out = run_cli(["designs", "BBB", "--tag", "AAA", "--mode", "meta"])
        self.assertEqual(rc, 0)
        rc, out = run_cli(["projects", "AAA", "--tag", "BBB", "--json"])
        self.assertEqual(json.loads(out)["scientists"], [])
        rc, out = run_cli(["projects", "AAA", "--tag", "AAA", "--json"])
        self.assertEqual(len(json.loads(out)["scientists"]), 1)
        rc, out = run_cli(["command", "AAA", "--tag", "BBB"])
        self.assertIn("command power at start hidden in FAIR mode", out)
        rc, out = run_cli(["diplomacy", "BBB", "--tag", "AAA"])  # ideology and spirits are public
        self.assertEqual(rc, 0)
        self.assertIn("ideology democratic", out)

    def test_dossier_section_10_and_energy(self):
        d = dossier.build_world(world())
        md = dossier.render_md(d)
        self.assertIn("## 10. Diplomacy, factions, peace, designers, MIOs, projects, command, agency", md)
        for text in ("- World tension at start 5% (history threats: AAA 5; wars at start AAA->BBB annex_everything "
                     "threat 0.5). fascism gates (world tension needed): justify any, faction any",
                     "lend-lease 70%", "templates by our focuses/decisions: faction_template_aaa",
                     "ours aaa_core_cheap take_states x0.5; naming the enemy bbb_defends take_states/puppet x1.5",
                     "- Designers: tank on (army XP 0): 1 families, 2 modules, 1 start variants",
                     "- MIOs: 2 ours (fix_works, scripted)", "- Special projects: 2 (land 2)",
                     "- Command: spirits army 1 in 1 slots (20 XP), navy 1 in 1 slots (40 XP)",
                     "- Intelligence agency: La Resistance OFF; none at start",
                     "- Energy: coal 2 in own states -> 28 produced"):
            self.assertIn(text, md)
        self.assertEqual(dossier.section_text(md, "rules").splitlines()[0], "## 10. Diplomacy, factions, peace, designers, "
                                                                           "MIOs, projects, command, agency")
        self.assertEqual(d["rules"]["fair"], "public")


@unittest.skipUnless(os.path.isfile(os.path.join(REAL, "common", "ideologies", "00_ideologies.txt")),
                     "HOI4 game folder not found")
class Golden(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(PLAN, encoding="utf-8") as fh:
            dlcs = json.load(fh)["_validator"]["dlcs"]
        cls.game = Game(REAL)
        cls.w = World(cls.game, "ITA", "1936.1.1", dlcs)
        cls.eth = World(cls.game, "ETH", "1936.1.1", dlcs)

    def _line(self, src):
        path, ln = src.rsplit(":", 1)
        with open(os.path.join(REAL, path), encoding="utf-8-sig", errors="replace") as fh:
            return fh.read().splitlines()[int(ln) - 1]

    def test_design_xp_defines_and_tank_modules(self):
        d = self.w.designs("ITA", "tank")
        c = d["costs"]
        self.assertEqual((c["EQUIPMENT_MODULE_ADD_XP_COST"]["v"], c["EQUIPMENT_MODULE_REPLACE_XP_COST"]["v"],
                          c["EQUIPMENT_MODULE_CONVERT_XP_COST"]["v"], c["EQUIPMENT_MODULE_REMOVE_XP_COST"]["v"]),
                         (5.0, 6.0, 3.0, 1.0))
        self.assertIn("EQUIPMENT_MODULE_ADD_XP_COST", self._line(c["EQUIPMENT_MODULE_ADD_XP_COST"]["src"]))
        tank = d["kinds"]["tank"]
        mods = {m["id"]: m for m in tank["modules"]}
        self.assertEqual(mods["tank_riveted_armor"]["xp_cost"], 8)
        self.assertIn("tank_riveted_armor", self._line(mods["tank_riveted_armor"]["src"]))
        self.assertIn("light_tank_chassis_1", [b["id"] for b in tank["bases"]])
        self.assertIn("L3/35", [v["v"] for v in tank["variants"]])
        ship = self.w.designs("ITA", "ship")["kinds"]["ship"]
        heavy = [b for b in ship["bases"] if b["id"] == "ship_hull_heavy_2"][0]
        self.assertIn("fixed_ship_battery_slot", [s["slot"] for s in heavy["slots"]])  # slot = inherit resolved

    def test_italian_mios(self):
        m = self.w.mio("ITA")
        own = [o["id"] for o in m["organisations"] if not o["foreign"]]
        self.assertIn("ITA_fiat_organization", own)
        self.assertNotIn("generic_tank_organization", own)
        self.assertFalse([o for o in own if o.startswith("DEBUG_")])
        fiat = [o for o in m["organisations"] if o["id"] == "ITA_fiat_organization"][0]
        self.assertIn("ITA_mio_trait_new_industrialization_program", [t["token"] for t in fiat["traits"]])
        self.assertEqual(m["defines"]["DEFAULT_INITIAL_POLICY_ATTACH_COST"]["v"], 25)

    def test_projects_and_scientists(self):
        p = self.w.projects("ITA")
        rows = {r["id"]: r for r in p["projects"]}
        self.assertEqual(rows["sp_land_flamethrower_tank"]["prototype_days"], 45)
        self.assertEqual(rows["sp_land_flamethrower_tank"]["complexity"], {"min": 15, "max": 25})
        self.assertIn("ITA_enrico_fermi", [s["id"] for s in p["scientists"]])

    def test_ideology_gates_and_wargoals(self):
        d = self.w.diplomacy("ITA")
        self.assertEqual(d["ideology"], "fascism")
        self.assertEqual(d["gates"]["join_faction_tension"]["ideology"], -0.1)
        self.assertGreaterEqual(d["gates"]["lend_lease_tension"]["need"], 0.5)
        e = self.eth.diplomacy("ETH")
        self.assertEqual(e["ideology"], "neutrality")
        self.assertEqual((e["gates"]["generate_wargoal_tension"]["need"], e["gates"]["join_faction_tension"]["need"]),
                         (0.5, 0.4))
        goals = {g["id"]: g for g in d["wargoals"]}
        self.assertEqual((goals["take_state"]["generate_base_cost"], goals["take_state"]["generate_per_state_cost"]),
                         (200, 50))
        self.assertIn("ITA", [w["attacker"] for w in d["world_tension"]["wars"]])
        self.assertEqual(World(self.game, "GER", "1939.1.1", self.w.dlcs).world_tension()["total"] >= 40, True)

    def test_factions_peace_command_agency(self):
        f = scenario.factions(self.w, "ITA")
        created = {t["id"] for t in f["templates"] if t["created_by"]}
        self.assertIn("faction_template_italy_empire_faction", created)
        p = self.w.peace("ITA")
        self.assertIn("ITA_proclaim_the_empire", [m["id"] for m in p["modifiers"] if m["relation"] == "own"])
        self.assertIn("ITA_forced_peace_with_ethiopia", [m["id"] for m in p["modifiers"] if m["relation"] == "enemy"])
        c = self.w.command("ITA")
        self.assertEqual(c["defines"]["PROMOTE_LEADER_CP_COST"]["v"], 40.0)
        self.assertFalse([s for s in c["spirits"] if s["id"].startswith("CHI_")])
        a = self.w.agency("ITA")
        self.assertIn("upgrade_passive_defense", [u["id"] for u in a["upgrades"]])
        self.assertEqual(a["defines"]["AGENCY_CREATION_DAYS"]["v"], 30)

    def test_energy_and_dossier_size(self):
        e = self.w.energy("ITA")
        self.assertEqual(e["resource"], "coal")
        self.assertEqual(e["defines"]["RESOURCE_TO_ENERGY_COEFFICIENT"]["v"], 9.0)
        self.assertEqual(e["coal"], sum(s["resources"].get("coal", 0) for sid, s in self.w.states().items()
                                        if s["owner"] == "ITA"))
        self.assertIn("energy_infrastructure", e["keystones"])
        md = dossier.render_md(dossier.build_world(self.w))
        self.assertLess(len(md.encode("utf-8")), 120_000)  # ~30k tokens
        for text in ("## 10. Diplomacy, factions", "- Energy: coal", "fascism gates", "- MIOs:", "- Designers:",
                     "- Intelligence agency: La Resistance on"):
            self.assertIn(text, md)


if __name__ == "__main__":
    unittest.main()
