"""Linux path discovery and typed unsupported capabilities without a desktop."""

from dataclasses import replace
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

from hoi4_live import platform
from hoi4_live.errors import CapabilityUnavailable
from hoi4_live.platform.linux import LinuxPlatform
from hoi4_live.platform.linux_process import ExecutableWitness, LinuxProcessIdentity, ProcessStat, creation_token


BOOT_ID = "12345678-1234-4567-89ab-123456789abc"
IDENTITY = LinuxProcessIdentity(42, creation_token(BOOT_ID, 1234), "/fixture/hoi4", BOOT_ID, 1234, 8, 99)


class LinuxPathTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()
        self.home = self.root / "home"
        self.home.mkdir()
        self.backend = LinuxPlatform(home=str(self.home), environ={})

    def binary(self, path):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"\x7fELFfixture")
        path.chmod(0o755)

    def test_linux_defaults_and_absolute_xdg_overrides(self):
        self.assertEqual(self.backend.documents_dir(), str(self.home / "Documents"))
        self.assertEqual(self.backend.user_dir(), str(self.home / ".local/share/Paradox Interactive/Hearts of Iron IV"))
        self.assertEqual(self.backend.local_data_dir(), str(self.home / ".cache"))
        backend = LinuxPlatform(home=str(self.home), environ={"XDG_DATA_HOME": str(self.root / "data"),
                                                            "XDG_CACHE_HOME": str(self.root / "cache")})
        self.assertEqual(backend.user_dir(), str(self.root / "data/Paradox Interactive/Hearts of Iron IV"))
        self.assertEqual(backend.local_data_dir(), str(self.root / "cache"))
        relative = LinuxPlatform(home=str(self.home), environ={"XDG_DATA_HOME": "relative", "XDG_CACHE_HOME": ""})
        self.assertEqual(relative.user_dir(), self.backend.user_dir())
        self.assertEqual(relative.local_data_dir(), self.backend.local_data_dir())

    def test_documents_file_is_parsed_without_executing_shell(self):
        path = self.home / ".config/user-dirs.dirs"
        path.parent.mkdir()
        path.write_text('XDG_DOCUMENTS_DIR="$HOME/Document Folder" # comment\n')
        self.assertEqual(self.backend.documents_dir(), str(self.home / "Document Folder"))
        for value in ('relative', '$OTHER/Documents', '$(touch /tmp/unsafe)'):
            path.write_text('XDG_DOCUMENTS_DIR="%s"\n' % value)
            self.assertEqual(self.backend.documents_dir(), str(self.home / "Documents"))

    def test_native_steam_library_discovery_including_escaped_custom_path(self):
        steam = self.home / ".local/share/Steam/steamapps"
        steam.mkdir(parents=True)
        library = self.root / ('External Steam Library' if os.name == "nt" else 'External "Steam" Library')
        game = library / "steamapps/common/Hearts of Iron IV"
        self.binary(game / "hoi4")
        escaped = str(library).replace("\\", "\\\\").replace('"', '\\"')
        (steam / "libraryfolders.vdf").write_text('"libraryfolders" { "1" { "path" "%s" } }' % escaped)
        self.assertEqual(self.backend.game_dir(), str(game))
        self.assertEqual(self.backend.game_executable(str(game)), str(game / "hoi4"))

    def test_legacy_library_manifest_and_flatpak_paths(self):
        steam = self.home / ".steam/steam/steamapps"
        steam.mkdir(parents=True)
        library = self.root / "legacy"
        game = library / "steamapps/common/Renamed Game"
        self.binary(game / "hoi4")
        (steam / "libraryfolders.vdf").write_text('"LibraryFolders" { "1" "%s" }' % library)
        (library / "steamapps/appmanifest_394360.acf").write_text('"AppState" { "installdir" "Renamed Game" }')
        self.assertEqual(self.backend.game_dir(), str(game))
        (steam / "libraryfolders.vdf").unlink()
        flatpak = self.home / ".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common/Hearts of Iron IV"
        self.binary(flatpak / "hoi4")
        self.assertEqual(self.backend.game_dir(), str(flatpak))

    def test_only_native_executable_and_safe_manifest_leaf_are_accepted(self):
        game = self.home / ".local/share/Steam/steamapps/common/Hearts of Iron IV"
        self.binary(game / "hoi4.exe")
        self.assertIsNone(self.backend.game_dir())
        self.assertIsNone(self.backend.game_executable(str(game)))
        (game / "hoi4").write_bytes(b"MZwindows")
        (game / "hoi4").chmod(0o755)
        self.assertIsNone(self.backend.game_executable(str(game)))
        (game.parent.parent / "appmanifest_394360.acf").write_text('"installdir" "../../escape"')
        self.assertIsNone(self.backend.game_dir())


