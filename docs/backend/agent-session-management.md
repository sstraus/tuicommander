# Agent Session Management

These rules moved here from `AGENTS.md` to keep that file inside the
32,768-byte project-instruction budget. They stay binding for every agent;
`AGENTS.md` keeps a summary and a link to this page.

## Agent Session Management

TUIC tracks each agent's session ID for resume-after-restart. Two strategies coexist:

**Discovery-based (Claude, Gemini, Codex, Grok).** TUIC does NOT inject `--session-id` at launch — the agent creates its own ID. TUIC discovers the active session and re-checks it on every idle↔busy transition and every 30s poll, so an agent that starts a replacement session is picked up. Resume uses `agentSessionId` (disk-discovered), not `tuicSession`.

Discovery has two tiers, and the difference is not cosmetic:

| Tier | Agents | Source |
|---|---|---|
| **Exact** | Claude, grok | the agent's own pid→session registry: `$CLAUDE_CONFIG_DIR/sessions/<pid>.json`, `~/.grok/active_sessions.json`. `get_session_leaf_pid` returns the agent's pid (verified: it stays the agent even while a tool subprocess runs) |
| **Heuristic** | Gemini, Codex, and any Claude/grok too old to publish a registry | newest unclaimed session file under the project dir |

**The heuristic is not a binding, and no amount of tuning makes it one.** N agent tabs in one folder all scan the same directory, so whichever tab polls first takes the newest file regardless of whose it is; the rest take another tab's session or nothing. `claimed_ids` only stops two tabs holding the *same* id — it cannot tell whose is whose. That is issue #119: measured on a live instance, 3 of 6 Claude tabs held no id and one held a different tab's, so every tab resumed with `claude --continue` into the same conversation.

**Finding the id is only half of a resume — the other half is which store holds it.** A shell alias is expanded before `exec`, so a run config that reads `c2` with an empty `env` is not what runs: the process is `claude --dangerously-skip-permissions` under `CLAUDE_CONFIG_DIR=~/.claude-private`, and TUIC never sees the assignment. While the agent lives, discovery reads argv and env off the process and rebuilds the real command into `agentLaunchCommand`; at restore time the pid is gone and that string is the only record left. Do not re-derive the config dir from the run config: `c` and `c2` differ *only* in an env var neither one declares, so the default config verifies an id in `~/.claude` and then sends `--resume` to a binary that reads `~/.claude-private` — Claude answers `No conversation found with session ID`, and the transcript is sitting untouched in the other directory.

So when you add a discovery-based agent, look for a pid registry *first*. Codex 0.153 has none — `session_index.jsonl` carries only id/name/updated_at, the rollout `session_meta` has no pid, and there is no `--session-id` flag — so it stays heuristic on purpose, cwd-scoped by the rollout's recorded `cwd`. Gemini is worse and knowingly so: its scan visits every project's `chats/` dir, so it is not even cwd-scoped (see the `DEFERRED` note on `discover_gemini_session`). Do not close either gap by guessing a path-hashing scheme — verify against a real install.

**Forced injection (Goose).** Shell wrapper injects `--name $TUIC_SESSION` into `goose session/run` commands. The TUIC tab UUID IS the goose session name. Discovery returns `None` (SQLite storage, no filesystem scan). Resume uses `tuicSession`.

**No session tracking (Amp, Cursor, Droid, OpenCode, pi).** Either no local session files, cloud-only, or no UUID-based resume. `TUIC_SESSION` env var is available but unused.

When adding a new agent: choose discovery-based if the agent writes session files to disk (add `sessionDiscovery` to `agents.ts` and a Rust `discover_*_session` to `agent_session.rs`). Choose forced injection only when discovery is impossible (e.g., SQLite-only storage).

All of the above describes the **PTY** transport. The AI Chat `ego` uses ACP and is not an `AgentType`. Read SPEC.md → "PTY versus ACP routing" before wiring either transport: a session has exactly one transport, with no fallback.

`ego` has three distinct faces: standalone CLI outside TUIC; AI Chat over ACP as an orchestration peer with a host-issued, durable `TUIC_SESSION` but **no** tab, PTY or terminal parser; and a separately launched terminal CLI over PTY, which may have an `AgentType`. The ACP peer reaches terminals and repositories through TUIC's MCP bridge. Its peer identity permits mail and child-parent routing, but does not turn the AI Chat conversation into a terminal or place it in tab routing, split panes or PTY agent-state detection.
