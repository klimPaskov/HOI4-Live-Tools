"""What each kind of issue means: its class, the executor lane that owns it, how long it may stay
open (a value judgement with its reason) and, for judgement calls, the named alternatives.

Classes (the owner's five): technical, tool_use, duplicate, idling, gameplay.

Deadlines are game days counted from the first sighting. Each one is a statement of what the
open issue costs per day, never a bare threshold: an idle research slot loses a day of research
every day and the research lane is checked every tick, so one day is already late; a losing
exchange needs two weeks of reads to be a trend and not one bad battle.
"""
from __future__ import annotations

CLASSES = ("technical", "tool_use", "duplicate", "idling", "gameplay")

# code -> (class, lane, days at war, days at peace, why this deadline)
RULES = {
    # --- idling: something assignable left unassigned ---------------------------------------
    "research_slot_idle": ("idling", "research", 1, 1,
                           "an idle slot loses a day of research each day and the research lane is offered every tick"),
    "military_factories_unassigned": ("idling", "production", 2, 3,
                                      "a factory on no line produces nothing, and a line started later "
                                      "starts again from low efficiency"),
    "free_military_factories": ("idling", "production", 2, 3,
                                "a factory on no line produces nothing; the production lane gets a turn every pass"),
    "civilian_factories_idle": ("idling", "construction", 3, 3,
                                "an idle civilian factory builds nothing; a project queued takes it the next day"),
    "free_civilian_factories": ("idling", "construction", 3, 3,
                                "an idle civilian factory builds nothing; a project queued takes it the next day"),
    "dockyards_unassigned": ("idling", "dockyards", 7, 7,
                             "an idle dockyard builds no convoy or ship; one week covers a hull's research wait"),
    "free_dockyards": ("idling", "dockyards", 7, 7,
                       "an idle dockyard builds no convoy or ship; one week covers a hull's research wait"),
    "army_group_without_field_marshal": ("idling", "command", 3, 7,
                                         "the group's armies lose the marshal's skills and he gains no experience; "
                                         "assigning one is a single order"),
    "army_without_general": ("idling", "command", 3, 7,
                             "an army without a general gets no skills and levels no one"),
    "command_power_at_cap": ("idling", "seams", 7, 14,
                             "command power at its cap is lost every day it is not spent"),
    "political_power_unspent": ("idling", "politics", 14, 30,
                                "banked power with an affordable pick buys nothing; a month covers saving for a "
                                "costlier one"),
    "decisions_available": ("idling", "decisions", 14, 30,
                            "a decision the plan does not decide is the model's choice; a month is long for it"),
    "doctrine_affordable": ("idling", "extras", 7, 14,
                            "experience banked with a doctrine affordable gives nothing until it is spent"),
    "doctrines_available": ("idling", "extras", 7, 14,
                            "experience banked with a doctrine affordable gives nothing until it is spent"),
    "special_project_idle": ("idling", "extras", 14, 14,
                             "a facility without a project wastes its scientists' days"),
    "special_project_available": ("idling", "extras", 14, 14,
                                  "a facility without a project wastes its scientists' days"),
    "scientist_unassigned": ("idling", "extras", 14, 14, "a free scientist advances no project"),
    "mio_traits": ("idling", "mio_traits", 21, 21,
                   "a design company's trait points give nothing until picked; its lane may wait for an attack, "
                   "not for three weeks"),
    "modify_officer_corps": ("idling", "officer_corps", 30, 30, "an empty spirit slot gives no bonus"),
    "air_wings_without_mission": ("idling", "air_plan", 2, 14,
                                  "planes on the ground do nothing; at war each day without a mission is a day "
                                  "the enemy owns the air"),
    "naval_task_force_idle": ("idling", "navy", 14, 30, "a fleet on hold raids and protects nothing"),
    "admiral_assignable": ("idling", "navy", 14, 30, "a fleet without an admiral fights without his skills"),
    "army_without_order": ("idling", "war", 1, 30,
                           "at war an army without an order fights only where each division wanders"),
    "front_without_divisions": ("idling", "war", 2, 14, "a front with no divisions holds nothing"),
    "divisions_unassigned": ("idling", "war", 3, 30, "divisions in no army take no orders"),
    "war_orders_silent": ("idling", "war", 3, 3,
                          "an executing operation that sends no army order for days is not steering its war"),
    "intel_no_network_abroad": ("idling", "intelligence", 30, 60,
                                "no operation can start without a network abroad"),
    "outdated_equipment_in_production": ("idling", "designs", 30, 60,
                                         "old equipment produced while a newer design exists"),
    "missing_equipment_production": ("idling", "production", 14, 30, "equipment the army needs has no line"),
    # --- gameplay ---------------------------------------------------------------------------
    "province_overstacked": ("gameplay", "war", 3, 14,
                             "divisions beyond what the province supplies lose organisation and strength daily"),
    "losing_exchange": ("gameplay", "war", 14, 14,
                        "two weeks of reads make a trend rather than one bad battle; a war paid at a losing "
                        "ratio for longer is lost"),
    "victory_points_flat": ("gameplay", "war", 21, 21,
                            "three weeks of fighting without a point taken means the attack is stalled"),
    "own_state_enemy_held": ("gameplay", "war", 7, 7,
                             "an own state held by the enemy costs its factories and points; a week allows a "
                             "counterattack to start"),
    "threatened_state_undefended": ("gameplay", "war", 2, 7, "a state on a hostile border with no division falls"),
    "offensive_at_own_state": ("gameplay", "war", 2, 7, "an arrow at our own state attacks nothing"),
    "offensive_off_line": ("gameplay", "war", 7, 14, "an arrow that does not start from its front line"),
    "planning_near_zero": ("gameplay", "war", 3, 14,
                           "an attack without planning bonus fights at a disadvantage it need not take"),
    "planning_low_at_declaration": ("gameplay", "gate", 0, 0,
                                    "the declaration came before the attacking armies had planned: the first "
                                    "battles are fought without the bonus"),
    "order_not_executing": ("gameplay", "war", 7, 30, "a battle plan that never executes"),
    "air_away_from_front": ("gameplay", "air_plan", 3, 14,
                            "planes flying land missions where no front of ours lies do not help the war"),
    "supply_collapsing": ("gameplay", "war", 7, 14,
                          "supply falling on an attacking front stops the attack before the enemy does"),
    "checkpoint_missed": ("gameplay", "gate", 0, 0,
                          "the plan's dated checkpoint is past and not met: the plan's later steps assume it"),
    "force_behind_plan": ("gameplay", "training", 30, 30,
                          "the operations were sized on the plan's forecast; a month short is a training or "
                          "production gap, not noise"),
    "equipment_short": ("gameplay", "production", 7, 14, "divisions short of equipment fight weaker"),
    "equipment_depleting": ("gameplay", "production", 14, 30, "the stockpile runs out at this rate"),
    "resource_deficit": ("gameplay", "trade", 30, 30, "lines short of resources run below efficiency"),
    "insufficient_resources": ("gameplay", "trade", 30, 30, "lines short of resources run below efficiency"),
    "unfulfilled_import_requests": ("gameplay", "trade", 30, 30, "an import request nobody fills"),
    "convoys_short": ("gameplay", "convoys", 14, 30, "trade and supply by sea stop without convoys"),
    "resistance_above_target": ("gameplay", "occupation", 30, 30, "resistance above target grows into revolt"),
    "battle_reserve_waiting": ("gameplay", "war", 3, 3,
                               "divisions in reserve behind a full combat width fight nobody; spread them"),
    "battle_across_river": ("gameplay", "war", 3, 3, "an attack across a river fights at a penalty"),
    "battle_being_lost": ("gameplay", "war", 3, 3, "a battle being lost costs men and ground"),
    "battle_low_organisation": ("gameplay", "war", 3, 3, "attackers at low organisation lose the battle"),
    "peace_conference_open": ("gameplay", "peace", 0, 0, "a peace conference waits for its demands"),
    "events_open": ("idling", "events", 2, 3, "an open event blocks other input"),
    "events_or_decisions_timeout": ("idling", "events", 2, 3, "a choice left to time out is the game's choice"),
    "executor_repeated_refusal": ("tool_use", "ledger", 3, 7,
                                  "an order refused three times for the same reason will not pass unchanged"),
    "plan_item_blocked": ("gameplay", "plan", 14, 30, "a plan row the executor cannot carry out"),
    # --- tool use, technical and duplicates (from the logs) -----------------------------------
    "invalid_arguments": ("tool_use", None, 1, 1,
                          "the next call of the tool should use the corrected form"),
    "refusal_retried_unchanged": ("tool_use", None, 1, 3,
                                  "an order refused and sent again unchanged meets the same refusal"),
    "precondition_unmet": ("tool_use", None, 1, 3,
                           "the tool was called again while the cause of its last refusal still held"),
    "readback_no_change": ("tool_use", None, 3, 7,
                           "the order's read-back showed no change: the game did not take it"),
    "noop_call": ("duplicate", None, 7, 7,
                  "an order for what already holds costs input time and changes nothing"),
    "duplicate_action": ("duplicate", None, 7, 7,
                         "a verified order sent again unchanged changes nothing and costs input time"),
    "tool_failing": ("technical", None, 1, 1, "a failing tool hides the state or loses the order"),
    "tool_below_history": ("technical", None, 1, 1,
                           "the tool succeeds less often in this run than in earlier ones: something changed"),
    "tool_timeout": ("technical", None, 1, 1, "a timed-out call holds the input lane"),
    "save_failing": ("technical", "saves", 0, 0,
                     "a campaign without a working save cannot be segmented or recovered"),
    "read_errors": ("technical", None, 1, 1, "a read that fails leaves the snapshot blind to its part"),
    "game_not_advancing": ("technical", None, 0, 0, "the game runs unpaused but its date stands still"),
    "memory_growth": ("technical", None, 0, 0, "the game slows and crashes as its memory nears the limit"),
    "executor_silent": ("technical", "executor", 30, 30,
                        "the executor acted on nothing for a month while things stayed open"),
}

