#!/usr/bin/env python3
"""Catches: fixture setup dropping resolver-2 dev-dependency feature unification."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
CARGO = shutil.which("cargo")
assert CARGO, "Cargo is required for the graph regression"


def graph(command):
    """Ask Cargo to resolve the selection the captured builder actually made."""
    mode, *args = command
    assert mode in ("build", "test"), f"unsupported builder command: {command}"
    selection = []
    while args:
        arg = args.pop(0)
        if arg in ("--bin", "--profile"):
            args.pop(0)  # Target/profile do not affect feature resolution.
        elif arg == "--no-run":
            assert mode == "test"
        elif arg in ("--features", "-p", "--package"):
            selection.extend((arg, args.pop(0)))
        elif arg in ("--workspace", "--no-default-features"):
            selection.append(arg)
        else:
            raise AssertionError(f"unhandled graph-selection argument: {arg}")
    if "--workspace" not in selection and "-p" not in selection and "--package" not in selection:
        selection.extend(("-p", "tuicommander"))
    # Cargo test activates dev dependencies; a build profile named test does not.
    edges = "normal,build,dev" if mode == "test" else "normal,build"
    result = subprocess.run(
        [CARGO, "tree", "--locked", "--offline", *selection, "-e", edges,
         "--prefix", "none", "--format", "{p} features={f}"],
        cwd=ROOT / "src-tauri", check=True, capture_output=True, text=True,
    )
    entries = {line.removesuffix(" (*)") for line in result.stdout.splitlines() if " features=" in line}
    assert entries, "Cargo returned an empty feature graph"
    return entries


# Independent oracle: the desktop CI nextest command's workspace test selection.
expected = graph(["test", "--workspace", "--features", "dictation", "--no-run"])
for package, feature in (("tokio", "test-util"), ("tuic-core", "test-support"), ("tuic-git", "test-support")):
    assert any(line.startswith(package + " v") and feature in line.split(" features=", 1)[1].split(",")
               for line in expected), f"test graph lost {package}/{feature}"

with tempfile.TemporaryDirectory(prefix="fixture-build-graph.", dir=os.environ["TUIC_TEST_TMP_ROOT"]) as tmp:
    work = Path(tmp)
    bin_dir = work / "bin"
    bin_dir.mkdir()
    argv = work / "argv"
    proxy = bin_dir / "cargo"
    proxy.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$FIXTURE_BUILD_ARGV"\n')
    proxy.chmod(0o755)
    env = dict(os.environ, PATH=str(bin_dir) + os.pathsep + os.environ["PATH"],
               FIXTURE_BUILD_ARGV=str(argv), TUIC_FIXTURE_FEATURES="dictation",
               XDG_CACHE_HOME=str(work / "cache"), PSModuleAnalysisCachePath=str(work / "analysis"))
    builders = [("sh", ["sh", "scripts/build-fixture-bins.sh"])]
    if shutil.which("pwsh"):
        builders.append(("pwsh", ["pwsh", "-NoProfile", "-NonInteractive", "-File", "scripts/build-fixture-bins.ps1"]))
    else:
        print("PowerShell unavailable; its graph selection was not executed")
    for name, builder in builders:
        subprocess.run(builder, cwd=ROOT / "src-tauri", env=env, check=True)
        command = argv.read_text().splitlines()
        actual = graph(command)
        missing = sorted(expected - actual)
        extra = sorted(actual - expected)
        assert actual == expected, (
            f"{name} fixture graph differs from nextest: argv={command}; "
            f"missing={missing[:20]}; extra={extra[:20]}"
        )
        print(f"{name} fixture graph matches nextest: {len(actual)} package-feature entries; "
              "tokio/test-util and core/git test-support included")
