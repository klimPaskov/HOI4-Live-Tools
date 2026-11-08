"""hoi4_check_trigger and hoi4_check_focus on a fake console that answers like the game does.

The fake reads each `eval_effect` command the tools send, evaluates the wrapped trigger against a table of leaf
answers, and appends the log line the `log` effect writes (the recorded format of game.log lines, see
telemetry/parser.py) to a real game.log in a temporary folder. The tools read it back exactly as they would.
"""

import os
import re
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import trigger_check as tc
from hoi4_live.game.logs import Logs
from hoi4_live.gamedata import focus as focus_data
from hoi4_live.gamedata import triggertext as tt
from hoi4_live.tools import check_tools

CMD_RX = re.compile(r'^eval_effect (?:(\S+) = \{ )?if = \{ limit = \{ (.*) \} log = "(HLT[0-9a-f]{3}_\d+) TRUE" '
                    r'else = \{ log = "\S+ FALSE" \} \}(?: \})?$')


def evaluate(stmt, leaves):
    """Truth of a statement: leaves by their rendered text, AND / OR / NOT, a scope block as an AND of its children."""
    if not stmt.is_block:
        return leaves.get(stmt.render(), False)
    kids = [evaluate(c, leaves) for c in stmt.children]
    key = (stmt.key or "AND").upper()
    if key == "OR":
        return any(kids)
    if key == "NOT":
        return not any(kids)
    return all(kids)


class Game:
    """A fake game: leaf answers, a log file, and the commands it received."""

    def __init__(self, folder, leaves, drop=(), error_line=None):
        self.logs = Logs(folder)
        self.leaves = leaves
        self.drop = drop               # leaf texts the game never answers (an unknown trigger)
        self.error_line = error_line
        self.commands = []
        self.scopes = []

    def run(self, session, commands, keep_open=False, read=True):
        for cmd in commands:
            self.commands.append(cmd)
            m = CMD_RX.match(cmd)
            assert m, cmd
            scope, trigger, tag = m.groups()
            self.scopes.append(scope)
            stmts = tt.parse(trigger)
            if any(s.render() in self.drop for s in stmts):
                if self.error_line:
                    with open(os.path.join(self.logs.dir, "error.log"), "a", encoding="utf-8") as f:
                        f.write(self.error_line + "\n")
                continue
            value = all(evaluate(s, self.leaves) for s in stmts)
            with open(os.path.join(self.logs.dir, "game.log"), "a", encoding="utf-8") as f:
                f.write(f"[03:09:06][1936.01.11.23][effectbase.cpp:1783]: {tag} {'TRUE' if value else 'FALSE'}\n")
        return [{"command": c} for c in commands]


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_chk_")
        self.addCleanup(self.tmp.cleanup)

    def session(self, game):
        s = SimpleNamespace(logs=game.logs, check_cancel=lambda: None, sleep=lambda t: time.sleep(min(t, 0.01)))
        s.cfg = SimpleNamespace()
        return s

    def game(self, leaves, **kw):
        g = Game(self.tmp.name, leaves, **kw)
        for name in ("game", "error"):
            open(os.path.join(self.tmp.name, name + ".log"), "w").close()
        p = mock.patch.object(tc.console, "run", side_effect=g.run)
        p.start()
        self.addCleanup(p.stop)
        return g

    def trigger(self, game, text, scope=None, depth=2):
        args = {"trigger": text, "depth": depth}
        if scope:
            args["scope"] = scope
        with mock.patch.object(tc, "POLL_SECONDS", 0.05):
            return check_tools.hoi4_check_trigger.__wrapped__(self.session(game), args)


