"""Native Linux desktop workflow contracts, independent of a cross-build."""
from pathlib import Path
import unittest
import importlib.util
import json
import tempfile
from contextlib import redirect_stdout
import io
import copy

ROOT = Path(__file__).resolve().parents[3]


class LinuxDesktopRelease(unittest.TestCase):
    def test_arm64_uses_native_runner_and_native_vulkan_tools(self):
        # Catches: Ubuntu 22.04 apt failure or the SDK action's x64 glslc on ARM64.
        for name in ("release", "nightly"):
            with self.subTest(workflow=name):
                source = (ROOT / f".github/workflows/{name}.yml").read_text()
                self.assertIn("platform: ubuntu-22.04-arm\n            args: '--features vulkan'\n            label: Linux-ARM64", source)
                self.assertIn("if: startsWith(matrix.platform, 'ubuntu-')", source)
                dependencies = source.split("- name: Install Linux dependencies", 1)[1].split("\n      - name:", 1)[0]
                self.assertIn("libvulkan-dev rpm", dependencies)
                self.assertNotIn(" glslc ", dependencies)
                native = source.split("- name: Configure native ARM64 Vulkan tools", 1)[1].split("\n      - name:", 1)[0]
                self.assertIn("if: matrix.label == 'Linux-ARM64'", native)
                self.assertIn("bash scripts/build-linux-arm64-glslc.sh", native)
                self.assertIn("cmake ninja-build python-is-python3", native)
                self.assertIn('echo "$RUNNER_TEMP/glslc/bin" >> "$GITHUB_PATH"', native)
                self.assertIn("matrix.platform != 'macos-latest' && matrix.label != 'Linux-ARM64'", source)
                self.assertIn('echo "VULKAN_SDK=$RUNNER_TEMP/glslc"', source)
                self.assertIn("matrix.label == 'Linux-ARM64' && '-fPIC -march=armv8-a'", source)

    def test_native_payload_gate_covers_both_linux_architectures(self):
        # Catches: an ARM build reaches publication without ELF/package checks.
        for name in ("release", "nightly"):
            with self.subTest(workflow=name):
                source = (ROOT / f".github/workflows/{name}.yml").read_text()
                gate = source.split("- name: Verify native Linux desktop packages and publish checksums", 1)[1].split("\n      - ", 1)[0]
                self.assertIn("if: startsWith(matrix.label, 'Linux-')", gate)
                self.assertIn("bash scripts/check-linux-desktop-packages.sh", gate)
                self.assertIn("SHA256SUMS-${{ matrix.label }}.txt", gate)
                self.assertIn("if: matrix.label == 'Linux-x64' || matrix.label == 'Windows-x64'", source)
                self.assertIn("label: macOS-ARM", source)
                self.assertIn("label: Windows-x64", source)
                self.assertIn("label: Linux-x64", source)

    def test_linux_dependencies_supply_rpm_extractor(self):
        # Catches: bsdtar works locally but is missing on a release/nightly runner.
        for name in ("release", "nightly"):
            with self.subTest(workflow=name):
                source = (ROOT / f".github/workflows/{name}.yml").read_text()
                dependencies = source.split("- name: Install Linux dependencies", 1)[1].split("\n      - name:", 1)[0]
                apt = next(line for line in dependencies.splitlines() if "apt-get install -y" in line).split()
                self.assertIn("libarchive-tools", apt)
                self.assertIn("rpm", apt)

    def test_rpm_extraction_uses_bsdtar_without_ignoring_failure(self):
        # Catches: rpm2cpio rejects a complete payload with absent optional size fields.
        source = (ROOT / "scripts/check-linux-desktop-packages.sh").read_text()
        self.assertIn('(cd "$work/rpm" && bsdtar -xf "${rpms[0]}")', source.splitlines())

    def test_extractor_change_retains_fail_closed_package_checks(self):
        # Catches: the extractor fix drops or suppresses metadata/ELF/signature/checksum gates.
        source = (ROOT / "scripts/check-linux-desktop-packages.sh").read_text()
        for check in (
            'set -euo pipefail',
            'test "$(dpkg-deb -f "${debs[0]}" Architecture)" = "$deb_arch"',
            'test "$(rpm -qp --queryformat \'%{ARCH}\' "${rpms[0]}")" = "$rpm_arch"',
            'dpkg-deb -f "${debs[0]}" Depends | grep -F libvulkan1 >/dev/null',
            'rpm -qp --requires "${rpms[0]}" | grep -F libvulkan.so.1 >/dev/null',
            '  test -x "$binary"\n  readelf -h "$binary" | grep -F "Machine:" | grep -F "$machine" >/dev/null',
            'for binary in tuicommander tuic tuic-bridge; do\n  check_elf "$release/$binary"',
            'for payload in deb rpm squashfs-root; do\n  for binary in tuicommander tuic tuic-bridge; do',
            '    test -n "$found"\n    check_elf "$found"',
            'test -s "${images[0]}.sig"',
            '(cd "$release/bundle/deb" && sha256sum -- *.deb; cd ../rpm && sha256sum -- *.rpm; cd ../appimage && sha256sum -- *.AppImage) > "$release/bundle/SHA256SUMS-${label}.txt"',
        ):
            with self.subTest(check=check):
                self.assertIn(check, source)
        self.assertNotRegex(source, r'\|\||\bset\s+\+')


