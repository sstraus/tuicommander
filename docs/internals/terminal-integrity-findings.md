# Terminal integrity findings — 2026-09-21

## Initial pre-fix scope and method

These are synthetic PTY → parser/grid → HTTP text and binary styled-row tests
against an isolated, locally built `tuic-remote` instance on port 9877. No live
user session was modified. Build: `cargo build --manifest-path
src-tauri/Cargo.toml --no-default-features --bin tuic-remote`, through mbx;
`mbx doctor` reported zero failures and warnings. The build completed in 2m 41s.
The checkout contained other concurrent work; this is not a clean-commit or
release-build certification.

The final xterm matrix (seed 819, 12 rows × 72 columns) completed **3,163 cases**:
1,156 ordered pairs of 34 operations, 2,000 generated sequences, and seven named
boundary cases. It completed 30,502 concurrent scroll requests in 146.28 seconds.
**1,266 comparisons failed; 1,897 passed.** These are mismatching sequences, not
1,266 distinct defects. The command exits nonzero and has no failure allowlist.

Reproduce the complete matrix using the README setup and:

```sh
.tmp/terminal-integrity/venv/bin/python tests/terminal-stress/ansi_integrity.py \
  --auth USER:PASS --seed 819 --random-cases 2000
```

Retained local evidence is under `.tmp/terminal-integrity/xterm-final/`, including
`summary.json`. Each failure has exact synthetic PTY bytes, dimensions, reference
rows, canonical HTTP rows and decoded binary rows. The generator is the durable
reproduction if those temporary artifacts are removed.

## Confirmed representation loss and regression coverage

The retained pre-fix run of `--case unicode --random-cases 0` emitted decomposed
`cafe\u0301`. Both canonical
HTTP rows and the binary styled-row payload contain `cafe`, losing U+0301. Both
pyte and xterm preserve the combining mark (their NFC/decomposed spelling is
normalized before comparison). CJK text in the same case survives.

Code corroborates the boundary: `terminal_grid.rs::row_to_text` appends only
`cell.c`, and `encode_cell` sends a single u32 codepoint. Neither includes the
cell's additional zero-width characters. The frontend's `DecodedRow.codepoints`
is also one u32 per cell. This evidence proves loss in the exposed text and
wire representation; it does not prove the parser discarded the mark internally.

The focused Unicode case now has a product-owned oracle rather than using xterm
as the specification. It requires exact decomposed codepoints, three ordered
marks on one cell, clearing old marks when their base cell is overwritten, and
CJK/wide-character preservation. Base characters and following marks are sent
in deliberately separated PTY writes. The same exact rows are checked through
canonical HTTP text, binary styled rows, row text, selection text and search;
search offsets must identify the exact UTF-16 string range, while selection
continues to use terminal-cell columns. The case scrolls each retained absolute
row into the viewport before checking row text, then resizes down and back after
the rows have entered history and repeats all checks. NFC
normalization remains only in the broad diagnostic differential matrix and
cannot mask a failure in the dedicated Unicode assertions.

The styled-row decoder keeps the fixed 11-byte cell core and understands the
optional sparse `TCX1` trailer after all row records. It rejects truncated data,
bad magic, out-of-range wire-row ordinals or absolute columns, duplicate cell
entries, mark counts outside 1–9, invalid Unicode scalars and trailing bytes.
The retained artifacts predate this protocol. No post-fix HTTP run is claimed
until the coordinated backend build is available on the isolated test instance.

## ANSI contract findings

- `--case pair-text-insert-lines --random-cases 0`: after `CSI 2 L`, TUIC writes
  the next character at the previous column. DEC's VT220 contract resets the
  cursor to the first column. The fork inherits this behavior unchanged from
  upstream Alacritty.
- `--case pair-text-delete-lines --random-cases 0`: the equivalent inherited
  defect after `CSI 2 M`. Many generated failures compound these two operations.
- `--case pair-wrap-mode-erase-chars --random-cases 0`: after filling the right
  margin with autowrap disabled, re-enabling it and erasing characters, TUIC
  writes the next character on the next row. The inherited ICH, DCH and ECH
  handlers fail to clear pending wrap; IL/DL have the same issue in addition to
  their column reset defect.
- `--case pair-wrap-mode-erase-display-tail --random-cases 0`: a **separate**
  pending-wrap defect, in the opposite direction. With the wrap armed, the erase
  origin is past the right margin, so ED0 must leave the current line alone.
  TUIC erased its last cell and lost a character that was already on screen. The
  earlier reading of this case — "ED0 fails to clear pending wrap" — was wrong:
  the reference keeps the pending wrap here too, and so does TUIC. Only the
  erase range differed.
