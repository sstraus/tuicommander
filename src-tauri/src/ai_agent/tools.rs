use serde::Serialize;
use serde_json::{json, Value};
use std::io::Write;
use std::sync::Arc;

use crate::state::AppState;
use super::safety::{SafetyChecker, RegexSafetyChecker, SafetyVerdict, SafeKey, KeyRisk};
use super::sandbox::FileSandbox;

/// Max file size accepted by `read_file` / `edit_file` (10 MB).
const MAX_FILE_BYTES: u64 = 10 * 1024 * 1024;
/// Default page size for `read_file` when `limit` is unset.
const READ_FILE_DEFAULT_LINES: usize = 200;
/// Upper bound for `read_file` `limit`.
const READ_FILE_MAX_LINES: usize = 2000;

/// Result of executing an agent tool.
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    /// When true, the engine should pause for user approval before re-executing.
    #[serde(skip)]
    pub needs_approval: bool,
    #[serde(skip)]
    pub approval_reason: Option<String>,
    #[serde(skip)]
    pub approval_command: Option<String>,
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self { success: true, output: output.into(), needs_approval: false, approval_reason: None, approval_command: None }
    }

    pub fn err(output: impl Into<String>) -> Self {
        Self { success: false, output: output.into(), needs_approval: false, approval_reason: None, approval_command: None }
    }

    pub fn approval(reason: impl Into<String>, command: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            success: false,
            output: format!("Needs approval: {reason}"),
            needs_approval: true,
            approval_reason: Some(reason),
            approval_command: Some(command.into()),
        }
    }
}

// ── Tool definitions (JSON Schema) ────────────────────────────

/// Returns the 6 agent tool definitions as a JSON array of MCP-style objects.
/// Each tool has `name`, `description`, and `inputSchema` fields.
pub fn tool_definitions() -> Value {
    json!([
        {
            "name": "read_screen",
            "description": "Read the current visible terminal content. In TUI mode (alternate screen), returns the app's screen; in shell mode, returns recent scrollback lines.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" },
                    "lines": { "type": "integer", "description": "Max lines to return (default: 50)", "default": 50 }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "send_input",
            "description": "Send a command to the terminal. Uses Ctrl-U prefix to clear any existing input, then types the command and presses Enter. Safe for both shells and Ink-based agents.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" },
                    "command": { "type": "string", "description": "Command text to send" }
                },
                "required": ["session_id", "command"]
            }
        },
        {
            "name": "send_key",
            "description": "Send a special key to the terminal (e.g., ctrl-c, ctrl-d, ctrl-z, escape, enter, tab, up, down).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" },
                    "key": { "type": "string", "description": "Key name: ctrl-c, ctrl-d, ctrl-z, escape, enter, tab, up, down, left, right" }
                },
                "required": ["session_id", "key"]
            }
        },
        {
            "name": "wait_for",
            "description": "Wait until a regex pattern appears in terminal output or output stabilizes. Returns the matching text or timeout notice.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" },
                    "pattern": { "type": "string", "description": "Regex pattern to match (optional — omit for stability wait)" },
                    "timeout_ms": { "type": "integer", "description": "Max wait time in ms (default: 10000)", "default": 10000 },
                    "stability_ms": { "type": "integer", "description": "Consider stable after this many ms without new output (default: 500)", "default": 500 }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "get_state",
            "description": "Get the current session state including shell state, agent type, intent, question status, and recent parsed events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "get_context",
            "description": "Get compact terminal context: shell state, CWD, git branch, dirty status, recent exit codes. ~500 chars for system prompt injection.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "PTY session ID" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "read_file",
            "description": "Read a text file from the session's sandboxed repo. Paginated: returns up to 200 lines by default (max 2000). Binary files and files >10MB are rejected. Output is prefixed with line numbers. Secrets are redacted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Path relative to the sandbox root, or absolute path within it" },
                    "offset": { "type": "integer", "description": "0-based line offset to start reading from", "default": 0 },
                    "limit": { "type": "integer", "description": "Max lines to return (default 200, max 2000)", "default": 200 }
                },
                "required": ["file_path"]
            }
        },
        {
            "name": "write_file",
            "description": "Create or overwrite a text file within the session's sandbox. Writes atomically via tmp+rename. Use `edit_file` for surgical changes to existing files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Path relative to the sandbox root, or absolute path within it" },
                    "content": { "type": "string", "description": "Full file content to write" }
                },
                "required": ["file_path", "content"]
            }
        },
        {
            "name": "edit_file",
            "description": "Surgical search-and-replace on a file. `old_string` must appear exactly once unless `replace_all` is true. Include enough surrounding context in `old_string` to disambiguate.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string", "description": "Path relative to the sandbox root, or absolute path within it" },
                    "old_string": { "type": "string", "description": "Exact text to replace (must be unique unless replace_all=true)" },
                    "new_string": { "type": "string", "description": "Replacement text" },
                    "replace_all": { "type": "boolean", "description": "Replace every occurrence (default false)", "default": false }
                },
                "required": ["file_path", "old_string", "new_string"]
            }
        },
        {
            "name": "list_files",
            "description": "List files inside the session's sandbox matching a glob pattern. Returns up to 500 entries and a truncation flag. Use `path` to anchor the glob at a subdirectory (defaults to the sandbox root).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Glob pattern (e.g. `src/**/*.rs`)" },
                    "path": { "type": "string", "description": "Subdirectory under the sandbox to anchor the glob (default: sandbox root)" }
                },
                "required": ["pattern"]
            }
        },
        {
            "name": "search_files",
            "description": "Regex search across files in the session's sandbox. Honors .gitignore via the `ignore` crate. Returns up to 50 matches with configurable context lines and per-file match counts.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regular expression to search for" },
                    "path": { "type": "string", "description": "Subdirectory under the sandbox to search (default: sandbox root)" },
                    "glob": { "type": "string", "description": "Optional file glob filter (e.g. `*.rs`)" },
                    "context_lines": { "type": "integer", "description": "Lines of context before and after each match (default 2, max 10)", "default": 2 }
                },
                "required": ["pattern"]
            }
        },
        {
            "name": "run_command",
            "description": "Run a shell command inside the session's sandbox and capture stdout/stderr. The command runs via `sh -c` with a sanitized environment. Destructive commands are blocked by the safety checker. Output is truncated at 30K chars (head+tail) and secrets are redacted.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute" },
                    "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds (default 120000, max 600000)", "default": 120000 },
                    "cwd": { "type": "string", "description": "Working directory relative to sandbox root (default: sandbox root)" }
                },
                "required": ["command"]
            }
        }
    ])
}

// ── Secret redaction ──────────────────────────────────────────

/// Redact known secret patterns from terminal output.
pub fn redact_secrets(text: &str) -> String {
    use regex::Regex;
    use std::sync::LazyLock;

    static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
        vec![
            // API keys / tokens
            Regex::new(r"sk-[A-Za-z0-9_-]{20,}").unwrap(),
            Regex::new(r"AKIA[A-Z0-9]{16}").unwrap(),
            Regex::new(r"ghp_[A-Za-z0-9]{36,}").unwrap(),
            Regex::new(r"gho_[A-Za-z0-9]{36,}").unwrap(),
            Regex::new(r"xoxb-[A-Za-z0-9\-]+").unwrap(),
            Regex::new(r"ya29\.[A-Za-z0-9_-]+").unwrap(),
            // PEM private keys
            Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap(),
            // Bearer tokens
            Regex::new(r"Bearer\s+[A-Za-z0-9_\-.]+").unwrap(),
            // Database URLs with credentials
            Regex::new(r"(?i)(postgres|mysql|mongodb|redis)://[^\s@]+@[^\s]+").unwrap(),
            // Generic DATABASE_URL value
            Regex::new(r"DATABASE_URL=[^\s]+").unwrap(),
        ]
    });

    let mut result = text.to_owned();
    for pattern in PATTERNS.iter() {
        result = pattern.replace_all(&result, "[REDACTED]").to_string();
    }
    result
}

// ── Key mapping ───────────────────────────────────────────────

