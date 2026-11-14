# Mobile control menu key evidence (1660-b7f6)

Measured on macOS, 2026-10-10. `agent-key-fixture.json` retains versions,
exact hex bytes, observed effects and short request-body markers from live
installed CLIs. It covers every `AGENT_TYPES` registry entry. The production
mapping lives in `src/agents.ts`, alongside the other per-agent configuration.

No single modified Enter sequence worked for both Claude Code and Codex.
Sending CR everywhere would silently execute input in an unknown shell, so it
is not the fallback. Ctrl+Enter is intentionally agent-aware, not a universal
submit command. Ordinary Enter remains available separately.

| Agent | Installed version | CSI-u | modifyOtherKeys | LF / Ctrl+J | CR | Selected |
|---|---|---|---|---|---|---|
| Claude | 2.1.286 | submit | submit | newline | submit | CSI-u |
| Codex | 0.162.0 | ignored | ignored | newline | submit | LF |
| OpenCode | 1.18.30 | newline | newline | newline | submit | CSI-u |
| Goose | 1.49.0 | ignored | corrupt (`13~`) | newline | submit | LF |
| Grok | 1.0.50 (c58f321264ba) | ignored | ignored | newline | submit | LF |
| pi | 0.84.2 | ignored | ignored | newline | submit | LF |
| ego terminal CLI | 0.1.0 | ignored | ignored | submit | submit | LF |
| Gemini | not installed | unverified | unverified | documented newline | documented submit | CSI-u, native intent unverified |
| Amp | not installed | unverified | unverified | documented newline | documented submit | LF, documented fallback |
| Cursor agent | not installed | unverified | unverified | documented newline | documented submit | LF, documented fallback |
| Droid | not installed | unverified | unverified | changelog shortcut | documented submit | CSI-u, native intent unverified |
| git / api | registry bookkeeping, no composer | unverified | unverified | unverified | shell-dependent | CSI-u, conservative fallback |
| null / unknown | no detected agent | unverified | unverified | unverified | shell-dependent | CSI-u, conservative fallback |

An unverified conservative mapping can be ignored by the target. It preserves
modified-key intent without pretending a plain Enter is equivalent. Gemini's
native Ctrl+Enter is documented as newline, but its byte spelling was not
verified locally. Droid Ctrl+J
opens the changelog, so LF is deliberately not used there.

Vendor references consulted:

- [Gemini keyboard shortcuts](https://geminicli.com/docs/reference/keyboard-shortcuts/)
- [Amp CLI keybindings](https://ampcode.com/docs/cli/keybindings)
- [Cursor terminal setup](https://prod.cursor.com/docs/cli/reference/terminal-setup)
- [Droid quickstart](https://docs.factory.com/droid-cli/quickstart)
- [Droid CLI reference](https://github.com/Factory-AI/factory/blob/main/docs/reference/cli-reference.mdx)

## Reproduction and boundaries

`python3 docs/evidence/ctrl-menu-1660/probe.py OUTPUT_ROOT [AGENT ...]`
starts each installed CLI in a fresh owned PTY, with isolated HOME/XDG/vendor
config directories, updates disabled, dummy keys, and a local HTTP endpoint.
The endpoint returns 401 for model turns, so no model tool turn can execute.
ego discovery gets dummy model metadata and an empty Ollama warmup response.
No real credentials or sessions are read. The probe derives from story 1656's
owned-PTY method. Each sequence uses a new process. Type `first-probe`, send
the candidate, then type `second-probe` and CR. A model request after the
candidate proves submit; the final request marker distinguishes ignored keys,
newlines, and corruption. ANSI snapshots remain under the chosen output root.

Set `TUIC_PROBE_HTTP=http://127.0.0.1:19877` to use sessions created and deleted
through an isolated worktree `tuic-remote` daemon instead of Python's PTY.
The dummy pairing token is `ctrl-menu-owned-probe`; launch with that value in
`TUIC_PAIRING_TOKEN`, `--instance ctrl-menu-1660 --no-agent-configs`, an isolated
HOME under `~/Gits`, and `TUIC_PORT=19877`. The probe never touches Boss's PTYs.

Initial invalid Goose telemetry and ego startup-dialog/warmup runs were
excluded. Valid Python evidence is retained under
`~/Gits/.tmp/ctrl-menu-1660-all/{runs,goose-retry,ego-context}`. Full ANSI and
request captures are local evidence; the committed fixture keeps only markers
needed to explain the observed key effects.

Visual iPhone/iPad screenshot verification remains open: the coordinator
confirmed that the locked Mac screen prevents the stealth browser capture.

All seven installed terminal agents were also probed through the worktree
TUIC HTTP/session PTY transport. The committed fixture uses these captures:
`~/Gits/.tmp/ctrl-menu-1660-all/tuic/isolated-runs` for the first six and
`tuic/ego-ports` for ego. Every effect in the table was reproduced. Each case
has a unique endpoint prefix; ego drops that prefix, so its cases instead use
a unique listening port. This excludes late retries from earlier processes.
The daemon was stopped and all owned sessions deleted by the probe.

Validation: the requested full Vitest run passed 8,722 tests and failed one
new CSS-loading test. After fixing that test harness, the targeted Ctrl menu
file passed 27/27; the other two keybar files passed 13/13 in the correction
run. No full suite was repeated. The worktree headless build passed.

## Current ego contract (2026-10-31)

The ego 0.1.0 observations and fixture remain historical evidence. Current ego
handles CSI u `ESC [ 13 ; 5 u` as Ctrl+Enter for busy-turn steering. Its
`crates/ego-cli/src/repl/keys.rs` maps Enter with Control to `SteerBusy`, and
`live_surface_ctrl_enter_steers_a_busy_turn_once` sends these exact bytes.
The mobile ego control now uses this contract instead of the old LF fallback.
Other agent mappings still use the recorded observations above.


### Legacy compatibility and native steering check (#2599-0a6b)

Do not send CSI u to all legacy programs. Native bash and zsh probes in critic
round 1 showed pending-command corruption (`5uAFTER: command not found`). Grid
Ctrl+Enter therefore stays CR unless Kitty flag 1 is active or the current
foreground identity is ego. The exception uses the existing Kitty encoder;
it ends as soon as the session identity returns to a shell. No new detection
or protocol state is added. The mobile ego mapping stays CSI u.

An isolated native PTY check with installed ego 0.4.1, a local streaming provider,
and no credentials showed `interrupt=yes`, `Interrupted by user`, and
`STEER-RECEIVED`. The provider received two requests: the initial request contained
no steer text, and the second contained the exact steer once. No Kitty push was
observed. Retained raw evidence is in
`~/Gits/.tmp/tuic-2599-f1-live-1793470215/{terminal.ansi,requests.json}`.
This is native ego evidence, not a rebuilt TUIC desktop end-to-end check.

Ego currently enables raw mode and bracketed paste in `repl/seat.rs`, but does
not call Reedline's `use_kitty_keyboard_enhancement(true)`. Its vendored Reedline
already provides negotiation and balanced push/pop in `read_line` and Drop.
Enable that builder option in ego, then verify startup, busy steering, modal
input, Ctrl+Z/fg, and exit restoration before removing TUIC's narrow exception.
