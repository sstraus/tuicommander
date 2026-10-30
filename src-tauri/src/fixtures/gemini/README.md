# Gemini 0.64 session fixture

Recorded on 2026-10-30 from the existing owner session
`~/.gemini/tmp/repo/chats/session-2026-10-15T14-02-1c1a1f22.jsonl`, read-only.
The installed CLI reports 0.64.0. Message content is replaced with `[REDACTED]`;
metadata, message IDs, types and timestamps are retained. No credentials were read.
The recorded project root is `/Users/stefano.straus/Gits/.tmp/live-tuic/repo`
(from the sibling `.project_root` file). Its SHA-256 matches `projectHash`.

Installed bundle `chunk-NR7L3FXV.js` SHA-256:
`d566e81b1df8d4d44a91bf7aeea96a58ec0e6c8934fa424315ef060507dcbb8c`.
`getProjectHash` hashes the exact project-root string (line 252198).
`ChatRecordingService` writes the metadata header before records (286740-286748),
and loads/migrates legacy JSON to JSONL (286670-286685). Thus legacy JSON remains
a supported resume input. Discovery reads identity metadata, not message upserts
or `$set` updates. A partial final append must not hide a valid header.
