"""The first-run doctor: python -m hoi4_live.doctor --prefix INSTALL_FOLDER (what `hoi4-live doctor` runs).

It checks everything a first run needs on this system and prints, for each item that is not in place, the one
step that fixes it. It only reads: it changes no system setting, asks for no password, starts no game and touches
no game process. It needs nothing beyond the Python standard library.

The work is split in two so every decision is tested on every system:

    gather(prefix)      reads the facts of this computer (files, helper programs, permission queries)
    evaluate(facts)     pure: turns facts into items, each ok, fix (with its step) or note

An item belongs to the runtime it matters for: the Python server (what the coding client starts on Windows and
macOS), the native runtime (memory reads; macOS on Apple silicon and Linux x86_64), or both. The exit code is 0
when every runtime installed here has nothing to fix.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

OK, FIX, NOTE = "ok", "fix", "note"
SERVER, NATIVE, BOTH = "server", "native", "both"
VERIFIED_SIZE = (1920, 1080)
# Game versions the Python server's screen layouts were run against. It has no build check.
SERVER_TESTED = ("1.19.4",)
STEAM_PROCESSES = {"macos": {"steam_osx", "steamwebhelper"}, "linux": {"steam", "steamwebhelper", "steamcmd"}}


def item(status, title, fix=None, needs=BOTH):
    return {"status": status, "title": title, "fix": fix, "needs": needs}


# ---------------------------------------------------------------------------------------------------------------
# decisions (pure)


def check_install(facts):
    out = []
    server, native = facts["server"], facts["native"]
    if server["expected"]:
        out.append(item(OK, f"the Python server {facts['version']} is installed", needs=SERVER) if server["installed"]
                   else item(FIX, "the Python server is not installed", "hoi4-live update", SERVER))
    if native["expected"]:
        out.append(item(OK, f"the native runtime {native.get('version') or facts['version']} is installed",
                        needs=NATIVE) if native["installed"]
                   else item(FIX, "the native runtime is not installed", "hoi4-live update", NATIVE))
    elif native.get("reason"):
        out.append(item(NOTE, f"no native runtime here: {native['reason']}", needs=NATIVE))
    return out


def check_game(facts):
    game = facts["game"]
    if not game.get("exe"):
        return [item(FIX, "Hearts of Iron IV was not found in a Steam library",
                     "install the game with Steam; if it is installed somewhere else, run: "
                     "hoi4-live game-dir \"<the game's folder>\"")]
    label = game.get("version_text") or game.get("version") or "version not readable"
    return [item(OK, f"game found: {label} in {game['dir']}")]


def check_build(facts):
    """Version drift: is the installed game a build the runtimes were made for?"""
    game, native, out = facts["game"], facts["native"], []
    if not game.get("exe"):
        return out
    version = game.get("version")
    if facts["server"]["installed"] and version and not version.startswith(SERVER_TESTED):
        out.append(item(NOTE, f"the Python server has no build check; its screen layouts were run on HOI4 "
                              f"{', '.join(SERVER_TESTED)} and yours is {version}, so a panel may read differently",
                        needs=SERVER))
    if not native["installed"]:
        return out
    builds = facts.get("builds")
    entry = next((b for b in (builds or {}).get("builds", []) if b.get("system") == facts["system"]), None)
    if not entry:
        out.append(item(NOTE, "this native runtime cannot list the game builds it reads; a newer release can",
                        "hoi4-live update", NATIVE))
        return out
    wanted = f"HOI4 {entry['game_version']} on {entry['label']}"
    if game.get("sha256") == entry["sha256"]:
        out.append(item(OK, f"the game is a build the native runtime reads ({wanted})", needs=NATIVE))
    else:
        mine = game.get("version_text") or version or "another build"
        out.append(item(FIX, f"your game is {mine}; native runtime {builds.get('runtime', '')} reads only {wanted}"
                        .replace("  ", " "), builds.get("meanwhile") or "hoi4-live update", NATIVE))
    return out


def parse_native_doctor(text):
    """(ok, title, fix) of every check line `hoi4-live-native --doctor` prints, and its closing notes."""
    checks, notes = [], []
    for line in text.splitlines():
        found = re.match(r"\s+\[(ok|missing)\]\s+(.*)", line)
        step = re.match(r"\s+do this once:\s+(.*)", line)
        if found:
            checks.append([found.group(1) == "ok", found.group(2).strip(), None])
        elif step and checks:
            checks[-1][2] = step.group(1).strip()
        elif line.startswith("  ") and line.strip():
            notes.append(line.strip())
    return [tuple(c) for c in checks], notes


def check_memory(facts):
    memory = facts.get("memory")
    if not facts["native"]["installed"] or memory is None:
        return []
    if memory["code"] is None:
        return [item(FIX, f"the native runtime does not start ({memory['text'].strip()[:160]})",
                     "hoi4-live update", NATIVE)]
    if memory["code"] != 0 and "Unknown argument" in memory["text"]:
        # A runtime from before its own memory-access check (1.4.2 and earlier) cannot answer; a newer one can.
        return [item(NOTE, "this native runtime predates its memory-access check, so memory access was not checked",
                     "hoi4-live update", NATIVE)]
    checks, notes = parse_native_doctor(memory["text"])
    out = [item(OK if good else FIX, title, fix, NATIVE) for good, title, fix in checks]
    if memory["code"] != 0 and not any(i["status"] == FIX for i in out):
        head = (memory["text"].strip().splitlines() or ["memory access is not ready"])[0]
        out.append(item(FIX, head, " ".join(notes) or "run: hoi4-live-native --doctor", NATIVE))
    return out


def check_permissions(facts):
    out, system = [], facts["system"]
    if system == "macos":
        where = ("System Settings > Privacy & Security > {pane}: switch on the app you start your coding client "
                 "from (and the terminal you ran this in), then restart that app")
        ax, capture = facts.get("accessibility"), facts.get("screen_capture")
        if ax is not None:
            out.append(item(OK, "input: this app may send keys and clicks (Accessibility)") if ax
                       else item(FIX, "input: this app may not send keys and clicks yet",
                                 where.format(pane="Accessibility")))
        if capture is not None and facts["server"]["installed"]:
            out.append(item(OK, "screen capture: this app may record the screen", needs=SERVER) if capture
                       else item(FIX, "screen capture: this app may not record the screen yet",
                                 where.format(pane="Screen & System Audio Recording"), SERVER))
        if ax is not None or capture is not None:
            out.append(item(NOTE, "macOS grants these permissions per app: the answers above are for the app "
                                  "this command ran in; the app that starts the server needs them too"))
    elif system == "windows":
        out.append(item(NOTE, "input: run the game and your coding client as the same Windows user, both without "
                              "\"Run as administrator\"; Windows blocks input into a game that runs elevated",
                        needs=SERVER))
    elif system == "linux" and facts["native"]["installed"]:
        out.append(item(NOTE, "display and input are not automatic on Linux yet: the native launcher needs an X11 "
                              "display of its own with a private Xauthority file "
                              "(docs/OWNED_LINUX_LAUNCHER.md in the install's app folder)", needs=NATIVE))
    return out


def check_display(facts):
    """The game's own display settings, which the Python server plays in."""
    display = facts.get("display") or {}
    if not facts["server"]["installed"] or not any(display.values()):
        return []
    out, where = [], "in the game: Options > Graphics > {what}"
    mode, size, scale = display.get("mode"), display.get("resolution"), display.get("gui_scale")
    if mode == "fullscreen":
        out.append(item(FIX, "the game is set to exclusive fullscreen, which cannot be captured or clicked reliably",
                        where.format(what="Display mode: Borderless (or Windowed)"), SERVER))
    elif mode:
        out.append(item(OK, f"display mode: {mode.replace('_', ' ')}", needs=SERVER))
    if scale is not None:
        out.append(item(OK, "UI scaling is 1.0", needs=SERVER) if abs(scale - 1.0) < 0.005
                   else item(FIX, f"UI scaling is {scale:g}; the tools are laid out for 1.0",
                             where.format(what="UI scaling: 1.0"), SERVER))
    if size:
        text = f"{size[0]}x{size[1]}"
        if tuple(size) == VERIFIED_SIZE:
            out.append(item(OK, f"resolution: {text} (the size the tools were verified at)", needs=SERVER))
        else:
            screen = facts.get("screen")
            small = screen and (screen[0] < VERIFIED_SIZE[0] or screen[1] < VERIFIED_SIZE[1])
            out.append(item(NOTE, f"resolution: {text}. The tools were verified at 1920x1080; other sizes are scaled "
                                  "by the Python server and may misread small text"
                            + (f". This screen shows {screen[0]}x{screen[1]}, which cannot hold a 1920x1080 window"
                               if small else ""), needs=SERVER))
    return out


