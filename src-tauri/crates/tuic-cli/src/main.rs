//! `tuic` — CLI companion for TUICommander.
//!
//! Editor opener (like `code`/`zed`), session multiplexer (like `tmux`),
//! and agent orchestrator. Communicates with a running TUICommander
//! instance via IPC (Unix socket / Windows named pipe).
//!
//! When invoked as `tmux` (via symlink), enters tmux-compatibility mode
//! and translates tmux commands to TUIC equivalents.

mod ipc;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "tuic",
    version,
    about = "TUICommander CLI — editor, multiplexer, orchestrator"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Open a file or directory (default action when no subcommand given)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    paths: Vec<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Open a file or directory in TUICommander
    Open {
        /// Path to open (file or directory)
        path: Option<String>,
        /// Wait until the file is closed (for $EDITOR use)
        #[arg(short, long)]
        wait: bool,
        /// Open at specific line number
        #[arg(short = 'g', long = "goto")]
        goto: Option<String>,
    },
    /// Show a diff between two files
    Diff {
        /// First file
        file_a: String,
        /// Second file
        file_b: String,
    },
    /// List sessions
    #[command(alias = "list-sessions")]
    Ls,
    /// Create a new terminal session
    #[command(alias = "new-session", alias = "new-window")]
    New {
        /// Session name
        #[arg(short, long)]
        name: Option<String>,
        /// Repository path
        repo: Option<String>,
    },
    /// Send input to a session
    #[command(alias = "send-keys")]
    Send {
        /// Session ID or name
        target: String,
        /// Keys/text to send
        keys: Vec<String>,
    },
    /// Capture session output
    #[command(alias = "capture-pane")]
    Capture {
        /// Session ID or name
        target: String,
        /// Output format: raw, text, log
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Kill a session
    #[command(alias = "kill-session")]
    Kill {
        /// Session ID or name
        target: String,
    },
    /// Resize a session
    #[command(alias = "resize-pane")]
    Resize {
        /// Session ID or name
        target: String,
        /// Size as WIDTHxHEIGHT (e.g. 120x40)
        size: String,
    },
    /// Spawn an AI agent
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Show TUICommander status
    Status,
    /// Install the tuic CLI to system PATH
    InstallCli {
        /// Target path (default: /usr/local/bin/tuic on Unix,
        /// %LOCALAPPDATA%\Microsoft\WindowsApps\tuic.exe on Windows)
        #[arg(long)]
        path: Option<String>,
    },
    /// Create tmux compatibility symlink
    Alias {
        /// Remove the alias instead of creating it
        #[arg(long)]
        remove: bool,
    },
    /// Pause a session (flow control)
    Pause {
        /// Session ID or name
        target: String,
    },
    /// Resume a paused session
    Resume {
        /// Session ID or name
        target: String,
    },
}

#[derive(Subcommand)]
enum AgentAction {
    /// Spawn a new agent
    Spawn {
        /// Agent type (claude, codex, etc.)
        agent_type: String,
        /// Repository path
        repo: Option<String>,
    },
    /// List running agents
    Ls,
    /// Send a message to an agent
    Send {
        /// Agent session ID
        target: String,
        /// Message text
        message: String,
    },
}

fn main() {
    let argv0 = std::env::args()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
        })
        .unwrap_or_default();

    if argv0 == "tmux" {
        return tmux_compat();
    }

    let cli = Cli::parse();

    let result = match cli.command {
        Some(cmd) => dispatch(cmd),
        None if !cli.paths.is_empty() => {
            // Default action: open
            let path = cli.paths.first().cloned();
            dispatch(Command::Open {
                path,
                wait: false,
                goto: None,
            })
        }
        None => {
            // No args — show status
            dispatch(Command::Status)
        }
    };

    if let Err(e) = result {
        eprintln!("tuic: {e}");
        std::process::exit(1);
    }
}

