"""Events from `events/*.txt` (game + mods): id, type, whether the event is hidden, and its options.

An events file holds `add_namespace = ns` lines and event blocks (`country_event`, `news_event`, `state_event`,
`unit_leader_event`, `operative_leader_event`, `character_event`) with an `id = ns.N`, `hidden = yes` for events that
never show a window, and any number of `option = { name = ... }` blocks. The console command `event <id> [TAG]`
fires country and news events; the other types need a scope the console cannot give, so the smoke test lists them
as skipped.
"""

from . import scriptdb
from .scriptdb import as_list

EVENT_TYPES = ("country_event", "news_event", "state_event", "unit_leader_event", "operative_leader_event",
               "character_event")
FIREABLE = ("country_event", "news_event")


def build_index(paths):
    out = {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:                                   # a broken mod file must not break the index
            continue
        if not isinstance(block, dict):
            continue
        for etype in EVENT_TYPES:
            for ev in as_list(block.get(etype)):
                if not isinstance(ev, dict) or not isinstance(ev.get("id"), str):
                    continue
                options = []
                for o in as_list(ev.get("option")):
                    if isinstance(o, dict):
                        name = o.get("name")
                        options.append({"name": name if isinstance(name, str) else None,
                                        "has_trigger": isinstance(o.get("trigger"), dict)})
                out[ev["id"]] = {"id": ev["id"], "type": etype, "hidden": ev.get("hidden") == "yes",
                                 "triggered_only": ev.get("is_triggered_only") == "yes",
                                 "once": ev.get("fire_only_once") == "yes", "options": options, "file": p}
    return out


def index(game_dir, mods):
    paths = scriptdb.effective_files("events", game_dir, mods)
    return scriptdb.cached_index("events", paths, build_index)


def for_session(session):
    return scriptdb.memo(session, "events", index)


def in_namespace(idx, namespace):
    """Events whose id is `<namespace>.<number>`, in id order (numbers compare as numbers)."""
    prefix = namespace + "."

    def key(e):
        tail = e["id"][len(prefix):]
        return (0, int(tail), "") if tail.isdigit() else (1, 0, tail)
    return sorted((e for eid, e in idx.items() if eid.startswith(prefix)), key=key)
