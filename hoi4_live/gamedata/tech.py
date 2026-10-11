"""Technologies from common/technologies (game + mods): folders, grid positions, costs, years, prerequisites,
exclusive groups; a planner for what can be researched now; and the tech tree layout from the interface files, so
a technology's tile can be found on screen.

Rules (Technology modding wiki, "Path", "Folder"):
    path = { leads_to_tech = B }   researching B needs at least one of the technologies that lead to it (OR)
    dependencies = { A = 1 }       every dependency is needed as well (AND)
    xor = { ... }                  mutually exclusive technologies
    folder = { name position = { x y } }   grid units of the gridbox the technology hangs from: the gridbox named
                                   `<tech>_tree` in the folder's window, or the gridbox of the earliest technology
                                   that paths to it in the same folder. Format LEFT: x goes down, y goes right;
                                   UP: x goes right, y goes down. One unit = the gridbox slot size (70 px).
Years: a technology researched before its start_year pays an ahead-of-time penalty (shown in the tooltip).
"""

import re

from . import scriptdb
from .scriptdb import as_list, num, render


def _tech_entry(tid, t, order):
    folders = []
    for f in as_list(t.get("folder")):
        if isinstance(f, dict) and isinstance(f.get("name"), str):
            p = f.get("position") or {}
            folders.append({"name": f["name"], "x": num(p.get("x"), 0), "y": num(p.get("y"), 0)})
    leads = []
    for p in as_list(t.get("path")):
        if isinstance(p, dict) and isinstance(p.get("leads_to_tech"), str):
            leads.append(p["leads_to_tech"])
    deps = [k for k in (t.get("dependencies") or {}) if isinstance(t.get("dependencies"), dict) and k != "_repeated"]
    xor = []
    for x in as_list(t.get("xor")) + as_list(t.get("XOR")):
        xor += [v for v in as_list(x) if isinstance(v, str)]
    cats = [c for c in as_list(t.get("categories")) if isinstance(c, str)]
    eq = [e for e in as_list(t.get("enable_equipments")) if isinstance(e, str)]
    return {"id": tid, "order": order, "folders": folders, "cost": num(t.get("research_cost"), 1),
            "year": num(t.get("start_year")), "leads": leads, "deps": deps, "xor": sorted(set(xor)),
            "categories": cats, "equipment": eq,
            "subunits": [s for s in as_list(t.get("enable_subunits")) if isinstance(s, str)],
            "allow": render(t["allow"], 300) if isinstance(t.get("allow"), dict) else None,
            "allow_branch": render(t["allow_branch"], 200) if isinstance(t.get("allow_branch"), dict) else None,
            "sub": [s for s in as_list(t.get("sub_technologies")) if isinstance(s, str)],
            "doctrine": t.get("doctrine") == "yes" or t.get("is_doctrine") == "yes",
            "small": t.get("force_use_small_tech_layout") == "yes" or not eq}


def build_index(paths):
    techs = {}
    order = 0
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        for tblock in as_list(block.get("technologies") if isinstance(block, dict) else None):
            if not isinstance(tblock, dict):
                continue
            for tid, t in tblock.items():
                if tid.startswith("_") or not isinstance(t, dict):
                    continue
                order += 1
                techs[tid] = _tech_entry(tid, t, order)
    parents = {}
    for tid, t in techs.items():
        for l in t["leads"]:
            parents.setdefault(l, []).append(tid)
    for tid, t in techs.items():
        t["parents"] = sorted(parents.get(tid, []), key=lambda p: techs[p]["order"])
    return techs


