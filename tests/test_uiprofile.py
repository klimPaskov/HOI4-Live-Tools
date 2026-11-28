"""The interface profile for overhaul / total-conversion mods (perception/uiprofile.py): month names and menu labels
derived from a mod's own files, a mod-supplied profile, the named failures, and the vanilla path left unchanged.
Everything runs offline on the synthetic fixture tests/fixtures/overhaul (invented names)."""

import json
import os
import re
import tempfile
import unittest
from types import SimpleNamespace
from unittest import mock

import numpy as np

import helpers
from hoi4_live.errors import ToolError
from hoi4_live.game import menu, panels
from hoi4_live.perception import screen, topbar, uiprofile
from hoi4_live.telemetry.modgen import mod_paths

FIX = os.path.join(helpers.FIXTURES, "overhaul")
GAME = os.path.join(FIX, "game")
USER = os.path.join(FIX, "user")
SHORT = ["Aru", "Bel", "Cor", "Dun", "Esk", "Fal", "Gim", "Hal", "Ild", "Jor", "Kes", "Lom"]


def line(text, x=300, y=100):
    return SimpleNamespace(text=text, center=(x, y), box=[x - 40, y - 8, 80, 16], words=None)


def ocr_result(*texts):
    return SimpleNamespace(lines=[line(t, y=100 + 20 * i) for i, t in enumerate(texts)])


class FakeOcr:
    def __init__(self, text):
        self.text = text

    def recognize(self, img, region=None, scale=None, pad=None):
        return SimpleNamespace(lines=[line(self.text)])


class ProfileCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_uiprofile_")
        self.addCleanup(self.tmp.cleanup)
        self.addCleanup(uiprofile.reset)
        uiprofile.reset()

    def derived(self, config_profile=None):
        return uiprofile.build(GAME, mod_paths(USER), "english", config_profile,
                               cache_dir=os.path.join(self.tmp.name, "loc"))


class VanillaTests(ProfileCase):
    def test_the_vanilla_profile_keeps_every_regex(self):
        p = uiprofile.active()
        self.assertTrue(p.is_vanilla)
        self.assertIsNone(p.month_table())
        for rx in (r"single\s*player", r"^\W*load(\s*game)?\W*$", r"exit\s*game"):
            self.assertIs(uiprofile.pattern("main_menu", "single_player", rx), rx)
        self.assertIs(p.section_rx(screen.ESC_MENU_WORDS, "pause_menu"), screen.ESC_MENU_WORDS)
        self.assertIs(p.panel_patterns(screen.PANELS)[0][1], screen.PANELS[0][1])
        self.assertEqual(p.main_menu_hits(["Begin the Saga", "Depart"]), (False, []))

    def test_the_vanilla_game_files_alone_derive_vanilla(self):
        p = uiprofile.Profile()
        uiprofile._derive(p, GAME, [], "english", cache_dir=os.path.join(self.tmp.name, "loc"))
        self.assertTrue(p.is_vanilla, p.summary())

    def test_no_mods_means_no_file_is_read(self):
        with mock.patch.object(uiprofile, "_derive") as derive:
            p = uiprofile.build(GAME, [], "english")
        derive.assert_not_called()
        self.assertTrue(p.is_vanilla)

    def test_vanilla_dates_parse_as_before(self):
        d = topbar.parse_date("12:00, 1 Jan, 1936")
        self.assertEqual((d["year"], d["month"], d["day"], d["hour"]), (1936, 1, 1, 12))
        self.assertEqual(topbar.parse_date("x 4:00, 6 March, 1936 y")["month"], 3)
        self.assertIsNone(topbar.parse_date("12:00, 1 Aru, 1936"))
        self.assertEqual(menu._date_rx("1936.1.1.12").pattern, r"\b12:00,?\s*1\s+January")


