"""The first-run doctor: every decision from facts, on every system, and the fact readers on temporary files."""

import copy
import json
import os
import shutil
import tempfile
import unittest
from unittest import mock

import helpers  # noqa: F401

from hoi4_live import doctor

MAC_SHA = "d" * 64
BUILDS = {"runtime": "1.5.0", "system": "macos", "meanwhile": "Until a release supports your build: hoi4-live update",
          "builds": [{"system": "macos", "label": "macOS", "game_version": "1.19.4.1", "build_id": "UUID",
                      "sha256": MAC_SHA},
                     {"system": "linux", "label": "Linux x86_64", "game_version": "1.19.4.1", "build_id": "abc",
                      "sha256": "e" * 64}]}
NATIVE_READY = "macOS: memory access is ready; nothing to renew\n  [ok] the tool is signed with the debugger entitlement\n"
NATIVE_MISSING = ("macOS: memory access is NOT ready\n"
                  "  [ok] the tool is signed with the debugger entitlement\n"
                  "  [missing] Developer mode is off\n"
                  "        do this once: sudo DevToolsSecurity -enable\n"
                  "  The tool changes no security setting and asks for no password; the steps above are yours.\n")


def facts(system="macos", **changes):
    """A computer where everything is in place."""
    base = {
        "system": system, "version": "1.5.0", "prefix": "/p",
        "server": {"expected": system != "linux", "installed": system != "linux",
                   "reason": "the Python server does not drive the game on Linux" if system == "linux" else None},
        "native": {"expected": system != "windows", "installed": system != "windows", "path": "/p/native/x",
                   "version": "1.5.0",
                   "reason": "the native runtime's Windows adapter is unfinished" if system == "windows" else None},
        "game": {"dir": "/games/Hearts of Iron IV", "exe": "/games/Hearts of Iron IV/hoi4", "version": "1.19.4.1",
                 "version_text": "Operation Postern v1.19.4.1 (f584)",
                 "sha256": MAC_SHA if system == "macos" else "e" * 64},
        "display": {"mode": "borderless_fullscreen", "resolution": (1920, 1080), "gui_scale": 1.0},
        "mods": 0, "steam_running": None if system == "windows" else False,
        "builds": copy.deepcopy(BUILDS) if system != "windows" else None,
        "memory": {"code": 0, "text": NATIVE_READY} if system != "windows" else None,
        "selftest": [(True, "python", "3.13"), (True, "cv2", "importable")] if system != "linux" else None,
    }
    if system == "macos":
        base.update({"accessibility": True, "screen_capture": True, "screen": (2560, 1440)})
    for key, value in changes.items():
        if isinstance(value, dict) and isinstance(base.get(key), dict):
            base[key] = {**base[key], **value}
        else:
            base[key] = value
    return base


def fixes(items, needs=None):
    return [i for i in items if i["status"] == doctor.FIX and (needs is None or i["needs"] == needs)]


