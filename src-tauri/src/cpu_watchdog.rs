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
//! | grid_frame_in_flight     | ui-freeze-investigation-2026-05-28       |
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
const SLEEP_WAKE_GAP: Duration = Duration::from_secs(30);
const CPU_THRESHOLD_PCT: f64 = 80.0;
const CONSECUTIVE_THRESHOLD: u32 = 2;
const STARTUP_DELAY: Duration = Duration::from_secs(30);
const COOLDOWN_BETWEEN_REPORTS: Duration = Duration::from_secs(60);

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
    #[cfg(target_os = "macos")]
    {
        let pid = std::process::id();
        std::process::Command::new("ps")
            .args(["-M", "-p", &pid.to_string()])
            .output()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .count()
                    .saturating_sub(1)
            })
            .unwrap_or(0)
    }
    #[cfg(target_os = "linux")]
    {
        let pid = std::process::id();
        std::fs::read_dir(format!("/proc/{pid}/task")).map_or(0, |d| d.count())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}

fn child_process_summary() -> String {
    let pid = std::process::id();
    // macOS `ps` doesn't support --ppid; use -o + awk to filter
    let output = std::process::Command::new("sh")
        .args([
            "-c",
            &format!("ps -eo pid,ppid,comm,%cpu | awk '$2 == {pid}'"),
        ])
        .output();
    match output {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "(no children)".to_string()
            } else {
                s
            }
        }
        Err(_) => "(failed to list children)".to_string(),
    }
}

/// Compact aggregate %CPU of direct PTY children, for the periodic HEALTH log.
///
/// The spike trigger uses `getrusage(RUSAGE_SELF)`, which by design excludes
/// children (it watches TUIC's own runaway loops, not legitimate `cargo`/agent
/// load). So this is the only place child CPU surfaces during a diagnostic
/// session that ISN'T already a TUIC-process spike. `%cpu` from `ps` is the
/// process-lifetime average, not instantaneous — good enough for visibility.
fn child_cpu_summary() -> String {
    let pid = std::process::id();
    let output = std::process::Command::new("sh")
        .args(["-c", &format!("ps -eo ppid,comm,%cpu | awk '$1 == {pid}'")])
        .output();
    let Ok(o) = output else {
        return "children_cpu=(failed)".to_string();
    };
    let text = String::from_utf8_lossy(&o.stdout);
    let mut total = 0.0_f64;
    let mut top_comm = String::new();
    let mut top_pct = 0.0_f64;
    for line in text.lines() {
        // Columns: ppid comm %cpu. `comm` may contain spaces, so ppid is the
        // first token and %cpu the last; everything between is the name.
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 3 {
            continue;
        }
        let pct: f64 = tokens[tokens.len() - 1].parse().unwrap_or(0.0);
        total += pct;
        if pct > top_pct {
            top_pct = pct;
            top_comm = tokens[1..tokens.len() - 1].join(" ");
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
}

fn collect_snapshot(state: &Arc<AppState>, cpu_pct: f64) -> HealthSnapshot {
    let in_flight_stuck: Vec<String> = state
        .grid_frame_in_flight
        .iter()
        .filter(|entry| entry.value().load(Ordering::Relaxed))
        .map(|entry| entry.key().clone())
        .collect();

    HealthSnapshot {
        cpu_pct,
        threads: thread_count(),
        open_fds: count_open_fds(),
        pty_sessions: state.sessions.len(),
        index_building: state.index_in_flight.iter().map(|r| r.clone()).collect(),
        index_sem_permits: state.index_build_sem.available_permits(),
        in_flight_stuck,
        event_bus_subscribers: state.event_bus.receiver_count(),
        git_cache_ttl_fallbacks: state.git_cache.ttl_fallbacks.load(Ordering::Relaxed),
        head_emits_suppressed: state.repo_head_emits_suppressed.load(Ordering::Relaxed),
    }
}

fn log_spike(state: &Arc<AppState>, cpu_pct: f64) {
    let s = collect_snapshot(state, cpu_pct);
    let children = child_process_summary();

    tracing::warn!(
        source = "diagnostics",
        "CPU SPIKE {:.1}% | threads={} fds={} sessions={} \
         index_building={:?} sem_permits={} in_flight_stuck={:?} \
         bus_subs={} git_cache_ttl_fallbacks={} head_emits_suppressed={}\n  children: {}",
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
         index={:?} sem={} bus_subs={} git_cache_ttl_fallbacks={} head_emits_suppressed={}{}",
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
        stuck_note,
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
            prev = CpuSample::now().unwrap_or(prev);
            consecutive_high = 0;
            continue;
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
