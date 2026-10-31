# Ego answer width and gutter investigation

Status: Implemented with targeted rb GREEN. Reproduction and layer diagnosis are complete. Independent critic and aggregate integration checks are pending. Evidence limitations are listed below; this is not a claim of full desktop acceptance.

## Scope and contract

Investigate Italian answer fragments and the missing green continuation gutter in a wide TUIC pane. Preserve the user-selected reflow setting. Fix only lost serialized wrap provenance. Do not change ego output, the MCP dimension default, or persisted configuration. Do not restart the installed app, land, or push.

Story: `2560-b448`. Worktree branch: `fix/ego-wrap-answer-bar`. Pre-fix HEAD: `664db9ace3260b3085f6a637e445a91874b50c0e`. Implementation commit: `2c210f9816a7fd446bd0e6eb096e0dead1dabc96` (later documentation-only evidence updates may follow). All five story criteria are checked in the main store; the story remains open for the parent's critic and integration.

## Real runtime evidence

Evidence root: `/Users/stefano.straus/Gits/.tmp/ego-wrap-bar/`.

The installed `/Users/stefano.straus/.local/bin/ego` reports version 0.4.0. It ran against a deterministic local streaming provider (`provider.py`, port 21682). The provider supplies a long Italian answer; real ego emits and TUIC renders it. These are not hand-set grid states.

The no-dimensions PTY is `a71fe60e-3aaf-4fdf-9ce0-b4c72231da34`. `stty size` reports `24 80`. After the answer completed, the session was resized to 200 columns and 40 rows. The control PTY `db773a96-eda8-4776-9de3-423f4393203e` started at 200 columns and 40 rows.

Command, with separate scratch homes for the two sessions:

```sh
EGO_HOME=/Users/stefano.straus/Gits/.tmp/ego-wrap-bar/home \
OLLAMA_HOST=127.0.0.1:21682 \
/Users/stefano.straus/.local/bin/ego --bare --sandbox off --no-tools \
  --thinking off --model ollama/qwen3 'rispondi con il testo di prova'
```

Before resize, the answer occupies five physical rows and has four outgoing wrap edges. After non-reflow resize, those same fragments remain at their old width, but the styled-range wire reports no outgoing wrap edges. The 200-column control occupies two answer rows and reports one wrap edge. Mid-word fragments follow the old physical column boundary, not emitted paragraph newlines.

The running installed app returned `{"scrollback_reflow":false}` through the read-only Tauri `load_config` command, projected to that one field. The saved styled log is the runtime evidence; no configuration was written by this investigation. The coordinator later confirmed that Boss enabled reflow at 12:16, after these captures. The false setting was retained from an earlier default. This report does not describe the current configuration as false.

### Evidence files

- `ego80-before-text.json`, `ego80-after200-text.json`, `ego200-control-text.json`: session output, including real ego answer rows.
- `ego80-before.bin`, `ego80-after200.bin`, `ego200-control.bin`: original styled grid wire bytes.
- Matching `*-styled.json`: debug log responses that contain the byte arrays.
- Matching `*-rows.json`: decoded row text and outgoing wrap bits.
- `production-marker-replay.png`: worktree Vite/browser screenshot of the recorded grids.
- `production-marker-results.json`: production marker planner results. Answer gutter rows are `[0,1,2,3,4]` before resize, `[0]` after resize, and `[0,1]` in the wide control.
- `answer.txt`, `requests.jsonl`, `provider.py`: deterministic provider input and requests.

The early `.ansi` captures contain only a short raw tail. A later explicit raw-output read recovered the complete retained streams: `ego80-full-raw.json` and `ego200-full-raw.json`, decoded as `ego80-full.ansi` and `ego200-full.ansi`. Both start at byte 0 with oldest offset 0, `has_more=false`, and `truncated=false` (3264 and 1966 source bytes). The extracted `ego80-answer-raw.txt` and `ego200-answer-raw.txt` are identical 385-byte marked answers with zero CR and zero LF between the marker and `END-WRAP-BAR`. This directly excludes ego-inserted row breaks as the cause.

Capture command, repeated for the wide control session:

```sh
tuic mcp session '{"action":"output","session_id":"a71fe60e-3aaf-4fdf-9ce0-b4c72231da34","format":"raw","limit":200000,"from_byte":0}'
```

## Rendered production evidence

The worktree Vite harness at `.tmp/ego-wrap-bar/index.html` decodes the real wire recordings with `decodeStyledRange`, paints text through `createGridRenderer`, plans markers through `planCanvasMarkers`, and draws gutters with `paintOverlayBlocks`. It derives continuation state from the recorded preceding row, as production does.

This is a browser replay of production renderer components/functions, not a mounted full `CanvasTerminal` or a rebuilt desktop app. No installed TUIC restart was performed. The screenshot and result file prove the marker pipeline consequence of the missing wire provenance. Full-component transport mounting remains a validation gap.

## Findings

### Narrow fragments: terminal sizing and configured non-reflow

Owner: TUIC MCP session creation and terminal grid configuration, not ego prose emission.

