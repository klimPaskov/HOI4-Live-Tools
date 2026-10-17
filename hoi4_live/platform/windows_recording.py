"""Windows recorder metadata checks; no VM_READ or process mutation."""
import ntpath


def inspect_owned_window(identity, hwnd, process_api=None, window_api=None):
    from ..memory.winproc import capture_launched_identity
    if type(hwnd) is not int or hwnd <= 0:
        raise ValueError("An explicit owned window handle is required")
    actual = capture_launched_identity(identity.pid, identity.executable_path, api=process_api)
    if (actual.pid != identity.pid or actual.creation_time != identity.creation_time
            or ntpath.normcase(ntpath.normpath(actual.executable_path)) !=
            ntpath.normcase(ntpath.normpath(identity.executable_path))):
        raise RuntimeError("Owned game identity changed or PID was reused")
    if window_api is None:
        from . import win32 as window_api
    window_api.set_dpi_awareness()
    if (not window_api.user32.IsWindow(hwnd) or not window_api.user32.IsWindowVisible(hwnd)
            or window_api.user32.IsIconic(hwnd) or window_api.window_pid(hwnd) != identity.pid):
        raise RuntimeError("Window is not the visible client of the owned game")
    return tuple(window_api.client_rect(hwnd))
