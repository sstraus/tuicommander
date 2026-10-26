# Ego stopped-turn verification — story 2317-ccaf

## Cause and scope

The cause was in ego, not TUIC's stopped-line parser. In ego commit
`870b20c85e`, `Announce::signal` emitted OSC 7770 `state=awaiting` when a
completed turn had a provider-failure notice. TUIC correctly maps that native
marker to a confident question. Ego merge `9e3df1b160595196f35d7a410cee56dfc8c619d3`
(story 634-bcab) changes this completion to `state=idle`. Failed progress and
the failure notification remain separate signals. Real permission requests
still emit awaiting.

No TUIC production code or heuristics change in this verification. The new
Rust tests and recorded fixtures protect this protocol boundary.

The original report quoted a stopped receipt at 15:43. Its raw PTY stream was
not recorded. The committed fixtures are new recordings from installed ego
0.1.5; they do not claim to reconstruct the original stream.

## Native isolated-host evidence

- Date: 2026-10-25.
- TUIC worktree base: `8b950f764a80b7fbbcde8be068d46051871d0e7c`.
- Launch: `make agent-test AGENT_TEST_INSTANCE=agent-test-2317 AGENT_TEST_PORT=19877`.
- A dry run preceded launch. The named instance built through the native mbx
  shim and build-slot scheduler. The daily application was not used.
- Requests used the instance's Unix socket under
  `~/Gits/.tmp/tuic-at/6305f99c9b70bfa1/`, not the orchestrator MCP connection.
- Each child ran installed `~/.local/bin/ego` with isolated HOME/EGO_HOME under
  `~/Gits/.tmp/2317/`, `--bare --mode default --sandbox off --thinking off
  --retry 0 -m ollama/qwen3`.
- A loopback-only Ollama-compatible fixture server on port 19879 supplied model
  discovery and a served context window. It sent a partial SSE text response
  with an incomplete HTTP body, then closed the connection. No paid provider
  or real model was called.

### Failed turn, submit and mail

Session `20a08c0b-302e-482b-a05f-439954f5385a` reported:

```json
{"awaiting_input":false,"shell_state":"idle","agent_state":"idle","agent_type":"ego"}
```

Its recorded receipt includes:

```text
εgo: Turn elapsed 81ms · stopped 11:08 PM · the response stream failed: HTTP response body failed (127.0.0.1), so the model
never reported why the generation stopped; review the output, then send a new prompt to continue
```

`session submit` of `SUBMIT_AFTER_FAILURE_2317` returned `submitted=true`,
`acknowledged=true`, `write_state=complete`, `composer_state=cleared`.
A second failed-turn receipt followed.

`agent send` then returned `delivered=true` and
`delivery_path=wake_notification_and_inbox`. The captured third turn has the
native intent `[TUIC] message available — read it with: agent action=inbox`,
a fresh busy/idle pair, and a third stopped receipt. This proves that mail
wakes generation; it does not claim that an unavailable model read the inbox.

`ego-provider-failed-idle-2317.tcap` preserves all three turns, submitted input,
chunk boundaries, monotonic times and the actual 41×128 terminal geometry.

### Real approval remains protected

The same local fixture server supplied an `exec` tool request with argv
`["sh", "-c", "printf APPROVAL_2317"]`. Ego, not the server, rendered the
permission prompt in default mode. Session
`07cad1f4-ec3b-434e-88d0-0a3e6b60879a` reported confident awaiting with title
`Allow run sh -c 'printf APPROVAL_2317'?` and native y/a/n/e/d controls.
Automated `session submit` returned `submitted=false`, `reason=awaiting_input`,
`prompt_kind=approval`, `composer_state=empty`.

`ego-provider-failure-approval-2317.tcap` records this prompt. The final idle
marker comes from shutdown, not an approval answer. The test checks the
observed open prompt before teardown.

Both sessions were deleted, capture was disabled and the named agent-test
supervisor was stopped. The supervisor returned exit 2 after its stop request;
its native build and the live verification had already completed successfully.

API outcomes are retained in `reality.json`. Full local diagnostic logs and
the disposable provider/harness scripts were retained under
`~/Gits/.tmp/2317/` during verification. Initial standalone PTY setup attempts
were not used as evidence.

## Replay contract

- `ego_2317_captured_provider_failure_releases_composer`: all three recorded
  completions and stopped receipts have no awaiting badge or choice prompt.
- `ego_2317_old_emitter_awaiting_remains_authoritative`: the exact old
  busy/awaiting state bytes still produce confident awaiting through the real
  chunk processor and state accumulator. Ignoring this marker would also
  undermine real permission prompts.
- `ego_2317_captured_approval_still_owns_composer`: the recorded native approval
  reaches session state with its title and exact native controls.

Ego's own producer-side RED is recorded in
`ego/docs/internals/634-failed-turn-osc-state.md`: the old producer failed the
expected-idle assertion on pre-fix commit `7acf6a519` (production base
`19fc53f66`); the fixed producer passed its focused tests. This is distinct from
a TUIC text-parser RED, which would misidentify the proven faulty layer.

## Targeted validation results

The initial rb request exited 78 because the changed test file was classified
as a macOS path. The assigned fallback was used: native tests through mbx,
then the same focused tests on rb with `RB_FORCE=1`.

Native RED used the recorded failed-turn fixture with only its three completion
OSC `state=idle` payloads replaced by the old producer's `state=awaiting`.
TCAP record lengths were adjusted; all other captured data, ordering and times
were preserved. This controlled producer substitution is not a claim that the
old binary was recorded. The test
`ego_2317_captured_provider_failure_releases_composer` failed at the stopped
receipt with `assertion failed: !session.awaiting_input` (one test, 0.354 s;
build 4m40s). The exact recorded fixture was then restored and compared
byte-for-byte with its backup.

Final native command, through build-slot and `scripts/with-test-tmp.sh`:

```sh
cargo nextest run --manifest-path src-tauri/Cargo.toml --lib -E 'test(ego_2317_)'
```

All three tests passed: 0.145 s, build 2m05s. The native linker warned that
`__eh_frame` was too large for compact unwind offsets; it did not fail the
build or tests.

The same command passed on rb through `build-slot.sh --remote`, with
`RB_FORCE=1`: three tests passed in 0.091 s, build 2m39s,
`RB_BOX_DIRTY=no`. Remote result:
`~/Gits/.tmp/rb/20261025T221957Z-98494`.

Logs: `~/Gits/.tmp/2317/old-emitter-red.log` and
`~/Gits/.tmp/2317/replay-native-remote.log`. No full suite, mutation run or
Clippy was run. These are focused protocol/fixture checks, not a full-app pass.
