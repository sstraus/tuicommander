# Whole-backend heap profiling

`heap-profile` is an **opt-in diagnostic Cargo feature**. It selects the prefixed
`tikv-jemallocator` global Rust allocator in the shared backend library, so the
desktop, `tuicommander-agent-test`, and `tuic-remote` entry points use it. It does
not interpose C/Objective-C malloc. Default builds and Windows (including MSVC)
keep the system allocator. Do not adopt it for memory savings based on profiles.

## Build and start

Use a linked worktree. Never replace or restart the daily application for a
capture. On macOS run this isolated preset (choose an unused port and instance):

```sh
make agent-test AGENT_TEST_DRY_RUN=1 AGENT_TEST_INSTANCE=agent-test-heap AGENT_TEST_PORT=19879 AGENT_TEST_HEAP_PROFILE=1
_RJEM_MALLOC_CONF='abort_conf:true,prof:true,prof_active:false,lg_prof_sample:19' \
  make agent-test AGENT_TEST_INSTANCE=agent-test-heap AGENT_TEST_PORT=19879 AGENT_TEST_HEAP_PROFILE=1
```

Run the long-lived launcher in the background. Stop **that instance** with
`make agent-test-stop AGENT_TEST_INSTANCE=agent-test-heap`. The launcher uses mbx
and the build slot. Ordinary diagnostic desktop builds add `--features heap-profile`
to `pnpm tauri build`; use the normal frontend embedding flow. For headless Linux,
install the platform's `libunwind` development package, then use the build slot
with `cargo build --manifest-path src-tauri/Cargo.toml --bin tuic-remote --no-default-features --features heap-profile`.
The Cargo feature is also valid with desktop builds on Linux. It is unavailable
on other targets; the diagnostic endpoint reports an error there.

Profiling must be enabled **at process start**. With the prefixed allocator the
variable is `_RJEM_MALLOC_CONF`, not the system allocator's configuration.
`prof:true,prof_active:false` initializes the profiler without sampling until
explicit activation. Existing allocations made while inactive cannot be recovered
later. For a complete startup baseline use `prof_active:true` in the isolated run.
`lg_prof_sample:19` samples about every 512 KiB; profiles estimate live bytes,
not a census. Smaller intervals cost more CPU and memory. Leave
`prof_thread_active_init:true` (the default), so newly created threads participate.
Do not enable `prof_gdump`, `prof_final`, automatic intervals, debug, or validation.

