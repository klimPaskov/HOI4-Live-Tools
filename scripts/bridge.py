"""Call bridge: keeps one hoi4-live server running and lets shell-based agents call its tools one by one.

    python scripts/bridge.py serve --port 8765 --call-log <calls.jsonl> [--image-dir <dir>]   (leave running)
    python scripts/bridge.py call hoi4_status '{"look": false}'  [--port 8765] [--timeout 1500]
    python scripts/bridge.py call hoi4_focus @args.json
    python scripts/bridge.py cancel  # request cancellation; original call still reports its result
    python scripts/bridge.py stop

The server is the real stdio MCP server (same protocol and tools as a coding client sees); the bridge only keeps
its process alive between shell invocations and prints each result as the text the client would see. Images are
saved under --image-dir and listed as `[image: <path>]`. Used for long campaigns whose calls are recorded with
HOI4_LIVE_CALL_LOG.
"""

import argparse
import json
import os
import sys
import threading
import time
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp_client import McpClient  # noqa: E402


def render(result):
    parts = []
    for c in result.get("content", []):
        if c.get("type") == "text":
            parts.append(c["text"])
        elif c.get("type") == "image":
            parts.append(f"[image: {c.get('file', 'not saved')}]")
    if result.get("isError"):
        parts.insert(0, "[ERROR]")
    return "\n".join(parts)


def serve(args):
    if args.call_log:
        os.environ["HOI4_LIVE_CALL_LOG"] = os.path.abspath(args.call_log)
    client = McpClient(image_dir=args.image_dir)
    client.__enter__()
    client.initialize()
    gate = threading.Lock()          # one call at a time: the game has one keyboard
    stop = threading.Event()

    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def do_POST(self):
            body = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))) or b"{}")
            if self.path == "/cancel":
                self._send(client.cancel_active_tool())
                return
            if self.path == "/stop":
                self._send({"text": "stopping"})
                stop.set()
                return
            try:
                with gate:
                    r = client.call(body["tool"], body.get("args") or {}, timeout=body.get("timeout", 1500),
                                    progress=False)
                self._send({"text": render(r), "isError": bool(r.get("isError"))})
            except Exception as e:
                self._send({"text": f"[BRIDGE ERROR] {type(e).__name__}: {e}", "isError": True})

        def _send(self, obj):
            raw = json.dumps(obj).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(raw)))
            self.end_headers()
            self.wfile.write(raw)

    httpd = ThreadingHTTPServer(("127.0.0.1", args.port), H)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    print(f"bridge on 127.0.0.1:{args.port}; call log {os.environ.get('HOI4_LIVE_CALL_LOG', '-')}", flush=True)
    while not stop.is_set():
        time.sleep(0.5)
    httpd.shutdown()
    client.__exit__(None, None, None)


def post(port, path, obj, timeout):
    req = urllib.request.Request(f"http://127.0.0.1:{port}{path}", data=json.dumps(obj).encode("utf-8"),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode("utf-8"))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("serve")
    s.add_argument("--port", type=int, default=8765)
    s.add_argument("--call-log")
    s.add_argument("--image-dir")
    c = sub.add_parser("call")
    c.add_argument("tool")
    c.add_argument("args", nargs="?", default="{}")
    c.add_argument("--port", type=int, default=8765)
    c.add_argument("--timeout", type=int, default=1500)
    for command in ("stop", "cancel"):
        st = sub.add_parser(command)
        st.add_argument("--port", type=int, default=8765)
    a = ap.parse_args()
    if a.cmd == "serve":
        serve(a)
    elif a.cmd == "stop":
        print(post(a.port, "/stop", {}, 10)["text"])
    elif a.cmd == "cancel":
        print(json.dumps(post(a.port, "/cancel", {}, 10)))
    else:
        raw = a.args
        if raw.startswith("@"):
            raw = open(raw[1:], encoding="utf-8").read()
        out = post(a.port, "/call", {"tool": a.tool, "args": json.loads(raw), "timeout": a.timeout}, a.timeout + 30)
        sys.stdout.reconfigure(encoding="utf-8")
        print(out["text"])
        sys.exit(1 if out.get("isError") else 0)


if __name__ == "__main__":
    main()
