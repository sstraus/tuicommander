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

## Cleanup and deployment

Stop closed the dedicated Edge launch. The throwaway PTY was deleted, Vite was closed, and the named agent-test instance was stopped. Final probes found no listener on the Vite port or `19876`, and no process using this instance's Design Mode profile.

The daily instance was not rebuilt or restarted. Loading the change there requires a Rust and sidecar rebuild and a manual restart when losing its current sessions is acceptable.
