"""Cancellation is an MCP notification; it never kills or takes over a game."""
import io
import sys
import threading
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from mcp_client import McpClient


class BridgeCancelTests(unittest.TestCase):
    def setUp(self):
        self.client = McpClient()
        self.sent = []
        self.ready = threading.Event()
        def sent(msg):
            self.sent.append(msg)
            if msg.get('method') == 'tools/call':
                self.ready.set()
        self.client.send = sent

    def test_cancel_targets_pending_call_and_waits_for_terminal_response(self):
        results = []
        thread = threading.Thread(target=lambda: results.append(self.client.request('tools/call', {}, timeout=2)))
        thread.start()
        self.assertTrue(self.ready.wait(1))
        with self.client.write_lock:
            pass
        result = self.client.cancel_active_tool()
        self.assertTrue(result['cancel_requested'])
        self.assertTrue(thread.is_alive())
        self.assertEqual(self.sent[-1]['method'], 'notifications/cancelled')
        self.assertEqual(self.sent[-1]['params']['requestId'], self.sent[0]['id'])
        with self.client.cv:
            self.client.responses[result['request_id']] = {'result': {'cancelled': True}}
            self.client.cv.notify_all()
        thread.join(1)
        self.assertFalse(thread.is_alive())
        self.assertEqual(results, [{'result': {'cancelled': True}}])
        self.assertFalse(self.client.cancel_active_tool()['cancel_requested'])

    def test_no_unique_tool_does_not_cancel_initialization_or_ambiguous_requests(self):
        for active in ({}, {1: 'initialize'}, {1: 'tools/call', 2: 'tools/call'}):
            self.client.active_requests = active
            self.assertFalse(self.client.cancel_active_tool()['cancel_requested'])
        self.assertEqual(self.sent, [])

    def test_timeout_requests_cooperative_cancel_and_retains_pending_identity(self):
        with self.assertRaises(TimeoutError):
            self.client.request('tools/call', {}, timeout=0)
        self.assertEqual(self.sent[-1]['method'], 'notifications/cancelled')
        self.assertEqual(self.client.active_requests, {1: 'tools/call'})
        self.assertTrue(self.client.cancel_active_tool()['cancel_requested'])

    def test_initialize_timeout_never_sends_cancellation(self):
        with self.assertRaises(TimeoutError):
            self.client.request('initialize', {}, timeout=0)
        self.assertEqual(len(self.sent), 1)
        self.assertEqual(self.client.active_requests, {})

    def test_reader_clears_timed_out_identity_on_actual_response(self):
        self.client.active_requests = {1: 'tools/call'}
        self.client.proc = SimpleNamespace(stdout=io.BytesIO(b'{"jsonrpc":"2.0","id":1,"result":{}}\n'))
        self.client._read()
        self.assertEqual(self.client.active_requests, {})
        self.assertEqual(self.client.responses[1]['result'], {})

    def test_concurrent_writes_remain_complete_json_lines(self):
        import json
        client = McpClient()
        stream = io.BytesIO()
        stream.flush = mock.Mock()
        client.proc = SimpleNamespace(stdin=stream)
        threads = [threading.Thread(target=client.send, args=({'id': n, 'payload': 'x' * 2000},))
                   for n in range(8)]
        for thread in threads: thread.start()
        for thread in threads: thread.join(1)
        self.assertEqual({json.loads(row)['id'] for row in stream.getvalue().splitlines()}, set(range(8)))


if __name__ == '__main__':
    unittest.main()
