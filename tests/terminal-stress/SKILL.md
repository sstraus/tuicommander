---
name: terminal-stress
description: Diagnose and reproduce TUICommander terminal truncation, duplicated history, missing output, Claude/Ink full-frame repaint pollution, slash-parser pressure, and false busy/idle reports. Use when a live tab looks corrupted, rows appear partial or duplicated, an agent is marked idle while working, or terminal integrity changes need repeatable PTY stress validation.
---

# Diagnose terminal integrity

Treat raw PTY bytes, canonical grid rows, clean log output, agent transcript, and
lifecycle state as separate evidence. Never call a visual anomaly database
corruption until those layers have been compared.

## Isolate the host clipboard before UI testing

A named app instance and browser session do not isolate the operating system
clipboard. Terminal selection can copy immediately, without a separate Copy
command. Before selection/copy/paste E2E, intercept or deny clipboard access in
the test browser and verify the interception; assert copied text inside that
isolated capture. Do not select text in a test terminal before this is in place.
Do not save/restore the host clipboard as a substitute: the user may change it
concurrently. Browser screenshots and PTY/HTTP reads do not require clipboard
access.

## Preserve volatile evidence first

1. Identify the exact session and confirm its cwd/display name.
2. Capture the raw ring before making additional requests; it rotates under
   sustained output.
3. Capture dimensions and canonical rows immediately afterwards.
4. Record lifecycle fields (`shell_state`, `agent_state`, background work, and
   current task) at the same time.
5. Read logs and transcripts only after the volatile terminal evidence is safe.

Use the repository capture helper:

```bash
python3 tests/terminal-stress/capture.py \
  --session SESSION_ID \
  --base-url http://127.0.0.1:9876 \
  -o "${TMPDIR:-/tmp}/tuic-terminal-capture"
```

Every scratch path in this skill lives under `$TMPDIR` (or your session
scratchpad), never a literal `/tmp`.

Port `9876` is the live orchestrator. Use it only for read-only capture of the
session Boss identified. Never run synthetic workloads or create throwaway
sessions there.

The capture must contain:

- `raw.bin`: pre-transform PTY bytes from the flight recorder.
- `canonical.txt`: rows returned by the backend grid.
- `meta.json`: dimensions, row range, scroll state, and capture time.

Do not commit an unsanitized live capture. It may contain prompts, paths,
credentials, or repository data.

## Compare the layers

Use a distinctive visible string such as a banner, table header, or allegedly
duplicated sentence. Count it independently in raw bytes and canonical rows.

| Evidence | Interpretation |
|---|---|
| grid count exceeds raw count | Investigate duplication, accounting for redraws and the raw-ring retention window |
| grid count equals raw count | Counts agree only; compare complete rows and ordering before claiming integrity |
| raw contains data absent from grid | Replay erasure/overwrite and history eviction before classifying unintended loss |
| transcript is complete but terminal is partial | Loss is downstream of the transcript; raw replay must distinguish agent TUI output from terminal handling |
| transcript and raw are both incomplete | Agent/process stopped before emitting data |

Do not deduplicate terminal history merely because adjacent rows share a prefix.
A carriage return cannot rewrite a row after output has pushed it into
scrollback. Removing that row would discard legitimate output.

For Claude/Ink, specifically search for this sequence:

1. Enough `CRLF` output to push the current frame into normal scrollback.
2. `ESC[H` to move home.
3. Per-row `ESC[2K` erases.
4. A full frame reprint beginning with the welcome banner.

If raw and grid contain the same banner copies and exact replay reproduces
those copies, attribute that duplication to Ink full-frame repaint output.
Matching counts alone do not rule out other terminal defects. The
`ink-banner-dup-raw-ring-2026-07-06` MDKB entry contains two real confirmations.

## Replay a live capture offline

Replay the exact bytes through the same terminal parser at fixed captured
dimensions:

```bash
TUIC_REPLAY_FILE="${TMPDIR:-/tmp}/tuic-terminal-capture/raw.bin" \
TUIC_REPLAY_ROWS=33 \
TUIC_REPLAY_COLS=146 \
TUIC_REPLAY_CHUNK=4096 \
TUIC_REPLAY_OUT="${TMPDIR:-/tmp}/tuic-terminal-replay.txt" \
cargo test --manifest-path src-tauri/Cargo.toml --lib \
  replay_capture_from_env -- --ignored --nocapture
```

Replace rows and columns with `meta.json` values. Compare replay rows with the
captured canonical rows exactly, not by screenshot. A fixed-size replay that
reproduces the anomaly rules out live resize scheduling and frontend frame
delivery as necessary causes.

## Check claimed data loss

Inspect the agent's authoritative transcript after raw/grid comparison. For
Claude, locate the active JSONL under:

```text
~/.claude/projects/<encoded-cwd>/<session-id>.jsonl
```

Distinguish intermediate tool-use messages from the final assistant message. A
partial table followed by tool calls and a later complete final table is not
model-data loss, even when Ink's repaint makes the terminal history misleading.

## Check busy/idle independently

Do not infer lifecycle correctness from the visible output alone. Capture and
compare:

- backend `shell_state`;
- parsed `agent_state`;
- background-work evidence;
- current task/status text;
- screen movement across consecutive snapshots;
- hook or transcript evidence when available.

