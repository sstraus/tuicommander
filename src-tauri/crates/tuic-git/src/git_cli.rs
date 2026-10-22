//! Unified git subprocess helper.
//!
//! Every git CLI invocation in the app should go through this module.
//! It wraps `Command::new(resolve_cli("git"))`, captures output, and
//! returns typed results with consistent error handling.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use tuic_core::cli::{enriched_path, resolve_cli};

#[cfg(test)]
thread_local! {
    static COMMAND_COUNTS: std::cell::RefCell<std::collections::BTreeMap<String, usize>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Thread-local so parallel fixture tests cannot contaminate spawn measurements.
#[cfg(test)]
pub(crate) fn take_command_counts() -> std::collections::BTreeMap<String, usize> {
    COMMAND_COUNTS.with(|counts| std::mem::take(&mut *counts.borrow_mut()))
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error from a git subprocess call.
#[derive(Debug)]
pub enum GitError {
    /// The git process could not be spawned (missing binary, permission error).
    SpawnFailed(std::io::Error),
    /// Git exited with a non-zero status code.
    NonZeroExit { code: Option<i32>, stderr: String },
    /// Git outlived its deadline and was killed. Only reachable when the caller
    /// set one with [`GitCmd::timeout`].
    TimedOut { after: Duration },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SpawnFailed(e) => write!(f, "Failed to spawn git: {e}"),
            Self::NonZeroExit { code, stderr } => {
                let code_str = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string());
                if stderr.is_empty() {
                    write!(f, "git exited with code {code_str}")
                } else {
                    write!(f, "git exited with code {code_str}: {stderr}")
                }
            }
            Self::TimedOut { after } => {
                write!(
                    f,
                    "git timed out after {:.1}s and was killed",
                    after.as_secs_f64()
                )
            }
        }
    }
}

impl From<GitError> for String {
    fn from(e: GitError) -> String {
        e.to_string()
    }
}

// ---------------------------------------------------------------------------
// Output type
// ---------------------------------------------------------------------------

/// Successful output from a git subprocess.
#[derive(Debug)]
pub struct GitOutput {
    pub stdout: String,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Builder for configuring and running a git subprocess.
///
/// # Examples
/// ```ignore
/// let out = git_cmd(repo_path)
///     .args(&["log", "--oneline", "-5"])
///     .run()?;
/// ```
pub struct GitCmd {
    cmd: Command,
    cwd: PathBuf,
    /// Deadline for the whole invocation. `None` (the default) waits forever,
    /// which is right for the local reads that dominate this module and wrong
    /// for anything that touches a network or a user script.
    timeout: Option<Duration>,
}

impl GitCmd {
    /// Add multiple arguments.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cmd.args(args);
        self
    }

    /// Set an environment variable for the subprocess.
    pub fn env(mut self, key: &str, val: &str) -> Self {
        self.cmd.env(key, val);
        self
    }

    /// Kill the invocation if it has not finished within `timeout`.
    ///
    /// Use it for anything that can block on something outside this machine's
    /// control — a network fetch, a user-supplied setup script — where the
    /// alternative to a deadline is a thread parked forever.
    ///
    /// Every `git fetch` in the app passes [`FETCH_TIMEOUT`].
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Run the command to completion, honoring [`GitCmd::timeout`] when set.
    fn output(&mut self, input: Option<&[u8]>) -> Result<std::process::Output, GitError> {
        #[cfg(test)]
        COMMAND_COUNTS.with(|counts| {
            let args = self
                .cmd
                .get_args()
                .map(|arg| arg.to_string_lossy())
                .collect::<Vec<_>>();
            let family = args
                .iter()
                .scan(false, |skip_operand, arg| {
                    let skip = *skip_operand;
                    *skip_operand = matches!(arg.as_ref(), "-c" | "-C");
                    Some((arg, skip))
                })
                .find(|(arg, skip)| !skip && !arg.starts_with('-'))
                .map(|(arg, _)| arg.to_string())
                .unwrap_or_default();
            *counts.borrow_mut().entry(family).or_default() += 1;
        });
        if let Some(input) = input {
            return output_with_input(&mut self.cmd, self.timeout, Some(input));
        }
        match self.timeout {
            Some(t) => output_with_deadline(&mut self.cmd, t),
            None => self.cmd.output().map_err(GitError::SpawnFailed),
        }
    }

    /// Run the git command, requiring success (non-zero exit → `Err`).
    ///
    /// Returns `GitOutput` containing raw (untrimmed) stdout on success.
    pub fn run(mut self) -> Result<GitOutput, GitError> {
        let output = self.output(None)?;
        Self::checked_output(output)
    }

    /// Run git with a finite stdin payload, requiring success.
    pub fn run_with_input(mut self, input: &str) -> Result<GitOutput, GitError> {
        let output = self.output(Some(input.as_bytes()))?;
        Self::checked_output(output)
    }

