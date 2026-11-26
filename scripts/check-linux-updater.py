#!/usr/bin/env python3
"""Reject incomplete or cross-architecture desktop updater feeds before publication."""
import json
from pathlib import Path
import sys


def verify(path):
    platforms = json.loads(Path(path).read_text())["platforms"]
    targets = [
        ("darwin-aarch64", "_aarch64.app.tar.gz"),
        ("darwin-aarch64-app", "_aarch64.app.tar.gz"),
        ("windows-x86_64", "_x64-setup.exe"),
        ("windows-x86_64-nsis", "_x64-setup.exe"),
    ]
    for arch, appimage, deb, rpm in (
        ("x86_64", "amd64", "amd64", "x86_64"),
        ("aarch64", "aarch64", "arm64", "aarch64"),
    ):
        targets.extend((
            (f"linux-{arch}", f"_{appimage}.AppImage"),
            (f"linux-{arch}-appimage", f"_{appimage}.AppImage"),
            (f"linux-{arch}-deb", f"_{deb}.deb"),
            (f"linux-{arch}-rpm", f".{rpm}.rpm"),
        ))
    # Parallel tauri-action writers can drop other platforms during their merge.
    # Check the complete supported set, not just the new Linux architecture.
    for target, suffix in targets:
        entry = platforms[target]
        signature, url = entry["signature"], entry["url"]
        if not isinstance(signature, str) or not signature.strip() or not isinstance(url, str) or not url.endswith(suffix):
            raise ValueError(f"Invalid signature or architecture for {target}")
    print("Verified complete macOS ARM64, Windows x64 and Linux x64/ARM64 updater targets")


if __name__ == "__main__":
    verify(sys.argv[1])
