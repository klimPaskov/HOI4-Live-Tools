"""Interface profile: the month names and the on-screen labels the OCR tools anchor on (main menu, pause menu,
pre-game screens, panel titles), for the vanilla game and for overhaul / total-conversion mods.

Every label comes from three layers; a later layer adds to the earlier ones (months are replaced, not added):

1. vanilla English: the built-in regexes of perception/screen.py and game/menu.py, unchanged;
2. derived from the game's and the enabled mods' own files, the way the game resolves them:
   - the element that carries a vanilla label is found by its `name` in the vanilla `interface/*.gui`; when an
     enabled mod defines an element of that name, the mod's `text` / `buttonText` key is used instead;
   - the key is resolved through localisation (game first, the mods in load order after it, `replace/` folders
     last; the language comes from settings.txt, `english` by default);
   - month names are the localisation keys `Jan`..`Dec` (the top-bar date) and `January`..`December` (the save
     list), so a mod that redefines those keys renames the months;
   - a mod that replaces the main menu without the vanilla button names gives away its own buttons: every button
     inside the vanilla main-menu containers it redefines becomes a main-menu anchor;
3. a profile (JSON, documented in docs/LEGACY_WINDOWS_USAGE.md): `hoi4_live_profile.json` in the root folder of
   an enabled mod (load order: a later mod overrides an earlier one), then the run config (`ui_profile` in
   config.json, inline or a path, or the HOI4_LIVE_UI_PROFILE environment variable). It covers what derivation
   cannot, such as a menu drawn as images.

When the result equals vanilla, every caller keeps its vanilla regex exactly: `pattern()` returns the regex it
was given, `month_table()` is None and the date parser takes its vanilla path.
"""

import json
import os
import re
import threading

FORMAT = 1
PROFILE_FILE = "hoi4_live_profile.json"
OFF_VALUES = ("off", "vanilla")          # ui_profile / HOI4_LIVE_UI_PROFILE: vanilla anchors only, nothing derived

MONTH_SHORT_KEYS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
MONTH_LONG_KEYS = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
                   "November", "December"]

# section -> name -> (vanilla .gui file under interface/, vanilla localisation key, vanilla English text)
ANCHORS = {
    "main_menu": {
        "single_player": ("frontendmainview.gui", "FE_SINGLE_PLAYER", "Single Player"),
        "multiplayer": ("frontendmainview.gui", "FE_MULTI_PLAYER", "Multiplayer"),
        "credits": ("frontendmainview.gui", "FE_CREDITS", "Credits"),
        "quit": ("frontendmainview.gui", "FE_EXIT", "Quit"),
        "continue": ("frontendmainview.gui", "FE_CONTINUE", "Continue"),
        "new_game": ("frontendmainview.gui", "FE_NEW", "New Game"),
        "load": ("frontendmainview.gui", "MENU_BAR_LOAD", "Load"),
        "tutorial": ("frontendmainview.gui", "FE_TUTORIAL", "Tutorial"),
    },
    "pause_menu": {
        "load_game": ("ingamemenu.gui", "MENU_BAR_LOAD_GAME", "Load Game"),
        "save_game": ("ingamemenu.gui", "MENU_BAR_SAVE_GAME", "Save Game"),
        "options": ("ingamemenu.gui", "MENU_BAR_GAME_OPTIONS", "Game Options"),
        "exit_to_menu": ("ingamemenu.gui", "MENU_BAR_GAME_RESIGN", "Exit to Menu"),
        "exit_game": ("ingamemenu.gui", "MENU_BAR_QUIT", "Exit Game"),
    },
    "setup": {
        "select_scenario": ("frontendgamesetupview.gui", "FE_SELECT_SCENARIO", "Select Scenario"),
        "select_country": ("frontendgamesetupview.gui", "FE_SELECT_COUNTRY", "Select Country"),
        "start": ("frontendgamesetupview.gui", "FE_START", "START"),
        "back_to_menu": ("frontendgamesetupview.gui", "MAIN_MENU_BACK", "Back to Menu"),
    },
    "panels": {
        "politics": ("countrypoliticsview.gui", "POLITICAL_POLITICAL", "Political"),
        "decisions": ("countrydecisionview.gui", "DECISIONVIEW_TITLE", "Events and Decisions"),
        "research": ("countrytechnologyview.gui", "RESEARCH_RESEARCH", "Research"),
        "trade": ("countrytradeview.gui", "TRADE_TRADE", "Trade"),
        "construction": ("countryconstructionsview.gui", "CONSTRUCTION_VIEW_LABLE", "Construction"),
        "production": ("countryproductionlineview.gui", "PRODUCTION_PRODUCTION_LABEL", "Production"),
        "deployment": ("countrydeploymentview.gui", "DEPLOYMENT_DEPLOYMENT_LABEL", "Recruit & Deploy"),
        "logistics": ("countrylogisticsview.gui", "LOGISTICS_LOGISTICS_LABEL", "Logistics"),
        "diplomacy": ("countrydiplomacyview.gui", "DIPLOMACY_DIPLOMACY", "Diplomacy"),
        "faction": ("factions/countryfactionview.gui", "FACTION_TITLE", "Faction"),
        "army_overview": ("countryarmyview.gui", "ARMY_ARMY_LABEL", "Division Overview"),
        "officer_corps": ("countryofficercorpview.gui", "OFFICER_CORPS_LABEL", "Officer Corps"),
    },
}
# every panel perception/screen.py knows (a profile may name any of them, also those with nothing to derive)
PANEL_NAMES = ["focus_tree", "politics", "decisions", "intelligence", "research", "trade", "construction",
               "production", "deployment", "logistics", "officer_corps", "faction", "army_overview", "navy_overview",
               "air_overview", "diplomacy"]
