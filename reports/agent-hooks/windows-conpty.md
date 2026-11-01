# Windows ConPTY OSC spike

Status: **runner transport probe passed**. OSC 7770 payload bytes reach the
native ConPTY reader unchanged with BEL or ST and inherited or piped hook stdout.
The surrounding output stream changes: OSC can precede earlier printable text.
This is not full Windows hook support or a completed installed-agent matrix.

## Recorded runner result on 2026-11-01

Run: <https://github.com/sstraus/tuicommander/actions/runs/41293413138> — success.
Source: `a0adafba09b0a27d08304ea6a04c62aaebd09a8e`.
Host: Windows Server 2025 Datacenter, version `10.0.26100`, build `26100`;
PowerShell `7.6.7`; Rust `1.99.1`, `x86_64-pc-windows-msvc`.

The standalone harness in `scripts/agent-capture/windows-hook-osc.rs` uses
`portable-pty = 0.9.0`, the same version and native API seam used by TUIC:
`native_pty_system().openpty`, `slave.spawn_command`, and
`master.try_clone_reader`. Production references are
`src-tauri/src/pty/lifecycle.rs::spawn_pty_pair_with_retry` and
`src-tauri/src/pty/commands.rs`. No full application binary was built or run.

The ConPTY child acts as an agent-shaped console parent. It starts a separate
hook-shaped subprocess with ordinary console inheritance, no new or detached
console, and no console attachment. The hook opens `CONOUT$` with read/write
access, reads the mode without changing it, and calls `WriteFile` with the
requested bytes. In the pipe cases, hook stdout and stderr are separate pipes
owned by its parent. The OSC still arrives at the ConPTY reader, while the
stdout marker arrives only in `hook-stdout.bin`.

The hook is not an installed agent hook. No agent version, event schema,
installed hook launch, state reducer, native ego control or ACP runtime result
is claimed. The parent is launched directly, not through a shell inside ConPTY;
PowerShell is the outer workflow shell only.

### Raw-byte decision

Fixtures: `reports/agent-hooks/fixtures/windows-conpty-2631/`. Each cell retains
unaltered `conpty.bin`, reader offsets and monotonic microsecond timings in
`frames.tsv`, exact `write-request.bin`, hook/parent process IDs, write result,
console mode, and child exit status. The pipe cells also retain hook stdout
and stderr. `terminal-input.bin` records the sole terminal reply, `ESC[1;1R`,
to ConPTY's startup cursor query `ESC[6n`. No captured output was filtered.

| Case | OSC bytes | Raw bytes | Console mode | Write result | Hook body cost |
| --- | ---: | ---: | --- | --- | ---: |
| BEL, inherited stdout | 78 | 269 | `0x7` | 116/116 bytes | 682 us |
| BEL, piped stdout | 78 | 248 | `0x7` | 116/116 bytes | 692 us |
| ST, inherited stdout | 82 | 273 | `0x7` | 120/120 bytes | 693 us |
| ST, piped stdout | 82 | 252 | `0x7` | 120/120 bytes | 711 us |

Costs start inside the hook and include its diagnostic file work. They exclude
process creation and installed-agent event dispatch. All four children exited 0.
All console-mode queries and writes succeeded, with no reported error.

In every cell the OSC block starts at raw byte offset 116. Exact BEL block:

```text
\x1b]7770;state=busy\x07\x1b]7770;state=awaiting\x07\x1b]7770;state=prompt\x07\x1b]7770;state=idle\x07
```

For ST each `\x07` above is the two bytes `\x1b\x5c`, unchanged at the reader.
`analysis.json` retains the complete OSC hex strings and raw SHA-256 hashes.
The four raw hashes, in table order, are:

```text
0e73826023287143f51b3967cbbb81a91d685380d331c0934d6930cea313f51b
b228546d9b960a62ef9773b0ee51745b7d38c3eedf7a96a7657946b37f7ecbfd
9fc5088513e7340b3a21cec64d3062e2ee33da14996eba7e8a6b33a243332f34
7113af18aae61f2df79eb1ab838ff33a180ba9ab6759e0f9d78c8fa944df12c5
```

**Decision: unchanged OSC, changed whole stream, not lost.** Within the OSC
block the state order and terminators are exact. However, the requested
`HOOK-CONSOLE-BEGIN` line precedes OSC in `write-request.bin` and follows it in
`conpty.bin`. Do not infer chronology between hook OSC and rendered prose from
this probe. This runner result removes a transport-level reason to require a
local pipe for this console-child shape; it does not select a production
transport for the unavailable installed-agent matrix.

Reproduce the evidence checks locally without a Rust build:

```sh
python3 scripts/agent-capture/windows-hook-osc-verify.py \
  reports/agent-hooks/fixtures/windows-conpty-2631
```

This command passed for all four cells. It checks exact OSC content/order,
terminators, frame coverage, control text, successful exits, actual mode and
stdout isolation. It is not a production parser or state-retraction test.

### Earlier attempts and limits

- Run <https://github.com/sstraus/tuicommander/actions/runs/41293098245>
  failed at the 30-second child bound. Its original artifact is in
  `fixtures/windows-conpty-2631-startup-query/`. The first four reader bytes
  were `ESC[6n`; the child had not reached its entry point. The corrected
  harness replies as a terminal would and records the reply separately.
- Run <https://github.com/sstraus/tuicommander/actions/runs/41293289634>
  succeeded for all four OSC cases. Its artifact is in
  `fixtures/windows-conpty-2631-write-only/`. Mode queries failed because the
  handle requested write-only access. The final run requests read/write access
  for `GetConsoleMode`; it still does not change console modes.

