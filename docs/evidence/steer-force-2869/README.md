# Forced steering — 2869-3e82

## Contract and entry points

Steer means cancel the active turn, confirm a ready composer, then submit the
full owner text. Plain Enter into a busy CLI is not steering.

| Entry point | Route |
|---|---|
| Session Chat Send now, desktop and mobile | `TerminalChatTranscript.actOnQueued` → `send_queued_agent_command_now` → `send_queued_command_now` |
| Compose queued-item Steer (2862-c636) | `Terminal`'s `onSteerQueued` → the same command and queue-id claim |
| Tauri and HTTP Send now | `pty/commands.rs` and `mcp_http/session.rs` → the same Rust function |
| `agent send urgency=urgent` | `mcp_transport_peer.handle_messaging` → `deliver_interrupting_mail` |
| `agent send urgency=interrupt` | Alias for the same force request |

All PTY routes reserve the next ready edge and call `deliver_reserved_steer`.
Claude Code, Codex and ego use Escape, independently confirmed idle, then the
existing split-write command writer. There is no provider-specific force path.
The `send_now` capability is `interrupt` for all three.

The reservation is not a second text queue. A selected item leaves the FIFO
before cancellation. The reservation prevents an ordinary FIFO drain or mail
wake from taking the cancellation's ready edge. It holds no input mutex while
waiting, so native answers and raw human input remain available. An unwritten
queued item returns on refusal. An uncertain payload never retries. Failed
urgent mail stays inbox-only and does not create a queued notice.

AI Chat over ACP is a distinct transport, not a PTY Chat button. Its advertised
native `_ego/steer` path remains in `acp/connection.rs`; ordinary ACP prompt
serialization and the subscribed inbox resource are not PTY input and are not
rerouted into a terminal. No ACP fallback was added.

## Cause and bounds

The old Claude/ego Send now path wrote Ctrl+B, text, then plain Enter. The old
urgent path wrote an inbox pointer plus plain Enter. Neither forced the text
past a running tool or Stop hook. Codex already used cancellation, but its
queue-head priority logic was separate from urgent delivery.

The shared wait is bounded at ten seconds, sampled every 50 ms. Input framing
adds the existing 50 ms clear-line gap and the harness's existing Enter gap.
There is one active force reservation per PTY. Concurrent force requests cannot
write a second Escape; already-claimed queue ids return false. Native dialogs,
drafts, unknown foreground ownership and uncertain input still refuse delivery.
This is cancellation, not rollback: interrupted work can be partial and child
processes can continue.

The retired busy-notice coalescer, background-key writer and their tests were
removed. Their old assertions required deferral, the behavior this story removes.
Normal inbox/wait ownership and normal idle wakes remain unchanged.

## Native Stop-hook evidence

Recorded on macOS on 2026-11-06 from the installed Claude Code **2.1.318** in a
throwaway 40-row, 140-column PTY. Each run used an isolated config and cwd under
`~/Gits/.tmp/steer-force-evidence/native/`, no TUIC identity or MCP server, and a
local fake Anthropic HTTP endpoint. The endpoint returned one text answer, then
the actual CLI ran a Stop command that waited for 60 seconds on its first call.
No paid model was called. The CLI, its key handler and its hook runner were real.
The marker `OWNER_CORRECTION_2869` was checked in the next real client request.

- **Old path:** Ctrl+B at 1.754727 s, owner text at 1.907806 s, Enter at
  2.062142 s. The hook remained active and the local API had not received the
  owner text at the two-second observation. Escape at 3.762092 s then released
  that queued text; the CLI's next request contained it.
- **Forced path:** Escape at 2.767828 s while the Stop hook ran. The CLI rendered
  `Interrupted · What should Claude do instead?`. Owner text at 4.834801 s and
  Enter at 4.989702 s reached the next client request, before the hook's natural
  60-second completion.

These observations prove CLI admission during a running Stop hook, not that a
model followed the request. The native probe used a fixed two-second settling
interval; the backend instead waits for its real ready-state evidence. Backend
writer tests cover the reservation and delivery order. No test desktop or daily
TUIC instance was launched or restarted. Current Codex and ego native key
semantics were not re-probed; their shared backend route is tested, and the
existing interrupted-screen adapters are unchanged.

Raw input/output chunks, monotonic timestamps and geometry are retained in:

- `src-tauri/src/fixtures/agent_prompts/claude-stop-hook-steer-old-2869.tcap`
- `src-tauri/src/fixtures/agent_prompts/claude-stop-hook-steer-forced-2869.tcap`

The local probe source and API-request evidence are retained under
`~/Gits/.tmp/steer-force-evidence/` (`probe.py`, `native/old2`, `native/new2`).
The dummy API-key approval text in the capture is not a credential.

