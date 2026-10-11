"""macOS desktop backend, with lazy PyObjC loading and injectable native services.

Client coordinates are pixels in the selected capture space (logical for
session tools, backing pixels for recordings). Quartz event coordinates are global
logical points (including negative origins on secondary displays). Never reuse
Windows DPI arithmetic here. Window content geometry is fail-closed: a fullscreen
window, or an Accessibility content element with bounds, must establish it.
"""

import math
import threading
import time
from dataclasses import dataclass

import numpy as np

from ..errors import CapabilityUnavailable, GameNotRunning, ToolError


def unavailable(feature, reason, remedy=None):
    return CapabilityUnavailable(feature, "macos", reason, remedy)


@dataclass(frozen=True)
class Geometry:
    x: float
    y: float
    width: float
    height: float
    scale: float

    def __post_init__(self):
        if (not all(math.isfinite(v) for v in (self.x, self.y, self.width, self.height, self.scale))
                or self.width <= 0 or self.height <= 0 or self.scale <= 0):
            raise unavailable("window geometry", "invalid or non-finite client geometry")

    @property
    def pixels(self):
        return int(round(self.width * self.scale)), int(round(self.height * self.scale))

    def to_screen(self, x, y, clamp=False):
        w, h = self.pixels
        if not all(math.isfinite(float(v)) for v in (x, y)):
            raise ToolError("client coordinates must be finite")
        if not (0 <= x < w and 0 <= y < h):
            if not clamp:
                raise ToolError("point is outside the game client area", {"client_size": [w, h]})
            x, y = min(max(0, x), w - 1), min(max(0, y), h - 1)
        return self.x + x / self.scale, self.y + y / self.scale

    def from_screen(self, x, y):
        return (x - self.x) * self.scale, (y - self.y) * self.scale


