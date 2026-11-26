# Workflow E2E (layer L2)

A black-box runner drives a running isolated instance through its public
transports. It never links the application code and never touches the
orchestrator instance (`:9876`, `:14319`) or any real session.

## Run it

```bash
make agent-test                  # foreground; builds once, then serves
make workflow-e2e                # in another shell; all scenarios
make workflow-e2e WF_E2E_ARGS="2 5"   # scenarios whose name starts with 2 or 5
make agent-test-stop             # at the end
```

The runner exits 0 when all scenarios pass, 1 on a failure, and 2 when no
instance answers. It prints `FINDING` lines for observations that are not
failures.

## Why a Node script

- It needs no Rust build: the instance already holds the code under test.
- It keeps the test black-box: only the wire contract is visible.
- Node is already a repository tool, and the runner has no dependencies.
  A `#[ignore]` Rust test would link the code it checks and add a cold build.

## Transport

The isolated instance listens on TCP `127.0.0.1:19876`, but that port needs
Basic auth, and `make agent-test` configures none. Agents use the instance
Unix socket (`scripts/agent-test.py` field `connect`). The runner reads the
socket path from
`~/Library/Application Support/com.tuic.commander/instances/<instance>/mcp-socket`.
Override with `WF_E2E_SOCKET` or `AGENT_TEST_INSTANCE`. The runner refuses a
socket path that names the orchestrator.

- HTTP: `POST /workflows/neutral/action?path=<project>` (caller is Owner).
- MCP: `POST /mcp`, tool `workflow` `{action:"neutral",input}`. The caller is
  the managed Agent bound to a throwaway PTY whose cwd is the test repository.
  The repository needs a repo watcher (`POST /watchers/repo`).

## Scenarios

Each scenario uses its own git repository under `~/Gits/.tmp/wf-e2e/`, removed
afterwards with its PTY and watcher.

| # | Checks | Bug it catches |
|---|--------|----------------|
| 1 | Save script operation, create draft, validate, publish, start (no story, no Git requirement), inspect to `completed`; false branch of the condition; retried start returns the same run | A run that needs a story or Git; a retry that starts a second run |
| 2 | Exact approval: stale sequence refused, exact answer accepted, exactly one effect; a duplicate answer adds none; a declined approval fails the run without the effect | A refused answer that still runs the effect; a double effect |
| 3 | MCP and HTTP: same shapes for create, validate and publish; byte-identical `get_publication`, `list_drafts`, `inspect`, `events`, paged `inspect`; same refusal text | Transport drift in a reply field or a refusal |
| 4 | Cancel while waiting: `cancelled`, history kept and extended, no effect, a late answer cannot revive the run | Lost history; an effect after cancel |
| 5 | Publish revision 2 while a run waits: the run keeps revision 1 and its effect; a new run pins revision 2 | A draft edit that changes a running workflow |
| 6 | Invalid definition: diagnosis with path, code and message; publish refused and no publication left; unknown request fields refused | A broken graph that publishes |
| 7 | Lost-response retries over HTTP: same `start` returns the same run; identical `answer` and `cancel` sent twice leave one event and one effect | A retry that starts a second run or repeats the effect |
| 8 | MCP: invalid validate, stale publish refused, valid publish, identical re-publish refused, no revision 2 | Non-idempotent publish over MCP |
| 9 | Scheduled binding: save (disabled), enable, the cron tick starts a run announced on `/events`, the scheduled text reaches the approval, answer completes it, binding disabled | A binding that never fires, or loses the scheduled text |

## Known limits

- MCP `start` needs a backend capability binding that only an agent launched
  by TUIC has. A bare registered peer is refused (`native caller has no backend
  capability binding`). The runner asserts this refusal and starts through HTTP.
- A binding over a deleted project stays enabled and makes every later schedule
  tick fail (`Neutral scheduled trigger admission failed`, `No such file or
  directory`), so scenario 9 disables its binding in cleanup.
- The GitHub triage template cannot publish on the isolated instance: the Designer
  reports `/definition/instances · missing descriptor model:request@1` because no
  model executable or profile is configured. GitHub fixture replay needs a polled
  baseline and an account, so it stays in layer L3.
- The model, Jira, GitLab and Telegram connectors are out of scope (layer L3).

## Designer smoke (manual, agent-browser)

The web UI of the instance is on the same TCP port, which needs Basic auth. Run
`node e2e/workflows/socket-proxy.mjs 19890` to expose the Unix socket on
loopback, then open `http://127.0.0.1:19890/` through the stealth wrapper
(`AB_SESSION=wf-e2e ab-stealth.sh <url>`). Click only; never synthesize keys.

1. Put a throwaway repository first in the sidebar: `PUT /config/repositories`
   with a delta `{mutationVersion:1, repos:[{id,before:null,after}], repoOrder,
   activeRepoPath}`, and `POST /watchers/repo?path=`. Save the `calc` script
   operation with `save_operation`. Web mode has no folder picker.
2. Set the viewport to 1400 x 900 (`set viewport`). At 720 px the dialog stays
   usable: toasts sit at the bottom left while a dialog is open.
3. Plans and Stories, then **Workflows**: add `calc` and `approval`; select each
   node, set its target, execution policy and a constant input; name it, then
   **Save neutral draft**, **Validate links and inputs**, **Publish neutral
   draft**.
4. The Designer has no start control. Start the run with HTTP `start`, paste the
   run ID in **Run ID** and click **Inspect retained run**.
5. Undo the repository delta and the watcher, and stop the proxy.

Evidence of the 2026-11-15 run: `~/Gits/.tmp/wf-e2e/s-designer-*.png`. The UI
authored graph `calc(7) -> approval -> end` published as revision 1; the
inspection showed `calc.next = 8` and the run waiting at the approval node.
