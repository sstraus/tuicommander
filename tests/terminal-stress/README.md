# Terminal Stress Regression Suite

For the complete evidence-capture, replay, classification, and regression
workflow, follow [`SKILL.md`](SKILL.md). It is a repository-local operational
skill intended for both humans and coding agents.

This directory contains repeatable end-to-end regressions for terminal data
integrity and agent lifecycle detection. It complements the in-process Rust
tests in `src-tauri/crates/tuic-terminal/src/terminal_grid.rs` and `src-tauri/src/pty.rs` by driving a
real PTY through TUICommander's HTTP transport.

The Python PTY producers use POSIX raw-mode APIs (`termios`/`tty`); these
runtime commands are for macOS/Linux. Their results do not certify Windows or
the desktop WebView. Native Rust and frontend tests cover separate boundaries.

## Safety

The runner creates a throwaway shell session, verifies only that session, and
deletes it in a `finally` block. It refuses port `9876` by default because that
is normally the orchestrator instance containing live user sessions.

Browser UI tests need separate clipboard isolation: selecting terminal text can
auto-copy into the host clipboard. A named instance/session does not prevent
this. Intercept or deny clipboard access and verify that guard before any
selection/copy/paste test; never overwrite or restore the user's clipboard.

Start a worktree build with `make dev`; it normally binds to `9877` when the
orchestrator already owns `9876`. Then run:

```bash
python3 tests/terminal-stress/run.py --base-url http://127.0.0.1:9877
```

### Against a headless instance (no window, fully isolated)

Use a named instance for isolated configuration and disposable sessions. The
headless binary requires the CLI `--instance` option (the desktop environment
initialization is not its isolation mechanism). Build through mbx on macOS:

```bash
PATH="$HOME/Library/Application Support/mbx/bin:$PATH" mbx doctor
PATH="$HOME/Library/Application Support/mbx/bin:$PATH" \
  cargo build --manifest-path src-tauri/Cargo.toml --no-default-features --bin tuic-remote
src-tauri/target/debug/tuic-remote --instance integrity-ansi --set-password
TUIC_PORT=9877 src-tauri/target/debug/tuic-remote --instance integrity-ansi
# In another shell:
python3 tests/terminal-stress/run.py --base-url http://127.0.0.1:9877 \
  --auth USER:PASS --scenario all --count 2000
```

`--auth` is required for headless instances, including loopback. Use dedicated
synthetic credentials, and stop only the test process after verification. For browser rendering, build the frontend and verify the served asset hashes.
The no-default-features headless binary does not mount static frontend routes;
use a test desktop instance or a disposable same-origin static/reverse proxy
pointing exclusively to the isolated backend. Record that harness separately
from native desktop or release validation.

To deliberately test a primary instance:

```bash
python3 tests/terminal-stress/run.py \
  --base-url http://127.0.0.1:9876 \
  --allow-primary
```

## Scenarios

- `atomic`: 2,000 DEC 2026 synchronized updates. Each record is first written
  partially, erased, then written completely. Escape sequences and payloads
  are split at deterministic irregular byte boundaries.
- `progressive`: writes the partial and replacement row in separate synchronized
  frames, modeling visible token-by-token growth while scroll and resize requests
  race with the producer.
- `timeout`: the same workload with periodic pauses longer than the synchronized
  update timeout before the erase-and-complete phase.
- `reflow`: commits a PARTIAL row to history with its own newline, then prints
  the complete extension as a separate row, while the runner resizes the
  viewport underneath. Models the shape reported in story #498-7e3d. Its
  verifier is deliberately different: it does NOT require the partial row to
  disappear (the producer really printed both, and discarding one would be the
  grid silently dropping history) — it requires that the grid does not
  MANUFACTURE a copy of the complete row under reflow.
- `scrollout`: the mechanism `reflow` does NOT model, and the one that
  reproduces story #498-7e3d. A partial row is left LIVE (no newline, still the
  cursor's row, so the application could still `\r` over it), then enough output
  arrives to push it into scrollback, and only then is the rewrite attempted. The
  rewrite lands on a new row and history keeps the partial — which is what a real
  terminal does, because carriage return cannot reach a row that already scrolled
  off. Its verifier hard-asserts only that no complete record is duplicated, and
  REPORTS the orphaned partials rather than failing on them.
- `slash-pressure`: enters TUIC's slash-command mode through a no-echo producer
  handshake, then emits the atomic workload. This reproduces the per-chunk
  slash-menu parser pressure that once generated thousands of application-log
  records during sustained agent output.
- `ink-repaint`: reproduces Claude/Ink's tall-frame behavior with four
  synchronized `home → erase each row → full-frame reprint` cycles. The frame is
  taller than the viewport, so previous copies enter normal-screen scrollback.
  The verifier compares raw-ring and canonical-grid counts and requires them to
  match exactly, proving whether copies came from the application or the grid.
- During these scenarios the controller repeatedly scrolls and resizes the
  terminal while output is still arriving.

