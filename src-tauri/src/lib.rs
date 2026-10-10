#![recursion_limit = "256"]
#![cfg_attr(
    not(feature = "desktop"),
    allow(dead_code, unused_imports, unused_variables)
)]

pub mod acp;
pub(crate) mod acp_chat;
pub(crate) mod acp_commands;
pub(crate) mod agent;
pub(crate) mod agent_hook;
pub(crate) mod agent_hook_codex;
pub(crate) mod agent_hook_commands;
pub(crate) mod agent_hook_installer;
pub(crate) mod agent_hook_launch;
pub(crate) mod agent_hook_opencode;
pub(crate) mod agent_mcp;
pub(crate) mod agent_session;
pub(crate) mod ai_agent;
pub(crate) mod attachments;
#[cfg(feature = "desktop")]
pub(crate) mod audio_enumeration;
pub(crate) mod prompt_receipt;
pub use tuic_core::app_instance;
pub(crate) mod app_logger;
pub(crate) mod changelog;
pub(crate) use tuic_terminal::chrome;
pub(crate) mod circleci;
pub(crate) mod claude_usage;
pub(crate) use tuic_core::cli;
#[expect(
    dead_code,
    reason = "Automation runtime and API consumers follow in plan steps 2–8"
)]
pub(crate) mod automations;
pub(crate) mod cli_usage_rpc;
pub(crate) mod codex_usage;
pub(crate) mod config;
pub(crate) mod conflict_assist;
pub(crate) mod content_index;
pub(crate) use tuic_git::cow;
pub(crate) mod cpu_watchdog;
pub(crate) use tuic_core::credentials;
#[cfg(feature = "desktop")]
pub(crate) mod design_mode;
// Tests of the sidecar config override that build.rs also compiles; a build
// script has no test harness.
#[cfg(test)]
#[path = "../build_sidecars.rs"]
mod build_sidecars;
#[cfg(feature = "dictation")]
mod dictation;
pub(crate) mod dir_watcher;
pub(crate) mod ego_cli;
#[cfg(all(feature = "desktop", not(feature = "dictation")))]
#[path = "dictation/ownership.rs"]
mod input_ownership;
pub(crate) use tuic_core::error_classification;
pub(crate) mod frontend_liveness;
pub(crate) mod fs;
pub(crate) mod generators;
pub(crate) mod git;
pub(crate) mod remote_transfer;
pub(crate) use tuic_git::git_cli;
pub(crate) mod git_graph;
pub(crate) mod idle_close;
pub(crate) use tuic_git::git_locks;
pub(crate) use tuic_git::git_reads;
#[cfg(test)]
mod critic_1420_tests;
pub(crate) mod github;
pub(crate) mod github_account;
pub(crate) mod github_auth;
#[cfg(test)]
mod github_compat_tests;
pub(crate) use tuic_git::github_debug;
pub(crate) mod github_poller;
#[cfg(feature = "desktop")]
mod global_hotkey;
pub(crate) use tuic_terminal::grid_gate;
pub(crate) mod grid_watch;
pub(crate) mod grok_usage;
pub(crate) mod improvement_scan;
pub(crate) use tuic_core::jsonc_edit;
pub(crate) use tuic_terminal::input_line_buffer;
pub(crate) mod mcp_http;
#[allow(dead_code)] // Incremental build: wired in story 1196+ (OAuth flow/token/registry)
pub(crate) mod mcp_oauth;
pub(crate) mod mcp_proxy;
pub(crate) mod mcp_upstream_config;
#[allow(dead_code)] // Used by OAuth discovery (story 1193-7f78), not yet wired
pub(crate) mod mcp_upstream_credentials;
pub(crate) mod mdkb_client;
#[cfg(feature = "desktop")]
pub(crate) mod mdkb_commands;
pub(crate) mod mdkb_daemon;
pub(crate) mod memory_report;
#[cfg(feature = "desktop")]
mod menu;
#[cfg(feature = "desktop")]
mod native_clipboard;
#[cfg(feature = "desktop")]
mod native_dialog;
#[cfg(feature = "desktop")]
mod native_drag;
#[cfg(feature = "desktop")]
mod native_keys;
#[cfg(feature = "desktop")]
mod native_notification;
#[cfg(feature = "desktop")]
pub(crate) mod notification_sound;
pub(crate) mod secrets;
pub(crate) use tuic_terminal::output_parser;
pub(crate) use tuic_terminal::output_watchers;
#[cfg(feature = "desktop")]
mod panel_window;
pub(crate) mod plugin_credentials;
pub(crate) mod plugin_exec;
pub(crate) mod plugin_fs;
pub(crate) mod plugin_http;
pub(crate) mod plugin_pty;
pub(crate) mod plugins;
pub(crate) mod pr_review;
#[cfg(feature = "desktop")]
mod press_and_hold;
pub(crate) use tuic_core::process_env;
pub(crate) mod progress;
pub(crate) mod prompt;
pub(crate) mod pty;
pub(crate) mod pty_capture;
pub(crate) mod push;
pub(crate) use tuic_core::redaction;
pub(crate) mod chat_view;
pub(crate) mod registry;
pub(crate) mod relay_client;
#[allow(dead_code)] // Constructors used by remote binary and future tests
pub(crate) mod remote_connection;
pub(crate) mod remote_deploy;
#[cfg_attr(feature = "desktop", allow(dead_code))]
pub(crate) mod remote_lifetime;
pub(crate) mod remote_mirror;
pub(crate) mod remote_runtime;
pub(crate) mod remote_update;
pub(crate) mod repo_watcher;
mod shell_integration;
#[cfg(feature = "desktop")]
pub(crate) mod sleep_prevention;
pub(crate) mod smart_prompt;
pub(crate) mod state;
pub(crate) mod stories;
pub(crate) mod subagent_map;
pub(crate) mod tailscale;
pub(crate) mod tasks;
#[expect(
    dead_code,
    reason = "Telegram offline ports await native integration after 1419/1420"
)]
pub(crate) mod telegram;
pub(crate) mod transcript_tail;
pub(crate) use tuic_terminal::terminal_grid;
#[cfg(test)]
mod build_graph_tests;
#[cfg(test)]
pub(crate) mod replay_oracle;
#[cfg(feature = "desktop")]
pub(crate) mod terminal_grid_commands;
#[cfg(test)]
pub(crate) mod test_support;
pub(crate) use tuic_core::text_rank;
pub(crate) mod themes;
pub(crate) mod tool_search;
#[cfg(feature = "desktop")]
mod tuic_cli;
#[allow(dead_code)] // Many items used only by the remote binary (not(desktop) build)
pub(crate) mod tunnels;
#[cfg(feature = "desktop")]
mod updater;
pub(crate) mod webview_recovery;
pub(crate) mod workflows;
pub(crate) mod worktree;

use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::{Emitter, Manager, State, WebviewWindow};

// Re-export shared types from state module
pub(crate) use state::MAX_CONCURRENT_SESSIONS;
pub(crate) use state::{AppState, OutputRingBuffer, PtySession};

#[cfg(feature = "desktop")]
/// Open a secondary window for multi-monitor use. The window loads the same
/// frontend with a `?mode=secondary` query param so App.tsx can render a
/// pane-only layout without sidebar or tab bar.
#[tauri::command]
async fn open_secondary_window(app: tauri::AppHandle) -> Result<(), String> {
    // If it already exists, just focus it
    if let Some(existing) = app.get_webview_window("secondary") {
        existing.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = tauri::WebviewUrl::App("/?mode=secondary".into());
    tauri::WebviewWindowBuilder::new(&app, "secondary", url)
        .title("TUICommander — Secondary")
        .inner_size(1200.0, 800.0)
        .min_inner_size(800.0, 600.0)
        .build()
        .map_err(|e| format!("Failed to create secondary window: {e}"))?;

    Ok(())
}

#[cfg(feature = "desktop")]
/// Fix corrupted dimensions in the window-state JSON before the plugin reads it.
/// titleBarStyle Overlay can persist width/height 0; SIZE is excluded from the
/// plugin flags so these zeros stay fossilised forever. We patch them at startup.
fn sanitize_window_state() {
    let Some(cfg_dir) = dirs::config_dir() else {
        return;
    };
    for id in ["com.tuic.preview", "com.tuic.commander"] {
        let path = cfg_dir.join(id).join(".window-state.json");
        let Ok(data) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&data) else {
            continue;
        };
        let Some(map) = json.as_object_mut() else {
            continue;
        };
        let mut changed = false;
        for (_label, state) in map.iter_mut() {
            let Some(obj) = state.as_object_mut() else {
                continue;
            };
            let w = obj.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
            let h = obj.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            if w < 800 || h < 600 {
                obj.insert("width".into(), serde_json::json!(1200));
                obj.insert("height".into(), serde_json::json!(800));
                changed = true;
            }
        }
        if changed && let Ok(out) = serde_json::to_string_pretty(&json) {
            let _ = std::fs::write(&path, out);
        }
    }
}

#[cfg(feature = "desktop")]
/// Ensure the window has valid dimensions and is positioned on a visible monitor.
/// The window-state plugin can persist invalid state (e.g. width/height 0, or
/// positions off-screen) which causes downstream failures like PTY garbage output.
fn ensure_window_visible(window: &WebviewWindow) {
    #[cfg(feature = "desktop")]
    use tauri::PhysicalPosition;

    const MIN_WIDTH: u32 = 800;
    const MIN_HEIGHT: u32 = 600;

    let size = window.outer_size().unwrap_or_default();
    let pos = window.outer_position().unwrap_or_default();

    let size_invalid = size.width < MIN_WIDTH || size.height < MIN_HEIGHT;

    // Check whether the window center is on any available monitor
    // Use saturating conversion to avoid arithmetic overflow on corrupted dimensions
    let half_w = i32::try_from(size.width / 2).unwrap_or(i32::MAX);
    let half_h = i32::try_from(size.height / 2).unwrap_or(i32::MAX);
    let center_x = pos.x.saturating_add(half_w);
    let center_y = pos.y.saturating_add(half_h);
    let on_screen = window
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .any(|m| {
            let mp = m.position();
            let ms = m.size();
            center_x >= mp.x
                && center_x < mp.x + ms.width as i32
                && center_y >= mp.y
                && center_y < mp.y + ms.height as i32
        });

    if size_invalid || !on_screen {
        tracing::warn!(
            width = size.width,
            height = size.height,
            x = pos.x,
            y = pos.y,
            "Invalid window state — resetting to defaults"
        );
        if let Err(e) = window.set_size(tauri::PhysicalSize::new(1200u32, 800u32)) {
            tracing::warn!("Failed to reset window size: {e}");
        }
        if let Err(e) = window.set_position(PhysicalPosition::new(100i32, 100i32)) {
            tracing::warn!("Failed to reset window position: {e}");
        }
        if let Err(e) = window.center() {
            tracing::warn!("Failed to center window: {e}");
        }
    }
}

#[cfg(feature = "desktop")]
/// Load configuration from cached AppState
#[tauri::command]
async fn load_config(app: tauri::AppHandle) -> config::AppConfig {
    let state = app.state::<Arc<AppState>>();
    state.config.read().clone()
}

#[cfg(feature = "desktop")]
mod boot_commands {
    use super::config;

    async fn load_boot_file<T, F>(name: &'static str, loader: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> T + Send + 'static,
    {
        tokio::task::spawn_blocking(loader)
            .await
            .map_err(|error| format!("{name} hydration task failed: {error}"))
    }

    #[tauri::command(rename = "load_repositories")]
    pub(super) async fn load_repositories_async() -> Result<serde_json::Value, String> {
        load_boot_file("repositories", config::load_repositories).await
    }

    #[tauri::command(rename = "load_ui_prefs")]
    pub(super) async fn load_ui_prefs_async() -> Result<config::UIPrefsConfig, String> {
        load_boot_file("UI preferences", config::load_ui_prefs).await
    }

    #[tauri::command(rename = "load_notification_config")]
    pub(super) async fn load_notification_config_async()
    -> Result<config::NotificationConfig, String> {
        load_boot_file("notification config", config::load_notification_config).await
    }

    #[tauri::command(rename = "load_repo_settings")]
    pub(super) async fn load_repo_settings_async() -> Result<config::RepoSettingsMap, String> {
        load_boot_file("repository settings", config::load_repo_settings).await
    }

    #[tauri::command(rename = "load_repo_defaults")]
    pub(super) async fn load_repo_defaults_async() -> Result<config::RepoDefaultsConfig, String> {
        load_boot_file("repository defaults", config::load_repo_defaults).await
    }

    #[tauri::command(rename = "load_prompt_library")]
    pub(super) async fn load_prompt_library_async() -> Result<config::PromptLibraryConfig, String> {
        load_boot_file("prompt library", config::load_prompt_library).await
    }

    #[tauri::command(rename = "load_notes")]
    pub(super) async fn load_notes_async() -> Result<serde_json::Value, String> {
        load_boot_file("notes", config::load_notes).await?
    }

    #[tauri::command(rename = "load_activity")]
    pub(super) async fn load_activity_async() -> Result<serde_json::Value, String> {
        load_boot_file("activity", config::load_activity).await
    }

    #[tauri::command(rename = "load_keybindings")]
    pub(super) async fn load_keybindings_async() -> Result<serde_json::Value, String> {
        load_boot_file("keybindings", config::load_keybindings).await
    }

    #[tauri::command(rename = "load_agents_config")]
    pub(super) async fn load_agents_config_async() -> Result<config::AgentsConfig, String> {
        load_boot_file("agent config", config::load_agents_config).await
    }
}

#[cfg(feature = "desktop")]
/// Save configuration to disk, update the AppState cache, and live-restart the HTTP server
/// if MCP / Remote Access settings changed (no app restart required).
#[tauri::command]
fn save_config(
    state: State<'_, Arc<AppState>>,
    base: config::AppConfig,
    config: config::AppConfig,
) -> Result<(), String> {
    // Serialized read-merge-persist: see config::commit_config_change. Two overlapping
    // saves used to read the same snapshot and the loser's fields were silently dropped.
    let effects = config::commit_config_save(state.inner(), base, config)?;

    if effects.tools_changed {
        let _ = state.mcp.tools_changed.send(());
    }

    if effects.server_changed {
        restart_server(state.inner(), "remote-access configuration changed");
    }

    Ok(())
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn save_app_config(
    state: State<'_, Arc<AppState>>,
    base: config::AppConfig,
    config: config::AppConfig,
) -> Result<(), String> {
    save_config(state, base, config)
}

/// Hash a plaintext password with bcrypt for remote access config
#[cfg_attr(feature = "desktop", tauri::command)]
async fn hash_password(password: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        bcrypt::hash(&password, 12).map_err(|e| format!("Failed to hash password: {e}"))
    })
    .await
    .map_err(|e| format!("spawn_blocking join error: {e}"))?
}

/// Clear all git/GitHub operation caches
#[cfg(feature = "desktop")]
#[tauri::command]
fn clear_caches(state: State<'_, Arc<AppState>>) {
    state.clear_caches();
}

