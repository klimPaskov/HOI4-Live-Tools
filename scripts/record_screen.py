"""Segmented capture of an explicitly owned HOI4 client, with a hidden supervisor.

start OUT --owned-pid PID --owned-created EXACT_TIME --owned-exe PATH --window-id ID --raw-root ROOT
stop OUT --token TOKEN

The supervisor retains ffmpeg stdin for graceful q (Windows) or EOF (Mac) shutdown.
Stop markers are session-bound; no stored PID is ever used to terminate a process.
"""

import argparse
from dataclasses import asdict
import json
import os
import posixpath
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import uuid

CREATE_NO_WINDOW = 0x08000000
DEFAULT_CAP = 40_000_000_000
DEFAULT_HEADROOM = 2_000_000_000
CAP_RESERVE = 256_000_000
POLL_SECONDS = 0.5
TERMINAL = {"stopped", "error"}


def ffmpeg_path():
    return shutil.which("ffmpeg") or "ffmpeg"


def encoder_args(a):
    if a.encoder == "amf":
        return ["-c:v", "h264_amf", "-usage", "transcoding", "-quality", "speed", "-rc", "cqp",
                "-qp_i", str(a.crf), "-qp_p", str(a.crf + 2), "-qp_b", str(a.crf + 2)]
    return ["-c:v", "libx264", "-preset", "veryfast", "-crf", str(a.crf), "-threads", "3",
            "-maxrate", "12M", "-bufsize", "24M"]


def capture_size(size, client):
    if len(client) != 4 or any(type(v) is not int for v in client) or min(client[2:]) <= 0:
        raise ValueError("Invalid game client rectangle")
    if size == "auto":
        w, h = (v - v % 2 for v in client[2:])
    else:
        match = re.fullmatch(r"([1-9][0-9]*)x([1-9][0-9]*)", size)
        if not match:
            raise ValueError("Capture size must be auto or WxH")
        w, h = map(int, match.groups())
    if min(w, h) <= 0 or w > client[2] or h > client[3]:
        raise ValueError("Capture region would extend outside the owned game client")
    if w % 2 or h % 2:
        raise ValueError("Explicit yuv420p dimensions must be even")
    return w, h


def validate_options(a):
    if not 1 <= a.fps <= 60 or not 0 <= a.crf <= 51 or a.segment <= 0:
        raise ValueError("Invalid fps, CRF or segment duration")
    if a.cap_bytes <= CAP_RESERVE or a.headroom_bytes < 0:
        raise ValueError("Footage cap must exceed its reserve; headroom must be nonnegative")
    if a.encoder not in ("x264", "amf"):
        raise ValueError("Unknown encoder")


def active_backend():
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from hoi4_live.platform import current
    name = current().name
    if name not in ("windows", "macos"):
        from hoi4_live.errors import CapabilityUnavailable
        raise CapabilityUnavailable("owned-window recording", name, "no recording backend is available")
    return name


def identity_from_args(a):
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    backend = getattr(a, "backend", None) or active_backend()
    if backend == "windows":
        from hoi4_live.memory.winproc import ProcessIdentity, _validate_identity
        identity = ProcessIdentity(a.owned_pid, a.owned_created, a.owned_exe)
        _validate_identity(identity)
    elif backend == "macos":
        from hoi4_live.platform.identity import ProcessIdentity
        if (type(a.owned_pid) is not int or a.owned_pid <= 0 or
                type(a.owned_created) is not int or a.owned_created <= 0):
            raise ValueError("An exact owned PID and process creation time are required")
        if (not isinstance(a.owned_exe, str) or not posixpath.isabs(a.owned_exe)
                or posixpath.basename(a.owned_exe) != "hoi4"):
            raise ValueError("The owned executable must be an absolute path to hoi4")
        identity = ProcessIdentity(a.owned_pid, a.owned_created, a.owned_exe)
    else:
        raise ValueError("Unknown recording backend")
    return identity


def inspect_owned_window(identity, hwnd, process_api=None, window_api=None):
    """Query metadata and geometry through the native boundary, never VM_READ."""
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    backend = "windows" if process_api is not None or window_api is not None else active_backend()
    if backend == "windows":
        from hoi4_live.platform.windows_recording import inspect_owned_window as inspect
        return inspect(identity, hwnd, process_api, window_api)
    from hoi4_live.platform.macos_recording import FrameSource
    return FrameSource(identity, hwnd).inspect()


