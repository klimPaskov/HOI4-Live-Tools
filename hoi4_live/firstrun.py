"""The front door of a first campaign: python -m hoi4_live.firstrun --prefix INSTALL COUNTRY "GOAL"
(what `hoi4-live play COUNTRY "GOAL"` runs).

It takes a country and a goal and does, in order, what a fresh install needs before an agent can play:

    1. the doctor: anything to fix for the runtime that will play stops here, with its step
    2. the plan: a shipped reviewed plan when one matches the country (and mode), or a strategist plan (--plan)
    3. the start: on macOS with the native runtime, attach to the game the user started, start the supervisor
       (hoi4_live.play) and print the one instruction for the coding client; elsewhere what this system's
       installed runtime can start today, or the exact piece that is still missing (docs/PLAY_FLOW.md)

`--continue [RUN]` starts the next segment of a campaign instead (the last run that ended, or RUN): the plan in
force and the executor's memory are carried into a new run folder and the run attaches to the game the user
loaded from the save the last segment wrote. `--until DATE` ends the run at that game date (saved, so it can be
continued).

It never starts the game binary: the user starts the game from Steam (--launch asks Steam to). It sends no input
itself; it changes no file outside the run folder except the save the run writes at its end with the game's own
save (listed in the report). Standard library only. Exit codes: 0 handed to the coding client, 1 the doctor has
steps to do, 2 usage or no plan for the country, 3 this system has no start path yet, 4 no game or the run did not
start, 5 a campaign is already running.
"""

import argparse
import datetime
import json
import os
import re
import shutil
import sys

from . import doctor
from .play import start as native_start

EXIT_OK, EXIT_DOCTOR, EXIT_USAGE, EXIT_NO_START = 0, 1, 2, 3
PLAYBOOK = os.path.join("skills", "hoi4-campaign-playbook")


def shipped_plans(app):
    """The reviewed plans that ship with the program: tag, country name, mode and dates read from each file."""
    folder = os.path.join(app, PLAYBOOK, "plans")
    plans = []
    try:
        names = sorted(n for n in os.listdir(folder) if n.endswith(".plan.json"))
    except OSError:
        return plans
    for name in names:
        path = os.path.join(folder, name)
        try:
            with open(path, encoding="utf-8") as f:
                data = json.load(f)
        except (OSError, ValueError):
            continue
        meta = data.get("_validator") or {}
        tag = str(meta.get("tag") or "").upper()
        if not re.fullmatch(r"[A-Z0-9]{3}", tag):
            continue
        country = re.search(r"-\s*([^/]+?)\.txt$", str(meta.get("history_file") or ""))
        stem = name[:-len(".plan.json")]
        guide = os.path.join(folder, stem + ".md")
        plans.append({"id": stem, "tag": tag, "name": country.group(1) if country else tag,
                      "mode": "meta" if "meta" in stem.split("-") else "fair",
                      "start": meta.get("start_date"), "stop": meta.get("stop_date"), "path": path,
                      "guide": guide if os.path.isfile(guide) else None})
    return plans


def find_plan(plans, country, mode=None):
    """The shipped plan for a country given as a tag or a name; a named mode must match."""
    wanted = country.strip().lower()
    found = [p for p in plans if wanted in (p["tag"].lower(), p["name"].lower())]
    if mode:
        found = [p for p in found if p["mode"] == mode]
    found.sort(key=lambda p: p["mode"] != "fair")          # FAIR is the default mode
    return found[0] if found else None