class DerivationTests(ProfileCase):
    def test_months_come_from_the_mods_localisation_keys(self):
        p = self.derived()
        self.assertEqual(p.month_names("short"), SHORT)
        self.assertEqual(p.month_names("long")[:4], ["Arunel", "Belmora", "Corvane", "Dunhollow"])
        self.assertEqual(p.month_names("long")[4], "Esk")              # one key for May, as in the game
        self.assertIn("Jan..Dec", p.sources["months.short"])

    def test_labels_follow_the_mods_gui_keys_and_replace_folders(self):
        p = self.derived()
        self.assertEqual(p.extra("pause_menu", "exit_game"), ["Leave the Realm"])       # the mod's own .gui key
        self.assertEqual(p.extra("pause_menu", "exit_to_menu"), ["Abdicate"])
        self.assertEqual(p.extra("pause_menu", "save_game"), ["Record Chronicle"])     # replace/ wins
        self.assertEqual(p.extra("pause_menu", "load_game"), [])                         # unchanged
        self.assertEqual(p.extra("panels", "decisions"), ["Edicts"])     # colour codes dropped, scoped by window
        self.assertIn("renamed interface element 'menu_quit_button'", p.sources["pause_menu.exit_game"])

    def test_a_rebuilt_main_menu_gives_its_own_buttons_away(self):
        p = self.derived()
        self.assertEqual(p.recognition[:2], ["Begin the Saga", "Depart"])
        self.assertEqual(p.main_menu_hits(["Begin the Saga", "Depart", "v1.0"]), (True, ["Begin the Saga", "Depart"]))
        self.assertFalse(p.main_menu_hits(["Depart"])[0])               # two anchors needed when more are known

    def test_the_mod_profile_adds_what_cannot_be_derived(self):
        p = self.derived()
        self.assertIn("Chronicle of Ages", p.recognition)
        self.assertEqual(p.extra("panels", "navy_overview"), ["Armada"])
        self.assertEqual(p.extra("main_menu", "new_game"), ["Open a Chronicle"])
        self.assertEqual(p.confirm, ["So be it"])
        self.assertTrue(p.profile_files[0].endswith(uiprofile.PROFILE_FILE))
        self.assertIn("profile:", p.sources["panels.navy_overview"])
        self.assertIn("derived:", p.sources["main_menu.anchors"])
        self.assertIn("profile:", p.sources["main_menu.anchors"])

    def test_the_run_config_overrides_the_derived_months(self):
        names = [f"M{i:02d}x" for i in range(1, 13)]
        p = self.derived(uiprofile.validate({"months": {"short": names}}))
        self.assertEqual(p.month_names("short"), names)
        self.assertEqual(p.month_names("long")[0], "Arunel")             # long names still derived
        self.assertTrue(p.sources["months.short"].startswith("profile:"))


