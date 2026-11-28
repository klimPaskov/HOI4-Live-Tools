"""evaluators/value.py: the Python weights equal native/src/pick_value.rs WEIGHTS (parsed), held and compounding."""
import os
import re
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluators_fixture  # noqa: E402,F401  (puts scripts/ on the path)

from hoi4facts.evaluators import value  # noqa: E402

RUST = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "native", "src", "pick_value.rs")


def rust_weights():
    text = open(RUST, encoding="utf-8").read()
    body = text[text.index("const WEIGHTS"):]
    body = body[:body.index("];")]
    body = re.sub(r"//[^\n]*", "", body)
    return {k: (b, float(w)) for k, b, w in re.findall(r'\(\s*"([a-z_]+)",\s*Base::(\w+),\s*(-?[\d.]+),?\s*\)', body)}


class ValueTest(unittest.TestCase):
    def test_weights_match_the_rust_table(self):
        self.assertEqual(value.WEIGHTS, rust_weights())

    def test_held_and_compounding(self):
        base = {"Military": 20.0, "Civilian": 30.0, "All": 50.0, "Dockyards": 0.0, "Slots": 3.0}
        v, parts, unvalued = value.held({"industrial_capacity_factory": 0.1, "soft_attack": 0.1}, base, 2)
        self.assertAlmostEqual(v, 0.1 * 20 * 2)
        self.assertEqual(unvalued, ["soft_attack"])
        v2, _, _ = value.compounding({"production_speed_buildings_factor": 0.1}, base, 2, rate=0.5)
        lin = 0.1 * 0.9 * 30 * 2
        self.assertAlmostEqual(v2, lin + 0.1 * 0.9 * 30 * 0.5 * 4 / 2)
        v3, _, _ = value.held({"stability_weekly": 0.001}, base, 1)
        self.assertAlmostEqual(v3, 0.001 * 52 * 0.2 * 50)


if __name__ == "__main__":
    unittest.main()
