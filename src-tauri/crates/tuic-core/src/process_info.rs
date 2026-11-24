//! Native process inspection: the process table, one process's identity,
//! resource usage, working directory and zombie children — without exec'ing
//! `ps`, `pgrep` or `sysctl`.
//!
//! Why native: `/bin/ps` is setuid root on macOS, and sandboxed hosts (agent
//! sandboxes, locked-down CI) refuse to exec it; `pgrep` additionally needs the
//! `sysmond` mach service. The libproc / `kern.proc` calls underneath both of
//! them stay available, so every consumer that used to parse `ps` output went
//! blind in exactly the environment where TUIC most needs to see its children.
//!
//! Platforms:
//! - **macOS** — `proc_listallpids`, `proc_pidinfo(PROC_PIDT_SHORTBSDINFO)`
//!   (identity, works for every uid), `sysctl KERN_PROC_PID` (start time and
//!   zombie state, every uid), `KERN_PROCARGS2` (argv, same uid only — see
//!   [`Unreadable::PermissionDenied`]), `proc_pidpath`, `PROC_PIDTASKINFO`,
//!   `PROC_PIDVNODEPATHINFO`.
//! - **Linux** — `/proc/<pid>/{stat,status,cmdline,exe,cwd}` and `/proc/stat`'s
//!   `btime`.
//! - **Windows** — not implemented here: every call returns `None`, and the
//!   callers keep their existing ToolHelp code paths.
//!
//! Every entry point fails soft (`None`, never a panic): a stricter sandbox
//! profile can deny process inspection outright, and the callers already treat
//! "inventory unavailable" as "don't decide".
//!
//! Linux `/proc` mounted with `hidepid=1|2` (some hardened hosts and
//! containers): other users' processes are hidden (`2`) or their
//! `cmdline`/`stat` unreadable (`1`). A snapshot then lists only this user's
//! processes, and a hidden pid reads as gone (`info`/`usage` → `None`) rather
//! than as [`Unreadable::PermissionDenied`]. TUIC only judges its own subtree,
//! which is this user's, so nothing it decides on is hidden; a `sudo` child is
//! the exception and is reported missing, not unreadable. Not tested: it needs
//! a remount, which no test can do.
//!
//! The [`ProcessSource`] trait is the test seam: production code takes a
//! `&dyn ProcessSource`, tests hand it a [`FakeProcessSource`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Why a field of a listed process could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unreadable {
    /// The process belongs to another user (typically root, under `sudo`), and
    /// an unprivileged caller may only see its identity, not its arguments.
    /// `ps` is setuid root and could; this module cannot. Callers must treat
    /// the field as unknown rather than as empty.
    PermissionDenied,
    /// The read failed for another reason: the process exited mid-read, the
    /// platform has no such data (a kernel thread's empty argv), or the
    /// platform is not implemented.
    Unavailable,
    /// The caller asked for a snapshot without argv.
    NotRequested,
}

/// One process, as listed by [`ProcessSource::snapshot`] or [`ProcessSource::info`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcInfo {
    pub pid: u32,
    pub ppid: u32,
    pub pgid: u32,
    /// Effective uid.
    pub uid: u32,
    /// The kernel's short process name: macOS `p_comm` (at most 16 bytes, the
    /// executable's basename), Linux `/proc/<pid>/comm` (at most 15 bytes).
    pub comm: String,
    /// Absolute path of the executable, when readable (`proc_pidpath` works for
    /// every uid on macOS; Linux `/proc/<pid>/exe` only for the caller's own).
    pub exe: Option<PathBuf>,
    /// The full argument vector.
    pub argv: Result<Vec<String>, Unreadable>,
    /// When the process started, if the platform reports it.
    pub start_time: Option<SystemTime>,
}

impl ProcInfo {
    /// The name `ps -o comm` would print, as closely as the readable fields
    /// allow: `argv[0]` (so a login shell reads as `-zsh` and an absolute
    /// invocation keeps its path), else the executable path, else the kernel's
    /// short name.
    pub fn display_name(&self) -> String {
        if let Ok(argv) = &self.argv
            && let Some(first) = argv.first()
            && !first.is_empty()
        {
            return first.clone();
        }
        if let Some(exe) = &self.exe {
            return exe.to_string_lossy().into_owned();
        }
        self.comm.clone()
    }

    /// Whole seconds since the process started, if the start time is known.
    pub fn age_seconds(&self, now: SystemTime) -> Option<u64> {
        let start = self.start_time?;
        Some(now.duration_since(start).map_or(0, |age| age.as_secs()))
    }
}

/// Resource usage of one process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcUsage {
    pub rss_bytes: u64,
    /// User + system CPU time consumed over the process's lifetime.
    pub cpu_time: Duration,
    pub threads: u32,
}

/// Lifetime-average CPU% (`cpu_time / wall time since start`, 100 = one core) —
/// the same semantics as Linux `ps -o %cpu`.
pub fn lifetime_cpu_percent(usage: &ProcUsage, start: SystemTime, now: SystemTime) -> f32 {
    let wall = now.duration_since(start).unwrap_or_default().as_secs_f64();
    if wall <= 0.0 {
        return 0.0;
    }
    (usage.cpu_time.as_secs_f64() / wall * 100.0) as f32
}

/// The inspection operations TUIC needs. [`NativeProcessSource`] reads the
/// live OS; [`FakeProcessSource`] serves a fixed table to tests.
pub trait ProcessSource: Send + Sync {
    /// Every live, non-zombie process. `None` only when the enumeration itself
    /// failed; a process that vanished mid-scan is silently skipped. With
    /// `want_argv == false` every `argv` is `Err(Unreadable::NotRequested)`.
    fn snapshot(&self, want_argv: bool) -> Option<Vec<ProcInfo>>;
    /// Direct children of `ppid`, zombies included.
    fn children(&self, ppid: u32) -> Option<Vec<u32>>;
    /// One live process, argv included. `None` when it is gone or a zombie.
    fn info(&self, pid: u32) -> Option<ProcInfo>;
    /// Resource usage. `None` when gone, or (macOS) owned by another user.
    fn usage(&self, pid: u32) -> Option<ProcUsage>;
    /// Current working directory.
    fn cwd(&self, pid: u32) -> Option<PathBuf>;
    /// Executable path.
    fn exe(&self, pid: u32) -> Option<PathBuf>;
    /// Direct children of `ppid` that are zombies (exited, not yet reaped).
    fn zombie_children(&self, ppid: u32) -> Option<Vec<u32>>;
    /// When `pid` started. The default reads [`ProcessSource::info`]; the
    /// native source skips argv (on macOS a `KERN_PROCARGS2` read per pid),
    /// which a caller that only needs the start time should not pay for.
    fn start_time(&self, pid: u32) -> Option<SystemTime> {
        self.info(pid)?.start_time
    }
}

/// The live operating system.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeProcessSource;

impl ProcessSource for NativeProcessSource {
    fn snapshot(&self, want_argv: bool) -> Option<Vec<ProcInfo>> {
        platform::snapshot(want_argv)
    }
    fn children(&self, ppid: u32) -> Option<Vec<u32>> {
        platform::children(ppid)
    }
    fn info(&self, pid: u32) -> Option<ProcInfo> {
        platform::info(pid, true)
    }
    fn usage(&self, pid: u32) -> Option<ProcUsage> {
        platform::usage(pid)
    }
    fn cwd(&self, pid: u32) -> Option<PathBuf> {
        platform::cwd(pid)
    }
    fn exe(&self, pid: u32) -> Option<PathBuf> {
        platform::exe(pid)
    }
    fn zombie_children(&self, ppid: u32) -> Option<Vec<u32>> {
        platform::zombie_children(ppid)
    }
    fn start_time(&self, pid: u32) -> Option<SystemTime> {
        platform::info(pid, false)?.start_time
    }
}

