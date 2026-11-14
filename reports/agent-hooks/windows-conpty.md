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
installed hook launch, live state reducer, native ego control or ACP runtime
result is claimed by the runner itself; the later code-level replay is below. The parent is launched directly, not through a shell inside ConPTY;
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
this probe.

**Step 9 transport outcome: use OSC through `CONOUT$`, not the local pipe.**
Run `41293413138` demonstrates that this transport works for the measured
console-child shape, including redirected stdout and both terminators. No
transport loss was observed that would justify the fallback pipe. This selects
the transport, not a completed Windows support matrix: installed-agent launch,
Windows 10/11 shells and the manual hardware checks remain unverified.

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

## Replay validation on 2026-11-10

Validated source: base `330b1de25c8bdf4ecc7899b2e7286164adb356b6`, with
`src-tauri/src/pty/core_tests.rs` Git blob
`941c66357db7589f4fccaae90e4fc91af4c45628`. Only tests and this report changed.

```sh
scripts/with-test-tmp.sh cargo nextest run --manifest-path src-tauri/Cargo.toml \
  --lib -E 'test(windows_conpty_recorded_hook_state_roundtrip)'
```

- Mac, through build-slot and mbx: **1 passed**, test 0.391 s, run 0.425 s;
  compile 3m 25s. Rust/cargo 1.99.1, nextest 0.9.140. The linker warned that
  `__eh_frame` exceeded the compact-unwind offset limit; validation passed.
- rb, through build-slot with `RB_FORCE=1`: **1 passed**, test 0.228 s,
  run 0.255 s; compile 4m 56s. Receipt
  `20261110T222405Z-76693`: exit 0, `RB_BOX_DIRTY=no`.
  The rb receipt reports toolchain/target as unknown; no version is inferred.
- Initial rb admission exited 78 for the macOS-sensitive test path; the Mac
  run preceded the forced rb run. A direct `--force` build-slot option was
  rejected before execution; `RB_FORCE=1` is the supported wrapper form.
- The Python raw-byte verifier passed all four cells. Rustfmt and diff
  whitespace checks passed. No full suite, clippy, VM build or live app run.
- RED: not applicable. This adds recorded-evidence coverage to unchanged
  production behavior; it does not fix a demonstrated parser defect.

## Replay and decision gate

`src-tauri/src/pty/core_tests.rs::windows_conpty_recorded_hook_state_roundtrip`
replays all four original `conpty.bin` files through the production UTF-8 and
escape buffers, `ChunkProcessor`, event bus and session-state reducer. Each
capture runs with its recorded `frames.tsv` boundaries, bytewise fragmentation
and a single coalesced read. The oracle is the recorded busy/awaiting/prompt/idle
write sequence: one confident wait, its retraction, and final shell idle.
Bytewise reads cover splits within OSC prefixes, payloads and ST terminators;
coalescing covers a wait and settlement before the accumulator reads the bus.

This is a code-level replay of real transport evidence, not a live installed
hook or process-exit test. The successful hook child exits are recorded in the
runner artifacts, but no production TUIC process-exit lifecycle was captured.
No production behavior changes and no pre-fix RED are claimed. Run only this
new test; the batch landing owns clippy and broader integration validation.

The current PTY boundary remains unchanged: `tuic_state_awaiting_event` in
`src-tauri/src/pty.rs` routes awaiting into a Question event and busy/idle/prompt into
UserInput clears. `handle_tuic_state` accepts idle/busy/prompt, not awaiting.
The ACP `Command::NewSession` path in `src-tauri/src/acp/connection.rs` uses an
ACP connection and request/reply owner. This spike changes neither path and
claims no live verification of state, questions, tools, children or interrupts.

The runner evidence selects OSC for Step 9 at the transport boundary. Keep
Windows hook availability honest: the manual installed-agent and hardware
matrix remains open and is not proved by this replay.

## Windows live follow-up on 2026-11-11

The remaining Windows live SET/CLEAR and final-idle pass is blocked. A successful
SSH preflight recorded Windows 11 Pro 10.0.26200, 6,290,484 KiB visible RAM,
6,683,700 KiB total virtual memory and 12,457,750,528 bytes free on C:.
`query user` reported no interactive user session. The PATH lookup found Git,
but no Node, installed agent CLI or native ego. No executable appeared in the
queried warm export's `src-tauri/target/debug` directory; other artifact
locations were not established.

The separate, bounded artifact inventory failed at SSH authentication with
`Permission denied (publickey,password,keyboard-interactive)`. No retry followed
that failure. No build was started below the recorded 15 GB free-disk threshold.
No live app, installed hook, state reducer, retraction or final-idle result is
claimed, and no hardware criterion is checked. Existing recorded replay and
four-cell transport results were not rerun.

Raw evidence and credential-free commands remain under
`~/Gits/.tmp/tuic-win-20261111/` (`preflight.log`, `artifacts.log`, and the two
Python command files). Reliable SSH, a runnable Windows artifact or sufficient
build resources, installed agent CLIs and a desktop login remain necessary.
