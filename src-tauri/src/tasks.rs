//! Task registry backing long-running MCP orchestration.
//!
//! `agent spawn` + `agent wait` is a hand-rolled long-running task capped at
//! `WAIT_MAX_MS` (300s): an orchestrator cannot supervise a peer that works
//! longer, and a client that drops mid-wait loses the outcome entirely. A task is
//! a handle it can poll at its own pace instead, and the outcome is recorded when
//! the agent finishes whether or not anyone was listening.
//!
//! **Process-lifetime, deliberately not persisted.** The case this exists for is a
//! *client* restart, which TUIC's own process outlives. Persisting to disk would
//! only cover a TUIC restart — and that tears down every PTY, so a recovered
//! `working` task would describe an agent that no longer exists. See the ADR in
//! `plans/mcp-2026-07-28-dual-era.md` (Phase A).
//!
//! The status vocabulary is the MCP `2026-07-28` Tasks vocabulary from day one
//! (`working|input_required|completed|failed|cancelled`) so the standard
//! `tasks/*` front door becomes a serialization change, not a semantic remap.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::fmt;

/// How long a task stays readable after creation. Completed tasks are kept for
/// the same window so a reconnecting client can still collect the outcome.
pub(crate) const TASK_TTL_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskStatus {
    Working,
    InputRequired,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    /// Terminal states are immutable — the spec forbids transitioning out of
    /// them, and the `tasks/*` front door relies on that guarantee.
    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }

    /// Wire spelling. Kept in lockstep with the `serde` derive by
    /// `status_as_str_matches_the_wire`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::InputRequired => "input_required",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// What a task is tracking. One variant today; `wait` and the standard `tasks/*`
/// surface add their own as they gain callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskKind {
    AgentSpawn,
}

/// Payload attached to a status transition. Every field is optional, and an
/// omitted one leaves the stored value alone — a progress message must not wipe
/// an earlier partial result.
#[derive(Debug, Clone, Default)]
pub(crate) struct TaskUpdate {
    pub status_message: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TaskRecord {
    pub task_id: String,
    pub kind: TaskKind,
    /// The identity that created the task — the ownership check for `task
    /// get`/`cancel` compares against this.
    pub owner: String,
    pub session_id: Option<String>,
    pub status: TaskStatus,
    pub status_message: Option<String>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TaskError {
    /// No such task, or it was already reaped.
    NotFound,
    /// The task already reached `status`, which is terminal and immutable.
    Terminal(TaskStatus),
}

impl fmt::Display for TaskError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskError::NotFound => write!(f, "unknown or expired task_id"),
            TaskError::Terminal(s) => write!(
                f,
                "task already finished as '{}' and is immutable",
                s.as_str()
            ),
        }
    }
}

impl std::error::Error for TaskError {}

#[derive(Default)]
pub(crate) struct TaskRegistry {
    tasks: DashMap<String, TaskRecord>,
}