SECTIONS = ("main_menu", "pause_menu", "setup", "panels")
MAIN_MENU_TOP = ("single_player", "multiplayer", "credits", "quit")     # the front page, not the single-player submenu
PROFILE_KEYS = {"format", "description", "language", "months", "main_menu", "pause_menu", "setup", "panels",
                "confirm"}


class ProfileError(ValueError):
    pass


# ------------------------------------------------------------------------------------------------ labels
def label_rx(label):
    """A literal label as a regex: case is ignored by the callers, spaces may be lost by OCR."""
    words = [w for w in re.split(r"\s+", label.strip()) if w]
    return r"\s*".join(re.escape(w) for w in words)


def _norm(text):
    return re.sub(r"\s+", " ", (text or "").strip().lower()).strip(" .,")


def _clean_label(text):
    from ..gamedata.loc import clean_text
    t = clean_text(text or "")
    return t if len(t) >= 2 and re.search(r"[^\W\d_]", t) else None


class Profile:
    """The resolved profile. `labels[section][name]` lists every label known for an anchor (vanilla text first),
    `months[kind]` twelve alias lists, `recognition` extra main-menu anchors, `sources` what each part came from."""

    def __init__(self):
        self.months = {"short": [[m] for m in MONTH_SHORT_KEYS], "long": [[m] for m in MONTH_LONG_KEYS]}
        self.labels = {s: {n: [v[2]] for n, v in ANCHORS.get(s, {}).items()} for s in SECTIONS}
        for n in PANEL_NAMES:
            self.labels["panels"].setdefault(n, [])
        self.recognition = []               # main-menu anchors with no vanilla counterpart
        self.confirm = []                   # extra labels of a dialog's confirming button
        self.language = "english"
        self.mods = []
        self.profile_files = []
        self.sources = {}                   # "months.short" / "main_menu.single_player" -> "derived: ..." / "profile: ..."
        self.errors = []
        self.warnings = []
        self._cache = {}

    # ------------------------------------------------------------------ building
    def add_label(self, section, name, label, source):
        label = (label or "").strip()
        if not label:
            return
        cur = self.labels[section].setdefault(name, [])
        if not any(_norm(label) == _norm(x) for x in cur):
            cur.append(label)
            self.note_source(f"{section}.{name}", source)
            self._cache.clear()

    def note_source(self, key, source):
        cur = self.sources.get(key)
        if not cur:
            self.sources[key] = source
        elif source not in cur.split(" + "):
            self.sources[key] = cur + " + " + source

    def set_months(self, kind, names, source):
        self.months[kind] = [list(n) for n in names]
        self.sources[f"months.{kind}"] = source
        self._cache.clear()

    # ------------------------------------------------------------------ queries
    @property
    def is_vanilla(self):
        return (self.month_table() is None and not self.recognition and not self.confirm and
                not any(self.extra(s, n) for s in SECTIONS for n in self.labels[s]))

    def extra(self, section, name, vanilla_rx=None):
        """Labels of an anchor beyond its vanilla text (and beyond what vanilla_rx already matches)."""
        labels = self.labels.get(section, {}).get(name, [])
        default = ANCHORS.get(section, {}).get(name, (None, None, None))[2]
        out = []
        for lab in labels:
            if default and _norm(lab) == _norm(default):
                continue
            if vanilla_rx and re.search(vanilla_rx, lab, re.I):
                continue
            out.append(lab)
        return out

    def pattern(self, section, name, vanilla_rx, anchored=False):
        """vanilla_rx itself when nothing differs, otherwise vanilla_rx or any extra label. anchored=True makes
        each extra label a whole-line match (for callers whose vanilla regex is anchored itself)."""
        extra = self.extra(section, name, vanilla_rx)
        if not extra:
            return vanilla_rx
        alts = [(r"^\W*" + label_rx(x) + r"\W*$") if anchored else label_rx(x) for x in extra]
        return "(?:(?:" + vanilla_rx + ")|" + "|".join(alts) + ")"

    def section_extras(self, section):
        out = []
        for name in self.labels.get(section, {}):
            out += [x for x in self.extra(section, name) if x not in out]
        return out

    def section_rx(self, base, section):
        """The compiled regex `base`, or `base` or any extra label of the section (no extras: `base` itself)."""
        key = ("section", base.pattern, section)
        if key not in self._cache:
            extra = [x for x in self.section_extras(section) if not base.search(x)]
            self._cache[key] = base if not extra else re.compile(
                "(?:" + base.pattern + ")|" + "|".join(label_rx(x) for x in extra), base.flags)
        return self._cache[key]

    def main_menu_anchors(self):
        """Labels that identify a mod's main menu (none for vanilla: its own regex decides there): the extra labels
        of the top-level buttons and the recognition anchors. The single-player submenu's buttons do not count."""
        out = []
        for name in MAIN_MENU_TOP:
            out += [x for x in self.extra("main_menu", name) if x not in out]
        return out + [x for x in self.recognition if x not in out]

    def main_menu_hits(self, texts):
        """(recognised, hits) by the non-vanilla anchors: two hits, or one when only one anchor is known."""
        anchors = self.main_menu_anchors()
        if not anchors:
            return False, []
        rxs = [(a, re.compile(r"^\W*" + label_rx(a) + r"\W*$", re.I)) for a in anchors]
        hits = [a for a, rx in rxs if any(rx.search(t) for t in texts)]
        return len(hits) >= min(2, len(anchors)), hits

    def month_table(self):
        """{normalised name: month} over the short and long names, or None when the months are vanilla."""
        if "months" in self._cache:
            return self._cache["months"]
        vanilla = all([_norm(x) for x in self.months["short"][i]] == [_norm(MONTH_SHORT_KEYS[i])] and
                      [_norm(x) for x in self.months["long"][i]] == [_norm(MONTH_LONG_KEYS[i])] for i in range(12))
        table = None
        if not vanilla:
            table = {}
            for kind in ("short", "long"):
                for i, names in enumerate(self.months[kind]):
                    for n in names:
                        table.setdefault(_norm(n), i + 1)
        self._cache["months"] = table
        return table

    def month_of(self, token):
        """Month number (1-12) of a top-bar token, or None. Exact first, then a unique prefix of 3+ letters either
        way (OCR adds or drops a trailing glyph)."""
        table = self.month_table()
        t = _norm(token)
        if table is None:
            return {m.lower(): i + 1 for i, m in enumerate(MONTH_SHORT_KEYS)}.get(t[:3])
        if t in table:
            return table[t]
        if len(t) >= 3:
            cands = {m for n, m in table.items() if len(n) >= 3 and (n.startswith(t) or t.startswith(n))}
            if len(cands) == 1:
                return cands.pop()
        return None

    def month_names(self, kind="short"):
        return [names[0] for names in self.months[kind]]

    def long_month_alts(self, month):
        return self.months["long"][month - 1]

    def panel_patterns(self, panels):
        """perception.screen.PANELS with each panel's extra titles added as whole-line matches."""
        key = ("panels", id(panels))
        if key in self._cache:
            return self._cache[key]
        out, known = [], set()
        for name, rx in panels:
            known.add(name)
            extra = self.extra("panels", name, rx.pattern)
            if extra:
                rx = re.compile("(?:" + rx.pattern + ")|" + "|".join(r"^\W*" + label_rx(x) + r"\W*$" for x in extra),
                                re.I)
            out.append((name, rx))
        self._cache[key] = out
        return out

    def summary(self):
        out = {"vanilla": self.is_vanilla, "language": self.language, "mods": list(self.mods),
               "profile_files": list(self.profile_files)}
        if self.month_table() is not None:
            out["months"] = {"short": self.month_names("short"), "long": self.month_names("long")}
        diff = {}
        for s in SECTIONS:
            d = {n: self.extra(s, n) for n in self.labels[s] if self.extra(s, n)}
            if d:
                diff[s] = d
        if diff:
            out["labels"] = diff
        if self.recognition:
            out["main_menu_anchors"] = list(self.recognition)
        if self.confirm:
            out["confirm"] = list(self.confirm)
        if self.sources:
            out["sources"] = dict(self.sources)
        if self.errors:
            out["errors"] = list(self.errors)
        if self.warnings:
            out["warnings"] = list(self.warnings)
        return out


