use parking_lot::Mutex;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::Serialize;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, State};
use uuid::Uuid;

use crate::pty::spawn_reader_thread;
use crate::state::{
    AgentConfig, AppState, OutputRingBuffer, PtyConfig, PtySession, OUTPUT_RING_BUFFER_CAPACITY,
};

/// Check if a CLI tool exists on PATH
fn has_cli(name: &str) -> bool {
    let checker = if cfg!(target_os = "windows") { "where" } else { "which" };
    Command::new(checker).arg(name).output()
        .map(|o| o.status.success()).unwrap_or(false)
}

/// Open a path in an IDE or application
#[tauri::command]
pub(crate) fn open_in_app(path: String, app: String) -> Result<(), String> {
    let mut cmd = match app.as_str() {
        // CLI-based editors (cross-platform)
        "vscode" => { let mut c = Command::new("code"); c.arg(&path); c }
        "cursor" => { let mut c = Command::new("cursor"); c.arg(&path); c }
        "zed" => { let mut c = Command::new("zed"); c.arg(&path); c }
        "windsurf" => { let mut c = Command::new("windsurf"); c.arg(&path); c }
        "neovim" => { let mut c = Command::new("nvim"); c.arg(&path); c }
        "smerge" => { let mut c = Command::new("smerge"); c.arg(&path); c }

        // Terminal emulators with CLI (cross-platform)
        "kitty" => { let mut c = Command::new("kitty"); c.arg("--directory").arg(&path); c }
        "wezterm" if has_cli("wezterm") => {
            let mut c = Command::new("wezterm");
            c.arg("start").arg("--cwd").arg(&path);
            c
        }
        "alacritty" if has_cli("alacritty") => {
            let mut c = Command::new("alacritty");
            c.arg("--working-directory").arg(&path);
            c
        }

        // macOS .app bundles (use 'open -a')
        app_name if cfg!(target_os = "macos") => {
            match app_name {
                "xcode" => { let mut c = Command::new("open"); c.arg("-a").arg("Xcode").arg(&path); c }
                "sourcetree" => { let mut c = Command::new("open"); c.arg("-a").arg("Sourcetree").arg(&path); c }
                "github-desktop" => { let mut c = Command::new("open"); c.arg("-a").arg("GitHub Desktop").arg(&path); c }
                "fork" => { let mut c = Command::new("open"); c.arg("-a").arg("Fork").arg(&path); c }
                "gitkraken" => { let mut c = Command::new("open"); c.arg("-a").arg("GitKraken").arg(&path); c }
                "ghostty" => { let mut c = Command::new("open"); c.arg("-a").arg("Ghostty").arg(&path); c }
                "wezterm" => { let mut c = Command::new("open"); c.arg("-a").arg("WezTerm").arg(&path); c }
                "alacritty" => { let mut c = Command::new("open"); c.arg("-a").arg("Alacritty").arg(&path); c }
                "warp" => { let mut c = Command::new("open"); c.arg("-a").arg("Warp").arg(&path); c }
                "terminal" => { let mut c = Command::new("open"); c.arg("-a").arg("Terminal").arg(&path); c }
                "finder" => { let mut c = Command::new("open"); c.arg(&path); c }
                "editor" => {
                    if let Ok(editor) = std::env::var("EDITOR") {
                        let mut c = Command::new(&editor);
                        c.arg(&path);
                        c
                    } else {
                        return Err("$EDITOR not set".to_string());
                    }
                }
                _ => return Err(format!("Unknown app: {app_name}")),
            }
        }

        // Linux: system terminal + file manager
        #[cfg(target_os = "linux")]
        "terminal" => {
            // Try common terminals in order
            let terminals = ["ghostty", "wezterm", "alacritty", "kitty", "gnome-terminal", "konsole", "xterm"];
            if let Some(term) = terminals.iter().find(|t| has_cli(t)) {
                let mut c = Command::new(term);
                c.arg(&path);
                c
            } else {
                return Err("No terminal emulator found".to_string());
            }
        }
        #[cfg(target_os = "linux")]
        "finder" => { let mut c = Command::new("xdg-open"); c.arg(&path); c }

        // Windows: system terminal + file manager
        #[cfg(target_os = "windows")]
        "terminal" => { let mut c = Command::new("cmd"); c.args(["/c", "start", "cmd", "/k", "cd", "/d", &path]); c }
        #[cfg(target_os = "windows")]
        "finder" => { let mut c = Command::new("explorer"); c.arg(&path); c }

        _ => return Err(format!("Unknown app: {app}")),
    };

    cmd.spawn()
        .map_err(|e| format!("Failed to open in {app}: {e}"))?;

    Ok(())
}

