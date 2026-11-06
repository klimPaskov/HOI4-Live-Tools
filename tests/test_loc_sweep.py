"""Localisation sweep: the line classifier, the log filter, the sweep driver on a fake session and the tool."""

import json
import os
import re
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

import numpy as np

import helpers  # noqa: F401  (puts the repository on sys.path)

from hoi4_live.errors import ToolError, ValidationError
from hoi4_live.game import locsweep, panels
from hoi4_live.game.logs import Logs
from hoi4_live.perception import screen as screen_mod
from hoi4_live.protocol.schema import validate
from hoi4_live.session import Cancelled
from hoi4_live.tools import registry, sweep_tools


def line(text, x=10, y=10, w=200, h=20):
    return SimpleNamespace(text=text, box=[x, y, w, h], center=(x + w // 2, y + h // 2))


class SuspiciousTests(unittest.TestCase):
    def test_flagged_lines(self):
        cases = [
            ("[missing_key]", "missing_key"), ("missing_key", "missing_key"), ("[MISSING_KEY]", "missing_key"),
            ("[Missing_Key]", "missing_key"), ("[m1ssing_key]", "missing_key"),         # OCR reads an i as 1
            ("[missing key]", "missing_key"), ("Focus: [missing_key]", "missing_key"),
            ("my_mod.1.t", "raw_key"), ("  my_mod.1.t  ", "raw_key"), ("my_mod.1.t.", "raw_key"),
            ("GER_focus_name", "raw_key"), ("GER_focus_name,", "raw_key"), ('"GER_focus_name"', "raw_key"),
            ("FOCUS_DESC_x", "raw_key"), ("ideas_title", "raw_key"), ("news.1.t", "raw_key"), ("GER_1936", "raw_key"),
            ("$COUNTRY$", "unresolved_variable"), ("$VALUE|Y$", "unresolved_variable"),
            ("Gain $AMOUNT$ political power", "unresolved_variable"),
            ("[Root.GetName]", "unresolved_variable"), ("[This.Owner.GetNameDef]", "unresolved_variable"),
            ("Welcome to [Root.GetName]!", "unresolved_variable"), ("[GetDateText]", "unresolved_variable"),
            ("[?my_var|0]", "unresolved_variable"), ("Line one\\nLine two", "unresolved_variable"),
            (chr(0xA7) + "Rred" + chr(0xA7) + "!", "colour_code"),
            ("Stability " + chr(0xA7) + "G+5%" + chr(0xA7) + "!", "colour_code"),
        ]
        for text, kind in cases:
            self.assertEqual(locsweep.suspicious(text), kind, text)

    def test_normal_lines_are_not_flagged(self):
        for text in ["", "   ", None, "Political Power", "Stability: 62%", "Germany", "USA", "GER", "OK", "100%",
                     "1936.1.1", "Day 12, 14:00", "v1.19.4.1", "1.19.4.1", "+12.5", "12_345",
                     "U.S.S.R", "e.g.", "config.json", "www.paradoxplaza.com", "Co-operation",
                     "Q_key", "ab_c",                                           # too short to be a key
                     "Press the Q_key", "Use the Shift_Left key to scroll",      # prose with an underscore
                     "$5", "$50 / $100", "[Get started]", "[PP]", "Cost: 5 [x]", "C:/new",
                     "|||", "l.l.l", "I_I"]:
            self.assertIsNone(locsweep.suspicious(text), repr(text))

    def test_raw_key_needs_the_whole_line_and_a_valid_shape(self):
        self.assertEqual(locsweep.suspicious("a" * 5 + "_" + "b" * 74), "raw_key")      # 80 characters
        self.assertIsNone(locsweep.suspicious("a" * 5 + "_" + "b" * 75))                # 81 characters
        self.assertIsNone(locsweep.suspicious("GER_focus_name Political Power"))
        self.assertIsNone(locsweep.suspicious("_leading_underscore"))
        self.assertIsNone(locsweep.suspicious("trailing_underscore_"))
        self.assertIsNone(locsweep.suspicious("double__underscore"))                   # single separators only
        self.assertIsNone(locsweep.suspicious("news.t.a"))                              # dots without a number part
        self.assertIsNone(locsweep.suspicious("1_2_3_4_5"))

    def test_custom_patterns(self):
        self.assertEqual(locsweep.suspicious("TODO translate me", ["^todo"]), "custom")      # case-insensitive
        self.assertEqual(locsweep.suspicious("Lorem ipsum dolor", [re.compile("lorem", re.I)]), "custom")
        self.assertIsNone(locsweep.suspicious("TODO translate me", []))
        self.assertIsNone(locsweep.suspicious("Political Power", ["^todo"]))
        self.assertEqual(locsweep.suspicious("[missing_key] todo", ["todo"]), "missing_key")   # built-ins win

    def test_bad_pattern_names_the_index(self):
        with self.assertRaises(ToolError) as cm:
            locsweep.compile_patterns(["fine", "(unclosed"])
        self.assertEqual(cm.exception.details, {"field": "extra_patterns[1]"})


class ClassifyTests(unittest.TestCase):
    def test_repeats_are_folded_and_counted(self):
        lines = [line("[missing_key]", y=50), line("Political Power", y=10), line("[missing_key]", y=90),
                 line("[MISSING_KEY]", y=120), line("GER_focus_name", y=30)]
        out = locsweep.classify_lines(lines)
        self.assertEqual(out["lines"], 5)
        self.assertEqual(out["total"], 2)
        self.assertEqual(out["by_kind"], {"missing_key": 1, "raw_key": 1})
        self.assertEqual([f["text"] for f in out["findings"]], ["GER_focus_name", "[missing_key]"])   # reading order
        self.assertEqual([f["count"] for f in out["findings"]], [1, 3])
        self.assertEqual(out["findings"][1]["box"], [10, 50, 200, 20])                                 # the first sighting
        self.assertEqual(out["omitted"], 0)

    def test_cap_keeps_the_totals(self):
        lines = [line(f"MOD_key_{i}", y=i * 30) for i in range(5)]
        out = locsweep.classify_lines(lines, max_findings=2)
        self.assertEqual((out["total"], len(out["findings"]), out["omitted"]), (5, 2, 3))
        self.assertEqual(out["by_kind"], {"raw_key": 5})

    def test_clean_screen(self):
        out = locsweep.classify_lines([line("Political Power"), line("Stability: 62%")])
        self.assertEqual((out["total"], out["findings"], out["by_kind"]), (0, [], {}))

    def test_extra_patterns_apply(self):
        out = locsweep.classify_lines([line("TODO fix"), line("fine")], ["^todo"])
        self.assertEqual(out["by_kind"], {"custom": 1})


class LogLineTests(unittest.TestCase):
    def test_keeps_only_localisation_lines_with_counts(self):
        rows = [
            {"line": "[12:00:01][localisation.cpp:55]: Missing localisation key: my_mod_t", "count": 3},
            {"line": "[12:00:02][script.cpp:10]: Unknown trigger 'has_wonder' in file common/ideas/x.txt"},
            {"line": "[12:00:03][persistent.cpp:12]: Duplicate localization key GER_focus in l_english"},
            {"line": "[12:00:04][game.cpp:1]: Failed to load localisation/english/x_l_english.yml", "count": 2,
             "known_mistake": "hint"},
            {"line": "[12:00:05][loc.cpp:9]: loc key 'abc' not found"},
            {"line": "[12:00:06][graphics.cpp:3]: Missing texture gfx/interface/x.dds"},
            {"line": "[12:00:07][event.cpp:2]: Duplicate event option key mod.1.a"},
        ]
        kept = locsweep.loc_log_lines(rows)
        self.assertEqual([k["count"] for k in kept], [3, 1, 2, 1, 1])
        self.assertTrue(kept[0]["line"].endswith("my_mod_t"))
        self.assertEqual(kept[2]["known_mistake"], "hint")
        self.assertNotIn("known_mistake", kept[0])
        self.assertTrue(all("Unknown trigger" not in k["line"] and "texture" not in k["line"] for k in kept))

    def test_nothing(self):
        self.assertEqual(locsweep.loc_log_lines([]), [])
        self.assertEqual(locsweep.loc_log_lines(None), [])


class FakeSession:
    """A session whose screens are lists of OCR texts. Panels are fakes that move `view` and record their calls."""

    def __init__(self, tmp, screens=None, fail_open=None, read_error=None):
        self.logs_dir = os.path.join(tmp, "logs")
        os.makedirs(self.logs_dir, exist_ok=True)
        self.logs = Logs(self.logs_dir)
        self.artifacts_dir = os.path.join(tmp, "artifacts")
        self.screens = screens or {}
        self.fail_open = fail_open or {}              # panel -> exception raised by open_panel
        self.read_error = read_error or {}            # panel -> exception raised when the OCR reads it
        self.on_open = {}                             # panel -> callable run when it opens
        self.to_map_errors = []                       # per to_map call: an exception to raise, or None
        self.cancel_on_check = None                   # number of the check_cancel call that raises
        self.checks = 0
        self.view = "map"
        self.events = []
        self.artifacts = []
        self.progress = []
        self.ocr = SimpleNamespace(recognize=self._recognize)
        self.names = ["map"] + list(screen_mod.PANEL_KEYS)

    # panels
    def open_panel(self, session, name):
        self.events.append(("open", name))
        if name in self.fail_open:
            raise self.fail_open[name]
        self.view = name
        if name in self.on_open:
            self.on_open[name]()
        return name

    def close(self, session, name=None):
        self.events.append(("close", self.view))
        was, self.view = self.view, "map"
        return None if was == "map" else was

    def to_map(self, session, max_steps=4):
        self.events.append(("to_map",))
        if self.to_map_errors:
            err = self.to_map_errors.pop(0)
            if err is not None:
                raise err
        closed = [] if self.view == "map" else [self.view]
        self.view = "map"
        return closed

    # capture and OCR
    def grab(self, focus=None):
        self.events.append(("grab", self.view))
        return np.full((4, 4, 3), self.names.index(self.view), dtype=np.uint8)

    def _recognize(self, img, region=None, scale=None):
        view = self.names[int(img[0, 0, 0])]
        self.events.append(("ocr", view, scale))
        if view in self.read_error:
            raise self.read_error[view]
        return SimpleNamespace(lines=[line(t, y=10 + 30 * i) for i, t in enumerate(self.screens.get(view, []))])

    def artifact(self, img, name):
        self.artifacts.append(name)
        return os.path.join(self.artifacts_dir, name + ".png")

    def artifact_path(self, name):
        os.makedirs(self.artifacts_dir, exist_ok=True)
        return os.path.join(self.artifacts_dir, name)

    # call context
    def check_cancel(self):
        self.checks += 1
        if self.cancel_on_check == self.checks:
            raise Cancelled("cancelled by the client")

    def report(self, message, value=None, total=None):
        self.progress.append((message, value, total))

    # helpers for the tests
    def patched(self):
        return mock.patch.multiple(panels, open_panel=self.open_panel, close=self.close, to_map=self.to_map)

    def append(self, log, *lines):
        with open(os.path.join(self.logs_dir, log + ".log"), "ab") as f:
            for l in lines:
                f.write(l.encode("utf-8") + b"\n")


class SweepTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_sweep_")
        self.addCleanup(self.tmp.cleanup)

    def session(self, **kw):
        return FakeSession(self.tmp.name, **kw)

    def run_sweep(self, sess, panel_names, **kw):
        with sess.patched():
            return locsweep.sweep(sess, panel_names, **kw)

    def test_findings_per_screen(self):
        sess = self.session(screens={
            "map": ["Political Power", "[missing_key]"],
            "politics": ["POLITICS", "GER_focus_name", "Stability: 62%", "$COUNTRY$", "GER_focus_name"],
            "research": ["Research", "Infantry Equipment"]})
        report = self.run_sweep(sess, ["politics", "research"])
        by_screen = {s["screen"]: s for s in report["screens"]}
        self.assertEqual(list(by_screen), ["map", "politics", "research"])
        self.assertEqual({k: s["status"] for k, s in by_screen.items()}, {"map": "ok", "politics": "ok", "research": "ok"})
        self.assertEqual([f["kind"] for f in by_screen["map"]["findings"]], ["missing_key"])
        self.assertEqual([(f["kind"], f["text"], f["count"]) for f in by_screen["politics"]["findings"]],
                         [("raw_key", "GER_focus_name", 2), ("unresolved_variable", "$COUNTRY$", 1)])
        self.assertEqual(by_screen["research"]["findings"], [])
        self.assertEqual(by_screen["politics"]["lines"], 5)
        self.assertEqual(report["totals"], {
            "screens": 3, "swept": 3, "not_opened": 0, "read_failed": 0, "with_findings": 2, "findings": 3,
            "by_kind": {"missing_key": 1, "raw_key": 1, "unresolved_variable": 1}})
        # a screenshot only for the screens with findings
        self.assertEqual(sess.artifacts, ["loc_sweep_map", "loc_sweep_politics"])
        self.assertTrue(by_screen["map"]["screenshot"].endswith("loc_sweep_map.png"))
        self.assertIsNone(by_screen["research"]["screenshot"])
        self.assertIn(("ocr", "politics", 1.5), sess.events)
        self.assertEqual(sess.progress[0], ("sweep: map", 1, 3))
        self.assertEqual(sess.progress[-1], ("sweep: research", 3, 3))

    def test_order_starts_and_ends_on_the_map(self):
        sess = self.session()
        report = self.run_sweep(sess, ["politics", "research"])
        calls = [e[0] if e[0] != "open" else "open " + e[1] for e in sess.events if e[0] in ("open", "close", "to_map")]
        self.assertEqual(calls, ["to_map", "open politics", "close", "open research", "close", "to_map"])
        self.assertEqual(sess.events[1][0], "grab")                 # the map is read right after to_map
        self.assertEqual(sess.events[1][1], "map")
        self.assertEqual(sess.view, "map")
        self.assertTrue(report["returned_to_map"])
        self.assertIsNone(report["map_error"])

    def test_map_only(self):
        sess = self.session(screens={"map": ["ok"]})
        report = self.run_sweep(sess, [])
        self.assertEqual([s["screen"] for s in report["screens"]], ["map"])
        self.assertNotIn("open", [e[0] for e in sess.events])

    def test_panel_that_does_not_open_is_recorded_and_skipped(self):
        err = ToolError("the trade panel did not open (title strip shows None)", {"panel": None})
        sess = self.session(fail_open={"trade": err}, screens={"research": ["res_key_x"], "production": ["fine"]})
        report = self.run_sweep(sess, ["research", "trade", "production"])
        by_screen = {s["screen"]: s for s in report["screens"]}
        self.assertEqual(by_screen["trade"]["status"], "not_opened")
        self.assertIn("did not open", by_screen["trade"]["message"])
        self.assertNotIn("findings", by_screen["trade"])
        self.assertEqual(by_screen["research"]["total"], 1)         # the sweep went on
        self.assertEqual(by_screen["production"]["status"], "ok")
        self.assertEqual(report["totals"]["not_opened"], 1)
        self.assertEqual(report["totals"]["swept"], 3)
        self.assertEqual(report["totals"]["screens"], 4)
        self.assertEqual(sess.view, "map")
        self.assertIn("1 panel(s) not opened", locsweep.summary(report))

    def test_unreadable_panel_is_recorded_and_closed(self):
        sess = self.session(read_error={"research": ToolError("capture failed")}, screens={"production": ["prod_key_x"]})
        report = self.run_sweep(sess, ["research", "production"])
        by_screen = {s["screen"]: s for s in report["screens"]}
        self.assertEqual((by_screen["research"]["status"], by_screen["research"]["message"]), ("read_failed", "capture failed"))
        self.assertEqual(by_screen["production"]["total"], 1)
        self.assertEqual(report["totals"]["read_failed"], 1)
        self.assertIn(("close", "research"), sess.events)
        self.assertEqual(sess.view, "map")

    def test_unexpected_error_still_returns_to_the_map(self):
        sess = self.session(read_error={"research": RuntimeError("boom")})
        with self.assertRaises(RuntimeError):
            self.run_sweep(sess, ["politics", "research", "trade"])
        self.assertEqual(sess.events[-1], ("to_map",))
        self.assertEqual(sess.view, "map")
        self.assertNotIn(("open", "trade"), sess.events)            # it stopped there

    def test_map_that_cannot_be_read_stops_the_sweep(self):
        sess = self.session(read_error={"map": ToolError("no game window")})
        with self.assertRaises(ToolError):
            self.run_sweep(sess, ["politics"])
        self.assertNotIn(("open", "politics"), sess.events)
        self.assertEqual(sess.events[-1], ("to_map",))

    def test_cancel_is_not_a_failed_panel(self):
        sess = self.session(fail_open={"research": Cancelled("cancelled by the client")})
        with self.assertRaises(Cancelled):
            self.run_sweep(sess, ["politics", "research", "trade"])
        self.assertNotIn(("open", "trade"), sess.events)
        self.assertEqual(sess.events[-1], ("to_map",))
        # and a cancel between two panels
        sess = self.session()
        sess.cancel_on_check = 3                                    # after the first to_map, before politics, before research
        with self.assertRaises(Cancelled):
            self.run_sweep(sess, ["politics", "research"])
        self.assertNotIn(("open", "research"), sess.events)
        self.assertEqual(sess.events[-1], ("to_map",))

    def test_failure_to_return_is_reported(self):
        sess = self.session(screens={"map": ["[missing_key]"]})
        sess.to_map_errors = [None, ToolError("pause menu is open")]     # the first to_map works, the last one fails
        report = self.run_sweep(sess, ["politics"])
        self.assertFalse(report["returned_to_map"])
        self.assertEqual(report["map_error"], "pause menu is open")
        self.assertIn("could not return to the map", locsweep.summary(report))
        self.assertEqual(report["totals"]["findings"], 1)                # the findings are still reported

    def test_log_warnings_only_since_the_sweep_started(self):
        sess = self.session()
        sess.append("error", "[08:00:00][localisation.cpp:1]: Missing localisation key: old_key",
                    "[08:00:01][script.cpp:2]: Unknown effect old_effect")
        sess.append("text", "old text.log line about l_english")
        sess.on_open["research"] = lambda: (
            sess.append("error", "[12:00:01][localisation.cpp:55]: Missing localisation key: my_mod_t",
                        "[12:00:02][script.cpp:10]: Unknown trigger new_trigger",
                        "[12:00:03][localisation.cpp:55]: Missing localisation key: my_mod_t"),
            sess.append("text", "Duplicate localization key GER_focus in l_english"))
        report = self.run_sweep(sess, ["politics", "research"])
        err = report["log_warnings"]["error"]
        self.assertEqual(len(err), 1)
        self.assertTrue(err[0]["line"].endswith("my_mod_t"))
        self.assertEqual(err[0]["count"], 2)                        # repeats fold even with other timestamps
        self.assertEqual([r["line"] for r in report["log_warnings"]["text"]],
                         ["Duplicate localization key GER_focus in l_english"])
        self.assertNotIn("before_sweep", report)
        self.assertEqual(locsweep.warning_count(report), 2)
        self.assertEqual(report["log_info"]["error"]["exists"], True)
        self.assertTrue(locsweep.summary(report).endswith("2 localisation warning(s) in the logs"))

    def test_since_all_adds_what_was_there_before(self):
        sess = self.session()
        sess.append("error", "[08:00:00][localisation.cpp:1]: Missing localisation key: old_key",
                    "[08:00:01][script.cpp:2]: Unknown effect old_effect")
        sess.on_open["politics"] = lambda: sess.append(
            "error", "[12:00:01][localisation.cpp:55]: Missing localisation key: new_key")
        report = self.run_sweep(sess, ["politics"], since="all")
        self.assertEqual([r["line"] for r in report["log_warnings"]["error"]],
                         ["[12:00:01][localisation.cpp:55]: Missing localisation key: new_key"])
        self.assertEqual([r["line"] for r in report["before_sweep"]["error"]],
                         ["[08:00:00][localisation.cpp:1]: Missing localisation key: old_key"])
        self.assertEqual(report["before_sweep"]["text"], [])         # no text.log at all
        self.assertEqual(report["log_info"]["text"]["exists"], False)
        self.assertEqual(locsweep.warning_count(report), 1)           # the summary counts the new ones only

    def test_markdown_lists_findings_and_warnings(self):
        sess = self.session(screens={"map": ["[missing_key]", "[missing_key]"]},
                            fail_open={"trade": ToolError("no key | no panel")})
        sess.on_open["politics"] = lambda: sess.append("error", "Missing localisation key: abc")
        text = locsweep.markdown(self.run_sweep(sess, ["politics", "trade"]))
        self.assertIn("# Localisation sweep", text)
        self.assertIn("- missing_key: `[missing_key]` (x2)", text)
        self.assertIn("| trade | not_opened |", text)
        self.assertIn("no key / no panel", text)                      # a pipe would break the table
        self.assertIn("error.log: `Missing localisation key: abc`", text)


class ToolTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_sweep_")
        self.addCleanup(self.tmp.cleanup)
        self.tool = registry.get("hoi4_sweep_loc")

    def call(self, sess, args=None):
        args = validate(self.tool.schema, args or {})
        with sess.patched():
            return sweep_tools.hoi4_sweep_loc.__wrapped__(sess, args)

    def test_registration(self):
        t = self.tool
        self.assertEqual((t.family, t.lock, t.test_only), ("testmode", "acting", True))
        self.assertIs(t.handler, sweep_tools.hoi4_sweep_loc)
        self.assertEqual(t.schema["properties"]["panels"]["default"], list(locsweep.DEFAULT_PANELS))
        for name in locsweep.DEFAULT_PANELS:
            self.assertTrue(screen_mod.PANEL_KEYS.get(name), name)   # every default panel has a key
        self.assertEqual(sorted(sweep_tools.PANEL_NAMES), sorted(locsweep.DEFAULT_PANELS))
        for ex in t.examples:
            validate(t.schema, ex["args"])

    def test_schema_limits(self):
        for bad in ({"panels": ["focus_tree"]}, {"panels": ["politics", "politics"]}, {"panels": "politics"},
                    {"since": "never"}, {"max_lines_per_screen": 0}, {"max_lines_per_screen": 201},
                    {"extra_patterns": [""]}, {"extra_patterns": "x"}, {"colour": True}):
            with self.assertRaises(ValidationError, msg=str(bad)):
                validate(self.tool.schema, bad)
        filled = validate(self.tool.schema, {})
        self.assertEqual((filled["since"], filled["max_lines_per_screen"]), ("start", 40))
        self.assertEqual(len(filled["panels"]), 14)

    def test_default_sweep_writes_the_report(self):
        sess = FakeSession(self.tmp.name, screens={
            "map": ["[missing_key]"], "politics": ["GER_focus_name", "$COUNTRY$"]})
        sess.on_open["research"] = lambda: sess.append(
            "error", "[12:00:01][localisation.cpp:55]: Missing localisation key: my_mod_t")
        res = self.call(sess)
        self.assertEqual(res.summary, "sweep: 3 finding(s) on 2 of 15 screen(s); 1 localisation warning(s) in the logs")
        d = res.data
        self.assertEqual(d["totals"]["findings"], 3)
        self.assertEqual(d["totals"]["by_kind"], {"missing_key": 1, "raw_key": 1, "unresolved_variable": 1})
        self.assertEqual(len(d["screens"]), 15)
        self.assertEqual(len(d["screenshots"]), 2)
        self.assertTrue(d["returned_to_map"])
        self.assertEqual(len(d["log_warnings"]["error"]), 1)
        self.assertNotIn("before_sweep", d)
        self.assertTrue(d["report"].endswith(".md") and os.path.isfile(d["report"]))
        self.assertTrue(d["json"].endswith(".json") and os.path.isfile(d["json"]))
        self.assertEqual(os.path.splitext(d["report"])[0], os.path.splitext(d["json"])[0])
        self.assertTrue(os.path.basename(d["report"]).startswith("loc_sweep_"))
        with open(d["json"], encoding="utf-8") as f:
            saved = json.load(f)
        self.assertEqual(saved["totals"], d["totals"])
        self.assertEqual(saved["log_warnings"]["error"][0]["count"], 1)
        with open(d["report"], encoding="utf-8") as f:
            md = f.read()
        self.assertIn("GER_focus_name", md)
        self.assertIn("my_mod_t", md)
        self.assertEqual([e[1] for e in sess.events if e[0] == "open"], list(locsweep.DEFAULT_PANELS))
        self.assertEqual(sess.view, "map")

    def test_arguments_reach_the_sweep(self):
        sess = FakeSession(self.tmp.name, screens={
            "politics": [f"MOD_key_{i}" for i in range(5)] + ["TODO later"]})
        sess.on_open["politics"] = lambda: sess.append("error", "old")
        res = self.call(sess, {"panels": ["politics"], "extra_patterns": ["^todo"], "max_lines_per_screen": 3,
                               "since": "all"})
        politics = res.data["screens"][1]
        self.assertEqual((politics["total"], len(politics["findings"]), politics["omitted"]), (6, 3, 3))
        self.assertEqual(res.data["totals"]["by_kind"], {"raw_key": 5, "custom": 1})
        self.assertIn("before_sweep", res.data)
        self.assertEqual(res.summary, "sweep: 6 finding(s) on 1 of 2 screen(s); 0 localisation warning(s) in the logs")
        self.assertEqual([e[1] for e in sess.events if e[0] == "open"], ["politics"])

    def test_bad_regex_is_refused_before_anything_happens(self):
        sess = FakeSession(self.tmp.name)
        with self.assertRaises(ToolError) as cm:
            self.call(sess, {"extra_patterns": ["ok", "("]})
        self.assertEqual(cm.exception.details["field"], "extra_patterns[1]")
        self.assertEqual(sess.events, [])

    def test_direct_call_without_filled_defaults(self):
        sess = FakeSession(self.tmp.name)
        with sess.patched():
            res = sweep_tools.hoi4_sweep_loc.__wrapped__(sess, {"panels": ["trade"]})
        self.assertEqual(res.summary, "sweep: 0 finding(s) on 0 of 2 screen(s); 0 localisation warning(s) in the logs")


if __name__ == "__main__":
    unittest.main()
