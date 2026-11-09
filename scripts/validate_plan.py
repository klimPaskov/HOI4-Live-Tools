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
  * schema: the whole plan against skills/hoi4-campaign-playbook/plan.schema.json
    (hoi4facts/schema_check.py); entries with schema errors skip the game-file checks below.
  * research_path: years in order, not_before dates, known techs, no duplicates, prerequisites at
    the start / earlier in the path / in research_priority, start_year not after the entry year
    (warning).
  * technology parents (both lists): a tech with several leads_to_tech parents is startable when
    ANY parent is done (hoi4_live/gamedata/tech.py) or only when ALL are (tech_tree.rs). An error
    when neither reading allows it; a warning naming the tech and the missing parents when only
    the ANY reading does.
  * construction_plan: buildings, state ids, province in its state, triggers (bare date,
    province_controlled:<id> in the item's state, state_fully_controlled:<id>, owner:<id>=<TAG>),
    railway paths (provinces in from_state/to_state at the ends, consecutive provinces sharing a
    pixel border in the mod-layered map graph, hoi4facts/mapgraph.py), unique ids.
  * vp_targets: province in its state, vp equal to the state history at the start date.
  * army_layout: role strike|guard, templates in the start OOB the history file loads, divisions
    of those templates in the state at the start (none: error; fewer than count: warning),
    guard posts, owner: triggers.
  * war / war_by_phase: state ids in axes, theatre, guard_states, guards; enemy tags.
  * doctrines: {id, folder, grand_doctrine and/or track + subdoctrine, xp} against
    common/doctrines (folder, track of the grand doctrine, subdoctrine track, XP type and cost).
  * event_rules: the event id exists in events/ and option names are options of it.
  * wave-2 executor keys (native/src/plan_military.rs, plan_affairs.rs): army_group, hold_targets and
    invasions armies are army_layout name_hints (else a warning: an in-game army name), an army in one
    group only, hold corridors whose consecutive provinces touch on the map graph; navy, convoy escort
    and invasion fleets in the start naval OOB (warning), sea region names, invasion states with a
    coastal province and target_province in target_state; designs kind against the base's designer,
    module slots and modules, upgrade rows and levels; production[].design names a design (a line switch
    needs no factories); diplomacy and faction tags, justify war_goal and states; agency upgrades (branch,
    levels), operative missions and their targets; MIO organisations allowed for the tag, policies, trait
    indexes; trade resources and partners; research_bonuses technologies.

Stdlib only, Python 3.9+. Exit status 0 when there are no errors.

Plan key "_validator" (ignored by the runner). Missing fields come from the run folder: run_spec.json
beside the plan (tag, start_date, mode, dlcs unless "auto"), then dossier/manifest.json (tag, date, mode,
resolved dlcs). With no tag anywhere the check reads GER and warns (an error under --strict):
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


def derive_validator(plan, plan_path):
    """Fill a missing _validator tag/start_date/mode/dlcs from the run folder: the plan's folder holds
    run_spec.json (tag, start_date, mode, dlcs unless "auto") and dossier/manifest.json (tag, date, mode,
    the resolved dlcs). Returns (source note or None, warning or None); without a tag anywhere the
    validator falls back to GER and the warning says so."""
    meta = plan.get("_validator")
    meta = dict(meta) if isinstance(meta, dict) else {}
    folder = os.path.dirname(os.path.abspath(plan_path))
    found = []
    sources = (
        ("run_spec.json", (("tag", "tag"), ("start_date", "start_date"), ("mode", "mode"), ("dlcs", "dlcs"))),
        (os.path.join("dossier", "manifest.json"),
         (("tag", "tag"), ("start_date", "date"), ("mode", "mode"), ("dlcs", "dlcs"))),
    )
    for rel, keys in sources:
        try:
            with open(os.path.join(folder, rel), encoding="utf-8") as fh:
                src = json.load(fh)
        except (OSError, ValueError):
            continue
        if not isinstance(src, dict):
            continue
        took = []
        for key, skey in keys:
            v = src.get(skey)
            if key in meta or v in (None, "", "auto"):
                continue
            if key == "dlcs" and not isinstance(v, list):
                continue
            meta[key] = v
            took.append(key)
        if took:
            found.append(f"{rel} ({', '.join(took)})")
    if not meta.get("tag"):
        return None, ("_validator.tag missing and no run_spec.json or dossier/manifest.json beside the plan: "
                      "checking as GER; add _validator {tag, start_date, dlcs}")
    plan["_validator"] = meta
    return ("_validator from " + "; ".join(found)) if found else None, None


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
    note, missing = derive_validator(plan, args.plan)
    if note:
        print("note:", note)
    game = load_game(args.game_dir, args.userdir, args.mod)
    for m in game.content.mods:
        print(f"mod: {m.name} ({m.path})")
    rep, counts = validate(plan, game)
    if missing:
        rep.warnings.insert(0, missing)
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
