# Story 2043-4419: width investigation

## Status

The reported narrow agent layout is not reproduced. No production code changed.
Boss closed the story as not reproducible on 2026-10-19; reopen on a fresh occurrence with a screenshot and session name. The diagnostic test passes on production revision
`96338d2199888df6f8126bd919a7b76c5e0d9705`; it is not a RED reproduction.

## Observations

Read-only inspection of the running desktop through `debug invoke_js` on
2026-10-18 confirmed the reported terminal identity:

- Terminal: `term-863`, Follow-ups and push, Claude.
- PTY: `546b8989-f4e8-4d68-8843-920d9aa5a619`.
- Retained canvas CSS width: 1774 pixels. The tab was hidden during inspection.
- `terminal_styled_rows` returned 221 grid columns.
- Composer rules at absolute rows 6728 and 6730 had nonblank cell index 220.
- Several retained answer rows had nonblank cells at indices 215 through 218.
- The raw PTY output contained a 221-column composer rule and explicit cursor
  addresses beyond column 200. Thus the agent itself had emitted wide rows.

This does not prove what the screenshot showed earlier, or that the hidden
canvas would paint those cells correctly when shown. No live user tab was
selected, resized, or sent input. No desktop process was restarted.

A separate throwaway session created through the running TUIC MCP session path
at 24 rows and 80 columns ran a foreground Python probe. It printed:

```text
INITIAL os.terminal_size(columns=80, lines=24)
WINCH os.terminal_size(columns=200, lines=40)
```

The second sample came from the probe's SIGWINCH handler after MCP session
resize. The throwaway session was closed after the check.

## Source path audited

- `Terminal.tsx`: `calcGridSize` and `initSession` compute initial dimensions and
  resize existing sessions before attaching them.
- `CanvasTerminal.tsx`: `measureNow` computes rows/columns from cell metrics;
  the visibility observer calls `remeasure` when showing a hidden terminal.
- `transport/http.ts`: `rpc` serializes/coalesces resize requests per session,
  including desktop IPC requests.
- `pty/commands.rs`: `create_pty` and `create_pty_with_worktree` assemble shell
  spawns; `resize_pty` uses the shared off-thread resize core.
- `agent.rs`: direct agent spawn passes the requested dimensions to the same
  PTY allocation helper.
- `mcp_http/session.rs`: direct and shell spawns register the grid using the
  requested geometry.
- `pty/lifecycle.rs`: `spawn_pty_pair_with_retry` supplies geometry to openpty;
  `resize_session_core` serializes grid and master resize under a session lock,
  updates the grid before signaling the child, and records successful geometry.
- `tuic-terminal/src/vt_log.rs`: `VtLogBuffer::resize` passes rows and columns to
  the alacritty grid, with reflow policy selected separately.

No fixed 80-column override was found in the audited resize path. This is a
scoped observation, not proof that every possible launch or observer is correct.

## Targeted test

`pty::tests::production_spawn_resize_reaches_child_winsize` uses the production
`spawn_pty_command` and `spawn_pty_session` entry points and
`resize_session_core`. Its foreground Python child reads its own kernel winsize
with `os.get_terminal_size(0)` in a SIGWINCH handler. The test checks grid
geometry against that independent child result.

Cases cover direct and shell spawns, a resize immediately after spawn without
waiting for child readiness, and normal 80-to-200, 200-to-60, 60-to-200 changes.
The Unix-only test requires `/usr/bin/python3`. Scratch data is under the
repository test temp root. An RAII guard closes every spawned session.

The first shell-based probes were unsuitable: an interrupted shell `read` could
exit on Linux, and an interactive shell could give its foreground `sleep` a
separate process group. Those failures are not evidence of the reported defect.
The final probe directly owns the foreground and passed on both hosts.

Final command inside the build-slot runner:

```sh
scripts/with-test-tmp.sh cargo nextest run \
  --manifest-path src-tauri/Cargo.toml --lib \
  -E 'test(production_spawn_resize_reaches_child_winsize)'
```

- Native macOS, through mbx/build-slot: 1 passed; test 1.828 seconds, build
  3 minutes 16 seconds.
