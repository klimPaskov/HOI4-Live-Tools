"""hoi4_telemetry: install, enable, disable and read the telemetry companion mod (hoi4live_telemetry)."""

import os
import re

from ..errors import ToolError
from ..game import process
from ..telemetry import modgen, playset
from .common import result
from .registry import tool

ERROR_HINT = re.compile(r"hoi4live|text promotion|loc(?:alisation|alization)? key|invalid event target", re.I)


def _suspect_errors(session, limit=20):
    """error.log lines that point at the telemetry mod: they name its files or keys, or they are scope/text
    errors written in the same second as a HOI4LIVE line."""
    try:
        with open(session.logs.path("error"), "rb") as f:
            err = f.read().decode("utf-8", "replace").splitlines()
    except OSError:
        return []
    store = session.telemetry
    clocks = set()
    try:
        with open(store.log_path, "rb") as f:
            for line in f.read().decode("utf-8", "replace").splitlines():
                if "HOI4LIVE" in line:
                    clocks.add(line[1:9])
    except OSError:
        pass
    out = []
    for line in err:
        if "hoi4live" in line.lower() or (ERROR_HINT.search(line) and line[1:9] in clocks):
            out.append(line)
    return out[-limit:]


def telemetry_status(session):
    cfg = session.cfg
    man = modgen.read_manifest(cfg.user_dir)
    ps = playset.status(cfg.user_dir)
    store = session.telemetry
    st = store.status()
    data = {"installed": modgen.installed(cfg.user_dir), "mod_dir": modgen.mod_dir(cfg.user_dir),
            "generation": (man or {}).get("generation"), "watch": (man or {}).get("watch"),
            "playset": ps, "store": st}
    running = bool(process.running_pids(cfg["window"]["process_name"]))
    if st.get("generation") and man:
        data["active_in_game"] = st["generation"] == man["generation"]
        if not data["active_in_game"]:
            data["note"] = ("the game logs generation %s but the files are %s: restart the game to load them"
                            % (st["generation"], man["generation"]))
    elif ps["enabled"] and running and st["records"] == 0:
        data["note"] = ("enabled in dlc_load.json, but no HOI4LIVE line in game.log yet: the game must be started "
                        "after enabling, and a game (not the menu) must run for a day")
    elif not ps["enabled"]:
        data["note"] = "not in the playset: hoi4_telemetry action=enable, then restart the game"
    data["suspect_errors"] = _suspect_errors(session)
    return data


@tool("hoi4_telemetry",
      "The telemetry companion mod hoi4live_telemetry: a tiny generated mod that logs the player's numbers daily "
      "(political power, stability, war support, manpower, factories, divisions, fuel, convoys, resources, "
      "equipment, wars) and a weekly world summary as HOI4LIVE lines in game.log, read by hoi4_state and "
      "hoi4_briefing. status: files, playset, whether the running game loaded them, records seen, suspect "
      "error.log lines. install: (re)generate the files. enable: install, then add it last to dlc_load.json "
      "(backed up first). disable: remove it from dlc_load.json. restore: put back the dlc_load.json from before "
      "the first enable. watch: add (or remove=true) player/global variables and flags to log daily. read: the "
      "latest telemetry records. The game loads mods only at launch: after install/enable/watch, restart it "
      "(hoi4_launch restart=true) and load a save.",
      {"properties": {
          "action": {"type": "string", "enum": ["status", "install", "enable", "disable", "restore", "watch", "read"],
                     "default": "status"},
          "variables": {"type": "array", "items": {"type": "string"}, "description": "watch: player country variables"},
          "global_variables": {"type": "array", "items": {"type": "string"}},
          "flags": {"type": "array", "items": {"type": "string"}, "description": "watch: player country flags"},
          "global_flags": {"type": "array", "items": {"type": "string"}},
          "remove": {"type": "boolean", "default": False, "description": "watch: remove the names instead"},
          "kinds": {"type": "array", "items": {"type": "string"},
                    "description": "read: record kinds (day, pol, fac, res, eq, war, watch, wc, we, wld, front, lost, ev)"},
          "raw": {"type": "boolean", "default": False, "description": "read: raw records instead of the snapshot"},
          "limit": {"type": "integer", "minimum": 1, "maximum": 500, "default": 40}}},
      family="understanding", lock="none", phase=2,
      examples=[{"args": {}, "note": "is telemetry installed, enabled and live?"},
                {"args": {"action": "enable"}, "note": "generate and add to the playset (restart the game after)"},
                {"args": {"action": "watch", "variables": ["my_mod_counter"], "flags": ["my_mod_started"]}},
                {"args": {"action": "read", "kinds": ["day", "war"]}},
                {"args": {"action": "disable"}}])
