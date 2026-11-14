#!/usr/bin/env python3
"""Install, update, check, register and remove HOI4 Live Tools for the current user (Windows, macOS, Linux).

    python install.py                       install (or upgrade) and register for the clients found
    python install.py install --register claude,codex
    python install.py install --register none
    python install.py update [--version X.Y.Z]      fetch a release and install it over the current one
    python install.py doctor                check what a first run needs and print the fix for each failed item
    python install.py status
    python install.py register [--clients ...]      (re)write the client registrations only
    python install.py unregister
    python install.py uninstall [--purge]

It needs only Python 3.9+. Run from an unpacked release or a checkout it installs that copy; run on its own (the
one-line installers install.sh and install.ps1 do this) it downloads the latest release, or the one named by
--version, and checks every download against the release's SHA256SUMS.txt before using it. The first install also
needs PyPI for the Python server's dependencies (numpy, Pillow, OpenCV, and PyObjC on macOS).

What is installed where a build exists for the computer:

    Python server    Windows and macOS: the MCP server the clients start
    native runtime   macOS on Apple silicon and Linux x86_64: hoi4-live-native, the memory-reading runtime;
                     on macOS its signature and debugger entitlement are verified, and it is signed when missing

Layout of an install (default prefix %LOCALAPPDATA%\\hoi4-live-tools on Windows,
~/Library/Application Support/hoi4-live-tools on macOS, ~/.local/share/hoi4-live-tools on Linux):

    hoi4-live      the command: doctor, update, uninstall, status (hoi4-live.cmd on Windows)
    app\\            the program (hoi4-live-tools.cmd on Windows, hoi4-live-tools elsewhere: what clients start)
    native\\         hoi4-live-native and its entitlements file
    venv\\           the private Python environment with the dependencies
    config.json    optional user settings (the launcher passes it as HOI4_LIVE_CONFIG when it exists)
    artifacts\\     screenshots, scenario reports, the server log, journals, state caches
    install.json   version, paths, registrations and links of this install

Upgrading replaces app\\ and native\\ and keeps venv\\ (dependencies are re-resolved), config.json, artifacts\\ and
the client registrations (the path does not change). The installer never touches the game, its settings or its
playset, and never changes a system security setting. Client registrations are per user, written atomically after
one backup (<file>.hoi4live.bak):

    claude    ~/.claude.json (or $CLAUDE_CONFIG_DIR/.claude.json) mcpServers
    codex     $CODEX_HOME/config.toml or ~/.codex/config.toml, one managed block
    cursor    ~/.cursor/mcp.json
    opencode  ~/.config/opencode/opencode.json

When a folder of the user's PATH is available (~/.local/bin, or %LOCALAPPDATA%\\Microsoft\\WindowsApps) the command
is linked there as `hoi4-live`; uninstall removes the link. HOI4_LIVE_RELEASE_BASE names another release location:
an https address shaped like the GitHub releases page, or a local folder holding the release files.

Exit codes: 0 done, 1 install failed, 2 usage or unsupported platform, 3 installed but verification failed,
4 installed and verified but a client registration failed or conflicts, 5 installed but the native runtime could
not be installed.
"""

from __future__ import annotations

import argparse
import datetime
import hashlib
import io
import json
import os
import platform as host_platform
import queue
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import threading
import urllib.error
import urllib.request
import zipfile

SERVER_ID = "hoi4-live"
APP_DIR_NAME = "hoi4-live-tools"
WINDOWS_LAUNCHER = "hoi4-live-tools.cmd"
POSIX_LAUNCHER = "hoi4-live-tools"
LAUNCHERS = (WINDOWS_LAUNCHER, POSIX_LAUNCHER)
SUPPORTED_PLATFORMS = ("win32", "darwin", "linux")
# Where the Python server runs. On Linux the native runtime is the game adapter and the server refuses.
SERVER_PLATFORMS = ("win32", "darwin")
REPO = "klimPaskov/HOI4-Live-Tools"
SUMS_NAME = "SHA256SUMS.txt"
NATIVE_BINARY = "hoi4-live-native"
COMMAND = "hoi4-live"
ENTITLEMENT = "com.apple.security.cs.debugger"
LINK_MARK = "rem hoi4-live-tools command link"
MARK_BEGIN = "# >>> hoi4-live-tools (managed) >>>"
MARK_END = "# <<< hoi4-live-tools (managed) <<<"
CLIENTS = ("claude", "codex", "cursor", "opencode")
TOOL_TIMEOUT_SEC = 1800

# What an install (and a release archive) contains, relative to the source folder.
PAYLOAD_DIRS = ("hoi4_live", "skills")
PAYLOAD_DOCS = ("docs/tools.md", "docs/integration.md", "docs/STATUS.md", "docs/PLAN.md")
PAYLOAD_FILES = ("README.md", "CHANGELOG.md", "LICENSE", "NOTICE", "config.json", "requirements.txt",
                 "pyproject.toml", "hoi4-live-tools.cmd", "hoi4-live-tools", "install.py")
SKIP_NAMES = {"__pycache__", ".pytest_cache"}
SKIP_SUFFIXES = (".pyc", ".pyo")

EXIT_OK, EXIT_FAILED, EXIT_USAGE, EXIT_UNVERIFIED, EXIT_REGISTRATION, EXIT_NATIVE = 0, 1, 2, 3, 4, 5


class InstallError(RuntimeError):
    def __init__(self, message, code=EXIT_FAILED):
        super().__init__(message)
        self.code = code


class Output:
    def __init__(self, quiet=False):
        self.quiet = quiet

    def info(self, message):
        if not self.quiet:
            print(message, flush=True)

    def warn(self, message):
        print(f"warning: {message}", file=sys.stderr, flush=True)


# ---------------------------------------------------------------------------------------------------------------
# paths and source


def default_prefix():
    override = os.environ.get("HOI4_LIVE_PREFIX")
    if override:
        return os.path.abspath(override)
    if sys.platform == "darwin":
        return os.path.join(os.path.expanduser("~"), "Library", "Application Support", APP_DIR_NAME)
    if sys.platform != "win32":
        data = os.environ.get("XDG_DATA_HOME") or os.path.join(os.path.expanduser("~"), ".local", "share")
        return os.path.join(data, APP_DIR_NAME)
    base = os.environ.get("LOCALAPPDATA") or os.path.join(os.path.expanduser("~"), "AppData", "Local")
    return os.path.join(base, APP_DIR_NAME)