VANILLA = Profile()

_LOCK = threading.Lock()
_ACTIVE = VANILLA
_KEY = None
_WATCHED = []


def active():
    return _ACTIVE


def activate(profile):
    global _ACTIVE
    _ACTIVE = profile or VANILLA
    return _ACTIVE


def reset():
    global _KEY
    _KEY = None
    return activate(VANILLA)


def pattern(section, name, vanilla_rx, anchored=False):
    return _ACTIVE.pattern(section, name, vanilla_rx, anchored)


def summary():
    return _ACTIVE.summary()


# ------------------------------------------------------------------------------------------------ .gui files
_TOK = re.compile(r'"([^"]*)"|([{}=])|#[^\n]*|([^\s{}="#]+)')


def parse_gui(text):
    """Blocks of a .gui file as a flat list: {"kind", "name", "text", "parent" (index or None)}. Only what the
    profile needs: names and the text / buttonText of each block."""
    toks = []
    for m in _TOK.finditer(text):
        if m.group(1) is not None:
            toks.append(("q", m.group(1)))
        elif m.group(2) is not None:
            toks.append(("p", m.group(2)))
        elif m.group(3) is not None:
            toks.append(("w", m.group(3)))
    blocks, stack = [], [None]
    i, n = 0, len(toks)
    while i < n:
        kind, v = toks[i]
        if kind == "p" and v == "}":
            if len(stack) > 1:
                stack.pop()
            i += 1
            continue
        if kind == "p" and v == "{":
            blocks.append({"kind": "", "name": None, "text": None, "parent": stack[-1]})
            stack.append(len(blocks) - 1)
            i += 1
            continue
        if i + 2 < n and toks[i + 1] == ("p", "="):
            vk, vv = toks[i + 2]
            if vk == "p" and vv == "{":
                blocks.append({"kind": v.lower(), "name": None, "text": None, "parent": stack[-1]})
                stack.append(len(blocks) - 1)
            elif stack[-1] is not None:
                key = v.lower()
                cur = blocks[stack[-1]]
                if key == "name":
                    cur["name"] = vv
                elif key in ("text", "buttontext"):
                    cur["text"] = vv
            i += 3
            continue
        i += 1
    return blocks


