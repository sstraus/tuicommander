# Recorded serial workflow runs

These JSON event streams were captured from the existing native RunStore during
WF01's pre-change RED run on 2026-10-30. Production code was commit
`0fd8fb21e23ac31a949ce560c6d9b4fff88f4e86`; only the two contract tests and their
fixture helper were added. The capture ran on rb, not on the Mac. No provider
payload, report, event ID or effect result was invented.

- `v2-exclusive-join.json`: the existing graph fixture with an exclusive Join
  after two mutually exclusive Judge outcomes. One selected arrival reaches End.
- `v2-active-pin.json`: the existing seeded story fixture immediately after its
  graph-start event. Its publication includes the native required-check policy.

The files retain the observed IDs, timestamps, definition revisions and rb
scratch-project paths. Those paths are event data, never filesystem inputs to a
replay. Read them through the pure reducer; do not operate on the recorded paths.

Capture command: `scripts/with-test-tmp.sh cargo nextest run --manifest-path
src-tauri/Cargo.toml --lib -E 'test(old_exclusive_join_replays_without_parallel_wait)
| test(draft_publish_does_not_repin_active_run)' --failure-output immediate`,
with `WF01_CAPTURE=1`, through remote build-slot. The two tests emitted their
actual `events_after` serialization before asserting the new version/pin fields.
The original log is `/Users/stefano.straus/Gits/.tmp/wf01-red.log`.

Both RED failures were upgrade assertions: missing contract version 3 and
missing immutable contract metadata. Their exclusive-Join and publication
immutability assertions passed on the old code. This is a feature boundary,
not evidence that the old serial engine had broken Join or publication behavior.

The current tests replay these unchanged version-2 streams and separately
exercise new version-3 starts. These are replay/migration-boundary fixtures;
WF01 performs no active-run migration and no operational cutover.
