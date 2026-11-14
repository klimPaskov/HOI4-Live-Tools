"""The installer's release download, checksum, native runtime, command and whole-flow decisions.

Nothing here touches the network, the real home folder or a system setting: releases come from a temporary
folder (HOI4_LIVE_RELEASE_BASE), helper programs go through install.run_tool, and the home is a temporary one.
"""

import contextlib
import hashlib
import io
import json
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from unittest import mock

import helpers  # noqa: F401

sys.path.insert(0, os.path.join(helpers.ROOT, "scripts"))
import install  # noqa: E402
import build_release  # noqa: E402

VERSION = install.read_version(helpers.ROOT)
FAKE_NATIVE = """#!/bin/sh
case "$1" in
  --list-tools) echo '{"tools":[{"name":"hoi4_overview"},{"name":"hoi4_time"}]}' ;;
  --doctor) echo "memory access is ready"; exit 0 ;;
  *) exit 9 ;;
esac
"""


def sha(data):
    return hashlib.sha256(data).hexdigest()


def native_archive(version, target, files=None, extra=None):
    """A native release archive like scripts/package_native.py writes, holding a stand-in runtime."""
    top = f"hoi4-live-native-{version}-{target}"
    files = files if files is not None else {install.NATIVE_BINARY: FAKE_NATIVE.encode(),
                                             "debugger.entitlements": b"<plist/>", "LICENSE": b"licence"}
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w:gz") as tar:
        for name, data in files.items():
            info = tarfile.TarInfo(f"{top}/{name}")
            info.size = len(data)
            info.mode = 0o755 if name == install.NATIVE_BINARY else 0o644
            tar.addfile(info, io.BytesIO(data))
        for info in extra or ():
            tar.addfile(info)
    return raw.getvalue()


class Sandbox(unittest.TestCase):
    """A temporary home, prefix and release folder, with the client and data-folder variables cleared."""

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_flow_")
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.home = os.path.join(self.tmp, "home")
        self.prefix = os.path.join(self.tmp, "prefix")
        self.release = os.path.join(self.tmp, "release")
        for folder in (self.home, self.release):
            os.makedirs(folder)
        self.env = {k: v for k, v in os.environ.items()
                    if k not in ("CLAUDE_CONFIG_DIR", "CODEX_HOME", "XDG_DATA_HOME", "HOI4_LIVE_PREFIX",
                                 "HOI4_LIVE_COMMAND", "HOI4_LIVE_VERSION", "HOI4_LIVE_INSTALL_ARGS")}
        self.env.update({"HOI4_LIVE_RELEASE_BASE": self.release, "HOI4_LIVE_HOME": self.home, "HOME": self.home,
                         "LOCALAPPDATA": os.path.join(self.tmp, "localappdata"), "USERPROFILE": self.home})
        patcher = mock.patch.dict(os.environ, self.env, clear=True)
        patcher.start()
        self.addCleanup(patcher.stop)

    def publish(self, native=True, version=VERSION):
        """Fill the release folder with the real Python archive, stand-in native archives and the checksums."""
        archive, _digest, _count = build_release.build(self.release, allow_dirty=True)
        names = [os.path.basename(archive), "install.py"]
        shutil.copyfile(os.path.join(helpers.ROOT, "install.py"), os.path.join(self.release, "install.py"))
        if native:
            for target in ("macos-arm64", "linux-x86_64"):
                name = f"hoi4-live-native-{version}-{target}.tar.gz"
                with open(os.path.join(self.release, name), "wb") as f:
                    f.write(native_archive(version, target))
                names.append(name)
        self.write_sums(names)

    def write_sums(self, names):
        with open(os.path.join(self.release, install.SUMS_NAME), "w", encoding="utf-8", newline="\n") as f:
            for name in names:
                with open(os.path.join(self.release, name), "rb") as g:
                    f.write(f"{sha(g.read())}  {name}\n")

    def run_main(self, *argv):
        """install.main with a signature check that accepts the stand-in runtime. Returns (code, output)."""
        text = io.StringIO()
        with mock.patch.object(install, "signature_ok", return_value=True), \
                contextlib.redirect_stdout(text), contextlib.redirect_stderr(text):
            code = install.main(list(argv) + ["--prefix", self.prefix, "--home", self.home])
        return code, text.getvalue()

    def record(self):
        with open(os.path.join(self.prefix, "install.json"), encoding="utf-8") as f:
            return json.load(f)


