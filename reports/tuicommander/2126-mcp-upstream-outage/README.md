# MCP upstream outage investigation — 2126-8000

## Outcome

**Recommend closing the native-outage claim as not a defect.** The supplied
October 22 deltas remove upstream tools only. Both historical October 19/20
core-native removal deltas match application startup with a changed bridge
binary and bridge initialization. No unexplained core-native removal was found
in the requested record. No production fix or failing regression test is justified.

On unchanged source `2c90bcf60a53b17eb6094e56c80094292b54d43e`, a real
`tuic-bridge` connected to an isolated desktop instance retained native tools
while an owned fake HTTP upstream stopped, opened its circuit and recovered.
Only `fake2126__echo` left the list. The bridge also forwarded
`notifications/tools/list_changed` during these transitions.

The coordinator subsequently supplied the exact Claude Code transcript. Its
structured deltas disprove the claimed native-tool loss in the supplied samples:
all nine TUIC removal deltas on October 22 remove only upstream tools. See
[Client transcript findings](#client-transcript-findings). This does not rule out
a different native outage outside those samples.

## Contract and scope

- Consumers must retain native discovery and calls during upstream outages.
- Native definitions come from `filtered_native_tools`; upstream definitions
  come from the registry's independent ready-entry snapshot.
- The owned fake implements the server contract for this experiment. It is not
  a fixture of Slack, Quill or another external service.
- Native macOS evidence only. No rb test, RED test, GREEN suite, installed binary
  replacement, daily-instance mutation or production edit was performed.

## Live experiment

Launch, from this linked worktree:

```sh
make agent-test AGENT_TEST_INSTANCE=agent-test-2126 AGENT_TEST_PORT=19889 AGENT_TEST_DRY_RUN=1
make agent-test AGENT_TEST_INSTANCE=agent-test-2126 AGENT_TEST_PORT=19889
```

The target builds through build-slot and the mbx Cargo shim. `mbx doctor`
reported zero failures and warnings. The instance has its own configuration,
binary and Unix socket. The probe used HTTP over **that socket**, not the daily
instance or an unauthenticated TCP request.

The probe started an owned Python HTTP server on `127.0.0.1:64834`, added it via
`PUT /mcp/upstreams`, and connected the built bridge with both
`--instance agent-test-2126` and an explicit `TUIC_SOCKET` pointing at the
isolated socket. It initialized as `claude-code`, then read and called tools
through the real bridge stdio protocol.

| UTC timestamp | Observation |
| --- | --- |
| 10:46:23.493352 | Healthy tools/list: nine native tools plus `fake2126__echo` |
| 10:46:23.671329 | Owned fake listener stopped and socket closed |
| 10:46:23.672142–10:46:23.673490 | Three calls to the fake returned `isError: true`, with `Upstream 'fake2126' request failed` |
| 10:46:23.674508 | `/mcp/upstream-status`: `circuit_open`, three calls and three errors |
| 10:46:23.678546 | tools/list: the same nine native tools, no fake tool |
| 10:46:23.679008 | Bridge stdout: `notifications/tools/list_changed` |
| 10:46:23.682520 | Owned fake listener restarted on the same port |
| 10:46:23.744170 | After explicit upstream reconnect, tools/list again included the fake tool |

The nine native names were `agent`, `automations`, `plugin_dev_guide`, `progress`,
`repo`, `secret`, `session`, `ui` and `workflow`. The probe checked that `agent`,
`ui`, `session`, `repo` and `progress` stayed listed at all three snapshots.
It also successfully called `agent list_peers`, `session list` and `repo list`
in each phase. It did not call `ui` or `progress`.

This first experiment induced the circuit through failed tool calls. Recovery
used the reconnect endpoint. Its snapshots and notifications are in
`observations.json`.

### Periodic health-check experiment

A second finite run used the same isolated target, real bridge and owned fake.
It made **no upstream tool calls** and did not request reconnect after the
outage. It waited for the production periodic health checker to open the circuit
and recover it automatically.

| UTC timestamp | Observation |
| --- | --- |
| 11:04:26.863452 | Healthy tools/list: nine native tools and the fake tool |
| 11:04:27.038407 | Fake listener stopped |
| 11:05:24.877341 | Application log: `Health check failed — circuit opening`, fake2126 |
| 11:05:24.957901 | HTTP status: `circuit_open`, zero upstream tool calls |
| 11:05:24.958323 | Bridge stdout: `notifications/tools/list_changed` |
| 11:05:24.965092 | tools/list retains all nine native names; only fake tool absent |
| 11:05:24.971322 | Fake listener restarted |
| 11:06:24.886730 | Application log: `Recovered (Ready) with 1 tools` |
| 11:06:25.649631 | Bridge stdout: another `notifications/tools/list_changed` |
| 11:06:25.683520 | tools/list contains all nine native names and fake tool again |

`agent list_peers`, `session list` and `repo list` again succeeded in every
phase. The bridge stayed connected across both health intervals; its stderr
contains startup and `connected to TUIC`, with no connection-lost or proxy
error. The finite run exited zero, removed the saved fake upstream and stopped
all owned processes. The observations are in `health-observations.json`; full
local logs and scripts use the `tuic-2126-health-` prefix under the same results
directory.

Both experiments used a stdio JSON-RPC probe rather than Claude Code itself.
Collapsed discovery and simultaneous outages of multiple upstreams were not
exercised.

## Source findings

- `src-tauri/src/mcp_proxy/registry.rs:478`: aggregation skips non-ready entries;
  it does not own the native definitions.
- `src-tauri/src/mcp_proxy/registry.rs:1274`: periodic health checks update the
  affected entry and publish the shared tools-changed signal.
- `src-tauri/src/mcp_http/mcp_transport_catalogue.rs:1014`: expanded discovery
  starts with filtered native definitions and extends them with upstream tools.
  Collapsed discovery takes a separate meta-tool branch.
- `src-tauri/src/mcp_http/mcp_transport.rs:1132`: tools/list performs a bounded
  initial settle, then serializes the merged catalogue without awaiting an
  upstream network request.
- `src-tauri/src/mcp_http/mcp_transport.rs:1430`: upstream changes cause a search
  index refresh and list_changed notification, not an explicit disconnect.
- `src-tauri/crates/tuic-bridge/src/main.rs:985`: bridge disconnect detection
  uses failed local MCP pings, not upstream health status. The offline response
  at line 665 contains an empty tool list; this is a separate possible loss layer.

## Daily-log correlation is not causation

Read-only source: `logs/tuic.log.2026-10-22` in the daily application's config
folder. The following observations narrow, but do not establish, the cause:

| UTC timestamp | Daily-log evidence |
| --- | --- |
| 08:42:36.635031 | `Sleep-wake detected`, `gap_secs=285` |
| 08:42:36.658548–08:42:36.658585 | Circuits open for openrouter-key-manager, quill and gh-metrics |
| 08:59:24.822617 | `Sleep-wake detected`, `gap_secs=1000` |
| 08:59:26.298034–08:59:27.978555 | Circuits open for spinach, slack, outlook-mail, outlook-calendar and publishwith-ai-mcp |
| 09:00:08.809789–09:01:09.223084 | Recorded upstream recoveries |
| 09:17:08.634471–09:17:08.651192 | Another circuit cluster, with continuous repo-watcher and standby logging before and after it |
| 09:18:10.575515–09:18:11.475007 | outlook-calendar, slack, spinach and outlook-mail recover |

The first two clusters coincide with long process timer gaps. The third does
not have the same local evidence. A sleep-only explanation would overstate the
record. None of these log lines records the downstream tools/list result or
Claude Code's tool registry.

## Setup errors and cleanup

The first bridge probe omitted the isolated socket override. It used the system
temp directory, could not connect, and returned an empty offline list **before
any upstream outage**. That is invalid reproduction evidence, not a defect in
upstream aggregation. The corrected run used the explicit isolated socket.

The initial background command wrapped the non-exiting desktop supervisor.
It could not emit BG DONE until stopped, which delayed continuation. The second
run used a finite supervisor/probe/cleanup wrapper. Its exit code was zero.
The second wrapper issued a redundant stop after the probe had already stopped
the instance; this printed `no running agent-test supervisor`, not a leaked
process or application failure.

The bridge exited, the fake server closed, the saved fake-upstream configuration
was removed, and `make agent-test-stop` stopped the isolated desktop instance.
Its PID file was absent at handoff. The daily instance was not restarted.

Full local evidence is under `~/Gits/.tmp/results/`:
`tuic-2126-probe.jsonl`, `tuic-2126-bridge.stderr`,
`tuic-2126-agent-test.log`, `tuic-2126-agent-test-relaunch.log` and
`tuic-2126-live.log`. The diagnostic scripts are retained there as
`tuic-2126-probe.py` and `tuic-2126-run-live.py`.

## Client transcript findings

The coordinator supplied `~/Gits/.tmp/results/coord-mcp-drops-20261022.md`, which
names this source transcript:

```text
~/.claude-private/projects/-Users-stefano-straus-Gits-personal-orchestrator/a06d7386-5100-42fb-b2d3-d56bdda6adcd.jsonl
```

Read the structured `attachment.removedNames`, **not** the rendered wildcard
summary. The client writes the attachment when building its next request, so
these timestamps are upper bounds for the catalogue changes.

| Client attachment UTC | Removed tools | Native tools removed |
| --- | --- | --- |
| 05:26:53.781 | openrouter-key-manager 53, quill 6 | None |
| 05:27:12.498 | outlook-calendar 11, outlook-mail 21, publishwith-ai-mcp 1, slack 27, spinach 5 | None |
| 09:00:02.749 | All eight upstream namespaces, 177 total | None |
| 09:03:31.201 | quill 6 | None |
| 09:44:03.583 | gh-metrics 53 | None |
| 09:49:01.978 | openrouter-key-manager 53, quill 6 | None |
| 10:35:22.527 | gh-metrics 53 | None |
| 10:52:25.409 | gh-metrics 53 | None |
| 10:59:34.601 | gh-metrics 53 | None |

In particular, the last three samples render this misleading text:

```text
53 deferred tools are no longer available (MCP server disconnected): mcp__tuicommander__* (53).
```

But every removed name has the narrower prefix
`mcp__tuicommander__gh-metrics__`. No `agent`, `ui`, `session`, `repo` or
`progress` removal appears in these deltas. The complete October 22 TUIC
removal arrays and rendered notices are retained in `client-deltas.json`.
The supplied claim that these samples removed native tools is contradicted by
their own source. This is consistent with normal upstream filtering and the
bridge's list_changed notifications, not evidence of a TUIC disconnect.

## Historical native-removal classification

The full transcript scan found four deltas with unnamespaced native names.
Only two contain the core names in the revised October 19/20 criterion.
`historical-native-deltas.json` retains the exact removal arrays, transcript
line numbers, selected daily-log lines and limits. The active transcript had
108 TUIC removal deltas at extraction; that total can grow.

| Client attachment UTC | Native removal | Classification and daily-log evidence |
| --- | --- | --- |
| October 19, 06:51:58.737 | 17 native names, including agent/ui/session/repo | App restart with changed bridge binary and bridge initialization. October 19 lines 6811–6825 rewrite bridge digest 9ce288… to f40761… at 06:51:27Z; lines 6837/6840 start the HTTP lifecycle (`first_start=true`) and socket at 06:51:28Z; lines 7051/7052 initialize the bridge and Claude at 06:51:57Z. |
| October 19, 15:47:16.513 | remote only | Explained runtime availability gate, outside the core-name criterion. Lines 26215–26361 show remote connection failures at 15:46–15:47Z, including DNS/HTTP 403 and SSH authentication failures. The native gate hides remote when no server is connected. |
| October 20, 18:34:48.905 | 18 native names, including agent/ui/session/repo | App restart with changed bridge binary and bridge initialization. October 20 lines 18604–18618 rewrite bridge digest 909db1… to efb199… at 18:34:21Z; lines 18624/18627 start the HTTP lifecycle (`first_start=true`) and socket; line 18853 initializes a fresh bridge at 18:34:46Z for the coordinator TUIC session. |
| October 21, 18:03:10.713 | remote only | Outside the requested October 19/20 dates and core-name set; not attributed by this investigation. |

The native `remote` gate is explicit in
`src-tauri/src/mcp_http/mcp_transport_catalogue.rs:915`. Git blame dates its
introduction to commit `c7cbdad3e3`, October 19 at 12:45:52Z, before the
remote-only removal. That event is not an agent/ui/session/repo disconnect.

For both core-native removals, startup and changed bridge digests establish
restart/binary replacement, not the reason the preceding process exited.
Manual restart versus crash remains unknown. The record does not prove the
exact instant of tool loss or the intermediate tools/list response. It does
place both client deltas in an app/bridge restart window, rather than an
otherwise running server's isolated upstream circuit transition.

No unexplained core-native removal remains in the requested October 19/20
record. Close the original native-outage claim as not a defect; do not create
a RED test for a failure that the evidence does not show. Do not generalize
this conclusion to every historical disconnect or a future native-loss trace.

## Upstream discovery policy recommendation

Recommend retaining last-known upstream schemas during a **temporary circuit
opening**, with a clear unavailable/retrying error on calls. This would make
discovery more stable for clients and avoid catalogue churn during short
outages. It is a separate product behavior change, not a fix for native loss.
The current behavior correctly follows its ready-only discovery contract.

Implement retention only after approval. Its contract must distinguish a
transient circuit from explicit disable/delete, missing authentication and
permanent configuration failure; define cache lifetime and schema replacement
on recovery; and test discovery/search/dispatch together. Do not suppress
list_changed globally or retain deleted/disabled tools to mask Claude's
misleading wildcard notice. No policy change is implemented here.

## Verification limits

The two live bridge probes are the targeted behavioral checks, both exit zero;
no Rust unit suite, rb run, mutation run or production-code change is claimed.
Live calls cover agent/session/repo, not ui/progress. Code inspection of the
shared discovery/dispatch gate at catalogue lines 902–920 shows no upstream
health dependency for ui/progress; it does not substitute for a claim that
their UI side effects were live exercised. Collapsed discovery and simultaneous
multi-upstream outages remain untested. All owned probe processes were stopped.
