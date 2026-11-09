"""The plan schema (skills/hoi4-campaign-playbook/plan.schema.json) stays in step with the executor.

(a) every plan key native/src/plan_run.rs (and the alarms it calls) reads is in the schema, tagged
    x-owner "executor", and no executor-tagged key is unread;
(b) the schema uses only the draft-7 subset scripts/hoi4facts/schema_check.py implements;
(c) the hand plans' violations are reported on stderr, not failed (they are being fixed elsewhere).
"""

import json
import os
import re
import sys
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import schema_check  # noqa: E402

SCHEMA_PATH = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")
PLAN_RUN = os.path.join(ROOT, "native", "src", "plan_run.rs")
ALARMS = os.path.join(ROOT, "native", "src", "alarms.rs")
HAND_PLANS = [
    os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", name)
    for name in ("italy-ethiopia-1936.plan.json", "ethiopia-italy-1936.plan.json")
]

# Per-item fields: list key -> (function that walks the list, loop variable names there).
ENTRY_VARS = {
    "politics": ("tick", ["pick"]),
    "production": ("tick", ["line"]),
    "research_path": ("tick", ["step"]),
    "construction": ("construction_step", ["job"]),
    "construction_plan": ("construction_plan_step", ["item"]),
    "army_layout": ("layout_step", ["e", "x"]),
    "doctrines": ("extras_step", ["entry"]),
    "vp_targets": ("war_step", ["t"]),
}
# Fields the executor still reads that the schema deliberately leaves out.
KNOWN_GAPS = {
    "doctrines[].doctrine": "old {doctrine, track} entry, still read for one release; plans use the new "
                            "shape {id, folder, grand_doctrine?, track?, subdoctrine?, xp}",
}


def _code(path):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    text = text.split("#[cfg(test)]", 1)[0]
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def _functions(code):
    """{function name: body} (a body runs to the next top-level fn)."""
    parts = re.split(r"^(?:pub )?fn (\w+)", code, flags=re.M)
    return {parts[i]: parts[i + 1] for i in range(1, len(parts), 2)}


def consumed_keys():
    """Plan key paths the executor reads: "war", "war.enemies", "politics[].idea", ..."""
    keys = set()
    functions = _functions(_code(PLAN_RUN))
    for body in functions.values():
        for k in re.findall(r'\bplan\["(\w+)"\]', body):
            keys.add(k)
        for k in re.findall(r'\bplan\.get\("(\w+)"\)', body):
            keys.add(k)
        for k in re.findall(r'\b(?:strings|list)\(&?plan, "(\w+)"\)', body):
            keys.add(k)
        for k in re.findall(r'\bskip_hours\(plan, "(\w+)"', body):
            keys.update(("skip_hours", "skip_hours." + k))
        for a, b in re.findall(r'\bplan\["(\w+)"\]\["(\w+)"\]', body):
            keys.add(a + "." + b)
        for a, b in re.findall(r'\b(?:strings|list)\(&plan\["(\w+)"\], "(\w+)"\)', body):
            keys.add(a + "." + b)
        for var, top in re.findall(r'let (\w+) = &plan\["(\w+)"\];', body):
            for k in re.findall(r"\b%s\[\"(\w+)\"\]" % var, body):
                keys.add(top + "." + k)
            for k in re.findall(r"\b%s\.get\(\"(\w+)\"\)" % var, body):
                keys.add(top + "." + k)
            for k in re.findall(r"\b(?:strings|list)\(%s, \"(\w+)\"\)" % var, body):
                keys.add(top + "." + k)
    for top, (fn, names) in ENTRY_VARS.items():
        body = functions.get(fn, "")
        if not re.search(r'plan, "%s"\)|plan\["%s"\]' % (top, top), body):
            raise AssertionError("ENTRY_VARS is stale: %s no longer walks %s" % (fn, top))
        for var in names:
            for k in re.findall(r"\b%s\[\"(\w+)\"\]" % var, body):
                keys.add("%s[].%s" % (top, k))
            for k in re.findall(r"\b(?:strings|list)\(%s, \"(\w+)\"\)" % var, body):
                keys.add("%s[].%s" % (top, k))
            if re.search(r"\bitem_id\(&?%s\)" % var, body):
                keys.add("%s[].id" % top)
            if re.search(r"\breached\(%s\b" % var, body):
                keys.add("%s[].from" % top)
    alarms = _code(ALARMS)
    for k in re.findall(r'\bplan(?:\.get\(|\[)"(\w+)"', alarms):
        keys.add(k)
    for a, b in re.findall(r'\bplan\["(\w+)"\]\["(\w+)"\]', alarms):
        keys.add(a + "." + b)
    for k in re.findall(r'\b(?:enabled|threshold)\(config, "(\w+)"', alarms):
        keys.add("alarms." + k)
    return keys


