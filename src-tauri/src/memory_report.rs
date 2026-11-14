//! Where the process's memory actually is.
//!
//! On 2026-09-08 the backend reached a **40.7 GB** physical footprint after 13
//! hours (`JetsamEvent-2026-09-08-214808` names it as the machine's largest
//! process; `vmmap` reports 1,453,700 live allocations in the default malloc
//! zone at 1% fragmentation, and `leaks` finds only 47 MB of it unreferenced).
//! So it is not fragmentation and not a classic leak: some structure the app
//! still holds a reference to grew and was never pruned.
//!
//! Nothing could say which one. Every candidate had to be excluded by reading
//! code and by re-running the workload on a second instance, and a 40 GB process
//! that macOS marks as not-debuggable cannot be asked what it is holding — the
//! evidence dies with the incident.
//!
//! This report is the answer to that. One request names the structure: entry
//! counts for every map that grows with sessions or clients, and measured bytes
//! for the four that hold the payloads. It is deliberately on-demand and not on
//! the diagnostics tick — walking a session's log lines is cheap once and is not
//! something to do every five seconds forever.

use crate::state::AppState;
use std::sync::Arc;

/// One growable structure, as reported.
///
/// `bytes` is `None` where a byte count would cost more than it is worth: for
/// those the entry count is the signal, because their values are small and
/// fixed-size. A structure that is big *per entry* always carries bytes.
#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct MapReport {
    pub(crate) name: &'static str,
    pub(crate) entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bytes: Option<usize>,
}

impl MapReport {
    fn counted(name: &'static str, entries: usize) -> Self {
        Self {
            name,
            entries,
            bytes: None,
        }
    }

    fn measured(name: &'static str, entries: usize, bytes: usize) -> Self {
        Self {
            name,
            entries,
            bytes: Some(bytes),
        }
    }
}

/// The process's physical footprint in bytes — resident *plus* compressed.
///
/// `ps` RSS is the wrong number and was misleading during the incident: it read
/// 0.52 GB while the process held 40 GB, because everything allocated and never
/// touched again ends up in the memory compressor. The compressed pages are what
/// the kernel charges against the machine, and what triggers the jetsam sweep.
#[cfg(target_os = "macos")]
pub(crate) fn phys_footprint_bytes() -> Option<u64> {
    // `ri_phys_footprint` is the same field `footprint(1)` prints. Read through
    // `proc_pid_rusage` rather than `task_info(TASK_VM_INFO)` because libc
    // exposes this one and the mach struct would mean a new dependency for a
    // single u64.
    let mut info: libc::rusage_info_v0 = unsafe { std::mem::zeroed() };
    let rc = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as libc::c_int,
            libc::RUSAGE_INFO_V0,
            std::ptr::addr_of_mut!(info).cast(),
        )
    };
    (rc == 0).then_some(info.ri_phys_footprint)
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn phys_footprint_bytes() -> Option<u64> {
    None
}

