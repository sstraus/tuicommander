//! Read-only projection of the items that wait on the user (manual checks and
//! Fabbrica requests, shown in Plans and Stories and the activity bell) plus
//! agent health (shown in the Activity Dashboard). The story store retains all
//! decision authority.
use crate::stories::{Story, StoryStatus, StoryStore};
use crate::{AppState, agent_report::ReportState, mcp_http::types::SessionInfo};
use serde::Serialize;

#[cfg(test)]
#[path = "boss_attention_tests.rs"]
mod tests;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoryContext {
    pub id: String,
    pub title: String,
    pub status: StoryStatus,
}

impl From<&Story> for StoryContext {
    fn from(story: &Story) -> Self {
        Self {
            id: story.id.clone(),
            title: story.title.clone(),
            status: story.status,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct OptionLabel {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AttentionKind {
    ManualCheck,
    /// A Fabbrica help request (elicitation); see `fabbrica_attention.rs`.
    Help,
    /// A Fabbrica permission question.
    Approval,
    /// A Fabbrica outside request in the ask state (a stranger asked for work).
    OutsideRequest,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AttentionItem {
    pub id: String,
    pub kind: AttentionKind,
    pub title: String,
    pub detail: String,
    pub options: Vec<OptionLabel>,
    pub project: String,
    /// Plan that holds the story; the link target in Plans and Stories.
    pub plan_id: Option<String>,
    pub story: Option<StoryContext>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentHealth {
    pub session_id: String,
    pub connection_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub reason: String,
}

#[derive(Default, Debug, Serialize)]
pub(crate) struct BossAttention {
    pub items: Vec<AttentionItem>,
    pub health: Vec<AgentHealth>,
    pub warnings: Vec<String>,
    pub fabbrica: crate::fabbrica_attention::FabbricaView,
}

/// Project a complete fresh snapshot; never retain decided items in this view.
pub(crate) fn snapshot(state: &AppState) -> BossAttention {
    let rows = crate::mcp_http::session::session_rows_including_remote(state);
    let projects = crate::config::local_repository_paths();
    let mut result = BossAttention::default();
    match StoryStore::open() {
        Ok(stories) => {
            for project in projects {
                if let Err(error) = add_project(&mut result, &stories, &project) {
                    result.warnings.push(format!("{project}: {error}"));
                }
            }
        }
        Err(error) => result
            .warnings
            .push(format!("Story state unavailable: {error}")),
    }
    add_health(&mut result, rows);
    crate::fabbrica_attention::add_to(&mut result, state);
    result.items.sort_by(|a, b| a.id.cmp(&b.id));
    result
        .health
        .sort_by(|a, b| (&a.connection_id, &a.session_id).cmp(&(&b.connection_id, &b.session_id)));
    result
}

pub(crate) fn add_project(
    result: &mut BossAttention,
    stories: &StoryStore,
    project: &str,
) -> Result<(), String> {
    for plan in stories.list_plans(project)? {
        for story in stories
            .list_stories(&plan.id)?
            .into_iter()
            .filter(|s| !s.archived)
        {
            let context = StoryContext::from(&story);
            if story.status == StoryStatus::Qa {
                result.items.push(AttentionItem {
                    id: format!("story:{}", story.id),
                    kind: AttentionKind::ManualCheck,
                    title: story.title.clone(),
                    detail: "Manual checks are waiting.".into(),
                    options: vec![],
                    project: project.into(),
                    plan_id: Some(story.plan_id.clone()),
                    story: Some(context),
                });
            }
        }
    }
    Ok(())
}

/// Agent health only. A terminal question is activity, not a waiting item: the
/// dashboard row and the awaiting-input badge already report it.
pub(crate) fn add_health(result: &mut BossAttention, rows: Vec<SessionInfo>) {
    for row in rows {
        let stage = row.agent_report.stage.as_deref();
        let report = row.agent_report.report.as_ref();
        let state = row.state.as_ref();
        let name = row
            .display_name
            .as_deref()
            .or(row.alias.as_deref())
            .unwrap_or(&row.session_id)
            .to_owned();
        let health = if report.is_some_and(|r| r.state == ReportState::Failed) {
            Some(("Failed", "The agent reported a failure.".into()))
        } else if row.agent_report.stalled {
            Some(("Stalled", "The owning host flagged no report, input or output within its configured silence window.".into()))
        } else if let Some(error) = state.and_then(|s| s.last_error.as_ref()) {
            Some(("Failed", error.clone()))
        } else if stage == Some("undelivered_input") {
            Some((
                "Input stalled",
                "Input is pending, failed or delivery is uncertain.".into(),
            ))
        } else if stage == Some("blocked") {
            Some((
                "Blocked",
                "The agent is waiting for a dependency or recovery.".into(),
            ))
        } else {
            None
        };
        if let Some((kind, reason)) = health {
            result.health.push(AgentHealth {
                session_id: row.session_id,
                connection_id: row.connection_id,
                name,
                kind: kind.into(),
                reason,
            });
        }
    }
}

/// Typed report publication wakes both desktop and browser readers without forwarding prose.
pub(crate) fn changed(state: &AppState) {
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::BossAttentionChanged);
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        use tauri::Emitter;
        let _ = app.emit("boss-attention-changed", serde_json::json!({}));
    }
}
