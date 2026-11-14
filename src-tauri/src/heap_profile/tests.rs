use super::*;

#[cfg(all(
    feature = "heap-profile",
    any(target_os = "macos", target_os = "linux")
))]
#[test]
fn activation_dump_and_metrics_describe_real_rust_allocations() {
    // Catches: feature links jemalloc without selecting it globally, or the
    // control plane acknowledges sampling/dumping without enabling the profiler.
    assert_eq!(status().unwrap()["active"], false);
    assert_eq!(set_active(true).unwrap()["active"], true);
    let before = metrics()["allocated_bytes"].as_u64().unwrap();
    let allocation = std::hint::black_box(vec![37u8; 8 * 1024 * 1024]);
    let during = metrics();
    assert!(during["allocated_bytes"].as_u64().unwrap() >= before + 8 * 1024 * 1024);
    assert!(
        during["active_bytes"].as_u64().unwrap() >= during["allocated_bytes"].as_u64().unwrap()
    );
    let dir = tempfile::tempdir_in(crate::test_support::test_temp_root()).unwrap();
    let path = dir.path().join("live.heap");
    dump(path.to_str().unwrap()).unwrap();
    let heap = std::fs::read_to_string(path).unwrap();
    assert!(heap.starts_with("heap_v2/"), "{heap}");
    assert!(heap.contains("\n@ "), "dump must contain allocation stacks");
    assert!(dump(dir.path().join("missing/dump.heap").to_str().unwrap()).is_err());
    assert!(dump("nul\0.heap").is_err());
    assert_eq!(set_active(false).unwrap()["active"], false);
    drop(allocation);
}

#[cfg(not(all(
    feature = "heap-profile",
    any(target_os = "macos", target_os = "linux")
)))]
#[test]
fn system_build_rejects_profiling_instead_of_empty_success() {
    // Catches: a normal/MSVC build claims activation or writes a bogus profile.
    assert_eq!(metrics()["kind"], "system");
    assert!(status().is_err());
    assert!(set_active(true).is_err());
    assert!(dump("unused.heap").is_err());
}

#[test]
fn memory_report_qualifies_native_malloc_separately_from_rust_allocator() {
    // Catches: native malloc-zone counters presented as the complete Rust heap
    // after selecting a prefixed allocator. Existing tests cover map accounting.
    let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
    let report = crate::memory_report::summary(&state);
    assert!(
        report["malloc_scope"]
            .as_str()
            .unwrap()
            .contains("excludes prefixed jemalloc")
    );
    assert!(report["allocator"]["kind"].is_string());
    assert!(report.get("malloc_bytes_in_use").is_some());
}
