"""Phase 4 exit check, driven through the MCP stdio server exactly as an agent would use it.

    "C:/Program Files/Python39/python.exe" scripts/exit_check_phase4.py [--save qa_001_fresh_head] [--days 40]

Setup in test mode (console): load the fresh 1936 QA save (USA), declare war on Mexico, place ten infantry
divisions at San Antonio (the USA starts with almost nothing on the border), grant army experience for the
division designer and equipment for training. Then campaign mode only (no console): list the forces, design a
template (duplicate, rename, add a battalion), recruit two divisions of it, form an army from the divisions around
San Antonio with a named general, draw a front line against Mexico and an offensive line through Coahuila, execute
the plan, put a fleet on patrol in a named sea region, save (the baseline), run time and show that the war moves
(the save's war score and captured provinces, telemetry, the map tooltip of the target, the army card and the
plan's execution), deploy the recruits, save through the Save dialog and show the army, its orders and executing
divisions and the fleet's mission in that save. Supplementary
steps exercise the rest of the tool surface (hold/execute, a second army, assign, rename, an army group, an air
mission, removing a battalion). At the end the QA save is loaded again and the game is left paused on the map.

Evidence: docs/evidence/phase4/ (exit_check.md, results.json, transcript.jsonl, images/).
"""

import argparse
import datetime
import json
import os
import re
import shutil
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
sys.path.insert(0, HERE)
from mcp_client import McpClient  # noqa: E402
from exit_check_phase3 import Run, name_in, norm, to_jpeg  # noqa: E402

EVIDENCE = os.path.join(ROOT, "docs", "evidence", "phase4")
TEMPLATE = "Rio Grande Infantry"
ARMY = "Rio Grande Army"        # the name box is 135 px wide: longer names are cut
GENERAL = "George S. Patton"
TARGET = "Coahuila"
ASSEMBLY = "San Antonio"          # a victory point in Texas (province 12782), reachable through the Find view
# create_unit runs in a state scope (Texas, 375); count makes ten
SPAWN = ('create_unit = { division = "division_template = \\"Infantry Division\\" start_experience_factor = 0.5" '
         'owner = USA count = 10 prioritize_location = 12782 }')
EQUIP = ("add_equipment_to_stockpile = { type = infantry_equipment_1 amount = 60000 } "
         "add_equipment_to_stockpile = { type = artillery_equipment_1 amount = 3000 } "
         "add_equipment_to_stockpile = { type = support_equipment_1 amount = 3000 }")
DEPLOY_TO = "Louisiana"


def save_war(d):
    return next(iter((d or {}).get("save_wars") or []), {}) or {}


