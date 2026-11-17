"""The user's game: find its process, ask Steam to start it, and what the native runtime needs from it.

Nothing here starts the game binary, sends input or reads game memory. `launch_through_steam` hands Steam's own
URL to the system, the same as a click on Play in the Steam library, and only when the user asked for it.
"""

import ntpath
import os
import re
import subprocess
import sys

STEAM_APP_ID = 394360
STEAM_URL = "steam://rungameid/%d" % STEAM_APP_ID
# The reviewed input profile of the native runtime (windowed, 1920x1080, UI scale 1.0).
NATIVE_SIZE = (1920, 1080)
# macOS draws a title bar above a window's content: the screen needs this many points beyond the content height.
MAC_TITLE_BAR = 28


def system_name(platform=None):
    platform = platform or sys.platform
    return "windows" if platform == "win32" else "macos" if platform == "darwin" else "linux"


def _run(args):
    try:
        result = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                timeout=20)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return result.stdout.decode("utf-8", errors="replace") if result.returncode == 0 else None


def parse_ps(text):
    """[(pid, uid, executable)] from `ps -A -o pid=,uid=,comm=` (the executable may contain spaces)."""
    rows = []
    for line in (text or "").splitlines():
        found = re.match(r"\s*(\d+)\s+(\d+)\s+(.+?)\s*$", line)
        if found:
            rows.append((int(found.group(1)), int(found.group(2)), found.group(3)))
    return rows


def parse_tasklist(text):
    """[(pid, None, image name)] from `tasklist /FO CSV /NH`."""
    rows = []
    for line in (text or "").splitlines():
        cells = re.findall(r'"([^"]*)"', line)
        if len(cells) >= 2 and cells[1].isdigit():
            rows.append((int(cells[1]), None, cells[0]))
    return rows


def list_processes(system=None):
    system = system or system_name()
    if system == "windows":
        return parse_tasklist(_run(["tasklist", "/FO", "CSV", "/NH"]))
    rows = parse_ps(_run(["ps", "-A", "-o", "pid=,uid=,comm="]))
    if system == "linux":
        # comm is cut to 15 characters on Linux: take the executable from /proc where it is readable.
        full = []
        for pid, uid, name in rows:
            try:
                full.append((pid, uid, os.readlink("/proc/%d/exe" % pid)))
            except OSError:
                full.append((pid, uid, name))
        rows = full
    return rows


def _same(a, b, system):
    if system == "windows":
        return ntpath.basename(a).lower() == ntpath.basename(b).lower()
    return os.path.normpath(a) == os.path.normpath(b)


def find_games(exe, system=None, processes=None, uid=None):
    """Process IDs of the current user that run exactly the game executable `exe`."""
    system = system or system_name()
    if uid is None and hasattr(os, "getuid"):
        uid = os.getuid()
    rows = list_processes(system) if processes is None else processes
    return [pid for pid, owner, path in rows
            if _same(path, exe, system) and (uid is None or owner is None or owner == uid)]


def launch_through_steam(system=None, opener=None):
    """Ask Steam to start the game, as a click on Play in the library does. Returns the command used."""
    system = system or system_name()
    if opener is None:
        if system == "windows":
            os.startfile(STEAM_URL)  # noqa: S606 - the system's URL handler (Steam)
            return ["startfile", STEAM_URL]
        opener = subprocess.Popen
    args = ["open", STEAM_URL] if system == "macos" else ["xdg-open", STEAM_URL]
    opener(args, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return args


def display_steps(display, screen=None, system=None):
    """What to change for the native runtime's input profile: [(title, step)], [] when everything fits.

    `display` is the doctor's reading of the game's settings files: mode, resolution and gui_scale (None where the
    file does not say). `screen` is the main display's size in points (macOS) when known."""
    system = system or system_name()
    where = "in the game: Options > Graphics > %s"
    steps = []
    mode, size, scale = display.get("mode"), display.get("resolution"), display.get("gui_scale")
    if mode and mode != "windowed":
        steps.append(("the game runs %s; the native runtime plays a window" % mode.replace("_", " "),
                      where % "Display mode: Windowed"))
    if size and tuple(size) != NATIVE_SIZE:
        steps.append(("the game's resolution is %dx%d; the native runtime plays at 1920x1080" % tuple(size),
                      where % "Resolution: 1920x1080"))
    if scale is not None and abs(scale - 1.0) >= 0.005:
        steps.append(("UI scaling is %g; the native runtime reads the interface at 1.0" % scale,
                      where % "UI scaling: 1.0"))
    if screen and system == "macos":
        width, height = screen
        if width < NATIVE_SIZE[0] or height < NATIVE_SIZE[1] + MAC_TITLE_BAR:
            steps.append(("this screen shows %dx%d points; a 1920x1080 window needs at least %dx%d"
                          % (width, height, NATIVE_SIZE[0], NATIVE_SIZE[1] + MAC_TITLE_BAR),
                          "System Settings > Displays: choose \"More Space\" (or a larger resolution), or use a "
                          "larger external display"))
    return steps


def bring_to_front(pid):
    """macOS: activate the game's application (the same request a click on its Dock icon makes). True when the
    request was accepted. Needs no permission; nothing happens on other systems."""
    if system_name() != "macos":
        return False
    try:
        import ctypes
        import ctypes.util
        objc = ctypes.CDLL(ctypes.util.find_library("objc"))
        ctypes.CDLL("/System/Library/Frameworks/AppKit.framework/AppKit")
        objc.objc_getClass.restype = objc.sel_registerName.restype = ctypes.c_void_p
        objc.objc_getClass.argtypes = objc.sel_registerName.argtypes = [ctypes.c_char_p]
        lookup = ctypes.CFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_int)(
            ("objc_msgSend", objc))
        activate = ctypes.CFUNCTYPE(ctypes.c_bool, ctypes.c_void_p, ctypes.c_void_p, ctypes.c_ulong)(
            ("objc_msgSend", objc))
        app = lookup(objc.objc_getClass(b"NSRunningApplication"),
                     objc.sel_registerName(b"runningApplicationWithProcessIdentifier:"), int(pid))
        if not app:
            return False
        # NSApplicationActivateAllWindows: the game's window comes forward with it.
        return bool(activate(app, objc.sel_registerName(b"activateWithOptions:"), 1))
    except (OSError, AttributeError, TypeError):
        return False


