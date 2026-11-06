"""DLC prepare step (maintainer decision R-6): launch without the Steam client and still keep the DLC.

The Steam client must be closed for every run, because Live Tools never lets Steam see a run. Started without
Steam, the game finds no DLC in a user folder that lacks the two files the Paradox launcher keeps there:
`launcher-v2.sqlite` (the DLC ownership records) and `dlc_signature`. The folder you normally play from has
them already. A fresh or redirected folder (`HOI4_LIVE_USER_DIR`, a `gameDataPath` redirect, a scratch folder
for mod tests) does not, and the game then loads 0 DLC.

`apply` copies the two files from the normal user folder into the run's user folder. Every file it replaces is
backed up byte for byte first, and a small journal records what was written. `restore` puts the originals back
(or removes the copies when the folder had none) once the game has exited. It never overwrites a file that
changed after the copy without being told to: such a file is reported as a conflict and left alone.

Nothing here starts or stops a process and nothing reads the DLC records: the files are copied as they are.
When the run's folder is the normal folder there is nothing to copy and nothing to restore.
"""

import hashlib
import json
import os
import time
from pathlib import Path

from ..errors import ToolError
from .session_backup import _atomic_write

DLC_FILES = ("launcher-v2.sqlite", "dlc_signature")
SQLITE_MAGIC = b"SQLite format 3\x00"
MANIFEST = "manifest.json"


def _digest(data):
    return hashlib.sha256(data).hexdigest()


def _same_dir(a, b):
    try:
        return os.path.samefile(a, b)
    except OSError:
        return os.path.normcase(os.path.realpath(a)) == os.path.normcase(os.path.realpath(b))


def _read(path):
    """The bytes of a regular file, None when it does not exist. A link is refused, never followed."""
    if os.path.islink(path):
        raise ToolError(f"{path} is a link; refusing to copy over or restore through it", {"error": "dlc_link"})
    try:
        with open(path, "rb") as f:
            return f.read()
    except FileNotFoundError:
        return None
    except (IsADirectoryError, PermissionError) as exc:
        raise ToolError(f"cannot read {path}: {exc}", {"error": "dlc_unreadable"})


def journal_dir(backup_root, user_dir):
    """One journal per run folder, so a server that crashed mid-run can restore from a new session."""
    key = _digest(os.path.normcase(os.path.realpath(user_dir)).encode("utf-8"))[:16]
    return os.path.join(backup_root, key)


def _load(jdir):
    path = os.path.join(jdir, MANIFEST)
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except FileNotFoundError:
        return None
    except (OSError, ValueError) as exc:
        raise ToolError(f"the DLC prepare journal {path} is unreadable ({exc}); the backups next to it are kept",
                        {"error": "dlc_journal_unreadable", "journal": path})


def _save(jdir, manifest):
    os.makedirs(jdir, exist_ok=True)
    _atomic_write(Path(os.path.join(jdir, MANIFEST)), (json.dumps(manifest, indent=1) + "\n").encode("utf-8"))


def _short(h):
    return h[:12] if h else None


def inspect(user_dir, source_dir):
    """What each folder holds, for hoi4_prepare status and for the checks of apply."""
    rows = {}
    for name in DLC_FILES:
        row = {}
        for label, folder in (("source", source_dir), ("target", user_dir)):
            try:
                data = _read(os.path.join(folder, name)) if folder else None
            except ToolError as exc:
                row[label] = {"error": str(exc)}
                continue
            row[label] = ({"exists": False} if data is None
                          else {"exists": True, "size": len(data), "sha256": _short(_digest(data))})
        rows[name] = row
    return rows


def status(user_dir, source_dir, backup_root):
    """Read-only report: both folders, the journal of an earlier prepare, and whether a prepare is needed."""
    jdir = journal_dir(backup_root, user_dir)
    manifest = _load(jdir)
    same = bool(source_dir) and os.path.isdir(source_dir) and os.path.isdir(user_dir) and _same_dir(source_dir, user_dir)
    files = inspect(user_dir, source_dir)
    ok = all(r["target"].get("exists") for r in files.values())
    matches = all(r["source"].get("exists") and r["target"].get("sha256") == r["source"].get("sha256")
                  for r in files.values())
    if same:
        need = not ok
        why = ("the run's folder is the source folder and lacks DLC files; start the Paradox launcher once with "
               "your DLC enabled" if need else "the run's folder is the normal user folder and has both files")
    else:
        need = not matches
        why = "the run's folder lacks or differs from the source's DLC files" if need else "both files match the source"
    out = {"user_dir": user_dir, "source_dir": source_dir, "same_folder": same, "files": files,
           "needs_prepare": need, "reason": why,
           "journal": None if manifest is None else {"state": manifest["state"], "path": os.path.join(jdir, MANIFEST),
                                                     "files": manifest["files"], "applied_at": manifest.get("applied_at")}}
    return out