- `--case pair-wrap-mode-delete-chars --random-cases 0` exposed a third defect
  that has nothing to do with the pending wrap: `CSI Ps P` blanked `Ps` cells at
  the right margin even when fewer than `Ps` cells sat right of the cursor, so a
  delete of 3 at column 71 of 72 destroyed three characters instead of one. The
  wrap merely made a large `Ps` easy to reach. Only the cells actually removed
  may be blanked.
- The right-margin DECSC/DECRC mismatch is different: DEC specifies that save
  cursor preserves the wrap flag, and Alacritty does. `@xterm/headless` 5.5.0
  does not preserve it in this case. Production must keep the DEC behavior and
  the diagnostic oracle must represent this compatibility difference explicitly.

These are exact screen-content mismatches independently reproduced without an
agent. They are not evidence that the original Cost Memory table suffered one
of these mechanisms. Of the 135 failing ordered pairs, 134 reduce to the
inherited edit defects above and one is the DECSC/DECRC oracle difference.

The generated failures that named none of the trigger operations are now
classified rather than open. Twelve of them (plus the already-fixed Unicode
case) carry no IL, DL, ICH, DCH or ECH at all; every one contains ED and eleven
of the twelve also disable autowrap, which is the `pair-wrap-mode-erase-display-tail`
mechanism above. Re-derive the set instead of trusting this count:

```sh
python3 - <<'PY'
import os, re
root = '.tmp/terminal-integrity/xterm-final'
edit = {b'L', b'M', b'@', b'P', b'X'}
for d in sorted(os.listdir(root)):
    if 'pair-' in d or not os.path.isdir(os.path.join(root, d)):
        continue
    raw = open(os.path.join(root, d, 'raw.bin'), 'rb').read()
    if not (set(re.findall(rb'\x1b\[[0-9;?]*([A-Za-z])', raw)) & edit):
        print(d)
PY
```

## ANSI contract fixes and what they leave open (2026-09-22)

Four handler defects are corrected in the fork, each pinned by a `term::tests`
case that names its operation. `docs/backend/alacritty-integration.md` carries
the contract table; the short version is IL/DL reset the cursor column inside
the scroll region, IL/DL/ICH/DCH/ECH resolve a pending wrap, ED0 spares the cell
behind one, and DCH blanks only the cells it actually removed.

The reference contract was re-derived against `@xterm/headless` for every
operation before any code changed, including the ones that turned out to be
correct already — ED1, EL0, EL1 and EL2 are unchanged on purpose and their
current behaviour is now pinned too. Probing the oracle first is what caught the
wrong ED0 reading above; the code would have "fixed" a conforming handler.

Verification replays each retained failure's exact `raw.bin` through
`terminal_grid::tests::replay_capture_from_env` and compares the canonical rows
against the stored `expected.json`. This is the same parser and the same
canonical-row extraction production runs, without an instance or a PTY:

```sh
TUIC_REPLAY_FILE=$PWD/<failure-dir>/raw.bin TUIC_REPLAY_ROWS=12 \
TUIC_REPLAY_COLS=72 TUIC_REPLAY_OUT=/tmp/replay.txt \
cargo test --manifest-path src-tauri/Cargo.toml --lib replay_capture_from_env \
  -- --ignored --nocapture
```

**931 of the 1,266 retained failures now match xterm exactly, including 130 of
the 135 ordered pairs.** The 335 that remain split cleanly, and the split is
re-derivable rather than asserted:

| Residual | Count | Reading |
|---|---|---|
| row count grew and the payload contains DL or SU | 279 | the unfixed mechanism below; the growth equals the deleted line count in every sampled case |
| same row count, payload contains DECSC or DECRC | 38 | consistent with the known oracle difference — presence is a heuristic, not proof that DECSC caused each one |
| same row count, no DECSC or DECRC | 18 | genuinely unclassified; these still need minimization |

Of the five ordered pairs left, one is the DECSC/DECRC case where TUIC is right
and the oracle is not, and the other four are a single unfixed mechanism:

