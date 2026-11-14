# Mobile live audit — 2026-10-20

## Scope and isolation

Stories: 1985-45bd and 1900-5ff6. Source revision: `e6ab3b7fcd41546348c4cd52d6f0893c3cb43359`, branch `chore/live-mobile`.

The native macOS headless backend was built in this worktree through `build-slot.sh cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features`, with the mbx shim. Build passed in 1m30s; mbx doctor reported zero failures. This is a native build, not an rb test result. No test suite ran.

Backend port: 22185. Worktree Vite port: 22186, strict port. Backend HOME, instance configuration and temporary files were below `~/Gits/.tmp/results/live-mobile/`. No desktop instance was started. The production application, port 9876 and its configuration were not used.

The mandated browser wrapper used named session `live-mobile`, with its orphan scan scoped to the evidence directory. The viewport was 390×844. One disposable shell and one real Claude CLI were created through the isolated backend. Claude reached its idle composer and its local `/permissions` dialog; no model question or paid inference was submitted.

## Evidence

Retained local evidence is at `~/Gits/.tmp/results/live-mobile/`:

- `build.log`, `mbx-doctor.txt`: native build and cache prerequisites.
- `resume.mjs`, `resume.json`: browser probe and complete snapshots.
- `current.png`, `list-dom.txt`, `permissions-dom.txt`: browser screenshot and DOM captures.
- `before-exit-sessions.json`, `claude-before.json`, `permissions-output.json`, `model-output.json`: actual backend state and native CLI output.
- `network.mjs`, `network.json`, `app-logs.json`: request and diagnostic evidence.
- `cleanup.json`, `sessions-after-cleanup.json`, `pids.json`: owned resources and cleanup.

These are local evidence paths, not committed fixtures.

## 1900-5ff6: resume

The real mobile detail screen retained one root child and a shell rectangle at `(0,0,390,844)` in all recorded snapshots:

1. Baseline, with real Claude terminal output.
2. Three synthetic hidden/pagehide/pageshow/visible cycles, `persisted=false`.
3. Three synthetic cycles, `persisted=true`.
4. Native Chromium CDP `frozen` then `active`.

No `Runtime.exceptionThrown` event was captured by this probe. Native document visibility remained hidden. Synthetic visibility and persisted values are not native iOS evidence, and synthetic persisted events do not prove bfcache. Vite later reloaded the page, so this run cannot prove retained-document recovery after freezing.

The isolated backend received the existing `Mobile previous document lifecycle trail` warning. Its payload contains URL without query credentials, root child count, shell rectangle, viewport and lifecycle events. The Vite harness did not proxy the configuration endpoints: theme and notes requests returned HTML and produced expected-JSON warnings. Those are harness limitations, not the original grey-screen failure.

The empty grey-screen symptom was not reproduced. No original fix criterion was checked. Physical standalone iPhone behavior remains unknown.

## 1985-45bd: agent exit

The browser displayed the real Claude detail header, CLI/Chat controls, terminal output and editable composer. The local permissions dialog was captured in native output, but the API did not report `awaiting_input=true`; it is not an awaiting-state pass. No busy-agent exit, awaiting-agent exit, recovered shell submission or ended-PTY Back round trip was proved.

Later browser actions were not reliable enough to complete those checks. Requests to the isolated backend were pending in `network.json`, while a direct browser fetch with the explicit audit token returned both real sessions. A later wrapper navigation timed out with `waiting for another entity to finish attaching`. The exact cause of the pending application requests was not isolated. Shared-host cookie contention is a hypothesis only.

An attempt to use a separate `live-mobile.localhost` hostname received HTTP 403 `Untrusted Host`. The request boundary was not weakened or bypassed. That attempt supplies no application pass. The existing phone-only visual criterion remains unchecked.

## Harness failures and limits

- The first CDP probe accidentally selected the service worker target and timed out. The corrected probe pinned the exact owned page target and produced `resume.json`.
- Early programmatic `sendCommand` calls used an incorrect argument order and failed before command delivery. The corrected call opened only the local permissions dialog. These errors are not product regressions.
- Browser automation after freeze/reload was incomplete; unavailable round trips are not passes.
- Chromium emulation does not establish physical iPhone touch, standalone process reclamation, WebKit paint or app-switch behavior.

## Cleanup and remaining work

Both owned PTYs were deleted. `sessions-after-cleanup.json` is `[]`. Backend PID 49994 and Vite PID 49996 received SIGTERM; a subsequent `ps` found neither. The owned browser tab closed, then `agent-browser --session live-mobile close` closed its named daemon. Shared Chrome was not stopped. The temporary Vite configuration was removed.

No production code, tests or capture corpus changed. No story was closed and no acceptance criterion was newly checked.

Remaining emulated checks: obtain a stable authenticated browser session, then capture actual busy and confidently-awaiting agent exits, shell input/output recovery, PTY closure and Back. Remaining physical checks for Boss: perform the 1985 exit flows on a phone; switch the installed standalone PWA away and back three times for 1900. If the grey screen recurs, retain its automatic lifecycle report and capture URL, DOM/root/shell geometry, viewport and console before reloading. Do not infer the missing failure cause from this successful shell-retention probe.


