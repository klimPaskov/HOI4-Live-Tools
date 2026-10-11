"""Phase 2 exit check, driven through the MCP stdio server exactly as an agent would use it.

    "C:/Program Files/Python39/python.exe" scripts/exit_check_phase2.py [--save qa_001_fresh_head] [--days 4]

From the QA save: check the telemetry mod is live, run a few days, save, then produce hoi4_state for the player
and a hoi4_briefing, and show that they match the screen (top bar, tooltips, the research, trade and production
panels) and the save (hoi4_save_query). Every claim the check makes is compared with its source in a consistency
table.

Evidence goes to docs/evidence/phase2/: exit_check.md (report with the briefing and the consistency table),
results.json, transcript.jsonl (every JSON-RPC message, images replaced by file names), state_USA.json (the full
hoi4_state result), briefing.md and images/ (JPEG copies; top-bar crops as PNG).
"""

import argparse
import datetime
import difflib
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

EVIDENCE = os.path.join(ROOT, "docs", "evidence", "phase2")


def to_jpeg(src, dst_dir, name, max_width=1280):
    os.makedirs(dst_dir, exist_ok=True)
    im = Image.open(src).convert("RGB")
    if im.width > max_width:
        im = im.resize((max_width, round(im.height * max_width / im.width)), Image.LANCZOS)
    dst = os.path.join(dst_dir, name + ".jpg")
    im.save(dst, "JPEG", quality=85)
    return os.path.relpath(dst, EVIDENCE).replace("\\", "/")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--save", default="qa_001_fresh_head")
    ap.add_argument("--tag", default="USA")
    ap.add_argument("--days", type=int, default=4)
    args = ap.parse_args()

    for name in ("images", "raw_images"):
        shutil.rmtree(os.path.join(EVIDENCE, name), ignore_errors=True)
    for name in ("transcript.jsonl", "results.json", "exit_check.md", "briefing.md", "state_USA.json"):
        if os.path.exists(os.path.join(EVIDENCE, name)):
            os.remove(os.path.join(EVIDENCE, name))
    os.makedirs(EVIDENCE, exist_ok=True)
    raw_images = os.path.join(EVIDENCE, "raw_images")
    steps = []
    checks = []
    started = time.time()

    def check(claim, ok, detail):
        checks.append({"claim": claim, "ok": bool(ok), "detail": detail})
        print(f"   [{'match' if ok else 'MISMATCH'}] {claim}: {detail}", flush=True)

    with McpClient(image_dir=raw_images, transcript=os.path.join(EVIDENCE, "transcript.jsonl")) as c:
        init = c.initialize()
        tools = c.request("tools/list")["result"]["tools"]

        def step(label, tool, targs=None, ok_if=None):
            t0 = time.time()
            res = c.call(tool, targs or {})
            text = "\n".join(x["text"] for x in res["content"] if x["type"] == "text")
            data = res.get("structuredContent") or {}
            ok = not res.get("isError")
            note = ""
            if ok and ok_if:
                ok, note = ok_if(data, text)
            imgs = [x["file"] for x in res["content"] if x["type"] == "image" and x.get("file")]
            rec = {"label": label, "tool": tool, "args": targs or {}, "ok": ok, "seconds": round(time.time() - t0, 1),
                   "summary": text.splitlines()[0] if text else "", "note": note, "data": data, "images": imgs,
                   "text": text}
            steps.append(rec)
            print(f"[{'ok' if ok else 'FAIL'}] {label}: {rec['summary'][:160]} ({rec['seconds']} s) {note}", flush=True)
            return rec

        step("acquire the desktop lock", "hoi4_lock", {"action": "acquire", "purpose": "phase 2 exit check"})
        step("status", "hoi4_status", {"screenshot": True},
             lambda d, t: (d.get("game", {}).get("running") is True, f"screen={d.get('screen')}"))
        tl0 = step("telemetry mod installed and in the playset", "hoi4_telemetry", {"action": "status"},
                   lambda d, t: (d.get("installed") and d["playset"]["enabled"],
                                 f"generation {d.get('generation')}, playset {d['playset']['enabled_mods']}"))
        load = step(f"load {args.save}", "hoi4_load", {"save": args.save},
                    lambda d, t: (d.get("date_matches") is True,
                                  f"map {d.get('date_on_screen')} vs save {d.get('save_date')}"))
        if not load["ok"]:
            print("load failed; stopping", flush=True)
            return 1
        step("mark the logs", "hoi4_logs", {"set_mark": "exit2"})
        step(f"run {args.days} days at speed 3, answering popups", "hoi4_time",
             {"action": "step", "days": args.days, "speed": 3, "popups": "first"},
             lambda d, t: (abs(d.get("elapsed_hours", 0) - args.days * 24) <= 1,
                           f"{d.get('start')} -> {d.get('end')}, {len(d.get('popups', []))} popup(s)"))
        tl = step("telemetry is live (records of this game, current generation, no suspect errors)",
                  "hoi4_telemetry", {"action": "status"},
                  lambda d, t: (d.get("active_in_game") is True and d["store"]["records"] > 0
                                and not d.get("suspect_errors"),
                                f"{d['store']['records']} records, last day {d['store'].get('last_day')}, "
                                f"suspect errors {len(d.get('suspect_errors') or [])}"))
        step("telemetry records (day, war, watch)", "hoi4_telemetry", {"action": "read", "kinds": ["day", "watch"]})
        step("save hoi4live_p2_exit", "hoi4_save", {"name": "p2_exit"},
             lambda d, t: (d.get("player") == args.tag, f"{d.get('player')} {d.get('date')} {d.get('size_mb')} MB"))
        st = step(f"hoi4_state {args.tag} (telemetry + save + screen)", "hoi4_state",
                  {"tag": args.tag, "screen": True,
                   "sections": ["overview", "politics", "economy", "military", "diplomacy", "focus", "research",
                                "world", "script"]},
                  lambda d, t: (bool(d.get("overview")), f"sources {list(d.get('sources', {}))}"))
        br = step("hoi4_briefing", "hoi4_briefing", {"log_notes": 3},
                  lambda d, t: (bool(d.get("markdown")), f"{len(d.get('attention', []))} attention item(s)"))
        scr = step("hoi4_screen (top bar, panel, popups)", "hoi4_screen", {"screenshot": True},
                   lambda d, t: (d.get("screen") == "map" and not d.get("topbar_unread"), t.splitlines()[0][:140]))
        topbar_shot = step("top bar at full size", "hoi4_screenshot",
                           {"region": [0, 0, 1000, 70], "max_width": 0, "name": "topbar_left"})
        step("top bar right (date, tension) at full size", "hoi4_screenshot",
             {"region": [1460, 0, 460, 100], "max_width": 0, "name": "topbar_right"})
        tt_fac = step("factories tooltip", "hoi4_hover", {"target": "factories"},
                      lambda d, t: (d.get("found") is True, d.get("title")))
        tt_pp = step("political power tooltip", "hoi4_hover", {"target": "political_power"},
                     lambda d, t: (d.get("found") is True, d.get("title")))
        step("open the research panel (W)", "hoi4_key", {"keys": "w"})
        rp = step("research panel on screen", "hoi4_screen", {"topbar": False, "screenshot": True},
                  lambda d, t: ((d.get("panel") or {}).get("name") == "research", str(d.get("panel"))))
        ro = step("research slots text (the four slot boxes of the panel)", "hoi4_ocr",
                  {"region": [0, 290, 520, 420], "pattern": r"techn"})
        step("close the research panel (W)", "hoi4_key", {"keys": "w"})
        step("open the trade panel (R)", "hoi4_key", {"keys": "r"})
        tp = step("trade panel on screen", "hoi4_screen", {"topbar": False, "screenshot": True},
                  lambda d, t: ((d.get("panel") or {}).get("name") == "trade", str(d.get("panel"))))
        to = step("civilian factories in the trade panel", "hoi4_ocr", {"region": [0, 60, 700, 260]})
        step("close the trade panel (R)", "hoi4_key", {"keys": "r"})
        step("open the production panel (Y)", "hoi4_key", {"keys": "y"})
        pp = step("production panel on screen", "hoi4_screen", {"topbar": False, "screenshot": True},
                  lambda d, t: ((d.get("panel") or {}).get("name") == "production", str(d.get("panel"))))
        po = step("production lines text", "hoi4_ocr", {"region": [0, 60, 700, 1000]})
        step("close the production panel (Y)", "hoi4_key", {"keys": "y"})
        sv_pol = step("save: USA politics", "hoi4_save_query",
                      {"save": "hoi4live_p2_exit", "query": "get", "path": f"countries/{args.tag}/politics"})
        sv_focus = step("save: USA focus", "hoi4_save_query",
                        {"save": "hoi4live_p2_exit", "query": "get", "path": f"countries/{args.tag}/focus"})
        sv_slots = step("save: USA research slots", "hoi4_save_query",
                        {"save": "hoi4live_p2_exit", "query": "get", "path": f"countries/{args.tag}/technology/slots"})
        sv_info = step("save: header", "hoi4_save_query", {"save": "hoi4live_p2_exit", "query": "info"})
        sv_lines = step("save: production lines", "hoi4_save_query",
                        {"save": "hoi4live_p2_exit", "query": "find", "pattern": r"equipment_variant_index",
                         "path": f"countries/{args.tag}/production", "limit": 200})
        sv_gen = step("save: construction queue", "hoi4_save_query",
                      {"save": "hoi4live_p2_exit", "query": "keys", "path": f"countries/{args.tag}/production"})
        errs = step("error.log delta since the mark", "hoi4_logs", {"log": "error", "since": "exit2", "limit": 80})
        gl = step("telemetry lines in game.log since the mark", "hoi4_logs",
                  {"log": "game", "since": "exit2", "grep": "HOI4LIVE \"k\":\"(day|wld)\"", "limit": 20})
        step("game left paused on the map", "hoi4_time", {"action": "status"},
             lambda d, t: (d.get("running") is False, f"{d.get('text')}"))
        step("release the desktop lock", "hoi4_lock", {"action": "release"})
        progress_notes = [n for n in c.notifications if n.get("method") == "notifications/progress"]

    # ------------------------------------------------------------------ consistency checks
    S = st["data"]
    ov, eco, mil, pol = S.get("overview", {}), S.get("economy", {}), S.get("military", {}), S.get("politics", {})
    tb = {k: v.get("value") for k, v in (scr["data"].get("topbar") or {}).items()}
    tbt = {k: v.get("text") for k, v in (scr["data"].get("topbar") or {}).items()}
    raw = None
    savepol = list(sv_pol["data"].get("values", {}).values())[0] if sv_pol["data"].get("values") else {}
    savefocus = list(sv_focus["data"].get("values", {}).values())[0] if sv_focus["data"].get("values") else {}
    saveslots = list(sv_slots["data"].get("values", {}).values())[0] if sv_slots["data"].get("values") else {}
    info = sv_info["data"].get("info", {})
    B = br["data"]
    md = B.get("markdown", "")
    srcs = S.get("sources", {})
    tday = srcs.get("telemetry", {}).get("as_of")
    sdate = srcs.get("save", {}).get("as_of")
    xdate = srcs.get("screen", {}).get("as_of")
    check("the three sources agree on the date (telemetry is the last daily tick, 23:00 the day before)",
          sdate == xdate == info.get("date") and tday and _hours_between(tday, xdate) <= 24,
          f"screen {xdate}, save {sdate} (header {info.get('date')}), telemetry {tday}")
    check("hoi4_state read the save just written", srcs.get("save", {}).get("file") == "hoi4live_p2_exit.hoi4",
          srcs.get("save", {}).get("file"))
    gain = ov.get("political_power_daily") or 0
    pp_tl, pp_save, pp_scr = ov.get("political_power"), savepol.get("political_power"), tb.get("political_power")
    check("political power: telemetry vs save (one daily gain apart at most) vs top bar (rounded)",
          pp_tl is not None and pp_save is not None and abs(pp_save - pp_tl) <= gain + 0.05
          and pp_scr is not None and abs(pp_scr - pp_save) <= 1.0,
          f"telemetry {pp_tl} (+{gain}/day), save {round(pp_save, 3) if pp_save is not None else None}, "
          f"top bar '{tbt.get('political_power')}'")
    for k, label in (("stability", "stability"), ("war_support", "war support")):
        check(f"{label}: telemetry vs top bar", ov.get(k) is not None and tb.get(k) is not None
              and abs(ov[k] - tb[k]) <= 0.011, f"telemetry {ov.get(k)}, top bar '{tbt.get(k)}'")
    mp = mil.get("manpower")
    check("manpower: telemetry vs top bar", mp is not None and tb.get("manpower") is not None
          and abs(mp - tb["manpower"]) <= 0.01 * mp + 50, f"telemetry {mp}, top bar '{tbt.get('manpower')}'")
    fac = eco.get("factories") or {}
    tt = tt_fac["data"].get("lines", [])
    owned_line = next((l for l in tt if re.match(r"^\W*owned:?\s*\d+", l, re.I)), "")
    m_owned = re.search(r"(\d+)", owned_line)
    owned_civ = int(m_owned.group(1)) if m_owned else None
    check("factories on the top bar = owned civilian (tooltip) + military + dockyards (telemetry); telemetry's "
          "owned-factory count agrees within one",
          owned_civ is not None and owned_civ + (fac.get("military") or 0) + (fac.get("dockyards") or 0) == tb.get("factories")
          and abs((fac.get("owned") or 0) - tb.get("factories", 0)) <= 1,
          f"top bar '{tbt.get('factories')}'; tooltip owned civilian {owned_civ} + military {fac.get('military')} + "
          f"dockyards {fac.get('dockyards')}; telemetry owned {fac.get('owned')}, total with trade {fac.get('total')}")
    civ_line = next((l for l in tt if re.search(r"civilian factories in use", l, re.I)), "")
    mil_line = next((l for l in tt if re.search(r"military factories in use", l, re.I)), "")
    m_civ = re.search(r"(\d+)\s*/\s*(\d+)", civ_line)
    m_mil = re.search(r"(\d+)\s*/\s*(\d+)", mil_line)
    check("civilian factories: telemetry vs the factories tooltip (in use / total)",
          m_civ and int(m_civ.group(2)) == fac.get("civilian")
          and int(m_civ.group(2)) - int(m_civ.group(1)) == fac.get("civilian_free"),
          f"telemetry {fac.get('civilian')} civilian, {fac.get('civilian_free')} free; tooltip '{civ_line}'")
    check("military factories: telemetry vs the factories tooltip",
          m_mil and int(m_mil.group(2)) == fac.get("military")
          and int(m_mil.group(2)) - int(m_mil.group(1)) == fac.get("military_free"),
          f"telemetry {fac.get('military')} military, {fac.get('military_free')} free; tooltip '{mil_line}'")
    trade_text = to["data"].get("text", "")
    m_av = re.search(r"available:?\s*(\d+)", trade_text, re.I)
    check("free civilian factories: telemetry vs the trade panel 'Available'",
          m_av and int(m_av.group(1)) == fac.get("civilian_free"),
          f"telemetry {fac.get('civilian_free')}, trade panel '{m_av.group(0) if m_av else trade_text[:80]}'")
    check("convoys: telemetry vs top bar", eco.get("convoys") == tb.get("convoys"),
          f"telemetry {eco.get('convoys')}, top bar '{tbt.get('convoys')}'")
    check("command power: telemetry vs top bar (whole number shown)",
          ov.get("command_power") is not None and tb.get("command_power") is not None
          and abs(ov["command_power"] - tb["command_power"]) <= 1.0,
          f"telemetry {ov.get('command_power')}, top bar '{tbt.get('command_power')}'")
    check("world tension: telemetry vs top bar", ov.get("world_tension") is not None
          and abs((ov.get("world_tension") or 0) - (tb.get("world_tension") or 0)) <= 0.01,
          f"telemetry {ov.get('world_tension')}, top bar '{tbt.get('world_tension')}'")
    check("ruling party: telemetry vs save", savepol.get("ruling_party") == ov.get("ideology")
          or (savepol.get("ruling_party") == "neutrality" and ov.get("ideology") == "non-aligned"),
          f"telemetry '{ov.get('ruling_party')}' ({ov.get('ideology')}), save ruling_party={savepol.get('ruling_party')}")
    focus = S.get("focus", {})
    check("national focus: hoi4_state vs save", focus.get("current") == savefocus.get("current"),
          f"state current={focus.get('current')}, save focus block {savefocus}")
    idle_save = len(saveslots.get("empty", [])) if isinstance(saveslots.get("empty"), list) else (1 if saveslots.get("empty") else 0)
    idle_screen = len(ro["data"].get("lines", []))
    research = S.get("research", {})
    check("idle research slots: hoi4_state vs save vs research panel",
          research.get("idle_slots") == idle_save == idle_screen,
          f"state {research.get('idle_slots')}, save {idle_save} empty slot(s), panel {idle_screen} "
          "'Select a technology to research'")
    nlines = len(sv_lines["data"].get("hits", []))
    lines_state = len(eco.get("production_lines") or [])
    check("production lines: hoi4_state vs save", lines_state == nlines,
          f"state {lines_state} line(s), save {nlines} equipment_variant_index entries")
    prod_lines = [l["text"] for l in po["data"].get("lines", [])]
    first = (eco.get("production_lines") or [{}])[0].get("name") or ""
    best = max(prod_lines, key=lambda t: difflib.SequenceMatcher(None, t.lower(), first.lower()).ratio(), default="")
    ratio = difflib.SequenceMatcher(None, best.lower(), first.lower()).ratio() if best else 0
    check("first production line is on the production panel", bool(first) and ratio >= 0.75,
          f"state first line '{first}'; closest panel text '{best}' (similarity {ratio:.2f})")
    attn = " ".join(a["text"] for a in B.get("attention", []))
    check("briefing: idle focus and research flagged", ("no national focus" in attn) == (not focus.get("current"))
          and (("research slot" in attn) == bool(research.get("idle_slots"))),
          "; ".join(a["text"] for a in B.get("attention", [])[:3]))
    check("briefing: date and pause state from the screen",
          md.startswith(f"# {S.get('name')} ({args.tag}), ") and "paused" in md.splitlines()[0],
          md.splitlines()[0] if md else "")
    check("telemetry left error.log clean", not tl["data"].get("suspect_errors")
          and not any("hoi4live" in (r.get("line") or "").lower() for r in errs["data"].get("lines", [])),
          f"{errs['data'].get('info', {}).get('lines')} new error.log line(s) during the run, none from the telemetry mod")

    # ------------------------------------------------------------------ artifacts
    img_dir = os.path.join(EVIDENCE, "images")
    for i, s in enumerate(steps):
        keep_png = s["tool"] == "hoi4_screenshot" or s["tool"] == "hoi4_hover"
        out = []
        for p in s["images"]:
            base = f"step{i + 1:02d}_{os.path.splitext(os.path.basename(p))[0]}"
            if keep_png:
                os.makedirs(img_dir, exist_ok=True)
                dst = os.path.join(img_dir, base + ".png")
                shutil.copy(p, dst)
                out.append(os.path.relpath(dst, EVIDENCE).replace("\\", "/"))
            else:
                out.append(to_jpeg(p, img_dir, base))
        s["images"] = out
    shutil.rmtree(raw_images, ignore_errors=True)
    with open(os.path.join(EVIDENCE, "state_USA.json"), "w", encoding="utf-8") as f:
        json.dump(S, f, indent=1, default=str)
    with open(os.path.join(EVIDENCE, "briefing.md"), "w", encoding="utf-8") as f:
        f.write(md + "\n")
    ok = all(s["ok"] for s in steps) and all(c["ok"] for c in checks)
    for s in steps:
        s.pop("text", None)
    with open(os.path.join(EVIDENCE, "results.json"), "w", encoding="utf-8") as f:
        json.dump({"ok": ok, "server": init.get("serverInfo"), "protocol": init.get("protocolVersion"),
                   "tools": len(tools), "progress_notifications": len(progress_notes), "checks": checks,
                   "steps": steps}, f, indent=1, default=str)
    write_report(ok, init, tools, steps, checks, md, time.time() - started, args, gl)
    print("exit check", "PASS" if ok else "FAIL", flush=True)
    return 0 if ok else 1


