# Antigravity CLI (agy) — Reference

Agent-specific reference for the Antigravity CLI (`agy`, Google). It replaces the
Gemini CLI agent type. See [agent-ui-analysis.md](../agent-ui-analysis.md) for shared concepts.

**Installed version**: 1.5.10 (`/opt/homebrew/bin/agy`, a wrapper that execs the `antigravity` binary)

## Evidence status

| Surface | Status |
|---|---|
| Flags and subcommands | Verified from `agy --help` on 1.5.10 |
| Hook events and payloads | Recorded 2026-11-21 from a headless turn (`SessionStart`, `PreInvocation`, `PostInvocation`, `PreToolUse`, `PostToolUse`, `Stop`); contract also in the embedded guide |
| MCP and rules files | Taken from the guides embedded in the binary |
| Conversation store layout | Recorded: `conversations/<uuid>.db` (SQLite), `presence/<uuid>.lock` |
| Headless resume | Recorded: `agy -p ... --conversation <id>` and `-c` resume the same conversation; `--output-format json` returns `conversation_id` |
| Screen layout, working/idle/awaiting captures | **Not recorded.** The first interactive launch shows a one-time onboarding (colour scheme, terms and data-use consent) that only the user may answer. Detection is UNVERIFIED |

Record the missing captures with `scripts/agent_capture` from a throwaway directory once a
sign-in exists, then add the fixtures to `src-tauri/src/fixtures/agent_prompts/`.

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
| `config/config.json` | settings |

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
