"""`hoi4-live report [RUN]`: read a campaign run against the bar, during the run or after it.

    python -m hoi4_live.play.review --prefix INSTALL [RUN] [--marks CIV MIL DIVS] [--no-baseline]

RUN is a run folder (a path, or its name under the install's runs); without it the running campaign, else the
last one that ended. It prints, and keeps in RUN/audit.txt:

    the campaign  country, goal, each segment with its dates, why it ended and the save it wrote
    the audit     one line per item of the bar from the run's own files (play/audit.py)
    the gate      where the country stands (play/gate.py): from the running game when the run is live (the
                  supervisor reads it; no input, the player's idle timer is not touched), else from the state the
                  run kept at its end (gate/<date>.json)

Marks: --marks CIV MIL DIVS, else the stored AI baseline for the country and start year when the install has one
(skills/hoi4-campaign-playbook/baselines), else none. Standard library only; reads only.
"""

import argparse
import json
import os
import sys

from . import audit, gate, runs


def resolve(prefix, name=None):
    """(run folder, live): the named run, else the active run, else the last run that ended; (None, False)."""
    active = runs.active_run(prefix)
    if name and name != "last":
        for path in (os.path.abspath(os.path.expanduser(name)), os.path.join(runs.runs_dir(prefix), name)):
            if os.path.isfile(os.path.join(path, "request.json")):
                live = bool(active) and os.path.realpath(active["run"]) == os.path.realpath(path)
                return path, live
        return None, False
    if active and not name:
        return active["run"], True
    last = runs.read_json(os.path.join(runs.runs_dir(prefix), "last.json")) or {}
    path = last.get("run")
    return (path, False) if path and os.path.isdir(path) else (None, False)


def live_snapshot(prefix, connect=None):
    """A fresh snapshot from the running campaign's supervisor, or None."""
    from . import proxy
    conn = (connect or proxy.connect)(prefix)
    if conn is None:
        return None
    try:
        with conn:
            conn.send({"op": "snapshot"})
            reply = conn.recv()
    except (OSError, EOFError):
        return None
    return reply.get("snapshot") if isinstance(reply, dict) else None


def kept_snapshot(run):
    """The newest snapshot the run kept (gate/<date>.json), or None."""
    folder = os.path.join(run, "gate")
    try:
        names = sorted(n for n in os.listdir(folder) if n.endswith(".json"))
    except OSError:
        return None
    for n in reversed(names):
        data = runs.read_json(os.path.join(folder, n))
        if data and data.get("reads"):
            return data
    return None


def game_dir(prefix, run):
    """The game's folder (for the unit files): HOI4_DIR, the install's config.json game_dir, else the folder of
    the executable the run attached to."""
    for d in (os.environ.get("HOI4_DIR"), (runs.read_json(os.path.join(prefix, "config.json")) or {}).get("game_dir")):
        if d and os.path.isdir(os.path.join(d, "common")):
            return d
    exe = ((runs.read_json(os.path.join(run, "session.json")) or {}).get("game") or {}).get("exe") or ""
    marker = os.sep + "hoi4.app" + os.sep
    folder = exe.split(marker)[0] if marker in exe else os.path.dirname(exe)
    return folder if folder and os.path.isdir(os.path.join(folder, "common")) else None


def segments(run):
    """The campaign's segments, first to this one: [(run, request, end)] following request.continues."""
    chain, seen = [], set()
    while run and run not in seen and os.path.isfile(os.path.join(run, "request.json")):
        seen.add(run)
        request = runs.read_json(os.path.join(run, "request.json")) or {}
        chain.append((run, request, runs.read_json(os.path.join(run, "end.json")) or {}))
        run = request.get("continues")
    return list(reversed(chain))


def campaign_lines(run, live):
    chain = segments(run)
    request = chain[-1][1] if chain else {}
    out = ["CAMPAIGN %s (%s): %s" % (request.get("country") or "?", request.get("tag") or "?",
                                      request.get("goal") or "?"),
           "plan %s, %s mode%s" % (request.get("plan") or "?", str(request.get("mode") or "fair").upper(),
                                   ", ends at %s" % request["until"] if request.get("until") else "")]
    for n, (path, req, end) in enumerate(chain, 1):
        start = req.get("resume_date") or req.get("start") or "?"
        if path == run and live:
            state = "running"
        else:
            state = "ended %s at %s" % (end.get("reason") or "?", end.get("date") or "?")
            state += ", saved as \"%s\"" % end["saved"] if end.get("saved") else (
                ", save failed (%s)" % end["save_error"] if end.get("save_error") else ", no save")
        out.append("segment %d: %s from %s, %s" % (n, os.path.basename(path), start, state))
    return out


def review(prefix, name=None, marks=None, baseline=True, out=sys.stdout, connect=None):
    run, live = resolve(prefix, name)
    if not run:
        out.write("No run to read%s. A campaign starts with: hoi4-live play COUNTRY \"GOAL\"\n"
                  % ("" if not name or name == "last" else " at %s" % name))
        return 2
    request = runs.read_json(os.path.join(run, "request.json")) or {}
    lines = ["RUN %s" % run] + campaign_lines(run, live) + [""]
    lines += audit.audit_lines(run)
    snap = live_snapshot(prefix, connect) if live else None
    if snap:
        source = "the running game"
        path = os.path.join(runs.private_dir(os.path.join(run, "gate")),
                            "%s.json" % (str(snap.get("date") or "now").replace(" ", "") or "now"))
        runs.write_json(path, snap)
    else:
        snap = kept_snapshot(run)
        source = "the state kept at the end of the run" if snap else None
    lines.append("")
    if not snap:
        lines.append("GATE: no game state to read (the run is not live and kept none at its end)")
    else:
        app = os.path.join(prefix, "app")
        tag = ((snap.get("reads") or {}).get("hoi4_observe") or {}).get("now", {}).get("player") \
            or request.get("tag")
        base = gate.find_baseline(app, tag, request.get("start")) if baseline and not marks else None
        plan = runs.read_json(os.path.join(run, "plan.json")) or {}
        units = gate.unit_categories(game_dir(prefix, run))
        glines, _ = gate.gate_lines(snap, plan=plan, units=units, baseline=base, marks=marks,
                                    target=request.get("until") if not base else None,
                                    plan_start=request.get("start"))
        lines.append("GATE (from %s)" % source)
        if not units:
            lines.append("  (the game's unit files were not found: roles count every template as line; set the game "
                         "folder with hoi4-live game-dir)")
        lines += glines
    text = "\n".join(lines) + "\n"
    out.write(text)
    runs.write_private(os.path.join(run, "audit.txt"), text)
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(prog="hoi4-live report", description=__doc__.split("\n\n")[0])
    parser.add_argument("--prefix", required=True, help="the install folder")
    parser.add_argument("--marks", nargs=3, type=int, metavar=("CIV", "MIL", "DIVS"),
                        help="civilian factories, military factories and divisions to reach (instead of the AI's)")
    parser.add_argument("--no-baseline", action="store_true", help="do not compare with the stored AI baseline")
    parser.add_argument("run", nargs="?", help="a run folder or its name (default: the running or the last run)")
    args = parser.parse_args(argv)
    return review(os.path.abspath(args.prefix), args.run, tuple(args.marks) if args.marks else None,
                  baseline=not args.no_baseline)


if __name__ == "__main__":
    sys.exit(main())
