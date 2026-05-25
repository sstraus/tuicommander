#![cfg_attr(
    not(feature = "desktop"),
    allow(dead_code, unused_imports, unused_variables)
)]

pub(crate) mod agent;
pub(crate) mod agent_mcp;
pub(crate) mod agent_session;
pub(crate) mod ai_agent;
pub(crate) mod ai_chat;
pub(crate) mod ai_chat_registry;
pub(crate) mod app_logger;
pub(crate) mod chrome;
pub(crate) mod claude_usage;
pub(crate) mod cli;
pub(crate) mod config;
pub(crate) mod content_index;
pub(crate) mod credentials;
#[cfg(feature = "desktop")]
mod dictation;
pub(crate) mod diff_triage;
pub(crate) mod dir_watcher;
pub(crate) mod error_classification;
pub(crate) mod fs;
pub(crate) mod generators;
pub(crate) mod git;
pub(crate) mod git_cli;
pub(crate) mod git_graph;
pub(crate) mod github;
pub(crate) mod github_auth;
pub(crate) mod github_debug;
pub(crate) mod github_poller;
#[cfg(feature = "desktop")]
mod global_hotkey;
mod input_line_buffer;
pub(crate) mod llm_api;
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
#[cfg(feature = "desktop")]
mod menu;
#[cfg(feature = "desktop")]
mod native_drag;
#[cfg(feature = "desktop")]
pub(crate) mod notification_sound;
mod output_parser;
#[cfg(feature = "desktop")]
mod panel_window;
pub(crate) mod plugin_credentials;
pub(crate) mod plugin_exec;
pub(crate) mod plugin_fs;
pub(crate) mod plugin_http;
pub(crate) mod plugin_pty;
pub(crate) mod plugins;
pub(crate) mod process_env;
pub(crate) mod prompt;
pub(crate) mod provider_registry;
pub(crate) mod pty;
pub(crate) mod push;
pub(crate) mod registry;
pub(crate) mod relay_client;
#[allow(dead_code)] // Constructors used by remote binary and future tests
pub(crate) mod remote_connection;
pub(crate) mod repo_watcher;
mod shell_integration;
#[cfg(feature = "desktop")]
pub(crate) mod sleep_prevention;
pub(crate) mod smart_prompt;
pub(crate) mod state;
#[cfg(feature = "desktop")]
mod tab_shortcut;
pub(crate) mod tailscale;
pub(crate) mod terminal_grid;
pub(crate) mod text_rank;
pub(crate) mod themes;
pub(crate) mod tool_search;
#[cfg(feature = "desktop")]
mod tuic_cli;
#[allow(dead_code)] // Many items used only by the remote binary (not(desktop) build)
pub(crate) mod tunnels;
#[cfg(feature = "desktop")]
mod updater;
pub(crate) mod worktree;

use std::path::{Path, PathBuf};
use std::sync::Arc;
#[cfg(feature = "desktop")]
use tauri::{Emitter, Manager, State, WebviewWindow};

// Re-export shared types from state module
pub(crate) use state::MAX_CONCURRENT_SESSIONS;
#[cfg(test)]
pub(crate) use state::SessionMetrics;
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
fn load_config(state: State<'_, Arc<AppState>>) -> config::AppConfig {
    state.config.read().clone()
}

