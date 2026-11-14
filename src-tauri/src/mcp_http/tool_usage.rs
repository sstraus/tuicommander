//! Native tools listed only while the caller's state uses them (story 2807).
//!
//! `workflow` is listed when the caller's project has a workflow definition or
//! an active run; `automations` when an automation exists. A peer registered as
//! orchestrator sees both. This gates discovery only: dispatch keeps its configuration gates.
//!
//! `tools/list` reads the cached [`ToolUsage`], never the stores. [`refresh`]
//! recomputes it and signals `tools_changed` (→ `notifications/tools/list_changed`
//! and a search index rebuild) only when it changed.

use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};

use tokio::sync::broadcast;

use crate::state::{AppEvent, AppState};

/// The line the instructions keep when a state-gated tool is hidden.
pub(super) const HIDDEN_LINE: &str = "- `workflow` is listed for a bound project with an installed neutral catalog, or when this repository has a workflow definition or an active run; `automations` is listed only when an automation exists. Create one in the Workflow Designer or the Automations panel; a client that ignores `notifications/tools/list_changed` sees it after an MCP reconnect.\n";

/// What the state-gated tools depend on.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ToolUsage {
    /// Projects with a user-made definition or an active run.
    pub(crate) workflow_projects: BTreeSet<String>,
    /// At least one automation definition exists.
    pub(crate) automations: bool,
}

impl ToolUsage {
    /// Whether `name` is listed for a caller in `project`.
    pub(crate) fn lists(&self, name: &str, project: Option<&str>, orchestrator: bool) -> bool {
        match name {
            "workflow" => {
                orchestrator || project.is_some_and(|p| self.workflow_projects.contains(p))
            }
            "automations" => orchestrator || self.automations,
            _ => true,
        }
    }
}

/// Read the usage from the workflow, run and automation stores.
pub(crate) fn read_usage() -> Result<ToolUsage, String> {
    let workflow_projects: BTreeSet<String> = crate::workflows::WorkflowStore::open()?
        .projects_with_user_definitions()?
        .into_iter()
        .collect();
    let automations = !crate::automations::definitions::DefinitionStore::new()
        .load()?
        .definitions
        .is_empty();
    Ok(ToolUsage {
        workflow_projects,
        automations,
    })
}

/// Recompute the cache; signal a catalogue change only when it moved.
pub(crate) fn refresh(state: &AppState) {
    let usage = match read_usage() {
        Ok(usage) => usage,
        Err(error) => {
            tracing::warn!(source = "tool_usage", %error, "tool usage not refreshed");
            return;
        }
    };
    let changed = {
        let mut current = state.mcp.tool_usage.write();
        let changed = *current != usage;
        *current = usage;
        changed
    };
    if changed {
        let _ = state.mcp.tools_changed.send(());
    }
}

/// Definition writes have no `AppState` at hand (the HTTP route takes none), so
/// they signal here and the updater refreshes every state that listens.
static DEFINITIONS_CHANGED: LazyLock<broadcast::Sender<()>> =
    LazyLock::new(|| broadcast::channel(16).0);

/// A workflow or automation definition may have changed.
pub(crate) fn note_definitions_changed() {
    let _ = DEFINITIONS_CHANGED.send(());
}

/// [`refresh`] off the caller's thread. Without a Tokio runtime it does nothing.
pub(crate) fn refresh_soon(state: &Arc<AppState>) {
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let state = state.clone();
    drop(runtime.spawn_blocking(move || refresh(&state)));
}

/// Build the cache once, then recompute it on every run or definition change.
pub(crate) fn spawn_updater(state: Arc<AppState>) {
    use broadcast::error::RecvError;
    // Subscribe first so a change during the initial read is not lost.
    let mut events = state.event_bus.subscribe();
    let mut definitions = DEFINITIONS_CHANGED.subscribe();
    refresh_soon(&state);
    tokio::spawn(async move {
        loop {
            let relevant = tokio::select! {
                event = events.recv() => match event {
                    Ok(AppEvent::WorkflowRunChanged { .. } | AppEvent::AutomationRunChanged { .. })
                    | Err(RecvError::Lagged(_)) => true,
                    Ok(_) => false,
                    Err(RecvError::Closed) => break,
                },
                change = definitions.recv() => !matches!(change, Err(RecvError::Closed)),
            };
            if relevant {
                refresh_soon(&state);
            }
        }
    });
}

/// The project the workflow stores key this caller under: its registered
/// repository, resolved to the owning project as `neutral_action` does.
fn caller_project(state: &Arc<AppState>, mcp_session_id: Option<&str>) -> Option<String> {
    let registered = super::mcp_transport_ancillary::resolve_mcp_origin_pty(state, mcp_session_id)
        .and_then(|pty| crate::progress::project_for_session(state, &pty))
        .or_else(|| {
            // An AI Chat peer has no PTY: its ACP binding names the workspace,
            // the same root the workflow tool reads for it.
            let origin = state.mcp.acp_origins.get(mcp_session_id?)?;
            let root = state.acp.peer_root(origin.peer_id.as_deref()?)?;
            Some(root.to_string_lossy().into_owned())
        })
        .or_else(|| {
            let roots = mcp_session_id
                .and_then(|sid| state.mcp.sessions.get(sid))
                .and_then(|meta| meta.repo_path.clone())?;
            crate::progress::project_for_path(state, &roots)
        })?;
    crate::progress::resolve_owning_project(Some(&registered))
        .ok()
        .map(|owner| owner.to_string_lossy().to_string())
}

/// Whether this caller is registered as orchestrator (`agent register orchestrator=true`).
fn is_orchestrator(state: &Arc<AppState>, mcp_session_id: Option<&str>) -> bool {
    super::mcp_transport_ancillary::resolve_mcp_origin_session(state, mcp_session_id)
        .is_some_and(|peer| state.orchestrator_peers.contains(&peer))
}

/// Whether `name` is listed for this caller.
pub(super) fn lists(state: &Arc<AppState>, mcp_session_id: Option<&str>, name: &str) -> bool {
    if !matches!(name, "workflow" | "automations") {
        return true;
    }
    if name == "workflow"
        && state.workflow_runtime.neutral_catalog_host().is_ok()
        && caller_project(state, mcp_session_id).is_some()
    {
        return true;
    }
    let usage = state.mcp.tool_usage.read().clone();
    usage.lists(
        name,
        caller_project(state, mcp_session_id).as_deref(),
        is_orchestrator(state, mcp_session_id),
    )
}
