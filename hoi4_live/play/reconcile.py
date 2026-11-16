"""Day-one reconcile: the plan's names checked against the live game before the executor starts.

The plan is written from the game files and the strategist's dossier; the live game names some things its
own way (a template renamed by a focus, "12° Stormo" for "12 Stormo", a wing created at runtime). With
read tools only (no lease, no input) this checks, against the names the plan uses:
  * division templates  (army_layout[].templates, training template(s), templates[].base)
                        vs hoi4_division_templates
  * generals            (army_layout[].general, armies.generals, army_group[].marshal) vs hoi4_commanders
  * air wings           (air_plan[].wing: a live name, or "<variant> at <state id>", which expands to one
                        entry per matching wing) vs hoi4_air (the memory read; hoi4_air_wings opens a panel
                        and needs a lease)
  * production targets  (production[].remove / replace) vs hoi4_production_lines
  * first research block (research_path, earliest year) vs hoi4_technology_list {available: true}
A name that differs from exactly one live name only in case, accents, punctuation or the degree sign is
patched; everything else is listed as unresolved. `run(folder, call)` writes the patched plan.json (the
original kept as plan.pre-reconcile.json) and reconcile.json. Used by the play supervisor before it switches
the executor on, and by scripts/strategist/reconcile.py.
"""
import json
import os
import re
import shutil
import unicodedata


def norm(name):
    """Case, accents, punctuation and the degree sign do not distinguish names."""
    s = unicodedata.normalize("NFKD", str(name)).replace("°", " ").replace("º", " ")
    s = "".join(c for c in s if not unicodedata.combining(c))
    return re.sub(r"[^a-z0-9]+", " ", s.casefold()).strip()


def live_names(call):
    """{kind: [names]} and info, from read tools only."""
    names, info = {}, {}
    t = call("hoi4_division_templates", {})
    names["template"] = [r.get("template") for r in t.get("templates") or [] if r.get("template")]
    c = call("hoi4_commanders", {})
    names["general"] = [r.get("name") for r in c.get("commanders") or [] if r.get("name")]
    # Wing names come from hoi4_air, the memory read: hoi4_air_wings opens the Air Forces Overview and takes
    # a lease, so the supervisor's lease-free reconcile failed on it (invalid_arguments) and checked nothing.
    # Wing names are made at runtime, so plans may also select wings as "<variant> at <state id>"
    # (the dossier knows the start OOB's variants and bases).
    air = call("hoi4_air", {})
    names["wing"] = [r.get("wing") for r in air.get("wings") or [] if r.get("wing")]
    base_state = {}
    for b in air.get("air_bases") or []:
        for name in b.get("wings") or []:
            base_state[name] = b.get("state_id")
    # (wing, variant, state, airframe type): a selector may name the variant or the airframe id
    names["wing_by_selector"] = [(r.get("wing"), r.get("variant"), base_state.get(r.get("wing")),
                                  r.get("aircraft_type"))
                                 for r in air.get("wings") or [] if r.get("wing")]
    lines = call("hoi4_production_lines", {})
    names["equipment"] = sorted({r.get("equipment_id") for r in lines.get("lines") or [] if r.get("equipment_id")})
    info["production_lines_at_start"] = [
        {"equipment": r.get("equipment_id"),
         "factories": (r.get("factory_counters") or {}).get("active_factories_raw_signed")}
        for r in lines.get("lines") or []]
    techs = call("hoi4_technology_list", {"available": True})
    names["tech_available"] = list(techs.get("technologies") or [])
    return names, info


def references(plan):
    """[(kind, path, getter, setter)] for every live-checked name in the plan."""
    refs = []

    def add(kind, container, key, path):
        value = container.get(key)
        if isinstance(value, str) and value:
            refs.append((kind, path, value, lambda new, c=container, k=key: c.__setitem__(k, new)))
        elif isinstance(value, list):
            for i, v in enumerate(value):
                if isinstance(v, str) and v:
                    refs.append((kind, "%s[%d]" % (path, i), v, lambda new, c=value, i=i: c.__setitem__(i, new)))

    for i, e in enumerate(plan.get("army_layout") or []):
        if isinstance(e, dict):
            add("template", e, "templates", "army_layout[%d].templates" % i)
            add("general", e, "general", "army_layout[%d].general" % i)
    training = plan.get("training")
    for i, e in enumerate(training if isinstance(training, list) else [training] if training else []):
        if isinstance(e, dict):
            add("template", e, "template", "training[%d].template" % i)
    for i, e in enumerate(plan.get("templates") or []):
        if isinstance(e, dict):
            add("template", e, "base", "templates[%d].base" % i)
    for i, e in enumerate(plan.get("army_group") or []):
        if isinstance(e, dict):
            add("general", e, "marshal", "army_group[%d].marshal" % i)
    armies = plan.get("armies") or {}
    if isinstance(armies, dict):
        gens = armies.get("generals")
        if isinstance(gens, list):
            add("general", armies, "generals", "armies.generals")
        elif isinstance(gens, dict):
            for k in list(gens):
                add("general", gens, k, "armies.generals.%s" % k)
    for i, e in enumerate(plan.get("air_plan") or []):
        if isinstance(e, dict):
            add("wing", e, "wing", "air_plan[%d].wing" % i)
    for i, e in enumerate(plan.get("production") or []):
        if isinstance(e, dict):
            for key in ("remove", "replace"):
                if isinstance(e.get(key), str):
                    add("equipment", e, key, "production[%d].%s" % (i, key))
    # a template the plan itself creates (templates[].name) does not exist on day one: the executor
    # makes it on its date, so a layout or training line naming it is not a dangling reference
    planned = {e.get("name") for e in plan.get("templates") or [] if isinstance(e, dict)}
    return [r for r in refs if not (r[0] == "template" and r[2] in planned)]


