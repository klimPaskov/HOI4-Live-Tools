"""Phase 3 exit check, driven through the MCP stdio server exactly as an agent would use it.

    "C:/Program Files/Python39/python.exe" scripts/exit_check_phase3.py [--save qa_001_fresh_head] [--only USA|MEX]

For a major (USA) and a minor (Mexico, reached with the console `tag` in test setup only), from a fresh 1936 start
(the QA save at 1 Jan 1936) and then in campaign mode (no console): set a national focus, fill two research slots,
add a production line and assign factories, queue a construction in a named state, run time until political power
allows, hire an advisor (or change a law) and take a decision; then save through the Save dialog and show every
action in hoi4_state and hoi4_briefing read from that save. Choices are made from the tools' own lists, as an
agent would. At the end the QA save is loaded again and the game is left paused on the map.

Evidence: docs/evidence/phase3/ (exit_check.md, results.json, transcript.jsonl, state_<TAG>.json,
briefing_<TAG>.md, images/).
"""

import argparse
import datetime
import json
import os
import re
import shutil
import sys
import time

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
from mcp_client import McpClient  # noqa: E402

EVIDENCE = os.path.join(ROOT, "docs", "evidence", "phase3")


def to_jpeg(src, dst_dir, name, max_width=1280):
    os.makedirs(dst_dir, exist_ok=True)
    im = Image.open(src).convert("RGB")
    if im.width > max_width:
        im = im.resize((max_width, round(im.height * max_width / im.width)), Image.LANCZOS)
    dst = os.path.join(dst_dir, name + ".jpg")
    im.save(dst, "JPEG", quality=82)
    return os.path.relpath(dst, EVIDENCE).replace("\\", "/")


class Run:
    def __init__(self, client):
        self.c = client
        self.steps = []
        self.checks = []

    def step(self, label, tool, targs=None, ok_if=None, country=None, retry=False, refusal_ok=None):
        """One tool call. ok_if(data, text) judges a successful call; refusal_ok(data, text) may accept an error
        result as the expected refusal (a full state, an unaffordable choice), which the agent then acts on."""
        t0 = time.time()
        try:
            res = self.c.call(tool, targs or {}, timeout=2400)
            if retry and res.get("isError"):
                # running time may be interrupted by the shared desktop (another window took the focus):
                # an agent simply asks again
                first = " ".join(x["text"] for x in res["content"] if x["type"] == "text").splitlines()[0]
                print(f"   retrying {tool} after: {first[:150]}", flush=True)
                res = self.c.call(tool, targs or {}, timeout=2400)
                res.setdefault("_retried_after", first)
        except Exception as e:                       # a protocol failure is a failed step, not a crash
            res = {"content": [{"type": "text", "text": f"{type(e).__name__}: {e}"}], "isError": True}
        text = "\n".join(x["text"] for x in res["content"] if x["type"] == "text")
        data = res.get("structuredContent") or {}
        ok = not res.get("isError")
        note = ""
        refused = False
        if ok and ok_if:
            try:
                ok, note = ok_if(data, text)
            except Exception as e:
                ok, note = False, f"check raised {type(e).__name__}: {e}"
        elif not ok and refusal_ok:
            try:
                refused, note = refusal_ok(data, text)
            except Exception as e:
                refused, note = False, f"check raised {type(e).__name__}: {e}"
            ok = bool(refused)
        imgs = [x["file"] for x in res["content"] if x["type"] == "image" and x.get("file")]
        rec = {"country": country, "label": label, "tool": tool, "args": targs or {}, "ok": bool(ok),
               "refused": bool(refused), "retried_after": res.get("_retried_after"),
               "seconds": round(time.time() - t0, 1), "summary": text.splitlines()[0] if text else "",
               "note": note, "data": data, "images": imgs}
        self.steps.append(rec)
        print(f"[{'refused' if refused else 'ok' if ok else 'FAIL'}] {country or ''} {label}: {rec['summary'][:170]} "
              f"({rec['seconds']} s) {note}", flush=True)
        return rec

    def check(self, country, claim, ok, detail):
        self.checks.append({"country": country, "claim": claim, "ok": bool(ok), "detail": detail})
        print(f"   [{'match' if ok else 'MISMATCH'}] {country} {claim}: {detail}", flush=True)


