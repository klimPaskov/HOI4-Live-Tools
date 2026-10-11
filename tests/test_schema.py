"""Argument validation: types, enums, ranges, required, unknown fields, defaults, nested paths, and every
registered tool schema plus its documented examples."""

import unittest

import helpers  # noqa: F401

from hoi4_live import tools  # noqa: F401
from hoi4_live.errors import ValidationError
from hoi4_live.protocol.schema import check_schema, validate
from hoi4_live.tools import registry

SCHEMA = {
    "type": "object", "additionalProperties": False,
    "properties": {
        "action": {"type": "string", "enum": ["a", "b"], "default": "a"},
        "n": {"type": "integer", "minimum": 1, "maximum": 5},
        "x": {"type": "number", "exclusiveMinimum": 0},
        "name": {"type": "string", "pattern": "^[a-z]+$", "minLength": 2},
        "region": {"type": "array", "items": {"type": "integer", "minimum": 0}, "minItems": 4, "maxItems": 4},
        "steps": {"type": "array", "items": {"type": "object", "properties": {"tool": {"type": "string"}},
                                              "required": ["tool"]}},
        "flag": {"type": "boolean", "default": False},
    },
    "required": ["n"],
}


class SchemaTests(unittest.TestCase):
    def fail_field(self, value, field):
        with self.assertRaises(ValidationError) as cm:
            validate(SCHEMA, value)
        self.assertEqual(cm.exception.field, field)
        self.assertIn(field, str(cm.exception))

    def test_defaults_applied(self):
        self.assertEqual(validate(SCHEMA, {"n": 2}), {"n": 2, "action": "a", "flag": False})

    def test_required(self):
        self.fail_field({}, "arguments.n")

    def test_type(self):
        self.fail_field({"n": "2"}, "arguments.n")
        self.fail_field({"n": True}, "arguments.n")         # booleans are not integers
        self.fail_field({"n": 2, "flag": "yes"}, "arguments.flag")

    def test_range_and_enum(self):
        self.fail_field({"n": 9}, "arguments.n")
        self.fail_field({"n": 0}, "arguments.n")
        self.fail_field({"n": 1, "x": 0}, "arguments.x")
        self.fail_field({"n": 1, "action": "c"}, "arguments.action")

    def test_string_rules(self):
        self.fail_field({"n": 1, "name": "A1"}, "arguments.name")
        self.fail_field({"n": 1, "name": "a"}, "arguments.name")

    def test_unknown_field(self):
        with self.assertRaises(ValidationError) as cm:
            validate(SCHEMA, {"n": 1, "colour": "red"})
        self.assertEqual(cm.exception.field, "arguments.colour")
        self.assertIn("allowed", str(cm.exception))

    def test_nested_paths(self):
        self.fail_field({"n": 1, "region": [0, 0, 10]}, "arguments.region")
        self.fail_field({"n": 1, "region": [0, -1, 10, 10]}, "arguments.region[1]")
        self.fail_field({"n": 1, "steps": [{"tool": "a"}, {}]}, "arguments.steps[1].tool")

    def test_number_accepts_int(self):
        self.assertEqual(validate(SCHEMA, {"n": 1, "x": 2})["x"], 2)

    def test_check_schema_rejects_typos(self):
        with self.assertRaises(ValueError):
            check_schema({"type": "object", "properties": {"a": {"typ": "string"}}})
        with self.assertRaises(ValueError):
            check_schema({"type": "object", "properties": {}, "required": ["missing"]})
        with self.assertRaises(ValueError):
            check_schema({"type": "object", "properties": {"a": {"type": "integer", "default": "x"}}})

    def test_registered_tools_and_examples(self):
        names = set()
        for t in registry.all_tools():
            check_schema(t.schema, t.name)
            self.assertNotIn(t.name, names)
            names.add(t.name)
            self.assertTrue(t.name.startswith("hoi4_"))
            self.assertTrue(len(t.description) > 40, t.name)
            self.assertFalse(t.schema.get("additionalProperties", True), f"{t.name} must reject unknown args")
            for ex in t.examples:
                validate(t.schema, ex.get("args", {}), f"{t.name}.example")


if __name__ == "__main__":
    unittest.main()
