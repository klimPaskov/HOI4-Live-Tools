"""One state model from three sources, each with its freshness:

    telemetry  HOI4LIVE lines of game.log (the companion mod): current to the last in-game day, numbers only
    save       a plain-text save: deep (focus, research, production, construction, laws, wars, states), as old as
               the save; 'latest' picks the newest save whose player is the country, 'fresh' writes one now
    screen     the top bar read from the screen (no mod needed): political power, stability, war support,
               manpower, factories, fuel, supply, convoys, command power, experience, world tension, date

collect() returns {"tag", "name", "sources": {...freshness...}, "<section>": {..., "_sources": {...}}}. Every
section lists, per source, its in-game date and the fields it provided, so the freshness of each field is known.
"""

import datetime
import os
import time

from ..errors import ToolError
from ..gamedata import loc as loc_mod
from ..gamedata import scripts
from . import model, queries

SECTIONS = ("overview", "politics", "economy", "military", "diplomacy", "focus", "research", "production",
            "construction", "decisions", "states", "world", "script")
DEFAULT_SECTIONS = ("overview", "politics", "economy", "military", "diplomacy", "focus", "research")
SAVE_SECTIONS = {"overview": ["overview"], "politics": ["politics"], "economy": ["production", "construction"],
                 "military": ["military"], "diplomacy": ["diplomacy"], "focus": ["focus"], "research": ["research"],
                 "production": ["production"], "construction": ["construction"], "decisions": ["decisions"],
                 "states": ["states"], "world": [], "script": []}

_MODEL_CACHE = {}


def hoi4_date(text):
    """'1936.1.8.8' -> datetime, None when unparseable."""
    return model.parse_date(text)


def days_between(a, b):
    da, db = hoi4_date(a), hoi4_date(b)
    if not da or not db:
        return None
    return round((db - da).total_seconds() / 86400.0, 2)


# ---------------------------------------------------------------------------------------------- saves
def find_save(session, tag=None, which="latest", now=None, max_files=20):
    """Path of the save to read and a note: a name, or 'latest' = the save of `tag` with the latest in-game date
    not after `now` (a save dated after the running game belongs to another timeline, for example a test run
    that was not kept), newest file first on ties."""
    d = session.cfg.saves_dir
    if which not in ("latest", "auto"):
        return queries.resolve_save(d, which), None
    files = sorted((os.path.join(d, f) for f in os.listdir(d) if f.lower().endswith(".hoi4")),
                   key=os.path.getmtime, reverse=True)[:max_files]
    from .savefile import SaveFile
    slack_h = 30                           # telemetry lags the screen by up to a day (it logs once a day)
    if not now:
        # without the running game's date the newest save by date would be a save of an earlier campaign that got
        # further (research and capitals were read from a 1940 save in a 1937 game): use the telemetry's day
        try:
            from ..game import timectl
            d, _ = timectl.read_date(session)             # the screen is the truth right after a new game or a load
            now = d.get("hoi4") if "error" not in d else None
        except Exception:
            now = None
        if not now:
            try:
                snap = session.telemetry.snapshot(tag) if (tag or session.telemetry.player()) else None
                now = (snap or {}).get("date")
            except Exception:
                now = None
    now_dt = hoi4_date(now) if now else None
    best, later, other = None, None, None
    for p in files:
        try:
            with SaveFile(p, use_cache=False) as sf:
                h = sf.header()
        except ToolError:
            continue
        if tag and h.get("player") != tag:
            other = other or p
            continue
        dt = hoi4_date(h.get("date"))
        if now_dt and dt and dt > now_dt + datetime.timedelta(hours=slack_h):
            later = later or p
            continue
        if best is None or (dt and dt > best[0]):
            best = (dt, p)
    if best:
        return best[1], None
    if later:
        raise ToolError("every save of this country is dated after the running game (another timeline): save the "
                        "game first (hoi4_save) or wait for the monthly autosave")
    if other:
        return other, f"no save of {tag}; read the newest save instead"
    raise ToolError("no readable plain-text save found (set save_as_binary=no, or save with hoi4_save)")


