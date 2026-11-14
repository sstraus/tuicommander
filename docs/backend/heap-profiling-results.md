# Isolated application heap experiment — 2026-11-11

This is a diagnostic demonstration, not attribution of the reported 3 GB in the
user's daily instance. That instance was neither restarted nor captured.

## Reproduction and identity

The isolated `make agent-test` instance `agent-test-2959`, port 19879, used the
whole desktop backend with `heap-profile`, on macOS 27.2 (26B123), arm64.
Rust 1.99.1 (58cf8f9c3), Cargo 1.99.1 (5f94df478). Source base
`f1bf630ac41da9bd472824b8d16620f0cb99861f` plus the preserved `source.diff` and
`Cargo.lock` identify the uncommitted diagnostic build. The implementation and
report are committed together; rebuilding that commit is not the captured binary.

Private artifacts are in `~/Gits/.tmp/heap-2959/`: exact executable, `metadata.json`,
source diff, lockfile, three raw heaps, metric/process snapshots, workload scripts,
`analysis.json`, raw/decoded symbol addresses and complete dSYM. Executable SHA-256:
`662c6277e6664d95ff839b119ebc5c4b7ed49e7b6aa4c8b1b1297d80f0383e25`.
The complete dSYM is `tuicommander-agent-test-complete.dSYM`, 380,978,373 bytes,
UUID `5443EF31-E37E-3DBB-B91B-318635AD7CDA`. Its generation had zero missing-object
warnings. Earlier incomplete dSYMs are not valid analysis inputs.

Process-start `_RJEM_MALLOC_CONF` was
`abort_conf:true,prof:true,prof_active:false,lg_prof_sample:19`.
Status verified compiled/enabled profiling, initially inactive sampling, both
thread sampling flags true, and allocator debug assertions false. The launcher
keeps this variable out of rustc/build tools and supplies it only to the app.

Three repeated phases used fresh throwaway repositories (120 Rust files with
180 functions each), content search/index construction and three 40×128 PTYs
emitting 4,000 ANSI lines each. Sampling was off/on/off in the same process.
Before/after sampling dumps bracketed the middle workload; a third dump followed
session deletion. All sessions were deleted, sampling deactivated and the recorded
isolated app PID stopped via `make agent-test-stop`. No browser was launched.

## Physical footprint and native memory

All values below are bytes, with process physical footprint obtained independently
through macOS process resource usage. These are separate processes, not allocator
subtotals to add to the backend.

| Process | PID | Initial footprint | Final footprint |
|---|---:|---:|---:|
| Rust backend plus in-process native frameworks | 68235 | 143,132,184 | 315,918,064 |
| Associated WebContent | 68308 | 130,499,568 | 163,120,136 |
| Associated WebKit GPU | 68301 | 24,494,920 | 239,272,800 |
| Associated WebKit networking | 68312 | 7,913,928 | 7,913,928 |

The WebKit group appeared immediately after this isolated launch, remained stable
across captures and disappeared with its shutdown. Association is based on this
controlled lifetime, not direct parentage: XPC processes have launchd parentage.
Coalition/responsible-PID checks were inconclusive because launch inherited the
orchestrator's identity. Other concurrent WebKit processes are excluded; lifetime
correlation does not establish exclusive ownership under concurrent app activity.

The final default native malloc-zone `bytes_in_use` was 8,779,040. It excludes
prefixed jemalloc and other zones. A separate `footprint` snapshot (315,983,600
total at a slightly later time) observed dirty WebKit Malloc 24,018,944, Malloc
Small 16,859,136, malloc metadata 1,523,712 and stack 2,277,376. These native
categories overlap the backend total and are not additive to it or to malloc
zone counters. App-Specific Tag 15 had 262,209,536 dirty bytes; its tag alone does
not prove Rust ownership. Physical footprint and resident/dirty counters differ.

## Heap captures and resolved stacks

`before.heap` was taken immediately after activation: zero sampled live bytes.
`after.heap` estimates 63,007,165 live bytes allocated during the sampling window;
`cleaned.heap` estimates 59,021,569 after session deletion. Values use upstream's
Poisson correction per stack (`size / (1 - exp(-(size/count)/524288))`), rounded
to bytes; summing individually rounded stacks can differ by a few bytes.
At after-dump time jemalloc allocated 188,930,880 bytes, so this window explains
approximately one third of then-live allocator bytes. It does not census earlier
startup/off-phase allocations or explain the entire physical footprint.

The largest distinct retained stacks in `after.heap` are:

| Estimated live bytes | Allocation path / resolved source |
|---:|---|
| 35,230,333 | `Row<Cell>::new` → `Grid::increase_scroll_limit` → `Grid::scroll_up_with` → `TerminalGrid::process`; `patches/alacritty_terminal/src/grid/row.rs:55`, `grid/mod.rs:434`, `crates/tuic-terminal/src/terminal_grid.rs:798` |
| 6,416,794 | `mcp_http::session::register_pty_session`; `src/state.rs:1293` (inlined allocation location) |
| 6,416,794 | `TerminalGrid::new` → `VtLogBuffer::new` → `AppState::new_vt_log_buffer`; `src/state.rs:4624` |
| 6,089,175 | `BuildTokenizer::tokenize` → `EmbeddingIndex::build`; `patches/bm25/src/embedder.rs:227` |
| 2,138,344 | `ContentIndex::build` allocation (inlined `core::hint::black_box` location) |
| 2,097,472 | `TerminalGrid::extract_log_line` → `read_scrollback_log_lines`; `crates/tuic-terminal/src/terminal_grid.rs:2003` |
| 1,573,032 | Separate `extract_log_line` allocation stack → `ChunkProcessor::process_safe_chunk`; `src/pty.rs:1970` |