/// Clear git/GitHub caches for a specific repo path
#[cfg(feature = "desktop")]
#[tauri::command]
fn clear_repo_caches(state: State<'_, Arc<AppState>>, path: String) {
    state.invalidate_repo_caches(&path);
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn report_progress_event(
    state: State<'_, Arc<AppState>>,
    project: String,
    report: crate::progress::ProgressReportInput,
) -> Result<crate::progress::ProgressReceipt, String> {
    // A local caller is not an agent: it has no name to attribute and no
    // per-agent override to apply.
    crate::mcp_http::mcp_transport::report_progress(
        state.inner(),
        Some(&project),
        report,
        None,
        None,
        None,
        None,
    )
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn progress_list(
    project: String,
    input: progress::ProgressListInput,
) -> Result<progress::ProgressList, String> {
    progress::progress_list(&project, input)
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn progress_projects() -> Result<Vec<String>, String> {
    progress::progress_projects()
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn progress_delete(
    project: String,
    input: progress::ProgressDeleteInput,
) -> Result<progress::ProgressDeleteReceipt, String> {
    progress::progress_delete(&project, input)
}
#[cfg(feature = "desktop")]
#[tauri::command]
async fn progress_flow(
    state: State<'_, Arc<AppState>>,
    project: String,
    input: progress::ProgressFlowInput,
) -> Result<progress::ProgressFlow, String> {
    progress::progress_flow_blocking(state.inner().clone(), project, input).await
}
#[cfg(feature = "desktop")]
#[tauri::command]
async fn chat_view_snapshot(
    state: State<'_, Arc<AppState>>,
    session_id: String,
    epoch: Option<u64>,
    from_seq: Option<u64>,
) -> Result<chat_view::ChatViewSnapshot, String> {
    chat_view::chat_view_snapshot_blocking(
        state.inner().clone(),
        session_id,
        epoch,
        from_seq.unwrap_or(0),
    )
    .await
}
#[cfg(feature = "desktop")]
#[tauri::command]
async fn progress_flow_detail(
    state: State<'_, Arc<AppState>>,
    input: progress::ProgressFlowDetailInput,
) -> Result<progress::ProgressFlowDetail, String> {
    progress::progress_flow_detail_blocking(state.inner().clone(), input).await
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn progress_mark_viewed(
    project: String,
    pty_id: Option<String>,
) -> Result<progress::ProgressViewedReceipt, String> {
    progress::progress_mark_viewed(&project, pty_id.as_deref())
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn story_capabilities() -> bool {
    true
}

#[cfg(feature = "desktop")]
#[tauri::command]
async fn story_action_command(
    state: State<'_, Arc<AppState>>,
    project: String,
    action: stories::StoryAction,
    session_id: Option<String>,
) -> Result<stories::StoryReply, String> {
    story_action_command_for_state(state.inner().clone(), project, action, session_id).await
}

#[cfg(feature = "desktop")]
async fn story_action_command_for_state(
    state: Arc<AppState>,
    project: String,
    action: stories::StoryAction,
    session_id: Option<String>,
) -> Result<stories::StoryReply, String> {
    tokio::task::spawn_blocking(move || {
        stories::story_action_for_session(&state, &project, action, session_id.as_deref())
    })
    .await
    .map_err(|error| format!("story task failed: {error}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
async fn workflow_definition_action(
    project: String,
    action: workflows::WorkflowAction,
) -> Result<workflows::WorkflowReply, String> {
    tokio::task::spawn_blocking(move || workflows::definition_action(&project, action))
        .await
        .map_err(|error| format!("workflow definition task failed: {error}"))?
}

#[cfg(feature = "desktop")]
#[tauri::command]
async fn workflow_run_action(
    state: State<'_, Arc<AppState>>,
    project: String,
    action: workflows::RunAction,
) -> Result<workflows::RunReply, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || workflows::run_action_with_events(&state, &project, action))
        .await
        .map_err(|error| format!("workflow run task failed: {error}"))?
}

/// Receive a screenshot response from the frontend (captured iframe content).
/// Pairs with the `screenshot-request` Tauri event emitted by `ui(action=screenshot)`.
#[cfg(feature = "desktop")]
#[tauri::command]
fn screenshot_response(state: State<'_, Arc<AppState>>, request_id: String, data: Option<String>) {
    if let Some((_, sender)) = state.screenshot_responses.remove(&request_id) {
        let _ = sender.send(data);
    }
}

/// Answer a pending MCP confirmation. Pairs with `ui(action=confirm)`.
///
/// Every client is shown the same request, so this is a race by design and the
/// first answer wins: a request already resolved (or expired) is a no-op rather
/// than an error, because the loser of that race did nothing wrong.
#[cfg(feature = "desktop")]
#[tauri::command]
fn mcp_confirm_response(state: State<'_, Arc<AppState>>, request_id: String, confirmed: bool) {
    crate::mcp_http::resolve_mcp_confirm(&state, &request_id, confirmed);
}

/// The tab's verdict on a `session action=suspend` request.
#[cfg(feature = "desktop")]
#[tauri::command]
fn session_suspend_response(
    state: State<'_, Arc<AppState>>,
    request_id: String,
    ok: bool,
    reason: Option<String>,
) {
    crate::mcp_http::mcp_transport::resolve_session_suspend(&state, &request_id, ok, reason);
}

/// One IPv4 address found on a network interface.
#[derive(serde::Serialize)]
struct LocalIpEntry {
    ip: String,
    label: String,
}

/// Return all non-loopback IP addresses on this machine, with human-readable labels.
///
/// Uses getifaddrs on Unix (macOS/Linux) to enumerate every interface.
/// On Windows, falls back to the UDP-route trick (returns one address only).
/// When `ipv6_enabled` is true in config, also includes non-loopback, non-link-local IPv6 addresses.
///
/// Labels are classified as:
///   "Tailscale" — 100.64.0.0/10 (CGNAT range Tailscale uses)
///   "Wi-Fi / LAN" — 192.168.x.x or 10.x.x.x with a broadcast address
///   "VPN" — 10.x.x.x point-to-point (no broadcast, /32)
///   "Network" — anything else non-loopback
/// Implementation shared between Tauri command and HTTP handler.
pub(crate) fn get_local_ips_impl(state: &AppState) -> Vec<LocalIpEntry> {
    let ipv6_enabled = state.config.read().services.server.ipv6_enabled;
    get_local_ips_with_config(ipv6_enabled)
}

#[cfg(feature = "desktop")]
#[tauri::command]
fn get_local_ips(state: State<'_, Arc<AppState>>) -> Vec<LocalIpEntry> {
    get_local_ips_impl(&state)
}

fn get_local_ips_with_config(ipv6_enabled: bool) -> Vec<LocalIpEntry> {
    #[cfg(unix)]
    {
        enumerate_unix_ips(ipv6_enabled)
    }
    #[cfg(windows)]
    {
        let mut result = Vec::new();
        use std::net::UdpSocket;
        // IPv4 route trick
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0")
            && sock.connect("8.8.8.8:80").is_ok()
            && let Ok(addr) = sock.local_addr()
        {
            let ip = addr.ip().to_string();
            if !ip.starts_with("127.") {
                result.push(LocalIpEntry {
                    ip,
                    label: "Network".to_string(),
                });
            }
        }
        // IPv6 route trick
        if ipv6_enabled
            && let Ok(sock) = UdpSocket::bind("[::]:0")
            && sock.connect("[2001:4860:4860::8888]:80").is_ok()
            && let Ok(addr) = sock.local_addr()
        {
            let ip_str = addr.ip().to_string();
            if !ip_str.starts_with("::1") {
                let label = classify_ipv6_addr(&addr.ip());
                result.push(LocalIpEntry { ip: ip_str, label });
            }
        }
        result
    }
}

#[cfg(unix)]
fn enumerate_unix_ips(ipv6_enabled: bool) -> Vec<LocalIpEntry> {
    use std::ffi::CStr;
    use std::net::{Ipv4Addr, Ipv6Addr};

    let mut result = Vec::new();
    // SAFETY: `getifaddrs` writes a valid linked list to `ifap` on success (return 0).
    // Each node's `ifa_addr` is checked for null before dereferencing. Pointer casts
    // to `sockaddr_in`/`sockaddr_in6` are valid only after verifying `sa_family`.
    // `freeifaddrs` is called unconditionally after traversal to free the list.
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 {
            return result;
        }
        let mut cur = ifap;
        while !cur.is_null() {
            let ifa = &*cur;
            if !ifa.ifa_addr.is_null() {
                let family = (*ifa.ifa_addr).sa_family as i32;
                let iface_name = || -> String {
                    if ifa.ifa_name.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned()
                    }
                };

                if family == libc::AF_INET {
                    let sa = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                    let raw = u32::from_be(sa.sin_addr.s_addr);
                    let ip = Ipv4Addr::from(raw);
                    if !ip.is_loopback() && !ip.is_link_local() {
                        let iface = iface_name();
                        let has_broadcast = (ifa.ifa_flags & libc::IFF_BROADCAST as u32) != 0;
                        let label = classify_ip(ip, &iface, has_broadcast);
                        result.push(LocalIpEntry {
                            ip: ip.to_string(),
                            label,
                        });
                    }
                } else if ipv6_enabled && family == libc::AF_INET6 {
                    let sa6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                    let ip = Ipv6Addr::from(sa6.sin6_addr.s6_addr);
                    // Skip loopback (::1) and link-local (fe80::/10, requires scope ID)
                    if !ip.is_loopback() && (ip.segments()[0] & 0xffc0) != 0xfe80 {
                        let iface = iface_name();
                        let label = classify_ipv6(ip, &iface);
                        result.push(LocalIpEntry {
                            ip: ip.to_string(),
                            label,
                        });
                    }
                }
            }
            cur = (*cur).ifa_next;
        }
        libc::freeifaddrs(ifap);
    }
    result
}

/// Classify a non-loopback IPv4 address into a human-readable label.
#[cfg(unix)]
fn classify_ip(ip: std::net::Ipv4Addr, iface: &str, has_broadcast: bool) -> String {
    let o = ip.octets();
    // Tailscale: 100.64.0.0 – 100.127.255.255 (CGNAT / RFC 6598)
    if o[0] == 100 && o[1] >= 64 && o[1] <= 127 {
        return format!("Tailscale ({})", iface);
    }
    // 192.168.x.x — always LAN
    if o[0] == 192 && o[1] == 168 {
        return format!("Wi-Fi / LAN ({})", iface);
    }
    // 10.x.x.x — LAN if it has a broadcast address (not point-to-point), else VPN
    if o[0] == 10 {
        if has_broadcast {
            return format!("LAN ({})", iface);
        } else {
            return format!("VPN ({})", iface);
        }
    }
    // 172.16–31.x.x — private LAN
    if o[0] == 172 && o[1] >= 16 && o[1] <= 31 {
        return format!("LAN ({})", iface);
    }
    format!("Network ({})", iface)
}

/// Classify a non-loopback, non-link-local IPv6 address into a human-readable label.
fn classify_ipv6(ip: std::net::Ipv6Addr, iface: &str) -> String {
    let seg = ip.segments();
    // Tailscale IPv6: fd7a:115c:a1e0::/48
    if seg[0] == 0xfd7a && seg[1] == 0x115c && seg[2] == 0xa1e0 {
        return format!("Tailscale ({})", iface);
    }
    // ULA fc00::/7 — private LAN
    if (seg[0] & 0xfe00) == 0xfc00 {
        return format!("LAN ({})", iface);
    }
    // Global unicast
    format!("Network ({})", iface)
}

/// Classify an IPv6 address without interface name (used by Windows UDP trick).
#[cfg(windows)]
fn classify_ipv6_addr(ip: &std::net::IpAddr) -> String {
    match ip {
        std::net::IpAddr::V6(v6) => classify_ipv6(*v6, ""),
        _ => "Network".to_string(),
    }
}

/// Pick preferred IP from a list (Tailscale > Wi-Fi/LAN > any)
pub(crate) fn pick_preferred_ip(ips: Vec<LocalIpEntry>) -> Option<String> {
    for label_prefix in &["Tailscale", "Wi-Fi", "LAN"] {
        if let Some(e) = ips.iter().find(|e| e.label.contains(label_prefix)) {
            return Some(e.ip.clone());
        }
    }
    ips.into_iter().next().map(|e| e.ip)
}

/// Legacy single-IP command kept for backwards compatibility.
/// Returns the LAN/Tailscale IP preferred for remote access, or the default-route IP.
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_local_ip(state: State<'_, Arc<AppState>>) -> Option<String> {
    pick_preferred_ip(get_local_ips(state))
}

/// A markdown file with its git status
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct MarkdownFileEntry {
    pub path: String,
    /// Git status: "modified", "staged", "untracked", or "" (clean).
    pub git_status: String,
    /// Whether the file is listed in .gitignore.
    pub is_ignored: bool,
    /// Last modification time as Unix epoch seconds (0 if unavailable).
    pub modified_at: u64,
}

/// List all markdown files in a repository recursively, with git status (shared logic)
pub(crate) fn list_markdown_files_impl(path: String) -> Result<Vec<MarkdownFileEntry>, String> {
    let repo_path = PathBuf::from(&path);

    if !repo_path.exists() {
        return Err(format!("Path does not exist: {path}"));
    }

    // Walk the filesystem to find all .md files (fast, skips heavy dirs).
    // We avoid `git ls-files --others` which is extremely slow on large repos.
    fn walk_dir(dir: &Path, base: &Path, md_paths: &mut Vec<(String, u64)>) -> std::io::Result<()> {
        if dir.is_dir() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                // Skip hidden directories and common ignore patterns
                if let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && (name.starts_with('.') || name == "node_modules" || name == "target")
                {
                    continue;
                }

                if path.is_dir() {
                    walk_dir(&path, base, md_paths)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("md")
                    && let Ok(relative) = path.strip_prefix(base)
                {
                    let mtime = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    md_paths.push((relative.to_string_lossy().replace('\\', "/"), mtime));
                }
            }
        }
        Ok(())
    }

    let mut md_paths = Vec::new();
    walk_dir(&repo_path, &repo_path, &mut md_paths)
        .map_err(|e| format!("Failed to walk directory: {e}"))?;

    // Get git statuses for .md files only (reuses the same logic as FileBrowser)
    // Passing "" scans whole repo but parse_git_status is fast (single git status call)
    let git_statuses = fs::parse_git_status(&path, "");

    // Detect gitignored paths
    let just_paths: Vec<String> = md_paths.iter().map(|(p, _)| p.clone()).collect();
    let ignored_set = fs::get_ignored_paths(&path, &just_paths);

    // Build entries with status
    let mut entries: Vec<MarkdownFileEntry> = md_paths
        .into_iter()
        .map(|(p, mtime)| {
            let git_status = git_statuses.get(&p).cloned().unwrap_or_default();
            let is_ignored = ignored_set.contains(&p);
            MarkdownFileEntry {
                path: p,
                git_status,
                is_ignored,
                modified_at: mtime,
            }
        })
        .collect();

    // Sort files alphabetically
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn list_markdown_files(path: String) -> Result<Vec<MarkdownFileEntry>, String> {
    list_markdown_files_impl(path)
}

/// Max file size for the generic readers used by the markdown/html-preview/plugin
/// panels. Those panels render the whole payload eagerly (markdown → HTML, etc.),
/// so a large file would freeze them — guarded at the source via `metadata().len()`
/// before reading. (AI-agent reads have their own limit in ai_agent/tools.rs.)
pub(crate) const MAX_EDITOR_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Larger cap for the CodeMirror code editor specifically (`read_editor_file*`).
/// The editor keeps the doc in a CM6 rope and renders only the viewport, so it
/// tolerates far larger files than the eager panels above.
///
/// Hard ceiling for the editor: files above this are refused before reading so a
/// huge payload can't freeze the webview crossing IPC as one string. The 100 MB
/// Tier-1 measurement (`plans/large-file-editor.md` T1.4) showed sub-100 ms JS cost
/// with heap ~1.2× the file; 250 MB trades some headroom for opening bigger files,
/// with the frontend showing a non-blocking "may be slow" warning past 100 MB.
pub(crate) const MAX_EDITOR_LARGE_FILE_SIZE: u64 = 250 * 1024 * 1024;

/// Read a UTF-8 text file, refusing files over `limit` before reading so a huge
/// file can't freeze the webview. The "too large" message is matched by the editor
/// frontend (regex /too large/i) to show a friendly blocking notice, so keep that
/// phrase stable.
fn read_text_file_guarded_with_limit(path: &std::path::Path, limit: u64) -> Result<String, String> {
    if let Ok(meta) = std::fs::metadata(path)
        && meta.len() > limit
    {
        return Err(format!(
            "File too large to open in editor: {:.1} MB (limit {} MB)",
            meta.len() as f64 / (1024.0 * 1024.0),
            limit / (1024 * 1024)
        ));
    }
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))
}

/// Guarded read at the generic [`MAX_EDITOR_FILE_SIZE`] cap (markdown/html/plugin panels).
fn read_text_file_guarded(path: &std::path::Path) -> Result<String, String> {
    read_text_file_guarded_with_limit(path, MAX_EDITOR_FILE_SIZE)
}

/// Read file content within a repo (shared logic), guarded at `limit`.
pub(crate) fn read_file_impl_with_limit(
    path: String,
    file: String,
    limit: u64,
) -> Result<String, String> {
    let repo_path = PathBuf::from(&path);
    let file_path = repo_path.join(&file);

    let canonical_repo = repo_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repo path: {e}"))?;

    // Security: ensure the file is within the repo path
    let canonical_file = file_path
        .canonicalize()
        .map_err(|e| format!("Failed to resolve file path: {e}"))?;

    if !canonical_file.starts_with(&canonical_repo) {
        return Err("Access denied: file is outside repository".to_string());
    }

    read_text_file_guarded_with_limit(&file_path, limit)
}

/// Read file content (shared logic) at the generic [`MAX_EDITOR_FILE_SIZE`] cap.
pub(crate) fn read_file_impl(path: String, file: String) -> Result<String, String> {
    read_file_impl_with_limit(path, file, MAX_EDITOR_FILE_SIZE)
}

#[cfg_attr(feature = "desktop", tauri::command)]
async fn read_file(path: String, file: String) -> Result<String, String> {
    fs::spawn_blocking_fs(move || read_file_impl(path, file)).await
}

/// Read a repo file for the CodeMirror editor, at the larger
/// [`MAX_EDITOR_LARGE_FILE_SIZE`] cap. Same repo-containment check as `read_file`.
#[cfg_attr(feature = "desktop", tauri::command)]
async fn read_editor_file(repo_path: String, file: String) -> Result<String, String> {
    fs::spawn_blocking_fs(move || {
        read_file_impl_with_limit(repo_path, file, MAX_EDITOR_LARGE_FILE_SIZE)
    })
    .await
}

/// Read a file by absolute path (read-only, no repo constraint).
/// Used for viewing files outside the active repository (e.g. drag & drop).
///
/// No TCC directory blocking: reading a specific file by known path does not
/// trigger macOS permission dialogs (TCC guards directory enumeration, not
/// individual reads). The HTTP endpoint has its own repo-root check.
#[cfg_attr(feature = "desktop", tauri::command)]
async fn read_external_file(path: String) -> Result<String, String> {
    fs::spawn_blocking_fs(move || read_external_file_impl(&path)).await
}

pub(crate) fn read_external_file_impl(path: &str) -> Result<String, String> {
    let p = std::path::Path::new(path);
    if !p.is_absolute() {
        return Err("read_external_file requires an absolute path".to_string());
    }
    read_text_file_guarded(p)
}

/// Read an absolute-path file (read-only) guarded at `limit`. Shared by the
/// `read_editor_file_external` command and its HTTP route.
pub(crate) fn read_external_file_with_limit(path: &str, limit: u64) -> Result<String, String> {
    let p = std::path::Path::new(path);
    if !p.is_absolute() {
        return Err("read_editor_file_external requires an absolute path".to_string());
    }
    read_text_file_guarded_with_limit(p, limit)
}

/// Read an absolute-path file for the CodeMirror editor, at the larger
/// [`MAX_EDITOR_LARGE_FILE_SIZE`] cap. The HTTP endpoint has its own repo-root check.
#[cfg_attr(feature = "desktop", tauri::command)]
async fn read_editor_file_external(path: String) -> Result<String, String> {
    fs::spawn_blocking_fs(move || read_external_file_with_limit(&path, MAX_EDITOR_LARGE_FILE_SIZE))
        .await
}

/// Write a file at an absolute path (used by the UI for files outside any registered repo,
/// e.g. markdown files opened via absolute path without a git root).
///
/// Target must be inside the user's home directory — see
/// [`crate::fs::validate_external_write_path`] for the full rationale (story 1273-c95e).
#[cfg_attr(feature = "desktop", tauri::command)]
async fn write_external_file(path: String, content: String) -> Result<(), String> {
    fs::spawn_blocking_fs(move || write_external_file_impl(path, content)).await
}

pub(crate) fn write_external_file_impl(path: String, content: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let home =
        dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    fs::validate_external_write_path(p, &home)?;
    fs::atomic_write(p, content.as_bytes()).map_err(|e| format!("Failed to write file: {e}"))
}

/// Get MCP server status (running, port, active sessions).
/// Async to avoid blocking the Tauri IPC thread during the TCP self-test.
#[cfg(feature = "desktop")]
#[tauri::command]
async fn get_mcp_status(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // Collect config and session count synchronously first (fast, no I/O)
    let (remote_enabled, active_sessions, mcp_protocol_sessions) = {
        let cfg = state.config.read();
        (
            cfg.services.server.enabled,
            state.session_maps.sessions.len(),
            state.mcp.sessions.len(),
        )
    };

    // Check if the Unix socket is alive with a real connect attempt.
    // file.exists() is unreliable — a stale socket from a crashed run passes
    // the file check but refuses connections.
    #[cfg(unix)]
    let running = tokio::net::UnixStream::connect(mcp_http::socket_path())
        .await
        .is_ok();
    #[cfg(not(unix))]
    let running = false;

    // TCP reachability self-test for remote access
    let remote_port = state.config.read().services.server.port;
    let reachable = if remote_enabled {
        let preferred_ip = pick_preferred_ip(get_local_ips_with_config(
            state.config.read().services.server.ipv6_enabled,
        ));
        if let Some(ip) = preferred_ip {
            let port = remote_port;
            let addr = if ip.contains(':') {
                format!("[{ip}]:{port}")
            } else {
                format!("{ip}:{port}")
            };
            tokio::task::spawn_blocking(move || {
                addr.parse::<std::net::SocketAddr>().ok().map(|sa| {
                    std::net::TcpStream::connect_timeout(&sa, std::time::Duration::from_millis(200))
                        .is_ok()
                })
            })
            .await
            .ok()
            .flatten()
        } else {
            None
        }
    } else {
        None
    };

    Ok(serde_json::json!({
        "native_tools": mcp_http::mcp_transport::native_tool_catalog(),
        "enabled": true,
        "running": running,
        "remote_port": if remote_enabled { Some(remote_port) } else { None },
        "active_sessions": active_sessions,
        "mcp_clients": mcp_protocol_sessions,
        "max_sessions": MAX_CONCURRENT_SESSIONS,
        "reachable": reachable,
    }))
}

/// Execute an MCP tool call via deep link: `tuic://cmd/{tool}/{action}?{params}`.
/// Reuses the same dispatch as the MCP `tools/call` handler — no HTTP round-trip.
#[cfg(feature = "desktop")]
#[tauri::command]
async fn deep_link_mcp_call(
    state: State<'_, Arc<AppState>>,
    tool: String,
    action: String,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
    // Build the args object: merge action into params
    let mut args = match params {
        serde_json::Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    // Defense-in-depth backstop: deep-link-handler.ts is the enforcement boundary
    // (default-deny + confirm dialog), but mirror its hard BLOCKED set here so these
    // can never run via a tuic:// URL even if the frontend allowlist regresses.
    // config/save mutates app config; debug/invoke_js is arbitrary JS execution.
    if matches!(
        (tool.as_str(), action.as_str()),
        ("config", "save") | ("debug", "invoke_js")
    ) {
        return Ok(serde_json::json!({
            "error": "This command is not permitted via deep link"
        }));
    }

    args.insert("action".to_string(), serde_json::Value::String(action));

    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 0).into();
    let result = Box::pin(mcp_http::mcp_transport::handle_mcp_tool_call(
        &state.inner().clone(),
        addr,
        &tool,
        &serde_json::Value::Object(args),
        None,
    ))
    .await;

    Ok(result)
}

/// Regenerate the session token, invalidating all existing remote sessions.
#[cfg(feature = "desktop")]
#[tauri::command]
fn regenerate_session_token(state: State<'_, Arc<AppState>>) {
    if let Err(e) = config::rotate_session_token(state.inner()) {
        tracing::error!(
            source = "auth",
            "Failed to persist regenerated session token: {e}"
        );
    }
}

/// Build a QR-code connect URL server-side, selecting scheme/host from the
/// current Tailscale + TLS state. The returned URL embeds the raw session
/// token as a `?token=` query param, so the token IS exposed to the JS caller.
/// Uses HTTPS + Tailscale FQDN when TLS is active on a Tailscale IP.
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_connect_url(state: State<'_, Arc<AppState>>, ip: String) -> String {
    let port = state.config.read().services.server.port;
    let token = state.session_token.read().clone();

    // If TLS is active and the IP is a Tailscale address, use https + FQDN
    let ts = state.tailscale_state.read().clone();
    if let tailscale::TailscaleState::Running {
        ref fqdn,
        https_enabled: true,
    } = ts
        && crate::mcp_http::auth::is_tailscale_ip(&ip)
    {
        return build_connect_url("https", fqdn, port, &token);
    }

    build_connect_url("http", &ip, port, &token)
}

/// Get Tailscale daemon status for the frontend Settings panel.
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_tailscale_status(state: State<'_, Arc<AppState>>) -> tailscale::TailscaleState {
    state.tailscale_state.read().clone()
}

#[cfg(feature = "desktop")]
/// Provision TLS config from current Tailscale state.
/// Returns Some(RustlsConfig) if Tailscale is running with HTTPS enabled and cert provisioning succeeds.
async fn provision_tls_config(
    ts_state: &tailscale::TailscaleState,
) -> Option<axum_server::tls_rustls::RustlsConfig> {
    if let tailscale::TailscaleState::Running {
        fqdn,
        https_enabled: true,
    } = ts_state
    {
        match tailscale::provision_cert(fqdn).await {
            Ok((cert_pem, key_pem)) => {
                match axum_server::tls_rustls::RustlsConfig::from_pem(cert_pem, key_pem).await {
                    Ok(tls) => {
                        tracing::info!(source = "tailscale", fqdn, "TLS cert provisioned");
                        return Some(tls);
                    }
                    Err(e) => {
                        tracing::error!(source = "tailscale", "Failed to load TLS config: {e}")
                    }
                }
            }
            Err(e) => tracing::error!(source = "tailscale", "Failed to provision cert: {e}"),
        }
    }
    None
}

#[cfg(feature = "desktop")]
/// Restart the HTTP/MCP server with fresh TLS config (reuses the shutdown/spawn pattern from save_config).
fn restart_server(state: &Arc<AppState>, reason: &'static str) {
    tracing::info!(
        source = "mcp_http",
        reason,
        remote_enabled = state.config.read().services.server.enabled,
        "HTTP server reconfiguration requested; local MCP IPC remains active"
    );
    // Shutdown existing server
    if let Some(tx) = state.server_shutdown.lock().take()
        && tx.send(()).is_err()
    {
        tracing::warn!(
            source = "mcp_http",
            reason,
            "Previous TCP server lifecycle had already stopped"
        );
    }
    let remote_enabled = state.config.read().services.server.enabled;
    let state_arc = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime for HTTP server restart");
        rt.block_on(async move {
            let ts = state_arc.tailscale_state.read().clone();
            let tls_config = provision_tls_config(&ts).await;
            mcp_http::start_server(state_arc, true, remote_enabled, tls_config).await;
        });
    });
}

/// Run the initial server future without letting its owning Tokio runtime die
/// after a TCP restart. Always-on IPC/background tasks are children of that
/// runtime and must live for the process lifetime.
async fn keep_server_owner_runtime_alive<F>(server: F)
where
    F: std::future::Future,
{
    let _ = server.await;
    std::future::pending::<()>().await;
}

/// Re-detect Tailscale daemon status and restart server if HTTPS availability changed.
#[cfg(feature = "desktop")]
#[tauri::command]
async fn recheck_tailscale_status(
    state: State<'_, Arc<AppState>>,
) -> Result<tailscale::TailscaleState, String> {
    let old_https = matches!(
        *state.tailscale_state.read(),
        tailscale::TailscaleState::Running {
            https_enabled: true,
            ..
        }
    );

    let new_state = tokio::task::spawn_blocking(tailscale::detect)
        .await
        .map_err(|e| format!("detect task failed: {e}"))?;

    let new_https = matches!(
        new_state,
        tailscale::TailscaleState::Running {
            https_enabled: true,
            ..
        }
    );

    *state.tailscale_state.write() = new_state.clone();

    // Restart server if HTTPS availability changed (HTTP→HTTPS or HTTPS→HTTP)
    if old_https != new_https && state.config.read().services.server.enabled {
        tracing::info!(
            source = "tailscale",
            old_https,
            new_https,
            "HTTPS state changed, restarting server"
        );
        restart_server(&state, "Tailscale HTTPS availability changed");
    }

    Ok(new_state)
}

/// Relay client status (enabled, connected, url, session_id).
///
/// Shared by the `get_relay_status` command and `GET /system/relay-status`:
/// one body means the two transports cannot answer different shapes.
#[cfg(feature = "desktop")]
pub(crate) fn relay_status_json(state: &AppState) -> serde_json::Value {
    let cfg = state.config.read();
    let connected = state
        .relay
        .connected
        .load(std::sync::atomic::Ordering::Relaxed);
    serde_json::json!({
        "enabled": cfg.services.relay.enabled,
        "connected": connected,
        "url": cfg.services.relay.url,
        "session_id": cfg.services.relay.session_id,
    })
}

/// Get relay client status (enabled, connected, url, session_id).
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_relay_status(state: State<'_, Arc<AppState>>) -> serde_json::Value {
    relay_status_json(&state)
}

/// Raise the open-file descriptor soft limit toward the hard limit.
///
/// No-op when the current soft limit already meets the target (e.g. launched
/// from a terminal that inherited a high ulimit). On non-Unix this does nothing.
#[cfg(unix)]
fn raise_fd_limit() {
    // macOS caps per-process descriptors at kern.maxfilesperproc (≈138k here);
    // 64k is comfortably below that and far above our steady state (~95) plus
    // any realistic git fan-out.
    const DESIRED: libc::rlim_t = 65_536;
    let mut lim = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) } != 0 {
        tracing::warn!(
            source = "boot",
            "getrlimit(RLIMIT_NOFILE) failed; leaving FD limit unchanged"
        );
        return;
    }
    let target = if lim.rlim_max == libc::RLIM_INFINITY {
        DESIRED
    } else {
        DESIRED.min(lim.rlim_max)
    };
    if lim.rlim_cur >= target {
        return;
    }
    let old = lim.rlim_cur;
    lim.rlim_cur = target;
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) } == 0 {
        tracing::info!(source = "boot", "Raised FD soft limit {old} → {target}");
    } else {
        tracing::warn!(
            source = "boot",
            "setrlimit(RLIMIT_NOFILE) {old} → {target} failed"
        );
    }
}

