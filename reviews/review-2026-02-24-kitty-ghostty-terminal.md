# Code Review: Kitty Keyboard Protocol / Ghostty Migration / Terminal UX
**Date:** 2026-02-24
**Reviewers:** Multi-Agent (security, performance, architecture, simplicity, silent-failure, test-quality, typescript, rust)
**Target:** Uncommitted changes on main

## Summary
- **P1 Critical Issues:** 1
- **P2 Important Issues:** 5
- **P3 Nice-to-Have:** 5
- **False Positives:** 3
- **Confidence Threshold:** 70

## Changed Files
- `src-tauri/src/cli.rs` — Added `/usr/bin` to Linux `extra_bin_dirs`
- `src-tauri/src/lib.rs` — Registered `get_kitty_flags` Tauri command
- `src-tauri/src/pty.rs` — Changed TERM_PROGRAM from kitty to ghostty, added TERM_PROGRAM_VERSION, env_remove CLAUDECODE, new `get_kitty_flags` command
- `src/components/Terminal/Terminal.tsx` — Sync initial kitty flags, Shift+Enter handler, Shift+Tab handler
- `src/hooks/useTerminalLifecycle.ts` — Improved tab close focus logic

---

## False Positives

### ~~[LOGIC] Shift+Enter always sends `\x1b\r`, even when kitty protocol is active~~
**File:** `src/components/Terminal/Terminal.tsx:489` | **Agents:** security, performance
**Status:** FALSE POSITIVE

Multiple agents flagged that the Shift+Enter handler fires before the kitty keyboard check and "should" send `\x1b[13;2u` when kitty flags are active. This is wrong.

**Why it's correct as-is:** Claude Code does NOT use kitty CSI u sequences for Shift+Enter. CC expects the terminal emulator to send `\x1b\r` (ESC + CR) natively — this was confirmed by binary analysis of CC v2.1.52 and documented in mdkb memory `kitty-keyboard-shift-enter-fix`. The `/terminal-setup` command in CC explicitly states for ghostty/kitty/WezTerm: "natively supported — no configuration needed", meaning the terminal sends `\x1b\r`, not the kitty-encoded variant. The handler ordering is intentional: Shift+Enter must always produce `\x1b\r` regardless of kitty flags.

---

### ~~[LOGIC] Shift+Tab also bypasses kitty encoding when protocol is active~~
**File:** `src/components/Terminal/Terminal.tsx:497` | **Agents:** security, performance
**Status:** FALSE POSITIVE

Same reasoning as above. The Shift+Tab handler returns `true` to let xterm send the standard CSI Z (`\x1b[Z`) sequence. This is the correct behavior — CSI Z is the universally expected Shift+Tab sequence across all terminal apps, including when kitty protocol is active. The kitty `\x1b[9;2u` encoding is an alternative representation that apps don't rely on for Shift+Tab specifically.

---

### ~~[TYPE] Non-null assertion `terminal!` in Shift+Enter handler~~
**File:** `src/components/Terminal/Terminal.tsx:491` | **Agent:** typescript
**Status:** FALSE POSITIVE

Flagged as P1 because `terminal` is `let terminal: XTerm | undefined`. However, this handler is registered inside `terminal.attachCustomKeyEventHandler()` which is called inside `openTerminal()` — at that point `terminal` is guaranteed non-null (it was just constructed). The `terminal!` assertion is safe and matches the existing pattern used throughout the same closure (lines 506, 562, 592). The suggestion to capture a local `const term = terminal` is a style preference, not a correctness issue, and would be a larger refactor touching pre-existing code outside this diff.

---

## P1 - Critical (Block Commit)

### 1. **[TEST]** `get_kitty_flags` Tauri command has zero tests
**File:** `src-tauri/src/pty.rs:683` | **Confidence: 97** | **Agent:** test-quality

The new `get_kitty_flags` command is the sync mechanism for the race-condition fix (initial kitty flags on listener attach). No test coverage at all. The contract (return 0 for unknown session, return current flags for known session) is unverified.

**Recommended tests:**
```rust
#[test]
fn test_get_kitty_flags_unknown_session_returns_zero() {
    let kitty_states: DashMap<String, Mutex<KittyKeyboardState>> = DashMap::new();
    let result = kitty_states.get("nonexistent")
        .map(|e| e.lock().current_flags()).unwrap_or(0);
    assert_eq!(result, 0);
}

#[test]
fn test_get_kitty_flags_known_session_returns_pushed_flags() {
    let kitty_states: DashMap<String, Mutex<KittyKeyboardState>> = DashMap::new();
    kitty_states.entry("sess-1".into())
        .or_insert_with(|| Mutex::new(KittyKeyboardState::new()));
    kitty_states.get("sess-1").unwrap().lock().push(3);
    let result = kitty_states.get("sess-1")
        .map(|e| e.lock().current_flags()).unwrap_or(0);
    assert_eq!(result, 3);
}
```

---

## P2 - Important (Fix Before/After Commit)

### 2. **[ARCH]** `get_kitty_flags` invoke bypasses `usePty` transport abstraction
**File:** `src/components/Terminal/Terminal.tsx:371` | **Confidence: 92** | **Agent:** architecture

`Terminal.tsx` calls `invoke("get_kitty_flags", ...)` directly. Every other PTY command goes through `usePty` -> `rpc()` -> `transport.ts`. This breaks the transport layer that enables both Tauri and browser/HTTP mode.

