"""macOS Vision OCR. PyObjC is imported only when recognition is requested.

Vision recognizes supplied image files; screen capture permissions belong to the
capture backend, not OCR. Boxes use Vision's normalized lower-left origin until
conversion to the public top-left pixel contract below.
"""

import math
import os
import re
import sys
import threading

from PIL import Image

from ..errors import CapabilityUnavailable, ToolError


def _pixel_box(rect, width, height):
    """Convert CGRect (also represented by nested tuples in tests) to pixels."""
    try:
        x, y = rect.origin.x, rect.origin.y
        w, h = rect.size.width, rect.size.height
    except AttributeError:
        (x, y), (w, h) = rect
    if not all(math.isfinite(v) for v in (x, y, w, h)) or w <= 0 or h <= 0:
        raise ToolError("Vision returned an invalid OCR bounding box")
    x0 = max(0, min(width, int(round(x * width))))
    y0 = max(0, min(height, int(round((1 - y - h) * height))))
    x1 = max(0, min(width, int(round((x + w) * width))))
    y1 = max(0, min(height, int(round((1 - y) * height))))
    if x1 <= x0 or y1 <= y0:
        raise ToolError("Vision returned an OCR bounding box outside the image")
    return {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0}


def _utf16_range(text, start, end):
    """NSRange counts UTF-16 code units, whereas Python indexes code points."""
    return (len(text[:start].encode("utf-16-le")) // 2,
            len(text[start:end].encode("utf-16-le")) // 2)


class VisionOcrBackend:
    max_dim = 4096

    def __init__(self, language="en-US", idle_shutdown=600, timeout=30, modules=None):
        self.language = language
        self.idle_shutdown = idle_shutdown
        self.timeout = timeout
        self._modules = modules
        self._active = False
        self.lock = threading.RLock()

    @property
    def running(self):
        return self._active

    def close(self):
        with self.lock:
            self._active = False

    def _load(self):
        if self._modules is not None:
            return self._modules
        if sys.platform != "darwin":
            raise CapabilityUnavailable("ocr", sys.platform, "Vision OCR requires macOS")
        try:
            import objc
            import Foundation
            import Vision
        except ImportError as exc:
            raise CapabilityUnavailable("ocr", "macos", "The PyObjC Vision framework is unavailable",
                                        "Install pyobjc-framework-Vision and pyobjc-framework-Cocoa") from exc
        if not hasattr(Vision, "VNRecognizeTextRequest"):
            raise CapabilityUnavailable("ocr", "macos", "Vision text recognition is unavailable",
                                        "Use macOS 10.15 or newer")
        self._modules = (objc, Foundation, Vision)
        return self._modules

    def recognize_file(self, path, language=None):
        with self.lock:
            return self._recognize_file(path, language)

    def _recognize_file(self, path, language):
        objc, foundation, vision = self._load()
        try:
            with Image.open(path) as image:
                width, height = image.size
        except PermissionError as exc:
            raise CapabilityUnavailable("ocr", "macos", "Permission denied reading the OCR image",
                                        "Allow this process to read the image file") from exc
        except OSError as exc:
            raise ToolError("Cannot read OCR image: %s" % exc) from exc
        with objc.autorelease_pool():
            request = vision.VNRecognizeTextRequest.alloc().init()
            request.setRecognitionLevel_(vision.VNRequestTextRecognitionLevelAccurate)
            request.setRecognitionLanguages_([language or self.language])
            # Game labels, identifiers and numeric values must not be autocorrected.
            request.setUsesLanguageCorrection_(False)
            url = foundation.NSURL.fileURLWithPath_(os.path.abspath(path))
            handler = vision.VNImageRequestHandler.alloc().initWithURL_options_(url, {})
            expired = threading.Event()

            def cancel():
                expired.set()
                request.cancel()

            timer = threading.Timer(self.timeout, cancel)
            timer.daemon = True
            timer.start()
            try:
                ok, error = handler.performRequests_error_([request], None)
            except Exception as exc:
                raise ToolError("Vision OCR failed: %s" % exc) from exc
            finally:
                timer.cancel()
                # Do not let cancellation race with autorelease-pool cleanup.
                timer.join()
            if expired.is_set():
                raise ToolError("Vision OCR did not answer in time")
            if not ok or error is not None:
                raise ToolError("Vision OCR failed: %s" % error)
            lines = []
            for observation in request.results() or []:
                candidates = observation.topCandidates_(1)
                if not candidates:
                    continue
                candidate = candidates[0]
                text = str(candidate.string())
                words = []
                for match in re.finditer(r"\S+", text):
                    box, error = candidate.boundingBoxForRange_error_(
                        _utf16_range(text, match.start(), match.end()), None)
                    if error is not None or box is None:
                        # A guessed line-wide box could click the wrong control.
                        raise ToolError("Vision could not locate the recognized word %r: %s" %
                                        (match.group(), error))
                    word = _pixel_box(box.boundingBox(), width, height)
                    word["t"] = match.group()
                    words.append(word)
                if words:
                    lines.append({"text": text, "words": words})
            self._active = True
            return {"ok": True, "angle": 0, "lines": lines}