def _read_text(path):
    try:
        with open(path, "rb") as f:
            data = f.read()
    except OSError:
        return None
    if data.startswith(b"\xef\xbb\xbf"):
        data = data[3:]
    return data.decode("utf-8", "replace")


def _ancestors(blocks, i):
    out = []
    p = blocks[i]["parent"]
    while p is not None:
        if blocks[p]["name"]:
            out.append(blocks[p]["name"])
        p = blocks[p]["parent"]
    return out


def vanilla_elements(game_dir, gui_file, key):
    """[(element name, [named ancestors])] of the vanilla elements that show `key`."""
    if not game_dir:
        return []
    text = _read_text(os.path.join(game_dir, "interface", *gui_file.split("/")))
    if text is None:
        return []
    blocks = parse_gui(text)
    return [(b["name"], _ancestors(blocks, i)) for i, b in enumerate(blocks) if b["text"] == key and b["name"]]


def mod_gui_index(mods):
    """{(outermost named container or None, element name): {"text", "buttons": [descendant button texts], "mod"}}
    over the mods' interface/*.gui files, a later mod (and a later file) overriding an earlier one. Names like
    `title` repeat across windows, so an element is matched within its outermost window."""
    index = {}
    for mod in mods:
        base = os.path.join(mod["path"], "interface")
        if not os.path.isdir(base):
            continue
        for dirpath, dirnames, names in os.walk(base):
            dirnames.sort()
            for fn in sorted(names):
                if not fn.lower().endswith(".gui"):
                    continue
                text = _read_text(os.path.join(dirpath, fn))
                if text is None:
                    continue
                blocks = parse_gui(text)
                kids = {}
                for i, b in enumerate(blocks):
                    if "button" in b["kind"] and b["text"]:
                        p = b["parent"]
                        while p is not None:
                            kids.setdefault(p, []).append(b["text"])
                            p = blocks[p]["parent"]
                for i, b in enumerate(blocks):
                    if b["name"]:
                        anc = _ancestors(blocks, i)
                        index[(anc[-1] if anc else None, b["name"])] = {
                            "text": b["text"], "buttons": kids.get(i, []), "mod": os.path.basename(mod["path"])}
    return index