/// [`ProcessSource::snapshot`] on the live OS.
pub fn snapshot(want_argv: bool) -> Option<Vec<ProcInfo>> {
    NativeProcessSource.snapshot(want_argv)
}

/// [`ProcessSource::children`] on the live OS.
pub fn children(ppid: u32) -> Option<Vec<u32>> {
    NativeProcessSource.children(ppid)
}

/// [`ProcessSource::info`] on the live OS.
pub fn info(pid: u32) -> Option<ProcInfo> {
    NativeProcessSource.info(pid)
}

/// [`ProcessSource::usage`] on the live OS.
pub fn usage(pid: u32) -> Option<ProcUsage> {
    NativeProcessSource.usage(pid)
}

/// [`ProcessSource::cwd`] on the live OS.
pub fn cwd(pid: u32) -> Option<PathBuf> {
    NativeProcessSource.cwd(pid)
}

/// [`ProcessSource::exe`] on the live OS.
pub fn exe(pid: u32) -> Option<PathBuf> {
    NativeProcessSource.exe(pid)
}

/// [`ProcessSource::zombie_children`] on the live OS.
pub fn zombie_children(ppid: u32) -> Option<Vec<u32>> {
    NativeProcessSource.zombie_children(ppid)
}

/// Whether `pid` names an existing process — `kill(pid, 0)`, the same check as
/// `/bin/kill -0`. An unreaped zombie still counts as existing, and a process
/// owned by another user counts too (`EPERM` proves it exists).
#[cfg(unix)]
pub fn is_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: kill(2) with signal 0 performs only the existence/permission check.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Send `signal` to one process. `false` when nothing was signalled.
#[cfg(unix)]
pub fn signal(pid: u32, signal: libc::c_int) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // 0 and negative pids address process groups, 1 is init: never a target here.
    if pid <= 1 {
        return false;
    }
    // SAFETY: kill(2) on a single positive pid with a plain signal number.
    unsafe { libc::kill(pid, signal) == 0 }
}

/// SIGTERM one process (see [`signal`]); for crates without a `libc` dependency.
#[cfg(unix)]
pub fn terminate(pid: u32) -> bool {
    signal(pid, libc::SIGTERM)
}

/// SIGKILL one process (see [`signal`]); for crates without a `libc` dependency.
#[cfg(unix)]
pub fn kill(pid: u32) -> bool {
    signal(pid, libc::SIGKILL)
}

// ─── Shared derivations (pure over a ProcessSource) ────────────────────────

/// Map every live process onto its direct children, from ONE snapshot.
/// `None` when the table could not be listed.
pub fn parent_map(source: &dyn ProcessSource) -> Option<HashMap<u32, Vec<u32>>> {
    let mut map: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in source.snapshot(false)? {
        map.entry(process.ppid).or_default().push(process.pid);
    }
    (!map.is_empty()).then_some(map)
}

/// Resident set size (KiB) and lifetime-average CPU% (see
/// [`lifetime_cpu_percent`]) for each pid whose usage is readable. A pid that
/// is gone, or (macOS) owned by another user, is simply absent.
pub fn usage_stats(
    source: &dyn ProcessSource,
    pids: &[u32],
    now: SystemTime,
) -> HashMap<u32, (u64, f32)> {
    let mut stats = HashMap::new();
    for &pid in pids {
        let Some(usage) = source.usage(pid) else {
            continue;
        };
        // The start time only: `info` would also read argv, once per pid of
        // every snapshot the process manager takes.
        let cpu = source
            .start_time(pid)
            .map_or(0.0, |start| lifetime_cpu_percent(&usage, start, now));
        stats.insert(pid, (usage.rss_bytes / 1024, cpu));
    }
    stats
}

/// A direct child with its kernel name and lifetime-average CPU%.
#[derive(Clone, Debug, PartialEq)]
pub struct ChildCpu {
    pub pid: u32,
    pub comm: String,
    /// `None` when the child's usage is not readable.
    pub cpu_percent: Option<f32>,
}

/// The live, non-zombie direct children of `ppid`. `None` when the children
/// could not be listed at all.
pub fn children_cpu(
    source: &dyn ProcessSource,
    ppid: u32,
    now: SystemTime,
) -> Option<Vec<ChildCpu>> {
    let children = source.children(ppid)?;
    Some(
        children
            .into_iter()
            .filter_map(|pid| {
                let process = source.info(pid)?;
                // The listed child may have exited and its pid been reused by an unrelated
                // process before this read; a row whose parent changed is not our child.
                if process.ppid != ppid {
                    return None;
                }
                // DEFERRED (2026-11-23): `usage` is read after `info` with no identity check
                // between them, and `now` is the caller's earlier timestamp rather than the
                // sample time, so a pid reused (or a slow read) between the calls skews this
                // one watchdog log percentage. Binding both to a start-time-stamped sample
                // needs a `usage` API change; the effect is a mislabeled log line, not state.
                let cpu_percent = match (source.usage(pid), process.start_time) {
                    (Some(usage), Some(start)) => Some(lifetime_cpu_percent(&usage, start, now)),
                    _ => None,
                };
                Some(ChildCpu {
                    pid,
                    comm: process.comm,
                    cpu_percent,
                })
            })
            .collect(),
    )
}

/// The first process in `root`'s subtree (root included) whose argv or start
/// time could not be read, if any. Callers that must judge a subtree by its
/// command lines use this to decide that a native snapshot is not enough.
pub fn first_unreadable_in_subtree(processes: &[ProcInfo], root: u32) -> Option<&ProcInfo> {
    let mut children: HashMap<u32, Vec<&ProcInfo>> = HashMap::new();
    let mut by_pid: HashMap<u32, &ProcInfo> = HashMap::new();
    for process in processes {
        children.entry(process.ppid).or_default().push(process);
        by_pid.insert(process.pid, process);
    }
    let mut seen = std::collections::HashSet::from([root]);
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if let Some(process) = by_pid.get(&pid)
            && (process.argv.is_err() || process.start_time.is_none())
        {
            return Some(process);
        }
        for child in children.get(&pid).into_iter().flatten() {
            if seen.insert(child.pid) {
                stack.push(child.pid);
            }
        }
    }
    None
}

/// A fixed process table for tests: build it, hand it to code that takes a
/// `&dyn ProcessSource`.
#[cfg(any(test, feature = "test-support"))]
#[derive(Clone, Debug, Default)]
pub struct FakeProcessSource {
    /// `None` makes [`ProcessSource::snapshot`] report a failed enumeration.
    pub processes: Option<Vec<ProcInfo>>,
    pub usages: HashMap<u32, ProcUsage>,
    pub cwds: HashMap<u32, PathBuf>,
    pub zombies: Vec<(u32, u32)>,
}

#[cfg(any(test, feature = "test-support"))]
impl FakeProcessSource {
    pub fn with_processes(processes: Vec<ProcInfo>) -> Self {
        Self {
            processes: Some(processes),
            ..Self::default()
        }
    }

