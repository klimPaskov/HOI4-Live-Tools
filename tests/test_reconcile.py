"""reconcile.py: plan names checked against live read tools (fake broker; no game)."""
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCRIPT = os.path.join(ROOT, "scripts", "strategist", "reconcile.py")
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from strategist import reconcile  # noqa: E402

FAKE = textwrap.dedent('''
    import json, sys
    tool = sys.argv[1]
    bodies = {
        "hoi4_division_templates": {"templates": [{"template": "Divisione di Fanteria"},
                                                  {"template": "Camicie Nere"}]},
        "hoi4_commanders": {"commanders": [{"name": "Giovanni Messe"}, {"name": "Rodolfo Graziani"}]},
        "hoi4_air_wings": {"wings": [{"wing": "12° Stormo"}, {"wing": "14° Stormo"}]},
        "hoi4_air": {"air_bases": [{"state_id": 550, "wings": ["12\u00b0 Stormo", "14\u00b0 Stormo"]}],
                     "wings": [{"wing": "12\u00b0 Stormo", "variant": "CR.32"},
                               {"wing": "14\u00b0 Stormo", "variant": "Ca.111"}]},
        "hoi4_production_lines": {"lines": [{"equipment_id": "infantry_equipment_1",
                                             "factory_counters": {"active_factories_raw_signed": 4}}]},
        "hoi4_technology_list": {"technologies": ["basic_machine_tools", "construction1"]},
    }
    print(json.dumps({"isError": False, "body": bodies[tool]}))
''')


class ReconcileTests(unittest.TestCase):
    def run_it(self, plan, extra=()):
        d = tempfile.mkdtemp()
        with open(os.path.join(d, "plan.json"), "w", encoding="utf-8") as f:
            json.dump(plan, f)
        fake = os.path.join(d, "fake_broker.py")
        with open(fake, "w", encoding="utf-8") as f:
            f.write(FAKE)
        cmd = '"%s" "%s"' % (sys.executable, fake)
        p = subprocess.run([sys.executable, SCRIPT, "--run", d, "--call-cmd", cmd] + list(extra),
                           capture_output=True, text=True, encoding="utf-8")
        path = os.path.join(d, "reconcile.json")
        self.assertTrue(os.path.exists(path), p.stdout + p.stderr)
        report = json.load(open(path, encoding="utf-8"))
        return p.returncode, report, json.load(open(os.path.join(d, "plan.json"), encoding="utf-8")), d

    def test_one_to_one_variants_are_patched_and_the_rest_listed(self):
        plan = {"army_layout": [{"name_hint": "North", "templates": ["divisione di fanteria", "Ascari Eritrea"],
                                 "general": "Giovanni Messe"}],
                "air_plan": [{"wing": "12 Stormo", "mission": "cas"}],
                "research_path": [{"year": 1936, "techs": ["basic_machine_tools", "tech_engineers"]},
                                  {"year": 1937, "techs": ["construction2"]}]}
        code, report, patched, d = self.run_it(plan)
        self.assertEqual(code, 2)
        self.assertEqual(patched["air_plan"][0]["wing"], "12° Stormo")
        self.assertEqual(patched["army_layout"][0]["templates"][0], "Divisione di Fanteria")
        self.assertEqual([u["name"] for u in report["unresolved"]], ["Ascari Eritrea"])
        self.assertEqual(report["research"]["not_yet_available"], ["tech_engineers"])
        self.assertTrue(os.path.exists(os.path.join(d, "plan.pre-reconcile.json")))

    def test_clean_plan_exits_zero_and_dry_run_does_not_write(self):
        plan = {"army_layout": [{"templates": ["Camicie Nere"], "general": "rodolfo graziani"}]}
        code, report, patched, d = self.run_it(plan, ["--dry-run"])
        self.assertEqual(code, 0)
        self.assertEqual(report["patched"][0]["to"], "Rodolfo Graziani")
        self.assertEqual(patched["army_layout"][0]["general"], "rodolfo graziani")  # dry run
        self.assertFalse(os.path.exists(os.path.join(d, "plan.pre-reconcile.json")))

    def test_wing_selector_expands_to_the_live_wing(self):
        plan = {"air_plan": [{"wing": "CR.32 at 550", "mission": "air_superiority"},
                             {"wing": "SM.79 at 550", "mission": "cas"}]}
        code, report, patched, _ = self.run_it(plan)
        self.assertEqual(code, 2)
        self.assertEqual(patched["air_plan"][0]["wing"], "12\u00b0 Stormo")
        self.assertEqual(patched["air_plan"][0]["selector"], "CR.32 at 550")
        self.assertEqual([u["name"] for u in report["unresolved"]], ["SM.79 at 550"])

    def test_marshals_are_checked(self):
        plan = {"army_group": [{"id": "g", "armies": ["A"], "marshal": ["rodolfo graziani", "Pietro Badoglio"]}]}
        code, report, patched, _ = self.run_it(plan)
        self.assertEqual(patched["army_group"][0]["marshal"][0], "Rodolfo Graziani")
        self.assertEqual([u["name"] for u in report["unresolved"]], ["Pietro Badoglio"])

    def test_norm(self):
        self.assertEqual(reconcile.norm("12° Stormo"), reconcile.norm("12 stormo"))
        self.assertEqual(reconcile.norm("Divisione Ascari Libica"), "divisione ascari libica")
        self.assertNotEqual(reconcile.norm("1 Stormo"), reconcile.norm("12 Stormo"))


if __name__ == "__main__":
    unittest.main()