/// Map a key name to the corresponding escape sequence and optional SafeKey.
fn map_key(name: &str) -> Result<(String, Option<SafeKey>), String> {
    match name.to_lowercase().as_str() {
        "ctrl-c" | "ctrl+c" => Ok(("\x03".to_string(), Some(SafeKey::CtrlC))),
        "ctrl-d" | "ctrl+d" => Ok(("\x04".to_string(), Some(SafeKey::CtrlD))),
        "ctrl-z" | "ctrl+z" => Ok(("\x1a".to_string(), Some(SafeKey::CtrlZ))),
        "escape" | "esc" => Ok(("\x1b".to_string(), Some(SafeKey::Escape))),
        "enter" | "return" => Ok(("\r".to_string(), None)),
        "tab" => Ok(("\t".to_string(), None)),
        "up" => Ok(("\x1b[A".to_string(), None)),
        "down" => Ok(("\x1b[B".to_string(), None)),
        "right" => Ok(("\x1b[C".to_string(), None)),
        "left" => Ok(("\x1b[D".to_string(), None)),
        "backspace" => Ok(("\x7f".to_string(), None)),
        other => Err(format!("Unknown key: {other}")),
    }
}

// ── Safe PTY write ────────────────────────────────────────────

/// Write to a PTY session — replicates sendCommand semantics from TypeScript.
/// Ctrl-U prefix clears existing input, then text, then \r on separate write.
fn safe_pty_write(
    state: &AppState,
    session_id: &str,
    command: &str,
) -> Result<(), String> {
    let entry = state.sessions.get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let mut session = entry.lock();

    // Write 1: Ctrl-U + command text
    let payload = format!("\x15{command}");
    session.writer.write_all(payload.as_bytes())
        .map_err(|e| format!("PTY write failed: {e}"))?;
    session.writer.flush()
        .map_err(|e| format!("PTY flush failed: {e}"))?;

    // Write 2: Enter (separate write for Ink agent compat)
    session.writer.write_all(b"\r")
        .map_err(|e| format!("PTY write \\r failed: {e}"))?;
    session.writer.flush()
        .map_err(|e| format!("PTY flush failed: {e}"))?;

    Ok(())
}

/// Write raw bytes to a PTY (for send_key).
fn raw_pty_write(
    state: &AppState,
    session_id: &str,
    data: &[u8],
) -> Result<(), String> {
    let entry = state.sessions.get(session_id)
        .ok_or_else(|| format!("Session not found: {session_id}"))?;
    let mut session = entry.lock();
    session.writer.write_all(data)
        .map_err(|e| format!("PTY write failed: {e}"))?;
    session.writer.flush()
        .map_err(|e| format!("PTY flush failed: {e}"))?;
    Ok(())
}

// ── Tool execution ────────────────────────────────────────────

/// Execute `read_screen`: return visible terminal text.
fn exec_read_screen(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let max_lines = args["lines"].as_u64().unwrap_or(50) as usize;

    let vt_log = match state.vt_log_buffers.get(session_id) {
        Some(v) => v,
        None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
    };
    let vt = vt_log.lock();
    let rows = vt.screen_rows();

    // Trim trailing empty rows and limit
    let trimmed: Vec<&str> = rows.iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>();
    let last_non_empty = trimmed.iter()
        .rposition(|r| !r.trim().is_empty())
        .map(|i| i + 1)
        .unwrap_or(0);
    let visible = &trimmed[..last_non_empty.min(max_lines)];

    let output = redact_secrets(&visible.join("\n"));
    ToolResult::ok(output)
}

/// Execute `send_input`: send a command with safety check.
fn exec_send_input(state: &AppState, args: &Value) -> ToolResult {
    exec_send_input_inner(state, args, false)
}

fn exec_send_input_inner(state: &AppState, args: &Value, skip_safety: bool) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let command = match args["command"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing command"),
    };

    if !skip_safety {
        let checker = RegexSafetyChecker::new();
        match checker.evaluate(command) {
            SafetyVerdict::Allow => {}
            SafetyVerdict::NeedsApproval { reason } => {
                return ToolResult::approval(reason, command);
            }
            verdict => {
                let rejection = super::safety::format_rejection(&verdict).unwrap_or_default();
                return ToolResult::err(rejection);
            }
        }
    }

    match safe_pty_write(state, session_id, command) {
        Ok(()) => ToolResult::ok(format!("Sent: {command}")),
        Err(e) => ToolResult::err(e),
    }
}

/// Execute `send_key`: send a special key with safety check.
fn exec_send_key(state: &AppState, args: &Value) -> ToolResult {
    exec_send_key_inner(state, args, false)
}

fn exec_send_key_inner(state: &AppState, args: &Value, skip_safety: bool) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let key_name = match args["key"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing key"),
    };

    let (seq, safe_key) = match map_key(key_name) {
        Ok(pair) => pair,
        Err(e) => return ToolResult::err(e),
    };

    if !skip_safety {
        if let Some(sk) = safe_key {
            if sk.risk() == KeyRisk::High {
                return ToolResult::approval(
                    format!("{key_name} is high-risk (may terminate shell)"),
                    format!("send_key:{key_name}"),
                );
            }
        }
    }

    match raw_pty_write(state, session_id, seq.as_bytes()) {
        Ok(()) => ToolResult::ok(format!("Sent key: {key_name}")),
        Err(e) => ToolResult::err(e),
    }
}

/// Execute `wait_for`: poll VtLogBuffer for pattern or stability.
async fn exec_wait_for(state: &Arc<AppState>, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s.to_string(),
        None => return ToolResult::err("Missing session_id"),
    };
    let pattern = args["pattern"].as_str();
    let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(10_000).min(60_000);
    let stability_ms = args["stability_ms"].as_u64().unwrap_or(500).min(10_000);

    let compiled = match pattern {
        Some(p) => match regex::Regex::new(p) {
            Ok(r) => Some(r),
            Err(e) => return ToolResult::err(format!("Invalid regex: {e}")),
        },
        None => None,
    };

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
    let mut last_content = String::new();
    let mut stable_since = tokio::time::Instant::now();

    loop {
        if tokio::time::Instant::now() >= deadline {
            return ToolResult::err("Timeout waiting for pattern or stability");
        }

        let current = {
            let vt_log = match state.vt_log_buffers.get(&session_id) {
                Some(v) => v,
                None => return ToolResult::err(format!("No VT buffer for session: {session_id}")),
            };
            let vt = vt_log.lock();
            vt.screen_rows().join("\n")
        };

        // Check regex match
        if let Some(ref re) = compiled {
            if let Some(m) = re.find(&current) {
                return ToolResult::ok(redact_secrets(m.as_str()));
            }
        }

        // Check stability
        if current != last_content {
            last_content = current;
            stable_since = tokio::time::Instant::now();
        } else if compiled.is_none()
            && stable_since.elapsed() >= std::time::Duration::from_millis(stability_ms)
        {
            return ToolResult::ok(redact_secrets(&last_content));
        }

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Execute `get_state`: return SessionState + terminal mode.
fn exec_get_state(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };

    let ss = state.session_states.get(session_id)
        .map(|entry| serde_json::to_value(entry.value()).ok())
        .flatten();

    match ss {
        Some(v) => ToolResult::ok(v.to_string()),
        None => ToolResult::err(format!("No state for session: {session_id}")),
    }
}

/// Execute `get_context`: compact context string (~500 chars).
fn exec_get_context(state: &AppState, args: &Value) -> ToolResult {
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };

    let shell_state = state.shell_states.get(session_id)
        .map(|atom| crate::pty::shell_state_str(
            atom.load(std::sync::atomic::Ordering::Relaxed)
        ).to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let ss = state.session_states.get(session_id);
    let agent_type = ss.as_ref()
        .and_then(|s| s.agent_type.clone())
        .unwrap_or_else(|| "none".to_string());
    let terminal_mode = ss.as_ref()
        .and_then(|s| s.terminal_mode.as_ref())
        .map(|m| serde_json::to_string(m).unwrap_or_default())
        .unwrap_or_else(|| "shell".to_string());

    let context = json!({
        "shell_state": shell_state,
        "agent_type": agent_type,
        "terminal_mode": terminal_mode,
        "session_id": session_id,
    });

    ToolResult::ok(context.to_string())
}

