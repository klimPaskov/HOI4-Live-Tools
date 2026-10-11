"""Durable, byte-exact config restoration for a single explicitly owned game run.

This module does not inspect, launch, or stop processes. The caller must record an
owned launch before launching and record its verified exit before restoring.
Unexpected edits remain untouched and keep the session available for recovery.
"""

import hashlib
import json
import os
from pathlib import Path
import tempfile
import uuid


CONFIG_FILES = ("dlc_load.json", "settings.txt", "pdx_settings.txt")


def _digest(data):
    return hashlib.sha256(data).hexdigest()


def _sync_directory(path):
    # Windows does not expose directory fsync through os.open. File contents are
    # fsynced before every atomic replace on all platforms.
    if os.name != "nt":
        fd = os.open(str(path), os.O_RDONLY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)


def _atomic_write(path, data):
    fd, temporary = tempfile.mkstemp(prefix=".hoi4-session-", dir=str(path.parent))
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        _sync_directory(path.parent)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def _safe_file(root, name):
    if name not in CONFIG_FILES:
        raise ValueError("Unknown config filename: %s" % name)
    path = root / name
    if path.is_symlink() or path.resolve() != root / name:
        raise ValueError("Config target escapes the recorded directory: %s" % path)
    if path.exists() and not path.is_file():
        raise ValueError("Config target is not a regular file: %s" % path)
    return path


def _read(root, name):
    path = _safe_file(root, name)
    try:
        return path.read_bytes()
    except FileNotFoundError:
        return None


