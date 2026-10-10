use crate::AppState;
use crate::pty::resolve_shell;
use std::net::SocketAddr;
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::Emitter;

use super::mcp_transport::insert_optional_value;
use super::mcp_transport::run_blocking_handler;
use super::mcp_transport::to_json_or_error;
use super::mcp_transport_catalogue::CONFIG_ACTIONS;
use super::mcp_transport_catalogue::DEBUG_ACTIONS;
use super::mcp_transport_catalogue::REMOTE_ACTIONS;
use super::mcp_transport_catalogue::REPO_ACTIONS;
use super::mcp_transport_catalogue::UI_ACTIONS;
#[cfg(feature = "dictation")]
use super::mcp_transport_catalogue::VOICE_ACTIONS;
use super::mcp_transport_catalogue::validate_mcp_repo_path;
use super::mcp_transport_peer::PEER_IDENTITY_BIND_LOCK;
use super::mcp_transport_session_agent::handle_session;
use super::mcp_transport_session_agent::require_action;
use super::mcp_transport_session_agent::require_path;
use super::mcp_transport_session_agent::require_string;

pub(super) async fn handle_remote_update(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    let action = match require_action(args, "remote", REMOTE_ACTIONS) {
        Ok(action) => action,
        Err(error) => return error,
    };
    let Some(id) = args["connection_id"].as_str().filter(|id| !id.is_empty()) else {
        return serde_json::json!({"error": "remote action requires connection_id"});
    };
    let result = match action {
        "preview" => crate::remote_update::prepare(state, id).await,
        "update" => {
            let Some(count) = args["confirmed_sessions"]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
            else {
                return serde_json::json!({"error": "remote update requires confirmed_sessions"});
            };
            let Some(digest) = args["expected_sha256"].as_str() else {
                return serde_json::json!({"error": "remote update requires expected_sha256"});
            };
            crate::remote_update::update_and_restart(state, id, count, digest).await
        }
        _ => {
            return serde_json::json!({"error": format!("Unknown remote action {action}. Available: {REMOTE_ACTIONS}")});
        }
    };
    match result {
        Ok(preview) => to_json_or_error(preview),
        Err(error) => serde_json::json!({"error": error}),
    }
}

pub(super) async fn handle_github(
    state: &Arc<AppState>,
    args: &serde_json::Value,
) -> serde_json::Value {
    let action = match require_action(args, "repo", REPO_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "status" => {
            // Cross-repo aggregate: for each workspace repo, return branch/ahead/behind/open PRs
            // Reads from poller cache to avoid fan-out API calls
            let repo_data = crate::config::load_repositories();
            let repo_order = repo_data
                .get("repoOrder")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut results: Vec<serde_json::Value> = Vec::new();
            for path_val in &repo_order {
                let Some(path) = path_val.as_str() else {
                    continue;
                };
                let info = crate::git::get_repo_info_cached(state, path);
                if !info.is_git_repo {
                    continue;
                }
                let gh = crate::github::get_github_status_cached(state, path);
                let cached_prs: Vec<crate::github::BranchPrStatus> = state
                    .git_cache
                    .github_status
                    .get(path)
                    .map(|a| (*a).clone())
                    .unwrap_or_default();
                let open_prs = cached_prs.len();
                let failing_ci = cached_prs.iter().filter(|p| p.checks.failed > 0).count();
                results.push(serde_json::json!({
                    "path": path,
                    "branch": info.branch,
                    "status": info.status,
                    "ahead": gh.ahead,
                    "behind": gh.behind,
                    "open_prs": open_prs,
                    "failing_ci": failing_ci,
                }));
            }
            serde_json::json!(results)
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'repo'. Available: {}", other, REPO_ACTIONS
        )}),
    }
}

/// Create a PTY session in the given directory, returning the session ID.
/// Reuses the same setup as `session action=create` but with fixed defaults.
fn create_session_in_dir(state: &Arc<AppState>, cwd: &str) -> Result<String, String> {
    let shell = resolve_shell(None);
    super::session::spawn_pty_session(
        state.clone(),
        shell,
        Some(cwd.to_string()),
        24,
        80,
        None,
        super::session::RequestedIdentity::default(),
    )
    .map_err(|(_, body)| {
        body.0
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("spawn failed")
            .to_string()
    })
}

pub(super) fn worktree_remove_success_response(
    branch_delete_warning: Option<String>,
    removal_rule: &str,
) -> serde_json::Value {
    let mut response = serde_json::json!({"ok": true, "removal_rule": removal_rule});
    insert_optional_value(
        response
            .as_object_mut()
            .expect("worktree remove response is an object"),
        "branch_delete_warning",
        branch_delete_warning.map(serde_json::Value::String),
    );
    response
}

/// A detached checkout has no branch to name: it is removed by its own path,
/// under the same safety verdict as the orphan sweep.
async fn remove_detached_checkout(
    state: &Arc<AppState>,
    repo_path: String,
    worktree_path: String,
) -> serde_json::Value {
    let state = state.clone();
    match tokio::task::spawn_blocking(move || {
        crate::worktree::remove_orphan_checkout(&state, &repo_path, &worktree_path, true, &[])
    })
    .await
    {
        Ok(Ok(())) => serde_json::json!({"ok": true}),
        Ok(Err(error)) => serde_json::json!({"error": error}),
        Err(error) => {
            serde_json::json!({"error": format!("orphan removal task failed: {error}")})
        }
    }
}

#[cfg(test)]
pub(super) async fn handle_worktree(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    is_claude_code: bool,
) -> serde_json::Value {
    handle_worktree_with_caller(state, args, is_claude_code, None).await
}

