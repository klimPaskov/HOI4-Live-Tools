"""The hoi4-live-play MCP server: python -m hoi4_live.play.proxy --prefix INSTALL (what the coding client starts).

It lists the native runtime's tools as the player sees them (no lease token; request ids optional) plus hoi4_batch
and hoi4_run, and forwards each call to the supervisor of the active run over its private endpoint. The list comes
from the running supervisor, or from the runtime's offline catalogue (`hoi4-live-native --list-tools`) before any
run exists, so the client always sees the same tools. Without an active run a call answers with the command that
starts one. Each call runs on its own thread, so a long action never holds up a read.
"""

import argparse
import json
import os
import subprocess
import sys
import threading

from . import runs, supervisor

NAME = "hoi4-live-play"
PROTOCOL = "2024-11-05"
INSTRUCTIONS = ("Hearts of Iron IV through the native runtime. A campaign is started in a terminal with "
                "`hoi4-live play <COUNTRY> \"<goal>\"`; it prints the one instruction to follow (read the run's "
                "BRIEF.txt). hoi4_run {\"op\":\"status\"} shows the active run.")
NO_RUN = ("No campaign is running. In a terminal run: hoi4-live play <COUNTRY> \"<goal>\" (for example hoi4-live "
          "play ITA \"conquer Ethiopia before 1937\"); it attaches to your game and prints what to do next.")


def native_binary(prefix):
    return os.path.join(prefix, "native", "hoi4-live-native" + (".exe" if os.name == "nt" else ""))


def offline_tools(prefix, run=subprocess.run):
    """The runtime's own catalogue as the player sees it; only hoi4_batch and hoi4_run without a runtime."""
    binary = native_binary(prefix)
    listed = []
    if os.path.isfile(binary):
        try:
            done = run([binary, "--list-tools"], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                       stderr=subprocess.DEVNULL, timeout=60)
            listed = json.loads(done.stdout.decode("utf-8", errors="replace")).get("tools") or []
        except (OSError, ValueError, subprocess.TimeoutExpired, AttributeError):
            listed = []
    return supervisor.player_tools(listed)


def connect(prefix):
    """A connection to the active run's supervisor, or None."""
    active = runs.active_run(prefix)
    if not active:
        return None
    endpoint = active.get("endpoint") or {}
    try:
        from multiprocessing import AuthenticationError
        from multiprocessing.connection import Client
        return Client(endpoint["address"], family=endpoint.get("family"),
                      authkey=bytes.fromhex(endpoint["authkey"]))
    except (OSError, KeyError, ValueError, EOFError, AuthenticationError):
        return None


def result(is_error, body):
    return {"content": [{"type": "text", "text": json.dumps(body, ensure_ascii=False)}], "isError": bool(is_error)}


class Proxy:
    def __init__(self, prefix, connect=connect, catalogue=offline_tools):
        self.prefix = prefix
        self.connect = connect
        self.catalogue = catalogue
        self._offline = None

    def ask(self, message):
        conn = self.connect(self.prefix)
        if conn is None:
            return None
        try:
            with conn:
                conn.send(message)
                return conn.recv()
        except (OSError, EOFError):
            return None

    def tools(self):
        live = self.ask({"op": "tools"})
        if live and live.get("tools"):
            return live["tools"]
        if self._offline is None:
            self._offline = self.catalogue(self.prefix)
        return self._offline

    def call(self, name, arguments):
        reply = self.ask({"op": "call", "name": name, "args": arguments or {}})
        if reply is not None:
            return result(reply.get("isError"), reply.get("body"))
        if name == "hoi4_run":
            last = runs.read_json(os.path.join(runs.runs_dir(self.prefix), "last.json")) or {}
            return result(False, {"state": "no active run", "next": NO_RUN, "last_run": last.get("run")})
        return result(True, {"error": {"code": "no_active_run", "message": NO_RUN}})

    def handle(self, message):
        method = message.get("method")
        if "id" not in message:
            return None
        if method == "initialize":
            params = message.get("params") or {}
            body = {"protocolVersion": params.get("protocolVersion") or PROTOCOL,
                    "capabilities": {"tools": {}}, "serverInfo": {"name": NAME, "version": "1"},
                    "instructions": INSTRUCTIONS}
        elif method == "ping":
            body = {}
        elif method == "tools/list":
            body = {"tools": self.tools()}
        elif method == "tools/call":
            params = message.get("params") or {}
            body = self.call(params.get("name"), params.get("arguments"))
        else:
            return {"jsonrpc": "2.0", "id": message["id"],
                    "error": {"code": -32601, "message": "method not found: %s" % method}}
        return {"jsonrpc": "2.0", "id": message["id"], "result": body}


def serve(proxy, stdin=None, stdout=None):
    stdin = stdin or sys.stdin.buffer
    stdout = stdout or sys.stdout.buffer
    lock = threading.Lock()
    workers = []

    def answer(message):
        reply = proxy.handle(message)
        if reply is not None:
            with lock:
                stdout.write((json.dumps(reply, ensure_ascii=False) + "\n").encode("utf-8"))
                stdout.flush()

    for raw in iter(stdin.readline, b""):
        try:
            message = json.loads(raw)
        except ValueError:
            continue
        if not isinstance(message, dict):
            continue
        if message.get("method") == "tools/call":
            worker = threading.Thread(target=answer, args=(message,), daemon=True)
            worker.start()
            workers.append(worker)
        else:
            answer(message)
    for worker in workers:
        worker.join(timeout=1800)


def main(argv=None):
    parser = argparse.ArgumentParser(prog=NAME, description=__doc__.split("\n\n")[0])
    parser.add_argument("--prefix", required=True)
    args = parser.parse_args(argv)
    serve(Proxy(os.path.abspath(args.prefix)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
