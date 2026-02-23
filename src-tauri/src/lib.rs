pub(crate) mod agent;
pub(crate) mod agent_mcp;
pub(crate) mod cli;
pub(crate) mod config;
mod dictation;
pub(crate) mod error_classification;
pub(crate) mod fs;
pub(crate) mod git;
pub(crate) mod github;
pub(crate) mod head_watcher;
pub(crate) mod repo_watcher;
pub(crate) mod mcp_http;
mod menu;
mod output_parser;
pub(crate) mod plugin_credentials;
pub(crate) mod plugin_fs;
pub(crate) mod plugin_http;
pub(crate) mod plugins;
pub(crate) mod prompt;
pub(crate) mod registry;
pub(crate) mod pty;
pub(crate) mod sleep_prevention;
pub(crate) mod state;
pub(crate) mod worktree;

use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{Emitter, Manager, State, WebviewWindow};

// Re-export shared types from state module
pub(crate) use state::{AppState, OutputRingBuffer, PtySession};
pub(crate) use state::{SessionMetrics, MAX_CONCURRENT_SESSIONS};

/// Ensure the window has valid dimensions and is positioned on a visible monitor.
/// The window-state plugin can persist invalid state (e.g. width/height 0, or
/// positions off-screen) which causes downstream failures like PTY garbage output.
fn ensure_window_visible(window: &WebviewWindow) {
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
        eprintln!(
            "[WindowGuard] Invalid window state ({}x{} at {},{}) — resetting to defaults",
            size.width, size.height, pos.x, pos.y
        );
        if let Err(e) = window.set_size(tauri::PhysicalSize::new(1200u32, 800u32)) {
            eprintln!("[WindowGuard] Failed to reset size: {e}");
        }
        if let Err(e) = window.set_position(PhysicalPosition::new(100i32, 100i32)) {
            eprintln!("[WindowGuard] Failed to reset position: {e}");
        }
        if let Err(e) = window.center() {
            eprintln!("[WindowGuard] Failed to center window: {e}");
        }
    }
}

/// Load configuration from cached AppState
#[tauri::command]
fn load_config(state: State<'_, Arc<AppState>>) -> config::AppConfig {
    state.config.read().clone()
}

