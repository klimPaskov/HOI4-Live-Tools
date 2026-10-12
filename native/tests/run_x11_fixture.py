"""Run the opt-in Rust transport fixture on its own authenticated Xvfb server."""
import os
import argparse
import json
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tempfile
import time


def identity(pid):
    root = Path("/proc") / str(pid)
    fields = (root / "stat").read_text().rsplit(") ", 1)[1].split()
    result = {"pid": pid, "start_ticks": fields[19],
              "exe": str((root / "exe").resolve(strict=True)), "uid": root.stat().st_uid}
    after = (root / "stat").read_text().rsplit(") ", 1)[1].split()
    if after[19] != result["start_ticks"] or after[1] != str(os.getpid()):
        raise RuntimeError("Owned fixture process creation/parent identity changed")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--test-binary", type=Path,
                        help="Run an explicitly supplied cross-built x11_fixture test binary")
    args = parser.parse_args()
    if sys.platform != "linux" or os.getuid() == 0:
        raise SystemExit("Fixture requires an ordinary Linux user")
    xvfb_exe = shutil.which("Xvfb")
    xauth_exe = shutil.which("xauth")
    if not xvfb_exe or not xauth_exe:
        raise SystemExit("Install Xvfb and xauth for the opt-in transport fixture")
    display = next((n for n in range(120, 150)
                    if not Path(f"/tmp/.X11-unix/X{n}").exists()
                    and not Path(f"/tmp/.X{n}-lock").exists()), None)
    if display is None:
        raise SystemExit("No unused fixture display; existing displays are preserved")
    with tempfile.TemporaryDirectory(prefix="hoi4-native-x11-") as directory:
        auth = Path(directory) / "authority"
        auth.touch(mode=0o600)
        subprocess.run([xauth_exe, "-f", str(auth), "add", f":{display}",
                        "MIT-MAGIC-COOKIE-1", secrets.token_hex(16)],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        with (Path(directory) / "xvfb.log").open("ab") as log:
            server = subprocess.Popen([xvfb_exe, f":{display}", "-screen", "0",
                                       "640x480x24", "-nolisten", "tcp", "-auth", str(auth)],
                                      stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            expected = None
            creation_ticks = None
            try:
                creation_ticks = identity(server.pid)["start_ticks"]
                deadline = time.monotonic() + 5
                while not Path(f"/tmp/.X11-unix/X{display}").exists():
                    if server.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError("Owned fixture display did not start")
                    time.sleep(0.05)
                expected = identity(server.pid)
                if (expected["start_ticks"] != creation_ticks
                        or expected["uid"] != os.getuid()
                        or expected["exe"] != str(Path(xvfb_exe).resolve())):
                    raise RuntimeError("Owned fixture Xvfb executable identity changed")
                env = dict(os.environ, DISPLAY=f":{display}", XAUTHORITY=str(auth),
                           HOI4_NATIVE_X11_FIXTURE_DISPLAY=str(display),
                           HOI4_NATIVE_X11_FIXTURE_SERVER=json.dumps(expected),
                           HOI4_NATIVE_X11_FIXTURE_AUTHORITY=str(auth))
                # Keep the host cargo/runtime selection; never open a game or
                # reuse the normal DISPLAY inherited from the user's shell.
                command = ([str(args.test_binary.resolve()), "--ignored", "--nocapture"]
                           if args.test_binary else
                           ["cargo", "test", "--manifest-path",
                            str(Path(__file__).resolve().parents[1] / "Cargo.toml"),
                            "--locked", "--test", "x11_fixture", "--", "--ignored", "--nocapture"])
                result = subprocess.run(command, env=env, timeout=45)
                if result.returncode:
                    raise RuntimeError("X11 transport fixture failed")
            finally:
                if server.poll() is None:
                    current = identity(server.pid)
                    if (creation_ticks is None
                            or current["start_ticks"] != creation_ticks
                            or current["uid"] != os.getuid()
                            or current["exe"] != str(Path(xvfb_exe).resolve())
                            or (expected is not None and current != expected)):
                        raise RuntimeError("Fixture process identity changed; no signal sent")
                    expected = current
                    server.terminate()
                    try:
                        server.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        if identity(server.pid) != expected:
                            raise RuntimeError("Fixture process identity changed; no signal sent")
                        server.kill()
                        server.wait(timeout=5)


if __name__ == "__main__":
    main()