def war(d):
    return (d or {}).get("war") or {}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--save", default="qa_001_fresh_head")
    ap.add_argument("--days", type=int, default=40, help="days to run after executing (more if nothing moved)")
    ap.add_argument("--max-days", type=int, default=120)
    ap.add_argument("--skip-extra", action="store_true")
    ap.add_argument("--reuse-game", action="store_true",
                    help="play on the running game: no load, no setup, no restore (setup was done by an earlier run)")
    args = ap.parse_args()
    for name in ("images", "raw_images"):
        shutil.rmtree(os.path.join(EVIDENCE, name), ignore_errors=True)
    os.makedirs(EVIDENCE, exist_ok=True)
    for name in ("transcript.jsonl", "results.json", "exit_check.md"):
        if os.path.exists(os.path.join(EVIDENCE, name)):
            os.remove(os.path.join(EVIDENCE, name))
    raw_images = os.path.join(EVIDENCE, "raw_images")
    started = time.time()
    facts = {}
    with McpClient(image_dir=raw_images, transcript=os.path.join(EVIDENCE, "transcript.jsonl")) as c:
        init = c.initialize()
        tools = c.request("tools/list")["result"]["tools"]
        run = Run(c)
        s, ck = run.step, run.check
        T = "USA"
        s("acquire the desktop lock for the run", "hoi4_lock", {"action": "acquire", "purpose": "phase 4 exit check"})
        # ------------------------------------------------------------------ setup (test mode)
        if args.reuse_game:      # the game is running with the war, the divisions and the experience in place
            s("test mode (setup already done)", "hoi4_lock", {"action": "set_mode", "mode": "test"}, None, T)
        else:
            s("test mode for the setup", "hoi4_lock", {"action": "set_mode", "mode": "test"}, None, T)
            load = s(f"fresh 1936 start: load {args.save}", "hoi4_load", {"save": args.save},
                     lambda d, t: (d.get("date_matches") is True, f"map {d.get('date_on_screen')}"), T)
            if not load["ok"]:
                return finish(run, init, tools, facts, started, args, c)
            s("run 1 day so telemetry describes this game", "hoi4_time",
              {"action": "step", "days": 1, "speed": 3, "popups": "first"}, None, T)
            s("map mode names from the buttons (the big buttons move when the air mode exists)", "hoi4_map",
              {"action": "modes", "recalibrate": True},
              lambda d, t: ("Strategic Navy" in (d.get("modes") or {}).values(), str(d.get("modes"))[:150]), T)
            before = s("telemetry before the war", "hoi4_telemetry", {"action": "read", "kinds": ["day"], "limit": 1},
                       None, T)
            s("setup: the USA declares war on Mexico (console)", "hoi4_eval_effect",
              {"effect": "declare_war_on = { target = MEX type = annex_everything }", "scope": "USA"},
              lambda d, t: (not d.get("errors"), f"{d.get('new_error_lines')} error line(s)"), T)
            s("setup: 50 army experience for the division designer (console)", "hoi4_eval_effect",
              {"effect": "army_experience = 50", "scope": "USA"},
              lambda d, t: (not d.get("errors"), "ok"), T)
            s(f"setup: ten infantry divisions at {ASSEMBLY} (console)", "hoi4_eval_effect",
              {"effect": SPAWN, "scope": "375"},
              lambda d, t: (not d.get("errors"), f"{d.get('new_error_lines')} error line(s)"), T)
            s("setup: equipment so recruits can train (console)", "hoi4_eval_effect", {"effect": EQUIP, "scope": "USA"},
              lambda d, t: (not d.get("errors"), "ok"), T)
            s("run 1 day (telemetry sees the war)", "hoi4_time",
              {"action": "step", "days": 1, "speed": 3, "popups": "first"}, None, T)
        # ------------------------------------------------------------------ campaign mode from here
        s("campaign mode (no console from here)", "hoi4_lock", {"action": "set_mode", "mode": "campaign"},
          lambda d, t: ("campaign" in t, t.splitlines()[0][:80]), T)
        lst = s("forces before: armies, templates, unassigned divisions (QA save)", "hoi4_army",
                {"action": "list", "save": args.save},
                lambda d, t: (d.get("divisions", 0) > 0, f"{len(d.get('armies', []))} armies, {d.get('divisions')} "
                                                          f"divisions, {len(d.get('templates', []))} templates"), T)
        facts["armies_before"] = len(lst["data"].get("armies", []))
        s("division templates as the panel shows them", "hoi4_army", {"action": "templates", "save": args.save},
          lambda d, t: (any(x["name"] == "Infantry Division" or name_in("Infantry Division", x["name"])
                            for x in d.get("templates", [])), f"{len(d.get('templates', []))} templates"), T)
        des = s(f"design: duplicate Infantry Division as '{TEMPLATE}' and add an infantry battalion", "hoi4_army",
                {"action": "design", "template": "Infantry Division", "duplicate": True, "new_name": TEMPLATE,
                 "add": [{"type": "infantry", "column": 4}]},
                lambda d, t: (any(name_in(TEMPLATE, n) for n in d.get("templates_after", [])) and
                              any(name_in("Infantry Division", n) for n in d.get("templates_after", [])),
                              f"after: {d.get('templates_after')}"), T)
        facts["design"] = des["data"]
        rec = s(f"recruit two '{TEMPLATE}' divisions, deploying to {DEPLOY_TO}", "hoi4_army",
                {"action": "recruit", "template": TEMPLATE, "count": 2, "state": DEPLOY_TO},
                lambda d, t: ((d.get("line") or {}).get("divisions") == 2 and
                              bool((d.get("line") or {}).get("location")),
                              f"line {d.get('line')}"), T)
        arm = s(f"form '{ARMY}' from the divisions around {ASSEMBLY} under {GENERAL}", "hoi4_army",
                {"action": "create_army", "state": ASSEMBLY, "name": ARMY, "general": GENERAL},
                lambda d, t: (name_in(ARMY, d.get("army") or "") and name_in("Patton", d.get("commander") or ""),
                              f"{d.get('army')}: {d.get('divisions')} divisions, {d.get('commander')}"), T)
        facts["army"] = arm["data"]
        fr = s(f"front line against Mexico (at {TARGET})", "hoi4_orders",
               {"action": "front", "army": ARMY, "target": TARGET},
               lambda d, t: (bool(d.get("changed")), f"toolbar '{d.get('toolbar_after')}'"), T)
        off = s(f"offensive line through {TARGET}", "hoi4_orders",
                {"action": "offensive", "army": ARMY, "target": TARGET},
                lambda d, t: (bool(d.get("changed")), f"red pixels at the target {d.get('red_near_target')}"), T)
        exe = s("execute the plan", "hoi4_orders", {"action": "execute", "army": ARMY},
                lambda d, t: (d.get("after") is True, f"execute animation {d.get('animation')}"), T)
        nav = s("navy: Battle Force on patrol in the Western Seaboard", "hoi4_navy_air",
                {"action": "naval_mission", "fleet": "Battle Force", "mission": "patrol",
                 "region": "Western Seaboard", "near": "California"},
                lambda d, t: (bool(d.get("changed")) and name_in("Western Seaboard", d.get("region") or ""),
                              f"{d.get('region')}: active in {(d.get('fleet_after') or {}).get('active_regions')}"), T)
        s("save the baseline through the Save Game dialog", "hoi4_save", {"name": "p4_start"},
          lambda d, t: (True, t.splitlines()[0][:100]), T)
        st0 = s("orders status before time runs (war from the baseline save)", "hoi4_orders",
                {"action": "status", "army": ARMY, "save": "hoi4live_p4_start"},
                lambda d, t: (d.get("executing") is True, f"toolbar '{d.get('toolbar')}', war {save_war(d)}"), T)
        w0 = war(st0["data"])
        sw0 = save_war(st0["data"])
        facts["war_before"] = w0
        facts["save_war_before"] = sw0
        # ------------------------------------------------------------------ time
        days_run, moved, wn = 0, False, w0
        step_days = args.days
        while days_run < args.max_days:
            r = s(f"run up to {step_days} days at speed 5 (hoi4_advance: war stop conditions)", "hoi4_advance",
                  {"days": step_days, "speed": 5, "popups": "first", "chunk_days": 5,
                   "stop_on": ["territory_gained", "territory_lost", "war_ended", "losing", "front_changed"]},
                  lambda d, t: (bool(d.get("end")), f"{d.get('start')} -> {d.get('end')} "
                                                    f"({d.get('days_run')} d): "
                                                    f"{[x['code'] for x in d.get('stopped_by', [])]}, changes "
                                                    f"{d.get('changes')}"), T, retry=True)
            days_run += step_days
            facts.setdefault("advance", []).append({k: r["data"].get(k) for k in ("start", "end", "days_run",
                                                                                 "stopped_by", "changes")})
            stn = s(f"orders status after {days_run} days (telemetry, map tooltip at {TARGET}, card)", "hoi4_orders",
                    {"action": "status", "army": ARMY, "target": TARGET, "save": "none"},
                    lambda d, t: (True, f"war {war(d)}; tooltip {(d.get('map_tooltip') or [])[:3]}"), T)
            wn = war(stn["data"])
            sp = lambda w: max([x.get("surrender") or 0 for x in w.get("wars") or []] or [0])
            moved = ((wn.get("war_score") or 0) != (w0.get("war_score") or 0) or sp(wn) > sp(w0) or
                     (wn.get("states_controlled") or 0) != (w0.get("states_controlled") or 0))
            if moved:
                break
            step_days = 20
        facts["war_after"] = wn
        facts["days_run"] = days_run
        tip = stn["data"].get("map_tooltip") or []
        facts["map_tooltip"] = tip
        ck(T, "telemetry: Mexico's surrender progress or our controlled states moved",
           moved, f"before {w0}; after {days_run} days {wn}")
        ck(T, "the army still executes its plan (execute button animation)", stn["data"].get("executing") is True,
           f"animation {stn['data'].get('execute_animation')}")
        ck(T, f"the map tooltip at {TARGET} was read", bool(tip), "; ".join(tip[:3]))
        dep = s(f"deploy the '{TEMPLATE}' divisions in training (early: at least 20 % trained)", "hoi4_army",
                {"action": "deploy", "template": TEMPLATE, "early": True, "state": DEPLOY_TO},
                lambda d, t: (bool(d.get("changed")), f"tooltip before: {(d.get('deploy_tooltip') or '')[:80]}"), T,
                refusal_ok=lambda d, t: ("nothing to deploy" in t, t.splitlines()[0][:120]))
        sv = s("save through the Save Game dialog", "hoi4_save", {"name": "p4_exit"},
               lambda d, t: (bool(d.get("path") or d.get("name")), t.splitlines()[0][:100]), T)
        al = s("forces in that save", "hoi4_army", {"action": "list", "save": "hoi4live_p4_exit", "screen": False},
               lambda d, t: (len(d.get("armies", [])) > facts["armies_before"], t.splitlines()[0][:160]), T)
        army_s = next((a for a in al["data"].get("armies", []) if name_in(ARMY, a.get("name") or "")), None)
        stx = s("orders status from the exit save (war score and captured provinces)", "hoi4_orders",
                {"action": "status", "army": ARMY, "save": "hoi4live_p4_exit"},
                lambda d, t: (bool(d.get("save_wars")), str(save_war(d))[:160]), T)
        sw1 = save_war(stx["data"])
        facts["save_war_after"] = sw1
        ck(T, "the save's war score moved: our score or our captured provinces grew since the baseline save",
           (sw1.get("score_breakdown_us") or 0) > (sw0.get("score_breakdown_us") or 0) or
           (sw1.get("provinces_captured_by_us") or 0) > (sw0.get("provinces_captured_by_us") or 0),
           f"baseline {sw0}; exit {sw1}")
        facts["army_in_save"] = army_s
        ck(T, f"the save has the army '{ARMY}'", bool(army_s), army_s and army_s.get("name"))
        ck(T, f"{GENERAL} commands it in the save", bool(army_s) and name_in("Patton", army_s.get("leader") or ""),
           army_s and army_s.get("leader"))
        types = [o["type"] for o in (army_s or {}).get("orders", [])]
        ck(T, "it has a front line and an offensive line in the save", "front_line" in types and "offensive" in types,
           types)
        ck(T, "its divisions execute the orders in the save", bool(army_s) and army_s.get("executing_divisions", 0) > 0,
           army_s and f"{army_s.get('executing_divisions')} of {army_s.get('divisions')}")
        ck(T, f"the template '{TEMPLATE}' is in the save",
           any(name_in(TEMPLATE, t["name"]) for t in al["data"].get("templates", [])),
           [t["name"] for t in al["data"].get("templates", [])])
        fl = s("fleets in that save", "hoi4_navy_air", {"action": "list", "save": "hoi4live_p4_exit"},
               lambda d, t: (len(d.get("fleets", [])) > 0, t.splitlines()[0][:150]), T)
        bf = next((f for f in fl["data"].get("fleets", []) if f["name"] == "Battle Force"), None)
        ck(T, "Battle Force's task forces have a mission with a region in the save",
           bool(bf) and any(tf.get("regions") or tf.get("mission") for tf in bf["task_forces"]),
           bf and [(tf["name"], tf.get("mission"), tf.get("regions")) for tf in bf["task_forces"]][:4])
        stt = s("hoi4_state military and diplomacy from that save", "hoi4_state",
                {"sections": ["military", "diplomacy"], "save": "hoi4live_p4_exit"},
                lambda d, t: (True, t.splitlines()[0][:150]), T)
        facts["state"] = stt["data"]
        br = s("briefing from that save", "hoi4_briefing", {"save": "hoi4live_p4_exit", "remember": False},
               lambda d, t: (bool(d.get("markdown")), f"{len(d.get('attention', []))} attention item(s)"), T)
        facts["briefing"] = br["data"].get("markdown")
        # ------------------------------------------------------------------ supplementary
        if not args.skip_extra:
            extra(run, T)
        # ------------------------------------------------------------------ restore
        if not args.reuse_game:
            s("test mode to restore the QA save", "hoi4_lock", {"action": "set_mode", "mode": "test"})
            s(f"restore: load {args.save} (the game is left as it was found)", "hoi4_load", {"save": args.save},
              lambda d, t: (d.get("date_matches") is True, f"{d.get('date_on_screen')}"))
        s("game left paused on the map", "hoi4_time", {"action": "status"},
          lambda d, t: (d.get("running") is False, f"{d.get('text')}"))
        s("release the desktop lock", "hoi4_lock", {"action": "release"})
        return finish(run, init, tools, facts, started, args, c)