async fn handle_worktree_with_caller(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    is_claude_code: bool,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "repo", REPO_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "branch_integrations" | "branch_integration" => {
            let path = match require_path(args, action) {
                Ok(path) => path,
                Err(error) => return error,
            };
            if let Err(error) = validate_mcp_repo_path(&path) {
                return error;
            }
            let single = action == "branch_integration";
            let branch = args["branch"].as_str().map(str::to_owned);
            if single && branch.is_none() {
                return serde_json::json!({"error":"Action 'branch_integration' requires 'branch' parameter"});
            }
            match tokio::task::spawn_blocking(move || {
                let repo = std::path::Path::new(&path);
                if let Some(branch) = branch.filter(|_| single) {
                    crate::worktree::branch_integration(repo, &branch).map(to_json_or_error)
                } else {
                    crate::worktree::branch_integrations(repo).map(to_json_or_error)
                }
            })
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(error)) => serde_json::json!({"error":error}),
                Err(error) => {
                    serde_json::json!({"error":format!("branch integration task failed: {error}")})
                }
            }
        }
        "worktree_list" => {
            let path = match require_path(args, "worktree_list") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) {
                return e;
            }
            let mut wts = match crate::worktree::get_worktree_paths(path.clone()) {
                Ok(wts) => wts,
                Err(e) => return serde_json::json!({"error": e}),
            };
            // Same source as the sidebar's lifecycleStatus (get_repo_summary /
            // get_repo_diff_stats): inspect_workspace_lifecycle, fanned out per
            // worktree on the blocking pool so the async worker isn't parked on
            // the git subprocesses each inspection runs.
            let mut lifecycle_handles = Vec::with_capacity(wts.len());
            for workspace_id in wts.keys().cloned().collect::<Vec<_>>() {
                let base_repo = path.clone();
                lifecycle_handles.push(tokio::task::spawn_blocking(move || {
                    let lifecycle = crate::worktree::inspect_workspace_lifecycle(
                        std::path::Path::new(&base_repo),
                        &workspace_id,
                    );
                    (workspace_id, lifecycle)
                }));
            }
            for handle in lifecycle_handles {
                match handle.await {
                    Ok((workspace_id, lifecycle)) => {
                        if let Some(workspace) = wts.get_mut(&workspace_id) {
                            workspace.lifecycle_status = Some(lifecycle);
                        }
                    }
                    Err(error) => {
                        return serde_json::json!({
                            "error": format!("worktree lifecycle task failed: {error}")
                        });
                    }
                }
            }
            to_json_or_error(wts)
        }
        "worktree_lifecycle" => {
            let path = match require_path(args, "worktree_lifecycle") {
                Ok(path) => path,
                Err(error) => return error,
            };
            if let Err(error) = validate_mcp_repo_path(&path) {
                return error;
            }
            let workspace_id = match args["branch"].as_str() {
                Some(id) => id.to_owned(),
                None => {
                    return serde_json::json!({"error": "Action 'worktree_lifecycle' requires 'branch' parameter"});
                }
            };
            let state = Arc::clone(state);
            match tokio::task::spawn_blocking(move || {
                crate::worktree::inspect_worktree_removal(
                    &state,
                    std::path::Path::new(&path),
                    &workspace_id,
                )
            })
            .await
            {
                Ok(preview) => to_json_or_error(preview),
                Err(error) => {
                    serde_json::json!({"error": format!("worktree lifecycle task failed: {error}")})
                }
            }
        }
        "worktree_create" => {
            let path = match require_path(args, "worktree_create") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) {
                return e;
            }
            let branch = args["branch"].as_str().map(|s| s.to_string());
            let base_ref = args["base_ref"].as_str().map(|s| s.to_string());

            // Generate a branch name if not specified
            let branch_name = branch.unwrap_or_else(|| {
                let existing: Vec<String> = match crate::worktree::get_worktree_paths(path.clone())
                {
                    // Branch names, so read the records — the keys are workspace ids.
                    Ok(wts) => wts.into_values().map(|w| w.branch).collect(),
                    Err(e) => {
                        tracing::warn!("Failed to list worktrees for name generation: {e}");
                        vec![]
                    }
                };
                crate::worktree::generate_worktree_name(&existing)
            });

            match super::worktree_routes::create_worktree_shared(
                state,
                path.clone(),
                branch_name,
                base_ref,
                resolve_mcp_origin_pty(state, mcp_session_id),
                args["spawn_session"].as_bool().unwrap_or(false),
            )
            .await
            {
                Ok(created) => {
                    let wt_path = created.path;
                    let branch_name = created.branch;
                    let mut response = serde_json::json!({
                        "worktree_path": &wt_path,
                        // The id `action=worktree_remove` asks for.
                        "workspace_id": &created.workspace_id,
                        "branch": &branch_name,
                        // The same value the HTTP route returns, not a second
                        // rendering of it: one payload, two carriers, so the two
                        // transports cannot describe the same workspace
                        // differently (#734-ca73).
                        "instructions": &created.instructions,
                    });
                    // Optionally spawn a PTY session in the new worktree
                    if args["spawn_session"].as_bool().unwrap_or(false) {
                        match create_session_in_dir(state, &wt_path) {
                            Ok(sid) => {
                                response["session_id"] = serde_json::json!(sid);
                            }
                            Err(e) => {
                                response["session_error"] = serde_json::json!(e);
                            }
                        }
                    }
                    if let Some(setup_script) = created.setup_script {
                        response["setup_script"] = setup_script;
                    }
                    if let Some(setup_script_error) = created.setup_script_error {
                        response["setup_script_error"] = setup_script_error;
                    }
                    // Add structured hint for Claude Code clients to spawn a subagent in the worktree
                    if is_claude_code {
                        // Sanitize branch name to prevent prompt injection via backticks/newlines
                        let safe_branch = branch_name.replace('`', "'").replace('\n', " ");
                        response["cc_agent_hint"] = serde_json::json!({
                            "worktree_path": wt_path,
                            "suggested_prompt": format!(
                                "Work in the worktree at `{}`. Use absolute paths for ALL file operations \
                                (Read, Edit, Glob, Grep). For git commands, use `cd {} && git ...`. \
                                The branch is `{}`.",
                                wt_path, wt_path, safe_branch,
                            )
                        });
                    }
                    response
                }
                Err((_status, body)) => body.0,
            }
        }
        "worktree_remove" => {
            let path = match require_path(args, "worktree_remove") {
                Ok(p) => p,
                Err(e) => return e,
            };
            if let Err(e) = validate_mcp_repo_path(&path) {
                return e;
            }
            if let Some(worktree_path) = args["worktree_path"].as_str() {
                return remove_detached_checkout(state, path, worktree_path.to_owned()).await;
            }
            let workspace_id = match args["branch"].as_str() {
                Some(b) => b.to_string(),
                None => {
                    return serde_json::json!({"error": "Action 'worktree_remove' requires 'branch' parameter"});
                }
            };
            let force = args["force"].as_bool().unwrap_or(false);
            let delete_branch = args["delete_branch"].as_bool().unwrap_or(!force);
            let override_lock = args["override_lock"].as_bool().unwrap_or(false);
            let expected_fingerprint = args["expected_fingerprint"].as_str().map(str::to_owned);
            if force && expected_fingerprint.is_none() {
                return serde_json::json!({"error": "force requires expected_fingerprint from worktree_lifecycle after user confirmation"});
            }
            let path_for_remove = path.clone();
            let workspace_id_for_remove = workspace_id.clone();
            let preview_state = Arc::clone(state);
            let result = tokio::task::spawn_blocking(move || {
                let warnings = crate::worktree::inspect_worktree_removal(
                    &preview_state,
                    std::path::Path::new(&path_for_remove),
                    &workspace_id_for_remove,
                )
                .warnings;
                let archive = crate::worktree::resolve_archive_script(&path_for_remove);
                let outcome = crate::worktree::remove_worktree_by_workspace_id_with_confirmation(
                    &path_for_remove,
                    &workspace_id_for_remove,
                    delete_branch,
                    archive.as_deref(),
                    force,
                    override_lock,
                    expected_fingerprint.as_deref(),
                )?;
                Ok::<_, String>((outcome, warnings))
            })
            .await;
            match result {
                Ok(Ok((outcome, warnings))) => {
                    state.notify_worktree_removed(crate::state::WorktreeRemovedPayload {
                        repo_path: path.clone(),
                        workspace_id: workspace_id.clone(),
                        branch: outcome.branch,
                    });
                    let mut response = worktree_remove_success_response(
                        outcome.branch_delete_warning,
                        &outcome.removal_rule,
                    );
                    response["warnings"] = serde_json::json!(warnings);
                    response
                }
                Ok(Err(e)) => serde_json::json!({"error": e}),
                Err(e) => serde_json::json!({
                    "error": format!("worktree removal task failed to complete: {e}")
                }),
            }
        }
        "orphan_cleanup_answer" => {
            let path = match require_path(args, "orphan_cleanup_answer") {
                Ok(path) => path,
                Err(error) => return error,
            };
            if let Err(error) = validate_mcp_repo_path(&path) {
                return error;
            }
            let remove = match args["decision"].as_str() {
                Some("remove") => true,
                Some("keep") => false,
                _ => return serde_json::json!({"error": "decision must be remove or keep"}),
            };
            let state = state.clone();
            match tokio::task::spawn_blocking(move || {
                crate::worktree::answer_orphan_cleanup_internal(&state, &path, remove)
            })
            .await
            {
                Ok(Ok(())) => serde_json::json!({"ok": true}),
                Ok(Err(error)) => serde_json::json!({"error": error}),
                Err(error) => {
                    serde_json::json!({"error": format!("orphan cleanup answer task failed: {error}")})
                }
            }
        }
        "branch_delete" => {
            let path = match require_path(args, "branch_delete") {
                Ok(path) => path,
                Err(error) => return error,
            };
            if let Err(error) = validate_mcp_repo_path(&path) {
                return error;
            }
            let Some(branch) = args["branch"].as_str().map(str::to_owned) else {
                return serde_json::json!({"error":"Action 'branch_delete' requires 'branch' parameter"});
            };
            match tokio::task::spawn_blocking(move || {
                crate::worktree::delete_integrated_local_branch(&path, &branch)
            })
            .await
            {
                Ok(Ok(deleted)) if deleted.proof == "archived" => serde_json::json!({
                    "ok":true,
                    "proof":deleted.proof,
                    "archive_ref":deleted.archive_ref
                }),
                Ok(Ok(deleted)) => serde_json::json!({"ok":true,"proof":deleted.proof}),
                Ok(Err(error)) => serde_json::json!({"error":error}),
                Err(error) => {
                    serde_json::json!({"error":format!("branch deletion task failed: {error}")})
                }
            }
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'repo'. Available: {}", other, REPO_ACTIONS
        )}),
    }
}

pub(super) fn handle_config(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
) -> serde_json::Value {
    let action = match require_action(args, "config", CONFIG_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "get" => {
            let config = state.config.read().clone();
            let mut json = to_json_or_error(config);
            if let Some(services) = json.pointer_mut("/services") {
                if let Some(auth) = services.pointer_mut("/auth")
                    && let Some(o) = auth.as_object_mut()
                {
                    o.remove("password_hash");
                    o.remove("session_token");
                }
                if let Some(push) = services.pointer_mut("/push")
                    && let Some(o) = push.as_object_mut()
                {
                    o.remove("vapid_private_key");
                }
                if let Some(relay) = services.pointer_mut("/relay")
                    && let Some(o) = relay.as_object_mut()
                {
                    o.remove("token");
                }
            }
            json
        }
        "save" => {
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "Config save is restricted to localhost connections"});
            }
            let request: crate::config::ConfigSaveRequest<crate::config::AppConfig> =
                match serde_json::from_value(args.clone()) {
                    Ok(request) => request,
                    Err(error) => {
                        return serde_json::json!({"error": format!("Invalid config save request: {error}")});
                    }
                };
            match crate::config::commit_config_save(state, request.base, request.config) {
                Ok(effects) => {
                    if effects.tools_changed {
                        let _ = state.mcp.tools_changed.send(());
                    }
                    if effects.server_changed {
                        super::restart_after_server_settings_change(
                            state,
                            "remote-access configuration changed over MCP",
                        );
                    }
                    serde_json::json!({"ok": true})
                }
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        "list_prompts" => {
            let lib = crate::config::load_prompt_library();
            serde_json::json!({
                "prompts": lib.prompts.iter().map(|p| serde_json::json!({
                    "id": p.id, "label": p.label, "pinned": p.pinned,
                })).collect::<Vec<_>>()
            })
        }
        "load_prompt" => {
            let id = match require_string(args, "id") {
                Ok(s) => s,
                Err(e) => return e,
            };
            let lib = crate::config::load_prompt_library();
            match lib.prompts.iter().find(|p| p.id == id) {
                Some(p) => to_json_or_error(p.clone()),
                None => serde_json::json!({"error": format!("Prompt not found: {id}")}),
            }
        }
        "save_prompt" => {
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "Prompt save is restricted to localhost connections"});
            }
            let id = match require_string(args, "id") {
                Ok(s) => s.to_string(),
                Err(e) => return e,
            };
            let label = match require_string(args, "label") {
                Ok(s) => s.to_string(),
                Err(e) => return e,
            };
            let text = match require_string(args, "text") {
                Ok(s) => s.to_string(),
                Err(e) => return e,
            };
            let pinned = args
                .get("pinned")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let mut lib = crate::config::load_prompt_library();
            let base = lib.clone();
            if let Some(existing) = lib.prompts.iter_mut().find(|p| p.id == id) {
                existing.label = label;
                existing.text = text;
                existing.pinned = pinned;
            } else {
                lib.prompts.push(crate::config::PromptEntry {
                    id,
                    label,
                    text,
                    pinned,
                });
            }
            match crate::config::save_prompt_library(base, lib) {
                Ok(()) => serde_json::json!({"ok": true}),
                Err(e) => serde_json::json!({"error": e}),
            }
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'config'. Available: {}", other, CONFIG_ACTIONS
        )}),
    }
}

