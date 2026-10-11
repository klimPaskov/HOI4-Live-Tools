"""Pure campaign views of structured models and telemetry.

These reducers protect tool output, not arbitrary save queries or free text.
Filter before rendering summaries. Campaign callers must independently enforce
the player tag and refuse unrestricted save paths, watch variables and raw logs.
Foreign rows use allowlists: newly added force/economy fields stay hidden.
"""

from copy import deepcopy
import re


PUBLIC_COUNTRY = frozenset((
    "c", "tag", "country", "name", "ideo", "ideology", "ruling_party", "leader", "major",
    "fac", "faction", "fleader", "faction_leader", "ally", "allies", "subj", "subject", "overlord",
    "nb", "neighbour", "neighbours", "vs", "at_war", "enemies", "sp", "surrender_progress",
    "opin", "opinion", "date", "save_date", "_date", "n", "k", "t", "since", "player",
    "overview", "politics", "diplomacy",
))
PUBLIC_WAR = frozenset((
    "k", "t", "n", "_date", "date", "e", "name", "enemy", "enemy_name", "since", "sp",
    "surrender_progress", "opin", "opinion", "first", "second", "attackers", "defenders",
    "participants", "we_started", "our_casualties", "their_casualties", "provinces_captured_by_us",
    "provinces_captured_by_them", "score_breakdown_us", "score_breakdown_them",
))
PUBLIC_MAP = frozenset((
    "s", "state", "id", "name", "owner", "ctrl", "controller", "held_by", "side", "vs",
    "vp", "victory_points", "distance", "neighbours", "anchor", "anchor_province", "provinces", "share",
    "k", "t", "n", "_date", "date",
))
PUBLIC_FACTION = frozenset(("name", "leader", "members", "ideology", "template", "ours"))
HIDDEN = frozenset((
    "enemy_strength_ratio", "alliance_strength_ratio", "enemy_str", "ally_str",
    "_raw", "raw", "script", "watch", "watch_stale", "variables", "global_variables",
    "flags", "global_flags", "var", "gvar", "flag", "gflag", "strategy", "strategies",
))
ENEMY_UNKNOWN = frozenset(("enemy_divisions", "enemy_org", "enemy_strength", "enemy_organisation"))
KNOWN_RECORDS = frozenset(("day", "pol", "fac", "subject", "res", "eq", "wc", "war", "we",
                           "wld", "front", "lost", "ev", "st", "kt", "rs"))
METADATA = frozenset(("k", "t", "n", "_date", "date"))
TAG = re.compile(r"^[A-Z][A-Z0-9]{2}$")


def _public_country(row, player_tag):
    out = {}
    for key, value in row.items():
        if key not in PUBLIC_COUNTRY:
            continue
        if key in ("overview", "politics") and isinstance(value, dict):
            out[key] = _public_country(value, player_tag)
        elif key == "diplomacy" and isinstance(value, dict):
            out[key] = {k: _reduce(v, player_tag, k) for k, v in value.items()
                        if k in ("at_war", "wars", "relations", "faction", "factions", "allies", "enemies")}
        else:
            out[key] = _reduce(value, player_tag, key)
    return out


def _reduce(value, player_tag, context=None):
    if isinstance(value, (list, tuple)):
        return [_reduce(item, player_tag, context) for item in value]
    if not isinstance(value, dict):
        return deepcopy(value)
    # Every known foreign country and opponent row is restricted even if its
    # telemetry ROOT (t) is the player. ROOT identifies the logger, not the data.
    if context == "wars" or value.get("k") == "war":
        value = {k: v for k, v in value.items() if k in PUBLIC_WAR}
    elif context == "countries":
        if any(TAG.fullmatch(str(k)) and isinstance(v, dict) for k, v in value.items()):
            return {k: (_reduce(v, player_tag) if k == player_tag else _public_country(v, player_tag))
                    for k, v in value.items() if TAG.fullmatch(str(k)) and isinstance(v, dict)}
        country = value.get("c") or value.get("tag") or value.get("country")
        if country != player_tag:
            return _public_country(value, player_tag)
    elif context in ("enemy", "foreign_countries", "opponents") and any(
            TAG.fullmatch(str(k)) and isinstance(v, dict) for k, v in value.items()):
        return {k: _public_country(v, player_tag) for k, v in value.items()
                if TAG.fullmatch(str(k)) and isinstance(v, dict)}
    elif value.get("k") == "wc":
        if value.get("c") != player_tag:
            return _public_country(value, player_tag)
    elif (value.get("tag") and value["tag"] != player_tag) or (
            value.get("c") and value["c"] != player_tag):
        return _public_country(value, player_tag)
    elif (context == "states" or any(k in value for k in ("s", "state", "id"))) and value.get("owner") not in (None, player_tag) and (
            value.get("ctrl") or value.get("controller")) != player_tag:
        value = {k: v for k, v in value.items() if k in PUBLIC_MAP}
    if context in ("faction", "factions") and isinstance(value, dict):
        value = {k: v for k, v in value.items() if k in PUBLIC_FACTION}
    kind = value.get("k")
    if kind and kind not in KNOWN_RECORDS:
        value = {k: v for k, v in value.items() if k in METADATA}
    elif kind and value.get("t") and value["t"] != player_tag and kind not in (
            "war", "wc", "we", "wld", "front", "lost", "ev"):
        return _public_country(value, player_tag)
    out = {}
    for key, item in value.items():
        if key in HIDDEN or str(key).lower().startswith("ai_"):
            continue
        if key in ENEMY_UNKNOWN:
            out[key] = None
        elif key == "_sources" and isinstance(item, dict):
            out[key] = {source: {k: ([f for f in v if f not in HIDDEN and f not in ENEMY_UNKNOWN]
                                    if k == "fields" and isinstance(v, list) else deepcopy(v))
                                 for k, v in details.items()}
                        for source, details in item.items() if isinstance(details, dict)}
        else:
            out[key] = _reduce(item, player_tag, key)
    return out


def campaign_view(value, player_tag):
    """Copy structured state while omitting foreign exact forces/economy and AI state.

    Own country production, units, manpower and other player-visible details are
    preserved. Exact foreign data is never converted into a misleading zero.
    No input is mutated, so a cached test-mode model cannot be contaminated.
    """
    if not isinstance(player_tag, str) or not TAG.fullmatch(player_tag):
        raise ValueError("A trusted player tag is required for campaign visibility")
    return _reduce(value, player_tag)
