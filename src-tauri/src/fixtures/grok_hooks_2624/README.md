# Recorded native Grok hook stdin (#2624-848f)

These are projections of real stdin from installed Grok 1.0.70 (f50904eaee6c), captured on 2026-11-01 in a disposable trusted project. Only event names, correlation identifiers, notification kinds, reasons and child identity remain. Correlation identifiers were consistently replaced with `recorded-id-N`; prompts, arguments, results, paths and timestamps were removed.

- `interrupt.json`: a real Ctrl+C produced StopCancelled(user_interrupt). A controller cleanup bug initially stalled after these receipts; the captured event stream itself proves cancellation arrived.
- `deny.json`: `--permission-mode default`, a project-local ask decision, and the actual `No,reject` selector produced PermissionDenied then StopCancelled(permission_rejected), without PostToolUse.
- `allow.json`: `--permission-mode default`, the project-local ask decision and a physical bare Enter on the selected approval produced permission_prompt followed by successful PostToolUse and Stop. No text-input receipt is needed for retraction.

The common command was `grok --no-alt-screen --no-subagents --disable-web-search --max-turns 1` with a trivial `printf 2624` request. Both permission cases used `--permission-mode default`. No global hooks were edited. A later denial replay with original PTY boundaries, input writes and provenance is committed as `../agent_prompts/grok-hook-denial-2624.tcap`.

Permission notifications contain neither promptId nor toolUseId. Match the newest unanswered PreToolUse in that session's current turn. SessionStart can arrive after UserPromptSubmit. Tests explicitly perturb the recorded receipts for documented outcomes absent from these recordings; those mutations are not presented as external fixtures.