# unknown codes (a game alert, a mod's alert): idling, owned by the alerts lane
DEFAULT_RULE = ("idling", "alerts", 14, 30, "the game flags it; two weeks of a flag no rule clears is an omission")

# what each lane's actions look like in the executor's ledger (action kinds start with these)
LANE_KINDS = {
    "research": ("research",), "focus": ("focus",), "production": ("production",),
    "construction": ("construction", "railway"), "dockyards": ("production",), "convoys": ("production",),
    "command": ("army_commander", "general"), "seams": ("general_trait", "army_commander"),
    "politics": ("politics",), "decisions": ("decision",), "extras": ("doctrine", "special_project"),
    "mio_traits": ("mio",), "officer_corps": ("officer",), "air_plan": ("air",), "navy": ("fleet", "admiral"),
    "war": ("army", "air"), "intelligence": ("operative", "intel"), "designs": ("design",),
    "trade": ("trade",), "occupation": ("occupation",), "events": ("event",), "training": ("train", "template"),
    "strategy": (), "plan": (), "gate": (), "peace": ("peace",), "alerts": (), "ledger": (), "saves": ("save",),
    "executor": (),
}

# tools a response may use, by lane (the allowed responses of an escalation packet)
LANE_TOOLS = {
    "research": ["hoi4_research_start"], "production": ["hoi4_production_add", "hoi4_production_set"],
    "construction": ["hoi4_construction_add", "hoi4_railway_build"],
    "dockyards": ["hoi4_production_add", "hoi4_production_set"], "convoys": ["hoi4_production_add"],
    "command": ["hoi4_army_commander"], "seams": ["hoi4_general_trait", "hoi4_army_commander"],
    "politics": ["hoi4_politics_choose"], "decisions": ["hoi4_decision_take"],
    "extras": ["hoi4_doctrine_adopt", "hoi4_special_project_assign"], "mio_traits": ["hoi4_mio_trait"],
    "officer_corps": ["hoi4_officer_corps"], "air_plan": ["hoi4_air_mission", "hoi4_air_rebase"],
    "navy": ["hoi4_fleet_mission", "hoi4_fleet_region"],
    "war": ["hoi4_army_front", "hoi4_army_offensive", "hoi4_army_plan", "hoi4_army_move",
            "hoi4_army_fallback"],
    "intelligence": ["hoi4_operative_assign"], "designs": ["hoi4_design_set_production"],
    "trade": ["hoi4_trade"], "occupation": ["hoi4_occupation_law"], "events": ["hoi4_event_choose"],
    "training": ["hoi4_train"], "saves": ["hoi4_save"],
}

