"""Sinks and slots: every spendable or assignable thing the snapshot can see, judged for "available
and unused" (owner, 23 Nov 2026: experience unspent, MIO and mastery idle, no decryption, free
manpower, too much political power: "definitely more issues i haven't listed").

Each finding names what is lost, what it waits for (the owning rule's own note, when the server's
omission rows carry one) and its lane; the deadline comes from issue_rules. Values come from the game
files through gamerules (experience caps, spirit slot costs, reward mastery, law costs), never from a
bare number: "the pool reaches its cap in 41 days at this gain", not "experience above 300".

Rows here only cover what the server's omission rows do not already name (the omission row wins).
`findings(snap, prev, facts)` -> list of finding dicts in the findings.F shape.
"""
from __future__ import annotations

from . import gamerules
from .snapshot import days_between, g


def F(code, severity, text, subject=None, **numbers):
    return {"code": code, "severity": severity, "text": text, "subject": subject,
            "owner": "sinks", "numbers": numbers}


def _items(snap):
    return {o.get("item") for o in snap.get("omissions") or [] if isinstance(o, dict)}


def _alerts(snap):
    return {a.get("alert") for a in snap.get("alerts") or [] if isinstance(a, dict) and a.get("alert")}


def _back(snap, prev, days):
    for p in reversed(prev or []):
        d = days_between(p.get("date"), snap.get("date"))
        if d is not None and d >= days - 0.5:
            return p, d
    return None, None


def _at_war(snap):
    return bool(g(snap, "wars", "at_war"))


# --- experience and doctrine mastery -------------------------------------------------------------


def experience(snap, prev, facts):
    """Each pool with experience the read shows something to buy with, or that grows toward its cap
    while nothing is listed to spend it: days to the cap at the observed gain."""
    out = []
    xp = g(snap, "doctrines", "experience", default={}) or {}
    alerts = _alerts(snap)
    back, span = _back(snap, prev, 14)
    folders = g(snap, "doctrines", "folders", default=[]) or []
    for pool in ("army", "navy", "air"):
        have = xp.get(pool)
        if have is None:
            continue
        cap = facts["xp_cap"].get(pool, 500.0)
        was = g(back, "doctrines", "experience", pool) if back else None
        gain = (have - was) / span if (was is not None and span) else None
        buys = []
        for f in folders:
            if f.get("xp_type") != pool:
                continue
            if not f.get("grand_doctrine") and f.get("grand_xp_cost") and have >= f["grand_xp_cost"]:
                buys.append(f"{f['folder']} grand doctrine ({f['grand_xp_cost']:.0f})")
            for t in f.get("open_tracks") or []:
                buys.append(f"{f['folder']}/{t} subdoctrine")
        spirit = min(facts["spirit_costs"].get(pool) or [20.0])
        if "modify_officer_corps" in alerts and have >= spirit:
            buys.append(f"an empty officer corps spirit slot (from {spirit:.0f})")
        days_to_cap = (cap - have) / gain if gain and gain > 0 else None
        spending = gain is not None and gain < 0
        if spending or (not buys and (days_to_cap is None or have < cap / 2)):
            continue
        trend = (f"+{gain * 30:.0f} a month, at the cap ({cap:.0f}, defines) in {days_to_cap:.0f} days: every "
                 f"day after it is lost" if days_to_cap is not None else "not growing")
        what = ("; it can buy " + ", ".join(buys[:4])) if buys else (
            "; nothing the read shows is open to buy: spirits of the other slots, designs and refits "
            "(equipment designer), template edits, or a completed track's next subdoctrine")
        sev = "high" if (days_to_cap is not None and days_to_cap <= 30) or have >= cap * 0.6 else "medium"
        out.append(F("experience_unspent", sev, f"{pool} experience {have:.0f} unspent ({trend}){what}",
                     subject=pool, experience=round(have, 1), cap=cap,
                     days_to_cap=None if days_to_cap is None else round(days_to_cap),
                     buys=buys[:6]))
    return out


def mastery(snap, facts):
    """A track whose mastery is past what its subdoctrine's rewards need reserves only part of what
    it gains (MASTERY_BANK_CONVERSION_RATE up to MASTERY_BANK_MAX); a track whose units never fight
    gains nothing, so the experience paid for its subdoctrine buys nothing."""
    out = []
    subs = facts.get("subdoctrines") or {}
    done, empty = [], []
    for f in g(snap, "doctrines", "folders", default=[]) or []:
        for t in f.get("tracks") or []:
            sub, m = t.get("subdoctrine"), t.get("mastery")
            if not sub or m is None:
                continue
            need = (subs.get(sub) or {}).get("mastery_to_complete")
            if need and m >= need:
                done.append(f"{f['folder']}/{t['track']} {sub} ({m:.0f} of {need:.0f})")
            elif m == 0:
                empty.append(f"{f['folder']}/{t['track']} {sub}")
    if done:
        out.append(F("doctrine_track_complete", "medium",
                     f"{len(done)} doctrine track(s) past their last reward: {'; '.join(done[:5])}; further mastery "
                     f"is reserved at {facts['bank_rate']:.0%} up to {facts['bank_max']:.0f}: a new subdoctrine on the "
                     f"track (its experience cost) starts its rewards with the reserve (what the switch keeps of the "
                     f"old rewards is a live check)", tracks=len(done)))
    if empty:
        out.append(F("doctrine_track_no_mastery", "low",
                     f"subdoctrine(s) with no mastery gained: {'; '.join(empty[:5])}: no unit of their kind fights or "
                     f"trains, so their rewards never come; field the arm or choose the subdoctrine of an arm in use",
                     tracks=len(empty)))
    return out


