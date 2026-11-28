"""Decision lines: every evaluator's answer as one JSON object per decision, for the plan generator and the companion.

Shape (one object per line, RUN/evaluations.jsonl or stdout with evaluate.py --lines):
    {"decision": "armour template vs POL", "chosen": "10 medium_armor, 5 cavalry", "value": 0.0519,
     "unit": "provinces a day x sqrt(width / IC)", "alternatives": [{"option": "...", "value": 0.0506,
     "terms": ["..."]}], "reasoning": "...", "inputs": {...}, "evaluator": "template", "country": "GER",
     "date": "1939.9.1", "command": "evaluate.py template armour --country GER ...", "source": "computed"}

`decision`, `chosen`, `value`, `unit`, `alternatives` (option, value, terms), `reasoning` and `inputs` are the
companion's keys (companion engine/explain.py); `evaluator`, `country`, `date`, `command` and `source` let the plan
generator quote the reproducing command beside a "[C] computed" entry. An evaluator without an adapter here gives one
line with its whole answer under `inputs` (never silently nothing).
"""
import json
import os
import shlex


def _opt(option, value, terms=()):
    return {"option": option, "value": value, "terms": [str(t) for t in terms]}


def _bats(t):
    return ", ".join(f"{n} {k}" for k, n in t["battalions"].items())


def design(r):
    if "speed_floor" in r:  # land
        ds = r.get("designs") or []
        if not ds:
            return [{"decision": f"{r['role']} design", "chosen": "none", "value": None, "unit": "score",
                     "alternatives": [], "reasoning": f"no legal design: {r.get('rejected_by')}"}]
        top = ds[0]
        name = lambda d: f"{d['base']}: " + ", ".join(sorted(d["modules"].values()))  # noqa: E731
        return [{"decision": f"{r['role']} design", "chosen": name(top), "value": top["score"],
                 "unit": "value / sqrt(IC)", "alternatives": [_opt(name(d), d["score"], [f"{k} {v}" for k, v in
                                                                                          d["stats"].items()])
                                                              for d in ds[1:]],
                 "reasoning": (f"speed floor {r['speed_floor']:g} km/h ({r['speed_floor_from']}); stats "
                               f"{top['stats']}; upgrades {top['upgrades']}"),
                 "inputs": {"enemy": r["enemy"].get("chosen"), "techs": r.get("techs")}}]
    ds = r.get("designs") or []
    if not ds:
        return [{"decision": f"{r['role']} design", "chosen": "none", "value": None, "unit": "score",
                 "alternatives": [], "reasoning": str(r.get("note") or r.get("rejected_by"))}]
    name = lambda d: f"{d['base']}: " + ", ".join(f"{k}={m}" for k, m in sorted(d["modules"].items()))  # noqa: E731
    return [{"decision": f"{r['role']} design", "chosen": name(ds[0]), "value": ds[0]["score"], "unit": "score",
             "alternatives": [_opt(name(d), d["score"]) for d in ds[1:]],
             "reasoning": str(r.get("method") or r.get("note") or ""), "inputs": {"techs": r.get("techs")}}]


def template(r):
    ts = r.get("templates") or []
    if not ts:
        return [{"decision": f"{r['role']} template", "chosen": "none", "value": None, "unit": "score",
                 "alternatives": [], "reasoning": "no fieldable template"}]
    top = ts[0]
    p = top["parts"]
    if "provinces_per_day" in p:
        unit = "provinces a day x sqrt(width / IC)"
        why = (f"{p['provinces_per_day']:g} provinces a day (battle {p['battle_hours']:g} h + march "
               f"{p['march_hours']:g} h at {p['speed']:g} km/h), R {p['R']:g}, supply ratio {p['supply_ratio']:g}")
    elif "R" in p:
        unit = "R x sqrt(width / IC)"
        why = f"R {p['R']:g}: our endurance {p['our_hours']:g} h against theirs {p['their_hours']:g} h"
    else:
        unit, why = "score", str(p)
    alts = [_opt(f"{_bats(t)}; support {t['support']}", t["score"], [f"width {t['width']:g}", f"IC {t['ic']:g}"])
            for t in ts[1:]]
    alts += [_opt(f"width {t['target_width']}: {_bats(t)}", t["score"], ["other width"])
             for t in r.get("other_widths") or []]
    return [{"decision": f"{r['role']} template", "chosen": f"width {top['width']:g}: {_bats(top)}; regimental "
                                                             f"{top['regimental']}; support {top['support']}",
             "value": top["score"], "unit": unit, "alternatives": alts,
             "reasoning": f"{why}; {r.get('width_why', '')}; chosen width {r.get('chosen_width')}",
             "inputs": {"enemy_template": r.get("enemy_template"), "designs": r.get("designs_used"),
                        "axis": r.get("axis"), "ic": top["ic"], "logistics": top.get("logistics")}}]


