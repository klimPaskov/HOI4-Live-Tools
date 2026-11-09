"""Effective game content: the install plus the enabled mods in load order.

The same merge as the native layer (native/src/launch/macos/modlayer.rs) and
hoi4_live/gamedata/scriptdb.effective_files, with `events/` included:

  * the enabled mods come from the user folder's `dlc_load.json` (`enabled_mods`, descriptor
    paths relative to the user folder), in that order;
  * each `.mod` descriptor gives `path` (or `archive`; relative paths are taken from the user
    folder) and any number of `replace_path` entries; `name` is kept for sources;
  * a mod file with the same relative path replaces the earlier one;
  * a mod's `replace_path = "a/b"` hides the files directly in `a/b` from the install and from
    earlier mods (not the files in its sub-folders), before the mod's own files are added.

Relative paths are compared without case (the game reads them that way on Windows; the
returned paths keep their spelling). The DLC folders are not layered (`integrated_dlc/` belongs
to the install).

Stdlib only, Python 3.9+.
"""
import fnmatch
import hashlib
import json
import os
import re

LAYERED = ("map", "history", "common", "localisation", "interface", "events")


class Mod:
    """One enabled mod: its content folder, replace_path entries, display name, descriptor."""

    def __init__(self, path, replace_paths=(), name=None, descriptor=None):
        self.path = os.path.normpath(path)
        self.replace_paths = [r.strip().strip("/").replace("\\", "/") for r in replace_paths if r.strip()]
        self.name = name or os.path.basename(self.path)
        self.descriptor = descriptor

    def as_dict(self):
        """The {path, replace_path, name} form hoi4_live.telemetry.modgen.mod_paths returns."""
        return {"path": self.path, "replace_path": list(self.replace_paths), "name": self.name}

    def __repr__(self):
        return f"Mod({self.name!r}, {self.path!r}, replace={self.replace_paths})"


def _descriptor_fields(text):
    path, name, replace = None, None, []
    for line in text.splitlines():
        line = line.split("#", 1)[0].strip()
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        key, value = key.strip(), value.strip().strip('"')
        if key in ("path", "archive") and value:
            path = value
        elif key == "replace_path" and value:
            replace.append(value.rstrip("/"))
        elif key == "name" and value:
            name = value
    return path, name, replace


def parse_descriptor(text, userdir, descriptor=None):
    """Mod from a `.mod` descriptor (`path`/`archive`, `replace_path`, `name`), or None without a
    path. Relative paths are taken from the user folder, as the launcher writes them."""
    path, name, replace = _descriptor_fields(text)
    if not path:
        return None
    if not os.path.isabs(path):
        path = os.path.join(userdir, path)
    return Mod(path, replace, name, descriptor)


def parse_enabled_mods(text):
    """`enabled_mods` of a dlc_load.json text (descriptor paths relative to the user folder)."""
    try:
        data = json.loads(text)
    except ValueError:
        return []
    mods = data.get("enabled_mods") if isinstance(data, dict) else None
    return [m for m in mods if isinstance(m, str)] if isinstance(mods, list) else []


def read_descriptor(path, userdir=None):
    userdir = userdir or os.path.dirname(os.path.dirname(os.path.abspath(path)))
    try:
        with open(path, encoding="utf-8-sig", errors="replace") as fh:
            return parse_descriptor(fh.read(), userdir, path)
    except OSError:
        return None


def mods_from_userdir(userdir, exclude=()):
    """The enabled mods of a user folder in load order (dlc_load.json + descriptors). Mods whose
    folder is missing are left out, as the native layer does. `exclude` drops descriptors whose
    path contains one of the given strings."""
    try:
        with open(os.path.join(userdir, "dlc_load.json"), encoding="utf-8-sig") as fh:
            enabled = parse_enabled_mods(fh.read())
    except OSError:
        return []
    out = []
    for rel in enabled:
        if any(x in rel for x in exclude):
            continue
        desc = rel if os.path.isabs(rel) else os.path.join(userdir, rel)
        m = read_descriptor(desc, userdir)
        if m and os.path.isdir(m.path):
            out.append(m)
    return out


def mod_from_arg(arg, userdir=None):
    """A mod given on a command line: a `.mod` descriptor, or a mod content folder (its own
    descriptor.mod, when present, gives the name and replace_path entries)."""
    if arg.lower().endswith(".mod") and os.path.isfile(arg):
        return read_descriptor(arg, userdir)
    if os.path.isdir(arg):
        name, replace = None, []
        desc = os.path.join(arg, "descriptor.mod")
        if os.path.isfile(desc):
            with open(desc, encoding="utf-8-sig", errors="replace") as fh:
                _, name, replace = _descriptor_fields(fh.read())
        return Mod(arg, replace, name, desc if os.path.isfile(desc) else None)
    return None