/// Live allocations in the default malloc zone, and the bytes they hold.
///
/// This is the number that separates "a structure in `AppState` grew" from
/// "the heap grew somewhere this report cannot see": when `footprint` climbs
/// and `accounted_bytes` does not, `blocks_in_use` says whether the memory is
/// in the system default malloc zone. A prefixed Rust allocator is separate.
///
/// It is the same pair `vmmap` prints as ALLOCATION COUNT and BYTES ALLOCATED,
/// read from inside the process. `vmmap` itself is not an option: it suspends
/// the target for as long as it takes to walk every region — measured at **22
/// seconds** on this app, which is a visible UI freeze. `malloc_zone_statistics`
/// reads counters the allocator already maintains and returns immediately.
#[cfg(target_os = "macos")]
pub(crate) fn malloc_zone_stats() -> Option<(u64, u64)> {
    // Declared here rather than taken from `libc`, which exposes neither the
    // struct nor the two calls. Layout is `<malloc/malloc.h>`.
    #[repr(C)]
    #[derive(Default)]
    struct MallocStatistics {
        blocks_in_use: libc::c_uint,
        size_in_use: libc::size_t,
        max_size_in_use: libc::size_t,
        size_allocated: libc::size_t,
    }
    unsafe extern "C" {
        fn malloc_default_zone() -> *mut libc::c_void;
        fn malloc_zone_statistics(zone: *mut libc::c_void, stats: *mut MallocStatistics);
    }

    let mut stats = MallocStatistics::default();
    unsafe {
        let zone = malloc_default_zone();
        if zone.is_null() {
            return None;
        }
        malloc_zone_statistics(zone, &mut stats);
    }
    Some((stats.blocks_in_use as u64, stats.size_in_use as u64))
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn malloc_zone_stats() -> Option<(u64, u64)> {
    None
}

/// Blocks at least this big are listed apart from the heap total: one of them
/// is a whole structure (a decoder state, an index, a buffer), not an
/// accumulation of small allocations.
pub(crate) const LARGE_BLOCK_BYTES: u64 = 8 * 1024 * 1024;

/// Live malloc blocks of at least [`LARGE_BLOCK_BYTES`]: how many, and their
/// combined size.
///
/// 2026-09-29: `malloc_bytes_in_use` was 912 MB against 260 MB accounted, and the
/// only way to see that four 82.76 MB blocks were the difference was `heap
/// -addresses all` on a 983k-node heap, which suspends the process for seconds.
/// The totals cannot say whether the gap is one structure or a million small
/// allocations; this pair can, and it reads the zones from inside the process.
///
/// Each zone's allocator lock is held for the walk — the protocol `fork` uses
/// (`force_lock`/`force_unlock`) — so a concurrent resize of the allocator's own
/// tables cannot be observed half-done. Other threads' `malloc` calls wait for
/// it, and the recorder therefore never allocates.
///
/// `None` means the census is not available or the walk failed part-way; a
/// partial count is never returned as if it were the whole heap.
pub(crate) fn large_malloc_blocks() -> Option<(u64, u64)> {
    #[cfg(target_os = "macos")]
    {
        walk_zones().map(|(count, bytes, _)| (count, bytes))
    }
    #[cfg(not(target_os = "macos"))]
    None
}

/// The walk behind [`large_malloc_blocks`], plus the longest time any one zone
/// stayed locked: that is how long every allocating thread waited.
#[cfg(target_os = "macos")]
fn walk_zones() -> Option<(u64, u64, std::time::Duration)> {
    // Declared here rather than taken from `libc`, which exposes none of it.
    // Layouts are `<malloc/malloc.h>`; only the prefix this walk reads is spelled.
    #[repr(C)]
    struct VmRange {
        address: usize,
        size: usize,
    }
    type Recorder =
        unsafe extern "C" fn(u32, *mut libc::c_void, libc::c_uint, *mut VmRange, libc::c_uint);
    type Lock = unsafe extern "C" fn(*mut Zone);
    #[repr(C)]
    struct Introspection {
        enumerator: Option<
            unsafe extern "C" fn(
                u32,
                *mut libc::c_void,
                libc::c_uint,
                usize,
                *mut libc::c_void,
                Recorder,
            ) -> libc::c_int,
        >,
        good_size: *const libc::c_void,
        check: *const libc::c_void,
        print: *const libc::c_void,
        log: *const libc::c_void,
        force_lock: Option<Lock>,
        force_unlock: Option<Lock>,
    }
    #[repr(C)]
    struct Zone {
        reserved1: *const libc::c_void,
        reserved2: *const libc::c_void,
        size: *const libc::c_void,
        malloc: *const libc::c_void,
        calloc: *const libc::c_void,
        valloc: *const libc::c_void,
        free: *const libc::c_void,
        realloc: *const libc::c_void,
        destroy: *const libc::c_void,
        zone_name: *const libc::c_char,
        batch_malloc: *const libc::c_void,
        batch_free: *const libc::c_void,
        introspect: *const Introspection,
    }
    #[derive(Default)]
    struct Census {
        count: u64,
        bytes: u64,
    }
    unsafe extern "C" {
        fn malloc_get_all_zones(
            task: u32,
            reader: *mut libc::c_void,
            addresses: *mut *mut usize,
            count: *mut libc::c_uint,
        ) -> libc::c_int;
    }
    const MALLOC_PTR_IN_USE_RANGE_TYPE: libc::c_uint = 1;

    // `memory_reader_t`: how an enumerator reads the target task. For this
    // process the address is the memory, so the reader hands it back.
    // 2026-10-02: Apple's zones tolerate a null reader for the own task, but
    // CoreAudio's caulk zone calls it unconditionally and jumped to address 0.
    unsafe extern "C" fn in_process_reader(
        _task: u32,
        address: usize,
        _size: usize,
        local: *mut *mut libc::c_void,
    ) -> libc::c_int {
        unsafe { *local = address as *mut libc::c_void };
        0 // KERN_SUCCESS
    }

    unsafe extern "C" fn record(
        _task: u32,
        context: *mut libc::c_void,
        _kind: libc::c_uint,
        ranges: *mut VmRange,
        count: libc::c_uint,
    ) {
        // No allocation, no panic, no log: the zone locks are held.
        let census = unsafe { &mut *context.cast::<Census>() };
        for i in 0..count as usize {
            let size = unsafe { (*ranges.add(i)).size } as u64;
            if size >= LARGE_BLOCK_BYTES {
                census.count += 1;
                census.bytes += size;
            }
        }
    }

    #[allow(deprecated)] // libc's own accessor; `mach2` would be a new dependency
    let task = unsafe { libc::mach_task_self() };
    let mut addresses: *mut usize = std::ptr::null_mut();
    let mut zone_count: libc::c_uint = 0;
    let reader = in_process_reader as *mut libc::c_void;
    let rc = unsafe { malloc_get_all_zones(task, reader, &mut addresses, &mut zone_count) };
    if rc != 0 || addresses.is_null() {
        return None;
    }

    let mut census = Census::default();
    let mut longest = std::time::Duration::ZERO;
    for i in 0..zone_count as usize {
        let zone = unsafe { *addresses.add(i) } as *mut Zone;
        let introspect = unsafe { zone.as_ref() }.map(|z| z.introspect)?;
        let Some(introspect) = (unsafe { introspect.as_ref() }) else {
            continue;
        };
        let (Some(enumerate), Some(lock), Some(unlock)) = (
            introspect.enumerator,
            introspect.force_lock,
            introspect.force_unlock,
        ) else {
            continue;
        };
        let started = std::time::Instant::now();
        let rc = unsafe {
            lock(zone);
            let rc = enumerate(
                task,
                std::ptr::addr_of_mut!(census).cast(),
                MALLOC_PTR_IN_USE_RANGE_TYPE,
                zone as usize,
                reader,
                record,
            );
            unlock(zone);
            rc
        };
        longest = longest.max(started.elapsed());
        if rc != 0 {
            return None;
        }
    }
    Some((census.count, census.bytes, longest))
}

/// Just the bytes half of [`malloc_zone_stats`], for callers weighing a
/// structure across its own construction.
pub(crate) fn malloc_bytes_in_use() -> Option<u64> {
    malloc_zone_stats().map(|(_, bytes)| bytes)
}

/// Every structure in `AppState` that grows with sessions, clients or repos.
///
/// Sorted by bytes and then by entries, so the culprit is the first row rather
/// than something to be found by reading all of them.
pub(crate) fn maps(state: &Arc<AppState>) -> Vec<MapReport> {
    let sm = &state.session_maps;
    let grid = &state.grid;

    // Measured: these four are the only ones whose *values* can be large.
    // The two halves of a vt log buffer are reported apart: the grid has a hard
    // ceiling (scrollback × columns × cell), the captured log lines do not,
    // because a span holds whatever the PTY printed. Summed, a runaway log is
    // indistinguishable from a terminal filling its scrollback.
    let (vt_log_bytes, vt_grid_bytes) = grid
        .vt_log_buffers
        .iter()
        .map(|e| {
            let buf = e.value().lock();
            (buf.log_bytes(), buf.grid_bytes())
        })
        .fold((0, 0), |(l, g), (dl, dg)| (l + dl, g + dg));
    let raw_ring_bytes: usize = grid
        .pty_raw_rings
        .iter()
        .map(|e| e.value().lock().len())
        .sum();
    let output_bytes: usize = sm
        .output_buffers
        .iter()
        .map(|e| e.value().lock().capacity())
        .sum();
    let inbox_bytes: usize = state
        .agent_inbox
        .iter()
        .map(|e| e.value().iter().map(|m| m.content.len()).sum::<usize>())
        .sum();
    // Measured, not counted: an index is the heaviest per-repo structure the
    // app holds and it is released only when the repo is retired, so a session
    // spent across many repos accumulates them. The count alone said nothing —
    // four indices and four hundred megabytes read the same.
    let index_bytes: usize = state
        .content_indices
        .iter()
        .map(|e| e.value().read().approx_bytes())
        .sum();

    let mut out = vec![
        MapReport::measured("grid.vt_log_lines", grid.vt_log_buffers.len(), vt_log_bytes),
        MapReport::measured(
            "grid.vt_log_grids",
            grid.vt_log_buffers.len(),
            vt_grid_bytes,
        ),
        MapReport::measured(
            "grid.pty_raw_rings",
            grid.pty_raw_rings.len(),
            raw_ring_bytes,
        ),
        MapReport::measured("output_buffers", sm.output_buffers.len(), output_bytes),
        MapReport::measured("agent_inbox", state.agent_inbox.len(), inbox_bytes),
        MapReport::counted("sessions", sm.sessions.len()),
        MapReport::counted("session_states", sm.session_states.len()),
        MapReport::counted("pty_event_channels", sm.pty_event_channels.len()),
        MapReport::counted("messaging_channels", sm.messaging_channels.len()),
        MapReport::counted("ws_clients", state.ws_clients.len()),
        MapReport::counted("session_html_tabs", sm.session_html_tabs.len()),
        MapReport::counted("input_buffers", sm.input_buffers.len()),
        MapReport::counted("grid.channels_watch", grid.watch.len()),
        MapReport::counted("grid.gates", grid.gates.len()),
        MapReport::measured("content_indices", state.content_indices.len(), index_bytes),
        MapReport::counted("repo_watchers", state.repo_watchers.len()),
        MapReport::counted("dir_watchers", state.dir_watchers.len()),
        MapReport::counted("mcp.sessions", state.mcp.sessions.len()),
        MapReport::counted("peer_agents", state.peer_agents.len()),
        MapReport::counted("pending_injections", state.pending_injections.len()),
        MapReport::counted("event_bus_subscribers", state.event_bus.receiver_count()),
    ];
    out.sort_by_key(|m| {
        (
            std::cmp::Reverse(m.bytes.unwrap_or(0)),
            std::cmp::Reverse(m.entries),
        )
    });
    out
}

/// Holders that keep memory outside `AppState` and mostly outside the malloc
/// heap, so neither `maps` nor `malloc_bytes_in_use` can show them. Reported
/// apart from `accounted_bytes`: adding them there would break the comparison
/// with the heap.
#[cfg(feature = "dictation")]
pub(crate) fn model_holders(dictation: &crate::dictation::DictationState) -> Vec<MapReport> {
    use crate::dictation::model;
    if dictation.transcriber_arc.lock().is_none() {
        return Vec::new();
    }
    // The weights are read whole into memory, so the file size is the holder's
    // size (measured 2026-09-29: 1548.7 MiB mapped against a 1549.3 MiB file).
    let bytes = dictation
        .active_model
        .lock()
        .as_deref()
        .and_then(model::WhisperModel::from_name)
        .and_then(|m| std::fs::metadata(model::model_path(m)).ok())
        .map_or(0, |meta| meta.len() as usize);
    vec![MapReport::measured("dictation.whisper_model", 1, bytes)]
}

/// [`model_holders`] for the running app; empty where there is no dictation.
fn holders(state: &Arc<AppState>) -> Vec<MapReport> {
    #[cfg(feature = "dictation")]
    {
        use tauri::Manager;
        if let Some(app) = state.app_handle.read().as_ref()
            && let Some(dictation) = app.try_state::<crate::dictation::DictationState>()
        {
            return model_holders(&dictation);
        }
    }
    let _ = state;
    Vec::new()
}

/// Cheap threshold-crossing context: no payload locks or allocator zone walk.
pub(crate) fn summary(state: &Arc<AppState>) -> serde_json::Value {
    let (blocks, heap_bytes) = malloc_zone_stats().unzip();
    serde_json::json!({
        "allocator": crate::heap_profile::metrics(),
        "malloc_scope": "system default malloc zone only; excludes prefixed jemalloc and other zones",
        "malloc_blocks_in_use": blocks,
        "malloc_bytes_in_use": heap_bytes,
        "sessions": state.session_maps.sessions.len(),
        "vt_log_buffers": state.grid.vt_log_buffers.len(),
        "content_indices": state.content_indices.len(),
    })
}

/// The whole report, as the endpoint returns it.
pub(crate) fn report(state: &Arc<AppState>) -> serde_json::Value {
    let maps = maps(state);
    let accounted: usize = maps.iter().filter_map(|m| m.bytes).sum();
    let (blocks, heap_bytes) = malloc_zone_stats().unzip();
    let large = large_malloc_blocks();
    let (large_count, large_bytes) = large.unzip();
    serde_json::json!({
        "phys_footprint_bytes": phys_footprint_bytes(),
        // What the maps below explain. A footprint far above this is memory no
        // structure here owns — which is itself the finding, and says the next
        // suspect is outside `AppState`.
        "accounted_bytes": accounted,
        // Native system default-zone counters, not the whole Rust heap when
        // the diagnostic prefixed jemalloc allocator is selected.
        "allocator": crate::heap_profile::metrics(),
        "malloc_scope": "system default malloc zone only; excludes prefixed jemalloc and other zones",
        "malloc_blocks_in_use": blocks,
        "malloc_bytes_in_use": heap_bytes,
        // All blocks of `min_bytes` or more, including map-owned allocations.
        // This census is not attribution: never add it to `accounted_bytes`.
        "malloc_large_blocks": {
            "min_bytes": LARGE_BLOCK_BYTES,
            "may_overlap_accounted_bytes": true,
            // false: the walk failed or is unsupported, so count/bytes are null
            // rather than a low number that reads as a real answer.
            "complete": large.is_some(),
            "count": large_count,
            "bytes": large_bytes,
        },
        "maps": maps,
        // Memory held outside `maps` and outside the malloc heap (a loaded
        // model). Not part of `accounted_bytes`, which is compared to the heap.
        "holders": holders(state),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_idle_state_reports_every_map_at_zero() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let maps = maps(&state);
        assert!(
            maps.iter().all(|m| m.entries == 0),
            "a fresh AppState holds nothing: {maps:?}"
        );
    }

    #[cfg(feature = "dictation")]
    #[test]
    fn a_loaded_whisper_model_is_named_with_its_size_and_an_unloaded_one_is_absent() {
        // 2026-09-29: a 1.5 GiB model was resident and no report row named it;
        // 652 MB of the heap and all of the model were unexplained.
        let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
        let _guard = crate::config::set_config_dir_override(dir.path().to_path_buf());
        let model = tuic_dictation::model::WhisperModel::from_name("large-v3-turbo").unwrap();
        std::fs::create_dir_all(tuic_dictation::model::models_dir()).unwrap();
        std::fs::write(tuic_dictation::model::model_path(model), vec![0u8; 4096]).unwrap();

        let dictation = crate::dictation::DictationState::new();
        assert!(model_holders(&dictation).is_empty(), "nothing loaded yet");

        dictation.install_transcriber_locked(
            &mut dictation.transcriber_arc.lock(),
            &mut dictation.active_model.lock(),
            Arc::new(NoopTranscriber),
            "large-v3-turbo",
        );
        let holders = model_holders(&dictation);
        assert_eq!(holders.len(), 1, "{holders:?}");
        assert_eq!(holders[0].name, "dictation.whisper_model");
        assert_eq!(holders[0].entries, 1);
        assert_eq!(holders[0].bytes, Some(4096), "the size of the loaded file");
    }

    #[cfg(feature = "dictation")]
    struct NoopTranscriber;

    #[cfg(feature = "dictation")]
    impl crate::dictation::transcribe::Transcriber for NoopTranscriber {
        fn transcribe(
            &self,
            _audio: &[f32],
            _language: Option<&str>,
            _gates: crate::dictation::transcribe::VoiceGates,
        ) -> Result<crate::dictation::transcribe::TranscribeResult, String> {
            Ok(crate::dictation::transcribe::TranscribeResult {
                text: String::new(),
                skip_reason: None,
                language: None,
            })
        }
    }

    #[test]
    fn the_biggest_structure_is_reported_first() {
        // The whole point of the ordering: during the incident the culprit had
        // to be found by reading candidates one by one. The row that matters
        // must be the first one.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.grid.pty_raw_rings.insert(
            "s1".into(),
            parking_lot::Mutex::new((0..4096).map(|_| 7u8).collect()),
        );
        let maps = maps(&state);
        assert_eq!(maps[0].name, "grid.pty_raw_rings");
        assert_eq!(maps[0].bytes, Some(4096));
    }

    #[test]
    fn the_heap_is_reported_as_live_blocks_and_the_bytes_they_hold() {
        // The discriminator: when the footprint climbs and `accounted_bytes`
        // stays flat, this pair says whether the memory is in the Rust heap at
        // all. A zero block count would silently answer "no" to every future
        // question, so the only useful assertion is that real counters arrive.
        let stats = malloc_zone_stats();
        #[cfg(target_os = "macos")]
        {
            let (blocks, bytes) = stats.expect("the default zone always exists");
            assert!(blocks > 0, "a running process holds live allocations");
            assert!(
                bytes >= blocks,
                "every live block holds at least a byte: {blocks} blocks, {bytes} bytes"
            );
        }
        #[cfg(not(target_os = "macos"))]
        assert_eq!(stats, None);
    }

    #[test]
    fn a_live_large_block_is_counted_in_the_heap_census() {
        // The bug this guards: the report shows only totals, so a gap made of a few
        // 80 MB blocks (the 2026-09-29 Whisper decoder state) reads the same as a
        // million small allocations. A census that walks the wrong zone, or that
        // never sees a block this size, answers "none" every time.
        let block = vec![1u8; 2 * LARGE_BLOCK_BYTES as usize];
        let census = large_malloc_blocks();
        #[cfg(target_os = "macos")]
        {
            let (count, bytes) = census.expect("the zones can be enumerated");
            assert!(count >= 1, "a 16 MiB block is live: {count} blocks");
            assert!(
                bytes >= block.len() as u64,
                "the census holds at least that block: {bytes} bytes"
            );
        }
        #[cfg(not(target_os = "macos"))]
        assert_eq!(census, None);
        drop(std::hint::black_box(block));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_zone_walk_over_a_gigabyte_of_small_blocks_does_not_stall_allocators_for_long() {
        // The bug this guards: every zone stays force-locked for its whole walk,
        // so on a big heap each allocating thread in the app waits that long.
        // 1 GiB in a million blocks is the shape of the heap this report is for.
        let blocks: Vec<Vec<u8>> = (0..1_000_000).map(|_| vec![1u8; 1024]).collect();
        let longest = (0..5)
            .map(|_| walk_zones().expect("the walk completes").2)
            .max()
            .unwrap();
        eprintln!(
            "longest single-zone walk over {} blocks: {longest:?}",
            blocks.len()
        );
        drop(std::hint::black_box(blocks));
        // The timer starts before `lock`, so a loaded box adds lock waits to it;
        // 1 s still sits orders of magnitude above the measured ~10 ms walk.
        assert!(
            longest < std::time::Duration::from_secs(1),
            "a zone stayed locked for {longest:?}"
        );
    }

    #[test]
    fn threshold_summary_omits_payload_accounting_and_large_block_census() {
        // Catches: watchdog threshold crossings invoking the expensive full report.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        state.session_maps.output_buffers.insert(
            "summary-output".to_string(),
            parking_lot::Mutex::new(crate::state::OutputRingBuffer::new(4096)),
        );
        let entry = state
            .session_maps
            .output_buffers
            .get("summary-output")
            .unwrap();
        let _payload_lock = entry.value().lock();
        let summary = summary(&state);
        assert_eq!(summary["sessions"], 0);
        assert_eq!(summary["vt_log_buffers"], 0);
        assert_eq!(summary["content_indices"], 0);
        assert!(summary.get("malloc_bytes_in_use").is_some());
        assert!(summary.get("maps").is_none());
        assert!(summary.get("accounted_bytes").is_none());
        assert!(summary.get("malloc_large_blocks").is_none());
    }

    #[test]
    fn census_does_not_add_owned_large_blocks_to_accounted_bytes() {
        // Catches: counting the same output buffer through both maps and census.
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let block = crate::state::OutputRingBuffer::new(2 * LARGE_BLOCK_BYTES as usize);
        let owned_bytes = block.capacity();
        state.session_maps.output_buffers.insert(
            "owned-large-output".to_string(),
            parking_lot::Mutex::new(block),
        );
        let report = report(&state);
        assert_eq!(report["accounted_bytes"], owned_bytes);
        assert_eq!(
            report["malloc_large_blocks"]["may_overlap_accounted_bytes"],
            true
        );
        #[cfg(target_os = "macos")]
        assert!(report["malloc_large_blocks"]["bytes"].as_u64().unwrap() >= owned_bytes as u64);
    }

    #[test]
    fn the_report_carries_the_large_block_census() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let report = report(&state);
        assert_eq!(
            report["malloc_large_blocks"]["min_bytes"],
            LARGE_BLOCK_BYTES
        );
        #[cfg(target_os = "macos")]
        {
            assert_eq!(report["malloc_large_blocks"]["complete"], true);
            assert!(report["malloc_large_blocks"]["count"].is_u64());
            assert!(report["malloc_large_blocks"]["bytes"].is_u64());
        }
        #[cfg(not(target_os = "macos"))]
        assert_eq!(report["malloc_large_blocks"]["complete"], false);
    }

    #[test]
    fn the_footprint_is_the_compressed_total_not_the_resident_one() {
        // `ps` RSS read 0.52 GB while the process held 40 GB. Anything that
        // reports less than the resident set would repeat that mistake, so the
        // only assertion worth making is that a real number comes back.
        let fp = phys_footprint_bytes();
        #[cfg(target_os = "macos")]
        assert!(
            fp.is_some_and(|b| b > 1024 * 1024),
            "a running process has a footprint above 1 MB, got {fp:?}"
        );
        #[cfg(not(target_os = "macos"))]
        assert_eq!(fp, None);
    }
}