def norm(s):
    return re.sub(r"[^a-z0-9]", "", (s or "").lower())


def name_in(name, text):
    """Every word of `name` (3+ letters) has a close word in `text`: 'Harold Ickes' is in 'Harold L. Ickes' and an
    OCR'd 'Smalt Lobbying Effort' is in 'Small Lobbying Effort'."""
    import difflib
    words = [w for w in re.findall(r"[a-z0-9]+", (name or "").lower()) if len(w) >= 3]
    have = re.findall(r"[a-z0-9]+", (text or "").lower())
    return bool(words) and all(any(difflib.SequenceMatcher(None, w, h).ratio() >= 0.8 for h in have) for w in words)


def play(run, tag, save, prefs, start_save):
    """The campaign part for one country. Returns what was done (for the checks). History (completed focuses,
    researched technologies) comes from the start save: the game was just loaded from it."""
    done = {"tag": tag}
    T = {"tag": tag, "save": start_save}
    s = run.step
    s("campaign mode (no console from here)", "hoi4_lock", {"action": "set_mode", "mode": "campaign"},
      lambda d, t: ("mode campaign" in t, t.splitlines()[0][:80]), tag)
    s("briefing before acting", "hoi4_briefing", dict(T, remember=False),
      lambda d, t: (bool(d.get("markdown")), f"{len(d.get('attention', []))} attention item(s)"), tag)
    # ---- focus
    fl = s("available focuses", "hoi4_focus", dict(T, limit=60),
           lambda d, t: (len(d.get("focuses", [])) > 0, f"{d.get('matching')} available in {d.get('tree')}"), tag)
    rows = fl["data"].get("focuses", [])
    pick = next((r for r in rows if r["id"] in prefs.get("focus", [])), None) or \
        next((r for r in rows if "add_political_power" in (r.get("effects") or "") and not r.get("requires")), None) or \
        next((r for r in rows if not r.get("requires")), None) or (rows[0] if rows else None)
    if pick:
        s(f"focus details: {pick['name']}", "hoi4_focus", dict(T, action="details", focus=pick["id"]),
          lambda d, t: (bool(d.get("detail", {}).get("title")), f"{d.get('detail', {}).get('days')} days, unmet "
                                                                f"{d.get('detail', {}).get('unmet')}"), tag)
        r = s(f"start focus {pick['name']}", "hoi4_focus", dict(T, action="select", focus=pick["id"], replace=True),
              lambda d, t: (d.get("changed") is True, f"politics panel shows {d.get('after')}"), tag)
        if r["ok"]:
            done["focus"] = pick
    # ---- research
    rl = s("researchable technologies (not ahead of time)", "hoi4_research",
           dict(T, action="list", ahead=False, limit=120),
           lambda d, t: (len(d.get("technologies", [])) >= 2, f"{d.get('matching')} available in {d.get('year')}"), tag)
    techs = rl["data"].get("technologies", [])
    wanted = [t for p in prefs.get("techs", []) for t in techs if t["id"] == p]
    for t in techs:
        if len(wanted) >= 2:
            break
        if t not in wanted and not t.get("allow") and not t["id"].startswith("sp_") and \
                t["folder"] not in [w["folder"] for w in wanted]:
            wanted.append(t)
    done["techs"] = []
    for t in wanted[:2]:
        r = s(f"research {t['name']} ({t['folder']})", "hoi4_research", dict(T, action="start", tech=t["id"]),
              lambda d, t2: (d.get("changed") is True or "already" in t2,
                             f"slot {d.get('slot')}, {d.get('days')} days; detail '{(d.get('detail') or {}).get('title')}'"),
              tag)
        if r["ok"]:
            done["techs"].append(t)
    s("research slots", "hoi4_research", {"action": "slots"},
      lambda d, t: (len([x for x in d.get("slots", []) if x["state"] == "busy"]) >= 2, t.splitlines()[0][:150]), tag)
    # ---- production
    pl = s("production lines", "hoi4_production", {"tag": tag, "action": "lines"},
           lambda d, t: (d.get("count", 0) >= 0, t.splitlines()[0][:150]), tag)
    lines = pl["data"].get("lines", [])
    eq = prefs.get("equipment") or (lines[0]["type"] if lines and lines[0].get("type") else "Infantry Equipment")
    r = s(f"add a production line: {eq}", "hoi4_production", {"tag": tag, "action": "add", "equipment": eq, "factories": 1},
          lambda d, t: (d.get("changed") is True and d.get("line"),
                        f"line {(d.get('line') or {}).get('number')} {(d.get('line') or {}).get('name')}, "
                        f"military factories {d.get('header_before', {}).get('military_used')} -> "
                        f"{d.get('header_after', {}).get('military_used')}"), tag)
    line = r["data"].get("line") if r["ok"] else None
    if line:
        hdr = r["data"]["header_after"]
        free = (hdr.get("military_total") or 0) - (hdr.get("military_used") or 0)
        # the line requests the factories; when none is free (Mexico's three are all busy) the game keeps the
        # request and shows "assigned/requested" until a factory frees up
        target = (line.get("requested") or line.get("factories") or 1) + max(1, min(2, free))
        r2 = s(f"assign factories to line {line['number']} ({line['name']}): {target}", "hoi4_production",
               {"tag": tag, "action": "assign", "line": str(line["number"]), "factories": target},
               lambda d, t: (d.get("changed") is True,
                             f"{(d.get('line_before') or {}).get('factories_text')} -> "
                             f"{(d.get('line_after') or {}).get('factories_text')}"),
               tag)
        after_line = r2["data"].get("line_after") or {}
        done["production"] = {"line": line, "factories": after_line.get("requested") or after_line.get("factories"),
                              "assigned": after_line.get("factories"), "name": line["name"], "type": line.get("type")}
    # ---- construction: the first candidate state with a free building slot (the tool names a full state)
    for state in prefs.get("states") or [prefs["state"]]:
        r = s(f"queue a {prefs.get('building_name', 'civilian factory')} in {state}", "hoi4_construction",
              dict(action="build", building=prefs.get("building", "industrial_complex"), state=state),
              lambda d, t: (d.get("changed") is True, f"queue {len(d.get('before', {}).get('rows', []))} -> "
                                                      f"{len(d.get('after', {}).get('rows', []))} item(s); slots "
                                                      f"{(d.get('slots') or {}).get('used')}/{(d.get('slots') or {}).get('total')}"),
              tag, refusal_ok=lambda d, t: (d.get("error") == "no_slot",
                                            f"every slot used ({(d.get('slots') or {}).get('used')}/"
                                            f"{(d.get('slots') or {}).get('total')}): the next state is tried"))
        if r["ok"] and not r["refused"]:
            done["construction"] = {"state": state, "building": prefs.get("building", "industrial_complex")}
            break
        if not r["refused"]:
            break
    s("trade (civilian factories for trade, deficits)", "hoi4_diplomacy", {"action": "trade"},
      lambda d, t: (d.get("civilian_total") is not None, t.splitlines()[0][:150]), tag)
    # ---- time, then political power
    s(f"run {prefs['days']} days answering popups (campaign mode: date polling)", "hoi4_time",
      {"action": "step", "days": prefs["days"], "speed": 5, "popups": "first", "max_seconds": 2400},
      lambda d, t: (True, t.splitlines()[0][:150]), tag, retry=True)
    s("focus progress", "hoi4_focus", {"tag": tag, "action": "progress", "save": "none"},
      lambda d, t: (True, t.splitlines()[0][:150]), tag)
    # decision: the cheapest one the panel says can be taken
    dl = s("decisions that can be taken", "hoi4_decisions", {"action": "list", "available": True},
           lambda d, t: (True, t.splitlines()[0][:200]), tag)
    cands = sorted(dl["data"].get("decisions", []), key=lambda r: (r.get("cost") or 0))
    cands = [c for c in cands if c.get("id") or c.get("name")]
    for c in cands[:3]:
        r = s(f"take decision {c['name']}", "hoi4_decisions", {"action": "take", "decision": c.get("id") or c["name"]},
              lambda d, t: (d.get("changed") is True, f"PP {d.get('political_power_before')} -> "
                                                      f"{d.get('political_power_after')}"), tag)
        if r["ok"]:
            done["decision"] = c
            break
    # advisor: the cheapest one that can be hired (run more days when none is affordable yet)
    for attempt in range(prefs.get("pp_waits", 3)):
        opt = s(f"{prefs['advisor_slot']} choices", "hoi4_politics", dict(T, action="options", slot=prefs["advisor_slot"]),
                lambda d, t: (len(d.get("choices", [])) > 0, t.splitlines()[0][:200]), tag)
        pp = opt["data"].get("political_power") or 0
        ch = [c for c in opt["data"].get("choices", []) if not c.get("condition_unmet") and c.get("cost")]
        ch.sort(key=lambda c: c["cost"])
        if ch and ch[0]["cost"] <= pp:
            r = s(f"hire {ch[0]['name']} ({ch[0]['cost']} PP)", "hoi4_politics",
                  dict(T, action="hire", advisor=ch[0]["name"], slot=prefs["advisor_slot"]),
                  lambda d, t: (d.get("changed") is True, f"{d.get('before')} -> {d.get('after')}, PP "
                                                          f"{d.get('political_power_before')} -> {d.get('political_power_after')}"),
                  tag)
            if r["ok"]:
                done["advisor"] = ch[0]
                break
        need = (ch[0]["cost"] - pp) if ch else 30
        days = int(min(60, max(5, need / max(0.3, prefs.get("pp_per_day", 1.0)) + 2)))
        s(f"run {days} days for political power ({pp} now)", "hoi4_time",
          {"action": "step", "days": days, "speed": 5, "popups": "first", "max_seconds": 2400},
          lambda d, t: (True, t.splitlines()[0][:150]), tag, retry=True)
    if "decision" not in done:
        dl = s("decisions that can be taken (second look)", "hoi4_decisions", {"action": "list", "available": True},
               lambda d, t: (True, t.splitlines()[0][:200]), tag)
        for c in sorted(dl["data"].get("decisions", []), key=lambda r: (r.get("cost") or 0))[:3]:
            r = s(f"take decision {c['name']}", "hoi4_decisions", {"action": "take", "decision": c.get("id") or c["name"]},
                  lambda d, t: (d.get("changed") is True, f"PP {d.get('political_power_before')} -> "
                                                          f"{d.get('political_power_after')}"), tag)
            if r["ok"]:
                done["decision"] = c
                break
    s("politics after (laws, advisors)", "hoi4_politics", {"tag": tag, "action": "view", "save": "none"},
      lambda d, t: (True, t.splitlines()[0][:200]), tag)
    # ---- the save shows everything
    sv = s(f"save hoi4live_{save} (Save Game dialog)", "hoi4_save", {"name": save},
           lambda d, t: (d.get("method") == "dialog" and d.get("player") == tag, f"{d.get('player')} {d.get('date')} "
                                                                                 f"{d.get('size_mb')} MB via {d.get('method')}"),
           tag)
    st = s(f"hoi4_state {tag} from hoi4live_{save}", "hoi4_state",
           {"tag": tag, "save": f"hoi4live_{save}", "screen": True, "limit": 60,
            "sections": ["overview", "politics", "economy", "focus", "research", "production", "construction",
                         "decisions"]},
           lambda d, t: ((d.get("sources", {}).get("save") or {}).get("file") == f"hoi4live_{save}.hoi4",
                         f"save source {(d.get('sources', {}).get('save') or {}).get('file')}"), tag)
    br = s(f"hoi4_briefing {tag} from hoi4live_{save}", "hoi4_briefing",
           {"tag": tag, "save": f"hoi4live_{save}", "world": False},
           lambda d, t: (bool(d.get("markdown")), f"{len(d.get('attention', []))} attention item(s)"), tag)
    verify(run, tag, done, st["data"], br["data"].get("markdown", ""))
    return done, st["data"], br["data"].get("markdown", "")


