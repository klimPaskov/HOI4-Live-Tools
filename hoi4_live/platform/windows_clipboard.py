"""Clipboard text get/set (CF_UNICODETEXT). Console commands are pasted because typed characters get dropped while
the game is busy; the previous clipboard text is restored afterwards so the user's clipboard survives."""

import ctypes
import time
from ctypes import wintypes

CF_UNICODETEXT = 13
GMEM_MOVEABLE = 0x0002

u32 = ctypes.WinDLL("user32", use_last_error=True)
k32 = ctypes.WinDLL("kernel32", use_last_error=True)
u32.OpenClipboard.argtypes = [wintypes.HWND]
u32.OpenClipboard.restype = wintypes.BOOL
u32.GetClipboardData.argtypes = [wintypes.UINT]
u32.GetClipboardData.restype = wintypes.HANDLE
u32.SetClipboardData.argtypes = [wintypes.UINT, wintypes.HANDLE]
u32.SetClipboardData.restype = wintypes.HANDLE
u32.EmptyClipboard.restype = wintypes.BOOL
u32.CloseClipboard.restype = wintypes.BOOL
u32.IsClipboardFormatAvailable.argtypes = [wintypes.UINT]
u32.IsClipboardFormatAvailable.restype = wintypes.BOOL
k32.GlobalAlloc.argtypes = [wintypes.UINT, ctypes.c_size_t]
k32.GlobalAlloc.restype = wintypes.HGLOBAL
k32.GlobalLock.argtypes = [wintypes.HGLOBAL]
k32.GlobalLock.restype = ctypes.c_void_p
k32.GlobalUnlock.argtypes = [wintypes.HGLOBAL]
k32.GlobalFree.argtypes = [wintypes.HGLOBAL]


def _open(retries=20):
    for _ in range(retries):
        if u32.OpenClipboard(None):
            return True
        time.sleep(0.05)
    return False


def get_text():
    if not _open():
        return None
    try:
        if not u32.IsClipboardFormatAvailable(CF_UNICODETEXT):
            return None
        h = u32.GetClipboardData(CF_UNICODETEXT)
        if not h:
            return None
        p = k32.GlobalLock(h)
        try:
            return ctypes.wstring_at(p) if p else None
        finally:
            k32.GlobalUnlock(h)
    finally:
        u32.CloseClipboard()


def set_text(text):
    data = ctypes.create_unicode_buffer(text)
    size = ctypes.sizeof(data)
    if not _open():
        raise OSError("clipboard is busy")
    try:
        u32.EmptyClipboard()
        h = k32.GlobalAlloc(GMEM_MOVEABLE, size)
        p = k32.GlobalLock(h)
        ctypes.memmove(p, data, size)
        k32.GlobalUnlock(h)
        if not u32.SetClipboardData(CF_UNICODETEXT, h):
            k32.GlobalFree(h)
            raise OSError("SetClipboardData failed")
    finally:
        u32.CloseClipboard()


class Preserved:
    """with Preserved(): ...  restores the previous clipboard text on exit (text only)."""

    def __enter__(self):
        try:
            self.saved = get_text()
        except OSError:
            self.saved = None
        return self

    def __exit__(self, *exc):
        if self.saved is not None:
            try:
                set_text(self.saved)
            except OSError:
                pass
