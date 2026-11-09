#!/usr/bin/env python3
"""Validate a HOI4 Live Tools standing plan against the installed game files.

Usage:
    validate_plan.py PLAN.json [--game-dir DIR] [--userdir DIR] [--mod PATH ...] [--strict]

Game content is the install plus the enabled mods of the user folder (--userdir, else
HOI4_USERDIR; dlc_load.json enabled_mods and their .mod descriptors, replace_path applied as the
native layer does) plus any --mod (a .mod descriptor or a mod folder), in that order. Without a
user folder or --mod only the install is read. The library is scripts/hoi4facts.

Checks (errors unless marked warning):
  * research_priority: every key is a technology, no duplicates, prerequisites are
    researched earlier in the list or at the start (warning: the game simply refuses
    a locked key and the runner moves on, but a long locked head wastes days);
    techs more than one year ahead of the plan's stop date are flagged (warning).
  * focus_path / focus_fallback: every key is a focus of one tree, no duplicates,
    each prerequisite block has a member earlier in the path, and no two mutually
    exclusive focuses are both listed (the second can never start).
  * production / production_priority: equipment keys exist (production_priority
    entries are prefixes).
  * construction: building keys exist, states are known ids or exact state names,
    no duplicates inside a list.
  * politics: category/slot shape and that the idea is a known idea (laws, designers
    from common/ideas) or a known character id / idea_token (advisors).
  * decisions: every key exists in common/decisions.
  * event options: shape only.
  * shapes: types of research_path, construction_plan, vp_targets, army_layout, war,
    doctrines and event_rules entries (what native/src/plan_run.rs reads).
  * research_path: years in order, not_before dates, known techs, no duplicates, prerequisites at
    the start / earlier in the path / in research_priority, start_year not after the entry year
    (warning).
  * technology parents (both lists): a tech with several leads_to_tech parents is startable when
    ANY parent is done (hoi4_live/gamedata/tech.py) or only when ALL are (tech_tree.rs). An error
    when neither reading allows it; a warning naming the tech and the missing parents when only
    the ANY reading does.
  * construction_plan: buildings, state ids, province in its state, triggers (bare date,
    province_controlled:<id> in the item's state, state_fully_controlled:<id>, owner:<id>=<TAG>;
    free text is a player-only warning), railway paths (provinces in from_state/to_state at the
    ends, consecutive provinces touching on map/provinces.bmp), unique order.
  * vp_targets: province in its state, vp equal to the state history at the start date.
  * army_layout: role strike|guard, templates in the start OOB the history file loads, divisions
    of those templates in the state at the start (none: error; fewer than count: warning),
    guard posts, owner: triggers.
  * war / war_by_phase: state ids in axes, theatre, guard_states, guards; enemy tags.
  * doctrines: {id, folder, grand_doctrine | track + subdoctrine, xp} against common/doctrines.
  * event_rules: the event id exists in events/ and option names are options of it.

Stdlib only, Python 3.9+. Exit status 0 when there are no errors.

Optional plan key "_validator" (ignored by the runner):
  {"tag": "GER", "start_date": "1936.1.1", "stop_date": "1938.1.1",
   "completed_techs": [...], "completed_focuses": [...],
   "history_file": "history/countries/GER - Germany.txt",   (default: found by tag)
   "dlcs": ["Anniversary Pack", "Poland: United and Ready"]}

"dlcs" lists the ACTIVE DLC display names (the game log's "Active DLC" lines, not
dlc_load.json). When present, every has_dlc/tag/original_tag condition in the files
is evaluated: focus_path/focus_fallback must belong to the focus tree that actually
loads for the tag (error), start techs and the start OOB follow the history file's
if/else branches, advisors in disallowed character instances are unknown, and
construction/deploy states not owned by the tag at the start date are warned about.
Without "dlcs" every branch is read (legacy behaviour). History (country and state) is applied
at "start_date" (default 1936.1.1): dated blocks on or before it count, later ones do not.
"""
import argparse
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from hoi4facts import content  # noqa: E402
from hoi4facts.game import Game  # noqa: E402,F401
from hoi4facts.script import (  # noqa: E402,F401
    Ctx, date_tuple, get, get1, is_date, parse, read, run_effects, tokenize, trigger, trigger_one,
)
from hoi4facts.validate import (  # noqa: E402,F401
    Report, check_army_layout, check_construction, check_construction_plan, check_decisions,
    check_doctrines, check_event_rules, check_focus, check_misc, check_politics, check_production,
    check_research, check_research_path, check_training, check_vp_targets, check_war, ctx_of,
    dup_check, parse_when, strings, validate,
)

DEFAULT_GAME_DIR = os.path.expanduser(
    "~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV"
)


def load_game(game_dir, userdir=None, mod_args=()):
    """Game over the install, the user folder's enabled mods, then the --mod arguments."""
    mods = content.mods_from_userdir(userdir) if userdir else []
    for arg in mod_args:
        m = content.mod_from_arg(arg, userdir)
        if m is None:
            raise SystemExit(f"--mod {arg}: not a .mod descriptor or a mod folder")
        mods.append(m)
    return Game(game_dir, mods)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("plan")
    ap.add_argument("--game-dir", default=os.environ.get("HOI4_DIR", DEFAULT_GAME_DIR))
    ap.add_argument("--userdir", default=os.environ.get("HOI4_USERDIR"),
                    help="user folder whose dlc_load.json enabled mods are layered over the install")
    ap.add_argument("--mod", action="append", default=[],
                    help="a .mod descriptor or mod folder layered after the user folder's mods")
    ap.add_argument("--strict", action="store_true", help="treat warnings as errors")
    args = ap.parse_args(argv)
    with open(args.plan, encoding="utf-8") as fh:
        plan = json.load(fh)
    if not isinstance(plan, dict):
        print("plan must be a JSON object", file=sys.stderr)
        return 2
    if not os.path.isdir(args.game_dir):
        print(f"game directory not found: {args.game_dir}", file=sys.stderr)
        return 2
    game = load_game(args.game_dir, args.userdir, args.mod)
    for m in game.content.mods:
        print(f"mod: {m.name} ({m.path})")
    rep, counts = validate(plan, game)
    for w in rep.warnings:
        print("warning:", w)
    for e in rep.errors:
        print("error:", e)
    print(
        "checked: "
        + ", ".join(f"{k}={v}" for k, v in counts.items())
        + f"; {len(rep.errors)} error(s), {len(rep.warnings)} warning(s)"
    )
    bad = rep.errors or (args.strict and rep.warnings)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