class TriggerTests(Base):
    def test_true_trigger(self):
        g = self.game({"has_war = yes": True})
        res = self.trigger(g, "has_war = yes", scope="GER")
        self.assertTrue(res.data["result"])
        self.assertTrue(res.summary.startswith("TRUE"))
        self.assertEqual(g.scopes, ["GER"])
        self.assertEqual(res.data["unmet"], [])

    def test_a_false_leaf_is_the_unmet_condition(self):
        g = self.game({"has_war = yes": False})
        res = self.trigger(g, "has_war = yes")
        self.assertIs(res.data["result"], False)
        self.assertEqual([u["condition"] for u in res.data["unmet"]], ["has_war = yes"])
        self.assertIn("FALSE: has_war = yes", res.summary)
        self.assertEqual(g.scopes, [None])                       # no scope: the player

    def test_each_top_level_condition_is_asked_and_the_false_ones_named(self):
        g = self.game({"has_war = yes": True, "tag = GER": False, "tag = ITA": False, "is_in_faction = yes": True})
        res = self.trigger(g, "has_war = yes  OR = { tag = GER tag = ITA }  NOT = { is_in_faction = yes }")
        self.assertIs(res.data["result"], False)
        unmet = {u["condition"]: u for u in res.data["unmet"]}
        self.assertEqual(set(unmet), {"OR = { tag = GER tag = ITA }", "NOT = { is_in_faction = yes }"})
        or_children = unmet["OR = { tag = GER tag = ITA }"]["because"]
        self.assertEqual([c["result"] for c in or_children], [False, False])
        not_children = unmet["NOT = { is_in_faction = yes }"]["because"]
        self.assertEqual(not_children[0]["condition"], "is_in_faction = yes")
        self.assertTrue(not_children[0]["result"])
        self.assertIn("a NOT needs it false", not_children[0]["note"])

    def test_depth_one_does_not_explain(self):
        g = self.game({"tag = GER": False})
        res = self.trigger(g, "OR = { tag = GER tag = ITA }", depth=1)
        self.assertNotIn("because", res.data["unmet"][0])
        self.assertEqual(len(g.commands), 1)

    def test_a_pasted_limit_wrapper_is_taken_off(self):
        g = self.game({"date > 1937.1.1": True, "tag = GER": True})
        res = self.trigger(g, "limit = { date > 1937.1.1 OR = { tag = GER tag = ITA } }")
        self.assertTrue(res.data["result"])
        self.assertEqual(len(g.commands), 2)

    def test_comments_and_quotes_survive(self):
        g = self.game({'has_dlc = "Waking the Tiger"': True})
        res = self.trigger(g, '# only with the DLC\nhas_dlc = "Waking the Tiger"')
        self.assertTrue(res.data["result"])
        self.assertIn('has_dlc = "Waking the Tiger"', g.commands[0])

    def test_a_state_scope(self):
        g = self.game({"is_coastal = yes": True})
        self.assertTrue(self.trigger(g, "is_coastal = yes", scope="64").data["result"])
        self.assertEqual(g.scopes, ["64"])

    def test_unbalanced_braces_are_a_syntax_error(self):
        g = self.game({})
        with self.assertRaises(ToolError) as cm:
            self.trigger(g, "OR = { tag = GER")
        self.assertEqual(cm.exception.details["error"], "trigger_syntax")
        self.assertEqual(g.commands, [])

    def test_no_answer_is_unknown_and_error_log_lines_come_back(self):
        err = "[03:09:07][effect.cpp:99]: Unknown trigger: has_flurb"
        g = self.game({}, drop=("has_flurb = yes",), error_line=err)
        res = self.trigger(g, "has_flurb = yes")
        self.assertIsNone(res.data["result"])
        self.assertTrue(res.summary.startswith("UNKNOWN"))
        self.assertEqual(res.data["errors"], [err])
        self.assertEqual(len(res.data["unknown"]), 1)

    def test_every_command_fits_the_console(self):
        leaves = {f"has_idea = idea_{i:02d}": False for i in range(30)}
        g = self.game(leaves)
        text = "OR = { " + " ".join(f"has_idea = idea_{i:02d}" for i in range(30)) + " }"      # about 560 characters
        res = self.trigger(g, text)
        self.assertTrue(all(len(c) <= tc.MAX_COMMAND for c in g.commands))
        # too long to ask whole, so it was answered from its children: every one of them false, so the OR is false
        self.assertIs(res.data["result"], False)
        self.assertEqual(len(res.data["unmet"][0]["because"]), 30)

    def test_a_too_long_leaf_is_unknown(self):
        g = self.game({})
        res = self.trigger(g, "has_idea = " + "x" * 400)
        self.assertIsNone(res.data["result"])
        self.assertIn("too long", res.data["unknown"][0]["note"])
        self.assertEqual(g.commands, [])

    def test_a_scope_block_is_explained_inside_the_same_scope(self):
        g = self.game({"is_in_faction_with = ROOT": True, "has_war = yes": False})
        res = self.trigger(g, "ENG = { is_in_faction_with = ROOT has_war = yes }")
        self.assertIs(res.data["result"], False)
        why = res.data["unmet"][0]["because"]
        self.assertEqual([(c["condition"], c["result"]) for c in why],
                         [("is_in_faction_with = ROOT", True), ("has_war = yes", False)])

    def test_a_grandchild_is_asked_inside_every_scope_around_it(self):
        g = self.game({"has_war = yes": False, "tag = ITA": False})
        res = self.trigger(g, "GER = { OR = { has_war = yes tag = ITA } }", depth=3)
        self.assertIs(res.data["result"], False)
        grandchildren = [c for c in g.commands if "limit = { GER = { has_war = yes } }" in c]
        self.assertEqual(len(grandchildren), 1, g.commands)
        or_node = res.data["unmet"][0]["because"][0]
        self.assertEqual(or_node["condition"], "OR = { has_war = yes tag = ITA }")
        self.assertEqual([c["condition"] for c in or_node["because"]], ["has_war = yes", "tag = ITA"])

    def test_blocks_that_cannot_be_split_say_so(self):
        g = self.game({"value > 5": True})
        res = self.trigger(g, "check_variable = { var = x value > 5 }  any_country = { tag = GER has_war = yes }")
        self.assertIs(res.data["result"], False)
        notes = {u["condition"].split(" ")[0]: u.get("note") for u in res.data["unmet"]}
        self.assertIn("not split", notes["any_country"])
        self.assertTrue(all("because" not in u for u in res.data["unmet"]))

    def test_the_check_budget_stops_the_explaining(self):
        g = self.game({})
        nodes = [tc.Node(s) for s in tt.parse("OR = { a = 1 b = 2 c = 3 }")]
        with mock.patch.object(tc, "POLL_SECONDS", 0.05):
            rep = tc.check(self.session(g), None, nodes, depth=2, max_checks=2)
        self.assertTrue(rep["truncated"])
        self.assertEqual(nodes[0].children, [])


