# Empty plan on a new ACP session

Captured 2026-10-31 from the installed `ego 0.4.0`, with `ego acp --bare`,
an isolated session store under this worktree's `.tmp/acp-plan-capture/sessions`,
and no MCP servers. The client sent ACP v1 `initialize`, then `session/new`.
The fixture is the unmodified `params.update` from the emitted `session/update`.
No prompt or paid generation was sent. The capture closed stdin and waited for
its own process to exit. Full frames remain in `.tmp/acp-plan-capture/frames.json`.

The empty list is a complete plan snapshot, not a plan step or a request to draw
an empty card. The existing projection already replaces the preceding list.
