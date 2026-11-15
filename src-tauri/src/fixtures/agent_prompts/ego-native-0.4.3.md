# Real ego 0.4.3 native agent-event captures

Six raw PTY captures of the real `ego 0.4.3` binary (story 771-b309) from
`~/Gits/.tmp/ego-771live-native/v043/<scenario>/raw.pty`, recorded on macOS at
80 columns by 24 rows against an owned loopback provider. They carry the
`ESC ] 7770 ; agent-event=<base64 JSON>` frames and the coarse `state` frames
exactly as written. No bytes were edited or constructed. Contract: ego repository,
`docs/internals/terminal-agent-events.md` and `2026-11-01-771-live-host-stream.md`.

| Fixture | SHA-256 |
| --- | --- |
| `ego-native-0.4.3-cancel.pty` | `18dcf60e4893f5ac0e0be60a821f79dd7fd2c5237dc75d19befc30e251c24d2f` |
| `ego-native-0.4.3-child.pty` | `e4b29f8d7a283a1247202399e03b6cd5c6c464588215f52b048d798a1a9667fd` |
| `ego-native-0.4.3-error.pty` | `71481a85ae68cce076f3527024e780b0413c195a90d9ced2c86e037a5ef74e01` |
| `ego-native-0.4.3-permission.pty` | `28445efd25258f3f9569b915b4de97b1ec64c1806c90e2b3c66c6f6e02cdad48` |
| `ego-native-0.4.3-question.pty` | `c40fd63aaa84cfb75256c3d0d741913bad09c2522eb4482424655e544a67f8a3` |
| `ego-native-0.4.3-tool.pty` | `ec4ec2b6575346b57bc676f583af3190493f9be150f3f03cf41a9c850e780729` |

Scenarios: tool (read), permission (exec, actual approval), question (single choice,
actual Enter), child (foreground task, completed), cancel (SIGINT, exit 130),
error (provider failure). Replayed by `ego_events_tests.rs`.