class DateTests(ProfileCase):
    def setUp(self):
        super().setUp()
        uiprofile.activate(self.derived())

    def test_mod_month_names_parse(self):
        d = topbar.parse_date("12:00, 1 Aru, 1936")
        self.assertEqual((d["year"], d["month"], d["day"], d["hour"]), (1936, 1, 1, 12))
        self.assertEqual(topbar.parse_date("8:00, 14 Lom, 1937")["month"], 12)
        self.assertEqual(topbar.parse_date("8:00, 3 Corvane, 1937")["month"], 3)        # the long name too
        self.assertEqual(topbar.parse_date("8:00, a Kes, 1936")["day"], 4)              # OCR 'a' for 4
        self.assertEqual(topbar.parse_date("za:oo, 9 Fal, 1936")["hour"], 24)

    def test_vanilla_names_are_not_mistaken_for_mod_months(self):
        self.assertIsNone(topbar.parse_date("12:00, 1 Jan, 1936"))
        self.assertEqual(topbar.unrecognised_month("12:00, 1 Jan, 1936"), "Jan")
        self.assertIsNone(topbar.unrecognised_month("12:00, 1 Aru, 1936"))

    def test_read_date_names_the_unrecognised_month(self):
        img = np.zeros((1080, 1920, 3), np.uint8)
        self.assertEqual(topbar.read_date(img, FakeOcr("12:00, 2 Hal, 1936"))["month"], 8)
        err = topbar.read_date(img, FakeOcr("12:00, 2 Xyq, 1936"))
        self.assertEqual(err["unrecognised_month"], "Xyq")
        self.assertEqual(err["known_months"], SHORT)
        self.assertIn("'Xyq'", err["hint"])
        self.assertIn("mod profile", err["hint"])

    def test_the_time_tool_names_the_month(self):
        from hoi4_live.tools import time_tools
        d = {"error": "date not readable", "unrecognised_month": "Xyq", "known_months": SHORT}
        ev = {"error": "date not readable", "date": d, "unrecognised_month": "Xyq", "known_months": SHORT}
        with mock.patch.object(time_tools.timectl, "probe_running", return_value=(None, d, ev)):
            with self.assertRaises(ToolError) as cm:
                time_tools.hoi4_time.__wrapped__(SimpleNamespace(), {"action": "status"})
        self.assertIn("month 'Xyq'", str(cm.exception))
        self.assertEqual(cm.exception.details["error"], "unrecognised_month")

    def test_the_save_list_matches_the_long_month_names(self):
        rx = menu._date_rx("1936.3.5.12")
        self.assertTrue(rx.search("12:00, 5 Corvane, 1936"))
        self.assertFalse(rx.search("12:00, 5 March, 1936"))
        self.assertTrue(screen.SAVE_ROW_DATE_ANY.search("12:00, 5 Corvane, 1936"))


class ScreenTests(ProfileCase):
    def setUp(self):
        super().setUp()
        uiprofile.activate(self.derived())

    def test_panel_titles_include_the_mods(self):
        name, title = screen.detect_panel([line("EDICTS", x=200, y=100)], 1920, 1080)
        self.assertEqual(name, "decisions")
        self.assertEqual(screen.detect_panel([line("Armada", x=200, y=100)], 1920, 1080)[0], "navy_overview")
        self.assertEqual(screen.detect_panel([line("Production", x=200, y=100)], 1920, 1080)[0], "production")

    def test_main_menu_is_recognised_by_the_mods_anchors(self):
        with mock.patch.object(menu.ui, "ocr_screen", return_value=(None, ocr_result("Begin the Saga", "Depart"))):
            ok, texts = menu.main_menu_visible(SimpleNamespace())
        self.assertTrue(ok)
        with mock.patch.object(menu.ui, "ocr_screen", return_value=(None, ocr_result("Single Player"))):
            self.assertTrue(menu.main_menu_visible(SimpleNamespace())[0])         # vanilla still counts

    def test_pause_menu_and_buttons_use_the_mods_labels(self):
        texts = ocr_result("Load Game", "Record Chronicle", "Abdicate", "Leave the Realm")
        with mock.patch.object(menu.ui, "ocr_screen", return_value=(None, texts)), \
                mock.patch.object(menu, "_center", return_value=[0, 0, 10, 10]):
            ok, hits = menu.esc_menu_open(SimpleNamespace())
        self.assertTrue(ok)
        self.assertEqual(len(hits), 4)
        rx = re.compile(r"^\W*" + menu._rx("pause_menu", "exit_game", r"exit\s*game") + r"\W*$", re.I)
        self.assertTrue(rx.search("Leave the Realm"))
        self.assertTrue(rx.search("Exit Game"))
        self.assertTrue(re.search(menu._confirm_rx(menu.CONFIRM_RX), "So be it", re.I))

    def test_classify_sees_the_mods_main_menu(self):
        ocr = SimpleNamespace(recognize=lambda img, region=None, scale=None, pad=None:
                              ocr_result("Chronicle of Ages", "Begin the Saga", "Depart"))
        out = screen.classify(np.zeros((1080, 1920, 3), np.uint8), ocr)
        self.assertEqual(out["screen"], "main_menu")


