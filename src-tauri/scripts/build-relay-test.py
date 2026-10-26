#!/usr/bin/env python3
"""Build the real relay once before its nextest consumers start their clocks."""

import json
import os
from pathlib import Path
import subprocess


binary = os.environ.get("TUIC_RELAY_BIN")
if not binary:
    manifest = Path(__file__).resolve().parents[2] / "tools/relay/Cargo.toml"
    result = subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "--manifest-path",
            str(manifest),
            "--message-format=json-render-diagnostics",
        ],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if (
            message.get("reason") == "compiler-artifact"
            and message.get("target", {}).get("name") == "tuic-relay"
            and message.get("executable")
        ):
            binary = message["executable"]
    if not binary:
        raise RuntimeError("the relay build did not report its executable")

binary = Path(binary).resolve()
if not binary.is_file():
    raise RuntimeError(f"relay executable is missing: {binary}")
with open(os.environ["NEXTEST_ENV"], "a", encoding="utf-8") as output:
    output.write(f"TUIC_RELAY_BIN={binary}\n")
