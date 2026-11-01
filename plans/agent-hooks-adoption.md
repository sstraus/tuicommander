---
status: ready
created: 2026-11-01
---
# Plan: Agent hook events

- **Goal:** Improve agent state and show exact questions, tools and child work.
- **Authority:** Boss, 2026-11-01. Include ego terminal and AI Chat paths. Use the coordinator research report. This commit contains a plan and stories only.
- **Branch:** `docs/agent-hooks-adoption-plan`. Do not land or push this branch.
- **Done:** Each step has a story, a state contract, a clear path, files, tests, risk and dependencies. Implementation remains open.

## Architecture context

- Unix hook command → controlling tty → OSC 7770 → `pty.rs` → ranked turn evidence and `SessionState` → desktop and HTTP/SSE views.
- `agent_hook.rs` makes event maps. `agent_hook_launch.rs` makes launch assets. `agent_hook_commands.rs` dispatches installers. Shared JSON uses a sentinel; OpenCode owns a marked TS file.
- `state=prompt` starts work and marks a submitted prompt. Tool events use `busy`, never `prompt` or per-tool `idle`.
- `awaiting_input` is sticky. A source must clear its own wait on answer, refusal, cancel, stop and confirmed agent exit. A tool in another request must not clear an open modal.
- Hook evidence has Protocol rank. Keep screen/process recovery, turn epochs and the five-minute lost-completion backstop. Test the event-bus accumulator, not only parsed events.
- Ego terminal already emits native OSC. Its `Announce` reads public run events; the terminal owner emits waits. Do not install a second state hook for ego.
- AI Chat uses one app-wide `ego acp` process and a separate ACP session per conversation. `acp/connection.rs` owns requests and settlement; `useAcpChat.ts` renders it. An ACP peer has no terminal or PTY.
- Use common state meanings across transports, with separate owners and identities. Never copy an ACP conversation into a terminal `SessionState` or let one chat set another chat's state.

## Evidence and limits

