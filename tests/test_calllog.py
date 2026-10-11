"""Call recorder: argument and result summaries, the dispatcher hook and hoi4_lock action=record."""

import json
import os
import tempfile
import unittest

import helpers

from hoi4_live.tools import calllog, dispatch, registry
import hoi4_live.tools  # noqa: F401  (registers every tool)


class SummaryTests(unittest.TestCase):
    def test_args_are_short(self):
        s = calllog.summarize_args({"text": "x" * 500, "n": 3, "flag": True, "items": [1, 2, 3, 4], "few": ["a", "b"],
                                    "d": {"k": "v"}})
        self.assertLess(len(s["text"]), 130)
        self.assertEqual(s["n"], 3)
        self.assertEqual(s["items"], "[4 items]")
        self.assertEqual(s["few"], ["a", "b"])

    def test_result_summary_keeps_first_line_and_fields(self):
        mcp = {"content": [{"type": "text", "text": "advanced 10 days\n{\n \"stopped_by\": \"focus_done\"\n}"},
                           {"type": "image", "data": "AAAA", "mimeType": "image/png"}],
               "structuredContent": {"stopped_by": "focus_done", "big": list(range(100))}, "isError": False}
        out = calllog.summarize_result(mcp)
        self.assertEqual(out["summary"], "advanced 10 days")
        self.assertEqual(out["data"], {"stopped_by": "focus_done"})
        self.assertEqual(out["images"], 1)


class RecordTests(unittest.TestCase):
    def test_record_appends_lines(self):
        d = tempfile.mkdtemp()
        path = os.path.join(d, "sub", "calls.jsonl")
        mcp = {"content": [{"type": "text", "text": "ok"}], "isError": False}
        self.assertTrue(calllog.record(path, "hoi4_status", {"look": False}, mcp, 100.0, 101.5))
        self.assertTrue(calllog.record(path, "hoi4_time", {"days": 2}, dict(mcp, isError=True), 102.0, 102.1))
        rows = [json.loads(x) for x in open(path, encoding="utf-8")]
        self.assertEqual([r["tool"] for r in rows], ["hoi4_status", "hoi4_time"])
        self.assertEqual(rows[0]["elapsed_s"], 1.5)
        self.assertTrue(rows[0]["ok"])
        self.assertFalse(rows[1]["ok"])
        self.assertFalse(calllog.record("", "x", {}, mcp, 0, 1))

    def test_dispatcher_and_lock_action(self):
        session = helpers.make_session()
        d = tempfile.mkdtemp()
        path = os.path.join(d, "calls.jsonl")
        r = dispatch.call(session, "hoi4_lock", {"action": "record", "path": path})
        self.assertFalse(r["isError"], r)
        dispatch.call(session, "hoi4_lock", {"action": "status"})
        dispatch.call(session, "hoi4_lock", {"action": "record", "path": ""})
        dispatch.call(session, "hoi4_lock", {"action": "status"})        # not recorded
        rows = [json.loads(x) for x in open(path, encoding="utf-8")]
        self.assertEqual([r["args"].get("action") for r in rows], ["record", "status"])
        self.assertEqual(session.call_log_path, "")

    def test_record_needs_path(self):
        session = helpers.make_session()
        r = dispatch.call(session, "hoi4_lock", {"action": "record"})
        self.assertTrue(r["isError"])


if __name__ == "__main__":
    unittest.main()
