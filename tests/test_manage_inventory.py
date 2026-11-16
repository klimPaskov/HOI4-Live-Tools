"""scripts/hoi4facts/manage_inventory.py and lint_manage.py: every item and alert has an owner."""

import os
import re
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import lint_manage, manage_inventory as inv, plan_keys  # noqa: E402

ALERTS_RS = os.path.join(ROOT, "native", "src", "launch", "macos", "alerts.rs")
MANAGE_RS = os.path.join(ROOT, "native", "src", "plan_manage.rs")


def rust(path):
    with open(path, encoding="utf-8") as fh:
        return fh.read()


class Mirror(unittest.TestCase):
    @unittest.skipUnless(os.path.isfile(ALERTS_RS), "native sources not here")
    def test_every_named_alert_is_in_the_inventory(self):
        body = rust(ALERTS_RS).split("fn label", 1)[1].split("pub(super) fn tool", 1)[0]
        names = set(re.findall(r'=> \(\s*"([a-z_]+)"', body))
        self.assertTrue(names)
        self.assertLessEqual(names, set(inv.ALERTS))

    @unittest.skipUnless(os.path.isfile(MANAGE_RS), "native sources not here")
    def test_alert_keys_follow_the_executor_routes(self):
        body = rust(MANAGE_RS).split("fn route(alert: &str)", 1)[1].split("\nfn ", 1)[0]
        for alert, (keys, _) in inv.ALERTS.items():
            if keys is None:
                continue
            m = re.search(r'"%s"[^=]*=>\s*Route::(Step|Wake|Info)\((.*?)\),?\n\s*(?:"|_)' % alert, body, re.S)
            if m is None:
                continue  # an alias arm ("a" | "b" => ...)
            if m.group(1) == "Wake":
                self.assertEqual(keys, [], alert)
            else:
                route_keys = re.findall(r'"([a-z_]+)"', m.group(2).split("&[", 2)[-1])
                self.assertTrue(set(keys) <= set(route_keys) | {"focus_fallback"}, (alert, keys, route_keys))

    def test_every_key_is_a_schema_key(self):
        schema = plan_keys.load_schema()
        for iid, _, _, keys, _, _ in inv.ITEMS:
            for k in keys:
                path = re.sub(r"\[[^\]]*\]", "", k)
                parts = path.split(".")
                tries = [".".join(parts)] + [".".join(parts[:i]) + "[]." + ".".join(parts[i:])
                                             for i in range(1, len(parts))]
                self.assertTrue(any(plan_keys.lookup(schema, t) is not None for t in tries), (iid, k))


class Owners(unittest.TestCase):
    def entry(self, iid):
        return next(e for e in inv.inventory({}) if e["id"] == iid)

    def test_key_rule_reason(self):
        self.assertEqual(inv.owners({"politics": [{"idea": "x"}]}, self.entry("political_power")), "key politics")
        self.assertIsNone(inv.owners({}, self.entry("political_power")))
        self.assertIn("rule", inv.owners({}, self.entry("convoys")))
        self.assertIsNone(inv.owners({"manage": {"off": ["convoys"]}}, self.entry("convoys")))
        self.assertIsNone(inv.owners({}, self.entry("officer_corps")))
        self.assertEqual(inv.owners({"manage": {"unowned_why": {"officer_corps": "brief line"}}},
                                    self.entry("officer_corps")), "reason")

    def test_filtered_keys(self):
        e = self.entry("call_allies")
        self.assertIsNone(inv.owners({"diplomacy": [{"action": "declare"}]}, e))
        self.assertIsNotNone(inv.owners({"diplomacy": [{"action": "call_allies", "target": "ITA"}]}, e))

    def test_alert_owned_through_its_item(self):
        e = self.entry("alert:mio_traits")
        self.assertIsNone(inv.owners({}, e))
        self.assertIn("mio", inv.owners({"mio": [{"organisation": "x"}]}, e))

    def test_presence_drops_items_the_country_lacks(self):
        ids = {e["id"] for e in inv.inventory({"fleets": False, "air_wings": False, "dockyards": False})}
        self.assertNotIn("fleets", ids)
        self.assertNotIn("air_experience", ids)
        self.assertNotIn("alert:admiral_assignable", ids)
        self.assertIn("political_power", ids)


class Lint(unittest.TestCase):
    class L:
        tag = "GER"
        world = None
        game = None

        def __init__(self, plan):
            self.plan, self.findings, self.facts = plan, [], {}

        def add(self, *f):
            self.findings.append(f)

    def test_unowned_items_fail(self):
        orig = inv.country_context
        inv.country_context = lambda *a: {}
        try:
            lt = self.L({})
            lint_manage.check(lt)
        finally:
            inv.country_context = orig
        rules = {f[1] for f in lt.findings}
        self.assertEqual(rules, {"manage.owner"})
        self.assertTrue(any("officer_corps" in f[2] for f in lt.findings))
        self.assertIn("manage_inventory", lt.facts)


if __name__ == "__main__":
    unittest.main()
