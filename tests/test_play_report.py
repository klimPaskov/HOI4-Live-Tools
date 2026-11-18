"""`hoi4-live report`: the bar audit and the gate of a run, without a game.

tests/fixtures/run_g17e is a trimmed copy of a recorded Germany run (1936-1938): its action journal, feed,
runtime action lines, production samples, monitor log and the gate's reads at 1 Jan 1938. expected-audit.txt and
expected-gate.txt are what the developer harness printed for the same run; the product must print the same lines.
"""

import io
import json
import os
import shutil
import tempfile
import unittest

import helpers

from hoi4_live.play import audit, gate, review, runs

FIXTURE = os.path.join(helpers.ROOT, "tests", "fixtures", "run_g17e")
BASELINE = os.path.join(helpers.ROOT, "skills", "hoi4-campaign-playbook", "baselines", "GER-1936.json")


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def units():
    with open(os.path.join(FIXTURE, "units.json"), encoding="utf-8") as f:
        return {k: (v[0], set(v[1])) for k, v in json.load(f).items()}


def snapshot():
    with open(os.path.join(FIXTURE, "gate", "1938.01.01.11.json"), encoding="utf-8") as f:
        return json.load(f)


class AuditTests(unittest.TestCase):
    def test_the_audit_prints_what_the_harness_printed_for_the_same_run(self):
        want = read(os.path.join(FIXTURE, "expected-audit.txt")).splitlines()
        want = want[:next(i for i, line in enumerate(want) if line.startswith("E equipped"))]
        self.assertEqual(audit.audit_lines(FIXTURE, "g17e"), want)

    def test_a_product_run_reads_its_feed_samples_and_pace(self):
        run = tempfile.mkdtemp(prefix="hoi4live_audit_")
        self.addCleanup(shutil.rmtree, run, True)
        feed = [{"seq": 1, "kind": "paused_changed", "date": "1936.01.01.12", "detail": {}},
                {"seq": 2, "kind": "alert_raised", "date": "1936.01.02.00", "detail": {"alert": "free_dockyards"}},
                {"seq": 3, "kind": "research_slot_idle", "date": "1936.03.01.00", "detail": {}},
                {"seq": 4, "kind": "own_state_lost", "date": "1936.04.01.00", "detail": {"state_id": 9}}]
        with open(os.path.join(run, "feed.jsonl"), "w", encoding="utf-8") as f:
            f.write("".join(json.dumps(c) + "\n" for c in feed))
        prod = {"date": "1936.03.01", "mil": 30, "pp": 160.0,
                "lines": [[0, "military", "infantry_equipment_1", 10, 6, 50.0],
                          [1, "military", "infantry_equipment_1", 2, 2, 50.0]],
                "stock": [["infantry_equipment", 100, 50, 0, 10, 5.0]]}
        watch = [{"t": 0, "date": "1936.01.01.12", "paused": False, "armies": [{"army": "A", "supply": 40}]},
                 {"t": 60, "date": "1936.01.11.12", "paused": False, "armies": [], "prod": prod}]
        with open(os.path.join(run, "watch.jsonl"), "w", encoding="utf-8") as f:
            f.write("".join(json.dumps(w) + "\n" for w in watch))
        with open(os.path.join(run, "game-actions.jsonl"), "w", encoding="utf-8") as f:
            for n in range(2):
                f.write(json.dumps({"id": "a%d" % n, "action": {"kind": "construction_add", "state": 3},
                                    "status": "NotApplied"}) + "\n")
        text = "\n".join(audit.audit_lines(run, "p1"))
        self.assertIn("BAR AUDIT p1: feed to 1936.04.01, 2 actions, 2 watch samples", text)
        self.assertIn("repeated failures: 2 in 1 groups: 2x construction_add", text)
        self.assertIn("3 research: slot_idle events 1", text)
        self.assertIn("supply<50% samples {'A': 1}", text)
        self.assertIn("15 territory lost: 1 [('1936.04.01', 'own_state_lost', 9)]", text)
        self.assertIn("free_dockyards [NO REASON]", text)
        self.assertIn("pace: 10.0 game days per wall minute over 1 running minutes", text)
        self.assertIn("P1 duplicate military lines: max 1", text)
        self.assertIn("P6 political power on hand: max 160, last 160", text)
        self.assertNotIn("above 150", text)
        # the omissions come first, each with the days the game flagged it and its reason
        self.assertLess(text.index("OMISSIONS"), text.index("1 self-pauses"))
        self.assertIn("free_dockyards", text.split("1 self-pauses")[0])

    def test_omissions_name_a_false_reason_repeats_and_a_polling_player(self):
        run = tempfile.mkdtemp(prefix="hoi4live_omit_")
        self.addCleanup(shutil.rmtree, run, True)
        wake = {"reason": "alert:free_military_factories",
                "message": "game alert free_military_factories has stayed 15 game days; production: day 0: no free "
                           "military factory"}
        feed = [{"seq": 1, "kind": "wake_player", "date": "1936.02.01.00", "detail": wake},
                {"seq": 2, "kind": "alert_raised", "date": "1936.02.01.00", "detail": {"alert": "mio_traits"}},
                {"seq": 3, "kind": "focus_finished", "date": "1937.02.01.00", "detail": {}}]
        with open(os.path.join(run, "feed.jsonl"), "w", encoding="utf-8") as f:
            f.write("".join(json.dumps(c) + "\n" for c in feed))
        prod = {"date": "1936.06.01", "mil": 38, "pp": 40.0,
                "lines": [[0, "military", "infantry_equipment_1", 30, 30, 50.0]], "stock": []}
        with open(os.path.join(run, "watch.jsonl"), "w", encoding="utf-8") as f:
            f.write(json.dumps({"t": 0, "date": "1936.06.01.00", "paused": False, "prod": prod}) + "\n")
        with open(os.path.join(run, "plan.json"), "w", encoding="utf-8") as f:
            json.dump({"manage": {"alerts_ignore": ["mio_traits"]}}, f)
        with open(os.path.join(run, "game-actions.jsonl"), "w", encoding="utf-8") as f:
            for n in range(5):
                f.write(json.dumps({"id": "plan-m-%d" % n, "action": {"kind": "army_move", "army": "A",
                                                                       "province_id": 7},
                                    "status": "NotApplied"}) + "\n")
        calls = [{"tool": "hoi4_wait_change", "args": {"since": n}, "source": "player"} for n in range(6)]
        calls += [{"tool": "hoi4_overview", "args": {}, "source": "player"}] * 3
        calls += [{"tool": "hoi4_production_set", "args": {"line": 0}, "source": "player"},
                  {"tool": "hoi4_plan", "args": {"op": "get"}, "source": "player"}]
        with open(os.path.join(run, "calls.jsonl"), "w", encoding="utf-8") as f:
            f.write("".join(json.dumps(c) + "\n" for c in calls))
        text = "\n".join(audit.omission_lines(run))
        self.assertIn("free_military_factories 360 d", text)
        self.assertIn("FALSE REASON", text)
        self.assertIn("read: 8 free on 1936.06.01", text)
        self.assertIn("mio_traits 360 d", text)
        self.assertIn("HIDDEN by the plan's alerts_ignore", text)
        self.assertIn("executor repeated identical actions: 5 sends in 1 groups; worst: 5x army_move FAILED", text)
        self.assertIn("player calls 11: waits 6, overviews 3 (82% of the calls), actions 1", text)
        self.assertIn("3x hoi4_overview", text)