#[cfg(not(unix))]
fn raise_fd_limit() {}

const TAILSCALE_DETECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

async fn detect_tailscale_bounded<F>(detection: F) -> tailscale::TailscaleState
where
    F: std::future::Future<Output = tailscale::TailscaleState>,
{
    match tokio::time::timeout(TAILSCALE_DETECTION_TIMEOUT, detection).await {
        Ok(state) => state,
        Err(_) => {
            tracing::warn!(
                source = "tailscale",
                timeout_ms = TAILSCALE_DETECTION_TIMEOUT.as_millis(),
                "Tailscale detection timed out; starting the local server without TLS"
            );
            tailscale::TailscaleState::NotInstalled
        }
    }
}

fn boot_repo_paths(repositories: &serde_json::Value) -> Vec<String> {
    repositories
        .get("repos")
        .and_then(|repos| repos.as_object())
        .into_iter()
        .flat_map(|repos| repos.iter())
        .filter(|(_, repo)| repo.get("parked").and_then(|value| value.as_bool()) != Some(true))
        .map(|(path, _)| path.clone())
        .collect()
}

/// Which repos to pre-warm a content index for at boot, in order.
///
/// - `active_only` / `active_and_switch`: the active repo, if it is known.
/// - `all_sequential`: every known repo, active first.
///
/// Pure, and shared by both boot paths on purpose. The desktop spawns the warm
/// loop through Tauri's runtime and the daemon through tokio's, but *which*
/// repos get an index must not be able to differ between them: a repo that is
/// never warmed on the daemon is a repo cross-repo search skips forever, since
/// `search_content_all` reports it pending and deliberately starts no build.
fn repos_to_prewarm(
    mut known: Vec<String>,
    active: Option<String>,
    index_strategy: &str,
) -> Vec<String> {
    if known.is_empty() {
        return Vec::new();
    }
    match index_strategy {
        "all_sequential" => {
            if let Some(ref active) = active
                && let Some(pos) = known.iter().position(|p| p == active)
            {
                known.swap(0, pos);
            }
            known
        }
        _ => match active {
            Some(active) if known.contains(&active) => vec![active],
            _ => Vec::new(),
        },
    }
}

