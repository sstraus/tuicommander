# Claude permission receipts, 2627-110e

Recorded on macOS on 2026-11-02 from installed Claude Code 2.1.314,
Sonnet 5.5 with low effort, in owned Python PTYs at 30 rows by 100 columns.
The CLI used `--permission-mode default` (through settings), `--setting-sources ''`,
`--strict-mcp-config`, and only the Bash tool. The scratch folders and original
recordings remain under `~/Gits/.tmp/claude-2627/`. No TUIC desktop was launched.
The prompt asks for `touch permission-proof.txt`. Logging hooks copy native stdin;
they do not make permission decisions. The recorder terminates only its own child.

The JSON files are recorded event arrays. Projection removes `cwd`,
`transcript_path`, `scratchpad_dir` and `permission_suggestions` (private paths).
All retained values, native session/prompt/tool ids and event order are unchanged.
The `notification.json` run leaves the prompt open long enough to record
`Notification(notification_type=permission_prompt)`.

The other runs answer with bare Enter (`accept`), `4` (`deny`) and Escape.
Accept produces `PostToolUse` and `Stop`. Deny and Escape render
`Interrupted · What should Claude do instead?` with neither post-tool nor Stop
before the recorder ends 25 seconds after the answer. These are observed native
cancellations, not assumed post-tool failures.

Each `.tcap` contains the complete recorded PTY stream. The TUICCAP2 header
records the actual geometry; records preserve all input/output bytes, read
boundaries, direction and monotonic timing. Like the existing Claude permission
fixture, the terminal output contains its scratch path. No credentials or
unrelated session content were captured.

The regression seeds the separately recorded native request metadata, then
replays each real terminal stream at its original geometry through production
input bookkeeping and the chunk processor. For accept it also feeds the recorded
post-tool receipt; deny and Escape have no such receipt. The replay is not a
claim about exact ordering between the separate hook-log and PTY streams.
Separate shell transport tests exercise actual generated commands; receipt tests
exercise native metadata ownership. The wrapped always-allow option must remain
a single native choice label so the answer keys reach input bookkeeping.

| Capture | SHA-256 |
| --- | --- |
| accept | ac3a66dcb5cca0e4ad45041b37b43ecf653df95890b200fe834ef56fcf7aa2d3 |
| deny | 651f70ba12ccb8bbe0384a7209284fe803a4916f138e9d6010e34de8ca700f87 |
| escape | 06b057f63e4e338333d1a5ec522bf2ee814fddb7b205138483daeac988f1f7ea |

[Native hook reference](https://code.claude.com/docs/en/hooks) documents permission,
notification and tool-result events. The real PermissionRequest omits tool_use_id;
the backend matches its tool name and input against an unresolved PreToolUse in
the same native session and prompt. Elicitation/result keep their existing event
contract. Mutated event names in unit tests are perturbations of these recorded
receipts, not additional claimed native captures. Windows was not exercised.

The accept, deny and escape TCAPs are stored at `../agent_prompts/claude-2627-permission-{accept,deny,escape}.tcap`, with companion provenance files, to follow the state-detection corpus convention. Their recorded bytes and hashes are unchanged.
