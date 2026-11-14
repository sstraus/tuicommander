# Selection integrity during output

Date: 2026-09-21. Focused validation passed; the installed application has not been
restarted. All runtime probes use an isolated test instance and disposable PTYs.

## Proven defects

1. **A new drag inherits the previous copied text.** Before reaching the cap,
   copy rows 4–6, start a different held selection across rows 8–13 and stream
   output. Full-frame revalidation compared the changing selection against
   the old cached text and cleared it. Runtime evidence shows overlay pixels
   dropping from 71,200 to zero with trusted `buttons=1` events and no mouseup.
   Canonical rows and base canvas stayed unchanged; viewport origin remained
   134 (`B=54`, `H=454→504`, `O=374→424`). New gestures now discard the old text
   snapshot, and content revalidation excludes active drags.
2. **Selection drift at the scrollback cap.** The frontend persisted
   `historySize - displayOffset + viewportRow`. When a parked full history
   buffer evicts a row, this coordinate changes although the visible text does
   not. The stable identity also includes `historyBase`. Evicted endpoints must
   be invalidated rather than selecting the replacement oldest row.
   Pre-fix browser cap evidence confirms both released and held selections
   disappear: `cap-old/released.json` keeps origin 5586 while `B=586→636`,
   `H=10000`, `O=5000→5050`; overlay pixels fall from 31,840 to zero.
   `cap-old/held.json` keeps origin 5636 and falls from 71,200 pixels to zero.
3. **Copy race during eviction.** Converting stable endpoints using the latest
   frontend frame is insufficient: the backend can evict more history before
   reading them. The optional `historyBase` selection argument allows rebasing
   under the same grid lock as text extraction. Expired endpoints return an
   explicit error (HTTP 409), without copying replacement text.
4. **Incomplete render state while recovering a frame.** The old frontend
   cleared its row map and adopted a changed viewport when receiving a delta,
   before the requested full replacement arrived. Mouse-driven painting could
   observe the empty/incomplete map. Recovery must retain a coherent accepted
   viewport, and a screenful of partial column spans is not a full replacement.

Native scrolling marks the grid fully damaged. These code-level findings do
not establish that native rows were lost in Boss's original session, nor do
they attribute the earlier Cost Memory table corruption to a specific producer.

## Runtime safety and baseline

The browser installs `clipboard_guard.js` before application code. Function
identity checks precede every sentinel invocation; navigator writes and legacy
copy/cut paths are intercepted in-page. A real application copy route was also
captured. Nothing reads, writes or restores the host clipboard. Browser mode is
asserted using the production `isTauri()` predicate, including its shim flag.
Stealth and clipboard initialization must be combined rather than allowing one
init script to replace the other.

Baseline evidence is under
`.tmp/terminal-integrity/selection-motion-20260921/`. The initial held/released
selection probes use numbered rows with a parked viewport before the history
cap. Root visually inspected `evidence/released-after-output.png`: rows 80–107
remain legible and row 90 remains highlighted. This is a baseline, not proof of
the cap correction or all possible ANSI sequences.

## Validation ledger

- Seven targeted native selection tests pass, including snapshot rebasing,
  eviction rejection, future-snapshot rejection and legacy omission behavior.
  The full-frame eviction assertion consumes the initial frame first.
- Frontend owner reports 285 targeted tests in six files passing, plus TypeScript
  validation and a production build (3,127 modules, Vite 6.22 seconds). Fresh
  main asset: `main-B4yL5XKO.js`.
- The two additional direct decoder consumers (`cellExtrasWire.test.ts` and
  `frameWrappedRows.test.ts`) also pass: 17 tests in 768ms, bringing this task's
  frontend total to 302 tests across eight files without rebuilding or editing.
- Native integration build passed in 1m40s after the fresh frontend build.
  `mbx doctor` reported zero warnings/failures; the five changed Rust files pass
  rustfmt. The shared tree's global formatting check still reports unrelated
  Kokoro/dictation formatting; those files were not changed by this task.
