//! Runtime diagnostics — always-on CPU watchdog + toggleable diagnostic mode.
//!
//! ## Always on (zero-cost when idle)
//! - **CPU spike detection**: polls `getrusage(RUSAGE_SELF)` every 5s. PTY children
//!   (cargo, rustc, etc.) are separate OS processes and don't affect RUSAGE_SELF —
//!   this is intentional: the trigger watches TUIC's own runaway loops, not
//!   legitimate child load. Logs a diagnostic snapshot when CPU > 80% for 10+
//!   consecutive seconds; that snapshot lists per-child %cpu (`child_process_summary`).
//! - The periodic HEALTH log (diagnostic mode) also reports aggregate child CPU
//!   (`child_cpu_summary`) so a hot PTY child is visible even when TUIC itself is calm.
//!
//! ## Diagnostic mode (toggle at runtime)
//! When enabled via `set_diagnostic_mode(true)`, emits periodic health snapshots
//! covering failure patterns from past incidents:
//!
//! | Check                    | Past incident (mdkb)                     |
//! |--------------------------|------------------------------------------|
//! | CPU %                    | ack-flush-loop-cpu-spike                 |
//! | grid frames outstanding  | ui-freeze-investigation-2026-05-28       |
//! | Event bus throughput     | invoke.ts thundering herd comment         |
//! | Content index state      | content-index-global-semaphore           |
//! | FD / thread count trend  | (previous investigation session)         |
//! | Sleep/wake gap           | sleep-wake-false-idle-detection           |

use crate::state::AppState;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const DIAGNOSTIC_POLL_INTERVAL: Duration = Duration::from_secs(10);
/// If the wall-clock gap between two consecutive watchdog polls exceeds this,
/// the machine was likely asleep (lid closed). Must stay well above
/// `DIAGNOSTIC_POLL_INTERVAL` (10s) so a normal slow tick never reads as sleep.
pub(crate) const SLEEP_WAKE_GAP: Duration = Duration::from_secs(30);
const CPU_THRESHOLD_PCT: f64 = 80.0;
const CONSECUTIVE_THRESHOLD: u32 = 2;
const STARTUP_DELAY: Duration = Duration::from_secs(30);
const COOLDOWN_BETWEEN_REPORTS: Duration = Duration::from_secs(60);

/// Footprint at which the memory report is logged for the first time.
///
/// A healthy backend sits at a few hundred MB. 4 GB is an order of magnitude
/// above anything legitimate and still an order of magnitude below the ~30 GB
/// that got the process jetsammed on 2026-09-08 — so the report lands with
/// hours of headroom instead of after the app is already gone. That incident
/// left no evidence at all: by the time the footprint was visible, macOS marked
/// the process as not-debuggable and nothing could say which structure held the
/// memory. This is the line that makes the next one self-diagnosing.
const MEMORY_REPORT_FLOOR: u64 = 4 * 1024 * 1024 * 1024;

/// Global toggle — checked by the polling loop.
static DIAGNOSTIC_MODE: AtomicBool = AtomicBool::new(false);

pub(crate) fn set_diagnostic_mode(on: bool) {
    let prev = DIAGNOSTIC_MODE.swap(on, Ordering::Relaxed);
    if prev != on {
        tracing::info!(
            source = "diagnostics",
            enabled = on,
            "Diagnostic mode {}",
            if on { "ENABLED" } else { "DISABLED" }
        );
    }
}

