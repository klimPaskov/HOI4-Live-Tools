"""Add the telemetry mod to the playset the game reads at launch (dlc_load.json), and take it out again.

dlc_load.json lists the enabled mods in load order ({"enabled_mods": ["mod/x.mod", ...], "disabled_dlcs": [...]}).
The game reads it at launch when started directly (hoi4.exe, as hoi4_launch does); the Paradox launcher rewrites it
from its own playset database when the game is started through the launcher.

Every change backs the file up first to <user_dir>/hoi4live_backups/dlc_load_<time>.json and records the backup in
<user_dir>/hoi4live_backups/telemetry_playset.json, so `restore` can put back the exact original. `disable` only
removes our entry and keeps everything else (including changes made after enabling). Our entry always goes last,
after the existing mods, so their load order is untouched.
"""

import datetime
import json
import os
import shutil

from ..errors import ToolError
from . import modgen

ENTRY = "mod/" + modgen.MOD_NAME + ".mod"


def dlc_load_path(user_dir):
    return os.path.join(user_dir, "dlc_load.json")


def backups_dir(user_dir):
    return os.path.join(user_dir, "hoi4live_backups")


def state_path(user_dir):
    return os.path.join(backups_dir(user_dir), "telemetry_playset.json")


def read(user_dir):
    p = dlc_load_path(user_dir)
    try:
        with open(p, encoding="utf-8") as f:
            raw = f.read()
    except OSError:
        return None, None
    try:
        return json.loads(raw), raw
    except ValueError:
        raise ToolError(f"{p} is not valid JSON; not touching it")


def read_state(user_dir):
    try:
        with open(state_path(user_dir), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def _write_state(user_dir, state):
    os.makedirs(backups_dir(user_dir), exist_ok=True)
    with open(state_path(user_dir), "w", encoding="utf-8") as f:
        json.dump(state, f, indent=1)


def backup(user_dir, reason):
    p = dlc_load_path(user_dir)
    if not os.path.exists(p):
        return None
    os.makedirs(backups_dir(user_dir), exist_ok=True)
    stamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    dst = os.path.join(backups_dir(user_dir), f"dlc_load_{stamp}_{reason}.json")
    shutil.copy2(p, dst)
    return dst


def _write(user_dir, data):
    p = dlc_load_path(user_dir)
    tmp = p + ".hoi4live_tmp"
    with open(tmp, "w", encoding="utf-8", newline="") as f:
        f.write(json.dumps(data, separators=(",", ":"), ensure_ascii=False))
    os.replace(tmp, p)


def status(user_dir):
    data, _ = read(user_dir)
    st = read_state(user_dir)
    mods = (data or {}).get("enabled_mods", [])
    return {"dlc_load": dlc_load_path(user_dir), "enabled": ENTRY in mods, "enabled_mods": mods,
            "position": mods.index(ENTRY) if ENTRY in mods else None, "original_backup": st.get("original_backup"),
            "history": st.get("history", [])[-6:]}


def enable(user_dir):
    if not modgen.installed(user_dir):
        raise ToolError("the telemetry mod is not generated yet (hoi4_telemetry action=install)")
    data, raw = read(user_dir)
    if data is None:
        data = {"enabled_mods": [], "disabled_dlcs": []}
    mods = list(data.get("enabled_mods", []))
    if ENTRY in mods:
        return {"changed": False, "enabled_mods": mods, "note": "already enabled"}
    dst = backup(user_dir, "before_enable")
    st = read_state(user_dir)
    if not st.get("original_backup"):
        st["original_backup"] = dst
    st.setdefault("history", []).append({"time": datetime.datetime.now().isoformat(timespec="seconds"),
                                         "action": "enable", "backup": dst, "before": list(mods)})
    mods.append(ENTRY)
    data["enabled_mods"] = mods
    data.setdefault("disabled_dlcs", [])
    _write(user_dir, data)
    _write_state(user_dir, st)
    return {"changed": True, "enabled_mods": mods, "backup": dst}


def disable(user_dir):
    data, raw = read(user_dir)
    if data is None:
        return {"changed": False, "note": "no dlc_load.json"}
    mods = list(data.get("enabled_mods", []))
    if ENTRY not in mods:
        return {"changed": False, "enabled_mods": mods, "note": "not enabled"}
    dst = backup(user_dir, "before_disable")
    data["enabled_mods"] = [m for m in mods if m != ENTRY]
    _write(user_dir, data)
    st = read_state(user_dir)
    st.setdefault("history", []).append({"time": datetime.datetime.now().isoformat(timespec="seconds"),
                                         "action": "disable", "backup": dst, "before": mods})
    _write_state(user_dir, st)
    return {"changed": True, "enabled_mods": data["enabled_mods"], "backup": dst}


def restore(user_dir, backup_path=None):
    st = read_state(user_dir)
    src = backup_path or st.get("original_backup")
    if not src or not os.path.exists(src):
        raise ToolError("no recorded dlc_load.json backup to restore", {"state": st})
    with open(src, encoding="utf-8") as f:
        json.load(f)                                  # refuse to restore a corrupt file
    dst = backup(user_dir, "before_restore")
    shutil.copy2(src, dlc_load_path(user_dir))
    st.setdefault("history", []).append({"time": datetime.datetime.now().isoformat(timespec="seconds"),
                                         "action": "restore", "from": src, "backup": dst})
    _write_state(user_dir, st)
    data, _ = read(user_dir)
    return {"changed": True, "restored_from": src, "enabled_mods": (data or {}).get("enabled_mods"), "backup": dst}
