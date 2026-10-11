"""Phase 5: the popup policy, the journal, the stop conditions and suggestions of hoi4_advance and the advance loop
itself (with a fake clock and a fake telemetry feed), without the game."""

import json
import os
import shutil
import tempfile
import unittest
from unittest import mock

from helpers import FIXTURES  # noqa: F401  (puts the repository on sys.path)

from hoi4_live.errors import ToolError
from hoi4_live.game.popups import PopupPolicy
from hoi4_live.perception.popups import Button, Popup
from hoi4_live.state import advance as adv
from hoi4_live.state import briefing, journal as journal_mod
from hoi4_live.telemetry.parser import TelemetryStore
from hoi4_live.tools import campaign_loop_tools as loop
from hoi4_live.tools import registry


def popup(kind="event", title="Title", text="", options=("A", "B")):
    p = Popup(kind, [0, 0, 500, 400], [Button([10, 100 + 50 * i, 300, 40], o) for i, o in enumerate(options)],
              "warm", 1.0)
    p.title, p.text = title, text
    return p


class PopupPolicyTests(unittest.TestCase):
    def test_first_answers_with_the_first_option(self):
        self.assertEqual(PopupPolicy("first").decide(popup())[0], 0)

    def test_choices_answers_news_and_single_option_events_but_stops_at_a_real_choice(self):
        pol = PopupPolicy("choices")
        self.assertEqual(pol.decide(popup("news", options=("A", "B")))[0], 0)
        self.assertEqual(pol.decide(popup("event", options=("Ok",)))[0], 0)
        self.assertIsNone(pol.decide(popup("event", options=("A", "B")))[0])

    def test_stop_stops(self):
        self.assertIsNone(PopupPolicy("stop").decide(popup())[0])
        self.assertEqual(PopupPolicy("pause").mode, "stop")

    def test_rules_decide_first_by_title_or_text(self):
        pol = PopupPolicy("choices", [{"match": "earhart", "option": "^No"},
                                      {"match": "oil concession", "stop": True},
                                      {"match": "pact", "option": 1}])
        p = popup(title="Earhart Wants to Enlist", options=("Yes, we need her.", "No, that won't be necessary."))
        self.assertEqual(pol.decide(p)[0], 1)
        self.assertIsNone(pol.decide(popup(text="The Mexican oil concession is offered"))[0])
        self.assertEqual(pol.decide(popup(title="A pact", options=("x", "y", "z")))[0], 1)
        self.assertEqual(pol.decide(popup(title="Pact", options=("only",)))[0], 0)       # index clamped
        self.assertIsNone(pol.decide(popup(title="Other"))[0])                           # the mode: a real choice

    def test_rule_without_matching_option_hands_the_popup_to_the_agent(self):
        pol = PopupPolicy("first", [{"match": "x", "option": "^Refuse"}])
        self.assertIsNone(pol.decide(popup(title="x", options=("Yes", "No")))[0])

    def test_bad_mode_and_bad_regex_are_errors(self):
        with self.assertRaises(ToolError):
            PopupPolicy("sometimes")
        with self.assertRaises(ToolError):
            PopupPolicy("first", [{"match": "("}])

    def test_parse_accepts_strings_dicts_and_policies(self):
        self.assertEqual(PopupPolicy.parse("choices").mode, "choices")
        self.assertEqual(PopupPolicy.parse({"mode": "stop", "rules": [{"match": "a"}]}).rules[0]["match"], "a")
        pol = PopupPolicy("first")
        self.assertIs(PopupPolicy.parse(pol), pol)
        self.assertEqual(PopupPolicy.parse(None).mode, "first")
        self.assertTrue(PopupPolicy.parse("first", [{"match": "a"}]).needs_text)


class JournalTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_journal_")
        self.j = journal_mod.Journal(self.tmp, "USA")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def test_add_read_filter(self):
        self.j.add("plan", "Industry first", "1936.1.2.12")
        self.j.add("action", "Research started", "1936.1.3.12", {"tool": "hoi4_research"})
        self.j.add("note", "Mexico looks weak", "1936.2.1.12", tags=["mexico"])
        self.assertEqual(len(self.j.read()), 3)
        self.assertEqual([e["kind"] for e in self.j.read(kinds=["plan", "note"])], ["plan", "note"])
        self.assertEqual([e["kind"] for e in self.j.read(auto=False)], ["plan", "note"])
        self.assertEqual(len(self.j.read(since="1936.2.1")), 1)
        self.assertEqual(self.j.read(query="mexico")[0]["tags"], ["mexico"])
        self.assertEqual(len(self.j.read(limit=2)), 2)
        self.assertEqual(self.j.latest("plan")["text"], "Industry first")

    def test_unreadable_lines_are_skipped_and_unknown_kinds_refused(self):
        self.j.add("note", "ok")
        with open(self.j.path, "a", encoding="utf-8") as f:
            f.write("not json\n[1, 2]\n")
        self.assertEqual(len(self.j.entries()), 1)
        with self.assertRaises(ValueError):
            self.j.add("rumour", "x")

    def test_report(self):
        self.assertIn("empty", journal_mod.report(self.j))
        self.j.add("plan", "Build factories", "1936.1.1.12")
        self.j.add("decision", "Focus Industry: cheapest", "1936.1.2.12")
        self.j.add("advance", "advanced 1936.1.1.12 -> 1936.3.1.12", "1936.3.1.12")
        text = journal_mod.report(self.j)
        self.assertIn("1 Jan 1936 to 1 Mar 1936 (60 days)", text)
        self.assertIn("## Current plan", text)
        self.assertIn("Build factories", text)
        self.assertIn("## Decisions and lessons", text)
        self.assertIn("## Timeline", text)

    def test_date_helpers(self):
        self.assertEqual(journal_mod.fmt_date("1936.2.5.12"), "5 Feb 1936")
        self.assertEqual(journal_mod.date_key("1936.2.5"), (1936, 2, 5, 0))
        self.assertIsNone(journal_mod.date_key("x"))


def day(n, date="1936.1.2.12", **kw):
    rec = {"k": "day", "t": "USA", "n": n, "_date": date, "_wall": 1.0, "pp": 50.0, "stab": 0.6, "ws": 0.1,
           "civ_free": 5, "mil_free": 0, "nav_free": 0, "enemies": 0, "surrender": 0.0, "techs": 10,
           "states_ctrl": 50, "states": 50, "front": 0, "fuel_ratio": 1.0, "div": 40, "cas_k": 0.0}
    rec.update(kw)
    return rec


class StopConditionTests(unittest.TestCase):
    def check(self, a, b, stop_on=None, **kw):
        codes = [r["code"] for r in adv.evaluate(adv.watch(a), adv.watch(b),
                                                       list(adv.STOP_CONDITIONS) if stop_on is None else stop_on, **kw)]
        return codes

    def test_nothing_changed(self):
        self.assertEqual(self.check(day(1), day(2)), [])

    def test_tech_and_factories(self):
        self.assertEqual(self.check(day(1), day(2, techs=11)), ["tech_done"])
        self.assertEqual(self.check(day(1), day(2, civ_free=6)), ["construction_done"])
        self.assertEqual(self.check(day(1, mil_free=0), day(2, mil_free=2)), ["production_idle"])
        self.assertEqual(self.check(day(1, civ_free=6), day(2, civ_free=5)), [])          # fewer free: busy again

    def test_wars_and_territory(self):
        self.assertEqual(self.check(day(1), day(2, enemies=1)), ["war_started"])
        self.assertEqual(self.check(day(1, enemies=2), day(2, enemies=1)), ["war_ended"])
        self.assertEqual(self.check(day(1), day(2, states_ctrl=49)), ["territory_lost"])
        self.assertEqual(self.check(day(1), day(2, states_ctrl=52)), ["territory_gained"])

    def test_levels_fire_when_crossed_not_while_beyond(self):
        self.assertEqual(self.check(day(1, surrender=0.1), day(2, surrender=0.25)), ["losing"])
        self.assertEqual(self.check(day(1, surrender=0.3), day(2, surrender=0.35)), [])
        self.assertEqual(self.check(day(1, stab=0.45), day(2, stab=0.35)), ["low_stability"])
        self.assertEqual(self.check(day(1, stab=0.3), day(2, stab=0.25)), [])
        self.assertEqual(self.check(day(1, pp=149), day(2, pp=151)), ["pp_high"])
        self.assertEqual(self.check(day(1, pp=149), day(2, pp=151), thresholds={"pp_high": 200}), [])
        self.assertEqual(self.check(day(1, fuel_ratio=0.2), day(2, fuel_ratio=0.05)), ["low_fuel"])

    def test_only_the_selected_conditions_stop(self):
        self.assertEqual(self.check(day(1), day(2, techs=11, enemies=1), ["war_started"]), ["war_started"])
        self.assertEqual(self.check(day(1), day(2, techs=11), []), [])

    def test_focus_notices(self):
        w = adv.watch(day(1))
        r = adv.evaluate(w, w, ["focus_done"], notices=["National Focus Completed", "Something else"])
        self.assertEqual([x["code"] for x in r], ["focus_done"])
        self.assertEqual(adv.evaluate(w, w, ["tech_done"], notices=["National Focus Completed"]), [])

    def test_missing_numbers_do_not_crash(self):
        self.assertEqual(adv.evaluate(None, adv.watch(day(1)), ["tech_done"]), [])
        a = adv.watch(day(1))
        b = dict(a, techs=None, civ_free=None)
        self.assertEqual(adv.evaluate(a, b, list(adv.STOP_CONDITIONS)), [])

    def test_diff_reports_only_moved_fields(self):
        d = adv.diff(adv.watch(day(1, pp=10.0)), adv.watch(day(9, pp=18.5, techs=12)))
        self.assertEqual(d, {"pp": [10.0, 18.5], "techs": [10, 12]})
        self.assertEqual(adv.diff(None, adv.watch(day(1))), {})


