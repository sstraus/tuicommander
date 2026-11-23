# Antigravity CLI (agy) — Reference

Agent-specific reference for the Antigravity CLI (`agy`, Google). It replaces the
Gemini CLI agent type. See [agent-ui-analysis.md](../agent-ui-analysis.md) for shared concepts.

**Installed version**: 1.5.10 (`/opt/homebrew/bin/agy`, a wrapper that execs the `antigravity` binary)

## Evidence status

| Surface | Status |
|---|---|
| Flags and subcommands | Verified from `agy --help` on 1.5.10 (re-checked 2026-11-22): `-i`, `-p`, `--conversation`, `--continue`/`-c`, `--add-dir`, `--settings`, `--model`, `--effort`, `--mode` all exist |
| Hook events and payloads | Recorded 2026-11-21 from a headless turn (`SessionStart`, `PreInvocation`, `PostInvocation`, `PreToolUse`, `PostToolUse`, `Stop`); contract also in the embedded guide. A live `hooks.json` written by another integration on 2026-11-22 has the same shape (named top-level section, flat `PreInvocation`/`PostInvocation`/`Stop`, grouped `PreToolUse`/`PostToolUse`) |
| MCP file | Recorded 2026-11-22: `HOME=<throwaway> agy mcp add probe echo hi` writes `<home>/.gemini/config/mcp_config.json` with `{"mcpServers":{"probe":{"command":"echo","args":["hi"],"disabled":false}}}` |
| Conversation store layout | Recorded: `conversations/<uuid>.db` (SQLite), `presence/<uuid>.lock`. The lock files are empty (0 bytes, checked 2026-11-22): they hold no pid, so agy has no pid registry |
| Headless resume | Recorded 2026-11-21 and again 2026-11-22: `agy -p ... --output-format json` returns `conversation_id`; `agy -p ... --conversation <id>` and `agy -p ... -c` continue that conversation (same `conversation_id`, prior reply recalled, `num_turns` grows) |
| Fork | **No headless fork.** `--help` lists no fork flag or subcommand. Forking exists only inside an interactive session (`/fork`, named in the changelog's "already open in another CLI instance" banner entry) |
| Screen layout, working/idle/awaiting captures | Recorded 2026-11-22 on 1.5.10 after the user finished the first-run onboarding: four `.tcap` files, described in `src-tauri/src/fixtures/agent_prompts/agy-1.5.10.md` and replayed by `src-tauri/src/pty/agy_capture_tests.rs` |

## Screen states

- **Ready:** a `>` composer row between two full-width rules, and no `esc to cancel` hint below it.
- **Working:** the hint row below the composer reads `esc to cancel`. The composer stays visible
  for the whole turn, so the prompt alone never means ready.
- **Awaiting:** three pickers, each with its own footer at column 2: workspace trust
  (`enter Confirm`), command approval (`tab Amend`), native question (`enter Select · esc Skip`).
  TUIC publishes them as a navigate-then-Enter choice prompt. Number keys are not recorded.
- A new directory shows the workspace-trust picker before the composer. Trusting it adds the
  path to `trustedWorkspaces` in `~/.gemini/antigravity-cli/settings.json`.
- The default mode `auto` approves safe commands without a picker; `--mode accept-edits`
  asks for each command.
- Not recorded: a user status-line command, wrapped option labels, the write-in row.

## Identity and launch

- Detection: the foreground process is named `antigravity` (the `agy` wrapper execs it); both names classify as `agy`.
- Interactive prompt: `agy -i "<prompt>"` (`--prompt-interactive`); headless: `agy -p "<prompt>"` (`--print`, with `--output-format text|json|stream-json`).
- Resume: `agy --conversation <id>` (exact) and `agy --continue` / `-c` (most recent). There is no `--session-id` and no fork flag in `--help`.
- Model and effort: `--model <name>`, `--effort low|medium|high|xhigh|max`; mode: `--mode accept-edits|auto|plan`.
- Extra directories: `--add-dir <path>` (repeatable). TUIC passes the session upload directory this way.
- Per-session settings: `--settings <json|file>`.
- Alternate screen: agy enters it by default (`?1049h`, recorded). TUIC launches it with `--settings={"altScreenMode":"never"}` (values `never`/`always`; recorded: `never` emits no `?1049h`), unless the user passes their own `--settings` or turns Prevent alternate screen off.
- TUIC instructions: agy has no system-prompt flag. TUIC does not inject launch instructions for agy yet (the previous Gemini agent had none either).

## Storage

Everything lives under `~/.gemini/`:

| Path | Content |
|---|---|
| `antigravity-cli/conversations/<uuid>.db` | one SQLite file per conversation; the stem is the ID for `--conversation` |
| `antigravity-cli/presence/<uuid>.lock` | empty per-conversation file; liveness semantics unverified |
| `antigravity-cli/brain/<uuid>/` | artifacts and `.system_generated/logs/transcript_full.jsonl` |
| `antigravity-cli/implicit/<uuid>.pb`, `brain/`, `log/`, `conversation_summaries.db` | internal state |
| `config/hooks.json` | lifecycle hooks (top-level key = hook name) |
| `config/mcp_config.json` | MCP servers (`mcpServers` map) |
| `config/config.json` | plugin switches (for example `plugins.swarm.enabled`) |
| `antigravity-cli/settings.json` | CLI settings, including the onboarding completion and the colour scheme (per the embedded guide; absent until onboarding finishes) |

Session discovery is newest-unclaimed over `conversations/*.db` within five minutes. The files
name no project or pid, so it is a heuristic, not a binding. Every hook payload carries
`conversationId` and `cwd` (recorded); forwarding it through the hook channel is the exact path (DEFERRED).

## Hooks

`hooks.json` is an object whose keys are hook names. TUIC owns only the key `tuic-status`
and never edits sibling keys. Events: `SessionStart`, `PreToolUse`, `PostToolUse`
(grouped: `matcher` + `hooks[]`), `PreInvocation`, `PostInvocation`, `Stop` (flat handler lists).
Handlers receive JSON on stdin (`conversationId`, `cwd`, `hookEventName`, …) and must answer JSON on
stdout. TUIC registers `PreInvocation` (busy) and `Stop` (idle) and answers `{}` (recorded: the turn proceeds).
**Never answer `{}` to `PreToolUse`: a recorded run shows the tool call denied** ("tool call denied by pre-tool hook"); a `PreToolUse` handler must answer an explicit `decision`.
agy has no permission or question event, so awaiting comes from the screen adapter.

## MCP

Global file `~/.gemini/config/mcp_config.json`, key `mcpServers`, stdio (`command`, `args`, `env`) or
Streamable HTTP (`url`). It is a dedicated file, so TUIC may auto-install the bridge there.

## Quota

No quota source has been investigated for agy. The earlier Gemini CLI finding (no stable
machine-readable account quota interface) is recorded in
[quota-source-spikes.md](../../backend/quota-source-spikes.md); agy stays terminal-detection only.
