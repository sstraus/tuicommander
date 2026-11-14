# Fabbrica control MCP fixtures

Raw JSON-RPC `tools/call` responses of the Fabbrica control MCP (`POST /mcp`),
copied unchanged from `fabbrica/evidence/040-control-mcp/` at fabbrica `da5519e`
(story 040-eddd). The Fabbrica panel code produced them in process; see the README
there to regenerate them.

- `tasks_list.json`: one active task (`asks`).
- `help_list.json`: one pending permission question of that task.

The answered case in `fabbrica_attention_tests.rs` empties `help.questions` of
`help_list.json`: Fabbrica's help page is a replacement snapshot in which empty
questions retract settled requests (`internal/execution/help.go`, `HelpPage`).

`outside_requests_list.json` is copied unchanged from
`fabbrica/evidence/043-outside-requests/` at fabbrica `00ceee2` (story 043-37f6):
one ask-state outside request. The regenerate command is in the README there.
The fake GitHub and assessor of that run are local fakes, so it is not a live
GitHub recording; the MCP reply shape is the real server path.

`outside_request_answer.json` is copied unchanged from
`fabbrica/evidence/048-outside-answer/` at fabbrica `da03a43` (story 048-032a):
the reply to `outside_request_answer` with `{request_id, decision}` (accept).
The call takes `request_id`, `decision` (`accept` or `decline`) and an optional
`note`; a second answer returns `isError: true`. Same local fakes as above.