    fn checked_output(output: std::process::Output) -> Result<GitOutput, GitError> {
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(GitError::NonZeroExit {
                code: output.status.code(),
                stderr,
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(GitOutput { stdout })
    }

    /// Run the git command, returning `None` on non-zero exit.
    /// Spawn failures are logged to stderr (they indicate a broken git
    /// installation, not normal git behavior).
    pub fn run_silent(self) -> Option<GitOutput> {
        let cwd = self.cwd.clone();
        match self.run() {
            Ok(o) => Some(o),
            Err(GitError::SpawnFailed(e)) => {
                // Use warn for "No such file or directory" — stale worktree entries are expected.
                // Reserve error for unexpected spawn failures (broken git installation).
                if e.kind() == std::io::ErrorKind::NotFound {
                    tracing::warn!(
                        source = "git_cli",
                        "Spawn failed (dir missing): {}",
                        cwd.display()
                    );
                } else {
                    tracing::error!(source = "git_cli", "Spawn failed in {}: {e}", cwd.display());
                }
                None
            }
            Err(GitError::TimedOut { after }) => {
                tracing::warn!(
                    source = "git_cli",
                    "Timed out after {:.1}s in {}",
                    after.as_secs_f64(),
                    cwd.display()
                );
                None
            }
            Err(GitError::NonZeroExit { .. }) => None,
        }
    }

    /// Run the git command, returning the full `Output` struct regardless
    /// of exit code. Use for callsites that need to inspect exit code and
    /// stderr independently (e.g. `run_git_command` which never returns Err).
    pub fn run_raw(mut self) -> Result<std::process::Output, GitError> {
        self.output(None)
    }
}

/// Spawn `cmd`, collect its output, and kill it if it outlives `timeout`.
///
/// The pipes are drained by two reader threads so a child that fills a pipe
/// buffer cannot deadlock against our own wait. On the timeout path those
/// threads are deliberately NOT joined: a killed git can leave a grandchild
/// (a credential helper, a `core.askpass`) holding the write end open, and
/// joining would reintroduce exactly the unbounded wait the deadline exists to
/// prevent. They exit on their own once the last writer closes.
///
/// Not git-specific: the owner probe below runs `lsof` through it, and
/// `worktree.rs` runs the user's setup scripts through it.
pub fn output_with_deadline(
    cmd: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, GitError> {
    output_with_input(cmd, Some(timeout), None)
}

fn output_with_input(
    cmd: &mut Command,
    timeout: Option<Duration>,
    input: Option<&[u8]>,
) -> Result<std::process::Output, GitError> {
    use std::io::Read;
    use std::io::Write;

    // `Command::output()` nulls stdin; `spawn()` inherits it. Match `output()`,
    // or a deadlined child could park on a read of the app's stdin — the exact
    // unbounded wait this function exists to bound.
    let mut child = cmd
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(GitError::SpawnFailed)?;
    let writer = input.map(|input| {
        let input = input.to_vec();
        let mut stdin = child.stdin.take().expect("stdin piped above");
        std::thread::spawn(move || stdin.write_all(&input))
    });
    let mut out_pipe = child.stdout.take().expect("stdout piped above");
    let mut err_pipe = child.stderr.take().expect("stderr piped above");
    let out_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out_pipe.read_to_end(&mut buf);
        buf
    });
    let err_reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err_pipe.read_to_end(&mut buf);
        buf
    });

    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    // Backs off from 1ms to 50ms so a fast command is not delayed by the poll
    // granularity while a slow one costs almost no wakeups.
    let mut poll = Duration::from_millis(1);
    let status = loop {
        match child.try_wait().map_err(GitError::SpawnFailed)? {
            Some(status) => break status,
            None => {
                let remaining = deadline
                    .map(|deadline| deadline.saturating_duration_since(Instant::now()))
                    .unwrap_or(poll);
                if remaining.is_zero() {
                    // DEFERRED (2026-09-25) — kill/reap covers the direct child only.
                    // Killing a whole process tree needs process groups on
                    // Unix and Job Objects on Windows; the reader threads are
                    // deliberately not joined if a grandchild holds a pipe.
                    let _ = child.kill();
                    // Reap it, so a timeout never leaves a zombie behind.
                    let _ = child.wait();
                    return Err(GitError::TimedOut {
                        after: timeout.expect("deadline set"),
                    });
                }
                std::thread::sleep(poll.min(remaining));
                poll = (poll * 2).min(Duration::from_millis(50));
            }
        }
    };

    if let Some(writer) = writer {
        let written = writer.join().expect("stdin writer panicked");
        // Preserve Git's diagnostic if it rejected input and closed the pipe.
        if status.success() {
            written.map_err(GitError::SpawnFailed)?;
        }
    }
    Ok(std::process::Output {
        status,
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

/// Deadline for every `git fetch` in the app.
///
/// A fetch is the only git call here that waits on something off this machine,
/// so it is the only one that can park a blocking thread forever: a
/// `credential.helper` sitting on a prompt, a half-open TCP connection with no
/// keepalive, a wedged network mount. `GIT_TERMINAL_PROMPT=0` in [`git_cmd`]
/// stops git's own prompt but not any of those.
///
/// Three minutes is deliberately generous. It clears a dual-stack connect
/// timeout (~75s per address family), so a genuinely unreachable host still
/// reports git's own error rather than ours, and it leaves room for a large
/// incremental fetch on a slow link. Every fetch below is a single refspec into
/// an existing clone, never a clone, so the transfer is a branch delta — killing
/// one that would have succeeded is a worse outcome than waiting for it.
#[cfg(not(any(test, feature = "test-support")))]
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(180);

/// Test binaries also compile this crate as a dependency, where `cfg(test)`
/// is absent. The root crate enables `test-support` for that build.
#[cfg(any(test, feature = "test-support"))]
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Remove a stale `.git/index.lock` left behind by a crashed process so the
/// next git invocation isn't blocked with `Unable to create '.git/index.lock':
/// File exists` / `could not write index`.
///
/// Staleness is age-based, with two thresholds because the two crash modes
/// produce locks of different sizes and we want a wide margin over any live
/// git process:
///
/// - **Empty lock** (0 bytes): git created the lock but crashed before writing
///   the new index. A real in-progress write fills the lock almost immediately,
///   so a 0-byte lock older than [`EMPTY_LOCK_STALE_SECS`] is certainly orphaned.
/// - **Non-empty lock**: git wrote the new index into the lock but died before
///   renaming it over `.git/index` (e.g. Claude Code killed mid-`git stash`).
///   A legitimate index write finishes in well under a second; we wait
///   [`NONEMPTY_LOCK_STALE_SECS`] to stay safely clear of even large
///   `stash`/`add` operations before reclaiming.
///
/// A 0-byte lock is reclaimed after this many seconds (early crash, no index written yet).
const EMPTY_LOCK_STALE_SECS: u64 = 5;
/// A non-empty lock (index written, rename never happened) is reclaimed after this
/// many seconds — wide margin over even large `stash`/`add` index writes.
const NONEMPTY_LOCK_STALE_SECS: u64 = 30;

/// The escape hatch for a lock **no probe can adjudicate** (#694-4fcc).
///
/// The owner probe fails closed: with no answer we keep the lock, because the
/// thing fail-open destroys is a live `git add`'s index. But "keep it" cannot
/// mean "forever". On a host with no `lsof` the probe is permanently
/// unavailable, so a lock left by a crash would outlive the process that made
/// it and strand the repo behind a guard nothing can ever satisfy.
///
/// So ownership is adjudicated by a second, much stronger rule when the first
/// one has no answer: age alone, at a threshold no live git can reach. The
/// hazard fail-closed protects against is a git that is merely *slow*, and the
/// slowest legitimate index write is seconds, not an hour. A process holding
/// `index.lock` for [`UNADJUDICATED_LOCK_STALE_SECS`] is not slow, it is dead.
///
/// One hour is deliberately far above [`NONEMPTY_LOCK_STALE_SECS`]: this rule
/// runs *without* evidence, so it must be the one that almost never fires.
///
/// `pub`: also the escape-hatch threshold [`crate::git_locks`] reuses
/// so its read-only detector agrees with this module's adjudication instead
/// of inventing a second, independent one (#694-4fcc, #724-9909).
pub const UNADJUDICATED_LOCK_STALE_SECS: u64 = 3600;

/// Pure staleness rule for an `index.lock` of the given byte size and age.
/// Split out from [`remove_stale_index_lock`] so the thresholds are unit-testable
/// without touching the filesystem clock.
///
/// `pub`: [`crate::git_locks`] calls this directly rather than
/// re-deriving its own age/size threshold — see the note on
/// [`UNADJUDICATED_LOCK_STALE_SECS`].
pub fn is_index_lock_stale(len: u64, age_secs: u64) -> bool {
    let threshold = if len == 0 {
        EMPTY_LOCK_STALE_SECS
    } else {
        NONEMPTY_LOCK_STALE_SECS
    };
    age_secs >= threshold
}

/// How long the owner probe may take before we give up.
///
/// Past this we have **no answer**, which is not the same fact as "nobody holds
/// the lock" — see [`UnknownOwner::DeadlineExceeded`].
#[cfg(unix)]
const LOCK_OWNER_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// What the owner probe established about an `index.lock`.
///
/// Three outcomes, because two of them used to be one. The probe returned
/// `Option<Vec<u32>>`, and `None` meant both "nothing holds this lock" and "we
/// could not find out" — so an `lsof` that merely ran slowly was read downstream
/// as permission to delete a lock a live `git add` was holding.
///
/// `pub`: [`crate::git_locks`] matches on this directly so its
/// detector's verdict is derived from the same probe result this module
/// acts on, not a second call that could — even in principle — race to a
/// different answer.
///
/// `allow(dead_code)` off unix: there is no probe there (see
/// `probe_index_lock_owner` below), so only `Unknown` is ever constructed. The
/// other variants still have to exist — `reclaim_stale_index_lock` matches all
/// three on every platform — and cfg-ing them out would fork that match arm by
/// platform for no gain.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub enum LockOwnership {
    /// The probe ran and named the live processes holding the lock open.
    HeldBy(Vec<u32>),
    /// The probe ran and found no holder. The only outcome that is evidence.
    Unowned,
    /// The probe could not answer. Says nothing either way about a holder.
    Unknown(UnknownOwner),
}

/// Why the owner probe has no answer. The two cases are not interchangeable:
/// one is permanent, the other is a bad minute.
///
/// `allow(dead_code)` off unix for the same reason as [`LockOwnership`]: with no
/// probe to time out, only `Unavailable` is ever constructed there.
#[cfg_attr(not(unix), allow(dead_code))]
#[derive(Debug, PartialEq, Eq)]
pub enum UnknownOwner {
    /// The probe could not be run at all — no `lsof` on `PATH`, exec refused, or
    /// a platform with no probe. Nothing can be determined here, ever, so
    /// retrying costs a fork and buys nothing.
    Unavailable(String),
    /// `lsof` was installed, ran, and outlived the deadline. An answer existed
    /// and we ran out of patience for it, so the next attempt may well get one.
    /// Measured on this machine at 3.7s against a 2s deadline under several
    /// concurrent agents, and at 0.8s an hour later — the latency is variable,
    /// not a constant we can size a deadline against once and forget.
    DeadlineExceeded(Duration),
}

impl fmt::Display for UnknownOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(detail) => write!(f, "the owner probe could not run: {detail}"),
            Self::DeadlineExceeded(after) => {
                write!(f, "the owner probe outlived its {after:?} deadline")
            }
        }
    }
}

