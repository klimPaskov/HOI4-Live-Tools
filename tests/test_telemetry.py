"""Telemetry companion mod: generated templates, playset edits, game.log parsing and the store."""

import json
import os
import re
import shutil
import tempfile
import unittest

from helpers import ROOT  # noqa: F401  (sys.path)

from hoi4_live.telemetry import modgen, parser, playset

DAY = ('[03:30:01][1936.01.12.01][effectbase.cpp:1783]: HOI4LIVE {"k":"day","v":1,"g":"G1","t":"USA","n":706651,'
       '"pp":14.59,"pp_gain":1.21,"stab":0.942,"ws":0.050,"cp":20.9,"mp_k":262.10,"civ":157,"mil":10,"nav":22,'
       '"civ_free":48,"div":36,"war":0,"enemies":0,"front":0,"convoys":641,"threat":0.000}')


def day_line(n, pp, clock="03:30:01", date="1936.01.12.01", tag="USA", war=0, gen="G1"):
    return (f'[{clock}][{date}][effectbase.cpp:1783]: HOI4LIVE {{"k":"day","v":1,"g":"{gen}","t":"{tag}","n":{n},'
            f'"pp":{pp},"stab":0.9,"ws":0.1,"civ":10,"war":{war},"enemies":{war}}}')


NL = chr(10)


def line(clock, date, body):
    return f"[{clock}][{date}][effectbase.cpp:1783]: HOI4LIVE {body}"


class ParseLineTests(unittest.TestCase):
    def test_day(self):
        r = parser.parse_line(DAY)
        self.assertEqual(r["k"], "day")
        self.assertEqual(r["t"], "USA")
        self.assertEqual(r["_date"], "1936.1.12.1")
        self.assertEqual(r["_clock"], "03:30:01")
        self.assertAlmostEqual(r["pp"], 14.59)
        self.assertEqual(r["civ"], 157)

    def test_brace_free_body_as_the_game_prints_it(self):
        r = parser.parse_line('[03:33:13][1936.01.01.23][effectbase.cpp:1783]: HOI4LIVE "k":"day","t":"USA",'
                              '"n":706640,"pp":1.21,"name":"United States"')
        self.assertEqual((r["k"], r["n"], r["pp"], r["name"]), ("day", 706640, 1.21, "United States"))
        # schema 1 printed an unmatched closing brace
        r = parser.parse_line('[03:33:13][1936.01.01.23][effectbase.cpp:1783]: HOI4LIVE "k":"pol","t":"USA",'
                              '"n":706640,"pop":0.990}')
        self.assertEqual(r["pop"], 0.99)

    def test_unflatten(self):
        rec = {"k": "eq", "t": "USA", "n": 1, "infantry_equipment": 100, "infantry_equipment_army": 11061,
               "infantry_equipment_need": 33120}
        eq = parser.unflatten(rec, "eq")["equipment"]["infantry_equipment"]
        self.assertEqual(eq["deficit"], 33120 - 11061 - 100)
        res = parser.unflatten({"k": "res", "oil": -3.0, "oil_prod": 1, "oil_used": 4, "oil_exp": 0, "oil_imp": 0},
                               "res")["resources"]["oil"]
        self.assertEqual((res["surplus"], res["consumed"]), (-3.0, 4))

    def test_other_lines_ignored(self):
        self.assertIsNone(parser.parse_line("[03:30:01][1936.01.12.01][effectbase.cpp:1783]: CHAOS METER: 0"))
        self.assertIsNone(parser.parse_line("random text"))

    def test_quotes_in_names_are_repaired(self):
        body = '{"k":"pol","t":"USA","n":5,"name":"The "Free" States","party":"A\\B Party","pop":0.5}'
        r = parser.parse_line(line("01:00:00", "1936.01.01.12", body))
        self.assertEqual(r["name"], 'The "Free" States')
        self.assertEqual(r["party"], "A\\B Party")
        self.assertEqual(r["pop"], 0.5)

    def test_unparseable_body(self):
        r = parser.parse_line(line("01:00:00", "1936.01.01.12", '{"k":"day","pp":1,2,3}'))
        self.assertEqual(r["k"], "_bad")

    def test_wall_clock_before_midnight(self):
        import datetime
        anchor = datetime.datetime(2026, 10, 6, 0, 0, 30).timestamp()
        w = parser._wall("23:59:50", anchor)
        self.assertEqual(datetime.datetime.fromtimestamp(w).day, 5)
        w = parser._wall("00:00:10", anchor)
        self.assertEqual(datetime.datetime.fromtimestamp(w).day, 6)


