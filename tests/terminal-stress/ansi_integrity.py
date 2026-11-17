#!/usr/bin/env python3
"""Differential PTY/HTTP integrity tests. Requires requirements-integrity.txt."""
from __future__ import annotations

import argparse
import base64
import difflib
import itertools
import json
import os
from pathlib import Path
import random
import selectors
import shlex
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unicodedata
import urllib.parse

from wcwidth import wcwidth

from run import Client, read_all_lines

# Text semantics shared by pyte and TUIC. Extensions need dedicated oracles.
OPS = {
    "text": b"replacement",
    "cr": b"\rR",
    "lf": b"\nN",
    "backspace": b"\bB",
    "tab": b"\tT",
    "up": b"\x1b[3AU",
    "down": b"\x1b[2BD",
    "right": b"\x1b[4CR",
    "left": b"\x1b[5DL",
    "position": b"\x1b[4;7HP",
    "column": b"\x1b[9GC",
    "erase-tail": b"\x1b[0KK",
    "erase-head": b"\x1b[1KK",
    "erase-line": b"\x1b[2KK",
    "erase-display-tail": b"\x1b[0JJ",
    "erase-display-head": b"\x1b[1JJ",
    "erase-display": b"\x1b[2JJ",
    "insert-chars": b"\x1b[3@I",
    "delete-chars": b"\x1b[3PD",
    "erase-chars": b"\x1b[3XE",
    "insert-lines": b"\x1b[2LI",
    "delete-lines": b"\x1b[2MD",
    "save-restore": b"\x1b7\x1b[1;1Htemporary\x1b8S",
    "sgr": b"\x1b[1;31mRED\x1b[0m",
    "reverse-index": b"\x1b[1;1H\x1bMR",
    "index": b"\x1bDI",
    "next-line": b"\x1b[2EN",
    "previous-line": b"\x1b[2FP",
    "absolute-line": b"\x1b[3dV",
    "clamped-up": b"\x1b[999AU",
    "scroll-region": b"\x1b[2;5r\x1b[5;1H\nS\x1b[r",
    "origin-mode": b"\x1b[2;5r\x1b[?6h\x1b[1;1HO\x1b[?6l\x1b[r",
    "insert-mode": b"\x1b[4hINSERT\x1b[4l",
    "wrap-mode": b"\x1b[?7l" + b"W" * 100 + b"\x1b[?7h",

}

TCX_MAGIC = b"TCX1"
CELL_BYTES = 11
MAX_CELL_EXTRAS = 9

UNICODE_ROWS = (
    "UNICODE-DECOMPOSED:cafe\u0301",
    "UNICODE-MULTI:A\u0301\u0308\u20dd",
    "UNICODE-OVERWRITE:Z",
    "UNICODE-WIDE:中文 日本語 € ─│",
)



def integrity_dir() -> Path:
    """Returns the scratch dir for the oracle install, venv and failure artifacts.

    Never inside the checkout: TUIC_TERMINAL_INTEGRITY_DIR, else
    <temp dir>/tuic-tests/terminal-integrity.
    """
    configured = os.environ.get("TUIC_TERMINAL_INTEGRITY_DIR")
    if configured:
        return Path(configured)
    return Path(tempfile.gettempdir()) / "tuic-tests" / "terminal-integrity"


def unicode_operations() -> bytes:
    """Product-owned Unicode contract, independent of another emulator."""
    rows = (
        "UNICODE-DECOMPOSED:cafe\u0301",
        "UNICODE-MULTI:A\u0301\u0308\u20dd",
        # BS returns to the base cell; replacing it must discard the old accent.
        "UNICODE-OVERWRITE:a\u0301\bZ",
        "UNICODE-WIDE:中文 日本語 € ─│",
    )
    # Additional rows force the contract rows into history before the runner
    # scrolls and resizes the grid.
    # The case overwrites seeded rows. Clear each whole line first so the exact
    # Unicode oracle measures this stimulus, not a suffix left by a longer seed.
    text = "\x1b[6;1H" + "".join(f"\x1b[2K{row}\r\n" for row in rows)
    text += "".join(f"\x1b[2KUNICODE-SCROLL-{index:02d}\r\n" for index in range(20))
    return text.encode()


