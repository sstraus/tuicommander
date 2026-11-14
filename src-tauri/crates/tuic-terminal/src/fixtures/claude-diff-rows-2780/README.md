# Claude diff rows — 2780-e44d

Twelve diff rows (`+` lines) copied byte for byte from the recorded Claude Code
PTY capture `src-tauri/src/fixtures/agent_prompts/claude-hooked-missing-question-20260921.raw`
(byte range 3606..7098). Claude Code drew them in a 229-column terminal.

Each row is `ESC[6G`, a green foreground, the background `ESC[48;2;2;40;0m`, the
line number and text, then literal spaces up to column 229, then `ESC[49m ESC[39m CR CR LF`.
The padding is plain spaces with the diff background, not an erase sequence.