def extra(run, T):
    s, ck = run.step, run.check
    X = "extra"
    s("hold the plan", "hoi4_orders", {"action": "hold", "army": ARMY},
      lambda d, t: (d.get("after") is False, f"animation {d.get('animation')}"), X)
    s("execute it again", "hoi4_orders", {"action": "execute", "army": ARMY},
      lambda d, t: (d.get("after") is True, f"animation {d.get('animation')}"), X)
    s("a second army from the divisions in Georgia (the US state, not the country's), first listed general",
      "hoi4_army", {"action": "create_army", "state": "Georgia", "name": "Second Army", "general": ""},
      lambda d, t: (name_in("Second Army", d.get("army") or "") and bool(d.get("commander")),
                    f"{d.get('army')}: {d.get('divisions')} div, {d.get('commander')}"), X)
    s("assign the divisions in Illinois to it", "hoi4_army",
      {"action": "assign", "army": "Second Army", "state": "Illinois"},
      lambda d, t: ((d.get("divisions_after") or 0) > (d.get("divisions_before") or 0),
                    f"{d.get('divisions_before')} -> {d.get('divisions_after')}"), X)
    s("rename it", "hoi4_army", {"action": "rename", "army": "Second Army", "new_name": "Potomac Army"},
      lambda d, t: (name_in("Potomac", (d.get("after") or {}).get("name") or ""), str((d.get("after") or {}).get("name"))), X)
    s("another general for it", "hoi4_army", {"action": "general", "army": "Potomac Army",
                                              "general": "Dwight D. Eisenhower"},
      lambda d, t: (name_in("Eisenhower", (d.get("after") or {}).get("commander") or ""),
                    str((d.get("after") or {}).get("commander"))), X)
    s("an army group of both armies", "hoi4_army", {"action": "army_group", "armies": [ARMY, "Potomac Army"]},
      lambda d, t: (bool(d.get("changed")), str(d.get("panel_head"))[:80]), X)
    s("air: the bombers at the California air base on air superiority over California", "hoi4_navy_air",
      {"action": "air_mission", "base": "California", "mission": "air_superiority"},
      lambda d, t: (bool(d.get("changed")), str(d.get("regions"))[:100]), X)
    s(f"design: remove the added battalion from '{TEMPLATE}' (column 4, row 1)", "hoi4_army",
      {"action": "design", "template": TEMPLATE, "remove": [{"column": 4, "row": 1}]},
      lambda d, t: (True, str(d.get("steps"))[:120]), X,
      refusal_ok=lambda d, t: ("cannot be saved" in t, t.splitlines()[0][:140]))
    s("an offensive where the army has no front is refused", "hoi4_orders",
      {"action": "offensive", "army": "Potomac Army", "target": "Chihuahua"},
      None, X, refusal_ok=lambda d, t: ("no offensive line" in t or "front" in t, t.splitlines()[0][:140]))