def check_steam(facts):
    running = facts.get("steam_running")
    if running is None or not facts["native"]["installed"]:
        return []
    if running:
        return [item(FIX, "Steam is running; the native launcher starts the game directly and refuses while Steam "
                          "is open", "quit Steam (Steam menu > Quit Steam) before a native game run", NATIVE)]
    return [item(OK, "Steam is closed (the native launcher needs it closed)", needs=NATIVE)]


def check_mods(facts):
    mods = facts.get("mods")
    if mods is None:
        return []
    if not mods:
        return [item(OK, "mods: none enabled in your playset")]
    out = []
    if facts["server"]["installed"]:
        out.append(item(NOTE, f"mods: {mods} enabled in your playset; the Python server starts the game with them",
                        needs=SERVER))
    if facts["native"]["installed"]:
        out.append(item(NOTE, f"mods: {mods} enabled in your playset; a native game run starts with an empty "
                              "playset, so they are not loaded there", needs=NATIVE))
    return out


SELFTEST_FIXES = {
    "numpy": "hoi4-live update", "PIL": "hoi4-live update", "cv2": "hoi4-live update",
    "tools": "hoi4-live update", "python": "install Python 3.9 or newer, then run the install command again",
    "user_dir": "start the game once from Steam so it creates its folder",
    "native ocr": {"windows": "add the English text recognition of Windows: Settings > Time & language > Language "
                              "& region > Add a language > English (United States), with \"Optical character "
                              "recognition\" ticked",
                   "macos": "hoi4-live update"},
}
SELFTEST_OPTIONAL = {"ffmpeg": "optional: FFmpeg makes capture faster (Windows: winget install Gyan.FFmpeg, "
                               "macOS: brew install ffmpeg)"}
