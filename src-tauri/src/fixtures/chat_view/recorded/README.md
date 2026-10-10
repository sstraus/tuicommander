# Recorded Claude chat-view corpus

These JSONL cases were cut from real Claude Code transcripts in the local
`.claude/projects` and `.claude-private/projects` trees, including subagent files.
`survey.json` records the counts and the source shape of each case. The survey
starts at 2026-09-07 UTC: dated rows use their timestamp; undated rows use the
file's modification time. Invalid/unfinished JSON rows are counted separately.
The live files can change during a capture, so this is a dated observation,
not a pinned transcript-format census.

All non-schema strings are replaced with equal-character-length `x` payloads.
Known harness prefixes, record/block discriminants, roles, MIME types and standard
tool names remain. IDs are regenerated consistently as UUIDs; dynamic path keys
are replaced by UUIDs. Numeric sizes, token counts, flags and array/object shapes
remain. JSON encoding size can differ from the source because whitespace and
Unicode encoding are normalized; `source_json_bytes` measures re-serialized
source rows, not their exact original disk encoding. No source path or prose is
stored. A tool-result case includes its real preceding call when available.

The recorder compares sanitized fixture tokens against private source vocabulary
using a linear token-set intersection and reports only the number of matches.
Protocol keys/enums and JSON literals are allowed. Generated UUIDs and repeated
`x` placeholders are normalized before the comparison to avoid incidental
matches. The retained audit reports zero matches. A previous large `grep -E -f`
audit was stopped after 97 minutes; the coordinator required the token-set method.

Re-record with:

```sh
python3 scripts/record-chat-fixtures.py \
  --output src-tauri/src/fixtures/chat_view/recorded \
  --since 2026-09-07 \
  --largest-path-file "$TMPDIR/largest-path"
```

The largest raw transcript is **never** committed or copied into this directory.
Set `TUIC_CHAT_TRANSCRIPT` to its path to opt into the ignored
`chat_view::measurement::view_real_transcript_throughput` test. It measures the
same `View::advance` reader/adapter/bounded-log/snapshot path used by Chat, both
with the production 2 MiB attach window and with a full-file window. It reports
elapsed time and process high-water RSS (bytes), not incremental allocated memory.
The full-file measurement runs after attach, so its high-water mark is cumulative.
The result is native test-profile evidence, not a release-build performance claim.

## Parser survey

| Observed shape | Chat behavior |
| --- | --- |
| Human string/array prompts, including images | User bubbles; images have markers |
| Older origin-less prompts with `promptId` | User bubbles; meta/tool rows and command/bash echoes excluded |
| Assistant text / nonempty thinking | Agent message / thought chunks |
| Empty thinking | Omitted; signature is not conversation |
| Tool use and string/array results | One lifecycle card; failed status preserved |
| Image/PDF tool results | `[image]` / `[document]` markers, without binary data |
| Assistant fallback | `Model changed` card |
| Compact-summary user rows | `Conversation compacted` card without summary body |
| Sidechain user/assistant rows | Omitted from the parent conversation |
| Human `queued_command` attachments | User bubbles at their transcript position, including prompts typed mid-turn |
| Other attachments, task-notification queued commands, queue operations, hook rows, system subtypes, snapshots | Harness plumbing; omitted |
| Titles, modes, queue/cost/history/fork/launch/link metadata | Recognized plumbing; omitted without unknown-row inflation |
| Nested `tool_reference`, `thinking_dropped`, `fallback_message` | Tool discovery/signature/API metadata; not conversation text |

Named regressions fixed by this corpus: missing older prompts, missing media
result markers, missing model-change cards, and recognized metadata counted as
unknown. Recorded image prompts, compaction and sidechain cases also exercise
the existing behavior that the earlier hand-written fixture could not prove.

## Human queued prompts (Claude Code 2.1.286)

`queued-human.jsonl` records seven real rows: a task notification, an assistant
text block, an enqueue/remove pair, a human queued command, a hook attachment,
and the next assistant text block. The human attachment came from the reported
2026-10-09 coordinator conversation. Rows retain source order, with intervening
unrelated rows omitted. Payloads use the same equal-character-length `x`
sanitation as the corpus above; source paths and prose are absent.
The full-load/live-tail regression checks that only the human attachment adds a
user entry, between the two assistant messages. Queue operations add no entries.

`queued-human-image.jsonl` records one real human queued prompt with text and
image blocks from the same reported transcript (source row 643). Non-schema
strings use equal-character-length `x` payloads; the image data is replaced
with `eA==` to avoid retaining binary content. The regression catches image
prompts disappearing when the adapter accepts only string prompt payloads.
