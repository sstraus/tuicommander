# Design Mode command lifecycle — story 2596-b9c6

## Scope and limits

The reported trial returned `armed` from Edge, then timed out for every agent browser command. The original trial is recorded in the orchestrator report `2026-10-31-design-mode-trial`.

This change removes false-success startup, closes disconnected command queues, starts the worker receiver without a sidebar request, makes receive failures visible, and retains ownership of the dedicated browser process. It does **not** establish the exact reason that the original worker lost its replies. The old worker discarded receive-loop exceptions, and the trial has no capture of those exceptions.

The installed original-profile `worker.js`, `browserCommands.js`, and `panel.html` had the same SHA-256 hashes as the pre-fix checkout. Edge Secure Preferences pointed to that extension directory. This rules out stale files at that path, not stale in-memory extension state. The separate panel-layout investigation owns the missing **Allow eval on this host** button; this change does not modify panel HTML or CSS.

## Proven defects and history

- `CommandChannel::cancel` cleared existing pending replies but left the queue open. A command submitted after native disconnect waited for the normal 10-second timeout instead of reporting disconnection.
- Worker startup did not start the command receiver. The first sidebar/native request, or a later heartbeat, had to start it.
- A completed command reply depended on saving optional action history. A storage failure could suppress that reply. Receive-loop failures were silently discarded.
- The launcher spawned Edge and returned `armed` without a browser acknowledgment. Stop had no retained owner for the process launched by the extension path.
- The MCP bridge and browser command both had 10-second deadlines. Either could win, explaining why an unanswered command could appear as an IPC read timeout or a browser `command_timeout`.

These code defects are present in `e4478925c8` (`feat(design-mode): add correlated native browser commands`, 2026-10-14), the introducing commit for correlated commands. They are not demonstrated as a later regression. No verified successful historical native command sequence was found; this report does not claim that the old path never worked.

## Regression evidence

Base commit: `574ebca11e`.

- Rust RED: `disconnected_channel_rejects_new_commands_and_releases_receiver` failed because the error was `command_timeout`, not `browser_disconnected`. Warnings-denied Clippy passed in the same rb call.
- Worker RED: `starts the native command receiver when the worker boots without a sidebar` failed because no `command_next` request was made at worker startup.
- Local final: 12 tests passed across `worker.test.ts` and `browserCommands.test.ts`, with no async leaks.
- rb: command-channel and manager tests passed. After fixing an idempotent-close race, all six launcher/process tests passed, including silent startup failure, runtime-independent reaping and repeated concurrent Stop.
- rb: both bridge-deadline tests passed; warnings-denied Clippy passed for `tuicommander` and `tuic-bridge` with all targets.

The first GREEN attempt exposed a test-only Clippy error and unresolved worker-test promises. The next exposed a race between repeated Stop calls and a disappearing reply channel. These failures were fixed, not disabled. Process closure now has one shared completion result.

## Native Edge trial

The trial used a native worktree build in the isolated `agent-test-2596` instance, not the running daily app. A throwaway real ego PTY supplied the agent identity; it remained at its workspace-trust question and received no model prompt. Worktree Vite served the existing composer geometry fixture on `127.0.0.1:58595`. The requests used MCP `ui` actions against the isolated instance socket, with the throwaway session header.

| Action | Result | Duration | Frontmost before | Frontmost after |
|---|---|---:|---|---|
| `design_start`, Edge | Acknowledged `armed`; launched tab selected without opening the sidebar | 7026 ms | tuicommander | Microsoft Edge |
| `design_screenshot` | Immediate `capture_permission_required`, with the separate capture grant guidance | 309 ms | Microsoft Edge | Microsoft Edge |
| `design_errors` | Fenced `No captured page errors` | 238 ms | Microsoft Edge | Microsoft Edge |
| `design_eval`, `1+1` | Immediate `eval_not_granted: grant needed: click Allow eval on this host in the Design Mode sidebar.` | 358 ms | Microsoft Edge | Microsoft Edge |
| `design_stop` | `stopped`; owned Edge process closed and reaped | 307 ms | Microsoft Edge | tuicommander |

The fresh Edge profile had no capture consent. No screenshot PNG was produced, and no successful visual-capture claim is made. Capture permission is independent of eval permission. No grant was silently added and no other browser window was manipulated.

Raw evidence is retained under `~/Gits/.tmp/dm-2596/`: `red.log`, `js-red.log`, `green-retry.log`, `rust-final.log`, and `live/{tool-results.json,focus.tsv,setup.json}`.

