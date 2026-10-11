"""Synthetic input through SendInput: absolute mouse moves, buttons, wheel and scan-code keys.

HOI4 reads hardware-like scan codes, so keys go out with KEYEVENTF_SCANCODE (extended flag where needed). Every
public function takes client coordinates through the GameWindow, which refuses points outside the client area and
makes sure the game is the foreground window first.
"""

import ctypes
import math
import time
from ctypes import wintypes

from ..errors import ToolError
from . import win32

INPUT_MOUSE, INPUT_KEYBOARD = 0, 1
MOUSEEVENTF_MOVE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_VIRTUALDESK = 0x0001, 0x8000, 0x4000
MOUSEEVENTF_WHEEL, MOUSEEVENTF_HWHEEL = 0x0800, 0x1000
BUTTONS = {"left": (0x0002, 0x0004), "right": (0x0008, 0x0010), "middle": (0x0020, 0x0040)}
KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE = 0x0001, 0x0002, 0x0008

ULONG_PTR = ctypes.c_size_t


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wintypes.LONG), ("dy", wintypes.LONG), ("mouseData", wintypes.DWORD),
                ("dwFlags", wintypes.DWORD), ("time", wintypes.DWORD), ("dwExtraInfo", ULONG_PTR)]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD), ("dwFlags", wintypes.DWORD),
                ("time", wintypes.DWORD), ("dwExtraInfo", ULONG_PTR)]


class HARDWAREINPUT(ctypes.Structure):
    _fields_ = [("uMsg", wintypes.DWORD), ("wParamL", wintypes.WORD), ("wParamH", wintypes.WORD)]


class _U(ctypes.Union):
    _fields_ = [("mi", MOUSEINPUT), ("ki", KEYBDINPUT), ("hi", HARDWAREINPUT)]


class INPUT(ctypes.Structure):
    _fields_ = [("type", wintypes.DWORD), ("u", _U)]


win32.user32.SendInput.restype = wintypes.UINT
win32.user32.SendInput.argtypes = [wintypes.UINT, ctypes.POINTER(INPUT), ctypes.c_int]

# scan code, extended
KEYS = {
    "esc": (0x01, False), "escape": (0x01, False), "1": (0x02, False), "2": (0x03, False), "3": (0x04, False),
    "4": (0x05, False), "5": (0x06, False), "6": (0x07, False), "7": (0x08, False), "8": (0x09, False),
    "9": (0x0A, False), "0": (0x0B, False), "-": (0x0C, False), "minus": (0x0C, False), "=": (0x0D, False),
    "equals": (0x0D, False), "backspace": (0x0E, False), "tab": (0x0F, False),
    "q": (0x10, False), "w": (0x11, False), "e": (0x12, False), "r": (0x13, False), "t": (0x14, False),
    "y": (0x15, False), "u": (0x16, False), "i": (0x17, False), "o": (0x18, False), "p": (0x19, False),
    "[": (0x1A, False), "]": (0x1B, False), "enter": (0x1C, False), "return": (0x1C, False),
    "ctrl": (0x1D, False), "lctrl": (0x1D, False), "a": (0x1E, False), "s": (0x1F, False), "d": (0x20, False),
    "f": (0x21, False), "g": (0x22, False), "h": (0x23, False), "j": (0x24, False), "k": (0x25, False),
    "l": (0x26, False), ";": (0x27, False), "'": (0x28, False), "`": (0x29, False), "grave": (0x29, False),
    "shift": (0x2A, False), "lshift": (0x2A, False), "\\": (0x2B, False), "z": (0x2C, False), "x": (0x2D, False),
    "c": (0x2E, False), "v": (0x2F, False), "b": (0x30, False), "n": (0x31, False), "m": (0x32, False),
    ",": (0x33, False), ".": (0x34, False), "/": (0x35, False), "rshift": (0x36, False),
    "multiply": (0x37, False), "alt": (0x38, False), "lalt": (0x38, False), "space": (0x39, False),
    " ": (0x39, False), "capslock": (0x3A, False),
    "f1": (0x3B, False), "f2": (0x3C, False), "f3": (0x3D, False), "f4": (0x3E, False), "f5": (0x3F, False),
    "f6": (0x40, False), "f7": (0x41, False), "f8": (0x42, False), "f9": (0x43, False), "f10": (0x44, False),
    "f11": (0x57, False), "f12": (0x58, False),
    "numlock": (0x45, False), "scrolllock": (0x46, False),
    "num7": (0x47, False), "num8": (0x48, False), "num9": (0x49, False), "subtract": (0x4A, False),
    "num4": (0x4B, False), "num5": (0x4C, False), "num6": (0x4D, False), "add": (0x4E, False),
    "num1": (0x4F, False), "num2": (0x50, False), "num3": (0x51, False), "num0": (0x52, False),
    "decimal": (0x53, False), "numenter": (0x1C, True), "divide": (0x35, True),
    "rctrl": (0x1D, True), "ralt": (0x38, True),
    "home": (0x47, True), "up": (0x48, True), "pageup": (0x49, True), "left": (0x4B, True), "right": (0x4D, True),
    "end": (0x4F, True), "down": (0x50, True), "pagedown": (0x51, True), "insert": (0x52, True),
    "delete": (0x53, True), "del": (0x53, True),
}
SHIFTED = {"!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7", "*": "8", "(": "9", ")": "0",
           "_": "-", "+": "=", "{": "[", "}": "]", ":": ";", '"': "'", "~": "`", "<": ",", ">": ".", "?": "/",
           "|": "\\"}