def cases(seed: int, random_cases: int):
    for a, b in itertools.product(OPS, repeat=2):
        yield f"pair-{a}-{b}", OPS[a] + OPS[b]
    tables = (
        "┌─────────────────────┬────────┐\r\n"
        "│ Stack               │ Month  │\r\n"
        "├─────────────────────┼────────┤\r\n"
        "│ Claude              │ 100    │\r\n"
        "│ Codex               │ 200    │\r\n"
        "│ Total               │ 300    │\r\n"
        "└─────────────────────┴────────┘\r\n"
        "Two distinct tables; this separator must survive.\r\n"
        "┌─────────────────────────┬────────┬────────┐\r\n"
        "│ Scenario                │ Month  │ Year   │\r\n"
        "├─────────────────────────┼────────┼────────┤\r\n"
        "│ Enterprise capped       │ 300    │ 3600   │\r\n"
        "│ Enterprise uncapped     │ 500    │ 6000   │\r\n"
        "└─────────────────────────┴────────┴────────┘\r\n"
    ).encode()
    yield "two-tables", b"\x1b[999;1H" + tables
    yield "two-tables-redraw", tables + b"\x1b[999A\r" + tables
    yield "unicode", unicode_operations()
    yield "autowrap", b"\r\n" + b"0123456789abcdef" * 45
    yield "alternate-screen", (b"\x1b[?1049h" + b"alternate only\r\n" * 30
                               + b"\x1b[?1049l")
    yield "synchronized-tables", b"\x1b[?2026h" + tables + b"\x1b[?2026l"
    rng = random.Random(seed)
    names = list(OPS)
    for index in range(random_cases):
        selected = rng.choices(names, k=rng.randint(3, 32))
        yield f"seed-{seed}-{index}", b"".join(OPS[name] for name in selected)
    # Deliberately cross the documented 10,000-row grid cap. Exact equality
    # includes legitimate eviction; retaining the entire stream would be wrong.
    yield "history-cap", b"\x1b[999;1H\r\n" + b"".join(
        f"EVICT-{index:05d}:preserve-order\r\n".encode() for index in range(10030)
    )


def stream_for(index: int, operations: bytes, rows: int) -> bytes:
    # RIS clears the preceding case. Seed > viewport establishes real history.
    prefix = b"\x1bc" + b"".join(
        f"seed-{line:03d}|abcdefghijklmnopqrstuvwxyz|0123456789\r\n".encode()
        for line in range(rows + 9)
    )
    # The marker deliberately goes on a fresh bottom row. Feed it to both
    # emulators; it is part of the oracle, never stripped from the comparison.
    return (prefix + b"\x1b[6;13H" + operations
            + f"\x1b[{rows};1H\r\nEND-{index:06d}".encode())


def reference(payload: bytes, rows: int, cols: int) -> list[str]:
    import pyte
    class RetainingScreen(pyte.HistoryScreen):
        """TUIC's documented ED2 policy retains the cleared screen in history.

        Only this retention policy differs; pyte still interprets all cursor,
        character, line and erase operations independently.
        """
        def erase_in_display(self, how=0, *args, **kwargs):
            if how == 2:
                from copy import deepcopy
                occupied = [y for y in range(self.lines)
                            if any(cell.data != " " or cell.bg != "default"
                                   for cell in self.buffer[y].values())]
                for y in range(max(occupied, default=-1) + 1):
                    self.history.top.append(deepcopy(self.buffer[y]))
            super().erase_in_display(how, *args, **kwargs)

        def index(self):
            # A scroll region below row zero cannot add primary history. pyte
            # records its top row anyway; retain its independent screen logic.
            if self.margins and self.margins.top != 0:
                pyte.Screen.index(self)
            else:
                super().index()

    # The alternate-screen scenario has an explicit restore contract: its
    # entire private screen is discarded on exit, primary rows are unchanged.
    # pyte does not implement 1049, so exclude that private screen from its input.
    import re
    payload = re.sub(rb"\x1b\[\?1049h.*?\x1b\[\?1049l", b"", payload, flags=re.S)
    screen = RetainingScreen(cols, rows, history=10000)
    pyte.ByteStream(screen).feed(payload)
    history = ["".join(row[col].data for col in range(cols)).rstrip()
               for row in screen.history.top]
    return history + [line.rstrip() for line in screen.display]


def normalized(lines: list[str]) -> list[str]:
    # Canonically equivalent combining sequences are not terminal data loss.
    return [unicodedata.normalize("NFC", line.replace("\t", " ").rstrip()) for line in lines]


def assert_rows(actual: list[str], expected: list[str]) -> None:
    if normalized(actual) != normalized(expected):
        diff = "\n".join(difflib.unified_diff(expected, actual,
                            fromfile="reference", tofile="tuic", lineterm=""))
        raise AssertionError(diff[:8000])


