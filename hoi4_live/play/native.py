"""One native runtime process, spoken to as an MCP server over its standard streams (JSON lines).

Replies may come back out of order (the runtime answers reads on their own lane while an action runs), so one
reader thread hands each reply to the request waiting for its id. A request never waits forever: when the
process has exited, the call answers with `process_exited`.
"""

import json
import subprocess
import threading

PROTOCOL = "2024-11-05"


class NativeError(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code
        self.message = message


class Native:
    def __init__(self, args, stderr_path, popen=subprocess.Popen):
        self._stderr = open(stderr_path, "ab")
        self.process = popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self._stderr)
        self._lock = threading.Lock()
        self._seq = 0
        self._waiting = {}
        self._reader = threading.Thread(target=self._read, daemon=True)
        self._reader.start()

    def _read(self):
        for raw in iter(self.process.stdout.readline, b""):
            try:
                message = json.loads(raw)
            except ValueError:
                continue
            slot = self._waiting.get(message.get("id")) if isinstance(message, dict) else None
            if slot:
                slot[1] = message
                slot[0].set()

    def alive(self):
        return self.process.poll() is None

    def rpc(self, method, params, timeout=None):
        with self._lock:
            self._seq += 1
            rid = self._seq
            slot = self._waiting[rid] = [threading.Event(), None]
            try:
                self.process.stdin.write((json.dumps({"jsonrpc": "2.0", "id": rid, "method": method,
                                                      "params": params}) + "\n").encode())
                self.process.stdin.flush()
            except (OSError, ValueError):
                self._waiting.pop(rid, None)
                raise NativeError("process_exited", "the native runtime is not running")
        waited = 0.0
        while not slot[0].wait(0.5):
            waited += 0.5
            if not self.alive():
                self._waiting.pop(rid, None)
                raise NativeError("process_exited", "the native runtime exited (%s)" % self.process.returncode)
            if timeout is not None and waited >= timeout:
                self._waiting.pop(rid, None)
                raise NativeError("timeout", "%s did not answer within %s s" % (method, timeout))
        self._waiting.pop(rid, None)
        reply = slot[1]
        if "error" in reply:
            raise NativeError("server_error", str((reply["error"] or {}).get("message")))
        return reply.get("result") or {}

    def notify(self, method):
        with self._lock:
            self.process.stdin.write((json.dumps({"jsonrpc": "2.0", "method": method}) + "\n").encode())
            self.process.stdin.flush()

    def initialize(self, timeout=60):
        result = self.rpc("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                         "clientInfo": {"name": "hoi4-live-play", "version": "1"}}, timeout)
        self.notify("notifications/initialized")
        return result

    def tools(self, timeout=60):
        return self.rpc("tools/list", {}, timeout).get("tools") or []

    def call(self, name, arguments, timeout=None):
        """(is_error, body): the tool's JSON body, or {"error": {"code", "message"}}."""
        try:
            result = self.rpc("tools/call", {"name": name, "arguments": arguments}, timeout)
        except NativeError as exc:
            return True, {"error": {"code": exc.code, "message": exc.message}}
        content = result.get("content") or []
        text = content[0].get("text") if content and isinstance(content[0], dict) else None
        try:
            body = json.loads(text) if text is not None else result
        except ValueError:
            body = {"text": text}
        return bool(result.get("isError")), body

    def close(self, timeout=10):
        try:
            self.process.stdin.close()
        except (OSError, ValueError):
            pass
        try:
            self.process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
        self._stderr.close()
        self._reader.join(timeout=5)
        try:
            self.process.stdout.close()
        except (OSError, ValueError):
            pass
        return self.process.returncode
