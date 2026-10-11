"""Buildings and equipment from the game and mod files: what can be constructed (state or province buildings,
costs, slot sharing) and what can be produced (equipment types, archetypes, years, costs).

    common/buildings          buildings = { id = { base_cost per_level_extra_cost provincial shares_slots ... } }
    common/units/equipment    equipments = { id = { is_archetype archetype parent year build_cost_ic type ... } }
"""

from . import scriptdb
from .scriptdb import as_list, num


def build_buildings(paths):
    out = {}
    order = 0
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        for bb in as_list(block.get("buildings") if isinstance(block, dict) else None):
            if not isinstance(bb, dict):
                continue
            for bid, b in bb.items():
                if bid.startswith("_") or not isinstance(b, dict):
                    continue
                order += 1
                out[bid] = {"order": order, "cost": num(b.get("base_cost")),
                            "cost_per_level": num(b.get("per_level_extra_cost")),
                            "provincial": b.get("provincial") == "yes",
                            "shares_slots": b.get("shares_slots") == "yes",
                            "max_level": num(b.get("max_level")), "port": b.get("is_port") == "yes",
                            "only_costal": b.get("only_costal") == "yes",
                            "special_project": b.get("special_project_building") == "yes",
                            "infrastructure": b.get("infrastructure") == "yes", "file": p}
    return out


def build_equipment(paths):
    out = {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        for eb in as_list(block.get("equipments") if isinstance(block, dict) else None):
            if not isinstance(eb, dict):
                continue
            for eid, e in eb.items():
                if eid.startswith("_") or not isinstance(e, dict):
                    continue
                types = [t for t in as_list(e.get("type")) if isinstance(t, str)]
                out[eid] = {"archetype": e.get("archetype") if isinstance(e.get("archetype"), str) else None,
                            "is_archetype": e.get("is_archetype") == "yes", "year": num(e.get("year")),
                            "cost": num(e.get("build_cost_ic")), "types": types,
                            "parent": e.get("parent") if isinstance(e.get("parent"), str) else None,
                            "buildable": e.get("is_buildable") != "no", "file": p}
    # inherit the archetype's cost and types
    for eid, e in out.items():
        a = out.get(e["archetype"] or "")
        if a:
            if e["cost"] is None:
                e["cost"] = a["cost"]
            if not e["types"]:
                e["types"] = a["types"]
    return out


def index(game_dir, mods):
    b_paths = scriptdb.effective_files("common/buildings", game_dir, mods)
    e_paths = scriptdb.effective_files("common/units/equipment", game_dir, mods)

    def build(_):
        return {"buildings": build_buildings(b_paths), "equipment": build_equipment(e_paths)}
    return scriptdb.cached_index("economy", b_paths + e_paths, build)


def for_session(session):
    return scriptdb.memo(session, "economy", index)


def equipment_name_keys(eid, tag=None):
    """Localisation keys the production list may show for an equipment type."""
    keys = []
    if tag:
        keys += [f"{tag}_{eid}", f"{tag}_{eid}_short"]
    return keys + [eid, f"{eid}_short"]