def construction(r):
    return [{"decision": "civilian factories until", "chosen": str(r.get("decision") or "no war date given"),
             "value": r["k"], "unit": "military factories per civilian factory (k)", "alternatives": [],
             "reasoning": (f"k = ({r['cost_civ']:g}/(1{r['b_civ']:+g})) / ({r['cost_mil']:g}/(1{r['b_mil']:+g})); "
                           f"a civilian factory pays when more than {r['standing_days']:g} days remain (factories "
                           f"standing) or {r['output_days']:g} (output)"),
             "inputs": {"factories": r["factories"], "war_date": r.get("war_date")}}]


def research(r):
    out = []
    for i, sl in enumerate(r.get("picks") or [], 1):
        for p in sl[:1]:
            out.append({"decision": f"research slot {i} first pick", "chosen": p["tech"], "value": p["value"],
                        "unit": "factory-years", "alternatives": [_opt(t, v, ["chain rate per day"])
                                                                  for t, v in p.get("next_best") or []],
                        "reasoning": (f"{p['days']} days, chain rate {p['rate']:g}/day via "
                                      f"{' > '.join(p['chain'])}; parts {p['parts']}"
                                      + (f"; not valued: {p['unvalued']}" if p.get("unvalued") else "")),
                        "inputs": {"slot": i, "order": [x["tech"] for x in sl]}})
    return out


def variant(r):
    out = []
    for x in r.get("clicks") or []:
        lv = {row["level"]: row for row in x["levels"]}
        out.append({"decision": f"cost clicks on {x['equipment']}", "chosen": f"{x['chosen']} x {x['upgrade']}",
                    "value": lv[x["chosen"]]["value_per_ic"], "unit": "value per IC",
                    "alternatives": [_opt(f"{row['level']} clicks", row["value_per_ic"],
                                          [f"{row['ic']:g} IC", f"reliability {row['reliability']:g}",
                                           f"{row['xp']:g} XP"]) for row in x["levels"] if row["level"] != x["chosen"]],
                    "reasoning": x["why"], "inputs": {"levels": x["levels"]}})
    return out


def convert(r):
    if not r.get("base") or r.get("note"):
        return [{"decision": f"base and convert {r.get('family')}", "chosen": "none", "value": None, "unit": "IC",
                 "alternatives": [], "reasoning": str(r.get("note"))}]
    b, f = r["base"], r["final"]
    return [{"decision": f"base and convert {r['family']} to the {r['to_date']} design",
             "chosen": f"base {b['base']}: " + ", ".join(sorted(b["modules"].values())),
             "value": r["recovered_per_base"], "unit": "IC recovered per base vehicle",
             "alternatives": [_opt("build the final design new", r["ic_final"], ["IC per vehicle"]),
                              _opt("convert a base vehicle", r["conversion_ic"], [f"{r['conversion_xp']:g} XP"])],
             "reasoning": r["why"], "inputs": {"final": {"base": f["base"], "modules": f["modules"],
                                                         "upgrades": f["upgrades"]},
                                               "conversion_rows": r["conversion_rows"]}}]


ADAPTERS = {"design": design, "template": template, "construction": construction, "research": research,
            "variant": variant, "convert": convert}


def lines(cmd, data, argv=None, sit=None):
    """[dict] for one evaluate.py answer (see the module doc)."""
    fn = ADAPTERS.get(cmd)
    rows = fn(data) if fn else [{"decision": cmd, "chosen": "see inputs", "value": None, "unit": "score",
                                 "alternatives": [], "reasoning": "no adapter yet: the whole answer is under inputs",
                                 "inputs": data}]
    command = "python3 scripts/hoi4facts/evaluate.py " + " ".join(shlex.quote(a) for a in (argv or [])
                                                                  if a not in ("--lines", "--json"))
    for row in rows:
        row.setdefault("inputs", {})
        row.update({"evaluator": cmd, "country": (data or {}).get("country") or (sit.tag if sit else None),
                    "date": (data or {}).get("date") or (sit.date if sit else None), "command": command.strip(),
                    "source": "computed"})
    return rows


def write(path, rows):
    """Append rows to a JSON-lines file (RUN/evaluations.jsonl), creating its folder."""
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, "a", encoding="utf-8") as fh:
        for row in rows:
            fh.write(json.dumps(row, ensure_ascii=False, default=lambda o: sorted(o) if isinstance(o, set) else str(o))
                     + "\n")
