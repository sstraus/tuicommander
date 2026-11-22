# To test — agents and PTY, part 3

## Antigravity CLI (agy) replaces Gemini CLI (3103-eb8e) — Rust restart required

The `gemini` agent type is gone; settings stored under `gemini` are dropped, no migration. Every item below needs a rebuilt backend and an `agy` that finished its first-run onboarding (colour scheme, terms and data-use consent, which only the user may answer).

- [ ] Start `agy` in a TUIC tab inside a throwaway repository. The launch must carry `--settings={"altScreenMode":"never"}` and the screen must stay on the main buffer (no alternate screen; scrollback works). The tab badge must read `agy` (process name `antigravity`), not a shell. Record `.tcap` captures (`POST localhost:9876/diagnostics/capture`) of idle, a working turn, a tool-permission prompt and a question, and add them as fixtures with `Awaiting-signal fixtures` cases. Until then `detect_agy_screen_activity` is UNVERIFIED.
- [ ] Ask agy one short question. Working must show while it answers and idle after; with Settings → Agents → agy → Install hooks globally on, `~/.gemini/config/hooks.json` must hold a `tuic-status` key with `PreInvocation` and `Stop` entries, siblings untouched, and agy must not report a hook error for the `{}` answer. Turn the toggle off: only `tuic-status` disappears.
- [ ] Quit that tab and restore it. The tab must resume with `agy --conversation <id>` for the conversation it ran, and a second agy tab in the same project must not take the first tab's ID (known heuristic limit: record what happens).
- [ ] Confirm `ls ~/.gemini/antigravity-cli/conversations/` holds `<uuid>.db` for the conversation (recorded for headless turns; confirm for an interactive one) and that the ID equals the `conversationId` hook payload field (recorded for headless).
- [ ] `agent action=spawn agent_type=agy prompt=...` must start `agy -i <prompt>`; record whether `agy` accepts a bare positional prompt and whether `--add-dir=<path>` (glued) is parsed.
- [ ] In Settings → Agents → agy, Install MCP bridge: `~/.gemini/config/mcp_config.json` must gain `mcpServers.tuicommander`, and agy must list the server.
- [ ] Upload a file to an agy session: agy must read it without a trust prompt, or record the prompt.
