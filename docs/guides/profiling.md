# Performance Profiling

Repeatable profiling infrastructure for identifying bottlenecks across the full stack: Rust backend, SolidJS frontend, Tauri IPC, and terminal I/O.

## Quick Start

```bash
# Install profiling tools (one-time)
scripts/perf/setup.sh

# Run all automated benchmarks (app must be running)
scripts/perf/run-all.sh

# Or run individual benchmarks
scripts/perf/bench-ipc.sh              # IPC latency
scripts/perf/bench-pty.sh              # PTY throughput
scripts/perf/record-cpu.sh             # CPU flamegraph
scripts/perf/record-tokio.sh           # Tokio runtime inspector
scripts/perf/snapshot-memory.sh        # Memory profiling guide
```

Results are saved in `scripts/perf/results/` (gitignored).

## Tools

| Tool | What it profiles | Install |
|------|-----------------|---------|
| [samply](https://github.com/mstange/samply) | Rust CPU time (flamegraphs) | `cargo install samply` |
| [tokio-console](https://github.com/tokio-rs/console) | Async task scheduling, lock contention | `cargo install tokio-console` |
| [hyperfine](https://github.com/sharkdp/hyperfine) | Command-line benchmarking | `brew install hyperfine` |
| Chrome DevTools | JS rendering, memory, layout | Built into Tauri webview |
| [Solid DevTools](https://github.com/thetarnav/solid-devtools) | SolidJS signal/memo reactivity graph | Browser extension |

All tools are installed by `scripts/perf/setup.sh`.

## Rust Backend

### CPU Flamegraph

```bash
scripts/perf/record-cpu.sh --duration 60
```

Builds a release binary with debug symbols, records under samply for 60 seconds, saves a JSON profile. Open the result with:

```bash
samply load scripts/perf/results/cpu-YYYYMMDD-HHMMSS.json
```

**What to look for:**
- Functions with wide bars = high cumulative CPU time
- `std::process::Command` in hot paths = subprocess forks
- `serde_json::to_value` / `serde_json::to_string` = serialization overhead
- `parking_lot::Mutex::lock` = contention

### Tokio Runtime Inspector

```bash
scripts/perf/record-tokio.sh
```

Builds with the `tokio-console` Cargo feature and launches both the app and the console UI. The console shows live stats for every Tokio task.

**What to look for:**
- Tasks with high "busy" time = CPU-bound work on the async executor
- Tasks with high "idle" time = blocked on I/O or lock contention
- Tasks stuck in "waiting" = possible deadlock
- Many short-lived spawn_blocking tasks = check if batching would help

**Note:** This uses a debug build. Timing numbers are not representative of production performance, but relative proportions and task scheduling patterns are valid.

### Building with tokio-console manually

```bash
cd src-tauri
RUSTFLAGS="--cfg tokio_unstable" cargo build --features tokio-console
```

Then run the binary and connect tokio-console separately:

```bash
tokio-console
```

The console subscriber listens on `127.0.0.1:6669` by default.

## IPC Latency

```bash
scripts/perf/bench-ipc.sh                     # auto-detect repo
scripts/perf/bench-ipc.sh /path/to/repo 50    # 50 iterations
```

Measures round-trip latency for key git commands via the HTTP API on port 9877. Reports p50, p95, and mean for each endpoint.

**Endpoints measured:**
- `repo_info` (cached after first call, 5s TTL)
- `git_panel_context` (cached, 5s TTL)
- `diff_stats`, `changed_files`, `branches`
- `recent_commits`, `stash_list`, `remote_url`

**Interpreting results:**
- p50 < 5ms for cached endpoints = healthy
- p50 < 50ms for uncached git commands = healthy
- p95 > 200ms = investigate (large repo? slow disk? lock contention?)

To measure cold vs warm cache, run bench-ipc.sh twice in quick succession: the first run hits cache misses, the second should show cache hits.

## PTY Throughput

```bash
scripts/perf/bench-pty.sh       # 10MB default
scripts/perf/bench-pty.sh 50    # 50MB stress test
```

Creates a PTY session, blasts data through it, and measures throughput in MB/s. Tests the full pipeline: PTY read -> UTF-8 decode -> escape processing -> VT100 parse -> Tauri event emit.

**If the API doesn't support session creation**, the script prints manual commands to run in an existing terminal tab for the same measurement.

## Frontend

### Performance Recording

1. Open DevTools in TUICommander: `Cmd+Shift+I`
2. Go to **Performance** tab
3. Click Record
4. Exercise the scenario for 10-30 seconds
5. Stop recording

**What to look for:**
- Long Tasks (>50ms red bars) = jank
- Layout/Recalculate Style = CSS forcing reflow
- `requestAnimationFrame` gaps = dropped frames
- Frequent minor GC = allocation pressure

### Memory Profiling

Run `scripts/perf/snapshot-memory.sh` for detailed scenario instructions. Key scenarios:

1. **Terminal memory** — open/close 5 terminals, compare heap snapshots
2. **Panel leak check** — open/close Settings/Activity/Git panels 10x
3. **Long-running session** — compare snapshots at 0/10/20 minutes

**Expected baselines:**
- Each terminal: ~1.6MB heap (10k scrollback lines at 80 cols)
- Panel open/close cycle: <500KB retained after GC
- 20-minute session: sub-linear growth (not linear)

### SolidJS Reactivity

Install [Solid DevTools](https://github.com/thetarnav/solid-devtools) browser extension. In the devtools panel:

- Check how many times each `createMemo` re-evaluates
- Find effects with unexpectedly high execution counts
- Trace which signal changes trigger cascading updates

Key areas to watch:
- `terminalsStore` updates propagating to StatusBar/TabBar/SmartButtonStrip
- `debouncedBusy` signal reactivity scope
- `githubStore` polling triggering re-renders in unrelated components

## Profiling Scenarios

### Scenario 1: Startup Performance

```bash
scripts/perf/record-cpu.sh --duration 30
```

Open the app, wait for it to fully load, open DevTools Performance tab. Measure time-to-interactive.

**Target:** < 2s from launch to first terminal ready.

### Scenario 2: Multi-Terminal Steady State

1. Open 5 terminal tabs
2. Run an AI agent in 2 of them
3. Record CPU + memory for 2 minutes
4. Check: is CPU usage stable? Is memory growing?

### Scenario 3: Git-Heavy Workflow

1. Open a large repo (>1000 commits, >50 branches)
2. Open the Git panel
3. Switch branches
4. Run `bench-ipc.sh` against this repo

### Scenario 4: High-Throughput Output

In a terminal tab:
```bash
dd if=/dev/urandom bs=1024 count=10240 | base64    # ~14MB of random base64
yes | head -n 500000                                 # ~2MB of repetitive data
find / -type f 2>/dev/null                           # realistic filesystem output
```

Monitor CPU usage and app responsiveness during output.

## Comparing Results Across Sessions

Results accumulate in `scripts/perf/results/`:

```
results/
  ipc-20260328-143000.txt    # IPC latency run
  ipc-20260330-091500.txt    # After optimization
  cpu-20260328-150000.json   # CPU flamegraph
  pty-throughput.log          # PTY throughput history (appended)
```

Compare IPC results:
```bash
diff scripts/perf/results/ipc-{before,after}.txt
```

The PTY throughput log is append-only — each run adds a line for trend tracking.

## Architecture Reference

The profiling targets map to these code areas:

| Layer | Key files | What to measure |
|-------|-----------|-----------------|
| Tauri commands | `src-tauri/src/git.rs` | spawn_blocking overhead, subprocess latency |
| PTY pipeline | `src-tauri/src/pty.rs` | Read buffer throughput, event emission rate |
| IPC serialization | `src-tauri/src/pty.rs`, `git.rs` | JSON payload sizes, serde time |
| State management | `src/stores/terminals.ts` | Signal propagation scope, batch effectiveness |
| Rendering | `src/components/Terminal/Terminal.tsx` | xterm.js write batching, WebGL atlas rebuilds |
| Polling | `src/hooks/useAgentPolling.ts`, `src/stores/github.ts` | Interval frequency, IPC calls per tick |
| Bundle | `vite.config.ts` | Chunk sizes, initial parse/eval time |