MODIFIERS = {"ctrl", "lctrl", "rctrl", "shift", "lshift", "rshift", "alt", "lalt", "ralt"}


def _send(*inputs):
    arr = (INPUT * len(inputs))(*inputs)
    n = win32.user32.SendInput(len(inputs), arr, ctypes.sizeof(INPUT))
    if n != len(inputs):
        raise ToolError("SendInput was blocked (a higher-integrity window may have focus, or the desktop is locked)")


def _mouse(flags, dx=0, dy=0, data=0):
    i = INPUT(type=INPUT_MOUSE)
    i.u.mi = MOUSEINPUT(dx, dy, data & 0xFFFFFFFF, flags, 0, 0)
    return i


def _key(name, up=False):
    if name not in KEYS:
        raise ToolError(f"unknown key name '{name}'", {"known_keys": sorted(KEYS)})
    sc, ext = KEYS[name]
    flags = KEYEVENTF_SCANCODE | (KEYEVENTF_KEYUP if up else 0) | (KEYEVENTF_EXTENDEDKEY if ext else 0)
    i = INPUT(type=INPUT_KEYBOARD)
    i.u.ki = KEYBDINPUT(0, sc, flags, 0, 0)
    return i


def _virtual_screen():
    g = win32.user32.GetSystemMetrics
    return g(76), g(77), g(78), g(79)        # SM_XVIRTUALSCREEN, Y, CX, CY


def move_screen(sx, sy):
    vx, vy, vw, vh = _virtual_screen()
    nx = int(round((sx - vx) * 65535 / max(1, vw - 1)))
    ny = int(round((sy - vy) * 65535 / max(1, vh - 1)))
    _send(_mouse(MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK, nx, ny))
    if win32.cursor_pos() != (sx, sy):
        win32.user32.SetCursorPos(int(sx), int(sy))


