"""Generate the telemetry companion mod `hoi4live_telemetry` into the user's mod folder.

The mod is tiny and additive: it overrides no vanilla file, every name it defines starts with `hoi4live_`, and its
script only reads state and calls `log`. It writes `HOI4LIVE <json members>` lines to game.log:

    daily, player country (on_daily, is_ai = no)
        day      political power, stability, war support, command power, manpower, factories by type and in use,
                 divisions, battalions, ships, planes, research slots, technologies, fuel, convoys, world tension,
                 states, war summary (enemies, surrender progress, strength ratios, casualties, front states)
        pol      name, ruling party, ideology, popularity, leader, days to the next election, subjects
        fac      faction name, leader, members                      (when in a faction)
        subject  overlord and autonomy                              (when a subject)
        res      per resource R: R (surplus), R_prod, R_used, R_exp, R_imp
        eq       per equipment archetype E: E (stockpile), E_army (in armies), E_need (needed by armies)
        war      one line per enemy: surrender progress, divisions, factories, manpower, opinion
        watch    agent-registered variables and flags (player country and global)
        kt       known technology (tid = index into the manifest's `techs`): a full scan weekly and whenever the
                 number of researched technologies changed
        rs       technology being researched (tid), one line per busy slot
    weekly, player country (on_weekly)
        wc       one line per relevant country (majors, countries at war, neighbours, faction members); the
                 faction leader only for countries in a faction (FactionLeader outside one logs an error)
        we       enemy pairs of majors and countries at war with the player
        wld      world tension, year, how many countries are at war
        front    enemy-held states touching the player's states and the player's states touching them
        lost     states the player owns but does not control
        st       one line per owned state: id, name, civilian and military factory levels, dockyards,
                 infrastructure, free building slots
    events (all countries; state control only when the player is involved)
        ev       declare_war, peace, capitulation, annex, puppet, join/leave/create faction, party change, control

Every line text is a localisation key (localisation/<language>/hoi4live_telemetry_l_<language>.yml): `log` localises
its argument, and a localisation value may contain escaped quotes, which a script string cannot. HOI4's
localisation drops a literal `{` and reads `[` as the start of a command, so a line carries the members of a JSON
object without the braces, flat (no arrays, no nested objects; watched names are prefixed var., gvar., flag.,
gflag.):

    HOI4LIVE "k":"day","v":1,"g":"<generation>","t":"USA","n":706650,"pp":13.38,...

and the parser wraps it in braces. All tokens used were checked against the game's documentation
(dynamic_variables, triggers, effects, loc_objects) and live on HOI4 1.19.2 (console probes, then the generated
mod over game days, a declared war, a lost state and a white peace) with no error.log line.

A manifest (hoi4live_telemetry.json in the mod folder) records the generation id, schema, the watch lists and the
resources and equipment covered; the generation id is also printed in every `day` line, so the server can tell
whether the running game loaded the current files (the game reads mods only at launch).
"""

import datetime
import hashlib
import json
import os
import re

from ..errors import ToolError

MOD_NAME = "hoi4live_telemetry"
DISPLAY_NAME = "HOI4 Live Telemetry"
SCHEMA = 3
LANGUAGES = ["english", "braz_por", "french", "german", "japanese", "korean", "polish", "russian", "simp_chinese",
             "spanish"]
DEFAULT_RESOURCES = ["oil", "aluminium", "rubber", "tungsten", "steel", "chromium"]
DEFAULT_EQUIPMENT = ["infantry_equipment", "support_equipment", "artillery_equipment", "anti_tank_equipment",
                     "anti_air_equipment", "motorized_equipment", "mechanized_equipment", "armored_car_equipment"]
NAME_RX = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
VAR_RX = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*(?:\^num|@[A-Za-z0-9_]+)?$")


# ---------------------------------------------------------------------------------------------- line templates
def S(expr):
    return ("s", expr)


def N(expr):
    return ("n", expr)


def _json_template(fields):
    """[(key, (kind, expr))] -> `HOI4LIVE ` + the members of a JSON object whose values are localisation
    expressions (no surrounding braces: the game drops `{`)."""
    parts = []
    for key, (kind, expr) in fields:
        val = '"' + expr + '"' if kind == "s" else expr
        parts.append(f'"{key}":{val}')
    return "HOI4LIVE " + ",".join(parts)


