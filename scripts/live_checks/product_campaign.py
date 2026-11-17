#!/usr/bin/env python3
"""Acceptance of the product path for a campaign: install, doctor, play, a segment boundary, continue, report.
Never run in CI; needs a running game on macOS (Apple silicon). About 10-15 minutes.

    python3 scripts/live_checks/product_campaign.py --prefix NEW_FOLDER --tag ITA --i-started-this-game
        [--install-from REPO --native PATH.tar.gz] [--days 20] [--same-game] [--silence] [--uninstall] [--out DIR]

Before running: start Hearts of Iron IV yourself (Steam), Windowed 1920x1080 at UI scaling 1.0, start a new 1936
campaign of TAG and leave it paused on the first day. Everything below goes through the `hoi4-live` command as a
user types it; the script plays the coding client's part with the hoi4-live-play MCP server (`hoi4-live mcp`): it
unpauses the game at speed 3 and reads once a minute (so the run never counts as silent). It sends no other input.

    1 install    --install-from REPO: python3 REPO/install.py --prefix NEW_FOLDER --native PATH --register none
                 (NEW_FOLDER must not exist); without it, NEW_FOLDER must already hold an install
    2 doctor     `hoi4-live doctor` exits 0
    3 segment 1  `hoi4-live play TAG "goal" --until <start + DAYS>`: prints "The run is live"; the run ends by
                 itself as end_date with the game paused and saved; end.json names the save, the save is in the
                 game's save folder, report.md has "Continue the campaign", gate/<date>.json and feed.jsonl exist,
                 watch.jsonl has a production sample
    4 report     `hoi4-live report` exits 0 and prints CAMPAIGN, BAR AUDIT and GATE lines
    5 restart    you quit the game, start it again and load the save (the launcher's Continue); --same-game skips
                 this and continues in the running game
    6 segment 2  `HOI4_LIVE_GAME_MEMORY_GB=0.5 hoi4-live play --continue`: a forced segment boundary: the run
                 starts at the saved date (not older), with plan_state.json carried and request.json naming segment
                 1; it ends as game_memory, saved
    7 report     `hoi4-live report` shows "segment 2" and segment 1's save
    8 silence    --silence: `hoi4-live play --continue` again with player_silent_minutes 1 in the run's settings and
                 no reads: the run ends as player_silent, paused and saved
    9 uninstall  --uninstall: `hoi4-live uninstall` removes the install

Each check prints PASS, FAIL or SKIP with its evidence; results.json in --out keeps them. Exit 0 when nothing failed.
"""

import argparse
import glob
import json
import os
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from play_attach import Client, RESULTS, check, finish, status, wait_status  # noqa: E402


def run_command(argv, env=None, timeout=3600):
    done = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, env=env, timeout=timeout)
    return done.returncode, done.stdout.decode("utf-8", errors="replace")


def run_folder(text):
    return next((line.split("Run folder: ", 1)[1].strip() for line in text.splitlines()
                 if line.startswith("Run folder: ")), None)


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def add_days(date, days):
    """A game date YYYY.MM.DD plus days (the game has no leap years)."""
    month_days = (31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31)
    y, m, d = (int(x) for x in date.split(".")[:3])
    d += days
    while d > month_days[m - 1]:
        d -= month_days[m - 1]
        m += 1
        if m > 12:
            m, y = 1, y + 1
    return "%04d.%02d.%02d" % (y, m, d)


def drive(command, run, minutes, read_every=60, unpause=True):
    """The client's part: unpause at speed 3, then read once a minute until the run ends or `minutes` pass."""
    client = Client(command)
    client.request("initialize", {"protocolVersion": "2024-11-05", "capabilities": {},
                                  "clientInfo": {"name": "acceptance", "version": "1"}})
    if unpause:
        client.call("hoi4_time", {"paused": False, "speed": 3})
    deadline = time.time() + minutes * 60
    last = 0
    while time.time() < deadline and status(run).get("state") != "ended":
        if read_every and time.time() - last >= read_every:
            last = time.time()
            try:
                client.call("hoi4_overview")
            except (RuntimeError, ValueError, OSError):
                pass
        time.sleep(5)
    try:
        client.close()
    except (OSError, subprocess.TimeoutExpired):
        pass
    return status(run)


