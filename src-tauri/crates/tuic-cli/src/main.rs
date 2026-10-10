//! `tuic` — CLI companion for TUICommander.
//!
//! Editor opener (like `code`/`zed`), session multiplexer (like `tmux`),
//! and agent orchestrator. Communicates with a running TUICommander
//! instance via IPC (Unix socket / Windows named pipe).
//!
//! When invoked as `tmux` (via symlink), enters tmux-compatibility mode
//! and translates tmux commands to TUIC equivalents.

mod bg;
mod ipc;
mod mcp;

use clap::{Args, Parser, Subcommand};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "tuic",
    version,
    about = "TUICommander CLI — editor, multiplexer, orchestrator"
)]
struct Cli {
    /// Select an isolated application instance (overrides TUIC_APP_INSTANCE)
    #[arg(long, global = true)]
    instance: Option<String>,

    #[command(subcommand)]
    command: Option<Command>,

    /// Open a file or directory (default action when no subcommand given)
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    paths: Vec<String>,
}

#[derive(Subcommand)]
enum Command {
    /// Run a command detached and queue a completion wake to this TUIC session
    Bg {
        /// Append command output here; write the exit code to <log>.exit
        log: String,
        /// Command and arguments after --
        #[arg(required = true, last = true)]
        command: Vec<String>,
    },
    #[command(name = "__bg-runner", hide = true)]
    BgRunner {
        log: String,
        caller: String,
        #[arg(required = true, last = true)]
        command: Vec<String>,
    },
    /// Call a server-owned MCP tool with JSON arguments
    Mcp {
        /// MCP tool name (for example agent or session)
        tool: String,
        /// JSON object, or - to read it from stdin (defaults to {})
        arguments: Option<String>,
    },
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
    Ls {
        /// Print the raw server payload instead of the table (for scripts)
        #[arg(long)]
        json: bool,
    },
    /// Create a new terminal session
    #[command(alias = "new-session", alias = "new-window")]
    New {
        /// Session name
        #[arg(short, long)]
        name: Option<String>,
        /// Repository path
        repo: Option<String>,
    },
    /// Create a session and run a command in it (shells only)
    Run {
        /// Command to run, e.g. `tuic run pnpm dev`
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
        /// Session name
        #[arg(short, long)]
        name: Option<String>,
        /// Repository path (defaults to the current directory)
        #[arg(long)]
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
        /// Only the last N lines
        #[arg(short = 'n', long)]
        lines: Option<usize>,
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
    /// Use server-owned MCP session actions
    Session {
        #[command(subcommand)]
        action: McpSessionAction,
    },
    /// Use server-owned MCP worktree actions
    Repo {
        #[command(subcommand)]
        action: RepoAction,
    },
    /// Call the native story service with a typed JSON action
    Story {
        /// JSON object tagged with action (for example '{"action":"list_plans"}')
        action: String,
        /// Owning project (defaults to the current directory)
        #[arg(long)]
        project: Option<String>,
        /// Live PTY session for a manual claim
        #[arg(long)]
        session_id: Option<String>,
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
    Spawn(Box<AgentSpawnArgs>),
    /// List running agents
    Ls,
    /// Send a message to a registered peer's inbox (peer registry, not the PTY).
    ///
    /// To type a prompt into an agent's terminal instead, use `tuic agent type`.
    Send {
        /// Recipient peer's tuic_session UUID
        target: String,
        /// Message text
        message: String,
        #[arg(long)]
        json: bool,
    },
    /// Type a prompt into an agent's PTY and submit it (no peer routing).
    ///
    /// Unlike `tuic send`, this uses the agent-safe framing: the text and the
    /// Enter go in separate PTY writes, because a raw-mode Ink TUI treats a
    /// combined `text\r` as a prefill and leaves it unsent.
    Type {
        /// Agent session ID or name
        target: String,
        /// Prompt text
        message: String,
    },
    /// Wait for new mailbox entries without polling
    Wait {
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        timeout_ms: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    /// Read the caller's mailbox
    Inbox {
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// List registered peers
    ListPeers {
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show orchestration capacity
    Stats {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Args)]
struct AgentSpawnArgs {
    /// Agent type (claude, codex, etc.)
    agent_type: String,
    /// Initial prompt for the agent (required by the server)
    prompt: String,
    /// Repository path (defaults to the current directory)
    #[arg(long)]
    repo: Option<String>,
    /// Agent display name
    #[arg(long)]
    name: Option<String>,
    /// Model routing value passed to the agent launcher
    #[arg(long)]
    model: Option<String>,
    /// Explicit launcher argument (repeat for multiple arguments)
    #[arg(long, allow_hyphen_values = true)]
    args: Vec<String>,
    /// Working directory (overrides --repo)
    #[arg(long, conflicts_with = "repo")]
    cwd: Option<String>,
    /// Enable print mode for agents that support it
    #[arg(long)]
    print_mode: bool,
    /// PTY description shown by TUICommander
    #[arg(long)]
    pty_description: Option<String>,
    #[arg(long)]
    rows: Option<u16>,
    #[arg(long)]
    cols: Option<u16>,
    #[arg(long)]
    output_format: Option<String>,
    #[arg(long)]
    binary_path: Option<String>,
    /// Print the raw server payload
    #[arg(long)]
    json: bool,
}

#[derive(Subcommand)]
enum McpSessionAction {
    Status {
        target: String,
        #[arg(long)]
        json: bool,
    },
    Wait {
        target: String,
        #[arg(long, default_value = "idle")]
        until: String,
        #[arg(long)]
        timeout_ms: Option<u64>,
        #[arg(long)]
        json: bool,
    },
    Output {
        target: String,
        #[arg(long)]
        since_cursor: Option<u64>,
        #[arg(long)]
        from_line: Option<usize>,
        #[arg(long)]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand)]
enum RepoAction {
    #[command(name = "worktree-list")]
    List {
        path: String,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "worktree-create")]
    Create {
        path: String,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        base_ref: Option<String>,
        #[arg(long)]
        spawn_session: bool,
        #[arg(long)]
        json: bool,
    },
    #[command(name = "worktree-remove")]
    Remove {
        path: String,
        branch: String,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        json: bool,
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
        select_instance(None);
        return tmux_compat();
    }

    let cli = Cli::parse();
    select_instance(cli.instance.as_deref());

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

fn select_instance(instance: Option<&str>) {
    let result = match instance {
        Some(id) => tuic_ipc::app_instance::select_app_instance(Some(id)),
        None => tuic_ipc::app_instance::select_app_instance_from_env(),
    };
    if let Err(error) = result {
        eprintln!("tuic: {error}");
        std::process::exit(1);
    }
}

fn dispatch(cmd: Command) -> Result<(), String> {
    match cmd {
        Command::Open { path, wait, goto } => cmd_open(path, wait, goto),
        Command::Bg { log, command } => bg::launch(&log, &command),
        Command::BgRunner {
            log,
            caller,
            command,
        } => bg::run(&log, &caller, &command),
        Command::Mcp { tool, arguments } => cmd_mcp(&tool, arguments.as_deref()),
        Command::Diff { file_a, file_b } => cmd_diff(&file_a, &file_b),
        Command::Ls { json } => cmd_ls(json),
        Command::New { name, repo } => cmd_new(name.as_deref(), repo.as_deref()).map(|_| ()),
        Command::Run {
            command,
            name,
            repo,
        } => cmd_run(&command, name.as_deref(), repo.as_deref()),
        Command::Send { target, keys } => cmd_send(&target, &keys),
        Command::Capture {
            target,
            format,
            lines,
        } => cmd_capture(&target, &format, lines),
        Command::Kill { target } => cmd_kill(&target),
        Command::Resize { target, size } => cmd_resize(&target, &size),
        Command::Agent { action } => cmd_agent(action),
        Command::Session { action } => cmd_mcp_session(action),
        Command::Repo { action } => cmd_repo(action),
        Command::Story {
            action,
            project,
            session_id,
        } => cmd_story(&action, project.as_deref(), session_id.as_deref()),
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

fn cmd_mcp(tool: &str, arguments: Option<&str>) -> Result<(), String> {
    let mut stdin_text = String::new();
    let text = if arguments == Some("-") {
        std::io::stdin()
            .read_to_string(&mut stdin_text)
            .unwrap_or_else(|e| {
                mcp_usage_error(&format!("Cannot read MCP arguments from stdin: {e}"))
            });
        stdin_text.as_str()
    } else {
        arguments.unwrap_or("{}")
    };
    let parsed: serde_json::Value = serde_json::from_str(text)
        .unwrap_or_else(|e| mcp_usage_error(&format!("MCP arguments must be a JSON object: {e}")));
    if !parsed.is_object() {
        mcp_usage_error("MCP arguments must be a JSON object");
    }
    let result = mcp::McpClient::connect_for_orchestration()?.call_text(tool, parsed)?;
    println!("{result}");
    Ok(())
}

fn mcp_usage_error(message: &str) -> ! {
    eprintln!("tuic: {message}");
    std::process::exit(2)
}

fn cmd_story(action: &str, project: Option<&str>, session_id: Option<&str>) -> Result<(), String> {
    let action: serde_json::Value = serde_json::from_str(action)
        .map_err(|e| format!("Story action must be a JSON object: {e}"))?;
    if !action.is_object() || action["action"].as_str().is_none() {
        return Err("Story action must be a JSON object with an action field".into());
    }
    let project = match project {
        Some(path) => resolve_path(path),
        None => std::env::current_dir()
            .map_err(|e| format!("Cannot get current directory: {e}"))?
            .to_string_lossy()
            .to_string(),
    };
    let body = serde_json::json!({"action": action, "sessionId": session_id});
    let response = ipc::request(
        "POST",
        &format!("/stories/action?path={}", urlencod(&project)),
        Some(&body.to_string()),
    )
    .map_err(|e| format!("Story request failed: {e}"))?;
    if !response.is_success() {
        return Err(format!(
            "Story request failed (HTTP {}): {}",
            response.status, response.body
        ));
    }
    println!("{}", response.body);
    Ok(())
}

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
        // Registration is permanent; a temp directory is not. Refuse rather than
        // leave a row the user must later hunt down (#763-d219).
        if is_disposable_root(Path::new(actual_path), &disposable_roots()) {
            return Err(format!(
                "Refusing to register \"{actual_path}\" as a repository: it is inside a temporary \
                 directory, which the OS deletes while the sidebar entry stays behind forever.\n\
                 For a shell there instead: tuic new {actual_path}"
            ));
        }
        // The selected server owns registration. Dev builds and headless
        // instances need no OS URL handler or connected desktop window.
        let result = mcp::McpClient::connect()?.call(
            "repo",
            serde_json::json!({"action": "add", "path": actual_path}),
        )?;
        if let Some(warning) = result["warning"].as_str() {
            eprintln!("warning: {warning}");
        }
        eprintln!("Opening {actual_path}");
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

/// Truncate to `max` chars with a trailing ellipsis so it fits a fixed column.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        s.to_string()
    }
}

/// Shorten a repo path to its last two components (e.g. `personal/tuicommander`).
fn short_repo(path: &str) -> String {
    path.rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/")
}

/// Derive a single-word status from a session's nested `state` object.
/// The `/sessions` response carries no top-level status field — state lives
/// under `state` as `awaiting_input` / `agent_state` / `shell_state`.
fn session_status(s: &serde_json::Value) -> String {
    let st = &s["state"];
    if st["awaiting_input"].as_bool().unwrap_or(false) {
        return "awaiting".to_string();
    }
    if let Some(agent_state) = st["agent_state"].as_str() {
        return agent_state.to_string();
    }
    if let Some(shell_state) = st["shell_state"].as_str() {
        return shell_state.to_string();
    }
    "-".to_string()
}

/// First segment of a session UUID — enough to identify a session by eye.
fn short_id(id: &str) -> &str {
    id.split('-').next().unwrap_or(id)
}

fn fetch_sessions() -> Result<Vec<serde_json::Value>, String> {
    let resp = ipc::get("/sessions").map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err(format!("Server error: {}", resp.status));
    }
    let sessions: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    Ok(sessions.as_array().cloned().unwrap_or_default())
}

fn cmd_ls(json: bool) -> Result<(), String> {
    let arr = fetch_sessions()?;

    if json {
        // Scripts get the untouched server payload; humans get the table.
        println!("{}", serde_json::Value::Array(arr));
        return Ok(());
    }

    if arr.is_empty() {
        println!("No active sessions.");
        return Ok(());
    }

    println!("{:<10} {:<24} {:<10} REPO", "ID", "NAME", "STATUS");

    for s in &arr {
        let id = short_id(s["session_id"].as_str().unwrap_or("-"));
        let name = truncate(s["display_name"].as_str().unwrap_or("-"), 24);
        let status = session_status(s);
        let repo = short_repo(s["cwd"].as_str().unwrap_or("-"));
        println!("{:<10} {:<24} {:<10} {}", id, name, status, repo);
    }

    Ok(())
}

/// Create a session, then type a command into it. Two round-trips because the
/// create endpoint takes no command — same thing you would do by hand.
fn cmd_run(command: &[String], name: Option<&str>, repo: Option<&str>) -> Result<(), String> {
    let id = cmd_new(name, repo)?;
    let body = serde_json::json!({ "data": format!("{}\r", command.join(" ")) });
    let resp = ipc::post(&format!("/sessions/{id}/write"), &body.to_string())
        .map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err(format!("Session created but command failed: {}", resp.body));
    }
    Ok(())
}

/// Returns the new session id so callers can keep driving it.
fn cmd_new(name: Option<&str>, repo: Option<&str>) -> Result<String, String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;

    let repo_path = match repo {
        Some(r) => resolve_path(r),
        None => std::env::current_dir()
            .map(|d| d.to_string_lossy().to_string())
            .map_err(|e| format!("Cannot get cwd: {e}"))?,
    };

    let body = serde_json::json!({
        "cwd": repo_path,
        "rows": 24,
        "cols": 80,
    });

    let resp = ipc::post("/sessions", &body.to_string()).map_err(|e| e.to_string())?;
    if !resp.is_success() {
        return Err(format!("Failed to create session: {}", resp.body));
    }

    let created = resp.json().map_err(|e| e.to_string())?;
    let id = created["session_id"]
        .as_str()
        .ok_or("Server returned no session id")?
        .to_string();

    // Naming is a separate endpoint — the create request has no name field.
    if let Some(n) = name {
        let name_body = serde_json::json!({ "name": n });
        match ipc::put(&format!("/sessions/{id}/name"), &name_body.to_string()) {
            Ok(r) if r.is_success() => {}
            Ok(r) => eprintln!("tuic: warning: could not set session name: {}", r.body),
            Err(e) => eprintln!("tuic: warning: could not set session name: {e}"),
        }
    }

    println!("{}: {}", name.unwrap_or(short_id(&id)), short_id(&id));
    Ok(id)
}

fn cmd_send(target: &str, keys: &[String]) -> Result<(), String> {
    mcp_session_call(
        "input",
        target,
        serde_json::json!({"input": translate_keys(keys)}),
    )?;
    Ok(())
}

/// Build the `/output` query for `capture`. `raw` takes no format so the server
/// returns the untouched byte tail; `lines` maps to the server's `limit`.
fn capture_payload(format: &str, lines: Option<usize>) -> Result<serde_json::Value, String> {
    if !matches!(format, "raw" | "text") {
        return Err(format!(
            "capture format '{format}' is not supported; use raw or text"
        ));
    }
    Ok(optional_fields(
        serde_json::json!({"format": format}),
        [("limit", lines.map(serde_json::Value::from))],
    ))
}

fn cmd_capture(target: &str, format: &str, lines: Option<usize>) -> Result<(), String> {
    let payload = mcp_session_call("output", target, capture_payload(format, lines)?)?;
    if let Some(data) = payload["data"].as_str() {
        print!("{data}");
    } else if let Some(lines) = payload["lines"].as_array() {
        for line in lines {
            if let Some(text) = line["text"].as_str() {
                println!("{text}");
            }
        }
    } else {
        print_mcp_payload(&payload, false);
    }

    Ok(())
}

fn cmd_kill(target: &str) -> Result<(), String> {
    mcp_session_call("kill", target, serde_json::json!({}))?;
    Ok(())
}

fn cmd_resize(target: &str, size: &str) -> Result<(), String> {
    let parts: Vec<&str> = size.split('x').collect();
    if parts.len() != 2 {
        return Err("Size must be WIDTHxHEIGHT (e.g. 120x40)".to_string());
    }
    let cols: u16 = parts[0].parse().map_err(|_| "Invalid width")?;
    let rows: u16 = parts[1].parse().map_err(|_| "Invalid height")?;

    mcp_session_call(
        "resize",
        target,
        serde_json::json!({"rows": rows, "cols": cols}),
    )?;
    Ok(())
}

fn cmd_agent(action: AgentAction) -> Result<(), String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;

    match action {
        AgentAction::Spawn(args) => {
            let AgentSpawnArgs {
                agent_type,
                prompt,
                repo,
                name,
                model,
                args,
                cwd,
                print_mode,
                pty_description,
                rows,
                cols,
                output_format,
                binary_path,
                json,
            } = *args;
            let cwd = match cwd.or(repo) {
                Some(r) => resolve_path(&r),
                None => std::env::current_dir()
                    .map(|d| d.to_string_lossy().to_string())
                    .map_err(|e| format!("Cannot get cwd: {e}"))?,
            };

            let payload = agent_spawn_payload(AgentSpawnInput {
                agent_type: &agent_type,
                prompt: &prompt,
                cwd: &cwd,
                name: name.as_deref(),
                model: model.as_deref(),
                args: &args,
                print_mode,
                pty_description: pty_description.as_deref(),
                rows,
                cols,
                output_format: output_format.as_deref(),
                binary_path: binary_path.as_deref(),
            });
            let response = mcp::McpClient::connect_for_orchestration()?.call("agent", payload)?;
            print_mcp_payload(&response, json);
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
                .filter(|s| s["state"]["agent_type"].as_str().is_some())
                .collect();

            if agents.is_empty() {
                println!("No active agents.");
                return Ok(());
            }

            println!("{:<38} {:<12} {:<10} REPO", "ID", "TYPE", "STATUS");
            println!("{}", "-".repeat(82));

            for s in &agents {
                let id = s["session_id"].as_str().unwrap_or("-");
                let agent_type = s["state"]["agent_type"].as_str().unwrap_or("-");
                let status = session_status(s);
                let repo = short_repo(s["cwd"].as_str().unwrap_or("-"));
                println!("{:<38} {:<12} {:<10} {}", id, agent_type, status, repo);
            }
        }
        AgentAction::Send {
            target,
            message,
            json,
        } => {
            // Peer routing only — deliberately NOT a session action. That
            // resolves PTYs, and a registered external orchestrator has no PTY,
            // so routing through it answered "Session not found" while the MCP
            // tool delivered the same UUID fine. PTY text injection stays
            // available, and explicit, as `tuic send` / `tuic send-keys`.
            let client = mcp::McpClient::connect_for_orchestration()?;
            let report = mcp::agent_send(&client, &target, &message)?;
            if json {
                println!("{report}");
            } else {
                println!("{}", mcp::delivery_line(&target, &report));
            }
        }
        AgentAction::Type { target, message } => {
            let (payload, enter) = agent_send_parts(&message);
            mcp_session_call("input", &target, serde_json::json!({"input": payload}))?;

            // Raw-mode agent TUIs require Enter in a later PTY read. A combined
            // `message\r` is commonly treated as a prefill and left unsent.
            std::thread::sleep(std::time::Duration::from_millis(100));
            mcp_session_call("input", &target, serde_json::json!({"input": enter}))?;
        }
        AgentAction::Wait {
            since,
            timeout_ms,
            json,
        } => {
            let payload = optional_fields(
                serde_json::json!({"action": "wait"}),
                [
                    ("since", since.map(serde_json::Value::from)),
                    ("timeout_ms", timeout_ms.map(serde_json::Value::from)),
                ],
            );
            print_mcp_payload(
                &mcp::McpClient::connect_for_orchestration()?.call("agent", payload)?,
                json,
            );
        }
        AgentAction::Inbox { since, limit, json } => {
            let payload = optional_fields(
                serde_json::json!({"action": "inbox"}),
                [
                    ("since", since.map(serde_json::Value::from)),
                    ("limit", limit.map(serde_json::Value::from)),
                ],
            );
            print_mcp_payload(
                &mcp::McpClient::connect_for_orchestration()?.call("agent", payload)?,
                json,
            );
        }
        AgentAction::ListPeers { path, json } => {
            let payload = optional_fields(
                serde_json::json!({"action": "list_peers"}),
                [("path", path.map(serde_json::Value::from))],
            );
            print_mcp_payload(&mcp::McpClient::connect()?.call("agent", payload)?, json);
        }
        AgentAction::Stats { json } => {
            let response = ipc::get("/stats").map_err(|e| e.to_string())?;
            if !response.is_success() {
                return Err(format!("Server error: {}", response.status));
            }
            print_mcp_payload(&response.json().map_err(|e| e.to_string())?, json);
        }
    }

    Ok(())
}

struct AgentSpawnInput<'a> {
    agent_type: &'a str,
    prompt: &'a str,
    cwd: &'a str,
    name: Option<&'a str>,
    model: Option<&'a str>,
    args: &'a [String],
    print_mode: bool,
    pty_description: Option<&'a str>,
    rows: Option<u16>,
    cols: Option<u16>,
    output_format: Option<&'a str>,
    binary_path: Option<&'a str>,
}

fn agent_spawn_payload(input: AgentSpawnInput<'_>) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "action": "spawn", "agent_type": input.agent_type, "prompt": input.prompt,
        "cwd": input.cwd, "print_mode": input.print_mode,
    });
    let fields = [
        ("name", input.name.map(serde_json::Value::from)),
        ("model", input.model.map(serde_json::Value::from)),
        (
            "pty_description",
            input.pty_description.map(serde_json::Value::from),
        ),
        ("rows", input.rows.map(serde_json::Value::from)),
        ("cols", input.cols.map(serde_json::Value::from)),
        (
            "output_format",
            input.output_format.map(serde_json::Value::from),
        ),
        (
            "binary_path",
            input.binary_path.map(serde_json::Value::from),
        ),
    ];
    payload = optional_fields(payload, fields);
    if !input.args.is_empty() {
        payload["args"] = serde_json::json!(input.args);
    }
    payload
}