- Fresh-build browser held/released cases and HTTP selection checks pass.
  Owned sessions were deleted, browser/proxy/server stopped, and root confirmed
  ports 9877/9878 have no listeners. No full-suite or native WebView validation
  is claimed here.

## Fresh-build browser and HTTP results

Evidence: `.tmp/terminal-integrity/selection-motion-20260921/cap-fixed/`.

- Original pre-cap held-drag reproduction now passes as well: see sibling
  `precap-fixed/held.json`. A different previous selection is seeded by the
  runner itself; 50 output rows move `B/H/O` from `1/153/73` to `1/203/123`
  while origin stays 81. Base text stays unchanged and overlay remains visible
  (71,200→71,360 pixels). Root inspected its final screenshot.
- Released multi-row selection: 50 output rows advance history across the cap
  (`B=27→76`, `H=9999→10000`, `O=4999→5049`) while origin stays 5027. Base and
  overlay pixel hashes remain exactly unchanged; overlay retains 31,840 pixels.
- Held multi-row selection after a different copied selection: 50 output rows
  advance `B=76→126`, `H=10000`, `O=5000→5050`, origin stays 5076. Base canvas
  hash stays unchanged. Overlay remains visible (71,200→71,360 pixels; the
  intentional held mousemove extends the last column). Root inspected
  `held-after.png`: rows 5081–5086 remain highlighted and surrounding text is
  legible.
- HTTP snapshot contract: snapshot `B=76`, grid row 5008 keeps returning
  `SEL-05081` after eviction advances. The legacy request without `historyBase`
  correctly returns the current grid row `SEL-05131`; a deliberately evicted
  endpoint returns HTTP 409 with the documented error. See
  `http-selection-contract.json`.

The headless test binary lacks the settings route, producing the visible
settings warning. The test terminal is intentionally outside a registered repo.
Neither warning is a failed selection assertion. This run does not certify
Windows, the native WebView, release packaging, or every ANSI permutation.

The durable runner checks real 50-row progress before behavioral assertions,
uses a configurable 120-second setup budget, derives the actual screen height,
and releases held mouse buttons in `finally`. It refuses primary port 9876 by
default. Python compilation, explicit primary-port refusal and diff checks pass;
the clipboard guard remained active throughout all three fresh-build UI cases.

Focused validation commands (Cargo used the required mbx PATH):

```sh
cargo nextest run --lib -E 'test(/get_selection_text|selection_snapshot_rebases_across_scrollback_eviction/)'
pnpm exec vitest run src/components/Terminal/__tests__/canvasTerminalSelection.test.ts src/components/Terminal/__tests__/decideFrameGrid.test.ts src/components/Terminal/__tests__/framePartialRows.test.ts src/components/Terminal/__tests__/frameAltScreen.test.ts src/__tests__/transport.test.ts src/__tests__/components/Terminal/canvasTerminalMountGuards.test.ts
pnpm exec vitest run src/components/Terminal/__tests__/cellExtrasWire.test.ts src/components/Terminal/__tests__/frameWrappedRows.test.ts
pnpm exec tsc --noEmit
pnpm build
cargo build --no-default-features --bin tuic-remote
```

Native test execution took 0.056s; its build phase took 2m04s including a shared
Cargo-lock wait. Vitest reported 2.44s. Scoped Biome checked 14 frontend files.
The new Rust method's initial red run was a missing-method compile failure;
frontend decision/coordinate regressions failed before their implementation,
and the held-drag defect additionally has a real pre-fix browser reproduction.

The per-change mutation gate remains pending until the changes are committed:
the repository mutation script tests a committed range, and this task does not
authorize a commit. No history-era token was added to the API; an indistinguishable
screen/history reset with the same base remains outside this correction.
The request-failure retry branch was code-reviewed but has no dedicated injected
component failure test; frame decisions and replacement sequences are covered.
