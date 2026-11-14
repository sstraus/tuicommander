# Pi 0.84.2 extension evidence

Captured on 2026-11-01 from the installed CLI with an isolated HOME and PI_CODING_AGENT_DIR under `~/Gits/.tmp/pi-capture/tuic-2629/`. No daily agent profile was read or changed.

## Native recordings

- `pi-extension-api.jsonl`: selected callback fields and modal return values recorded by an extension using the actual CLI. The TUI run covers select/confirm/input/editor/custom answers and Escape cancellation, a custom factory exception, new-session replacement and clean quit. A one-shot tool run records agent_start, tool_execution_start, tool_call, tool_execution_end, agent_end and session_shutdown. A separate native RPC set_model request records model_select with idle=true and no agent_start. Model identity and provider responses are omitted.
- `pi-extension-modals-2629.tcap`: original input/output chunks, monotonic times and 41x128 geometry from a real PTY with a controlling terminal. The generated managed extension and the probe were both loaded by Pi. Eleven modal promises emit awaiting followed by idle. The immediate factory failure was recorded as adjacent output records; replay also coalesces that pair to cover a legal PTY read boundary.
- Original capture driver, probe, action log and unfiltered terminal bytes remain in the isolated evidence directory. Command: `pi --no-session -e <evidence-dir>/probe.ts`; global extension auto-discovery loads the generated `extensions/tuic.ts`. The final capture exited 0 after native Ctrl+C quit. All owned capture processes exited.

## API ownership

The installed public ExtensionAPI has lifecycle callbacks, including session_start/session_shutdown, agent_start/agent_end and tool events. It has no ui_prompt_start/end callbacks. Public ctx.ui methods return promises; ctx.ui is shared by the extension runner. The live capture proves that wrapping it at session_start observes calls made by another extension. The paired finally preserves results and errors. Print methods are no-ops and RPC has a different UI owner, so only TUI modal promises become PTY waits.

Agent_end fires while ctx.isIdle() is still false in the native tool run. The extension therefore records agent_end directly instead of deriving completion from ctx.isIdle() there. Model selection is not a busy event. Session shutdown runs on new-session replacement and clean quit.

The generated-source unit test injects terminal IO and modal promises at our own callback boundary; it is not a vendor recording. The PTY tests replay the native fixture through ChunkProcessor and the production accumulator. The initial native replay exposed empty-title repaint deduplication after the first Pi modal; lifecycle markers must re-arm each new modal even when their empty titles are identical. Windows behavior and the rebuilt Settings screen were not exercised.

## Installed source fingerprints

- `dist/core/extensions/types.d.ts` SHA-256 `f2c7df6c808598794ef528d758ea18891828b6d97251a20ea74d92108e63d645`
- `dist/core/extensions/runner.js` SHA-256 `b39d59b8f86693b9aca15f13e14f367fce9a0ad8ed7ce9ad17950906f226951d`
- `dist/modes/interactive/interactive-mode.js` SHA-256 `e4c84f7e3b884533db5a8a6e6b65f34767fb3ade30a2f4213202d99e7ff4d8b6`

## Capture fingerprint

- `pi-extension-modals-2629.tcap` SHA-256 `7b5518c430ce7afa275c649b8e286c5d8e59bbf1c1480d12c4b975e451428b03`

## Reviewed replay-oracle update (round 1)

The corpus discovery run produced 28 changed existing goldens. Each difference
was reviewed against the original binary recording, not only the new JSONL.
All 42 insertions are empty `user-input` events with line -1 at a recorded
OSC 7770 `state=idle` receipt. Removing only these insertions reproduces every
old trace exactly: no state snapshot, question, choice signature, intent, error,
geometry or output-ring length changes. No event was deleted or replaced.

Accepted idle closes a native wait in receipt order, including open/close in one
PTY read. A clear when no wait exists is idempotent. Submit and completion are
different lifecycle receipts even when their empty clear events are identical.
The parser still rejects a delayed completion over unanswered native controls.
Some older recordings still report `working` after a shell idle event because
of their existing evidence state; this update does not relabel those states.

The accumulator also clears slash-menu/choice state and attempts a guarded
queued-message flush on `user-input`. These are completion effects, not evidence
that a human typed text: empty content cannot replace `last_prompt`. The flush
still requires an idle agent with no confident question. These recordings have
no queued peer payloads, so they establish trace parity, not delivery coverage.
No old choice/menu event changes; later native controls must still be detected.

Record numbers below are zero-based TCAP records, not JSONL line numbers. Each
row states why its new clear is accepted.