/// Read an `lsof -t` run into a [`LockOwnership`].
///
/// Split from the spawn so both failure shapes are testable without an `lsof`
/// that misbehaves on cue.
#[cfg(unix)]
fn classify_owner_probe(probe: Result<std::process::Output, GitError>) -> LockOwnership {
    let out = match probe {
        Ok(out) => out,
        Err(GitError::TimedOut { after }) => {
            return LockOwnership::Unknown(UnknownOwner::DeadlineExceeded(after));
        }
        // The io error is unwrapped rather than printed through `GitError`'s
        // Display, which opens with "Failed to spawn git". This is not git.
        Err(GitError::SpawnFailed(e)) => {
            return LockOwnership::Unknown(UnknownOwner::Unavailable(e.to_string()));
        }
        Err(e) => return LockOwnership::Unknown(UnknownOwner::Unavailable(e.to_string())),
    };

    // The exit code is deliberately not consulted, because it cannot separate the
    // two cases that matter: measured on macOS lsof 4.91, "nobody has this file
    // open" and "lsof could not stat the path" BOTH exit 1 with empty stdout.
    // Only stderr tells them apart, and `-w` does not hide it — that flag
    // silences warnings, not status errors.
    //
    // Order matters, and it is the safe direction twice over:
    //   1. PIDs win outright. A complaint does not outrank an answer, and
    //      `HeldBy` keeps the lock.
    //   2. Anything else with stderr is a probe that RAN AND FAILED, which is a
    //      non-answer — `Unknown`, never `Unowned`. `Unowned` licenses a delete,
    //      so letting a failure land there is a fail-open with no log and no name.
    //   3. Silence with no PIDs is the real "no match", the only outcome that is
    //      evidence.
    let pids: Vec<u32> = String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .filter_map(|pid| pid.parse::<u32>().ok())
        .collect();
    if !pids.is_empty() {
        return LockOwnership::HeldBy(pids);
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    let complaint = stderr.trim();
    if !complaint.is_empty() {
        return LockOwnership::Unknown(UnknownOwner::Unavailable(complaint.to_string()));
    }
    LockOwnership::Unowned
}

/// Which live processes currently hold `lock` open.
///
/// Only ever consulted for a lock the age rule has already condemned, so the
/// `lsof` fork happens at most once per reclaim attempt and never on the hot
/// path of an ordinary git call.
///
/// `pub`: shared with [`crate::git_locks`] — see
/// [`UNADJUDICATED_LOCK_STALE_SECS`].
#[cfg(unix)]
pub fn probe_index_lock_owner(lock: &Path) -> LockOwnership {
    // -t: PIDs only, one per line. -w: no warnings on unreadable mounts.
    //
    // Deadlined: this runs inside `git_cmd`, so an `lsof` stuck on a wedged
    // network mount would wedge every git call in the app.
    classify_owner_probe(output_with_deadline(
        Command::new("lsof").args([
            OsStr::new("-w"),
            OsStr::new("-t"),
            OsStr::new("--"),
            lock.as_os_str(),
        ]),
        LOCK_OWNER_PROBE_TIMEOUT,
    ))
}

/// No portable owner probe outside unix. Windows refuses to unlink a file another
/// process holds open, so the OS itself provides the protection `lsof` gives us here.
#[cfg(not(unix))]
pub fn probe_index_lock_owner(_lock: &Path) -> LockOwnership {
    LockOwnership::Unknown(UnknownOwner::Unavailable(
        "no owner probe on this platform".to_string(),
    ))
}

fn remove_stale_index_lock(cwd: &Path) {
    reclaim_stale_index_lock(cwd, probe_index_lock_owner);
}

/// [`remove_stale_index_lock`] with the owner probe injected, so each of the
/// probe's three answers can be driven from a test without an `lsof` that fails
/// on cue.
fn reclaim_stale_index_lock(cwd: &Path, probe: impl FnOnce(&Path) -> LockOwnership) {
    // The pointer must be followed, not assumed. In a linked worktree `.git` is a
    // FILE holding `gitdir: <path>`, so `cwd/.git/index.lock` traverses a file and
    // the `metadata` below fails — the whole sweep used to return here, silently,
    // in every worktree. TUIC runs most of its work in worktrees, so the guard was
    // inert exactly where it was needed most.
    let Some(git_dir) = crate::git::resolve_git_dir(cwd) else {
        return;
    };
    let lock = git_dir.join("index.lock");
    let Ok(meta) = std::fs::metadata(&lock) else {
        return;
    };

    // Without a reliable age we can't tell a stale lock from a live one — leave it.
    let Some(age_secs) = meta
        .modified()
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs())
    else {
        return;
    };

    if !is_index_lock_stale(meta.len(), age_secs) {
        return;
    }

    // Age says "crashed", but age cannot see a git that is merely slow. On a large
    // monorepo an `add`/`stash` index write can outrun the threshold, and deleting
    // the lock under it corrupts the index. Ask who holds it before reclaiming.
    match probe(&lock) {
        LockOwnership::HeldBy(pids) => {
            tracing::info!(
                source = "git_cli",
                "Keeping index.lock in {} — still held by {pids:?}",
                cwd.display()
            );
            return;
        }
        LockOwnership::Unowned => {}
        // FAIL CLOSED — Boss's decision, 2026-09-07 (#694-4fcc). With no answer
        // we keep the lock: the age rule alone cannot tell a crashed git from a
        // merely slow one, and reclaiming under a live `git add` corrupts the
        // index. A stranded repo is recoverable by hand; a corrupted index is
        // not, so the two costs are not symmetric.
        //
        // The escape hatch is age at a threshold no live git can reach, so
        // "keep" never becomes "forever" — see `UNADJUDICATED_LOCK_STALE_SECS`.
        // It matters most exactly where the probe can never work: on a host
        // with no `lsof`, `Unavailable` is permanent, and without this branch
        // the lock would outlive the process that left it.
        LockOwnership::Unknown(reason) if age_secs < UNADJUDICATED_LOCK_STALE_SECS => {
            tracing::warn!(
                source = "git_cli",
                "Keeping index.lock in {} — ownership could not be determined ({reason}), \
                 so it is kept until it reaches {UNADJUDICATED_LOCK_STALE_SECS}s \
                 (now {age_secs}s) rather than reclaimed on age alone",
                cwd.display()
            );
            return;
        }
        LockOwnership::Unknown(reason) => {
            tracing::warn!(
                source = "git_cli",
                "Reclaiming index.lock in {} on age alone: ownership could not be \
                 determined ({reason}), but at {age_secs}s it is past the \
                 {UNADJUDICATED_LOCK_STALE_SECS}s no live git can reach",
                cwd.display()
            );
        }
    }

    match std::fs::remove_file(&lock) {
        Ok(()) => {
            tracing::info!(
                source = "git_cli",
                "Removed stale index.lock ({} bytes, {age_secs}s old) in {}",
                meta.len(),
                cwd.display()
            );
        }
        Err(e) => {
            tracing::warn!(
                source = "git_cli",
                "Failed to remove stale index.lock in {}: {e}",
                cwd.display()
            );
        }
    }
}

