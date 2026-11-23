"""The API budget ledger and the monitor's own model calls (hoi4_live/observe/budget.py, answer.py)."""
import json
import os
import shutil
import sys
import tempfile
import types
import unittest
from pathlib import Path
from unittest import mock

from hoi4_live.observe import answer, budget
from hoi4_live.observe.escalation import Escalator
from hoi4_live.observe.issues import Detector

KEY = "sk-test-secret-value"


def fake_anthropic(calls, usage=(1000, 200)):
    """A stand-in for the SDK: records each request, answers with fixed usage."""
    mod = types.ModuleType("anthropic")

    class APIError(Exception):
        pass

    class APIStatusError(APIError):
        status_code = 500

    class Messages:
        def create(self, **kw):
            calls.append(kw)
            u = types.SimpleNamespace(input_tokens=usage[0], output_tokens=usage[1], cache_read_input_tokens=0,
                                      cache_creation_input_tokens=0)
            block = types.SimpleNamespace(type="text", text='{"kind":"leave","why":"it clears itself"}')
            return types.SimpleNamespace(usage=u, content=[block], stop_reason="end_turn")

    class Anthropic:
        def __init__(self, api_key=None, **kw):
            assert api_key == KEY
            self.messages = Messages()

    mod.Anthropic, mod.APIError, mod.APIStatusError = Anthropic, APIError, APIStatusError
    return mod


class BudgetTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)
        (self.tmp / "secrets").mkdir()
        self.key = self.tmp / "secrets" / "anthropic.key"
        self.key.write_text(KEY + "\n")
        env = mock.patch.dict(os.environ, {"HOI4_LIVE_ANTHROPIC_KEY_FILE": str(self.key)})
        env.start()
        self.addCleanup(env.stop)
        os.environ.pop("HOI4_LIVE_API_LEDGER", None)
        (self.tmp / "secrets" / "budget.json").write_text(json.dumps(
            {"total": 200, "run_cap": 15, "day_cap": 40, "floor": 20}))

    def ledger(self):
        return json.loads((self.tmp / "secrets" / "api-ledger.json").read_text())

    def test_the_ledger_sits_beside_the_key_and_adds_up(self):
        b = budget.Budget()
        usd = b.record("r1", "escalation", "claude-haiku-5-5", {"input_tokens": 100_000, "output_tokens": 0})
        self.assertAlmostEqual(usd, 0.01, places=4)
        b.record("r2", "review", "claude-opus-5-5", {"input_tokens": 0, "output_tokens": 100_000})
        d = self.ledger()
        self.assertAlmostEqual(d["spent"], 0.01 + 2.0, places=3)
        self.assertFalse(d["prices"]["verified"])
        self.assertIn("r1", d["runs"])
        line = b.line("r2")
        self.assertIn("this run $2.00", line)
        self.assertIn("remaining $197.99 of $200", line)
        self.assertNotIn(KEY, json.dumps(d))

    def test_run_day_and_floor_caps(self):
        b = budget.Budget()
        self.assertEqual(b.check("r1")[0], budget.OK)
        b.record("r1", "player", "claude-opus-5-5", {"output_tokens": 800_000})  # $16
        self.assertEqual(b.check("r1")[0], budget.RUN_CAP)
        self.assertEqual(b.check("r2")[0], budget.OK)
        b.record("r2", "player", "claude-opus-5-5", {"output_tokens": 700_000})  # $14
        b.record("r3", "player", "claude-opus-5-5", {"output_tokens": 600_000})  # $12 -> day $42
        code, why = b.check("r4")
        self.assertEqual(code, budget.DAY_CAP)
        self.assertIn("day cap", why)
        (self.tmp / "secrets" / "budget.json").write_text(json.dumps({"total": 50, "day_cap": 1000, "floor": 20}))
        self.assertEqual(b.check("r4")[0], budget.FLOOR)

    def test_the_player_role_is_replaced_from_its_stream_not_added_twice(self):
        b = budget.Budget()
        rows = {"claude-haiku-5-5": {"calls": 3, "input": 0, "output": 0, "cache_read": 0, "cache_write": 0,
                                     "usd": 1.5}}
        b.set_role("r1", "player", rows)
        b.set_role("r1", "player", rows)
        s = b.status("r1")
        self.assertAlmostEqual(s["run"], 1.5)
        self.assertAlmostEqual(s["today"], 1.5)

    def test_player_rows_and_key_source_from_a_stream(self):
        run = self.tmp / "run"
        run.mkdir()
        lines = [{"type": "system", "subtype": "init", "apiKeySource": "ANTHROPIC_API_KEY"},
                 {"type": "assistant", "message": {"id": "m1", "model": "claude-haiku-5-5",
                                                   "usage": {"input_tokens": 1000, "output_tokens": 100}}},
                 {"type": "assistant", "message": {"id": "m1", "model": "claude-haiku-5-5",
                                                   "usage": {"input_tokens": 1000, "output_tokens": 100}}}]
        (run / "player.out").write_text("\n".join(json.dumps(x) for x in lines))
        self.assertTrue(budget.player_billed_to_key(str(run)))
        rows = budget.player_rows(str(run))
        self.assertEqual(rows["claude-haiku-5-5"]["calls"], 1)
        lines[0]["apiKeySource"] = "none"
        (run / "player.out").write_text("\n".join(json.dumps(x) for x in lines))
        self.assertFalse(budget.player_billed_to_key(str(run)))

    def test_a_call_is_bounded_checked_first_and_recorded(self):
        calls = []
        with mock.patch.dict(sys.modules, {"anthropic": fake_anthropic(calls)}):
            out = answer.ask("ESCALATION I0001 ...", tier=1, run="r1")
            self.assertEqual(out["model"], "claude-haiku-5-5")
            self.assertEqual(calls[0]["max_tokens"], answer.LIMITS[1]["max_tokens"])
            self.assertEqual(calls[0]["output_config"], {"effort": "low"})
            self.assertEqual(self.ledger()["runs"]["r1"]["escalation"]["claude-haiku-5-5"]["calls"], 1)
            # over the run's cap: no call at all
            budget.Budget().record("r1", "player", "claude-opus-5-5", {"output_tokens": 800_000})
            out = answer.ask("ESCALATION I0002 ...", tier=1, run="r1")
            self.assertTrue(out["skipped"] and out["error"].startswith("budget:"))
            self.assertEqual(len(calls), 1)
        self.assertNotIn(KEY, json.dumps(out))

    def test_without_a_key_file_nothing_is_called(self):
        os.environ["HOI4_LIVE_ANTHROPIC_KEY_FILE"] = str(self.tmp / "missing.key")
        self.assertIn("no API key file", answer.available())
        self.assertTrue(answer.ask("x")["skipped"])

    def test_an_escalation_on_the_key_logs_its_tokens_and_money(self):
        det = Detector(str(self.tmp))
        f = {"code": "province_overstacked", "severity": "high", "text": "63 divisions on 4 provinces",
             "subject": "1", "numbers": {}}
        det.snapshot({"date": "1940.05.10.00", "wars": {"at_war": True}, "findings": [f]})
        det.snapshot({"date": "1940.05.20.00", "wars": {"at_war": True}, "findings": [f]})
        it = det.issues["province_overstacked:1"]
        calls = []
        with mock.patch.dict(sys.modules, {"anthropic": fake_anthropic(calls)}):
            rec = Escalator(str(self.tmp), run_name="r1").consider("overdue", it, det.date, True)
            self.assertIsNone(Escalator(str(self.tmp / "x"), run_name="r1", live=False).consider(
                "overdue", dict(it, code="losing_exchange"), det.date, True).get("answered"))
        self.assertTrue(rec["answered"])
        self.assertEqual(rec["answer"]["kind"], "leave")
        self.assertEqual(len(calls), 1)  # the replay (live=False) asked nothing
        cost = [json.loads(x) for x in (self.tmp / "escalation_costs.jsonl").read_text().splitlines()]
        self.assertEqual(cost[0]["role"], "escalation")
        self.assertGreater(cost[0]["usd"], 0)


if __name__ == "__main__":
    unittest.main()