# judgement calls: the named alternatives a rule cannot rank (the escalation offers them)
ALTERNATIVES = {
    "province_overstacked": [
        "widen the front: give the formation's front more of the border (plan edit: front own_states)",
        "split the group: a second formation takes part of the divisions (plan edit: operation group_cap)",
        "move the surplus to the second line or the neighbouring provinces (hoi4_army_move)",
    ],
    "losing_exchange": [
        "stop attacking on that front and hold until planning and organisation recover (hoi4_army_plan stop)",
        "shift the main effort to the sector the enemy holds weakest (plan edit: offensive target_line)",
        "bring armour or air support to the sector (plan edit: formation or air_plan rows)",
    ],
    "victory_points_flat": [
        "move the arrow to the victory-point provinces behind the line (plan edit: offensive target_line)",
        "add the armour reserve as a spearhead on the stalled sector (plan edit)",
        "accept the stall and redirect to the next enemy (plan edit: operation order)",
    ],
    "own_state_enemy_held": [
        "counterattack with the nearest reserve (hoi4_army_offensive)",
        "fall back to a defensible line and hold (hoi4_army_fallback)",
    ],
    "supply_collapsing": [
        "halt the attack until the railway and trucks catch up (hoi4_army_plan stop)",
        "build or repair the railway to the front (hoi4_railway_build)",
        "thin the attacking stack (hoi4_army_move)",
    ],
    "force_behind_plan": [
        "raise the training queues of the plan's templates (plan edit: training caps)",
        "move the operation's date later (plan edit: operation when)",
    ],
    "checkpoint_missed": ["revise the plan's later rows that assume the checkpoint (plan edit)",
                          "take the contingency the plan names for it"],
}