def assert_exact_rows(actual: list[str], expected: list[str]) -> None:
    """Compare stored codepoints exactly; canonical equivalence cannot hide loss."""
    if actual != expected:
        diff = "\n".join(difflib.unified_diff(expected, actual,
                            fromfile="expected-codepoints", tofile="actual-codepoints",
                            lineterm=""))
        raise AssertionError(diff[:8000])


def selected_cases(args):
    for index, item in enumerate(cases(args.seed, args.random_cases)):
        if args.case is None or args.case in item[0]:
            yield index, item


def _unicode_scalar(value: int) -> str:
    if value > 0x10ffff or 0xd800 <= value <= 0xdfff:
        raise AssertionError(f"invalid Unicode scalar U+{value:04X} in TCX1 trailer")
    return chr(value)


def decode_styled_rows(payload: bytes, total: int, cols: int) -> list[str]:
    """Decode fixed cells plus the optional sparse TCX1 extension trailer."""
    if len(payload) < 12:
        raise AssertionError("styled payload is shorter than its 12-byte header")
    _, _, actual_cols, count = struct.unpack_from("<IIHH", payload)
    if actual_cols != cols or count != total:
        raise AssertionError(f"styled geometry {actual_cols}x{count}; expected {cols}x{total}")
    offset, rows, previous = 12, [], None
    cell_counts = []
    for _ in range(count):
        if offset + 6 > len(payload):
            raise AssertionError("styled payload ends inside a row header")
        absolute, flags = struct.unpack_from("<IH", payload, offset)
        offset += 6
        if previous is not None and absolute != previous + 1:
            raise AssertionError("styled row indices missing, reordered or duplicated")
        previous = absolute
        cells = flags & 0x3fff
        if flags & 0x4000 or cells != cols:
            raise AssertionError("styled range must contain full rows")
        if offset + cells * CELL_BYTES > len(payload):
            raise AssertionError("styled payload ends inside fixed-width cell data")
        decoded_cells = []
        for _ in range(cells):
            codepoint = struct.unpack_from("<I", payload, offset)[0]
            decoded_cells.append(_unicode_scalar(codepoint) if codepoint else "")
            offset += CELL_BYTES
        rows.append(decoded_cells)
        cell_counts.append(cells)

    if offset < len(payload):
        if len(payload) - offset < 8:
            raise AssertionError("truncated TCX1 trailer header")
        if payload[offset:offset + 4] != TCX_MAGIC:
            raise AssertionError("unexpected bytes after styled rows; missing TCX1 magic")
        entries = struct.unpack_from("<I", payload, offset + 4)[0]
        offset += 8
        seen = set()
        for _ in range(entries):
            if offset + 5 > len(payload):
                raise AssertionError("TCX1 trailer ends inside an entry header")
            row, column, mark_count = struct.unpack_from("<HHB", payload, offset)
            offset += 5
            if row >= count:
                raise AssertionError(f"TCX1 row ordinal {row} is outside {count} wire rows")
            if column >= cell_counts[row]:
                raise AssertionError(
                    f"TCX1 column {column} is outside row {row}'s {cell_counts[row]} cells"
                )
            if not 1 <= mark_count <= MAX_CELL_EXTRAS:
                raise AssertionError(f"TCX1 mark count {mark_count} is outside 1..9")
            key = (row, column)
            if key in seen:
                raise AssertionError(f"duplicate TCX1 entry for row {row}, column {column}")
            seen.add(key)
            byte_count = mark_count * 4
            if offset + byte_count > len(payload):
                raise AssertionError("TCX1 trailer ends inside Unicode scalar data")
            for mark in struct.unpack_from(f"<{mark_count}I", payload, offset):
                rows[row][column] += _unicode_scalar(mark)
            offset += byte_count
        if offset != len(payload):
            raise AssertionError("unexpected trailing bytes after TCX1 entries")

    return ["".join(row).rstrip() for row in rows]


def read_styled(client, sid, total, cols):
    payload = client.request_bytes(f"/sessions/{sid}/terminal/styled-rows?start=0&count={total}")
    return decode_styled_rows(payload, total, cols)


def _unicode_row_indices(lines: list[str]) -> dict[str, int]:
    result = {}
    for expected in UNICODE_ROWS:
        matches = [index for index, line in enumerate(lines) if line == expected]
        if len(matches) != 1:
            raise AssertionError(
                f"Unicode row {expected!r} occurred {len(matches)} times; expected exactly once"
            )
        result[expected] = matches[0]
    return result