/// Test-only count of git subprocesses built for a working directory.
///
/// Keyed by cwd rather than process-wide: the suite runs tests in parallel, so a
/// single counter would measure every other test's git calls. Each test owns its
/// own temp repo, so the key isolates it.
#[cfg(any(test, feature = "test-support"))]
static GIT_CMD_FORKS: std::sync::LazyLock<dashmap::DashMap<PathBuf, usize>> =
    std::sync::LazyLock::new(dashmap::DashMap::new);

/// How many git subprocesses were built for `cwd`. See [`GIT_CMD_FORKS`].
#[cfg(any(test, feature = "test-support"))]
pub fn git_cmd_forks(cwd: &Path) -> usize {
    GIT_CMD_FORKS.get(cwd).map(|n| *n).unwrap_or(0)
}

/// The extra argument a requested `git merge` needs so a user's `merge.ff=only`
/// cannot make it fail with "Not possible to fast-forward".
///
/// TUIC merges because the user asked for a merge (merge-and-archive, "update
/// from base" with the merge strategy). Under `merge.ff=only` git refuses
/// whenever the target has diverged, which is a policy for what a stray
/// `git merge`/`git pull` may do, not a statement that this merge should fail.
/// Returns `Some("--ff")` only when the effective fast-forward mode is `only`;
/// `--ff` still fast-forwards when it can and creates a merge commit when it
/// cannot. Default users (`merge.ff` unset) and `merge.ff=false`/`true` users
/// get `None`, so their behaviour is untouched. `branch` is the branch being
/// merged INTO: its `branch.<name>.mergeOptions` is applied by git ahead of
/// the command line, so the last `--ff`/`--no-ff`/`--ff-only` in it
/// decides, falling back to `merge.ff`.
pub fn merge_ff_override(cwd: &Path, branch: Option<&str>) -> Option<&'static str> {
    let read = |key: &str| -> Option<String> {
        git_cmd(cwd)
            .args(["config", "--get", key])
            .run_silent()
            .map(|o| o.stdout.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let from_options = branch
        .and_then(|b| read(&format!("branch.{b}.mergeOptions")))
        .and_then(|opts| {
            split_cmdline(&opts)?
                .iter()
                .rev()
                .find_map(|tok| match tok.as_str() {
                    "--ff-only" => Some(true),
                    "--ff" | "--no-ff" => Some(false),
                    _ => None,
                })
        });
    let ff_only = from_options
        .unwrap_or_else(|| read("merge.ff").is_some_and(|v| v.eq_ignore_ascii_case("only")));
    ff_only.then_some("--ff")
}

/// [`merge_ff_override`] for a merge run in `cwd` that merges into whatever
/// `cwd` has checked out.
///
/// Git applies `branch.<name>.mergeOptions` of the branch HEAD is on, not of a
/// branch the caller merely names, so the checked-out branch is resolved with
/// `symbolic-ref`. A detached HEAD has no branch and falls back to `merge.ff`.
///
/// The full ref is read and `refs/heads/` stripped by hand: `--short` prints
/// `heads/<name>` when a tag of the same name exists, which would look up
/// `branch.heads/<name>.mergeOptions` instead of the real branch's.
pub fn merge_ff_override_for_head(cwd: &Path) -> Option<&'static str> {
    let head = git_cmd(cwd)
        .args(["symbolic-ref", "--quiet", "HEAD"])
        .run_silent()
        .and_then(|o| {
            o.stdout
                .trim()
                .strip_prefix("refs/heads/")
                .filter(|b| !b.is_empty())
                .map(str::to_string)
        });
    merge_ff_override(cwd, head.as_deref())
}

/// Split a config value into words the way git's `split_cmdline` does (the
/// parser behind `branch.<name>.mergeOptions` and aliases): whitespace
/// separates words, `'` and `"` group (and are dropped), and a backslash
/// takes the next character literally. Returns `None` for an unterminated
/// quote or a trailing backslash, which git rejects as well.
fn split_cmdline(s: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) if c == '\\' => cur.push(chars.next()?),
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => {
                quote = Some(c);
                in_word = true;
            }
            None if c == '\\' => {
                cur.push(chars.next()?);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            None => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_word {
        words.push(cur);
    }
    Some(words)
}

/// Create a git command builder rooted at the given directory.
pub fn git_cmd(cwd: &Path) -> GitCmd {
    #[cfg(any(test, feature = "test-support"))]
    {
        *GIT_CMD_FORKS.entry(cwd.to_path_buf()).or_insert(0) += 1;
    }
    remove_stale_index_lock(cwd);
    let mut cmd = Command::new(resolve_cli("git"));
    cmd.current_dir(cwd);
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("PATH", enriched_path());
    // Git for Windows otherwise refuses valid deep worktree and ref paths.
    #[cfg(windows)]
    cmd.args(["-c", "core.longpaths=true"]);
    cmd.arg("--no-optional-locks");
    tuic_core::cli::apply_no_window(&mut cmd);
    GitCmd {
        cmd,
        cwd: cwd.to_path_buf(),
        timeout: None,
    }
}

fn finish_failed_operation_after_abort_result(
    operation: &str,
    failure_summary: &str,
    original_error: &str,
    abort_result: Result<(), GitError>,
) -> String {
    match abort_result {
        Ok(()) => format!("{failure_summary} (aborted): {original_error}"),
        Err(abort_error) => {
            tracing::error!(
                source = "git_cli",
                operation = %operation,
                original_error = %original_error,
                abort_error = %abort_error,
                "git {operation} --abort failed after failed {operation}"
            );
            format!(
                "{failure_summary}; repo left in a conflicted state, run git {operation} --abort manually: {abort_error}; original error: {original_error}"
            )
        }
    }
}

/// Abort a failed merge/rebase and describe the final state.
///
/// Returns a message that claims `(aborted)` only when the abort command
/// succeeds. If abort fails, the message tells the user the repository may still
/// be conflicted and includes the manual recovery command.
pub fn finish_failed_git_operation_after_abort(
    repo_path: &Path,
    operation: &str,
    failure_summary: &str,
    original_error: impl fmt::Display,
) -> String {
    let original_error = original_error.to_string();
    let abort_result = git_cmd(repo_path)
        .args([operation, "--abort"])
        .run()
        .map(|_| ());
    finish_failed_operation_after_abort_result(
        operation,
        failure_summary,
        &original_error,
        abort_result,
    )
}

/// Split a `git status --porcelain` v1 record into its XY status field and path.
///
/// The status codes are positional, so a path is only ever read from column 3
/// onward — never matched anywhere in the line. The mandatory space at index 2
/// also proves indices 2 and 3 are char boundaries, so the slices cannot panic.
fn split_porcelain_line(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    (bytes.len() >= 4 && bytes[2] == b' ').then(|| (&line[..2], &line[3..]))
}