class LinuxBackendTests(unittest.TestCase):
    def test_platform_factory_selects_linux_and_imports_are_inert(self):
        with patch("hoi4_live.platform.sys.platform", "linux"):
            self.assertIsInstance(platform._detect(), LinuxPlatform)
        code = """
import sys
import hoi4_live.platform.linux
import hoi4_live.platform.linux_process
import hoi4_live.platform.linux_memory
assert 'hoi4_live.platform.windows' not in sys.modules
assert 'hoi4_live.platform.win32' not in sys.modules
assert 'Quartz' not in sys.modules
assert 'Vision' not in sys.modules
"""
        result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_proc_listing_rejects_zombies_and_treats_permission_denial_conservatively(self):
        api = Mock()
        api.list_pids.return_value = [42, 43, 44, 45]
        api.read_stat.side_effect = lambda pid: ProcessStat(pid, "hoi4", "Z" if pid == 44 else "R", 1234)
        def image(pid):
            if pid == 43:
                raise PermissionError("denied")
            return ExecutableWitness("/fixture/hoi4" if pid == 42 else "/other/game", 8, 99)
        api.image.side_effect = image
        api.process_name.return_value = "hoi4"
        backend = LinuxPlatform(process_api=api)
        self.assertEqual(backend.find_processes("hoi4"), [42, 43])
        self.assertEqual(api.image.call_count, 3)
        for exception, expected in ((FileNotFoundError(), False), (PermissionError(), True)):
            api.read_stat.side_effect = exception
            self.assertEqual(backend.pid_alive(42), expected)
        api.read_stat.side_effect = None
        api.read_stat.return_value = ProcessStat(42, "hoi4", "Z", 1234)
        self.assertFalse(backend.pid_alive(42))
        self.assertFalse(backend.pid_alive(True))

    def test_desktop_requests_are_typed_errors_and_not_false_absence(self):
        backend = LinuxPlatform()
        window = backend.make_window("hoi4", "Hearts of Iron IV", True)
        capturer, inputs = backend.make_capturer(window), backend.make_input(window)
        self.assertFalse(backend.capabilities()["window"])
        self.assertFalse(backend.capabilities()["live_memory_layout"])
        for feature, action in (("window discovery", window.find), ("window discovery", window.require),
                                ("window capture", capturer.grab), ("mouse and keyboard input", inputs.key),
                                ("clipboard", backend.clipboard), ("game launch prerequisites", backend.preflight_game_launch),
                                ("screen recording", lambda: backend.recording_input_args(window, 30))):
            with self.subTest(feature=feature), self.assertRaises(CapabilityUnavailable) as caught:
                action()
            self.assertEqual(caught.exception.details["platform"], "linux")
            self.assertEqual(caught.exception.details["capability"], feature)
        capturer.close()

    def test_desktop_injection_does_not_touch_native_apis(self):
        desktop = Mock()
        backend = LinuxPlatform(desktop_backend=desktop)
        self.assertIs(backend.make_window("hoi4"), desktop.make_window.return_value)
        self.assertIs(backend.make_capturer("window"), desktop.make_capturer.return_value)
        self.assertIs(backend.make_input("window"), desktop.make_input.return_value)
        self.assertIs(backend.clipboard(), desktop.clipboard.return_value)

    def test_pidfd_termination_requires_exact_identity_and_handles_acquisition_races(self):
        backend = LinuxPlatform()
        with self.assertRaises(CapabilityUnavailable):
            backend.terminate(42)
        for changed in (IDENTITY, replace(IDENTITY, executable_inode=100)):
            with patch.object(backend, "capture_process_identity", side_effect=[IDENTITY, changed]), \
                    patch("hoi4_live.platform.linux.os.pidfd_open", return_value=17, create=True) as opened, \
                    patch("hoi4_live.platform.linux.signal.pidfd_send_signal", create=True) as send, \
                    patch("hoi4_live.platform.linux.os.close") as close:
                if changed == IDENTITY:
                    self.assertTrue(backend.terminate(42, expected_identity=IDENTITY))
                    send.assert_called_once()
                else:
                    with self.assertRaises(OSError):
                        backend.terminate(42, expected_identity=IDENTITY)
                    send.assert_not_called()
                opened.assert_called_once_with(42, 0)
                close.assert_called_once_with(17)

    def test_linux_session_stdio_init_list_and_capability_response_without_native_imports(self):
        from hoi4_live import config
        from hoi4_live.protocol.server import StdioServer
        from hoi4_live.session import Session
        from hoi4_live import tools  # noqa: F401
        with tempfile.TemporaryDirectory() as temporary:
            backend = LinuxPlatform(home=temporary, environ={}, process_api=Mock())
            backend.capture_process_identity = Mock(side_effect=OSError("fixture has no process"))
            previous = platform._BACKEND
            platform.set_backend(backend)
            try:
                cfg = config.load(path=os.path.join(temporary, "no-config.json"), overrides={
                    "game_dir": "", "user_dir": temporary, "artifacts_dir": os.path.join(temporary, "artifacts"),
                    "lock": {"path": os.path.join(temporary, "test.lock")}})
                session = Session(cfg, backend=backend)
                self.addCleanup(session.shutdown)
                messages = [
                    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
                    {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                    {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                     "params": {"name": "hoi4_status", "arguments": {"look": False}}},
                ]
                inp = io.StringIO("".join(json.dumps(message) + "\n" for message in messages))
                out = io.StringIO()
                StdioServer(session, inp, out).serve_forever()
                responses = {item["id"]: item for item in map(json.loads, out.getvalue().splitlines())}
                self.assertEqual(responses[1]["result"]["serverInfo"]["name"], "hoi4-live-tools")
                self.assertGreater(len(responses[2]["result"]["tools"]), 40)
                self.assertTrue(responses[3]["result"]["isError"])
                self.assertEqual(responses[3]["result"]["structuredContent"]["error"], "capability_unavailable")
                self.assertEqual(responses[3]["result"]["structuredContent"]["platform"], "linux")
                session.shutdown()
            finally:
                platform.set_backend(previous)


if __name__ == "__main__":
    unittest.main()
