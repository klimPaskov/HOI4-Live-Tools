"""hoi4_smoke_namespace on a fake game: events fired through the console, windows answered, error.log read after each
step, decisions checked and taken. The fake keeps one world: which event is current, which window is open, which
error line each answer writes, and the answers to the trigger checks (same wrapper format as test_check_tools)."""

import json
import os
import tempfile
import time
import unittest
from types import SimpleNamespace
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.errors import ToolError
from hoi4_live.game import trigger_check as tc
from hoi4_live.game.logs import Logs
from hoi4_live.gamedata import decisions as dec_data
from hoi4_live.gamedata import events as ev_data
from hoi4_live.gamedata import triggertext as tt
from hoi4_live.tools import smoke_tools as st

from test_check_tools import CMD_RX, evaluate

EVENTS = """
add_namespace = my_mod
country_event = { id = my_mod.1 title = my_mod.1.t is_triggered_only = yes
	option = { name = my_mod.1.a } option = { name = my_mod.1.b } option = { name = my_mod.1.c trigger = { always = no } } }
country_event = { id = my_mod.2 hidden = yes is_triggered_only = yes immediate = { add_stability = 0.01 } }
state_event = { id = my_mod.3 is_triggered_only = yes option = { name = my_mod.3.a } }
news_event = { id = my_mod.10 option = { name = my_mod.10.a } }
country_event = { id = my_mod.4 title = my_mod.4.t option = { name = my_mod.4.a } }
country_event = { id = other.1 option = { name = other.1.a } }
"""

DECISIONS = """
my_mod_decisions = {
	my_mod_take_it = { cost = 10 available = { has_war = no } visible = { always = yes } }
	my_mod_blocked = { available = { has_war = yes OR = { tag = GER tag = ITA } } }
	my_mod_target = { target_trigger = { always = yes } targets = { GER } }
	my_mod_mission = { days_mission_timeout = 30 }
	my_mod_take_fails = { }
}
other_decisions = { my_mod_other = { } other_one = { } }
"""