/// Detect installed IDE applications (cross-platform)
#[tauri::command]
pub(crate) fn detect_installed_ides() -> Vec<String> {
    let mut installed = Vec::new();

    // CLI-detectable tools (cross-platform via which/where)
    let cli_tools: &[(&str, &str)] = &[
        ("vscode", "code"),
        ("cursor", "cursor"),
        ("zed", "zed"),
        ("windsurf", "windsurf"),
        ("neovim", "nvim"),
        ("smerge", "smerge"),
        ("kitty", "kitty"),
    ];
    for (id, bin) in cli_tools {
        if has_cli(bin) {
            installed.push(id.to_string());
        }
    }

    // macOS: .app bundle detection
    #[cfg(target_os = "macos")]
    {
        let app_bundles: &[(&str, &str)] = &[
            ("xcode", "/Applications/Xcode.app"),
            ("sourcetree", "/Applications/Sourcetree.app"),
            ("github-desktop", "/Applications/GitHub Desktop.app"),
            ("fork", "/Applications/Fork.app"),
            ("gitkraken", "/Applications/GitKraken.app"),
            ("ghostty", "/Applications/Ghostty.app"),
            ("wezterm", "/Applications/WezTerm.app"),
            ("alacritty", "/Applications/Alacritty.app"),
            ("warp", "/Applications/Warp.app"),
        ];
        for (id, path) in app_bundles {
            if std::path::Path::new(path).exists() && !installed.contains(&id.to_string()) {
                installed.push(id.to_string());
            }
        }
    }

    // Linux: additional CLI detection for apps without separate CLI
    #[cfg(target_os = "linux")]
    {
        let linux_tools: &[(&str, &str)] = &[
            ("ghostty", "ghostty"),
            ("wezterm", "wezterm"),
            ("alacritty", "alacritty"),
        ];
        for (id, bin) in linux_tools {
            if has_cli(bin) && !installed.contains(&id.to_string()) {
                installed.push(id.to_string());
            }
        }
    }

    // $EDITOR support
    if let Ok(editor) = std::env::var("EDITOR")
        && !editor.is_empty() {
            installed.push("editor".to_string());
        }

    // System utilities (always available)
    installed.push("terminal".to_string());
    installed.push("finder".to_string());

    installed
}

/// Agent binary detection result
#[derive(Clone, Serialize)]
pub(crate) struct AgentBinaryDetection {
    pub(crate) path: Option<String>,
    pub(crate) version: Option<String>,
}

/// Detect any agent binary location
#[tauri::command]
pub(crate) fn detect_agent_binary(binary: String) -> AgentBinaryDetection {
    let home = dirs::home_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Platform-specific candidate paths
    #[cfg(not(windows))]
    let candidates = vec![
        format!("{}/.local/bin/{}", home, binary),
        format!("/usr/local/bin/{}", binary),
        format!("/opt/homebrew/bin/{}", binary),
        format!("{}/.npm-global/bin/{}", home, binary),
        format!("{}/.cargo/bin/{}", home, binary),
        format!("{}/go/bin/{}", home, binary),
        format!("{}/.pyenv/shims/{}", home, binary),
    ];

    #[cfg(windows)]
    let candidates = vec![
        format!("{}\\.cargo\\bin\\{}.exe", home, binary),
        format!("{}\\go\\bin\\{}.exe", home, binary),
        format!("{}\\AppData\\Local\\Programs\\{}\\{}.exe", home, binary, binary),
        format!("{}\\AppData\\Local\\Microsoft\\WinGet\\Packages\\**\\{}.exe", home, binary),
        format!("{}\\scoop\\shims\\{}.exe", home, binary),
        format!("C:\\Program Files\\{}\\{}.exe", binary, binary),
    ];

    // Use platform-appropriate PATH lookup (which on Unix, where on Windows)
    let checker = if cfg!(target_os = "windows") { "where" } else { "which" };
    if let Ok(output) = Command::new(checker).arg(&binary).output()
        && output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            // `where` on Windows may return multiple lines; take the first
            let path = path.lines().next().unwrap_or("").to_string();
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                let version = get_binary_version(&path);
                return AgentBinaryDetection {
                    path: Some(path),
                    version,
                };
            }
        }

    // Fall back to known locations
    for candidate in &candidates {
        if !candidate.is_empty() && std::path::Path::new(candidate).exists() {
            let version = get_binary_version(candidate);
            return AgentBinaryDetection {
                path: Some(candidate.clone()),
                version,
            };
        }
    }

    AgentBinaryDetection {
        path: None,
        version: None,
    }
}

