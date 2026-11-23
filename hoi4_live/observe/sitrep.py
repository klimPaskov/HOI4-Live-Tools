"""The situation report (plain text) and the diff between two snapshots."""
from __future__ import annotations

from .findings import lookback
from .reads import GAPS
from .snapshot import g


def pct(v):
    if v is None:
        return "?"
    return f"{v * 100:.0f}%" if v <= 1.0 else f"{v:.0f}%"


def n(v, fmt="{:.0f}"):
    return "?" if v is None else fmt.format(v)


def _order_text(o):
    s = f"{o.get('kind')} st{o.get('states')} d{o.get('divisions')}"
    if o.get("most_in_one_province"):
        s += f" max{o['most_in_one_province']}/prov"
    s += " EX" if o.get("executing") else ""
    for off in o.get("offensives") or []:
        s += f" ->{off.get('target_states')}{' EX' if off.get('executing') else ''}"
    return s


def render(snap, prev=None, run=None, findings=None):
    prev = prev or []
    fs = findings if findings is not None else snap.get("findings") or []
    L = []
    state = "paused" if snap.get("paused") else f"speed {snap.get('speed')}"
    L.append(f"SITREP {run or ''} {snap.get('date')} {snap.get('country') or snap.get('player')} ({state}); "
             f"snapshot trigger {snap.get('trigger')}")
    L.append("")
    L.append(f"WHAT IS WRONG ({len(fs)})")
    if not fs:
        L.append("  nothing found in what is readable (see NOT READABLE below)")
    for f in fs:
        open_ = f" [open {f['open_snapshots']} snapshots]" if f.get("open_snapshots") else ""
        owner = f" ({f['owner']})" if f.get("owner") and f["owner"] != "findings" else ""
        L.append(f"  {f['severity'].upper():6} {f['text']}{open_}{owner}")
    L.append("")
    eco, pol, prod = snap.get("economy") or {}, snap.get("politics") or {}, snap.get("production") or {}
    fac = eco.get("factories") or {}
    L.append("ECONOMY")
    L.append(f"  civilian {n(fac.get('civilian'))} ({n(eco.get('civilian_on_construction'))} building, "
             f"{n(eco.get('civilian_idle'))} idle, {n(g(snap, 'construction', 'queued'))} projects queued, "
             f"consumer goods {n(eco.get('consumer_goods'))}); military {n(fac.get('military'))} "
             f"({n(prod.get('military_assigned'))} on lines); dockyards {n(fac.get('naval'))} "
             f"({n(prod.get('naval_assigned'))} on lines)")
    L.append(f"  political power {n(pol.get('political_power'))}, stability {pct(pol.get('stability'))}, "
             f"war support {pct(pol.get('war_support'))}, manpower {n(pol.get('manpower'), '{:,.0f}')}, "
             f"command power {n(pol.get('command_power'))}; laws {pol.get('laws') or '?'}")
    short = [f"{k} {v['balance']:+.0f}" for k, v in (eco.get("resources") or {}).items()
             if v.get("balance") is not None and v["balance"] < 0]
    fuel = eco.get("fuel") or {}
    L.append(f"  resources short: {', '.join(short) or 'none'}; fuel {pct(fuel.get('ratio'))} of capacity")
    lines = sorted(prod.get("lines") or [], key=lambda x: -(x.get("factories") or 0))
    if lines:
        L.append("  lines: " + "; ".join(f"{x['equipment']} x{x['factories']:.0f} "
                                         f"{n(x.get('efficiency'))}%" for x in lines[:8]))
    stock = [e for e in prod.get("stockpile") or [] if e.get("fielded_short")]
    if stock:
        L.append("  fielded short: " + "; ".join(f"{e['archetype']} {e['fielded_short']:.0f}" for e in stock[:6]))
    res = snap.get("research") or {}
    L.append("  research: " + "; ".join(f"{s.get('researching') or 'IDLE'}"
                                       f"{' ' + str(s['days_remaining']) + 'd' if s.get('days_remaining') else ''}"
                                       for s in res.get("slots") or []) + f" | focus {res.get('focus')}")
    xp = g(snap, "doctrines", "experience", default={})
    if xp:
        L.append("  experience: " + ", ".join(f"{k} {v:.0f}" for k, v in xp.items() if v is not None))
    L.append("")
    land, st = snap.get("land") or {}, snap.get("stacking") or {}
    L.append("FORCE")
    L.append(f"  {land.get('division_count')} divisions in {len(land.get('armies') or [])} armies, "
             f"{len(land.get('groups') or [])} army groups, {land.get('unassigned')} unassigned; supply mean "
             f"{n(st.get('supply_mean'))}% lowest {n(st.get('supply_lowest'))}%, bands {st.get('bands')}")
    for grp in land.get("groups") or []:
        L.append(f"  GROUP {grp['group']} FM {grp.get('field_marshal') or 'NONE'}: "
                 + ("; ".join(_order_text(o) for o in grp.get("orders") or []) or "no group order"))
    for a in sorted(land.get("armies") or [], key=lambda x: -(x.get("divisions") or 0)):
        top = sorted((a.get("states") or {}).items(), key=lambda x: -x[1])[:3]
        pl = a.get("planning") or {}
        L.append(f"  {a['army']} [{a.get('group') or '-'}] {a.get('commander') or 'NO GENERAL'} "
                 f"d{a.get('divisions')} at {top} plan {pl.get('mean')}/{pl.get('max')} supply "
                 f"{n(a.get('supply_mean'))}% org {n(a.get('organisation_mean'))} | "
                 + ("; ".join(_order_text(o) for o in a.get("orders") or []) or "NO ORDER"))
    top = (st.get("provinces") or [])[:5]
    if top:
        L.append("  most divisions per province: " + "; ".join(
            f"{p['province']}(st {p.get('state')}) {p['divisions']} div supply {n(p.get('supply'), '{:.1f}')}"
            f"/{n(p.get('demand'), '{:.1f}')}" for p in top))
    L.append("")
    w = snap.get("wars") or {}
    L.append(f"WARS at war {w.get('at_war')} enemies {len(w.get('enemies') or [])}, fighting "
             f"{w.get('fighting')[:8] if w.get('fighting') else []}, won {w.get('won')}")
    back = lookback(snap, prev, 7)
    back2 = lookback(snap, prev, 14)
    for tag, e in sorted((w.get("per_enemy") or {}).items(),
                         key=lambda x: -((x[1].get("casualties_own") or 0) + (x[1].get("casualties_enemy") or 0))):
        if not (e.get("casualties_own") or e.get("casualties_enemy") or e.get("vp_held_by_us")):
            continue
        trend = []
        for label, b in (("7d", back), ("14d", back2)):
            be = g(b, "wars", "per_enemy", tag) if b else None
            if be:
                trend.append(f"{label}: VP {be.get('vp_held_by_us')}, cas "
                             f"{(e.get('casualties_own') or 0) - (be.get('casualties_own') or 0):,}:"
                             f"{(e.get('casualties_enemy') or 0) - (be.get('casualties_enemy') or 0):,}")
        L.append(f"  {tag}{' (at war)' if e.get('at_war') else ''}: VP held {n(e.get('vp_held_by_us'))}/"
                 f"{n(e.get('vp_total'))}, casualties ours:theirs {e.get('casualties_own') or 0:,}:"
                 f"{e.get('casualties_enemy') or 0:,}, states we control {e.get('states_we_control')}"
                 + (f" | {' | '.join(trend)}" if trend else ""))
    if w.get("own_states_lost"):
        L.append(f"  OWN STATES HELD BY OTHERS: {w['own_states_lost']}")
    L.append("")
    bt = snap.get("battles") or {}
    if bt.get("readable") and bt.get("list"):
        sm = bt.get("summary") or {}
        L.append(f"BATTLES {bt.get('count')} (attacking {sm.get('attacking')}, defending {sm.get('defending')}; "
                 f"our divisions in the line {sm.get('divisions_engaged')}, in reserve {sm.get('divisions_in_reserve')})")
        for x in sorted(bt["list"], key=lambda x: -((x.get("our_engaged") or 0) + (x.get("our_reserve") or 0)))[:12]:
            extra = [f"{ln['line']} {ln.get('value', ln.get('level', ''))}".strip() for ln in x.get("lines") or []
                     if ln.get("line") in ("terrain", "fort", "supply")]
            extra += [f"river {c.get('river')} {c.get('value')}" for ln in x.get("lines") or []
                      if ln.get("line") == "river" for c in (ln.get("crossings") or [])[:1]]
            L.append(f"  {x.get('province')} ({x.get('terrain')}, st {x.get('state')}) we {x.get('we')} vs "
                     f"{'/'.join(x.get('enemy') or [])} {x.get('hours')}h: line {x.get('our_engaged')}+"
                     f"{x.get('our_reserve')} reserve vs {x.get('their_engaged')}+{x.get('their_reserve')}, "
                     f"width {x.get('width_engaged')}/{x.get('width_max')}, org {x.get('our_org')} vs "
                     f"{x.get('their_org')}, planning {x.get('our_planning')}, winning {x.get('winning')}"
                     + (f" | {', '.join(extra)}" if extra else ""))
        L.append("")
    air = snap.get("air") or {}
    L.append(f"AIR {air.get('aircraft_total')} planes in {len(air.get('wings') or [])} wings")
    for rid, r in sorted((air.get("regions") or {}).items(), key=lambda x: -(x[1].get("aircraft") or 0)):
        if r.get("aircraft"):
            L.append(f"  {r.get('name')} ({rid}): {r['aircraft']} planes, missions {r.get('missions')}")
    if air.get("wings_without_region"):
        L.append(f"  wings without a region: {len(air['wings_without_region'])}")
    navy = snap.get("navy") or {}
    L.append(f"NAVY {navy.get('ships')} ships; " + "; ".join(
        f"{f.get('fleet')} ({f.get('admiral') or 'no admiral'}): "
        + ", ".join(f"{t.get('name')} {t.get('mission')}" for t in f.get("task_forces") or [])
        for f in navy.get("fleets") or []) + f"; convoys {navy.get('convoys')}")
    L.append("")
    ex = snap.get("executor") or {}
    L.append("EXECUTOR")
    for op, o in (ex.get("operations") or {}).items():
        L.append(f"  operation {op}: {o.get('phase')}, fought {o.get('fought')}, {o.get('steps')} steps done")
    for key in ("last_executor_action", "last_order", "last_step"):
        if ex.get(key):
            L.append(f"  {key.replace('_', ' ')}: {ex[key]}")
    for f in (ex.get("failures") or [])[:4]:
        L.append(f"  refused x{f['count']}: {f['what'][:140]}")
    calls = snap.get("player_calls") or []
    if calls:
        L.append("PLAYER last calls: " + "; ".join(
            f"{c['tool']}{' ERR ' + str(c.get('code')) if c.get('error') else ''}" for c in calls))
    if snap.get("read_errors"):
        L.append(f"READ ERRORS: {snap['read_errors']}")
    L.append("")
    L.append("NOT READABLE (no pure read yet): " + "; ".join(x["what"] for x in GAPS))
    return "\n".join(L) + "\n"