/// True when an XY status field marks an unmerged (conflicted) path.
fn is_unmerged_code(code: &str) -> bool {
    matches!(code, "DD" | "AU" | "UD" | "UA" | "DU" | "AA" | "UU")
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_conflicted_files_porcelain(status: &str) -> Vec<String> {
    status
        .lines()
        .filter_map(|line| {
            let (code, path) = split_porcelain_line(line)?;
            is_unmerged_code(code).then(|| path.trim().to_string())
        })
        .filter(|path| !path.is_empty())
        .collect()
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn build_conflict_assist_prompt(pr_number: i64, base: &str, files: &[String]) -> String {
    let mut prompt = format!(
        "Resolve the merge conflicts for PR #{pr_number} after rebasing onto {base}. Do not push and do not merge. Edit only the conflicted files, run relevant checks, and stop for human review.\n\nConflicted files:"
    );
    for file in files {
        prompt.push_str(&format!("\n- {file}"));
    }
    prompt
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Helper: create a temp dir with `git init`.
    fn setup_test_repo() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let init = Command::new("git")
            .current_dir(&path)
            .args(["init"])
            .output()
            .expect("git init");
        assert!(init.status.success(), "git init failed: {init:?}");
        assert_repo_root(&path);
        Command::new("git")
            .current_dir(&path)
            .args(["config", "user.email", "test@test.com"])
            .output()
            .expect("git config email");
        Command::new("git")
            .current_dir(&path)
            .args(["config", "user.name", "Test"])
            .output()
            .expect("git config name");
        (dir, path)
    }

    /// Panic unless `path` is itself a git work-tree root. A failed `git init`
    /// under a parent repo would otherwise let later `git config` writes land
    /// in that parent's `.git/config`.
    fn assert_repo_root(path: &Path) {
        let out = Command::new("git")
            .current_dir(path)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .expect("git rev-parse");
        let top = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let want = std::fs::canonicalize(path).expect("canonicalize");
        assert!(
            out.status.success() && std::fs::canonicalize(&top).is_ok_and(|t| t == want),
            "{} is not its own repo root (toplevel: {top:?})",
            path.display()
        );
    }

    #[test]
    #[should_panic(expected = "is not its own repo root")]
    fn assert_repo_root_fires_for_non_repo_dir_under_a_repo() {
        // The crate's own checkout is a repo; a not-yet-initialised subdir of
        // a temp dir inside it resolves to the parent, which must be rejected.
        let parent = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).expect("tempdir");
        assert_repo_root(parent.path());
    }

    #[test]
    fn split_cmdline_matches_git() {
        let w = |s: &str| split_cmdline(s);
        let v = |a: &[&str]| Some(a.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(w("--ff-only"), v(&["--ff-only"]));
        assert_eq!(w("  --no-ff   -s  ours "), v(&["--no-ff", "-s", "ours"]));
        assert_eq!(w("\"--ff-only\""), v(&["--ff-only"]));
        assert_eq!(w("'--ff-only'"), v(&["--ff-only"]));
        assert_eq!(w("-m 'a b' \"c d\""), v(&["-m", "a b", "c d"]));
        assert_eq!(w("a\\ b"), v(&["a b"]));
        assert_eq!(w("''"), v(&[""]));
        assert_eq!(w("\"unterminated"), None);
        assert_eq!(w("trailing\\"), None);
        assert_eq!(w(""), v(&[]));
    }

    #[test]
    fn merge_ff_override_sees_quoted_merge_options() {
        let (_dir, path) = setup_test_repo();
        git_in(&path, &["config", "merge.ff", "true"]);
        for quoted in ["\"--ff-only\"", "'--ff-only'", "--no-ff \"--ff-only\""] {
            git_in(&path, &["config", "branch.dev.mergeOptions", quoted]);
            assert_eq!(
                merge_ff_override(&path, Some("dev")),
                Some("--ff"),
                "{quoted}"
            );
        }
    }

    fn git_in(path: &Path, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(path)
            .args(args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }

    #[test]
    fn merge_ff_override_only_fires_for_ff_only() {
        let (_dir, path) = setup_test_repo();
        // Each case sets merge.ff repo-locally, which outranks the machine's
        // global config (a developer's own merge.ff=only would otherwise leak in).
        for (value, want) in [
            ("only", Some("--ff")),
            ("ONLY", Some("--ff")),
            ("false", None),
            ("true", None),
        ] {
            git_in(&path, &["config", "merge.ff", value]);
            assert_eq!(merge_ff_override(&path, Some("main")), want, "{value}");
        }
    }

    #[test]
    fn merge_ff_override_honours_branch_merge_options() {
        let (_dir, path) = setup_test_repo();
        // Pin a neutral repo-local merge.ff so the "no mergeOptions" cases do
        // not pick up a developer's global merge.ff=only.
        git_in(&path, &["config", "merge.ff", "true"]);
        git_in(&path, &["config", "branch.dev.mergeOptions", "--ff-only"]);
        assert_eq!(merge_ff_override(&path, Some("dev")), Some("--ff"));
        assert_eq!(merge_ff_override(&path, Some("other")), None);
        assert_eq!(merge_ff_override(&path, None), None);
        // mergeOptions outranks merge.ff: an explicit --no-ff wins over merge.ff=only.
        git_in(&path, &["config", "merge.ff", "only"]);
        git_in(&path, &["config", "branch.dev.mergeOptions", "--no-ff"]);
        assert_eq!(merge_ff_override(&path, Some("dev")), None);
    }

    #[test]
    fn merge_ff_override_for_head_reads_the_checked_out_branch() {
        let (_dir, path) = setup_test_repo();
        git_in(&path, &["config", "merge.ff", "true"]);
        git_in(&path, &["commit", "--allow-empty", "-m", "init"]);
        git_in(&path, &["checkout", "-b", "feature"]);
        // The branch a caller might name is not the one checked out here.
        git_in(
            &path,
            &["config", "branch.feature.mergeOptions", "--ff-only"],
        );
        git_in(&path, &["config", "branch.other.mergeOptions", "--no-ff"]);
        assert_eq!(merge_ff_override_for_head(&path), Some("--ff"));
        // Detached HEAD: no branch, falls back to merge.ff (true here).
        git_in(&path, &["checkout", "--detach"]);
        assert_eq!(merge_ff_override_for_head(&path), None);
    }

    #[test]
    fn merge_ff_override_for_head_survives_a_tag_named_like_the_branch() {
        let (_dir, path) = setup_test_repo();
        git_in(&path, &["config", "merge.ff", "true"]);
        git_in(&path, &["commit", "--allow-empty", "-m", "init"]);
        git_in(&path, &["checkout", "-b", "feature"]);
        // With a tag `feature` too, `symbolic-ref --short HEAD` prints
        // `heads/feature`.
        git_in(&path, &["tag", "feature"]);
        git_in(
            &path,
            &["config", "branch.feature.mergeOptions", "--ff-only"],
        );
        assert_eq!(merge_ff_override_for_head(&path), Some("--ff"));
    }

    /// A child that outlives any deadline these tests set.
    ///
    /// `sleep` is not a Windows program. The tests below spawned it anyway and
    /// passed on GitHub's runner, which carries `C:\Program Files\Git\usr\bin`
    /// on `PATH` and so has a POSIX `sleep`; on a Windows machine without it
    /// the spawn fails and the test proves nothing about the timeout it names.
    ///
    /// Spawned directly, never under a shell: what these tests check is that
    /// the deadline kills the child, and killing a `cmd /C` wrapper leaves the
    /// real sleeper running — nextest saw exactly that and reported both tests
    /// as leaky while they passed.
    fn sleeping_command(cwd: &Path) -> Command {
        let (program, args) = tuic_test_support::sleep_argv();
        let mut cmd = Command::new(program);
        cmd.args(args);
        cmd.current_dir(cwd);
        cmd
    }

    /// A linked worktree of `main`, whose `.git` is a **file** holding
    /// `gitdir: <path>` rather than a directory — the shape that made the sweep
    /// inert. `git worktree add` needs a commit to branch from, so one is made.
    fn linked_worktree(main: &Path) -> (tempfile::TempDir, PathBuf) {
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .current_dir(main)
                .args(args)
                .output()
                .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        std::fs::write(main.join("seed.txt"), b"seed").expect("write seed");
        run(&["add", "seed.txt"]);
        run(&["commit", "-m", "seed"]);

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("wt");
        run(&[
            "worktree",
            "add",
            "-b",
            "wt",
            path.to_str().expect("utf8 worktree path"),
        ]);
        (dir, path)
    }

    #[test]
    fn parse_conflicted_files_porcelain_extracts_unmerged_paths() {
        let status = "\
UU src/lib.rs
AA src/new.rs
 M src/clean.rs
?? notes.txt
DU src/deleted.rs
";
        assert_eq!(
            parse_conflicted_files_porcelain(status),
            vec!["src/lib.rs", "src/new.rs", "src/deleted.rs"]
        );
    }

    /// The XY field lives in columns 1-2; a path is never read as a status. The
    /// `DD.rs` case is the one that bites — a real conflict code appearing in a
    /// filename, on a line whose actual code is a plain modification.
    #[test]
    fn parse_conflicted_files_porcelain_ignores_conflict_codes_inside_names() {
        assert!(parse_conflicted_files_porcelain("?? UUID.md\n").is_empty());
        assert!(parse_conflicted_files_porcelain("?? AAA.md\n M src/DD.rs\n").is_empty());
        assert!(parse_conflicted_files_porcelain("").is_empty());
        assert_eq!(
            parse_conflicted_files_porcelain("?? notes.txt\nUU file.rs\n"),
            vec!["file.rs".to_string()]
        );
    }

    #[test]
    fn build_conflict_assist_prompt_lists_files_and_gates_push() {
        let files = vec!["src/lib.rs".to_string(), "src/db.rs".to_string()];
        let prompt = build_conflict_assist_prompt(42, "main", &files);
        assert!(prompt.contains("PR #42"));
        assert!(prompt.contains("rebasing onto main"));
        assert!(prompt.contains("Do not push and do not merge"));
        assert!(prompt.contains("- src/lib.rs"));
        assert!(prompt.contains("- src/db.rs"));
    }

    #[test]
    fn finish_failed_operation_claims_aborted_only_when_abort_succeeds() {
        let msg = finish_failed_operation_after_abort_result(
            "rebase",
            "Rebase failed",
            "conflicts",
            Ok(()),
        );

        assert_eq!(msg, "Rebase failed (aborted): conflicts");
    }

    #[test]
    fn finish_failed_operation_surfaces_abort_failure() {
        let msg = finish_failed_operation_after_abort_result(
            "merge",
            "Merge failed",
            "conflicts",
            Err(GitError::NonZeroExit {
                code: Some(128),
                stderr: "fatal: There is no merge to abort".to_string(),
            }),
        );

        assert!(!msg.contains("(aborted)"));
        assert!(msg.contains("repo left in a conflicted state"));
        assert!(msg.contains("run git merge --abort manually"));
        assert!(msg.contains("original error: conflicts"));
    }

    #[test]
    fn test_empty_lock_kept_while_fresh_removed_when_old() {
        // 0-byte lock: kept under 5s, reclaimed at/after 5s.
        assert!(!is_index_lock_stale(0, 0));
        assert!(!is_index_lock_stale(0, 4));
        assert!(is_index_lock_stale(0, 5));
        assert!(is_index_lock_stale(0, 60));
    }

    #[test]
    fn test_nonempty_lock_kept_until_30s() {
        // Non-empty lock (index written, rename never happened): kept under 30s
        // so we never nuke a live large stash/add, reclaimed at/after 30s.
        assert!(!is_index_lock_stale(4096, 0));
        assert!(!is_index_lock_stale(4096, 29));
        assert!(is_index_lock_stale(4096, 30));
        assert!(is_index_lock_stale(4096, 120));
    }

    /// Backdate a file's mtime so the age-based staleness rule sees it as old,
    /// without sleeping in the test.
    fn age_file(path: &Path, secs: u64) {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("open for set_times");
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(secs);
        f.set_times(
            std::fs::FileTimes::new()
                .set_accessed(when)
                .set_modified(when),
        )
        .expect("set_times");
    }

    /// A lock past the age threshold that a live process still holds open must
    /// survive. Age alone cannot tell a crashed git from a slow one, and
    /// deleting the lock under a running `git add` on a large monorepo is the
    /// index-corruption path.
    #[test]
    fn stale_by_age_lock_with_a_live_owner_is_kept() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let held = std::fs::File::open(&lock).expect("open lock");
        remove_stale_index_lock(&path);
        assert!(
            lock.exists(),
            "a lock a live process holds open must not be reclaimed"
        );
        drop(held);
    }

    /// The control: nobody owns the lock, so the age rule still reclaims it.
    ///
    /// The probe is injected rather than real. Since the policy went
    /// fail-closed (#694-4fcc) a missing `lsof` answers `Unavailable`, not
    /// `Unowned`, and the lock would be KEPT — so with the real probe this test
    /// would assert the host's `lsof` rather than the rule, and fail on Windows
    /// (where no probe exists at all) for a reason that has nothing to do with
    /// what it is checking.
    #[test]
    fn stale_lock_nobody_owns_is_reclaimed() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        reclaim_stale_index_lock(&path, |_| LockOwnership::Unowned);
        assert!(!lock.exists(), "an unowned stale lock must be reclaimed");
    }

    /// Capture every `tracing` event emitted on this thread while `f` runs.
    fn capture_tracing<T>(f: impl FnOnce() -> T) -> (T, String) {
        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct Sink(Arc<Mutex<Vec<u8>>>);
        impl std::io::Write for Sink {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().expect("sink").extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Sink {
            type Writer = Sink;
            fn make_writer(&'a self) -> Self::Writer {
                self.clone()
            }
        }

        let sink = Arc::new(Mutex::new(Vec::new()));
        let subscriber = tracing_subscriber::fmt()
            .with_writer(Sink(sink.clone()))
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .finish();
        let out = tracing::subscriber::with_default(subscriber, f);
        let text = String::from_utf8(sink.lock().expect("sink").clone()).expect("utf8");
        (out, text)
    }

    /// Build the `Output` an `lsof` run would have produced.
    #[cfg(unix)]
    fn probe_output(stdout: &[u8], code: i32, stderr: &[u8]) -> std::process::Output {
        use std::os::unix::process::ExitStatusExt;
        std::process::Output {
            status: std::process::ExitStatus::from_raw(code << 8),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    /// `lsof -t` exiting non-zero with empty stdout is a real answer — "nobody
    /// has this file open" — and is the only outcome that licenses a delete.
    #[cfg(unix)]
    #[test]
    fn probe_reads_an_empty_answer_as_unowned_and_pids_as_held() {
        assert_eq!(
            classify_owner_probe(Ok(probe_output(b"", 1, b""))),
            LockOwnership::Unowned
        );
        assert_eq!(
            classify_owner_probe(Ok(probe_output(b"431\n7\n", 0, b""))),
            LockOwnership::HeldBy(vec![431, 7])
        );
    }

    /// An `lsof` that RAN and FAILED must not read as "nobody owns this lock".
    ///
    /// Measured, macOS lsof 4.91: a missing path exits **1 with empty stdout** —
    /// byte-identical to the clean "no match" — and the two are told apart only
    /// by stderr, which `-w` does NOT suppress (it silences warnings, not status
    /// errors). Without this, a probe that failed licensed a delete, which is the
    /// same fail-open the `Unknown` arm exists to make visible, only invisible.
    #[cfg(unix)]
    #[test]
    fn a_probe_that_ran_and_failed_is_unknown_not_unowned() {
        let failed = probe_output(
            b"",
            1,
            b"lsof: status error on .git/index.lock: No such file or directory\n",
        );
        match classify_owner_probe(Ok(failed)) {
            LockOwnership::Unknown(UnknownOwner::Unavailable(detail)) => {
                assert!(
                    detail.contains("status error"),
                    "the log must carry lsof's own complaint: {detail}"
                );
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    /// A complaint does not outrank an answer. If `lsof` named a holder, the
    /// lock is held whatever else it wrote to stderr — and `HeldBy` is the safe
    /// direction, so a partial failure that still found a PID must keep the lock
    /// rather than degrade to `Unknown`.
    #[cfg(unix)]
    #[test]
    fn pids_outrank_stderr_noise() {
        assert_eq!(
            classify_owner_probe(Ok(probe_output(
                b"9182\n",
                0,
                b"lsof: WARNING: something\n"
            ))),
            LockOwnership::HeldBy(vec![9182])
        );
    }

    /// No `lsof` on `PATH`: the question cannot be asked, now or ever. That is
    /// not the same fact as "nobody owns the lock", and the type must not let
    /// the caller confuse the two.
    #[cfg(unix)]
    #[test]
    fn probe_reports_unavailable_when_the_tool_is_missing() {
        let mut missing = Command::new("tuic-no-such-owner-probe");
        match classify_owner_probe(output_with_deadline(&mut missing, LOCK_OWNER_PROBE_TIMEOUT)) {
            LockOwnership::Unknown(UnknownOwner::Unavailable(detail)) => {
                assert!(
                    !detail.is_empty(),
                    "the spawn error must survive into the log"
                );
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }

    /// The measured failure. `lsof` is installed and working and merely slow —
    /// 3.7s observed on this machine against a 2s deadline. An answer existed
    /// and we ran out of patience for it; reading that as "nobody owns the
    /// lock" is how a live `git add` loses its index.
    #[cfg(unix)]
    #[test]
    fn probe_reports_deadline_exceeded_when_the_tool_is_slow() {
        let slow = Duration::from_millis(50);
        match classify_owner_probe(output_with_deadline(Command::new("sleep").arg("30"), slow)) {
            LockOwnership::Unknown(UnknownOwner::DeadlineExceeded(after)) => {
                assert_eq!(after, slow);
            }
            other => panic!("expected DeadlineExceeded, got {other:?}"),
        }
    }

    /// Fail-closed is the recorded policy (Boss, 2026-09-07, #694-4fcc): with
    /// no answer the lock is KEPT, because the age rule alone cannot tell a
    /// crashed git from a slow one and reclaiming under a live `git add`
    /// corrupts the index. The log must still name why, so the guard that is
    /// now holding the lock is visible rather than silent.
    #[test]
    fn an_unavailable_probe_keeps_the_lock_and_records_why() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let (_, logs) = capture_tracing(|| {
            reclaim_stale_index_lock(&path, |_| {
                LockOwnership::Unknown(UnknownOwner::Unavailable("no lsof on PATH".to_string()))
            })
        });

        assert!(
            lock.exists(),
            "fail-closed: an undetermined owner must not license a delete"
        );
        assert!(logs.contains("could not be determined"), "logs: {logs}");
        assert!(
            logs.contains("could not run") && logs.contains("no lsof on PATH"),
            "the log must name the tool problem: {logs}"
        );
    }

    /// Same policy, different reason — and the reason is the whole point. A
    /// probe that timed out means a live owner may well exist; a probe that is
    /// absent means nothing can ever be known. One is worth chasing, the other
    /// is not.
    #[test]
    fn a_slow_probe_keeps_the_lock_and_names_the_deadline() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let (_, logs) = capture_tracing(|| {
            reclaim_stale_index_lock(&path, |_| {
                LockOwnership::Unknown(UnknownOwner::DeadlineExceeded(Duration::from_secs(2)))
            })
        });

        assert!(
            lock.exists(),
            "fail-closed: a probe that timed out must not license a delete"
        );
        assert!(logs.contains("could not be determined"), "logs: {logs}");
        assert!(
            logs.contains("outlived its 2s deadline"),
            "the log must name the deadline, not just the failure: {logs}"
        );
    }

    /// The escape hatch, and the reason fail-closed does not strand a repo.
    /// On a host with no `lsof` the probe is permanently unavailable, so
    /// without this branch a crash-orphaned lock would outlive the process that
    /// left it and no future call could ever clear it. Age at a threshold no
    /// live git can reach adjudicates what the probe cannot.
    #[test]
    fn an_unadjudicated_lock_is_reclaimed_once_no_live_git_could_still_hold_it() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, UNADJUDICATED_LOCK_STALE_SECS + 60);

        let (_, logs) = capture_tracing(|| {
            reclaim_stale_index_lock(&path, |_| {
                LockOwnership::Unknown(UnknownOwner::Unavailable("no lsof on PATH".to_string()))
            })
        });

        assert!(
            !lock.exists(),
            "past the escape-hatch age an unadjudicated lock must be reclaimed, \
             or a host without lsof could never clear one"
        );
        assert!(
            logs.contains("on age alone") && logs.contains("no lsof on PATH"),
            "the log must say it reclaimed without evidence, and why: {logs}"
        );
    }

    /// The hatch must not swallow a real answer. A live owner outranks age at
    /// every age — otherwise a slow `git add` on a huge monorepo would lose its
    /// index to a rule that never even asked.
    #[test]
    fn the_escape_hatch_never_overrides_a_named_owner() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, UNADJUDICATED_LOCK_STALE_SECS * 24);

        reclaim_stale_index_lock(&path, |_| LockOwnership::HeldBy(vec![4321]));

        assert!(
            lock.exists(),
            "evidence of a holder outranks age no matter how old the lock is"
        );
    }

    /// A named owner is kept, and the log says who has it.
    #[test]
    fn a_named_owner_keeps_the_lock() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        let (_, logs) = capture_tracing(|| {
            reclaim_stale_index_lock(&path, |_| LockOwnership::HeldBy(vec![4321]))
        });

        assert!(
            lock.exists(),
            "a lock with a live owner must not be reclaimed"
        );
        assert!(
            logs.contains("4321"),
            "the log must name the holder: {logs}"
        );
    }

    /// In a linked worktree `.git` is a **file** holding `gitdir: <path>`, so
    /// `cwd/.git/index.lock` traverses a file and `metadata()` fails — the whole
    /// sweep returned before ever looking at a lock. TUIC runs most of its work
    /// in worktrees, so the guard was inert exactly where it was needed most.
    #[test]
    fn a_stale_lock_in_a_linked_worktree_is_reclaimed() {
        let (_dir, main) = setup_test_repo();
        let (_wt_dir, worktree) = linked_worktree(&main);

        let git_dir = crate::git::resolve_git_dir(&worktree).expect("worktree gitdir resolves");
        assert!(
            !worktree.join(".git").is_dir(),
            "a linked worktree must have .git as a file for this test to mean anything"
        );

        let lock = git_dir.join("index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        reclaim_stale_index_lock(&worktree, |_| LockOwnership::Unowned);

        assert!(
            !lock.exists(),
            "the sweep must follow the gitdir pointer, not assume cwd/.git is a directory"
        );
    }

    /// The mirror of the above: a *live* lock in a worktree must still be kept,
    /// so following the pointer widens the guard's reach without weakening it.
    #[test]
    fn a_held_lock_in_a_linked_worktree_is_kept() {
        let (_dir, main) = setup_test_repo();
        let (_wt_dir, worktree) = linked_worktree(&main);

        let git_dir = crate::git::resolve_git_dir(&worktree).expect("worktree gitdir resolves");
        let lock = git_dir.join("index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");
        age_file(&lock, 120);

        reclaim_stale_index_lock(&worktree, |_| LockOwnership::HeldBy(vec![4321]));

        assert!(lock.exists(), "a worktree lock with a live owner is kept");
    }

    /// The probe stays off the hot path: only a lock the age rule has already
    /// condemned is worth a fork.
    #[test]
    fn a_fresh_lock_is_never_probed() {
        let (_dir, path) = setup_test_repo();
        let lock = path.join(".git/index.lock");
        std::fs::write(&lock, b"index payload").expect("write lock");

        reclaim_stale_index_lock(&path, |_| panic!("a fresh lock must not be probed"));
        assert!(lock.exists(), "a fresh lock is kept without asking anyone");
    }

    /// Catches: Windows -c operands being counted as subcommands instead of status/cherry.
    #[test]
    fn command_counts_skip_windows_global_config_operands() {
        let (_dir, path) = setup_test_repo();
        take_command_counts();
        // Exercise the Windows-injected argv on every host, including Linux rb.
        git_cmd(&path)
            .args(["-c", "core.longpaths=true", "status", "--porcelain"])
            .run()
            .unwrap();
        git_cmd(&path)
            .args([
                "-c",
                "core.longpaths=true",
                "-c",
                "core.autocrlf=false",
                "status",
                "--porcelain",
            ])
            .run()
            .unwrap();
        git_cmd(&path)
            .args(["status", "--porcelain"])
            .run()
            .unwrap();
        assert_eq!(
            take_command_counts(),
            std::collections::BTreeMap::from([("status".into(), 3),])
        );
    }

    #[test]
    fn test_run_non_zero_exit() {
        let (_dir, path) = setup_test_repo();
        // Asking for log in a repo with no commits → non-zero exit
        let result = git_cmd(&path).args(["log", "--oneline"]).run();
        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err {
            GitError::NonZeroExit { code, stderr: _ } => {
                assert!(code.is_some());
            }
            _ => panic!("Expected NonZeroExit, got {err:?}"),
        }
        // Display impl should produce a readable message
        let msg = err.to_string();
        assert!(msg.contains("git exited with code"));
    }

    #[test]
    fn test_run_spawn_failed() {
        let (_dir, path) = setup_test_repo();
        // Use a non-existent binary to trigger spawn failure
        let mut cmd = Command::new("/nonexistent/git-binary-that-does-not-exist");
        cmd.current_dir(&path);
        cmd.args(["status"]);
        let gc = GitCmd {
            cmd,
            cwd: path.clone(),
            timeout: None,
        };
        let result = gc.run();
        assert!(result.is_err());
        match result.unwrap_err() {
            GitError::SpawnFailed(_) => {} // expected
            other => panic!("Expected SpawnFailed, got {other:?}"),
        }
    }

    #[test]
    fn test_run_silent_returns_none_on_error() {
        let (_dir, path) = setup_test_repo();
        // log in empty repo → non-zero → None
        let result = git_cmd(&path).args(["log", "--oneline"]).run_silent();
        assert!(result.is_none());
    }

    #[test]
    fn test_run_silent_returns_some_on_success() {
        let (_dir, path) = setup_test_repo();
        let result = git_cmd(&path).args(["status", "--porcelain"]).run_silent();
        assert!(result.is_some());
    }

    #[test]
    fn test_run_raw_returns_output_on_failure() {
        let (_dir, path) = setup_test_repo();
        // log in empty repo → non-zero but raw still returns Ok(Output)
        let result = git_cmd(&path).args(["log", "--oneline"]).run_raw();
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.status.success());
    }

    /// A command that hangs must be killed at the deadline, not waited on.
    #[test]
    fn run_kills_a_command_that_outlives_its_timeout() {
        let (_dir, path) = setup_test_repo();
        let cmd = sleeping_command(&path);
        let gc = GitCmd {
            cmd,
            cwd: path.clone(),
            timeout: None,
        }
        .timeout(Duration::from_millis(200));

        let started = Instant::now();
        let err = gc.run().expect_err("must time out");
        let waited = started.elapsed();

        assert!(
            matches!(err, GitError::TimedOut { .. }),
            "expected TimedOut, got {err:?}"
        );
        assert!(
            waited < Duration::from_secs(5),
            "must not wait for the child; waited {waited:?}"
        );
    }

    /// run_silent must swallow a timeout the same way it swallows a non-zero exit.
    #[test]
    fn run_silent_returns_none_on_timeout() {
        let (_dir, path) = setup_test_repo();
        let cmd = sleeping_command(&path);
        let gc = GitCmd {
            cmd,
            cwd: path.clone(),
            timeout: None,
        }
        .timeout(Duration::from_millis(200));

        assert!(gc.run_silent().is_none());
    }

    /// The deadline must not truncate a command that finishes inside it.
    #[test]
    fn timeout_does_not_fire_for_a_command_that_finishes_in_time() {
        let (_dir, path) = setup_test_repo();
        std::fs::write(path.join("untracked.txt"), "u\n").expect("write");
        let out = git_cmd(&path)
            .timeout(Duration::from_secs(30))
            .args(["status", "--porcelain"])
            .run()
            .expect("status must succeed inside the deadline");
        assert!(
            out.stdout.contains("untracked.txt"),
            "stdout must survive the piped path: {:?}",
            out.stdout
        );
    }

    #[test]
    fn timed_out_error_names_the_deadline() {
        let err = GitError::TimedOut {
            after: Duration::from_millis(2500),
        };
        assert_eq!(err.to_string(), "git timed out after 2.5s and was killed");
    }

    #[test]
    fn test_git_error_display() {
        let err = GitError::NonZeroExit {
            code: Some(128),
            stderr: "fatal: not a git repository".to_string(),
        };
        assert_eq!(
            err.to_string(),
            "git exited with code 128: fatal: not a git repository"
        );

        let err_empty = GitError::NonZeroExit {
            code: Some(1),
            stderr: String::new(),
        };
        assert_eq!(err_empty.to_string(), "git exited with code 1");

        let err_signal = GitError::NonZeroExit {
            code: None,
            stderr: "killed".to_string(),
        };
        assert_eq!(
            err_signal.to_string(),
            "git exited with code signal: killed"
        );
    }

    #[test]
    fn test_git_error_into_string() {
        let err = GitError::NonZeroExit {
            code: Some(1),
            stderr: "oops".to_string(),
        };
        let s: String = err.into();
        assert!(s.contains("oops"));
    }

    #[test]
    fn test_env_is_passed() {
        let (_dir, path) = setup_test_repo();
        // `git status` ignores the variable, so the old is_ok() check passed even when
        // env() dropped it. `git var` echoes the author identity built from it.
        let out = git_cmd(&path)
            .env("GIT_AUTHOR_NAME", "TestBot")
            .args(["var", "GIT_AUTHOR_IDENT"])
            .run()
            .expect("git var");
        assert!(out.stdout.starts_with("TestBot <"), "{:?}", out.stdout);
    }
}