class ParserTests(unittest.TestCase):
    def test_round_trip_keeps_structure_and_quotes(self):
        text = 'a = 1 OR = { b = "x y" c > 5 } { bare } flag'
        self.assertEqual(tt.render_all(tt.parse(text)), 'a = 1 OR = { b = "x y" c > 5 } { bare } flag')

    def test_unwrap_only_takes_known_wrappers(self):
        self.assertEqual(tt.render_all(tt.conditions("limit = { a = 1 b = 2 }")), "a = 1 b = 2")
        self.assertEqual(tt.render_all(tt.conditions("available = { limit = { a = 1 } }")), "a = 1")
        self.assertEqual(tt.render_all(tt.conditions("OR = { a = 1 }")), "OR = { a = 1 }")

    def test_results_of_other_runs_are_ignored(self):
        line = "[03:09:06][1936.01.11.23][effectbase.cpp:1783]: "
        text = f"{line}HLTabc_1 TRUE\n{line}HLTdef_1 FALSE\n{line}HLTabc_2 FALSE\n"
        self.assertEqual(tc.parse_results(text, "HLTabc"), {1: True, 2: False})

    def test_an_echo_of_the_command_is_not_an_answer(self):
        echo = ('[03:09:05][console.cpp:12]: eval_effect GER = { if = { limit = { a = b } log = "HLTabc_1 TRUE" '
                'else = { log = "HLTabc_1 FALSE" } } }\n')
        real = "[03:09:06][1936.01.11.23][effectbase.cpp:1783]: HLTabc_1 FALSE\r\n"
        self.assertEqual(tc.parse_results(echo + real, "HLTabc"), {1: False})
        self.assertEqual(tc.parse_results(echo, "HLTabc"), {})

    def test_operators_without_spaces_and_inline_math_stay_whole(self):
        self.assertEqual(tt.render_all(tt.parse("a!=b c?=1 d>=2")), "a != b c ?= 1 d >= 2")
        self.assertEqual(tt.render_all(tt.parse("x > @[ base + 1 ]")), "x > @[ base + 1 ]")

    def test_constants_of_the_file_are_replaced(self):
        text = "@min = 5\n@name_long = 7 # note\nfocus = { id = a available = { num > @min num > @name_long } }\n"
        self.assertEqual(tt.render_all(tt.parse(tt.resolve_constants(text))),
                         "focus = { id = a available = { num > 5 num > 7 } }")


