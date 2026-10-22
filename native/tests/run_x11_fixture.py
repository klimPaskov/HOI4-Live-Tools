"""Run the opt-in Rust transport fixture on its own authenticated Xvfb server."""
import os
import argparse
import json
from pathlib import Path
import secrets
import signal
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
                        help="Run a cross-built test binary (library binary for --owner-close-only)")
    parser.add_argument("--close-only", action="store_true",
                        help="Only WM close fixtures, with XTEST disabled on owned Xvfb")
    parser.add_argument("--owner-close-only", action="store_true",
                        help="Run full owner/reaper and actual ambiguous-wire library fixtures")
    parser.add_argument("--pointer-only", action="store_true",
                        help="Only private generated pointer delivery/refusal/wire fixtures")
    args = parser.parse_args()
    if sum((args.close_only, args.owner_close_only, args.pointer_only)) > 1:
        parser.error("Choose one isolated fixture mode")
    close_mode = args.close_only or args.owner_close_only
    if sys.platform != "linux" or os.getuid() == 0:
        raise SystemExit("Fixture requires an ordinary Linux user")
    if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
        raise SystemExit("Fixture requires pinned pidfd cleanup; no numeric-PID fallback")
    probe = os.pidfd_open(os.getpid(), 0)
    os.close(probe)
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
            server_command = [xvfb_exe, f":{display}", "-screen", "0", "2560x1440x24",
                              "-nolisten", "tcp", "-auth", str(auth)]
            if close_mode:
                server_command.extend(["-extension", "XTEST"])
            server = subprocess.Popen(server_command,
                                      stdin=subprocess.DEVNULL, stdout=log, stderr=log)
            server_fd = None
            expected = None
            creation_ticks = None
            try:
                try:
                    server_fd = os.pidfd_open(server.pid, 0)
                except OSError:
                    print("Cannot pin fixture Xvfb; preserving parent until its exit",
                          file=sys.stderr)
                    server.wait()
                    raise
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
                if close_mode:
                    env["HOI4_NATIVE_X11_CLOSE_NO_XTEST"] = "1"
                # Keep the host cargo/runtime selection; never open a game or
                # reuse the normal DISPLAY inherited from the user's shell.
                test_args = ["--ignored", "--nocapture", "--test-threads=1"]
                if args.close_only:
                    test_args.extend(["--exact", "owned_xvfb_window_close"])
                if args.owner_close_only:
                    test_args.append("launch::linux::tests::window_close::owned_xvfb_window_close")
                if args.pointer_only:
                    test_args.append("pointer::owned_xvfb_pointer")
                suite = ["--lib"] if args.owner_close_only else ["--test", "x11_fixture"]
                command = ([str(args.test_binary.resolve())] + test_args
                           if args.test_binary else
                           ["cargo", "test", "--manifest-path",
                            str(Path(__file__).resolve().parents[1] / "Cargo.toml"),
                            "--locked"] + suite + ["--"] + test_args)
                runner = subprocess.Popen(command, env=env)
                try:
                    code = runner.wait(timeout=45)
                except subprocess.TimeoutExpired:
                    # Killing the suite can abandon its owned-child reaper.
                    # Keep this parent/display until that exact runner exits.
                    print("Fixture runner exceeded 45 seconds; preserving ownership; no kill",
                          file=sys.stderr)
                    runner.wait()
                    raise
                if code:
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
                    try:
                        signal.pidfd_send_signal(server_fd, signal.SIGTERM, None, 0)
                    except ProcessLookupError:
                        pass  # Exact pinned child exited between checks.
                    try:
                        server.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        if identity(server.pid) != expected:
                            raise RuntimeError("Fixture process identity changed; no signal sent")
                        signal.pidfd_send_signal(server_fd, signal.SIGKILL, None, 0)
                        server.wait(timeout=5)
                if server_fd is not None:
                    os.close(server_fd)


if __name__ == "__main__":
    main()
