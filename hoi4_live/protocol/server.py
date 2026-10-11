"""MCP server over stdio (JSON-RPC 2.0, one JSON message per line).

Handled methods: initialize, notifications/initialized, ping, tools/list, tools/call, notifications/cancelled,
resources/list, resources/templates/list and prompts/list (empty lists), logging/setLevel.

Threading: the main thread reads stdin and answers cheap requests (initialize, ping, tools/list) at once; tools/call
requests run one at a time on a worker thread, because they drive a single shared desktop. A long tool call therefore
never blocks pings, and a notifications/cancelled message sets the call's cancel flag, which long loops check.
stdout carries protocol messages only; logs go to stderr and the log file.
"""

import json
import logging
import queue
import sys
import threading

from .. import SERVER_NAME, __version__
from ..tools import dispatch, registry
from . import jsonrpc

log = logging.getLogger("hoi4_live.server")

SUPPORTED_VERSIONS = ("2025-06-18", "2025-03-26", "2024-11-05")

INSTRUCTIONS = (
    "HOI4 Live Tools drives a running Hearts of Iron IV on this Windows desktop. Start with hoi4_status. "
    "Coordinates are client pixels of the game window (0,0 = top-left of the game picture). Acting tools take a "
    "shared desktop lock and act only inside the HOI4 window; hold it across a multi-step run with hoi4_lock. "
    "Saves you create get the hoi4live_ prefix. Test-mode tools (console, events, eval_effect) are refused in "
    "campaign mode. Leave the game paused on the map when you finish."
)


