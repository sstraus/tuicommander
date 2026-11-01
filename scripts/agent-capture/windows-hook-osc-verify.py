#!/usr/bin/env python3
"""Verify recorded runner evidence, not TUIC parser or lifecycle behavior."""
# DEFERRED (2026-11-01) — production accumulator replay and installed-hook
# lifecycle coverage belong to the broader story, not this runner transport probe.
import hashlib
import json
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
results = []
for terminator in ("bel", "st"):
    end = b"\x07" if terminator == "bel" else b"\x1b\\"
    expected = b"".join(
        b"\x1b]7770;state=" + state + end
        for state in (b"busy", b"awaiting", b"prompt", b"idle")
    )
    for stdout in ("inherit", "pipe"):
        case = f"{terminator}-{stdout}"
        directory = root / case
        raw = (directory / "conpty.bin").read_bytes()
        request = (directory / "write-request.bin").read_bytes()
        assert request == b"HOOK-CONSOLE-BEGIN\r\n" + expected + b"HOOK-CONSOLE-END\r\n", case
        # Catches OSC loss, duplication, mutation, terminator conversion and reordering.
        osc = re.findall(rb"\x1b\]7770;[^\x07\x1b]*(?:\x07|\x1b\\)", raw)
        assert b"".join(osc) == expected, case
        assert raw.count(expected) == 1, case
        assert b"HOOK-CONSOLE-BEGIN\r\nHOOK-CONSOLE-END\r\n" in raw, case
        assert (directory / "terminal-input.bin").read_bytes() == b"\x1b[1;1R", case
        assert "console_mode_ok=1 console_mode=0x7 error=None" in (directory / "hook.txt").read_text(), case
        assert "code: 0" in (directory / "status.txt").read_text(), case
        offset = 0
        prior_time = 0
        frames = []
        for line in (directory / "frames.tsv").read_text().splitlines():
            timestamp, start, length = map(int, line.split("\t"))
            assert timestamp >= prior_time and start == offset and length > 0, case
            frames.append(raw[start:start + length])
            offset += length
            prior_time = timestamp
        assert offset == len(raw) and b"".join(frames) == raw, case
        if stdout == "pipe":
            assert (directory / "hook-stdout.bin").read_bytes() == b"HOOK-STDOUT-CONTROL\n", case
            assert (directory / "hook-stderr.bin").read_bytes() == b"", case
            assert b"HOOK-STDOUT-CONTROL" not in raw, case
        else:
            assert b"HOOK-STDOUT-CONTROL\r\n" in raw, case
        results.append({
            "case": case,
            "raw_bytes": len(raw),
            "raw_sha256": hashlib.sha256(raw).hexdigest(),
            "osc_bytes": len(expected),
            "osc_hex": expected.hex(),
            "osc_offset": raw.index(expected),
            "osc_decision": "unchanged",
            "whole_stream_decision": "changed: OSC precedes earlier printable text",
            "reader_frames": len(frames),
        })
print(json.dumps(results, indent=2))