def _as_mod(m):
    if isinstance(m, Mod):
        return m
    if isinstance(m, dict):
        return Mod(m["path"], m.get("replace_path") or m.get("replace_paths") or (), m.get("name"))
    return Mod(str(m))


def _walk(base, rel, out, src):
    try:
        names = sorted(os.listdir(base))
    except OSError:
        return
    for n in names:
        p = os.path.join(base, n)
        key = f"{rel}/{n}"
        if os.path.isdir(p):
            _walk(p, key, out, src)
        else:
            out[key.lower()] = (key, p, src)


class Content:
    """The merged file view of the install and its enabled mods."""

    def __init__(self, game_dir, mods=()):
        self.game_dir = os.path.normpath(game_dir)
        self.mods = [_as_mod(m) for m in mods]
        self._merged = None
        self._by_path = None

    @classmethod
    def from_userdir(cls, game_dir, userdir, exclude=()):
        return cls(game_dir, mods_from_userdir(userdir, exclude) if userdir else [])

    def merged(self):
        """{lower relative path: (relative path, file path, mod name or None)} for LAYERED."""
        if self._merged is None:
            files = {}
            for top in LAYERED:
                _walk(os.path.join(self.game_dir, top), top, files, None)
            for m in self.mods:
                for r in m.replace_paths:
                    prefix = r.lower() + "/"
                    for k in [k for k in files if k.startswith(prefix) and "/" not in k[len(prefix):]]:
                        del files[k]
                for top in LAYERED:
                    _walk(os.path.join(m.path, top), top, files, m.name)
            self._merged = files
            self._by_path = {p: src for _, p, src in files.values()}
        return self._merged

    def file(self, rel):
        """The effective file of a relative path ("history/units/ITA_1936.txt"), or None."""
        hit = self.merged().get(rel.replace("\\", "/").strip("/").lower())
        return hit[1] if hit else None

    def files(self, folder, ext=(".txt",), recursive=False):
        """Effective files of a folder, sorted by relative path (the game's load order)."""
        prefix = folder.replace("\\", "/").strip("/").lower() + "/"
        ext = tuple(e.lower() for e in ext) if ext else None
        out = []
        for k in sorted(self.merged()):
            if not k.startswith(prefix):
                continue
            tail = k[len(prefix):]
            if not recursive and "/" in tail:
                continue
            if ext and not tail.endswith(ext):
                continue
            out.append(self._merged[k][1])
        return out

    def glob(self, folder, pattern):
        """Effective files directly in `folder` whose name matches the glob (case-insensitive)."""
        pat = pattern.lower()
        return [p for p in self.files(folder, ext=None) if fnmatch.fnmatch(os.path.basename(p).lower(), pat)]

    def source(self, path):
        """Name of the mod that supplied a file, or None for the install."""
        self.merged()
        return self._by_path.get(path)

    def relpath(self, path):
        """Relative content path of an effective file ("common/decisions/ITA.txt")."""
        for rel, p, _ in self.merged().values():
            if p == path:
                return rel
        return os.path.basename(path)

    def signature(self, folders=LAYERED):
        """Hash of the effective files' paths, sizes and mtimes under `folders`."""
        h = hashlib.sha1()
        pre = tuple(f.lower() + "/" for f in folders)
        for k in sorted(self.merged()):
            if not k.startswith(pre):
                continue
            p = self._merged[k][1]
            try:
                st = os.stat(p)
                h.update(f"{k}|{p}|{st.st_size}|{st.st_mtime}".encode())
            except OSError:
                pass
        return h.hexdigest()[:20]


def effective_files(folder, game_dir, mods, ext=(".txt",), recursive=False):
    """scriptdb.effective_files with the native layer's replace_path rule (see the module doc)."""
    return Content(game_dir, mods).files(folder, ext, recursive)


def default_userdir():
    """HOI4_USERDIR, else the platform's default user folder if it exists, else None."""
    d = os.environ.get("HOI4_USERDIR")
    if d:
        return d
    home = os.path.expanduser("~")
    for p in (
        os.path.join(home, "Documents", "Paradox Interactive", "Hearts of Iron IV"),
        os.path.join(home, ".local", "share", "Paradox Interactive", "Hearts of Iron IV"),
    ):
        if os.path.isdir(p):
            return p
    return None


def integrated_dlcs(game_dir):
    """Display names of DLCs shipped inside the base game (always active)."""
    names = []
    base = os.path.join(game_dir, "integrated_dlc")
    if not os.path.isdir(base):
        return names
    for d in sorted(os.listdir(base)):
        folder = os.path.join(base, d)
        if not os.path.isdir(folder):
            continue
        for f in sorted(os.listdir(folder)):
            if f.endswith(".dlc"):
                with open(os.path.join(folder, f), encoding="utf-8-sig", errors="replace") as fh:
                    m = re.search(r'^name\s*=\s*"([^"]+)"', fh.read(), re.M)
                if m:
                    names.append(m.group(1))
    return names