# --- manpower -------------------------------------------------------------------------------------


def manpower(snap, prev):
    """Free manpower that no training row takes: name what binds the force (equipment short, rows
    idle) rather than the law; the law question is the politics lane's (laws_affordable)."""
    pol = snap.get("politics") or {}
    free = pol.get("manpower")
    if not free:
        return []
    templates = snap.get("templates") or []
    if not templates or any(t.get("training") for t in templates):
        return []
    divs = g(snap, "land", "division_count") or 0
    if not divs:
        return []
    stock = g(snap, "production", "stockpile", default=[]) or []
    # equipment a new division needs that the field already lacks more of than the stock holds
    binding = [f"{r['archetype']} (stock {r.get('stockpile', 0):.0f}, field short {r['fielded_short']:.0f})"
               for r in stock if (r.get("fielded_short") or 0) > (r.get("stockpile") or 0)]
    law = (pol.get("laws") or {}).get("conscription")
    if binding:
        why = "equipment binds: " + ", ".join(binding[:4]) + ": raise its output"
    else:
        why = ("no equipment binds (every archetype's stock covers what the field lacks): the training rows "
               "(their caps, `when` and stock gates) or the training lane hold the men back")
    return [F("manpower_idle", "medium",
              f"{free:,.0f} free manpower and no division in training ({divs} fielded, law {law}); {why}; a "
              f"lower conscription law only removes its own modifiers (laws_affordable), it trains no one",
              free=free, divisions=divs, law=law, equipment_binding=len(binding))]


# --- navy -----------------------------------------------------------------------------------------


def navy(snap):
    out = []
    at_war = _at_war(snap)
    training, reserve = [], 0
    for f in g(snap, "navy", "fleets", default=[]) or []:
        for tf in f.get("task_forces") or []:
            if tf.get("mission") == "training" and at_war:
                training.append(f"{tf.get('name')} ({tf.get('ships')} ships, {f.get('fleet', '').strip()})")
            if tf.get("mission") == "reserve":
                reserve += tf.get("ships") or 0
    if training:
        out.append(F("fleet_training_at_war", "high",
                     f"at war, {len(training)} task force(s) on training: {'; '.join(training[:4])}: exercise brings "
                     f"crews to the training cap and costs fuel; raiding, patrol or escort is what the war asks "
                     f"(the plan's war mission and its sea region)", task_forces=len(training)))
    if reserve:
        out.append(F("reserve_fleet_ships", "medium",
                     f"{reserve} ship(s) in a reserve fleet: a reserve fleet takes no admiral and no mission; give "
                     f"the ships a fleet by their role (submarines to the raiders, screens to escort or patrol)",
                     ships=reserve))
    return out


# --- intelligence, design companies, special projects --------------------------------------------


def intelligence(snap):
    out = []
    intel = snap.get("intelligence") or {}
    ops = g(intel, "agency", "operatives", "list", default=None) or g(intel, "operatives", "list", default=[]) or []
    home = [o for o in ops if o.get("mission_target") == o.get("nationalities", [None])[0] and not o.get("operation")]
    if ops and len(home) == len(ops) and _at_war(snap):
        out.append(F("operatives_home_only", "medium",
                     f"all {len(ops)} operatives at home ({', '.join(sorted({str(o.get('mission')) for o in home}))}) "
                     f"while at war: no network abroad, no operation, no decryption support", operatives=len(ops)))
    if intel and intel.get("decryption") is None and _at_war(snap):
        out.append(F("decryption_unread", "medium",
                     "decryption and encryption (cipher progress per enemy) are not read: whether the agency decrypts "
                     "any enemy is unknown; the read and a decryption tool are the intelligence lane's gap",
                     target=None))
    elif isinstance(intel.get("decryption"), dict) and _at_war(snap) and not intel["decryption"].get("target"):
        out.append(F("decryption_idle", "high", "at war and the agency decrypts no enemy: cipher bonuses unused"))
    return out


