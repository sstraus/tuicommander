---
id: 418-c4a8
title: Output parser fails to detect Ink-style interactive prompts as questions
status: complete
priority: P1
created: "2026-02-27T07:03:01.223Z"
updated: "2026-02-27T10:52:06.032Z"
dependencies: []
---

# Output parser fails to detect Ink-style interactive prompts as questions

## Problem Statement

The output parser MENU_RE expects ❯ (U+276F) or ) before numbered options, but Ink SelectInput uses › (U+203A). Also QUESTION_RE only matches 3 hardcoded phrases (Would you like to proceed / Do you want to / Is this plan okay) — arbitrary questions like What should we do with this story? are missed. This means Claude Code sessions running Ink-based CLIs (wiz:triage, npm init, etc.) never trigger ParsedEvent::Question, so the UI cannot surface the interactive prompt to the user.

## Acceptance Criteria

- [ ] Broaden MENU_RE to also match › (U+203A) and > as cursor indicators before numbered options
- [ ] Add pattern for Ink navigation footer: Enter to select · ↑/↓ to navigate · Esc to cancel
- [ ] Consider broadening QUESTION_RE to match any line ending with ? that passes the existing rejection filters, or add a generic numbered-list-with-footer heuristic
- [ ] Add test cases for Ink SelectInput output format
- [ ] Verify no false positives with existing test corpus

## Files

- src-tauri/src/output_parser.rs

## QA

None — covered by tests

## Work Log

### 2026-02-27T10:51:27.417Z - Completed: Broadened MENU_RE to match › and >, added INK_FOOTER_RE for 'Enter to select' footer, added generic question fallback using line_is_likely_not_a_prompt filter, added 11 test cases. All 720 Rust tests pass.

