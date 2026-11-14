//! External story backend: a child process that speaks the story-backend
//! protocol (`docs/backend/story-backend-protocol.md`) over stdio.
//!
//! `ExternalBackend` owns the process. It starts the adapter on the first call,
//! restarts it with backoff after a failure, and binds every change cursor to
//! the process generation, so a cursor from before a restart is answered with
//! `RescanRequired`. Every transport failure is `AdapterError::Unavailable`:
//! there is no fallback to another store.

pub(crate) mod binding;
#[cfg(test)]
mod execution_tests;
mod port;
#[cfg(test)]
mod port_tests;
pub(crate) mod selection;
#[cfg(test)]
mod tests;

pub(crate) use port::{ExternalStories, external_capabilities};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub(crate) const PROTOCOL_VERSION: u32 = 1;
/// Prefix of every error that means "this project's story backend cannot be used".
pub(crate) const UNAVAILABLE: &str = "story backend unavailable";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const BACKOFF_FIRST: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

const CONFLICT: i64 = -32001;
const REFUSED: i64 = -32002;
const UNSUPPORTED: i64 = -32003;
const NOT_FOUND: i64 = -32004;
const METHOD_NOT_FOUND: i64 = -32601;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdapterCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct AdapterIdentity {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AdapterInfo {
    pub protocol: u32,
    pub adapter: AdapterIdentity,
    #[serde(default)]
    pub capabilities: BTreeMap<String, bool>,
    /// Resolved against the project root when the adapter starts.
    #[serde(default)]
    pub watch_paths: Vec<PathBuf>,
}

impl AdapterInfo {
    /// A capability the adapter did not declare is unsupported.
    pub fn supports(&self, name: &str) -> bool {
        self.capabilities.get(name).copied().unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AdapterError {
    /// The adapter is not running, did not start, exited, or did not answer in time.
    Unavailable(String),
    /// The record changed after the caller read its version.
    Conflict,
    /// A backend rule refused the write.
    Refused(String),
    Unsupported(String),
    NotFound(String),
    /// Any other error answer, or an answer that does not match the protocol.
    Failed(String),
}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(f, "{UNAVAILABLE}: {reason}"),
            Self::Conflict => write!(f, "story changed since it was read"),
            Self::Refused(reason) => write!(f, "story backend refused: {reason}"),
            Self::Unsupported(what) => write!(f, "story backend does not support {what}"),
            Self::NotFound(what) => write!(f, "story backend has no record {what}"),
            Self::Failed(reason) => write!(f, "story backend error: {reason}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ExternalPlan {
    pub id: String,
    pub version: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ExternalCriterion {
    pub text: String,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ExternalNote {
    pub at: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ExternalStory {
    pub id: String,
    pub version: String,
    pub plan_id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub priority: Option<u8>,
    #[serde(default)]
    pub problem: String,
    #[serde(default)]
    pub criteria: Vec<ExternalCriterion>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub notes: Vec<ExternalNote>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct NewExternalStory {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    pub title: String,
    pub problem: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    pub criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ApplyOp {
    SetStatus {
        status: String,
    },
    CheckCriterion {
        index: usize,
    },
    AddCriterion {
        text: String,
    },
    AddDependency {
        id: String,
    },
    RemoveDependency {
        id: String,
    },
    SetFields {
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        problem: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        priority: Option<u8>,
    },
}

/// An adapter cursor, valid only for the process generation that issued it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeCursor {
    generation: u64,
    token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Changes {
    Changed {
        ids: Vec<String>,
        cursor: ChangeCursor,
    },
    /// The caller must reread everything, then continue from `cursor`.
    RescanRequired { cursor: ChangeCursor },
}

#[derive(Deserialize)]
struct RawChanges {
    #[serde(default)]
    rescan_required: bool,
    #[serde(default)]
    changed: Vec<String>,
    cursor: String,
}

#[derive(Debug)]
struct RpcError {
    code: i64,
    message: String,
}

type Reply = Result<Value, RpcError>;
/// `None` after the adapter's stdout closed: no answer can arrive any more.
type Pending = Arc<Mutex<Option<HashMap<u64, Sender<Reply>>>>>;

enum CallError {
    /// The adapter answered with an error.
    Rpc(RpcError),
    /// The process is gone or did not answer in time.
    Lost(String),
}

struct Connection {
    child: Child,
    stdin: ChildStdin,
    pending: Pending,
    next_id: u64,
}

impl Connection {
    fn spawn(command: &AdapterCommand, project_root: &Path) -> Result<Self, String> {
        let mut child = Command::new(&command.program)
            .args(&command.args)
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("cannot start {}: {error}", command.program))?;
        // Owning the child first means an early return below still kills it.
        let mut conn = Self {
            stdin: child.stdin.take().ok_or("adapter stdin missing")?,
            pending: Arc::new(Mutex::new(Some(HashMap::new()))),
            next_id: 0,
            child,
        };
        let stdout = conn.child.stdout.take().ok_or("adapter stdout missing")?;
        let stderr = conn.child.stderr.take().ok_or("adapter stderr missing")?;
        let pending = Arc::clone(&conn.pending);
        std::thread::Builder::new()
            .name("story-adapter-out".into())
            .spawn(move || read_replies(BufReader::new(stdout), &pending))
            .map_err(|error| error.to_string())?;
        std::thread::Builder::new()
            .name("story-adapter-err".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    tracing::debug!(target: "story_adapter", "{line}");
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(conn)
    }

    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value, CallError> {
        self.next_id += 1;
        let id = self.next_id;
        let (tx, rx) = mpsc::channel();
        match lock(&self.pending).as_mut() {
            Some(map) => map.insert(id, tx),
            None => return Err(CallError::Lost("adapter exited".into())),
        };
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{line}")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| CallError::Lost(format!("cannot write to adapter: {error}")))?;
        match rx.recv_timeout(timeout) {
            Ok(reply) => reply.map_err(CallError::Rpc),
            Err(RecvTimeoutError::Timeout) => Err(CallError::Lost(format!(
                "no answer to {method} within {} ms",
                timeout.as_millis()
            ))),
            Err(RecvTimeoutError::Disconnected) => Err(CallError::Lost("adapter exited".into())),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn read_replies(stdout: impl BufRead, pending: &Pending) {
    for line in stdout.lines().map_while(Result::ok) {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            tracing::warn!(target: "story_adapter", "ignoring non-JSON adapter line");
            continue;
        };
        // Notifications (no id) such as `changed` are hints only; polling
        // `changes` stays correct without them.
        let Some(id) = message.get("id").and_then(Value::as_u64) else {
            continue;
        };
        let reply = match message.get("error") {
            Some(error) => Err(RpcError {
                code: error.get("code").and_then(Value::as_i64).unwrap_or(0),
                message: error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }),
            None => Ok(message.get("result").cloned().unwrap_or(Value::Null)),
        };
        let sender = lock(pending).as_mut().and_then(|map| map.remove(&id));
        if let Some(sender) = sender {
            let _ = sender.send(reply);
        }
    }
    // Dropping the senders wakes every waiting call with `Disconnected`.
    lock(pending).take();
}

fn map_rpc(error: RpcError) -> AdapterError {
    match error.code {
        CONFLICT => AdapterError::Conflict,
        REFUSED => AdapterError::Refused(error.message),
        UNSUPPORTED | METHOD_NOT_FOUND => AdapterError::Unsupported(error.message),
        NOT_FOUND => AdapterError::NotFound(error.message),
        _ => AdapterError::Failed(format!("{} ({})", error.message, error.code)),
    }
}

/// The first restart is immediate; later consecutive failures wait 1 s, 2 s, … 60 s.
fn backoff(failures: u32) -> Duration {
    if failures <= 1 {
        return Duration::ZERO;
    }
    BACKOFF_FIRST
        .saturating_mul(1 << (failures - 2).min(6))
        .min(BACKOFF_MAX)
}

#[derive(Default)]
struct State {
    conn: Option<Connection>,
    info: Option<AdapterInfo>,
    generation: u64,
    failures: u32,
    retry_at: Option<Instant>,
}

pub(crate) struct ExternalBackend {
    command: AdapterCommand,
    project_root: PathBuf,
    timeout: Duration,
    state: Mutex<State>,
}

impl ExternalBackend {
    pub fn new(command: AdapterCommand, project_root: impl Into<PathBuf>) -> Self {
        Self {
            command,
            project_root: project_root.into(),
            timeout: DEFAULT_TIMEOUT,
            state: Mutex::new(State::default()),
        }
    }

    #[cfg(test)]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Starts the adapter when it is not running and returns its `initialize` answer.
    pub fn info(&self) -> Result<AdapterInfo, AdapterError> {
        let mut state = lock(&self.state);
        self.ensure(&mut state)?;
        Ok(state.info.clone().expect("info is set while connected"))
    }

    pub fn list_plans(&self) -> Result<Vec<ExternalPlan>, AdapterError> {
        #[derive(Deserialize)]
        struct Plans {
            plans: Vec<ExternalPlan>,
        }
        Ok(self.typed::<Plans>("list_plans", json!({}))?.plans)
    }

    pub fn list_stories(&self, plan_id: Option<&str>) -> Result<Vec<ExternalStory>, AdapterError> {
        #[derive(Deserialize)]
        struct Stories {
            stories: Vec<ExternalStory>,
        }
        let params = match plan_id {
            Some(plan_id) => json!({ "plan_id": plan_id }),
            None => json!({}),
        };
        Ok(self.typed::<Stories>("list_stories", params)?.stories)
    }

    pub fn get_story(&self, id: &str) -> Result<ExternalStory, AdapterError> {
        self.typed("get_story", json!({ "id": id }))
    }

    pub fn create_story(&self, fields: &NewExternalStory) -> Result<ExternalStory, AdapterError> {
        self.typed("create_story", json!({ "fields": fields }))
    }

    pub fn apply(
        &self,
        id: &str,
        expected_version: &str,
        op: &ApplyOp,
    ) -> Result<ExternalStory, AdapterError> {
        self.typed(
            "apply",
            json!({ "id": id, "expected_version": expected_version, "op": op }),
        )
    }

    pub fn append_note(&self, id: &str, text: &str) -> Result<ExternalStory, AdapterError> {
        self.typed("append_note", json!({ "id": id, "text": text }))
    }

    /// `None`, or a cursor from an earlier adapter process, yields `RescanRequired`.
    pub fn changes(&self, cursor: Option<&ChangeCursor>) -> Result<Changes, AdapterError> {
        let mut state = lock(&self.state);
        self.ensure(&mut state)?;
        let generation = state.generation;
        let token = cursor
            .filter(|cursor| cursor.generation == generation)
            .map(|cursor| cursor.token.clone());
        let stale = token.is_none();
        let raw: RawChanges =
            decode(self.call(&mut state, "changes", json!({ "cursor": token }))?)?;
        let cursor = ChangeCursor {
            generation,
            token: raw.cursor,
        };
        Ok(if stale || raw.rescan_required {
            Changes::RescanRequired { cursor }
        } else {
            Changes::Changed {
                ids: raw.changed,
                cursor,
            }
        })
    }

    /// The watch paths of the running adapter; `None` when no process runs.
    pub fn running_watch_paths(&self) -> Option<Vec<PathBuf>> {
        let state = lock(&self.state);
        state.conn.as_ref()?;
        state.info.as_ref().map(|info| info.watch_paths.clone())
    }

    /// Asks the adapter to stop, then ends the process. A later call starts it again.
    pub fn shutdown(&self) {
        let mut state = lock(&self.state);
        if let Some(conn) = state.conn.as_mut() {
            let _ = conn.call("shutdown", json!({}), SHUTDOWN_TIMEOUT);
        }
        state.conn = None;
        state.info = None;
    }

    fn typed<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, AdapterError> {
        let mut state = lock(&self.state);
        self.ensure(&mut state)?;
        decode(self.call(&mut state, method, params)?)
    }

    fn call(&self, state: &mut State, method: &str, params: Value) -> Result<Value, AdapterError> {
        let conn = state.conn.as_mut().expect("connected after ensure");
        match conn.call(method, params, self.timeout) {
            Ok(value) => {
                state.failures = 0;
                Ok(value)
            }
            Err(CallError::Rpc(error)) => {
                state.failures = 0;
                Err(map_rpc(error))
            }
            Err(CallError::Lost(reason)) => Err(self.fail(state, reason)),
        }
    }

    fn ensure(&self, state: &mut State) -> Result<(), AdapterError> {
        if state.conn.is_some() {
            return Ok(());
        }
        if let Some(retry_at) = state.retry_at {
            let now = Instant::now();
            if now < retry_at {
                return Err(AdapterError::Unavailable(format!(
                    "restart in {} ms after {} failures",
                    (retry_at - now).as_millis(),
                    state.failures
                )));
            }
        }
        state.generation += 1;
        let mut conn = Connection::spawn(&self.command, &self.project_root)
            .map_err(|reason| self.fail(state, reason))?;
        let params = json!({ "protocol": PROTOCOL_VERSION, "project_root": self.project_root });
        let answer = match conn.call("initialize", params, self.timeout) {
            Ok(answer) => answer,
            Err(CallError::Lost(reason)) => return Err(self.fail(state, reason)),
            Err(CallError::Rpc(error)) => {
                return Err(self.fail(state, format!("initialize refused: {}", error.message)));
            }
        };
        let mut info: AdapterInfo = serde_json::from_value(answer)
            .map_err(|error| self.fail(state, format!("bad initialize answer: {error}")))?;
        if info.protocol != PROTOCOL_VERSION {
            let reason = format!(
                "adapter speaks protocol {}, TUIC speaks {PROTOCOL_VERSION}",
                info.protocol
            );
            return Err(self.fail(state, reason));
        }
        for path in &mut info.watch_paths {
            *path = self.project_root.join(&*path);
        }
        state.conn = Some(conn);
        state.info = Some(info);
        Ok(())
    }

    fn fail(&self, state: &mut State, reason: String) -> AdapterError {
        state.conn = None;
        state.info = None;
        state.failures += 1;
        state.retry_at = Some(Instant::now() + backoff(state.failures));
        tracing::warn!(
            target: "story_adapter",
            "story adapter {} unavailable (failure {}): {reason}",
            self.command.program,
            state.failures
        );
        AdapterError::Unavailable(reason)
    }

    /// Kills the adapter process without telling the backend, like a crash.
    #[cfg(test)]
    fn kill_adapter_for_test(&self) {
        if let Some(conn) = lock(&self.state).conn.as_mut() {
            let _ = conn.child.kill();
            let _ = conn.child.wait();
        }
    }
}

fn decode<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, AdapterError> {
    serde_json::from_value(value)
        .map_err(|error| AdapterError::Failed(format!("bad answer: {error}")))
}
