# Troubleshooting

This page collects the most common problems when using TUICommander. Start with the quick checks, then inspect the application logs if the problem persists.

## Quick checks

1. Confirm that the agent binary works outside TUICommander (`claude`, `codex`, `gemini`, etc.).
2. Confirm the repository is a valid Git checkout and that the selected directory is writable.
3. Check that the terminal is using the expected shell and PATH.
4. Restart the affected terminal tab before restarting the whole app; the PTY and other tabs can remain alive.
5. Check the application log endpoint:

```bash
curl 'http://localhost:9876/logs?limit=200'
```

A second debug/test instance normally uses the next available port, such as `9877`.

## The agent is shown as idle or busy incorrectly

Agent state is inferred from terminal output and, for supported agents, can also be driven by native hooks. Check the following:

- the correct agent type is selected or detected;
- the agent is not waiting for a hidden confirmation prompt;
- native hooks are enabled under Settings > Agents when available;
- the terminal is not displaying a full-screen TUI whose state cannot be inferred reliably;
- the agent has not changed its prompt format in a newer release.

If only one agent is affected, capture the terminal output and check the relevant agent page under `docs/architecture/agents/` before reporting a regression.

## A session was not restored

Session restore is lazy and branch-scoped. Select the repository and branch that owned the session first. Agent sessions may show a resume banner instead of executing a resume command automatically; activate the banner to continue.

Plain shell tabs are intentionally not restored as live processes after an application restart. Create a new shell tab instead.

## A worktree is missing or the branch is not visible

Check the repository's worktree list from a shell:

```bash
git worktree list
```

A worktree involved in a rebase, merge, cherry-pick, revert, or bisect should not be removed automatically. If the directory was deleted externally, refresh the repository state and check the application log for the removal reason.

## GitHub, PR, or CI data is stale

- Verify that `gh auth status` succeeds for the intended account.
- Check the repository remote and account binding in Settings.
- Use Fetch or refresh the GitHub panel.
- Check whether the provider is rate-limited or temporarily unavailable.
- For multiple GitHub accounts, confirm that the repository is bound to the correct account.

## Browser mode does not show a feature

Browser mode intentionally lacks some desktop integrations. See [TUICommander modes](modes.md) for the complete distinction.

The Command Palette, native file picker, global hotkey, dictation, IDE launcher, updater, detached OS windows, and installation of user plugins from local files require the desktop app.

For a connection problem, verify that the backend is listening on the expected port and that the browser can reach it from the same network. Inspect the browser console and the backend logs before restarting.

## Clipboard or file opening behaves differently in a browser

Browser mode uses browser clipboard and file-opening APIs where native integrations are unavailable. Permissions, HTTPS requirements, popup blocking, and browser focus can affect the result. Retry the action from the desktop app to distinguish a browser limitation from a backend problem.

## Dictation cannot build or start on Windows

Whisper builds require CMake and libclang. This project currently expects LLVM 18 for the Windows `whisper-rs` bindings. Set `LIBCLANG_PATH` to the LLVM `bin` directory before building. See [Development Setup](../guides/development-setup.md#windows-prerequisites).

## MCP or OAuth problems

Check the upstream server status, authorization URL, and the application log. For a local MCP endpoint, verify that the client is using the current protocol/session configuration. A client that does not support tool-list refresh may need to reconnect after MCP configuration changes.

## Performance problems

Enable runtime diagnostics only while reproducing the problem:

```bash
curl -X POST http://localhost:9876/diagnostics \
  -H 'Content-Type: application/json' \
  -d '{"enabled":true}'

curl 'http://localhost:9876/logs?source=diagnostics'
```

Disable diagnostics after the investigation. See [Performance Profiling](../guides/profiling.md) for a deeper investigation workflow.

## What to include in a bug report

Include:

- TUICommander version and platform;
- desktop, browser, mobile, or remote-daemon mode;
- agent and agent version;
- repository/worktree context, without secrets;
- exact reproduction steps;
- relevant logs with tokens and credentials removed;
- whether the issue survives creating a fresh terminal tab.

## Community & support

Join [TUICommander & Co on Discord](https://discord.gg/4DQ7Ah6hSh) for questions and ideas. Use **#support**, **#ideas**, or **#plugins** in the **TUICommander** category. The server is checked once a day. Report reproducible bugs in [GitHub issues](https://github.com/sstraus/tuicommander/issues).