def build_command(a, client, token, executable=None):
    validate_options(a)
    w, h = capture_size(a.size, client)
    if type(a.hwnd) is not int or a.hwnd <= 0:
        raise ValueError("Owned window ID is required")
    backend = getattr(a, "backend", None) or active_backend()
    if backend == "windows":
        # gdigrab HWND uses GetDC/GetClientRect; offsets are client-relative.
        capture = ["-f", "gdigrab", "-framerate", str(a.fps), "-draw_mouse", "1",
                   "-offset_x", "0", "-offset_y", "0", "-video_size", "%dx%d" % (w, h),
                   "-i", "hwnd=%d" % a.hwnd]
    elif backend == "macos":
        if a.encoder != "x264":
            raise ValueError("macOS owned-window recording requires the x264 encoder")
        # Only verified window RGB frames enter stdin. Wall timestamps preserve
        # elapsed recording time if capture delivers fewer frames than requested.
        capture = ["-f", "rawvideo", "-pixel_format", "rgb24", "-video_size", "%dx%d" % (w, h),
                   "-framerate", str(a.fps), "-use_wallclock_as_timestamps", "1", "-i", "pipe:0",
                   "-vsync", "cfr"]
    else:
        raise ValueError("Unknown recording backend")
    return [executable or ffmpeg_path(), "-hide_banner", "-loglevel", "warning", "-n",
            *capture, *encoder_args(a), "-pix_fmt", "yuv420p",
            "-g", str(a.fps * 2), "-f", "segment", "-segment_time", str(a.segment),
            "-reset_timestamps", "1", "-strftime", "1", "-segment_format", "matroska",
            str(Path(a.out) / ("seg_" + token[:12] + "_%Y%m%d_%H%M%S.mkv"))]


def write_json(path, data):
    path = Path(path)
    fd, temporary = tempfile.mkstemp(prefix=".recording-", dir=str(path.parent))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(data, stream, indent=2)
            stream.write("\n")
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def read_info(out):
    return json.loads((Path(out) / "recording.json").read_text(encoding="utf-8"))


def valid_token(token):
    if not isinstance(token, str) or not re.fullmatch(r"[a-f0-9]{32}", token):
        raise ValueError("An explicit recording session token is required")
    return token


def paths(out, raw_root):
    root, out = Path(raw_root).resolve(), Path(out).resolve()
    if out == root or root not in out.parents:
        raise ValueError("Output must be a campaign directory within the shared raw root")
    return out, root


def budget_status(raw_root, cap_bytes=DEFAULT_CAP, headroom_bytes=DEFAULT_HEADROOM):
    """Sum all campaign MKVs. Reserve covers polling delay and final segment flush."""
    root, used = Path(raw_root).resolve(), 0
    def inaccessible(error):
        raise error

    for directory, _dirs, files in os.walk(root, followlinks=False, onerror=inaccessible):
        for filename in files:
            path = Path(directory) / filename
            if path.suffix.lower() == ".mkv" and not path.is_symlink():
                try:
                    used += path.stat().st_size
                except FileNotFoundError:
                    pass
    free = shutil.disk_usage(root).free
    reason = "footage_cap" if used >= cap_bytes - CAP_RESERVE else (
        "disk_headroom" if free <= headroom_bytes else None)
    return {"used_bytes": used, "free_bytes": free, "cap_bytes": cap_bytes,
            "headroom_bytes": headroom_bytes, "reserve_bytes": CAP_RESERVE, "reason": reason}


def check_lock(root, token):
    path = root / ".recording-active.json"
    if path.is_symlink() or json.loads(path.read_text(encoding="utf-8")).get("token") != token:
        raise RuntimeError("Recording does not own the shared raw-root lock")


def release_lock(root, token):
    check_lock(root, token)
    (root / ".recording-active.json").unlink()


def stop_requested(out, token):
    path = out / (".stop-" + token + ".json")
    if not path.exists():
        return False
    if path.is_symlink():
        raise RuntimeError("Stop marker must not be a symlink")
    return json.loads(path.read_text(encoding="utf-8")).get("token") == token


def request_stop(out, token):
    """Only write a token-bound request. No PID lookup or termination."""
    token, out = valid_token(token), Path(out).resolve()
    info = read_info(out)
    if info.get("token") != token:
        raise RuntimeError("Stop token does not match the recording session")
    if info.get("status") not in TERMINAL:
        write_json(out / (".stop-" + token + ".json"), {"token": token, "requested_epoch": time.time()})
    return info


def finish_process(process, timeout=10, raw_input=False):
    """Graceful q or raw-input EOF; escalation uses only the actual Popen."""
    escalation = None
    if process.poll() is None:
        try:
            if raw_input:
                process.stdin.close()
            else:
                process.stdin.write(b"q\n")
                process.stdin.flush()
        except (BrokenPipeError, OSError):
            pass
        try:
            process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            escalation = "terminate"
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                escalation = "kill"
                process.kill()
                process.wait(timeout=5)
    if process.stdin is not None:
        process.stdin.close()
    return process.returncode, escalation