class Native:
    """All calls to the Apple frameworks. Constructors do not request permissions."""
    def __init__(self):
        self._modules = None

    def _load(self):
        if self._modules is None:
            try:
                import Quartz
                import AppKit
                import ApplicationServices
            except ImportError as exc:
                raise unavailable("desktop", "PyObjC desktop frameworks are unavailable",
                                  "Install the macOS dependencies: Quartz, Cocoa and ApplicationServices") from exc
            self._modules = Quartz, AppKit, ApplicationServices
        return self._modules

    def permission(self, feature):
        q, _, ax = self._load()
        self._require_desktop()
        if feature == "capture":
            check = getattr(q, "CGPreflightScreenCaptureAccess", None)
            if check is None or not check():
                raise unavailable("capture", "Screen Recording permission is unavailable",
                                  "Grant Screen Recording to the host app in System Settings, then restart it")
        else:
            if not ax.AXIsProcessTrusted():
                raise unavailable(feature, "Accessibility permission is unavailable",
                                  "Grant Accessibility to the host app in System Settings, then restart it")

    def _require_desktop(self):
        """Reject a locked or switched-away login session before capture/input."""
        q, _, _ = self._load()
        session = q.CGSessionCopyCurrentDictionary()
        if (not session or not session.get(q.kCGSessionOnConsoleKey)
                or not session.get(q.kCGSessionLoginDoneKey)
                or session.get("CGSSessionScreenIsLocked", False)):
            raise unavailable("desktop session", "the macOS desktop is locked or not the active console session",
                              "Unlock the logged-in desktop before resuming game control or capture")

    def windows(self):
        q, _, _ = self._load()
        items = q.CGWindowListCopyWindowInfo(q.kCGWindowListOptionAll |
                                           q.kCGWindowListExcludeDesktopElements, q.kCGNullWindowID)
        result = []
        for item in items or []:
            b = item.get(q.kCGWindowBounds, {})
            if item.get(q.kCGWindowLayer, 0) != 0 or b.get("Width", 0) <= 0 or b.get("Height", 0) <= 0:
                continue
            result.append({"id": int(item[q.kCGWindowNumber]), "pid": int(item[q.kCGWindowOwnerPID]),
                           "title": str(item.get(q.kCGWindowName, "")),
                           "onscreen": bool(item.get(q.kCGWindowIsOnscreen, False)),
                           "bounds": tuple(float(b[k]) for k in ("X", "Y", "Width", "Height"))})
        return result

    def displays(self):
        q, _, _ = self._load()
        error, ids, _ = q.CGGetActiveDisplayList(64, None, None)
        if error:
            raise unavailable("window geometry", "could not enumerate displays")
        if not ids:
            raise unavailable("window geometry", "macOS reports no active displays",
                              "Wake and unlock the desktop before observing or controlling the game")
        result = []
        for ident in ids:
            rect = q.CGDisplayBounds(ident)
            # Quartz display pixel counts can describe a scaled mode. The mode's
            # backing pixel size is the relevant scale, not its logical width.
            mode = q.CGDisplayCopyDisplayMode(ident)
            scale = float(q.CGDisplayModeGetPixelWidth(mode)) / max(1, q.CGDisplayModeGetWidth(mode))
            result.append((int(ident), Geometry(rect.origin.x, rect.origin.y,
                                               rect.size.width, rect.size.height, scale)))
        return result

    def _attr(self, element, name):
        _, _, ax = self._load()
        error, value = ax.AXUIElementCopyAttributeValue(element, name, None)
        return value if error == 0 else None

    def _ax_rect(self, element):
        """Read public AX point/size values, including older PyObjC releases."""
        _, _, ax = self._load()
        position = self._attr(element, "AXPosition")
        size = self._attr(element, "AXSize")
        if position is None or size is None:
            return None
        if hasattr(ax, "AXValueGetValue"):
            ok_position, p = ax.AXValueGetValue(position, ax.kAXValueCGPointType, None)
            ok_size, s = ax.AXValueGetValue(size, ax.kAXValueCGSizeType, None)
            if ok_position and ok_size:
                return p.x, p.y, s.width, s.height
            return None

        # AXValueGetValue was not wrapped by older PyObjC versions. CF proxies
        # expose their public pointer without depending on object descriptions.
        import ctypes
        import objc

        class Pair(ctypes.Structure):
            _fields_ = [("a", ctypes.c_double), ("b", ctypes.c_double)]

        library = ctypes.CDLL("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices")
        fn = library.AXValueGetValue
        fn.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_void_p]
        fn.restype = ctypes.c_bool
        p, s = Pair(), Pair()
        def pointer(value):
            if hasattr(value, "__c_void_p__"):
                return value.__c_void_p__()
            return ctypes.c_void_p(objc.pyobjc_id(value))
        if not fn(pointer(position), 1, ctypes.byref(p)) or not fn(pointer(size), 2, ctypes.byref(s)):
            return None
        return p.a, p.b, s.a, s.b

    def _ax_window(self, info):
        _, _, ax = self._load()
        application = ax.AXUIElementCreateApplication(info["pid"])
        candidates = self._attr(application, "AXWindows") or []
        matched = []
        for candidate in candidates:
            bounds = self._ax_rect(candidate)
            if bounds and all(abs(a - b) <= 1 for a, b in zip(bounds, info["bounds"])):
                matched.append(candidate)
        return matched[0] if len(matched) == 1 else None

    def frame_geometry(self, info):
        """Known window-server frame bounds; these are not a client rectangle."""
        x, y, w, h = info["bounds"]
        displays = self.displays()
        containing = [(ident, g) for ident, g in displays if x >= g.x and y >= g.y and
                      x + w <= g.x + g.width and y + h <= g.y + g.height]
        if len(containing) != 1:
            raise unavailable("window geometry", "window spans displays or is partly off-screen",
                              "Place the whole game window on one display")
        _, display = containing[0]
        return Geometry(x, y, w, h, display.scale)

    def geometry(self, info):
        frame = self.frame_geometry(info)
        x, y, w, h = info["bounds"]
        displays = self.displays()
        # Exact display bounds establish a borderless fullscreen content rectangle.
        if any(all(abs(a - b) < 0.5 for a, b in zip(info["bounds"],
                (display.x, display.y, display.width, display.height))) for _, display in displays):
            return frame
        self.permission("window geometry")
        element = self._ax_window(info)
        if element is not None:
            if bool(self._attr(element, "AXFullScreen")):
                return frame
            content = self._attr(element, "AXContents")
            if content is not None:
                contents = list(content) if isinstance(content, (list, tuple)) else [content]
                rects = [self._ax_rect(c) for c in contents]
                rects = [r for r in rects if r and r[2] > 0 and r[3] > 0 and r[0] >= x and r[1] >= y and
                         r[0] + r[2] <= x + w and r[1] + r[3] <= y + h]
                if len(rects) == 1:
                    return Geometry(*rects[0], frame.scale)
        raise unavailable("window geometry", "the game does not expose a verified client-content rectangle",
                          "Use fullscreen on a single display; decorated window geometry must be verified first")

    def focused(self, info):
        self._require_desktop()
        _, appkit, ax = self._load()
        app = appkit.NSWorkspace.sharedWorkspace().frontmostApplication()
        if app is None or int(app.processIdentifier()) != info["pid"]:
            return False
        # SDL titles can expose AXWindows/AXFocusedWindow as an AXApplication,
        # without any AX window geometry. Quartz still supplies the window-server
        # ordering and owning PID. Requiring the exact global front window ID
        # prevents another dialog/window of the same process receiving input.
        windows = [item for item in self.windows() if item.get("onscreen", True)]
        if not windows or windows[0]["pid"] != info["pid"]:
            return False
        application = ax.AXUIElementCreateApplication(info["pid"])
        focused = self._attr(application, "AXFocusedWindow")
        rect = self._ax_rect(focused) if focused is not None else None
        if rect is not None:
            # macOS can place its fullscreen titlebar above the main CG window.
            # Exact AX keyboard-focus geometry remains authoritative in that case;
            # a different app's window above it is still refused by the PID check.
            return all(abs(a - b) <= 1 for a, b in zip(rect, info["bounds"]))
        return windows[0]["id"] == info["id"]

    def focus(self, info):
        self.permission("input")
        _, appkit, ax = self._load()
        element = self._ax_window(info)
        app = appkit.NSRunningApplication.runningApplicationWithProcessIdentifier_(info["pid"])
        if app is None or not app.activateWithOptions_(appkit.NSApplicationActivateIgnoringOtherApps):
            raise unavailable("input", "macOS refused to activate the game")
        if element is not None and ax.AXUIElementPerformAction(element, "AXRaise") != 0:
            raise unavailable("input", "macOS refused to raise the game window")

    def capture(self, info, geometry, timeout, method="auto"):
        self.permission("capture")
        q, _, _ = self._load()
        image = None
        selected = None
        if method in ("auto", "screencapturekit"):
            try:
                image = self._screenkit(info, geometry, timeout)
                selected = "screencapturekit"
            except (ImportError, AttributeError) as exc:
                if method == "screencapturekit":
                    raise unavailable("capture", "ScreenCaptureKit screenshot API is unavailable",
                                      "Install pyobjc-framework-ScreenCaptureKit and use macOS 14 or later") from exc
        if image is None and method in ("auto", "quartz"):
            fn = getattr(q, "CGWindowListCreateImage", None)
            if fn is None:
                raise unavailable("capture", "Quartz capture is unavailable; ScreenCaptureKit is required")
            rect = q.CGRectMake(geometry.x, geometry.y, geometry.width, geometry.height)
            image = fn(rect, q.kCGWindowListOptionIncludingWindow, info["id"],
                       q.kCGWindowImageBoundsIgnoreFraming | q.kCGWindowImageBestResolution)
            selected = "quartz"
        if image is None:
            raise unavailable("capture", "window capture returned no image")
        rgb = self._rgb(image)
        if selected == "screencapturekit":
            # Capture the full window, then crop in backing pixels. Supplying a
            # window-local sourceRect shifts SCK pixels when the global origin is
            # nonzero (observed +68 px for a Retina fullscreen window at y=34).
            x, y, w, h = info["bounds"]
            expected = (int(round(h * geometry.scale)), int(round(w * geometry.scale)), 3)
            if rgb.shape != expected:
                raise unavailable("capture", "ScreenCaptureKit full-window size changed")
            left = int(round((geometry.x - x) * geometry.scale))
            top = int(round((geometry.y - y) * geometry.scale))
            width, height = geometry.pixels
            if left < 0 or top < 0 or left + width > rgb.shape[1] or top + height > rgb.shape[0]:
                raise unavailable("capture", "verified client rectangle is outside captured window")
            rgb = rgb[top:top + height, left:left + width].copy()
        return rgb, selected

    def _screenkit(self, info, geometry, timeout):
        import ScreenCaptureKit as sc
        q, _, _ = self._load()
        done = threading.Event()
        outcome = []
        def content_ready(content, error):
            outcome.extend((content, error))
            done.set()
        sc.SCShareableContent.getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler_(
            True, False, content_ready)
        if not done.wait(timeout):
            raise unavailable("capture", "ScreenCaptureKit window enumeration timed out")
        content, error = outcome
        if error is not None or content is None:
            raise unavailable("capture", "ScreenCaptureKit window enumeration failed: %s" % error)
        matches = [w for w in content.windows() if int(w.windowID()) == info["id"] and
                   int(w.owningApplication().processID()) == info["pid"]]
        if len(matches) != 1:
            raise unavailable("capture", "owned window disappeared from ScreenCaptureKit")
        filt = sc.SCContentFilter.alloc().initWithDesktopIndependentWindow_(matches[0])
        config = sc.SCStreamConfiguration.alloc().init()
        frame_width = int(round(info["bounds"][2] * geometry.scale))
        frame_height = int(round(info["bounds"][3] * geometry.scale))
        config.setWidth_(frame_width)
        config.setHeight_(frame_height)
        config.setShowsCursor_(False)
        if hasattr(config, "setIgnoreShadowsSingleWindow_"):
            config.setIgnoreShadowsSingleWindow_(True)
        done = threading.Event()
        outcome = []
        def image_ready(image, error):
            outcome.extend((image, error))
            done.set()
        sc.SCScreenshotManager.captureImageWithFilter_configuration_completionHandler_(filt, config, image_ready)
        if not done.wait(timeout):
            raise unavailable("capture", "ScreenCaptureKit screenshot timed out")
        image, error = outcome
        if error is not None or image is None:
            raise unavailable("capture", "ScreenCaptureKit screenshot failed: %s" % error)
        return image

    def _rgb(self, image):
        q, _, _ = self._load()
        # Render into a declared RGBA layout, avoiding assumptions about native
        # CGImage channel order, row padding or alpha representation.
        width, height = q.CGImageGetWidth(image), q.CGImageGetHeight(image)
        rgba = np.zeros((height, width, 4), dtype=np.uint8)
        context = q.CGBitmapContextCreate(rgba, width, height, 8, width * 4, q.CGColorSpaceCreateDeviceRGB(),
                                        q.kCGImageAlphaPremultipliedLast | q.kCGBitmapByteOrder32Big)
        if context is None:
            raise unavailable("capture", "could not allocate RGBA bitmap context")
        q.CGContextDrawImage(context, q.CGRectMake(0, 0, width, height), image)
        return rgba[:, :, :3].copy()

    def cursor(self):
        q, _, _ = self._load()
        event = q.CGEventCreate(None)
        p = q.CGEventGetLocation(event)
        return p.x, p.y

    def post_mouse(self, action, position, button="left", flags=0, clicks=1):
        q, _, _ = self._load()
        codes = {"left": (q.kCGMouseButtonLeft, q.kCGEventLeftMouseDown, q.kCGEventLeftMouseUp,
                          q.kCGEventLeftMouseDragged),
                 "right": (q.kCGMouseButtonRight, q.kCGEventRightMouseDown, q.kCGEventRightMouseUp,
                           q.kCGEventRightMouseDragged),
                 "middle": (q.kCGMouseButtonCenter, q.kCGEventOtherMouseDown, q.kCGEventOtherMouseUp,
                            q.kCGEventOtherMouseDragged)}
        number, down, up, drag = codes[button]
        kind = {"move": q.kCGEventMouseMoved, "down": down, "up": up, "drag": drag}[action]
        event = q.CGEventCreateMouseEvent(None, kind, position, number)
        if event is None:
            raise unavailable("input", "could not create mouse event")
        q.CGEventSetFlags(event, flags)
        q.CGEventSetIntegerValueField(event, q.kCGMouseEventClickState, clicks)
        q.CGEventPost(q.kCGHIDEventTap, event)

    def post_key(self, code, down, flags):
        q, _, _ = self._load()
        event = q.CGEventCreateKeyboardEvent(None, code, down)
        if event is None:
            raise unavailable("input", "could not create keyboard event")
        q.CGEventSetFlags(event, flags)
        q.CGEventPost(q.kCGHIDEventTap, event)

    def post_text(self, text, down):
        q, _, _ = self._load()
        event = q.CGEventCreateKeyboardEvent(None, 0, down)
        if event is None:
            raise unavailable("input", "could not create text event")
        q.CGEventKeyboardSetUnicodeString(event, len(text.encode("utf-16-le")) // 2, text)
        q.CGEventPost(q.kCGHIDEventTap, event)

    def post_scroll(self, notches, flags):
        q, _, _ = self._load()
        event = q.CGEventCreateScrollWheelEvent(None, q.kCGScrollEventUnitLine, 1, int(notches))
        if event is None:
            raise unavailable("input", "could not create scroll event")
        q.CGEventSetFlags(event, flags)
        q.CGEventPost(q.kCGHIDEventTap, event)


class GameWindow:
    def __init__(self, process_name="hoi4", title_prefix="Hearts of Iron IV", pin=True,
                 native=None, process_backend=None, expected_identity=None, window_id=None, coordinate_space="pixels"):
        from . import current
        if coordinate_space not in ("pixels", "logical"):
            raise ValueError("coordinate_space must be pixels or logical")
        self.coordinate_space = coordinate_space
        self.native = native or Native()
        self.process_backend = process_backend or current()
        self.process_name = process_name.lower()
        self.title_prefix = title_prefix
        self.pin = pin
        self.expected_identity = expected_identity
        self._identity = expected_identity
        self._requested_window_id = window_id
        self.hwnd = window_id
        self.pid = expected_identity.pid if expected_identity is not None else None
        self._info = None

    def bind_identity(self, identity):
        """Bind to a session-owned process before discovery; never weaken an existing binding."""
        if self._identity is not None and self._identity != identity:
            raise ToolError("window is already bound to a different process identity")
        self.expected_identity = self._identity = identity
        self.pid = identity.pid

    def _identity_for(self, pid):
        return self.process_backend.capture_process_identity(pid)

    def find(self):
        pids = ([self._identity.pid] if self._identity is not None else self.process_ids())
        if self._identity is not None:
            if self._identity.pid not in self.process_ids():
                self.hwnd = self._info = None
                return None
            actual = self._identity_for(self._identity.pid)
            if actual != self._identity:
                self.hwnd = self._info = None
                raise ToolError("game process identity changed; refusing to reuse its window",
                                {"error": "process_identity_changed", "pid": self._identity.pid})
        candidates = [info for info in self.native.windows() if info["pid"] in pids and
                      (self._requested_window_id is None or info["id"] == self._requested_window_id)]
        if self.hwnd is not None:
            exact = [info for info in candidates if info["id"] == self.hwnd]
            if not exact and self._requested_window_id is not None:
                self._info = None
                return None
            if exact:
                candidates = exact
            else:
                # Launchers replace their splash window with the real game
                # window. Only an implicit cache can follow that transition,
                # and only inside the creation-time/executable-validated process.
                # An explicitly pinned recorder/diagnostic ID must never follow.
                self.hwnd = None
        if not candidates:
            self._info = None
            return None
        best = max(candidates, key=lambda item: (item["title"].startswith(self.title_prefix),
                                                 item["bounds"][2] * item["bounds"][3]))
        identity = self._identity_for(best["pid"])
        if identity is None:
            raise unavailable("window ownership", "cannot establish the game's process identity")
        if self._identity is not None and self._identity != identity:
            raise ToolError("game process identity changed during window discovery")
        self._identity = identity
        self.hwnd, self.pid, self._info = best["id"], best["pid"], best
        return self.hwnd

    def process_ids(self):
        return self.process_backend.find_processes(self.process_name)

    def require(self):
        if self.find() is None:
            raise GameNotRunning("Hearts of Iron IV has no visible owned window; launch it or restore its window")
        return self.hwnd

    def geometry(self):
        self.require()
        return self.native.geometry(self._info)

    def client(self):
        """Global origin in Quartz points, width/height in client capture pixels.

        Use to_screen/from_screen for conversion; adding a pixel to the origin
        is not valid on Retina. This tuple remains for legacy status reporting.
        """
        g = self.client_geometry()
        return g.x, g.y, *g.pixels

    def client_geometry(self):
        """Coordinate space shared by capture, OCR and input.

        Native geometry always retains the backing scale for exact capture.
        Session tools use logical client pixels so gui_scale=1 layout remains
        the same across Retina and non-Retina displays. Recorders explicitly
        retain backing pixels and never supply tool click coordinates.
        """
        g = self.geometry()
        if self.coordinate_space == "logical":
            return Geometry(g.x, g.y, g.width, g.height, 1.0)
        return g

    def size(self):
        return self.client_geometry().pixels

    def to_screen(self, x, y, clamp=False):
        return self.client_geometry().to_screen(x, y, clamp)

    def from_screen(self, x, y):
        return self.client_geometry().from_screen(x, y)

    def title(self):
        self.require()
        return self._info["title"]

    def monitor(self):
        g = self.geometry()
        for index, (_, d) in enumerate(self.native.displays()):
            if d.x <= g.x and d.y <= g.y and g.x + g.width <= d.x + d.width and g.y + g.height <= d.y + d.height:
                return index, (d.x, d.y, d.x + d.width, d.y + d.height)
        raise unavailable("window geometry", "window has no single containing display")

    def is_foreground(self):
        return bool(self.find()) and self.native.focused(self._info)

    def ensure_foreground(self):
        self.require()
        self.native.permission("input")
        for _ in range(3):
            if self.native.focused(self._info):
                return
            self.native.focus(self._info)
            time.sleep(0.15)
            self.require()
        if not self.native.focused(self._info):
            raise unavailable("input", "game window is not focused; refusing input")

    def pin_client(self):
        # Moving a fullscreen/borderless window is unnecessary. A titlebar offset
        # is never inferred, and geometry must still be verified before input.
        self.geometry()
        return False

    def close_notepad_logs(self):
        return []


class Capturer:
    def __init__(self, window, ffmpeg="ffmpeg", method="auto", output_idx="auto", timeout=20, native=None,
                 allow_blank=False):
        self.window, self.ffmpeg, self.method = window, ffmpeg, method
        self.output_idx, self.timeout = output_idx, timeout
        self.native = native or window.native
        self.last_method = None
        self.allow_blank = allow_blank

    def grab(self, method=None):
        from ..desktop.capture import is_blank
        method = method or self.method
        if method not in ("auto", "quartz", "screencapturekit"):
            raise unavailable("capture", "capture method %r is not supported on macOS" % method,
                              "Use auto, screencapturekit or quartz")
        g = self.window.geometry()
        before = dict(self.window._info)
        rgb, used = self.native.capture(before, g, self.timeout, method)
        after_geometry = self.window.geometry()
        if self.window._info != before or after_geometry != g:
            raise ToolError("window changed during capture; discard this frame and retry")
        width, height = g.pixels
        if rgb.dtype != np.uint8 or rgb.shape != (height, width, 3):
            raise unavailable("capture", "capture pixel dimensions do not match verified Retina geometry",
                              "Recalibrate this display's capture scale before sending input")
        if not self.allow_blank and is_blank(rgb):
            raise unavailable("capture", "capture returned a blank frame; no usable game image")
        self.last_method = used
        if self.window.coordinate_space == "logical" and g.scale != 1:
            from PIL import Image
            # Downsample only after validating complete native capture geometry.
            size = (int(round(g.width)), int(round(g.height)))
            rgb = np.asarray(Image.fromarray(rgb).resize(size, Image.LANCZOS))
        return rgb

    def capture_frame(self, method=None):
        """Diagnostic owned-window image, never a source of client click coordinates.

        This can inspect a decorated SDL frame whose AX content rectangle is
        unavailable. It requires an explicit process binding and labels the
        coordinate space so callers cannot confuse the titlebar with game UI.
        """
        if self.window.expected_identity is None:
            raise unavailable("capture", "diagnostic frame capture requires an explicitly owned process identity")
        method = method or self.method
        if method not in ("auto", "quartz", "screencapturekit"):
            raise unavailable("capture", "capture method %r is not supported on macOS" % method)
        self.window.require()
        before = dict(self.window._info)
        frame = self.native.frame_geometry(before)
        rgb, used = self.native.capture(before, frame, self.timeout, method)
        self.window.require()
        if self.window._info != before or self.native.frame_geometry(self.window._info) != frame:
            raise ToolError("window changed during diagnostic capture; discard this frame and retry")
        width, height = frame.pixels
        if rgb.dtype != np.uint8 or rgb.shape != (height, width, 3):
            raise unavailable("capture", "diagnostic frame dimensions do not match verified window geometry")
        self.last_method = used
        return {"image": rgb, "coordinate_space": "window-frame-pixels", "client_bounds_verified": False,
                "window_id": before["id"], "pid": before["pid"], "bounds_points": list(before["bounds"]),
                "scale": frame.scale, "method": used}


# Apple virtual key codes are physical keys. Unicode entry below does not assume
# a US keyboard layout; game hotkeys intentionally use these physical positions.
KEYS = {
    "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5, "z": 6, "x": 7, "c": 8, "v": 9,
    "b": 11, "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17,
    "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23, "=": 24, "9": 25,
    "7": 26, "-": 27, "8": 28, "0": 29, "]": 30, "o": 31, "u": 32, "[": 33,
    "i": 34, "p": 35, "enter": 36, "l": 37, "j": 38, "'": 39, "k": 40, ";": 41,
    "\\": 42, ",": 43, "/": 44, "n": 45, "m": 46, ".": 47, "tab": 48, "space": 49,
    "grave": 50, "backspace": 51, "esc": 53, "cmd": 55, "shift": 56, "capslock": 57,
    "alt": 58, "ctrl": 59, "rshift": 60, "ralt": 61, "rctrl": 62,
    "decimal": 65, "multiply": 67, "add": 69, "numlock": 71, "divide": 75,
    "numenter": 76, "subtract": 78, "num0": 82, "num1": 83, "num2": 84,
    "num3": 85, "num4": 86, "num5": 87, "num6": 88, "num7": 89, "num8": 91, "num9": 92,
    "f1": 122, "f2": 120, "f3": 99, "f4": 118, "f5": 96, "f6": 97, "f7": 98,
    "f8": 100, "f9": 101, "f10": 109, "f11": 103, "f12": 111, "home": 115,
    "pageup": 116, "delete": 117, "end": 119, "pagedown": 121,
    "left": 123, "right": 124, "down": 125, "up": 126,
}
ALIASES = {"escape": "esc", "return": "enter", " ": "space", "`": "grave", "del": "delete",
           "lctrl": "ctrl", "lshift": "shift", "lalt": "alt", "command": "cmd", "meta": "cmd",
           "equals": "=", "minus": "-"}
MODIFIER_FLAGS = {"shift": 1 << 17, "rshift": 1 << 17, "ctrl": 1 << 18, "rctrl": 1 << 18,
                  "alt": 1 << 19, "ralt": 1 << 19, "cmd": 1 << 20}


class Input:
    def __init__(self, window, key_delay=0.04, type_delay=0.035, click_hold=0.06, settle=0.25, native=None):
        self.window = window
        self.native = native or window.native
        self.key_delay, self.type_delay = key_delay, type_delay
        self.click_hold, self.settle = click_hold, settle
        self._held_keys = set()
        self._held_buttons = set()
        self._position = None

    @property
    def flags(self):
        result = 0
        for name in self._held_keys:
            result |= MODIFIER_FLAGS.get(name, 0)
        return result

    def _ready(self):
        self.native.permission("input")
        self.window.ensure_foreground()

    def _check_focus(self):
        self.window.require()
        if not self.window.is_foreground():
            raise unavailable("input", "game lost focus during the action; input stopped")

    @staticmethod
    def _key_name(name):
        name = ALIASES.get(name.lower(), name.lower())
        if name not in KEYS:
            raise ToolError("unknown macOS key name %r" % name, {"known_keys": sorted(set(KEYS) | set(ALIASES))})
        return name

    @staticmethod
    def _button(name):
        if name not in ("left", "right", "middle"):
            raise ToolError("unknown mouse button %r" % name)
        return name

    def _move_screen(self, position):
        self._check_focus()
        button = next(iter(self._held_buttons), "left")
        self.native.post_mouse("drag" if self._held_buttons else "move", position, button, self.flags)
        self._position = position

    def move(self, x, y, glide=0.0):
        self._ready()
        destination = self.window.to_screen(x, y)
        if glide > 0:
            start = self.native.cursor()
            steps = max(6, int(glide * 60))
            for i in range(1, steps + 1):
                t = i / steps
                ease = t * t * (3 - 2 * t)
                self._move_screen(tuple(a + (b - a) * ease for a, b in zip(start, destination)))
                time.sleep(glide / steps)
        self._move_screen(destination)
        return destination

    def mouse_down(self, button="left", clicks=1):
        button = self._button(button)
        self._ready()
        self._check_focus()
        position = self.native.cursor()
        x, y = self.window.from_screen(*position)
        self.window.to_screen(x, y)  # bounds validation even for button-only input
        self.native.post_mouse("down", position, button, self.flags, clicks)
        self._held_buttons.add(button)
        self._position = position

    def mouse_up(self, button="left", clicks=1):
        button = self._button(button)
        if button not in self._held_buttons:
            return
        # Always release events we pressed, even after focus/ownership loss.
        self.native.post_mouse("up", self._position or self.native.cursor(), button, self.flags, clicks)
        self._held_buttons.discard(button)

    def click(self, x, y, button="left", clicks=1, hold=None, settle=None):
        self._button(button)
        self.move(x, y)
        for i in range(int(clicks)):
            try:
                self.mouse_down(button, i + 1)
                time.sleep(self.click_hold if hold is None else hold)
            finally:
                self.mouse_up(button, i + 1)
            if i + 1 < clicks:
                time.sleep(0.07)
        time.sleep(self.settle if settle is None else settle)

    def drag(self, x0, y0, x1, y1, button="left", seconds=0.6):
        self._button(button)
        self.window.to_screen(x1, y1)  # reject bad destination before mouse down
        self.move(x0, y0)
        try:
            self.mouse_down(button)
            steps = max(10, int(seconds * 60))
            for i in range(1, steps + 1):
                t = i / steps
                point = self.window.to_screen(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)
                self._move_screen(point)
                time.sleep(seconds / steps)
        finally:
            self.mouse_up(button)
        time.sleep(self.settle)

    def scroll(self, x, y, notches):
        self.move(x, y)
        self._check_focus()
        self.native.post_scroll(int(notches), self.flags)
        time.sleep(self.settle)

    def park(self, fx, fy):
        w, h = self.window.size()
        self.move(min(w - 1, int(w * fx)), min(h - 1, int(h * fy)))

    def key_down(self, name):
        name = self._key_name(name)
        self._ready()
        self._check_focus()
        self._held_keys.add(name)
        try:
            self.native.post_key(KEYS[name], True, self.flags)
        except Exception:
            self._held_keys.discard(name)
            raise

    def key_up(self, name):
        name = self._key_name(name)
        if name not in self._held_keys:
            return
        self._held_keys.discard(name)
        self.native.post_key(KEYS[name], False, self.flags)

    def key(self, combo, times=1, delay=None):
        parts = ["add"] if combo == "+" else [self._key_name(p.strip()) for p in combo.split("+") if p.strip()]
        if not parts:
            raise ToolError("empty key combination")
        if any(p not in MODIFIER_FLAGS for p in parts[:-1]):
            raise ToolError("only modifiers may precede the final key in a chord")
        for _ in range(max(1, int(times))):
            pressed = []
            try:
                for name in parts:
                    if name in self._held_keys:
                        raise ToolError("key is already held: %s" % name)
                    self.key_down(name)
                    pressed.append(name)
                time.sleep(0.03)
            finally:
                for name in reversed(pressed):
                    self.key_up(name)
            time.sleep(self.key_delay if delay is None else delay)

    def type_text(self, text):
        if self._held_keys:
            raise ToolError("release held keys before entering text")
        self._ready()
        for char in text:
            self._check_focus()
            self.native.post_text(char, True)
            try:
                time.sleep(self.type_delay)
            finally:
                self.native.post_text(char, False)


class Clipboard:
    def __init__(self, native=None):
        self.native = native or Native()

    def _board(self):
        _, appkit, _ = self.native._load()
        board = appkit.NSPasteboard.generalPasteboard()
        if board is None:
            raise unavailable("clipboard", "the general pasteboard is unavailable")
        return board, appkit

    def get_text(self):
        board, appkit = self._board()
        value = board.stringForType_(appkit.NSPasteboardTypeString)
        return str(value) if value is not None else None

    def set_text(self, text):
        if not isinstance(text, str):
            raise ToolError("clipboard text must be a string")
        board, appkit = self._board()
        board.clearContents()
        if not board.setString_forType_(text, appkit.NSPasteboardTypeString):
            raise unavailable("clipboard", "macOS refused to write clipboard text")

    def snapshot(self):
        board, _ = self._board()
        # Materialize each representation before mutating the pasteboard. This
        # preserves image/file-list/rich-text content as well as plain text.
        result = []
        for item in board.pasteboardItems() or []:
            data = []
            for kind in item.types():
                value = item.dataForType_(kind)
                if value is None:
                    raise unavailable("clipboard", "a promised clipboard representation cannot be preserved")
                data.append((str(kind), bytes(value)))
            result.append(data)
        return result

    def restore(self, snapshot):
        board, appkit = self._board()
        items = []
        for entries in snapshot:
            item = appkit.NSPasteboardItem.alloc().init()
            for kind, value in entries:
                if not item.setData_forType_(value, kind):
                    raise unavailable("clipboard", "could not restore a clipboard representation")
            items.append(item)
        board.clearContents()
        if items and not board.writeObjects_(items):
            raise unavailable("clipboard", "could not restore the previous clipboard contents")
