# Code Review: Story 415 — git_cli Consolidation
**Date:** 2026-02-27
**Reviewers:** Multi-Agent (rust, security, architecture, simplicity, silent-failure, performance, test-quality)
**Target:** Commits ac8bb3a..f373d6c (5 commits, 9 files, +606/-578 lines)
**Confidence Threshold:** 70

## Summary
- **P1 Critical Issues:** 1
- **P2 Important Issues:** 10
- **P3 Nice-to-Have:** 9
- **Filtered Out (below threshold):** 2

---

## P1 — Critical (Block Merge)

- [x] **#1 [SILENT-FAILURE]** `switch_branch` dirty check silently skips when git fails (Confidence: 92)
  - Location: `src-tauri/src/worktree.rs:661-670`
  - Issue: `run_silent()` returns `None` on spawn failure → dirty check is entirely skipped → branch switch proceeds as if clean. The old code used `.map_err(...)` which propagated spawn failures as hard errors. This is a **regression** introduced by the refactor.
  - Fix: Use `run()` with explicit error handling instead of `run_silent()`:
    ```rust
    let status_out = git_cmd(&base_repo)
        .args(&["status", "--porcelain"])
        .run()
        .map_err(|e| format!("Failed to check working tree status: {e}"))?;
    if !status_out.stdout.trim().is_empty() {
        return Err("dirty".to_string());
    }
    ```
  - Agent: silent-failure-hunter

---

## P2 — Important (Fix Before/After Merge)

- [x] **#2 [SILENT-FAILURE]** `check_worktree_dirty` returns `false` when git fails (Confidence: 85)
  - Location: `src-tauri/src/worktree.rs:370-390`
  - Issue: Same regression as #1 — spawn failure now returns `Ok(false)` instead of `Err`. Old code propagated spawn failures.
  - Fix: Use `run()` and match on `GitError::NonZeroExit` (return `Ok(false)`) vs `SpawnFailed` (return `Err`).
  - Agent: silent-failure-hunter

- [x] **#3 [ERRORS]** `get_file_diff` ls-files probe treats spawn failure as "untracked" (Confidence: 80)
  - Location: `src-tauri/src/git.rs:445-448`
  - Issue: `.run().is_err()` conflates `SpawnFailed` with `NonZeroExit`. If git can't spawn, the code proceeds to `--no-index` diff path (which also fails to spawn, but is caught there).
  - Fix: Match on `GitError` variants explicitly — propagate `SpawnFailed`, treat only `NonZeroExit` as "untracked".
  - Agent: rust-reviewer

- [x] **#4 [SILENT-FAILURE]** `run_silent()` conflates spawn failure with expected non-zero exits (Confidence: 80)
  - Location: `src-tauri/src/git_cli.rs:122-124`
  - Issue: Root cause of #1, #2. `run_silent() = self.run().ok()` — both `SpawnFailed` and `NonZeroExit` become `None`.
  - Fix: Log spawn failures inside `run_silent()`:
    ```rust
    pub fn run_silent(self) -> Option<GitOutput> {
        match self.run() {
            Ok(o) => Some(o),
            Err(GitError::SpawnFailed(e)) => {
                eprintln!("[git_cli] spawn failed: {e}");
                None
            }
            Err(GitError::NonZeroExit { .. }) => None,
        }
    }
    ```
  - Agent: silent-failure-hunter

- [x] **#5 [DOCS/API]** `run()` docstring says "Returns the trimmed stdout" — false (Confidence: 95)
  - Location: `src-tauri/src/git_cli.rs:103`
  - Issue: stdout is NOT trimmed (only stderr is). The docstring will mislead the next engineer.
  - Fix: Update to `"Returns GitOutput containing raw stdout and trimmed stderr on success."`
  - Agent: architecture-reviewer

- [x] **#6 [PERF]** `run()` always allocates both stdout and stderr before checking exit code (Confidence: 95)
  - Location: `src-tauri/src/git_cli.rs:107-108`
  - Issue: On failure, stdout is allocated then dropped (wasted). On success, stderr is allocated but never read by any caller.
  - Fix: Check exit status first, then allocate only the needed field per branch.
  - Agent: performance-reviewer

- [x] **#7 [YAGNI]** `GitOutput.stderr` is dead — never read by any caller (Confidence: 97)
  - Location: `src-tauri/src/git_cli.rs:60-63`
  - Issue: `#[allow(dead_code)]` suppresses the warning. No production callsite reads `.stderr`. The `run_raw()` escape hatch exists for callers that need stderr.
  - Fix: Remove the field. Set `stderr: String::new()` only if keeping it for future use (but then use a comment, not `#[allow]`).
  - Agent: simplicity-reviewer

- [x] **#8 [YAGNI]** `GitCmd::arg()` is dead API surface (Confidence: 96)
  - Location: `src-tauri/src/git_cli.rs:83-87`
  - Issue: `#[allow(dead_code)]`. Zero callers. Every callsite uses `.args()`.
  - Fix: Delete it. If needed in the future, it's 3 lines to add back.
  - Agent: simplicity-reviewer

- [x] **#9 [SECURITY]** `get_ignored_paths` passes paths without `--` separator (Confidence: 82)
  - Location: `src-tauri/src/fs.rs:135-138`
  - Issue: File paths beginning with `-` (e.g., `--stdin`) would be interpreted as git options. Pre-existing, not introduced by refactor.
  - Fix: Add `"--"` before path arguments: `vec!["check-ignore", "--no-index", "--"]`
  - Agent: security-reviewer

