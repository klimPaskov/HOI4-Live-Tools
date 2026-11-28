"""What is wrong in a snapshot: findings with their numbers and their reason.

Each finding is a value judgement about the game state, never a bare-number
rule: it says what is lost and why it matters ("24 divisions draw 34 supply
where the province gives 12: their supply is 22%"). Thresholds that appear are
descriptions of the state ("near zero", "more than they lost"), not play rules.

Ownership. The server's executor publishes its own open omissions (things the
game marks actionable and a rule owns: `hoi4_observe now.omissions`). Those rows
are the one implementation of the "assignable and unassigned" checks they cover;
this module reports them as they come and only computes its own version of an
overlapping check when the snapshot carries no omission with that item (older
servers). Everything about force distribution, trends, wars, the air by theatre
and the executor's repeated refusals is owned here.

`findings(snap, prev)`: snap = the newest snapshot, prev = earlier snapshots of
the same run, oldest first. Returns a list sorted by severity.
"""
from __future__ import annotations

from collections import defaultdict

from .snapshot import days_between, g, num

SEVERITY = {"high": 0, "medium": 1, "low": 2}
LAND_AIR_MISSIONS = {"air_superiority", "cas", "interception", "interdiction",
                     "strategic_bombing", "attack_logistics", "paradrop"}


def F(code, severity, text, subject=None, owner="findings", **numbers):
    return {"code": code, "severity": severity, "text": text, "subject": subject,
            "owner": owner, "numbers": numbers}


def lookback(snap, prev, days):
    """The newest earlier snapshot at least `days` game days before `snap`."""
    for p in reversed(prev or []):
        d = days_between(p.get("date"), snap.get("date"))
        if d is not None and d >= days - 0.5:
            return p
    return None


def _omission_items(snap):
    items = set()
    for o in snap.get("omissions") or []:
        if isinstance(o, dict) and o.get("item"):
            items.add(o["item"])
    return items


def from_omissions(snap):
    out = []
    seen = {}
    for o in snap.get("omissions") or []:
        if not isinstance(o, dict):
            continue
        sig = (o.get("item"), o.get("todo"))
        if sig in seen:  # the same row for several alert icons
            seen[sig]["numbers"]["rows"] = seen[sig]["numbers"].get("rows", 1) + 1
            continue
        days = o.get("days_open")
        text = (o.get("todo") or o.get("item") or "").strip()
        if days:
            text += f" (open {days} days)"
        if o.get("owner"):
            text += f"; executor rule {o['owner']}"
        if o.get("owner_note"):
            text += f": {str(o['owner_note'])[:160]}"
        if o.get("note_contradicts_read"):
            text += " (the rule's note contradicts the read)"
        sev = "high" if (days or 0) >= 14 else "medium"
        if o.get("left_by_plan"):
            text += " (left by the plan)"
        out.append(F(o.get("item") or "omission", sev, text, owner="server omissions",
                     days_open=days, owner_rule=o.get("owner"),
                     owner_note=str(o["owner_note"])[:300] if o.get("owner_note") else None,
                     left_by_plan=bool(o.get("left_by_plan"))))
        seen[sig] = out[-1]
    return out


# --- assignable and unassigned (fallback when the server sends no omission) ----