| Recording | Idle records | Review decision |
|---|---|---|
| `claude-askuser-multi-question-20261012.tcap` | 39, 178 | The two completion frames are outside the unanswered wizard; the wizard SET/CLEAR sequence stays unchanged. |
| `claude-await-exit-before-input-1985.tcap` | 682 | The final completion follows the departed wait. It does not revive the retained footer. |
| `claude-preview-chat-2536.tcap` | 131 | Completion ends the preview turn; the preview and choice events stay unchanged. |
| `claude-question-exit-2091.tcap` | 144 | The matching protocol-question-cleared already precedes completion; the additional clear is idempotent. |
| `claude-suggest-consumed-2029.tcap` | 104, 150 | Both completed turns may clear a wait; the consumed suggestion and later busy frames stay unchanged. |
| `claude-suggest-stop-hooks-2029.tcap` | 76 | The stop hook completes the turn; suggestion metadata stays unchanged. |
| `claude-suggest-two-turns-2029.tcap` | 93, 139 | Each turn completes separately; both suggestion decisions stay unchanged. |
| `codex-0.154-runtime-hook-idle.tcap` | 0 | The actual runtime idle frame explicitly reports completion and no open dialog. |
| `codex-headless-short-20261008.tcap` | 370, 371 | Both completion receipts are accepted after the short response; the second clear is idempotent. |
| `codex-plan-question-20261019-real.tcap` | 525 | This completion follows the answered plan question; it is not the delayed idle rejected while controls are active. |
| `codex-request-user-input-20260929.tcap` | 698, 5643 | The first completed turn and final cleared question both accept idle; the final protocol clear remains before idle. |
| `codex-streaming-intent-20260924.tcap` | 1253, 1258 | Both completed streaming turns accept idle; the intent decisions stay unchanged. |
| `ego-answered-selector-1997.tcap` | 48, 78 | Both selectors have been answered before completion; retained selector history never re-arms awaiting. |
| `ego-clean-lifecycle-1902.tcap` | 5 | The combined submit and completion record contains two distinct lifecycle receipts, each with its own clear. |
| `ego-composer-1957-narrow.tcap` | 27 | The narrow composer submit/completion record has no active modal; retaining both lifecycle clears preserves receipt order. |
| `ego-composer-1957.tcap` | 24 | The wide composer submit/completion record has no active modal; retaining both lifecycle clears preserves receipt order. |
| `ego-enforced-approval-2028.tcap` | 354 | The enforced approval has settled before completion; native approval decisions stay unchanged. |
| `ego-intent-1964.tcap` | 18, 42 | Both intent-bearing turns complete; the idle clear is distinct from the submit clear. |
| `ego-intent-aside-wrap-2060.tcap` | 0, 0, 0 | Three recorded idle frames share one output record; all three are preserved rather than deduplicated by read boundary. |
| `ego-intent-joined-2060.tcap` | 0 | The joined intent carries an explicit idle frame; clearing a wait does not change title extraction. |
| `ego-mcp-questions-2088-2089.tcap` | 156 | The native MCP question has settled before completion; the multiline title and answer decisions stay unchanged. |
| `ego-native-questions-2028.tcap` | 320, 414 | Both native question sequences have settled before their idle receipts; completed state remains completed. |
| `ego-provider-failed-idle-2317.tcap` | 11, 23, 37 | Each failed attempt reports idle; the clear preserves error and recovery metadata and does not invent success. |
| `ego-provider-failure-2335.tcap` | 9 | Failure reports idle; the provider error still follows it and is not erased. |
| `ego-provider-failure-approval-2317.tcap` | 13 | Approval withdrawal already emitted protocol-question-cleared; idle adds an idempotent clear, not a new answer. |
| `ego-provider-recovered-2335.tcap` | 9, 26 | The submitted turn and recovered response each report idle; recovery and intent events stay unchanged. |
| `ego-provider-recovery-1902.tcap` | 26 | Recovery completion reports idle; provider recovery decisions stay unchanged. |
| `ego-slash-zero-2030.tcap` | 4 | Completion precedes later slash-menu interaction; its clear must not prevent the later menu from opening. |

The new `pi-extension-modals-2629.tcap.jsonl` has 249 trace entries. Its
scenario contract is independent of regeneration: initial not-awaiting, then
eleven ordered awaiting/not-awaiting pairs, one for each recorded modal.
The existing public accumulator tests additionally require exactly eleven
SET/CLEAR transitions and a clear after coalescing the immediate exception.
The recording, geometry, provenance and fingerprint above are unchanged.
`corpus.jsonl` adds this capture and updates only the reviewed event counts.

## Base behavioral RED and final targeted GREEN

Base: `0aa6ff23092105bbd23e0165e19bd7c0ee75bcb4`. A detached source
clone retained that base's production code. Only the two public accumulator
regression functions and this real TCAP/provenance were overlaid for the RED run.
On rb, both tests compiled and failed for behavior:

- `captured_pi_fast_modal_exception_cannot_latch_after_coalescing`: the
  accumulated wait stayed true after the recorded immediate close.
- `captured_pi_modal_completion_retracts_the_accumulated_wait`: observed only
  `[true, false]`, not eleven such pairs.

Nextest exit: 100, two tests run, zero passed. The same rb call ran
`cargo clippy --manifest-path src-tauri/Cargo.toml -p tuicommander --all-targets -- -D warnings`
and passed. Log: `~/Gits/.tmp/results/tuic-2629b-base-red.log`.
This is not RED for the generated-extension callback test: that test and its
production module do not exist in the base. It proves the public parser and
accumulator bugs without treating a compile failure as behavioral evidence.

After the reviewed golden update, rb ran the replay-oracle tests, Pi extension
and installer tests, Pi hook dispatch, captured Pi accumulator tests, state
mapping tests and named-instance hook restriction. All 16 targeted tests passed;
nextest execution took 12.570 seconds. Warnings-denied clippy also passed in the
same call. Log: `~/Gits/.tmp/results/tuic-2629b-final-green.log`.
No full-suite or final integration-batch pass is claimed.