#[cfg(all(test, target_os = "macos"))]
mod census_critic_tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn small_allocations_do_not_register_as_large_blocks() {
        // Catches: enumerating with MALLOC_PTR_REGION_RANGE_TYPE (2) instead of
        // MALLOC_PTR_IN_USE_RANGE_TYPE (1). The region type reports every 8 MiB
        // small-allocator region whole, so 128 MiB of 4 KiB blocks reads as
        // ~16 "large blocks" that are not blocks at all.
        let (_, before) = large_malloc_blocks().expect("zones enumerable");
        let small: Vec<Vec<u8>> = (0..32 * 1024).map(|_| vec![7u8; 4096]).collect();
        let (_, after) = large_malloc_blocks().expect("zones enumerable");
        std::hint::black_box(&small);
        let delta = after.saturating_sub(before);
        assert!(
            delta < 32 * MIB,
            "128 MiB of 4 KiB allocations added {delta} bytes to the large-block census"
        );
    }

    #[test]
    fn the_census_sees_exactly_the_large_blocks_that_were_added() {
        // Catches: a recorder that double-counts a block (several zones, or both
        // in-use and region callbacks) or drops the size.
        let (c0, b0) = large_malloc_blocks().unwrap();
        let blocks: Vec<Vec<u8>> = (0..3).map(|_| vec![1u8; 16 * MIB as usize]).collect();
        let (c1, b1) = large_malloc_blocks().unwrap();
        std::hint::black_box(&blocks);
        assert!(c1 >= c0 + 3, "count {c0} -> {c1}");
        assert!(b1 >= b0 + 48 * MIB, "bytes {b0} -> {b1}");
        assert!(
            b1 <= b0 + 48 * MIB + 16 * MIB,
            "overcount: bytes {b0} -> {b1}"
        );
    }

    #[test]
    fn concurrent_census_walks_and_an_allocating_thread_do_not_deadlock() {
        // Catches: a zone lock held while something in the walk allocates, or two
        // walks taking zone locks in conflicting order. The harness bound is a
        // channel timeout so a deadlock fails instead of hanging the suite.
        let stop = Arc::new(AtomicBool::new(false));
        let allocator = {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut keep: Vec<Vec<u8>> = Vec::new();
                let mut i = 0usize;
                while !stop.load(Ordering::Relaxed) {
                    let n = match i % 4 {
                        0 => 64,
                        1 => 4096,
                        2 => 200 * 1024,
                        _ => 9 * MIB as usize,
                    };
                    keep.push(vec![1u8; n]);
                    if keep.len() > 64 {
                        keep.drain(..32);
                    }
                    i += 1;
                }
            })
        };
        let (tx, rx) = mpsc::channel();
        let walkers: Vec<_> = (0..2)
            .map(|_| {
                let tx = tx.clone();
                std::thread::spawn(move || {
                    for _ in 0..25 {
                        if large_malloc_blocks().is_none() {
                            let _ = tx.send(false);
                            return;
                        }
                    }
                    let _ = tx.send(true);
                })
            })
            .collect();
        for _ in 0..2 {
            let ok = rx
                .recv_timeout(Duration::from_secs(120))
                .expect("a census walk never returned: deadlock between walks and malloc");
            assert!(ok, "a walk returned None under concurrent allocation");
        }
        stop.store(true, Ordering::Relaxed);
        for w in walkers {
            w.join().unwrap();
        }
        allocator.join().unwrap();
    }
}

