# Discovery E2E (model-driven)

Real agents solve TUICommander tasks through lazy discovery (`search_tools`,
`get_tool_schema`, `call_tool`) against a running isolated instance. An oracle
checks every outcome from outside, through the instance's own API and the file
system. The test answers one question for a change to the discovery surface:
do agents still find the right tool, understand it, and get there without extra
iterations or failed calls?

It spends model tokens: every run is a real `ego run` or `claude -p` session.

## Run it

```bash
# In the worktree under test (the bridge is taken from its .tmp/agent-test/<instance>/).
make agent-test AGENT_TEST_INSTANCE=discovery-b AGENT_TEST_PORT=19882
AGENT_TEST_INSTANCE=discovery-b make discovery-e2e
make discovery-e2e DISCOVERY_E2E_ARGS="mail output"   # only these tasks
make agent-test-stop AGENT_TEST_INSTANCE=discovery-b AGENT_TEST_PORT=19882
```

Environment: `DISCOVERY_E2E_EGO` (ego binary, default `ego` on PATH), `DISCOVERY_E2E_AGENTS` (default `ego,claude`), `DISCOVERY_E2E_REPS`
(default 2), `DISCOVERY_E2E_MAX_TURNS` (default 30), `DISCOVERY_E2E_BRIDGE`,
`DISCOVERY_E2E_OUT`. The runner enables `collapse_tools` on the isolated instance
so that Claude Code also uses lazy discovery; ego always does. Each agent gets only
the instance's MCP server: ego runs `--bare --no-hooks --tools read`, Claude Code
runs `--tools "" --strict-mcp-config --setting-sources project`.

## Tasks and oracles

| Task | Agent is asked to | Oracle |
|---|---|---|
| `mail` | send a message to a named peer | the peer's inbox holds it |
| `output` | read a named terminal and return a printed secret | the answer contains it |
| `input` | run a shell command in a named terminal | the file it writes exists |
| `worktree` | create a worktree on a new branch | `git worktree list` shows the branch |
| `runread` | run a command in a named terminal and report its output | the answer contains it |
| `recipient` | message a peer by a partial name | the peer's inbox holds it |
| `rename` | rename a named terminal | `session list` shows the new name |
| `count` | report the number of open terminals | equals `session list` |
| `settings` | report a repository setting | equals `repo get_settings` |

Every run starts from fresh fixtures: a throwaway repository, a named terminal
that printed a secret, and a registered listener peer.

## A/B comparison

Build the baseline and the candidate in two linked worktrees, run the E2E on each
instance, then compare:

```bash
node e2e/discovery/compare.mjs ~/Gits/.tmp/discovery-e2e/discovery-a.json ~/Gits/.tmp/discovery-e2e/discovery-b.json
```

The candidate fails, per agent and per task, when its success rate is lower than
the baseline's, when a task is never solved, or when mean discovery hops, tool
calls or failed calls rise by more than `DISCOVERY_E2E_SLACK` (default 0.5 per
run). Tokens and cost are reported, not gated. Transcripts stay in the work
directory the runner prints.