def save_state(session, path, tag, sections, limit=40):
    """Cached model.country_state for (file, mtime, tag, sections)."""
    st = os.stat(path)
    key = (path, st.st_mtime, st.st_size, tag, tuple(sorted(sections)), limit)
    if key in _MODEL_CACHE:
        return _MODEL_CACHE[key]
    loc = loc_mod.for_session(session)
    try:
        sf = queries.open_save(path)
        data = model.country_state(sf, tag, sections, names=loc.get_many,
                                   idea_slots=scripts.for_session(session, "ideas"),
                                   tech_costs=scripts.for_session(session, "techs"), limit=limit)
        data["_file"] = os.path.basename(path)
        data["_modified"] = datetime.datetime.fromtimestamp(st.st_mtime).isoformat(timespec="seconds")
    finally:
        queries.close_all()
    _MODEL_CACHE.clear() if len(_MODEL_CACHE) > 8 else None
    _MODEL_CACHE[key] = data
    return data


# ---------------------------------------------------------------------------------------------- screen
def screen_read(session):
    """Top bar values, date and pause state from one screenshot (no lock: PrintWindow)."""
    from ..perception import popups as popups_mod
    from ..perception import topbar
    img = session.grab(focus=False)
    d = topbar.read_date(img, session.ocr)
    bar = topbar.read_bar(img, session.ocr, game_dir=session.cfg.game_dir)
    state, ev = topbar.pause_state(img)
    for _ in range(3):                    # the pause sign pulses: a frame between pulses reads as unknown
        if state != "unknown":
            break
        time.sleep(0.3)
        state, ev = topbar.pause_state(session.grab(focus=False))
    out = {"date": d.get("hoi4"), "date_text": d.get("text"), "paused": state == "paused" if state != "unknown" else None,
           "speed": topbar.speed_level(img), "popups": len(popups_mod.detect(img)),
           "values": {k: v["value"] for k, v in bar.items() if k != "_missing"},
           "texts": {k: v["text"] for k, v in bar.items() if k != "_missing"},
           "unread": bar.get("_missing", [])}
    return out, img


# ---------------------------------------------------------------------------------------------- merge helpers
class Section(dict):
    """A section dict that records which source gave each field."""

    def put(self, source, as_of, **fields):
        src = self.setdefault("_sources", {}).setdefault(source, {"as_of": as_of, "fields": []})
        for k, v in fields.items():
            if v is None:
                continue
            if k not in self or self.get("_from", {}).get(k) is None:
                self[k] = v
                self.setdefault("_from", {})[k] = source
            src["fields"].append(k)

    def finish(self):
        self.pop("_from", None)
        return dict(self)


