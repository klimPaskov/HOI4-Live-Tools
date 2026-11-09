"""Technology eligibility under the two readings of `path = { leads_to_tech = X }`.

A technology with several parents (more than one technology leads to it) is read differently by
the repo's two readers:

  * "all": native/src/tech_tree.rs `startable` needs EVERY parent researched;
  * "any": hoi4_live/gamedata/tech.py `status` (and validate_plan before this library) needs at
    least ONE parent.

Which one matches the game is not verified live yet (design finding 3), so callers report both
where they differ. `dependencies = { A = 1 }` is needed in full under both readings.

A technology record is the dict Game.techs() returns: {"parents": set, "deps": set, ...}.
"""

RULES = ("all", "any")


def _record(tech, techs):
    if isinstance(tech, dict):
        return tech
    if techs is None or tech not in techs:
        raise KeyError(f"unknown technology {tech!r}")
    return techs[tech]


def missing(tech, done, rule="all", techs=None):
    """Sorted prerequisites that keep `tech` from starting under `rule` ("all" or "any"): the
    missing parents (all of them under "any" when none is done) plus missing dependencies."""
    if rule not in RULES:
        raise ValueError(f"rule must be one of {RULES}, not {rule!r}")
    t = _record(tech, techs)
    done = set(done)
    parents = set(t.get("parents") or ())
    out = set(t.get("deps") or ()) - done
    if parents:
        if rule == "all":
            out |= parents - done
        elif not (parents & done):
            out |= parents
    return sorted(out)


def eligible(tech, done, rule="all", techs=None):
    """True when `tech` (a record, or a key with `techs`) can start with `done` researched."""
    return not missing(tech, done, rule, techs)


def multi_parent(techs):
    """Keys of the technologies with more than one parent (where the readings can differ)."""
    return sorted(k for k, t in techs.items() if len(t.get("parents") or ()) > 1)


def readings(tech, done, techs=None):
    """{"all": missing-list, "any": missing-list} for one technology."""
    return {r: missing(tech, done, r, techs) for r in RULES}