Rows are distinct full stacks, not overlapping inclusive totals. Optimized inline
DWARF sometimes reports a callee's library location; those locations are labelled
rather than asserted to be the allocation callsite. Cached dependency DWARF
contains original worktree prefixes; use relative source paths with the preserved
revision. Raw addresses were checked against the captured backend mapping at
`0x104ab4000`, relocated and resolved using the exact binary and complete dSYM.
For example runtime `0x107a1d1e7` resolves to `TerminalGrid::process`,
`terminal_grid.rs:798:28`. Automatic macOS jeprof labels were unreliable; raw-PC
LLDB/atos verification supplies the source evidence.

The top three stacks retain the same estimates after cleanup. The final diagnostics
report zero sessions but ten retained VT-log grids (110,770,176 accounted bytes),
output buffers (20,971,520), VT-log lines (5,277,384), raw rings (4,323,774) and
three content indexes. This identifies retained holders after session deletion;
it does not establish whether that retention is a defect or its lifetime policy.
The profile excludes native malloc, mmap/model pages, stacks, image pages, GPU and
WebContent. No unexplained residual is computed by subtracting overlapping views.

## Overhead and retention

| Phase | Backend footprint | jemalloc allocated | active | resident | retained virtual |
|---|---:|---:|---:|---:|---:|
| Initial, off | 143,148,568 | 42,506,120 | 67,026,944 | 128,794,624 | 107,708,416 |
| First off workload | 216,467,064 | 114,846,896 | 144,244,736 | 220,872,704 | 120,029,184 |
| Off, cleaned | 215,238,264 | 109,563,992 | 139,870,208 | 215,826,432 | 125,075,456 |
| On workload | 270,075,560 | 174,310,592 | 208,535,552 | 286,392,320 | 123,551,744 |
| On, after dump | 275,629,736 | 188,930,880 | 223,494,144 | 292,143,104 | 124,108,800 |
| On, cleaned | 275,072,680 | 172,639,560 | 207,929,344 | 287,981,568 | 128,303,104 |
| Last off workload | 329,139,952 | 233,478,736 | 268,419,072 | 342,114,304 | 159,449,088 |
| Final, off and cleaned | 315,918,064 | 227,908,216 | 263,061,504 | 325,255,168 | 176,308,224 |

Dump wall times were 0.861/0.826/1.062 ms and sizes 129,938/133,551/132,984 bytes.
Off/on/off workload wall times were 0.515/0.621/0.834 s; measured backend CPU
increments were 6.670/4.619/5.193 ms. The on phase was +20.5% versus first off and
−25.5% versus final off, inside the off-phase range. One sequence with asynchronous
PTY processing/indexing cannot establish stable profiler CPU overhead. The timing
covers workload submission/completion markers, not a quiescent backend CPU bench.

Memory continues growing with retained grids, indexes and framework caches across
phases, so the table measures operational cost and retained state, not isolated
profiler bookkeeping. `retained_bytes` is unused virtual mapping capacity, not
live objects or physical memory. Deactivation does not free sampled live objects.
No system-versus-jemalloc memory savings are claimed. Linux was tested for controls
and allocator metrics; macOS for the live desktop capture. Windows selection and
headless shared-library wiring were inspected, not separately built here.

## Targeted validation

Default rb filter `test(heap_profile)`: 3 passed (system selection, HTTP unavailable
errors, metric provenance), Nextest `826f0a74-a0eb-4b8b-8cb5-bdee06de142d`.
Feature rb same filter with `--features heap-profile` and process-start
`_RJEM_MALLOC_CONF=prof:true,prof_active:false`: 2 passed (real allocation,
activation/deactivation, valid dump plus invalid-path/NUL errors; provenance),
Nextest `c704b3d6-2952-4b8e-9f08-afac0bd3f0d9`. Both used
`build-slot.sh --remote scripts/with-test-tmp.sh ... cargo nextest run
--manifest-path src-tauri/Cargo.toml --lib -E 'test(heap_profile)'` on the source
diff preserved with the executable; no production/test code changed afterwards.
The initial feature link failed for missing `-lunwind`; the passing rerun followed
installation of libunwind-dev on rb. This is a documented platform prerequisite.

The two targeted Python tests for opt-in feature selection and keeping profiler
configuration out of the build subprocess passed. Native isolated startup, verified
status, actual dump files and explicit HTTP 400 for a missing dump parent complete
the runtime boundary evidence. Full-suite/clippy/headless-build gates belong to
batch validation and were not duplicated here.
