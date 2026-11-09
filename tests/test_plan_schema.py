"""The plan schema (skills/hoi4-campaign-playbook/plan.schema.json) stays in step with the executor.

(a) every plan key native/src/plan_run.rs and its child modules (plan_manage.rs, plan_military.rs,
    plan_predicate.rs once it lands) and the alarms read is in the schema, tagged x-owner "executor", and no executor-tagged key is
    unread (the code is grepped for plan["..."] and helper reads, comments stripped);
(b) the schema uses only the draft-7 subset scripts/hoi4facts/schema_check.py implements;
(c) the predicate grammar (pattern, x-grammar, x-atoms) agrees with hoi4facts/validate.py parse_predicate;
(d) the coverage-build keys (W1-2) have their shapes, owners and x-executor-planned build items;
(e) both hand plans pass the schema.
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
# The runner's child modules read plan keys too (plan_manage.rs: continuous management;
# plan_military.rs: army groups, navy, invasions, casualty and air superiority rules;
# plan_predicate.rs: the predicate evaluator; plan_affairs.rs: designs, diplomacy, faction, trade,
# MIOs, intelligence, research bonuses). Missing files are skipped.
EXECUTOR_SOURCES = [PLAN_RUN] + [os.path.join(ROOT, "native", "src", name)
                                 for name in ("plan_manage.rs", "plan_military.rs", "plan_predicate.rs",
                                              "plan_affairs.rs")]
ALARMS = os.path.join(ROOT, "native", "src", "alarms.rs")
HAND_PLANS = [
    os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", name)
    for name in ("italy-ethiopia-1936.plan.json", "ethiopia-italy-1936.plan.json")
]

# Per-item fields: list key -> (functions that read its entries, the first one walks the list;
# loop variable names there).
ENTRY_VARS = {
    "politics": (["tick"], ["pick"]),
    "production": (["tick", "wanted_switches"], ["line"]),
    "research_path": (["tick"], ["step"]),
    "construction": (["construction_step"], ["job"]),
    "construction_plan": (["construction_plan_step"], ["item"]),
    "army_layout": (["layout_step"], ["e", "x"]),
    "doctrines": (["extras_step"], ["entry"]),
    "vp_targets": (["war_step"], ["t"]),
    "training": (["training_step"], ["tr"]),
    "templates": (["templates_step"], ["entry"]),
    "air_plan": (["air_plan_step", "wing_matches", "rebase_target"], ["entry"]),
    "decision_rules": (["decisions_step", "decision_forbidden"], ["rule", "r"]),
    "event_rules": (["event_rule_choice"], ["rule"]),
    "designs": (["designs_step", "wanted_switches"], ["entry", "e"]),
    "diplomacy": (["diplomacy_step", "justify", "declare", "relation", "widening", "expiry_watch",
                   "justify_items", "dip_target"], ["item", "j", "o"]),
    "mio": (["mio_step"], ["entry"]),
    "trade": (["trade_step"], ["item"]),
}
# Fields the executor still reads that the schema deliberately leaves out.
KNOWN_GAPS = {
    "doctrines[].doctrine": "old {doctrine, track} entry, still read for one release; plans use the new "
                            "shape {id, folder, grand_doctrine?, track?, subdoctrine?, xp}",
    "templates[].regimental": "alias of regimental_support the templates step also skips; plans write "
                              "regimental_support",
}


def _code(path):
    with open(path, encoding="utf-8") as f:
        text = f.read()
    text = text.split("#[cfg(test)]", 1)[0]
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def _functions(code):
    """{function name: body} (a body runs to the next top-level fn)."""
    parts = re.split(r"^(?:pub(?:\(\w+\))? )?fn (\w+)", code, flags=re.M)
    return {parts[i]: parts[i + 1] for i in range(1, len(parts), 2)}


def executor_functions():
    """{function name: body} over plan_run.rs and its child modules (code only, comments and
    tests stripped; a name defined twice keeps both bodies)."""
    out = {}
    for path in EXECUTOR_SOURCES:
        if os.path.exists(path):
            for name, body in _functions(_code(path)).items():
                out[name] = out.get(name, "") + body
    return out


def consumed_keys():
    """Plan key paths the executor reads: "war", "war.enemies", "politics[].idea", ..."""
    keys = set()
    functions = executor_functions()
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
    for top, (fns, names) in ENTRY_VARS.items():
        if not re.search(r'plan, "%s"\)|plan\["%s"\]' % (top, top), functions.get(fns[0], "")):
            raise AssertionError("ENTRY_VARS is stale: %s no longer walks %s" % (fns[0], top))
        body = "".join(functions.get(fn, "") for fn in fns)
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


PRED_OK = [
    "1936.05.01", "date >= 1936.5.1", "date<1937.01.01", "province_controlled:2072", "state_fully_controlled:842",
    "owner:908=ITA", "owner:908 == ITA", "state_controller:271!=ETH", "province_controller:13250=ETH",
    "at_war:ETH", "not_at_war:ETH", "capitulated:ETH", "exists:AFA", "not_exists:AFA",
    "focus_completed:ETH_the_second_italo-ethiopian_war", "tech_researched:radio", "phase:P2_tigray",
    "at_peace_conference", "vp_share:ETH<0.5", "vp_share:ETH >= 0.25", "stability >= 0.5", "war_support>0.25",
    "pp > 150", "pp < -20", "manpower < 20000", "xp:army >= 100", "xp:navy==0", "fuel_ratio < 0.25",
    "convoys_free <= 10", "world_tension >= 0.4", "resistance:550 >= 50", "compliance:550 > 40",
    "equipment_fill:North strike (Army 1) < 0.8", "stockpile_days:infantry_equipment < 30",
    "var:ITA_escalation > 4", "var:ETH.war_level != 0", "bop:ETH_centralization_balance >= -0.5",
    "mission_days:ITA_missiolini_conquer_ethiopia < 15",
    # the spellings native/src/plan_predicate.rs reads today
    "at_war_with:ETH", "controls_state:550", "controller:550 == ITA", "focus_done:ITA_paris", "tech_done:radio",
    "war_support >= 25", "war_support >= 25%", "stability<50%", "date:1936.6.1", "xp:land > 5", "xp:naval < 9",
    "", "true", "false",
]
PRED_BAD = [  # rejected by the schema pattern and by validate.parse_atom
    "stability", "at_war:ETH == 1", "xp:sea > 5", "owner:908 > ITA", "foo > 1", " stability > 0.5",
    "stability > 0.5 ", "stability >> 0.5", "vp_share:Ethiopia>0.5", "resistance:Eritrea > 5", "phase:P-2",
    "stability = 0.5", "Ethiopia capitulated", "at_peace_conference:ETH", "equipment_fill: x < 0.8",
    "date >= soon", "manpower < 20k",
]
PRED_RANGE = [  # pattern-valid, rejected by the parser's ranges, percent rule and calendar
    "stability >= 150", "war_support > 101%", "resistance:550 >= 150", "bop:x < -2", "date >= 1936.13.01",
    "1936.02.31", "pp > 5%", "manpower < 10%",
]


class PredicateGrammar(unittest.TestCase):
    """definitions.predicate: the schema pattern, x-grammar / x-atoms and validate.parse_predicate agree."""

    @classmethod
    def setUpClass(cls):
        from hoi4facts import validate
        cls.v = validate
        cls.schema = _load_schema()
        cls.pred = cls.schema["definitions"]["predicate"]
        cls.table = validate.predicate_table(cls.schema)

    def schema_ok(self, p):
        return schema_check.validate(p, self.pred, self.schema) == []

    def test_doc_block_and_atom_table(self):
        self.assertEqual(list(self.pred)[0], "x-grammar")  # the doc block comes first
        grammar = "\n".join(self.pred["x-grammar"])
        for word in ("all", "any", "not", "<=", ">=", "==", "!=", "three-valued", "fraction", "percent"):
            self.assertIn(word, grammar)
        self.assertEqual(set(self.table) >= {
            "stability", "war_support", "pp", "manpower", "xp", "fuel_ratio", "convoys_free", "world_tension",
            "resistance", "compliance", "equipment_fill", "stockpile_days", "var", "bop", "mission_days",
            "at_peace_conference", "date", "province_controlled", "state_fully_controlled", "owner"}, True)
        self.assertEqual({k for k, a in self.table.items() if not a["readable"]},
                         {"convoys_free", "stockpile_days", "var", "bop", "mission_days"})
        for k in ("at_war_with", "controls_state", "controller", "focus_done", "tech_done", "exists"):
            self.assertTrue(self.table[k]["readable"], k)
        for name, atom in self.table.items():
            self.assertIn(atom["kind"], ("bool", "number", "tag", "date"), name)
            self.assertIn(atom["arg"], [None] + list(self.v.PRED_ARGS), name)

    def test_accepted(self):
        for p in PRED_OK:
            self.assertTrue(self.schema_ok(p), p)
            atom, why = self.v.parse_atom(p, self.table)
            self.assertIsNone(why, p)
        atom, _ = self.v.parse_atom("owner:908=ITA", self.table)
        self.assertEqual((atom["name"], atom["arg"], atom["op"], atom["value"]), ("owner", "908", "==", "ITA"))
        atom, _ = self.v.parse_atom("1936.5.1", self.table)
        self.assertEqual((atom["name"], atom["op"], atom["value"]), ("date", ">=", (1936, 5, 1)))
        atom, _ = self.v.parse_atom("equipment_fill:North strike (Army 1) < 0.8", self.table)
        self.assertEqual((atom["arg"], atom["op"], atom["value"]), ("North strike (Army 1)", "<", 0.8))
        for text in ("war_support >= 25", "war_support >= 25%", "war_support >= 0.25"):  # ratios: > 1 is a percent
            self.assertEqual(self.v.parse_atom(text, self.table)[0]["value"], 0.25, text)
        self.assertEqual(self.v.parse_atom("resistance:550 >= 50", self.table)[0]["value"], 0.5)
        self.assertEqual(self.v.parse_atom("pp > 150", self.table)[0]["value"], 150)
        self.assertEqual(self.v.parse_atom(False, self.table)[0]["value"], False)
        self.assertTrue(self.v.parse_atom("capitulated:ETH", self.table)[0]["readable"])
        self.assertFalse(self.v.parse_atom("bop:ETH_centralization > 0.5", self.table)[0]["readable"])

    def test_can_hold(self):
        can = lambda p: self.v.predicate_can_hold(p, self.table)  # noqa: E731
        self.assertEqual(can("at_war_with:ETH"), (True, True))
        self.assertEqual(can("capitulated:ETH"), (True, True))
        self.assertEqual(can("mission_days:m < 5"), (False, False))
        self.assertEqual(can(["at_war_with:ETH", "mission_days:m < 5"]), (False, True))  # all: one never true
        self.assertEqual(can({"any": ["vp_share:ETH < 0.2", "mission_days:m < 5"]}), (True, False))
        self.assertEqual(can({"not": "bop:x > 0"}), (False, False))
        self.assertEqual(can(False), (False, True))

    def test_rejected(self):
        for p in PRED_BAD:
            self.assertFalse(self.schema_ok(p), p)
            self.assertIsNotNone(self.v.parse_atom(p, self.table)[1], p)
        for p in PRED_RANGE:
            self.assertTrue(self.schema_ok(p), p)
            self.assertIsNotNone(self.v.parse_atom(p, self.table)[1], p)

    def test_every_atom_has_a_pattern_branch(self):
        sample = {None: None, "province": "12", "state": "550", "tag": "ITA", "key": "ITA_x.y-1", "id": "P1",
                  "branch": "army", "army": "Army 1", "equipment": "infantry_equipment", "name": "ITA.v"}
        for name, atom in self.table.items():
            head = name + (":" + sample[atom["arg"]] if atom["arg"] else "")
            text = {"bool": head, "number": head + " >= 0", "tag": head + " == ETH",
                    "date": head + " >= 1936.1.1"}[atom["kind"]]
            self.assertTrue(self.schema_ok(text), text)
            self.assertIsNone(self.v.parse_atom(text, self.table)[1], text)

    def test_composites(self):
        good = {"all": ["capitulated:ETH", {"any": ["stability >= 0.5", {"not": "at_war:ETH"}]}]}
        self.assertTrue(self.schema_ok(good))
        atoms, problems = self.v.parse_predicate(good, self.table, "goal")
        self.assertEqual(problems, [])
        self.assertEqual([p for p, _ in atoms], ["goal.all[0]", "goal.all[1].any[0]", "goal.all[1].any[1].not"])
        for bad in ({"all": []}, {"and": ["at_war:ETH"]}, {"all": ["at_war:ETH"], "any": ["at_war:ETH"]}, 3,
                    ["at_war:ETH", "foo"]):
            self.assertFalse(self.schema_ok(bad), bad)
            self.assertTrue(self.v.parse_predicate(bad, self.table)[1], bad)
        plain = ["at_war_with:ETH", {"not": "phase:P1"}]  # a plain list is all
        self.assertTrue(self.schema_ok(plain))
        self.assertEqual([p for p, _ in self.v.parse_predicate(plain, self.table, "x")[0]],
                         ["x.all[0]", "x.all[1].not"])
        self.assertEqual(self.v.predicate_dates({"all": ["1936.3.1", "date > 1936.4.1", {"not": "1937.1.1"}]},
                                                self.table), [(1936, 3, 1), (1936, 4, 1)])

    def test_instances_of_finds_every_predicate(self):
        plan = {"goal": {"predicates": ["capitulated:ETH"], "fail_predicates": ["not_at_war:ETH"]},
                "phases": [{"id": "P1", "enter": "1936.1.1", "exit": {"not": "at_war:ETH"}}],
                "contingencies": [{"id": "c", "trigger": "province_controller:13250=ETH", "phase": "P1"}],
                "politics": [{"id": "p", "category": "government", "idea": "x", "when": "war_support > 0.5"}],
                "air_plan": [{"region": 1, "mission": "cas", "switch_when": "province_controlled:5097"}],
                "templates": [{"name": "T", "regiments": ["infantry"], "when": "1936.6.1"}]}
        paths = [p for p, _ in schema_check.instances_of(plan, self.schema, "#/definitions/predicate")]
        self.assertEqual(sorted(paths), sorted([
            "goal.predicates[0]", "goal.fail_predicates[0]", "phases[0].enter", "phases[0].exit",
            "contingencies[0].trigger", "politics[0].when", "air_plan[0].switch_when", "templates[0].when"]))


class CoverageKeys(unittest.TestCase):
    """The coverage-build keys (W1-2) and the owners plan_manage.rs took over."""

    @classmethod
    def setUpClass(cls):
        cls.schema = _load_schema()
        cls.props = cls.schema["properties"]

    def test_new_keys_and_owners(self):
        owners = schema_check.owners(self.schema)
        for key in ("training", "training_rules", "templates", "occupation", "peace", "auto_import", "air_plan",
                    "manage", "goal", "phases", "checkpoints", "contingencies", "event_rules", "decision_rules"):
            self.assertEqual(owners[key], "executor", key)
        self.assertEqual(_lookup(self.schema, "politics[].when").get("x-owner"), "executor")
        alarms = self.schema["definitions"]["alarms"]["properties"]
        for k in ("goal_reached", "goal_at_risk", "goal_risk_days", "checkpoint_missed", "phase_changed",
                  "contingency_triggered"):
            self.assertIn(k, alarms)
        for key in ("force_posture", "xp_budget", "stockpile_targets", "sea_transfer", "garrison_template", "navy",
                    "intelligence", "designs", "mio", "trade", "_coverage"):
            self.assertEqual(owners[key], "player", key)
        planned = schema_check.planned(self.schema)
        for key, item in (("xp_budget", "W1-6"), ("stockpile_targets", "W1-10"), ("sea_transfer", "W1-14"),
                          ("garrison_template", "W1-7"), ("navy", "W2-4"), ("occupation.per_state", "W1-7"),
                          ("production[].remove", "W1-9"), ("hold_targets", "W1-15"), ("manage.fuel_floor", "W3-10")):
            self.assertEqual(planned.get(key), item, key)

    def test_planned_marks_are_build_items_on_non_executor_nodes(self):
        planned = schema_check.planned(self.schema)
        self.assertGreater(len(planned), 15)
        for path, item in planned.items():
            self.assertRegex(item, r"^W[1-3]-\d+$", path)
            node = _lookup(self.schema, path)
            self.assertIsNotNone(node, path)
            self.assertNotEqual(node.get("x-owner"), "executor", path)

    def test_manage_uses_the_executor_names_with_defaults(self):
        m = self.props["manage"]["properties"]
        self.assertEqual(set(m["off"]["items"]["enum"]), {"alerts", "ledger", "manpower", "stability",
                                                          "equipment_fill", "resources", "stockpile", "trucks"})
        want = {"wake_repeat_days": 7, "alert_persist_days": 14, "manpower_months": 2, "stability_floor": 0.5,
                "equipment_fill_floor": 0.8, "stockpile_days_critical": 7, "stockpile_days_warning": 30,
                "trucks_ratio": 1.5, "deficit_wake_days": 14}
        for k, v in want.items():
            self.assertEqual(m[k]["default"], v, k)
        for k, node in m.items():
            if k not in ("why", "note"):
                self.assertIn("default", node, k)  # every rule has a default
                self.assertTrue(node.get("x-executor-planned") or k in want or k in
                                ("off", "alerts_ignore", "conscription_laws"), k)
                if node.get("x-rule"):
                    self.assertIn(node["x-rule"], m["off"]["items"]["enum"], k)

    def test_coverage_checklist_ids(self):
        cov = self.props["_coverage"]["properties"]
        self.assertEqual(list(cov), ["c%d" % n for n in range(1, 30)])
        v = lambda s: schema_check.validate({"_coverage": {"c1": s}}, self.schema)  # noqa: E731
        for good in ("key:goal", "key:war.motorization,vp_targets", "call", "call:army_group", "md:plan.md 1",
                     "na: landlocked"):
            self.assertEqual(v(good), [], good)
        for bad in ("key:", "goal", "na:", "na:   ", "call:", "md:"):
            self.assertNotEqual(v(bad), [], bad)

    def test_new_shapes(self):
        v = lambda plan: schema_check.validate(plan, self.schema)  # noqa: E731
        plan = {
            "_meta": {"version": 1, "source": "strategist", "review": None, "lint_summary": None},
            "training": [{"template": "Line", "queue": 6, "batch": 2, "deploy_state": 2, "cap": 24,
                          "only_if_stockpile": {"infantry_equipment": 1000}, "when": "1936.3.1"}],
            "templates": [{"id": "line", "name": "Line", "regiments": [["infantry"] * 4, ["artillery_brigade"] * 2],
                           "support": ["engineer"], "priority": "elite", "role": "line", "xp": 40},
                          {"name": "Flat", "regiments": ["infantry", "infantry", "infantry"]}],
            "occupation": {"default_law": "military_governor_occupation", "ladder": ["no_garrison"],
                           "harsher_at_resistance": 50, "change_days": 30, "per_state": {"550": "no_garrison"}},
            "peace": {"take": [271, 842], "puppet": ["ETH"], "liberate": [], "max_rounds": 5},
            "auto_import": {"budget": 3},
            "air_plan": [{"wing": "CAS at 550", "region": "Ethiopia", "mission": "cas", "base_region": 273,
                          "then_mission": "air_superiority", "when": "1936.1.16"}],
            "manage": {"off": ["trucks"], "stability_floor": 50, "conscription_laws": ["limited_conscription"]},
            "force_posture": {"reason": "land war", "phases": [{"phase": "base", "land": 90, "air": 10, "navy": 0}]},
            "xp_budget": {"army": [{"id": "x", "cost": 40, "purpose": "template", "ref": "template:line"}]},
            "stockpile_targets": {"infantry_equipment": 30, "why": "two months of attrition"},
            "sea_transfer": {"convoys": 40, "ports": [550], "routes": [{"id": "r", "from_state": 2, "to_state": 550}]},
            "navy": {"posture": "none", "reason": "landlocked", "convoy_escort": False},
            "intelligence": {"agency": False, "reason": "minor"},
            "designs": [{"id": "d", "type": "plane", "name": "CAS", "base": "small_plane_cas_airframe_1",
                         "call": {"tool": "hoi4_design_create"}}],
            "mio": [{"id": "m", "organisation": "ITA_ansaldo_organization"}],
            "trade": {"deals": [{"id": "t", "resource": "steel", "amount": 8}]},
            "production": [{"equipment": "infantry_equipment_1", "remove": True},
                           {"equipment": "infantry_equipment_1", "replace": "infantry_equipment_2"}],
        }
        self.assertEqual(v(plan), [])
        self.assertEqual(v({"auto_import": 2}), [])
        self.assertEqual(v({"auto_import": True}), [])
        for bad in ({"manage": {"off": ["fuel"]}}, {"manage": {"stability_floor": 150}},
                    {"manage": {"surprise": 1}}, {"templates": [{"name": "x"}]},
                    {"navy": {"posture": "big"}}, {"force_posture": {"phases": []}},
                    {"xp_budget": {"army": [{"id": "x", "cost": 1, "purpose": "template", "ref": "templates:x"}]}}):
            self.assertNotEqual(v(bad), [], bad)


class HandPlans(unittest.TestCase):
    def test_hand_plans_pass_the_schema(self):
        schema = _load_schema()
        for path in HAND_PLANS:
            with open(path, encoding="utf-8") as f:
                plan = json.load(f)
            self.assertEqual(schema_check.validate(plan, schema), [], os.path.basename(path))
            self.assertEqual(plan["_meta"]["source"], "hand")
            self.assertEqual(sorted(plan["_coverage"], key=lambda c: int(c[1:])), ["c%d" % n for n in range(1, 30)])


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
