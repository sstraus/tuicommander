//! One reserved run becomes a durable precheck decision and at most one launch.
use super::{
    definitions::DefinitionStore,
    model::{AutomationDefinition, Workspace},
    precheck::{PrecheckOutcome, Termination, run_precheck},
    run::{AutomationRun, RunDetails, RunStatus, RunTrigger, SavedOutput, SavedPrecheck},
    scheduler,
    store::RunStore,
};
use std::{future::Future, path::Path};

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
}

pub(crate) struct Dispatcher<E> {
    pub definitions: DefinitionStore,
    pub runs: RunStore,
    pub effects: E,
}

impl<E: DispatchEffects> Dispatcher<E> {
    /// Shared manual boundary: paused Once definitions may run, but capacity
    /// and overlap refusals are recorded immediately rather than queued.
    pub async fn run_now(&self, id: &str) -> Result<AutomationRun, String> {
        let config = self.definitions.load()?;
        let definition = config
            .definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or("Automation not found")?;
        require_once(definition)?;
        let run = scheduler::run_now(&self.runs, &config, id, chrono::Utc::now())?;
        self.dispatch(&run.id).await?;
        self.runs.get(&run.id)
    }

    /// The runtime uses this admission boundary on every 30-second wake.
    pub fn reserve_due(
        &self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<AutomationRun>, String> {
        let mut config = self.definitions.load()?;
        config.definitions.retain(|d| d.once_local.is_some());
        scheduler::tick(&self.runs, &config, now)
    }

    pub async fn dispatch(&self, id: &str) -> Result<(), String> {
        let Some(run) = self.runs.claim_dispatch(id, now_ms())? else {
            return Ok(());
        };
        if let Err(error) = self.dispatch_claimed(&run).await {
            self.runs.transition(
                id,
                RunStatus::Failed,
                RunDetails {
                    reason: Some(error),
                    ..Default::default()
                },
                now_ms(),
            )?;
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
        let precheck = match &outcome {
            PrecheckOutcome::Executed(result) => Some(SavedPrecheck {
                exit_code: if let Termination::Exited(code) = result.termination {
                    code
                } else {
                    None
                },
                timed_out: matches!(result.termination, Termination::TimedOut),
                stdout: SavedOutput {
                    text: result.stdout.clone(),
                    truncated: result.stdout_truncated,
                },
                stderr: SavedOutput {
                    text: result.stderr.clone(),
                    truncated: result.stderr_truncated,
                },
            }),
            _ => None,
        };
        self.runs.transition(
            &run.id,
            if proceed {
                RunStatus::Prechecking
            } else {
                RunStatus::SkippedPrecheck
            },
            RunDetails {
                precheck,
                precheck_outcome: Some(outcome),
                ..Default::default()
            },
            now_ms(),
        )?;
        if !proceed {
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
                prompt: run.definition.prompt.clone(),
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
        Ok(())
    }

    fn require_current(&self, run: &AutomationRun) -> Result<(), String> {
        require_once(&run.definition)?;
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

fn require_once(definition: &AutomationDefinition) -> Result<(), String> {
    if definition.once_local.is_none() {
        return Err("Only Once automations dispatch in phase 1".into());
    }
    definition.validate()
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