def first_block(plan):
    blocks = [b for b in plan.get("research_path") or [] if isinstance(b, dict) and b.get("techs")]
    if not blocks:
        return []
    year = min(int(b.get("year", 9999)) for b in blocks)
    return [t for b in blocks if int(b.get("year", 9999)) == year for t in b["techs"]]


SELECTOR = re.compile(r"^\s*(.+?)\s+at\s+(\d+)\s*$")


def expand_wing_selectors(plan, names, patched, unresolved):
    """Replace air_plan entries whose wing is "<variant or airframe> at <state>" by one entry per live wing."""
    entries = plan.get("air_plan")
    if not isinstance(entries, list):
        return
    out = []
    for i, e in enumerate(entries):
        m = SELECTOR.match(e.get("wing", "")) if isinstance(e, dict) and isinstance(e.get("wing"), str) else None
        if not m or e["wing"] in (names.get("wing") or []):
            out.append(e)
            continue
        variant, state = norm(m.group(1)), int(m.group(2))
        sel = [tuple(r) + (None,) * (4 - len(r)) for r in names.get("wing_by_selector") or []]
        hits = [w for w, v, st, t in sel
                if st == state and ((v and norm(v) == variant) or (t and norm(t) == variant))]
        if not hits:
            unresolved.append({"kind": "wing", "path": "air_plan[%d].wing" % i, "name": e["wing"],
                               "candidates": ["%s at %s" % (v, st) for _, v, st, _ in sel][:8]})
            out.append(e)
            continue
        for w in hits:
            out.append(dict(e, wing=w, selector=e["wing"]))
        patched.append({"kind": "wing", "path": "air_plan[%d].wing" % i, "from": e["wing"], "to": hits})
    plan["air_plan"] = out


def reconcile(plan, names):
    """(patched, unresolved, research) and patches `plan` in place."""
    patched, unresolved = [], []
    expand_wing_selectors(plan, names, patched, unresolved)
    for kind, path, value, setter in references(plan):
        live = names.get(kind) or []
        if not live:
            continue  # the read listed none of this kind: nothing to judge the name against
        if value in live or (kind == "wing" and SELECTOR.match(value)):
            continue  # selectors were resolved (or listed) by expand_wing_selectors
        matches = [n for n in live if norm(n) == norm(value)]
        if len(matches) == 1:
            setter(matches[0])
            patched.append({"kind": kind, "path": path, "from": value, "to": matches[0]})
        else:
            unresolved.append({"kind": kind, "path": path, "name": value,
                               "candidates": matches or sorted(live, key=lambda n: _distance(n, value))[:5]})
    available = set(names.get("tech_available") or [])
    block = first_block(plan)
    # an empty list is a read that listed nothing, not a game where nothing is researchable
    research = {"first_block": block, "available_now": [t for t in block if t in available],
                "not_yet_available": [t for t in block if t not in available] if available else []}
    return patched, unresolved, research


def _distance(a, b):
    a, b = norm(a), norm(b)
    return 0 if a == b else len(set(a.split()) ^ set(b.split())) + abs(len(a) - len(b)) / 100.0



def run(folder, call, dry_run=False):
    """Reconcile RUN/plan.json against the live game through `call(tool, args) -> body` (raises on a failed
    read). Writes reconcile.json; patches plan.json unless dry_run. Returns the report."""
    plan_path = os.path.join(folder, "plan.json")
    with open(plan_path, encoding="utf-8") as f:
        plan = json.load(f)
    names, info = live_names(call)
    patched, unresolved, research = reconcile(plan, names)
    if patched and not dry_run:
        backup = os.path.join(folder, "plan.pre-reconcile.json")
        if not os.path.exists(backup):
            shutil.copy2(plan_path, backup)
        tmp = plan_path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump(plan, f, indent=1, ensure_ascii=False)
            f.write("\n")
        os.replace(tmp, plan_path)
    report = {"ok": not unresolved, "patched": patched, "unresolved": unresolved, "research": research,
              "info": info, "dry_run": dry_run}
    with open(os.path.join(folder, "reconcile.json"), "w", encoding="utf-8") as f:
        json.dump(report, f, indent=1, ensure_ascii=False)
    return report
