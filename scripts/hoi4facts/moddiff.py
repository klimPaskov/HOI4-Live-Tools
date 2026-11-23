"""What a mod set adds or changes against the plain game, read from files only (`facts.py mod-diff`).

An agent that meets a total conversion first needs to know what is different: new resources, buildings,
decision categories, scripted interfaces (custom mechanics), unit types, focus trees, a new map, changed
defines, and which panels the mod replaces. This module compares the effective content of the install with
the effective content of the install plus the mods (the same merge the game does, content.Content) and
reports, per category, what is new, removed or overridden. Nothing here names a mod or a country.

It also checks the panels the typed MCP tools drive: `scripts/data/tool-widgets.json` lists the interface
element names each tool looks up in memory (generated from the runtime's source by `tool_widgets.py`). A name
the plain game defines but the mod set no longer does means that tool will likely not find its button; the
report lists those tools, so the agent knows where to fall back on hoi4_ui_inspect / hoi4_ui_act.

Stdlib only, Python 3.9+.
"""
import json
import os
import re

from .derive import defines, localisation
from .script import get1, parse_file, read

_HERE = os.path.dirname(os.path.abspath(__file__))
TOOL_WIDGETS = os.path.join(_HERE, "..", "data", "tool-widgets.json")

# Categories read from common/: (report key, folder, wrapper key or None, recursive, what an entry is).
# A wrapper key means the entries are the keys inside `wrapper = { ... }`; None means top-level keys.
CATEGORIES = (
    ("resources", "common/resources", "resources", False, "strategic resources"),
    ("buildings", "common/buildings", "buildings", False, "buildings"),
    ("idea_categories", "common/idea_tags", "idea_categories", False, "idea and law categories"),
    ("ideologies", "common/ideologies", "ideologies", False, "ideologies"),
    ("decision_categories", "common/decisions/categories", None, False, "decision categories"),
    ("sub_units", "common/units", "sub_units", False, "battalion and company types"),
    ("equipment", "common/units/equipment", "equipment", False, "equipment types"),
    ("technologies", "common/technologies", "technologies", False, "technologies"),
    ("country_tags", "common/country_tags", None, False, "country tags"),
    ("dynamic_modifiers", "common/dynamic_modifiers", None, False, "dynamic modifiers"),
    ("scripted_effects", "common/scripted_effects", None, False, "scripted effects"),
    ("scripted_triggers", "common/scripted_triggers", None, False, "scripted triggers"),
    ("abilities", "common/abilities", "ability", False, "army abilities"),
    ("occupation_laws", "common/occupation_laws", None, False, "occupation laws"),
)
# Effects whose negative amount spends something the player holds.
SPEND_EFFECTS = ("add_political_power", "add_command_power", "add_manpower", "add_stability", "add_war_support",
                 "army_experience", "navy_experience", "air_experience", "add_resource", "add_fuel",
                 "add_equipment_to_stockpile")
# Effects that cannot be taken back by the player with one click.
IRREVERSIBLE_EFFECTS = ("declare_war_on", "annex_country", "create_wargoal", "set_politics", "puppet",
                        "release", "release_puppet", "transfer_state", "set_state_owner", "start_civil_war",
                        "leave_faction", "dismantle_faction", "create_faction", "add_to_faction", "white_peace",
                        "set_capital", "retire_character", "kill_country_leader", "remove_ideas", "add_ideas",
                        "add_timed_idea", "activate_decision", "complete_national_focus", "unlock_decision_tooltip")
_VAR = re.compile(r"\[\?([A-Za-z_][A-Za-z0-9_@.:^]*)")
_GUI_TYPES = re.compile(r"Type$")


def _sorted(s):
    return sorted(s, key=str.lower)


