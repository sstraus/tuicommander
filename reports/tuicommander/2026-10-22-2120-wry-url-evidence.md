# Story 2120-9f3f: WebView URL evidence

## Status

The historical panic message and its calling frame were not found in the
inspected sources. Do not identify the recovery thread as the historical caller.
The coordinator revised the assignment to a preventive fix of both native URL
getter callers and authorized the exact isolated `agent-test-2120` directory.
The shared nil-safe adapter is implemented; verification is recorded below.

## Read-only evidence

Inspected `~/Library/Logs/DiagnosticReports`, including `Retired`, for TUIC reports
from 2026-10-22. The 25 matching reports are for `tuic-aarch64-apple-darwin`.
Every report has the same termination classification and zero stack frames.
For example:

`~/Library/Logs/DiagnosticReports/tuic-aarch64-apple-darwin-2026-10-22-074822.ips`

```text
"captureTime" : "2026-10-22 07:48:22.5460 +0200"
"procName" : "tuic-aarch64-apple-darwin"
"exception" : {"codes":"0x0000000000000000, 0x0000000000000000","rawCodes":[0,0],"type":"EXC_CRASH","signal":"SIGKILL (Code Signature Invalid)"}
"termination" : {"flags":66,"code":1,"namespace":"CODESIGNING","indicator":"Taskgated Invalid Signature"}
```

The triggered thread has `"frames":[]`. These reports do not contain a Rust
panic message or a backtrace. They do not prove that Wry caused the reported
desktop process death.

Inspected the persisted TUIC log:

`~/Library/Application Support/com.tuic.commander/logs/tuic.log.2026-10-22`

A case-insensitive search for `panic`, `wry`, `unwrap`, and `backtrace` finds no
panic entry. The only `wry` substring match is the newly created worktree path
`fix-2120-wry-url-panic`. Rust stderr is not established as an input to this log.
No files under the application support directory were changed.

## Proven source defect and caller audit

The worktree lockfile selects Wry 0.57.0 and tauri-runtime-wry 2.12.1.
The installed source is:

`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/wry-0.57.0/src/wkwebview/mod.rs:1369`

```rust
pub fn url_from_webview(webview: &WKWebView) -> Result<String> {
  let url_obj = unsafe { webview.URL().unwrap() };
  let absolute_url = url_obj.absoluteString().unwrap();
```

Thus, a nil URL or absolute string panics before a `Result` can be returned.
This establishes a possible failure path, not a historical calling frame.
The coordinator identified the IPS reports as stale-signature failures of the
`tuic` CLI, not the reported desktop crash; that attribution is coordinator
context, separate from the quoted report fields.

A source-wide search for `.url(` in `src-tauri/src` finds these native desktop
getters:

| Caller | Current handling | Audit result |
| --- | --- | --- |
| `webview_recovery.rs:156`, `desktop::lost_document` | `window.url().ok()?` | `ok()` does not catch the native getter's panic. |
| `background_mode_windows.rs:64`, `DesktopWindows::destroy` | `window.url().map_err(...)` | The detached-panel snapshot has the same exposure on macOS. The file is a desktop adapter, not Windows-only. |

The `payload.url()` calls in `lib.rs` read an existing page-load payload. They
are not native URL lookups. The async `candidate.url()` in
`design_mode/manager.rs` reads a CDP page. Other `.url()` matches are HTTP
requests, redirect attempts, or test-server addresses, not WKWebView getters.

The fix uses `webview_url::read` and `with_webview` to read the nullable native
`URL` and `absoluteString` values on the event-loop thread without `unwrap`.
Both native getter callers use that path. The callback returns only an owned
Rust URL across the reply channel. A missing or unparseable native URL returns
`None`; the recovery poll skips it without changing the saved boot URL, and the
panel snapshot returns `WebView has no committed URL` before destroying its
window. Non-macOS getters retain their original Tauri implementation.

Catching an unwind in the poller is not suitable: Wry reads the URL on the
event-loop thread, not the worker waiting for the reply. No `catch_unwind` was
added. Tauri runtime dispatch executes callbacks inline when already on the
main thread; the detached-panel snapshot therefore does not wait for a callback
queued behind itself. A worker waits just as it did with the old Tauri getter.
The poll schedule and recovery backoff are unchanged.

## Black screen

The evidence above does not establish why the initial window was black, whether
WebKit returned nil, or whether the black screen and reported desktop crash
share a cause. The code-signature reports are not evidence for those claims.
The coordinator must decide whether a separate investigation story is needed;
this managed peer did not create an out-of-scope story.

## Live-check prerequisite conflict

Ran only the non-mutating prerequisite:

```sh
make agent-test AGENT_TEST_DRY_RUN=1 AGENT_TEST_INSTANCE=agent-test-2120 AGENT_TEST_PORT=19880
```

It reports:

```text
config_dir: /Users/stefano.straus/Library/Application Support/com.tuic.commander/instances/agent-test-2120
listener: 127.0.0.1:19880 (no port fallback)
```