def finish(run, init, tools, facts, started, args, c):
    progress = [n for n in c.notifications if n.get("method") == "notifications/progress"]
    img_dir = os.path.join(EVIDENCE, "images")
    raw_images = os.path.join(EVIDENCE, "raw_images")
    for i, st in enumerate(run.steps):
        st["images"] = [to_jpeg(p, img_dir, f"step{i + 1:03d}_{os.path.splitext(os.path.basename(p))[0]}")
                        for p in st["images"]]
    shutil.rmtree(raw_images, ignore_errors=True)
    core = [st for st in run.steps if st["country"] != "extra"]
    ok = all(st["ok"] for st in core) and all(ck["ok"] for ck in run.checks if ck["country"] != "extra")
    extra_ok = all(st["ok"] for st in run.steps if st["country"] == "extra")
    with open(os.path.join(EVIDENCE, "results.json"), "w", encoding="utf-8") as f:
        json.dump({"ok": ok, "extra_ok": extra_ok, "server": init.get("serverInfo"),
                   "protocol": init.get("protocolVersion"), "tools": len(tools),
                   "progress_notifications": len(progress), "checks": run.checks, "facts": facts,
                   "steps": run.steps}, f, indent=1, default=str)
    write_report(ok, extra_ok, init, tools, run, facts, time.time() - started, args)
    print("exit check", "PASS" if ok else "FAIL", "| supplementary", "PASS" if extra_ok else "FAIL", flush=True)
    return 0 if ok else 1


