//! Workflow-only assembly for the genuine ACP managed-launch adapter.
use super::managed_launch::Launch;
use crate::state::AppState;
use serde_json::{Value, json};
use std::sync::Arc;

pub(super) fn launch(
    state: &Arc<AppState>,
    input: Launch<'_>,
    profile: &str,
    model: &str,
    effort: &str,
    runtime: &crate::workflows::AcpRuntimeRoute,
) -> Value {
    let request = json!({
        "idempotency_key":input.idempotency_key,
        "parent_session_id":input.owner_scope,
        "name":input.name, "keep_open":true,
        "workspace":{"existing_path":input.workspace},
        "spawn":{"prompt":input.prompt,"acp":{
            "executable":runtime.executable,"machine_profile":runtime.machine_profile,
            "profile":profile,"model":model,"effort":effort,
        }},
    });
    let runtime = match tokio::runtime::Handle::try_current() {
        Ok(runtime) => runtime,
        Err(error) => {
            return json!({"error":format!("ACP launch requires the runtime worker: {error}")});
        }
    };
    let receipt = runtime.block_on(super::launch_transaction::execute_owned(
        state,
        ([127, 0, 0, 1], 0).into(),
        &request,
    ));
    if receipt.get("error").is_some() {
        return receipt;
    }
    if receipt["reason"].is_string() || receipt["live"] != true || !receipt["child"].is_object() {
        return json!({"error":receipt["reason"],"launch_id":receipt["launch_id"]});
    }
    let mut child = receipt["child"].clone();
    child["launch_id"] = receipt["launch_id"].clone();
    child
}

pub(super) fn deliver(state: &Arc<AppState>, launch_id: &str) -> Result<Value, String> {
    let runtime = tokio::runtime::Handle::try_current().map_err(|e| e.to_string())?;
    let receipt = runtime.block_on(super::launch_transaction::deliver_acp_owned(
        state, launch_id,
    ))?;
    if receipt["prompt_delivery"] != "delivered" {
        return Err(format!(
            "ACP prompt not confirmed delivered: {}",
            receipt["reason"]
        ));
    }
    Ok(receipt)
}
