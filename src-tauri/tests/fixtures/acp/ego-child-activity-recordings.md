# Ego child activity recording

`ego-child-activity-native-0.4.1.jsonl` is the native ego 0.4.1 stdio
recording from ego story 772-d55a
(`crates/ego-acp/tests/fixtures/child-activity/native-0.4.1.jsonl` in the ego
repository). It holds 138 messages in both directions: the negotiated
`childActivity` capability, foreground child lifecycles, foreground and
session clears, an empty snapshot after `session/load`, tool calls, a question,
a refused permission and a cancelled turn.

The only change from the source is that the recording user's home directory is
replaced with `/home/tester`. No frame was added, removed or reordered.

The recording has no durable child. Tests that need one use the frame shape
documented in ego `docs/integrations/acp-extensions.md`, and say so.
