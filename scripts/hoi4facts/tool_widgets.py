"""Which plain-game interface element names each typed tool looks up (scripts/data/tool-widgets.json).

The runtime's typed tools find their buttons by the element names of the game's .gui files. A mod that
replaces a panel can drop or rename those elements, and the tool then fails to find its target. This builder
reads the runtime's source (native/src/launch/macos/**.rs): every whole string literal that is also an element
name in the plain game's interface files counts as a lookup, and the module is joined to the tools it serves
(its own descriptors, and the dispatch arms of macos.rs that call into it). `moddiff.affected_tools` compares
the result with a mod set's interface.

    python scripts/hoi4facts/tool_widgets.py build --game GAME_DIR [--out scripts/data/tool-widgets.json]

Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import re
import sys

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from hoi4facts.game import Game  # noqa: E402
from hoi4facts.moddiff import TOOL_WIDGETS, gui_elements  # noqa: E402

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
SOURCE = os.path.join(ROOT, "native", "src", "launch", "macos")
DISPATCH = os.path.join(ROOT, "native", "src", "launch", "macos.rs")
_LIT = re.compile(r'"([a-z][a-z0-9_]{2,63})"')
_DESC = re.compile(r'"name":\s*"(hoi4_[a-z0-9_]+)"')
_CONST = re.compile(r'(?:TOOL|NAME)[A-Z_]*:\s*&str\s*=\s*"(hoi4_[a-z0-9_]+)"')
# A dispatch site: `name == "hoi4_x"` or a match arm `"hoi4_x" =>` / `"hoi4_x" |`.
_SITE = re.compile(r'(?:name\s*==\s*"(hoi4_[a-z0-9_]+)")|(?:"(hoi4_[a-z0-9_]+)"\s*(?:=>|\|))')
_CALL = re.compile(r'\b([a-z_][a-z0-9_]*)::[a-z_]')
# A literal used as a JSON key or an index (`"name":`, `["name"]`, `get("name")`) is not a lookup.
_NOT_LOOKUP = re.compile(r'(?:\[|get\()\s*"([a-z][a-z0-9_]{2,63})"|"([a-z][a-z0-9_]{2,63})"\s*:(?!:)')


def module_of(path, source=SOURCE):
    rel = os.path.relpath(path, source)
    return rel.split(os.sep)[0].replace(".rs", "")


def scan(gui_names, source=SOURCE, dispatch=DISPATCH):
    """{"modules": {module: {"tools": [...], "widgets": [...]}}} from the runtime source."""
    mods = {}
    for base, _dirs, files in os.walk(source):
        for f in files:
            if not f.endswith(".rs") or f.endswith("tests.rs"):
                continue
            path = os.path.join(base, f)
            with open(path, encoding="utf-8", errors="replace") as fh:
                text = fh.read()
            # Unit tests build fake trees with real names; only the code above them counts.
            text = text.split("#[cfg(test)]")[0]
            row = mods.setdefault(module_of(path, source), {"tools": set(), "widgets": set()})
            row["tools"].update(_DESC.findall(text))
            row["tools"].update(_CONST.findall(text))
            lits = {}
            for m in _LIT.finditer(text):
                lits.setdefault(m.group(1), 0)
                lits[m.group(1)] += 1
            for m in _NOT_LOOKUP.finditer(text):
                n = m.group(1) or m.group(2)
                if n in lits:
                    lits[n] -= 1
            row["widgets"].update(n for n, c in lits.items()
                                  if c > 0 and n in gui_names and not n.startswith("hoi4_"))
    try:
        with open(dispatch, encoding="utf-8", errors="replace") as fh:
            text = fh.read()
        sites = [(m.start(), m.group(1) or m.group(2)) for m in _SITE.finditer(text)]
        for i, (at, tool) in enumerate(sites):
            end = sites[i + 1][0] if i + 1 < len(sites) else len(text)
            for module in set(_CALL.findall(text[at:min(end, at + 6000)])):
                if module in mods:
                    mods[module]["tools"].add(tool)
    except OSError:
        pass
    return {"modules": {m: {"tools": sorted(r["tools"]), "widgets": sorted(r["widgets"])}
                        for m, r in sorted(mods.items()) if r["widgets"]}}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("action", choices=("build", "show"))
    ap.add_argument("--game", default=os.environ.get("HOI4_DIR"), help="plain game folder")
    ap.add_argument("--out", default=TOOL_WIDGETS)
    a = ap.parse_args(argv)
    if a.action == "show":
        with open(a.out, encoding="utf-8") as fh:
            print(fh.read())
        return 0
    if not a.game:
        ap.error("--game (or HOI4_DIR) is required")
    data = scan(set(gui_elements(Game(a.game, []))))
    with open(a.out, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=1, sort_keys=True)
        fh.write("\n")
    n = sum(len(r["widgets"]) for r in data["modules"].values())
    print("%d modules, %d element names -> %s" % (len(data["modules"]), n, a.out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