All three runs built only the small native harness. No rb, application suite,
RED, GREEN, clippy or production replay result is claimed. The required
`windows_conpty_recorded_hook_state_roundtrip` remains absent; this runner
fixture can inform it, but synthetic state commands do not prove native
installed-agent event ownership or wait retraction.

### CI isolation and landing

`ci.yml` triggers push only on `main`, plus pull requests. The dedicated
`spike-2631-conpty.yml` triggers push only on the authorized spike branch, with
paths limited to itself and the harness, plus manual dispatch. No pull request,
tag, other branch push or full CI matrix was started. The existing lightweight
CSP policy workflow also ran and passed on each push.

The workflow must not land on `main`. Land only the separate result/evidence
commit, not a merge of this branch and not the three workflow/harness commits.
If the capture harness is needed later, copy only its source file from the final
harness commit. The temporary remote branch is deleted after the result commit
is pushed; deletion is verified separately in the final handoff.

## Parser evidence inspected before capture

The initial investigation host reports `Darwin`. The runner artifacts above
now provide real Windows bytes, but not the broader installed-agent matrix.

Existing parser tests accept OSC 7770 with BEL and ST terminators
(`src-tauri/crates/tuic-terminal/src/terminal_grid.rs`,
`osc7770_state_event` and `osc7770_st_terminated`). They do not prove that Windows
passes an unknown OSC from a hook subprocess through `CONOUT$` into ConPTY.
Those tests were inspected, not executed for this report.

The application already has a raw PTY capture tap:
`src-tauri/src/mcp_http/log_routes.rs` exposes `GET/POST /diagnostics/capture`.
`src-tauri/src/pty_capture.rs` uses the absolute `TUIC_CAPTURE_DIR` override or
an instance-local captures directory. Capture is session-filterable.

Production files, user hook configuration and state sources remain unchanged.
This spike adds runner fixture bytes only. It makes no full Windows support claim.

## Windows capture procedure

Run this only on an authorized isolated Windows TUIC instance and a throwaway
repository. Preserve user hook settings. Use a disposable agent configuration
when the installed agent supports it; otherwise retain and restore the exact
configuration before and after each cell. Never change the daily instance.

1. Record Windows build, shell and version, TUIC commit, installed agent version,
   exact launch command, hook event name, hook command, parent/child process IDs,
   working directory and console inheritance. Record native ego version too.
2. Create a throwaway terminal session. Start capture on the isolated instance
   before the hook event. The request is
   `POST /diagnostics/capture` with JSON
   `{"enabled":true,"session_id":"<throwaway-session-id>"}`. Check the returned
   status and retain the reported directory. Do not use the orchestrator endpoint.
3. Adapt `scripts/agent-capture/windows-hook-osc.rs` to the installed hook on
   that Windows host. Its hook subprocess must open `CONOUT$`, record console mode and open/write
   errors, and write the exact OSC bytes. Keep hook stdout separately redirected
   to prove that delivery does not rely on stdout inheritance. Do not silently
   attach to a different console or change console mode to obtain a pass.
4. Launch the installed agent normally and cause its real hook to invoke the
   helper. Record the native event payload and hook elapsed time separately from
   the parent PTY capture. Exercise BEL and ST independently. A direct shell
   invocation of the helper is a control, not the hook-subprocess result.
5. Exercise busy, human wait, answer, reject, cancel, stop and process exit.
   Retain both raw frames and observed session state. Establish which native
   owner emits each settlement; an idle frame alone does not prove wait clear.
6. Stop capture with `POST /diagnostics/capture`, JSON `{"enabled":false}`.
   Retain the original `.tcap`, hash, capture metadata, hook stdout/stderr,
   helper errors, elapsed measurements and state observations. Restore hooks,
   delete the throwaway session and stop only the isolated instance you started.
7. Repeat the native ego PTY control without an installed hook. For ego ACP,
   exercise activity, human wait, question, tool, child and interrupted outcome
   through the existing ACP connection; do not create a PTY for the ACP peer.
8. Commit only reviewed, non-sensitive evidence. Record unavailable cells as
   unavailable. Never replace a missing capture with a hand-written fixture.

## Required matrix

Every cell is currently **unavailable**. Expand each shell cell by installed
agent/version, BEL/ST, inherited console/stdout redirection, successful hook,
write error and hook exit. Retain elapsed cost for each successful invocation.

| OS | cmd | PowerShell | Git Bash | Native ego PTY | ego ACP |
| --- | --- | --- | --- | --- | --- |
| Windows 10 | unavailable | unavailable | unavailable | unavailable | unavailable |
| Windows 11 | unavailable | unavailable | unavailable | unavailable | unavailable |

## Replay and decision gate

After receiving a real capture, add
`windows_conpty_recorded_hook_state_roundtrip` under the existing agent-prompt
fixture tests. Replay original bytes and splits across OSC prefix, payload and
terminator. Assert transitions through the production accumulator and state
reducer, including wait settlement and process exit. Associate expected state
with the recorded native event and observed outcome, rather than inferring it
from the fixture's name. Record the pre-fix RED if a defect is exposed.

Run only the new replay and touched module tests on rb, with the required clippy
command in the same call if a Rust test is added. This report adds no Rust test,
so a Rust build cannot supply the missing evidence.

The current PTY boundary remains unchanged: `tuic_state_awaiting_event` in
`src-tauri/src/pty.rs` routes awaiting into a Question event and busy/prompt into
UserInput clears. `handle_tuic_state` accepts idle/busy/prompt, not awaiting.
The ACP `Command::NewSession` path in `src-tauri/src/acp/connection.rs` uses an
ACP connection and request/reply owner. This spike changes neither path and
claims no live verification of state, questions, tools, children or interrupts.

Only the real matrix can justify shipping OSC or choosing the local pipe.
Missing evidence is not a failed OSC experiment. Keep Windows hook availability
honest until one transport has been demonstrated end to end.