def hoi4_telemetry(session, args):
    cfg = session.cfg
    a = args["action"]
    version, _ = process.game_version(cfg.game_dir) if cfg.game_dir else (None, None)
    raw_version = process.game_version(cfg.game_dir)[1] if cfg.game_dir else None
    running = bool(process.running_pids(cfg["window"]["process_name"]))
    restart_note = "the running game keeps the old files until it is restarted" if running else None
    if a == "status":
        data = telemetry_status(session)
        st = data["store"]
        summ = (f"telemetry {'installed' if data['installed'] else 'not installed'}, "
                f"{'enabled' if data['playset']['enabled'] else 'not enabled'}; {st['records']} record(s)"
                + (f", last day {st.get('last_day')}" if st.get("last_day") else ""))
        return result(summ, **data)
    if a == "install":
        man = modgen.generate(cfg.user_dir, cfg.game_dir, raw_version)
        return result(f"generated {modgen.MOD_NAME} ({man['generation']})", manifest=man, note=restart_note)
    if a == "enable":
        man = modgen.generate(cfg.user_dir, cfg.game_dir, raw_version)
        rep = playset.enable(cfg.user_dir)
        return result(f"telemetry generated ({man['generation']}) and "
                      f"{'added to' if rep['changed'] else 'already in'} dlc_load.json",
                      manifest=man, playset=rep, note=(restart_note or "launch the game to load it"))
    if a == "disable":
        rep = playset.disable(cfg.user_dir)
        return result("removed from dlc_load.json" if rep["changed"] else "was not enabled", playset=rep,
                      note=restart_note)
    if a == "restore":
        rep = playset.restore(cfg.user_dir)
        return result(f"dlc_load.json restored from {os.path.basename(rep['restored_from'])}", playset=rep,
                      note=restart_note)
    if a == "watch":
        man = modgen.read_manifest(cfg.user_dir) or {}
        watch = man.get("watch") or modgen.empty_watch()
        keys = ("variables", "global_variables", "flags", "global_flags")
        if not any(args.get(k) for k in keys):
            return result("watch list", watch=watch)
        for k in keys:
            for name in args.get(k) or []:
                if args["remove"]:
                    watch[k] = [x for x in watch[k] if x != name]
                elif name not in watch[k]:
                    watch[k].append(name)
        man = modgen.generate(cfg.user_dir, cfg.game_dir, raw_version, watch=watch)
        return result(f"watch list updated ({man['generation']})", watch=man["watch"],
                      note=restart_note or "launch the game to load it")
    # read
    store = session.telemetry
    if args["raw"]:
        kinds = set(args.get("kinds") or [])
        rows = []
        for hist in store.history.values():
            rows += [r for r in hist if not kinds or "day" in kinds]
        for k, by_tag in store.latest.items():
            if k != "day" and (not kinds or k in kinds):
                rows += list(by_tag.values())
        for k, g in store.groups.items():
            if not kinds or k in kinds:
                rows += g["rows"]
        if not kinds or "ev" in kinds:
            rows += list(store.events)
        rows.sort(key=lambda r: (r.get("n") or 0, r.get("_wall") or 0))
        rows = rows[-args["limit"]:]
        return result(f"{len(rows)} record(s)", records=rows, status=store.status())
    snap = store.snapshot()
    if snap is None:
        raise ToolError("no telemetry records in game.log yet", telemetry_status(session))
    if session.mode == "campaign":
        from ..state.visibility import campaign_view
        snap = campaign_view(snap, store.player())
    kinds = args.get("kinds")
    if kinds:
        keep = {"tag", "date", "n"}
        mapping = {"wc": "week", "we": "week", "wld": "week", "front": "week", "lost": "week", "war": "wars",
                   "ev": "events"}
        keep |= {mapping.get(k, k) for k in kinds}
        snap = {k: v for k, v in snap.items() if k in keep}
    return result(f"telemetry for {snap['tag']} on {snap['date']}", snapshot=snap, status=store.status())