def _head(kind, gen=None):
    out = [("k", S(kind)), ("t", S("[ROOT.GetTag]")), ("n", N("[?global.num_days|0]"))]
    if gen:
        out.insert(1, ("v", N(str(SCHEMA))))
        out.insert(2, ("g", S(gen)))
    return out


def templates(spec):
    """{localisation key: line text} for a generation spec."""
    _validate_campaign_spec(spec)
    gen = spec["generation"]
    T = {}
    T["day"] = _head("day", gen) + [
        ("pp", N("[?political_power|.2]")), ("pp_gain", N("[?political_power_growth|.2]")),
        ("stab", N("[?stability|.3]")), ("ws", N("[?has_war_support|.3]")), ("cp", N("[?command_power|.1]")),
        ("mp_k", N("[?manpower_k|.2]")), ("mp_max_avail_k", N("[?max_available_manpower_k|.1]")),
        ("mp_deployed_k", N("[?deployed_total_manpower_k|.1]")),
        ("mp_queue", N("[?amount_manpower_in_deployment_queue|0]")),
        ("garrison_need", N("[?garrison_manpower_need|0]")),
        ("civ", N("[?num_of_civilian_factories|0]")), ("mil", N("[?num_of_military_factories|0]")),
        ("nav", N("[?num_of_naval_factories|0]")), ("civ_free", N("[?num_of_available_civilian_factories|0]")),
        ("mil_free", N("[?num_of_available_military_factories|0]")),
        ("nav_free", N("[?num_of_available_naval_factories|0]")),
        ("civ_proj", N("[?num_of_civilian_factories_available_for_projects|0]")),
        ("fact_owned", N("[?num_of_owned_factories|0]")), ("fact_total", N("[?num_of_factories|0]")),
        ("div", N("[?num_divisions|0]")), ("bat", N("[?num_battalions|0]")), ("armies", N("[?num_armies|0]")),
        ("ships", N("[?num_ships|0]")), ("planes", N("[?num_deployed_planes|0]")),
        ("rslots", N("[?amount_research_slots|0]")), ("techs", N("[?num_researched_technologies|0]")),
        ("fuel_k", N("[?fuel_k|.2]")), ("fuel_max_k", N("[?max_fuel_k|.2]")), ("fuel_ratio", N("[?fuel_ratio|.3]")),
        ("convoys", N("[?num_equipment@convoy|0]")), ("convoy_threat", N("[?convoy_threat|.3]")),
        ("threat", N("[?global.threat|.3]")),
        ("states", N("[?num_owned_states|0]")), ("states_ctrl", N("[?num_controlled_states|0]")),
        ("occupied", N("[?num_occupied_states|0]")),
        ("war", N("[?hoi4live_war|0]")), ("enemies", N("[?enemies^num|0]")),
        ("war_months", N("[?longest_war_length|0]")), ("surrender", N("[?surrender_progress|.3]")),
        ("war_score_any", N("[?any_war_score|.1]")), ("enemy_str", N("[?enemies_strength_ratio|.2]")),
        ("ally_str", N("[?alliance_strength_ratio|.2]")), ("cas_k", N("[?casualties_k|.1]")),
        ("front", N("[?hoi4live_front|0]")),
        ("xp_army", N("[?army_experience|.1]")), ("xp_navy", N("[?navy_experience|.1]")),
        ("xp_air", N("[?air_experience|.1]")),
    ]
    T["pol"] = _head("pol") + [
        ("name", S("[ROOT.GetName]")), ("party", S("[ROOT.GetRulingParty]")),
        ("ideo", S("[ROOT.GetRulingIdeology]")), ("pop", N("[?party_popularity@ruling_party|.3]")),
        ("leader", S("[ROOT.GetLeader]")), ("elect_days", N("[?days_until_next_election|0]")),
        ("subjects", N("[?num_subjects|0]")), ("in_faction", N("[?hoi4live_in_faction|0]")),
        ("subject", N("[?hoi4live_subject|0]")),
    ]
    T["fac"] = _head("fac") + [
        ("name", S("[ROOT.GetFactionName]")), ("leader", S("[ROOT.FactionLeader.GetTag]")),
        ("members", N("[?faction_members^num|0]")),
    ]
    T["subject"] = _head("subject") + [
        ("overlord", S("[ROOT.Overlord.GetTag]")), ("autonomy", N("[?autonomy_ratio|.3]")),
    ]
    T["res"] = _head("res")
    for r in spec["resources"]:
        T["res"] += [(r, N(f"[?resource@{r}|.1]")), (r + "_prod", N(f"[?resource_produced@{r}|.1]")),
                     (r + "_used", N(f"[?resource_consumed@{r}|.1]")), (r + "_exp", N(f"[?resource_exported@{r}|.1]")),
                     (r + "_imp", N(f"[?resource_imported@{r}|.1]"))]
    T["eq"] = _head("eq")
    for e in spec["equipment"]:
        T["eq"] += [(e, N(f"[?num_equipment@{e}|0]")), (e + "_army", N(f"[?num_equipment_in_armies@{e}|0]")),
                    (e + "_need", N(f"[?num_target_equipment_in_armies@{e}|0]"))]
    T["war"] = _head("war") + [
        ("e", S("[THIS.GetTag]")), ("name", S("[THIS.GetName]")), ("sp", N("[?surrender_progress|.3]")),
        ("div", N("[?num_divisions|0]")), ("civ", N("[?num_of_civilian_factories|0]")),
        ("mil", N("[?num_of_military_factories|0]")), ("mp_k", N("[?manpower_k|.1]")),
        ("opin", N("[?opinion@ROOT|0]")),
    ]
    w = spec["watch"]
    watch = []
    watch += [("var." + v, N(f"[?{v}|.3]")) for v in w["variables"]]
    watch += [("gvar." + v, N(f"[?global.{v}|.3]")) for v in w["global_variables"]]
    watch += [("flag." + f, N(f"[?hoi4live_f{i}|0]")) for i, f in enumerate(w["flags"])]
    watch += [("gflag." + f, N(f"[?hoi4live_g{i}|0]")) for i, f in enumerate(w["global_flags"])]
    if watch:
        T["watch"] = _head("watch") + watch
    T["kt"] = _head("kt") + [("tid", N("[?hoi4live_tid|0]"))]
    T["rs"] = _head("rs") + [("tid", N("[?hoi4live_tid|0]"))]
    T["st"] = _head("st") + [
        ("s", N("[THIS.GetID]")), ("name", S("[THIS.GetName]")), ("civ", N("[?industrial_complex_level|0]")),
        ("mil", N("[?arms_factory_level|0]")), ("dock", N("[?building_level@dockyard|0]")),
        ("infra", N("[?infrastructure_level|0]")), ("free", N("[?hoi4live_fs|0]")),
        ("ctrl", S("[THIS.Controller.GetTag]")),
    ]
    T["wld"] = _head("wld") + [
        ("yr", N("[?global.year|0]")), ("threat", N("[?global.threat|.3]")),
        ("at_war", N("[?hoi4live_at_war|0]")), ("listed", N("[?hoi4live_listed|0]")),
    ]
    T["wc"] = _head("wc") + [
        ("c", S("[THIS.GetTag]")), ("name", S("[THIS.GetName]")), ("ideo", S("[THIS.GetRulingIdeology]")),
        ("stab", N("[?stability|.3]")), ("ws", N("[?has_war_support|.3]")),
        ("civ", N("[?num_of_civilian_factories|0]")), ("mil", N("[?num_of_military_factories|0]")),
        ("nav", N("[?num_of_naval_factories|0]")), ("div", N("[?num_divisions|0]")),
        ("mp_k", N("[?manpower_k|.1]")), ("enemies", N("[?enemies^num|0]")), ("sp", N("[?surrender_progress|.3]")),
        ("opin", N("[?opinion@ROOT|0]")), ("major", N("[?hoi4live_major|0]")), ("vs", N("[?hoi4live_vs|0]")),
        ("fac", N("[?hoi4live_fac|0]")), ("ally", N("[?hoi4live_ally|0]")), ("subj", N("[?hoi4live_subj|0]")),
        ("nb", N("[?hoi4live_nb|0]")),
    ]
    # FactionLeader of a country outside a faction logs "no valid faction leader" to error.log: separate key
    T["wc_fac"] = T["wc"] + [("fleader", S("[THIS.FactionLeader.GetTag]"))]
    T["we"] = [("k", S("we")), ("t", S("[ROOT.GetTag]")), ("n", N("[?global.num_days|0]")),
               ("a", S("[PREV.GetTag]")), ("b", S("[THIS.GetTag]"))]
    state = [("s", N("[THIS.GetID]")), ("name", S("[THIS.GetName]")), ("ctrl", S("[THIS.Controller.GetTag]")),
             ("owner", S("[THIS.Owner.GetTag]"))]
    T["front"] = _head("front") + state + [("side", S("enemy"))]
    T["front_own"] = _head("front") + state + [("side", S("own")), ("vs", N("[PREV.GetID]"))]
    T["lost"] = _head("lost") + state
    ev = lambda e, b=True: [("k", S("ev")), ("e", S(e)), ("n", N("[?global.num_days|0]")),
                            ("a", S("[ROOT.GetTag]"))] + ([("b", S("[FROM.GetTag]"))] if b else [])
    T["ev_declare_war"] = ev("declare_war")
    T["ev_peace"] = ev("peace", False)
    T["ev_capitulation"] = ev("capitulation")
    T["ev_annex"] = ev("annex")
    T["ev_puppet"] = ev("puppet")
    T["ev_join_faction"] = ev("join_faction")
    T["ev_leave_faction"] = ev("leave_faction")
    T["ev_create_faction"] = ev("create_faction")
    T["ev_party"] = ev("party", False) + [("ideo", S("[ROOT.GetRulingIdeology]")),
                                           ("party", S("[ROOT.GetRulingParty]"))]
    T["ev_control"] = ev("control", False) + state
    T["hello"] = [("k", S("hello")), ("v", N(str(SCHEMA))), ("g", S(gen)), ("t", S("[THIS.GetTag]")),
                  ("n", N("[?global.num_days|0]"))]
    if spec.get("campaign", False):
        # Remove the expressions before localization/script generation: filtering
        # tool output alone would still collect exact foreign intelligence in logs.
        from ..state.visibility import HIDDEN, PUBLIC_COUNTRY, PUBLIC_WAR
        T["day"] = [(key, expr) for key, expr in T["day"] if key not in HIDDEN]
        T["war"] = [(key, expr) for key, expr in T["war"] if key in PUBLIC_WAR]
        for kind in ("wc", "wc_fac"):
            T[kind] = [(key, expr) for key, expr in T[kind] if key in PUBLIC_COUNTRY]
    out = {}
    for key, fields in T.items():
        out[f"hoi4live_tlm_{key}"] = _render(fields)
    return out