class DecisionTests(unittest.TestCase):
    def test_a_computer_with_everything_in_place_is_ready_on_each_system(self):
        for system in ("macos", "windows", "linux"):
            f = facts(system)
            items = doctor.evaluate(f)
            self.assertEqual(fixes(items), [], system)
            self.assertTrue(doctor.ready(f, items), system)
        states = dict((label, steps) for label, steps, _why in doctor.runtimes(facts("windows"), []))
        self.assertEqual(states, {"Python server": 0, "native runtime": None})
        states = dict((label, steps) for label, steps, _why in doctor.runtimes(facts("linux"), []))
        self.assertEqual(states, {"Python server": None, "native runtime": 0})

    def test_a_missing_install_part_is_fixed_by_update(self):
        f = facts("macos", server={"installed": False}, native={"installed": False}, memory=None, builds=None,
                  selftest=None)
        found = fixes(doctor.check_install(f))
        self.assertEqual([i["fix"] for i in found], ["hoi4-live update", "hoi4-live update"])
        self.assertEqual({i["needs"] for i in found}, {doctor.SERVER, doctor.NATIVE})
        self.assertFalse(doctor.ready(f, doctor.evaluate(f)))

    def test_a_game_that_is_not_found_blocks_both_runtimes_and_names_the_command(self):
        f = facts("windows", game={"exe": None, "dir": None})
        items = doctor.evaluate(f)
        found = fixes(items)
        self.assertEqual(len(found), 1)
        self.assertIn("hoi4-live game-dir", found[0]["fix"])
        self.assertEqual(found[0]["needs"], doctor.BOTH)
        self.assertFalse(doctor.ready(f, items))

    def test_version_drift_names_the_supported_build_and_what_works_meanwhile(self):
        for system in ("macos", "linux"):
            f = facts(system, game={"sha256": "0" * 64, "version": "1.20.0", "version_text": "Next v1.20.0 (abcd)"})
            items = doctor.evaluate(f)
            found = fixes(items, doctor.NATIVE)
            self.assertEqual(len(found), 1, system)
            self.assertIn("Next v1.20.0 (abcd)", found[0]["title"])
            self.assertIn("HOI4 1.19.4.1", found[0]["title"])
            self.assertIn("1.5.0", found[0]["title"])
            self.assertEqual(found[0]["fix"], BUILDS["meanwhile"])
            self.assertFalse(doctor.ready(f, items))
        # The Python server has no build check: it stays ready and says what it was run on.
        f = facts("macos", game={"sha256": "0" * 64, "version": "1.20.0"})
        items = doctor.evaluate(f)
        self.assertEqual(fixes(items, doctor.SERVER), [])
        self.assertTrue([i for i in items if i["status"] == doctor.NOTE and "no build check" in i["title"]])
        self.assertEqual(dict((l, s) for l, s, _ in doctor.runtimes(f, items)),
                         {"Python server": 0, "native runtime": 1})

    def test_a_runtime_that_cannot_list_its_builds_asks_for_an_update_without_blocking(self):
        f = facts("macos", builds=None)
        items = doctor.evaluate(f)
        self.assertTrue([i for i in items if i["status"] == doctor.NOTE and i["fix"] == "hoi4-live update"])
        self.assertTrue(doctor.ready(f, items))

    def test_memory_access_items_come_from_the_native_doctor_with_their_steps(self):
        checks, notes = doctor.parse_native_doctor(NATIVE_MISSING)
        self.assertEqual(checks, [(True, "the tool is signed with the debugger entitlement", None),
                                  (False, "Developer mode is off", "sudo DevToolsSecurity -enable")])
        self.assertEqual(len(notes), 1)
        f = facts("macos", memory={"code": 1, "text": NATIVE_MISSING})
        found = fixes(doctor.check_memory(f))
        self.assertEqual([(i["title"], i["fix"], i["needs"]) for i in found],
                         [("Developer mode is off", "sudo DevToolsSecurity -enable", doctor.NATIVE)])
        f = facts("linux", memory={"code": None, "text": "exec format error"})
        self.assertEqual(fixes(doctor.check_memory(f))[0]["fix"], "hoi4-live update")
        f = facts("linux", memory={"code": 1, "text": "Linux: memory access is NOT ready\n"})
        self.assertEqual(len(fixes(doctor.check_memory(f))), 1)
        self.assertEqual(doctor.check_memory(facts("windows")), [])

    def test_a_runtime_from_before_its_memory_check_asks_for_an_update_without_a_bare_error(self):
        # 1.4.2 answers --doctor with its argument error (seen on a real install).
        f = facts("macos", memory={"code": 1, "text": 'Error: "Unknown argument"\n'})
        items = doctor.check_memory(f)
        self.assertEqual([(i["status"], i["fix"]) for i in items], [(doctor.NOTE, "hoi4-live update")])
        self.assertNotIn("Unknown argument", items[0]["title"])

    def test_macos_permissions_name_the_settings_pane_and_are_never_changed_here(self):
        f = facts("macos", accessibility=False, screen_capture=False)
        found = fixes(doctor.check_permissions(f))
        self.assertEqual(len(found), 2)
        self.assertIn("Privacy & Security > Accessibility", found[0]["fix"])
        self.assertEqual(found[0]["needs"], doctor.BOTH)
        self.assertIn("Screen & System Audio Recording", found[1]["fix"])
        self.assertEqual(found[1]["needs"], doctor.SERVER)
        unknown = doctor.check_permissions(facts("macos", accessibility=None, screen_capture=None))
        self.assertEqual(unknown, [])

    def test_windows_and_linux_say_what_input_needs(self):
        notes = doctor.check_permissions(facts("windows"))
        self.assertEqual([i["status"] for i in notes], [doctor.NOTE])
        self.assertIn("Run as administrator", notes[0]["title"])
        notes = doctor.check_permissions(facts("linux"))
        self.assertIn("not automatic on Linux yet", notes[0]["title"])

    def test_display_mode_scale_and_resolution(self):
        self.assertEqual(fixes(doctor.check_display(facts("windows"))), [])
        f = facts("windows", display={"mode": "fullscreen", "gui_scale": 1.5, "resolution": (1280, 720)})
        items = doctor.check_display(f)
        found = fixes(items)
        self.assertEqual(len(found), 2)
        self.assertIn("Display mode: Borderless", found[0]["fix"])
        self.assertIn("UI scaling: 1.0", found[1]["fix"])
        self.assertTrue(all(i["needs"] == doctor.SERVER for i in items))
        self.assertTrue([i for i in items if i["status"] == doctor.NOTE and "1280x720" in i["title"]])
        f = facts("macos", display={"mode": "windowed", "resolution": (1710, 1107)}, screen=(1710, 1107))
        note = [i for i in doctor.check_display(f) if i["status"] == doctor.NOTE][0]
        self.assertIn("cannot hold a 1920x1080 window", note["title"])
        self.assertEqual(doctor.check_display(facts("linux")), [])
        self.assertEqual(doctor.check_display(facts("windows", display={"mode": None, "resolution": None,
                                                                         "gui_scale": None})), [])

    def test_the_doctor_says_what_play_does_on_each_system(self):
        self.assertIn("attaches", doctor.play_summary(facts("macos")))
        self.assertIn("Python server", doctor.play_summary(facts("macos", native={"installed": False})))
        self.assertIn("no Windows adapter", doctor.play_summary(facts("windows")))
        self.assertIn("cannot start", doctor.play_summary(facts("linux")))
        self.assertIn("hoi4-live play on this system: attaches", doctor.render(facts("macos"), []))

    def test_steam_is_how_the_user_starts_the_game(self):
        items = doctor.check_steam(facts("macos", steam_running=True))
        self.assertEqual([i["status"] for i in items], [doctor.NOTE])
        self.assertIn("attaches", items[0]["title"])
        self.assertEqual(fixes(doctor.check_steam(facts("macos", steam_running=False))), [])
        self.assertEqual(doctor.check_steam(facts("windows", steam_running=True)), [])
        self.assertEqual(doctor.check_steam(facts("linux", steam_running=True)), [])

    def test_native_display_profile_on_macos(self):
        windowed = {"mode": "windowed", "resolution": (1920, 1080), "gui_scale": 1.0}
        items = doctor.check_native_display(facts("macos", display=windowed))
        self.assertEqual([i["status"] for i in items], [doctor.OK])
        # Game settings are notes: hoi4-live play waits while the user sets them in the running game.
        items = doctor.check_native_display(facts("macos", display={"mode": "fullscreen", "resolution": (2560, 1440),
                                                                    "gui_scale": 1.25}))
        self.assertEqual([i["status"] for i in items], [doctor.NOTE] * 3)
        self.assertTrue(all(i["needs"] == doctor.NATIVE for i in items))
        self.assertIn("Windowed", items[0]["fix"])
        # A screen too small for the window must change before the game starts: a step.
        found = fixes(doctor.check_native_display(facts("macos", display=windowed, screen=(1710, 1107))))
        self.assertEqual(len(found), 1)
        self.assertIn("More Space", found[0]["fix"])
        self.assertEqual(doctor.check_native_display(facts("linux")), [])
        self.assertEqual(doctor.check_native_display(facts("macos", native={"installed": False})), [])

    def test_mods_are_reported_per_runtime_and_never_block(self):
        self.assertEqual(doctor.check_mods(facts("macos"))[0]["status"], doctor.OK)
        items = doctor.check_mods(facts("macos", mods=3))
        self.assertEqual([i["needs"] for i in items], [doctor.SERVER, doctor.NATIVE])
        self.assertIn("mods included", items[1]["title"])
        self.assertEqual(fixes(items), [])
        self.assertEqual(doctor.check_mods(facts("macos", mods=None)), [])

    def test_server_selftest_lines_get_a_step_each(self):
        lines = [(False, "cv2", "No module named cv2"), (False, "ffmpeg", "not found"),
                 (False, "native ocr", "OcrError"), (False, "game_dir", "None"), (True, "numpy", "importable")]
        items = doctor.check_selftest(facts("windows", selftest=lines))
        found = fixes(items)
        self.assertEqual(found[0]["fix"], "hoi4-live update")
        self.assertIn("Optical character recognition", found[1]["fix"])
        self.assertEqual(len(found), 2)
        self.assertEqual([i["status"] for i in items if "FFmpeg" in i["title"]], [doctor.NOTE])
        mac = fixes(doctor.check_selftest(facts("macos", selftest=[(False, "native ocr", "x")])))
        self.assertEqual(mac[0]["fix"], "hoi4-live update")

    def test_report_marks_each_item_and_ends_with_a_line_per_runtime(self):
        f = facts("macos", memory={"code": 1, "text": NATIVE_MISSING}, mods=2)
        text = doctor.render(f, doctor.evaluate(f))
        self.assertTrue(text.startswith("HOI4 Live Tools 1.5.0 doctor (macOS)\n"))
        self.assertIn("  [fix]  Developer mode is off\n          do this: sudo DevToolsSecurity -enable\n", text)
        self.assertIn("\nPython server: ready\n", text)
        self.assertIn("\nnative runtime: 1 step to do (marked [fix] above)\n", text)
        self.assertIn("The doctor only reads", text)
        text = doctor.render(facts("windows"), doctor.evaluate(facts("windows")))
        self.assertIn("native runtime: not installed here (the native runtime's Windows adapter is unfinished)", text)


class FactTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_doc_")
        self.addCleanup(shutil.rmtree, self.tmp, True)

    def write(self, name, text):
        path = os.path.join(self.tmp, name)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)
        return path

    def test_game_version_and_hash_come_from_the_install(self):
        exe = self.write("hoi4", "game bytes")
        self.write("launcher-settings.json", json.dumps({"version": "Operation Postern v1.19.4.1.99a5 (f584)",
                                                          "rawVersion": "v1.19.4.1"}))
        game = doctor.read_game(self.tmp, exe)
        self.assertEqual(game["version"], "1.19.4.1")
        self.assertEqual(game["version_text"], "Operation Postern v1.19.4.1.99a5 (f584)")
        self.assertEqual(game["sha256"], doctor.hashlib.sha256(b"game bytes").hexdigest())
        self.assertIsNone(doctor.read_game(self.tmp, os.path.join(self.tmp, "absent"))["exe"])
        self.write("launcher-settings.json", "not json")
        self.assertIsNone(doctor.read_game(self.tmp, exe)["version"])

    def test_display_settings_are_read_from_the_games_files(self):
        self.write("pdx_settings.txt", '"Graphics"={\n\t"display_mode"={\n\t\tvalue="windowed"\n\t\tversion=0\n\t}\n'
                   '\t"fullscreen_resolution"={\n\t\tvalue="2560x1440"\n\t}\n'
                   '\t"windowed_resolution"={\n\t\tvalue="1710x1107"\n\t}\n}\n')
        self.write("settings.txt", "graphics={\n\tsize={ x=1710 y=1107 }\n\tgui_scale=1.250000\n}\n")
        self.assertEqual(doctor.read_display(self.tmp),
                         {"mode": "windowed", "resolution": (1710, 1107), "gui_scale": 1.25})
        self.write("pdx_settings.txt", '"Graphics"={ "display_mode"={ value="fullscreen" } '
                   '"fullscreen_resolution"={ value="2560x1440" } }')
        self.assertEqual(doctor.read_display(self.tmp)["resolution"], (2560, 1440))
        empty = doctor.read_display(os.path.join(self.tmp, "absent"))
        self.assertEqual(empty, {"mode": None, "resolution": None, "gui_scale": None})

    def test_mods_are_counted_from_the_playset_file(self):
        self.assertIsNone(doctor.read_mods(self.tmp))
        self.write("dlc_load.json", json.dumps({"enabled_mods": ["mod/a.mod", "mod/b.mod"], "disabled_dlcs": []}))
        self.assertEqual(doctor.read_mods(self.tmp), 2)
        self.write("dlc_load.json", "[]")
        self.assertIsNone(doctor.read_mods(self.tmp))

    def test_steam_is_found_by_process_name(self):
        listing = "/sbin/launchd\n/Applications/Steam.app/Contents/MacOS/steam_osx\n/usr/bin/zsh\n"
        with mock.patch.object(doctor, "run", return_value=(0, listing)):
            self.assertTrue(doctor.steam_running("macos"))
            self.assertFalse(doctor.steam_running("linux"))
            self.assertIsNone(doctor.steam_running("windows"))
        with mock.patch.object(doctor, "run", return_value=(None, "no ps")):
            self.assertIsNone(doctor.steam_running("linux"))

    def test_system_names_and_selftest_lines(self):
        self.assertEqual([doctor.system_name(p) for p in ("win32", "darwin", "linux")],
                         ["windows", "macos", "linux"])
        text = "ok   python: 3.13.1\nFAIL cv2: No module named 'cv2'\nnoise\n"
        with mock.patch.object(doctor, "run", return_value=(1, text)):
            self.assertEqual(doctor.read_selftest("launcher"),
                             [(True, "python", "3.13.1"), (False, "cv2", "No module named 'cv2'")])
        with mock.patch.object(doctor, "run", return_value=(None, "cannot start")):
            self.assertEqual(doctor.read_selftest("launcher"), [(False, "tools", "cannot start")])

    def test_gather_reads_an_install_and_a_game_without_touching_the_system(self):
        prefix = os.path.join(self.tmp, "prefix")
        game = os.path.join(self.tmp, "game")
        user = os.path.join(self.tmp, "user")
        for name in ("hoi4.exe", "hoi4", os.path.join("hoi4.app", "Contents", "MacOS", "hoi4")):
            self.write(os.path.join("game", name), "game bytes")
        self.write(os.path.join("game", "launcher-settings.json"), json.dumps({"rawVersion": "1.19.4.1"}))
        self.write(os.path.join("user", "dlc_load.json"), json.dumps({"enabled_mods": ["a"]}))
        self.write(os.path.join("prefix", "install.json"), json.dumps({"version": "9.9.9", "native": {
            "status": "unavailable", "detail": "no build for this processor"}}))
        env = {"HOI4_LIVE_GAME_DIR": game, "HOI4_LIVE_USER_DIR": user,
               "HOI4_LIVE_CONFIG": os.path.join(self.tmp, "absent.json")}
        with mock.patch.dict(os.environ, env), \
                mock.patch.object(doctor, "steam_running", return_value=False), \
                mock.patch.object(doctor, "mac_permissions", return_value={"accessibility": True,
                                                                           "screen_capture": True, "screen": None}):
            found = doctor.gather(prefix)
        self.assertEqual(found["version"], "9.9.9")
        self.assertEqual(found["game"]["version"], "1.19.4.1")
        self.assertEqual(found["game"]["sha256"], doctor.hashlib.sha256(b"game bytes").hexdigest())
        self.assertEqual(found["mods"], 1)
        self.assertFalse(found["server"]["installed"])
        self.assertFalse(found["native"]["expected"])
        self.assertEqual(found["native"]["reason"], "no build for this processor")
        items = doctor.evaluate(found)
        self.assertTrue(any("no build for this processor" in i["title"] for i in items))
        self.assertIn("doctor (", doctor.render(found, items))


if __name__ == "__main__":
    unittest.main()