/// Build the content index of each repo in turn, one at a time.
///
/// The two second delay keeps the first build off the boot path, where it would
/// compete with the window (desktop) or the socket bind (daemon).
async fn prewarm_content_indices(state: Arc<AppState>, repos: Vec<String>) {
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    for repo in repos {
        let index_arc = crate::content_index::ensure_index(&state, &repo);
        while !index_arc.read().is_ready() {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }
    tracing::info!("content index pre-warm complete");
}

#[cfg(feature = "desktop")]
fn is_app_navigation(url: &tauri::Url, dev_url: Option<&tauri::Url>) -> bool {
    // Wry invokes this callback for subframes on macOS and Linux. Keep the
    // origins and schemes used by previews, plugin panels and downloads here;
    // rendered Markdown links are intercepted before they can navigate a frame.
    let bundled_origin = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (cfg!(windows)
            && url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    let internal_frame = matches!(url.scheme(), "asset" | "plugin")
        || (url.scheme() == "http"
            && matches!(url.host_str(), Some("asset.localhost" | "plugin.localhost")))
        || (url.scheme() == "about" && matches!(url.path(), "blank" | "srcdoc"))
        || matches!(url.scheme(), "data" | "blob")
        || (matches!(url.scheme(), "http" | "https")
            && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")));
    #[cfg(debug_assertions)]
    {
        bundled_origin || internal_frame || dev_url.is_some_and(|dev| url.origin() == dev.origin())
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = dev_url;
        bundled_origin || internal_frame
    }
}

#[cfg(feature = "desktop")]
fn release_webview_document_resources(state: &AppState, webview_label: &str) {
    let mut removed = Vec::new();
    state.grid.channels.retain(|session_id, subscription| {
        if subscription.webview_label == webview_label {
            removed.push((session_id.clone(), subscription.epoch));
            false
        } else {
            true
        }
    });
    for (session_id, epoch) in removed {
        state
            .grid
            .gates
            .remove_if(&session_id, |_, gate| gate.epoch() == epoch);
    }
    state
        .plugin_output_watchers
        .write()
        .remove_webview(webview_label);
}

#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must run before the first `config::config_dir()` read (below) — see
    // `app_instance::select_app_instance_from_env` for why this exists: a
    // debug/test build launched with `TUIC_APP_INSTANCE=<id>` gets its own
    // isolated config directory instead of sharing Boss's production
    // `repositories.json` (#763-d219).
    if let Err(e) = app_instance::select_app_instance_from_env() {
        eprintln!("Invalid {}: {e}", app_instance::APP_INSTANCE_ENV_VAR);
        std::process::exit(1);
    }

    // Install the rustls CryptoProvider before anything touches TLS.
    // With both `ring` and `aws-lc-rs` features active, rustls cannot
    // auto-detect which provider to use and panics at runtime.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    // Create the shared log ring buffer and initialise the tracing subscriber.
    // This must happen before any other code so all log output is captured —
    // including config loading, migration warnings, etc.
    let log_buffer = Arc::new(parking_lot::Mutex::new(app_logger::LogRingBuffer::new(
        app_logger::LOG_RING_CAPACITY,
    )));
    app_logger::init_tracing(log_buffer.clone());

    // Raise the open-file descriptor soft limit before any watcher or subprocess
    // fan-out starts. macOS GUI apps launched via launchd inherit a soft
    // RLIMIT_NOFILE of just 256 (`launchctl limit maxfiles`). TUIC spawns many
    // subprocesses (git, agents, PTYs) and watches many repos; a repo-change
    // burst can fan out enough concurrent git pipes to cross 256 → EMFILE
    // ("Too many open files"). Best-effort: logs and continues on failure.
    raise_fd_limit();

    // Default worktrees directory: <config_dir>/worktrees
    let worktrees_dir = config::config_dir().join("worktrees");

    let mut config = config::load_app_config();

    // Auto-generate VAPID keys and session token on first run
    let mut config_dirty = false;
    if config.services.push.vapid_private_key.is_empty() {
        match push::generate_vapid_keys() {
            Ok((private, public)) => {
                tracing::info!(source = "push", "Generated VAPID key pair");
                config.services.push.vapid_private_key = private;
                config.services.push.vapid_private_key_exists = true;
                config.services.push.vapid_public_key = public;
                config_dirty = true;
            }
            Err(e) => {
                tracing::error!(source = "push", "Failed to generate VAPID keys: {e}");
            }
        }
    }
    if config.services.auth.session_token.is_empty() {
        config.services.auth.session_token = uuid::Uuid::new_v4().to_string();
        config.services.auth.session_token_exists = true;
        tracing::info!(source = "auth", "Generated persistent session token");
        config_dirty = true;
    }
    if config_dirty && let Err(e) = config::save_app_config(config.clone()) {
        tracing::error!(source = "app", "Failed to persist config: {e}");
    }

    // Boot reads the environment and nothing else. Every other token source
    // spawns `gh auth token` or reads the OS credential store, neither of which
    // is guaranteed to answer, and this runs before the window is built — a
    // wedged `gh` used to mean no window at all. `setup()` finishes the chain
    // once the window exists.
    let (github_token, github_token_source) = crate::github_auth::resolve_token_from_env();

    let data_dir = config::config_dir();

    agent_hook_launch::regenerate_launch_assets_at_boot(&data_dir);

    let mut app_state = AppState::new(data_dir, worktrees_dir, config.clone(), log_buffer);
    *app_state.github.token.get_mut() = github_token;
    *app_state.github.token_source.get_mut() = github_token_source;

    let state = Arc::new(app_state);
    state.wire_event_bus();

    // The relay supervisor runs whether or not the relay is enabled at boot:
    // "off" is a state it supervises, and spawning it only when the setting was
    // already on is what made the Settings toggle need an app restart.
    let (relay_tx, relay_rx) = tokio::sync::oneshot::channel();
    *state.relay.shutdown.lock() = Some(relay_tx);

    // Always start HTTP API server (Unix socket is always on; TCP only if remote access enabled)
    // Tailscale detection + TLS provisioning happens inside the server thread (non-blocking to Tauri setup)
    {
        let remote_enabled = config.services.server.enabled;
        let server_state = state.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime for HTTP server");
            rt.block_on(async move {
                spawn_background_tasks(&server_state);

                tokio::spawn(relay_client::supervise(server_state.clone(), relay_rx));

                // Detect Tailscale and provision TLS cert (async, doesn't block window render)
                let tls_config = if remote_enabled {
                    let ts_state = detect_tailscale_bounded(async {
                        tokio::task::spawn_blocking(tailscale::detect)
                            .await
                            .unwrap_or(tailscale::TailscaleState::NotInstalled)
                    })
                    .await;
                    tracing::info!(
                        source = "tailscale",
                        ?ts_state,
                        "Tailscale detection result"
                    );
                    *server_state.tailscale_state.write() = ts_state.clone();
                    provision_tls_config(&ts_state).await
                } else {
                    None
                };

                // `start_server` binds the IPC socket and then parks on the
                // shutdown signal — it only returns on save_config/restart, so
                // it must be the call that owns this boot thread's runtime for
                // the process lifetime. Auto-connect therefore CANNOT run after
                // it (that line would be dead code, leaving every upstream
                // unconnected until the user touches the UI). Spawn auto-connect
                // to run concurrently: it registers upstreams + spawns their
                // async init (it does not await slow network/OAuth), so it never
                // delays socket binding — keeping the MCP bridge reachable for
                // Claude Code while still connecting saved upstreams at boot.
                let auto_state = server_state.clone();
                let settle_guard = auto_state.clone();
                // Remote machines come up on their own too, for the same reason
                // upstreams do: a machine the user registered is one they expect
                // to be there. Each gets a supervisor that connects and then
                // keeps retrying, so this returns at once and nothing here waits
                // on a machine that is asleep.
                crate::remote_runtime::autoconnect_all(&auto_state);
                let auto_handle = tokio::spawn(async move {
                    crate::mcp_upstream_config::auto_connect_saved_upstreams(&auto_state).await;
                });
                // If the auto-connect task panics it would never call
                // mark_initial_connect_complete(), leaving every tools/list to
                // block for the full settle timeout with no log. Watch the handle
                // and recover the latch on failure.
                tokio::spawn(async move {
                    if let Err(e) = auto_handle.await {
                        tracing::error!(
                            source = "mcp_upstream",
                            "auto_connect_saved_upstreams task failed: {e}"
                        );
                        settle_guard
                            .mcp
                            .upstream_registry
                            .mark_initial_connect_complete();
                    }
                });

                let srv_state = server_state.clone();
                keep_server_owner_runtime_alive(mcp_http::start_server(
                    srv_state,
                    true,
                    remote_enabled,
                    tls_config,
                ))
                .await;
            });
        });
    }

    // Ensure MCP bridge config is installed and up-to-date in all agent configs.
    // Runs every launch: installs missing entries and updates stale paths.
    // Skips agents the user explicitly disabled via Settings > Agents.
    agent_mcp::ensure_mcp_configs(&config.disabled_mcp_agents);

    sanitize_window_state();

    let index_strategy = config.index_strategy.clone();
    let builder = tauri::Builder::default();
    let builder = plugins::register_plugin_protocol(builder);
    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("navigation-guard")
                .on_navigation(|webview, url| {
                    // Wry also calls this for subframes on macOS/Linux; allow
                    // internal preview/panel origins while refusing external
                    // top-document navigation. Explicit opens use the opener.
                    let dev_url = webview.app_handle().config().build.dev_url.as_ref();
                    if is_app_navigation(url, dev_url) {
                        return true;
                    }
                    // The callback has no user-gesture or frame identity. Tell the
                    // UI what was blocked, but never open it from here: scripts in
                    // an embedded dashboard could otherwise spam the OS browser.
                    if matches!(url.scheme(), "http" | "https" | "mailto") {
                        let _ = webview.emit("navigation-blocked", url.as_str());
                    }
                    tracing::debug!(url = %url, "Blocked implicit WebView navigation");
                    false
                })
                .on_page_load(|webview, payload| {
                    if payload.event() == tauri::webview::PageLoadEvent::Started
                        && let Some(state) = webview.try_state::<Arc<AppState>>()
                    {
                        release_webview_document_resources(state.inner(), webview.label());
                    }
                    // A WebContent crash leaves the WebView on about:blank; the
                    // 2026-09-08 standby incident left it on about:srcdoc. Both
                    // are blank top documents with no URL behind them, so both
                    // are recovered the same way — see `webview_recovery`.
                    if payload.event() == tauri::webview::PageLoadEvent::Finished
                        && webview_recovery::is_lost(payload.url().as_str())
                    {
                        tracing::error!(
                            source = "webview",
                            label = webview.label(),
                            url = %payload.url(),
                            "WebView landed on a blank document — navigating back to the app"
                        );
                        let handle = webview.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            // The last healthy URL, not a hardcoded one: the
                            // previous `tauri://localhost/` was never the dev
                            // server's address, so this hook could not recover
                            // a `make dev` window at all.
                            let state: tauri::State<'_, Arc<AppState>> = handle.state();
                            let _ =
                                webview_recovery::navigate_home(state.inner(), "page_load_hook");
                        });
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_process::init())
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
        .manage(crate::fs::ContentSearchCancel(std::sync::Mutex::new(None)))
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed)
                && let Some(state) = window.try_state::<Arc<AppState>>()
            {
                release_webview_document_resources(state.inner(), window.label());
            }
        })
        .plugin(tauri_plugin_deep_link::init())
        .manage(native_clipboard::NativeClipboard::default());

    #[cfg(feature = "desktop")]
    let builder = builder.manage(sleep_prevention::SleepBlocker::new());

    #[cfg(feature = "dictation")]
    let builder = builder.manage(dictation::DictationState::new());

    // Single-instance lock only in release builds — allows tauri dev to run
    // alongside the installed TUIC-preview.app (they share the same identifier).
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }));

    builder
        .setup(move |app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;

            #[cfg(feature = "desktop")]
            {
                let m = menu::build_menu(app)?;
                app.set_menu(m)?;
                app.on_menu_event(|app_handle, event| {
                    let _ = app_handle.emit("menu-action", event.id().0.as_str());
                });
            }

            // Store AppHandle so HTTP handlers can emit Tauri events
            let app_state: &Arc<AppState> = app.state::<Arc<AppState>>().inner();
            *app_state.app_handle.write() = Some(app.handle().clone());

            // Ensure main window exists — if tauri.conf.json windows[] is
            // empty (accidental edit, merge conflict), create it programmatically
            // so the app doesn't start as a headless dock icon.
            if app.get_webview_window("main").is_none() {
                tracing::warn!("Main window missing from config — creating programmatically");
                let builder = tauri::WebviewWindowBuilder::new(
                    app,
                    "main",
                    tauri::WebviewUrl::App("index.html".into()),
                )
                .title("TUICommander")
                .inner_size(1200.0, 800.0)
                .min_inner_size(800.0, 600.0)
                .decorations(true)
                .resizable(true);
                // hidden_title / title_bar_style are macOS-only builder methods.
                #[cfg(target_os = "macos")]
                let builder = builder
                    .hidden_title(true)
                    .title_bar_style(tauri::TitleBarStyle::Overlay);
                builder.build()?;
            }

            // Track desktop window focus so push notifications can be
            // suppressed while the user is at their machine.
            if let Some(window) = app.get_webview_window("main") {
                let push_flag = Arc::clone(app_state);
                #[cfg(all(target_os = "macos", feature = "dictation"))]
                let focus_app = app.handle().clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(focused) = event {
                        push_flag
                            .desktop_window_focused
                            .store(*focused, std::sync::atomic::Ordering::Relaxed);
                        #[cfg(all(target_os = "macos", feature = "dictation"))]
                        if !focused {
                            dictation::fn_key_monitor::release_on_focus_loss(&focus_app);
                        }
                    }
                });
            }

            #[cfg(feature = "desktop")]
            {
                #[cfg(feature = "dictation")]
                let input_plan = {
                    let dictation_state = app.state::<dictation::DictationState>();
                    dictation_state.claim_ownership(&config::config_dir());
                    dictation::ownership::GlobalInputPlan::for_ownership(dictation_state.is_owner())
                };
                #[cfg(feature = "dictation")]
                let restore_hotkey = input_plan.restore_hotkey;
                #[cfg(not(feature = "dictation"))]
                let restore_hotkey = {
                    let ownership = input_ownership::Ownership::acquire(&config::config_dir());
                    let is_owner = ownership.is_owner();
                    tracing::info!(source = "dictation", path = %ownership.path().display(), owner = is_owner, "Global input ownership");
                    app.manage(ownership);
                    is_owner
                };

                // The same config-directory lock guards global shortcuts with or without voice.
                if let Err(e) = global_hotkey::init(app.handle()) {
                    tracing::warn!(source = "global-hotkey", "Failed to init plugin: {e}");
                } else if restore_hotkey {
                    global_hotkey::restore_from_config(app.handle());
                }

                #[cfg(feature = "dictation")]
                {
                    // Install Fn/Globe key monitor for push-to-talk dictation
                    if input_plan.install_fn_monitor {
                        dictation::fn_key_monitor::install(app.handle().clone());
                    }
                    dictation::spawn_idle_unload_sweeper(app.handle().clone());
                    // Before any conversation can be armed: a speaker built without
                    // this one reports its replies to nobody but a poller.
                    dictation::commands::install_utterance_observer(app.handle());
                }
            }

            #[cfg(feature = "desktop")]
            {
                // Install the native key monitor (macOS swallows Ctrl+Tab and F13-F20
                // before JS/WKWebView ever sees them)
                native_keys::install(app.handle().clone());

                // Disable macOS press-and-hold accent popup so held keys repeat
                // in the terminal's hidden input (vim j/l/i — issue #79)
                press_and_hold::disable();
            }

            // Seed built-in themes on first run, then start hot-reload watcher
            let themes_dir = config::config_dir().join("themes");
            if let Err(e) = themes::seed_builtin_themes(&themes_dir) {
                tracing::warn!("Failed to seed built-in themes: {e}");
            }
            themes::start_theme_watcher(themes_dir, app_state);

            // Former built-ins are seeded once as ordinary uninstallable plugins.
            // Seed before the watcher starts so startup does not emit redundant
            // hot-reload events for packages the frontend has not loaded yet.
            if let Err(e) = plugins::seed_externalized_builtin_plugins(&config::config_dir()) {
                tracing::warn!(
                    source = "plugins",
                    "Failed to seed externalized plugins: {e}"
                );
            }

            // Start plugin directory watcher for hot-reload
            plugins::start_plugin_watcher(app.handle());

            // Auto-start repo watchers for known repositories.
            // Uses raw notify::RecommendedWatcher — registration is instant on
            // macOS (FSEvents) and Windows (ReadDirectoryChangesW). On Linux
            // (inotify) notify emulates recursion with a per-directory walk, so
            // registration is not free there (see issue #82 / repo_watcher.rs).
            let repos_json = config::load_repositories();
            let known_repo_paths = boot_repo_paths(&repos_json);
            for repo_path in &known_repo_paths {
                if let Err(e) = repo_watcher::start_watching(repo_path, app_state) {
                    app_logger::log_via_state(
                        app_state,
                        "warn",
                        "app",
                        &format!("[RepoWatcher] Failed to watch {repo_path}: {e}"),
                    );
                }
            }

            // Auto-update CLI binary if installed
            #[cfg(feature = "desktop")]
            tauri::async_runtime::spawn(async {
                if let Err(error) = tokio::task::spawn_blocking(tuic_cli::auto_update_cli).await {
                    tracing::warn!(source = "tuic_cli", "CLI auto-update task failed: {error}");
                }
            });

            // Finish GitHub token resolution now the window exists. Boot only
            // took the env vars; the keychain/`gh` part of the chain runs here,
            // off the window path and under its own timeout.
            crate::github_auth::spawn_deferred_token_resolution(Arc::clone(app_state));

            // Pre-warm content indices per the `index_strategy` setting. The
            // global semaphore in AppState (capacity 1) serialises the builds.
            let repos_to_warm = repos_to_prewarm(
                known_repo_paths,
                repos_json
                    .get("activeRepoPath")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned()),
                &index_strategy,
            );
            if !repos_to_warm.is_empty() {
                let state_for_prewarm = Arc::clone(app_state);
                tauri::async_runtime::spawn(prewarm_content_indices(
                    state_for_prewarm,
                    repos_to_warm,
                ));
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            telegram::settings::telegram_settings,
            telegram::settings::telegram_setup,
            generators::generate_value,
            native_dialog::pick_path,
            native_clipboard::write_clipboard_text,
            native_clipboard::read_clipboard_text,
            native_drag::start_native_drag,
            remote_connection::list_remote_connections,
            remote_connection::save_remote_connection,
            remote_connection::delete_remote_connection,
            remote_connection::set_remote_connection_password,
            remote_connection::remote_connection_password_exists,
            remote_connection::fetch_remote_connection_token,
            remote_runtime::connect_remote_connection,
            remote_runtime::disconnect_remote_connection,
            remote_runtime::remote_connection_statuses,
            remote_update::prepare_remote_update,
            remote_update::update_and_restart_remote,
            remote_deploy::service::install_remote_daemon,
            remote_deploy::service::uninstall_remote_daemon,
            open_secondary_window,
            native_notification::show_native_notification,
            panel_window::open_panel_window,
            panel_window::focus_panel_window,
            panel_window::close_panel_window,
            panel_window::focus_main_window,
            pty::create_pty,
            pty::create_pty_with_worktree,
            pty::list_worktrees,
            pty::write_pty,
            pty::write_pty_parts,
            attachments::upload_attachment,
            pty::get_input_buffer_content,
            pty::resize_pty,
            pty::set_ansi_colors,
            pty::pause_pty,
            pty::resume_pty,
            pty::get_kitty_flags,
            pty::get_last_prompt,
            prompt_receipt::get_prompt_receipt,
            pty::get_shell_state,
            pty::get_session_shell_family,
            pty::close_pty,
            worktree::get_worktrees_dir,
            git::get_repo_info,
            git::get_remote_url,
            git::get_git_diff,
            git::get_diff_stats,
            git::get_changed_files,
            git::get_file_diff,
            git::get_gutter_changes,
            git::get_recent_commits,
            list_markdown_files,
            read_file,
            read_editor_file,
            read_external_file,
            read_editor_file_external,
            write_external_file,
            github::get_github_status,
            pty::get_orchestrator_stats,
            pty::get_session_metrics,
            pty::can_spawn_session,
            pty::list_active_sessions,
            pty::enqueue_agent_command,
            pty::clear_queued_agent_commands,
            pty::list_queued_agent_commands,
            pty::remove_queued_agent_command,
            pty::get_process_stats,
            pty::read_vt_log,
            pty::subscribe_terminal_grid,
            pty::unsubscribe_terminal_grid,
            pty::terminal_request_frame,
            pty::ack_terminal_frame,
            frontend_liveness::frontend_heartbeat,
            pty::terminal_exit_alt_screen,
            pty::terminal_scroll,
            pty::terminal_scroll_to_offset,
            pty::terminal_styled_rows,
            pty::terminal_scroll_to,
            pty::terminal_get_block_rows,
            pty::terminal_scroll_info,
            pty::terminal_search,
            pty::terminal_search_buffer,
            pty::terminal_get_row_text,
            pty::terminal_get_logical_line,
            pty::terminal_get_selection_text,
            pty::terminal_get_lines,
            pty::terminal_get_cursor_line,
            pty::terminal_hyperlink_at,
            pty::terminal_hyperlink_span,
            pty::set_session_visible,
            pty::set_session_name,
            pty::get_session_foreground_process,
            pty::get_session_leaf_pid,
            pty::has_foreground_process,
            pty::debug_agent_detection,
            pty_capture::get_pty_capture,
            pty_capture::set_pty_capture,
            load_config,
            save_config,
            themes::list_themes,
            mdkb_commands::mdkb_outline,
            mdkb_commands::mdkb_goto_definition,
            mdkb_commands::mdkb_references,
            mdkb_commands::mdkb_code_find,
            mdkb_commands::mdkb_status,
            mdkb_commands::install_mdkb,
            mdkb_commands::uninstall_mdkb,
            hash_password,
            agent::open_in_app,
            agent::open_in_custom,
            agent::detect_claude_binary,
            agent::detect_agent_binary,
            agent_hook_launch::prepare_agent_launch_args,
            agent::detect_all_agent_binaries,
            agent::spawn_agent,
            agent_session::discover_agent_session,
            agent_session::verify_agent_session,
            agent_session::claude_project_dir,
            worktree::remove_worktree,
            worktree::check_worktree_dirty,
            worktree::get_workspace_lifecycle,
            worktree::delete_local_branch,
            agent::detect_installed_ides,
            worktree::create_worktree,
            git::rename_branch,
            git::create_branch,
            git::get_branch_base,
            git::update_from_base,
            git::delete_branch,
            worktree::get_worktree_paths,
            git::get_git_branches,
            git::get_branches_detail,
            git::get_recent_branches,
            git::get_merged_branches,
            git::get_repo_summary,
            git::get_repo_structure,
            git::get_repo_diff_stats,
            git::check_is_main_branch,
            git::get_initials,
            git::run_git_command,
            git::get_git_panel_context,
            git::get_working_tree_status,
            git::git_stage_files,
            git::git_unstage_files,
            git::git_discard_files,
            git::git_apply_reverse_patch,
            git::git_commit,
            git::get_commit_log,
            git::get_stash_list,
            git::git_stash_apply,
            git::git_stash_pop,
            git::git_stash_drop,
            git::git_stash_show,
            git::get_file_history,
            git::get_file_blame,
            github::get_github_viewer_login,
            github::get_ci_checks,
            github::get_pr_review_threads,
            github::get_repo_pr_statuses,
            github::get_all_pr_statuses,
            github::merge_pr_via_github,
            github::get_pr_diff,
            github::approve_pr,
            github::update_pr_branch,
            github::close_pr,
            github::create_pr,
            github::create_issue,
            github::post_pr_review,
            github::get_merged_prs,
            pr_review::run_pr_review,
            changelog::generate_changelog,
            improvement_scan::run_improvement_scan,
            improvement_scan::create_issue_from_proposal,
            conflict_assist::start_conflict_assist,
            github::fetch_ci_failure_logs,
            circleci::circleci_token_status,
            circleci::circleci_set_token,
            circleci::circleci_delete_token,
            github::get_all_issues,
            github::get_issue_detail,
            github::close_issue,
            github::reopen_issue,
            github_poller::github_start_polling,
            github_poller::github_stop_polling,
            github_poller::github_set_visibility,
            github_poller::github_poll_repo,
            github_poller::github_update_paths,
            github_poller::github_set_issue_filter,
            github_poller::github_set_pr_hide_drafts,
            github_auth::github_start_login,
            github_auth::github_poll_login,
            github_auth::github_poll_add_account,
            github_auth::github_logout,
            github_auth::github_disconnect,
            github_auth::github_diagnostics,
            github_auth::github_auth_status,
            github_account::github_add_account,
            github_account::github_remove_account,
            github_account::github_bind_repo,
            github_account::github_unbind_repo,
            github_account::github_resolve_repo,
            github_account::github_resolve_repos,
            github_account::github_list_accounts,
            github_account::github_list_bindings,
            worktree::generate_worktree_name_cmd,
            worktree::generate_clone_branch_name_cmd,
            worktree::merge_and_archive_worktree,
            worktree::finalize_merged_worktree,
            worktree::list_local_branches,
            worktree::list_base_ref_options,
            worktree::switch_branch,
            worktree::checkout_remote_branch,
            worktree::detect_orphan_worktrees,
            worktree::assess_orphan_cleanup,
            worktree::begin_orphan_cleanup,
            worktree::pending_orphan_cleanup_answer,
            worktree::clear_orphan_cleanup,
            worktree::remove_orphan_worktree,
            worktree::run_setup_script,
            clear_caches,
            clear_repo_caches,
            report_progress_event,
            progress_list,
            progress_projects,
            progress_delete,
            progress_mark_viewed,
            progress_flow,
            progress_flow_detail,
            chat_view_snapshot,
            story_action_command,
            story_capabilities,
            workflow_definition_action,
            workflow_run_action,
            get_local_ip,
            get_local_ips,
            updater::check_update_channel,
            get_mcp_status,
            deep_link_mcp_call,
            get_connect_url,
            regenerate_session_token,
            get_tailscale_status,
            recheck_tailscale_status,
            get_relay_status,
            #[cfg(feature = "dictation")]
            dictation::commands::get_dictation_status,
            #[cfg(feature = "dictation")]
            dictation::commands::get_model_info,
            #[cfg(feature = "dictation")]
            dictation::commands::download_whisper_model,
            #[cfg(feature = "dictation")]
            dictation::commands::delete_whisper_model,
            #[cfg(feature = "dictation")]
            dictation::commands::get_speech_assets,
            #[cfg(feature = "dictation")]
            dictation::commands::download_speech_asset,
            #[cfg(feature = "dictation")]
            dictation::commands::cancel_speech_download,
            #[cfg(feature = "dictation")]
            dictation::commands::delete_speech_asset,
            #[cfg(feature = "dictation")]
            dictation::commands::get_speech_voices,
            #[cfg(feature = "dictation")]
            dictation::commands::get_edge_voices,
            #[cfg(feature = "dictation")]
            dictation::commands::import_speech_voice,
            #[cfg(feature = "dictation")]
            dictation::commands::delete_speech_voice,
            #[cfg(feature = "dictation")]
            dictation::commands::preview_speech_voice,
            #[cfg(feature = "dictation")]
            dictation::commands::speak_reply,
            #[cfg(feature = "dictation")]
            dictation::commands::stop_speech,
            #[cfg(feature = "dictation")]
            dictation::commands::pause_speech,
            #[cfg(feature = "dictation")]
            dictation::commands::resume_speech,
            #[cfg(feature = "dictation")]
            dictation::commands::get_speech_status,
            #[cfg(feature = "dictation")]
            dictation::commands::start_dictation,
            #[cfg(feature = "dictation")]
            dictation::commands::stop_dictation_and_transcribe,
            #[cfg(feature = "dictation")]
            dictation::commands::get_correction_map,
            #[cfg(feature = "dictation")]
            dictation::commands::set_correction_map,
            #[cfg(feature = "dictation")]
            dictation::commands::list_audio_devices,
            #[cfg(feature = "dictation")]
            dictation::commands::inject_text,
            #[cfg(feature = "dictation")]
            dictation::commands::get_dictation_config,
            #[cfg(feature = "dictation")]
            dictation::commands::get_hands_free_default_notice,
            #[cfg(feature = "dictation")]
            dictation::commands::set_dictation_config,
            #[cfg(feature = "dictation")]
            dictation::commands::check_microphone_permission,
            #[cfg(feature = "dictation")]
            dictation::commands::open_microphone_settings,
            #[cfg(feature = "dictation")]
            dictation::commands::arm_hands_free_dictation,
            #[cfg(feature = "dictation")]
            dictation::commands::disarm_hands_free_dictation,
            #[cfg(feature = "dictation")]
            dictation::commands::get_hands_free_status,
            global_hotkey::set_global_hotkey,
            config::load_app_config,
            save_app_config,
            boot_commands::load_notification_config_async,
            config::save_notification_config,
            boot_commands::load_ui_prefs_async,
            config::save_ui_prefs,
            boot_commands::load_repo_settings_async,
            config::save_repo_settings,
            config::set_branch_label,
            config::load_repo_local_config,
            config::save_repo_local_config,
            mcp_upstream_config::load_mcp_upstreams,
            mcp_upstream_config::save_mcp_upstreams,
            mcp_upstream_config::set_project_mcp_upstreams,
            mcp_upstream_config::reconnect_mcp_upstream,
            mcp_upstream_config::get_mcp_upstream_status,
            mcp_upstream_credentials::save_mcp_upstream_credential,
            mcp_upstream_credentials::delete_mcp_upstream_credential,
            mcp_oauth::commands::start_mcp_upstream_oauth,
            mcp_oauth::commands::mcp_oauth_callback,
            mcp_oauth::commands::cancel_mcp_upstream_oauth,
            config::check_has_custom_settings,
            boot_commands::load_repo_defaults_async,
            config::save_repo_defaults,
            boot_commands::load_repositories_async,
            config::save_repositories,
            config::list_stale_temp_repository_candidates,
            config::repair_stale_temp_repositories,
            config::load_pane_layout,
            config::save_pane_layout,
            boot_commands::load_prompt_library_async,
            config::save_prompt_library,
            boot_commands::load_notes_async,
            config::save_notes,
            config::save_note_image,
            config::delete_note_assets,
            config::delete_note_assets_batch,
            config::get_note_images_dir,
            boot_commands::load_activity_async,
            config::save_activity,
            boot_commands::load_keybindings_async,
            config::save_keybindings,
            boot_commands::load_agents_config_async,
            config::save_agents_config,
            config::get_config_defaults,
            agent_hook_commands::set_agent_hook_instrumentation,
            agent_hook_commands::get_agent_hook_state,
            agent_hook_commands::get_agent_native_status_signals,
            agent_hook_commands::set_agent_native_status_signals,
            agent_mcp::get_agent_mcp_status,
            agent_mcp::install_agent_mcp,
            agent_mcp::remove_agent_mcp,
            agent_mcp::list_installed_mcp_integrations,
            agent_mcp::remove_all_mcp_integrations,
            agent_mcp::get_agent_config_path,
            agent_mcp::get_mcp_bridge_info,
            prompt::extract_prompt_variables,
            prompt::process_prompt_content,
            prompt::process_prompt_content_shell_safe,
            prompt::resolve_context_variables,
            prompt::resolve_prompt_variables,
            smart_prompt::execute_headless_prompt,
            smart_prompt::execute_shell_script,
            repo_watcher::start_repo_watcher,
            repo_watcher::stop_repo_watcher,
            repo_watcher::set_hot_repos,
            dir_watcher::start_dir_watcher,
            dir_watcher::stop_dir_watcher,
            sleep_prevention::block_sleep,
            sleep_prevention::unblock_sleep,
            fs::resolve_terminal_path,
            fs::resolve_terminal_paths,
            fs::resolve_markdown_link,
            fs::list_directory,
            fs::get_home_directory,
            fs::stat_path,
            fs::search_files,
            fs::warm_content_index,
            fs::search_content,
            fs::search_content_all,
            fs::fs_read_file,
            fs::write_file,
            fs::write_file_if_unchanged,
            fs::create_directory,
            fs::delete_path,
            fs::rename_path,
            fs::copy_path,
            fs::copy_path_abs,
            fs::move_path_abs,
            fs::fs_transfer_paths,
            remote_transfer::fs_transfer_remote_paths,
            fs::add_to_gitignore,
            plugins::list_user_plugins,
            plugins::get_plugin_readme_path,
            plugins::read_plugin_data,
            plugins::write_plugin_data,
            plugins::delete_plugin_data,
            plugins::install_plugin_from_zip,
            plugins::install_plugin_from_folder,
            plugins::install_plugin_from_url,
            plugins::uninstall_plugin,
            plugins::register_loaded_plugin,
            plugins::unregister_loaded_plugin,
            plugins::set_plugin_output_watchers,
            plugin_fs::plugin_read_file,
            plugin_fs::plugin_read_files,
            plugin_fs::plugin_read_file_base64,
            plugin_fs::plugin_write_file_base64,
            plugin_fs::plugin_list_directory,
            plugin_fs::plugin_read_file_tail,
            plugin_fs::plugin_write_file,
            plugin_fs::plugin_rename_path,
            plugin_fs::plugin_watch_path,
            plugin_fs::plugin_unwatch,
            plugin_fs::scan_build_artifacts,
            plugin_fs::delete_build_artifact,
            plugin_fs::trim_build_artifact,
            plugin_http::plugin_http_fetch,
            plugin_pty::plugin_read_session_output,
            plugin_exec::plugin_exec_cli,
            plugin_credentials::plugin_read_credential,
            registry::fetch_plugin_registry,
            claude_usage::get_claude_usage_api,
            claude_usage::get_claude_usage_timeline,
            claude_usage::get_claude_session_stats,
            claude_usage::get_claude_project_list,
            codex_usage::get_codex_usage_api,
            codex_usage::get_codex_usage_stats,
            grok_usage::get_grok_usage_api,
            terminal_grid_commands::set_terminal_theme_colors,
            screenshot_response,
            secrets::forms::secret_form_bootstrap,
            secrets::forms::secret_form_submit,
            mcp_confirm_response,
            session_suspend_response,
            app_logger::push_log,
            app_logger::get_logs,
            app_logger::clear_logs,
            notification_sound::play_notification_sound,
            notification_sound::list_audio_output_devices,
            git::get_commit_graph,
            tuic_cli::get_cli_status,
            tuic_cli::install_cli,
            tuic_cli::uninstall_cli,
            tuic_cli::dismiss_cli_prompt,
            tuic_cli::get_last_seen_version,
            tuic_cli::set_last_seen_version,
            tunnels::tauri_commands::list_tunnel_profiles,
            tunnels::tauri_commands::save_tunnel_profile,
            tunnels::tauri_commands::delete_tunnel_profile,
            tunnels::tauri_commands::start_tunnel,
            tunnels::tauri_commands::stop_tunnel,
            tunnels::tauri_commands::list_active_tunnels,
            tunnels::tauri_commands::get_tunnel_status,
            tunnels::tauri_commands::list_ssh_config_hosts,
            tunnels::tauri_commands::list_discovered_ssh_hosts,
            tunnels::tauri_commands::probe_discovered_ssh_host,
            tunnels::tauri_commands::probe_ssh_config_hosts,
            tunnels::tauri_commands::list_ssh_agent_keys,
            tunnels::tauri_commands::get_tunnel_audit,
            design_mode::tauri_commands::start_design_mode,
            design_mode::tauri_commands::stop_design_mode,
            design_mode::tauri_commands::get_design_mode_status,
            acp_commands::acp_workspace_root,
            acp_commands::acp_connect,
            acp_chat::acp_chat_open,
            acp_commands::acp_reconnect,
            acp_commands::acp_disconnect,
            acp_commands::acp_kill,
            acp_commands::acp_connection_snapshot,
            acp_commands::acp_subscribe,
            acp_commands::acp_session_new,
            acp_commands::acp_session_list,
            acp_commands::acp_session_load,
            acp_commands::acp_session_resume,
            acp_commands::acp_session_fork,
            acp_commands::acp_session_delete,
            acp_commands::acp_session_close,
            acp_commands::acp_session_prompt,
            acp_commands::acp_session_cancel,
            acp_commands::acp_queued_prompt_cancel,
            acp_commands::acp_session_set_config_option,
            acp_commands::acp_turn_pause,
            acp_commands::acp_turn_resume,
            acp_commands::acp_session_compact,
            acp_commands::acp_pending_interactions,
            acp_commands::acp_respond_permission,
            acp_commands::acp_respond_elicitation,
            acp_commands::acp_one_shot_prompt,
            ego_cli::ego_providers,
            ego_cli::perimeter::ego_perimeter,
            ego_cli::perimeter::ego_set_perimeter_roots,
            ego_cli::perimeter::ego_set_perimeter_network,
            ego_cli::ego_set_default_model
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match &event {
                // Guard against corrupted window-state applied by tauri-plugin-window-state.
                // Must run at Ready (after plugins have restored persisted position/size),
                // not in setup() which fires before the plugin applies its state.
                tauri::RunEvent::Ready => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        ensure_window_visible(&window);
                    }
                }
                // Dock-icon click (applicationShouldHandleReopen). macOS suppresses
                // the default un-minimize when ANY window is visible — and a detached
                // panel counts as visible, so the minimized main window would stay
                // hidden. Explicitly restore main on every reopen.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.unminimize();
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                // Forward file-open events (macOS file associations) to the frontend
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Opened { urls } => {
                    let paths: Vec<String> = urls
                        .iter()
                        .filter_map(|u| {
                            if u.scheme() == "file" {
                                u.to_file_path()
                                    .ok()
                                    .map(|p| p.to_string_lossy().into_owned())
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !paths.is_empty() {
                        let _ = app_handle.emit("file-open", paths);
                    }
                }
                // Cleanly tear down the Whisper/GGML context before std::process::exit
                // triggers C++ static destructors. GGML's Metal backend uses
                // dispatch_async for GPU resource init — if that GCD thread is still
                // running when __cxa_finalize_ranges destroys the Metal device
                // singleton, ggml_metal_rsets_free aborts. shutdown() joins the
                // streaming thread (which holds an Arc<WhisperContext>), then drops
                // the transcriber while the process is still alive.
                tauri::RunEvent::Exit => {
                    app_handle.state::<native_clipboard::NativeClipboard>().clear();
                    workflows::shutdown_checks();
                    #[cfg(feature = "dictation")]
                    if let Some(dictation) = app_handle.try_state::<dictation::DictationState>() {
                        dictation.shutdown();
                    }
                    // Kill all SSH tunnel processes so ports are freed for restart
                    if let Some(state) = app_handle.try_state::<Arc<AppState>>() {
                        state.secrets.clear();
                        state.tunnel_manager.shutdown_all();
                        if let Some(manager) = state.design_mode.get() {
                            tauri::async_runtime::block_on(manager.stop_all());
                        }
                        // End every ego AI Chat started: `std::process::exit`
                        // skips the destructors that would kill them.
                        tauri::async_runtime::block_on(state.acp.shutdown_all());
                        crate::ai_agent::knowledge::flush_dirty(state.inner());
                    }
                    // Flush the last buffered log lines to disk before the
                    // process exits (story #672-c1a3) — the lines a shutdown
                    // bug needs most.
                    app_logger::flush_logs_on_exit();
                }
                _ => {}
            }
        });
}