def assignable(snap, prev):
    out = []
    have = _omission_items(snap)
    eco, prod, res = snap.get("economy") or {}, snap.get("production") or {}, snap.get("research") or {}
    idle = eco.get("civilian_idle")
    if idle and "civilian_factories_idle" not in have and "free_civilian_factories" not in have:
        out.append(F("civilian_factories_idle", "high",
                     f"{idle} civilian factories idle with {g(snap, 'construction', 'queued')} projects "
                     f"queued: an idle factory builds nothing, queue more projects (each takes at most "
                     f"15 factories)", idle=idle))
    mil = g(eco, "factories", "military")
    if mil and prod.get("lines"):
        free = mil - (prod.get("military_assigned") or 0)
        if free > 0 and "military_factories_unassigned" not in have:
            out.append(F("military_factories_unassigned", "high",
                         f"{free} of {mil} military factories are on no production line: their output is lost",
                         free=free, total=mil))
    nav = g(eco, "factories", "naval")
    if nav and prod.get("lines"):
        free = nav - (prod.get("naval_assigned") or 0)
        if free > 0 and "free_dockyards" not in have:
            out.append(F("dockyards_unassigned", "medium",
                         f"{free} of {nav} dockyards build nothing", free=free, total=nav))
    if res.get("idle_slots") and "research_slot_idle" not in have:
        out.append(F("research_slot_idle", "high",
                     f"{res['idle_slots']} research slot(s) idle: research time lost every day",
                     idle=res["idle_slots"]))
    leaders = g(snap, "leaders", "commanders", default=[])
    free_fm = [c["name"] for c in leaders if c.get("rank") == "field_marshal" and not c.get("assigned_to")]
    free_any = [c["name"] for c in leaders if not c.get("assigned_to") and c.get("recruited") is not False]
    for grp in g(snap, "land", "groups", default=[]):
        if grp.get("field_marshal") or "army_group_without_field_marshal" in have:
            continue
        divs = sum(a.get("divisions") or 0 for a in g(snap, "land", "armies", default=[])
                   if a.get("group") == grp.get("group"))
        div_orders = sum(o.get("divisions") or 0 for o in grp.get("orders") or [])
        if not (divs or div_orders):
            continue
        if free_fm:
            why = f"field marshal(s) {', '.join(free_fm[:3])} lead nothing"
        elif free_any:
            why = (f"{len(free_any)} leaders lead nothing (rank not readable: promote the best one "
                   f"if no field marshal is free)")
        else:
            why = "no free leader is known"
        out.append(F("army_group_without_field_marshal", "high",
                     f"army group {grp.get('group')} ({max(divs, div_orders)} divisions) has no field "
                     f"marshal: its armies get no marshal bonus and he gains no experience; {why}",
                     subject=grp.get("group"), divisions=max(divs, div_orders), free_field_marshals=free_fm))
    if "army_without_general" not in have:
        for a in g(snap, "land", "armies", default=[]):
            if not a.get("commander") and (a.get("divisions") or 0) > 0:
                out.append(F("army_without_general", "medium",
                             f"army {a['army']} ({a['divisions']} divisions) has no general: no leader "
                             f"skills, no experience gained", subject=a["army"]))
    out.extend(political_power(snap, prev, have))
    sp = snap.get("special_projects") or {}
    idle_fac = [f for f in sp.get("facilities") or [] if not f.get("project")]
    if idle_fac and "special_project_idle" not in have:
        out.append(F("special_project_idle", "medium",
                     f"{len(idle_fac)} special-project facilit(y/ies) run no project "
                     f"({sp.get('scientists_free', 0)} scientists free)", facilities=len(idle_fac)))
    return out


def political_power(snap, prev, have):
    pol = snap.get("politics") or {}
    pp = pol.get("political_power")
    if pp is None or "political_power_unspent" in have:
        return []
    alerts = {a.get("alert") for a in snap.get("alerts") or [] if a.get("alert")}
    # the game's decisions_available alert lights while any decision is enabled, wanted or not:
    # no sign of idle power (the executor's own row judges the decisions by value)
    marked = sorted(alerts & {"modify_government"})
    affordable = [a for a in pol.get("affordable") or [] if (a.get("cost") or 0) <= pp]
    if not marked and not affordable:
        return []
    back = lookback(snap, prev, 14)
    was = g(back, "politics", "political_power") if back is not None else None
    gain = None if was is None else pp - was
    if gain is not None and gain <= 0:
        return []  # being spent
    saving = pol.get("saving_for")
    if saving and (saving.get("cost") or 0) > pp:
        return []  # a costlier pick is being saved for
    what = (", ".join(f"{a.get('name')} ({a.get('cost')})" for a in affordable[:3]) if affordable
            else "the game marks " + " and ".join(marked))
    trend = f", +{gain:.0f} in 14 days" if gain else ""
    return [F("political_power_unspent", "medium",
              f"political power {pp:.0f} banked{trend} while something is affordable ({what}) and no "
              f"costlier pick is being saved for: unspent power buys nothing",
              political_power=pp, gain_14d=gain)]


