"""hoi4-live play: the plan chosen for a country, the doctor gate and what each system can start."""

import datetime
import io
import json
import os
import shutil
import tempfile
import unittest

import helpers

from hoi4_live import doctor, firstrun
from test_doctor import facts


class PlanTests(unittest.TestCase):
    def test_shipped_plans_are_indexed_from_their_own_files(self):
        plans = firstrun.shipped_plans(helpers.ROOT)
        by_tag = {p["tag"]: p for p in plans}
        self.assertTrue({"ITA", "GER", "CHI", "POL", "ETH"} <= set(by_tag))
        self.assertEqual(by_tag["ITA"]["name"], "Italy")
        self.assertEqual(by_tag["POL"]["mode"], "meta")
        self.assertEqual(by_tag["GER"]["mode"], "fair")
        self.assertTrue(by_tag["GER"]["guide"].endswith(".md"))
        self.assertEqual(firstrun.shipped_plans(os.path.join(helpers.ROOT, "absent")), [])

    def test_a_country_is_found_by_tag_or_name_and_a_named_mode_must_match(self):
        plans = firstrun.shipped_plans(helpers.ROOT)
        for word in ("ITA", "ita", "Italy", " italy "):
            self.assertEqual(firstrun.find_plan(plans, word)["tag"], "ITA", word)
        self.assertIsNone(firstrun.find_plan(plans, "SWE"))
        self.assertIsNone(firstrun.find_plan(plans, "POL", "fair"))
        self.assertEqual(firstrun.find_plan(plans, "POL", "meta")["tag"], "POL")
        both = [dict(plans[0], tag="AAA", name="A", mode="meta"), dict(plans[0], tag="AAA", name="A", mode="fair")]
        self.assertEqual(firstrun.find_plan(both, "AAA")["mode"], "fair")


class StartPathTests(unittest.TestCase):
    def test_each_system_says_what_can_start_today(self):
        for system, runtime, can, native in (("windows", "Python server", True, False),
                                             ("macos", "native runtime", True, True),
                                             ("linux", "native runtime", False, False)):
            f = facts(system)
            path = firstrun.start_path(system, f, doctor.evaluate(f))
            self.assertEqual((path["runtime"], path["ready"], path["can_start"], path["native"]),
                             (runtime, True, can, native), system)
            self.assertEqual(bool(path["missing"]), not native, system)
        f = facts("windows", game={"exe": None})
        self.assertFalse(firstrun.start_path("windows", f, doctor.evaluate(f))["ready"])
        # A Mac without the native runtime (Intel) keeps the Python server.
        f = facts("macos", native={"expected": False, "installed": False}, memory=None, builds=None)
        self.assertEqual(firstrun.start_path("macos", f, doctor.evaluate(f))["runtime"], "Python server")