class ReleaseTests(Sandbox):
    def test_locations_for_the_release_page_and_a_local_folder(self):
        self.assertEqual(install.asset_location("1.2.3", "a.zip"), os.path.join(self.release, "a.zip"))
        with mock.patch.dict(os.environ, {"HOI4_LIVE_RELEASE_BASE": "https://example.invalid/releases/"}):
            self.assertEqual(install.asset_location(None, "a.zip"),
                             "https://example.invalid/releases/latest/download/a.zip")
            self.assertEqual(install.asset_location("1.2.3", "a.zip"),
                             "https://example.invalid/releases/download/v1.2.3/a.zip")
        with mock.patch.dict(os.environ, {"HOI4_LIVE_RELEASE_BASE": ""}):
            self.assertTrue(install.asset_location(None, "a").startswith(
                "https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest/download/"))

    def test_only_https_and_local_files_are_fetched(self):
        for location in ("http://example.invalid/x", "ftp://example.invalid/x", "file:///etc/hosts"):
            with self.assertRaises(install.InstallError):
                install.fetch_url(location)
        with self.assertRaises(install.InstallError) as caught:
            install.fetch_url(os.path.join(self.release, "absent"))
        self.assertIn("could not download", str(caught.exception))

    def test_sums_name_the_latest_version_and_refuse_a_missing_one(self):
        self.assertEqual(install.parse_sums(f"{'a' * 64}  x.zip\n{'B' * 64} *y.tar.gz\nnot a line\n"),
                         {"x.zip": "a" * 64, "y.tar.gz": "b" * 64})
        with open(os.path.join(self.release, install.SUMS_NAME), "w", encoding="utf-8") as f:
            f.write(f"{'a' * 64}  hoi4-live-tools-9.8.7.zip\n{'b' * 64}  hoi4-live-native-9.8.7-linux-x86_64.tar.gz\n")
        sums, version = install.release_sums()
        self.assertEqual(version, "9.8.7")
        self.assertIn("hoi4-live-native-9.8.7-linux-x86_64.tar.gz", sums)
        with self.assertRaises(install.InstallError):
            install.release_sums("1.0.0")
        with open(os.path.join(self.release, install.SUMS_NAME), "w", encoding="utf-8") as f:
            f.write("nothing useful\n")
        with self.assertRaises(install.InstallError):
            install.release_sums()

    def test_a_download_is_refused_unless_its_checksum_matches(self):
        out = install.Output(quiet=True)
        with open(os.path.join(self.release, "a.bin"), "wb") as f:
            f.write(b"payload")
        self.assertEqual(install.fetch_checked("1.0.0", "a.bin", {"a.bin": sha(b"payload")}, out), b"payload")
        with self.assertRaises(install.InstallError) as caught:
            install.fetch_checked("1.0.0", "a.bin", {"a.bin": sha(b"other")}, out)
        self.assertIn("does not match its checksum", str(caught.exception))
        with self.assertRaises(install.InstallError) as caught:
            install.fetch_checked("1.0.0", "a.bin", {}, out)
        self.assertIn("not listed", str(caught.exception))

    def test_archives_cannot_write_outside_their_folder(self):
        for name in ("../evil.py", "/abs/evil.py", "top/../../evil.py", "C:/evil.py"):
            raw = io.BytesIO()
            with zipfile.ZipFile(raw, "w") as z:
                z.writestr(name, "x")
            with self.assertRaises(install.InstallError, msg=name):
                install.unpack_payload(raw.getvalue(), os.path.join(self.tmp, "out"))
        self.assertFalse(os.path.exists(os.path.join(self.tmp, "evil.py")))
        raw = io.BytesIO()
        with zipfile.ZipFile(raw, "w") as z:
            z.writestr("one/a.txt", "x")
            z.writestr("two/b.txt", "x")
        with self.assertRaises(install.InstallError):
            install.unpack_payload(raw.getvalue(), os.path.join(self.tmp, "out2"))

    def test_native_archive_takes_plain_files_of_one_folder_only(self):
        folder = os.path.join(self.tmp, "native")
        os.makedirs(folder)
        names = install.unpack_native(native_archive("1.0.0", "linux-x86_64"), folder)
        self.assertIn(install.NATIVE_BINARY, names)
        if os.name != "nt":
            self.assertTrue(os.access(os.path.join(folder, install.NATIVE_BINARY), os.X_OK))
        link = tarfile.TarInfo("hoi4-live-native-1.0.0-linux-x86_64/link")
        link.type, link.linkname = tarfile.SYMTYPE, "/etc/passwd"
        nested = tarfile.TarInfo("hoi4-live-native-1.0.0-linux-x86_64/deep/file")
        for bad in (native_archive("1.0.0", "linux-x86_64", extra=[link]),
                    native_archive("1.0.0", "linux-x86_64", extra=[nested]),
                    native_archive("1.0.0", "linux-x86_64", files={"LICENSE": b"x"}),
                    b"not an archive"):
            with self.assertRaises(install.InstallError) as caught:
                install.unpack_native(bad, folder)
            self.assertEqual(caught.exception.code, install.EXIT_NATIVE)