    /// A failed enumeration: every lookup answers `None`.
    pub fn failing() -> Self {
        Self::default()
    }

    /// A process entry with readable argv and no start time.
    pub fn process(pid: u32, ppid: u32, comm: &str, argv: &[&str]) -> ProcInfo {
        ProcInfo {
            pid,
            ppid,
            pgid: pid,
            uid: 501,
            comm: comm.to_string(),
            exe: None,
            argv: Ok(argv.iter().map(|arg| (*arg).to_string()).collect()),
            start_time: None,
        }
    }
}

#[cfg(any(test, feature = "test-support"))]
impl ProcessSource for FakeProcessSource {
    fn snapshot(&self, want_argv: bool) -> Option<Vec<ProcInfo>> {
        let mut processes = self.processes.clone()?;
        if !want_argv {
            for process in &mut processes {
                process.argv = Err(Unreadable::NotRequested);
            }
        }
        Some(processes)
    }
    fn children(&self, ppid: u32) -> Option<Vec<u32>> {
        let processes = self.processes.as_ref()?;
        Some(
            processes
                .iter()
                .filter(|process| process.ppid == ppid)
                .map(|process| process.pid)
                .chain(
                    self.zombies
                        .iter()
                        .filter(|(_, parent)| *parent == ppid)
                        .map(|(pid, _)| *pid),
                )
                .collect(),
        )
    }
    fn info(&self, pid: u32) -> Option<ProcInfo> {
        self.processes
            .as_ref()?
            .iter()
            .find(|process| process.pid == pid)
            .cloned()
    }
    fn usage(&self, pid: u32) -> Option<ProcUsage> {
        self.processes.as_ref()?;
        self.usages.get(&pid).copied()
    }
    fn cwd(&self, pid: u32) -> Option<PathBuf> {
        self.processes.as_ref()?;
        self.cwds.get(&pid).cloned()
    }
    fn exe(&self, pid: u32) -> Option<PathBuf> {
        self.info(pid)?.exe
    }
    fn zombie_children(&self, ppid: u32) -> Option<Vec<u32>> {
        self.processes.as_ref()?;
        Some(
            self.zombies
                .iter()
                .filter(|(_, parent)| *parent == ppid)
                .map(|(pid, _)| *pid)
                .collect(),
        )
    }
}

