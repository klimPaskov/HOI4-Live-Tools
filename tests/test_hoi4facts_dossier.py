"""Dossier, facts CLI and calculators on a small fixture (no game files needed).

Fixture tests/fixtures/dossier/game: AAA (states 1) at war with BBB (states 2, 3) from the history
(`has_start_date < 1936.01.02`), BBB's starting spirit lowers its surrender limit by 0.1 and a
possible spirit by 0.2; a goal decision for BBB, a mission for AAA with a variable timer, an
outcome flag in a scripted effect (IF/ELSE), an on_daily threshold, a focus tree with a DLC-only
focus, technologies with a DLC folder, sub-units and equipment, defines with a mod-style override.
"""
import contextlib
import io
import json
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import calc, dossier, facts, focus, research, scenario  # noqa: E402
from hoi4facts.defines import parse_lua  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.script import Ctx, parse, trigger  # noqa: E402
from hoi4facts.world import World, add_days, control_lists, effect_summary, reduce_trigger  # noqa: E402

GAME = os.path.join(ROOT, "tests", "fixtures", "dossier", "game")


def world(date="1936.1.1", dlcs=(), mode="fair", tag="AAA"):
    return World(Game(GAME, cache_dir=tempfile.gettempdir()), tag, date, dlcs, mode)


class ScriptLines(unittest.TestCase):
    def test_block_lines_and_ops(self):
        items = parse("a = 1\nb = {\n  c > 2\n}\n")
        self.assertEqual(items.lines, [1, 2])
        self.assertEqual(items[1][1].lines, [3])
        self.assertEqual(items[1][1].ops, [">"])

    def test_has_start_date(self):
        block = parse("has_start_date < 1936.01.02")
        self.assertTrue(trigger(block, Ctx("AAA", [], "1936.1.1")))
        self.assertFalse(trigger(block, Ctx("AAA", [], "1939.1.1")))
        self.assertIsNone(trigger(block, Ctx("AAA", [])))

    def test_reduce_trigger(self):
        block = parse('has_war = yes has_dlc = "X" IF = { limit = { tag = BBB } a = 1 } ELSE = { b = 2 }')
        from hoi4facts.script import render
        self.assertEqual(render(reduce_trigger(block, Ctx("AAA", ["X"]))), "has_war = yes b = 2")

    def test_control_lists_and_effects(self):
        av = parse("has_full_control_of_state = 1 controls_state = 3 NOT = { controls_state = 2 } "
                   "count_triggers = { amount = 2 controls_province = 20 }")
        ctl = control_lists(av)
        self.assertEqual(ctl["states"], [1, 3])
        self.assertEqual(ctl["provinces"], [20])
        eff = effect_summary(parse("1 = { add_building_construction = { type = arms_factory level = 2 } } "
                                   "add_ideas = x country_event = { id = e.1 }"))
        self.assertEqual(eff, ["build:arms_factory x2@1", "idea:x", "event:e.1"])

    def test_calendar(self):
        self.assertEqual(add_days("1936.1.1", 35), "1936.2.5")
        self.assertEqual(add_days("1936.12.31", 1), "1937.1.1")
        self.assertEqual(add_days("1936.2.28", 1), "1936.3.1")  # 365-day calendar


class Defines(unittest.TestCase):
    def test_parse_lua(self):
        rows = {k: (v, ln) for k, v, ln in parse_lua('NDefines = {\nNA = {\n\tX = 0.8, -- c\n\tL = { 1, 2 },\n},\n}\n'
                                                     'NDefines.NA.Y = -3\n')}
        self.assertEqual(rows["NA.X"], (0.8, 3))
        self.assertEqual(rows["NA.L"][0], [1, 2])
        self.assertEqual(rows["NA.Y"], (-3, 7))

    def test_override_and_source(self):
        w = world()
        v, f = w.define("MIN_SURRENDER_LIMIT")
        self.assertEqual(v, 0.25)
        self.assertEqual(f["src"], "common/defines/zz_mod_override.lua:1")


