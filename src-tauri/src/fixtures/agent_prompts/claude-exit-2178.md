# Claude exit and mobile log projection (2178-24da)

Recorded on 2026-10-24 from the isolated `agent-test-1986` instance at
`05ec6346d08f362a2a7881bfc25906ff9227dbcd`. PTY geometry: 41 rows, 128 columns.
Claude Code identified itself as v2.1.308. No model request was made.

These `.raw` files contain the complete output returned by the task-owned
session's raw-output HTTP endpoint. They preserve output bytes, but not PTY
read boundaries, input events, or timing. Replay them only as log-projection
fixtures at the recorded geometry, not as agent-state lifecycle fixtures.

- `claude-exit-normal-2178.raw`: onboarding, handled application exit after
  Ctrl-C input and the selected `No, exit` screen, shell status 1, then
  `SHELL_NORMAL_2178` and `shell2178$`.
- `claude-exit-sigterm-2178.raw`: onboarding, SIGTERM sent to the recorded
  task-owned Claude child, shell status 143, then `SHELL_SIGTERM_2178` and
  `shell2178$`.

In both live sessions, `/sessions` had released the agent identity and canonical
`format=text` contained the marker. `format=log` omitted it and retained the
agent's selection as `input_line` (`No, exit` or `✔ Dark mode`). No visibility
change or standby setup was used. Mobile DOM snapshots at 390x844 also omitted
the markers. Full HTTP snapshots and screenshots are retained under
`~/Gits/.tmp/results/2178/`.

The regression exercises the real HTTP log handler. The oracle is executed
shell output and its literal prompt, not another projection's computed cutoff.