def apply(user_dir, source_dir, backup_root, now=None):
    """Copy the two DLC files from source_dir into user_dir, backing up what they replace. Idempotent.

    Raises ToolError when the source is missing or damaged, or when an earlier prepare of this folder is still
    waiting to be restored with different content."""
    if not source_dir or not os.path.isdir(source_dir):
        raise ToolError(f"the source folder with the DLC files was not found ({source_dir}); set dlc_source_dir in "
                        "config.json or pass source_dir", {"error": "dlc_source_missing"})
    same = os.path.isdir(user_dir) and _same_dir(source_dir, user_dir)
    sources = {}
    for name in DLC_FILES:
        data = _read(os.path.join(source_dir, name))
        if not data:
            raise ToolError(f"{name} is missing or empty in {source_dir}. Start the Paradox launcher once with your "
                            "DLC enabled, close it, then prepare again.", {"error": "dlc_source_incomplete", "file": name})
        if name.endswith(".sqlite") and not data.startswith(SQLITE_MAGIC):
            raise ToolError(f"{name} in {source_dir} is not an SQLite database; not copying it",
                            {"error": "dlc_source_damaged", "file": name})
        sources[name] = data
    warnings = []
    wal = os.path.join(source_dir, "launcher-v2.sqlite-wal")
    if os.path.isfile(wal) and os.path.getsize(wal) > 0:
        warnings.append("the source has an unmerged launcher-v2.sqlite-wal; close the Paradox launcher so its latest "
                        "DLC records are in the main file")
    if same:
        return {"copied": [], "already_in_place": list(DLC_FILES), "same_folder": True, "warnings": warnings,
                "user_dir": user_dir, "source_dir": source_dir}

    jdir = journal_dir(backup_root, user_dir)
    manifest = _load(jdir)
    if manifest and manifest.get("state") == "applied":
        # an earlier prepare is still active: running it again is fine when it wrote the same bytes
        for name in DLC_FILES:
            if manifest["files"][name]["written_sha256"] != _digest(sources[name]):
                raise ToolError("an earlier DLC prepare of this folder is still active with different files; run "
                                "hoi4_prepare action=restore first", {"error": "dlc_prepare_active", "journal": jdir})
    created_dir = not os.path.isdir(user_dir)
    os.makedirs(user_dir, exist_ok=True)
    files = dict(manifest["files"]) if manifest and manifest.get("state") == "applied" else {}
    copied, kept = [], []
    for name in DLC_FILES:
        target = os.path.join(user_dir, name)
        current = _read(target)
        written = _digest(sources[name])
        entry = files.get(name)
        if entry is None:
            entry = {"existed": current is not None, "baseline_sha256": _digest(current) if current is not None else None,
                     "written_sha256": written}
            if current is not None and current != sources[name]:
                os.makedirs(jdir, exist_ok=True)
                _atomic_write(Path(os.path.join(jdir, name)), current)       # the backup, before anything is replaced
        elif current is not None and _digest(current) not in (entry["written_sha256"], entry["baseline_sha256"]):
            raise ToolError(f"{name} in {user_dir} changed since the earlier prepare; run hoi4_prepare action=restore "
                            "first", {"error": "dlc_prepare_conflict", "file": name})
        entry["written_sha256"] = written
        files[name] = entry
    manifest = {"version": 1, "state": "applied", "user_dir": user_dir, "source_dir": source_dir,
                "applied_at": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(now or time.time())),
                "created_dir": created_dir, "files": files}
    _save(jdir, manifest)                                  # intent first: a crash after this point can still restore
    for name in DLC_FILES:
        target = os.path.join(user_dir, name)
        current = _read(target)
        if current == sources[name]:
            kept.append(name)
            continue
        _atomic_write(Path(target), sources[name])
        copied.append(name)
    return {"copied": copied, "already_matching": kept, "same_folder": False, "created_dir": created_dir,
            "warnings": warnings, "user_dir": user_dir, "source_dir": source_dir,
            "journal": os.path.join(jdir, MANIFEST)}


def restore(user_dir, backup_root, force=False):
    """Put the run folder's DLC files back as they were before apply. Returns what was restored or kept.

    A file whose bytes are neither the copy nor the original changed after the prepare (the game or the launcher
    rewrote it). It is reported as a conflict and left alone, unless force=True: then the current bytes are kept
    next to the backup as `<name>.after-run` and the original is put back."""
    jdir = journal_dir(backup_root, user_dir)
    manifest = _load(jdir)
    if manifest is None or manifest.get("state") != "applied":
        return {"restored": [], "removed": [], "conflicts": {}, "nothing_to_restore": True}
    restored, removed, conflicts = [], [], {}
    for name in DLC_FILES:
        entry = manifest["files"][name]
        target = os.path.join(user_dir, name)
        current = _read(target)
        digest = _digest(current) if current is not None else None
        baseline = entry["baseline_sha256"]
        if digest == baseline:
            continue                                        # already as it was (or never changed)
        if digest != entry["written_sha256"] and current is not None:
            if not force:
                conflicts[name] = ("changed after the prepare (neither the copy nor the original); left as it is. "
                                   "hoi4_prepare action=restore force=true restores the original and keeps the "
                                   "current file as " + name + ".after-run")
                continue
            os.makedirs(jdir, exist_ok=True)
            _atomic_write(Path(os.path.join(jdir, name + ".after-run")), current)
        if entry["existed"]:
            try:
                with open(os.path.join(jdir, name), "rb") as f:
                    original = f.read()
            except OSError as exc:
                conflicts[name] = f"the backup of the original is missing ({exc}); the file was not touched"
                continue
            if _digest(original) != baseline:
                conflicts[name] = "the backup of the original is damaged; the file was not touched"
                continue
            _atomic_write(Path(target), original)
            restored.append(name)
        elif current is not None:
            os.remove(target)
            removed.append(name)
    if conflicts:
        return {"restored": restored, "removed": removed, "conflicts": conflicts, "journal": os.path.join(jdir, MANIFEST)}
    manifest["state"] = "restored"
    _save(jdir, manifest)
    kept_after_run = []
    for fname in os.listdir(jdir):
        if fname.endswith(".after-run"):
            kept_after_run.append(os.path.join(jdir, fname))
        elif fname != MANIFEST:
            try:
                os.remove(os.path.join(jdir, fname))
            except OSError:
                pass
    out = {"restored": restored, "removed": removed, "conflicts": {}, "journal": os.path.join(jdir, MANIFEST)}
    if kept_after_run:
        out["after_run_copies"] = kept_after_run
    return out