fn optional_fields<const N: usize>(
    mut payload: serde_json::Value,
    fields: [(&str, Option<serde_json::Value>); N],
) -> serde_json::Value {
    let object = payload.as_object_mut().expect("payload is an object");
    for (name, value) in fields {
        if let Some(value) = value {
            object.insert(name.to_string(), value);
        }
    }
    payload
}

fn print_mcp_payload(payload: &serde_json::Value, json: bool) {
    if json {
        println!("{payload}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string())
        );
    }
}

/// Ask the authoritative MCP session resolver to accept a PTY id,
/// `tuic_session`, or terminal alias. The CLI must not infer addresses from
/// `/sessions`, because that list has no peer identity or alias contract.
fn mcp_session_call(
    action: &str,
    target: &str,
    fields: serde_json::Value,
) -> Result<serde_json::Value, String> {
    mcp::McpClient::connect()?.call("session", session_payload(action, target, fields)?)
}

fn session_payload(
    action: &str,
    target: &str,
    mut fields: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let object = fields
        .as_object_mut()
        .ok_or("session payload is not an object")?;
    object.insert("action".to_string(), serde_json::json!(action));
    object.insert("session_id".to_string(), serde_json::json!(target));
    Ok(fields)
}

fn cmd_mcp_session(action: McpSessionAction) -> Result<(), String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;
    let (payload, json) = match action {
        McpSessionAction::Status { target, json } => (
            serde_json::json!({"action": "status", "session_id": target}),
            json,
        ),
        McpSessionAction::Wait {
            target,
            until,
            timeout_ms,
            json,
        } => (
            optional_fields(
                serde_json::json!({"action": "wait", "session_id": target, "until": until}),
                [("timeout_ms", timeout_ms.map(serde_json::Value::from))],
            ),
            json,
        ),
        McpSessionAction::Output {
            target,
            since_cursor,
            from_line,
            limit,
            json,
        } => (
            optional_fields(
                serde_json::json!({"action": "output", "session_id": target}),
                [
                    ("since_cursor", since_cursor.map(serde_json::Value::from)),
                    ("from_line", from_line.map(serde_json::Value::from)),
                    ("limit", limit.map(serde_json::Value::from)),
                ],
            ),
            json,
        ),
    };
    let target = payload["session_id"]
        .as_str()
        .ok_or("missing session target")?;
    let fields = payload
        .as_object()
        .cloned()
        .ok_or("session payload is not an object")?;
    let mut fields = serde_json::Value::Object(fields);
    fields.as_object_mut().expect("object").remove("action");
    fields.as_object_mut().expect("object").remove("session_id");
    print_mcp_payload(
        &mcp_session_call(
            payload["action"].as_str().unwrap_or_default(),
            target,
            fields,
        )?,
        json,
    );
    Ok(())
}

