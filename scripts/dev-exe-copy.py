#!/usr/bin/env python3
"""Cargo runner for make dev on macOS: keep executable paths outside targets."""

import fcntl
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys


def run(source: Path, args: list[str]) -> int:
    directory = Path(os.environ.get(
        "TUIC_DEV_BIN_DIR",
        str(Path.home() / "Library/Application Support/com.tuic.commander/dev-bin"),
    )).expanduser().resolve()
    directory.mkdir(parents=True, exist_ok=True)
    # Keep the lock in this supervisor, not the app: app startup may close FDs.
    # Another make dev must not replace the path while its process is alive.
    with (directory / ".lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RuntimeError(f"dev executable already running from {directory}") from None

        sources = [source, *(source.parent / name for name in (
            "tuic-remote", "tuic-bridge", "tuic",
        ))]
        # These are built by make dev and discovered relative to current_exe.
        # Refuse a partial install rather than silently using old siblings.
        for binary in sources:
            if not binary.is_file() or not os.access(binary, os.X_OK):
                raise RuntimeError(f"missing executable: {binary}; run make dev")
        for binary in sources:
            destination = directory / binary.name
            staging = directory / f".{binary.name}.new"
            try:
                shutil.copy(binary, staging)
                staging.replace(destination)
            finally:
                staging.unlink(missing_ok=True)

        print(f"Starting dev executable: {directory / source.name}", file=sys.stderr)
        child = subprocess.Popen(
            [str(directory / source.name), *args],
            env={**os.environ, "TUIC_DEV_BUILD_EXE": str(source)},
        )

        def forward(signum: int, _frame: object) -> None:
            try:
                child.send_signal(signum)
            except ProcessLookupError:
                pass

        for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(signum, forward)
        status = child.wait()
        return status if status >= 0 else 128 - status


if __name__ == "__main__":
    try:
        if len(sys.argv) < 2:
            raise RuntimeError("usage: dev-exe-copy.py <Cargo executable> [args...]")
        sys.exit(run(Path(sys.argv[1]).absolute(), sys.argv[2:]))
    except (OSError, RuntimeError) as error:
        print(f"dev executable: {error}", file=sys.stderr)
        sys.exit(1)
