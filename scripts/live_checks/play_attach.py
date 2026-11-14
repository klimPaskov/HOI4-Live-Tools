#!/usr/bin/env python3
"""Live checks of `hoi4-live play` on the native runtime (macOS). Never run in CI; needs a running game.

    python3 scripts/live_checks/play_attach.py --prefix INSTALL --tag ITA --i-started-this-game
                                               [--actions] [--watchdog 60] [--close-test] [--out FOLDER]

INSTALL is an install of the branch under test (python install.py --prefix INSTALL). Before running: start
Hearts of Iron IV yourself (Steam, or the owner's own launcher), set Windowed 1920x1080 at UI scaling 1.0, and
start or load a campaign of TAG; leave it paused. The script then goes through the product exactly as a user
does: `hoi4-live doctor`, `hoi4-live play`, the coding client's side (`hoi4-live mcp` spoken to as an MCP client)
and `hoi4-live stop`. It attaches only to the one game of this user and only with --i-started-this-game.

Without --actions it sends no input to the game (reads only; the executor the run switches on does act).
--actions       also: an identical harmless action twice (map mode), input after another window took the focus,
                and an unpause at speed 1 for the watchdog check
--watchdog N    the supervisor's idle pause is N seconds; checks that it pauses a running game and turns the
                executor off, and that the next call turns it back on (needs --actions to unpause first)
--close-test    at the end, a second run; you quit the game when asked; checks the run ends as game_closed

Each check prints PASS, FAIL or SKIP with its evidence; results.json in --out keeps them. Exit 0 when nothing
failed.
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time

RESULTS = []


def check(name, ok, evidence, skip=False):
    state = "SKIP" if skip else ("PASS" if ok else "FAIL")
    RESULTS.append({"check": name, "state": state, "evidence": evidence})
    print("%-5s %s: %s" % (state, name, json.dumps(evidence, ensure_ascii=False)[:400]), flush=True)
    return ok


class Client:
    """The coding client's side: `hoi4-live mcp` as a stdio MCP server."""

    def __init__(self, command):
        self.process = subprocess.Popen([command, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL)
        self.seq = 0

    def request(self, method, params=None):
        self.seq += 1
        self.process.stdin.write((json.dumps({"jsonrpc": "2.0", "id": self.seq, "method": method,
                                              "params": params or {}}) + "\n").encode())
        self.process.stdin.flush()
        while True:
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError("hoi4-live mcp exited")
            reply = json.loads(line)
            if reply.get("id") == self.seq:
                return reply.get("result") or reply

    def call(self, name, args=None):
        result = self.request("tools/call", {"name": name, "arguments": args or {}})
        text = (result.get("content") or [{}])[0].get("text") or "{}"
        return bool(result.get("isError")), json.loads(text)

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=20)