fn handle_debug(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "debug", DEBUG_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "agent_detection" => {
            let session_ids: Vec<String> = if let Some(sid) = args["session_id"].as_str() {
                vec![sid.to_string()]
            } else {
                state
                    .session_maps
                    .sessions
                    .iter()
                    .map(|e| e.key().clone())
                    .collect()
            };
            let results: Vec<serde_json::Value> = session_ids.iter().map(|sid| {
                let entry = match state.session_maps.sessions.get(sid) {
                    Some(e) => e,
                    None => return serde_json::json!({ "error": "session not found", "session_id": sid }),
                };
                let session = entry.value().lock();
                #[cfg(not(windows))]
                {
                    let raw_fd = session.master.as_raw_fd();
                    let pgid = session.master.process_group_leader();
                    let name = pgid.and_then(|p| crate::pty::process_name_from_pid(p as u32));
                    let classified = name.as_deref().and_then(crate::pty::classify_agent);
                    serde_json::json!({
                        "session_id": sid,
                        "master_raw_fd": raw_fd,
                        "process_group_leader": pgid,
                        "process_name": name,
                        "classified_agent": classified,
                        "child_pid": session._child.process_id(),
                    })
                }
                #[cfg(windows)]
                {
                    let child_pid = session._child.process_id();
                    let leaf = child_pid.and_then(crate::pty::deepest_descendant_pid);
                    let name = leaf.and_then(crate::pty::process_name_from_pid);
                    let classified = name.as_deref().and_then(crate::pty::classify_agent);
                    serde_json::json!({
                        "session_id": sid,
                        "child_pid": child_pid,
                        "leaf_pid": leaf,
                        "process_name": name,
                        "classified_agent": classified,
                    })
                }
            }).collect();
            serde_json::json!(results)
        }
        "logs" => {
            let level_filter = args["level"].as_str();
            let source_filter = args["source"].as_str();
            let limit = args["limit"].as_u64().unwrap_or(50) as usize;
            let buf = state.log_buffer.lock();
            let all = buf.get_entries(0);
            let filtered: Vec<_> = all
                .into_iter()
                .filter(|e| level_filter.is_none_or(|l| e.level == l))
                .filter(|e| source_filter.is_none_or(|s| e.source == s))
                .collect();
            let start = filtered.len().saturating_sub(limit);
            serde_json::json!(filtered[start..])
        }
        "sessions" => {
            let sessions: Vec<serde_json::Value> = state
                .session_maps
                .sessions
                .iter()
                .map(|entry| {
                    let sid = entry.key().clone();
                    let session = entry.value().lock();
                    #[cfg(not(windows))]
                    let pgid = session.master.process_group_leader();
                    #[cfg(windows)]
                    let pgid = session._child.process_id();
                    #[cfg(not(windows))]
                    let process_name =
                        pgid.and_then(|p| crate::pty::process_name_from_pid(p as u32));
                    #[cfg(windows)]
                    let process_name = pgid.and_then(crate::pty::process_name_from_pid);
                    serde_json::json!({
                        "session_id": sid,
                        "cwd": session.cwd,
                        "child_pid": session._child.process_id(),
                        "foreground_pgid": pgid,
                        "foreground_process": process_name,
                    })
                })
                .collect();
            serde_json::json!(sessions)
        }
        "invoke_js" => {
            // invoke_js executes arbitrary JS in the WebView — must be routed through
            // handle_debug_unified which enforces the loopback guard.
            serde_json::json!({"error": "invoke_js must be called via the debug tool (loopback-only)"})
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'debug'. Available: {}", other, DEBUG_ACTIONS
        )}),
    }
}

fn handle_repo_listing(state: &Arc<AppState>, args: &serde_json::Value) -> serde_json::Value {
    let action = match require_action(args, "repo", REPO_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "list" => {
            let repo_data = crate::config::load_repositories();
            let repos = repo_data
                .get("repos")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let repo_order = repo_data
                .get("repoOrder")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let groups = repo_data
                .get("groups")
                .cloned()
                .unwrap_or(serde_json::json!({}));

            // Build group membership lookup: repo_path → group name
            let mut repo_group: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            if let Some(groups_obj) = groups.as_object() {
                for (_gid, group) in groups_obj {
                    let group_name = group["name"].as_str().unwrap_or("").to_string();
                    if let Some(order) = group["repoOrder"].as_array() {
                        for path_val in order {
                            if let Some(path) = path_val.as_str() {
                                repo_group.insert(path.to_string(), group_name.clone());
                            }
                        }
                    }
                }
            }

            let mut results: Vec<serde_json::Value> = Vec::new();
            for path_val in &repo_order {
                let path = match path_val.as_str() {
                    Some(p) => p,
                    None => continue,
                };
                let repo_entry = repos.get(path);
                let display_name = repo_entry
                    .and_then(|r| r["displayName"].as_str())
                    .unwrap_or("")
                    .to_string();

                let info = crate::git::get_repo_info_cached(state, path);
                let worktrees = crate::worktree::get_worktree_paths_cached(state, path);

                let mut entry = serde_json::json!({
                    "path": path,
                    "name": if display_name.is_empty() { &info.name } else { &display_name },
                    "branch": info.branch,
                    "status": info.status,
                    "is_git_repo": info.is_git_repo,
                });
                // Include ahead/behind for git repos with remotes
                if info.is_git_repo {
                    let gh = crate::github::get_github_status_cached(state, path);
                    if gh.has_remote {
                        entry["ahead"] = serde_json::json!(gh.ahead);
                        entry["behind"] = serde_json::json!(gh.behind);
                    }
                }
                if let Some(group_name) = repo_group.get(path) {
                    entry["group"] = serde_json::json!(group_name);
                }
                if !worktrees.is_empty() {
                    entry["worktrees"] = to_json_or_error(&worktrees);
                }
                results.push(entry);
            }
            serde_json::json!(results)
        }
        "active" => {
            let repo_data = crate::config::load_repositories();
            let active_path = match repo_data.get("activeRepoPath").and_then(|v| v.as_str()) {
                Some(p) => p.to_string(),
                None => return serde_json::json!({"active": null}),
            };

            let info = crate::git::get_repo_info_cached(state, &active_path);

            // Find group membership
            let groups = repo_data
                .get("groups")
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let mut group_name: Option<String> = None;
            if let Some(groups_obj) = groups.as_object() {
                for (_gid, group) in groups_obj {
                    if let Some(order) = group["repoOrder"].as_array()
                        && order.iter().any(|p| p.as_str() == Some(&active_path))
                    {
                        group_name = group["name"].as_str().map(|s| s.to_string());
                        break;
                    }
                }
            }

            let mut result = serde_json::json!({
                "path": active_path,
                "name": info.name,
                "branch": info.branch,
                "status": info.status,
            });
            if let Some(gn) = group_name {
                result["group"] = serde_json::json!(gn);
            }
            result
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'repo'. Available: {}", other, REPO_ACTIONS
        )}),
    }
}

/// The TUIC session behind an MCP call, when the caller is bound to a PTY.
/// `None` for an unbound caller — never guess one, a wrong id would send a
/// toast click to somebody else's terminal.
pub(super) fn resolve_mcp_origin_session(
    state: &Arc<AppState>,
    mcp_session_id: Option<&str>,
) -> Option<String> {
    mcp_session_id.and_then(|mcp_sid| state.mcp.to_session.get(mcp_sid).map(|s| s.value().clone()))
}

/// The live PTY behind this MCP connection, keyed as `sessions` keys it.
///
/// Not the peer id: a hand-opened tab registers under its `$TUIC_SESSION`
/// while its PTY carries the UUID `create_pty` minted, and anything bound to a
/// terminal (hands-free, a progress entry) holds the PTY key.
pub(super) fn resolve_mcp_origin_pty(
    state: &Arc<AppState>,
    mcp_session_id: Option<&str>,
) -> Option<String> {
    resolve_mcp_origin_session(state, mcp_session_id)
        .and_then(|peer| state.live_pty_for_peer(&peer))
}

pub(super) fn resolve_mcp_origin_repo_path(
    state: &Arc<AppState>,
    mcp_session_id: Option<&str>,
) -> Option<String> {
    let caller_tuic = resolve_mcp_origin_session(state, mcp_session_id);
    caller_tuic
        .as_ref()
        .and_then(|tuic| {
            state
                .peer_agents
                .get(tuic)
                .and_then(|p| p.project.clone())
                .or_else(|| {
                    // Resolution, not equality: `sessions` is keyed by the UUID
                    // `create_pty` minted, while a peer is keyed by its
                    // `$TUIC_SESSION`. The two are the same value only for a
                    // spawn-registered child, so reading `sessions` under the
                    // peer id answered "no cwd" for every hand-opened tab — and
                    // `progress` then refused the report as `project_required`
                    // with the cwd sitting in the map under the other key.
                    state
                        .live_pty_for_peer(tuic)
                        .and_then(|pty| {
                            state
                                .session_maps
                                .sessions
                                .get(&pty)
                                .map(|s| s.lock().cwd.clone())
                        })
                        .flatten()
                })
        })
        .or_else(|| {
            mcp_session_id.and_then(|sid| {
                state
                    .mcp
                    .sessions
                    .get(sid)
                    .and_then(|m| m.repo_path.clone())
            })
        })
}

fn parse_progress_report_input(
    args: &serde_json::Value,
) -> Result<crate::progress::ProgressReportInput, serde_json::Value> {
    let kind = args["type"]
        .as_str()
        .ok_or_else(|| serde_json::json!({"error": "progress requires 'type'"}))
        .and_then(|value| {
            crate::progress::ProgressKind::parse_reportable(value)
                .map_err(|error| serde_json::json!({"error": error}))
        })?;
    let text = args["text"]
        .as_str()
        .ok_or_else(|| serde_json::json!({"error": "progress requires 'text'"}))?
        .to_string();
    Ok(crate::progress::ProgressReportInput {
        kind,
        text,
        step: args["step"].as_str().map(str::to_string),
    })
}