def verify(run, tag, done, S, md):
    ck = run.check
    f = S.get("focus", {})
    fid = (done.get("focus") or {}).get("id")
    completed = []
    raw = S.get("focus") or {}
    ck(tag, "the focus is in progress or completed in the save",
       fid and (raw.get("current") == fid or fid in (raw.get("completed") or [])),
       f"set {fid}; save current {raw.get('current')}, completed {(raw.get('completed') or [])[-3:]}")
    fname = (done.get("focus") or {}).get("name") or ""
    ck(tag, "the briefing names the focus (in progress or completed)", bool(fname) and norm(fname) in norm(md), fname)
    r = S.get("research", {})
    have = {x.get("tech") for x in r.get("researching") or []} | {x.get("tech") for x in r.get("recently_completed") or []}
    for t in done.get("techs", []):
        ck(tag, f"research {t['id']} is in a slot (or done) in the save", t["id"] in have,
           f"save researching {[x.get('tech') for x in r.get('researching') or []]}")
        ck(tag, f"the briefing lists {t['name']}", norm(t["name"]) in norm(md), t["name"])
    ck(tag, "two research slots were filled", len(done.get("techs", [])) >= 2, str([t["id"] for t in done.get("techs", [])]))
    p = done.get("production")
    lines = (S.get("production") or {}).get("lines") or []
    if p:
        hit = [l for l in lines if norm(p["name"])[:8] in norm(l.get("name")) or
               norm(p.get("type") or "")[:10] in norm(l.get("name"))]
        # the save keeps the requested factories and the assigned ones (fewer when none is free)
        ck(tag, "the new production line is in the save with the factories it requests",
           any((l.get("requested") or l.get("factories") or 0) >= (p.get("factories") or 1) for l in hit),
           f"{p['name']} requests {p.get('factories')} (assigned {p.get('assigned')}); save lines of that equipment "
           f"{[(l.get('name'), l.get('factories'), l.get('requested')) for l in hit][:4]}")
    else:
        ck(tag, "a production line was added", False, "no line added")
    c = done.get("construction")
    q = (S.get("construction") or {}).get("queue") or []
    ck(tag, f"the construction in {c and c['state']} is queued in the save",
       bool(c) and any(norm(x.get("state_name")) == norm(c["state"]) and x.get("building") == c["building"] for x in q),
       f"save queue {[(x.get('state_name'), x.get('building')) for x in q][:6]}")
    ck(tag, f"the briefing shows the construction in {c and c['state']}", bool(c) and norm(c["state"]) in norm(md),
       c and c["state"])
    adv = (S.get("politics") or {}).get("advisors") or []
    a = done.get("advisor")
    ck(tag, "the hired advisor is appointed in the save",
       bool(a) and any(name_in(a["name"], x.get("name")) or name_in(a["name"], (x.get("character") or "").replace("_", " "))
                       for x in adv), f"hired {a and a['name']}; save advisors {[x.get('name') for x in adv]}")
    ck(tag, "the briefing lists the advisor", bool(a) and name_in(a["name"], md), a and a["name"])
    d = done.get("decision")
    dec = S.get("decisions") or {}
    taken = [x.get("id") for x in dec.get("taken") or []] + [x.get("id") for x in dec.get("in_effect") or []]
    names = [x.get("name") for x in (dec.get("taken") or []) + (dec.get("in_effect") or [])]
    ck(tag, "the decision is taken in the save",
       bool(d) and ((d.get("id") and d["id"] in taken) or any(name_in(d["name"], n) for n in names)),
       f"took {d and (d.get('id') or d['name'])}; save taken {sorted(set(taken))[:6]}")
    ck(tag, "the briefing lists the decision", bool(d) and name_in(d["name"], md), d and d["name"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--save", default="qa_001_fresh_head")
    ap.add_argument("--only", choices=["USA", "MEX"])
    args = ap.parse_args()
    for name in ("images", "raw_images"):
        shutil.rmtree(os.path.join(EVIDENCE, name), ignore_errors=True)
    os.makedirs(EVIDENCE, exist_ok=True)
    for name in ("transcript.jsonl", "results.json", "exit_check.md"):
        if os.path.exists(os.path.join(EVIDENCE, name)):
            os.remove(os.path.join(EVIDENCE, name))
    raw_images = os.path.join(EVIDENCE, "raw_images")
    started = time.time()
    results = {}
    with McpClient(image_dir=raw_images, transcript=os.path.join(EVIDENCE, "transcript.jsonl")) as c:
        init = c.initialize()
        tools = c.request("tools/list")["result"]["tools"]
        run = Run(c)
        s = run.step
        s("acquire the desktop lock for the run", "hoi4_lock", {"action": "acquire", "purpose": "phase 3 exit check"})
        countries = [("USA", "p3_usa", {"focus": ["USA_continue_the_new_deal"],
                                        "techs": ["basic_machine_tools", "electronic_mechanical_engineering"],
                                        "equipment": "support_equipment_1", "state": "Michigan",
                                        "building": "industrial_complex", "building_name": "civilian factory",
                                        "days": 72, "advisor_slot": "political_advisor", "pp_per_day": 1.2}),
                     ("MEX", "p3_mex", {"focus": ["MEX_focus_national_bank", "MEX_focus_plan_of_agua_prieta"],
                                        "state": "Mexico City", "states": ["Mexico City", "Jalisco", "Veracruz",
                                                                           "Yucatan", "Sonora"],
                                        "building": "industrial_complex",
                                        "building_name": "civilian factory", "days": 36,
                                        "advisor_slot": "political_advisor", "pp_per_day": 1.0, "pp_waits": 4})]
        for tag, save, prefs in countries:
            if args.only and tag != args.only:
                continue
            s("test mode for the setup", "hoi4_lock", {"action": "set_mode", "mode": "test"}, None, tag)
            load = s(f"fresh 1936 start: load {args.save}", "hoi4_load", {"save": args.save},
                     lambda d, t: (d.get("date_matches") is True, f"map {d.get('date_on_screen')} vs save "
                                                                  f"{d.get('save_date')}"), tag)
            if not load["ok"]:
                continue
            if tag != "USA":
                s(f"setup: play {tag} (console `tag`, test mode only)", "hoi4_console", {"commands": [f"tag {tag}"]},
                  lambda d, t: (True, " | ".join(d.get("response", []) or [])[:120]), tag)
            done, S, md = play(run, tag, save, prefs, args.save)
            results[tag] = {"done": done, "state": S, "briefing": md}
        s("test mode to restore the QA save", "hoi4_lock", {"action": "set_mode", "mode": "test"})
        s(f"restore: load {args.save} (the game is left as it was found)", "hoi4_load", {"save": args.save},
          lambda d, t: (d.get("date_matches") is True, f"{d.get('date_on_screen')}"))
        s("game left paused on the map", "hoi4_time", {"action": "status"},
          lambda d, t: (d.get("running") is False, f"{d.get('text')}"))
        s("release the desktop lock", "hoi4_lock", {"action": "release"})
        progress = [n for n in c.notifications if n.get("method") == "notifications/progress"]

    # ---------------------------------------------------------------- evidence
    img_dir = os.path.join(EVIDENCE, "images")
    for i, st in enumerate(run.steps):
        st["images"] = [to_jpeg(p, img_dir, f"step{i + 1:03d}_{os.path.splitext(os.path.basename(p))[0]}")
                        for p in st["images"]]
    shutil.rmtree(raw_images, ignore_errors=True)
    for tag, r in results.items():
        with open(os.path.join(EVIDENCE, f"state_{tag}.json"), "w", encoding="utf-8") as f:
            json.dump(r["state"], f, indent=1, default=str)
        with open(os.path.join(EVIDENCE, f"briefing_{tag}.md"), "w", encoding="utf-8") as f:
            f.write(r["briefing"] + "\n")
    ok = all(st["ok"] for st in run.steps) and all(ck["ok"] for ck in run.checks)
    with open(os.path.join(EVIDENCE, "results.json"), "w", encoding="utf-8") as f:
        json.dump({"ok": ok, "server": init.get("serverInfo"), "protocol": init.get("protocolVersion"),
                   "tools": len(tools), "progress_notifications": len(progress), "checks": run.checks,
                   "done": {k: v["done"] for k, v in results.items()}, "steps": run.steps}, f, indent=1, default=str)
    write_report(ok, init, tools, run, results, time.time() - started, args)
    print("exit check", "PASS" if ok else "FAIL", flush=True)
    return 0 if ok else 1


def write_report(ok, init, tools, run, results, seconds, args):
    now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    L = [f"# Phase 3 exit check: {'PASS' if ok else 'FAIL'}", "",
         f"Run {now}, {round(seconds / 60)} min, through the MCP stdio server "
         f"({init.get('serverInfo', {}).get('name')} {init.get('serverInfo', {}).get('version')}, protocol "
         f"{init.get('protocolVersion')}, {len(tools)} tools). Driver: `scripts/exit_check_phase3.py`. For each "
         f"country: a fresh 1936 start (`{args.save}`, 1 Jan 1936; Mexico through the console `tag` in test setup), "
         "then campaign mode only: a focus, two research slots, a production line with factories assigned, a "
         "construction in a named state, time run until political power allowed an advisor, a decision; a save "
         "through the Save Game dialog; `hoi4_state` and `hoi4_briefing` read from that save.", "",
         "## What the saves show", "", "| # | country | claim | result | evidence |", "|---|---|---|---|---|"]
    for i, c in enumerate(run.checks, 1):
        L.append(f"| {i} | {c['country']} | {c['claim']} | {'match' if c['ok'] else '**MISMATCH**'} | "
                 f"{str(c['detail']).replace('|', '/')[:240]} |")
    for tag, r in results.items():
        L += ["", f"## Briefing of {tag} from its save", "",
              f"(also [briefing_{tag}.md](briefing_{tag}.md); the `hoi4_state` result is [state_{tag}.json](state_{tag}.json))",
              "", "```markdown", r["briefing"], "```"]
    L += ["", "## Steps", "", "| # | country | step | tool | result | s | detail |", "|---|---|---|---|---|---|---|"]
    for i, st in enumerate(run.steps, 1):
        detail = (st["note"] or st["summary"]).replace("|", "/")[:170]
        verdict = "refused (expected)" if st.get("refused") else "ok" if st["ok"] else "**FAIL**"
        L.append(f"| {i} | {st['country'] or ''} | {st['label']} | `{st['tool']}` | {verdict} | "
                 f"{st['seconds']} | {detail} |")
    L += ["", "## Screens", ""]
    for i, st in enumerate(run.steps, 1):
        for img in st["images"]:
            L.append(f"- step {i} ({st['country'] or ''} {st['label']}): ![step {i}]({img})")
    with open(os.path.join(EVIDENCE, "exit_check.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(L) + "\n")


if __name__ == "__main__":
    sys.exit(main())