class WorldFacts(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_country(self):
        c = self.w.country()
        self.assertEqual(c["research_slots"]["v"], 3)
        self.assertEqual(c["research_slots"]["src"], "history/countries/AAA - Alpha.txt:2")
        self.assertEqual([o["v"] for o in c["oobs"]], ["AAA_1936"])
        self.assertEqual(world(dlcs=["Fixture DLC"]).country()["oobs"][0]["v"], "AAA_dlc")
        self.assertEqual({v["name"]: v["v"] for v in c["variables"]}, {"AAA_conquer_days": 170, "AAA_other": 3})
        self.assertIsNone(c["political_power"])  # the 1939 block is after the start

    def test_wars_by_start_date(self):
        self.assertEqual(self.w.at_war_with("AAA"), ["BBB"])
        self.assertEqual(world("1939.1.1").at_war_with("AAA"), [])

    def test_states_and_slots(self):
        s = self.w.states()[1]
        self.assertEqual((s["slots"], s["used_slots"], s["free_slots"], s["infrastructure"]), (6, 3, 3, 3))
        self.assertEqual(s["province_buildings"], {11: {"naval_base": 1}})

    def test_vp_and_surrender(self):
        v = self.w.vp("BBB")
        self.assertEqual(v["total"], 15)
        self.assertEqual(v["thresholds"][0]["limit"], 0.7)          # 0.8 - 0.1 held at start
        self.assertEqual(v["thresholds"][0]["vp_needed"], 11)       # ceil(0.7 x 15)
        self.assertEqual(v["thresholds"][1], {"limit": 0.5, "vp_needed": 8, "with": "BBB_demoralized"})
        self.assertEqual(v["base_limit"]["src"], "common/defines/00_defines.lua:8")

    def test_decisions_missions_goals(self):
        m = self.w.decisions("AAA", missions=True)
        self.assertEqual([x["id"] for x in m], ["AAA_conquer_mission"])
        self.assertEqual(m[0]["mission_days_value"]["v"], 170)
        d = [x for x in self.w.decisions("BBB") if x["id"] == "BBB_defeat_aaa"][0]
        self.assertEqual(d["controls"]["states"], [1, 3])
        self.assertEqual(d["src"], "common/decisions/BBB.txt:2")
        self.assertNotIn("AAA_build", [x["id"] for x in self.w.decisions("BBB")])

    def test_laws_and_ideas(self):
        laws = self.w.laws("AAA")
        self.assertEqual(laws["economy"]["current"], "partial_mob")
        war = [o for o in laws["economy"]["options"] if o["id"] == "war_mob"][0]
        self.assertEqual(war["cost"], 150)
        self.assertEqual(war["available"], "has_war_support > 0.5 has_dlc = \"Fixture DLC\"")
        self.assertEqual(world(dlcs=["Fixture DLC"]).idea_row("war_mob")["available"], "has_war_support > 0.5")

    def test_oob_and_characters(self):
        o = self.w.oob("AAA")
        self.assertEqual(o["templates"]["Line"]["regiments"], {"infantry": 2})
        self.assertEqual([d["state"] for d in o["divisions"]], [1, 1])
        gen = self.w.characters("AAA", "general")
        self.assertEqual(gen[0]["id"], "AAA_general")
        self.assertTrue(gen[0]["recruited"])
        self.assertEqual(gen[0]["roles"][0]["attack"], 2)


class FocusResearchHooks(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_tree_loads_by_tag_and_dlc(self):
        t = focus.tree(self.w, "AAA")
        self.assertEqual(t["id"], "alpha_focus")
        self.assertEqual(t["hidden"], ["AAA_dlc_only"])
        self.assertIn("AAA_dlc_only", focus.tree(world(dlcs=["Fixture DLC"]), "AAA")["focuses"])
        self.assertEqual(focus.tree(self.w, "BBB")["id"], "generic_focus")
        root = t["focuses"]["AAA_root"]
        self.assertEqual((root["days"], root["name"]), (35, "Root focus"))
        self.assertEqual(t["focuses"]["AAA_left"]["reward"], ["slots:+1@1", "build:arms_factory x2@1"])

    def test_timeline(self):
        rows = focus.timeline(self.w, "AAA", ["AAA_root", "AAA_left", "AAA_right", "AAA_end", "AAA_dlc_only"])
        self.assertEqual((rows[0]["end"], rows[1]["end"]), ("1936.2.5", "1936.4.16"))
        self.assertIn("exclusive with AAA_left", rows[2]["flags"])
        self.assertEqual(rows[3]["flags"], [])
        self.assertEqual(rows[4]["flags"], ["not loaded"])

    def test_research(self):
        ts = research.techs(self.w)
        self.assertNotIn("t_dlc", ts)
        self.assertEqual(ts["t_c"]["parents"], ["t_a", "t_b"])
        cal = research.calendar(self.w, "AAA", "1936.1.1", (1936, 1937))
        c = [t for t in cal[1]["techs"] if t["id"] == "t_c"][0]
        self.assertEqual(c["chain_all"], ["t_b"])
        self.assertEqual(c["chain_any"], [])
        self.assertEqual((c["days"], c["days_at_start"]), (150, 450))
        plan = {"_validator": {"tag": "AAA"}, "research_path": [{"year": 1936, "techs": ["t_b", "t_c"]}]}
        tl = research.timeline(self.w, plan, slots=1)
        self.assertEqual([(r["id"], r["start"], r["end"]) for r in tl["rows"]],
                         [("t_b", "1936.1.1", "1936.7.20"), ("t_c", "1936.7.20", "1937.10.13")])

    def test_hooks(self):
        h = scenario.hooks(self.w, "AAA")
        self.assertEqual(h["wars"][0]["target"], "BBB")
        flags = {o["flag"]: o for o in h["outcomes"]}
        self.assertEqual(flags["AAA_victory_flag"]["requires_control"]["states"], [2])
        self.assertTrue(flags["AAA_defeat_flag"]["conditions"][0].startswith("NOT ("))
        self.assertIn("on_capitulation", flags["AAA_victory_flag"]["called_from"][0])
        self.assertEqual(h["escalation"][0]["threshold"], "check_variable BBB.escalation > 4")
        hb = scenario.hooks(self.w, "BBB")
        self.assertEqual([g["id"] for g in hb["goals"]], ["BBB_defeat_aaa"])
        e = scenario.event(self.w, "aaa.5")
        self.assertEqual([o["name"] for o in e["options"]], ["aaa.5.a", "aaa.5.b"])
        self.assertEqual(e["options"][0]["text"], "Fine")
        self.assertEqual(e["options"][1]["ai_chance"], "base = 0")


class DossierAndCli(unittest.TestCase):
    def test_build_render_write(self):
        w = world()
        d = dossier.build_world(w)
        self.assertEqual(d["enemies"][0]["tag"], "BBB")
        self.assertEqual(d["enemies"][0]["oob"]["aggregate"]["divisions"], 1)
        self.assertNotIn("private", d["enemies"][0]["oob"])  # FAIR hides positions
        self.assertIn("private", dossier.build_world(world(mode="meta"))["enemies"][0]["oob"])
        md = dossier.render_md(d)
        for n in range(10):
            self.assertIn(f"\n## {n}. ", md)
        self.assertIn("limit 0.7 -> 11 VP", md)
        self.assertIn("Goal decision BBB_defeat_aaa", md)
        tmp = tempfile.mkdtemp()
        try:
            out = os.path.join(tmp, "dossier")
            dossier.write(d, out)
            for f in ("dossier.json", "dossier.md", "manifest.json"):
                self.assertTrue(os.path.isfile(os.path.join(out, f)))
            with open(os.path.join(out, "dossier.md")) as fh:
                md_text = fh.read()
            self.assertTrue(dossier.section_text(md_text, "4").startswith(
                "## 4. Scenario hooks"))
            # the CLI finds the run's dossier through HOI4_LIVE_RUN
            old = os.environ.get("HOI4_LIVE_RUN")
            os.environ["HOI4_LIVE_RUN"] = tmp
            try:
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    facts.main(["dossier", "show", "3", "--game", GAME])
                self.assertIn("## 3. Neighbours and enemies", buf.getvalue())
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    facts.main(["vp", "BBB", "--game", GAME, "--json"])
                self.assertEqual(json.loads(buf.getvalue())["total"], 15)
                buf = io.StringIO()
                with contextlib.redirect_stdout(buf):
                    facts.main(["oob", "BBB", "--game", GAME])  # tag AAA from the manifest: FAIR aggregate
                self.assertIn("FAIR aggregate", buf.getvalue())
            finally:
                if old is None:
                    os.environ.pop("HOI4_LIVE_RUN", None)
                else:
                    os.environ["HOI4_LIVE_RUN"] = old
        finally:
            shutil.rmtree(tmp)

    def test_cli_commands_run(self):
        cmds = [["focus", "AAA_left"], ["focus-tree", "AAA", "--depth", "1"],
                ["focus-timeline", "AAA", "AAA_root", "AAA_left"], ["tech", "t_c"],
                ["research-calendar", "AAA", "--years", "1936-1937"], ["state", "1"], ["states", "AAA"],
                ["vp", "BBB"], ["oob", "AAA"], ["characters", "AAA"], ["laws", "AAA"], ["idea", "bbb_spirit"],
                ["decisions", "BBB"], ["missions", "AAA"], ["hooks", "AAA"], ["event", "aaa.5"],
                ["grep", "AAA_conquer_days"], ["defines", "BASE_SURRENDER_LIMIT"]]
        for c in cmds:
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = facts.main(c + ["--game", GAME, "--tag", "AAA", "--dlc", "none", "--userdir", GAME])
            self.assertEqual(rc, 0, (c, buf.getvalue()))
            self.assertTrue(buf.getvalue().strip(), c)

    def test_split_dlcs(self):
        self.assertEqual(facts.split_dlcs("A;B, C"), ["A", "B, C"])
        self.assertEqual(facts.split_dlcs("A,B"), ["A", "B"])


class Calculators(unittest.TestCase):
    def setUp(self):
        self.w = world()

    def test_build_order_compounds_with_cap(self):
        # 40 civs, no consumer goods: three projects (15, 15, 10 civs), infrastructure 0
        r = calc.build_order(self.w, ["civ"] * 3, 40, 0, 0.0, "1936.1.1", infra=0)
        self.assertEqual(r["constants"]["project_cap"]["v"], 15)
        self.assertEqual(r["rows"][0]["date"], add_days("1936.1.1", 144))  # 10800 / (15 x 5) = 144 days
        self.assertEqual(r["rows"][-1]["civ"], 43)
        self.assertEqual(r["rows"][-1]["date"], add_days("1936.1.1", 192))  # 10 civs for 144 days, then 15: 7200 + 48 x 75
        # infrastructure 5 doubles the speed
        r5 = calc.build_order(self.w, ["civ"], 15, 0, 0.0, "1936.1.1", infra=5)
        self.assertEqual(r5["rows"][0]["date"], add_days("1936.1.1", 72))
        # consumer goods take ceil(share x all factories)
        rc = calc.build_order(self.w, ["mil"], 10, 10, 0.25, "1936.1.1", infra=0)
        self.assertEqual(rc["rows"][0]["date"], add_days("1936.1.1", 288))  # 7200 / (5 civs x 5)

    def test_build_order_uses_state_slots(self):
        r = calc.build_order(self.w, ["civ", "civ"], 15, 0, 0.0, "1936.1.1", slots=[[1, 1, 3]])
        self.assertEqual(r["rows"][0]["state"], 1)
        self.assertEqual(r["unfinished"][0]["blocked"], "no free slot")

    def test_build_order_says_when_infrastructure_is_a_guess(self):
        # no state and no --infra: level 0 is a guess, said in the result and on the row
        r = calc.build_order(self.w, calc.parse_order("supply_node:1", self.w), 15, 0, 0.0, "1936.1.1")
        self.assertEqual((r["rows"][0]["infra"], r["rows"][0]["infra_src"]), (0, "assumed"))
        self.assertEqual(r["rows"][0]["date"], add_days("1936.1.1", 267))  # 20000 / 75
        self.assertEqual(len(r["warnings"]), 1)
        self.assertIn("infrastructure 0 assumed for 1 project(s) with no state (supply_node)", r["warnings"][0])
        # an explicit level is no guess
        r = calc.build_order(self.w, ["supply_node"], 15, 0, 0.0, "1936.1.1", infra=0)
        self.assertEqual((r["warnings"], r["rows"][0]["infra_src"]), ([], "--infra"))

    def test_build_order_takes_infrastructure_from_the_named_state(self):
        # state 1 has infrastructure 3: 20000 / (75 x 1.6) = 167 days
        items = calc.parse_order("supply_node:1#1", self.w)
        self.assertEqual(items, [("supply_node", None, 1, 1)])
        r = calc.build_order(self.w, items, 15, 0, 0.0, "1936.1.1")
        self.assertEqual((r["rows"][0]["state"], r["rows"][0]["infra"], r["rows"][0]["infra_src"]), (1, 3, "state"))
        self.assertEqual(r["rows"][0]["date"], add_days("1936.1.1", 167))
        self.assertEqual(r["warnings"], [])
        # an infrastructure level finished there first speeds up what follows: 6000 / (75 x 1.6) = 50 days,
        # then 20000 / (75 x 1.8) = 149 days at level 4
        r = calc.build_order(self.w, calc.parse_order("infrastructure:1#1,supply_node#1", self.w), 15, 0, 0.0, "1936.1.1")
        self.assertEqual([(x["kind"], x["infra"], x["days"]) for x in r["rows"]],
                         [("infrastructure", 3, 50), ("supply_node", 4, 149)])
        # a factory in a named state takes one of its free slots (state 1 has 3)
        r = calc.build_order(self.w, calc.parse_order("civ:4#1", self.w), 15, 0, 0.0, "1936.1.1", until="1936.1.2")
        self.assertEqual([u.get("blocked") for u in r["unfinished"]], [None, None, None, "no free slot"])
        with self.assertRaises(ValueError):
            calc.build_order(self.w, calc.parse_order("civ:1#99", self.w), 15, 0, 0.0, "1936.1.1")

    def test_build_order_ranked_states_do_not_place_a_port(self):
        # the caller's own ranking gives the factories their states, but says nothing about where a
        # building without a slot stands
        slots = [[1, 3, 3]]
        r = calc.build_order(self.w, ["civ", "naval_base"], 15, 0, 0.0, "1936.1.1", slots=slots, slots_named=False)
        src = {x["kind"]: x["infra_src"] for x in r["rows"]}
        self.assertEqual(src, {"civ": "state", "naval_base": "assumed"})
        self.assertIn("naval_base take no building slot and no state was named", r["warnings"][0])
        named = calc.build_order(self.w, ["civ", "naval_base"], 15, 0, 0.0, "1936.1.1", slots=slots)
        self.assertEqual(named["warnings"], [])

    def test_build_order_command_prints_the_warning(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            calc.main(["build-order", "--game", GAME, "--civ", "15", "--mil", "0", "--cg", "0", "--order", "supply_node:1"])
        text = out.getvalue()
        self.assertTrue(text.startswith("WARNING: infrastructure 0 assumed"), text[:200])
        self.assertIn("infra 0 ASSUMED", text)
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            calc.main(["build-order", "--game", GAME, "--civ", "15", "--mil", "0", "--cg", "0", "--order", "supply_node:1#1"])
        self.assertNotIn("WARNING", out.getvalue())
        self.assertIn("state 1 infra 3 ->", out.getvalue())

    def test_consumer_goods_from_laws(self):
        self.assertEqual(calc.consumer_goods(self.w, "AAA")[0], 0.25)

    def test_equipment(self):
        r = calc.equipment(self.w, {"infantry": 2}, ["artillery"], divisions=3, year=1936)
        rows = {x["equipment"]: x for x in r["rows"]}
        self.assertEqual(rows["infantry_equipment"]["total"], 600)
        self.assertEqual(rows["infantry_equipment"]["variant"], "infantry_equipment_1")
        self.assertEqual(rows["artillery_equipment"]["ic_each"], 3)  # inherited from the archetype
        self.assertEqual(r["manpower_total"], 7500)

    def test_pp(self):
        # the fixture has no gain_focus modifier: no focus cost, 2 PP a day
        r = calc.pp(self.w, 0, 0, 0, "1936.1.1", "1936.6.1", [("1936.1.1", 150, "law")])
        self.assertEqual(r["focus_cost"], 0.0)
        self.assertEqual(r["rows"][0]["date"], add_days("1936.1.1", 75))
        self.assertEqual(r["rows"][0]["late_days"], 75)

    def test_pp_subtracts_the_running_focus(self):
        # a running focus costs 1 PP a day: the 150 PP law waits 150 days, not 75
        r = calc.pp(self.w, 0, 0, 0, "1936.1.1", "1936.8.1", [("1936.1.1", 150, "law")], focus_cost=1.0)
        self.assertEqual(r["per_day"], 1.0)
        self.assertEqual(r["rows"][0]["late_days"], 150)
        # the focus path ends on 1 March: 2 a day after it
        r = calc.pp(self.w, 0, 0, 0, "1936.1.1", "1936.8.1", [("1936.1.1", 150, "law")], focus_cost=1.0,
                    focus_until="1936.3.1")
        self.assertEqual(r["rows"][0]["late_days"], 59 + (150 - 59) // 2 + 1)
        # stability above 50% raises the factor, below lowers it (fixture: no stability modifiers)
        self.assertEqual(calc.pp_rates(self.w, stability=0.8)["stability_factor"], 0.0)


if __name__ == "__main__":
    unittest.main()
