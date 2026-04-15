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
}

impl ToolResult {
    pub fn ok(output: impl Into<String>) -> Self {
        Self { success: true, output: output.into() }
    }

    pub fn err(output: impl Into<String>) -> Self {
        Self { success: false, output: output.into() }
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
    let session_id = match args["session_id"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing session_id"),
    };
    let command = match args["command"].as_str() {
        Some(s) => s,
        None => return ToolResult::err("Missing command"),
    };

    // Safety check
    let checker = RegexSafetyChecker::new();
    match checker.evaluate(command) {
        SafetyVerdict::Allow => {}
        verdict => {
            let rejection = super::safety::format_rejection(&verdict).unwrap_or_default();
            return ToolResult::err(rejection);
        }
    }

    match safe_pty_write(state, session_id, command) {
        Ok(()) => ToolResult::ok(format!("Sent: {command}")),
        Err(e) => ToolResult::err(e),
    }
}

/// Execute `send_key`: send a special key with safety check.
fn exec_send_key(state: &AppState, args: &Value) -> ToolResult {
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

    // Safety check for special keys
    if let Some(sk) = safe_key {
        if sk.risk() == KeyRisk::High {
            return ToolResult::err(json!({
                "status": "needs_approval",
                "reason": format!("{key_name} is high-risk (may terminate shell)"),
                "action": "Ask the user for confirmation before sending this key."
            }).to_string());
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
    let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(10_000);
    let stability_ms = args["stability_ms"].as_u64().unwrap_or(500);

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
    let Some(file_path) = args["file_path"].as_str() else {
        return missing_arg("file_path");
    };
    let Some(content) = args["content"].as_str() else {
        return missing_arg("content");
    };

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
    match fn_name {
        "read_screen" => exec_read_screen(state, args),
        "send_input" => exec_send_input(state, args),
        "send_key" => exec_send_key(state, args),
        "wait_for" => exec_wait_for(state, args).await,
        "get_state" => exec_get_state(state, args),
        "get_context" => exec_get_context(state, args),
        "read_file" => exec_read_file(state, session_id, args),
        "write_file" => exec_write_file(state, session_id, args),
        "edit_file" => exec_edit_file(state, session_id, args),
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

    #[test]
    fn definitions_returns_9_tools() {
        let defs = tool_definitions();
        let arr = defs.as_array().unwrap();
        assert_eq!(arr.len(), 9);
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
            ]
        );
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
        assert!(result.output.contains("needs_approval"));
    }

    #[tokio::test]
    async fn dispatch_send_key_blocks_ctrl_d() {
        let state = Arc::new(crate::state::tests_support::make_test_app_state());
        let result = dispatch(&state, "test", "send_key", &json!({
            "session_id": "test",
            "key": "ctrl-d"
        })).await;
        assert!(!result.success);
        assert!(result.output.contains("needs_approval"));
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
}