def build_folders(paths):
    """{folder: {"ledger", "available"}} from common/technology_tags."""
    out = {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        for tf in as_list(block.get("technology_folders") if isinstance(block, dict) else None):
            if isinstance(tf, dict):
                for name, v in tf.items():
                    if name == "_repeated":
                        continue
                    if name == "_items":
                        for n in as_list(v):
                            out.setdefault(str(n), {"ledger": None, "available": None})
                        continue
                    v = v if isinstance(v, dict) else {}
                    out[name] = {"ledger": v.get("ledger"), "available": v.get("available")}
    return out


def _lower_keys(v):
    if isinstance(v, dict):
        return {k.lower() if isinstance(k, str) else k: _lower_keys(x) for k, x in v.items()}
    if isinstance(v, list):
        return [_lower_keys(x) for x in v]
    return v


def pct(v):
    """0.8 for '80%' (or '80%%'), else None."""
    if isinstance(v, str) and v.rstrip("%") != v:
        try:
            return float(v.rstrip("%")) / 100.0
        except ValueError:
            return None
    return None


def _size_info(c, parent):
    """What a container's children inherit for percentage widths: {'w': the nearest numeric width up the chain,
    'fill': whether that width grows to the window (a `min = { width = 100% }` or a percentage width)}."""
    size = c.get("size") if isinstance(c.get("size"), dict) else {}
    w = num(size.get("width"))
    mn = size.get("min") if isinstance(size.get("min"), dict) else {}
    fill = bool(pct(size.get("width")) or pct(mn.get("width")))
    if w is not None:
        return {"w": w, "fill": fill}
    return {"w": parent["w"], "fill": parent["fill"] or fill}


def build_layout(paths, folder_names):
    """{folder: {"gridboxes": {name: {x, y, w, h, format}}, "file"}} from interface .gui files. Gridbox positions
    are relative to the folder window (nested container positions are added). A gridbox with a percentage width
    keeps the percentage (sw_pct) and what it is a percentage of (parent_w, parent_fill); the folder window's
    horizontal margins (margin_w) let tile_offset grow it to the game window."""
    out = {}
    names = set(folder_names)

    tabs = {}

    def walk(block, origin, folder, file, parent):
        for key in ("containerwindowtype", "windowtype"):
            for c in as_list(block.get(key)):
                if not isinstance(c, dict):
                    continue
                name = c.get("name")
                pos = c.get("position") or {}
                px, py = num(pos.get("x"), 0), num(pos.get("y"), 0)
                if isinstance(name, str) and name in names:
                    margin = c.get("margin") if isinstance(c.get("margin"), dict) else {}
                    out[name] = {"gridboxes": {}, "file": file,
                                 "margin_w": num(margin.get("left"), 0) + num(margin.get("right"), 0)}
                    walk(c, (0, 0), name, file, _size_info(c, {"w": None, "fill": True}))
                else:
                    walk(c, (origin[0] + px, origin[1] + py), folder, file, _size_info(c, parent))
        if not folder:
            for b in as_list(block.get("buttontype")):
                if isinstance(b, dict) and isinstance(b.get("name"), str) and b["name"].endswith("_tab"):
                    f = b["name"][:-4]
                    if f in names:
                        pos = b.get("position") or {}
                        tabs[f] = {"x": origin[0] + num(pos.get("x"), 0), "y": origin[1] + num(pos.get("y"), 0)}
        if folder:
            for g in as_list(block.get("gridboxtype")):
                if isinstance(g, dict) and isinstance(g.get("name"), str):
                    pos = g.get("position") or {}
                    ss = g.get("slotsize") or {}
                    size = g.get("size") if isinstance(g.get("size"), dict) else {}
                    out[folder]["gridboxes"][g["name"]] = {
                        "x": origin[0] + num(pos.get("x"), 0), "y": origin[1] + num(pos.get("y"), 0),
                        "w": num(ss.get("width"), 70), "h": num(ss.get("height"), 70),
                        "sw": num(size.get("width")), "sh": num(size.get("height")),
                        "sw_pct": pct(size.get("width")), "parent_w": parent["w"], "parent_fill": parent["fill"],
                        "format": str(g.get("format") or "LEFT").upper()}

    for p in paths:
        try:
            data = scriptdb.read_bytes(p)
        except OSError:
            continue
        if b"gridbox" not in data.lower():
            continue
        try:
            block = _lower_keys(scriptdb.parse_block(data))
        except Exception:
            continue
        for gt in as_list(block.get("guitypes") if isinstance(block, dict) else None):
            if isinstance(gt, dict):
                walk(gt, (0, 0), None, p, {"w": None, "fill": True})
    for f, t in tabs.items():
        out.setdefault(f, {"gridboxes": {}, "file": None})["tab"] = t
    return out


def index(game_dir, mods):
    tech_paths = scriptdb.effective_files("common/technologies", game_dir, mods)
    tag_paths = scriptdb.effective_files("common/technology_tags", game_dir, mods)
    gui_paths = scriptdb.effective_files("interface", game_dir, mods, ext=(".gui",))

    def build(_paths):
        techs = build_index(tech_paths)
        folders = build_folders(tag_paths)
        for t in techs.values():
            for f in t["folders"]:
                folders.setdefault(f["name"], {"ledger": None, "available": None})
        layout = build_layout(gui_paths, list(folders))
        return {"techs": techs, "folders": folders, "layout": layout}
    return scriptdb.cached_index("tech", tech_paths + tag_paths + gui_paths, build, extra="layout-pct-widths")


def for_session(session):
    return scriptdb.memo(session, "tech", index)


# ---------------------------------------------------------------------------------------------- DLC conditions
def eval_dlc(cond, has_dlc):
    """Evaluate a folder's `available` block made of has_dlc / NOT / OR / AND / always (others count as true)."""
    if not isinstance(cond, dict):
        return True
    ok = True
    for k, v in cond.items():
        if k == "_repeated":
            continue
        ku = k.upper()
        for x in as_list(v):
            if ku == "HAS_DLC":
                ok = ok and has_dlc(str(x))
            elif ku == "ALWAYS":
                ok = ok and x == "yes"
            elif ku == "NOT":
                ok = ok and not _any(x, has_dlc)
            elif ku == "OR":
                ok = ok and _any(x, has_dlc)
            elif ku == "AND":
                ok = ok and eval_dlc(x, has_dlc)
    return ok


def _any(cond, has_dlc):
    if not isinstance(cond, dict):
        return False
    for k, v in cond.items():
        if k == "_repeated":
            continue
        for x in as_list(v):
            if eval_dlc({k: x}, has_dlc):
                return True
    return False


def visible_folders(idx, has_dlc=lambda name: True):
    """Folder names whose `available` holds (DLC alternatives such as armour_folder / nsb_armour_folder)."""
    return [f for f, v in idx["folders"].items() if eval_dlc(v.get("available"), has_dlc)]


# ---------------------------------------------------------------------------------------------- planner
def status(t, researched, researching=(), idx_techs=None):
    """'researched', 'researching', 'available', 'locked' (no path parent or a dependency missing) or
    'excluded' (an xor partner is researched). Script `allow` conditions are left to the tree view."""
    done = set(researched or ())
    if t["id"] in done:
        return "researched"
    if t["id"] in set(researching or ()):
        return "researching"
    if any(x in done for x in t.get("xor", ())):
        return "excluded"
    parents = t.get("parents") or []
    if parents and not any(p in done for p in parents):
        return "locked"
    if any(d not in done for d in t.get("deps", ())):
        return "locked"
    if not parents and not t.get("deps") and not t.get("folders"):
        return "hidden"
    return "available"


def plan(idx, researched, researching=(), year=None, folders=None, names=None, limit=None):
    """Available technologies (sorted by folder, then year), each with its folder, year, cost and whether it is
    ahead of time in `year`."""
    names = names or {}
    allowed = set(folders) if folders else None
    rows = []
    for tid, t in idx["techs"].items():
        if not t["folders"] or t.get("doctrine"):
            continue
        fl = [f["name"] for f in t["folders"] if allowed is None or f["name"] in allowed]
        if not fl:
            continue
        st = status(t, researched, researching)
        if st != "available":
            continue
        row = {"id": tid, "name": names.get(tid, tid), "folder": fl[0], "year": t["year"], "cost": t["cost"],
               "approx_days": round(t["cost"] * 100), "categories": t["categories"][:4],
               "equipment": t["equipment"][:3]}
        if year and t["year"] and t["year"] > year:
            row["ahead_years"] = t["year"] - year
        if t.get("allow"):
            row["allow"] = t["allow"]
        rows.append(row)
    rows.sort(key=lambda r: (r["folder"], r["year"] or 0, r["id"]))
    return rows[:limit] if limit else rows


def display_keys(t, tag):
    """Localisation keys the game may show as the technology's name, best first."""
    keys = [f"{tag}_{t['id']}"] if tag else []
    for e in t.get("equipment", [])[:1]:
        if tag:
            keys += [f"{tag}_{e}", f"{tag}_{e}_short"]
    keys.append(t["id"])
    for e in t.get("equipment", [])[:1]:
        keys += [e, f"{e}_short"]
    return keys


# ---------------------------------------------------------------------------------------------- layout
def gridbox_for(idx, tech_id, folder, seen=None):
    """(gridbox, folder entry of the tech) the tech's position is measured from, or (None, entry)."""
    t = idx["techs"].get(tech_id)
    lay = (idx.get("layout") or {}).get(folder) or {}
    boxes = lay.get("gridboxes", {})
    entry = next((f for f in (t or {}).get("folders", []) if f["name"] == folder), None)
    if not t or entry is None:
        return None, entry
    if f"{tech_id}_tree" in boxes:
        return boxes[f"{tech_id}_tree"], entry
    seen = (seen or set()) | {tech_id}
    for p in t.get("parents", []):
        if p in seen:
            continue
        pt = idx["techs"].get(p)
        if pt and any(f["name"] == folder for f in pt["folders"]):
            gb, _ = gridbox_for(idx, p, folder, seen)
            if gb:
                return gb, entry
    return None, entry


def gridbox_width(idx, folder, gb, window_w=None):
    """The gridbox's width in pixels: its own, or its percentage of the nearest sized ancestor, which grows to the
    game window (less the folder window's margins) when that ancestor has a `min` of 100%."""
    if gb.get("sw"):
        return gb["sw"]
    if not gb.get("sw_pct"):
        return None
    pw = gb.get("parent_w") or 0
    if gb.get("parent_fill") and window_w:
        lay = (idx.get("layout") or {}).get(folder) or {}
        pw = max(pw, window_w - (lay.get("margin_w") or 0))
    return gb["sw_pct"] * pw if pw else None


# Folders whose percentage-width gridboxes do not centre their columns (seen live on 1.19.2: the NSB armour tree's
# tiles sit 717 px left of where centring would put them).
LEFT_ALIGNED_FOLDERS = ("nsb_armour_folder",)


def tile_offset(idx, tech_id, folder, window_w=None):
    """Centre of the tech's tile in pixels from the folder window's origin, or None. window_w (the game window's
    width) resolves gridboxes whose width is a percentage of a window-filling container."""
    gb, entry = gridbox_for(idx, tech_id, folder)
    if not gb or entry is None:
        return None
    x, y = entry["x"], entry["y"]
    if gb.get("format", "LEFT").startswith("UP"):
        px, py = gb["x"] + x * gb["w"], gb["y"] + y * gb["h"]
        sw = gridbox_width(idx, folder, gb, window_w)
        if sw and folder not in LEFT_ALIGNED_FOLDERS:   # an UP gridbox with a width centres its columns (seen live)
            px += (sw - gb["w"]) / 2.0
    else:
        px, py = gb["x"] + y * gb["w"], gb["y"] + x * gb["h"]
    return (px + gb["w"] / 2.0, py + gb["h"] / 2.0)


def norm_name(text):
    return re.sub(r"[^a-z0-9]+", "", (text or "").lower())