class SuggestionTests(unittest.TestCase):
    def test_every_suggestion_names_a_registered_tool_and_action(self):
        for code, (tool, args, then) in adv.SUGGEST.items():
            t = registry.get(tool)
            self.assertIsNotNone(t, code)
            enum = t.schema["properties"].get("action", {}).get("enum")
            if enum:
                self.assertIn(args.get("action"), enum, f"{code}: {tool} {args}")
            self.assertTrue(then)

    def test_briefing_attention_items_carry_codes_that_have_suggestions(self):
        m = {"overview": {"political_power": 160, "stability": 0.3}, "economy": {"factories": {
            "civilian_free": 6, "military_free": 3}, "construction_queued": 0}, "research": {"idle_slots": 2},
            "focus": {"_sources": {"save": {}}, "current": None}, "diplomacy": {"at_war": True,
                                                                              "surrender_progress": 0.3},
            "sources": {}}
        items = briefing.attention(m)
        codes = {i.get("code") for i in items}
        for want in ("losing_war", "no_focus", "idle_research", "idle_construction", "idle_production", "spend_pp",
                     "low_stability"):
            self.assertIn(want, codes)
        sugg = adv.suggestions(items)
        self.assertEqual(len({s["tool"] + json.dumps(s["arguments"], sort_keys=True) for s in sugg}) <= len(sugg), True)
        self.assertTrue(all(s["tool"].startswith("hoi4_") for s in sugg))

    def test_suggestions_skip_unknown_codes_and_duplicates(self):
        items = [{"level": "now", "text": "a", "code": "idle_research"},
                 {"level": "now", "text": "b", "code": "idle_research"}, {"level": "info", "text": "c"},
                 {"level": "info", "text": "d", "code": "nothing_like_it"}]
        self.assertEqual(len(adv.suggestions(items)), 1)

    def test_telemetry_attention(self):
        items = adv.telemetry_attention(adv.watch(day(1, mil_free=2, civ_free=0, enemies=1, surrender=0.3)))
        self.assertEqual([i["code"] for i in items][:2], ["idle_production", "losing_war"])
        self.assertEqual(adv.telemetry_attention(None), [])


class DateParseTests(unittest.TestCase):
    def test_formats(self):
        self.assertEqual(loop.parse_date("1936.3.1"), (1936, 3, 1))
        self.assertEqual(loop.parse_date("1936-03-01"), (1936, 3, 1))
        self.assertEqual(loop.parse_date("1 Mar 1936"), (1936, 3, 1))
        self.assertEqual(loop.parse_date("15 September 1939"), (1939, 9, 15))
        self.assertIsNone(loop.parse_date("next spring"))

    def test_hour_24_counts_as_the_end_of_the_day(self):
        self.assertEqual(loop._hours("1936.2.16.24") - loop._hours("1936.2.16.0"), 24)
        self.assertEqual(loop._hours("1936.2.17.0"), loop._hours("1936.2.16.24"))
        self.assertIsNone(loop._hours("garbage"))

    def test_days_until(self):
        self.assertEqual(loop.days_until("1936.1.8.12", (1936, 2, 8)), 31)
        self.assertEqual(loop.days_until("1936.2.8.12", (1936, 1, 8)), -31)
        self.assertIsNone(loop.days_until("bad", (1936, 1, 8)))


