"""Portable OCR facade; native backends start only on the first request."""

import math
import os
import shutil
import sys
import tempfile
import threading
import time

import numpy as np

from ..desktop import images
from ..errors import CapabilityUnavailable, ToolError
from .models import OcrLine, OcrResult


def make_backend(language="en-US", idle_shutdown=600, timeout=30):
    if sys.platform == "win32":
        from ..platform.ocr_windows import WindowsOcrBackend
        return WindowsOcrBackend(language, idle_shutdown, timeout)
    if sys.platform == "darwin":
        from ..platform.ocr_macos import VisionOcrBackend
        return VisionOcrBackend(language, idle_shutdown, timeout)
    raise CapabilityUnavailable("ocr", sys.platform, "No OCR backend is available for this operating system",
                                "Use Windows or macOS, or inject an OCR backend")


class OcrHost:
    def __init__(self, language="en-US", idle_shutdown=600, timeout=30, backend=None):
        self.language = language
        self.idle_shutdown = idle_shutdown
        self.timeout = timeout
        self._backend = backend
        self.lock = threading.RLock()
        self.tmpdir = tempfile.mkdtemp(prefix="hoi4live_ocr_")
        self.file_seq = 0
        self.pending_delete = []

    @property
    def backend(self):
        if self._backend is None:
            self._backend = make_backend(self.language, self.idle_shutdown, self.timeout)
        return self._backend

    @property
    def max_dim(self):
        return self.backend.max_dim

    @property
    def running(self):
        return bool(self._backend is not None and self._backend.running)

    def close(self):
        with self.lock:
            if self._backend is not None:
                self._backend.close()
            shutil.rmtree(self.tmpdir, ignore_errors=True)
            self.pending_delete = []

    def recognize_file(self, path, language=None):
        with self.lock:
            return self.backend.recognize_file(path, language or self.language)

    def _cleanup(self, path):
        # WinRT may retain the current file briefly; retry deletion on the next request.
        self.pending_delete.append(path)
        keep = []
        for pending in self.pending_delete:
            try:
                os.remove(pending)
            except OSError:
                keep.append(pending)
        self.pending_delete = keep

    def recognize(self, img, region=None, scale=None, language=None, invert=False, binarize=None, pad=0,
                  pad_color=(255, 255, 255)):
        with self.lock:
            return self._recognize(img, region, scale, language, invert, binarize, pad, pad_color)

    def _recognize(self, img, region=None, scale=None, language=None, invert=False, binarize=None, pad=0,
                  pad_color=(255, 255, 255)):
        """OCR a numpy RGB image (optionally a client region [x, y, w, h]). Boxes come back in the image's
        coordinates. scale defaults to 2x for small crops (game fonts are small) within the engine's size limit.
        pad adds a plain border first: Windows OCR misses text that touches the image edge."""
        img = np.asarray(img)
        if img.ndim not in (2, 3) or not all(img.shape[:2]):
            raise ToolError("OCR requires a nonempty image")
        try:
            sub = images.crop(img, region) if region else img
        except (TypeError, ValueError) as exc:
            raise ToolError("Invalid OCR region: %s" % exc) from exc
        ox, oy = (max(0, int(region[0])), max(0, int(region[1]))) if region else (0, 0)
        h, w = sub.shape[:2]
        if isinstance(pad, bool) or not isinstance(pad, int) or pad < 0:
            raise ToolError("OCR pad must be a nonnegative integer")
        if scale is None:
            scale = 2.0 if max(w, h) <= 1300 else 1.0
        if not math.isfinite(float(scale)) or float(scale) <= 0:
            raise ToolError("OCR scale must be positive and finite")
        scale = min(float(scale), self.max_dim / float(max(w, h) + 2 * pad))
        pim = images.to_pil(sub)
        if invert or binarize is not None:
            a = np.asarray(pim.convert("L"), np.float32)
            if binarize is not None:
                a = np.where(a > binarize, 255, 0).astype(np.float32)
            if invert:
                a = 255 - a
            from PIL import Image
            pim = Image.fromarray(a.astype(np.uint8)).convert("RGB")
        if pad:
            from PIL import Image
            canvas = Image.new("RGB", (pim.width + 2 * pad, pim.height + 2 * pad), pad_color)
            canvas.paste(pim.convert("RGB"), (pad, pad))
            pim = canvas
            ox, oy = ox - pad, oy - pad
        original_width, original_height = pim.size
        if scale != 1.0:
            pim = images.upscale(pim, scale)
        sx, sy = pim.width / original_width, pim.height / original_height
        os.makedirs(self.tmpdir, exist_ok=True)
        self.file_seq += 1
        path = os.path.join(self.tmpdir, f"ocr_{self.file_seq % 64:02d}_{int(time.time() * 1000) % 100000}.png")
        pim.save(path)
        try:
            resp = self.recognize_file(path, language)
        finally:
            self._cleanup(path)
        lines = []
        for l in resp.get("lines") or []:
            ws = l.get("words") or []
            if isinstance(ws, dict):
                ws = [ws]
            words = []
            for wd in ws:
                bx = [int(round(wd["x"] / sx)) + ox, int(round(wd["y"] / sy)) + oy,
                      max(1, int(round(wd["w"] / sx))), max(1, int(round(wd["h"] / sy)))]
                words.append({"text": wd["t"], "box": bx})
            if not words:
                continue
            x0 = min(b["box"][0] for b in words)
            y0 = min(b["box"][1] for b in words)
            x1 = max(b["box"][0] + b["box"][2] for b in words)
            y1 = max(b["box"][1] + b["box"][3] for b in words)
            lines.append(OcrLine(l.get("text") or " ".join(b["text"] for b in words), [x0, y0, x1 - x0, y1 - y0], words))
        lines.sort(key=lambda l: (l.box[1], l.box[0]))
        return OcrResult(lines, list(region) if region else [0, 0, w, h], round(scale, 3))
