# Agent tool readout fixtures (story 2636-5551)

`ego-0.4.3-tool.json` and `ego-0.4.3-cancel.json` are the decoded
`OSC 7770;agent-event` frames of two live ego 0.4.3 runs (story 771-b309,
`ollama/fixture` model, `--tools read,exec,ask_question,task`). They were
copied unchanged from the `frames.json` file of each capture.

- `tool`: one `read` call that completes, then turn and session end.
- `cancel`: one `task` call with a foreground child, then a cancelled turn.
