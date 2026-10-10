# Claude tool-call dot capture — 2026-10-10

`claude-dot-blink-1654.json` contains the exact UTF-8 raw output returned by
TUICommander `session output format=raw` for an owned disposable PTY. The
producer was Claude Code v2.1.286, with 60 columns and 35 rows. No terminal
resize was requested. The PTY was closed after capture.

The prompt asked for one Bash call containing `sleep 20` and a long `echo`
argument. The command and source session UUID are in the JSON `source` field.
Consecutive reads covered byte offsets 0–9214, 9214–27463 and 27463–52896.
The `data` field decodes to 52,896 bytes; SHA-256:
`8a8dc75fc0151a506276bf803d4fae1b2d8ed1338d2709ac9d57d67b7984c8fa`.
The capture retains shell startup, prompt text, hooks, HUD and output; it has
not been reduced to invented ANSI sequences. Read boundaries carry no timing.

## Decoded behavior

The running tool dot is U+23FA `⏺`, foreground RGB 153/153/153. The OFF frame
writes U+0020 space with the same foreground. Neither contains VS15 or VS16.
The completed dot uses RGB 78/186/101. Both pulse characters occupy one cell
with this checkout's `unicode-width`, asserted by the replay test.

Claude explicitly lays out four tool rows with CR and CUD, rather than relying
on terminal autowrap. The command continuation rows start at column 7. Their
content fits the 60-column grid; the final continuation is abbreviated with
an ellipsis. The first tool row uses CHA 3 after the dot, so the Bash label
starts at a fixed column even if the painted glyph has a different advance.

There are 30 isolated pulse frames: 15 ON and 15 OFF. Each enters synchronized
update mode (DEC 2026), moves from the composer to its bottom anchor, issues CR
and CUU 14 or 15 to reach the dot, writes the dot or space, returns through
14 or 15 CR/LF steps, then uses CUU 5 or 6 to restore the composer position.
These isolated frames contain no EL or ED. Other frames use EL while changing
the tool content or HUD. The final green frame also adds command output.

## Replay result and limits

`real_claude_dot_pulse_does_not_shift_wrapped_block_1654` replays the full
capture into `TerminalGrid` at the recorded geometry. Across all 30 isolated
pulses it compares all four tool rows at the same screen indexes, excluding
only column 1, where the dot changes. It passes on unchanged production code
at `4f2e73cc4a046a2d2d0e7207f9cdcc83f9e584e2` (native macOS).

This is diagnostic evidence, not a RED reproduction or a fix. The earlier
failed assertion counted an additional matching prompt row; it did not prove
movement. No producer defect, grid defect or canvas defect is established.
Unwrapped, two-row, three-row and final-green layout assertions remain pending
a reproducing capture. No desktop instance was launched and no canvas screenshot
was taken.

The alternative DOM terminal Chat view uses `Transcript`, whose status dot is
an empty span with fixed 6px width and `flex-shrink: 0`; its pulse animation
changes opacity only (`AIChatPanel.module.css`). CLI mode uses `CanvasTerminal`:
`gridRenderer.ts` paints each glyph at `column * cellWidth`, with no text
word-wrap. These source observations rule out a glyph-advance explanation in
those specific paths, but do not establish which view Boss observed.