fn dispatch(cmd: Command) -> Result<(), String> {
    match cmd {
        Command::Open { path, wait, goto } => cmd_open(path, wait, goto),
        Command::Diff { file_a, file_b } => cmd_diff(&file_a, &file_b),
        Command::Ls => cmd_ls(),
        Command::New { name, repo } => cmd_new(name.as_deref(), repo.as_deref()),
        Command::Send { target, keys } => cmd_send(&target, &keys),
        Command::Capture { target, format } => cmd_capture(&target, &format),
        Command::Kill { target } => cmd_kill(&target),
        Command::Resize { target, size } => cmd_resize(&target, &size),
        Command::Agent { action } => cmd_agent(action),
        Command::Status => cmd_status(),
        Command::InstallCli { path } => cmd_install_cli(path.as_deref()),
        Command::Alias { remove } => cmd_alias(remove),
        Command::Pause { target } => cmd_pause(&target),
        Command::Resume { target } => cmd_resume(&target),
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

fn cmd_open(path: Option<String>, _wait: bool, goto: Option<String>) -> Result<(), String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;

    let resolved = match &path {
        Some(p) => resolve_path(p),
        None => std::env::current_dir()
            .map(|d| d.to_string_lossy().to_string())
            .map_err(|e| format!("Cannot get current directory: {e}"))?,
    };

    // Parse goto (file:line:col or --goto flag)
    let (file_path, line, col) = if let Some(g) = &goto {
        parse_goto(g)
    } else {
        parse_goto(&resolved)
    };

    let actual_path = if goto.is_some() {
        &resolved
    } else {
        &file_path
    };

    // Check if path is a directory → open as repo, file → open in editor
    let metadata = std::fs::metadata(actual_path);
    if metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
        // Open repo
        let body = serde_json::json!({ "path": actual_path });
        let resp = ipc::post(
            "/sessions",
            &serde_json::json!({
                "repo_path": actual_path,
                "rows": 24,
                "cols": 80,
            })
            .to_string(),
        )
        .map_err(|e| e.to_string())?;

        if resp.is_success() {
            eprintln!("Opened {actual_path}");
        } else {
            // Maybe it's already a known repo — try activating it via deep link
            let _ = open_deep_link(&format!("tuic://open-repo?path={}", urlencod(actual_path)));
            eprintln!("Activated {actual_path}");
        }
        let _ = body; // suppress unused warning
    } else {
        // Open file in editor via deep link
        let mut url = format!("tuic://edit/{}", urlencod(actual_path));
        if let Some(l) = line {
            url.push_str(&format!("?line={l}"));
            if let Some(c) = col {
                url.push_str(&format!("&col={c}"));
            }
        }
        open_deep_link(&url).map_err(|e| e.to_string())?;
    }

    // TODO: --wait support via polling session state
    Ok(())
}

fn cmd_diff(file_a: &str, file_b: &str) -> Result<(), String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;
    let a = resolve_path(file_a);
    let b = resolve_path(file_b);
    open_deep_link(&format!(
        "tuic://diff?a={}&b={}",
        urlencod(&a),
        urlencod(&b)
    ))
    .map_err(|e| e.to_string())
}

fn cmd_ls() -> Result<(), String> {
    let resp = ipc::get("/sessions").map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err(format!("Server error: {}", resp.status));
    }

    let sessions: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let arr = sessions.as_array().unwrap_or(&Vec::new()).clone();

    if arr.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }

    // Header
    println!("{:<38} {:<20} {:<10} {}", "ID", "NAME", "STATUS", "REPO");
    println!("{}", "-".repeat(90));

    for s in &arr {
        let id = s["id"].as_str().unwrap_or("-");
        let name = s["name"].as_str().unwrap_or("-");
        let status = if s["paused"].as_bool().unwrap_or(false) {
            "paused"
        } else {
            "running"
        };
        let repo = s["repo_path"].as_str().unwrap_or("-");
        // Shorten repo to last 2 components
        let short_repo = repo
            .rsplit('/')
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/");
        println!("{:<38} {:<20} {:<10} {}", id, name, status, short_repo);
    }

    Ok(())
}

