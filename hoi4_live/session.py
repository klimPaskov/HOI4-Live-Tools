"""The session: one object that owns configuration, the desktop lock, the game window, capture, input, OCR, logs,
the session mode and the artifacts folder. Every tool receives it as its first argument.

Heavy members (OCR host) start lazily and stop on shutdown, so an idle server costs almost nothing.
"""

import atexit
import datetime
import logging
import os
import threading
import time
import uuid

from . import config as config_mod
from . import perf
from . import platform
from .desktop.capture import Capturer
from .desktop.images import save_artifact
from .desktop.inputs import Input
from .desktop.lock import DesktopLock
from .desktop.window import GameWindow
from .errors import ToolError
from .game.logs import Logs

log = logging.getLogger("hoi4_live.session")


class Cancelled(ToolError):
    pass


class Session:
    def __init__(self, cfg=None, backend=None):
        self.cfg = cfg or config_mod.get()
        self.platform = backend or platform.current()
        c = self.cfg
        self.mode = c.mode if c.mode in ("test", "campaign") else "test"
        self._campaign_latched = self.mode == "campaign"
        from .game.play_policy import PlayPolicy
        self.play_policy = PlayPolicy(getattr(c, "play_mode", "fair"), session_id=uuid.uuid4().hex)
        self.lock = DesktopLock(c.lock_path, c["lock"]["stale_seconds"], c["lock"]["wait_seconds"], backend=self.platform)
        w = c["window"]
        self.window = self.platform.make_window(w["process_name"], w["title_prefix"], w["pin_client_to_origin"])
        cap = c["capture"]
        self.capturer = self.platform.make_capturer(self.window, cap["ffmpeg"], cap["method"], cap["output_idx"], cap["timeout_seconds"])
        i = c["input"]
        self.input = self.platform.make_input(self.window, i["key_delay"], i["type_delay"], i["click_hold"], i["settle"])
        self.logs = Logs(c.logs_dir)
        perf.instrument(self.capturer, ["grab"], "grab")
        perf.instrument(self.input, ["click", "key", "type_text", "drag", "scroll", "move"], "input")
        self._ocr = None
        self._telemetry = None
        self.briefings = []                  # previous briefings of this server session (for deltas)
        self.popup_log = []                  # popups seen and answered this session (for briefings)
        self._call = threading.local()
        self.started = time.time()
        self.run_id = datetime.datetime.now().strftime("%Y%m%d_%H%M%S")
        self.artifacts_dir = os.path.join(c.artifacts_dir, self.run_id)
        self.last_popups = []
        self.launched_pid = None
        self.launched_identity = None
        self.config_session = None
        self.config_owner_id = None
        self.owned_process_only = self._campaign_latched or os.environ.get("HOI4_LIVE_OWNED_ONLY", "0") == "1"
        self._memory_process = None
        self._memory_reader = None
        self.governor = None
        self.calibration = {}
        self.ui_mutex = threading.RLock()       # keyboard and mouse sections: acting tools, the background clock
        self.clock = None                       # the latest AdvanceRun (hoi4_advance, hoi4_clock)
        self.standing = None                    # standing orders (state/standing.py)
        self.date_uncertain = False
        self.image_mode = "all"                 # all | errors | none: screenshots attached to results
        self.call_log_path = os.environ.get("HOI4_LIVE_CALL_LOG") or c.data.get("call_log") or ""
        try:
            self.logs.mark("session", ("error", "game", "system"))
        except OSError:
            pass
        atexit.register(self.shutdown)

    def set_mode(self, mode):
        from .errors import ModeRefused
        if mode not in ("test", "campaign"):
            raise ModeRefused("mode must be test or campaign")
        if mode == "test" and self._campaign_latched:
            raise ModeRefused("campaign sessions cannot return to test mode; use a separate QA session")
        self.mode = mode
        self._campaign_latched |= mode == "campaign"
        if mode == "campaign":
            self.owned_process_only = True
        return mode

    def authorize_tool(self, name, arguments, test_only=False):
        if self.mode == "campaign" or self._campaign_latched or arguments.get("mode") == "campaign":
            self.set_mode("campaign")
            return self.play_policy.authorize(name, arguments, test_only=test_only)

    def prepare_config_session(self):
        """Take byte-exact backups before campaign launch or playset mutation."""
        if self.config_session is None or self.config_session.manifest["state"] == "restored":
            from .game.session_backup import ConfigSession
            self.config_session = ConfigSession.begin(
                self.cfg.user_dir, os.path.join(self.cfg.artifacts_dir, "config_backups"))
        return self.config_session

    def restore_configs_after_exit(self):
        """Restore only when the retained owned launch has verifiably exited.

        Unknown external/game rewrites remain conflicts, never blindly accepted
        as owned writes. A recovery journal stays outside Git for those cases.
        """
        from .game import process
        if self.config_session is None:
            return None
        state = process.launch_status(self.launched_pid)
        if not state or state["running"] or process.running_pids(self.cfg["window"]["process_name"]):
            raise ToolError("owned launch exit is not verified or another game is running; configuration preserved")
        game = self.config_session.manifest.get("game")
        if game and game["phase"] != "exited":
            self.config_session.mark_game_exited(self.config_owner_id)
        return self.config_session.restore()

    # ------------------------------------------------------------------ per-call context
    def begin_call(self, name, progress=None, cancel=None):
        """Enter a tool call; nested calls (hoi4_scenario steps) inherit the outer progress and cancel hooks.
        Returns the previous context for end_call()."""
        prev = (getattr(self._call, "name", None), getattr(self._call, "progress", None),
                getattr(self._call, "cancel", None))
        self._call.name = name
        self._call.progress = progress if progress is not None else prev[1]
        self._call.cancel = cancel if cancel is not None else prev[2]
        return prev

    def end_call(self, prev=None):
        prev = prev or (None, None, None)
        self._call.name, self._call.progress, self._call.cancel = prev

    def report(self, message, value=None, total=None):
        """Progress notification to the client (if it asked for progress) and a heartbeat for the lock."""
        log.info("progress: %s", message)
        cb = getattr(self._call, "progress", None)
        if cb:
            try:
                cb(message, value, total)
            except Exception as e:          # a closed client pipe must never abort an action half-way
                log.warning("progress callback failed: %s", e)
        if self.lock.held_by_us():
            self.lock.refresh()

    def check_cancel(self):
        ev = getattr(self._call, "cancel", None)
        if ev is not None and ev.is_set():
            raise Cancelled("cancelled by the client")

    def sleep(self, seconds):
        """Sleep in small steps so cancellation is noticed."""
        t0 = time.time()
        try:
            self._sleep(seconds)
        finally:
            perf.add("sleep", time.time() - t0)

    def _sleep(self, seconds):
        end = time.time() + seconds
        while True:
            self.check_cancel()
            left = end - time.time()
            if left <= 0:
                return
            time.sleep(min(0.25, left))

    # ------------------------------------------------------------------ members
    @property
    def ocr(self):
        if self._ocr is None:
            from .ocr.winocr import OcrHost
            o = self.cfg["ocr"]
            self._ocr = OcrHost(o["language"], o["idle_shutdown_seconds"], o["request_timeout_seconds"])
            perf.instrument(self._ocr, ["recognize"], "ocr")
        return self._ocr

    @property
    def telemetry(self):
        """The telemetry store (HOI4LIVE lines of game.log), refreshed on every access (a cheap byte delta)."""
        if self._telemetry is None:
            from .telemetry.parser import TelemetryStore
            self._telemetry = TelemetryStore(os.path.join(self.cfg.logs_dir, "game.log"))
            try:
                from .telemetry import modgen
                man = modgen.read_manifest(self.cfg.user_dir) or {}
                self._telemetry.set_catalog(man.get("techs"))
            except Exception as e:
                log.warning("telemetry catalog not read: %s", e)
        try:
            self._telemetry.refresh()
        except OSError as e:
            log.warning("telemetry refresh failed: %s", e)
        if self.mode == "campaign":
            from .telemetry import modgen
            manifest = modgen.read_manifest(self.cfg.user_dir)
            if not modgen.campaign_compatible(manifest):
                raise ToolError("campaign telemetry is not installed; enable it while the game is closed",
                                {"error": "campaign_telemetry_required"})
            if self._telemetry.generation != manifest["generation"]:
                raise ToolError("no fresh records from this campaign telemetry generation yet",
                                {"error": "campaign_telemetry_not_active"})
        return self._telemetry

    def require_game(self):
        hwnd = self.window.require()
        if self.owned_process_only:
            self.require_owned_game()
        return hwnd

    def require_owned_game(self):
        """Validate our launch, including creation time, without reading game memory."""
        from .memory.winproc import MemoryReadError
        identity = self.launched_identity
        if identity is None or self.launched_pid != identity.pid:
            raise ToolError("this session did not launch the game; wait for it to exit, then hoi4_launch",
                            {"error": "unowned_process"})
        # A second game must not redirect UI actions away from the owned instance.
        if not self.window.find() or self.window.pid != identity.pid:
            raise ToolError("the game window does not belong to this session's launch",
                            {"error": "unowned_process"})
        try:
            current = self.platform.capture_process_identity(identity.pid, identity.executable_path)
        except (OSError, MemoryReadError) as exc:
            raise ToolError("owned game exited or its identity cannot be verified",
                            {"error": "unowned_process"}) from exc
        if current != identity:
            raise ToolError("game PID was reused; refusing access", {"error": "unowned_process"})
        return identity

    def memory_snapshot(self, clock_only=False):
        """Opt-in, read-only player snapshot. Never attaches to a discovered game."""
        from .memory.winproc import ReadOnlyProcess, MemoryReadError
        from .memory.reader import MemoryReader
        from .memory.pe import load_rtti
        identity = self.require_owned_game()
        if self.platform.name != "windows":
            from .errors import CapabilityUnavailable
            raise CapabilityUnavailable("live memory layout", self.platform.name,
                                        "no independently validated build/architecture layout is registered",
                                        "Use fresh own-country telemetry and UI observations")
        try:
            if self._memory_process is None or self._memory_process.closed:
                img, rtti = load_rtti(identity.executable_path)
                proc = ReadOnlyProcess(identity, owned=True)
                try:
                    module = proc.module_info()
                    reader = MemoryReader(proc, module.base, img.build_key, rtti, module_size=module.size)
                except BaseException:
                    proc.close()
                    raise
                self._memory_process, self._memory_reader = proc, reader
            return self._memory_reader.clock_snapshot() if clock_only else self._memory_reader.snapshot()
        except (OSError, MemoryReadError, ValueError) as exc:
            raise ToolError(str(exc), {"error": "memory_unavailable", "fallback": "hoi4_briefing"}) from exc

    # ------------------------------------------------------------------ capture
    def grab(self, focus=None):
        """Screenshot of the game client as an RGB numpy array.

        ddagrab copies the monitor, so the game must be on top. When the lock is ours (acting tools) the game is
        brought to the foreground first; read-only callers get PrintWindow when it works, otherwise ddagrab of
        whatever is visible (flagged in self.capturer.last_method)."""
        self.require_game()
        if focus is None:
            focus = self.lock.held_by_us()
        if focus and not self.window.is_foreground():
            self.window.ensure_foreground()
            time.sleep(0.3)
        try:
            return self.capturer.grab()
        except ToolError as e:
            # a blank PrintWindow frame and a ddagrab timeout happen now and then while the game is busy (seen
            # in phase 4 right after painting a battle plan line): one more try after a pause
            log.warning("capture failed (%s); retrying", e)
            time.sleep(1.0)
            return self.capturer.grab()

    def artifact(self, img, name):
        return save_artifact(img, self.artifacts_dir, name)

    def artifact_path(self, name):
        os.makedirs(self.artifacts_dir, exist_ok=True)
        return os.path.join(self.artifacts_dir, name)

    def park_cursor(self):
        try:
            fx, fy = self.cfg["input"]["park"]
            self.input.park(fx, fy)
        except ToolError:
            pass

    # ------------------------------------------------------------------ lifecycle
    def shutdown(self):
        run = getattr(self, "clock", None)
        if run is not None and not run.done.is_set():
            # a background advance must not leave the game running unattended: stop it (it pauses the game)
            try:
                run.stop()
                run.join(20)
            except Exception:
                pass
        try:
            if self._memory_process is not None:
                self._memory_process.close()
                self._memory_process = self._memory_reader = None
        except Exception:
            pass
        try:
            if self._ocr is not None:
                self._ocr.close()
                self._ocr = None
        except Exception:
            pass
        try:
            self.lock.release_all()
        except Exception:
            pass