// ── Filesystem tools ──────────────────────────────────────────

/// Fetch (or lazily create) the filesystem sandbox for a session. The CWD
/// fallback keeps tests + unconfigured sessions usable; production wiring
/// will populate the map at agent-loop start.
fn get_sandbox(state: &AppState, session_id: &str) -> Result<FileSandbox, String> {
    if let Some(sb) = state.file_sandboxes.get(session_id) {
        return Ok(sb.clone());
    }
    Err(format!("No filesystem sandbox for session: {session_id}"))
}

fn missing_arg(name: &str) -> ToolResult {
    ToolResult::err(format!("Missing argument: {name}"))
}

/// `read_file`: paginated, line-numbered file read with binary + size guards.
fn exec_read_file(state: &AppState, session_id: &str, args: &Value) -> ToolResult {
    let Some(file_path) = args["file_path"].as_str() else {
        return missing_arg("file_path");
    };
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let requested_limit = args["limit"].as_u64().map(|v| v as usize).unwrap_or(READ_FILE_DEFAULT_LINES);
    let limit = requested_limit.min(READ_FILE_MAX_LINES).max(1);

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };
    let resolved = match sandbox.resolve(file_path) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };

    let meta = match std::fs::metadata(&resolved) {
        Ok(m) => m,
        Err(e) => return ToolResult::err(format!("stat failed: {e}")),
    };
    if !meta.is_file() {
        return ToolResult::err(format!("not a regular file: {}", resolved.display()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return ToolResult::err(format!(
            "file too large: {} bytes (max {})",
            meta.len(),
            MAX_FILE_BYTES
        ));
    }
    if FileSandbox::is_binary(&resolved) {
        return ToolResult::err("binary file rejected".to_string());
    }

    let content = match std::fs::read_to_string(&resolved) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(format!("read failed: {e}")),
    };

    let lines: Vec<&str> = content.split('\n').collect();
    // `split('\n')` on a trailing newline produces an extra empty element;
    // drop it so the line count matches the visible content.
    let total_lines = if content.ends_with('\n') && !lines.is_empty() {
        lines.len() - 1
    } else {
        lines.len()
    };

    if offset >= total_lines && total_lines > 0 {
        return ToolResult::err(format!(
            "offset {offset} past end of file ({total_lines} lines)"
        ));
    }

    let end = (offset + limit).min(total_lines);
    let mut out = String::with_capacity(content.len().min(64 * 1024));
    for (i, line) in lines[offset..end].iter().enumerate() {
        use std::fmt::Write as _;
        let _ = writeln!(out, "{}\t{}", offset + i + 1, line);
    }

    if end < total_lines {
        use std::fmt::Write as _;
        let _ = write!(
            out,
            "[... truncated: {total_lines} lines total, showing {}-{}. Use offset={end} to continue ...]",
            offset + 1,
            end
        );
    }

    ToolResult::ok(redact_secrets(&out))
}

/// `write_file`: atomic full overwrite (tmp+rename) inside the sandbox.
fn exec_write_file(state: &AppState, session_id: &str, args: &Value) -> ToolResult {
    exec_write_file_inner(state, session_id, args, false)
}

fn exec_write_file_inner(state: &AppState, session_id: &str, args: &Value, skip_safety: bool) -> ToolResult {
    let Some(file_path) = args["file_path"].as_str() else {
        return missing_arg("file_path");
    };
    let Some(content) = args["content"].as_str() else {
        return missing_arg("content");
    };

    if !skip_safety {
        let checker = RegexSafetyChecker::new();
        let verdict = checker.evaluate_file_write(file_path);
        match &verdict {
            SafetyVerdict::NeedsApproval { reason } => {
                return ToolResult::approval(reason, format!("write_file:{file_path}"));
            }
            SafetyVerdict::Block { .. } => {
                let msg = super::safety::format_rejection(&verdict).unwrap();
                return ToolResult::err(msg);
            }
            SafetyVerdict::Allow => {}
        }
    }

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };
    let resolved = match sandbox.resolve_for_write(file_path) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };

    let tmp = {
        let mut t = resolved.clone().into_os_string();
        t.push(".tmp.tuic");
        std::path::PathBuf::from(t)
    };

    let bytes = content.as_bytes();
    let write_result = (|| -> std::io::Result<()> {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.flush()?;
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return ToolResult::err(format!("write failed: {e}"));
    }
    if let Err(e) = std::fs::rename(&tmp, &resolved) {
        let _ = std::fs::remove_file(&tmp);
        return ToolResult::err(format!("rename failed: {e}"));
    }

    ToolResult::ok(
        json!({
            "written": true,
            "path": resolved.display().to_string(),
            "bytes": bytes.len(),
        })
        .to_string(),
    )
}

/// `edit_file`: string-exact search-and-replace with uniqueness enforcement.
fn exec_edit_file(state: &AppState, session_id: &str, args: &Value) -> ToolResult {
    exec_edit_file_inner(state, session_id, args, false)
}

fn exec_edit_file_inner(state: &AppState, session_id: &str, args: &Value, skip_safety: bool) -> ToolResult {
    let Some(file_path) = args["file_path"].as_str() else {
        return missing_arg("file_path");
    };
    let Some(old_string) = args["old_string"].as_str() else {
        return missing_arg("old_string");
    };
    let Some(new_string) = args["new_string"].as_str() else {
        return missing_arg("new_string");
    };
    let replace_all = args["replace_all"].as_bool().unwrap_or(false);

    if !skip_safety {
        let checker = RegexSafetyChecker::new();
        let verdict = checker.evaluate_file_write(file_path);
        match &verdict {
            SafetyVerdict::NeedsApproval { reason } => {
                return ToolResult::approval(reason, format!("edit_file:{file_path}"));
            }
            SafetyVerdict::Block { .. } => {
                let msg = super::safety::format_rejection(&verdict).unwrap();
                return ToolResult::err(msg);
            }
            SafetyVerdict::Allow => {}
        }
    }

    if old_string.is_empty() {
        return ToolResult::err("old_string must not be empty".to_string());
    }
    if old_string == new_string {
        return ToolResult::err("old_string and new_string are identical".to_string());
    }

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };
    let resolved = match sandbox.resolve(file_path) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };

    let meta = match std::fs::metadata(&resolved) {
        Ok(m) => m,
        Err(e) => return ToolResult::err(format!("stat failed: {e}")),
    };
    if !meta.is_file() {
        return ToolResult::err(format!("not a regular file: {}", resolved.display()));
    }
    if meta.len() > MAX_FILE_BYTES {
        return ToolResult::err(format!(
            "file too large: {} bytes (max {})",
            meta.len(),
            MAX_FILE_BYTES
        ));
    }

    let content = match std::fs::read_to_string(&resolved) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(format!("read failed: {e}")),
    };

    let occurrences = content.matches(old_string).count();
    if occurrences == 0 {
        return ToolResult::err(
            json!({
                "error": "old_string_not_found",
                "hint": "The literal string was not found — verify whitespace, line endings, and casing.",
            })
            .to_string(),
        );
    }
    if occurrences > 1 && !replace_all {
        return ToolResult::err(
            json!({
                "error": "old_string_not_unique",
                "occurrences": occurrences,
                "hint": "Include more surrounding lines in old_string, or set replace_all=true.",
            })
            .to_string(),
        );
    }

    let updated = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };

    let replacements = if replace_all { occurrences } else { 1 };

    // Reuse atomic write path.
    let write_args = json!({
        "file_path": file_path,
        "content": updated,
    });
    let write_res = exec_write_file(state, session_id, &write_args);
    if !write_res.success {
        return write_res;
    }

    ToolResult::ok(
        json!({
            "edited": true,
            "replacements": replacements,
            "path": resolved.display().to_string(),
        })
        .to_string(),
    )
}

/// Default timeout for `run_command` (2 min).
const RUN_COMMAND_DEFAULT_TIMEOUT_MS: u64 = 120_000;
/// Max timeout for `run_command` (10 min).
const RUN_COMMAND_MAX_TIMEOUT_MS: u64 = 600_000;
/// Output cap for `run_command`: head + tail window (30K chars).
const RUN_COMMAND_OUTPUT_CAP: usize = 30_000;