fn cmd_new(name: Option<&str>, repo: Option<&str>) -> Result<(), String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;

    let repo_path = match repo {
        Some(r) => resolve_path(r),
        None => std::env::current_dir()
            .map(|d| d.to_string_lossy().to_string())
            .map_err(|e| format!("Cannot get cwd: {e}"))?,
    };

    let mut body = serde_json::json!({
        "repo_path": repo_path,
        "rows": 24,
        "cols": 80,
    });

    if let Some(n) = name {
        body["name"] = serde_json::Value::String(n.to_string());
    }

    let resp = ipc::post("/sessions", &body.to_string()).map_err(|e| e.to_string())?;

    if resp.is_success() {
        if let Ok(v) = resp.json() {
            let id = v["id"].as_str().unwrap_or("?");
            let name_display = name.unwrap_or(id);
            println!("{name_display}: {id}");
        }
    } else {
        return Err(format!("Failed to create session: {}", resp.body));
    }

    Ok(())
}

fn cmd_send(target: &str, keys: &[String]) -> Result<(), String> {
    let id = resolve_session_id(target)?;
    let text = keys.join(" ");

    // Translate tmux-style key names
    let translated = translate_keys(&text);

    let body = serde_json::json!({ "data": translated });
    let resp = ipc::post(&format!("/sessions/{id}/write"), &body.to_string())
        .map_err(|e| e.to_string())?;

    if !resp.is_success() {
        return Err(format!("Failed to send keys: {}", resp.body));
    }

    Ok(())
}

fn cmd_capture(target: &str, format: &str) -> Result<(), String> {
    let id = resolve_session_id(target)?;
    let fmt_param = match format {
        "raw" => "",
        "log" => "?format=log",
        _ => "?format=text",
    };

    let resp = ipc::get(&format!("/sessions/{id}/output{fmt_param}")).map_err(|e| e.to_string())?;

    if resp.is_success() {
        // Output might be JSON with a "data" field or plain text
        if let Ok(v) = resp.json() {
            if let Some(data) = v["data"].as_str() {
                print!("{data}");
            } else if let Some(lines) = v["lines"].as_array() {
                for line in lines {
                    if let Some(text) = line["text"].as_str() {
                        println!("{text}");
                    }
                }
            } else {
                print!("{}", resp.body);
            }
        } else {
            print!("{}", resp.body);
        }
    } else {
        return Err(format!("Failed to capture: {}", resp.body));
    }

    Ok(())
}

fn cmd_kill(target: &str) -> Result<(), String> {
    let id = resolve_session_id(target)?;
    let resp = ipc::delete(&format!("/sessions/{id}")).map_err(|e| e.to_string())?;

    if resp.is_success() {
        eprintln!("Killed session {id}");
    } else {
        return Err(format!("Failed to kill session: {}", resp.body));
    }

    Ok(())
}

fn cmd_resize(target: &str, size: &str) -> Result<(), String> {
    let id = resolve_session_id(target)?;
    let parts: Vec<&str> = size.split('x').collect();
    if parts.len() != 2 {
        return Err("Size must be WIDTHxHEIGHT (e.g. 120x40)".to_string());
    }
    let cols: u16 = parts[0].parse().map_err(|_| "Invalid width")?;
    let rows: u16 = parts[1].parse().map_err(|_| "Invalid height")?;

    let body = serde_json::json!({ "rows": rows, "cols": cols });
    let resp = ipc::post(&format!("/sessions/{id}/resize"), &body.to_string())
        .map_err(|e| e.to_string())?;

    if !resp.is_success() {
        return Err(format!("Failed to resize: {}", resp.body));
    }

    Ok(())
}