def _entries(game, folder, wrapper, recursive):
    """{key: source file} of the category's entries in the effective content."""
    out = {}
    for path in game.files(folder, ext=(".txt",), recursive=recursive):
        for k, v in parse_file(path):
            if k is None:
                continue
            if wrapper is None:
                if isinstance(v, list):
                    out[k] = path
            elif k == wrapper and isinstance(v, list):
                for kk, vv in v:
                    if kk is not None and isinstance(vv, list):
                        out[kk] = path
    return out


def _decisions(game):
    """{decision key: (category, is_mission)} from common/decisions (not the categories folder)."""
    out = {}
    for path in game.files("common/decisions", ext=(".txt",)):
        for cat, body in parse_file(path):
            if not isinstance(body, list):
                continue
            for key, d in body:
                if key is None or not isinstance(d, list):
                    continue
                out[key] = (cat, get1(d, "days_mission_timeout") is not None)
    return out


def _focus_trees(game):
    """{tree id: focus count} from common/national_focus (shared focuses under 'shared')."""
    out = {}
    for path in game.files("common/national_focus", ext=(".txt",)):
        for k, v in parse_file(path):
            if k == "focus_tree" and isinstance(v, list):
                tid = get1(v, "id") or os.path.basename(path)
                out[tid] = out.get(tid, 0) + sum(1 for kk, _ in v if kk == "focus")
            elif k == "shared_focus":
                out["shared"] = out.get("shared", 0) + 1
    return out


def _negative(v):
    """A spend: a negative literal, a negated variable, or a block whose amount/value is one."""
    if isinstance(v, str):
        return v.startswith("-")
    if isinstance(v, list):
        return any(_negative(get1(v, k)) for k in ("amount", "value") if get1(v, k) is not None)
    return False


def effect_profile(block, depth=0, spends=None, irreversible=None):
    """(spends, irreversible) effect keys found anywhere in an effect block (every branch counts)."""
    spends = set() if spends is None else spends
    irreversible = set() if irreversible is None else irreversible
    if isinstance(block, list) and depth <= 12:
        for k, v in block:
            if k in SPEND_EFFECTS and _negative(v):
                spends.add(k)
            elif k in IRREVERSIBLE_EFFECTS:
                irreversible.add(k)
            if isinstance(v, list):
                effect_profile(v, depth + 1, spends, irreversible)
    return spends, irreversible


def scripted_guis(game):
    """{name: {...}} of common/scripted_guis: the window it shows, its context, its buttons with what each
    click does (effect keys) and whether the click spends something (cost effects), from files only."""
    out = {}
    for path in game.files("common/scripted_guis", ext=(".txt",)):
        for k, v in parse_file(path):
            if k != "scripted_gui" or not isinstance(v, list):
                continue
            for name, g in v:
                if name is None or not isinstance(g, list):
                    continue
                effects = get1(g, "effects") or []
                triggers = get1(g, "triggers") or []
                buttons = []
                for ek, ev in effects if isinstance(effects, list) else []:
                    if ek is None:
                        continue
                    spends, irreversible = effect_profile(ev)
                    widget = re.sub(r"_(click|right_click|shift_click|control_click|alt_click)(_.*)?$", "", ek)
                    buttons.append({
                        "effect": ek,
                        "widget": widget,
                        "spends": _sorted(spends),
                        "irreversible": _sorted(irreversible),
                        "enabled_trigger": any(tk == ek + "_enabled" for tk, _ in triggers
                                               if isinstance(triggers, list)),
                    })
                out[name] = {
                    "window_name": get1(g, "window_name"),
                    "context_type": get1(g, "context_type"),
                    "parent_window": get1(g, "parent_window_token") or get1(g, "parent_window_name"),
                    "has_visible_trigger": get1(g, "visible") is not None,
                    "buttons": buttons,
                    "file": path,
                }
    return out