def source_dir():
    return os.path.dirname(os.path.abspath(__file__))


def read_version(src):
    """The version of the source folder, read from the package without importing it."""
    path = os.path.join(src, "hoi4_live", "__init__.py")
    try:
        with open(path, encoding="utf-8") as f:
            match = re.search(r'^__version__\s*=\s*"([^"]+)"', f.read(), re.M)
    except OSError as exc:
        raise InstallError(f"{src} is not an HOI4 Live Tools folder ({exc})", EXIT_USAGE)
    if not match:
        raise InstallError(f"no __version__ in {path}", EXIT_USAGE)
    return match.group(1)


def payload_files(src):
    """(relative path, absolute path) of every file an install or archive contains, sorted."""
    found = []
    for name in PAYLOAD_FILES + PAYLOAD_DOCS:
        path = os.path.join(src, *name.split("/"))
        if os.path.isfile(path):
            found.append((name, path))
    for top in PAYLOAD_DIRS:
        root = os.path.join(src, top)
        for folder, dirs, files in os.walk(root):
            dirs[:] = sorted(d for d in dirs if d not in SKIP_NAMES)
            for file in sorted(files):
                if file.endswith(SKIP_SUFFIXES):
                    continue
                path = os.path.join(folder, file)
                found.append((os.path.relpath(path, src).replace(os.sep, "/"), path))
    return sorted(found)


def system():
    """win32, darwin or linux (older Pythons report linux2)."""
    return "linux" if sys.platform.startswith("linux") else sys.platform


def launcher_name():
    return WINDOWS_LAUNCHER if system() == "win32" else POSIX_LAUNCHER


def has_server():
    return system() in SERVER_PLATFORMS


def check_platform():
    if system() not in SUPPORTED_PLATFORMS:
        raise InstallError(f"this installer supports Windows, macOS and Linux (this is {sys.platform})", EXIT_USAGE)
    if sys.version_info < (3, 9):
        raise InstallError(f"Python 3.9 or newer is required (this is {sys.version.split()[0]})", EXIT_USAGE)


# ---------------------------------------------------------------------------------------------------------------
# install steps


def run_process(args, out, what, timeout=1800, env=None):
    try:
        result = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                timeout=timeout, env=env)
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise InstallError(f"{what} could not run: {exc}")
    text = result.stdout.decode("utf-8", errors="replace")
    if result.returncode != 0:
        tail = "\n".join(text.strip().splitlines()[-12:])
        raise InstallError(f"{what} failed (exit {result.returncode}):\n{tail}")
    return text


def stage_app(src, prefix, out):
    """Copy the payload into prefix\\app, replacing the previous app folder. Returns the app folder."""
    app = os.path.join(prefix, "app")
    if os.path.abspath(src) == os.path.abspath(app):
        out.info("source is the installed app folder; keeping it")
        return app
    staging, old = app + ".new", app + ".old"
    for leftover in (staging, old):
        shutil.rmtree(leftover, ignore_errors=True)
    files = payload_files(src)
    if not any(name == "hoi4_live/__init__.py" for name, _ in files):
        raise InstallError(f"{src} has no hoi4_live package", EXIT_USAGE)
    for name, path in files:
        target = os.path.join(staging, *name.split("/"))
        os.makedirs(os.path.dirname(target), exist_ok=True)
        shutil.copyfile(path, target)
    if sys.platform != "win32":
        os.chmod(os.path.join(staging, POSIX_LAUNCHER), 0o755)
    try:
        if os.path.isdir(app):
            os.replace(app, old)
        os.replace(staging, app)
    except OSError as exc:
        if os.path.isdir(old) and not os.path.isdir(app):
            os.replace(old, app)
        raise InstallError(f"could not replace {app}: {exc}. Close the clients that run HOI4 Live Tools and retry")
    shutil.rmtree(old, ignore_errors=True)
    out.info(f"installed {len(files)} files to {app}")
    return app


def venv_python(prefix):
    if sys.platform == "win32":
        return os.path.join(prefix, "venv", "Scripts", "python.exe")
    return os.path.join(prefix, "venv", "bin", "python")


def ensure_venv(prefix, out, system_site_packages=False):
    python = venv_python(prefix)
    cfg = os.path.join(prefix, "venv", "pyvenv.cfg")
    if os.path.exists(python) and os.path.exists(cfg):
        probe = subprocess.run([python, "-c", "import sys; print('%d.%d' % sys.version_info[:2])"],
                               stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL)
        if probe.returncode == 0:
            return python
        out.info("the existing environment does not start; recreating it")
    shutil.rmtree(os.path.join(prefix, "venv"), ignore_errors=True)
    args = [sys.executable, "-m", "venv", os.path.join(prefix, "venv")]
    if system_site_packages:
        args.append("--system-site-packages")
    out.info("creating the Python environment")
    run_process(args, out, "creating the Python environment")
    return python


def install_requirements(python, app, out):
    out.info("installing numpy, Pillow and OpenCV" + (" and PyObjC" if sys.platform == "darwin" else "")
             + " (first install downloads about 100 MB)")
    run_process([python, "-m", "pip", "install", "--disable-pip-version-check", "--no-input",
                 "--only-binary=:all:", "-r", os.path.join(app, "requirements.txt")],
                out, "installing the dependencies")


class McpProbe:
    """A throwaway MCP stdio client: initialize, tools/list, stop."""

    def __init__(self, command, env=None):
        self.proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL, env=env)
        self.lines = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for raw in self.proc.stdout:
            self.lines.put(raw.decode("utf-8", errors="replace"))
        self.lines.put(None)

    def send(self, message):
        self.proc.stdin.write((json.dumps(message) + "\n").encode("utf-8"))
        self.proc.stdin.flush()

    def request(self, msg_id, method, params, timeout):
        self.send({"jsonrpc": "2.0", "id": msg_id, "method": method, "params": params})
        while True:
            try:
                line = self.lines.get(timeout=timeout)
            except queue.Empty:
                raise InstallError(f"the server did not answer {method} within {timeout} s", EXIT_UNVERIFIED)
            if line is None:
                raise InstallError("the server exited before answering", EXIT_UNVERIFIED)
            try:
                message = json.loads(line)
            except ValueError:
                continue
            if isinstance(message, dict) and message.get("id") == msg_id:
                if "error" in message:
                    raise InstallError(f"{method} failed: {message['error']}", EXIT_UNVERIFIED)
                return message["result"]

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=15)
        except Exception:
            self.proc.kill()


