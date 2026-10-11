"""Laws, advisors and idea slots from the game and mod files.

    common/idea_tags     slots: law slots (economy, trade_laws, mobilization_laws ...), character slots
                         (political_advisor, theorist, army_chief ...), their default costs
    common/ideas         law ideas (categories with law = yes): cost, conditions, modifiers
    common/characters    advisors: character = { advisor = { slot idea_token cost allowed available traits } }
                         (Character modding wiki: advisors are hired through their idea_token; the default cost
                         comes from the slot when the advisor has none)
"""

from . import scriptdb
from .scriptdb import as_list, num, render


def build_slots(paths):
    """{slot: {"group", "cost", "removal_cost", "law", "character"}} plus {"_groups": {group: [slots]}}."""
    slots, groups = {}, {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        cats = block.get("idea_categories") if isinstance(block, dict) else None
        for cb in as_list(cats):
            if not isinstance(cb, dict):
                continue
            for group, g in cb.items():
                if group.startswith("_") or not isinstance(g, dict):
                    continue
                law = g.get("law") == "yes"
                for kind in ("slot", "character_slot"):
                    for s in as_list(g.get(kind)):
                        if not isinstance(s, str):
                            continue
                        slots.setdefault(s, {"group": group, "cost": num(g.get("cost")),
                                             "removal_cost": num(g.get("removal_cost")), "law": law,
                                             "character": kind == "character_slot"})
                        groups.setdefault(group, [])
                        if s not in groups[group]:
                            groups[group].append(s)
    slots["_groups"] = groups
    return slots


def build_laws(paths):
    """{category: {"law": bool, "ideas": {id: {...}}}} for idea categories marked law = yes."""
    out = {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        for ib in as_list(block.get("ideas") if isinstance(block, dict) else None):
            if not isinstance(ib, dict):
                continue
            for cat, cb in ib.items():
                if cat.startswith("_") or not isinstance(cb, dict) or cb.get("law") != "yes":
                    continue
                entry = out.setdefault(cat, {"ideas": {}})
                for iid, idea in cb.items():
                    if not isinstance(idea, dict) or iid in ("law", "use_list_view", "_repeated"):
                        continue
                    entry["ideas"][iid] = {
                        "cost": num(idea.get("cost")), "removal_cost": num(idea.get("removal_cost")),
                        "allowed": render(idea["allowed"], 200) if isinstance(idea.get("allowed"), dict) else None,
                        "available": render(idea["available"], 300) if isinstance(idea.get("available"), dict) else None,
                        "modifier": render(idea["modifier"], 300) if isinstance(idea.get("modifier"), dict) else None,
                        "default": idea.get("default") == "yes",
                        "file": p}
    return out


def build_advisors(paths):
    """{idea_token: {"character", "name", "slot", "cost", "allowed", "available", "traits", "ledger"}}."""
    out = {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:
            continue
        for cb in as_list(block.get("characters") if isinstance(block, dict) else None):
            if not isinstance(cb, dict):
                continue
            for cid, ch in cb.items():
                if cid.startswith("_") or not isinstance(ch, dict):
                    continue
                for adv in as_list(ch.get("advisor")):
                    if not isinstance(adv, dict) or not isinstance(adv.get("slot"), str):
                        continue
                    token = adv.get("idea_token") if isinstance(adv.get("idea_token"), str) else cid
                    out[token] = {
                        "character": cid, "name": ch.get("name") if isinstance(ch.get("name"), str) else cid,
                        "slot": adv["slot"], "cost": num(adv.get("cost")), "ledger": adv.get("ledger"),
                        "allowed": render(adv["allowed"], 200) if isinstance(adv.get("allowed"), dict) else None,
                        "available": render(adv["available"], 300) if isinstance(adv.get("available"), dict) else None,
                        "traits": [t for t in as_list(adv.get("traits")) if isinstance(t, str)],
                        "file": p}
    return out


def index(game_dir, mods):
    tag_paths = scriptdb.effective_files("common/idea_tags", game_dir, mods)
    idea_paths = scriptdb.effective_files("common/ideas", game_dir, mods)
    char_paths = scriptdb.effective_files("common/characters", game_dir, mods)

    def build(_):
        return {"slots": build_slots(tag_paths), "laws": build_laws(idea_paths),
                "advisors": build_advisors(char_paths)}
    return scriptdb.cached_index("politics", tag_paths + idea_paths + char_paths, build)


def for_session(session):
    return scriptdb.memo(session, "politics", index)


def law_of(idx, idea):
    """The law category of an idea id, or None."""
    for cat, c in idx["laws"].items():
        if idea in c["ideas"]:
            return cat
    return None


def law_cost(idx, category, idea):
    i = idx["laws"].get(category, {}).get("ideas", {}).get(idea) or {}
    if i.get("cost") is not None:
        return i["cost"]
    s = idx["slots"].get(category) or {}
    return s.get("cost")


def advisors_for(idx, tag, slot=None):
    """Advisors whose `allowed` names this country (original_tag/tag), optionally of one slot. Conditions other
    than the tag are not evaluated (the hiring list on screen is the truth)."""
    out = []
    for token, a in idx["advisors"].items():
        if slot and a["slot"] != slot:
            continue
        allowed = a.get("allowed") or ""
        if tag and (f"original_tag = {tag}" in allowed or f"tag = {tag}" in allowed or
                    a["character"].startswith(tag + "_")):
            out.append(dict(a, token=token))
    return out