- rb Linux, through build-slot `--remote` with `RB_FORCE=1` after native
  validation: 1 passed; test 1.254 seconds, build 4 minutes 7 seconds.
- Final log: `~/Gits/.tmp/fix-tuic-2043-python-probe.log`.
- rb result: `~/Gits/.tmp/rb/20261018T221250Z-55617`.

No full suite, clippy, mutation, or unrelated frontend test ran.

## Live trigger exploration, 2026-10-19

The coordinator authorized live manipulation of one throwaway shell in the
existing desktop, without restarting it. The shell was
`a73a3680-7893-4f72-a740-2a86d16b6f9e` (`term-1035`, `width-2043-live`).
It ran the installed interactive Claude Code 2.1.286 with Sonnet 5.5 and the
normal profile. Prompts requested prose only, without tools or file changes.

The experiment used existing UI handlers: sidebar/tab clicks, the terminal
context menu's Split Right action, native menu-action dispatch, and the split
divider's mouse handlers. No layout geometry was set directly in a store.
Kernel measurements used `stty -f /dev/ttys012 size`; grid measurements used
`terminal_styled_rows`. Right-edge indices below are zero-based.

| Trigger/stage | Canvas CSS width | Grid columns | Kernel rows × columns | Observed agent rows |
| --- | ---: | ---: | --- | --- |
| Visible narrow launch, completed answer | 638 | 79 | 63 × 79 | Prose and composer reach index 78 |
| Widen after completed answer | 1582 | 197 | 63 × 197 | Prose reflows; composer reaches 196 |
| Switch to document tab | Hidden | 221 | 63 × 221 | Composer reaches 220 |
| Return to Claude tab | 1774 | 221 | 63 × 221 | Prose reaches 220; composer reaches 220 |
| Relaunch Claude while tab is hidden | Hidden | 221 | 64 × 221 | Composer reaches 220 |
| Show hidden-launched Claude | 1774 | 221 | 63 × 221 | New answer reaches 220; composer reaches 220 |
| New answer streaming in narrow split | 638 | 79 | 63 × 79 | New answer reaches 78 |
| Rapid divider movement while streaming, ending wide | 1582 | 197 | 63 × 197 | New answer reaches 195; composer reaches 196 |

These are three trigger variants: completed narrow output followed by widening
and tab return; hidden launch followed by showing; and rapid resize during a
new streaming answer. None reproduced a grid/kernel/agent width mismatch.
The profile's status text has finite content; its stopping near column 89 in
a wide pane is not evidence of an 80-column terminal. The composer rule and
prose both extend beyond that point.

Evidence is under `~/Gits/.tmp/2043-live-20261019/`: screenshots
`01-narrow.png`, `02-wide.png`, `03-away.png`, `04-return.png`,
`05-hidden-launch.png`, `06-hidden-return.png`, `07-stream-narrow.png`,
`08-stream-wide.png`, and `09-stream-complete.png`, with matching JSON/grid
and kernel files. `00-before.png` and `10-restored.png` record the surrounding
UI state. The coordinator inspected these full-screen captures and found
other windows, not the TUIC terminal. These screenshots provide no terminal
paint evidence. The kernel/grid measurements remain valid. This agent had no
image-view tool and did not visually inspect them; no renderer-only visual
pass is claimed. The asynchronous debug-log reader briefly returned an
older result for stages 06 and 09; their JSON files were replaced with the
completed, timestamped result before reporting.

The throwaway session and its Claude process were closed. The backend session
list and frontend terminal list no longer contain it. The original `term-904`
view and side-panel settings were restored; the layout root and active group
are null (no split). The debug layout snapshot still reports an unreferenced
`g1` entry naming the removed test tab. This residual snapshot/state is noted,
not silently presented as an exact restoration of the original empty groups
map. No production code, installed binary, or running desktop was changed.

## Canvas export follow-up, 2026-10-19