/// Build a connect URL for QR-code authentication.
/// Brackets IPv6 addresses for valid URL syntax.
fn build_connect_url(scheme: &str, host: &str, port: u16, token: &str) -> String {
    let host = if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    };
    format!("{scheme}://{host}:{port}/?token={token}")
}

/// Spawn background tasks shared by both desktop and headless modes.
fn spawn_background_tasks(state: &Arc<AppState>) {
    workflows::WorkflowRuntime::spawn(state);
    automations::runtime::AutomationRuntime::spawn(state);
    AppState::spawn_session_state_accumulator(state.clone());
    idle_close::spawn(state.clone());
    AppState::spawn_acp_notice_pump(state.clone());
    drop(
        state
            .mcp
            .oauth_flow_manager
            .spawn_cleanup_task(state.mcp.upstream_registry.clone()),
    );
    mcp_http::mcp_transport::spawn_tool_search_index_updater(state.clone());
    pty::spawn_tombstone_sweeper(state.clone());
    content_index::spawn_content_index_updater(state.clone());
    cpu_watchdog::spawn(state.clone());
    // Its own thread on purpose: probing the webview URL blocks on the event
    // loop, and the CPU watchdog must not be able to hang behind it.
    #[cfg(feature = "desktop")]
    webview_recovery::spawn(state.clone());
    ai_agent::knowledge::spawn_persist_task(state.clone());
}

