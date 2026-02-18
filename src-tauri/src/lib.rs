pub(crate) mod agent;
pub(crate) mod config;
mod dictation;
pub(crate) mod error_classification;
pub(crate) mod git;
pub(crate) mod github;
pub(crate) mod head_watcher;
pub(crate) mod mcp_http;
mod menu;
mod output_parser;
pub(crate) mod prompt;
pub(crate) mod pty;
pub(crate) mod state;
pub(crate) mod worktree;

use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::{Emitter, Manager, State};

// Re-export shared types from state module
pub(crate) use state::{AppState, OutputRingBuffer, PtySession};
pub(crate) use state::{SessionMetrics, MAX_CONCURRENT_SESSIONS};

/// Load configuration from cached AppState
#[tauri::command]
fn load_config(state: State<'_, Arc<AppState>>) -> config::AppConfig {
    state.config.read().unwrap().clone()
}

/// Save configuration to disk and update the AppState cache
#[tauri::command]
fn save_config(state: State<'_, Arc<AppState>>, config: config::AppConfig) -> Result<(), String> {
    config::save_app_config(config.clone())?;
    *state.config.write().unwrap() = config;
    Ok(())
}

/// Hash a plaintext password with bcrypt for remote access config
#[tauri::command]
fn hash_password(password: String) -> Result<String, String> {
    bcrypt::hash(&password, 12).map_err(|e| format!("Failed to hash password: {e}"))
}

/// Clear all git/GitHub operation caches
#[tauri::command]
fn clear_caches(state: State<'_, Arc<AppState>>) {
    state.clear_caches();
}


/// List all markdown files in a repository recursively (shared logic)
pub(crate) fn list_markdown_files_impl(path: String) -> Result<Vec<String>, String> {
    let repo_path = PathBuf::from(&path);

    if !repo_path.exists() {
        return Err(format!("Path does not exist: {path}"));
    }

    let mut md_files = Vec::new();

    // Use git ls-files to list tracked .md files (faster and respects .gitignore)
    let output = Command::new("git")
        .current_dir(&repo_path)
        .args(["ls-files", "*.md", "**/*.md"])
        .output()
        .map_err(|e| format!("Failed to execute git ls-files: {e}"))?;

    if output.status.success() {
        let files_text = String::from_utf8_lossy(&output.stdout);
        for line in files_text.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                md_files.push(trimmed.to_string());
            }
        }
    } else {
        // Fallback: manually walk the directory if git fails
        fn walk_dir(dir: &Path, base: &Path, md_files: &mut Vec<String>) -> std::io::Result<()> {
            if dir.is_dir() {
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    let path = entry.path();

                    // Skip hidden directories and common ignore patterns
                    if let Some(name) = path.file_name().and_then(|n| n.to_str())
                        && (name.starts_with('.') || name == "node_modules" || name == "target") {
                            continue;
                        }

                    if path.is_dir() {
                        walk_dir(&path, base, md_files)?;
                    } else if path.extension().and_then(|s| s.to_str()) == Some("md")
                        && let Ok(relative) = path.strip_prefix(base) {
                            md_files.push(relative.to_string_lossy().to_string());
                        }
                }
            }
            Ok(())
        }

        walk_dir(&repo_path, &repo_path, &mut md_files)
            .map_err(|e| format!("Failed to walk directory: {e}"))?;
    }

    // Sort files alphabetically
    md_files.sort();
    Ok(md_files)
}

#[tauri::command]
fn list_markdown_files(path: String) -> Result<Vec<String>, String> {
    list_markdown_files_impl(path)
}

/// Read file content (shared logic)
pub(crate) fn read_file_impl(path: String, file: String) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);
    let file_path = repo_path.join(&file);

    // Security: ensure the file is within the repo path
    let canonical_repo = repo_path.canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;
    let canonical_file = file_path.canonicalize()
        .map_err(|e| format!("Failed to resolve file path: {e}"))?;

    if !canonical_file.starts_with(&canonical_repo) {
        return Err("Access denied: file is outside repository".to_string());
    }

    std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read file: {e}"))
}

#[tauri::command]
fn read_file(path: String, file: String) -> Result<String, String> {
    read_file_impl(path, file)
}

