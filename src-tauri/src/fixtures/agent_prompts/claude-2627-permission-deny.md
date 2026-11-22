# Claude permission deny capture, 2627-110e

Recorded from Claude Code 2.1.314 on macOS, 2026-11-02, at 30 rows by
100 columns. This is the unchanged deny capture documented in
`../claude_hooks_2627/README.md`, including its SHA-256 and native input.
It preserves PTY output, input, geometry and timing. The native answer
retracts permission attention without a PostToolUse receipt.

Also replayed by `claude_permission_cancel_or_reject_returns_to_idle_without_stop`
(#3099-6f4e): the `Interrupted · What should Claude do instead?` row above the
composer reads as an interrupted screen, and after the dialog answer it releases
the busy the prompt hook set, because no Stop hook follows.