> **DL and SU push the lines they remove into scrollback.** `delete_lines` and
> `scroll_up` both route through `scroll_up_relative` → `Grid::scroll_up`, which
> feeds history whenever the region starts at line 0. The reference grows the
> buffer only for an index past the bottom margin: after `CSI 2 M` at the top
> row its buffer length is unchanged, while TUIC's row count grows by exactly
> the two deleted lines — the same `+2` in all four remaining cases. An agent
> TUI that repaints with DL therefore manufactures scrollback rows, which is the
> shape of the duplication the `ink-repaint` and `scrollout` scenarios exist to
> catch.

That one was **deliberately not fixed here.** Separating "scrolled off the
bottom" from "deleted by a control" changes `Grid::scroll_up`, and TUIC's
absolute row coordinate (`lines_scrolled` / `total_scrolled`) is built on its
current meaning — selection snapshots, search offsets and eviction rebasing all
read it. It is a fork architecture decision, not a handler patch, and it removes
scrollback a user may currently be relying on. Tracked as story `#834-1878`,
with a `DEFERRED (2026-09-22)` marker at the `delete_lines` call site.

### Resolved by #834-1878 (2026-09-22)

`Grid::scroll_up_with` now takes a `ScrollSource`, and only `Overflow` feeds
history. Re-measured by replaying every retained `raw.bin` under
`.tmp/terminal-integrity/failures/` through `replay_capture_from_env` twice —
once with the two control call sites routed back through `Overflow`, once with
the fix — so the before and after come from the same driver over the same set:

| | Match | Rows grew, payload has DL or SU | Other residual |
|---|---|---|---|
| Control scrolls as overflow (pre-fix) | 873 | 205 | 292 |
| Control scrolls as control (fixed) | 1032 | **0** | 338 |

The bucket the story exists to close is empty. Two cautions on reading the rest
of that table. The set is **1,371 directories**, not the 1,266 counted above, so
these totals are not comparable to the pre-fix figures in the previous section —
only to each other. And "other residual" grows by 46 because those captures
stopped differing by row count and now differ only in content: removing the
manufactured rows revealed a mismatch it had been masking, it did not create
one. They need minimisation, the same as the 18 already unclassified.

One ordered pair was in the DL/SU class on this set,
`pair-reverse-index-delete-lines`, and it now matches exactly. The nine that
still fail are untouched by this change: eight are tab-stop cases with identical
row counts, and `pair-text-erase-display` grows by 11 with no DL or SU in the
payload.

## Passing boundaries

The named two-table, tall-table redraw, ASCII autowrap, synchronized-table,
alternate-screen restore and history-cap cases all match xterm, including the
binary styled rows. The cap case emits 10,030 numbered lines and verifies exact
retention/eviction at the configured 10,000-row history limit.

The existing `atomic`, `progressive`, `timeout` and `reflow` scenarios pass at
both 50 and 2,000 records while the runner scrolls and resizes during production.
The reflow check now requires every intentionally emitted partial row as well as
the final row, in order. Focused checker tests cover deliberately missing,
reordered, duplicated and truncated rows, malformed `TCX1` payloads, exact
Unicode codepoint identity and the independent diagnostic oracle.

The existing `slash-pressure` and `ink-repaint` scenarios also pass at 50 and
2,000 records. `scrollout` passes at 50; its legacy all-records invariant cannot
be run at 2,000 against the 10,000-row grid cap, so cap behavior is covered by the
new exact `history-cap` oracle instead. Final focused runs confirm both table
cases still pass. The retained pre-fix Unicode run exits nonzero after harness
cleanup changes. All disposable sessions were deleted.

## Harness and reference findings

- Negotiate geometry with `/resize`: session creation has an initial wide grid.
  Assuming the requested creation width caused a false wrapping mismatch.
- Wait for a producer raw-mode handshake before racing resize with output.
  The old runner could resize while the shell was still editing its command.
- Basic Auth on every request dominates the workload; exchange it once for the
  existing session token before the request-intensive loop.
- ED2 retention is a declared TUIC policy, adapted explicitly in the reference.
  Xterm.js remains a diagnostic comparison for cursor/editing operations; it is
  not the product specification, and known contract differences are reviewed
  before changing production behavior.
- Pyte differs from both xterm and TUIC on margin reset and pending-wrap cases.
  It remains a secondary diagnostic oracle, not automatic evidence against TUIC.
- Equal raw/grid occurrence counts cannot establish integrity: erasure can be
  intentional, and counts cannot detect reordering or all middle-row omissions.

## Limits

The prepared HTTP Unicode case does not exercise WebSocket delta delivery,
GPU/canvas painting, the native WebView, Windows ConPTY or every ANSI
extension/parameter. Those visual and streaming boundaries require the
coordinated isolated-instance validation after the backend/frontend build. These
tests do not constitute a claim that the terminal can never lose data. The
original screenshot still needs its actual PTY stream to attribute that
particular corruption. The source transcript alone cannot assign fault.


