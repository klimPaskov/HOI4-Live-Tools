"""Launches are refused while Steam or steamcmd runs only when HOI4_LIVE_REQUIRE_STEAM_CLOSED asks for it, and no
Steam variable reaches the game."""

import unittest
from unittest import mock

from hoi4_live.errors import ToolError
from hoi4_live.game import process


class FakePlatform:
    def __init__(self, running):
        self.running = running

    def find_processes(self, name):
        return [4242] if name in self.running else []


ON = {process.REQUIRE_STEAM_CLOSED_ENV: "1"}


class SteamGuardTest(unittest.TestCase):
    def test_refuses_each_windows_steam_process(self):
        for name in ("steam.exe", "steamcmd.exe", "steamwebhelper.exe"):
            with self.assertRaises(ToolError):
                process.refuse_running_steam(FakePlatform({name}), windows=True, env=ON)

    def test_refuses_each_posix_steam_process(self):
        for name in ("steam", "steamcmd", "steamwebhelper", "steam_osx"):
            with self.assertRaises(ToolError):
                process.refuse_running_steam(FakePlatform({name}), windows=False, env=ON)

    def test_allows_when_steam_is_closed(self):
        process.refuse_running_steam(FakePlatform({"hoi4", "ipcserver"}), windows=False, env=ON)
        process.refuse_running_steam(FakePlatform({"hoi4.exe"}), windows=True, env=ON)

    def test_off_by_default_even_while_steam_runs(self):
        for env in ({}, {process.REQUIRE_STEAM_CLOSED_ENV: ""}, {process.REQUIRE_STEAM_CLOSED_ENV: "0"},
                    {process.REQUIRE_STEAM_CLOSED_ENV: "off"}):
            self.assertFalse(process.steam_check_required(env))
            process.refuse_running_steam(FakePlatform({"steam.exe"}), windows=True, env=env)
            process.refuse_running_steam(FakePlatform({"steam"}), windows=False, env=env)

    def test_opt_in_values(self):
        for value in ("1", "true", "YES", " on "):
            self.assertTrue(process.steam_check_required({process.REQUIRE_STEAM_CLOSED_ENV: value}))

    def test_env_has_no_steam_variables(self):
        env = process.launch_env({
            "PATH": "/usr/bin", "HOME": "/h", "SteamAppId": "394360", "SteamGameId": "394360",
            "SteamClientLaunch": "1", "STEAM_COMPAT_DATA_PATH": "/x", "SteamOverlayGameId": "1",
            "ENABLE_VK_LAYER_VALVE_steam_overlay_1": "1",
            "LD_PRELOAD": "/s/ubuntu12_32/gameoverlayrenderer.so:/usr/lib/libkeep.so",
            "DYLD_INSERT_LIBRARIES": "/s/gameoverlayrenderer.dylib",
        })
        self.assertEqual(env, {"PATH": "/usr/bin", "HOME": "/h", "LD_PRELOAD": "/usr/lib/libkeep.so"})

    def test_launch_refuses_before_starting_when_opted_in(self):
        with mock.patch.dict(process.os.environ, ON), \
                mock.patch.object(process, "steam_processes", return_value=[("steam.exe", 1)]), \
                mock.patch.object(process.os.path, "exists", return_value=True), \
                mock.patch.object(process.subprocess, "Popen") as popen:
            with self.assertRaises(ToolError):
                process.launch("/game", [])
            popen.assert_not_called()

    def test_launch_goes_ahead_by_default_while_steam_runs(self):
        env = {k: v for k, v in process.os.environ.items() if k != process.REQUIRE_STEAM_CLOSED_ENV}
        with mock.patch.dict(process.os.environ, env, clear=True), \
                mock.patch.object(process, "steam_processes", return_value=[("steam.exe", 1)]), \
                mock.patch.object(process.os.path, "exists", return_value=True), \
                mock.patch.object(process.platform, "current") as cur, \
                mock.patch.object(process.subprocess, "Popen") as popen:
            cur.return_value.game_executable.return_value = "/game/hoi4"
            popen.return_value.pid = 99
            self.assertEqual(process.launch("/game", []), 99)
            popen.assert_called_once()


if __name__ == "__main__":
    unittest.main()
