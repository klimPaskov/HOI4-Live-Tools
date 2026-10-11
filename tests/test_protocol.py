"""MCP stdio protocol: framing, lifecycle, tools/list, tools/call, errors, batches, cancellation."""

import io
import json
import threading
import time
import unittest

from helpers import make_session

from hoi4_live import tools  # noqa: F401  (registers tools)
from hoi4_live.protocol import jsonrpc
from hoi4_live.protocol.server import StdioServer
from hoi4_live.tools import registry


class Pipe(io.StringIO):
    """stdout replacement that collects lines."""

    def lines(self):
        return [json.loads(l) for l in self.getvalue().splitlines() if l.strip()]


def run_server(messages, session=None, wait=0.0):
    session = session or make_session()
    inp = io.StringIO("".join((m if isinstance(m, str) else json.dumps(m)) + "\n" for m in messages))
    out = Pipe()
    srv = StdioServer(session, inp, out)
    srv.serve_forever()
    if wait:
        time.sleep(wait)
    return out.lines(), session


def by_id(responses):
    return {r.get("id"): r for r in responses}


INIT = {"jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}}


class ProtocolTests(unittest.TestCase):
    def test_initialize_and_ping(self):
        out, _ = run_server([INIT, {"jsonrpc": "2.0", "method": "notifications/initialized"},
                             {"jsonrpc": "2.0", "id": 2, "method": "ping"}])
        r = by_id(out)
        self.assertEqual(r[1]["result"]["protocolVersion"], "2025-06-18")
        self.assertIn("tools", r[1]["result"]["capabilities"])
        self.assertEqual(r[1]["result"]["serverInfo"]["name"], "hoi4-live-tools")
        self.assertEqual(r[2]["result"], {})
        self.assertEqual(len(out), 2, "notifications must not be answered")

    def test_unknown_protocol_version_falls_back(self):
        msg = json.loads(json.dumps(INIT))
        msg["params"]["protocolVersion"] = "1999-01-01"
        out, _ = run_server([msg])
        self.assertEqual(out[0]["result"]["protocolVersion"], "2025-06-18")

    def test_tools_list_has_schemas(self):
        out, _ = run_server([INIT, {"jsonrpc": "2.0", "id": 2, "method": "tools/list"}])
        tools_ = by_id(out)[2]["result"]["tools"]
        names = {t["name"] for t in tools_}
        for n in ("hoi4_status", "hoi4_screenshot", "hoi4_console", "hoi4_time", "hoi4_popups", "hoi4_save_query",
                  "hoi4_scenario", "hoi4_lock", "hoi4_load", "hoi4_save", "hoi4_fire_event", "hoi4_logs"):
            self.assertIn(n, names)
        for t in tools_:
            self.assertEqual(t["inputSchema"]["type"], "object")
            self.assertIn("description", t)
            self.assertIn("readOnlyHint", t["annotations"])

    def test_call_validation_error_names_field(self):
        out, _ = run_server([INIT, {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                                    "params": {"name": "hoi4_time", "arguments": {"action": "fly"}}},
                             {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                              "params": {"name": "hoi4_lock", "arguments": {"bogus": 1}}},
                             {"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                              "params": {"name": "hoi4_save", "arguments": {}}}])
        r = by_id(out)
        self.assertTrue(r[2]["result"]["isError"])
        self.assertEqual(r[2]["result"]["structuredContent"]["field"], "arguments.action")
        self.assertIn("arguments.bogus", r[3]["result"]["content"][0]["text"])
        self.assertEqual(r[4]["result"]["structuredContent"]["field"], "arguments.name")

    def test_call_ok_returns_text_and_structured(self):
        out, _ = run_server([INIT, {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                                    "params": {"name": "hoi4_lock", "arguments": {"action": "status"}}}])
        res = by_id(out)[2]["result"]
        self.assertFalse(res["isError"])
        self.assertEqual(res["content"][0]["type"], "text")
        self.assertEqual(res["structuredContent"]["mode"], "test")

    def test_unknown_tool_and_method(self):
        out, _ = run_server([INIT, {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "nope"}},
                             {"jsonrpc": "2.0", "id": 3, "method": "no/such/method"}])
        r = by_id(out)
        self.assertEqual(r[2]["error"]["code"], jsonrpc.INVALID_PARAMS)
        self.assertEqual(r[3]["error"]["code"], jsonrpc.METHOD_NOT_FOUND)

    def test_parse_error_and_invalid_request(self):
        out, _ = run_server(["{not json", {"jsonrpc": "1.0", "id": 9, "method": "ping"}, {"id": 10}])
        codes = [r["error"]["code"] for r in out]
        self.assertIn(jsonrpc.PARSE_ERROR, codes)
        self.assertIn(jsonrpc.INVALID_REQUEST, codes)

    def test_batch(self):
        out, _ = run_server([json.dumps([{"jsonrpc": "2.0", "id": 1, "method": "ping"},
                                         {"jsonrpc": "2.0", "id": 2, "method": "ping"}])])
        self.assertEqual(sorted(r["id"] for r in out), [1, 2])

    def test_mode_refusal(self):
        session = make_session()
        session.mode = "campaign"
        out, _ = run_server([INIT, {"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                                    "params": {"name": "hoi4_console", "arguments": {"commands": ["time"]}}}],
                            session=session)
        res = by_id(out)[2]["result"]
        self.assertTrue(res["isError"])
        self.assertIn("campaign mode", res["content"][0]["text"])

    def test_cancellation_flag_reaches_tool(self):
        seen = {}

        @registry.tool("test_wait_for_cancel", "test only", {"properties": {}}, family="test", lock="none")
        def _wait(session, args):
            for _ in range(100):
                try:
                    session.check_cancel()
                except Exception as e:
                    seen["cancelled"] = type(e).__name__
                    raise
                time.sleep(0.02)
            return "finished"

        try:
            session = make_session()
            inp_r, inp_w = io.StringIO(), None
            out = Pipe()
            srv = StdioServer(session, io.StringIO(""), out)
            srv.worker.start()
            srv.handle({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "test_wait_for_cancel"}})
            time.sleep(0.2)
            srv.handle({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7}})
            for _ in range(100):
                if out.getvalue():
                    break
                time.sleep(0.05)
            srv.calls.put(None)
            res = out.lines()[0]["result"]
            self.assertTrue(res["isError"])
            self.assertEqual(seen.get("cancelled"), "Cancelled")
        finally:
            registry._TOOLS.pop("test_wait_for_cancel", None)

    def test_encode_is_single_line(self):
        line = jsonrpc.encode({"jsonrpc": "2.0", "id": 1, "result": {"text": "a\nbé"}})
        self.assertEqual(line.count("\n"), 1)
        self.assertTrue(line.endswith("\n"))
        self.assertEqual(json.loads(line)["result"]["text"], "a\nbé")


if __name__ == "__main__":
    unittest.main()
