//! Fabbrica help requests, approvals and ask-state outside requests for Waiting on you and the activity
//! bell. They are read through the Fabbrica control MCP, configured as the MCP
//! upstream named `fabbrica` (OAuth, one owner consent). The control MCP has no
//! cross-task "waiting on you" tool, so one poll calls `tasks_list`, then
//! `help_list` for every active task, then `outside_requests_list`. Outside
//! requests are answered here through `outside_request_answer`; help requests
//! and approvals are answered in the Fabbrica console. Answers stay in the Fabbrica console; a
//! read that fails publishes an explicit unknown state, never an empty list.
use crate::AppState;
use crate::boss_attention::{AttentionItem, AttentionKind, BossAttention, OptionLabel};
use crate::mcp_proxy::registry::{UpstreamRegistry, UpstreamStatus};
use crate::mcp_upstream_config::UpstreamTransport;
use serde::Serialize;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

#[cfg(test)]
#[path = "fabbrica_attention_tests.rs"]
mod tests;

/// Name of the MCP upstream that holds the Fabbrica control MCP.
pub(crate) const UPSTREAM: &str = "fabbrica";
/// Fabbrica pushes nothing to TUIC, so its help questions are polled.
const POLL_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub(crate) enum FabbricaState {
    /// No enabled upstream named `fabbrica` exists.
    #[default]
    Absent,
    /// The upstream waits for the owner's OAuth consent.
    Connect,
    /// The last read failed; Fabbrica items are not known.
    Unknown { reason: String },
    /// The last read succeeded; `items` is the complete list.
    Ready,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub(crate) struct FabbricaView {
    #[serde(flatten)]
    pub state: FabbricaState,
    /// The Fabbrica console, where the items are answered.
    pub console: Option<String>,
    #[serde(skip)]
    pub items: Vec<AttentionItem>,
}

/// Merge the last Fabbrica read into a waiting-items snapshot.
pub(crate) fn add_to(result: &mut BossAttention, state: &AppState) {
    let view = state.mcp.fabbrica.read().clone();
    result.items.extend(view.items.iter().cloned());
    result.fabbrica = view;
}

/// Poll the control MCP, and re-read at once when the upstream changes state
/// (for example after the owner gives consent).
pub(crate) fn spawn_poller(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut events = state.event_bus.subscribe();
        loop {
            refresh(&state).await;
            tokio::select! {
                _ = tokio::time::sleep(POLL_INTERVAL) => {}
                _ = upstream_changed(&mut events) => {}
            }
        }
    });
}

async fn upstream_changed(events: &mut tokio::sync::broadcast::Receiver<crate::state::AppEvent>) {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match events.recv().await {
            Ok(crate::state::AppEvent::UpstreamStatusChanged { name, .. }) if name == UPSTREAM => {
                return;
            }
            Ok(_) => {}
            Err(RecvError::Lagged(_)) => return,
            Err(RecvError::Closed) => std::future::pending::<()>().await,
        }
    }
}

/// Read Fabbrica once and wake the waiting-items readers when the view changed.
pub(crate) async fn refresh(state: &AppState) {
    let view = read(&state.mcp.upstream_registry).await;
    let changed = {
        let mut current = state.mcp.fabbrica.write();
        let changed = *current != view;
        *current = view;
        changed
    };
    if changed {
        crate::boss_attention::changed(state);
    }
}

async fn read(registry: &UpstreamRegistry) -> FabbricaView {
    let Some(entry) = registry.entry(UPSTREAM) else {
        return FabbricaView::default();
    };
    let console = match &entry.config.transport {
        UpstreamTransport::Http { url } => reqwest::Url::parse(url)
            .and_then(|url| url.join("/console/"))
            .ok()
            .map(String::from),
        _ => None,
    };
    let view = |state| FabbricaView {
        state,
        console: console.clone(),
        items: vec![],
    };
    let needs_consent = || {
        matches!(
            registry.status(UPSTREAM),
            Some(UpstreamStatus::NeedsAuth | UpstreamStatus::Authenticating)
        )
    };
    if registry.status(UPSTREAM) == Some(UpstreamStatus::Disabled) {
        return FabbricaView::default();
    }
    if needs_consent() {
        return view(FabbricaState::Connect);
    }
    match read_items(registry).await {
        Ok(items) => FabbricaView {
            items,
            ..view(FabbricaState::Ready)
        },
        // A call can discover that the token is missing or revoked.
        Err(_) if needs_consent() => view(FabbricaState::Connect),
        Err(reason) => view(FabbricaState::Unknown { reason }),
    }
}