def collect(session, tag=None, sections=DEFAULT_SECTIONS, save="latest", screen=False, limit=40):
    """The merged model. save: 'latest' | 'none' | 'fresh' | a save name."""
    sections = [s for s in SECTIONS if s in set(sections)]
    store = session.telemetry
    tag = tag or store.player()
    snap = store.snapshot(tag) if tag else None
    out = {"tag": tag, "sources": {}}
    # telemetry
    if snap:
        tl = {"as_of": snap["date"], "wall_age_s": round(time.time() - snap["wall"], 1), "generation": store.generation}
        out["sources"]["telemetry"] = tl
    else:
        out["sources"]["telemetry"] = {"available": False,
                                       "note": "no HOI4LIVE lines for this country in game.log (hoi4_telemetry status)"}
    # screen
    scr = None
    if screen:
        try:
            scr, _ = screen_read(session)
            out["sources"]["screen"] = {"as_of": scr["date"], "paused": scr["paused"], "speed": scr["speed"],
                                        "popups": scr["popups"], "unread": scr["unread"]}
        except ToolError as e:
            out["sources"]["screen"] = {"available": False, "error": str(e)}
    # telemetry newer than the screen: a save was loaded and no daily tick ran yet (the lines are from the
    # timeline that was left), so they must not describe this game
    if snap and scr and scr.get("date"):
        gap = days_between(scr["date"], snap["date"])
        if gap is not None and gap > 1.05:
            out["sources"]["telemetry"] = {"available": False, "as_of": snap["date"],
                                           "note": f"telemetry is {gap:g} days ahead of the screen: a save was "
                                                   "loaded and no daily tick has run yet"}
            snap = None
    # save
    sv = None
    if save != "none":
        want = sorted({x for s in sections for x in SAVE_SECTIONS.get(s, [])} | {"overview"})
        try:
            if save == "fresh":
                from ..game import saves as saves_mod
                if session.mode != "test":
                    raise ToolError("save='fresh' writes a save through the console (test mode); in campaign "
                                    "mode save with hoi4_save first and pass its name")
                with session.lock.hold("hoi4_state fresh save"):
                    rep = saves_mod.save(session, "state")
                path = rep["path"]
            else:
                now_guess = (scr or {}).get("date") or (snap or {}).get("date")
                path, note = find_save(session, tag, save, now_guess)
                if note:
                    out.setdefault("warnings", []).append(note)
            sv = save_state(session, path, tag, want, limit) if tag else None
            if sv is None:
                from .savefile import SaveFile
                with SaveFile(path, use_cache=False) as sf:
                    tag = sf.header().get("player")
                out["tag"] = tag
                sv = save_state(session, path, tag, want, limit)
            if "error" in sv:
                out["sources"]["save"] = {"file": os.path.basename(path), "error": sv["error"]}
                sv = None
            else:
                out["sources"]["save"] = {"file": sv["_file"], "as_of": sv["save_date"], "modified": sv["_modified"],
                                          "player": sv.get("player")}
        except ToolError as e:
            out["sources"]["save"] = {"available": False, "error": str(e)}
            sv = None
    # "now" is the newest date any source has seen (the screen when read; a fresh save can be newer than the
    # last daily telemetry tick)
    dates = [d for d in ((scr or {}).get("date"), (snap or {}).get("date"), (sv or {}).get("save_date")) if d]
    now = max(dates, key=lambda d: hoi4_date(d) or datetime.datetime.min) if dates else None
    out["date"] = now
    for name, info in out["sources"].items():
        if info.get("as_of") and now:
            info["age_days"] = days_between(info["as_of"], now)
    T = snap or {}
    day = T.get("day") or {}
    pol = T.get("pol") or T.get("pol_stale") or {}
    tdate = T.get("date")
    sdate = (sv or {}).get("save_date")
    xdate = (scr or {}).get("date")
    so = (sv or {}).get("overview") or {}
    out["name"] = pol.get("name") or so.get("name") or tag
    v = (scr or {}).get("values", {})

    if "overview" in sections:
        s = Section()
        s.put("telemetry", tdate, name=pol.get("name"), ruling_party=pol.get("party"), ideology=pol.get("ideo"),
              party_popularity=pol.get("pop"), leader=pol.get("leader"),
              political_power=day.get("pp"), political_power_daily=day.get("pp_gain"), stability=day.get("stab"),
              war_support=day.get("ws"), command_power=day.get("cp"),
              manpower=round(day["mp_k"] * 1000) if day.get("mp_k") is not None else None,
              civilian_factories=day.get("civ"), military_factories=day.get("mil"), dockyards=day.get("nav"),
              divisions=day.get("div"), at_war=bool(day.get("war")) if "war" in day else None,
              enemies=day.get("enemies"), world_tension=day.get("threat"))
        if scr:
            s.put("screen", xdate, political_power=v.get("political_power"), stability=v.get("stability"),
                  war_support=v.get("war_support"), manpower=v.get("manpower"), command_power=v.get("command_power"),
                  factories_owned=v.get("factories"), world_tension=v.get("world_tension"))
        if sv:
            s.put("save", sdate, name=so.get("name"), ruling_party=so.get("ruling_party"),
                  capital=so.get("capital_name"), focus_tree=so.get("focus_tree"), major=so.get("major"),
                  research_slots=so.get("research_slots"), political_power=so.get("political_power"),
                  command_power=so.get("command_power"))
        out["overview"] = s.finish()

    if "politics" in sections:
        s = Section()
        s.put("telemetry", tdate, ruling_party=pol.get("party"), ideology=pol.get("ideo"),
              party_popularity=pol.get("pop"), leader=pol.get("leader"), days_to_election=pol.get("elect_days"),
              subjects=pol.get("subjects"), political_power=day.get("pp"), political_power_daily=day.get("pp_gain"),
              stability=day.get("stab"), war_support=day.get("ws"))
        fac = T.get("fac")
        if fac:
            s.put("telemetry", tdate, faction={"name": fac.get("name"), "leader": fac.get("leader"),
                                               "members": fac.get("members")})
        sub = T.get("subject")
        if sub:
            s.put("telemetry", tdate, overlord=sub.get("overlord"), autonomy=sub.get("autonomy"))
        sp = (sv or {}).get("politics")
        if sp:
            s.put("save", sdate, laws=sp.get("laws"), advisors=sp.get("advisors") or None,
                  national_spirits=[i["name"] for i in sp.get("spirits", [])],
                  other_ideas={k: [i["name"] for i in v] for k, v in sp.get("other_ideas", {}).items()
                               if k != "hidden_ideas"},
                  popularity=sp.get("popularity"), last_election=sp.get("last_election"),
                  elections_allowed=sp.get("elections_allowed"))
        out["politics"] = s.finish()

    if "economy" in sections:
        s = Section()
        s.put("telemetry", tdate,
              factories={"civilian": day.get("civ"), "military": day.get("mil"), "dockyards": day.get("nav"),
                         "civilian_free": day.get("civ_free"), "military_free": day.get("mil_free"),
                         "dockyards_free": day.get("nav_free"), "civilian_for_construction": day.get("civ_proj"),
                         "owned": day.get("fact_owned"), "total": day.get("fact_total")} if day else None,
              resources=(T.get("res") or T.get("res_stale") or {}).get("resources"),
              fuel={"fuel": round(day["fuel_k"] * 1000) if day.get("fuel_k") is not None else None,
                    "capacity": round(day["fuel_max_k"] * 1000) if day.get("fuel_max_k") is not None else None,
                    "ratio": day.get("fuel_ratio")} if day else None,
              convoys=day.get("convoys"), convoy_threat=day.get("convoy_threat"))
        if scr:
            s.put("screen", xdate, factories_owned=v.get("factories"), fuel_text=(scr["texts"] or {}).get("fuel"),
                  supply=v.get("supply"), convoys=v.get("convoys"))
        if sv:
            pr = sv.get("production") or {}
            co = sv.get("construction") or {}
            s.put("save", sdate, production_lines=pr.get("lines"),
                  military_factories_assigned=pr.get("military_factories_assigned"),
                  dockyards_assigned=pr.get("dockyards_assigned"), production_short_of=pr.get("short_of_resources"),
                  construction_queue=co.get("queue"), construction_queued=co.get("queued"))
        out["economy"] = s.finish()

    if "military" in sections:
        s = Section()
        eq = (T.get("eq") or T.get("eq_stale") or {}).get("equipment")
        s.put("telemetry", tdate, divisions=day.get("div"), battalions=day.get("bat"), armies=day.get("armies"),
              ships=day.get("ships"), planes=day.get("planes"),
              manpower=round(day["mp_k"] * 1000) if day.get("mp_k") is not None else None,
              manpower_deployed=round(day["mp_deployed_k"] * 1000) if day.get("mp_deployed_k") is not None else None,
              in_training_queue=day.get("mp_queue"), garrison_need=day.get("garrison_need"),
              equipment=eq, experience={"army": day.get("xp_army"), "navy": day.get("xp_navy"),
                                        "air": day.get("xp_air")} if day else None,
              command_power=day.get("cp"))
        if scr:
            s.put("screen", xdate, manpower=v.get("manpower"), command_power=v.get("command_power"),
                  experience={"army": v.get("army_experience"), "navy": v.get("navy_experience"),
                              "air": v.get("air_experience")})
        sm = (sv or {}).get("military")
        if sm:
            s.put("save", sdate, divisions=sm.get("divisions"), average_strength=sm.get("avg_strength"),
                  average_organisation=sm.get("avg_organisation"), stockpile=sm.get("stockpile"),
                  army_list=sm.get("armies"), army_groups=sm.get("army_groups"),
                  unassigned_divisions=sm.get("unassigned_divisions"))
        out["military"] = s.finish()

    if "diplomacy" in sections:
        s = Section()
        s.put("telemetry", tdate, at_war=bool(day.get("war")) if day else None, enemies=day.get("enemies"),
              wars=T.get("wars"), surrender_progress=day.get("surrender"),
              enemy_strength_ratio=day.get("enemy_str"), alliance_strength_ratio=day.get("ally_str"),
              casualties=round(day["cas_k"] * 1000) if day.get("cas_k") is not None else None,
              front_states=day.get("front"), faction=(T.get("fac") or {}).get("name"))
        wk = T.get("week")
        if wk:
            s.put("telemetry", wk["date"], front=wk.get("front"), lost_states=wk.get("lost"),
                  neighbours=[c["c"] for c in wk.get("countries", []) if c.get("nb")],
                  allies=[c["c"] for c in wk.get("countries", []) if c.get("ally") and c.get("c") != tag])
        sd = (sv or {}).get("diplomacy")
        if sd:
            s.put("save", sdate, wars=sd.get("wars"), relations=sd.get("relations"), faction=sd.get("faction"))
        s.put("telemetry", tdate, recent_events=T.get("events", [])[-15:] or None)
        out["diplomacy"] = s.finish()

    if "focus" in sections:
        s = Section()
        f = (sv or {}).get("focus")
        if f:
            s.put("save", sdate, current=f.get("current"), current_name=f.get("current_name"),
                  progress_days=f.get("progress_days"), paused=f.get("paused"), tree=f.get("tree"),
                  completed=f.get("completed") or None, recently_completed=f.get("recently_completed") or None,
                  note=f.get("note"))
        out["focus"] = s.finish()

    if "research" in sections:
        s = Section()
        s.put("telemetry", tdate, slots=day.get("rslots"), researched=day.get("techs"))
        r = (sv or {}).get("research")
        if r:
            s.put("save", sdate, slots=r.get("slots_total"), researching=r.get("researching"),
                  idle_slots=r.get("idle_slots"), researched=r.get("researched"),
                  recently_completed=r.get("recently_completed"))
        out["research"] = s.finish()

    for name in ("production", "construction", "decisions", "states"):
        if name in sections:
            s = Section()
            if (sv or {}).get(name):
                s.put("save", sdate, **sv[name])
            out[name] = s.finish()

    if "world" in sections:
        s = Section()
        wk = T.get("week")
        if wk:
            s.put("telemetry", wk["date"], world_tension=wk["world"].get("threat"),
                  countries_at_war=wk["world"].get("at_war"), countries=wk.get("countries"),
                  enemy_pairs=wk.get("enemy_pairs"))
        s.put("telemetry", tdate, events=T.get("events") or None)
        out["world"] = s.finish()

    if "script" in sections:
        s = Section()
        w = T.get("watch") or T.get("watch_stale")
        if w:
            s.put("telemetry", w.get("_date") or tdate, **{k: w.get(k) for k in ("var", "gvar", "flag", "gflag")})
        out["script"] = s.finish()
    out["_raw"] = {"telemetry": snap, "save": sv, "screen": scr}
    return out