The build forces upstream's `profiling_libunwind` method, rather than the GCC
unwinder with its known multithreaded livelock. macOS provides unwind symbols in
libSystem; Linux requires libunwind. See the
[upstream feature rationale](https://docs.rs/crate/tikv-jemalloc-sys/0.7.1+5.3.1-0-g81034ce1f1373e37dc865038e1bc8eeecf559ce8/source/README.md).
No frame-pointer or global compiler overrides are needed. Verify actual stacks
on the platform before trusting a capture: unwinding support alone is not proof
that every Rust, native, or JIT frame resolves.

## Control and capture

Use the isolated Unix socket from the dry run, or authenticated TCP on its port.
The existing HTTP diagnostics control plane is shared by desktop and headless;
there is no additional Tauri IPC command to drift from it.

```sh
curl --unix-socket "$socket" http://localhost/diagnostics/heap
curl --unix-socket "$socket" -H 'Content-Type: application/json' \
  -d '{"action":"activate"}' http://localhost/diagnostics/heap
curl --unix-socket "$socket" -H 'Content-Type: application/json' \
  -d '{"action":"dump","path":"/absolute/path/before.heap"}' http://localhost/diagnostics/heap
# Exercise throwaway repositories, searches, PTYs, logs and application state.
# Capture after.heap using the same endpoint, then deactivate.
curl --unix-socket "$socket" -H 'Content-Type: application/json' \
  -d '{"action":"deactivate"}' http://localhost/diagnostics/heap
curl --unix-socket "$socket" http://localhost/diagnostics/memory
```

GET confirms `compiled`, `enabled`, `active`, new/current-thread sampling flags,
allocator debug assertions, sample interval, PID, executable,
and the process-start configuration. Unsupported builds, missing `prof:true`,
invalid filenames and failed writes produce HTTP 400 with an explicit `error`.
A dump is synchronous on a blocking worker; its response includes path, file size
and elapsed milliseconds. It does not stop sampling. Deactivation stops future
sampling, not retention of already sampled live allocations. Choose unique paths:
jemalloc may overwrite an existing dump. The server writes paths on its host.

## Preserve symbols before rebuilding

Save the **exact** executable returned by GET, its SHA-256, source commit and
working-tree diff, Cargo.lock, toolchain versions, configuration and the raw
captures together in a private directory under `~/Gits/.tmp/heap-<date>/`.
On macOS copy the executable, then run **before any build or cache eviction**:

```sh
(cd "$(realpath src-tauri/target)" && dsymutil /absolute/saved-executable -o /absolute/saved-executable.dSYM)
```

The development build's unpacked DWARF refers to both relative and absolute mbx
object paths. Running from the repository root can produce a matching UUID yet
omit most symbols; `--oso-prepend-path` can break absolute paths. Require no missing
object warnings, verify matching UUIDs with `dwarfdump --uuid`, and resolve a real
allocation frame before trusting the dSYM. Keep it next to the saved executable.
On Linux preserve the unstripped ELF (or its exact separate debug file and build
ID). Release builds strip debug info by default: explicitly produce/preserve
symbols before stripping; an unrelated rebuild cannot symbolize an old capture.

```sh
jeprof --show_bytes --text saved-executable after.heap
jeprof --show_bytes --text --base=before.heap saved-executable after.heap
jeprof --show_bytes --lines --text saved-executable after.heap
```

Keep complete stack paths: inclusive rows overlap and must not be added together.
The sampled `t*` records in a heap_v2 dump are per-stack live object/byte counts;
`jeprof` corrects sampling bias using the interval in the header. On macOS verify
ASLR mappings and image load addresses; use `atos -o saved-executable.dSYM/Contents/Resources/DWARF/<name> -l <load-address> <runtime-address>`
for a raw frame. In the measured macOS run, automatic `jeprof` symbol labels
were missing or incorrect despite correct numeric totals. Relocate raw PCs using
the dump's `MAPPED_LIBRARIES` load address; LLDB `image lookup -v -a` on the saved
binary plus complete dSYM resolves static PCs (`runtime - load + Mach-O vmaddr`).
Check addresses against the image range before resolving. A resolved Rust function **and file:line** is the minimum symbol
check. Keep unsymbolized frames visible as unknowns.

## Interpret memory and overhead

`GET /diagnostics/memory` adds `allocator` to the existing report:

| Field | Meaning |
|---|---|
| `allocated_bytes` | jemalloc's current allocated bytes, including Rust allocations outside AppState |
| `active_bytes` | pages backing those allocations; includes size-class/page slack |
| `resident_bytes` | allocator metadata plus active/dirty pages; an allocator estimate, not OS footprint |
| `retained_bytes` | unused virtual mappings retained for reuse; **not** live object bytes or physical footprint |
| `phys_footprint_bytes` | macOS backend physical footprint including compressed memory; null elsewhere |
| `malloc_*`, `malloc_scope` | native system malloc zone counters; default zone only for `malloc_bytes_in_use`, all enumerable zones for `malloc_large_blocks` |

These are overlapping views, not quantities to add. `accounted_bytes` describes
selected AppState holders and can overlap the Rust profile. Native system malloc
statistics cannot explain the prefixed Rust heap. Conversely jemalloc excludes
native malloc, mapped model files, stacks, executable pages, GPU allocations and
WebContent processes. Linux currently has no physical-footprint equivalent in
this endpoint: record `/proc/<pid>/smaps_rollup` separately and label RSS/PSS as such.

For a representative run, record backend footprint, associated WebContent PIDs
and their separate footprint, native counters and allocator metrics before,
during and after sampling. Record PIDs before launch and creation times, then
verify associated processes disappear when your isolated instance stops. Shared
resource coalitions and responsible PIDs can be inherited from the orchestrator:
they are not sufficient ownership evidence. Keep ambiguous processes separate;
a process-name match alone is insufficient.
Do not sum RSS with footprint or add allocator counters to OS process totals.
Never assign the user's reported total to Rust without a matching capture.

Repeat the same throwaway workload with sampling off/on/off. Record elapsed work,
dump latency/file size, metric changes, cleanup, and retained memory after cleanup.
Keep the allocator and workload fixed; this measures profiler cost, **not** the
system-versus-jemalloc adoption question. Asynchronous indexing and WebKit caches
can survive the workload, so report run order and these limits. A separate matched
whole-backend experiment is required before claiming reduced memory.

Profiles contain code addresses, binary/source paths and allocation/thread metadata;
application logs and throwaway data can still contain sensitive information.
Keep raw captures, executable, symbols and process snapshots local; share a redacted
numeric/stack summary. Do not capture the daily instance or read its credentials.

## Recorded application experiment

See [the measured isolated run](heap-profiling-results.md) for source/symbol identity,
retained allocation stacks, process split, overhead and unresolved attribution.
