"""Decision categories and decisions from common/decisions (game + mods).

Decision modding wiki: a decisions file holds `category = { decision = { ... } }`; categories are defined in
common/decisions/categories. A decision has cost (political power, or custom_cost_trigger with custom_cost_text),
allowed / visible / available conditions, fire_only_once, days_remove (timed), days_re_enable, and missions have
days_mission_timeout with activation / cancel_trigger. Targeted decisions have target_trigger and targets.
"""

from . import scriptdb
from .scriptdb import as_list, num, render


def build_categories(paths):
    out = {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        if not isinstance(block, dict):
            continue
        for cid, c in block.items():
            if cid.startswith("_") or not isinstance(c, dict):
                continue
            out[cid] = {"icon": c.get("icon") if isinstance(c.get("icon"), str) else None,
                        "priority": num(c.get("priority")),
                        "allowed": render(c["allowed"], 200) if isinstance(c.get("allowed"), dict) else None,
                        "visible": render(c["visible"], 200) if isinstance(c.get("visible"), dict) else None,
                        "file": p}
    return out


def build_decisions(paths):
    out = {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        if not isinstance(block, dict):
            continue
        for cat, cb in block.items():
            if cat.startswith("_") or not isinstance(cb, dict):
                continue
            for did, d in cb.items():
                if did.startswith("_") or not isinstance(d, dict):
                    continue
                cost = d.get("cost")
                out[did] = {
                    "category": cat, "cost": num(cost) if not isinstance(cost, dict) else None,
                    "cost_text": d.get("custom_cost_text") if isinstance(d.get("custom_cost_text"), str) else None,
                    "custom_cost": render(d["custom_cost_trigger"], 160) if isinstance(d.get("custom_cost_trigger"), dict) else None,
                    "once": d.get("fire_only_once") == "yes",
                    "days_remove": num(d.get("days_remove")), "days_re_enable": num(d.get("days_re_enable")),
                    "mission": num(d.get("days_mission_timeout")),
                    "targeted": isinstance(d.get("target_trigger"), dict) or d.get("targets") is not None,
                    "allowed": render(d["allowed"], 200) if isinstance(d.get("allowed"), dict) else None,
                    "visible": render(d["visible"], 300) if isinstance(d.get("visible"), dict) else None,
                    "available": render(d["available"], 400) if isinstance(d.get("available"), dict) else None,
                    "effect": render(d["complete_effect"], 500) if isinstance(d.get("complete_effect"), dict) else None,
                    "remove_effect": render(d["remove_effect"], 300) if isinstance(d.get("remove_effect"), dict) else None,
                    "war_with": d.get("war_with_on_complete") or d.get("war_with_target_on_complete"),
                    "icon": d.get("icon") if isinstance(d.get("icon"), str) else None,
                    "file": p}
    return out


def index(game_dir, mods):
    cat_paths = scriptdb.effective_files("common/decisions/categories", game_dir, mods)
    dec_paths = scriptdb.effective_files("common/decisions", game_dir, mods)

    def build(_):
        return {"categories": build_categories(cat_paths), "decisions": build_decisions(dec_paths)}
    return scriptdb.cached_index("decisions", cat_paths + dec_paths, build)


def for_session(session):
    return scriptdb.memo(session, "decisions", index)


def in_category(idx, category):
    return {k: v for k, v in idx["decisions"].items() if v["category"] == category}