class FailureTests(ProfileCase):
    def test_launch_names_the_missing_main_menu_anchors(self):
        from hoi4_live.game.logs import Logs
        from hoi4_live.tools import session_tools
        uiprofile.activate(self.derived())
        cfg = type("Cfg", (dict,), {})({"window": {"process_name": "hoi4"},
                                         "launch": {"args": [], "debug_args": ["-debug"]}})
        cfg.user_dir = cfg.game_dir = self.tmp.name
        cfg.game_exe = os.path.join(self.tmp.name, "hoi4.exe")
        session = mock.Mock()
        session.mode = "test"
        session.cfg = cfg
        session.logs = Logs(self.tmp.name)
        session.logs.idle_seconds = mock.Mock(return_value=60.0)
        found = iter([False, False])
        session.window.find.side_effect = lambda: next(found, True)
        session.platform.capture_process_identity.return_value = SimpleNamespace(pid=7, creation_time=1,
                                                                                 executable_path="x")
        polls = []

        def visible(s):
            polls.append(1)
            return False, ["A Splash Screen", "Press any key"]
        pids = iter([[]])
        args = {"debug": False, "wait_menu": True, "restart": False, "timeout_s": 900, "mode": None}
        with mock.patch.object(session_tools.process, "running_pids", side_effect=lambda *a: next(pids, [7])), \
                mock.patch.object(session_tools.process, "crash_dirs", return_value=set()), \
                mock.patch.object(session_tools.process, "launch", return_value=7), \
                mock.patch.object(session_tools, "_check_launch_alive"), \
                mock.patch.object(session_tools, "_ui_profile", return_value=uiprofile.active()), \
                mock.patch.object(session_tools.menu, "main_menu_visible", side_effect=visible):
            with self.assertRaises(ToolError) as cm:
                session_tools.hoi4_launch.__wrapped__(session, args)
        e = cm.exception
        self.assertEqual(e.details["error"], "main_menu_unrecognised")
        self.assertEqual(len(polls), session_tools.MENU_UNRECOGNISED_POLLS)
        self.assertIn("Begin the Saga", e.details["anchors"])
        self.assertIn("Single Player", e.details["anchors"])
        self.assertEqual(e.details["seen"], ["A Splash Screen", "Press any key"])
        self.assertIn("main menu not recognised", str(e))
        self.assertIn("main_menu.anchors", str(e))

    def test_a_panel_that_does_not_open_names_the_titles_it_looked_for(self):
        uiprofile.activate(self.derived())
        session = mock.Mock()
        session.window.size.return_value = (1920, 1080)
        session.grab.return_value = np.zeros((1080, 1920, 3), np.uint8)
        with mock.patch.object(panels, "current", return_value=None), \
                mock.patch.object(panels, "dismiss_notices"), \
                mock.patch.object(panels, "wait_panel", return_value=None), \
                mock.patch.object(panels.time, "sleep"):
            with self.assertRaises(ToolError) as cm:
                panels.open_panel(session, "decisions")
        self.assertEqual(cm.exception.details["expected_titles"], ["Events and Decisions", "Edicts"])
        self.assertIn("panels.decisions", str(cm.exception))