def gui_elements(game, only=None):
    """{element name: (element type, file)} of every named element in interface/*.gui (only: paths to read)."""
    out = {}

    def walk(items, depth):
        if depth > 40:
            return
        for k, v in items:
            if isinstance(v, list):
                if k and _GUI_TYPES.search(k):
                    n = get1(v, "name")
                    if isinstance(n, str) and n:
                        out.setdefault(n, (k, path))
                walk(v, depth + 1)

    for path in only if only is not None else game.files("interface", ext=(".gui",), recursive=True):
        walk(parse_file(path), 0)
    return out


def _gui_vars(game, paths):
    """Variable names a mod shows in its interface and texts ([?var] in .gui files and localisation)."""
    names = set()
    for p in paths:
        try:
            names.update(m.split("@")[0].split("|")[0] for m in _VAR.findall(read(p)))
        except OSError:
            continue
    return names


def _map_facts(game):
    out = {}
    d = game.content.file("map/definition.csv")
    if d:
        try:
            out["provinces"] = sum(1 for line in read(d).splitlines()[1:] if line.strip() and line[0].isdigit())
        except OSError:
            pass
    out["states"] = len(game.files("history/states", ext=(".txt",)))
    out["strategic_regions"] = len(game.files("map/strategicregions", ext=(".txt",)))
    return out


def _common_folders(game):
    """{common sub-folder: file count} of the effective content."""
    out = {}
    for low, (rel, _p, _s) in game.content.merged().items():
        parts = rel.split("/")
        if parts[0] == "common" and len(parts) > 2:
            out[parts[1]] = out.get(parts[1], 0) + 1
    return out


def _mod_paths(game, top, ext):
    """Effective files under `top` that come from a mod."""
    return [p for p in game.files(top, ext=ext, recursive=True) if game.content.source(p)]


def _cmp(a, b, limit):
    new = _sorted(set(b) - set(a))
    gone = _sorted(set(a) - set(b))
    return {"vanilla": len(a), "modded": len(b), "new_count": len(new), "removed_count": len(gone),
            "new": new[:limit], "removed": gone[:limit]}