def start_path(system, facts, items):
    """What the installed product can start on this system today:
    {"runtime", "ready", "can_start", "native", "missing"}."""
    states = {label: steps for label, steps, _why in doctor.runtimes(facts, items)}
    if system == "macos" and facts["native"].get("expected") and facts["native"].get("installed"):
        return {"runtime": "native runtime", "ready": states.get("native runtime") == 0, "can_start": True,
                "native": True, "missing": None}
    if system == "linux":
        return {"runtime": "native runtime", "ready": states.get("native runtime") == 0, "can_start": False,
                "native": False,
                "missing": "On Linux the native runtime cannot attach to a game you start yet: it holds only games "
                           "its owned launcher (hoi4-live-native --serve) started under an X11 display of its own, "
                           "and its input is bound to that display (docs/OWNED_LINUX_LAUNCHER.md). Nothing starts a "
                           "campaign from the install there yet."}
    note = ("This Mac has no native runtime (it needs Apple silicon), so the coding client plays through the "
            "Python server with the plan's notes as its guide; the Python server has had no accepted live campaign "
            "on macOS."
            if system == "macos" else
            "The shipped plan is written for the native runtime's executor, which has no Windows adapter yet, so "
            "the coding client plays through the Python server with the plan's notes as its guide.")
    return {"runtime": "Python server", "ready": states.get("Python server") == 0, "can_start": True,
            "native": False, "missing": note}


def client_prompt(plan, goal, run, app):
    """The one instruction to give the coding client that has the hoi4-live tools."""
    guide = plan["guide"] or plan["path"]
    year = str(plan["start"] or "1936")[:4]
    return (f"Play Hearts of Iron IV as {plan['name']} ({plan['tag']}) with the hoi4-live tools. Goal: {goal}. "
            f"Start the game with hoi4_launch {{\"prepare_dlc\": true}}. If {plan['name']} is one of the countries "
            f"the Select Country screen lists by name, start it with hoi4_new_game {{\"bookmark\": \"{year}\", "
            f"\"country\": \"<its name as that screen shows it>\"}}; otherwise open the {year} scenario's Select "
            f"Country screen with hoi4_screen and hoi4_click, click {plan['name']} on the map, check that the panel "
            f"names it, then Select Country and START. Rules: {plan['mode'].upper()} mode of the campaign playbook at "
            f"{os.path.join(app, PLAYBOOK, 'SKILL.md')}: Normal difficulty, historical AI, no console, no save "
            f"editing, no reloads. Follow the plan notes at {guide}; the run folder is {run}. Keep the game running "
            "while you act and stop at " + str(plan["stop"] or "the goal") + " or when the goal is reached.")


