"""Portable OCR contracts; native smoke uses generated text, never the game."""

import contextlib
import importlib.util
import os
import io
import queue
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch
from types import SimpleNamespace

import numpy as np
from PIL import Image, ImageDraw

from hoi4_live.errors import CapabilityUnavailable, ToolError
from hoi4_live.ocr import OcrHost, OcrLine, OcrResult
from hoi4_live.platform.ocr_macos import VisionOcrBackend, _pixel_box, _utf16_range
from hoi4_live.platform.ocr_windows import WindowsOcrBackend


class FakeBackend:
    max_dim = 2600
    running = False

    def __init__(self, words=None):
        self.words = words or [{"t": "Hello", "x": 4, "y": 6, "w": 10, "h": 8}]
        self.calls = []

    def recognize_file(self, path, language=None):
        with Image.open(path) as image:
            self.calls.append((image.size, language, str(path)))
        self.running = True
        return {"ok": True, "lines": [{"text": "Hello", "words": self.words}]}

    def close(self):
        self.running = False


class OcrFacadeTests(unittest.TestCase):
    def host(self, backend=None):
        host = OcrHost(backend=backend or FakeBackend())
        self.addCleanup(host.close)
        return host

    def test_legacy_api_is_same_types(self):
        from hoi4_live.ocr import winocr
        self.assertIs(winocr.OcrHost, OcrHost)
        self.assertIs(winocr.OcrLine, OcrLine)
        self.assertIs(winocr.OcrResult, OcrResult)

    def test_injected_backend_crop_padding_and_scale(self):
        backend = FakeBackend()
        host = self.host(backend)
        result = host.recognize(np.zeros((100, 200, 3), np.uint8),
                                region=[20, 30, 40, 20], scale=2, pad=2, language="de-DE")
        self.assertEqual(backend.calls[0][:2], ((88, 48), "de-DE"))
        self.assertEqual(result.lines[0].words, [{"text": "Hello", "box": [20, 31, 5, 4]}])
        self.assertEqual(result.lines[0].center, (22, 33))
        self.assertEqual(result.region, [20, 30, 40, 20])
        self.assertEqual(result.text, "Hello")
        self.assertFalse(os.path.exists(backend.calls[0][2]))
        self.assertTrue(host.running)
        host.close()
        self.assertFalse(host.running)
        self.assertFalse(os.path.exists(host.tmpdir))
        host.recognize(np.zeros((10, 10, 3), np.uint8))
        self.assertTrue(host.running)

    def test_crop_clamps_coordinate_origin(self):
        host = self.host()
        result = host.recognize(np.zeros((50, 50, 3), np.uint8), region=[-10, -20, 40, 40], scale=1)
        self.assertEqual(result.lines[0].box, [4, 6, 10, 8])

    def test_padding_included_in_engine_dimension_limit(self):
        backend = FakeBackend()
        backend.max_dim = 100
        host = self.host(backend)
        host.recognize(np.zeros((40, 80, 3), np.uint8), scale=2, pad=10)
        self.assertEqual(backend.calls[0][0], (100, 60))

    def test_rejects_invalid_image_and_scale(self):
        host = self.host()
        for scale in (0, -1, float("nan"), float("inf")):
            with self.subTest(scale=scale), self.assertRaises(ToolError):
                host.recognize(np.zeros((20, 20, 3), np.uint8), scale=scale)
        with self.assertRaises(ToolError):
            host.recognize(np.zeros((0, 20, 3), np.uint8))
        with self.assertRaises(ToolError):
            host.recognize(np.zeros((20, 20, 3), np.uint8), region=[50, 50, 10, 10])

    def test_cleanup_after_failure(self):
        backend = FakeBackend()
        host = self.host(backend)
        backend.recognize_file = lambda *args: (_ for _ in ()).throw(ToolError("test failure"))
        with self.assertRaisesRegex(ToolError, "test failure"):
            host.recognize(np.zeros((20, 20, 3), np.uint8))
        self.assertEqual(os.listdir(host.tmpdir), [])

    def test_unsupported_platform_fails_only_on_request(self):
        with patch("hoi4_live.ocr.host.sys.platform", "other-os"):
            host = OcrHost()
            self.addCleanup(host.close)
            self.assertFalse(host.running)
            with self.assertRaises(CapabilityUnavailable) as caught:
                host.recognize(np.zeros((20, 20, 3), np.uint8))
            self.assertEqual(caught.exception.details["capability"], "ocr")

    def test_lazy_native_imports(self):
        code = """
import sys
from hoi4_live.ocr.winocr import OcrHost, OcrLine, OcrResult
h = OcrHost()
assert 'Vision' not in sys.modules
assert 'Foundation' not in sys.modules
assert 'hoi4_live.platform.ocr_windows' not in sys.modules
assert not h.running
h.close()
"""
        result = subprocess.run([sys.executable, "-c", code], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "")


class VisionContractTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.path = str(Path(temp.name) / "input.png")
        Image.new("RGB", (200, 100), "white").save(self.path)

    def modules(self, fail=None, missing_box=False):
        self.ranges = []
        outer = self

        class Candidate:
            def string(self):
                return "😀 Hello"

            def boundingBoxForRange_error_(self, region, error):
                outer.ranges.append(region)
                if missing_box:
                    return None, "not located"
                rect = ((0.1, 0.2), (0.2, 0.1))
                return SimpleNamespace(boundingBox=lambda: rect), None

        class Request:
            @classmethod
            def alloc(cls):
                return cls()

            def init(self):
                return self

            def setRecognitionLevel_(self, value):
                outer.level = value

            def setRecognitionLanguages_(self, value):
                outer.languages = value

            def setUsesLanguageCorrection_(self, value):
                outer.correction = value

            def cancel(self):
                outer.cancelled = True

            def results(self):
                return [SimpleNamespace(topCandidates_=lambda n: [Candidate()])]

        class Handler:
            @classmethod
            def alloc(cls):
                return cls()

            def initWithURL_options_(self, url, options):
                outer.url = url
                return self

            def performRequests_error_(self, requests, error):
                return (False, fail) if fail else (True, None)

        return (SimpleNamespace(autorelease_pool=contextlib.nullcontext),
                SimpleNamespace(NSURL=SimpleNamespace(fileURLWithPath_=lambda p: p)),
                SimpleNamespace(VNRecognizeTextRequest=Request, VNImageRequestHandler=Handler,
                                VNRequestTextRecognitionLevelAccurate=1))

    def test_vision_word_boxes_and_utf16_ranges(self):
        backend = VisionOcrBackend(modules=self.modules())
        result = backend.recognize_file(self.path, "de-DE")
        self.assertEqual(self.ranges, [(0, 2), (3, 5)])
        self.assertEqual(self.languages, ["de-DE"])
        self.assertFalse(self.correction)
        self.assertEqual(result["lines"][0]["words"][1],
                         {"t": "Hello", "x": 20, "y": 70, "w": 40, "h": 10})
        self.assertTrue(backend.running)
        backend.close()
        self.assertFalse(backend.running)

    def test_native_error_does_not_become_empty_success(self):
        backend = VisionOcrBackend(modules=self.modules(fail="denied"))
        with self.assertRaisesRegex(ToolError, "Vision OCR failed: denied"):
            backend.recognize_file(self.path)
        self.assertFalse(backend.running)

    def test_missing_word_box_is_not_guessed(self):
        backend = VisionOcrBackend(modules=self.modules(missing_box=True))
        with self.assertRaisesRegex(ToolError, "could not locate"):
            backend.recognize_file(self.path)

    def test_permission_denial_is_capability_error(self):
        backend = VisionOcrBackend(modules=self.modules())
        with patch("hoi4_live.platform.ocr_macos.Image.open", side_effect=PermissionError("denied")):
            with self.assertRaises(CapabilityUnavailable) as caught:
                backend.recognize_file(self.path)
        self.assertEqual(caught.exception.details["capability"], "ocr")

    def test_dependency_failure_is_capability_error(self):
        with patch("hoi4_live.platform.ocr_macos.sys.platform", "darwin"), patch.dict(sys.modules, {"Vision": None}):
            with self.assertRaises(CapabilityUnavailable):
                VisionOcrBackend().recognize_file(self.path)

    def test_pixel_conversion_and_utf16(self):
        self.assertEqual(_pixel_box(((0.1, 0.2), (0.3, 0.4)), 100, 50),
                         {"x": 10, "y": 20, "w": 30, "h": 20})
        self.assertEqual(_utf16_range("😀 text", 2, 6), (3, 4))
        for box in (((0, 0), (-1, 1)), ((float("nan"), 0), (1, 1)), ((2, 2), (1, 1))):
            with self.assertRaises(ToolError):
                _pixel_box(box, 100, 50)

    def test_windows_backend_refuses_other_platform(self):
        with patch("hoi4_live.platform.ocr_windows.sys.platform", "darwin"):
            with self.assertRaises(CapabilityUnavailable):
                WindowsOcrBackend().recognize_file(self.path)

    def test_windows_old_reader_cannot_poison_replacement_queue(self):
        backend = WindowsOcrBackend()
        old_queue = queue.Queue()
        backend._reader(SimpleNamespace(stdout=io.BytesIO(b'{"id": 1}\n')), old_queue)
        self.assertTrue(backend.lines.empty())
        self.assertEqual(old_queue.get_nowait(), '{"id": 1}')
        self.assertIsNone(old_queue.get_nowait())

    def test_windows_request_timeout_bounds_noisy_host(self):
        backend = WindowsOcrBackend(timeout=1)
        backend.proc = SimpleNamespace(stdin=io.BytesIO(), poll=lambda: None)
        # The first read consumes unrelated output; the total deadline must
        # still expire before another read, even if output never stops.
        with patch("hoi4_live.platform.ocr_windows.time.monotonic", side_effect=[0, 0.5, 2]), \
                patch.object(backend, "_next_line", return_value='{"id": -1}') as read, \
                patch.object(backend, "close") as close:
            with self.assertRaisesRegex(ToolError, "did not answer in time"):
                backend.recognize_file(self.path)
        self.assertEqual(read.call_count, 1)
        close.assert_called_once()

    def test_windows_timeout_releases_lock_without_deadlock(self):
        backend = WindowsOcrBackend(timeout=0.001)
        failures = []

        def run():
            with backend.lock:
                try:
                    backend._next_line(0.001)
                except ToolError:
                    failures.append(True)

        thread = threading.Thread(target=run, daemon=True)
        thread.start()
        thread.join(1)
        self.assertFalse(thread.is_alive())
        self.assertEqual(failures, [True])


class NativeOcrSmokeTests(unittest.TestCase):
    def check_native(self):
        host = OcrHost()
        self.addCleanup(host.close)
        image = Image.new("RGB", (300, 70), "white")
        ImageDraw.Draw(image).text((15, 20), "HOI4 LIVE TOOLS 1936", fill="black")
        result = host.recognize(np.asarray(image), scale=3)
        self.assertIn("1936", result.text)
        for line in result.lines:
            x, y, w, h = line.box
            self.assertGreaterEqual(x, 0)
            self.assertGreaterEqual(y, 0)
            self.assertLessEqual(x + w, image.width)
            self.assertLessEqual(y + h, image.height)

    @unittest.skipUnless(sys.platform == "darwin" and importlib.util.find_spec("Vision"),
                         "requires macOS and PyObjC Vision")
    def test_macos_vision_generated_text(self):
        self.check_native()

    @unittest.skipUnless(sys.platform == "win32" and shutil.which("powershell.exe"),
                         "requires Windows and PowerShell OCR")
    def test_windows_generated_text(self):
        self.check_native()