`src-tauri/src/mcp_http/mcp_transport_session_agent.rs` defaults omitted dimensions to 24 rows and 80 columns. The live no-dimensions session confirmed this value. A visible pane becoming wider later cannot retroactively change retained physical rows when reflow is disabled. The wide control shows that ego can use the wider grid.

The no-reflow path was already present in `75496f4ddc`: `ReflowMode::None` declines reflow and later calls `row.grow(columns)`. Thus retained old-width rows are expected configured behavior, not an established ego regression. The coordinator traced the old false default to that commit and the true default to `db14f3e667`; existing explicit false values were retained. No migration is authorized.

The specific original screenshot session was not inspected, so its launch path remains an inference. The controlled experiment proves the mechanism, not the provenance of that original PTY.

### Missing gutter: serialized wrap provenance

Owner: TUIC terminal grid row serialization.

The fork keeps the original `WRAPLINE` cell when a row grows without reflow. It adds blank cells; it does not move the edge to the new last column. `encode_col_count` checks only that new last column. The real retained edge is therefore omitted from both styled-range and live-frame wire flags. The frontend correctly treats unmarked rows as separate rows and stops extending the answer gutter.

This resize case never worked in the serialized wrap contract: the margin-only probe was introduced with the wire wrap bit in `1c7c7bc9f18ddcf206e87676718a55e5d5f9fec7` on 2026-08-07. That commit is the contract origin, not evidence of an earlier working gutter resize. The current and original fork non-reflow growth retain cells at their old positions. Later gutter gap changes are not required to explain this failure.

Implemented: inspect retained terminal `WRAPLINE` flags across the row when serializing its wrap bit. No prose inference, new wire fields, glyph relocation, reflow setting change, or copy policy change is included. The scan is bounded by the same row width already serialized. The fork already retains the correct flag, so no fork mutation is needed; the owning defect is the encoder's margin-only interpretation.

## Validation

Regression: `non_reflow_growth_preserves_answer_continuation_flags` in `terminal_grid.rs`. It feeds a recorded logical answer into the real parser at 80 columns, verifies four wrap edges, grows to 200 with reflow disabled, then compares serialized flags and original cells. It also covers live frames, repeated growth, explicit newlines, erasure, and truncation.

Fixture: `src-tauri/crates/tuic-terminal/tests/fixtures/ego-wrap-2560/answer.txt`, reconstructed from the recorded original grid by omitting wide-cell spacers and final padding. No wrap bits are hand-set.

RED command launched through `tuic-bg.sh` and `build-slot.sh --remote`:

```sh
scripts/with-test-tmp.sh cargo clippy --manifest-path src-tauri/Cargo.toml \
  -p tuic-terminal --all-targets -- -D warnings
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml \
  -p tuic-terminal --lib \
  -E 'test(non_reflow_growth_preserves_answer_continuation_flags)'
```

RED log: `/Users/stefano.straus/Gits/.tmp/ego-wrap-bar/red-2560.log`. Clippy passed. The single test failed as required on the pre-fix production source: actual wrap bits were `[false,false,false,false,false,false]`, expected `[true,true,true,true,false,false]`. Runner exit: 100.

GREEN ran the same Clippy command, followed by the targeted nextest filter:

```text
test(non_reflow_growth_preserves_answer_continuation_flags) |
test(serialized_rows_carry_the_wrapline_flag) |
test(styled_range_rows_carry_the_wrapline_flag) |
test(partial_row) |
test(reflow_history_disabled_truncates_scrollback)
```

Result: Clippy passed; 4 tests passed, 842 filtered out; exit 0. The `partial_row` term matched no additional test, so no dedicated partial-span regression execution is claimed. The production change is shared by the partial serializer path; its existing test coverage and full integration remain for the critic/batch owner. GREEN log: `/Users/stefano.straus/Gits/.tmp/ego-wrap-bar/green-2560.log`. Both runs were on rb/Linux; no native Rust GREEN or rebuilt installed app is claimed. The native Mac evidence is the real installed ego PTY experiment and Vite browser capture.

Risk: medium. Shared row serialization affects all marker consumers, but the wire layout, terminal cell state, configuration, and copy policy do not change. A separate critic is required before integration.

## Remaining gaps

- Independent critic and aggregate integration validation.
- Full mounted component capture, if production-function replay does not meet acceptance.
- Verify archived ego story details through the supported story CLI; earlier archive reads were unavailable.
- Installed ego binary provenance is not established by source ancestry. The real tested output does contain the answer marker.
- The original screenshot session launch path is not proven.

## Handoff and cleanup

`fmt-changed.sh <worktree> main...HEAD` passed. `story-close.sh --check <main> 2560-b448 <worktree>` passed without writing. No landing or push was performed.

All five experiment/provider/Vite PTYs were closed after their captures. The named browser capture tab and its agent-browser daemon were closed without closing the shared Chrome browser. Ports 21682 and 21683 no longer accept connections. Required evidence and the Vite replay sources were retained under the evidence root.
