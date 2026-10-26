//! Desktop file-tab metadata shared with the mobile menu. No document contents or disk persistence.
use crate::{AppState, state::AppEvent};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DesktopMarkdownTab {
    pub id: String,
    pub repo_path: String,
    pub file_path: String,
    pub fs_root: Option<String>,
    pub opener_tuic_session: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct OpenMarkdownTab {
    pub id: String,
    pub title: String,
    pub relative_path: String,
    pub absolute_path: String,
    pub fs_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opener_tuic_session: Option<String>,
}

fn normalized(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.trim_end_matches('/').to_owned();
    #[cfg(windows)]
    let path = path.to_lowercase();
    path
}

fn within(path: &str, root: &str) -> bool {
    let path = normalized(path);
    let root = normalized(root);
    !root.is_empty() && (path == root || path.starts_with(&format!("{root}/")))
}

/// The same registered workspace paths used by the session/file browser.
fn roots(config: &serde_json::Value) -> Vec<(String, String)> {
    let mut roots = Vec::new();
    if let Some(repos) = config["repos"].as_object() {
        for (path, repo) in repos {
            if repo["connectionId"].as_str().is_some() {
                continue;
            }
            roots.push((path.clone(), path.clone()));
            for key in ["workspaces", "branches", "declaredWorktrees"] {
                if let Some(workspaces) = repo[key].as_object() {
                    for workspace in workspaces.values() {
                        if let Some(root) =
                            workspace["worktreePath"].as_str().filter(|s| !s.is_empty())
                        {
                            roots.push((path.clone(), root.to_owned()));
                        }
                    }
                }
            }
        }
    }
    roots
}

fn select_tabs(
    tabs: &[DesktopMarkdownTab],
    cwd: &str,
    config: &serde_json::Value,
) -> Result<Vec<OpenMarkdownTab>, String> {
    let roots = roots(config);
    let owner = roots
        .iter()
        .filter(|(_, root)| within(cwd, root))
        .max_by_key(|(_, root)| normalized(root).len())
        .map(|(repo, _)| repo)
        .ok_or("No registered repository contains this session directory")?;
    Ok(tabs
        .iter()
        .filter_map(|tab| {
            if normalized(&tab.repo_path) != normalized(owner) {
                return None;
            }
            let root = tab.fs_root.as_deref().unwrap_or(&tab.repo_path);
            if !roots.iter().any(|(repo, allowed)| {
                normalized(repo) == normalized(owner) && normalized(root) == normalized(allowed)
            }) {
                return None;
            }
            let file = tab.file_path.replace('\\', "/");
            if file.contains('\0') || file.split('/').any(|part| part == "..") {
                return None;
            }
            let absolute = if crate::fs::is_absolute_on_any_platform(&file) {
                file
            } else {
                format!("{}/{}", root.replace('\\', "/").trim_end_matches('/'), file)
            };
            if !within(&absolute, root) {
                return None;
            }
            let relative =
                absolute.get(root.replace('\\', "/").trim_end_matches('/').len() + 1..)?;
            if !relative.to_lowercase().ends_with(".md") {
                return None;
            }
            Some(OpenMarkdownTab {
                id: tab.id.clone(),
                title: relative.rsplit('/').next()?.to_owned(),
                relative_path: relative.to_owned(),
                absolute_path: absolute.clone(),
                fs_root: root.to_owned(),
                opener_tuic_session: tab.opener_tuic_session.clone(),
            })
        })
        .collect())
}

pub(crate) fn publish(state: &AppState, tabs: Vec<DesktopMarkdownTab>) -> Result<(), String> {
    let mut current = state.open_markdown_tabs.write();
    if *current != tabs {
        *current = tabs;
        // Hold the snapshot lock until invalidation is enqueued: every subsequent read sees the change.
        let _ = state.event_bus.send(AppEvent::OpenMarkdownTabsChanged);
        #[cfg(feature = "desktop")]
        if let Some(app) = state.app_handle.read().as_ref() {
            use tauri::Emitter;
            let _ = app.emit("open-markdown-tabs-changed", serde_json::json!({}));
        }
    }
    Ok(())
}

pub(crate) async fn list(
    state: Arc<AppState>,
    cwd: String,
) -> Result<Vec<OpenMarkdownTab>, String> {
    tokio::task::spawn_blocking(move || {
        select_tabs(
            &state.open_markdown_tabs.read(),
            &cwd,
            &crate::config::load_repositories(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) fn sync_open_markdown_tabs(
    state: tauri::State<'_, Arc<AppState>>,
    tabs: Vec<DesktopMarkdownTab>,
) -> Result<(), String> {
    publish(&state, tabs)
}

#[cfg(feature = "desktop")]
#[tauri::command]
pub(crate) async fn list_open_markdown_tabs(
    state: tauri::State<'_, Arc<AppState>>,
    cwd: String,
) -> Result<Vec<OpenMarkdownTab>, String> {
    list(state.inner().clone(), cwd).await
}

#[cfg(test)]
mod tests;