## Implemented fix and final focused validation

The approved Unicode fix is implemented in the shared checkout: complete cell
text extraction, delayed-mark damage, exact native search, UTF-16 buffer-search
ranges and the optional `TCX1` wire trailer. Frontend decoding, partial merges,
scroll caches, painting and text consumers retain those extras. Details and
exact test commands are in `plans/terminal-unicode-integrity.md`.

- Native: 45 distinct targeted tests passed; the original U+0301 row-text
  regression changed from red to green.
- Frontend Unicode: 67 targeted tests passed, plus TypeScript, Biome and build.
- Python harness: 11 tests passed.
- Fresh isolated HTTP Unicode: 1/1 passed, 49 concurrent scroll requests,
  0.419 s, product-owned `native-exact` oracle (no xterm process).
- Two-table cases: 2/2 passed, 35 scroll requests, 0.215 s.

Runtime artifacts: `.tmp/terminal-integrity/post-fix-20260921-1801/`. The initial
Unicode attempt there failed because shorter stimulus rows inherited seed-row
suffixes; adding EL2 to each stimulus row fixed the harness. That failed evidence
remains under `unicode/`, while the passing run is under `unicode-pass/`.

## Resize exposed a separate canvas visibility defect

A rapid viewport resize followed by scroll-to-top left the canvas uniformly
blank while canonical rows, styled rows and the actual browser's proxied WS
stream were nonblank and valid. The page was visible, standalone RAF worked,
but live frames caused no render scheduling; forcing an intersection out/in
crossing recovered immediately. `IntersectionObserver` consumed `entries[0]`
instead of the newest timed observation, stranding its private hidden gate.
The frontend now selects the newest entry; 24 targeted visibility/mount tests
passed, including three new regressions, followed by TypeScript/Biome/build.

Fresh-dist reproduction passed without forced visibility crossing:
1200×1223 → 1000×700 → scroll-to-top. At the bottom after resize the canvas had
110,889 non-background pixels, 2,961 fillText calls and three RAFs. At the top it
had 71,216 non-background pixels, 537 fillText calls and one RAF; scrollbar
position followed correctly. Root inspected the readable screenshots under
`.tmp/terminal-integrity/post-fix-20260921-1819-frontend/`.

The browser used Chrome 149.0.7827.54 through the required stealth wrapper
(`navigator.webdriver` undefined, UA major 149). A disposable same-origin proxy
served hash-verified current dist and forwarded to isolated backend 9877. The
headless build does not expose desktop-feature-gated static/settings routes;
the settings-load toast/default configuration is a harness limitation. This is
debug browser-mode evidence, not native WebView, release or Windows/Linux proof.
Repro support scripts are retained under `.tmp/terminal-integrity/` as
`browser_proxy.mjs` and `capture_grid_frame.mjs`.

Exact decomposed `e+0301` reached canvas fillText and was legible. The stacked
`A+0301+0308+20DD` reached one fillText call with exact scalars but showed font
fallback boxes. This validates character retention, not universal font coverage.
The existing nine-mark bound and whole-cell search-result ranges remain.

## Clipboard isolation incident and cleanup

The real browser selection test triggered terminal auto-copy and overwrote
Boss's host clipboard; Boss observed `RECOMPOSED: café` when pasting. No pre-test
clipboard backup existed. No clipboard/history reads or attempted restoration
were performed. Root interrupted the worker and stopped further interaction
tests. The planned block-cursor-on-accent visual check remains **unverified**.
Named app/browser instances isolate configuration and sessions, not the OS
clipboard. The workflow now requires verified clipboard interception/denial
before any selection/copy/paste test; saving/restoring the shared clipboard is
not a safe substitute while the user is working.

Cleanup completed: disposable PTY deleted (HTTP 200 and explicit close/exit in
backend log), named backend/proxy stopped (9877/9878 unbound), dedicated browser
daemon closed; unrelated browser session `mw004` was left untouched. Production
sessions/config on 9876 were not used, but the host environment was **not**
untouched because of the clipboard write.

The original Cost Memory cause and separate ANSI edit defects remain unresolved.
No full post-fix differential-matrix pass is claimed. Mutation validation remains
pending an authorized commit because the gate tests committed HEAD. Boss's live
Rust backend still requires a deliberate restart to load the fix.
