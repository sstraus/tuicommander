"""Execute release-note generation without publishing. Optional argument: git ref."""

from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[3]
REF = sys.argv.pop(1) if len(sys.argv) > 1 else None
VERSION = "2.0.0-beta.1"
# Recorded at 8ff1d6475; extended with the ARM64 download legend contract.
# Kept independent of the workflow output, not re-derived in CI.
SHORT_BODY = Path(__file__).with_name("release-body-short.md").read_bytes()


def source(ref=None):
    if ref:
        return subprocess.check_output(
            ["git", "show", f"{ref}:.github/workflows/release.yml"], cwd=ROOT, text=True
        )
    return (ROOT / ".github/workflows/release.yml").read_text()


def build_body(workflow, changelog):
    step = workflow.split("      - name: Build release notes\n", 1)[1]
    script = textwrap.dedent(step.split("        run: |\n", 1)[1].split(
        "      - name:", 1
    )[0])
    # Stop before the publication boundary: never invoke gh, even in a test.
    script = script.split('# A stable release leaves "latest"', 1)[0]
    scratch = ROOT / ".tmp/tuic-tests"
    scratch.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=scratch) as directory:
        work = Path(directory)
        (work / "src-tauri").mkdir()
        (work / "src-tauri/tauri.conf.json").write_text('{"version":"' + VERSION + '"}')
        (work / "CHANGELOG.md").write_text(changelog)
        subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], cwd=work,
                       check=True, capture_output=True, text=True)
        return (work / ".tmp/tuic-tests/release-body.md").read_bytes()


class ReleaseBodyContracts(unittest.TestCase):
    def test_current_beta_changelog_cannot_exceed_github_limit(self):
        # Catches: the full beta changelog makes gh release edit fail with HTTP 422.
        body = build_body(source(REF), (ROOT / "CHANGELOG.md").read_text())
        print(f"{VERSION}: {len(body.decode('utf-8'))} characters, {len(body)} bytes")
        self.assertLess(len(body.decode("utf-8")), 125000)

    def test_short_and_missing_sections_stay_byte_identical(self):
        # Catches: bounding rewrites short notes or removes the missing-section fallback.
        for section in ["A short change.\n", "", "é 🍎\n"]:
            with self.subTest(section=section):
                changelog = f"## [{VERSION}]\n{section}\n## [1.0.0]\nOld\n"
                self.assertEqual(build_body(source(REF), changelog),
                                 SHORT_BODY.replace(b"A short change.\n", section.encode("utf-8")))
        fallback = ("See [CHANGELOG.md](https://github.com/sstraus/tuicommander/"
                    "blob/main/CHANGELOG.md) for details.\n").encode("utf-8")
        self.assertEqual(build_body(source(REF), "## [1.0.0]\nOld\n"),
                         SHORT_BODY.replace(b"A short change.\n\n", fallback))

    def test_oversized_lines_and_unicode_keep_footer_and_complete_lines(self):
        # Catches: byte slicing splits Unicode/lines, or a total-body cap loses installers.
        for line in ["- ordinary change\n", "- 🍎 café 漢字\n", "x" * 130000 + "\n"]:
            with self.subTest(line=line[:30]):
                section = "### Features\n" + line * (1 if len(line) > 125000 else 10000)
                changelog = f"## [{VERSION}]\n{section}\n## [1.0.0]\nOld\n"
                body = build_body(source(REF), changelog)
                text = body.decode("utf-8")
                self.assertLess(len(body), 125000)
                self.assertIn(f"/blob/v{VERSION}/CHANGELOG.md", text)
                footer = SHORT_BODY[SHORT_BODY.rindex("\n---\n\n📦".encode()):]
                self.assertTrue(body.endswith(footer))
                retained = text.split("\n\n[Full changelog]", 1)[0]
                self.assertTrue(retained.startswith("## What's Changed\n\n"))
                self.assertTrue(("## What's Changed\n\n" + section + "\n\n").startswith(retained))
                self.assertTrue(retained.endswith("\n"))


if __name__ == "__main__":
    unittest.main(verbosity=2)