// ─── macOS ──────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use super::{ProcInfo, ProcUsage, Unreadable};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    /// Every pid in the table, from `proc_listallpids`. Zombies included.
    /// Run a libproc listing call and tell a failure from an empty result.
    ///
    /// libproc's listing wrappers (`proc_listpids`, `proc_listchildpids`) return 0, not
    /// -1, when the underlying syscall fails, leaving the reason in `errno`. A bare
    /// `< 0` check would read a denied enumeration as "no processes" (and a zombie
    /// check would then report none without inspecting anything). So `errno` is cleared
    /// first, and a zero count with `errno` set is a failure.
    fn checked_listing(call: impl FnOnce() -> libc::c_int) -> Option<libc::c_int> {
        // SAFETY: `__error` returns this thread's errno slot.
        unsafe { *libc::__error() = 0 };
        let count = call();
        if count < 0 || (count == 0 && std::io::Error::last_os_error().raw_os_error() != Some(0)) {
            return None;
        }
        Some(count)
    }

    fn all_pids() -> Option<Vec<i32>> {
        // SAFETY: a null buffer asks only for the current count.
        let estimate = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        if estimate <= 0 {
            return None;
        }
        list_pids_into(estimate as usize, |buffer, bytes| {
            // SAFETY: `buffer` holds `bytes` writable bytes.
            unsafe { libc::proc_listallpids(buffer, bytes) }
        })
    }

    /// Run a libproc pid-listing call with a buffer of `estimate` pids plus
    /// headroom, growing it when the table outgrew the estimate between calls.
    /// The listing calls return a pid COUNT.
    fn list_pids_into(
        estimate: usize,
        call: impl Fn(*mut libc::c_void, libc::c_int) -> libc::c_int,
    ) -> Option<Vec<i32>> {
        // `count` below is already vetted by `checked_listing`.
        let mut capacity = estimate + 64;
        for _ in 0..4 {
            let mut pids = vec![0i32; capacity];
            let bytes = libc::c_int::try_from(capacity * std::mem::size_of::<i32>()).ok()?;
            let count = checked_listing(|| call(pids.as_mut_ptr().cast(), bytes))?;
            let count = count as usize;
            if count < capacity {
                pids.truncate(count);
                pids.retain(|&pid| pid > 0);
                return Some(pids);
            }
            capacity *= 2;
        }
        None
    }

    fn short_info(pid: i32) -> Option<libc::proc_bsdshortinfo> {
        let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_bsdshortinfo>() as libc::c_int;
        // SAFETY: `info` is a writable proc_bsdshortinfo of exactly `size` bytes.
        let ret = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDT_SHORTBSDINFO,
                0,
                (&raw mut info).cast(),
                size,
            )
        };
        // A short read means gone, denied or a zombie (ESRCH); a mismatched pid
        // means the slot was reused between listing and inspection.
        (ret == size && info.pbsi_pid == pid as u32).then_some(info)
    }

    /// The fields of `struct kinfo_proc` this module reads, by offset into
    /// `extern_proc` (its first member). `libc` does not bind `kinfo_proc`;
    /// these three offsets are stable across 64-bit Darwin releases:
    /// `p_un.__p_starttime` (struct timeval) at 0, `p_stat` (char) at 36,
    /// `p_pid` at 40. The whole struct is 648 bytes on arm64 and x86_64.
    struct KinfoFields {
        start: SystemTime,
        zombie: bool,
    }

    const KINFO_PROC_SIZE: usize = 648;
    const KINFO_STAT_OFFSET: usize = 36;
    const KINFO_PID_OFFSET: usize = 40;

    /// `sysctl {CTL_KERN, KERN_PROC, KERN_PROC_PID, pid}` — readable for every
    /// uid and, unlike `proc_pidinfo`, for zombies too.
    fn kinfo(pid: i32) -> Option<KinfoFields> {
        let mut mib = [libc::CTL_KERN, libc::KERN_PROC, libc::KERN_PROC_PID, pid];
        // u64 storage keeps the buffer 8-byte aligned for the timeval read.
        let mut storage = [0u64; KINFO_PROC_SIZE / 8];
        let mut len: libc::size_t = KINFO_PROC_SIZE;
        // SAFETY: `storage` provides `len` writable bytes; sysctl writes at most `len`.
        let ret = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                4,
                storage.as_mut_ptr().cast(),
                &mut len,
                std::ptr::null_mut(),
                0,
            )
        };
        // A missing pid is success with len == 0.
        if ret != 0 || len < KINFO_PID_OFFSET + 4 {
            return None;
        }
        let bytes: &[u8] = unsafe {
            // SAFETY: reinterpreting the initialised u64 buffer as bytes.
            std::slice::from_raw_parts(storage.as_ptr().cast::<u8>(), KINFO_PROC_SIZE)
        };
        let reported_pid = i32::from_ne_bytes(
            bytes[KINFO_PID_OFFSET..KINFO_PID_OFFSET + 4]
                .try_into()
                .ok()?,
        );
        if reported_pid != pid {
            return None;
        }
        let seconds = i64::from_ne_bytes(bytes[0..8].try_into().ok()?);
        let micros = i32::from_ne_bytes(bytes[8..12].try_into().ok()?);
        let start = SystemTime::UNIX_EPOCH
            + Duration::from_secs(u64::try_from(seconds).ok()?)
            + Duration::from_micros(u64::try_from(micros.max(0)).ok()?);
        Some(KinfoFields {
            start,
            zombie: u32::from(bytes[KINFO_STAT_OFFSET]) == libc::SZOMB,
        })
    }

    fn comm_of(raw: &[libc::c_char]) -> String {
        let bytes: Vec<u8> = raw
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn effective_uid() -> u32 {
        // SAFETY: geteuid has no preconditions.
        unsafe { libc::geteuid() }
    }

    fn argv_of(pid: i32, uid: u32, own_uid: u32) -> Result<Vec<String>, Unreadable> {
        // KERN_PROCARGS2 refuses another user's process (EINVAL) unless the
        // caller is root; skip the doomed syscall and say why.
        if own_uid != 0 && uid != own_uid {
            return Err(Unreadable::PermissionDenied);
        }
        crate::process_env::read_process_argv(pid as u32).ok_or(Unreadable::Unavailable)
    }

    fn entry(pid: i32, want_argv: bool, own_uid: u32) -> Option<ProcInfo> {
        let short = short_info(pid)?;
        // DEFERRED (2026-11-23): one `KERN_PROC_PID` sysctl per process makes a full snapshot
        // quadratic kernel work on a stable table (XNU walks the process list under its
        // lock for each call). Measured cost is small at realistic process counts; indexing
        // one `KERN_PROC_ALL` read by pid (and a direct libproc start-time query for single
        // lookups) is the fix once a snapshot shows up in a profile.
        let kinfo = kinfo(pid);
        if kinfo.as_ref().is_some_and(|kinfo| kinfo.zombie) {
            return None;
        }
        let argv = if want_argv {
            argv_of(pid, short.pbsi_uid, own_uid)
        } else {
            Err(Unreadable::NotRequested)
        };
        Some(ProcInfo {
            pid: pid as u32,
            ppid: short.pbsi_ppid,
            pgid: short.pbsi_pgid,
            uid: short.pbsi_uid,
            comm: comm_of(&short.pbsi_comm),
            exe: exe(pid as u32),
            argv,
            start_time: kinfo.map(|kinfo| kinfo.start),
        })
    }

    pub(super) fn snapshot(want_argv: bool) -> Option<Vec<ProcInfo>> {
        let own_uid = effective_uid();
        let processes: Vec<ProcInfo> = all_pids()?
            .into_iter()
            .filter_map(|pid| entry(pid, want_argv, own_uid))
            .collect();
        (!processes.is_empty()).then_some(processes)
    }

    pub(super) fn info(pid: u32, want_argv: bool) -> Option<ProcInfo> {
        entry(i32::try_from(pid).ok()?, want_argv, effective_uid())
    }

    pub(super) fn children(ppid: u32) -> Option<Vec<u32>> {
        let ppid = i32::try_from(ppid).ok()?;
        // SAFETY: a null buffer asks only for the current count.
        let estimate =
            checked_listing(|| unsafe { libc::proc_listchildpids(ppid, std::ptr::null_mut(), 0) })?;
        let pids = list_pids_into(estimate as usize, |buffer, bytes| {
            // SAFETY: `buffer` holds `bytes` writable bytes.
            unsafe { libc::proc_listchildpids(ppid, buffer, bytes) }
        })?;
        Some(pids.into_iter().map(|pid| pid as u32).collect())
    }

    pub(super) fn zombie_children(ppid: u32) -> Option<Vec<u32>> {
        Some(
            children(ppid)?
                .into_iter()
                .filter(|&pid| kinfo(pid as i32).is_some_and(|kinfo| kinfo.zombie))
                .collect(),
        )
    }

    /// `pti_total_*` count mach absolute-time ticks, which are nanoseconds only
    /// on Intel; Apple Silicon needs the timebase (125/3).
    fn mach_ticks_to_duration(ticks: u64) -> Duration {
        static TIMEBASE: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();
        // `libc`'s binding is deprecated in favour of the `mach2` crate; this
        // one call does not justify a dependency, so declare it locally.
        #[repr(C)]
        struct MachTimebaseInfo {
            numer: u32,
            denom: u32,
        }
        unsafe extern "C" {
            fn mach_timebase_info(info: *mut MachTimebaseInfo) -> libc::c_int;
        }
        let (numer, denom) = *TIMEBASE.get_or_init(|| {
            let mut info = MachTimebaseInfo { numer: 0, denom: 0 };
            // SAFETY: mach_timebase_info fills the provided struct.
            let ok = unsafe { mach_timebase_info(&raw mut info) } == 0;
            if ok && info.numer != 0 && info.denom != 0 {
                (u64::from(info.numer), u64::from(info.denom))
            } else {
                (1, 1)
            }
        });
        let nanos = u128::from(ticks) * u128::from(numer) / u128::from(denom);
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }

    pub(super) fn usage(pid: u32) -> Option<ProcUsage> {
        let pid = i32::try_from(pid).ok()?;
        let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
        // SAFETY: `info` is a writable proc_taskinfo of exactly `size` bytes.
        let ret = unsafe {
            libc::proc_pidinfo(pid, libc::PROC_PIDTASKINFO, 0, (&raw mut info).cast(), size)
        };
        if ret != size {
            return None;
        }
        Some(ProcUsage {
            rss_bytes: info.pti_resident_size,
            cpu_time: mach_ticks_to_duration(
                info.pti_total_user.saturating_add(info.pti_total_system),
            ),
            threads: u32::try_from(info.pti_threadnum).unwrap_or(0),
        })
    }

    pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
        let pid = i32::try_from(pid).ok()?;
        let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
        // SAFETY: `info` is a writable proc_vnodepathinfo of exactly `size` bytes.
        let ret = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDVNODEPATHINFO,
                0,
                (&raw mut info).cast(),
                size,
            )
        };
        if ret != size {
            return None;
        }
        let raw: Vec<u8> = info
            .pvi_cdir
            .vip_path
            .iter()
            .flatten()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        if raw.is_empty() {
            return None;
        }
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(raw)))
    }

    pub(super) fn exe(pid: u32) -> Option<PathBuf> {
        let pid = i32::try_from(pid).ok()?;
        // PROC_PIDPATHINFO_MAXSIZE (4 * MAXPATHLEN).
        let mut buf = vec![0u8; 4 * libc::MAXPATHLEN as usize];
        // SAFETY: proc_pidpath writes at most `buf.len()` bytes into `buf`.
        let ret = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if ret <= 0 {
            return None;
        }
        buf.truncate(ret as usize);
        use std::os::unix::ffi::OsStringExt;
        Some(PathBuf::from(std::ffi::OsString::from_vec(buf)))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The pid table can grow between the count call and the listing
        /// call. A listing that fills the whole buffer may have been cut off,
        /// so it is retried with twice the room (at most four times), zero
        /// pids are dropped, and a failing call is `None`.
        #[test]
        fn pid_listing_grows_until_the_table_fits_and_gives_up_after_four_tries() {
            use std::cell::RefCell;
            let table: Vec<i32> = (1..=300).collect();
            let capacities = RefCell::new(Vec::new());
            let pids = list_pids_into(100, |buffer, bytes| {
                let capacity = bytes as usize / std::mem::size_of::<i32>();
                capacities.borrow_mut().push(capacity);
                let written = table.len().min(capacity);
                // SAFETY: `buffer` holds `capacity` i32s; at most that many are written.
                let out = unsafe { std::slice::from_raw_parts_mut(buffer.cast::<i32>(), written) };
                out.copy_from_slice(&table[..written]);
                written as libc::c_int
            })
            .expect("the table fits on the second try");
            assert_eq!(pids, table);
            assert_eq!(*capacities.borrow(), vec![164, 328]);

            let tries = std::cell::Cell::new(0);
            let never_fits = list_pids_into(1, |_, bytes| {
                tries.set(tries.get() + 1);
                (bytes as usize / std::mem::size_of::<i32>()) as libc::c_int
            });
            assert_eq!(never_fits, None);
            assert_eq!(tries.get(), 4);

            assert_eq!(list_pids_into(10, |_, _| -1), None);
            let with_zero = list_pids_into(10, |buffer, _| {
                // SAFETY: the buffer holds at least 74 i32s.
                let out = unsafe { std::slice::from_raw_parts_mut(buffer.cast::<i32>(), 3) };
                out.copy_from_slice(&[0, 7, 9]);
                3
            });
            assert_eq!(with_zero, Some(vec![7, 9]));
        }
    }
}