def verify_server(prefix, version, out, timeout=90):
    """Start the installed launcher and read its tool list. Returns the tool count."""
    launcher = os.path.join(prefix, "app", launcher_name())
    env = dict(os.environ)
    for name in ("HOI4_LIVE_PYTHON", "HOI4_LIVE_CONFIG", "HOI4_LIVE_ARTIFACTS"):
        env.pop(name, None)
    probe = McpProbe([launcher], env=env)
    try:
        init = probe.request(1, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {},
                                               "clientInfo": {"name": "hoi4-live-tools-installer", "version": "1"}},
                             timeout)
        probe.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        reported = (init.get("serverInfo") or {}).get("version")
        if reported != version:
            raise InstallError(f"the installed server reports version {reported}, expected {version}", EXIT_UNVERIFIED)
        tools = probe.request(2, "tools/list", {}, timeout).get("tools", [])
    finally:
        probe.close()
    names = {t.get("name") for t in tools}
    if "hoi4_status" not in names or len(names) < 10:
        raise InstallError(f"the installed server lists {len(names)} tools and lacks hoi4_status", EXIT_UNVERIFIED)
    out.info(f"server answers: {len(names)} tools")
    return len(names)


def selftest(prefix, out):
    """The offline environment check, informational: it needs the game and Windows OCR, which the install does not."""
    launcher = os.path.join(prefix, "app", launcher_name())
    try:
        result = subprocess.run([launcher, "selftest"], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=180)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return [f"selftest could not run: {exc}"]
    lines = result.stdout.decode("utf-8", errors="replace").splitlines()
    problems = [line for line in lines if line.startswith("FAIL")]
    for line in lines:
        out.info("  " + line)
    return problems


# ---------------------------------------------------------------------------------------------------------------
# releases: download and checksum


def release_base():
    return (os.environ.get("HOI4_LIVE_RELEASE_BASE") or f"https://github.com/{REPO}/releases").rstrip("/\\")


def asset_location(version, name):
    """Where a release file lives: a path under a local release folder, or its https address."""
    base = release_base()
    if not base.startswith("https://"):
        return os.path.join(base, name)
    return f"{base}/download/v{version}/{name}" if version else f"{base}/latest/download/{name}"


def fetch_url(location, timeout=180):
    """The bytes of a release file. Every download goes through here."""
    try:
        if location.startswith("https://"):
            request = urllib.request.Request(location, headers={"User-Agent": "hoi4-live-tools-installer"})
            with urllib.request.urlopen(request, timeout=timeout) as response:
                return response.read()
        if "://" in location:
            raise ValueError("only https addresses and local folders are accepted")
        with open(location, "rb") as f:
            return f.read()
    except (urllib.error.URLError, OSError, ValueError) as exc:
        raise InstallError(f"could not download {location}: {exc}. Check the connection and run the install again")


def parse_sums(text):
    """{file name: sha256} of a SHA256SUMS.txt."""
    sums = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 2 and re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
            sums[parts[1].lstrip("*")] = parts[0].lower()
    return sums


def release_sums(version=None):
    """(checksums, version) of a release; the latest one when no version is given."""
    text = fetch_url(asset_location(version, SUMS_NAME)).decode("utf-8", errors="replace")
    sums = parse_sums(text)
    found = sorted({m.group(1) for name in sums
                    for m in [re.fullmatch(APP_DIR_NAME + r"-(\d+\.\d+\.\d+[0-9A-Za-z.+-]*)\.zip", name)] if m})
    if version and version not in found:
        raise InstallError(f"release {version} does not list {APP_DIR_NAME}-{version}.zip in {SUMS_NAME}")
    if not version and len(found) != 1:
        raise InstallError(f"{SUMS_NAME} of the latest release does not name one {APP_DIR_NAME} archive")
    return sums, version or found[0]


def fetch_checked(version, name, sums, out):
    """A release file whose SHA-256 matches SHA256SUMS.txt. Anything else is refused before it is used."""
    expected = sums.get(name)
    if not expected:
        raise InstallError(f"release {version} has no {name} (it is not listed in {SUMS_NAME})")
    out.info(f"downloading {name}")
    data = fetch_url(asset_location(version, name))
    actual = hashlib.sha256(data).hexdigest()
    if actual != expected:
        raise InstallError(f"{name} does not match its checksum in {SUMS_NAME} (expected {expected}, got {actual}); "
                           "nothing from it was installed. Run the install again; if it repeats, report it")
    return data


def _safe_parts(name):
    parts = [part for part in name.replace("\\", "/").split("/") if part not in ("", ".")]
    if name.startswith(("/", "\\")) or ".." in parts or any(":" in part for part in parts):
        raise InstallError(f"the archive holds an unsafe path ({name}); nothing was installed")
    return parts


def unpack_payload(data, folder):
    """Unpack the Python release archive under `folder`; returns its top folder (the source to install from)."""
    tops = set()
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for info in archive.infolist():
            parts = _safe_parts(info.filename)
            if info.is_dir() or not parts:
                continue
            tops.add(parts[0])
            target = os.path.join(folder, *parts)
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with open(target, "wb") as f:
                f.write(archive.read(info))
    if len(tops) != 1:
        raise InstallError("the release archive does not hold one top folder")
    return os.path.join(folder, tops.pop())


def fetch_payload(version, sums, folder, out):
    return unpack_payload(fetch_checked(version, f"{APP_DIR_NAME}-{version}.zip", sums, out), folder)


# ---------------------------------------------------------------------------------------------------------------
# native runtime


