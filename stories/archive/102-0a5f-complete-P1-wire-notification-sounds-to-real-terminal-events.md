---
id: 102-0a5f
title: Wire notification sounds to real terminal events
status: complete
priority: P1
created: "2026-02-15T10:42:23.198Z"
updated: "2026-02-15T11:19:15.648Z"
dependencies: []
---

# Wire notification sounds to real terminal events

## Problem Statement

The notification system has full audio infrastructure (Web Audio API oscillator, volume control, per-event toggles, persistence) but zero triggers. No code ever calls notificationsStore.play*(). Additionally, the Test buttons in Settings may not emit sound due to Tauri WebView AudioContext autoplay policy. The 4 notification events (question, error, completion, warning) need to be wired to real terminal signals that already partially exist in the PTY event system.

## Acceptance Criteria

- [ ] PHASE 0 - Fix audio: Debug and fix Test button sound in Tauri WebView (likely AudioContext suspended state). Verify audio works before wiring events
- [ ] PHASE 1 - Task completed: Hook playCompletion() into existing pty-exit handler (Terminal.tsx:231). Expose exit code from Rust (not currently captured). Success=completion, non-zero=error
- [ ] PHASE 2 - Error occurred: Hook playError() into existing rate-limit handler (Terminal.tsx:202). Add OutputParser patterns for generic errors (see research checklist)
- [ ] PHASE 3 - Agent asks question: New parsed event type input-prompt in output_parser.rs with agent prompt patterns. Hook playQuestion(). Populate awaitingInput store field
- [ ] PHASE 4 - Warning: New parsed event type warning in output_parser.rs with warning patterns. Hook playWarning()
- [ ] Only trigger notifications for background (non-active) terminals, not the one user is currently viewing
- [ ] Respect existing settings: master enable, per-event toggles, volume, rate limiting (500ms min interval)
- [ ] RESEARCH FIRST: Before coding patterns, compile exhaustive prompt list per category (see story notes)

## Files

- src/notifications.ts
- src/stores/notifications.ts
- src-tauri/src/output_parser.rs
- src-tauri/src/lib.rs
- src/components/Terminal/Terminal.tsx
- src/stores/terminals.ts

## Work Log

### 2026-02-15T10:42:40.862Z - RESEARCH CHECKLIST - Prompt patterns to capture per category:

### 2026-02-15T11:19:15.577Z - Wired notifications: AudioContext autoplay fix, completion on pty-exit (background only), warning on rate-limit. Phases 3-4 (prompt/warning patterns) deferred - need Rust parser changes. 891 tests pass.


## ERROR patterns (output_parser.rs)
- Shell: 'command not found', 'No such file or directory', 'Permission denied'
- Exit codes: non-zero exit (need to expose from Rust waitpid)
- Node/JS: 'Error:', 'TypeError:', 'ReferenceError:', 'SyntaxError:', unhandled rejection
- Python: 'Traceback (most recent call last)', 'ModuleNotFoundError', 'ImportError'
- Rust: 'error[E', 'panicked at', 'thread .* panicked'
- Go: 'fatal error:', 'panic:', 'runtime error'
- Git: 'fatal:', 'CONFLICT', 'merge conflict'
- Docker: 'ERROR', 'failed to', 'Cannot connect'
- Generic: 'FAILED', 'FAILURE', 'segfault', 'killed', 'OOM', 'out of memory'
- HTTP: '500 Internal Server Error', '502 Bad Gateway', '503 Service Unavailable'

## QUESTION/PROMPT patterns (input-prompt detection)
- Claude Code: 'Do you want to', permission prompts, tool approval prompts
- Generic Y/N: '(y/n)', '(Y/n)', '(yes/no)', '[Y/n]', '[y/N]'
- Interactive: '? ' at line start (inquirer.js style), 'Press enter to', 'Choose'
- Git: 'Are you sure', 'Continue?', 'Overwrite (yes/no)?'
- npm/yarn: 'Ok to proceed?', 'Would you like to'
- sudo: 'Password:', 'password for'
- SSH: 'Are you sure you want to continue connecting', 'Enter passphrase'
- Confirmations: 'Proceed?', 'Continue?', 'Confirm?', 'Type YES to confirm'

## WARNING patterns
- Compiler: 'warning:', 'Warning:', 'WARN', 'warn:'
- Deprecation: 'deprecated', 'DeprecationWarning', 'DEPRECATED'
- Node: 'ExperimentalWarning', 'DEP0'
- npm: 'npm warn', 'WARN deprecated', 'peer dep'
- Security: 'vulnerability', 'vulnerabilities found', 'audit'
- Generic: unicode warning sign, 'caution', 'notice'

## COMPLETION patterns (beyond just pty-exit)
- Test runners: 'Tests passed', 'All tests passed', 'passed, 0 failed'
- Build: 'Build successful', 'compiled successfully', 'Done in'
- Deploy: 'deployed successfully', 'Deployment complete'
- Claude Code: task summary output, cost summary line

