# Recorded automatic-compaction update

Recorded on 2026-11-04 from ego commit feb314d5f (`ego acp` against the stub provider of
`crates/ego-cli/tests/it/acp_compaction.rs::an_acp_prompt_at_the_threshold_continues_in_the_successor_and_the_next_prompt_follows`).
A disposable copy of that commit logged every stdout frame of the test host. The JSON file is the
unmodified `params.update` of the one `session_info_update` carrying `_meta.ego.compacted`.
The session ids are those the stub run minted; nothing was rewritten.
The initialize advertisement is `src-tauri/tests/fixtures/acp/ego-compacted-initialize.json`, a
byte copy of ego's `tests/goldens/tuicommander/initialize.json`.
