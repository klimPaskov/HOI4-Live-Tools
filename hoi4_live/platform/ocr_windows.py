"""Lazy Windows.Media.Ocr transport through the persistent PowerShell host."""

import json
import logging
import os
import queue
import shutil
import subprocess
import sys
import threading
import time

from ..errors import CapabilityUnavailable, ToolError

log = logging.getLogger("hoi4_live.ocr")
HOST_SCRIPT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ocr", "ocr_host.ps1")


class WindowsOcrBackend:
    def __init__(self, language="en-US", idle_shutdown=600, timeout=30):
        self.language = language
        self.idle_shutdown = idle_shutdown
        self.timeout = timeout
        self.proc = None
        self.max_dim = 2600
        self.lines = queue.Queue()
        self.lock = threading.RLock()
        self.last_used = 0.0
        self.next_id = 1
        self._idle_thread = None
        self._idle_stop = threading.Event()

    # ------------------------------------------------------------------ process
    def _start(self):
        if sys.platform != "win32":
            raise CapabilityUnavailable("ocr", sys.platform, "Windows OCR requires Windows")
        if shutil.which("powershell.exe") is None:
            raise CapabilityUnavailable("ocr", "windows", "Windows PowerShell is unavailable",
                                        "Install or enable Windows PowerShell and its OCR language pack")
        if not os.path.exists(HOST_SCRIPT):
            raise ToolError(f"OCR host script missing: {HOST_SCRIPT}")
        self.lines = queue.Queue()
        try:
            self.proc = subprocess.Popen(
                ["powershell.exe", "-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                 "-File", HOST_SCRIPT],
                stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                creationflags=0x08000000)                           # CREATE_NO_WINDOW
        except OSError as exc:
            raise CapabilityUnavailable("ocr", "windows", "Cannot start Windows OCR: %s" % exc,
                                        "Allow Windows PowerShell to run the bundled OCR host") from exc
        threading.Thread(target=self._reader, args=(self.proc, self.lines), daemon=True).start()
        threading.Thread(target=self._drain, args=(self.proc,), daemon=True).start()
        first = self._next_line(timeout=60)
        try:
            ready = json.loads(first)
            if not isinstance(ready, dict) or not ready.get("ready"):
                raise ValueError("missing ready handshake")
            self.max_dim = int(ready.get("max_dim") or 2600)
            if self.max_dim <= 0:
                raise ValueError("invalid maximum image dimension")
        except (TypeError, ValueError):
            self._stop()
            raise ToolError(f"OCR host failed to start: {first!r}")
        log.info("OCR host started (pid %s, max dimension %s)", self.proc.pid, self.max_dim)
        if self._idle_thread is None or self._idle_stop.is_set():
            self._idle_stop = threading.Event()
            self._idle_thread = threading.Thread(target=self._idle_watch, args=(self._idle_stop,), daemon=True)
            self._idle_thread.start()

    def _reader(self, proc, lines):
        # A terminating old host must never enqueue EOF into its replacement's queue.
        try:
            for raw in proc.stdout:
                lines.put(raw.decode("utf-8", "replace").strip())
        except (OSError, ValueError):
            pass  # The owner closed a terminated host's pipes.
        finally:
            lines.put(None)

    def _drain(self, proc):
        try:
            for raw in proc.stderr:
                log.debug("ocr host stderr: %s", raw.decode("utf-8", "replace").rstrip())
        except (OSError, ValueError):
            pass

    def _next_line(self, timeout):
        try:
            line = self.lines.get(timeout=timeout)
        except queue.Empty:
            self.close()
            raise ToolError("OCR host did not answer in time")
        if line is None:
            self._stop()
            raise ToolError("OCR host exited unexpectedly")
        return line

    def _idle_watch(self, stop):
        while not stop.wait(15):
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
                p.wait(timeout=3)
        if p:
            for pipe in (p.stdin, p.stdout, p.stderr):
                if pipe:
                    pipe.close()

    def close(self):
        with self.lock:
            self._idle_stop.set()
            self._stop()

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
            try:
                self.proc.stdin.write((req + "\n").encode("utf-8"))
                self.proc.stdin.flush()
            except OSError as exc:
                self._stop()
                raise ToolError("OCR host connection failed: %s" % exc) from exc
            self.last_used = time.time()
            deadline = time.monotonic() + self.timeout
            while True:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    self.close()
                    raise ToolError("OCR host did not answer in time")
                line = self._next_line(remaining)
                try:
                    resp = json.loads(line)
                except ValueError:
                    log.debug("ocr host noise: %s", line)
                    continue
                if isinstance(resp, dict) and resp.get("id") == rid:
                    break
            self.last_used = time.time()
        if not resp.get("ok"):
            raise ToolError(f"OCR failed: {resp.get('error')}")
        return resp