def _render(fields):
    return _json_template(fields)


# ---------------------------------------------------------------------------------------------- script files
HEADER = """# {display} - generated by HOI4 Live Tools ({gen}). Do not edit: regenerate with hoi4_telemetry action=install.
# Writes compact `HOI4LIVE "k":"day",...` lines (JSON members) to game.log for the HOI4 Live Tools MCP server (read only: no state is
# changed except temporary variables). Remove the mod from the playset with hoi4_telemetry action=disable.
"""


def on_actions_text(spec):
    return HEADER.format(display=DISPLAY_NAME, gen=spec["generation"]) + """
on_actions = {
	# every country runs on_daily and on_weekly; only the player's country (is_ai = no) logs
	on_daily = {
		effect = {
			if = {
				limit = { is_ai = no }
				hoi4live_telemetry_daily = yes
			}
		}
	}
	on_weekly = {
		effect = {
			if = {
				limit = { is_ai = no }
				hoi4live_telemetry_weekly = yes
			}
		}
	}
	# no scope: logs once per human player
	on_startup = {
		effect = {
			every_country = {
				limit = { is_ai = no }
				log = hoi4live_tlm_hello
			}
		}
	}
	# ROOT declares war on FROM
	on_declare_war = { effect = { log = hoi4live_tlm_ev_declare_war } }
	# ROOT is no longer at war
	on_peace = { effect = { log = hoi4live_tlm_ev_peace } }
	# ROOT capitulated, FROM is the winner
	on_capitulation = { effect = { log = hoi4live_tlm_ev_capitulation } }
	# ROOT annexed FROM
	on_annex = { effect = { log = hoi4live_tlm_ev_annex } }
	# ROOT was puppeted by FROM in a peace conference
	on_puppet = { effect = { log = hoi4live_tlm_ev_puppet } }
	# ROOT joined the faction led by FROM
	on_join_faction = { effect = { log = hoi4live_tlm_ev_join_faction } }
	# ROOT left the faction led by FROM
	on_leave_faction = { effect = { log = hoi4live_tlm_ev_leave_faction } }
	# ROOT created a faction, FROM joins it
	on_create_faction = { effect = { log = hoi4live_tlm_ev_create_faction } }
	# ROOT changed its ruling ideology
	on_ruling_party_change = { effect = { log = hoi4live_tlm_ev_party } }
	# ROOT is the new controller, FROM the old one, FROM.FROM the state; logged when the player is either side
	on_state_control_changed = {
		effect = {
			if = {
				limit = {
					OR = {
						is_ai = no
						FROM = { is_ai = no }
					}
				}
				FROM.FROM = { log = hoi4live_tlm_ev_control }
			}
		}
	}
}
"""