/// Interactive CLI to set username + password for headless auth.
/// Reads from stdin, hashes with bcrypt, writes to config.json.
#[cfg(not(feature = "desktop"))]
pub fn set_password_interactive() -> anyhow::Result<()> {
    use std::io::{self, BufRead, Write};

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    print!("Username: ");
    stdout.flush()?;
    let mut username = String::new();
    stdin.lock().read_line(&mut username)?;
    let username = username.trim().to_string();
    if username.is_empty() {
        anyhow::bail!("Username cannot be empty");
    }

    print!("Password: ");
    stdout.flush()?;
    let mut password = String::new();
    stdin.lock().read_line(&mut password)?;
    let password = password.trim().to_string();
    if password.is_empty() {
        anyhow::bail!("Password cannot be empty");
    }

    let hash =
        bcrypt::hash(&password, 12).map_err(|e| anyhow::anyhow!("Failed to hash password: {e}"))?;

    let mut cfg = config::load_app_config();
    cfg.services.auth.username = username.clone();
    cfg.services.auth.password_hash = hash;
    config::save_app_config(cfg).map_err(|e| anyhow::anyhow!(e))?;

    let masked = if username.len() <= 2 {
        format!("{}*", &username[..1])
    } else {
        format!("{}…{}", &username[..1], &username[username.len() - 1..])
    };
    println!("Credentials saved for user \"{masked}\"");
    Ok(())
}

/// Background tasks the `tuic-remote` daemon runs.
///
/// The daemon is a whole machine, not a session server: it holds the repos, the
/// PTYs and the agents, so nearly everything `spawn_background_tasks` gives the
/// desktop it needs too. What it deliberately does not run is named below with
/// the reason, and `the_daemon_decides_on_every_desktop_background_task` fails
/// if a task is added to `spawn_background_tasks` and not decided on here
/// (#793-23a5).
#[cfg(not(feature = "desktop"))]
fn spawn_daemon_background_tasks(state: &Arc<AppState>) {
    workflows::WorkflowRuntime::spawn(state);
    automations::runtime::AutomationRuntime::spawn(state);
    AppState::spawn_session_state_accumulator(state.clone());
    idle_close::spawn(state.clone());
    AppState::spawn_acp_notice_pump(state.clone());
    pty::spawn_tombstone_sweeper(state.clone());
    // The agents run here, so the argv/env snapshot session discovery reads is
    // this process's to keep — no snapshot, no resume after a restart.
    pty::spawn_process_snapshot_refresher(state.clone());
    // A remote client reports tab visibility over `/sessions/{id}/visible`, so
    // parking an idle hidden session is as correct here as on the desktop.
    #[cfg(unix)]
    pty::spawn_standby_checker(state.clone());
    content_index::spawn_content_index_updater(state.clone());
    mcp_http::mcp_transport::spawn_tool_search_index_updater(state.clone());
    crate::mcp_proxy::registry::UpstreamRegistry::spawn_health_checker(Arc::clone(
        &state.mcp.upstream_registry,
    ));
    drop(
        state
            .mcp
            .oauth_flow_manager
            .spawn_cleanup_task(state.mcp.upstream_registry.clone()),
    );
    // The daemon is precisely where nobody can watch a CPU spike happen.
    cpu_watchdog::spawn(state.clone());
    mcp_http::acp_mcp::install(state);
    mcp_http::spawn_maintenance_sweep(state);

    // Deliberately NOT started on the daemon:
    //
    // - `webview_recovery::spawn` — `#[cfg(feature = "desktop")]`, so it does
    //   not exist in this build. There is no WebView whose document can be lost.
    // - `ai_agent::knowledge::spawn_persist_task` — `build_remote_router`
    //   serves no route that exposes command knowledge. Deleting the embedded
    //   AI engine (#784-0aec) took its last reader too, so the desktop build
    //   keeps recording while nothing consumes it yet; the daemon has no reason
    //   to write files no one asks it for.
}

/// Run the tuic-remote server.
///
/// - Uses `build_remote_router()` (no config, MCP, plugins, push, static files).
/// - Spawns only the two essential background tasks: session state accumulator
///   and tombstone sweeper.
/// - Logs "Starting tuic-remote" with the `protocol_version` field.
/// - Binds TCP directly without spawning an IPC socket.
///
/// Runtime options for the standalone remote daemon.
pub struct RemoteOptions {
    /// TCP port exposed by the daemon.
    pub port: u16,
    /// IP address the daemon binds.
    pub bind: std::net::IpAddr,
    /// Idle lifetime in seconds, or no automatic expiry.
    pub survive_secs: Option<u64>,
    /// Whether startup writes MCP configuration for local agents.
    pub agent_configs: bool,
    /// Exit with failure after an update so the installed service restarts us.
    pub supervised: bool,
    /// Allow the old process a short grace period to release its TCP port.
    pub wait_for_restart: bool,
    pairing_token: Option<String>,
}

/// Read-only metadata for selecting a locally built daemon binary.
pub fn remote_build_info_json() -> Result<String, String> {
    let build = remote_deploy::assets::running_build_identity()?;
    serde_json::to_string(build).map_err(|error| error.to_string())
}

impl Default for RemoteOptions {
    fn default() -> Self {
        Self {
            port: 9877,
            bind: std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
            survive_secs: None,
            agent_configs: true,
            supervised: false,
            wait_for_restart: false,
            pairing_token: None,
        }
    }
}

impl RemoteOptions {
    /// Return the complete TCP bind address.
    pub fn bind_addr(&self) -> std::net::SocketAddr {
        std::net::SocketAddr::new(self.bind, self.port)
    }

    /// Set the one-shot pairing token supplied by the launcher.
    pub fn set_pairing_token(&mut self, token: Option<String>) {
        self.pairing_token = token;
    }
}

#[cfg(any(not(feature = "desktop"), test))]
struct RemotePidFile(std::path::PathBuf);

#[cfg(any(not(feature = "desktop"), test))]
impl RemotePidFile {
    fn create(config_dir: &std::path::Path) -> std::io::Result<Self> {
        let path = config_dir.join("tuic-remote.pid");
        std::fs::write(&path, std::process::id().to_string())?;
        Ok(Self(path))
    }
}

#[cfg(any(not(feature = "desktop"), test))]
impl Drop for RemotePidFile {
    fn drop(&mut self) {
        if std::fs::read_to_string(&self.0).ok().as_deref() != Some(&std::process::id().to_string())
        {
            return;
        }
        if let Err(error) = std::fs::remove_file(&self.0)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(source = "remote", path = %self.0.display(), "Failed to remove pid file: {error}");
        }
    }
}

#[cfg(all(unix, not(feature = "desktop")))]
async fn remote_shutdown_signal() -> std::io::Result<()> {
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate())?;
    let mut hangup = signal(SignalKind::hangup())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
        _ = hangup.recv() => Ok(()),
    }
}

#[cfg(all(not(unix), not(feature = "desktop")))]
async fn remote_shutdown_signal() -> std::io::Result<()> {
    tokio::signal::ctrl_c().await
}