# The doctrine finding is the game's own: its doctrines_available alert (issue doctrine_affordable)
# and the executor's omission row. A price compared here would be the files' base cost, before the
# modifiers the game applies (owner, 27 Nov 2026: no cost judged in code).


# --- force distribution --------------------------------------------------------


# The game's base command limits (common/defines CORPS_COMMANDER_DIVISIONS_CAP and
# FIELD_MARSHAL_ARMIES_CAP), used only when the roster read gives no limit of its own.
ARMY_DIVISIONS_BASE = 24
GROUP_ARMIES_BASE = 5



def _idle_arrows(out, name, order):
    """An arrow that holds divisions and does not execute while the war is on (its front may read
    executing): those divisions stand still. t1939n (27 Nov 2026): Army Group 2's front read
    executing from W+13 to W+20 while its 37 divisions stood on unexecuted arrows."""
    idle = [x for x in order.get("offensives") or []
            if not x.get("executing") and (x.get("divisions") is None or x.get("divisions") > 0)]
    if idle and order.get("executing"):
        held = sum(x.get("divisions") or 0 for x in idle)
        out.append(F("arrow_not_executing", "high",
                     f"{name}: {len(idle)} arrow(s) holding {held} divisions do not execute on a front that "
                     f"does (targets {[x.get('target_states') for x in idle][:3]}): those divisions stand still",
                     subject=name, divisions=held))

HOLD_KINDS = ("area_defense", "fallback")


def _idle_at_war(out, snap, group_orders):
    """At war: the armies standing only on holds (area defence, fallback line) outside any group
    front, named with their divisions at every gate, with the loans the executor has made (w96a,
    w96c, w96d: 40-58 divisions on area defences at home through the whole West)."""
    lent = {}
    for op, o in (g(snap, "executor", "operations", default={}) or {}).items():
        for army in o.get("lent") or []:
            lent[army] = op
    rows = []
    for a in g(snap, "land", "armies", default=[]):
        n = a.get("divisions") or 0
        orders = a.get("orders") or []
        if not n or not orders:
            continue
        if any(o.get("offensives") or o.get("executing") for o in orders):
            continue
        if not all(o.get("kind") in HOLD_KINDS for o in orders):
            continue
        if a.get("group") and any(o.get("kind") == "front" for o in group_orders.get(a.get("group")) or []):
            continue  # army_outside_group_front names it
        rows.append((n, a["army"]))
    if not rows:
        return
    rows.sort(key=lambda r: (-r[0], r[1]))
    total = sum(n for n, _ in rows)
    named = ", ".join(f"{name} ({n}{', lent to ' + lent[name] if name in lent else ''})" for n, name in rows)
    out.append(F("armies_idle_at_war", "high" if total >= 10 else "medium",
                 f"{total} divisions in {len(rows)} army(ies) stand on holds away from any attack at war: {named}; "
                 f"each should be lent to the operation whose front has room, or kept as a named reserve or "
                 f"garrison with its reason (the executor's lend note)",
                 divisions=total, armies=[name for _, name in rows], lent=sorted(lent)))