class SourceTests(unittest.TestCase):
    def test_install_py_compiles_without_a_warning_on_every_python(self):
        # A fresh copy is compiled by the user's Python; a warning there would land in the command's output.
        import warnings
        with open(os.path.join(helpers.ROOT, "install.py"), encoding="utf-8") as f:
            source = f.read()
        with warnings.catch_warnings():
            warnings.simplefilter("error")
            compile(source, "install.py", "exec")


class NativeTargetTests(unittest.TestCase):
    def test_each_computer_gets_its_build_or_a_reason(self):
        self.assertEqual(install.native_target("darwin", "arm64"), ("macos-arm64", None))
        self.assertEqual(install.native_target("linux", "x86_64"), ("linux-x86_64", None))
        self.assertEqual(install.native_target("linux", "AMD64"), ("linux-x86_64", None))
        for platform, machine, word in (("darwin", "x86_64", "Intel"), ("linux", "aarch64", "x86_64 Linux"),
                                        ("win32", "AMD64", "Windows")):
            target, reason = install.native_target(platform, machine)
            self.assertIsNone(target)
            self.assertIn(word, reason)

    def test_a_translated_python_on_apple_silicon_still_gets_the_arm_build(self):
        with mock.patch.object(install.host_platform, "machine", return_value="x86_64"), \
                mock.patch.object(install, "run_tool", return_value=(0, "1\n")):
            self.assertEqual(install.native_target("darwin"), ("macos-arm64", None))
        with mock.patch.object(install.host_platform, "machine", return_value="x86_64"), \
                mock.patch.object(install, "run_tool", return_value=(1, "unknown oid")):
            self.assertIsNone(install.native_target("darwin")[0])

    def test_linux_is_installed_but_has_no_python_server(self):
        with mock.patch.object(install.sys, "platform", "linux"), \
                mock.patch.dict(os.environ, {"HOME": "/home/a"}, clear=True), \
                mock.patch.object(install.os.path, "expanduser", return_value="/home/a"):
            install.check_platform()
            self.assertFalse(install.has_server())
            self.assertEqual(install.default_prefix(), os.path.join("/home/a", ".local", "share", "hoi4-live-tools"))
            self.assertEqual(install.launcher_name(), "hoi4-live-tools")
        with mock.patch.object(install.sys, "platform", "linux"), \
                mock.patch.dict(os.environ, {"XDG_DATA_HOME": "/data"}, clear=True):
            self.assertEqual(install.default_prefix(), os.path.join("/data", "hoi4-live-tools"))
        with mock.patch.object(install.sys, "platform", "sunos5"):
            with self.assertRaises(install.InstallError):
                install.check_platform()