FOCUS_FILE = """
focus_tree = {
	id = test_tree
	country = { factor = 0 modifier = { add = 10 tag = GER } }
	focus = { id = GER_first x = 0 y = 0 cost = 5 }
	focus = { id = GER_left x = -1 y = 1 relative_position_id = GER_first
		prerequisite = { focus = GER_first } mutually_exclusive = { focus = GER_right } }
	focus = { id = GER_right x = 1 y = 1 relative_position_id = GER_first
		prerequisite = { focus = GER_first } mutually_exclusive = { focus = GER_left }
		available = { has_war = no date > 1936.6.1 }
		allow_branch = { NOT = { has_country_flag = GER_closed } }
		bypass = { has_country_flag = GER_done } }
	focus = { id = GER_choice x = 0 y = 2 relative_position_id = GER_first
		prerequisite = { focus = GER_left focus = GER_right }
		prerequisite = { focus = GER_first }
		available = { OR = { tag = GER tag = AUS } } }
}
"""


class FocusTests(Base):
    def setUp(self):
        super().setUp()
        self.path = os.path.join(self.tmp.name, "tree.txt")
        with open(self.path, "w", encoding="utf-8") as f:
            f.write(FOCUS_FILE)
        self.idx = focus_data.build_index([self.path])
        p = mock.patch.object(focus_data, "for_session", return_value=self.idx)
        p.start()
        self.addCleanup(p.stop)

    def focus(self, game, focus, tag="GER", depth=2):
        with mock.patch.object(tc, "POLL_SECONDS", 0.05):
            return check_tools.hoi4_check_focus.__wrapped__(self.session(game),
                                                            {"focus": focus, "tag": tag, "depth": depth})

    def test_available_when_everything_holds(self):
        g = self.game({"has_completed_focus = GER_first": True, "has_war = no": True, "date > 1936.6.1": True})
        res = self.focus(g, "GER_right")
        self.assertEqual(res.data["status"], "available", res.data)
        self.assertTrue(res.data["available"])
        self.assertEqual(res.data["tree"], "test_tree")
        self.assertFalse(res.data["completed"])
        self.assertIs(res.data["bypass"], False)
        self.assertIn("AVAILABLE", res.summary)

    def test_each_kind_of_unmet_condition_is_named(self):
        # the prerequisite is missing, the exclusive focus is taken, `available` fails on the date, the branch is closed
        g = self.game({"has_completed_focus = GER_first": False, "has_completed_focus = GER_left": True,
                       "has_war = no": True, "date > 1936.6.1": False, "has_country_flag = GER_closed": True})
        res = self.focus(g, "GER_right")
        self.assertEqual(res.data["status"], "unavailable")
        kinds = {u["kind"] for u in res.data["unmet"]}
        self.assertEqual(kinds, {"prerequisite", "mutually_exclusive", "available", "allow_branch"})
        labels = [u["condition"] for u in res.data["unmet"]]
        self.assertIn("prerequisite: GER_first", labels)
        self.assertIn("mutually exclusive with GER_left, which is completed", labels)
        self.assertIn("date > 1936.6.1", labels)
        self.assertIn("NOT AVAILABLE", res.summary)

    def test_a_completed_focus_is_not_available(self):
        g = self.game({"has_completed_focus = GER_first": True})
        res = self.focus(g, "GER_first")
        self.assertEqual(res.data["status"], "completed")
        self.assertIs(res.data["available"], False)
        self.assertIn("already completed", res.summary)

    def test_prerequisite_groups_are_or_inside_and_across(self):
        leaves = {"has_completed_focus = GER_first": True, "tag = GER": True}
        g = self.game(leaves)
        res = self.focus(g, "GER_choice")
        unmet = res.data["unmet"]
        self.assertEqual([u["condition"] for u in unmet], ["prerequisite: one of GER_left, GER_right"])
        self.assertEqual(len(unmet[0]["because"]), 2)

    def test_a_bypass_that_holds_is_reported(self):
        g = self.game({"has_country_flag = GER_done": True})
        res = self.focus(g, "GER_right")
        self.assertIs(res.data["bypass"], True)
        self.assertIn("bypass", res.data["bypass_note"])

    def test_a_focus_whose_file_cannot_be_read_is_never_called_available(self):
        g = self.game({"has_completed_focus = GER_first": True})
        with mock.patch.object(check_tools, "_raw_focus", return_value=(None, "gone.txt")):
            res = self.focus(g, "GER_right")
        self.assertEqual(res.data["status"], "unknown")
        self.assertIsNone(res.data["available"])
        self.assertTrue(any("could not be read" in n for n in res.data["not_checked"]))

    def test_an_unanswered_completed_check_leaves_the_status_unknown(self):
        g = self.game({}, drop=("has_completed_focus = GER_first",))
        res = self.focus(g, "GER_first")
        self.assertEqual(res.data["status"], "unknown")
        self.assertIsNone(res.data["completed"])

    def test_a_constant_in_available_is_resolved_before_it_is_asked(self):
        with open(self.path, "w", encoding="utf-8") as f:
            f.write("@min = 5\n" + FOCUS_FILE.replace("has_war = no date > 1936.6.1",
                                                      "has_war = no num_of_factories > @min"))
        self.idx = focus_data.build_index([self.path])
        g = self.game({"has_completed_focus = GER_first": True, "has_war = no": True, "num_of_factories > 5": True})
        with mock.patch.object(focus_data, "for_session", return_value=self.idx):
            res = self.focus(g, "GER_right")
        self.assertEqual(res.data["status"], "available", res.data)
        self.assertTrue(any("num_of_factories > 5" in c for c in g.commands))

    def test_an_unknown_focus_suggests_the_close_ones(self):
        g = self.game({})
        with self.assertRaises(ToolError) as cm:
            self.focus(g, "GER_rigth")
        self.assertEqual(cm.exception.details["error"], "focus_unknown")
        self.assertIn("GER_right", cm.exception.details["close"])
        self.assertEqual(g.commands, [])

    def test_unanswered_conditions_leave_the_status_unknown(self):
        g = self.game({"has_completed_focus = GER_first": True, "has_war = no": True}, drop=("date > 1936.6.1",))
        res = self.focus(g, "GER_right")
        self.assertEqual(res.data["status"], "unknown")
        self.assertIsNone(res.data["available"])
        self.assertTrue(res.data["unknown"])


if __name__ == "__main__":
    unittest.main()