/// The agent type of the peer that is reporting, for the per-agent half of
/// `progress_tracking`. It comes from the peer's live PTY session, which is the
/// only place an agent type is recorded — the MCP session knows a client name,
/// and a client name is not an agent.
fn resolve_mcp_origin_agent_type(
    state: &Arc<AppState>,
    mcp_session_id: Option<&str>,
) -> Option<String> {
    let peer = resolve_mcp_origin_session(state, mcp_session_id)?;
    let pty = state.live_pty_for_peer(&peer)?;
    state
        .session_maps
        .session_states
        .get(&pty)
        .and_then(|entry| entry.agent_type.clone())
}

/// Speak into the conversation this caller is the target of.
///
/// The binding is the whole security property, and it is checked in
/// `dictation::commands` rather than here: a model may drive only the
/// hands-free conversation armed for its own terminal. What this function
/// contributes is the *identity* — an unbound caller has no terminal, so it
/// can never match a binding and is told speech is unavailable rather than
/// being allowed to speak into whichever conversation happens to be armed.
#[cfg(feature = "dictation")]
pub(super) fn handle_voice(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    use crate::dictation::{DictationState, commands as dictation};
    use tauri::Manager;

    let action = match require_action(args, "voice", VOICE_ACTIONS) {
        Ok(action) => action,
        Err(error) => return error,
    };

    // Identity first, before the app is even consulted. No session header means
    // no terminal, which means no conversation — and that answer does not
    // depend on how far along startup is. Saying so here also keeps the caller
    // off the owner's own control surface: that path exists for the user's UI,
    // and handing it to a model would let any MCP client speak into somebody
    // else's conversation.
    let Some(caller) = resolve_mcp_origin_pty(state, mcp_session_id) else {
        return serde_json::json!({"error":
            "This connection is not bound to a terminal, so there is no conversation to speak into"
        });
    };
    let caller = dictation::Caller::Model(&caller);

    let app_handle = state.app_handle.read();
    let Some(app) = app_handle.as_ref() else {
        return serde_json::json!({"error": "TUICommander is still starting up"});
    };
    let dictation_state = app.state::<DictationState>();

    match action {
        "speak" => {
            let Some(text) = args["text"].as_str() else {
                return serde_json::json!({"error": "Missing 'text' for action=speak"});
            };
            let turn = args["turn"].as_u64();
            match dictation::speak(&dictation_state, caller, text, turn) {
                Ok(reply) => to_json_or_error(reply),
                Err(error) => {
                    serde_json::json!({"available": false, "unavailableReason": error, "error": error})
                }
            }
        }
        "stop" => match dictation::stop_speaking(&dictation_state, caller) {
            Ok(status) => to_json_or_error(status),
            Err(error) => {
                serde_json::json!({"available": false, "unavailableReason": error, "error": error})
            }
        },
        // Status answers whether this caller *could* speak, so it checks the
        // binding too: a model bound elsewhere must be told it is not the
        // target rather than shown another conversation's queue.
        "status" => match dictation::speech_status_for(
            &dictation_state,
            caller,
            args["utterance_id"].as_str(),
        ) {
            Ok(status) => to_json_or_error(status),
            Err(error) => {
                serde_json::json!({"available": false, "unavailableReason": error, "error": error})
            }
        },
        other => serde_json::json!({"error": format!(
            "Unknown voice action '{other}'. Available: {VOICE_ACTIONS}"
        )}),
    }
}

/// Voice needs a microphone, a speaker and the dictation stack, none of which
/// the headless binary builds. Reported as unavailable rather than as an
/// unknown tool, so a model reads one consistent reason on both builds.
#[cfg(not(feature = "dictation"))]
pub(super) fn handle_voice(
    state: &Arc<AppState>,
    _args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    if resolve_mcp_origin_pty(state, mcp_session_id).is_none() {
        return serde_json::json!({"error":
            "This connection is not bound to a terminal, so there is no conversation to speak into"
        });
    }
    serde_json::json!({
        "available": false,
        "unavailable_reason": "This TUICommander build has no audio support",
    })
}

pub(super) fn handle_story(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let Some(pty) = resolve_mcp_origin_pty(state, mcp_session_id) else {
        return serde_json::json!({"error": "story requires a bound live managed session"});
    };
    let Some(project) = crate::progress::project_for_session(state, &pty) else {
        return serde_json::json!({"error": "calling session has no registered project"});
    };
    let action: crate::stories::StoryAction = match serde_json::from_value(args["input"].clone()) {
        Ok(value) => value,
        Err(error) => return serde_json::json!({"error": format!("invalid story action: {error}")}),
    };
    to_json_or_error(crate::stories::story_action_for_session(
        state,
        &project,
        action,
        Some(&pty),
    ))
}

