#!/usr/bin/env python3
"""Install, check, register and remove HOI4 Live Tools for the current Windows user.

    python install.py                       install (or upgrade) and register for the clients found
    python install.py install --register claude,codex
    python install.py install --register none
    python install.py status
    python install.py register [--clients ...]      (re)write the client registrations only
    python install.py unregister
    python install.py uninstall [--purge]

It runs from an unpacked release or from a checkout and needs only Python 3.9+ and, for the first install, network
access to PyPI (numpy, Pillow, OpenCV). Layout of an install (default prefix %LOCALAPPDATA%\\hoi4-live-tools):

    app\\            the program (hoi4-live-tools.cmd lives here: this is what clients start)
    venv\\           the private Python environment with the dependencies
    config.json    optional user settings (the launcher passes it as HOI4_LIVE_CONFIG when it exists)
    artifacts\\     screenshots, scenario reports, the server log, journals, state caches
    install.json   version, paths and registrations of this install

Upgrading replaces app\\ and keeps venv\\ (dependencies are re-resolved), config.json, artifacts\\ and the client
registrations (the path does not change). The installer never touches the game, its settings or its playset.
Client registrations are per user, written atomically after one backup (<file>.hoi4live.bak):

    claude    ~/.claude.json (or $CLAUDE_CONFIG_DIR/.claude.json) mcpServers
    codex     $CODEX_HOME/config.toml or ~/.codex/config.toml, one managed block
    cursor    ~/.cursor/mcp.json
    opencode  ~/.config/opencode/opencode.json

Exit codes: 0 done, 1 install failed, 2 usage or unsupported platform, 3 installed but verification failed,
4 installed and verified but a client registration failed or conflicts.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import queue

SERVER_ID = "hoi4-live"
APP_DIR_NAME = "hoi4-live-tools"
LAUNCHER = "hoi4-live-tools.cmd"
MARK_BEGIN = "# >>> hoi4-live-tools (managed) >>>"
MARK_END = "# <<< hoi4-live-tools (managed) <<<"
CLIENTS = ("claude", "codex", "cursor", "opencode")
TOOL_TIMEOUT_SEC = 1800

# What an install (and a release archive) contains, relative to the source folder.
PAYLOAD_DIRS = ("hoi4_live", "skills")
PAYLOAD_DOCS = ("docs/tools.md", "docs/integration.md", "docs/STATUS.md", "docs/PLAN.md")
PAYLOAD_FILES = ("README.md", "CHANGELOG.md", "LICENSE", "NOTICE", "config.json", "requirements.txt",
                 "pyproject.toml", "hoi4-live-tools.cmd", "install.py")
SKIP_NAMES = {"__pycache__", ".pytest_cache"}
SKIP_SUFFIXES = (".pyc", ".pyo")

EXIT_OK, EXIT_FAILED, EXIT_USAGE, EXIT_UNVERIFIED, EXIT_REGISTRATION = 0, 1, 2, 3, 4


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


def check_platform():
    if sys.platform != "win32":
        raise InstallError("HOI4 Live Tools drives the Windows game window and installs on Windows only", EXIT_USAGE)
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
    return os.path.join(prefix, "venv", "Scripts", "python.exe")


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
    out.info("installing numpy, Pillow and OpenCV (first install downloads about 100 MB)")
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
    launcher = os.path.join(prefix, "app", LAUNCHER)
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
    launcher = os.path.join(prefix, "app", LAUNCHER)
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
# client registrations


def launcher_path(prefix):
    return os.path.join(prefix, "app", LAUNCHER)


def is_ours(command):
    command = command[0] if isinstance(command, list) and command else command
    return isinstance(command, str) and os.path.basename(command.replace("\\", "/")).lower() == LAUNCHER


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


def write_record(prefix, version, python, registrations):
    data = {
        "name": "HOI4 Live Tools",
        "version": version,
        "installed_at": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "prefix": prefix,
        "launcher": launcher_path(prefix),
        "python": python,
        "registrations": [{"client": r["client"], "status": r["status"], "path": r["path"]} for r in registrations],
    }
    with open(record_path(prefix), "w", encoding="utf-8", newline="\n") as f:
        f.write(dump_json(data))


# ---------------------------------------------------------------------------------------------------------------
# commands


def cmd_install(args, out):
    check_platform()
    src = source_dir()
    version = read_version(src)
    prefix = os.path.abspath(args.prefix or default_prefix())
    clients = resolve_clients(args.register, args.home)
    previous = read_record(prefix)
    out.info(f"HOI4 Live Tools {version} -> {prefix}"
             + (f" (upgrading from {previous.get('version')})" if previous else ""))
    os.makedirs(prefix, exist_ok=True)
    app = stage_app(src, prefix, out)
    python = venv_python(prefix)
    if not args.skip_dependencies:
        python = ensure_venv(prefix, out, args.system_site_packages)
        install_requirements(python, app, out)
    registrations = []
    try:
        count = verify_server(prefix, version, out)
    except InstallError as exc:
        raise InstallError(f"installed to {prefix}, but the server did not start: {exc}", EXIT_UNVERIFIED)
    if clients:
        registrations = register_clients(clients, prefix, args.home)
        for r in registrations:
            out.info(f"  {r['client']}: {r['status']} ({r['path']})" + (f" - {r['detail']}" if r.get("detail") else ""))
    else:
        out.info("no client registration requested; register later with: python install.py register")
    write_record(prefix, version, python, registrations)
    problems = [] if args.skip_selftest else selftest(prefix, out)
    for line in problems:
        out.warn(f"environment check: {line}")
    summary = {"version": version, "prefix": prefix, "launcher": launcher_path(prefix), "tools": count,
               "registrations": registrations, "environment_problems": problems}
    if args.json:
        print(json.dumps(summary, indent=2))
    failed = registration_problems(registrations)
    for r in failed:
        out.warn(f"{r['client']}: {r['status']} - {r.get('detail', '')}")
    if failed:
        out.warn("installed and verified, but not every client is registered")
        return EXIT_REGISTRATION
    out.info("Restart your coding client, then check that the hoi4-live server lists its tools "
             "(Claude Code: /mcp). Use `hoi4-live-tools.cmd selftest` for the game and OCR checks.")
    return EXIT_OK


def cmd_register(args, out):
    check_platform()
    prefix = os.path.abspath(args.prefix or default_prefix())
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


def cmd_uninstall(args, out):
    prefix = os.path.abspath(args.prefix or default_prefix())
    results = unregister_clients(list(CLIENTS), args.home)
    for r in results:
        if r["status"] != "absent":
            out.info(f"{r['client']}: {r['status']}")
    for name in ("app", "app.new", "app.old", "venv"):
        shutil.rmtree(os.path.join(prefix, name), ignore_errors=True)
    for name in ("install.json",) + (("config.json",) if args.purge else ()):
        try:
            os.remove(os.path.join(prefix, name))
        except OSError:
            pass
    if args.purge:
        shutil.rmtree(os.path.join(prefix, "artifacts"), ignore_errors=True)
    try:
        os.rmdir(prefix)
    except OSError:
        pass
    left = [name for name in ("app", "venv") if os.path.exists(os.path.join(prefix, name))]
    if left:
        out.warn(f"could not remove {', '.join(left)} in {prefix}; close the programs that use them and retry")
        return EXIT_FAILED
    out.info(f"removed HOI4 Live Tools from {prefix}"
             + ("" if args.purge or not os.path.exists(prefix) else " (settings and artifacts kept; --purge removes them)"))
    return EXIT_OK


def cmd_status(args, out):
    prefix = os.path.abspath(args.prefix or default_prefix())
    record = read_record(prefix)
    installed = os.path.exists(launcher_path(prefix))
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
    status = {"installed": installed, "prefix": prefix, "version": (record or {}).get("version"),
              "launcher": launcher_path(prefix) if installed else None, "registered": registered}
    print(json.dumps(status, indent=2))
    return EXIT_OK if installed else EXIT_FAILED


def build_parser():
    parser = argparse.ArgumentParser(prog="install.py", description=__doc__.split("\n\n")[0])
    parser.add_argument("command", nargs="?", default="install",
                        choices=("install", "status", "register", "unregister", "uninstall"))
    parser.add_argument("--prefix", help="install folder (default %%LOCALAPPDATA%%\\hoi4-live-tools)")
    parser.add_argument("--register", default=None,
                        help="clients to register: auto (those found, default), all, none, or a list of "
                             + ", ".join(CLIENTS))
    parser.add_argument("--clients", default=None, help="same as --register, for the register/unregister commands")
    parser.add_argument("--home", help=argparse.SUPPRESS)
    parser.add_argument("--quiet", action="store_true", help="print only warnings and errors")
    parser.add_argument("--json", action="store_true", help="print a JSON summary of an install")
    parser.add_argument("--purge", action="store_true", help="uninstall: also remove config.json and artifacts")
    parser.add_argument("--skip-dependencies", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--system-site-packages", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--skip-selftest", action="store_true", help="do not run the environment check")
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    out = Output(args.quiet)
    handlers = {"install": cmd_install, "register": cmd_register, "unregister": cmd_unregister,
                "uninstall": cmd_uninstall, "status": cmd_status}
    try:
        return handlers[args.command](args, out)
    except InstallError as exc:
        print(f"error: {exc}", file=sys.stderr, flush=True)
        return exc.code


if __name__ == "__main__":
    sys.exit(main())