#[cfg(all(test, target_os = "macos"))]
mod foreign_zone_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const MAGIC: u64 = 0x1371_3968_cafe_f00d;
    static ENUMERATIONS: AtomicUsize = AtomicUsize::new(0);

    type Reader = unsafe extern "C" fn(u32, usize, usize, *mut *mut libc::c_void) -> libc::c_int;
    type Recorder =
        unsafe extern "C" fn(u32, *mut libc::c_void, libc::c_uint, *mut [usize; 2], libc::c_uint);

    // `<malloc/malloc.h>` malloc_introspection_t, spelled to its end so nothing
    // that walks the zone list reads past the struct.
    #[repr(C)]
    struct Introspection {
        enumerator: unsafe extern "C" fn(
            u32,
            *mut libc::c_void,
            libc::c_uint,
            usize,
            Option<Reader>,
            Recorder,
        ) -> libc::c_int,
        good_size: *const libc::c_void,
        check: *const libc::c_void,
        print: *const libc::c_void,
        log: *const libc::c_void,
        force_lock: unsafe extern "C" fn(*mut libc::c_void),
        force_unlock: unsafe extern "C" fn(*mut libc::c_void),
        rest: [*const libc::c_void; 8],
    }

    #[repr(C)]
    struct Zone {
        reserved1: *const libc::c_void,
        reserved2: *const libc::c_void,
        size: unsafe extern "C" fn(*mut libc::c_void, *const libc::c_void) -> usize,
        malloc: *const libc::c_void,
        calloc: *const libc::c_void,
        valloc: *const libc::c_void,
        free: *const libc::c_void,
        realloc: *const libc::c_void,
        destroy: *const libc::c_void,
        zone_name: *const libc::c_char,
        batch_malloc: *const libc::c_void,
        batch_free: *const libc::c_void,
        introspect: *const Introspection,
        version: libc::c_uint,
        rest: [*const libc::c_void; 4],
        magic: u64,
    }

    unsafe extern "C" {
        fn malloc_zone_register(zone: *mut libc::c_void);
        fn malloc_zone_unregister(zone: *mut libc::c_void);
    }

    unsafe extern "C" fn owns_nothing(_: *mut libc::c_void, _: *const libc::c_void) -> usize {
        0
    }
    unsafe extern "C" fn no_lock(_: *mut libc::c_void) {}

    // Behaves as caulk does: the reader is called without a null check, and the
    // bytes it returns are used. A null reader is a jump to address 0.
    unsafe extern "C" fn enumerate(
        task: u32,
        context: *mut libc::c_void,
        _kind: libc::c_uint,
        zone: usize,
        reader: Option<Reader>,
        recorder: Recorder,
    ) -> libc::c_int {
        let mut local: *mut libc::c_void = std::ptr::null_mut();
        let rc = unsafe {
            (reader.unwrap_unchecked())(
                task,
                zone + std::mem::offset_of!(Zone, magic),
                8,
                &mut local,
            )
        };
        if rc != 0 || unsafe { *local.cast::<u64>() } != MAGIC {
            return 1;
        }
        ENUMERATIONS.fetch_add(1, Ordering::SeqCst);
        let mut range = [0usize, 2 * LARGE_BLOCK_BYTES as usize];
        unsafe { recorder(task, context, 1, &mut range, 1) };
        0
    }

    #[test]
    fn a_zone_whose_enumerator_calls_the_reader_unconditionally_survives_the_census() {
        // Catches: passing a null memory_reader_t to every enumerator. CoreAudio's
        // caulk zone calls the reader without a check, so GET /diagnostics/memory
        // jumped to address 0 and killed the app (2026-10-02, story 1371-3968).
        let introspect = Box::new(Introspection {
            enumerator: enumerate,
            good_size: std::ptr::null(),
            check: std::ptr::null(),
            print: std::ptr::null(),
            log: std::ptr::null(),
            force_lock: no_lock,
            force_unlock: no_lock,
            rest: [std::ptr::null(); 8],
        });
        let zone = Box::into_raw(Box::new(Zone {
            reserved1: std::ptr::null(),
            reserved2: std::ptr::null(),
            size: owns_nothing,
            malloc: std::ptr::null(),
            calloc: std::ptr::null(),
            valloc: std::ptr::null(),
            free: std::ptr::null(),
            realloc: std::ptr::null(),
            destroy: std::ptr::null(),
            zone_name: c"tuic-test-reader-zone".as_ptr(),
            batch_malloc: std::ptr::null(),
            batch_free: std::ptr::null(),
            introspect: &*introspect,
            version: 0,
            rest: [std::ptr::null(); 4],
            magic: MAGIC,
        }));
        unsafe { malloc_zone_register(zone.cast()) };
        let before = ENUMERATIONS.load(Ordering::SeqCst);

        let census = large_malloc_blocks();

        unsafe { malloc_zone_unregister(zone.cast()) };
        drop(unsafe { Box::from_raw(zone) });
        let (count, bytes) = census.expect("the census completes with a foreign zone present");
        assert!(
            ENUMERATIONS.load(Ordering::SeqCst) > before,
            "the foreign zone was enumerated"
        );
        assert!(
            count >= 1 && bytes >= 2 * LARGE_BLOCK_BYTES,
            "its block is counted: {count}, {bytes}"
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn the_census_survives_after_the_audio_stack_was_touched() {
        // Catches: the same null reader against whatever zones CoreAudio registers
        // once a device list has been read. Whether this registers the caulk zone
        // depends on the machine; the test only asserts the walk completes.
        let _ = crate::notification_sound::list_output_devices();
        assert!(large_malloc_blocks().is_some());
    }
}
