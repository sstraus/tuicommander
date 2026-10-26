"""Frontend CI contracts. Pass a git ref to check the pre-fix workflow."""

from itertools import product
from pathlib import Path
import os
import re
import subprocess
import sys
import unittest


ROOT = Path(__file__).resolve().parents[3]
REF = sys.argv.pop(1) if len(sys.argv) > 1 else None
SOURCE = (
    subprocess.check_output(["git", "show", f"{REF}:.github/workflows/ci.yml"], cwd=ROOT, text=True)
    if REF else (ROOT / ".github/workflows/ci.yml").read_text()
)


def job(name):
    match = re.search(rf"^  {name}:\n(.*?)(?=^  [\w-]+:|\Z)", SOURCE, re.M | re.S)
    if not match:
        raise AssertionError(f"Missing CI job: {name}")
    return match.group(1)


class FrontendShardContracts(unittest.TestCase):
    def test_all_four_partitions_run_without_fail_fast_or_selection_filters(self):
        # Catches: one runner still owns the whole suite, or a shard is omitted.
        tests = job("frontend-tests")
        self.assertIn("fail-fast: false", tests)
        self.assertRegex(tests, r"shard: \[1, 2, 3, 4\]")
        command = re.search(r"(?m)^        run: (.*vitest run.*)$", tests).group(1)
        self.assertEqual(
            command,
            "bash scripts/with-test-tmp.sh pnpm exec vitest run --reporter=default --shard=${{ matrix.shard }}/4",
        )
        self.assertNotIn("continue-on-error", tests)

    def test_existing_required_check_rejects_every_non_success_dependency(self):
        # Catches: a skipped, cancelled or failed shard/check yields a green gate.
        gate = job("frontend")
        self.assertIn("name: Frontend (typecheck + tests)", gate)
        self.assertIn("needs: [frontend-checks, frontend-tests]", gate)
        self.assertIn("if: always()", gate)
        self.assertIn("CHECKS_RESULT: ${{ needs.frontend-checks.result }}", gate)
        self.assertIn("TESTS_RESULT: ${{ needs.frontend-tests.result }}", gate)
        self.assertNotIn("continue-on-error", gate)
        command = re.search(r"(?m)^        run: (.*)$", gate).group(1)
        for checks, tests in product(["success", "failure", "cancelled", "skipped"], repeat=2):
            with self.subTest(checks=checks, tests=tests):
                result = subprocess.run(
                    ["bash", "-e", "-c", command],
                    env={**os.environ, "CHECKS_RESULT": checks, "TESTS_RESULT": tests},
                    capture_output=True,
                )
                self.assertEqual(result.returncode == 0, checks == tests == "success")

    def test_non_vitest_checks_still_gate_the_frontend(self):
        # Catches: splitting the job drops typecheck, shell or workflow contracts.
        checks = job("frontend-checks")
        for command in [
            "pnpm exec tsc --noEmit", "make test-shell",
            "python3 .github/workflows/tests/test-release-body-limit.py",
            "python3 .github/workflows/tests/test-remote-daemon-release.py",
            "python3 .github/workflows/tests/test-frontend-shards.py",
            "node scripts/third-party-notices.mjs --check --only js",
            "xargs git ls-files --error-unmatch",
        ]:
            with self.subTest(command=command):
                self.assertIn(command, checks)
        self.assertNotIn("continue-on-error", checks)


if __name__ == "__main__":
    unittest.main(verbosity=2)