def verify_unicode_http(client, sid, text_rows, styled_rows) -> None:
    """Verify exact product behavior through every HTTP text consumer."""
    assert_exact_rows(styled_rows, text_rows)
    indices = _unicode_row_indices(text_rows)
    if any("UNICODE-OVERWRITE:a" in line for line in text_rows):
        raise AssertionError("overwritten cell retained its old base or combining marks")

    for expected, row in indices.items():
        # row-text is viewport-relative, while `row` here is the retained
        # absolute line index returned by /lines and search-buffer.
        client.request("POST", f"/sessions/{sid}/terminal/scroll-to", {"line": row})
        query = urllib.parse.urlencode({"row": 0})
        row_text = client.request(
            "GET", f"/sessions/{sid}/terminal/row-text?{query}"
        )["text"]
        if row_text != expected:
            raise AssertionError(f"row-text lost codepoints: {row_text!r} != {expected!r}")

        terminal_columns = sum(max(0, wcwidth(char)) for char in expected)
        selection_query = urllib.parse.urlencode({
            "startRow": row, "startCol": 0,
            "endRow": row, "endCol": terminal_columns - 1,
        })
        selected = client.request(
            "GET", f"/sessions/{sid}/terminal/selection-text?{selection_query}"
        )["text"]
        if selected != expected:
            raise AssertionError(f"selection text lost codepoints: {selected!r} != {expected!r}")

        matches = client.request(
            "POST", f"/sessions/{sid}/terminal/search-buffer", {"query": expected}
        )["matches"]
        exact = [match for match in matches
                 if match["line_index"] == row and match["line_text"] == expected]
        if len(exact) != 1:
            raise AssertionError(f"exact Unicode search returned {exact!r} for {expected!r}")
        utf16_units = len(expected.encode("utf-16-le")) // 2
        if (exact[0]["match_start"], exact[0]["match_end"]) != (0, utf16_units):
            raise AssertionError(
                "Unicode search coordinates are not UTF-16 string offsets: "
                f"{exact[0]!r}, expected [0, {utf16_units})"
            )


class MatrixMismatch(AssertionError):
    pass


def producer(args) -> None:
    import termios
    import tty
    original = termios.tcgetattr(0)
    try:
        tty.setraw(0)
        os.write(1, b"\r\nINTEGRITY-READY")
        for index, (name, operations) in selected_cases(args):
            if os.read(0, 1) != b"n":
                return
            payload = stream_for(index, operations, args.rows)
            rng = random.Random(args.seed + index)
            if name == "unicode":
                # Deliberately separate base characters from every following
                # mark. The native regression owns the exact parser-chunk proof;
                # these spaced writes exercise the real PTY/read path as well.
                mark_bytes = tuple(mark.encode() for mark in ("\u0301", "\u0308", "\u20dd"))
                chunks, start = [], 0
                while True:
                    candidates = [(payload.find(mark, start), mark) for mark in mark_bytes]
                    candidates = [
                        (position, mark) for position, mark in candidates if position >= 0
                    ]
                    if not candidates:
                        chunks.append((payload[start:], False))
                        break
                    position, mark = min(candidates, key=lambda item: item[0])
                    chunks.append((payload[start:position], False))
                    chunks.append((mark, True))
                    start = position + len(mark)
            else:
                chunks = [(payload, False)]

            for chunk, delayed_mark in chunks:
                offset = 0
                if delayed_mark:
                    time.sleep(0.03)
                while offset < len(chunk):
                    size = len(chunk) if delayed_mark else rng.choice((1, 2, 3, 7, 31, 127))
                    written = os.write(1, chunk[offset:offset + size])
                    offset += written
                    # Keep scroll requests overlapping actual writes, not only the
                    # stable snapshot. No assumption about PTY read chunk boundaries.
                    time.sleep(0.0005)
        os.read(0, 1)  # Keep the shell prompt out of the last snapshot.
    finally:
        termios.tcsetattr(0, termios.TCSADRAIN, original)


def wait_marker(client, sid, marker, timeout):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = client.request("POST", f"/sessions/{sid}/terminal/search-buffer",
                                {"query": marker})
        if any(item["line_text"].endswith(marker) for item in result["matches"]):
            return
        time.sleep(0.01)
    raise TimeoutError(f"producer never reached {marker!r} within {timeout}s")