def _load_schema():
    with open(SCHEMA_PATH, encoding="utf-8") as f:
        return json.load(f)


def _deref(node, schema):
    while "$ref" in node:
        node = schema_check._resolve(node["$ref"], schema)
    return node


def _lookup(schema, path):
    """The schema node for a key path ("war.enemies", "politics[].idea"), or None."""
    node = schema
    for part in path.split("."):
        node = _deref(node, schema)
        if part.endswith("[]"):
            part = part[:-2]
            item = _deref(node, schema).get("properties", {}).get(part)
            if item is None:
                return None
            node = _deref(item, schema).get("items")
            if node is None:
                return None
            continue
        props = node.get("properties", {})
        if part not in props:
            return None
        node = props[part]
    return node


class PlanSchemaCoverage(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = _load_schema()
        cls.keys = consumed_keys()

    def test_grep_finds_the_known_executor_keys(self):
        # a guard against the grep silently matching nothing
        for k in ("war", "war.enemies", "war.guard_states", "construction_plan[].order", "politics[].idea",
                  "army_layout[].templates", "research_path[].techs", "skip_hours.focus_blocked",
                  "stop_on_date", "alarms.political_power", "production[].from", "politics[].id",
                  "construction_plan[].id", "doctrines[].id", "doctrines[].subdoctrine", "_meta.campaign"):
            self.assertIn(k, self.keys)
        self.assertGreater(len(self.keys), 100)

    def test_every_consumed_key_is_in_the_schema(self):
        missing = sorted(k for k in self.keys if k not in KNOWN_GAPS and _lookup(self.schema, k) is None)
        self.assertEqual(missing, [], "plan keys read by plan_run.rs but missing from plan.schema.json")

    def test_consumed_keys_are_tagged_executor(self):
        wrong = []
        for k in sorted(self.keys):
            node = _lookup(self.schema, k)
            if node is not None and node.get("x-owner") == "player":
                wrong.append(k)
        self.assertEqual(wrong, [], "the executor reads these, so they cannot be x-owner player")

    def test_every_top_level_key_has_an_owner(self):
        for key, node in self.schema["properties"].items():
            self.assertIn(node.get("x-owner"), ("executor", "player"), key)

    def test_executor_tagged_keys_are_read(self):
        top = {k.split(".")[0].split("[")[0] for k in self.keys}
        unread = sorted(k for k, o in schema_check.owners(self.schema).items() if o == "executor" and k not in top)
        self.assertEqual(unread, [], "tagged executor but plan_run.rs never reads them")

    def test_new_keys_exist(self):
        props = self.schema["properties"]
        for k in ("goal", "checkpoints", "phases", "contingencies", "event_rules", "decision_rules", "_meta"):
            self.assertIn(k, props)
        goal = _deref(props["goal"], self.schema)
        self.assertEqual(set(goal["properties"]) >= {"predicates", "deadline", "fail_predicates"}, True)
        meta = props["_meta"]["properties"]
        self.assertEqual(set(meta) >= {"version", "parent", "dossier_hash", "models", "created", "lint_summary"}, True)
        doc = props["doctrines"]["items"]
        self.assertEqual(set(doc["properties"]) - {"why", "note"},
                         {"id", "folder", "grand_doctrine", "track", "subdoctrine", "xp"})

    def test_done_memory_lists_have_a_required_id(self):
        for k in ("construction_plan", "politics", "doctrines", "decision_rules"):
            items = _deref(_deref(self.schema["properties"][k], self.schema)["items"], self.schema)
            self.assertIn("id", items["required"], k)


class SchemaShape(unittest.TestCase):
    def test_schema_uses_the_supported_subset(self):
        self.assertEqual(schema_check.check_schema(_load_schema()), [])

    def test_check_schema_flags_problems(self):
        bad = {"$schema": "http://json-schema.org/draft-07/schema#", "type": "object",
               "properties": {"a": {"type": "strng"}, "b": {"$ref": "#/definitions/none"},
                              "c": {"pattern": "("}, "d": {"oneOff": []}},
               "required": ["z"], "additionalProperties": False}
        problems = "\n".join(schema_check.check_schema(bad))
        for word in ("unknown type", "does not resolve", "bad pattern", "unsupported keyword", "required 'z'"):
            self.assertIn(word, problems)


class Validator(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.schema = _load_schema()

    def errors(self, plan):
        return schema_check.validate(plan, self.schema)

    def test_minimal_plans_pass(self):
        self.assertEqual(self.errors({}), [])
        plan = {
            "_meta": {"version": 1, "parent": None, "dossier_hash": "abc", "created": "2026-11-09T10:00:00Z",
                      "models": {"strategist": {"provider": "claude", "model": "sonnet", "effort": "xhigh"}},
                      "lint_summary": {"errors": 0, "warnings": 2}},
            "_validator": {"tag": "ITA", "anything": [1, 2]},
            "active": True, "stop_on_date": {"year": 1937, "month": 1, "day": 1},
            "doctrines": [{"id": "d1", "folder": "land", "track": "infantry", "subdoctrine": "assault_infantry",
                           "xp": 100}],
            "politics": [{"id": "war_economy", "category": "government", "slot": 2, "idea": "war_economy",
                          "cost": 150}],
            "construction_plan": [{"id": "hub-2020", "order": 1, "building": "supply_node", "state": 559,
                                   "province": 2020, "when": "1936.01.01"}],
            "war": {"enemies": ["ETH"], "axes": [[842, 843, 271]], "guards": {"Army 4": 844}},
            "goal": {"predicates": [{"all": ["capitulated:ETH", "state_controller:271=ITA"]}],
                     "deadline": "1936.12.22", "fail_predicates": ["not_at_war:ETH"]},
            "checkpoints": [{"id": "tigray", "by": "1936.03.01", "must": ["state_fully_controlled:842"],
                             "else": "revise"}],
            "phases": [{"id": "P2", "enter": "1936.01.16", "exit": {"not": "vp_share:ETH>0.5"},
                        "overrides": {"war": {"axes": [[842]]}}}],
            "contingencies": [{"id": "asmara", "trigger": "province_controller:13250=ETH", "phase": "P1"}],
            "event_rules": [{"event": "BBA_italo_ethiopian_war_events.01",
                             "option_name": "BBA_italo_ethiopian_war_events.01.b"}],
            "event_prefix_options": {"DOD_": "last", "X_": 1},
        }
        self.assertEqual(self.errors(plan), [])

    def test_unknown_top_level_keys_fail_except_underscore(self):
        self.assertEqual(self.errors({"_note": "x"}), [])
        self.assertEqual(self.errors({"surprise": 1}), [("surprise", "unknown key 'surprise'")])

    def test_old_doctrine_shape_fails(self):
        paths = [p for p, _ in self.errors({"doctrines": [{"folder": "land", "track": "infantry",
                                                            "subdoctrine": "x", "when": "army_xp >= 100"}]})]
        self.assertIn("doctrines[0].when", paths)
        self.assertEqual(paths.count("doctrines[0]"), 2)  # id and xp missing

    def test_types_and_patterns(self):
        e = dict(self.errors({"active": 1, "war": {"enemies": ["Ethiopia"], "first": "no"},
                              "army_layout": [{"name_hint": "A", "state": 1, "templates": [], "role": "hold"}],
                              "phases": [{"id": "p", "exit": "Ethiopia capitulated"}],
                              "contingencies": [{"id": "c", "trigger": "1936.1.1"}],
                              "auto_events": 1}))
        self.assertIn("active", e)
        self.assertIn("war.enemies[0]", e)
        self.assertIn("war.first", e)
        self.assertIn("army_layout[0].role", e)
        self.assertIn("phases[0].exit", e)
        self.assertIn("anyOf", e["contingencies[0]"])
        self.assertIn("auto_events", e)

    def test_subset_semantics(self):
        v = schema_check.validate
        self.assertEqual(v(True, {"type": "integer"}), [("$", "expected integer, got boolean")])
        self.assertEqual(v(3.0, {"type": "integer"}), [])
        self.assertNotEqual(v(1, {"enum": [True]}), [])
        self.assertEqual(v("a", {"oneOf": [{"type": "string"}, {"type": "integer"}]}), [])
        self.assertNotEqual(v("a", {"oneOf": [{"type": "string"}, {"pattern": "a"}]}), [])
        self.assertNotEqual(v(1, {"not": {"type": "integer"}}), [])
        self.assertEqual(v({"x": 1}, {"additionalProperties": {"type": "integer"}}), [])
        self.assertNotEqual(v({"x": "1"}, {"additionalProperties": {"type": "integer"}}), [])
        self.assertEqual(schema_check.by_top_key([("a[0].b", "m"), ("a", "n"), ("c.d", "o")]),
                         {"a": [("a[0].b", "m"), ("a", "n")], "c": [("c.d", "o")]})


class HandPlanReport(unittest.TestCase):
    """Reports, never fails: the hand plans are being migrated to the schema in parallel."""

    def test_report_hand_plan_violations(self):
        schema = _load_schema()
        lines = []
        for path in HAND_PLANS:
            if not os.path.exists(path):
                continue
            with open(path, encoding="utf-8") as f:
                errors = schema_check.validate(json.load(f), schema)
            grouped = schema_check.by_top_key(errors)
            lines.append("%s: %d violation(s) in %s" % (
                os.path.basename(path), len(errors), ", ".join("%s(%d)" % (k, len(v)) for k, v in sorted(grouped.items()))
                or "none"))
        if lines:
            sys.stderr.write("\nplan.schema.json hand-plan report:\n  " + "\n  ".join(lines) + "\n")


if __name__ == "__main__":
    unittest.main()