/// Get MCP server status (running, port, active sessions)
#[tauri::command]
fn get_mcp_status(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    let config = state.config.read().unwrap().clone();
    let port_file = config::config_dir().join("mcp-port");

    let port = std::fs::read_to_string(&port_file)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok());

    let server_should_run = config.mcp_server_enabled || config.remote_access_enabled;
    let running = server_should_run && port.is_some();
    let active_sessions = state.sessions.len();

    serde_json::json!({
        "enabled": config.mcp_server_enabled,
        "running": running,
        "port": port,
        "active_sessions": active_sessions,
        "max_sessions": MAX_CONCURRENT_SESSIONS,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Default worktrees directory: <config_dir>/worktrees
    let worktrees_dir = config::config_dir().join("worktrees");

    let config = config::load_app_config();

    let github_token = crate::github::resolve_github_token();
    if github_token.is_none() {
        eprintln!("[github] No GitHub token found (checked GH_TOKEN, GITHUB_TOKEN, gh CLI config)");
    }

    let state = Arc::new(AppState {
        sessions: DashMap::new(),
        worktrees_dir,
        metrics: SessionMetrics::new(),
        output_buffers: DashMap::new(),
        mcp_sse_sessions: DashMap::new(),
        ws_clients: DashMap::new(),
        config: std::sync::RwLock::new(config.clone()),
        repo_info_cache: DashMap::new(),
        github_status_cache: DashMap::new(),
        head_watchers: DashMap::new(),
        http_client: reqwest::blocking::Client::new(),
        github_token,
    });

    // Start HTTP API server if either MCP or Remote Access is enabled
    if config.mcp_server_enabled || config.remote_access_enabled {
        let mcp_enabled = config.mcp_server_enabled;
        let remote_enabled = config.remote_access_enabled;
        let mcp_state = state.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create tokio runtime for HTTP server");
            rt.block_on(async move {
                mcp_http::start_server(mcp_state, mcp_enabled, remote_enabled).await;
            });
        });
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(
                    // Exclude SIZE to prevent progressive shrinking with titleBarStyle Overlay
                    tauri_plugin_window_state::StateFlags::POSITION
                        | tauri_plugin_window_state::StateFlags::MAXIMIZED
                        | tauri_plugin_window_state::StateFlags::VISIBLE
                        | tauri_plugin_window_state::StateFlags::DECORATIONS
                        | tauri_plugin_window_state::StateFlags::FULLSCREEN,
                )
                .build(),
        )
        .manage(state)
        .manage(dictation::DictationState::new())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus existing window when another instance is launched (Story 065)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let m = menu::build_menu(app)?;
            app.set_menu(m)?;
            app.on_menu_event(|app_handle, event| {
                let _ = app_handle.emit("menu-action", event.id().0.as_str());
            });

            // Auto-start HEAD watchers for known repositories
            let repos_json = config::load_repositories();
            if let Some(repos) = repos_json.get("repos").and_then(|r| r.as_object()) {
                let handle = app.handle().clone();
                for repo_path in repos.keys() {
                    if let Err(e) = head_watcher::start_watching(repo_path, &handle) {
                        eprintln!("[HeadWatcher] Failed to watch {repo_path}: {e}");
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pty::create_pty,
            pty::create_pty_with_worktree,
            pty::list_worktrees,
            pty::write_pty,
            pty::resize_pty,
            pty::pause_pty,
            pty::resume_pty,
            pty::close_pty,
            worktree::get_worktrees_dir,
            git::get_repo_info,
            git::get_git_diff,
            git::get_diff_stats,
            git::get_changed_files,
            git::get_file_diff,
            list_markdown_files,
            read_file,
            github::get_github_status,
            pty::get_orchestrator_stats,
            pty::get_session_metrics,
            pty::can_spawn_session,
            pty::list_active_sessions,
            pty::get_session_foreground_process,
            load_config,
            save_config,
            hash_password,
            agent::open_in_app,
            agent::detect_claude_binary,
            agent::detect_agent_binary,
            agent::detect_lazygit_binary,
            agent::spawn_agent,
            worktree::remove_worktree,
            agent::detect_installed_ides,
            worktree::create_worktree,
            git::rename_branch,
            worktree::get_worktree_paths,
            git::get_git_branches,
            git::check_is_main_branch,
            git::get_initials,
            github::get_ci_checks,
            github::get_repo_pr_statuses,
            worktree::generate_worktree_name_cmd,
            clear_caches,
            get_mcp_status,
            dictation::commands::get_dictation_status,
            dictation::commands::get_model_info,
            dictation::commands::download_whisper_model,
            dictation::commands::delete_whisper_model,
            dictation::commands::start_dictation,
            dictation::commands::stop_dictation_and_transcribe,
            dictation::commands::get_correction_map,
            dictation::commands::set_correction_map,
            dictation::commands::list_audio_devices,
            dictation::commands::inject_text,
            dictation::commands::get_dictation_config,
            dictation::commands::set_dictation_config,
            config::load_app_config,
            config::save_app_config,
            config::load_agent_config,
            config::save_agent_config,
            config::load_notification_config,
            config::save_notification_config,
            config::load_ui_prefs,
            config::save_ui_prefs,
            config::load_repo_settings,
            config::save_repo_settings,
            config::check_has_custom_settings,
            config::load_repositories,
            config::save_repositories,
            config::load_prompt_library,
            config::save_prompt_library,
            prompt::extract_prompt_variables,
            prompt::process_prompt_content,
            error_classification::classify_error_message,
            error_classification::calculate_backoff_delay_cmd,
            head_watcher::start_head_watcher,
            head_watcher::stop_head_watcher
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