#[cfg(not(feature = "desktop"))]
pub async fn run_remote(mut options: RemoteOptions) -> anyhow::Result<()> {
    crate::remote_deploy::assets::running_build_identity().map_err(anyhow::Error::msg)?;
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("Failed to install rustls CryptoProvider"))?;

    credentials::probe_named_vault_read().map_err(anyhow::Error::msg)?;

    let log_buffer = Arc::new(parking_lot::Mutex::new(app_logger::LogRingBuffer::new(
        app_logger::LOG_RING_CAPACITY,
    )));
    app_logger::init_tracing(log_buffer.clone());

    let mut app_config = config::load_app_config();
    app_config.services.server.enabled = true;
    if app_config.services.server.port != options.port {
        tracing::info!(
            source = "remote",
            config_port = app_config.services.server.port,
            override_port = options.port,
            "Port overridden by TUIC_PORT / CLI argument"
        );
        app_config.services.server.port = options.port;
    }
    if app_config.services.auth.lan_auth_bypass {
        tracing::warn!(
            source = "remote",
            "lan_auth_bypass is not supported in headless mode — forcing off"
        );
        app_config.services.auth.lan_auth_bypass = false;
    }
    if let Some(token) = options.pairing_token.take() {
        app_config.services.auth.session_token = token;
        app_config.services.auth.session_token_exists = true;
    } else if app_config.services.auth.session_token.is_empty() {
        app_config.services.auth.session_token = uuid::Uuid::new_v4().to_string();
        app_config.services.auth.session_token_exists = true;
        // Keep it across restarts so paired phones stay logged in. A host with
        // no usable vault falls back to a token that lives for this run only.
        if let Err(e) = config::persist_session_token(&app_config.services.auth.session_token) {
            tracing::warn!(
                source = "remote",
                error = %e,
                "Could not persist the session token; it changes on every restart"
            );
        }
    }

    let data_dir = config::config_dir();
    let worktrees_dir = data_dir.join("worktrees");
    std::fs::create_dir_all(&worktrees_dir)?;
    let _pid_file = RemotePidFile::create(&data_dir)?;

    // Env only, for the same reason the desktop boot does it: the rest of the
    // chain spawns `gh` or reads the credential store, and this runs before the
    // HTTP server binds. No window here, but a wedged `gh` would still keep the
    // server unreachable.
    let (github_token, github_token_source) = crate::github_auth::resolve_token_from_env();

    agent_hook_launch::regenerate_launch_assets_at_boot(&data_dir);

    let mut app_state = AppState::new(data_dir, worktrees_dir, app_config.clone(), log_buffer);
    app_state.remote_survive_secs = options.survive_secs;
    let restart = Arc::new(tokio::sync::Notify::new());
    app_state.remote_update = Some(remote_update::RemoteUpdateState {
        executable: std::env::current_exe()?,
        restart: restart.clone(),
        in_progress: tokio::sync::Mutex::new(()),
        installed: std::sync::atomic::AtomicBool::new(false),
    });
    *app_state.github.token.get_mut() = github_token;
    *app_state.github.token_source.get_mut() = github_token_source;

    let state = Arc::new(app_state);
    // The Host guard trusts the Tailscale FQDN only while the state is Running, so
    // the daemon must detect it like the desktop boot does (#1535).
    *state.tailscale_state.write() = detect_tailscale_bounded(async {
        tokio::task::spawn_blocking(tailscale::detect)
            .await
            .unwrap_or(tailscale::TailscaleState::NotInstalled)
    })
    .await;
    state.wire_event_bus();
    crate::github_auth::spawn_deferred_token_resolution(state.clone());

    spawn_daemon_background_tasks(&state);
    telegram::start(&state);

    // The bridge reaches this process over the local IPC socket, and an agent on
    // this machine can only find the socket if it is listening. Awaited: the
    // configs written below name a bridge that must have something to connect to.
    mcp_http::spawn_ipc_listener(&state, true).await;
    if options.agent_configs {
        agent_mcp::ensure_mcp_configs(&app_config.disabled_mcp_agents);
    }

    // Watch and pre-warm the repos this machine holds. Cross-repo content search
    // never starts a build of its own, so without this the daemon answers every
    // such query with "pending" for as long as it runs.
    let repos_json = config::load_repositories();
    let known_repo_paths = boot_repo_paths(&repos_json);
    for repo_path in &known_repo_paths {
        if let Err(e) = repo_watcher::start_watching(repo_path, &state) {
            tracing::warn!(source = "remote", repo = %repo_path, "Failed to watch repo: {e}");
        }
    }
    let repos_to_warm = repos_to_prewarm(
        known_repo_paths,
        repos_json
            .get("activeRepoPath")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned()),
        &app_config.index_strategy,
    );
    if !repos_to_warm.is_empty() {
        tokio::spawn(prewarm_content_indices(state.clone(), repos_to_warm));
    }

    // Upstream MCP servers are per-machine configuration (#792-c255), so the
    // machine that holds them is the one that must connect them. Spawned, not
    // awaited: registration is fast but a dead upstream must not delay the bind.
    let auto_state = state.clone();
    let settle_guard = auto_state.clone();
    let auto_handle = tokio::spawn(async move {
        crate::mcp_upstream_config::auto_connect_saved_upstreams(&auto_state).await;
    });
    // Recover the settle latch if the auto-connect task panics: nothing else
    // releases it, and a waiter would park for the life of the daemon.
    tokio::spawn(async move {
        if let Err(e) = auto_handle.await {
            tracing::error!(
                source = "mcp_upstream",
                "auto_connect_saved_upstreams task failed: {e}"
            );
            settle_guard
                .mcp
                .upstream_registry
                .mark_initial_connect_complete();
        }
    });

    let tls_config = match &app_config.services.tls {
        config::TlsConfig::Manual {
            cert_path,
            key_path,
        } => {
            let cert_pem = std::fs::read(cert_path)
                .map_err(|e| anyhow::anyhow!("Failed to read TLS cert at {cert_path}: {e}"))?;
            let key_pem = std::fs::read(key_path)
                .map_err(|e| anyhow::anyhow!("Failed to read TLS key at {key_path}: {e}"))?;
            let tls = axum_server::tls_rustls::RustlsConfig::from_pem(cert_pem, key_pem)
                .await
                .map_err(|e| anyhow::anyhow!("Invalid TLS cert/key: {e}"))?;
            tracing::info!(
                source = "remote",
                cert_path,
                key_path,
                "TLS loaded (manual mode)"
            );
            Some(tls)
        }
        config::TlsConfig::Off => None,
    };

    let protocol_version = remote_runtime::REMOTE_PROTOCOL_VERSION as u32;
    tracing::info!(
        source = "remote",
        port = options.port,
        tls = tls_config.is_some(),
        protocol_version,
        "Starting tuic-remote"
    );

    let bind_addr = options.bind_addr();
    let listener = if options.wait_for_restart {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            match std::net::TcpListener::bind(bind_addr) {
                Ok(listener) => break listener,
                Err(error)
                    if error.kind() == std::io::ErrorKind::AddrInUse
                        && std::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(error) => anyhow::bail!("Fatal: failed to bind TCP on {bind_addr}: {error}"),
            }
        }
    } else {
        std::net::TcpListener::bind(bind_addr)
            .map_err(|e| anyhow::anyhow!("Fatal: failed to bind TCP on {bind_addr}: {e}"))?
    };
    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;

    let router = mcp_http::build_remote_router(state.clone());
    let svc = router.into_make_service_with_connect_info::<std::net::SocketAddr>();
    let lifetime = remote_lifetime::expired(
        state.clone(),
        options.survive_secs.map(std::time::Duration::from_secs),
    );
    tokio::pin!(lifetime);

    let mut updated = false;
    let shutdown_result: anyhow::Result<()> = tokio::select! {
        result = axum::serve(listener, svc) => {
            result.map_err(|e| anyhow::anyhow!("TCP server error: {e}"))
        }
        signal = remote_shutdown_signal() => {
            signal.map_err(anyhow::Error::from).map(|()| {
                tracing::info!(source = "remote", "Received shutdown signal");
            })
        }
        () = &mut lifetime => {
            tracing::info!(source = "remote", "Remote daemon survive time expired");
            Ok(())
        }
        () = restart.notified() => {
            updated = true;
            tracing::info!(source = "remote", "Restarting after remote binary update");
            Ok(())
        }
    };

    workflows::shutdown_checks();
    shutdown_result?;
    // Flush the last buffered log lines to disk before the process exits
    // (story #672-c1a3) — the lines a shutdown bug needs most.
    app_logger::flush_logs_on_exit();
    if updated {
        if options.supervised {
            anyhow::bail!("Remote update installed; exiting for supervisor restart");
        }
        let mut child = std::process::Command::new(std::env::current_exe()?);
        child.args(
            std::env::args()
                .skip(1)
                .filter(|arg| arg != "--wait-for-restart"),
        );
        child.arg("--wait-for-restart");
        child.env(
            "TUIC_PAIRING_TOKEN",
            &app_config.services.auth.session_token,
        );
        child
            .spawn()
            .map_err(|e| anyhow::anyhow!("Updated remote failed to restart: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "desktop")]
    fn register_document_grid(state: &AppState, session_id: &str, webview_label: &str) -> u64 {
        let gate = Arc::new(crate::grid_gate::GridGate::new());
        let epoch = gate.epoch();
        state.grid.gates.insert(session_id.to_string(), gate);
        state.grid.channels.insert(
            session_id.to_string(),
            crate::state::DesktopGridChannel {
                channel: tauri::ipc::Channel::new(|_| Ok(())),
                webview_label: webview_label.to_string(),
                epoch,
            },
        );
        epoch
    }

    #[cfg(feature = "desktop")]
    fn watcher(id: &str, pattern: &str) -> output_watchers::WatcherSpec {
        output_watchers::WatcherSpec {
            id: id.to_string(),
            pattern: pattern.to_string(),
            flags: String::new(),
        }
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn reload_releases_every_old_grid_channel_and_watcher() {
        let state = crate::state::tests_support::make_test_app_state();
        register_document_grid(&state, "one", "main");
        register_document_grid(&state, "two", "main");
        state.plugin_output_watchers.write().sync_for_webview(
            "main",
            "old-document",
            1,
            &[watcher("w", "old document")],
        );

        release_webview_document_resources(&state, "main");

        assert!(state.grid.channels.is_empty());
        assert!(state.grid.gates.is_empty());
        assert!(!crate::pty::grid_has_subscriber(&state, "one"));
        assert!(!crate::pty::grid_has_subscriber(&state, "two"));
        assert!(
            state
                .plugin_output_watchers
                .read()
                .matching_ids("old document")
                .is_empty()
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn closing_a_panel_releases_only_its_watcher_set() {
        let state = crate::state::tests_support::make_test_app_state();
        register_document_grid(&state, "main-terminal", "main");
        state.plugin_output_watchers.write().sync_for_webview(
            "main",
            "main-client",
            1,
            &[watcher("w", "main line")],
        );
        state.plugin_output_watchers.write().sync_for_webview(
            "panel-activity",
            "panel-client",
            1,
            &[watcher("w", "panel line")],
        );

        release_webview_document_resources(&state, "panel-activity");

        assert!(crate::pty::grid_has_subscriber(&state, "main-terminal"));
        let watchers = state.plugin_output_watchers.read();
        assert_eq!(watchers.matching_ids("main line"), vec!["main-client/w"]);
        assert!(watchers.matching_ids("panel line").is_empty());
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn main_reload_preserves_a_floating_terminals_grid_subscription() {
        let state = crate::state::tests_support::make_test_app_state();
        register_document_grid(&state, "main-terminal", "main");
        let floating_epoch = register_document_grid(&state, "floating-terminal", "floating-tab-1");

        release_webview_document_resources(&state, "main");

        assert!(!crate::pty::grid_has_subscriber(&state, "main-terminal"));
        assert!(crate::pty::grid_has_subscriber(&state, "floating-terminal"));
        assert_eq!(
            state.grid.gates.get("floating-terminal").unwrap().epoch(),
            floating_epoch
        );
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn destroying_a_floating_window_releases_only_its_grid_subscription() {
        let state = crate::state::tests_support::make_test_app_state();
        let main_epoch = register_document_grid(&state, "main-terminal", "main");
        register_document_grid(&state, "floating-terminal", "floating-tab-1");

        release_webview_document_resources(&state, "floating-tab-1");

        assert!(crate::pty::grid_has_subscriber(&state, "main-terminal"));
        assert_eq!(
            state.grid.gates.get("main-terminal").unwrap().epoch(),
            main_epoch
        );
        assert!(!crate::pty::grid_has_subscriber(
            &state,
            "floating-terminal"
        ));
        assert!(!state.grid.gates.contains_key("floating-terminal"));
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn reload_keeps_browser_watchers_and_accepts_fresh_document() {
        let state = crate::state::tests_support::make_test_app_state();
        state
            .plugin_output_watchers
            .write()
            .sync("browser", 1, &[watcher("w", "browser line")]);
        state.plugin_output_watchers.write().sync_for_webview(
            "main",
            "old-document",
            1,
            &[watcher("w", "old line")],
        );
        release_webview_document_resources(&state, "main");

        let new_epoch = register_document_grid(&state, "one", "main");
        state.plugin_output_watchers.write().sync_for_webview(
            "main",
            "new-document",
            1,
            &[watcher("w", "new line")],
        );

        assert_eq!(state.grid.gates.get("one").unwrap().epoch(), new_epoch);
        let watchers = state.plugin_output_watchers.read();
        assert_eq!(watchers.matching_ids("browser line"), vec!["browser/w"]);
        assert_eq!(watchers.matching_ids("new line"), vec!["new-document/w"]);
        assert!(watchers.matching_ids("old line").is_empty());
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn repeated_reloads_leave_only_the_latest_document_watchers() {
        let state = crate::state::tests_support::make_test_app_state();
        for i in 0..10 {
            release_webview_document_resources(&state, "main");
            let client = format!("document-{i}");
            state.plugin_output_watchers.write().sync_for_webview(
                "main",
                &client,
                1,
                &[watcher("w", &format!("line-{i}"))],
            );
        }
        let watchers = state.plugin_output_watchers.read();
        for i in 0..9 {
            assert!(watchers.matching_ids(&format!("line-{i}")).is_empty());
        }
        assert_eq!(watchers.matching_ids("line-9"), vec!["document-9/w"]);
    }

    #[cfg(feature = "desktop")]
    #[test]
    fn navigation_guard_preserves_internal_frames_and_blocks_external_targets() {
        let dev = tauri::Url::parse("http://dev.example:1421/").unwrap();
        for allowed in [
            "tauri://localhost/index.html",
            "asset://localhost/file.pdf",
            "http://asset.localhost/file.pdf",
            "plugin://localhost/panel",
            "plugin://custom/panel",
            "http://plugin.localhost/panel",
            "about:blank",
            "about:srcdoc#/3",
            "data:text/html,hello",
            "blob:tauri://localhost/id",
            "http://localhost:9877/panel",
            "http://127.0.0.1:14319/",
            "http://[::1]:9877/",
        ] {
            assert!(
                is_app_navigation(&tauri::Url::parse(allowed).unwrap(), Some(&dev)),
                "{allowed}"
            );
        }
        assert_eq!(
            is_app_navigation(
                &tauri::Url::parse("http://dev.example:1421/docs").unwrap(),
                Some(&dev)
            ),
            cfg!(debug_assertions)
        );
        for blocked in [
            "http://dev.example:1422/docs",
            "https://example.com/",
            "http://localhost@evil.com/",
            "tauri://localhost.evil/",
            "file:///tmp/secret",
            "javascript:alert(1)",
        ] {
            assert!(
                !is_app_navigation(&tauri::Url::parse(blocked).unwrap(), Some(&dev)),
                "{blocked}"
            );
        }
        assert_eq!(
            is_app_navigation(&tauri::Url::parse("http://tauri.localhost/").unwrap(), None),
            cfg!(windows)
        );
    }

    #[cfg(feature = "desktop")]
    #[tokio::test]
    async fn story_action_ipc_shared_path_creates_a_plan() {
        let config = tempfile::tempdir().expect("config directory");
        let _guard = crate::config::set_config_dir_override(config.path().to_path_buf());
        let project = tempfile::tempdir().expect("project directory");
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let reply = story_action_command_for_state(
            state,
            project.path().to_string_lossy().into_owned(),
            stories::StoryAction::CreatePlan {
                title: "IPC plan".into(),
                source: "plan.md".into(),
            },
            None,
        )
        .await
        .expect("IPC story action");
        let stories::StoryReply::Plan(plan) = reply else {
            panic!("expected created plan");
        };
        assert_eq!(plan.title, "IPC plan");
    }

    /// The body of one function, read out of this file's own source.
    fn fn_body(source: &str, signature: &str) -> String {
        source
            .split(signature)
            .nth(1)
            .unwrap_or_else(|| panic!("{signature} must exist"))
            .split("\n}\n")
            .next()
            .expect("function body")
            .to_string()
    }

    /// Names of the background tasks a function body starts.
    ///
    /// Derived from the source rather than listed, so a task added tomorrow is
    /// picked up without anyone remembering to update a list here. Three shapes
    /// are counted: a `spawn_*` call, a bare `module::spawn(` (the module names
    /// the task), and the engine types that are spawned by hand. Nothing
    /// matches the third shape today — `WatcherEngine` was its only case and it
    /// went with the embedded AI engine (#784-0aec) — so it is kept for the
    /// next hand-spawned engine rather than dropped, which is why the test
    /// anchors on one name per shape it can still see.
    fn background_task_names(body: &str) -> Vec<String> {
        let tokens: Vec<String> = body
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|t| !t.is_empty())
            .map(|t| t.to_string())
            .collect();
        let mut names = Vec::new();
        for (index, token) in tokens.iter().enumerate() {
            let name = if token.starts_with("spawn_") {
                token.clone()
            } else if token == "spawn" {
                match tokens.get(index.wrapping_sub(1)) {
                    // `tokio::spawn` is the spawner, not a task.
                    Some(module) if module != "tokio" && index > 0 => module.clone(),
                    _ => continue,
                }
            } else if token == "ensure_running" || token.ends_with("Engine") {
                token.clone()
            } else {
                continue;
            };
            // The spawner functions name themselves in their own log lines.
            if name.ends_with("_background_tasks") {
                continue;
            }
            if !names.contains(&name) {
                names.push(name);
            }
        }
        names
    }

    /// Every background task the desktop spawns must be decided on for the
    /// daemon: started there, or named in the "Deliberately NOT started" block
    /// with the reason it must not be. Before #793-23a5 the daemon ran three of
    /// twelve and said so only in a comment that listed no names, so content
    /// indexing, the tool search index and scheduling were missing with nothing
    /// to notice it.
    #[test]
    fn the_daemon_decides_on_every_desktop_background_task() {
        let source = include_str!("lib.rs");
        let desktop = fn_body(source, "fn spawn_background_tasks(state: &Arc<AppState>) {");
        let daemon = fn_body(
            source,
            "fn spawn_daemon_background_tasks(state: &Arc<AppState>) {",
        );

        let expected = background_task_names(&desktop);
        // Anchors, not a count. The scanner failing open makes this test pass
        // by finding nothing, so it has to prove it still matches — but the
        // number of tasks is not a durable fact: it was twelve before
        // #784-0aec deleted the scheduler and the watcher engine, and a count
        // pinned here breaks on the next legitimate add or remove while
        // asserting nothing about whether the scanner works. One name per
        // shape the scanner recognises does assert that.
        for (name, shape) in [
            ("spawn_cleanup_task", "a `spawn_*` call"),
            ("cpu_watchdog", "a bare `module::spawn(`"),
        ] {
            assert!(
                expected.iter().any(|n| n == name),
                "the scanner no longer matches {shape} — `{name}` is in \
                 spawn_background_tasks but not in {expected:?}"
            );
        }
        for name in expected {
            assert!(
                daemon.contains(&name),
                "`{name}` runs on the desktop but the daemon neither starts it nor says why not"
            );
        }
    }

    /// The daemon must actually serve MCP, not merely write configs that point
    /// at it. `tuic-bridge` speaks HTTP over the local IPC socket and has no
    /// other transport, so an agent on a daemon without that listener finds a
    /// configured server it can never reach.
    #[test]
    fn the_daemon_listens_on_ipc_before_it_writes_bridge_configs() {
        let source = include_str!("lib.rs");
        let body = fn_body(
            source,
            "pub async fn run_remote(mut options: RemoteOptions)",
        );
        let listener = body
            .find("spawn_ipc_listener")
            .expect("run_remote must start the IPC listener the bridge connects to");
        let configs = body
            .find("ensure_mcp_configs")
            .expect("run_remote must install the bridge configs for this machine's agents");
        assert!(
            listener < configs,
            "the socket must be listening before a config names the bridge that connects to it"
        );
    }

    /// Cross-repo content search skips a repo whose index does not exist and
    /// deliberately starts no build — the warm strategy owns that. The daemon
    /// ran neither, so `/fs/search-content-all` answered "pending" for every
    /// repo for as long as the process lived: empty results that never resolve.
    #[test]
    fn the_daemon_warms_and_watches_the_repos_it_holds() {
        let body = fn_body(
            include_str!("lib.rs"),
            "pub async fn run_remote(mut options: RemoteOptions)",
        );
        for call in [
            "repo_watcher::start_watching",
            "repos_to_prewarm",
            "prewarm_content_indices",
        ] {
            assert!(
                body.contains(call),
                "run_remote must call {call} — without it the machine's repos have no index"
            );
        }
    }

    /// The remote boot path enters a Tokio runtime before registering its
    /// repositories. Registering a real watcher here exercises the same
    /// runtime-dependent path that panicked in synchronous headless tests.
    #[cfg(not(feature = "desktop"))]
    #[tokio::test]
    async fn remote_boot_can_register_a_repository_watcher() {
        let repo = tempfile::tempdir().expect("repository directory");
        let state = std::sync::Arc::new(crate::state::tests_support::make_test_app_state());
        let path = repo.path().to_str().expect("UTF-8 test path");

        crate::repo_watcher::start_watching(path, &state)
            .expect("remote boot must register a watcher inside its Tokio runtime");
        assert!(state.repo_watchers.contains_key(path));
        crate::repo_watcher::stop_watching(path, &state);
    }

    #[test]
    fn remote_options_control_binding_and_agent_config_installation() {
        let options = RemoteOptions {
            port: 4545,
            bind: "127.0.0.1".parse().expect("loopback"),
            survive_secs: Some(30),
            agent_configs: false,
            ..RemoteOptions::default()
        };
        assert_eq!(options.bind_addr(), "127.0.0.1:4545".parse().unwrap());

        let body = fn_body(
            include_str!("lib.rs"),
            "pub async fn run_remote(mut options: RemoteOptions)",
        );
        assert!(
            body.contains("if options.agent_configs")
                && body.contains("agent_mcp::ensure_mcp_configs"),
            "run_remote must gate agent config writes behind the explicit option"
        );
    }

    #[test]
    fn remote_pid_file_is_removed_when_its_guard_drops() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tuic-remote.pid");
        {
            let _guard = RemotePidFile::create(dir.path()).expect("pid file");
            assert_eq!(
                std::fs::read_to_string(&path).expect("pid contents"),
                std::process::id().to_string()
            );
        }
        assert!(!path.exists());
    }

    #[test]
    fn remote_restart_old_pid_guard_preserves_successor_pid() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("tuic-remote.pid");
        let old = RemotePidFile::create(dir.path()).expect("old pid file");
        std::fs::write(&path, "424242").expect("successor pid");
        drop(old);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "424242");
    }

    #[test]
    fn prewarm_follows_the_index_strategy() {
        let known = || vec!["/a".to_string(), "/b".to_string(), "/c".to_string()];

        // Default strategies warm the active repo and nothing else.
        assert_eq!(
            repos_to_prewarm(known(), Some("/b".to_string()), "active_and_switch"),
            vec!["/b".to_string()]
        );
        assert_eq!(
            repos_to_prewarm(known(), Some("/b".to_string()), "active_only"),
            vec!["/b".to_string()]
        );
        // An active repo that is not registered (or parked) warms nothing.
        assert!(repos_to_prewarm(known(), Some("/gone".to_string()), "active_only").is_empty());
        assert!(repos_to_prewarm(known(), None, "active_and_switch").is_empty());
        // `all_sequential` warms every repo, active first so it is ready first.
        assert_eq!(
            repos_to_prewarm(known(), Some("/c".to_string()), "all_sequential"),
            vec!["/c".to_string(), "/b".to_string(), "/a".to_string()]
        );
        assert!(repos_to_prewarm(Vec::new(), Some("/a".to_string()), "all_sequential").is_empty());
    }

    #[test]
    fn relay_does_not_own_a_second_tokio_runtime() {
        let source = include_str!("lib.rs");
        assert!(
            !source.contains(&["fn relay_", "runtime()"].concat()),
            "the relay task must run on the long-lived HTTP server runtime"
        );
    }

    #[tokio::test]
    async fn tailscale_detection_is_bounded_before_server_bind() {
        let started = std::time::Instant::now();
        let state = detect_tailscale_bounded(async {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            tailscale::TailscaleState::NotInstalled
        })
        .await;

        assert_eq!(state, tailscale::TailscaleState::NotInstalled);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "a stalled tailscale status must not delay local socket and HTTP binding"
        );
    }

    #[test]
    fn boot_repo_paths_exclude_parked_repositories() {
        let repositories = serde_json::json!({
            "repos": {
                "/active": { "parked": false },
                "/legacy": {},
                "/parked": { "parked": true }
            }
        });

        let paths = boot_repo_paths(&repositories);

        assert_eq!(paths.len(), 2);
        assert!(paths.iter().any(|path| path == "/active"));
        assert!(paths.iter().any(|path| path == "/legacy"));
        assert!(!paths.iter().any(|path| path == "/parked"));
    }

    #[test]
    fn splash_gating_hydration_commands_are_async() {
        let source = include_str!("lib.rs");
        assert!(source.contains("async fn load_config("));
        for command in [
            "load_repositories",
            "load_ui_prefs",
            "load_notification_config",
            "load_repo_settings",
            "load_repo_defaults",
            "load_prompt_library",
            "load_notes",
            "load_activity",
            "load_keybindings",
            "load_agents_config",
        ] {
            assert!(
                source.contains(&format!("pub(super) async fn {command}_async(")),
                "{command} must yield to the async runtime during parallel hydration"
            );
        }
    }

    #[test]
    fn cli_auto_update_is_deferred_off_tauri_setup() {
        let source = include_str!("lib.rs");
        let deferred_call = ["spawn_blocking(tuic_cli::", "auto_update_cli)"].concat();
        assert!(
            source.contains(&deferred_call),
            "CLI version probes and replacement must not block Tauri setup"
        );
    }

    /// `gh auth token` reads the OS credential store and can hang there. It ran
    /// synchronously before the window was built, so a wedged `gh` meant no
    /// window at all. Boot now takes only the env vars — which cost nothing and
    /// outrank every other source — and `setup()` finishes the chain.
    #[test]
    fn boot_takes_only_the_env_github_token_and_defers_the_rest() {
        let source = include_str!("lib.rs");
        let desktop_run = source
            .split("pub fn run()")
            .nth(1)
            .expect("desktop run function")
            .split("fn build_connect_url")
            .next()
            .expect("desktop run body");

        assert!(
            desktop_run.contains("github_auth::resolve_token_from_env()"),
            "boot must take only the env token — every other source spawns `gh`"
        );
        assert!(
            !desktop_run.contains("github_auth::resolve_token_without_keychain()"),
            "that chain still spawns `gh`; it must not run before the window exists"
        );
        assert!(
            desktop_run.contains("github_auth::spawn_deferred_token_resolution("),
            "the rest of the chain must still run, or GitHub panels silently see no token"
        );

        // The daemon has no window, but the chain still ran before its HTTP
        // server bound its socket — a wedged `gh` kept the server unreachable
        // instead of the window unpainted. Same treatment.
        let entry = "pub async fn run_remote(";
        let body = source
            .split(entry)
            .nth(1)
            .unwrap_or_else(|| panic!("{entry} must exist"))
            .split("\n}\n")
            .next()
            .expect("entry body");
        assert!(
            body.contains("github_auth::resolve_token_from_env()")
                && body.contains("github_auth::spawn_deferred_token_resolution("),
            "{entry} must take the env token and defer the rest, like the desktop boot"
        );
        assert!(
            !body.contains("github_auth::resolve_token_without_keychain()"),
            "{entry} must not run the `gh`-spawning chain before its server binds"
        );
    }

    /// Catches: a headless daemon that never writes `agent-hooks/claude.json`, so
    /// a `claude` spawn fails with "Settings file not found".
    #[test]
    fn every_boot_path_writes_the_launch_assets_through_the_shared_helper() {
        let source = include_str!("lib.rs");
        for entry in ["pub fn run()", "pub async fn run_remote("] {
            let body = source
                .split(entry)
                .nth(1)
                .unwrap_or_else(|| panic!("{entry} must exist"))
                .split("\n}\n")
                .next()
                .expect("entry body");
            assert!(
                body.contains("agent_hook_launch::regenerate_launch_assets_at_boot("),
                "{entry} must write the launch assets before spawning agents"
            );
        }
    }

    #[test]
    fn desktop_setup_reuses_the_config_loaded_at_process_start() {
        let source = include_str!("lib.rs");
        let desktop_run = source
            .split("pub fn run()")
            .nth(1)
            .expect("desktop run function")
            .split("fn build_connect_url")
            .next()
            .expect("desktop run body");

        assert_eq!(
            desktop_run.matches("config::load_app_config()").count(),
            1,
            "setup must reuse the boot config instead of taking the file lock again for index_strategy"
        );
    }

    #[tokio::test]
    async fn initial_server_runtime_stays_alive_after_tcp_shutdown() {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let owner = tokio::spawn(keep_server_owner_runtime_alive(async move {
            let _ = shutdown_rx.await;
        }));

        shutdown_tx.send(()).unwrap();
        tokio::task::yield_now().await;
        assert!(
            !owner.is_finished(),
            "the runtime owner must remain parked after start_server returns"
        );
        owner.abort();
    }

    #[test]
    fn build_connect_url_ipv4() {
        assert_eq!(
            build_connect_url("http", "192.168.1.1", 8080, "abc-123"),
            "http://192.168.1.1:8080/?token=abc-123"
        );
    }

    #[test]
    fn build_connect_url_ipv6() {
        assert_eq!(
            build_connect_url("http", "fe80::1", 9443, "tok"),
            "http://[fe80::1]:9443/?token=tok"
        );
    }

    #[test]
    fn read_text_file_guarded_reads_small_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("small.txt");
        std::fs::write(&f, "hello world").unwrap();
        assert_eq!(read_text_file_guarded(&f).unwrap(), "hello world");
    }

    #[test]
    fn read_text_file_guarded_refuses_oversized_file() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("huge.txt");
        // One byte over the limit must be refused BEFORE reading, with a stable
        // "too large" phrase the editor frontend matches.
        std::fs::write(&f, vec![b'a'; (MAX_EDITOR_FILE_SIZE + 1) as usize]).unwrap();
        let err = read_text_file_guarded(&f).unwrap_err();
        assert!(
            err.to_lowercase().contains("too large"),
            "expected a 'too large' message, got: {err}"
        );
    }

    #[test]
    fn read_text_file_guarded_allows_file_at_limit() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("atlimit.bin");
        // Exactly at the limit (not over) is allowed; content is valid UTF-8.
        std::fs::write(&f, vec![b'a'; MAX_EDITOR_FILE_SIZE as usize]).unwrap();
        assert!(read_text_file_guarded(&f).is_ok());
    }

    #[test]
    fn read_text_file_guarded_with_limit_respects_custom_limit() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sized.txt");
        std::fs::write(&f, vec![b'a'; 50]).unwrap();
        // Under a generous limit → read; over a tight limit → refused "too large".
        assert!(read_text_file_guarded_with_limit(&f, 100).is_ok());
        let err = read_text_file_guarded_with_limit(&f, 10).unwrap_err();
        assert!(
            err.to_lowercase().contains("too large"),
            "expected a 'too large' message, got: {err}"
        );
    }

    #[test]
    fn editor_large_cap_exceeds_generic_cap() {
        // The editor reader must allow strictly larger files than the generic one,
        // otherwise the dedicated command is pointless.
        const { assert!(MAX_EDITOR_LARGE_FILE_SIZE > MAX_EDITOR_FILE_SIZE) };
    }

    #[test]
    fn read_editor_file_external_requires_absolute_path() {
        let err = read_external_file_with_limit("relative/path.txt", MAX_EDITOR_LARGE_FILE_SIZE)
            .unwrap_err();
        assert!(
            err.contains("absolute path"),
            "expected an absolute-path error, got: {err}"
        );
    }

    #[test]
    fn build_connect_url_localhost() {
        assert_eq!(
            build_connect_url("http", "127.0.0.1", 3000, "t"),
            "http://127.0.0.1:3000/?token=t"
        );
    }

    #[test]
    fn build_connect_url_https_fqdn() {
        assert_eq!(
            build_connect_url("https", "myhost.tail-abc.ts.net", 9876, "tok"),
            "https://myhost.tail-abc.ts.net:9876/?token=tok"
        );
    }
}