pub(super) fn handle_workflow_run(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let Some(pty) = resolve_mcp_origin_pty(state, mcp_session_id) else {
        return serde_json::json!({"error": "workflow_run requires a bound live managed session"});
    };
    let Some(project) = crate::progress::project_for_session(state, &pty) else {
        return serde_json::json!({"error": "calling session has no registered project"});
    };
    let action = match serde_json::from_value(args["input"].clone()) {
        Ok(value) => value,
        Err(error) => {
            return serde_json::json!({"error": format!("invalid workflow run action: {error}")});
        }
    };
    to_json_or_error(crate::workflows::run_action_with_events(
        state, &project, action,
    ))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowStoryCreateInput {
    pub(super) run_id: String,
    proposal_key: String,
    pub(super) story: crate::stories::NewStory,
}

pub(super) fn handle_workflow_story_create(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let Some(pty) = resolve_mcp_origin_pty(state, mcp_session_id) else {
        return serde_json::json!({"error": "workflow story creation requires a bound live managed session"});
    };
    let Some(project) = crate::progress::project_for_session(state, &pty) else {
        return serde_json::json!({"error": "calling session has no registered project"});
    };
    let input: WorkflowStoryCreateInput = match serde_json::from_value(args["input"].clone()) {
        Ok(input) => input,
        Err(error) => {
            return serde_json::json!({"error": format!("invalid workflow story proposal: {error}")});
        }
    };
    let result = (|| -> Result<serde_json::Value, String> {
        let owner = crate::progress::resolve_owning_project(Some(&project))?;
        let store = crate::workflows::RunStore::open()?;
        let before = store.snapshot(&input.run_id)?;
        if before.project != owner.to_string_lossy() {
            return Err("workflow run does not belong to calling session's project".into());
        }
        let story = store.create_story_from_coordinator(
            &input.run_id,
            &pty,
            &input.proposal_key,
            input.story,
        )?;
        let after = store.snapshot(&input.run_id)?;
        if after.sequence > before.sequence {
            crate::workflows::emit_run_changed(state, &after.project, &after.id, after.sequence);
        }
        Ok(serde_json::json!({"story": story, "runId": after.id, "sequence": after.sequence}))
    })();
    result.unwrap_or_else(|error| serde_json::json!({"error": error}))
}

pub(super) fn handle_workflow_report(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let Some(pty) = resolve_mcp_origin_pty(state, mcp_session_id) else {
        return serde_json::json!({"error": "workflow report requires a bound live managed session"});
    };
    let Some(project) = crate::progress::project_for_session(state, &pty) else {
        return serde_json::json!({"error": "calling session has no registered project"});
    };
    let report: crate::workflows::AttemptReport =
        match serde_json::from_value(args["input"].clone()) {
            Ok(value) => value,
            Err(error) => {
                return serde_json::json!({"error": format!("invalid workflow report: {error}")});
            }
        };
    let store = match crate::workflows::RunStore::open() {
        Ok(store) => store,
        Err(error) => return serde_json::json!({"error": error}),
    };
    let snapshot = match store.snapshot(&report.run_id) {
        Ok(snapshot) => snapshot,
        Err(error) => return serde_json::json!({"error": error}),
    };
    let owner = match crate::progress::resolve_owning_project(Some(&project)) {
        Ok(owner) => owner,
        Err(error) => return serde_json::json!({"error": error}),
    };
    if snapshot.project != owner.to_string_lossy() {
        return serde_json::json!({"error": "workflow run does not belong to calling session's project"});
    }
    let reported_story_id = report.story_id.clone();
    match store.report_bound_agent(report, &pty) {
        Ok(receipt) => {
            if receipt.sequence > snapshot.sequence {
                crate::workflows::emit_run_changed(
                    state,
                    &receipt.snapshot.project,
                    &receipt.snapshot.id,
                    receipt.sequence,
                );
            }
            if receipt.sequence > snapshot.sequence
                && reported_story_id != snapshot.plan_id
                && matches!(
                    &receipt.event.kind,
                    crate::workflows::RunEventKind::AttemptReported { .. }
                )
                && let Ok(Some(coordinator_session)) =
                    crate::workflows::active_coordinator_session(&receipt.snapshot)
            {
                match state.resolve_peer_ref_checked(&coordinator_session) {
                    Ok(Some(peer)) => {
                        queue_workflow_coordinator_wake(
                            state,
                            &peer,
                            &pty,
                            &receipt.snapshot.id,
                            &reported_story_id,
                            receipt.sequence,
                        );
                    }
                    Ok(None) => {}
                    Err(error) => tracing::warn!(
                        "workflow coordinator wake skipped for run {}: {error}",
                        receipt.snapshot.id
                    ),
                }
            }
            to_json_or_error(receipt)
        }
        Err(error) => serde_json::json!({"error": error}),
    }
}

/// Inbox mail is a cursor into the durable run log, never a second result record.
pub(super) fn queue_workflow_coordinator_wake(
    state: &AppState,
    recipient: &str,
    reporter: &str,
    run_id: &str,
    story_id: &str,
    sequence: i64,
) -> bool {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let message = crate::state::AgentMessage {
        id: format!("workflow-event:{run_id}:{sequence}"),
        from_tuic_session: reporter.to_owned(),
        from_name: "workflow".into(),
        content: serde_json::json!({
            "type": "workflow_event",
            "runId": run_id,
            "storyId": story_id,
            "sequence": sequence,
        })
        .to_string(),
        timestamp: now_ms,
        delivered_via_channel: false,
    };
    let message_id = message.id.clone();
    let timestamp = {
        let _guard = PEER_IDENTITY_BIND_LOCK.lock();
        if !state.peer_agents.contains_key(recipient) {
            return false;
        }
        state.push_agent_inbox(recipient, message)
    };
    if crate::pty::route_registered_orchestrator_mail(state, recipient, &message_id, timestamp)
        .is_none()
    {
        let live_pty = state.live_pty_for_peer(recipient);
        if state.assign_agent_delivery(recipient, &message_id, live_pty.is_some())
            == crate::state::AgentDeliveryAssignment::Terminal
            && let Some(session_id) = live_pty
        {
            let outcome = crate::pty::deliver_notice_to_managed_pty(
                state,
                &session_id,
                crate::pty::PEER_MAIL_WAKE,
            );
            crate::pty::settle_terminal_delivery(state, recipient, &message_id, outcome);
        }
    }
    true
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkflowLaunchInput {
    pub(super) run_id: String,
    attempt_id: String,
    pub(super) worktree_path: String,
    pub(super) agent_type: String,
    #[serde(default)]
    pub(super) skills: Vec<crate::workflows::SkillReference>,
    pub(super) feedback: Option<String>,
}

pub(super) fn handle_workflow_launch(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let input: WorkflowLaunchInput = match serde_json::from_value(args["input"].clone()) {
        Ok(input) => input,
        Err(error) => {
            return serde_json::json!({"error": format!("invalid workflow launch: {error}")});
        }
    };
    launch_workflow_agent(state, addr, mcp_session_id, input)
        .unwrap_or_else(|error| serde_json::json!({"error": error}))
}

fn launch_workflow_agent(
    state: &Arc<AppState>,
    addr: SocketAddr,
    mcp_session_id: Option<&str>,
    input: WorkflowLaunchInput,
) -> Result<serde_json::Value, String> {
    if !addr.ip().is_loopback() {
        return Err("workflow agent launch is restricted to localhost".into());
    }
    let caller = resolve_mcp_origin_pty(state, mcp_session_id)
        .ok_or("workflow launch requires a bound live managed session")?;
    let project = crate::progress::project_for_session(state, &caller)
        .ok_or("calling session has no registered project")?;
    let owner = crate::progress::resolve_owning_project(Some(&project))?
        .to_string_lossy()
        .to_string();
    let store = crate::workflows::RunStore::open()?;
    let run = store.snapshot(&input.run_id)?;
    if run.project != owner {
        return Err("workflow run does not belong to calling session's project".into());
    }
    let attempt = run
        .attempts
        .iter()
        .find(|a| a.id == input.attempt_id)
        .ok_or("workflow attempt not found")?;
    if attempt.story_id != run.plan_id
        && crate::workflows::active_coordinator_session(&run)?.as_deref() != Some(&caller)
    {
        return Err("only this run's active coordinator may launch a story worker".into());
    }
    if attempt.story_id == run.plan_id {
        let cwd = state
            .session_maps
            .sessions
            .get(&caller)
            .and_then(|session| session.lock().cwd.clone())
            .ok_or("calling session has no working directory")?;
        let cwd = std::path::Path::new(&cwd)
            .canonicalize()
            .map_err(|e| format!("resolve calling worktree: {e}"))?;
        let path = std::path::Path::new(&input.worktree_path)
            .canonicalize()
            .map_err(|e| format!("resolve workflow worktree: {e}"))?;
        if !cwd.starts_with(&path) {
            return Err("workflow plan launch must use the caller's isolated worktree".into());
        }
    }
    launch_workflow_effect(state, addr, mcp_session_id, input, &owner, false)
}

fn launch_workflow_effect(
    state: &Arc<AppState>,
    addr: SocketAddr,
    mcp_session_id: Option<&str>,
    input: WorkflowLaunchInput,
    owner: &str,
    daemon: bool,
) -> Result<serde_json::Value, String> {
    if input.agent_type.trim().is_empty() || input.agent_type.len() > 128 {
        return Err("invalid workflow agent type".into());
    }
    let worktree = std::path::Path::new(&input.worktree_path)
        .canonicalize()
        .map_err(|error| format!("resolve workflow worktree: {error}"))?;
    let worktree = worktree.to_string_lossy().to_string();
    if worktree == owner {
        return Err("workflow agents require an isolated worktree".into());
    }
    crate::worktree::validate_worktree_path(owner, &worktree)?;
    let store = crate::workflows::RunStore::open()?;
    let run = store.snapshot(&input.run_id)?;
    if run.project != owner {
        return Err("workflow run does not belong to calling session's project".into());
    }
    if run.status != crate::workflows::RunStatus::Running {
        return Err("workflow run is not running".into());
    }
    let attempt = run
        .attempts
        .iter()
        .find(|attempt| attempt.id == input.attempt_id)
        .ok_or("workflow attempt not found")?;
    if attempt.state != crate::workflows::AttemptState::Running {
        return Err("workflow attempt is not running".into());
    }
    if let Some(binding) = &attempt.agent {
        return Ok(serde_json::json!({
            "runId": run.id,
            "attemptId": attempt.id,
            "session_id": binding.session_id,
            "task_id": binding.task_id,
            "already_bound": true,
        }));
    }
    let package = if attempt.story_id == run.plan_id {
        let plan = crate::stories::StoryStore::open()?.get_plan(&run.plan_id)?;
        let definition = crate::workflows::WorkflowStore::open()?
            .get_published(&run.definition_id, run.definition_revision)?;
        let stories = crate::stories::StoryStore::open()?.list_stories(&run.plan_id)?;
        let plan_text = read_plan_text_for_prompt(&worktree, &plan.source)?;
        crate::workflows::render_plan_prompt(
            &run,
            &plan,
            attempt,
            &definition,
            &stories,
            &[],
            run.sequence,
            &plan_text,
            &input.skills,
        )?
    } else {
        let story = crate::stories::StoryStore::open()?.get_story(&attempt.story_id)?;
        let definition = crate::workflows::WorkflowStore::open()?
            .get_published(&run.story_definition_id, run.story_definition_revision)?;
        crate::workflows::render_story_prompt(
            &run,
            &story,
            attempt,
            &definition,
            &input.skills,
            input.feedback.as_deref(),
        )?
    };
    let effect_key = format!("spawn:{}", attempt.id);
    let intended = run.effects.iter().find(|effect| effect.key == effect_key);
    if intended
        .is_some_and(|effect| !daemon || effect.state != crate::workflows::EffectState::Intended)
    {
        return Err("spawn intent already exists; reconcile before retrying".into());
    }
    if attempt.story_id != run.plan_id
        && !run.stories.iter().any(|story| {
            story.story_id == attempt.story_id && story.worktree_path.as_deref() == Some(&worktree)
        })
    {
        let assignment = store.command(
            &run.id,
            &format!("assign-worktree:{}", attempt.story_id),
            crate::workflows::RunCommand::AssignWorktree {
                story_id: attempt.story_id.clone(),
                path: worktree.clone(),
            },
        )?;
        if assignment.sequence > run.sequence {
            crate::workflows::emit_run_changed(state, &run.project, &run.id, assignment.sequence);
        }
    }
    let effect = if let Some(effect) = intended {
        effect.clone()
    } else {
        let reserved = store.command(
            &run.id,
            &format!("spawn-intent:{}", attempt.id),
            crate::workflows::RunCommand::ReserveEffect {
                key: effect_key,
                kind: crate::workflows::EffectKind::SpawnAgent,
            },
        )?;
        crate::workflows::emit_run_changed(state, &run.project, &run.id, reserved.sequence);
        let crate::workflows::RunEventKind::EffectReserved { effect } = reserved.event.kind else {
            return Err("workflow state changed before spawn; retry after refreshing".into());
        };
        effect
    };
    // The reservation and external spawn cannot share a database transaction.
    // Avoid starting the process when a cancellation already won the race.
    if daemon && store.duration_expired(&run.id)? {
        store.command(
            &run.id,
            &format!("daemon:spawn-deadline:{}", attempt.id),
            crate::workflows::RunCommand::ExpireDeadline,
        )?;
    }
    if store.snapshot(&run.id)?.status != crate::workflows::RunStatus::Running {
        // No external action happened, so a paused run can close the intent.
        // Cancellation has already marked outstanding intents uncertain.
        if store.snapshot(&run.id)?.status == crate::workflows::RunStatus::Paused
            && let Ok(failed) = store.command(
                &run.id,
                &format!("spawn-aborted:{}", attempt.id),
                crate::workflows::RunCommand::MarkEffect {
                    effect_id: effect.id.clone(),
                    succeeded: false,
                },
            )
        {
            crate::workflows::emit_run_changed(state, &run.project, &run.id, failed.sequence);
        }
        return Err("workflow stopped before agent spawn".into());
    }
    let spawned = super::managed_launch::launch(
        state,
        addr,
        mcp_session_id,
        &input.agent_type,
        &format!("Workflow {:?}", package.role),
        &package.prompt,
        &worktree,
    );
    if let Some(error) = spawned.get("error").and_then(serde_json::Value::as_str) {
        if let Ok(failed) = store.command(
            &run.id,
            &format!("spawn-failed:{}", attempt.id),
            crate::workflows::RunCommand::MarkEffect {
                effect_id: effect.id.clone(),
                succeeded: false,
            },
        ) {
            crate::workflows::emit_run_changed(state, &run.project, &run.id, failed.sequence);
        }
        return Err(error.to_owned());
    }
    let Some(session_id) = spawned["session_id"].as_str() else {
        let _ = store.reconcile(&run.id);
        return Err(
            "managed spawn returned no session id; workflow paused for reconciliation".into(),
        );
    };
    let binding = crate::workflows::AgentBinding {
        session_id: session_id.into(),
        task_id: spawned["task_id"].as_str().map(str::to_owned),
        effect_id: effect.id.clone(),
        prompt_contract_version: package.contract_version,
        prompt_sha256: package.prompt_sha256,
        audit_preview: package.audit_preview,
    };
    let bound = match store.bind_agent(&run.id, &attempt.id, binding) {
        Ok(receipt) => receipt,
        Err(error) => {
            // Cancellation can still win while the managed process starts.
            // Stop the unbound child; its intent remains uncertain in the run
            // snapshot so an operator can see what happened.
            let killed = handle_session(
                state,
                &serde_json::json!({"action": "kill", "session_id": session_id}),
                mcp_session_id,
            );
            let _ = store.reconcile(&run.id);
            return Err(format!(
                "managed agent started but attempt binding failed: {error}; child termination: {killed}"
            ));
        }
    };
    crate::workflows::emit_run_changed(state, &run.project, &run.id, bound.sequence);
    if !state.session_maps.sessions.contains_key(session_id) {
        for changed in store.interrupt_agent_session(session_id)? {
            crate::workflows::emit_run_changed(
                state,
                &changed.project,
                &changed.id,
                changed.sequence,
            );
        }
    }
    Ok(serde_json::json!({
        "runId": run.id,
        "attemptId": attempt.id,
        "session_id": session_id,
        "task_id": spawned["task_id"],
        "sequence": bound.sequence,
    }))
}

fn read_plan_text_for_prompt(project: &str, source: &str) -> Result<String, String> {
    let relative = std::path::Path::new(source);
    if relative.as_os_str().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Ok(String::new());
    }
    let path = std::path::Path::new(project).join(relative);
    if !path.is_file() {
        return Ok(String::new());
    }
    let resolved = path
        .canonicalize()
        .map_err(|error| format!("resolve plan source: {error}"))?;
    if !resolved.starts_with(project) {
        return Err("plan source escapes the project".into());
    }
    if resolved
        .metadata()
        .map_err(|error| format!("inspect plan source: {error}"))?
        .len()
        > 16_000
    {
        return Err("plan source exceeds prompt size limit".into());
    }
    std::fs::read_to_string(&resolved).map_err(|error| format!("read plan source: {error}"))
}

pub(super) async fn handle_progress(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let input = match parse_progress_report_input(args) {
        Ok(input) => input,
        Err(error) => return error,
    };
    let agent_name = resolve_mcp_origin_session(state, mcp_session_id)
        .and_then(|id| state.peer_agents.get(&id).map(|peer| peer.name.clone()));
    let agent_type = resolve_mcp_origin_agent_type(state, mcp_session_id);
    let workspace_path = resolve_mcp_origin_repo_path(state, mcp_session_id);
    let pty_id = resolve_mcp_origin_pty(state, mcp_session_id);
    let acp_session_id = resolve_mcp_origin_session(state, mcp_session_id)
        .and_then(|peer| state.acp.peer_conversation(&peer))
        .map(|(_, session)| session.to_string());
    let state = state.clone();
    run_blocking_handler(move || {
        match report_progress(
            &state,
            workspace_path.as_deref(),
            input,
            agent_name,
            agent_type.as_deref(),
            pty_id.as_deref(),
            acp_session_id.as_deref(),
        ) {
            Ok(receipt) => to_json_or_error(receipt),
            Err(error) => serde_json::json!({"error": error}),
        }
    })
    .await
}

/// Persist, then push. A toast that claims a record which does not exist is
/// worse than silence, so nothing is emitted until the entry is committed; a
/// frontend that missed the push recovers by querying.
pub(crate) fn report_progress(
    state: &Arc<AppState>,
    workspace_path: Option<&str>,
    input: crate::progress::ProgressReportInput,
    agent_name: Option<String>,
    agent_type: Option<&str>,
    pty_id: Option<&str>,
    acp_session_id: Option<&str>,
) -> Result<crate::progress::ProgressReceipt, String> {
    let submitted = crate::progress::submit_progress_report(
        state.as_ref(),
        workspace_path,
        input,
        agent_name,
        agent_type,
        pty_id,
    )?;
    let blocked_question = (submitted.kind == crate::progress::ProgressKind::Blocked)
        .then(|| (submitted.pty_id.clone(), submitted.text.clone()));
    let receipt = emit_progress_entry_with_acp(state, submitted, acp_session_id);
    if let Some((Some(session_id), text)) = blocked_question.as_ref()
        && state.session_maps.sessions.contains_key(session_id)
    {
        // Reuse the authoritative session-state lane. It owns awaiting state,
        // parent routing, mobile deep links, and the per-session push limit.
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.clone(),
            parsed: serde_json::json!({
                "type": "question",
                "prompt_text": text,
                "confident": true,
                "source": "progress-blocked",
            })
            .into(),
        });
    }
    if let (Some(session_id), Some((_, text))) = (acp_session_id, blocked_question.as_ref()) {
        let config = state.config.read();
        let push_ready = config.services.push.enabled
            && !config.services.push.vapid_private_key.is_empty()
            && !state.push_store.is_empty();
        drop(config);
        let window_focused = state
            .desktop_window_focused
            .load(std::sync::atomic::Ordering::Relaxed);
        if push_ready
            && crate::state::mobile_push_away(
                window_focused,
                if window_focused {
                    crate::state::hid_idle_seconds()
                } else {
                    None
                },
            )
        {
            crate::state::AppState::send_mobile_push_url(
                state,
                format!(
                    "/mobile?acpSessionId={}",
                    url::form_urlencoded::byte_serialize(session_id.as_bytes()).collect::<String>()
                ),
                text,
            );
        }
    }
    Ok(receipt)
}

