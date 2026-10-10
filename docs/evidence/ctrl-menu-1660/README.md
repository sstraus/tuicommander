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
| Aider | not installed | unverified | unverified | unverified | mode-dependent | CSI-u, native intent unverified |
| Amp | not installed | unverified | unverified | documented newline | documented submit | LF, documented fallback |
| Cursor agent | not installed | unverified | unverified | documented newline | documented submit | LF, documented fallback |
| Droid | not installed | unverified | unverified | changelog shortcut | documented submit | CSI-u, native intent unverified |
| git / api | registry bookkeeping, no composer | unverified | unverified | unverified | shell-dependent | CSI-u, conservative fallback |
| null / unknown | no detected agent | unverified | unverified | unverified | shell-dependent | CSI-u, conservative fallback |

An unverified conservative mapping can be ignored by the target. It preserves
modified-key intent without pretending a plain Enter is equivalent. Gemini's
native Ctrl+Enter is documented as newline, but its byte spelling was not
verified locally. Aider multiline mode changes Enter behavior. Droid Ctrl+J
opens the changelog, so LF is deliberately not used there.

Vendor references consulted:

- [Gemini keyboard shortcuts](https://geminicli.com/docs/reference/keyboard-shortcuts/)
- [Aider commands and multiline mode](https://aider.chat/docs/usage/commands.html)
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
