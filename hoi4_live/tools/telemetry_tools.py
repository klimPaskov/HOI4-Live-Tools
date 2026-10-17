"""hoi4_telemetry: install, enable, disable and read the telemetry companion mod (hoi4live_telemetry)."""

import os
import re
from pathlib import Path

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
            "campaign_compatible": modgen.campaign_compatible(man), "playset": ps, "store": st}
    config_session = getattr(session, "config_session", None)
    if config_session is not None:
        data["config_backup"] = str(config_session.session_dir)
        data["config_session_state"] = config_session.manifest["state"]
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


def _campaign_change(session, action, raw_version):
    """Campaign configuration is reversible and owned by one durable session."""
    from ..game.session_backup import ConfigSession
    cfg = session.cfg
    with session.ui_mutex:
        if process.running_pids(cfg["window"]["process_name"]):
            raise ToolError("campaign telemetry configuration requires the game to be closed",
                            {"error": "game_running", "action": action})
        config_session = getattr(session, "config_session", None)
        try:
            if action in ("restore", "disable"):
                if config_session is None:
                    raise ToolError("no owned configuration backup to restore; refusing legacy playset restore",
                                    {"error": "config_session_missing"})
                # ConfigSession refuses starting/running owned launches and
                # preserves any unrelated byte changes as recovery conflicts.
                rep = config_session.restore()
                rep["backup_path"] = str(config_session.session_dir)
                if not rep["conflicts"]:
                    session.config_session = None
                return result("campaign configuration restored" if not rep["conflicts"] else
                              "configuration conflicts preserved; recovery is required", **rep)
            if config_session is not None:
                if (config_session.manifest["state"] != "active"
                        or config_session.manifest["game"] is not None
                        or config_session.user_dir != Path(cfg.user_dir).resolve()):
                    raise ToolError("existing configuration session cannot be reused; restore it first",
                                    {"error": "config_session_unavailable",
                                     "backup_path": str(config_session.session_dir)})
            else:
                config_session = ConfigSession.begin(cfg.user_dir, Path(cfg.artifacts_dir) / "config_backups")
                session.config_session = config_session
            man = modgen.generate(cfg.user_dir, cfg.game_dir, raw_version,
                                  watch=modgen.empty_watch(), campaign=True)
            if action == "enable":
                # A user may have started the game during generation. Do not
                # write its playset after that change; leave the backup intact.
                if process.running_pids(cfg["window"]["process_name"]):
                    raise ToolError("game started while generating telemetry; playset left unchanged",
                                    {"error": "game_running", "backup_path": str(config_session.session_dir)})
                config_session.write_telemetry_mod(playset.ENTRY)
            session._telemetry = None
            return result("campaign telemetry generated" + (" and enabled exclusively" if action == "enable" else ""),
                          manifest=man, backup_path=str(config_session.session_dir),
                          config_session_id=config_session.manifest["session_id"],
                          enabled_mods=[playset.ENTRY] if action == "enable" else None,
                          note="launch the game to load this campaign generation")
        except (OSError, RuntimeError, ValueError) as exc:
            raise ToolError("campaign telemetry configuration failed: " + str(exc),
                            {"error": "config_session_failed", "backup_path":
                             str(config_session.session_dir) if config_session is not None else None}) from exc


def _campaign_read_store(session):
    man = modgen.read_manifest(session.cfg.user_dir)
    if not modgen.campaign_compatible(man):
        raise ToolError("campaign reads require campaign telemetry generated with no watches",
                        {"error": "campaign_telemetry_required", "fix": "enable telemetry while the game is closed"})
    store = session.telemetry
    if store.status().get("generation") != man["generation"]:
        raise ToolError("the running telemetry generation is unverified; wait for this campaign generation",
                        {"error": "campaign_telemetry_not_active", "expected_generation": man["generation"]})
    return store


@tool("hoi4_telemetry",
      "The telemetry companion mod hoi4live_telemetry: a tiny generated mod that logs the player's numbers daily "
      "(political power, stability, war support, manpower, factories, divisions, fuel, convoys, resources, "
      "equipment, wars) and a weekly world summary as HOI4LIVE lines in game.log, read by hoi4_state and "
      "hoi4_briefing. status: files, playset, whether the running game loaded them, records seen, suspect "
      "error.log lines. install: (re)generate the files. enable: install, then add it last to dlc_load.json "
      "(backed up first). disable: remove it from dlc_load.json. restore: put back the dlc_load.json from before "
      "the first enable. watch: add (or remove=true) player/global variables and flags to log daily. read: the "
      "latest telemetry records. The game loads mods only at launch: after install/enable/watch, restart it "
      "(hoi4_launch restart=true) and load a save. Campaign install/enable require a closed game, empty watches "
      "and vanilla data; enable selects only observational telemetry under a durable configuration backup. "
      "Campaign disable/restore restore that owned backup only after any owned game has exited.",
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
    if session.mode == "campaign":
        if a == "watch" or args.get("raw"):
            raise ToolError("campaign telemetry forbids watches and raw records",
                            {"error": "campaign_telemetry_restricted"})
        if a in ("install", "enable", "disable", "restore"):
            return _campaign_change(session, a, raw_version)
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
    store = _campaign_read_store(session) if session.mode == "campaign" else session.telemetry
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