class StoreTests(unittest.TestCase):
    def test_ticks_groups_and_snapshot(self):
        s = parser.TelemetryStore()
        text = "\n".join([
            day_line(100, 10.0, date="1936.01.10.01", war=1),
            line("03:30:01", "1936.01.10.01", '{"k":"pol","t":"USA","n":100,"name":"United States","party":"Dem"}'),
            line("03:30:01", "1936.01.10.01", '{"k":"war","t":"USA","n":100,"e":"GER","sp":0.1}'),
            line("03:30:01", "1936.01.10.01", '{"k":"war","t":"USA","n":100,"e":"ITA","sp":0.2}'),
            day_line(101, 11.0, date="1936.01.11.01", war=1),
            line("03:30:02", "1936.01.11.01", '{"k":"war","t":"USA","n":101,"e":"GER","sp":0.15}'),
            line("03:30:02", "1936.01.11.01", '{"k":"ev","e":"declare_war","n":101,"a":"GER","b":"POL"}'),
            line("03:30:02", "1936.01.11.01", '{"k":"wld","t":"USA","n":101,"threat":0.25,"at_war":3}'),
            line("03:30:02", "1936.01.11.01", '{"k":"wc","t":"USA","n":101,"c":"GER","div":100}'),
            line("03:30:02", "1936.01.11.01", '{"k":"we","t":"USA","n":101,"a":"GER","b":"POL"}'),
            line("03:30:02", "1936.01.11.01", '{"k":"we","t":"USA","n":101,"a":"POL","b":"GER"}'),
            line("03:30:02", "1936.01.11.01", '{"k":"front","t":"USA","n":101,"s":64,"name":"X","ctrl":"GER",'
                                              '"owner":"GER","side":"enemy"}'),
            line("03:30:02", "1936.01.11.01", '{"k":"front","t":"USA","n":101,"s":10,"name":"Y","ctrl":"USA",'
                                              '"owner":"USA","side":"own","vs":64}'),
            line("03:30:02", "1936.01.11.01", '{"k":"front","t":"USA","n":101,"s":10,"name":"Y","ctrl":"USA",'
                                              '"owner":"USA","side":"own","vs":65}'),
        ]) + "\n"
        self.assertEqual(s.feed(text), 14)
        self.assertEqual(s.player(), "USA")
        snap = s.snapshot()
        self.assertEqual(snap["n"], 101)
        self.assertEqual(snap["day"]["pp"], 11.0)
        self.assertEqual([w["e"] for w in snap["wars"]], ["GER"])         # only today's war rows
        self.assertIn("pol_stale", snap)                                   # pol is from yesterday
        self.assertEqual(snap["week"]["enemy_pairs"], [["GER", "POL"]])
        self.assertEqual(snap["week"]["front"]["own"][0]["vs"], [64, 65])
        self.assertEqual(snap["events"][0]["e"], "declare_war")
        self.assertEqual(s.day_ago(days=1)["n"], 100)

    def test_a_reloaded_save_repeats_a_weekly_tick(self):
        # the same weekly tick (same day counter) logged twice, minutes apart, after loading the save again
        s = parser.TelemetryStore()
        wc = lambda clock, c: line(clock, "1936.01.04.01", '{"k":"wc","t":"USA","n":706643,"c":"%s"}' % c)
        wld = lambda clock: line(clock, "1936.01.04.01", '{"k":"wld","t":"USA","n":706643,"at_war":2}')
        s.feed("\n".join([day_line(706642, 1.0, clock="04:00:00"), wc("04:00:01", "GER"), wc("04:00:01", "ENG"),
                          wld("04:00:01"), day_line(706643, 2.0, clock="04:00:02"),
                          day_line(706640, 1.0, clock="04:10:00"), wc("04:10:03", "GER"), wc("04:10:03", "ENG"),
                          wld("04:10:03"), day_line(706643, 2.0, clock="04:10:04")]) + "\n")
        week = s.snapshot()["week"]
        self.assertEqual([c["c"] for c in week["countries"]], ["GER", "ENG"])

    def test_partial_lines_wait_for_the_rest(self):
        s = parser.TelemetryStore()
        full = day_line(5, 1.0)
        self.assertEqual(s.feed(full[:40]), 0)
        self.assertEqual(s.feed(full[40:] + "\n"), 1)

    def test_rewind_after_loading_an_older_save(self):
        s = parser.TelemetryStore()
        s.feed("\n".join([day_line(100, 1.0), day_line(101, 2.0),
                          line("03:30:02", "1936.01.11.01", '{"k":"ev","e":"peace","n":101,"a":"GER"}'),
                          day_line(102, 3.0), day_line(100, 9.0)]) + "\n")
        self.assertEqual([r["n"] for r in s.history["USA"]], [100])
        self.assertEqual(s.day()["pp"], 9.0)
        self.assertEqual(len(s.events), 0)
        self.assertEqual(s.rewinds, 1)

    def test_drop_after_forgets_the_days_after_a_loaded_save(self):
        s = parser.TelemetryStore()
        s.feed(NL.join([day_line(100, 1.0, date="1936.01.10.01"), day_line(101, 2.0, date="1936.01.11.01"),
                          day_line(102, 3.0, date="1936.01.12.01"), day_line(103, 4.0, date="1936.01.13.01")]) + NL)
        self.assertEqual(s.drop_after("1936.1.11.12"), 2)
        self.assertEqual([r["n"] for r in s.history["USA"]], [100, 101])
        self.assertEqual(s.day()["pp"], 2.0)
        self.assertEqual(s.drop_after("1936.1.11.12"), 0)             # nothing later: unchanged
        self.assertEqual(s.drop_after("not a date"), 0)
        self.assertEqual(s.drop_after("1936.1.1.12"), 2)             # the whole log is later than the new date
        self.assertIsNone(s.day())

    def test_refresh_reads_deltas_and_detects_restart(self):
        d = tempfile.mkdtemp(prefix="hoi4live_tlm_")
        try:
            p = os.path.join(d, "game.log")
            with open(p, "w", encoding="utf-8") as f:
                f.write("[00:00:01][1936.01.01.12][x.cpp:1]: hello\n" + day_line(1, 1.0) + "\n")
            s = parser.TelemetryStore(p)
            self.assertEqual(s.refresh(), 1)
            self.assertEqual(s.refresh(), 0)
            with open(p, "a", encoding="utf-8") as f:
                f.write(day_line(2, 2.0) + "\n")
            self.assertEqual(s.refresh(), 1)
            self.assertEqual(s.day()["n"], 2)
            with open(p, "w", encoding="utf-8") as f:              # the game restarted: a new, shorter log
                f.write(day_line(7, 7.0, tag="SWE") + "\n")
            self.assertEqual(s.refresh(), 1)
            self.assertEqual(s.restarts, 1)
            self.assertEqual(s.player(), "SWE")
            self.assertNotIn("USA", s.history)
        finally:
            shutil.rmtree(d, ignore_errors=True)


