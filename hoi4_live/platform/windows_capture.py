"""Screen capture of the game client area.

HOI4 renders through a DirectX 11 flip-model swap chain, so GDI/BitBlt capture returns black. Two methods work:

* ddagrab: ffmpeg's Desktop Duplication source, one frame piped as raw BGRA (proven on this machine). It captures
  what is on the monitor, so the game must not be covered by another window.
* printwindow: PrintWindow(PW_RENDERFULLCONTENT) asks DWM for the window's own content; fast and unaffected by
  covering windows, but not every driver/compositor path returns the DirectX content. Each result is checked and
  a black frame falls back to ddagrab.

"auto" tries printwindow first when it has worked in this session, otherwise ddagrab.
"""

import ctypes
import subprocess
import time
from ctypes import wintypes

import numpy as np

from ..errors import ToolError
from . import win32


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
                ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
                ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", wintypes.LONG),
                ("biYPelsPerMeter", wintypes.LONG), ("biClrUsed", wintypes.DWORD), ("biClrImportant", wintypes.DWORD)]


gdi = win32.gdi32
gdi.CreateCompatibleDC.restype = wintypes.HDC
gdi.CreateCompatibleDC.argtypes = [wintypes.HDC]
gdi.CreateCompatibleBitmap.restype = wintypes.HBITMAP
gdi.CreateCompatibleBitmap.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int]
gdi.SelectObject.restype = wintypes.HGDIOBJ
gdi.SelectObject.argtypes = [wintypes.HDC, wintypes.HGDIOBJ]
gdi.GetDIBits.argtypes = [wintypes.HDC, wintypes.HBITMAP, wintypes.UINT, wintypes.UINT, ctypes.c_void_p,
                          ctypes.c_void_p, wintypes.UINT]
gdi.DeleteObject.argtypes = [wintypes.HGDIOBJ]
gdi.DeleteDC.argtypes = [wintypes.HDC]


def is_blank(img):
    """True for an all-black (or single-colour) frame: the signature of a failed capture."""
    if img is None or img.size == 0:
        return True
    small = img[::16, ::16]
    return float(small.std()) < 2.0 and float(small.mean()) < 8.0


class Capturer:
    def __init__(self, window, ffmpeg="ffmpeg", method="auto", output_idx="auto", timeout=20):
        self.window = window
        self.ffmpeg = ffmpeg
        self.method = method
        self.output_idx = output_idx
        self.timeout = timeout
        self.printwindow_ok = None         # learned per session
        self.last_method = None

    def grab(self, method=None):
        """RGB uint8 array of the client area, shape (h, w, 3)."""
        method = method or self.method
        hwnd = self.window.require()
        if method == "printwindow" or (method == "auto" and self.printwindow_ok is not False):
            img = self._printwindow(hwnd)
            if not is_blank(img):
                self.printwindow_ok = True
                self.last_method = "printwindow"
                return img
            self.printwindow_ok = False
            if method == "printwindow":
                raise ToolError("PrintWindow returned a blank frame for the game window")
        img = self._ddagrab(hwnd)
        self.last_method = "ddagrab"
        return img

    # ------------------------------------------------------------------ ddagrab
    def _ddagrab(self, hwnd):
        cx, cy, w, h = win32.client_rect(hwnd)
        idx, (ml, mt, mr, mb) = win32.monitor_of(hwnd)
        if self.output_idx != "auto":
            idx = int(self.output_idx)
        ox, oy = cx - ml, cy - mt
        if ox < 0 or oy < 0 or ox + w > mr - ml or oy + h > mb - mt:
            raise ToolError("the game client area is not fully on its monitor; cannot capture it with ddagrab",
                            {"client": [cx, cy, w, h], "monitor": [ml, mt, mr, mb]})
        w2, h2 = w - (w % 2), h - (h % 2)      # ddagrab wants even sizes
        src = (f"ddagrab=output_idx={idx}:framerate=10:draw_mouse=0:video_size={w2}x{h2}:"
               f"offset_x={ox}:offset_y={oy},hwdownload,format=bgra")
        cmd = [self.ffmpeg, "-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", src, "-frames:v", "1",
               "-f", "rawvideo", "-pix_fmt", "bgra", "-"]
        try:
            p = subprocess.run(cmd, capture_output=True, timeout=self.timeout, creationflags=0x08000000)
        except FileNotFoundError:
            raise ToolError(f"ffmpeg not found ('{self.ffmpeg}'); set capture.ffmpeg in config.json")
        except subprocess.TimeoutExpired:
            raise ToolError("ffmpeg ddagrab timed out")
        need = w2 * h2 * 4
        if p.returncode != 0 or len(p.stdout) < need:
            raise ToolError("ffmpeg ddagrab failed: " + p.stderr.decode("utf-8", "replace").strip()[-400:])
        a = np.frombuffer(p.stdout[:need], np.uint8).reshape(h2, w2, 4)
        rgb = a[..., 2::-1].copy()
        if (w2, h2) != (w, h):
            full = np.zeros((h, w, 3), np.uint8)
            full[:h2, :w2] = rgb
            rgb = full
        return rgb

    # ------------------------------------------------------------------ PrintWindow
    def _printwindow(self, hwnd):
        cx, cy, w, h = win32.client_rect(hwnd)
        wx, wy, ww, wh = win32.window_rect(hwnd)
        if ww <= 0 or wh <= 0:
            return None
        hdc = win32.user32.GetWindowDC(hwnd)
        mdc = gdi.CreateCompatibleDC(hdc)
        bmp = gdi.CreateCompatibleBitmap(hdc, ww, wh)
        old = gdi.SelectObject(mdc, bmp)
        try:
            if not win32.user32.PrintWindow(hwnd, mdc, 2):        # PW_RENDERFULLCONTENT
                return None
            bi = BITMAPINFOHEADER(ctypes.sizeof(BITMAPINFOHEADER), ww, -wh, 1, 32, 0, 0, 0, 0, 0, 0)
            buf = ctypes.create_string_buffer(ww * wh * 4)
            gdi.GetDIBits(mdc, bmp, 0, wh, buf, ctypes.byref(bi), 0)
            a = np.frombuffer(buf.raw, np.uint8).reshape(wh, ww, 4)
            ox, oy = cx - wx, cy - wy
            return a[oy:oy + h, ox:ox + w, 2::-1].copy()
        finally:
            gdi.SelectObject(mdc, old)
            gdi.DeleteObject(bmp)
            gdi.DeleteDC(mdc)
            win32.user32.ReleaseDC(hwnd, hdc)


def timed(fn, *a, **k):
    t = time.time()
    r = fn(*a, **k)
    return r, time.time() - t