async fn read_items(registry: &UpstreamRegistry) -> Result<Vec<AttentionItem>, String> {
    let tasks = call(registry, "tasks_list", serde_json::json!({})).await?;
    let mut items = Vec::new();
    for task in tasks["tasks"].as_array().into_iter().flatten() {
        // Only an active task has a live execution that can ask for help.
        if task["status"] != "active" {
            continue;
        }
        let Some(task_id) = task["id"].as_str() else {
            continue;
        };
        let name = task["launch"]["name"].as_str().unwrap_or(task_id);
        let help = call(
            registry,
            "help_list",
            serde_json::json!({ "task_id": task_id }),
        )
        .await?;
        for question in help["help"]["questions"].as_array().into_iter().flatten() {
            items.push(item(task_id, name, question));
        }
    }
    let outside = call(registry, "outside_requests_list", serde_json::json!({})).await?;
    for request in outside["requests"].as_array().into_iter().flatten() {
        items.push(outside_item(request));
    }
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}

fn item(task_id: &str, task_name: &str, question: &Value) -> AttentionItem {
    let text = |key: &str| question[key].as_str().unwrap_or_default().to_owned();
    let (kind, label) = if question["kind"] == "permission" {
        (AttentionKind::Approval, "Fabbrica approval")
    } else {
        (AttentionKind::Help, "Fabbrica help request")
    };
    AttentionItem {
        id: format!("fabbrica:{task_id}:{}:{}", text("conversation"), text("id")),
        kind,
        title: text("message"),
        detail: format!("{label} · {task_name}"),
        options: question["options"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|option| OptionLabel {
                id: option["id"].as_str().unwrap_or_default().to_owned(),
                label: option["name"].as_str().unwrap_or_default().to_owned(),
            })
            .collect(),
        // Fabbrica items belong to no local project; every project shows them.
        project: String::new(),
        plan_id: None,
        story: None,
    }
}

fn outside_item(request: &Value) -> AttentionItem {
    let text = |key: &str| request[key].as_str().unwrap_or_default();
    AttentionItem {
        id: format!("fabbrica:outside:{}", text("id")),
        kind: AttentionKind::OutsideRequest,
        title: text("question").to_owned(),
        detail: format!(
            "Fabbrica outside request · {} · {}: {}",
            text("repository"),
            text("author"),
            text("excerpt")
        ),
        options: vec![],
        project: String::new(),
        plan_id: None,
        story: None,
    }
}

/// Answer one outside request through the control MCP, once, then reread so the
/// answered item leaves the view. Fabbrica refuses a second answer; an uncertain
/// result is reported, never retried here.
pub(crate) async fn answer_outside_request(
    state: &AppState,
    request_id: &str,
    decision: &str,
) -> Result<(), String> {
    if !matches!(decision, "accept" | "decline") {
        return Err(format!("unknown decision {decision:?}"));
    }
    let args = serde_json::json!({ "request_id": request_id, "decision": decision });
    let result = call(&state.mcp.upstream_registry, "outside_request_answer", args).await;
    refresh(state).await;
    result.map(|_| ())
}

/// Call one control MCP tool and return its structured result.
async fn call(registry: &UpstreamRegistry, tool: &str, args: Value) -> Result<Value, String> {
    let result = registry
        .proxy_tool_call(&format!("{UPSTREAM}__{tool}"), args)
        .await?;
    let text = result["content"][0]["text"].as_str().unwrap_or_default();
    if result["isError"] == true {
        return Err(format!("{tool}: {text}"));
    }
    match result.get("structuredContent") {
        Some(content) => Ok(content.clone()),
        None => serde_json::from_str(text).map_err(|error| format!("{tool}: {error}")),
    }
}