class GateTests(unittest.TestCase):
    def test_the_gate_prints_what_the_harness_printed_for_the_same_state(self):
        with open(os.path.join(FIXTURE, "plan.json"), encoding="utf-8") as f:
            plan = json.load(f)
        lines, (fill_ok, force_ok) = gate.gate_lines(snapshot(), plan=plan, units=units(), baseline=BASELINE)
        self.assertEqual(lines, read(os.path.join(FIXTURE, "expected-gate.txt")).splitlines())
        self.assertTrue(fill_ok)
        self.assertIsNone(force_ok)

    def test_without_a_baseline_the_counts_stand_without_marks(self):
        lines, _ = gate.gate_lines(snapshot(), units=units())
        self.assertIn("marks: none: no stored AI baseline for GER", lines[0])
        self.assertEqual(lines[1], "civilian 70")
        self.assertTrue(lines[3].startswith("divisions in armies 88 + unassigned 0: EQUIPPED ok"))
        self.assertFalse(any(line.startswith("path to") for line in lines))

    def test_marks_given_by_the_user_win(self):
        lines, _ = gate.gate_lines(snapshot(), units=units(), baseline=BASELINE, marks=(80, 30, 100))
        self.assertIn("(marks: given)", lines[0])
        self.assertEqual(lines[1], "civilian 70 / AI 80: BELOW")
        self.assertIn("/ AI 100: count BELOW; EQUIPPED BELOW", lines[3])

    def test_a_force_table_is_gated(self):
        plan = {"force_design": {"land": [{"template": "Panzer-Division", "count": 6, "ready": "1937.6.1"},
                                          {"role": "line", "count": 60, "ready": "1939.9.1"}]}}
        lines, (_, force_ok) = gate.gate_lines(snapshot(), plan=plan, units=units(), baseline=BASELINE)
        self.assertFalse(force_ok)
        self.assertIn("Panzer-Division 3/6 by 1937.6.1 BELOW", "\n".join(lines))
        self.assertIn("FORCE OFF the plan path (gate fails)", lines)

    def test_the_baseline_is_found_by_country_and_start_year(self):
        app = helpers.ROOT
        self.assertEqual(gate.find_baseline(app, "GER", "1936.1.1"), BASELINE)
        self.assertIsNone(gate.find_baseline(app, "ETH", "1936.1.1"))


