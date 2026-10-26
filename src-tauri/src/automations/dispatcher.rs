//! One reserved run becomes a durable precheck decision and at most one launch.
use super::{
    context::inject_context,
    definitions::DefinitionStore,
    model::{AutomationDefinition, Workspace},
    precheck::run_precheck,
    run::{AutomationRun, RunDetails, RunStatus, RunTrigger},
    scheduler,
    store::RunStore,
};
use std::{future::Future, path::Path, sync::Arc};

pub(crate) mod native;

#[derive(Clone, Debug)]
pub(crate) struct LaunchRequest {
    pub run_id: String,
    pub name: String,
    pub run_config: String,
    pub workspace: String,
    pub prompt: String,
}

pub(crate) struct LaunchBinding {
    pub session_id: String,
    pub task_id: String,
}

pub(crate) struct ResolvedWorkspace {
    pub path: String,
    pub id: Option<String>,
}

/// Only TUIC-owned workspace/managed-process effects vary in integration tests.
pub(crate) trait DispatchEffects: Send + Sync {
    fn workspace(
        &self,
        definition: &AutomationDefinition,
        id: &str,
    ) -> impl Future<Output = Result<ResolvedWorkspace, String>> + Send;
    fn launch(
        &self,
        request: LaunchRequest,
    ) -> impl Future<Output = Result<LaunchBinding, String>> + Send;
    fn stop(&self, binding: &LaunchBinding) -> Result<(), String>;
    fn publish(&self, _run: &AutomationRun) {}
}

pub(crate) struct Dispatcher<E> {
    pub definitions: DefinitionStore,
    pub runs: RunStore,
    pub effects: E,
}

impl<E: DispatchEffects> Dispatcher<E> {
    /// Shared manual boundary: paused definitions may run, but capacity
    /// and overlap refusals are recorded immediately rather than queued.
    pub async fn run_now(self: &Arc<Self>, id: &str) -> Result<AutomationRun, String>
    where
        E: 'static,
    {
        let config = self.definitions.load()?;
        let definition = config
            .definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or("Automation not found")?;
        definition.validate()?;
        let run = scheduler::run_now(&self.runs, &config, id, chrono::Utc::now())?;
        // Admission and task creation have no intervening yield. The task holds
        // the writer/owner lock until dispatch settles, even if the caller drops.
        let dispatcher = self.clone();
        tokio::spawn(async move {
            dispatcher.dispatch(&run.id).await.inspect_err(|error| {
                tracing::warn!(source="automations", %error, run_id=%run.id, "Automation dispatch persistence failed");
            })?;
            dispatcher.runs.get(&run.id)
        })
        .await
        .map_err(|error| format!("Automation dispatch task failed: {error}"))?
    }

    /// The runtime uses this admission boundary on every 30-second wake.
    pub fn reserve_due(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<AutomationRun>, String> {
        self.runs.prune(now.timestamp_millis(), None)?;
        scheduler::tick(&self.runs, &self.definitions.load()?, now)
    }

    /// Admit eligible labelled issues from the poller's latest snapshot. Safe to
    /// repeat: the run store admits each issue and label once.
    pub fn admit_events(
        &self,
        issues: &[tuic_git::github::GitHubIssue],
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<AutomationRun>, String> {
        super::event_intake::admit_issues(
            &self.runs,
            &self.definitions.load()?,
            issues,
            now.timestamp_millis(),
        )
    }

    pub async fn dispatch(&self, id: &str) -> Result<(), String> {
        let Some(run) = self.runs.claim_dispatch(id, now_ms())? else {
            return Ok(());
        };
        if let Err(error) = self.dispatch_claimed(&run).await {
            let saved = self.runs.transition(
                id,
                RunStatus::Failed,
                RunDetails {
                    reason: Some(error),
                    ..Default::default()
                },
                now_ms(),
            )?;
            self.effects.publish(&saved);
        }
        Ok(())
    }

    async fn dispatch_claimed(&self, run: &AutomationRun) -> Result<(), String> {
        self.require_current(run)?;
        let workspace = self.effects.workspace(&run.definition, &run.id).await?;
        let path = Path::new(&workspace.path)
            .canonicalize()
            .map_err(|e| format!("resolve automation workspace: {e}"))?;
        if !path.is_dir() {
            return Err("Automation workspace is not a directory".into());
        }
        let path = path.to_string_lossy().into_owned();
        self.runs.transition(
            &run.id,
            RunStatus::Prechecking,
            RunDetails {
                workspace: Some(path.clone()),
                workspace_id: workspace.id,
                ..Default::default()
            },
            now_ms(),
        )?;
        self.require_current(run)?;
        let outcome = run_precheck(
            Path::new(&path),
            run.definition.precheck.as_ref(),
            matches!(run.trigger, RunTrigger::Manual),
        )
        .await;
        let proceed = outcome.proceeds();
        let (prompt, context) = if proceed {
            let (prompt, context) = inject_context(
                &outcome,
                run.definition.precheck.as_ref(),
                &run.definition.prompt,
            );
            match super::event_intake::event_context(&run.trigger) {
                Some(event) => (format!("{prompt}\n\n{event}"), context),
                None => (prompt, context),
            }
        } else {
            (String::new(), Default::default())
        };
        let saved = self.runs.transition(
            &run.id,
            if proceed {
                RunStatus::Prechecking
            } else {
                RunStatus::SkippedPrecheck
            },
            RunDetails {
                precheck_outcome: Some(outcome),
                precheck_context: Some(context),
                ..Default::default()
            },
            now_ms(),
        )?;
        if !proceed {
            self.effects.publish(&saved);
            return Ok(());
        }
        self.require_current(run)?;
        if !Path::new(&path).is_dir() {
            return Err("Automation workspace disappeared before launch".into());
        }
        let binding = self
            .effects
            .launch(LaunchRequest {
                run_id: run.id.clone(),
                name: run.definition.name.clone(),
                run_config: run.definition.run_config.clone(),
                workspace: path,
                prompt,
            })
            .await?;
        let bound = self.runs.transition(
            &run.id,
            RunStatus::Running,
            RunDetails {
                session_id: Some(binding.session_id.clone()),
                task_id: Some(binding.task_id.clone()),
                ..Default::default()
            },
            now_ms(),
        );
        if !matches!(&bound, Ok(saved) if saved.status == RunStatus::Running && saved.session_id.as_deref() == Some(&binding.session_id))
        {
            self.effects.stop(&binding)?;
            return Err("Automation stopped before launch binding; owned child terminated".into());
        }
        if let Ok(saved) = bound {
            self.effects.publish(&saved);
        }
        Ok(())
    }

    fn require_current(&self, run: &AutomationRun) -> Result<(), String> {
        run.definition.validate()?;
        if self.runs.get(&run.id)?.status != RunStatus::Prechecking {
            return Err("Automation stopped before dispatch".into());
        }
        let mut latest = self.definitions.get(&run.definition.id)?;
        if matches!(run.trigger, RunTrigger::Manual) {
            latest.enabled = run.definition.enabled;
        }
        if !matches!(run.trigger, RunTrigger::Manual) && !latest.enabled {
            return Err("Automation paused before dispatch".into());
        }
        if latest != run.definition {
            return Err("Automation definition changed before dispatch".into());
        }
        Ok(())
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
