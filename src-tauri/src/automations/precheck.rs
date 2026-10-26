//! Bounded pre-dispatch checks. Run persistence belongs to the dispatcher.
use super::model::Precheck;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrecheckOutcome {
    NotConfigured,
    Bypassed,
    Executed(PrecheckResult),
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PrecheckResult {
    pub termination: Termination,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub duration_ms: u64,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Termination {
    Exited(Option<i32>),
    TimedOut,
    SpawnError(String),
    ProcessError(String),
}
impl PrecheckOutcome {
    /// False requires the dispatcher to persist `skipped_precheck`, never launch.
    pub fn proceeds(&self) -> bool {
        matches!(
            self,
            Self::NotConfigured
                | Self::Bypassed
                | Self::Executed(PrecheckResult {
                    termination: Termination::Exited(Some(0)),
                    ..
                })
        )
    }
}
/// Manual runs record a bypass even when no precheck is configured. Neither
/// branch launches a shell; callers persist this outcome with the run ledger.
pub async fn run_precheck(
    workspace: &Path,
    precheck: Option<&Precheck>,
    manual: bool,
) -> PrecheckOutcome {
    if manual {
        return PrecheckOutcome::Bypassed;
    }
    let Some(precheck) = precheck else {
        return PrecheckOutcome::NotConfigured;
    };
    let workspace = workspace.to_owned();
    let precheck = precheck.clone();
    let started = Instant::now();
    let result = tokio::task::spawn_blocking(move || execute(&workspace, &precheck)).await;
    PrecheckOutcome::Executed(match result {
        Ok(result) => result,
        Err(error) => empty_result(Termination::ProcessError(error.to_string()), started),
    })
}

pub const MAX_STREAM_BYTES: usize = 256 * 1024;

use crate::agent::ScreenProbeTree;
use parking_lot::Mutex;
use std::io::Read;
use std::process::{Child, Stdio};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

struct OwnedProcess {
    child: Child,
    tree: Option<ScreenProbeTree>,
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if let Some(tree) = self.tree.take() {
            tree.terminate(self.child.id());
        }
        if let Err(error) = self.child.kill() {
            // Already-exited children are normal; wait still reaps the child.
            if error.kind() != std::io::ErrorKind::InvalidInput {
                tracing::debug!(%error, "Precheck child kill returned an error");
            }
        }
        if let Err(error) = self.child.wait() {
            tracing::warn!(%error, "Failed to reap precheck child");
        }
    }
}

#[derive(Default)]
struct StreamCapture {
    bytes: Vec<u8>,
    truncated: bool,
}
impl StreamCapture {
    fn append(&mut self, bytes: &[u8]) {
        let remaining = MAX_STREAM_BYTES - self.bytes.len();
        self.bytes
            .extend_from_slice(&bytes[..bytes.len().min(remaining)]);
        self.truncated |= bytes.len() > remaining;
    }
    fn text(&self) -> (String, bool) {
        let mut text = String::from_utf8_lossy(&self.bytes).into_owned();
        let mut end = text.len().min(MAX_STREAM_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let truncated = self.truncated || end < text.len();
        text.truncate(end);
        (text, truncated)
    }
}

// Keep draining after the cap: stopping would fill a pipe and make a successful
// check time out. Channels bound cleanup even if a script detaches a pipe owner.
fn drain(
    mut stream: impl Read + Send + 'static,
) -> (
    Arc<Mutex<StreamCapture>>,
    mpsc::Receiver<std::io::Result<()>>,
) {
    let capture = Arc::new(Mutex::new(StreamCapture::default()));
    let target = Arc::clone(&capture);
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (|| {
            let mut buffer = [0; 8192];
            loop {
                let count = stream.read(&mut buffer)?;
                if count == 0 {
                    return Ok(());
                }
                target.lock().append(&buffer[..count]);
            }
        })();
        if tx.send(result).is_err() {
            tracing::debug!("Precheck stream owner already returned");
        }
    });
    (capture, rx)
}

fn empty_result(termination: Termination, started: Instant) -> PrecheckResult {
    PrecheckResult {
        termination,
        stdout: String::new(),
        stderr: String::new(),
        stdout_truncated: false,
        stderr_truncated: false,
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    }
}

#[cfg(test)]
pub(crate) static PRECHECK_INVOCATIONS: std::sync::LazyLock<
    dashmap::DashMap<std::path::PathBuf, String>,
> = std::sync::LazyLock::new(dashmap::DashMap::new);

fn execute(workspace: &Path, precheck: &Precheck) -> PrecheckResult {
    let started = Instant::now();
    // Persisted definitions validate these bounds; retain the check at this
    // execution boundary for direct callers as well.
    if precheck.command.trim().is_empty() || precheck.timeout_secs == 0 {
        return empty_result(
            Termination::ProcessError("Precheck requires a command and positive timeout".into()),
            started,
        );
    }
    let mut command = crate::smart_prompt::clean_shell_command(&precheck.command);
    command
        .current_dir(workspace)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let tree = match ScreenProbeTree::prepare(&mut command) {
        Ok(tree) => tree,
        Err(error) => return empty_result(Termination::SpawnError(error.to_string()), started),
    };
    #[cfg(test)]
    PRECHECK_INVOCATIONS.insert(
        workspace.to_path_buf(),
        format!(
            "argv={:?} {:?}; cwd={:?}",
            command.get_program(),
            command.get_args().collect::<Vec<_>>(),
            command.get_current_dir()
        ),
    );
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => return empty_result(Termination::SpawnError(error.to_string()), started),
    };
    let mut process = OwnedProcess {
        child,
        tree: Some(tree),
    };
    if let Err(error) = process
        .tree
        .as_ref()
        .expect("tree just assigned")
        .assign(&process.child)
    {
        return empty_result(Termination::SpawnError(error.to_string()), started);
    }
    let (stdout, out_done) = drain(process.child.stdout.take().expect("stdout piped"));
    let (stderr, err_done) = drain(process.child.stderr.take().expect("stderr piped"));
    // Arm the behavior deadline only after process setup has succeeded.
    let executing = Instant::now();
    let budget = Duration::from_secs(precheck.timeout_secs);
    let termination = loop {
        match process.child.try_wait() {
            Ok(Some(status)) => break Termination::Exited(status.code()),
            Ok(None) if executing.elapsed() >= budget => break Termination::TimedOut,
            Ok(None) => std::thread::sleep(
                Duration::from_millis(10).min(budget.saturating_sub(executing.elapsed())),
            ),
            Err(error) => break Termination::ProcessError(error.to_string()),
        }
    };
    // Kill only our isolated group/job, including children retaining pipe ends.
    drop(process);
    let mut result = empty_result(termination, started);
    let cleanup = Instant::now();
    for done in [out_done, err_done] {
        match done.recv_timeout(Duration::from_secs(2).saturating_sub(cleanup.elapsed())) {
            Ok(Ok(())) => {}
            error => {
                // A timeout remains a timeout even when a detached pipe owner
                // prevents full capture. An otherwise successful check cannot
                // silently admit dispatch with lost output evidence.
                if !matches!(result.termination, Termination::TimedOut) {
                    result.termination = Termination::ProcessError(format!(
                        "Precheck stream capture failed: {error:?}"
                    ));
                }
            }
        }
    }
    (result.stdout, result.stdout_truncated) = stdout.lock().text();
    (result.stderr, result.stderr_truncated) = stderr.lock().text();
    result.duration_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    result
}

#[cfg(test)]
#[path = "precheck_tests.rs"]
mod tests;