# the game's alert and the sitrep's own finding for the same thing: one family
FAMILY = {
    "free_military_factories": "military_factories_unassigned", "free_civilian_factories": "civilian_factories_idle",
    "free_dockyards": "dockyards_unassigned", "doctrines_available": "doctrine_affordable",
    "special_project_available": "special_project_idle", "insufficient_resources": "resource_deficit",
}


def family(code):
    return FAMILY.get(code, code)


def rule(code):
    base = code.split(":", 1)[0]
    return RULES.get(code) or RULES.get(base) or DEFAULT_RULE


def deadline_days(code, at_war):
    cls, lane, war_days, peace_days, why = rule(code)
    return (war_days if at_war else peace_days), why


def lane_of(code, tool=None):
    lane = rule(code)[1]
    if lane:
        return lane
    if tool:
        t = tool[5:] if tool.startswith("hoi4_") else tool
        for name, kinds in LANE_KINDS.items():
            if any(t.startswith(k) for k in kinds if k):
                return name
    return None


def class_of(code):
    return rule(code)[0]


# error codes that say the CALL was wrong (tool use), as against the game or the GUI failing
USE_CODES = ("invalid_arguments", "invalid_target", "unknown_", "use_", "owned_by_operation",
             "template_unknown", "region_not_found", "not_offered", "missing_argument")
# error codes that say a precondition did not hold (the same call can pass later)
PRECONDITION_CODES = ("events_open", "insufficient_", "no_free", "no_enabled", "operative_busy", "air_base_full",
                      "ai_would_refuse", "action_disabled", "event_covered")
NOOP_CODES = ("nothing_to_change", "already_")
TECH_CODES = ("input_readback_failed", "invalid_gui", "timeout", "call_failed", "gui_", "building_select_failed",
              "input_geometry", "action_not_applied", "action_diverged", "platform_", "watcher_unavailable")


def error_kind(code):
    c = str(code or "")
    if any(c.startswith(p) for p in NOOP_CODES):
        return "noop"
    if any(c.startswith(p) for p in USE_CODES):
        return "tool_use"
    if any(c.startswith(p) for p in PRECONDITION_CODES):
        return "precondition"
    return "technical"