fn cmd_agent(action: AgentAction) -> Result<(), String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;

    match action {
        AgentAction::Spawn { agent_type, repo } => {
            let repo_path = match repo {
                Some(r) => resolve_path(&r),
                None => std::env::current_dir()
                    .map(|d| d.to_string_lossy().to_string())
                    .map_err(|e| format!("Cannot get cwd: {e}"))?,
            };

            let body = serde_json::json!({
                "agent_type": agent_type,
                "repo_path": repo_path,
            });
            let resp =
                ipc::post("/sessions/agent", &body.to_string()).map_err(|e| e.to_string())?;

            if resp.is_success() {
                if let Ok(v) = resp.json() {
                    let id = v["id"].as_str().unwrap_or("?");
                    println!("Spawned {agent_type} agent: {id}");
                }
            } else {
                return Err(format!("Failed to spawn agent: {}", resp.body));
            }
        }
        AgentAction::Ls => {
            let resp = ipc::get("/sessions").map_err(|e| e.to_string())?;
            if !resp.is_success() {
                return Err(format!("Server error: {}", resp.status));
            }
            let sessions: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
            let arr = sessions.as_array().unwrap_or(&Vec::new()).clone();
            let agents: Vec<_> = arr
                .iter()
                .filter(|s| s["agent_type"].as_str().is_some())
                .collect();

            if agents.is_empty() {
                println!("No active agents.");
                return Ok(());
            }

            println!("{:<38} {:<12} {:<10} {}", "ID", "TYPE", "STATUS", "REPO");
            println!("{}", "-".repeat(82));

            for s in &agents {
                let id = s["id"].as_str().unwrap_or("-");
                let agent_type = s["agent_type"].as_str().unwrap_or("-");
                let status = if s["paused"].as_bool().unwrap_or(false) {
                    "paused"
                } else {
                    "running"
                };
                let repo = s["repo_path"]
                    .as_str()
                    .unwrap_or("-")
                    .rsplit('/')
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("/");
                println!("{:<38} {:<12} {:<10} {}", id, agent_type, status, repo);
            }
        }
        AgentAction::Send { target, message } => {
            let id = resolve_session_id(&target)?;
            let body = serde_json::json!({ "data": format!("{message}\r") });
            let resp = ipc::post(&format!("/sessions/{id}/write"), &body.to_string())
                .map_err(|e| e.to_string())?;

            if !resp.is_success() {
                return Err(format!("Failed to send: {}", resp.body));
            }
        }
    }

    Ok(())
}

fn cmd_status() -> Result<(), String> {
    let resp = ipc::get("/health").map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err("TUICommander is not responding".to_string());
    }

    let version_resp = ipc::get("/api/version").map_err(|e| e.to_string())?;
    let version = version_resp
        .json()
        .ok()
        .and_then(|v| v["version"].as_str().map(String::from))
        .unwrap_or_else(|| "unknown".to_string());

    let sessions_resp = ipc::get("/sessions").map_err(|e| e.to_string())?;
    let session_count = sessions_resp
        .json()
        .ok()
        .and_then(|v| v.as_array().map(|a| a.len()))
        .unwrap_or(0);

    println!("TUICommander v{version}");
    println!("Status: running");
    println!("Sessions: {session_count}");

    Ok(())
}

