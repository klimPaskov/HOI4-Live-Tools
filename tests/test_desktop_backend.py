"""Offline desktop tests. Fake services never launch or control a game."""
import importlib
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import numpy as np

from hoi4_live.errors import CapabilityUnavailable, GameNotRunning, ToolError
from hoi4_live.platform.identity import ProcessIdentity
from hoi4_live.platform.macos_desktop import Capturer, Clipboard, GameWindow, Geometry, Input, Native, KEYS, MODIFIER_FLAGS


IDENTITY = ProcessIdentity(41, 12345678, "/fixture/hoi4")


class Processes:
    def __init__(self):
        self.identity = IDENTITY

    def capture_process_identity(self, pid):
        return self.identity

    def find_processes(self, name):
        return [41, 42]


class FakeNative:
    def __init__(self):
        self.info = {"id": 500, "pid": 41, "title": "Hearts of Iron IV", "bounds": (-1000, 20, 400, 300)}
        self.g = Geometry(-1000, 20, 400, 300, 2)
        self.events = []
        self.visible = True
        self.is_focused = True
        self.position = (-900, 70)
        self.denied = False
        self.after_capture = lambda: None
        self.frame_shape = (600, 800, 3)
        self.fail_key = None

    def windows(self):
        return [dict(self.info)] if self.visible else []

    def geometry(self, info):
        return self.g

    def frame_geometry(self, info):
        return self.g

    def permission(self, feature):
        if self.denied:
            raise CapabilityUnavailable(feature, "macos", "denied", "enable in System Settings")

    def focused(self, info):
        return self.is_focused

    def focus(self, info):
        self.events.append(("focus",))
        self.is_focused = True

    def cursor(self):
        return self.position

    def post_mouse(self, action, position, button="left", flags=0, clicks=1):
        self.events.append(("mouse", action, position, button, flags, clicks))
        self.position = position

    def post_key(self, code, down, flags):
        if code == self.fail_key and down:
            raise ToolError("test event failure")
        self.events.append(("key", code, down, flags))

    def post_text(self, text, down):
        self.events.append(("text", text, down))

    def post_scroll(self, notches, flags):
        self.events.append(("scroll", notches, flags))

    def capture(self, info, geometry, timeout, method):
        self.permission("capture")
        frame = np.full(self.frame_shape, 80, dtype=np.uint8)
        self.after_capture()
        return frame, "fake"