At the coordinator's request, variants 1 and 3 were repeated in a new throwaway
session, `111648d9-faa2-4533-b592-c5780d7279a2` (`term-1037`). Captures now
come directly from the visible CanvasTerminal base canvas's
`toDataURL("image/png")`, not the desktop. The capture requires exactly one
visible terminal canvas. A temporary loopback collector decoded the PNG and
saved it with the same invocation's grid/geometry metadata. No UI state or
canvas drawing was changed by this export.

Files are under `~/Gits/.tmp/2043-live-20261019/canvas/`:

| PNG | CSS width | Grid columns | Kernel rows × columns | PNG dimensions |
| --- | ---: | ---: | --- | --- |
| `v1-narrow.png` | 638 | 79 | 63 × 79 | 1276 × 2520 |
| `v1-wide.png` | 1582 | 197 | 63 × 197 | 3164 × 2520 |
| `v1-return.png` | 1582 | 197 | 62 × 197 | 3164 × 2480 |
| `v3-narrow.png` | 638 | 79 | 62 × 79 | 1276 × 2480 |
| `v3-wide.png` | 1582 | 197 | 62 × 197 | 3164 × 2480 |
| `v3-return.png` | 1582 | 197 | 62 × 197 | 3164 × 2480 |

Each PNG has matching JSON and kernel text. `summary.json` records timestamps,
PNG hashes and dimensions. Variant 1 widened a completed narrow answer, then
switched to a document tab and back without removing the split. Variant 3
started a new answer at narrow width, moved the divider rapidly while it was
streaming, then switched away and back. The wide and return PNGs for variant 3
have identical SHA-256 hashes. Wide grid composer rules reach index 196; no
kernel/grid mismatch was reproduced. Paint interpretation is left to the
coordinator, who can view these actual canvas exports.

The temporary collector was stopped. The throwaway PTY and Claude process
were closed and verified absent. The original `term-904` selection was restored
without restarting the desktop. During the experiment an unrelated managed
peer, `term-1038` (`critic-polish-layout`), was auto-assigned to the same pane
group. It was preserved: the final tree has one leaf, no split, containing
that unrelated peer rather than an empty/null layout. `canvas/restored.json`
records this state. No production change or failing regression is claimed.

## Remaining gap

A fresh occurrence must correlate the visible pane and rendered cells with the
same moment's grid, raw agent output, kernel geometry, and resize history.
The current observations do not distinguish a historical startup/repaint race
from retained narrow hard-wrapped output or a renderer-only issue. No such cause
is claimed. The main-store worklog at `2026-10-19T01:20:47.522Z` records the
coordinator's pixel review: Claude prose and composer rules reach the full
197-column width after widening and after tab return in variants 1 and 3.
This is evidence against a persistent Claude paint defect in those runs, not
a reproduction of the reported failure. Ego visual acceptance remains unverified.
No production guard or forced resize was added without a reproduced failure.

## Continuation audit, 2026-10-19

The continuation reused the recorded experiments instead of repeating successful
checks. The test log was read again: the native Mac test passed in 1.828 seconds
and the rb Linux test passed in 1.254 seconds on unchanged production code.
There is still no RED result. None of the three acceptance criteria can be
checked in full: the first requires a failing regression, the second includes
agent layout, and the third requires both Claude and ego.

Source inspection confirms that `pty/lifecycle.rs:3797` calls `vt.resize(rows,
cols)` before `pty/lifecycle.rs:3819` resizes the PTY master. The last-applied
size is recorded only after successful PTY resize at line 3833. These lines
explain the intended ordering; they do not establish the cause of a historical
failure. The child-side oracle is `os.get_terminal_size(0)` in
`pty/lifecycle_tests.rs:3629`.

The existing canvas captures, metadata, kernel samples and original test log
were copied to `~/Gits/.tmp/results/2043-4419/` with SHA-256 verification.
These are retained prior-run artifacts, not fresh worktree-browser captures.
No browser, backend, desktop MCP call, agent run or test process was started
for this continuation. The exact remaining dependency is a failing occurrence
with simultaneous pane measurement, grid width, child kernel winsize and raw
agent bytes. Until that exists, neither a TUIC cause nor agent hard wrapping
is proved, and a production fix would be speculative.