class SignatureTests(unittest.TestCase):
    """macOS signing decisions; codesign and the runtime are stand-ins."""

    def tool(self, verify, entitled, after_sign=None):
        calls = []
        state = {"verify": verify, "entitled": entitled}

        def run(args, timeout=120):
            calls.append(list(args))
            if args[0].endswith("codesign") and "--verify" in args:
                return (0 if state["verify"] else 1), ""
            if args[0].endswith("codesign") and "--display" in args:
                return 0, ("[Key] com.apple.security.cs.debugger" if state["entitled"] else "no entitlements")
            if args[1:] == ["--doctor", "--sign"]:
                if after_sign:
                    state.update(after_sign)
                return 1, "signed, then the doctor listed a missing step"
            raise AssertionError(args)
        return run, calls

    def test_a_signed_entitled_runtime_is_left_alone(self):
        run, calls = self.tool(True, True)
        with mock.patch.object(install, "run_tool", run):
            self.assertEqual(install.ensure_signed("/p/hoi4-live-native", install.Output(True)), "verified")
        self.assertFalse([c for c in calls if "--sign" in c])

    def test_a_runtime_without_the_entitlement_is_signed_by_its_own_doctor_and_checked_again(self):
        for verify, entitled in ((True, False), (False, False), (False, True)):
            run, calls = self.tool(verify, entitled, after_sign={"verify": True, "entitled": True})
            with mock.patch.object(install, "run_tool", run):
                self.assertEqual(install.ensure_signed("/p/hoi4-live-native", install.Output(True)), "signed")
            self.assertIn(["/p/hoi4-live-native", "--doctor", "--sign"], calls)

    def test_a_signature_that_still_does_not_verify_fails_the_native_install(self):
        run, _calls = self.tool(True, False)
        with mock.patch.object(install, "run_tool", run):
            with self.assertRaises(install.InstallError) as caught:
                install.ensure_signed("/p/hoi4-live-native", install.Output(True))
        self.assertEqual(caught.exception.code, install.EXIT_NATIVE)
        self.assertIn("could not be signed", str(caught.exception))

    def test_a_helper_that_cannot_run_is_an_answer_not_a_crash(self):
        code, why = install.run_tool([os.path.join(tempfile.gettempdir(), "hoi4live-no-such-program")])
        self.assertIsNone(code)
        self.assertTrue(why)


class CommandTests(Sandbox):
    def test_the_command_runs_the_installed_installer_with_its_prefix(self):
        os.makedirs(self.prefix)
        path = install.write_command(self.prefix)
        with open(path, encoding="utf-8") as f:
            text = f.read()
        self.assertIn(os.path.join(self.prefix, "app", "install.py"), text)
        self.assertIn("HOI4_LIVE_COMMAND", text)
        self.assertIn(sys.executable, text)
        if os.name == "nt":
            self.assertTrue(path.endswith("hoi4-live.cmd"))
            self.assertTrue(text.rstrip().endswith("& exit /b"))
        else:
            self.assertTrue(os.access(path, os.X_OK))

    def test_no_argument_through_the_command_prints_help_and_installs_nothing(self):
        with mock.patch.dict(os.environ, {"HOI4_LIVE_COMMAND": "1"}):
            code, text = self.run_main()
        self.assertEqual(code, install.EXIT_USAGE)
        self.assertIn("doctor", text)
        self.assertFalse(os.path.exists(self.prefix))

    @unittest.skipIf(os.name == "nt", "POSIX link")
    def test_link_on_path_is_made_replaced_and_removed_only_when_ours(self):
        bin_dir = os.path.join(self.home, ".local", "bin")
        os.makedirs(self.prefix)
        install.write_command(self.prefix)
        self.assertEqual(install.link_command(self.prefix, self.home)["status"], "no_path_folder")
        with mock.patch.dict(os.environ, {"PATH": bin_dir + os.pathsep + os.environ.get("PATH", "")}):
            linked = install.link_command(self.prefix, self.home)
            self.assertEqual(linked, {"path": os.path.join(bin_dir, "hoi4-live"), "status": "linked"})
            self.assertEqual(os.readlink(linked["path"]), install.command_path(self.prefix))
            self.assertEqual(install.link_command(self.prefix, self.home)["status"], "linked")
            self.assertEqual(install.unlink_command({"command": {"link": linked["path"]}}), linked["path"])
            self.assertFalse(os.path.lexists(linked["path"]))
            with open(linked["path"], "w", encoding="utf-8") as f:
                f.write("#!/bin/sh\necho another program\n")
            self.assertEqual(install.link_command(self.prefix, self.home)["status"], "conflict")
            self.assertIsNone(install.unlink_command({"command": {"link": linked["path"]}}))
            self.assertTrue(os.path.exists(linked["path"]))

    @unittest.skipUnless(os.name == "nt", "Windows link")
    def test_windows_link_is_a_marked_forwarder_in_the_users_path_folder(self):
        apps = os.path.join(os.environ["LOCALAPPDATA"], "Microsoft", "WindowsApps")
        os.makedirs(self.prefix)
        install.write_command(self.prefix)
        self.assertEqual(install.link_command(self.prefix, self.home)["status"], "no_path_folder")
        os.makedirs(apps)
        with mock.patch.dict(os.environ, {"PATH": apps + os.pathsep + os.environ.get("PATH", "")}):
            linked = install.link_command(self.prefix, self.home)
            self.assertEqual(linked["status"], "linked")
            with open(linked["path"], encoding="utf-8") as f:
                self.assertIn(install.command_path(self.prefix), f.read())
            self.assertEqual(install.unlink_command({"command": {"link": linked["path"]}}), linked["path"])
            with open(linked["path"], "w", encoding="utf-8") as f:
                f.write("@echo another program\r\n")
            self.assertEqual(install.link_command(self.prefix, self.home)["status"], "conflict")