class StdioServer:
    def __init__(self, session, stdin=None, stdout=None):
        self.session = session
        self.stdin = stdin or sys.stdin
        self.stdout = stdout or sys.stdout
        self.write_lock = threading.Lock()
        self.calls = queue.Queue()
        self.cancels = {}
        self.initialized = False
        self.protocol_version = SUPPORTED_VERSIONS[0]
        self.client_info = {}
        self.worker = threading.Thread(target=self._worker, name="tool-worker", daemon=True)

    # ------------------------------------------------------------------ output
    def send(self, msg):
        data = jsonrpc.encode(msg)
        with self.write_lock:
            self.stdout.write(data)
            self.stdout.flush()

    def notify(self, method, params=None):
        self.send(jsonrpc.notification(method, params))

    # ------------------------------------------------------------------ main loop
    def serve_forever(self):
        self.worker.start()
        while True:
            line = self.stdin.readline()
            if not line:
                break
            line = line.strip()
            if not line:
                continue
            try:
                msg = jsonrpc.decode(line)
            except jsonrpc.RpcError as e:
                self.send(jsonrpc.error(None, e.code, e.message))
                continue
            if isinstance(msg, list):
                if not msg:
                    self.send(jsonrpc.error(None, jsonrpc.INVALID_REQUEST, "empty batch"))
                    continue
                for m in msg:
                    self.handle(m)
            else:
                self.handle(msg)
        self.calls.put(None)
        self.worker.join(timeout=5)

    def handle(self, msg):
        try:
            kind, msg_id, method, params = jsonrpc.check_request(msg)
        except jsonrpc.RpcError as e:
            mid = msg.get("id") if isinstance(msg, dict) else None
            self.send(jsonrpc.error(mid, e.code, e.message))
            return
        if kind == "response":
            return                                           # we never send requests; ignore stray responses
        if kind == "notification":
            self.on_notification(method, params)
            return
        try:
            if method == "tools/call":
                self.enqueue_call(msg_id, params)
                return
            result = self.on_request(method, params)
            self.send(jsonrpc.response(msg_id, result))
        except jsonrpc.RpcError as e:
            self.send(jsonrpc.error(msg_id, e.code, e.message, e.data))
        except Exception as e:  # never die on a bad request
            log.exception("request %s failed", method)
            self.send(jsonrpc.error(msg_id, jsonrpc.INTERNAL_ERROR, f"{type(e).__name__}: {e}"))

    # ------------------------------------------------------------------ requests
    def on_request(self, method, params):
        if method == "initialize":
            requested = params.get("protocolVersion") if isinstance(params, dict) else None
            self.protocol_version = requested if requested in SUPPORTED_VERSIONS else SUPPORTED_VERSIONS[0]
            self.client_info = params.get("clientInfo", {}) if isinstance(params, dict) else {}
            log.info("initialize from %s (protocol %s)", self.client_info, self.protocol_version)
            return {
                "protocolVersion": self.protocol_version,
                "capabilities": {"tools": {"listChanged": False}, "logging": {}},
                "serverInfo": {"name": SERVER_NAME, "title": "HOI4 Live Tools", "version": __version__},
                "instructions": INSTRUCTIONS,
            }
        if method == "ping":
            return {}
        if method == "tools/list":
            return {"tools": [t.listing() for t in sorted(registry.all_tools(), key=lambda t: t.name)]}
        if method in ("resources/list",):
            return {"resources": []}
        if method == "resources/templates/list":
            return {"resourceTemplates": []}
        if method == "prompts/list":
            return {"prompts": []}
        if method == "logging/setLevel":
            level = str(params.get("level", "info")).upper()
            logging.getLogger("hoi4_live").setLevel(getattr(logging, level, logging.INFO))
            return {}
        raise jsonrpc.RpcError(jsonrpc.METHOD_NOT_FOUND, f"method not found: {method}")

    def on_notification(self, method, params):
        if method == "notifications/initialized":
            self.initialized = True
        elif method == "notifications/cancelled":
            rid = params.get("requestId") if isinstance(params, dict) else None
            ev = self.cancels.get(rid)
            if ev is not None:
                log.info("cancel requested for %s", rid)
                ev.set()
        # other notifications are ignored, as the spec requires

    # ------------------------------------------------------------------ tool calls
    def enqueue_call(self, msg_id, params):
        if not isinstance(params, dict) or not isinstance(params.get("name"), str):
            raise jsonrpc.RpcError(jsonrpc.INVALID_PARAMS, "tools/call needs params.name (string)")
        name = params["name"]
        if registry.get(name) is None:
            raise jsonrpc.RpcError(jsonrpc.INVALID_PARAMS, f"unknown tool: {name}")
        args = params.get("arguments")
        if args is not None and not isinstance(args, dict):
            raise jsonrpc.RpcError(jsonrpc.INVALID_PARAMS, "params.arguments must be an object")
        token = (params.get("_meta") or {}).get("progressToken")
        cancel = threading.Event()
        self.cancels[msg_id] = cancel
        self.calls.put((msg_id, name, args or {}, token, cancel))

    def _worker(self):
        while True:
            item = self.calls.get()
            if item is None:
                return
            msg_id, name, args, token, cancel = item
            progress = None
            if token is not None:
                counter = {"n": 0}

                def progress(message, value=None, total=None, _t=token):
                    counter["n"] += 1
                    p = {"progressToken": _t, "progress": value if value is not None else counter["n"],
                         "message": message}
                    if total is not None:
                        p["total"] = total
                    try:
                        self.notify("notifications/progress", p)
                    except Exception:
                        pass
            try:
                if cancel.is_set():
                    result = registry.ToolResult("cancelled before start", {"error": "cancelled"}, is_error=True).to_mcp()
                else:
                    result = dispatch.call(self.session, name, args, progress=progress, cancel=cancel)
                self.send(jsonrpc.response(msg_id, result))
            except Exception as e:
                log.exception("tool worker failure")
                self.send(jsonrpc.error(msg_id, jsonrpc.INTERNAL_ERROR, f"{type(e).__name__}: {e}"))
            finally:
                self.cancels.pop(msg_id, None)


def run(session):
    # Binary-safe, UTF-8 line IO on Windows regardless of the console code page.
    stdin = open(sys.stdin.fileno(), "r", encoding="utf-8", errors="replace", newline="\n", closefd=False)
    stdout = open(sys.stdout.fileno(), "w", encoding="utf-8", newline="\n", closefd=False)
    sys.stdout = sys.stderr          # stray prints from libraries must never corrupt the protocol stream
    server = StdioServer(session, stdin, stdout)
    try:
        server.serve_forever()
    finally:
        session.shutdown()