def _hours_between(a, b):
    def dt(t):
        p = [int(x) for x in t.split(".")]
        return datetime.datetime(p[0], p[1], p[2]) + datetime.timedelta(hours=p[3] if len(p) > 3 else 0)
    return abs((dt(b) - dt(a)).total_seconds()) / 3600.0


def write_report(ok, init, tools, steps, checks, md, seconds, args, gl):
    now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    L = [f"# Phase 2 exit check: {'PASS' if ok else 'FAIL'}", "",
         f"Run {now}, {round(seconds)} s, through the MCP stdio server ({init.get('serverInfo', {}).get('name')} "
         f"{init.get('serverInfo', {}).get('version')}, protocol {init.get('protocolVersion')}, {len(tools)} tools). "
         "Driver: `scripts/exit_check_phase2.py`. From `" + args.save + f"` the game ran {args.days} days with the "
         "telemetry mod, was saved as `hoi4live_p2_exit`, and `hoi4_state` and `hoi4_briefing` were produced and "
         "compared with the screen (top bar, tooltips, research/trade/production panels) and the save "
         "(`hoi4_save_query`).", "",
         "## Consistency: model and briefing against the screen and the save", "",
         "| # | claim | result | evidence |", "|---|---|---|---|"]
    for i, c in enumerate(checks, 1):
        L.append(f"| {i} | {c['claim']} | {'match' if c['ok'] else '**MISMATCH**'} | {c['detail'].replace('|', '/')[:260]} |")
    L += ["", "## The briefing", "", "(also in [briefing.md](briefing.md); the full `hoi4_state` result is "
          "[state_USA.json](state_USA.json))", "", "```markdown", md, "```", "",
          "## Steps", "", "| # | step | tool | result | s | detail |", "|---|---|---|---|---|---|"]
    for i, s in enumerate(steps, 1):
        detail = (s["note"] or s["summary"]).replace("|", "/")[:150]
        L.append(f"| {i} | {s['label']} | `{s['tool']}` | {'ok' if s['ok'] else '**FAIL**'} | {s['seconds']} | {detail} |")
    L += ["", "## Telemetry lines written during the run", ""]
    for row in gl["data"].get("lines", [])[:12]:
        L.append(f"- `{row['line'][:400]}`")
    L += ["", "## Screens", ""]
    for i, s in enumerate(steps, 1):
        for img in s["images"]:
            L.append(f"- step {i} ({s['label']}): ![step {i}]({img})")
    with open(os.path.join(EVIDENCE, "exit_check.md"), "w", encoding="utf-8") as f:
        f.write("\n".join(L) + "\n")


if __name__ == "__main__":
    sys.exit(main())