class FlowTests(Sandbox):
    """Install from a release folder, update in place, check, uninstall."""

    ARGS = ("--skip-dependencies", "--skip-verify", "--skip-selftest", "--register", "claude")

    def test_update_fetches_checks_installs_registers_and_uninstall_reverses_it(self):
        self.publish()
        code, text = self.run_main("update", *self.ARGS)
        self.assertEqual(code, install.EXIT_OK, text)
        record = self.record()
        self.assertEqual(record["version"], VERSION)
        self.assertTrue(os.path.isfile(os.path.join(self.prefix, "app", "hoi4_live", "__init__.py")))
        self.assertTrue(os.path.isfile(record["command"]["path"]))
        target, _reason = install.native_target()
        claude = os.path.join(self.home, ".claude.json")
        if target:
            self.assertEqual(record["native"]["status"], "installed")
            self.assertEqual(record["native"]["platform"], target)
            self.assertEqual(record["native"]["tools"], 2)
            self.assertTrue(os.path.isfile(install.native_binary(self.prefix)))
            self.assertTrue(os.path.isfile(os.path.join(self.prefix, "native", "debugger.entitlements")))
        else:
            self.assertEqual(record["native"]["status"], "unavailable")
        if install.has_server():
            with open(claude, encoding="utf-8") as f:
                entry = json.load(f)["mcpServers"]["hoi4-live"]
            self.assertEqual(entry["command"], install.launcher_path(self.prefix))
        else:
            self.assertFalse(os.path.exists(claude))
            self.assertIn("no client is registered on Linux yet", text)

        code, text = self.run_main("update", *self.ARGS)          # again: updates in place
        self.assertEqual(code, install.EXIT_OK, text)
        self.assertIn("upgrading from", text)
        if install.has_server():
            self.assertIn("claude: unchanged", text)

        code, text = self.run_main("uninstall")
        self.assertEqual(code, install.EXIT_OK, text)
        self.assertFalse(os.path.exists(self.prefix))
        if install.has_server():
            with open(claude, encoding="utf-8") as f:
                self.assertNotIn("hoi4-live", json.load(f)["mcpServers"])

    def test_a_tampered_python_archive_installs_nothing(self):
        self.publish()
        with open(os.path.join(self.release, f"hoi4-live-tools-{VERSION}.zip"), "ab") as f:
            f.write(b"tampered")
        code, text = self.run_main("update", *self.ARGS)
        self.assertEqual(code, install.EXIT_FAILED)
        self.assertIn("does not match its checksum", text)
        self.assertFalse(os.path.exists(os.path.join(self.prefix, "app")))

    def test_a_tampered_native_archive_is_refused_and_reported_while_the_rest_installs(self):
        target, _reason = install.native_target()
        if not target:
            self.skipTest("no native runtime is built for this computer")
        self.publish()
        with open(os.path.join(self.release, f"hoi4-live-native-{VERSION}-{target}.tar.gz"), "ab") as f:
            f.write(b"tampered")
        code, text = self.run_main("update", *self.ARGS)
        self.assertEqual(code, install.EXIT_NATIVE, text)
        self.assertIn("does not match its checksum", text)
        self.assertEqual(self.record()["native"]["status"], "failed")
        self.assertFalse(os.path.exists(install.native_binary(self.prefix)))
        self.assertTrue(os.path.isdir(os.path.join(self.prefix, "app")))

    def test_a_release_without_this_computers_native_build_says_so(self):
        target, _reason = install.native_target()
        if not target:
            self.skipTest("no native runtime is built for this computer")
        self.publish(native=False)
        code, text = self.run_main("update", *self.ARGS)
        self.assertEqual(code, install.EXIT_NATIVE)
        self.assertIn("not listed", text)

    def test_native_can_be_skipped_and_status_reports_the_install(self):
        self.publish(native=False)
        code, text = self.run_main("update", *self.ARGS, "--native", "skip")
        self.assertEqual(code, install.EXIT_OK, text)
        self.assertEqual(self.record()["native"], {"status": "skipped"})
        code, text = self.run_main("status")
        self.assertEqual(code, install.EXIT_OK)
        status = json.loads(text)
        self.assertTrue(status["installed"])
        self.assertEqual(status["version"], VERSION)
        self.assertEqual(status["command"]["path"], install.command_path(self.prefix))

    def test_doctor_needs_an_install_and_falls_back_to_the_native_check(self):
        code, text = self.run_main("doctor")
        self.assertEqual(code, install.EXIT_USAGE)
        self.assertIn("not installed", text)
        target, _reason = install.native_target()
        if not target or os.name == "nt":
            return
        os.makedirs(os.path.join(self.prefix, "app"))
        os.makedirs(os.path.join(self.prefix, "native"))
        with open(install.native_binary(self.prefix), "w", encoding="utf-8") as f:
            f.write(FAKE_NATIVE)
        os.chmod(install.native_binary(self.prefix), 0o755)
        with mock.patch.object(install, "has_server", return_value=False):
            code, text = self.run_main("doctor")
        self.assertEqual(code, install.EXIT_OK, text)
        self.assertIn("memory access is ready", text)