# ------------------------------------------------------------------------------------------------ profile files
def _labels_value(v, where, errors):
    if isinstance(v, str):
        v = [v]
    if not isinstance(v, list) or not v or not all(isinstance(x, str) and x.strip() for x in v):
        errors.append(f"{where} must be a label or a non-empty list of labels")
        return None
    return [x.strip() for x in v]


def validate(data, source="profile"):
    """A clean copy of a profile dict, or ProfileError naming every problem."""
    errors = []
    if not isinstance(data, dict):
        raise ProfileError(f"{source}: a profile is a JSON object")
    out = {}
    for k in data:
        if k not in PROFILE_KEYS:
            errors.append(f"unknown key '{k}' (known: {', '.join(sorted(PROFILE_KEYS))})")
    if "format" in data and data["format"] != FORMAT:
        errors.append(f"format must be {FORMAT}")
    if "language" in data:
        if not isinstance(data["language"], str) or not re.fullmatch(r"(l_)?[a-z_]+", data["language"]):
            errors.append("language must be a localisation language such as 'english'")
        else:
            out["language"] = data["language"][2:] if data["language"].startswith("l_") else data["language"]
    if "months" in data:
        mo = data["months"]
        if not isinstance(mo, dict) or not mo or set(mo) - {"short", "long"}:
            errors.append("months must be an object with 'short' and/or 'long'")
        else:
            out["months"] = {}
            for kind, names in mo.items():
                if not isinstance(names, list) or len(names) != 12:
                    got = len(names) if isinstance(names, list) else type(names).__name__
                    errors.append(f"months.{kind} must list 12 month names, January first (got {got})")
                    continue
                vals = [_labels_value(n, f"months.{kind}[{i}]", errors) for i, n in enumerate(names)]
                if all(vals):
                    out["months"][kind] = vals
    for section in SECTIONS:
        if section not in data:
            continue
        sec = data[section]
        if not isinstance(sec, dict):
            errors.append(f"{section} must be an object of name: label(s)")
            continue
        known = PANEL_NAMES if section == "panels" else list(ANCHORS[section]) + (
            ["anchors"] if section == "main_menu" else [])
        out[section] = {}
        for name, v in sec.items():
            if name not in known:
                errors.append(f"{section}.{name} is not a known name (known: {', '.join(known)})")
                continue
            vals = _labels_value(v, f"{section}.{name}", errors)
            if vals:
                out[section][name] = vals
    if "confirm" in data:
        vals = _labels_value(data["confirm"], "confirm", errors)
        if vals:
            out["confirm"] = vals
    if errors:
        raise ProfileError(f"{source}: " + "; ".join(errors))
    return out


def load_file(path):
    """Read and validate one profile file. ProfileError with the file name on any problem."""
    try:
        with open(path, encoding="utf-8-sig") as f:
            data = json.load(f)
    except OSError as e:
        raise ProfileError(f"{path}: cannot be read ({e})")
    except ValueError as e:
        raise ProfileError(f"{path}: not valid JSON ({e})")
    return validate(data, path)