def segment_checks(name, run, reason, user_dir):
    end = read_json(os.path.join(run, "end.json"))
    check("%s: the run ends by itself as %s" % (name, reason), end.get("reason") == reason, end)
    saved = end.get("saved")
    check("%s: the game's own save was written and named in end.json" % name,
          bool(saved) and os.path.isfile(os.path.join(user_dir, "save games", saved)),
          {"saved": saved, "error": end.get("save_error")})
    text = open(os.path.join(run, "report.md"), encoding="utf-8").read() if os.path.isfile(
        os.path.join(run, "report.md")) else ""
    check("%s: report.md says how to continue" % name, "## Continue the campaign" in text and saved and saved in text,
          text.split("## Continue the campaign", 1)[-1][:300])
    check("%s: the gate state, the feed and the samples are kept" % name,
          bool(glob.glob(os.path.join(run, "gate", "*.json"))) and os.path.isfile(os.path.join(run, "feed.jsonl"))
          and '"prod"' in open(os.path.join(run, "watch.jsonl"), encoding="utf-8").read(),
          sorted(os.listdir(run)))
    return end


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--prefix", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--goal", default="acceptance of the product path")
    parser.add_argument("--i-started-this-game", action="store_true", dest="confirmed")
    parser.add_argument("--install-from")
    parser.add_argument("--native", default="auto")
    parser.add_argument("--days", type=int, default=20)
    parser.add_argument("--same-game", action="store_true")
    parser.add_argument("--silence", action="store_true")
    parser.add_argument("--uninstall", action="store_true")
    parser.add_argument("--user-dir", default=os.path.expanduser("~/Documents/Paradox Interactive/Hearts of Iron IV"))
    parser.add_argument("--out")
    args = parser.parse_args(argv)
    if not args.confirmed:
        sys.exit("Refusing: pass --i-started-this-game to confirm the running game is yours to attach to.")
    prefix = os.path.abspath(args.prefix)
    command = os.path.join(prefix, "hoi4-live")
    out = args.out or tempfile.mkdtemp(prefix="hoi4live_acceptance_")
    os.makedirs(out, exist_ok=True)
    env = dict(os.environ)

    # 1 install
    if args.install_from:
        if os.path.exists(prefix):
            sys.exit("%s exists: the install check needs a new folder" % prefix)
        code, text = run_command([sys.executable, os.path.join(args.install_from, "install.py"), "--prefix", prefix,
                                  "--native", args.native, "--register", "none", "--no-link"], env, 1800)
        check("install: a clean prefix installs with the native runtime", code == 0 and os.path.isfile(command),
              {"exit": code, "tail": text[-400:]})
    else:
        check("install", os.path.isfile(command), "existing install at %s" % prefix, skip=True)
    if not os.path.isfile(command):
        return finish(out)

    # 2 doctor
    code, text = run_command([command, "doctor"], env, 600)
    if not check("doctor: exits 0 (nothing to fix)", code == 0, text[-400:]):
        return finish(out)

    # 3 segment 1, to an end date
    start_date = "1936.01.01"
    until = add_days(start_date, args.days)
    code, text = run_command([command, "play", args.tag, args.goal, "--until", until, "--wait", "5"], env)
    run1 = run_folder(text)
    with open(os.path.join(out, "play-1.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    if not check("segment 1: play attaches and the run is live", code == 0 and "The run is live" in text,
                 {"exit": code, "run": run1, "tail": text[-300:]}):
        return finish(out)
    check("segment 1: the plan's stop_on_date is gone and the end date is recorded",
          "stop_on_date" not in read_json(os.path.join(run1, "plan.json"))
          and read_json(os.path.join(run1, "request.json")).get("until") == until, until)
    ended = drive(command, run1, minutes=args.days / 4.0 + 10)
    check("segment 1: the game was paused at the end", ended.get("state") == "ended", ended)
    segment_checks("segment 1", run1, "end_date", args.user_dir)

    # 4 report
    code, text = run_command([command, "report"], env, 300)
    with open(os.path.join(out, "report-1.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    check("report: CAMPAIGN, BAR AUDIT and GATE lines", code == 0 and all(
        k in text for k in ("CAMPAIGN", "BAR AUDIT", "GATE (from the state kept")), text[:600])

    # 5 restart
    if not args.same_game:
        input("\nQuit the game now (Esc > Quit > Quit to desktop), start it again from Steam, press Continue in "
              "the launcher (it loads the newest save, %s), leave it paused, then press Enter here... "
              % read_json(os.path.join(run1, "end.json")).get("saved"))
    else:
        check("restart between segments", True, "--same-game: continued in the running game", skip=True)

    # 6 segment 2, forced boundary
    code, text = run_command([command, "play", "--continue", "--wait", "10"],
                             dict(env, HOI4_LIVE_GAME_MEMORY_GB="0.5"))
    run2 = run_folder(text)
    with open(os.path.join(out, "play-2.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    if not check("segment 2: --continue attaches to the loaded save", code == 0 and "The run is live" in text,
                 {"exit": code, "run": run2, "tail": text[-300:]}):
        return finish(out)
    request = read_json(os.path.join(run2, "request.json"))
    check("segment 2: the executor's memory and the plan in force are carried",
          os.path.isfile(os.path.join(run2, "plan_state.json")) and request.get("continues") == run1
          and request.get("segment") == 2, request)
    ready = [json.loads(line) for line in open(os.path.join(run2, "supervisor.log"), encoding="utf-8")
             if '"ready"' in line]
    check("segment 2: the game starts at the saved date", bool(ready) and str(ready[0].get("date"))[:10] >= until,
          {"ready": ready[:1], "until": until})
    drive(command, run2, minutes=5)
    segment_checks("segment 2", run2, "game_memory", args.user_dir)
    code, text = run_command([command, "report"], env, 300)
    with open(os.path.join(out, "report-2.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    check("report: both segments are listed", code == 0 and "segment 2:" in text and "segment 1:" in text,
          text[:600])

    # 8 silence
    if args.silence:
        code, text = run_command([command, "play", "--continue", "--wait", "5"], env)
        run3 = run_folder(text)
        if check("silence: a third segment starts", code == 0 and run3, text[-300:]):
            settings = read_json(os.path.join(run3, "settings.json"))
            with open(os.path.join(run3, "settings.json"), "w", encoding="utf-8") as f:
                json.dump(dict(settings, player_silent_minutes=1), f)
            drive(command, run3, minutes=4, read_every=0)
            segment_checks("silence", run3, "player_silent", args.user_dir)
    else:
        check("silent player", True, "needs --silence", skip=True)

    # 9 uninstall
    if args.uninstall:
        code, text = run_command([command, "uninstall"], env, 300)
        check("uninstall: the install is removed", code == 0 and not os.path.exists(os.path.join(prefix, "app")),
              text[-300:])
    return finish(out)


if __name__ == "__main__":
    sys.exit(main())