SELFTEST_COVERED = ("game_dir", "saves_dir", "logs_dir", "settings.txt", "save format")


def check_selftest(facts):
    """The Python server's own environment check, with a step for each failed line."""
    out = []
    for good, name, detail in facts.get("selftest") or []:
        if good or name in SELFTEST_COVERED:
            continue
        if name in SELFTEST_OPTIONAL:
            out.append(item(NOTE, SELFTEST_OPTIONAL[name], needs=SERVER))
            continue
        fix = SELFTEST_FIXES.get(name, "hoi4-live update")
        if isinstance(fix, dict):
            fix = fix.get(facts["system"], "hoi4-live update")
        out.append(item(FIX, f"Python server check \"{name}\" failed: {detail}"[:240], fix, SERVER))
    return out


CHECKS = (check_install, check_game, check_build, check_memory, check_permissions, check_display, check_steam,
          check_mods, check_selftest)


def evaluate(facts):
    items = []
    for check in CHECKS:
        items.extend(check(facts))
    return items


def runtimes(facts, items):
    """[(name, state)] for each runtime of this system: ready, the number of steps to do, or why it is absent."""
    out = []
    for key, label in ((SERVER, "Python server"), (NATIVE, "native runtime")):
        part = facts[key]
        if not part["expected"]:
            out.append((label, None, part.get("reason") or "not used on this system"))
            continue
        steps = sum(1 for i in items if i["status"] == FIX and i["needs"] in (key, BOTH))
        out.append((label, steps, None))
    return out


def ready(facts, items):
    return all(steps == 0 for _label, steps, _why in runtimes(facts, items) if steps is not None)