def land(snap, prev):
    out = []
    for grp in g(snap, "land", "groups", default=[]):
        armies = grp.get("armies") or []
        cap = grp.get("army_limit") or GROUP_ARMIES_BASE
        if len(armies) > cap:
            out.append(F("group_over_limit", "medium",
                         f"army group {grp.get('group')} holds {len(armies)} armies, over its field marshal's "
                         f"limit of {cap}: the armies past it lower his bonuses; form a second group under a "
                         f"free marshal", subject=grp.get("group"), armies=len(armies), limit=cap))
    at_war = g(snap, "wars", "at_war")
    controlled = set(g(snap, "wars", "states_controlled", default=[]))
    divs = g(snap, "land", "divisions", default=[])
    by_prov = defaultdict(list)
    for d in divs:
        by_prov[d.get("province")].append(d)
    group_orders = {grp.get("group"): grp.get("orders") or [] for grp in g(snap, "land", "groups", default=[])}
    for a in g(snap, "land", "armies", default=[]):
        n = a.get("divisions") or 0
        if not n:
            continue
        cap = a.get("division_limit") or ARMY_DIVISIONS_BASE
        if n > cap:
            out.append(F("army_over_limit", "medium",
                         f"army {a['army']} holds {n} divisions, over its commander's limit of {cap}: every "
                         f"division past it lowers the army's leadership bonuses; the excess belongs in another "
                         f"army of the same role with room or a new army under a free general",
                         subject=a["army"], divisions=n, limit=cap))
        orders = a.get("orders") or []
        if not orders and not a.get("executing") and not group_orders.get(a.get("group")):
            top = sorted((a.get("states") or {}).items(), key=lambda x: -x[1])[:3]
            where = ", ".join(f"state {s}: {c}" for s, c in top)
            out.append(F("army_without_order", "high" if at_war else "low",
                         f"army {a['army']} ({n} divisions, at {where}) has no front, offensive or hold "
                         f"order{': at war it fights only where the AI of each division wanders' if at_war else ''}",
                         subject=a["army"], divisions=n))
            continue
        pl = a.get("planning") or {}
        for o in orders:
            if o.get("kind") == "front" and not o.get("divisions"):
                out.append(F("front_without_divisions", "high",
                             f"army {a['army']} has a front on states {o.get('states')} with no divisions "
                             f"assigned", subject=a["army"]))
            for off in o.get("offensives") or []:
                tgt = set(off.get("target_states") or [])
                if tgt and controlled and tgt <= controlled:
                    out.append(F("offensive_at_own_state", "high",
                                 f"army {a['army']}'s offensive arrow points at state(s) {sorted(tgt)}, which we "
                                 f"already control: it attacks nothing", subject=a["army"]))
                elif off.get("path_states") and o.get("states") and \
                        not set(off["path_states"]) & set(o["states"]) and not tgt & set(o["states"]):
                    out.append(F("offensive_off_line", "medium",
                                 f"army {a['army']}'s offensive (through {off['path_states']}) does not start "
                                 f"from its front line {o['states']}", subject=a["army"]))
                if not off.get("executing") and pl.get("mean") is not None and pl["mean"] < 0.05:
                    out.append(F("planning_near_zero", "high" if at_war else "medium",
                                 f"army {a['army']} has an offensive ready but its planning bonus is "
                                 f"{pl['mean']:.2f} (best division {pl.get('max')}): an attack now gets "
                                 f"nothing from planning; let it plan on its line first", subject=a["army"],
                                 planning=pl.get("mean")))
            if at_war and o.get("offensives") and not any(x.get("executing") for x in o["offensives"]) \
                    and not o.get("executing"):
                out.append(F("order_not_executing", "medium",
                             f"army {a['army']}'s battle plan on {o.get('states')} is not executing",
                             subject=a["army"]))
            elif at_war:
                _idle_arrows(out, a["army"], o)
        # an army with an order of its own inside a group whose front is drawn takes no part in it
        # (t1939n, 27 Nov 2026: Armee Süd, 15 divisions, on an area defence in Army Group 2)
        if at_war and orders and a.get("group") and group_orders.get(a.get("group")) and \
                not any(o.get("offensives") for o in orders) and \
                any(o.get("kind") == "front" for o in group_orders[a["group"]]):
            kinds = sorted({str(o.get("kind")) for o in orders})
            out.append(F("army_outside_group_front", "high",
                         f"army {a['army']} ({n} divisions) stands in {a['group']} with an order of its own "
                         f"({', '.join(kinds)}) while the group has a front: its divisions take no part in the "
                         f"group's attack", subject=a["army"], divisions=n))
    if at_war:
        _idle_at_war(out, snap, group_orders)
        for grp in g(snap, "land", "groups", default=[]):
            for o in grp.get("orders") or []:
                _idle_arrows(out, grp.get("group"), o)
    # stacking: provinces whose divisions draw more than the province gives
    rows = []
    for p in g(snap, "stacking", "provinces", default=[]):
        n = p.get("divisions") or 0
        head, sup, dem = p.get("headroom"), p.get("supply"), p.get("demand")
        if n >= 2 and head is not None and head < 0 and sup is not None:
            here = [d.get("supply") for d in by_prov.get(p.get("province")) or [] if d.get("supply") is not None]
            mean = sum(here) / len(here) if here else None
            rows.append((head, p, n, sup, dem, mean))
    rows.sort(key=lambda r: r[0])
    for head, p, n, sup, dem, mean in rows[:4]:
        carry = int(n * sup / dem) if dem else None
        sup_txt = f": their supply is {mean:.0f}%" if mean is not None else ""
        out.append(F("province_overstacked", "high" if (mean or 100) < 50 else "medium",
                     f"province {p['province']} (state {p.get('state')}): {n} divisions draw {dem:.1f} supply "
                     f"where the province gives {sup:.1f}{sup_txt}; it carries about {carry} at full supply: "
                     f"spread the rest along the line or into the second line",
                     subject=str(p["province"]), divisions=n, supply=sup, demand=dem, division_supply=mean))
    un = g(snap, "land", "unassigned") or 0
    if at_war and un:
        out.append(F("divisions_unassigned", "medium",
                     f"{un} divisions belong to no army: they take no orders", divisions=un))
    return out


