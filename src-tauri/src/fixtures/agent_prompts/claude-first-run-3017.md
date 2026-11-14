# Claude first-run dialogs, 3017-5158

Source: Claude Code 2.1.322 (`claude --dangerously-skip-permissions`, the
flag a managed spawn passes) in an owned Python PTY, recorded on 2026-11-12
at 41 rows by 80 columns. The scratch Git repository was
`~/Gits/.tmp/tuic-3017/scratch`. Its `CLAUDE.md` imported an owned external
Markdown file, so both first-run dialogs rendered. No prompt was submitted
and no production TUIC session was used.

The capture holds the workspace trust dialog (default `No, exit`), the
recorded answer Down+Enter, the external-imports dialog (default `No, disable
external imports`), the recorded answer Enter, then the ready composer. The
recorder then sent SIGTERM to its own child.

The recorder writes TUICCAP2 records directly from PTY reads and input
writes, with monotonic microsecond timestamps and the PTY geometry. Bytes
and read boundaries are unchanged. SHA-256:
`2ee4d3f69435b1a3f9a23aea3b9c9fa01ec6264f3e332076648b5114aedfc8ef`.

The managed first-run tests replay the output records through the
production chunk processor: a managed Claude child answers both dialogs once,
and an unanswered dialog reaches the parent as a wait with its text.
