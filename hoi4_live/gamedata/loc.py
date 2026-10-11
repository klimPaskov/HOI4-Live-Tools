"""Localisation lookup: display names for keys found in saves (countries, states, technologies, focuses, ideas,
equipment, decisions), from the game's and the enabled mods' localisation files.

The English localisation of the game plus a large mod is 25+ MB in hundreds of files, so nothing is loaded up front:
a lookup batch scans each file with bytes.find for the requested keys only (C speed), later files win (mods after
the game, `replace` folders last), and results are cached in memory and on disk (keyed by the file set's mtimes).
"""

import hashlib
import json
import os
import re
import tempfile

from .. import platform

LINE = re.compile(rb'^[ \t\xef\xbb\xbf]*([A-Za-z0-9_.\-]+):\d*[ \t]*"(.*)"', re.M)
REF = re.compile(r"\$([A-Za-z0-9_.]+)\$")
FORMAT = re.compile(r"§[A-Za-z0-9!]|£[A-Za-z0-9_|]+£?|\$[A-Za-z0-9_|.]+\$")


def clean_text(text):
    """Drop colour codes, icons and $VARIABLE$ references; keep the readable words."""
    t = FORMAT.sub("", text.replace("\\n", " ").replace('\\"', '"'))
    return re.sub(r"\s+", " ", t).strip()


class Localisation:
    def __init__(self, game_dir, mod_dirs=(), language="english", cache_dir=None):
        self.game_dir = game_dir
        self.mod_dirs = list(mod_dirs)
        self.language = language
        self._files = None
        self._mem = {}
        self._missing = set()
        self.cache_dir = cache_dir or os.path.join(platform.current().local_data_dir(),
                                                   "hoi4_live", "loc_cache")
        self._disk_loaded = False

    # ------------------------------------------------------------------ files
    def files(self):
        if self._files is not None:
            return self._files
        suffix = f"_l_{self.language}.yml"
        normal, replace = [], []
        for root in [self.game_dir] + self.mod_dirs:
            if not root:
                continue
            base = os.path.join(root, "localisation")
            for dirpath, _, names in os.walk(base):
                for n in sorted(names):
                    if n.lower().endswith(suffix):
                        p = os.path.join(dirpath, n)
                        (replace if os.sep + "replace" in dirpath.lower() else normal).append(p)
        self._files = normal + replace
        return self._files

    def _signature(self):
        h = hashlib.sha1()
        for p in self.files():
            try:
                st = os.stat(p)
                h.update(f"{p}|{st.st_size}|{st.st_mtime}".encode())
            except OSError:
                pass
        return h.hexdigest()[:20]

    def _cache_file(self):
        os.makedirs(self.cache_dir, exist_ok=True)
        return os.path.join(self.cache_dir, f"{self.language}_v2_{self._signature()}.json")   # v2: $key$ resolved

    def _load_disk(self):
        if self._disk_loaded:
            return
        self._disk_loaded = True
        try:
            with open(self._cache_file(), encoding="utf-8") as f:
                data = json.load(f)
            self._mem.update(data.get("found", {}))
            self._missing.update(data.get("missing", []))
        except (OSError, ValueError):
            pass

    def _save_disk(self):
        try:
            p = self._cache_file()
            with open(p + ".tmp", "w", encoding="utf-8") as f:
                json.dump({"found": self._mem, "missing": sorted(self._missing)}, f, ensure_ascii=False)
            os.replace(p + ".tmp", p)
        except OSError:
            pass

    # ------------------------------------------------------------------ lookup
    def get_many(self, keys, depth=0):
        """{key: text} for the keys that exist (cleaned of formatting; $key$ references resolved)."""
        keys = [k for k in dict.fromkeys(keys) if k]
        self._load_disk()
        todo = [k for k in keys if k not in self._mem and k not in self._missing]
        if todo:
            found = {}
            needles = {k: k.encode("utf-8") + b":" for k in todo}
            wanted = set(todo)
            for p in self.files():
                try:
                    with open(p, "rb") as f:
                        data = f.read()
                except OSError:
                    continue
                if len(needles) > 150:
                    # big batches: one pass over the file's lines instead of one search per key
                    for m in LINE.finditer(data):
                        k = m.group(1).decode("utf-8", "replace")
                        if k in wanted:
                            found[k] = m.group(2).decode("utf-8", "replace")
                    continue
                for k, needle in needles.items():
                    v = _find_value(data, needle)
                    if v is not None:
                        found[k] = v
            # values that point at other keys ($harold_ickes$ in USA_harold_ickes): resolve one level
            refs = {k: REF.findall(v) for k, v in found.items() if "$" in v}
            wanted_refs = sorted({r for rs in refs.values() for r in rs if r not in found})
            resolved = self.get_many(wanted_refs, depth + 1) if wanted_refs and depth < 2 else {}
            for k in todo:
                if k in found:
                    v = found[k]
                    for r in refs.get(k, ()):
                        sub = clean_text(found[r]) if r in found else resolved.get(r)
                        if sub is not None:
                            v = v.replace(f"${r}$", sub)
                    self._mem[k] = clean_text(v)
                else:
                    self._missing.add(k)
            self._save_disk()
        return {k: self._mem[k] for k in keys if k in self._mem}

    def get(self, key, default=None):
        return self.get_many([key]).get(key, default)


def _find_value(data, needle):
    """Last definition of `needle` (b'KEY:') in a localisation file: the key must start a line (after spaces)."""
    best = None
    start = 0
    while True:
        i = data.find(needle, start)
        if i < 0:
            break
        start = i + 1
        j = i - 1
        while j >= 0 and data[j] in (32, 9):
            j -= 1
        if j >= 0 and data[j] not in (10, 13, 0xBF):
            continue
        end = data.find(b"\n", i)
        line = data[i + len(needle):end if end >= 0 else len(data)].decode("utf-8", "replace")
        m = re.match(r'\s*\d*\s*"(.*)"', line)
        if m:
            best = m.group(1)
    return best


_INSTANCES = {}


def for_session(session):
    """One Localisation per (game, playset) for the server process."""
    from ..telemetry.modgen import mod_paths
    cfg = session.cfg
    mods = [m["path"] for m in mod_paths(cfg.user_dir)]
    key = (cfg.game_dir, tuple(mods))
    if key not in _INSTANCES:
        _INSTANCES.clear()
        _INSTANCES[key] = Localisation(cfg.game_dir, mods)
    return _INSTANCES[key]