def air(snap, prev, state_regions=None):
    out = []
    a = snap.get("air") or {}
    # transport wings fly only a paradrop or an air supply, both need a target the plan names:
    # standing by is their normal state, not idling
    transport = {x.get("wing") for x in a.get("wings") or []
                 if "transport" in str(x.get("airframe") or x.get("type") or "")}
    w = [n for n in a.get("wings_without_mission") or [] if n not in transport]
    if w:
        out.append(F("air_wings_without_mission", "medium",
                     f"{len(w)} air wing(s) have no mission ({', '.join(w[:3])}): planes on the ground do "
                     f"nothing", wings=len(w)))
    if not g(snap, "wars", "at_war"):
        return out
    state_regions = state_regions or {}
    front = set()
    for army in g(snap, "land", "armies", default=[]):
        for o in army.get("orders") or []:
            front.update(o.get("states") or [])
            for off in o.get("offensives") or []:
                front.update(off.get("target_states") or [])
    for b in g(snap, "wars", "borders", default=[]):
        if b.get("hostile"):
            front.update(b.get("their_states") or [])
    front_regions = {str(state_regions[s]) for s in front if s in state_regions}
    if not front_regions:
        return out
    total = a.get("aircraft_total") or sum(r.get("aircraft") or 0 for r in (a.get("regions") or {}).values())
    for rid, r in (a.get("regions") or {}).items():
        combat = sum(v for m, v in (r.get("missions") or {}).items() if m in LAND_AIR_MISSIONS)
        if not combat or rid in front_regions:
            continue
        weeks = 0
        for p in reversed(prev or []):
            pr = g(p, "air", "regions", rid) or {}
            if not pr.get("aircraft"):
                break
            weeks += 1
        share = f" ({100 * combat / total:.0f}% of the air force)" if total else ""
        dur = f", for {weeks} snapshot(s) before this one" if weeks else ""
        names = sorted(front_regions)
        out.append(F("air_away_from_front", "high",
                     f"{combat} planes{share} fly land missions over {r.get('name') or 'region ' + rid}, "
                     f"where no front of ours lies{dur}; the fronts are in region(s) {', '.join(names)}",
                     subject=rid, aircraft=combat, snapshots=weeks))
    return out


