"""Exercise the Cargo runner with the real host shell, never the desktop app."""

import json
import os
from pathlib import Path
import shutil
import shlex
import subprocess
import sys
import tempfile
import unittest


RUNNER = Path(__file__).with_name("dev-exe-copy.py").resolve()
NAMES = ("tuicommander", "tuic-remote", "tuic-bridge", "tuic")


class DevExecutableTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory(dir=os.environ["TMPDIR"])
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.target = self.root / "Cargo view with spaces/debug"
        self.target.mkdir(parents=True)
        self.installed = self.root / "Application Support/dev-bin"
        self.shell = self.record_shell("/bin/sh")
        # Record host executable code; do not invent Cargo/mbx output.
        for name in NAMES:
            shutil.copy(self.shell, self.target / name)
        self.env = dict(os.environ, TUIC_DEV_BIN_DIR=str(self.installed), COPY_PROBE="kept")

    def record_shell(self, shell: str) -> Path:
        recorded = self.root / (Path(shell).name + "-recorded")
        shutil.copy(shell, recorded)
        if sys.platform == "darwin":
            # Relocated Apple platform binaries were killed with 137 on this host.
            # Match a Rust debug build's ad-hoc signature, without any keychain.
            subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-",
                            "--timestamp=none", str(recorded)],
                           check=True, capture_output=True, text=True)
        return recorded

    def command(self, script: str, *args: str) -> list[str]:
        return [sys.executable, "-B", str(RUNNER), str(self.target / NAMES[0]),
                "-c", script, "probe", *args]

    def test_make_dev_routes_only_macos_through_the_copy_runner(self) -> None:
        # Catches: an unwired runner, bad shell quoting, or changed instance scope.
        root = RUNNER.parent.parent
        result = subprocess.run(["make", "-n", "dev"], cwd=root,
                                env=dict(os.environ, MAKEFLAGS="", TUIC_APP_INSTANCE=""),
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        launch = next(line for line in result.stdout.splitlines() if "pnpm tauri dev" in line)
        args = shlex.split(launch)
        self.assertIn("TUIC_APP_INSTANCE=", args)
        self.assertIn("--no-watch", args)
        config_index = args.index("--config")
        self.assertEqual(args[config_index - 1], "--")
        key, value = args[config_index + 1].rsplit(".runner = ", 1)
        self.assertEqual(key, 'target."cfg(target_os = \\"macos\\")"')
        self.assertEqual(json.loads(value), ["python3", str(RUNNER)])

    def test_target_cleanup_cannot_remove_running_copy_or_its_siblings(self) -> None:
        # Catches: running from a target, or copying a link instead of real bytes.
        process = subprocess.Popen(
            self.command('printf "ready\\n"; read answer; test -x "$COPY_PATH"'),
            env=dict(self.env, COPY_PATH=str(self.installed / NAMES[0])),
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        self.addCleanup(self.stop, process)
        self.assertEqual(process.stdout.readline(), "ready\n")
        for name in NAMES:
            self.assertFalse((self.installed / name).is_symlink())
            self.assertEqual((self.installed / name).read_bytes(), self.shell.read_bytes())
        # Catches: a second invocation replacing a live executable's identity.
        second = subprocess.run(self.command("exit 0"), env=self.env, capture_output=True, text=True)
        self.assertEqual(second.returncode, 1)
        self.assertIn("already running", second.stderr)
        shutil.rmtree(self.target.parent)
        _, stderr = process.communicate("continue\n")
        self.assertEqual(process.returncode, 0, stderr)
        self.assertTrue((self.installed / NAMES[0]).is_file())

    def test_repeat_launch_keeps_path_argv_env_exit_and_no_revision_copies(self) -> None:
        # Catches: path churn, dropped args/env, swallowed exit, and stale locks.
        for shell in ("/bin/sh", "/bin/bash"):
            recorded = self.record_shell(shell)
            shutil.copy(recorded, self.target / NAMES[0])
            result = subprocess.run(
                self.command('printf "%s\\n%s\\n%s\\n%s\\n" "$1" "$2" "$COPY_PROBE" "$TUIC_DEV_BUILD_EXE"; exit 7',
                             "argument with spaces", "--flag"),
                env=self.env, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 7, result.stderr)
            self.assertEqual(result.stdout, f"argument with spaces\n--flag\nkept\n{self.target / NAMES[0]}\n")
            self.assertIn(str(self.installed / NAMES[0]), result.stderr)
            self.assertEqual((self.installed / NAMES[0]).read_bytes(), recorded.read_bytes())
            self.assertEqual(set(path.name for path in self.installed.iterdir()), {*NAMES, ".lock"})

    def test_symlinked_cargo_artifact_uses_view_siblings_and_missing_sibling_fails_closed(self) -> None:
        # Catches: resolving a Cargo view symlink and searching in its object store.
        source = self.target / NAMES[0]
        source.unlink()
        source.symlink_to(self.shell)
        good = subprocess.run(self.command("exit 0"), env=self.env, capture_output=True, text=True)
        self.assertEqual(good.returncode, 0, good.stderr)
        # Catches: silently launching an old installed revision after copy failure.
        (self.target / "tuic-bridge").unlink()
        bad = subprocess.run(self.command("echo SHOULD-NOT-RUN"), env=self.env,
                             capture_output=True, text=True)
        self.assertEqual(bad.returncode, 1)
        self.assertEqual(bad.stdout, "")
        self.assertIn("missing executable", bad.stderr)

    @staticmethod
    def stop(process: subprocess.Popen) -> None:
        if process.poll() is None:
            process.communicate("cleanup\n")
        for stream in (process.stdin, process.stdout, process.stderr):
            stream.close()


if __name__ == "__main__":
    unittest.main()