def render(facts, items):
    names = {"macos": "macOS", "windows": "Windows", "linux": "Linux"}
    lines = [f"HOI4 Live Tools {facts['version']} doctor ({names.get(facts['system'], facts['system'])})", ""]
    marks = {OK: "[ok]  ", FIX: "[fix] ", NOTE: "[note]"}
    for entry in items:
        lines.append(f"  {marks[entry['status']]} {entry['title']}")
        if entry["fix"]:
            lines.append(f"          do this: {entry['fix']}")
    lines.append("")
    for label, steps, why in runtimes(facts, items):
        if steps is None:
            lines.append(f"{label}: not installed here ({why})")
        elif steps:
            lines.append(f"{label}: {steps} step{'s' if steps != 1 else ''} to do (marked [fix] above)")
        else:
            lines.append(f"{label}: ready")
    lines.append("The doctor only reads; every step above is yours to run.")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------------------------
# facts (reads only)


def system_name(platform=None):
    platform = platform or sys.platform
    return "windows" if platform == "win32" else "macos" if platform == "darwin" else "linux"


def run(args, timeout=60):
    """(exit code, output) of a helper program; (None, why) when it cannot run."""
    try:
        result = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return None, str(exc)
    return result.returncode, result.stdout.decode("utf-8", errors="replace")