# --- wars and trends ----------------------------------------------------------


def wars(snap, prev):
    out = []
    w = snap.get("wars") or {}
    for s in w.get("own_states_lost") or []:
        out.append(F("own_state_enemy_held", "high",
                     f"own state {s.get('name') or s.get('state')} is held by {s.get('controller')}",
                     subject=str(s.get("state"))))
    for t in w.get("threatened") or []:
        if t.get("defended") is False:
            out.append(F("threatened_state_undefended", "high",
                         f"state {t.get('name') or t.get('state')} borders {t.get('hostile')} and no division "
                         f"of ours stands in it", subject=str(t.get("state"))))
    back = lookback(snap, prev, 14)
    for tag, e in (w.get("per_enemy") or {}).items():
        if not e.get("at_war"):
            continue
        if back is not None:
            b = g(back, "wars", "per_enemy", tag) or {}
            if b.get("at_war"):
                lost = (e.get("casualties_own") or 0) - (b.get("casualties_own") or 0)
                dealt = (e.get("casualties_enemy") or 0) - (b.get("casualties_enemy") or 0)
                # below a thousand men in two weeks it is skirmish noise, not an exchange
                if lost > dealt and lost >= 1000:
                    out.append(F("losing_exchange", "high",
                                 f"against {tag} in the last 14 days we lost {lost:,} men and they lost "
                                 f"{dealt:,} (1:{dealt / lost:.2f}): we pay more than we take",
                                 subject=tag, lost=lost, dealt=dealt))
                held, was = e.get("vp_held_by_us"), b.get("vp_held_by_us")
                total = e.get("vp_total")
                fighting = lost > 0 or dealt > 0
                if fighting and held is not None and was is not None and held <= was and total and held < total:
                    out.append(F("victory_points_flat", "medium",
                                 f"victory points of {tag} we hold: {held:.0f}/{total:.0f}, no gain in 14 days",
                                 subject=tag, held=held, total=total))
    return out


def executor(snap):
    out = []
    ex = snap.get("executor") or {}
    hour = ex.get("game_hour")
    for f in ex.get("failures") or []:
        if f.get("count", 0) < 3:
            continue
        recent = hour is None or f.get("last_hour") is None or hour - f["last_hour"] <= 7 * 24
        if recent:
            out.append(F("executor_repeated_refusal", "medium",
                         f"the executor has tried `{f['what'][:120]}` {f['count']} times and was refused each "
                         f"time; it keeps waiting on the same refusal", subject=f["what"][:80],
                         count=f["count"]))
    for b in ex.get("blocked") or []:
        if (b.get("tries") or 0) >= 3:
            out.append(F("plan_item_blocked", "low",
                         f"plan item {b['item']} blocked after {b['tries']} tries {b.get('why') or ''}".strip(),
                         subject=b["item"]))
    return out