def mios(snap):
    m = snap.get("mios") or {}
    if m.get("listed") and m.get("state_read") is False:
        return [F("mio_state_unread", "medium",
                  f"{m['listed']} design companies listed but their state (traits held, points, policy, line) is "
                  f"not readable: trait and policy orders cannot be checked and are re-sent (the MIO read is the "
                  f"cause to fix)", listed=m["listed"])]
    return []


def special_projects(snap):
    sp = snap.get("special_projects") or {}
    out = []
    for o in snap.get("omissions") or []:
        note = str(o.get("owner_note") or "")
        if o.get("item") == "special_project_available" and "no facility of their kind" in note:
            out.append(F("special_project_kind_without_facility", "low",
                         f"open special projects with no facility of their kind ({note.split('kind:', 1)[-1][:120].strip()}): "
                         f"the plan decides whether a facility is worth its cost; say so (special_projects.skip_kinds "
                         f"with why) or build one", free_scientists=sp.get("scientists_free")))
    return out


# --- political power and laws ---------------------------------------------------------------------


def laws(snap, prev, facts_dir=None, idle_men=False):
    """The conscription question (owner, 23 Nov 2026: "lower the law then, it gives some bonuses"):
    with men idle, a lower mobilisation law is worth its cost only by the penalties of the law in
    force that it drops (the files' modifiers: training time, factory and dockyard output,
    construction); a lower law itself adds nothing. Named only when such a penalty exists."""
    pol = snap.get("politics") or {}
    if pol.get("political_power") is None or not idle_men:
        return []
    f = gamerules.facts(facts_dir)
    ladder = f.get("laws", {}).get("mobilization_laws") or []
    current = (pol.get("laws") or {}).get("conscription")
    cur = next((x for x in ladder if x["law"] == current), None)
    if not cur:
        return []
    penalties = {k: v for k, v in cur["modifiers"].items() if k != "conscription" and v}
    if not penalties:
        return []
    rows = gamerules.laws_affordable(pol, _at_war(snap), facts_dir)
    lower = [r for r in rows if r["group"] == "conscription" and r["available"] is not False
             and r["modifiers"].get("conscription", 0) < cur["modifiers"].get("conscription", 0)
             and r["modifiers"].get("conscription", 0) > 0]
    if not lower:
        return []
    best = max(lower, key=lambda r: r["modifiers"].get("conscription", 0))
    drop = ", ".join(f"{k} {v:+g}" for k, v in penalties.items() if best["modifiers"].get(k, 0) != v)
    if not drop:
        return []
    return [F("conscription_above_need", "medium",
              f"{current} costs {drop} (files) while men sit idle; {best['law']} ({best['cost']:.0f} political "
              f"power) recruits {best['modifiers'].get('conscription', 0):.1%} of the population instead of "
              f"{cur['modifiers'].get('conscription', 0):.1%} and drops those penalties; worth it only if the "
              f"force's need (the plan's divisions, losses, garrisons) stays under the lower ceiling",
              law=current, lower=best["law"])]


# --- lanes that wait --------------------------------------------------------------------------------

WAIT_WORDS = ("wait", "saving", "saved for", "held for", "until", "not yet", "no trait to queue",
              "every planned", "rests")


def waits(snap, prev):
    """An omission row whose owning rule has said the same thing for longer than the row's own
    deadline: the wait outlived its reason (or never had one). Read from the row's days open and its
    note; the monitor raises it against the same lane."""
    from . import issue_rules as R
    out = []
    at_war = _at_war(snap)
    for o in snap.get("omissions") or []:
        if not isinstance(o, dict) or not o.get("item") or str(o["item"]).startswith(("frame_", "game_alert")):
            continue  # an alert frame no label names has no lane to answer for it
        if o.get("owner") in (None, "player") or str(o.get("owner", "")).startswith("player"):
            continue
        days = o.get("days_open") or 0
        note = str(o.get("owner_note") or "")
        limit, _ = R.deadline_days(o["item"], at_war)
        if days <= max(limit, 1):
            continue
        said = note[:160] if note else "no reason given"
        out.append(F("lane_wait_outlived", "high",
                     f"{o['item']} open {days} days, past its {limit}-day deadline; lane {o.get('owner')} says: {said}",
                     subject=o["item"], days_open=days, owner=o.get("owner"), note=note[:300]))
    return out


def findings(snap, prev=None, game_dir=None):
    prev = list(prev or [])
    facts = gamerules.facts(game_dir)
    have = _items(snap)
    out = []
    out += experience(snap, prev, facts)
    out += mastery(snap, facts)
    out += manpower(snap, prev)
    out += navy(snap)
    out += intelligence(snap)
    out += mios(snap)
    out += special_projects(snap)
    out += laws(snap, prev, game_dir, idle_men=any(x["code"] == "manpower_idle" for x in out))
    out += waits(snap, prev)
    return [f for f in out if f["code"] not in have]
