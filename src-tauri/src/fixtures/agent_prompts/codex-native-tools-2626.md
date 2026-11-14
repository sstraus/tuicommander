# Recorded native Codex hook transport — 2626-f570

Captured 2026-11-02 on macOS arm64 with installed codex-cli 0.166.0.
SHA-256: `f338d651887880d82f969d4e53f4645d4dff4a0c9a62df8c59257e838440b822`.

Real codex app-server turns in an isolated CODEX_HOME/project emitted native hook
stdin. A deterministic local Responses server requested exec_command and
request_user_input; the actual native RPC answered Blue. The probe passed each
original stdin payload to the production shell emitter extracted from
agent_hook.rs. Only the tty destination resolver was replaced with the harness's
real PTY slave path, as in the shell producer regression tests. Hook subprocesses
launched by App Server lack a controlling tty: an earlier unmodified resolver
probe captured no output, so this fixture does not prove native tty discovery.

A recorder read the real PTY master bytes into TUICCAP2, with actual elapsed
timestamps and 24×100 geometry. No OSC/output bytes or native JSON were rewritten.
This is output-only hook transport evidence, not interactive TUI rendering,
keyboard controls, permission ownership, or Windows evidence. Existing terminal
fixtures separately protect screen-derived answer controls. Native receipt/RPC
logs remain in ~/Gits/.tmp/codex-2626-pty3/; script in
~/Gits/.tmp/codex-2626-source/capture_pty.py. No paid model calls, authentication,
daily config or installed binary changes. All probe processes/server stopped.