class Input:
    def __init__(self, window, key_delay=0.04, type_delay=0.035, click_hold=0.06, settle=0.25):
        self.window = window
        self.key_delay = key_delay
        self.type_delay = type_delay
        self.click_hold = click_hold
        self.settle = settle

    def _ready(self):
        self.window.ensure_foreground()

    # ------------------------------------------------------------------ mouse
    def move(self, x, y, glide=0.0):
        """Move to client (x, y). glide > 0 moves along an eased path over that many seconds."""
        self._ready()
        sx, sy = self.window.to_screen(x, y)
        if glide > 0:
            x0, y0 = win32.cursor_pos()
            steps = max(6, int(glide * 90))
            t0 = time.perf_counter()
            for i in range(1, steps + 1):
                t = i / steps
                e = t * t * (3 - 2 * t)
                move_screen(int(x0 + (sx - x0) * e), int(y0 + (sy - y0) * e))
                delay = t0 + glide * t - time.perf_counter()
                if delay > 0:
                    time.sleep(delay)
        move_screen(sx, sy)
        return sx, sy

    def click(self, x, y, button="left", clicks=1, hold=None, settle=None):
        if button not in BUTTONS:
            raise ToolError(f"unknown mouse button '{button}' (left, right, middle)")
        self.move(x, y)
        time.sleep(0.05)
        down, up = BUTTONS[button]
        for i in range(clicks):
            _send(_mouse(down))
            time.sleep(self.click_hold if hold is None else hold)
            _send(_mouse(up))
            if i + 1 < clicks:
                time.sleep(0.07)
        time.sleep(self.settle if settle is None else settle)

    def mouse_down(self, button="left"):
        _send(_mouse(BUTTONS[button][0]))

    def mouse_up(self, button="left"):
        _send(_mouse(BUTTONS[button][1]))

    def drag(self, x0, y0, x1, y1, button="left", seconds=0.6):
        self.move(x0, y0)
        time.sleep(0.05)
        self.mouse_down(button)
        try:
            steps = max(10, int(seconds * 60))
            for i in range(1, steps + 1):
                t = i / steps
                sx, sy = self.window.to_screen(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, clamp=True)
                move_screen(sx, sy)
                time.sleep(seconds / steps)
        finally:
            self.mouse_up(button)
        time.sleep(self.settle)

    def scroll(self, x, y, notches):
        """Wheel at client (x, y): positive notches scroll up / zoom in, negative down / out."""
        self.move(x, y)
        for _ in range(abs(int(notches))):
            _send(_mouse(MOUSEEVENTF_WHEEL, data=120 if notches > 0 else -120))
            time.sleep(0.04)
        time.sleep(self.settle)

    def park(self, fx, fy):
        """Move the cursor to a neutral spot (fractions of the client size) so no tooltip covers the screen."""
        w, h = self.window.size()
        sx, sy = self.window.to_screen(min(w - 1, int(w * fx)), min(h - 1, int(h * fy)))
        move_screen(sx, sy)

    # ------------------------------------------------------------------ keyboard
    def key(self, combo, times=1, delay=None):
        """Press a key or chord: 'esc', 'space', 'ctrl+v', 'shift+f5'. delay overrides the pause between presses."""
        parts = [p.strip().lower() for p in combo.split("+") if p.strip()] if combo != "+" else ["add"]
        if not parts:
            raise ToolError("empty key combination")
        for p in parts:
            if p not in KEYS:
                raise ToolError(f"unknown key name '{p}'", {"known_keys": sorted(KEYS)})
        self._ready()
        mods, last = parts[:-1], parts[-1]
        for _ in range(max(1, int(times))):
            for m in mods:
                _send(_key(m))
                time.sleep(0.03)
            _send(_key(last))
            time.sleep(0.03 if delay is None else min(0.03, delay))
            _send(_key(last, up=True))
            for m in reversed(mods):
                time.sleep(0.02)
                _send(_key(m, up=True))
            time.sleep(self.key_delay if delay is None else delay)

    def key_down(self, name):
        _send(_key(name))

    def key_up(self, name):
        _send(_key(name, up=True))

    def type_text(self, text):
        """Type with scan codes (US layout). Unsupported characters raise before anything is typed."""
        plan = []
        for ch in text:
            if ch == " ":
                plan.append(("space", False))
            elif ch == "\n":
                plan.append(("enter", False))
            elif ch in SHIFTED:
                plan.append((SHIFTED[ch], True))
            elif ch.isalpha() and ch.isascii():
                plan.append((ch.lower(), ch.isupper()))
            elif ch in KEYS:
                plan.append((ch, False))
            else:
                raise ToolError(f"cannot type character {ch!r} with scan codes; use paste=true")
        self._ready()
        for name, shift in plan:
            if shift:
                _send(_key("shift"))
            _send(_key(name))
            _send(_key(name, up=True))
            if shift:
                _send(_key("shift", up=True))
            time.sleep(self.type_delay)


def distance(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])