# ---------------------------------------------------------------------------------------------------------------
# the windows on screen (macOS): reads only, no permission needed for owners, numbers and bounds

# The system's own prompts (a privacy or permission request, a password dialog). While one is on screen the native
# runtime refuses input (system_dialog_open) and only the user may answer it.
SYSTEM_DIALOG_OWNERS = ("UserNotificationCenter", "CoreServicesUIAgent", "SecurityAgent")


def mac_windows(on_screen=True):
    """[{"owner", "pid", "id", "layer", "bounds": (x, y, width, height)}] of the windows macOS lists, front to back;
    [] where the list cannot be read (another system, no CoreGraphics). Standard library only (ctypes)."""
    if system_name() != "macos":
        return []
    try:
        import ctypes
        vp = ctypes.c_void_p
        cg = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
        cf = ctypes.CDLL("/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation")
        cg.CGWindowListCopyWindowInfo.restype = vp
        cg.CGWindowListCopyWindowInfo.argtypes = [ctypes.c_uint32, ctypes.c_uint32]
        cf.CFArrayGetCount.restype = ctypes.c_long
        cf.CFArrayGetCount.argtypes = [vp]
        cf.CFArrayGetValueAtIndex.restype = vp
        cf.CFArrayGetValueAtIndex.argtypes = [vp, ctypes.c_long]
        cf.CFDictionaryGetValue.restype = vp
        cf.CFDictionaryGetValue.argtypes = [vp, vp]
        cf.CFStringGetCString.restype = ctypes.c_bool
        cf.CFStringGetCString.argtypes = [vp, ctypes.c_char_p, ctypes.c_long, ctypes.c_uint32]
        cf.CFStringCreateWithCString.restype = vp
        cf.CFStringCreateWithCString.argtypes = [vp, ctypes.c_char_p, ctypes.c_uint32]
        cf.CFNumberGetValue.restype = ctypes.c_bool
        cf.CFNumberGetValue.argtypes = [vp, ctypes.c_long, vp]
        cf.CFRelease.argtypes = [vp]
        utf8, double = 0x08000100, 13

        def text(ref):
            buf = ctypes.create_string_buffer(512)
            return buf.value.decode("utf-8", "replace") if ref and cf.CFStringGetCString(ref, buf, 512, utf8) else None

        def number(ref):
            out = ctypes.c_double()
            return out.value if ref and cf.CFNumberGetValue(ref, double, ctypes.byref(out)) else None

        keys = {n: vp.in_dll(cg, "kCGWindow" + n).value for n in ("OwnerName", "OwnerPID", "Number", "Layer", "Bounds")}
        corner = {n: cf.CFStringCreateWithCString(None, n.encode(), utf8) for n in ("X", "Y", "Width", "Height")}
        # kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements
        array = cg.CGWindowListCopyWindowInfo((1 if on_screen else 0) | 16, 0)
        rows = []
        try:
            for i in range(cf.CFArrayGetCount(array) if array else 0):
                d = cf.CFArrayGetValueAtIndex(array, i)
                b = cf.CFDictionaryGetValue(d, keys["Bounds"])
                bounds = tuple(number(cf.CFDictionaryGetValue(b, corner[n])) if b else None
                               for n in ("X", "Y", "Width", "Height"))
                rows.append({"owner": text(cf.CFDictionaryGetValue(d, keys["OwnerName"])),
                             "pid": int(number(cf.CFDictionaryGetValue(d, keys["OwnerPID"])) or 0),
                             "id": int(number(cf.CFDictionaryGetValue(d, keys["Number"])) or 0),
                             "layer": int(number(cf.CFDictionaryGetValue(d, keys["Layer"])) or 0),
                             "bounds": None if None in bounds else bounds})
        finally:
            if array:
                cf.CFRelease(array)
            for ref in corner.values():
                if ref:
                    cf.CFRelease(ref)
        return rows
    except (OSError, AttributeError, TypeError, ValueError):
        return []


def system_dialogs(windows=None):
    """The owners of the system prompts on screen (sorted, each once); [] when none or unreadable."""
    rows = mac_windows() if windows is None else windows
    return sorted({w["owner"] for w in rows if w.get("owner") in SYSTEM_DIALOG_OWNERS})


def game_window(pid, windows=None):
    """The game's main window ({"id", "bounds"} of its largest normal-layer window), None when not on screen."""
    rows = mac_windows() if windows is None else windows
    mine = [w for w in rows if w.get("pid") == pid and w.get("layer") == 0 and w.get("bounds")]
    if not mine:
        return None
    best = max(mine, key=lambda w: w["bounds"][2] * w["bounds"][3])
    return {"id": best["id"], "bounds": best["bounds"]}