class DesktopBackendTests(unittest.TestCase):
    def setUp(self):
        self.native = FakeNative()
        self.processes = Processes()
        self.window = GameWindow(native=self.native, process_backend=self.processes,
                                 expected_identity=IDENTITY, window_id=500)
        self.input = Input(self.window, native=self.native, key_delay=0, type_delay=0, click_hold=0, settle=0)

    def test_facades_import_without_loading_windows_native_modules(self):
        script = """
import ctypes, importlib, sys
for name in ('window', 'capture', 'inputs', 'clipboard', 'win32'):
    importlib.import_module('hoi4_live.desktop.' + name)
assert 'hoi4_live.platform.win32' not in sys.modules
assert 'hoi4_live.platform.windows_capture' not in sys.modules
"""
        result = subprocess.run([sys.executable, "-c", script], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_factory_injection_does_not_touch_native_frameworks(self):
        from hoi4_live.desktop.window import GameWindow as WindowFacade
        from hoi4_live.desktop.inputs import Input as InputFacade
        from hoi4_live.desktop.capture import Capturer as CaptureFacade
        class Factory:
            game_process_name = "fixture"
            def make_window(self, **kwargs):
                return kwargs
            def make_input(self, window, **kwargs):
                return (window, kwargs)
            def make_capturer(self, window, **kwargs):
                return (window, kwargs)
        f = Factory()
        self.assertEqual(WindowFacade(backend=f)["process_name"], "fixture")
        self.assertEqual(InputFacade("test", backend=f)[0], "test")
        self.assertEqual(CaptureFacade("test", backend=f)[0], "test")

    def test_retina_negative_monitor_origin_and_roundtrip(self):
        self.assertEqual(self.window.size(), (800, 600))
        self.assertEqual(self.window.to_screen(200, 100), (-900, 70))
        self.assertEqual(self.window.from_screen(-900, 70), (200, 100))
        self.assertEqual(self.window.to_screen(799, 599), (-600.5, 319.5))
        with self.assertRaises(ToolError):
            self.window.to_screen(800, 0)
        with self.assertRaises(ToolError):
            self.window.to_screen(float("nan"), 0)

    def test_pid_reuse_and_changed_executable_refused(self):
        for identity in (ProcessIdentity(41, 9999, IDENTITY.executable_path),
                         ProcessIdentity(41, IDENTITY.creation_time, "/fixture/other")):
            self.processes.identity = identity
            with self.assertRaisesRegex(ToolError, "identity changed"):
                self.window.require()
        self.assertEqual(self.native.events, [])

    def test_other_pid_and_window_id_never_selected(self):
        for key, value in (("pid", 42), ("id", 501)):
            old = self.native.info[key]
            self.native.info[key] = value
            with self.assertRaises(GameNotRunning):
                self.window.require()
            self.native.info[key] = old

    def test_splash_replacement_follows_only_same_verified_identity(self):
        window = GameWindow(native=self.native, process_backend=self.processes, expected_identity=IDENTITY)
        self.assertEqual(window.find(), 500)
        self.native.info["id"] = 501
        self.assertEqual(window.find(), 501)
        self.assertEqual(window.pid, IDENTITY.pid)
        self.native.info.update(id=502, pid=42)
        self.assertIsNone(window.find())

    def test_explicit_window_pin_never_follows_splash_replacement(self):
        self.assertEqual(self.window.find(), 500)
        self.native.info["id"] = 501
        self.assertIsNone(self.window.find())
        self.assertEqual(self.window._requested_window_id, 500)

    def test_splash_replacement_refuses_recycled_process_identity(self):
        window = GameWindow(native=self.native, process_backend=self.processes, expected_identity=IDENTITY)
        window.find()
        self.native.info["id"] = 501
        self.processes.identity = ProcessIdentity(41, 999, IDENTITY.executable_path)
        with self.assertRaisesRegex(ToolError, "identity changed"):
            window.find()

    def test_disappeared_or_minimized_window_fails(self):
        self.window.require()
        self.native.visible = False
        with self.assertRaises(GameNotRunning):
            self.window.require()

    def test_input_permission_denial_produces_no_event(self):
        self.native.denied = True
        with self.assertRaises(CapabilityUnavailable) as cm:
            self.input.key("ctrl+a")
        self.assertEqual(cm.exception.details["capability"], "input")
        self.assertEqual(self.native.events, [])

    def test_capture_is_observational_and_retina_sized(self):
        image = Capturer(self.window).grab()
        self.assertEqual(image.shape, (600, 800, 3))
        self.assertEqual(self.native.events, [])

    def test_capture_permission_denied_no_focus(self):
        self.native.denied = True
        with self.assertRaises(CapabilityUnavailable):
            Capturer(self.window).grab()
        self.assertEqual(self.native.events, [])

    def test_diagnostic_frame_is_explicitly_not_client_coordinates(self):
        self.native.geometry = lambda info: self.fail("diagnostic must not claim client bounds")
        result = Capturer(self.window).capture_frame()
        self.assertEqual(result["coordinate_space"], "window-frame-pixels")
        self.assertFalse(result["client_bounds_verified"])
        self.assertEqual(result["window_id"], 500)
        self.assertEqual(result["image"].shape, (600, 800, 3))
        self.assertEqual(self.native.events, [])

    def test_diagnostic_frame_refuses_implicit_process_attachment(self):
        window = GameWindow(native=self.native, process_backend=self.processes)
        with self.assertRaisesRegex(CapabilityUnavailable, "explicitly owned"):
            Capturer(window).capture_frame()

    def test_diagnostic_frame_rejects_window_changed_during_capture(self):
        self.native.after_capture = lambda: self.native.info.update(bounds=(-900, 20, 400, 300))
        with self.assertRaisesRegex(ToolError, "changed during diagnostic"):
            Capturer(self.window).capture_frame()

    def test_capture_rejects_torn_geometry(self):
        self.native.after_capture = lambda: setattr(self.native, "g", Geometry(-500, 20, 400, 300, 2))
        with self.assertRaisesRegex(ToolError, "changed during capture"):
            Capturer(self.window).grab()

    def test_capture_rejects_wrong_pixel_scale(self):
        self.native.frame_shape = (300, 400, 3)
        with self.assertRaisesRegex(CapabilityUnavailable, "pixel dimensions"):
            Capturer(self.window).grab()

    def test_recording_can_keep_blank_frames_with_identity_checks(self):
        def blank_capture(*args):
            return np.zeros((600, 800, 3), dtype=np.uint8), "fake"
        self.native.capture = blank_capture
        with self.assertRaises(CapabilityUnavailable):
            Capturer(self.window).grab()
        self.assertEqual(Capturer(self.window, allow_blank=True).grab().shape, (600, 800, 3))
        self.processes.identity = ProcessIdentity(41, 999, IDENTITY.executable_path)
        with self.assertRaises(ToolError):
            Capturer(self.window, allow_blank=True).grab()

    def test_native_geometry_rejects_unknown_decorated_client(self):
        native = Native()
        native.displays = lambda: [(1, Geometry(-1000, 0, 1000, 800, 2))]
        native.permission = lambda feature: None
        native._ax_window = lambda info: None
        with self.assertRaisesRegex(CapabilityUnavailable, "verified client-content"):
            native.geometry(self.native.info)

    def test_native_fullscreen_geometry_does_not_need_focus(self):
        native = Native()
        native.displays = lambda: [(1, Geometry(-1000, 20, 400, 300, 2))]
        native.permission = lambda feature: self.fail("fullscreen observation must not request input permission")
        self.assertEqual(native.geometry(self.native.info), self.native.g)

    def test_native_geometry_rejects_cross_display_window(self):
        native = Native()
        native.displays = lambda: [(1, Geometry(-1000, 0, 300, 800, 2)), (2, Geometry(-700, 0, 700, 800, 1))]
        with self.assertRaisesRegex(CapabilityUnavailable, "spans displays"):
            native.geometry(self.native.info)

    def test_capture_rejects_foreign_backend_method(self):
        with self.assertRaises(CapabilityUnavailable):
            Capturer(self.window, method="printwindow").grab()

    @patch("hoi4_live.platform.macos_desktop.time.sleep")
    def test_chord_modifier_flags_and_release(self, sleep):
        self.input.key("ctrl+shift+a")
        events = self.native.events
        self.assertEqual(events[0], ("key", KEYS["ctrl"], True, MODIFIER_FLAGS["ctrl"]))
        self.assertEqual(events[2][3], MODIFIER_FLAGS["ctrl"] | MODIFIER_FLAGS["shift"])
        self.assertEqual(events[-1], ("key", KEYS["ctrl"], False, 0))
        self.assertFalse(self.input._held_keys)

    @patch("hoi4_live.platform.macos_desktop.time.sleep")
    def test_failed_chord_releases_pressed_modifier(self, sleep):
        self.native.fail_key = KEYS["a"]
        with self.assertRaises(ToolError):
            self.input.key("ctrl+a")
        self.assertEqual(self.native.events[-1], ("key", KEYS["ctrl"], False, 0))
        self.assertFalse(self.input._held_keys)

    def test_unknown_key_rejected_before_any_input(self):
        with self.assertRaises(ToolError):
            self.input.key("ctrl+not-a-key")
        self.assertEqual(self.native.events, [])

    def test_mouse_down_outside_owned_client_rejected(self):
        self.native.position = (0, 0)
        with self.assertRaises(ToolError):
            self.input.mouse_down()
        self.assertEqual(self.native.events, [])

    def test_drag_releases_button_after_focus_loss(self):
        original = self.native.post_mouse
        def lose_focus(action, *args):
            original(action, *args)
            if action == "down":
                self.native.is_focused = False
        self.native.post_mouse = lose_focus
        with self.assertRaises(CapabilityUnavailable):
            self.input.drag(100, 100, 200, 200, seconds=0)
        self.assertEqual(self.native.events[-1][1], "up")
        self.assertFalse(self.input._held_buttons)

    def test_unicode_text_has_balanced_key_events(self):
        self.input.type_text("é界")
        self.assertEqual(self.native.events, [("text", "é", True), ("text", "é", False),
                                             ("text", "界", True), ("text", "界", False)])

    def test_windows_compat_access_is_capability_error_off_windows(self):
        if sys.platform == "win32":
            self.skipTest("non-Windows capability check")
        from hoi4_live.desktop import win32
        with self.assertRaises(CapabilityUnavailable):
            _ = win32.user32


class NativeFocusSafetyTests(unittest.TestCase):
    def setUp(self):
        self.info = {"id": 500, "pid": 41, "title": "Game", "bounds": (0, 0, 100, 100)}
        self.front_pid = 41
        self.window_list = [self.info]
        self.ax_rect = None
        self.activations = []
        self.session = {"on_console": True, "login_done": True}
        self.q = SimpleNamespace(CGSessionCopyCurrentDictionary=lambda: self.session,
                                 kCGSessionOnConsoleKey="on_console", kCGSessionLoginDoneKey="login_done",
                                 CGPreflightScreenCaptureAccess=lambda: True)
        app = SimpleNamespace(processIdentifier=lambda: self.front_pid,
                              activateWithOptions_=lambda flags: self.activations.append(flags) or True)
        self.appkit = SimpleNamespace(
            NSWorkspace=SimpleNamespace(sharedWorkspace=lambda: SimpleNamespace(frontmostApplication=lambda: app)),
            NSRunningApplication=SimpleNamespace(runningApplicationWithProcessIdentifier_=lambda pid: app),
            NSApplicationActivateIgnoringOtherApps=1)
        self.ax = SimpleNamespace(AXUIElementCreateApplication=lambda pid: object(),
                                  AXIsProcessTrusted=lambda: True)
        self.native = Native()
        self.native._load = lambda: (self.q, self.appkit, self.ax)
        self.native.windows = lambda: self.window_list
        self.native._attr = lambda element, name: object()
        self.native._ax_rect = lambda element: self.ax_rect
        self.native._ax_window = lambda info: None

    def test_sdl_focus_fallback_requires_exact_frontmost_window_and_pid(self):
        self.assertTrue(self.native.focused(self.info))
        self.front_pid = 42
        self.assertFalse(self.native.focused(self.info))
        self.front_pid = 41
        self.window_list = [dict(self.info, id=501), self.info]
        self.assertFalse(self.native.focused(self.info))

    def test_known_mismatching_ax_window_is_not_overridden(self):
        self.ax_rect = (20, 20, 100, 100)
        self.assertFalse(self.native.focused(self.info))

    def test_sdl_activation_does_not_require_an_invented_ax_window(self):
        self.native.focus(self.info)
        self.assertEqual(self.activations, [1])

    def test_locked_or_nonconsole_session_refuses_before_activation(self):
        for session in ({"on_console": True, "login_done": True, "CGSSessionScreenIsLocked": True},
                        {"on_console": False, "login_done": True}, None):
            self.session = session
            with self.assertRaisesRegex(CapabilityUnavailable, "locked or not"):
                self.native.focus(self.info)
            with self.assertRaises(CapabilityUnavailable):
                self.native.permission("capture")
        self.assertEqual(self.activations, [])

    def test_no_active_display_is_an_explicit_wake_blocker(self):
        self.q.CGGetActiveDisplayList = lambda *args: (0, (), 0)
        with self.assertRaisesRegex(CapabilityUnavailable, "no active displays"):
            self.native.displays()


class ClipboardTests(unittest.TestCase):
    class Item:
        def __init__(self, values=None):
            self.values = values or {}
        def init(self):
            return self
        def types(self):
            return list(self.values)
        def dataForType_(self, kind):
            return self.values[kind]
        def setData_forType_(self, data, kind):
            self.values[kind] = bytes(data)
            return True

    class Board:
        def __init__(self, items):
            self.items, self.clears = items, 0
        def pasteboardItems(self):
            return self.items
        def clearContents(self):
            self.clears += 1
            self.items = []
        def writeObjects_(self, items):
            self.items = items
            return True

    def clipboard(self, values):
        board = self.Board([self.Item(values)])
        appkit = SimpleNamespace(NSPasteboard=SimpleNamespace(generalPasteboard=lambda: board),
                                 NSPasteboardItem=SimpleNamespace(alloc=lambda: self.Item()))
        native = SimpleNamespace(_load=lambda: (None, appkit, None))
        return Clipboard(native=native), board

    def test_snapshot_restores_binary_and_text_representations(self):
        values = {"public.utf8-plain-text": "猫".encode(), "public.png": b"\x89PNG\x00\xff"}
        clipboard, board = self.clipboard(values)
        saved = clipboard.snapshot()
        self.assertEqual(board.clears, 0)
        board.items = []
        clipboard.restore(saved)
        self.assertEqual(board.items[0].values, values)

    def test_unreadable_promised_data_refuses_before_mutating_clipboard(self):
        clipboard, board = self.clipboard({"promised.file": None})
        with self.assertRaises(CapabilityUnavailable):
            clipboard.snapshot()
        self.assertEqual(board.clears, 0)

    def test_empty_snapshot_restores_empty_clipboard(self):
        clipboard, board = self.clipboard({"public.text": b"sample"})
        clipboard.restore([])
        self.assertEqual(board.items, [])


@unittest.skipUnless(sys.platform == "darwin", "macOS public-framework geometry conversion")
class MacNativeConversionTests(unittest.TestCase):
    def test_rgba_to_rgb_preserves_channel_and_row_order(self):
        try:
            import Quartz as q
            import ApplicationServices
        except ImportError:
            self.skipTest("optional PyObjC dependencies unavailable")
        rgba = np.array([[[255, 0, 0, 255], [0, 255, 0, 255]],
                         [[0, 0, 255, 255], [255, 255, 0, 255]]], dtype=np.uint8)
        data = rgba.tobytes()
        provider = q.CGDataProviderCreateWithData(None, data, len(data), None)
        image = q.CGImageCreate(2, 2, 8, 32, 8, q.CGColorSpaceCreateDeviceRGB(),
                               q.kCGImageAlphaPremultipliedLast | q.kCGBitmapByteOrder32Big,
                               provider, None, False, q.kCGRenderingIntentDefault)
        np.testing.assert_array_equal(Native()._rgb(image), rgba[:, :, :3])

    def test_ax_value_bounds_roundtrip_without_desktop_access(self):
        try:
            import ApplicationServices as ax
            import Quartz as q
        except ImportError:
            self.skipTest("optional PyObjC dependencies unavailable")
        native = Native()
        values = {"AXPosition": ax.AXValueCreate(ax.kAXValueCGPointType, q.CGPointMake(-33.5, 20)),
                  "AXSize": ax.AXValueCreate(ax.kAXValueCGSizeType, q.CGSizeMake(200, 150))}
        native._attr = lambda element, name: values[name]
        self.assertEqual(native._ax_rect(object()), (-33.5, 20, 200, 150))


if __name__ == "__main__":
    unittest.main()
