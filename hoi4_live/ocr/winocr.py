"""Windows.Media.Ocr through one persistent PowerShell process (ocr_host.ps1).

The host starts on first use (about 1-2 s), answers each request in roughly 100-400 ms, and shuts down after
`idle_shutdown_seconds` without requests so it does not sit in memory on a shared machine. Results are lines with
word boxes, mapped back to the caller's coordinate frame (client pixels when a region is given).
"""

import json
import logging
import os
import queue
import re
import subprocess
import tempfile
import threading
import time

import numpy as np

from ..desktop import images
from ..errors import ToolError

log = logging.getLogger("hoi4_live.ocr")
HOST_SCRIPT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ocr_host.ps1")


class OcrLine:
    __slots__ = ("text", "box", "words")

    def __init__(self, text, box, words):
        self.text = text
        self.box = box            # [x, y, w, h]
        self.words = words        # [{"text", "box"}]

    @property
    def center(self):
        x, y, w, h = self.box
        return x + w // 2, y + h // 2

    def to_dict(self, words=False):
        d = {"text": self.text, "box": self.box}
        if words:
            d["words"] = self.words
        return d


class OcrResult:
    def __init__(self, lines, region, scale):
        self.lines = lines
        self.region = region
        self.scale = scale

    @property
    def text(self):
        return "\n".join(l.text for l in self.lines)

    def find(self, pattern, flags=re.I):
        """Lines matching a regex (or a plain substring when pattern is not a valid regex)."""
        try:
            rx = re.compile(pattern, flags)
        except re.error:
            rx = re.compile(re.escape(pattern), flags)
        return [l for l in self.lines if rx.search(l.text)]

    def find_words(self, pattern, flags=re.I):
        rx = re.compile(pattern, flags)
        out = []
        for l in self.lines:
            for w in l.words:
                if rx.search(w["text"]):
                    out.append(w)
        return out

    def to_dict(self, words=False):
        return {"region": self.region, "scale": self.scale, "lines": [l.to_dict(words) for l in self.lines]}


class OcrHost:
    def __init__(self, language="en-US", idle_shutdown=600, timeout=30):
        self.language = language
        self.idle_shutdown = idle_shutdown
        self.timeout = timeout
        self.proc = None
        self.max_dim = 2600
        self.lines = queue.Queue()
        self.lock = threading.Lock()
        self.last_used = 0.0
        self.next_id = 1
        self.tmpdir = tempfile.mkdtemp(prefix="hoi4live_ocr_")
        self.file_seq = 0
        self.pending_delete = []
        self._idle_thread = None

    # ------------------------------------------------------------------ process
    def _start(self):
        if not os.path.exists(HOST_SCRIPT):
            raise ToolError(f"OCR host script missing: {HOST_SCRIPT}")
        self.lines = queue.Queue()
        self.proc = subprocess.Popen(
            ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
             "-File", HOST_SCRIPT],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            creationflags=0x08000000)                               # CREATE_NO_WINDOW
        threading.Thread(target=self._reader, args=(self.proc,), daemon=True).start()
        threading.Thread(target=self._drain, args=(self.proc,), daemon=True).start()
        first = self._next_line(timeout=60)
        try:
            ready = json.loads(first)
            self.max_dim = int(ready.get("max_dim") or 2600)
        except (TypeError, ValueError):
            raise ToolError(f"OCR host failed to start: {first!r}")
        log.info("OCR host started (pid %s, max dimension %s)", self.proc.pid, self.max_dim)
        if self._idle_thread is None:
            self._idle_thread = threading.Thread(target=self._idle_watch, daemon=True)
            self._idle_thread.start()

    def _reader(self, proc):
        for raw in proc.stdout:
            self.lines.put(raw.decode("utf-8", "replace").strip())
        self.lines.put(None)

    def _drain(self, proc):
        for raw in proc.stderr:
            log.debug("ocr host stderr: %s", raw.decode("utf-8", "replace").rstrip())

    def _next_line(self, timeout):
        try:
            line = self.lines.get(timeout=timeout)
        except queue.Empty:
            self.close()
            raise ToolError("OCR host did not answer in time")
        if line is None:
            self.proc = None
            raise ToolError("OCR host exited unexpectedly")
        return line

    def _idle_watch(self):
        while True:
            time.sleep(15)
            with self.lock:
                if self.proc and time.time() - self.last_used > self.idle_shutdown:
                    log.info("OCR host idle, shutting down")
                    self._stop()

    def _stop(self):
        p, self.proc = self.proc, None
        if p and p.poll() is None:
            try:
                p.stdin.write(b"quit\n")
                p.stdin.flush()
                p.wait(timeout=3)
            except Exception:
                p.kill()

    def close(self):
        with self.lock:
            self._stop()
        import shutil
        shutil.rmtree(self.tmpdir, ignore_errors=True)

    @property
    def running(self):
        return bool(self.proc and self.proc.poll() is None)

    # ------------------------------------------------------------------ requests
    def recognize_file(self, path, language=None):
        with self.lock:
            if not self.running:
                self._start()
            rid = self.next_id
            self.next_id += 1
            req = json.dumps({"id": rid, "path": os.path.abspath(path), "lang": language or self.language})
            self.proc.stdin.write((req + "\n").encode("utf-8"))
            self.proc.stdin.flush()
            self.last_used = time.time()
            while True:
                line = self._next_line(self.timeout)
                try:
                    resp = json.loads(line)
                except ValueError:
                    log.debug("ocr host noise: %s", line)
                    continue
                if resp.get("id") == rid:
                    break
            self.last_used = time.time()
        if not resp.get("ok"):
            raise ToolError(f"OCR failed: {resp.get('error')}")
        return resp

    def _cleanup(self, path):
        """WinRT may keep a just-read file open for a moment: delete files a few requests later."""
        self.pending_delete.append(path)
        keep = []
        for p in self.pending_delete:
            if p == path:
                keep.append(p)
                continue
            try:
                os.remove(p)
            except OSError:
                keep.append(p)
        self.pending_delete = keep[-32:]

    def recognize(self, img, region=None, scale=None, language=None, invert=False, binarize=None, pad=0,
                  pad_color=(255, 255, 255)):
        """OCR a numpy RGB image (optionally a client region [x, y, w, h]). Boxes come back in the image's
        coordinates. scale defaults to 2x for small crops (game fonts are small) within the engine's size limit.
        pad adds a plain border first: Windows OCR misses text that touches the image edge."""
        sub = images.crop(img, region) if region else img
        ox, oy = (int(region[0]), int(region[1])) if region else (0, 0)
        h, w = sub.shape[:2]
        if scale is None:
            scale = 2.0 if max(w, h) <= 1300 else 1.0
        scale = min(float(scale), self.max_dim / float(max(w, h)))
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
        if scale != 1.0:
            pim = images.upscale(pim, scale)
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
                bx = [int(round(wd["x"] / scale)) + ox, int(round(wd["y"] / scale)) + oy,
                      max(1, int(round(wd["w"] / scale))), max(1, int(round(wd["h"] / scale)))]
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
