# Ego readiness verification — 2721-ce6b

2026-11-02: current main completed the seeded turn, an automated second prompt,
and a native background-completion wake with the installed ego binary.
No production correction is needed for the observed terminal-reply stall.
Deployment remains an operator decision; this check did not restart the daily host.

## Pinned binaries

Source: `ee8908602ea1d4fb6c19c204217495965aa5edfe`, native macOS aarch64 debug.
The source contains both terminal corrections: canonical replies queue until raw
mode, and supported queries finalize a held masking prefix.
The 2680 and 2681 fix commits are ancestors of this source.

| Binary | SHA256 |
| --- | --- |
| Isolated host | `6ec8de5178eead34dd6e824916b79e90b18030b7da0a80b6c390487afd9a9597` |
| Installed ego 0.4.3 | `2e4ac9b8da009d3fc9e6e776b068ece0c6641edb42ad055189884d6edf9d7b07` |
| Daily mapped executable path | `ebc526f77a7c39a9c51fd9d278e9ca0c46dcb111d4a9e757d7dced7ce65144b7` |

The isolated executable contains `queued reply` and lacks the old withheld
diagnostic; the daily executable has the reverse. This agrees with the prior
process-start evidence. String presence alone does not identify an exact build.

## Live result

Instance `agent-test-2721`, port 19921, accessed through its explicit Unix socket.
Final managed PTY: `528829b5-edc8-4845-9156-fcce49366664`.
The child used `TERM=xterm-256color`, the native exec tool, a dedicated session
store, and a real provider request. It started a 60-second background command.

1. Seed completed with `SEED2721_DONE` and explicit OSC idle.
2. `session submit` acknowledged the next prompt with complete write state;
   ego completed a distinct turn with `SECOND2721_DONE`.
3. Background completion started a third distinct turn, which completed with
   `WAKE2721_DONE` and printed the background command's output.

The client syscall tap recorded **11 CSI 6n queries and 11 cursor replies read**.
Maximum paired latency was **377 microseconds**. No cursor query was unanswered.
Three distinct native turn IDs each have a start and completed end event.
The built-in host tap also retains input/output bytes, including the submitted
prompt; generated terminal replies are proved by the separate client read tap.

Evidence directory: `~/Gits/.tmp/results/tuic-2721-evidence/`.
Key files: `binaries.json`, `ego.tap`, `tap-events.json`, `tap-summary.json`,
`tap-second-submit.json`, `final-output.json`, `sessions-after.json`, and raw
`.tcap` files. The retained `tap.c`/`tap.dylib` were reused from the prior live
gate. The interposer records fd 0/1/2 reads and writes without replacing the PTY;
its overhead can affect timing. No claim about race frequency follows.

## Setup corrections and limits

- Initial dependency warming left Vite unavailable. An offline frozen install
  with an explicit worktree directory restored it; tracked dependencies did not change.
- Inherited `TUIC_SOCKET` selected the daily host despite a CLI instance flag.
  Early owned children were closed. Final runs used an explicit isolated socket,
  with its session inventory checked directly. The daily process was never
  restarted, signalled, or debugger-attached.
- Duplicate positional prompt and incompatible trust/bare flags caused two early
  argument-parser exits. They provided no readiness evidence.
- The tool shell supplied `TERM=dumb`; ego then omitted OSC idle and submit
  correctly refused unconfirmed readiness. The background wake still ran.
  Explicit xterm capabilities corrected the final fixture.
- All owned PTYs were closed; the isolated inventory was empty before
  `make agent-test-stop`. No host, watcher, or test child was left running.

Build: `make agent-test AGENT_TEST_INSTANCE=agent-test-2721 AGENT_TEST_PORT=19921`,
with frontend and native Cargo through the target's build-slot/mbx integration.
`mbx doctor`: zero failures. Cargo/rustc 1.99.1; Node 26.13.0; pnpm 11.15.1.
Instruction-link validation passed. No new regression test, rb run, full suite,
release build, deployment, or installed-binary replacement was performed.

Criteria 1–3 are verified. Criterion 4 remains open for Boss's authorized
deployment process. No new RED is asserted for code that passed this check.

## Daily host redeployment (2026-11-02, 23:5x check)

The daily host was redeployed outside this investigation. This agent did not
deploy, restart, signal or attach to it.

| Item | Value |
| --- | --- |
| Mapped path | `~/Library/Application Support/com.tuic.commander/dev-bin/tuicommander` |
| SHA256 | `f03f9c0e47b420622f0437cb241d9eca48aba1d4d562ece0e46b9c82a3449844` |
| Executable mtime | 2026-11-02 19:17:08 |
| Host PID 84319 start | 2026-11-02 19:17:09, listens on :9876 |
| `queued reply` label | present |
| Old `Terminal reply withheld: tty is canonical` diagnostic | absent |

The process started after the executable was written, so it maps these bytes.
The last `main` commit before the deployment, `3b83a344dc` (19:14:30), has both
fix commits as ancestors: `48d6d658c9` (2680-2de2) and `f1ec4f5151` (2681-9810).
2681 adds no unique string, so its presence in the binary is inferred from the
build time and the 2680 label, not proved from bytes.

Not verified: a managed ego turn on the daily host. That check needs a session in
Boss's daily instance, which this assignment does not authorize. Criterion 4 needs
confirmation that this deployment was the authorized one.