fn cmd_install_cli(target: Option<&str>) -> Result<(), String> {
    let default_path = if cfg!(target_os = "windows") {
        // %LOCALAPPDATA%\Microsoft\WindowsApps is user-writable and already in
        // PATH on modern Windows — matches the GUI installer (tuic_cli.rs).
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        format!("{local_app_data}\\Microsoft\\WindowsApps\\tuic.exe")
    } else {
        "/usr/local/bin/tuic".to_string()
    };

    let target_path = target.unwrap_or(&default_path);
    let self_exe =
        std::env::current_exe().map_err(|e| format!("Cannot find own executable: {e}"))?;

    // Check if target already exists and points to us
    if let Ok(existing) = std::fs::read_link(target_path) {
        if existing == self_exe {
            println!("Already installed at {target_path}");
            return Ok(());
        }
    }

    // Try direct copy/symlink first, fall back to sudo on Unix
    #[cfg(unix)]
    {
        // Try symlink first
        if std::os::unix::fs::symlink(&self_exe, target_path).is_ok() {
            println!("Installed {target_path} -> {}", self_exe.display());
            return Ok(());
        }

        // Needs elevation — use osascript on macOS, sudo on Linux
        let parent = std::path::Path::new(target_path)
            .parent()
            .unwrap_or(std::path::Path::new("/usr/local/bin"));

        #[cfg(target_os = "macos")]
        {
            let script = format!(
                "do shell script \"mkdir -p '{}' && ln -sf '{}' '{}'\" with administrator privileges",
                parent.display(),
                self_exe.display(),
                target_path
            );
            let status = std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .status()
                .map_err(|e| format!("Failed to run osascript: {e}"))?;
            if !status.success() {
                return Err("Installation cancelled".to_string());
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let status = std::process::Command::new("sudo")
                .args(["ln", "-sf"])
                .arg(self_exe.to_str().unwrap_or(""))
                .arg(target_path)
                .status()
                .map_err(|e| format!("Failed to run sudo: {e}"))?;
            if !status.success() {
                return Err("Installation cancelled".to_string());
            }
        }

        println!("Installed {target_path} -> {}", self_exe.display());
    }

    #[cfg(windows)]
    {
        // Create the target directory if it doesn't exist (avoids OS error 3).
        if let Some(parent) = std::path::Path::new(target_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&self_exe, target_path).map_err(|e| format!("Failed to copy: {e}"))?;
        println!("Installed {target_path}");
    }

    Ok(())
}

fn cmd_alias(remove: bool) -> Result<(), String> {
    let self_exe =
        std::env::current_exe().map_err(|e| format!("Cannot find own executable: {e}"))?;

    let tmux_path = if cfg!(target_os = "windows") {
        // Same user-writable, in-PATH location as the tuic install (see cmd_install_cli).
        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
        format!("{local_app_data}\\Microsoft\\WindowsApps\\tmux.exe")
    } else {
        "/usr/local/bin/tmux".to_string()
    };

    if remove {
        // Only remove if it's our symlink
        #[cfg(unix)]
        {
            if let Ok(target) = std::fs::read_link(&tmux_path) {
                if target == self_exe
                    || target
                        .file_name()
                        .map(|f| f.to_string_lossy().contains("tuic"))
                        .unwrap_or(false)
                {
                    remove_with_elevation(&tmux_path)?;
                    println!("Removed tmux alias at {tmux_path}");
                } else {
                    return Err(format!(
                        "{tmux_path} exists but points to {}, not tuic — refusing to remove",
                        target.display()
                    ));
                }
            } else {
                println!("No tmux alias found at {tmux_path}");
            }
        }
        #[cfg(windows)]
        {
            let _ = std::fs::remove_file(&tmux_path);
            println!("Removed tmux alias at {tmux_path}");
        }
        return Ok(());
    }

    // Check if real tmux exists
    let has_real_tmux = std::process::Command::new("which")
        .arg("tmux")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if has_real_tmux {
        // Check if it's already our symlink
        #[cfg(unix)]
        if let Ok(target) = std::fs::read_link(&tmux_path) {
            if target == self_exe {
                println!("tmux alias already installed at {tmux_path}");
                return Ok(());
            }
        }

        eprintln!("Warning: real tmux is installed. The alias will shadow it.");
        eprintln!("Use `tuic alias --remove` to restore the original tmux.");
    }

    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(&self_exe, &tmux_path).is_ok() {
            println!("Created tmux -> tuic alias at {tmux_path}");
            return Ok(());
        }

        // Needs elevation
        #[cfg(target_os = "macos")]
        {
            let script = format!(
                "do shell script \"ln -sf '{}' '{}'\" with administrator privileges",
                self_exe.display(),
                tmux_path
            );
            let status = std::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .status()
                .map_err(|e| format!("Failed to run osascript: {e}"))?;
            if !status.success() {
                return Err("Alias creation cancelled".to_string());
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let status = std::process::Command::new("sudo")
                .args(["ln", "-sf"])
                .arg(self_exe.to_str().unwrap_or(""))
                .arg(&tmux_path)
                .status()
                .map_err(|e| format!("Failed to run sudo: {e}"))?;
            if !status.success() {
                return Err("Alias creation cancelled".to_string());
            }
        }

        println!("Created tmux -> tuic alias at {tmux_path}");
    }

    #[cfg(windows)]
    {
        // Create the target directory if it doesn't exist (avoids OS error 3).
        if let Some(parent) = std::path::Path::new(&tmux_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        std::fs::copy(&self_exe, &tmux_path).map_err(|e| format!("Failed to copy: {e}"))?;
        println!("Created tmux alias at {tmux_path}");
    }

    Ok(())
}

fn cmd_pause(target: &str) -> Result<(), String> {
    let id = resolve_session_id(target)?;
    let resp = ipc::post(&format!("/sessions/{id}/pause"), "{}").map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err(format!("Failed to pause: {}", resp.body));
    }
    Ok(())
}

fn cmd_resume(target: &str) -> Result<(), String> {
    let id = resolve_session_id(target)?;
    let resp = ipc::post(&format!("/sessions/{id}/resume"), "{}").map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err(format!("Failed to resume: {}", resp.body));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// tmux compatibility mode
// ---------------------------------------------------------------------------

fn tmux_compat() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        // bare `tmux` → new session in cwd
        if let Err(e) = dispatch(Command::New {
            name: None,
            repo: None,
        }) {
            eprintln!("tmux: {e}");
            std::process::exit(1);
        }
        return;
    }

    let subcmd = args[0].as_str();
    let rest = &args[1..];

    let result = match subcmd {
        "new-session" | "new" => {
            let name = find_flag(rest, "-s").or_else(|| find_flag(rest, "-n"));
            dispatch(Command::New { name, repo: None })
        }
        "list-sessions" | "ls" => dispatch(Command::Ls),
        "kill-session" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            dispatch(Command::Kill { target })
        }
        "kill-server" => {
            // Kill all sessions
            if let Ok(resp) = ipc::get("/sessions") {
                if let Ok(v) = resp.json() {
                    if let Some(arr) = v.as_array() {
                        for s in arr {
                            if let Some(id) = s["id"].as_str() {
                                let _ = ipc::delete(&format!("/sessions/{id}"));
                            }
                        }
                    }
                }
            }
            Ok(())
        }
        "send-keys" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            let keys: Vec<String> = rest
                .iter()
                .filter(|a| *a != "-t" && find_flag(rest, "-t").as_deref() != Some(a.as_str()))
                .cloned()
                .collect();
            dispatch(Command::Send { target, keys })
        }
        "capture-pane" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            dispatch(Command::Capture {
                target,
                format: "text".to_string(),
            })
        }
        "resize-pane" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            let x = find_flag(rest, "-x").unwrap_or("80".to_string());
            let y = find_flag(rest, "-y").unwrap_or("24".to_string());
            dispatch(Command::Resize {
                target,
                size: format!("{x}x{y}"),
            })
        }
        "attach-session" | "attach" | "a" => {
            // Focus TUICommander window
            let _ = open_deep_link("tuic://focus");
            Ok(())
        }
        "has-session" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            match resolve_session_id(&target) {
                Ok(_) => std::process::exit(0),
                Err(_) => std::process::exit(1),
            }
        }
        "display-message" => {
            // tmux display-message -p "#{session_name}" etc.
            // Return session info
            dispatch(Command::Status)
        }
        _ => {
            eprintln!("tmux (tuic compat): unknown command '{subcmd}'");
            eprintln!("Supported: new-session, list-sessions, kill-session, kill-server,");
            eprintln!("           send-keys, capture-pane, resize-pane, attach-session,");
            eprintln!("           has-session, display-message");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("tmux: {e}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn resolve_path(path: &str) -> String {
    if path.starts_with('/') || path.starts_with('\\') {
        return path.to_string();
    }
    #[cfg(windows)]
    if path.len() >= 2 && path.as_bytes()[1] == b':' {
        return path.to_string();
    }
    if let Ok(cwd) = std::env::current_dir() {
        cwd.join(path).to_string_lossy().to_string()
    } else {
        path.to_string()
    }
}

fn parse_goto(path: &str) -> (String, Option<u32>, Option<u32>) {
    // Parse file:line:col or file:line
    let parts: Vec<&str> = path.rsplitn(3, ':').collect();
    match parts.len() {
        3 => {
            if let (Ok(line), Ok(col)) = (parts[1].parse::<u32>(), parts[0].parse::<u32>()) {
                return (parts[2].to_string(), Some(line), Some(col));
            }
        }
        2 => {
            if let Ok(line) = parts[0].parse::<u32>() {
                return (parts[1].to_string(), Some(line), None);
            }
        }
        _ => {}
    }
    (path.to_string(), None, None)
}

fn resolve_session_id(target: &str) -> Result<String, String> {
    // If it looks like a UUID, use directly
    if target.len() >= 32 && target.contains('-') {
        return Ok(target.to_string());
    }

    // Otherwise search by name
    let resp = ipc::get("/sessions").map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err("Cannot list sessions".to_string());
    }

    let sessions: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    let arr = sessions.as_array().ok_or("Invalid response")?;

    // Try exact name match
    for s in arr {
        if s["name"].as_str() == Some(target) {
            return s["id"]
                .as_str()
                .map(String::from)
                .ok_or("Session has no ID".to_string());
        }
    }

    // Try prefix match on ID
    let matches: Vec<_> = arr
        .iter()
        .filter(|s| {
            s["id"]
                .as_str()
                .map(|id| id.starts_with(target))
                .unwrap_or(false)
        })
        .collect();

    match matches.len() {
        0 => Err(format!("No session found matching '{target}'")),
        1 => matches[0]["id"]
            .as_str()
            .map(String::from)
            .ok_or("Session has no ID".to_string()),
        n => Err(format!(
            "Ambiguous target '{target}': {n} sessions match. Use full ID."
        )),
    }
}