# --- diff ---------------------------------------------------------------------


def _ckey(r):
    return (r.get("building"), r.get("state"), r.get("province"))


def diff(a, b):
    """What changed from snapshot a (older) to b (newer), as text lines."""
    L = [f"CHANGES {a.get('date')} -> {b.get('date')}"]

    def delta(label, x, y, fmt="{:+.0f}"):
        if x is not None and y is not None and x != y:
            L.append(f"  {label}: {x} -> {y} ({fmt.format(y - x)})")

    fa, fb = g(a, "economy", "factories", default={}), g(b, "economy", "factories", default={})
    for k in ("civilian", "military", "naval"):
        delta(f"{k} factories", fa.get(k), fb.get(k))
    delta("political power", g(a, "politics", "political_power"), g(b, "politics", "political_power"))
    delta("manpower", g(a, "politics", "manpower"), g(b, "politics", "manpower"), "{:+,.0f}")
    qa = {_ckey(r) for r in g(a, "construction", "queue", default=[])}
    qb = {_ckey(r) for r in g(b, "construction", "queue", default=[])}
    for k in sorted(qa - qb, key=str):
        L.append(f"  built (left the queue): {k[0]} in state {k[1]}")
    for k in sorted(qb - qa, key=str):
        L.append(f"  queued: {k[0]} in state {k[1]}")
    ra = {s.get("slot"): s.get("researching") for s in g(a, "research", "slots", default=[])}
    for s in g(b, "research", "slots", default=[]):
        if ra.get(s.get("slot")) != s.get("researching"):
            L.append(f"  research slot {s.get('slot')}: {ra.get(s.get('slot'))} -> {s.get('researching')}")
    da = {(f.get("folder"), t.get("track")): t.get("subdoctrine")
          for f in g(a, "doctrines", "folders", default=[]) for t in f.get("tracks") or []}
    for f in g(b, "doctrines", "folders", default=[]):
        for t in f.get("tracks") or []:
            if t.get("subdoctrine") and da.get((f.get("folder"), t.get("track"))) != t.get("subdoctrine"):
                L.append(f"  doctrine adopted: {f.get('folder')} {t.get('track')} {t.get('subdoctrine')}")
    la = {c["name"]: c.get("assigned_to") for c in g(a, "leaders", "commanders", default=[])}
    for c in g(b, "leaders", "commanders", default=[]):
        if c["name"] in la and la[c["name"]] != c.get("assigned_to"):
            L.append(f"  leader {c['name']}: {la[c['name']] or 'nothing'} -> {c.get('assigned_to') or 'nothing'}")
    aa = {x["army"]: x for x in g(a, "land", "armies", default=[])}
    ab = {x["army"]: x for x in g(b, "land", "armies", default=[])}
    for k in sorted(set(ab) - set(aa)):
        L.append(f"  new army {k} ({ab[k].get('divisions')} divisions)")
    for k in sorted(set(aa) - set(ab)):
        L.append(f"  army gone: {k}")
    for k in sorted(set(aa) & set(ab)):
        x, y = aa[k], ab[k]
        if x.get("divisions") != y.get("divisions"):
            L.append(f"  {k}: divisions {x.get('divisions')} -> {y.get('divisions')}")
        sx = max((x.get("states") or {}).items(), key=lambda t: t[1], default=(None, 0))[0]
        sy = max((y.get("states") or {}).items(), key=lambda t: t[1], default=(None, 0))[0]
        if sx != sy:
            L.append(f"  {k} moved: main state {sx} -> {sy}")
        ox = [(o.get("kind"), tuple(o.get("states") or [])) for o in x.get("orders") or []]
        oy = [(o.get("kind"), tuple(o.get("states") or [])) for o in y.get("orders") or []]
        if ox != oy:
            L.append(f"  {k} orders: {ox or 'none'} -> {oy or 'none'}")
    ea, eb = set(g(a, "wars", "enemies", default=[])), set(g(b, "wars", "enemies", default=[]))
    for t in sorted(eb - ea):
        L.append(f"  war: now at war with {t}")
    for t in sorted(ea - eb):
        L.append(f"  war: no longer at war with {t}")
    for t, e in sorted(g(b, "wars", "per_enemy", default={}).items()):
        p = g(a, "wars", "per_enemy", t) or {}
        if e.get("vp_held_by_us") != p.get("vp_held_by_us") and e.get("vp_held_by_us") is not None:
            L.append(f"  {t}: victory points held {p.get('vp_held_by_us')} -> {e.get('vp_held_by_us')}"
                     f" of {e.get('vp_total')}")
        lo = (e.get("casualties_own") or 0) - (p.get("casualties_own") or 0)
        de = (e.get("casualties_enemy") or 0) - (p.get("casualties_enemy") or 0)
        if lo or de:
            L.append(f"  {t}: casualties ours {lo:+,} theirs {de:+,}")
    ca, cb = set(g(a, "wars", "states_controlled", default=[])), set(g(b, "wars", "states_controlled", default=[]))
    if cb - ca:
        L.append(f"  states taken: {sorted(cb - ca)}")
    if ca - cb:
        L.append(f"  states lost: {sorted(ca - cb)}")
    ra_ = g(a, "air", "regions", default={})
    for rid, r in sorted(g(b, "air", "regions", default={}).items()):
        x, y = (ra_.get(rid) or {}).get("aircraft") or 0, r.get("aircraft") or 0
        if x != y:
            L.append(f"  air {r.get('name')} ({rid}): {x} -> {y} planes")
    for rid, r in sorted(ra_.items()):
        if rid not in g(b, "air", "regions", default={}) and r.get("aircraft"):
            L.append(f"  air {r.get('name')} ({rid}): {r['aircraft']} -> 0 planes")
    fa_ = {(f["code"], f.get("subject")) for f in a.get("findings") or []}
    fb_ = {(f["code"], f.get("subject")): f for f in b.get("findings") or []}
    for k in sorted(set(fb_) - fa_, key=str):
        L.append(f"  new finding: {fb_[k]['text'][:160]}")
    for k in sorted(fa_ - set(fb_), key=str):
        L.append(f"  resolved: {k[0]} {k[1] or ''}".rstrip())
    if len(L) == 1:
        L.append("  nothing readable changed")
    return "\n".join(L) + "\n"