def apply_profile(profile, data, source):
    """Layer a validated profile dict onto `profile`."""
    for kind, names in data.get("months", {}).items():
        profile.set_months(kind, names, source)
    for section in SECTIONS:
        for name, labels in data.get(section, {}).items():
            if section == "main_menu" and name == "anchors":
                for lab in labels:
                    if lab not in profile.recognition:
                        profile.recognition.append(lab)
                profile.note_source("main_menu.anchors", source)
                continue
            for lab in labels:
                profile.add_label(section, name, lab, source)
    for lab in data.get("confirm", []):
        if lab not in profile.confirm:
            profile.confirm.append(lab)
    profile._cache.clear()


# ------------------------------------------------------------------------------------------------ building
def settings_language(user_dir):
    """The game's language from settings.txt (language="l_english"), 'english' when unknown."""
    text = _read_text(os.path.join(user_dir, "settings.txt")) if user_dir else None
    m = re.search(r'^\s*language\s*=\s*"?l_([a-z_]+)"?', text or "", re.M)
    return m.group(1) if m else "english"


def build(game_dir, mods=(), language="english", config_profile=None, config_source="config", loc=None,
          cache_dir=None):
    """Resolve the profile for a game folder and the enabled mods ([{"path": ...}] in load order).

    config_profile: a validated dict (from config.json or HOI4_LIVE_UI_PROFILE), applied last. Profile files that
    fail validation are recorded in `errors` and skipped; the rest still applies."""
    p = Profile()
    mods = list(mods)
    p.mods = [os.path.basename(m["path"]) for m in mods]
    mod_profiles = []
    for m in mods:
        f = os.path.join(m["path"], PROFILE_FILE)
        if os.path.isfile(f):
            p.profile_files.append(f)
            try:
                mod_profiles.append((f, load_file(f)))
            except ProfileError as e:
                p.errors.append(str(e))
    asked = language
    for _, data in mod_profiles:
        language = data.get("language", language)
    if config_profile and config_profile.get("language"):
        language = config_profile["language"]
    p.language = language
    if language != asked:
        loc = None                                   # a profile chose another language than the given index
    if mods or language != "english":
        _derive(p, game_dir, mods, language, loc, cache_dir)
    for f, data in mod_profiles:
        apply_profile(p, data, f"profile: {f}")
    if config_profile:
        apply_profile(p, config_profile, f"profile: {config_source}")
    return p


def _derive(p, game_dir, mods, language, loc=None, cache_dir=None):
    index = mod_gui_index(mods)
    keys = {}                                         # (section, name) -> (key, how)
    containers = set()
    replaced_menu = False
    for section, names in ANCHORS.items():
        for name, (gui_file, vkey, _) in names.items():
            elems = vanilla_elements(game_dir, gui_file, vkey)
            key, how = vkey, None
            for ename, anc in elems:
                if section == "main_menu" and anc:
                    containers.add(anc[-1])
                hit = index.get((anc[-1] if anc else None, ename))
                if hit is not None:
                    if hit["text"]:
                        key, how = hit["text"], f"derived: {hit['mod']} interface element '{ename}'"
                    else:
                        key, how = None, None
                        p.warnings.append(f"{section}.{name}: the mod element '{ename}' has no text (drawn as an "
                                          "image?); add its label to the mod profile")
                    break
            else:
                if section == "main_menu" and name == "single_player" and elems and index:
                    replaced_menu = True
            keys[(section, name)] = (key, how)
    # a main menu rebuilt without the vanilla button names: its own buttons in the vanilla containers
    extra_keys = []
    if replaced_menu or any(k is None for (s, _), (k, _) in keys.items() if s == "main_menu"):
        for c in sorted(containers):
            hit = index.get((None, c))
            if hit:
                extra_keys += [k for k in hit["buttons"] if k not in extra_keys]
    if loc is None:
        from ..gamedata.loc import Localisation
        loc = Localisation(game_dir, [m["path"] for m in mods], language, cache_dir=cache_dir)
    wanted = MONTH_SHORT_KEYS + MONTH_LONG_KEYS + [k for k, _ in keys.values() if k] + extra_keys
    values = loc.get_many(wanted)
    for kind, mkeys in (("short", MONTH_SHORT_KEYS), ("long", MONTH_LONG_KEYS)):
        names = []
        for k in mkeys:
            v = _clean_label(values.get(k)) if values.get(k) else None
            names.append([v or k])
        if [n[0] for n in names] != mkeys:
            p.set_months(kind, names, f"derived: localisation keys {mkeys[0]}..{mkeys[-1]} ({language})")
    for (section, name), (key, how) in keys.items():
        if not key:
            continue
        v = values.get(key)
        lab = _clean_label(v) if v else None
        if lab is None:
            if how:
                p.warnings.append(f"{section}.{name}: localisation key '{key}' not found")
            continue
        p.add_label(section, name, lab, how or f"derived: localisation key {key} ({language})")
    known = {_norm(x) for labs in p.labels["main_menu"].values() for x in labs}
    for k in extra_keys:
        lab = _clean_label(values.get(k)) if values.get(k) else None
        if lab and _norm(lab) not in known and lab not in p.recognition:
            p.recognition.append(lab)
            p.note_source("main_menu.anchors", "derived: buttons of the mod's main menu")