pub(crate) fn diagnostic_mode() -> bool {
    DIAGNOSTIC_MODE.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// CPU measurement via getrusage(RUSAGE_SELF)
// ---------------------------------------------------------------------------

// Never constructed on non-Unix (getrusage is POSIX-only) — the watchdog
// disables itself there. Suppress the resulting dead-code lint on Windows.
#[cfg_attr(not(unix), allow(dead_code))]
struct CpuSample {
    user_us: i64,
    sys_us: i64,
    wall: Instant,
}

impl CpuSample {
    #[cfg(unix)]
    fn now() -> Option<Self> {
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        let ret = unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) };
        if ret != 0 {
            return None;
        }
        Some(Self {
            user_us: usage.ru_utime.tv_sec * 1_000_000 + usage.ru_utime.tv_usec as i64,
            sys_us: usage.ru_stime.tv_sec * 1_000_000 + usage.ru_stime.tv_usec as i64,
            wall: Instant::now(),
        })
    }

    /// `getrusage(RUSAGE_SELF)` is POSIX-only; there's no equivalent self-usage
    /// probe wired up on non-Unix, so CPU sampling is unavailable and the
    /// watchdog disables itself (callers treat `None` as "watchdog unavailable").
    #[cfg(not(unix))]
    fn now() -> Option<Self> {
        None
    }

    #[cfg_attr(not(unix), allow(dead_code))]
    fn cpu_pct_since(&self, prev: &CpuSample) -> f64 {
        let cpu_delta_us = (self.user_us - prev.user_us) + (self.sys_us - prev.sys_us);
        let wall_us = self.wall.duration_since(prev.wall).as_micros() as f64;
        if wall_us <= 0.0 {
            return 0.0;
        }
        (cpu_delta_us as f64 / wall_us) * 100.0
    }
}

// ---------------------------------------------------------------------------
// System probes (cheap, no allocations on the happy path)
// ---------------------------------------------------------------------------

