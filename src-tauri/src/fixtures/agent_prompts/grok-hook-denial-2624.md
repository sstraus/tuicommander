# Grok permission denial capture (#2624-848f)

Recorded from installed Grok 1.0.70 (f50904eaee6c), 2026-11-01, on a real PTY with 35 rows and 120 columns. Original output read boundaries and input writes are retained with monotonic microsecond offsets in TUICCAP2 format.

Command: `grok --no-alt-screen --no-subagents --permission-mode default --disable-web-search --max-turns 1 'Use the shell tool to run printf 2624. Then reply only OK.'`

The disposable project installed a project-local PreToolUse hook returning `decision: ask`. The controller selected the displayed `4(○)No,reject` choice. Actual native receipts were UserPromptSubmit, SessionStart, PreToolUse, Notification(permission_prompt), PermissionDenied and StopCancelled(permission_rejected). Hook stdin projections are in `../grok_hooks_2624/deny.json`; they are separate from terminal bytes. No PostToolUse followed denial.

The capture controller bounded the case at 25 seconds and cleanup at one second. Its only child was killed or reaped before returning. Folder trust was scoped to the disposable capture project and explicitly accepted by the coordinator; global hook configuration was unchanged.

SHA-256: `969c59e2a45ad78b074d7144c566f300af180c6581e89afa7899949d5ce141ae`
