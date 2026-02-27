---
id: 421-e6ab
title: LLM-declared intent via parseable output token
status: complete
priority: P2
created: "2026-02-27T11:24:45.635Z"
updated: "2026-02-27T11:42:52.619Z"
dependencies: []
---

# LLM-declared intent via parseable output token

## Problem Statement

The activity board currently infers agent intent from user prompts (user-input ParsedEvent). This is indirect and often imprecise. A second, more reliable signal is needed: the LLM itself declaring what it is about to do. The LLM should emit a structured intent token in its output that the output parser can capture as a new ParsedEvent type, so the activity board always shows accurate real-time intent even when the user prompt is vague.

## Acceptance Criteria

- [ ] Design a recognizable intent token format that LLMs can emit (e.g. a line like `⟦intent: Refactoring auth module⟧` or `<!-- intent: Writing tests -->`) — must be easy for an LLM to produce and unambiguous for the parser
- [ ] Add intent token to agent MCP instructions / system prompt so agents are instructed to emit it at the start of each action
- [ ] Add ParsedEvent::Intent { text: String } variant to output_parser.rs
- [ ] Add regex detection for the chosen token format in parse_intent() function
- [ ] Wire emit of pty-parsed-{session_id} Intent event from pty.rs like other events
- [ ] Update activity board / useAgentDetection to consume Intent events and display as current intent
- [ ] Add TDD tests: positive cases (various phrasings LLM might use), negative cases (no false positives from normal prose)
- [ ] Update docs/backend/output-parser.md with Intent event type and token format

## Files

- src-tauri/src/output_parser.rs
- src-tauri/src/pty.rs
- src-tauri/src/mcp_http/plugin_docs.rs
- src/hooks/useAgentDetection.ts
- docs/backend/output-parser.md

## QA

None — covered by tests

## Work Log

### 2026-02-27T11:40:21.537Z - Implemented full intent pipeline: ParsedEvent::Intent in Rust parser with [[intent: text]] and ⟦intent: text⟧ regex, wired through pty-parsed events to Terminal.tsx, stored as agentIntent on terminal state, displayed in ActivityDashboard with crosshair icon. MCP init instructions auto-inject the prompt. Docs updated with manual CLAUDE.md snippet.

### 2026-02-27T11:42:48.893Z - Completed: All acceptance criteria verified — ParsedEvent::Intent in output_parser.rs with regex + 7 tests, pty.rs emits via generic loop, Terminal.tsx handles intent case, terminalsStore has agentIntent field + setAgentIntent(), ActivityDashboard displays intent with crosshair icon (priority over lastPrompt), MCP transport injects [[intent:]] instruction, docs/backend/output-parser.md updated. Fixed broken Sidebar and PrDetailPopover test mocks (missing isBusy, getRemoteOnlyPrs, loadCheckDetails).