# ------------------------------------------------------------------------------------------------ session
def _mtime(path):
    try:
        return os.stat(path).st_mtime_ns
    except (OSError, TypeError, ValueError):
        return None


def _config_spec(cfg):
    """(validated dict or None, source label, file path or None). ProfileError when it is invalid."""
    env = os.environ.get("HOI4_LIVE_UI_PROFILE")
    data = getattr(cfg, "data", None)
    spec = env or (data.get("ui_profile") if isinstance(data, dict) else None)
    if not spec:
        return None, None, None
    if isinstance(spec, dict):
        return validate(spec, "config.json ui_profile"), "config.json ui_profile", None
    if not isinstance(spec, str):
        raise ProfileError("config.json ui_profile must be an object or the path of a JSON file")
    path = spec
    if not os.path.isabs(path):
        src = getattr(cfg, "source", None)
        from .. import config as config_mod
        base = os.path.dirname(src) if isinstance(src, str) and src else config_mod.ROOT
        path = os.path.join(base, path)
    return load_file(path), path, path


def ensure(session):
    """Resolve and activate the profile for the session's game and playset; cheap when nothing changed (a few
    file stats). Returns the active profile; its `errors` list problems (an invalid profile is never applied)."""
    global _KEY
    cfg = session.cfg
    game_dir = getattr(cfg, "game_dir", None)
    user_dir = getattr(cfg, "user_dir", None)
    game_dir = game_dir if isinstance(game_dir, str) else None
    user_dir = user_dir if isinstance(user_dir, str) else None
    data = getattr(cfg, "data", None)
    raw = os.environ.get("HOI4_LIVE_UI_PROFILE") or (data.get("ui_profile") if isinstance(data, dict) else None)
    raw_key = json.dumps(raw, sort_keys=True, default=str)
    files = list(_WATCHED) if _KEY is not None else []
    key = (game_dir, user_dir, raw_key,
           _mtime(os.path.join(user_dir, "dlc_load.json")) if user_dir else None,
           _mtime(os.path.join(user_dir, "settings.txt")) if user_dir else None,
           _mtime(raw) if isinstance(raw, str) else None,
           tuple(_mtime(f) for f in files))
    with _LOCK:
        if key == _KEY:
            return _ACTIVE
        if isinstance(raw, str) and raw.strip().lower() in OFF_VALUES:
            _WATCHED[:] = []
            _KEY = key
            return activate(VANILLA)
        mods = []
        if user_dir:
            from ..telemetry.modgen import mod_paths
            mods = mod_paths(user_dir)
        errors = []
        try:
            spec, source, _ = _config_spec(cfg)
        except ProfileError as e:
            spec, source = None, None
            errors.append(str(e))
        language = settings_language(user_dir)
        loc = None
        if language == "english" and not (spec or {}).get("language") and mods:
            try:                                    # share the server's localisation index and its disk cache
                from ..gamedata.loc import for_session
                loc = for_session(session)
            except Exception:
                loc = None
        p = build(game_dir, mods, language, spec, source or "config", loc=loc)
        p.errors = errors + p.errors
        activate(p)
        # a profile file added to (or changed in) a mod folder is picked up by the mod folders' and files' mtimes
        _WATCHED[:] = [m["path"] for m in mods] + [os.path.join(m["path"], PROFILE_FILE) for m in mods]
        files = list(_WATCHED)
        _KEY = key[:-1] + (tuple(_mtime(f) for f in files),)
        return p