class TemplateTests(unittest.TestCase):
    SPEC = {"generation": "G", "resources": ["oil", "steel"], "equipment": ["infantry_equipment"],
            "techs": ["infantry_weapons", "gw_artillery", "basic_train"],
            "watch": {"variables": ["my_var", "arr^num"], "global_variables": ["g"], "flags": ["f1"],
                      "global_flags": ["gf"]}}

    def substitute(self, text):
        """Replace localisation expressions with sample values, as the game would."""
        text = re.sub(r"\[\?[^\]]+\]", "1.5", text)
        text = re.sub(r"\[[A-Za-z.]+\.GetID\]", "361", text)          # state ids print as numbers
        return re.sub(r"\[[A-Za-z.]+\]", "ABC", text)

    def body(self, text):
        return json.loads("{" + self.substitute(text)[len("HOI4LIVE "):] + "}")

    def test_every_line_is_valid_json(self):
        lines = modgen.templates(self.SPEC)
        for key, text in lines.items():
            self.assertTrue(text.startswith("HOI4LIVE "), key)
            # the game drops `{` and reads `[` as a command: neither may appear outside a [..] expression
            outside = re.sub(r"\[[^\]]*\]", "", text)
            self.assertNotIn("{", outside, key)
            self.assertNotIn("[", outside, key)
            obj = self.body(text)
            self.assertIn("k", obj, key)
        day = self.body(lines["hoi4live_tlm_day"])
        self.assertEqual(day["g"], "G")
        res = parser.unflatten(self.body(lines["hoi4live_tlm_res"]), "res")
        self.assertEqual(res["resources"]["oil"]["produced"], 1.5)
        eq = parser.unflatten(self.body(lines["hoi4live_tlm_eq"]), "eq")
        self.assertEqual(set(eq["equipment"]["infantry_equipment"]), {"stockpile", "in_armies", "needed", "deficit"})
        watch = parser.unflatten(self.body(lines["hoi4live_tlm_watch"]), "watch")
        self.assertEqual(set(watch["var"]), {"my_var", "arr^num"})
        self.assertEqual(set(watch["gflag"]), {"gf"})

    def test_no_watch_no_watch_line(self):
        spec = dict(self.SPEC, watch=modgen.empty_watch())
        self.assertNotIn("hoi4live_tlm_watch", modgen.templates(spec))
        self.assertIn("nothing registered", modgen.effects_text(spec))

    def test_faction_leader_only_inside_a_faction(self):
        lines = modgen.templates(self.SPEC)
        self.assertNotIn("FactionLeader", lines["hoi4live_tlm_wc"])
        self.assertIn("FactionLeader", lines["hoi4live_tlm_wc_fac"])
        self.assertIn("is_in_faction = yes", modgen.effects_text(self.SPEC))

    def test_localisation_file_escapes_quotes_and_has_bom(self):
        text = modgen.localisation_text("german", {"k1": 'HOI4LIVE {"a":"[ROOT.GetTag]"}'})
        self.assertTrue(text.startswith("﻿l_german:"))
        self.assertIn(' k1: "HOI4LIVE {\\"a\\":\\"[ROOT.GetTag]\\"}"', text)

    def test_technology_and_state_scans(self):
        lines = modgen.templates(self.SPEC)
        for key in ("hoi4live_tlm_kt", "hoi4live_tlm_rs", "hoi4live_tlm_st"):
            self.assertIn(key, lines)
            self.assertIn("k", self.body(lines[key]))
        st = self.body(lines["hoi4live_tlm_st"])
        self.assertEqual({"s", "name", "civ", "mil", "dock", "infra", "free", "ctrl"} <= set(st), True)
        fx = modgen.effects_text(self.SPEC)
        self.assertIn("if = { limit = { has_tech = gw_artillery } set_temp_variable = { hoi4live_tid = 1 } "
                      "log = hoi4live_tlm_kt }", fx)
        self.assertIn("is_researching_technology = basic_train", fx)
        self.assertIn("hoi4live_telemetry_tech = yes", fx)
        self.assertIn("free_building_slots = { building = industrial_complex size > 9 }", fx)

    def test_script_braces_balance(self):
        for text in (modgen.effects_text(self.SPEC), modgen.on_actions_text(self.SPEC)):
            code = "\n".join(l.split("#", 1)[0] for l in text.splitlines())
            self.assertEqual(code.count("{"), code.count("}"))
            self.assertNotIn("<=", code)
            self.assertNotIn(">=", code)

    def test_watch_names_are_validated(self):
        with self.assertRaises(Exception):
            modgen.clean_watch({"variables": ["bad name"]})
        with self.assertRaises(Exception):
            modgen.clean_watch({"flags": ["x^num"]})
        self.assertEqual(modgen.clean_watch({"variables": ["a", "a", "b@ROOT"]})["variables"], ["a", "b@ROOT"])


