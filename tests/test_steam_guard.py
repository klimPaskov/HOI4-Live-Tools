"""The launcher never starts the game while Steam or steamcmd runs, and passes no Steam variables to it."""

import unittest
from unittest import mock

from hoi4_live.errors import ToolError
from hoi4_live.game import process


class FakePlatform:
    def __init__(self, running):
        self.running = running

    def find_processes(self, name):
        return [4242] if name in self.running else []


class SteamGuardTest(unittest.TestCase):
    def test_refuses_each_windows_steam_process(self):
        for name in ("steam.exe", "steamcmd.exe", "steamwebhelper.exe"):
            with self.assertRaises(ToolError):
                process.refuse_running_steam(FakePlatform({name}), windows=True)

    def test_refuses_each_posix_steam_process(self):
        for name in ("steam", "steamcmd", "steamwebhelper", "steam_osx"):
            with self.assertRaises(ToolError):
                process.refuse_running_steam(FakePlatform({name}), windows=False)

    def test_allows_when_steam_is_closed(self):
        process.refuse_running_steam(FakePlatform({"hoi4", "ipcserver"}), windows=False)
        process.refuse_running_steam(FakePlatform({"hoi4.exe"}), windows=True)

    def test_env_has_no_steam_variables(self):
        env = process.launch_env({
            "PATH": "/usr/bin", "HOME": "/h", "SteamAppId": "394360", "SteamGameId": "394360",
            "SteamClientLaunch": "1", "STEAM_COMPAT_DATA_PATH": "/x", "SteamOverlayGameId": "1",
            "ENABLE_VK_LAYER_VALVE_steam_overlay_1": "1",
            "LD_PRELOAD": "/s/ubuntu12_32/gameoverlayrenderer.so:/usr/lib/libkeep.so",
            "DYLD_INSERT_LIBRARIES": "/s/gameoverlayrenderer.dylib",
        })
        self.assertEqual(env, {"PATH": "/usr/bin", "HOME": "/h", "LD_PRELOAD": "/usr/lib/libkeep.so"})

    def test_launch_refuses_before_starting(self):
        with mock.patch.object(process, "steam_processes", return_value=[("steam.exe", 1)]), \
                mock.patch.object(process.os.path, "exists", return_value=True), \
                mock.patch.object(process.subprocess, "Popen") as popen:
            with self.assertRaises(ToolError):
                process.launch("/game", [])
            popen.assert_not_called()


if __name__ == "__main__":
    unittest.main()