#[cfg(feature = "desktop")]
/// Save configuration to disk, update the AppState cache, and live-restart the HTTP server
/// if MCP / Remote Access settings changed (no app restart required).
#[tauri::command]
fn save_config(state: State<'_, Arc<AppState>>, config: config::AppConfig) -> Result<(), String> {
    let old = state.config.read().clone();
    let server_changed = old.services.server.enabled != config.services.server.enabled
        || old.services.server.port != config.services.server.port
        || old.services.auth.username != config.services.auth.username
        || old.services.auth.password_hash != config.services.auth.password_hash
        || old.services.server.ipv6_enabled != config.services.server.ipv6_enabled;

    let tools_changed = old.disabled_native_tools != config.disabled_native_tools
        || old.collapse_tools != config.collapse_tools;

    config::save_app_config(config.clone())?; // clone goes to disk
    *state.config.write() = config; // move original into state

    if tools_changed {
        let _ = state.mcp_tools_changed.send(());
    }

    if server_changed {
        restart_server(state.inner());
    }

    Ok(())
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

/// Receive a screenshot response from the frontend (captured iframe content).
/// Pairs with the `screenshot-request` Tauri event emitted by `ui(action=screenshot)`.
#[cfg(feature = "desktop")]
#[tauri::command]
fn screenshot_response(state: State<'_, Arc<AppState>>, request_id: String, data: Option<String>) {
    if let Some((_, sender)) = state.screenshot_responses.remove(&request_id) {
        let _ = sender.send(data);
    }
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

/// Read file content (shared logic)
pub(crate) fn read_file_impl(path: String, file: String) -> Result<String, String> {
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

    std::fs::read_to_string(&file_path).map_err(|e| format!("Failed to read file: {e}"))
}

#[cfg_attr(feature = "desktop", tauri::command)]
fn read_file(path: String, file: String) -> Result<String, String> {
    read_file_impl(path, file)
}

/// Read a file by absolute path (read-only, no repo constraint).
/// Used for viewing files outside the active repository (e.g. drag & drop).
///
/// No TCC directory blocking: reading a specific file by known path does not
/// trigger macOS permission dialogs (TCC guards directory enumeration, not
/// individual reads). The HTTP endpoint has its own repo-root check.
#[cfg_attr(feature = "desktop", tauri::command)]
fn read_external_file(path: String) -> Result<String, String> {
    let p = std::path::Path::new(&path);
    if !p.is_absolute() {
        return Err("read_external_file requires an absolute path".to_string());
    }
    std::fs::read_to_string(p).map_err(|e| format!("Failed to read file: {e}"))
}

/// Write a file at an absolute path (used by the UI for files outside any registered repo,
/// e.g. markdown files opened via absolute path without a git root).
///
/// Target must be inside the user's home directory — see
/// [`crate::fs::validate_external_write_path`] for the full rationale (story 1273-c95e).
#[cfg_attr(feature = "desktop", tauri::command)]
fn write_external_file(path: String, content: String) -> Result<(), String> {
    let p = std::path::Path::new(&path);
    let home =
        dirs::home_dir().ok_or_else(|| "Could not resolve user home directory".to_string())?;
    fs::validate_external_write_path(p, &home)?;
    std::fs::write(p, content).map_err(|e| format!("Failed to write file: {e}"))
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
            state.sessions.len(),
            state.mcp_sessions.len(),
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
    args.insert("action".to_string(), serde_json::Value::String(action));

    let addr: std::net::SocketAddr = ([127, 0, 0, 1], 0).into();
    let result = mcp_http::mcp_transport::handle_mcp_tool_call(
        &state.inner().clone(),
        addr,
        &tool,
        &serde_json::Value::Object(args),
        None,
    )
    .await;

    Ok(result)
}

/// Regenerate the session token, invalidating all existing remote sessions.
#[cfg(feature = "desktop")]
#[tauri::command]
fn regenerate_session_token(state: State<'_, Arc<AppState>>) {
    let new_token = uuid::Uuid::new_v4().to_string();
    *state.session_token.write() = new_token.clone();
    // Persist so the new token survives restarts
    let mut cfg = state.config.read().clone();
    cfg.services.auth.session_token = new_token;
    if let Err(e) = config::save_app_config(cfg) {
        tracing::error!(
            source = "auth",
            "Failed to persist regenerated session token: {e}"
        );
    }
}

/// Build a QR-code connect URL server-side so the raw session token
/// never reaches JS (where a malicious plugin could steal it).
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
fn restart_server(state: &Arc<AppState>) {
    // Shutdown existing server
    if let Some(tx) = state.server_shutdown.lock().take() {
        let _ = tx.send(());
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
        restart_server(&state);
    }

    Ok(new_state)
}

/// Get relay client status (enabled, connected, url, session_id).
#[cfg(feature = "desktop")]
#[tauri::command]
fn get_relay_status(state: State<'_, Arc<AppState>>) -> serde_json::Value {
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

#[cfg(feature = "desktop")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        tracing::info!(source = "auth", "Generated persistent session token");
        config_dirty = true;
    }
    if config_dirty && let Err(e) = config::save_app_config(config.clone()) {
        tracing::error!(source = "app", "Failed to persist config: {e}");
    }

    let (github_token, github_token_source) = crate::github_auth::resolve_token_without_keychain();
    if github_token.is_none() {
        tracing::info!(
            source = "github",
            "No GitHub token from env/CLI — keychain deferred until first use"
        );
    }

    let data_dir = config::config_dir();

    let mut app_state = AppState::new(data_dir, worktrees_dir, config.clone(), log_buffer);
    *app_state.github_token.get_mut() = github_token;
    *app_state.github_token_source.get_mut() = github_token_source;

    let state = Arc::new(app_state);
    state.wire_event_bus();

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

                // Detect Tailscale and provision TLS cert (async, doesn't block window render)
                let tls_config = if remote_enabled {
                    let ts_state = tokio::task::spawn_blocking(tailscale::detect)
                        .await
                        .unwrap_or(tailscale::TailscaleState::NotInstalled);
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

                // Bind the IPC socket BEFORE upstream auto-connect so the MCP
                // bridge is reachable as soon as possible. Upstream connections
                // can be slow (network timeouts, OAuth) and must not delay socket
                // availability — that causes "MCP disconnected" in Claude Code.
                let srv_state = server_state.clone();
                mcp_http::start_server(srv_state, true, remote_enabled, tls_config).await;

                // Auto-connect saved upstream MCP servers (after socket is live)
                crate::mcp_upstream_config::auto_connect_saved_upstreams(&server_state).await;
            });
        });
    }

    // Ensure MCP bridge config is installed and up-to-date in all agent configs.
    // Runs every launch: installs missing entries and updates stale paths.
    // Skips agents the user explicitly disabled via Settings > Agents.
    agent_mcp::ensure_mcp_configs(&config.disabled_mcp_agents);

    // Start relay client if configured
    if config.services.relay.enabled {
        let (relay_tx, relay_rx) = tokio::sync::oneshot::channel();
        *state.relay.shutdown.lock() = Some(relay_tx);
        let relay_state = state.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime for relay client");
            rt.block_on(relay_client::run(relay_state, relay_rx));
        });
    }

    sanitize_window_state();

    let builder = tauri::Builder::default();
    let builder = plugins::register_plugin_protocol(builder);
    let builder = builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri::plugin::Builder::<tauri::Wry, ()>::new("navigation-guard")
                .on_navigation(|_webview, url| {
                    // Allow internal navigation (tauri://, localhost in dev,
                    // and http://tauri.localhost/ on Windows production builds)
                    let scheme = url.scheme();
                    if scheme == "tauri" || scheme == "asset" || scheme == "plugin" {
                        return true;
                    }
                    let host = url.host_str().unwrap_or("");
                    if host == "tauri.localhost" || host == "localhost" || host == "127.0.0.1" {
                        return true;
                    }
                    // External URL — open in system browser, block webview navigation
                    if scheme == "http" || scheme == "https" {
                        let url_str = url.to_string();
                        tracing::info!(url = %url_str, "Opening external URL in browser");
                        #[cfg(target_os = "macos")]
                        let _ = std::process::Command::new("open").arg(&url_str).spawn();
                        #[cfg(target_os = "linux")]
                        let _ = std::process::Command::new("xdg-open").arg(&url_str).spawn();
                        #[cfg(target_os = "windows")]
                        {
                            let mut cmd = std::process::Command::new("cmd");
                            cmd.args(["/c", "start", &url_str]);
                            cli::apply_no_window(&mut cmd);
                            let _ = cmd.spawn();
                        }
                        return false;
                    }
                    true
                })
                .on_page_load(|webview, payload| {
                    // WebKit WebContent process crashes leave the WebView on about:blank.
                    // Detect this and force-reload the embedded app page.
                    if payload.event() == tauri::webview::PageLoadEvent::Finished
                        && payload.url().as_str() == "about:blank"
                    {
                        tracing::error!(
                            source = "webview",
                            label = webview.label(),
                            "WebView landed on about:blank — WebContent likely crashed, reloading app"
                        );
                        let label = webview.label().to_string();
                        let handle = webview.app_handle().clone();
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            if let Some(wv) = handle.get_webview_window(&label) {
                                let target: url::Url = "tauri://localhost/".parse().unwrap();
                                let _ = wv.navigate(target);
                            }
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
        .manage(ai_chat_registry::ChatRegistry::new())
        .manage(crate::fs::ContentSearchCancel(std::sync::Mutex::new(None)))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_clipboard_manager::init())
;

    #[cfg(feature = "desktop")]
    let builder = builder
        .manage(dictation::DictationState::new())
        .manage(sleep_prevention::SleepBlocker::new());

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
        .setup(|app| {
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

            // Track desktop window focus so push notifications can be
            // suppressed while the user is at their machine.
            if let Some(window) = app.get_webview_window("main") {
                let push_flag = Arc::clone(app_state);
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(focused) = event {
                        push_flag
                            .desktop_window_focused
                            .store(*focused, std::sync::atomic::Ordering::Relaxed);
                    }
                });
            }

            #[cfg(feature = "desktop")]
            {
                // Install global hotkey plugin (registers handler, no shortcuts yet)
                if let Err(e) = global_hotkey::init(app.handle()) {
                    tracing::warn!(source = "global-hotkey", "Failed to init plugin: {e}");
                } else {
                    global_hotkey::restore_from_config(app.handle());
                }

                // Install Fn/Globe key monitor for push-to-talk dictation
                dictation::fn_key_monitor::install(app.handle().clone());

                // Install Ctrl+Tab monitor (macOS swallows it before JS/WKWebView)
                tab_shortcut::install(app.handle().clone());
            }

            // Seed built-in themes on first run, then start hot-reload watcher
            let themes_dir = config::config_dir().join("themes");
            if let Err(e) = themes::seed_builtin_themes(&themes_dir) {
                tracing::warn!("Failed to seed built-in themes: {e}");
            }
            themes::start_theme_watcher(themes_dir, app_state);

            // Start plugin directory watcher for hot-reload
            plugins::start_plugin_watcher(app.handle());

            // Auto-start repo watchers for known repositories.
            // Uses raw notify::RecommendedWatcher — registration is instant on
            // macOS (FSEvents) and Windows (ReadDirectoryChangesW), no walkdir scan.
            let repos_json = config::load_repositories();
            let mut known_repo_paths: Vec<String> = Vec::new();
            if let Some(repos) = repos_json.get("repos").and_then(|r| r.as_object()) {
                for repo_path in repos.keys() {
                    known_repo_paths.push(repo_path.clone());
                    if let Err(e) = repo_watcher::start_watching(repo_path, app_state) {
                        app_logger::log_via_state(
                            app_state,
                            "warn",
                            "app",
                            &format!("[RepoWatcher] Failed to watch {repo_path}: {e}"),
                        );
                    }
                }
            }

            // Auto-update CLI binary if installed
            #[cfg(feature = "desktop")]
            tuic_cli::auto_update_cli(app.handle());

            // Pre-warm content indices based on index_strategy setting:
            // - "active_only": only the active repo at boot
            // - "active_and_switch": active repo at boot, others on repo switch (default)
            // - "all_sequential": all repos sequentially
            // Global semaphore in AppState (capacity 1) serialises concurrent builds.
            if !known_repo_paths.is_empty() {
                let active_repo = repos_json
                    .get("activeRepoPath")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_owned());

                let strategy = config::load_app_config().index_strategy;

                let repos_to_warm = match strategy.as_str() {
                    "all_sequential" => {
                        if let Some(ref active) = active_repo
                            && let Some(pos) = known_repo_paths.iter().position(|p| p == active)
                        {
                            known_repo_paths.swap(0, pos);
                        }
                        known_repo_paths
                    }
                    _ => {
                        // active_only and active_and_switch: only pre-warm the active repo
                        if let Some(active) = active_repo {
                            if known_repo_paths.contains(&active) {
                                vec![active]
                            } else {
                                Vec::new()
                            }
                        } else {
                            Vec::new()
                        }
                    }
                };

                if !repos_to_warm.is_empty() {
                    let state_for_prewarm = Arc::clone(app_state);
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        for repo in repos_to_warm {
                            let index_arc =
                                crate::content_index::ensure_index(&state_for_prewarm, &repo);
                            while !index_arc.read().is_ready() {
                                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                            }
                        }
                        tracing::info!("content index pre-warm complete");
                    });
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            generators::generate_value,
            native_drag::start_native_drag,
            remote_connection::list_remote_connections,
            remote_connection::save_remote_connection,
            remote_connection::delete_remote_connection,
            open_secondary_window,
            panel_window::open_panel_window,
            panel_window::focus_panel_window,
            panel_window::close_panel_window,
            panel_window::focus_main_window,
            pty::create_pty,
            pty::create_pty_with_worktree,
            pty::list_worktrees,
            pty::write_pty,
            pty::get_input_buffer_content,
            pty::resize_pty,
            pty::set_ansi_colors,
            pty::pause_pty,
            pty::resume_pty,
            pty::get_kitty_flags,
            pty::get_last_prompt,
            pty::get_shell_state,
            pty::get_session_shell_family,
            pty::close_pty,
            worktree::get_worktrees_dir,
            git::get_repo_info,
            git::get_git_diff,
            git::get_diff_stats,
            git::get_changed_files,
            git::get_file_diff,
            diff_triage::run_diff_triage,
            git::get_recent_commits,
            list_markdown_files,
            read_file,
            read_external_file,
            write_external_file,
            github::get_github_status,
            pty::get_orchestrator_stats,
            pty::get_session_metrics,
            pty::can_spawn_session,
            pty::list_active_sessions,
            pty::get_process_stats,
            pty::read_vt_log,
            pty::subscribe_terminal_grid,
            pty::unsubscribe_terminal_grid,
            pty::terminal_request_frame,
            pty::ack_terminal_frame,
            pty::terminal_exit_alt_screen,
            pty::terminal_scroll,
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
            pty::update_session_cwd,
            pty::set_session_name,
            pty::get_session_foreground_process,
            pty::get_session_leaf_pid,
            pty::has_foreground_process,
            pty::debug_agent_detection,
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
            agent::detect_claude_binary,
            agent::detect_agent_binary,
            agent::detect_all_agent_binaries,
            agent::spawn_agent,
            agent_session::discover_agent_session,
            agent_session::verify_agent_session,
            agent_session::claude_project_dir,
            worktree::remove_worktree,
            worktree::check_worktree_dirty,
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
            github::get_repo_pr_statuses,
            github::get_all_pr_statuses,
            github::merge_pr_via_github,
            github::get_pr_diff,
            github::approve_pr,
            github::fetch_ci_failure_logs,
            github::get_all_issues,
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
            github_auth::github_logout,
            github_auth::github_disconnect,
            github_auth::github_diagnostics,
            github_auth::github_auth_status,
            worktree::generate_worktree_name_cmd,
            worktree::generate_clone_branch_name_cmd,
            worktree::merge_and_archive_worktree,
            worktree::finalize_merged_worktree,
            worktree::list_local_branches,
            worktree::list_base_ref_options,
            worktree::switch_branch,
            worktree::checkout_remote_branch,
            worktree::detect_orphan_worktrees,
            worktree::remove_orphan_worktree,
            worktree::run_setup_script,
            clear_caches,
            clear_repo_caches,
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
            dictation::commands::check_microphone_permission,
            dictation::commands::open_microphone_settings,
            global_hotkey::set_global_hotkey,
            global_hotkey::get_global_hotkey,
            config::load_app_config,
            config::save_app_config,
            config::load_notification_config,
            config::save_notification_config,
            config::load_ui_prefs,
            config::save_ui_prefs,
            config::load_repo_settings,
            config::save_repo_settings,
            config::set_branch_label,
            config::load_repo_local_config,
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
            config::load_repo_defaults,
            config::save_repo_defaults,
            config::load_repositories,
            config::save_repositories,
            config::load_pane_layout,
            config::save_pane_layout,
            config::load_prompt_library,
            config::save_prompt_library,
            config::load_ai_prompts,
            config::save_ai_prompts,
            config::load_notes,
            config::save_notes,
            config::save_note_image,
            config::delete_note_assets,
            config::delete_note_assets_batch,
            config::get_note_images_dir,
            config::load_activity,
            config::save_activity,
            config::load_keybindings,
            config::save_keybindings,
            config::load_agents_config,
            config::save_agents_config,
            agent_mcp::get_agent_mcp_status,
            agent_mcp::install_agent_mcp,
            agent_mcp::remove_agent_mcp,
            agent_mcp::get_agent_config_path,
            agent_mcp::get_mcp_bridge_info,
            prompt::extract_prompt_variables,
            prompt::process_prompt_content,
            prompt::process_prompt_content_shell_safe,
            prompt::resolve_context_variables,
            prompt::resolve_prompt_variables,
            smart_prompt::execute_headless_prompt,
            smart_prompt::execute_shell_script,
            provider_registry::load_provider_registry,
            provider_registry::save_provider_registry,
            provider_registry::get_provider_api_key_exists,
            provider_registry::save_provider_api_key,
            provider_registry::delete_provider_api_key,
            provider_registry::test_slot_connection,
            provider_registry::check_ollama_models,
            llm_api::execute_api_prompt,
            ai_chat::load_ai_chat_config,
            ai_chat::save_ai_chat_config,
            ai_chat::list_conversations,
            ai_chat::load_conversation,
            ai_chat::save_conversation,
            ai_chat::delete_conversation,
            ai_chat::new_conversation_id,
            ai_chat_registry::chat_subscribe,
            ai_chat_registry::chat_unsubscribe,
            ai_agent::commands::start_conversation,
            ai_agent::commands::cancel_conversation,
            ai_agent::commands::pause_conversation,
            ai_agent::commands::resume_conversation,
            ai_agent::commands::approve_conversation_action,
            ai_agent::commands::agent_loop_status,
            ai_agent::commands::get_session_knowledge,
            ai_agent::commands::toggle_ai_suggestions,
            ai_agent::commands::get_ai_suggestions_enabled,
            ai_agent::commands::list_knowledge_sessions,
            ai_agent::commands::get_knowledge_session_detail,
            ai_agent::commands::load_scheduler_config,
            ai_agent::commands::save_scheduler_config,
            ai_agent::commands::watcher_create,
            ai_agent::commands::watcher_list,
            ai_agent::commands::watcher_delete,
            ai_agent::commands::watcher_toggle,
            ai_agent::commands::watcher_attach,
            ai_agent::commands::watcher_detach,
            ai_agent::commands::watcher_update,
            repo_watcher::start_repo_watcher,
            repo_watcher::stop_repo_watcher,
            repo_watcher::set_hot_repos,
            dir_watcher::start_dir_watcher,
            dir_watcher::stop_dir_watcher,
            sleep_prevention::block_sleep,
            sleep_prevention::unblock_sleep,
            fs::resolve_terminal_path,
            fs::list_directory,
            fs::stat_path,
            fs::search_files,
            fs::warm_content_index,
            fs::search_content,
            fs::fs_read_file,
            fs::write_file,
            fs::create_directory,
            fs::delete_path,
            fs::rename_path,
            fs::copy_path,
            fs::fs_transfer_paths,
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
            plugin_fs::plugin_read_file,
            plugin_fs::plugin_list_directory,
            plugin_fs::plugin_read_file_tail,
            plugin_fs::plugin_write_file,
            plugin_fs::plugin_rename_path,
            plugin_fs::plugin_watch_path,
            plugin_fs::plugin_unwatch,
            plugin_http::plugin_http_fetch,
            plugin_pty::plugin_read_session_output,
            plugin_exec::plugin_exec_cli,
            plugin_credentials::plugin_read_credential,
            registry::fetch_plugin_registry,
            claude_usage::get_claude_usage_api,
            claude_usage::get_claude_usage_timeline,
            claude_usage::get_claude_session_stats,
            claude_usage::get_claude_project_list,
            screenshot_response,
            app_logger::push_log,
            app_logger::get_logs,
            app_logger::clear_logs,
            notification_sound::play_notification_sound,
            notification_sound::list_audio_output_devices,
            git_graph::get_commit_graph,
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
            tunnels::tauri_commands::list_ssh_agent_keys,
            tunnels::tauri_commands::get_tunnel_audit
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
                    if let Some(dictation) = app_handle.try_state::<dictation::DictationState>() {
                        dictation.shutdown();
                    }
                    // Kill all SSH tunnel processes so ports are freed for restart
                    if let Some(state) = app_handle.try_state::<Arc<AppState>>() {
                        state.tunnel_manager.shutdown_all();
                        crate::ai_agent::knowledge::flush_dirty(state.inner());
                    }
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
    AppState::spawn_session_state_accumulator(state.clone());
    mcp_http::mcp_transport::spawn_tool_search_index_updater(state.clone());
    pty::spawn_tombstone_sweeper(state.clone());
    content_index::spawn_content_index_updater(state.clone());
    ai_agent::knowledge::spawn_persist_task(state.clone());
    {
        let sched_state = state.clone();
        tokio::spawn(async move {
            let scheduler = ai_agent::scheduler::Scheduler::new(sched_state);
            scheduler.run().await;
        });
    }
    {
        let watcher_state = state.clone();
        let engine = Arc::new(ai_agent::watcher::WatcherEngine::new(watcher_state));
        if state.watcher_engine.set(Arc::clone(&engine)).is_err() {
            tracing::error!(
                "WatcherEngine already initialized — duplicate spawn_background_tasks call"
            );
        }
        tokio::spawn(async move {
            engine.run().await;
        });
    }
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

/// Run the headless (non-desktop) server.
/// Called by the `tuic-remote` binary.
#[cfg(not(feature = "desktop"))]
pub async fn run_headless(port: u16) -> anyhow::Result<()> {
    let log_buffer = Arc::new(parking_lot::Mutex::new(app_logger::LogRingBuffer::new(
        app_logger::LOG_RING_CAPACITY,
    )));
    app_logger::init_tracing(log_buffer.clone());

    let mut app_config = config::load_app_config();
    app_config.services.server.enabled = true;
    if app_config.services.server.port != port {
        tracing::info!(
            source = "remote",
            config_port = app_config.services.server.port,
            override_port = port,
            "Port overridden by TUIC_PORT / CLI argument"
        );
        app_config.services.server.port = port;
    }
    if app_config.services.auth.lan_auth_bypass {
        tracing::warn!(
            source = "remote",
            "lan_auth_bypass is not supported in headless mode — forcing off"
        );
        app_config.services.auth.lan_auth_bypass = false;
    }
    if app_config.services.auth.session_token.is_empty() {
        app_config.services.auth.session_token = uuid::Uuid::new_v4().to_string();
    }

    let data_dir = config::config_dir();
    let worktrees_dir = data_dir.join("worktrees");
    std::fs::create_dir_all(&worktrees_dir)?;

    let (github_token, github_token_source) = crate::github_auth::resolve_token_without_keychain();

    let mut app_state = AppState::new(data_dir, worktrees_dir, app_config.clone(), log_buffer);
    *app_state.github_token.get_mut() = github_token;
    *app_state.github_token_source.get_mut() = github_token_source;

    let state = Arc::new(app_state);
    state.wire_event_bus();

    spawn_background_tasks(&state);

    agent_mcp::ensure_mcp_configs(&app_config.disabled_mcp_agents);

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

    tracing::info!(
        source = "remote",
        port,
        tls = tls_config.is_some(),
        "Starting tuic-remote"
    );

    // Run server until SIGINT/SIGTERM, then shut down gracefully.
    tokio::select! {
        tcp_bound = mcp_http::start_server(state.clone(), true, true, tls_config) => {
            if !tcp_bound {
                anyhow::bail!("Fatal: failed to bind TCP on port {port} — cannot serve in headless mode");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!(source = "remote", "Received shutdown signal");
            if let Some(tx) = state.server_shutdown.lock().take() {
                let _ = tx.send(());
            }
        }
    }

    Ok(())
}

/// Run the tuic-remote server — a slim variant of `run_headless()`.
///
/// Differences from `run_headless()`:
/// - Uses `build_remote_router()` (no config, MCP, plugins, push, static files).
/// - Spawns only the two essential background tasks: session state accumulator
///   and tombstone sweeper.
/// - Logs "Starting tuic-remote" with the `protocol_version` field.
/// - Binds TCP directly without spawning an IPC socket.
#[cfg(not(feature = "desktop"))]
pub async fn run_remote(port: u16) -> anyhow::Result<()> {
    let log_buffer = Arc::new(parking_lot::Mutex::new(app_logger::LogRingBuffer::new(
        app_logger::LOG_RING_CAPACITY,
    )));
    app_logger::init_tracing(log_buffer.clone());

    let mut app_config = config::load_app_config();
    app_config.services.server.enabled = true;
    if app_config.services.server.port != port {
        tracing::info!(
            source = "remote",
            config_port = app_config.services.server.port,
            override_port = port,
            "Port overridden by TUIC_PORT / CLI argument"
        );
        app_config.services.server.port = port;
    }
    if app_config.services.auth.lan_auth_bypass {
        tracing::warn!(
            source = "remote",
            "lan_auth_bypass is not supported in headless mode — forcing off"
        );
        app_config.services.auth.lan_auth_bypass = false;
    }
    if app_config.services.auth.session_token.is_empty() {
        app_config.services.auth.session_token = uuid::Uuid::new_v4().to_string();
    }

    let data_dir = config::config_dir();
    let worktrees_dir = data_dir.join("worktrees");
    std::fs::create_dir_all(&worktrees_dir)?;

    let (github_token, github_token_source) = crate::github_auth::resolve_token_without_keychain();

    let mut app_state = AppState::new(data_dir, worktrees_dir, app_config.clone(), log_buffer);
    *app_state.github_token.get_mut() = github_token;
    *app_state.github_token_source.get_mut() = github_token_source;

    let state = Arc::new(app_state);
    state.wire_event_bus();

    // Only the two tasks required for session management — no scheduler,
    // watcher engine, content index, knowledge persist, or tool search index.
    AppState::spawn_session_state_accumulator(state.clone());
    pty::spawn_tombstone_sweeper(state.clone());

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

    const PROTOCOL_VERSION: u32 = 1;
    tracing::info!(
        source = "remote",
        port,
        tls = tls_config.is_some(),
        protocol_version = PROTOCOL_VERSION,
        "Starting tuic-remote"
    );

    let bind_addr = format!("0.0.0.0:{port}");
    let listener = std::net::TcpListener::bind(&bind_addr)
        .map_err(|e| anyhow::anyhow!("Fatal: failed to bind TCP on port {port}: {e}"))?;
    listener.set_nonblocking(true)?;
    let listener = tokio::net::TcpListener::from_std(listener)?;

    let router = mcp_http::build_remote_router(state.clone());
    let svc = router.into_make_service_with_connect_info::<std::net::SocketAddr>();

    tokio::select! {
        result = axum::serve(listener, svc) => {
            if let Err(e) = result {
                anyhow::bail!("TCP server error: {e}");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!(source = "remote", "Received shutdown signal");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