/// Announce a committed entry on both transports and hand back its receipt.
/// Shared by the reporting tool and `intent:` capture: one entry, one push,
/// whichever half of the journal wrote it.
pub(crate) fn emit_progress_entry(
    state: &AppState,
    entry: crate::progress::ProgressEntry,
) -> crate::progress::ProgressReceipt {
    emit_progress_entry_with_acp(state, entry, None)
}

fn emit_progress_entry_with_acp(
    state: &AppState,
    entry: crate::progress::ProgressEntry,
    acp_session_id: Option<&str>,
) -> crate::progress::ProgressReceipt {
    let receipt = crate::progress::ProgressReceipt { id: entry.id };
    let mut payload = serde_json::json!({ "entry": &entry });
    if let Some(session_id) = acp_session_id {
        payload["acpSessionId"] = serde_json::json!(session_id);
    }
    let repo_path = entry.project.clone();
    // There is no bus→window forwarder: producers dual-emit (AGENTS.md).
    #[cfg(feature = "desktop")]
    if let Some(app) = state.app_handle.read().as_ref() {
        let _ = app.emit(
            "progress-recorded",
            serde_json::json!({"repo_path": &repo_path, "payload": &payload}),
        );
    }
    let _ = state
        .event_bus
        .send(crate::state::AppEvent::ProgressRecorded { repo_path, payload });
    // A terminal that reports done or hands work off has moved on from any
    // question it raised with `blocked`. The accumulator clears only that
    // source; a later `blocked` re-arms by replacing the question text.
    if matches!(
        entry.kind,
        crate::progress::ProgressKind::Done | crate::progress::ProgressKind::Delegated
    ) && let Some(session_id) = entry.pty_id.as_deref()
        && state.session_maps.sessions.contains_key(session_id)
    {
        state.emit_pty_event(crate::state::AppEvent::PtyParsed {
            session_id: session_id.to_string(),
            parsed: serde_json::json!({"type": "progress-superseded"}).into(),
        });
    }
    receipt
}

/// Journal a spawn or a send as a hand-off between two terminals, for the
/// Progress Flow view.
///
/// Silent on every skip, like `intent:` capture: collection is off, the sender
/// is in no registered project, or the sender is not a terminal. `from_peer` is
/// a peer identity and is walked back to the PTY it runs in.
pub(crate) fn journal_hand_off(
    state: &AppState,
    kind: crate::progress::ProgressKind,
    from_peer: &str,
    to_pty: &str,
    to_name: Option<&str>,
    text: &str,
) {
    let Some(from_pty) = state.live_pty_for_peer(from_peer) else {
        return;
    };
    match crate::progress::record_hand_off(state, kind, &from_pty, to_pty, to_name, text) {
        Ok(entry) => {
            emit_progress_entry(state, entry);
        }
        Err(error) => tracing::debug!(
            source = "progress",
            from = %from_pty,
            to = %to_pty,
            error = %error,
            "hand-off not recorded in the Progress journal"
        ),
    }
}

/// How many HTML tab ids one TUIC session may keep registered for auto-close.
///
/// The list is replayed into `close-html-tabs` when the session exits, so it is
/// held for the whole life of the session. A caller that opens tabs in a loop
/// would otherwise grow it without limit; past the cap the oldest registration
/// is dropped, which costs at worst one orphaned tab the user can close by hand.
pub(super) const SESSION_HTML_TAB_LIMIT: usize = 64;