class TechAndStateStoreTests(unittest.TestCase):
    def feed(self, store, *bodies):
        n = 0
        for clock, date, body in bodies:
            store.feed(line(clock, date, body) + NL)

    def test_known_researching_and_states(self):
        store = parser.TelemetryStore()
        store.set_catalog(["a", "b", "c", "d"])
        self.assertIsNone(store.known_techs())
        self.assertIsNone(store.researching_techs())
        self.feed(store,
                  ("03:00:00", "1936.01.07.24", '"k":"day","t":"GER","n":100,"pp":1'),
                  ("03:00:00", "1936.01.07.24", '"k":"kt","t":"GER","n":100,"tid":0'),
                  ("03:00:00", "1936.01.07.24", '"k":"kt","t":"GER","n":100,"tid":2'),
                  ("03:00:00", "1936.01.07.24", '"k":"st","t":"GER","n":100,"s":3,"name":"Saxony","civ":4,"mil":2,'
                                               '"dock":0,"infra":4,"free":1,"ctrl":"GER"'),
                  ("03:00:00", "1936.01.07.24", '"k":"rs","t":"GER","n":100,"tid":3'))
        self.assertEqual(store.known_techs(), {"a", "c"})
        self.assertEqual(store.researching_techs(), ["d"])
        self.assertEqual(store.state_rows()[0]["name"], "Saxony")
        # tomorrow: nothing researched today, one new known technology
        self.feed(store, ("03:00:01", "1936.01.08.24", '"k":"day","t":"GER","n":101,"pp":2'))
        self.assertEqual(store.researching_techs(), [])
        self.assertEqual(store.known_techs(), {"a", "c"})
        self.feed(store, ("03:00:01", "1936.01.08.24", '"k":"kt","t":"GER","n":101,"tid":0'),
                  ("03:00:01", "1936.01.08.24", '"k":"kt","t":"GER","n":101,"tid":1'))
        self.assertEqual(store.known_techs(), {"a", "b"})

    def test_busy_slots_count_distinct_technologies_and_forget_an_abandoned_timeline(self):
        store = parser.TelemetryStore()
        store.set_catalog(["a", "b", "c"])
        self.feed(store, ("03:00:00", "1936.01.07.24", '"k":"day","t":"GER","n":100,"pp":1'),
                  ("03:00:00", "1936.01.07.24", '"k":"rs","t":"GER","n":100,"tid":0'),
                  ("03:00:00", "1936.01.07.24", '"k":"rs","t":"GER","n":100,"tid":1'),
                  # a second timeline logs the same day number again: the same technology counts once
                  ("03:05:00", "1936.01.07.24", '"k":"rs","t":"GER","n":100,"tid":0'))
        self.assertEqual(store.busy_slots(100), 2)
        # the new timeline's day record (same counter) rewinds the old one
        self.feed(store, ("03:05:01", "1936.01.07.24", '"k":"day","t":"GER","n":100,"pp":1'))
        self.assertEqual(store.busy_slots(100), 0)

    def test_ids_outside_the_catalog_are_ignored(self):
        store = parser.TelemetryStore()
        store.set_catalog(["a"])
        self.feed(store, ("03:00:00", "1936.01.07.24", '"k":"day","t":"GER","n":100,"pp":1'),
                  ("03:00:00", "1936.01.07.24", '"k":"kt","t":"GER","n":100,"tid":9'))
        self.assertEqual(store.known_techs(), set())


