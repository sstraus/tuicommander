# Scroll-during-output REALITY evidence

## Scope

This audit supplies the three missing REALITY criteria for story `1264-89c8`.
The merged implementation was inspected at `950715de69`. No production code
or tests changed. Story metadata was updated only through `stories-cli.js` in
the main checkout. Criteria 18, 19 and 20 were checked after the targeted run,
with a worklog entry before each check. The story was not closed.

## Behavior contracts

### Output while scrolled back

- **Effect:** the viewport remains `l2,l3,l4` after five new output lines.
  The capped-buffer sweep contains exactly `l47` through `l59`, in order.
- **State:** the real TerminalGrid parser derives history and display offset.
  See `src-tauri/crates/tuic-terminal/src/terminal_grid.rs:5389-5444`.
- **Assumptions:** numbered CRLF input tests the owned parser, not an actual
  agent stream or canvas pixels.
- **Radius:** one terminal grid, viewport and scrollback cap.

### Output during a wheel gesture

- **Effect:** the next wheel-up request selects older rows, rather than
  jumping forward across the five new output lines.
- **State:** the real CanvasTerminal consumes modeled binary frames and wheel
  events. Transport and renderer are mocked. See
  `src/__tests__/components/CanvasTerminal-scroll-during-output.test.tsx:295-327`.
  History growth is computed at
  `src/components/Terminal/CanvasTerminal.tsx:1674-1679`; the controller rebases
  its position and pending target at
  `src/components/Terminal/canvasTerminalScroll.ts:184-192`, called from
  `src/components/Terminal/CanvasTerminal.tsx:1799-1801`.
- **Assumptions:** model frames arrive in order and report consistent history
  growth and backend offsets. This is not PTY or pixel evidence.
- **Radius:** one terminal gesture, pending scroll and settle target.

### Live rows committed to history

- **Effect:** the renderer receives current row text and a contiguous final
  run after live `V100..V103` become committed `F100..F104`.
- **State:** real CanvasTerminal cache logic receives hand-built binary frames
  and styled-row replies from one explicit model. See
  `src/__tests__/components/CanvasTerminal-scroll-during-output.test.tsx:364-420`.
  Committed rows are invalidated and stale in-flight rows rejected at
  `src/components/Terminal/canvasTerminalScroll.ts:157-172`, used from
  `src/components/Terminal/CanvasTerminal.tsx:1308-1333,1677-1679`.
- **Assumptions:** the modeled redraw is not a recorded agent fixture;
  transport and renderer are mocked. Actual appearance remains a VISUAL check.
- **Radius:** one terminal row cache, chunk request keys and in-flight fetches.

## Targeted verification

Run on 2026-10-14, against unchanged production and test sources:

```sh
build-slot.sh scripts/with-test-tmp.sh pnpm vitest run \
  src/__tests__/components/CanvasTerminal-scroll-during-output.test.tsx \
  src/components/Terminal/__tests__/canvasTerminalScroll.test.ts \
  -t 'keeps the lines under the user when output lands mid-gesture|shows the committed lines, not the live-region rows cached before they were committed|followHistory|commitLiveRows|cacheFetchedRows'

build-slot.sh --remote bash -lc \
  'scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml -p tuic-terminal -E "test(output_while_scrolled_back_keeps_the_viewport_lines) | test(scrolled_back_output_leaves_every_retained_line_in_order_at_the_cap)"'
```

- Local Vitest: **16 passed**, 18 excluded by the explicit name filter,
  2 files passed, 3.40 seconds. This includes the two cap variants and the
  six `followHistory` plus six commit/cache-fetch controller tests.
- rb Linux Nextest: **2 passed**, 961 excluded by the filter. Build:
  18.54 seconds; test summary: 0.009 seconds. `RB_BOX_DIRTY=no`.
- Full log: `~/Gits/.tmp/results/ego-1264-validation.log`.
- rb result: `~/Gits/.tmp/rb/20261014T143905Z-5894`.

The frontend run emitted unsized-pane and terminal-not-in-store warnings.
The assertions passed, but the run is not noise-free and does not establish
actual layout, canvas pixels or a live PTY sequence. No native macOS Rust,
full suite, desktop launch, restart or reload was performed.

## RED evidence boundary

The story already records the gesture RED failure on `bdbd763c1` and the
committed-row failure painting `V102,V103` before the row-cache fix. Those
historical observations were retained, not rerun or presented as new evidence.
The original generic RED entry has no precise pre-fix SHA in its worklog;
the backend tests are GREEN layer-localization evidence, not a backend RED.
This audit adds no regression and makes no stronger historical claim.

## Remaining limits

All five RED behaviors now have a checked REALITY criterion in the main story
store. Existing rewind and delayed-application REALITY criteria were retained.
Boss's VISUAL observation remains skipped and unobserved. No production gap
was found within this targeted scope. Shared viewport/protocol behavior remains
medium risk; this audit changes documentation and shared story metadata only.