**Fix:** Add `getKittyFlags` to `usePty.ts`, add HTTP mapping to `transport.ts::mapCommandToHttp`, replace bare `invoke` in `Terminal.tsx`.

---

### 3. **[SILENT]** Bare `catch {}` swallows errors that cannot actually occur
**File:** `src/components/Terminal/Terminal.tsx:375` | **Confidence: 88** | **Agents:** silent-failure, simplicity

The Rust `get_kitty_flags` returns `u32` (infallible — no `Result`). The comment "session may not exist yet" describes a failure that can never happen (Rust returns `0` for missing sessions, not an error). The catch swallows real IPC errors silently.

**Fix:** Either remove try/catch entirely, or add `console.debug("[Terminal] get_kitty_flags:", err)`.

---

### 4. **[RACE]** Kitty flags sync can overwrite a concurrent event listener update
**File:** `src/components/Terminal/Terminal.tsx:365-377` | **Confidence: 82** | **Agent:** performance

The listener fires with flags F1, then `invoke` resolves with stale F0. Since `if (flags > 0)` doesn't prevent overwrite when F0 > 0, the invoke can stomp a newer value.

**Fix:** Only apply invoked value if listener hasn't updated since registration:
```typescript
const preListenFlags = kittyFlags;
unlistenKitty = await listen<number>(...);
const flags = await invoke<number>("get_kitty_flags", ...);
if (flags > 0 && kittyFlags === preListenFlags) {
  kittyFlags = flags;
}
```

---

### 5. **[DESIGN]** `get_kitty_flags` returns `u32` instead of `Option<u32>`
**File:** `src-tauri/src/pty.rs:683` | **Confidence: 85** | **Agents:** silent-failure, rust

Every other session-scoped command returns `Result<_, String>` with "Session not found". This is the only one that silently degrades to `0`, conflating "no session" with "flags=0". A wrong session ID will never surface as an error.

---

### 6. **[TEST]** `build_shell_command` env var contract not tested after ghostty migration
**File:** `src-tauri/src/pty.rs:31` | **Confidence: 92** | **Agent:** test-quality

`TERM_PROGRAM=ghostty` and `TERM_PROGRAM_VERSION=3.0.0` are critical values (CC gates on them). No test verifies these — a future change could silently break CC integration.

---

## P3 - Nice-to-Have

### 7. **[STYLE]** IIFE for array last-element access obfuscates intent
**File:** `src/hooks/useTerminalLifecycle.ts:125` | **Confidence: 95** | **Agents:** simplicity, architecture, typescript

```typescript
((t) => t.length > 0 ? t[t.length - 1] : null)(activeRepo.branches[...].terminals ?? [])
```
Replace with named variable + `branchTerminals.at(-1) ?? null`.

---

### 8. **[STYLE]** `TERM_PROGRAM_VERSION` should be a named constant
**File:** `src-tauri/src/pty.rs:54` | **Confidence: 80** | **Agents:** architecture, rust

Extract `"3.0.0"` to `const GHOSTTY_VERSION_FOR_CC: &str = "3.0.0"` with comment tying it to CC's version gate.

---

### 9. **[TEST]** `handleTerminalSelect` focus call (`ref.focus()`) never asserted in tests
**File:** `src/hooks/useTerminalLifecycle.ts:297` | **Confidence: 88** | **Agent:** test-quality

The `requestAnimationFrame(() => handleTerminalSelect(nextId))` wrapping means JSDOM tests won't run the callback without explicit flushing.

---

### 10. **[DOC]** `requestAnimationFrame` intent undocumented
**File:** `src/hooks/useTerminalLifecycle.ts:129` | **Confidence: 80** | **Agent:** simplicity

Add comment: "Defer one frame — SolidJS renders the next terminal on remove(), but ref.focus() requires the DOM node to be present."

---

### 11. **[DOC]** `/usr/bin` addition lacks rationale comment
**File:** `src-tauri/src/cli.rs:34` | **Confidence: 72** | **Agent:** rust

Consider: `// /usr/bin included for minimal Wayland sessions that strip PATH`.

---

## Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src/hooks/usePty.ts` | Transport pattern all PTY calls must follow | architecture |
| `src/transport.ts` | Missing `get_kitty_flags` HTTP mapping | architecture |
| `src/components/Terminal/kittyKeyboard.ts` | `kittySequenceForKey` — companion to key handler | typescript |
| `src-tauri/src/state.rs` | `KittyKeyboardState`, confirms infallibility | rust, silent-failure |
| `src/__tests__/hooks/useTerminalLifecycle.test.ts` | Existing test coverage | test-quality |

---

## Root Causes

| Root Cause | Findings | Suggested Fix |
|------------|----------|---------------|
| Bare `invoke()` bypassing transport | #2, #3 | Route through `usePty` + `mapCommandToHttp` |
| No tests for new API surface | #1, #6, #9 | Add unit tests for `get_kitty_flags`, env vars, `ref.focus()` |
| Silent fallback vs explicit error | #3, #5 | Return `Option<u32>` or log unexpected errors |

## Single-Fix Opportunities

1. **Route through `usePty`** — Fixes #2 and #3 (~15 lines across `usePty.ts`, `transport.ts`, `Terminal.tsx`)
2. **Add Rust unit tests** — Fixes #1 and #6 (~30 lines in `pty.rs` test module)