For every scenario the verifier requires every expected `REC-NNNN` record to
exist exactly once and byte-complete in the canonical backend grid. Missing,
truncated, or duplicate records fail with the relevant record IDs.

### Guarded browser selection regression

[`selection_motion_e2e.py`](selection_motion_e2e.py) checks a stationary canvas
selection while [`selection_motion_producer.py`](selection_motion_producer.py)
continues printing numbered rows. It covers both a released selection and a
held drag seeded with a different prior copied selection. The runner asserts
actual 50-row progress, stable absolute viewport origin
`historyBase + historySize - displayOffset`, unchanged viewport row identities,
nonempty base/overlay canvases, trusted held-button events, and the in-page
clipboard guard. It refuses port 9876 unless `--allow-primary` is explicit.

Open the browser through the stealth wrapper and a disposable same-origin proxy
for the isolated backend. Install [`clipboard_guard.js`](clipboard_guard.js) as
a pre-page init script, verify `navigator.webdriver` is undefined and
`__tuicClipboardGuard.assertInstalled()` succeeds, then run:

```sh
python3 tests/terminal-stress/selection_motion_e2e.py \
  --session-id "$SESSION_ID" --case released --line 5000 \
  --frames "$PROXY_CAPTURE/frames.jsonl" --output "$ARTIFACTS"
python3 tests/terminal-stress/selection_motion_e2e.py \
  --session-id "$SESSION_ID" --case held --line 5000 \
  --frames "$PROXY_CAPTURE/frames.jsonl" --output "$ARTIFACTS"
```

The producer requires a POSIX PTY and supports `--initial-rows 10050` for the
scrollback-cap boundary. The harness never reads, snapshots, restores, or
writes the host clipboard; copied text is retained only in the page guard.

## Capturing a live anomaly

`capture.py` snapshots the three things a grid anomaly can only be diagnosed
from together — the raw ring, the dimensions, and the canonical rows — in one
shot, because two of them are volatile and the ring rotates:

```bash
python3 tests/terminal-stress/capture.py --session <id> -o capture-dir
```

It also flags adjacent rows where the next row EXTENDS the previous one, which
is the reported #498-7e3d shape. A worked example lives in
`fixtures/capture-498-7e3d/` — raw ring, dimensions and canonical rows of a real
captured occurrence, with a README explaining why that shape is correct output
rather than a defect.

`raw.bin` is the complete byte-for-byte response from the session's
`/raw-ring` endpoint; it is never UTF-8 decoded or truncated to the terminal
output snapshot limit. Capture stops with an error instead of creating a
misleading artifact when the endpoint fails or the ring is empty.

The suite does not erase legitimate terminal history. If an application scrolls
a partial row into history and only later prints an extended version, both rows
are valid terminal output and cannot be deduplicated safely without an
application-specific semantic signal. `scrollout` demonstrates this concretely:
it produces the partial-then-extension shape on purpose, and the grid keeping
both rows is the correct outcome, not a defect. If you see the shape live and
suspect something else is going on, `capture.py` preserves the evidence.

The screen snapshots in `fixtures/` are sanitized captures of real false-idle
layouts. Rust lifecycle tests load these fixtures so changes to agent chrome can
be reviewed independently from the assertions.

## Extending the suite

Add producer behavior as a named scenario in `producer.py`, keep all schedules
deterministic, and document the exact invariant here. A regression fixture must
contain no repository secrets, credentials, or full user prompts.

## Generated ANSI integrity matrix

`ansi_integrity.py` drives a real PTY and compares every retained row, in order,
with the independently implemented `xterm.js` headless emulator. `--oracle pyte`
provides a second implementation for diagnosis. It also decodes the binary
`styled-rows` response used by the canvas history cache and requires the same
content and consecutive absolute row indices. This is an end-to-end backend and
HTTP test, **not a screenshot or browser canvas test**.

The dedicated `--case unicode --random-cases 0` check uses literal native
expectations, independent of xterm. It requires exact codepoints through delayed
marks, multiple marks, overwrites, scroll/resize, styled rows, selection and
search; NFC equivalence cannot hide loss. The broader matrix uses emulators as
diagnostic comparisons and does not declare every divergence a TUIC defect.

Install its pinned test-only dependencies:

```sh
npm install --prefix .tmp/terminal-integrity/oracle --ignore-scripts \
  --no-audit --no-fund @xterm/headless@5.5.0
uv venv .tmp/terminal-integrity/venv
uv pip install --python .tmp/terminal-integrity/venv/bin/python \
  -r tests/terminal-stress/requirements-integrity.txt
```

Start a current headless build with a named instance. On macOS, build through
mbx, check `mbx doctor`, and let mbx select its cache directory. For headless
isolation use the CLI option `--instance`; do not rely on the desktop-only
`TUIC_APP_INSTANCE` environment initialization.