- Read `AGENTS.md`, `docs/sync-matrix.md`, `docs/user-guide/ai-agents.md`, hook producers/installers and PTY callers. TUIC source has no Grok cancel/error entries and no explicit OpenCode reply entries.
- Grok vendor guide lists `StopCancelled`, `StopFailure`, `permission_prompt` and child events. A cancel replaces `Stop`. Runtime badge failure is not yet captured.
- Installed `codex --version`: **0.166.0**. Existing identity fixtures cover 0.162.1. Capture the current hook/RPC shapes before implementation.
- [Official OpenAI documentation](https://developers.openai.com/plugins/build/plugins) says enabling a plugin does not trust its hooks. This does not prove a particular App Server RPC on 0.166.0; Step 2 must record it.
- Ego `crates/ego-core/src/event/hook.rs` has 21 lifecycle variants: session start/end, instructions, prompt/expansion, pre/post/failure tool, permission request/denied, batch, model switch, child start/stop, stop/failure, compact and elicitation/result. No separate `StopCancelled` variant is declared there.
- Ego `render/osc.rs` has Idle/Busy/Awaiting. `run/interactive_driver.rs::Announce::signal` emits turn state, recovery and final idle, but no rich tool/child payload. Permission requests alone are deliberately silent before policy has decided.
- Ego `ego-acp/src/project.rs` projects tool start/end and cancellation. It omits `ItemKind::Subagent` from v1. Questions use `question.rs` elicitation; permissions use `permission.rs`. Preserve these existing paths.
- No Windows run, native CLI event capture or new runtime test was performed for this plan. Do not mark proposed regressions as measured failures.

## Common implementation contract

- Record real CLI stdin JSON, transcript rows and `.tcap` streams with version, command, geometry and input/output order. Put detection captures in `src-tauri/src/fixtures/agent_prompts/`; replay through the production parser and state accumulator. Never hand-write a vendor fixture.
- Each fix starts with a named failing regression on the pre-fix SHA. Record `RED`, `REALITY` and `SYMPTOM`; distinguish observed code omissions from unmeasured runtime impact. Features also name the plausible bug their tests prevent.
- Run targeted Rust checks on rb through `build-slot.sh --remote` and the test temp wrapper. Include warnings-denied Clippy with the RED call when adding Rust tests. Run local JS/TS tests through `scripts/with-test-tmp.sh`; run other JS/TS checks locally. The integrator owns final GREEN and review. Do not run broad suites per story.
- For **every step**, the critic checks ego terminal and ego ACP effects: state, human wait, question, tool, child and interrupted outcome. If the step does not touch that path, cite the unchanged boundary; do not run an unrelated suite. Producer/consumer steps replay matching recorded frames on both sides.
- Preserve user hooks, explicit launch flags, selected CLI profile/home and agent-test restrictions. Installation does not mean a live hook was seen. Missing/untrusted hooks retain heuristics and honest feature availability.
- Rich metadata enriches a live state; it cannot create a human wait by itself. Key by source session, turn and request/item. Answered request ids cannot reappear from old transcript rows or repaint.
- Candidate rich frame: `OSC 7770;agent-event=<base64 JSON> ST`. Fields: version, agent, native session, turn, event, item/request id, bounded data. Define final schemas from real captures before coding; unknown fields/events must not corrupt state. Retain existing state frames as the authoritative coarse state.
- Cap decoded prompt metadata at 16 KiB and labels at existing backend limits. An oversized or invalid frame drops enrichment, preserves valid state and leaves screen fallback active. Test split chunks, Unicode, BEL/ST, duplicates and two simultaneous sessions.

## Steps

### Step 1: Grok completion and human permission state

- **Contract:** `StopCancelled`/`StopFailure` → idle; `Notification(permission_prompt)` → awaiting. Ignore `idle_prompt`/`task_complete` as waits. Clear on paired post-tool success/failure, permission denial, prompt, cancel/stop and exit; clear only the matching active request.
- **Pattern:** `StopCancelled => idle; permission_prompt => awaiting; settled => busy|idle`.
- **Files:** `src-tauri/src/agent_hook.rs`, `agent_hook_commands.rs`, `agent_hook_installer.rs`; `pty/core_tests.rs`, `pty/tests.rs`, real Grok fixtures.
- **Tests:** `grok_cancelled_turn_releases_protocol_busy`, `grok_permission_wait_retracts_on_deny`; generic notification stays idle, API error, bare Enter, duplicate imported Claude hooks and sibling user-hook preservation. Start with a real short interrupted Grok session; record the result without assuming it is broken.
- **Risk:** medium; sticky state and imported hooks. No dependency. Shares maps/install tests with 3, 4, 7, 11–13.

### Step 2: Codex hook trust on the installed CLI

- **Contract:** Discover current hook identities/hashes with Codex itself. An explicit hook install/update can trust only exact TUIC-owned definitions through the supported CLI API. If unavailable, report review required and guide the normal `/hooks` review. Never report operational hooks from file presence alone.
- **Pattern:** `list_current_hooks -> match_owned_definition -> trust_current_hash -> read_back`.
- **Files:** `src-tauri/src/agent_hook_codex.rs`, `agent_hook_commands.rs`, `agent_hook_installer.rs`; new `agent_hook_codex_trust.rs` and tests; `lib.rs`, Settings Agents status and `config_routes.rs` if status changes.
- **Tests:** Record real 0.166.0 `hooks/list` and `config/batchWrite` traffic in a scratch home, or record their exact unsupported response. Test changed definitions, stale hash, failed write/readback, concurrent install, profile selection, missing CLI and preservation of unrelated hook trust/config.
- **Clear path:** no new awaiting source. Trust review is a setup state, not a terminal permission wait.
- **Risk:** high; saved user config and protocol version. Do not copy a hash algorithm, grant trust to all hooks or add a trust-bypass flag. Must precede 3. Shares Codex config files with 3 and 7.

### Step 3: Codex permission and tool activity

- **Contract:** `PreToolUse`/`PostToolUse` → busy; human-owned `PermissionRequest` → awaiting. Auto-review-owned requests stay busy. A native user-input tool needs the structured question path, not a generic busy override.
- **Pattern:** `permission(owner=human) => awaiting; permission(owner=auto_review) => busy`.
- **Clear:** matching post-tool/failure or denial, next accepted prompt, stop, cancellation and confirmed exit. Capture denial with no post-tool; use the actual CLI completion event or request-scoped resolution. Do not depend on screen disappearance to clear a confident wait.
- **Files:** `agent_hook.rs`, `agent_hook_codex*.rs`, `agent_hook_commands.rs`, `codex_binding.rs`, `pty.rs`, `state.rs`, PTY tests and Codex fixtures.
- **Tests:** `codex_human_approval_sets_and_clears_wait`, `codex_auto_review_never_requests_human_attention`; real stdin/rollout + `.tcap` for allow/deny/Escape, error, long tool loop, delayed idle, two sessions and exact identity retention.
- **Risk:** medium; false human attention and hook latency. Depends on 2. Shares state files with 4 and 10–13.

### Step 4: Claude permission and attention events

- **Contract:** `PermissionRequest` and a verified human-action `Notification` → awaiting. Filter by actual notification type/payload; ordinary idle notifications remain idle. Preserve existing AskUserQuestion/ExitPlanMode and Elicitation/result behavior.
- **Pattern:** `permission_prompt => awaiting; ordinary_idle_notice => unchanged`.
- **Clear:** matching broad PostToolUse/PostToolUseFailure or permission resolution, refusal/cancel, next prompt, stop/failure/session end and confirmed exit. Capture an answered permission with no post-tool. A still-open wizard remains awaiting across tool noise.
- **Files:** `agent_hook.rs`, `agent_hook_launch.rs`, PTY/state tests, recorded Claude fixtures. Update map installation expectations too.
- **Tests:** `claude_permission_answer_does_not_latch_wait`; replay accept/deny/Escape/bare Enter and multi-question dialog; retain `completed_claude_prose_notification_does_not_flash_awaiting` from story 1119-57fa.
- **Risk:** medium; false idle-prompt attention is an existing regression class. Shares map and state files with 1, 3, 7 and 10–13; serialize.

### Step 5: OpenCode question state and retraction

- **Contract:** `question.asked`/`permission.asked` → awaiting; matching replied/rejected → busy if the turn runs, otherwise idle. `session.idle`/exit also settles. Track request/session ownership so another reply cannot clear a newer question.
- **Pattern:** `asked(id) => wait(id); replied(id)|rejected(id) => settle(id)`.
- **Files:** `src-tauri/src/agent_hook_opencode.rs`, generated-plugin tests, PTY captures. Reuse marked-file install/outdated checks.
- **Tests:** `opencode_answer_retracts_wait_without_next_tool`; record actual 2.2.3 callback/event shapes; test rejection, concurrent prompts, duplicate reply, no next tool, outside-TUIC no-op and user-plugin preservation.
- **Risk:** medium; API shape and pending modal ownership. Independent source file; shared docs/state replay files require serial integration.

### Step 6: Pi managed extension

- **Contract:** agent start/tool call/start/end → busy; agent end → idle; UI prompt start → awaiting; UI prompt end restores current run state. Model selection alone cannot start work. Session switch/extension close drops old waits. A background tool cannot clear an open modal.
- **Pattern:** `ui_prompt_start(id) => wait(id); finally(ui_prompt_end(id)) => current_activity`.
- **Files:** new `src-tauri/src/agent_hook_pi.rs`; `agent_hook_commands.rs`, `agents.ts`, `pty.rs`, `lib.rs`, Settings Agents tests. Own one marked extension under Pi's documented extension directory; never overwrite an unmarked file.
- **Tests:** `pi_modal_survives_background_tool_and_clears_on_close`; record Pi 0.84.2 actual extension API/events, all supported modal types, session change, idle utility modal, cancel/exception and install/update/remove. Do not assume UI prompt names are native events: use supported extension callbacks and a paired finally path.
- **Risk:** medium; extension API drift. Shares registry/installer/Settings with 7. Depends on captured Pi API evidence before production edits.

### Step 7: Honest Windows hook availability now

- **Contract:** Native Windows launch/boot/global install must not write or inject unusable POSIX hooks. Return honest unsupported state; hide unsupported install/status controls for the owning Windows machine. Preserve saved preference and heuristic detection. Do not delete existing user hook config.
- **Pattern:** `native_windows && !supported_transport => no_hook_injection`.
- **Files:** `agent_hook_launch.rs`, `agent_hook_commands.rs`, `shell_integration.rs`, `pty.rs`, `agents.ts`, `SettingsPanel/tabs/AgentsTab.tsx`, launch/config route tests.
- **Tests:** `windows_launch_does_not_inject_posix_hook`; code-level platform matrix, explicit override, remote machine capability and no writes. **[MANUAL]** Check Windows controls and heuristic badge on real Windows; take a screenshot for visual changes. Correct the current hidden-toggle claim in the guide.
- **Ego:** Keep native ego OSC capability separate from the disabled shell installer; ACP remains available without PTY hooks.
- **Risk:** medium; launch regressions. No dependency. Shares central files with 1–6 and 9.

### Step 8: Windows ConPTY OSC spike

- **Contract:** Prove whether an actual hook subprocess can write unknown OSC 7770 through `CONOUT$` into its parent's ConPTY. No production support claim from a parser-only test.
- **Pattern:** `real_hook -> CONOUT$ -> ConPTY -> captured_OSC -> SessionState`.
- **Files:** new `scripts/agent-capture/windows-hook-osc.*`, new `reports/agent-hooks/windows-conpty.md`, recorded fixtures. No shared production file.
- **Tests:** code-level replay of recorded raw frames with splits and state clear; **[MANUAL]** Windows 10/11, cmd/PowerShell/Git Bash, installed agent hook launch and native ego control, console inheritance, BEL/ST, error/exit and elapsed cost. Record unavailable cells honestly. ACP control verifies no dependence on console writes.
- **Risk:** medium; hardware and console inheritance. No dependency. If the matrix passes, Step 9 ships OSC; if it fails, Step 9 ships the local pipe. Preserve Step 7 honesty until one is proven.

### Step 9: Ship the proven Windows transport

- **Contract:** Prefer proven in-band OSC. Otherwise `tuic agent-state` carries a bounded native event over the existing named pipe, including Codex identity stdin. Bind the caller to its live session and selected TUIC instance; route through the same state reducer as OSC. Carry awaiting through the question/awaiting event path; `handle_tuic_state` alone accepts no awaiting value.
- **Pattern:** `tuic --instance <owner> agent-state <event> < stdin_json`.
- **Files:** `src-tauri/crates/tuic-cli/src/main.rs`, new `agent_state.rs`; `tuic-ipc/src/lib.rs`, new `mcp_http/agent_hook_routes.rs`, `mcp_http/mod.rs`, `pty.rs`, `pty/lifecycle.rs`, hook producers and launch assets. OSC success may remove the need for the new route/CLI files; record that decision.
- **Clear:** carry the same request settlement, turn end and exit as Unix. Reject stale generation/turn or out-of-order sequence; ensure a late event cannot revive a closed session. Never infer cross-process order from wall clock alone.
- **Tests:** `late_pipe_hook_cannot_revive_settled_turn`, instance/session isolation, unknown/closed session, duplicate sequence, named-pipe disconnect, Unicode/spaces in exe path, Codex identity and ego rich frame parity. **[MANUAL]** End-to-end Windows native CLI check and latency. Keep ACP independent.
- **Risk:** high; out-of-band ordering. Depends on 7, 8 and rich contracts 11/14 for metadata. IPC/HTTP shapes and any Tauri mapping stay identical. No new loopback server, token or relay.

### Step 10: Recover unresolved questions from exact transcripts

- **Contract:** Enrich a confirmed live human wait from an unresolved tool call in the exact Claude/Codex transcript. Priority: current structured event, matching unresolved transcript, screen fallback. Transcript history alone cannot set awaiting. Ego uses its native question owner, not Claude/Codex transcript heuristics.
- **Pattern:** `live_wait(request) + unresolved_exact_tool(request) => question_card`.
- **Clear:** answer/cancel/result, turn/session change, identity revocation or exit retires that request; replay cannot restore it.
- **Files:** `src-tauri/src/chat_view/{claude,codex,mod}.rs`, new `agent_question.rs`, `codex_binding.rs`, `state.rs`, PTY integration tests and transcript fixtures.
- **Tests:** `answered_transcript_question_never_reopens_card`; real incomplete/completed tool rows, wrong session, torn JSONL, reload, multiple questions and no rich hook. Existing Codex completed-only chat projection must not gain fake completed rows.
- **Risk:** medium; exact ownership and stale cards. Depends on 3, 4. Shares question/state files with 11.

### Step 11: Structured questions and plan approval

- **Contract:** Pass recorded AskUserQuestion/ExitPlanMode payloads from Claude, verified Codex user-input events and ego native question owner into backend-normalized cards. Preserve multi-step, multiple-choice, free text, plan approval and native answer semantics. Payload is enrichment, not approval authority.
- **Pattern:** `active_wait + matching_prompt_payload => normalized_card`.
- **Clear:** matching result/answer/Escape/refusal/stop/exit removes metadata and wait as appropriate; late payloads cannot re-open answered requests. No arbitrary tool name may claim a human question.
- **Files:** `agent_hook.rs`, new `agent_hook_payload.rs`, `pty.rs`, `state.rs`, `tuic-terminal/src/output_parser.rs`, `patches/alacritty_terminal/src/term/mod.rs` if needed; shared desktop/mobile `src/mobile/components/ChoicePromptOverlay.tsx`, transport/event types.
- **Tests:** `late_structured_question_cannot_rearm_answered_wait`, capped/malformed/split payloads, same question text with different ids, multi-question repaint and native answer bytes. Record real producer JSON and TUICCAP2; desktop/mobile contract and screenshot after visual changes.
- **Risk:** high; protocol and input semantics. Depends on 10 and 14. Share the schema with 12/13. Keep answer writes on `sendCommand` or the existing request owner, never hook decisions.

### Step 12: Native child roster for Codex, Grok and ego

- **Contract:** Child start/stop updates a bounded roster keyed by agent/session/turn/child. Show child rows; live children keep a completed lead in background work. A human wait takes precedence. Internal children never become managed TUIC peers or PTYs.
- **Pattern:** `effective = human_wait ? awaiting : lead_busy || live_children ? busy : idle`.
- **Clear:** duplicate end is idempotent; reconcile omitted stop from exact transcript/run settlement, confirmed process exit or session revocation. A new turn cannot inherit an old child's busy latch; do not mark a child complete from arbitrary silence.
- **Files:** new `src-tauri/src/agent_children.rs`; `agent_hook.rs`, `pty.rs`, `state.rs`, `subagent_map.rs`, `progress/flow.rs`, session snapshots/events and existing Flow UI/tests.
- **Tests:** `missing_child_stop_does_not_latch_parent_forever`; real Codex/Grok child hooks and ego 14 frames, stop-before-start, duplicate, parent stop with live child, child human wait, restart and two parents. Preserve Claude transcript lane behavior. ACP child behavior is Step 16.
- **Risk:** high; shared state and missing events. Depends on 3, 11, 14. Shares Flow/state files with 13/16.

### Step 13: Tool-name readout

- **Contract:** Show the current tool name from verified native metadata for Claude, Codex, Grok, Pi, OpenCode and ego. Add Gemini AfterTool only when needed to retire its readout. Metadata cannot change coarse state. Tool end clears its own item, not a concurrent tool or open modal.
- **Pattern:** `tool_start(item,name) => readout(item); tool_end(item) => remove(item)`.
- **Files:** hook/plugin producers, `agent_hook_payload.rs`, `pty.rs`, `state.rs`, session transport types, `components/StatusBar/StatusBar.tsx`, mobile session state, component tests.
- **Clear:** matching tool completion/failure, turn/session end, cancel and exit. Bound/redact labels in Rust; omit raw arguments by default.
- **Tests:** `late_tool_end_does_not_clear_new_tool`; recorded provider input, parallel tools, no end, two sessions, unknown event, cap and desktop/mobile parity; screenshot the actual readout. Use ACP tool ids for AI Chat in 16.
- **Risk:** medium; stale labels and per-tool cost. Depends on 6, 11, 14. No frontend data reshaping.

### Step 14: Ego native terminal event enrichment (ego repository)

- **Contract:** Extend the existing native emitter with the rich frame in the common contract: actual modal open/settlement and question data, tool start/end, child start/settlement and interrupted outcome. Use public events and the terminal modal owner; do not execute external shell hooks to mirror native state.
- **Pattern:** `PublicEvent|modal_owner -> bounded_native_event + existing_state`.
- **Clear:** modal answer/refusal/Escape/EOF/error restores **current** run activity, not activity saved on open. Turn cancel/failure/exit clears tools/children/questions exactly once. A judge-decided permission stays busy and silent. Normal terminals and JSON/JSONL output receive no TUIC-only frame.
- **Files in ego:** `crates/ego-cli/src/render/osc.rs`, `run/interactive_driver.rs`, `run/tests.rs`, `repl/seat/question.rs`, `crates/ego-core/src/public/mod.rs` only if an existing public event lacks required identity, focused tests and `docs/internals/terminal-agent-events.md` (new).
- **Tests:** `native_tool_and_child_events_reach_host_without_text`, `cancelled_modal_retracts_native_wait`; real recorded ego CLI stream and source-owner assertions for start/stop/error/permission/question/tool/child/cancel. Preserve `an_auto_decidable_permission_request_announces_nothing` and recovery tests. Check every existing hook lifecycle; add a hook lifecycle only if evidence shows a missing external contract, not for a second native emitter.
- **Risk:** high; cross-repo protocol and terminal ownership. No implementation dependency; schema is agreed here. Supplies fixtures to 11–13 and 9. May run beside TUIC steps.

### Step 15: Ego ACP child activity (ego repository)

- **Contract:** Keep existing ACP prompt start/end, tool calls, permission RPC and question elicitation. Carry native child start/settlement over a documented negotiated ego ACP extension because v1 has no child variant. Include item/parent/turn identity and final outcome. Standard clients retain current behavior when the extension is absent.
- **Pattern:** `public_child_item -> negotiated_ego_activity; TurnCompleted -> existing_stop_reason`.
- **Clear:** child settlement/cancel/parent session close clears the roster; pending human requests settle through their existing permission/elicitation owner. Cancellation remains distinct from success. Reattach reconstructs current state, without replaying a human prompt as new.
- **Files in ego:** `crates/ego-acp/src/{project,serve,negotiate}.rs`, `crates/ego-core/src/public/mod.rs` only if required, ACP schema/extension declaration selected from the existing extension mechanism, tests `ego-acp/tests/{prompt,session_lifecycle}.rs`, new `docs/internals/acp-agent-activity.md`.
- **Tests:** `acp_child_activity_is_scoped_to_its_parent_session`; real `ego acp` transcript with two sessions, child omission, cancellation, rejected permission, answered question, reconnect, and extension absent. Record actual negotiated wire shapes; never invent a standard ACP variant or use terminal OSC on stdio.
- **Risk:** high; protocol and public-event identity. Independent of 14 except any `public/mod.rs` change: serialize those edits if both need it. Supplies Step 16.

### Step 16: TUIC AI Chat state and activity from ACP

- **Contract:** Derive AI Chat busy/awaiting/question/tool/child/interrupted meanings in Rust from its ACP attachment, active turn, pending requests and negotiated child updates. Publish to desktop and HTTP/WS clients. Keep one app-wide ego process and per-chat ownership; do not create a terminal record.
- **Pattern:** `attachment + active_turn + pending_interactions + children => chat_activity`.
- **Clear:** `PermissionSettled`/`ElicitationSettled` removes only its request. TurnSettled/TurnFailed/cancel, disconnect and session close settle matching state. Snapshot+stream reconnect cannot restore a closed interaction. Background child work does not permit a second prompt into an occupied turn.
- **Files:** `src-tauri/src/acp/{mod,connection,events}.rs`, ACP snapshots/routes; `src/types/acp.ts`, `stores/{acp,acpTranscript}.ts`, `components/AIChatPanel/useAcpChat.ts`, shared chat/remote/mobile consumers and focused tests. Reuse existing tool/elicitation cards.
- **Tests:** `acp_settled_question_does_not_survive_reconnect`, `acp_child_in_chat_a_does_not_mark_chat_b_busy`; recorded 15 wire frames plus existing permission/question tests, multi-client answer, app-wide process reuse, interrupted outcome, old capability absence and remote parity. Review PTY ego regression fixtures separately; equal meanings do not mean equal session ids.
- **Risk:** high; shared app state and protocol. Depends on 15 and shared child semantics from 12. Shares state/Flow consumers with 12/13; serialize.

## Documentation and validation ownership

- Each TUIC implementation story updates affected rows in `docs/user-guide/ai-agents.md`, `docs/backend/{pty,output-parser}.md`, `docs/FEATURES.md`, `SPEC.md`, `CHANGELOG.md` and `to-test.md`. Config/launch changes also update config/settings docs. New routes update API/MCP docs and parity tests; new pushes dual-emit and bridge SSE/WS. Regenerate route snapshots with Vitest, never by hand.
- Rich UI stories read `docs/frontend/STYLE_GUIDE.md`, use shared controls, and take screenshots. Do not change sidebar click or drag behavior.
- Ego stories update the producer contract docs named in 14/15 and relevant `CLI.md`/`ARCHITECTURE.md` sections. The planner creates stories only in ego; no ego source/doc edits in this task.
- A Rust change needs a post-rebuild `to-test.md` item. Do not restart Boss's app. The integrator checks native Windows hardware, real CLI behavior, per-event latency and coexistence cost before release.

## Story index

All stories use `plan: plans/agent-hooks-adoption.md` and their numbered step. Ego resolves this identifier in the TUIC repository. The plan remains in this branch until landing. Both stores are ignored by Git. Cross-repository prerequisites are criteria, because each store resolves local ids only.

| Step | Store | Story | Local prerequisites |
|---|---|---|---|
| 1 | TUIC | `2624-848f` | None |
| 2 | TUIC | `2625-c4b5` | None |
| 3 | TUIC | `2626-f570` | `2625-c4b5` |
| 4 | TUIC | `2627-110e` | None |
| 5 | TUIC | `2628-ad9b` | None |
| 6 | TUIC | `2629-9537` | None |
| 7 | TUIC | `2630-568a` | None |
| 8 | TUIC | `2631-1f9f` | None |
| 9 | TUIC | `2632-97ef` | `2630-568a`, `2631-1f9f`, `2634-ea5b` |
| 10 | TUIC | `2633-cbd3` | `2626-f570`, `2627-110e` |
| 11 | TUIC | `2634-ea5b` | `2633-cbd3` |
| 12 | TUIC | `2635-8052` | `2626-f570`, `2634-ea5b` |
| 13 | TUIC | `2636-5551` | `2629-9537`, `2634-ea5b` |
| 14 | ego | `771-b309` | None |
| 15 | ego | `772-d55a` | None |
| 16 | TUIC | `2637-c6d3` | `2635-8052` |

Cross-repository prerequisites: Steps 9, 11, 12 and 13 require ego `771-b309`; Step 16 requires ego `772-d55a`. Step 15 must retain ACP v1 and leave draft v2 inactive. Completed ego story `643-905e` supplies existing terminal child rows; Step 15 adds host-visible ACP data.

## Execution and file conflicts

- Default TUIC order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 10 → 11 → 12 → 13 → 9 → 16. Steps 8, 14 and 15 can start early. Step 9 waits for 7/8/11/14; Step 16 waits for 12/15.
- Conservative parallel waves: **1 + 8 + 14**, then **2 + 15** if 14 no longer owns `public/mod.rs`; then TUIC steps serially in the order above. Steps 8/14/15 use different repositories or unique evidence files and do not edit TUIC common docs.
- Do not run two TUIC stories in parallel merely because they change different maps: most share `agent_hook.rs`, `agent_hook_commands.rs`, `pty.rs`, `state.rs`, shared fixtures/test modules and the same guide/changelog. Shared docs are real file conflicts too.
- Coordinator may assign tighter file ownership after the first central changes land, but must recompute overlap from the committed files. Never run 14/15 concurrently if both change ego `public/mod.rs`.

## Security and performance

- Named risks: corrupting user hook/config files; exposing raw question/tool secrets; mixing sessions; sending wrong answer bytes. Preserve atomic writes, redaction, ownership and existing input submission paths. Hooks observe permissions; they never approve or deny them.
- Measure installed CLI hooks with the user's existing hook set. Shell plus `ps` per tool and Windows process startup can dominate; record median/tail elapsed cost and command/version. Do not call an unmeasured estimate a result.
- Bound metadata, child count and retained answered ids. No new per-session transcript poll if the existing follower can deliver changes. ACP consumes its existing ordered stream.

## Out of scope

- New agent types, Amp/Cursor/Droid hooks: no installed test CLI/account; Cursor response bodies need separate vendor proof. Boss can promote them later.
- Replacing heuristics, new hook HTTP servers/spool/WSL relays, synthetic titles, mascots, growth prompts, branch naming and usage/keep-awake rewrites: no required gain for this contract. Keep existing consumers on improved state.
- Gemini lifecycle overhaul: existing events already cover its state. Add only verified tool readout metadata in 13.
- Installing external hooks for ego or putting ACP behind PTY fallback: native emitter and ACP request owners already exist.
- Changing global hook config, trusting hooks, launching paid model turns, Windows checks or production implementation during this planning task: implementation stories own those actions and evidence.

## Acceptance

- [ ] Execute each linked story and prove its complete contract, including each new wait's retraction.
- [ ] Review the ego PTY/ACP matrix for every step; record evidence or the unchanged boundary.
- [ ] Pass the targeted final batch, preserve fallback behavior, update docs and complete required Windows checks.
