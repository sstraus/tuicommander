# ego native question answer capture, 2634-ea5b

Converted from a live ego 0.4.3 PTY capture on macOS, 2026-11-01, at 32 rows by
110 columns (`--tools ask_question`, local fixture model). SHA-256 of this
file: `56830371ee4e575aae41470dd52983e8da1d39ddedfb7a38aa6a46d8c15e0583`.
The source kept output bytes only; chunk boundaries, the answer Enter and
`/quit` are reconstructed and timing is synthetic. See
`../agent_hook_payload/README.md` for the source SHA-256 and the converter.

## Replay contract after input-readiness separation

Commit `15619b4a2f` (#3064-3141) makes native ego turn frames own shell
readiness. Coarse `state=busy` still clears a wait through its empty
`user-input` event, but no longer repeats the shell transition that also
emitted `protocol-question-cleared`.

This capture starts its native session at record 0 and its turn at record 1.
The matching `modal_end` at record 9 retires the answered question card.
Record 10 then contains coarse `state=busy`: its existing empty `user-input`
event clears the confident question, choices and awaiting evidence. The
snapshot must still be busy/working, with no question and no awaiting input.

The replay correction (#3073-bad7) removes only the redundant
`protocol-question-cleared` event at record 10 (33 trace entries become 32).
Every remaining event, its order and metadata, all snapshots, the final idle
state and ring length stay unchanged. The capture bytes stay unchanged.
The recorded modal-card test independently checks card retirement and rejects
a late attempt to reopen the settled modal.