class ValidationTests(ProfileCase):
    def test_errors_name_every_problem(self):
        with self.assertRaises(uiprofile.ProfileError) as cm:
            uiprofile.validate({"months": {"short": ["a"] * 11}, "panels": {"armory": "X"}, "colour": 1,
                                "main_menu": {"single_player": []}}, "p.json")
        msg = str(cm.exception)
        self.assertTrue(msg.startswith("p.json: "))
        for part in ("months.short must list 12 month names", "got 11", "panels.armory is not a known name",
                     "unknown key 'colour'", "main_menu.single_player must be a label"):
            self.assertIn(part, msg)

    def test_a_label_may_be_one_string_or_a_list(self):
        out = uiprofile.validate({"format": 1, "language": "l_english", "setup": {"start": "Embark"},
                                  "months": {"long": [["Arunel", "Arun."]] + [f"N{i}" for i in range(11)]}})
        self.assertEqual(out["setup"]["start"], ["Embark"])
        self.assertEqual(out["language"], "english")
        self.assertEqual(out["months"]["long"][0], ["Arunel", "Arun."])

    def test_bad_json_and_a_bad_mod_profile_are_reported_not_applied(self):
        mod = os.path.join(self.tmp.name, "badmod")
        os.makedirs(mod)
        with open(os.path.join(mod, uiprofile.PROFILE_FILE), "w", encoding="utf-8") as f:
            f.write('{"months": {"short": [1, 2]}')
        p = uiprofile.build(GAME, [{"path": mod}], "english", cache_dir=os.path.join(self.tmp.name, "loc"))
        self.assertEqual(len(p.errors), 1)
        self.assertIn("not valid JSON", p.errors[0])
        self.assertIsNone(p.month_table())


class EnsureTests(ProfileCase):
    def session(self, data=None, user=USER):
        return SimpleNamespace(cfg=SimpleNamespace(game_dir=GAME, user_dir=user, data=data or {}, source=None))

    def ensure(self, session, env=None):
        environ = {k: v for k, v in os.environ.items() if k != "HOI4_LIVE_UI_PROFILE"}
        environ.update(env or {})
        with mock.patch.dict(os.environ, environ, clear=True), \
                mock.patch("hoi4_live.gamedata.loc.for_session",
                           side_effect=lambda s: _fixture_localisation(self.tmp.name)):
            return uiprofile.ensure(session)

    def test_the_playset_is_derived_and_activated(self):
        p = self.ensure(self.session())
        self.assertIs(uiprofile.active(), p)
        self.assertEqual(p.month_names("short"), SHORT)
        self.assertIs(self.ensure(self.session()), p)                   # nothing changed: the same profile

    def test_off_keeps_vanilla(self):
        p = self.ensure(self.session({"ui_profile": "off"}))
        self.assertIs(p, uiprofile.VANILLA)
        self.assertIs(self.ensure(self.session(), env={"HOI4_LIVE_UI_PROFILE": "vanilla"}), uiprofile.VANILLA)

    def test_an_inline_config_profile_applies_last(self):
        p = self.ensure(self.session({"ui_profile": {"setup": {"start": "Embark"}}}))
        self.assertEqual(p.extra("setup", "start"), ["Embark"])
        self.assertEqual(p.month_names("short"), SHORT)

    def test_a_profile_file_named_in_the_config_is_read(self):
        path = os.path.join(self.tmp.name, "profile.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump({"panels": {"trade": "Markets"}}, f)
        p = self.ensure(self.session({"ui_profile": path}))
        self.assertEqual(p.extra("panels", "trade"), ["Markets"])
        self.assertEqual(p.errors, [])

    def test_an_invalid_config_profile_is_an_error_and_launch_refuses(self):
        from hoi4_live.tools import session_tools
        session = self.session({"ui_profile": {"months": {"short": ["x"]}}})
        p = self.ensure(session)
        self.assertIn("months.short must list 12 month names", p.errors[0])
        with mock.patch.object(uiprofile, "ensure", return_value=p):
            with self.assertRaises(ToolError) as cm:
                session_tools._ui_profile(session)
        self.assertEqual(cm.exception.details["error"], "ui_profile_invalid")

    def test_a_user_folder_without_mods_is_vanilla(self):
        user = os.path.join(self.tmp.name, "user")
        os.makedirs(user)
        self.assertTrue(self.ensure(self.session(user=user)).is_vanilla)


def _fixture_localisation(tmp):
    from hoi4_live.gamedata.loc import Localisation
    return Localisation(GAME, [m["path"] for m in mod_paths(USER)], cache_dir=os.path.join(tmp, "loc"))


if __name__ == "__main__":
    unittest.main()