def tech_scan_text(spec, kind, trigger):
    """One `if` per technology: when `trigger = <tech>` holds, log hoi4live_tlm_<kind> with tid set to the index."""
    out = []
    for i, tid in enumerate(spec.get("techs") or []):
        out.append(f"\tif = {{ limit = {{ {trigger} = {tid} }} set_temp_variable = {{ hoi4live_tid = {i} }} "
                   f"log = hoi4live_tlm_{kind} }}")
    return "\n".join(out) or "\t# no technologies found"


def effects_text(spec):
    _validate_campaign_spec(spec)
    w = spec["watch"]
    watch_lines = []
    for i, f in enumerate(w["flags"]):
        watch_lines += [f"\tset_temp_variable = {{ hoi4live_f{i} = 0 }}",
                        f"\tif = {{ limit = {{ has_country_flag = {f} }} set_temp_variable = {{ hoi4live_f{i} = 1 }} }}"]
    for i, f in enumerate(w["global_flags"]):
        watch_lines += [f"\tset_temp_variable = {{ hoi4live_g{i} = 0 }}",
                        f"\tif = {{ limit = {{ has_global_flag = {f} }} set_temp_variable = {{ hoi4live_g{i} = 1 }} }}"]
    if any(w.values()):
        watch_lines.append("\tlog = hoi4live_tlm_watch")
    else:
        watch_lines.append("\t# nothing registered (hoi4_telemetry action=watch)")
    watch_body = "\n".join(watch_lines)
    text = HEADER.format(display=DISPLAY_NAME, gen=spec["generation"]) + """
# Country scope: the player's country. Daily lines.
hoi4live_telemetry_daily = {
	set_temp_variable = { hoi4live_war = 0 }
	set_temp_variable = { hoi4live_front = 0 }
	set_temp_variable = { hoi4live_in_faction = 0 }
	set_temp_variable = { hoi4live_subject = 0 }
	if = {
		limit = { has_war = yes }
		set_temp_variable = { hoi4live_war = 1 }
		# enemy-held states that touch a state we control
		every_enemy_country = {
			every_controlled_state = {
				limit = { any_neighbor_state = { is_controlled_by = ROOT } }
				add_to_temp_variable = { hoi4live_front = 1 }
			}
		}
	}
	if = {
		limit = { is_in_faction = yes }
		set_temp_variable = { hoi4live_in_faction = 1 }
	}
	if = {
		limit = { is_subject = yes }
		set_temp_variable = { hoi4live_subject = 1 }
	}
	log = hoi4live_tlm_day
	log = hoi4live_tlm_pol
	if = {
		limit = { is_in_faction = yes }
		log = hoi4live_tlm_fac
	}
	if = {
		limit = { is_subject = yes }
		log = hoi4live_tlm_subject
	}
	log = hoi4live_tlm_res
	log = hoi4live_tlm_eq
	hoi4live_telemetry_tech = yes
	if = {
		limit = { has_war = yes }
		every_enemy_country = { log = hoi4live_tlm_war }
	}
	hoi4live_telemetry_watch = yes
}

# Country scope: the player's country. Technologies being researched (every day) and, when the number of known
# technologies changed, all known ones. tid is the technology's index in the manifest's list.
hoi4live_telemetry_tech = {
	set_temp_variable = { hoi4live_cnt = num_researched_technologies }
	if = {
		limit = { NOT = { check_variable = { hoi4live_known_count = hoi4live_cnt } } }
		hoi4live_telemetry_known = yes
		set_variable = { hoi4live_known_count = hoi4live_cnt }
	}
""" + tech_scan_text(spec, "rs", "is_researching_technology") + """
}

hoi4live_telemetry_known = {
""" + tech_scan_text(spec, "kt", "has_tech") + """
}

# Country scope: the player's country. Every owned state with its factory levels and free building slots.
hoi4live_telemetry_states = {
	every_owned_state = {
		set_temp_variable = { hoi4live_fs = 0 }
""" + "\n".join(f"\t\tif = {{ limit = {{ free_building_slots = {{ building = industrial_complex size > {k} }} }} "
                  f"set_temp_variable = {{ hoi4live_fs = {k + 1} }} }}" for k in range(10)) + """
		log = hoi4live_tlm_st
	}
}

# Country scope: the player's country. Agent-registered variables and flags.
hoi4live_telemetry_watch = {
""" + watch_body + """
}

# Country scope: one listed country (THIS), compared with the player's country (ROOT). Sets the 0/1 temporary
# variables printed by hoi4live_tlm_wc.
hoi4live_telemetry_country_flags = {
	set_temp_variable = { hoi4live_major = 0 }
	set_temp_variable = { hoi4live_vs = 0 }
	set_temp_variable = { hoi4live_fac = 0 }
	set_temp_variable = { hoi4live_ally = 0 }
	set_temp_variable = { hoi4live_subj = 0 }
	set_temp_variable = { hoi4live_nb = 0 }
	if = { limit = { is_major = yes } set_temp_variable = { hoi4live_major = 1 } }
	if = { limit = { has_war_with = ROOT } set_temp_variable = { hoi4live_vs = 1 } }
	if = { limit = { is_in_faction = yes } set_temp_variable = { hoi4live_fac = 1 } }
	if = { limit = { is_in_faction_with = ROOT } set_temp_variable = { hoi4live_ally = 1 } }
	if = { limit = { is_subject = yes } set_temp_variable = { hoi4live_subj = 1 } }
	if = { limit = { is_neighbor_of = ROOT } set_temp_variable = { hoi4live_nb = 1 } }
}

# Country scope: the player's country. Weekly world summary, enemy pairs and fronts.
hoi4live_telemetry_weekly = {
	set_temp_variable = { hoi4live_at_war = 0 }
	set_temp_variable = { hoi4live_listed = 0 }
	every_country = {
		limit = {
			OR = {
				is_major = yes
				has_war = yes
				is_neighbor_of = ROOT
				is_in_faction_with = ROOT
			}
		}
		add_to_temp_variable = { hoi4live_listed = 1 }
		if = {
			limit = { has_war = yes }
			add_to_temp_variable = { hoi4live_at_war = 1 }
		}
		hoi4live_telemetry_country_flags = yes
		if = {
			limit = { is_in_faction = yes }
			log = hoi4live_tlm_wc_fac
		}
		else = {
			log = hoi4live_tlm_wc
		}
		if = {
			limit = {
				has_war = yes
				OR = {
					is_major = yes
					has_war_with = ROOT
				}
			}
			every_enemy_country = { log = hoi4live_tlm_we }
		}
	}
	log = hoi4live_tlm_wld
	hoi4live_telemetry_known = yes
	hoi4live_telemetry_states = yes
	if = {
		limit = { has_war = yes }
		every_enemy_country = {
			every_controlled_state = {
				limit = { any_neighbor_state = { is_controlled_by = ROOT } }
				log = hoi4live_tlm_front
				every_neighbor_state = {
					limit = { is_controlled_by = ROOT }
					log = hoi4live_tlm_front_own
				}
			}
		}
	}
	every_owned_state = {
		limit = { NOT = { is_controlled_by = ROOT } }
		log = hoi4live_tlm_lost
	}
}
"""
    if spec.get("campaign", False):
        # Factory information for our occupied states may reflect enemy-held
        # changes. Keep their public geography in `lost`, but do not collect
        # their exact economy through the owned-state scan.
        text = text.replace("hoi4live_telemetry_states = {\n\tevery_owned_state = {\n",
                            "hoi4live_telemetry_states = {\n\tevery_owned_state = {\n"
                            "\t\tlimit = { is_controlled_by = ROOT }\n")
    return text


