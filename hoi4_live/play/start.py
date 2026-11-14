"""The native start path of `hoi4-live play`: run folder, the user's game, attach, supervisor, one instruction.

Every outside effect goes through an argument (process listing, attach, supervisor start, sleep, clock), so the
whole path is tested without a game. Nothing here starts the game unless the user passed --launch, and then only
through Steam.
"""

import json
import os
import shutil
import subprocess
import sys
import time

from . import game, runs

EXIT_OK, EXIT_NO_GAME, EXIT_BUSY = 0, 4, 5
CONTROLLER = "hoi4-live-play"


def pid_alive(pid):
    if not pid:
        return False
    if os.name == "nt":
        out = subprocess.run(["tasklist", "/FI", "PID eq %d" % int(pid), "/FO", "CSV", "/NH"],
                             stdout=subprocess.PIPE, stderr=subprocess.DEVNULL).stdout.decode(errors="replace")
        return '"%d"' % int(pid) in out
    try:
        os.kill(int(pid), 0)
        return True
    except PermissionError:
        return True
    except (OSError, ValueError):
        return False


def prepare(prefix, plan, goal, template, minutes=180, now=None):
    """The run folder: the plan switched on, the runtime's input binding, the request, the brief, the prompt."""
    run = runs.new_run(prefix, plan["tag"], now)
    with open(plan["path"], encoding="utf-8") as f:
        data = json.load(f)
    data["active"] = True
    runs.write_json(os.path.join(run, "plan.json"), data)
    runs.write_json(os.path.join(run, "input.json"), {"controller": CONTROLLER, "player": plan["tag"]})
    runs.write_json(os.path.join(run, "request.json"), {
        "country": plan["name"], "tag": plan["tag"], "goal": goal, "mode": plan["mode"], "plan": plan["id"],
        "start": plan.get("start"), "stop": plan.get("stop"), "runtime": "native"})
    if plan.get("guide") and os.path.isfile(plan["guide"]):
        shutil.copyfile(plan["guide"], os.path.join(run, "plan.md"))
    # The brief sends the player back to plan.brief.md when the plan is revised: it always exists.
    runs.write_private(os.path.join(run, "plan.brief.md"), runs.plan_brief_text(plan) + "\n")
    runs.write_private(os.path.join(run, "BRIEF.txt"), runs.render_brief(template, plan, goal, run, minutes))
    runs.write_private(os.path.join(run, "prompt.txt"), runs.instruction(run) + "\n")
    return run


def attach(binary, pid, run):
    """(ok, output) of `hoi4-live-native --attach PID RUN`."""
    try:
        done = subprocess.run([binary, "--attach", str(pid), run], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT, timeout=120)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return False, str(exc)
    return done.returncode == 0, done.stdout.decode("utf-8", errors="replace").strip()


def spawn_supervisor(prefix, run, binary, python=None):
    """Start the supervisor in the background, in its own session so closing the terminal does not end it."""
    app = os.path.join(prefix, "app")
    env = dict(os.environ, PYTHONPATH=app)
    args = [python or sys.executable, "-m", "hoi4_live.play.supervisor", "--prefix", prefix, "--run", run,
            "--native", binary]
    log = open(os.path.join(run, "supervisor.out"), "ab")
    options = {"creationflags": 0x00000008 | 0x00000200} if os.name == "nt" else {"start_new_session": True}
    return subprocess.Popen(args, cwd=app, env=env, stdin=subprocess.DEVNULL, stdout=log, stderr=log, **options)


def game_wanted(plan):
    year = str(plan.get("start") or "1936")[:4]
    return ("Start Hearts of Iron IV the way you always do (Steam). Start a new %s game as %s (Normal difficulty, "
            "historical AI, no console) and leave it paused on the first day. Game settings for the native "
            "runtime: Options > Graphics > Windowed, 1920x1080, UI scaling 1.0." % (year, plan["name"]))


def busy(prefix):
    """The active run when its supervisor still runs, else None (a stale pointer is removed)."""
    active = runs.active_run(prefix)
    if not active:
        return None
    if pid_alive(active.get("pid")):
        return active
    runs.clear_active(prefix, active["run"])
    return None