impl TaskRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Create a `working` task and return its handle.
    pub(crate) fn create(&self, kind: TaskKind, owner: &str, session_id: Option<&str>) -> String {
        let task_id = uuid::Uuid::new_v4().to_string();
        let now = unix_now();
        self.tasks.insert(
            task_id.clone(),
            TaskRecord {
                task_id: task_id.clone(),
                kind,
                owner: owner.to_string(),
                session_id: session_id.map(str::to_string),
                status: TaskStatus::Working,
                status_message: None,
                result: None,
                error: None,
                created_at: now,
                updated_at: now,
                expires_at: now + TASK_TTL_SECS,
            },
        );
        task_id
    }

    pub(crate) fn get(&self, task_id: &str) -> Option<TaskRecord> {
        self.tasks.get(task_id).map(|e| e.value().clone())
    }

    /// Move a task to `status`. Rejects any transition out of a terminal state.
    ///
    /// The check and the write happen under the same per-entry lock, so a cancel
    /// racing a completion cannot both win.
    pub(crate) fn set_status(
        &self,
        task_id: &str,
        status: TaskStatus,
        update: TaskUpdate,
    ) -> Result<(), TaskError> {
        let mut entry = self.tasks.get_mut(task_id).ok_or(TaskError::NotFound)?;
        let rec = entry.value_mut();
        if rec.status.is_terminal() {
            return Err(TaskError::Terminal(rec.status));
        }

        rec.status = status;
        if let Some(message) = update.status_message {
            rec.status_message = Some(message);
        }
        if let Some(result) = update.result {
            rec.result = Some(result);
        }
        if let Some(error) = update.error {
            rec.error = Some(error);
        }
        rec.updated_at = unix_now();
        Ok(())
    }

    /// Terminate a task as `cancelled`.
    pub(crate) fn cancel(&self, task_id: &str) -> Result<(), TaskError> {
        self.set_status(task_id, TaskStatus::Cancelled, TaskUpdate::default())
    }

    /// Handles of still-live tasks tracking `session_id`. Terminal tasks are
    /// excluded: a task the orchestrator already cancelled must not be revisited
    /// when the session it was watching finally exits.
    pub(crate) fn live_ids_for_session(&self, session_id: &str) -> Vec<String> {
        self.tasks
            .iter()
            .filter(|e| {
                let rec = e.value();
                rec.session_id.as_deref() == Some(session_id) && !rec.status.is_terminal()
            })
            .map(|e| e.key().clone())
            .collect()
    }

    /// Test-only: total tasks held, live or terminal.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Drop tasks past their TTL. Returns the number removed.
    pub(crate) fn reap_expired(&self) -> usize {
        let now = unix_now();
        let before = self.tasks.len();
        self.tasks.retain(|_, rec| rec.expires_at > now);
        before - self.tasks.len()
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn working_task(reg: &TaskRegistry) -> String {
        reg.create(TaskKind::AgentSpawn, "orchestrator", Some("sess-1"))
    }

    #[test]
    fn create_then_get_round_trips() {
        let reg = TaskRegistry::new();
        let id = working_task(&reg);

        let rec = reg.get(&id).expect("task must exist");
        assert_eq!(rec.task_id, id);
        assert_eq!(rec.kind, TaskKind::AgentSpawn);
        assert_eq!(rec.owner, "orchestrator");
        assert_eq!(rec.session_id.as_deref(), Some("sess-1"));
        assert_eq!(rec.status, TaskStatus::Working);
        assert_eq!(rec.status_message, None);
        assert_eq!(rec.result, None);
        assert_eq!(rec.error, None);
        assert_eq!(
            rec.created_at, rec.updated_at,
            "a fresh task has never been updated"
        );
        assert_eq!(rec.expires_at, rec.created_at + TASK_TTL_SECS);
    }

    #[test]
    fn each_terminal_state_is_reachable_and_carries_its_payload() {
        let reg = TaskRegistry::new();

        let completed = working_task(&reg);
        reg.set_status(
            &completed,
            TaskStatus::Completed,
            TaskUpdate {
                result: Some(serde_json::json!({"session_id": "sess-1"})),
                ..Default::default()
            },
        )
        .expect("working → completed");
        let rec = reg.get(&completed).unwrap();
        assert_eq!(rec.status, TaskStatus::Completed);
        assert_eq!(rec.result.unwrap()["session_id"], "sess-1");

        let failed = working_task(&reg);
        reg.set_status(
            &failed,
            TaskStatus::Failed,
            TaskUpdate {
                error: Some("agent exited 1".to_string()),
                ..Default::default()
            },
        )
        .expect("working → failed");
        let rec = reg.get(&failed).unwrap();
        assert_eq!(rec.status, TaskStatus::Failed);
        assert_eq!(rec.error.as_deref(), Some("agent exited 1"));

        let cancelled = working_task(&reg);
        reg.cancel(&cancelled).expect("working → cancelled");
        assert_eq!(reg.get(&cancelled).unwrap().status, TaskStatus::Cancelled);
    }

    /// The spec makes terminal states immutable, and Phase G's `tasks/*` front
    /// door relies on it: a late-arriving worker update must not resurrect a task
    /// the orchestrator already cancelled.
    #[test]
    fn a_terminal_task_rejects_every_further_transition() {
        let reg = TaskRegistry::new();

        for terminal in [
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            let id = working_task(&reg);
            reg.set_status(&id, terminal, TaskUpdate::default())
                .expect("first transition");

            for attempt in [
                TaskStatus::Working,
                TaskStatus::InputRequired,
                TaskStatus::Completed,
                TaskStatus::Failed,
                TaskStatus::Cancelled,
            ] {
                let err = reg
                    .set_status(
                        &id,
                        attempt,
                        TaskUpdate {
                            error: Some("late worker write".to_string()),
                            ..Default::default()
                        },
                    )
                    .expect_err("a terminal task is immutable");
                assert_eq!(err, TaskError::Terminal(terminal));
            }

            let rec = reg.get(&id).unwrap();
            assert_eq!(rec.status, terminal, "state must be untouched");
            assert_eq!(rec.error, None, "rejected payload must not be written");
        }
    }

    /// `input_required` is the spec's "waiting on the human" state — a task there
    /// is still live and must be able to resume.
    #[test]
    fn input_required_is_not_terminal_and_can_return_to_working() {
        let reg = TaskRegistry::new();
        let id = working_task(&reg);

        assert!(!TaskStatus::InputRequired.is_terminal());
        reg.set_status(
            &id,
            TaskStatus::InputRequired,
            TaskUpdate {
                status_message: Some("needs approval".to_string()),
                ..Default::default()
            },
        )
        .expect("working → input_required");
        let rec = reg.get(&id).unwrap();
        assert_eq!(rec.status, TaskStatus::InputRequired);
        assert_eq!(rec.status_message.as_deref(), Some("needs approval"));

        reg.set_status(&id, TaskStatus::Working, TaskUpdate::default())
            .expect("input_required → working");
        let rec = reg.get(&id).unwrap();
        assert_eq!(rec.status, TaskStatus::Working);
        assert_eq!(
            rec.status_message.as_deref(),
            Some("needs approval"),
            "an omitted field must not be wiped by a later transition"
        );
    }

    /// The session-exit path finishes tasks through this lookup. It must skip
    /// terminal rows, or a cancelled task would be resurrected as completed when
    /// the agent it was watching eventually exits.
    #[test]
    fn live_ids_for_session_skips_terminal_and_foreign_tasks() {
        let reg = TaskRegistry::new();
        let live = working_task(&reg);
        let waiting = working_task(&reg);
        let cancelled = working_task(&reg);
        let other_session = reg.create(TaskKind::AgentSpawn, "orchestrator", Some("sess-2"));
        let sessionless = reg.create(TaskKind::AgentSpawn, "orchestrator", None);

        reg.set_status(&waiting, TaskStatus::InputRequired, TaskUpdate::default())
            .expect("transition");
        reg.cancel(&cancelled).expect("cancel");

        let ids = reg.live_ids_for_session("sess-1");
        assert_eq!(ids.len(), 2, "only the live tasks of sess-1: {ids:?}");
        assert!(ids.contains(&live));
        assert!(ids.contains(&waiting), "input_required is still live");
        assert!(!ids.contains(&cancelled));
        assert!(!ids.contains(&other_session));
        assert!(!ids.contains(&sessionless));
        assert_eq!(
            reg.live_ids_for_session("unknown-session"),
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn reap_expired_drops_only_past_ttl_tasks() {
        let reg = TaskRegistry::new();
        let stale = working_task(&reg);
        let fresh = working_task(&reg);

        reg.tasks
            .get_mut(&stale)
            .expect("stale task")
            .value_mut()
            .expires_at = unix_now() - 1;

        assert_eq!(reg.reap_expired(), 1);
        assert!(reg.get(&stale).is_none(), "expired task is gone");
        assert!(reg.get(&fresh).is_some(), "fresh task survives");
        assert_eq!(
            reg.reap_expired(),
            0,
            "a second reap has nothing left to do"
        );
    }

    #[test]
    fn an_unknown_task_id_is_not_found() {
        let reg = TaskRegistry::new();
        assert!(reg.get("no-such-task").is_none());
        assert_eq!(
            reg.set_status("no-such-task", TaskStatus::Completed, TaskUpdate::default()),
            Err(TaskError::NotFound)
        );
        assert_eq!(reg.cancel("no-such-task"), Err(TaskError::NotFound));
    }

    /// Phase G serializes these strings straight onto the `tasks/*` surface, where
    /// they are the spec. Keep the explicit expected spellings aligned with serde.
    #[test]
    fn status_as_str_matches_the_wire() {
        for status in [
            TaskStatus::Working,
            TaskStatus::InputRequired,
            TaskStatus::Completed,
            TaskStatus::Failed,
            TaskStatus::Cancelled,
        ] {
            assert_eq!(
                serde_json::to_value(status).expect("serialize"),
                serde_json::Value::String(status.as_str().to_string()),
                "{status:?} spelling drifted from the wire"
            );
        }

        assert_eq!(
            serde_json::to_value(TaskKind::AgentSpawn).expect("serialize"),
            serde_json::Value::String("agent_spawn".to_string())
        );
    }

    /// Concurrent transitions must not both succeed: exactly one writer wins and
    /// the rest see the terminal state.
    #[test]
    fn a_terminal_transition_is_atomic_under_contention() {
        let reg = std::sync::Arc::new(TaskRegistry::new());
        let id = working_task(&reg);

        let winners: usize = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|_| {
                    let reg = std::sync::Arc::clone(&reg);
                    let id = id.clone();
                    scope.spawn(move || {
                        reg.set_status(&id, TaskStatus::Completed, TaskUpdate::default())
                            .is_ok()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("thread must not panic"))
                .filter(|won| *won)
                .count()
        });

        assert_eq!(winners, 1, "exactly one transition may win the race");
        assert_eq!(reg.get(&id).unwrap().status, TaskStatus::Completed);
    }
}