def localisation_text(language, lines):
    out = [f"l_{language}:"]
    for key, text in lines.items():
        out.append(f' {key}: "{text.replace(chr(34), chr(92) + chr(34))}"')
    return "﻿" + "\n".join(out) + "\n"


def descriptor_text(game_version_major_minor, path=None):
    lines = ['version="1"', 'tags={', '\t"Utilities"', '}', f'name="{DISPLAY_NAME}"',
             f'supported_version="{game_version_major_minor}.*"']
    if path:
        lines.append(f'path="{path}"')
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------- game data lookups
def mod_paths(user_dir):
    """Folders of the mods enabled in dlc_load.json, in load order (our own mod excluded)."""
    try:
        with open(os.path.join(user_dir, "dlc_load.json"), encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError):
        return []
    out = []
    for rel in data.get("enabled_mods", []):
        if MOD_NAME in rel:
            continue
        desc = os.path.join(user_dir, rel)
        try:
            with open(desc, encoding="utf-8", errors="replace") as f:
                text = f.read()
        except OSError:
            continue
        m = re.search(r'^\s*path\s*=\s*"([^"]+)"', text, re.M)
        if not m:
            continue
        p = m.group(1)
        p = p if os.path.isabs(p) else os.path.join(user_dir, p)
        replace = re.findall(r'^\s*replace_path\s*=\s*"([^"]+)"', text, re.M)
        if os.path.isdir(p):
            out.append({"path": os.path.normpath(p), "replace_path": [r.strip("/").replace("\\", "/") for r in replace]})
    return out