/// Get version of a binary (try --version or -v)
fn get_binary_version(path: &str) -> Option<String> {
    // Try --version first
    if let Ok(output) = Command::new(path).arg("--version").output()
        && output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                return Some(first_line.to_string());
            }
        }
    // Try -v
    if let Ok(output) = Command::new(path).arg("-v").output()
        && output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() {
                return Some(first_line.to_string());
            }
        }
    None
}

/// Detect claude binary location (legacy, delegates to detect_agent_binary)
#[tauri::command]
pub(crate) fn detect_claude_binary() -> Result<String, String> {
    let detection = detect_agent_binary("claude".to_string());
    detection.path.ok_or_else(|| {
        "Claude binary not found. Install with: npm install -g @anthropic-ai/claude-code".to_string()
    })
}

/// Spawn an agent in a PTY
#[tauri::command]
pub(crate) async fn spawn_agent(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    pty_config: PtyConfig,
    agent_config: AgentConfig,
) -> Result<String, String> {
    // Determine binary path - use provided path, detect by type, or fall back to claude
    let binary_path = if let Some(ref path) = agent_config.binary_path {
        path.clone()
    } else if let Some(ref agent_type) = agent_config.agent_type {
        let detection = detect_agent_binary(agent_type.clone());
        detection.path.ok_or_else(|| {
            format!("Agent binary '{agent_type}' not found")
        })?
    } else {
        detect_claude_binary()?
    };

    let session_id = Uuid::new_v4().to_string();
    let pty_system = native_pty_system();

    let pair = pty_system
        .openpty(PtySize {
            rows: pty_config.rows,
            cols: pty_config.cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|e| format!("Failed to open PTY: {e}"))?;

    // Build agent command
    let mut cmd = CommandBuilder::new(&binary_path);

    // If custom args are provided, use them directly
    if let Some(ref args) = agent_config.args {
        for arg in args {
            cmd.arg(arg);
        }
    } else {
        // Default Claude-style args for backward compatibility
        if agent_config.print_mode {
            cmd.arg("--print");
        }

        if let Some(ref format) = agent_config.output_format {
            cmd.arg("--output-format");
            cmd.arg(format);
        }

        if let Some(ref model) = agent_config.model {
            cmd.arg("--model");
            cmd.arg(model);
        }

        // Add prompt
        cmd.arg(&agent_config.prompt);
    }

    // Set working directory
    if let Some(ref cwd) = agent_config.cwd {
        cmd.cwd(cwd);
    } else if let Some(cwd) = pty_config.cwd {
        cmd.cwd(cwd);
    }

    let child = pair
        .slave
        .spawn_command(cmd)
        .map_err(|e| format!("Failed to spawn agent: {e}"))?;

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| format!("Failed to get PTY writer: {e}"))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| format!("Failed to get PTY reader: {e}"))?;

    // Store session (master handle kept for resize support)
    let paused = Arc::new(AtomicBool::new(false));
    state.sessions.insert(
        session_id.clone(),
        Mutex::new(PtySession {
            writer,
            master: pair.master,
            _child: child,
            paused: paused.clone(),
            worktree: None,
            cwd: agent_config.cwd.clone(),
        }),
    );
    state.metrics.total_spawned.fetch_add(1, Ordering::Relaxed);
    state.metrics.active_sessions.fetch_add(1, Ordering::Relaxed);

    // Create ring buffer for this session
    state.output_buffers.insert(
        session_id.clone(),
        Mutex::new(OutputRingBuffer::new(OUTPUT_RING_BUFFER_CAPACITY)),
    );

    spawn_reader_thread(
        reader,
        paused,
        session_id.clone(),
        app,
        state.inner().clone(),
    );

    Ok(session_id)
}

/// Detect if lazygit is installed (Story 048)
#[tauri::command]
pub(crate) fn detect_lazygit_binary() -> AgentBinaryDetection {
    detect_agent_binary("lazygit".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_claude_binary() {
        // This test checks that detect_claude_binary returns a result
        // It may succeed or fail depending on whether claude is installed
        let result = detect_claude_binary();
        // We just verify it doesn't panic and returns a proper Result
        match result {
            Ok(path) => {
                assert!(!path.is_empty());
                assert!(std::path::Path::new(&path).exists());
            }
            Err(msg) => {
                assert!(msg.contains("not found") || msg.contains("Install"));
            }
        }
    }
}
