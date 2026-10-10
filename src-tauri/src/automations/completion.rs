//! Reconcile durable runs against host-owned evidence, including after bus lag.
use super::{
    run::{AutomationRun, OUTPUT_LIMIT, RunDetails, RunStatus},
    store::RunStore,
};
use crate::{
    state::{AppEvent, AppState},
    tasks::TaskStatus,
};

/// All observations are scoped to the binding persisted by dispatch.
#[derive(Default)]
pub(crate) struct Observation {
    pub task: Option<TaskStatus>,
    pub task_unknown_exit: bool,
    pub live: bool,
    pub awaiting: bool,
    pub exit: Option<i32>,
    pub progress: Option<crate::progress::ProgressKind>,
    pub output: Option<String>,
}

pub(crate) trait CompletionEffects {
    fn observe(&self, run: &AutomationRun) -> Observation;
    fn stop(&self, run: &AutomationRun) -> Result<(), String>;
    fn publish(&self, run: &AutomationRun);
}

/// The runtime has one monitor, and final ledger transitions reject late signals.
pub(crate) fn reconcile(
    runs: &RunStore,
    effects: &impl CompletionEffects,
    now: i64,
) -> Result<(), String> {
    let mut stop_errors = Vec::new();
    for run in runs.open_runs()? {
        let observed = effects.observe(&run);
        let expired = now >= run.deadline_ms.unwrap_or_else(|| deadline(&run));
        let next = if expired {
            if let Err(error) = effects.stop(&run) {
                stop_errors.push(format!("Run {}: {error}", run.id));
                continue;
            }
            Some(RunStatus::TimedOut)
        } else if !matches!(run.status, RunStatus::Running | RunStatus::NeedsYou) {
            None
        } else {
            classify(&observed)
        };
        let Some(next) = next else { continue };
        if next == run.status {
            continue;
        }
        let saved = runs.transition(
            &run.id,
            next,
            RunDetails {
                stdout: (!next.is_open()).then_some(observed.output).flatten(),
                reason: Some(
                    match next {
                        RunStatus::Completed => {
                            "Completion confirmed by task, progress or known process exit"
                        }
                        RunStatus::Failed => "Owned task or process failed",
                        RunStatus::NeedsYou => "Owned agent requires input",
                        RunStatus::TimedOut => "Maximum duration expired; owned session stopped",
                        RunStatus::Unknown => {
                            "Completion evidence lost or unverifiable; not retried"
                        }
                        _ => "Owned agent resumed",
                    }
                    .into(),
                ),
                ..Default::default()
            },
            now.max(run.updated_ms),
        )?;
        effects.publish(&saved);
    }
    if stop_errors.is_empty() {
        Ok(())
    } else {
        Err(stop_errors.join("; "))
    }
}

fn deadline(run: &AutomationRun) -> i64 {
    run.created_ms.saturating_add(
        i64::try_from(run.definition.max_duration_secs.saturating_mul(1000)).unwrap_or(i64::MAX),
    )
}

fn classify(o: &Observation) -> Option<RunStatus> {
    use crate::progress::ProgressKind;
    // Nonzero exit and explicit task failure outrank a previous progress report.
    if o.exit.is_some_and(|code| code != 0) || o.task == Some(TaskStatus::Failed) {
        Some(RunStatus::Failed)
    } else if o.progress == Some(ProgressKind::Done)
        || o.exit == Some(0)
        || (o.task == Some(TaskStatus::Completed) && !o.task_unknown_exit)
    {
        Some(RunStatus::Completed)
    } else if o.task == Some(TaskStatus::Cancelled) || !o.live {
        Some(RunStatus::Unknown)
    } else if o.awaiting
        || o.task == Some(TaskStatus::InputRequired)
        || o.progress == Some(ProgressKind::Blocked)
    {
        Some(RunStatus::NeedsYou)
    } else {
        Some(RunStatus::Running)
    }
}

pub(crate) struct NativeCompletion<'a>(pub &'a std::sync::Arc<AppState>);
impl CompletionEffects for NativeCompletion<'_> {
    fn observe(&self, run: &AutomationRun) -> Observation {
        let Some(session) = run.session_id.as_deref() else {
            return Observation::default();
        };
        let task = run
            .task_id
            .as_deref()
            .and_then(|id| self.0.tasks.get(id))
            .filter(|task| task.session_id.as_deref() == Some(session));
        let progress = run
            .workspace
            .as_deref()
            .and_then(|workspace| {
                crate::progress::progress_list(
                    workspace,
                    crate::progress::ProgressListInput {
                        pty_id: Some(session.into()),
                        limit: Some(100),
                        ..Default::default()
                    },
                )
                .ok()
            })
            .and_then(|page| {
                page.entries
                    .into_iter()
                    .find(|entry| {
                        i128::from(entry.created_at_ms)
                            >= i128::from(run.started_ms.unwrap_or(run.created_ms))
                            && matches!(
                                entry.kind,
                                crate::progress::ProgressKind::Done
                                    | crate::progress::ProgressKind::Blocked
                                    | crate::progress::ProgressKind::Intent
                            )
                    })
                    .map(|entry| entry.kind)
            });
        Observation {
            task: task.as_ref().map(|t| t.status),
            // PTY task completion explicitly permits absent exit codes. Automations
            // require positive evidence instead of inheriting that optimistic default.
            task_unknown_exit: task
                .as_ref()
                .and_then(|t| t.result.as_ref())
                .is_some_and(|r| r.get("exit_code").is_some_and(serde_json::Value::is_null)),
            live: self.0.session_maps.sessions.contains_key(session),
            awaiting: self
                .0
                .session_maps
                .session_states
                .get(session)
                .is_some_and(|s| s.awaiting_input),
            exit: self
                .0
                .session_maps
                .exit_codes
                .get(session)
                .map(|code| *code),
            progress,
            output: self
                .0
                .session_maps
                .output_buffers
                .get(session)
                .map(|buffer| {
                    String::from_utf8_lossy(&buffer.lock().read_last(OUTPUT_LIMIT + 4).0)
                        .into_owned()
                }),
        }
    }

    fn stop(&self, run: &AutomationRun) -> Result<(), String> {
        let Some(session) = run.session_id.as_deref() else {
            return Ok(());
        };
        // A binding is ownership, not an arbitrary session selected by an event.
        if self.0.session_maps.sessions.contains_key(session) {
            crate::mcp_http::managed_launch::stop(self.0, session)?;
        }
        if let Some(task) = run.task_id.as_deref()
            && self
                .0
                .tasks
                .get(task)
                .is_some_and(|record| record.session_id.as_deref() == Some(session))
        {
            let _ = self.0.tasks.cancel(task);
        }
        Ok(())
    }

    fn publish(&self, run: &AutomationRun) {
        let payload = serde_json::json!({"run": run});
        #[cfg(feature = "desktop")]
        if let Some(app) = self.0.app_handle.read().as_ref() {
            use tauri::Emitter;
            let _ = app.emit("automation-run-changed", &payload);
        }
        let _ = self
            .0
            .event_bus
            .send(AppEvent::AutomationRunChanged { payload });
    }
}

#[cfg(test)]
mod tests;