pub(super) fn handle_ui(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "ui", UI_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "tab" => {
            let id = match args["id"].as_str() {
                Some(v) => v.to_string(),
                None => return serde_json::json!({"error": "Action 'tab' requires 'id'"}),
            };
            let title = match args["title"].as_str() {
                Some(v) => v.to_string(),
                None => return serde_json::json!({"error": "Action 'tab' requires 'title'"}),
            };
            let html_arg = args["html"].as_str().map(|s| s.to_string());
            let url_arg = args["url"].as_str().map(|s| s.to_string());
            let html = match (&html_arg, &url_arg) {
                (Some(h), None) => h.clone(),
                (None, Some(_)) => String::new(), // URL mode — html is empty, frontend uses url
                (Some(_), Some(_)) => {
                    return serde_json::json!({"error": "Provide either 'html' or 'url', not both"});
                }
                (None, None) => {
                    return serde_json::json!({"error": "Action 'tab' requires 'html' or 'url'"});
                }
            };
            // Guard: if a tuic session_id is provided and it already has a terminal,
            // decline to create an HTML tab (agent should use the terminal instead).
            if let Some(sid) = args["session_id"].as_str()
                && (state.grid.vt_log_buffers.contains_key(sid)
                    || state.session_maps.sessions.contains_key(sid))
            {
                return serde_json::json!({
                    "ok": false,
                    "warning": format!("Session '{}' already has an active terminal. Use the terminal tab instead of creating an HTML tab.", sid)
                });
            }
            let pinned = args["pinned"].as_bool().unwrap_or(false);
            let focus = args["focus"].as_bool().unwrap_or(true);
            // Resolve origin repo for the calling MCP session so the tab lands
            // in the repo where the agent is actually working, not whichever
            // repo happens to have focus in the frontend.
            let caller_tuic = mcp_session_id
                .and_then(|mcp_sid| state.mcp.to_session.get(mcp_sid).map(|s| s.value().clone()));
            let origin_repo_path = resolve_mcp_origin_repo_path(state, mcp_session_id);
            let mut payload = serde_json::json!({
                "id": id,
                "title": title,
                "html": html,
                "pinned": pinned,
                "focus": focus,
            });
            if let Some(ref u) = url_arg {
                payload["url"] = serde_json::Value::String(u.clone());
            }
            if let Some(ref p) = origin_repo_path {
                payload["origin_repo_path"] = serde_json::Value::String(p.clone());
            }
            // Register this tab under the creator's tuic session so it can be
            // closed automatically when that session exits.
            if let Some(ref tuic_session) = caller_tuic {
                let mut registered = state
                    .session_maps
                    .session_html_tabs
                    .entry(tuic_session.clone())
                    .or_default();
                // The id IS the dedup key — this same call updates an existing
                // tab when the id repeats, so registering it twice would only
                // grow the close list with a duplicate close of one tab.
                if !registered.contains(&id) {
                    if registered.len() >= SESSION_HTML_TAB_LIMIT {
                        registered.remove(0);
                    }
                    registered.push(id.clone());
                }
            }
            // Emit to Tauri webview (native mode)
            #[cfg(feature = "desktop")]
            if let Some(app) = state.app_handle.read().as_ref() {
                let _ = app.emit("ui-tab", &payload);
            }
            // Emit to SSE clients (browser/mobile)
            let _ = state.event_bus.send(crate::state::AppEvent::UiTab {
                id: id.clone(),
                title,
                html,
                url: url_arg,
                pinned,
                focus,
                origin_repo_path,
            });
            serde_json::json!({"ok": true, "id": id})
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'ui'. Available: {}", other, UI_ACTIONS
        )}),
    }
}

/// Sounds an MCP caller may request by name. `attention` is the callback added for
/// autonomous agents that need the user back at the keyboard; the rest are the
/// tones already wired into the app's own notifications, exposed so a caller can
/// borrow the meaning the user has already learned.
const TOAST_SOUNDS: [&str; 6] = [
    "question",
    "completion",
    "error",
    "warning",
    "info",
    "attention",
];

/// Resolve the `sound` argument of `ui action=toast` into a notification sound
/// name, or `None` for a silent toast.
///
/// `true` keeps meaning "the sound that matches this level" — but it now
/// resolves to a real `NotificationSound` rather than a toast-local tone, so a
/// muted sound or a chosen output device is honoured either way. A name lets the
/// caller override that, which is the whole point of `attention`: the level says
/// how bad it is, the sound says how hard to pull on the user's sleeve.
pub(super) fn resolve_toast_sound(
    sound: &serde_json::Value,
    level: &str,
) -> Result<Option<String>, serde_json::Value> {
    match sound {
        serde_json::Value::Null => Ok(None),
        serde_json::Value::Bool(false) => Ok(None),
        serde_json::Value::Bool(true) => Ok(Some(
            match level {
                "warn" => "warning",
                "error" => "error",
                _ => "info",
            }
            .to_string(),
        )),
        serde_json::Value::String(name) if TOAST_SOUNDS.contains(&name.as_str()) => {
            Ok(Some(name.clone()))
        }
        other => Err(serde_json::json!({"error": format!(
            "Invalid sound {}. Use true/false or one of: {}",
            other,
            TOAST_SOUNDS.join(", ")
        )})),
    }
}

fn handle_ui_toast(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "ui", UI_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "toast" => {
            let title = match args["title"].as_str() {
                Some(t) => t.to_string(),
                None => return serde_json::json!({"error": "Action 'toast' requires 'title'"}),
            };
            let message = args["message"].as_str().map(|s| s.to_string());
            let level = args["level"].as_str().unwrap_or("info");
            let level = match level {
                "info" | "warn" | "error" => level.to_string(),
                other => {
                    return serde_json::json!({"error": format!(
                        "Invalid level '{}'. Must be: info, warn, error", other
                    )});
                }
            };
            let sound = match resolve_toast_sound(&args["sound"], &level) {
                Ok(sound) => sound,
                Err(e) => return e,
            };
            let origin_repo_path = resolve_mcp_origin_repo_path(state, mcp_session_id);
            let origin_session_id = resolve_mcp_origin_session(state, mcp_session_id);
            // Dual-emit. The bus only reaches SSE clients (browser/PWA); the
            // desktop WebView listens on the Tauri bridge and there is no
            // bus→window forwarder, so a bus-only send made this tool a no-op on
            // the very client the user is usually sitting in front of.
            #[cfg(feature = "desktop")]
            if let Some(ref app) = *state.app_handle.read() {
                use tauri::Emitter;
                let _ = app.emit(
                    "mcp-toast",
                    serde_json::json!({
                        "title": title,
                        "message": message,
                        "level": level,
                        "sound": sound,
                        "origin_repo_path": origin_repo_path,
                        "origin_session_id": origin_session_id,
                    }),
                );
            }
            let _ = state.event_bus.send(crate::state::AppEvent::McpToast {
                title,
                message,
                level,
                sound,
                origin_repo_path,
                origin_session_id,
            });
            serde_json::json!({"ok": true})
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'ui'. Available: {}", other, UI_ACTIONS
        )}),
    }
}

/// How long a confirmation waits for a human before it gives up.
///
/// Matches the MCP wait ceiling used elsewhere. A native dialog waited forever,
/// which is only tolerable when the human is guaranteed to be at the machine —
/// the whole point of routing this through the clients is that they may not be.
pub(super) const CONFIRM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

/// Ask the human a yes/no question and wait for the answer.
///
/// The request goes to **every** client — desktop WebView, browser, mobile PWA —
/// plus a mobile push, and the first answer wins. It used to be a native OS
/// dialog, which meant an agent asking to confirm a destructive operation blocked
/// until someone walked back to the machine: unanswerable, and therefore blocking,
/// for a human working remotely.
///
/// A request nobody answers within [`CONFIRM_TIMEOUT`] resolves as *not* confirmed
/// and says so, so the caller can tell silence apart from a deliberate refusal.
pub(super) async fn handle_confirm(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    if !addr.ip().is_loopback() {
        return serde_json::json!({"error": "Action 'confirm' is restricted to localhost connections"});
    }
    let title = match args["title"].as_str() {
        Some(t) => t.to_string(),
        None => return serde_json::json!({"error": "Action 'confirm' requires 'title'"}),
    };
    let message = args["message"].as_str().unwrap_or("").to_string();
    let origin_repo_path = resolve_mcp_origin_repo_path(state, mcp_session_id);
    let origin_session_id = resolve_mcp_origin_session(state, mcp_session_id);

    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.confirm_responses.insert(request_id.clone(), tx);

    // Dual-emit, same reason as the toast above: the bus only reaches SSE
    // clients, and there is no bus→window forwarder for the desktop WebView.
    #[cfg(feature = "desktop")]
    if let Some(ref app) = *state.app_handle.read() {
        use tauri::Emitter;
        let _ = app.emit(
            "mcp-confirm",
            serde_json::json!({
                "request_id": request_id,
                "title": title,
                "message": message,
                "origin_repo_path": origin_repo_path,
                "origin_session_id": origin_session_id,
            }),
        );
    }
    let _ = state.event_bus.send(crate::state::AppEvent::McpConfirm {
        request_id: request_id.clone(),
        title: title.clone(),
        message: message.clone(),
        origin_repo_path,
        origin_session_id: origin_session_id.clone(),
    });

    // A PWA that is not open receives nothing over SSE, so push is the only way
    // a remote human learns an agent is waiting on them.
    let url = match origin_session_id {
        Some(ref sid) => format!("/mobile/session/{sid}"),
        None => "/mobile".to_string(),
    };
    let body = if message.is_empty() {
        title.clone()
    } else {
        format!("{title} — {message}")
    };
    crate::state::AppState::send_mobile_push_url(state, url, &body);

    let confirmed = match tokio::time::timeout(CONFIRM_TIMEOUT, rx).await {
        Ok(Ok(answer)) => answer,
        // Sender dropped without answering, or nobody answered in time. Both are
        // "no human said yes", which is the safe reading for a destructive op.
        _ => {
            state.confirm_responses.remove(&request_id);
            let _ = state
                .event_bus
                .send(crate::state::AppEvent::McpConfirmResolved {
                    request_id: request_id.clone(),
                    confirmed: false,
                });
            #[cfg(feature = "desktop")]
            if let Some(ref app) = *state.app_handle.read() {
                use tauri::Emitter;
                let _ = app.emit(
                    "mcp-confirm-resolved",
                    serde_json::json!({ "request_id": request_id, "confirmed": false }),
                );
            }
            return serde_json::json!({
                "confirmed": false,
                "reason": format!("no answer within {}s", CONFIRM_TIMEOUT.as_secs()),
            });
        }
    };
    serde_json::json!({"confirmed": confirmed})
}

// ── Unified handlers (merged tools) ──────────────────────────────────────

/// Merged repo tool: dispatches to workspace, github, or worktree handlers.
#[cfg(test)]
pub(super) async fn handle_repo(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    is_claude_code: bool,
) -> serde_json::Value {
    handle_repo_with_caller(state, args, is_claude_code, None).await
}