## Sequential native replay after rebase

The branch was rebased onto `fbbdc90c2d59a5cd5e134641d5ddfc6e57c617d5`, including panel fix 2592. Fixed implementation HEAD was `9b76888f4659e9bed51f0eac871690f837824ebf`. For the pre-fix build, the clean worktree's tracked files were restored from that main commit without switching branches. The fixed HEAD was restored before the second build. Only one isolated desktop instance ran at a time, with the same instance name and Edge profile.

The pre-fix sequence did **not** reproduce the historical IPC or command timeouts. Start returned `armed` in 1083 ms, before Edge became frontmost. Screenshot, errors, eval, reload and navigate each returned `no_reviewed_tab: Open the Design Mode sidebar on a page first.` Their durations were 1860, 239, 300, 254 and 239 ms. Stop returned success in 230 ms but left the launched Edge process running. The exact owned browser PID, 87011, was then terminated. This proves false-success startup and missing browser ownership in this replay, not the cause of the original missing replies.

The fixed sequence selected the reviewed tab without opening the sidebar. Errors succeeded; eval returned the actionable grant denial in 261 ms; reload and navigate succeeded in 289 and 271 ms. Stop succeeded in 280 ms and returned focus from Edge to TUIC. All requested actions have before/after frontmost application records in `fixed-replay.log`. The original timeout attribution remains unknown. Worker receive exceptions are now retained and reported instead of discarded, so a recurrence has a diagnostic path.

## Native capture consent and grant-control verification

A further fixed-build run used the same isolated instance. Start acknowledged in 5233 ms. Screenshot initially returned the separate `capture_permission_required` error in 296 ms; errors succeeded in 474 ms and denied eval returned its guidance in 415 ms.

The owned page URL was verified as `http://127.0.0.1:54907/src/__tests__/browser/aiChatComposerGeometry.html`. The extension action shortcut opened its native sidebar in owned Edge window 14730. MacControl found a visible **Allow eval on this host** button at window-relative `(984, 434)`, size `124 × 30`, and a separate **Let agents capture any site** button at `(830, 434)`, size `146 × 30`. This verifies the rebased panel control without granting eval.

The first capture-consent click was interrupted by physical user activity. Capture remained denied. A later native click completed without that interruption. MCP `ui design_screenshot` then succeeded in 406 ms, with Edge frontmost before and after. Its returned PNG was copied to `capture-live/mcp-screenshot.png`: 1612 × 2496 RGB, SHA-256 `210db04af119e2d4cd1c832dc4fc938206dbec9279e98078d42550a915198b43`. The scoped native-window screenshot is `capture-live/native-sidebar.png`: 2400 × 2732 RGBA, SHA-256 `7edb069ab1571831bd8b687bf78200420c4c6f20e18396548053f61dcce179a3`. No daily browser consent or eval grant was changed.

Additional evidence under `~/Gits/.tmp/dm-2596/`: `replay-refs.txt`, `prefix-replay.log`, `prefix-live/{tool-results.json,focus.tsv,setup.json}`, `fixed-replay.log`, `fixed-live/{tool-results.json,focus.tsv,setup.json}`, `capture-replay.log`, `capture-live/tool-results.json`, and `capture-after-native-click.json`.

### Harness failures and final cleanup

The first fixed replay's optional self-notification failed because `TUIC_SESSION` was absent from the Node environment. Its browser and PTY cleanup succeeded, but its control server kept Node alive. That owned Node PID, 10286, was stopped. The retry used the explicit registered peer ID.

During the capture run, an AppleScript accessibility read failed with `osascript is not allowed assistive access`; no permission workaround was used. The final frontmost-app probe also failed with `Can't get application process 1 whose frontmost = true`. That harness exception interrupted its automatic Stop/PTY cleanup. The named app was stopped, and its exact owned Edge PID, 11525, was terminated explicitly. Final probes found no listener on 19876, 19877 or 54907 and no browser process using this instance's Design Mode profile. The successful PNG and its MCP response precede that cleanup failure. The earlier fixed replay, not this interrupted cleanup, is the evidence for successful `design_stop` ownership.

## Cleanup and deployment

Stop closed the dedicated Edge launch. The throwaway PTY was deleted, Vite was closed, and the named agent-test instance was stopped. Final probes found no listener on the Vite port or `19876`, and no process using this instance's Design Mode profile.

The daily instance was not rebuilt or restarted. Loading the change there requires a Rust and sidecar rebuild and a manual restart when losing its current sessions is acceptable.