def host_machine(platform):
    """The processor of this computer. A Python that runs translated on Apple silicon reports x86_64."""
    machine = host_platform.machine() or ""
    if platform == "darwin" and machine.lower() == "x86_64":
        code, text = run_tool(["/usr/sbin/sysctl", "-n", "hw.optional.arm64"], timeout=10)
        if code == 0 and text.strip() == "1":
            return "arm64"
    return machine


def native_target(platform=None, machine=None):
    """(release platform key, None) when a native runtime is built for this computer, else (None, the reason)."""
    platform = platform or system()
    machine = (machine or host_machine(platform)).lower()
    if platform == "darwin":
        if machine in ("arm64", "aarch64"):
            return "macos-arm64", None
        return None, "the native runtime is built for Apple silicon; an Intel Mac uses the Python server"
    if platform == "linux":
        if machine in ("x86_64", "amd64"):
            return "linux-x86_64", None
        return None, f"the native runtime is built for x86_64 Linux, not {machine or 'this processor'}"
    return None, "the native runtime's Windows adapter is unfinished; Windows uses the Python server"


def native_binary(prefix):
    return os.path.join(prefix, "native", NATIVE_BINARY)


def run_tool(args, timeout=120):
    """(exit code, output) of a helper program (codesign, the native runtime); (None, why) when it cannot run."""
    try:
        result = subprocess.run(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return None, str(exc)
    return result.returncode, result.stdout.decode("utf-8", errors="replace")


def unpack_native(data, folder):
    """Unpack the files of a native release archive (one top folder, plain files only) into `folder`."""
    names = []
    try:
        archive = tarfile.open(fileobj=io.BytesIO(data), mode="r:gz")
    except (tarfile.TarError, OSError, EOFError) as exc:
        raise InstallError(f"the native archive cannot be read: {exc}", EXIT_NATIVE)
    with archive:
        for member in archive.getmembers():
            parts = _safe_parts(member.name)
            if member.isdir():
                continue
            if not member.isfile() or len(parts) != 2:
                raise InstallError(f"the native archive holds an unexpected entry ({member.name})", EXIT_NATIVE)
            target = os.path.join(folder, parts[1])
            with open(target, "wb") as f:
                shutil.copyfileobj(archive.extractfile(member), f)
            os.chmod(target, 0o755 if parts[1] == NATIVE_BINARY else 0o644)
            names.append(parts[1])
    if NATIVE_BINARY not in names:
        raise InstallError(f"the native archive holds no {NATIVE_BINARY}", EXIT_NATIVE)
    return names


def signature_ok(binary):
    """macOS: the binary's signature verifies and it carries the debugger entitlement."""
    code, _ = run_tool(["/usr/bin/codesign", "--verify", "--strict", binary])
    if code != 0:
        return False
    code, text = run_tool(["/usr/bin/codesign", "--display", "--entitlements", "-", binary])
    return code == 0 and ENTITLEMENT in text


def ensure_signed(binary, out):
    """macOS: verify the signature and entitlement; sign with the runtime's own `--doctor --sign` when missing."""
    if signature_ok(binary):
        return "verified"
    out.info("signing the native runtime with the debugger entitlement")
    _, text = run_tool([binary, "--doctor", "--sign"])
    if not signature_ok(binary):
        tail = " | ".join(text.strip().splitlines()[-3:])
        raise InstallError(f"the native runtime could not be signed with the debugger entitlement ({tail})",
                           EXIT_NATIVE)
    return "signed"


def install_native(prefix, version, sums, out, archive=None):
    """Download, check, unpack, sign (macOS) and start-test the native runtime. Returns its record."""
    target, reason = native_target()
    if not target:
        out.info(f"native runtime: not installed ({reason})")
        return {"status": "unavailable", "detail": reason}
    if archive:
        with open(archive, "rb") as f:
            data = f.read()
    else:
        sums = sums if sums is not None else release_sums(version)[0]
        data = fetch_checked(version, f"{NATIVE_BINARY}-{version}-{target}.tar.gz", sums, out)
    folder = os.path.join(prefix, "native")
    staging, old = folder + ".new", folder + ".old"
    for leftover in (staging, old):
        shutil.rmtree(leftover, ignore_errors=True)
    os.makedirs(staging)
    unpack_native(data, staging)
    try:
        if os.path.isdir(folder):
            os.replace(folder, old)
        os.replace(staging, folder)
    except OSError as exc:
        if os.path.isdir(old) and not os.path.isdir(folder):
            os.replace(old, folder)
        raise InstallError(f"could not replace {folder}: {exc}. Close the game session that uses it and retry",
                           EXIT_NATIVE)
    shutil.rmtree(old, ignore_errors=True)
    binary = native_binary(prefix)
    record = {"status": "installed", "version": version, "platform": target, "path": binary}
    if system() == "darwin":
        record["signature"] = ensure_signed(binary, out)
    code, text = run_tool([binary, "--list-tools"])
    try:
        record["tools"] = len(json.loads(text)["tools"]) if code == 0 else 0
    except (ValueError, KeyError, TypeError):
        record["tools"] = 0
    if not record["tools"]:
        tail = " | ".join(text.strip().splitlines()[-3:])
        raise InstallError(f"the native runtime does not start on this computer ({tail})", EXIT_NATIVE)
    out.info(f"native runtime {version} ({target}): {record['tools']} tools"
             + (f", signature {record['signature']}" if "signature" in record else ""))
    return record


# ---------------------------------------------------------------------------------------------------------------
# the hoi4-live command


def command_path(prefix):
    return os.path.join(prefix, COMMAND + (".cmd" if system() == "win32" else ""))


def write_command(prefix):
    """Write the hoi4-live command of this install: it runs this installer's commands with the Python that
    installed it (python3 or python on PATH when that one is gone)."""
    path = command_path(prefix)
    script = os.path.join(prefix, "app", "install.py")
    if system() == "win32":
        text = ("@echo off\r\nrem HOI4 Live Tools command, written by install.py: doctor, update, uninstall, status\r\n"
                "setlocal\r\nset \"HOI4_LIVE_COMMAND=1\"\r\n"
                f"set \"PY={sys.executable}\"\r\nif not exist \"%PY%\" set \"PY=python\"\r\n"
                f"\"%PY%\" \"{script}\" --prefix \"{prefix}\" %*\r\n")
    else:
        def quote(value):
            return "'" + value.replace("'", "'\\''") + "'"
        text = ("#!/bin/sh\n# HOI4 Live Tools command, written by install.py: doctor, update, uninstall, status\n"
                f"PY={quote(sys.executable)}\n[ -x \"$PY\" ] || PY=python3\n"
                f"HOI4_LIVE_COMMAND=1 exec \"$PY\" {quote(script)} --prefix {quote(prefix)} \"$@\"\n")
    with open(path, "w", encoding="utf-8", newline="") as f:
        f.write(text)
    if system() != "win32":
        os.chmod(path, 0o755)
    return path


def path_folders():
    return [os.path.normcase(os.path.abspath(p)) for p in os.environ.get("PATH", "").split(os.pathsep) if p]


def link_location(home=None):
    """The file on the user's PATH that carries the command, or None when no such folder is on PATH."""
    if system() == "win32":
        base = os.environ.get("LOCALAPPDATA")
        folder = os.path.join(base, "Microsoft", "WindowsApps") if base else None
        name = COMMAND + ".cmd"
    else:
        folder, name = os.path.join(home_dir(home), ".local", "bin"), COMMAND
    if not folder or os.path.normcase(os.path.abspath(folder)) not in path_folders():
        return None
    if system() == "win32" and not os.path.isdir(folder):
        return None
    return os.path.join(folder, name)


def link_is_ours(link):
    if os.path.islink(link):
        return os.path.basename(os.readlink(link)) == COMMAND
    try:
        with open(link, encoding="utf-8", errors="replace") as f:
            return LINK_MARK in f.read(400)
    except OSError:
        return False


def link_command(prefix, home=None):
    """Put the command on the user's PATH. Returns {"path", "status"} (linked | conflict | no_path_folder)."""
    link = link_location(home)
    if not link:
        return {"path": None, "status": "no_path_folder"}
    if os.path.lexists(link) and not link_is_ours(link):
        return {"path": link, "status": "conflict"}
    try:
        os.makedirs(os.path.dirname(link), exist_ok=True)
        if os.path.lexists(link):
            os.remove(link)
        if system() == "win32":
            with open(link, "w", encoding="utf-8", newline="") as f:
                f.write(f"@echo off\r\n{LINK_MARK}\r\n\"{command_path(prefix)}\" %*\r\n")
        else:
            os.symlink(command_path(prefix), link)
    except OSError as exc:
        return {"path": link, "status": "failed", "detail": str(exc)}
    return {"path": link, "status": "linked"}


def unlink_command(record):
    """Remove the PATH link an install recorded, when it is still ours."""
    link = ((record or {}).get("command") or {}).get("link")
    if link and os.path.lexists(link) and link_is_ours(link):
        try:
            os.remove(link)
            return link
        except OSError:
            pass
    return None


# ---------------------------------------------------------------------------------------------------------------
# doctor


def run_doctor(prefix, out):
    """Check what a first run needs. True when everything is in place, False when something is to fix, None when
    nothing could be checked. The full doctor is the installed program's own (hoi4_live.doctor); an install that
    predates it gets the native runtime's memory-access check and the server's environment check."""
    app = os.path.join(prefix, "app")
    if os.path.isfile(os.path.join(app, "hoi4_live", "doctor.py")):
        env = dict(os.environ, PYTHONPATH=app, PYTHONIOENCODING="utf-8")
        try:
            result = subprocess.run([sys.executable, "-m", "hoi4_live.doctor", "--prefix", prefix], env=env,
                                    stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    timeout=300)
        except (OSError, subprocess.TimeoutExpired) as exc:
            out.warn(f"the doctor could not run: {exc}")
            return None
        for line in result.stdout.decode("utf-8", errors="replace").splitlines():
            out.info(line)
        return result.returncode == 0
    verdict = None
    if os.path.isfile(native_binary(prefix)):
        code, text = run_tool([native_binary(prefix), "--doctor"])
        for line in text.splitlines():
            out.info(line)
        verdict = code == 0
    if has_server() and os.path.exists(launcher_path(prefix)):
        problems = selftest(prefix, out)
        verdict = not problems and verdict is not False
    return verdict


# ---------------------------------------------------------------------------------------------------------------
# client registrations


def launcher_path(prefix):
    return os.path.join(prefix, "app", launcher_name())


def is_ours(command):
    command = command[0] if isinstance(command, list) and command else command
    return isinstance(command, str) and os.path.basename(command.replace("\\", "/")).lower() in LAUNCHERS


def atomic_write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if os.path.exists(path) and not os.path.exists(path + ".hoi4live.bak"):
        shutil.copyfile(path, path + ".hoi4live.bak")
    temporary = f"{path}.hoi4live.{os.getpid()}.tmp"
    with open(temporary, "w", encoding="utf-8", newline="") as f:
        f.write(text)
    os.replace(temporary, path)


def read_json_object(path):
    """The parsed JSON object of a client file, {} when it does not exist."""
    if not os.path.exists(path):
        return {}
    with open(path, encoding="utf-8-sig") as f:
        raw = f.read()
    if not raw.strip():
        return {}
    data = json.loads(raw)
    if not isinstance(data, dict):
        raise ValueError(f"{path} does not hold a JSON object")
    return data


def dump_json(data):
    return json.dumps(data, indent=2, ensure_ascii=False) + "\n"


def home_dir(home=None):
    return home or os.environ.get("HOI4_LIVE_HOME") or os.path.expanduser("~")


def client_files(home=None):
    home = home_dir(home)
    codex_home = os.environ.get("CODEX_HOME") or os.path.join(home, ".codex")
    claude_dir = os.environ.get("CLAUDE_CONFIG_DIR") or home
    return {
        "claude": os.path.join(claude_dir, ".claude.json"),
        "codex": os.path.join(codex_home, "config.toml"),
        "cursor": os.path.join(home, ".cursor", "mcp.json"),
        "opencode": os.path.join(home, ".config", "opencode", "opencode.json"),
    }


def detect_clients(home=None):
    """Clients with a user configuration folder or file on this machine."""
    home = home_dir(home)
    files = client_files(home)
    markers = {
        "claude": [files["claude"], os.path.join(home, ".claude")],
        "codex": [os.path.dirname(files["codex"])],
        "cursor": [os.path.dirname(files["cursor"])],
        "opencode": [os.path.dirname(files["opencode"])],
    }
    found = [name for name in CLIENTS if any(os.path.exists(p) for p in markers[name])]
    if "claude" not in found and shutil.which("claude"):
        found.insert(0, "claude")
    return found


def _json_entry(client, command):
    if client == "opencode":
        return {"type": "local", "command": [command], "enabled": True}
    entry = {"command": command, "args": []}
    if client == "claude":
        entry = {"type": "stdio", "command": command, "args": [], "env": {}}
    return entry


def _json_servers_key(client):
    return "mcp" if client == "opencode" else "mcpServers"


def register_json(client, path, command):
    key = _json_servers_key(client)
    data = read_json_object(path)
    servers = data.get(key)
    if servers is None:
        servers = data[key] = {}
    if not isinstance(servers, dict):
        raise ValueError(f"{key} in {path} is not an object")
    wanted = _json_entry(client, command)
    existing = servers.get(SERVER_ID)
    if existing is not None:
        current = existing.get("command") if isinstance(existing, dict) else None
        if not is_ours(current):
            return {"client": client, "status": "conflict", "path": path,
                    "detail": f"{SERVER_ID} is already registered for another program ({current}); left unchanged"}
        if existing == wanted:
            return {"client": client, "status": "unchanged", "path": path}
        wanted = {**existing, **wanted}
    servers[SERVER_ID] = wanted
    atomic_write(path, dump_json(data))
    return {"client": client, "status": "updated" if existing is not None else "registered", "path": path}


def unregister_json(client, path):
    if not os.path.exists(path):
        return {"client": client, "status": "absent", "path": path}
    key = _json_servers_key(client)
    data = read_json_object(path)
    servers = data.get(key)
    entry = servers.get(SERVER_ID) if isinstance(servers, dict) else None
    if entry is None:
        return {"client": client, "status": "absent", "path": path}
    command = entry.get("command") if isinstance(entry, dict) else None
    if not is_ours(command):
        return {"client": client, "status": "conflict", "path": path,
                "detail": f"{SERVER_ID} belongs to another program ({command}); left unchanged"}
    del servers[SERVER_ID]
    atomic_write(path, dump_json(data))
    return {"client": client, "status": "removed", "path": path}


def codex_block(command):
    return "\n".join([MARK_BEGIN, f"[mcp_servers.{SERVER_ID}]", f"command = '{command}'", "args = []",
                      "startup_timeout_sec = 30", f"tool_timeout_sec = {TOOL_TIMEOUT_SEC}", MARK_END]) + "\n"


_BLOCK = re.compile(re.escape(MARK_BEGIN) + r".*?" + re.escape(MARK_END) + r"\r?\n?", re.S)
_TABLE = re.compile(r"^\s*\[\s*mcp_servers\s*\.\s*(?:\"hoi4-live\"|'hoi4-live'|hoi4-live)\s*(?:\]|\.)", re.M)


def register_codex(path, command):
    if "'" in command:
        return {"client": "codex", "status": "failed", "path": path,
                "detail": "the install path contains a quote, which TOML literal strings cannot hold"}
    text = ""
    if os.path.exists(path):
        with open(path, encoding="utf-8-sig") as f:
            text = f.read()
    block = codex_block(command)
    if _BLOCK.search(text):
        updated = _BLOCK.sub(lambda _m: block, text, count=1)
        if updated == text:
            return {"client": "codex", "status": "unchanged", "path": path}
        atomic_write(path, updated)
        return {"client": "codex", "status": "updated", "path": path}
    if _TABLE.search(text):
        return {"client": "codex", "status": "conflict", "path": path,
                "detail": f"[mcp_servers.{SERVER_ID}] already exists outside the managed block; left unchanged"}
    separator = "" if not text or text.endswith("\n\n") else ("\n" if text.endswith("\n") else "\n\n")
    atomic_write(path, text + separator + block)
    return {"client": "codex", "status": "registered", "path": path}


def unregister_codex(path):
    if not os.path.exists(path):
        return {"client": "codex", "status": "absent", "path": path}
    with open(path, encoding="utf-8-sig") as f:
        text = f.read()
    if not _BLOCK.search(text):
        return {"client": "codex", "status": "absent", "path": path}
    updated = _BLOCK.sub("", text, count=1).rstrip("\n")
    atomic_write(path, updated + "\n" if updated else "")
    return {"client": "codex", "status": "removed", "path": path}


def register_clients(clients, prefix, home=None):
    command = launcher_path(prefix)
    files = client_files(home)
    results = []
    for client in clients:
        path = files[client]
        try:
            results.append(register_codex(path, command) if client == "codex"
                           else register_json(client, path, command))
        except (OSError, ValueError) as exc:
            results.append({"client": client, "status": "failed", "path": path, "detail": str(exc)})
    return results


def unregister_clients(clients, home=None):
    files = client_files(home)
    results = []
    for client in clients:
        path = files[client]
        try:
            results.append(unregister_codex(path) if client == "codex" else unregister_json(client, path))
        except (OSError, ValueError) as exc:
            results.append({"client": client, "status": "failed", "path": path, "detail": str(exc)})
    return results


def resolve_clients(spec, home=None):
    spec = (spec or "auto").strip().lower()
    if spec == "none":
        return []
    if spec == "auto":
        return detect_clients(home)
    if spec == "all":
        return list(CLIENTS)
    names = [part.strip() for part in spec.split(",") if part.strip()]
    unknown = [name for name in names if name not in CLIENTS]
    if unknown:
        raise InstallError(f"unknown client {', '.join(unknown)}; choose from {', '.join(CLIENTS)}, auto, all, none",
                           EXIT_USAGE)
    return names


def registration_problems(results):
    return [r for r in results if r["status"] in ("conflict", "failed")]


# ---------------------------------------------------------------------------------------------------------------
# install record


def record_path(prefix):
    return os.path.join(prefix, "install.json")


def read_record(prefix):
    try:
        with open(record_path(prefix), encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else None
    except (OSError, ValueError):
        return None


def write_record(prefix, version, python, registrations, native=None, command=None):
    previous = read_record(prefix) or {}
    data = {
        "name": "HOI4 Live Tools",
        "version": version,
        "installed_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "prefix": prefix,
        "launcher": launcher_path(prefix) if has_server() else None,
        "python": python,
        "registrations": [{"client": r["client"], "status": r["status"], "path": r["path"]} for r in registrations],
        "native": native if native is not None else previous.get("native"),
        "command": command if command is not None else previous.get("command"),
    }
    with open(record_path(prefix), "w", encoding="utf-8", newline="\n") as f:
        f.write(dump_json(data))


# ---------------------------------------------------------------------------------------------------------------
# commands


def show_registrations(results, out):
    for r in results:
        out.info(f"  {r['client']}: {r['status']} ({r['path']})" + (f" - {r['detail']}" if r.get("detail") else ""))


def cmd_install(args, out, from_release=False):
    check_platform()
    prefix = os.path.abspath(args.prefix or default_prefix())
    clients = resolve_clients(args.register, args.home)
    previous = read_record(prefix)
    work = None
    try:
        src, sums = source_dir(), None
        local = os.path.isfile(os.path.join(src, "hoi4_live", "__init__.py"))
        if from_release or not local or (args.version and args.version != read_version(src)):
            sums, version = release_sums(args.version)
            if from_release and previous and previous.get("version") == version and not args.version:
                out.info(f"HOI4 Live Tools {version} is the latest release; installing it again")
            work = tempfile.mkdtemp(prefix="hoi4live_dl_")
            src = fetch_payload(version, sums, work, out)
        version = read_version(src)
        out.info(f"HOI4 Live Tools {version} -> {prefix}"
                 + (f" (upgrading from {previous.get('version')})" if previous else ""))
        os.makedirs(prefix, exist_ok=True)
        app = stage_app(src, prefix, out)
    finally:
        if work:
            shutil.rmtree(work, ignore_errors=True)
    python = venv_python(prefix) if has_server() else None
    if has_server() and not args.skip_dependencies:
        python = ensure_venv(prefix, out, args.system_site_packages)
        install_requirements(python, app, out)
    native = {"status": "skipped"}
    if args.native != "skip":
        try:
            native = install_native(prefix, version, sums, out, None if args.native == "auto" else args.native)
        except InstallError as exc:
            native = {"status": "failed", "detail": str(exc)}
            out.warn(f"native runtime: {exc}")
    count, registrations = None, []
    if has_server():
        if not args.skip_verify:
            try:
                count = verify_server(prefix, version, out)
            except InstallError as exc:
                raise InstallError(f"installed to {prefix}, but the server did not start: {exc}", EXIT_UNVERIFIED)
        if clients:
            registrations = register_clients(clients, prefix, args.home)
            show_registrations(registrations, out)
        else:
            out.info(f"no client registration requested; register later with: {COMMAND} register")
    else:
        out.info("no client is registered on Linux yet: the native runtime serves one game run at a time and is "
                 "started for that run (docs/OWNED_LINUX_LAUNCHER.md)")
    command = {"path": write_command(prefix)}
    link = {"path": None, "status": "not_requested"} if args.no_link else link_command(prefix, args.home)
    command["link"] = link["path"] if link["status"] == "linked" else None
    write_record(prefix, version, python, registrations, native, command)
    call = COMMAND if command["link"] else command["path"]
    if link["status"] == "conflict":
        out.warn(f"{link['path']} belongs to another program; left unchanged")
    out.info(f"command: {call}" + ("  (no folder of your PATH could take a link)"
                                   if link["status"] == "no_path_folder" else ""))
    ready = None if args.skip_selftest else run_doctor(prefix, out)
    summary = {"version": version, "prefix": prefix, "launcher": launcher_path(prefix) if has_server() else None,
               "tools": count, "registrations": registrations, "native": native, "command": command,
               "ready": ready, "environment_problems": ["the doctor lists steps to take"] if ready is False else []}
    if args.json:
        print(json.dumps(summary, indent=2))
    failed = registration_problems(registrations)
    for r in failed:
        out.warn(f"{r['client']}: {r['status']} - {r.get('detail', '')}")
    if failed:
        out.warn("installed and verified, but not every client is registered")
        return EXIT_REGISTRATION
    if native["status"] == "failed":
        out.warn(f"installed, but without the native runtime; run `{call} update` to try again")
        return EXIT_NATIVE
    if ready is False:
        out.info(f"Installed. Do the steps listed above, then check again with: {call} doctor")
    elif has_server():
        out.info("Installed. Restart your coding client, then check that the hoi4-live server lists its tools "
                 "(Claude Code: /mcp).")
    else:
        out.info("Installed.")
    return EXIT_OK


def cmd_update(args, out):
    return cmd_install(args, out, from_release=True)


def cmd_doctor(args, out):
    prefix = os.path.abspath(args.prefix or default_prefix())
    if not os.path.isdir(os.path.join(prefix, "app")):
        raise InstallError(f"HOI4 Live Tools is not installed at {prefix}; run the install command first", EXIT_USAGE)
    ready = run_doctor(prefix, Output(False))
    return EXIT_OK if ready else EXIT_FAILED


def cmd_register(args, out):
    check_platform()
    prefix = os.path.abspath(args.prefix or default_prefix())
    if not has_server():
        raise InstallError("there is no server to register on Linux yet (see docs/OWNED_LINUX_LAUNCHER.md)",
                           EXIT_USAGE)
    if not os.path.exists(launcher_path(prefix)):
        raise InstallError(f"HOI4 Live Tools is not installed at {prefix}; run: python install.py", EXIT_USAGE)
    clients = resolve_clients(args.register or args.clients, args.home)
    results = register_clients(clients, prefix, args.home)
    for r in results:
        out.info(f"{r['client']}: {r['status']} ({r['path']})" + (f" - {r['detail']}" if r.get("detail") else ""))
    record = read_record(prefix) or {}
    if record:
        write_record(prefix, record.get("version", "?"), record.get("python", ""), results)
    return EXIT_REGISTRATION if registration_problems(results) else EXIT_OK


def cmd_unregister(args, out):
    clients = resolve_clients(args.register or args.clients or "all", args.home)
    results = unregister_clients(clients, args.home)
    for r in results:
        out.info(f"{r['client']}: {r['status']} ({r['path']})" + (f" - {r['detail']}" if r.get("detail") else ""))
    return EXIT_REGISTRATION if registration_problems(results) else EXIT_OK


def remove_after_exit(path, folder):
    """Windows: delete the running command file, then its folder when empty, a moment after this process ends."""
    line = f'cmd /c "ping -n 3 127.0.0.1 >nul & del /q "{path}" & rmdir "{folder}""'
    try:
        subprocess.Popen(line, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                         creationflags=0x00000008 | 0x00000200, close_fds=True)   # detached, own process group
    except OSError:
        pass


def cmd_uninstall(args, out):
    prefix = os.path.abspath(args.prefix or default_prefix())
    results = unregister_clients(list(CLIENTS), args.home)
    for r in results:
        if r["status"] != "absent":
            out.info(f"{r['client']}: {r['status']}")
    link = unlink_command(read_record(prefix))
    if link:
        out.info(f"removed {link}")
    for name in ("app", "app.new", "app.old", "venv", "native", "native.new", "native.old"):
        shutil.rmtree(os.path.join(prefix, name), ignore_errors=True)
    # On Windows the command file that started this uninstall is still being read by the command prompt.
    running = COMMAND + ".cmd" if system() == "win32" and os.environ.get("HOI4_LIVE_COMMAND") else None
    for name in ("install.json", COMMAND, COMMAND + ".cmd") + (("config.json",) if args.purge else ()):
        if name == running:
            continue
        try:
            os.remove(os.path.join(prefix, name))
        except OSError:
            pass
    if args.purge:
        shutil.rmtree(os.path.join(prefix, "artifacts"), ignore_errors=True)
    if running:
        remove_after_exit(os.path.join(prefix, running), prefix)
    else:
        try:
            os.rmdir(prefix)
        except OSError:
            pass
    left = [name for name in ("app", "venv", "native") if os.path.exists(os.path.join(prefix, name))]
    if left:
        out.warn(f"could not remove {', '.join(left)} in {prefix}; close the programs that use them and retry")
        return EXIT_FAILED
    out.info(f"removed HOI4 Live Tools from {prefix}"
             + ("" if args.purge or not os.path.exists(prefix) else " (settings and artifacts kept; --purge removes them)"))
    return EXIT_OK


def cmd_status(args, out):
    prefix = os.path.abspath(args.prefix or default_prefix())
    record = read_record(prefix)
    installed = os.path.isdir(os.path.join(prefix, "app")) and bool(record)
    files = client_files(args.home)
    registered = {}
    for client in CLIENTS:
        path = files[client]
        try:
            if client == "codex":
                text = open(path, encoding="utf-8-sig").read() if os.path.exists(path) else ""
                registered[client] = bool(_BLOCK.search(text))
            else:
                servers = read_json_object(path).get(_json_servers_key(client)) or {}
                entry = servers.get(SERVER_ID)
                registered[client] = isinstance(entry, dict) and is_ours(entry.get("command"))
        except (OSError, ValueError):
            registered[client] = False
    server = installed and has_server() and os.path.exists(launcher_path(prefix))
    status = {"installed": installed, "prefix": prefix, "version": (record or {}).get("version"),
              "launcher": launcher_path(prefix) if server else None, "registered": registered,
              "native": (record or {}).get("native"), "command": (record or {}).get("command")}
    print(json.dumps(status, indent=2))
    return EXIT_OK if installed else EXIT_FAILED


COMMANDS = ("install", "update", "doctor", "status", "register", "unregister", "uninstall")


def build_parser():
    parser = argparse.ArgumentParser(prog=COMMAND if os.environ.get("HOI4_LIVE_COMMAND") else "install.py",
                                     description=__doc__.split("\n\n")[0])
    parser.add_argument("command", nargs="?", default=None, choices=COMMANDS)
    parser.add_argument("--prefix", help="install folder (default %%LOCALAPPDATA%%\\hoi4-live-tools on Windows, "
                                         "~/Library/Application Support/hoi4-live-tools on macOS, "
                                         "~/.local/share/hoi4-live-tools on Linux)")
    parser.add_argument("--version", default=None, help="install or update: the release to fetch (default: the "
                                                        "copy next to install.py, or the latest release)")
    parser.add_argument("--native", default="auto",
                        help="the native runtime: auto (download it for this computer, default), skip, or the "
                             "path of a hoi4-live-native .tar.gz")
    parser.add_argument("--no-link", action="store_true",
                        help="do not link the hoi4-live command into a folder of the PATH")
    parser.add_argument("--register", default=None,
                        help="clients to register: auto (those found, default), all, none, or a list of "
                             + ", ".join(CLIENTS))
    parser.add_argument("--clients", default=None, help="same as --register, for the register/unregister commands")
    parser.add_argument("--home", help=argparse.SUPPRESS)
    parser.add_argument("--quiet", action="store_true", help="print only warnings and errors")
    parser.add_argument("--json", action="store_true", help="print a JSON summary of an install")
    parser.add_argument("--purge", action="store_true", help="uninstall: also remove config.json and artifacts")
    parser.add_argument("--skip-dependencies", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--skip-verify", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--system-site-packages", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--skip-selftest", action="store_true", help="do not run the doctor after installing")
    return parser


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.command is None:
        if os.environ.get("HOI4_LIVE_COMMAND"):
            # The installed command with no argument explains itself; it never reinstalls by accident.
            parser.print_help()
            return EXIT_USAGE
        args.command = "install"
    out = Output(args.quiet)
    handlers = {"install": cmd_install, "update": cmd_update, "doctor": cmd_doctor, "register": cmd_register,
                "unregister": cmd_unregister, "uninstall": cmd_uninstall, "status": cmd_status}
    try:
        return handlers[args.command](args, out)
    except InstallError as exc:
        print(f"error: {exc}", file=sys.stderr, flush=True)
        return exc.code


if __name__ == "__main__":
    sys.exit(main())
