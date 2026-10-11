"""Windows backend. Paths and processes live here; window/capture/input still sit in hoi4_live/desktop (win32.py)."""

import os

from .base import Platform


class WindowsPlatform(Platform):
    name = "windows"

    @property
    def game_process_name(self):
        return "hoi4.exe"

    def documents_dir(self):
        """Follows OneDrive redirection through SHGetKnownFolderPath."""
        import ctypes
        import uuid
        from ctypes import wintypes

        class GUID(ctypes.Structure):
            _fields_ = [("Data1", wintypes.DWORD), ("Data2", wintypes.WORD), ("Data3", wintypes.WORD),
                        ("Data4", wintypes.BYTE * 8)]
        u = uuid.UUID("{FDD39AD0-238F-46AF-ADB4-6C85480369C7}")   # FOLDERID_Documents
        guid = GUID(u.fields[0], u.fields[1], u.fields[2], (wintypes.BYTE * 8)(*u.bytes[8:]))
        out = ctypes.c_wchar_p()
        try:
            if ctypes.windll.shell32.SHGetKnownFolderPath(ctypes.byref(guid), 0, None, ctypes.byref(out)) == 0:
                path = out.value
                ctypes.windll.ole32.CoTaskMemFree(out)
                return path
        except (AttributeError, OSError):
            pass
        return os.path.join(os.path.expanduser("~"), "Documents")

    def game_dir(self):
        """The HOI4 install through the Steam registry key and libraryfolders.vdf."""
        import re
        import winreg
        roots = []
        for hive, key, name in ((winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
                                (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath")):
            try:
                with winreg.OpenKey(hive, key) as k:
                    roots.append(winreg.QueryValueEx(k, name)[0])
            except OSError:
                pass
        libs = []
        for r in roots:
            libs.append(r)
            vdf = os.path.join(r, "steamapps", "libraryfolders.vdf")
            if os.path.exists(vdf):
                with open(vdf, encoding="utf-8") as f:
                    text = f.read()
                libs += [p.replace("\\\\", "\\") for p in re.findall(r'"path"\s+"([^"]+)"', text)]
        libs.append(r"C:\Program Files (x86)\Steam")
        for lib in libs:
            p = os.path.join(lib, "steamapps", "common", "Hearts of Iron IV")
            if os.path.exists(os.path.join(p, "hoi4.exe")):
                return os.path.normpath(p)
        return None

    def game_executable(self, game_dir):
        exe = os.path.join(game_dir, "hoi4.exe") if game_dir else None
        return exe if exe and os.path.exists(exe) else None

    def local_data_dir(self):
        return os.environ.get("LOCALAPPDATA") or self.temp_dir()

    def temp_dir(self):
        return os.environ.get("TEMP") or super().temp_dir()

    def find_processes(self, name):
        from ..desktop import win32
        return win32.find_processes(name)

    def pid_alive(self, pid):
        from ..desktop import win32
        return win32.pid_alive(pid)

    def terminate(self, pid):
        from ..desktop import win32
        return win32.terminate(pid)

    def process_image(self, pid):
        from ..desktop import win32
        return win32.process_image(pid)