def effective_files(folder, game_dir, mods):
    """Files of a game folder after mod overrides (same relative name replaces, replace_path drops the folder)."""
    files = {}
    roots = [{"path": game_dir, "replace_path": []}] + list(mods)
    for root in roots:
        if folder in root.get("replace_path", []):
            files = {}
        d = os.path.join(root["path"], folder)
        if not os.path.isdir(d):
            continue
        for name in sorted(os.listdir(d)):
            if name.endswith(".txt"):
                files[name] = os.path.join(d, name)
    return [files[k] for k in sorted(files)]


def _read_script(path):
    from ..state.savefile import parse_block
    with open(path, "rb") as f:
        data = f.read()
    if data.startswith(b"\xef\xbb\xbf"):
        data = data[3:]
    return parse_block(data)


def game_resources(game_dir, mods):
    names = []
    for p in effective_files("common/resources", game_dir, mods):
        try:
            block = _read_script(p)
        except Exception:
            continue
        res = block.get("resources") if isinstance(block, dict) else None
        if isinstance(res, dict):
            names += [k for k in res if not k.startswith("_") and NAME_RX.match(k) and k not in names]
    return names


def game_techs(game_dir, mods):
    """Technology ids of the enabled files, in file order (the telemetry's tid is an index into this list)."""
    from ..gamedata import tech as tech_data
    return list(dict.fromkeys(tech_data.build_index(effective_files("common/technologies", game_dir, mods))))


