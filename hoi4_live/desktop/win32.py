"""Thin ctypes wrappers over user32/kernel32 (64-bit safe: every handle-taking function gets argtypes)."""

import ctypes
import time
from ctypes import wintypes

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)

HWND = wintypes.HWND
WNDENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, HWND, wintypes.LPARAM)
MONITORENUMPROC = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HMONITOR, wintypes.HDC, ctypes.POINTER(wintypes.RECT),
                                     wintypes.LPARAM)


def _sig(fn, res, *args):
    fn.restype = res
    fn.argtypes = list(args)


_sig(user32.EnumWindows, wintypes.BOOL, WNDENUMPROC, wintypes.LPARAM)
_sig(user32.GetWindowTextLengthW, ctypes.c_int, HWND)
_sig(user32.GetWindowTextW, ctypes.c_int, HWND, wintypes.LPWSTR, ctypes.c_int)
_sig(user32.GetClassNameW, ctypes.c_int, HWND, wintypes.LPWSTR, ctypes.c_int)
_sig(user32.IsWindowVisible, wintypes.BOOL, HWND)
_sig(user32.IsIconic, wintypes.BOOL, HWND)
_sig(user32.IsWindow, wintypes.BOOL, HWND)
_sig(user32.GetWindowThreadProcessId, wintypes.DWORD, HWND, ctypes.POINTER(wintypes.DWORD))
_sig(user32.GetClientRect, wintypes.BOOL, HWND, ctypes.POINTER(wintypes.RECT))
_sig(user32.GetWindowRect, wintypes.BOOL, HWND, ctypes.POINTER(wintypes.RECT))
_sig(user32.ClientToScreen, wintypes.BOOL, HWND, ctypes.POINTER(wintypes.POINT))
_sig(user32.GetForegroundWindow, HWND)
_sig(user32.SetForegroundWindow, wintypes.BOOL, HWND)
_sig(user32.BringWindowToTop, wintypes.BOOL, HWND)
_sig(user32.SetFocus, HWND, HWND)
_sig(user32.ShowWindow, wintypes.BOOL, HWND, ctypes.c_int)
_sig(user32.SetWindowPos, wintypes.BOOL, HWND, HWND, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wintypes.UINT)
_sig(user32.AttachThreadInput, wintypes.BOOL, wintypes.DWORD, wintypes.DWORD, wintypes.BOOL)
_sig(user32.PostMessageW, wintypes.BOOL, HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM)
_sig(user32.MonitorFromWindow, wintypes.HMONITOR, HWND, wintypes.DWORD)
_sig(user32.EnumDisplayMonitors, wintypes.BOOL, wintypes.HDC, ctypes.POINTER(wintypes.RECT), MONITORENUMPROC, wintypes.LPARAM)
_sig(user32.GetCursorPos, wintypes.BOOL, ctypes.POINTER(wintypes.POINT))
_sig(user32.SetCursorPos, wintypes.BOOL, ctypes.c_int, ctypes.c_int)
_sig(user32.GetSystemMetrics, ctypes.c_int, ctypes.c_int)
_sig(user32.GetWindowDC, wintypes.HDC, HWND)
_sig(user32.ReleaseDC, ctypes.c_int, HWND, wintypes.HDC)
_sig(user32.PrintWindow, wintypes.BOOL, HWND, wintypes.HDC, wintypes.UINT)
_sig(kernel32.OpenProcess, wintypes.HANDLE, wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
_sig(kernel32.CloseHandle, wintypes.BOOL, wintypes.HANDLE)
_sig(kernel32.GetExitCodeProcess, wintypes.BOOL, wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD))
_sig(kernel32.QueryFullProcessImageNameW, wintypes.BOOL, wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR,
     ctypes.POINTER(wintypes.DWORD))
_sig(kernel32.GetCurrentThreadId, wintypes.DWORD)
_sig(kernel32.TerminateProcess, wintypes.BOOL, wintypes.HANDLE, wintypes.UINT)
_sig(kernel32.CreateToolhelp32Snapshot, wintypes.HANDLE, wintypes.DWORD, wintypes.DWORD)

PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
PROCESS_TERMINATE = 0x0001
STILL_ACTIVE = 259
WM_CLOSE = 0x0010
SW_RESTORE = 9
SW_MINIMIZE = 6
SWP_NOSIZE, SWP_NOZORDER, SWP_NOACTIVATE = 0x0001, 0x0004, 0x0010
MONITOR_DEFAULTTONEAREST = 2


class MONITORINFO(ctypes.Structure):
    _fields_ = [("cbSize", wintypes.DWORD), ("rcMonitor", wintypes.RECT), ("rcWork", wintypes.RECT),
                ("dwFlags", wintypes.DWORD)]


_sig(user32.GetMonitorInfoW, wintypes.BOOL, wintypes.HMONITOR, ctypes.POINTER(MONITORINFO))

_dpi_done = False


def set_dpi_awareness():
    """Physical pixels everywhere (per-monitor aware v2, falling back to system aware)."""
    global _dpi_done
    if _dpi_done:
        return
    _dpi_done = True
    try:
        if user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4)):
            return
    except AttributeError:
        pass
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
        return
    except (AttributeError, OSError):
        pass
    try:
        user32.SetProcessDPIAware()
    except AttributeError:
        pass


def window_text(hwnd):
    n = user32.GetWindowTextLengthW(hwnd)
    buf = ctypes.create_unicode_buffer(n + 1)
    user32.GetWindowTextW(hwnd, buf, n + 1)
    return buf.value


def class_name(hwnd):
    buf = ctypes.create_unicode_buffer(256)
    user32.GetClassNameW(hwnd, buf, 256)
    return buf.value


def window_pid(hwnd):
    pid = wintypes.DWORD()
    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    return pid.value


