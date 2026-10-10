use super::*;
use crate::state::AppState;
use std::sync::Arc;

pub(crate) struct NativeEffects(pub Arc<AppState>);

fn destination(path: &str) -> Result<String, String> {
    let path = Path::new(path)
        .canonicalize()
        .map_err(|e| format!("resolve automation destination: {e}"))?;
    if !path.is_dir() || crate::git::resolve_git_dir(&path).is_none() {
        return Err("Automation destination is not a Git workspace".into());
    }
    Ok(path.to_string_lossy().into_owned())
}

impl DispatchEffects for NativeEffects {
    async fn workspace(
        &self,
        definition: &AutomationDefinition,
        id: &str,
    ) -> Result<ResolvedWorkspace, String> {
        let repository = destination(&definition.repository)?;
        match &definition.workspace {
            Workspace::Existing => Ok(ResolvedWorkspace {
                path: repository,
                id: None,
            }),
            Workspace::NewPerRun { base_branch } => {
                let workspace = crate::mcp_http::mcp_transport::create_daemon_workflow_worktree(
                    &self.0,
                    &repository,
                    &format!("automation/{id}"),
                    Some(base_branch),
                )
                .await?;
                Ok(ResolvedWorkspace {
                    path: destination(&workspace.path)?,
                    id: Some(workspace.id),
                })
            }
        }
    }

    async fn launch(&self, request: LaunchRequest) -> Result<LaunchBinding, String> {
        let state = self.0.clone();
        tokio::task::spawn_blocking(move || {
            if destination(&request.workspace)? != request.workspace {
                return Err("Automation destination changed before launch".into());
            }
            let spawned = crate::mcp_http::managed_launch::launch_named(
                &state,
                &request.run_config,
                &format!("Automation {} ({})", request.name, request.run_id),
                &request.prompt,
                &request.workspace,
            )?;
            let session = spawned["session_id"]
                .as_str()
                .ok_or("Managed launch returned no session id; not retried")?;
            let Some(task) = spawned["task_id"].as_str() else {
                crate::mcp_http::managed_launch::stop(&state, session)?;
                return Err("Managed launch returned no task id; owned child stopped".into());
            };
            Ok(LaunchBinding {
                session_id: session.into(),
                task_id: task.into(),
            })
        })
        .await
        .map_err(|e| format!("Automation launch task: {e}"))?
    }

    fn publish(&self, run: &AutomationRun) {
        use crate::automations::completion::CompletionEffects;
        crate::automations::completion::NativeCompletion(&self.0).publish(run);
    }

    fn stop(&self, binding: &LaunchBinding) -> Result<(), String> {
        crate::mcp_http::managed_launch::stop(&self.0, &binding.session_id)
    }
}