// ─── Linux ──────────────────────────────────────────────────────────────────

/// Decode `/proc/<pid>/cmdline`: NUL-TERMINATED arguments. Only the final terminator
/// is dropped, so an empty argument (a process started with an empty `argv[0]`) stays
/// in its slot; an empty buffer (kernel threads, zombies) has no argv at all.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn decode_cmdline(buf: &[u8]) -> Result<Vec<String>, Unreadable> {
    if buf.is_empty() {
        return Err(Unreadable::Unavailable);
    }
    let body = buf.strip_suffix(&[0]).unwrap_or(buf);
    Ok(body
        .split(|&b| b == 0)
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect())
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{ProcInfo, ProcUsage, Unreadable};
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    /// The fields of `/proc/<pid>/stat` this module reads.
    struct Stat {
        comm: String,
        state: char,
        ppid: u32,
        pgid: u32,
        utime_ticks: u64,
        stime_ticks: u64,
        threads: u32,
        start_ticks: u64,
        rss_pages: u64,
    }

    /// Parse `/proc/<pid>/stat`. `comm` is parenthesised and may itself
    /// contain spaces and parens, so split on the LAST `)`.
    fn parse_stat(text: &str) -> Option<Stat> {
        let open = text.find('(')?;
        let close = text.rfind(')')?;
        let comm = text.get(open + 1..close)?.to_string();
        // fields[0] is field 3 (state) of proc(5).
        let fields: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
        let field = |n: usize| fields.get(n - 3).copied();
        Some(Stat {
            comm,
            state: field(3)?.chars().next()?,
            ppid: field(4)?.parse().ok()?,
            pgid: field(5)?.parse().ok()?,
            utime_ticks: field(14)?.parse().ok()?,
            stime_ticks: field(15)?.parse().ok()?,
            threads: field(20)?.parse().ok()?,
            start_ticks: field(22)?.parse().ok()?,
            rss_pages: field(24)?.parse().ok()?,
        })
    }

    fn stat_of(pid: u32) -> Option<Stat> {
        // Bytes, decoded lossily: the kernel writes `comm` raw, so a name holding a non-UTF-8
        // byte would make `read_to_string` fail and a live process read as absent (its
        // children then lose their parent in the tree).
        let bytes = std::fs::read(format!("/proc/{pid}/stat")).ok()?;
        parse_stat(&String::from_utf8_lossy(&bytes))
    }

    fn clock_ticks_per_second() -> u64 {
        // SAFETY: sysconf has no preconditions.
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        u64::try_from(ticks).ok().filter(|&t| t > 0).unwrap_or(100)
    }

    fn page_size() -> u64 {
        // SAFETY: sysconf has no preconditions.
        let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        u64::try_from(size).ok().filter(|&s| s > 0).unwrap_or(4096)
    }

    fn boot_time() -> Option<SystemTime> {
        let text = std::fs::read_to_string("/proc/stat").ok()?;
        let seconds: u64 = text
            .lines()
            .find_map(|line| line.strip_prefix("btime "))?
            .trim()
            .parse()
            .ok()?;
        Some(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
    }

    fn ticks_to_duration(ticks: u64, per_second: u64) -> Duration {
        Duration::from_secs(ticks / per_second)
            + Duration::from_nanos((ticks % per_second) * 1_000_000_000 / per_second)
    }

    fn effective_uid_of(pid: u32) -> Option<u32> {
        // Bytes, not `read_to_string`: the `Name:` line carries the kernel's raw `comm`, so a
        // non-UTF-8 name would fail the whole read and a live process would report no uid
        // even though the numeric `Uid:` line is intact.
        let status = std::fs::read(format!("/proc/{pid}/status")).ok()?;
        parse_effective_uid(&status)
    }

    /// The effective uid (second field of `Uid:`), decoding only that numeric line.
    fn parse_effective_uid(status: &[u8]) -> Option<u32> {
        let line = status
            .split(|&b| b == b'\n')
            .find_map(|line| line.strip_prefix(b"Uid:"))?;
        std::str::from_utf8(line)
            .ok()?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()
    }

    fn argv_of(pid: u32) -> Result<Vec<String>, Unreadable> {
        match std::fs::read(format!("/proc/{pid}/cmdline")) {
            Ok(buf) => super::decode_cmdline(&buf),
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(Unreadable::PermissionDenied)
            }
            Err(_) => Err(Unreadable::Unavailable),
        }
    }

    fn all_pids() -> Option<Vec<u32>> {
        let entries = std::fs::read_dir("/proc").ok()?;
        Some(
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
                .collect(),
        )
    }

    fn entry(pid: u32, want_argv: bool, boot: Option<SystemTime>, hz: u64) -> Option<ProcInfo> {
        let stat = stat_of(pid)?;
        if stat.state == 'Z' {
            return None;
        }
        Some(ProcInfo {
            pid,
            ppid: stat.ppid,
            pgid: stat.pgid,
            uid: effective_uid_of(pid).unwrap_or(u32::MAX),
            comm: stat.comm,
            exe: exe(pid),
            argv: if want_argv {
                argv_of(pid)
            } else {
                Err(Unreadable::NotRequested)
            },
            start_time: boot.map(|boot| boot + ticks_to_duration(stat.start_ticks, hz)),
        })
    }

    pub(super) fn snapshot(want_argv: bool) -> Option<Vec<ProcInfo>> {
        let boot = boot_time();
        let hz = clock_ticks_per_second();
        let processes: Vec<ProcInfo> = all_pids()?
            .into_iter()
            .filter_map(|pid| entry(pid, want_argv, boot, hz))
            .collect();
        (!processes.is_empty()).then_some(processes)
    }

    pub(super) fn info(pid: u32, want_argv: bool) -> Option<ProcInfo> {
        entry(pid, want_argv, boot_time(), clock_ticks_per_second())
    }

    fn children_matching(ppid: u32, keep: impl Fn(&Stat) -> bool) -> Option<Vec<u32>> {
        Some(
            all_pids()?
                .into_iter()
                .filter(|&pid| stat_of(pid).is_some_and(|stat| stat.ppid == ppid && keep(&stat)))
                .collect(),
        )
    }

    pub(super) fn children(ppid: u32) -> Option<Vec<u32>> {
        children_matching(ppid, |_| true)
    }

    pub(super) fn zombie_children(ppid: u32) -> Option<Vec<u32>> {
        children_matching(ppid, |stat| stat.state == 'Z')
    }

    pub(super) fn usage(pid: u32) -> Option<ProcUsage> {
        let stat = stat_of(pid)?;
        Some(ProcUsage {
            rss_bytes: stat.rss_pages.saturating_mul(page_size()),
            cpu_time: ticks_to_duration(
                stat.utime_ticks.saturating_add(stat.stime_ticks),
                clock_ticks_per_second(),
            ),
            threads: stat.threads,
        })
    }

    pub(super) fn cwd(pid: u32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    pub(super) fn exe(pid: u32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/exe")).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn stat_parser_survives_a_non_utf8_comm() {
            let mut bytes = b"42 (we".to_vec();
            bytes.push(0xff);
            bytes.extend_from_slice(b"ird) S 7 42 42 0 -1 4194560 1 0 0 0 11 22 0 0 20 0 3 0 12345 1000 77 18446744073709551615");
            let stat = parse_stat(&String::from_utf8_lossy(&bytes)).expect("parses");
            assert_eq!((stat.ppid, stat.pgid), (7, 42));
            assert!(stat.comm.contains('\u{fffd}'));
        }

        // Catches: reading `status` as UTF-8, so a non-UTF-8 `Name:` made a live process
        // report no uid although its numeric `Uid:` line was intact.
        #[test]
        fn effective_uid_survives_a_non_utf8_name() {
            let mut status = b"Name:\tweird".to_vec();
            status.push(0xff);
            status.extend_from_slice(
                b"\nUmask:\t0022\nUid:\t1000\t1001\t1000\t1000\nGid:\t5\t5\t5\t5\n",
            );
            assert_eq!(parse_effective_uid(&status), Some(1001));
            assert_eq!(parse_effective_uid(b"Name:\tx\n"), None);
        }

        #[test]
        fn stat_parser_survives_parens_and_spaces_in_comm() {
            let text = "42 (we ird) (x)) S 7 42 42 0 -1 4194560 1 0 0 0 11 22 0 0 20 0 3 0 12345 1000 77 18446744073709551615";
            let stat = parse_stat(text).expect("parses");
            assert_eq!(stat.comm, "we ird) (x)");
            assert_eq!(stat.state, 'S');
            assert_eq!((stat.ppid, stat.pgid), (7, 42));
            assert_eq!((stat.utime_ticks, stat.stime_ticks), (11, 22));
            assert_eq!(stat.threads, 3);
            assert_eq!(stat.start_ticks, 12345);
            assert_eq!(stat.rss_pages, 77);
        }
    }
}

