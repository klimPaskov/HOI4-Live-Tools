"""Minimal MCP stdio client used by the exit checks and for manual testing.

    from mcp_client import McpClient
    with McpClient() as c:
        c.initialize()
        print(c.call("hoi4_status", {"look": False})["content"][0]["text"])

Images in results are written to `image_dir` (if given) and replaced by their file path in the returned dict
under content[i]["file"], so transcripts stay small.
"""

import base64
import json
import os
import queue
import subprocess
import sys
import threading
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


class McpClient:
    def __init__(self, command=None, image_dir=None, transcript=None):
        self.command = command or [sys.executable, "-m", "hoi4_live"]
        self.image_dir = image_dir
        self.transcript = transcript
        self.next_id = 1
        self.responses = {}
        self.notifications = []
        self.cv = threading.Condition()
        self.proc = None
        self.image_count = 0

    def __enter__(self):
        self.proc = subprocess.Popen(self.command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, cwd=ROOT)
        threading.Thread(target=self._read, daemon=True).start()
        threading.Thread(target=self._drain, daemon=True).start()
        return self

    def __exit__(self, *exc):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=20)
        except Exception:
            self.proc.kill()

    def _log(self, direction, msg):
        if self.transcript:
            with open(self.transcript, "a", encoding="utf-8") as f:
                f.write(json.dumps({"t": round(time.time(), 2), "dir": direction, "msg": msg}) + "\n")

    def _read(self):
        for raw in self.proc.stdout:
            msg = json.loads(raw.decode("utf-8"))
            if "id" in msg and ("result" in msg or "error" in msg):
                with self.cv:
                    self.responses[msg["id"]] = msg
                    self.cv.notify_all()
            else:
                self.notifications.append(msg)
                self._log("server->client", msg)

    def _drain(self):
        for raw in self.proc.stderr:
            sys.stderr.write("[server] " + raw.decode("utf-8", "replace"))

    def send(self, msg):
        self._log("client->server", msg)
        self.proc.stdin.write((json.dumps(msg) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, method, params=None, timeout=1200):
        rid = self.next_id
        self.next_id += 1
        msg = {"jsonrpc": "2.0", "id": rid, "method": method}
        if params is not None:
            msg["params"] = params
        self.send(msg)
        deadline = time.monotonic() + timeout
        with self.cv:
            while rid not in self.responses:
                left = deadline - time.monotonic()
                if left <= 0:
                    raise TimeoutError(f"{method} timed out")
                self.cv.wait(left)
            resp = self.responses.pop(rid)
        resp = self._strip_images(resp)
        self._log("server->client", resp)
        return resp

    def _strip_images(self, resp):
        res = resp.get("result")
        if not isinstance(res, dict) or "content" not in res:
            return resp
        out = json.loads(json.dumps(resp))
        for c in out["result"]["content"]:
            if c.get("type") == "image":
                self.image_count += 1
                if self.image_dir:
                    os.makedirs(self.image_dir, exist_ok=True)
                    ext = "png" if c["mimeType"] == "image/png" else "jpg"
                    path = os.path.join(self.image_dir, f"mcp_image_{self.image_count:02d}.{ext}")
                    with open(path, "wb") as f:
                        f.write(base64.b64decode(c["data"]))
                    c["file"] = path
                c["data"] = f"<{len(c['data'])} base64 chars>"
        return out

    def initialize(self):
        r = self.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                        "clientInfo": {"name": "hoi4-live-exit-check", "version": "1"}})
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        return r["result"]

    def call(self, name, args=None, timeout=1500, progress=True):
        params = {"name": name, "arguments": args or {}}
        if progress:
            params["_meta"] = {"progressToken": f"p{self.next_id}"}
        resp = self.request("tools/call", params, timeout)
        if "error" in resp:
            raise RuntimeError(f"{name}: JSON-RPC error {resp['error']}")
        return resp["result"]