class ReviewTests(unittest.TestCase):
    def setUp(self):
        self.prefix = tempfile.mkdtemp(prefix="hoi4live_review_")
        self.addCleanup(shutil.rmtree, self.prefix, True)
        os.makedirs(os.path.join(self.prefix, "app", "skills", "hoi4-campaign-playbook"))
        shutil.copytree(os.path.dirname(BASELINE),
                        os.path.join(self.prefix, "app", "skills", "hoi4-campaign-playbook", "baselines"))

    def segment(self, name, request, end=None):
        run = os.path.join(self.prefix, "runs", name)
        shutil.copytree(FIXTURE, run)
        runs.write_json(os.path.join(run, "request.json"), dict({"country": "German Reich", "tag": "GER",
                                                                 "goal": "the gate", "mode": "fair", "plan": "p",
                                                                 "start": "1936.1.1"}, **request))
        if end:
            runs.write_json(os.path.join(run, "end.json"), end)
        return run

    def test_the_last_run_is_read_with_its_segments_audit_and_kept_gate(self):
        first = self.segment("r1", {}, {"reason": "game_memory", "date": "1937.03.02.10", "saved": "Germany.hoi4"})
        second = self.segment("r2", {"continues": first, "segment": 2, "resume_date": "1937.03.02.10"},
                              {"reason": "end_date", "date": "1938.01.01.11", "saved": "Germany2.hoi4"})
        runs.write_json(os.path.join(self.prefix, "runs", "last.json"), {"run": second})
        out = io.StringIO()
        self.assertEqual(review.review(self.prefix, out=out), 0)
        text = out.getvalue()
        self.assertIn("CAMPAIGN German Reich (GER): the gate", text)
        self.assertIn('segment 1: r1 from 1936.1.1, ended game_memory at 1937.03.02.10, saved as "Germany.hoi4"',
                      text)
        self.assertIn("segment 2: r2 from 1937.03.02.10, ended end_date at 1938.01.01.11", text)
        self.assertIn("BAR AUDIT r2: feed to 1938.01.01", text)
        self.assertIn("GATE (from the state kept at the end of the run)", text)
        self.assertIn("civilian 70 / AI 64: ok", text)
        self.assertEqual(read(os.path.join(second, "audit.txt")), text)

    def test_a_live_run_is_read_from_the_running_game_without_a_player_call(self):
        run = self.segment("r1", {})
        runs.set_active(self.prefix, run, {}, os.getpid())
        sent = []

        class Conn:
            def __enter__(self):
                return self

            def __exit__(self, *a):
                return False

            def send(self, message):
                sent.append(message)

            def recv(self):
                return {"snapshot": dict(snapshot(), date="1938.02.01.00")}
        out = io.StringIO()
        self.assertEqual(review.review(self.prefix, out=out, connect=lambda prefix: Conn()), 0)
        self.assertEqual(sent, [{"op": "snapshot"}])
        self.assertIn("segment 1: r1 from 1936.1.1, running", out.getvalue())
        self.assertIn("GATE (from the running game)", out.getvalue())
        self.assertTrue(os.path.isfile(os.path.join(run, "gate", "1938.02.01.00.json")))

    def test_no_run_says_how_to_start_one(self):
        out = io.StringIO()
        self.assertEqual(review.review(self.prefix, out=out), 2)
        self.assertIn("hoi4-live play COUNTRY", out.getvalue())

    def test_the_installer_passes_the_report_options(self):
        import install
        args = install.build_parser().parse_args(["report", "r1", "--marks", "60", "30", "50", "--no-baseline"])
        self.assertEqual((args.command, args.value, args.marks, args.no_baseline), ("report", "r1", [60, 30, 50],
                                                                                    True))
        args = install.build_parser().parse_args(["play", "--continue", "--until", "1939.09.01"])
        self.assertEqual((args.continue_run, args.until), ("last", "1939.09.01"))


if __name__ == "__main__":
    unittest.main()