// ─── Everything else (Windows included) ────────────────────────────────────

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod platform {
    use super::{ProcInfo, ProcUsage};
    use std::path::PathBuf;

    pub(super) fn snapshot(_want_argv: bool) -> Option<Vec<ProcInfo>> {
        None
    }
    pub(super) fn info(_pid: u32, _want_argv: bool) -> Option<ProcInfo> {
        None
    }
    pub(super) fn children(_ppid: u32) -> Option<Vec<u32>> {
        None
    }
    pub(super) fn zombie_children(_ppid: u32) -> Option<Vec<u32>> {
        None
    }
    pub(super) fn usage(_pid: u32) -> Option<ProcUsage> {
        None
    }
    pub(super) fn cwd(_pid: u32) -> Option<PathBuf> {
        None
    }
    pub(super) fn exe(_pid: u32) -> Option<PathBuf> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Catches: dropping empty arguments, so `\0claude\0` decoded as ["claude"].
    #[test]
    fn cmdline_keeps_empty_arguments_and_drops_only_the_final_terminator() {
        assert_eq!(
            decode_cmdline(b"\0claude\0").unwrap(),
            vec![String::new(), "claude".to_string()]
        );
        assert_eq!(
            decode_cmdline(b"sh\0-c\0\0").unwrap(),
            vec!["sh".to_string(), "-c".to_string(), String::new()]
        );
        assert_eq!(decode_cmdline(b"a").unwrap(), vec!["a".to_string()]);
        assert_eq!(decode_cmdline(b"\0").unwrap(), vec![String::new()]);
        assert!(matches!(decode_cmdline(b""), Err(Unreadable::Unavailable)));
    }

    fn entry(argv: Result<Vec<String>, Unreadable>, exe: Option<&str>) -> ProcInfo {
        ProcInfo {
            pid: 10,
            ppid: 1,
            pgid: 10,
            uid: 0,
            comm: "zsh".to_string(),
            exe: exe.map(PathBuf::from),
            argv,
            start_time: None,
        }
    }

    #[test]
    fn display_name_prefers_argv0_then_exe_then_comm() {
        let login = entry(Ok(vec!["-zsh".into()]), Some("/bin/zsh"));
        assert_eq!(login.display_name(), "-zsh");
        let denied = entry(Err(Unreadable::PermissionDenied), Some("/bin/zsh"));
        assert_eq!(denied.display_name(), "/bin/zsh");
        let bare = entry(Err(Unreadable::PermissionDenied), None);
        assert_eq!(bare.display_name(), "zsh");
        let empty_argv0 = entry(Ok(vec![String::new()]), None);
        assert_eq!(empty_argv0.display_name(), "zsh");
    }

    #[test]
    fn lifetime_cpu_is_cpu_time_over_wall_time() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let now = start + Duration::from_secs(10);
        let usage = ProcUsage {
            rss_bytes: 0,
            cpu_time: Duration::from_secs(5),
            threads: 1,
        };
        assert!((lifetime_cpu_percent(&usage, start, now) - 50.0).abs() < 0.01);
        assert!(lifetime_cpu_percent(&usage, now, now).abs() < f32::EPSILON);
    }

    #[test]
    fn fake_source_without_argv_reports_not_requested() {
        let fake = FakeProcessSource::with_processes(vec![FakeProcessSource::process(
            5,
            1,
            "sleep",
            &["sleep", "30"],
        )]);
        let listed = fake.snapshot(false).unwrap();
        assert_eq!(listed[0].argv, Err(Unreadable::NotRequested));
        assert_eq!(fake.children(1), Some(vec![5]));
        assert!(FakeProcessSource::failing().snapshot(true).is_none());
    }

    fn started(pid: u32, ppid: u32, comm: &str, argv: &[&str]) -> ProcInfo {
        ProcInfo {
            start_time: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)),
            ..FakeProcessSource::process(pid, ppid, comm, argv)
        }
    }

    #[test]
    fn parent_map_groups_children_and_fails_with_the_enumeration() {
        let fake = FakeProcessSource::with_processes(vec![
            started(10, 1, "zsh", &["zsh"]),
            started(11, 10, "cargo", &["cargo"]),
            started(12, 10, "node", &["node"]),
        ]);
        let map = parent_map(&fake).unwrap();
        assert_eq!(map.get(&1), Some(&vec![10]));
        assert_eq!(map.get(&10), Some(&vec![11, 12]));
        assert!(parent_map(&FakeProcessSource::failing()).is_none());
    }

    #[test]
    fn usage_stats_report_kib_and_lifetime_cpu_and_skip_unreadable_pids() {
        let mut fake = FakeProcessSource::with_processes(vec![started(10, 1, "zsh", &["zsh"])]);
        fake.usages.insert(
            10,
            ProcUsage {
                rss_bytes: 4 * 1024 * 1024,
                cpu_time: Duration::from_secs(25),
                threads: 1,
            },
        );
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_100);
        let stats = usage_stats(&fake, &[10, 99], now);
        let (rss_kb, cpu) = stats[&10];
        assert_eq!(rss_kb, 4096);
        assert!((cpu - 25.0).abs() < 0.01, "{cpu}");
        assert!(!stats.contains_key(&99));
    }

    /// The process manager calls `usage_stats` for every pid of a snapshot, so
    /// it must stay cheap: only the usage and the start time, never `info`
    /// (which on macOS reads each process's argv with `KERN_PROCARGS2`).
    #[test]
    fn usage_stats_reads_only_the_start_time_never_argv() {
        struct NoInfo;
        impl ProcessSource for NoInfo {
            fn snapshot(&self, _: bool) -> Option<Vec<ProcInfo>> {
                None
            }
            fn children(&self, _: u32) -> Option<Vec<u32>> {
                None
            }
            fn info(&self, pid: u32) -> Option<ProcInfo> {
                panic!("usage_stats read the full info (argv) of {pid}")
            }
            fn usage(&self, pid: u32) -> Option<ProcUsage> {
                (pid == 10).then_some(ProcUsage {
                    rss_bytes: 2048,
                    cpu_time: Duration::from_secs(10),
                    threads: 1,
                })
            }
            fn cwd(&self, _: u32) -> Option<PathBuf> {
                None
            }
            fn exe(&self, _: u32) -> Option<PathBuf> {
                None
            }
            fn zombie_children(&self, _: u32) -> Option<Vec<u32>> {
                None
            }
            fn start_time(&self, pid: u32) -> Option<SystemTime> {
                (pid == 10).then_some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000))
            }
        }
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_100);
        let stats = usage_stats(&NoInfo, &[10, 11], now);
        assert_eq!(stats.len(), 1);
        let (rss_kb, cpu) = stats[&10];
        assert_eq!(rss_kb, 2);
        assert!((cpu - 10.0).abs() < 0.01, "{cpu}");
    }

    #[test]
    fn children_cpu_lists_live_children_and_keeps_unreadable_usage_as_none() {
        let mut fake = FakeProcessSource::with_processes(vec![
            started(11, 10, "cargo", &["cargo"]),
            started(12, 10, "node", &["node"]),
            started(13, 1, "other", &["other"]),
        ]);
        fake.zombies.push((14, 10));
        fake.usages.insert(
            11,
            ProcUsage {
                rss_bytes: 0,
                cpu_time: Duration::from_secs(50),
                threads: 1,
            },
        );
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_100);
        let children = children_cpu(&fake, 10, now).unwrap();
        assert_eq!(children.len(), 2, "the zombie and the stranger are skipped");
        assert_eq!(children[0].comm, "cargo");
        assert!((children[0].cpu_percent.unwrap() - 50.0).abs() < 0.01);
        assert_eq!(children[1].cpu_percent, None);
    }

    // Catches: a pid listed as our child that was reused by an unrelated process before
    // `info` ran being reported (and its CPU attributed) as our child.
    #[test]
    fn children_cpu_skips_a_listed_pid_whose_parent_changed() {
        let mut fake = FakeProcessSource::with_processes(vec![
            started(11, 10, "cargo", &["cargo"]),
            started(13, 1, "reused", &["reused"]),
        ]);
        // The fake lists zombie entries as children; pid 13 is also a live process under pid 1,
        // which is exactly what a reused pid looks like to `children` + `info`.
        fake.zombies.push((13, 10));
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_100);
        let children = children_cpu(&fake, 10, now).unwrap();
        assert_eq!(
            children.iter().map(|c| c.pid).collect::<Vec<_>>(),
            vec![11],
            "the reused pid belongs to pid 1 now"
        );
    }

    #[test]
    fn unreadable_subtree_member_is_found_but_strangers_are_ignored() {
        let mut denied = started(12, 11, "zsh", &[]);
        denied.argv = Err(Unreadable::PermissionDenied);
        let mut root_owned_elsewhere = started(50, 1, "launchd", &[]);
        root_owned_elsewhere.argv = Err(Unreadable::PermissionDenied);
        let processes = vec![
            started(10, 1, "tuic", &["tuic"]),
            started(11, 10, "sudo", &["sudo", "su"]),
            denied,
            root_owned_elsewhere,
        ];
        assert_eq!(
            first_unreadable_in_subtree(&processes, 10).map(|p| p.pid),
            Some(12)
        );
        assert!(
            first_unreadable_in_subtree(&processes[..2], 10).is_none(),
            "a fully readable subtree needs nothing more"
        );
        let mut ageless = processes[..2].to_vec();
        ageless[1].start_time = None;
        assert_eq!(
            first_unreadable_in_subtree(&ageless, 10).map(|p| p.pid),
            Some(11),
            "a missing start time is unreadable too"
        );
    }

    /// Real child processes: identity, argv, cwd, start time, children, usage
    /// and zombie state must all come from the native calls (no `ps`).
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    mod live {
        use super::super::*;
        use std::os::unix::process::CommandExt;
        use std::process::{Command, Stdio};

        struct KillOnDrop(std::process::Child);
        impl Drop for KillOnDrop {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }

        fn spawn_sleep(cwd: &std::path::Path) -> KillOnDrop {
            KillOnDrop(
                Command::new("/bin/sleep")
                    .arg("30")
                    .current_dir(cwd)
                    .process_group(0)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("spawn /bin/sleep"),
            )
        }

        fn canonical(path: &std::path::Path) -> PathBuf {
            std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
        }

        #[test]
        fn a_live_child_reports_its_identity_argv_cwd_and_start() {
            let dir = tempfile_dir();
            let child = spawn_sleep(&dir);
            let pid = child.0.id();
            let own = std::process::id();

            let info = info(pid).expect("our own child is inspectable");
            assert_eq!(info.pid, pid);
            assert_eq!(info.ppid, own);
            assert_eq!(info.pgid, pid, "process_group(0) makes it a group leader");
            assert_eq!(info.comm, "sleep");
            assert_eq!(
                info.argv,
                Ok(vec!["/bin/sleep".to_string(), "30".to_string()])
            );
            assert!(
                info.exe
                    .as_deref()
                    .is_some_and(|exe| exe.to_string_lossy().ends_with("sleep")),
                "exe: {:?}",
                info.exe
            );
            assert_eq!(info.display_name(), "/bin/sleep");
            let age = info
                .age_seconds(SystemTime::now())
                .expect("the start time is readable");
            assert!(age <= 5, "a just-spawned child is {age}s old");

            assert_eq!(cwd(pid).map(|p| canonical(&p)), Some(canonical(&dir)));
            assert!(children(own).expect("children").contains(&pid));

            let snap = snapshot(true).expect("the process table is listable");
            let listed = snap
                .iter()
                .find(|process| process.pid == pid)
                .expect("snapshot lists the child");
            assert_eq!(listed.ppid, own);
            assert!(listed.argv.is_ok());
            assert!(
                snap.iter().any(|process| process.pid == own),
                "snapshot lists ourselves"
            );
            let without_argv = snapshot(false).expect("listable without argv");
            assert!(
                without_argv
                    .iter()
                    .all(|process| process.argv == Err(Unreadable::NotRequested))
            );
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn own_usage_reports_memory_threads_and_cpu() {
            let own = std::process::id();
            let usage = usage(own).expect("own usage is readable");
            assert!(usage.rss_bytes > 1024 * 1024, "rss {}", usage.rss_bytes);
            assert!(usage.threads >= 1);
            // Burn a little CPU so the counter is non-zero on every platform.
            let mut x = 0u64;
            for i in 0..5_000_000u64 {
                x = x.wrapping_add(i * i);
            }
            std::hint::black_box(x);
            let later = super::super::usage(own).expect("still readable");
            assert!(later.cpu_time > Duration::ZERO);
            // CPU time is nanoseconds, not raw mach ticks: a test process cannot
            // have used more CPU than (threads x wall time since it started).
            let start = info(own).and_then(|p| p.start_time).expect("own start");
            let wall = SystemTime::now().duration_since(start).unwrap_or_default();
            assert!(
                later.cpu_time <= wall * later.threads.max(1) + Duration::from_secs(1),
                "cpu {:?} vs wall {:?} x {} threads",
                later.cpu_time,
                wall,
                later.threads
            );
        }

        #[test]
        fn an_unreaped_child_is_a_zombie_until_waited() {
            let mut child = Command::new("/usr/bin/true")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn /usr/bin/true");
            let pid = child.id();
            let own = std::process::id();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !zombie_children(own).unwrap_or_default().contains(&pid) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "child {pid} never showed up as a zombie"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(info(pid).is_none(), "a zombie is not a live process");
            assert!(
                snapshot(false)
                    .unwrap_or_default()
                    .iter()
                    .all(|process| process.pid != pid),
                "snapshots skip zombies"
            );
            assert!(is_alive(pid), "kill(pid, 0) still sees an unreaped zombie");
            child.wait().expect("reap");
            assert!(!zombie_children(own).unwrap_or_default().contains(&pid));
            assert!(!is_alive(pid));
        }

        #[test]
        fn a_dead_pid_reads_as_nothing() {
            let mut child = Command::new("/usr/bin/true").spawn().expect("spawn");
            let pid = child.id();
            child.wait().expect("reap");
            assert!(info(pid).is_none());
            assert!(usage(pid).is_none());
            assert!(cwd(pid).is_none());
            assert!(!is_alive(pid));
            assert!(!signal(pid, libc::SIGTERM));
        }

        #[test]
        fn signal_refuses_group_and_init_targets() {
            assert!(!signal(0, 0));
            assert!(!signal(1, 0));
            assert!(!signal(u32::MAX, 0));
        }

        /// The cheap start-time read agrees with the full `info`.
        #[test]
        fn start_time_matches_info_without_reading_argv() {
            let own = std::process::id();
            let full = info(own).and_then(|process| process.start_time);
            assert!(full.is_some());
            assert_eq!(NativeProcessSource.start_time(own), full);
            assert_eq!(NativeProcessSource.start_time(u32::MAX), None);
        }

        /// EPERM: another user's process (launchd, root) keeps its identity but
        /// its argv is `PermissionDenied`, never an empty `Ok` that would read
        /// as "no command". macOS only: Linux lets any user read `cmdline`.
        #[cfg(target_os = "macos")]
        #[test]
        fn another_users_process_has_identity_but_permission_denied_argv() {
            // SAFETY: geteuid has no preconditions.
            if unsafe { libc::geteuid() } == 0 {
                eprintln!("UNCHECKED: running as root, every argv is readable");
                return;
            }
            let launchd = info(1).expect("launchd's identity is readable by every uid");
            assert_eq!(launchd.uid, 0);
            assert_eq!(launchd.comm, "launchd");
            assert_eq!(launchd.argv, Err(Unreadable::PermissionDenied));
            assert_eq!(
                launchd.display_name(),
                launchd.exe.as_ref().map_or(launchd.comm.clone(), |exe| exe
                    .to_string_lossy()
                    .into_owned())
            );
        }

        /// A process whose executable was deleted while it runs (an upgrade
        /// replacing an agent binary) is still a live, readable process: its
        /// identity and argv do not vanish with the file.
        #[cfg(feature = "test-support")]
        #[test]
        fn a_process_whose_executable_was_deleted_is_still_readable() {
            let dir = tempfile_dir();
            let probe = tuic_test_support::install_identity_probe(&dir, "deleted-probe");
            let child = KillOnDrop(
                Command::new(&probe)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("spawn the identity probe"),
            );
            let pid = child.0.id();
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            // comm changes at exec, a moment before the argument vector is readable: wait for both,
            // or the read after the delete below can race the exec under load.
            while info(pid).is_none_or(|p| p.comm != "deleted-probe" || p.argv.is_err()) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "probe {pid} never showed up"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            std::fs::remove_file(&probe).expect("delete the running executable");

            let after = info(pid).expect("a deleted executable does not hide its process");
            assert_eq!(after.comm, "deleted-probe");
            assert_eq!(
                after
                    .argv
                    .as_deref()
                    .ok()
                    .and_then(|argv| argv.first().cloned()),
                Some(probe.to_string_lossy().into_owned())
            );
            // The path, when the kernel still reports one, still names the
            // probe (Linux appends " (deleted)").
            if let Some(exe) = after.exe {
                let name = exe.file_name().unwrap().to_string_lossy().into_owned();
                assert!(name.starts_with("deleted-probe"), "{}", exe.display());
            }
            drop(child);
            let _ = std::fs::remove_dir_all(&dir);
        }

        /// The native table against the real `ps`, where `ps` can run (it
        /// cannot in the agent sandbox: the setuid exec is denied, and the test
        /// says UNCHECKED). For every pid both list, the parent, group and uid
        /// agree, and the native snapshot misses no more than churn explains.
        #[cfg(target_os = "macos")]
        #[test]
        fn native_snapshot_agrees_with_ps() {
            let ps = match Command::new("/bin/ps")
                .args(["-axo", "pid=,ppid=,pgid=,uid="])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
            {
                Ok(output) if output.status.success() => output,
                other => {
                    eprintln!("UNCHECKED: /bin/ps cannot run here: {other:?}");
                    return;
                }
            };
            let native: HashMap<u32, ProcInfo> = snapshot(false)
                .expect("the native table is listable")
                .into_iter()
                .map(|process| (process.pid, process))
                .collect();
            let mut compared = 0;
            let mut missing = Vec::new();
            for line in String::from_utf8_lossy(&ps.stdout).lines() {
                let fields: Vec<u32> = line
                    .split_whitespace()
                    .filter_map(|field| field.parse().ok())
                    .collect();
                let [pid, ppid, pgid, uid] = fields[..] else {
                    continue;
                };
                let Some(process) = native.get(&pid) else {
                    missing.push(pid);
                    continue;
                };
                assert_eq!(
                    (process.ppid, process.pgid, process.uid),
                    (ppid, pgid, uid),
                    "pid {pid}: native {process:?} vs ps {line:?}"
                );
                compared += 1;
            }
            assert!(compared > 10, "ps and native shared only {compared} pids");
            // Processes that exited between the two listings (and ps's own
            // short-lived pid) are fine; a large gap is not.
            assert!(
                missing.len() * 20 <= compared,
                "native snapshot missed {} of {} ps pids: {missing:?}",
                missing.len(),
                compared + missing.len()
            );
            assert!(native.contains_key(&std::process::id()));
        }

        fn tempfile_dir() -> PathBuf {
            let base = std::env::var_os("TUIC_TEST_TMP_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir);
            let dir = base.join(format!(
                "process-info-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).expect("create temp cwd");
            dir
        }
    }
}