class StandaloneTests(Sandbox):
    """install.py on its own, and the one-line installers, against a release folder."""

    ARGS = ["--skip-dependencies", "--skip-verify", "--skip-selftest", "--register", "none", "--native", "skip"]

    def test_install_py_alone_downloads_the_release_it_needs(self):
        self.publish(native=False)
        alone = os.path.join(self.tmp, "alone")
        os.makedirs(alone)
        shutil.copyfile(os.path.join(helpers.ROOT, "install.py"), os.path.join(alone, "install.py"))
        result = subprocess.run([sys.executable, os.path.join(alone, "install.py"), "--prefix", self.prefix,
                                 "--home", self.home] + self.ARGS, env=self.env, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=300)
        text = result.stdout.decode("utf-8", errors="replace")
        self.assertEqual(result.returncode, 0, text)
        self.assertIn(f"downloading hoi4-live-tools-{VERSION}.zip", text)
        self.assertTrue(os.path.isfile(os.path.join(self.prefix, "app", "install.py")))
        # The installed command reaches the same installer: status through it reports this install.
        command = install.command_path(self.prefix)
        result = subprocess.run([command, "status"], env=self.env, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(json.loads(result.stdout.decode("utf-8"))["version"], VERSION)
        result = subprocess.run([command, "uninstall"], env=self.env, stdin=subprocess.DEVNULL,
                                stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=120)
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertFalse(os.path.exists(os.path.join(self.prefix, "app")))

    def one_liner(self):
        if os.name == "nt":
            env = dict(self.env, HOI4_LIVE_INSTALL_ARGS=" ".join(["--prefix", self.prefix, "--home", self.home]
                                                                  + self.ARGS))
            args = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
                    os.path.join(helpers.ROOT, "install.ps1")]
        else:
            env = self.env
            args = ["sh", os.path.join(helpers.ROOT, "install.sh"), "--prefix", self.prefix, "--home", self.home]
            args += self.ARGS
        result = subprocess.run(args, env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=300)
        return result.returncode, result.stdout.decode("utf-8", errors="replace")

    def test_one_line_installer_checks_install_py_then_runs_it(self):
        if " " in self.tmp:
            self.skipTest("the temporary folder has a space; the argument variable cannot carry it")
        self.publish(native=False)
        code, text = self.one_liner()
        self.assertEqual(code, 0, text)
        self.assertTrue(os.path.isfile(os.path.join(self.prefix, "app", "hoi4_live", "__init__.py")), text)

    def test_one_line_installer_refuses_an_install_py_that_fails_its_checksum(self):
        self.publish(native=False)
        with open(os.path.join(self.release, "install.py"), "ab") as f:
            f.write(b"\n# tampered\n")
        code, text = self.one_liner()
        self.assertNotEqual(code, 0)
        self.assertIn("does not match its checksum", text)
        self.assertFalse(os.path.exists(self.prefix))


if __name__ == "__main__":
    unittest.main()