def write_report(ok, extra_ok, init, tools, run, facts, seconds, args):
    now = datetime.datetime.now().strftime("%Y-%m-%d %H:%M")
    L = [f"# Phase 4 exit check: {'PASS' if ok else 'FAIL'} (supplementary steps: {'PASS' if extra_ok else 'FAIL'})", "",
         f"Run {now}, {round(seconds / 60)} min, through the MCP stdio server "
         f"({init.get('serverInfo', {}).get('name')} {init.get('serverInfo', {}).get('version')}, protocol "
         f"{init.get('protocolVersion')}, {len(tools)} tools). Driver: `scripts/exit_check_phase4.py`. Setup in test "
         f"mode: `{args.save}` (USA, 1 Jan 1936), the USA declares war on Mexico and gets 50 army experience through "
         "the console. Everything after that in campaign mode (no console): a template designed, two divisions "
         f"recruited, an army formed in Texas under {GENERAL}, a front line against Mexico and an offensive line "
         f"through {TARGET}, the plan executed, a fleet sent on patrol in a named sea region, time run until the war "
         "moved, the recruits deployed, a save through the Save Game dialog read back.", "",
         "## Claims", "", "| # | claim | result | evidence |", "|---|---|---|---|"]
    for i, c in enumerate(run.checks, 1):
        L.append(f"| {i} | {c['claim']} | {'match' if c['ok'] else '**MISMATCH**'} | "
                 f"{str(c['detail']).replace('|', '/')[:260]} |")
    L += ["", "## The war", "", f"- baseline save (plan just executed): `{facts.get('save_war_before')}`",
          f"- exit save: `{facts.get('save_war_after')}`",
          f"- telemetry before the plan ran: `{facts.get('war_before')}`",
          f"- telemetry after {facts.get('days_run')} days: `{facts.get('war_after')}`",
          f"- map tooltip at {TARGET}: {'; '.join(facts.get('map_tooltip') or [])}"]
    if facts.get("briefing"):
        L += ["", "## Briefing from the exit save", "", "```markdown", facts["briefing"], "```"]
    L += ["", "## Steps", "", "| # | part | step | tool | result | s | detail |", "|---|---|---|---|---|---|---|"]
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