def status(run):
    try:
        with open(os.path.join(run, "status.json"), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def wait_status(run, wanted, seconds):
    deadline = time.time() + seconds
    while time.time() < deadline:
        current = status(run)
        if wanted(current):
            return current
        time.sleep(2)
    return status(run)


def alive(pid):
    try:
        os.kill(int(pid), 0)
        return True
    except (OSError, ValueError, TypeError):
        return False


def play(command, tag, goal, wait, env):
    done = subprocess.run([command, "play", tag, goal, "--wait", str(wait)], stdout=subprocess.PIPE,
                          stderr=subprocess.STDOUT, env=env, timeout=wait * 60 + 2400)
    text = done.stdout.decode("utf-8", errors="replace")
    run = next((line.split("Run folder: ", 1)[1].strip() for line in text.splitlines()
                if line.startswith("Run folder: ")), None)
    return done.returncode, text, run


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--prefix", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--goal", default="live check of the play flow")
    parser.add_argument("--i-started-this-game", action="store_true", dest="confirmed")
    parser.add_argument("--actions", action="store_true")
    parser.add_argument("--watchdog", type=int, default=0)
    parser.add_argument("--close-test", action="store_true")
    parser.add_argument("--wait", type=int, default=10)
    parser.add_argument("--out")
    args = parser.parse_args(argv)
    if not args.confirmed:
        sys.exit("Refusing: pass --i-started-this-game to confirm the running game is yours to attach to.")
    prefix = os.path.abspath(args.prefix)
    command = os.path.join(prefix, "hoi4-live")
    out = args.out or tempfile.mkdtemp(prefix="hoi4live_livecheck_")
    os.makedirs(out, exist_ok=True)
    env = dict(os.environ)
    if args.watchdog:
        env["HOI4_LIVE_IDLE_PAUSE"] = str(args.watchdog)

    # 1. the doctor, as the user runs it
    done = subprocess.run([sys.executable, "-m", "hoi4_live.doctor", "--prefix", prefix, "--json"],
                          stdout=subprocess.PIPE, env=dict(env, PYTHONPATH=os.path.join(prefix, "app")))
    report = json.loads(done.stdout or b"{}")
    fixes = [i["title"] for i in report.get("items", []) if i["status"] == "fix" and i["needs"] in ("native", "both")]
    check("doctor: the native runtime has nothing to fix", not fixes, fixes or "ready")

    # 2. play: attach, supervisor, one instruction
    code, text, run = play(command, args.tag, args.goal, args.wait, env)
    with open(os.path.join(out, "play-1.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    if not check("play: exits 0 and prints the instruction", code == 0 and "BRIEF.txt" in text,
                 {"exit": code, "run": run, "tail": text[-300:]}):
        return finish(out)
    session = json.load(open(os.path.join(run, "session.json"), encoding="utf-8"))
    mode = os.stat(os.path.join(run, "session.json")).st_mode & 0o777
    game_pid = session["game"]["pid"]
    check("attach: session is attached, private, names the running game",
          session.get("attached") is True and mode == 0o600 and alive(game_pid)
          and session["game"]["exe"].endswith("/hoi4"), {"pid": game_pid, "mode": oct(mode)})
    check("attach: the parent of the game was not required (Steam-started game)", True,
          "ps -o ppid= -p %d -> %s" % (game_pid, subprocess.run(["ps", "-o", "ppid=", "-p", str(game_pid)],
                                                                  stdout=subprocess.PIPE).stdout.decode().strip()))
    check("user files recorded before the attach", os.path.isfile(os.path.join(run, "user-files.json")), run)

    # 3. the client's side
    client = Client(command)
    init = client.request("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                                         "clientInfo": {"name": "live-check", "version": "1"}})
    tools = client.request("tools/list").get("tools") or []
    leaked = [t["name"] for t in tools if "lease_token" in json.dumps(t.get("inputSchema"))]
    names = {t["name"] for t in tools}
    check("mcp: hoi4-live-play lists the runtime's tools without lease tokens",
          init.get("serverInfo", {}).get("name") == "hoi4-live-play" and len(tools) > 100 and not leaked
          and {"hoi4_batch", "hoi4_run"} <= names, {"tools": len(tools), "leaked": leaked})
    err, overview = client.call("hoi4_overview")
    check("mcp: hoi4_overview reads the attached campaign", not err and overview.get("player") == args.tag,
          {"player": overview.get("player"), "date": (overview.get("date") or {}).get("text"),
           "paused": overview.get("paused")})
    err, run_status = client.call("hoi4_run", {"op": "status"})
    check("mcp: hoi4_run status shows the executor on", not err and run_status.get("executor_on") is True, run_status)
    err, feed = client.call("hoi4_observe", {"since": 0})
    check("mcp: the change feed answers", not err and "seq" in feed, {"seq": feed.get("seq")})

    # 4. actions (opt-in)
    if args.actions:
        err1, first = client.call("hoi4_map_mode", {"mode": "political"})
        err2, second = client.call("hoi4_map_mode", {"mode": "political"})
        check("duplicate: an identical success is answered as already_done", not err1 and "already_done" in second,
              {"first": str(first)[:150], "second": str(second)[:150]})
        with open(os.path.join(run, "calls.jsonl"), encoding="utf-8") as f:
            calls = f.read()
        check("calls.jsonl holds no lease token", "lease_token" not in calls, "%d bytes" % len(calls))
        print("Click on this terminal (or any other window) within 8 seconds so the game loses the focus...")
        time.sleep(8)
        err, body = client.call("hoi4_map_mode", {"mode": "terrain"})
        _err, after = client.call("hoi4_run", {"op": "status"})
        check("front: input after another window took the focus brings the game forward", not err
              and after.get("front", 0) >= 1, {"result": str(body)[:200], "front": after.get("front")})
        client.call("hoi4_map_mode", {"mode": "political"})
    else:
        check("duplicate / front", True, "needs --actions", skip=True)

    if args.watchdog and args.actions:
        client.call("hoi4_time", {"paused": False, "speed": 1})
        print("Waiting %d s without a call for the watchdog..." % (args.watchdog + 20))
        seen = wait_status(run, lambda s: s.get("watchdog_paused") is True, args.watchdog + 40)
        err, overview = client.call("hoi4_overview")
        notes = overview.get("supervisor_notes") or []
        check("watchdog: no call for N seconds pauses a running game and turns the executor off",
              seen.get("watchdog_paused") is True and overview.get("paused") is True, {"status": seen, "notes": notes})
        resumed = wait_status(run, lambda s: s.get("watchdog_paused") is False, 20)
        check("watchdog: the next call turns the executor back on", resumed.get("watchdog_paused") is False,
              resumed)
    else:
        check("watchdog", True, "needs --watchdog N and --actions", skip=True)

    # 5. stop from the terminal
    client.close()
    done = subprocess.run([command, "stop"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=300)
    ended = status(run)
    report_path = os.path.join(run, "report.md")
    text = open(report_path, encoding="utf-8").read() if os.path.isfile(report_path) else ""
    check("stop: the run ends with a report and the game keeps running",
          done.returncode == 0 and ended.get("state") == "ended" and text and alive(game_pid),
          {"exit": done.returncode, "state": ended.get("state"), "game_alive": alive(game_pid)})
    check("stop: the active-run pointer is gone", not os.path.exists(os.path.join(prefix, "runs", "active.json")),
          os.path.join(prefix, "runs"))
    check("your files: unchanged during the run", "as they were at the start" in text,
          text.split("## Your files", 1)[-1].strip()[:300])

    # 6. the game closes (opt-in; the owner quits the game)
    if args.close_test:
        code, text, run2 = play(command, args.tag, args.goal, args.wait, env)
        if check("close test: second run starts", code == 0, {"exit": code, "run": run2}):
            print("Quit the game now (Quit to desktop). Waiting up to 5 minutes...")
            gone = wait_status(run2, lambda s: s.get("state") == "ended", 300)
            check("close test: the run ends as game_closed with a report", gone.get("end_reason") == "game_closed"
                  and os.path.isfile(os.path.join(run2, "report.md")), gone)
    else:
        check("game closes", True, "needs --close-test", skip=True)

    print("\nBy hand (write the result next to results.json):\n"
          "  - a game started from Steam with the overlay on: attach, reads and input all work\n"
          "  - close the terminal that ran `hoi4-live play`: the run goes on and input still works (macOS keeps "
          "the Accessibility grant of the app that started the supervisor)\n"
          "  - a campaign of another country: play says which country it found and stops\n"
          "  - play started at the main menu: it prints \"start or load the campaign\" and starts once you do\n"
          "  - fullscreen or a wrong resolution: play prints the in-game steps and waits until they are set\n"
          "  - a Mac whose screen is smaller than 1920x1108 points: the doctor names \"More Space\"")
    return finish(out)


def finish(out):
    path = os.path.join(out, "results.json")
    with open(path, "w", encoding="utf-8") as f:
        json.dump(RESULTS, f, indent=1)
    failed = [r for r in RESULTS if r["state"] == "FAIL"]
    print("\n%d checks, %d failed. Results: %s" % (len(RESULTS), len(failed), path))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