fn count_open_fds() -> usize {
    #[cfg(target_os = "macos")]
    {
        std::fs::read_dir("/dev/fd").map_or(0, |d| d.count())
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_dir("/proc/self/fd").map_or(0, |d| d.count())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

fn thread_count() -> usize {
    // Native (`PROC_PIDTASKINFO` on macOS, `/proc/self/stat` on Linux): the
    // old `ps -M` exec is refused in sandboxed hosts, where this read 0.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        tuic_core::process_info::usage(std::process::id()).map_or(0, |usage| usage.threads as usize)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

/// TUIC's direct children with their lifetime-average CPU, read natively
/// (`tuic_core::process_info`) — `ps | awk` cannot run in a sandboxed host.
fn direct_children_cpu() -> Option<Vec<tuic_core::process_info::ChildCpu>> {
    tuic_core::process_info::children_cpu(
        &tuic_core::process_info::NativeProcessSource,
        std::process::id(),
        std::time::SystemTime::now(),
    )
}

fn child_process_summary() -> String {
    format_child_process_summary(std::process::id(), direct_children_cpu())
}

/// One `pid ppid comm %cpu` line per child, the columns the old `ps` listing had.
fn format_child_process_summary(
    own_pid: u32,
    children: Option<Vec<tuic_core::process_info::ChildCpu>>,
) -> String {
    let Some(children) = children else {
        return "(failed to list children)".to_string();
    };
    if children.is_empty() {
        return "(no children)".to_string();
    }
    children
        .iter()
        .map(|child| {
            let cpu = child
                .cpu_percent
                .map_or_else(|| "?".to_string(), |pct| format!("{pct:.1}"));
            format!("{} {own_pid} {} {cpu}", child.pid, child.comm)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compact aggregate %CPU of direct PTY children, for the periodic HEALTH log.
///
/// The spike trigger uses `getrusage(RUSAGE_SELF)`, which by design excludes
/// children (it watches TUIC's own runaway loops, not legitimate `cargo`/agent
/// load). So this is the only place child CPU surfaces during a diagnostic
/// session that ISN'T already a TUIC-process spike. `%cpu` is the
/// process-lifetime average (CPU time over wall time since the child started),
/// not instantaneous — good enough for visibility.
fn child_cpu_summary() -> String {
    format_child_cpu_summary(direct_children_cpu())
}

fn format_child_cpu_summary(children: Option<Vec<tuic_core::process_info::ChildCpu>>) -> String {
    let Some(children) = children else {
        return "children_cpu=(failed)".to_string();
    };
    let mut total = 0.0_f64;
    let mut top_comm = String::new();
    let mut top_pct = 0.0_f64;
    for child in &children {
        let pct = f64::from(child.cpu_percent.unwrap_or(0.0));
        total += pct;
        if pct > top_pct {
            top_pct = pct;
            top_comm.clone_from(&child.comm);
        }
    }
    if top_comm.is_empty() {
        "children_cpu=0.0%".to_string()
    } else {
        format!("children_cpu={total:.1}% (top: {top_comm} {top_pct:.1}%)")
    }
}

// ---------------------------------------------------------------------------
// Snapshot: emitted on CPU spike or periodic diagnostic
// ---------------------------------------------------------------------------

struct HealthSnapshot {
    cpu_pct: f64,
    threads: usize,
    open_fds: usize,
    pty_sessions: usize,
    index_building: Vec<String>,
    index_sem_permits: usize,
    in_flight_stuck: Vec<String>,
    event_bus_subscribers: usize,
    git_cache_ttl_fallbacks: u64,
    head_emits_suppressed: u64,
    /// Events queued on the lossless per-session state lane and not yet applied.
    /// The lane is unbounded by design, so this is the only warning of a backlog.
    state_lane_depth: usize,
}

fn collect_snapshot(state: &Arc<AppState>, cpu_pct: f64) -> HealthSnapshot {
    // A session appears here with the number of frames it was sent and has not
    // reported back: anything above zero means the WebView is behind.
    let in_flight_stuck: Vec<String> = state
        .grid
        .gates
        .iter()
        .filter_map(|entry| {
            let outstanding = entry.value().outstanding();
            (outstanding > 0).then(|| format!("{}×{}", entry.key(), outstanding))
        })
        .collect();

    HealthSnapshot {
        cpu_pct,
        threads: thread_count(),
        open_fds: count_open_fds(),
        pty_sessions: state.session_maps.sessions.len(),
        index_building: state.index_in_flight.iter().map(|r| r.clone()).collect(),
        index_sem_permits: state.index_build_sem.available_permits(),
        in_flight_stuck,
        event_bus_subscribers: state.event_bus.receiver_count(),
        git_cache_ttl_fallbacks: state.git_cache.ttl_fallbacks.load(Ordering::Relaxed),
        head_emits_suppressed: state.repo_head_emits_suppressed.load(Ordering::Relaxed),
        state_lane_depth: state.session_maps.session_state_events.depth(),
    }
}

fn log_spike(state: &Arc<AppState>, cpu_pct: f64) {
    let s = collect_snapshot(state, cpu_pct);
    let children = child_process_summary();

    tracing::warn!(
        source = "diagnostics",
        "CPU SPIKE {:.1}% | threads={} fds={} sessions={} \
         index_building={:?} sem_permits={} in_flight_stuck={:?} \
         bus_subs={} git_cache_ttl_fallbacks={} head_emits_suppressed={} \
         state_lane={}\n  children: {}",
        s.cpu_pct,
        s.threads,
        s.open_fds,
        s.pty_sessions,
        s.index_building,
        s.index_sem_permits,
        s.in_flight_stuck,
        s.event_bus_subscribers,
        s.git_cache_ttl_fallbacks,
        s.head_emits_suppressed,
        s.state_lane_depth,
        children,
    );
}

fn log_periodic(state: &Arc<AppState>, cpu_pct: f64) {
    let s = collect_snapshot(state, cpu_pct);

    let stuck_note = if s.in_flight_stuck.is_empty() {
        String::new()
    } else {
        format!(" ⚠ in_flight_stuck={:?}", s.in_flight_stuck)
    };
    // `cpu` is TUIC-self only (RUSAGE_SELF); `children_cpu` covers PTY children
    // (cargo/agents) which the spike trigger deliberately ignores.
    let children = child_cpu_summary();

    tracing::info!(
        source = "diagnostics",
        "HEALTH cpu={:.1}% {} threads={} fds={} sessions={} \
         index={:?} sem={} bus_subs={} git_cache_ttl_fallbacks={} head_emits_suppressed={} \
         state_lane={}{}",
        s.cpu_pct,
        children,
        s.threads,
        s.open_fds,
        s.pty_sessions,
        s.index_building,
        s.index_sem_permits,
        s.event_bus_subscribers,
        s.git_cache_ttl_fallbacks,
        s.head_emits_suppressed,
        s.state_lane_depth,
        stuck_note,
    );
}

/// Say once when the desktop WebView's main thread stops running, and once when
/// it comes back.
///
/// This is the line that was missing on 2026-09-08: the UI was white for five
/// hours and nothing in the logs named the frontend. Diagnosis had to be
/// reconstructed by hand from the *absence* of frontend log lines.
fn report_frontend_liveness(state: &Arc<AppState>) {
    use crate::frontend_liveness::{FREEZE_AFTER, Verdict};

    match state.frontend_liveness.poll(FREEZE_AFTER) {
        Verdict::Quiet => {}
        Verdict::Frozen { gap } => tracing::warn!(
            source = "diagnostics",
            silent_secs = gap.as_secs(),
            last_heartbeat_received_ms = state.frontend_liveness.last_received_ms(),
            backend_observed_ms = chrono::Utc::now().timestamp_millis(),
            "Frontend unresponsive: no heartbeat for {}s — the WebView main thread is blocked or gone. \
             The backend and every PTY session are unaffected; recover with \
             POST /debug/reload_webview, or open the UI in a browser on this port.",
            gap.as_secs(),
        ),
        Verdict::Recovered => tracing::info!(
            source = "diagnostics",
            "Frontend responsive again — heartbeat resumed"
        ),
    }
}

// ---------------------------------------------------------------------------
// Memory tripwire
// ---------------------------------------------------------------------------

/// The threshold to arm next, or `None` while `footprint` is still under
/// `armed`.
///
/// Thresholds only ever double, so a process that keeps growing is reported at
/// 4, 8, 16 GB — each report a fresh reading of which structure grew between
/// them — while a process that sits just under one is never reported twice.
/// A footprint that jumps several thresholds at once arms above where it
/// landed, so the next report means real further growth.
///
/// The arming is monotonic: memory that falls back below the line does not
/// re-arm it. Re-arming would flap around the threshold, and the first report
/// already names the structure.
fn next_memory_threshold(footprint: u64, armed: u64) -> Option<u64> {
    if footprint < armed {
        return None;
    }
    let mut next = armed;
    while next <= footprint {
        let doubled = next.saturating_mul(2);
        if doubled == next {
            break;
        }
        next = doubled;
    }
    Some(next)
}

/// Log where the memory is, at a level that survives log filtering.
fn log_memory_report(state: &Arc<AppState>, footprint: u64) {
    const GB: f64 = (1024 * 1024 * 1024) as f64;
    let report = crate::memory_report::summary(state);
    tracing::error!(
        source = "diagnostics",
        report = %report,
        "Memory footprint {:.2} GB — this is far above a healthy backend and is what \
         gets the app killed by macOS under memory pressure. This summary contains \
         allocator counters and structure counts only; request /diagnostics/memory \
         explicitly for payload accounting and the large-block census.",
        footprint as f64 / GB,
    );
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

pub(crate) fn spawn(state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("diagnostics".into())
        .spawn(move || run(state))
        .expect("failed to spawn diagnostics thread");
}

fn run(state: Arc<AppState>) {
    std::thread::sleep(STARTUP_DELAY);
    tracing::debug!(source = "diagnostics", "Diagnostics thread started");

    let mut prev = match CpuSample::now() {
        Some(s) => s,
        None => {
            tracing::warn!(
                source = "diagnostics",
                "getrusage failed — watchdog disabled"
            );
            return;
        }
    };

    let mut consecutive_high: u32 = 0;
    let mut last_spike_report = Instant::now() - COOLDOWN_BETWEEN_REPORTS;
    let mut last_periodic_report = Instant::now();
    let mut last_poll_wall = Instant::now();

    // Trend tracking for FD / thread growth
    let mut baseline_fds: Option<usize> = None;
    let mut baseline_threads: Option<usize> = None;

    // Memory tripwire — always on, not gated behind diagnostic mode. The
    // incident it exists for took 13 hours to build up with nobody watching.
    let mut armed_memory = MEMORY_REPORT_FLOOR;

    loop {
        let interval = if diagnostic_mode() {
            DIAGNOSTIC_POLL_INTERVAL
        } else {
            POLL_INTERVAL
        };
        std::thread::sleep(interval);

        // Sleep/wake detection: if wall-clock gap is way larger than poll interval,
        // the machine was asleep. Skip this tick to avoid stale deltas.
        let wall_gap = last_poll_wall.elapsed();
        last_poll_wall = Instant::now();
        if wall_gap > SLEEP_WAKE_GAP {
            tracing::info!(
                source = "diagnostics",
                gap_secs = wall_gap.as_secs(),
                "Sleep/wake detected — skipping tick"
            );
            // Tell the frontend a wake just happened so it can suppress the
            // false-busy completion cascade: on wake, idle shells/agents get
            // nudged busy→idle and would otherwise fire spurious completion
            // notifications (purple "unseen" dot + sound) for work that never ran.
            #[cfg(feature = "desktop")]
            {
                use tauri::Emitter;
                if let Some(ref app) = *state.app_handle.read() {
                    let _ = app.emit("system-wake", wall_gap.as_secs());
                }
            }
            // The JS thread not having run while the machine was off is not a
            // freeze. Without this every wake reports one.
            state.frontend_liveness.rebaseline();
            prev = CpuSample::now().unwrap_or(prev);
            consecutive_high = 0;
            continue;
        }

        if !state.webview_unloaded.load(Ordering::Acquire) {
            report_frontend_liveness(&state);
        }

        let current = match CpuSample::now() {
            Some(s) => s,
            None => continue,
        };

        let pct = current.cpu_pct_since(&prev);
        prev = current;

        // --- CPU spike detection (always on) ---
        if pct >= CPU_THRESHOLD_PCT {
            consecutive_high += 1;
            if consecutive_high >= CONSECUTIVE_THRESHOLD
                && last_spike_report.elapsed() >= COOLDOWN_BETWEEN_REPORTS
            {
                log_spike(&state, pct);
                last_spike_report = Instant::now();
                consecutive_high = 0;
            }
        } else {
            if consecutive_high >= CONSECUTIVE_THRESHOLD {
                tracing::info!(
                    source = "diagnostics",
                    cpu_pct = format!("{pct:.1}"),
                    "CPU spike resolved — back to {pct:.1}%"
                );
            }
            consecutive_high = 0;
        }

        // --- Memory tripwire (always on) ---
        // One `proc_pid_rusage` call per tick; the report itself is only built
        // when a threshold trips.
        if let Some(footprint) = crate::memory_report::phys_footprint_bytes()
            && let Some(next) = next_memory_threshold(footprint, armed_memory)
        {
            log_memory_report(&state, footprint);
            armed_memory = next;
        }

        // --- Diagnostic mode: periodic health snapshots ---
        if diagnostic_mode() && last_periodic_report.elapsed() >= Duration::from_secs(30) {
            log_periodic(&state, pct);
            last_periodic_report = Instant::now();

            // FD / thread growth trend
            let fds = count_open_fds();
            let threads = thread_count();
            let base_fds = *baseline_fds.get_or_insert(fds);
            let base_threads = *baseline_threads.get_or_insert(threads);

            if fds > base_fds + 50 {
                tracing::warn!(
                    source = "diagnostics",
                    "FD growth: {} → {} (+{} since baseline)",
                    base_fds,
                    fds,
                    fds - base_fds,
                );
            }
            if threads > base_threads + 20 {
                tracing::warn!(
                    source = "diagnostics",
                    "Thread growth: {} → {} (+{} since baseline)",
                    base_threads,
                    threads,
                    threads - base_threads,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child(pid: u32, comm: &str, cpu: Option<f32>) -> tuic_core::process_info::ChildCpu {
        tuic_core::process_info::ChildCpu {
            pid,
            comm: comm.to_string(),
            cpu_percent: cpu,
        }
    }

    #[test]
    fn child_summaries_keep_their_log_shapes() {
        assert_eq!(
            format_child_process_summary(7, None),
            "(failed to list children)"
        );
        assert_eq!(
            format_child_process_summary(7, Some(vec![])),
            "(no children)"
        );
        assert_eq!(
            format_child_process_summary(
                7,
                Some(vec![child(11, "zsh", Some(1.3)), child(12, "sudo", None)])
            ),
            "11 7 zsh 1.3\n12 7 sudo ?"
        );
        assert_eq!(format_child_cpu_summary(None), "children_cpu=(failed)");
        assert_eq!(format_child_cpu_summary(Some(vec![])), "children_cpu=0.0%");
        assert_eq!(
            format_child_cpu_summary(Some(vec![
                child(11, "zsh", Some(2.0)),
                child(12, "cargo", Some(30.5)),
                child(13, "sudo", None),
            ])),
            "children_cpu=32.5% (top: cargo 30.5%)"
        );
    }

    /// Red before the native read in a sandboxed host: `ps -M` could not be
    /// exec'd there, so this read 0.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn thread_count_reads_this_process_natively() {
        assert!(thread_count() >= 1);
    }

    /// A real child must show up in the spike-log listing.
    #[cfg(unix)]
    #[test]
    fn direct_children_include_a_live_child() {
        let mut sleeper = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let pid = sleeper.id();
        let listed = direct_children_cpu().expect("children are listable");
        let _ = sleeper.kill();
        let _ = sleeper.wait();
        assert!(
            listed.iter().any(|c| c.pid == pid && c.comm == "sleep"),
            "{listed:?}"
        );
    }

    const GB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn a_healthy_footprint_never_trips() {
        // A few hundred MB is normal and must stay silent forever, or the one
        // line that matters drowns in daily noise.
        assert_eq!(
            next_memory_threshold(400 * 1024 * 1024, MEMORY_REPORT_FLOOR),
            None
        );
    }

    #[test]
    fn crossing_the_floor_arms_the_next_doubling() {
        assert_eq!(
            next_memory_threshold(5 * GB, MEMORY_REPORT_FLOOR),
            Some(8 * GB),
            "reporting again at 8 GB means real further growth, not the same 5 GB twice"
        );
    }

    #[test]
    fn a_footprint_that_jumps_several_thresholds_is_reported_once() {
        // The 2026-09-08 shape: nobody was watching while it grew, and the
        // first reading was already deep past the floor. It must report there
        // and then arm above it, not walk every threshold it skipped.
        let armed = next_memory_threshold(40 * GB, MEMORY_REPORT_FLOOR);
        assert_eq!(armed, Some(64 * GB));
        assert_eq!(
            next_memory_threshold(41 * GB, armed.unwrap()),
            None,
            "still growing slowly at 41 GB is the same incident, not a new one"
        );
    }

    #[test]
    fn memory_falling_back_does_not_re_arm() {
        let armed = next_memory_threshold(5 * GB, MEMORY_REPORT_FLOOR).unwrap();
        assert_eq!(
            next_memory_threshold(GB, armed),
            None,
            "a footprint below the armed line is silent — re-arming would flap"
        );
    }

    /// The state lane is unbounded on purpose — dropping a SET or a CLEAR strands
    /// clients in a state that never existed or never ended — so a backlog is invisible
    /// until it is a memory problem. This snapshot is the only place it surfaces.
    ///
    /// A bare test `AppState` has no accumulator task, so nothing drains the lane: the
    /// same shape as the wedged consumer the metric exists to reveal.
    #[test]
    fn snapshot_reports_the_state_lane_backlog() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        assert_eq!(
            collect_snapshot(&state, 0.0).state_lane_depth,
            0,
            "an idle lane is empty"
        );

        for _ in 0..3 {
            state.emit_pty_event(crate::state::AppEvent::PtyParsed {
                session_id: "s1".to_string(),
                parsed: serde_json::json!({ "type": "choice-cleared" }).into(),
            });
        }

        assert_eq!(
            collect_snapshot(&state, 0.0).state_lane_depth,
            3,
            "every queued event must be counted while it waits to be applied"
        );
    }
}