class PlaysetTests(unittest.TestCase):
    def setUp(self):
        self.d = tempfile.mkdtemp(prefix="hoi4live_ps_")
        os.makedirs(os.path.join(self.d, "mod"))
        with open(os.path.join(self.d, "dlc_load.json"), "w", encoding="utf-8") as f:
            f.write('{"enabled_mods":["mod/a.mod","mod/b.mod"],"disabled_dlcs":["x"]}')

    def tearDown(self):
        shutil.rmtree(self.d, ignore_errors=True)

    def read(self):
        with open(os.path.join(self.d, "dlc_load.json"), encoding="utf-8") as f:
            return json.load(f)

    def test_enable_disable_restore(self):
        with self.assertRaises(Exception):
            playset.enable(self.d)                                  # not generated yet
        modgen.generate(self.d, None, "1.19.2.0")
        rep = playset.enable(self.d)
        self.assertTrue(rep["changed"])
        self.assertTrue(os.path.exists(rep["backup"]))
        data = self.read()
        self.assertEqual(data["enabled_mods"], ["mod/a.mod", "mod/b.mod", playset.ENTRY])
        self.assertEqual(data["disabled_dlcs"], ["x"])
        self.assertFalse(playset.enable(self.d)["changed"])
        # the user changes something after enabling: disable keeps it
        data["enabled_mods"].insert(0, "mod/c.mod")
        with open(os.path.join(self.d, "dlc_load.json"), "w", encoding="utf-8") as f:
            json.dump(data, f)
        playset.disable(self.d)
        self.assertEqual(self.read()["enabled_mods"], ["mod/c.mod", "mod/a.mod", "mod/b.mod"])
        playset.restore(self.d)
        self.assertEqual(self.read()["enabled_mods"], ["mod/a.mod", "mod/b.mod"])
        st = playset.status(self.d)
        self.assertFalse(st["enabled"])
        self.assertEqual([h["action"] for h in st["history"]], ["enable", "disable", "restore"])
        self.assertEqual(st["history"][0]["before"], ["mod/a.mod", "mod/b.mod"])

    def test_generated_files(self):
        man = modgen.generate(self.d, None, "1.19.2.0", watch={"variables": ["v"]})
        self.assertTrue(os.path.exists(modgen.descriptor_path(self.d)))
        for rel in man["files"]:
            self.assertTrue(os.path.exists(os.path.join(modgen.mod_dir(self.d), rel)), rel)
        self.assertEqual(len([f for f in man["files"] if f.endswith(".yml")]), len(modgen.LANGUAGES))
        # regenerating without a watch argument keeps the list
        self.assertEqual(modgen.generate(self.d, None, "1.19.2.0")["watch"]["variables"], ["v"])
        with open(modgen.descriptor_path(self.d), encoding="utf-8") as f:
            self.assertIn('supported_version="1.19.*"', f.read())


if __name__ == "__main__":
    unittest.main()