class World:
    def __init__(self, folder, popups=None, errors=None, stuck=(), leaves=None, take_fails=()):
        self.logs = Logs(folder)
        for name in ("game", "error"):
            open(os.path.join(folder, name + ".log"), "w").close()
        self.folder = folder
        self.popups = popups or {}          # event id -> list of option texts (a window), missing: no window
        self.errors = errors or {}          # (event id, option) -> error.log line the answer writes
        self.stuck = set(stuck)             # event ids whose window does not close
        self.leaves = leaves or {}
        self.take_fails = set(take_fails)
        self.current = None
        self.open = False
        self.commands = []
        self.answers = []
        self.taken = []

    # console.run
    def console_run(self, session, commands, keep_open=False, read=True):
        out = []
        for cmd in commands:
            self.commands.append(cmd)
            if cmd.startswith("event "):
                self.current = cmd.split()[1]
                self.open = self.current in self.popups
            elif cmd.startswith("eval_effect") and CMD_RX.match(cmd):
                scope, trigger, tag = CMD_RX.match(cmd).groups()
                value = all(evaluate(s, self.leaves) for s in tt.parse(trigger))
                with open(os.path.join(self.folder, "game.log"), "a", encoding="utf-8") as f:
                    f.write(f"[03:09:06][1936.01.11.23][effectbase.cpp:1783]: {tag} {'TRUE' if value else 'FALSE'}\n")
            out.append({"command": cmd, "response": ["ok"]})
        return out

    def detect(self, img):
        return [SimpleNamespace(buttons=[1])] if self.open else []

    def answer(self, session, popup_index=0, option=0, option_text=None, img=None, save_shot=True):
        texts = self.popups[self.current]
        if option >= len(texts):
            raise ToolError(f"option {option} out of range", {"options": texts})
        self.answers.append((self.current, option))
        line = self.errors.get((self.current, option))
        if line:
            with open(os.path.join(self.folder, "error.log"), "a", encoding="utf-8") as f:
                f.write(line + "\n")
        closed = self.current not in self.stuck
        self.open = not closed
        return {"kind": "country", "title": self.current + " title", "text": "body", "options": list(texts),
                "chosen": option, "chosen_text": texts[option], "closed": closed}

    def dispatch_call(self, session, name, arguments, **kw):
        assert name == "hoi4_decisions"
        did = arguments["decision"]
        self.taken.append(did)
        failed = did in self.take_fails
        return {"content": [{"type": "text", "text": "could not take: not on the panel" if failed else "taken"}],
                "isError": failed}


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_smoke_")
        self.addCleanup(self.tmp.cleanup)
        self.events_path = os.path.join(self.tmp.name, "events.txt")
        self.decisions_path = os.path.join(self.tmp.name, "decisions.txt")
        with open(self.events_path, "w", encoding="utf-8") as f:
            f.write(EVENTS)
        with open(self.decisions_path, "w", encoding="utf-8") as f:
            f.write(DECISIONS)

    def run_tool(self, world, **args):
        full = {"namespace": "my_mod", "what": "events", "options": "each", "limit": 60, "wait_s": 0.5, "pp": 2000,
                "stop_on_error": False, "dry_run": False}
        full.update(args)
        session = SimpleNamespace(
            logs=world.logs, check_cancel=lambda: None, sleep=lambda t: None, park_cursor=lambda: None,
            report=lambda *a, **k: None, grab=lambda focus=None: object(),
            artifact_path=lambda name: os.path.join(self.tmp.name, name))
        patches = (
            mock.patch.object(ev_data, "for_session", return_value=ev_data.build_index([self.events_path])),
            mock.patch.object(dec_data, "for_session", return_value={
                "categories": {}, "decisions": dec_data.build_decisions([self.decisions_path])}),
            mock.patch.object(st.console, "run", side_effect=world.console_run),
            mock.patch.object(st.popups_mod, "detect", side_effect=world.detect),
            mock.patch.object(st.game_popups, "answer", side_effect=world.answer),
            mock.patch.object(st.game_popups, "handle_all", return_value=[]),
            mock.patch.object(st.dispatch, "call", side_effect=world.dispatch_call),
            mock.patch.object(tc, "POLL_SECONDS", 0.05))
        for p in patches:
            p.start()
            self.addCleanup(p.stop)
        return st.hoi4_smoke_namespace.__wrapped__(session, full)