This is under the assignment's explicitly read-only application support root.
A real launch writes there. The coordinator subsequently authorized this exact
instance directory, and no other application-support writes. The daily app and
port 9876 remain off limits.

## Validation

- Instruction-link checker: passed, two instruction pairs.
- `git diff --check`: passed.
- rb command: `build-slot.sh --remote scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(webview_url::tests) | test(webview_recovery::tests)'`.
- rb result: six recovery tests passed; build 5m18s, test execution 0.043s.
- Evidence: `~/Gits/.tmp/results/tuic-2120-targeted-rb.log`.
- The three `webview_url::tests` cases are macOS-only and absent on rb, not
  passed by the Linux run. The rb macOS-path allowlist does not cover these
  native getter paths; the coordinator authorized targeted native tests and
  will handle that allowlist gap separately.
- Native command: `build-slot.sh scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(webview_url::tests)'`, with the mbx cargo shim and Gits-local TMPDIR.
- Native result: three tests passed; build 3m26s, test execution 0.034s.
  Evidence: `~/Gits/.tmp/results/tuic-2120-native-tests.log`.
  This tests nil NSURL, empty native URL, and retained app/about URLs against
  Foundation, not a hand-written URL fixture. An unavailable absoluteString is
  handled by `?`; that separate native state was not induced.
- Native build warnings: existing deprecated `AtomicU32::fetch_update` in
  `design_mode/manager.rs:970`, plus a linker compact-unwind-size warning.
- Isolated desktop build: `make agent-test AGENT_TEST_INSTANCE=agent-test-2120 AGENT_TEST_PORT=19880`; 5m03s dev-profile build. The harness communicated only through that instance's Unix socket.
- Live result: main and `panel-activity` existed; the same backend instance
  `82aa68e2-b4cc-406c-91b0-6a8be3011aa4` answered through uptime 171s. It survived
  the 30s startup delay and several 15s recovery polls, then a one-minute
  background unload interval. `debug/invoke_js` finally returned
  `main window not found` while health remained OK. `destroy_hidden` processes
  panels before main and aborts on a failed snapshot: main destruction with the
  detached panel present therefore exercises the panel URL snapshot successfully.
  This is a healthy-URL lifecycle check, not live nil-URL fault injection.
- Live evidence: `~/Gits/.tmp/results/tuic-2120-live.log`,
  `~/Gits/.tmp/results/tuic-2120-live/{desktop.log,configured.json,initial-health.json,final-health.json,final-logs.json}`;
  harness: `~/Gits/.tmp/results/tuic-2120-live.py`.
- The harness stopped its own instance using `make agent-test-stop`; the
  desktop exited on the requested SIGTERM (make reports Error 143), and the
  harness returned 0. Its recorded supervisor PID 52350 no longer exists and
  the instance PID file was removed.
- Historical RED: unavailable because the original panic/caller is not retained.
  The coordinator accepted the nil-NSURL contract test as a fallback, not as a
  claim that the historical process crash was reproduced.
- Native source-defect RED: the standalone `webkit_nil_url` target creates a
  real unloaded WKWebView on the process main thread with a nonpersistent data
  store. WebKit returned nil, and the former URL unwrap pattern panicked. The
  probe captures and validates that expected panic. It does not reproduce the
  historical desktop crash or run the pre-fix application.
- `cargo test --manifest-path src-tauri/Cargo.toml --test webkit_nil_url`
  passed, and `cargo nextest list --manifest-path src-tauri/Cargo.toml --test
  webkit_nil_url` discovered `unloaded_webkit_url_unwrap_panics`.
  Evidence: `~/Gits/.tmp/results/tuic-2120-real-webkit-main-fixed.log`.
  The existing nil-conversion unit test remains the safe-conversion GREEN half.
- Native nextest execution of `webkit_nil_url`, filtered to
  `unloaded_webkit_url_unwrap_panics`, passed: one test run, one passed, zero
  skipped; 2m12s build and 0.199s test run. The command ran through build-slot,
  the mbx shim, and `scripts/with-test-tmp.sh` with Gits-local TMPDIR.
  Evidence: `~/Gits/.tmp/results/tuic-2120-real-webkit-nextest.log`.
- Both the original nextest unit probe and plain libtest with one test thread
  failed the main-thread prerequisite before native construction. Those failures
  are not RED evidence. The standalone target uses `harness = false` to supply
  its own process-main-thread entry point. An initial compile error was corrected
  by receiving the native initializer's raw pointer and adopting its ownership
  with `Retained::from_raw`; production visibility was not changed.
- Mutation, a full suite, and a daily-app restart are not part of this peer run.

## Isolation caveat discovered after the live run

The dedicated instance's desktop log reports its debug credential-store path as
`/Users/stefano.straus/.tuicommander-dev/instances/agent-test-2120/credentials.json`.
This is outside the advertised application-support instance directory and outside
Gits. No credential-store file was read or inspected. The approved agent-test
preset produced this path automatically; no further live run is planned here.
The coordinator must reconcile that preset isolation gap before reusing it under
an exact-directory-only write grant. This is not evidence of a daily-store write.
