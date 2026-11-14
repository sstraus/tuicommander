# Native worktree recycling measurement — 1705-67b3

Measured on macOS on 2026-10-14 with source commit `ed856e5f0392`.
The coordinator explicitly authorized one native headless build after the existing
`tuic-remote` was found to be a desktop-feature stub.

## Method

- Build: mbx shim through `build-slot.sh cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features`. `mbx doctor`: zero failures. Build: 3m34s, 453 hits and 122 misses.
- Start only the headless binary with a unique loopback port and named instance, `--no-agent-configs`, isolated HOME and TMPDIR under `~/Gits/.tmp/feat-1705r-native`. No desktop process was launched or restarted. The owned daemon PID was terminated and awaited in `finally`.
- Scratch Git repository: 101 tracked files; one finished checkout with 1,000 ignored cache files (1,024,000 bytes). Source and ignored-file contents are synthetic local fixtures, not a copy of the production repository.
- Seed one checkout, wait for warming, add ignored files, then safely remove/archive it. Measure 15 serial `POST /worktrees` responses: pilot 3, main 10, completion 2. Safely remove/archive each checkout between creations so the next create recycles it. No agent PTYs were spawned.
- Every response had `worktree_rename`; the old path disappeared, branch HEAD matched repository HEAD, and the ignored artifact inode stayed identical.
- Authenticate once with fixture-only Basic credentials, then use the issued session cookie. An earlier run repeated bcrypt Basic authentication per request; it is excluded from the final timings (15 creates, 15.173 s total) because it measured avoidable authentication work too.
- Sample `ps -axo pid=,pcpu=,comm=` every 100 ms around the create/archive run. Sum matching `wdavdaemon` processes and `fseventsd` separately per sample. **`ps %cpu` is a rolling process average, not interval CPU or CPU attributable to this benchmark.** The host had other work. Sampling includes archive operations between creates.

The script and raw final results are adjacent: `worktree-recycling-1705-native.py`
and `worktree-recycling-1705-native.json`. The script uses an isolated scratch
root; remove that task-owned root before a repeated run. Its fixture credentials
are not production secrets.

## Results

15 of 15 recycled creations succeeded. Total HTTP create wall time: **10.801823 s**.
Median: **0.701350 s**; minimum: **0.621877 s**; maximum: **0.888786 s**.

| Creation | Seconds |
|---|---:|
| pilot-0 | 0.621877 |
| pilot-1 | 0.749290 |
| pilot-2 | 0.881157 |
| main-0 | 0.690551 |
| main-1 | 0.758963 |
| main-2 | 0.701350 |
| main-3 | 0.705567 |
| main-4 | 0.661313 |
| main-5 | 0.791435 |
| main-6 | 0.721188 |
| main-7 | 0.652631 |
| main-8 | 0.652088 |
| main-9 | 0.646087 |
| completion-0 | 0.888786 |
| completion-1 | 0.679540 |

153 CPU samples over 25.655 s:

| Process family | Minimum % | Mean % | Peak % |
|---|---:|---:|---:|
| wdavdaemon, summed | 68.5 | 144.116 | 284.4 |
| fseventsd | 0.1 | 5.203 | 19.1 |

## Baseline comparison and limits

The orchestrator report `2026-10-10-ego-swarm-benchmark.md` reports a
18:25–18:31 window for 15 full production worktree spawns, three failures,
Defender peak 277% and fseventsd peak 32%. Each production checkout held
approximately 5.4 GB, including a large untracked directory and ignored artifacts.
The baseline does not give per-create wall times, and its initial CPU sampler was
incorrect; some peaks came from later or manual samples.

This scratch-repository run is not a matched before/after experiment: it is serial,
uses a much smaller fixture, has no agent launches, and observes shared-host CPU.
It proves native API recycling succeeds and retains ignored inodes; it does **not**
prove a production throughput speedup or reduced Defender CPU. In fact the observed
Defender peak here was higher than the reported baseline. fseventsd's observed
peak was lower, but attribution and workload differences prevent a causal claim.
No settings, exclusions, cache, or production checkout were changed.