/// Max entries returned by `list_files` before truncating.
const LIST_FILES_MAX: usize = 500;
/// Max matches returned by `search_files` before truncating.
const SEARCH_MAX_MATCHES: usize = 50;
/// Upper bound for `context_lines` in `search_files`.
const SEARCH_MAX_CONTEXT: usize = 10;

/// Resolve an optional subdirectory path against the sandbox.
/// Empty / unset defaults to the sandbox root.
fn resolve_sandbox_subdir(sandbox: &FileSandbox, path: Option<&str>) -> Result<std::path::PathBuf, String> {
    match path {
        None | Some("") | Some(".") => Ok(sandbox.root().to_path_buf()),
        Some(p) => sandbox.resolve(p),
    }
}

/// `list_files`: glob-pattern listing inside the sandbox.
fn exec_list_files(state: &AppState, session_id: &str, args: &Value) -> ToolResult {
    let Some(pattern) = args["pattern"].as_str() else {
        return missing_arg("pattern");
    };
    if pattern.split('/').any(|c| c == "..") {
        return ToolResult::err("pattern must not contain `..` components".to_string());
    }
    let subdir = args["path"].as_str();

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };
    let anchor = match resolve_sandbox_subdir(&sandbox, subdir) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };

    // Anchor the glob on the resolved subdirectory; absolute patterns are
    // left as-is but still checked against the sandbox below.
    let full_pattern = if std::path::Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        anchor.join(pattern).to_string_lossy().into_owned()
    };

    let entries = match glob::glob(&full_pattern) {
        Ok(it) => it,
        Err(e) => return ToolResult::err(format!("invalid glob: {e}")),
    };

    let root = sandbox.root().to_path_buf();
    let mut out: Vec<Value> = Vec::new();
    let mut total: usize = 0;
    let mut truncated = false;

    for entry in entries {
        let path = match entry {
            Ok(p) => p,
            Err(_) => continue,
        };
        // Enforce sandbox: globs can escape with `..` inside the pattern.
        // Require canonicalize to succeed — without it, a literal `..`
        // component would make `starts_with(root)` pass despite escaping.
        let canon = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !canon.starts_with(&root) {
            continue;
        }
        total += 1;
        if out.len() >= LIST_FILES_MAX {
            truncated = true;
            continue;
        }
        let rel = canon.strip_prefix(&root).unwrap_or(&canon);
        let kind = if canon.is_dir() {
            "dir"
        } else if canon.is_file() {
            "file"
        } else {
            "other"
        };
        out.push(json!({
            "path": rel.to_string_lossy(),
            "type": kind,
        }));
    }

    ToolResult::ok(
        json!({
            "entries": out,
            "total": total,
            "truncated": truncated,
        })
        .to_string(),
    )
}

/// `search_files`: .gitignore-aware regex search with per-match context.
fn exec_search_files(state: &AppState, session_id: &str, args: &Value) -> ToolResult {
    let Some(pattern) = args["pattern"].as_str() else {
        return missing_arg("pattern");
    };
    let subdir = args["path"].as_str();
    let file_glob = args["glob"].as_str();
    let context_lines = args["context_lines"]
        .as_u64()
        .map(|v| v as usize)
        .unwrap_or(2)
        .min(SEARCH_MAX_CONTEXT);

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };
    let anchor = match resolve_sandbox_subdir(&sandbox, subdir) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };

    let re = match regex::Regex::new(pattern) {
        Ok(r) => r,
        Err(e) => return ToolResult::err(format!("invalid regex: {e}")),
    };

    let mut walk_builder = ignore::WalkBuilder::new(&anchor);
    walk_builder.hidden(false).git_ignore(true).git_global(true).git_exclude(true);
    if let Some(g) = file_glob {
        let mut overrides = ignore::overrides::OverrideBuilder::new(&anchor);
        if let Err(e) = overrides.add(g) {
            return ToolResult::err(format!("invalid glob filter: {e}"));
        }
        match overrides.build() {
            Ok(ov) => {
                walk_builder.overrides(ov);
            }
            Err(e) => return ToolResult::err(format!("invalid glob filter: {e}")),
        }
    }

    let root = sandbox.root().to_path_buf();
    let mut matches: Vec<Value> = Vec::new();
    let mut files_with_matches: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut total_matches: usize = 0;
    let mut truncated = false;

    'walk: for dent in walk_builder.build() {
        let dent = match dent {
            Ok(d) => d,
            Err(_) => continue,
        };
        if !dent.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }
        let path = dent.path();
        let canon = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !canon.starts_with(&root) {
            continue;
        }
        if FileSandbox::is_binary(&canon) {
            continue;
        }
        let meta = match std::fs::metadata(&canon) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() > MAX_FILE_BYTES {
            continue;
        }
        let content = match std::fs::read_to_string(&canon) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let lines: Vec<&str> = content.lines().collect();
        let rel = canon
            .strip_prefix(&root)
            .unwrap_or(&canon)
            .to_string_lossy()
            .into_owned();

        for (idx, line) in lines.iter().enumerate() {
            if !re.is_match(line) {
                continue;
            }
            total_matches += 1;
            files_with_matches.insert(rel.clone());
            if matches.len() >= SEARCH_MAX_MATCHES {
                truncated = true;
                continue;
            }
            let before_start = idx.saturating_sub(context_lines);
            let after_end = (idx + 1 + context_lines).min(lines.len());
            let context_before: Vec<String> = lines[before_start..idx]
                .iter()
                .map(|s| redact_secrets(s))
                .collect();
            let context_after: Vec<String> = lines[idx + 1..after_end]
                .iter()
                .map(|s| redact_secrets(s))
                .collect();
            matches.push(json!({
                "file": rel,
                "line": idx + 1,
                "content": redact_secrets(line),
                "context_before": context_before,
                "context_after": context_after,
            }));
            if truncated && total_matches > SEARCH_MAX_MATCHES * 4 {
                break 'walk;
            }
        }
    }

    ToolResult::ok(
        json!({
            "matches": matches,
            "total_matches": total_matches,
            "truncated": truncated,
            "files_with_matches": files_with_matches.into_iter().collect::<Vec<_>>(),
        })
        .to_string(),
    )
}

/// Truncate output to `RUN_COMMAND_OUTPUT_CAP` using head+tail windows.
fn truncate_output(s: &str) -> (String, bool) {
    if s.len() <= RUN_COMMAND_OUTPUT_CAP {
        return (s.to_string(), false);
    }
    let half = RUN_COMMAND_OUTPUT_CAP / 2;
    let head = &s[..half];
    let tail = &s[s.len() - half..];
    let truncated = format!(
        "{head}\n\n[... truncated: {} total chars, showing first {half} + last {half} ...]\n\n{tail}",
        s.len()
    );
    (truncated, true)
}

/// `run_command`: sandboxed shell command execution with captured output.
async fn exec_run_command(state: &AppState, session_id: &str, args: &Value) -> ToolResult {
    exec_run_command_inner(state, session_id, args, false).await
}

