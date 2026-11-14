//! Common managed-agent assembly; callers retain their own authorization fences.
use super::mcp_transport_session_agent::handle_session;
use crate::state::AppState;
use serde_json::{Value, json};
use std::{net::SocketAddr, sync::Arc};

/// Run-owned launch policy of automation bindings.
pub(super) struct Launch<'a> {
    pub profile: &'a str,
    pub name: &'a str,
    pub prompt: &'a str,
    pub workspace: &'a str,
    pub owner_scope: &'a str,
    pub idempotency_key: &'a str,
    pub keep_open: bool,
}

/// Compatibility projection for existing run bindings. All creation and policy
/// now lives in the keyed transaction; call only from a blocking worker.
pub(super) fn launch(
    state: &Arc<AppState>,
    caller: super::guards::CallerAuthority,
    origin: Option<&str>,
    input: Launch<'_>,
) -> Value {
    let Launch {
        profile,
        name,
        prompt,
        workspace,
        owner_scope,
        idempotency_key,
        keep_open,
    } = input;
    let parent = origin.and_then(|origin| {
        state
            .mcp
            .to_session
            .get(origin)
            .map(|entry| entry.value().clone())
    });
    let request = json!({
        "idempotency_key":idempotency_key,
        "parent_session_id":parent.as_deref().unwrap_or(owner_scope),
        "name":name, "keep_open":keep_open,
        "workspace":{"existing_path":workspace},
        "spawn":{"agent_type":profile, "prompt":prompt},
    });
    let runtime = match tokio::runtime::Handle::try_current() {
        Ok(runtime) => runtime,
        Err(error) => {
            return json!({"error":format!("Managed launch requires the runtime worker: {error}")});
        }
    };
    let receipt = runtime.block_on(async {
        if parent.is_some() {
            super::launch_transaction::execute(state, caller, &request).await
        } else {
            super::launch_transaction::execute_owned(state, caller, &request).await
        }
    });
    if receipt.get("error").is_some() {
        return receipt;
    }
    if receipt["stage"] == "blocked" || receipt["live"] != true {
        return json!({"error":receipt["reason"], "launch_id":receipt["launch_id"]});
    }
    let mut child = receipt["child"].as_object().map(|value| Value::Object(value.clone())).unwrap_or_else(|| {
        let session = receipt["session_id"].as_str().unwrap_or_default();
        json!({"session_id":session, "task_id":state.tasks.live_ids_for_session(session).first(), "name":name})
    });
    child["launch_id"] = receipt["launch_id"].clone();
    child
}

/// A definition names a saved run configuration, never an implicit CLI fallback.
pub(crate) fn launch_named(
    state: &Arc<AppState>,
    profile: &str,
    name: &str,
    prompt: &str,
    workspace: &str,
    run_id: &str,
) -> Result<Value, String> {
    let config = crate::config::load_agents_config();
    config.require_named_run_config(profile)?;
    let spawned = launch(
        state,
        SocketAddr::from(([127, 0, 0, 1], 0)).into(),
        None,
        Launch {
            profile,
            name,
            prompt,
            workspace,
            owner_scope: &format!("automation:{run_id}"),
            idempotency_key: run_id,
            keep_open: true,
        },
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

/// Reuse explicit session close: no worktree cleanup, desktop and SSE stay in sync.
pub(crate) fn close(state: &Arc<AppState>, session: &str) -> Result<(), String> {
    let result = handle_session(
        state,
        &json!({"action":"close", "session_id":session}),
        None,
    );
    if let Some(error) = result["error"].as_str() {
        return Err(error.into());
    }
    Ok(())
}
