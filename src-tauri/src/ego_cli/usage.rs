//! Terminal model/context from ego's versioned single-session report.
//! Provider quotas use the existing account usage endpoints, not the ego ledger.

use serde::Serialize;
use serde_json::Value;

use crate::AppState;

#[derive(Debug, Serialize)]
pub(crate) struct EgoUsage {
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    pub usage_agent: Option<&'static str>,
}

pub(crate) fn validate_report(report: &Value) -> Result<(), String> {
    if matches!(report["v"].as_u64(), Some(3..=6)) {
        return Ok(());
    }
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        tracing::warn!(source = "ego_usage", version = ?report["v"], "Unsupported ego usage report version");
    });
    Err("Failed to parse ego usage version (supported: 3, 4, 5, 6)".into())
}

fn project(report: &Value, session: &str) -> Result<EgoUsage, String> {
    validate_report(report)?;
    let row = report["sessions"].as_array().and_then(|rows| {
        rows.iter()
            .find(|row| row["session"].as_str() == Some(session))
    });
    let Some(row) = row else {
        return Ok(EgoUsage {
            text: None,
            tooltip: None,
            usage_agent: None,
        });
    };
    let model = row["model"].as_str().filter(|model| !model.is_empty());
    let identity = model.and_then(|model| model.split_once('/'));
    let usage_agent = match identity.map(|(provider, _)| provider) {
        Some("anthropic") => Some("claude"),
        Some("openai-codex") => Some("codex"),
        _ => None,
    };
    let mut parts = Vec::new();
    if let Some(model) = model {
        parts.push(identity.map_or(model, |(_, short)| short).to_owned());
    }
    if let Some(used) = row["context"]["used_tokens"].as_u64() {
        let capacity = row["context_capacity"]["tokens"]
            .as_u64()
            .filter(|tokens| *tokens > 0);
        parts.push(match capacity {
            Some(capacity) => format!("ctx: {used}/{capacity}"),
            None => format!("ctx: {used}"),
        });
    }
    let text = (!parts.is_empty()).then(|| parts.join(" · "));
    let tooltip = model.map(|model| match &text {
        Some(text) => format!("{model}\n{text}"),
        None => model.to_owned(),
    });
    Ok(EgoUsage {
        text,
        tooltip,
        usage_agent,
    })
}

pub(crate) async fn read(state: &AppState, terminal: &str) -> Result<EgoUsage, String> {
    let Some(source) = crate::session_usage::ego_terminal_source(state, terminal).await? else {
        return Ok(EgoUsage {
            text: None,
            tooltip: None,
            usage_agent: None,
        });
    };
    let (report, _) = source.read(state, terminal).await?;
    project(&report, &source.ego_session)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn get_ego_usage(
    state: tauri::State<'_, std::sync::Arc<AppState>>,
    session_id: String,
) -> Result<EgoUsage, String> {
    read(&state, &session_id).await
}

#[cfg(test)]
mod tests;