def supervisor_command(out, token):
    return [sys.executable, str(Path(__file__).resolve()), "run", str(out), "--token", token]


def start(a, *, inspect=inspect_owned_window, popen=subprocess.Popen, sleep=time.sleep):
    validate_options(a)
    a.backend = getattr(a, "backend", None) or active_backend()
    identity = identity_from_args(a)
    client = inspect(identity, a.hwnd)
    capture_size(a.size, client)
    if not a.raw_root:
        raise ValueError("An explicit shared --raw-root is required")
    out, root = paths(a.out, a.raw_root)
    root.mkdir(parents=True, exist_ok=True)
    if budget_status(root, a.cap_bytes, a.headroom_bytes)["reason"]:
        raise RuntimeError("Shared footage or disk budget has been reached")
    out.mkdir(parents=True, exist_ok=True)
    token, lock = uuid.uuid4().hex, root / ".recording-active.json"
    try:
        with lock.open("x", encoding="utf-8") as stream:
            json.dump({"token": token, "out": str(out)}, stream)
            stream.flush()
            os.fsync(stream.fileno())
    except FileExistsError:
        raise RuntimeError("Another recording owns this raw root; stop its supervisor first")
    info = {"version": 3, "backend": a.backend, "token": token, "status": "starting", "started_epoch": time.time(),
            "raw_root": str(root), "out": str(out), "owned_game": asdict(identity), "hwnd": a.hwnd,
            "client": list(client), "fps": a.fps, "crf": a.crf, "encoder": a.encoder,
            "size": a.size, "segment": a.segment, "cap_bytes": a.cap_bytes,
            "headroom_bytes": a.headroom_bytes}
    child = None
    try:
        write_json(out / "recording.json", info)
        with (out / "supervisor.log").open("ab") as log:
            child = popen(supervisor_command(out, token), stdin=subprocess.DEVNULL,
                          stdout=log, stderr=log, close_fds=True,
                          start_new_session=os.name != "nt",
                          creationflags=CREATE_NO_WINDOW if os.name == "nt" else 0)
        for _ in range(40):
            saved = read_info(out)
            if saved.get("token") != token:
                raise RuntimeError("Recording metadata changed during startup")
            if saved["status"] == "running":
                print(json.dumps(saved))
                return saved
            if saved["status"] in TERMINAL or child.poll() is not None:
                raise RuntimeError(saved.get("error") or "Supervisor exited during startup; see supervisor.log")
            sleep(0.25)
        raise RuntimeError("Recorder startup timed out; graceful stop requested")
    except BaseException as error:
        if child is None or child.poll() is not None:
            info.update(status="error", error=str(error), stopped_epoch=time.time())
            write_json(out / "recording.json", info)
            if lock.exists():
                release_lock(root, token)
        else:
            request_stop(out, token)
        raise