async fn exec_run_command_inner(state: &AppState, session_id: &str, args: &Value, skip_safety: bool) -> ToolResult {
    let Some(command) = args["command"].as_str() else {
        return missing_arg("command");
    };
    if command.trim().is_empty() {
        return ToolResult::err("command must not be empty");
    }

    let timeout_ms = args["timeout_ms"]
        .as_u64()
        .unwrap_or(RUN_COMMAND_DEFAULT_TIMEOUT_MS)
        .min(RUN_COMMAND_MAX_TIMEOUT_MS);
    let cwd_arg = args["cwd"].as_str();

    if !skip_safety {
        let checker = RegexSafetyChecker::new();
        let verdict = checker.evaluate(command);
        match &verdict {
            SafetyVerdict::Block { .. } => {
                let msg = super::safety::format_rejection(&verdict).unwrap();
                return ToolResult::err(msg);
            }
            SafetyVerdict::NeedsApproval { reason } => {
                return ToolResult::approval(reason, command);
            }
            SafetyVerdict::Allow => {}
        }
    }

    let sandbox = match get_sandbox(state, session_id) {
        Ok(s) => s,
        Err(e) => return ToolResult::err(e),
    };
    let cwd = match resolve_sandbox_subdir(&sandbox, cwd_arg) {
        Ok(p) => p,
        Err(e) => return ToolResult::err(e),
    };
    if !cwd.is_dir() {
        return ToolResult::err(format!("cwd is not a directory: {}", cwd.display()));
    }

    let home = sandbox.root().to_string_lossy().into_owned();
    let cwd_display = cwd.display().to_string();
    let lang = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
    let start = std::time::Instant::now();

    let mut child = match tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(&cwd)
        .env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", &home)
        .env("TERM", "xterm-256color")
        .env("LANG", &lang)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .process_group(0)
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return ToolResult::err(format!("spawn failed: {e}")),
    };

    // Take ownership of pipes, then spawn concurrent read tasks.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let Some(mut pipe) = stdout_pipe else { return Vec::new() };
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf).await;
        buf
    });
    let stderr_task = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let Some(mut pipe) = stderr_pipe else { return Vec::new() };
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf).await;
        buf
    });

    let timeout_dur = std::time::Duration::from_millis(timeout_ms);
    let wait_result = tokio::time::timeout(timeout_dur, child.wait()).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match wait_result {
        Ok(Ok(status)) => {
            let stdout_buf = stdout_task.await.unwrap_or_default();
            let stderr_buf = stderr_task.await.unwrap_or_default();
            let raw_stdout = String::from_utf8_lossy(&stdout_buf).into_owned();
            let raw_stderr = String::from_utf8_lossy(&stderr_buf).into_owned();
            let (stdout, stdout_truncated) = truncate_output(&redact_secrets(&raw_stdout));
            let (stderr, stderr_truncated) = truncate_output(&redact_secrets(&raw_stderr));
            let exit_code = status.code().unwrap_or(-1);

            ToolResult::ok(
                json!({
                    "exit_code": exit_code,
                    "stdout": stdout,
                    "stderr": stderr,
                    "truncated": stdout_truncated || stderr_truncated,
                    "duration_ms": duration_ms,
                    "cwd": cwd_display,
                })
                .to_string(),
            )
        }
        Ok(Err(e)) => ToolResult::err(format!("process error: {e}")),
        Err(_) => {
            // Timeout — kill the process group.
            #[cfg(unix)]
            if let Some(pid) = child.id() {
                unsafe { libc::killpg(pid as i32, libc::SIGKILL); }
            }
            #[cfg(not(unix))]
            let _ = child.kill().await;
            let _ = child.wait().await;

            ToolResult::err(
                json!({
                    "error": "timeout",
                    "timeout_ms": timeout_ms,
                    "duration_ms": duration_ms,
                })
                .to_string(),
            )
        }
    }
}

// ── Dispatch ──────────────────────────────────────────────────

/// Dispatch a tool call by function name.
///
/// `session_id` is the identity of the agent's PTY session — threaded through
/// so filesystem tools can look up their sandbox without the LLM needing to
/// pass it explicitly. Terminal tools continue to read `session_id` from
/// `args` (the LLM still supplies it) but the dispatch-level value is the
/// source of truth for cross-session isolation.
pub async fn dispatch(
    state: &Arc<AppState>,
    session_id: &str,
    fn_name: &str,
    args: &Value,
) -> ToolResult {
    dispatch_inner(state, session_id, fn_name, args, false).await
}

/// Re-dispatch a tool call after the user approved it — skips safety checks.
pub async fn dispatch_approved(
    state: &Arc<AppState>,
    session_id: &str,
    fn_name: &str,
    args: &Value,
) -> ToolResult {
    dispatch_inner(state, session_id, fn_name, args, true).await
}

