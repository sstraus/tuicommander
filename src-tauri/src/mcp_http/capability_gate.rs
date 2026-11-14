//! MCP enforcement of the per-agent capabilities fixed at spawn (story 2713).
//!
//! Both checks read only `AppState::agent_capabilities`, which spawn alone
//! writes; no message, prompt or later argument is consulted.

use std::sync::Arc;

use serde_json::{Value, json};

use crate::agent_capabilities::{self, Capability, CapabilitySet};
use crate::state::AppState;

/// Refuse an MCP call that its caller's fixed capability set does not cover.
/// Callers without a set (roots, hand-opened tabs, unbound clients) pass.
pub(super) fn refuse_call(
    state: &Arc<AppState>,
    tool: &str,
    args: &Value,
    mcp_session_id: Option<&str>,
) -> Option<Value> {
    let action = args["action"].as_str();
    let capability = agent_capabilities::required_for(tool, action)?;
    let caller = super::mcp_transport_ancillary::resolve_mcp_origin_session(state, mcp_session_id)?;
    let held = state.agent_capabilities.get(&caller)?.clone();
    if !held.contains(capability) {
        return Some(deny(state, &caller, capability, tool, action));
    }
    // A launch names its parent, so the spawn path checks that parent's set;
    // the caller's set must bound the grant too, or naming an uncapped peer
    // widens the grandchild. An invalid argument is left to the spawn path.
    let granted = (tool == "agent" && action == Some("launch"))
        .then(|| CapabilitySet::from_spawn_arg(args.pointer("/spawn/capabilities")?).ok())
        .flatten()?;
    let missing = granted.first_outside(&held)?;
    Some(deny(state, &caller, missing, tool, action))
}

/// The set a spawn fixes for its new session: the `capabilities` argument,
/// else none for a child, else no set (uncapped). A capped spawner may grant
/// only capabilities it holds.
pub(super) fn spawn_set(
    state: &Arc<AppState>,
    args: &Value,
    is_child: bool,
    spawner: Option<&str>,
) -> Result<Option<CapabilitySet>, Value> {
    let requested = match args.get("capabilities") {
        Some(value) => CapabilitySet::from_spawn_arg(value).map_err(
            |error| json!({"error": error, "code": "invalid_arguments", "effect": "not_started"}),
        )?,
        None if is_child => CapabilitySet::default(),
        None => return Ok(None),
    };
    let held = spawner.and_then(|spawner| {
        state
            .agent_capabilities
            .get(spawner)
            .map(|set| (spawner, set.clone()))
    });
    if let Some((spawner, held)) = held
        && let Some(missing) = requested.first_outside(&held)
    {
        return Err(deny(state, spawner, missing, "agent", Some("spawn")));
    }
    Ok(Some(requested))
}

/// The typed refusal. The refused session's parent gets one mail per
/// capability, however often the session retries.
fn deny(
    state: &Arc<AppState>,
    session: &str,
    capability: Capability,
    tool: &str,
    action: Option<&str>,
) -> Value {
    if state
        .capability_denials_mailed
        .insert((session.to_owned(), capability))
    {
        crate::pty::push_state_change_to_parent(
            state,
            session,
            json!({
                "type": "capability_denied",
                "capability": capability.as_str(),
                "tool": tool,
                "action": action,
                "session_id": session,
            }),
        );
    }
    json!({
        "error": format!(
            "This agent lacks the '{}' capability. TUIC fixed its capabilities at spawn; text sent to the agent cannot change them.",
            capability.as_str()
        ),
        "code": "capability_denied",
        "capability": capability.as_str(),
        "effect": "not_started",
        "retryable": false,
    })
}