class PlayTests(unittest.TestCase):
    def setUp(self):
        self.prefix = tempfile.mkdtemp(prefix="hoi4live_play_")
        self.addCleanup(shutil.rmtree, self.prefix, True)
        for part in ("plans", "briefs"):
            shutil.copytree(os.path.join(helpers.ROOT, "skills", "hoi4-campaign-playbook", part),
                            os.path.join(self.prefix, "app", "skills", "hoi4-campaign-playbook", part))

    def play(self, country, goal, found, mode=None, **options):
        out = io.StringIO()
        self.started = []

        def start_native(prefix, plan, goal, facts, out, template, **kwargs):
            self.started.append((plan, goal, template, kwargs))
            out.write("native start\n")
            return 0
        code = firstrun.play(self.prefix, country, goal, mode, gather=lambda _prefix: found, out=out,
                             now=datetime.datetime(2026, 11, 14, 12, 0, 0), start_native=start_native, **options)
        return code, out.getvalue()

    def test_a_ready_windows_install_gets_a_run_folder_and_one_instruction(self):
        code, text = self.play("Italy", "conquer Ethiopia before 1937", facts("windows"))
        self.assertEqual(code, firstrun.EXIT_OK, text)
        run = os.path.join(self.prefix, "runs", "20261114-120000-ITA")
        with open(os.path.join(run, "request.json"), encoding="utf-8") as f:
            request = json.load(f)
        self.assertEqual((request["tag"], request["goal"], request["mode"]),
                         ("ITA", "conquer Ethiopia before 1937", "fair"))
        self.assertTrue(os.path.isfile(os.path.join(run, "plan.json")))
        with open(os.path.join(run, "prompt.txt"), encoding="utf-8") as f:
            prompt = f.read()
        # A country the Select Country screen does not list by name is clicked on the map: no name list is assumed.
        for word in ("Italy (ITA)", "conquer Ethiopia before 1937", "hoi4_launch", "hoi4_new_game", "FAIR", run,
                     '"bookmark": "1936"', "click Italy on the map"):
            self.assertIn(word, prompt)
        self.assertIn(prompt.strip(), text)
        self.assertIn("no Windows adapter yet", text)

    def test_steps_to_do_stop_the_start_and_are_listed_for_the_runtime_that_plays(self):
        found = facts("macos", accessibility=False, screen_capture=False)
        code, text = self.play("ITA", "win", found)
        self.assertEqual(code, firstrun.EXIT_DOCTOR)
        self.assertIn("Privacy & Security > Accessibility", text)
        self.assertNotIn("Screen & System Audio Recording", text)   # a server-only step does not block native play
        self.assertFalse(os.path.exists(os.path.join(self.prefix, "runs")))
        self.assertEqual(self.started, [])

    def test_a_ready_mac_attaches_through_the_native_start(self):
        code, text = self.play("ITA", "conquer Ethiopia", facts("macos"), launch=True, wait_minutes=3)
        self.assertEqual(code, 0, text)
        plan, goal, template, options = self.started[0]
        self.assertEqual((plan["tag"], goal), ("ITA", "conquer Ethiopia"))
        self.assertIn("{PLAN_BRIEF}", template)
        self.assertEqual((options["launch"], options["wait_minutes"]), (True, 3))
        self.assertIn("native start", text)

    def test_a_strategist_folder_is_played_instead_of_a_shipped_plan(self):
        folder = os.path.join(self.prefix, "strat")
        os.makedirs(folder)
        with open(os.path.join(folder, "run_spec.json"), "w", encoding="utf-8") as f:
            json.dump({"tag": "SWE", "goal": "x", "mode": "fair", "date": "1936.1.1", "horizon": "1938.1.1"}, f)
        with open(os.path.join(folder, "plan.json"), "w", encoding="utf-8") as f:
            json.dump({"focus_path": []}, f)
        open(os.path.join(folder, "plan.brief.md"), "w").close()
        code, text = self.play("SWE", "win", facts("macos"), plan_folder=folder)
        self.assertEqual(code, 0, text)
        plan = self.started[0][0]
        self.assertEqual((plan["tag"], plan["stop"]), ("SWE", "1938.1.1"))
        self.assertTrue(plan["brief"].endswith("plan.brief.md"))
        self.assertIn("strategist plan strat", text)
        code, text = self.play("NOR", "win", facts("macos"), plan_folder=folder)
        self.assertEqual(code, firstrun.EXIT_USAGE)
        code, text = self.play("SWE", "win", facts("macos"), plan_folder=os.path.join(self.prefix, "absent"))
        self.assertEqual(code, firstrun.EXIT_USAGE)

    def test_linux_is_told_which_piece_is_missing(self):
        code, text = self.play("GER", "win", facts("linux"))
        self.assertEqual(code, firstrun.EXIT_NO_START)
        self.assertIn("cannot attach", text)
        self.assertFalse(os.path.exists(os.path.join(self.prefix, "runs")))

    def test_a_country_without_a_plan_lists_the_campaigns_that_have_one(self):
        code, text = self.play("Sweden", "win", facts("windows"))
        self.assertEqual(code, firstrun.EXIT_USAGE)
        self.assertIn("Italy (ITA, FAIR", text)
        self.assertIn("strategist", text)
        self.assertIn("--plan", text)
        self.assertIn("minutes", text)
        code, text = self.play("POL", "win", facts("windows"), mode="fair")
        self.assertEqual(code, firstrun.EXIT_USAGE)
        self.assertIn("FAIR mode", text)


if __name__ == "__main__":
    unittest.main()
