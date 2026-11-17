"""Exercise the workflow shell against a release store, without GitHub writes.

Assumptions: gh release upload requires an existing release (gh CLI manual).
The tauri-action v0 source runs buildProject before getOrCreateRelease:
https://github.com/tauri-apps/tauri-action/blob/v0/src/index.ts
Thus all desktop builds failing leaves no desktop-created release.
Run: python3 tests/ci/test_nightly_daemon_publication.py
"""

import os
from pathlib import Path
import re
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]


def shell_steps(path, job):
    source = path.read_text()
    body = re.search(rf"^  {job}:\n(.*?)(?=^  [\w-]+:|\Z)", source, re.M | re.S)[1]
    for step in re.split(r"^      - ", body, flags=re.M)[1:]:
        run = re.search(r"^        run: \|\n((?:          .*\n|\n)+)", step, re.M)
        if run:
            yield step.splitlines()[0], textwrap.dedent(run[1])


class NightlyDaemonPublication(unittest.TestCase):
    # Catches: cleanup removes the release and successful daemon builds cannot
    # publish anything when every desktop build fails before tauri-action creates it.
    def test_successful_daemon_publishes_when_all_desktop_builds_fail(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            store = work / "release"
            store.mkdir()
            (store / "stale-daemon").write_text("old")
            target = "x86_64-unknown-linux-gnu"
            binaries = work / "src-tauri" / "target" / target / "release"
            binaries.mkdir(parents=True)
            for binary in ("tuic-remote", "tuic-bridge"):
                (binaries / binary).write_text("new daemon build")
            # External CLI model: upload never creates a release. sleep advances
            # the retry schedule immediately; desktop failure is permanent.
            prelude = r'''
            gh() {
              case "$1 $2" in
                'release delete') rm -rf "$RELEASE_STORE" ;;
                'release create') mkdir -p "$RELEASE_STORE" ;;
                'release view') test -d "$RELEASE_STORE" ;;
                'release upload')
                  test -d "$RELEASE_STORE" || return 1
                  shift 3
                  for asset in "$@"; do
                    case "$asset" in --*) ;; *) cp "$asset" "$RELEASE_STORE/" ;; esac
                  done ;;
                *) echo "Unsupported gh operation: $*" >&2; return 2 ;;
              esac
            }
            sleep() { :; }
            '''
            environment = dict(os.environ, RELEASE_STORE=str(store), GITHUB_REPOSITORY="owner/repo")
            for name, script in shell_steps(ROOT / ".github/workflows/nightly.yml", "cleanup"):
                if "Move nightly tag" not in name:
                    cleanup = subprocess.run(["bash", "-e", "-c", textwrap.dedent(prelude) + script],
                                             cwd=work, env=environment, capture_output=True, text=True)
                    self.assertEqual(cleanup.returncode, 0, cleanup.stderr)
            upload = next(script for name, script in shell_steps(
                ROOT / ".github/workflows/remote-daemon.yml", "remote-daemon") if "Upload to release" in name)
            for expression, value in (("inputs.nightly", "true"), ("matrix.target", target), ("matrix.ext", "")):
                upload = upload.replace("${{ " + expression + " }}", value)
            result = subprocess.run(["bash", "-e", "-c", textwrap.dedent(prelude) + upload],
                                    cwd=work, env=environment, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0,
                             "Successful daemon build lost because desktop never created nightly release:\n" + result.stdout + result.stderr)
            self.assertEqual(sorted(path.name for path in store.iterdir()),
                             ["tuic-bridge-" + target, "tuic-remote-" + target])


if __name__ == "__main__":
    unittest.main()