def run(a, *, inspect=inspect_owned_window, popen=subprocess.Popen, sleep=time.sleep,
        frame_source_factory=None, monotonic=time.monotonic):
    token, info = valid_token(a.token), read_info(a.out)
    if info.get("token") != token or info.get("status") != "starting":
        raise RuntimeError("Supervisor token or starting state does not match")
    out, root = paths(a.out, info["raw_root"])
    if info.get("out") != str(out):
        raise RuntimeError("Recording output identity does not match its metadata")
    check_lock(root, token)
    options = argparse.Namespace(**{k: info[k] for k in (
        "out", "fps", "crf", "encoder", "size", "segment", "hwnd", "cap_bytes", "headroom_bytes")})
    options.backend = info.get("backend", "windows")
    options.owned_pid = info["owned_game"]["pid"]
    options.owned_created = info["owned_game"]["creation_time"]
    options.owned_exe = info["owned_game"]["executable_path"]
    process, log, source, reason = None, None, None, "error"
    try:
        validate_options(options)
        identity = identity_from_args(options)
        if options.backend == "macos":
            if frame_source_factory is None:
                from hoi4_live.platform.macos_recording import FrameSource
                frame_source_factory = FrameSource
            source = frame_source_factory(identity, options.hwnd)
            client = source.inspect()
        else:
            client = inspect(identity, options.hwnd)
        w, h = capture_size(options.size, client)
        budget = budget_status(root, options.cap_bytes, options.headroom_bytes)
        if budget["reason"]:
            raise RuntimeError("Recording budget reached before capture: " + budget["reason"])
        if stop_requested(out, token):
            reason = "stop_requested_before_capture"
        else:
            first_frame = source.frame((w, h)) if source else None
            command = build_command(options, client, token)
            log = (out / "ffmpeg.log").open("ab")
            process = popen(command, stdin=subprocess.PIPE, stdout=log, stderr=log,
                            close_fds=True, creationflags=CREATE_NO_WINDOW if os.name == "nt" else 0)
            if source:
                source.write(process.stdin, first_frame)
                info["frames_written"] = 1
            info.update(status="running", supervisor_pid=os.getpid(), ffmpeg_pid=process.pid,
                        pid=process.pid, capture_size=[w, h], client=list(client),
                        cmd=command, budget=budget, capture_started_epoch=time.time())
            write_json(out / "recording.json", info)
            next_frame = monotonic()
            while True:
                if process.poll() is not None:
                    reason = "ffmpeg_exited"
                    if process.returncode:
                        raise RuntimeError("ffmpeg exited with %s; see ffmpeg.log" % process.returncode)
                    break
                if stop_requested(out, token):
                    reason = "stop_requested"
                    break
                check_lock(root, token)
                current = source.inspect() if source else inspect(identity, options.hwnd)
                if current[2:] != client[2:]:
                    reason = "client_geometry_changed"
                    break
                budget = budget_status(root, options.cap_bytes, options.headroom_bytes)
                info["budget"] = budget
                if budget["reason"]:
                    reason = budget["reason"]
                    break
                if source:
                    frame = source.frame((w, h))
                    source.write(process.stdin, frame)
                    info["frames_written"] = info.get("frames_written", 0) + 1
                    next_frame = max(next_frame + 1.0 / options.fps, monotonic())
                    sleep(max(0, next_frame - monotonic()))
                else:
                    sleep(POLL_SECONDS)
        info["status"] = "stopped"
    except BaseException as error:
        info.update(status="error", error=str(error))
    finally:
        if process is not None:
            try:
                code, escalation = finish_process(process, raw_input=source is not None)
                info.update(ffmpeg_returncode=code, shutdown_escalation=escalation)
                if escalation or (code and info["status"] != "error"):
                    info.update(status="error", error="ffmpeg shutdown: returncode=%s, escalation=%s" % (code, escalation))
            except BaseException as error:
                info.update(status="error", error="Could not stop owned ffmpeg child: " + str(error))
        if log is not None:
            log.close()
        info.update(stop_reason=reason, stopped_epoch=time.time())
        write_json(out / "recording.json", info)
        if process is None or process.poll() is not None:
            release_lock(root, token)
    return info


def stop(a, *, sleep=time.sleep):
    info = request_stop(a.out, a.token)
    for _ in range(max(0, int(a.wait / 0.25))):
        info = read_info(a.out)
        if info.get("token") != a.token:
            raise RuntimeError("Recording session changed while waiting for stop")
        if info.get("status") in TERMINAL:
            break
        sleep(0.25)
    print(json.dumps(info))
    if info.get("status") not in TERMINAL:
        raise RuntimeError("Stop requested; supervisor has not finished yet (no PID termination attempted)")
    return info


def parser():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("cmd", choices=["start", "run", "stop"])
    ap.add_argument("out")
    ap.add_argument("--fps", type=int, default=24)
    ap.add_argument("--crf", type=int, default=28)
    ap.add_argument("--size", default="auto", help="WxH within client; auto rounds down to even dimensions")
    ap.add_argument("--segment", type=int, default=900)
    ap.add_argument("--encoder", choices=["x264", "amf"], default="x264")
    ap.add_argument("--owned-pid", type=int)
    ap.add_argument("--owned-created", type=int, help="Exact process creation identity (Windows FILETIME; Mac microseconds)")
    ap.add_argument("--owned-exe")
    ap.add_argument("--window-id", "--hwnd", dest="hwnd", metavar="ID", type=lambda value: int(value, 0))
    ap.add_argument("--raw-root")
    ap.add_argument("--cap-bytes", type=int, default=DEFAULT_CAP)
    ap.add_argument("--headroom-bytes", type=int, default=DEFAULT_HEADROOM)
    ap.add_argument("--token")
    ap.add_argument("--wait", type=float, default=20)
    return ap


def main():
    a = parser().parse_args()
    sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
    from hoi4_live.errors import ToolError
    try:
        info = {"start": start, "run": run, "stop": stop}[a.cmd](a)
        return 1 if info.get("status") == "error" else 0
    except (OSError, ValueError, RuntimeError, ToolError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
