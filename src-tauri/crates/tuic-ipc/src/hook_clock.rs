//! The clock that orders agent hook posts across processes (#2632-97ef).
//!
//! A hook process reads it when it starts; the server reads it when a PTY
//! generation begins and compares the two. Both sides must read the same
//! system-wide monotonic source: `CLOCK_MONOTONIC` on Unix,
//! `QueryPerformanceCounter` on Windows. The unit is opaque (nanoseconds on
//! Unix, performance-counter ticks on Windows); only the order is used. Never
//! the wall clock, which can step backwards.

/// The current reading of the system-wide monotonic clock.
#[cfg(unix)]
pub fn now() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable timespec; CLOCK_MONOTONIC always exists.
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts) };
    (ts.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(ts.tv_nsec as u64)
}

/// The current reading of the system-wide monotonic clock.
#[cfg(windows)]
pub fn now() -> u64 {
    let mut ticks = 0i64;
    // SAFETY: `ticks` is a valid, writable i64; QPC cannot fail on Windows XP+.
    unsafe { windows_sys::Win32::System::Performance::QueryPerformanceCounter(&mut ticks) };
    ticks as u64
}