def runs_read(run):
    """A run folder's request (the goal of a continued campaign)."""
    try:
        with open(os.path.join(run, "request.json"), encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def campaigns(plans):
    return "; ".join(f"{p['name']} ({p['tag']}, {p['mode'].upper()}, {p['start']} to {p['stop']})" for p in plans)


def prepare_run(prefix, plan, goal, now=None):
    stamp = (now or datetime.datetime.now()).strftime("%Y%m%d-%H%M%S")
    run = os.path.join(prefix, "runs", f"{stamp}-{plan['tag']}")
    os.makedirs(run)
    shutil.copyfile(plan["path"], os.path.join(run, "plan.json"))
    with open(os.path.join(run, "request.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump({"country": plan["name"], "tag": plan["tag"], "goal": goal, "mode": plan["mode"],
                   "plan": plan["id"], "start": plan["start"], "stop": plan["stop"], "created": stamp}, f, indent=2)
    return run


STRATEGIST_NOTE = (
    "A plan for another country, start or mod is written by the strategist, a step of its own: it researches the "
    "game files with a strong model for a long time (typically 20 to 60 minutes, and a model bill to match), then a "
    "second model reviews the plan. It runs from the repository: python scripts/strategist/strategize.py --run "
    "<folder> --tag <TAG> --goal \"<goal>\" (skills/hoi4-strategist explains it). Then: hoi4-live play <TAG> "
    "\"<goal>\" --plan <folder>.")


def plan_from_folder(folder):
    """A strategist plan folder (plan.json, run_spec.json, plan.brief.md, plan.md) as a plan entry, or None."""
    folder = os.path.abspath(folder)
    try:
        with open(os.path.join(folder, "run_spec.json"), encoding="utf-8") as f:
            spec = json.load(f)
        with open(os.path.join(folder, "plan.json"), encoding="utf-8") as f:
            json.load(f)
    except (OSError, ValueError):
        return None
    tag = str(spec.get("tag") or "").upper()
    if not re.fullmatch(r"[A-Z0-9]{3}", tag):
        return None
    brief, guide = os.path.join(folder, "plan.brief.md"), os.path.join(folder, "plan.md")
    return {"id": os.path.basename(folder), "tag": tag, "name": spec.get("country") or tag,
            "mode": str(spec.get("mode") or "fair").lower(), "start": spec.get("date"),
            "stop": spec.get("horizon"), "path": os.path.join(folder, "plan.json"),
            "brief": brief if os.path.isfile(brief) else None, "guide": guide if os.path.isfile(guide) else None}


DATE = re.compile(r"^\d{4}[.-]\d{1,2}[.-]\d{1,2}$")


def play(prefix, country, goal, mode=None, gather=doctor.gather, out=sys.stdout, now=None, plan_folder=None,
         launch=False, wait_minutes=15, minutes=180, start_native=native_start.start_native,
         idle_pause_minutes=native_start.IDLE_PAUSE_MINUTES, until=None, continue_run=None,
         player_silent_minutes=None):
    app = os.path.join(prefix, "app")
    plans = shipped_plans(app)
    if until and not DATE.match(until):
        out.write(f"--until takes a game date such as 1938.01.01, not \"{until}\".\n")
        return EXIT_USAGE
    previous = None
    if continue_run:
        previous = native_start.previous_run(prefix, None if continue_run == "last" else continue_run)
        if not previous:
            out.write("No earlier run to continue" + ("" if continue_run == "last" else f" at {continue_run}")
                      + ". A campaign starts with: hoi4-live play COUNTRY \"GOAL\"\n")
            return EXIT_USAGE
        plan = native_start.continued_plan(previous)
        goal = (runs_read(previous).get("goal") or goal or "")
    elif plan_folder:
        plan = plan_from_folder(plan_folder)
        if not plan:
            out.write(f"{plan_folder} is not a strategist plan folder (plan.json and run_spec.json with a tag).\n")
            return EXIT_USAGE
        if country.strip().upper() != plan["tag"] and country.strip().lower() != plan["name"].lower():
            out.write(f"The plan in {plan_folder} is for {plan['tag']}, not {country}.\n")
            return EXIT_USAGE
    else:
        plan = find_plan(plans, country, mode)
    if not plan:
        out.write(f"No shipped plan for \"{country}\"" + (f" in {mode.upper()} mode" if mode else "") + ".\n"
                  f"Campaigns with a reviewed plan: {campaigns(plans) or 'none found in this install'}.\n"
                  + STRATEGIST_NOTE + "\n")
        return EXIT_USAGE
    facts = gather(prefix)
    items = doctor.evaluate(facts)
    path = start_path(facts["system"], facts, items)
    out.write(f"Campaign: {plan['name']} ({plan['tag']}), {plan['mode'].upper()}, {plan['start']} to {plan['stop']}\n"
              f"Goal: {goal}\nPlan: " + ("the plan in force at the end of the last segment " if previous else
                                          "strategist plan " if plan_folder else "shipped reviewed plan ")
              + f"{plan['id']}\n" + (f"Ends at the game date {until}\n" if until else ""))
    if not path["ready"]:
        key = doctor.NATIVE if path["runtime"] == "native runtime" else doctor.SERVER
        out.write(f"\nThe {path['runtime']} is not ready. Do these steps, then run the command again:\n")
        for entry in items:
            if entry["status"] == doctor.FIX and entry["needs"] in (key, doctor.BOTH):
                out.write(f"  [fix]  {entry['title']}\n          do this: {entry['fix']}\n")
        return EXIT_DOCTOR
    out.write(f"Checks: the {path['runtime']} is ready\n")
    if not path["can_start"]:
        out.write(f"\n{path['missing']}\n")
        return EXIT_NO_START
    if previous and not path.get("native"):
        out.write("\nContinuing a campaign needs the native runtime (macOS on Apple silicon today).\n")
        return EXIT_NO_START
    if path.get("native"):
        template = os.path.join(app, PLAYBOOK, "briefs", "player.txt")
        with open(template, encoding="utf-8") as f:
            text = f.read()
        return start_native(prefix, plan, goal, facts, out, text, launch=launch, wait_minutes=wait_minutes,
                            minutes=minutes, now=now, idle_pause_minutes=idle_pause_minutes, until=until,
                            continue_from=previous, **({} if player_silent_minutes is None else
                                                       {"player_silent_minutes": player_silent_minutes}))
    run = prepare_run(prefix, plan, goal, now)
    prompt = client_prompt(plan, goal, run, app)
    with open(os.path.join(run, "prompt.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(prompt + "\n")
    out.write(f"Run folder: {run}\n\n{path['missing']}\n\n"
              "Give your coding client this instruction (it is also in prompt.txt in the run folder):\n\n"
              f"{prompt}\n")
    return EXIT_OK


def main(argv=None):
    parser = argparse.ArgumentParser(prog="hoi4-live play", description=__doc__.split("\n\n")[0])
    parser.add_argument("--prefix", required=True, help="the install folder")
    parser.add_argument("--mode", choices=("fair", "meta"), help="the rule set (default: the plan's, FAIR first)")
    parser.add_argument("--plan", help="a strategist plan folder instead of a shipped plan")
    parser.add_argument("--launch", action="store_true", help="ask Steam to start the game (macOS)")
    parser.add_argument("--wait", type=int, default=15, help="minutes to wait for the game (default 15)")
    parser.add_argument("--minutes", type=int, default=180, help="the agent's time limit in the brief")
    parser.add_argument("--idle-pause", type=int, default=native_start.IDLE_PAUSE_MINUTES, metavar="MINUTES",
                        help="pause the game only after this many minutes without any call from the agent "
                             "(default %d; 0 never pauses). The game is never paused otherwise; the run's "
                             "settings.json holds it and can be changed during the run"
                             % native_start.IDLE_PAUSE_MINUTES)
    parser.add_argument("--player-silent", type=int, default=None, metavar="MINUTES",
                        help="end the run (paused and saved) after this many minutes without any call from the "
                             "agent (default 45; 0 never: the executor then plays on alone)")
    parser.add_argument("--until", metavar="DATE", help="end the run at this game date, for example 1938.01.01 "
                                                       "(the game is paused and saved there)")
    parser.add_argument("--continue", dest="continue_run", nargs="?", const="last", metavar="RUN",
                        help="the next segment of a campaign: the last run that ended (or RUN), from the save it "
                             "wrote; no country or goal needed")
    parser.add_argument("country", nargs="?", help="a country tag or name, for example ITA or Italy")
    parser.add_argument("goal", nargs="*", help="what the campaign should achieve, in your words")
    args = parser.parse_args(argv)
    prefix = os.path.abspath(args.prefix)
    if args.continue_run:
        return play(prefix, args.country or "", " ".join(args.goal), args.mode, launch=args.launch,
                    wait_minutes=args.wait, minutes=args.minutes, idle_pause_minutes=max(0, args.idle_pause),
                    until=args.until, continue_run=args.continue_run)  # a segment keeps its run's settings
    if not args.country or not args.goal:
        sys.stdout.write("usage: hoi4-live play COUNTRY \"GOAL\"   for example: hoi4-live play ITA \"conquer "
                         "Ethiopia before 1937\"\n"
                         f"Campaigns with a reviewed plan: {campaigns(shipped_plans(os.path.join(prefix, 'app')))}\n")
        return EXIT_USAGE
    return play(prefix, args.country, " ".join(args.goal), args.mode, plan_folder=args.plan, launch=args.launch,
                wait_minutes=args.wait, minutes=args.minutes, idle_pause_minutes=max(0, args.idle_pause),
                until=args.until, player_silent_minutes=None if args.player_silent is None
                else max(0, args.player_silent))


if __name__ == "__main__":
    sys.exit(main())