def process_image(pid):
    h = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not h:
        return None
    try:
        size = wintypes.DWORD(1024)
        buf = ctypes.create_unicode_buffer(1024)
        if kernel32.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(size)):
            return buf.value
        return None
    finally:
        kernel32.CloseHandle(h)


def pid_alive(pid):
    if not pid:
        return False
    h = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, int(pid))
    if not h:
        return ctypes.get_last_error() == 5      # access denied: exists but not ours
    try:
        code = wintypes.DWORD()
        if kernel32.GetExitCodeProcess(h, ctypes.byref(code)):
            return code.value == STILL_ACTIVE
        return False
    finally:
        kernel32.CloseHandle(h)


def terminate(pid):
    h = kernel32.OpenProcess(PROCESS_TERMINATE, False, int(pid))
    if not h:
        return False
    try:
        return bool(kernel32.TerminateProcess(h, 1))
    finally:
        kernel32.CloseHandle(h)


class PROCESSENTRY32W(ctypes.Structure):
    _fields_ = [("dwSize", wintypes.DWORD), ("cntUsage", wintypes.DWORD), ("th32ProcessID", wintypes.DWORD),
                ("th32DefaultHeapID", ctypes.c_void_p), ("th32ModuleID", wintypes.DWORD),
                ("cntThreads", wintypes.DWORD), ("th32ParentProcessID", wintypes.DWORD),
                ("pcPriClassBase", ctypes.c_long), ("dwFlags", wintypes.DWORD), ("szExeFile", ctypes.c_wchar * 260)]


_sig(kernel32.Process32FirstW, wintypes.BOOL, wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W))
_sig(kernel32.Process32NextW, wintypes.BOOL, wintypes.HANDLE, ctypes.POINTER(PROCESSENTRY32W))


def find_processes(exe_name):
    """PIDs of running processes with this image name (Toolhelp snapshot; tasklist can hang under load)."""
    snap = kernel32.CreateToolhelp32Snapshot(2, 0)
    out = []
    if not snap or snap == wintypes.HANDLE(-1).value:
        return out
    try:
        e = PROCESSENTRY32W()
        e.dwSize = ctypes.sizeof(PROCESSENTRY32W)
        ok = kernel32.Process32FirstW(snap, ctypes.byref(e))
        while ok:
            if e.szExeFile.lower() == exe_name.lower():
                out.append(e.th32ProcessID)
            ok = kernel32.Process32NextW(snap, ctypes.byref(e))
    finally:
        kernel32.CloseHandle(snap)
    return out


def enum_windows(visible_only=True):
    out = []

    def cb(hwnd, _):
        if not visible_only or user32.IsWindowVisible(hwnd):
            out.append(hwnd)
        return True
    user32.EnumWindows(WNDENUMPROC(cb), 0)
    return out


def client_rect(hwnd):
    """(x, y, w, h) of the client area in screen pixels."""
    pt = wintypes.POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(pt))
    rc = wintypes.RECT()
    user32.GetClientRect(hwnd, ctypes.byref(rc))
    return pt.x, pt.y, rc.right - rc.left, rc.bottom - rc.top


def window_rect(hwnd):
    rc = wintypes.RECT()
    user32.GetWindowRect(hwnd, ctypes.byref(rc))
    return rc.left, rc.top, rc.right - rc.left, rc.bottom - rc.top


def foreground():
    return user32.GetForegroundWindow()


def monitor_of(hwnd):
    """(index, (left, top, right, bottom)) of the monitor holding most of the window; index follows
    EnumDisplayMonitors order, which matches DXGI output order on single-adapter systems."""
    target = user32.MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)
    mons = []

    def cb(hmon, hdc, lprc, data):
        mons.append(hmon)
        return True
    user32.EnumDisplayMonitors(None, None, MONITORENUMPROC(cb), 0)
    info = MONITORINFO()
    info.cbSize = ctypes.sizeof(MONITORINFO)
    user32.GetMonitorInfoW(target, ctypes.byref(info))
    r = info.rcMonitor
    idx = 0
    for i, m in enumerate(mons):
        if m == target:
            idx = i
    return idx, (r.left, r.top, r.right, r.bottom)


def focus(hwnd, attempts=3):
    """Bring a window to the foreground. Windows only lets the foreground owner hand focus over, so attach to the
    foreground thread first; a synthetic Alt tap unlocks SetForegroundWindow when that is not enough."""
    for i in range(attempts):
        if user32.IsIconic(hwnd):
            user32.ShowWindow(hwnd, SW_RESTORE)
        fg = user32.GetForegroundWindow()
        if fg == hwnd:
            return True
        t_fg = user32.GetWindowThreadProcessId(fg, None)
        t_me = kernel32.GetCurrentThreadId()
        attached = bool(t_fg and t_fg != t_me and user32.AttachThreadInput(t_me, t_fg, True))
        try:
            if i > 0:
                user32.keybd_event(0x12, 0, 0, 0)            # Alt down/up: grants foreground rights
                user32.keybd_event(0x12, 0, 2, 0)
            user32.BringWindowToTop(hwnd)
            user32.SetForegroundWindow(hwnd)
            user32.SetFocus(hwnd)
        finally:
            if attached:
                user32.AttachThreadInput(t_me, t_fg, False)
        time.sleep(0.25)
        if user32.GetForegroundWindow() == hwnd:
            return True
    return user32.GetForegroundWindow() == hwnd


def move_window(hwnd, x, y):
    return bool(user32.SetWindowPos(hwnd, None, int(x), int(y), 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE))


def post_close(hwnd):
    return bool(user32.PostMessageW(hwnd, WM_CLOSE, 0, 0))


def cursor_pos():
    pt = wintypes.POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    return pt.x, pt.y