def supply_and_stock(snap):
    out = []
    for e in g(snap, "production", "stockpile", default=[]):
        short = e.get("fielded_short") or 0
        net = e.get("net_per_day")
        made = e.get("produced_per_day") or 0
        if short > 0 and (net or 0) <= 0 and not e.get("stockpile"):
            # what the lines make, not an unread net shown as zero (t1939n, 27 Nov 2026: "+0.0/day"
            # for support equipment whose line of 3 factories made 4.25 a day)
            if made > 0:
                how = (f"its {e.get('production_lines') or 'its'} line(s) make {made:.1f}/day "
                       f"({short / made:.0f} days to cover the shortfall alone"
                       + (f", net {net:+.1f}/day after losses" if net is not None else "") + ")")
                fix = "raise its line if those days are too long for the war"
            else:
                how = "no line makes it"
                fix = "add a line"
            out.append(F("equipment_short", "high" if made <= 0 else "medium",
                         f"divisions lack {short:.0f} {e['archetype']} with an empty stockpile; {how}: {fix}",
                         subject=e["archetype"], short=short, produced_per_day=made))
        elif e.get("depleting") and e.get("days_of_cover") is not None and e["days_of_cover"] < 30:
            out.append(F("equipment_depleting", "medium",
                         f"{e['archetype']} stockpile runs out in {e['days_of_cover']:.0f} days at "
                         f"{e.get('net_per_day'):+.1f}/day", subject=e["archetype"]))
    rs = g(snap, "production", "resource_shortages", default=[])
    if rs:
        kinds = sorted({r for x in rs for r in x.get("short") or []})
        out.append(F("resource_deficit", "medium",
                     f"{len(rs)} production line(s) lack {', '.join(kinds)}: they run below efficiency "
                     f"(trade or build for it)", lines=len(rs)))
    cv = g(snap, "navy", "convoys", default={}) or {}
    if (cv.get("needed") or 0) > (cv.get("total") or 0) > 0 or (cv.get("free") == 0 and cv.get("needed")):
        out.append(F("convoys_short", "medium",
                     f"convoys needed {cv.get('needed')}, in stock {cv.get('total')}, free {cv.get('free')}"))
    idle_tf = [t["name"] for f in g(snap, "navy", "fleets", default=[]) for t in f.get("task_forces") or []
               if t.get("mission") in (None, "hold")]
    if idle_tf and "naval_task_force_idle" not in _omission_items(snap):
        out.append(F("naval_task_force_idle", "low",
                     f"{len(idle_tf)} naval task force(s) on hold with no mission", task_forces=len(idle_tf)))
    return out


def affairs(snap):
    """Espionage and occupation, from the pure reads the snapshot keeps (no judgement on numbers the game
    does not show: a network abroad either exists or not; resistance is compared with the game's own target)."""
    out = []
    it = snap.get("intelligence") or {}
    ops = it.get("operatives")
    roster = ops.get("list") if isinstance(ops, dict) else None
    if it.get("created") and roster and not it.get("networks"):
        home = [o for o in roster if o.get("mission") and o.get("mission_target") == snap.get("player")]
        if len(home) == len(roster):
            out.append(F("intel_no_network_abroad", "medium",
                         f"the agency has {len(roster)} operative(s), all on missions at home "
                         f"({', '.join(sorted({o['mission'] for o in home}))}) and no intelligence network "
                         f"abroad: no operation (collaboration government, coup, cipher) can start without one",
                         operatives=len(roster)))
    occ = snap.get("occupation") or {}
    over = [s for s in occ.get("states") or []
            if s.get("resistance") is not None and s.get("resistance_target") is not None
            and s["resistance"] > s["resistance_target"]]
    if over:
        names = ", ".join(f"{s.get('name') or s.get('state')} {s['resistance']:.0f}/{s['resistance_target']:.0f}"
                          for s in sorted(over, key=lambda s: -s["resistance"])[:4])
        out.append(F("resistance_above_target", "medium",
                     f"resistance above its target in {len(over)} occupied state(s) ({names}): garrisons or "
                     f"the occupation law do not hold it (law {g(occ, 'law', 'key') or occ.get('law')})",
                     states=len(over)))
    return out


