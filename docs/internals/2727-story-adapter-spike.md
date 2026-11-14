# External story adapter spike (2727-ae2c)

This is a disposable JSON-RPC stdio adapter, not a TUIC integration. It imports the
parser from the installed root returned by `wiz-run --root`; it does not copy Wiz
source or change the WORK repository. Writes call that root's `stories-cli.js`.

The measurement uses a fresh Git repository below `~/Gits/.tmp/2727-spike/`.
It copies 385 real story files: the current top-level files followed by enough
archived files to reach 385. Archive samples are flattened only in the copy.
The original design's count was a snapshot; the active folder changes during work.
No adapter write targets the real store. Story tracking commands target the main
checkout separately.

Reproduce from the spike worktree:

```sh
node scripts/spikes/2727-story-adapter/measure.cjs \
  /Users/stefano.straus/Gits/personal/tuicommander \
  > "$HOME/Gits/.tmp/2727-spike/output.json"
```

The driver starts one Node adapter and closes it through `shutdown`. It exercises
`initialize`, `list_stories`, `get_story`, `apply` and `changes` over newline-delimited
JSON-RPC 2.0. `initialize` returns the copied folder as `watch_paths`.
`changes` reads and hashes every file on each call; a mismatched cursor requests a
rescan. It keeps one baseline, sufficient for this single-consumer spike only.
Read records include Wiz frontmatter and the SHA-256 version of the original bytes.

Latency is wall-clock time at the driver, including pipe transport, parsing and
serialization. Each warm measurement contains 50 sequential requests. Cold
initialize includes adapter startup. No jobs run in parallel except the explicit
competing writer. Watch latency contains 10 appends: arm a listener, append to a
copy, receive the `fs.watch` hint over stdout. This measures the hint, with no
debounce, not freshness of a rendered view. Events can coalesce; required cursor
scanning remains the correctness path.

The race probe first verifies a stale hash is refused with unchanged bytes. Then,
five times, it pauses the adapter immediately after a successful hash comparison,
runs the real Wiz CLI in a second process to change the same title, confirms that
write through `get_story`, releases the adapter, and reads the result. This is a
controlled interleaving, not an estimate of production race frequency. The pause
changes only scheduling; both competing mutations still use unmodified Wiz CLI.
The adapter re-reads after mutation, as proposed. A post-read cannot distinguish
its own legitimate write from a write that replaced another writer's field.

The completion probe attempts `set_status: complete` on the selected pending/ready
story without satisfying its criteria. It records the actual Wiz refusal and final
status. Configuration in this scratch Git repository is default; it does not prove
all project-specific proof/review gates or exercise the native TUIC command path.

## Results (2026-11-02)

Native Mac, darwin/arm64, Node v26.13.0, Wiz 6.0.0-beta.3. Source tree: `dbefa5be83e5a5d791c6d6ac7a1fafed3be6acc5`. Scratch evidence: `/Users/stefano.straus/Gits/.tmp/2727-spike/run-1793639665721`. [Raw results](./2727-story-adapter-measurements.json) preserve the samples summary, gate error and race outcomes. The committed scripts are the measured implementation; no Rust build or TUIC runtime integration was needed.

| Operation | Samples | Median ms | p95 ms | Maximum ms |
| --- | ---: | ---: | ---: | ---: |
| initialize | 50 | 0.047 | 0.123 | 0.172 |
| get_story | 50 | 186.455 | 815.561 | 1513.866 |
| list_stories | 50 | 204.790 | 932.340 | 1010.835 |
| changes | 50 | 184.937 | 901.196 | 2890.126 |
| watch_hint | 10 | 50.904 | 61.693 | 61.693 |

Cold initialize: 72.096 ms (one sample). Complete refusal: 147.547 ms (one sample); CLI exit 1, RPC `refused`, two unmet criteria, status stayed `pending`. Stale expected-version: one conflict, byte version unchanged. Controlled check/write races: 5/5 external titles overwritten, 0/5 conflicts reported. Race elapsed times: 403.940, 1056.903, 382.014, 468.494, 337.332 ms.

The first setup attempt refused to run because only 381 active files existed, rather than silently reporting a 385-file measurement. The completed run used four real archive files to reach the requested size. The measurement assertions all passed; the unsafe race is the expected experimental finding, not a passing safety guarantee.

## Decision: NO-GO for writable deployment

The process protocol works, but a hash check followed by an independent CLI mutation does not meet “detected, never overwritten”. Re-reading after the write does not repair that guarantee. This acceptance criterion remains unmet; it must not be checked off as safe. A lock used only by the adapter would also leave external CLI writers outside the transaction. The writer must compare the expected version under the same lock as its mutation, with all writers honoring that lock, before TUIC can offer this guarantee. That requires ownership and work in Wiz, outside this spike.

The stdio transport itself is viable for continued protocol and port work. Full-folder scans here cost about 185 ms median and 901 ms p95 under shared-machine load. These are implementation-specific costs, not pipe overhead: `get_story` intentionally scans all 385 files as well. Indexing/caching was outside the bounded experiment. A later adapter must budget scan frequency; watcher hints cannot replace cursor scans. No performance acceptance threshold was supplied.

Limits: one platform, one loaded-machine run, default scratch configuration, one operation class raced (same-field title update), and no production supervisor, crash recovery, malformed-file handling or multi-client cursor history. No claims are made for these unmeasured behaviors. The completion call came from the throwaway protocol driver, not the native TUIC UI. No production frontend/backend behavior changed, so no manual restart or visual check is required.