class XtermOracle:
    def __init__(self):
        self.process = subprocess.Popen(
            ["node", str(Path(__file__).with_name("check_xterm.cjs")), "--server"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)

    def render(self, payload, rows, cols):
        self.process.stdin.write(json.dumps({"payload": base64.b64encode(payload).decode(),
                                            "rows": rows, "cols": cols}) + "\n")
        self.process.stdin.flush()
        with selectors.DefaultSelector() as selector:
            selector.register(self.process.stdout, selectors.EVENT_READ)
            if not selector.select(timeout=120):
                self.process.kill()
                raise TimeoutError("independent oracle did not return within 120 seconds")
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError("xterm oracle exited; check the pinned npm prerequisite")
        return json.loads(line)

    def close(self):
        self.process.stdin.close()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()
        self.process.stdout.close()


def run(args) -> None:
    from urllib.parse import urlparse
    if urlparse(args.base_url).port == 9876:
        raise SystemExit("Synthetic tests must not target the orchestrator on 9876")
    # The Unicode case has a product-owned exact-codepoint oracle below. It can
    # run focused without installing or starting xterm.js.
    only_unicode = [name for name, _ in selected_cases(args)] == ["unicode"]
    result_oracle = "native-exact" if only_unicode else args.oracle
    oracle = XtermOracle() if args.oracle == "xterm" and not only_unicode else None
    render = oracle.render if oracle else reference
    # Check the reference and pinned dependencies before creating a PTY.
    if not only_unicode:
        try:
            render(b"probe", args.rows, args.cols)
        except Exception:
            if oracle:
                oracle.close()
            raise
    try:
        client = Client(args.base_url, args.auth)
        if args.auth:
            client.use_session_token()
        root = Path(__file__).resolve().parents[2]
        sid = client.request("POST", "/sessions", {
            "rows": args.rows, "cols": args.cols, "shell": "/bin/sh", "cwd": str(root)
        })["session_id"]
    except Exception:
        if oracle:
            oracle.close()
        raise
    stop = threading.Event()
    scroll_request_lock = threading.Lock()
    errors = []
    scrolls = [0]

    def scroll_worker():
        try:
            while not stop.is_set():
                for offset in (0, 4, 10000, 1):
                    if stop.is_set():
                        break
                    if offset == 4:
                        route, body = "scroll", {"delta": 3}
                    elif offset == 1:
                        route, body = "scroll-to", {"line": 0}
                    else:
                        route, body = "scroll-to-offset", {"offset": offset}
                    with scroll_request_lock:
                        client.request("POST", f"/sessions/{sid}/terminal/{route}", body)
                    scrolls[0] += 1
                    stop.wait(0.003)
        except Exception as error:
            errors.append(error)

    worker = threading.Thread(target=scroll_worker, daemon=True)
    index, name, payload, expected, actual = -1, "setup", b"", [], []
    started = time.monotonic()
    failures = []
    checked = 0
    try:
        # Creation uses a wide initial grid. Negotiate the tested geometry as
        # the frontend does; even a setup failure must delete the owned PTY.
        client.request("POST", f"/sessions/{sid}/resize", {"rows": args.rows, "cols": args.cols})
        # A ready marker from the producer, not a guessed shell-start delay,
        # establishes that raw input and all setup have completed.
        command = " ".join(map(shlex.quote, [sys.executable, str(Path(__file__).resolve()),
                     "--producer", "--seed", str(args.seed), "--random-cases",
                     str(args.random_cases), "--rows", str(args.rows), "--cols", str(args.cols)]
                     + (["--case", args.case] if args.case else [])))
        client.request("POST", f"/sessions/{sid}/write", {"data": command})
        time.sleep(0.01)
        client.request("POST", f"/sessions/{sid}/write", {"data": "\r"})
        wait_marker(client, sid, "INTEGRITY-READY", args.timeout)
        worker.start()
        for index, (name, operations) in selected_cases(args):
            payload = stream_for(index, operations, args.rows)
            expected = list(UNICODE_ROWS) if name == "unicode" else render(
                payload, args.rows, args.cols
            )
            before = scrolls[0]
            client.request("POST", f"/sessions/{sid}/write", {"data": "n"})
            wait_marker(client, sid, f"END-{index:06d}", args.timeout)
            actual = read_all_lines(client, sid)
            styled = read_styled(client, sid, len(actual), args.cols)
            try:
                if name == "unicode":
                    # Hold off concurrent display scrolling while row-text is
                    # positioned and read through its viewport-relative API.
                    with scroll_request_lock:
                        verify_unicode_http(client, sid, actual, styled)
                        # Preserve exact rows through a real resize cycle after
                        # they have already scrolled into history.
                        resized_cols = max(60, args.cols - 4)
                        resized_rows = args.rows + (1 if resized_cols == args.cols else 0)
                        client.request("POST", f"/sessions/{sid}/resize",
                                       {"rows": resized_rows, "cols": resized_cols})
                        client.request("POST", f"/sessions/{sid}/resize",
                                       {"rows": args.rows, "cols": args.cols})
                        resized = read_all_lines(client, sid)
                        resized_styled = read_styled(client, sid, len(resized), args.cols)
                        verify_unicode_http(client, sid, resized, resized_styled)
                else:
                    assert_rows(actual, expected)
                    assert_rows(styled, expected)
            except AssertionError as error:
                failures.append(name)
                directory = save_evidence(
                    args, index, name, payload, expected, actual, scrolls[0], result_oracle
                )
                (directory / "styled.json").write_text(json.dumps(styled, ensure_ascii=False, indent=2))
                print(f"FAIL {name}: {str(error)[:180]}", flush=True)
            if errors:
                raise errors[0]
            if scrolls[0] <= before:
                raise AssertionError("no concurrent scroll request completed during case")
            checked += 1
            if checked % 100 == 0:
                print(f"CHECKED {checked} cases; {scrolls[0]} scroll requests", flush=True)
        if checked == 0:
            raise AssertionError("case filter matched no cases")
        summary = {"checked": checked, "failed": len(failures), "failures": failures,
                   "seed": args.seed, "oracle": result_oracle, "rows": args.rows, "cols": args.cols,
                   "scroll_requests": scrolls[0], "seconds": time.monotonic() - started}
        Path(args.artifacts).mkdir(parents=True, exist_ok=True)
        (Path(args.artifacts) / "summary.json").write_text(json.dumps(summary, indent=2))
        if failures:
            raise MatrixMismatch(f"{len(failures)}/{checked} cases differ: {failures[:10]}")
        kind = "native exact cases" if only_unicode else "differential cases"
        print(f"PASS {checked} {kind} ({len(OPS)} ANSI operations, "
              f"filter={args.case or 'all'}, seed={args.seed}); "
              f"{scrolls[0]} scroll requests; {time.monotonic() - started:.1f}s", flush=True)
    except MatrixMismatch:
        raise
    except Exception:
        evidence = save_evidence(
            args, index, name, payload, expected, actual, scrolls[0], result_oracle
        )
        print(f"Failure evidence: {evidence}", file=sys.stderr)
        raise
    finally:
        stop.set()
        if worker.ident is not None:
            worker.join(timeout=15)
        try:
            client.request("DELETE", f"/sessions/{sid}")
        finally:
            if oracle:
                oracle.close()


def save_evidence(args, index, name, payload, expected, actual, scrolls, oracle):
    evidence = Path(args.artifacts) / f"{args.seed}-{index}-{name}"
    evidence.mkdir(parents=True, exist_ok=True)
    (evidence / "raw.bin").write_bytes(payload)
    (evidence / "expected.json").write_text(json.dumps(expected, ensure_ascii=False, indent=2))
    (evidence / "actual.json").write_text(json.dumps(actual, ensure_ascii=False, indent=2))
    (evidence / "meta.json").write_text(json.dumps({
        "seed": args.seed, "case": index, "name": name,
        "rows": args.rows, "cols": args.cols, "oracle": oracle, "scroll_requests": scrolls}, indent=2))
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--producer", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--base-url", default="http://127.0.0.1:9877")
    parser.add_argument("--auth")
    parser.add_argument("--oracle", choices=("xterm", "pyte"), default="xterm")
    parser.add_argument("--rows", type=int, default=12)
    parser.add_argument("--cols", type=int, default=72)
    parser.add_argument("--case", help="Run only case names containing this text (empty matches fail)")
    parser.add_argument("--seed", type=int, default=819)
    parser.add_argument("--random-cases", type=int, default=2000)
    parser.add_argument("--timeout", type=float, default=120)
    parser.add_argument("--artifacts", default=str(integrity_dir() / "failures"))
    args = parser.parse_args()
    if args.rows < 8 or args.cols < 60 or args.random_cases < 0:
        parser.error("rows >= 8, cols >= 60 and random-cases >= 0 required")
    (producer if args.producer else run)(args)


if __name__ == "__main__":
    main()
