//! Common managed-agent assembly; callers retain their own authorization fences.
use super::mcp_transport_session_agent::{handle_agent_with_parent_cwd, handle_session};
use crate::state::AppState;
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc};

pub(super) fn launch(
    state: &Arc<AppState>,
    addr: SocketAddr,
    origin: Option<&str>,
    profile: &str,
    name: &str,
    prompt: &str,
    workspace: &str,
) -> Value {
    handle_agent_with_parent_cwd(
        state,
        addr,
        &json!({
            "action":"spawn", "agent_type":profile, "name":name,
            "prompt":prompt, "cwd":workspace,
        }),
        origin,
        None,
    )
}

/// A definition names a saved run configuration, never an implicit CLI fallback.
pub(crate) fn launch_named(
    state: &Arc<AppState>,
    profile: &str,
    name: &str,
    prompt: &str,
    workspace: &str,
) -> Result<Value, String> {
    let config = crate::config::load_agents_config();
    if !config.agents.values().any(|agent| {
        agent
            .run_configs
            .iter()
            .any(|run| run.name.eq_ignore_ascii_case(profile))
    }) {
        return Err(format!(
            "Automation run configuration '{profile}' not found"
        ));
    }
    let spawned = launch(
        state,
        SocketAddr::from(([127, 0, 0, 1], 0)),
        None,
        profile,
        name,
        prompt,
        workspace,
    );
    if let Some(error) = spawned["error"].as_str() {
        return Err(error.into());
    }
    Ok(spawned)
}

pub(crate) fn stop(state: &Arc<AppState>, session: &str) -> Result<(), String> {
    let result = handle_session(state, &json!({"action":"kill", "session_id":session}), None);
    if let Some(error) = result["error"].as_str() {
        return Err(error.into());
    }
    Ok(())
}