## Regression evidence

Pre-fix commit: `446abbfe903b8cfc41b74ebd9086771084d6b581`.

The new test `steer_interrupts_each_working_harness_before_delivering_owner_text`
was written first. On that pre-fix production tree, rb failed at the first
Claude busy write: actual `[2]` (Ctrl+B), expected `[27]` (Escape). The final
assertions also require the selected full text before the older queued item.

RED command, through `build-slot.sh --remote` with `RB_FORCE=1` after the
macOS-path preflight:

```sh
cargo nextest run --manifest-path src-tauri/Cargo.toml --lib \
  -E 'test(steer_interrupts_each_working_harness_before_delivering_owner_text)'
```

RED log: `~/Gits/.tmp/steer-force-evidence/red-native-rb.log`.
The required native attempt was killed with signal 9 (exit 137) before a test
result; no native Rust result is claimed. The rb RED ran and failed as expected.
The first GREEN upload exited 75 (`tree changed during sync`) before starting
Cargo because evidence files were added during upload; it is not a test result.

Local validation: Vitest 5.0.0, pnpm 12.15.0: both touched component test files
passed (42 tests). The isolated Chat test then gained the same activity-store
hydration setup as its sibling test; its four cases were rerun without the
previous unhydrated-promise leak. The other 38 cases remain valid by content hash. `pnpm exec tsc --noEmit` passed. Source hashes and commands
are in `~/Gits/.tmp/steer-force-evidence/green.log`.

The separate critic and batch integration gates remain coordinator-owned.
Rust changes require a manual `make dev` restart or release rebuild before the
running application uses them. The post-rebuild check is in
`to-test/agents-pty-2.md`.

The first completed Rust run passed 28 of 31 targeted cases. It found one stale
MCP-description assertion and two input-lock contention failures. A routine FIFO
flush briefly held the transaction without owning the composer; `try_lock`
incorrectly refused the force request. The shared flow now serializes those
short critical sections instead. Only force consumers and the corrected catalogue
contract are rerun; unaffected mailbox results remain valid.

## Final targeted GREEN

Rust ran on rb (`aarch64-unknown-linux-gnu`) with rustc **1.99.1**
(`58cf8f9c3`, 2026-10-13) and cargo-nextest **0.9.146**
(`8af696ddc`, 2026-09-21). Commands used `build-slot.sh --remote`, `RB_FORCE=1`,
and `scripts/with-test-tmp.sh`, with the repository default features/profile.

1. The force-consumer rerun selected
   `test(pty::tests::chat_) | test(steer_interrupts_each_working_harness) | test(queued_commands_report_the_harness) | test(interrupt_mail_) | test(urgent_mail_) | test(agent_send_schema_and_description_explain_forced_urgent_mail)`.
   **17 force tests passed**; the remaining catalogue case failed because native
   definitions expose the compact role guide, not the full reference wording.
   Both original concurrency failures passed after short transaction sections
   serialized with the routine FIFO flush. Log: `green-rust-force.log`;
   rb receipt: `20261106T115416Z-6589` (exit 100, classified catalogue failure).
2. Only that catalogue case was rerun after its assertion was aligned with the
   compact guide and the guide named both urgent receipt fields:
   `cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(agent_send_schema_and_description_explain_forced_urgent_mail)'`.
   **1 passed**, exit **0**, rb receipt `20261106T115734Z-40577`.
   Log: `green-catalogue.log`.
3. The **13 unaffected mailbox/lifecycle cases** from the first completed run
   remain valid. They cover inbox cursor/eviction/ownership and orchestrator
   idle wakes; the later lock change is confined to forced delivery. Together
   the retained evidence covers **31 distinct Rust cases**, not a full suite.

All logs are under `~/Gits/.tmp/steer-force-evidence/`; each rb receipt is under
`~/Gits/.tmp/rb/`. The final source/fixture hashes are in
`source-sha256.json` beside this document. Later changes were documentation only.

Final local static validation passed: `fmt-changed.sh "$PWD" main` (14 files,
no fixes), `pnpm exec tsc --noEmit`, and `git diff --check`. Log:
`final-static.log` (exit 0). No unchanged component test was rerun. No visual
geometry or CSS changed; no desktop screenshot or live TUIC rebuild is claimed.

The main-store criterion update was denied by the session's read-only root:
`edit may not write .../tuicommander/stories/2869-3e82-pending-P1-steer-must-force-the-running-turn-never-act-as-a-s.md: it is inside a read-only root`.
The coordinator must apply the proven criteria/worklog from this evidence before
story closure. No alternate transport was used to bypass that denial.