## 1985 retry: isolated desktop, 2026-10-20 09:38–09:42 CEST

Source: `aeebb64e819ea6e293e56f907b1fb843dab1d255`, branch `chore/desk-1985`.
The authorized command was `make agent-test AGENT_TEST_INSTANCE=at-1985
AGENT_TEST_PORT=19880`, after its dry run. The target built embedded assets and
native macOS binaries through build-slot and mbx; mbx doctor reported zero
failures. This was not an rb build or a test-suite run. No Vite server was needed.
The named ab-stealth session `desk-1985` opened the instance's `/mobile` page at
390×844. Authentication used the instance's session-token API through its own
Unix socket, not the daily app or a credential-store read. The browser requests
completed; the previous pending-request harness problem did not recur.

Evidence is retained locally at `~/Gits/.tmp/results/desk-1985/`. One disposable
PTY, `bdd756b5-34d8-45f4-9264-2beb39da1833`, ran two real Claude Code processes.
The existing account was available; no login was performed. One short inference
was interrupted while working; a second requested only an AskUserQuestion picker.
No repository modification was requested from either process.

| Check | Observed result | Evidence |
| --- | --- | --- |
| Busy exit | API reported `agent_type=claude`, `agent_state=working`, `awaiting_input=false`; SIGTERM to the recorded owned Claude PID restored Terminal branding and removed CLI/Chat controls | `busy-before-exit.json`, `busy-before-exit.txt`, `busy-after-exit.json`, `busy-after-exit.txt`, `busy-after-exit.png` |
| Shell after busy exit | The mobile composer submitted `printf`; terminal output contained the separate result line `BUSY_EXIT_SHELL_OK` | `busy-shell-output.txt` |
| Awaiting exit | Real AskUserQuestion displayed Red/Blue buttons; API reported `awaiting_input=true`, `question_confident=true`, and the actual choice prompt. SIGTERM restored Terminal branding and removed those answer buttons; the next API snapshot omitted agent/question fields and reported shell idle | `await-progress.json`, `await-progress.txt`, `awaiting-before-exit.png`, `await-after-exit.json`, `await-after-exit.txt` |
| Shell after awaiting exit | The mobile composer submitted `printf`; terminal output contained `AWAIT_EXIT_SHELL_OK`. However, a later Input badge and Yes/No shortcuts appeared again: see the anomaly below | `await-shell-output.txt`, `await-shell-recovered.png` |
| PTY closure | Submitting `exit` through the mobile composer displayed Session ended and Back, with composer/keybar disabled. Clicking Back returned to No active sessions | `pty-ended.txt`, `pty-ended.png`, `pty-ended.json`, `back-to-sessions.txt` |

### Backend question re-arm anomaly: not an unqualified pass

After awaiting-agent exit, `await-after-exit.json` shows the correct shell
snapshot: no agent, no question, `awaiting_input=false`, `shell_state=idle`.
The shell then printed the requested marker successfully. Nevertheless,
`await-shell-output.txt` shows Terminal branding with an Input badge and Yes/No
shortcuts, while the old native question footer remains in terminal history.
The native backend log records new confident question events at
`07:41:28.250719Z` and `07:41:29.043168Z`, both with
`hook_instrumented=false`, after the shell-return snapshot. Thus this is not
only evidence of the mobile retaining its pre-exit WebSocket object.

The exact producer was not isolated by replay. Source inspection identifies
`rearm_awaiting_for_open_dialog` and its full-screen call in `pty.rs` as a
candidate: they accept a retained Ink footer without a foreground-agent argument.
Do not treat that candidate as a proven root cause. No production change, RED
regression, or new story was made for this separate backend detection finding.
The coordinator must route it to the backend owner before claiming completely
clean awaiting-exit state. Shell input/output recovery and ended-PTY navigation
were proved despite the indicator anomaly.

`exit-state-logs.txt` retains the relevant native events. The local-only
`await-exit-local-only.tcap` starts after the original question was rendered; it
contains subsequent exit/shell traffic, not the full question-start sequence.
It is not a committed fixture and has no replay-oracle golden. Do not claim it
alone can reproduce the complete detector transition.

### Cleanup and criterion status

The PTY exited normally and `/sessions` returned `[]`. The named browser tab and
its own `ab-shared-desk-1985-81136` daemon were closed; shared Chrome was not
stopped. The matching `make agent-test-stop` target stopped only `at-1985`.
Recorded supervisor, shell and both Claude PIDs no longer existed; port 19880
refused connections. No daily-app endpoint, installed binary, or daily config was
used. The isolated instance retains its own configuration and local evidence.

Criteria 2 and 3 were already checked; their check-criterion commands confirmed
that state. `story-close.sh --check` returned check-ok and explicitly reported
only the unchecked physical-phone VISUAL criterion 4. It remains unchecked for
Boss. Chromium emulation is not physical-phone touch, iOS/WebKit, or standalone
PWA evidence. No story was closed. The new backend anomaly remains a material
limit on this retry, not a reason to weaken the physical-phone criterion.