- [x] **#10 [SILENT-FAILURE]** `archive_worktree` worktree remove failure is completely silent (Confidence: 82)
  - Location: `src-tauri/src/worktree.rs:860-862`
  - Issue: `let _ =` discards the error with no log. Inconsistent with `remove_worktree_internal` which logs prune failures.
  - Fix: `if let Err(e) = ... { eprintln!("..."); }`
  - Agent: silent-failure-hunter

- [x] **#11 [API-DESIGN]** `args()` should accept `impl AsRef<OsStr>` to eliminate double-collect pattern (Confidence: 90)
  - Location: `src-tauri/src/git_cli.rs:89-92`; callsites at `git.rs:278,295,328,338,472`; `worktree.rs:130,182,859`
  - Issue: 7+ callsites must materialize `Vec<String>` → `Vec<&str>` because `args()` takes `&[&str]`. Path arguments also need `to_string_lossy().to_string()`. The underlying `Command::args()` already accepts `AsRef<OsStr>`.
  - Fix: Make `args()` generic: `pub fn args<I, S>(mut self, args: I) -> Self where I: IntoIterator<Item = S>, S: AsRef<std::ffi::OsStr>`
  - Agent: rust-reviewer, performance-reviewer

---

## P3 — Nice-to-Have

- [x] **#12** `git_cli.rs` test helper uses raw `Command::new("git")` instead of `git_cmd()` (Confidence: 82) — architecture
- [x] **#13** `fs.rs`, `github.rs`, `worktree_routes.rs` use full `crate::git_cli::git_cmd(...)` path without top-level import (Confidence: 85) — architecture
- [x] **#14** `test_run_success` verifies `is_ok()` but not output content (Confidence: 88) — test-quality
- [x] **#15** `test_env_is_passed` doesn't verify the env var reaches the subprocess (Confidence: 90) — test-quality
- [x] **#16** `GIT_TERMINAL_PROMPT=0` behavior has no test (Confidence: 82) — test-quality
- [x] **#17** `GitError::SpawnFailed` Display variant not tested (Confidence: 90) — test-quality
- [x] **#18** `test_run_non_zero_exit` ignores stderr content (Confidence: 80) — test-quality
- [x] **#19** `run_raw` spawn-failure path not tested (Confidence: 92) — test-quality
- [x] **#20** askpass script macOS `$1` in osascript — injection risk from malicious SSH server (Confidence: 72) — security (pre-existing)

---

## Cross-Cutting Analysis

### Root Causes Identified

| Root Cause | Findings Affected | Suggested Fix |
|------------|-------------------|---------------|
| `run_silent()` conflates spawn failure with non-zero exit | #1, #2, #3, #4 | Log spawn failures inside `run_silent()` + use `run()` for safety-gate callsites |
| `args()` requires `&[&str]` instead of generic `AsRef<OsStr>` | #11, P3 path conversions | Make `args()` generic — one-line signature change |
| `GitOutput.stderr` dead field + `arg()` dead method | #6, #7, #8 | Remove dead code, eliminate `#[allow(dead_code)]` |
| Test assertions too shallow | #14, #15, #16, #17, #18, #19 | Strengthen existing tests to verify output content, not just `is_ok()` |

### Single-Fix Opportunities

1. **Log spawn failures in `run_silent()`** — Fixes #1, #2, #4 and adds diagnostics for #3 (~8 lines)
2. **Make `args()` generic** — Fixes #11 and eliminates all 7+ `Vec<&str>` intermediates (~3 lines in git_cli.rs, removes ~15 lines across callsites)
3. **Remove dead `stderr` field + `arg()` method** — Fixes #6, #7, #8 (~10 lines removed)

### Context Files (Read Before Fixing)

| File | Reason | Referenced By |
|------|--------|---------------|
| `src-tauri/src/cli.rs` | Defines `resolve_cli()` — git binary resolution | rust, architecture |
| `src-tauri/src/state.rs` | `AppState::invalidate_repo_caches` — cache contract | architecture |
| `src-tauri/src/git.rs:666-714` | `ensure_askpass_script()` — SSH injection risk | security |

---

## Agent Highlights

- **Rust:** `args()` API should be generic over `AsRef<OsStr>` to match `Command::args()` ergonomics; `ls-files` probe conflates error types
- **Security:** Net positive — `GIT_TERMINAL_PROMPT=0` is now universal. Two pre-existing issues found (askpass injection, missing `--` separator)
- **Architecture:** Sound module boundary. One false docstring on `run()` (P2)
- **Simplicity:** Builder pattern justified. Two dead-code items suppressed with `#[allow]` instead of deleted
- **Silent Failures:** **One regression** — `switch_branch` dirty check demoted from hard error to silent skip. Root cause: `run_silent()` design
- **Performance:** stdout/stderr allocated before exit-code check. Generic `args()` eliminates 7+ intermediate Vecs
- **Test Quality:** 9 tests cover main paths but assertions are shallow — verify `is_ok()` not output content. Security-relevant `GIT_TERMINAL_PROMPT` untested

---

## Recommended Actions

1. **Immediate (P1):** Fix `switch_branch` dirty check — use `run()` instead of `run_silent()` for safety gates
2. **This session:** Address P2 items #2-#11 — most are small fixes
3. **Follow-up:** Strengthen test assertions (P3 #14-#19)
