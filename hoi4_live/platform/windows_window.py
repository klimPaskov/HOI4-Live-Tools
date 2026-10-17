"""The Hearts of Iron IV window: discovery by process image, client geometry, focus and pinning.

Matching by owning process (hoi4.exe) avoids false hits such as an Explorer window opened on the game folder,
whose title is also "Hearts of Iron IV". All public coordinates are client pixels; to_screen() converts and refuses
anything outside the client area, which is how "act only inside the HOI4 window" is enforced.
"""

import os
import time

from ..errors import GameNotRunning, ToolError
from . import win32


class GameWindow:
    def __init__(self, process_name="hoi4.exe", title_prefix="Hearts of Iron IV", pin=True,
                 process_backend=None, expected_identity=None, window_id=None):
        from . import current
        self.process_backend = process_backend or current()
        self._identity = expected_identity
        self._requested_window_id = window_id
        self.process_name = process_name.lower()
        self.title_prefix = title_prefix
        self.pin = pin
        self.hwnd = window_id
        self.pid = expected_identity.pid if expected_identity else None

    def bind_identity(self, identity):
        if self._identity is not None and self._identity != identity:
            raise ToolError("window is already bound to a different process identity")
        self._identity = identity
        self.pid = identity.pid

    # ------------------------------------------------------------------ discovery
    def find(self):
        """Locate the top-level visible window owned by hoi4.exe. Returns hwnd or None."""
        win32.set_dpi_awareness()
        if self._identity is not None:
            if self.process_backend.capture_process_identity(self._identity.pid) != self._identity:
                raise ToolError("game process identity changed; refusing to reuse its window",
                                {"error": "process_identity_changed", "pid": self._identity.pid})
        if (self.hwnd and win32.user32.IsWindow(self.hwnd) and win32.pid_alive(self.pid)
                and win32.window_pid(self.hwnd) == self.pid):
            return self.hwnd
        self.hwnd = self.pid = None
        pids = {self._identity.pid} if self._identity else set(win32.find_processes(self.process_name))
        if not pids:
            return None
        best = None
        for h in win32.enum_windows():
            if self._requested_window_id is not None and h != self._requested_window_id:
                continue
            pid = win32.window_pid(h)
            if pid not in pids:
                continue
            title = win32.window_text(h)
            x, y, w, hh = win32.client_rect(h)
            score = (title.startswith(self.title_prefix), w * hh)
            if best is None or score > best[0]:
                best = (score, h, pid)
        if best:
            _, self.hwnd, self.pid = best
            identity = self.process_backend.capture_process_identity(self.pid)
            if identity is None or (self._identity is not None and identity != self._identity):
                raise ToolError("cannot verify game process identity during window discovery")
            self._identity = identity
        return self.hwnd

    def process_ids(self):
        return win32.find_processes(self.process_name)

    def require(self):
        if not self.find():
            raise GameNotRunning("Hearts of Iron IV is not running (no hoi4.exe window). Use hoi4_launch first.")
        return self.hwnd

    # ------------------------------------------------------------------ geometry
    def client(self):
        """(x, y, w, h) of the client area in screen pixels."""
        return win32.client_rect(self.require())

    def size(self):
        _, _, w, h = self.client()
        return w, h

    def to_screen(self, x, y, clamp=False):
        cx, cy, w, h = self.client()
        if not (0 <= x < w and 0 <= y < h):
            if clamp:
                x, y = min(max(0, x), w - 1), min(max(0, y), h - 1)
            else:
                raise ToolError(f"point ({x}, {y}) is outside the game client area {w}x{h}",
                                {"client_size": [w, h]})
        return int(cx + x), int(cy + y)

    def from_screen(self, sx, sy):
        cx, cy, _, _ = self.client()
        return sx - cx, sy - cy

    def title(self):
        return win32.window_text(self.require())

    def monitor(self):
        return win32.monitor_of(self.require())

    # ------------------------------------------------------------------ focus
    def is_foreground(self):
        h = self.find()
        return bool(h) and win32.foreground() == h

    def ensure_foreground(self):
        """Focus the game; abort (instead of typing into another app) when Windows refuses."""
        h = self.require()
        for attempt in range(4):              # Windows refuses now and then (another app just took focus)
            if win32.foreground() == h:
                break
            win32.focus(h)
            time.sleep(0.2 + 0.4 * attempt)
        if win32.foreground() != h:
            fg = win32.foreground()
            raise ToolError("could not bring Hearts of Iron IV to the foreground; refusing to send input to "
                            f"'{win32.window_text(fg)}'", {"foreground": win32.window_text(fg)})
        if self.pin:
            self.pin_client()

    def pin_client(self):
        """Keep the client area at the screen's top-left (title bar off-screen) when the window is windowed and
        fits the monitor. Other desktop tools on this machine use the same placement."""
        h = self.require()
        cx, cy, w, hh = win32.client_rect(h)
        _, (ml, mt, mr, mb) = win32.monitor_of(h)
        if (cx, cy) == (ml, mt) or w > mr - ml or hh > mb - mt:
            return False
        wx, wy, _, _ = win32.window_rect(h)
        win32.move_window(h, ml - (cx - wx), mt - (cy - wy))
        time.sleep(0.3)
        return True

    def close_notepad_logs(self):
        """The debug build opens error.log in Notepad when errors arrive; close those windows (WM_CLOSE)."""
        closed = []
        for h in win32.enum_windows():
            title = win32.window_text(h)
            if "error.log" not in title.lower():
                continue
            img = (win32.process_image(win32.window_pid(h)) or "").lower()
            if os.path.basename(img) in ("notepad.exe", "notepad++.exe") or "notepad" in title.lower():
                win32.post_close(h)
                closed.append(title)
        return closed