class LinuxUpdaterSelection(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location("linux_updater", ROOT / "scripts/check-linux-updater.py")
        cls.module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.module)

    def setUp(self):
        self.data = json.loads((ROOT / "src-tauri/test-fixtures/updater/stable-manifest.json").read_text())
        # Extend the recorded manifest with explicit schema-level ARM64 mutations.
        # Copied signatures are opaque test data, NOT native package/signature proof.
        for suffix in ("", "-appimage", "-deb", "-rpm"):
            entry = copy.deepcopy(self.data["platforms"][f"linux-x86_64{suffix}"])
            entry["url"] = entry["url"].replace("amd64.deb", "arm64.deb").replace("amd64.AppImage", "aarch64.AppImage").replace("x86_64.rpm", "aarch64.rpm")
            self.data["platforms"][f"linux-aarch64{suffix}"] = entry

    def verify(self, data):
        root = ROOT / ".tmp/tuic-tests"
        root.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=root) as directory:
            path = Path(directory) / "latest.json"
            path.write_text(json.dumps(data))
            with redirect_stdout(io.StringIO()):
                self.module.verify(path)

    def test_missing_arm64_feed_never_falls_back_to_x64(self):
        # Catches: a completed desktop matrix publishes an x64-only updater feed.
        data = json.loads((ROOT / "src-tauri/test-fixtures/updater/stable-manifest.json").read_text())
        with self.assertRaises(KeyError):
            self.verify(data)

    def test_arm64_target_cannot_point_to_x64_package(self):
        # Catches: an ARM64 key disguises a signed x64 artifact URL.
        self.data["platforms"]["linux-aarch64"] = self.data["platforms"]["linux-x86_64"]
        with self.assertRaises(ValueError):
            self.verify(self.data)

    def test_dropped_macos_or_windows_entries_block_finalization(self):
        # Catches: parallel latest.json writers lose non-Linux updater entries.
        for target in ("darwin-aarch64", "darwin-aarch64-app", "windows-x86_64", "windows-x86_64-nsis"):
            with self.subTest(target=target):
                data = copy.deepcopy(self.data)
                del data["platforms"][target]
                with self.assertRaises(KeyError):
                    self.verify(data)

    def test_complete_supported_platform_set_is_accepted(self):
        # Catches: completeness validation blocks a correctly shaped full feed.
        self.verify(self.data)

    def test_each_platform_requires_native_url_and_nonblank_signature(self):
        # Catches: preserved platform keys hide unusable signatures or wrong installers.
        for target in self.data["platforms"]:
            for field, value in (("url", "https://example.invalid/wrong-architecture.zip"), ("signature", ""), ("signature", "   ")):
                with self.subTest(target=target, field=field, value=value):
                    data = copy.deepcopy(self.data)
                    data["platforms"][target][field] = value
                    with self.assertRaises(ValueError):
                        self.verify(data)


if __name__ == "__main__":
    unittest.main(verbosity=2)