pub(super) async fn handle_repo_with_caller(
    state: &Arc<AppState>,
    args: &serde_json::Value,
    is_claude_code: bool,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "repo", REPO_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "list" | "active" => handle_repo_listing(state, args),
        "status" => handle_github(state, args).await,
        "branch_integrations"
        | "branch_integration"
        | "worktree_list"
        | "worktree_lifecycle"
        | "worktree_create"
        | "worktree_remove"
        | "orphan_cleanup_answer"
        | "branch_delete" => {
            handle_worktree_with_caller(state, args, is_claude_code, mcp_session_id).await
        }
        "progress_list" => {
            let path = match require_path(args, action) {
                Ok(path) => path,
                Err(error) => return error,
            };
            if let Err(error) = validate_mcp_repo_path(&path) {
                return error;
            }
            let input = args
                .get("input")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            run_blocking_handler(move || {
                let result: Result<serde_json::Value, String> = serde_json::from_value(input)
                    .map_err(|e| format!("progress_invalid_request: {e}"))
                    .and_then(|v| crate::progress::progress_list(&path, v))
                    .and_then(|v| serde_json::to_value(v).map_err(|e| e.to_string()));
                result.unwrap_or_else(|error| serde_json::json!({"error": error}))
            })
            .await
        }
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'repo'. Available: {}", other, REPO_ACTIONS
        )}),
    }
}

/// Merged ui tool: original tab action + notify toast/confirm + screenshot.
pub(super) async fn handle_ui_unified(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
    mcp_session_id: Option<&str>,
) -> serde_json::Value {
    let action = match require_action(args, "ui", UI_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "tab" => handle_ui(state, args, mcp_session_id),
        "toast" => handle_ui_toast(state, args, mcp_session_id),
        // Waits for the human, but only on a oneshot — no blocking-pool worker is
        // held, so a confirmation left unanswered costs a pending task and nothing
        // else.
        "confirm" => handle_confirm(state, addr, args, mcp_session_id).await,
        "screenshot" => handle_screenshot(state, addr, args).await,
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'ui'. Available: {}", other, UI_ACTIONS
        )}),
    }
}

/// Capture a screenshot of a plugin panel tab and return it as an MCP image content block.
/// Desktop-only, loopback-only. Sends a Tauri event to the frontend which captures the
/// iframe content and responds via the `screenshot_response` command.
pub(super) async fn handle_screenshot(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
) -> serde_json::Value {
    #[cfg(not(feature = "desktop"))]
    {
        let _ = (state, addr, args);
        serde_json::json!({"error": "Action 'screenshot' requires desktop feature"})
    }
    #[cfg(feature = "desktop")]
    {
        if !addr.ip().is_loopback() {
            return serde_json::json!({"error": "Action 'screenshot' is restricted to localhost connections"});
        }
        let panel_id = match args["id"].as_str() {
            Some(id) => id.to_string(),
            None => {
                return serde_json::json!({"error": "Action 'screenshot' requires 'id' (the plugin panel ID)"});
            }
        };
        let app_handle = state.app_handle.read().clone();
        let Some(handle) = app_handle else {
            return serde_json::json!({"error": "AppHandle not available (headless mode)"});
        };

        let request_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = tokio::sync::oneshot::channel();
        state.screenshot_responses.insert(request_id.clone(), tx);

        use tauri::Emitter;
        if let Err(e) = handle.emit(
            "screenshot-request",
            serde_json::json!({
                "id": panel_id,
                "request_id": request_id,
            }),
        ) {
            state.screenshot_responses.remove(&request_id);
            return serde_json::json!({"error": format!("Failed to emit screenshot request: {e}")});
        }

        match tokio::time::timeout(std::time::Duration::from_secs(15), rx).await {
            Ok(Ok(Some(base64_data))) => {
                use base64::Engine;
                let bytes = match base64::engine::general_purpose::STANDARD.decode(&base64_data) {
                    Ok(b) => b,
                    Err(e) => {
                        return serde_json::json!({"error": format!("Invalid base64 from frontend: {e}")});
                    }
                };
                let dir = state.data_dir.join("screenshots");
                let _ = std::fs::create_dir_all(&dir);
                let filename = format!("{}.webp", request_id);
                let path = dir.join(&filename);
                if let Err(e) = std::fs::write(&path, &bytes) {
                    return serde_json::json!({"error": format!("Failed to write screenshot: {e}")});
                }
                serde_json::json!({
                    "ok": true,
                    "path": path.to_string_lossy(),
                    "size_bytes": bytes.len()
                })
            }
            Ok(Ok(None)) => {
                serde_json::json!({"error": format!(
                    "Screenshot failed: panel '{}' not found or iframe content not accessible", panel_id
                )})
            }
            Ok(Err(_)) => {
                serde_json::json!({"error": "Screenshot response channel dropped"})
            }
            Err(_) => {
                state.screenshot_responses.remove(&request_id);
                serde_json::json!({"error": "Screenshot timed out (15s)"})
            }
        }
    }
}

/// Extended debug tool: original actions + plugin_guide.
pub(super) fn handle_debug_unified(
    state: &Arc<AppState>,
    addr: SocketAddr,
    args: &serde_json::Value,
) -> serde_json::Value {
    let action = match require_action(args, "debug", DEBUG_ACTIONS) {
        Ok(a) => a,
        Err(e) => return e,
    };
    match action {
        "invoke_js" => {
            if !addr.ip().is_loopback() {
                return serde_json::json!({"error": "invoke_js is restricted to localhost connections"});
            }
            let script = match args["script"].as_str() {
                Some(s) => s,
                None => return serde_json::json!({"error": "script required (string)"}),
            };
            // Shared with the HTTP /debug/invoke_js route (see log_routes::eval_debug_script).
            super::log_routes::eval_debug_script(state, script)
        }
        "agent_detection" | "logs" | "sessions" => handle_debug(state, args),
        "help" => serde_json::json!({
            "actions": {
                "help": "This guide.",
                "agent_detection": "Agent detection pipeline diagnostics. Optional session_id (omit for all sessions).",
                "logs": "App log entries (info/warn/error mirrored from JS). Params: level, source, limit (default 50).",
                "sessions": "All PTY sessions with pid, cwd, foreground process info.",
                "invoke_js": "Execute JS in the main WebView (localhost only). Use `return expr` for output. Result + captured console output logged as source='eval_js'. Read via logs(source='eval_js', limit=1)."
            },
            "invoke_js_guide": {
                "console_capture": "console.log/warn/error/info are captured and included in the result.",
                "globals": {
                    "window.__TUIC__.stores()": "List all registered store snapshot names",
                    "window.__TUIC__.store(name)": "Get a store snapshot by name (repositories, paneLayout, settings, ui, keybindings, ...)",
                    "window.__TUIC__.plugins()": "All plugin states: id, loaded, enabled, error, builtIn",
                    "window.__TUIC__.plugin(id)": "Single plugin state with manifest",
                    "window.__TUIC__.pluginLogs(id, limit?)": "Plugin's internal PluginLogger entries (default 20)",
                    "window.__TUIC__.terminals()": "All terminals: id, name, sessionId, shellState, agentType, cwd",
                    "window.__TUIC__.terminal(id)": "Single terminal with awaitingInput, usageLimit",
                    "window.__TUIC__.agentTypeForSession(sid)": "Agent type lookup by PTY session ID",
                    "window.__TUIC__.activity()": "Activity center sections and active items",
                    "window.__TUIC__.logs(limit?)": "JS-side appLogger entries, all levels (default 50)"
                },
                "examples": [
                    "return window.__TUIC__.stores()",
                    "return window.__TUIC__.store('repositories')",
                    "return window.__TUIC__.store('paneLayout')",
                    "return window.__TUIC__.plugins()",
                    "return window.__TUIC__.terminals()"
                ]
            }
        }),
        other => serde_json::json!({"error": format!(
            "Unknown action '{}' for tool 'debug'. Available: {}", other, DEBUG_ACTIONS
        )}),
    }
}
/// Called only by the database-owning workflow actor, never by a transport caller.
pub(crate) fn launch_daemon_workflow_agent(
    state: &Arc<AppState>,
    run_id: &str,
    attempt_id: &str,
    worktree: &str,
    agent_type: &str,
    feedback: Option<String>,
) -> Result<serde_json::Value, String> {
    state.workflow_runtime.require_owner()?;
    let run = crate::workflows::RunStore::open()?.snapshot(run_id)?;
    let attempt = run
        .attempts
        .iter()
        .find(|a| a.id == attempt_id)
        .ok_or("workflow attempt missing")?;
    if !run.graph_executions.iter().any(|g| {
        g.target_id == attempt.story_id
            && g.activations.iter().any(|a| {
                (a.node_id == attempt.node_id
                    || (attempt.story_id == run.plan_id
                        && g.definition.graph.nodes.iter().any(|n| {
                            n.id == a.node_id
                                && matches!(n.kind, crate::workflows::NodeKind::CreateStories)
                        })))
                    && a.state == crate::workflows::graph::ActivationState::Running
            })
    }) {
        return Err("daemon launch requires a reached Agent activation".into());
    }
    launch_workflow_effect(
        state,
        "127.0.0.1:0"
            .parse()
            .map_err(|e| format!("daemon address: {e}"))?,
        None,
        WorkflowLaunchInput {
            run_id: run_id.into(),
            attempt_id: attempt_id.into(),
            worktree_path: worktree.into(),
            agent_type: agent_type.into(),
            skills: vec![],
            feedback,
        },
        &run.project,
        true,
    )
}

pub(crate) struct DaemonWorkspace {
    pub path: String,
    pub id: String,
}

pub(crate) async fn create_daemon_workflow_worktree(
    state: &Arc<AppState>,
    project: &str,
    branch: &str,
    base_ref: Option<&str>,
) -> Result<DaemonWorkspace, String> {
    super::worktree_routes::create_worktree_shared(
        state,
        project.into(),
        branch.into(),
        base_ref.map(str::to_owned),
        None,
        false,
    )
    .await
    .map_err(|(_, value)| value.0.to_string())
    .and_then(|created| {
        if let Some(error) = created.setup_script_error {
            return Err(error.to_string());
        }
        Ok(DaemonWorkspace {
            path: created.path,
            id: created.workspace_id,
        })
    })
}