def game_flags(snap, already):
    out = []
    seen = {f["code"] for f in already}
    for a in snap.get("alerts") or []:
        name = a.get("alert")
        if not name or name in ("at_war",) or name in seen:
            continue
        out.append(F(f"game_alert:{name}", "low", f"the game flags {name}: {a.get('todo') or ''}".strip(),
                     subject=name, owner="game alert row"))
        seen.add(name)
    # Alert icons no tool names (a mod's own alerts, a new DLC's): something no typed tool manages.
    unknown = sorted({a.get("frame") for a in snap.get("alerts") or [] if not a.get("alert") and a.get("frame")
                      is not None}, key=str)
    if unknown and "game_alert_unknown" not in seen:
        out.append(F("game_alert_unknown", "low",
                     "alert icons no tool names (frames %s): hover one for its text (hoi4_ui_act action hover), "
                     "inspect the panel it opens (hoi4_ui_inspect) and learn what it wants (hoi4_skill_save)"
                     % ", ".join(str(f) for f in unknown[:6]), owner="game alert row", frames=len(unknown)))
    if snap.get("events_open"):
        out.append(F("events_open", "low", f"open events: {', '.join(snap['events_open'][:4])}"))
    if snap.get("peace_conference_open"):
        out.append(F("peace_conference_open", "high", "a peace conference is open"))
    return out


def battles(snap):
    """What the battle read shows about our own fights (hoi4_battles)."""
    out = []
    b = snap.get("battles") or {}
    if not b.get("readable"):
        return out
    for x in b.get("list") or []:
        where = f"province {x['province']} ({x.get('terrain') or '?'}, state {x.get('state')})"
        enemy = "/".join(x.get("enemy") or []) or "the enemy"
        if x.get("we") == "attacker" and (x.get("our_reserve") or 0) >= 3:
            out.append(F("battle_reserve_waiting", "high",
                         f"the attack on {where} has {x['our_reserve']} of our divisions in reserve and "
                         f"{x.get('our_engaged')} in the line: width {x.get('width_engaged')} of "
                         f"{x.get('width_max')} ({x.get('directions')} direction(s), terrain width "
                         f"{x.get('width_terrain')}); the waiting divisions add nothing here, another "
                         f"province or direction would use them", subject=x["province"],
                         reserve=x["our_reserve"], engaged=x.get("our_engaged")))
        for line in x.get("lines") or []:
            if line.get("line") == "river" and x.get("we") == "attacker":
                worst = min((c.get("value") or 0) for c in line.get("crossings") or [{}])
                out.append(F("battle_across_river", "medium",
                             f"attacking {where} across a river at {worst * 100:+.0f}% "
                             f"({line.get('divisions_crossing')} divisions crossing)", subject=x["province"],
                             penalty=worst))
        if x.get("winning") and x.get("winning") not in (x.get("we"), "even", "unknown"):
            out.append(F("battle_being_lost", "high" if x.get("we") == "defender" else "medium",
                         f"{enemy} is winning the battle at {where}: our side has {x.get('our_hours_left')} h "
                         f"of organisation left at its loss rate, theirs {x.get('their_hours_left')} h "
                         f"(our org {x.get('our_org')} vs {x.get('their_org')}, {x.get('our_engaged')} vs "
                         f"{x.get('their_engaged')} in the line, {x.get('hours')} h in)", subject=x["province"]))
        if x.get("we") == "attacker" and x.get("our_org") is not None and x["our_org"] < 20:
            out.append(F("battle_low_organisation", "medium",
                         f"our attack on {where} fights at organisation {x['our_org']} "
                         f"({', '.join(x.get('our_templates') or []) or '?'})", subject=x["province"]))
    return out


def findings(snap, prev=None, state_regions=None):
    prev = prev or []
    out = from_omissions(snap)
    out += assignable(snap, prev)
    out += land(snap, prev)
    out += air(snap, prev, state_regions)
    out += wars(snap, prev)
    out += battles(snap)
    out += supply_and_stock(snap)
    out += affairs(snap)
    out += executor(snap)
    out += game_flags(snap, out)
    # how long each finding has been open: consecutive earlier snapshots with the same code+subject
    for f in out:
        n = 0
        for p in reversed(prev):
            if any(x.get("code") == f["code"] and x.get("subject") == f["subject"]
                   for x in p.get("findings") or []):
                n += 1
            else:
                break
        f["open_snapshots"] = n
    out.sort(key=lambda f: (SEVERITY.get(f["severity"], 3), -f["open_snapshots"]))
    return out
