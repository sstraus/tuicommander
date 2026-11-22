#!/usr/bin/env python3
"""Catch shell launchers dropping colon-bearing Cargo metadata on native ARM64.

Run inside the task container with a fresh, short task directory, retained Rust
on PATH, and task-local old/fixed mbx binaries. No application build is run.
"""

import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys


KEYS = [
    "DEP_TAURI_CORE:WEBVIEW__CORE_PLUGIN___PERMISSION_FILES_PATH",
    "DEP_TAURI_CORE:WINDOW__CORE_PLUGIN___PERMISSION_FILES_PATH",
    "DEP_TAURI_CORE__CORE_PLUGIN___PERMISSION_FILES_PATH",
    "DEP_TAURI_PLUGIN_DIALOG_PERMISSION_FILES_PATH",
]


def run(*args, **kwargs):
    print("COMMAND", json.dumps(args), flush=True)
    return subprocess.run(args, check=True, text=True, **kwargs)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    old, fixed, root = map(Path, sys.argv[1:])
    assert platform.system() == "Linux" and platform.machine() == "aarch64"
    assert not root.exists(), "Use a new task directory; retain prior evidence."
    root.mkdir(parents=True)
    os.environ.update(
        HOME=str(root / "h"),
        XDG_CONFIG_HOME=str(root / "h/cfg"),
        XDG_DATA_HOME=str(root / "h/data"),
        XDG_CACHE_HOME=str(root / "h/cache"),
        TMPDIR=str(root / "tmp"),
        PATH=str(root / "h/data/mbx/bin") + ":" + os.environ["PATH"],
    )
    (root / "tmp").mkdir()
    crate = root / "fixture"
    crate.mkdir()
    (crate / "Cargo.toml").write_text(
        '[package]\nname="mbx-env-fixture"\nversion="0.0.0"\n'
        'edition="2021"\n[lib]\npath="lib.rs"\n[workspace]\n'
    )
    (crate / "lib.rs").write_text("pub fn fixture() {}\n")
    build = crate / "build.rs"
    build.write_text(
        "fn main() {\n"
        ' if std::env::var_os("MBX_ENV_SENTINELS").is_some() {\n'
        + "".join(
            f'  assert_eq!(std::env::var("{key}").as_deref(), Ok("sentinel-{i}"));\n'
            f'  println!("{key}=sentinel-{i}");\n'
            for i, key in enumerate(KEYS)
        )
        + " }\n}\n"
    )
    os.chdir(crate)
    for tool in (old, fixed):
        run(str(tool), "--version")
    run("rustc", "--version")
    for key, value in (("cache_dir", str(root / "c")), ("target.views", "true")):
        run(str(old), "settings", "set", key, value)
    run(str(old), "setup", "--yes")
    run(str(old), "doctor")
    run(str(old), "build", "--offline")
    launchers = list((crate / "target/debug/build").glob("*/build-script-build"))
    assert len(launchers) == 1, launchers
    launcher = launchers[0].resolve()
    before = launcher.read_text()
    assert before.startswith("#!/bin/sh\n"), before
    print("OLD_LAUNCHER", launcher, sha(launcher), repr(before), flush=True)
    # Preserve the entire tiny fixture view, including its old pinned tool.
    shutil.copytree((crate / "target").resolve(), root / "old-view", symlinks=True)
    # Upstream's moved-profile repair does not convert old shell launchers.
    # A source change makes Cargo regenerate this fixture's exact launcher.
    build.write_text(build.read_text() + "// Regenerate the fixture launcher.\n")
    run(str(fixed), "setup", "--yes")
    run(str(fixed), "doctor")
    run(str(fixed), "build", "--offline")
    after = launcher.read_text()
    interpreter = after.removeprefix("#!").strip()
    assert after.startswith("#!/") and after.count("\n") == 1, after
    assert not any(c.isspace() for c in interpreter), interpreter
    assert len(after.rstrip("\n").encode()) <= 127, after
    assert sha(Path(interpreter)) == sha(fixed), "Launcher must pin the fixed tool."
    env = dict(os.environ, MBX_ENV_SENTINELS="1")
    env.update({key: f"sentinel-{i}" for i, key in enumerate(KEYS)})
    child = run(str(launcher), env=env, capture_output=True)
    assert child.stderr == "", child.stderr
    assert child.stdout.splitlines() == [f"{k}=sentinel-{i}" for i, k in enumerate(KEYS)]
    result = {
        "machine": platform.machine(),
        "launcher": str(launcher),
        "old_launcher_sha256": hashlib.sha256(before.encode()).hexdigest(),
        "launcher_sha256": sha(launcher),
        "shebang": after.strip(),
        "shebang_bytes": len(after.rstrip("\n").encode()),
        "fixed_binary_sha256": sha(fixed),
        "pinned_binary_sha256": sha(Path(interpreter)),
        "controls": child.stdout.splitlines(),
        "result": "PASS",
    }
    (root / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2), flush=True)


if __name__ == "__main__":
    main()