def load_tool_widgets(path=TOOL_WIDGETS):
    try:
        with open(path, encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return {}


def affected_tools(vanilla_gui, modded_gui, tool_widgets):
    """Typed tools whose interface element names the plain game defines but the mod set does not."""
    out = []
    for module, row in sorted((tool_widgets.get("modules") or {}).items()):
        missing = _sorted(n for n in row.get("widgets", []) if n in vanilla_gui and n not in modded_gui)
        if missing:
            out.append({"module": module, "tools": row.get("tools", []), "missing_widgets": missing[:25],
                        "missing_count": len(missing)})
    out.sort(key=lambda r: -r["missing_count"])
    return out


def diff(vanilla, modded, limit=40, tool_widgets=None):
    """The mod-diff report of two hoi4facts Game objects (install only, install + mods)."""
    rep = {"mods": [m.name for m in modded.content.mods], "categories": {}}
    for key, folder, wrapper, rec, what in CATEGORIES:
        row = _cmp(_entries(vanilla, folder, wrapper, rec), _entries(modded, folder, wrapper, rec), limit)
        row["what"] = what
        rep["categories"][key] = row
    vd, md = _decisions(vanilla), _decisions(modded)
    row = _cmp(vd, md, limit)
    row["what"] = "decisions"
    row["missions_modded"] = sum(1 for _, m in md.values() if m)
    new_cats = {}
    for k in set(md) - set(vd):
        new_cats[md[k][0]] = new_cats.get(md[k][0], 0) + 1
    row["new_by_category"] = dict(sorted(new_cats.items(), key=lambda kv: -kv[1])[:limit])
    rep["categories"]["decisions"] = row
    vf, mf = _focus_trees(vanilla), _focus_trees(modded)
    row = _cmp(vf, mf, limit)
    row["what"] = "focus trees"
    row["largest_new"] = sorted(((k, mf[k]) for k in set(mf) - set(vf)), key=lambda kv: -kv[1])[:10]
    rep["categories"]["focus_trees"] = row

    vs, ms = scripted_guis(vanilla), scripted_guis(modded)
    new_guis = {k: ms[k] for k in _sorted(set(ms) - set(vs))}
    rep["scripted_guis"] = {
        "vanilla": len(vs), "modded": len(ms), "new_count": len(new_guis),
        "new": [dict(name=k, **{kk: vv for kk, vv in g.items() if kk != "file"},
                     buttons_count=len(g["buttons"])) for k, g in list(new_guis.items())[:limit]],
    }
    for g in rep["scripted_guis"]["new"]:
        g["buttons"] = g["buttons"][:12]

    va, ma = _map_facts(vanilla), _map_facts(modded)
    rep["map"] = {"vanilla": va, "modded": ma,
                  "replaced": bool(modded.content.source(modded.content.file("map/definition.csv") or ""))}
    vdef, mdef = defines(vanilla), defines(modded)
    changed = _sorted(k for k in mdef if k in vdef and mdef[k] != vdef[k])
    rep["defines"] = {"changed_count": len(changed), "new_count": len(set(mdef) - set(vdef)),
                      "changed": [{"name": k, "vanilla": vdef[k], "modded": mdef[k]} for k in changed[:limit]]}

    vcf, mcf = _common_folders(vanilla), _common_folders(modded)
    rep["common_folders"] = {
        "new": _sorted(set(mcf) - set(vcf)),
        "changed": [{"folder": k, "vanilla_files": vcf[k], "modded_files": mcf[k]}
                    for k in _sorted(set(vcf) & set(mcf)) if any(
                        modded.content.source(p) for p in modded.files("common/" + k, ext=None, recursive=True))],
    }

    mod_gui = _mod_paths(modded, "interface", (".gui",))
    vanilla_rel = {r.lower() for r in (rel for rel, _p, _s in vanilla.content.merged().values())}
    replaced = _sorted(modded.content.relpath(p) for p in mod_gui if modded.content.relpath(p).lower() in vanilla_rel)
    added = _sorted(modded.content.relpath(p) for p in mod_gui if modded.content.relpath(p).lower() not in vanilla_rel)
    vg, mg = gui_elements(vanilla), gui_elements(modded)
    rep["interface"] = {"mod_gui_files": len(mod_gui), "replaced_files": replaced[:limit],
                        "replaced_count": len(replaced), "new_files": added[:limit], "new_count": len(added),
                        "new_elements": len(set(mg) - set(vg)), "removed_elements": len(set(vg) - set(mg))}
    variables = _gui_vars(modded, mod_gui + _mod_paths(modded, "localisation", (".yml",)))
    rep["shown_variables"] = {"count": len(variables), "names": _sorted(variables)[:limit]}
    rep["affected_tools"] = affected_tools(vg, mg, tool_widgets if tool_widgets is not None else load_tool_widgets())
    rep["learning_targets"] = learning_targets(rep, modded, new_guis, limit)
    return rep


def _clean(text):
    """Localised text without icons (£icon|frame) and colour codes (§X ... §!)."""
    if not text:
        return None
    text = re.sub(r"£[^\s£]*", "", text)
    text = re.sub(r"§.", "", text).strip()
    return text or None


def learning_targets(rep, modded, new_guis=None, limit=40):
    """What an agent should go and learn first, most useful first: panels the typed tools may no longer find,
    new resources and buildings, scripted interfaces with buttons (those that spend something or cannot be
    undone first, then by button count), new decision categories."""
    table = localisation(modded)

    def loc(key):
        return _clean(table.get(key))

    out = []
    for row in rep["affected_tools"][:10]:
        out.append({"kind": "replaced_panel", "id": row["module"], "tools": row["tools"],
                    "missing_widgets": row["missing_widgets"][:6],
                    "how": "the typed tool may fail here: open the panel, hoi4_ui_inspect it, learn a skill"})
    for key in ("resources", "buildings"):
        for k in rep["categories"][key]["new"][:(8 if key == "resources" else 5)]:
            out.append({"kind": key[:-1], "id": k, "name": loc(k) or loc("state_resource_" + k)})
    guis = []
    for name, g in (new_guis or {}).items():
        if not g["buttons"]:
            continue
        spends = _sorted({x for b in g["buttons"] for x in b["spends"]})
        irreversible = _sorted({x for b in g["buttons"] for x in b["irreversible"]})
        guis.append(((not spends and not irreversible), -len(g["buttons"]), name.lower(), {
            "kind": "scripted_gui", "id": name, "window": g.get("window_name"), "context": g.get("context_type"),
            "attaches_to": g.get("parent_window"), "name": loc(g.get("window_name") or "") or loc(name),
            "buttons": len(g["buttons"]), "spends": spends, "irreversible": irreversible,
            "how": "open the panel it attaches to (or the map, for a map context), hoi4_ui_inspect, "
                   "then hoi4_ui_act on one button; save what worked with hoi4_skill_save"}))
    out.extend(row for *_k, row in sorted(guis, key=lambda t: t[:3])[:15])
    for cat in rep["categories"]["decision_categories"]["new"][:15]:
        out.append({"kind": "decision_category", "id": cat, "name": loc(cat),
                    "how": "hoi4_decisions_list shows its decisions; hoi4_decision_take takes one"})
    return out[:limit]


def text(rep):
    """A short plain-text summary of a diff report."""
    lines = ["Mod set: " + (", ".join(rep["mods"]) or "(none)")]
    m = rep["map"]
    lines.append("Map: %s (provinces %s -> %s, states %s -> %s)" % (
        "REPLACED" if m["replaced"] else "plain", m["vanilla"].get("provinces"), m["modded"].get("provinces"),
        m["vanilla"].get("states"), m["modded"].get("states")))
    for key, row in rep["categories"].items():
        if row["new_count"] or row["removed_count"]:
            lines.append("%-20s %5d -> %5d  new %d, removed %d%s" % (
                key, row["vanilla"], row["modded"], row["new_count"], row["removed_count"],
                (": " + ", ".join(row["new"][:8])) if row["new"] else ""))
    sg = rep["scripted_guis"]
    lines.append("Scripted interfaces: %d -> %d (%d new)" % (sg["vanilla"], sg["modded"], sg["new_count"]))
    for g in sg["new"][:10]:
        lines.append("  %s (window %s, %d buttons%s)" % (g["name"], g.get("window_name"), g["buttons_count"],
                                                       ", spends " + "/".join(sorted({s for b in g["buttons"]
                                                                                      for s in b["spends"]}))
                                                       if any(b["spends"] for b in g["buttons"]) else ""))
    d = rep["defines"]
    lines.append("Defines: %d changed, %d new" % (d["changed_count"], d["new_count"]))
    i = rep["interface"]
    lines.append("Interface: %d mod .gui files (%d replace plain-game files, %d new)" % (
        i["mod_gui_files"], i["replaced_count"], i["new_count"]))
    if rep["common_folders"]["new"]:
        lines.append("New common folders: " + ", ".join(rep["common_folders"]["new"]))
    lines.append("Variables shown in the mod's texts: %d" % rep["shown_variables"]["count"])
    if rep["affected_tools"]:
        lines.append("Typed tools likely affected (interface names gone):")
        for r in rep["affected_tools"][:12]:
            lines.append("  %s %s: %s" % (r["module"], ",".join(r["tools"][:4]) or "(shared helper)",
                                          ", ".join(r["missing_widgets"][:6])))
    lines.append("Learn first:")
    for t in rep["learning_targets"][:24]:
        extra = ""
        if t["kind"] == "scripted_gui":
            extra = " (%d buttons%s)" % (t["buttons"], ", spends " + "/".join(t["spends"]) if t["spends"] else "")
        lines.append("  %-16s %s%s%s" % (t["kind"], t["id"], " = " + t["name"] if t.get("name") else "", extra))
    return "\n".join(lines)