def equipment_archetypes(game_dir, mods):
    out = set()
    for p in effective_files("common/units/equipment", game_dir, mods):
        try:
            block = _read_script(p)
        except Exception:
            continue
        eq = block.get("equipments") if isinstance(block, dict) else None
        if isinstance(eq, dict):
            for k, v in eq.items():
                if isinstance(v, dict) and v.get("is_archetype") == "yes":
                    out.add(k)
    return out


# ---------------------------------------------------------------------------------------------- generation
def mod_dir(user_dir):
    return os.path.join(user_dir, "mod", MOD_NAME)


def descriptor_path(user_dir):
    return os.path.join(user_dir, "mod", MOD_NAME + ".mod")


def manifest_path(user_dir):
    return os.path.join(mod_dir(user_dir), MOD_NAME + ".json")


def read_manifest(user_dir):
    try:
        with open(manifest_path(user_dir), encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def empty_watch():
    return {"variables": [], "global_variables": [], "flags": [], "global_flags": []}


def clean_watch(watch):
    out = empty_watch()
    for key in out:
        for name in (watch or {}).get(key, []) or []:
            rx = VAR_RX if "variables" in key else NAME_RX
            if not isinstance(name, str) or not rx.match(name) or len(name) > 80:
                raise ToolError(f"invalid {key[:-1].replace('_', ' ')} name '{name}' (letters, digits, _; variables "
                                "may end in ^num or @target)", {"field": f"watch.{key}"})
            if name not in out[key]:
                out[key].append(name)
    if sum(len(v) for v in out.values()) > 60:
        raise ToolError("at most 60 watched variables and flags", {"field": "watch"})
    return out


def _validate_campaign_spec(spec):
    if not isinstance(spec.get("campaign", False), bool):
        raise ToolError("campaign generation flag must be boolean")
    if spec.get("campaign", False) and spec.get("watch") != empty_watch():
        raise ToolError("campaign telemetry requires an explicitly empty watch list",
                        {"error": "campaign_watch_forbidden"})


def campaign_compatible(manifest):
    """Older/test manifests are not proof of a campaign-safe generation."""
    return (isinstance(manifest, dict) and manifest.get("campaign") is True
            and manifest.get("schema") == SCHEMA and isinstance(manifest.get("generation"), str)
            and bool(manifest.get("generation"))
            and manifest.get("visibility") == "player_visible_v1"
            and manifest.get("watch") == empty_watch())


def generate(user_dir, game_dir, game_version=None, watch=None, resources=None, equipment=None, techs=None,
             campaign=False):
    """Write the mod; campaign builds never inherit watches or other enabled mods.

    QA builds preserve the previous watch list when watch is None. Campaign
    builds use explicit empty watches, remove foreign force/economy expressions,
    and record their visibility contract in the manifest. This is offline source
    generation; it does not establish that a running game loaded these files.
    """
    if not isinstance(campaign, bool):
        raise ToolError("campaign generation flag must be boolean")
    old = read_manifest(user_dir) or {}
    if campaign:
        if watch not in (None, {}, empty_watch()):
            raise ToolError("campaign telemetry cannot watch variables or flags",
                            {"error": "campaign_watch_forbidden"})
        watch = empty_watch()
        mods = []
    else:
        watch = clean_watch(old.get("watch") if watch is None else watch)
        mods = mod_paths(user_dir)
    if resources is None:
        found = game_resources(game_dir, mods) if game_dir else []
        resources = found or list(DEFAULT_RESOURCES)
    if equipment is None:
        arch = equipment_archetypes(game_dir, mods) if game_dir else set()
        equipment = [e for e in DEFAULT_EQUIPMENT if not arch or e in arch]
    for name in list(resources) + list(equipment):
        if not NAME_RX.match(name):
            raise ToolError(f"invalid resource/equipment name '{name}'")
    if techs is None:
        techs = game_techs(game_dir, mods) if game_dir else []
    for name in techs:
        if not NAME_RX.match(name):
            raise ToolError(f"invalid technology id '{name}'")
    stamp = datetime.datetime.now().strftime("%Y%m%d%H%M%S")
    digest = hashlib.sha1(json.dumps([watch, resources, equipment, techs, SCHEMA, campaign]).encode()).hexdigest()[:6]
    spec = {"generation": f"{stamp}-{digest}", "watch": watch, "resources": list(resources),
            "equipment": list(equipment), "techs": list(techs), "campaign": campaign}
    root = mod_dir(user_dir)
    files = {
        "descriptor.mod": descriptor_text(_major_minor(game_version)),
        os.path.join("common", "on_actions", "hoi4live_telemetry_on_actions.txt"): on_actions_text(spec),
        os.path.join("common", "scripted_effects", "hoi4live_telemetry_effects.txt"): effects_text(spec),
    }
    lines = templates(spec)
    for lang in LANGUAGES:
        files[os.path.join("localisation", lang, f"hoi4live_telemetry_l_{lang}.yml")] = localisation_text(lang, lines)
    written = []
    for rel, text in files.items():
        p = os.path.join(root, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        written.append(rel.replace("\\", "/"))
    with open(descriptor_path(user_dir), "w", encoding="utf-8", newline="\n") as f:
        f.write(descriptor_text(_major_minor(game_version), root.replace("\\", "/")))
    manifest = {"mod": MOD_NAME, "schema": SCHEMA, "generation": spec["generation"],
                "generated_at": datetime.datetime.now().isoformat(timespec="seconds"),
                "watch": watch, "resources": spec["resources"], "equipment": spec["equipment"],
                "techs": spec["techs"], "line_keys": sorted(lines), "files": written, "descriptor": descriptor_path(user_dir),
                "game_version": game_version, "campaign": campaign,
                "visibility": "player_visible_v1" if campaign else "qa_unrestricted"}
    with open(manifest_path(user_dir), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    return manifest


def _major_minor(version):
    m = re.search(r"(\d+)\.(\d+)", version or "")
    return f"{m.group(1)}.{m.group(2)}" if m else "1.19"


def installed(user_dir):
    return os.path.exists(descriptor_path(user_dir)) and os.path.isdir(mod_dir(user_dir))
