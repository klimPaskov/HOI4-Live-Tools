"""tool_schemas.py: the tool catalogue extracted from native/src, and call checks."""
import os
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import tool_schemas as ts  # noqa: E402


class ToolSchemaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tools = ts.extract()

    def test_catalogue_covers_the_tools_plans_use(self):
        self.assertGreater(len(self.tools), 140)
        for name in ("hoi4_production_set", "hoi4_army_commander", "hoi4_peace_take", "hoi4_occupation_law",
                     "hoi4_air_mission", "hoi4_template_create", "hoi4_resources"):
            self.assertIn(name, self.tools)

    @unittest.skipUnless(os.path.exists(ts.DEFAULT_BINARY), "native binary not built")
    def test_committed_catalogue_matches_the_binary(self):
        self.assertEqual(sorted(ts.load()), sorted(ts.from_binary()),
                         "run: python3 scripts/hoi4facts/tool_schemas.py build")

    def test_union_types(self):
        cat = {"t": {"inputSchema": {"properties": {"base": {"type": ["integer", "string"]}}}}}
        self.assertEqual(ts.check_call(cat, "t", {"base": 3}), [])
        self.assertEqual(ts.check_call(cat, "t", {"base": "x"}), [])
        self.assertTrue(ts.check_call(cat, "t", {"base": [1]}))

    def test_call_checks(self):
        self.assertIn("hoi4_production_set: missing required argument line",
                      ts.check_call(self.tools, "hoi4_production_set", {"equipment": "x", "factories": 0}))
        self.assertTrue(any("unknown argument commander" in p for p in
                            ts.check_call(self.tools, "hoi4_army_commander", {"army": "A", "commander": "B"})))
        self.assertEqual(ts.check_call(self.tools, "hoi4_peace_take", {"states": [271]}), [])
        self.assertEqual(ts.check_call(self.tools, "hoi4_nope", {}), ["unknown tool hoi4_nope"])
        # a schema written as a Rust value: only existence is checked
        self.assertEqual(ts.check_call(self.tools, "hoi4_resources", {"anything": 1}), [])
        # broker-filled arguments are never required of a plan
        self.assertFalse(any("request_id" in p or "lease_token" in p for p in
                             ts.check_call(self.tools, "hoi4_occupation_law", {"law": "x"})))

    def test_calls_in_text(self):
        found = ts.calls_in_text('do `hoi4_peace_take {"states":[1]}` then `hoi4_air_wings {}` and `hoi4_x {bad}`')
        self.assertEqual([(t, a) for t, a, _ in found], [("hoi4_peace_take", {"states": [1]}),
                                                         ("hoi4_air_wings", {}), ("hoi4_x", None)])


if __name__ == "__main__":
    unittest.main()
