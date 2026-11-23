# To test — agents and PTY, part 3

## Antigravity CLI (agy) replaces Gemini CLI (3103-eb8e) — Rust restart required

The `gemini` agent type is gone; settings stored under `gemini` are dropped, no migration. Every item below needs a rebuilt backend and an `agy` that finished its first-run onboarding (colour scheme, terms and data-use consent, which only the user may answer). As of 2026-11-22 the onboarding is done and agy is signed in; the screen states are recorded in `agy-1.5.10-*.tcap` and replayed by `agy_capture_tests.rs`.

- [ ] Start `agy` in a TUIC tab inside a throwaway repository. The launch must carry `--settings={"altScreenMode":"never"}` and the screen must stay on the main buffer (no alternate screen; scrollback works). The tab badge must read `agy` (process name `antigravity`), not a shell.
- [ ] Start `agy` in a directory that agy does not trust yet. The tab must show Input (not Idle) on `Do you trust the contents of this project?`, with the two options on mobile; Enter must clear it and the tab must become Idle at the empty composer. _(replayed from `agy-1.5.10-trust-turn.tcap`; live check after the Rust restart)_
- [ ] Start `agy --mode accept-edits` and ask it to run `date -u`. The tab must show Input on `Run this command? date -u` with four options, and return to Working then Idle after Enter. Record what a number key does on that picker. _(replayed from `agy-1.5.10-command-approval.tcap`)_
- [ ] Ask agy to use its question tool. The tab must show Input with the question title and options through arrow-key navigation, and clear on Enter. Record the write-in row and Escape. _(replayed from `agy-1.5.10-question.tcap`)_
- [ ] If you set an agy status-line command, check that Working and Idle still follow the turn; that layout is not recorded.
- [ ] Ask agy one short question. Working must show while it answers and idle after; with Settings → Agents → agy → Install hooks globally on, `~/.gemini/config/hooks.json` must hold a `tuic-status` key with `PreInvocation` and `Stop` entries, siblings untouched, and agy must not report a hook error for the `{}` answer. Turn the toggle off: only `tuic-status` disappears.
- [ ] Quit that tab and restore it. The tab must resume with `agy --conversation <id>` for the conversation it ran, and a second agy tab in the same project must not take the first tab's ID (known heuristic limit: record what happens).
- [ ] Confirm `ls ~/.gemini/antigravity-cli/conversations/` holds `<uuid>.db` for the conversation (recorded for headless turns; confirm for an interactive one) and that the ID equals the `conversationId` hook payload field (recorded for headless).
- [ ] `agent action=spawn agent_type=agy prompt=...` must start `agy -i <prompt>`; record whether `agy` accepts a bare positional prompt and whether `--add-dir=<path>` (glued) is parsed.
- [ ] In Settings → Agents → agy, Install MCP bridge: `~/.gemini/config/mcp_config.json` must gain `mcpServers.tuicommander`, and agy must list the server.
- [ ] Upload a file to an agy session: agy must read it without a trust prompt, or record the prompt.