Use the sanitized fixtures in `fixtures/` for lifecycle parser regressions.
Keep false-idle diagnosis separate from history integrity: both can occur under
load, but one does not prove the other.

When Codex shows both `Working (... esc to interrupt)` and a persistent `Goal
achieved` footer, treat movement of the Working row as current activity. The
goal footer describes goal bookkeeping, not the current PTY execution cycle.
Use `fixtures/codex-completed-internal-working.txt` for this regression.

## Evaluate logging pressure separately

Thousands of `slash_menu parse` DEBUG lines can add CPU, allocation, and log
I/O pressure. They are not proof of terminal corruption. If a fixed offline raw
replay reproduces the visual anomaly without application logging, the log storm
is not a necessary cause of that anomaly. Retain it as a separate performance
investigation.

Enable runtime diagnostics during a load reproduction when performance is part
of the report:

```bash
curl -X POST http://127.0.0.1:9876/diagnostics \
  -H 'Content-Type: application/json' \
  -d '{"enabled":true}'
curl 'http://127.0.0.1:9876/logs?source=diagnostics'
```

Correlate timestamps with CPU, child CPU, threads, file descriptors, session
count, stuck grid frames, and event-bus subscribers.

## Run the synthetic suite safely

Test worktree changes only through the test instance HTTP API. The TUICommander
MCP and port `9876` target the orchestrator build and do not validate worktree
changes.

With a test desktop instance already running on `9877`:

```bash
python3 tests/terminal-stress/run.py \
  --base-url http://127.0.0.1:9877 \
  --scenario all \
  --count 2000
```

Run the exact Claude/Ink provenance case with:

```bash
python3 tests/terminal-stress/run.py \
  --base-url http://127.0.0.1:9877 \
  --scenario ink-repaint \
  --count 2000
```

For full isolation, build the headless binary and give it a temporary home:

```bash
cargo build --manifest-path src-tauri/Cargo.toml \
  --no-default-features --bin tuic-remote
export TUIC_STRESS_HOME="$(mktemp -d "${TMPDIR:-/tmp}/tuic-stress-home.XXXXXX")"
printf 'stress\nstresspass\n' | \
  HOME="$TUIC_STRESS_HOME" \
  src-tauri/target/debug/tuic-remote --set-password
HOME="$TUIC_STRESS_HOME" TUIC_PORT=9879 \
  src-tauri/target/debug/tuic-remote
```

In another shell:

```bash
python3 tests/terminal-stress/run.py \
  --base-url http://127.0.0.1:9879 \
  --auth stress:stresspass \
  --scenario all \
  --count 2000
```

Stop the headless instance and remove only the explicit temporary directory
after the run. The runner creates and deletes a throwaway PTY session in a
`finally` block.

## Interpret the synthetic scenarios

- `atomic`: split-byte synchronized updates must yield one complete copy.
- `progressive`: visible partial frames must be replaced without preservation.
- `timeout`: synchronized-update timeout must not retain a partial row.
- `reflow`: deliberately committed partial and complete rows must both remain;
  the grid must not manufacture another complete copy.
- `scrollout`: report orphaned live partials pushed beyond carriage-return
  reach; never silently delete them.
- `slash-pressure`: exercise slash parsing under fragmented sustained output.
- `ink-repaint`: emit four full tall-frame repaints; require every canonical
  copy to have raw provenance and require no expected record to disappear.

Treat every failure as evidence. Do not weaken an invariant to make the suite
green. If the original invariant was technically wrong, document why terminal
semantics require changing it, then encode the corrected invariant.

## Add a new regression

1. Add deterministic producer behavior to `producer.py`.
2. Add a scenario-specific verifier to `run.py`.
3. Assert the correct layer boundary: raw emission, canonical retention,
   lifecycle classification, or transport delivery.
4. Add the scenario to `all` only when it is safe and bounded at the default
   count.
5. Document the invariant in `README.md` and this skill when it changes the
   diagnostic process.
6. Run Python syntax validation and the scenario at a small count.
7. Run the scenario at the default count of 2,000 on an isolated instance.
8. Run `git diff --check`.

Sanitize committed fixtures. Keep only synthetic payloads and the minimum
terminal metadata required to reproduce the behavior.

## Record conclusions

Search MDKB before writing memory. Update an existing entry when the new
evidence confirms the same failure class. Record:

- symptom and exact session context;
- raw byte length and captured dimensions;
- raw/grid occurrence counts;
- offline replay equality or first difference;
- transcript completeness;
- required and ruled-out causes;
- synthetic scenario that preserves the regression.

Never create competing memories for the same root cause.

## Generated ANSI matrix

Use `ansi_integrity.py` for ordered-pair and seeded-sequence coverage with
concurrent scroll requests. Follow the setup, oracle policies, artifact format,
and coverage boundaries in `README.md` → "Generated ANSI integrity matrix".
It compares complete canonical and binary styled rows, not occurrence counts.
A mismatch stays a failure until classified; consult a second emulator with
`check_xterm.cjs` when cursor semantics differ. Raw occurrence counts alone do
not prove equality: legitimate erase/overwrite can remove emitted text, and
identical counts can hide reordered rows. Never claim GPU or browser coverage
from this HTTP suite. Run the separately controlled legacy producer for
concurrent resize, progressive output, and synchronized-update timeout cases.