/// Save configuration to disk, update the AppState cache, and live-restart the HTTP server
/// if MCP / Remote Access settings changed (no app restart required).
#[tauri::command]
fn save_config(state: State<'_, Arc<AppState>>, config: config::AppConfig) -> Result<(), String> {
    let old = state.config.read().clone();
    let server_changed = old.mcp_server_enabled != config.mcp_server_enabled
        || old.remote_access_enabled != config.remote_access_enabled
        || old.remote_access_port != config.remote_access_port
        || old.remote_access_username != config.remote_access_username
        || old.remote_access_password_hash != config.remote_access_password_hash
        || old.ipv6_enabled != config.ipv6_enabled;

    // Capture flags before moving config into state
    let mcp_server_enabled = config.mcp_server_enabled;
    let remote_access_enabled = config.remote_access_enabled;

    config::save_app_config(config.clone())?;  // clone goes to disk
    *state.config.write() = config;             // move original into state

    if server_changed {
        // Shutdown existing server (if any)
        if let Some(tx) = state.server_shutdown.lock().take() {
            let _ = tx.send(());
        }

        // Start fresh server if either mode is now enabled
        if mcp_server_enabled || remote_access_enabled {
            let mcp_enabled = mcp_server_enabled;
            let remote_enabled = remote_access_enabled;
            let state_arc = state.inner().clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime for HTTP server restart");
                rt.block_on(async move {
                    mcp_http::start_server(state_arc, mcp_enabled, remote_enabled).await;
                });
            });
        }
    }

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
    let ipv6_enabled = state.config.read().ipv6_enabled;
    get_local_ips_with_config(ipv6_enabled)
}

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
        if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
            if sock.connect("8.8.8.8:80").is_ok() {
                if let Ok(addr) = sock.local_addr() {
                    let ip = addr.ip().to_string();
                    if !ip.starts_with("127.") {
                        result.push(LocalIpEntry { ip, label: "Network".to_string() });
                    }
                }
            }
        }
        // IPv6 route trick
        if ipv6_enabled {
            if let Ok(sock) = UdpSocket::bind("[::]:0") {
                if sock.connect("[2001:4860:4860::8888]:80").is_ok() {
                    if let Ok(addr) = sock.local_addr() {
                        let ip_str = addr.ip().to_string();
                        if !ip_str.starts_with("::1") {
                            let label = classify_ipv6_addr(&addr.ip());
                            result.push(LocalIpEntry { ip: ip_str, label });
                        }
                    }
                }
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
                        result.push(LocalIpEntry { ip: ip.to_string(), label });
                    }
                } else if ipv6_enabled && family == libc::AF_INET6 {
                    let sa6 = &*(ifa.ifa_addr as *const libc::sockaddr_in6);
                    let ip = Ipv6Addr::from(sa6.sin6_addr.s6_addr);
                    // Skip loopback (::1) and link-local (fe80::/10, requires scope ID)
                    if !ip.is_loopback() && (ip.segments()[0] & 0xffc0) != 0xfe80 {
                        let iface = iface_name();
                        let label = classify_ipv6(ip, &iface);
                        result.push(LocalIpEntry { ip: ip.to_string(), label });
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
fn pick_preferred_ip(ips: Vec<LocalIpEntry>) -> Option<String> {
    for label_prefix in &["Tailscale", "Wi-Fi", "LAN"] {
        if let Some(e) = ips.iter().find(|e| e.label.contains(label_prefix)) {
            return Some(e.ip.clone());
        }
    }
    ips.into_iter().next().map(|e| e.ip)
}

/// Legacy single-IP command kept for backwards compatibility.
/// Returns the LAN/Tailscale IP preferred for remote access, or the default-route IP.
#[tauri::command]
fn get_local_ip(state: State<'_, Arc<AppState>>) -> Option<String> {
    pick_preferred_ip(get_local_ips(state))
}


/// A markdown file with its git status
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct MarkdownFileEntry {
    pub path: String,
    /// Git status: "modified", "staged", "untracked", or "" (clean/ignored).
    pub git_status: String,
}

/// List all markdown files in a repository recursively, with git status (shared logic)
pub(crate) fn list_markdown_files_impl(path: String) -> Result<Vec<MarkdownFileEntry>, String> {
    let repo_path = PathBuf::from(&path);

    if !repo_path.exists() {
        return Err(format!("Path does not exist: {path}"));
    }

    // Walk the filesystem to find all .md files (fast, skips heavy dirs).
    // We avoid `git ls-files --others` which is extremely slow on large repos.
    fn walk_dir(dir: &Path, base: &Path, md_paths: &mut Vec<String>) -> std::io::Result<()> {
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
                    walk_dir(&path, base, md_paths)?;
                } else if path.extension().and_then(|s| s.to_str()) == Some("md")
                    && let Ok(relative) = path.strip_prefix(base) {
                        md_paths.push(relative.to_string_lossy().replace('\\', "/"));
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

    // Build entries with status
    let mut entries: Vec<MarkdownFileEntry> = md_paths
        .into_iter()
        .map(|p| {
            let git_status = git_statuses.get(&p).cloned().unwrap_or_default();
            MarkdownFileEntry { path: p, git_status }
        })
        .collect();

    // Sort files alphabetically
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

#[tauri::command]
fn list_markdown_files(path: String) -> Result<Vec<MarkdownFileEntry>, String> {
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

/// Get MCP server status (running, port, active sessions).
/// Async to avoid blocking the Tauri IPC thread during the TCP self-test.
#[tauri::command]
async fn get_mcp_status(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    // Collect config and session count synchronously first (fast, no I/O)
    let (remote_enabled, mcp_enabled, active_sessions, session_token) = {
        let cfg = state.config.read();
        (
            cfg.remote_access_enabled,
            cfg.mcp_server_enabled,
            state.sessions.len(),
            state.session_token.read().clone(),
        )
    };

    let port_file = config::config_dir().join("mcp-port");
    let port = std::fs::read_to_string(&port_file)
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok());

    let server_should_run = mcp_enabled || remote_enabled;
    let running = server_should_run && port.is_some();

    // Quick TCP self-test: can we reach the server on the external (non-loopback) IP?
    // A failure here usually means a firewall is blocking incoming connections.
    // Only runs when remote is enabled and we have an IP and port.
    // Uses spawn_blocking to avoid blocking the async executor during the TCP connect.
    let reachable = if remote_enabled {
        let preferred_ip = pick_preferred_ip(get_local_ips_with_config(state.config.read().ipv6_enabled));
        if let (Some(p), Some(ip)) = (port, preferred_ip) {
            // Bracket-wrap IPv6 literals for valid SocketAddr parsing
            let addr = if ip.contains(':') { format!("[{ip}]:{p}") } else { format!("{ip}:{p}") };
            tokio::task::spawn_blocking(move || {
                addr.parse::<std::net::SocketAddr>().ok().map(|sa| {
                    std::net::TcpStream::connect_timeout(
                        &sa,
                        std::time::Duration::from_millis(200),
                    )
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
        "enabled": mcp_enabled,
        "running": running,
        "port": port,
        "active_sessions": active_sessions,
        "max_sessions": MAX_CONCURRENT_SESSIONS,
        // session_token is the primary auth credential — included in the QR code URL.
        // Only exposed via this Tauri command (local IPC), never via the HTTP API.
        "session_token": session_token,
        // null = remote disabled, true = TCP connect succeeded, false = likely firewalled
        "reachable": reachable,
    }))
}

/// Regenerate the session token, invalidating all existing remote sessions.
/// Returns the new token so the frontend can refresh the QR code.
#[tauri::command]
fn regenerate_session_token(state: State<'_, Arc<AppState>>) -> String {
    let new_token = uuid::Uuid::new_v4().to_string();
    *state.session_token.write() = new_token.clone();
    new_token
}

/// Register TUI Commander as an MCP server with Claude Code CLI.
/// Runs `claude mcp add --transport http --scope user tui-commander <url>`.
#[tauri::command]
async fn register_mcp_with_claude(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let port = state.config.read().mcp_port;
    let url = format!("http://127.0.0.1:{}/mcp", port);

    let detection = crate::agent::detect_agent_binary("claude".to_string());
    let claude_bin = detection.path
        .ok_or_else(|| "Claude Code CLI not found on this system".to_string())?;

    let output = tokio::process::Command::new(&claude_bin)
        .args(["mcp", "add", "--transport", "http", "--scope", "user", "tui-commander", &url])
        .output()
        .await
        .map_err(|e| format!("Failed to run claude: {}", e))?;

    if output.status.success() {
        Ok(url)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
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
        mcp_sessions: DashMap::new(),
        ws_clients: DashMap::new(),
        config: parking_lot::RwLock::new(config.clone()),
        repo_info_cache: DashMap::new(),
        github_status_cache: DashMap::new(),
        head_watchers: DashMap::new(),
        repo_watchers: DashMap::new(),
        http_client: std::mem::ManuallyDrop::new(reqwest::blocking::Client::new()),
        github_token: parking_lot::RwLock::new(github_token),
        github_circuit_breaker: crate::github::GitHubCircuitBreaker::new(),
        server_shutdown: parking_lot::Mutex::new(None),
        session_token: parking_lot::RwLock::new(uuid::Uuid::new_v4().to_string()),
        app_handle: parking_lot::RwLock::new(None),
        plugin_watchers: DashMap::new(),
        kitty_states: DashMap::new(),
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

    let builder = tauri::Builder::default();
    let builder = plugins::register_plugin_protocol(builder);
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
        .manage(dictation::DictationState::new())
        .manage(sleep_prevention::SleepBlocker::new())
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // Focus existing window when another instance is launched (Story 065)
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            #[cfg(desktop)]
            app.handle().plugin(tauri_plugin_updater::Builder::new().build())?;

            let m = menu::build_menu(app)?;
            app.set_menu(m)?;
            app.on_menu_event(|app_handle, event| {
                let _ = app_handle.emit("menu-action", event.id().0.as_str());
            });

            // Store AppHandle so HTTP handlers can emit Tauri events
            let app_state: &Arc<AppState> = app.state::<Arc<AppState>>().inner();
            *app_state.app_handle.write() = Some(app.handle().clone());

            // Start plugin directory watcher for hot-reload
            plugins::start_plugin_watcher(app.handle());

            // Auto-start HEAD and repo watchers for known repositories
            let repos_json = config::load_repositories();
            if let Some(repos) = repos_json.get("repos").and_then(|r| r.as_object()) {
                let handle = app.handle().clone();
                for repo_path in repos.keys() {
                    if let Err(e) = head_watcher::start_watching(repo_path, &handle) {
                        eprintln!("[HeadWatcher] Failed to watch {repo_path}: {e}");
                    }
                    if let Err(e) = repo_watcher::start_watching(repo_path, &handle) {
                        eprintln!("[RepoWatcher] Failed to watch {repo_path}: {e}");
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
            git::get_recent_commits,
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
            agent::detect_all_agent_binaries,
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
            git::run_git_command,
            github::get_ci_checks,
            github::get_repo_pr_statuses,
            worktree::generate_worktree_name_cmd,
            worktree::list_local_branches,
            clear_caches,
            get_local_ip,
            get_local_ips,
            get_mcp_status,
            regenerate_session_token,
            register_mcp_with_claude,
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
            config::load_notification_config,
            config::save_notification_config,
            config::load_ui_prefs,
            config::save_ui_prefs,
            config::load_repo_settings,
            config::save_repo_settings,
            config::check_has_custom_settings,
            config::load_repo_defaults,
            config::save_repo_defaults,
            config::load_repositories,
            config::save_repositories,
            config::load_prompt_library,
            config::save_prompt_library,
            config::load_notes,
            config::save_notes,
            config::load_keybindings,
            config::save_keybindings,
            config::load_agents_config,
            config::save_agents_config,
            agent_mcp::get_agent_mcp_status,
            agent_mcp::install_agent_mcp,
            agent_mcp::remove_agent_mcp,
            agent_mcp::get_agent_config_path,
            prompt::extract_prompt_variables,
            prompt::process_prompt_content,
            head_watcher::start_head_watcher,
            head_watcher::stop_head_watcher,
            repo_watcher::start_repo_watcher,
            repo_watcher::stop_repo_watcher,
            sleep_prevention::block_sleep,
            sleep_prevention::unblock_sleep,
            fs::resolve_terminal_path,
            fs::list_directory,
            fs::fs_read_file,
            fs::write_file,
            fs::create_directory,
            fs::delete_path,
            fs::rename_path,
            fs::copy_path,
            fs::add_to_gitignore,
            plugins::list_user_plugins,
            plugins::read_plugin_data,
            plugins::write_plugin_data,
            plugins::delete_plugin_data,
            plugins::install_plugin_from_zip,
            plugins::install_plugin_from_url,
            plugins::uninstall_plugin,
            plugin_fs::plugin_read_file,
            plugin_fs::plugin_list_directory,
            plugin_fs::plugin_read_file_tail,
            plugin_fs::plugin_watch_path,
            plugin_fs::plugin_unwatch,
            plugin_http::plugin_http_fetch,
            plugin_credentials::plugin_read_credential,
            registry::fetch_plugin_registry
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            // Guard against corrupted window-state applied by tauri-plugin-window-state.
            // Must run at Ready (after plugins have restored persisted position/size),
            // not in setup() which fires before the plugin applies its state.
            if let tauri::RunEvent::Ready = event
                && let Some(window) = app_handle.get_webview_window("main")
            {
                ensure_window_visible(&window);
            }
        });
}

