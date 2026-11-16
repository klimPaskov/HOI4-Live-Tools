"""The game files' numbers for the planner, through the repository's planning tools (scripts/hoi4facts).

The planning tools are found next to the package (a repository checkout or an install, which ships them) or at
HOI4_LIVE_TOOLS_DIR. Without them, or without the game files, `load` returns None and the planner works with flat estimates and the live
reads alone (its gates then say `partial`). Nothing is country specific: costs, years, folders, prerequisites
and focus trees all come from the effective game content (install plus mods).
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
DEFAULT_GAME_DIRS = (
    os.path.expanduser("~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV"),
    os.path.expanduser("~/.local/share/Steam/steamapps/common/Hearts of Iron IV"),
    r"C:\Program Files (x86)\Steam\steamapps\common\Hearts of Iron IV",
)


def tools_dir():
    """The folder that holds hoi4facts/ and validate_plan.py, or None."""
    for d in (os.environ.get("HOI4_LIVE_TOOLS_DIR"), os.path.join(REPO, "scripts")):
        if d and os.path.isfile(os.path.join(d, "hoi4facts", "lint_plan.py")):
            return d
    return None


def game_dir(given=None):
    for d in (given, os.environ.get("HOI4_DIR")) + DEFAULT_GAME_DIRS:
        if d and os.path.isdir(os.path.join(d, "common", "technologies")):
            return d
    return None


class Facts:
    """Costs, tech metadata and focus trees of one game install and mod set."""

    def __init__(self, game, derive, tech_rule):
        self.game, self.derive, self.techrule = game, derive, tech_rule
        self.meta = derive.tech_meta(game)
        self.techs = game.techs()
        self._trees = None
        try:
            self.focus_point_days = float(derive.define(game, "FOCUS_POINT_DAYS", 7))
        except Exception:  # noqa: BLE001 - a define the files do not hold
            self.focus_point_days = 7.0

    def tech_days(self, tech):
        return self.derive.tech_days(self.game, tech) if tech in self.meta else None

    def tech_year(self, tech):
        m = self.meta.get(tech)
        return m["year"] if m else None

    def folder(self, tech):
        return (self.meta.get(tech) or {}).get("folder")

    def startable(self, tech, done):
        """True when every prerequisite of `tech` is in `done` and no exclusive partner is."""
        info = self.techs.get(tech)
        if info is None:
            return False
        if self.techrule.missing(info, done, "all"):
            return False
        return not ((self.meta.get(tech) or {}).get("xor", set()) & set(done))

    def gated(self, tech):
        return bool((self.meta.get(tech) or {}).get("gated"))

    def trees(self):
        if self._trees is None:
            self._trees = self.game.focus_trees()
        return self._trees

    def focus_tree(self, keys):
        """The focus tree that holds most of `keys` (the plan's own focuses), as {focus: row}."""
        best, hits = {}, 0
        for tree in self.trees().values():
            n = sum(1 for k in keys if k in tree)
            if n > hits:
                best, hits = tree, n
        return best

    def focus_days(self, focus, tree=None):
        row = (tree or {}).get(focus)
        if row is None:
            for t in self.trees().values():
                if focus in t:
                    row = t[focus]
                    break
        try:
            return float((row or {}).get("cost")) * self.focus_point_days
        except (TypeError, ValueError):
            return None

    def costs(self):
        from .horizon import Costs
        return Costs(tech_days=self.tech_days, focus_days=self.focus_days, tech_year=self.tech_year)


_CACHE = {}


def load(game=None, mods=()):
    """Facts for the install (and mods), cached per process; None when the tools or the game are missing."""
    tools, root = tools_dir(), game_dir(game)
    if not tools or not root:
        return None
    key = (root, tuple(mods))
    if key not in _CACHE:
        if tools not in sys.path:
            sys.path.insert(0, tools)
        try:
            from hoi4facts import derive, tech as tech_rule  # noqa: E402
            from hoi4facts.lint_plan import load_game  # noqa: E402
            _CACHE[key] = Facts(load_game(root, None, list(mods)), derive, tech_rule)
        except Exception:  # noqa: BLE001 - unreadable files: the planner falls back to estimates
            _CACHE[key] = None
    return _CACHE[key]