class FakeSession:
    """Just enough of Session for hoi4_advance: artifacts_dir, telemetry (refreshed on access), check_cancel."""

    def __init__(self, tmp, store):
        self.cfg = mock.Mock(artifacts_dir=tmp)
        self._store = store
        self.last_tag = None

    @property
    def telemetry(self):
        return self._store

    def check_cancel(self):
        pass

    def report(self, *a, **k):
        pass

    def grab(self, *a, **k):
        raise AssertionError("no screenshot expected")


class FakeClock:
    """Replaces timectl.read_date and run_days: each chunk advances the clock and feeds one telemetry day per day;
    `script` maps a day counter to field changes."""

    def __init__(self, store, script=None, popups=None, feed=True):
        self.store, self.script, self.n, self.hour, self.feed = store, script or {}, 100, 0, feed
        self.popups = popups or {}
        self.fields = {}
        self.calls = []

    def date(self):
        d = self.n - 100
        return f"1936.{1 + d // 28}.{1 + d % 28}.{self.hour}"

    def read_date(self, session, img=None):
        return {"hoi4": self.date(), "text": self.date()}, None

    def run_days(self, session, days, hours, policy, speed, max_seconds, *a, **k):
        self.calls.append((days, hours, policy.mode, speed))
        rep = {"start": self.date(), "popups": [], "elapsed_hours": 0}
        total = days * 24 + hours
        while rep["elapsed_hours"] < total:
            self.n += 1
            self.fields.update(self.script.get(self.n, {}))
            if self.feed:
                self.store.add(day(self.n, self.date(), **self.fields))
            rep["elapsed_hours"] += 24
            if self.n in self.popups:
                rep["popups"] += self.popups[self.n]
                if any(not p.get("answered", True) for p in self.popups[self.n]):
                    rep["stopped_on_popup"] = True
                    break
        rep["end"] = self.date()
        return rep


class AdvanceLoopTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_advance_")
        self.store = TelemetryStore()
        self.store.add(day(100, "1936.1.1.0"))
        self.session = FakeSession(self.tmp, self.store)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_tool(self, clock, **args):
        full = {"popups": "choices", "speed": 5, "chunk_days": 2, "max_seconds": 600, "situation": False,
                "journal": True, "screenshot": False, "engine": "screen"}
        full.update(args)
        with mock.patch.object(loop.timectl, "run_days", clock.run_days), \
                mock.patch.object(loop.timectl, "read_date", clock.read_date):
            res = loop.hoi4_advance(self.session, full)
        return res

    def test_runs_the_whole_period_when_nothing_happens(self):
        clock = FakeClock(self.store)
        res = self.run_tool(clock, days=6)
        self.assertEqual(res.data["stopped_by"][0]["code"], "done")
        self.assertEqual(res.data["days_run"], 6.0)
        self.assertEqual(res.data["chunks"], 3)
        self.assertTrue(res.data["telemetry"])
        entries = loop.journal_for(self.session).entries()
        self.assertEqual([e["kind"] for e in entries], ["advance"])

    def test_stops_at_the_first_decision_point_at_the_end_of_its_chunk(self):
        clock = FakeClock(self.store, {103: {"techs": 11}})
        res = self.run_tool(clock, days=30)
        self.assertEqual([r["code"] for r in res.data["stopped_by"]], ["tech_done"])
        self.assertEqual(res.data["days_run"], 4.0)
        self.assertEqual(res.data["changes"], {"techs": [10, 11]})

    def test_stop_on_empty_list_ignores_everything_but_the_date(self):
        clock = FakeClock(self.store, {102: {"enemies": 1}})
        res = self.run_tool(clock, days=4, stop_on=[])
        self.assertEqual(res.data["stopped_by"][0]["code"], "done")
        self.assertEqual(res.data["changes"]["enemies"], [0, 1])

    def test_until_date_limits_the_run(self):
        clock = FakeClock(self.store)
        res = self.run_tool(clock, until="5 Jan 1936")
        self.assertEqual(res.data["days_run"], 4.0)
        with self.assertRaises(ToolError):
            self.run_tool(FakeClock(self.store), until="1 Jan 1930")
        with self.assertRaises(ToolError):
            self.run_tool(FakeClock(self.store), until="whenever")

    def test_popup_left_for_the_agent_stops_and_is_returned(self):
        waiting = {"kind": "event", "title": "Choose", "options": [{"text": "A"}, {"text": "B"}],
                   "answered": False}
        clock = FakeClock(self.store, popups={101: [waiting]})
        res = self.run_tool(clock, days=10)
        self.assertEqual(res.data["stopped_by"][0]["code"], "popup")
        self.assertEqual(res.data["events_waiting"][0]["title"], "Choose")

    def test_answered_popups_and_notices_are_reported_and_journaled(self):
        popups = {101: [{"kind": "news", "title": "Headline", "chosen_text": "Ok", "answered": True, "why": "policy"},
                        {"kind": "notice", "title": "National Focus Completed", "answered": "ok",
                         "date": "1936.1.2.0"}]}
        clock = FakeClock(self.store, popups=popups)
        res = self.run_tool(clock, days=10)
        self.assertEqual(res.data["answered"][0]["title"], "Headline")
        self.assertEqual(res.data["stopped_by"][0]["code"], "focus_done")
        kinds = [e["kind"] for e in loop.journal_for(self.session).entries()]
        self.assertEqual(sorted(kinds), ["advance", "popup"])

    def test_suggestions_follow_the_numbers(self):
        clock = FakeClock(self.store, {101: {"mil_free": 3}})
        res = self.run_tool(clock, days=2, stop_on=[])
        self.assertIn("hoi4_production", [s["tool"] for s in res.data["suggestions"]])

    def test_without_telemetry_only_the_date_and_popups_stop_a_run(self):
        self.session._store = TelemetryStore()
        clock = FakeClock(self.session._store, feed=False)
        res = self.run_tool(clock, days=4)
        self.assertFalse(res.data["telemetry"])
        self.assertIn("no telemetry", res.data["note"])
        self.assertEqual(res.data["stopped_by"][0]["code"], "done")

    def test_wall_clock_limit_is_reported(self):
        clock = FakeClock(self.store)
        orig = clock.run_days

        def slow(session, days, hours, policy, speed, max_seconds, *a, **k):
            rep = orig(session, days, hours, policy, speed, max_seconds)
            rep["timeout"] = True
            return rep
        clock.run_days = slow
        res = self.run_tool(clock, days=30)
        self.assertEqual(res.data["stopped_by"][0]["code"], "time_limit")


class ActionJournalTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_actions_")
        store = TelemetryStore()
        store.add(day(100, "1936.1.5.12"))
        self.session = FakeSession(self.tmp, store)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def result(self, summary, **data):
        return registry.ToolResult(summary, data)

    def test_verified_campaign_actions_are_journaled(self):
        loop.journal_action(self.session, "hoi4_research", "campaign",
                            {"action": "start", "tech": "basic_machine_tools", "limit": 5, "save": "latest"},
                            self.result("started Basic Machine Tools in slot 1", changed=True, verified_by="slot"))
        e = loop.journal_for(self.session).entries()
        self.assertEqual(len(e), 1)
        self.assertEqual(e[0]["kind"], "action")
        self.assertEqual(e[0]["date"], "1936.1.5.12")
        self.assertEqual(e[0]["data"]["args"], {"action": "start", "tech": "basic_machine_tools"})

    def test_reads_failures_and_other_families_are_not(self):
        loop.journal_action(self.session, "hoi4_research", "campaign", {"action": "list"}, self.result("12 techs"))
        loop.journal_action(self.session, "hoi4_focus", "campaign", {"action": "select"},
                            registry.ToolResult("failed", {"changed": True}, is_error=True))
        loop.journal_action(self.session, "hoi4_time", "time", {"action": "step"},
                            self.result("ran 5 days", changed=True))
        self.assertEqual(loop.journal_for(self.session).entries(), [])

    def test_journal_tool_add_read_plan_report(self):
        run = lambda **a: loop.hoi4_journal(self.session, dict({"action": "read", "kind": "note", "limit": 20,
                                                               "auto": True}, **a))
        res = run(action="add", kind="plan", text="Industry first")
        self.assertEqual(res.data["entry"]["date"], "1936.1.5.12")
        run(action="add", kind="decision", text="Declined the concession: political power is short")
        self.assertEqual(run(action="plan").summary, "Industry first")
        self.assertEqual(len(run(action="read", kinds=["decision"]).data["entries"]), 1)
        self.assertIn("Current plan", run(action="report").summary)
        self.assertEqual(run(action="summary").data["entries"], 2)
        with self.assertRaises(ToolError):
            run(action="add", kind="action", text="forged")
        with self.assertRaises(ToolError):
            run(action="add", kind="note", text="  ")

    def test_tools_are_registered_with_examples(self):
        for name in ("hoi4_advance", "hoi4_journal"):
            t = registry.get(name)
            self.assertIsNotNone(t)
            self.assertTrue(t.examples)
            self.assertEqual(t.family, "campaign_loop")


if __name__ == "__main__":
    unittest.main()