async fn dispatch_inner(
    state: &Arc<AppState>,
    session_id: &str,
    fn_name: &str,
    args: &Value,
    skip_safety: bool,
) -> ToolResult {
    match fn_name {
        "read_screen" => exec_read_screen(state, args),
        "send_input" => exec_send_input_inner(state, args, skip_safety),
        "send_key" => exec_send_key_inner(state, args, skip_safety),
        "wait_for" => exec_wait_for(state, args).await,
        "get_state" => exec_get_state(state, args),
        "get_context" => exec_get_context(state, args),
        "read_file" => exec_read_file(state, session_id, args),
        "write_file" => exec_write_file_inner(state, session_id, args, skip_safety),
        "edit_file" => exec_edit_file_inner(state, session_id, args, skip_safety),
        "list_files" => exec_list_files(state, session_id, args),
        "search_files" => exec_search_files(state, session_id, args),
        "run_command" => exec_run_command_inner(state, session_id, args, skip_safety).await,
        other => ToolResult::err(format!("Unknown tool: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── tool_definitions ───────────────────────────────────────

    const TERMINAL_TOOLS: &[&str] = &[
        "read_screen",
        "send_input",
        "send_key",
        "wait_for",
        "get_state",
        "get_context",
    ];
    const FILESYSTEM_TOOLS: &[&str] = &["read_file", "write_file", "edit_file"];
    const FILESYSTEM_SEARCH_TOOLS: &[&str] = &["list_files", "search_files"];

    #[test]
    fn definitions_returns_12_tools() {
        let defs = tool_definitions();
        let arr = defs.as_array().unwrap();
        assert_eq!(arr.len(), 12);
    }

    #[test]
    fn each_tool_has_name_description_schema() {
        let defs = tool_definitions();
        for tool in defs.as_array().unwrap() {
            assert!(tool["name"].is_string(), "tool missing name");
            assert!(tool["description"].is_string(), "tool missing description");
            assert!(tool["inputSchema"].is_object(), "tool missing inputSchema");
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn tool_names_are_correct() {
        let defs = tool_definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "read_screen",
                "send_input",
                "send_key",
                "wait_for",
                "get_state",
                "get_context",
                "read_file",
                "write_file",
                "edit_file",
                "list_files",
                "search_files",
                "run_command",
            ]
        );
    }

    #[test]
    fn filesystem_search_tools_require_pattern() {
        let defs = tool_definitions();
        for tool in defs.as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            if !FILESYSTEM_SEARCH_TOOLS.contains(&name) {
                continue;
            }
            let required = tool["inputSchema"]["required"].as_array().unwrap();
            let has_pattern = required.iter().any(|v| v == "pattern");
            assert!(has_pattern, "search tool {name} must require pattern");
        }
    }

    #[test]
    fn terminal_tools_require_session_id() {
        let defs = tool_definitions();
        for tool in defs.as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            if !TERMINAL_TOOLS.contains(&name) {
                continue;
            }
            let required = tool["inputSchema"]["required"].as_array().unwrap();
            let has_session_id = required.iter().any(|v| v == "session_id");
            assert!(
                has_session_id,
                "terminal tool {name} must require session_id"
            );
        }
    }

    #[test]
    fn filesystem_tools_require_file_path() {
        let defs = tool_definitions();
        for tool in defs.as_array().unwrap() {
            let name = tool["name"].as_str().unwrap();
            if !FILESYSTEM_TOOLS.contains(&name) {
                continue;
            }
            let required = tool["inputSchema"]["required"].as_array().unwrap();
            let has_file_path = required.iter().any(|v| v == "file_path");
            assert!(has_file_path, "fs tool {name} must require file_path");
        }
    }

    // ── redact_secrets ─────────────────────────────────────────

    #[test]
    fn redact_sk_key() {
        let input = "export OPENAI_API_KEY=sk-abc123def456ghi789jkl012mno345";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("sk-abc"));
    }

    #[test]
    fn redact_aws_key() {
        let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
        assert!(!output.contains("AKIA"));
    }

    #[test]
    fn redact_github_token() {
        let input = "gh auth token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redact_github_oauth() {
        let input = "token=gho_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmn";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redact_slack_token() {
        let input = "SLACK_BOT_TOKEN=xoxb-123-456-abc";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redact_bearer_token() {
        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiJ9.payload.sig";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redact_pem_key() {
        let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBA...";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redact_database_url() {
        let input = "DATABASE_URL=postgres://user:pass@host:5432/db";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn redact_google_oauth() {
        let input = "token: ya29.a0AfH6SMBx_long_token_here";
        let output = redact_secrets(input);
        assert!(output.contains("[REDACTED]"));
    }

    #[test]
    fn no_redaction_on_safe_text() {
        let input = "$ cargo test\nrunning 32 tests\ntest result: ok";
        assert_eq!(redact_secrets(input), input);
    }

    // ── map_key ────────────────────────────────────────────────

    #[test]
    fn key_ctrl_c_maps() {
        let (seq, sk) = map_key("ctrl-c").unwrap();
        assert_eq!(seq, "\x03");
        assert_eq!(sk, Some(SafeKey::CtrlC));
    }

    #[test]
    fn key_ctrl_d_maps() {
        let (seq, sk) = map_key("ctrl-d").unwrap();
        assert_eq!(seq, "\x04");
        assert_eq!(sk, Some(SafeKey::CtrlD));
    }

    #[test]
    fn key_escape_maps() {
        let (seq, sk) = map_key("escape").unwrap();
        assert_eq!(seq, "\x1b");
        assert_eq!(sk, Some(SafeKey::Escape));
    }

    #[test]
    fn key_enter_maps() {
        let (seq, sk) = map_key("enter").unwrap();
        assert_eq!(seq, "\r");
        assert_eq!(sk, None);
    }

    #[test]
    fn key_arrow_up_maps() {
        let (seq, sk) = map_key("up").unwrap();
        assert_eq!(seq, "\x1b[A");
        assert_eq!(sk, None);
    }

    #[test]
    fn key_case_insensitive() {
        let (seq, _) = map_key("Ctrl-C").unwrap();
        assert_eq!(seq, "\x03");
    }

    #[test]
    fn key_plus_syntax() {
        let (seq, _) = map_key("ctrl+z").unwrap();
        assert_eq!(seq, "\x1a");
    }

    #[test]
    fn key_unknown_errors() {
        assert!(map_key("f13").is_err());
    }

    // ── dispatch routing ───────────────────────────────────────

    #[tokio::test]
    async fn dispatch_unknown_tool() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "nonexistent", &json!({})).await;
        assert!(!result.success);
        assert!(result.output.contains("Unknown tool"));
    }

    #[tokio::test]
    async fn dispatch_read_screen_missing_session() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "read_screen", &json!({"session_id": "nope"})).await;
        assert!(!result.success);
        assert!(result.output.contains("No VT buffer"));
    }

    #[tokio::test]
    async fn dispatch_send_input_blocks_sudo() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "send_input", &json!({
            "session_id": "test",
            "command": "sudo rm -rf /"
        })).await;
        assert!(!result.success);
        assert!(result.output.contains("blocked"));
    }

    #[tokio::test]
    async fn dispatch_send_input_needs_approval_for_rm_rf() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "send_input", &json!({
            "session_id": "test",
            "command": "rm -rf /tmp/build"
        })).await;
        assert!(!result.success);
        assert!(result.needs_approval);
        assert!(result.approval_reason.is_some());
        assert_eq!(result.approval_command.as_deref(), Some("rm -rf /tmp/build"));
    }

    #[tokio::test]
    async fn dispatch_send_key_needs_approval_for_ctrl_d() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "send_key", &json!({
            "session_id": "test",
            "key": "ctrl-d"
        })).await;
        assert!(!result.success);
        assert!(result.needs_approval);
        assert!(result.approval_reason.unwrap().contains("high-risk"));
    }

    #[tokio::test]
    async fn dispatch_approved_bypasses_safety() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        // Normal dispatch returns needs_approval
        let result = dispatch(&state, "test", "send_input", &json!({
            "session_id": "test",
            "command": "rm -rf /tmp/build"
        })).await;
        assert!(result.needs_approval);
        // Approved dispatch skips safety (will fail on missing session, not safety)
        let result = dispatch_approved(&state, "test", "send_input", &json!({
            "session_id": "test",
            "command": "rm -rf /tmp/build"
        })).await;
        assert!(!result.needs_approval);
        assert!(!result.success); // fails because session doesn't exist, not safety
        assert!(result.output.contains("Session not found"));
    }

    #[test]
    fn tool_result_approval_constructor() {
        let r = ToolResult::approval("destructive command", "rm -rf /");
        assert!(!r.success);
        assert!(r.needs_approval);
        assert_eq!(r.approval_reason.as_deref(), Some("destructive command"));
        assert_eq!(r.approval_command.as_deref(), Some("rm -rf /"));
        assert!(r.output.contains("Needs approval"));
    }

    #[test]
    fn tool_result_ok_not_approval() {
        let r = ToolResult::ok("done");
        assert!(r.success);
        assert!(!r.needs_approval);
        assert!(r.approval_reason.is_none());
    }

    #[test]
    fn tool_result_err_not_approval() {
        let r = ToolResult::err("failed");
        assert!(!r.success);
        assert!(!r.needs_approval);
    }

    #[tokio::test]
    async fn dispatch_get_state_missing_session() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "get_state", &json!({"session_id": "nope"})).await;
        assert!(!result.success);
    }

    #[tokio::test]
    async fn dispatch_get_context_missing_session() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "get_context", &json!({"session_id": "nope"})).await;
        assert!(result.success); // Returns defaults for missing session
        assert!(result.output.contains("shell_state"));
    }

    #[tokio::test]
    async fn dispatch_wait_for_invalid_regex() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "wait_for", &json!({
            "session_id": "test",
            "pattern": "[invalid"
        })).await;
        assert!(!result.success);
        assert!(result.output.contains("Invalid regex"));
    }

    // ── Filesystem tools ───────────────────────────────────────

    use tempfile::TempDir;

    fn fs_test_state(session: &str) -> (TempDir, Arc<AppState>) {
        let dir = TempDir::new().unwrap();
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let sb = FileSandbox::new(dir.path()).unwrap();
        state.file_sandboxes.insert(session.to_string(), sb);
        (dir, state)
    }

    #[tokio::test]
    async fn read_file_requires_sandbox() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(&state, "nosession", "read_file", &json!({ "file_path": "x.txt" })).await;
        assert!(!r.success);
        assert!(r.output.contains("No filesystem sandbox"));
    }

    #[tokio::test]
    async fn read_file_missing_file_path_arg() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "read_file", &json!({})).await;
        assert!(!r.success);
        assert!(r.output.contains("file_path"));
    }

    #[tokio::test]
    async fn read_file_returns_numbered_lines() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "alpha\nbeta\ngamma\n").unwrap();
        let r = dispatch(&state, "s1", "read_file", &json!({ "file_path": "a.txt" })).await;
        assert!(r.success, "{}", r.output);
        assert!(r.output.contains("1\talpha"));
        assert!(r.output.contains("2\tbeta"));
        assert!(r.output.contains("3\tgamma"));
        assert!(!r.output.contains("truncated"));
    }

    #[tokio::test]
    async fn read_file_respects_offset_and_limit() {
        let (dir, state) = fs_test_state("s1");
        let body: String = (1..=10).map(|i| format!("line{i}\n")).collect();
        std::fs::write(dir.path().join("a.txt"), body).unwrap();
        let r = dispatch(
            &state,
            "s1",
            "read_file",
            &json!({ "file_path": "a.txt", "offset": 3, "limit": 2 }),
        )
        .await;
        assert!(r.success);
        assert!(r.output.contains("4\tline4"));
        assert!(r.output.contains("5\tline5"));
        assert!(!r.output.contains("line3"));
        assert!(r.output.contains("truncated"));
        assert!(r.output.contains("offset=5"));
    }

    #[tokio::test]
    async fn read_file_caps_limit_at_2000() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "x\n").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "read_file",
            &json!({ "file_path": "a.txt", "limit": 10_000 }),
        )
        .await;
        // Should succeed without error — cap is silently applied.
        assert!(r.success);
    }

    #[tokio::test]
    async fn read_file_rejects_binary() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("b.bin"), [0xff, 0xfe, 0x00, 0xc3, 0x28]).unwrap();
        let r = dispatch(&state, "s1", "read_file", &json!({ "file_path": "b.bin" })).await;
        assert!(!r.success);
        assert!(r.output.contains("binary"));
    }

    #[tokio::test]
    async fn read_file_rejects_oversized() {
        let (dir, state) = fs_test_state("s1");
        // Create a sparse file larger than MAX_FILE_BYTES without writing 10MB.
        let path = dir.path().join("big.txt");
        let f = std::fs::File::create(&path).unwrap();
        f.set_len(MAX_FILE_BYTES + 1).unwrap();
        let r = dispatch(&state, "s1", "read_file", &json!({ "file_path": "big.txt" })).await;
        assert!(!r.success);
        assert!(r.output.contains("too large"));
    }

    #[tokio::test]
    async fn read_file_rejects_path_outside_sandbox() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "read_file",
            &json!({ "file_path": "../../etc/passwd" }),
        )
        .await;
        assert!(!r.success);
    }

    #[tokio::test]
    async fn read_file_redacts_secrets() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(
            dir.path().join("env"),
            "OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz0123\n",
        )
        .unwrap();
        let r = dispatch(&state, "s1", "read_file", &json!({ "file_path": "env" })).await;
        assert!(r.success);
        assert!(r.output.contains("[REDACTED]"));
        assert!(!r.output.contains("sk-abcdef"));
    }

    #[tokio::test]
    async fn write_file_creates_new_file() {
        let (dir, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "write_file",
            &json!({ "file_path": "new.txt", "content": "hello" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let written = std::fs::read_to_string(dir.path().join("new.txt")).unwrap();
        assert_eq!(written, "hello");
        assert!(r.output.contains("\"written\":true"));
        assert!(r.output.contains("\"bytes\":5"));
    }

    #[tokio::test]
    async fn write_file_overwrites_existing() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "old").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "write_file",
            &json!({ "file_path": "a.txt", "content": "new" }),
        )
        .await;
        assert!(r.success);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "new"
        );
    }

    #[tokio::test]
    async fn write_file_creates_nested_dirs() {
        let (dir, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "write_file",
            &json!({ "file_path": "a/b/c.txt", "content": "x" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        assert!(dir.path().join("a/b/c.txt").exists());
    }

    #[tokio::test]
    async fn write_file_is_atomic_no_leftover_tmp() {
        let (dir, state) = fs_test_state("s1");
        let _ = dispatch(
            &state,
            "s1",
            "write_file",
            &json!({ "file_path": "a.txt", "content": "x" }),
        )
        .await;
        // Make sure no .tmp.tuic leftover.
        for entry in std::fs::read_dir(dir.path()).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            assert!(!name.ends_with(".tmp.tuic"), "leftover tmp: {name}");
        }
    }

    #[tokio::test]
    async fn write_file_rejects_path_outside_sandbox() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "write_file",
            &json!({ "file_path": "../escape.txt", "content": "x" }),
        )
        .await;
        assert!(!r.success);
    }

    #[tokio::test]
    async fn edit_file_replaces_unique_occurrence() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "foo bar baz").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "edit_file",
            &json!({ "file_path": "a.txt", "old_string": "bar", "new_string": "BAR" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "foo BAR baz"
        );
        assert!(r.output.contains("\"edited\":true"));
        assert!(r.output.contains("\"replacements\":1"));
    }

    #[tokio::test]
    async fn edit_file_rejects_non_unique_without_replace_all() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "x x x").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "edit_file",
            &json!({ "file_path": "a.txt", "old_string": "x", "new_string": "y" }),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("old_string_not_unique"));
        assert!(r.output.contains("\"occurrences\":3"));
        // Verify file is unchanged.
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "x x x");
    }

    #[tokio::test]
    async fn edit_file_replace_all_replaces_all() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "x x x").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "edit_file",
            &json!({
                "file_path": "a.txt",
                "old_string": "x",
                "new_string": "y",
                "replace_all": true,
            }),
        )
        .await;
        assert!(r.success);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "y y y"
        );
        assert!(r.output.contains("\"replacements\":3"));
    }

    #[tokio::test]
    async fn edit_file_rejects_missing_old_string() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "edit_file",
            &json!({ "file_path": "a.txt", "old_string": "world", "new_string": "earth" }),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("old_string_not_found"));
    }

    #[tokio::test]
    async fn edit_file_rejects_empty_old_string() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "edit_file",
            &json!({ "file_path": "a.txt", "old_string": "", "new_string": "x" }),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("must not be empty"));
    }

    #[tokio::test]
    async fn edit_file_rejects_identical_strings() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.txt"), "hello").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "edit_file",
            &json!({ "file_path": "a.txt", "old_string": "hello", "new_string": "hello" }),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("identical"));
    }

    #[tokio::test]
    async fn edit_file_rejects_path_outside_sandbox() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "edit_file",
            &json!({ "file_path": "../a.txt", "old_string": "x", "new_string": "y" }),
        )
        .await;
        assert!(!r.success);
    }

    #[tokio::test]
    async fn dispatch_routes_fs_tools() {
        // Unknown file_path -> the tool routes correctly and surfaces a
        // sandbox/resolve error, not an "unknown tool" error.
        let (_d, state) = fs_test_state("s1");
        for name in &["read_file", "write_file", "edit_file"] {
            let args = match *name {
                "write_file" => json!({ "file_path": "nope", "content": "x" }),
                "edit_file" => {
                    json!({ "file_path": "nope", "old_string": "a", "new_string": "b" })
                }
                _ => json!({ "file_path": "nope" }),
            };
            let r = dispatch(&state, "s1", name, &args).await;
            assert!(!r.output.contains("Unknown tool"), "tool {name} did not route");
        }
    }

    // ── list_files ─────────────────────────────────────────────

    #[tokio::test]
    async fn list_files_requires_sandbox() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(&state, "none", "list_files", &json!({ "pattern": "*" })).await;
        assert!(!r.success);
        assert!(r.output.contains("No filesystem sandbox"));
    }

    #[tokio::test]
    async fn list_files_missing_pattern() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "list_files", &json!({})).await;
        assert!(!r.success);
        assert!(r.output.contains("pattern"));
    }

    #[tokio::test]
    async fn list_files_matches_glob() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();
        let r = dispatch(&state, "s1", "list_files", &json!({ "pattern": "*.rs" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let entries = parsed["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(parsed["total"], 2);
        assert_eq!(parsed["truncated"], false);
        let paths: Vec<&str> = entries.iter().map(|e| e["path"].as_str().unwrap()).collect();
        assert!(paths.iter().any(|p| p.ends_with("a.rs")));
        assert!(paths.iter().any(|p| p.ends_with("b.rs")));
        assert!(!paths.iter().any(|p| p.ends_with("c.txt")));
    }

    #[tokio::test]
    async fn list_files_recursive_glob() {
        let (dir, state) = fs_test_state("s1");
        std::fs::create_dir_all(dir.path().join("sub/nested")).unwrap();
        std::fs::write(dir.path().join("sub/a.rs"), "").unwrap();
        std::fs::write(dir.path().join("sub/nested/b.rs"), "").unwrap();
        let r = dispatch(&state, "s1", "list_files", &json!({ "pattern": "**/*.rs" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(parsed["total"], 2);
    }

    #[tokio::test]
    async fn list_files_reports_dir_vs_file() {
        let (dir, state) = fs_test_state("s1");
        std::fs::create_dir_all(dir.path().join("a_dir")).unwrap();
        std::fs::write(dir.path().join("a_file"), "").unwrap();
        let r = dispatch(&state, "s1", "list_files", &json!({ "pattern": "a_*" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let entries = parsed["entries"].as_array().unwrap();
        assert!(entries.iter().any(|e| e["type"] == "dir"));
        assert!(entries.iter().any(|e| e["type"] == "file"));
    }

    #[tokio::test]
    async fn list_files_rejects_invalid_glob() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "list_files", &json!({ "pattern": "[unterminated" })).await;
        assert!(!r.success);
        assert!(r.output.contains("invalid glob"));
    }

    #[tokio::test]
    async fn list_files_rejects_dotdot_pattern() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "list_files", &json!({ "pattern": "../*" })).await;
        assert!(!r.success);
        assert!(r.output.contains(".."));
    }

    // ── search_files ───────────────────────────────────────────

    #[tokio::test]
    async fn search_files_requires_sandbox() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(&state, "none", "search_files", &json!({ "pattern": "x" })).await;
        assert!(!r.success);
        assert!(r.output.contains("No filesystem sandbox"));
    }

    #[tokio::test]
    async fn search_files_missing_pattern() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "search_files", &json!({})).await;
        assert!(!r.success);
        assert!(r.output.contains("pattern"));
    }

    #[tokio::test]
    async fn search_files_finds_matches_with_context() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(
            dir.path().join("a.rs"),
            "line1\nline2\nneedle here\nline4\nline5\n",
        )
        .unwrap();
        let r = dispatch(
            &state,
            "s1",
            "search_files",
            &json!({ "pattern": "needle", "context_lines": 1 }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let matches = parsed["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["line"], 3);
        assert_eq!(matches[0]["context_before"].as_array().unwrap().len(), 1);
        assert_eq!(matches[0]["context_after"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["total_matches"], 1);
    }

    #[tokio::test]
    async fn search_files_rejects_invalid_regex() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "search_files",
            &json!({ "pattern": "(unclosed" }),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("invalid regex"));
    }

    #[tokio::test]
    async fn search_files_truncates_at_50() {
        let (dir, state) = fs_test_state("s1");
        let body: String = (0..60).map(|_| "needle\n").collect();
        std::fs::write(dir.path().join("a.txt"), body).unwrap();
        let r = dispatch(&state, "s1", "search_files", &json!({ "pattern": "needle" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(parsed["matches"].as_array().unwrap().len(), 50);
        assert_eq!(parsed["total_matches"], 60);
        assert_eq!(parsed["truncated"], true);
    }

    #[tokio::test]
    async fn search_files_respects_gitignore() {
        let (dir, state) = fs_test_state("s1");
        // Need a git dir for the ignore crate to honor .gitignore fully;
        // WalkBuilder respects .gitignore when present in an ancestor.
        std::fs::write(dir.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(dir.path().join("ignored.txt"), "needle\n").unwrap();
        std::fs::write(dir.path().join("visible.txt"), "needle\n").unwrap();
        // Init a git repo so ignore respects .gitignore.
        std::fs::create_dir_all(dir.path().join(".git")).unwrap();
        let r = dispatch(&state, "s1", "search_files", &json!({ "pattern": "needle" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let files = parsed["files_with_matches"].as_array().unwrap();
        assert!(files.iter().any(|f| f.as_str().unwrap().ends_with("visible.txt")));
        assert!(!files.iter().any(|f| f.as_str().unwrap().ends_with("ignored.txt")));
    }

    #[tokio::test]
    async fn search_files_glob_filter() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("a.rs"), "needle\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "needle\n").unwrap();
        let r = dispatch(
            &state,
            "s1",
            "search_files",
            &json!({ "pattern": "needle", "glob": "*.rs" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let files = parsed["files_with_matches"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].as_str().unwrap().ends_with("a.rs"));
    }

    #[tokio::test]
    async fn search_files_skips_binary() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(dir.path().join("bin"), [0xff, 0xfe, 0xff, 0xfe]).unwrap();
        std::fs::write(dir.path().join("a.txt"), "needle\n").unwrap();
        let r = dispatch(&state, "s1", "search_files", &json!({ "pattern": "needle" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(parsed["total_matches"], 1);
    }

    #[tokio::test]
    async fn search_files_redacts_secrets() {
        let (dir, state) = fs_test_state("s1");
        std::fs::write(
            dir.path().join("a.txt"),
            "api=sk-abc123def456ghi789jkl012mno345\n",
        )
        .unwrap();
        let r = dispatch(&state, "s1", "search_files", &json!({ "pattern": "api" })).await;
        assert!(r.success, "{}", r.output);
        assert!(r.output.contains("[REDACTED]"));
        assert!(!r.output.contains("sk-abc"));
    }

    // ── run_command ────────────────────────────────────────────

    #[tokio::test]
    async fn run_command_requires_sandbox() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let r = dispatch(&state, "none", "run_command", &json!({ "command": "echo hi" })).await;
        assert!(!r.success);
        assert!(r.output.contains("No filesystem sandbox"));
    }

    #[tokio::test]
    async fn run_command_missing_command() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "run_command", &json!({})).await;
        assert!(!r.success);
        assert!(r.output.contains("command"));
    }

    #[tokio::test]
    async fn run_command_rejects_empty_command() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "run_command", &json!({ "command": "  " })).await;
        assert!(!r.success);
        assert!(r.output.contains("empty"));
    }

    #[tokio::test]
    async fn run_command_captures_stdout() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "run_command", &json!({ "command": "echo hello_world" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(parsed["exit_code"], 0);
        assert!(parsed["stdout"].as_str().unwrap().contains("hello_world"));
        assert_eq!(parsed["truncated"], false);
    }

    #[tokio::test]
    async fn run_command_captures_stderr() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "echo err_msg >&2" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert!(parsed["stderr"].as_str().unwrap().contains("err_msg"));
    }

    #[tokio::test]
    async fn run_command_returns_exit_code() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "run_command", &json!({ "command": "exit 42" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(parsed["exit_code"], 42);
    }

    #[tokio::test]
    async fn run_command_uses_sandbox_root_as_cwd() {
        let (dir, state) = fs_test_state("s1");
        let r = dispatch(&state, "s1", "run_command", &json!({ "command": "pwd" })).await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let stdout = parsed["stdout"].as_str().unwrap().trim();
        let expected = dir.path().canonicalize().unwrap();
        assert_eq!(stdout, expected.to_str().unwrap());
    }

    #[tokio::test]
    async fn run_command_respects_cwd() {
        let (dir, state) = fs_test_state("s1");
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "pwd", "cwd": "sub" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert!(parsed["stdout"].as_str().unwrap().contains("sub"));
    }

    #[tokio::test]
    async fn run_command_blocks_destructive() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "sudo rm -rf /" }),
        )
        .await;
        assert!(!r.success);
    }

    #[tokio::test]
    async fn run_command_timeout_kills_process() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "sleep 60", "timeout_ms": 500 }),
        )
        .await;
        assert!(!r.success);
        assert!(r.output.contains("timeout"));
    }

    #[tokio::test]
    async fn run_command_sanitized_env() {
        let (_d, state) = fs_test_state("s1");
        // `env` alone is blocked (data exfil); use `printenv HOME` (specific var)
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "printenv HOME" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        let stdout = parsed["stdout"].as_str().unwrap();
        assert!(stdout.contains('/'));
        // Verify bare `env` is blocked
        let r2 = dispatch(&state, "s1", "run_command", &json!({ "command": "env" })).await;
        assert!(!r2.success);
        assert!(r2.output.contains("blocked"));
    }

    #[tokio::test]
    async fn run_command_redacts_secrets() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "echo sk-abc123def456ghi789jkl012mno345" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        assert!(r.output.contains("[REDACTED]"));
        assert!(!r.output.contains("sk-abc"));
    }

    #[tokio::test]
    async fn run_command_truncates_large_output() {
        let (_d, state) = fs_test_state("s1");
        let r = dispatch(
            &state,
            "s1",
            "run_command",
            &json!({ "command": "yes aaaa | head -20000" }),
        )
        .await;
        assert!(r.success, "{}", r.output);
        let parsed: Value = serde_json::from_str(&r.output).unwrap();
        assert_eq!(parsed["truncated"], true);
        assert!(parsed["stdout"].as_str().unwrap().contains("truncated"));
    }

    #[test]
    fn truncate_output_short_passes_through() {
        let (out, trunc) = truncate_output("short");
        assert_eq!(out, "short");
        assert!(!trunc);
    }

    #[test]
    fn truncate_output_long_head_tail() {
        let long: String = "x".repeat(40_000);
        let (out, trunc) = truncate_output(&long);
        assert!(trunc);
        assert!(out.contains("truncated"));
        assert!(out.len() < long.len());
    }
}