```sh
PATH="$HOME/Library/Application Support/mbx/bin:$PATH" \
  cargo build --manifest-path src-tauri/Cargo.toml --no-default-features --bin tuic-remote
src-tauri/target/debug/tuic-remote --instance integrity-ansi --set-password
TUIC_PORT=9877 src-tauri/target/debug/tuic-remote --instance integrity-ansi
```

Use the credentials configured for that disposable instance:

```sh
.tmp/terminal-integrity/venv/bin/python tests/terminal-stress/ansi_integrity.py \
  --auth USER:PASS --random-cases 0
.tmp/terminal-integrity/venv/bin/python tests/terminal-stress/ansi_integrity.py \
  --auth USER:PASS --seed 819 --random-cases 2000
```

The matrix covers all ordered pairs, including repeated operations, from the
explicit `OPS` vocabulary: text, CR/LF, BS/tab, relative/absolute/clamped cursor
movement, line/display/character erasure, character/line insertion and deletion,
save/restore, SGR, index/reverse index, scrolling margins, origin, insertion and
wrapping modes. It then generates seeded sequences of 3–32 operations. Named
cases cover two different-width tables separated by prose, oversized redraws,
wide and combining Unicode, autowrap, synchronized updates, alternate-screen
restoration and the 10,000-row history cap.

A separate thread sends relative scroll, absolute scroll and coalesced viewport
offset requests **while the producer is fragmenting its writes**. Every case
must overlap completed scroll requests. The producer waits for explicit input
between cases so snapshots are stable; there is no guessed completion sleep.
Creation's initial wide grid is explicitly resized before testing.

`--case TEXT` selects matching case names and fails if nothing matches. For
example, `--case unicode --random-cases 0` isolates combining-character loss.
`--rows` and `--cols` vary geometry. Fixed-geometry ANSI differential testing is
separate from the original runner's concurrent resize workload: reflow is not a
portable `pyte` contract, and racing geometry changes with absolute cursor
commands would make the expected state ambiguous.

On failure, the suite continues through the matrix, saves `raw.bin` (the exact
synthetic stream), `expected.json`, `actual.json`, `styled.json`, and `meta.json`,
and exits nonzero. `summary.json` reports the tested dimensions, seed, count,
failed names, completed scroll requests and elapsed time. Choose a different
`--artifacts DIRECTORY` when retaining multiple runs. The files contain
synthetic data, not user transcripts. They can be replayed with the existing
Rust `replay_capture_from_env` harness.

### Oracle boundaries and failure triage

A differential mismatch is evidence to investigate, not automatic proof that
TUIC is wrong. The primary adapter declares one history-policy difference: ED2 retains the
occupied cleared viewport in TUIC. It observes xterm through its public parser
and buffer APIs; xterm still executes the erase. The optional pyte adapter also
corrects region-history retention and gives DEC 1049 an explicit primary-restore
contract because pyte does not implement alternate screens. Pyte additionally
differs from xterm and TUIC on some margin and pending-wrap transitions; its
failures must not be reported as TUIC bugs without independent confirmation. No cursor, character,
or line-editing failures are suppressed. Canonically equivalent Unicode is
normalized; a literal tab occupying a grid cell is compared as a blank cell.
Trailing padding is ignored, but internal blanks, line order and blank rows are
not. Color/style correctness is outside this text-integrity oracle.

For an independent tie-breaker on a saved failure:

```sh
npm install --prefix .tmp/terminal-integrity/oracle --ignore-scripts \
  --no-audit --no-fund @xterm/headless@5.5.0
node tests/terminal-stress/check_xterm.cjs FAILURE_DIRECTORY
```

This writes `xterm.json`. Do not modify an oracle merely to match TUIC: first
classify the mismatch as a protocol defect, a documented terminal policy,
reference-emulator behavior, or a harness error. A single control stream may
legitimately produce different history under different terminal policies. No expected-failure allowlist
turns an unresolved mismatch green.

The matrix is bounded coverage, not every possible ANSI stream: OSC/DCS/APC
extensions, device replies, mouse/keyboard protocols, arbitrary parameter values,
visual attributes, WebSocket delta recovery, GPU painting and native WebView
behavior require their own tests. The Python producer currently requires POSIX
termios; a passing macOS run does not prove Windows ConPTY behavior.

### Checker regression tests

```sh
.tmp/terminal-integrity/venv/bin/python -m unittest discover \
  -s tests/terminal-stress -p test_ansi_integrity.py -v
```

These deliberately remove, duplicate, reorder and corrupt rows to prove the
checker rejects those failures. The legacy atomic verifier now checks record
order too; the reflow verifier requires **both** deliberately committed partial
and complete rows, in order. The original producer uses `--controlled` for the
runner's raw-mode start handshake; standalone output generation remains usable.
Basic Auth is exchanged once for a session token before request-intensive tests.

`scrollout` emits 18 physical rows per record and its existing verifier requires
all records. Keep that scenario at `--count 500` or below with the current grid
cap; the separate `history-cap` case verifies intentional eviction exactly.