def start_native(prefix, plan, goal, facts, out, template, launch=False, wait_minutes=15, minutes=180,
                 finder=game.find_games, attacher=attach, spawner=spawn_supervisor, steam=game.launch_through_steam,
                 sleep=time.sleep, clock=time.time, now=None):
    active = busy(prefix)
    if active:
        out.write("A campaign is already running (run folder %s).\nEnd it with: hoi4-live stop\n" % active["run"])
        return EXIT_BUSY
    binary, exe = facts["native"]["path"], facts["game"]["exe"]
    steps = game.display_steps(facts.get("display") or {}, facts.get("screen"), facts["system"])
    if steps:
        out.write("\nBefore the game starts, set these (the runtime refuses input otherwise):\n")
        for title, step in steps:
            out.write("  [set]  %s\n          do this: %s\n" % (title, step))
    run = prepare(prefix, plan, goal, template, minutes, now)
    out.write("Run folder: %s\n" % run)

    pids = finder(exe)
    if not pids:
        if launch:
            steam()
            out.write("\nAsked Steam to start Hearts of Iron IV.\n")
        out.write("\n%s\nWaiting for the game (up to %d minutes; Ctrl+C stops waiting and changes nothing)...\n"
                  % (game_wanted(plan), wait_minutes))
        deadline = clock() + wait_minutes * 60
        while not pids and clock() < deadline:
            sleep(2)
            pids = finder(exe)
        if not pids:
            out.write("No game appeared. Start it and run the same command again.\n")
            return EXIT_NO_GAME
    if len(pids) > 1:
        out.write("More than one Hearts of Iron IV is running (process IDs %s). Close the extra ones and run the "
                  "command again.\n" % ", ".join(map(str, pids)))
        return EXIT_NO_GAME
    pid = pids[0]
    out.write("Game found (process %d).\n" % pid)
    runs.record_user_files(facts["user_dir"], run)
    ok, text = attacher(binary, pid, run)
    if not ok:
        out.write("Could not attach to the game: %s\nRun hoi4-live doctor for the step that fixes it.\n" % text[-600:])
        return EXIT_NO_GAME
    process = spawner(prefix, run, binary)
    out.write("Attached. Waiting for your campaign as %s (start or load it; the run begins when the game shows "
              "it)...\n" % plan["name"])
    shown = None
    try:
        while True:
            status = runs.read_json(os.path.join(run, "status.json")) or {}
            state = status.get("state")
            if status.get("message") != shown and state in ("waiting", "starting"):
                shown = status.get("message")
                out.write("  ... %s\n" % shown)
            if state == "ready":
                break
            if state in ("failed", "ended") or process.poll() is not None:
                out.write("The run did not start: %s\nDetails: %s\n"
                          % (status.get("message") or "the supervisor exited",
                             os.path.join(run, "native.stderr")))
                return EXIT_NO_GAME
            sleep(1)
    except KeyboardInterrupt:
        process.terminate()
        out.write("\nStopped before the run began. Nothing in your game or its files was changed.\n")
        return EXIT_NO_GAME
    out.write("\nThe run is live: %s on %s, the plan's executor is on, the supervisor watches the run.\n"
              "\nIn your coding client (restart it once after installing so it loads the hoi4-live-play tools), "
              "give this instruction; it is also in %s:\n\n    %s\n\n"
              "When the agent stops it calls hoi4_run {\"op\":\"stop\"}; `hoi4-live stop` does the same from here. "
              "The report is written to %s.\n"
              % (plan["name"], status.get("date") or "day one", os.path.join(run, "prompt.txt"), runs.instruction(run),
                 os.path.join(run, "report.md")))
    return EXIT_OK


def stop_active(prefix, out, sleep=time.sleep, clock=time.time, connect=None):
    """`hoi4-live stop`: ask the active run's supervisor to stop and wait for its report."""
    from . import proxy
    active = busy(prefix)
    if not active:
        out.write("No campaign is running.\n")
        return 0
    conn = (connect or proxy.connect)(prefix)
    if conn is None:
        out.write("The run's supervisor does not answer (run folder %s).\n" % active["run"])
        return 1
    with conn:
        conn.send({"op": "stop"})
        conn.recv()
    deadline = clock() + 120
    while clock() < deadline:
        status = runs.read_json(os.path.join(active["run"], "status.json")) or {}
        if status.get("state") == "ended":
            out.write("Stopped. Report: %s\nThe game keeps running.\n" % status.get("report"))
            return 0
        sleep(1)
    out.write("The supervisor did not finish within two minutes; see %s\n" % active["run"])
    return 1