fn cmd_repo(action: RepoAction) -> Result<(), String> {
    ipc::ensure_running().map_err(|e| e.to_string())?;
    let (payload, json) = match action {
        RepoAction::List { path, json } => (
            serde_json::json!({"action": "worktree_list", "path": resolve_path(&path)}),
            json,
        ),
        RepoAction::Create {
            path,
            branch,
            base_ref,
            spawn_session,
            json,
        } => (
            optional_fields(
                serde_json::json!({"action": "worktree_create", "path": resolve_path(&path), "spawn_session": spawn_session}),
                [
                    ("branch", branch.map(serde_json::Value::from)),
                    ("base_ref", base_ref.map(serde_json::Value::from)),
                ],
            ),
            json,
        ),
        RepoAction::Remove {
            path,
            branch,
            force,
            json,
        } => (
            serde_json::json!({"action": "worktree_remove", "path": resolve_path(&path), "branch": branch, "force": force}),
            json,
        ),
    };
    print_mcp_payload(&mcp::McpClient::connect()?.call("repo", payload)?, json);
    Ok(())
}

fn agent_send_parts(message: &str) -> (String, &'static str) {
    let payload = if message.contains('\n') {
        format!("\x15\x1b[200~{message}\x1b[201~")
    } else {
        format!("\x15{message}")
    };
    (payload, "\r")
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

    let sessions = fetch_sessions()?;
    let agents = sessions
        .iter()
        .filter(|s| s["state"]["agent_type"].as_str().is_some())
        .count();
    let waiting: Vec<&serde_json::Value> = sessions
        .iter()
        .filter(|s| s["state"]["awaiting_input"].as_bool().unwrap_or(false))
        .collect();

    println!("TUICommander v{version}");
    println!("Status: running");
    println!("Sessions: {} ({agents} agents)", sessions.len());

    // The only genuinely actionable line: who is blocked on you right now.
    if !waiting.is_empty() {
        println!("Awaiting input:");
        for s in waiting {
            let id = short_id(s["session_id"].as_str().unwrap_or("-"));
            let name = s["display_name"].as_str().unwrap_or("-");
            println!(
                "  {id}  {name}  ({})",
                short_repo(s["cwd"].as_str().unwrap_or("-"))
            );
        }
    }

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
    if let Ok(existing) = std::fs::read_link(target_path)
        && existing == self_exe
    {
        println!("Already installed at {target_path}");
        return Ok(());
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
        #[cfg(target_os = "macos")]
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
        if let Ok(target) = std::fs::read_link(&tmux_path)
            && target == self_exe
        {
            println!("tmux alias already installed at {tmux_path}");
            return Ok(());
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
    mcp_session_call("pause", target, serde_json::json!({}))?;
    Ok(())
}

fn cmd_resume(target: &str) -> Result<(), String> {
    mcp_session_call("resume", target, serde_json::json!({}))?;
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
        "list-sessions" | "ls" => dispatch(Command::Ls { json: false }),
        "kill-session" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            dispatch(Command::Kill { target })
        }
        "kill-server" => {
            // Kill all sessions
            if let Ok(resp) = ipc::get("/sessions")
                && let Ok(v) = resp.json()
                && let Some(arr) = v.as_array()
            {
                for s in arr {
                    if let Some(id) = s["session_id"].as_str() {
                        let _ = ipc::delete(&format!("/sessions/{id}"));
                    }
                }
            }
            Ok(())
        }
        "send-keys" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            dispatch(Command::Send {
                target,
                keys: without_flag(rest, "-t"),
            })
        }
        "capture-pane" => {
            let target = find_flag(rest, "-t").unwrap_or_default();
            dispatch(Command::Capture {
                target,
                format: "text".to_string(),
                lines: None,
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
            match mcp_session_call("status", &target, serde_json::json!({})) {
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
    let absolute = absolute_path(path);
    // `tuic .` must not register the repo as `/repo/.` — canonicalize when the
    // path exists. Non-existent paths (a file being created, `file.rs:42` before
    // the line suffix is split off) keep the plain absolute form.
    std::fs::canonicalize(&absolute)
        .map(|p| strip_verbatim(&p.to_string_lossy()))
        .unwrap_or(absolute)
}

/// Directories a repository can never legitimately be registered from.
///
/// `tuic <dir>` registers the directory in `repositories.json` *permanently* —
/// the sidebar keeps it across restarts. Handed a temp directory, that is a row
/// pointing at something the OS deletes, and it outlives every trace of who
/// created it: fifteen such rows accumulated in Boss's config between
/// 2026-09-11 and 2026-09-13 (#763-d219).
///
/// The check lives in the CLI, not in the app, on purpose. `std::env::temp_dir`
/// reads `TMPDIR`/`TEMP` from *this* process, so it resolves to the caller's
/// shell — which is how `~/Gits/.tmp` (this repo's own `TMPDIR` convention for
/// Rust suites) is caught. The app process has a different `TMPDIR` and cannot
/// see it.
fn disposable_roots() -> Vec<PathBuf> {
    let roots =
        std::iter::once(std::env::temp_dir()).chain(cfg!(unix).then_some(PathBuf::from("/tmp")));
    roots
        .into_iter()
        .flat_map(|root| {
            // Keep both forms: on macOS `/var/folders/…` canonicalizes to
            // `/private/var/folders/…`, and a caller can hand us either.
            std::fs::canonicalize(&root)
                .ok()
                .into_iter()
                .chain(std::iter::once(root))
        })
        .collect()
}

/// `true` when `path` is one of `roots` or lives below it. Compares components,
/// never string prefixes — `/tmpfoo` is not inside `/tmp`.
fn is_disposable_root(path: &Path, roots: &[PathBuf]) -> bool {
    let canonical = std::fs::canonicalize(path);
    let candidate: &Path = canonical.as_deref().unwrap_or(path);
    roots.iter().any(|root| candidate.starts_with(root))
}

/// Windows canonicalization yields verbatim paths (`\\?\C:\src`). The app stores
/// and displays plain paths, so drop the prefix to keep both sides comparable.
fn strip_verbatim(path: &str) -> String {
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

fn absolute_path(path: &str) -> String {
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

/// Translate one argument if it is EXACTLY a key name, else `None`.
///
/// Whole-token matching is the point: the old substring rewrite turned
/// `tuic send x "Enter the room"` into a carriage return followed by
/// "the room", and any text containing "Tab", "Space" or "C-c" was
/// corrupted the same way. tmux resolves key names per argument too.
fn key_sequence(token: &str) -> Option<String> {
    let named = match token {
        "Enter" | "C-m" => "\r",
        "Space" => " ",
        "Tab" => "\t",
        "Escape" | "Esc" => "\x1b",
        "BSpace" | "BackSpace" => "\x7f",
        "Up" => "\x1b[A",
        "Down" => "\x1b[B",
        "Right" => "\x1b[C",
        "Left" => "\x1b[D",
        "Home" => "\x1b[H",
        "End" => "\x1b[F",
        "PageUp" | "PPage" => "\x1b[5~",
        "PageDown" | "NPage" => "\x1b[6~",
        _ => return control_key(token),
    };
    Some(named.to_string())
}

/// `C-a` … `C-z` → the matching control byte, so the whole range works without
/// a hand-maintained table.
fn control_key(token: &str) -> Option<String> {
    let letter = token.strip_prefix("C-")?;
    let mut chars = letter.chars();
    let c = chars.next()?;
    if chars.next().is_some() || !c.is_ascii_alphabetic() {
        return None;
    }
    let byte = c.to_ascii_lowercase() as u8 - b'a' + 1;
    Some((byte as char).to_string())
}

/// Join `send` arguments the way tmux does: key names become their escape
/// sequence, everything else is literal text, and only adjacent literals are
/// separated by a space.
fn translate_keys(tokens: &[String]) -> String {
    let mut out = String::new();
    let mut previous_was_literal = false;
    for token in tokens {
        match key_sequence(token) {
            Some(seq) => {
                out.push_str(&seq);
                previous_was_literal = false;
            }
            None => {
                if previous_was_literal {
                    out.push(' ');
                }
                out.push_str(token);
                previous_was_literal = true;
            }
        }
    }
    out
}

fn find_flag(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

/// Drop `flag` and the value right after it, keeping every other argument —
/// including one that happens to equal the flag's value (`tmux send-keys -t
/// build build` must still send "build").
fn without_flag(args: &[String], flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == flag {
            skip_next = true;
            continue;
        }
        out.push(arg.clone());
    }
    out
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

#[cfg(test)]
mod tests {
    use super::{
        Cli, Command, Path, PathBuf, agent_send_parts, capture_payload, disposable_roots,
        is_disposable_root, resolve_path, session_status, short_id, short_repo, strip_verbatim,
        translate_keys, truncate, without_flag,
    };
    use clap::Parser;

    #[test]
    fn agent_spawn_payload_preserves_every_mcp_spawn_field() {
        let payload = super::agent_spawn_payload(super::AgentSpawnInput {
            agent_type: "codex",
            prompt: "do work",
            cwd: "/repo",
            name: Some("worker"),
            model: Some("gpt-5"),
            args: &["exec".into(), "--full-auto".into()],
            print_mode: true,
            pty_description: Some("task worker"),
            rows: Some(40),
            cols: Some(120),
            output_format: Some("json"),
            binary_path: Some("/usr/local/bin/codex"),
        });
        assert_eq!(
            payload,
            serde_json::json!({
                "action":"spawn", "agent_type":"codex", "prompt":"do work", "cwd":"/repo",
                "name":"worker", "model":"gpt-5", "args":["exec","--full-auto"], "print_mode":true,
                "pty_description":"task worker", "rows":40, "cols":120, "output_format":"json",
                "binary_path":"/usr/local/bin/codex"
            })
        );
    }

    #[test]
    fn session_target_is_forwarded_without_cli_resolution() {
        for target in ["01234567", "reviewer"] {
            let payload = super::session_payload("input", target, json!({"input":"echo ok"}))
                .expect("payload");
            assert_eq!(payload["session_id"], target);
        }
    }
    use serde_json::json;

    #[test]
    fn story_cli_accepts_a_typed_action_and_project() {
        let cli = Cli::try_parse_from([
            "tuic",
            "story",
            "{\"action\":\"list_plans\"}",
            "--project",
            "/repo",
        ])
        .expect("parse story command");
        assert!(matches!(
            cli.command,
            Some(Command::Story { action, project: Some(project), session_id: None })
                if action == "{\"action\":\"list_plans\"}" && project == "/repo"
        ));
    }

    #[test]
    fn agent_peer_filter_uses_path_name() {
        let cli = Cli::try_parse_from(["tuic", "agent", "list-peers", "--path", "/repo"])
            .expect("parse list-peers");
        assert!(matches!(
            cli.command,
            Some(Command::Agent {
                action: super::AgentAction::ListPeers { path, .. }
            }) if path.as_deref() == Some("/repo")
        ));
    }

    #[test]
    fn agent_spawn_keeps_positional_prompt_and_launcher_flags() {
        let cli = Cli::try_parse_from([
            "tuic",
            "agent",
            "spawn",
            "codex",
            "do work",
            "--cwd",
            "/repo",
            "--model",
            "gpt-5",
            "--json",
            "--args=--full-auto",
        ])
        .expect("parse agent spawn");
        assert!(matches!(
            cli.command,
            Some(Command::Agent {
                action: super::AgentAction::Spawn(args)
            }) if args.agent_type == "codex"
                && args.prompt == "do work"
                && args.cwd.as_deref() == Some("/repo")
                && args.model.as_deref() == Some("gpt-5")
                && args.args == ["--full-auto"]
                && args.json
        ));
    }

    #[test]
    fn repo_worktree_list_keeps_its_command_spelling() {
        let cli = Cli::try_parse_from(["tuic", "repo", "worktree-list", "/repo", "--json"])
            .expect("parse worktree-list");
        assert!(matches!(
            cli.command,
            Some(Command::Repo { action: super::RepoAction::List { path, json } })
                if path == "/repo" && json
        ));
    }

    #[test]
    fn repo_worktree_create_keeps_branch_and_base_flags() {
        let cli = Cli::try_parse_from([
            "tuic",
            "repo",
            "worktree-create",
            "/repo",
            "--branch",
            "feature",
            "--base-ref",
            "main",
            "--spawn-session",
        ])
        .expect("parse worktree-create");
        assert!(matches!(
            cli.command,
            Some(Command::Repo {
                action: super::RepoAction::Create { path, branch, base_ref, spawn_session, .. }
            }) if path == "/repo"
                && branch.as_deref() == Some("feature")
                && base_ref.as_deref() == Some("main")
                && spawn_session
        ));
    }

    #[test]
    fn repo_worktree_remove_keeps_force_flag() {
        let cli = Cli::try_parse_from([
            "tuic",
            "repo",
            "worktree-remove",
            "/repo",
            "feature",
            "--force",
        ])
        .expect("parse worktree-remove");
        assert!(matches!(
            cli.command,
            Some(Command::Repo {
                action: super::RepoAction::Remove { path, branch, force, .. }
            }) if path == "/repo" && branch == "feature" && force
        ));
    }

    fn tokens(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn send_translates_only_whole_key_tokens() {
        assert_eq!(
            translate_keys(&tokens(&["make test", "Enter"])),
            "make test\r"
        );
    }

    #[test]
    fn send_leaves_text_that_merely_contains_a_key_name_alone() {
        // The regression: substring replacement turned this into "\rthe room".
        assert_eq!(
            translate_keys(&tokens(&["Enter the room"])),
            "Enter the room"
        );
        assert_eq!(
            translate_keys(&tokens(&["Tab completion"])),
            "Tab completion"
        );
        assert_eq!(translate_keys(&tokens(&["C-c is SIGINT"])), "C-c is SIGINT");
    }

    #[test]
    fn send_joins_adjacent_literals_with_one_space_and_keys_without() {
        assert_eq!(
            translate_keys(&tokens(&["git", "status", "Enter"])),
            "git status\r"
        );
        assert_eq!(translate_keys(&tokens(&["Escape", "Escape"])), "\x1b\x1b");
    }

    #[test]
    fn send_maps_control_letters_and_navigation_keys() {
        assert_eq!(translate_keys(&tokens(&["C-c"])), "\x03");
        assert_eq!(translate_keys(&tokens(&["C-U"])), "\x15");
        assert_eq!(translate_keys(&tokens(&["Up", "Down"])), "\x1b[A\x1b[B");
        // Not a control key: two letters after C-, so it stays literal.
        assert_eq!(translate_keys(&tokens(&["C-ab"])), "C-ab");
    }

    #[test]
    fn capture_payload_rejects_log_and_omits_absent_limit() {
        assert_eq!(
            capture_payload("log", None).unwrap_err(),
            "capture format 'log' is not supported; use raw or text"
        );
        assert_eq!(
            capture_payload("text", None).expect("text payload"),
            json!({"format":"text"})
        );
        assert_eq!(
            capture_payload("raw", Some(10)).expect("raw payload"),
            json!({"format":"raw", "limit":10})
        );
    }

    #[test]
    fn short_id_is_the_first_uuid_group() {
        assert_eq!(short_id("43263870-7ac5-4091-9dca-c4acd22ad78f"), "43263870");
        assert_eq!(short_id("plain"), "plain");
    }

    #[test]
    fn without_flag_drops_the_pair_but_keeps_a_repeated_value() {
        assert_eq!(
            without_flag(&tokens(&["-t", "build", "build", "Enter"]), "-t"),
            tokens(&["build", "Enter"])
        );
    }

    #[test]
    fn resolve_path_canonicalizes_dot_so_a_repo_is_not_registered_with_a_trailing_component() {
        let resolved = resolve_path(".");
        assert!(!resolved.ends_with("/."), "got {resolved}");
        assert!(!resolved.ends_with("\\."), "got {resolved}");
    }

    #[test]
    fn resolve_path_keeps_paths_that_do_not_exist_yet() {
        let resolved = resolve_path("/definitely/not/here/new-file.rs");
        assert_eq!(resolved, "/definitely/not/here/new-file.rs");
    }

    // #763-d219 — `tuic <dir>` registers a repository permanently. The fifteen
    // rows it left in Boss's `repositories.json` were all temp roots, so the
    // shapes below are the ones actually observed on disk, not invented.
    #[test]
    fn a_temp_root_and_everything_under_it_is_disposable() {
        let roots = vec![
            PathBuf::from("/private/var/folders/3h/abc/T"),
            PathBuf::from("/Users/dev/Gits/.tmp"),
        ];

        for observed in [
            "/private/var/folders/3h/abc/T/.tmpmq7MuH",
            "/private/var/folders/3h/abc/T/.tmpmq7MuH/nested/deeper",
            "/Users/dev/Gits/.tmp/.tmpcDMEC4",
            // The root itself is no more registrable than its children.
            "/private/var/folders/3h/abc/T",
            "/Users/dev/Gits/.tmp",
        ] {
            assert!(
                is_disposable_root(Path::new(observed), &roots),
                "{observed} should be refused"
            );
        }
    }

    #[test]
    fn a_real_repository_is_not_disposable() {
        let roots = vec![PathBuf::from("/tmp"), PathBuf::from("/Users/dev/Gits/.tmp")];

        for keeper in [
            "/Users/dev/Gits/personal/tuicommander",
            // Shares a string prefix with a root but is a sibling of it, not a
            // child. A `starts_with` on strings would refuse these.
            "/tmpfoo",
            "/Users/dev/Gits/.tmpfiles/repo",
        ] {
            assert!(
                !is_disposable_root(Path::new(keeper), &roots),
                "{keeper} should be allowed"
            );
        }
    }

    #[test]
    fn disposable_roots_follow_this_process_tmpdir() {
        // The whole reason the check lives in the CLI: it reads the *caller's*
        // TMPDIR, which is what `~/Gits/.tmp` runs under in this repo.
        let roots = disposable_roots();
        let temp = std::env::temp_dir();
        assert!(
            is_disposable_root(&temp.join("scratch-repo"), &roots),
            "a directory under {} was not recognised; roots were {roots:?}",
            temp.display()
        );
    }

    #[test]
    fn strip_verbatim_removes_the_windows_prefix_only() {
        assert_eq!(strip_verbatim(r"\\?\C:\src\repo"), r"C:\src\repo");
        assert_eq!(strip_verbatim("/Users/dev/repo"), "/Users/dev/repo");
    }

    #[test]
    fn short_repo_keeps_last_two_components() {
        assert_eq!(
            short_repo("/Users/s/personal/tuicommander"),
            "personal/tuicommander"
        );
    }

    #[test]
    fn short_repo_single_component() {
        assert_eq!(short_repo("tuicommander"), "tuicommander");
    }

    #[test]
    fn short_repo_empty() {
        assert_eq!(short_repo(""), "");
    }

    #[test]
    fn truncate_leaves_short_unchanged() {
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(truncate("abcde", 5), "abcde"); // count == max, not >
    }

    #[test]
    fn truncate_cuts_and_appends_ellipsis() {
        assert_eq!(truncate("abcdefgh", 5), "abcd…"); // 4 chars + ellipsis = 5
    }

    #[test]
    fn truncate_counts_chars_not_bytes() {
        // Multi-byte input must not panic and must cut on char boundaries.
        assert_eq!(truncate("日本語abcdef", 5), "日本語a…");
    }

    #[test]
    fn session_status_awaiting_input_wins() {
        let s = json!({ "state": { "awaiting_input": true, "agent_state": "working" } });
        assert_eq!(session_status(&s), "awaiting");
    }

    #[test]
    fn session_status_prefers_agent_state_over_shell_state() {
        let s = json!({ "state": { "agent_state": "working", "shell_state": "idle" } });
        assert_eq!(session_status(&s), "working");
    }

    #[test]
    fn session_status_falls_back_to_shell_state() {
        let s = json!({ "state": { "shell_state": "busy" } });
        assert_eq!(session_status(&s), "busy");
    }

    #[test]
    fn session_status_defaults_to_dash_when_state_absent_or_empty() {
        assert_eq!(session_status(&json!({})), "-");
        assert_eq!(session_status(&json!({ "state": {} })), "-");
    }

    #[test]
    fn agent_send_separates_framed_payload_from_enter() {
        let (payload, enter) = agent_send_parts("report complete");
        assert_eq!(payload, "\x15report complete");
        assert!(!payload.contains('\r'));
        assert_eq!(enter, "\r");
    }

    #[test]
    fn agent_send_bracket_pastes_multiline_before_separate_enter() {
        let (payload, enter) = agent_send_parts("line one\nline two");
        assert_eq!(payload, "\x15\x1b[200~line one\nline two\x1b[201~");
        assert_eq!(enter, "\r");
    }
}
