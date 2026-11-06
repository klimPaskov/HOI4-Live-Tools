"""Localisation sweep tool: hoi4_sweep_loc. Test mode only, because it opens and closes panels with keys."""

import datetime
import json

from ..game import locsweep
from ..perception import screen as screen_mod
from .common import result
from .registry import tool

# panels with a key: the focus tree and diplomacy have none and cannot be swept by hotkey
PANEL_NAMES = [name for name, key in screen_mod.PANEL_KEYS.items() if key]


@tool("hoi4_sweep_loc",
      "Sweep the game for untranslated and broken text (test mode). Visits the bare map and each panel, reads the "
      "screen with OCR and flags [missing_key], raw localisation keys (my_mod.1.t, GER_focus_name), unresolved "
      "variables ($NAME$, [Root.GetName], literal \\n), section-sign colour codes and your own regexes. A screenshot "
      "is kept for every screen with findings. Also lists the localisation warnings that error.log and text.log "
      "gained during the sweep (since='all' adds the ones that were already there). Panels that do not open are "
      "reported and skipped. Ends on the bare map and writes a report (Markdown + JSON) under artifacts/.",
      {"properties": {
          "panels": {"type": "array", "items": {"type": "string", "enum": PANEL_NAMES}, "maxItems": len(PANEL_NAMES),
                     "uniqueItems": True, "default": list(locsweep.DEFAULT_PANELS),
                     "description": "panels to open after the map (default: all 14 with a key, [] sweeps the map only)"},
          "extra_patterns": {"type": "array", "items": {"type": "string", "minLength": 1, "maxLength": 200},
                             "maxItems": 20, "description": "regexes (case-insensitive search) for text you want flagged"},
          "since": {"type": "string", "enum": ["start", "all"], "default": "start",
                    "description": "log warnings since the sweep started, or also those already in the logs"},
          "max_lines_per_screen": {"type": "integer", "minimum": 1, "maximum": 200, "default": 40,
                                   "description": "findings listed per screen (the totals count all of them)"}}},
      family="testmode", lock="acting", test_only=True,
      examples=[{"args": {}},
                {"args": {"panels": ["politics", "decisions"], "extra_patterns": ["^TODO", "lorem ipsum"]}},
                {"args": {"panels": [], "since": "all"}}])
def hoi4_sweep_loc(session, args):
    panels = args["panels"] if args.get("panels") is not None else list(locsweep.DEFAULT_PANELS)
    patterns = locsweep.compile_patterns(args.get("extra_patterns"))
    report = locsweep.sweep(session, panels, patterns, args.get("since", "start"), args.get("max_lines_per_screen", 40))
    stamp = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
    base = session.artifact_path(f"loc_sweep_{stamp}")
    with open(base + ".json", "w", encoding="utf-8", newline="\n") as f:
        json.dump(report, f, indent=1, default=str)
    with open(base + ".md", "w", encoding="utf-8", newline="\n") as f:
        f.write(locsweep.markdown(report))
    shots = [s["screenshot"] for s in report["screens"] if s.get("screenshot")]
    return result(locsweep.summary(report), report=base + ".md", json=base + ".json", screenshots=shots,
                  totals=report["totals"], screens=report["screens"], log_warnings=report["log_warnings"],
                  log_info=report["log_info"], returned_to_map=report["returned_to_map"],
                  **({"before_sweep": report["before_sweep"]} if "before_sweep" in report else {}))