class ConfigSession:
    """Baseline and mutation journal stored under an exclusive directory lock.

    ``owner_id`` should identify a particular launch (for example PID plus creation
    time), not a process name. ``expected_rewrites`` accepts hashes attested by the
    caller as writes from that owned game. It must not be populated by blindly
    accepting every current file: external writes cannot be inferred from bytes.
    """

    @classmethod
    def begin(cls, user_dir, backup_root):
        user_dir = Path(user_dir).resolve(strict=True)
        backup_root = Path(backup_root).resolve()
        if not user_dir.is_dir():
            raise ValueError("User config directory must exist")
        backup_root.mkdir(parents=True, exist_ok=True)
        session_id = uuid.uuid4().hex
        session_dir = backup_root / session_id
        lock_path = cls._lock_path(backup_root, user_dir)
        try:
            fd = os.open(str(lock_path), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        except FileExistsError:
            raise RuntimeError("A config session already owns this directory; recover it first")
        with os.fdopen(fd, "wb") as stream:
            stream.write(session_id.encode("ascii"))
            stream.flush()
            os.fsync(stream.fileno())
        _sync_directory(backup_root)
        try:
            session_dir.mkdir()
            baseline = {}
            for name in CONFIG_FILES:
                data = _read(user_dir, name)
                baseline[name] = {"exists": data is not None,
                                  "sha256": _digest(data) if data is not None else None,
                                  "size": len(data) if data is not None else None}
                if data is not None:
                    _atomic_write(session_dir / name, data)
            manifest = {"version": 1, "session_id": session_id,
                        "user_dir": str(user_dir), "backup_root": str(backup_root),
                        "baseline": baseline, "writes": {}, "created": [],
                        "restored": [], "state": "active", "game": None,
                        "expected_rewrites": {}}
            session = cls(user_dir, backup_root, session_dir, manifest)
            session._save()
            # Refuse mutation if a config changed while taking its baseline.
            if session.file_hashes() != {n: b["sha256"] for n, b in baseline.items()}:
                raise RuntimeError("Configs changed while taking the baseline")
            return session
        except BaseException:
            # No config mutation can occur before begin returns. Preserve any
            # partial backup for inspection, but release the unused ownership.
            lock_path.unlink()
            _sync_directory(backup_root)
            raise

    @staticmethod
    def _lock_path(backup_root, user_dir):
        key = _digest(os.path.normcase(str(user_dir)).encode("utf-8"))[:24]
        return backup_root / ("active-" + key + ".lock")

    def __init__(self, user_dir, backup_root, session_dir, manifest):
        self.user_dir, self.backup_root = user_dir, backup_root
        self.session_dir, self.manifest = session_dir, manifest
        self.lock_path = self._lock_path(backup_root, user_dir)

    @classmethod
    def recover(cls, session_dir, user_dir, backup_root):
        """Read a known session; never trust target paths supplied by its journal."""
        user_dir = Path(user_dir).resolve(strict=True)
        backup_root = Path(backup_root).resolve(strict=True)
        session_dir = Path(session_dir).resolve(strict=True)
        if session_dir.parent != backup_root:
            raise ValueError("Session directory is outside the backup root")
        manifest_path = session_dir / "manifest.json"
        if manifest_path.is_symlink():
            raise ValueError("Manifest must not be a symlink")
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        if (manifest.get("version") != 1 or manifest.get("session_id") != session_dir.name
                or manifest.get("user_dir") != str(user_dir)
                or manifest.get("backup_root") != str(backup_root)
                or set(manifest.get("baseline", {})) != set(CONFIG_FILES)):
            raise ValueError("Session identity or baseline does not match")
        session = cls(user_dir, backup_root, session_dir, manifest)
        session._check_owner()
        for name in CONFIG_FILES:
            session._baseline_bytes(name)
            _safe_file(user_dir, name)
        return session

    def _check_owner(self):
        if self.manifest["state"] == "restored" and not self.lock_path.exists():
            return
        if (self.lock_path.is_symlink() or not self.lock_path.is_file()
                or self.lock_path.read_text(encoding="ascii") != self.manifest["session_id"]):
            raise RuntimeError("Session no longer owns the config directory")

    def _save(self):
        _atomic_write(self.session_dir / "manifest.json",
                      (json.dumps(self.manifest, indent=2) + "\n").encode("utf-8"))

    def _baseline_bytes(self, name):
        entry = self.manifest["baseline"][name]
        if not entry["exists"]:
            if entry["sha256"] is not None or entry["size"] is not None:
                raise ValueError("Invalid absent baseline")
            return None
        path = _safe_file(self.session_dir, name)
        data = path.read_bytes()
        if _digest(data) != entry["sha256"] or len(data) != entry["size"]:
            raise ValueError("Baseline backup is damaged: " + name)
        return data

    def file_hashes(self):
        """Return current hashes, with None representing a missing file."""
        return {name: _digest(data) if data is not None else None
                for name in CONFIG_FILES for data in [_read(self.user_dir, name)]}

    def write_telemetry_mod(self, mod_descriptor):
        """Atomically enable only this descriptor, preserving all other JSON keys."""
        self._check_owner()
        if self.manifest["state"] != "active" or self.manifest["game"] is not None:
            raise RuntimeError("Playset changes require an active session before launch")
        if not isinstance(mod_descriptor, str) or not mod_descriptor:
            raise ValueError("A telemetry mod descriptor is required")
        name = "dlc_load.json"
        current = _read(self.user_dir, name)
        current_hash = _digest(current) if current is not None else None
        expected = self.manifest["writes"].get(name, self.manifest["baseline"][name]["sha256"])
        if current_hash != expected:
            raise RuntimeError("Playset changed outside this session")
        data = json.loads(current.decode("utf-8-sig")) if current is not None else {}
        if not isinstance(data, dict):
            raise ValueError("Playset JSON must be an object")
        data["enabled_mods"] = [mod_descriptor]
        replacement = (json.dumps(data, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
        # Persist the intended write BEFORE touching the target, permitting
        # recovery on either side of the atomic replacement.
        self.manifest["writes"][name] = _digest(replacement)
        if not self.manifest["baseline"][name]["exists"] and name not in self.manifest["created"]:
            self.manifest["created"].append(name)
        self._save()
        _atomic_write(_safe_file(self.user_dir, name), replacement)

    def mark_game_starting(self, owner_id):
        """Persist launch intent before invoking the process launcher."""
        self._check_owner()
        if self.manifest["state"] != "active" or self.manifest["game"] is not None or not owner_id:
            raise RuntimeError("A session can own exactly one game launch")
        self.manifest["game"] = {"owner_id": str(owner_id), "phase": "starting"}
        self._save()

    def mark_game_started(self, owner_id):
        self._check_owner()
        game = self.manifest["game"]
        if not game or game["owner_id"] != str(owner_id) or game["phase"] != "starting":
            raise RuntimeError("Launch identity does not match this session")
        game["phase"] = "running"
        self._save()

    def mark_game_exited(self, owner_id, expected_rewrites=None):
        """Record verified owned exit (or verified failed launch) and known rewrites."""
        self._check_owner()
        game = self.manifest["game"]
        if not game or game["owner_id"] != str(owner_id) or game["phase"] not in ("starting", "running"):
            raise RuntimeError("Exit identity does not match this session")
        expected_rewrites = dict(expected_rewrites or {})
        for name, digest in expected_rewrites.items():
            if name not in CONFIG_FILES or (digest is not None and
                    (not isinstance(digest, str) or len(digest) != 64 or
                     any(c not in "0123456789abcdef" for c in digest))):
                raise ValueError("Expected rewrite must be a config filename and SHA-256")
        self.manifest["expected_rewrites"] = expected_rewrites
        game["phase"] = "exited"
        self._save()

    def restore(self):
        """Restore safe files; return per-file conflicts without overwriting them.

        Restoration is idempotent, including interruption after a config replace
        but before its completion record. A conflicting session retains its lock.
        """
        self._check_owner()
        game = self.manifest["game"]
        if game and game["phase"] != "exited":
            raise RuntimeError("The owned game must be verified exited before restoration")
        if self.manifest["state"] == "restored":
            if self.lock_path.exists():
                self.lock_path.unlink()
                _sync_directory(self.backup_root)
            return {"restored": list(CONFIG_FILES), "conflicts": {}}
        conflicts = {}
        for name in CONFIG_FILES:
            baseline = self._baseline_bytes(name)
            baseline_hash = self.manifest["baseline"][name]["sha256"]
            try:
                current = _read(self.user_dir, name)
            except ValueError as error:
                conflicts[name] = str(error)
                continue
            current_hash = _digest(current) if current is not None else None
            allowed = {baseline_hash}
            if name in self.manifest["writes"]:
                allowed.add(self.manifest["writes"][name])
            if name in self.manifest["expected_rewrites"]:
                allowed.add(self.manifest["expected_rewrites"][name])
            if current_hash not in allowed:
                conflicts[name] = "Unexpected config change; current bytes preserved"
                continue
            if baseline is None and current is not None and name not in self.manifest["created"]:
                conflicts[name] = "File did not exist at baseline and was not created by this manager"
                continue
            if current_hash != baseline_hash:
                target = _safe_file(self.user_dir, name)
                # Recheck immediately before mutation to catch intervening edits.
                checked = _read(self.user_dir, name)
                if checked != current:
                    conflicts[name] = "Config changed during restoration"
                    continue
                if baseline is None:
                    target.unlink()
                    _sync_directory(self.user_dir)
                else:
                    _atomic_write(target, baseline)
            if name not in self.manifest["restored"]:
                self.manifest["restored"].append(name)
            self._save()
        if not conflicts:
            self.manifest["state"] = "restored"
            self._save()
            self.lock_path.unlink()
            _sync_directory(self.backup_root)
        return {"restored": list(self.manifest["restored"]), "conflicts": conflicts}