def read_text(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            return f.read()
    except OSError:
        return None


def file_sha256(path):
    try:
        digest = hashlib.sha256()
        with open(path, "rb") as f:
            for block in iter(lambda: f.read(1 << 20), b""):
                digest.update(block)
        return digest.hexdigest()
    except OSError:
        return None


def read_game(game_dir, exe):
    """What the install says about itself: the launcher's version file and the executable's hash."""
    game = {"dir": game_dir, "exe": exe if exe and os.path.isfile(exe) else None, "version": None,
            "version_text": None, "sha256": None}
    if not game["exe"]:
        return game
    try:
        data = json.loads(read_text(os.path.join(game_dir, "launcher-settings.json")) or "")
        game["version"] = str(data.get("rawVersion") or "").lstrip("v") or None
        game["version_text"] = data.get("version") or None
    except ValueError:
        pass
    game["sha256"] = file_sha256(game["exe"])
    return game


def read_display(user_dir):
    """Display mode, resolution and UI scale from the game's own settings files (never written)."""
    display = {"mode": None, "resolution": None, "gui_scale": None}
    pdx = read_text(os.path.join(user_dir, "pdx_settings.txt")) or ""

    def value(key):
        found = re.search(r'"%s"\s*=\s*\{\s*value\s*=\s*"([^"]*)"' % key, pdx)
        return found.group(1) if found else None
    display["mode"] = value("display_mode")
    size = re.fullmatch(r"(\d+)x(\d+)", value("windowed_resolution" if display["mode"] == "windowed"
                                              else "fullscreen_resolution") or "")
    if size:
        display["resolution"] = (int(size.group(1)), int(size.group(2)))
    scale = re.search(r"\bgui_scale\s*=\s*([0-9.]+)", read_text(os.path.join(user_dir, "settings.txt")) or "")
    if scale:
        display["gui_scale"] = float(scale.group(1))
    return display


def read_mods(user_dir):
    try:
        data = json.loads(read_text(os.path.join(user_dir, "dlc_load.json")) or "")
        return len(data.get("enabled_mods") or [])
    except (ValueError, AttributeError):
        return None


def steam_running(system):
    names = STEAM_PROCESSES.get(system)
    if not names:
        return None
    code, text = run(["ps", "-A", "-o", "comm="], timeout=20)
    if code != 0:
        return None
    return any(os.path.basename(line.strip()) in names for line in text.splitlines())


def mac_permissions():
    """Accessibility, screen recording and the main screen's size, asked without any prompt."""
    facts = {"accessibility": None, "screen_capture": None, "screen": None}
    try:
        import ctypes
        services = ctypes.CDLL("/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices")
        services.AXIsProcessTrusted.restype = ctypes.c_bool
        facts["accessibility"] = bool(services.AXIsProcessTrusted())
        graphics = ctypes.CDLL("/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics")
        graphics.CGPreflightScreenCaptureAccess.restype = ctypes.c_bool
        facts["screen_capture"] = bool(graphics.CGPreflightScreenCaptureAccess())
        graphics.CGMainDisplayID.restype = ctypes.c_uint32
        graphics.CGDisplayPixelsWide.restype = graphics.CGDisplayPixelsHigh.restype = ctypes.c_size_t
        graphics.CGDisplayPixelsWide.argtypes = graphics.CGDisplayPixelsHigh.argtypes = [ctypes.c_uint32]
        main = graphics.CGMainDisplayID()
        facts["screen"] = (int(graphics.CGDisplayPixelsWide(main)), int(graphics.CGDisplayPixelsHigh(main)))
    except (OSError, AttributeError):
        pass
    return facts


def read_selftest(launcher):
    code, text = run([launcher, "selftest"], timeout=180)
    if code is None:
        return [(False, "tools", text)]
    found = []
    for line in text.splitlines():
        match = re.match(r"(ok  |FAIL) ([^:]+): ?(.*)", line)
        if match:
            found.append((match.group(1) != "FAIL", match.group(2).strip(), match.group(3).strip()))
    return found


def gather(prefix):
    """The facts of this computer for the install at `prefix`."""
    from . import __version__, config
    system = system_name()
    record = {}
    try:
        record = json.loads(read_text(os.path.join(prefix, "install.json")) or "") or {}
    except ValueError:
        pass
    windows = system == "windows"
    launcher = os.path.join(prefix, "app", "hoi4-live-tools" + (".cmd" if windows else ""))
    venv = os.path.join(prefix, "venv", "Scripts" if windows else "bin", "python" + (".exe" if windows else ""))
    binary = os.path.join(prefix, "native", "hoi4-live-native")
    recorded = record.get("native") or {}
    facts = {
        "system": system, "version": record.get("version") or __version__, "prefix": prefix,
        "server": {"expected": system != "linux", "installed": os.path.isfile(launcher) and os.path.isfile(venv),
                   "reason": "the Python server does not drive the game on Linux" if system == "linux" else None},
        "native": {"expected": recorded.get("status") != "unavailable" and not windows,
                   "installed": os.path.isfile(binary), "path": binary, "version": recorded.get("version"),
                   "reason": recorded.get("detail") if recorded.get("status") == "unavailable" else (
                       "the native runtime's Windows adapter is unfinished" if windows else None)},
    }
    cfg = config.load(os.path.join(prefix, "config.json") if os.path.isfile(os.path.join(prefix, "config.json"))
                      else None)
    facts["game"] = read_game(cfg.game_dir, cfg.game_exe if cfg.game_dir else None)
    facts["display"] = read_display(cfg.user_dir)
    facts["mods"] = read_mods(cfg.user_dir)
    facts["steam_running"] = steam_running(system)
    if system == "macos":
        facts.update(mac_permissions())
    if facts["native"]["installed"]:
        code, text = run([binary, "--doctor"])
        facts["memory"] = {"code": code, "text": text}
        code, text = run([binary, "--supported-builds"])
        try:
            facts["builds"] = json.loads(text) if code == 0 else None
        except ValueError:
            facts["builds"] = None
    if facts["server"]["installed"]:
        facts["selftest"] = read_selftest(launcher)
    return facts


def main(argv=None):
    parser = argparse.ArgumentParser(prog="hoi4-live doctor", description=__doc__.split("\n\n")[0])
    parser.add_argument("--prefix", required=True, help="the install folder")
    parser.add_argument("--json", action="store_true", help="print the facts and items as JSON")
    args = parser.parse_args(argv)
    facts = gather(os.path.abspath(args.prefix))
    items = evaluate(facts)
    if args.json:
        print(json.dumps({"ready": ready(facts, items), "items": items, "facts": facts}, indent=2))
    else:
        sys.stdout.write(render(facts, items))
    return 0 if ready(facts, items) else 1


if __name__ == "__main__":
    sys.exit(main())