class EventTests(Base):
    def test_every_option_is_answered_with_its_own_firing(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A", "B", "C"], "my_mod.10": ["Ok"], "my_mod.4": ["Fine"]})
        res = self.run_tool(w)
        by_id = {r["id"]: r for r in res.data["events"]}
        self.assertEqual([a for a in w.answers if a[0] == "my_mod.1"],
                         [("my_mod.1", 0), ("my_mod.1", 1), ("my_mod.1", 2)])
        self.assertEqual(by_id["my_mod.1"]["options_seen"], 3)
        self.assertEqual([f["chosen_text"] for f in by_id["my_mod.1"]["fires"]], ["A", "B", "C"])
        self.assertEqual(by_id["my_mod.10"]["status"], "ok")
        self.assertEqual(by_id["my_mod.4"]["status"], "ok")
        self.assertEqual(by_id["my_mod.2"]["status"], "ok")                  # hidden: no window is right
        self.assertEqual(by_id["my_mod.2"]["fires"][0]["popup"], None)

    def test_events_are_fired_in_numeric_id_order_and_only_this_namespace(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A"], "my_mod.10": ["Ok"], "my_mod.4": ["Fine"]})
        res = self.run_tool(w)
        self.assertEqual([r["id"] for r in res.data["events"]], ["my_mod.1", "my_mod.2", "my_mod.3", "my_mod.4",
                                                                  "my_mod.10"])
        self.assertNotIn("event other.1", w.commands)
        self.assertIn("event my_mod.1", w.commands)

    def test_a_scoped_event_type_is_skipped_with_the_reason(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A"], "my_mod.10": ["Ok"], "my_mod.4": ["Fine"]})
        res = self.run_tool(w)
        skipped = next(r for r in res.data["events"] if r["id"] == "my_mod.3")
        self.assertEqual(skipped["status"], "skipped")
        self.assertIn("state_event", skipped["reason"])
        self.assertNotIn("event my_mod.3", w.commands)

    def test_the_tag_goes_on_the_console_command(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A"], "my_mod.10": ["Ok"], "my_mod.4": ["Fine"]})
        self.run_tool(w, tag="GER", only=["my_mod.4"])
        self.assertEqual(w.commands, ["event my_mod.4 GER"])

    def test_an_error_is_assigned_to_the_firing_that_caused_it(self):
        line = "[10:00:01][event.cpp:7]: Failed to find scripted effect my_mod_missing"
        w = World(self.tmp.name, popups={"my_mod.1": ["A", "B", "C"], "my_mod.4": ["Fine"], "my_mod.10": ["Ok"]},
                  errors={("my_mod.1", 1): line})
        res = self.run_tool(w)
        ev = next(r for r in res.data["events"] if r["id"] == "my_mod.1")
        self.assertEqual([f["new_error_lines"] for f in ev["fires"]], [0, 1, 0])
        self.assertEqual(ev["fires"][1]["new_errors"][0]["line"], line)
        self.assertEqual(ev["status"], "errors")
        self.assertFalse(res.data["ok"])
        self.assertTrue(res.summary.startswith("smoke my_mod: FAIL"))

    def test_a_window_that_does_not_close_is_stuck(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A"], "my_mod.4": ["Fine"], "my_mod.10": ["Ok"]},
                  stuck={"my_mod.4"})
        res = self.run_tool(w)
        ev = next(r for r in res.data["events"] if r["id"] == "my_mod.4")
        self.assertEqual(ev["status"], "stuck")

    def test_a_visible_event_without_a_window_is_reported(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A"], "my_mod.10": ["Ok"]})        # my_mod.4 shows nothing
        res = self.run_tool(w)
        ev = next(r for r in res.data["events"] if r["id"] == "my_mod.4")
        self.assertEqual(ev["status"], "no_window")
        self.assertFalse(res.data["ok"])

    def test_first_option_only(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A", "B", "C"]})
        self.run_tool(w, options="first", only=["my_mod.1"])
        self.assertEqual(w.answers, [("my_mod.1", 0)])

    def test_stop_on_error_stops_at_the_first_bad_event(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A"], "my_mod.4": ["Fine"], "my_mod.10": ["Ok"]},
                  errors={("my_mod.1", 0): "[10:00:01][x.cpp:1]: boom"})
        res = self.run_tool(w, stop_on_error=True)
        self.assertEqual([r["id"] for r in res.data["events"]], ["my_mod.1"])

    def test_limit_and_dry_run(self):
        w = World(self.tmp.name, popups={})
        res = self.run_tool(w, dry_run=True, limit=2)
        self.assertEqual(w.commands, [])
        self.assertEqual([e["id"] for e in res.data["events"]], ["my_mod.1", "my_mod.2"])
        self.assertEqual(res.data["not_run_because_of_limit"]["events"], 3)
        self.assertEqual(res.data["events"][0]["options_in_file"], 3)

    def test_an_empty_namespace_is_an_error(self):
        w = World(self.tmp.name)
        with self.assertRaises(ToolError) as cm:
            self.run_tool(w, namespace="nothing_here")
        self.assertEqual(cm.exception.details["error"], "smoke_empty")

    def test_the_report_files_list_the_events_and_the_error_lines(self):
        line = "[10:00:01][event.cpp:7]: Failed to find scripted effect my_mod_missing"
        w = World(self.tmp.name, popups={"my_mod.1": ["A", "B"], "my_mod.4": ["Fine"], "my_mod.10": ["Ok"]},
                  errors={("my_mod.1", 1): line})
        res = self.run_tool(w)
        with open(res.data["json"], encoding="utf-8") as f:
            saved = json.load(f)
        self.assertEqual(saved["namespace"], "my_mod")
        with open(res.data["report"], encoding="utf-8") as f:
            md = f.read()
        self.assertIn("| my_mod.1 | event | errors |", md)
        self.assertIn("Failed to find scripted effect my_mod_missing", md)


class DecisionTests(Base):
    LEAVES = {"has_war = no": True, "always = yes": True, "has_war = yes": False, "tag = GER": False,
              "tag = ITA": False}

    def run_decisions(self, **kw):
        w = World(self.tmp.name, leaves=self.LEAVES, take_fails=kw.pop("take_fails", ()))
        res = self.run_tool(w, what="decisions", decision_category="my_mod_decisions", **kw)
        return w, res, {r["id"]: r for r in res.data["decisions"]}

    def test_a_decision_with_met_conditions_is_taken(self):
        w, res, by_id = self.run_decisions()
        self.assertEqual(by_id["my_mod_take_it"]["status"], "ok")
        self.assertIn("my_mod_take_it", w.taken)
        self.assertTrue(by_id["my_mod_take_it"]["conditions"])

    def test_a_decision_with_an_unmet_condition_names_it_and_is_not_taken(self):
        w, res, by_id = self.run_decisions()
        rec = by_id["my_mod_blocked"]
        self.assertEqual(rec["status"], "not_taken")
        self.assertNotIn("my_mod_blocked", w.taken)
        conditions = [u["condition"] for u in rec["unmet"]]
        self.assertEqual(conditions, ["has_war = yes", "OR = { tag = GER tag = ITA }"])

    def test_targeted_decisions_and_missions_are_skipped(self):
        w, res, by_id = self.run_decisions()
        self.assertEqual(by_id["my_mod_target"]["status"], "skipped")
        self.assertIn("target", by_id["my_mod_target"]["reason"])
        self.assertEqual(by_id["my_mod_mission"]["status"], "skipped")
        self.assertNotIn("my_mod_target", w.taken)
        self.assertNotIn("my_mod_mission", w.taken)

    def test_a_decision_the_panel_refuses_is_not_taken_with_the_message(self):
        w, res, by_id = self.run_decisions(take_fails=("my_mod_take_fails",))
        self.assertEqual(by_id["my_mod_take_fails"]["status"], "not_taken")
        self.assertIn("not on the panel", by_id["my_mod_take_fails"]["reason"])

    def test_the_tag_and_political_power_are_set_once_first(self):
        w, res, by_id = self.run_decisions(tag="GER")
        self.assertEqual(w.commands[:2], ["tag GER", "eval_effect add_political_power = 2000"])
        self.assertEqual(sum(1 for c in w.commands if c.startswith("tag ")), 1)

    def test_selection_by_prefix_defaults_to_the_namespace(self):
        w = World(self.tmp.name, leaves=self.LEAVES)
        res = self.run_tool(w, what="decisions", dry_run=True)
        self.assertEqual([d["id"] for d in res.data["decisions"]],
                         ["my_mod_blocked", "my_mod_mission", "my_mod_other", "my_mod_take_fails", "my_mod_take_it",
                          "my_mod_target"])

    def test_both_runs_events_then_decisions(self):
        w = World(self.tmp.name, popups={"my_mod.1": ["A"], "my_mod.4": ["Fine"], "my_mod.10": ["Ok"]},
                  leaves=self.LEAVES)
        res = self.run_tool(w, what="both", decision_category="my_mod_decisions", only=["my_mod.4", "my_mod_take_it"])
        self.assertEqual([r["id"] for r in res.data["events"]], ["my_mod.4"])
        self.assertEqual([r["id"] for r in res.data["decisions"]], ["my_mod_take_it"])


if __name__ == "__main__":
    unittest.main()