fn translate_keys(text: &str) -> String {
    // Translate tmux key names to actual characters
    text.replace("Enter", "\r")
        .replace("Space", " ")
        .replace("Tab", "\t")
        .replace("Escape", "\x1b")
        .replace("BSpace", "\x7f")
        .replace("C-c", "\x03")
        .replace("C-d", "\x04")
        .replace("C-z", "\x1a")
        .replace("C-l", "\x0c")
        .replace("C-a", "\x01")
        .replace("C-e", "\x05")
        .replace("C-k", "\x0b")
        .replace("C-u", "\x15")
}

fn find_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn urlencod(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('&', "%26")
        .replace('=', "%3D")
}

fn open_deep_link(url: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(url).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(url).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/c", "start", "", url])
            .spawn()?;
    }
    Ok(())
}

#[cfg(unix)]
fn remove_with_elevation(path: &str) -> Result<(), String> {
    if std::fs::remove_file(path).is_ok() {
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        let script = format!("do shell script \"rm -f '{path}'\" with administrator privileges");
        let status = std::process::Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .status()
            .map_err(|e| format!("Failed to run osascript: {e}"))?;
        if !status.success() {
            return Err("Removal cancelled".to_string());
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let status = std::process::Command::new("sudo")
            .args(["rm", "-f", path])
            .status()
            .map_err(|e| format!("Failed to run sudo: {e}"))?;
        if !status.success() {
            return Err("Removal cancelled".to_string());
        }
    }

    Ok(())
}
